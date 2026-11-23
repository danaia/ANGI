#!/usr/bin/env python3
"""App library: release resolution, checksum-verified download, hardened unpacking and atomic install.

Everything runs against a fake GitHub: a release document and an in-memory archive served through an
injected opener. Nothing touches the network and no downloaded program is executed.
"""
import bz2
import hashlib
import io
import json
import os
from pathlib import Path
import sys
import tarfile
import tempfile
import time
import unittest
import urllib.error
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'apps/agent'))
# Recording a release or an app in ANGI's knowledge graph must not touch this account's real one.
os.environ['XDG_STATE_HOME'] = tempfile.mkdtemp(prefix='angi-test-state-')
import app_catalog
import app_fetch
import app_install
import app_updates
import update_checks
import update_sources
from app_store import AppError, Store

ELF = b'\x7fELF\x02\x01\x01' + b'\0' * 11 + (183).to_bytes(2, 'little') + b'\0' * 40      # 64-bit LE AArch64
X86 = b'\x7fELF\x02\x01\x01' + b'\0' * 11 + (62).to_bytes(2, 'little') + b'\0' * 40


def tarball(files, links=()):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode='w:gz') as tar:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size, info.mode = len(data), 0o4755       # setuid on purpose: it must not survive
            tar.addfile(info, io.BytesIO(data))
        for name, target in links:
            info = tarfile.TarInfo(name)
            info.type, info.linkname = tarfile.SYMTYPE, target
            tar.addfile(info)
    return buffer.getvalue()


def zipped(files):
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, 'w') as bundle:
        for name, data in files.items():
            info = zipfile.ZipInfo(name)
            info.external_attr = 0o755 << 16
            bundle.writestr(info, data)
    return buffer.getvalue()


def app_entry(**changes):
    item = dict(id='demo', name='Demo', summary='A demo', kind='Terminal', repo='acme/demo',
                asset=r'^demo-.*-linux-arm64\.tar\.gz$', bins=[dict(name='demo', path='demo-*/demo')])
    item.update(changes)
    return app_catalog.parse(item)


class Fake:
    """A fake GitHub: serves release metadata and asset bytes, and records what was requested."""
    def __init__(self, tag='v1.0', payload=None, name=None, size_lie=0, digest=None, host='github.com'):
        self.tag = tag
        self.name = name or f'demo-{tag}-linux-arm64.tar.gz'
        self.payload = payload if payload is not None else tarball({f'demo-{tag}/demo': ELF})
        self.size_lie, self.digest, self.host = size_lie, digest, host
        self.requests = []

    def release(self, repo='acme/demo'):
        digest = self.digest or 'sha256:' + hashlib.sha256(self.payload).hexdigest()
        return {'tag_name': self.tag, 'draft': False, 'prerelease': False, 'assets': [
            {'name': self.name, 'size': len(self.payload) + self.size_lie, 'digest': digest,
             'browser_download_url': f'https://{self.host}/{repo}/releases/download/{self.tag}/{self.name}'},
            {'name': 'demo-source.zip', 'size': 1, 'digest': 'sha256:' + '0' * 64,
             'browser_download_url': f'https://github.com/{repo}/archive/{self.tag}.zip'}]}

    def get(self, url):
        self.requests.append(url)
        return self.release()

    def opener(self):
        fake = self

        class Response(io.BytesIO):
            url = 'https://objects.githubusercontent.com/blob'
            def __enter__(self): return self
            def __exit__(self, *args): return False

        class Opener:
            def open(self, request, timeout=None):
                fake.requests.append(request.full_url)
                return Response(fake.payload)
        return Opener()


