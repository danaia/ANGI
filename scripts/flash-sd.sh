#!/usr/bin/env bash
# Safely erase a microSD card and write the ANGI image to it.
#
#   scripts/flash-sd.sh --list
#   scripts/flash-sd.sh --board rpi5 --device /dev/sdX         # asks you to type the path
#   scripts/flash-sd.sh --board rpi5 --device /dev/sdX --confirm-erase /dev/sdX
#   scripts/flash-sd.sh --board rpi5 --device /dev/sdX --image PRIVATE_DIR/sdcard.img --personalized
#
# --confirm-erase is for a coding agent acting AFTER the person has seen the card's
# identity and approved erasing it. It must repeat the --device path exactly.
# The script refuses system disks, mounted system paths and very large drives, wipes the
# old signatures and partition table, writes the image, verifies every byte and ejects.
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
IMAGE=""
MAX_GIB=256
BOARD=""
DEVICE=""
CONFIRM=""
LIST=0
PERSONALIZED=0

die() { echo "flash-sd: $*" >&2; exit 1; }
usage() { sed -n '2,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while (( $# )); do
    case $1 in
        --list) LIST=1 ;;
        --personalized) PERSONALIZED=1 ;;
        --device) DEVICE=${2:?--device needs a path}; shift ;;
        --confirm-erase) CONFIRM=${2:?--confirm-erase needs the device path}; shift ;;
        --board) BOARD=${2:?--board needs a target id (see scripts/detect-board.py --list)}; shift ;;
        --image) IMAGE=${2:?--image needs a path}; shift ;;
        --max-gib) MAX_GIB=${2:?--max-gib needs a number}; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; die "unknown option: $1" ;;
    esac
    shift
done

command -v lsblk >/dev/null || die "lsblk is required"

field() { lsblk -dnpo "$2" "$1" 2>/dev/null | sed 's/^ *//; s/ *$//'; }
identity() {
    local device=$1 details number bytes read_only sys_path sequence=""
    # udev properties can disappear while its event is blocked by our disk lock.
    # Keep model and serial in the confirmation display and use kernel data here.
    details=$(lsblk -dnbr -o MAJ:MIN,SIZE,RO -- "$device") || return 1
    read -r number bytes read_only <<<"$details"
    [[ -n $number && -n $bytes && -n $read_only ]] || return 1
    sys_path=$(readlink -e -- "/sys/dev/block/$number") || return 1
    if [[ -e $sys_path/diskseq ]]; then
        read -r sequence <"$sys_path/diskseq" || return 1
    fi
    printf '%s|%s|%s\n' "$details" "$sys_path" "$sequence"
}
size_bytes() { lsblk -dnbo SIZE "$1" | tr -d ' '; }
human() { numfmt --to=iec-i --suffix=B "$1" 2>/dev/null || echo "$1 bytes"; }

# Mount points that mean "this disk is part of the running system".
critical_mounts() {
    local mounts
    mounts=$(device_mounts "$1") || return 1
    grep -E '^(/|/boot|/boot/efi|/efi|/home|/usr|/var|/etc|/nix|/opt|\[SWAP\])$' <<<"$mounts" || [[ $? == 1 ]]
}

device_mounts() {
    lsblk -rno MOUNTPOINTS -- "$1" | sed 's/\\x0a/\n/g'
}

require_same_card() {
    local current
    current=$(identity "$DEVICE") || die "cannot read the card's identity"
    [[ $current == "$BEFORE" ]] || die "the card identity changed; stopping"
}

require_unmounted() {
    local mounts
    mounts=$(device_mounts "$DEVICE") || die "cannot check the card's mounts"
    [[ -z $mounts ]] || die "$DEVICE is mounted; close any file windows using it and try again"
}

