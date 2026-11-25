---
name: angi-pi-provisioning
description: Build, provision, flash and verify ANGI on a Raspberry Pi (Zero 2 W, 3, 4/400/CM4, 5) so the Pi boots to on-screen setup, gets Wi-Fi first, then finds the right drivers for its own hardware. Use when preparing or reflashing an SD card, adding Pi 3 support, or checking that a Pi has working Wi-Fi, Bluetooth, sound and display drivers.
---

# Provision ANGI on a Raspberry Pi

Pi targets share one flow with the PC targets (see `angi-build-and-flash-usb` for the remote build,
verify, personalize and flash loop). This skill holds what is **specific to Pis** and the lessons
from the Intel PC work: connectivity first, then drivers found from the hardware that is actually
there. Reach a running Pi with `angi-pi-ethernet` (or the Zero 2 W's USB gadget link). Read
`scripts/boards.json` for IDs, links and host-key aliases instead of trusting this file.

Never put a private hostname, key, serial or password in this file or a commit.

## Targets and their differences

| Target | ID | Link to the workstation | Host alias | Notes |
| --- | --- | --- | --- | --- |
| Pi 5 | `rpi5` | USB-C gadget `10.77.0.2`; Ethernet `10.78.0.2` | `agx_rpi5` | Reference build. NVMe optional; PCIe RP1 carries USB and Ethernet. |
| Pi 4 / 400 / CM4 | `rpi4` | USB-C gadget `10.77.0.2`; Ethernet `10.78.0.2` | `agx_rpi4` | Onboard Broadcom Wi-Fi and Bluetooth. |
| Pi Zero 2 W | `rpizero2w` | micro-USB gadget `10.79.0.2` only | `agx_zero` | 512 MB RAM, 2.4 GHz Wi-Fi only, software VNC; keep the image lean. |
| Pi 3 | not registered | | | Add it first: BCM2837, 64-bit, `vc4` graphics, `brcmfmac` Wi-Fi, UART Bluetooth. Copy `rpi4`, change the DTB, defconfig, `disk_signature`, links and alias, then add it wherever `check-image.py`, `build.sh` and `sync-install-prompt.py` list boards. |

The workstation side of each link is `10.77.0.1`, `10.78.0.1` or `10.79.0.1` with no gateway and
`ipv4.never-default yes`. Reuse the existing NetworkManager profiles (`agx-direct`,
`agx-ethernet`, `angi-zero-usb`); never create duplicates or change the default route.

Pi images are `sdcard.img` (MBR, firmware files on the first partition, `root=PARTUUID=<signature>-02`
in `cmdline.txt`). `provision-sd.py` gives each personalized card a **new** disk signature and
root reference, a fresh Ed25519 host key and a one-line `known_hosts`. The Zero 2 W's older
fixed `root=/dev/mmcblk0p2` path is left as is.

## The order every Pi follows

1. **Boot to the on-screen wizard.** No login prompt, no key in a public image. A Pi with no
   display is reached over its gadget link with the pinned `known_hosts` and VNC through SSH.
2. **Wi-Fi first.** Onboard Wi-Fi is Broadcom (`brcmfmac`) on the SDIO bus, not PCI or USB. The
   Zero 2 W, Pi 3 and Pi 4 need the Broadcom/Cypress firmware and the board's `.txt` and `.clm_blob`
   files; check them in the image. Country code matters for 5 GHz channels (Zero 2 W has none).
   Ethernet is the fallback on Pi 3, 4 and 5.
3. **Then the other drivers, from Settings once online.** Settings → Drivers lists devices and fixes
   what it can; Settings → Display lists the graphics adapter, driver and screens. Both read
   `/sys/class/{drm,net,bluetooth,sound}` as well as PCI and USB, because a Pi's graphics, Wi-Fi
   and Bluetooth sit on the SoC. They fetch missing **firmware** from the upstream linux-firmware
   project and load kernel drivers already in the image; they cannot add userspace stacks.
4. **Late hardware.** `agx-hwload --watch` (shared overlay) loads drivers for USB radios and hubs
   that appear after the first scan. It is shared by every board, so a Pi gets it too.

## What each Pi build must contain

Check in `buildroot/configs/agx_<board>_defconfig`, the board kernel fragment and the built
`target/` before a card is called ready:

- **Wi-Fi:** `brcmfmac` and its firmware set, wpa_supplicant (WPA3), `agx-netd`. Add the common USB
  adapter drivers (rtw88/89, mt76, ath9k_htc) so a dongle also works.
- **Bluetooth:** BlueZ with the audio plugin, the UART transport (`hci_uart`, `btbcm`) and Broadcom
  patch files for the onboard chip; `btusb` for dongles; `agx-bluetoothd`.
- **Sound:** ALSA, PulseAudio, SBC and the board's kernel sound drivers (`snd_bcm2835`, HDMI audio).
  **Without PulseAudio a Bluetooth speaker pairs and then fails with "No more profiles to connect
  to".** The Zero 2 W defconfig currently has no sound stack.
- **Display:** KMS (`vc4`, plus `v3d` on Pi 4 and 5), Mesa with the matching Gallium driver, GBM,
  EGL and GLES; the browser's sandbox needs user namespaces and seccomp in the kernel.
- **Browser:** `BR2_PACKAGE_AGX_WEB`; it is aarch64 and x86-64 only.
- **Loader and tools:** the shared `agx-hwload`, `curl` and CA certificates (the driver finder
  downloads over HTTPS), and enough root filesystem space for the firmware sets.

## Build, personalize, flash

Use `angi-build-and-flash-usb` with `BOARD=<id>`: remote build (personal and public), verify inside
the image, `provision-sd.py` on the build machine (copy `dropbearkey` there if it is missing),
`flash.sh --check` locally, then flash with **the install directory's own `flash.sh`**
(`<install-dir>/flash.sh --device /dev/sdX --expect-identity '<identity>' --confirm-erase /dev/sdX`),
wrapped in a one-line launcher the user runs. See `angi-build-and-flash-usb` sections 4 and 5.

Always list the removable media, show the whole device's path, size, model, serial and contents,
and get explicit erase approval for that exact device. The identity string changes between plugs,
so re-inspect immediately before every flash and stop if anything differs. The user runs the
command: it needs their sudo password. Check the actual image size against the card; Pi images are smaller than the PC image.

The public, shareable image needs `AGX_GENERIC_ISO=1` and a separate `AGX_BUILD_OUTPUT`; today the
generic gate in `scripts/build.sh` allows `rpi4` and `beelink-mini-s`. Extend it deliberately for a
new Pi target, with the generic checks in `check-image.py`.

## Verify on the Pi

Run the same acceptance order as the PC targets and record what was not tested:

1. Wizard appears; the Wi-Fi step lists hotspots and connects.
2. Settings → Drivers shows no unexplained "Not ready" or "No driver"; **Find drivers** completes.
3. Settings → Display lists the adapter and the connected screen; Settings → Bluetooth finds the
   adapter and a device; a Bluetooth speaker connects and plays a tone from Settings → Sound.
4. The Web app loads a page. Check both themes on the new pages.
5. `dmesg | grep -i firmware`: a missing file here names the firmware a driver wants. A built-in
   driver asking before the file system exists (Intel graphics on a PC, some Pi firmware) is cosmetic
   unless the device really fails.

Deploying a tested desktop change to the development Pi follows the standing deployment rule in
`AGENTS.md` and `angi-pi-ethernet`; a card rebuild is only needed for kernel, package or image
changes.

## Known gaps

- No Pi 3 board; the Zero 2 W has no sound stack; the generic image gate does not yet include the
  Pi 5, Pi Zero 2 W or a Pi 3.
- The Drivers and Display pages read the SoC through sysfs classes; they have been unit-tested with
  a Pi-shaped tree but **not yet run on a Pi**. Treat the first Pi run as the real test, and fix the
  skill with what it shows.
- `regulatory.db` is not shipped, so Wi-Fi uses default country rules until it is added.
