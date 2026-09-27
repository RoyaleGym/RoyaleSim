"""The avoidance scan's waypoint drop keeps the frozen segment (pathfinding.AVOIDANCE_DROP_SEGMENT).

WHAT THIS PINS. A walking unit's avoidance scan drops its next waypoint when a static neighbour's circle holds the
waypoint's centre (move16402.rs `avoidance_scan`). On client 16.402 (61 battles) and client 15.535.29 (792 scenario
runs) the frozen segment direction is then left as it was, 11 of 11 (8 moments) and 2 of 2; a waypoint the reached test
pops refreezes it from the end-of-tick position, 42314 of 42314 and 21282 of 21282. The engine before this key clears it
on the drop, and the step refreezes it from the start of the tick toward the new waypoint. The reached test measures the
new waypoint along the segment, so the two arms can take the next waypoint on different ticks: in 20260920-002736 a
Skeleton behind the Red King Tower keeps (255, -19) after a drop on tick 756, and on 768 the client takes its next
waypoint (921 along that direction) where the engine does not (1119 along its refrozen (235, -101)).

THE SCENE. A Blue Knight put down at (5250, 9250) walks up and to the left toward the left lane. Blue Goblins played at
(3750, 11250) on tick 106 lay their members in front of it, and they wait out the deploy stagger as static obstacles
(movement.WAITING_HEADING = static_obstacle). The Knight's scan drops the waypoints whose centres they hold. On this
engine at the old arm it drops (8, 20) on tick 107 and (8, 21) on 108.

WHAT IS CHECKED, on every tick the Knight's route loses its next waypoint and nothing else, by the segment direction
after the tick: 'end' is the refreeze from the end-of-tick position (a reached pop, under both arms); 'kept' is the
direction of the tick before (a drop under client16402_kept); 'start' is the refreeze from the start-of-tick position
(a drop under refrozen).

PLANT. `avoidance_drop_refreezes` (state.rs, the drop clears the segment under either arm): run on a plant build of the
module (`RUSTFLAGS='--cfg clash_plant="avoidance_drop_refreezes"' CARGO_TARGET_DIR=target/plant maturin develop
--release` in a scratch venv); test_a_drop_keeps_the_segment goes red and the rest stay green.
"""

from __future__ import annotations

import itertools
import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "pathfinding.AVOIDANCE_DROP_SEGMENT"
NEW_ARM, OLD_ARM = "client16402_kept", "refrozen"
KNIGHT, TAP, PLAY_ON, TICKS = (5250, 9250), (3750, 11250), 106, 60


def isqrt(n: int) -> int:
    if n <= 0:
        return 0
    x = int(n**0.5)
    while x * x > n:
        x -= 1
    while (x + 1) * (x + 1) <= n:
        x += 1
    return x


def tdiv(a: int, b: int) -> int:
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q


def norm256(p: tuple[int, int], cell: tuple[int, int]) -> tuple[int, int]:
    """The segment direction from p toward a half-tile cell's centre, length 256 (move16402.rs `segment_dir`)."""
    dx, dy = cell[0] * 500 + 250 - p[0], cell[1] * 500 + 250 - p[1]
    n = isqrt(dx * dx + dy * dy)
    return (tdiv(dx * 256, n), tdiv(dy * 256, n)) if n else (dx, dy)


def scene(arm: str | None, goblins: bool = True) -> list[tuple]:
    """Per tick after the tick: (tick, the Knight's position, its route goal first, its segment direction). `arm` None
    runs the build's own value."""
    overrides = {KEY: json.dumps(arm)} if arm is not None else {}
    b = royalesim.Battle(["Knight", "Goblins"], [[0, 1, 1], [0, 1, 1]], calibration_overrides=overrides)
    b.reset(2, [[1] * 8, [0] * 8], 0, 100, [10_000, 10_000], None, [(0, 0, KNIGHT[0] * SUB, KNIGHT[1] * SUB, -1)])
    knight = None
    rows = []
    for _ in range(TICKS):
        tick = json.loads(b.state_json())["tick"]
        cmds = [(0, 0, TAP[0] * SUB, TAP[1] * SUB)] if goblins and tick == PLAY_ON else []
        played = b.step(cmds, 1)
        if cmds:
            assert played, "the Goblins play returned nothing"
            assert played[0][1] == 0, f"the Goblins play was refused: {played}"
        units = {u[0]: u for u in b.debug_units()}
        if knight is None:
            knight = next(uid for uid, u in units.items() if u[1] == "Knight")
        u = units.get(knight)
        assert u is not None, f"the scene drifted: the Knight is gone on tick {tick + 1}"
        c = next(c for c in b.debug_contact() if c[0] == knight)
        rows.append((tick + 1, (u[2] // SUB, u[3] // SUB), [tuple(n) for n in u[6]], (c[2], c[3])))
    return rows


def pops(rows: list[tuple]) -> list[tuple]:
    """(tick, the waypoint lost, the class) for every tick whose route lost its next waypoint and nothing else."""
    out = []
    for before, after in itertools.pairwise(rows):
        t, pos, route, seg = after
        _, pos0, route0, seg0 = before
        if len(route0) < 2 or route != route0[:-1]:
            continue
        start, end = norm256(pos0, route[-1]), norm256(pos, route[-1])
        if seg == end:
            cls = "end"
        elif seg == seg0 and seg != start:
            cls = "kept"
        elif seg == start and seg != seg0:
            cls = "start"
        else:
            cls = "ambiguous"
        out.append((t, route0[-1], cls, seg0, seg, start, end))
    return out


def test_a_drop_keeps_the_segment():
    got = pops(scene(NEW_ARM))
    assert any(p[2] != "end" for p in got), f"new arm: the scene drifted: the Knight's scan dropped nothing {got}"
    refrozen = [p for p in got if p[2] == "start"]
    assert not refrozen, "new arm: " + "; ".join(
        f"on tick {t} the scan dropped {cell} and the segment was refrozen from the start of the tick to "
        f"{seg} (it had {seg0})"
        for t, cell, _, seg0, seg, _, _ in refrozen
    )
    assert any(p[2] == "kept" for p in got), f"new arm: no drop kept the segment: {got}"


def test_the_old_arm_refreezes_from_the_start_of_the_tick():
    got = pops(scene(OLD_ARM))
    kept = [p for p in got if p[2] == "kept"]
    assert not kept, f"old arm: a drop kept the segment: {kept}"
    assert any(p[2] == "start" for p in got), f"old arm: the scene drifted: no drop refroze the segment: {got}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_reached_pop_refreezes_from_the_end_of_the_tick(arm):
    got = pops(scene(arm, goblins=False))
    assert len(got) >= 4, f"{arm}: the scene drifted: the Knight popped {len(got)} waypoints"
    other = [p for p in got if p[2] != "end"]
    assert not other, f"{arm}: with nothing to drop a waypoint, a pop did not refreeze from the end: {other}"


def test_without_a_drop_the_two_arms_walk_the_same_track():
    new, old = scene(NEW_ARM, goblins=False), scene(OLD_ARM, goblins=False)
    parted = [(a[0], a[1], b[1]) for a, b in zip(new, old, strict=True) if a[1:] != b[1:]]
    assert not parted, f"the arms part with nothing dropped: {parted[:3]}"


def test_the_shipped_value_is_the_old_arm():
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    assert ledger["pathfinding"]["AVOIDANCE_DROP_SEGMENT"]["value"] == OLD_ARM
