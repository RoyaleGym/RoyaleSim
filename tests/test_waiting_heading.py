"""A member waiting out its formation stagger is a blocker in a walker's avoidance vote (movement.WAITING_HEADING).

WHAT THIS PINS. On client 15.535.29, a Knight walks up its lane and a Goblins deploy lands beside it. Under
formation.STAGGER_WAIT at its measured arm, three of the four Goblins wait out their stagger, and one of them lands 365
behind the Knight's left shoulder. On that tick the game's Knight steps (80, 121), away from the waiting Goblin; the
engine's Knight stepped (20, 156), as if a same-facing neighbour were no obstacle, and was 309 off three ticks later.
A waiting member does not steer its neighbours by heading: the walker counts it as a blocker, as it counts an attacking
unit.

WHY THE CONTROL IS HERE. "Count it as a blocker" also passes for a build that zeroes EVERY deploying unit's heading,
which is wrong: two Archers deploying together on client 15.535.29 show the walker going straight past its deploying
(not waiting) partner, and zeroing every deploying unit costs the 16.402 corpus 10.8 points within 250. So the same
file asks the zeroed arm to leave the Archer walker on the track client 15.535.29 recorded. And the kept arm is
pinned too (the engine before the flip), which is also what shows the Knight scenario has a point.

WHICH ARM. zeroed is the SHIPPED arm since the 2026-09-26 flip. kept is the engine before the flip. The tests below
pin each arm BY NAME through the battle's calibration, never through the shipped value, and the last one runs the
shipped build with no override at all.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "movement.WAITING_HEADING"
SHIPPED_ARM, KEPT_ARM = "zeroed", "kept"
STAGGER = {"formation.STAGGER_WAIT": json.dumps("client16402_untargetable_immovable")}
# ENTITY_FIELDS: 1 team, 3 card_id, 4 tower_slot, 5 x, 6 y
TEAM, CARD, SLOT, X, Y = 1, 3, 4, 5, 6
#: the walking Knight client 15.535.29 recorded, native units, on the ticks after the Goblins land (issued on 139)
KNIGHT_TRUTH = {
    140: (4431, 10735),
    141: (4518, 10859),
    142: (4611, 10980),
    143: (4687, 11076),
    144: (4740, 11142),
    145: (4774, 11192),
    146: (4803, 11244),
}
#: client 15.535.29's first Archer, which leaves its deploy on 120 and walks past its partner, still deploying
#: through 121
ARCHER_TRUTH = {121: (12017, 13556), 122: (12048, 13607), 123: (12079, 13658)}


def arms(waiting: str | None) -> dict:
    """The battle's calibration pinning `waiting` BY NAME; None runs the shipped build with no override of this key."""
    return dict(STAGGER) if waiting is None else {**STAGGER, KEY: json.dumps(waiting)}


def track(overrides: dict, cards: list, plays: list, card: int, ticks: range) -> dict:
    """Side 0 plays `plays`, (issue tick, catalogue id, x, y); the sorted positions of side 0's units of `card` on each
    tick in `ticks`."""
    b = royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(2, [[0, 1] * 4, [0, 1] * 4], 0, 0, None, None, [])
    now = 0
    for issue, cid, x, y in plays:
        b.step([], issue - now)
        played = b.step([(0, cid, x * SUB, y * SUB)], 1)
        assert played, f"the play of {cards[cid]} on {issue} returned nothing"
        assert played[0][1] == 0, f"the play of {cards[cid]} on {issue} was refused: {played}"
        now = issue + 1
    out = {}
    while now <= ticks.stop:
        tick = json.loads(b.state_json())
        mine = (e for e in tick["entities"] if e[TEAM] == 0 and e[SLOT] < 0 and e[CARD] == card)
        units = sorted((e[X] // SUB, e[Y] // SUB) for e in mine)
        if tick["tick"] in ticks and units:
            out[tick["tick"]] = units
        b.step([], 1)
        now += 1
    return out


def knight(waiting: str | None) -> dict:
    got = track(arms(waiting), ["Knight", "Goblins"], [(100, 0, 4499, 9500), (139, 1, 3499, 9500)], 0, range(140, 147))
    return {t: units[0] for t, units in got.items()}


def off(a: tuple, b: tuple) -> int:
    return round(((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2) ** 0.5)


def test_the_knight_steps_away_from_the_waiting_goblin():
    got = knight(SHIPPED_ARM)
    assert got[140] == KNIGHT_TRUTH[140]
    assert all(off(got[t], KNIGHT_TRUTH[t]) <= 60 for t in KNIGHT_TRUTH), got


def test_a_deploying_partner_still_keeps_its_heading():
    """The Archer walker must not be turned by its partner, which is deploying, not waiting."""
    got = track(arms(SHIPPED_ARM), ["Archer", "Knight"], [(100, 0, 12500, 13500)], 0, range(121, 124))
    assert all(ARCHER_TRUTH[t] in got.get(t, []) for t in ARCHER_TRUTH), got


def test_the_kept_arm_goes_past_the_waiting_goblin():
    """Kept: the Knight goes nearly straight past the waiting Goblin: 69 off on the first tick, 442 by the seventh."""
    got = knight(KEPT_ARM)
    assert got[140] == (4371, 10770)
    assert off(got[146], KNIGHT_TRUTH[146]) > 300


def test_the_arm_does_nothing_when_no_member_waits():
    """Under formation.STAGGER_WAIT = deploying nothing waits, so the two arms must agree."""
    plays = [(100, 0, 4499, 9500), (139, 1, 3499, 9500)]
    runs = [
        track(
            {"formation.STAGGER_WAIT": json.dumps("deploying"), KEY: json.dumps(w)},
            ["Knight", "Goblins"],
            plays,
            0,
            range(140, 147),
        )
        for w in (KEPT_ARM, SHIPPED_ARM)
    ]
    assert runs[0] == runs[1]


def test_the_shipped_build_steps_away_from_the_waiting_goblin():
    """No override at all. The compiled-in ledger ships SHIPPED_ARM, so the shipped build behaves as the client."""
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    shipped = ledger["movement"]["WAITING_HEADING"]["value"]
    assert shipped == SHIPPED_ARM, f"{KEY} ships {shipped!r}, not the arm this file names as shipped"
    got = knight(None)
    assert got[140] == KNIGHT_TRUTH[140]
    assert all(off(got[t], KNIGHT_TRUTH[t]) <= 60 for t in KNIGHT_TRUTH), got
