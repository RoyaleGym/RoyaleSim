"""docs/calibration.md states counts over the ledger, and they must still hold.

They went stale TWICE on 2026-09-22: corrected in the morning, wrong again by the afternoon,
because every status promotion moves them and nothing was watching. A number typed into prose
is stale the moment the ledger moves, so it needs a gate or it needs not to be there.
"""

import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DOC = ROOT / "docs" / "calibration.md"
TOOL = ROOT / "tools" / "ledger_census.py"

sys.path.insert(0, str(ROOT / "tools"))
import ledger_census  # noqa: E402


def run_check(doc: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(TOOL), "--check", str(doc)],
        capture_output=True, text=True, cwd=ROOT,
    )


def test_the_calibration_page_counts_match_the_ledger():
    got = run_check(DOC)
    assert got.returncode == 0, got.stdout + got.stderr


def test_every_pattern_still_matches_its_sentence():
    """A pattern that stops matching is a silent hole: the sentence was reworded and its
    number is no longer watched. The checker reports that as a failure, not a pass."""
    text = DOC.read_text(encoding="utf-8")
    import re
    for pattern, _ in ledger_census.CHECKS:
        assert len(re.findall(pattern, text)) == 1, f"{pattern!r} no longer matches exactly once"


def test_the_check_fails_on_a_stale_page(tmp_path):
    """THE PLANT. Without this the test above passes on a checker that can only say yes."""
    text = DOC.read_text(encoding="utf-8")
    real = ledger_census.census()["entries"]
    stale = text.replace(f"All {real} carry a status", f"All {real + 7} carry a status", 1)
    assert stale != text, "the plant did not land: that sentence is not in the page any more"
    doc = tmp_path / "calibration.md"
    doc.write_text(stale, encoding="utf-8")
    got = run_check(doc)
    assert got.returncode == 1, "a page with a wrong count passed the check"
    assert "STALE" in got.stdout, got.stdout


def test_the_census_stops_at_an_entry_and_the_one_nested_entry_is_excluded():
    """THE POPULATION IS A CONVENTION, and this pins it rather than assuming it.

    `entries()` records a dict with a string `status` and does NOT recurse into it. So
    `pathfinding.PATHFINDING_COSTS.application`, which carries its own status INSIDE an entry,
    is not counted. That is deliberate and it is what `tools/check_data.py`, the README tile
    and the integrator's own census all count, so changing it here would make four tools
    disagree. The exclusion is recorded because it is invisible otherwise: the count is one
    short of every status in the file, and nothing else says so.
    """
    ledger = json.loads((ROOT / "data" / "calibration.json").read_text(encoding="utf-8"))
    keys = ledger_census.entries(ledger)
    assert "pathfinding.PATHFINDING_COSTS" in keys
    assert "pathfinding.PATHFINDING_COSTS.application" not in keys, (
        "the census now recurses into entries, which changes the population every other tool counts"
    )
    # and it really does carry a status, so the exclusion is a choice and not an absence
    nested = ledger["pathfinding"]["PATHFINDING_COSTS"]["application"]
    assert isinstance(nested.get("status"), str), "this test's premise is gone: that key no longer has a status"

    # Every status in the file, counted without the convention, to show the size of the gap.
    def all_statuses(node):
        n = 0
        for key, value in node.items():
            if key.startswith("$") or not isinstance(value, dict):
                continue
            if isinstance(value.get("status"), str):
                n += 1
            n += all_statuses(value)
        return n

    assert all_statuses(ledger) == len(keys) + 1, (
        "exactly one entry is nested inside another; if that changes, the convention needs restating"
    )
