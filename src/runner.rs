// This file is part of paperd, the PaperMC server daemon
// Copyright (C) 2019 Kyle Wood (DemonWav)
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Lesser General Public License as published by
// the Free Software Foundation, version 3 only.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::daemon::{run_daemon, Status};
use crate::log::{find_log_file, tail};
use crate::util::{find_prog, ExitError};
use clap::ArgMatches;
use nix::errno::Errno::ESRCH;
use nix::sys::signal;
use nix::sys::signal::kill;
use nix::unistd::Pid;
use nix::Error;
use regex::Regex;
use signal_hook::iterator::Signals;
use signal_hook::{SIGABRT, SIGHUP, SIGINT, SIGQUIT, SIGTERM, SIGTRAP};
use std::cmp::{max, min};
use std::fs::canonicalize;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use std::{fs, thread};
use sys_info::mem_info;

static JNI_LIB: &'static [u8] = include_bytes!(env!("PAPERD_JNI_LIB"));

pub const PID_FILE_NAME: &str = "paper.pid";
const RESTART_EXIT_CODE: i32 = 27;

pub fn start(sub_m: &ArgMatches) -> Result<(), i32> {
    let env = setup_java_env(sub_m)?;

    let mut lib_file = std::env::temp_dir();
    lib_file.push("libpaperd_jni.so.gz");
    // Find a file name that is available..
    let mut count = 1;
    while lib_file.exists() {
        lib_file.pop();
        lib_file.push(format!("libpaperd_jni.so.gz.{}", count));
        count += 1;
    }

    if let Err(e) = fs::write(&lib_file, JNI_LIB) {
        eprintln!("Failed to write JNI library to temp directory: {}", e);
        return Err(1);
    }

    match run_daemon() {
        Ok(Status::QUIT(pid)) => {
            println!("Server starting in background, waiting for server to start...");

            let pid_file = env.working_dir.join(PID_FILE_NAME);
            let dur = Duration::from_secs(5);
            let start = Instant::now();

            // wait for pid file to be created, until timeout
            while Instant::now().duration_since(start) < dur && !pid_file.exists() {
                thread::yield_now();
            }

            if pid_file.exists() {
                println!("Server started in the background. PID: {}", pid);
                if sub_m.is_present("TAIL") {
                    let log_file = find_log_file(&pid_file)?;
                    return tail(log_file, 0, true);
                } else {
                    return Ok(());
                }
            } else {
                eprintln!("Timeout while waiting for server to start.");
                return Err(1);
            }
        }
        Ok(Status::CONTINUE) => {}
        Err(err) => return Err(err),
    }

    let mut env = env;
    env.args
        .push("-Dio.papermc.daemon.enabled=true".to_string());
    env.args.push(format!(
        "-Dio.papermc.daemon.paperd.binary={}",
        lib_file.to_string_lossy()
    ));

    let mut result: i32;
    loop {
        let child = start_process(&env)?;

        let pid = child.id();

        // Write pid file
        let pid_file = env.working_dir.join(PID_FILE_NAME);
        let pid_file = pid_file.as_path();
        if let Err(_) = fs::write(pid_file, pid.to_string()) {
            result = 1;
            break;
        }

        let signals = forward_signals(pid)?;

        result = wait_for_child(child);

        signals.close();

        let _ = fs::remove_file(pid_file);

        if result != RESTART_EXIT_CODE {
            break;
        }
    }

    // Attempt to cleanup a little
    if lib_file.exists() {
        if let Ok(data) = fs::read_to_string(&lib_file) {
            let path = PathBuf::from(data);
            if path.exists() {
                let _ = fs::remove_file(&path);
            }
        }
        let _ = fs::remove_file(&lib_file);
    }

    return Err(result);
}

pub fn run_cmd(sub_m: &ArgMatches) -> Result<(), i32> {
    let env = setup_java_env(sub_m)?;
    let child = start_process(&env)?;

    let pid = child.id();

    let signals = forward_signals(pid)?;

    let result = wait_for_child(child);

    signals.close();

    return Err(result);
}

struct JavaEnv {
    java_file: PathBuf,
    jar_file: PathBuf,
    working_dir: PathBuf,
    args: Vec<String>,
}

fn start_process(env: &JavaEnv) -> Result<Child, i32> {
    let result = Command::new(&env.java_file)
        .args(&env.args)
        .arg("-jar")
        .arg(&env.jar_file)
        .current_dir(&env.working_dir)
        .spawn();

    return match result {
        Ok(c) => Ok(c),
        Err(err) => {
            eprintln!("Failed to start server: {}", err);
            Err(1)
        }
    };
}

