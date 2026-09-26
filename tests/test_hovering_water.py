"""Hovering units route over the river's water and fight from there (pathfinding.HOVERING_WATER_RULE).

WHAT THIS PINS. On client 15.535.29 the two hovering troops of the card data (Hovering = true: the Battle Healer and
the Royal Ghost) plan their path through water cells away from the bridges, walk their centre over the water and stand
there to attack. Played on the blue bank at (11500, 14500) with no enemy in sight, the Battle Healer's route holds 2
water cells and its centre crosses the river's middle line (y 16000) at x 13000, west of the right bridge (x 13500 to
15500), up to 740 from land or bridge. The Royal Ghost at (8500, 14500) holds 3 water cells, crosses at x 6999 and goes
988 from land. With a red Goblin Cage at (9500, 18500) both walk straight across and attack the cage from over the
water, 893 to 994 from land, until it dies. Their non-hovering twins on the same tiles (a Knight and a Mini PEKKA) never
hold a water cell, cross on a bridge and stay within 356 of land. Today's engine plans the Battle Healer like the
Knight: by the bridge, on the Knight's exact track. The Royal Ghost does not load in today's engine, so the Battle
Healer stands in for both hovering troops here, on both tiles.

WHY THE CONTROLS ARE HERE. The Knight and the Mini PEKKA must keep taking the bridges under both arms: an
implementation that prices water for every ground mover fails them. A hovering unit walks the water at its own walking
speed (the client's Battle Healer and Royal Ghost are in the walking state on every frame over water, never the leap
state): an implementation that gives it the JumpEnabled leap moves it at JumpSpeed over the water and fails that
control. A flying implementation plans no grid route at all and fails the route assertion of the new-arm tests.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `hovering_priced_as_walker` -- a hovering troop is planned as a walker under the new arm:
    test_hovering_unit_routes_over_open_water, test_hovering_unit_attacks_from_over_the_river,
    test_hovering_unit_walks_the_water_at_its_walking_speed.
"""

from __future__ import annotations

import itertools
import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "pathfinding.HOVERING_WATER_RULE"
NEW_ARM, OLD_ARM = "priced_water_no_hop", "walker"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
ARENA = json.loads(royalesim.EMBEDDED_ARENA_JSON)
WATER_BIT = ARENA["bits"]["WATER"]
GRID = ARENA["grid"]
CELL = 500
BRIDGES = [(round(b["x_min"] * 1000), round(b["x_max"] * 1000)) for b in ARENA["bridges"]]
RIVER_MIDDLE = 16000
FAR_BANK = 17000
#: "deep over the water": more than one grid cell from land or bridge. On client 15.535.29 the hovering troops reached
#: 740 to 994 and their non-hovering twins 158 to 356 in the same scenes.
DEEP = 500
CAGE_AT = (9500, 18500)
CARDS = ["BattleHealer", "Knight", "MiniPekka", "GoblinCage"]


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def is_water(col, row) -> bool:
    return 0 <= row < len(GRID) and 0 <= col < len(GRID[0]) and bool(GRID[row][col] & WATER_BIT)


