"""Manage the filesystem used for ANGI data storage."""
from contextlib import contextmanager
import fcntl
import json
import os
from pathlib import Path
import re
import shlex
import stat
import subprocess
import sys
import tempfile
import uuid as filesystem_uuid

DATA = Path("/var/lib/agx-data")
CONFIG = Path("/etc/agx/data-volume.uuid")
PARTITION = re.compile(r"nvme[0-9]+n[0-9]+p[0-9]+\Z")
SATA_PARTITION = re.compile(r"sd[a-z]+[0-9]+\Z")
DISK = re.compile(r"nvme[0-9]+n[0-9]+\Z")
UUID = re.compile(r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\Z")
BLOCK = Path("/sys/class/block")
DEV = Path("/dev")
SFDISK = "/sbin/sfdisk"
MKFS = "/sbin/mkfs.ext4"
# Linux BLKGETDISKSEQ returns the kernel generation of the opened disk.
BLKGETDISKSEQ = 0x80081280


def command(*arguments, timeout=15, **options):
    return subprocess.run(arguments, capture_output=True, text=True, timeout=timeout,
                          check=True, **options).stdout


def mounted_devices():
    return {line.split()[2] for line in Path("/proc/self/mountinfo").read_text().splitlines()}


def data_partition(partition):
    if PARTITION.fullmatch(partition.name):
        return True
    if not SATA_PARTITION.fullmatch(partition.name):
        return False
    disk = partition.resolve(strict=True).parent
    return ((disk / "removable").read_text().strip() == "0"
            and any(re.fullmatch(r"ata[0-9]+", component) for component in disk.parts))


def volumes():
    mounted = mounted_devices()
    found = []
    for partition in sorted(BLOCK.iterdir()):
        path = DEV / partition.name
        try:
            if (not data_partition(partition) or (partition / "dev").read_text().strip() in mounted
                    or not path.is_block_device()):
                continue
            output = command("/sbin/blkid", str(path)).partition(":")[2]
            fields = dict(field.split("=", 1) for field in shlex.split(output) if "=" in field)
            uuid = fields.get("UUID", "")
            if fields.get("TYPE") == "ext4" and UUID.fullmatch(uuid):
                found.append({"path": str(path), "uuid": uuid, "label": fields.get("LABEL", ""),
                              "bytes": int((partition / "size").read_text()) * 512})
        except (OSError, ValueError, subprocess.SubprocessError):
            continue
    return found


def format_ready():
    return all(os.access(tool, os.X_OK) for tool in (SFDISK, MKFS))


def disk_parts(disk):
    return [child for child in disk.iterdir() if (child / "partition").is_file()]


def devices_in_use():
    active = mounted_devices()
    swaps = Path("/proc/swaps")
    if swaps.exists():
        for line in swaps.read_text().splitlines()[1:]:
            path, kind, *_ = line.split()
            if kind != "partition":
                continue
            info = Path(path).stat()
            if stat.S_ISBLK(info.st_mode):
                active.add(f"{os.major(info.st_rdev)}:{os.minor(info.st_rdev)}")
    return active


def disk_available(disk, active):
    return all((node / "ro").read_text().strip() == "0"
               and (node / "dev").read_text().strip() not in active
               and not any((node / "holders").iterdir())
               for node in [disk, *disk_parts(disk)])


def format_targets():
    active = devices_in_use()
    found = []
    for disk in sorted(BLOCK.glob("nvme*n*")):
        if not DISK.fullmatch(disk.name):
            continue
        try:
            path = DEV / disk.name
            sequence = int((disk / "diskseq").read_text())
            size = int((disk / "size").read_text()) * 512
            model_path = disk / "device/model"
            model = model_path.read_text().strip() if model_path.is_file() else ""
            if size and path.is_block_device() and disk_available(disk, active):
                found.append({"path": str(path), "label": model, "bytes": size,
                              "id": f"{disk.name}:{sequence}"})
        except (OSError, ValueError):
            continue
    return found


@contextmanager
def disk_device(node, sequence):
    """Keep destructive tools attached to the device opened for this request."""
    fd = os.open(DEV / node.name, os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        info = os.fstat(fd)
        device = f"{os.major(info.st_rdev)}:{os.minor(info.st_rdev)}"
        if not stat.S_ISBLK(info.st_mode) or (node / "dev").read_text().strip() != device:
            raise RuntimeError("The selected NVMe device changed. Open the drive picker again.")
        generation = bytearray(8)
        fcntl.ioctl(fd, BLKGETDISKSEQ, generation)
        if int.from_bytes(generation, sys.byteorder) != sequence:
            raise RuntimeError("The selected NVMe device changed. Open the drive picker again.")
        yield f"/proc/self/fd/{fd}", fd
    finally:
        os.close(fd)


def remember(uuid):
    with tempfile.NamedTemporaryFile(mode="w", dir=CONFIG.parent, delete=False) as record:
        pending = Path(record.name)
        try:
            record.write(uuid + "\n")
            record.flush()
            os.fsync(record.fileno())
            pending.replace(CONFIG)
        finally:
            pending.unlink(missing_ok=True)


@contextmanager
def mount_target():
    with os.fdopen(os.open("/run/agx-data-volume.lock", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600), "r+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if DATA.is_symlink() or DATA.is_mount():
            raise RuntimeError("The ANGI data volume is already mounted or its mount point is unavailable.")
        DATA.mkdir(exist_ok=True)
        # Buildroot packages this empty directory for OpenCode's SD fallback.
        # Cover it only while it is empty, so users retain access to any data.
        if any(entry.name != "agx-opencode-users" or entry.is_symlink()
               or not entry.is_dir() or any(entry.iterdir()) for entry in DATA.iterdir()):
            raise RuntimeError("The ANGI data mount point contains files. Move those files before mounting.")
        yield


def mount_and_remember(path, uuid, **options):
    command("/bin/mount", "-t", "ext4", "-o", "noatime,nosuid,nodev", path, str(DATA), **options)
    try:
        info = DATA.stat()
        if info.st_uid != 0 or info.st_mode & 0o022:
            raise RuntimeError("The data filesystem root must be root-owned and protected from group or other writes.")
        command("/etc/init.d/S12agx-data", "prepare")
        gui = Path("/usr/bin/agx-gui")
        if gui.is_file() and not gui.is_symlink():
            command(str(gui), "--prepare-web-storage")
        remember(uuid)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError):
        command("/bin/umount", str(DATA))
        raise


def mount(uuid):
    if not UUID.fullmatch(uuid):
        raise ValueError("Choose an existing ext4 partition.")
    with mount_target():
        matches = [entry for entry in volumes() if entry["uuid"] == uuid]
        if len(matches) != 1:
            raise RuntimeError("The selected partition is missing, mounted, or has a duplicate filesystem UUID.")
        mount_and_remember(matches[0]["path"], uuid)


def format_and_mount(identifier):
    name, _, generation = identifier.partition(":")
    if not DISK.fullmatch(name) or not generation.isascii() or not generation.isdecimal():
        raise ValueError("Choose an NVMe drive to erase.")
    if not format_ready():
        raise RuntimeError("Quick format requires the ANGI disk-formatting tools.")
    disk = BLOCK / name
    sequence = int(generation)
    uuid = str(filesystem_uuid.uuid4())
    partition_name = f"ANGI Data {uuid[:18]}"
    with mount_target():
        if int((disk / "diskseq").read_text()) != sequence or not disk_available(disk, devices_in_use()):
            raise RuntimeError("The selected NVMe drive changed or is in use. Open the drive picker again.")
        with disk_device(disk, sequence) as (path, fd):
            # sfdisk independently refuses busy devices.
            command(SFDISK, "--wipe", "always", "--wipe-partitions", "always", path,
                    input=f'label: gpt\n\nname="{partition_name}"\n', pass_fds=(fd,), timeout=60)
        partition = disk / f"{name}p1"
        # sfdisk can succeed even if the kernel kept the previous partition map.
        if (disk_parts(disk) != [partition]
                or f"PARTNAME={partition_name}" not in (partition / "uevent").read_text().splitlines()
                or not disk_available(disk, devices_in_use())):
            raise RuntimeError("The new NVMe partition is unavailable or in use. Open the drive picker again.")
        with disk_device(partition, sequence) as (path, fd):
            command(MKFS, "-q", "-L", "ANGI Data", "-U", uuid, "-m", "0",
                    "-E", "nodiscard,lazy_itable_init=1,lazy_journal_init=1",
                    path, pass_fds=(fd,), timeout=120)
            try:
                mount_and_remember(path, uuid, pass_fds=(fd,))
            except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
                detail = error.stderr.strip() if isinstance(error, subprocess.CalledProcessError) else str(error)
                raise RuntimeError(f"The drive was formatted, but mounting failed. "
                                   f"Reopen the picker and choose Mount to retry. {detail}") from error


def main(args):
    if os.geteuid() != 0:
        raise RuntimeError("Mounting the ANGI data volume requires administrator access.")
    if args == ["list"]:
        print(json.dumps(volumes()))
    elif args == ["list-picker"]:
        ready = format_ready()
        print(json.dumps({"partitions": volumes(), "drives": format_targets() if ready else [],
                          "format_ready": ready}))
    elif len(args) == 2 and args[0] == "mount":
        mount(args[1])
    elif len(args) == 2 and args[0] == "format":
        format_and_mount(args[1])
    else:
        raise ValueError("Choose a data volume action.")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except subprocess.CalledProcessError as error:
        sys.exit(error.stderr.strip() or "The system could not open the selected partition.")
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
