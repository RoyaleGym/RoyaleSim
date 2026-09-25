"""Troop taps at the own king: which are refused, where the accepted ones land (placement.TROOP_TOWER_TAPS).

WHAT THIS PINS. A deliberate-tap battery measured on client 15.535.29: a Knight, Goblins and Minions at eight own-frame
tiles around the own king, both sides, 48 casts, every member's first-frame position. Three laws, and the stagger rule:
  1. REFUSAL. The king's no-deploy block is HALF-OPEN in absolute coordinates, [min, max) on both axes, so it is not a
     mirror: side 0 refuses own (8500, 1500) and (7500, 1500), side 1 accepts the same own tiles (abs y 30500 is the
     block's open edge), and own (9500, 2500) is refused on both sides.
  2. RELOCATION. An accepted troop tap whose tile overlaps the own king is moved to the nearest free tile, the way a
     building's is (placement.ILLEGAL_TAP): own (8500, 1499) lands on own (8499, 500), side 1's own (7500, 1500) on
     own (6499, 1501). A formation is laid around the relocated point.
  3. BACK CLAMP. Side 1's ground back bound (own y 1000, formation.GROUND_Y_CLAMP) holds single units too: a side-1
     Knight at own (8500, 500) stands on own 1000.
The 16.402 corpus agrees where it can see: its side-1 Goblins at own (8500, 1500) sit exactly on the 15.535.29
battery's own 850 / 1262 / 1262 / 1000 (7 of 11 groups), and its side-1 Bombers there on own 1000 (11 of 15).

THE TEST. Each cast is played through the engine's own play path; the play must be refused or accepted as it
was on client 15.535.29, and every member's first-frame position must match within 20 native. The recorded
positions include waiting members holding still, so formation.STAGGER_WAIT rides at its measured arm too.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
#: explicit, because catalogue ids are positional
DECK = ["Goblins", "Knight", "Minions", "Archer", "Giant", "Musketeer", "Cannon", "Tesla"]
IDS = list(range(len(DECK)))
CARD = {"Goblins": 0, "Knight": 1, "Minions": 2}
KEY = "placement.TROOP_TOWER_TAPS"
STAGGER = {"formation.STAGGER_WAIT": json.dumps("client16402_untargetable_immovable")}
W, H = 18000, 32000
TOL = 20
# ENTITY_FIELDS: 1 team, 4 tower_slot, 5 x, 6 y
TEAM, SLOT, X, Y = 1, 4, 5, 6

#: (side, card, own tile tapped, accepted, members' first-frame own positions, sorted) -- the 15.535.29 battery
CASTS_15535 = [
    (0, "Knight", (8500, 1500), False, []),
    (0, "Goblins", (8500, 1500), False, []),
    (0, "Minions", (8500, 1500), False, []),
    (0, "Knight", (7500, 1500), False, []),
    (0, "Goblins", (7500, 1500), False, []),
    (0, "Minions", (7500, 1500), False, []),
    (0, "Knight", (10500, 1500), True, [(10500, 500)]),
    (0, "Goblins", (10500, 1500), True, [(9739, 250), (9739, 1261), (11261, 250), (11261, 1261)]),
    (0, "Minions", (10500, 1500), True, [(10001, 250), (10500, 1107), (10999, 250)]),
    (0, "Knight", (9500, 2500), False, []),
    (0, "Goblins", (9500, 2500), False, []),
    (0, "Minions", (9500, 2500), False, []),
    (0, "Knight", (8500, 500), True, [(8499, 500)]),
    (0, "Goblins", (8500, 500), True, [(7738, 250), (7738, 1261), (9260, 250), (9260, 1261)]),
    (0, "Minions", (8500, 500), True, [(8001, 250), (8500, 1107), (8999, 250)]),
    (0, "Knight", (8500, 1499), True, [(8499, 500)]),
    (0, "Goblins", (8500, 1499), True, [(7738, 250), (7738, 1261), (9260, 250), (9260, 1261)]),
    (0, "Minions", (8500, 1499), True, [(8001, 250), (8500, 1107), (8999, 250)]),
    (0, "Knight", (7500, 1499), True, [(7499, 500)]),
    (0, "Goblins", (7500, 1499), True, [(6738, 250), (6738, 1261), (8260, 250), (8260, 1261)]),
    (0, "Minions", (7500, 1499), True, [(7001, 250), (7500, 1107), (7999, 250)]),
    (0, "Knight", (9500, 1499), True, [(9500, 500)]),
    (0, "Goblins", (9500, 1499), True, [(8739, 250), (8739, 1261), (10261, 250), (10261, 1261)]),
    (0, "Minions", (9500, 1499), True, [(9001, 250), (9500, 1107), (9999, 250)]),
    (1, "Knight", (8500, 1500), True, [(8499, 1000)]),
    (1, "Goblins", (8500, 1500), True, [(7738, 850), (7738, 1262), (9260, 1000), (9260, 1262)]),
    (1, "Minions", (8500, 1500), True, [(8001, 250), (8500, 1107), (8999, 250)]),
    (1, "Knight", (7500, 1500), True, [(6499, 1501)]),
    (1, "Goblins", (7500, 1500), True, [(5738, 1000), (5738, 2262), (7260, 1000), (7260, 2262)]),
    (1, "Minions", (7500, 1500), True, [(6001, 1212), (6500, 2079), (6999, 1212)]),
    (1, "Knight", (10500, 1500), True, [(10500, 1000)]),
    (1, "Goblins", (10500, 1500), True, [(9739, 1000), (9739, 1262), (11261, 850), (11261, 1262)]),
    (1, "Minions", (10500, 1500), True, [(10001, 250), (10500, 1107), (10999, 250)]),
    (1, "Knight", (9500, 2500), False, []),
    (1, "Goblins", (9500, 2500), False, []),
    (1, "Minions", (9500, 2500), False, []),
    (1, "Knight", (8500, 500), True, [(8499, 1000)]),
    (1, "Goblins", (8500, 500), True, [(7738, 850), (7738, 1262), (9260, 1000), (9260, 1262)]),
    (1, "Minions", (8500, 500), True, [(8001, 250), (8500, 1107), (8999, 250)]),
    (1, "Knight", (8500, 1499), True, [(8499, 1000)]),
    (1, "Goblins", (8500, 1499), True, [(7738, 850), (7738, 1262), (9260, 1000), (9260, 1262)]),
    (1, "Minions", (8500, 1499), True, [(8001, 250), (8500, 1107), (8999, 250)]),
    (1, "Knight", (7500, 1499), True, [(7499, 1000)]),
    (1, "Goblins", (7500, 1499), True, [(6738, 850), (6738, 1262), (8260, 1000), (8260, 1262)]),
    (1, "Minions", (7500, 1499), True, [(7001, 250), (7500, 1107), (7999, 250)]),
    (1, "Knight", (9500, 1499), True, [(9500, 1000)]),
    (1, "Goblins", (9500, 1499), True, [(8739, 1000), (8739, 1262), (10261, 850), (10261, 1262)]),
    (1, "Minions", (9500, 1499), True, [(9001, 250), (9500, 1107), (9999, 250)]),
]


def own(side: int, x: int, y: int) -> tuple:
    return (x, y) if side == 0 else (W - x, H - y)


def play(side: int, card: str, tile: tuple, arm: str) -> tuple:
    """(accepted, members' own first-frame positions, sorted) for one cast through the engine's play path."""
    overrides = {KEY: json.dumps(arm), **STAGGER}
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, [])
    b.step([], 1)
    x, y = own(side, *tile)
    got = b.step([(side, CARD[card], x * SUB, y * SUB)], 1)
    accepted = bool(got) and got[0][1] == 0
    rows = [e for e in json.loads(b.state_json())["entities"] if e[TEAM] == side and e[SLOT] < 0]
    return accepted, sorted(own(side, e[X] // SUB, e[Y] // SUB) for e in rows)


@pytest.mark.parametrize(("side", "card", "tile", "accepted", "members"), CASTS_15535)
def test_a_king_area_tap_lands_as_on_client_15535(side, card, tile, accepted, members):
    got_accepted, got = play(side, card, tile, "client16402_half_open_relocate")
    verdict = "accepts" if accepted else "refuses"
    assert got_accepted == accepted, f"side {side} {card} at own {tile}: client 15.535.29 {verdict}"
    if accepted:
        assert len(got) == len(members), f"{len(got)} members, client 15.535.29 {len(members)}: {got}"
        off = [(a, b) for a, b in zip(got, members, strict=True) if abs(a[0] - b[0]) > TOL or abs(a[1] - b[1]) > TOL]
        assert not off, f"side {side} {card} at own {tile}: engine vs 15.535.29 members off by more than {TOL}: {off}"


def test_the_old_arm_is_todays_engine():
    """Today's closed block refuses side 1's own (8500, 1500), which client 15.535.29 accepts; and today a tap at
    own y 1499 is laid where it was tapped, not one tile back."""
    assert play(1, "Knight", (8500, 1500), "closed_block")[0] is False
    accepted, got = play(0, "Knight", (8500, 1499), "closed_block")
    assert accepted
    assert got[0][1] > 1000, f"the old arm moved the Knight back to row 0: {got}"
