"""Dropping a doomed target keeps the swing when the new target is already in reach (targeting.DOOMED_DROP_SWING).

WHAT THIS PINS. On the 16.402 corpus a projectile attacker attacking a target that the shots in flight doom, and that
has not fired at it, drops it on the next tick (targeting.DOOMED_TARGET_DROP). When its new target already stands in
its reach its attack progress runs on and it fires on the old cycle: 57 of 57 corpus switches, 31 of 31 on client
15.535.29. In 20260920-072148, on tick 1035, a Minion at progress 1050 of 1200 switched from a Skeleton doomed by a
Musketeer's shot to another Skeleton in reach and fired on 1037. When the new target is out of reach the attacker walks
with progress 0 (44 of 44). The old arm cancels the swing in both cases.

THE SCENE. A blue Minion at (9000, 8800), a red Knight X at (9000, 13000), a second red Knight Y at (9800, 13100) with
3000 hp and a blue Musketeer at (9000, 6000). The Minion takes X, starts its swing, and the Musketeer's shot at X is in
flight before the Minion fires. With X at 60 hp that shot dooms X and the Minion switches to Y, which then stands in
its reach. The control gives X 2000 hp, so nothing dooms it and the Minion fires at it: that launch is the old cycle.
On RoyaleSim 126992a (the old arm): the switch on tick 15, the control's launch on 17, the doomed run's launch at Y
on 31. The out-of-reach scene moves the Minion to (9000, 9000) and Y to (10600, 13000).

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run
this file: the named tests go red and the rest stay green.
  * `doomed_drop_cancels_swing` -- the new arm still cancels the swing of a doomed drop:
    test_a_doomed_drop_to_an_enemy_in_reach_keeps_the_swing.
"""

from __future__ import annotations

import json
import math
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.DOOMED_DROP_SWING"
NEW_ARM, OLD_ARM = "client_keep_in_reach", "cancel"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
CARDS = ["Minions", "Knight", "Musketeer"]
IDLE, WINDUP, FIRED = 0, 1, 2
X_AT, MUSKETEER_AT = (9000, 13000), (9000, 6000)
IN_REACH = ((9000, 8800), (9800, 13100))
OUT_OF_REACH = ((9000, 9000), (10600, 13000))
DOOMED_HP, CONTROL_HP, Y_HP = 60, 2000, 3000
#: the Minion's reach on a Knight, centre distance: Range 2500 + radii 500 + 500
REACH = 3500
LEDGER = pathlib.Path(__file__).resolve().parent.parent / "data" / "calibration.json"


