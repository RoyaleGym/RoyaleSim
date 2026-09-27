"""A troop does not let go of a target that its chase-drop limit is crossed by a knockback
(targeting.CHASE_DROP_KNOCKED_TARGET).

WHAT THIS PINS. Under targeting.CHASE_DROP_RANGE = client_sight_minus_1000 a troop lets go of a troop target on the
first tick whose start-of-tick max(|dx|, |dy|) exceeds SightRange + both radii - 1000. On the 16.402 corpus a target
that crosses that limit while it slides under a knockback is not let go: in 20260920-081051 a Bowler attacking a
Bomber pushed it from 5,499 to 5,767 (limit 5,750) with its boulder, kept it through the slide and after it, and
launched its next boulder one HitSpeed after the first (ticks 928 and 978), standing where it was. 3 of 3 such pushes
in the corpus kept the target, all three by the holder's own boulder. Under client_holds_knocked the sliding target is
held through the slide (neither the chase drop nor a rescan lets it go); a push by anything else is inferred and not
pinned here. The old arm lets the Bomber go on the crossing tick and walks the Bowler off.
The scene is that battle's geometry: a blue Bowler at (14735, 17126), a red Bomber at (13669, 22260), 5,244 apart
(the Bowler's reach is 4000 + 750 + 500 = 5,250), with combat.RANGE_PROJECTILE = straight_to_range, under which the
boulder pushes.

WHY THE CONTROL IS HERE. A runner that is not sliding is still let go on the edge under both values: a red Knight
chasing a blue Hog Rider up the right lane lets it go on the tick it crosses 5,500 + 500 + 600 - 1,000. An
implementation that switches the chase drop off, or holds every target, fails it.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `chase_drop_knocked_dropped` -- client_holds_knocked still lets a sliding target go:
    test_a_target_pushed_past_the_limit_is_kept.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.CHASE_DROP_KNOCKED_TARGET"
NEW_ARM, OLD_ARM = "client_holds_knocked", "drops_knocked"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: the chase drop's own arm and the boulder's straight flight, both read by this scene
BASE = {"targeting.CHASE_DROP_RANGE": "client_sight_minus_1000", "combat.RANGE_PROJECTILE": "straight_to_range"}
#: 20260920-081051: the Bowler (blue) and the Bomber (red) it pushed past the limit
BOWLER_AT, BOMBER_AT = (14735, 17126), (13669, 22260)
BOWLER_LIMIT = 5500 + 750 + 500 - 1000
BOWLER_HIT_SPEED_TICKS = 2500 // 50
#: the control: a red Knight on the right lane and a blue Hog Rider behind it, outrunning it up the lane
KNIGHT_AT, HOG_AT = (14500, 12500), (14500, 9500)
KNIGHT_LIMIT = 5500 + 500 + 600 - 1000
FIRED = 2  # attack_phase: attacking, the hit (a launch) on this tick


def overrides(arm) -> dict:
    """`arm` None runs the build's own value of KEY."""
    over = dict(BASE)
    if arm is not None:
        over[KEY] = arm
    return {k: json.dumps(v) for k, v in over.items()}


def run(arm, cards, holder_team, holder_at, other_at, ticks):
    """Per tick t (1..): (holder row, other row, start-of-tick max(|dx|, |dy|) native). The holder is `cards[0]` on
    team `holder_team`, the other `cards[1]` on the other team; both spawned standing, not deploying."""
    b = royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    spawns = [
        (holder_team, 0, holder_at[0] * SUB, holder_at[1] * SUB, -1),
        (1 - holder_team, 1, other_at[0] * SUB, other_at[1] * SUB, -1),
    ]
    decks = [[0] * 8, [1] * 8] if holder_team == 0 else [[1] * 8, [0] * 8]
    b.reset(0, decks, 0, 200, [10_000, 10_000], None, spawns)

    def units():
        es = [e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0]
        return {e[F["team"]]: e for e in es}

    prev, rows = units(), []
    for _ in range(ticks):
        b.step([], 1)
        now = units()
        if len(now) < 2 or len(prev) < 2:
            break
        ph, po = prev[holder_team], prev[1 - holder_team]
        m = max(abs(ph[F["x"]] - po[F["x"]]), abs(ph[F["y"]] - po[F["y"]])) / SUB
        rows.append((now[holder_team], now[1 - holder_team], m))
        prev = now
    return rows