class Case(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.store = Store(Path(self.directory.name) / 'apps')
        self.app = app_entry()

    def install(self, fake, app=None):
        out = io.StringIO()
        report = app_install.Report(app or self.app, out)
        app_install.install(app or self.app, self.store, report, get=fake.get, opener=fake.opener(), pause=lambda s: None)
        return [json.loads(line) for line in out.getvalue().splitlines()]


class Labels(unittest.TestCase):
    def test_tags_read_as_versions_without_losing_unusual_ones(self):
        from app_store import version_label
        for tag, app_id, name, shown in (('jq-1.8.2', 'jq', 'jq', '1.8.2'), ('v15.2.0', 'ripgrep', 'ripgrep', '15.2.0'),
                                         ('15.2.0', 'ripgrep', 'ripgrep', '15.2.0'), ('r42', 'lf', 'lf', 'r42'),
                                         ('2026-11-16', 'x', 'X', '2026-11-16'), ('v2.105.0', 'github-cli', 'GitHub CLI', '2.105.0')):
            self.assertEqual(version_label(tag, app_id, name), shown, tag)


class Catalogue(unittest.TestCase):
    def test_shipped_catalogue_is_valid_complete_and_consistent_with_the_desktop_list(self):
        apps = app_catalog.load()
        self.assertGreaterEqual(len(apps), 45)
        for app in apps.values():
            self.assertTrue(app.programs and (app.repo or app.native) and app.summary
                            and app.kind in {'Desktop', 'Terminal', 'Server'}, app.id)
        self.assertEqual(list(apps)[:4], ['blender', 'vscode', 'vlc', 'gstreamer'])
        self.assertTrue(all(apps[app_id].native for app_id in list(apps)[:4]))
        names = {name for app in apps.values() for name in (p.name for p in app.programs)}
        self.assertTrue({'rg', 'nvim', 'gh', 'btm', 'syncthing'} <= names)

    def test_unsafe_entries_are_rejected(self):
        for change in (dict(id='../x'), dict(repo='no-slash'), dict(bins=[dict(name='a/b')]),
                       dict(bins=[dict(name='ok', path='../escape')]), dict(bins=[dict(name='ok', path='/abs')]),
                       dict(bins=[]), dict(launch='missing'), dict(asset='(')):
            with self.assertRaises(app_catalog.CatalogueError, msg=change):
                app_entry(**change)


class Resolve(Case):
    def test_picks_the_single_matching_asset_and_requires_a_published_checksum(self):
        fake = Fake()
        release = app_fetch.resolve(self.app, fake.get)
        self.assertEqual((release.tag, release.asset), ('v1.0', 'demo-v1.0-linux-arm64.tar.gz'))
        self.assertEqual(len(release.sha256), 64)
        for broken, text in ((dict(digest='sha256:short'), 'no checksum'), (dict(name='other.tar.gz'), 'no download')):
            fake = Fake(**broken)
            with self.assertRaisesRegex(AppError, text):
                app_fetch.resolve(self.app, fake.get)

    def test_prereleases_unsafe_hosts_and_ambiguity_are_refused(self):
        fake = Fake()
        release = fake.release(); release['prerelease'] = True
        with self.assertRaisesRegex(AppError, 'no stable release'):
            app_fetch.resolve(self.app, lambda url: release)
        with self.assertRaisesRegex(AppError, 'unexpected'):
            app_fetch.resolve(self.app, Fake(host='evil.example').get)
        release = fake.release(); release['assets'].append(dict(release['assets'][0], name='demo-x-linux-arm64.tar.gz'))
        with self.assertRaisesRegex(AppError, 'several'):
            app_fetch.resolve(self.app, lambda url: release)

    def test_network_problems_become_plain_language(self):
        def offline(url): raise update_sources.TemporarilyUnavailable('x', 'network')
        with self.assertRaisesRegex(AppError, 'Cannot reach GitHub'):
            app_fetch.resolve(self.app, offline)
        def limited(url): raise update_sources.TemporarilyUnavailable('x', 'rate-limit')
        with self.assertRaisesRegex(AppError, 'limiting requests'):
            app_fetch.resolve(self.app, limited)


class Download(Case):
    def test_a_download_that_does_not_match_its_checksum_is_discarded(self):
        fake = Fake(digest='sha256:' + 'a' * 64)
        with self.assertRaisesRegex(AppError, 'did not match its published checksum'):
            self.install(fake)
        self.assertEqual(self.store.read(), {})
        self.assertFalse((self.store.bin / 'demo').exists())
        self.assertEqual(list((self.store.root / 'tmp').iterdir()), [], 'the partial download is removed')

    def test_a_download_larger_than_announced_is_stopped(self):
        fake = Fake(size_lie=-5)
        with self.assertRaisesRegex(AppError, 'larger than'):
            self.install(fake)

    def test_redirects_to_other_hosts_are_refused(self):
        with self.assertRaises(AppError):
            app_fetch.check_url('https://evil.example/x')
        with self.assertRaises(AppError):
            app_fetch.check_url('http://github.com/x')

    def test_transient_failures_retry_then_report_the_network(self):
        calls = []
        class Flaky:
            def open(self, request, timeout=None):
                calls.append(1)
                raise urllib.error.URLError('down')
        fake = Fake()
        release = app_fetch.resolve(self.app, fake.get)
        with self.assertRaisesRegex(AppError, 'Cannot reach GitHub'):
            app_fetch.download(release, Path(self.directory.name) / 'x', lambda a, b: None, Flaky(), lambda s: None)
        self.assertEqual(len(calls), app_fetch.ATTEMPTS)


class Unpack(Case):
    def unpack(self, data, name='x.tar.gz'):
        archive = Path(self.directory.name) / 'a'
        archive.write_bytes(data)
        app_fetch.unpack(archive, name, Path(self.directory.name) / 'out')

    def test_path_traversal_absolute_paths_and_escaping_links_are_refused(self):
        for data in (tarball({'../evil': b'x'}), tarball({'/etc/evil': b'x'}),
                     tarball({'ok': b'x'}, links=[('l', '../../escape')]), tarball({'ok': b'x'}, links=[('l', '/etc/passwd')])):
            with self.assertRaises(AppError):
                self.unpack(data)
        with self.assertRaises(AppError):
            self.unpack(zipped({'../evil': b'x'}), 'x.zip')
        self.assertFalse((Path(self.directory.name) / 'evil').exists())

    def test_a_dot_slash_root_entry_is_harmless(self):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode='w:gz') as tar:      # how GitHub Actions archives such as jj's start
            root = tarfile.TarInfo('./'); root.type = tarfile.DIRTYPE
            tar.addfile(root)
            info = tarfile.TarInfo('./jj'); info.size, info.mode = len(ELF), 0o755
            tar.addfile(info, io.BytesIO(ELF))
        self.unpack(buffer.getvalue())
        self.assertEqual((Path(self.directory.name) / 'out/jj').read_bytes(), ELF)

    def test_writing_through_a_link_that_leaves_the_folder_is_refused(self):
        data = tarball({'dir/f': b'x'}, links=[('dir2', '.')])
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode='w:gz') as tar:
            link = tarfile.TarInfo('dir'); link.type, link.linkname = tarfile.SYMTYPE, '..'
            tar.addfile(link)
            info = tarfile.TarInfo('dir/evil'); info.size = 1
            tar.addfile(info, io.BytesIO(b'x'))
        with self.assertRaises(AppError):
            self.unpack(buffer.getvalue())

    def test_setuid_bits_are_dropped_and_zip_modes_restored(self):
        self.unpack(tarball({'bin/tool': ELF}))
        mode = os.stat(Path(self.directory.name) / 'out/bin/tool').st_mode
        self.assertEqual(mode & 0o7000, 0)
        self.assertTrue(mode & 0o100)
        self.unpack(zipped({'tool': ELF}), 'y.zip')
        self.assertTrue(os.stat(Path(self.directory.name) / 'out/tool').st_mode & 0o100)

    def test_single_file_decompression_and_raw_assets(self):
        self.unpack(bz2.compress(ELF), 'restic_1_linux_arm64.bz2')
        self.assertEqual((Path(self.directory.name) / 'out/restic_1_linux_arm64').read_bytes(), ELF)
        found = app_fetch.find_program(Path(self.directory.name) / 'out', app_catalog.Program('restic', ''))
        self.assertEqual(found, 'restic_1_linux_arm64')

    def test_programs_must_be_arm64_linux_elf_files(self):
        path = Path(self.directory.name) / 'p'
        for data in (X86, b'#!/bin/sh\n', b'\x7fELF'):
            path.write_bytes(data)
            with self.assertRaises(AppError):
                app_fetch.check_program(path, 'p')
        path.write_bytes(ELF)
        app_fetch.check_program(path, 'p')
        self.assertTrue(os.stat(path).st_mode & 0o111)

    def test_an_ambiguous_or_missing_program_path_is_an_error(self):
        self.unpack(tarball({'a/tool': ELF, 'b/tool': ELF}))
        out = Path(self.directory.name) / 'out'
        with self.assertRaises(AppError):
            app_fetch.find_program(out, app_catalog.Program('tool', '*/tool'))
        with self.assertRaises(AppError):
            app_fetch.find_program(out, app_catalog.Program('tool', 'nothing'))


