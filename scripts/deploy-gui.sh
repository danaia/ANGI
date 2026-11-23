#!/usr/bin/env bash
# Install the desktop (agx-gui) on a running AGX Pi without reflashing: the
# runtime libraries and data from the Buildroot target tree, the binary, and
# the HDMI display overlay in config.txt. Run `make deploy-gui` to build all binaries.
# Prerequisite: the packages in agx_rpi5_defconfig's "Desktop" section are built.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
DESTINATION=${AGX_SSH_HOST:-root@10.77.0.2}
KEY=${AGX_SSH_KEY:-"${HOME}/.ssh/id_ed25519"}
TARGET="$ROOT/output/target"
BINARY="${AGX_GUI_BINARY:-${CARGO_TARGET_DIR:-$ROOT/.cache/cargo-target}/aarch64-unknown-linux-gnu/release/agx-gui}"
RELEASE="${CARGO_TARGET_DIR:-$ROOT/.cache/cargo-target}/aarch64-unknown-linux-gnu/release"
SSH_OPTIONS=(-o BatchMode=yes -o ConnectTimeout=10 -i "$KEY")
if [[ -n "${AGX_SSH_HOST_KEY_ALIAS:-}" ]]; then
    SSH_OPTIONS+=(-o "HostKeyAlias=$AGX_SSH_HOST_KEY_ALIAS" -o StrictHostKeyChecking=yes)
fi
SSH=(ssh "${SSH_OPTIONS[@]}" "$DESTINATION")
SCP=(scp -O "${SSH_OPTIONS[@]}")
[[ -f "$BINARY" ]] || { echo 'Run make gui-build before deployment.' >&2; exit 1; }
for binary in agx agx-init; do
    [[ -f "$RELEASE/$binary" ]] || { echo 'Run make init-build before deployment.' >&2; exit 1; }
done
[[ -d "$TARGET/usr/share/X11/xkb" ]] || { echo 'Build the desktop packages first (see defconfig).' >&2; exit 1; }
wayland_installed=false
if "${SSH[@]}" 'test -x /usr/bin/angi-wayland'; then
    wayland_installed=true
    grep -aFq 'EGL_KHR_platform_wayland' "$TARGET/usr/lib/libEGL.so.1" || {
        echo 'Reconfigure Buildroot Mesa with Wayland enabled before deploying to this ANGI device.' >&2
        exit 1
    }
fi
RUNTIME="$ROOT/.cache/agx-gui-runtime.tar"
python3 "$ROOT/scripts/package-gui-runtime.py" "$TARGET" "$RUNTIME"

# Keep the first installed versions so recovery SSH can restore the whole stack.
"${SSH[@]}" 'set -e
mountpoint -q /var/lib/agx-data || { echo "The NVMe data volume must be mounted" >&2; exit 1; }
mkdir -p /var/lib/agx-data/agx-gui/backup
if ! test -e /root/agx-gui-backup; then
    ln -s /var/lib/agx-data/agx-gui/backup /root/agx-gui-backup
fi
for binary in /usr/bin/agx /usr/bin/agx-gui /usr/sbin/agx-init; do
    if test -f "$binary" && ! test -f "/root/agx-gui-backup/${binary##*/}"; then
        cp -pL "$binary" "/root/agx-gui-backup/${binary##*/}"
    fi
done'

STRIPPED="$ROOT/.cache/agx-gui.stripped"
"$ROOT/output/host/bin/aarch64-linux-strip" -o "$STRIPPED" "$BINARY"

