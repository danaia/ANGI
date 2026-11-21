# Code structure and file-size review

Codex reads the repository's `AGENTS.md`; Claude is directed to the same instructions by `CLAUDE.md`. These rules apply to work in this repository, including IDE sessions.

## Review thresholds

Line counts are review signals, not architectural goals. Use roughly 500 lines to trigger an evaluation and roughly 750 lines to trigger segmentation when a file contains separable responsibilities. Library entry points should normally stay below 150 lines. Evaluate large executable entry points too: they should wire modules together rather than own UI state, rendering, and input handling.

Before extending a file at these thresholds, identify what it owns and which responsibilities can be moved behind a stable interface. A cohesive algorithm, generated file, protocol definition, or self-contained test fixture can justify a larger file. Record that reasoning in the change summary; do not split tightly coupled logic into arbitrary pieces just to reduce its line count. Generated/vendor/build outputs are not hand-refactoring targets.

Existing large files should be evaluated when touched, rather than forcing unrelated cleanup into every change. `python3 scripts/check-file-sizes.py` reports the current review backlog without failing a build merely because existing files are large.

## How to segment

1. Separate actual responsibilities: public models, parsing/validation, transport, application state, rendering, input dispatch, terminal/platform lifecycle, and background collection.
2. Keep `lib.rs` as a small public facade with explicit re-exports. Preserve existing public paths and signatures unless the task requests an API change.
3. Use child modules for related implementation that shares private state. Limit visibility to the parent or crate as needed; avoid widening the public API to make a move compile.
4. Import dependencies explicitly. Do not rely on a parent module's incidental private imports or introduce circular dependencies between modules.
5. Keep focused tests near their domain, or use a dedicated test module for cross-module behavior. Retain behavior assertions rather than replacing them with file-layout checks.
6. Separate refactoring from behavior changes. Preserve existing input, rendering, authentication, shutdown, and recovery behavior, including changes already present in the working tree.

## Current library boundaries

| Crate | Responsibilities |
| --- | --- |
| `agx-core` | `models.rs` owns snapshots and counters; `parsers.rs` owns pure counter parsing; `sampler.rs` owns Linux collection; `vnc.rs` observes VNC TCP sockets and GUI resident memory without connecting or controlling services. |
| `agx-net` | `protocol.rs` owns wire models, `client.rs` owns socket requests, `ssid.rs` owns byte encoding and scan parsing, `credentials.rs` owns validation and PSK derivation, `service.rs` owns the network service, and `service/clock.rs` saves network-synchronized time to the hardware clock. `setup/` owns the USB-only first-boot listener, its HTTP transport and the claim workflow. |
| `agx-gui` (standalone) | `graphics/detection.rs` discovers connected HDMI outputs at startup and rejects kernel-forced connections without monitor EDID; `graphics.rs` resolves automatic or explicit renderer selection before threads start. `graphics/kms_input.rs` bridges physical Super keys and native scroll events missing from Slint’s LinuxKMS mapping into the current login/desktop window, tracking native pointer position for scroll targeting. `graphics/virtual_display.rs` owns the optional resizable VNC-only software platform and its event loop. `remote/` owns optional VNC access: `rfb.rs` is the pure wire protocol, `resize.rs` validates single-screen resize requests and encodes layout replies, `frame.rs` captured pixels and change detection, `input.rs` key/pointer translation into Slint events, `server.rs` shared state, listeners and client lifecycle, `server/session.rs` established sessions, framebuffer streaming and message dispatch, `server/tests.rs` live socket and request scheduling tests, `handoff.rs` passing the listener and viewers from the login screen to the desktop across the exec, `mod.rs` configuration, UI-thread capture and resize callbacks. `wm.rs` and `login.rs` retain their existing composition and authentication responsibilities. |
| `agx-tui` | `app/mod.rs` owns state/navigation; `app/render.rs` owns chrome/composition; `app/screens.rs` owns built-in read-only screens; `runtime.rs` owns the event loop; `terminal.rs` owns terminal cleanup/signals and suspend/resume; `shell.rs` owns foreground process groups, shell/tmux launch and restoring terminal ownership/modes; `app/shell.rs` renders the Terminal launch screen; `worker.rs` owns sampling; `palette.rs` owns shared styling; `remote_access.rs` composes the shared VNC status panel on Network and Services. Feature screens retain their own modules. |

