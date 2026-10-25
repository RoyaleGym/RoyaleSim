"""THE ELIXIR DATA OF THE TABLE A BATTLE RUNS (py.rs `Battle.unit_mana_json`), for a caller's own elixir count.

Pinned: the Elixir Collector's payout and period, its death payout, and the Elixir Golem pieces' payouts to the
opponent, in thousandths of an elixir as the engine pays them, on the default table and on an override table whose
Collector period differs (13000 ms on the 15.535.29 table, 15000 ms on the 160402017 tables).
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SLOTS = [[0, 1, 2], [0, 1, 2]]


def mana(table: str | None) -> dict:
    over = None if table is None else {"cards.CARD_TABLE": json.dumps(table)}
    return json.loads(royalesim.Battle(None, SLOTS, calibration_overrides=over).unit_mana_json())


def test_the_default_tables_elixir_rows():
    m = mana(None)
    assert m["ElixirCollector"] == [1000, 13000, 1000, 0], m
    assert m["ElixirGolem1"] == [0, 0, 0, 1000], m
    assert m["ElixirGolem2"] == [0, 0, 0, 500], m
    assert m["ElixirGolem4"] == [0, 0, 0, 500], m
    assert all(any(v) for v in m.values()), "only rows that produce or pay"


def test_an_override_tables_own_numbers():
    m = mana("client160402017_20261019")
    assert m["ElixirCollector"] == [1000, 15000, 1000, 0], m
    assert m["ElixirGolem1"][3] == 1000, m


def test_the_opponent_payout_follows_its_unit_key():
    over = {"economy.MANA_ON_DEATH_FOR_OPPONENT_UNIT": json.dumps("elixir")}
    m = json.loads(royalesim.Battle(None, SLOTS, calibration_overrides=over).unit_mana_json())
    assert m["ElixirGolem1"][3] == 1000 * 1000, "a whole elixir per unit under the elixir arm"
