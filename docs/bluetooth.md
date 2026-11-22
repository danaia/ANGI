# Bluetooth

ANGI finds, pairs and connects Bluetooth devices through one system service. The desktop's
**Settings → Bluetooth** page, the `agx bluetooth` command and agents all use that service, so they
see the same devices and obey the same rules.

```
Settings → Bluetooth      agx bluetooth …      agents
        \                       |                /
         '--- /run/agx/bluetooth.sock (JSON) ---'
                          agx-bluetoothd          policy, approvals, prompts
                               | Backend trait
                          BlueZ (BlueR)           bluetoothd over the system D-Bus
                               |
                     Linux Bluetooth + Broadcom controller
```

## Status

| Piece | State |
| --- | --- |
| `agx-bluetoothd`, CLI, Settings page | Built and tested on the workstation against a real BlueZ adapter (status, a short scan, every authorization gate). Pairing and connecting to real devices were **not** exercised. |
| Image changes (BlueZ, D-Bus, firmware, boot, supervision) | **Written but not built or booted.** Nothing is verified on the Pi yet. |

The dev Pi's current kernel already ships the Bluetooth modules (`bluetooth`, `hci_uart`, `btbcm`,
`uhid`, …); what it lacks is the userland, the radio firmware and the boot parameter below. Until an
image with these changes is built and flashed, Settings → Bluetooth shows "The Bluetooth service is not
running on this device."

## Using it

`agx bluetooth` (add `--json` for machine-readable output):

| Command | What it does |
| --- | --- |
| `status` (default) | Adapter state and what is connected |
| `devices [--paired]` | Known and nearby devices |
| `scan [--seconds N]` | Search for up to N seconds (1–45), then list |
| `on` / `off` | Radio power. `off` is refused unless it comes from the desktop |
| `pair ADDRESS` | Pair, trust and connect |
| `connect ADDRESS` / `disconnect ADDRESS` | Connect or disconnect |
| `trust ADDRESS` / `untrust ADDRESS` | Reconnect automatically, or stop |
| `forget ADDRESS` | Remove the device and its pairing keys |

Addresses look like `AA:BB:CC:DD:EE:FF`; `devices` lists them. Commands that need the person's approval
(see below) wait for the answer and report the outcome; `--no-wait` returns once the request is queued.

In Settings, click **Bluetooth** in the sidebar (or press PageDown; PageUp returns to Mouse & pointer).
Up/Down select a device, Enter performs its main action, `T` toggles auto-connect, Delete removes it (a
second press confirms), Tab moves between the power switch, Search, the list and Refresh. A pairing
question or an agent's request takes Enter (accept) and Escape (decline) while it is showing.

## Who may do what

Pairing gives a device lasting access, so it is the person's decision. The service decides who is asking
from the connecting process (the desktop binary is `agx-gui`):

| Request | From the desktop | From `agx`, an agent or a script |
| --- | --- | --- |
| Status, search, power on, disconnect, stop auto-connect | Allowed | Allowed |
| Connect to an already paired device | Allowed | Allowed |
| Connect to a new device, **pair**, auto-connect on, **forget** | Allowed | Held until the person allows it in Settings (60 seconds) |
| Power off, answer a pairing prompt, allow or decline a request | Allowed | Refused |

Pairing from Settings pairs, trusts (so the device reconnects by itself) and connects. Pairing questions
from BlueZ — compare a code, type a code on the device, enter a PIN or passkey, allow an incoming
device — appear as a card on the page and are refused if unanswered for 60 seconds.

This guards against unattended action by an agent or script that goes through the service. It is not a
boundary against a hostile program running as the same account, which could already drive the desktop.

## Wire protocol

One JSON request per line, one JSON reply per line, on `/run/agx/bluetooth.sock` (root and the ANGI
administration group, like the Wi-Fi service). `AGX_BLUETOOTH_SOCKET` redirects both service and clients
for desktop previews. Requests are `{"cmd":"scan","on":true}`, `{"cmd":"pair","address":"…"}` and so on;
every reply is `{"ok":…,"message":"…","status":{…}}` with the adapter, devices, pending prompt and
approvals. Requests are bounded (4 KiB), addresses and PINs are validated, and at most 16 connections
are served at once.

## Image changes (unbuilt)

- `buildroot/configs/agx_rpi5_defconfig`: `dbus`, `bluez5_utils` (client, monitor, HID/HOG), and the
  Raspberry Pi Bluetooth firmware (`BCM4345C0.hcd`).
- `buildroot/board/agx/config.txt`: `dtparam=krnbt=on` enables the controller node that this Pi's device
  tree already contains (it is off by default here).
- `etc/modules-load.d/agx-bluetooth.conf`: loads `hci_uart` and `uhid` at boot.
- `apps/bluetooth/run.sh`, installed to `/usr/libexec/agx-bluetooth/run`, is the one `bluetooth` service
  that `agx-init` supervises: it starts the system D-Bus, then `bluetoothd`, then `agx-bluetoothd`, and
  restarts all three together if any dies. Pairing keys go to `/var/lib/agx-data/bluetooth` so they
  survive reboots and system updates (the root filesystem is used if the data volume is missing).
  Buildroot's own D-Bus and BlueZ init scripts are not used; `agx-init` runs a fixed script list.
- `agx-bluetoothd` links `libdbus-1` through BlueR, so `make network-build` and `make init-build` now
  exclude it; `make bluetooth-build` cross-builds it once Buildroot has put D-Bus in the sysroot.
- `scripts/check-image.py` verifies all of the above after a build.

Things only a real build and boot can confirm: that BlueZ configures against this image's `libudev-zero`,
that the controller binds and loads its firmware, that rfkill unblocks, and that a Bluetooth keyboard's
input reaches the desktop through the existing device-event relay. Pi 4 and Pi 5 images with the Sound service register A2DP playback and native
headset profiles with BlueZ. After pairing, choose the connected speaker in
**Settings → Sound → Output**. Select a compatible microphone under **Input** to switch that device to
hands-free audio.
Headset mode uses a lower-quality audio link than A2DP.

## Trying it on a workstation

With a BlueZ adapter and the system `bluetooth` service running:

```bash
export AGX_BLUETOOTH_SOCKET=/tmp/agx-bluetooth.sock
scripts/cargo.sh build -p agx-bluetoothd -p agx-cli
.cache/cargo-target/debug/agx-bluetoothd &
.cache/cargo-target/debug/agx bluetooth scan
AGX_GUI_MOD=alt .cache/cargo-target/debug/agx-gui     # same variable, then Settings → Bluetooth
```

The daemon needs no root in this mode. The service's policy tests use a fake stack
(`cargo test -p agx-bluetoothd`); the Settings page's tests run in the headless desktop test and can
write screenshots with `AGX_BLUETOOTH_SCREENSHOT=/path/shot.ppm`.
