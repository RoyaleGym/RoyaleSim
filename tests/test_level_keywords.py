"""`Battle(level=..., tower_level=...)`: the card and tower level of every battle one object starts.

WHAT IS PINNED:
  1. with neither keyword, the towers run at the card level;
  2. `level` sets both sides' cards and, with no `tower_level`, the towers: a spawned Knight and the king tower have
     more hitpoints at 16 than at the default;
  3. `tower_level` sets the towers alone: the Knight keeps level 16's hitpoints, the king tower takes 12's;
  4. a level a catalogue card's or a tower's ladder lacks is refused at construction, naming the keyword.
"""

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
SLOTS = [[0, 1, 2], [0, 1, 2]]
CARDS = ["Knight", "Giant"]


def _hp(b):
    """(the spawned Blue Knight's max hp, Blue's king tower's max hp) in a fresh battle from `b`."""
    b.reset(0, [[0] * 8, [1] * 8], 0, 0, None, None, [(0, 0, 9000 * SUB, 8000 * SUB, -1)])
    state = json.loads(b.state_json())
    fields = list(royalesim.ENTITY_FIELDS)
    card, team, max_hp = fields.index("card_id"), fields.index("team"), fields.index("max_hp")
    knights = [e[max_hp] for e in state["entities"] if e[card] == 0 and e[team] == 0]
    assert len(knights) == 1, f"one Blue Knight expected, got {knights}"
    return knights[0], state["players"][0]["tower_max_hp"][0]


def test_the_default_towers_run_at_the_card_level():
    b = royalesim.Battle(CARDS, SLOTS)
    assert b.tower_level() == b.card_level()


def test_level_sets_cards_and_towers():
    base = royalesim.Battle(CARDS, SLOTS)
    high = royalesim.Battle(CARDS, SLOTS, level=16)
    assert (high.card_level(), high.tower_level()) == (16, 16)
    assert base.card_level() < 16
    (k0, t0), (k1, t1) = _hp(base), _hp(high)
    assert k1 > k0, f"the Knight at 16 ({k1}) has no more hitpoints than at {base.card_level()} ({k0})"
    assert t1 > t0, f"the king tower at 16 ({t1}) has no more hitpoints than at {base.tower_level()} ({t0})"


def test_tower_level_sets_the_towers_alone():
    high = royalesim.Battle(CARDS, SLOTS, level=16)
    split = royalesim.Battle(CARDS, SLOTS, level=16, tower_level=12)
    assert (split.card_level(), split.tower_level()) == (16, 12)
    (k1, t1), (k2, t2) = _hp(high), _hp(split)
    assert k2 == k1, "tower_level moved the Knight's hitpoints"
    assert t2 < t1, f"the king tower at 12 ({t2}) is not below 16's ({t1})"


def test_a_level_a_ladder_lacks_is_refused_by_name():
    with pytest.raises(ValueError, match="level 99"):
        royalesim.Battle(CARDS, SLOTS, level=99)
    with pytest.raises(ValueError, match="tower_level 99"):
        royalesim.Battle(CARDS, SLOTS, tower_level=99)
