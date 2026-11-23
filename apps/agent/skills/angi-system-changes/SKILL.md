---
name: angi-system-changes
description: Choose the smallest build and deployment for ANGI settings, agent tooling, desktop code, services, and base-system changes.
---

# Change ANGI by component

1. **Settings or behavior the installed system already exposes.** Use the
   existing setting or command first. Examples include Settings → Key Bindings,
   appearance, and mouse preferences. Check the result in the affected session.
   These changes need no Cargo build.
2. **Agent skills, profiles, or Python tools.** Run the relevant agent checks.
   The workstation's `make deploy-agent` updates the installed agent runtime
   without building the desktop. Start a new agent run to verify a changed
   profile or skill.
3. **Desktop Rust or Slint.** Read the angi-gui skill. On a device with the
   matching native SDK, finish editing and run `angi-update gui` once. It already
   compiles the desktop. From the workstation, use `make deploy-gui-only BOARD=…`
   when the device already has a compatible desktop runtime and source baseline.
   Save your report before `angi-update refresh` closes the current terminal.
   Verify the running release and the changed behavior after refresh.
4. **An individual app or service.** Find its existing install or deployment
   command and restart only the affected component. Read angi-development for
   the device's toolchain and deployment procedures.
5. **Kernel, boot, or shared base-system dependencies.** Use the board's
   Buildroot workflow. Read angi-development and the repository's system and
   board docs before changing boot or base-system files.

If the source changes both the desktop and a runtime dependency, use the
deployment that covers both. Keep edits and verification confined to the
requested change. Report the component, checks, and any activation still pending.
