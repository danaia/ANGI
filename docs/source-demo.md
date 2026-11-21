# Pi 5: install, inspect, change, rebuild, verify

This is a reproducible demo procedure, not a completed hardware test report.
The README reports a working Pi 5 terminal dashboard, SSH, services and agent
workshop. Physical HDMI desktop operation and this complete demo remain unverified.
Pi 4 and Zero 2 W have experimental source targets; do not infer that the Pi 5
workshop or native SDK works on those boards.

Use a Linux workstation with the build dependencies, a Pi 5, suitable power,
a dedicated data link and a spare microSD card. Keep the original working card
untouched for rollback. Never run the erase step without fresh approval for the
exact spare device. No hardware is accessed by this document itself.

1. **Install.** Copy the Pi 5 source prompt from the website. It pins the supported
   source baseline, preflights the host, builds and checks the image, prepares
   per-card SSH trust, and requires exact device approval before writing. Record
   the checkout commit, board ID, image SHA-256 and card identity. Complete first
   boot and strict SSH identity verification before proceeding.
2. **Inspect.** Using the same verified key, known_hosts and direct address, run
   `cat /proc/device-tree/model`, `agx status --json` and `agx services --json`.
   Save their output locally as the before record. Confirm Pi 5 hardware and
   inspect the source paths for init and the CLI. Do not change networking,
   credentials, service supervision or boot configuration for this demo.
3. **Make one safe change.** In that checkout create a local demo branch. Add a
   static text marker to the existing rootfs overlay:

   ```sh
   git switch -c demo/pi5-marker
   printf '%s\n' 'ANGI source demo: marker v1' > buildroot/board/agx/rootfs-overlay/etc/angi-demo.txt
   git add buildroot/board/agx/rootfs-overlay/etc/angi-demo.txt
   git commit -m 'Add harmless source demo marker'
   git rev-parse HEAD
   ```

4. **Rebuild.** Keep the original SSH_PUBLIC_KEY selection. Run
   `make build BOARD=rpi5`, then `make check-image BOARD=rpi5` and
   `(cd output/images && sha256sum -c sdcard.img.sha256)`.
   Require `output/images/board.txt` to say `rpi5`. Record the new commit and
   checksum. Re-run the prompt's per-card provisioning and verify the resulting
   private image and its new known_hosts file. Never reuse an old trust file for
   a newly personalized image.
5. **Verify on hardware, with approval.** Follow the prompt's removable-device
   identification, exact erase approval, write/read-back and eject sequence for
   the spare card. Power off before swapping cards. Boot, verify SSH using the
   new per-card trust file, confirm the model, then run:

   ```sh
   cat /etc/angi-demo.txt
   agx status --json
   agx services --json
   ```

   The marker must be exactly `ANGI source demo: marker v1`. Compare status and
   services with the before record. Report the source commit, image checksum,
   flash verification result, model, marker and service observations. Missing
   marker, failed boot, wrong hardware or an SSH mismatch is a failed demo:
   stop and report it, never relax trust checks. To roll back, shut down and
   restore the untouched original card; use its original verified SSH identity.

The marker demonstrates the source-to-image-to-device loop without claiming a
GUI feature or an autonomous on-device full image rebuild. Use the
[agent workshop guide](agent-swarm.md) separately on a verified Pi 5 when its
native SDK and NVMe prerequisites are met.
