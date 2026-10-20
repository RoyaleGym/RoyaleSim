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
