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
if [ -n "${BASE_DIR:-}" ] && [ -f "$BASE_DIR/.generic-iso" ]; then
    install -d -m 0700 "$TARGET_DIR/root/.ssh"
    rm -f -- "$TARGET_DIR/root/.ssh/authorized_keys" "$dropbear_dir"/dropbear_*_host_key
    sed -i 's/^root:[^:]*:/root:!:/' "$TARGET_DIR/etc/shadow"
    install -m 0600 /dev/null "$TARGET_DIR/etc/agx/setup-enabled"
    rm -f -- "$TARGET_DIR/etc/init.d/S50avahi-daemon" "$TARGET_DIR/etc/agx/setup-claimed"
else
    rm -f -- "$TARGET_DIR/etc/agx/setup-enabled"
    if [ -f "$TARGET_DIR/root/.ssh/authorized_keys" ]; then
        chmod 0600 "$TARGET_DIR/root/.ssh/authorized_keys"
    fi
fi

install -D -m 0644 "$BR2_EXTERNAL_AGX_PATH/../brand/logo-ascii.md" "$TARGET_DIR/usr/share/agx/logo.ascii"
chmod 0755 "$TARGET_DIR/usr/share/agx"

# Bluetooth service chain (D-Bus, BlueZ, agx-bluetoothd); agx-init supervises it.
if [ -x "$TARGET_DIR/usr/libexec/bluetooth/bluetoothd" ]; then
    install -D -m 0755 "$BR2_EXTERNAL_AGX_PATH/../apps/bluetooth/run.sh" "$TARGET_DIR/usr/libexec/agx-bluetooth/run"
else
    rm -f -- "$TARGET_DIR/usr/libexec/agx-bluetooth/run"
fi
