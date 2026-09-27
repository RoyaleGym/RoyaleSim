"""After a kill, a swing runs on only onto a target in reach (combat.CORPSE_SWITCH_REACH).

WHAT THIS PINS. combat.RETARGET_PROGRESS = keep_when_dead: replacing a dead target is not a switch, so the swing runs
on (a princess tower killing one-shot Skeletons keeps its 16-tick cadence). On client 15.535.29 and the 16.402 corpus
that holds only when the new target stands in attack range: of the kills whose next target was named at once (no
post-kill wait), 194 of 196 in reach kept the swing and 23 of 23 out of reach dropped it (progress 0, walking), 13 of
them mid-swing: a Bowler whose boulder killed a Skeleton walked on the next tick after a Knight out of its reach
(20260920-081819, 1673 and 1674). Today's engine keeps the swing whatever the range, so the Bowler stands until the
swing lands.
The scene: a blue Bowler at (9500, 8000), a red Skeleton at (9500, 12500) and a red Knight at (9500, 14700), with
combat.RANGE_PROJECTILE = straight_to_range. The Bowler's first boulder kills the Skeleton some ticks after the launch,
mid-swing, and the Bowler's next target is the Knight, still out of its reach (4000 + 750 + 500 = 5,250).

WHY THE CONTROL IS HERE. A princess tower killing one-shot Skeletons replaces each with one in range: the swing must
run on under both values, so its cadence stays at its HitSpeed. An implementation that drops every swing across a
kill fails it.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `corpse_switch_keeps_any` -- client_in_reach_only still keeps the swing onto a target out of reach:
    test_a_kill_followed_by_a_target_out_of_reach_drops_the_swing.
"""

from __future__ import annotations

import json
from itertools import pairwise

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.CORPSE_SWITCH_REACH"
NEW_ARM, OLD_ARM = "client_in_reach_only", "keeps_any"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
BASE = {"combat.RANGE_PROJECTILE": "straight_to_range"}
BOWLER_AT, SKELETON_AT, KNIGHT_AT = (9500, 8000), (9500, 12500), (9500, 14700)
BOWLER_REACH = 4000 + 750 + 500
IDLE, WINDUP, FIRED = 0, 1, 2
#: the control, test_retarget_cadence.py's swarm: the blue left princess tower and eight one-shot Skeletons
TILE = 18_000
DECK = ["Skeletons", "Giant", "Musketeer", "Archer", "Knight", "Minions", "Cannon", "Tesla"]


def overrides(arm) -> dict:
    """`arm` None runs the build's own value of KEY."""
    over = dict(BASE)
    if arm is not None:
        over[KEY] = arm
    return {k: json.dumps(v) for k, v in over.items()}


def bowler(arm, ticks=40):
    """Per tick t (1..): (Bowler row, Skeleton alive, Knight row, the Knight's start-of-tick centre distance)."""
    b = royalesim.Battle(
        ["Bowler", "Skeletons", "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm)
    )
    spawns = [(0, 0, *(v * SUB for v in BOWLER_AT), -1), (1, 1, *(v * SUB for v in SKELETON_AT), -1)]
    spawns.append((1, 2, *(v * SUB for v in KNIGHT_AT), -1))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, spawns)

    def units():
        es = [e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0]
        return {e[F["card_id"]]: e for e in es}  # 0 the Bowler, 1 the Skeleton, 2 the Knight

    prev, rows = units(), []
    for _ in range(ticks):
        b.step([], 1)
        now = units()
        pb, pk = prev[0], prev[2]
        d = ((pk[F["x"]] - pb[F["x"]]) ** 2 + (pk[F["y"]] - pb[F["y"]]) ** 2) ** 0.5 / SUB
        rows.append((now[0], 1 in now, now[2], d))
        prev = now
    return rows


def the_switch(arm):
    """(rows, the launch index, the kill index, the index of the first tick the Bowler names the Knight)."""
    rows = bowler(arm)
    knight_uid = rows[0][2][F["uid"]]
    launch = next((t for t, r in enumerate(rows) if r[0][F["attack_phase"]] == FIRED), None)
    kill = next((t for t, r in enumerate(rows) if not r[1]), None)
    assert launch is not None, f"{arm}: the scene drifted: the Bowler never launched"
    assert kill is not None, f"{arm}: the scene drifted: the Skeleton never died"
    assert kill > launch + 1, f"{arm}: the scene drifted: the kill on {kill + 1} is not mid-swing (launch {launch + 1})"
    first = rows[0][0]
    moved = [t + 1 for t in range(kill) if (rows[t][0][F["x"]], rows[t][0][F["y"]]) != (first[F["x"]], first[F["y"]])]
    assert not moved, f"{arm}: the scene drifted: the Bowler moved on {moved} before the kill"
    switch = next((t for t in range(kill, len(rows)) if rows[t][0][F["target_uid"]] == knight_uid), None)
    assert switch is not None, f"{arm}: the scene drifted: the Bowler never took the Knight"
    assert rows[switch][3] > BOWLER_REACH, (
        f"{arm}: the scene drifted: the Knight stood {rows[switch][3]:.0f}, inside the reach {BOWLER_REACH}"
    )
    return rows, launch, kill, switch


def assert_the_swing_is_dropped(arm):
    rows, launch, kill, switch = the_switch(arm)
    phase = rows[switch][0][F["attack_phase"]]
    assert phase == IDLE, (
        f"{arm}: the Skeleton died on {kill + 1} mid-swing (boulder launched on {launch + 1}) and the Bowler took "
        f"the Knight {rows[switch][3]:.0f} away (reach {BOWLER_REACH}) on {switch + 1} with its swing running "
        f"(phase {phase})"
    )
    b0, b1 = rows[switch][0], rows[min(switch + 1, len(rows) - 1)][0]
    assert (b0[F["x"]], b0[F["y"]]) != (b1[F["x"]], b1[F["y"]]), f"{arm}: the Bowler did not walk after {switch + 1}"


def test_a_kill_followed_by_a_target_out_of_reach_drops_the_swing():
    assert_the_swing_is_dropped(NEW_ARM)


def test_old_arm_is_todays_engine():
    rows, _, _, switch = the_switch(OLD_ARM)
    assert rows[switch][0][F["attack_phase"]] == WINDUP, (
        f"old arm: the swing did not run on across the kill on {switch + 1}"
    )


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_tower_replacing_its_victims_in_range_keeps_its_cadence(arm):
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    skels = [(1, 0, int((2.5 + 0.5 * i) * TILE), int((8.5 + 0.4 * (i % 3)) * TILE), -1) for i in range(8)]
    ids = list(range(len(DECK)))
    b.reset(0, [ids, ids], 0, 200, [10_000, 10_000], None, skels)
    prev, deaths = None, []
    for t in range(200):
        red = {e[0] for e in json.loads(b.state_json())["entities"] if e[1] == 1 and e[4] < 0}
        if prev is not None:
            deaths += [t] * len(prev - red)
        prev = red
        b.step([], 1)
    steady = [y - x for x, y in pairwise(deaths)][2:]
    assert len(steady) >= 4, f"{arm}: the scene drifted: too few kills {deaths}"
    assert max(steady) <= 17, f"{arm}: the tower's kills came {steady} apart, slower than its 16-tick cadence"
