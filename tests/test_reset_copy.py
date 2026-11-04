"""A COPY OF A BATTLE UNDER WAY through `Battle.reset` (py.rs): `unit_spawns`, `king_wake_ms`, `evo_counts`,
`last_played` and `spells_in_flight`, and `Battle.spell_in_flight_kind` and `Battle.unit_spawn_kind`. The engine's own
rules for each are pinned in crates/royalesim/tests/scenario_copy.rs; here the binding's half: what each argument
takes, what it refuses and how what it sets reads back.
  1. a summoned unit goes down by its record's name at the level given, and reads under the card that produces it; a
     form goes down by its card's name and form, as a deck names it, or by its record's name and own form, alike;
  2. a record the table does not hold, one no catalogue card puts on the board, a form the record is not, a rider and
     a hero's record a press makes are refused, and a copy's builder can ask each record's kind first;
  3. an evolved card's count is taken per deck slot, and refused past its cycle or on a card not evolved;
  4. a king's wake clock and a Mirror's last play are taken, and read back in `state_json` and `mirror_target`;
  5. a Fireball put on its flight is on the board and lands; a record that does not fly is refused by its kind, and a
     copy's builder can ask the kind first, by form;
  6. each argument of the wrong shape is refused by name.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CARDS = ["Golem", "Knight", "BabyDragon", "Fireball", "Zap", "Log", "Snowball", "Ghost", "Musketeer", "Mirror"]
GOLEM, KNIGHT, DRAGON, FIREBALL, ZAP, LOG, SNOWBALL, GHOST, MUSKETEER, MIRROR = range(10)
#: Blue's deck (the Baby Dragon in slot 2, evolved when `forms` says so) and Red's.
DECKS = [[GOLEM, KNIGHT, DRAGON, FIREBALL, ZAP, MIRROR, KNIGHT, KNIGHT], [KNIGHT] * 8]


def battle(cards=None, spawns=(), **kw):
    b = royalesim.Battle(cards or CARDS, [[0, 1, 2], [0, 1, 2]])
    b.reset(0, DECKS, 0, 200, [10_000, 10_000], None, list(spawns), **kw)
    return b


def units(b) -> list:
    return [e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0]


def test_a_summoned_unit_goes_down_by_name_at_its_level():
    spawns = [("Golemite", 1, 5000 * SUB, 20000 * SUB, -1, 9, 0), ("Golemite", 1, 13000 * SUB, 20000 * SUB, -1, -1, 0)]
    b = battle(unit_spawns=spawns)
    got = sorted(units(b), key=lambda e: e[F["x"]])
    assert [e[F["level"]] for e in got] == [9, b.card_level()], "level 9 as given; the other at the side's level"
    assert got[0][F["max_hp"]] < got[1][F["max_hp"]], "the lower level, the fewer hitpoints"
    assert {e[F["card_id"]] for e in got} == {GOLEM}, "a Golemite reads under the card that produces it"
    assert all(e[F["deploy_ticks"]] == 0 for e in got), "standing, deployed"


def unit_types(b) -> list:
    return json.loads(b.unit_types_json())


def test_a_form_goes_down_as_a_deck_names_it_or_as_its_row_reads_it():
    for name in ("Musketeer", "Musketeer_EV1"):
        b = battle(unit_spawns=[(name, 0, 9000 * SUB, 8000 * SUB, -1, -1, 1)])
        (e,) = units(b)
        assert unit_types(b)[e[F["unit_type"]]] == "Musketeer_EV1", name
        assert e[F["card_id"]] == MUSKETEER, f"{name}: the evolved Musketeer reads under the Musketeer"


@pytest.mark.parametrize(("unit", "form", "why"), [
    ("NoSuchUnit", 0, "no such record"),
    ("Musketeer_EV1", 0, "is a form 1 record, not form 0"),
    ("Golem", 2, "has no form 2"),
    ("Musketeer", 7, "unknown form 7"),
    ("GhostOverlay", 0, "a rider is not put down alone"),
    ("MusketeerTurret", 2, "a hero's press makes it"),
])
def test_a_unit_that_cannot_stand_alone_is_refused(unit, form, why):
    with pytest.raises(ValueError, match=why):
        battle(unit_spawns=[(unit, 1, 9000 * SUB, 20000 * SUB, -1, -1, form)])


def test_a_copys_builder_can_ask_each_units_kind_first():
    b = battle()
    asks = [("Knight", 0), ("Golemite", 0), ("Musketeer", 1), ("Musketeer_EV1", 1), ("Musketeer", 2), ("GhostOverlay", 0),
            ("MusketeerTurret", 2), ("Fireball", 0)]
    assert [b.unit_spawn_kind(*a) for a in asks] == ["stands", "stands", "fresh", "fresh", "fresh", "rider", "by_press",
                                                     "not_a_unit"]
    knights = royalesim.Battle(["Knight"], [[0, 1, 2], [0, 1, 2]])
    assert knights.unit_spawn_kind("Golemite") == "not_in_catalogue"
    with pytest.raises(ValueError, match="is a form 1 record, not form 0"):
        b.unit_spawn_kind("Musketeer_EV1")


def test_a_unit_no_catalogue_card_puts_down_is_refused():
    # a catalogue without the Golem: no card of it puts a Golemite on the board
    with pytest.raises(ValueError, match="no card of this catalogue"):
        royalesim.Battle(["Knight"], [[0, 1, 2], [0, 1, 2]]).reset(
            0, [[0] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, [],
            unit_spawns=[("Golemite", 1, 9000 * SUB, 20000 * SUB, -1, -1, 0)])


def test_an_evo_count_is_taken_per_deck_slot():
    forms = [[0, 0, 1, 0, 0, 0, 0, 0], []]
    battle(forms=forms, evo_counts=[[0, 0, 2, 0, 0, 0, 0, 0], []])
    with pytest.raises(ValueError, match="past its cycle"):
        battle(forms=forms, evo_counts=[[0, 0, 3, 0, 0, 0, 0, 0], []])
    with pytest.raises(ValueError, match="not an evolved card"):
        battle(forms=forms, evo_counts=[[0, 1, 0, 0, 0, 0, 0, 0], []])
    with pytest.raises(ValueError, match="one count per deck card"):
        battle(forms=forms, evo_counts=[[0, 0, 2], []])


def players(b) -> list:
    return json.loads(b.state_json())["players"]


def test_a_kings_wake_clock_and_a_mirrors_last_play_read_back():
    fresh = battle()
    assert [p["king_wake_ms"] for p in players(fresh)] == [-1, -1], "no trigger in a fresh battle"
    assert fresh.mirror_target(0) == -1
    b = battle(king_wake_ms=[100_000, 1000], last_played=[FIREBALL, -1])
    got = players(b)
    assert [p["king_wake_ms"] for p in got] == [100_000, 1000]
    assert [p["king_active"] for p in got] == [True, False], "awake past the delay; waking short of it"
    assert (b.mirror_target(0), b.mirror_target(1)) == (FIREBALL, -1)


def king_target_ticks(**kw) -> int:
    """Of 20 ticks with a red Knight 4500 from Blue's king, how many the king has a target on."""
    b = battle(spawns=[(1, KNIGHT, 9000 * SUB, 7500 * SUB, -1)], **kw)
    on = 0
    for _ in range(20):
        b.step([], 1)
        king = next(e for e in json.loads(b.state_json())["entities"] if e[F["team"]] == 0 and e[F["tower_slot"]] == 0)
        on += king[F["target_uid"]] >= 0
    return on


