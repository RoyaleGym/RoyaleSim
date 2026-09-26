"""A chasing unit reads its target's position as the creation-order move pass holds it
(pathfinding.GOAL_TARGET_POSITION).

WHAT THIS PINS. A flyer chasing a target that is out of reach flies straight at one goal cell: among the 500 x 500
cells whose centre lies within Range + its own CollisionRadius of the target's centre, the one whose centre is nearest
the flyer's start-of-tick position. On client 15.535.29 the target's centre in that test is the one the target holds
at the flyer's turn in the creation-order move pass: its position AFTER this tick's move when the target was created
before the flyer, BEFORE it when the target was created after. Measured on client 15.535.29 over 458 walk ticks of
Minions and a Mega Minion chasing a Knight (the five flyer runs of the client 15.535.29 goal-cell scenario, both
creation orders), the flyer's heading points at that cell on 458 of 458. The start-of-tick reading scores 419 of 458,
and all 39 misses are ticks of a flyer created after the Knight. The same holds for ground chasers on client 15.535.29:
in the melee-group scenarios (Barbarians, Skeletons, Goblins and others against a Knight) the as-held reading fits 71
of 71 discriminating ticks of units chasing a Knight created first, and 16 of 16 of the Knight chasing units created
after it. The start_of_tick arm reads the start-of-tick position for every chaser, so a chaser created after its
target turns one tick late when a cell crosses the reach circle.

WHICH ARM. creation_order is the SHIPPED arm since the 2026-09-26 flip. start_of_tick is the engine before the flip.
The tests below pin each arm BY NAME through the battle's calibration, never through the shipped value, and the last
one runs the shipped build with no override at all.

WHY THE CONTROLS ARE HERE. A flyer created BEFORE its target keeps the start-of-tick reading on both arms. On client
15.535.29 that order fits the unmoved target on 174 of 174 ticks, and the moved target on 151 of 174. An
implementation that always reads the moved target fails this control. A GROUND chaser (a Knight created after a Giant)
is tested too, so an implementation that changes only flyers fails. Every scene sits far from the river, and a guard
asserts that no scored tick depends on whether water cells are demoted. So the separate water law
(pathfinding.FLYER_GOAL_WATER) cannot move these results.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "pathfinding.GOAL_TARGET_POSITION"
SHIPPED_ARM, START_ARM = "creation_order", "start_of_tick"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CELL = 500
#: Range + own CollisionRadius (the 15.535 card table) and the CollisionRadius the engine must carry
REACH = {"MegaMinion": 1600 + 600, "Minions": 2500 + 500, "Knight": 1200 + 500}
RADIUS = {"MegaMinion": 600, "Minions": 500, "Knight": 500}
#: movement direction is 256 x the unit vector; the client check allows 2.5 per component
TOL = 2.5
#: As on the client, a BLUE flyer chases a RED Knight; only the creation order differs. The red Knight stands at
#: (8500, 12000) on the blue half and walks south-west to the blue left princess tower, so its reach circle never
#: touches the river. AFTER: the Knight is spawned on the reset and the blue card is PLAYED at (5500, 14000) on the
#: first tick, so the Knight is created first. BEFORE: both are spawned on the reset, and the engine creates blue
#: first; a Minions card is spawned as the same three-member formation the played card lays out.
KNIGHT_AT, FLYER_AT = (8500, 12000), (5500, 14000)
FORMATION = {"MegaMinion": [(0, 0)], "Minions": [(0, 579), (499, -288), (-499, -288)]}
FLYERS = ["MegaMinion", "Minions"]
#: fewest scored ticks and fewest ticks where the two readings pick different cells, before a scene counts
MIN_TICKS, MIN_SPLIT = 20, 3


def overrides(arm) -> dict:
    """The battle's calibration pinning `arm` BY NAME; None runs the shipped build with no override."""
    return {} if arm is None else {KEY: json.dumps(arm)}


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


