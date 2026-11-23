# ANGI desktop (GUI)

`agx-gui` is a Slint front end with Omarchy/Hyprland-style window management. The tiling engine is `crates/agx-gui/src/wm.rs`, and the key bindings are `keymap.rs`. Both are unit-tested (`scripts/cargo.sh test --manifest-path crates/agx-gui/Cargo.toml`).

The desktop starts in **windowed** mode: Settings fills the left half, with the ANGI overview above CPU and Memory and storage on the right. The overview uses `brand/brand-mark-text.svg`. All five workspaces start in windowed mode; click the layout icon in the top bar or press **Super+L** to switch to tiling. Windows still at their initial positions follow display size changes; moved or resized windows keep their custom geometry.

Click the date or time in the top bar to open the calendar. Drag the lower-right corner to resize it. At larger sizes, the month grid shows saved events and recurring U.S. federal holidays, including observed dates. Wider cells show event start times, and taller cells fit more entries. Multi-day events carry their color across the dates they cover. Choose a date to see its agenda or add an event. Select a saved event to edit its start, end, color, or details, or to delete it. You can choose a date from a month grid and scroll through time choices in either 12-hour or 24-hour format. New events return you to the month after saving, and edits return you to the event’s agenda. The clock format, time zone, and events are saved for the current account.

Click the **memory** bar in **Memory and storage** on workspace 1 to see live resident memory usage by process. Click the **Memory** heading to reverse the sort order. Select a row to see its risk explanation. **End process…** lets you request a normal stop or explicitly force it. System, other-account, and desktop startup processes are protected. A **Lower risk** label still means the process's current task will stop.

The storage grid has tabs for All, Apps & Games, Media, Downloads, Documents,
Cache, and Other. Personal files are grouped by recognizable file extensions and
folders. Downloads groups unclassified downloads and downloaded media or
documents, while recognized installers and game files appear in Apps & Games.
Tile colors continue to indicate file size. The Cache tab checks for verified
OpenCode npm download blobs of at least 1 MiB. Browser caches, build outputs,
and app data are excluded from personal cleanup. OpenCode may download a removed
blob again when needed.

The ANGI Overview includes separate **SD card** and **NVMe** personal-file heat maps. Use the category icons above the maps to filter files. Downloads includes smaller files, while other personal files normally enter the map at 1 MiB. Tiles are ordered largest first, up to 4,096 files per drive. Colors indicate file size. Hover for the path, size and allocated space, or select a square for file details and estimated removal risk.

When NVMe storage is unmounted, **Mount External Drive** can mount an existing ext4 partition with its files intact. **Quick Format…** lets an administrator select an idle NVMe drive and confirm **Erase and Mount**. The quick format replaces the partition table and creates one ext4 partition, erasing access to all files on the selected drive. ANGI mounts the partition immediately and remembers it for startup.

Choose **Move to Trash** to remove a file from the map. The file stays on the same drive and can be restored. The **Trash** tab also shows items moved to Trash from ANGI Files. Open an item's details to restore it to its original path or explicitly confirm permanent deletion to reclaim its space. A restore leaves an existing file at that path intact. **Clear cache…** separately purges verified, re-downloadable cache files to reclaim space immediately.

The map includes files owned by the signed-in user in their home, personal NVMe files, the `angi-dev` project, and selected browser/OpenCode cache, temporary and build-cache folders. OS files, installed runtimes, SDK files, other accounts' files, Git internals, credentials, configuration directories and `~/.local/bin` are excluded. A file also needs a writable parent directory. A drive can show no eligible personal files even while its overall usage is high; drive totals still include system and other-account space. Symlinks are not followed and hard links are counted once (deleting one hard link may leave its data allocated elsewhere).

Scans run off the UI thread every minute after completion, limited to 500,000 entries or 15 seconds per drive. Partial scans and skipped entries are labeled. Only mounted SD/NVMe filesystems are shown; an unmounted boot partition is not scanned.

On Workspace 1, **Clear cache…** next to **Refresh files** checks the signed-in user's OpenCode npm download cache and previews the verified file count and disk space before asking for confirmation. It only selects single-link regular files whose SHA-512 contents match their npm cache path and whose last modification was at least an hour ago. Files changed after the preview are kept. Projects, sessions, settings, installed tools, cache indexes and other cache directories are kept. Packages may need to be downloaded again after cleanup.

Run a preview window on a desktop: `scripts/cargo.sh run --manifest-path crates/agx-gui/Cargo.toml`.

In **Files**, right-click one folder and choose **Start Python server** to serve its contents and open it immediately in **Web**. Right-click empty space and choose **Start Python server here** to serve the folder being viewed. This uses Python 3's static HTTP server on a free loopback port (`127.0.0.1`), under the signed-in account. The Files status bar shows the URL or a startup error. Repeating the action for the same folder reuses its server. Servers stay running when Web closes and stop when the desktop session ends or refreshes.

## Boot and graphical login

With `/usr/bin/agx-gui` installed, Rust init starts `agx-gui --login` on the HDMI console (`tty1`). If no HDMI monitor is connected and VNC is enabled, the greeter uses the software display. Sign in with the ANGI administrator username and password. The desktop runs as that user, with their home directory and supplementary groups. Root, empty passwords, locked accounts and expired credentials are rejected. A failed login has a two-second retry delay. Exiting the desktop or the TUI session returns to graphical login through the supervisor.

`agx desktop` opens the desktop immediately. Its **Open TUI** menu action switches to the authenticated user's TUI, and **Sign out** returns to login. SSH and serial (`ttyAMA10`) retain their existing login paths. Images without the GUI retain the HDMI text login. A GUI startup failure is retried with supervisor backoff; recovery is available through SSH or serial.

The power icon at the right of the GUI header opens a session dropdown: **Log out**, **Enter TUI**, **Reboot**, and **Shut down**. Use the mouse or Up/Down and Enter; Escape or clicking outside dismisses it. Reboot and shutdown require confirmation, with clickable Confirm/Cancel buttons.

From the HDMI TUI, **F8** returns directly to the GUI. The same action is available as **Back to GUI** in Profile and **[GUI F8]** in the footer beside shutdown and reboot. Save or discard Profile edits before switching. Both directions run through one authenticated CLI session; repeated switching does not create nested launchers. SSH/serial TUI sessions display the shortcut disabled and stay in their terminal rather than opening the Pi's graphical display.

The Pi 4 / 400 and Pi 5 images ship with the GUI and desktop runtime libraries. For an incremental development deployment on Pi 5, `make deploy-gui` installs the GUI, CLI, init and device-access helper and saves the original binaries in `/root/agx-gui-backup`. Override `AGX_SSH_HOST` and `AGX_SSH_KEY` when needed. Reboot to activate the new init process.

Use [direct Ethernet](pi-ethernet.md) for development access and deployment. As a recovery alternative, the development Pi was reachable over Wi-Fi at `192.168.1.184` (DHCP; checked November 16, 2026). Its SSH host key matches the recorded USB address. To deploy over this connection:

```sh
AGX_SSH_HOST=root@192.168.1.184 AGX_SSH_HOST_KEY_ALIAS=10.77.0.2 make deploy-gui
```

