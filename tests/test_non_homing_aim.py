"""A splash shot whose row is not Homing keeps the aim it was fired with (combat.NON_HOMING_AIM).

WHAT THIS PINS. projectiles.csv marks each shot Homing or not. The engine re-aims every shot at its target each tick,
so a splash lands centred on the target wherever it has walked. On the client 16.402 corpus a shot that is not Homing
keeps its end point for the whole flight: the Bomber 26 of 26 shots at a moving target, the Mortar 2 of 2, the
Princess 39 of 39 arrows, and the end point is the target's position at the start of the fire tick. The Princess's
area lands on that point. On 20260920-082459 her volley of 2577 kills a Bomber 2,479 from the end point and 3,040 from
the Giant she shot at, and her volley of 2520 takes 168 off a Goblin 2,020 from the end point and 2,928 from the Giant.
Today's engine leaves both alive. Homing shots follow their target on the client too (2,141 of 2,150).

The scenes: a blue shooter and a red Giant walking toward the blue side, and for the splash a red Cannon standing where
the Giant was when the Princess fired, inside her area there (2,000 + its radius 600) and outside it where the Giant is
when the shot lands. The positions are the engine's own, read from the battle, so the tests say what they compare.

WHY THE CONTROLS ARE HERE. An engine that stops re-aiming every shot passes the new-arm tests, so a Wizard (a Homing
splash row) must still follow the Giant under both arms. And the old arm must be today's engine: the Princess's shot
follows the Giant and the Cannon is not hit.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then
run this file: the named tests go red and the rest stay green.
  * `non_homing_shot_follows` -- the new arm's non-homing splash shot still follows its target:
    test_a_non_homing_shot_keeps_its_fire_time_aim, test_the_area_lands_where_the_shot_was_aimed.
"""

from __future__ import annotations

import json
import math
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.NON_HOMING_AIM"
NEW_ARM, OLD_ARM = "fixed_at_fire", "follows_target"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
LEDGER = pathlib.Path(__file__).resolve().parent.parent / "data" / "calibration.json"
#: where each shooter is spawned (blue), the Giant (red, it walks toward the blue right princess tower) and the Cannon
SHOOTER_AT = {"Princess": (5500, 5000), "Bomber": (11000, 7500), "Wizard": (10000, 7000)}
GIANT_AT, CANNON_AT = (13000, 12000), (12356, 13913)
#: the Princess's shot at level 11 (PrincessProjectile 66 x 256 %), its Radius, and the Cannon's collision radius
VOLLEY, RADIUS, CANNON_R = 168, 2000, 600
#: a Cannon's lifetime takes a little hp every tick; a hit is a loss of at least VOLLEY, a tick without one loses less
DECAY_MAX = 5
#: the Giant must have walked at least this far between the shot's creation and its landing for a scene to count
MIN_WALK = 200


def overrides(arm) -> dict:
    """The battle's calibration pinning `arm` BY NAME (None: the build's own value)."""
    return {} if arm is None else {KEY: json.dumps(arm)}


def dist(a, b) -> float:
    return math.hypot(a[0] - b[0], a[1] - b[1])