The desktop greeter uses `ui/login.slint` for sign-in and `ui/first-boot.slint` for first-run setup. `login/setup.rs` gates setup on the administrator account and completion marker, and `login/setup/wifi.rs` waits for the system Wi-Fi service off the UI thread. The greeter and desktop Settings share the Bluetooth page and socket client. The profile service records the completion marker when it creates an administrator, so the desktop, USB browser and TUI can all finish account setup.

The GUI launcher has its own boundaries: `src/launcher.rs` owns the three tabs (Settings for built-in tools, Apps, Commands), search ranking, secondary commands and model updates; `src/launcher/apps.rs` loads and validates the index of ANGI-compatible command-line tools and servers from `apps/agent/apps.json` (the same file the installer reads), `src/launcher/packages.rs` turns each install state into a row, arranges the Apps tab (installed first, headed sections) and owns choosing, launching and removing an app through `Cmd::Package`, and `src/launcher/tests.rs` holds the search and row-state tests; `src/library/` owns installs: `mod.rs` is the shared live state (installed versions, jobs, outcomes) and the UI timer, `worker.rs` runs `angi-update app …` on a worker thread and folds its JSON events into that state, and `tests.rs` checks the transitions including a real stand-in process. `ui/launcher.slint` owns the responsive overlay, keyboard navigation and scrolling; `ui/icons/apps/` contains its embedded vector icons. `main.rs` calls the launcher methods without owning their implementation.

Desktop gestures live in `src/input.rs`, including keyboard routing, drag lifecycle and title-bar double-click recognition. `wm.rs` retains the desktop model, workspace membership and scene composition; `wm/startup.rs` owns the initial panel composition and windowed layout defaults; `wm/tiling.rs` owns split-tree algorithms, `wm/scene.rs` spatial queries and hit testing, `wm/windowed.rs` floating geometry, collapse/restore and layout transitions, and `wm/tests.rs` the model behavior suite. `wm.rs` remains on the review backlog for its existing composition/model role; desktop state and startup wiring now have the named boundaries documented below.

Some feature/service files remain large and are candidates for later focused passes. This library refactor does not claim to have segmented the entire repository.

`scripts/trace-gui.py` owns workstation-side diagnostic collection, bounded log storage, SSH reconnection, and collector cleanup. It reads the Pi's existing logs and process state without changing GUI or system configuration.

`scripts/flash-sd.sh` owns workstation-side card identification, erase confirmation, and the locked write, read-back and eject sequence. `scripts/check-image.py` validates Buildroot output. `scripts/iso_media.py` assembles and verifies the hybrid ISO format. `scripts/build-iso.py` packages the generic release, while `scripts/provision-iso.py` adds the installation's SSH keys to a private copy before flashing.

`scripts/sync-install-prompt.py` renders the README and website source-build
prompts and first-boot handoffs from `docs/install-prompts/` and the board
registry in `scripts/boards.json`. The website selects the generated prompt
by board ID.

The Zero 2 W defconfig selects the GUI's `headless` feature and the existing
software VNC platform. Its board overlay owns the gadget address, Wi-Fi module
options and compressed swap script. The common data-volume startup invokes
the swap script on images that ship it. Image validation checks the board's
boot files and the shared desktop binary packed into the root filesystem.

`scripts/provision-sd.py` creates the private Pi 5 and Zero 2 W SD images and SSH trust
record from validated Buildroot output. `scripts/ssh_provisioning.py` owns
workstation identity selection and creation, plus offline SSH key installation
and verification for both SD and ISO provisioning.

`agx-init/src/device_events.rs` owns desktop device permissions and the kernel-to-libudev-zero input lifecycle relay. Its `bin/agx-display-devices.rs` entry point only starts that service. The existing supervised `agx-display-access` wrapper executes it when installed, retaining the permission-only fallback for older images. Device arrival notifications are forwarded after group access is ready; keystroke data never enters this relay.

## Validation

