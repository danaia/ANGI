"""Read the repository's supported build and install targets."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BOARDS = json.loads((ROOT / "scripts/boards.json").read_text())["boards"]
BY_ID = {board["id"]: board for board in BOARDS}
