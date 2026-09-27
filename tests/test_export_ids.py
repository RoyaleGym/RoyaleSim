"""Every unit on the board reports a card, and only a crown tower's shot reads as a tower's.

WHAT THIS PINS. `state_json`'s projectile rows promise that firer_card_id -1 means a crown
tower. That promise rested on the catalogue map returning -1 only for towers, which was
false: a formation's SECOND summon (the Rascals' RascalGirl beside the RascalBoy) was never
collected, so it reported card -1 in every entity row and firer -1 on every shot, and a viewer
drew RascalGirls' shots as tower bolts flying from mid-field. The engine's own export test
checked that tower shots read -1 and never that ONLY towers do.

PLANT. `unit_refs_skips_new_paths` (card.rs `unit_refs`, built into the module with maturin under the cfg) drops the
second summon and the transformation from the one enumeration: both tests here go red.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

#: explicit, because catalogue ids are positional
DECK = ["Rascals", "Knight", "Archer", "Musketeer", "Giant", "Minions", "Cannon", "Zap"]
IDS = list(range(len(DECK)))
RASCALS, KNIGHT = DECK.index("Rascals"), DECK.index("Knight")
TILE = 18_000


def test_a_second_summon_reports_its_card_and_its_shots_are_not_a_towers():
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    # A blue Knight mid-field, and red DEPLOYS the Rascals from hand in front of it. Deployed,
    # not scenario-spawned: a scenario spawn places the card's primary unit alone (the
    # RascalBoy), and the RascalGirls -- the units this test is about -- exist only when the
    # card is played. Hand slot 0 is the Rascals: no shuffle, so the hand is the deck's first four.
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, [(0, KNIGHT, 9 * TILE, 14 * TILE, -1)])
    [(_card, reason, *_)] = b.step([(1, 0, 9 * TILE, 19 * TILE)], 1)
    assert royalesim.DEPLOY_REASONS[reason] == "OK", royalesim.DEPLOY_REASONS[reason]
    rascal_shots = 0
    for _ in range(120):
        b.step([], 1)
        st = json.loads(b.state_json())
        for e in st["entities"]:
            if e[4] < 0:  # not a crown tower
                assert e[3] >= 0, f"a unit on the board reports no card (-1), so it cannot be named: {e}"
        for p in st["projectiles"]:
            firer = p[7]
            assert firer != -2, f"a projectile fired in this battle reports its firer as NOT RECORDED: {p}"
            if p[0] == 1 and firer == RASCALS:
                rascal_shots += 1
            if p[0] == 1:
                # red has no crown tower in range of a Knight at y=14, so a red -1 is a lie
                assert firer != -1, f"a red shot mid-field reads as a crown tower's: {p}"
    assert rascal_shots > 0, "no RascalGirl shot was seen in 120 ticks, so the firer was never checked"


#: the health-threshold transformation (crates/royalesim/tests/transform.rs): catalogue ids are positional
TRANSFORM_DECK = ["MovingCannon", "GoblinDemolisher", "Knight", "Archer", "Musketeer", "Giant", "Minions", "Zap"]
CART, DEMOLISHER = TRANSFORM_DECK.index("MovingCannon"), TRANSFORM_DECK.index("GoblinDemolisher")
SUB = 18


def test_a_transformed_unit_reports_the_card_it_was_played_as():
    """The Cannon Cart becomes BrokenCannon and the Goblin Demolisher its kamikaze form: the same entity, as another
    row. Neither row is a card, so each must report the card whose transformation reaches it (py.rs `ids_of_indices`,
    through card.rs `unit_refs`), or a viewer cannot name it. A Red Cannon Cart just above half its hitpoints in front
    of the Blue engine-Left princess tower, and a Red Goblin Demolisher likewise before the Blue engine-Right one: the
    towers' shots take both over their lines."""
    ids = list(range(len(TRANSFORM_DECK)))
    b = royalesim.Battle(card_names=TRANSFORM_DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    b.reset(
        0,
        [ids, ids],
        0,
        200,
        [10_000, 10_000],
        None,
        [(1, CART, 3500 * SUB, 12500 * SUB, 905), (1, DEMOLISHER, 14500 * SUB, 12000 * SUB, 651)],
    )
    cards_by_uid: dict[int, set[int]] = {}
    kinds_by_uid: dict[int, set[int]] = {}
    for _ in range(120):
        b.step([], 1)
        st = json.loads(b.state_json())
        for e in st["entities"]:
            if e[4] >= 0:  # a crown tower
                continue
            assert e[3] >= 0, f"a unit on the board reports no card (-1), so it cannot be named: {e}"
            cards_by_uid.setdefault(e[0], set()).add(e[3])
            kinds_by_uid.setdefault(e[0], set()).add(e[2])
    red = {uid: c for uid, c in cards_by_uid.items() if c & {CART, DEMOLISHER}}
    assert sorted(red.values(), key=min) == [{CART}, {DEMOLISHER}], f"each unit keeps the card it was played as: {red}"
    cart = next(uid for uid, c in red.items() if c == {CART})
    assert kinds_by_uid[cart] == {0, 1}, "precondition: the Cannon Cart broke into its cannon (troop, then building)"
