"""THE ORACLE DIFF GATE, as a test.

Runs tools/oracle_diff.py's comparison in-process: for every `walk/` trace the Rust
engine must reproduce the offline oracle's trajectory EXACTLY -- zero native units of
error on every tick from the oracle's first moving tick to the tick its unit starts
attacking, which is where the isolated-unit laws stop applying (calibration
movement.CONTACT_DOMAIN).

SKIPS, LOUDLY, when data/oracle-native is absent: the traces are large and are not
required to be present in a checkout. A skip here is not a pass -- tools/oracle_diff.py
is the thing to run when they are.

WHAT IT DOES NOT GATE
  * building_Giant / repath_Giant: their first paths are the same COST as the
    oracle's but not the same cells, because this model does not reproduce the
    expansion order. They are reported by the tool, not asserted here.
  * a deploy's crowd siblings (Skeletons #1 and #2): crowd separation is unmodelled
    and sets no flag in the oracle's own state either.
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
    reason=f"{WALK} is absent; run tools/oracle_diff.py where the offline-oracle traces are",
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

    The window is the oracle's first moving tick to the tick before its unit starts
    attacking, and it is asserted as well as the error. It used to be guarded only by
    `len(d.rows) >= 100`: `diff_trace` stops early and leaves a note when the engine's
    unit dies or never moves, so a regression that killed a unit two thirds of the way
    through the Giant's 308-tick walk would still have left 100 rows and passed. The
    Skeletons case was already exiting that way.
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
            # THE ONE KNOWN SHORT WINDOW, pinned exactly rather than waved through.
            # state.rs `setup_spawn_place` materialises ONE entity per spawn whatever
            # the card's summon count, so the engine faces the princess tower with a
            # single Skeleton where the oracle has three -- it takes every shot and
            # dies at t226, one tick before the oracle's window ends. Fixing it means
            # spawning three, which puts the comparison in the crowd-separation regime
            # the spec says is unmodelled, so it stays a one-unit comparison and the
            # shortfall is pinned here instead.
            assert d.note == "  TRACKED UNIT GONE at t226", f"{d.name}: unexpected note {d.note!r}"
            assert (len(d.rows), d.window_ticks) == (105, 107), f"{d.name}: {len(d.rows)}/{d.window_ticks}"
            continue
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
    """Five of the six also publish the same cell list; MiniPekka does not, and the
    reason is card DATA, not the search.

    data/derived/cards.json is the 2018 data (its own `vintage_warning` says so) and
    gives MiniPekka Range 1050 where the live 2026 build ships 800, so the goal rule
    (Range + own CollisionRadius) stops one cell short of the oracle's. It costs
    nothing on the trajectory -- the unit is still walking up the shared prefix when
    the oracle starts attacking -- which is why the gate above is still exact.
    """
    import oracle_diff

    gated = [d for d in diffs if d.card in oracle_diff.WALK_GATE_CARDS and not d.note.startswith("  SKIPPED")]
    same = {d.card for d in gated if d.engine_cells == d.oracle_cells}
    assert same == {"Knight", "Giant", "Golem", "HogRider", "Skeletons"}, sorted(same)
    mini = next(d for d in gated if d.card == "MiniPekka")
    assert mini.engine_cells == mini.oracle_cells[1:], "MiniPekka should differ only in the goal cell"
