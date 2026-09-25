"""The Tornado's attract, against the law measured from capture 20260920-081819.

WHY THESE ASSERTIONS AND NOT A PINNED TRACK. A per-tick position pin would fail for any
reason at all -- a pathfinder change, a contact change, the Knight's own walk -- and say
"Tornado" while meaning something else. What is pinned here is the SHAPE the measurement
established and that nothing else in the engine produces:

  * the victim closes on the cast point while its own walk carries it AWAY, so a run with
    the attract removed does not merely score worse, it moves in the opposite direction;
  * the approach rate is the pull MINUS the walk, near trunc(speed * 360 / 100) = 216 for
    a Knight against its own ~57 native per tick;
  * it OVERSHOOTS the centre rather than stopping on it, which `Vec2::step_toward` cannot
    express (it returns the target whenever the step exceeds the distance);
  * and it stops with the area effect rather than with the buff, 21 ticks after the cast.

`status.ATTRACT_LAW` carries the evidence for each of those.
"""

from __future__ import annotations

import json
import math
from itertools import pairwise

import pytest

royalesim = pytest.importorskip("royalesim")

TILE = 18_000  # subtiles
K = 18  # subtiles per native millitile
DECK = ["Tornado", "Knight", "Archer", "Musketeer", "Giant", "Minions", "Cannon", "Tesla"]
KNIGHT = DECK.index("Knight")
#: The cast is deliberately BEHIND the enemy knight, so the pull and the knight's own walk
#: oppose each other. A cast in front of it would be satisfied by a unit that simply kept
#: walking, which is the control this test needs to fail.
CAST = (9 * TILE, 12 * TILE)
START = (9 * TILE, 10 * TILE)


def battle_with_a_knight_in_the_tornado():
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    ids = list(range(len(DECK)))
    # start_tick past match.DEPLOY_LOCKOUT_TICKS, or the cast is refused for its timing
    b.reset(0, [ids, ids], 0, 200, [10_000, 10_000], None, [(1, KNIGHT, *START, -1)])
    return b


def knight_of(b):
    for e in json.loads(b.state_json())["entities"]:
        if e[1] == 1 and e[3] == KNIGHT:
            return e
    return None


def distance_native(e):
    return math.hypot(CAST[0] - e[5], CAST[1] - e[6]) / K


def test_the_tornado_pulls_its_victim_against_its_own_walk():
    b = battle_with_a_knight_in_the_tornado()
    before = distance_native(knight_of(b))
    b.step([(0, 0, *CAST)], 1)
    track = []
    for _ in range(10):
        track.append(distance_native(knight_of(b)))
        b.step([], 1)

    assert track[0] < before, (
        "the knight did not move toward the cast point on the first tick: "
        f"{before:.0f} -> {track[0]:.0f} native"
    )
    # STRICTLY closing, every tick of the approach
    assert all(b_ < a for a, b_ in pairwise(track)), f"the approach is not monotone: {track}"
    # and closing at the pull minus the walk, not at a walk's pace. A Knight walks about 57
    # native per tick; anything under 100 here is a unit that is walking, not being pulled.
    steps = [a - b_ for a, b_ in pairwise(track)]
    assert min(steps) > 100, f"too slow to be an attract: {[round(s) for s in steps]}"
    assert max(steps) < 250, f"faster than the law allows: {[round(s) for s in steps]}"


def test_the_tornado_overshoots_the_centre_rather_than_parking_on_it():
    """The signature a clamped implementation cannot produce."""
    b = battle_with_a_knight_in_the_tornado()
    b.step([(0, 0, *CAST)], 1)
    seen = []
    for _ in range(14):
        seen.append(distance_native(knight_of(b)))
        b.step([], 1)

    closest = min(seen)
    assert closest < 60, f"never reached the centre: closest {closest:.0f} native"
    after = seen[seen.index(closest) + 1:]
    assert after, "ran out of ticks before the crossing"
    assert max(after) > closest + 50, (
        "the victim stopped on the centre instead of crossing it. A clamp at the target "
        f"produces exactly this: closest {closest:.0f}, then {[round(d) for d in after]}"
    )


def test_the_pull_stops_with_the_area_effect_and_not_with_the_buff():
    """21 ticks, life_duration_ms 1050 / TICK_MS 50.

    The buff outlives the area by BuffTime 500 ms with CapBuffTimeToAreaEffectTime false,
    so an implementation that pulled from the buff slot would still be pulling ten ticks
    later. The corpus says it is not.
    """
    b = battle_with_a_knight_in_the_tornado()
    start = distance_native(knight_of(b))
    b.step([(0, 0, *CAST)], 1)
    b.step([], 30)  # well past the 21-tick life
    settled = distance_native(knight_of(b))
    # IT MUST HAVE BEEN PULLED FIRST. Without this line the test passes when there is NO
    # attract at all: a knight that was never pulled walks away under its own power and
    # satisfies the "it left" assertion below for the wrong reason. The plant that sets
    # AttractPercentage to 0 failed the other two tests in this file and PASSED this one,
    # which is how the hole was found -- a test passing while testing less than it says.
    assert settled < start - 500, (
        f"the knight was never drawn in: {start:.0f} -> {settled:.0f} native, so what "
        "follows is not measuring the end of a pull"
    )
    b.step([], 6)
    after = distance_native(knight_of(b))
    # free of the tornado, the knight walks its own way again: it does not hold station on
    # the cast point, which is what a pull that never ended would look like
    assert abs(after - settled) > 100, (
        f"the knight is still being held near the cast point 30 ticks after the cast: "
        f"{settled:.0f} -> {after:.0f} native"
    )


