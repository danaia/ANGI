---
name: angi-install-media
description: Select the hardware target and matching ANGI image for an SD card or USB drive. Use for first-time installation, preparing boot media from a build or release, or selecting an SDK for a specific ANGI device.
---

# Install ANGI on the selected hardware

Run repository commands from the ANGI checkout. Use [scripts/boards.json](../../../scripts/boards.json) as the source of truth for target IDs, architectures, image status, and build output directories.

## Identify the target

- Select the exact board or PC architecture ANGI will boot on. If it is unclear, run `python3 scripts/detect-board.py --list` and use the model printed on the board or the destination computer's architecture and boot mode. `--host` describes the computer running the command. A card reader alone cannot identify the destination.
- Run `python3 scripts/detect-board.py --require "$board"` with the selected ID. For an experimental target, explain its verification status and confirm the user wants to proceed. For a planned or unknown target, stop before building or writing media. A shared CPU architecture does not establish board compatibility. The `x86-desktop` entry is x86-64. A literal Intel 486 has no entry.

## Choose and verify the image

- **Build from source.** Pass the same explicit `BOARD="$board"` to `make configure`, `make dependencies`, `make build`, and `make check-image`. Read the output directory with `python3 scripts/detect-board.py --field "$board" output` or use the explicitly selected build directory. Verify that `images/board.txt` matches the target and check the corresponding `.sha256` file for the registry-selected artifact. Follow the [README install prompt](../../../README.md#get-started-with-one-prompt) for per-card SSH provisioning.
- **Use a published image.** Select the artifact explicitly published for that board and verify its matching release SHA-256 before writing it. Use the [Pi 4 / 400 hybrid ISO procedure](../../../docs/install-prompts/iso-headless.md) for `angi-rpi4.iso`. If the selected target has no published image, use the source-build route only if the board registry and user authorize it.
- List removable media and show the selected whole device's path, size, model, and contents. Get explicit erase approval for that exact device. On Linux use `scripts/flash-sd.sh` with explicit `--board`, `--image`, and `--device` arguments. Use `--personalized` for a private install artifact and retain its per-install `known_hosts` file. For an existing non-Linux release workflow, use its writer with the same verified board-specific image and read-back check. Confirm the write and ejection before handoff. Confirm the hardware model and ANGI status after first boot.

## Use the SDK only for native development

Installing from a release needs no SDK. A source image build uses the workstation Buildroot toolchain selected by `BOARD`. The optional `make agent-sdk` workflow prepares a running development device for native builds and currently takes its sysroot from the Pi 5 build at `output/staging`. Use it only for a verified ARM64 Pi 5 with a matching ANGI build. For another board, native provisioning needs a verified SDK with that board's matching sysroot and host architecture. Until that exists, use the board's workstation build when supported. See [Agent Angi development and provisioning](../../../docs/agent-swarm.md#native-development-and-administration).

## Raspberry Pi Zero 2 W

Use [the Zero 2 W build and setup guide](../../../docs/pi-zero-2w.md). Build on the workstation with `BOARD=rpizero2w`. The Zero has a separate micro-USB gadget address and host identity. The desktop uses software VNC through SSH. Verify both Pebble Line themes and 2.4 GHz Wi-Fi on the board after first boot.
