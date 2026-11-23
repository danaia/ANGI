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

**Our mission: make building enterprise-level software unbelievably easy, starting with a Raspberry Pi.**

ANGI OS aims to turn a tiny, affordable computer into a powerful workspace where multiple AI agents work together to build sophisticated applications. The goal is to orchestrate lightweight, low-cost models so each task uses only the intelligence and resources it needs—keeping hardware requirements, running costs, and setup effort to a minimum.

From first boot to your first application, we want to make ambitious software development accessible to anyone, at a fraction of the cost of a traditional setup.

## What is ANGI?

**ANGI is a thin operating system made for one job: letting AI coding agents build software — and build the operating system itself.**

Most operating systems are made for people and later adapted for agents. ANGI starts from the other end. It is a small, fully inspectable machine whose every command, service, and log is available to an agent as plain text and JSON, running on a Raspberry Pi 5 that costs a fraction of a developer workstation. The whole system, from boot to the dashboard, lives in one repository. An agent can read it, change it, rebuild it, flash it, and verify the result. Nothing is inherited from someone else's distribution, so nothing is off-limits.

**A note on names.** The product is **ANGI** (pronounced “AN-jee”). **AGX** stays as the technical name under the hood: the `agx` command, the `agx-*` crates and services, and file paths such as `/usr/share/agx`. The GitHub repository is named ANGI. Everything you see on screen, in the terminal dashboard, and on the desktop says ANGI.

### How ANGI is different

- **It is not a Linux distribution.** ANGI is not Debian, Ubuntu, Arch, Fedora, or Alpine underneath, and it has no package manager or distribution release cycle. It is assembled from source with [Buildroot](https://buildroot.org/), which is a build tool rather than a distribution, so the image contains only what ANGI asks for. There is no distribution to be compatible with, and nothing to uninstall before you can change something.
- **The ANGI layer is written in Rust.** Startup (`agx-init`, PID 1), the service supervisor, the network and account services, the `agx` command line, the terminal dashboard, and the desktop are all Rust, about 11,000 lines in one workspace. The init program is a 0.5 MB binary.
- **Everything is scriptable.** Status, services, logs, Wi-Fi, accounts, and power each have an `agx` command with `--json` output. The terminal dashboard and the CLI use the same service interfaces, so what an agent can do is what a person can do. The desktop is meant to join them; that wiring is not finished.
- **Agents already build it.** The setup below is a single prompt: your coding agent downloads the code, builds ANGI, flashes the card, and checks that it boots.
- **It is small on purpose.** A fresh image has about 120 MB (Pi Zero 2 W) to 650 MB (Pi 5) of files once fully deployed, boots to a reachable SSH login in about 2.5 seconds after Linux starts, and used roughly 120 MB of the Pi's 16 GB of RAM while idle. These are development measurements, not release benchmarks.

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

Choose your board on the [install page](website/download.html), select how
you want it to boot, and paste the prompt into a coding agent that can run
commands on your computer. The agent builds ANGI locally in Linux, including
a local Linux VM on macOS or Windows or WSL 2 with access to the hardware.
Allow time for the first build and have a spare card or drive ready.

| Target | Boot options |
| --- | --- |
| Pi Zero 2 W | Onboard microSD. Headless setup over the micro-USB port marked USB. |
| Pi 4 and Pi 5 | Onboard microSD, USB stick, or SD card in a USB reader. |
| ARM64, AMD x86-64, Intel x86-64 | USB stick or SD card in a USB reader on compatible 64-bit UEFI hardware. |

The agent verifies the locally built boot artifact, prepares a per-install SSH
identity, and waits for approval to erase the exact removable device. Move it
to the target after the agent verifies and ejects it. On a UEFI client, select
the USB stick or card reader in the external boot menu. ANGI runs from that
media and leaves the internal disk alone.

Follow the steps in the same chat. The agent checks the target over strict SSH
using the prepared identity. Enter your administrator and network passwords
directly in ANGI. The Pi images use the Pebble first-boot wizard on a local
display or over VNC through SSH. The [first-boot guides](docs/install-prompts/)
cover the physical links and network checks.

A [direct Ethernet installer](docs/install-prompts/ethernet.md) is experimental
on compatible Pi 4, Pi 5 and UEFI targets. It requires separate approval to erase
the target disk. Install successes are recorded against the tested hardware and
transport, independently from artifact and write verification.

<!-- prompt1:start -->
```text
Help me install ANGI from source. Ask me whether the target is a Pi Zero 2 W,
Pi 4, Pi 5, ARM64 UEFI, AMD x86-64 UEFI, or Intel x86-64 UEFI.
Use the matching profile and deployment method in website/prompts.js. Follow the
generated prompt, including its pinned source commit, exact-device erase
approval, SHA-256 verification, and trusted first boot.
```
<!-- prompt1:end -->

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
