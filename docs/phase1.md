# Phase 1 — Pi 5 boot image

## Review decisions

- Use the Pi 5 EEPROM bootloader already installed on the board. The SD image does not flash or replace EEPROM. Check SD boot support on the board before changing its EEPROM settings.
- Pin Buildroot 2026.08.1, including its kernel, Bootlin cross toolchain and Raspberry Pi firmware selections. The downloaded Buildroot archive is checked against the SHA-256 recorded in `scripts/build.sh`; package downloads use upstream hash checks. This is a fixed build input baseline, not a claim of independently verified or bit-identical builds.
- Keep the upstream `bcm2712` kernel and 4 KB page-size fragment for initial bring-up; add a fragment that builds the DWC2 device controller and Ethernet gadget into the kernel so `usb0` exists before BusyBox networking starts. Preserve both Pi 5 DTB revisions from upstream. Kernel trimming belongs to Phase 2 after a successful baseline boot. The vendor kernel release suffix may still say `v8-16k`; verify actual page size through the resolved kernel config and `/proc/1/smaps`, rather than the release name.
- Use BusyBox init and shell for Phase 1 recovery and login. Rust PID 1 arrives in Phase 3.
- Enable HDMI/USB login on `tty1` through the upstream post-build hook. Also enable the Pi 5 debug UART login on `ttyAMA10`, at 115200 baud.
- At the user's request, bring forward minimal SSH over USB access for Phase 1 testing. Use USB Ethernet gadget mode through the Pi's USB-C port, with the Pi at `10.77.0.2/24` and workstation at `10.77.0.1/24`, without a gateway. Dropbear listens only on the Pi's cable address and disables SSH password login. The build copies a selected workstation public key into an ignored local overlay. Local recovery login remains `root` with an empty password. Wi-Fi and Rust networking remain later phases.
- Do not advance to Phase 2 until the image is built, validated, and a real Pi boot is recorded.

## Build

Install prerequisites and run the commands in the README. `make configure` deliberately resets the output configuration to the tracked AGX defconfig. `make build` resumes an existing build; use `make configure` after changing the defconfig. The Buildroot source is cached under `.cache/src/`, downloads under `.cache/downloads/`, and build outputs under `output/`.

The external tree in `buildroot/` derives its configuration from the pinned upstream `raspberrypi5_defconfig`. It uses the upstream genimage helper with an AGX layout: an MBR disk image, a 128 MB FAT boot partition (including the DWC2 overlay), and a 256 MB ext4 root partition. AGX's `config.txt` selects the ARM64 `Image`; `cmdline.txt` selects SD partition 2 as root and directs kernel messages to serial while keeping the HDMI login console available.

`make check-image` checks the ARM64 and DTB headers, boot file presence in the FAT image, ext filesystem magic, partition layout, partition contents, generated console entries, and the SSH/network files read back from the ext4 image. These checks do not establish physical boot success.

For a build log:

```sh
set -o pipefail
make build 2>&1 | tee .cache/build.log
make check-image
sha256sum output/images/sdcard.img
```

## Clean install: format and write the card

`scripts/flash-sd.sh` is the supported way to erase a card and write `output/images/sdcard.img`. `--list` shows candidate cards and changes nothing. `--device /dev/sdX` shows the card (path, size, model, serial, partitions) and requires typing the path to confirm. A coding agent passes `--confirm-erase /dev/sdX` only after the person has approved that exact card. The script refuses partitions, disks holding system mounts or the root filesystem, non-removable disks, drives over 256 GiB (`--max-gib` overrides) and cards smaller than the image. It rechecks the card's identity and holds a whole-disk lock from unmount through eject. It leaves the new partition table unpublished during read-back so the desktop cannot auto-mount the new filesystems and change them. The script flushes the write, compares every byte, and reports success after ejection. `make flash-sd DEVICE=/dev/sdX` wraps it.