Run the affected crates' existing tests. For moves across these shared library boundaries, use `scripts/cargo.sh test --workspace --locked -- --test-threads=1` and `scripts/cargo.sh test --manifest-path crates/agx-gui/Cargo.toml --locked`. The GUI is a standalone workspace and is not covered by the root workspace command. Review the diff for altered signatures, visibility, initialization order, and discarded working-tree edits. A pure structural refactor should not require deploying new binaries to the Pi to change product behavior.

The embedded Web app lives in `apps/web/`: `host.c` owns the WPE lifecycle, settings and user data, `platform.c` owns offscreen display/view and bounded frame presentation, `gpu.c` sends bounded DMA-BUF descriptors, `protocol.c` owns the private control channel, `dialogs.c` owns form menus, script dialogs and new-window navigation policy, `downloads.c` owns the download destination (`~/Downloads`), naming and status notices, and `storage.c` owns private NVMe cache/data selection. In the GUI, `browser/protocol.rs` owns validation/encoding and key conversion, `browser/engine.rs` owns process isolation and off-thread frame/IPC transport, `browser/input.rs` maps desktop input, `browser/storage.rs` prepares protected per-user NVMe stores from the root greeter, and `browser/mod.rs` composes lifecycle, geometry and native chrome state. `browser/bookmarks.rs` owns the bookmark bar data (Chrome `Bookmarks` JSON and Google HTML export import, the saved bar in `~/.config/agx/bookmarks.json`) and `browser/bookmarks_ui.rs` publishes it to the chrome. `ui/browser.slint` owns browser chrome and frame display; `ui/browser-popup.slint` owns native option and script popups. `app.rs` owns browser state and close-command coordination; `runtime.rs` installs its callbacks and timer. `wm.rs` only adds a window kind.

The native Web presentation bridge is isolated under `browser/gpu/`: `transport.rs` validates descriptor messages and owns received file descriptors; `egl.rs` imports DMA-BUF images and manages EGL fences; `renderer.rs` owns reusable GL textures and frame completion; `completion.rs` waits for exported native fences off the UI thread; `mod.rs` wires Slint rendering callbacks and CPU fallback diagnostics. Software/VNC presentation retains the shared-memory path. GPU resources are created and destroyed only with the Slint GL context current.

The GUI entry point now owns startup and signal handling. `app.rs` owns desktop state, command/session dispatch and layout synchronization; `runtime.rs` wires windows, feature callbacks, timers and sampling; `system_ui.rs` formats system snapshots and the optional preview palette; `pointer_tests.rs` retains the existing desktop interaction checks. `wm.rs` keeps its existing composition/model responsibilities; the terminal addition only extends its window-kind catalogue.

`cpu_temperature.rs` reads the CPU thermal sensor on the existing sampling
thread. `network_safety.rs` reads the local TCP and UDP socket tables and
ranks exposure by bind address and conventional management ports.
`ui/panels.slint` displays the network exposure score and its reason below
the CPU readings.

Native Terminal is isolated under `src/terminal/`: `session.rs` owns Alacritty's PTY/parser threads, shell startup, resize and child cleanup; `keys.rs` encodes xterm keys and bracketed paste; `render.rs` converts terminal cells to reusable styled row runs; `input.rs` handles selection, mouse reports, scrolling, terminal shortcuts and chrome hits; `mod.rs` composes sessions, tabs, search and model updates. `ui/terminal.slint` owns its native chrome and glyph presentation. `scripts/build-terminal.sh`, `package-terminal-runtime.py` and `deploy-terminal.sh` build and stage the bounded CLI runtime and locale data without replacing boot libraries.

OpenCode's vendor executable is pinned in `buildroot/package/agx-opencode/`; `apps/opencode/launch.sh` owns its private NVMe environment and preserves the caller's working directory. `scripts/build-opencode.sh`, `package-opencode-runtime.py` and `deploy-opencode.sh` handle its verified binary and Git/search runtime. The packager preserves Git's hard links and leaves OpenCode's embedded runtime unstripped. GUI launch uses a Terminal session directly; the TUI reuses its existing foreground-process handoff. The data-volume mount script prepares the sticky per-account storage parent after mounting.

