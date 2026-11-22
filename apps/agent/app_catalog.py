"""The curated catalogue of command-line tools and servers that ANGI can install (apps.json).

The same file is embedded in the desktop for search; this module is the installer's reader. Every entry
names an ARM64 GitHub artifact or a native Buildroot package and the commands it exposes.
"""
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re

LOCATIONS = (Path('/usr/share/agx/agent/apps.json'), Path(__file__).with_name('apps.json'))
ID = re.compile(r'[a-z0-9][a-z0-9-]{0,40}')
LINK = re.compile(r'[A-Za-z0-9][A-Za-z0-9._-]{0,40}')
REPO = re.compile(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+')


class CatalogueError(ValueError):
    pass


@dataclass(frozen=True)
class Program:
    name: str            # the command it is run as
    path: str            # where it is inside the unpacked asset; '' when the download itself is the program


@dataclass(frozen=True)
class App:
    id: str
    name: str
    summary: str
    kind: str
    repo: str
    asset: re.Pattern | None
    programs: tuple
    launch: str          # program to start directly in a terminal after install, or ''
    native: bool = False


def parse(item):
    try:
        app_id, repo = item['id'], item.get('repo', '')
        native = item.get('native', False)
        if type(native) is not bool or 'debian' in item:
            raise CatalogueError(f'{app_id}: unsupported app runtime')
        if not ID.fullmatch(app_id) or (not native and not REPO.fullmatch(repo)) or (native and repo):
            raise CatalogueError(f'Invalid id or repository for {app_id!r}')
        programs = []
        for entry in item['bins']:
            name, path = entry['name'], entry.get('path', '')
            if not LINK.fullmatch(name):
                raise CatalogueError(f'{app_id}: invalid command name {name!r}')
            if path.startswith('/') or '..' in path.split('/'):
                raise CatalogueError(f'{app_id}: invalid program path {path!r}')
            programs.append(Program(name, path))
        if not programs or len({p.name for p in programs}) != len(programs):
            raise CatalogueError(f'{app_id}: needs at least one uniquely named program')
        launch = item.get('launch', '')
        if launch and launch not in {p.name for p in programs}:
            raise CatalogueError(f'{app_id}: launch names an unknown program')
        return App(app_id, item['name'], item['summary'], item['kind'], repo,
                   None if native else re.compile(item['asset']), tuple(programs), launch, native)
    except (KeyError, TypeError, re.error) as error:
        raise CatalogueError(f'Invalid catalogue entry: {error!r}') from error


def load(path=None):
    """{id: App} in catalogue order."""
    candidates = [Path(path)] if path else [Path(p) for p in filter(None, [os.environ.get('ANGI_APPS_CATALOGUE'), *LOCATIONS])]
    for candidate in candidates:
        try:
            raw = json.loads(candidate.read_text())
            break
        except OSError:
            continue
    else:
        raise CatalogueError('The app catalogue is missing')
    if not isinstance(raw, list):
        raise CatalogueError('The app catalogue must be a list')
    apps = {}
    for item in raw:
        app = parse(item)
        if app.id in apps:
            raise CatalogueError(f'{app.id} is listed twice')
        apps[app.id] = app
    return apps
