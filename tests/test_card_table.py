"""THE CARD TABLE A BATTLE RUNS, from Python (calibration cards.CARD_TABLE; `Battle.card_table`).

Option B moves the engine's card tables to the current client; until the ledger flips, a caller picks the 160402017
table by overriding the key. Pinned: the default is the 15.535.29 table; the override builds every battle of the
object on the 160402017 table, whose values are its own (the Royal Ghost's hitpoints at level 11); and a name the key
does not list is refused.
"""

from __future__ import annotations

import json

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
    with pytest.raises(ValueError):
        battle("client9999")
