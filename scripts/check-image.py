#!/usr/bin/env python3
"""Validate Raspberry Pi image artifacts without mounting or accessing block devices."""
import argparse
import hashlib
from pathlib import Path
import re
import struct
import subprocess
import sys


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest_region(stream, length):
    digest = hashlib.sha256()
    while length:
        chunk = stream.read(min(length, 1024 * 1024))
        require(chunk, "Truncated partition image")
        digest.update(chunk)
        length -= len(chunk)
    return digest.digest()


def check_arm64_files(target, read_packed, filenames):
    for name in filenames:
        binary = (target / name).read_bytes()
        require(binary.startswith(b'\x7fELF\x02\x01') and len(binary) >= 20,
                f'{name} must be a 64-bit little-endian ELF')
        require(struct.unpack_from('<H', binary, 18)[0] == 183, f'{name} must target ARM64')
        require(read_packed(name) == binary, f'Packed {name} differs from the build output')


PROFILES = {
    "rpi5": {"dtbs": ["bcm2712-rpi-5-b.dtb", "bcm2712d0-rpi-5-b.dtb"], "serial": "ttyAMA10",
             "wifi_fw": "brcmfmac43455-sdio.raspberrypi,5-model-b.bin", "label": "Pi 5", "extra_boot": []},
    "rpi4": {"dtbs": ["bcm2711-rpi-4-b.dtb", "bcm2711-rpi-400.dtb"], "serial": "ttyS0",
             "wifi_fw": "brcmfmac43455-sdio.raspberrypi,4-model-b.bin", "label": "Pi 4",
             "extra_boot": ["start4.elf", "fixup4.dat"]},
    "rpizero2w": {"dtbs": ["bcm2710-rpi-zero-2-w.dtb"], "serial": "ttyAMA0",
                 "wifi_fw": "brcmfmac43430-sdio.raspberrypi,model-zero-2-w.bin", "label": "Pi Zero 2 W",
                 "extra_boot": ["bootcode.bin", "start.elf", "fixup.dat", "start_cd.elf",
                                "fixup_cd.dat", "overlays/disable-bt.dtbo"]},
}


