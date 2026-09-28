"""`Battle(tap_snap=...)`: the Python selector of placement.TAP_SNAP.

The key's measured arm, client16402_tile_centre, takes a troop or spell tap at its tile's centre, so a scene's tap on a
tile boundary snaps up for one seat and down for the other and is not the rotation of its twin. A rotation gate must be
able to turn it off from Python, which is what this argument is for (crates/royalesim/tests/mirror.rs
`every_asymmetric_calib_key_is_selectable_from_python` asks for it the day the ledger ships the measured arm).

WHAT IS PINNED:
  1. both arms construct a battle, by keyword;
  2. a name with no engine implementation is refused, naming the argument;
  3. the five leading arguments still go by position, as callers pass them.
"""

import pytest

royalesim = pytest.importorskip("royalesim")

SLOTS = [[0, 1, 2], [0, 1, 2]]
CARDS = ["Knight", "Giant"]


@pytest.mark.parametrize("arm", ["none", "client16402_tile_centre"])
def test_both_arms_construct_a_battle(arm):
    royalesim.Battle(CARDS, SLOTS, tap_snap=arm)


def test_an_unknown_arm_is_refused_by_name():
    with pytest.raises(ValueError, match="tap_snap"):
        royalesim.Battle(CARDS, SLOTS, tap_snap="no_such_arm")


def test_the_five_leading_arguments_still_go_by_position():
    royalesim.Battle(CARDS, SLOTS, None, None, None)