echo "Copying runtime libraries and data to $DESTINATION"
"${SCP[@]}" "$RUNTIME" "$DESTINATION:/var/lib/agx-data/agx-gui/runtime.tar"
"${SSH[@]}" 'set -e
archive=/var/lib/agx-data/agx-gui/runtime.tar
release_id=$(sha256sum "$archive" | cut -c1-16)
release=/var/lib/agx-data/agx-gui/releases/$release_id
mkdir -p "$release"
tar -xf "$archive" -C "$release"
# Libraries live on NVMe. Atomic symlink replacement leaves running mappings intact.
find "$release" \( -type f -o -type l \) | while IFS= read -r source; do
    relative=${source#"$release"/}
    case "$relative" in lib/*|lib64/*) echo "Refusing to relocate boot library: $relative" >&2; exit 1 ;; esac
    destination=/$relative
    mkdir -p "$(dirname "$destination")"
    ln -sf "$source" "$destination.agx-new"
    mv -f "$destination.agx-new" "$destination"
done
rm "$archive"
modprobe v3d'

PREVIOUS=$("${SSH[@]}" 'basename "$(readlink -f /usr/bin/agx-gui)"')
echo 'Staging desktop, graphical login and startup binaries'
"${SCP[@]}" "$STRIPPED" "$DESTINATION:/var/lib/agx-data/agx-gui/agx-gui.new"
"${SCP[@]}" "$RELEASE/agx" "$DESTINATION:/usr/bin/agx.new"
"${SCP[@]}" "$RELEASE/agx-init" "$DESTINATION:/usr/sbin/agx-init.new"
"${SSH[@]}" 'mkdir -p /usr/libexec'
"${SCP[@]}" "$RELEASE/agx-display-devices" "$DESTINATION:/usr/libexec/agx-display-devices.new"
"${SCP[@]}" "$ROOT/buildroot/board/agx/rootfs-overlay/usr/libexec/agx-display-access" "$DESTINATION:/usr/libexec/agx-display-access.new"
if [[ "$wayland_installed" = true ]]; then
    "${SCP[@]}" "$ROOT/buildroot/package/agx-wayland/agx-wayland-shell" "$DESTINATION:/usr/libexec/agx-wayland-shell.new"
    "${SCP[@]}" "$ROOT/buildroot/package/agx-wayland/angi-wayland" "$DESTINATION:/usr/bin/angi-wayland.new"
fi
"${SSH[@]}" 'mkdir -p /etc/modules-load.d'
"${SCP[@]}" "$ROOT/buildroot/board/agx/rootfs-overlay/etc/modules-load.d/agx-display.conf" "$DESTINATION:/etc/modules-load.d/agx-display.conf"
"${SSH[@]}" 'set -e
chmod 755 /usr/libexec/agx-display-devices.new /var/lib/agx-data/agx-gui/agx-gui.new /usr/bin/agx.new /usr/sbin/agx-init.new /usr/libexec/agx-display-access.new
/usr/bin/agx.new --version
/usr/sbin/agx-init.new --check
# PID 1 must load before NVMe is mounted. Check resolved paths, not only ldd success.
for library in $(ldd /usr/sbin/agx-init.new); do
    case "$library" in
        /*) case "$(readlink -f "$library")" in
            /var/lib/agx-data/*) echo "PID 1 depends on NVMe: $library" >&2; exit 1 ;;
        esac ;;
    esac
done
libraries=$(ldd /var/lib/agx-data/agx-gui/agx-gui.new 2>&1) || { printf "%s\n" "$libraries"; exit 1; }
if printf "%s\n" "$libraries" | grep -q "not found"; then
    printf "%s\n" "$libraries" >&2; exit 1
fi
mv /usr/bin/agx.new /usr/bin/agx
mv /usr/libexec/agx-display-devices.new /usr/libexec/agx-display-devices
mv /usr/libexec/agx-display-access.new /usr/libexec/agx-display-access
gui_hash=$(sha256sum /var/lib/agx-data/agx-gui/agx-gui.new | cut -c1-16)
gui=/var/lib/agx-data/agx-gui/agx-gui.$gui_hash
mv /var/lib/agx-data/agx-gui/agx-gui.new "$gui"
ln -sf "$gui" /usr/bin/agx-gui.new
mv -f /usr/bin/agx-gui.new /usr/bin/agx-gui
if test -f /usr/bin/angi-wayland.new && test -f /usr/libexec/agx-wayland-shell.new; then
    chmod 755 /usr/bin/angi-wayland.new /usr/libexec/agx-wayland-shell.new
    mv -f /usr/libexec/agx-wayland-shell.new /usr/libexec/agx-wayland-shell
    mv -f /usr/bin/angi-wayland.new /usr/bin/angi-wayland
    ln -sf agx-wayland-shell /usr/libexec/agx-gui-wayland.new
    mv -f /usr/libexec/agx-gui-wayland.new /usr/libexec/agx-gui-wayland
fi
# `open FILE` is this same binary under another name; it follows whichever release agx-gui points at.
ln -sf agx-gui /usr/bin/open.new
mv -f /usr/bin/open.new /usr/bin/open
mv /usr/sbin/agx-init.new /usr/sbin/agx-init'

# Record this release and put its source where the device's agents build from. Without this, an update
# made on the device would start from an older checkout and replace this release with older code.
if "${SSH[@]}" 'test -f /usr/libexec/agx-agent/update.py'; then
    echo 'Recording the release and syncing its source to the device'
    TREE=$(python3 -I -c 'import sys; sys.path.insert(0, sys.argv[1]); import releases; print(releases.tree_hash(sys.argv[2]))' "$ROOT/apps/agent" "$ROOT")
    (cd "$ROOT" && git ls-files -z --cached --others --exclude-standard | while IFS= read -r -d '' file; do [[ -e $file || -L $file ]] && printf '%s\0' "$file"; done | tar --null -T - -cf "$ROOT/.cache/angi-source.tar")
    "${SCP[@]}" "$ROOT/.cache/angi-source.tar" "$DESTINATION:/var/lib/agx-data/angi-source.tar"
    "${SSH[@]}" "set -e; python3 /usr/libexec/agx-agent/update.py adopt agx-gui.$(sha256sum "$STRIPPED" | cut -c1-16) workstation $TREE '$PREVIOUS' /var/lib/agx-data/angi-source.tar auto; rm -f /var/lib/agx-data/angi-source.tar"
else
    echo 'warning: angi-update is not installed on the device (run make deploy-agent, then deploy the GUI again), so this release is not recorded.' >&2
fi

# fontconfig finds no fonts on the bare image, and Slint's text stack panics on that.
echo 'Installing a system font'
"${SSH[@]}" 'mkdir -p /usr/share/fonts'
"${SCP[@]}" "$ROOT/crates/agx-gui/ui/fonts/JetBrainsMonoNerdFont-Regular.ttf" "$DESTINATION:/usr/share/fonts/"

echo 'Checking the display overlay in config.txt'
"${SSH[@]}" 'set -e
mkdir -p /mnt/agxboot && mount /dev/mmcblk0p1 /mnt/agxboot
trap "umount /mnt/agxboot" EXIT
if grep -q "^dtoverlay=vc4-kms-v3d" /mnt/agxboot/config.txt; then
    echo "vc4-kms-v3d already enabled"
else
    test -f /mnt/agxboot/config.txt.pre-gui || cp /mnt/agxboot/config.txt /mnt/agxboot/config.txt.pre-gui
    printf "\n# HDMI display through DRM/KMS for the desktop.\ndtoverlay=vc4-kms-v3d\n" >> /mnt/agxboot/config.txt
    echo "Added dtoverlay=vc4-kms-v3d (backup: config.txt.pre-gui). Reboot to apply."
fi
sync'
echo 'Installed. Reboot to start directly at graphical login; SSH and serial recovery remain available.'
