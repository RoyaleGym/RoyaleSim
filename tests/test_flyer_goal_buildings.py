"""A flying chaser ranks a cell inside a building's box like a free one (pathfinding.FLYER_GOAL_BUILDINGS).

WHAT THIS PINS. A chaser out of reach picks one goal cell: among the 500 x 500 cells whose centre lies within its
reach of the target's centre, the one nearest its start-of-tick position (path16402.rs choose_goal_cell). Today every
chaser of a ground target ranks a cell inside a building's box (every tower and building, occlusion_box) below the
free ones. On the client 16.402 corpus a flyer does not. The Inferno Dragon of 20260920-082459, placed at (8500, 500)
behind its own king, chases a Giant at (12093, 4634). It flies at cell (20, 3), inside the blue king's box, with the
step (48, 34), and stands at (9460, 1180). Today's engine sends it to (21, 3), with the step (52, 29), and stands it at
(9540, 1080). A Lava Pup of 20260920-071744 chased a Goblin to three cells inside a princess tower's box. Of the
corpus's 291 fresh flyer goal choices, 4 separate the two readings, and all 4 fit not_demoted. Ground chasers keep the
demotion: 23 of 23 fresh choices that separate the readings (12 battles).

The scenes: the 082459 Dragon, with targeting.VARIABLE_DAMAGE_WALK_REACH at its measured arm (a walking Inferno
Dragon's reach is Range alone, as in the client), and a Mega Minion behind the blue king chasing a Giant at the
king's front. Every chaser is created before the Giant, so the Giant's start-of-tick position is also its position
at the chaser's turn; pathfinding.GOAL_TARGET_POSITION reads the same point on either arm.

WHY THE CONTROLS ARE HERE. An engine that drops the demotion for every chaser passes both flyer tests, so a Knight in
the Mega Minion's place must keep a free goal cell under both arms. And the old arm must be today's engine.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then
run this file: the named tests go red and the rest stay green.
  * `flyer_goal_boxes_demoted` -- the new arm still demotes a boxed cell for a flyer:
    test_the_082459_dragon_flies_at_a_cell_inside_the_kings_box, test_a_mega_minion_behind_its_king_flies_into_the_box.
"""

from __future__ import annotations

import json
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "pathfinding.FLYER_GOAL_BUILDINGS"
NEW_ARM, OLD_ARM = "not_demoted", "demoted"
REACH_KEY, REACH_ARM = "targeting.VARIABLE_DAMAGE_WALK_REACH", "client16402_no_own_radius_walking"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
LEDGER = pathlib.Path(__file__).resolve().parent.parent / "data" / "calibration.json"
CELL = 500
#: on 082459 side 0's right princess tower and side 1's left one are down when the Dragon is placed
TOWER_HP = [[4824, 3052, 0], [4824, 0, 3052]]
BEHIND_KING = (8500, 500)
#: the 082459 Giant where the Dragon first reads it, and a Giant at the blue king's front (it stands and hits the king)
GIANT_082459, KING_FRONT = (12093, 4634), (8500, 5250)
#: the reach a walking chaser's goal scan uses: the Dragon's Range alone (REACH_ARM), else Range + own CollisionRadius
REACH = {"InfernoDragon": 3500, "MegaMinion": 1600 + 600, "Knight": 1200 + 500}
RADIUS = {"InfernoDragon": 500, "MegaMinion": 600, "Knight": 500}
#: the client's first goal cell, first step and stand for the 082459 Dragon, and today's engine's
CLIENT_082459 = {"cell": (20, 3), "step": (48, 34), "stand": (9460, 1180)}
ENGINE_082459 = {"cell": (21, 3), "step": (52, 29), "stand": (9540, 1080)}
#: fewest walk ticks with a route, and fewest where the two readings name different cells, before a scene counts
MIN_TICKS, MIN_SPLIT = 10, 5


def overrides(arm, dragon=False) -> dict:
    """The battle's calibration pinning `arm` BY NAME (None: the build's own value), and for the Dragon the measured
    walking reach."""
    out = {} if arm is None else {KEY: json.dumps(arm)}
    if dragon:
        out[REACH_KEY] = json.dumps(REACH_ARM)
    return out


