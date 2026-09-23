"""The globals cross-check must stay green on the tracked data, and its plants must fire.

This file exists because of a failure the repository had no way to see. `START_MANA` was
promoted to a measured 6 on 2026-09-22 and pushed. The 2018 table says 5, so
`tools/extract_globals.py` -- step 3 of the install in README.md -- started exiting 1 for
every fresh clone, and nothing noticed, because the gate ran ONLY in that manual step.
A gate nobody runs is a gate that reports on nothing.

Two different things are checked here and they fail for different reasons:
  * the gate is green on what is tracked, which is the install step working; and
  * each of the tool's own plants still goes red, which is the gate working.
The second is what keeps the first from being satisfied by a gate that has quietly stopped
comparing anything.
"""

import json
import pathlib
import subprocess
import sys

import pytest

ROOT = pathlib.Path(__file__).resolve().parent.parent
TOOL = ROOT / "tools" / "extract_globals.py"

sys.path.insert(0, str(ROOT / "tools"))
import extract_globals as eg  # noqa: E402


def calibration() -> dict:
    return json.loads((ROOT / "data" / "calibration.json").read_text(encoding="utf-8"))


def run_gate(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(TOOL), "--quiet", *args],
        capture_output=True, text=True, cwd=ROOT,
    )


def test_the_install_step_exits_zero_on_the_tracked_data():
    """Step 3 of README.md's install, run the way a newcomer runs it."""
    got = run_gate()
    assert got.returncode == 0, got.stdout + got.stderr


def test_the_gate_is_comparing_something():
    """Green means nothing if the comparison is empty. Assert the coverage, not the colour."""
    g, _ = eg.read_globals(eg.SRC)
    fail, agree, _absent, _context, _superseded = eg.cross_check(g, calibration())
    assert not fail, fail
    assert len(agree) >= 20, f"only {len(agree)} registry values are cross-checked against globals"


def test_every_declared_divergence_is_reported_and_none_is_swallowed():
    """The population is the notes in the ledger, so this cannot pass by there being none."""
    notes = {qual for qual, entry in eg.registry_constants(calibration())
             if isinstance(entry.get("supersedes_globals"), dict)}
    assert notes, "no entry declares a divergence, so nothing here is being tested"
    g, _ = eg.read_globals(eg.SRC)
    _, _, _, _, superseded = eg.cross_check(g, calibration())
    reported = {line.split(" =", 1)[0] for line in superseded}
    assert notes <= reported, f"declared but not reported: {sorted(notes - reported)}"


@pytest.mark.parametrize("plant", eg.PLANTS)
def test_the_plant_lands(plant: str):
    """Each plant must go red with ITS OWN message. The tool compares the message rather
    than the exit code, which is what caught `stale-note` going red for a different
    reason the first time it was written."""
    got = run_gate("--plant", plant)
    assert got.returncode == 0, got.stdout + got.stderr
    assert "LANDED" in got.stdout, got.stdout + got.stderr


def test_the_tool_offers_every_plant_it_is_supposed_to():
    """Guards the parametrisation above from shrinking to nothing unnoticed."""
    assert len(eg.PLANTS) >= 7, eg.PLANTS


DIVERGENCE_CASES = [
    ({}, "fail", "undocumented"),
    ({"supersedes_globals": {"key": "START_MANA", "value": 5, "why": "measured"}}, "superseded", ""),
    ({"supersedes_globals": {"key": "OTHER_KEY", "value": 5, "why": "measured"}}, "fail", "wrong key"),
    ({"supersedes_globals": {"key": "START_MANA", "value": 4, "why": "measured"}}, "fail", "stale"),
    ({"supersedes_globals": {"key": "START_MANA", "value": 5, "why": "  "}}, "fail", "no reason"),
    ({"supersedes_globals": {"key": "START_MANA", "value": 5}}, "fail", "incomplete"),
    ({"supersedes_globals": "measured, honest"}, "fail", "undocumented"),
]


@pytest.mark.parametrize(("entry", "kind", "why"), DIVERGENCE_CASES)
def test_a_note_is_only_accepted_while_it_still_fits_the_file(entry, kind, why):
    """A note is a claim ABOUT globals.csv, so it is checked against globals.csv. Otherwise
    the field is a switch that turns the cross-check off for whatever it is written on."""
    got, msg = eg.check_divergence(
        entry, "match.START_MANA", "START_MANA", 5, 6, "globals.csv", "undocumented: START_MANA",
    )
    assert got == kind, msg
    assert why in msg


def test_a_note_on_a_value_that_agrees_is_itself_a_failure():
    """The entry would be claiming to depart from a table it matches."""
    assert eg.agreement_is_declared_away({"supersedes_globals": {"key": "K"}}, "match.X")
    assert eg.agreement_is_declared_away({}, "match.X") is None
