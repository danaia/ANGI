---
name: angi-hot-update
description: Iterate on ANGI by sending changed binaries to a running device over Ethernet and refreshing the desktop, instead of building and flashing an image. Use for every change you want to see on the real hardware while developing. Build and flash an image only once the person likes the result.
---

# Hot updates over Ethernet

The loop while developing: **edit, test locally, `scripts/hot-update.sh`, look at the device, repeat.**
Only when the person likes what they see, build one image and flash it
(`angi-build-and-flash-usb`). Do not build or flash an image to try a change.

## What it does

`scripts/hot-update.sh` sends the source to the build machine, compiles only the `agx` and
`agx-gui` Buildroot packages there (no image; the toolchain and vendored crates already exist
from an earlier build), collects the x86-64 binaries, and compares each with what runs on the
device by SHA-256. Only changed ones are sent, over strict-host-key SSH, and activated:

| Binary | How it takes effect |
| --- | --- |
| `agx-gui` (desktop, Settings, first-boot screens) | `SIGUSR1` refreshes the desktop exactly like Settings > Refresh desktop: settings saved, the new binary exec'd in the same session. The script checks it is running the new binary and stays up for 12 seconds. |
| `agx-securityd` (security service) | Tried first on a spare port with private state. Then replaced and killed so `agx-init` restarts it. SSH drops for a few seconds and returns. Verified by PID and binary hash. |
| `agx` (CLI) | Takes effect the next time it runs. |

`agx-init`, `agx-netd` and `agx-bluetoothd` need a reboot or service restart and are **not**
touched. Everything replaced is copied to `/root/hot-backup/<time>/` first, and a binary that does
not come up healthy is put back automatically. `scripts/hot-update.sh --rollback` restores the
newest backup.

## Run it

```bash
export AGX_SSH_HOST=root@10.78.0.2
export AGX_SSH_KNOWN_HOSTS=~/angi-install/private/<install-dir>/known_hosts   # strict checking, always
scripts/hot-update.sh --check      # compile, then show what would change; nothing on the device changes
scripts/hot-update.sh              # compile and apply
scripts/hot-update.sh --no-build   # apply the binaries from the last compile
```

`AGX_BUILD_HOST` (default `dana`), `AGX_BUILD_TREE` (default `angi-build/mini-s`) and `AGX_BOARD`
(default `beelink-mini-s`) pick the build machine's tree. The tree must have been built once.

## Rules

- **Identify the device first.** `10.78.0.2` has been different machines (the Pi 5, then the Beelink
  Mini S). Check `hostname`, `uname -m` and the pinned host key before sending anything. The script
  refuses a non-x86-64 build, but not a wrong machine.
- **Never close the Ethernet link this connection uses** (Settings > Security > Close, Close all
  remote access, or `security.conf`) while a hot update runs or while you are connected over it.
- Run the Rust and GUI tests locally first. A hot update is not a substitute for them.
- The scope is hot updates of ANGI binaries, with rollback. A reboot, an erase, a reflash or a
  change to the boot configuration still needs the person's explicit approval.
- When a change touches the image itself (kernel, Buildroot packages, `agx-init`, the rootfs
  overlay, the defconfig), a hot update cannot carry it. Say so, and group such changes for the
  next image build.
- Slint is compiled in: there is no live preview of `.slint` edits. The loop is a compile (about
  a minute for an incremental `agx-gui`) plus the refresh. For pure layout work, render the page
  locally to a PNG with a throwaway test and look at it before sending.
