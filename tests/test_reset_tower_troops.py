"""Per-side crown tower troop selection through Battle.reset.

The existing tower_troops_unloaded plant makes the selection assertions fail.
"""

import json

import pytest
import royalesim

CARDS = ["Knight", "Giant", "Archer", "Musketeer", "Cannon", "Fireball", "Zap", "Skeletons"]


def battle():
    return royalesim.Battle(
        CARDS,
        [[0, 1, 2], [0, 1, 2]],
        level=11,
        tower_level=11,
        calibration_overrides={"cards.CARD_TABLE": json.dumps("client160635007_20261103")},
    )


def reset(b, **kwargs):
    ids = {row[0]: i for i, row in enumerate(json.loads(b.catalogue_json()))}
    deck = [ids[name] for name in CARDS]
    b.reset(1, [deck, deck], 0, 0, None, None, [], **kwargs)


def unit_names(b):
    types = json.loads(b.unit_types_json())
    column = royalesim.ENTITY_FIELDS.index("unit_type")
    return [types[row[column]] for row in json.loads(b.state_json())["entities"]]


@pytest.mark.parametrize("troop", ["Cannoneer", "DaggerDuchess", "ChefTower"])
def test_reset_selects_each_tower_troop_on_one_side(troop):
    b = battle()
    reset(b, tower_troops=[troop, None])
    names = unit_names(b)
    assert names.count(troop) == 2
    assert names.count("PrincessTower") == 2
    assert names.count("KingTower") == 2


def test_default_selection_preserves_the_original_battle_hash():
    b = battle()
    reset(b)
    original = b.state_hash()
    reset(b, tower_troops=[None, None])
    assert b.state_hash() == original
    reset(b, tower_troops=["Cannoneer", "DaggerDuchess"])
    assert b.state_hash() != original
    assert unit_names(b).count("PrincessTower") == 0


@pytest.mark.parametrize("troops", [[None], ["Knight", None]])
def test_invalid_tower_selection_is_refused(troops):
    with pytest.raises(ValueError, match="tower"):
        reset(battle(), tower_troops=troops)
