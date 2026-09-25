"""A spawner's timer carries its overshoot into the next reload (spawner.TIMER_LEFTOVER), and a dying spawner
building puts its death spawn at its own emission point (spawner.DEATH_SPAWN_AT_EMISSION_POINT).

WHAT THIS PINS, 1: THE LEFTOVER. A spawner with a blank SpawnStartTime emits its first unit on its activation tick
A, the tick its timer is first decremented, from 0 to -50. The engine then reloaded SpawnInterval and dropped the -50,
so its next unit came at A + 10. In the 16.402 corpus a Tombstone emits at A, A + 9, A + 79, A + 89, A + 159 (first
gap 9 on 36 of 38 exact activations), and a Barbarian Hut at A, A + 9, A + 19, A + 299, A + 309. That is the reload
ADDED to the overshoot. A spawner with a start time fires at exactly 0, so it has nothing to carry.

WHAT THIS PINS, 2: THE DEATH POINT. A dying Tombstone's four Skeletons appear together, on one point, at the
place its spawner emits: about 1564 native from it (the tangent of the two circles plus one Skeleton step), in its
last emission's direction on 48 of 49 deaths. A dying Barbarian Hut's Barbarian does the same (1544, 4 degrees
from its last periodic Barbarian). The engine scattered a death spawn on a ring of its collision radius.

WHY THE CONTROLS ARE HERE. A Witch has a start time, so the leftover arm must leave her emission ticks alone. A
Golem is not on the death-point list, so its Golemites must land where they always did. The old arms must
reproduce today's engine, which also shows each scenario has a point.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
#: explicit, because catalogue ids are positional
DECK = ["Tombstone", "BarbarianHut", "Witch", "Golem", "Knight", "Musketeer", "Cannon", "Tesla"]
IDS = list(range(len(DECK)))
TS, BH, WI, GO, KN = 0, 1, 2, 3, 4
#: a blue building mid-way down its own half, native units, out of every tower's reach
AT = (9000, 8000)
LEFTOVER = "spawner.TIMER_LEFTOVER"
DEATH = "spawner.DEATH_SPAWN_AT_EMISSION_POINT"
# ENTITY_FIELDS: 0 uid, 1 team, 2 kind (1 = building), 4 tower_slot, 5 x, 6 y. A spawned unit carries its
# root's card_id, so a spawner and its units are told apart by kind and by creation order, not by card.
UID, TEAM, KIND, SLOT, X, Y = 0, 1, 2, 4, 5, 6


def death_arm(name: str) -> str:
    """The shipped value with only its arm swapped: the unit list comes from the ledger the module was built
    with, so the test never carries a second copy of it."""
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    value = ledger.get("spawner", {}).get("DEATH_SPAWN_AT_EMISSION_POINT", {}).get("value")
    if value is None:
        pytest.fail(f"{DEATH} is not in the compiled-in ledger: this build predates the key")
    return json.dumps({**value, "arm": name})


def births(overrides: dict, spawns: list, play, ticks: int) -> dict:
    """uid -> (first tick seen, kind, native x, native y) for every blue non-tower entity, from the tick after
    `play` resolves (or after reset). `spawns` is reset's list of (team, card, x, y, hp) in native units."""
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, [(t, c, x * SUB, y * SUB, hp) for t, c, x, y, hp in spawns])
    b.step([], 1)
    if play is not None:
        played = b.step([(0, play, AT[0] * SUB, AT[1] * SUB)], 1)
        assert played, "the play did not resolve"
        assert played[0][0] == play, f"another card resolved: {played}"
    first = {}
    for t in range(ticks):
        for e in json.loads(b.state_json())["entities"]:
            if e[TEAM] == 0 and e[SLOT] < 0 and e[UID] not in first:
                first[e[UID]] = (t, e[KIND], e[X] // SUB, e[Y] // SUB)
        b.step([], 1)
    return first


def emissions(overrides: dict, card: int, ticks: int) -> list:
    """The ticks of a played spawner's emissions, relative to its first. The spawner is the first blue
    entity created; everything after it is its output."""
    first = births(overrides, [], card, ticks)
    spawner = min(first)
    got = sorted(t for uid, (t, _, _, _) in first.items() if uid != spawner)
    assert got, "the spawner emitted nothing"
    return [t - got[0] for t in got]


def death_spawn(overrides: dict, card: int) -> tuple:
    """A blue spawner building placed with 3 hp decays to death within a few ticks. Returns (its periodic unit's
    first position, the death spawn's first positions). The periodic unit comes out on tick 0, where the
    spawner emits."""
    first = births(overrides, [(0, card, *AT, 3)], None, 12)
    units = sorted((t, (x, y)) for uid, (t, k, x, y) in first.items() if k != 1)
    periodic = [p for t, p in units if t == 0]
    later = [(t, p) for t, p in units if t > 0]
    assert len(periodic) == 1, f"expected one periodic unit on tick 0: {units}"
    assert later, f"the {DECK[card]} did not die within the window, so nothing was tested: {units}"
    death_tick = later[0][0]
    return periodic[0], [p for t, p in later if t == death_tick]


def half_width(points: list) -> float:
    cx = sum(p[0] for p in points) / len(points)
    cy = sum(p[1] for p in points) / len(points)
    return max(math.hypot(p[0] - cx, p[1] - cy) for p in points)


# ---- 1. the leftover


def test_a_tombstone_carries_its_leftover():
    got = emissions({LEFTOVER: json.dumps("client16402_carried")}, TS, 200)
    assert got[:5] == [0, 9, 79, 89, 159], f"Tombstone emissions {got[:5]}; the game's are 0, 9, 79, 89, 159"


def test_a_barbarian_hut_carries_its_leftover():
    got = emissions({LEFTOVER: json.dumps("client16402_carried")}, BH, 330)
    assert got[:5] == [0, 9, 19, 299, 309], f"Barbarian Hut emissions {got[:5]}; the game's are 0, 9, 19, 299, 309"


def test_a_witch_has_nothing_to_carry():
    """A start time fires at exactly 0: the carried arm changes nothing."""
    new = emissions({LEFTOVER: json.dumps("client16402_carried")}, WI, 330)
    old = emissions({LEFTOVER: json.dumps("dropped")}, WI, 330)
    assert len(new) >= 6, f"too few Witch emissions to compare: {new}"
    assert new == old, f"the Witch's emissions moved: {new} vs {old}"


def test_the_old_leftover_arm_is_todays_engine():
    got = emissions({LEFTOVER: json.dumps("dropped")}, TS, 200)
    assert got[:5] == [0, 10, 80, 90, 160], f"the old arm must reproduce today's engine: {got[:5]}"


# ---- 2. the death point


@pytest.mark.parametrize(("card", "n"), [(TS, 4), (BH, 1)], ids=["Tombstone", "BarbarianHut"])
def test_a_dying_spawner_building_spawns_at_its_emission_point(card, n):
    """Every death-spawn member appears on the point its spawner emits at, all together. The tolerance is one
    unit step, so a later spawn-tick step does not turn this red."""
    periodic, dead = death_spawn({DEATH: death_arm("client16402_measured_list")}, card)
    assert len(dead) == n, f"expected {n} death-spawn units: {dead}"
    assert half_width(dead) == 0, f"the death spawn is not on one point: {dead}"
    off = math.hypot(dead[0][0] - periodic[0], dead[0][1] - periodic[1])
    assert off <= 100, f"the death spawn stands {off:.0f} from the emission point {periodic}: {dead}"


def test_an_unlisted_golem_is_unchanged():
    """Not on the list: the Golemites land where they always did, under both arms. A red Knight kills the
    3-hp Golem."""
    units = [(0, GO, 9000, 12000, 3), (1, KN, 9000, 13300, -1)]
    first_new = births({DEATH: death_arm("client16402_measured_list")}, units, None, 30)
    first_old = births({DEATH: death_arm("none")}, units, None, 30)
    new = sorted((t, x, y) for _, (t, _, x, y) in first_new.items() if t > 0)
    old = sorted((t, x, y) for _, (t, _, x, y) in first_old.items() if t > 0)
    assert len(new) == 2, f"the Golem did not leave two Golemites within the window: {new}"
    assert new == old, f"the Golemites moved: {new} vs {old}"


def test_the_old_death_arm_is_todays_engine():
    """The engine before the flip scattered the Tombstone's four on a ring of its collision radius. The flip
    moved spawner.DEATH_SPAWN_RADIUS_DEFAULT too (to zero), so the old engine is BOTH old arms: the death arm
    alone would now lay the four on the Tombstone's centre."""
    old = {DEATH: death_arm("none"), "spawner.DEATH_SPAWN_RADIUS_DEFAULT": json.dumps("own_collision_radius")}
    _, dead = death_spawn(old, TS)
    assert len(dead) == 4, f"expected 4 death Skeletons: {dead}"
    assert half_width(dead) >= 500, f"the old arm did not scatter the death spawn: {dead}"