def bowler(arm, ticks=70):
    return run(arm, ["Bowler", "Bomber"], 0, BOWLER_AT, BOMBER_AT, ticks)


def crossing(rows, limit):
    """The first row index whose start-of-tick distance is past `limit` after one within it."""
    return next((i for i in range(1, len(rows)) if rows[i - 1][2] <= limit < rows[i][2]), None)


def assert_kept_through_the_push(arm):
    rows = bowler(arm)
    bomber_uid = rows[0][1][F["uid"]]
    i = crossing(rows, BOWLER_LIMIT)
    assert i is not None, f"{arm}: the scene drifted: the Bomber never crossed the Bowler's chase limit {BOWLER_LIMIT}"
    assert rows[i][1][F["knockback_ticks"]] > 0, (
        f"{arm}: the scene drifted: the Bomber crossed the limit on {i + 1} without sliding"
    )
    launches = [t for t, (h, _, _) in enumerate(rows) if h[F["attack_phase"]] == FIRED]
    assert launches, f"{arm}: the scene drifted: the Bowler never launched"
    assert launches[0] < i, f"{arm}: the scene drifted: no boulder before the crossing on {i + 1}"
    first = launches[0]
    let_go = [t + 1 for t in range(first, i) if rows[t][0][F["target_uid"]] != bomber_uid]
    assert not let_go, f"{arm}: the scene drifted: the Bowler lost the Bomber on {let_go} before the crossing"
    held, moved = [], []
    for t in range(i, len(rows)):
        h = rows[t][0]
        if h[F["target_uid"]] != bomber_uid:
            held.append(t + 1)
        if (h[F["x"]], h[F["y"]]) != (rows[first][0][F["x"]], rows[first][0][F["y"]]):
            moved.append(t + 1)
        if h[F["attack_phase"]] == FIRED and t > first:
            break
    assert not held, (
        f"{arm}: the Bomber slid past the limit on {i + 1} (start-of-tick {rows[i][2]:.0f} > {BOWLER_LIMIT}) "
        f"and the Bowler let it go on {held[:3]}"
    )
    assert not moved, f"{arm}: the Bowler walked on {moved[:3]} while it held the Bomber"
    second = next((t for t in launches if t > first), None)
    assert second == first + BOWLER_HIT_SPEED_TICKS, (
        f"{arm}: the Bowler's boulders came on {first + 1} and {None if second is None else second + 1}, "
        f"not one HitSpeed apart"
    )


def test_a_target_pushed_past_the_limit_is_kept():
    assert_kept_through_the_push(NEW_ARM)


def test_old_arm_is_the_pre_flip_engine():
    rows = bowler(OLD_ARM)
    bomber_uid = rows[0][1][F["uid"]]
    i = crossing(rows, BOWLER_LIMIT)
    assert i is not None, "old arm: the scene drifted: the Bomber never crossed the limit"
    assert rows[i][1][F["knockback_ticks"]] > 0, "old arm: the scene drifted: the crossing is not a slide"
    assert rows[i][0][F["target_uid"]] != bomber_uid, f"old arm: the Bowler kept the sliding Bomber on {i + 1}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_runner_that_is_not_sliding_is_let_go_on_the_edge(arm):
    rows = run(arm, ["Knight", "HogRider"], 1, KNIGHT_AT, HOG_AT, 120)
    hog_uid = rows[0][1][F["uid"]]
    i = crossing(rows, KNIGHT_LIMIT)
    assert i is not None, f"{arm}: the scene drifted: the Hog Rider never crossed the Knight's limit {KNIGHT_LIMIT}"
    assert all(r[1][F["knockback_ticks"]] == 0 for r in rows), f"{arm}: the scene drifted: the Hog Rider was pushed"
    assert rows[i - 1][0][F["target_uid"]] == hog_uid, (
        f"{arm}: the scene drifted: the Knight did not hold the Hog Rider"
    )
    assert rows[i][0][F["target_uid"]] != hog_uid, (
        f"{arm}: the Hog Rider crossed {KNIGHT_LIMIT} on {i + 1} (start-of-tick {rows[i][2]:.0f}) "
        f"and the Knight kept it"
    )