The development swarm lives in `apps/agent/`: `agent.sh` owns private runtime
environment setup; `main.py` dispatches commands; `ui.py` owns the build-brief
form; `session.py` owns Herdr lifecycle and queued delivery; `messaging.py`
owns durable peer mail. Role profiles and the development skill are separate
assets. `angi-root.c` is the small administrator execution boundary;
`sdk-env.sh` and `angi-build.sh` own native build configuration. SDK preparation,
provisioning and runtime deployment are separate scripts under `scripts/`.

Desktop preferences live in `agx-gui/src/settings.rs`: validated per-account mouse settings, the shared atomic per-account file writer (`config_file`, `write_private`) that `appearance.rs` reuses, live libinput device configuration and hotplug reapplication. `ui/settings.slint` owns the responsive Settings tile content, mouse controls and test area; `ui/app.slint` hosts it in the regular window frame and exposes it through the session menu. `ui/desktop-input.slint` owns shared desktop pointer forwarding behind windows and above their read-only content, allowing native Settings controls to receive input within normal window stacking. `graphics/kms_input.rs` forwards device lifecycle events and applies the configured native scroll multiplier.

The desktop's look is the Pebble Line design system ([design-system.md](design-system.md)). `ui/pebble/` owns it for Slint: `tokens.slint` defines `Theme` (light and dark colour tokens switched by `Theme.dark`, plus space, radius, stroke, type and the chrome metrics mirrored from `wm.rs` and `main.rs`), `icons.slint` the outline icon registry over `ui/icons/pebble/`, `type.slint` the type scale and `Icon`, `actions.slint` buttons and the focus ring, `inputs.slint` text, choice and range controls, and `surfaces.slint` cards, badges, progress, menus and dialogs; `pebble.slint` re-exports them for feature files. `data.slint` re-exports `Theme` for older imports. `agx-gui/src/appearance.rs` owns the per-account light or dark choice (`~/.config/agx/appearance.conf`) and applies it to `Theme`; `ui/settings-appearance.slint` is its Settings page. Token values originate in `brand/pebble-line/tokens.json`, which also feeds `tokens.css` and the web components in `brand/pebble-line/web/`.

`agx-gui/src/open_request.rs` owns the terminal `open` command: the private-socket listener inside the desktop, the client (the same binary run as `open`), and `resolve()`, the one place that decides which app opens which file. `agx-gui/src/session_refresh.rs` owns authenticated desktop restart and tracks asynchronous terminal/browser cleanup jobs so refresh can finish them before exec. Normal window closure remains asynchronous. `main.rs` invokes restart only after the desktop runtime has returned and released its UI state.

The GUI file-size heat map is isolated in `src/storage/`: `scan.rs` discovers mounted SD/NVMe filesystems through mountinfo and kernel device IDs, and bounds a personal-file walk without following symlinks or counting hard links twice; `policy.rs` limits cleanup to owned, removable files in personal storage and excludes system/configuration paths; `risk.rs` estimates and explains removal risk from location/type independently of size; `delete.rs` pins parent directories and verifies the confirmed file before removal; `worker.rs` runs scanning and deletion independently and publishes deletion completion before rescanning; `mod.rs` wires UI callbacks; `view.rs` owns persistent tile models, stable cleanup batches and explicit refresh epochs, preserving removed-path exclusions across manual refreshes for the desktop session, even if an app recreates them; `tests.rs` exercises mount discovery, file sizing and scan limits. `ui/storage.slint` owns the size-colored squares and file selection; `ui/storage-dialog.slint` owns details, keyboard navigation and the explicit delete confirmation. `ui/panels.slint` composes the fixed overview header, scrollbar-free drive-map viewport and fixed bottom-edge hover readout. The overview sits above the shared gesture layer so the maps receive hover events; title bars and borders retain desktop gestures.

Workspace 1's bulk cache cleanup uses `src/storage/cache.rs` for its separate, bounded allowlist and SHA-512 verification, `cache_dialog.rs` for preview and confirmation state, and `ui/cache-dialog.slint` for the modal. Bulk cleanup does not use the file tile risk estimate to authorize removal.

