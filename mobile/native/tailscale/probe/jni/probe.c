// Pont JNI minimal vers libtailscale pour le sonde de connectivite.
#include <jni.h>
#include <errno.h>
#include <poll.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include "tailscale.h"

static tailscale g_sd = -1;

#define LOG_CAP (64 * 1024)
static char g_log[LOG_CAP];
static size_t g_log_len = 0;
static pthread_mutex_t g_log_mu = PTHREAD_MUTEX_INITIALIZER;

static void *log_reader(void *arg) {
    int fd = (int)(long)arg;
    char buf[1024];
    ssize_t n;
    while ((n = read(fd, buf, sizeof buf)) > 0) {
        pthread_mutex_lock(&g_log_mu);
        for (ssize_t i = 0; i < n; i++) {
            if (g_log_len + 1 >= LOG_CAP) {  // tampon plein : on garde la fin
                memmove(g_log, g_log + LOG_CAP / 2, LOG_CAP - LOG_CAP / 2);
                g_log_len = LOG_CAP - LOG_CAP / 2;
            }
            g_log[g_log_len++] = buf[i];
        }
        g_log[g_log_len] = 0;
        pthread_mutex_unlock(&g_log_mu);
    }
    return NULL;
}

static jstring fmt_err(JNIEnv *env, const char *what) {
    char msg[512] = "";
    if (g_sd >= 0) tailscale_errmsg(g_sd, msg, sizeof msg);
    char out[768];
    snprintf(out, sizeof out, "ERREUR %s : %s", what, msg[0] ? msg : "(sans detail)");
    return (*env)->NewStringUTF(env, out);
}

// Cree le serveur et le connecte au tailnet (bloque jusqu'a usable ou erreur).
JNIEXPORT jstring JNICALL Java_com_nestor_tsprobe_Native_up(
        JNIEnv *env, jclass cls, jstring jdir, jstring jhost, jstring jkey) {
    const char *dir = (*env)->GetStringUTFChars(env, jdir, 0);
    const char *host = (*env)->GetStringUTFChars(env, jhost, 0);
    const char *key = (*env)->GetStringUTFChars(env, jkey, 0);

    int p[2];
    if (pipe(p) == 0) {
        pthread_t t;
        pthread_create(&t, NULL, log_reader, (void *)(long)p[0]);
        pthread_detach(t);
    }

    jstring result;
    g_sd = tailscale_new();
    if (g_sd < 0) {
        result = (*env)->NewStringUTF(env, "ERREUR tailscale_new");
    } else if (tailscale_set_dir(g_sd, dir) || tailscale_set_hostname(g_sd, host) ||
               tailscale_set_authkey(g_sd, key) || tailscale_set_ephemeral(g_sd, 1) ||
               tailscale_set_logfd(g_sd, p[1])) {
        result = fmt_err(env, "configuration");
    } else if (tailscale_up(g_sd)) {
        result = fmt_err(env, "tailscale_up");
    } else {
        char ips[256] = "";
        tailscale_getips(g_sd, ips, sizeof ips);
        char out[400];
        snprintf(out, sizeof out, "OK connecte au tailnet, IPs : %s", ips);
        result = (*env)->NewStringUTF(env, out);
    }
    (*env)->ReleaseStringUTFChars(env, jdir, dir);
    (*env)->ReleaseStringUTFChars(env, jhost, host);
    (*env)->ReleaseStringUTFChars(env, jkey, key);
    return result;
}

// Ouvre une connexion TCP via le tailnet, envoie `payload` et lit la reponse.
JNIEXPORT jstring JNICALL Java_com_nestor_tsprobe_Native_dial(
        JNIEnv *env, jclass cls, jstring jaddr, jstring jpayload) {
    if (g_sd < 0) return (*env)->NewStringUTF(env, "ERREUR : tailscale_up pas encore appele");
    const char *addr = (*env)->GetStringUTFChars(env, jaddr, 0);
    const char *payload = (*env)->GetStringUTFChars(env, jpayload, 0);

    tailscale_conn conn = -1;
    jstring result;
    if (tailscale_dial(g_sd, "tcp", addr, &conn)) {
        result = fmt_err(env, "tailscale_dial");
    } else {
        size_t plen = strlen(payload);
        if (plen > 0) write(conn, payload, plen);
        char buf[512];
        size_t total = 0;
        struct pollfd pfd = { .fd = conn, .events = POLLIN };
        while (total < sizeof buf && poll(&pfd, 1, 5000) > 0) {
            ssize_t n = read(conn, buf + total, sizeof buf - total);
            if (n <= 0) break;
            total += (size_t)n;
        }
        close(conn);
        char out[160];
        snprintf(out, sizeof out, "OK connexion TCP etablie vers %s, %zu octets recus en reponse", addr, total);
        result = (*env)->NewStringUTF(env, out);
    }
    (*env)->ReleaseStringUTFChars(env, jaddr, addr);
    (*env)->ReleaseStringUTFChars(env, jpayload, payload);
    return result;
}

JNIEXPORT void JNICALL Java_com_nestor_tsprobe_Native_close(JNIEnv *env, jclass cls) {
    if (g_sd >= 0) { tailscale_close(g_sd); g_sd = -1; }
}

JNIEXPORT jstring JNICALL Java_com_nestor_tsprobe_Native_logs(JNIEnv *env, jclass cls) {
    pthread_mutex_lock(&g_log_mu);
    // Evite une coupure au milieu d'un caractere UTF-8 : on remplace les octets non ASCII.
    char tmp[LOG_CAP];
    size_t n = g_log_len;
    for (size_t i = 0; i < n; i++) tmp[i] = ((unsigned char)g_log[i] < 0x80) ? g_log[i] : '?';
    tmp[n] = 0;
    pthread_mutex_unlock(&g_log_mu);
    return (*env)->NewStringUTF(env, tmp);
}
