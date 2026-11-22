"""Validated, private model choices for the ANGI workshop roles."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

from messaging import ROLES

# Chat and legacy OS runs share the original "os" key.
SINGLE_ROLE = "single"
ALL_ROLES = (*ROLES, "os", SINGLE_ROLE)


# Services Agent Angi can use, one at a time.
SERVICES = ("apple", "openai", "lmstudio")


def settings_path():
    return Path(os.environ["XDG_STATE_HOME"]) / "angi-swarm/models.json"


def active_path():
    return Path(os.environ["XDG_STATE_HOME"]) / "angi-swarm/active-service.json"


def selections(service=None):
    """Saved role models; with a service, only that service's models."""
    path = settings_path()
    if not path.exists():
        return {}
    result = json.loads(path.read_text())
    if not isinstance(result, dict) or any(role not in ALL_ROLES or not isinstance(model, str) or not model.strip() or "/" not in model for role, model in result.items()):
        raise RuntimeError("Invalid saved Agent Angi model choices.")
    if service is None:
        return result
    return {role: model for role, model in result.items() if model.partition("/")[0] == service}


def active():
    """The one service new builds use, or None. Before a service is chosen, existing model choices decide it."""
    path = active_path()
    if path.exists():
        try:
            service = json.loads(path.read_text()).get("service")
        except (ValueError, AttributeError):
            service = None
        if service not in SERVICES:
            raise RuntimeError("Invalid saved Agent Angi service choice.")
        return service
    for model in selections().values():
        if model.partition("/")[0] in SERVICES:
            return model.partition("/")[0]
    return None


def activate(service):
    if service not in SERVICES:
        raise ValueError("Agent Angi cannot use this service yet.")
    atomic_write(active_path(), {"service": service})
    return service


def active_selections():
    service = active()
    return selections(service) if service else {}


def catalog(provider):
    if provider not in SERVICES:
        raise ValueError("Unknown model provider.")
    if provider in ("apple", "lmstudio"):
        from providers import local_config
        return [provider + "/" + key for key in local_config(provider).get("models", {})]
    result = subprocess.run(["opencode", "models", provider], capture_output=True, text=True, timeout=45)
    if result.returncode:
        raise RuntimeError(f"Unable to load {provider} models from OpenCode.")
    return sorted({line.strip() for line in result.stdout.splitlines() if line.strip().startswith(provider + "/")})


def save(role, model):
    if role not in ALL_ROLES:
        raise ValueError("Unknown workshop role.")
    provider = model.partition("/")[0]
    if provider != active():
        raise ValueError("Use this service for Agent Angi before choosing its models.")
    if model not in catalog(provider):
        raise ValueError("Choose a model available in OpenCode.")
    choices = selections()
    choices[role] = model
    path = settings_path()
    atomic_write(path, choices)
    return choices


def atomic_write(path, choices):
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix="models.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w") as output:
            json.dump(choices, output, indent=2)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    return choices