fn setup_java_env(sub_m: &ArgMatches) -> Result<JavaEnv, i32> {
    // Find Java executable
    let java_path = sub_m.value_of("JVM").map(PathBuf::from).or_else(find_java);
    let java_path = match java_path {
        Some(path) => path,
        None => {
            eprintln!(
                "Could not find a JVM executable. Either make sure it's present on the PATH, or \
                 there's a valid JAVA_HOME, or specify it with -j. See --help for more details."
            );
            return Err(1);
        }
    };

    // Find target jar file
    let jar_path = match sub_m.value_of("JAR") {
        Some(path) => match canonicalize(PathBuf::from(path)) {
            Ok(canonical) => canonical,
            _ => {
                eprintln!("Failed to get full path to jar {}", path);
                return Err(1);
            }
        },
        None => {
            eprintln!("Failed to resolve jar file path");
            return Err(1);
        }
    };
    if !jar_path.is_file() {
        eprintln!("Could not find jar {}", jar_path.to_string_lossy());
        return Err(1);
    }

    // Get the jar's parent directory
    let parent_path = sub_m
        .value_of("CWD")
        .map(|s| PathBuf::from(s))
        .or_else(|| jar_path.parent().map(|p| p.to_path_buf()));
    let parent_path = match parent_path {
        Some(path) => path,
        None => {
            eprintln!(
                "Failed to find parent directory for jar {}",
                jar_path.to_string_lossy()
            );
            return Err(1);
        }
    };

    let pid_file = parent_path.join(PID_FILE_NAME);
    if pid_file.is_file() {
        let pid = fs::read_to_string(&pid_file).conv()?;
        let pid = Pid::from_raw(pid.parse::<i32>().conv()?);

        match kill(pid, None) {
            Ok(()) => {
                eprintln!(
                    "Found server already running in this directory with PID {}, will not continue",
                    pid
                );
                return Err(1);
            }
            Err(Error::Sys(e)) => {
                if e == ESRCH {
                    println!("Found stale PID file, removing");
                    fs::remove_file(&pid_file).conv()?;
                } else {
                    println!("Unknown error occurred (start): {}", e);
                    return Err(1);
                }
            }
            _ => {}
        }
    }

    let jvm_args = get_jvm_args(sub_m)?;

    return Ok(JavaEnv {
        java_file: java_path,
        jar_file: jar_path,
        working_dir: parent_path,
        args: jvm_args,
    });
}

fn forward_signals(pid: u32) -> Result<Signals, i32> {
    // While the server is running we'll redirect some signals to it
    let signals = Signals::new(&[SIGHUP, SIGINT, SIGQUIT, SIGTRAP, SIGABRT, SIGTERM]);
    let signals = match signals {
        Ok(s) => s,
        Err(err) => {
            eprintln!("Failed to register signal handlers: {}", err);
            return Err(1);
        }
    };

    let signals_bg = signals.clone();
    thread::spawn(move || {
        for sig_int in signals_bg.forever() {
            if let Ok(sig) = signal::Signal::from_c_int(sig_int) {
                let _ = signal::kill(Pid::from_raw(pid as i32), sig);
            }
        }
    });

    return Ok(signals);
}

fn wait_for_child(mut child: Child) -> i32 {
    return match child.wait().map(|status| status.code().unwrap_or(1)) {
        Ok(status) => status,
        Err(err) => {
            eprintln!("Error while running server: {}", err);
            1
        }
    };
}

/// Searches the PATH for java. If that fails, JAVA_HOME is searched as well.
fn find_java() -> Option<PathBuf> {
    return find_prog(&[("PATH", "java"), ("JAVA_HOME", "bin/java")]);
}

fn get_jvm_args(sub_m: &ArgMatches) -> Result<Vec<String>, i32> {
    if let Some(vals) = sub_m.values_of("CUSTOM_ARGS") {
        return Ok(vals.map(|s| s.to_string()).collect());
    }

    // When all else fails, use 500m
    // This should hopefully be small enough to not cause problems for anyone
    let mut heap: String = "500m".to_string();

    if let Some(value) = sub_m.value_of("DEFAULT_ARGS") {
        let reg = Regex::new(r"\d+[mG]").unwrap();
        if !reg.is_match(value) {
            eprintln!("Invalid format for JVM heap size. Should be something like 500m or 2G.");
            return Err(1);
        }

        heap = value.to_string();
    } else {
        // If no arguments are provided, use 1/2 of the current available memory with default flags
        if let Ok(info) = mem_info() {
            // info.avail should always be greater than free, but it seems there may be a bug
            // for macOS. Assuming most users are using linux this doesn't really affect much
            let mem = max(info.avail, info.free);
            // mem is in kb, so convert to mb by dividing by 1000
            // Then we take half of it
            // Cap the amount we automatically choose at 10G
            let mut mb = min((mem / 1000) / 2, 10000).to_string();

            println!(
                "Warning: No memory argument provided, automatically determining to use {} MB \
                 instead. This is not recommended, please specify an amount of memory with -d or \
                 --default-args",
                mb
            );

            mb.push_str("m");
            heap = mb;
        }
    }

    let mut xms = "-Xms".to_string();
    let mut xmx = "-Xmx".to_string();
    xms.push_str(heap.as_str());
    xmx.push_str(heap.as_str());

    return Ok(vec![
        xms,
        xmx,
        "-XX:+UseG1GC".to_string(),
        "-XX:+UnlockExperimentalVMOptions".to_string(),
        "-XX:MaxGCPauseMillis=100".to_string(),
        "-XX:+DisableExplicitGC".to_string(),
        "-XX:TargetSurvivorRatio=90".to_string(),
        "-XX:G1NewSizePercent=50".to_string(),
        "-XX:G1MaxNewSizePercent=80".to_string(),
        "-XX:G1MixedGCLiveThresholdPercent=35".to_string(),
        "-XX:+AlwaysPreTouch".to_string(),
        "-XX:+ParallelRefProcEnabled".to_string(),
        "-Dusing.aikars.flags=mcflags.emc.gs".to_string(),
    ]);
}
