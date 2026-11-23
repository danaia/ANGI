"""NDJSON install events and a durable, verified desktop installation receipt."""
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import time

import refresh
import releases
from update_checks import save

JOB = Path.home() / '.cache/angi/update-job.json'
STAGES = ('checking', 'building', 'verifying', 'installing', 'recording', 'activating', 'confirming')


def process_token(pid):
    try:
        return Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[19]
    except (OSError, IndexError):
        return None


def status(path=None):
    try:
        data = json.loads(Path(path or JOB).read_text())
    except (OSError, ValueError):
        return {}
    if not isinstance(data, dict) or data.get('format') != 'angi-update-job-v1':
        return {}
    if data.get('state') == 'running' and (not data.get('process_token') or process_token(data.get('pid')) != data.get('process_token')):
        data.update(state='failed', message='The update process stopped before a verified installation result.', indeterminate=False)
    if data.get('state') == 'succeeded' and data.get('release') != releases.installed():
        data['message'] = 'An earlier desktop installation was verified. A different release is now selected.'
        data['activation_pending'] = False
    if data.get('state') == 'succeeded' and data.get('release') == releases.installed():
        running = refresh.desktops()
        pending = bool(running) and any(name != data['release'] for name in running.values())
        data['activation_pending'] = pending
        data['message'] = ('Installation verified. Restart the desktop in Settings to load it.' if pending
                           else 'Installation verified. The selected release is ready.')
    return data


class Reporter:
    def __init__(self, path, stream):
        self.path, self.stream = Path(path), stream
        self.last_saved = 0
        self.data = dict(format='angi-update-job-v1', component='ANGI desktop', state='running',
                         pid=os.getpid(), process_token=process_token(os.getpid()),
                         started_at=time.time(), step=0, total=len(STAGES), log='', indeterminate=True)

    def output(self, event):
        if self.stream is not None:
            try:
                self.stream.write(json.dumps(event) + '\n')
                self.stream.flush()
            except (BrokenPipeError, OSError):
                # A hidden/closed desktop must not abort an in-flight install.
                self.stream = None

    def emit(self, *, persist=True, **fields):
        self.data.update(fields, updated_at=time.time())
        if persist:
            try:
                save(self.path, self.data)
            except OSError:
                self.data['status_warning'] = 'Could not save the update receipt.'
        self.output(self.data)

    def phase(self, phase, message):
        self.emit(phase=phase, message=message, step=STAGES.index(phase), indeterminate=phase == 'building')

    def log(self, line):
        self.data['log'] = (self.data['log'] + line + '\n')[-2400:]
        now = time.monotonic()
        persist = now - self.last_saved >= 1
        self.emit(persist=persist)
        if persist:
            self.last_saved = now


class BuildOutput(io.TextIOBase):
    def __init__(self, reporter):
        self.reporter, self.pending = reporter, ''

    def write(self, text):
        self.pending += text
        while '\n' in self.pending:
            line, self.pending = self.pending.split('\n', 1)
            self.reporter.log(line)
        return len(text)

    def flush(self):
        if self.pending:
            self.reporter.log(self.pending)
            self.pending = ''


def run(build, path=None, stream=None):
    path = Path(path or JOB)
    stream = stream or sys.stdout
    try:
        with releases.update_lock():
            return run_locked(build, path, stream)
    except releases.Refused as error:
        print(json.dumps({'state': 'failed', 'message': str(error)}), file=stream, flush=True)
        return 3


def run_locked(build, path, stream):
    reporter = Reporter(path, stream)
    output = BuildOutput(reporter)
    try:
        with redirect_stdout(output):
            message = releases.install(build, progress=reporter.phase)
            output.flush()
        release = releases.installed()
        # Completion is sent only after the installer read back the file,
        # active link and release ledger successfully.
        running = refresh.desktops()
        pending = bool(running) and any(name != release for name in running.values())
        reporter.emit(state='succeeded', phase='complete', step=len(STAGES), indeterminate=False,
                      message=('Desktop already current. The installed release has been verified.' if message.startswith('Already installed')
                               else 'The desktop update is installed. Its file and release record have been verified.'),
                      detail=message, release=release, activation_pending=pending, completed_at=time.time())
        return 0
    except (releases.Refused, OSError, RuntimeError, subprocess.SubprocessError) as error:
        output.flush()
        reporter.emit(state='failed', message=str(error), indeterminate=False, completed_at=time.time())
        return 3 if isinstance(error, releases.Refused) else 1
