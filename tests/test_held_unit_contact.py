"""A unit held by a freeze or a stun still meets its neighbours (collision.HELD_UNIT_CONTACT).

WHAT THIS PINS. On the 16.402 corpus a troop held by a freeze or a stun keeps its contact update at speed 0. One seat
per battle, 40 battles, every held unit-tick of a walking or attacking troop: with a ground neighbour overlapping it
at the start of the tick it moved on 52 of 52 (the separation push; 6 Goblins frozen by Ice Spirits, 3 battles),
with none it stood still on 353 of 353, and its avoidance offset shrank by 10 on 28 of 28 held ticks that started
nonzero, walking or attacking. Its neighbours meet it too: in 20260919-184136 (ticks 351-358) a Skeleton walking past
a Goblin frozen by an Ice Spirit is turned off it and the Goblin is pushed 11 to 60 a tick while it is held. On client
15.535.29 a Knight stunned by an Electro Giant's reflect moved on 13 of 13 stunned ticks that began with the Giant
overlapping it and on 0 of 5 that began apart. The old arm takes a held unit out of the move pass and out of every
neighbour's scans, so a walker enters a held unit's circle unopposed and the held unit never moves. How far a held
unit is pushed per tick is not pinned here: the cases assert that it moves, not by how much.

THE SCENES.
  * The freeze: two Blue Knights on the left lane, one at (3500, 13000) and one behind it at (3500, 7500). A Red
    Freeze on (3500, 13500), played on tick 103, holds the front Knight for 80 ticks and misses the rear one (it stands
    5,500 away), which walks up the lane into the held Knight's back. At the old arm the rear Knight reaches about 500
    from the held one (the radii sum to 1000) and the held Knight does not move until the hold ends.
  * The stun (the client 15.535.29 Electro Giant scenario's spots): a red Knight at (13945, 15507) and a blue Electro
    Giant at (9500, 11500), set down together. The Giant walks up the Knight's line; the Knight attacks it and is
    stunned on each hit. From the second stun on, the Giant walks through it. At the old arm the Knight stands while
    the Giant walks into it, and the two are thrown apart when a stun ends.

WHY THE CONTROLS ARE HERE. A held unit with nobody overlapping it must stand still under both arms (353 of 353 corpus
ticks, 0 of 5 in the client scenario, and tests/status.rs `a_freeze_is_a_whole_unit_hold_of_its_bufftime`): the new
arm is a contact update at speed 0, not a walk. And the rear Knight must never be held itself, or the freeze scene
tests two held units.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run
this file: the named tests go red and the rest stay green.
  * `held_contact_invisible` -- under client16402_speed_zero_update a held unit is still left out of its neighbours'
    scans: test_a_held_unit_is_pushed_by_the_unit_walking_into_it,
    test_a_stunned_knight_is_moved_by_the_giant_walking_through_it.
  * `held_contact_walks` -- under client16402_speed_zero_update a held unit takes its walking step, at its own
    speed: test_a_held_unit_with_nobody_near_stands_still, and likely
    test_a_held_unit_is_pushed_by_the_unit_walking_into_it.
"""

from __future__ import annotations

import json
import math
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "collision.HELD_UNIT_CONTACT"
NEW_ARM, OLD_ARM = "client16402_speed_zero_update", "out_of_the_pass"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
FRONT, REAR, FREEZE_AT, FREEZE_ON = (3500, 13000), (3500, 7500), (3500, 13500), 103
KNIGHTS_RADII = 500 + 500
KNIGHT_AT, GIANT_AT = (13945, 15507), (9500, 11500)
KNIGHT_GIANT_RADII = 500 + 750
#: a tick "begins overlapping" when the start-of-tick centre distance is under both radii by at least this much
OVERLAP = 5
LEDGER = pathlib.Path(__file__).resolve().parent.parent / "data" / "calibration.json"


def overrides(arm) -> dict:
    """`arm` None runs the build's own value."""
    return {KEY: json.dumps(arm)} if arm is not None else {}


