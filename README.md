<h1 align="center">
  <img src="brand/brand-mark-text-banner.svg" alt="ANGI OS" width="560">
</h1>

<p align="center"><strong>A thin operating system made for AI coding agents.<br>A small machine where agents build software, and the operating system itself.</strong></p>

| | |
| --- | --- |
| **Built for** | AI coding agents, on a Raspberry Pi 5 that costs a fraction of a developer workstation |
| **Desktop UX** | Based on [Omarchy](https://omarchy.org): Hyprland-style tiling, Omarchy's key bindings, keyboard first |
| **Base system** | The Linux kernel, assembled from source with Buildroot. Not Debian, Arch, Fedora, or any distribution |
| **Control plane** | Written in Rust: init, services, CLI, terminal dashboard, desktop |
| **Status** | Early development. The terminal dashboard runs on a real Pi 5; the desktop is built but not yet verified on the Pi's own display |

> [!NOTE]
> **Designed with Omarchy's UX.** ANGI's desktop follows the user experience of [Omarchy](https://omarchy.org), David Heinemeier Hansson's opinionated Arch Linux and Hyprland setup: dwindle tiling, the same chords (<kbd>Super</kbd> + arrows to move focus, <kbd>Super</kbd> + <kbd>Q</kbd> to close, <kbd>Super</kbd> + <kbd>1</kbd>-<kbd>5</kbd> for workspaces), and the same gaps and borders. ANGI is not built on Arch or on Omarchy. It borrows the experience, not the base system.

<table>
  <tr>
    <td width="50%"><img src="docs/images/angi-tui.png" alt="The ANGI terminal dashboard (Overview) running on a Raspberry Pi 5"></td>
    <td width="50%"><img src="docs/images/angi-desktop.png" alt="The ANGI desktop shell with tiled windows"></td>
  </tr>
  <tr>
    <td><sub><strong>Terminal dashboard</strong> (<code>agx tui</code>) on a Raspberry Pi 5, over SSH. The Pi's MAC addresses and LAN address are covered.</sub></td>
    <td><sub><strong>Desktop</strong> in a preview window on a development workstation, in a network sandbox. It has not yet been shown on the Pi's HDMI output.</sub></td>
  </tr>
</table>

**The OS that builds itself.**

Use your coding agent to install ANGI on a Raspberry Pi, then change, rebuild, and test the OS on the hardware you own.

Start with the Pi 5: inspect status and services over SSH, change a harmless system
text file, rebuild the image, and verify the result on your own hardware. Follow the
[reproducible source demo](docs/source-demo.md). This is a procedure to validate,
not a claim that the full demo has passed. Pi 4 and Zero 2 W remain experimental.

## What is ANGI?

**ANGI is a thin operating system made for one job: letting AI coding agents build software — and build the operating system itself.**

Most operating systems are made for people and later adapted for agents. ANGI starts from the other end. It is a small, fully inspectable machine whose every command, service, and log is available to an agent as plain text and JSON, running on a Raspberry Pi 5 that costs a fraction of a developer workstation. The whole system, from boot to the dashboard, lives in one repository. An agent can read it, change it, rebuild it, flash it, and verify the result. Nothing is inherited from someone else's distribution, so nothing is off-limits.

**A note on names.** The product is **ANGI** (pronounced “AN-jee”). **AGX** stays as the technical name under the hood: the `agx` command, the `agx-*` crates and services, and file paths such as `/usr/share/agx`. The GitHub repository is named ANGI. Everything you see on screen, in the terminal dashboard, and on the desktop says ANGI.

### How ANGI is different

- **It is not a Linux distribution.** ANGI is not Debian, Ubuntu, Arch, Fedora, or Alpine underneath, and it has no package manager or distribution release cycle. It is assembled from source with [Buildroot](https://buildroot.org/), which is a build tool rather than a distribution, so the image contains only what ANGI asks for. There is no distribution to be compatible with, and nothing to uninstall before you can change something.
- **The ANGI layer is written in Rust.** Startup (`agx-init`, PID 1), the service supervisor, the network and account services, the `agx` command line, the terminal dashboard, and the desktop are all Rust, about 11,000 lines in one workspace. The init program is a 0.5 MB binary.
- **Everything is scriptable.** Status, services, logs, Wi-Fi, accounts, and power each have an `agx` command with `--json` output. The terminal dashboard and the CLI use the same service interfaces, so what an agent can do is what a person can do. The desktop is meant to join them; that wiring is not finished.
- **Agents already build it.** The setup below is a single prompt: your coding agent downloads the code, builds ANGI, flashes the card, and checks that it boots.
- **It is small on purpose.** A fresh image has about 93 MB of files, boots to a reachable SSH login in about 2.5 seconds after Linux starts, and used roughly 120 MB of the Pi's 16 GB of RAM while idle. These are development measurements, not release benchmarks.

### What ANGI is built on

Honesty matters here, because "not a distribution" does not mean "from nothing." ANGI runs the **Linux kernel** (Raspberry Pi's 6.12 branch, trimmed for the Pi 5), uses the Pi's own firmware bootloader, and relies on a few small, well-known components: BusyBox for the shell and core utilities, Dropbear for SSH, `wpa_supplicant` for Wi-Fi security, and Mesa for the desktop's GPU drivers. ANGI does not rewrite these. It owns everything above them, and that is where agents do their work. A Linux kernel written in Rust is a separate, far larger project and is not part of the current plan.

### Working today and planned

| Working on a real Pi 5 | Planned |
| --- | --- |
| Bootable image built from source; SSH over USB-C, Ethernet and Wi-Fi | An ANGI agent that runs on the Pi, signs in with a ChatGPT subscription, and uses permission-controlled tools (Phase 6) |
| Rust init, service supervision, logs, confirmed reboot and shutdown | Herdr orchestration of several agents across machines |
| `agent angi`: three communicating OpenCode agents in Herdr, native Pi builds, and an administrator development skill | Native-host configuration for complete Buildroot image rebuilds |
| Terminal dashboard and `agx` CLI with JSON output | A reproducible `v0.1.0` release image, with measured boot time and resource use (Phase 7) |
| Wi-Fi, accounts, and password login provisioned from the dashboard | On-device system updates with rollback (on hold) |
| Desktop with tiling windows and graphical login (built; not yet verified on a monitor) | The desktop built into the standard image |

The development Pi includes the [ANGI agent workshop](docs/agent-swarm.md).
Run `agent angi` in Terminal or the TUI's shell, describe what to build, and
work with its orchestrator, builder and tester. GUI/CLI builds run directly
on the Pi, with source, tools and caches on NVMe. The integrated Agent screen
and subscription sign-in workflow remain separate roadmap work. The
[project roadmap](plans/plan1.md) records the broader project stages.

## Get started with one prompt

> **You need an AI agent that can run commands on your computer to do this install.** Any agent harness or IDE works: **Codex**, **Claude Code** (terminal or VS Code), **Cursor** (agent mode), **OpenCode**, **Hermes**, or any other that can run terminal commands. Open it on the computer you are installing from, then paste the prompt into its chat. A website chat such as ChatGPT in a browser tab cannot do it: it cannot run commands, build ANGI or write your card.

You paste **one prompt** into that chat. The agent runs the whole install and guides you through it; you only plug things in and move the card.

1. **Paste the prompt (on your computer).** The agent builds ANGI and writes it onto an SD card that is in a card reader on your computer. The card is formatted there, never in the Pi.
2. **Wait while it builds.** The first build takes about 1 to 3 hours. The agent stays with it, keeps checking its progress, tells you how it is going and waits until the build has finished.
3. **Move the card** when the agent tells you to: into the Pi, then connect it with Ethernet or USB-C.
4. **Continue in the same chat.** The agent verifies the Pi’s identity, connects over SSH, guides account and Wi-Fi setup, and reports any incomplete checks.

For Pi 4 / 400, the primary workflow writes `sdcard.img` with your SSH public key. On first boot, the agent connects over USB-C and opens the ANGI TUI for account and Wi-Fi setup. See the [Pi 4 / 400 provisioning guide](docs/pi4-provisioning.md).

> **The card goes in your computer first, never in the Pi.** The Pi stays off, with an empty card slot, until the agent tells you the card is finished.
>
> **This erases the SD card.** Everything on it is deleted. Use a card you do not need.

ANGI is in early development. This guide is tested on **Arch Linux + NetworkManager**, with an agent that can run terminal commands (Codex, Claude Code, Cursor, OpenCode, Hermes or any other harness or IDE).

| Your computer | What the agent prepares | How you connect (for first boot) |
| --- | --- | --- |
| **Raspberry Pi 5** | A microSD card | Ethernet or USB-C |
| **Raspberry Pi 4 / 400** | A microSD card | Ethernet or USB-C |
| **Raspberry Pi Zero 2 W** | A microSD card | [Micro-USB and VNC](docs/pi-zero-2w.md) |
| **ARM computer** (UEFI) | An SD card or USB drive you start from | Its network or screen |
| **Intel / AMD computer** (x86-64) | An SD card or USB drive you start from | Its network or screen |

Your agent asks which computer you have and builds the image for it (`make build BOARD=rpi4`). It **never writes another computer's image**, and if it cannot build something that will boot your computer it tells you plainly instead of making a card that will not.

### Step 1: Gather your parts

- [ ] Your Raspberry Pi (Zero 2 W, 4, 400 or 5), or an ARM or Intel/AMD computer. **Take the card out of the Pi first**: it cannot format its own card.
- [ ] **USB-A to USB-C data cable** (a charge-only cable will not work), *or* an Ethernet cable and the Pi's own power supply
- [ ] A microSD card you can erase (32 GB tested). A card that is already in the Pi is fine; take it out.
- [ ] A card reader plugged into your computer. The computer must see the card to format it; the Pi cannot do this for you.
- [ ] A computer running Arch Linux with internet
- [ ] An AI agent that can run commands, open on that computer. Any harness or IDE works: Codex, Claude Code, Cursor, OpenCode, Hermes and others
- [ ] Your **Wi-Fi network name, password and two-letter country code**: ANGI must join Wi-Fi during setup

You do not need a monitor or keyboard for the Pi.

### Step 2: Put the card in your computer

```text
Computer ── card reader ── SD card      (formatted here, by the agent)
Pi: off, card slot empty, not connected yet
```

Put the SD card in a card reader and plug the reader into your computer. Check that your computer sees it. Unplug other USB drives so the right one is easy to spot. If the card is inside the Pi now, take it out: the Pi cannot format its own card.

### Step 3: Paste the prompt into your agent (build ANGI and put it on the card)

<!-- prompt1:start -->
```text
INSTALL ANGI: build it and put it on an SD card, on this computer. I will tell you which computer ANGI is for.
Explain each step in one short, plain sentence. Do the terminal work yourself and
guide me through it: before each step say what you are about to do and what I will
see, tell me exactly when I need to do something physical, and wait for me. Ask me
only for missing information, approvals, or something physical. Passwords belong
in a local hidden prompt, never in chat or logs. Keep private keys on this computer.

THE PLAN (tell me this in your first message, in plain words)
  1) You check this computer and build ANGI. This is the long part: about 1 to 3 hours
     for a first build, depending on this computer's CPU and internet speed. Give me
     your own estimate once you have checked this computer.
  2) You stay with the build the whole time, keep checking its progress, and tell me
     how it is going. I can leave it running.
  3) When the build has finished, you write it to my SD card or USB drive (I confirm the erase).
  4) Guide boot and setup in this same chat.

Repository: https://github.com/danaia/ANGI.git
Use a fresh isolated checkout; preserve any existing checkout and uncommitted work.
Fetch and check out exactly 02d035cf0d5e92ce35899b16761d4b67316c9b8b (themes source baseline).
Verify git rev-parse HEAD equals that commit. Do not substitute the default branch.
If the ref cannot be fetched, is wrong, or lacks the selected target's registry,
defconfig, firmware, scripts or setup guide, STOP before building or touching media.
From that checkout, read .agents/skills/angi-install-media/SKILL.md and run the target
checks below before starting a build.

STEP 0 - ASK WHAT ANGI IS GOING ON
If I have not already said, ask me this and wait for the answer:
  1) Raspberry Pi Zero 2 W        (rpizero2w)
  2) Raspberry Pi 4 or 400        (rpi4)
  3) Raspberry Pi 5               (rpi5)
  4) ARM desktop or laptop        (arm-desktop)
  5) Intel/AMD desktop or laptop  (x86-desktop)
  6) Not sure - help me tell
If I answer 6, run scripts/detect-board.py --list and --host and show me the "how to
tell" lines (for a Pi, ask me to read the name printed on the board). Then run
scripts/detect-board.py --require BOARD and follow what it reports. If it says a
target has no image yet, stop. If a build is experimental, explain what remains
unverified and ask if I want to proceed. Use only the selected board and the
removable drive I approve.

STEP 1 - CHECK THIS COMPUTER
Check the host OS and architecture, free disk space (at least 20 GB, more if the
build requires it), memory, dependency requirements in scripts/build.sh, and that
my microSD card (or SD card or USB drive) is attached to THIS computer, in a card reader. The repository build and card writer use Linux. On macOS or Windows,
use a Linux environment that can access the physical card reader, or a separate
Linux computer. Stop on an unsupported host or without confirmed physical reader
access; do not pretend a container or VM can see the card. Confirm access before proceeding. Check the build dependencies
and use the Linux distribution's package manager to install missing tools with approval.
Use the host's network settings for the direct link in part 2.

STEP 2 - GET AND BUILD ANGI
Select or create the workstation Ed25519 key with python3 scripts/ssh_provisioning.py.
Export SSH_PUBLIC_KEY as the returned public-key path for every build command;
record its matching private-key path. Never overwrite an existing key.
As my normal user run make configure, make dependencies, make build and make
check-image, each with BOARD=<my board>. Monitor progress and stop on failure.
Resolve IMAGE_DIR from python3 scripts/detect-board.py --field <my board> output,
appending /images. Do not search for a convenient image or reuse another board's output.
Require exactly IMAGE_DIR/sdcard.img, board.txt equal to <my board>, and its matching
sdcard.img.sha256. Run sha256sum -c sdcard.img.sha256 from IMAGE_DIR after check-image.
Missing or ambiguous artifacts, wrong board stamps or bad checksums mean STOP.
Do not regenerate a checksum to make a mismatching image pass.
For rpi4: Use the validated build sdcard.img directly. Do not run provision-sd.py:
it does not support rpi4. The Pi creates its host key on first boot. Before SSH,
verify that key through the Pi's local console or an independent trusted per-device
record. If neither is available, STOP and explain the missing trust step.
Never use ssh-keyscan output alone as proof of identity.
For rpi5 or rpizero2w: For this board, ensure dropbearkey is available, then run
python3 scripts/provision-sd.py --images IMAGE_DIR --output-dir PRIVATE_PARENT
--public-key PUBLIC_KEY, with a private directory outside the checkout.
Use the returned private directory, run provision-sd.py --verify PRIVATE_DIR
--board the selected board ID and sha256sum -c sdcard.img.sha256 there. Use only its sdcard.img
and prepared known_hosts for flashing and first SSH. Stop if either check fails.
Record the resolved absolute image path, board, source commit, checksum and trust-file
path together. Keep this image private because it permits access using my key.

WATCH THE BUILD. THIS IS REQUIRED.
- Before you start make build, say: "The build takes a long time, about <your
  estimate>. I will keep watching it and checking its progress until it is finished."
- Run the build so you can watch it (in the background, writing to a log file), and
  look at the log every few minutes. Do not start it and walk away, and do not wait
  silently.
- Each time you check, tell me in one or two short lines: which stage it is on, whether
  it is still moving, and about how much time is left.
- If it stops, errors or looks stuck, stop progression and report the cause.
  Resume only after resolving it and rerunning the failed checks.
- Do not assume it worked. Wait until the build has actually finished and
  make check-image has passed, then tell me clearly: "The build is complete." Do not
  start step 3 before that.

STEP 3 - ERASE AND FORMAT THE CARD ON THIS COMPUTER (needs my OK)
The SD card or USB drive must be attached to THIS computer. Never ask me to put it in the target
computer for this step. If it is not attached here, stop and ask me to attach it.
1. Run scripts/flash-sd.sh --list and show me the SD card or USB drive: path, size, model,
   serial, and what is on it. If there are several or I might not recognize it,
   stop and ask me which. Never use a disk that holds this computer's system.
   Resolve aliases to the exact whole device and record its capacity and serial.
   If identity is missing or ambiguous, STOP; do not guess.
2. Say clearly that EVERYTHING on it will be erased. Wait for me to type the
   approval in the form "erase DEVICE", using the exact whole-device path reported
   by the Linux card writer. Anything else means no.
3. Re-list and confirm that path, capacity, model, serial and mounts still match
   the approved device immediately before writing. If anything changed, STOP and
   request fresh approval. No approval means no write, including retries.
   Then run scripts/flash-sd.sh --board <my board> --image IMAGE --device PATH --confirm-erase PATH.
   Use --personalized only for the private Pi 5 or Zero 2 W image.
   It wipes the old partitions, writes the image, checks every byte, and ejects.
   Wait for the command to finish successfully before saying the SD card or USB drive is ready.
   If it refuses, verification fails, or ejection fails, tell me. Do not try
   another disk. If a retry succeeds, tell me which attempt succeeded and have
   me close any terminal still showing the earlier failure.

STEP 4 - HAND OFF THE VERIFIED MEDIA
Tell me the SD card or USB drive is ready and what happened ("built, written, every byte checked").
Then give me these steps as a short numbered list, one physical action per line:
  1) Power off the target (a desktop: shut it down).
  2) Take the finished card out of the card reader on this computer.
  3) Put it in the target: push a microSD card into the Pi's slot, gold contacts toward the board, until it seats. A desktop: plug the drive in.
  4) Connect the Pi 4 or Pi 5 by USB-C or direct Ethernet. For the Zero 2 W, use the micro-USB port marked USB.
  5) Power on the target and allow up to 2 minutes for first boot.
Remind me to have my Wi-Fi network name, its password and my two-letter country
code ready, because the second part joins ANGI to Wi-Fi, which is the most important
step.
Continue in this chat after I insert the card and power on the target.
Read docs/install-prompts/part2-BOARD.txt using my selected board ID. Carry forward
the checkout, key and trust-file paths. Stop on boot, network or SSH failure;
never disable strict host-key checking or trust an unverified scanned key.
```
<!-- prompt1:end -->

Your agent may ask for your computer password. Type it in the secure prompt. The first build downloads a lot and takes about 1 to 3 hours. Your agent watches it for you and reports progress; you can leave it running.

### Step 4: Confirm the erase

Your agent shows you a card like this and **stops and waits**:

```text
/dev/sdb   29.1 GiB   SD32G   (partitions: boot, root)
EVERYTHING on this card will be erased. Type: erase /dev/sdb
```

Check that the size and name match your card, then type the path. If it is not your card, say **no**. You can also run this yourself:

```sh
scripts/flash-sd.sh --list                  # shows cards, changes nothing
make flash-sd BOARD=rpi5 DEVICE=/dev/sdb    # asks you to type the path first
```

### Step 5: Move the card into the Pi and connect it

When the build is done and the card is written, your agent tells you what to do. This is the first time the card goes into the Pi:

1. Power off the Pi.
2. Take the card out of the reader and push it into the Pi's slot.
3. Connect a Pi 4 or Pi 5 using **Ethernet** or a **USB-C data cable**. For the **Zero 2 W**, use the micro-USB port marked **USB**. Desktops use the boot menu to start from the drive.
4. Power the Pi on.

```text
Before:  Computer ── card reader ── SD card
After:   Computer ── Ethernet or USB-C ── Pi [SD card]
```

### Step 6: Continue in the same chat (join Wi-Fi and get the system ready)

> **ANGI must be on Wi-Fi.** The cable only carries the setup; Wi-Fi is how ANGI reaches the web. Have your **Wi-Fi network name, password and two-letter country code** ready. Setup does not finish until `agx wifi check` shows ANGI can reach the web. You type the password privately, with the screen echo off, never into the chat. (The Pi Zero 2 W supports 2.4 GHz networks only.)

Your agent continues using the checkout and verified identity from step 5. The first-boot instructions for each computer are in [docs/install-prompts/](docs/install-prompts/) (`part2-rpi5.txt`, `part2-rpi4.txt` and so on).

Within about two minutes the Pi shows up on your computer. Your agent connects, joins it to Wi-Fi with you and confirms it is online, then sets up your account and checks every service. If it does not show up:

| What you see | Try this |
| --- | --- |
| Nothing appears on your computer | Use a **data** cable. Charge-only cables only power the Pi. |
| The Pi keeps restarting or disconnecting | A USB-A port may not give enough power. Try a USB 3 port, or power the Pi with its own supply and use Ethernet. |
| No lights on the Pi | Check power. Red = power; green blinking = reading the card. |
| Green light never blinks | Push the card in all the way, or run the install again. |
| SSH says the host key changed | Expected after a reinstall. Your agent will ask before replacing the old key. |
| Wi-Fi will not connect | Check the password and the two-letter country code. A Pi Zero 2 W needs a 2.4 GHz network. Captive-portal networks (a login page) are not supported. |
| Writing the card fails | Try another card or reader. Never retry on a different disk without checking it. |

### Step 7: Use ANGI

In the dashboard:

- Press **1** for Profile, **2** for Overview, or **3** for Hardware.
- Press **4** for Network, **5** for System, **6** for Services, or **7** for Logs.
- Press **F9** to confirm power off or **F10** to confirm reboot.
- Press **?** for help.
- Click tabs and rows, or use the arrow keys and mouse wheel.
- Press **q** to return to the Pi's terminal.

When you are finished, ask your agent to shut down the Pi, or run `poweroff` in the Pi's terminal. Wait a few seconds after the connection closes before unplugging the cable.

## If you get stuck

Tell your agent what happened and ask it to continue diagnosing the setup.

- **The Pi keeps restarting or disconnecting:** the computer's USB port may provide too little power. A suitable powered data connection may be needed. See [Raspberry Pi's USB networking and power guidance](https://www.raspberrypi.com/news/usb-gadget-mode-in-raspberry-pi-os-ssh-over-usb/).
- **The agent cannot identify the card:** give it the card's capacity and reader details. Let it inspect devices again after you remove and reinsert the card.
- **The build stopped:** ask the agent to fix the error and resume. It can reuse downloaded files and completed build work.
- **The dashboard looks wrong:** ask the agent to try `agx tui --ascii --no-color --no-mouse`.

## Project status and technical details

USB login, Rust-managed startup, the dashboard, service supervision, and Wi-Fi have been tested on a real Pi 5. A graphical desktop (`agx-gui`, with graphical login on HDMI) is under development: the Pi's GPU initializes, but on-screen operation has not yet been verified on a monitor. Multi-agent orchestration (the Phase 6 agent) is not built yet. The image uses a small, fixed filesystem, so most of a 32 GB card remains unused. HDMI and serial recovery login still need physical verification.

- [Manual build, USB connection, flashing, and recovery instructions](docs/phase1.md)
- [Dashboard controls, development, and verification](docs/phase2.md)
- [Rust startup, service controls, logs, and recovery](docs/phase3.md)
- [CLI and TUI administration](docs/phase5.md)
- [System tab, update candidates, and releases](docs/system.md)
- [NVMe data volume and ChatGPT sign-in](docs/agent-sign-in.md)
- [Desktop (GUI) status and usage](docs/desktop.md)
- [Project roadmap](plans/plan1.md)
