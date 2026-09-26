"""Death-spawned units are not acquired as targets before their 8th frame (targeting.SPAWNED_UNIT_ACQUIRE_DELAY).

WHAT THIS PINS. On client 15.535.29 every enemy first targets a death-spawned unit on its 8th frame, F+7, where F is
the unit's first frame. The witnesses are an idle Cannon and the Knight that killed the cage for the Goblin Cage's
Brawler, an Inferno Tower, a Knight and a Musketeer for the Golemites (both sides), and the attackers of the Lava
Hound's Pups and the Skeleton Barrel's Skeletons. Over the 15.535.29 scenario runs, 35 death spawns were first targeted
on exactly F+7 and none on F+1 to F+6. The Golemite row has no DeployDelay, so the rule is not DeployDelay. The none
arm lets an idle Cannon target the Brawler and a Golemite on F+1.

WHY THE CONTROL IS HERE. A hand-played unit is targeted on its first frame, on client 15.535.29 (a Barbarian beside the
Goblin Cage scenario) and on both arms (a Knight). An implementation that delays every new unit fails it.

WHICH ARM. client_8th_frame is the SHIPPED arm since the 2026-09-26 flip. none is the engine before the flip. The
tests below pin each arm BY NAME through the battle's calibration, never through the shipped value, and the last one
runs the shipped build with no override at all.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.SPAWNED_UNIT_ACQUIRE_DELAY"
SHIPPED_ARM, NONE_ARM = "client_8th_frame", "none"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: the 8th frame of a unit whose first frame is F
EIGHTH = 7
CANNON_AT = (12500, 18500)


def overrides(arm) -> dict:
    """The battle's calibration pinning `arm` BY NAME; None runs the shipped build with no override."""
    return {} if arm is None else {KEY: json.dumps(arm)}


def track(cards, spawns, arm, ticks=40, plays=(), decks=((0,) * 8, (1,) * 8)):
    """Run `spawns` (team, card index, x, y, hp), with `plays` (team, x, y) of each team's first deck slot issued on
    the first tick. Returns (births, looks, names): births maps each unit that is not there after the reset to (first
    tick, team); looks maps (newborn uid, looker uid) to the first tick the looker's target is that newborn, for lookers
    of the other team; names maps uids to card names."""
    b = royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [list(d) for d in decks], 0, 200, [10_000, 10_000], None,
            [(t, c, x * SUB, y * SUB, hp) for t, c, x, y, hp in spawns])
    start = {e[F["uid"]] for e in json.loads(b.state_json())["entities"]}
    births, looks, names = {}, {}, {}
    for t in range(1, ticks + 1):
        b.step([(team, 0, x * SUB, y * SUB) for team, x, y in plays] if t == 1 else [], 1)
        ents = json.loads(b.state_json())["entities"]
        for e in ents:
            u = e[F["uid"]]
            names.setdefault(u, cards[e[F["card_id"]]] if e[F["tower_slot"]] < 0 else "tower")
            if u not in start and u not in births:
                births[u] = (t, e[F["team"]])
        for e in ents:
            tu = e[F["target_uid"]]
            if tu in births and births[tu][1] != e[F["team"]]:
                looks.setdefault((tu, e[F["uid"]]), t)
    return births, looks, names


def offset(got, first):
    """The first-targeted tick as F+k, or None when the unit was never targeted."""
    return None if got is None else got - first


def cannon_uid(names) -> int:
    return next(u for u, n in names.items() if n == "Cannon")


def cage_run(arm):
    """A blue Goblin Cage at 0 hp at (14500, 13500) dies on the first tick before anything aims at it: no killer and no
    attack on it, so no attacker is in the wait after a kill. A red Cannon at (12500, 18500), 5385 away, has nothing
    else to shoot. (At 1 hp the cage dies of its lifetime drain AFTER the Cannon has begun an attack on it, and the
    Cannon finishes that attack before it looks again, combat.POST_KILL_RETARGET_WAIT: F+6 under the none arm.)"""
    births, looks, names = track(["Cannon", "Knight", "GoblinCage"],
                                 [(1, 0, *CANNON_AT, -1), (0, 2, 14500, 13500, 0)], arm)
    assert len(births) == 1, f"expected the Brawler alone to be born, got {births}"
    (brawler, (first, _)), = births.items()
    assert first <= 3, f"the cage did not die at once (the Brawler's first tick is {first})"
    return brawler, first, looks, names


