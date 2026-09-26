"""A walking chaser drops a runner 1000 short of its sight (targeting.CHASE_DROP_RANGE).

WHAT THIS PINS. On client 15.535.29 (the chase-drop scenarios: a P.E.K.K.A, a Knight, a Prince or a Mini
P.E.K.K.A walking down the right lane, a Hog Rider or a Battle Ram played behind it and outrunning it up the lane) the
chaser drops the runner for the crown tower on the first tick whose start-of-tick distance exceeds SightRange + both
collision radii - 1000: 18 of 18 drops fit one constant in [-1008, -986). Today's engine keeps the runner to SightRange
+ both radii.
The runs keep the runner on the chaser's lane (|dx| <= 6), so the distance measure off the lane is not pinned here.

THE DROP IS AN EDGE. The chaser lets a target go only on the tick it crosses the limit, having been within it at the
previous tick. A troop it takes past the limit (at plain sight) is kept while it stays past. On client 15.535.29 two
Knights meeting at the left bridge take each other at a start-of-tick distance of 6423, past their limit of 5500, and
keep each other on all 7 ticks until they are inside; in the chase scenarios 31 of 31 such ticks are keeps, one of
them a Knight walking away for 16 ticks. A level-triggered drop would let go on the tick after the take.

WHY THE CONTROL IS HERE. A Knight after a slower runner (a Giant) never falls behind, so it keeps it on both arms. An
implementation that drops every chase after some time fails it.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `chase_drop_ignored` -- the chaser keeps the runner to its plain sight, as today:
    test_a_chaser_drops_the_runner_1000_short_of_its_sight.
  * `chase_drop_rescan_admits` -- the dropped troop is a candidate again at plain sight on the next rescan:
    test_a_chaser_drops_the_runner_1000_short_of_its_sight.
  * `chase_drop_level_triggered` -- a target taken past the limit is let go on the next tick:
    test_a_troop_taken_past_the_limit_is_kept.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.CHASE_DROP_RANGE"
NEW_ARM, OLD_ARM = "client_sight_minus_1000", "sight_plus_radii"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: the red chaser played on the right lane's red bank; the blue runner played behind it once it has walked past
CHASER_AT, RUNNER_AT = (14500, 18500), (14500, 14500)
#: chaser -> (SightRange + its radius + the Hog Rider's 600, the tick the Hog is played)
CHASERS = {"Pekka": (5000 + 750 + 600, 192), "Knight": (5500 + 500 + 600, 134)}
DROP_BY = 1000
#: two Knights played on tick 2 in the left lane, one on each side of the river: blue (team 0) and red (team 1)
MEET_AT = {0: (3500, 12500), 1: (3500, 19500)}
KNIGHT_LIMIT = 5500 + 500 + 500 - DROP_BY


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def chase(arm, chaser, runner, runner_tick, ticks=420):
    """(the drop tick, the runner's start-of-tick centre distance on it and on the tick before), or None if kept."""
    b = royalesim.Battle([chaser, runner], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, [])
    prev, c_uid, r_uid, dists = None, None, None, []
    for t in range(1, ticks + 1):
        plays = [(1, 0, CHASER_AT[0] * SUB, CHASER_AT[1] * SUB)] if t == 2 else []
        if t == runner_tick:
            plays.append((0, 0, RUNNER_AT[0] * SUB, RUNNER_AT[1] * SUB))
        b.step(plays, 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        for e in now.values():
            if e[F["tower_slot"]] < 0 and e[F["team"]] == 1 and c_uid is None:
                c_uid = e[F["uid"]]
            if e[F["tower_slot"]] < 0 and e[F["team"]] == 0 and r_uid is None:
                r_uid = e[F["uid"]]
        if prev and c_uid in prev and r_uid in prev and c_uid in now and r_uid in now:
            c0, r0 = prev[c_uid], prev[r_uid]
            d = ((r0[F["x"]] - c0[F["x"]]) ** 2 + (r0[F["y"]] - c0[F["y"]]) ** 2) ** 0.5 / SUB
            dists.append(d)
            if c0[F["target_uid"]] == r_uid and now[c_uid][F["target_uid"]] != r_uid:
                return t, d, dists[-2]
        prev = now
    return None


def meet(arm, ticks=60):
    """Per team: its Knight's take tick of the other Knight and, from the take on, (tick, start-of-tick distance,
    whether it still holds the other Knight); None if it never took it."""
    b = royalesim.Battle(["Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[0] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, [])
    prev, uid, out = None, {}, {0: None, 1: None}
    for t in range(1, ticks + 1):
        b.step([(team, 0, x * SUB, y * SUB) for team, (x, y) in MEET_AT.items()] if t == 2 else [], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        for u, e in now.items():
            if e[F["tower_slot"]] < 0:
                uid.setdefault(e[F["team"]], u)
        if prev and all(uid.get(k) in prev and uid.get(k) in now for k in (0, 1)):
            b0, r0 = prev[uid[0]], prev[uid[1]]
            d = max(abs(r0[F["x"]] - b0[F["x"]]), abs(r0[F["y"]] - b0[F["y"]])) / SUB
            for team in (0, 1):
                holds = now[uid[team]][F["target_uid"]] == uid[1 - team]
                if out[team] is None and holds:
                    out[team] = (t, [])
                if out[team] is not None:
                    out[team][1].append((t, d, holds))
        prev = now
    return out


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_troop_taken_past_the_limit_is_kept(arm):
    for team, got in meet(arm).items():
        assert got is not None, f"{arm}: team {team}'s Knight never took the other Knight"
        take, rows = got
        # The scene must test the case: taken past the limit, and still past it on the next tick.
        assert len(rows) > 1, f"{arm}: team {team}'s Knight took the other on {take}, the last tick run"
        past = [d > KNIGHT_LIMIT for _, d, _ in rows[:2]]
        assert all(past), (
            f"{arm}: team {team}'s Knight took the other on {take} at {rows[0][1]:.0f}, not past {KNIGHT_LIMIT} "
            f"for two ticks; the scene no longer tests a take past the limit"
        )
        for t, d, holds in rows:
            if d <= KNIGHT_LIMIT:
                break
            assert holds, (
                f"{arm}: team {team}'s Knight took the other on {take} at {rows[0][1]:.0f} and let it go "
                f"on {t} at {d:.0f}, still past the limit {KNIGHT_LIMIT}: a level-triggered drop"
            )


@pytest.mark.parametrize("chaser", sorted(CHASERS))
def test_a_chaser_drops_the_runner_1000_short_of_its_sight(chaser):
    limit, tick = CHASERS[chaser]
    got = chase(NEW_ARM, chaser, "HogRider", tick)
    assert got is not None, f"the {chaser} never dropped the Hog Rider"
    t, dropped, held = got
    assert held <= limit - DROP_BY < dropped, (
        f"the {chaser} dropped the Hog Rider on {t} at a start-of-tick distance of {dropped:.0f} (held at {held:.0f}), "
        f"not on crossing {limit - DROP_BY}"
    )


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_knight_keeps_a_slower_runner(arm):
    assert chase(arm, "Knight", "Giant", 134, ticks=300) is None, f"{arm}: the Knight dropped a Giant it was catching"


def test_old_arm_is_todays_engine():
    limit, tick = CHASERS["Pekka"]
    got = chase(OLD_ARM, "Pekka", "HogRider", tick)
    assert got is not None, "old arm: the P.E.K.K.A never dropped the Hog Rider"
    t, dropped, held = got
    assert held <= limit < dropped, f"old arm: dropped on {t} at {dropped:.0f} (held at {held:.0f}), not on {limit}"
