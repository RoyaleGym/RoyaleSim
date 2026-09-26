"""Battle.debug_contact(): each troop's contact state, debug-only, row for row with debug_units and state_json.

A blue Knight walks from the bridge; each tick, debug_contact has one row per troop, keyed by the same uid as
debug_units' element 0, and its facing is the one state_json carries. The Knight walks, so its facing and its
segment direction are length-256 vectors once it has a route, and its avoidance offset is a multiple of 10 in
[-190, 190].
"""
from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}


def test_debug_contact_rows_match_the_units_and_the_state():
    b = royalesim.Battle(["Knight"], [[0, 1, 2], [0, 1, 2]])
    b.reset(0, [[0] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, [(0, 0, 3500 * SUB, 14000 * SUB, -1)])
    walked = 0
    for _ in range(30):
        b.step([], 1)
        rows = b.debug_contact()
        assert sorted(r[0] for r in rows) == sorted(u[0] for u in b.debug_units())
        facing = {e[F["uid"]]: tuple(e[F["facing"]]) for e in json.loads(b.state_json())["entities"]}
        for uid, offset, sx, sy, fx, fy in rows:
            assert (fx, fy) == facing[uid], f"uid {uid}: debug_contact's facing is not state_json's"
            assert offset % 10 == 0, f"uid {uid}: offset {offset} is not a multiple of 10"
            assert -190 <= offset <= 190, f"uid {uid}: offset {offset} is outside [-190, 190]"
            assert (sx, sy) == (0, 0) or 250 <= (sx * sx + sy * sy) ** 0.5 <= 257, f"uid {uid}: segment ({sx}, {sy})"
            walked += (sx, sy) != (0, 0)
    assert walked > 0, "vacuous: the Knight never had a segment direction"
