# AGX repository instructions for Claude

Read and follow [AGENTS.md](AGENTS.md) and [docs/code-structure.md](docs/code-structure.md) before changing code in this repository. They are the shared source of truth for both Claude and Codex.

In particular, evaluate source files around 500 lines, split clear responsibilities around 750 lines before extending them, and keep `lib.rs`/`main.rs` focused on their entry-point roles. Prefer cohesive named modules over mechanical file splitting. Preserve public APIs, runtime behavior, and unrelated working-tree changes; validate with the existing tests.

Run `python3 scripts/check-file-sizes.py` to review the current large-file backlog. Explain any decision to retain an oversized file rather than silently growing it. Apply the shared guide's exceptions and validation requirements.

For any UI work (new apps, screens, dialogs, restyles), follow the shared [ANGI design system skill](.claude/skills/angi-design-system/SKILL.md) and [docs/design-system.md](docs/design-system.md): Pebble Line tokens and components only, checked in both the light and dark themes.

For desktop tooltips, follow the shared [ANGI desktop tooltips skill](.claude/skills/angi-desktop-tooltips/SKILL.md). Agent Angi's OS and Project toggle is the reference.

For any Pi access or control, read [.claude/skills/angi-pi-ethernet/SKILL.md](.claude/skills/angi-pi-ethernet/SKILL.md) and follow [docs/pi-ethernet.md](docs/pi-ethernet.md). This is the same skill and procedure used by Codex. Connect over direct Ethernet to `root@10.78.0.2`; `10.77.0.2` is only the recorded host-key alias for that connection. Apply the deployment authorization and access rules in `AGENTS.md`.

For trying a change on real hardware, use hot updates (`scripts/hot-update.sh`, [.claude/skills/angi-hot-update/SKILL.md](.claude/skills/angi-hot-update/SKILL.md)): send changed binaries over Ethernet and refresh the desktop. Build and flash an image only once the person likes the result.