def play(arm, scene, x_hp, ticks=45):
    """The states after each tick (index 0 before the first) and the uids {M, X, Y, B}. `arm` None runs the build's own
    value."""
    minion, y = scene
    overrides = {KEY: json.dumps(arm)} if arm is not None else {}
    b = royalesim.Battle(CARDS, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(0, [[0] * 8, [0] * 8], 0, 200, [10_000, 10_000], None,
            [(0, 0, minion[0] * SUB, minion[1] * SUB, -1), (1, 1, X_AT[0] * SUB, X_AT[1] * SUB, x_hp),
             (1, 1, y[0] * SUB, y[1] * SUB, Y_HP), (0, 2, MUSKETEER_AT[0] * SUB, MUSKETEER_AT[1] * SUB, -1)])
    s = json.loads(b.state_json())
    uid = {}
    for e in s["entities"]:
        if e[F["tower_slot"]] >= 0:
            continue
        if e[F["team"]] == 0:
            uid["M" if e[F["card_id"]] == 0 else "B"] = e[F["uid"]]
        else:
            uid["X" if e[F["hp"]] == x_hp else "Y"] = e[F["uid"]]
    states = [({e[F["uid"]]: e for e in s["entities"]}, s["projectiles"])]
    for _ in range(ticks):
        b.step([], 1)
        s = json.loads(b.state_json())
        states.append(({e[F["uid"]]: e for e in s["entities"]}, s["projectiles"]))
    return states, uid


def launches(states, uid):
    """(tick, the Minion's target) on every tick its attack phase reads FIRED (its shot leaves)."""
    m = uid["M"]
    return [(t, states[t][0][m][F["target_uid"]]) for t in range(1, len(states))
            if m in states[t][0] and states[t][0][m][F["attack_phase"]] == FIRED]


def switch(states, uid, what):
    """The tick S the Minion goes from X to Y with X still standing, its preconditions checked."""
    m, x, y = uid["M"], uid["X"], uid["Y"]
    s = next((t for t in range(1, len(states)) if states[t - 1][0][m][F["target_uid"]] == x
              and states[t][0][m][F["target_uid"]] == y), None)
    assert s is not None, f"{what}: the scene drifted: the Minion never switched from X to Y"
    assert x in states[s][0], f"{what}: precondition: X died on {s}, a kill and not a doomed drop"
    assert states[s - 1][0][m][F["attack_phase"]] == WINDUP, (
        f"{what}: precondition: the Minion was not in its swing on {s - 1}")
    assert not launches(states[:s], uid), f"{what}: precondition: the Minion fired before the switch on {s}"
    assert any(p[P["target_uid"]] == x and p[P["firer_card_id"]] == 2 for p in states[s - 1][1]), (
        f"{what}: precondition: no Musketeer shot flew at X on {s - 1}")
    return s


def distance(states, uid, t):
    a = states[t][0]
    m, y = a[uid["M"]], a[uid["Y"]]
    return math.dist((m[F["x"]], m[F["y"]]), (y[F["x"]], y[F["y"]])) / SUB


def fires_on_the_old_cycle(arm):
    control, cu = play(arm, IN_REACH, CONTROL_HP)
    old = launches(control, cu)
    assert old, f"{arm}: the scene drifted: the control Minion never fired"
    assert old[0][1] == cu["X"], f"{arm}: the scene drifted: the control Minion did not fire at X first"
    states, uid = play(arm, IN_REACH, DOOMED_HP)
    s = switch(states, uid, arm)
    d = distance(states, uid, s - 1)
    assert d <= REACH, f"{arm}: precondition: Y stood {d:.0f} from the Minion on {s - 1}, out of its reach {REACH}"
    assert s < old[0][0], f"{arm}: precondition: the switch on {s} is not before the control's launch on {old[0][0]}"
    first = launches(states, uid)
    assert first, f"{arm}: the Minion never fired"
    return old[0][0], first[0], uid


@pytest.mark.parametrize("arm", [NEW_ARM])
def test_a_doomed_drop_to_an_enemy_in_reach_keeps_the_swing(arm):
    old, (t, target), uid = fires_on_the_old_cycle(arm)
    assert (t, target) == (old, uid["Y"]), (
        f"{arm}: the Minion dropped the doomed X for Y in its reach and fired on {t} at "
        f"{'Y' if target == uid['Y'] else target}; on the old cycle it fires on {old}")


@pytest.mark.parametrize("arm", [OLD_ARM])
def test_the_old_arm_restarts_the_swing(arm):
    old, (t, target), uid = fires_on_the_old_cycle(arm)
    assert target == uid["Y"], f"{arm}: the Minion's first shot went to {target}, not Y"
    assert t > old, f"{arm}: the Minion fired on {t} and the control on {old}; the swing was kept"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_doomed_drop_to_an_enemy_out_of_reach_cancels_the_swing(arm):
    states, uid = play(arm, OUT_OF_REACH, DOOMED_HP)
    s = switch(states, uid, arm)
    d = distance(states, uid, s - 1)
    assert d > REACH, f"{arm}: precondition: Y stood {d:.0f} from the Minion on {s - 1}, inside its reach {REACH}"
    assert states[s][0][uid["M"]][F["attack_phase"]] == IDLE, (
        f"{arm}: the Minion kept its swing on {s} for Y out of reach")


def test_the_shipped_value_is_the_new_arm():
    entry = json.loads(LEDGER.read_text(encoding="utf-8"))["targeting"]["DOOMED_DROP_SWING"]
    assert entry["value"] == NEW_ARM
    assert entry["candidates"] == [OLD_ARM, NEW_ARM]
