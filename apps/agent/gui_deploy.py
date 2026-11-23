"""Install a prebuilt desktop and its source from the trusted workstation."""
import hashlib
from pathlib import Path, PurePosixPath
import posixpath
import re
import subprocess
import tarfile
import tempfile

import releases


def extract_source(archive, destination):
    with tarfile.open(archive) as source:
        members = source.getmembers()
        links = {PurePosixPath(member.name) for member in members if member.issym()}
        for member in members:
            path = PurePosixPath(member.name)
            if (not path.parts or path.is_absolute() or ".." in path.parts or ".git" in path.parts
                    or links.intersection(path.parents)):
                raise releases.Refused("Invalid path in the desktop source archive.")
            if member.issym():
                target = PurePosixPath(posixpath.normpath(path.parent / member.linkname))
                if target.is_absolute() or ".." in target.parts or ".git" in target.parts:
                    raise releases.Refused("Invalid symbolic link in the desktop source archive.")
            elif not (member.isfile() or member.isdir()):
                raise releases.Refused("Unsupported file in the desktop source archive.")
        source.extractall(destination, members)


def install(stage, digest, tree, expected):
    if not re.fullmatch(r"[0-9a-f]{64}", digest) or not re.fullmatch(r"[0-9a-f]{40}", tree):
        raise releases.Refused("Invalid desktop or source digest.")
    stage = Path(stage)
    binary, archive = stage / "agx-gui", stage / "source.tar"
    with binary.open("rb") as payload:
        verified = hashlib.file_digest(payload, "sha256").hexdigest()
    if verified != digest:
        raise releases.Refused("The transferred desktop did not match its digest.")
    result = subprocess.run(["ldd", str(binary)], capture_output=True, text=True, check=False)
    if result.returncode or "not found" in result.stdout + result.stderr:
        raise releases.Refused(f"The desktop cannot load on this system. {result.stdout}{result.stderr}")
    with tempfile.TemporaryDirectory(dir=stage) as scratch:
        extract_source(archive, scratch)
        releases.git("init", "-q", source=scratch)
        if releases.tree_hash(scratch) != tree:
            raise releases.Refused("The transferred source did not match its Git tree.")

    release = "agx-gui." + digest[:16]
    with releases.update_lock():
        current = releases.installed()
        if current != expected:
            raise releases.Refused("The installed desktop changed during deployment. Review it before retrying.")
        known = releases.entry(current)
        if known is None or releases.base() != current:
            raise releases.Refused("The desktop needs a recorded source baseline. Use the full workstation deployment to establish it.")
        checkout = releases.tree_hash()
        if checkout not in (known["tree"], tree):
            raise releases.Refused("The device has source edits absent from this deployment. Integrate them before retrying.")
        if release == current and known["tree"] == tree and checkout == tree:
            if releases.release_name(releases.LINK) != release:
                raise releases.Refused("The installed desktop did not match its digest.")
            return f"Already installed: {release}."
        parent = known.get("parent") if release == current else current
        releases.place(binary, releases.GUI_DIR / release, "755")
        if releases.release_name(releases.GUI_DIR / release) != release:
            raise releases.Refused("Installed desktop verification failed.")
        if checkout == tree:
            releases.record(release, "workstation", tree, parent)
            owner = releases.SOURCE.stat()
            subprocess.run(["chown", "-R", f"{owner.st_uid}:{owner.st_gid}", str(releases.SOURCE / ".git")], check=True)
        else:
            releases.adopt(release, "workstation", tree, parent, archive, "auto")
        releases.point_link_at(release)
        if releases.installed() != release:
            releases.marker().write_text(current + "\n")
            raise releases.Refused("The installed desktop link could not be verified.")
    return f"Installed {release}. Use angi-update refresh to load it."
