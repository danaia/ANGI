"""Read the repository's supported build and install targets."""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BOARDS = json.loads((ROOT / "scripts/boards.json").read_text())["boards"]
BY_ID = {board["id"]: board for board in BOARDS}



def published_boards(commit):
    registry = subprocess.check_output(
        ["git", "show", f"{commit}:scripts/boards.json"], cwd=ROOT, text=True)
    return json.loads(registry)["boards"]