class Install(Case):
    def test_install_streams_ordered_stages_links_the_program_and_records_it(self):
        events = self.install(Fake())
        self.assertEqual([e['state'] for e in events[:-1]], ['running'] * (len(events) - 1))
        self.assertEqual(events[-1]['state'], 'succeeded')
        fractions = [e['fraction'] for e in events]
        self.assertEqual(fractions, sorted(fractions), 'progress never goes backwards')
        self.assertEqual(events[-1]['fraction'], 1.0)
        phases = []
        for e in events:
            if e['phase'] not in phases: phases.append(e['phase'])
        self.assertEqual(phases, list(app_install.STAGES))
        self.assertTrue(any('MB' in e['message'] for e in events), 'the download reports real byte counts')
        link = self.store.bin / 'demo'
        self.assertTrue(link.is_symlink() and link.exists())
        self.assertEqual(os.readlink(link), '../pkg/demo/current/demo-v1.0/demo')
        self.assertIn('Demo 1.0 is installed', events[-1]['message'], 'a "v" prefix is not repeated to the person')
        self.assertEqual(link.read_bytes(), ELF)
        self.assertEqual(os.stat(link).st_mode & 0o7000, 0)
        ledger = self.store.read()['demo']
        self.assertEqual((ledger['tag'], ledger['repo'], ledger['bins']), ('v1.0', 'acme/demo', {'demo': 'demo-v1.0/demo'}))
        listed = app_install.listing(self.store)['apps']['demo']
        self.assertEqual((listed['tag'], listed['version'], listed['update']), ('v1.0', '1.0', ''))

    def test_reinstalling_the_current_release_is_a_verified_no_op_unless_it_is_broken(self):
        self.install(Fake())
        fake = Fake()
        self.assertIn('already installed', self.install(fake)[-1]['message'])
        self.assertFalse(any('download' in url for url in fake.requests), 'nothing is downloaded again')
        (self.store.bin / 'demo').unlink()
        self.assertIn('was repaired', self.install(Fake())[-1]['message'], 'a damaged install is repaired')
        self.assertTrue((self.store.bin / 'demo').exists())

    def test_update_switches_atomically_and_removes_the_old_version(self):
        self.install(Fake('v1.0'))
        events = self.install(Fake('v1.1'))
        self.assertIn('updated from 1.0 to 1.1', events[-1]['message'])
        self.assertEqual(os.readlink(self.store.bin / 'demo'), '../pkg/demo/current/demo-v1.1/demo')
        self.assertEqual(sorted(p.name for p in (self.store.pkg / 'demo').iterdir()), ['current', 'v1.1'])
        self.assertEqual(self.store.read()['demo']['previous'], 'v1.0')

    def test_a_failed_update_leaves_the_working_version_untouched(self):
        self.install(Fake('v1.0'))
        with self.assertRaises(AppError):
            self.install(Fake('v1.1', digest='sha256:' + 'b' * 64))
        self.assertEqual(self.store.read()['demo']['tag'], 'v1.0')
        self.assertEqual((self.store.bin / 'demo').read_bytes(), ELF)
        with self.assertRaises(AppError):
            self.install(Fake('v1.2', payload=tarball({'demo-v1.2/demo': X86})))
        self.assertEqual(os.readlink(self.store.bin / 'demo'), '../pkg/demo/current/demo-v1.0/demo')

    def test_a_foreign_program_of_the_same_name_is_never_overwritten(self):
        self.store.prepare()
        (self.store.bin / 'demo').write_text('mine')
        with self.assertRaisesRegex(AppError, 'different program called demo'):
            self.install(Fake())
        self.assertEqual((self.store.bin / 'demo').read_text(), 'mine')

    def test_two_installs_cannot_interleave(self):
        handle = self.store.lock()
        try:
            with self.assertRaisesRegex(AppError, 'Another app'):
                self.install(Fake())
        finally:
            handle.close()

    def test_a_full_disk_is_reported_before_downloading(self):
        fake = Fake()
        self.store.free_bytes = lambda: 1024
        with self.assertRaisesRegex(AppError, 'Not enough free space'):
            self.install(fake)
        self.assertFalse(any('download' in url for url in fake.requests))

    def test_remove_deletes_only_this_apps_files_and_links(self):
        self.install(Fake())
        other = app_entry(id='other', name='Other', bins=[dict(name='other', path='demo-*/demo')])
        self.install(Fake(), other)
        out = io.StringIO()
        app_install.remove(self.app, self.store, app_install.Report(self.app, out))
        self.assertFalse((self.store.bin / 'demo').is_symlink())
        self.assertTrue((self.store.bin / 'other').exists())
        self.assertFalse((self.store.pkg / 'demo').exists())
        self.assertEqual(sorted(self.store.read()), ['other'])
        with self.assertRaisesRegex(AppError, 'not installed'):
            app_install.remove(self.app, self.store, app_install.Report(self.app, out))

    def test_zip_assets_install_with_their_runtime_files(self):
        app = app_entry(id='zipped', name='Zipped', asset=r'^z-linux-arm64\.zip$', bins=[dict(name='tool', path='*/tool')])
        fake = Fake(name='z-linux-arm64.zip', payload=zipped({'z-1/tool': ELF, 'z-1/runtime/data.txt': b'data'}))
        self.install(fake, app)
        self.assertTrue((self.store.pkg / 'zipped/current/z-1/runtime/data.txt').exists())
        self.assertEqual((self.store.bin / 'tool').read_bytes(), ELF)