Wi-Fi deployment requires the NVMe volume to be mounted. It stores the GUI binary, Mesa/runtime files and original binary backups under `/var/lib/agx-data/agx-gui`, with symlinks at their normal system paths. This avoids filling the small SD root filesystem. Libraries are replaced by symlink rename, preserving mappings held by running processes; deployment does not replace libc or its loader.

The data-volume mount script `/etc/init.d/S12agx-data` must remain a regular file on SD. An older OpenCode deployment linked it into its NVMe release, preventing the volume from mounting on reboot and selecting the HDMI text recovery login. To repair that installation over root SSH, mount the configured data volume after verifying its UUID against `/etc/agx/data-volume.uuid`, copy the resolved script to a new regular file on SD, and atomically replace the symlink with that file. Reboot to restart graphical login. The OpenCode installer now copies this boot script instead of linking it.

To install graphical login directly onto an SD card connected to the workstation, run `sudo scripts/install-gui-login-sd.sh /path/to/agx-rootfs /path/to/agx-boot`. This places the GUI executable on SD, verifies the Rust PID 1 boot entry, enables the existing graphical greeter, and checks that accounts and Wi-Fi settings remain unchanged. The Pi's previously installed Mesa/input libraries still require its NVMe volume. A missing GUI executable selects the HDMI text recovery login; missing GPU libraries cause GUI startup retries while SSH/serial recovery remain available.

Both physical Super keys drive desktop shortcuts on HDMI. Slint 1.19's LinuxKMS mapping omits `Super_L`/`Super_R`, so ANGI uses its public libinput hook to deliver those two keys as Slint Meta press/release events to the current window. Other keys keep Slint's existing layout and modifier handling; VNC retains its separate keysym mapping. The hook feature is pinned to the installed Slint/input API and should be reviewed when upgrading Slint. Native scrolling uses the same bridge: wheel v120 units are converted to 60 logical pixels per notch, fractional high-resolution steps are retained, and touchpad/continuous scroll uses logical pixel deltas. Both axes follow Slint/VNC direction conventions. The deprecated duplicate axis event is consumed without scrolling twice. Native cursor position is tracked so the hovered page or scrollable UI receives the event.

Local keyboards and mice are discovered through libinput/libudev-zero. The supervised display-access service runs `agx-display-devices`, which grants `agx-admin` access before forwarding kernel input arrival/removal notifications to libudev-zero. This covers keyboards that enumerate late during boot and devices reconnected through a USB hub or switch. It relays device lifecycle notifications, not keypress contents. Install the helper with the GUI; the older permission-only shell fallback cannot notify libinput of newly attached devices.

Preview just the login screen without root or real authentication:
`scripts/cargo.sh run --manifest-path crates/agx-gui/Cargo.toml -- --login-preview`.

### Native Wayland session

The full Pi 5 image includes Weston and the Native desktop. Other graphical
Buildroot images can include Weston with `BR2_PACKAGE_WAYLAND=y` and
`BR2_PACKAGE_AGX_WAYLAND=y`. Weston uses ANGI's existing libudev provider.
Weston owns the HDMI display, input, and external app windows. Settings,
Calendar, and the other Slint panels still live in the ANGI shell window.
The shell fills the available display with its own header and controls.
On HDMI it draws the Pebble Line interface directly into a GPU-backed Wayland
surface using FemtoVG and Mesa Wayland EGL. Headless test sessions use software
rendering by default. When enabling Wayland in an existing Buildroot output tree,
reconfigure Mesa so it builds its Wayland EGL platform.
Native Wayland apps started from ANGI Terminal inherit the session socket
and join the same split layout as ANGI panels. Apps started from Applications
join it too. The Pebble Line header owns the tile controls, and each app
submits its own buffers to Weston. The app fills the tile body edge to edge
and follows divider and frame resizing. Menus and transient windows follow
their parent application's workspace.

Use the header to move, float, fullscreen, or close a window. The desktop
keyboard shortcuts work when focus is in the app. Switching workspaces
retains the running apps and split geometry. Reopening an app with a Wayland
app ID restores its last workspace and a compatible saved size. The
workspace's active layout governs new placements. In windowed mode,
the shell checks for space beside existing floating windows before
restoring overlapping bounds. The app manages its own documents and
session state.

During a live resize, Weston moves the tile immediately and uses the last
client buffer until the app commits a new one. During the drag, the next
size is sent as soon as the app commits a new size, at most once per 16 ms
frame. A slow app, or one that keeps its own minimum size, receives the latest
size every 66 ms. The final size is sent immediately on release. On Pi 5 this
doubled weston-terminal's resize updates during a one-second drag (15 to 28).
Set `AGX_WAYLAND_RESIZE_TRACE=1` in a test session to log requested and
committed native tile sizes.

Move the pointer to the top edge of a fullscreen window to reveal Exit
fullscreen, Minimize, and Close. Move down to hide the controls.

Choose **Native** or **Full Desktop** on the sign-in screen. Native is the
default on Raspberry Pi 4 and 5. On HDMI it uses Slint and FemtoVG directly
through DRM/KMS, with no Weston process. ANGI Files, Agent Angi, Terminal and
the built-in Web panel are available in Native. Choose Full Desktop to open
standalone Wayland applications such as Chromium and VS Code.

Full Desktop requires a local display, the installed Weston runtime and a running
seatd service. An active greeter VNC viewer also disables Full Desktop so that
the viewer can follow Native across sign-in. An earlier saved Wayland choice
does not overwrite the selection. To change modes, sign out and select the
other mode. Refresh desktop restarts the current shell. If the selected
desktop fails to start, the console logs the failure and returns to sign-in.
For a side-by-side protocol check, `angi-wayland --headless` creates its
own socket in a private temporary runtime directory, with no HDMI output.

