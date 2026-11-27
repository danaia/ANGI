#!/usr/bin/env sh
set -eu
sed -i '/AGX_NETWORK/d; /::respawn:\/usr\/sbin\/agx-netd/d' "$TARGET_DIR/etc/inittab"
if ! grep -q 'AGX_INIT' "$TARGET_DIR/etc/inittab"; then
    printf '\n# AGX_INIT\n::respawn:/usr/sbin/agx-init --supervise\n' >> "$TARGET_DIR/etc/inittab"
fi

# Rust init starts Dropbear directly, so the Buildroot service script does
# not replace the package's temporary host-key link on first boot.
dropbear_dir="$TARGET_DIR/etc/dropbear"
if [ -L "$dropbear_dir" ] && [ "$(readlink "$dropbear_dir")" = /var/run/dropbear ]; then
    rm -- "$dropbear_dir"
fi
if [ -L "$dropbear_dir" ]; then
    echo "Unexpected SSH host-key directory symlink: $dropbear_dir" >&2
    exit 1
fi
install -d -m 0700 "$dropbear_dir"
# Clear wired setup scripts left by a previous x86 build.
rm -f "$TARGET_DIR/usr/libexec/agx-wired-network" \
    "$TARGET_DIR/usr/share/udhcpc/default.script.d/50-agx-direct-ethernet"
if [ -n "${BASE_DIR:-}" ] && [ -f "$BASE_DIR/.generic-iso" ]; then
    # Strip private SSH provisioning when staging public media.
    install -d -m 0700 "$TARGET_DIR/root/.ssh"
    rm -f -- "$TARGET_DIR/root/.ssh/authorized_keys" "$dropbear_dir"/dropbear_*_host_key
    sed -i 's/^root:[^:]*:/root:!:/' "$TARGET_DIR/etc/shadow"
    install -m 0600 /dev/null "$TARGET_DIR/etc/agx/setup-enabled"
    rm -f -- "$TARGET_DIR/etc/init.d/S50avahi-daemon" "$TARGET_DIR/etc/agx/setup-claimed" "$TARGET_DIR/etc/agx/setup-driver-resume"
else
    rm -f -- "$TARGET_DIR/etc/agx/setup-enabled"
    if [ -f "$TARGET_DIR/root/.ssh/authorized_keys" ]; then
        chmod 0600 "$TARGET_DIR/root/.ssh/authorized_keys"
    fi
fi

case "${BOARD:-}" in
    x86-desktop|x86-amd)
        cat > "$TARGET_DIR/etc/network/interfaces" <<'EOF'
auto lo
iface lo inet loopback
EOF
        board_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
        install -D -m 0755 "$board_dir/uefi/wired-network" "$TARGET_DIR/usr/libexec/agx-wired-network"
        install -D -m 0755 "$board_dir/uefi/direct-ethernet" \
            "$TARGET_DIR/usr/share/udhcpc/default.script.d/50-agx-direct-ethernet"
        ;;
esac

# Match the Pi desktop default. The VNC listener remains bound to loopback.
if [ -n "${BASE_DIR:-}" ] && grep -qx 'BR2_x86_64=y' "$BASE_DIR/.config" &&
    grep -qx 'BR2_PACKAGE_AGX_GUI=y' "$BASE_DIR/.config"; then
    gui_env="$TARGET_DIR/etc/agx/gui.env"
    if ! grep -Eq '^[[:space:]]*AGX_GUI_VNC[[:space:]]*=' "$gui_env" 2>/dev/null; then
        (umask 077; printf '\nAGX_GUI_VNC=on\n' >> "$gui_env")
    fi
fi

# Install release metadata before removing the modules that setup downloads.
driver_feed=${AGX_DRIVER_FEED_DIR:-}
if [ -n "${BASE_DIR:-}" ] && [ -f "$BASE_DIR/.generic-iso" ] && [ -z "$driver_feed" ]; then
    echo 'The ANGI setup image needs AGX_DRIVER_FEED_DIR. See docs/desktop.md.' >&2
    exit 1
fi
provision_graphics=false
if [ -n "${BASE_DIR:-}" ] &&
    grep -qx 'BR2_PACKAGE_AGX_GUI_PROVISION_GRAPHICS=y' "$BASE_DIR/.config"; then
    provision_graphics=true
