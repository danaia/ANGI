"""Verify the ANGI UEFI disk layout and the artifacts packed inside it."""
import hashlib
import json
import re
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
    subprocess.run(["sfdisk", "--verify", image], stdout=subprocess.DEVNULL, check=True)
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
        header = binary.read(1024)
        if board["arch"] == "aarch64":
            kernel_matches = header[56:60] == b"ARM\x64"
        else:
            kernel_matches = (len(header) > 0x236 and header[0x202:0x206] == b"HdrS"
                              and bool(header[0x236] & 1))
        if not kernel_matches:
            raise ValueError("Linux kernel header is for a different architecture")
        binary.seek(0)
        digest = hashlib.file_digest(binary, "sha256").digest()
    if hashlib.sha256(read_boot(kernel)).digest() != digest:
        raise ValueError("Packed kernel differs from the build")
    config = read_boot("EFI/BOOT/grub.cfg").decode()
    if f"root=partuuid={root['uuid'].lower()}" not in config.lower():
        raise ValueError("GRUB configuration uses the wrong root partition")
    if "init=/usr/sbin/agx-init" not in config:
        raise ValueError("GRUB configuration must start ANGI")
    def read_root(name, command="cat"):
        return subprocess.check_output(
            [debugfs, "-R", f"{command} /{name}", images / "rootfs.ext4"],
            stderr=subprocess.DEVNULL,
        )

    generic = board["arch"] == "x86_64" and (images.parent / ".generic-iso").is_file()
    files = ["usr/sbin/agx-init", "usr/bin/agx", "usr/sbin/agx-netd"]
    if generic:
        files.extend(("usr/bin/agx-gui", "etc/init.d/S10agx-hardware"))
        check_public_setup(read_root, config)
        build_config = images.parent / ".config"
        options = build_config.read_text().splitlines() if build_config.is_file() else []
        if board["id"] == "x86-desktop" and "BR2_PACKAGE_AGX_WEB=y" not in options:
            raise ValueError("The x86 desktop image must include ANGI Web")
        if "BR2_PACKAGE_AGX_WEB=y" in options:
            files.extend(("usr/libexec/agx-web", "usr/libexec/wpe-webkit-2.0/WPEWebProcess", "usr/bin/bwrap"))
            webkit = "usr/lib/libWPEWebKit-2.0.so.1"
            library = (target / webkit).resolve(strict=True)
            if b"Inode:" not in read_root(webkit, "stat"):
                raise ValueError("The x86 Web panel needs the packaged WPE WebKit library")
            files.append(library.relative_to(target.resolve()).as_posix())
            if b"Inode:" not in read_root("usr/lib/agx-web-mesa/libEGL.so.1", "stat"):
                raise ValueError("The x86 Web panel needs its isolated Mesa fallback")
    else:
        files.append("root/.ssh/authorized_keys")
    for name in files:
        built = (target / name).read_bytes()
        packed = read_root(name)
        if not packed or packed != built:
            raise ValueError(f"Missing or incorrect packed file: {name}")
        if name.startswith("usr/") and (packed[:6] != b"\x7fELF\x02\x01" or
                                       packed[18:20] != struct.pack("<H", elf_arch)):
            raise ValueError(f"{name} is for a different architecture")
    print(f"PASS: {board['id']} EFI loader, kernel, root filesystem and ANGI executables")


def check_public_setup(read_root, grub_config):
    marker = read_root("etc/agx/setup-enabled", "stat")
    if b"Type: regular" not in marker or not re.search(rb"\bMode:\s+0600\b", marker):
        raise ValueError("The public image must start in first-boot setup")
    if b"Inode:" in read_root("etc/agx/wired-ssh-only", "stat"):
        raise ValueError("The public image contains private SSH provisioning state")
    for directory in ("root/.ssh", "etc/dropbear"):
        entries = read_root(directory, "ls -p").splitlines()
        names = {line.split(b"/")[5] for line in entries if line.startswith(b"/")}
        if names != {b".", b".."}:
            raise ValueError(f"The public image contains provisioned SSH files in /{directory}")
    if not any(line.startswith(b"root:!:") for line in read_root("etc/shadow").splitlines()):
        raise ValueError("The public image must lock the root password")
    wired = read_root("usr/libexec/agx-wired-network")
    direct = read_root("usr/share/udhcpc/default.script.d/50-agx-direct-ethernet")
    if b"inet dhcp" not in wired or b"10.78.0.2/24" not in direct:
        raise ValueError("The public image needs wired DHCP discovery and the direct-cable fallback")
    for path in ("usr/lib/libsbc.so.1", "usr/lib/pulseaudio/modules/module-bluez5-discover.so"):
        if b"Inode:" not in read_root(path, "stat"):
            raise ValueError(f"The public image is missing Bluetooth audio support: /{path}")
    if 'menuentry "ANGI (graphics recovery)"' not in grub_config:
        raise ValueError("The public image is missing its graphics recovery boot option")
    for path in ("rules/evdev", "keycodes/evdev", "symbols/us"):
        if not read_root(f"usr/share/X11/xkb/{path}"):
            raise ValueError(f"The public setup image is missing keyboard data: {path}")
    for path in ("usr/lib/libEGL.so.1", "usr/lib/libgbm.so.1", "usr/lib/dri"):
        if b"Inode:" in read_root(path, "stat"):
            raise ValueError(f"The public bootstrap image contains a graphics runtime at /{path}")
    feed = b"Inode:" in read_root("etc/agx/driver-catalog.json", "stat")
    signers = b"Inode:" in read_root("etc/agx/driver-signers", "stat")
    if feed != signers:
        raise ValueError("The online driver feed needs its catalog configuration and release public key")