def land_distance(x, y) -> float:
    """0 on a dry cell (land or bridge); on a water cell, the distance to the nearest dry cell."""
    if not is_water(x // CELL, y // CELL):
        return 0.0
    best = math.inf
    for row, cells in enumerate(GRID):
        for col, bits in enumerate(cells):
            if bits & WATER_BIT:
                continue
            dx = max(col * CELL - x, 0, x - (col + 1) * CELL)
            dy = max(row * CELL - y, 0, y - (row + 1) * CELL)
            best = min(best, math.hypot(dx, dy))
    return best


def scene(arm, card, x, cage, ticks=130):
    """A blue `card` played at (x, 14500) on the 2nd tick; with `cage`, a red Goblin Cage played at (9500, 18500) on
    the 1st. Returns (rows, cage uid): rows are (tick, x, y, target uid, attack phase, route cells) of the blue unit."""
    b = royalesim.Battle(CARDS, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[CARDS.index(card)] * 8, [CARDS.index("GoblinCage")] * 8], 0, 200, [10_000, 10_000], None, [])
    rows, cage_uid = [], None
    for t in range(1, ticks + 1):
        plays = []
        if t == 1 and cage:
            plays.append((1, 0, CAGE_AT[0] * SUB, CAGE_AT[1] * SUB))
        if t == 2:
            plays.append((0, 0, x * SUB, 14500 * SUB))
        b.step(plays, 1)
        routes = {u[0]: u[6] for u in b.debug_units()}
        mine = []
        for e in json.loads(b.state_json())["entities"]:
            if e[F["tower_slot"]] >= 0:
                continue
            if e[F["team"]] == 1 and cage_uid is None:
                cage_uid = e[F["uid"]]
            if e[F["team"]] == 0:
                mine.append(e)
        if mine:
            assert len(mine) == 1, f"expected one blue unit, got {len(mine)}"
            e = mine[0]
            rows.append(
                (
                    t,
                    e[F["x"]] // SUB,
                    e[F["y"]] // SUB,
                    e[F["target_uid"]],
                    e[F["attack_phase"]],
                    routes.get(e[F["uid"]]) or [],
                )
            )
    assert rows, f"the blue {card} never appeared"
    if cage:
        assert cage_uid is not None, "the red Goblin Cage never appeared"
    return rows, cage_uid


def water_route_cells(rows) -> list:
    return sorted({tuple(c) for r in rows for c in r[5] if is_water(*c)})


def deepest(rows) -> tuple:
    """(largest distance from land or bridge, tick, x, y) over the run."""
    return max((land_distance(r[1], r[2]), r[0], r[1], r[2]) for r in rows)


def middle_crossing(rows):
    """(tick, x) where the centre first crosses y 16000 going north, x interpolated; None if it never does."""
    for a, b in itertools.pairwise(rows):
        if a[2] < RIVER_MIDDLE <= b[2]:
            return b[0], a[1] + (b[1] - a[1]) * (RIVER_MIDDLE - a[2]) // (b[2] - a[2])
    return None


def on_a_bridge_span(x) -> bool:
    return any(lo <= x <= hi for lo, hi in BRIDGES)


def cage_attacks(rows, cage_uid) -> list:
    """(tick, x, y, distance from land) of every tick the unit is in an attack phase on the cage."""
    return [(r[0], r[1], r[2], land_distance(r[1], r[2])) for r in rows if r[3] == cage_uid and r[4] > 0]


def check_crosses_open_water(rows):
    """The new law's open-scene predicate: a water cell in the route, the centre DEEP over water, the middle line
    crossed off both bridges."""
    assert max(r[2] for r in rows) >= FAR_BANK, "precondition: the unit never reached the far bank"
    cells = water_route_cells(rows)
    deep = deepest(rows)
    cross = middle_crossing(rows)
    assert cells, (
        f"no water cell in any route (deepest {deep[0]:.0f} from land at tick {deep[1]}, "
        f"crossed y {RIVER_MIDDLE} at {cross})"
    )
    assert deep[0] > DEEP, f"the centre went only {deep[0]:.0f} from land or bridge (tick {deep[1]}, {deep[2:]})"
    assert cross is not None, f"the centre never crossed y {RIVER_MIDDLE}"
    assert not on_a_bridge_span(cross[1]), f"crossed y {RIVER_MIDDLE} at {cross}, on a bridge span {BRIDGES}"


def check_attacks_from_the_water(rows, cage_uid):
    """The new law's target-scene predicate: every attack tick on the cage stands DEEP over water."""
    assert any(r[3] == cage_uid for r in rows), "precondition: the unit never took the cage"
    hits = cage_attacks(rows, cage_uid)
    assert hits, "precondition: the unit never attacked the cage"
    dry = [h for h in hits if h[3] <= DEEP]
    assert not dry, (
        f"{len(dry)} of {len(hits)} attack ticks on the cage stand within {DEEP} of land, "
        f"first {dry[0]}, last {dry[-1]}"
    )


