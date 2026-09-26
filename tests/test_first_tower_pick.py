"""A summon member's first tower comes from its spawn lane, not its x (targeting.FIRST_TOWER_PICK).

WHAT THIS PINS. On client 15.535.29 (the lane-pick scenarios, both sides) Minions played on the tile centre x 9500 lay
members at x 9500, 9999 and 9001. The 9001 member flies to the LEFT princess tower although it stands right of the
centre line. Played at x 8500, the members at 8500 and 8001 go left and the 8999 member goes RIGHT. Every other member,
and a single troop, goes to the side of its x. The 16.402 corpus agrees: the 9001 Minion of a red king-area deploy
goes left (3 deploys in 2 battles), and Goblins shoved across the line during their deploy keep their spawn side (12 of
12 records). Today's engine sends each member to the side of its x.

The rule (client_spawn_lane): at creation a unit gets a lane, the lane of the nearest lane cell of the arena tilemap to
its half-tile cell. A summon member also reads its deploy point's cell; when the member and its deploy point sit on
the two sides of the centre column in the way the ledger entry states, and both cells give the same lane, the member
takes the other lane. Its first tower is its lane's tower; later picks follow its x, as today.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `first_pick_by_x` -- the first pick goes by the unit's current x, as today:
    test_the_straddling_member_takes_its_spawn_lane.
  * `spawn_lane_no_flip` -- a summon member keeps the lane of its own point, as a single troop does:
    test_the_straddling_member_takes_its_spawn_lane.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.FIRST_TOWER_PICK"
NEW_ARM, OLD_ARM = "client_spawn_lane", "current_x"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
PLAY_TICK, TICKS = 2, 50
#: (team, the tapped tile centre) -> {a member's first x: the side it heads to on client 15.535.29, +1 right, -1 left}
SCENES = {
    (0, (9500, 13500)): {9500: +1, 9999: +1, 9001: -1},
    (0, (8500, 13500)): {8500: -1, 8001: -1, 8999: +1},
    (1, (9500, 18500)): {9500: +1, 9999: +1, 9001: -1},
    (1, (8500, 18500)): {8500: -1, 8001: -1, 8999: +1},
}
#: the member whose spawn lane differs from its x, per scene
STRADDLER = {9500: 9001, 8500: 8999}


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def headings(arm, team, at, card="Minions"):
    """{first x: sign of the x travelled over the run} for every unit of the played card."""
    b = royalesim.Battle([card, "Giant"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[0] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, [])
    first, last = {}, {}
    for t in range(1, TICKS + 1):
        b.step([(team, 0, at[0] * SUB, at[1] * SUB)] if t == PLAY_TICK else [], 1)
        for e in json.loads(b.state_json())["entities"]:
            if e[F["tower_slot"]] < 0 and e[F["team"]] == team:
                first.setdefault(e[F["uid"]], e[F["x"]])
                last[e[F["uid"]]] = e[F["x"]]
    out = {}
    for u, x0 in first.items():
        dx = last[u] - x0
        out[x0 // SUB] = (dx > 0) - (dx < 0)
    return out


@pytest.mark.parametrize(("team", "at"), sorted(SCENES))
def test_the_straddling_member_takes_its_spawn_lane(team, at):
    got = headings(NEW_ARM, team, at)
    x = STRADDLER[at[0]]
    want = SCENES[(team, at)][x]
    assert got.get(x) == want, (
        f"team {team} at {at}: the member created at x {x} heads "
        f"{'right' if got.get(x) == 1 else 'left'}, not to its spawn lane's tower ({got})"
    )


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
@pytest.mark.parametrize(("team", "at"), sorted(SCENES))
def test_the_other_members_follow_their_x(arm, team, at):
    got = headings(arm, team, at)
    want = {x: s for x, s in SCENES[(team, at)].items() if x != STRADDLER[at[0]]}
    assert {x: got.get(x) for x in want} == want, f"{arm}, team {team} at {at}: {got}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
@pytest.mark.parametrize("x", [8500, 9500])
def test_a_single_troop_follows_its_x(arm, x):
    got = headings(arm, 0, (x, 13500), card="Knight")
    assert list(got.values()) == [1 if x > 9000 else -1], f"{arm}: a Knight tapped at x {x} heads {got}"


def test_old_arm_is_todays_engine():
    for team, at in sorted(SCENES):
        got = headings(OLD_ARM, team, at)
        x = STRADDLER[at[0]]
        assert got.get(x) == (1 if x > 9000 else -1), f"old arm, team {team} at {at}: {got}"