The [board registry](../scripts/boards.json) lists Pi 5 as ready, Pi 4 and Zero 2 W as experimental, and the ARM64 and x86-64 desktop targets as planned. The [Pi 4 guide](pi4-provisioning.md) covers the HDMI desktop, VNC and embedded browser. The [Zero 2 W guide](pi-zero-2w.md) covers its software VNC desktop and micro-USB setup. The kernel command line selects the supported serial console for each board.

`scripts/detect-board.py` reports the workstation and can read a running Pi's model over SSH. Its `--require ID` command exits 3 for boards without a build. `make build` stamps the board ID in the selected output directory and the card writer checks that ID before writing.

### Pi 4 / 400 provisioning

Follow the [Pi 4 / 400 provisioning guide](pi4-provisioning.md) to build and verify `sdcard.img`, write it with the workstation SSH public key, and provision the administrator and Wi-Fi in the TUI over the direct link.

### Pi 5 desktop SD image

Run the same build and provisioning sequence with `BOARD=rpi5`. `make check-image BOARD=rpi5` produces `output/images/sdcard.img.sha256`. Verify it before writing the card. The image includes the desktop, loopback VNC, and ANGI Web. After booting it, use [the Pi 5 SSH setup prompt](install-prompts/part2-rpi5.txt).

For a private Pi 5 card with SSH trust ready before boot, install
`dropbearkey` on the workstation and run:

```sh
python3 scripts/provision-sd.py --output-dir .cache
```