fi
if [ -n "$driver_feed" ]; then
    kernel_version=$(sed -n 's/^BR2_LINUX_KERNEL_VERSION="\(.*\)"$/\1/p' "$BASE_DIR/.config")
    arch=$(sed -n 's/^BR2_ARCH="\(.*\)"$/\1/p' "$BASE_DIR/.config")
    objcopy=objcopy
    for tool in "$HOST_DIR/bin/$arch"*-objcopy; do
        if [ -x "$tool" ]; then
            objcopy=$tool
            break
        fi
    done
    python3 "$BR2_EXTERNAL_AGX_PATH/../scripts/driver_release.py" \
        "$driver_feed" "$BUILD_DIR/linux-$kernel_version" --objcopy "$objcopy"
    install -m 0600 "$driver_feed/driver-catalog.json" "$TARGET_DIR/etc/agx/driver-catalog.json"
    install -m 0600 "$driver_feed/driver-signers" "$TARGET_DIR/etc/agx/driver-signers"
else
    rm -f "$TARGET_DIR/etc/agx/driver-catalog.json" "$TARGET_DIR/etc/agx/driver-signers"
fi

# An x86 image that keeps its drivers also needs the USB Bluetooth firmware
# that Buildroot's linux-firmware options leave out.
if ! "$provision_graphics" && [ -n "${BASE_DIR:-}" ] &&
    grep -qx 'BR2_PACKAGE_LINUX_FIRMWARE_IBT=y' "$BASE_DIR/.config"; then
    for firmware_source in "$BUILD_DIR"/linux-firmware-*; do
        python3 "$BR2_EXTERNAL_AGX_PATH/../scripts/x86_media_firmware.py" \
            "$firmware_source" "$TARGET_DIR"
    done
fi

# The kernel build retains these modules for the matching release bundles.
# Keep the generic firmware framebuffer and shared DRM helpers on the media.
if "$provision_graphics"; then
    # Web's CPU fallback lives outside the paths used by provisioned GPU drivers.
    lib="$TARGET_DIR/usr/lib"
    mesa="$lib/agx-web-mesa"
    for path in "$lib"/libEGL.so* "$lib"/libGLESv*.so* "$lib"/libgbm.so* "$lib"/libgallium* "$lib/dri" "$lib/gbm"; do
        [ -f "$path" ] || [ -d "$path" ] || [ -L "$path" ] || continue
        install -d -m 0755 "$mesa"
        rm -rf -- "$mesa/${path##*/}"
        mv -- "$path" "$mesa/"
    done
    for module_tree in "$TARGET_DIR"/lib/modules/*; do
        [ -d "$module_tree" ] || continue
        drivers="$module_tree/kernel/drivers"
        sound="$module_tree/kernel/sound"
        rm -rf -- "$drivers/gpu/drm/i915" "$drivers/gpu/drm/amd" "$drivers/gpu/drm/xe" \
            "$drivers/bluetooth" "$sound/pci/hda" "$sound/usb"
        "$HOST_DIR/sbin/depmod" -a -b "$TARGET_DIR" "${module_tree##*/}"
    done
fi

install -D -m 0644 "$BR2_EXTERNAL_AGX_PATH/../brand/logo-ascii.md" "$TARGET_DIR/usr/share/agx/logo.ascii"
chmod 0755 "$TARGET_DIR/usr/share/agx"

# Bluetooth service chain (D-Bus, BlueZ, agx-bluetoothd); agx-init supervises it.
if [ -x "$TARGET_DIR/usr/libexec/bluetooth/bluetoothd" ]; then
    install -D -m 0755 "$BR2_EXTERNAL_AGX_PATH/../apps/bluetooth/run.sh" "$TARGET_DIR/usr/libexec/agx-bluetooth/run"
else
    rm -f -- "$TARGET_DIR/usr/libexec/agx-bluetooth/run"
fi

if [ -x "$TARGET_DIR/usr/bin/pulseaudio" ]; then
    rm -f "$TARGET_DIR/etc/init.d/S50pulseaudio"
    sound="$BR2_EXTERNAL_AGX_PATH/../apps/sound"
    install -D -m 0755 "$sound/run.sh" "$TARGET_DIR/usr/libexec/agx-sound/run"
    install -D -m 0644 "$sound/system.pa" "$TARGET_DIR/etc/pulse/agx-system.pa"
    install -D -m 0644 "$sound/client.conf" "$TARGET_DIR/etc/pulse/client.conf.d/10-angi.conf"
    install -D -m 0644 "$sound/asound.conf" "$TARGET_DIR/etc/asound.conf"
fi
