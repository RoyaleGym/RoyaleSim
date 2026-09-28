"""A walking troop takes an enemy nearer than its crown tower by CENTRE distance (targeting.TARGET_RANK_DISTANCE).

WHAT THIS PINS. On the 16.402 corpus a troop walking to a crown tower re-decides on every tick from the start-of-tick
positions, and takes the nearest enemy within SightRange + both collision radii when that enemy's centre distance is
below the tower's centre distance: 577 of 578 switches fit. In 20260918-124946, on tick 941, a Goblin 6,578.4 from a
Cannon and 6,840.8 from its princess tower took the Cannon. Today's engine ranks by centre distance minus the
candidate's radius, so the tower (radius 1000) scores 5,840.8 against the Cannon's (600) 5,978.4 and the Goblin walks
on. The scene below is that geometry: the Cannon where it stood in that battle, (9500, 9500), and a red Knight
(SightRange 5500 and radius 500, as a Goblin's) walking down the left lane, from x 4000, to the blue left princess
tower at (3500, 6500).
On this engine the Knight first stands inside its sight sum of the Cannon about 6,574 from it and 6,871 from the tower
(the Goblin: 6,578 and 6,841).

WHY THE CONTROL IS HERE. With the Knight at x 3500 it sees the same Cannon while the tower is nearer by centre, and it
must keep the tower under both values: Goblins 32, 33 and 35 of the same battle walked past that Cannon, and 894 corpus
ticks of 57 walkers are of this kind. It refuses an implementation where any enemy in sight beats the tower.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run
this file: the named tests go red and the rest stay green.
  * `rank_centre_minus_radius` -- client16402_centre still subtracts the candidate's radius:
    test_a_walker_takes_an_enemy_nearer_than_its_tower_by_centre.
"""

from __future__ import annotations

import json
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.TARGET_RANK_DISTANCE"
NEW_ARM, OLD_ARM = "client16402_centre", "centre_minus_target_radius"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: the red Knight below the river on the left lane, walking down to the blue left princess tower at (3500, 6500): at x
#: 4000 it first sees the Cannon nearer than the tower by centre (the switch scene), at x 3500 farther (the control)
KNIGHT_SWITCH, KNIGHT_CONTROL = (4000, 14500), (3500, 14500)
TOWER_AT = (3500, 6500)
#: the Cannon of 20260918-124946
CANNON_AT = (9500, 9500)
SIGHT_SUM = 5500 + 500 + 600        # the Knight's SightRange + its radius + the Cannon's
TOWER_R, CANNON_R = 1000, 600
LEDGER = pathlib.Path(__file__).resolve().parent.parent / "data" / "calibration.json"


def centre(state, uid, other):
    """Centre distance between two entities of one state, native units."""
    a, b = state[uid], state[other]
    return ((a[F["x"]] - b[F["x"]]) ** 2 + (a[F["y"]] - b[F["y"]]) ** 2) ** 0.5 / SUB


def walk(arm, knight_at, ticks=160):
    """Per tick, from the first: (tick, the Cannon's and the tower's START-of-tick centre distance from the Knight, what
    the Knight targets after the tick: 'cannon', 'tower', None or 'other'). `arm` None runs the build's own value."""
    overrides = {KEY: json.dumps(arm)} if arm is not None else {}
    b = royalesim.Battle(["Knight", "Cannon"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(0, [[0] * 8, [0] * 8], 0, 200, [10_000, 10_000], None,
            [(1, 0, knight_at[0] * SUB, knight_at[1] * SUB, -1), (0, 1, CANNON_AT[0] * SUB, CANNON_AT[1] * SUB, -1)])

    def entities():
        return {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}

    now = entities()
    knight = next(u for u, e in now.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 1)
    cannon = next(u for u, e in now.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 0)
    tower = next(u for u, e in now.items() if e[F["tower_slot"]] >= 0 and e[F["team"]] == 0
                 and (e[F["x"]], e[F["y"]]) == (TOWER_AT[0] * SUB, TOWER_AT[1] * SUB))
    rows = []
    for t in range(1, ticks + 1):
        prev = now
        b.step([], 1)
        now = entities()
        if knight not in now or knight not in prev or cannon not in prev:
            break
        target = now[knight][F["target_uid"]]
        name = {cannon: "cannon", tower: "tower"}.get(target, None if target in (None, -1) else "other")
        rows.append((t, centre(prev, knight, cannon), centre(prev, knight, tower), name))
    return rows


def entry(rows, what):
    """The first tick that starts with the Cannon inside the Knight's sight sum, with the scene's preconditions."""
    first = next((i for i, r in enumerate(rows) if r[1] <= SIGHT_SUM), None)
    assert first is not None, f"{what}: the scene drifted: the Cannon never came into the Knight's sight"
    assert first > 0, f"{what}: the scene drifted: the Cannon was in sight from the first tick"
    assert all(r[3] == "tower" for r in rows[:first]), (
        f"{what}: the scene drifted: the Knight did not walk to its tower until the Cannon came into sight")
    return first


@pytest.mark.parametrize("arm", [NEW_ARM])
def test_a_walker_takes_an_enemy_nearer_than_its_tower_by_centre(arm):
    rows = walk(arm, KNIGHT_SWITCH)
    i = entry(rows, arm)
    t, dc, dt, _ = rows[i]
    # The scene must separate the rankings on this tick: nearer by centre, farther by centre minus radius.
    drift = f"{arm}: the scene drifted: on {t} the Cannon stands {dc:.1f} and the tower {dt:.1f}"
    assert dc < dt, drift + "; the tower is the nearer by centre"
    assert dc - CANNON_R > dt - TOWER_R, drift + "; the Cannon's centre - radius is the smaller too"
    assert rows[i][3] == "cannon", (
        f"{arm}: on {t} the Cannon is {dc:.1f} from the Knight and the tower {dt:.1f} (centre), but the Knight "
        f"targets {rows[i][3]}")


@pytest.mark.parametrize("arm", [OLD_ARM])
def test_the_old_arm_keeps_the_tower(arm):
    rows = walk(arm, KNIGHT_SWITCH)
    i = entry(rows, arm)
    split = [r for r in rows[i:] if r[1] <= SIGHT_SUM and r[1] - CANNON_R > r[2] - TOWER_R]
    assert split, f"{arm}: the scene drifted: the tower's key is never the smaller with the Cannon in sight"
    taken = [r for r in split if r[3] == "cannon"]
    assert not taken, f"{arm}: the Knight took the Cannon while its edge key was the larger: {taken[:3]}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_an_enemy_in_sight_but_farther_than_the_tower_is_not_taken(arm):
    rows = walk(arm, KNIGHT_CONTROL)
    i = entry(rows, arm)
    seen = [r for r in rows[i:] if r[1] <= SIGHT_SUM]
    nearer = [r for r in seen if r[1] < r[2]]
    assert not nearer, f"{arm}: the scene drifted: the Cannon was nearer than the tower: {nearer[:3]}"
    taken = [r for r in seen if r[3] == "cannon"]
    assert not taken, f"{arm}: the Knight took a Cannon in sight but farther than its tower: {taken[:3]}"


def test_the_shipped_value_is_the_new_arm():
    entry_ = json.loads(LEDGER.read_text(encoding="utf-8"))["targeting"]["TARGET_RANK_DISTANCE"]
    assert entry_["value"] == NEW_ARM
    assert entry_["candidates"] == [OLD_ARM, NEW_ARM]
