"""The catalogue names its champions (CATALOGUE_FIELDS "champion", card.rs `CardDb::is_champion`).

A deck builder needs to tell a champion from the catalogue alone: a deck holds at most 3 ability buttons a side, and the
ladder allows one champion. Pinned: the column is exactly the cards whose deck entry gets an ability button with no form
chosen, read off the battle itself.
"""

import json

import royalesim

COL = {name: i for i, name in enumerate(royalesim.CATALOGUE_FIELDS)}
PLAIN = ["Knight", "Archer", "Giant", "Minions", "Fireball", "Zap", "Cannon", "Musketeer"]


def test_the_champion_column_is_the_cards_a_battle_gives_a_button():
    assert "champion" in COL, royalesim.CATALOGUE_FIELDS
    b = royalesim.Battle(card_names=None, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    cat = json.loads(b.catalogue_json())
    plain = [i for i, row in enumerate(cat) if row[0] in PLAIN]
    by_battle = set()
    for i, row in enumerate(cat):
        deck = [i, *[p for p in plain if p != i][:7]]
        try:
            b.reset(0, [deck, deck], 0, 0, None, None, [])
        except ValueError:
            continue
        if json.loads(b.state_json())["players"][0]["abilities"]:
            by_battle.add(row[0])
    by_column = {row[0] for row in cat if row[COL["champion"]]}
    assert by_column == by_battle, (sorted(by_column), sorted(by_battle))
    assert len(by_column) >= 8, f"the column names only {sorted(by_column)}"
