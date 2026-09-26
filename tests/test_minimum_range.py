"""A Mortar cannot keep a target inside its MinimumRange (targeting.MINIMUM_RANGE).

WHAT THIS PINS. On the 16.402 corpus a Mortar (CollisionRadius 600, MinimumRange 3500) shooting a Giant (radius 750)
that walks in DROPS it on the first tick whose start-of-tick edge distance, centre distance less both radii, is below
3500 (4,829 apart: 3,479), goes idle, and never takes it again while it stands there. The card data carries MinimumRange
and the engine never read it, so its Mortar shot at its own feet.

WHY THE CONTROLS ARE HERE. "Never targets the Giant near the Mortar" also passes for a Mortar that targets nothing, so
the Mortar must first lock the Giant at range; and the Giant must actually stand inside the range for a while, or the
check looks at nothing. The old arm must be today's engine: its Mortar keeps the Giant inside the range.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `minimum_range_unread` -- the loader drops MinimumRange, so the Mortar keeps a target at its feet:
    test_the_mortar_drops_a_target_inside_its_minimum_range.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.MINIMUM_RANGE"
NEW_ARM, OLD_ARM = "client16402_edge_distance", "not_read"
# ENTITY_FIELDS: 0 uid, 1 team, 3 card_id, 4 tower_slot, 5 x, 6 y, 15 target_uid
UID, TEAM, CARD, SLOT, X, Y, TARGET = 0, 1, 3, 4, 5, 6, 15
MORTAR, GIANT = 0, 1
MIN_RANGE, R_MORTAR, R_GIANT = 3500, 600, 750


def run(arm: str, ticks: int = 500) -> list:
    """(target is the Giant, the start-of-tick edge distance) per tick, for a blue Mortar at the corpus's (14500, 11500)
    and a red Giant walking in from (14500, 22000)."""
    b = royalesim.Battle(["Mortar", "Giant"], [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    units = [(0, MORTAR, 14500 * SUB, 11500 * SUB, -1), (1, GIANT, 14500 * SUB, 22000 * SUB, -1)]
    b.reset(0, [[0, 1] * 4, [0, 1] * 4], 0, 200, [10_000, 10_000], None, units)
    rows, prev_edge = [], None
    for _ in range(ticks):
        ents = json.loads(b.state_json())["entities"]
        m = next((e for e in ents if e[TEAM] == 0 and e[SLOT] < 0 and e[CARD] == MORTAR), None)
        g = next((e for e in ents if e[TEAM] == 1 and e[SLOT] < 0 and e[CARD] == GIANT), None)
        if m is None or g is None:
            break
        rows.append((m[TARGET] == g[UID], prev_edge))
        d = ((m[X] - g[X]) ** 2 + (m[Y] - g[Y]) ** 2) ** 0.5 / SUB
        prev_edge = d - R_MORTAR - R_GIANT
        b.step([], 1)
    return rows


def test_the_mortar_drops_a_target_inside_its_minimum_range():
    rows = run(NEW_ARM)
    assert any(t for t, e in rows if e is not None and e >= MIN_RANGE), "the Mortar never locked the Giant at range"
    inside = [t for t, e in rows if e is not None and e < MIN_RANGE]
    assert len(inside) >= 20, f"the Giant stood inside the range for {len(inside)} ticks only"
    assert not any(inside), "the Mortar kept or took the Giant inside its minimum range"


def test_the_old_arm_is_todays_engine():
    rows = run(OLD_ARM)
    assert any(t for t, e in rows if e is not None and e < MIN_RANGE)
