"""A troop whose spawn lane's princess tower is down walks to the king (targeting.FALLEN_LANE_TOWER_PICK).

WHAT THIS PINS. On the 16.402 corpus, with one enemy princess tower down, a troop that picks a crown tower to walk to
(no crown tower in sight) takes the king when the princess tower of its SPAWN lane is the one down, wherever it stands:
5 of 5 such picks, by troops standing on the other side of the centre, beside the standing princess tower. In
20260920-082459 an Inferno Dragon created at x 8500, with the left tower down, chased a Giant to x 9460 and then flew
to the king for 282 ticks. Today's engine takes the tower of the troop's current x after the first-pick window, so it
sent that Dragon to the right princess tower.

The scene: the red left princess tower is down; a blue Baby Dragon created at x 8500 (spawn lane left) chases a red
Cannon (60 hp) to x 9800 on its own half and kills it, long after the first-pick window. From there it walks to its
default tower, and the heading between the kill and the first crown tower it names says which one.

WHY THE CONTROLS ARE HERE. "Walks to the king" also passes for an engine that sends every troop to the king once any
princess tower is down, so a Baby Dragon created at x 9500 (its spawn lane's tower standing) must still head for the
right princess tower under both values; and with both princess towers standing the value must not matter. The old
value must be today's engine.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then
run this file: the named tests go red and the rest stay green.
  * `fallen_lane_by_x` -- the new value still takes the tower of the current x:
    test_a_troop_whose_spawn_lane_tower_is_down_walks_to_the_king.
"""

from __future__ import annotations

import json
import math
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.FALLEN_LANE_TOWER_PICK"
NEW_ARM, OLD_ARM = "client16402_spawn_lane_king", "current_x"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: the flyer created left of the centre (and, for a control, right of it); the Cannon it chases on its own half, out
#: of every tower's reach
FLYER_LEFT, FLYER_RIGHT, CANNON_AT = (8500, 13000), (9500, 13000), (12500, 18000)
W = 18000
#: ticks of the lane window after the spawn (FIRST_TOWER_PICK's 500 ms); the kill must come after it
WINDOW_TICKS = 10
LEDGER = pathlib.Path(__file__).resolve().parent.parent / "data" / "calibration.json"


def overrides(arm):
    """The calibration overrides for `arm` (None: the build's own value)."""
    return {KEY: json.dumps(arm)} if arm is not None else {}


def run(arm, flyer_at, left_down=True, ticks=140):
    """The kill tick and the flyer's position there, the red crown towers {uid: (slot, x, y)}, and the flyer's positions
    from the kill up to the tick before it names a new target."""
    b = royalesim.Battle(["BabyDragon", "Cannon"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[0] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, [])
    start = json.loads(b.state_json())["entities"]
    hp = {(e[F["team"]], e[F["tower_slot"]]): e[F["hp"]] for e in start if e[F["tower_slot"]] >= 0}
    red = [hp[(1, 0)], 0 if left_down else hp[(1, 1)], hp[(1, 2)]]
    b.reset(
        0,
        [[0] * 8, [0] * 8],
        0,
        200,
        [10_000, 10_000],
        [[hp[(0, 0)], hp[(0, 1)], hp[(0, 2)]], red],
        [(0, 0, flyer_at[0] * SUB, flyer_at[1] * SUB, -1), (1, 1, CANNON_AT[0] * SUB, CANNON_AT[1] * SUB, 60)],
    )
    now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
    flyer = next(u for u, e in now.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 0)
    cannon = next(u for u, e in now.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 1)
    towers = {
        u: (e[F["tower_slot"]], e[F["x"]] / SUB, e[F["y"]] / SUB)
        for u, e in now.items()
        if e[F["tower_slot"]] >= 0 and e[F["team"]] == 1
    }
    kill, path = None, []
    for t in range(1, ticks + 1):
        b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        f = now[flyer]
        pos = (f[F["x"]] / SUB, f[F["y"]] / SUB)
        if kill is None:
            if cannon not in now:
                kill = (t, pos)
                path = [pos]
            continue
        if f[F["target_uid"]] not in (None, -1):
            break
        path.append(pos)
    return kill, towers, path


def heading(arm, flyer_at, left_down=True):
    """Which red crown tower the flyer's walk after the kill points at: (slot of the best-aligned tower, the kill, the
    number of walked ticks), with the scene's preconditions."""
    kill, towers, path = run(arm, flyer_at, left_down)
    assert kill is not None, f"{arm}: the scene drifted: the Cannon never died"
    assert kill[0] > WINDOW_TICKS, f"{arm}: the scene drifted: the kill on {kill[0]} fell inside the lane window"
    assert kill[1][0] > W / 2, f"{arm}: the scene drifted: at the kill the flyer stood at x {kill[1][0]:.0f}, left"
    assert len(path) >= 20, f"{arm}: the scene drifted: the flyer walked only {len(path)} ticks to its default tower"
    dx, dy = path[-1][0] - path[0][0], path[-1][1] - path[0][1]

    def cos(t):
        tx, ty = t[1] - path[0][0], t[2] - path[0][1]
        return (dx * tx + dy * ty) / (math.hypot(dx, dy) * math.hypot(tx, ty))

    best = max(towers.values(), key=cos)
    return best[0], kill, len(path)


@pytest.mark.parametrize("arm", [NEW_ARM])
def test_a_troop_whose_spawn_lane_tower_is_down_walks_to_the_king(arm):
    slot, kill, n = heading(arm, FLYER_LEFT)
    assert slot == 0, (
        f"{arm}: created at x {FLYER_LEFT[0]} with the left princess tower down, killed its target on {kill[0]} at x "
        f"{kill[1][0]:.0f}, then walked {n} ticks toward tower slot {slot}, not the king"
    )


@pytest.mark.parametrize("arm", [OLD_ARM])
def test_the_old_arm_walks_to_the_tower_of_the_current_x(arm):
    slot, kill, _ = heading(arm, FLYER_LEFT)
    assert slot == 2, (
        f"{arm}: after the kill at x {kill[1][0]:.0f} the flyer walked to tower slot {slot}, not the right"
    )


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_troop_whose_spawn_lane_tower_stands_walks_to_the_tower_of_its_x(arm):
    slot, _, _ = heading(arm, FLYER_RIGHT)
    assert slot == 2, f"{arm}: created at x {FLYER_RIGHT[0]}, it walked to tower slot {slot}, not the right princess"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_with_both_princess_towers_standing_the_value_does_not_matter(arm):
    slot, _, _ = heading(arm, FLYER_LEFT, left_down=False)
    assert slot == 2, f"{arm}: with both towers standing it walked to tower slot {slot}, not the right princess"


def test_the_shipped_value_is_the_old_arm():
    entry = json.loads(LEDGER.read_text(encoding="utf-8"))["targeting"]["FALLEN_LANE_TOWER_PICK"]
    assert entry["value"] == OLD_ARM
    assert entry["candidates"] == [OLD_ARM, NEW_ARM]
