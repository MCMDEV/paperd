/* DO NOT EDIT THIS FILE - it is machine generated */
#include <jni.h>
/* Header for class com_destroystokyo_paper_daemon_PaperDaemonJni */

#ifndef _Included_com_destroystokyo_paper_daemon_PaperDaemonJni
#define _Included_com_destroystokyo_paper_daemon_PaperDaemonJni
#ifdef __cplusplus
extern "C" {
#endif
/*
 * Class:     com_destroystokyo_paper_daemon_PaperDaemonJni
 * Method:    createQueue
 * Signature: (Ljava/nio/file/Path;)I
 */
JNIEXPORT jint JNICALL Java_com_destroystokyo_paper_daemon_PaperDaemonJni_createSocket
  (JNIEnv *, jclass, jobject);

/*
 * Class:     com_destroystokyo_paper_daemon_PaperDaemonJni
 * Method:    acceptConnection
 * Signature: (I)Ljava/util/OptionalInt;
 */
JNIEXPORT jint JNICALL Java_com_destroystokyo_paper_daemon_PaperDaemonJni_acceptConnection
  (JNIEnv *, jclass, jint);

/*
 * Class:     com_destroystokyo_paper_daemon_PaperDaemonJni
 * Method:    receiveMessage
 * Signature: (I)Lcom/destroystokyo/paper/daemon/PaperDaemonMessageBuffer;
 */
JNIEXPORT jobject JNICALL Java_com_destroystokyo_paper_daemon_PaperDaemonJni_receiveMessage
  (JNIEnv *, jclass, jint);

/*
 * Class:     com_destroystokyo_paper_daemon_PaperDaemonJni
 * Method:    sendMessage
 * Signature: (ILcom/destroystokyo/paper/daemon/PaperDaemonMessageBuffer;)V
 */
JNIEXPORT void JNICALL Java_com_destroystokyo_paper_daemon_PaperDaemonJni_sendMessage
  (JNIEnv *, jclass, jint, jobject);

/*
 * Class:     com_destroystokyo_paper_daemon_PaperDaemonJni
 * Method:    closeSocket
 * Signature: (I)V
 */
JNIEXPORT void JNICALL Java_com_destroystokyo_paper_daemon_PaperDaemonJni_closeSocket
  (JNIEnv *, jclass, jint);

#ifdef __cplusplus
}
#endif
#endif
