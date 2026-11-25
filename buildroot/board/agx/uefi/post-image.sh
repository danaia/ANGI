#!/bin/sh
set -eu

board_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
images=$BINARIES_DIR
case "${BOARD:?BOARD must be set}" in
    arm-desktop) kernel=Image ;;
    x86-desktop|x86-amd) kernel=bzImage ;;
    *) echo "Unknown UEFI build target: $BOARD" >&2; exit 1 ;;
esac
root_uuid=$("$HOST_DIR/sbin/dumpe2fs" -h "$images/rootfs.ext4" 2>/dev/null |
    sed -n 's/^Filesystem UUID:[[:space:]]*//p')
[ -n "$root_uuid" ] || exit 1
timeout=0
setup_video=
if [ "$kernel" = bzImage ] && [ -f "$BASE_DIR/.generic-iso" ]; then
    timeout=4
    # The EFI stub selects a GOP mode before Linux starts its software display.
    setup_video=' video=efifb:1920x1080'
fi
printf 'set timeout=%s\nmenuentry "ANGI" {\n  search --no-floppy --file --set=root /%s\n  linux /%s root=PARTUUID=%s rootwait console=tty1 init=/usr/sbin/agx-init%s\n}\n' \
    "$timeout" "$kernel" "$kernel" "$root_uuid" "$setup_video" > "$images/efi-part/EFI/BOOT/grub.cfg"
if [ "$timeout" -gt 0 ]; then
    printf 'menuentry "ANGI (graphics recovery)" {
  search --no-floppy --file --set=root /%s
  linux /%s root=PARTUUID=%s rootwait console=tty1 init=/usr/sbin/agx-init agx.driver_recovery=1 module_blacklist=amdgpu,i915,xe,nvidia,nvidia_modeset,nvidia_drm,nvidia_uvm
}
' "$kernel" "$kernel" "$root_uuid" >> "$images/efi-part/EFI/BOOT/grub.cfg"
fi
sed -e "s/@KERNEL@/$kernel/g" -e "s/@ROOT_UUID@/$root_uuid/g" \
    "$board_dir/genimage.cfg" > "$images/genimage-angi.cfg"
support/scripts/genimage.sh -c "$images/genimage-angi.cfg"
