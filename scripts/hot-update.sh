#!/usr/bin/env bash
# Hot update: compile only the ANGI binaries that changed on the build machine, send them over
# Ethernet to a RUNNING device, and activate them there. No image build, no reflash.
#
#   AGX_SSH_HOST=root@10.78.0.2 AGX_SSH_KNOWN_HOSTS=<install-dir>/known_hosts scripts/hot-update.sh
#   scripts/hot-update.sh --no-build      # push what the last build produced
#   scripts/hot-update.sh --check         # show what would change; change nothing
#   scripts/hot-update.sh --rollback      # restore the previous binaries
#
# What it updates, and how each takes effect:
#   agx-gui          the desktop. Sent SIGUSR1, which refreshes it exactly as Settings > Refresh
#                    desktop does (settings saved, then the new binary is exec'd in the same session).
#   agx-securityd    the security service. Tried first on a spare port; then killed so agx-init
#                    restarts it. SSH over every link drops for a few seconds and comes back.
#   agx              the command line tool. Takes effect the next time it runs.
# agx-init, agx-netd and agx-bluetoothd are not touched: they need a reboot, which this does not do.
#
# Every replaced binary is first copied to /root/hot-backup/<time>/ on the device. A binary that
# does not come up healthy is put back. Strict host-key checking is always on: point
# AGX_SSH_KNOWN_HOSTS at the install directory's known_hosts. Do not close the Ethernet link that
# this connection uses (Settings > Security) while it runs.
#
# Build machine: AGX_BUILD_HOST (default "dana"), AGX_BUILD_TREE (default angi-build/mini-s,
# relative to its home), AGX_BOARD (default beelink-mini-s). The tree must already have been built
# once (make build), so the Buildroot output, toolchain and vendored crates exist.
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
BUILD_HOST=${AGX_BUILD_HOST:-dana}
BUILD_TREE=${AGX_BUILD_TREE:-angi-build/mini-s}
BOARD=${AGX_BOARD:-beelink-mini-s}
DEVICE=${AGX_SSH_HOST:?set AGX_SSH_HOST, for example root@10.78.0.2}
KEY=${AGX_SSH_KEY:-$HOME/.ssh/id_ed25519}
KNOWN=${AGX_SSH_KNOWN_HOSTS:?set AGX_SSH_KNOWN_HOSTS to the install directory known_hosts (strict checking)}
JOBS=${JOBS:-16}
STAGE="$ROOT/.cache/hot-update/$BOARD"
BUILD=1 CHECK=0 ROLLBACK=0
for argument in "$@"; do
    case $argument in
        --no-build) BUILD=0 ;;
        --check) CHECK=1 ;;
        --rollback) ROLLBACK=1; BUILD=0 ;;
        -h|--help) sed -n '2,28p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "hot-update: unknown option $argument" >&2; exit 2 ;;
    esac
done

die() { echo "hot-update: $*" >&2; exit 1; }
say() { printf '\n== %s\n' "$*"; }
[[ -f $KNOWN ]] || die "known_hosts not found: $KNOWN"
SSH_OPTIONS=(-i "$KEY" -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=8 -o ConnectionAttempts=1
    -o "UserKnownHostsFile=$KNOWN" -o StrictHostKeyChecking=yes -o ServerAliveInterval=5 -o ServerAliveCountMax=3)
dev() { ssh "${SSH_OPTIONS[@]}" "$DEVICE" "$@"; }
push() { scp -O -q "${SSH_OPTIONS[@]}" "$1" "$DEVICE:$2"; }

# name -> path inside the build output and on the device
declare -A PATHS=([agx-gui]=usr/bin/agx-gui [agx-securityd]=usr/sbin/agx-securityd [agx]=usr/bin/agx)
ORDER=(agx agx-securityd agx-gui)

wait_for_device() {
    local seconds=$1 deadline=$((SECONDS + $1))
    while ((SECONDS < deadline)); do
        dev true 2>/dev/null && return 0
        sleep 2
    done
    return 1
}

say "Device"
dev 'echo "$(hostname) $(uname -m)"; df -h / | tail -1' || die "cannot reach $DEVICE with the pinned host key"