def check(images, expected_board=None):
    stamp = images / "board.txt"
    board = stamp.read_text().strip() if stamp.is_file() else "rpi5"
    require(board in PROFILES, f"Unknown board stamp: {board}")
    require(expected_board is None or board == expected_board,
            f"Image is for {board}, expected {expected_board}")
    profile = PROFILES[board]
    zero = board == "rpizero2w"
    build_config = (images.parent / '.config').read_text().splitlines()
    dtbs, serial, label = profile["dtbs"], profile["serial"], profile["label"]
    desktop_options = ('BR2_PACKAGE_AGX_GUI_HEADLESS=y',) if zero else (
        'BR2_PACKAGE_AGX_WEB=y', 'BR2_PACKAGE_AGX_WEB_SD_STORAGE=y')
    for option in ('BR2_PACKAGE_AGX_GUI=y', *desktop_options):
        require(option in build_config, f'{label} desktop image requires {option}')
    disk_image = images / "sdcard.img"
    for name in ("Image", *dtbs, "boot.vfat", "rootfs.ext4", "sdcard.img"):
        require((images / name).is_file(), f"Missing artifact: {name}")
    with (images / "Image").open("rb") as kernel:
        kernel.seek(56)
        require(kernel.read(4) == b"ARM\x64", "Invalid ARM64 kernel header")
    for name in dtbs:
        with (images / name).open("rb") as dtb:
            require(dtb.read(4) == b"\xd0\x0d\xfe\xed", f"Invalid DTB: {name}")
            if zero:
                require(b"raspberrypi,model-zero-2-w\0" in dtb.read(),
                        "Device tree does not identify a Raspberry Pi Zero 2 W")
    with (images / "rootfs.ext4").open("rb") as rootfs:
        rootfs.seek(1080)
        require(rootfs.read(2) == b"\x53\xef", "Invalid ext filesystem magic")
    if zero:
        with (images / "boot.vfat").open("rb") as boot:
            boot.seek(82)
            require(boot.read(8) == b"FAT32   ", "Zero 2 W requires a FAT32 boot partition")
    with disk_image.open("rb") as image:
        mbr = image.read(512)
        require(mbr[510:512] == b"\x55\xaa", "Invalid MBR signature")
        previous_end = 512
        for index, (name, kind) in enumerate((("boot.vfat", 0x0C), ("rootfs.ext4", 0x83))):
            entry = mbr[446 + 16 * index:462 + 16 * index]
            require(entry[4] == kind, f"Wrong partition type: {name}")
            if index == 0:
                require(entry[0] == 0x80, "Boot partition is not bootable")
            start, sectors = struct.unpack_from("<II", entry, 8)
            size = (images / name).stat().st_size
            require(start * 512 >= previous_end, "Overlapping partitions")
            require(size <= sectors * 512, f"Partition too small: {name}")
            require((start + sectors) * 512 <= disk_image.stat().st_size,
                    f"Partition outside disk image: {name}")
            previous_end = (start + sectors) * 512
            image.seek(start * 512)
            with (images / name).open("rb") as source:
                require(digest_region(image, size) == digest_region(source, size),
                        f"Partition contents differ: {name}")
    mtype = images.parent / "host/bin/mtype"
    boot_files = ["Image", *dtbs, "config.txt", "cmdline.txt", "overlays/dwc2.dtbo", *profile["extra_boot"]]
    if not zero:
        overlay_board = 'pi4' if board == 'rpi4' else 'pi5'
        boot_files.extend(('overlays/overlay_map.dtb', f'overlays/vc4-kms-v3d-{overlay_board}.dtbo'))
    for name in boot_files:
        subprocess.run([str(mtype), "-i", str(images / "boot.vfat"), f"::{name}"],
                       stdout=subprocess.DEVNULL, check=True)
    for name in ("config.txt", "cmdline.txt"):
        packed = subprocess.check_output([str(mtype), "-i", str(images / "boot.vfat"), f"::{name}"])
        require(packed == (images / "rpi-firmware" / name).read_bytes(),
                f"Packed boot file differs: {name}")
    config = (images / "rpi-firmware/config.txt").read_text()
    require("kernel=Image" in config, "Firmware does not select Image")
    require("dtoverlay=dwc2,dr_mode=peripheral" in config, "Missing USB device overlay")
    cmdline = (images / "rpi-firmware/cmdline.txt").read_text()
    usb_prefix = "79" if zero else "77"
    for token in ("root=/dev/mmcblk0p2", "rootwait", "init=/usr/sbin/agx-init", "logo.nologo", f"console={serial},115200",
                  f"g_ether.host_addr=02:{usb_prefix}:00:00:00:01", f"g_ether.dev_addr=02:{usb_prefix}:00:00:00:02"):
        require(token in cmdline.split(), f"Missing boot argument: {token}")
    require("console=tty1" not in cmdline.split(), "Kernel messages must not overwrite the HDMI login")
    require("disable_splash=1" in config, "Firmware splash must be disabled")
    inittab = (images.parent / "target/etc/inittab").read_text()
    for tty in ("tty1::respawn:", f"{serial}::respawn:"):
        require(tty in inittab, f"Missing login console: {tty}")
    target = images.parent / "target"
    require((target / "usr/sbin/dropbear").is_file(), "Missing SSH server")
    dropbear = (target / "etc/default/dropbear").read_text()
    require('-g -p 22' in dropbear and '-s' not in dropbear.split('DROPBEAR_ARGS=')[-1], "SSH must allow named-user passwords and retain key-only root")
    interfaces = (target / "etc/network/interfaces").read_text()
    require("auto usb0" in interfaces and f"address 10.{usb_prefix}.0.2" in interfaces,
            "Missing USB Ethernet address")
    if not zero:
        require("auto eth0" in interfaces and "address 10.78.0.2" in interfaces,
                "Missing RJ45 Ethernet address")
    kernel_config = next((images.parent / "build").glob("linux-*/.config")).read_text()
    if board == 'rpi5':
        require('dtparam=pciex1' in config.splitlines(), 'Pi 5 PCIe connector is disabled')
        for option in ('CONFIG_PCIE_BRCMSTB=y', 'CONFIG_BLK_DEV_NVME=y'):
            require(option in kernel_config.splitlines(), f'Missing Pi 5 NVMe support: {option}')
    for option in ("CONFIG_USB_GADGET=y", "CONFIG_USB_DWC2=y", "CONFIG_USB_DWC2_PERIPHERAL=y", "CONFIG_USB_ETH=y"):
        require(option in kernel_config.splitlines(), f"Missing built-in kernel option: {option}")
    require("# CONFIG_BLK_DEV_RAM is not set" in kernel_config.splitlines(),
            "Unused RAM block devices must be disabled")
    require("CONFIG_BRCMFMAC=m" in kernel_config.splitlines() and
            "CONFIG_BRCMFMAC_SDIO=y" in kernel_config.splitlines(),
            "Missing Broadcom SDIO Wi-Fi support")
    # Bluetooth: controller enabled at boot, kernel modules available, BlueZ/D-Bus, the service chain
    # and the Broadcom radio firmware.
    if zero:
        require("dtoverlay=disable-bt" in config.splitlines(), "Serial console needs the PL011 UART")
        require("arm_64bit=1" in config.splitlines(), "Zero 2 W firmware must boot in ARM64 mode")
        require("CONFIG_ZRAM=y" in kernel_config.splitlines(), "Missing built-in compressed swap")
    else:
        require("dtparam=krnbt=on" in config.splitlines(), "Bluetooth controller is not enabled")
        for option in ("CONFIG_BT=m", "CONFIG_BT_HCIUART=m"):
            require(option in kernel_config.splitlines(), f"Missing Bluetooth kernel option: {option}")
        for name in ("usr/sbin/agx-bluetoothd", "usr/libexec/bluetooth/bluetoothd", "usr/bin/dbus-daemon",
                     "usr/libexec/agx-bluetooth/run", "lib/firmware/brcm/BCM4345C0.hcd"):
            require((target / name).is_file(), f"Missing Bluetooth component: {name}")
        require((target / "usr/libexec/agx-bluetooth/run").stat().st_mode & 0o111, "Bluetooth service script is not executable")
    keys = target / "root/.ssh/authorized_keys"
    generic = (images.parent / ".generic-iso").is_file()
    if generic:
        require(not keys.exists(), "Generic image contains an authorized SSH key")
        for name in ("etc/agx/setup-enabled", "etc/agx/dhcp-usb.conf", "etc/avahi/avahi-daemon.conf",
                     "usr/sbin/udhcpd", "usr/sbin/avahi-daemon", "usr/bin/dropbearkey", "usr/bin/ssh-keygen"):
            require((target / name).is_file(), f"Missing browser setup prerequisite: {name}")
    else:
        require(keys.is_file() and keys.stat().st_size > 0, "Missing authorised SSH public key")
        require(not (target / 'etc/agx/setup-enabled').exists(),
                'The SSH setup image must start with browser setup disabled')
    debugfs = images.parent / "host/sbin/debugfs"

    def read_packed(name):
        return subprocess.run([str(debugfs), '-R', f'cat /{name}', str(images / 'rootfs.ext4')],
                              capture_output=True, check=True).stdout

    paths_and_modes = [
        ("etc/dropbear", 0o700), ("root/.ssh", 0o700),
        ("etc/agx", 0o700), ("etc/agx/network.json", 0o600), ("etc/agx/wpa_supplicant.conf", 0o600),
    ]
    if generic:
        paths_and_modes.append(("etc/agx/setup-enabled", 0o600))
    else:
        paths_and_modes.append(("root/.ssh/authorized_keys", 0o600))
    for name, mode in paths_and_modes:
        metadata = subprocess.run([str(debugfs), "-R", f"stat /{name}", str(images / "rootfs.ext4")],
                                  capture_output=True, check=True).stdout.decode()
        actual = re.search(r"\bMode:\s+([0-7]+)\b", metadata)
        if name == "etc/dropbear":
            require("Type: directory" in metadata, "Missing persistent SSH host-key directory")
        if name == "etc/agx/setup-enabled":
            require("Type: regular" in metadata, "Browser setup is not enabled in the generic ISO")
        require(actual is not None and int(actual.group(1), 8) == mode,
                f"Incorrect packed SSH permissions: {name}")
        require(re.search(r"User:\s+0\s+Group:\s+0\b", metadata),
                f"Incorrect packed SSH ownership: {name}")
    for directory in (("root/.ssh", "etc/dropbear") if generic else ("etc/dropbear",)):
        entries = subprocess.check_output([str(debugfs), "-R", f"ls -p /{directory}", str(images / "rootfs.ext4")],
                                          stderr=subprocess.DEVNULL, text=True)
        names = {line.split("/")[5] for line in entries.splitlines() if line.startswith("/")}
        require(names == {".", ".."}, f"Image contains unexpected SSH files in /{directory}")
    if generic:
        shadow = subprocess.check_output([str(debugfs), "-R", "cat /etc/shadow", str(images / "rootfs.ext4")],
                                         stderr=subprocess.DEVNULL)
        require(re.search(rb"^root:!:", shadow, re.MULTILINE), "Generic image must lock the root password")
    packed_files = ["etc/inittab", "etc/network/interfaces", "etc/default/dropbear", "etc/issue",
                    "etc/profile.d/agx-welcome.sh", "etc/init.d/S12agx-data", "usr/share/agx/logo.ascii"]
    generic_defaults = set()
    if generic:
        generic_defaults = {"etc/agx/network.json", "etc/agx/wpa_supplicant.conf",
                            "etc/agx/dhcp-usb.conf", "etc/avahi/avahi-daemon.conf"}
        packed_files.extend(sorted(generic_defaults))
        require("CONFIG_USB_ETH_RNDIS=y" in kernel_config.splitlines(), "Missing USB RNDIS gadget configuration")
        overlay = Path(__file__).resolve().parents[1] / "buildroot/board/agx/rootfs-overlay"
    else:
        packed_files.append("root/.ssh/authorized_keys")
        marker = subprocess.check_output([str(debugfs), '-R', 'stat /etc/agx/setup-enabled',
                                          str(images / 'rootfs.ext4')], stderr=subprocess.DEVNULL)
        require(not marker.startswith(b'Inode:'), 'Packed SSH setup image enables browser setup')
    packed_files.append('etc/agx/gui.env')
    if zero:
        packed_files.extend(('etc/modprobe.d/angi-zero-wifi.conf', 'usr/libexec/agx-zram'))
        require('SLINT_BACKEND=vnc-software' in (target / 'etc/agx/gui.env').read_text().splitlines(),
                'Zero 2 W must use the software display')
        require('options brcmfmac feature_disable=0x2000' in
                (target / 'etc/modprobe.d/angi-zero-wifi.conf').read_text().splitlines(),
                'Zero 2 W must handle WPA2 in wpa_supplicant')
        require((target / 'usr/libexec/agx-zram').stat().st_mode & 0o111,
                'Compressed swap setup must be executable')
    else:
        require('dtoverlay=vc4-kms-v3d' in config.splitlines(), f'Missing {label} graphics overlay')
        for driver in ('CONFIG_DRM_VC4', 'CONFIG_DRM_V3D'):
            require(any(f'{driver}={kind}' in kernel_config.splitlines() for kind in ('y', 'm')),
                    f'Missing {label} graphics driver: {driver}')
        for option in ('CONFIG_USER_NS=y', 'CONFIG_SECCOMP_FILTER=y'):
            require(option in kernel_config.splitlines(), f'Missing browser sandbox option: {option}')
        packed_files.append('etc/modules-load.d/desktop.conf' if board == 'rpi4'
                            else 'etc/modules-load.d/agx-display.conf')
    require('AGX_GUI_VNC=on' in (target / 'etc/agx/gui.env').read_text().splitlines(),
            f'{label} must enable loopback VNC')
    require((target / 'usr/bin/open').readlink() == Path('agx-gui'), 'Missing desktop open command')
    browser_binaries = () if zero else (
        'usr/lib/gbm/dri_gbm.so', 'usr/libexec/agx-web', 'usr/libexec/wpe-webkit-2.0/WPEWebProcess',
        'usr/bin/bwrap', 'usr/bin/xdg-dbus-proxy')
    check_arm64_files(target, read_packed, ('usr/bin/agx-gui', *browser_binaries))
    for name in packed_files:
        packed = read_packed(name)
        require(packed == (target / name).read_bytes(), f"Packed root filesystem differs: {name}")
        if name in generic_defaults:
            require(packed == (overlay / name).read_bytes(), f"Generic image contains unexpected network settings: {name}")
    print(f'PASS: {label} ANGI desktop and loopback VNC configuration'
          + (', with graphics and browser' if not zero else ''))
    if board == 'rpi5':
        print('PASS: Pi 5 PCIe and NVMe kernel support, data-volume mount script')
    if 'BR2_PACKAGE_E2FSPROGS=y' in build_config:
        require((target / 'sbin/mke2fs').is_file() and not (target / 'sbin/mke2fs').is_symlink(),
                'Real e2fsprogs mke2fs is required for ext4 data volumes')
    if 'BR2_PACKAGE_AGX=y' in build_config:
        check_arm64_files(target, read_packed, ('usr/bin/agx', 'usr/sbin/agx-netd', 'usr/sbin/agx-dhcp',
                                               'usr/sbin/agx-init'))
        require('/usr/sbin/agx-init --supervise' in inittab, 'BusyBox recovery must start the Rust supervisor')
        wifi_firmware = [profile['wifi_fw']]
        if zero:
            wifi_firmware.append("brcmfmac43436-sdio.raspberrypi,model-zero-2-w.bin")
        for name in ('usr/sbin/wpa_supplicant', 'usr/sbin/iw', 'usr/bin/curl', 'etc/ssl/certs/ca-certificates.crt',
                     'lib/firmware/regulatory.db', *('lib/firmware/brcm/' + fw for fw in wifi_firmware)):
            require((target / name).exists(), f'Missing Wi-Fi prerequisite: {name}')
        print('PASS: ARM64 AGX/TUI/network executables, startup and Wi-Fi prerequisites')
    print(f"PASS: ARM64 kernel, {label} DTBs, boot files, consoles, SSH setup and disk partitions")
    print(f"Hardware boot remains unverified until tested on a Raspberry {label}.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("images", type=Path, nargs="?", default=Path("output/images"))
    parser.add_argument("--board", choices=PROFILES)
    args = parser.parse_args()
    try:
        check(args.images.resolve(), args.board)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"FAIL: {error}")
