"""angi-update: the fast, guarded way to change the ANGI desktop on this device.

    angi-update inventory   services and local update availability (JSON)
    angi-update check-releases  refresh available upstream versions (JSON)
    angi-update check       also verify the staged kernel/base image (JSON)
    angi-update status      installed release, its source, and whether an install is allowed
    angi-update gui         build the desktop from the checkout and install it as a new release
    angi-update gui --progress  stream installation stages and a verified result (JSON)
    angi-update apps        installed apps (JSON); `app install|remove ID [--progress]` changes them
    angi-update refresh     load the installed release into the running desktop (closes ANGI's terminals)
    angi-update rollback    point the desktop back at the previous release
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

import refresh
import releases


TARGET = Path(f"/var/lib/agx-data/agx-opencode-users/{os.getuid()}/cargo/target/release")
# What a damaged incremental cache looks like: the build is cut off partway (a tool timeout, a reboot), the
# next one links against object files that were never written, and the linker misses the crate's own symbols.
DAMAGED_CACHE = ("undefined reference", "hidden symbol", "internal compiler error", "failed to load dep-graph", "incremental compilation")


def compile_gui():
    """Run the desktop build, streaming its output as it arrives. Returns (exit status, all output)."""
    process = subprocess.Popen(["angi-build", "gui"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace")
    lines = []
    for line in process.stdout:
        print(line, end="", flush=True)
        lines.append(line)
    return process.wait(), "".join(lines)


def build(compile=compile_gui, cache=None):
    """Release build of the desktop; cargo is incremental, so a small change rebuilds in seconds to minutes.

    A build that fails the way a damaged incremental cache does is retried once from a clean cache. Ordinary
    compile errors are not retried: they are for the agent to fix.
    """
    cache = Path(cache or TARGET / "incremental")
    status, output = compile()
    if status and any(sign in output for sign in DAMAGED_CACHE):
        print("angi-update: the incremental build cache looks damaged (an earlier build was probably cut off). "
              "Discarding it and building once more; this one takes about six minutes.", flush=True)
        shutil.rmtree(cache, ignore_errors=True)
        status, output = compile()
    if status:
        raise releases.Refused("The desktop did not build. Nothing was installed; fix the errors above and run again.")
    return TARGET / "agx-gui"


def main(args):
    if args in (["inventory"], ["check"], ["check-releases"]):
        import system_updates
        print(json.dumps(system_updates.inventory(verify=args == ["check"], refresh=args == ["check-releases"])))
    elif args[:1] in (["apps"], ["app"]):
        import app_install
        return app_install.main(args)
    elif args == ["job"]:
        import install_progress
        print(json.dumps(install_progress.status()))
    elif args == ["status"]:
        print(json.dumps({**releases.status(), "running": sorted(set(refresh.desktops().values())), "last_refresh": refresh.report()}, indent=2))
    elif args == ["gui", "--progress"]:
        import install_progress
        return install_progress.run(build)
    elif args == ["gui"]:
        with releases.update_lock():
            print(releases.install(build))
    elif args == ["refresh"]:
        print(refresh.start())
    elif len(args) == 4 and args[0] == "watch":
        # Started by `refresh`, detached: PID RELEASE PREVIOUS.
        refresh.watch(int(args[1]), args[2], args[3])
    elif args == ["rollback"]:
        with releases.update_lock():
            print(releases.rollback())
    elif len(args) == 5 and args[0] == "deploy-gui":
        if os.geteuid() != 0:
            raise releases.Refused("Workstation desktop deployment requires an administrator SSH session.")
        import gui_deploy
        print(gui_deploy.install(*args[1:]))
    elif len(args) == 7 and args[0] == "adopt":
        # Used by `make deploy-gui` on the workstation: RELEASE ORIGIN TREE PARENT ARCHIVE OWNER.
        with releases.update_lock():
            releases.adopt(*args[1:])
        print("recorded " + args[1])
    else:
        print(__doc__.strip())
        return 2
    return 0


if __name__ == "__main__":
    os.environ.pop("LD_LIBRARY_PATH", None)
    os.environ.pop("PYTHONHOME", None)
    try:
        sys.exit(main(sys.argv[1:]))
    except releases.Refused as error:
        print(f"angi-update: {error}", file=sys.stderr)
        sys.exit(3)
    except (RuntimeError, OSError, subprocess.CalledProcessError) as error:
        print(f"angi-update: {error}", file=sys.stderr)
        sys.exit(1)