def test_an_awake_king_targets_and_a_sleeping_one_does_not():
    assert king_target_ticks() == 0, "the control: a fresh king sleeps"
    assert king_target_ticks(king_wake_ms=[100_000, -1]) > 0, "the awake king targets the Knight"


def test_a_fireball_on_its_flight_lands():
    b = battle(spells_in_flight=[(FIREBALL, 0, 0, 9000 * SUB, 19000 * SUB, 9000 * SUB, 22000 * SUB, -1, 0)])
    assert json.loads(b.state_json())["spells"], "the Fireball is on its way"
    for _ in range(60):
        b.step([], 1)
        if not json.loads(b.state_json())["spells"]:
            break
    else:
        pytest.fail("the Fireball never landed")
    with pytest.raises(ValueError, match=r"does not fly \(at_once\)"):
        battle(spells_in_flight=[(ZAP, 0, 0, 9000 * SUB, 22000 * SUB, 9000 * SUB, 22000 * SUB, -1, 0)])
    with pytest.raises(ValueError, match=r"does not fly \(rolls\)"):
        battle(spells_in_flight=[(SNOWBALL, 1, 0, 9000 * SUB, 22000 * SUB, 9000 * SUB, 22000 * SUB, -1, 0)])


SP = {name: i for i, name in enumerate(royalesim.SPELL_FIELDS)}


