"""The entity row's `level` column (py.rs ENTITY_FIELDS, state_json): the unified level each entity plays at.

WHAT IS PINNED:
  1. `level` is ENTITY_FIELDS' last name, right after `status_flags`;
  2. in a battle whose cards run at 12 and whose towers run at 13, a spawned Knight's row reads 12 and every crown
     tower's reads 13 (a column that read the card level, or a constant, would read one of them wrong);
  3. the prefix rule the Gym's decoder holds (royalegym.protocol EntityState): the engine's ENTITY_FIELDS is a prefix
     of EntityState's fields, `level` in the same place. Skipped, saying so, where royalegym is not installed.

The rules the column reports (a Mirror's copy, a Clone's copy, a spawned or death-spawned unit) are pinned on the
engine in crates/royalesim/tests/entity_level.rs.
"""

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
SLOTS = [[0, 1, 2], [0, 1, 2]]
CARDS = ["Knight", "Giant"]


def test_level_is_the_last_entity_field():
    fields = list(royalesim.ENTITY_FIELDS)
    assert fields[-2:] == ["status_flags", "level"]


def test_a_units_row_reads_its_card_level_and_a_towers_its_tower_level():
    b = royalesim.Battle(CARDS, SLOTS, level=12, tower_level=13)
    b.reset(0, [[0] * 8, [1] * 8], 0, 0, None, None, [(0, 0, 9000 * SUB, 8000 * SUB, -1)])
    state = json.loads(b.state_json())
    fields = list(royalesim.ENTITY_FIELDS)
    card, slot, level = fields.index("card_id"), fields.index("tower_slot"), fields.index("level")
    rows = state["entities"]
    towers = [r[level] for r in rows if r[slot] >= 0]
    knights = [r[level] for r in rows if r[card] == 0 and r[slot] < 0]
    assert towers == [13] * 6, f"every crown tower at its tower level: {towers}"
    assert knights == [12], f"the Knight at its card level: {knights}"


def test_the_engine_fields_are_a_prefix_of_the_gyms_entity_state():
    why = "royalegym is not installed: the Gym's side of the prefix rule is not checked here"
    protocol = pytest.importorskip("royalegym.protocol", reason=why)
    theirs = list(protocol.EntityState.__struct_fields__)
    ours = list(royalesim.ENTITY_FIELDS)
    assert theirs[: len(ours)] == ours, f"ENTITY_FIELDS {ours} is not a prefix of EntityState {theirs}"