Agent Angi’s workspace welcome and prompt live in `ui/agent.slint`; `runtime.rs` routes submissions to the native terminal, and `apps/agent/main.py` accepts the brief and launches the existing Herdr session. A swarm outlives the terminal tab that started it, so `agent_swarm.rs` watches `/proc` off the UI thread for the account's Herdr servers, `angi-*` agents and mail-delivery workers and publishes the counts that show the prompt's "Open Herdr" banner, whose Stop button (after a confirmation) ends all of them (SIGTERM, then SIGKILL for stragglers); `terminal/herdr.rs` handles the button by focusing a tab that is already a live Herdr client (`Session::runs_herdr`) or else starting one that runs `agent attach`, which `session.py` points at the newest swarm whose server is still up.

`terminal/windows.rs` owns independent terminal window contexts and per-window UI snapshots. The window manager assigns each added terminal a stable identity; workspace 3 seeds one full-height left terminal and two stacked right terminals. Terminal sessions, tabs, search and input state remain local to each window.

The window catalogue in `wm.rs` remains cohesive: dynamic terminal identities extend its existing model role. Terminal lifecycle and UI snapshots are kept in `terminal/windows.rs`; `ui/app.slint` only hosts the per-window terminal view.

Terminal `session.rs` also reads a live child shell’s directory through `/proc`, guarded by its pidfd lifetime. `terminal/windows.rs` captures the focused tab’s directory when opening a window; session creation applies it directly as the child working directory.

Agent provider setup lives in `agent_providers.rs` (off-thread private process transport and GUI status), `ui/agent-providers.slint` (provider selection drawer, connection setup and role selectors), `apps/agent/providers.py` (authenticated loopback OpenCode service, OAuth lifecycle, Apple and LM Studio discovery, and the card overview), `apps/agent/tailnet.py` (Tailscale state, sign-in and the LM Studio relay target), and `apps/agent/models.py` (validated atomic per-role preferences and the one active service). `apps/agent/scope.py` decides what a run may touch (OS or one validated project folder) and supplies each role's scope text; the prompt's OS/Project/Chat switch is `ui/agent-scope.slint` with `agent_project.rs` connecting its folder choice to ANGI Files' picker overlay and relaying the runtime's validation result back to it. `apps/tailscale/` holds the supervised Tailscale and loopback relay scripts that `scripts/deploy-tailscale.sh` installs. The provider bridge saves the last submitted Apple address in per-account state and uses the existing OpenCode endpoint as a fallback. It updates the endpoint that runs use after a successful model lookup. While the provider drawer is open, the GUI worker checks the configured Apple endpoint's model list every five seconds. The bridge reports reachability independently of the selected provider and saved role models. Credentials stay in OpenCode’s private account store. `session.py` merges the active service's saved choices and local server configuration into new Herdr agent profiles. OS and Project use the same three Herdr roles. `solo.py` resumes older solo OS runs. `update.py` (the `angi-update` command) installs desktop releases through `releases.py`, which owns the release ledger, the source snapshots and the guard that refuses to replace a newer release with an older checkout; `make deploy-gui` calls the same module to record workstation releases and sync their source. `apps/agent/applefit.py` keeps OpenCode's requests inside an Apple Intelligence server's 8,192-token context and size limits (leaner agent profiles plus a loopback guard that trims and retries, used for that service only). Chat mode does not use Herdr either: `chat.py` starts one `angi-chat` agent with Angi's persona (`skills/angi-chat`, `profiles/chat.md`) loaded directly into its prompt. It starts the same Apple request guard as OS and Project when Apple Intelligence is active. Chat has its own private folder and tracks its OpenCode session ID across messages. `agent chat-turn` answers one message as JSON. `history.py` reads Chat sessions from OpenCode’s database and OS and Project briefs from run manifests, restores Chat folder selections saved per session, and stores history titles and removed-entry markers in per-account state. The folder picker keeps separate Project and Chat selections. In Chat, `chat_files.py` serves the four local MCP file tools in the user's home folder or an explicitly selected folder, using directory descriptors and rejecting symlinks. In the desktop, `agent_chat.rs` owns conversation state, the worker and session tracking. `work_history.py` stores agent reports and looks up older run replies and folder links. `agent_history.rs` opens the selected conversation or work record, `ui/agent-history.slint` draws the history drawer, and `ui/agent-work-summary.slint` draws the work overview and its ANGI Files links. `ui/agent-chat.slint` draws the transcript and `ui/agent-conversation.slint` owns the shared Chat, OS and Project layout, starter prompts and composer. `ui/agent-controls.slint` defines the shared Angi logo, text and buttons; `terminal/session.rs`'s `AgentRun` carries OS or Project to `agent angi`. `apps/agent/refresh.py` loads an installed release into the running desktop by signalling it (`SIGUSR1`, handled in `session_refresh.rs`) and, through a detached watchdog, restores the previous release if the new desktop does not stay up.

