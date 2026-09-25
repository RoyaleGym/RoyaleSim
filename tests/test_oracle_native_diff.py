"""THE ORACLE DIFF GATE, as a test.

Runs tools/oracle_diff.py's comparison in-process: for every `walk/` trace the Rust
engine must reproduce the recorded trajectory EXACTLY -- zero native units of
error on every tick from the recorded unit's first moving tick to the tick it starts
attacking, which is where the isolated-unit laws stop applying (calibration
movement.CONTACT_DOMAIN).

SKIPS, LOUDLY, when data/oracle-native is absent: the traces are large and are not
required to be present in a checkout. A skip here is not a pass -- tools/oracle_diff.py
is the thing to run when they are.

WHAT IT DOES NOT GATE
  * building_Giant / repath_Giant: their first paths are the same COST as the
    recorded paths but not the same cells, because this model does not reproduce the
    expansion order. They are reported by the tool, not asserted here.
  * a deploy's crowd siblings (Skeletons #1 and #2): crowd separation is unmodelled
    and sets no flag in the recorded trace either.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))

TRACES = ROOT / "data" / "oracle-native"
WALK = TRACES / "walk"


pytestmark = pytest.mark.skipif(
    not WALK.is_dir() or not any(WALK.glob("*.jsonl.gz")),
    reason=f"{WALK} is absent; run tools/oracle_diff.py where the recorded 15.535.29 traces are",
)


def _diffs():
    import oracle_diff

    out = []
    for p in sorted(WALK.glob("*.jsonl.gz")):
        for d in oracle_diff.diff_trace(p, "walk"):
            out.append(d)
    return out


@pytest.fixture(scope="module")
def diffs():
    return _diffs()


def test_the_walk_traces_are_reproduced_exactly(diffs):
    """Zero error, over the WHOLE window -- not over whatever ticks happened to run.

    The window is the recorded unit's first moving tick to the tick before it starts
    attacking, and it is asserted as well as the error. It used to be guarded only by
    `len(d.rows) >= 100`: `diff_trace` stops early and leaves a note when the engine's
    unit dies or never moves, so a regression that killed a unit two thirds of the way
    through the Giant's 308-tick walk would still have left 100 rows and passed. The
    Skeletons case used to exit that way; it no longer does, and the pin below holds
    all six to the whole window.
    """
    import oracle_diff

    gated = [d for d in diffs if d.card in oracle_diff.WALK_GATE_CARDS and not d.note.startswith("  SKIPPED")]
    assert len(gated) == 6, f"expected the six measured walk cards, got {[d.card for d in gated]}"
    for d in gated:
        assert d.rows, f"{d.name}: nothing was compared"
        assert d.max_err == 0, (
            f"{d.name} ({d.card}): max error {d.max_err} native units, "
            f"first divergence {d.first_bad}"
        )
        if d.card == "Skeletons":
            # THE SHORT WINDOW IS GONE, and this pins that rather than the old
            # shortfall. state.rs `setup_spawn_place` still materialises ONE entity
            # per spawn whatever the card's summon count, so the engine still faces
            # the princess tower with a single Skeleton where the recording has three
            # and still takes every shot -- but it now survives the whole window: it
            # is on the board on the window's last tick, t227, and goes at t228, one
            # tick past the end. So the comparison covers all 107 of the window's
            # ticks, the case is held to the same whole-window standard as the other
            # five below, and what is pinned here is that the tracked unit outlives
            # the window at all.
            assert d.note == "", f"{d.name}: unexpected note {d.note!r}"
            assert (len(d.rows), d.window_ticks) == (107, 107), f"{d.name}: {len(d.rows)}/{d.window_ticks}"
        assert not d.note, f"{d.name}: the comparison did not run clean --{d.note}"
        assert len(d.rows) == d.window_ticks, (
            f"{d.name} ({d.card}): compared {len(d.rows)} of the window's "
            f"{d.window_ticks} ticks (t{d.window[0]}..t{d.window[1]})"
        )
        assert d.rows[-1][0] == d.window[1], f"{d.name}: the comparison stopped at t{d.rows[-1][0]}, not t{d.window[1]}"


def test_the_deploy_countdown_is_the_measured_one():
    """spec 9.1 / calibration movement.DEPLOY_TIMING, which NOTHING else here covers.

    The trajectory diff spawns its walker through `setup_spawn_place`, which sets
    `deploy_ms = 0`, and then re-aligns on the engine's own first moving tick -- so it
    absorbs any countdown error rather than catching one. This issues a real deploy
    COMMAND instead: the unit must appear the tick after the command and first move
    DeployTime/TICK_MS ticks after appearing.
    """
    import oracle_diff

    got, want, note = oracle_diff.deploy_countdown_is_spawn_plus_deploy_time()
    assert got == want, note


def test_the_first_path_matches_the_oracles_published_cells(diffs):
    """ALL SIX publish the same first-path cell list as the recorded path, cell for cell.

    MiniPekka used to be the one exception, and the reason was card DATA rather than
    the search: the goal rule is Range + the unit's own CollisionRadius, and a
    MiniPekka Range of 1050 stops the path one cell short of the recorded goal cell, so
    the engine's list was the recorded one without its first cell.
    data/derived/cards.json now carries the 15.535.29 table, whose MiniPekka Range is
    800, and the list is the recorded one. The pin is therefore the whole
    set, with no exception left to carve out.
    """
    import oracle_diff

    gated = [d for d in diffs if d.card in oracle_diff.WALK_GATE_CARDS and not d.note.startswith("  SKIPPED")]
    # Vacuity: an empty cell list on both sides would satisfy the equality below.
    empty = [(d.card, len(d.engine_cells), len(d.oracle_cells)) for d in gated]
    assert all(d.engine_cells and d.oracle_cells for d in gated), f"a first-path cell list is empty: {empty}"
    same = {d.card for d in gated if d.engine_cells == d.oracle_cells}
    assert same == {"Knight", "Giant", "Golem", "HogRider", "Skeletons", "MiniPekka"}, sorted(same)
