# ANGI OS — Native / Wayland Session Switching

Status: Implementation plan (not implemented)
Source: apps at 4cb425f2af62e560f2678f23eb31e8182f722570
Proposed branch: feature/session-mode-switching
Initial hardware target: Raspberry Pi 5 HDMI; follow with Pi 4; x86-64 once graphical support is ready
Owner constraints: do not alter the deployed Pi or the shared apps branch without coordinating the single deployment owner.

## 1. Product decision

ANGI has one Rust/Slint desktop and two mutually exclusive graphics sessions:

- Native: Slint + FemtoVG using linuxkms-femtovg, DRM/KMS directly, no Weston compositor process. Default for Pi 4/5.
- Full Desktop: Slint + FemtoVG using winit-femtovg as a native Wayland client, with Weston controlling display, input and native application windows. Use on demand for Chromium, VS Code, and other graphical applications. Default for supported PC desktop profiles later.
- The same installed full image contains both runtimes. Do NOT uninstall Wayland packages when Native is selected; the optimization comes from not running the compositor.
- Switching desktop modes is a controlled session restart, NOT a live backend replacement of an existing Slint window. Only one process owns the physical DRM display at once.
- Built-in WPE WebKit and ANGI Slint panels remain usable in Native; standalone graphical Wayland apps do not.
- Do not claim a specific memory saving until PSS / MemAvailable / GPU allocations are measured on identical workloads.

## 2. Login screen spec

Reuse crates/agx-gui/ui/login.slint and the existing Pebble Line design tokens and 440px centered card.

Visual order:

    [ANGI mark]
    Sign in
    [Username]
    [Password]

    Desktop environment
    [ ● Native                 |  ○ Full Desktop  ]
    Lightweight · Slint + DRM/KMS. No compositor.

    [Sign in to Native]

Native selector: icon Zap; caption "Lightweight · Slint + DRM/KMS. No compositor."
Full Desktop selector: icon AppWindow; caption "Wayland · Chromium, VS Code and Linux apps."
Selected option: subtle contrasting fill and Pebble Line selection border. One selected at a time. Use a focusable two-option radio/segmented control, not a graphical toggle with ambiguous on/off semantics.

The primary sign-in button is unchanged except its accessible label and destination reflect the selection. The username/password authentication logic remains unchanged. The new selector and sign-in action are disabled while authentication is pending. Arrow keys, tab order, screen-reader labels and Enter submission must be tested.

Default Pi 4/5: Native. Default supported x86 desktop: Full Desktop. If Weston or seatd is not installed, mark Full Desktop unavailable and state why. For the MVP, do not silently apply an old saved preference over a visible explicit selection. An optional per-user "remember choice" feature can be added after the initial handoff is stable.

Current card height is 452px, preferred display 1280x720, minimum display 640x400. Increase the card to fit the mode row at normal desktop heights; for 640x400 use compact spacing or scrollable form contents so password and Sign In remain reachable.

VNC limitation: the greeter's existing socket handoff works for the Native desktop, not for replacing it with a local Weston session. In V1, disable Full Desktop in an active greeter VNC session with a clear explanation and document the existing separate Wayland VNC session. Do not drop remote viewers silently. Extend handoff later.

## 3. Session lifecycle: authoritative controller

Prefer extending the existing unprivileged agx CLI session loop (crates/agx-cli/src/start.rs and main.rs), rather than adding a second service daemon.

The root greeter stays in agx-init on tty1. After authentication, login.rs drops privileges as it already does, then execs:

    /usr/bin/agx desktop --mode native
    /usr/bin/agx desktop --mode wayland

The agx process becomes the single authenticated session coordinator. It is the parent of exactly one active graphical session. The root agx-init supervisor continues supervising the original console process and retains SSH/serial recovery.

Session states:

    Login -> Native -> Switching -> Wayland -> Switching -> Native
                        \-> recovery on failure
    Logout or unrecoverable error -> root greeter, via existing agx-init restart

Native child: /usr/bin/agx-gui, using linuxkms-femtovg on physical HDMI.
Wayland child: /usr/bin/angi-wayland --login; Weston autolaunches agx-gui through agx-wayland-shell, which sets winit-femtovg.

Define explicit exit codes, keeping existing GUI_WANTS_TUI=10:
- 0: logout / normal end
- 10: switch to existing TUI flow
- 20: request Full Desktop (Wayland)
- 21: request Native
- anything else: unexpected error; log and recover according to bounded retry rules

The existing Weston patch 0005 propagates the watched shell exit status to its session launcher. Ensure angi-wayland forwards code 21 to agx rather than treating it as an error and independently execing the Native desktop. Move authoritative fallback decisions into the agx coordinator. Ensure the default refresh path in crates/agx-gui/src/session_refresh.rs remains distinct from a mode transition: do not let its direct exec path bypass the coordinator.

For each switch:
1. Confirm with user if there are graphical apps/unsaved work; permit cancellation.
2. Flush ANGI settings, snapshot only serializable UI state, and record an allowlisted pending app ID if applicable.
3. Ask GUI to exit with code 20 or 21 and wait for completion.
4. Reap/terminate ONLY the managed graphical process tree and wait until DRM/seat ownership is released; do not kill unrelated user or system processes.
5. Sanitize graphical env (SLINT_BACKEND, WAYLAND_DISPLAY, XDG_SESSION_TYPE, XDG_RUNTIME_DIR, display FDs) and create the new session environment.
6. Start the selected backend. Detect readiness from a socket/session handshake, not a fixed sleep. On failure return to the previous mode or a safe recovery login, with an error message.
7. Only after the new Wayland shell is ready, launch the pending allowed application once and clear the request. Returning to Native requires no pending app.

