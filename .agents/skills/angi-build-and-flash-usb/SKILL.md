---
name: angi-build-and-flash-usb
description: Rebuild and write an ANGI image for any supported hardware (Raspberry Pi Zero 2 W, 3, 4, 5, ARM or x86 UEFI PCs, Intel and AMD) on the fast remote build machine, verify it, personalize it, and flash the SD card or USB stick. Use after any change that must reach a machine, when adding a hardware target, or when asked to rebuild, reflash, or publish a shareable image.
---

# Build, verify and flash an ANGI image for any hardware target

The repeat loop is the same for every board: change, test, build remotely, verify, personalize,
flash, test on the hardware. `angi-install-media` covers choosing the target for a first install;
`angi-pi-ethernet` covers reaching a Pi. Board IDs, image names, output directories and host-key
aliases come from `scripts/boards.json`; read it instead of trusting the table below.

Never put a private hostname, key, device serial or password in this file or in a commit.

## Targets

| Target | Registry ID | Image | Notes |
| --- | --- | --- | --- |
| Raspberry Pi 5 | `rpi5` | `sdcard.img` | The reference build. |
| Raspberry Pi 4 / 400 / CM4 | `rpi4` | `sdcard.img` | |
| Raspberry Pi Zero 2 W | `rpizero2w` | `sdcard.img` | USB gadget network; 2.4 GHz Wi-Fi only; little RAM. |
| Raspberry Pi 3 | none yet | `sdcard.img` | Not in the registry. Add it (see below) before building. |
| x86-64 PC, Intel or AMD | `x86-desktop` | `disk.img` | Generic UEFI image; experimental. |
| Beelink Mini S (Intel N5095) | `beelink-mini-s` | `disk.img` | Hardware-tested x86 example. |
| ARM64 UEFI PC | `arm-desktop` | `disk.img` | |
| A specific AMD or Intel machine | add an entry | `disk.img` | Copy the closest UEFI board, then change the GPU, Wi-Fi and Bluetooth packages. |

Pi images boot from firmware files with a disk signature in `cmdline.txt`; UEFI images are GPT with
an EFI system partition and `root=PARTUUID=` in `grub.cfg`. `provision-sd.py` and `flash-sd.sh`
handle both; every personalized install directory carries its own `flash.sh` (section 4). A new board needs a `boards.json` entry, a defconfig under `buildroot/configs`, a
board directory under `buildroot/board/agx`, and the board added to `scripts/check-image.py`,
`scripts/build.sh` (generic-image gate) and `scripts/sync-install-prompt.py` where the existing
boards are listed. Run `python3 scripts/detect-board.py --require <id>` before building.

## The goal of every build: Wi-Fi first, then the rest

Every image must get the machine online on its own, then let the person fetch the rest from
Settings. Design and test each target against this order:

1. **Boot to the on-screen setup, never a login prompt.** The public image has no keys and a locked
   root account; the person creates the administrator on screen.
2. **Wi-Fi works first.** The image carries a wide set of Wi-Fi drivers and firmware for that
   hardware class, and a boot-time loader (`agx-hwload --watch`) that loads whatever driver a
   device asks for, including USB radios that appear seconds after boot. The setup wizard finds
   hotspots and connects; Settings → Wi-Fi changes the network later. Ethernet is the fallback.
3. **Then the other drivers, from Settings once online.** Settings → Drivers finds hardware that
   lacks a driver, loads the kernel's own driver, and downloads the firmware a driver asked for
   (from the upstream linux-firmware project), then restarts that driver. Settings → Display does
   the same for graphics and lists the connected screens. Bluetooth and Sound have their own pages.
4. **Primary drivers per target:** display, Bluetooth, sound and Wi-Fi must each work, or the
   Drivers page must say exactly why not.

What the Settings pages can and cannot do: they fetch **firmware** and load **kernel drivers that
are already in the image**. They cannot add a kernel module the image lacks, a built-in driver's
boot-time firmware, or userspace stacks (Mesa, ALSA, BlueZ). Those must be in the image, so the
build has to include the right ones for the hardware class.

## Driver coverage to check for each target

Verify these in `buildroot/configs/agx_<board>_defconfig`, the board's kernel fragment and the
built image (`target/` and the kernel `.config`) before calling a target done:

