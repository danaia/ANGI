"""Prepare workstation login keys and personalize an offline ANGI root filesystem."""

import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


LOGIN_KEY = "/root/.ssh/authorized_keys"
HOST_KEY = "/etc/dropbear/dropbear_ed25519_host_key"
# A card prepared with the person's own workstation key and a pinned host key has already been
# authorized for wired SSH; an unprovisioned image starts with Ethernet and USB closed.
SECURITY_POLICY = "/etc/agx/security.conf"
PROVISIONED_POLICY = b"ethernet=open\nusb=open\n"


def checked_public_key(path):
    lines = path.read_bytes().splitlines()
    if len(lines) != 1 or not re.match(rb"^(ssh-ed25519|ssh-rsa|ecdsa-sha2-[^ ]+) ", lines[0]):
        raise ValueError("Select one OpenSSH public key")
    subprocess.run(["ssh-keygen", "-l", "-f", str(path)], stdout=subprocess.DEVNULL, check=True)
    return lines[0] + b"\n"


def available_public_key(identity):
    public = identity.with_suffix(".pub")
    if not os.path.lexists(identity) and not os.path.lexists(public):
        with tempfile.TemporaryDirectory(dir=identity.parent, prefix=".angi-ssh-") as directory:
            pending = Path(directory) / "key"
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "",
                            "-C", "ANGI provisioning", "-f", str(pending)], check=True)
            try:
                os.link(pending, identity)
            except FileExistsError:
                pass
    if not identity.is_file():
        return None
    derived = subprocess.run(["ssh-keygen", "-y", "-P", "", "-f", str(identity)],
                             stdin=subprocess.DEVNULL, capture_output=True,
                             env={**os.environ, "LC_ALL": "C"})
    if not os.path.lexists(public) and derived.returncode == 0:
        with tempfile.NamedTemporaryFile(dir=identity.parent, prefix=".angi-pub-") as pending:
            pending.write(derived.stdout)
            pending.flush()
            try:
                os.link(pending.name, public)
            except FileExistsError:
                pass
    if not public.is_file():
        return None
    try:
        existing = checked_public_key(public)
    except (ValueError, subprocess.CalledProcessError):
        return None
    # An encrypted key can be reused with its existing public file. SSH
    # can unlock the identity through the user's usual agent or terminal.
    if derived.returncode == 0:
        return public if existing.split()[:2] == derived.stdout.split()[:2] else None
    if b"incorrect passphrase supplied to decrypt private key" in derived.stderr:
        return public
    return None


def workstation_public_key():
    directory = Path.home() / ".ssh"
    directory.mkdir(mode=0o700, exist_ok=True)
    for name in ("id_ed25519", "angi_ed25519"):
        public = available_public_key(directory / name)
        if public is not None:
            return public
    raise ValueError("Could not select a workstation SSH key pair. Supply the public key for an existing identity.")


def personalize_rootfs(work, public_key, dropbearkey):
    login_key = work / "login.pub"
    login_key.write_bytes(public_key)
    host_key = work / "device.key"
    generated = subprocess.check_output(
        [dropbearkey, "-t", "ed25519", "-f", str(host_key)], cwd=work,
    )
    key_line = next((line for line in generated.splitlines() if line.startswith(b"ssh-ed25519 ")), None)
    if key_line is None:
        raise ValueError("Dropbear did not report the generated public host key")
    host_public_key = b" ".join(key_line.split()[:2]) + b"\n"
    policy = work / "security.conf"
    policy.write_bytes(PROVISIONED_POLICY)
    commands = [f"rm {LOGIN_KEY}", "mkdir /etc/agx", f"rm {SECURITY_POLICY}"]
    for source, destination, mode in ((login_key, LOGIN_KEY, "0100600"), (host_key, HOST_KEY, "0100600"),
                                      (policy, SECURITY_POLICY, "0100644")):
        commands.extend([
            f"write {source.name} {destination}",
            f"set_inode_field {destination} uid 0",
            f"set_inode_field {destination} gid 0",
            f"set_inode_field {destination} mode {mode}",
        ])
    rootfs = work / "rootfs"
    subprocess.run(["debugfs", "-w", "-f", "/dev/stdin", str(rootfs)], cwd=work,
                   input="\n".join(commands) + "\n", text=True, capture_output=True, check=True)

    # debugfs may print file-level errors without returning a failing status.
    for source, destination, mode in ((login_key, LOGIN_KEY, "0600"), (host_key, HOST_KEY, "0600"),
                                      (policy, SECURITY_POLICY, "0644")):
        content = subprocess.check_output(["debugfs", "-R", f"cat {destination}", str(rootfs)],
                                          stderr=subprocess.DEVNULL)
        metadata = subprocess.check_output(["debugfs", "-R", f"stat {destination}", str(rootfs)],
                                           stderr=subprocess.DEVNULL, text=True)
        if (content != source.read_bytes() or "Type: regular" not in metadata
                or not re.search(rf"\bMode:\s+{mode}\b", metadata)
                or not re.search(r"User:\s+0\s+Group:\s+0\b", metadata)):
            raise ValueError(f"Could not verify the installed SSH key at {destination}")
    return host_public_key


if __name__ == "__main__":
    try:
        print(workstation_public_key())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"Could not prepare the workstation SSH key: {error}")