class Cli(Case):
    def test_main_installs_removes_and_rejects_unknown_apps(self):
        out = io.StringIO()
        self.assertEqual(app_install.main(['app', 'install', 'nope', '--progress'], self.store, out), 2)
        self.assertEqual(app_install.main(['bogus'], self.store, out), 2)
        listing = json.loads(self._apps())
        self.assertEqual((listing['format'], listing['bin'], listing['apps']),
                         ('angi-apps-list-v1', str(self.store.bin), {}))
        self.assertIsInstance(listing['unavailable'], dict)

    def test_incompatible_app_fails_before_downloading(self):
        from unittest.mock import patch
        out = io.StringIO()
        with patch('app_platform.architecture', return_value='amd64'):
            self.assertEqual(app_install.main(['app', 'install', 'ripgrep', '--progress'], self.store, out), 3)
        event = json.loads(out.getvalue())
        self.assertEqual(event['state'], 'failed')
        self.assertIn('64-bit ARM', event['message'])
        self.assertEqual(self.store.read(), {})

    def test_native_package_rejects_the_wrong_board_and_installs_on_a_match(self):
        from unittest.mock import patch
        root = Path(self.directory.name)
        archive = tarball({'libexec/gst-inspect-1.0': ELF, 'libexec/gst-launch-1.0': ELF})
        (root / 'gstreamer.tar.gz').write_bytes(archive)
        metadata = dict(board='rpi5', arch='aarch64', buildroot='2026.08.1', version='1.24.13',
                        sha256=hashlib.sha256(archive).hexdigest())
        (root / 'gstreamer.json').write_text(json.dumps(metadata))
        args = ['app', 'install', 'gstreamer', '--progress']
        with patch('app_native.PACKAGES', root), patch('app_native.target', return_value=('rpi4', 'aarch64', '2026.08.1')):
            log = io.StringIO()
            self.assertEqual(app_install.main(args, self.store, log), 3)
            self.assertIn('different ANGI build', log.getvalue())
            self.assertEqual(self.store.read(), {})
        with patch('app_native.PACKAGES', root), patch('app_native.target', return_value=('rpi5', 'aarch64', '2026.08.1')):
            self.assertEqual(app_install.main(args, self.store, io.StringIO()), 0)
        self.assertTrue(self.store.owns('gst-launch-1.0', 'gstreamer'))
        self.assertIn('GST_PLUGIN_SCANNER', (self.store.bin / 'gst-launch-1.0').read_text())

    def test_desktop_package_is_available_and_installable_when_staged_for_the_device(self):
        import subprocess
        from unittest.mock import patch
        root = Path(self.directory.name)
        archive = tarball({'vlc': ELF})
        (root / 'vlc.tar.gz').write_bytes(archive)
        metadata = dict(board='rpi5', arch='aarch64', buildroot='2026.08.1', version='3.0.24',
                        sha256=hashlib.sha256(archive).hexdigest())
        (root / 'vlc.json').write_text(json.dumps(metadata))
        with patch('app_native.PACKAGES', root), patch('app_native.target', return_value=('rpi5', 'aarch64', '2026.08.1')):
            unavailable = json.loads(self._apps())['unavailable']
            self.assertNotIn('vlc', unavailable)
            self.assertIn('Blender Wayland package', unavailable['blender'])
            self.assertIn('VS Code desktop runtime', unavailable['vscode'])
            events = io.StringIO()
            self.assertEqual(app_install.main(['app', 'install', 'vlc', '--progress'], self.store, events), 0)
            self.assertIn('Open it from Apps in an ANGI Wayland session.', events.getvalue())
        command = (self.store.bin / 'vlc').read_text()
        self.assertIn('QT_QPA_PLATFORM=wayland', command)
        self.assertNotIn('GST_PLUGIN_SCANNER', command)
        run = subprocess.run([self.store.bin / 'vlc'], capture_output=True, text=True,
                             env={'PATH': os.environ['PATH'], 'XDG_RUNTIME_DIR': str(root)})
        self.assertEqual(run.returncode, 1)
        self.assertIn('angi-wayland --next-login', run.stderr)

    def test_vscode_installs_the_verified_microsoft_archive_and_separate_runtime(self):
        from unittest.mock import Mock, patch
        root = Path(self.directory.name)
        app = app_catalog.load()['vscode']
        publisher_url = 'https://vscode.download.prss.microsoft.com/arm64.tar.gz'
        upstream = tarball({'VSCode-linux-arm64/code': ELF,
                            'VSCode-linux-arm64/bin/code': b'#!/bin/sh\n'})
        runtime = tarball({'lib/libgtk-3.so.0': ELF})
        manifest = dict(board='rpi5', arch='aarch64', buildroot='2026.08.1', version='1.147.0',
                        sha256=hashlib.sha256(runtime).hexdigest(),
                        microsoft=dict(url=publisher_url, size=len(upstream),
                                       sha256=hashlib.sha256(upstream).hexdigest()))
        (root / 'vscode.json').write_text(json.dumps(manifest))
        (root / 'vscode.tar.gz').write_bytes(runtime)
        response = io.BytesIO(upstream)
        response.url = publisher_url
        opener = Mock()
        opener.open.return_value = response
        with patch('app_native.PACKAGES', root), patch('app_native.target', return_value=('rpi5', 'aarch64', '2026.08.1')):
            app_install.install(app, self.store, app_install.Report(app, io.StringIO()), opener=opener)
        self.assertEqual(opener.open.call_args.args[0].full_url, publisher_url)
        self.assertIn('--ozone-platform=wayland', (self.store.bin / 'code').read_text())
        installed = self.store.pkg / 'vscode' / 'current'
        self.assertEqual((installed / 'runtime/lib/libgtk-3.so.0').read_bytes(), ELF)
        self.assertTrue((installed / 'VSCode-linux-arm64/bin/code').is_file())

    def _apps(self):
        from contextlib import redirect_stdout
        buffer = io.StringIO()
        with redirect_stdout(buffer):
            self.assertEqual(app_install.main(['apps'], self.store), 0)
        return buffer.getvalue()


