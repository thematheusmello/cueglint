#define _GNU_SOURCE
#include <alsa/asoundlib.h>
#include <dlfcn.h>
#include <inttypes.h>
#include <stdio.h>
#include <time.h>

static int (*avail_delay)(snd_pcm_t *, snd_pcm_sframes_t *, snd_pcm_sframes_t *);
static snd_pcm_sframes_t (*read_pcm)(snd_pcm_t *, void *, snd_pcm_uframes_t);
static _Thread_local snd_pcm_sframes_t available, delayed;
static _Thread_local uint64_t measured;

static struct {
    uint64_t measured, read_finished;
    snd_pcm_sframes_t available, delayed, frames;
} reads[512];
static size_t count;

__attribute__((constructor)) static void bind_alsa(void)
{
    avail_delay = dlsym(RTLD_NEXT, "snd_pcm_avail_delay");
    read_pcm = dlsym(RTLD_NEXT, "snd_pcm_readi");
}

uint64_t cueglint_test_alsa_now_ns(void)
{
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0)
        return 0;
    return (uint64_t)now.tv_sec * 1000000000 + now.tv_nsec;
}

int snd_pcm_avail_delay(snd_pcm_t *pcm, snd_pcm_sframes_t *avail,
                        snd_pcm_sframes_t *delay)
{
    if (!avail_delay)
        return -ENOSYS;
    int result = avail_delay(pcm, avail, delay);
    if (result == 0) {
        available = *avail;
        delayed = *delay;
        measured = cueglint_test_alsa_now_ns();
    }
    return result;
}

snd_pcm_sframes_t snd_pcm_readi(snd_pcm_t *pcm, void *buffer, snd_pcm_uframes_t size)
{
    if (!read_pcm)
        return -ENOSYS;
    snd_pcm_sframes_t result = read_pcm(pcm, buffer, size);
    // Only the isolated one-stream fixture uses this shim. Print after the worker joins.
    if (result > 0 && count < sizeof(reads) / sizeof(reads[0])) {
        reads[count].measured = measured;
        reads[count].read_finished = cueglint_test_alsa_now_ns();
        reads[count].available = available;
        reads[count].delayed = delayed;
        reads[count++].frames = result;
    }
    return result;
}

size_t cueglint_test_alsa_dump(uint64_t marker_ns)
{
    for (size_t i = 0; i < count; i++) {
        if (reads[i].read_finished + 50000000 < marker_ns ||
            reads[i].read_finished > marker_ns + 400000000)
            continue;
        fprintf(stderr,
                "ALSA capture read=%zu measured_ns=%" PRIu64 " read_finished_ns=%" PRIu64
                " available_frames=%ld delay_frames=%ld frames_read=%ld\n",
                i, reads[i].measured, reads[i].read_finished, reads[i].available,
                reads[i].delayed, reads[i].frames);
    }
    return count;
}
