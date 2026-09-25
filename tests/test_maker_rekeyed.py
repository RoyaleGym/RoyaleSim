"""tools/make_replay_fixture.py `merge_rekeyed`: a truth unit the capture re-keyed mid-life is ONE entity.

In 010218 a Tombstone Skeleton's key 21 ends on tick 571 and key 22 starts on 572, 89 native away, same hp, same state.
As two entities the second was a phantom spawn the harness paired with the engine's next emission, and every later
Tombstone pair landed a wave off. These cases pin the rule on synthetic rows: what merges, and the three look-alikes
that must not (a fresh emission beside a death, a jump too far for the gap, two candidates).
"""

from __future__ import annotations

import importlib.util
import os

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


@pytest.fixture(scope="module")
def m():
    path = os.path.join(ROOT, "tools", "make_replay_fixture.py")
    spec = importlib.util.spec_from_file_location("make_replay_fixture", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def ent(key, first, last, side=0, card_id=7, kind=1, max_hp=81):
    return {"key": key, "side": side, "card_id": card_id, "kind_first": kind, "max_hp": max_hp, "first_index": first,
            "last_index": last, "frames": last - first + 1, "states": [], "positions": []}


def row(x, y, hp=81, target=-1, state=1):
    """(x, y, hp, target, path nodes, behaviour state, cells, attack timer): TRUTH_COLUMNS' order."""
    return (x, y, hp, target, 0, state, [], 0)


def scene(b_first_tick_gap=1, b_xy=(1089, 1000), b_state=1, twin=False):
    """Key 21 on frames 0-1, key 22 from the frame b_first_tick_gap ticks after, and key 30, an enemy, targeting 22."""
    ticks = [570, 571, 571 + b_first_tick_gap, 572 + b_first_tick_gap]
    ents = {21: ent(21, 0, 1), 22: ent(22, 2, 3), 30: ent(30, 0, 3, side=1)}
    rows = [
        {21: row(1000, 900), 30: row(5000, 5000, 500, 21)},
        {21: row(1000, 1000), 30: row(5000, 5000, 500, 21)},
        {22: row(*b_xy, state=b_state), 30: row(5000, 5000, 500, 22)},
        {22: row(b_xy[0], b_xy[1] + 89, state=b_state), 30: row(5000, 5000, 500, 22)},
    ]
    if twin:
        ents[23] = ent(23, 0, 1)
        rows[0][23] = row(1050, 900)
        rows[1][23] = row(1050, 1000)
    return ents, rows, ticks


def test_a_rekeyed_unit_is_one_entity(m):
    ents, rows, ticks = scene()
    merges = m.merge_rekeyed(ents, rows, ticks)
    assert merges == [[21, 22, 572]]
    assert 22 not in ents
    assert ents[21]["last_index"] == 3
    assert all(22 not in r for r in rows), "the merged key's rows still exist"
    assert rows[2][21][:2] == (1089, 1000)
    assert rows[3][30][3] == 21, "a target that named the merged key must name the kept one"


def test_a_dropped_frame_between_the_keys_still_merges(m):
    """010218-A.b1: the same re-key across a 2-tick gap, 178 native: two steps."""
    ents, rows, ticks = scene(b_first_tick_gap=2, b_xy=(1000, 1178))
    assert m.merge_rekeyed(ents, rows, ticks) == [[21, 22, 573]]


def test_a_fresh_emission_beside_a_death_is_not_a_rekey(m):
    """Same side, card, hp and place, but a new unit starts in another behaviour state."""
    ents, rows, ticks = scene(b_state=4)
    assert m.merge_rekeyed(ents, rows, ticks) == []
    assert 22 in ents


def test_a_jump_too_far_for_the_gap_is_not_a_rekey(m):
    ents, rows, ticks = scene(b_first_tick_gap=1, b_xy=(1000, 1178))
    assert m.merge_rekeyed(ents, rows, ticks) == []


def test_two_candidates_merge_nothing(m):
    ents, rows, ticks = scene(twin=True)
    assert m.merge_rekeyed(ents, rows, ticks) == []