class Tracking(Case):
    def rows(self, latest=None, error=None):
        path = Path(self.directory.name) / 'checks.json'
        if path.exists():       # a manual check inside the 60 s freshness window reuses the last answer; age it
            data = json.loads(path.read_text())
            for entry in data.values():
                entry['checked_at'] -= 3600
                if 'successful_at' in entry: entry['successful_at'] -= 3600
            path.write_text(json.dumps(data))
        def get(url):
            if error: raise error
            return {'tag_name': latest, 'draft': False, 'prerelease': False}
        return app_updates.rows(lambda t: 'Updated just now' if t else 'Not recorded', store=self.store,
                                path=path, get=get, refresh_now=True)

    def test_installed_apps_become_service_rows_that_offer_an_update_when_a_newer_tag_exists(self):
        self.assertEqual(self.rows('v1.0'), [], 'nothing installed, nothing tracked')
        self.install(Fake('v1.0'))
        current = self.rows('v1.0')[0]
        self.assertEqual((current['key'], current['update_state'], current['action'], current['installed']), ('app:demo', 'current', '', '1.0'))
        newer = self.rows('v1.1')[0]
        self.assertEqual((newer['update_state'], newer['action'], newer['latest'], newer['installed']), ('update', 'Update', '1.1', '1.0'))
        self.assertEqual(app_install.listing(self.store)['apps']['demo']['tag'], 'v1.0')
        self.assertEqual(app_updates.latest_cached(Path(self.directory.name) / 'checks.json'), {'demo': 'v1.1'})

    def test_check_failures_never_invent_an_update_and_keep_the_last_known_answer(self):
        self.install(Fake('v1.0'))
        self.rows('v1.1')
        offline = self.rows(error=update_sources.TemporarilyUnavailable('Network unavailable', 'network'))[0]
        self.assertEqual(offline['update_state'], 'update', 'the last good answer survives a failed check')
        fresh = Store(Path(self.directory.name) / 'apps2')
        self.assertEqual(fresh.read(), {})

    def test_pre_release_and_odd_tags_are_not_offered(self):
        with self.assertRaises(ValueError):
            update_sources.latest({'endpoint': 'x', 'kind': 'tag'}, get=lambda u: {'tag_name': 'v1', 'prerelease': True})
        with self.assertRaises(ValueError):
            update_sources.latest({'endpoint': 'x', 'kind': 'tag'}, get=lambda u: {'tag_name': '../v1'})
        self.assertEqual(update_sources.latest({'endpoint': 'x', 'kind': 'tag'}, get=lambda u: {'tag_name': 'r42'}), 'r42')

    def test_services_inventory_includes_apps_with_a_sensible_restart_note(self):
        import update_restart
        row = dict(key='app:demo', name='Demo', action='Update')
        update_restart.decorate([row], {})
        self.assertEqual(row['restart_label'], 'Next launch')
        self.assertFalse(row['restart_pending'])
        self.assertIn('checksum', row['install_note'])


if __name__ == '__main__':
    unittest.main()
