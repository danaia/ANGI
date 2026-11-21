#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
# Optional signed distro tools extracted locally on the development workstation.
if [[ -d "$ROOT/.cache/host-tools/usr/bin" ]]; then
    export PATH="$ROOT/.cache/host-tools/usr/bin:$PATH"
fi
VERSION=2026.08.1
SHA256=34fb7bea8a213ffde39cc02e61703391560f26e0850f03138c722d0b09880f2b
ARCHIVE="$ROOT/.cache/downloads/buildroot-$VERSION.tar.xz"
SOURCE="$ROOT/.cache/src/buildroot-$VERSION"
BOARD=${BOARD:-rpi5}
board_field() { python3 -I "$ROOT/scripts/detect-board.py" --field "$BOARD" "$1"; }
python3 -I "$ROOT/scripts/detect-board.py" --require "$BOARD" >/dev/null || {
    python3 -I "$ROOT/scripts/detect-board.py" --require "$BOARD" >&2; exit 3; }
DEFCONFIG=$(board_field defconfig)
OUTPUT=$(realpath -m -- "${AGX_BUILD_OUTPUT:-"$ROOT/$(board_field output)"}")
FIRMWARE_DIR="$ROOT/$(board_field firmware_dir)"
ACTION=${1:-build}
case "$ACTION" in
    prepare|configure|build|dependencies) ;;
    *) echo "Usage: $0 {prepare|configure|dependencies|build}" >&2; exit 2 ;;
esac
mkdir -p "$(dirname "$ARCHIVE")" "$(dirname "$SOURCE")"
if [[ ! -f "$ARCHIVE" ]]; then
    curl -fL --retry 3 "https://buildroot.org/downloads/buildroot-$VERSION.tar.xz" -o "$ARCHIVE.part"
    mv "$ARCHIVE.part" "$ARCHIVE"
fi
printf '%s  %s\n' "$SHA256" "$ARCHIVE" | sha256sum --check --status
if [[ ! -d "$SOURCE" ]]; then
    tar -xJf "$ARCHIVE" -C "$(dirname "$SOURCE")"
fi
[[ "$ACTION" == prepare ]] && exit 0
build_options=()
if [[ ${AGX_GENERIC_ISO:-0} == 1 || -f "$OUTPUT/.generic-iso" ]]; then
    [[ $BOARD == rpi4 && -n ${AGX_BUILD_OUTPUT:-} ]] || {
        echo "Set AGX_BUILD_OUTPUT to a separate Pi 4 / 400 build directory for the generic ISO." >&2
        exit 1
    }
    mkdir -p -- "$OUTPUT"
    touch -- "$OUTPUT/.generic-iso"
    build_options+=("BR2_ROOTFS_OVERLAY=$ROOT/buildroot/board/agx/rootfs-overlay $FIRMWARE_DIR/rootfs-overlay")
else
    # Keep workstation-specific public keys out of the tracked board overlay.
    PUBLIC_KEY=${SSH_PUBLIC_KEY:-$(python3 "$ROOT/scripts/ssh_provisioning.py")}
    ssh-keygen -lf "$PUBLIC_KEY" >/dev/null
    if ! grep -qE '^(ssh-ed25519|ssh-rsa|ecdsa-sha2-)[^ ]* ' "$PUBLIC_KEY"; then
        echo "Expected an OpenSSH public key, not a private key or certificate." >&2
        exit 1
    fi
    local_overlay="$OUTPUT/.ssh-overlay"
    overlays="$ROOT/buildroot/board/agx/rootfs-overlay"
    if [[ -d $ROOT/buildroot/board/agx/$BOARD/rootfs-overlay ]]; then
        overlays+=" $ROOT/buildroot/board/agx/$BOARD/rootfs-overlay"
    fi
    build_options+=("BR2_ROOTFS_OVERLAY=$overlays $local_overlay")
    install -d -m 700 "$local_overlay/root/.ssh"
    install -m 600 "$PUBLIC_KEY" "$local_overlay/root/.ssh/authorized_keys"
fi
brmake() {
    make -C "$SOURCE" O="$OUTPUT" BR2_EXTERNAL="$ROOT/buildroot" "${build_options[@]}" "$@"
}
if [[ "$ACTION" == configure || ! -f "$OUTPUT/.config" ]]; then
    case "$(uname -m)" in
        x86_64)
            brmake "$DEFCONFIG"
            ;;
        aarch64)
            # Buildroot's prebuilt Bootlin toolchains run on x86-64 hosts.
            # Build a glibc toolchain locally inside a native ARM64 Linux guest.
            mkdir -p "$OUTPUT"
            host_config="$OUTPUT/.angi-arm64-host-defconfig"
            {
                sed '/^BR2_TOOLCHAIN_EXTERNAL/d' "$ROOT/buildroot/configs/$DEFCONFIG"
                printf '%s\n' BR2_TOOLCHAIN_BUILDROOT=y BR2_TOOLCHAIN_BUILDROOT_GLIBC=y \
                    BR2_TOOLCHAIN_BUILDROOT_CXX=y BR2_KERNEL_HEADERS_6_12=y
            } > "$host_config"
            brmake defconfig BR2_DEFCONFIG="$host_config"
            ;;
        *)
            echo "ANGI builds require an x86-64 or ARM64 Linux host" >&2
            exit 1
            ;;
    esac
fi
case "$ACTION" in
    configure) ;;
    dependencies) brmake dependencies ;;
    build)
        if grep -q '^BR2_PACKAGE_AGX=y$' "$OUTPUT/.config"; then
            # Supply Cargo for vendoring even on hosts without a Rust installation.
            brmake "BR2_JLEVEL=${JOBS:-8}" host-rustc
            stage_options=()
            if grep -q '^BR2_PACKAGE_AGX_GUI=y$' "$OUTPUT/.config"; then
                stage_options+=(--desktop)
            fi
            PATH="$OUTPUT/host/bin:$PATH" "$ROOT/scripts/prepare-rust-package.sh" "${stage_options[@]}"
            # Local source packages need an explicit rebuild to pick up edits.
            brmake "BR2_JLEVEL=${JOBS:-8}" agx-rebuild
            if ((${#stage_options[@]})); then
                brmake "BR2_JLEVEL=${JOBS:-8}" agx-gui-rebuild
            fi
        fi
        if grep -q '^BR2_PACKAGE_AGX_AGENT=y$' "$OUTPUT/.config"; then
            brmake "BR2_JLEVEL=${JOBS:-8}" agx-agent-rebuild
        fi
        if grep -q '^BR2_PACKAGE_AGX_WEB=y$' "$OUTPUT/.config"; then
            brmake "BR2_JLEVEL=${JOBS:-8}" agx-web-rebuild
        fi
        # Buildroot does not otherwise recopy changed external firmware configuration.
        if [[ -f "$FIRMWARE_DIR/config.txt" ]] && {
            ! cmp -s "$FIRMWARE_DIR/config.txt" "$OUTPUT/images/rpi-firmware/config.txt" ||
            ! cmp -s "$FIRMWARE_DIR/cmdline.txt" "$OUTPUT/images/rpi-firmware/cmdline.txt";
        }; then
            brmake "BR2_JLEVEL=${JOBS:-8}" rpi-firmware-rebuild
        fi
        brmake "BR2_JLEVEL=${JOBS:-8}" all
        # Record which board this image is for; scripts/flash-sd.sh checks it.
        echo "$BOARD" > "$OUTPUT/images/board.txt"
        ;;
esac
