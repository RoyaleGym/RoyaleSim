"""`Battle(death_spawn_pushback=...)`: the Python selector of spawner.DEATH_SPAWN_PUSHBACK.

The key's measured arm, client_ring_slide, lays a death spawn's slide in the arena's frame for both seats, so a Red
death is not the rotation of a Blue one. A rotation gate must be able to turn it off from Python, which is what this
argument is for (crates/royalesim/tests/mirror.rs `every_asymmetric_calib_key_is_selectable_from_python` asks for it
the day the ledger ships the measured arm).

WHAT IS PINNED:
  1. both arms construct a battle, by keyword;
  2. a name with no engine implementation is refused, naming the argument;
  3. the argument is the LAST one: the five leading arguments still go by position, as callers pass them.
"""

import pytest

royalesim = pytest.importorskip("royalesim")

SLOTS = [[0, 1, 2], [0, 1, 2]]
CARDS = ["Knight", "Giant"]


@pytest.mark.parametrize("arm", ["not_read", "client_ring_slide"])
def test_both_arms_construct_a_battle(arm):
    royalesim.Battle(CARDS, SLOTS, death_spawn_pushback=arm)


def test_an_unknown_arm_is_refused_by_name():
    with pytest.raises(ValueError, match="death_spawn_pushback"):
        royalesim.Battle(CARDS, SLOTS, death_spawn_pushback="no_such_arm")


def test_the_five_leading_arguments_still_go_by_position():
    royalesim.Battle(CARDS, SLOTS, None, None, None)
