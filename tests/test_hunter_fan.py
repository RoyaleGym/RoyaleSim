"""The Hunter fires a fan of MultipleProjectiles pellets (combat.MULTIPLE_PROJECTILES).

WHAT THIS PINS. On client 15.535.29 a Hunter's shot (MultipleProjectiles 10; HunterProjectile: Speed 550,
ProjectileRange 6500, Damage 33) is ten pellets created on the same tick at one point, ProjectileStartRadius (1000) out
on the bearing to its target's start-of-tick centre. Each pellet is aimed at the point 6500 from the Hunter on its own
offset from that bearing, in the order 0, +7, -7, +14, -14, +21, -21, +28, -28, +35 degrees, and each starts to move 1
to 5 ticks after it is created. Every pellet is a range projectile (combat.RANGE_PROJECTILE) and deals 84 at level 11.
Measured on the client 15.535.29 Hunter scenarios, 26 volleys in 5 runs: the spawn bearing equals the bearing to the
Knight's start-of-tick centre to 0.04 degrees, the offsets hold to 0.06 degrees, every aim point is 6499-6501 out, and
every delay is 1-5 (a single 1 among about 250). The delays repeat by volley number across scenes on the same battle
seed ([4, 2, 3, 4, 3, 4, 3, 5, 3, 4] for the first), so they come from the battle's random stream; this key pins their
range, not their order. The old arms (to_target and one, shipped until the 2026-09-27 and 2026-09-26 flips) fire
one projectile a shot.

WHY THE CONTROL IS HERE. A row without MultipleProjectiles (a Musketeer) fires one projectile a shot on both arms.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `fan_single_pellet` -- the new arm fires one projectile a shot, as `one` does:
    test_a_hunter_fires_ten_pellets_in_a_fan.
  * `fan_pellets_unheld` -- every pellet moves on the tick after its creation:
    test_each_pellet_starts_moving_one_to_five_ticks_after_its_creation.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.MULTIPLE_PROJECTILES"
KEYS = ("combat.RANGE_PROJECTILE", KEY)
NEW = ("straight_to_range", "client_fan")
OLD = ("to_target", "one")
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
OFFSETS = (0, 7, -7, 14, -14, 21, -21, 28, -28, 35)
START, AIM = 1000, 6500
#: a blue Hunter and a red Knight walking at it on the red half
SCENE = ((12500, 18500), (12500, 24000))


def overrides(arms) -> dict:
    return {k: json.dumps(v) for k, v in zip(KEYS, arms, strict=True)}


def run(arms, attacker, ticks=60):
    """Per tick: the attacker and the Knight (native positions) and the attacker's projectiles (x, y, aim)."""
    (ax, ay), (kx, ky) = SCENE
    b = royalesim.Battle(["Knight", attacker], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arms))
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
        ents = [e for e in st["entities"] if e[F["tower_slot"]] < 0]
        me = next(e for e in ents if e[F["team"]] == 0)
        kn = next((e for e in ents if e[F["team"]] == 1), None)
        shots = [
            (p[P["x"]] // SUB, p[P["y"]] // SUB, p[P["aim_x"]] // SUB, p[P["aim_y"]] // SUB)
            for p in st.get("projectiles", [])
            if p[P["firer_card_id"]] == 1
        ]
        rows.append(
            (
                (me[F["x"]] // SUB, me[F["y"]] // SUB),
                None if kn is None else (kn[F["x"]] // SUB, kn[F["y"]] // SUB),
                shots,
            )
        )
        b.step([], 1)
    return rows


def bearing(a, b):
    return math.degrees(math.atan2(b[1] - a[1], b[0] - a[0]))


def first_volley(rows):
    """(creation tick, the new projectiles of that tick) for the first tick with projectiles."""
    t = next(i for i, r in enumerate(rows) if r[2])
    return t, rows[t][2]


def test_a_hunter_fires_ten_pellets_in_a_fan():
    rows = run(NEW, "Hunter")
    t, shots = first_volley(rows)
    assert len(shots) == len(OFFSETS), f"the first volley on {t} has {len(shots)} projectiles"
    me, knight = rows[t - 1][0], rows[t - 1][1]
    b0 = bearing(me, knight)
    offsets = sorted(round(((bearing(me, (s[2], s[3])) - b0 + 180) % 360) - 180) for s in shots)
    assert offsets == sorted(OFFSETS), f"aim offsets from the bearing to the Knight: {offsets}"
    radii = [round(math.dist(me, (s[2], s[3]))) for s in shots]
    assert all(abs(r - AIM) <= 5 for r in radii), f"aim points' distance from the Hunter: {radii}"
    starts = [round(math.dist(me, (s[0], s[1]))) for s in shots]
    assert all(abs(r - START) <= 5 for r in starts), f"pellets created at {starts} from the Hunter"


def test_each_pellet_starts_moving_one_to_five_ticks_after_its_creation():
    rows = run(NEW, "Hunter")
    t, shots = first_volley(rows)
    delays = []
    for s in shots:
        moved = next(
            (
                u - t
                for u in range(t + 1, t + 10)
                if not any((q[2], q[3]) == (s[2], s[3]) and (q[0], q[1]) == (s[0], s[1]) for q in rows[u][2])
            ),
            None,
        )
        delays.append(moved)
    assert len(delays) == len(OFFSETS), f"the volley has {len(delays)} projectiles, so no fan to time: {delays}"
    assert all(d is not None and 1 <= d <= 5 for d in delays), f"release delays: {delays}"
    # the client held all but one of about 250 pellets for 2 ticks or more
    assert sum(d >= 2 for d in delays) >= len(OFFSETS) - 2, f"too few pellets held 2 ticks or more: {delays}"


@pytest.mark.parametrize("arms", [NEW, OLD])
def test_a_single_projectile_row_fires_one(arms):
    rows = run(arms, "Musketeer")
    t, shots = first_volley(rows)
    assert len(shots) == 1, f"the Musketeer's first shot on {t} is {len(shots)} projectiles"


def test_old_arms_are_todays_engine():
    rows = run(OLD, "Hunter")
    t, shots = first_volley(rows)
    assert len(shots) == 1, f"old arms: the Hunter's first shot on {t} is {len(shots)} projectiles"
