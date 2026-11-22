// Test-only writer for the private PulseAudio null sinks.
#define _POSIX_C_SOURCE 200809L
#include <inttypes.h>
#include <math.h>
#include <pthread.h>
#include <pulse/error.h>
#include <pulse/simple.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

enum { RATE = 16000, FRAGMENT = 320, MARKER = 12800 };
static const int64_t ns_per_frame = 1000000000 / RATE;
struct route {
    const char *sink;
    int hz;
    pa_simple *stream;
    int64_t target_ns, before_ns, after_ns;
};

static void fail(const char *message, int err) {
    fprintf(stderr, "%s: %s\n", message, pa_strerror(err));
    exit(1);
}
static int64_t now_ns(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) { perror("clock_gettime"); exit(1); }
    return (int64_t)ts.tv_sec * 1000000000 + ts.tv_nsec;
}
// Estimate the render time of the end of the bytes already written, using
// Pulse's playback clock. Bracket read time to reject an untimed observation.
static int64_t render_end(pa_simple *s) {
    int error;
    int64_t before = now_ns();
    pa_usec_t delay = pa_simple_get_latency(s, &error);
    int64_t after = now_ns();
    if (delay == (pa_usec_t)-1) fail("playback latency", error);
    if (after - before > 50000000 && delay > 0) {
        before = now_ns();
        delay = pa_simple_get_latency(s, &error);
        after = now_ns();
        if (delay == (pa_usec_t)-1) fail("playback latency", error);
    }
    if (after - before > 50000000 || delay > 3000000) {
        fprintf(stderr, "playback clock observation out of bounds\n"); exit(1);
    }
    return before + (after - before) / 2 + (int64_t)delay * 1000;
}
static void write_frames(pa_simple *s, const int16_t *pcm, size_t frames) {
    int err;
    if (frames && pa_simple_write(s, pcm, frames * sizeof(*pcm), &err) < 0) fail("write", err);
}
static void *play(void *opaque) {
    struct route *r = opaque;
    int16_t silence[FRAGMENT] = {0}, marker[MARKER];
    for (int i = 0; i < MARKER; ++i)
        marker[i] = (int16_t)(8192.0 * sin(6.283185307179586 * r->hz * i / RATE));
    // Keep each stream running and schedule sample zero on a shared render clock.
    // Waiting for two child processes to start measures neither server's playback.
    for (;;) {
        write_frames(r->stream, silence, FRAGMENT);
        int64_t remaining = r->target_ns - render_end(r->stream);
        if (remaining > FRAGMENT * ns_per_frame) continue;
        if (remaining > 0) write_frames(r->stream, silence, (size_t)(remaining / ns_per_frame));
        break;
    }
    r->before_ns = render_end(r->stream);
    write_frames(r->stream, marker, MARKER);
    r->after_ns = render_end(r->stream) - MARKER * ns_per_frame;
    int err;
    if (pa_simple_drain(r->stream, &err) < 0) fail("drain", err);
    return NULL;
}
int main(void) {
    const char *server = getenv("PULSE_SERVER");
    const char *dir = getenv("CUEGLINT_PULSE_FIXTURE");
    if (!server || !dir || dir[0] != '/') return 2;
    char expected[4096];
    int n = snprintf(expected, sizeof(expected), "unix:%s/native", dir);
    if (n < 0 || (size_t)n >= sizeof(expected) || strcmp(server, expected) != 0) return 2;
    const pa_sample_spec spec = {.format=PA_SAMPLE_S16LE, .rate=RATE, .channels=1};
    const pa_buffer_attr attr = {.maxlength=UINT32_MAX, .tlength=2*FRAGMENT*2,
        .prebuf=UINT32_MAX, .minreq=UINT32_MAX, .fragsize=UINT32_MAX};
    struct route routes[] = {{.sink="cueglint_decoy", .hz=400}, {.sink="cueglint_output", .hz=800}};
    pthread_t threads[2];
    for (int i = 0; i < 2; ++i) {
        int err;
        routes[i].stream = pa_simple_new(server, "Cueglint original tone fixture", PA_STREAM_PLAYBACK,
            routes[i].sink, "Original tone", &spec, NULL, &attr, &err);
        if (!routes[i].stream) fail("open fixture playback", err);
    }
    int64_t target = now_ns() + 3000000000LL;
    for (int i=0;i<2;++i) {
        routes[i].target_ns=target;
        if (pthread_create(&threads[i], NULL, play, &routes[i]) != 0) return 1;
    }
    for (int i=0;i<2;++i) {
        if (pthread_join(threads[i], NULL) != 0) return 1;
        pa_simple_free(routes[i].stream);
        printf("%s %"PRId64" %"PRId64" %"PRId64"\n", routes[i].sink, target, routes[i].before_ns, routes[i].after_ns);
    }
    return 0;
}
