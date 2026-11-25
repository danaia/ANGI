# Google News window drag on ANGI

Open ANGI Web in the signed-in **Native** desktop. Open Google News and click
**Technology**. Leave Terminal visible behind the floating Web
window and wait until the news cards and images are visible. Keep the desktop
at its normal HDMI refresh rate and leave the window size and zoom fixed
during the runs. Aim for 1920 × 1080 at 60 Hz when comparing to the Intel run.
[Google News][news]

The test sends a seven-second drag in a repeating oval from the Web title bar
at 120 pointer messages per second. It measures framebuffer ID changes on the
active HDMI display during the central 5.5 seconds. Each run saves its own CSV.
The report shows updates per second, the 95th-percentile gap, and gaps longer
than one and a half refresh intervals. The VNC client sends pointer input only
during sampling, so the measured rates describe HDMI presentation. Google News
content and network conditions can vary between runs.

## Repeat on the development Pi

Verify the device and open its authenticated VNC tunnel at `127.0.0.1:5941`.
Use the VNC viewer to find the
center of a clear section of the Web title bar, in framebuffer pixels. Leave
room for 110 pixels of horizontal travel in either direction and 60 pixels
below that point. Close the viewer before sampling and keep the SSH tunnel
open. [Pi Ethernet setup](../pi-ethernet.md)

Using the guide's Bash connection block, check which DRM card owns the
connected HDMI output.

```bash
pi_ssh 'for connector in /sys/class/drm/card*-HDMI-*/status; do
  [ "$(cat "$connector" 2>/dev/null)" = connected ] && printf "%s\n" "$connector"
done'
```

Build the sampler on the workstation with the Pi Buildroot toolchain. This
uses the workstation's DRM UAPI headers in a local directory. Adjust the
Buildroot output path if the toolchain is elsewhere.

```bash
mkdir -p .cache
run=$(mktemp -d "$PWD/.cache/google-news-pi.XXXXXX")
mkdir "$run/include"
drm_headers="$(pkg-config --variable=includedir libdrm)/libdrm"
cp "$drm_headers/drm.h" "$drm_headers/drm_mode.h" "$run/include/"
output/host/bin/aarch64-linux-gcc -O2 -static -I"$run/include" \
  scripts/benchmarks/drm-cadence.c -o "$run/drm-cadence"
```

Set `card`, `title_x`, and `title_y` to the observed values and run two passes.

```bash
python3 scripts/benchmarks/window-drag.py \
  --host "$AGX_SSH_HOST" --ssh-option "HostKeyAlias=$AGX_SSH_HOST_KEY_ALIAS" \
  --ssh-option "IdentityFile=$AGX_SSH_KEY" --ssh-option IdentitiesOnly=yes \
  --vnc-port 5941 --card "$card" --title "$title_x" "$title_y" \
  --probe "$run/drm-cadence" --output "$run/samples" | tee "$run/summary.txt"
```

## Intel baseline, November 25, 2026

Beelink Mini S, Intel Celeron N5095 with `i915`, kernel 6.12.47, native
1920 × 1080 HDMI at 60 Hz. Terminal remained open behind ANGI Web. The first
two runs began while Google News was settling after the category click.

| Run | HDMI updates/s | p95 gap | Gaps over 25 ms |
| --- | ---: | ---: | ---: |
| After category click, 1 | 59.46 | 17.07 ms | 3 / 326 |
| After category click, 2 | 59.64 | 17.04 ms | 2 / 327 |
| Page loaded, 1 | 59.64 | 17.04 ms | 2 / 327 |
| Page loaded, 2 | 59.45 | 17.06 ms | 3 / 326 |

[news]: https://news.google.com/home?hl=en-US&gl=US&ceid=US:en