# ---------------------------------------------------------------------------------------------
# PAIRED CONTROLS, the kernel's own method: the same battle run with and without the Tornado.
# The engine is deterministic, so the two runs are identical until the Tornado first acts, and
# the displacement between them on that tick IS the pull -- no model of the victim's walk.
# ---------------------------------------------------------------------------------------------

PAIRED_DECK = ["Tornado", "Zap", "Snowball", "Knight", "Archer", "Musketeer", "Giant", "Minions"]
P_TORNADO, P_ZAP, P_SNOWBALL, P_KNIGHT = 0, 1, 2, 3  # hand slots: no shuffle, the first four
#: an unbuffed Knight's pull, trunc(60 * 360 / 100), and the kernel's measured figure
KNIGHT_PULL = 216


def knight_trace(setup_slot, tornado_at, ticks=90):
    """The enemy Knight's row after every tick: `setup_slot` cast on it at tick 0, and a
    Tornado behind it at `tornado_at` (None = never)."""
    b = royalesim.Battle(card_names=PAIRED_DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    ids = list(range(len(PAIRED_DECK)))
    b.reset(0, [ids, ids], 0, 200, [10_000, 10_000], None, [(1, P_KNIGHT, *START, -1)])
    rows = []
    for t in range(ticks):
        cmds = []
        if t == 0:
            k = next(e for e in json.loads(b.state_json())["entities"] if e[1] == 1 and e[3] == P_KNIGHT)
            cmds.append((0, setup_slot, k[5], k[6]))
        if tornado_at is not None and t == tornado_at:
            cmds.append((0, P_TORNADO, *CAST))
        b.step(cmds, 1)
        rows.append(next((e for e in json.loads(b.state_json())["entities"] if e[1] == 1 and e[3] == P_KNIGHT), None))
    return rows


def step_of(rows, t):
    return math.hypot(rows[t][5] - rows[t - 1][5], rows[t][6] - rows[t - 1][6]) / K


def paired_first_pull(setup_slot, in_state):
    """ORACLE'S PROCEDURE, so a state that has not landed yet cannot be measured by mistake.

    First a run WITHOUT the Tornado finds the first tick the Knight is in the state
    (`in_state(rows, t)`); the Tornado is cast on that tick in a second run; and the pull is the
    displacement between the two runs on the first tick they differ, which must still be a tick
    the Knight is in the state. Returns (pull, the no-Tornado row that tick, its own step).
    """
    without = knight_trace(setup_slot, None)
    t_state = next((t for t in range(1, len(without)) if without[t] is not None and in_state(without, t)), None)
    assert t_state is not None, "the Knight never entered the state in 90 ticks; the scene measures nothing"
    with_ = knight_trace(setup_slot, t_state)
    for t, (ra, rb) in enumerate(zip(without, with_, strict=True)):
        assert ra is not None, f"the Knight died by tick {t} without the Tornado; the scene measures nothing"
        assert rb is not None, f"the Knight died by tick {t} with the Tornado; the scene measures nothing"
        if (ra[5], ra[6]) != (rb[5], rb[6]):
            assert in_state(without, t), f"the Tornado first acted on tick {t}, after the state had ended"
            return math.hypot(rb[5] - ra[5], rb[6] - ra[6]) / K, ra, step_of(without, t)
    raise AssertionError("the two runs never differed: the Tornado never pulled the Knight")


def test_a_stunned_victim_is_still_pulled():
    """status.ATTRACT_WHILE_HELD = pulled. The kernel pulls a Zap-stunned Knight 216.1.

    The hold stops the WALK. It must not anchor the unit: the engine used to skip the move pass
    for a held unit, and the pull lives in that pass, so a stunned victim was never moved.
    """
    pull, row, _ = paired_first_pull(P_ZAP, lambda rows, t: rows[t][12] > 0)
    assert row[12] > 0
    assert abs(pull - KNIGHT_PULL) <= 3, f"a stunned Knight was pulled {pull:.1f}, not the unbuffed {KNIGHT_PULL}"


def test_the_pull_ignores_a_slow():
    """status.ATTRACT_LAW = base_speed. The kernel pulls a slowed Knight 215.7, not its slowed figure.

    THE PREMISE IS CHECKED, not assumed: the Knight must actually be walking slower than its own
    ~59 on the measured tick, or a "216" here would be an unslowed Knight passing for a slowed one.
    """
    pull, row, step = paired_first_pull(P_SNOWBALL, lambda rows, t: rows[t][12] == 0 and 0 < step_of(rows, t) < 55)
    assert row[12] == 0, f"the Knight is stunned, not merely slowed, on the measured tick: {row}"
    assert 0 < step < 55, (
        f"the Knight is not slowed on the measured tick (its step is {step:.1f}), so this does not test the base"
    )
    assert abs(pull - KNIGHT_PULL) <= 3, (
        f"a slowed Knight (step {step:.1f}) was pulled {pull:.1f}: the pull followed the buffed "
        f"speed, where the kernel pulls the unbuffed {KNIGHT_PULL}"
    )