flash_card() {
    local DEVICE=$1 IMAGE=$2 IMG_BYTES=$3 BEFORE=$4
    local nodes part result mounts
    # udev takes a shared BSD lock on the whole disk before probing filesystems.
    # Open without creating a file if the reader has disappeared.
    require_same_card
    exec 9<"$DEVICE"
    flock --exclusive --timeout 30 9 || die "could not lock $DEVICE; nothing was written"
    require_same_card
    mounts=$(critical_mounts "$DEVICE") || die "cannot check the card's system mounts"
    [[ -z $mounts ]] || die "$DEVICE now hosts a system mount; refusing"
    [[ $(field "$DEVICE" RO) == 0 ]] || die "$DEVICE is read-only"
    nodes=$(lsblk -nrpo NAME -- "$DEVICE" | tac)

    echo "Unmounting $DEVICE..."
    while IFS= read -r part; do
        if findmnt -rn --source "$part" >/dev/null; then
            umount --all-targets -- "$part" || die "could not unmount $part; nothing was written"
        fi
    done <<<"$nodes"
    require_unmounted
    require_same_card

    echo "Erasing old partition table and filesystem signatures..."
    # The caller holds the whole-disk lock. A second lock would block on it.
    LOCK_BLOCK_DEVICE=no wipefs --all --force -- "$DEVICE" >/dev/null
    require_same_card

    echo "Writing $(human "$IMG_BYTES")..."
    dd if="$IMAGE" of="$DEVICE" bs=4M conv=fsync,nocreat oflag=direct status=progress
    # Drop cached blocks before reading the card back. Leave the new partition
    # table unpublished so the desktop cannot auto-mount it during verification.
    blockdev --flushbufs "$DEVICE" || die "could not flush $DEVICE for read-back"
    require_same_card
    require_unmounted

    echo "Verifying every byte..."
    if cmp -n "$IMG_BYTES" -- "$IMAGE" "$DEVICE"; then
        echo "Verified: the card matches the image."
    else
        result=$?
        if (( result == 1 )); then
            die "VERIFY FAILED: the card does not match the image. Do not boot from it."
        fi
        die "VERIFY FAILED: could not read back the whole image. Do not boot from the card."
    fi

    echo "Ejecting..."
    require_same_card
    require_unmounted
    if command -v udisksctl >/dev/null && udisksctl power-off -b "$DEVICE" >/dev/null; then
        return 0
    fi
    if command -v eject >/dev/null && eject --no-unmount -- "$DEVICE"; then
        return 0
    fi
    die "the card was verified and unmounted, but automatic ejection failed"
}

candidates() {
    local line name type tran rm hot mounts
    while read -r line; do
        name=$(sed -n 's/.*NAME="\([^"]*\)".*/\1/p' <<<"$line")
        type=$(sed -n 's/.*TYPE="\([^"]*\)".*/\1/p' <<<"$line")
        tran=$(sed -n 's/.*TRAN="\([^"]*\)".*/\1/p' <<<"$line")
        rm=$(sed -n 's/.* RM="\([^"]*\)".*/\1/p' <<<"$line")
        hot=$(sed -n 's/.*HOTPLUG="\([^"]*\)".*/\1/p' <<<"$line")
        [[ $type == disk ]] || continue
        if [[ $rm == 1 || $hot == 1 || $tran == usb || $name == /dev/mmcblk* ]]; then
            mounts=$(critical_mounts "$name") || continue
            [[ -z $mounts ]] && echo "$name"
        fi
    done < <(lsblk -dPpo NAME,TYPE,TRAN,RM,HOTPLUG)
}

show_disk() {
    echo "  Device : $1"
    echo "  Size   : $(human "$(size_bytes "$1")")"
    echo "  Model  : $(field "$1" MODEL)"
    echo "  Serial : $(field "$1" SERIAL)"
    echo "  Bus    : $(field "$1" TRAN)"
    echo "  Contents:"
    lsblk -pno NAME,SIZE,FSTYPE,LABEL,MOUNTPOINTS "$1" | sed 's/^/    /'
}

if (( LIST )); then
    echo "Candidate microSD cards (removable or card-reader disks, none hosting the running system):"
    found=0
    while read -r name; do
        [[ -n $name ]] || continue
        found=1; echo; show_disk "$name"
    done < <(candidates)
    (( found )) || echo "  None found. Insert the card into the reader and run this again."
    exit 0
fi

[[ -n $DEVICE ]] || { usage >&2; die "choose a card with --device (see --list)"; }
[[ -b $DEVICE ]] || die "$DEVICE is not a block device"
[[ $(lsblk -dnro TYPE "$DEVICE") == disk ]] || die "$DEVICE is a partition; pass the whole disk (for example /dev/sdb, not /dev/sdb1)"
BEFORE=$(identity "$DEVICE") || die "cannot read the card's identity"
[[ -n $BOARD ]] || die "say which board this card is for with --board (scripts/detect-board.py --list)"
[[ -n $IMAGE ]] || IMAGE="$ROOT/$(python3 -I "$ROOT/scripts/detect-board.py" --field "$BOARD" output)/images/sdcard.img"
[[ -f $IMAGE ]] || die "image not found: $IMAGE (run make build first)"

