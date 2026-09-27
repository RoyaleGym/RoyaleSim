"""A Goblin Hut's wave takes no update on its creation tick (spawner.LIFE_STATE_FIRST_UPDATE), and its point is the line
to the aim scaled to SpawnOffset first and turned after (spawner.LIFE_STATE_WAVE_POINT).

WHAT THIS PINS. A Goblin Hut's controller creates each wave's Spear Goblin SpawnOffset (1200) from the hut's centre,
inside the hut's circle, and the contact law pushes it out at up to 150 a tick. On client 15.535.29 every wave (169 of
169) stands on its creation point on its first frame with avoidance offset 0, and is pushed and scanned from the next
frame; on the 16.402 corpus all 79 rows whose creation tick is recorded (both seats' recordings, about 66 distinct
waves in 10 battles) stand 1,198 to 1,201 from the hut. Today's
engine gives the wave its first update on the creation tick, as it does a Tombstone's Skeleton, so on its first frame it
already stands about 1,350 out with offset +-190. The point: the client scales the line to the aim to 1200 first, each
axis truncated, and turns it by the 1024 sine table after, each axis truncated (169 of 169; the engine's one division
fits 43). The scene: a blue hut at (9000, 7000) and a red Cannon at (14000, 12000), whose line (5000, 5000) puts the
first wave at (10086, 7506) by the client's arithmetic and at (10087, 7507) by the engine's.

WHY THE OLD-ARM CASES ARE HERE. The first-update test alone also passes for a build that stops every emitted unit from
stepping on its creation tick (spawner.SPAWNED_FIRST_STEP = none). The Tombstone's Skeletons stay pinned to the step by
tests/test_spawned_first_step.py; here the old first-update arm must be today's engine, so a flip is the only change.
The one-division case reads the old point arithmetic on an unpushed first frame (FIRST_UPDATE at its new arm), which is
not today's engine.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run
this file: the named tests go red and the rest stay green.
  * `life_wave_first_update` -- the new arm still gives the wave its creation-tick update:
    test_a_wave_stands_on_its_creation_point_on_its_first_frame, test_the_wave_point_is_normalised_then_rotated.
  * `life_point_one_division` -- the new arm keeps the one-division arithmetic:
    test_the_wave_point_is_normalised_then_rotated.
"""

from __future__ import annotations

import json
import math
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
FIRST, POINT = "spawner.LIFE_STATE_FIRST_UPDATE", "spawner.LIFE_STATE_WAVE_POINT"
FIRST_NEW, FIRST_OLD = "client16402_next_tick", "creation_tick"
POINT_NEW, POINT_OLD = "client16402_normalise_then_rotate", "one_division"
HUT_AT, CANNON_AT = (9000, 7000), (14000, 12000)
#: SpawnOffset and SingleDeployOffsetAngle of the hut's controller, and the 1024 sine table at 20 and 70 degrees
OFFSET, SIN20, COS20 = 1200, 350, 962
#: one contact push at the 150 cap, less a margin for the truncations
PUSHED = 1340
LEDGER = pathlib.Path(__file__).resolve().parent.parent / "data" / "calibration.json"


def tdiv(a: int, b: int) -> int:
    """Division truncated toward zero, as the engine's integer arithmetic."""
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q


def lower_y_point(normalise_first: bool) -> tuple:
    """The first wave's point for the scene's line: of the two points 20 degrees either side of the line, the one with
    the lower y (they are 580 apart in y, far beyond the 14 of the tie rule)."""
    dx, dy = CANNON_AT[0] - HUT_AT[0], CANNON_AT[1] - HUT_AT[1]
    n = math.isqrt(dx * dx + dy * dy)
    if normalise_first:
        ux, uy = tdiv(OFFSET * dx, n), tdiv(OFFSET * dy, n)
        pts = [(tdiv(ux * COS20 - uy * SIN20, 1024), tdiv(ux * SIN20 + uy * COS20, 1024)),
               (tdiv(ux * COS20 + uy * SIN20, 1024), tdiv(uy * COS20 - ux * SIN20, 1024))]
    else:
        den = n * 1024
        pts = [(tdiv(OFFSET * (dx * COS20 - dy * SIN20), den), tdiv(OFFSET * (dx * SIN20 + dy * COS20), den)),
               (tdiv(OFFSET * (dx * COS20 + dy * SIN20), den), tdiv(OFFSET * (dy * COS20 - dx * SIN20), den))]
    x, y = min(pts, key=lambda p: p[1])
    return (HUT_AT[0] + x, HUT_AT[1] + y)