def freeze_scene(arm, rear=True, ticks=95):
    """Per tick: (tick, the front Knight's position, its stun ticks, the rear Knight's position or None, its stun
    ticks). `rear` False leaves the rear Knight out."""
    b = royalesim.Battle(["Knight", "Freeze"], [[0, 1, 1], [0, 1, 1]], calibration_overrides=overrides(arm))
    spawns = [(0, 0, FRONT[0] * SUB, FRONT[1] * SUB, -1)] + ([(0, 0, REAR[0] * SUB, REAR[1] * SUB, -1)] if rear else [])
    b.reset(2, [[0] * 8, [1] * 8], 0, 100, [10_000, 10_000], None, spawns)

    def knights():
        es = [
            e
            for e in json.loads(b.state_json())["entities"]
            if e[F["tower_slot"]] < 0 and e[F["team"]] == 0 and e[F["kind"]] == 0
        ]
        return sorted(es, key=lambda e: -e[F["y"]])  # the front Knight first: nothing reorders them in the scene

    rows = []
    for _ in range(ticks):
        tick = json.loads(b.state_json())["tick"]
        cmds = [(1, 0, FREEZE_AT[0] * SUB, FREEZE_AT[1] * SUB)] if tick == FREEZE_ON else []
        played = b.step(cmds, 1)
        if cmds:
            assert played, "the Freeze play returned nothing"
            assert played[0][1] == 0, f"the Freeze was refused: {played}"
        ks = knights()
        front = ks[0]
        back = ks[1] if len(ks) > 1 else None
        rows.append(
            (
                tick + 1,
                (front[F["x"]] // SUB, front[F["y"]] // SUB),
                front[F["stun_ticks"]],
                (back[F["x"]] // SUB, back[F["y"]] // SUB) if back else None,
                back[F["stun_ticks"]] if back else 0,
            )
        )
    return rows


def hold(rows, what):
    """The rows of the hold (the front Knight's stun ticks > 0 after the tick), with the scene's preconditions."""
    held = [i for i, r in enumerate(rows) if r[2] > 0]
    assert held, f"{what}: the scene drifted: the Freeze never held the front Knight"
    assert held == list(range(held[0], held[-1] + 1)), f"{what}: the scene drifted: the hold is not one run"
    assert all(r[4] == 0 for r in rows), f"{what}: the scene drifted: the Freeze held the rear Knight too"
    return held


def start_distance(rows, i):
    """The centre distance of the two Knights at the START of row i's tick (the previous row's positions)."""
    return math.dist(rows[i - 1][1], rows[i - 1][3])


@pytest.mark.parametrize("arm", [NEW_ARM])
def test_a_held_unit_is_pushed_by_the_unit_walking_into_it(arm):
    rows = freeze_scene(arm)
    held = hold(rows, arm)
    # The hold's first tick applies the freeze; from the next one the front Knight is held through the whole tick.
    inside = held[1:]
    touching = [i for i in inside if start_distance(rows, i) <= KNIGHTS_RADII + 60]
    assert touching, f"{arm}: the scene drifted: the rear Knight never came near the held one"
    moved = [rows[i][0] for i in inside if rows[i][1] != rows[i - 1][1]]
    assert moved, (
        f"{arm}: the rear Knight came within {min(start_distance(rows, i) for i in inside):.0f} of the "
        f"held Knight, whose position never changed while it was held"
    )
    over_still = [
        rows[i][0]
        for i in inside
        if start_distance(rows, i) <= KNIGHTS_RADII - OVERLAP and rows[i][1] == rows[i - 1][1]
    ]
    assert not over_still, f"{arm}: the held Knight stood on held ticks {over_still[:3]} that began overlapping"
    far = [rows[i][0] for i in inside if start_distance(rows, i) > KNIGHTS_RADII + 200 and rows[i][1] != rows[i - 1][1]]
    assert not far, f"{arm}: the held Knight moved with nobody touching it on {far[:3]}"


@pytest.mark.parametrize("arm", [OLD_ARM])
def test_the_old_arm_walks_into_a_held_unit(arm):
    rows = freeze_scene(arm)
    held = hold(rows, arm)
    inside = held[1:]
    moved = [rows[i][0] for i in inside if rows[i][1] != rows[i - 1][1]]
    assert not moved, f"{arm}: the held Knight moved on {moved[:3]}"
    closest = min(math.dist(rows[i][1], rows[i][3]) for i in inside)
    assert closest < 800, f"{arm}: the scene drifted: the rear Knight stayed {closest:.0f} away"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_held_unit_with_nobody_near_stands_still(arm):
    rows = freeze_scene(arm, rear=False)
    held = hold(rows, arm)
    moved = [rows[i][0] for i in held[1:] if rows[i][1] != rows[i - 1][1]]
    assert not moved, f"{arm}: a held Knight with no neighbour moved on {moved[:3]}"
    after = held[-1] + 2
    assert after < len(rows), f"{arm}: the scene drifted: the hold ran to the end of the scene"
    assert rows[after][1] != rows[held[-1]][1], f"{arm}: the Knight never walked again"


def stun_scene(arm, ticks=170):
    """Per tick: (held, start-of-tick centre distance, the Knight's move, the Giant's move, the distance from the
    Knight's start to the Giant's END of the tick). Held: the Knight's stun counter is up after the tick or after the
    one before (the reflect stuns it inside the tick of its hit). In this scene the Giant takes its step before the
    Knight's contact update, so a tick's contact is judged against the moved Giant (a tick that begins 1,269 apart meets
    a Giant 40 nearer): the overlapping and the apart ticks are classified on the last field."""
    b = royalesim.Battle(["ElectroGiant", "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(
        0,
        [[1] * 8, [0] * 8],
        0,
        200,
        [10_000, 10_000],
        None,
        [(1, 1, KNIGHT_AT[0] * SUB, KNIGHT_AT[1] * SUB, -1), (0, 0, GIANT_AT[0] * SUB, GIANT_AT[1] * SUB, -1)],
    )
    rows, prev = [], None
    for _ in range(ticks):
        s = json.loads(b.state_json())
        eg = next((e for e in s["entities"] if e[F["team"]] == 0 and e[F["tower_slot"]] < 0), None)
        kn = next((e for e in s["entities"] if e[F["team"]] == 1 and e[F["tower_slot"]] < 0), None)
        if eg is None or kn is None:
            break
        if prev is not None:
            pe, pk = prev
            rows.append(
                (
                    kn[F["stun_ticks"]] > 0 or pk[F["stun_ticks"]] > 0,
                    math.hypot(pk[F["x"]] - pe[F["x"]], pk[F["y"]] - pe[F["y"]]) / SUB,
                    math.hypot(kn[F["x"]] - pk[F["x"]], kn[F["y"]] - pk[F["y"]]) / SUB,
                    math.hypot(eg[F["x"]] - pe[F["x"]], eg[F["y"]] - pe[F["y"]]) / SUB,
                    math.hypot(pk[F["x"]] - eg[F["x"]], pk[F["y"]] - eg[F["y"]]) / SUB,
                )
            )
        prev = (eg, kn)
        b.step([], 1)
    return rows


def test_a_stunned_knight_is_moved_by_the_giant_walking_through_it():
    rows = stun_scene(NEW_ARM)
    over = [(i, round(r[2], 1)) for i, r in enumerate(rows) if r[0] and r[4] <= KNIGHT_GIANT_RADII - OVERLAP]
    assert len(over) >= 5, f"the scene drifted: only {len(over)} stunned ticks began with the Giant overlapping"
    still = [(i, s) for i, s in over if s == 0]
    assert still == [], f"{NEW_ARM}: the stunned Knight stood on overlapping ticks {still} (of {over})"


def test_the_stun_scene_old_arm_is_the_pre_flip_engine():
    rows = stun_scene(OLD_ARM)
    over = [i for i, r in enumerate(rows) if r[0] and r[4] <= KNIGHT_GIANT_RADII - OVERLAP]
    assert len(over) >= 5, f"the scene drifted: only {len(over)} stunned ticks began with the Giant overlapping"
    moved = [i for i in over if rows[i][2] > 0]
    assert moved == [], f"{OLD_ARM}: the stunned Knight moved on {moved}"
    assert max(r[3] for r in rows) > 100, "the scene drifted: the Giant was never thrown back when a stun ended"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_stunned_knight_with_nothing_overlapping_stands(arm):
    rows = stun_scene(arm)
    apart = [i for i, r in enumerate(rows) if r[0] and min(r[1], r[4]) >= KNIGHT_GIANT_RADII]
    assert len(apart) >= 5, f"the scene drifted: only {len(apart)} stunned ticks began apart"
    moved = [(i, round(rows[i][2], 1)) for i in apart if rows[i][2] > 0]
    assert moved == [], f"{arm}: the stunned Knight moved with nothing overlapping it: {moved}"


def test_the_shipped_value_is_the_new_arm():
    entry_ = json.loads(LEDGER.read_text(encoding="utf-8"))["collision"]["HELD_UNIT_CONTACT"]
    assert entry_["value"] == NEW_ARM
    assert entry_["candidates"] == [OLD_ARM, NEW_ARM]
