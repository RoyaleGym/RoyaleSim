"""A COPY OF A BATTLE UNDER WAY through `Battle.reset` (py.rs): `unit_spawns`, `king_active`, `evo_counts` and
`spells_in_flight`. The engine's own rules for each are pinned in crates/royalesim/tests/scenario_copy.rs; here the
binding's half: what each argument takes, what it refuses and how a unit it puts down reads in `state_json`.
  1. a summoned unit goes down by its record's name at the level given, and reads under the card that produces it;
  2. a record the table does not hold, or one no catalogue card puts on the board, is refused;
  3. an evolved card's count is taken per deck slot, and refused past its cycle or on a card not evolved;
  4. a Fireball put on its flight is on the board and lands; a spell that does not fly is refused;
  5. each argument of the wrong shape is refused by name.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CARDS = ["Golem", "Knight", "BabyDragon", "Fireball", "Zap"]
GOLEM, KNIGHT, DRAGON, FIREBALL, ZAP = range(5)
#: Blue's deck (the Baby Dragon in slot 2, evolved when `forms` says so) and Red's.
DECKS = [[GOLEM, KNIGHT, DRAGON, FIREBALL, ZAP, KNIGHT, KNIGHT, KNIGHT], [KNIGHT] * 8]


def battle(cards=None, **kw):
    b = royalesim.Battle(cards or CARDS, [[0, 1, 2], [0, 1, 2]])
    b.reset(0, DECKS, 0, 200, [10_000, 10_000], None, [], **kw)
    return b


def units(b) -> list:
    return [e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0]


def test_a_summoned_unit_goes_down_by_name_at_its_level():
    spawns = [("Golemite", 1, 5000 * SUB, 20000 * SUB, -1, 9), ("Golemite", 1, 13000 * SUB, 20000 * SUB, -1, -1)]
    b = battle(unit_spawns=spawns)
    got = sorted(units(b), key=lambda e: e[F["x"]])
    assert [e[F["level"]] for e in got] == [9, b.card_level()], "level 9 as given; the other at the side's level"
    assert got[0][F["max_hp"]] < got[1][F["max_hp"]], "the lower level, the fewer hitpoints"
    assert {e[F["card_id"]] for e in got} == {GOLEM}, "a Golemite reads under the card that produces it"
    assert all(e[F["deploy_ticks"]] == 0 for e in got), "standing, deployed"


def test_a_unit_the_battle_cannot_carry_is_refused():
    with pytest.raises(ValueError, match="no such record"):
        battle(unit_spawns=[("NoSuchUnit", 1, 9000 * SUB, 20000 * SUB, -1, -1)])
    # a catalogue without the Golem: no card of it puts a Golemite on the board
    with pytest.raises(ValueError, match="no card of this catalogue"):
        royalesim.Battle(["Knight"], [[0, 1, 2], [0, 1, 2]]).reset(
            0, [[0] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, [],
            unit_spawns=[("Golemite", 1, 9000 * SUB, 20000 * SUB, -1, -1)])


def test_an_evo_count_is_taken_per_deck_slot():
    forms = [[0, 0, 1, 0, 0, 0, 0, 0], []]
    battle(forms=forms, evo_counts=[[0, 0, 2, 0, 0, 0, 0, 0], []])
    with pytest.raises(ValueError, match="past its cycle"):
        battle(forms=forms, evo_counts=[[0, 0, 3, 0, 0, 0, 0, 0], []])
    with pytest.raises(ValueError, match="not an evolved card"):
        battle(forms=forms, evo_counts=[[0, 1, 0, 0, 0, 0, 0, 0], []])
    with pytest.raises(ValueError, match="one count per deck card"):
        battle(forms=forms, evo_counts=[[0, 0, 2], []])


def test_a_fireball_on_its_flight_lands_and_a_spell_that_does_not_fly_is_refused():
    b = battle(spells_in_flight=[(FIREBALL, 0, 9000 * SUB, 19000 * SUB, 9000 * SUB, 22000 * SUB, -1)])
    assert json.loads(b.state_json())["spells"], "the Fireball is on its way"
    for _ in range(60):
        b.step([], 1)
        if not json.loads(b.state_json())["spells"]:
            break
    else:
        pytest.fail("the Fireball never landed")
    with pytest.raises(ValueError, match="does not fly"):
        battle(spells_in_flight=[(ZAP, 0, 9000 * SUB, 22000 * SUB, 9000 * SUB, 22000 * SUB, -1)])


@pytest.mark.parametrize(("kw", "why"), [
    ({"king_active": [True]}, "king_active must be"),
    ({"evo_counts": [[]]}, "evo_counts must be"),
    ({"unit_spawns": [("Golemite", 2, 0, 0, -1, -1)]}, "bad unit spawn team"),
    ({"spells_in_flight": [(FIREBALL, 3, 0, 0, 0, 0, -1)]}, "bad team"),
])
def test_an_argument_of_the_wrong_shape_is_refused(kw, why):
    with pytest.raises(ValueError, match=why):
        battle(**kw)


def king_target_ticks(**kw) -> int:
    """Of 20 ticks with a red Knight 4500 from Blue's king, how many the king has a target on."""
    b = royalesim.Battle(CARDS, [[0, 1, 2], [0, 1, 2]])
    b.reset(0, DECKS, 0, 200, [10_000, 10_000], None, [(1, KNIGHT, 9000 * SUB, 7500 * SUB, -1)], **kw)
    on = 0
    for _ in range(20):
        b.step([], 1)
        king = next(e for e in json.loads(b.state_json())["entities"] if e[F["team"]] == 0 and e[F["tower_slot"]] == 0)
        on += king[F["target_uid"]] >= 0
    return on


def test_a_woken_king_targets_and_a_sleeping_one_does_not():
    assert king_target_ticks() == 0, "the control: a fresh king sleeps"
    assert king_target_ticks(king_active=[True, False]) > 0, "the woken king targets the Knight"
