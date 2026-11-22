# AGX repository instructions for Codex

Read [docs/code-structure.md](docs/code-structure.md) before extending or refactoring source files. Apply it to Rust, Slint, and supporting code throughout this repository.

- Evaluate a file's responsibilities before adding more code. At roughly 500 lines, review its boundaries; at roughly 750 lines, split clear responsibilities before extending it, or explain why keeping it cohesive is better.
- Keep `lib.rs` and `main.rs` focused on exports, configuration, and startup. Aim for fewer than 150 lines in library entry points; put implementation in named modules.
- Extract by responsibility, such as models, parsing, transport, state, rendering, input, or lifecycle. Do not create arbitrary numbered parts, catch-all helpers, or trivial forwarding modules just to meet a line count.
- Preserve public APIs and runtime behavior during structural refactors. Keep imports explicit and visibility as narrow as practical. Preserve unrelated working-tree changes.
- Use existing behavior tests to validate a move. Run the relevant workspace tests and standalone GUI checks when shared APIs change. Do not add tests that merely check module names or file lengths.
- Run `python3 scripts/check-file-sizes.py` to identify files needing review. Existing large files are a staged backlog, not a reason to refactor unrelated subsystems during a focused change.

These are repository-scoped instructions. Keep the shared guide current when module responsibilities change.

## Product name: ANGI (technical name: AGX)

The public product name is **ANGI** (pronounced “AN-jee”). Anything a person sees must say ANGI: README and public docs prose, the TUI and GUI text, window titles, login and shutdown messages, `/etc/issue`, the login splash, and the ASCII logo in `brand/logo-ascii.md` (white "ANG", lime "I").

Keep **AGX** for technical identifiers: the `agx` command and its help text, `agx-*` crates and binaries, service and group names, file paths and environment variables (`/usr/share/agx`, `AGX_*`), Buildroot package names, (the GitHub repository itself is now `danaia/ANGI`). Do not rename identifiers when changing visible text, and do not change visible text to an identifier. When adding a user-visible string, write ANGI.


## Omarchy reference

When a task references Omarchy or adapts one of its features or packages for ANGI,
consult the Omarchy skill for installed-system guidance. Check the upstream
[Omarchy repository](https://github.com/omacom/omarchy) and, for package choices,
[Omarchy Packages](https://github.com/omacom/omarchy-pkgs). Apply ANGI’s own
design and deployment rules to the implementation.

## Design system (Pebble Line)

Everything a person sees in ANGI uses the Pebble Line design system, in both the light and dark themes. Before designing or changing any screen, app, dialog or web UI, read [docs/design-system.md](docs/design-system.md) and the shared [ANGI design system skill](.agents/skills/angi-design-system/SKILL.md). Use `Theme` tokens and the `pebble/pebble.slint` components; never hard-code colours or use std-widgets controls.

## Desktop tooltips

Use the Agent Angi OS and Project tooltip pattern throughout the ANGI desktop. Read the shared [ANGI desktop tooltips skill](.agents/skills/angi-desktop-tooltips/SKILL.md) when adding or changing a tooltip.

## Development device deployment

The user grants Codex standing administrator authorization on the established ANGI development Pi 5. For requested changes, use the documented root SSH connection to install live component and operating-system updates, refresh the authenticated desktop or reboot when needed, and verify the running version and requested behavior. Carry deployment through verification without asking again for permission to perform those steps. Preserve user data and the previous release for rollback. An erase or reflash requires separate explicit authorization.

## ANGI installation media

Read the shared [ANGI installation media skill](.agents/skills/angi-install-media/SKILL.md) before selecting a hardware target, preparing an SD card or USB drive, or choosing a development SDK for an ANGI device.

## Pi access and control (Claude and Codex)

For Pi access, remote commands, file transfer, desktop control, deployment or recovery, read and follow [.agents/skills/angi-pi-ethernet/SKILL.md](.agents/skills/angi-pi-ethernet/SKILL.md) and [docs/pi-ethernet.md](docs/pi-ethernet.md) before connecting. These are the shared procedure for both agents; Claude discovers the same skill through `.claude/skills/angi-pi-ethernet`.

- Use direct RJ45 Ethernet: workstation `10.78.0.1/24` on `enp1s0`, Pi `root@10.78.0.2` on `eth0`. USB `10.77.0.2` is the recorded SSH host-key alias, not the Ethernet destination. Wi-Fi is an explicit recovery alternative.
- Use the existing Ed25519 key and strict host-key checking. Set the documented `AGX_SSH_*` variables before deployment; older scripts otherwise default to USB.
- Control the desktop through the existing OpenSSH VNC tunnel at `127.0.0.1:5941`. Keep Pi VNC bound to loopback and Remmina's built-in SSH tunnel disabled.
- Distinguish workstation commands from Pi commands. The Pi runs Buildroot with Rust init, not systemd; workstation tunnel services use `systemctl --user` only on the workstation.
- Verify the route and SSH identity before mutation. After a reflash or host-key rotation, follow the guide to verify the replacement key against the device console or a trusted per-device provisioning record. Once verified, update the development Pi's known-host entries and continue the authorized work. Follow the guide's bounded recovery sequence on failure.
