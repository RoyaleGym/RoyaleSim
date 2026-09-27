"""movement.SPAWN_PATHFIND_STEP: how one tick's step of a unit under ground is taken (state.rs `tunnel_step`).

THE LAW (client_250_substeps), measured on client 16.402 (capture 20260920-083112, the seat that saw every first frame:
the five tunnels of its three Goblin Drills and two Miners) and on client 15.535.29 (37 Miner and Goblin Drill tunnel
runs over 20 distinct routes): a tick's SpawnPathfindSpeed is taken in sub-steps of at most 250, each straight at the
route's next node and shortened to it when it is nearer, and after each sub-step that node is dropped when it is within
the speed + 1. It reproduces 1956 of 1956 recorded tunnel frames exactly, 42 of 42 creation points and 41 of 41
surfacing ticks. The old arm, one_step (the engine before the key: the whole speed in one piece, stopped at a nearer
node, then every node within the speed dropped), reproduces 0 of the frames exactly, 21 of the creation points and 23
of the surfacing ticks: its Miner comes up a tick late wherever its route's first node lies within 650 of its King.

WHAT THIS PINS, through the protocol:
  1. the old arm is the engine before the key: a Miner played to (3500, 1500) is first seen at (8101, 3249);
  2. under client_250_substeps the recorded tunnels, frame by frame: the Miner played to (3500, 1500) (client 16.402
     tick 2947 and client 15.535.29 alike), the Miner played on its own King's tile, which comes up on (8500, 500) the
     tick after its first frame (client 16.402 tick 1203), and the Goblin Drill played to (3500, 23500).

It needs an extension built from this tree: a build that predates the key refuses the override, and the new-arm case
fails saying so. PLANT: `tunnel_substep_whole_speed` (the sub-step arm takes the whole speed in one piece): 2 goes red.
"""

from __future__ import annotations

import pytest

DECK = ["Miner", "GoblinDrill", "Knight", "Archer", "Giant", "Minions", "Musketeer", "Valkyrie"]
IDS = list(range(len(DECK)))
MINER, DRILL = DECK.index("Miner"), DECK.index("GoblinDrill")
KEY = "movement.SPAWN_PATHFIND_STEP"


@pytest.fixture(scope="module")
def royalesim():
    return pytest.importorskip("royalesim")


def tunnel(rs, overrides, slot: int, at: tuple[int, int], ticks: int) -> list:
    """Blue plays hand `slot` at `at` (native), 200 ticks in with full elixir. Returns the played unit's (x, y), native,
    on each tick from its first frame, and None once it is gone (a dig that left its building)."""
    sub = rs.SUBTILE_PER_MILLITILE
    b = rs.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, [])
    [(_card, reason, _tick, _rx, _ry)] = b.step([(0, slot, at[0] * sub, at[1] * sub)], 1)
    assert rs.DEPLOY_REASONS[reason] == "OK", rs.DEPLOY_REASONS[reason]
    uid = None
    out = []
    for _ in range(ticks):
        units = [u for u in b.debug_units() if u[0] % 2 == 0 and u[1] == DECK[slot]]
        if uid is None and units:
            uid = units[0][0]
        if uid is not None:
            u = next((u for u in units if u[0] == uid), None)
            out.append(None if u is None else (u[2] // sub, u[3] // sub))
        b.step([], 1)
    return out


def test_the_old_arm_is_the_engine_before_the_key(royalesim):
    got = tunnel(royalesim, None, MINER, (3500, 1500), 8)
    assert got[:2] == [(8101, 3249), (7451, 3249)], got
    assert got[6] != (3500, 1500), f"one_step is still under ground on the seventh frame: {got}"
    assert got[7] == (3500, 1500), f"one_step comes up on the eighth frame: {got}"


RECORDED = [
    # the Miner west to (3500, 1500): client 16.402 20260920-083112 tick 2947, client 15.535.29 the same numbers
    ("Miner west", MINER, (3500, 1500),
     [(7777, 3235), (7127, 3241), (6477, 3243), (5827, 3245), (5240, 2976), (4718, 2594), (3500, 1500)]),
    # the Miner on its own King's tile, moved to (8500, 500): client 16.402 20260920-083112 tick 1203
    ("Miner on the King tile", MINER, (8500, 1500), [(9235, 1777), (8500, 500)]),
    # the Goblin Drill to (3500, 23500): client 16.402 20260920-083112 tick 2974, client 15.535.29 the same numbers
    ("Goblin Drill north", DRILL, (3500, 23500), [(9178, 3569), (9208, 3866), (9235, 4164), (9241, 4464)]),
]


@pytest.mark.parametrize(("what", "slot", "at", "want"), RECORDED, ids=[r[0] for r in RECORDED])
def test_the_client_arm_walks_the_recorded_tunnels(royalesim, what, slot, at, want):
    try:
        overrides = {KEY: '"client_250_substeps"'}
        royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    except Exception as e:
        pytest.fail(f"this build predates {KEY} ({e}): rebuild the extension from this tree")
    got = tunnel(royalesim, overrides, slot, at, len(want))
    assert got == want, f"{what}: the engine {got}, the client {want}"
