#!/usr/bin/env bash
# Build and install only the ANGI desktop against an already provisioned runtime.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
root=$PWD
board=${BOARD:-rpi5}
field() { python3 -I scripts/detect-board.py --field "$board" "$1"; }
arch=$(field arch)
out=$(realpath -m -- "${AGX_BUILD_OUTPUT:-$(field output)}")
if [[ "$(cat "$out/images/board.txt" 2>/dev/null || true)" != "$board" ||
      ! -f "$out/.config" ]] || ! grep -qx 'BR2_PACKAGE_AGX_GUI=y' "$out/.config"; then
    echo "Use existing Buildroot output with ANGI Desktop enabled for BOARD=$board." >&2
    exit 2
fi
target=$arch-unknown-linux-gnu
compiler=$out/host/bin/$arch-linux-gcc
strip=$out/host/bin/$arch-linux-strip
pkg_config=$out/host/bin/pkg-config
for tool in "$compiler" "$strip" "$pkg_config"; do
    [[ -x "$tool" ]] || { echo "Missing Buildroot tool: $tool" >&2; exit 2; }
done
features=kms
if grep -qx 'BR2_PACKAGE_AGX_GUI_HEADLESS=y' "$out/.config"; then
    features=headless
elif grep -qx 'BR2_PACKAGE_AGX_WAYLAND=y' "$out/.config"; then
    features=kms,wayland
fi

destination=${AGX_SSH_HOST:-}
if [[ -z "$destination" && "$board" == rpi5 ]]; then
    destination=root@10.77.0.2
fi
[[ -n "$destination" ]] || { echo "Set AGX_SSH_HOST for BOARD=$board." >&2; exit 2; }
options=(-i "${AGX_SSH_KEY:-$HOME/.ssh/id_ed25519}" -o IdentitiesOnly=yes
         -o BatchMode=yes -o StrictHostKeyChecking=yes -o ConnectTimeout=8
         -o ConnectionAttempts=1 -o ServerAliveInterval=5 -o ServerAliveCountMax=3)
if [[ -n ${AGX_SSH_HOST_KEY_ALIAS:-} ]]; then
    options+=(-o "HostKeyAlias=$AGX_SSH_HOST_KEY_ALIAS")
fi
ssh_device() { ssh "${options[@]}" "$destination" "$@"; }
device_arch=$(ssh_device 'uname -m')
[[ "$device_arch" == "$arch" ]] || { echo "Expected $arch, device reports $device_arch." >&2; exit 2; }
previous=$(ssh_device 'set -e
    mountpoint -q /var/lib/agx-data
    test -f /usr/libexec/agx-agent/gui_deploy.py || { echo "Install the current ANGI agent runtime first." >&2; exit 2; }
    basename "$(readlink -f /usr/bin/agx-gui)"')
[[ "$previous" =~ ^agx-gui\.[a-f0-9]{16}$ ]] || {
    echo "Use the full workstation deployment to establish a desktop release first." >&2; exit 2;
}

local_stage=$(mktemp -d)
remote_stage=
cleanup() {
    if [[ -n "$remote_stage" ]]; then
        ssh_device "rm -rf -- '$remote_stage'" || true
    fi
    rm -rf -- "$local_stage"
}
trap cleanup EXIT
tree_hash() {
    python3 -I -c 'import sys; sys.path.insert(0,"apps/agent"); import releases; print(releases.tree_hash("."))'
}
tree=$(tree_hash)
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$root/.cache/cargo-target}
export PKG_CONFIG=$pkg_config PKG_CONFIG_ALLOW_CROSS=1
target_env=${target//-/_}
export "CARGO_TARGET_${target_env^^}_LINKER=$compiler"
export "CC_$target_env=$compiler"
./scripts/cargo.sh build --locked --release --manifest-path crates/agx-gui/Cargo.toml \
    --target "$target" --no-default-features --features "$features"
[[ "$(tree_hash)" == "$tree" ]] || { echo "Source changed during the desktop build. Retry." >&2; exit 1; }
git archive --format=tar "$tree" > "$local_stage/source.tar"
"$strip" -o "$local_stage/agx-gui" "$CARGO_TARGET_DIR/$target/release/agx-gui"
digest=$(sha256sum "$local_stage/agx-gui" | cut -d ' ' -f 1)

remote_stage=$(ssh_device 'mktemp -d /var/lib/agx-data/agx-gui/.deploy.XXXXXX')
scp -O "${options[@]}" "$local_stage/agx-gui" "$local_stage/source.tar" "$destination:$remote_stage/"
ssh_device "python3 /usr/libexec/agx-agent/update.py deploy-gui '$remote_stage' '$digest' '$tree' '$previous'"