| Need | What the build must contain | Typical per hardware |
| --- | --- | --- |
| Wi-Fi | cfg80211/mac80211, the vendor drivers, wpa_supplicant, the firmware packages | Pi: `brcmfmac` and Broadcom/Cypress firmware. Intel PC: `iwlwifi`/`iwlmvm`. AMD PC: Realtek, MediaTek, Qualcomm, Intel M.2 cards too. Add USB-adapter drivers (rtw88/89, mt76, ath9k_htc) everywhere. |
| Bluetooth | `CONFIG_BT` plus transport (`btusb`; Pi onboard uses UART/`hci_uart` with `btbcm`), vendor helpers (btintel, btrtl, btmtk, btbcm), BlueZ, firmware | Pi: Broadcom patch files. PCs: Intel `ibt-*`, Realtek `rtl_bt`, MediaTek, Qualcomm. |
| Display | KMS driver and its firmware, Mesa with the matching Gallium driver, GBM/EGL/GLES, libinput | Pi 4/5 `v3d`; Pi 3 and Zero 2 W `vc4`; Intel `i915` + `iris`; AMD `amdgpu` + `radeonsi` (needs the `amdgpu` firmware set, large). Mesa llvmpipe is the software fallback. |
| Sound | ALSA userspace, `CONFIG_SND` with the codec/HDMI/USB-audio drivers the hardware uses | Pi: `snd_bcm2835`/HDMI via device tree. PCs: `snd_hda_intel`, `snd_usb_audio`, SOF firmware on newer Intel. |
| Ethernet | The NIC driver and its firmware | `r8169` and `rtl_nic/*` on many PCs, `bcmgenet` on Pi 4/5. |
| Late devices | `agx-hwload --watch` started from `S11modules` | PCs especially; USB radios enumerate after the first scan. |

Check what the machine actually has before choosing: on a running Linux machine
`lspci -nn`, `lsusb`, `/sys/class/*`, `dmesg | grep -i firmware`; the Drivers page shows the same.
A wide Wi-Fi/Bluetooth firmware set costs hundreds of megabytes; size the root partition for it.

How the shared pieces work on every board (Pi and PC alike):
- `agx-hwload` and `S11modules` live in the **shared** overlay (`buildroot/board/agx/rootfs-overlay`),
  so every target loads drivers from device aliases at boot and keeps watching for late devices.
- The Drivers and Display pages find hardware through bus scans (PCI, USB) **and** kernel classes
  (`/sys/class/drm`, `net` wireless, `bluetooth`, `sound`), so SoC devices such as a Pi's graphics,
  onboard Wi-Fi and onboard Bluetooth are listed too. Test a new board by running the page on it.

Known gaps to close before a target is called complete:
- The generic `x86-desktop`, `arm-desktop` and `rpizero2w` defconfigs have **no ALSA, PulseAudio or
  SBC packages**, so sound and Bluetooth speakers do not work there. The Mini S, Pi 4 and Pi 5
  have them. A Bluetooth speaker error "No more profiles to connect to" means the audio stack is
  missing from the image.
- There is no Pi 3 board, no AMD-specific board and no AMD GPU firmware set in a defconfig.
- The Drivers and Display pages cannot repair a built-in driver whose firmware loads before the
  file system exists (for example Intel graphics on the Mini S); that needs the firmware embedded
  in the image.
- Secure Boot is not supported on UEFI targets (unsigned loader); the person must turn it off.

## 1. Test the change before building

The workstation may have no Rust toolchain. Test on the build machine in a **separate copy**, so a
running image build is not disturbed:

```bash
rsync -a --exclude target --exclude 'output-*' --exclude .git --exclude .cache --exclude dl ./ "$BUILD_HOST":angi-build/dev/
ssh "$BUILD_HOST" 'cd ~/angi-build/dev/crates/agx-gui && export PATH=$HOME/.cargo/bin:$PATH \
  CARGO_TARGET_DIR=$HOME/angi-build/check/target-test \
  RUSTFLAGS="-C link-arg=-fuse-ld=bfd -C link-arg=-L/usr/lib/x86_64-linux-gnu" && cargo test'
```

- `agx-gui` is not in the root workspace: run cargo from `crates/agx-gui`, not with `-p agx-gui`.
- The linker flags are needed on that machine (`/lib64/libm.so.6` is missing otherwise).
- **Do not run `cargo fmt` on the whole crate.** It reformats unrelated files. Format only files you
  wrote, or none, and never rsync the build copy back over the working tree.
- Adding Settings pages grows the sidebar. A test that clicks a fixed position can start hitting a
  nav item; fix the test, not the page.

## 2. Build on the remote machine

