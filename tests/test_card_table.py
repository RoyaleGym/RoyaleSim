"""THE CARD TABLE A BATTLE RUNS, from Python (calibration cards.CARD_TABLE; `Battle.card_table`).

Option B moves the engine's card tables to the current client; until the ledger flips, a caller picks the 160402017
table by overriding the key. Pinned: the default is the 15.535.29 table; the override builds every battle of the
object on the 160402017 table, whose values are its own (the Royal Ghost's hitpoints at level 11); a name the key
does not list is refused; and `Battle.card_table_fnv1a64` is the hash of the very table text the battle read.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

royalesim = pytest.importorskip("royalesim")

KEY = "cards.CARD_TABLE"
SLOTS = [[0, 1, 2], [0, 1, 2]]


def battle(table: str | None) -> object:
    over = None if table is None else {KEY: json.dumps(table)}
    return royalesim.Battle(["Ghost", "Knight"], SLOTS, calibration_overrides=over)


def ghost_hp(b) -> int:
    rows = b.unit_hitpoints(0, 11)
    assert rows[0][0] == "own", rows
    return rows[0][2]


def test_the_default_table_is_15535():
    assert battle(None).card_table() == "cards-15535.1"
    assert battle("client15535").card_table() == "cards-15535.1"


def test_reset_equips_chef_for_just_one_side_and_preserves_equipment_on_restore():
    over = {KEY: json.dumps("client160635007_20261103")}
    b = royalesim.Battle(["Ghost", "Knight", "Fireball"], SLOTS, calibration_overrides=over)
    decks = [[0, 1] * 4, [2, 1, 0, 1, 0, 1, 0, 1]]
    b.reset(3, decks, 0, 0, [5000, 5000], None, [], tower_troops=["ChefTower", None],
            tower_levels=[11, 11], king_levels=[13, None])
    state = json.loads(b.state_json())
    types = json.loads(b.unit_types_json())
    fields = royalesim.ENTITY_FIELDS
    towers = [dict(zip(fields, row, strict=True)) for row in state["entities"]]
    flanks = [t for t in towers if t["tower_slot"] in (1, 2)]
    assert [(t["team"], types[t["unit_type"]]) for t in flanks if t["team"] == 0] == [(0, "ChefTower")] * 2
    assert [(t["team"], types[t["unit_type"]]) for t in flanks if t["team"] == 1] == [(1, "PrincessTower")] * 2
    blue, red = state["players"]
    assert blue["tower_max_hp"][0] > red["tower_max_hp"][0]
    assert blue["tower_max_hp"][1] < red["tower_max_hp"][1]
    blob, fingerprint = b.save(), b.state_hash()
    b.reset(5, decks, 0, 0, None, None, [])
    b.load(blob)
    assert b.state_hash() == fingerprint
    with pytest.raises(ValueError, match="Knight"):
        b.reset(3, decks, 0, 0, None, None, [], tower_troops=["Knight", None])
    hp = [list(blue["tower_hp"]), list(red["tower_hp"])]
    hp[0][:2] = [1, 0]
    b.reset(3, decks, 0, 100, [5000, 5000], hp, [], tower_troops=["ChefTower", None],
            tower_levels=[11, 11], king_levels=[13, None])
    sub = royalesim.SUBTILE_PER_MILLITILE
    assert b.step([(1, 0, 9000 * sub, 3000 * sub)], 100)[0][1] == 0
    fallen = json.loads(b.state_json())["players"][0]
    assert fallen["tower_hp"][:2] == [0, 0]
    assert fallen["tower_max_hp"] == blue["tower_max_hp"]
    blob = b.save()
    b.reset(5, decks, 0, 0, None, None, [])
    b.load(blob)
    restored = json.loads(b.state_json())["players"][0]
    assert restored["tower_hp"][:2] == [0, 0]
    assert restored["tower_max_hp"] == blue["tower_max_hp"]


@pytest.mark.parametrize(("field", "value", "message"), [
    ("tower_troops", ["Missing", None], "tower troop Missing"),
    ("tower_troops", ["Knight", None], "tower troop Knight"),
    ("king_level", [0, None], "invalid tower level 0"),
    ("king_level", [999, None], "has no level 999"),
])
def test_snapshot_refuses_invalid_tower_equipment_and_king_level(field, value, message):
    b = battle("client160635007_20261103")
    decks = [[0, 1] * 4] * 2
    b.reset(3, decks, 0, 100, None, None, [], tower_troops=["ChefTower", None], king_levels=[13, None])
    original = b.save()
    corrupt = json.loads(original)
    corrupt[field] = value
    with pytest.raises(ValueError, match=message):
        b.load(json.dumps(corrupt).encode())
    assert b.save() == original


def test_mirror_evolution_in_extra_state_and_at_the_play():
    b = royalesim.Battle(None, SLOTS, calibration_overrides={KEY: json.dumps("client160635007_20261103")})
    rows = json.loads(b.catalogue_json())
    ids = {r[0]: i for i, r in enumerate(rows)}
    names = ["Knight", "Archer", "Giant", "Musketeer", "Mirror", "Valkyrie", "HogRider", "Minions"]
    deck = [ids[n] for n in names]
    b.reset(9, [deck, deck], 0, 100, [10000, 10000], None, [], forms=[[0, 0, 0, 0, 1, 0, 0, 0], []],
            evo_counts=[[0, 0, 0, 0, 2, 0, 0, 0], []])

    def play(slot):
        sub = royalesim.SUBTILE_PER_MILLITILE
        return b.step([(0, slot, 9000 * sub, 10000 * sub)], 1)[0]

    assert play(0)[1] == 0
    blue = json.loads(b.state_json(extra=True))["players"][0]
    assert blue["hand"][0] == ids["Mirror"]
    assert blue["hand_plays"][0] == ["Knight_EV1", 1]
    assert blue["queue"][0] == blue["next_card"]
    assert play(0)[1] == 0
    assert json.loads(b.state_json())["players"][0]["evo"] == [[ids["Mirror"], 0, 0, 2]]


def test_chef_levels_its_own_troop_and_the_button_points_to_its_hero():
    b = royalesim.Battle(None, SLOTS, level=11, tower_level=11,
                         calibration_overrides={KEY: json.dumps("client160635007_20261103")})
    ids = {row[0]: i for i, row in enumerate(json.loads(b.catalogue_json()))}
    deck = [ids[n] for n in ("Golem", "MiniPekka", "Knight", "Fireball", "Archer", "Musketeer", "Arrows", "Skeletons")]
    b.reset(2, [deck, deck], 0, 0, [10000, 10000], None, [], tower_troops=["ChefTower", None],
            forms=[[0, 2, 0, 0, 0, 0, 0, 0], []])
    b.step([], 660)
    sub = royalesim.SUBTILE_PER_MILLITILE
    assert [row[1] for row in b.step([(0, 0, 3500*sub, 3500*sub), (1, 0, 14500*sub, 28500*sub)], 65)] == [0, 0]
    fields = royalesim.ENTITY_FIELDS + royalesim.ENTITY_EXTRA_FIELDS
    def units(state):
        return [dict(zip(fields, row, strict=True)) for row in state["entities"]]

    state = json.loads(b.state_json(extra=True))
    assert {u["team"]: u["level"] for u in units(state) if u["card_id"] == ids["Golem"]} == {0: 12, 1: 11}
    b.step([], 110)
    assert b.step([(0, 1, 3500*sub, 4500*sub)], 30)[0][1] == 0
    state = json.loads(b.state_json(extra=True))
    hero, = [u for u in units(state) if u["card_id"] == ids["MiniPekka"] and u["team"] == 0]
    owner = state["players"][0]
    assert owner["ability_uids"] == [hero["uid"]]
    ready, _, cost, _, _ = owner["abilities"][0]
    assert ready
    assert b.step([(0, royalesim.HAND_SIZE, 0, 0)], 0)[0][1] == 0
    assert json.loads(b.state_json())["players"][0]["elixir_milli"] == owner["elixir_milli"] - cost * 1000


def test_the_override_runs_the_160402017_table():
    old, new = battle("client15535"), battle("client160402017_20261006")
    assert new.card_table() == "cards-160402017-20261006.1"
    assert ghost_hp(new) != ghost_hp(old), "the 160402017 table's own Royal Ghost (450 base, 473 on 15.535.29)"
    new.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, [])
    for _ in range(20):
        new.step([], 1)


def test_a_table_the_key_does_not_list_is_refused():
    with pytest.raises(ValueError, match=r"client9999|CARD_TABLE"):
        battle("client9999")


def test_the_160402017_table_keeps_every_card_id():
    """CARD IDS ARE FIXED, APPEND ONLY (option B): the default catalogue's ids (`card_names` None, the positions every
    trained policy and converted dataset reads) name the same cards on the 160402017 table as on the 15.535.29 one. The
    newer table refuses two event cards (the Super Hog Rider, GlobalLightning), which CATALOGUE_ORDER lists last, so
    its catalogue is the older one's less those two at its end; a card either table adds may only come after them."""
    def names(table: str | None) -> list[str]:
        over = None if table is None else {KEY: json.dumps(table)}
        b = royalesim.Battle(None, SLOTS, calibration_overrides=over)
        return [row[0] for row in json.loads(b.catalogue_json())]

    old, new = names(None), names("client160402017_20261006")
    assert len(old) > 100, len(old)
    assert old[len(new) :] == ["SuperHogRider", "GlobalLightning"], old[len(new) :]
    assert new == old[: len(new)], [(i, a, b) for i, (a, b) in enumerate(zip(old, new, strict=False)) if a != b][:5]