def check_takes_the_bridge(rows, cage_uid):
    """The walkers' predicate: no water cell in any route, never DEEP over water, attacks from land."""
    cells = water_route_cells(rows)
    deep = deepest(rows)
    assert not cells, f"water cells in the route: {cells}"
    assert deep[0] <= DEEP, f"the centre went {deep[0]:.0f} from land or bridge (tick {deep[1]}, {deep[2:]})"
    if cage_uid is not None:
        hits = cage_attacks(rows, cage_uid)
        assert hits, "precondition: the walker never attacked the cage"
        wet = [h for h in hits if h[3] > DEEP]
        assert not wet, f"attack ticks DEEP over water: {wet[:3]}"
    else:
        assert max(r[2] for r in rows) >= FAR_BANK, "precondition: the walker never reached the far bank"


def check_walks_the_water(rows):
    """Every step that ends on a water cell is no longer than the unit's walking step (the median of its moving
    steps over the run, most of which are on land)."""
    steps = [(math.hypot(b[1] - a[1], b[2] - a[2]), b) for a, b in itertools.pairwise(rows)]
    moving = sorted(s for s, _ in steps if s > 0)
    wet = [(round(s, 1), b[0], b[1], b[2]) for s, b in steps if s > 0 and is_water(b[1] // CELL, b[2] // CELL)]
    assert len(moving) >= 20, f"precondition: the unit moved on only {len(moving)} ticks"
    assert wet, "precondition: the unit never stepped onto a water cell"
    walk = moving[len(moving) // 2]
    fast = [w for w in wet if w[0] > walk + 1]
    assert not fast, (
        f"{len(fast)} of {len(wet)} steps over water are longer than the walking step {walk:.1f}: {fast[:3]}"
    )


@pytest.mark.parametrize("x", [11500, 8500])
def test_hovering_unit_routes_over_open_water(x):
    """The Battle Healer from the client's Battle Healer tile (11500) and the Royal Ghost's tile (8500)."""
    rows, _ = scene(NEW_ARM, "BattleHealer", x, cage=False)
    check_crosses_open_water(rows)


def test_hovering_unit_attacks_from_over_the_river():
    rows, cage = scene(NEW_ARM, "BattleHealer", 11500, cage=True)
    check_attacks_from_the_water(rows, cage)


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
@pytest.mark.parametrize(
    ("card", "x", "cage"),
    [("Knight", 11500, False), ("MiniPekka", 8500, False), ("Knight", 11500, True), ("MiniPekka", 8500, True)],
)
def test_walkers_take_the_bridges(arm, card, x, cage):
    """Control: the non-hovering twins on the same tiles, in both scenes."""
    rows, cage_uid = scene(arm, card, x, cage)
    check_takes_the_bridge(rows, cage_uid)


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_hovering_unit_walks_the_water_at_its_walking_speed(arm):
    """Control: no leap over the water. Under the old arm the Battle Healer still touches the water cells at the
    bridge's corner."""
    rows, _ = scene(arm, "BattleHealer", 11500, cage=False)
    check_walks_the_water(rows)


def test_old_arm_is_todays_engine():
    rows, _ = scene(OLD_ARM, "BattleHealer", 11500, cage=False)
    cross = middle_crossing(rows)
    assert water_route_cells(rows) == [], "old arm: the Battle Healer planned through water"
    assert cross is not None, f"old arm: never crossed y {RIVER_MIDDLE}"
    assert on_a_bridge_span(cross[1]), f"old arm: crossed y {RIVER_MIDDLE} at {cross}"
    assert deepest(rows)[0] <= DEEP, f"old arm: went {deepest(rows)[0]:.0f} from land"
    rows, cage = scene(OLD_ARM, "BattleHealer", 11500, cage=True)
    hits = cage_attacks(rows, cage)
    assert hits, "old arm: the Battle Healer never attacked the cage"
    assert max(h[3] for h in hits) <= DEEP, f"old arm: attacked the cage from over water: {hits[:3]}"
