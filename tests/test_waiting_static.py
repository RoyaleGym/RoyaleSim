"""A member waiting out its formation stagger is a static obstacle to a walker's avoidance scan
(movement.WAITING_HEADING = static_obstacle).

WHAT THIS PINS. On client 15.535.29, a Knight walks up its lane and a Goblins deploy lands beside it. Three of the four
Goblins wait out their stagger, and one of them lands 365 behind the Knight's shoulder (side 0, and the same scene
mirrored on side 1). On the landing tick the Knight turns away. It then KEEPS its turn while the waiting Goblin stays
in front of its look-ahead point: a static obstacle moves a running avoidance offset back up by 20 each tick, where a
moving blocker leaves it to decay by 10 a tick. Under zeroed (the waiting Goblin as a moving blocker whose heading
does not count) the Knight is exact on the landing tick, 4 (side 0) or 5 (side 1) off on the next, and 80 or 161 off
twelve ticks later. Under static_obstacle it is on the recorded track through the end of the recording, tick 152.
The Goblins are played on their tap's tile centre, (3500, 9500) and (3500, 21500): that puts the four members on the
points client 15.535.29 recorded, where a play at x 3499 puts them one native left.

The same reading holds on the 16.402 corpus: of 2,300 unit-steps with a waiting member nearby, replayed from the
client's recorded state, the static reading gives the client's position on 2,297 and zeroed on 2,141.

WHY THE CONTROLS ARE HERE. static_obstacle must not touch a DEPLOYING member (state 4, not waiting): the Archer walker
passes its deploying partner on the recorded track. Under formation.STAGGER_WAIT = deploying nothing waits, so the
three arms must agree. And zeroed must be today's engine, which is also what shows the Knight scenes have a point.

WHICH ARM. zeroed is the SHIPPED arm until its flip; static_obstacle is the measured one. The tests pin each arm BY NAME
through the battle's calibration.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "movement.WAITING_HEADING"
NEW_ARM, SHIPPED_ARM, KEPT_ARM = "static_obstacle", "zeroed", "kept"
STAGGER = {"formation.STAGGER_WAIT": json.dumps("client16402_untargetable_immovable")}
# ENTITY_FIELDS: 1 team, 3 card_id, 4 tower_slot, 5 x, 6 y
TEAM, CARD, SLOT, X, Y = 1, 3, 4, 5, 6
#: per side: the plays, (issue tick, hand slot, x, y), and the walking Knight client 15.535.29 recorded, native
#: units, from the tick the Goblins land (issued on 139) to the end of the recording
KNIGHT = {
    0: {
        "plays": [(100, 0, 4499, 9500), (139, 1, 3500, 9500)],
        "truth": {
            140: (4431, 10735), 141: (4518, 10859), 142: (4611, 10980), 143: (4687, 11076), 144: (4740, 11142),
            145: (4774, 11192), 146: (4803, 11244), 147: (4825, 11301), 148: (4839, 11359), 149: (4857, 11417),
            150: (4870, 11475), 151: (4875, 11535), 152: (4875, 11595),
        },
        # today's engine (zeroed) on 141 and 152
        "zeroed": {141: (4516, 10862), 152: (4803, 11631)},
    },
    1: {
        "plays": [(100, 0, 4499, 23499), (139, 1, 3500, 21500)],
        "truth": {
            140: (4464, 22440), 141: (4591, 22476), 142: (4724, 22496), 143: (4859, 22501), 144: (4993, 22494),
            145: (5104, 22470), 146: (5179, 22430), 147: (5231, 22382), 148: (5266, 22329), 149: (5291, 22275),
            150: (5315, 22221), 151: (5336, 22165), 152: (5352, 22107),
        },
        "zeroed": {141: (4588, 22472), 152: (5208, 22035)},
    },
}
#: client 15.535.29's first Archer, which leaves its deploy on 120 and walks past its partner, still deploying
#: through 121
ARCHER_TRUTH = {121: (12017, 13556), 122: (12048, 13607), 123: (12079, 13658)}


def arms(waiting: str) -> dict:
    return {**STAGGER, KEY: json.dumps(waiting)}


def track(overrides: dict, cards: list, side: int, plays: list, card: int, ticks: range) -> dict:
    """`side` plays `plays`, (issue tick, hand slot, x, y); the sorted positions of that side's units of `card` on each
    tick in `ticks`."""
    b = royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(2, [[0, 1] * 4, [0, 1] * 4], 0, 0, None, None, [])
    now = 0
    for issue, slot, x, y in plays:
        b.step([], issue - now)
        played = b.step([(side, slot, x * SUB, y * SUB)], 1)
        assert played, f"the play of {cards[slot]} on {issue} returned nothing"
        assert played[0][1] == 0, f"the play of {cards[slot]} on {issue} was refused: {played}"
        now = issue + 1
    out = {}
    while now <= ticks.stop:
        tick = json.loads(b.state_json())
        mine = (e for e in tick["entities"] if e[TEAM] == side and e[SLOT] < 0 and e[CARD] == card)
        units = sorted((e[X] // SUB, e[Y] // SUB) for e in mine)
        if tick["tick"] in ticks and units:
            out[tick["tick"]] = units
        b.step([], 1)
        now += 1
    return out


def knight(overrides: dict, side: int) -> dict:
    got = track(overrides, ["Knight", "Goblins"], side, KNIGHT[side]["plays"], 0, range(140, 153))
    return {t: units[0] for t, units in got.items()}


def off(a: tuple, b: tuple) -> int:
    return round(((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2) ** 0.5)


@pytest.mark.parametrize("side", [0, 1])
def test_the_knight_keeps_its_turn_past_the_waiting_goblin(side):
    got = knight(arms(NEW_ARM), side)
    truth = KNIGHT[side]["truth"]
    missed = {t: (got.get(t), truth[t]) for t in truth if got.get(t) != truth[t]}
    assert not missed, missed


def test_a_deploying_partner_is_not_an_obstacle():
    """The Archer walker must not be turned by its partner, which is deploying, not waiting."""
    got = track(arms(NEW_ARM), ["Archer", "Knight"], 0, [(100, 0, 12500, 13500)], 0, range(121, 124))
    assert all(ARCHER_TRUTH[t] in got.get(t, []) for t in ARCHER_TRUTH), got


@pytest.mark.parametrize("side", [0, 1])
def test_zeroed_is_todays_engine(side):
    """zeroed: exact on the landing tick, then the running offset decays and the Knight drifts off the track."""
    got = knight(arms(SHIPPED_ARM), side)
    truth, today = KNIGHT[side]["truth"], KNIGHT[side]["zeroed"]
    assert got[140] == truth[140]
    assert (got[141], got[152]) == (today[141], today[152]), got
    assert off(got[152], truth[152]) > 60


def test_the_arms_agree_when_no_member_waits():
    """Under formation.STAGGER_WAIT = deploying nothing waits, so the three arms must agree."""
    runs = [
        track(
            {"formation.STAGGER_WAIT": json.dumps("deploying"), KEY: json.dumps(w)},
            ["Knight", "Goblins"],
            0,
            KNIGHT[0]["plays"],
            0,
            range(140, 153),
        )
        for w in (KEPT_ARM, SHIPPED_ARM, NEW_ARM)
    ]
    assert runs[0] == runs[1] == runs[2]
