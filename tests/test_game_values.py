"""docs/game-values.md is what tools/game_values.py prints from the engine, so the cheatsheet cannot drift.

A change to the card table or the calibration changes the page. When this fails, regenerate it:

    python tools/game_values.py --write
"""

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

import game_values  # noqa: E402


def test_the_committed_cheatsheet_is_what_the_engine_prints() -> None:
    committed = game_values.PAGE.read_text(encoding="utf-8")
    assert committed == game_values.page(), (
        "docs/game-values.md is stale against this engine. Regenerate it: python tools/game_values.py --write"
    )


def test_the_cheatsheet_reads_the_full_card_table() -> None:
    """Green means nothing if the page was generated from the four-card fallback table."""
    rows = [line for line in game_values.page().splitlines() if line.startswith("| ") and " troop " in line]
    assert len(rows) > 50, f"only {len(rows)} troop rows: the engine loaded a partial card table"
