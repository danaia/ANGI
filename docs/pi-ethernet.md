# ANGI development Pi: Ethernet access and control

This is the shared procedure for Claude and Codex on the development workstation.
Run workstation blocks in **Bash**, from `/home/dana/dev/AGX`. Commands passed
to `pi_ssh` execute on the Pi. If already working locally on the Pi, use
`apps/agent/skills/angi-development/SKILL.md` instead of reconnecting to yourself.

## Connection identity

| Setting | Established value |
| --- | --- |
| Transport | Direct RJ45 cable between workstation and Pi; no router required |
| Workstation | `enp1s0`, `10.78.0.1/24`, NetworkManager profile `agx-ethernet` |
| Pi | `eth0`, `10.78.0.2/24`, SSH destination `root@10.78.0.2` |
| Private key | Workstation `~/.ssh/id_ed25519`; never copy it to the Pi |
| Host-key lookup | `HostKeyAlias=10.77.0.2`, using the existing known-host entry |
| Recorded host key | ED25519 `SHA256:cgKA/OnkSFz5mLIuid7kuuRLhFk7wCP1+foOHNuAstQ` |
| Desktop | Pi `127.0.0.1:5900` through OpenSSH to workstation `127.0.0.1:5941` |
| Persistent data | Pi NVMe mounted at `/var/lib/agx-data` |

`10.77.0.2` is the separate USB recovery endpoint and the recorded SSH identity
alias; it is **not** the RJ45 destination. Wi-Fi `192.168.1.184` was observed
November 16, 2026, via DHCP and is not a guaranteed address. Ethernet route,
key-authenticated SSH, Pi `eth0` and the NVMe mount were verified November 17,
2026. Recheck connectivity each session; do not infer it from this record.

## Connect and verify

Check the workstation route and existing profile first:

```bash
cd /home/dana/dev/AGX
ip route get 10.78.0.2
nmcli -f GENERAL.STATE,GENERAL.CONNECTION,IP4.ADDRESS device show enp1s0
```

Expect `10.78.0.2 dev enp1s0 src 10.78.0.1`. If the existing profile is inactive,
run `nmcli connection up agx-ethernet`, then repeat the checks. If route or source
still differs, diagnose the cable/profile before issuing remote mutations.
The established profile uses manual IPv4, `ipv4.never-default yes`, disabled
IPv6, autoconnect enabled and priority 10. Keep workstation Internet routing
on its existing other connection. Do not create duplicate profiles.

Load this connection block in each workstation Bash session that needs access:

```bash
export AGX_SSH_HOST=root@10.78.0.2
export AGX_SSH_KEY="$HOME/.ssh/id_ed25519"
export AGX_SSH_HOST_KEY_ALIAS=10.77.0.2
PI_SSH_OPTIONS=(
    -i "$AGX_SSH_KEY" -o IdentitiesOnly=yes
    -o BatchMode=yes -o ConnectTimeout=8 -o ConnectionAttempts=1
    -o ServerAliveInterval=5 -o ServerAliveCountMax=3
    -o "HostKeyAlias=$AGX_SSH_HOST_KEY_ALIAS" -o StrictHostKeyChecking=yes
)
pi_ssh() { ssh "${PI_SSH_OPTIONS[@]}" "$AGX_SSH_HOST" "$@"; }
pi_scp() { scp -O "${PI_SSH_OPTIONS[@]}" "$@"; }
pi_ssh 'hostname; uname -m; ip -4 addr show dev eth0; mountpoint /var/lib/agx-data'
```

Ethernet SSH is controlled by Settings → Security (see [security.md](security.md)). A device whose
Ethernet link is closed refuses the connection; open it from the local console first.

Expect hostname `agx`, architecture `aarch64`, `eth0` at `10.78.0.2/24` and a
mounted data volume. On a host-key mismatch, pause remote work and verify the
replacement key before resuming. If batch authentication fails after host
verification, diagnose the existing login key and device account.

### Verify a replacement host key

For a card prepared with `scripts/provision-sd.py`, use its private
`known_hosts` file for the first strict SSH connection. If provisioning
reported a different workstation identity, set `AGX_SSH_KEY` to that path
before connecting:

```bash
pi_ssh -o UserKnownHostsFile=/path/to/private-install/known_hosts hostname
```

Use the Pi's local ANGI Terminal to read the replacement Ed25519 fingerprint:

```sh
angi-root /usr/bin/dropbearkey -y -f /etc/dropbear/dropbear_ed25519_host_key | grep '^Fingerprint:'
```

An existing independently trusted recovery session can read the same key. A
private provisioning record can also supply the fingerprint when it belongs to
the exact installed image for this Pi. A key presented by the Ethernet peer or
returned by `ssh-keyscan` needs one of those independent sources to establish
identity.

When the fingerprint matches, pin the verified public key for the existing
`10.77.0.2` host-key alias and the `10.78.0.2` entry some deployment scripts
use directly. Scope the replacement to those two entries and keep the
workstation's other host keys. Repeat the strict connection and device checks
above before deploying. Standing administrator authorization covers this
verified trust update and the subsequent deployment.

## Commands and file transfer

```bash
pi_ssh 'agx services --json'
pi_ssh 'tail -n 80 /run/agx/gui.log'
pi_ssh 'readlink -f /usr/bin/agx-gui'
```

For a transfer, use `pi_scp ./LOCAL_FILE "$AGX_SSH_HOST:/var/lib/agx-data/REMOTE_FILE"`
or `pi_scp "$AGX_SSH_HOST:/REMOTE_FILE" ./LOCAL_FILE`, replacing the file paths
with the intended files. Dropbear supports legacy SCP, so retain `-O` in the
function. Stage application payloads on NVMe. `make stage-system-update` transfers
system image candidates to `/var/lib/agx-data/agx-update` and verifies their
transfer digest before publishing them.
Use single quotes around remote shell programs; for example,
`pi_ssh 'readlink -f /usr/bin/agx-gui'`. Local variables outside those quotes
expand on the workstation.

