#!/usr/bin/env bash
# Deploy the pinned Herdr runtime and ANGI swarm tooling to the development Pi.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
DESTINATION=${AGX_SSH_HOST:-root@10.77.0.2}
OPTIONS=(-i "${AGX_SSH_KEY:-$HOME/.ssh/id_ed25519}" -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=10 -o StrictHostKeyChecking=yes)
if [[ -n ${AGX_SSH_HOST_KEY_ALIAS:-} ]]; then OPTIONS+=(-o "HostKeyAlias=$AGX_SSH_HOST_KEY_ALIAS"); fi
mkdir -p "$ROOT/.cache/herdr"
binary=$ROOT/.cache/herdr/herdr-linux-aarch64
if [[ ! -f $binary ]]; then
    curl -fL --retry 3 https://github.com/herdrdev/herdr/releases/download/v0.9.5/herdr-linux-aarch64 -o "$binary"
fi
printf '%s  %s\n' 17d7536f2da50859523680ef279dffcac8e4a235f4e46fe8da6c2ec44141897c "$binary" | sha256sum -c --status
stage=$ROOT/.cache/herdr/stage
mkdir -p "$stage/usr/bin" "$stage/usr/libexec/agx-agent" "$stage/usr/share/agx/agent"
install -m755 "$binary" "$stage/usr/bin/herdr"
install -m755 "$ROOT/apps/agent/agent.sh" "$stage/usr/bin/agent"
install -m755 "$ROOT/apps/agent/angi-build.sh" "$stage/usr/bin/angi-build"
install -m755 "$ROOT/apps/agent/angi-update.sh" "$stage/usr/bin/angi-update"
"$ROOT/output/host/bin/aarch64-linux-gcc" -Os -Wall -Wextra -Werror "$ROOT/apps/agent/angi-root.c" -o "$stage/usr/bin/angi-root"
install -m644 "$ROOT"/apps/agent/*.py "$stage/usr/libexec/agx-agent/"
cp -RL "$ROOT/apps/agent/skills" "$ROOT/apps/agent/profiles" "$stage/usr/share/agx/agent/"
install -m644 "$ROOT/apps/agent/herdr.toml" "$ROOT/apps/agent/sdk-env.sh" "$ROOT/apps/agent/LICENSE.herdr" "$ROOT/apps/agent/apps.json" "$stage/usr/share/agx/agent/"
bash "$ROOT/scripts/stage-monaco.sh" "$stage/usr/share/agx/agent/work-editor"
tar -cf "$ROOT/.cache/herdr/runtime.tar" -C "$stage" .
ssh "${OPTIONS[@]}" "$DESTINATION" 'mountpoint -q /var/lib/agx-data && test -x /usr/bin/python3 && mkdir -p /var/lib/agx-data/agx-agent'
scp -O "${OPTIONS[@]}" "$ROOT/.cache/herdr/runtime.tar" "$DESTINATION:/var/lib/agx-data/agx-agent/runtime.tar"
ssh "${OPTIONS[@]}" "$DESTINATION" '/bin/bash -s' <<'INSTALL'
set -euo pipefail
base=/var/lib/agx-data/agx-agent
identifier=$(sha256sum "$base/runtime.tar" | cut -c1-16)
release=$base/releases/$identifier
mkdir -p "$release"
tar -xf "$base/runtime.tar" -C "$release"
chown -R 0:0 "$release"
chmod -R go-w "$release"
find "$release/usr" -type d | while IFS= read -r path; do mkdir -p "/${path#"$release"/}"; done
find "$release/usr" -type f | while IFS= read -r path; do
    destination=/${path#"$release"/}
    if [[ $destination == /usr/bin/angi-root ]]; then
        # A regular executable is necessary: the NVMe release is nosuid.
        cp "$path" "$destination.new"
        chown root:agx-admin "$destination.new"
        chmod 4750 "$destination.new"
        mv -fT "$destination.new" "$destination"
    else
        ln -s "$path" "$destination.new"
        mv -fT "$destination.new" "$destination"
    fi
done
ln -sfn "$release" "$base/current"
rm "$base/runtime.tar"
herdr --version
INSTALL
