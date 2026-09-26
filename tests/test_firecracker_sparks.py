"""A troop's shot releases its row's SpawnProjectile where it lands: the Firecracker's sparks
(combat.SPAWN_PROJECTILE).

WHAT THIS PINS. On client 15.535.29 (the Firecracker's catalogue scenario, both shots) the rocket
(FirecrackerProjectile: Speed 500, no Damage) is aimed at its target's centre on the tick before the launch and keeps
that aim while the target walks. On the tick its step reaches the aim point it is gone and 5 sparks
(FirecrackerExplosion: SpawnCount 5, Speed 550, ProjectileRange 5000, ProjectileRadius 400, Damage 25) appear at
that point, 0 off it. Each is aimed 5000 out at -32, -16, 0, +16, +32 degrees from the rocket's flight line (to 0.05
degrees), moves 549-551 a tick from the next tick and is last seen 4942-4950 out. The Knight, 493 and 209 from the
landing point, lost 5 x 64 = 320 (Damage 25 at level 11) on the landing tick; every spark then flew on to its range
and none hit the Knight again, although all 5 were 786-815 from it on the next tick. Today's engine flies the rocket
after the Knight, releases nothing and deals nothing.

Unmeasured, and not pinned here: whether a spark hits a unit it reaches later on its path.

WHY THE CONTROL IS HERE. A Musketeer's bullet (no SpawnProjectile) lands on the Knight and releases nothing under both
arms, which refuses "every shot releases sparks".

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `sparks_unspawned` -- the new arm's shot lands and releases nothing, as not_read:
    test_the_shot_lands_where_the_knight_stood_and_releases_five_sparks,
    test_each_spark_steps_550_and_ends_within_5000,
    test_every_spark_hits_the_knight_once_on_the_landing_tick_and_flies_on.
  * `carrier_follows_target` -- the new arm's carrier follows its target, as a homing shot does:
    test_the_shot_lands_where_the_knight_stood_and_releases_five_sparks.
"""

from __future__ import annotations