def occlusion_box(x, y, r):
    """path16402.rs occlusion_box: the centre snapped up to 500, the half-open box [c - r, c + r), inclusive cells;
    a box that does not fit inside the 36 x 64 grid marks nothing."""
    c500, r500 = (x - 1) // CELL * CELL + CELL, (y - 1) // CELL * CELL + CELL
    if c500 - r < 0 or r500 - r < 0 or c500 + r >= 36 * CELL or r500 + r >= 64 * CELL:
        return set()
    return {(c, rr) for c in range((c500 - r) // CELL, (c500 + r - 1) // CELL + 1)
            for rr in range((r500 - r) // CELL, (r500 + r - 1) // CELL + 1)}


def boxed_cells(entities) -> set:
    """Every live tower's and building's box, from the engine's own positions and radii."""
    out = set()
    for e in entities:
        if e[F["tower_slot"]] >= 0 or e[F["footprint"]] is not None:
            out |= occlusion_box(e[F["x"]] // SUB, e[F["y"]] // SUB, e[F["radius"]] // SUB)
    return out


def goal_cell(actor, target, reach, boxed, water, demote_boxed, demote_water):
    """The 500-grid goal scan: rows ascending, columns ascending when the actor is on the left half; the highest
    category (2 free, 1 boxed or water where demoted), then the strictly smallest squared distance to the actor."""
    tc, tr, rr = target[0] // CELL, target[1] // CELL, reach // CELL
    cols = range(max(0, tc - rr - 1), min(35, tc + rr + 1) + 1)
    best, best_key = None, None
    for r in range(max(0, tr - rr - 1), min(63, tr + rr + 1) + 1):
        for c in (cols if actor[0] < 18 * CELL else reversed(cols)):
            cx, cy = c * CELL + CELL // 2, r * CELL + CELL // 2
            if (cx - target[0]) ** 2 + (cy - target[1]) ** 2 > reach * reach:
                continue
            low = (demote_boxed and (c, r) in boxed) or (demote_water and (c, r) in water)
            key = (1 if low else 2, -((cx - actor[0]) ** 2 + (cy - actor[1]) ** 2))
            if best_key is None or key > best_key:
                best, best_key = (c, r), key
    return best


def pos(e):
    return e[F["x"]] // SUB, e[F["y"]] // SUB


def creation_rank(b) -> dict:
    """uid -> the entity's place in the creation order (the order of the move pass)."""
    ents = json.loads(b.save())["ents"]
    teams = {"Blue": 0, "Red": 1}
    return {ents["team_seq"][i] * 2 + teams[t]: ents["creation_seq"][i]
            for i, t in enumerate(ents["team"]) if ents["alive"][i]}


def chase(card, chaser_at, giant_at, arm, ticks=40):
    """A blue `card` and a red Giant spawned on the reset, the 082459 towers standing. One row per tick on which the
    chaser targets the Giant and holds a route: (tick, its route's goal cell, the cell the scan names with boxed cells
    demoted, and not demoted, from the start-of-tick positions). Also the chaser's position per tick, its first step,
    and where it first stood to attack. Water is demoted for a ground chaser only (pathfinding.FLYER_GOAL_WATER)."""
    dragon = card == "InfernoDragon"
    b = royalesim.Battle([card, "Giant"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm, dragon))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], TOWER_HP,
            [(0, 0, chaser_at[0] * SUB, chaser_at[1] * SUB, -1), (1, 1, giant_at[0] * SUB, giant_at[1] * SUB, -1)])
    grid = b.passable_half_cells()
    water = {(c, r) for r, row in enumerate(grid) for c, ok in enumerate(row) if not ok}
    ents = json.loads(b.state_json())["entities"]
    units = {e[F["uid"]]: e for e in ents if e[F["tower_slot"]] < 0}
    chaser = next(u for u, e in units.items() if e[F["team"]] == 0)
    giant = next(u for u, e in units.items() if e[F["team"]] == 1)
    assert units[chaser][F["radius"]] == RADIUS[card] * SUB, f"{card} radius {units[chaser][F['radius']] / SUB}"
    assert bool(units[chaser][F["flying"]]) == (card != "Knight"), f"{card} flying {units[chaser][F['flying']]}"
    rank = creation_rank(b)
    assert rank[chaser] < rank[giant], "the chaser must be created before the Giant (the timing law held fixed)"
    boxed = boxed_cells(ents)
    prev = {e[F["uid"]]: e for e in ents}
    rows, track, stand = [], [pos(units[chaser])], None
    for _ in range(ticks):
        b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        if chaser not in now:
            break
        track.append(pos(now[chaser]))
        if stand is None and now[chaser][F["attack_phase"]] != 0:
            stand = pos(now[chaser])
        route = {u[0]: u[6] for u in b.debug_units()}.get(chaser)
        if route and now[chaser][F["target_uid"]] == giant and giant in prev:
            actor, target = pos(prev[chaser]), pos(prev[giant])
            cells = {d: goal_cell(actor, target, REACH[card], boxed, water, d, card == "Knight") for d in (True, False)}
            rows.append((json.loads(b.state_json())["tick"], tuple(route[0]), cells[True], cells[False]))
        prev = now
    first_step = (track[1][0] - track[0][0], track[1][1] - track[0][1])
    return rows, boxed, first_step, stand


def assert_scene(rows):
    assert len(rows) >= MIN_TICKS, f"only {len(rows)} ticks with a route: the chase did not happen"
    split = [r[0] for r in rows if r[2] != r[3]]
    assert len(split) >= MIN_SPLIT, f"the two readings name different cells on only {len(split)} ticks: {split}"


def off(rows, demoted):
    """The ticks whose route goal is not the cell the scan names under `demoted`."""
    return [r for r in rows if r[1] != (r[2] if demoted else r[3])]


@pytest.mark.parametrize("arm", [NEW_ARM])
def test_the_082459_dragon_flies_at_a_cell_inside_the_kings_box(arm):
    rows, boxed, first_step, stand = chase("InfernoDragon", BEHIND_KING, GIANT_082459, arm)
    assert_scene(rows)
    got = {"cell": rows[0][1], "step": first_step, "stand": stand}
    assert got == CLIENT_082459, f"new arm: {got}, the client's {CLIENT_082459} (today's engine {ENGINE_082459})"
    assert rows[0][1] in boxed, f"the client's cell {rows[0][1]} is not inside a box: the scene tests nothing"
    wrong = off(rows, demoted=False)
    assert wrong == [], f"new arm: goal cells off the not-demoted scan (tick, goal, demoted, not demoted): {wrong}"


@pytest.mark.parametrize("arm", [NEW_ARM])
def test_a_mega_minion_behind_its_king_flies_into_the_box(arm):
    rows, boxed, _, _ = chase("MegaMinion", BEHIND_KING, KING_FRONT, arm)
    assert_scene(rows)
    assert rows[0][3] in boxed, f"the first not-demoted cell {rows[0][3]} is free: the scene tests nothing"
    wrong = off(rows, demoted=False)
    assert wrong == [], f"new arm: goal cells off the not-demoted scan (tick, goal, demoted, not demoted): {wrong}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_ground_chaser_keeps_a_free_goal_cell(arm):
    """Control: a Knight in the Mega Minion's place plans to a cell outside every box under both arms, on the ticks
    where the nearest cell in reach is inside the blue king's box."""
    rows, boxed, _, _ = chase("Knight", BEHIND_KING, KING_FRONT, arm)
    assert_scene(rows)
    assert rows[0][3] in boxed, f"the first not-demoted cell {rows[0][3]} is free: the scene tests nothing"
    assert rows[0][2] not in boxed, f"the first demoted cell {rows[0][2]} is boxed: the scan did not demote it"
    wrong = off(rows, demoted=True)
    assert wrong == [], f"{arm}: the Knight's goal left the free cell (tick, goal, demoted, not demoted): {wrong}"


@pytest.mark.parametrize("arm", [OLD_ARM])
def test_the_old_arm_demotes_boxed_cells_for_a_flyer(arm):
    rows, _, first_step, stand = chase("InfernoDragon", BEHIND_KING, GIANT_082459, arm)
    assert_scene(rows)
    got = {"cell": rows[0][1], "step": first_step, "stand": stand}
    assert got == ENGINE_082459, f"old arm: {got}, today's engine {ENGINE_082459}"
    assert off(rows, demoted=True) == [], "old arm: a Dragon goal off the demoted scan"
    rows, _, _, _ = chase("MegaMinion", BEHIND_KING, KING_FRONT, arm)
    assert_scene(rows)
    assert off(rows, demoted=True) == [], "old arm: a Mega Minion goal off the demoted scan"


def test_the_shipped_value_is_the_new_arm():
    entry = json.loads(LEDGER.read_text(encoding="utf-8"))["pathfinding"]["FLYER_GOAL_BUILDINGS"]
    assert entry["value"] == NEW_ARM
    assert entry["candidates"] == [OLD_ARM, NEW_ARM]
