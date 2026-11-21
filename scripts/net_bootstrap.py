"""Assemble the public, RAM-only ANGI network bootstrap from a local build."""
import gzip
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from detect_board_registry import BY_ID, ROOT
from ssh_provisioning import checked_public_key

ASSETS = ROOT / "buildroot/board/agx/net-install"
HOST = "10.78.0.1"
CLIENT = "10.78.0.2"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def pack_runtime(target, root):
    lib_dirs = (target / "lib", target / "lib64", target / "usr/lib", target / "usr/lib64")
    binaries = ("busybox", "dropbear", "lsblk", "wipefs", "blockdev", "flock")
    bin_dirs = tuple(target / path for path in ("bin", "sbin", "usr/bin", "usr/sbin"))

    def locate(name, directories):
        for directory in directories:
            path = (directory / name).resolve()
            if path.is_relative_to(target) and path.is_file():
                return path
        raise ValueError(f"The target build is missing {name}")

    def copy_elf(source, destination):
        if destination.exists():
            return
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        headers = run("readelf", "-l", "-d", source, capture_output=True, text=True).stdout
        interp = re.search(r"Requesting program interpreter: ([^\]]+)", headers)
        if interp:
            path = interp[1].lstrip("/")
            copy_elf(locate(Path(path).name, lib_dirs), root / path)
        for name in re.findall(r"\(NEEDED\).*?\[([^\]]+)\]", headers):
            copy_elf(locate(name, lib_dirs), root / "lib" / name)

    for name in binaries:
        copy_elf(locate(name, bin_dirs), root / "usr/bin" / name)
    (root / "bin/busybox").symlink_to("../usr/bin/busybox")
    (root / "bin/sh").symlink_to("busybox")
    (root / "usr/bin/dropbearkey").symlink_to("dropbear")


def pack_initramfs(output, target, public_key):
    with tempfile.TemporaryDirectory(prefix="angi-net-root-") as temp:
        root = Path(temp)
        for path in ("bin", "sbin", "proc", "sys", "dev", "run", "etc", "tmp", "root/.ssh"):
            (root / path).mkdir(parents=True, exist_ok=True)
        pack_runtime(target, root)
        shutil.copy2(ASSETS / "init", root / "init")
        shutil.copy2(ASSETS / "disk", root / "usr/bin/angi-install-disk")
        (root / "root/.ssh/authorized_keys").write_bytes(checked_public_key(public_key))
        (root / "etc/passwd").write_text("root:x:0:0:root:/root:/bin/sh\n")
        (root / "etc/shadow").write_text("root:*:0:0:99999:7:::\n")
        (root / "etc/group").write_text("root:x:0:\n")
        (root / "etc/shells").write_text("/bin/sh\n")
        manifest = b"\0".join(os.fsencode(path.relative_to(root)) for path in sorted(root.rglob("*"))) + b"\0"
        with gzip.open(output, "wb") as destination:
            archive = run("cpio", "--null", "--create", "--format=newc", "--owner=0:0", "--quiet",
                          input=manifest, cwd=root, capture_output=True).stdout
            destination.write(archive)


def make_bootstrap(board, public_key, directory):
    config = BY_ID[board]
    output = ROOT / config["output"]
    images = output / "images"
    kernel = "bzImage" if config["arch"] == "x86_64" else "Image"
    shutil.copy2(images / kernel, directory / kernel)
    pack_initramfs(directory / "installer.cpio.gz", (output / "target").resolve(), public_key)
    if board in ("rpi4", "rpi5"):
        names = ("start4.elf", "fixup4.dat") if board == "rpi4" else ()
        for name in (*names, *(name + ".dtb" for name in config["dtbs"])):
            source = images / name
            shutil.copy2(source if source.is_file() else images / "rpi-firmware" / name, directory)
        serial = config["serial"]
        (directory / "config.txt").write_text(
            "arm_64bit=1\nkernel=Image\nenable_uart=1\ninitramfs installer.cpio.gz followkernel\n"
            + ("dtparam=pciex1\n" if board == "rpi5" else ""))
        (directory / "cmdline.txt").write_text(
            f"angi.netinstall=1 console={serial},115200 console=tty1\n")
    else:
        platform = "x86_64-efi" if board == "x86-desktop" else "arm64-efi"
        modules, = output.glob(f"build/grub2-*/build-{platform}/grub-core")
        grub = directory / "bootstrap.cfg"
        grub.write_text(
            f"net_bootp\nlinux (tftp,{HOST})/{kernel} angi.netinstall=1 console=tty1\n"
            f"initrd (tftp,{HOST})/installer.cpio.gz\nboot\n")
        run(output / "host/bin/grub-mkimage", "-d", modules, "-O", platform, "-p", "/",
            "-c", grub, "-o", directory / "bootstrap.efi", "efinet", "net", "tftp", "boot",
            "linux", "normal", "efi_gop")
