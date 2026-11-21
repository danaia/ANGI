"""Verify the ANGI UEFI disk layout and the artifacts packed inside it."""
import hashlib
import json
import struct
import subprocess

EFI = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b"
LINUX = "0fc63daf-8483-4772-8e79-3d69d8477de4"
TARGETS = {
    "aarch64": ("Image", "BOOTAA64.EFI", 0xAA64, 183),
    "x86_64": ("bzImage", "BOOTX64.EFI", 0x8664, 62),
}


def partitions(image):
    table = json.loads(subprocess.check_output(["sfdisk", "--json", image]))["partitiontable"]
    parts = table["partitions"]
    if (table["label"] != "gpt" or table["sectorsize"] != 512 or
            [p["type"].lower() for p in parts] != [EFI, LINUX]):
        raise ValueError("Expected a GPT with an EFI system partition and a Linux root partition")
    end = 34 * 512
    for part in parts:
        offset, length = part["start"] * 512, part["size"] * 512
        if offset < end or length <= 0 or offset + length > image.stat().st_size - 33 * 512:
            raise ValueError("Invalid UEFI partition bounds")
        end = offset + length
    return parts


def check_uefi(images, board):
    kernel, loader, pe_arch, elf_arch = TARGETS[board["arch"]]
    image = images / board["image"]
    boot, root = partitions(image)
    host = images.parent / "host"
    target = images.parent / "target"
    mtype = host / "bin/mtype"
    debugfs = host / "sbin/debugfs"
    offset = boot["start"] * 512
    with image.open("rb") as disk:
        for part, name in ((boot, "efi.vfat"), (root, "rootfs.ext4")):
            disk.seek(part["start"] * 512)
            pending = part["size"] * 512
            with (images / name).open("rb") as original:
                if pending != (images / name).stat().st_size:
                    raise ValueError(f"Packed partition has the wrong size: {name}")
                while pending:
                    count = min(pending, 1024 * 1024)
                    if disk.read(count) != original.read(count):
                        raise ValueError(f"Packed partition differs: {name}")
                    pending -= count

    def read_boot(name):
        return subprocess.check_output([mtype, "-i", f"{image}@@{offset}", f"::{name}"])

    efi = read_boot(f"EFI/BOOT/{loader}")
    if len(efi) < 64 or efi[:2] != b"MZ":
        raise ValueError("Invalid removable-media EFI loader")
    pe_offset = struct.unpack_from("<I", efi, 60)[0]
    if efi[pe_offset:pe_offset + 6] != b"PE\0\0" + struct.pack("<H", pe_arch):
        raise ValueError("EFI loader is for a different architecture")
    with (images / kernel).open("rb") as binary:
        digest = hashlib.file_digest(binary, "sha256").digest()
    if hashlib.sha256(read_boot(kernel)).digest() != digest:
        raise ValueError("Packed kernel differs from the build")
    config = read_boot("EFI/BOOT/grub.cfg").decode()
    if f"root=partuuid={root['uuid'].lower()}" not in config.lower():
        raise ValueError("GRUB configuration uses the wrong root partition")
    if "init=/usr/sbin/agx-init" not in config:
        raise ValueError("GRUB configuration must start ANGI")
    for name in ("usr/sbin/agx-init", "usr/bin/agx", "usr/sbin/agx-netd", "root/.ssh/authorized_keys"):
        built = (target / name).read_bytes()
        packed = subprocess.check_output([debugfs, "-R", f"cat /{name}", images / "rootfs.ext4"],
                                         stderr=subprocess.DEVNULL)
        if not packed or packed != built:
            raise ValueError(f"Missing or incorrect packed file: {name}")
        if name.startswith("usr/") and (packed[:6] != b"\x7fELF\x02\x01" or
                                       packed[18:20] != struct.pack("<H", elf_arch)):
            raise ValueError(f"{name} is for a different architecture")
    print(f"PASS: {board['id']} EFI loader, kernel, root filesystem and ANGI executables")
