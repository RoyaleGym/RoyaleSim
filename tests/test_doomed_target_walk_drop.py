"""A walking projectile attacker drops a doomed target it has shot at (targeting.DOOMED_TARGET_DROP).

WHAT THIS PINS. An attacker with a projectile that has fired at its target keeps that target once the shots in flight
will kill it, while it is in its attack. When it has stopped attacking and WALKS after the target, beyond its keep
reach (Range + both radii + 25), it drops the doomed target on the next tick, fired or not, and does not take it
back while it lives. Measured on client 15.535.29, the Skeleton Dragons sweep scene: a Skeleton Dragon
spat at a Knight from 5356 (keep reach 4925), re-evaluated, kept the Knight (not yet doomed) and walked after it;
on 347 the princess tower's arrow joined its spit in flight (151 + 109 against 248 hp) and on 348, 5041 from the
Knight, the dragon dropped it for the tower. The other dragon, still in its attack and 3372 from the Knight, kept it
until it died. Across every 15.535.29 record on disk (792 scenario runs, 94 sweep scenes), attackers in their attack
that had fired kept a doomed target in 1,007 of 1,007 episodes, one of them 234 beyond reach (a Minion of the Minion
Horde scene); walkers that had not fired dropped it in 23 of 23.

THE SCENARIOS. A red Knight walks down the right lane to the blue right princess tower; a blue Skeleton Dragon
attacks it.
  - Walking: the Knight from (14231, 10500), the dragon from (9351, 12287). The dragon's first spit leaves from beyond
    its keep reach, so it re-evaluates, keeps the Knight and walks after it; the tower's first arrow then joins the
    spit in flight. With 248 hp that dooms the Knight (the client's numbers); with 400 it does not.
  - Attacking: the Knight from (14231, 11000) with 330 hp, the dragon from (9551, 11787). The dragon spits once in
    reach, then stays in its attack while the Knight walks on out of its keep reach; the tower's second arrow dooms
    the Knight (70 hp) there.

WHICH ARM. projectile_attackers_rescan is the SHIPPED arm since the 2026-09-26 integration flip: its fired-at exemption
covers every keep, so the walking dragon keeps the doomed Knight. projectile_attackers_walk_drop is the proposed arm.
The tests pin each arm BY NAME through the battle's calibration.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.DOOMED_TARGET_DROP"
SHIPPED_ARM, NEW_ARM = "projectile_attackers_rescan", "projectile_attackers_walk_drop"
HOLD_KEY = "targeting.PROJECTILE_HOLD_SCOPE"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
CARDS = ("Knight", "SkeletonDragons")
WALK_KNIGHT, WALK_DRAGON = (14231, 10500), (9351, 12287)
ATTACK_KNIGHT, ATTACK_DRAGON = (14231, 11000), (9551, 11787)
#: the dragon's keep reach on the Knight: Range 3500 + radii 900 + 500, plus LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET 25
KEEP_REACH = 4925
IDLE, FIRED = 0, 2
#: LOGIC_PENDING_DAMAGE_IGNORE_IF_DURATION_LESS, in ticks of 50 ms
ETA_TICKS = 12
TICKS = 45


def play(arm, knight, dragon, knight_hp, hold=None):
    ov = {KEY: json.dumps(arm)}
    if hold is not None:
        ov[HOLD_KEY] = json.dumps(hold)
    b = royalesim.Battle(list(CARDS), [[0, 1, 2], [0, 1, 2]], calibration_overrides=ov)
    b.reset(
        0,
        [[0] * 8, [0] * 8],
        0,
        200,
        [10_000, 10_000],
        None,
        [(1, 0, knight[0] * SUB, knight[1] * SUB, knight_hp), (0, 1, dragon[0] * SUB, dragon[1] * SUB, -1)],
    )
    states = []
    for t in range(TICKS + 1):
        if t:
            b.step([], 1)
        s = json.loads(b.state_json())
        states.append(({e[F["uid"]]: e for e in s["entities"]}, s["projectiles"]))
    (k,) = [u for u, e in states[0][0].items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 1]
    (d,) = [u for u, e in states[0][0].items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 0]
    return states, k, d


def dist(ents, a, b):
    return math.dist((ents[a][F["x"]], ents[a][F["y"]]), (ents[b][F["x"]], ents[b][F["y"]])) / SUB


def shots_at(states, t, uid):
    return sum(1 for p in states[t][1] if p[P["target_uid"]] == uid)


def gone(states, uid):
    return next(t for t, (ents, _) in enumerate(states) if uid not in ents)


def fire_ticks(states, dragon):
    return [
        t for t in range(1, len(states)) if dragon in states[t][0] and states[t][0][dragon][F["attack_phase"]] == FIRED
    ]


def realised_doom(states, knight):
    """(D, K): the Knight is gone on K, and D is the last tick a shot at it was launched, the shots in flight on D
    being what killed it. Checked: no shot is launched at it after D, and they land within the 600 ms gate."""
    k = gone(states, knight)
    d = max(t for t in range(1, k) if shots_at(states, t, knight) > shots_at(states, t - 1, knight))
    assert k - d <= ETA_TICKS, f"precondition: the lethal shots land {k - d} ticks after D={d}, past the 600 ms gate"
    return d, k


def walking_scene(arm, knight_hp, hold=None):
    """The walking scenario, with its preconditions checked: (L, D, K, states, knight, dragon). `hold`: a
    targeting.PROJECTILE_HOLD_SCOPE arm by name, None the shipped one."""
    states, knight, dragon = play(arm, WALK_KNIGHT, WALK_DRAGON, knight_hp, hold)
    fired = fire_ticks(states, dragon)
    assert fired, "the dragon never fired: the scenario drifted"
    lt = fired[0]
    ents = states[lt][0]
    assert ents[dragon][F["target_uid"]] == knight, f"precondition: the dragon fired on {lt} at another target"
    assert shots_at(states, lt, knight) == 1, (
        f"precondition: the dragon's first spit on {lt} is not the only shot at the Knight"
    )
    assert dist(ents, dragon, knight) > KEEP_REACH, (
        f"precondition: the spit left from {dist(ents, dragon, knight):.0f}, within the keep reach {KEEP_REACH}"
    )
    dt = next((t for t in range(lt + 1, len(states)) if shots_at(states, t, knight) == 2), None)
    assert dt is not None, "precondition: no arrow ever joined the dragon's spit in flight"
    walk = [
        t
        for t in range(lt + 1, dt + 1)
        if states[t][0][dragon][F["attack_phase"]] != IDLE or states[t][0][dragon][F["target_uid"]] != knight
    ]
    assert not walk, f"precondition: the dragon was not walking after the Knight on {walk} (spit {lt}, arrow {dt})"
    far = dist(states[dt][0], dragon, knight)
    assert far > KEEP_REACH, f"precondition: on the arrow's tick {dt} the dragon is {far:.0f} away, within {KEEP_REACH}"
    return lt, dt, states, knight, dragon


def test_a_walking_attacker_drops_a_doomed_target_it_has_shot_at():
    lt, dt, states, knight, dragon = walking_scene(NEW_ARM, 248)
    d, k = realised_doom(states, knight)
    assert d == dt, f"precondition: the Knight was doomed on {d}, not on the arrow's tick {dt}"
    assert d + 1 < k, f"precondition: the Knight died on {k}, before the tick after the doom ({d + 1})"
    held = [t for t in range(d + 1, k) if states[t][0][dragon][F["target_uid"]] == knight]
    assert not held, (
        f"the dragon spat at the Knight on {lt}, walked after it and still (or again) targets it on {held}, "
        f"after the arrow of {d} doomed it"
    )


def test_the_shipped_arm_keeps_the_doomed_target():
    """projectile_attackers_rescan: the dragon has fired at the Knight, so it keeps it while it walks. Under the hold
    scope every_tick, by name: the shipped client_troop_in_attack (the 2026-09-28 flip) holds nothing for a walking
    dragon past its keep reach, so both arms of this key drop the Knight there (tests/doomed_target_drop.rs case 6
    selects the same)."""
    _, dt, states, knight, dragon = walking_scene(SHIPPED_ARM, 248, hold="every_tick")
    assert states[dt + 1][0][dragon][F["target_uid"]] == knight


@pytest.mark.parametrize("arm", [SHIPPED_ARM, NEW_ARM])
def test_a_walking_attacker_keeps_a_target_that_is_not_doomed(arm):
    """Control: with 400 hp the spit and the arrow (260) leave the Knight alive, so the walking dragon keeps it."""
    _, dt, states, knight, dragon = walking_scene(arm, 400)
    assert states[dt + 1][0][knight][F["hp"]] > 0
    assert states[dt + 1][0][dragon][F["target_uid"]] == knight, (
        f"the walking dragon let go of a Knight that was not doomed on {dt + 1}"
    )


@pytest.mark.parametrize("arm", [SHIPPED_ARM, NEW_ARM])
def test_an_attacker_in_its_attack_keeps_a_doomed_target_beyond_its_keep_reach(arm):
    """Control: the dragon that fired in reach and is still in its attack keeps the doomed Knight, though it stands
    beyond its keep reach. The walk, not the distance, ends the exemption."""
    states, knight, dragon = play(arm, ATTACK_KNIGHT, ATTACK_DRAGON, 330)
    d, k = realised_doom(states, knight)
    fired = [t for t in fire_ticks(states, dragon) if t < d]
    assert fired, f"precondition: the dragon had not fired at the Knight before the doom on {d}"
    ents = states[d][0]
    assert ents[dragon][F["attack_phase"]] != IDLE, f"precondition: the dragon was not in its attack on {d}"
    assert dist(ents, dragon, knight) > KEEP_REACH, (
        f"precondition: on {d} the dragon is {dist(ents, dragon, knight):.0f} away, within its keep reach"
    )
    assert d + 1 < k, f"precondition: the Knight died on {k}, before the tick after the doom ({d + 1})"
    lost = [t for t in range(d + 1, k) if states[t][0][dragon][F["target_uid"]] != knight]
    assert not lost, f"the dragon in its attack let go of the doomed Knight it had shot at, on {lost}"