The Pi runs minimal Buildroot with custom Rust init: use `agx services --json`
and the repository's component procedures, not `apt` or `systemctl`.
Root SSH administers the device; desktop applications run as the authenticated
desktop account. Do not launch a replacement desktop as root from SSH.

## See and control the desktop

On the workstation, using the connection environment above:

```bash
scripts/vnc-tunnel.sh up ethernet
scripts/vnc-tunnel.sh status ethernet
```

Connect the VNC viewer to `127.0.0.1:5941`. Keep Remmina's built-in SSH tunnel
disabled; its libssh tunnel has crashed on this workstation. The existing
OpenSSH tunnel carries screen updates, keyboard and pointer input. Keep the
Pi VNC listener on `127.0.0.1:5900`; do not expose it on `eth0`. Do not send
desktop input unrelated to the user's task.

The script reuses the enabled workstation user service if installed. For that
case, inspect `systemctl --user status angi-vnc-tunnel@ethernet.service` on
the workstation. `scripts/vnc-tunnel.sh down ethernet` stops the tunnel when
requested; leave an existing user tunnel running after ordinary verification.
If no image arrives, check the RFB greeting as described below and inspect
`/run/agx/gui.log`; an open local port alone does not prove a working desktop.
See [desktop configuration and renderer selection](desktop.md#remote-access-vnc)
before changing GUI/VNC configuration.

## Deploy and activate requested changes

Read the affected component's deployment script before running it. With the
connection block loaded, run the appropriate target, for example `make deploy-gui`
or `make deploy-tui`. Export all three `AGX_SSH_*` variables: older deployment
scripts otherwise default to USB. Scripts that support the host-key alias use
it; older ones rely on the existing known-host entry for `10.78.0.2`. If that
entry is missing or mismatched, resolve trust before using those scripts.

Use `make deploy-gui-only BOARD=rpi5` for a desktop binary change on a device
with the current agent runtime and a recorded, compatible desktop release.
Then refresh from the authenticated session and check `angi-update status`.

Preserve the current executable/link and relevant configuration for rollback
before replacement. `deploy-gui.sh` keeps the first installed binaries under
`/root/agx-gui-backup`; those are not necessarily the immediately preceding
release. Record `readlink -f /usr/bin/agx-gui` before each GUI deployment and
retain its target. Use versioned NVMe application runtimes and atomic link
replacement. Preserve `/lib`, `/lib64`, libc, the loader and regular SD boot
scripts, especially `/etc/init.d/S12agx-data`.

`make deploy-gui` also records the release in the device's ledger and syncs the workstation source into
`/var/lib/agx-data/angi-dev` (see [agent-swarm.md](agent-swarm.md#os-updates-angi-update-and-the-revert-guard)), so
on-device updates cannot revert it. Run `make deploy-agent` first on a device that lacks `angi-update`.

For a GUI-only update, use **Overview → Refresh desktop… → confirm** in the
authenticated desktop when appropriate. This closes its terminal/browser
sessions and execs the installed GUI as the same user. Do not emulate refresh
with `killall`; killing the session can return it to login. Reconnect VNC if
necessary. Init/kernel/boot changes need a reboot; the user has authorized it
when needed for requested changes. Record the boot ID, run `pi_ssh 'sync; reboot'`,
then allow up to 120 seconds for recovery, retrying `pi_ssh 'cat /proc/sys/kernel/random/boot_id'`
at five-second intervals. A reboot can close SSH before it returns a status;
verify a changed boot ID rather than treating disconnection as success.

After activation, check `agx services --json`, `/run/agx/gui.log`, and the running
GUI executable via `/proc/PID/exe` for the PID reported by `pidof agx-gui`.
Compare it with the installed link and verify the actual requested behavior.
Report VNC checks separately from physical HDMI/keyboard/mouse checks.

## Bounded failure diagnosis and recovery

1. Check cable/carrier with `ip link show enp1s0`, then the route and profile
   above. Activate the existing profile once if inactive. Retry SSH once after
   correcting that cause. Do not cycle networking on the Pi through its only
   working connection or reset workstation networking globally.
2. If SSH fails, distinguish timeout/refusal, authentication and host-key
   errors. A single `ssh -v` invocation using `PI_SSH_OPTIONS` can clarify the
   failing layer. Preserve trust and credentials; do not probe other hosts.
3. If SSH works but VNC fails, run `scripts/vnc-tunnel.sh status ethernet`,
   inspect the workstation service status if present, and read the Pi GUI log.
   Check that connecting to `127.0.0.1:5941` returns an `RFB ` greeting, with a
   five-second socket timeout. This verifies the tunnel and VNC listener, not
   screen rendering or input. Repair only the identified failing component.
4. After the reboot recovery deadline, stop retries and report the failing
   layer. USB `root@10.77.0.2` or a verified current Wi-Fi address can be chosen
   explicitly for recovery using the same key/host identity; announce the
   transport change and repeat identity checks. Do not silently fall back.
5. If the Pi boots but the new GUI fails, restore the recorded previous GUI
   link atomically over trusted SSH and refresh/reboot as appropriate. If NVMe
   is missing, follow [data-volume recovery](desktop.md#boot-and-graphical-login).
   If SSH never returns, use the documented local/SD recovery in
   [desktop boot recovery](desktop.md#rendering-and-performance) rather than reflashing or
   changing boot libraries speculatively. Preserve user data.