Ensure Native does not spawn Weston or hold a stale Wayland socket. seatd may stay resident as an optional small boot service for fast transitions; measure before making it on-demand.

## 4. On-demand handoff from Applications

Current crates/agx-gui/src/launcher/packages.rs shows a toast when an installed Desktop app is selected without WAYLAND_DISPLAY. Replace that case with a confirmation modal:

    Open VS Code
    This application needs Full Desktop (Wayland).
    ANGI will restart the graphical shell. Save open work first.
    [Cancel]    [Switch and Open]

Use package catalog IDs (vscode, chromium, etc.) and confirmed installed status; never accept arbitrary executable paths from persisted requests.

On confirmation, send a mode request to the coordinator with a pending application ID. Save request in a private, session-scoped path created by agx (mode 0700 directory, atomic 0600 file), or use a same-user Unix socket. The new Wayland GUI consumes the request after initialization and readiness, invokes the existing trusted package launcher, and removes the request. A failed app launch shows an error; it must NOT repeatedly switch modes.

Do NOT kill agents that should outlive the graphical shell. First classify GUI-owned terminal/agent subprocesses; persistent agents must be supervised independently of agx-gui. The GUI must not promise to preserve unsaved application buffers.

Returning to Native is a deliberate action in the session/power menu. Do not automatically switch after one VS Code window closes if other Wayland windows are still active.

## 5. Files to change

- crates/agx-gui/ui/login.slint: two-state mode selector, accessible states, compact-height layout.
- crates/agx-gui/src/login.rs: carry validated selected mode through authentication to agx desktop --mode.
- crates/agx-gui/src/graphics.rs: explicit mode resolution; remove implicit default-to-Wayland behavior for Pi login while preserving KMS detection.
- crates/agx-cli/src/main.rs and crates/agx-cli/src/start.rs: authenticated session coordinator, explicit modes and exit-code transitions.
- crates/agx-gui/src/session_refresh.rs and crates/agx-gui/src/main.rs: separate refresh from mode-switch exit and flush/cleanup.
- buildroot/package/agx-wayland/angi-wayland: forward intentional switch code; delegate recovery to coordinator.
- crates/agx-gui/src/launcher/packages.rs: on-demand prompt, trusted queued app, one-shot launch.
- crates/agx-gui/src/app.rs and UI session menu: explicit Return to Native / Enter Full Desktop commands and confirmation.
- docs/desktop.md and docs/code-structure.md: operation, limitations, recovery and memory benchmark instructions.
- Tests in existing Rust GUI/CLI test modules; new HDMI integration script/test harness when practical.

Preserve the Buildroot Pi 5 full profile with both runtimes installed. A separate Pi Lite image without Wayland packages can come later, but then cannot upgrade to Wayland without installing those packages.

## 6. Implementation PR sequence

PR A — Login selection, explicit mode and no compositor for Native.
- New UI, enum, CLI mode, Pi Native default.
- Both login choices manually verified; no automatic app switching yet.

PR B — Session coordinator and bidirectional clean handoff.
- Exit codes, Weston status propagation, no nested DRM owners, recovery.
- Switch Native -> Wayland -> Native repeatedly on HDMI.

PR C — App-triggered transition.
- Launcher modal, allowlisted pending app, readiness handshake, one-shot launch.
- Validate Chromium and VS Code separately; preserve unsaved-work warnings.

PR D — Reliability, instrumentation and docs.
- Test failure handling, IPC/readiness timeouts, orphan cleanup, VNC limitations.
- CI for unit tests and package build; capture PSS and frame-time comparisons.

Keep every PR isolated from the shared apps branch until reviewed. No automatic deploy to the Pi. Use one designated deployment owner and versioned, reversible builds.

## 7. Acceptance criteria (definition of done)

1. Native is initially selected on Pi 4/5 login; full Wayland available when installed.
2. Logging into Native starts no Weston compositor process and has no active local WAYLAND_DISPLAY.
3. Native retains ANGI Files, agent UI, terminal and built-in WPE WebKit GPU path.
4. Login to Wayland starts exactly one Weston and one ANGI Slint shell, both rendering correctly.
5. Selecting installed Chromium or VS Code in Native prompts for switch; confirmation reopens that app in Wayland exactly once.
6. Cancel leaves Native and its applications unchanged.
7. Return to Native closes managed Wayland GUI safely; Weston exits; DRM ownership is released; Native shell starts.
8. Logout, compositor crash, unavailable seatd, missing Wayland package, rapid double-clicks and repeated switching do not cause a login loop or orphan GPU owners.
9. VNC handoff is preserved for Native; remote Wayland limitations are explicit and tested.
10. All existing GUI tests pass, plus new login/coordinator/app-handoff tests. A dedicated integration test repeats mode switching at least 20 times on HDMI.
11. Compare idle PSS, MemAvailable, GPU allocations and frame-time percentiles between equivalent Native and Wayland workloads. Publish observed results; no invented savings.
12. GitHub checks pass and the live Pi deployment is verified separately with a reversible release.

## 8. Deliberately out of scope

- Changing from Slint/FemtoVG to another UI framework.
- Removing Weston from the installable full Pi image.
- Running graphical native Wayland apps inside Slint/KMS without a compositor.
- Live rendering-context migration without restarting the UI.
- Automatic return to Native immediately after app exit.
- Reworking the existing VNC transport during the initial HDMI implementation.
- Steam, Proton, Gamescope, and NVIDIA driver changes (follow-on work).

## 9. First engineering task

Implement PR A only in this feature branch. Add a login selector and pass the explicit chosen session to the authenticated agx CLI; make Native the Pi default while retaining a Wayland choice. Do not deploy on the shared Pi until the UI and unit tests have passed locally.