def flyer_chase(card, order, arm, ticks=200):
    """Blue `card` flyers chase the red Knight; `order` is "after" (the Knight created first) or "before". Returns
    (rows, created_first, water): one row per walk tick that follows a walk tick, pooled over the flyers, (tick, uid,
    the flyer's start-of-tick position, its facing, the Knight's position before and after this tick's move);
    created_first names which the engine created first, the Knight ("target") or every flyer ("flyer")."""
    b = royalesim.Battle(["Knight", card], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    knight_spawn = (1, 0, KNIGHT_AT[0] * SUB, KNIGHT_AT[1] * SUB, -1)
    if order == "after":
        b.reset(0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, [knight_spawn])
        b.step([(0, 0, FLYER_AT[0] * SUB, FLYER_AT[1] * SUB)], 1)
    else:
        b.reset(0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None,
                [knight_spawn] + [(0, 1, (FLYER_AT[0] + dx) * SUB, (FLYER_AT[1] + dy) * SUB, -1)
                                  for dx, dy in FORMATION[card]])
    water = water_cells(b)
    frames = [{e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}]
    units = {u: e for u, e in frames[0].items() if e[F["tower_slot"]] < 0}
    flyers = sorted(u for u, e in units.items() if e[F["team"]] == 0)
    knight = next(u for u, e in units.items() if e[F["team"]] == 1)
    assert len(flyers) == len(FORMATION[card]), f"expected {len(FORMATION[card])} {card}, got {flyers}"
    for u in flyers:
        assert units[u][F["flying"]], f"{card} is not flying"
        assert units[u][F["radius"]] == RADIUS[card] * SUB, f"{card} radius {units[u][F['radius']] / SUB}"
    rank = creation_rank(b)
    firsts = {"target" if rank[knight] < rank[u] else "flyer" for u in flyers}
    assert len(firsts) == 1, f"the Knight was created between the flyers: {rank}"
    for _ in range(ticks):
        b.step([], 1)
        frames.append({e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]})
    rows = []
    for u in flyers:
        for t in range(2, len(frames)):
            c2, c1, c, k1, k = (frames[t - 2].get(u), frames[t - 1].get(u), frames[t].get(u),
                                frames[t - 1].get(knight), frames[t].get(knight))
            if not (c2 and c1 and c and k1 and k):
                continue
            walking = all(e[F["target_uid"]] == knight and e[F["attack_phase"]] == 0 and e[F["deploy_ticks"]] == 0
                          for e in (c1, c))
            if walking and pos(c2) != pos(c1) != pos(c):
                rows.append((t, u, pos(c1), tuple(c[F["facing"]]), pos(k1), pos(k)))
    return rows, firsts.pop(), water


def score(rows, reach, water, reading, demote_water=True):
    """The ticks whose facing is NOT within TOL of the cell chosen with the Knight's `reading` position ("before" or
    "after" its move), and the split ticks: those where the two readings' headings differ by more than 2 x TOL in a
    component, so a facing within TOL of one is outside TOL of the other."""
    misses, split = [], []
    for t, u, actor, facing, k_before, k_after in rows:
        knight = {"before": k_before, "after": k_after}
        cells = {r: goal_cell(actor, k, reach, water, demote_water) for r, k in knight.items()}
        heads = {r: heading_to(actor, cell) for r, cell in cells.items()}
        h = heads[reading]
        if abs(facing[0] - h[0]) > TOL or abs(facing[1] - h[1]) > TOL:
            misses.append((t, u, facing, tuple(round(v, 1) for v in h), cells[reading]))
        if max(abs(a - b) for a, b in zip(heads["before"], heads["after"], strict=True)) > 2 * TOL:
            split.append((t, u))
    return misses, split


def assert_scene(rows, reach, water, created_first, want_first):
    """Guards: the chase happened, the two readings disagree on enough ticks, and water plays no part."""
    assert created_first == want_first, f"the scene needs the {want_first} created first, got the {created_first}"
    assert len(rows) >= MIN_TICKS, f"only {len(rows)} walk ticks: the chase did not happen"
    _, split = score(rows, reach, water, "before")
    assert len(split) >= MIN_SPLIT, f"the two readings pick different cells on only {len(split)} ticks: {split}"
    for _, _, actor, _, k_before, k_after in rows:
        for k in (k_before, k_after):
            assert goal_cell(actor, k, reach, water, True) == goal_cell(actor, k, reach, water, False), (
                f"a scored tick depends on the water rule (flyer {actor}, Knight {k})")
    return split


@pytest.mark.parametrize("card", FLYERS)
def test_flyer_created_after_its_target_reads_the_moved_target(card):
    rows, first, water = flyer_chase(card, "after", SHIPPED_ARM)
    split = assert_scene(rows, REACH[card], water, first, "target")
    misses, _ = score(rows, REACH[card], water, "after")
    assert misses == [], (
        f"{len(misses)} of {len(rows)} walk ticks do not head for the cell chosen from the Knight's moved position "
        f"(the readings split on (tick, uid) {split}); (tick, uid, facing, wanted, cell): {misses}")


