"""Transcribe on the device with whisper.cpp; no audio leaves it."""
import os
from pathlib import Path
import re
import subprocess
import tempfile

from voice_models import find, installed, path


class LocalError(Exception):
    def __init__(self, message, code="failed"):
        super().__init__(message)
        self.code = code


def binary():
    return Path(os.environ.get("ANGI_WHISPER_CLI") or Path(__file__).with_name("whisper-cli"))


def language():
    """The account's language (en from en_US.UTF-8); a fixed language skips a second pass to detect it."""
    code = re.match(r"[a-z]{2,3}", os.environ.get("LC_ALL") or os.environ.get("LANG") or "")
    return code.group(0) if code else "en"


def context(audio):
    """Whisper reads a fixed 30 seconds; shrink the window for short recordings, but never below
    15 seconds: tighter windows made Whisper repeat the sentence (measured on the Raspberry Pi 5)."""
    seconds = max(0, len(audio) - 44) / 32000
    return str(min(1500, max(750, int(seconds * 50) + 200)))


def collapse_repeats(text):
    """One copy of a transcript that is the same words said over again."""
    return re.sub(r"^(.{3,}?)(?:\s+\1)+$", r"\1", text, flags=re.DOTALL | re.IGNORECASE)


def transcribe(audio, model_id):
    model = find(model_id)
    program = binary()
    if not program.is_file():
        raise LocalError("Local speech recognition is not installed on this device.")
    if not installed(model):
        raise LocalError("That speech model is not downloaded yet.", "choose-engine")
    threads = str(max(1, min(os.cpu_count() or 1, 4)))
    # whisper-cli reads a file, not standard input: a private temporary copy, removed straight after.
    with tempfile.NamedTemporaryFile(prefix="voice.", suffix=".wav") as recording:
        recording.write(audio)
        recording.flush()
        command = [str(program), "-m", str(path(model)), "-f", recording.name, "-nt", "-np", "-l", language(), "-ac", context(audio), "-t", threads]
        try:
            result = subprocess.run(command, stdin=subprocess.DEVNULL, capture_output=True, timeout=300)
        except (OSError, subprocess.TimeoutExpired):
            raise LocalError("Speech recognition did not finish. Try a shorter recording.") from None
    if result.returncode != 0:
        raise LocalError("Speech recognition failed on this recording.")
    text = result.stdout.decode("utf8", "replace")
    return collapse_repeats(" ".join(re.sub(r"\[BLANK_AUDIO\]", "", text).split()))
