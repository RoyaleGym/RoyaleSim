"""The Fisherman stands, throws his hook and drags his target to him (combat.SPECIAL_HOOK).

WHAT THIS PINS. On client 15.535.29 (the fisherman-knight, -near and -far scenarios, 5 runs) a Fisherman stops walking
once a Knight is within 7,500 centre to centre (SpecialRange 7000 plus 500, on start-of-tick positions; the far runs
bracket it in [7,495, 7,547)), and loads the hook for SpecialLoadTime, 1300 ms = 26 ticks. He then throws
FishermanProjectile, which flies at its Speed 800 a tick. The Knight is dragged straight at him in steps of 510, and
the drag stops before the step that would bring the centres within 1,200 of each other (DragMargin 200 plus both radii,
500 each), so it ends anywhere from 1,200 to 1,710 apart depending on where it started (1,450, 1,231, 1,607, 1,671 and
1,227 in the five runs). Only then does his ordinary melee start. The hook deals no damage. The engine read none of
the special columns: its Fisherman walked to melee range.

WHY THE CONTROLS ARE HERE. "The Knight ends near him" also passes for a Knight that simply walked up, so the drag must
show as steps far faster than a walk; the stand must last the load time, not the whole approach; and the old arm must be
today's engine, where nothing moves faster than a walk. The stop is asserted as a window, not a point: an earlier draft
pinned "edge 200" from one run measured with a wrong Knight radius (750), which four more runs refuted.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `special_hook_unread` -- the special never starts; the Fisherman walks to melee range:
    test_the_fisherman_stands_hooks_and_drags.
  * `hook_drag_unread` -- the hook lands and drags nothing: test_the_fisherman_stands_hooks_and_drags.
  * `hook_drag_steps_inside_margin` -- the drag takes the step that ends inside the margin:
    test_the_fisherman_stands_hooks_and_drags.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.SPECIAL_HOOK"
NEW_ARM, OLD_ARM = "client_hook_drag", "not_read"
# ENTITY_FIELDS: 1 team, 4 tower_slot, 5 x, 6 y, 7 hp
TEAM, SLOT, X, Y, HP = 1, 4, 5, 6, 7
LOAD_TICKS, DRAG_STEP, MARGIN, R_FISHERMAN, R_KNIGHT = 26, 510, 200, 500, 500
STOP_FLOOR = MARGIN + R_FISHERMAN + R_KNIGHT  # 1200: the drag never takes a step that ends closer than this


def run(arm: str, ticks: int = 120) -> list:
    """A blue Fisherman at (9000, 11000) and a red Knight at (9000, 17500), 6,500 apart, out of every tower's reach at
    the start. Per tick: (Fisherman step, Knight step, centre distance, Knight hp)."""
    b = royalesim.Battle(["Fisherman", "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    units = [(0, 0, 9000 * SUB, 11000 * SUB, -1), (1, 1, 9000 * SUB, 17500 * SUB, -1)]
    b.reset(0, [[0, 1] * 4, [0, 1] * 4], 0, 200, [10_000, 10_000], None, units)
    rows, prev = [], None
    for _ in range(ticks):
        ents = json.loads(b.state_json())["entities"]
        f = next((e for e in ents if e[TEAM] == 0 and e[SLOT] < 0), None)
        k = next((e for e in ents if e[TEAM] == 1 and e[SLOT] < 0), None)
        if f is None or k is None:
            break
        fp, kp = (f[X] / SUB, f[Y] / SUB), (k[X] / SUB, k[Y] / SUB)
        d = ((fp[0] - kp[0]) ** 2 + (fp[1] - kp[1]) ** 2) ** 0.5
        if prev is not None:
            fs = ((fp[0] - prev[0][0]) ** 2 + (fp[1] - prev[0][1]) ** 2) ** 0.5
            ks = ((kp[0] - prev[1][0]) ** 2 + (kp[1] - prev[1][1]) ** 2) ** 0.5
            rows.append((round(fs), round(ks), round(d), k[HP]))
        prev = (fp, kp)
        b.step([], 1)
    return rows


def test_the_fisherman_stands_hooks_and_drags():
    rows = run(NEW_ARM)
    assert all(fs == 0 for fs, _, _, _ in rows[:LOAD_TICKS]), "he walked during the load"
    drag = [ks for _, ks, _, _ in rows if ks > 300]
    assert drag, "the Knight was never dragged"
    assert all(abs(ks - DRAG_STEP) <= 2 for ks in drag[1:-1]), drag
    end = next(i for i, (_, ks, _, _) in enumerate(rows) if ks > 300 and (i + 1 == len(rows) or rows[i + 1][1] <= 300))
    d_end = rows[end][2]
    assert d_end >= STOP_FLOOR - 2, f"the drag stepped inside {STOP_FLOOR}: it ended {d_end} apart"
    assert d_end - DRAG_STEP < STOP_FLOOR + 2, f"the drag stopped early: {d_end} apart, room for another step"
    assert rows[end][3] == rows[0][3], "the hook itself did damage"


def test_the_old_arm_is_todays_engine():
    """Checked on 7bfcfd2 with the key dropped: both walk; no step exceeds a walk."""
    rows = run(OLD_ARM)
    assert rows
    assert all(ks <= 150 for _, ks, _, _ in rows), max(ks for _, ks, _, _ in rows)