Reach it with the SSH host the user names (Tailscale SSH can ask for browser re-authentication; a
silent hang means it is waiting, so ask the user to approve the link it prints, then retry).

```bash
rsync -a --exclude target --exclude 'output-*' --exclude .git --exclude .cache --exclude dl ./ "$BUILD_HOST":angi-build/src/
scp .agents/skills/angi-build-and-flash-usb/scripts/remote-build.sh "$BUILD_HOST":angi-build/
ssh -f "$BUILD_HOST" 'cd ~/angi-build && BOARD=<board-id> AGX_DRIVER_FEED_DIR=<image-config-dir> nohup bash remote-build.sh > build.log 2>&1 < /dev/null &'
```

- Detach it (`ssh -f`, `nohup`, redirected stdio) or the local call hangs for the whole build.
- It builds twice: a **personal** image and a **public** one (`AGX_GENERIC_ISO=1`: no keys, root
  locked). The second pass only relinks, so it is quick.
- Finished when the log ends with `ALLDONE`. Check `STAGE=` and `PASS:` lines. Do not grep the log
  for "Error": WebKit file names contain it. Do not wait with `pgrep -f` over SSH: it matches
  itself. Wait on the log's last line instead.
- **Buildroot does not notice changed options.** After editing a kernel fragment run
  `make -C .cache/src/buildroot-* O=<output> BR2_EXTERNAL=<repo>/buildroot linux-dirclean`; after
  changing firmware packages run `linux-firmware-dirclean`; after changing a package's
  `Config.in` or defconfig, `make configure` (the script does this) is enough. GUI and web code
  rebuild on every `make build`.
- First build of WebKit and LLVM/Mesa takes hours; later builds take minutes.

## 3. Verify inside the image, not just the log

On the build machine look at `output-<board>/target` for the things you changed (for example
`grep -c "text from the new page" target/usr/bin/agx-gui`, a script's contents, the kernel
`.config`), and for firmware under `target/lib/firmware`. Confirm both `check-image` passes and
that the public image reports no login key, no SSH host key and a locked root account.

## 4. Bring the images down and personalize

```bash
rsync -a --info=progress2 "$BUILD_HOST":angi-build/out/ ~/angi-install/new/
sha256sum ~/angi-install/new/*/disk.img     # compare with the remote sums
```

Personalize on the build machine, where the full `output/images` and host tools exist. It needs
`dropbearkey`, which that machine may lack: copy the workstation's, `chmod +x` it and pass an
absolute path.

```bash
scp /usr/bin/dropbearkey "$BUILD_HOST":angi-build/ && ssh "$BUILD_HOST" chmod +x angi-build/dropbearkey
ssh "$BUILD_HOST" 'cd ~/angi-build/src && python3 scripts/provision-sd.py \
  --images output-<board>/images --public-key ~/angi-build/workstation.pub \
  --dropbearkey $HOME/angi-build/dropbearkey --output-dir ~/angi-build/prov'
rsync -a "$BUILD_HOST":angi-build/prov/angi-<board>-install-XXXX ~/angi-install/private/
~/angi-install/private/angi-<board>-install-XXXX/flash.sh --device /dev/sdX --check   # nothing is written
```

The install directory holds `disk.img` or `sdcard.img` (your key, a fresh SSH host key, a unique
root partition UUID), `known_hosts`, `board.txt`, the receipt `install.json` and **`flash.sh`**.
Keep it private.

`flash.sh` is this build's own flasher: `provision-sd.py` copies `scripts/flash-sd.sh` into every
install directory. Run from there, it needs no `--board` or `--image`. It takes the board, image and
SSH trust record from the directory's receipt and checks the image's SHA-256, the receipt and the
single pinned host key itself, with **no board registry entry**, so it works for any hardware,
including one that exists only in the build machine's tree. **Always flash an install directory with
its own `flash.sh`.** Never point the repository's `scripts/flash-sd.sh --board <id>` at a personalized
image: a board the workstation checkout does not list is refused, and the registry path cannot tell
which build the directory came from. For a directory made before `flash.sh` existed, copy
`scripts/flash-sd.sh` into it as `flash.sh` and `chmod 700` it.

## 5. Write a USB stick or SD card (the user runs this; it needs sudo)

For a generic ANGI image, use the repository flasher with the board ID and
absolute image path. Verify the checksum for the uncompressed image. Identify
the whole removable device with `scripts/flash-sd.sh --list`, inspect it with
`--device DEVICE --inspect`, and show its path, size, model, serial, contents
and identity. Run the command below with `--check` in place of
`--confirm-erase DEVICE` to verify the image and the target without writing.
Re-inspect immediately before handing over the write command. If the device
or identity changes, repeat the checks with the new identity.

