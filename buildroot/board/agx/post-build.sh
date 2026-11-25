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
# Remove keys left in staging if an existing build is repurposed for a public ISO.
rm -f "$TARGET_DIR/usr/libexec/agx-wired-network" \
    "$TARGET_DIR/usr/share/udhcpc/default.script.d/50-agx-direct-ethernet"
if [ -n "${BASE_DIR:-}" ] && [ -f "$BASE_DIR/.generic-iso" ]; then
    install -d -m 0700 "$TARGET_DIR/root/.ssh"
    rm -f -- "$TARGET_DIR/root/.ssh/authorized_keys" "$dropbear_dir"/dropbear_*_host_key
    sed -i 's/^root:[^:]*:/root:!:/' "$TARGET_DIR/etc/shadow"
    install -m 0600 /dev/null "$TARGET_DIR/etc/agx/setup-enabled"
    rm -f -- "$TARGET_DIR/etc/init.d/S50avahi-daemon" "$TARGET_DIR/etc/agx/setup-claimed" "$TARGET_DIR/etc/agx/setup-driver-resume"
    if { [ "${BOARD:-}" = x86-desktop ] || [ "${BOARD:-}" = x86-amd ]; }; then
        cat > "$TARGET_DIR/etc/network/interfaces" <<'EOF'
auto lo
iface lo inet loopback
EOF
        board_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
        install -D -m 0755 "$board_dir/uefi/wired-network" "$TARGET_DIR/usr/libexec/agx-wired-network"
        install -D -m 0755 "$board_dir/uefi/direct-ethernet" \
            "$TARGET_DIR/usr/share/udhcpc/default.script.d/50-agx-direct-ethernet"
    fi
else
    rm -f -- "$TARGET_DIR/etc/agx/setup-enabled"
    if [ -f "$TARGET_DIR/root/.ssh/authorized_keys" ]; then
        chmod 0600 "$TARGET_DIR/root/.ssh/authorized_keys"
    fi
fi

# The kernel build retains these modules for the matching release bundles.
# Keep the generic firmware framebuffer and shared DRM helpers on the media.
if [ -n "${BASE_DIR:-}" ] &&
    grep -qx 'BR2_PACKAGE_AGX_GUI_PROVISION_GRAPHICS=y' "$BASE_DIR/.config"; then
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

# Release assembly provides the feed address and public signing key.
if [ -n "${AGX_DRIVER_FEED_DIR:-}" ]; then
    install -m 0600 "$AGX_DRIVER_FEED_DIR/driver-catalog.json" "$TARGET_DIR/etc/agx/driver-catalog.json"
    install -m 0600 "$AGX_DRIVER_FEED_DIR/driver-signers" "$TARGET_DIR/etc/agx/driver-signers"
else
    rm -f "$TARGET_DIR/etc/agx/driver-catalog.json" "$TARGET_DIR/etc/agx/driver-signers"
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