@pytest.mark.parametrize("arm", [SHIPPED_ARM, START_ARM])
@pytest.mark.parametrize("card", FLYERS)
def test_flyer_created_before_its_target_reads_the_unmoved_target(card, arm):
    """Control: the target moves after the flyer in the pass, so its start-of-tick position is the one at the
    flyer's turn. An implementation that always reads the moved target fails here."""
    rows, first, water = flyer_chase(card, "before", arm)
    split = assert_scene(rows, REACH[card], water, first, "flyer")
    misses, _ = score(rows, REACH[card], water, "before")
    assert misses == [], (
        f"{len(misses)} of {len(rows)} walk ticks do not head for the cell chosen from the Knight's unmoved position "
        f"(the readings split on (tick, uid) {split}); (tick, uid, facing, wanted, cell): {misses}")


@pytest.mark.parametrize("arm", [SHIPPED_ARM, START_ARM])
def test_ground_chaser_created_after_its_target_reads_the_moved_target(arm):
    """A red Knight created after a blue Giant chases it north on the red half. Its goal cell (the route's first
    element, goal first) is the one chosen from the Giant's MOVED position under the shipped arm (the Giant was created
    first), and from its start-of-tick position under the start_of_tick arm, on every tick, including the ticks
    where the two readings pick different cells."""
    b = royalesim.Battle(["Giant", "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None,
            [(0, 0, 14500 * SUB, 20500 * SUB, -1), (1, 1, 9500 * SUB, 19000 * SUB, -1)])
    water = water_cells(b)
    prev = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
    giant, knight = (next(u for u, e in prev.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == t) for t in (0, 1))
    rank = creation_rank(b)
    assert rank[giant] < rank[knight], "the Giant must be created before the Knight"
    checked, split, wrong = 0, [], []
    for t in range(1, 80):
        b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        route = {u[0]: u[6] for u in b.debug_units()}.get(knight)
        if route and now[knight][F["target_uid"]] == giant and giant in prev:
            actor = pos(prev[knight])
            before = goal_cell(actor, pos(prev[giant]), REACH["Knight"], water, True)
            after = goal_cell(actor, pos(now[giant]), REACH["Knight"], water, True)
            checked += 1
            if before != after:
                split.append(t)  # a route's goal is a cell, so any change of cell discriminates
            want = after if arm == SHIPPED_ARM else before
            if tuple(route[0]) != want:
                wrong.append((t, tuple(route[0]), before, after))
        prev = now
    assert checked >= MIN_TICKS, f"only {checked} ticks with a route: the chase did not happen"
    assert len(split) >= MIN_SPLIT, f"the two readings pick different cells on only {len(split)} ticks: {split}"
    reading = "moved" if arm == SHIPPED_ARM else "start-of-tick"
    assert wrong == [], f"the ground goal left the {reading} reading (tick, goal, unmoved, moved): {wrong}"


@pytest.mark.parametrize("card", FLYERS)
def test_the_start_of_tick_arm_reads_the_unmoved_target(card):
    rows, first, water = flyer_chase(card, "after", START_ARM)
    split = assert_scene(rows, REACH[card], water, first, "target")
    misses, _ = score(rows, REACH[card], water, "before")
    assert misses == [], f"start_of_tick arm: ticks off the start-of-tick reading: {misses}"
    moved, _ = score(rows, REACH[card], water, "after")
    assert set(split) <= {m[:2] for m in moved}, f"start_of_tick arm: split ticks {split}, moved-reading misses {moved}"


def test_the_shipped_build_reads_the_moved_target():
    """No override at all. The compiled-in ledger ships SHIPPED_ARM, so the shipped build behaves as the client."""
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    shipped = ledger["pathfinding"]["GOAL_TARGET_POSITION"]["value"]
    assert shipped == SHIPPED_ARM, f"{KEY} ships {shipped!r}, not the arm this file names as shipped"
    rows, first, water = flyer_chase("Minions", "after", None)
    split = assert_scene(rows, REACH["Minions"], water, first, "target")
    misses, _ = score(rows, REACH["Minions"], water, "after")
    assert misses == [], f"the shipped build left the moved reading (the readings split on {split}): {misses}"
