"""An attacker that launched at a doomed target from beyond reach does not take it back (targeting.DOOMED_TARGET_DROP).

WHAT THIS PINS. An attacker with a projectile that has fired at its target keeps that target while it stays in
reach, even when the shots in flight will kill it. But when the attacker LAUNCHES from beyond its reach (Range + both
radii + 25), it re-evaluates its target on the next tick, and the re-evaluation never takes a doomed unit, not even
the one it has just shot at. Measured on client 15.535.29, over the 775 scenario runs: 4 of 4 such launches at a
doomed target (three Minions at a Knight 257-434 beyond reach, an Electro Dragon at a Skeleton 952 beyond) were
followed by a drop on the next tick, and none of the four took the target back while it lived. The controls hold on
the same records: a re-evaluation after a launch beyond reach at a target that was NOT doomed returned that target
64 of 64 times (Archers, Mega Minions, Minions), and an attacker that launched in reach at a doomed target kept it
245 of 245 times.

THE SCENARIO. A red Knight walks down the right lane from (14500, 20000); a blue Minion starts 5000 to its left. The
Minion flies in, begins its swing when the Knight is in range and spits while the Knight has walked on out of reach.
With 60 hp the Knight is doomed by that spit (107); with 300 it is not.

WHICH ARM. projectile_attackers is the SHIPPED arm since the 2026-09-26 flip; it takes the doomed Knight back on the
re-evaluation, because the attacker has fired at it. projectile_attackers_rescan is the proposed arm. The tests pin
each arm BY NAME through the battle's calibration.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.DOOMED_TARGET_DROP"
SHIPPED_ARM, NEW_ARM = "projectile_attackers", "projectile_attackers_rescan"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
CARDS = ("Knight", "Minions")
KNIGHT, MINION = (14500, 20000), (9500, 20000)
#: the Minion's reach on the Knight, Range 2500 + radii 500 + 500, plus LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET 25
KEEP_REACH = 3525
FIRED = 2
TICKS = 45


def play(arm, knight_hp):
    b = royalesim.Battle(list(CARDS), [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    b.reset(0, [[0] * 8, [0] * 8], 0, 200, [10_000, 10_000], None,
            [(1, 0, KNIGHT[0] * SUB, KNIGHT[1] * SUB, knight_hp), (0, 1, MINION[0] * SUB, MINION[1] * SUB, -1)])
    states = []
    for t in range(TICKS + 1):
        if t:
            b.step([], 1)
        s = json.loads(b.state_json())
        states.append(({e[F["uid"]]: e for e in s["entities"]}, s["projectiles"]))
    (knight,) = [u for u, e in states[0][0].items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 1]
    (minion,) = [u for u, e in states[0][0].items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 0]
    return states, knight, minion


def launch(states, knight, minion):
    """The tick the Minion fires its first spit at the Knight, checked to be beyond reach."""
    fired = [t for t in range(1, len(states))
             if minion in states[t][0] and states[t][0][minion][F["attack_phase"]] == FIRED]
    assert fired, "the Minion never fired: the scenario drifted"
    t = fired[0]
    ents, shots = states[t]
    assert any(p[P["target_uid"]] == knight and p[P["firer_card_id"]] == 1 for p in shots), (
        f"precondition: no spit flies at the Knight on the fire tick {t}")
    assert ents[minion][F["target_uid"]] == knight, "precondition: the Minion fired at something other than the Knight"
    d = math.dist((ents[minion][F["x"]], ents[minion][F["y"]]), (ents[knight][F["x"]], ents[knight][F["y"]])) / SUB
    assert d > KEEP_REACH, f"precondition: the spit left from {d:.0f}, within the Minion's reach {KEEP_REACH}"
    return t


def gone(states, knight):
    return next(t for t, (ents, _) in enumerate(states) if knight not in ents)


def test_a_launch_beyond_reach_at_a_doomed_target_drops_it_for_good():
    states, knight, minion = play(NEW_ARM, 60)
    t = launch(states, knight, minion)
    k = gone(states, knight)
    assert t + 1 < k, f"precondition: the Knight died on {k}, before the tick after the spit ({t + 1})"
    held = [s for s in range(t + 1, k) if states[s][0][minion][F["target_uid"]] == knight]
    assert not held, (
        f"the Minion spat at the doomed Knight from beyond reach on {t} and still (or again) targets it on {held}")


@pytest.mark.parametrize("arm", [SHIPPED_ARM, NEW_ARM])
def test_the_re_evaluation_returns_a_target_that_is_not_doomed(arm):
    """Control: with 300 hp the spit (107) leaves the Knight alive, so the re-evaluation takes it back."""
    states, knight, minion = play(arm, 300)
    t = launch(states, knight, minion)
    assert states[t + 1][0][knight][F["hp"]] > 0
    assert states[t + 1][0][minion][F["target_uid"]] == knight, (
        f"the Minion let go of a Knight that was not doomed on {t + 1}")


def test_the_shipped_arm_takes_the_doomed_target_back():
    """projectile_attackers: the attacker has fired at the Knight, so the re-evaluation takes it back."""
    states, knight, minion = play(SHIPPED_ARM, 60)
    t = launch(states, knight, minion)
    assert states[t + 1][0][minion][F["target_uid"]] == knight