if ((ROLLBACK)); then
    say "Rollback"
    latest=$(dev 'ls -1d /root/hot-backup/* 2>/dev/null | tail -1')
    [[ -n $latest ]] || die "no backup on the device"
    echo "restoring from $latest"
    dev "set -e; for f in $latest/*; do n=\$(basename \$f); case \$n in agx-gui) d=/usr/bin/agx-gui;; agx-securityd) d=/usr/sbin/agx-securityd;; agx) d=/usr/bin/agx;; *) continue;; esac; cp \$f \$d.rollback; chmod 755 \$d.rollback; mv \$d.rollback \$d; echo restored \$d; done"
    echo "Restored files are in place. Restart the desktop (Overview > Refresh desktop) and the security service (kill it; agx-init restarts it) to run them."
    exit 0
fi

if ((BUILD)); then
    say "Send source to $BUILD_HOST"
    rsync -a --relative --exclude target --exclude .cache --exclude __pycache__ \
        crates Cargo.toml Cargo.lock rust-toolchain.toml brand apps/agent/apps.json \
        "$BUILD_HOST:$BUILD_TREE/"
    say "Compile on $BUILD_HOST (the Buildroot packages only, no image)"
    ssh -o BatchMode=yes "$BUILD_HOST" bash -s -- "$BUILD_TREE" "$BOARD" "$JOBS" <<'REMOTE'
