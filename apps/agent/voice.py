"""Speech to text for the desktop's microphone button.

The desktop records and sends one WAV on stdin; this module owns the choice of engine and every
credential. Two engines: OpenAI Whisper in the cloud (needs an OpenAI API key; a ChatGPT sign-in
lacks the audio permission), or a Whisper model on the device (`voice_local`, `voice_models`).
The first transcription has no engine, so it reports `choose-engine` and the desktop offers both.
"""
import json
import os
from pathlib import Path
import secrets
import urllib.error
import urllib.request

import voice_local
import voice_models
from models import atomic_write

ENDPOINT = "https://api.openai.com/v1/audio/transcriptions"
MODEL = "whisper-1"
MAX_AUDIO = 24 * 1024 * 1024  # the cloud service accepts 25 MB


class VoiceError(Exception):
    def __init__(self, message, code="failed"):
        super().__init__(message)
        self.code = code


def settings_path():
    return Path(os.environ["XDG_CONFIG_HOME"]) / "opencode/angi-voice.json"


def settings():
    try:
        value = json.loads(settings_path().read_text())
    except (OSError, ValueError):
        return {}
    return value if isinstance(value, dict) else {}


def write_settings(values):
    atomic_write(settings_path(), values)
    os.chmod(settings_path(), 0o600)


def api_key():
    """A voice API key, or an OpenAI API key OpenCode already holds; never a ChatGPT sign-in."""
    if key := settings().get("key"):
        return key
    try:
        entry = json.loads((Path(os.environ["XDG_DATA_HOME"]) / "opencode/auth.json").read_text()).get("openai") or {}
    except (OSError, ValueError, AttributeError):
        return ""
    return entry.get("key", "") if entry.get("type") == "api" else ""


def save_key(key):
    key = key.strip()
    if not key.startswith("sk-") or len(key) < 20 or not key.isprintable() or " " in key:
        raise VoiceError("That does not look like an OpenAI API key. It starts with sk-.", "bad-key")
    write_settings({**settings(), "key": key})


def engine():
    """('openai', None) or ('local', model id); the saved choice, or a saved key's implied one."""
    current = settings()
    if current.get("engine") == "local" and current.get("model"):
        return "local", current["model"]
    if current.get("engine") == "openai" or (not current.get("engine") and api_key()):
        return "openai", None
    raise VoiceError("Choose how ANGI should turn your speech into text.", "choose-engine")


def choose(kind, model=None):
    if kind == "openai":
        if not api_key():
            raise VoiceError("Enter an OpenAI API key first.", "needs-key")
        write_settings({**settings(), "engine": "openai"})
    elif kind == "local":
        if not voice_models.installed(voice_models.find(model)):
            raise VoiceError("Download that model first.", "choose-engine")
        write_settings({**settings(), "engine": "local", "model": model})
    else:
        raise ValueError("Unknown speech engine.")
    return {"engine": kind}


def options():
    """What the setup overlay shows: this device, the practical models, and the cloud choice."""
    device = voice_models.hardware()
    try:
        kind, model = engine()
    except VoiceError:
        kind, model = "", ""
    models = [{"id": m["id"], "name": m["name"], "size": m["size"], "seconds": m["estimate"], "quality": m["quality"],
               "installed": voice_models.installed(m), "recommended": m["recommended"], "slow": m["slow"]}
              for m in voice_models.practical(device)]
    if kind == "local":
        ready = voice_local.binary().is_file() and voice_models.installed(voice_models.find(model))
    else:
        ready = kind == "openai" and bool(api_key())
    return {"device": device, "models": models, "key_saved": bool(api_key()), "ready": ready,
            "local_ready": voice_local.binary().is_file(),
            "engine": kind, "model": model or ""}


def multipart(audio):
    boundary = "angi" + secrets.token_hex(12)
    head = "".join(f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n'
                   for name, value in (("model", MODEL), ("response_format", "json")))
    head += f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="voice.wav"\r\nContent-Type: audio/wav\r\n\r\n'
    return boundary, head.encode() + audio + f"\r\n--{boundary}--\r\n".encode()


def transcribe_cloud(audio):
    key = api_key()
    if not key:
        raise VoiceError("Enter an OpenAI API key to use cloud transcription.", "needs-key")
    boundary, body = multipart(audio)
    request = urllib.request.Request(ENDPOINT, data=body, headers={
        "Authorization": "Bearer " + key, "Content-Type": "multipart/form-data; boundary=" + boundary})
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            return json.load(response).get("text", "").strip()
    except urllib.error.HTTPError as error:
        if error.code in (401, 403):
            raise VoiceError("OpenAI did not accept the API key. Enter a new one.", "needs-key") from None
        if error.code == 429:
            raise VoiceError("OpenAI is limiting voice requests right now. Try again in a moment.") from None
        raise VoiceError("OpenAI could not transcribe that recording.") from None
    except (OSError, ValueError):
        raise VoiceError("Could not reach OpenAI. Check the network and try again.", "network") from None


def transcribe(audio):
    if not audio:
        raise VoiceError("Nothing was recorded.", "empty")
    if len(audio) > MAX_AUDIO:
        raise VoiceError("That recording is too long. Try a shorter one.", "too-long")
    kind, model = engine()
    try:
        text = transcribe_cloud(audio) if kind == "openai" else voice_local.transcribe(audio, model)
    except voice_local.LocalError as error:
        raise VoiceError(str(error), error.code) from None
    return {"text": text}


def install(model_id):
    """Download a model, yielding {"progress": 0..1} lines, then {"done": true}."""
    updates = []
    def note(fraction):
        if not updates or fraction - updates[-1] >= 0.01 or fraction >= 1:
            updates.append(fraction)
            print(json.dumps({"progress": round(fraction, 3)}), flush=True)
    try:
        voice_models.install(model_id, note)
    except voice_models.ModelError as error:
        return {"error": str(error), "code": "failed"}
    return {"done": True}


def run(args, stdin):
    """`agent voice options | choose KIND [MODEL] | install MODEL | transcribe | key`: JSON lines."""
    try:
        if args[:1] == ["options"]:
            return options()
        if args[:1] == ["choose"] and len(args) in (2, 3):
            return choose(args[1], args[2] if len(args) == 3 else None)
        if args[:1] == ["install"] and len(args) == 2:
            return install(args[1])
        if args[:1] == ["transcribe"]:
            return transcribe(stdin.read(MAX_AUDIO + 1))
        if args[:1] == ["key"]:
            save_key(stdin.readline())
            return {"saved": True}
    except (VoiceError, voice_models.ModelError) as error:
        return {"error": str(error), "code": getattr(error, "code", "failed")}
    raise ValueError("Usage: agent voice options | choose KIND [MODEL] | install MODEL | transcribe | key")
