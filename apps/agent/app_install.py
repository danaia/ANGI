"""Install, update and remove catalogue apps, reporting real progress.

    angi-update app install ID [--progress]    install (or update) an app
    angi-update app remove ID [--progress]     remove it; its settings and data are left alone
    angi-update apps [--check]                 what is installed, as JSON; --check refreshes stale update checks

With --progress each line on stdout is one JSON event the desktop reads; the final one has state
"succeeded" or "failed". Without it the same stages print as plain text.
"""
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time

import app_catalog
import app_fetch
from app_store import AppError, Store, version_label

STAGES = ('resolving', 'downloading', 'verifying', 'extracting', 'installing')
# Overall fraction at which each stage starts; downloading spans the long middle.
START = {'resolving': 0.0, 'downloading': 0.06, 'verifying': 0.82, 'extracting': 0.86, 'installing': 0.96}
SPACE_FACTOR = 4          # the archive, its unpacked copy, and headroom
SPACE_RESERVE = 64 * 1024 * 1024


def megabytes(count):
    return f'{count / 1024 / 1024:.1f} MB'


class Report:
    def __init__(self, app, stream=None, json_events=True):
        self.app, self.stream, self.json_events = app, stream if stream is not None else sys.stdout, json_events
        self.phase_name, self.step, self.fraction, self.last = '', 0, 0.0, 0.0
        self.tag = ''           # raw release tag; `shown` is how it reads

    @property
    def shown(self):
        return version_label(self.tag, self.app.id, self.app.name)

    def emit(self, state='running', message='', **extra):
        event = dict(format='angi-app-job-v1', component=self.app.name, app=self.app.id, state=state,
                     phase=self.phase_name, step=self.step, total=len(STAGES), message=message,
                     fraction=round(self.fraction, 4), indeterminate=False, release=self.tag, log='',
                     activation_pending=False, updated_at=time.time(), **extra)
        try:
            self.stream.write((json.dumps(event) if self.json_events else message) + '\n')
            self.stream.flush()
        except (BrokenPipeError, OSError):
            self.stream = open(os.devnull, 'w')     # a closed desktop must not abort an install in progress

    def phase(self, name, message):
        self.phase_name, self.step, self.fraction = name, STAGES.index(name), START[name]
        self.emit(message=message)

    def download(self, received, total):
        span = START['verifying'] - START['downloading']
        self.fraction = START['downloading'] + span * (received / total)
        now = time.monotonic()
        if now - self.last >= 0.15 or received == total:
            self.last = now
            self.emit(message=f'Downloading {self.app.name} {self.shown}: {megabytes(received)} of {megabytes(total)}')

    def done(self, message):
        self.fraction, self.step = 1.0, len(STAGES)
        self.emit('succeeded', message, completed_at=time.time())

    def fail(self, message):
        if self.json_events:
            self.emit('failed', message, completed_at=time.time())
        else:
            print(message, file=sys.stderr)


def healthy(store, app, entry):
    """Every program is still linked and its file exists, so 'up to date' is not just a ledger claim."""
    return all(store.owns(name, app.id) and (store.bin / name).exists() for name in entry.get('bins', {}))


