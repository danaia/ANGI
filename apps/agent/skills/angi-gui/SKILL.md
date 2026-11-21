---
name: angi-gui
description: Change the ANGI desktop (agx-gui, Rust and Slint) on its Raspberry Pi quickly and install it safely with angi-update, without reverting newer releases.
---

# Changing the ANGI desktop

The desktop is `crates/agx-gui` in the source checkout `/var/lib/agx-data/angi-dev`:
Rust in `src/`, Slint UI in `ui/`. Read the checkout's `AGENTS.md` and
`docs/code-structure.md` before editing (file-size and module rules). User-visible text says
ANGI; technical names stay `agx`.

## Design rules (Pebble Line)

Every screen and app in ANGI uses the Pebble Line design system. Before you design or change any UI,
read `docs/design-system.md` and `.agents/skills/angi-design-system/SKILL.md` in the checkout, then:

- Use only `Theme` tokens (`ui/pebble/tokens.slint`) and the components in `ui/pebble/pebble.slint`
  (`Button`, `TextField`, `Select`, `SegmentedControl`, `Slider`, `Switch`, `Badge`, `Progress`, `Card`,
  `Dialog`, `Icon`, the type components and so on). No hex colours, no std-widgets controls (apart from
  `ScrollView` and `ListView`), no Nerd Font icons in chrome, no font weight 700.
- Keep it flat: outlined `surface` or a filled shape, never both. Use pills for controls, `radius-lg`
  for cards and `radius-xl` for dialogs. The single primary action is solid `accent`.
- The person chooses Light or Dark in Settings › Appearance. Your screen must work in both; check
  it in both over VNC before you report it done.
- Web apps that ANGI serves use `brand/pebble-line/tokens.css` and `brand/pebble-line/web/`.
- Finish with the checklist at the end of `docs/design-system.md`.

For desktop tooltips, read `.agents/skills/angi-desktop-tooltips/SKILL.md` in the
checkout. Use its shared Slint components from Agent Angi's OS and Project toggle.

## The loop

1. Edit the source in the checkout. Keep changes small and reviewable.
2. Run `angi-build gui-test` when you changed Rust logic (skip it for pure layout or text).
3. Run `angi-update gui`. It builds the release desktop (incremental, so repeat runs are fast),
   records the release with its exact source, and switches `/usr/bin/agx-gui` to it atomically.
   It prints the new and the previous release.
4. Write your report for the user now (what changed, the release installed, what you could not
   verify), then run `angi-update refresh` as your **last action**. It loads the new release into
   the running desktop three seconds later, which closes ANGI's terminals and browser sessions,
   including this one, so nothing you do after it will be seen. Never restart the desktop any other
   way (no `kill`, `killall` or reboot).
5. The user reopens Agent Angi and resumes (`agent angi`, then resume) to continue. After a refresh,
   `angi-update status` shows the release now running and how the refresh went; check it, and
   `tail /run/agx/gui.log`. If the new desktop fails to stay up, `angi-update` restores the
   previous release by itself; use `angi-update rollback` plus another refresh if it merely misbehaves.
6. If `angi-update refresh` refuses because the running desktop predates automatic refresh, do not
   work around it: tell the user to choose **Settings > Refresh desktop...** once. Later releases can
   refresh themselves.

## Rules that keep work from being lost

- Install only with `angi-update`. Never link, copy or move files in `/usr/bin` or
  `/var/lib/agx-data/agx-gui`, never run deploy scripts, and never edit
  `/var/lib/agx-data/agx-gui/ledger.json` or `.git/angi-base`.
- If `angi-update` refuses (for example, "this checkout is based on release X but release Y is
  installed"), stop. Give the user its message. A newer release exists that this checkout does not
  contain; the user fixes that with `make deploy-gui` from the workstation. Do not try to get
  around the refusal.
- Never run `git reset`, `git checkout`, `git stash`, `git clean` or replace the checkout's files
  wholesale. The checkout holds the desktop's current source. Each installed release's exact source
  is also kept as `refs/angi/<release>` for recovery.
- Do not run `cargo clean` or start two builds at once; both slow everything down.
- Finish all your edits before running `angi-update gui`; it refuses a build whose source changed
  while it ran. A build can take two minutes or more, so give the command a timeout of at least ten
  minutes and never interrupt it: a build cut off partway damages the incremental cache. If the
  linker reports undefined references, `angi-update` discards the cache and retries once by itself.

## Verify and report

Say what you changed, the release `angi-update` installed, and what you checked. The desktop can
be checked over VNC; behavior that depends on the physical HDMI display, keyboard or mouse is
untested unless the user confirms it.