set -euo pipefail
cd "$HOME/$1"
BOARD=$2 JOBS=$3
OUT=$(python3 -I scripts/detect-board.py --field "$BOARD" output)
[[ $OUT == /* ]] || OUT=$PWD/$OUT
[[ -d ${AGX_BUILD_OUTPUT:-$OUT} ]] && OUT=${AGX_BUILD_OUTPUT:-$OUT}
SRC=$(ls -d "$PWD"/.cache/src/buildroot-* | head -1)
export BOARD
PATH="$OUT/host/bin:$PATH" scripts/prepare-rust-package.sh --desktop
for target in agx-rebuild agx-gui-rebuild; do
    make -C "$SRC" O="$OUT" BR2_EXTERNAL="$PWD/buildroot" BR2_JLEVEL="$JOBS" "$target" 2>&1 | tail -3
done
echo "output: $OUT"
REMOTE
fi

say "Collect the build"
mkdir -p "$STAGE"
OUT_REMOTE=$(ssh -o BatchMode=yes "$BUILD_HOST" bash -s -- "$BUILD_TREE" "$BOARD" <<'REMOTE'
cd "$HOME/$1"
OUT=$(python3 -I scripts/detect-board.py --field "$2" output)
[[ $OUT == /* ]] || OUT=$PWD/$OUT
[[ -d ${AGX_BUILD_OUTPUT:-} ]] && OUT=$AGX_BUILD_OUTPUT
echo "$OUT"
REMOTE
)
for name in "${ORDER[@]}"; do
    ssh -o BatchMode=yes "$BUILD_HOST" "cat '$OUT_REMOTE/target/${PATHS[$name]}'" > "$STAGE/$name"
    chmod 755 "$STAGE/$name"
    file "$STAGE/$name" | grep -q 'x86-64' || die "$name is not an x86-64 binary; wrong BOARD?"
done

say "What changed"
CHANGED=()
for name in "${ORDER[@]}"; do
    new=$(sha256sum "$STAGE/$name" | cut -d' ' -f1)
    old=$(dev "sha256sum /${PATHS[$name]} 2>/dev/null | cut -d' ' -f1" || true)
    if [[ $new == "$old" ]]; then echo "  same      $name"; else echo "  CHANGED   $name"; CHANGED+=("$name"); fi
done
((${#CHANGED[@]})) || { echo "Nothing to update."; exit 0; }
((CHECK)) && { echo "(--check: nothing was changed)"; exit 0; }

STAMP=$(date +%Y%m%d-%H%M%S)
BACKUP=/root/hot-backup/$STAMP
dev "mkdir -p $BACKUP"
for name in "${CHANGED[@]}"; do
    say "Install $name"
    path=/${PATHS[$name]}
    dev "cp -p $path $BACKUP/$name"
    push "$STAGE/$name" "$path.hot"
    dev "chmod 755 $path.hot"
    new=$(sha256sum "$STAGE/$name" | cut -d' ' -f1)
    [[ $(dev "sha256sum $path.hot | cut -d' ' -f1") == "$new" ]] || { dev "rm -f $path.hot"; die "$name arrived damaged; nothing replaced"; }

    case $name in
    agx)
        dev "$path.hot --version >/dev/null && mv $path.hot $path" || { dev "rm -f $path.hot"; die "new agx does not run; kept the old one"; }
        echo "agx replaced" ;;

    agx-securityd)
        # Try the exact binary on a spare port, with private state, before it replaces the live one.
        port=2290
        test_pid=$(dev "rm -rf /tmp/hotsec && mkdir -p /tmp/hotsec && printf 'ethernet=open\nusb=open\n' > /tmp/hotsec/security.conf && dropbearkey -t ed25519 -f /tmp/hotsec/host_key >/dev/null 2>&1; AGX_SECURITY_DIR=/tmp/hotsec AGX_SECURITY_SOCKET=/tmp/hotsec/s.sock AGX_SSH_GATE_PORT=$port $path.hot >/tmp/hotsec/log 2>&1 </dev/null & echo \$!")
        sleep 4
        banner=$(timeout 8 ssh-keyscan -T 5 -t ed25519 -p $port "${DEVICE#*@}" 2>/dev/null | head -1 || true)
        dev "kill $test_pid 2>/dev/null; sleep 1; tail -3 /tmp/hotsec/log 2>/dev/null; rm -rf /tmp/hotsec" || true
        [[ -n $banner ]] || { dev "rm -f $path.hot" || true; die "the new agx-securityd did not relay SSH on spare port $port; live one untouched"; }
        echo "spare-port test passed ($port relayed an SSH host key)"
        dev "mv $path.hot $path"
        old_pid=$(dev "pidof agx-securityd")
        # Return before the kill so this command finishes cleanly; the kill ends every SSH session.
        dev "nohup sh -c 'sleep 1; kill $old_pid' >/dev/null 2>&1 </dev/null &" || true
        sleep 4
        wait_for_device 40 || { echo "SSH did not return; the supervisor should restart agx-securityd. Use the console." >&2; exit 1; }
        new_pid=$(dev "pidof agx-securityd" || true)
        if [[ -z $new_pid || $new_pid == "$old_pid" ]]; then
            echo "agx-securityd did not restart; restoring the previous binary" >&2
            dev "cp $BACKUP/agx-securityd $path.rb && mv $path.rb $path" || true
            die "agx-securityd restart failed"
        fi
        [[ $(dev "sha256sum /proc/$new_pid/exe | cut -d' ' -f1") == "$new" ]] || die "agx-securityd restarted but is not the new binary"
        echo "agx-securityd restarted (pid $old_pid -> $new_pid), SSH is back" ;;

    agx-gui)
        dev "grep -c angi-refresh-on-sigusr1 /usr/bin/agx-gui >/dev/null" || die "the running desktop has no refresh handler; refresh it from Settings once"
        dev "mv $path.hot $path"
        # BusyBox has no stat: read the owner from /proc. Never signal the root greeter.
        pid=$(dev 'for p in $(pidof agx-gui); do uid=$(sed -n "s/^Uid:[[:space:]]*\([0-9]*\).*/\1/p" /proc/$p/status); [ "$uid" != 0 ] && echo $p; done | head -1')
        if [[ -z $pid ]]; then echo "No signed-in desktop is running. The new binary starts at the next sign-in."; continue; fi
        dev "kill -USR1 $pid"
        ok=0
        for _ in $(seq 1 20); do
            sleep 2
            [[ $(dev "sha256sum /proc/$pid/exe 2>/dev/null | cut -d' ' -f1" || true) == "$new" ]] && { ok=1; break; }
        done
        if ((ok)); then
            sleep 12
            [[ $(dev "sha256sum /proc/$pid/exe 2>/dev/null | cut -d' ' -f1" || true) == "$new" ]] && echo "desktop refreshed into the new binary (pid $pid) and stayed up" || ok=0
        fi
        if ((!ok)); then
            echo "the new desktop did not come up healthy; restoring the previous binary" >&2
            dev "cp $BACKUP/agx-gui $path.rb && mv $path.rb $path" || true
            die "agx-gui hot update failed and was rolled back; the supervisor restarts the desktop"
        fi ;;
    esac
done
say "Done. Backup of the replaced binaries: $BACKUP on the device (scripts/hot-update.sh --rollback restores it)."