def battle(overrides: dict):
    return royalesim.Battle(["GoblinHut", "Cannon"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)


def first_frames(overrides: dict, ticks: int = 60) -> list:
    """The first wave's first two frames: [(position, distance from the hut's centre, avoidance offset), ...]."""
    b = battle({k: json.dumps(v) for k, v in overrides.items()})
    b.reset(0, [[0] * 8, [0] * 8], 0, 200, [10_000, 10_000], None,
            [(0, 0, HUT_AT[0] * SUB, HUT_AT[1] * SUB, -1), (1, 1, CANNON_AT[0] * SUB, CANNON_AT[1] * SUB, -1)])
    rows, wave = [], None
    for _ in range(ticks):
        b.step([], 1)
        ents = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0}
        if wave is None:
            wave = next((u for u, e in ents.items() if e[F["team"]] == 0 and e[F["kind"]] == 0), None)
        if wave is not None:
            e = ents[wave]
            p = (e[F["x"]] // SUB, e[F["y"]] // SUB)
            offset = {row[0]: row[1] for row in b.debug_contact()}[wave]
            rows.append((p, math.dist(p, HUT_AT), offset))
            if len(rows) == 2:
                return rows
    raise AssertionError(f"the scene drifted: no wave in {ticks} ticks")


def test_the_scene_separates_the_two_arithmetics():
    """Precondition of the point tests: the two laws put this scene's first wave on different points."""
    assert lower_y_point(True) == (10086, 7506)
    assert lower_y_point(False) == (10087, 7507)


def test_a_wave_stands_on_its_creation_point_on_its_first_frame():
    (_, d0, off0), (_, d1, off1) = first_frames({FIRST: FIRST_NEW})
    first = (f"client16402_next_tick: on its first frame the wave stands {d0:.1f} from the hut with avoidance offset "
             f"{off0}; on its creation point it would stand 1195-1200 out with offset 0")
    assert 1195 <= d0 <= OFFSET, first
    assert off0 == 0, first
    second = (f"client16402_next_tick: on its second frame the wave stands {d1:.1f} out with offset {off1}; its first "
              f"push and scan come there (>= {PUSHED}, +-190)")
    assert d1 >= PUSHED, second
    assert abs(off1) == 190, second


def test_the_old_first_update_arm_is_todays_engine():
    """Today the wave is pushed and scanned on its creation tick: about 1,350 out with +-190 on its first frame."""
    (_, d0, off0), _ = first_frames({FIRST: FIRST_OLD})
    assert d0 >= PUSHED, f"creation_tick: first frame {d0:.1f} out with offset {off0}"
    assert abs(off0) == 190, f"creation_tick: first frame {d0:.1f} out with offset {off0}"


def test_the_wave_point_is_normalised_then_rotated():
    (p0, _, _), _ = first_frames({FIRST: FIRST_NEW, POINT: POINT_NEW})
    assert p0 == lower_y_point(True), (
        f"client16402_normalise_then_rotate: the first wave stands at {p0}; the client's arithmetic puts it at "
        f"{lower_y_point(True)} (one division: {lower_y_point(False)})")


def test_the_one_division_arm_puts_the_unpushed_wave_on_its_point():
    """The old point arm's arithmetic, read on the wave's creation point. FIRST_UPDATE is set to the new arm so the
    first frame is unpushed; this is not today's engine, whose wave is already pushed on its first frame."""
    (p0, _, _), _ = first_frames({FIRST: FIRST_NEW, POINT: POINT_OLD})
    assert p0 == lower_y_point(False), f"one_division: the first wave stands at {p0}, not {lower_y_point(False)}"


def test_both_keys_ship_at_their_old_arms():
    spawner = json.loads(LEDGER.read_text(encoding="utf-8"))["spawner"]
    assert spawner["LIFE_STATE_FIRST_UPDATE"]["value"] == FIRST_OLD
    assert spawner["LIFE_STATE_WAVE_POINT"]["value"] == POINT_OLD
