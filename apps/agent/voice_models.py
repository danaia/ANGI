"""Local speech models for the microphone: what suits this device, and a verified download.

The models are whisper.cpp's quantized Whisper files. Sizes and SHA-256 sums are pinned, so a
download that does not match is discarded rather than used.
"""
import hashlib
import os
from pathlib import Path
import shutil
import urllib.request

BASE_URL = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/"
# `seconds` is the measured time to transcribe about ten seconds of speech on a Raspberry Pi 5 (four cores).
MODELS = [
    {"id": "tiny", "name": "Tiny", "file": "ggml-tiny-q5_1.bin", "size": 32152673, "need_gb": 1, "seconds": 2,
     "sha256": "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7",
     "quality": "Basic. Clear, close speech only."},
    {"id": "base", "name": "Base", "file": "ggml-base-q5_1.bin", "size": 59707625, "need_gb": 1, "seconds": 3,
     "sha256": "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
     "quality": "Good for clear speech."},
    {"id": "small", "name": "Small", "file": "ggml-small-q5_1.bin", "size": 190085487, "need_gb": 2, "seconds": 8,
     "sha256": "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
     "quality": "Accurate for most voices and accents."},
    {"id": "medium", "name": "Medium", "file": "ggml-medium-q5_0.bin", "size": 539212467, "need_gb": 4, "seconds": 25,
     "sha256": "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
     "quality": "Most accurate, but slow on this device."},
]
RECOMMENDED_WITHIN = 15  # seconds to transcribe ten seconds of speech
TOO_SLOW = 60


class ModelError(Exception):
    pass


def find(model_id):
    for model in MODELS:
        if model["id"] == model_id:
            return model
    raise ModelError("Unknown speech model.")


def hardware():
    """The device name, memory and cores, from the kernel."""
    try:
        device = Path("/proc/device-tree/model").read_bytes().rstrip(b"\0").decode()
    except OSError:
        device = ""
    memory = 0.0
    try:
        for line in Path("/proc/meminfo").read_text().splitlines():
            if line.startswith("MemTotal:"):
                memory = int(line.split()[1]) / 1024 / 1024
    except (OSError, ValueError, IndexError):
        pass
    return {"device": device or "This computer", "memory_gb": round(memory, 1), "cores": os.cpu_count() or 1}


def estimate(model, device):
    """Seconds to transcribe ten seconds of speech: scaled from a four-core Pi 5."""
    slower = 1 if "Raspberry Pi 5" in device["device"] else 2
    return round(model["seconds"] * slower * 4 / max(1, min(device["cores"], 4)))


def practical(device):
    """The models worth offering here, each with its estimate; the best one that stays quick is recommended."""
    choices = [dict(model, estimate=estimate(model, device)) for model in MODELS
               if device["memory_gb"] >= model["need_gb"] and estimate(model, device) <= TOO_SLOW]
    quick = [choice for choice in choices if choice["estimate"] <= RECOMMENDED_WITHIN]
    best = quick[-1]["id"] if quick else (choices[0]["id"] if choices else "")
    for choice in choices:
        choice["recommended"] = choice["id"] == best
        choice["slow"] = choice["estimate"] > RECOMMENDED_WITHIN
    return choices


def directory():
    return Path(os.environ["XDG_DATA_HOME"]) / "angi-voice/models"


def path(model):
    return directory() / model["file"]


def installed(model):
    try:
        return path(model).stat().st_size == model["size"]
    except OSError:
        return False


def install(model_id, progress=lambda fraction: None):
    """Download and verify one model. `progress` receives 0..1."""
    model = find(model_id)
    if installed(model):
        return
    folder = directory()
    folder.mkdir(mode=0o700, parents=True, exist_ok=True)
    if shutil.disk_usage(folder).free < model["size"] * 2:
        raise ModelError("There is not enough free storage for that model.")
    part = path(model).with_suffix(".part")
    digest = hashlib.sha256()
    done = 0
    try:
        request = urllib.request.Request(BASE_URL + model["file"], headers={"User-Agent": "angi"})
        with urllib.request.urlopen(request, timeout=30) as response, open(part, "wb") as output:
            while chunk := response.read(256 * 1024):
                output.write(chunk)
                digest.update(chunk)
                done += len(chunk)
                progress(min(1.0, done / model["size"]))
            output.flush()
            os.fsync(output.fileno())
        if done != model["size"] or digest.hexdigest() != model["sha256"]:
            raise ModelError("The download was damaged and was discarded. Try again.")
        os.replace(part, path(model))
    except OSError:
        raise ModelError("The download failed. Check the network connection and try again.") from None
    finally:
        part.unlink(missing_ok=True)