When the user asks for the complete command, give it as one copyable line
with every argument filled in, including the quoted identity. Warn that
running it erases the whole device. Give the command for the user to run.
Agent execution requires separate explicit erase approval for the inspected
device.

```bash
/absolute/path/to/ANGI/scripts/flash-sd.sh --board BOARD --image '/absolute/path/to/disk.img' --device /dev/sdX --expect-identity 'Identity value from --inspect' --confirm-erase /dev/sdX
```

For a personalized install directory, use its own flasher as described below.

1. `lsblk -o NAME,SIZE,MODEL,SERIAL,TRAN,FSTYPE,LABEL`: find the stick. If it is missing it is
   probably still in the target machine; ask the user to plug it in.
2. `<install-dir>/flash.sh --device /dev/sdX --inspect`. Show the user the device, size, model,
   serial, current contents and the `Identity:` line. The last field of the identity changes between
   plugs, so **re-inspect every time**. Stop and ask again if the serial, size or model differ from
   what the user approved. Run `--check` as well; it verifies the build and the stick without writing.
3. Get explicit erase approval for that exact device (earlier approval does not carry over).
4. Re-inspect immediately before handing over the command, then write it to a **one-line launcher**
   and give the user only that path. Pasting a multi-line command with backslashes into a terminal
   breaks easily, and you cannot enter their sudo password:

```bash
printf '%s\n' '#!/usr/bin/env bash' \
  "exec ~/angi-install/private/angi-<board>-install-XXXX/flash.sh --device /dev/sdX --expect-identity '<Identity line>' --confirm-erase /dev/sdX" \
  > ~/angi-install/flash-<board>.sh
chmod +x ~/angi-install/flash-<board>.sh && bash -n ~/angi-install/flash-<board>.sh
```

   Tell the user to run `~/angi-install/flash-<board>.sh` in a terminal. It unmounts, writes,
   verifies every byte and ejects.
5. Afterwards you cannot read the raw device without root. Confirm from `lsblk`: the old filesystem
   is gone, the stick is ejected, and after a replug it shows the new partitions. If the stick
   disappeared and the kernel log (`journalctl -k`) shows a plain disconnect with no write, it was
   pulled, not flashed; say so and re-inspect instead of assuming success.

## 6. Shareable image

```bash
cp new/public/disk.img* ~/angi-install/public/
xz -T0 -9 -k -c ~/angi-install/public/disk.img > share/angi-<board>-disk.img.xz && xz -t share/angi-<board>-disk.img.xz
```

Publish only the `.xz` and the checksum of the **uncompressed** `disk.img`. Never share an install
directory. Say that the target is experimental and only the hardware actually tested is supported.

## 7. Test on the machine

Run the **Wi-Fi first** acceptance test on every target, in this order, and record which steps
were not tested:

1. Boots to the on-screen setup (no login prompt, no console noise that hides it).
2. The Wi-Fi step lists hotspots and connects; Settings → Wi-Fi shows the connection.
3. Settings → Drivers lists every device, none stuck at "Not ready" or "No driver" without a stated
   reason; **Find drivers** completes and its steps read sensibly.
4. Settings → Display shows the adapter, its driver and the connected screens; Settings → Bluetooth
   finds the adapter and discovers a device; Settings → Sound lists an output and plays a tone.
5. The Web app opens and loads a page over Wi-Fi.
6. Both themes look right on the new pages.

Access for diagnosis:
- **Raspberry Pi** (`rpi5`, `rpi4`, `rpizero2w`, a future Pi 3): follow `angi-pi-ethernet`
  (direct Ethernet, `root@10.78.0.2`, the documented host-key alias). The Zero 2 W also has its USB
  gadget address.
- **UEFI PC**: boot from the stick with the one-time boot menu; Secure Boot must be off. Use a
  direct Ethernet cable and the pinned `known_hosts` from the install directory with
  `StrictHostKeyChecking=yes` and the alias the board registry names. Never bypass a host-key
  mismatch.

Useful checks: `dmesg | grep -iE 'firmware|bluetooth|iwlwifi|brcm'`, `lsmod`, `/sys/class/bluetooth`,
`/sys/class/net/*/wireless`, `/sys/class/drm/*/status`, `aplay -l`, and `ps` for the services.
