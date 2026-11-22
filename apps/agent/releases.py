"""Which desktop (agx-gui) release is installed, where its source came from, and the guard that stops
an older checkout from replacing a newer release.

Every installed release is recorded in a ledger together with a hash of the exact source tree it was
built from. The checkout that agents edit carries a marker naming the release it is based on. An
install is allowed only while that marker names the release that is installed now. A release put in
place by anything else (the workstation, another checkout) therefore blocks installs from a checkout
that does not contain its source, instead of silently rolling the desktop back to older code.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from scope import OS_SOURCE

GUI_DIR = Path("/var/lib/agx-data/agx-gui")
LINK = Path("/usr/bin/agx-gui")
LEDGER = GUI_DIR / "ledger.json"
SOURCE = OS_SOURCE
# Privileged commands go through the administrator helper, unless already root (a workstation deploy) or in a test sandbox.
ROOT = [] if os.getuid() == 0 else ["angi-root"]
COPY = 'cp "$1" "$2.new" && chmod "$3" "$2.new" && mv -f "$2.new" "$2"'
SWAP = 'ln -sf "$1" "$2.new" && mv -f "$2.new" "$2"'


class Refused(RuntimeError):
    """An install that must not happen; the message says why and what to do instead."""


def git(*arguments, source=None, env=None, input=None):
    source = Path(source or SOURCE)
    result = subprocess.run(["git", "-c", f"safe.directory={source}", *arguments], cwd=source, capture_output=True, text=True, input=input, env={**os.environ, **(env or {})})
    if result.returncode:
        raise RuntimeError(f"git {arguments[0]} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def tree_hash(source=None):
    """Git's hash of every file in the checkout that is not ignored, identical on any machine holding the same files."""
    source = Path(source or SOURCE)
    with tempfile.TemporaryDirectory() as scratch:
        env = {"GIT_INDEX_FILE": str(Path(scratch) / "index")}
        git("add", "-A", source=source, env=env)
        return git("write-tree", source=source, env=env)


def snapshot(release, tree, message, source=None):
    """Keep the exact source of a release in the checkout's history, so it can always be recovered."""
    identity = {"GIT_AUTHOR_NAME": "ANGI", "GIT_AUTHOR_EMAIL": "angi@localhost", "GIT_COMMITTER_NAME": "ANGI", "GIT_COMMITTER_EMAIL": "angi@localhost"}
    commit = git("commit-tree", tree, "-m", message, source=source, env=identity)
    git("update-ref", f"refs/angi/{release}", commit, source=source)


def read_ledger():
    if not LEDGER.exists():
        return []
    data = json.loads(LEDGER.read_text())
    if not isinstance(data, list) or any(not isinstance(item, dict) or "release" not in item for item in data):
        raise RuntimeError("The release ledger is damaged. Do not install until it is repaired.")
    return data


def entry(release):
    return next((item for item in reversed(read_ledger()) if item["release"] == release), None)


def installed():
    """Name of the release /usr/bin/agx-gui points at, e.g. agx-gui.e1e637ba9e7631d3."""
    return Path(os.path.realpath(LINK)).name


def marker(source=None):
    return Path(source or SOURCE) / ".git/angi-base"


def base(source=None):
    path = marker(source)
    return path.read_text().strip() if path.exists() else None


def privileged(*command):
    subprocess.run([*ROOT, *command], check=True)


def place(source, destination, mode="644"):
    privileged("/bin/sh", "-c", COPY, "sh", str(source), str(destination), mode)


def write_ledger(items):
    with tempfile.NamedTemporaryFile("w", dir=SOURCE / ".git", delete=False, suffix=".ledger") as temporary:
        json.dump(items, temporary, indent=1)
        temporary.write("\n")
    try:
        place(temporary.name, LEDGER)
    finally:
        os.unlink(temporary.name)


def point_link_at(release):
    privileged("/bin/sh", "-c", SWAP, "sh", str(GUI_DIR / release), str(LINK))


def guard():
    """None when installing from this checkout is safe, else the reason it is not."""
    current = installed()
    known = entry(current)
    if known is None:
        return (f"The installed desktop release {current} has no source record, so this checkout cannot be shown to contain it. "
                "Installing could revert it. Ask the user to run `make deploy-gui` from the workstation, which records the release and syncs its source here.")
    if base() != current:
        return (f"This checkout is based on release {base() or 'none'}, but release {current} is installed ({known.get('origin', 'unknown')} build). "
                f"Installing from here would revert what {current} contains. Do not work around this: "
                "ask the user to run `make deploy-gui` from the workstation, which syncs the current source into this checkout.")
    return None


