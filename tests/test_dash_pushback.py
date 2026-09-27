"""The Mega Knight's jump blow pushes the troops it hits DashPushBack (combat.DASH_PUSHBACK).

WHAT THIS PINS. On client 15.535.29 (the Mega Knight's single-card scenario and its two jump scenarios) and on the
16.402 corpus (one battle), each Knight the jump blow hit slid 199 or 200, 174 or 175, 149 or 150, 124 or 125, 99 or
100, 74 or 75, 49 or 50, 24 or 25 from the next tick (4 of 4, within 2), away from the Mega Knight's position on the
blow tick: the knockback ladder of Pushback 1000 that spells and the Mega Knight's deploy blow already run. Today's
engine loads DashPushBack and never applies it, so the Knight stays in melee and keeps hitting the Mega Knight.

THE SCENE IS THE CLIENT'S. A red Knight stands hitting the blue right princess tower at (14231, 9182), where it stood
when the client's Mega Knight landed on it. A blue Mega Knight set down at (9500, 6500) walks, jumps and lands its
blow on it.

WHY THE CONTROL IS HERE. A Giant (IgnorePushback) hit by the same blow must not move under either value: two Giants
did not in the client. An implementation that pushes every victim fails it.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then
run this file: the named tests go red and the rest stay green.
  * `dash_pushback_unapplied` -- client_ladder_from_landing pushes nobody:
    test_the_jump_blow_slides_the_knight_down_the_ladder.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.DASH_PUSHBACK"
NEW_ARM, OLD_ARM = "client_ladder_from_landing", "not_read"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
KNIGHT_AT, MK_FOR_KNIGHT = (14231, 9182), (9500, 6500)
GIANT_AT, MK_FOR_GIANT = (14731, 9439), (9500, 12500)
BLOW = 537   # DashDamage 210 at level 11
LADDER = [200, 175, 150, 125, 100, 75, 50, 25]


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def scene(arm, victim, victim_at, mk_at, ticks=90):
    """Per tick: (the victim's hp loss, its move, the Mega Knight's position, the victim's position), native."""
    b = royalesim.Battle([victim, "MegaKnight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None,
            [(1, 0, victim_at[0] * SUB, victim_at[1] * SUB, -1), (0, 1, mk_at[0] * SUB, mk_at[1] * SUB, -1)])
    prev = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
    v = next(u for u, e in prev.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 1)
    m = next(u for u, e in prev.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 0)
    rows = []
    for _ in range(ticks):
        b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        if v not in now or m not in now:
            break
        V0, V, M = prev[v], now[v], now[m]
        rows.append((V0[F["hp"]] - V[F["hp"]],
                     math.hypot(V[F["x"]] - V0[F["x"]], V[F["y"]] - V0[F["y"]]) / SUB,
                     (M[F["x"]] / SUB, M[F["y"]] / SUB), (V[F["x"]] / SUB, V[F["y"]] / SUB)))
        prev = now
    return rows


def blow_tick(rows):
    hits = [i for i, r in enumerate(rows) if r[0] == BLOW]
    assert len(hits) == 1, f"the scene drifted: the victim lost {BLOW} on ticks {hits}"
    assert hits[0] + 9 < len(rows), "the scene drifted: the run ended right after the blow"
    return hits[0]


def test_the_jump_blow_slides_the_knight_down_the_ladder():
    rows = scene(NEW_ARM, "Knight", KNIGHT_AT, MK_FOR_KNIGHT)
    b = blow_tick(rows)
    steps = [round(r[1]) for r in rows[b + 1:b + 9]]
    off_ladder = [(s, lad) for s, lad in zip(steps, LADDER, strict=True) if abs(s - lad) > 2]
    assert off_ladder == [], f"the Knight's moves after the blow: {steps}, not the ladder {LADDER}"
    (mx, my), (kx, ky), (nx, ny) = rows[b][2], rows[b][3], rows[b + 1][3]
    ax, ay, sx, sy = kx - mx, ky - my, nx - kx, ny - ky
    off = math.degrees(math.atan2(ax * sy - ay * sx, ax * sx + ay * sy))
    assert abs(off) <= 1.0, f"the Knight's first move is {off:.2f} degrees off the line away from the Mega Knight"


def test_old_arm_is_todays_engine():
    rows = scene(OLD_ARM, "Knight", KNIGHT_AT, MK_FOR_KNIGHT)
    b = blow_tick(rows)
    assert rows[b + 1][1] == 0, f"{OLD_ARM}: the Knight moved {rows[b + 1][1]:.1f} on the tick after the blow"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_giant_that_ignores_pushback_is_not_moved(arm):
    rows = scene(arm, "Giant", GIANT_AT, MK_FOR_GIANT)
    b = blow_tick(rows)
    steps = [round(r[1]) for r in rows[b + 1:b + 9]]
    assert steps == [0] * 8, f"{arm}: the Giant moved after the blow: {steps}"