def scene(shooter: str, arm, cannon: bool = False, ticks: int = 60) -> dict:
    """A blue `shooter` and a red Giant (and a red Cannon) spawned on the reset, the towers standing. Its first shot
    followed tick by tick: the state before each step, the shot's aims, the tick it appears and the tick it is gone."""
    b = royalesim.Battle([shooter, "Giant", "Cannon"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    sx, sy = SHOOTER_AT[shooter]
    spawns = [(0, 0, sx * SUB, sy * SUB, -1), (1, 1, GIANT_AT[0] * SUB, GIANT_AT[1] * SUB, -1)]
    if cannon:
        spawns.append((1, 2, CANNON_AT[0] * SUB, CANNON_AT[1] * SUB, -1))
    b.reset(1, [[0] * 8, [1] * 8], 0, 0, None, None, spawns)
    states = []
    for _ in range(ticks):
        st = json.loads(b.state_json())
        units = {e[F["card_id"]]: (e[F["x"]] // SUB, e[F["y"]] // SUB, e[F["hp"]])
                 for e in st["entities"] if e[F["tower_slot"]] < 0 and e[F["team"]] == 1}
        shots = [((p[P["x"]] // SUB, p[P["y"]] // SUB), (p[P["aim_x"]] // SUB, p[P["aim_y"]] // SUB))
                 for p in st["projectiles"] if p[P["firer_card_id"]] == 0]
        states.append((st["tick"], units, shots))
        b.step([], 1)
    first = next((i for i, s in enumerate(states) if s[2]), None)
    assert first is not None, f"the {shooter} never shot"
    gone = next((i for i in range(first, len(states)) if not states[i][2]), None)
    assert gone is not None, f"the {shooter}'s first shot never landed"
    aims = [states[i][2][0][1] for i in range(first, gone)]
    giant = {states[i][0]: states[i][1][1][:2] for i in range(len(states)) if 1 in states[i][1]}
    return {"states": states, "first": first, "gone": gone, "aims": aims, "giant": giant}


def walked(s) -> float:
    t0, t1 = s["states"][s["first"]][0], s["states"][s["gone"] - 1][0]
    return dist(s["giant"][t0], s["giant"][t1])


@pytest.mark.parametrize("shooter", ["Princess", "Bomber"])
def test_a_non_homing_shot_keeps_its_fire_time_aim(shooter):
    s = scene(shooter, NEW_ARM)
    assert walked(s) >= MIN_WALK, f"vacuous: the Giant walked {walked(s):.0f} during the {shooter}'s flight"
    fire = s["states"][s["first"]][0]
    start = s["giant"][fire - 1]
    assert s["aims"][0] == start, f"the {shooter}'s shot is aimed at {s['aims'][0]}, the Giant stood at {start}"
    moved = [(s["states"][s["first"] + k][0], a) for k, a in enumerate(s["aims"]) if a != start]
    assert not moved, (f"the {shooter}'s shot followed the Giant: aimed at {start} on {fire}, then at {moved[:4]}; "
                       f"a shot whose row is not Homing keeps its aim")


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_homing_shot_follows_its_target(arm):
    """The control: the Wizard's row is Homing, so its shot is re-aimed at the Giant each tick under either arm."""
    s = scene("Wizard", arm)
    assert walked(s) >= MIN_WALK, f"vacuous: the Giant walked {walked(s):.0f} during the Wizard's flight"
    off = [(s["states"][s["first"] + k][0], a, s["giant"][s["states"][s["first"] + k][0]])
           for k, a in enumerate(s["aims"]) if k > 0 and a != s["giant"][s["states"][s["first"] + k][0]]]
    assert not off, f"the Wizard's shot did not follow the Giant: (tick, aim, Giant) {off[:4]}"


def cannon_loss(s) -> list:
    """(tick, the Cannon's hp loss on that tick) over the Princess's first flight, through its landing."""
    out = []
    for i in range(s["first"], s["gone"] + 1):
        a, b = s["states"][i - 1][1].get(2), s["states"][i][1].get(2)
        if a and b:
            out.append((s["states"][i][0], a[2] - b[2]))
    return out


def test_the_area_lands_where_the_shot_was_aimed():
    s = scene("Princess", NEW_ARM, cannon=True)
    aim = s["aims"][0]
    land = s["states"][s["gone"]][0]
    cannon = s["states"][s["gone"] - 1][1][2][:2]
    giant_then = s["giant"][land]
    assert dist(cannon, aim) <= RADIUS + CANNON_R - 100, f"vacuous: the Cannon is {dist(cannon, aim):.0f} from the aim"
    assert dist(cannon, giant_then) > RADIUS + CANNON_R + 100, (
        f"vacuous: the Cannon is {dist(cannon, giant_then):.0f} from the Giant where the shot lands")
    loss = dict(cannon_loss(s))
    assert VOLLEY <= loss.get(land, 0) <= VOLLEY + DECAY_MAX, (
        f"the shot aimed at {aim} landed on {land} and the Cannon {dist(cannon, aim):.0f} from that point lost "
        f"{loss.get(land)}, not {VOLLEY}: the area was centred on the Giant at {giant_then}, "
        f"{dist(cannon, giant_then):.0f} from the Cannon (losses by tick {sorted(loss.items())})")


def test_the_old_arm_is_todays_engine():
    """The Princess's shot follows the Giant and its area misses the Cannon."""
    s = scene("Princess", OLD_ARM, cannon=True)
    assert walked(s) >= MIN_WALK, f"vacuous: the Giant walked {walked(s):.0f} during the flight"
    follows = [a == s["giant"][s["states"][s["first"] + k][0]] for k, a in enumerate(s["aims"]) if k > 0]
    assert follows, f"vacuous: the old arm's shot lasted one frame: {s['aims']}"
    assert all(follows), f"the old arm's shot did not follow the Giant: {s['aims'][:6]}"
    hits = [(t, d) for t, d in cannon_loss(s) if d > DECAY_MAX]
    assert not hits, f"the Cannon was hit under the old arm: {hits}"


def test_the_shipped_value_is_the_new_arm():
    entry = json.loads(LEDGER.read_text(encoding="utf-8"))["combat"]["NON_HOMING_AIM"]
    assert entry["value"] == NEW_ARM, entry["value"]
    assert set(entry["candidates"]) == {OLD_ARM, NEW_ARM}, entry["candidates"]
