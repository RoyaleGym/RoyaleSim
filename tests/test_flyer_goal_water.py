"""A chasing flyer does not demote water cells when it picks its goal cell (pathfinding.FLYER_GOAL_WATER).

WHAT THIS PINS. A flyer chasing a target that is out of reach flies straight at one goal cell: among the 500 x 500
cells whose centre lies within Range + its own CollisionRadius of the target's centre, the one whose centre is nearest
the flyer's start-of-tick position. On client 15.535.29 a river cell is as good a goal as a dry one for a flyer.
Measured on client 15.535.29 over 458 walk ticks of Minions and a Mega Minion chasing a Knight near the river (the five
flyer runs of the client 15.535.29 goal-cell scenario), the flyer's heading points at that cell on 458 of 458. The goal
is a river cell on 280 of those ticks. With water demoted below every dry cell, as today's engine does for every
mover, the same cells score 181 of 458, and 3 of the 280 river-goal ticks. Today's engine sends a flyer to the nearest
DRY cell in reach, so it flies a different line whenever the river is inside the reach circle.

WHY THE CONTROLS ARE HERE. A GROUND chaser still demotes water on both arms: a blue Knight chasing a red Knight across
the river keeps a dry goal cell while the nearest in-reach cell is in the water. An implementation that drops the water
test for every mover fails it. Every flyer here is created before its target, so the target's start-of-tick position is
also its position at the flyer's turn in the creation-order move pass. The separate timing law
(pathfinding.GOAL_TARGET_POSITION) reads the same position on either of its arms, so it cannot move these results.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "pathfinding.FLYER_GOAL_WATER"
NEW_ARM, OLD_ARM = "not_demoted", "demoted"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CELL = 500
#: Range + own CollisionRadius (the 15.535 card table) and the CollisionRadius the engine must carry
REACH = {"MegaMinion": 1600 + 600, "Minions": 2500 + 500, "Knight": 1200 + 500}
RADIUS = {"MegaMinion": 600, "Minions": 500, "Knight": 500}
#: movement direction is 256 x the unit vector; the client check allows 2.5 per component
TOL = 2.5
#: the client 15.535.29 goal-cell scenario's geometry: a blue flyer at (10500, 14500), created first, chases a red
#: Knight that starts at (14500, 17499) on the red bank and walks south toward the bridge
FLYER_AT, KNIGHT_AT = (10500, 14500), (14500, 17499)
GROUND_CHASER_AT = (10500, 13500)
#: a Minions card is spawned as the three-member formation the played card lays out
FORMATION = {"MegaMinion": [(0, 0)], "Minions": [(0, 579), (499, -288), (-499, -288)], "Knight": [(0, 0)]}
FLYERS = ["MegaMinion", "Minions"]
#: fewest scored ticks and fewest ticks where the two water rules pick different cells, before a scene counts
MIN_TICKS, MIN_SPLIT = 20, 3


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def water_cells(b) -> set:
    """The river: the cells a ground unit cannot stand on (the only impassable terrain on this arena)."""
    grid = b.passable_half_cells()
    cells = {(c, r) for r, row in enumerate(grid) for c, ok in enumerate(row) if not ok}
    assert cells, "no impassable cell: the arena has no river"
    assert all(30 <= r <= 33 for _, r in cells), f"the impassable cells are not the river rows: {cells}"
    return cells


def goal_cell(actor, target, reach, water, demote_water):
    """The 500-grid goal scan: rows ascending, columns ascending when the actor is on the left half; the highest
    category (2 dry, 1 water when water is demoted), then the strictly smallest squared distance to the actor."""
    tc, tr, rr = target[0] // CELL, target[1] // CELL, reach // CELL
    cols = range(max(0, tc - rr - 1), min(35, tc + rr + 1) + 1)
    best, best_key = None, None
    for r in range(max(0, tr - rr - 1), min(63, tr + rr + 1) + 1):
        for c in (cols if actor[0] < 18 * CELL else reversed(cols)):
            cx, cy = c * CELL + CELL // 2, r * CELL + CELL // 2
            if (cx - target[0]) ** 2 + (cy - target[1]) ** 2 > reach * reach:
                continue
            key = (1 if demote_water and (c, r) in water else 2, -((cx - actor[0]) ** 2 + (cy - actor[1]) ** 2))
            if best_key is None or key > best_key:
                best, best_key = (c, r), key
    return best


def heading_to(actor, cell):
    dx, dy = cell[0] * CELL + CELL // 2 - actor[0], cell[1] * CELL + CELL // 2 - actor[1]
    n = math.hypot(dx, dy)
    return 256 * dx / n, 256 * dy / n


def creation_rank(b) -> dict:
    """uid -> the entity's place in the creation order (the order of the move pass)."""
    ents = json.loads(b.save())["ents"]
    teams = {"Blue": 0, "Red": 1}
    return {ents["team_seq"][i] * 2 + teams[t]: ents["creation_seq"][i]
            for i, t in enumerate(ents["team"]) if ents["alive"][i]}


def pos(e):
    return e[F["x"]] // SUB, e[F["y"]] // SUB