def fnv1a64(data: bytes) -> str:
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) % 2**64
    return f"{h:016x}"


def read_text(file_name: str, embedded: str) -> str:
    """The text the engine reads for a table (card.rs `CardDb::table_text`): the build checkout's file where a checkout
    build has it, else the copy compiled in."""
    data_dir = getattr(royalesim, "BUILD_DATA_DIR", None)
    if data_dir is not None:
        path = Path(data_dir) / "derived" / file_name
        if path.exists():
            return path.read_text(encoding="utf-8")
    return getattr(royalesim, embedded)


@pytest.mark.parametrize(
    ("table", "file_name", "embedded"),
    [
        (None, "cards.json", "EMBEDDED_CARDS_JSON"),
        ("client160402017_20261006", "cards-160402017-20261006.json", "EMBEDDED_CARDS_160402017_JSON"),
        ("client160402017_20261019", "cards-160402017-20261019.json", "EMBEDDED_CARDS_160402017_20261019_JSON"),
        ("client160635007_20261103", "cards-160635007-20261103.json", "EMBEDDED_CARDS_160635007_20261103_JSON"),
    ],
)
def test_the_battle_hashes_the_table_it_read(table, file_name, embedded):
    """THE STAMP IS OF THE TABLE THE BATTLE LOADED (py.rs `Battle.card_table_fnv1a64`): an override's table, not the
    15.535.29 one every override battle was stamped with before (RoyaleGym's card-table stamp reads this name)."""
    got = battle(table).card_table_fnv1a64()
    assert got == fnv1a64(read_text(file_name, embedded).encode("utf-8")), (table, got)
    assert len(got) == 16, got
    int(got, 16)