def golem_run(arm):
    """A blue Golem at 0 hp at (9000, 13000) dies on the first tick before anything aims at it; a red Knight at
    (9000, 14700) and a red Cannon at (12000, 18500) watch, both idle. The two Golemites are the newborns. (At 1 hp the
    Knight kills the Golem, and the Knight and the Cannon are both still busy with it, combat.POST_KILL_RETARGET_WAIT:
    the none arm then reads F+6, a tick from the shipped arm's F+7, so the pin below would barely separate the arms.)"""
    births, looks, names = track(["Golem", "Knight", "Cannon"],
                                 [(0, 0, 9000, 13000, 0), (1, 1, 9000, 14700, -1), (1, 2, 12000, 18500, -1)], arm)
    assert len(births) == 2, f"expected two Golemites, got {births}"
    first = {t for t, _ in births.values()}
    assert len(first) == 1, f"the Golemites were not born on one tick: {births}"
    return set(births), first.pop(), looks, names


def test_brawler_first_targeted_on_its_8th_frame():
    brawler, first, looks, names = cage_run(SHIPPED_ARM)
    early = {names[w]: t - first for (u, w), t in looks.items() if u == brawler and t - first < EIGHTH}
    assert early == {}, f"targeted before its 8th frame (looker: F+k): {early}"
    got = looks.get((brawler, cannon_uid(names)))
    assert offset(got, first) == EIGHTH, (
        f"the idle Cannon first targeted the Brawler on F+{offset(got, first)}, not F+{EIGHTH}")


def test_golemites_first_targeted_on_their_8th_frame():
    """The Golemite row has no DeployDelay: the delay belongs to the death spawn, not to a DeployDelay column."""
    kids, first, looks, names = golem_run(SHIPPED_ARM)
    early = {(names[w], u): t - first for (u, w), t in looks.items() if u in kids and t - first < EIGHTH}
    assert early == {}, f"a Golemite was targeted before its 8th frame ((looker, uid): F+k): {early}"
    cannon = cannon_uid(names)
    got = min((t for (u, w), t in looks.items() if u in kids and w == cannon), default=None)
    assert offset(got, first) == EIGHTH, (
        f"the Cannon first targeted a Golemite on F+{offset(got, first)}, not F+{EIGHTH}")


@pytest.mark.parametrize("arm", [SHIPPED_ARM, NONE_ARM])
def test_hand_played_unit_targeted_on_its_first_frame(arm):
    """Control: the red Cannon targets a blue Knight played at (14500, 13500) on the Knight's first frame."""
    births, looks, names = track(["Cannon", "Knight", "GoblinCage"], [(1, 0, *CANNON_AT, -1)], arm, ticks=10,
                                 plays=[(0, 14500, 13500)], decks=((1,) * 8, (0,) * 8))
    knight = next(u for u in births if names[u] == "Knight")
    first = births[knight][0]
    got = looks.get((knight, cannon_uid(names)))
    assert offset(got, first) == 0, f"the Cannon first targeted the hand-played Knight on F+{offset(got, first)}"


def test_the_none_arm_gives_f1():
    brawler, first, looks, names = cage_run(NONE_ARM)
    got = looks.get((brawler, cannon_uid(names)))
    assert offset(got, first) == 1, f"none arm: the idle Cannon targeted the Brawler on F+{offset(got, first)}, not F+1"
    kids, first, looks, names = golem_run(NONE_ARM)
    got = min((t for (u, w), t in looks.items() if u in kids), default=None)
    assert got is not None, "none arm: no red unit ever targeted a Golemite"
    assert got - first == 1, f"none arm: a Golemite was first targeted on F+{got - first}, not F+1"


def test_the_shipped_build_delays_the_brawler_to_f7():
    """No override at all. The compiled-in ledger ships SHIPPED_ARM, so the shipped build behaves as the client."""
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    shipped = ledger["targeting"]["SPAWNED_UNIT_ACQUIRE_DELAY"]["value"]
    assert shipped == SHIPPED_ARM, f"{KEY} ships {shipped!r}, not the arm this file names as shipped"
    brawler, first, looks, names = cage_run(None)
    got = looks.get((brawler, cannon_uid(names)))
    assert offset(got, first) == EIGHTH, (
        f"the shipped build: the Cannon first targeted the Brawler on F+{offset(got, first)}")
