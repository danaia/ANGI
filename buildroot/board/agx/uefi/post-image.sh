#!/bin/sh
set -eu

board_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
images=$BINARIES_DIR
case "${BOARD:?BOARD must be set}" in
    arm-desktop) kernel=Image ;;
    x86-desktop) kernel=bzImage ;;
    *) echo "Unknown UEFI build target: $BOARD" >&2; exit 1 ;;
esac
root_uuid=$("$HOST_DIR/sbin/dumpe2fs" -h "$images/rootfs.ext4" 2>/dev/null |
    sed -n 's/^Filesystem UUID:[[:space:]]*//p')
[ -n "$root_uuid" ] || exit 1
printf 'set timeout=0\nmenuentry "ANGI" {\n  search --no-floppy --file --set=root /%s\n  linux /%s root=PARTUUID=%s rootwait console=tty1 init=/usr/sbin/agx-init\n}\n' \
    "$kernel" "$kernel" "$root_uuid" > "$images/efi-part/EFI/BOOT/grub.cfg"
sed -e "s/@KERNEL@/$kernel/g" -e "s/@ROOT_UUID@/$root_uuid/g" \
    "$board_dir/genimage.cfg" > "$images/genimage-angi.cfg"
support/scripts/genimage.sh -c "$images/genimage-angi.cfg"