Work review lives in `apps/agent/work_changes.py` (bounded before/after evidence
and checked working-copy saves), `work_history.py` (summaries and matching older
OpenCode records), `work_editor.py` (desktop-owned loopback bridge) and
`work-editor/` (the three-tile Monaco view). The desktop starts the bridge lazily
when history opens a Project or OS record. `ui/agent-work-view.slint` hosts
the review inside Agent Angi and returns to the same history list.
`src/work_editor.rs` owns the review lifecycle and routes Files and refresh
commands. `src/work_editor/view.rs` owns the input and frames for its dedicated
WPE surface. The surface uses an ephemeral network session and shares the
rendering transport with Web. The agent package installs pinned Monaco assets
through `scripts/stage-monaco.sh`.

ANGI Files lives in `agx-gui/src/files/`. The filesystem layer has no UI types, so agents can share the engine later: `entry.rs` classifies entries and formats their text; `listing.rs` reads folders, searches below them and owns natural, folders-first sorting; `ops.rs` performs copy, move, rename, create and delete without ever overwriting and records reversible actions for undo/redo; `trash.rs` implements the freedesktop Trash per volume so trashing is a rename, never a copy. The browser layer: `model.rs` is the lazily formatted Slint row model with selection (only visible rows are formatted, selection notifies only changed rows); `worker.rs` runs a scan thread (listing, search, change detection) and an operations thread so long copies never delay navigation; `pane.rs` holds one browser's state, history and prompt; `hub.rs` owns the commands, clipboard, undo history and worker replies for both browsers (the Files window and the folder picker); `keys.rs` owns keyboard navigation and type-ahead; `places.rs` builds the sidebar; `mod.rs` wires Slint callbacks and an adaptive poll timer. `ui/files.slint` is the single browser component (toolbar, breadcrumbs, virtualized `ListView`, context menu, prompts) used by both the window panel and `ui/files-picker.slint`, the modal folder chooser that Agent Angi opens. `Kind::Files` in `wm.rs` only adds a window kind; terminal window identities now start after it.

`files/server.rs` owns session-scoped Python static HTTP servers: isolated process startup, loopback port selection/readiness, per-folder reuse and child cleanup on a worker thread. `files/mod.rs` polls results and opens Web through the existing desktop/browser APIs. `files/hub.rs` remains cohesive at about 500 lines as the shared browser command/state coordinator; server process logic stays outside it.

The memory process overlay uses `memory_processes.rs` for procfs snapshots, risk classification, and PID-safe signal delivery on a worker thread. `ui/memory-processes.slint` owns the sortable process list and termination confirmation. The desktop composition only opens and hosts the overlay.

Bluetooth is three small pieces around one service ([bluetooth.md](bluetooth.md)). `agx-bluetooth` is the shared, dependency-light crate: `protocol.rs` owns requests, replies and state, `address.rs` validation, `category.rs` device categories, and `client.rs` the socket client. `agx-bluetoothd` is the service: `backend.rs` is the stack boundary (`Backend`), `bluez.rs` its BlueR implementation with an event-driven device cache and the pairing agent, `prompts.rs` the one pending question for the person, `core.rs` state, authorization policy and dispatch, `peer.rs` who is asking, `rfkill.rs` the radio unblock, and `server.rs` the socket; `tests.rs` runs the policy against a fake stack. `agx-cli/src/bluetooth.rs` holds `agx bluetooth`, kept out of `main.rs`, which is already on the review backlog. In the desktop, `src/bluetooth/mod.rs` owns the worker thread and Status→UI mirroring, `view.rs` the pure wording and grouping, and `tests.rs` the checks that need the headless desktop (called from `pointer_tests.rs`, which owns the one-per-process test platform); `ui/bluetooth.slint` owns the page, and `ui/settings.slint` hosts it as a second page whose choice lives in `MouseSettings.page` because the desktop rebuilds the panel when its window moves. `apps/bluetooth/run.sh` supervises D-Bus, BlueZ and the service as one `agx-init` service.