def scene(card, chaser_at, arm):
    """Blue `card` chasers (FORMATION) and a red Knight, all spawned on the reset. Returns the battle, the chasers'
    uids, the Knight's uid and the river cells, and asserts every chaser was created before the Knight."""
    b = royalesim.Battle(["Knight", "MegaMinion" if card == "Knight" else card], [[0, 1, 2], [0, 1, 2]],
                         calibration_overrides=overrides(arm))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None,
            [(1, 0, KNIGHT_AT[0] * SUB, KNIGHT_AT[1] * SUB, -1)]
            + [(0, 0 if card == "Knight" else 1, (chaser_at[0] + dx) * SUB, (chaser_at[1] + dy) * SUB, -1)
               for dx, dy in FORMATION[card]])
    units = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0}
    chasers = sorted(u for u, e in units.items() if e[F["team"]] == 0)
    knight = next(u for u, e in units.items() if e[F["team"]] == 1)
    assert len(chasers) == len(FORMATION[card]), f"expected {len(FORMATION[card])} {card}, got {chasers}"
    rank = creation_rank(b)
    for u in chasers:
        assert units[u][F["radius"]] == RADIUS[card] * SUB, f"{card} radius {units[u][F['radius']] / SUB}"
        assert rank[u] < rank[knight], "every chaser must be created before the Knight (the timing law held fixed)"
    return b, chasers, knight, water_cells(b)


def flyer_chase(card, arm, ticks=120):
    """One row per walk tick that follows a walk tick, pooled over the flyers: (tick, uid, the flyer's start-of-tick
    position, its facing, the Knight's start-of-tick position)."""
    b, flyers, knight, water = scene(card, FLYER_AT, arm)
    frames = [{e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}]
    for u in flyers:
        assert frames[0][u][F["flying"]], f"{card} is not flying"
    for _ in range(ticks):
        b.step([], 1)
        frames.append({e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]})
    rows = []
    for u in flyers:
        for t in range(2, len(frames)):
            c2, c1, c, k1 = frames[t - 2].get(u), frames[t - 1].get(u), frames[t].get(u), frames[t - 1].get(knight)
            if not (c2 and c1 and c and k1):
                continue
            walking = all(e[F["target_uid"]] == knight and e[F["attack_phase"]] == 0 for e in (c1, c))
            if walking and pos(c2) != pos(c1) != pos(c):
                rows.append((t, u, pos(c1), tuple(c[F["facing"]]), pos(k1)))
    return rows, water


def score(rows, reach, water, demote_water):
    """The ticks whose facing is NOT within TOL of the cell chosen under `demote_water`, and the split ticks: those
    where the two water rules' headings differ by more than 2 x TOL in a component, so a facing within TOL of one is
    outside TOL of the other."""
    misses, split = [], []
    for t, u, actor, facing, knight in rows:
        cells = {d: goal_cell(actor, knight, reach, water, d) for d in (False, True)}
        heads = {d: heading_to(actor, cell) for d, cell in cells.items()}
        h = heads[demote_water]
        if abs(facing[0] - h[0]) > TOL or abs(facing[1] - h[1]) > TOL:
            misses.append((t, u, facing, tuple(round(v, 1) for v in h), cells[demote_water]))
        if max(abs(a - b) for a, b in zip(heads[False], heads[True], strict=True)) > 2 * TOL:
            assert cells[False] in water, f"tick {t}: the rules split on a dry cell {cells}"
            split.append((t, u))
    return misses, split


def assert_scene(rows, reach, water):
    """Guards: the chase happened and the nearest in-reach cell was a river cell on enough ticks."""
    assert len(rows) >= MIN_TICKS, f"only {len(rows)} walk ticks: the chase did not happen"
    _, split = score(rows, reach, water, True)
    assert len(split) >= MIN_SPLIT, f"the two water rules pick different cells on only {len(split)} ticks: {split}"
    return split


@pytest.mark.parametrize("card", FLYERS)
def test_flyer_heads_for_the_nearest_in_reach_cell_water_or_not(card):
    rows, water = flyer_chase(card, NEW_ARM)
    split = assert_scene(rows, REACH[card], water)
    misses, _ = score(rows, REACH[card], water, False)
    assert misses == [], (
        f"{len(misses)} of {len(rows)} walk ticks do not head for the nearest in-reach cell with water not demoted "
        f"(the rules split on (tick, uid) {split}); (tick, uid, facing, wanted, cell): {misses}")


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_ground_chaser_still_demotes_water(arm):
    """Control: a blue Knight chases a red Knight across the river. Its goal cell (the route's first element, goal
    first) is the dry one on every tick, including the ticks where the nearest in-reach cell is in the river."""
    b, (chaser,), knight, water = scene("Knight", GROUND_CHASER_AT, arm)
    prev = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
    checked, split, wrong = 0, [], []
    for t in range(1, 60):
        b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        route = {u[0]: u[6] for u in b.debug_units()}.get(chaser)
        if route and chaser in now and now[chaser][F["target_uid"]] == knight and knight in prev:
            actor, target = pos(prev[chaser]), pos(prev[knight])
            dry = goal_cell(actor, target, REACH["Knight"], water, True)
            nearest = goal_cell(actor, target, REACH["Knight"], water, False)
            checked += 1
            if nearest != dry:
                split.append(t)
            if tuple(route[0]) != dry:
                wrong.append((t, tuple(route[0]), dry, nearest))
        prev = now
    assert checked >= MIN_TICKS, f"only {checked} ticks with a route: the chase did not happen"
    assert len(split) >= MIN_SPLIT, f"the nearest in-reach cell was a river cell on only {len(split)} ticks: {split}"
    assert wrong == [], f"the ground goal left the dry cell (tick, goal, dry, nearest): {wrong}"


@pytest.mark.parametrize("card", FLYERS)
def test_old_arm_is_todays_engine(card):
    rows, water = flyer_chase(card, OLD_ARM)
    split = assert_scene(rows, REACH[card], water)
    misses, _ = score(rows, REACH[card], water, True)
    assert misses == [], f"old arm: ticks off the water-demoted reading: {misses}"
    open_misses, _ = score(rows, REACH[card], water, False)
    assert set(split) <= {m[:2] for m in open_misses}, f"old arm: split ticks {split}, open-rule misses {open_misses}"
