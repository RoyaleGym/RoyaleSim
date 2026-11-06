"""A COPY OF A BATTLE UNDER WAY through `Battle.reset` (py.rs): `unit_spawns`, `unit_deploy_ticks`, `king_wake_ms`,
`evo_counts`, `last_played`, `spells_in_flight` and `projectiles_in_flight`, and `Battle.spell_in_flight_kind`,
`Battle.unit_spawn_kind` and `Battle.projectile_in_flight_kind`. The engine's own rules for each are pinned in
crates/royalesim/tests/scenario_copy.rs; here the binding's half: what each argument takes, what it refuses and how
what it sets reads back.
  1. a summoned unit goes down by its record's name at the level given, and reads under the card that produces it; a
     form goes down by its card's name and form, as a deck names it, or by its record's name and own form, alike;
  2. a record the table does not hold, one no catalogue card puts on the board, a form the record is not, a rider and
     a hero's record a press makes are refused, and a copy's builder can ask each record's kind first;
  3. an evolved card's count is taken per deck slot, and refused past its cycle or on a card not evolved;
  4. a king's wake clock and a Mirror's last play are taken, and read back in `state_json` and `mirror_target`;
  5. a Fireball put on its flight is on the board and lands; a record that does not fly is refused by its kind, and a
     copy's builder can ask the kind first, by form;
  6. each argument of the wrong shape is refused by name;
  7. a unit's deploy ticks left read back in its row and run down to its deploy;
  8. a shot goes down from the copy's own firer at the copy's own target (a unit_spawns entry, `spawns` ahead of them,
     or a crown tower by its tower index), reads back in `projectiles` and lands where the copy without it lands
     nothing; one whose target is gone goes down with None; a target of the firer's own side is refused; a copy's
     builder can ask the kind first.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
TILE = 1000 * SUB
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CARDS = ["Golem", "Knight", "BabyDragon", "Fireball", "Zap", "Log", "Snowball", "GoblinGiant", "Musketeer", "Mirror"]
GOLEM, KNIGHT, DRAGON, FIREBALL, ZAP, LOG, SNOWBALL, GOBLIN_GIANT, MUSKETEER, MIRROR = range(10)
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


def test_a_form_goes_down_as_a_deck_names_it_or_by_its_records_name():
    for name, form in (("Musketeer", 1), ("Musketeer_EV1", 1), ("Musketeer_EV1", 0)):
        b = battle(unit_spawns=[(name, 0, 9000 * SUB, 8000 * SUB, -1, -1, form)])
        (e,) = units(b)
        assert unit_types(b)[e[F["unit_type"]]] == "Musketeer_EV1", (name, form)
        assert e[F["card_id"]] == MUSKETEER, f"{name}: the evolved Musketeer reads under the Musketeer"
        assert b.snipe_states() == [(e[F["uid"]], 3)], "put down as just played: all three snipes left"


@pytest.mark.parametrize(("unit", "form", "why"), [
    ("NoSuchUnit", 0, "no such record"),
    ("Musketeer_EV1", 2, "is a form 1 record, not form 2"),
    ("Golem", 2, "has no form 2"),
    ("Musketeer", 7, "unknown form 7"),
    ("SpearGoblinGiant", 0, "a rider is not put down alone"),
    ("MusketeerTurret", 2, "a hero's press makes it"),
])
def test_a_unit_that_cannot_stand_alone_is_refused(unit, form, why):
    with pytest.raises(ValueError, match=why):
        battle(unit_spawns=[(unit, 1, 9000 * SUB, 20000 * SUB, -1, -1, form)])


def test_a_copys_builder_can_ask_each_units_kind_first():
    b = battle()
    asks = [("Knight", 0), ("Golemite", 0), ("Musketeer", 1), ("Musketeer_EV1", 0), ("Musketeer", 2),
            ("SpearGoblinGiant", 0), ("MusketeerTurret", 0), ("Fireball", 0)]
    assert [b.unit_spawn_kind(*a) for a in asks] == ["stands", "stands", "fresh", "fresh", "fresh", "rider",
                                                     "by_press", "not_a_unit"]
    knights = royalesim.Battle(["Knight"], [[0, 1, 2], [0, 1, 2]])
    # a record's name and a unit row's unit_type name alike (the Minions card's Minion names no record)
    assert [knights.unit_spawn_kind(n) for n in ("Golemite", "Minion")] == ["not_in_catalogue"] * 2
    with pytest.raises(ValueError, match="is a form 2 record, not form 1"):
        b.unit_spawn_kind("MusketeerTurret", 1)


def test_a_unit_rows_unit_type_name_puts_it_back():
    # a hero form's unit_type is its unit name (KnightHero), not its record's (Knight_hero); a Skeletons card's
    # skeleton's is Skeleton, which names no unit this catalogue puts down but the card's own
    def put(cards, decks, spawns=(), unit_spawns=()):
        b = royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]])
        b.reset(0, decks, 0, 200, [10_000, 10_000], None, list(spawns), unit_spawns=list(unit_spawns))
        return b

    cases = [(CARDS, DECKS, [], [("Knight", 0, 9000 * SUB, 8000 * SUB, -1, -1, 2)]),
             (["Skeletons", "Knight"], [[0] * 8, [1] * 8], [(0, 0, 9000 * SUB, 8000 * SUB, -1)], [])]
    for cards, decks, spawns, unit_spawns in cases:
        b = put(cards, decks, spawns, unit_spawns)
        (e,) = units(b)
        name = unit_types(b)[e[F["unit_type"]]]
        assert name in ("KnightHero", "Skeleton"), name
        again = put(cards, decks, unit_spawns=[(name, 0, 9000 * SUB, 8000 * SUB, -1, -1, 0)])
        (f,) = units(again)
        assert (f[F["unit_type"]], f[F["card_id"]]) == (e[F["unit_type"]], e[F["card_id"]]), name


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
    ({"unit_deploy_ticks": [5]}, "unit_deploy_ticks has 1 entries for 0 unit spawns"),
    ({"unit_spawns": [("Knight", 0, 9 * TILE, 8 * TILE, -1, -1, 0)], "unit_deploy_ticks": [-1]}, "negative deploy"),
    ({"projectiles_in_flight": [(0, 0, 0, 0, 0, 0, None)]}, "no unit_spawns entry 0"),
    ({"projectiles_in_flight": [(-1, 2, 0, 0, 0, 0, None)]}, "bad team"),
])
def test_an_argument_of_the_wrong_shape_is_refused(kw, why):
    with pytest.raises(ValueError, match=why):
        battle(**kw)


def test_a_units_deploy_ticks_left_read_back_and_run_down():
    b = battle(unit_spawns=[("Knight", 0, 9000 * SUB, 8000 * SUB, -1, -1, 0)], unit_deploy_ticks=[12])
    left = []
    for _ in range(13):
        (e,) = units(b)
        left.append(e[F["deploy_ticks"]])
        b.step([], 1)
    assert left == list(range(12, -1, -1)), "12 ticks left, one a tick, then deployed"
    (e,) = units(battle(unit_spawns=[("Knight", 0, 9000 * SUB, 8000 * SUB, -1, -1, 0)], unit_deploy_ticks=[0]))
    assert e[F["deploy_ticks"]] == 0, "0: deployed, as without the list"


P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
#: A blue Musketeer and a red Knight 5 tiles before her, and her shot at him (unit_spawns entry 1) 3 tiles on its way.
DUEL = [("Musketeer", 0, 9 * TILE, 9 * TILE, -1, -1, 0), ("Knight", 1, 9 * TILE, 14 * TILE, -1, -1, 0)]
HER_SHOT = (0, 0, 9 * TILE, 11 * TILE, 9 * TILE, 14 * TILE, 1)


def red_units(b) -> list:
    return [e for e in units(b) if e[F["team"]] == 1]


def towers(b, team) -> dict:
    return {e[F["tower_slot"]]: e[F["hp"]] for e in json.loads(b.state_json())["entities"]
            if e[F["tower_slot"]] >= 0 and e[F["team"]] == team}


def test_a_shot_goes_down_from_the_copys_firer_at_the_copys_target():
    b = battle(unit_spawns=DUEL, projectiles_in_flight=[HER_SHOT])
    (p,) = json.loads(b.state_json())["projectiles"]
    (knight,) = red_units(b)
    assert (p[P["team"]], p[P["x"]], p[P["y"]], p[P["aim_x"]], p[P["aim_y"]]) == (0, *HER_SHOT[2:6])
    assert (p[P["target_uid"]], p[P["firer_card_id"]]) == (knight[F["uid"]], MUSKETEER), "her shot at the Knight"

    def knight_hp(shots) -> list:
        b = battle(unit_spawns=DUEL, projectiles_in_flight=shots)
        hp = []
        for _ in range(12):
            b.step([], 1)
            hp.append(red_units(b)[0][F["hp"]])
        return hp

    shot, none = knight_hp([HER_SHOT]), knight_hp(None)
    lands = next((t for t, (a, c) in enumerate(zip(shot, none, strict=True)) if a != c), None)
    assert lands is not None, f"the shot changes nothing: {shot}"
    assert shot[lands] < none[lands], f"it lands on him, where the copy without it does not: {shot} {none}"


def test_spawns_go_first_and_the_lists_name_the_unit_spawns_entries():
    # a catalogue spawn ahead of them: `unit_deploy_ticks` and the shot's refs are the unit_spawns entries'
    b = battle(spawns=[(1, KNIGHT, 3 * TILE, 20 * TILE, -1)], unit_spawns=DUEL, unit_deploy_ticks=[0, 7],
               projectiles_in_flight=[HER_SHOT])
    spawned, copied = sorted(red_units(b), key=lambda e: e[F["x"]])
    assert (spawned[F["deploy_ticks"]], copied[F["deploy_ticks"]]) == (0, 7), "the 7 is the unit_spawns Knight's"
    (p,) = json.loads(b.state_json())["projectiles"]
    assert p[P["target_uid"]] == copied[F["uid"]], "target 1 is the unit_spawns Knight, not the spawn"


def test_a_shot_at_a_crown_tower_lands_on_that_tower():
    # a red Musketeer's shot at Blue's crown tower 1 (-2): that tower loses hitpoints, the king and tower 2 do not
    b = battle(unit_spawns=[("Musketeer", 1, 3500 * SUB, 12500 * SUB, -1, -1, 0)])
    full = towers(b, 0)
    b = battle(unit_spawns=[("Musketeer", 1, 3500 * SUB, 12500 * SUB, -1, -1, 0)],
               projectiles_in_flight=[(0, 1, 3500 * SUB, 9000 * SUB, 3500 * SUB, 7000 * SUB, -2)])
    for _ in range(8):
        b.step([], 1)
    now = towers(b, 0)
    assert (now[0], now[2]) == (full[0], full[2]), f"the king and tower 2 untouched: {full} -> {now}"
    assert now[1] < full[1], f"tower 1 hit: {full} -> {now}"
    # a crown tower's shot by its tower index, whose target is gone: it is the tower's, and flies at no one
    b = battle(projectiles_in_flight=[(-2, 0, 3500 * SUB, 8000 * SUB, 3500 * SUB, 11000 * SUB, None)])
    (p,) = json.loads(b.state_json())["projectiles"]
    assert (p[P["target_uid"]], p[P["firer_card_id"]]) == (-1, -1), "no target; a crown tower's"


def test_a_copys_builder_can_ask_what_shots_fly_first():
    b = battle()
    names = ("Musketeer", "PrincessTower", "KingTower", "Knight", "BabyDragon")
    assert [b.projectile_in_flight_kind(u) for u in names] == ["flies", "flies", "flies", "no_projectile", "flies"]
    assert b.projectile_in_flight_kind("Musketeer", 1) == "form_shot", "the Evo Musketeer's snipe"
    with pytest.raises(ValueError, match=r"fires no single shot \(no_projectile\)"):
        battle(unit_spawns=DUEL, projectiles_in_flight=[(1, 1, 0, 0, 0, 0, None)])
    with pytest.raises(ValueError, match="is not of team 1"):
        battle(unit_spawns=DUEL, projectiles_in_flight=[(0, 1, 0, 0, 0, 0, None)])
    friend = [*DUEL, ("Knight", 0, 9 * TILE, 8 * TILE, -1, -1, 0)]
    with pytest.raises(ValueError, match="of its own side"):
        battle(unit_spawns=friend, projectiles_in_flight=[(0, 0, 9 * TILE, 9 * TILE, 9 * TILE, 8 * TILE, 2)])
