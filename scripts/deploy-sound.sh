#!/usr/bin/env bash
# Install the audio runtime from the Pi 5 Buildroot output without replacing boot libraries.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
target="$root/output/target"
destination=${AGX_SSH_HOST:?Export AGX_SSH_HOST as documented in docs/pi-ethernet.md}
options=(-i "${AGX_SSH_KEY:-$HOME/.ssh/id_ed25519}" -o IdentitiesOnly=yes
    -o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=yes
    -o "HostKeyAlias=${AGX_SSH_HOST_KEY_ALIAS:-10.77.0.2}")
ssh=(ssh "${options[@]}" "$destination")
scp=(scp -O "${options[@]}")
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

# Keep each SONAME and its versioned target together.
files=(usr/bin/{aplay,arecord,pactl,pacat,pulseaudio}
    usr/libexec/{agx-sound/cliamp,bluetooth/bluetoothd}
    usr/share/{alsa,licenses/cliamp} usr/lib/pulseaudio)
for stem in asound ltdl pulse pulse-simple sbc sndfile; do
    for lib in "$target"/usr/lib/lib"$stem".so.*; do
        files+=("${lib#"$target/"}")
    done
done
for lib in "$target"/usr/lib/alsa-lib/libasound_module_*_pulse.so; do
    files+=("${lib#"$target/"}")
done
for file in "${files[@]}"; do
    [[ -e "$target/$file" ]] || { echo "Missing Buildroot audio artifact $file" >&2; exit 1; }
done
(cd "$target" && cp -a --parents -- "${files[@]}" "$stage")
install -Dm755 "$root/apps/sound/run.sh" "$stage/usr/libexec/agx-sound/run"
install -Dm755 "$root/apps/sound/cliamp.sh" "$stage/usr/bin/cliamp"
install -Dm644 "$root/apps/sound/stations.m3u" "$stage/usr/share/agx/sound/stations.m3u"
install -Dm644 "$root/apps/sound/radio-paradise.m3u" "$stage/usr/share/agx/sound/radio-paradise.m3u"
install -Dm644 "$root/apps/sound/system.pa" "$stage/etc/pulse/agx-system.pa"
install -Dm644 "$root/apps/sound/client.conf" "$stage/etc/pulse/client.conf.d/10-angi.conf"
install -Dm644 "$root/apps/sound/asound.conf" "$stage/etc/asound.conf"

archive="$stage/runtime.tar"
tar -cf "$archive" -C "$stage" etc usr
digest=$(sha256sum "$archive" | cut -d' ' -f1)
"${ssh[@]}" 'mountpoint -q /var/lib/agx-data && install -d -m 0755 /var/lib/agx-data/agx-sound'
"${scp[@]}" "$archive" "$destination:/var/lib/agx-data/agx-sound/$digest.tar"
"${ssh[@]}" sh -s -- "$digest" <<'REMOTE'
set -eu
base=/var/lib/agx-data/agx-sound
digest=$1
archive="$base/$digest.tar"
release="$base/releases/$digest"
[ "$(sha256sum "$archive" | cut -d' ' -f1)" = "$digest" ]
if ! grep -q '^pulse:' /etc/group; then
    busybox addgroup -S pulse
fi
if ! id pulse >/dev/null 2>&1; then
    adduser -S -D -H -G pulse -h /var/run/pulse -s /bin/false pulse
fi
mkdir -p "$release"
tar --no-same-owner -xf "$archive" -C "$release"
rm "$archive"
backup="$base/backup-before-$digest"
mkdir -p "$backup"
find "$release" \( -type f -o -type l \) | while IFS= read -r source; do
    relative=${source#"$release"/}
    destination=/$relative
    mkdir -p "$(dirname "$destination")"
    if [ "$(readlink "$destination" 2>/dev/null || :)" = "$source" ]; then continue; fi
    if [ -e "$destination" ] || [ -L "$destination" ]; then
        saved="$backup/$relative"
        mkdir -p "$(dirname "$saved")"
        if [ ! -e "$saved" ] && [ ! -L "$saved" ]; then
            cp -a "$destination" "$saved"
        fi
    fi
    ln -sfn "$source" "$destination.agx-new"
    mv -f "$destination.agx-new" "$destination"
done
/usr/bin/cliamp --version
/usr/bin/pulseaudio --version
printf 'Installed sound runtime %s\n' "$release"
REMOTE