Web tabs share a single sandboxed WPE context and network session. `apps/web/tabs.c` owns the views and active-tab metadata, `platform.c` tracks which view owns each frame until acknowledgement, and `browser/tabs.rs` owns the desktop tab commands and strip state. Workspace defaults stay in `wm/startup.rs`, which opens the dashboard on Workspace 1 and Agent Angi on Workspace 2.

Services and update UI lives in `agx-gui/src/services.rs` (off-thread updater
commands and model publication) and `ui/services.slint` (update-status dots,
aligned software/date/status columns, relative installation dates, keyboard-accessible
update buttons). `ui/service-controls.slint` owns their shared fixed-size, keyboard-accessible
controls; `ui/services-dialog.slint` owns the desktop-wide activation-impact review and
install confirmation. `ui/services-progress.slint` owns the live installation and
verified-result overlay; `src/services/progress.rs` owns NDJSON transport and progress
publication. `apps/agent/install_progress.py` streams real stage/build events and
persists receipts, while `releases.py` performs file/link/ledger verification before
reporting success. `apps/agent/system_updates.py`
owns the complete supervised-service inventory, update ordering and date formatting, local desktop
source comparison, and streaming validation of staged Buildroot image candidates.
`update_sources.py` owns fixed publisher endpoints, stable-version parsing and
installed-version probes; `update_checks.py` owns per-account persistent results,
request timestamps, boot-clock gating, bounded retries for transient network failures,
shared package ownership and background publisher checks.
`update_restart.py` owns activation-impact policy and compares this account’s running
desktop with the installed release to identify a real pending restart.
It never installs a kernel or root filesystem; those need a recovery-capable image
installer. `update.py` exposes inventory/check alongside the existing guarded GUI
installer. `ui/app.slint` remains the desktop composition boundary, hosting the
interactive Services view above the gesture layer just like Settings and Files.

Storage cleanup categories live in `storage/category.rs`. The personal-file policy
excludes app-managed cache and build trees. The Cache tab uses the existing
verified npm download plan and revalidates each selected tile before removal.
`ui/hover-tip.slint` owns tooltip appearance and hover timing for Agent Angi's
scope toggle and the Overview storage category icons. Each panel owns its tooltip
copy and placement. `storage/trash_panel.rs` owns the Trash tab's view state,
and the storage worker moves validated files into the same per-volume Trash
used by ANGI Files. `files/trash/inventory.rs` reads the per-volume records for
restore and explicit permanent deletion.

App installation lives in `apps/agent/`: `app_catalog.py` reads and validates `apps.json`; `app_fetch.py` resolves a GitHub release, downloads it against its published SHA-256 and unpacks it with hardened extraction, never executing anything; `app_store.py` owns the per-account NVMe layout, the ledger, `bin/` links and the cross-process lock; `app_install.py` orchestrates install, update and remove with real progress events and is the `angi-update app` / `apps` command; `app_updates.py` turns installed apps into Services rows using the shared cache in `update_checks.py` (`cached_checks`). `update_restart.py` describes their activation impact (the next launch). The native terminal prepends the account's apps `bin/` to `PATH` (`library::terminal_path`, applied in `terminal/session.rs`). In the desktop, `services.rs` runs an app's update from its Services row through the same progress overlay as the desktop update, with `Services.operation-component` naming what is being updated.

The Overview drive picker in `ui/drive-mount.slint` owns NVMe selection and erase confirmation. `storage/mount.rs` calls `apps/agent/data_volume.py` with administrator access off the UI thread. The helper lists existing ext4 partitions and idle NVMe drives, rechecks the selected drive before creating a GPT and ext4 filesystem, and remembers the mounted filesystem UUID for `S12agx-data`.