For remote access to native Wayland apps such as VS Code, start
`angi-wayland --vnc` as the signed-in user when the Pi's port 5900 is free.
Weston starts a separate GPU-rendered desktop with its own application
configuration and listens on `127.0.0.1:5900`. Connect through the
[existing SSH tunnel](pi-ethernet.md#see-and-control-the-desktop).
The HDMI desktop can keep running beside the remote session.
ANGI patches Weston's VNC backend so a viewer's Super key acts as a modifier
(desktop shortcuts work while a native app has focus) and so app windows draw
again after a workspace switch or an overlay hides them. Both problems affected
only VNC; the HDMI backend handles modifiers through libinput and assigns
every window a plane on each frame.

For frame pacing measurements, start a test session on an available VT with
`AGX_WAYLAND_DEBUG=1 angi-wayland`. In its Terminal, run
`weston-debug timeline > timeline.jsonl` while reproducing the workload.
Weston exposes extra debugging and capture interfaces in this mode. Use it
with trusted clients. To compare another HDMI mode, copy the installed
`/usr/share/agx/wayland/weston.ini` to `~/.config/agx/weston.ini` and add an
`[output]` section with the connector name and `mode=1920x1080@60`. The
current connector, mode, and GL renderer are recorded in `/run/agx/gui.log`
for the sign-in session.

The Install catalogue enables apps when a matching native package is staged
under `/usr/share/agx/native-apps`. Installed desktop apps open as independent
windows from the Wayland session. For VLC on Pi 5, merge
`buildroot/board/agx/rpi5/native-vlc.config` into a Pi 5 Buildroot configuration,
build the `vlc` and `qt5wayland` targets, and run
`python3 scripts/package-native-vlc.py output DESTINATION` to create the archive
and target manifest. If reusing a build tree, reconfigure PCRE2 for Qt's 16-bit
library before building those targets.

For Chromium on Pi 5, extract the matching Raspberry Pi OS Bookworm ARM64
`chromium` and `chromium-common` packages into one directory. Extract the
supplemental Debian Bookworm ARM64 packages into a second directory. The
supplemental packages are `libdav1d6`, `libdouble-conversion3`, `libflac12`,
`libjpeg62-turbo`, `liblcms2-2`, `libminizip1`, `libogg0`, `libopenh264-7`,
`libopenjp2-7`, `libopus0`, `libxnvctrl0` and `libzstd1`. Verify each package
against its repository checksum. With the Pi 5 VS Code runtime dependencies
built, run
`python3 scripts/package-native-chromium.py output CHROMIUM_TREE DEBIAN_TREE VERSION DESTINATION`
and stage the resulting archive and manifest. Chromium opens as its own
Wayland window using the kernel's user-namespace sandbox.

For VS Code on Pi 5, merge `buildroot/board/agx/rpi5/native-vscode.config` and
build `libnss cups libgtk3 xlib_libXcomposite xlib_libXdamage xlib_libXrandr`.
Reconfigure Cairo first when reusing a build tree so its PDF and X11 client
support matches this configuration.
Run `python3 scripts/package-native-vscode.py output TARBALL METADATA DESTINATION`
with Microsoft's original Linux ARM64 tarball and its matching update metadata.
Stage the resulting runtime archive and manifest together. Install downloads the
editor directly from Microsoft and checks its published SHA-256. The packaged X11
client libraries satisfy Electron's linker dependencies. Wayland handles the
editor window.

For Blender on Pi 5, use Blender 5.3 or later with the matching Blender ARM64
dependencies. Apply `buildroot/board/agx/rpi5/blender-vulkan-depth-format.patch`
to the Blender source tree so Vulkan can use the Pi's packed depth and stencil
format. Configure a portable Linux ARM64 build against the ANGI Pi 5 SDK
with `WITH_GHOST_WAYLAND` and `WITH_VULKAN_BACKEND` enabled and
`WITH_GHOST_X11` and `WITH_OPENGL_BACKEND` disabled. Point CMake's
`_ffmpeg_INCLUDE_DIR` and FFmpeg component library entries at the matching
Blender dependency bundle each time you configure so the headers and libraries
provide the same API. The `buildroot/board/agx/rpi5/native-blender.config`
fragment selects Mesa's
Broadcom Vulkan driver and the Vulkan loader. Reconfigure Mesa if reusing an
existing build tree. For a build using the upstream USD library, build the
client libraries with `make -C output xlib_libSM libglvnd 'LIBGLVND_CONF_OPTS=-Dx11=enabled -Dglx=enabled -Degl=false -Dgles1=false -Dgles2=false'`.
Add the MaterialX library directory and Buildroot staging `usr/lib` to
the linker's `-rpath-link` search path. Run
`python3 scripts/package-native-blender.py output PORTABLE_INSTALL DRIVER LOADER VERSION DESTINATION`
with the portable install, `libvulkan_broadcom.so`, and `libvulkan.so.1`. Stage
the Blender archive and manifest in the same native package directory. The
Blender launcher uses that bundled Vulkan driver for its window and viewport.

## Mouse settings

Open the top-right power menu and choose **Settings**, or search for Settings in Apps. Settings opens as a normal desktop tile alongside your other windows. It supports tiling, floating, resizing, fullscreen and workspace moves; opening it again focuses the existing tile. The Mouse & pointer panel offers pointer speed, adaptive acceleration, left/right primary button, natural scrolling and scroll speed. Changes apply immediately to connected native pointing devices and are reapplied when a mouse reconnects. The test area lets you try movement and clicks; **Restore defaults** resets this collection of preferences. Close it with the title-bar close button or the normal desktop shortcut; Escape leaves the tile open. The category column collapses to a narrow icon rail in small tiles, and controls stack below their descriptions. **Refresh desktop…** now lives in the Overview footer, to the left of **Clear cache…**. It asks for confirmation, saves pending preferences, closes the desktop’s terminal and browser sessions, and starts the latest installed GUI as the same signed-in account. It does not reboot the Pi; VNC viewers may need to reconnect.

Preferences belong to the signed-in account and are saved atomically in `$XDG_CONFIG_HOME/agx/mouse.conf` (normally `~/.config/agx/mouse.conf`). The greeter keeps its own defaults. Weston applies the saved pointer speed, acceleration profile, button handedness and natural scrolling to supported devices after the preference saves and when a device connects. VNC movement remains controlled by the viewer. Desktop previews show the controls without changing the workstation's mouse settings.

On Pi 5 HDMI, Weston moves the cursor on the DRM hardware plane when the
cursor buffer fits the plane. Pointer-only motion over an unchanged desktop
can reuse the primary framebuffer. Hover effects, scrolling and dragging
still update the affected content.

## Rendering and performance

Both appliance and preview builds include FemtoVG and software rendering. The default on the Pi is `auto`: a connected HDMI monitor selects `linuxkms-femtovg`; otherwise it selects the resizable `vnc-software` display, which requires VNC to be enabled. Previews default to `winit-femtovg`. Mesa provides V3D, VC4, EGL, GLES and GBM; the kernel's `v3d` and `vc4` modules provide rendering and HDMI respectively. No X11 or Wayland server is needed on the Pi. Renderer selection is explicit: initialization failures are reported rather than silently producing software-renderer benchmark results. Slint draws the normal GPU pointer. LinuxKMS maps built-in resize shapes to its default arrow, so AGX overlays directional move/resize images and hides the native pointer while those shapes are active. The software renderer uses the same images in its cursor overlay. Cursor strokes use rounded tips and switch between pearl/dark-halo and slate/white-halo variants according to the current background palette. Run `scripts/cargo.sh run --manifest-path crates/agx-gui/Cargo.toml -- --cursor-preview` for the live dark/light comparison at 1× and 3× scale.

Slint 1.19 aligns FemtoVG's rounded clip layers to the device pixel grid. Pebble Line uses these layers for window tiles. Fractional tile positions previously caused FemtoVG to resample the whole layer, softening text and icons inside the tile. [Slint 1.19 release notes](https://github.com/slint-ui/slint/blob/v1.19.0/CHANGELOG.md#1190---2026-11-16)

Pointer hover uses the cached window scene without rebuilding layout or replacing Slint models. Tiled dragging updates only the ghost and affected target highlights. Floating dragging and resizing notify changed window rows in a persistent `VecModel`. Membership/stacking changes rebuild the rows within that same model. Motion remains immediate; frame-based input coalescing can be evaluated if physical profiling still shows a backlog.

The shared desktop gesture layer forwards motion only through Slint's `pointer-event`. Slint also emits `moved` during a grabbed drag; forwarding both processes the same physical motion twice. Because the first callback can move the window, recomputing desktop coordinates in the second callback adds the window's displacement to the pointer position and causes jumps. Rendered input checks cover forward/reverse floating motion, top-left resizing and tiled drag ghosts.

Verified November 17, 2026: the old rendered input path moved a window 6.5×3 pixels for a 3.25×1.5-pixel pointer step. With the duplicate callback removed, all 130 GUI tests and the workspace suite passed. The development Pi runs the updated executable after an authenticated desktop refresh; VNC captures of its 1920×1080 FemtoVG desktop confirmed exact floating-window displacement in both directions and return to the starting position. The previous executable is retained at `/root/agx-smooth-drag-backup/previous-gui`. Physical HDMI smoothness still needs a hands-on check.

The root greeter reads `/etc/agx/gui.env` and passes its graphics options to the authenticated session. Environment variables override the file. It accepts only these literal `KEY=value` options (no shell expressions):

```text
SLINT_BACKEND=linuxkms-femtovg
# Optional while measuring frame rate; leave off for normal use.
# SLINT_DEBUG_PERFORMANCE=refresh_lazy,console
# Optional: select the connected output and its listed 1080p/60 mode index.
# SLINT_DRM_OUTPUT=HDMI-A-1
# SLINT_DRM_MODE=4
# Remote desktop over VNC; see "Remote access (VNC)" below.
# AGX_GUI_VNC=on
```

Set `SLINT_DRM_MODE=list` to list actual supported modes in the log, then replace `list` with the index for **1920×1080 at 60 Hz**. Mode indices depend on the connected display; `4` above is only an example. Reboot after changing the configuration to restart the protected graphical console cleanly. Greeter and authenticated-session stderr are written to `/run/agx/gui.log`, readable through root recovery SSH. This log is volatile and rotates between sessions after 4 MiB. Remove the performance option after measuring.

Compare `linuxkms-software` and `linuxkms-femtovg` at the same mode. Drag floating windows, resize splits, and switch workspaces. Use `refresh_lazy` during continuous motion, record FPS dips as well as sustained rates, and aim for 60 frames/s (16.7 ms/frame). Idle lazy-refresh FPS is not a performance failure. Repeat at 4K only after the 1080p baseline. FPS averages alone do not establish worst-case frame latency. See [Slint's LinuxKMS backend](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_linuxkms/) and [performance debugging](https://docs.slint.dev/latest/docs/slint/guide/development/debugging_techniques/).

For a headless GPU check, cross-build `scripts/check-gpu.c` against the Buildroot EGL/GLES libraries and run it on the Pi. It creates a GLES2 context and requires the renderer string to identify V3D. This verifies driver/context initialization, not HDMI performance.

On November 16, 2026, the headless check passed as root and `dana`: EGL 1.5, Broadcom V3D 7.1.10.2, OpenGL ES 3.1, Mesa 26.1.8. Both HDMI ports were disconnected, so HDMI startup and frame-rate measurements remain unverified.

The first GPU deployment mistakenly linked `/lib/libgcc_s.so.1` to NVMe, creating a boot dependency cycle; the Pi did not return after reboot. The packager now excludes `/lib` and `/lib64`, and deployment checks that PID 1's resolved dependencies do not require NVMe. To repair that first installation, power off the Pi, mount its SD root partition on the workstation, then run `sudo scripts/repair-gui-boot.sh /path/to/mounted/agx-rootfs`. This restores only the original ARM64 `libgcc_s.so.1` to the SD filesystem. Unmount the card before returning it to the Pi. Boot recovery still needs confirmation after this repair.

## Remote access (VNC)

`agx-gui` can serve its own screen over VNC so a viewer can display the login screen and desktop without a monitor on the Pi. The Pi 4 / 400 and Pi 5 images include the desktop and ANGI Web and set `AGX_GUI_VNC=on` at build time. On other images, set it in `/etc/agx/gui.env` on the Pi and reboot to enable it, or export it when running a preview. The greeter passes this setting to the signed-in session. Accepted values are `on` (127.0.0.1, port 5900), a port number, or a loopback `address:port`. Set `off` to disable it. The log in `/run/agx/gui.log` shows `vnc listening on ...` when it started.

For Pi 4 / 400, the [provisioning guide](pi4-provisioning.md#open-the-desktop) shows how to open VNC over the direct SSH connection after creating the administrator. For a browser-provisioned ISO, use the administrator’s SSH login and verify the host fingerprint shown during browser setup.

For an actual desktop resolution that follows the Remmina window, select the VNC-only software display in `/etc/agx/gui.env`:

```text
SLINT_BACKEND=vnc-software
AGX_GUI_VNC=on
```

Restart the graphical session (or reboot), then use Remmina's **Toggle dynamic resolution update** mode; saved profiles use `scale=2`. This changes the rendered workspace dimensions rather than stretching a fixed-size image. The initial display is 1280×720; resizing Remmina requests its available content dimensions, excluding the toolbar and window decorations. One screen is supported, from 320×200 through 4096×2160; the UI is most useful at 640×400 or larger. All viewers share the desktop size, so the latest accepted request applies to everyone. Screen layout capability and the current viewer size survive login handoff. LibVNCClient 0.9.15 ignores screen ID zero and sends uninitialised screen metadata when resizing. ANGI therefore canonicalises the single-screen metadata to ID 1 at origin (0,0), with no flags, while using the requested dimensions; this compatibility behaviour differs from strictly echoing the client's metadata in a successful reply. Invalid sizes, multiple screens, or screen dimensions that differ from the framebuffer receive a failure reply without allocating a new capture.

`vnc-software` renders only for remote viewers and does not draw the desktop on HDMI or read local keyboard/mouse input. SSH and serial recovery remain available. To restore the local display, select `linuxkms-software` or `linuxkms-femtovg` and restart the graphical session. With those fixed HDMI backends, VNC continues to mirror the physical display; client resize requests receive a refusal, and Remmina's scaled mode (`scale=1`) can fit the picture into its window.

For a complete PC with a local HDMI screen and optional VNC mirroring, use:

```text
SLINT_BACKEND=auto
AGX_GUI_VNC=on
```

Automatic mode checks both HDMI connectors for up to two seconds at graphical startup. With HDMI connected, VNC mirrors the monitor; without HDMI, Remmina can change the desktop resolution dynamically. The selected backend is retained across sign-in. Selection happens at startup, including after logout/restart; plugging in a monitor during a running VNC-only session requires restarting the graphical session. Explicit backend settings still override detection.

Remove old `video=HDMI-A-1:...D` overrides from the boot command line: those force HDMI to appear connected with no cable. Automatic detection ignores a forced connector unless it also has monitor EDID evidence. Normal connector detection does not require EDID.

Verified on November 16, 2026: after removing the forced-HDMI boot argument and rebooting the development Pi with both ports disconnected, automatic mode selected `vnc-software` and started the VNC login screen. The GUI suite passes 90 tests, including both HDMI ports, requested-output filtering and forced connectors without monitor evidence.

A working VNC login with a blank HDMI screen can indicate that `vnc-software` is still selected. Check `/etc/agx/gui.env` and `/run/agx/gui.log` before changing HDMI boot settings. Restart the graphical session after selecting the physical backend; the configuration persists across boots. Use Remmina's scaled mode for fitting this mirrored desktop into its window.

Verified on the development Pi on November 16, 2026: scripted resize requests produced full frames at 1600×900, 1920×1080, 1024×768 and 1280×720, with keyboard events and oversized-request rejection. Remmina 1.4.43 with LibVNCClient 0.9.15 changed the rendered size from 1454×861 to 1554×911 during window resizing. After deployment and reboot, the main Ethernet profile negotiated 1215×991 for its usable content area; the firmware power check returned `throttled=0x00000000`. Both saved ANGI profiles use `scale=2`. The previous Pi executable link and graphics configuration are saved under `/root/agx-vnc-dynamic-backup/`. The workspace suite and all 69 standalone GUI tests passed; resize capability across login handoff is covered by a live socket test.

The server listens on **loopback only** and offers no VNC password, because the classic VNC password scheme (a DES challenge) is too weak to put on a network. Access control is SSH: a non-loopback address is refused, and you connect through an SSH tunnel. Anyone who can already open a TCP connection to the Pi's loopback port, such as another logged-in user, can type into the desktop, so enable it only on a Pi whose shell users you trust.

Use an OpenSSH tunnel, not Remmina's built-in one: Remmina's libssh tunnel crashed Remmina (SIGSEGV inside libssh while closing a tunnel) on November 16, 2026. `scripts/vnc-tunnel.sh` runs the tunnel with `ssh` instead, listening on `127.0.0.1:5941` (Ethernet, to `10.78.0.2`) or `127.0.0.1:5942` (Wi-Fi, to `192.168.1.184`). Start one with `scripts/vnc-tunnel.sh up ethernet` (or `wifi`), check it with `status`, stop it with `down`. To keep both up automatically, including after a Pi reboot, install the user service from the repository root. Stop any manually started tunnels first so they do not occupy the service ports:

```bash
scripts/vnc-tunnel.sh down ethernet
scripts/vnc-tunnel.sh down wifi
install -Dm644 scripts/systemd/angi-vnc-tunnel@.service ~/.config/systemd/user/angi-vnc-tunnel@.service
systemctl --user daemon-reload
systemctl --user enable --now angi-vnc-tunnel@ethernet angi-vnc-tunnel@wifi
```

Once enabled, the script's `up` and `down` commands start and stop the corresponding service; systemd restarts unexpected SSH exits every three seconds. Keep this checkout at `~/dev/AGX`, or adjust the unit's `ExecStart`. User services start at login; with user lingering enabled they start at workstation boot and stay available after logout (`loginctl show-user "$USER" -p Linger` checks it). Lingering is already enabled on the development workstation. If the Pi is off when the workstation starts, SSH retries until the Pi returns. Existing VNC sessions may disconnect when the network drops; reopen the saved Remmina profile once the tunnel returns.

In Remmina, create a profile with protocol **VNC**, **Server** `127.0.0.1:5941` (or `5942`), the SSH tunnel option **off**, and no VNC password. Remmina's "Execute a Command" before-connect setting did not run in this version, so do not rely on it to start the tunnel. Adjust the Pi's address, key and user with `AGX_ETHERNET_HOST`, `AGX_WIFI_HOST`, `AGX_SSH_KEY` and `AGX_SSH_USER`. ANGI sends uncompressed pixels, so on slow Wi-Fi choose a lower colour depth in Remmina to reduce traffic.

The VNC server claims each update request before sending its frame, preserving another request received during transmission. Clearing the request after the write can lose the viewer's next request and leave the picture frozen while SSH stays connected. A live socket regression test reproduces that overlap; it failed before the fix and passes afterward. On November 16, 2026, the same overlapping-request probe stalled on the development Pi before deployment; after deploying the fix and restarting the graphical login, Ethernet delivered all 20 full-screen frames with Shift press/release events in 2.55 s without losing a request. The user subsequently confirmed that typing in Remmina over Ethernet was fast and stable.

The TUI's **Network** and **Services** pages show observed VNC listening addresses, established TCP connection count, and GUI resident memory (RSS, including VNC). Taller terminals also show the Pi's IPv4 addresses and the SSH-tunnel requirement; compact terminals retain the VNC and memory lines. The same information is in `agx status --json` under `vnc`. The monitor uses `AGX_GUI_VNC` from the session or readable `/etc/agx/gui.env` to select the port, otherwise 5900. Socket connections are counted before the VNC handshake, so the count is not an authenticated-viewer count. VNC runs inside the GUI process, rather than as a separate service with its own restart button. The workstation's tunnel services belong to its systemd user manager and are not Pi services. Switching the graphical session to the TUI stops VNC until the GUI is opened again; an SSH TUI can inspect it while the GUI stays running.

VNC shares Slint's RGBA snapshot with the viewer threads. Each full capture is about 7.9 MiB at 1920×1080, and each viewer references its previous capture for change detection. Pixel conversion streams one row through a 64 KiB output buffer. The handshake keeps only the screen dimensions. When the last viewer disconnects, the server releases the shared capture and asks glibc to return unused allocator pages on its network thread. GUI RSS includes rendering, GUI data and VNC. The TUI calculates system-wide used memory as total minus `MemAvailable`; BusyBox `free` uses a different calculation.

Signing in replaces the root login process with your desktop, which would normally drop every connection. The login screen therefore passes its listening socket and connected viewers to the desktop through the exec (`AGX_GUI_VNC_HANDOFF`, with `agx` closing its own copies), and the desktop carries on serving the same connection, sending a full picture as soon as it has one. If anything about the handoff looks wrong the desktop ignores it and opens its own listener, and then the viewer shows a dropped connection and you reconnect (Remmina's reconnect button, or open the profile again).

How it works: the UI thread captures the window with Slint's `take_snapshot` only while a viewer is waiting (about 2 frames/s idle, about 15 frames/s for two seconds after input), sends just the changed rectangle in Raw encoding, and resizes the viewer if the window size changes. Pointer and key events are injected as Slint window events, so the shortcuts below work remotely. Automatic mode uses the independent software display when no HDMI monitor is present; it does not need a forced HDMI boot mode. Explicit KMS backends require a connected display and log startup failures in `/run/agx/gui.log`. The pointer you see is the viewer's local cursor. On the Pi the Mod key is Super, which many viewer desktops intercept; plain number keys switch to existing workspaces (use `0` for workspace 10), and `/` opens Apps without it.

Status: the protocol, input mapping and server have unit tests (including a live socket client). On November 16, 2026, Remmina displayed the desktop preview window on the workstation, including a window resize, and a scripted client confirmed keyboard input, incremental updates, and 32-bit and 16-bit pixel formats. On the development Pi (no monitor, historical forced-connector configuration, `linuxkms-femtovg`) a client reached the graphical login screen through an SSH tunnel to Dropbear and received a 1920×1080 frame; the first full frame took about 2.8 s over Wi-Fi. After signing in, a reconnect from Remmina's saved profile showed the live desktop. Over Wi-Fi the first full frame took about 2.5 s and later updates 0.5-1 s each, because pixels are sent uncompressed; that is the main thing to improve. The sign-in handoff is covered by unit tests (a new server adopting a listener and a connected viewer, resizing it, and accepting new ones) but was deployed to the Pi only on November 16, 2026 and still needs confirming with a real sign-in over VNC. Not implemented: compressed encodings (ZRLE, Tight), clipboard sharing, a VNC password, and multi-screen resize layouts. The VNC-only software display described above works without a forced connector.

### Tracing a GUI freeze

Run `python3 scripts/trace-gui.py` on the workstation before reproducing a freeze. It saves logs under `.cache/gui-traces/` for two hours, reconnecting after Pi restarts. The default destination is `root@10.78.0.2`, using the existing SSH key and the USB host-key alias; override these with `--host`, `--key`, and `--host-key-alias`. Use `--duration SECONDS` to change the capture lifetime, or Ctrl+C to stop it. No Pi binaries or settings are changed.

The capture includes GUI stderr, kernel messages, syslog, the service lifecycle journal, memory, temperature, boot IDs, and GUI thread kernel stacks sampled every half second. Separate SSH streams prevent a blocked stack read from stopping the health logger. Workstation timestamps remain useful even while the Pi's clock is unset. Workstation kernel messages and tunnel logs distinguish a lost Ethernet carrier from a GUI connection closing. Each stream rotates at 8 MiB with three previous files retained. Existing application logs are included; screen pixels and process memory are not captured.

The Pi's normal GUI and system logs are volatile, so start the collector before reproducing the failure. If the GUI freezes, leave the Pi powered on briefly to allow stack capture. A final GPU fence wait is evidence of in-flight graphics work, not proof that the GPU caused the failure; correlate it with kernel errors, process exits, boot IDs, and the timing of any manual restart.

## Direct Ethernet

Use [the shared Ethernet access and control procedure](pi-ethernet.md) for Claude and Codex: route and identity checks, SSH, legacy SCP, tunneled VNC, deployment, activation and recovery. Direct RJ45 Ethernet is the established development connection: workstation `10.78.0.1/24` on `enp1s0`, Pi `10.78.0.2/24` on `eth0`. The existing `agx-ethernet` workstation profile autoconnects at priority 10 and does not replace the default Internet route. The Pi address is static; connecting its Ethernet port to a router does not enable DHCP. The separate USB recovery subnet is `10.77.0.0/24`. On November 16, 2026, the direct link negotiated 1000 Mbps, answered ping in about 0.2 ms, and delivered the first full VNC frame in 0.09 s.

## Modifier

The main modifier is **Super** in both appliance and preview builds. **Super+Space** opens Applications. For a nested preview whose host compositor reserves Super, explicitly set `AGX_GUI_MOD=alt`; `AGX_GUI_MOD=super` selects the normal default. Below, `Mod` means that key. "A little" is Alt with Super (Ctrl with Alt); "a lot" is Ctrl with Super (Super with Alt).

## Opening files from a terminal

`open FILE_OR_ADDRESS...` hands a file or web address to the desktop, which shows it in the app that
handles it. Today that is **Web**: `.html`, `.htm` and `.xhtml` files, `http(s)://` addresses, and servers on
this device. Anything else is refused with a reason (for example "ANGI has no app for .txt files yet"), and so
is a folder; a bare word such as `example.org` is never guessed to be a website.

Servers on this device work too, for example a Python server started with `python3 -m http.server 8000`:
`open localhost:8000` (or `127.0.0.1:8000/page.html`, `[::1]:8000`) opens `http://localhost:8000`. Bare
`localhost`, `127.x.x.x` and `[::1]` names, with an optional port and path, default to plain `http://` here
and in Web's address bar, because such servers rarely have a certificate; every other bare name still gets
`https://`, and a file that really is called `localhost:8000` is opened as a file. Relative paths work from the terminal's own
folder, links are followed before the file type is judged, and Web comes forward without opening a second
window.

`open` is the desktop binary under another name (`/usr/bin/open` links to `agx-gui`, so it always matches the
running release; `agx-gui open FILE` is the same thing). The desktop listens on a Unix socket in the signed-in
account's own folder (`$XDG_RUNTIME_DIR`, else `~/.cache/angi`; mode 0600 in a 0700 folder) and only answers
that account, so `open` also works from an SSH session of the same user and does nothing for other accounts.
It prints `The ANGI desktop is not running for this account.` when there is nothing to answer, including
right after `open` is first installed, until the desktop has been refreshed once to load the listener.

Local pages load as `file://` addresses, which the address bar still refuses when typed; only `open` can ask
for one. The Web engine keeps local pages from reading other local files (`allow-file-access-from-file-urls`
is off). `open` works from the app code in `crates/agx-gui/src/open_request.rs`; adding another file type
means adding its app to `resolve()` there.

## Applications launcher

**Super+Space** opens Applications on **Apps**. This tab shows Web, Files, Terminal, and installed software. **Commands** contains window, workspace, and session actions. **Settings** contains ANGI’s built-in tools and preferences. **Install** starts with Blender, VS Code, VLC, and GStreamer, followed by compatible tools and servers from the catalogue.

Choose a row in **Install** to start a background job. The launcher shows its progress and makes the app available in **Apps** when installation succeeds. A featured app that cannot run on the current device shows the reason. Newly installed commands are available in existing Terminal sessions. Select **Remove** or press Delete on an installed row to remove its runtime. App preferences and files are kept.

The installer checks the running target before offering a package. On the Pi 5, the native GStreamer package exposes `gst-launch-1.0` and `gst-inspect-1.0` in ANGI Terminal. Blender, VS Code, and VLC require a matching native package and open in the [ANGI Wayland session](#native-wayland-session). Users with a previously installed Debian-runtime package can remove it through `angi-update app remove ID` and keep its settings and data.

**Browsers.** ANGI Web uses WPE WebKit with GPU-accelerated rendering on the Pi. Its frames reach the desktop as DMA-BUFs imported into the FemtoVG GL context on both the standalone DRM/KMS desktop and the Wayland session; the software renderer and VNC-only display use the CPU frame transport. Set `AGX_WEB_TRACE=1` to log the active bridge and its frame rate. On a Pi 5 Wayland session, scrolling a long page took about 0.5 CPU cores on the GPU bridge against 1.2 on the CPU bridge. The appliance desktop runs directly on DRM/KMS. External graphical browsers require a display-server integration.

The desktop runs `angi-update app install ID [--progress]` in the background. For GitHub apps, the installer selects the matching asset from the latest stable release and verifies its published SHA-256. For native apps, it verifies the archive against the staged manifest. VS Code also downloads Microsoft's ARM64 archive and verifies its published SHA-256. Downloads use verified HTTPS and check each redirect against the publisher's allowed hosts.

Extraction checks archive paths, links, file types and size limits, and drops setuid bits. The installer checks that programs are 64-bit ARM Linux ELF files before switching `pkg/ID/current` and exposing their commands through `bin/NAME` in the account's private NVMe store at `/var/lib/agx-data/agx-opencode-users/UID/apps/`. One install runs at a time, and an existing command owned by another app is preserved. `angi-update app remove ID` removes the links, the release and the ledger entry.

**Tracking and updates:** every installed app appears in **Services** as its own row (key `app:ID`) with its installed and latest versions, using the same per-account update-check cache, retry, clock and rate-limit handling as the system components. The row's Update button reviews and installs the newer release with the same progress overlay. Opening the launcher refreshes stale checks (at most every ten minutes), and the desktop checks at start. Updating never restarts anything: running copies keep the old version until they exit.

All 51 entries were installed for real from GitHub into a scratch store during development (checksum-verified, unpacked and ELF-checked, never executed). They have not all been run on the device. To add an app, find its aarch64 Linux release asset, add an entry, and run `scripts/test-app-install.py`; the tests reject duplicates, unsafe paths and incomplete entries.

Type to search across Settings, Apps and Commands at once, so autocomplete works from any tab (installed state included): a name match outranks a description match, and the open tab's results come first among equals. Search in Install stays within the install catalogue. Search recognises terms such as Wi-Fi, RAM, disk, reboot, vlc and spotify. Up/Down select a result, Page Up/Page Down move five results, Enter opens it, Tab and Shift+Tab cycle Apps, Commands, Settings and Install, and Escape closes the launcher. Mouse selection and scrolling work throughout the clipped results area. The panel adapts to the window height and shortens when search returns fewer results; unmatched queries show a useful no-results state. `/` remains a fallback shortcut when a VNC viewer intercepts Super. In Remmina, enable **Grab all keyboard events** to send Super shortcuts to ANGI while the viewer is focused.

On November 16, 2026, the launcher passed visual checks at 1280×900, 720×700 and 640×400, including app search, command scrolling, empty results, mouse activation and reaching the last app by keyboard in a small window. The Pi preview confirmed Super+Space, Wi-Fi search, opening Network and small-window navigation. The workspace tests and 71 GUI tests passed. The tested executable is installed on the development Pi, and both saved ANGI Remmina profiles enable keyboard grab; Right Ctrl twice toggles the grab with the workstation's current Remmina shortcuts. The previous executable link is saved under `/root/agx-launcher-backup/`.

## Keyboard (same chords as Omarchy's `bindings/tiling.lua`)

| Keys | Action |
| --- | --- |
| Mod+Arrows | Focus the window in that direction |
| Mod+Shift+Arrows | Swap with the window in that direction |
| Mod+Q or Mod+W | Close the window (reopen it from the menu) |
| Mod+J | Toggle the split direction |
| Mod+T | Float or tile the window |
| Mod+F | Fullscreen (bars hidden) |
| Mod+(a little)+F | Maximize (full width) |
| Mod+L | Toggle tiled/windowed mode; the top bar's layout icon is also clickable |
| Mod+1..5 | Switch workspace (plain 1..5 also works) |
| Mod+Shift+1..5 | Move the window to a workspace and follow it; add "a little" to stay |
| Mod+Tab, Mod+Shift+Tab | Next, previous workspace |
| Alt+Tab | Cycle window focus |
| Mod+`-` and Mod+`=` | Narrow or widen by 100px (Shift: height) |
| a little / a lot | 25px or 300px steps |
| Super+Space (`Mod+Space` with an explicit override), or `/` | Applications launcher; built-in tools under Settings, compatible software under Install, desktop/session actions under Commands |

## Customize key bindings

Open **Settings → Key bindings**, then choose **Keyboard** or **Mouse**. Search
for an action by name or by its current binding. Select an existing binding to
replace or remove it, or select **Add binding…** to give an action another input.

Press a key or click in the mouse recording area. You can hold modifiers with
the input or tap them separately while recording. The proposed shortcut stays
in the recorder until you choose **Accept**. If another action uses the same
input, the editor names it and asks you to choose **Replace binding**. You can
change the input device in the recorder to move an assignment between keyboard
and mouse. Plain left click stays available for clicking controls.

Bindings apply immediately and are saved for the signed-in account. **Restore
defaults…** applies to the selected input device. For Stream Deck buttons
configured to send a Hotkey, record the emitted shortcut in the Keyboard tab.

## Mouse

Click the tiled-layout icon in the top bar to switch layout: it is half opacity in windowed mode and full opacity in tiled mode, and hovering it for a moment shows a tooltip (same style as the Agent Angi OS/Project tip). Windowed mode lets you freely move windows by their title bars and resize their edges. The first switch preserves the current tiled positions; later switches restore your previous floating positions. Double-click a floating window's title bar to collapse its body into a movable header, then double-click again to restore its full size at the new position. A drag does not count as a double-click. Tiled windows display at full height. When you switch back to windowed mode, previously collapsed windows return as headers. Opening a collapsed app from Applications expands it. Display resizing keeps floating title bars reachable.

Verified on November 16, 2026 in both the software preview and on the Pi over VNC: clicking the layout label, collapsing to a 30-pixel header, moving that header, restoring the full body, returning to tiled, and restoring the saved floating position all passed. The workspace suite and 77 GUI tests passed, including double-click timing, drag rejection, collapse hit testing and keeping headers reachable after a display resize. The development Pi runs the tested executable; the previous executable link is retained under `/root/agx-windowed-backup/`.

- Click a window to focus it and click a workspace number to switch.
- Right-click anywhere on the desktop’s top bar and choose **Add workspace** to append a number while staying on the current workspace. ANGI starts with five workspaces and supports up to ten. **Super+Shift+number** creates any missing workspaces through that number, moves the focused window there, and follows it. Use **0** for workspace 10. From an empty workspace, the shortcut opens the new workspace directly. **Super+number** switches to a workspace that already exists.
- Right-click a workspace number and choose **Remove workspace** to remove it. Open windows move to the preceding workspace, or to the next one when removing the first. The numbers close up and at least one workspace remains.
- Drag a title bar to move a window. A tiled window shows a ghost and swaps with the tile it is dropped on; a floating window moves freely.
- Drag the gap between two tiles to resize the split. Drag the edges or corners of a floating window to resize it.
- Mod+left-drag moves a window from anywhere in it; Mod+right-drag resizes it. Mod+scroll changes workspace.
- The title bar buttons float or tile the window, enter fullscreen, and close it. Right-click a title bar to send that window to another workspace or close it. Sending a window leaves you on the current workspace. Use the arrow keys and Enter to choose, or Escape to dismiss.

## Layout

Dwindle tiling: a new window splits the focused tile along its longer side. Gaps are 10px at the screen edge and 10px (2 × 5) between tiles, with 2px borders and square corners, matching Omarchy's Hyprland defaults. The focused window has the cyan-to-green border.

Not implemented: window grouping (tabs), scratchpad, multiple monitors.

Native **Terminal** opens with **Super+Enter** or from Apps. Its Alacritty engine provides shell emulation inside the existing window system, with Bash, tabs, searchable scrollback and CLI tools. See [Terminal](terminal.md) for shortcuts, runtime packaging and validation.

Workspace 3 opens three independent Terminal windows in a tiled layout: one full-height terminal on the left and two stacked on the right. Hardware remains available in Apps. Choosing Terminal from Apps always adds a new window on the current workspace. Each terminal window has its own shell sessions, tabs, font size, selection and search state; there is no fixed application limit on terminal windows or tabs. Available memory and process resources determine practical capacity. Agent Angi uses a separate terminal window for Herdr.

**Super+Return** opens a new terminal window in the focused terminal tab’s current working directory. New windows opened from Apps and new tabs inherit that directory too. With no terminal focused, a new terminal starts in your home directory.

### Memory (knowledge graph)

Agent Angi's header has a graph button, **What ANGI remembers**. It opens ANGI's own knowledge graph of chats,
projects, system changes and agent work, where you can search, edit, pin, forget, audit what agents were
told, turn remembering off or clear it all. It works the same on every AI provider. See
[knowledge-graph.md](knowledge-graph.md).

### Services and updates

Open **Services** for the system software and supervised services. Installable
updates appear first, followed by items requiring a check and current software. The panel omits newer upstream versions without an ANGI installer.
The inventory JSON retains those comparisons for diagnostics.
Rows align software/version, installation date, last check, status and a compact
update button. Smaller panels stack the two timestamps beneath the software name.
Green means up to date. Orange means an update is available to install. A yellow
dot accompanies an explicit check result such as Not checked yet, Check failed,
Version unavailable, or Local build. There is no generic “status unknown” label.

Installation dates and check dates are separate. The desktop installation date
comes from its release ledger, and runtime installation dates from the atomic
current-link switch. Components without a recorded install date say so. The
**Last checked** timestamp records a real publisher request, including a failed
attempt; failure never becomes an up-to-date result. Per-account check results
persist in `~/.cache/angi/update-checks.json`. Checks share publisher results
between services owned by the same package and reuse results for one minute to
avoid rate-limit bursts. Startup reuses successful checks for six hours; the manual
Check action can refresh them sooner. Publisher rate limits honor the server’s reset
time, retain previously observed available versions, and retry automatically.
Visible timestamp text refreshes once a minute. At boot,
checks wait for a plausible system clock (also bounded by a recorded successful
check) before making HTTPS requests. The UI says **Waiting for clock**, then
resumes checks automatically after time synchronization. Transient network
failures likewise retry with bounded backoff. Waiting for clock synchronization
never stamps a publisher-check timestamp, and TLS certificate verification is
always retained. Recovery polling continues while the Services panel is closed
until the transient condition clears. After network time synchronizes, the network
service saves it to the Pi’s hardware clock for the next boot.

**Check for updates** refreshes upstream version metadata over HTTPS off the UI
thread and summarizes the results. Use `angi-update check` to also verify a
workstation-staged system image. The checker probes installed binaries
or libraries instead of assuming the source checkout's version was deployed.
GitHub releases are used for Herdr, OpenCode, Tailscale and tmux. GitHub tags are
used for Dropbear, BlueZ, OpenSSH, OpenSSL and Buildroot. The Raspberry Pi kernel
check reads its matching `rpi-X.Y.y` maintenance branch. Buildroot and OpenSSL
checks stay in their configured maintenance series. BusyBox, WPE WebKit and
wpa_supplicant are checked against their publishers' release indexes because
GitHub mirrors/releases do not reliably represent those projects. Preview
versions are excluded, and numeric version comparison handles tags correctly.
ANGI checks its GitHub main source tree; a customized installed tree is labeled
Local build rather than pretending it matches a published binary release.
The inventory JSON includes each component's exact `source_url`, installed/latest
versions, check timestamp and any request error.

The per-item icon opens a review overlay for an installable update or a pending
desktop restart. The overlay names the restart scope and explains session,
network or Bluetooth interruptions. Its Install action appears only for a supported installer. Enter
defaults to Cancel; Escape closes the review, and Tab stays within its actions.
Computer reboot requirements are activation advice, not a claim that an uninstalled
upstream release already requires a reboot. The desktop release is compared with
this account’s running GUI to detect a real pending desktop restart; a new pending
release automatically opens the overlay once. Nothing reboots or restarts
automatically. Kernel, base image, core BusyBox and OpenSSL updates require a reboot
under the system-image deployment path; other items identify their application or
service scope.

A guarded desktop source change can be installed through
`angi-update gui` after confirmation, retaining its revert guard and previous
release. To load it, use **Overview → Refresh desktop…**. Services has no desktop
refresh action. Upstream availability does not imply an automatic installer:
ANGI uses Buildroot, with no apt repository, and kernel/base updates still need a
recovery-capable system-image installer. Staged-image checksums establish
integrity, not authenticity or release freshness.

Supported desktop installs stream real stage transitions and build output through
`angi-update gui --progress`. The GUI shows an activity bar while building (with no
invented percentage), then installation and verification stages. Success is emitted
only after the copied file hash, selected release link and source record are checked.
A failed verification leaves or restores the prior selected release. **Hide** keeps
the job running; **View progress** reopens it. Completion is surfaced even after
hiding the window. A private receipt at `~/.cache/angi/update-job.json` survives
refreshes and records the installed release and whether desktop activation is still
pending. Read it with `angi-update job`. Services resumes any running installer
after a desktop refresh and shows its result when it finishes. Completed receipts
from earlier sessions stay in the CLI history. This progress view does not implement system
image installation. The base system and kernel are not downloaded or installed by
opening their information overlay.

CLI equivalents: `angi-update inventory` reads current inventory and cached
checks and retries due transient failures; `angi-update check` refreshes publisher checks and validates a staged image.

## Sound and radio

Open **Settings → Sound** to select the default output and microphone. Pair
Bluetooth devices in **Settings → Bluetooth** first. Select a compatible Bluetooth microphone in Sound to switch that device to
hands-free audio. Choose A2DP under Bluetooth mode for stereo playback.
The Pi 4's 3.5 mm jack is output-only. The Pi 5 can use HDMI, Bluetooth or a USB
audio adapter, including adapters with analog jacks.

ANGI plays a short confirmation tone after changes to the audio route. Use
**Play test tone** to check the current output again. **Play Radio Paradise**
opens CLIamp in ANGI Terminal. Use the output volume slider or the keyboard's
volume and mute keys to adjust the current output; the keys also slide a
volume indicator down from the top of the desktop for about a second and a half,
wherever you are, with a soft, short note whose pitch follows the level (none when you
mute). The input meter shows the
selected microphone's level while Sound is open. Running `cliamp` in an existing Terminal also starts
Radio Paradise and queues the channels published by
[CLIamp Radio](https://radio.cliamp.stream/stations), including Omarchy.
Press `/` to search the current playlist. To search the worldwide
[Radio Browser directory](https://www.radio-browser.info/), press uppercase
`R`, then `/`, enter a station name and press Enter. Esc returns to the station
browse view. The **cliamp radio** row opens CLIamp’s current channels.
CLIamp accepts its usual arguments.

Omarchy launches the upstream player. Its
[CLIamp package](https://github.com/omacom/omarchy-pkgs/blob/master/pkgbuilds/cliamp/PKGBUILD)
and [TUI guide](https://github.com/omacom/omarchy/blob/quattro/manual/21-tuis.md#cliamp)
are the reference for how Omarchy ships and opens it.

On a running development Pi 5, `scripts/deploy-sound.sh` stages the built ALSA,
PulseAudio, audio-enabled BlueZ and CLIamp runtime on NVMe. Build the
`alsa-plugins`, `alsa-utils`, `bluez5_utils` and `agx-cliamp` Buildroot targets
with the Pi 5 defconfig first. When enabling the BlueZ audio plugin in an
existing build, clean that package with `bluez5_utils-dirclean` before rebuilding.
Deploy the current init and desktop as well the first time. Init starts the new
Sound service at the next boot.