def install(app, store, report, get=None, opener=None, pause=time.sleep):
    kwargs = {'get': get} if get else {}
    lock = store.lock()
    try:
        ledger = store.read()
        entry = ledger.get(app.id)
        report.phase('resolving', f'Finding the latest {app.name} release…')
        release = app_fetch.resolve(app, **kwargs)
        report.tag = release.tag
        if entry and entry['tag'] == release.tag and healthy(store, app, entry):
            report.done(f'{app.name} {report.shown} is already installed and up to date.')
            return
        if store.free_bytes() < release.size * SPACE_FACTOR + SPACE_RESERVE:
            raise AppError(f'Not enough free space on the data drive to install {app.name} '
                           f'(needs about {megabytes(release.size * SPACE_FACTOR)}).')
        for program in app.programs:       # a different program of the same name is refused before anything changes
            destination = store.bin / program.name
            if (destination.is_symlink() or destination.exists()) and not store.owns(program.name, app.id):
                raise AppError(f'A different program called {program.name} is already installed here.')
        work = Path(tempfile.mkdtemp(prefix=app.id + '-', dir=store.root / 'tmp'))
        try:
            report.phase('downloading', f'Downloading {app.name} {report.shown}…')
            digest = app_fetch.download(release, work / 'asset', report.download, opener, pause)
            report.phase('verifying', 'Checking the download against the checksum GitHub published…')
            app_fetch.verify(release, digest)
            report.phase('extracting', f'Unpacking {app.name}…')
            tree = work / 'tree'
            app_fetch.unpack(work / 'asset', release.asset, tree)
            located = {program.name: app_fetch.find_program(tree, program) for program in app.programs}
            for name, relative in located.items():
                app_fetch.check_program(tree / relative, name)
            report.phase('installing', f'Installing {app.name}…')
            target = store.pkg / app.id / release.tag
            target.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
            shutil.rmtree(target, ignore_errors=True)
            os.replace(tree, target)
            store.switch(app.id, release.tag)
            for name, relative in located.items():
                store.link(name, app.id, relative)
            ledger[app.id] = dict(name=app.name, repo=app.repo, tag=release.tag, asset=release.asset,
                                  sha256=release.sha256, size=release.size, bins=located,
                                  installed_at=time.time(), previous=entry['tag'] if entry else '')
            store.write(ledger)
            store.prune(app.id, keep=release.tag)
        finally:
            shutil.rmtree(work, ignore_errors=True)
        names = ', '.join(sorted(located))
        old = version_label(entry['tag'], app.id, app.name) if entry else ''
        import graph_capture
        graph_capture.app_change(app.id, app.name, 'Installed' if not entry else 'Repaired' if entry['tag'] == release.tag else 'Updated', report.shown)
        report.done(f'{app.name} {report.shown} is installed. Run {names} in Terminal.' if not entry
                    else f'{app.name} {report.shown} was repaired.' if entry['tag'] == release.tag
                    else f'{app.name} updated from {old} to {report.shown}.')
    finally:
        lock.close()


def remove(app, store, report):
    lock = store.lock()
    try:
        ledger = store.read()
        entry = ledger.get(app.id)
        if not entry:
            raise AppError(f'{app.name} is not installed.')
        report.tag = entry['tag']
        for name in entry['bins']:
            store.unlink(name, app.id)
        shutil.rmtree(store.pkg / app.id, ignore_errors=True)
        del ledger[app.id]
        store.write(ledger)
        report.fraction = 1.0
        import graph_capture
        graph_capture.app_change(app.id, app.name, 'Removed')
        report.done(f'{app.name} was removed. Its settings and data were not touched.')
    finally:
        lock.close()


def listing(store=None, check=False):
    """What is installed, for the desktop. Local files only, unless `check` also refreshes stale update checks."""
    import app_updates
    store = store or Store()
    if check:
        import system_updates
        try:
            app_updates.rows(system_updates.updated_ago, store=store)
        except (OSError, ValueError, RuntimeError):
            pass        # an offline check must never hide what is installed
    latest = app_updates.latest_cached()
    apps = {}
    for key, item in store.read().items():
        name = item.get('name', key)
        newer = latest.get(key, '')
        apps[key] = {'tag': item['tag'], 'version': version_label(item['tag'], key, name),
                     'installed_at': item.get('installed_at', 0), 'bins': sorted(item['bins']),
                     'latest': newer, 'update': version_label(newer, key, name) if newer and newer != item['tag'] else ''}
    return {'format': 'angi-apps-list-v1', 'bin': str(store.bin), 'apps': apps}


def main(args, store=None, stream=None):
    """`app install|remove ID [--progress]` and `apps`; returns the process exit status."""
    if args in (['apps'], ['apps', '--check']):
        print(json.dumps(listing(store, check=len(args) == 2)))
        return 0
    progress = '--progress' in args
    args = [arg for arg in args if arg != '--progress']
    if len(args) != 3 or args[0] != 'app' or args[1] not in {'install', 'update', 'remove'}:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    try:
        catalogue = app_catalog.load()
    except app_catalog.CatalogueError as error:
        print(f'angi-update: {error}', file=sys.stderr)
        return 1
    app = catalogue.get(args[2])
    if app is None:
        print(f'angi-update: unknown app {args[2]!r}', file=sys.stderr)
        return 2
    store = store or Store()
    report = Report(app, stream, json_events=progress)
    try:
        (remove if args[1] == 'remove' else install)(app, store, report)
        return 0
    except AppError as error:
        report.fail(str(error))
        return 3
    except OSError as error:
        report.fail(f'{app.name} could not be installed: {error.strerror or error}')
        return 1