python3 "$ROOT/scripts/detect-board.py" --require "$BOARD" >/dev/null || { python3 "$ROOT/scripts/detect-board.py" --require "$BOARD" >&2 || true; die "no image for $BOARD; refusing"; }
IMAGE_DIR=$(dirname -- "$IMAGE")
if (( PERSONALIZED )); then
    [[ ${IMAGE##*/} == sdcard.img ]] || die "select a private sdcard.img"
fi
if [[ $IMAGE == *.iso ]]; then
    [[ $BOARD == rpi4 ]] || die "the hybrid ISO requires Pi 4 / 400"
else
    STAMP="$IMAGE_DIR/board.txt"
    [[ -f $STAMP && $(<"$STAMP") == "$BOARD" ]] || die "the image was not built for $BOARD (${STAMP##*/} says: $(cat "$STAMP" 2>/dev/null || echo none)); rebuild for the right board"
fi

# Safety checks.
mounts=$(critical_mounts "$DEVICE") || die "cannot check the card's system mounts"
[[ -z $mounts ]] || die "$DEVICE hosts a system mount point; refusing"
root_src=$(findmnt --nofsroot -no SOURCE / 2>/dev/null || true)
if [[ -n $root_src && -b $root_src ]]; then
    root_disk=$(lsblk -nspo NAME,TYPE "$root_src" 2>/dev/null | awk '$2=="disk"{print $1}')
    for d in $root_disk; do [[ $d != "$DEVICE" ]] || die "$DEVICE holds this computer's root filesystem; refusing"; done
fi
if ! grep -qxF "$DEVICE" < <(candidates); then
    die "$DEVICE does not look like a removable card (not removable, hotplug, USB or mmcblk); refusing"
fi
BYTES=$(size_bytes "$DEVICE")
IMG_BYTES=$(stat -c %s "$IMAGE")
(( BYTES >= IMG_BYTES )) || die "card is smaller than the image ($(human "$BYTES") < $(human "$IMG_BYTES"))"
(( BYTES <= MAX_GIB * 1024 * 1024 * 1024 )) \
    || die "$DEVICE is $(human "$BYTES"), larger than ${MAX_GIB} GiB. That is unusual for a microSD card; pass --max-gib to override"
if (( PERSONALIZED )); then
    python3 "$ROOT/scripts/provision-sd.py" --verify "$IMAGE_DIR" --board "$BOARD" \
        || die "the private SD image failed verification; refusing to write it"
elif [[ $IMAGE == *.iso ]]; then
    python3 "$ROOT/scripts/iso_media.py" "$IMAGE" \
        || die "the ISO failed validation; refusing to write it"
else
    python3 "$ROOT/scripts/check-image.py" "$IMAGE_DIR" >/dev/null \
        || die "the image failed validation; refusing to write it"
    if [[ ! $IMAGE -ef "$IMAGE_DIR/sdcard.img" ]] && ! cmp -s -- "$IMAGE" "$IMAGE_DIR/sdcard.img"; then
        die "the selected image differs from the checked sdcard.img"
    fi
fi
require_same_card

echo "ANGI will ERASE this entire card and write a fresh image to it:"
echo
show_disk "$DEVICE"
echo
echo "  Image  : $IMAGE ($(human "$IMG_BYTES"))"
echo

if [[ -n $CONFIRM ]]; then
    [[ $CONFIRM == "$DEVICE" ]] || die "--confirm-erase ($CONFIRM) does not match --device ($DEVICE)"
    echo "Erase confirmed on the command line for $DEVICE."
else
    [[ -t 0 ]] || die "no terminal to confirm on. Re-run in a terminal, or pass --confirm-erase $DEVICE after the person approves"
    read -r -p "Type the device path ($DEVICE) to erase it, anything else to cancel: " answer
    [[ $answer == "$DEVICE" ]] || die "cancelled. Nothing was changed"
fi

SUDO=()
if (( EUID != 0 )); then
    command -v sudo >/dev/null || die "administrator access is required and sudo is missing"
    SUDO=(sudo)
fi
command -v flock >/dev/null || die "flock is required"
worker=$(declare -f die field identity human critical_mounts device_mounts require_same_card require_unmounted flash_card)
"${SUDO[@]}" /bin/bash -euo pipefail -c "$worker"$'\nflash_card "$@"' flash-sd "$DEVICE" "$IMAGE" "$IMG_BYTES" "$BEFORE"
echo "Done. $DEVICE is written, verified and ejected. You can remove it now."