import json
import math
from itertools import pairwise

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.SPAWN_PROJECTILE"
NEW, OLD = "client_spark_fan", "not_read"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
OFFSETS = (-32, -16, 0, 16, 32)
RANGE, STEP = 5000, 550
SPARK = 64  # FirecrackerExplosion Damage 25 at level 11
#: a blue attacker on the red half and a red Knight walking at it, clear of every tower (the sparks end short of the
#: red king tower)
SCENE = ((9000, 18000), (9000, 22000))


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def run(arm, attacker, ticks=45):
    """Per tick: the Knight (native position and hp) and the attacker's projectiles (x, y, aim)."""
    (ax, ay), (kx, ky) = SCENE
    b = royalesim.Battle(["Knight", attacker], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(
        0,
        [[1] * 8, [0] * 8],
        0,
        200,
        [10_000, 10_000],
        None,
        [(1, 0, kx * SUB, ky * SUB, -1), (0, 1, ax * SUB, ay * SUB, -1)],
    )
    rows = []
    for _ in range(ticks + 1):
        st = json.loads(b.state_json())
        kn = next((e for e in st["entities"] if e[F["tower_slot"]] < 0 and e[F["team"]] == 1), None)
        shots = [
            (p[P["x"]] // SUB, p[P["y"]] // SUB, p[P["aim_x"]] // SUB, p[P["aim_y"]] // SUB)
            for p in st.get("projectiles", [])
            if p[P["firer_card_id"]] == 1
        ]
        rows.append((None if kn is None else (kn[F["x"]] // SUB, kn[F["y"]] // SUB, kn[F["hp"]]), shots))
        b.step([], 1)
    return rows


def bearing(a, b):
    return math.degrees(math.atan2(b[1] - a[1], b[0] - a[0]))


def landing(rows):
    """(the rocket's creation tick, its launch point, its aim on every tick of its flight, the landing tick, the sparks
    created on the landing tick as {aim: [(tick, x, y), ...]})."""
    t0 = next(i for i, r in enumerate(rows) if r[1])
    assert len(rows[t0][1]) == 1, f"the first shot on {t0} is {len(rows[t0][1])} projectiles"
    x, y, ax, ay = rows[t0][1][0]
    aims = [(ax, ay)]
    t = t0 + 1
    while t < len(rows) and len(rows[t][1]) == 1 and rows[t][1][0][:2] != (x, y):
        x, y = rows[t][1][0][:2]
        aims.append(rows[t][1][0][2:])
        t += 1
    sparks = {(s[2], s[3]): [] for s in rows[t][1]} if t < len(rows) else {}
    for u in range(t, len(rows)):
        for s in rows[u][1]:
            if (s[2], s[3]) in sparks:
                sparks[(s[2], s[3])].append((u, s[0], s[1]))
    return t0, rows[t0][1][0][:2], aims, t, sparks


def test_the_shot_lands_where_the_knight_stood_and_releases_five_sparks():
    rows = run(NEW, "Firecracker")
    t0, launch, aims, land, sparks = landing(rows)
    k0 = rows[t0 - 1][0][:2]
    assert all(math.dist(a, k0) <= 1 for a in aims), (
        f"the rocket's aim on each tick {aims} against the Knight's centre {k0} on the tick before the launch"
    )
    point = aims[0]
    assert len(sparks) == len(OFFSETS), f"{len(sparks)} projectiles appeared on the landing tick {land}"
    starts = [math.dist(tr[0][1:], point) for tr in sparks.values()]
    assert all(d <= 1 for d in starts), f"sparks created {starts} from the landing point"
    out = [round(math.dist(a, point)) for a in sparks]
    assert all(abs(d - RANGE) <= 5 for d in out), f"sparks aimed {out} from the landing point"
    line = bearing(launch, point)
    offs = sorted(((bearing(point, a) - line + 180) % 360) - 180 for a in sparks)
    assert all(abs(o - w) <= 0.5 for o, w in zip(offs, OFFSETS, strict=True)), f"offsets from the flight line {offs}"


def test_each_spark_steps_550_and_ends_within_5000():
    rows = run(NEW, "Firecracker")
    _, _, aims, land, sparks = landing(rows)
    assert len(sparks) == len(OFFSETS), f"{len(sparks)} projectiles appeared on the landing tick {land}"
    point = aims[0]
    for aim, tr in sparks.items():
        assert tr[1][0] == land + 1, f"the spark to {aim} was next seen on {tr[1][0]}"
        assert tr[1][1:] != tr[0][1:], f"the spark to {aim} did not move on {tr[1][0]}"
        steps = [math.dist(a[1:], b[1:]) for a, b in pairwise(tr)]
        assert all(abs(s - STEP) <= 6 for s in steps), f"the spark to {aim} stepped {steps}"
        last = math.dist(tr[-1][1:], point)
        assert tr[-1][0] < len(rows) - 1, f"the spark to {aim} is still in flight at the end"
        assert RANGE - STEP < last < RANGE, f"the spark to {aim} was last seen {last:.0f} from the landing point"


def test_every_spark_hits_the_knight_once_on_the_landing_tick_and_flies_on():
    rows = run(NEW, "Firecracker")
    t0, _, _, land, sparks = landing(rows)
    assert len(sparks) == len(OFFSETS), f"{len(sparks)} projectiles appeared on the landing tick {land}"
    end = max(tr[-1][0] for tr in sparks.values()) + 1
    lost = {t: rows[t - 1][0][2] - rows[t][0][2] for t in range(t0, end + 1) if rows[t][0][2] != rows[t - 1][0][2]}
    assert lost == {land: len(OFFSETS) * SPARK}, f"the Knight's losses {lost}; the sparks landed on {land}"
    assert all(tr[-1][0] > land for tr in sparks.values()), "a spark that hit the Knight was gone on the landing tick"


@pytest.mark.parametrize("arm", [NEW, OLD])
def test_a_shot_without_a_spawn_projectile_releases_nothing(arm):
    rows = run(arm, "Musketeer")
    lost = {t: rows[t - 1][0][2] - rows[t][0][2] for t in range(1, len(rows)) if rows[t][0][2] != rows[t - 1][0][2]}
    assert lost, "the Musketeer never hit the Knight"
    t = min(lost)
    most = max(len(r[1]) for r in rows[: t + 3])
    assert most == 1, f"the Musketeer had up to {most} projectiles out"
    assert not rows[t][1], f"the Musketeer's bullet is still out on {t}, the tick it hit"


def test_old_arm_the_firecracker_releases_nothing():
    rows = run(OLD, "Firecracker", ticks=40)
    most = max(len(r[1]) for r in rows)
    lost = {t: rows[t - 1][0][2] - rows[t][0][2] for t in range(1, len(rows)) if rows[t][0][2] != rows[t - 1][0][2]}
    assert most == 1, f"old arm: up to {most} Firecracker projectiles out"
    assert not lost, f"old arm: the Knight lost {lost} to the Firecracker"
