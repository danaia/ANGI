#!/usr/bin/env bash
set -euo pipefail

repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
usage="Usage: ${0##*/} [--vm] [IMAGE [STATE_DIR]]"

if [[ ${1:-} == --help || ${1:-} == -h ]]; then
    echo "$usage"
    echo "Open the native, resizable ANGI desktop preview by default."
    echo "Use --vm to boot ANGI in QEMU/KVM. Passing IMAGE also selects QEMU/KVM."
    echo "The default VM state directory is .cache/angi-vm/desktop."
    echo "Set OVMF_DIR to override the UEFI firmware directory."
    exit 0
fi

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

if (( $# == 0 )); then
    rustup_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
    if [[ -x "$rustup_bin/cargo" ]] && ! cargo --version >/dev/null 2>&1; then
        export PATH="$rustup_bin:$PATH"
    fi
    export SLINT_BACKEND=winit-femtovg
    exec "$repo/scripts/cargo.sh" run --locked --manifest-path crates/agx-gui/Cargo.toml
fi

if [[ $1 == --vm ]]; then
    shift
fi
(( $# <= 2 )) || fail "$usage"

cache="$repo/.cache/angi-vm"
[[ -r /dev/kvm && -w /dev/kvm ]] || fail "The launcher needs read and write access to /dev/kvm."
state=${2:-"$cache/desktop"}
mkdir -p -m 700 -- "$state"
state=$(realpath -e -- "$state")
exec 9>"$state/lock"
flock -n 9 || fail "ANGI is already running from $state."

source_image=${1:-}
if [[ -z $source_image && ! -f "$state/disk.img" ]]; then
    images="$repo/output-x86-desktop/images"
    if [[ -f "$images/disk.img" ]] && grep -qx x86-desktop "$images/board.txt" 2>/dev/null; then
        source_image="$images/disk.img"
    elif [[ -f "$cache/preview.img" ]]; then
        source_image="$cache/preview.img"
    else
        fail "Build the x86-desktop image or pass an ANGI x86-64 disk image to this launcher."
    fi
fi
if [[ -n $source_image ]]; then
    [[ -f $source_image && -r $source_image ]] || fail "Cannot read VM image $source_image."
    source_image=$(realpath -e -- "$source_image")
    if [[ -f "$state/disk.img" ]] && [[ $(cat "$state/source-image" 2>/dev/null || true) != "$source_image" ]]; then
        fail "This VM directory already has a disk from another image. Pass a new STATE_DIR to start a separate guest."
    fi
fi

qemu=$(command -v qemu-system-x86_64 || true)
qemu_args=()
if [[ -z $qemu ]]; then
    runtime="$cache/runtime/usr"
    qemu="$runtime/bin/qemu-system-x86_64"
    [[ -x $qemu ]] || fail "Install QEMU with GTK and VirtIO VGA support. On Arch Linux, use qemu-desktop."
    export LD_LIBRARY_PATH="$runtime/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    export QEMU_MODULE_DIR="$runtime/lib/qemu"
    qemu_args=(-L "$runtime/share/qemu")
fi

firmware_dirs=(/usr/share/edk2/x64 /usr/share/OVMF /usr/share/edk2/ovmf "$cache/runtime/usr/share/edk2/x64")
if [[ -n ${OVMF_DIR:-} ]]; then
    firmware_dirs=("$OVMF_DIR")
fi
code=
vars=
for directory in "${firmware_dirs[@]}"; do
    for suffix in .4m.fd _4M.fd .fd; do
        if [[ -r "$directory/OVMF_CODE$suffix" && -r "$directory/OVMF_VARS$suffix" ]]; then
            code="$directory/OVMF_CODE$suffix"
            vars="$directory/OVMF_VARS$suffix"
            break 2
        fi
    done
done
[[ -n $code ]] || fail "Install OVMF or set OVMF_DIR to a directory with matching OVMF_CODE and OVMF_VARS files."

copy_for_guest() {
    local partial
    partial=$(mktemp -- "$2.XXXXXXXX")
    if ! cp --reflink=auto --sparse=always -- "$1" "$partial"; then
        unlink -- "$partial"
        fail "Could not copy $1 into the VM directory."
    fi
    mv -- "$partial" "$2"
}

if [[ ! -f "$state/disk.img" ]]; then
    printf '%s\n' "$source_image" > "$state/source-image"
    copy_for_guest "$source_image" "$state/disk.img"
    printf 'Created an ANGI VM disk from %s\n' "$source_image"
fi
if [[ ! -f "$state/OVMF_VARS.fd" ]]; then
    copy_for_guest "$vars" "$state/OVMF_VARS.fd"
fi

printf 'Starting ANGI with %s\n' "$state/disk.img"
exec "$qemu" "${qemu_args[@]}" \
    -name 'ANGI OS' -machine q35,accel=kvm -cpu host -smp 2 -m 2048 \
    -drive "if=pflash,unit=0,format=raw,readonly=on,file=${code//,/,,}" \
    -drive "if=pflash,unit=1,format=raw,file=${state//,/,,}/OVMF_VARS.fd" \
    -drive "if=virtio,format=raw,file=${state//,/,,}/disk.img" \
    -vga none -device virtio-vga -display gtk,gl=off \
    -netdev user,id=net0 -device virtio-net-pci,netdev=net0,romfile= \
    -usb -device usb-tablet
