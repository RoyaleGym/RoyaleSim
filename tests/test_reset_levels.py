"""Battle.reset's per-side levels: `levels` (one unified level per deck card, per side) and `tower_levels` (per side).

A real match has unequal levels on the two sides and within a deck (a climb capture's own 7-8 against the opponent's
8-10), so the bindings take them per card. Pinned here: each side's played card and towers read their own level in the
entity rows' `level` column; None keeps today's single level; a list of the wrong length is refused.
"""

from __future__ import annotations

import json

import pytest

DECK = ["Knight", "Archer", "Giant", "Musketeer", "Fireball", "Valkyrie", "HogRider", "Minions"]


@pytest.fixture(scope="module")
def royalesim():
    return pytest.importorskip("royalesim")


def battle(royalesim):
    return royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])


def field(royalesim, name: str) -> int:
    return list(royalesim.ENTITY_FIELDS).index(name)


def levels_by_team(b, card_id: int) -> dict[int, set[int]]:
    import royalesim

    v = json.loads(b.state_json())
    team, card, slot, level = (field(royalesim, n) for n in ("team", "card_id", "tower_slot", "level"))
    out: dict[int, set[int]] = {}
    for e in v["entities"]:
        if e[card] == card_id and e[slot] < 0:
            out.setdefault(e[team], set()).add(e[level])
    return out


def tower_levels(b) -> dict[int, set[int]]:
    import royalesim

    v = json.loads(b.state_json())
    team, slot, level = (field(royalesim, n) for n in ("team", "tower_slot", "level"))
    out: dict[int, set[int]] = {}
    for e in v["entities"]:
        if e[slot] >= 0:
            out.setdefault(e[team], set()).add(e[level])
    return out


def test_each_side_plays_its_own_card_and_tower_levels(royalesim):
    sub = royalesim.SUBTILE_PER_MILLITILE
    b = battle(royalesim)
    ids = list(range(len(DECK)))
    levels = [[9] + [11] * 7, [13] + [11] * 7]
    b.reset(0, [ids, ids], 0, 200, [10_000, 10_000], None, [], None, levels, [10, 14])
    assert tower_levels(b) == {0: {10}, 1: {14}}
    b.step([], 1)
    b.step([(0, 0, 9500 * sub, 9500 * sub), (1, 0, 8500 * sub, 22500 * sub)], 1)
    for _ in range(10):
        b.step([], 1)
    assert levels_by_team(b, 0) == {0: {9}, 1: {13}}, "each Knight at its own deck entry's level"


def test_no_levels_keeps_the_single_level(royalesim):
    b = battle(royalesim)
    ids = list(range(len(DECK)))
    b.reset(0, [ids, ids], 0, 200, [10_000, 10_000], None, [])
    lv = b.card_level()
    assert tower_levels(b) == {0: {b.tower_level()}, 1: {b.tower_level()}}
    assert lv > 0


@pytest.mark.parametrize(
    ("levels", "tower", "says"),
    [([[9, 10], []], None, "levels[0]"), ([[]], None, "levels must be"), (None, [11], "tower_levels must be")],
)
def test_a_malformed_level_list_is_refused(royalesim, levels, tower, says):
    b = battle(royalesim)
    ids = list(range(len(DECK)))
    with pytest.raises(ValueError, match=says.replace("[", r"\[")):
        b.reset(0, [ids, ids], 0, 200, [10_000, 10_000], None, [], None, levels, tower)