def spell_rows(b) -> list:
    return json.loads(b.state_json())["spells"]


def test_a_spell_row_says_its_level_and_form():
    assert list(royalesim.SPELL_FIELDS[-3:]) == ["ticks_flown", "level", "form"], "the newer columns come last"
    b = battle(spells_in_flight=[(FIREBALL, 0, 0, 9000 * SUB, 19000 * SUB, 9000 * SUB, 22000 * SUB, 9, 0)])
    (row,) = spell_rows(b)
    assert (row[SP["level"]], row[SP["form"]]) == (9, 0), "the level given; the card itself"
    # an evolved Giant Snowball played from the hand: its objects read form 1, under the base card's id
    deck = [SNOWBALL, KNIGHT, KNIGHT, KNIGHT, KNIGHT, KNIGHT, KNIGHT, KNIGHT]
    e = royalesim.Battle(CARDS, [[0, 1, 2], [0, 1, 2]])
    e.reset(0, [deck, [KNIGHT] * 8], 0, 200, [10_000, 10_000], None, [], forms=[[1] + [0] * 7, []],
            evo_counts=[[2] + [0] * 7, []])
    e.step([(0, 0, 9500 * SUB, 22500 * SUB)], 1)
    forms = set()
    for _ in range(40):
        forms |= {(r[SP["card_id"]], r[SP["form"]]) for r in spell_rows(e)}
        e.step([], 1)
    assert (SNOWBALL, 1) in forms, f"the evolved Snowball's objects: {sorted(forms)}"


def test_a_copys_builder_can_ask_what_flies_first():
    b = battle()
    kinds = [b.spell_in_flight_kind(c) for c in (FIREBALL, SNOWBALL, LOG, ZAP, KNIGHT)]
    assert kinds == ["flies", "flies", "rolls", "at_once", "not_a_spell"]
    assert b.spell_in_flight_kind(SNOWBALL, 1) == "rolls", "the Evo Giant Snowball's capture rolls"
    with pytest.raises(ValueError, match="unknown card id"):
        b.spell_in_flight_kind(99)
    with pytest.raises(ValueError, match="has no form 1"):
        b.spell_in_flight_kind(FIREBALL, 1)
    with pytest.raises(ValueError, match="unknown form"):
        b.spell_in_flight_kind(FIREBALL, 7)


@pytest.mark.parametrize(("kw", "why"), [
    ({"king_wake_ms": [0]}, "king_wake_ms must be"),
    ({"last_played": [0]}, "last_played must be"),
    ({"evo_counts": [[]]}, "evo_counts must be"),
    ({"unit_spawns": [("Golemite", 2, 0, 0, -1, -1, 0)]}, "bad unit spawn team"),
    ({"spells_in_flight": [(FIREBALL, 0, 3, 0, 0, 0, 0, -1, 0)]}, "bad team"),
])
def test_an_argument_of_the_wrong_shape_is_refused(kw, why):
    with pytest.raises(ValueError, match=why):
        battle(**kw)
