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


def test_under_a_mirrored_shuffle_each_card_keeps_its_own_level(royalesim):
    """A MIRRORED shuffle permutes both decks before the battle starts; each card's level must go with it, so a card
    plays at the level set for IT, wherever the shuffle put it (players[t]["deck"] reports the dealt order)."""
    sub = royalesim.SUBTILE_PER_MILLITILE
    b = battle(royalesim)
    ids = list(range(len(DECK)))
    level_of = {cid: lv for cid, lv in zip(ids, [9, 10, 11, 12, 13, 10, 11, 12], strict=True)}
    levels = [[level_of[c] for c in ids], [level_of[c] for c in ids]]
    b.reset(5, [ids, ids], 2, 200, [10_000, 10_000], None, [], None, levels, None)
    import royalesim as rs

    card, team_f, slot_f, lvl_f = (field(rs, n) for n in ("card_id", "team", "tower_slot", "level"))
    player = json.loads(b.state_json())["players"][0]
    assert sorted(player["deck"]) == ids, player["deck"]
    assert player["deck"] != ids, "scene: this seed's shuffle moves the deck (else the test checks nothing)"
    played = []
    for _ in range(4):
        hand = json.loads(b.state_json())["players"][0]["hand"]
        slot = next((k for k, c in enumerate(hand) if DECK[c] != "Fireball" and c not in played), None)
        if slot is None:
            break
        played.append(hand[slot])
        b.step([(0, slot, 9000 * sub, (6000 + 1500 * len(played)) * sub)], 1)
        b.step([], 40)
    v = json.loads(b.state_json())
    seen = {e[card]: e[lvl_f] for e in v["entities"] if e[team_f] == 0 and e[slot_f] < 0}
    assert played, "scene: no troop card was played"
    assert all(seen.get(c) == level_of[c] for c in played), (played, seen, level_of)