The command prepares a workstation login key if one is missing, embeds its
public half, and creates a fresh host key for this card. It prints the private
install directory and the local SSH identity to use. Pass `--public-key` to
select a specific existing identity. Flash the private `sdcard.img`
with `scripts/flash-sd.sh --board rpi5 --personalized --image PATH_TO_SD_IMAGE --device DEVICE`
after identifying the microSD card. Keep the image private because it contains
the Pi's host private key. The matching `known_hosts` file pins the public
key for both direct connections. With the workstation's `agx-ethernet`
profile active, the Pi configures `eth0` as `10.78.0.2/24` at boot.
Connect with strict SSH verification using the private install directory's
`known_hosts` file. [Pi Ethernet guide](pi-ethernet.md#connect-and-verify)

The Pi 5 boots from microSD and can mount a separate NVMe data partition. Its firmware enables external PCIe for NVMe adapters. Identify the ext4 partition you intend to use with `blkid` on the Pi. Save its filesystem UUID in `/etc/agx/data-volume.uuid` as root and reboot. Verify with `mountpoint /var/lib/agx-data` and `df /var/lib/agx-data`. ANGI mounts only a configured UUID and uses the volume for browser and agent data. On a microSD-only system, Web keeps its data in the signed-in account’s home. [Raspberry Pi PCIe documentation](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html)

### Pi 4 / 400 hybrid ISO alternative

Build the release in a separate output directory with `AGX_GENERIC_ISO=1 AGX_BUILD_OUTPUT=/absolute/output-rpi4-iso make build BOARD=rpi4`. Run `AGX_BUILD_OUTPUT=/absolute/output-rpi4-iso make iso BOARD=rpi4` with `xorriso` installed or set `XORRISO` to a built copy. Distribute `images/angi-rpi4.iso` and `images/angi-rpi4.iso.sha256` together. The ISO 9660 image contains the FAT boot partition at MBR slot 1 and the ext4 root partition at slot 2. The release check requires empty SSH key directories and default Wi-Fi settings.

For browser setup, verify the release checksum, write the generic ISO to the approved card, and boot the Pi with a direct USB-C data connection. ANGI assigns the workstation `10.77.0.1` without changing its default route. Open `http://angi.local` or `http://10.77.0.2` to create the first administrator and join Wi-Fi. The Pi generates its SSH host key, and the setup page closes after claiming the Pi. The page and the automatic address assignment are limited to the dedicated USB gadget.

Use the [headless provisioning prompt](install-prompts/iso-headless.md) for agent-assisted setup. The SSH option prepares a private install ISO and a dedicated SSH trust file before writing the card. It can use USB-C or direct Ethernet with manual addressing.

The image configures both links: USB-C gadget `usb0` at `10.77.0.2/24` (workstation `10.77.0.1/24`, profile `agx-direct`) and RJ45 `eth0` at `10.78.0.2/24` (workstation `10.78.0.1/24`, profile `agx-ethernet`). Neither has a gateway; Wi-Fi carries the Pi's internet access. Use the Pi's own power supply with the Ethernet cable.

## Test with the 32 GB card and SSH over USB

Use a data-capable cable from the workstation's USB-A port to the Pi 5's USB-C port. The Pi presents a USB Ethernet device after AGX boots. The Pi uses `usb0` at `10.77.0.2/24`; the workstation uses `10.77.0.1/24`. No router, Wi-Fi setup, or Internet sharing is required.

The gadget has fixed MAC addresses: workstation `02:77:00:00:00:01`, Pi `02:77:00:00:00:02`. Connect one AGX development Pi at a time. A dedicated `agx-direct` NetworkManager profile has been saved on this workstation, bound to the gadget MAC, with autoconnect enabled and no default route. On another Linux workstation, create it once:

```sh
nmcli connection add type ethernet ifname '*' con-name agx-direct \
  802-3-ethernet.mac-address 02:77:00:00:00:01 \
  ipv4.method manual ipv4.addresses 10.77.0.1/24 ipv4.never-default yes \
  ipv6.method disabled connection.autoconnect yes
```

After booting the card, check for the USB Ethernet adapter and connect:

```sh
nmcli device status
ping -c 3 10.77.0.2
ssh -i ~/.ssh/id_ed25519 root@10.77.0.2
```

If automatic activation fails, find the gadget interface in `nmcli device status` (usually `enx027700000001` on this host) and run `nmcli connection up agx-direct ifname INTERFACE`.

A USB-A port may provide too little power for a Pi 5. Reboots, undervoltage warnings or repeated USB disconnects require a suitable powered data connection, or switching to the official Pi power supply plus an actual RJ45 Ethernet cable and a corresponding wired image configuration. Do not treat unstable power as a software boot failure. The Pi's other USB ports remain available for the recovery keyboard.

The first SSH connection asks to trust the newly generated host key. For direct verification, obtain the fingerprint from the HDMI recovery console with `dropbearkey -y -f /etc/dropbear/dropbear_ed25519_host_key` and compare it. Reflashing produces new host keys; check the new fingerprint before removing the old entry with `ssh-keygen -R 10.77.0.2`.

Capture boot evidence over SSH from the workstation:

```sh
mkdir -p .cache/evidence
ssh -i ~/.ssh/id_ed25519 root@10.77.0.2 \
  'uname -a; cat /proc/device-tree/model; echo; cat /proc/cmdline; grep -m1 KernelPageSize /proc/1/smaps; cat /proc/meminfo; mount; ps; dmesg' \
  | tee .cache/evidence/pi5-boot.txt
sha256sum output/images/sdcard.img | tee .cache/evidence/sdcard.sha256
```

An SSH shell proves the system reached userspace; serial or HDMI evidence is still needed to verify that the local recovery console works. Disconnect the workstation profile with `nmcli connection down agx-direct` after testing. The image build itself does not modify workstation networking.

## Write the card and use HDMI/USB recovery

1. Build and validate `sdcard.img` first.
2. Use Raspberry Pi Imager's **Use custom** option to select `output/images/sdcard.img`. Select the 32 GB microSD card by its device identity and capacity. Writing erases the selected card. Skip OS customisation; this is a Buildroot image. Let the writer verify it, then eject the card.
3. With the Pi powered off, insert the card and connect an HDMI monitor to HDMI0, a USB keyboard, and suitable Pi 5 power. Apply power.
4. Confirm kernel messages appear, followed by the AGX banner and a login prompt. Log in as `root` with no password.
5. Run:

```sh
uname -a
cat /proc/device-tree/model; echo
cat /proc/cmdline
grep -m1 KernelPageSize /proc/1/smaps
cat /proc/meminfo | head
mount
ps
```

6. Save the visible results and boot messages (a monitor photo is acceptable for initial evidence), plus the image SHA-256, board model/RAM, and test date. Run `poweroff` before disconnecting power.

The image uses a small fixed filesystem and leaves most of the 32 GB card unused. Automatic filesystem growth is outside Phase 1.

If HDMI stays blank, record LED patterns and check power, HDMI0/cable/display, card write verification, and EEPROM SD boot support. Serial diagnostics use the Pi 5 dedicated debug UART connector, not an assumed Pi 4 GPIO UART mapping.

## Acceptance record

- Configuration generated successfully on the development workstation.
- Host dependency check passed using signed distro `bc` and `wget` binaries extracted under `.cache/host-tools/`; system packages were not changed.
- Full image build: **passed**, November 15, 2026. Vendor kernel `6.12.61-v8-16k` with verified 4 KB pages and built-in USB gadget support.
- Artifact checks: **passed**, including disk partition contents, packed boot configuration, SSH/network files read from ext4, and root-owned SSH directory/key modes (`0700`/`0600`). Corrupted MBR and boot-partition copies were correctly rejected.
- SD card write: **passed**, November 15, 2026. The approved 29.1 GiB removable card (serial `000000009451`) was written, synced, and read back with a matching image SHA-256, then unmounted and safely powered off. Local evidence: `.cache/evidence/card-flash.txt`.
- Physical Pi 5 boot and USB Ethernet SSH: **passed**, November 15, 2026. The user opened an interactive root shell, and the workstation independently connected with public-key authentication and captured live system information and the kernel boot log.
- Phase 1 acceptance for the user-selected USB SSH test path: **passed**. No Phase 2 work has started.
- Interactive HDMI and debug UART recovery login: **unverified**. Both getty processes are running, but the boot log records `bcm2708_fb ... probe ... failed with error -2`; display bring-up requires a separate physical check.

### Hardware observations

- Board: Raspberry Pi 5 Model B Rev 1.1; ARM64 kernel `6.12.61-v8-16k`.
- Runtime page size: `KernelPageSize: 4 kB`.
- Memory reported: `MemTotal: 16387812 kB` (about 15.6 GiB).
- SD card detected as `SD32G`, 29.1 GiB, with partitions `p1` and `p2`.
- Kernel mounted the ext4 root at about 2.9 seconds, started `/sbin/init`, and remounted root read/write at about 3.0 seconds. This timing describes the kernel log, excluding EEPROM startup and time until SSH login.
- USB gadget driver bound successfully; live `usb0` is up at `10.77.0.2/24` with the configured device MAC.
- Dropbear is running with password authentication disabled; a public-key SSH session succeeded.
- SSH host fingerprint read directly on the Pi matches the user's first-connection prompt: `SHA256:cgKA/OnkSFz5mLIuid7kuuRLhFk7wCP1+foOHNuAstQ`.
- Local boot evidence: `.cache/evidence/pi5-boot.txt`. The user-visible empty `ls` output comes from the initial root home directory; the shell itself is working.

This is the historical Phase 1 build record. Later builds replace `output/images/sdcard.img`; see [Phase 2](phase2.md) for its current build record.

Phase 1 generated image: `output/images/sdcard.img`, 402,653,696 bytes. Workstation-specific public-key input is intentionally outside Git. This build's SHA-256:

```text
2eeecad2952da59f522d547a6dd98077d6841324b633ae53547cc49ffda79b41
```

Local evidence: `.cache/build.log`, `.cache/evidence/image-validation.txt`, and `.cache/evidence/sdcard.sha256`. These are generated artifacts and are ignored by Git.

## References

- [Buildroot releases](https://buildroot.org/download.html)
- [Buildroot manual: requirements and external trees](https://buildroot.org/downloads/manual/manual.html)
- [Pinned Pi 5 board support](https://github.com/buildroot/buildroot/tree/2026.08.1/board/raspberrypi5)
- [Raspberry Pi USB networking and power guidance](https://www.raspberrypi.com/news/usb-gadget-mode-in-raspberry-pi-os-ssh-over-usb/)
- [Raspberry Pi boot documentation](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html)
