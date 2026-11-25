#!/usr/bin/env bash
# SSH tunnel to the Pi's desktop VNC server, run with OpenSSH instead of Remmina's
# built-in libssh tunnel (which has crashed Remmina while closing a tunnel).
#
#   scripts/vnc-tunnel.sh run|up|down|status ethernet|wifi|beelink
#
# The tunnel listens on 127.0.0.1 only: 5941 for Ethernet, 5942 for Wi-Fi, 5944 for the Beelink
# Mini S (the Intel machine on the Ethernet cable). The Beelink checks its host key against the
# private install directory's known_hosts (AGX_SSH_KNOWN_HOSTS, else the newest
# ~/angi-install/private/angi-beelink-mini-s-install-*/known_hosts), never the Pi's old entry. Point the
# VNC viewer at that local port with its own tunnel turned off.
#   run     stay in the foreground holding the tunnel (for the systemd user service in
#           scripts/systemd/, which restarts it after a Pi reboot or a dropped link)
#   up      start a background tunnel (idempotent); down stops it; status reports it
# The Pi's VNC server itself listens on its loopback port 5900 (see docs/desktop.md).
set -euo pipefail

ACTION=${1:-}
LINK=${2:-}
KEY=${AGX_SSH_KEY:-"$HOME/.ssh/id_ed25519"}
USER_AT=${AGX_SSH_USER:-root}
HOST_KEY_ALIAS=${AGX_SSH_HOST_KEY_ALIAS:-10.77.0.2}
REMOTE_PORT=${AGX_VNC_REMOTE_PORT:-5900}

case "$LINK" in
    ethernet) HOST=${AGX_ETHERNET_HOST:-10.78.0.2}; PORT=${AGX_ETHERNET_PORT:-5941} ;;
    wifi) HOST=${AGX_WIFI_HOST:-192.168.1.184}; PORT=${AGX_WIFI_PORT:-5942} ;;
    beelink) HOST=${AGX_BEELINK_HOST:-10.78.0.2}; PORT=${AGX_BEELINK_PORT:-5944} ;;
    *) echo "usage: ${0##*/} run|up|down|status ethernet|wifi|beelink" >&2; exit 2 ;;
esac
FORWARD="$PORT:127.0.0.1:$REMOTE_PORT"

# The tunnel is found by its forward argument, so nothing depends on a pid file that a
# crash could leave stale or a reused pid could make wrong.
tunnel_pid() { pgrep -u "$(id -u)" -f -- "^ssh .* -L $FORWARD " | head -n 1; }

port_open() { (exec 3<>"/dev/tcp/127.0.0.1/$PORT") 2>/dev/null; }

SSH_OPTIONS=(
    -i "$KEY" -o BatchMode=yes -o ConnectTimeout=8
    -o GatewayPorts=no
    -o ExitOnForwardFailure=yes -o ServerAliveInterval=5 -o ServerAliveCountMax=3
    -o StrictHostKeyChecking=yes
)
if [[ $LINK == beelink ]]; then
    KNOWN=${AGX_SSH_KNOWN_HOSTS:-$(ls -t "$HOME"/angi-install/private/angi-beelink-mini-s-install-*/known_hosts 2>/dev/null | head -n 1)}
    [[ -n $KNOWN && -f $KNOWN ]] || { echo "No install known_hosts found; set AGX_SSH_KNOWN_HOSTS." >&2; exit 1; }
    SSH_OPTIONS+=(-o IdentitiesOnly=yes -o "UserKnownHostsFile=$KNOWN")
else
    SSH_OPTIONS+=(-o "HostKeyAlias=$HOST_KEY_ALIAS")
fi

# Once installed, systemd owns the lifecycle. In particular, stopping only its
# ssh process would cause Restart=always to bring the tunnel straight back.
UNIT="angi-vnc-tunnel@$LINK.service"
if [[ "$ACTION" == up || "$ACTION" == down ]] &&
    command -v systemctl >/dev/null && systemctl --user is-enabled --quiet "$UNIT" 2>/dev/null; then
    if [[ "$ACTION" == up ]]; then
        exec systemctl --user start "$UNIT"
    else
        exec systemctl --user stop "$UNIT"
    fi
fi

case "$ACTION" in
    run)
        exec ssh -N "${SSH_OPTIONS[@]}" -L "$FORWARD" "$USER_AT@$HOST"
        ;;
    up)
        if tunnel_pid >/dev/null && port_open; then exit 0; fi
        if pid=$(tunnel_pid); then kill "$pid" 2>/dev/null || true; fi
        # -f makes ssh detach only after the forward is listening (with ExitOnForwardFailure),
        # so a zero exit means the port is ready. ServerAlive makes a tunnel to a vanished
        # Pi end by itself within about 15 seconds.
        if ! ssh -f -N "${SSH_OPTIONS[@]}" -L "$FORWARD" "$USER_AT@$HOST" </dev/null >/dev/null 2>&1; then
            echo "SSH to $HOST failed; is the Pi reachable?" >&2
            exit 1
        fi
        port_open || { echo "Tunnel to $HOST did not open port $PORT." >&2; exit 1; }
        ;;
    down)
        if pid=$(tunnel_pid); then kill "$pid" 2>/dev/null || true; fi
        ;;
    status)
        if pid=$(tunnel_pid) && port_open; then
            echo "$LINK tunnel up: 127.0.0.1:$PORT -> $HOST (pid $pid)"
        else
            echo "$LINK tunnel down"; exit 1
        fi
        ;;
    *) echo "usage: ${0##*/} run|up|down|status ethernet|wifi|beelink" >&2; exit 2 ;;
esac