def test_two_tables_hash_apart():
    assert battle(None).card_table_fnv1a64() != battle("client160402017_20261006").card_table_fnv1a64()


def test_the_override_runs_the_20261019_table():
    """The Season 88 mid-season content (16.402.22): its own table, the same catalogue as the 2026-10-06 one."""
    new = battle("client160402017_20261019")
    assert new.card_table() == "cards-160402017-20261019.1"

    def names(table: str) -> list[str]:
        b = royalesim.Battle(None, SLOTS, calibration_overrides={KEY: json.dumps(table)})
        return [row[0] for row in json.loads(b.catalogue_json())]

    assert names("client160402017_20261019") == names("client160402017_20261006"), "card ids are fixed, append only"


def test_the_override_runs_the_16635_table():
    """The 16.635 client (build 160635007, content 16.635.8): its own table, the same catalogue as the 2026-10-19 one,
    and a stamp of its own."""
    new = battle("client160635007_20261103")
    assert new.card_table() == "cards-160635007-20261103.1"
    assert new.card_table_fnv1a64() != battle("client160402017_20261019").card_table_fnv1a64()

    def names(table: str) -> list[str]:
        b = royalesim.Battle(None, SLOTS, calibration_overrides={KEY: json.dumps(table)})
        return [row[0] for row in json.loads(b.catalogue_json())]

    assert names("client160635007_20261103") == names("client160402017_20261019"), "card ids are fixed, append only"
