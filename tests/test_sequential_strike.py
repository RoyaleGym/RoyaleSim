"""A walker sees a kill made earlier in the same tick by a unit created before it (match.TICK_ORDER).

WHAT THIS PINS. On client 15.535.29 the units take their target decision and their strike one after another in
creation order, and a direct (melee) strike's damage lands at once. So when a melee strike kills a walker's target, the
walker turns for its next goal on that same tick if the striker was created before it, and on the next tick if the
striker was created after it. Measured over the 775 client 15.535.29 scenario runs: 11 of 11 walkers whose target fell
to a strike by an earlier-created unit held their next target on the frame the target was gone, and 9 of 9 whose
target fell to a later-created striker held it one frame later. Of 23 walkers whose target died with no melee strike
that tick (a shot), 22 held their next target one frame later. Three more walkers, one in each class, waited 6 frames:
that is the post-kill wait, another rule.

THE SCENARIO. A red Cannon with 60 hp stands at (9500, 14500), out of both blue princess towers' reach, so no arrow
touches it. Two blue Knights are played one tick apart: the striker A at (9500, 13000), which reaches the Cannon and
kills it with its first strike, and the walker U at (9500, 9500), still walking toward the Cannon when it dies. Played
A then U, the striker comes first in creation order; played U then A, it comes second. The test reads U's step on the
tick the Cannon is gone: toward the Cannon's last position (it has not yet seen the kill) or away from it (it has).

WHICH ARM. client16402 is the shipped arm: every unit decides its target before any strike lands, and strikes land at
the end of the tick, so a walker always sees a kill one tick late. client_sequential_strike is the proposed arm. The
tests pin each arm BY NAME through the battle's calibration.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `sequential_strike_buffered` -- the pass runs in creation order but its strikes land at Resolve:
    test_a_walker_turns_on_the_kill_tick_when_the_striker_was_created_first.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "match.TICK_ORDER"
NEW_ARM, OLD_ARM = "client_sequential_strike", "client16402"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CARDS = ["Knight", "Cannon"]
#: The Cannon stands 9,434 and 10,000 from the blue princess towers, past their reach, so only the striker can kill it.
CANNON_AT, CANNON_HP = (9500, 14500), 60
STRIKER_AT, WALKER_AT = (9500, 13000), (9500, 9500)
#: a step within 25 degrees of the direction to the Cannon is still walking at it
TOWARD = math.cos(math.radians(25))


def overrides(arm) -> dict:
    """The battle's calibration pinning `arm` BY NAME."""
    return {KEY: json.dumps(arm)}


def play(arm, striker_first):
    """(the tick the Cannon is gone, the walker's positions by tick, the striker's and walker's uids)."""
    b = royalesim.Battle(CARDS, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(
        0,
        [[0] * 8, [1] * 8],
        0,
        200,
        [100_000, 100_000],
        None,
        [(1, 1, CANNON_AT[0] * SUB, CANNON_AT[1] * SUB, CANNON_HP)],
    )
    first, second = (STRIKER_AT, WALKER_AT) if striker_first else (WALKER_AT, STRIKER_AT)
    plays = {1: [(0, 0, first[0] * SUB, first[1] * SUB)], 2: [(0, 0, second[0] * SUB, second[1] * SUB)]}
    walker = striker = cannon = None
    pos, gone, struck = {}, None, False
    for t in range(1, 60):
        b.step(plays.get(t, []), 1)
        ents = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        if cannon is None:
            (cannon,) = [u for u, e in ents.items() if e[F["tower_slot"]] < 0 and CARDS[e[F["card_id"]]] == "Cannon"]
        knights = {u: e for u, e in ents.items() if e[F["tower_slot"]] < 0 and CARDS[e[F["card_id"]]] == "Knight"}
        for u, e in knights.items():
            at = (e[F["x"]] // SUB, e[F["y"]] // SUB)
            if walker is None and at == WALKER_AT:
                walker = u
            if striker is None and at == STRIKER_AT:
                striker = u
        if walker is not None and walker in ents:
            pos[t] = (ents[walker][F["x"]] / SUB, ents[walker][F["y"]] / SUB)
        if gone is None and cannon not in ents:
            gone = t
            struck = striker is not None and striker in ents and ents[striker][F["attack_phase"]] == 2
            break
        on_it = [e[F["tower_slot"]] for e in ents.values() if e[F["tower_slot"]] >= 0 and e[F["target_uid"]] == cannon]
        assert not on_it, f"the scene drifted: a crown tower targets the Cannon on {t} (slots {on_it})"
    assert gone is not None, "the Cannon never died: the scene drifted"
    assert walker is not None, "precondition: the walker was never seen on its play point"
    assert striker is not None, "precondition: the striker was never seen on its play point"
    # Nothing has died before the Cannon, so uids are fresh and follow the play order.
    assert (striker < walker) == striker_first, "precondition: the play order did not set the creation order"
    assert struck, f"the scene drifted: the striker did not strike on the kill tick {gone}"
    b.step([], 1)
    ents = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
    pos[gone + 1] = (ents[walker][F["x"]] / SUB, ents[walker][F["y"]] / SUB)
    return gone, pos


def toward_cannon(pos, t):
    """cos of the angle between the walker's step on tick t and the direction from its t-1 position to the Cannon."""
    (x0, y0), (x1, y1) = pos[t - 1], pos[t]
    sx, sy = x1 - x0, y1 - y0
    vx, vy = CANNON_AT[0] - x0, CANNON_AT[1] - y0
    assert math.hypot(sx, sy) > 30, f"precondition: the walker did not walk on {t} (step {sx:.0f}, {sy:.0f})"
    return (sx * vx + sy * vy) / (math.hypot(sx, sy) * math.hypot(vx, vy))


def test_a_walker_turns_on_the_kill_tick_when_the_striker_was_created_first():
    gone, pos = play(NEW_ARM, striker_first=True)
    assert toward_cannon(pos, gone - 1) >= TOWARD, "precondition: the walker was not walking at the Cannon"
    assert toward_cannon(pos, gone) < TOWARD, (
        f"the Cannon fell on {gone} to a striker created before the walker, and the walker still stepped at it"
    )


@pytest.mark.parametrize("arm", [OLD_ARM, NEW_ARM])
def test_a_walker_turns_a_tick_late_when_the_striker_was_created_after_it(arm):
    """Control, both arms: the walker decided its target before the later-created striker's strike."""
    gone, pos = play(arm, striker_first=False)
    assert toward_cannon(pos, gone) >= TOWARD, f"the walker turned on the kill tick {gone}, before its striker struck"
    assert toward_cannon(pos, gone + 1) < TOWARD, f"the walker was still walking at the dead Cannon on {gone + 1}"


def test_the_shipped_arm_turns_a_tick_late_in_both_orders():
    for first in (True, False):
        gone, pos = play(OLD_ARM, striker_first=first)
        assert toward_cannon(pos, gone) >= TOWARD, (
            f"client16402 turned the walker on the kill tick (striker first: {first})"
        )