def record(release, origin, tree, parent):
    items = read_ledger()
    items.append({"release": release, "origin": origin, "tree": tree, "parent": parent, "time": time.strftime("%Y-%m-%dT%H:%M:%S%z")})
    write_ledger(items)
    snapshot(release, tree, f"Desktop release {release} ({origin})")
    marker().write_text(release + "\n")
    import graph_capture
    # A root workstation deploy writes into the owner of the checkout's memory, not root's.
    graph_capture.release(release, origin, parent, SOURCE.stat().st_uid if os.getuid() == 0 else None)


def release_name(binary):
    return "agx-gui." + hashlib.sha256(Path(binary).read_bytes()).hexdigest()[:16]


def install(build, origin="pi", progress=None):
    """Build and install a guarded release; report only actual stage transitions."""
    report = progress or (lambda *_: None)
    report('checking', 'Validating the installed release and local source')
    reason = guard()
    if reason:
        raise Refused(reason)
    before = tree_hash()
    report('building', 'Building the ANGI desktop')
    binary = build()
    report('verifying', 'Checking the built desktop and source snapshot')
    if tree_hash() != before:
        raise Refused("The source changed while it was building. Run the update again once edits are finished.")
    name = release_name(binary)
    current = installed()
    if name == current:
        report('confirming', 'Confirming the existing installation')
        if release_name(GUI_DIR / name) != name or not entry(name):
            raise Refused('The existing release could not be verified.')
        return f"Already installed: {name}. The build matches the installed release."
    report('installing', 'Copying the new desktop release')
    place(binary, GUI_DIR / name, "755")
    # Read back the installed bytes before changing the active link.
    if release_name(GUI_DIR / name) != name:
        raise Refused('Installed file verification failed. The previous release is still selected.')
    report('recording', 'Recording the release and its source for rollback')
    record(name, origin, before, current)
    report('activating', 'Selecting the installed release')
    point_link_at(name)
    report('confirming', 'Verifying the installed link and release record')
    if installed() != name or (entry(name) or {}).get('tree') != before:
        point_link_at(current)
        marker().write_text(current + "\n")
        raise Refused('Installation verification failed. The previous release was restored.')
    return f"Installed {name} (previous {current}). Load it with `angi-update refresh` (last step: it closes this terminal); `angi-update rollback` restores the previous release."


def rollback():
    current = installed()
    known = entry(current)
    previous = known and known.get("parent")
    if not previous or not (GUI_DIR / previous).is_file():
        raise Refused("There is no earlier release to restore.")
    point_link_at(previous)
    marker().write_text(previous + "\n")
    return f"Restored {previous}. Choose Settings → Refresh desktop… to load it."


def status():
    current = installed()
    known = entry(current) or {}
    return {"installed": current, "origin": known.get("origin"), "installed_at": known.get("time"), "previous": known.get("parent"),
            "checkout_base": base(), "can_install": guard() is None, "reason": guard()}


def adopt(release, origin, tree, parent, archive, owner):
    """Put a release's source into the checkout (from a tar of its files) and record it, as a workstation deploy does.

    The checkout's own unsaved edits are kept in history first, so replacing them is always recoverable.
    """
    if owner == "auto":
        status = SOURCE.stat()
        owner = f"{status.st_uid}:{status.st_gid}"
    keep = tree_hash()
    snapshot("pi-edits-" + time.strftime("%Y%m%d-%H%M%S"), keep, "Checkout state before a workstation sync")
    for name in git("ls-files", "-co", "--exclude-standard", "-z").split("\0"):
        if name:
            try:
                (SOURCE / name).unlink()
            except FileNotFoundError:
                pass
    subprocess.run(["tar", "-xf", str(archive), "-C", str(SOURCE)], check=True)
    subprocess.run(["chown", "-R", owner, str(SOURCE)], check=True)
    if tree_hash() != tree:
        raise Refused("The synced source does not match the release's source tree; the release was not recorded.")
    record(release, origin, tree, parent)
    subprocess.run(["chown", "-R", owner, str(SOURCE / ".git")], check=True)
