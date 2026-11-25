/* Sample the framebuffer ID of an active DRM display at 1 ms intervals. */
#define _POSIX_C_SOURCE 200809L
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <time.h>
#include <unistd.h>
#include <drm.h>

static uint64_t monotonic_ns(void)
{
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (uint64_t)now.tv_sec * 1000000000 + now.tv_nsec;
}

int main(int argc, char *argv[])
{
    if (argc != 2) {
        fprintf(stderr, "Usage: %s /dev/dri/cardN\n", argv[0]);
        return 1;
    }
    int drm = open(argv[1], O_RDONLY | O_CLOEXEC);
    if (drm < 0) {
        perror(argv[1]);
        return 1;
    }
    uint32_t ids[32];
    struct drm_mode_card_res resources = {
        .crtc_id_ptr = (uintptr_t)ids, .count_crtcs = 32,
    };
    if (ioctl(drm, DRM_IOCTL_MODE_GETRESOURCES, &resources) < 0) {
        perror("DRM display resources");
        return 1;
    }
    struct drm_mode_crtc display = {0};
    for (unsigned i = 0; i < resources.count_crtcs && i < 32; ++i) {
        display = (struct drm_mode_crtc){ .crtc_id = ids[i] };
        if (ioctl(drm, DRM_IOCTL_MODE_GETCRTC, &display) == 0 && display.mode_valid &&
            display.fb_id && display.mode.htotal && display.mode.vtotal)
            break;
        display.mode_valid = 0;
    }
    if (!display.mode_valid) {
        fputs("No active display with a readable framebuffer ID\n", stderr);
        return 1;
    }
    double hz = 1000.0 * display.mode.clock / (display.mode.htotal * (double)display.mode.vtotal);
    printf("mode,%u,%u,%.3f\n", display.mode.hdisplay, display.mode.vdisplay, hz);
    fflush(stdout);

    uint64_t start = monotonic_ns();
    uint64_t next = start, previous = start, max_gap = 0;
    uint32_t last = 0;
    while (next - start < 8000000000ULL) {
        if (ioctl(drm, DRM_IOCTL_MODE_GETCRTC, &display) < 0) {
            perror("Read DRM framebuffer ID");
            return 1;
        }
        uint64_t now = monotonic_ns();
        if (now - previous > max_gap)
            max_gap = now - previous;
        previous = now;
        if (display.fb_id != last) {
            printf("fb,%.3f,%u\n", (now - start) / 1000000.0, display.fb_id);
            last = display.fb_id;
        }
        do { next += 1000000; } while (next <= now);
        struct timespec wake = { next / 1000000000, next % 1000000000 };
        clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &wake, NULL);
    }
    printf("max_poll_gap_ms,%.3f\n", max_gap / 1000000.0);
    close(drm);
    return 0;
}
