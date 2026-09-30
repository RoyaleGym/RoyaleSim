"""`Battle.unit_hitpoints(card_id, level)`: every unit a card puts on the board, as (role, unit name, hitpoints).

WHAT IS PINNED:
  1. for every card of the default catalogue at level 11, the first row is ("own", the unit's name, the catalogue's
     hitpoints), and a spell (hitpoints 0 in the catalogue) has no own row; every role is one of UNIT_ROLES;
  2. a card with a death spawn and one with a second summon list them: the Golem's Golemite, the Rascals' Girl; the
     Tri Wizards list their three units at the client's hitpoints (755, 714, 688 at level 11);
  3. the hitpoints are the battle's card data: under cards.CLIENT16402_VALUES = client16402 (shipped) the Goblin
     Cage's death spawn, the Goblin Brawler, reads 1121 at level 11 (Hitpoints 438), under "none" 1080 (the tables'
     422); the Ice Golem's own row reads 1228 and 1315 (480 and 514), the catalogue's under each;
  4. a level a card's ladder lacks raises ValueError naming it, and so does an unknown card id.
"""

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SLOTS = [[0, 1, 2], [0, 1, 2]]


def _battle(**kw):
    b = royalesim.Battle(None, SLOTS, **kw)
    ids = {row[0]: k for k, row in enumerate(json.loads(b.catalogue_json()))}
    return b, ids


def test_the_own_row_is_the_catalogues_hitpoints():
    b, _ = _battle()
    rows = json.loads(b.catalogue_json())
    assert len(rows) == 133
    for cid, row in enumerate(rows):
        got = b.unit_hitpoints(cid, 11)
        assert all(r[0] in royalesim.UNIT_ROLES for r in got), (row[0], got)
        if got and got[0][0] == "own":
            assert got[0][2] == row[6], (row[0], got[0], row[6])
        else:
            assert row[6] == 0, (row[0], got)


def test_a_death_spawn_a_second_summon_and_the_tri_wizards_are_listed():
    b, ids = _battle()
    assert b.unit_hitpoints(ids["Golem"], 11) == [("own", "Golem", 5120), ("death_spawn", "Golemite", 1039)]
    assert b.unit_hitpoints(ids["Rascals"], 11) == [("own", "RascalBoy", 1832), ("second_summon", "RascalGirl", 261)]
    assert b.unit_hitpoints(ids["TriWizards"], 11) == [
        ("own", "TriWizard", 755),
        ("second_summon", "ElectroWizard", 714),
        ("second_summon", "IceWizard", 688),
    ]
    # the level asked for, not the battle's
    assert b.unit_hitpoints(ids["Golem"], 13) == [("own", "Golem", 6180), ("death_spawn", "Golemite", 1254)]


def _arm(name: str) -> dict:
    key = "cards.CLIENT16402_VALUES"
    value = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)["cards"]["CLIENT16402_VALUES"]["value"]
    return {key: json.dumps({**value, "arm": name})}


def test_the_hitpoints_are_the_battles_card_data():
    shipped, ids = _battle()
    tables, _ = _battle(calibration_overrides=_arm("none"))
    brawler = [r for r in shipped.unit_hitpoints(ids["GoblinCage"], 11) if r[0] == "death_spawn"]
    assert brawler == [("death_spawn", "GoblinBrawler", 1121)]
    brawler = [r for r in tables.unit_hitpoints(ids["GoblinCage"], 11) if r[0] == "death_spawn"]
    assert brawler == [("death_spawn", "GoblinBrawler", 1080)]
    own = [b.unit_hitpoints(ids["IceGolemite"], 11)[0] for b in (shipped, tables)]
    assert own == [("own", "IceGolemite", 1228), ("own", "IceGolemite", 1315)]
    for b in (shipped, tables):
        rows = json.loads(b.catalogue_json())
        assert b.unit_hitpoints(ids["IceGolemite"], 11)[0][2] == rows[ids["IceGolemite"]][6]


def test_a_missing_level_and_an_unknown_card_raise():
    b, ids = _battle()
    with pytest.raises(ValueError, match="level 99"):
        b.unit_hitpoints(ids["Knight"], 99)
    with pytest.raises(ValueError, match="unknown card id"):
        b.unit_hitpoints(10_000, 11)
