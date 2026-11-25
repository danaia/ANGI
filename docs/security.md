# Security: wired SSH access and this computer's key

ANGI controls which wired links let another computer sign in over SSH, and shows the key
this computer offers to others. It lives in **Settings → Security** and as the second
last first-boot step: Wi-Fi, the administrator account, then Security.

## What it does

- **Public key.** Settings → Security shows this computer's public SSH key and its
  `SHA256:` fingerprint, with a **Copy key** button. Add the key to the other computer's
  `authorized_keys` so this one can sign in there. The private key stays in `/root/.ssh`.
- **Wired access.** Ethernet and USB cable each have a state, **SSH open** or **SSH closed**.
  A closed link refuses connections before SSH starts. **Allow access…** opens it:
  the first-boot step runs at the local console as root and asks for nothing more; from Settings
  it asks for the administrator's username and password. **Close** needs no password and ends
  sessions already open on that link.
- **Connected computer.** With an Ethernet cable connected, **Check connected computer**
  reads the other end's SSH host key and shows its fingerprint. **Trust this computer** pins
  it after you compare the fingerprint. The service scans again and refuses to save if the key
  changed in between. Pins go to `/etc/agx/peer_hosts` in `known_hosts` format.

- **Remote desktop (VNC).** A row turns the desktop's VNC server on or off. The server never
  listens on the network: it is loopback-only with no VNC password, so SSH is its access control
  (see [desktop.md](desktop.md#remote-access-vnc)). The row shows whether it is serving and how
  many viewers are connected. Turning it on asks for the same authorization as opening a link.
  The setting is the `AGX_GUI_VNC` line of `/etc/agx/gui.env`. Only the root sign-in screen reads
  that file and hands the value to your session, so a change applies at the **next sign-in**.
  **Refresh desktop** keeps the old value and does not start or stop VNC. Other lines in the file,
  and a port chosen by hand, are kept.
- **How to connect.** Plain sentences and ready-to-paste commands for what is connected and open
  right now: `ssh <user>@<address>` for each open cable, the SSH tunnel command and viewer address
  for remote desktop, a prompt to open a cable that is connected but closed, and a note that
  Wi-Fi access is not controlled here.
- **Close all remote access.** A two-step button. It closes Ethernet and USB SSH, ends their open
  sessions and turns remote desktop off. It needs no password. If you are connected over a cable,
  you are disconnected. Wi-Fi is not changed.

Wi-Fi, loopback and other interfaces keep their previous behavior; only Ethernet and USB
are gated.

## How it works

`agx-securityd` (crate `agx-security`) owns port 22. Dropbear listens on `127.0.0.1:2022`
only. The daemon opens one listening socket per allowed interface, bound to that interface
with `SO_BINDTODEVICE`, and relays accepted connections to Dropbear. This works whatever
address the link has, so it needs no kernel firewall. It rescans interfaces every two
seconds, so hot-plugged links follow the policy. A closed link has no socket at all.
Sessions relayed through it show `127.0.0.1` as their source in Dropbear's log.

`agx-init` starts `agx-securityd` and moves Dropbear behind it only when
`/usr/sbin/agx-securityd` exists. Without it, Dropbear listens on port 22 as before.

The control socket is `/run/agx/security.sock` (root and `agx-admin`). Requests: `status`,
`open`, `close`, `vnc`, `close_all`, `peer`, `pin`.

## From a shell

`agx-securityd` with arguments is a client for the running service, as root without a password:
`agx-securityd status`, `vnc on|off`, `close-all`, `open|close ethernet|usb`, `peer`. As another user
set `AGX_SECURITY_USER` and `AGX_SECURITY_PASSWORD`. A request without valid credentials is
refused with "That username and password did not match."

To prove remote desktop shows the screen, tunnel to it and save one frame:

```bash
ssh -N -L 5941:127.0.0.1:5900 <user>@<address> &
scripts/vnc-snapshot.py 127.0.0.1 5941 frame.png
```

## The policy file

Two lines, `ethernet=open|closed` and `usb=open|closed`.

| Location | Role |
| --- | --- |
| `/var/lib/agx-data/security.conf` | The saved choice. Wins when the data volume is mounted. |
| `/etc/agx/security.conf` | The image default (`closed`). A card prepared by `provision-sd.py` or `provision-iso.py` with the person's workstation key carries `open` instead, because that key is already pinned and authorized. |

On first run, with the data volume mounted, the daemon copies the policy in force to the data
volume. System updates replace the whole root filesystem, so this keeps an earlier choice from
being reset. With no file anywhere, which is an older system, both links stay open and that is
recorded. A damaged file fails closed.

## Recovery

If Ethernet SSH is closed and unreachable, use the local console (Settings → Security), the
serial console, or Wi-Fi. To open a link from a root shell:
`printf 'ethernet=open\nusb=open\n' > /var/lib/agx-data/security.conf`. The change applies within
two seconds.

## Checks

```bash
./scripts/cargo.sh test -p agx-security
./scripts/cargo.sh test -p agx-init
./scripts/cargo.sh test --manifest-path crates/agx-gui/Cargo.toml security
```
