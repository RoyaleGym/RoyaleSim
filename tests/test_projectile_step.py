"""A flying projectile steps by a truncated integer vector in native units (combat.PROJECTILE_STEP).

WHAT THIS PINS. On client 15.535.29 every projectile in flight (a troop's or a building's shot, a crown tower's arrow, a
spell such as the Rocket) moves each tick by

    step = trunc0(v * Speed / isqrt(v.v)),   v = aim - position,   in native units (1 tile = 1000),

each component truncated toward zero, the direction recomputed every tick, nothing carried over. It lands on the tick
whose v has isqrt(v.v) <= Speed. A troop's or a tower's shot is born ProjectileStartRadius from the attacker's centre
by the same arithmetic, trunc0(v * R / isqrt(v.v)), and takes its first step on the next tick; a spell takes its
first step on the tick it is cast. Measured over the 775 client 15.535.29 scenario runs: 69,508 of 69,775 moving
steps are exact (the rest are in groups that mix two speeds under one card id), 69,774 of 69,775 steps of a
projectile still flying on the next frame had isqrt(v.v) > Speed, and a troop's or tower's shot sat at the start point
exactly on its first frame in 7,421 of 7,421 launches whose group has one start radius. Truncation makes a projectile
a little slower than Speed; a slow one lands a tick later than exact stepping: the client's Rocket from the blue king
tower to (14500, 18500) is on the board for 47 frames and lands on the 48th, where exact steps of 350 land on the 47th.

WHICH ARM. fraction_carry is today's engine (exact steps of Speed along the line, the remainder carried in 1/FRAC
subtiles). client_native_truncated is the proposed arm. The tests pin each arm BY NAME through the battle's
calibration.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `projectile_step_carries_fraction` -- the new arm steps exactly Speed and carries the remainder:
    test_the_rocket_steps_and_lands_as_the_client_does.
  * `projectile_start_in_subtiles` -- the new arm's start point is truncated in subtiles:
    test_a_troop_shot_and_a_tower_arrow_start_and_step_as_the_client_does.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.PROJECTILE_STEP"
NEW_ARM, OLD_ARM = "client_native_truncated", "fraction_carry"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
SP = {name: i for i, name in enumerate(royalesim.SPELL_FIELDS)}
BLUE_KING = (9000, 3000)
RED_RIGHT_PRINCESS = (14500, 25500)
ROCKET_TAP = (14500, 18500)
ROCKET_SPEED, MUSKETEER_SHOT_SPEED, ARROW_SPEED = 350, 1000, 600
MUSKETEER_START_RADIUS, TOWER_START_RADIUS = 450, 300
#: a blue Musketeer that stands and shoots the red right princess tower, which shoots back
MUSKETEER_AT = (11500, 20500)
#: the client 15.535.29 Rocket scenario: blue Rocket tapped at (14500, 18500), 47 frames on the board
CLIENT_ROCKET_FIRST, CLIENT_ROCKET_LAST, CLIENT_ROCKET_FRAMES = (9117, 3329), (14488, 18467), 47


def overrides(arm) -> dict:
    """The battle's calibration pinning `arm` BY NAME."""
    return {KEY: json.dumps(arm)}


def trunc0(n: int, d: int) -> int:
    q = abs(n) // abs(d)
    return q if (n >= 0) == (d > 0) else -q


def toward(p, aim, amount):
    """p moved `amount` toward `aim` by the client's arithmetic, and isqrt(v.v)."""
    vx, vy = aim[0] - p[0], aim[1] - p[1]
    n = math.isqrt(vx * vx + vy * vy)
    return (p[0] + trunc0(vx * amount, n), p[1] + trunc0(vy * amount, n)), n


def law(start, aim, speed):
    """Positions on the frames after `start` until the landing, and the number of steps to the landing."""
    track, p = [], start
    while True:
        nxt, n = toward(p, aim, speed)
        if n <= speed:
            return track, len(track) + 1
        p = nxt
        track.append(p)


def native(sub_xy):
    x, y = sub_xy
    assert x % SUB == 0, f"({x}, {y}) subtiles is not a whole native position"
    assert y % SUB == 0, f"({x}, {y}) subtiles is not a whole native position"
    return x // SUB, y // SUB


def test_the_law_reproduces_the_client_rocket():
    """No engine: the law itself against the client's recorded Rocket."""
    first, _ = toward(BLUE_KING, ROCKET_TAP, ROCKET_SPEED)
    track, steps = law(first, ROCKET_TAP, ROCKET_SPEED)
    frames = [first, *track]
    assert frames[0] == CLIENT_ROCKET_FIRST
    assert frames[-1] == CLIENT_ROCKET_LAST
    assert len(frames) == CLIENT_ROCKET_FRAMES
    assert steps == CLIENT_ROCKET_FRAMES


def rocket(arm):
    """The Rocket's position after each tick from the cast, and the tick it is gone."""
    b = royalesim.Battle(["Rocket", "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[0] * 8, [0] * 8], 0, 200, [100_000, 100_000], None, [])
    out = []
    for t in range(1, 80):
        b.step([(0, 0, ROCKET_TAP[0] * SUB, ROCKET_TAP[1] * SUB)] if t == 1 else [], 1)
        sp = json.loads(b.state_json())["spells"]
        if not sp:
            return out, t
        assert len(sp) == 1
        out.append((sp[0][SP["x"]], sp[0][SP["y"]]))
    raise AssertionError("the Rocket never landed")


def test_the_rocket_steps_and_lands_as_the_client_does():
    track, gone = rocket(NEW_ARM)
    got = [native(p) for p in track]
    first, _ = toward(BLUE_KING, ROCKET_TAP, ROCKET_SPEED)
    want, _ = law(first, ROCKET_TAP, ROCKET_SPEED)
    assert got == [first, *want], "the Rocket's positions depart from the client's step"
    assert (len(got), gone) == (CLIENT_ROCKET_FRAMES, CLIENT_ROCKET_FRAMES + 1), (
        f"the Rocket was on the board for {len(got)} ticks and gone on {gone}; the client: 47 and the 48th"
    )


def test_the_fraction_carrying_arm_lands_the_rocket_a_tick_early():
    track, gone = rocket(OLD_ARM)
    assert (len(track), gone) == (CLIENT_ROCKET_FRAMES - 1, CLIENT_ROCKET_FRAMES), (
        f"fraction_carry: on the board for {len(track)} ticks, gone on {gone}"
    )


def duel(arm):
    """(states after each tick, the Musketeer's uid, the red right princess tower's uid)."""
    b = royalesim.Battle(["Musketeer", "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(
        0,
        [[0] * 8, [0] * 8],
        0,
        200,
        [10_000, 10_000],
        None,
        [(0, 0, MUSKETEER_AT[0] * SUB, MUSKETEER_AT[1] * SUB, -1)],
    )
    states = []
    for _ in range(45):
        b.step([], 1)
        s = json.loads(b.state_json())
        states.append(({e[F["uid"]]: e for e in s["entities"]}, s["projectiles"]))
    ents = states[0][0]
    (mus,) = [u for u, e in ents.items() if e[F["tower_slot"]] < 0]
    (tower,) = [
        u
        for u, e in ents.items()
        if e[F["tower_slot"]] >= 0 and e[F["team"]] == 1 and (e[F["x"]] // SUB, e[F["y"]] // SUB) == RED_RIGHT_PRINCESS
    ]
    return states, mus, tower


def first_shot(states, shooter_team, target):
    """The first projectile of `shooter_team` at `target`: its position on each tick it is seen, and the tick it is
    gone. Projectiles carry no id here; the first one seen at the target is followed while it is the only one."""
    seen, t0 = [], None
    for t, (_, prs) in enumerate(states):
        mine = [p for p in prs if p[P["team"]] == shooter_team and p[P["target_uid"]] == target]
        if t0 is None and mine:
            t0 = t
        if t0 is not None:
            if not mine:
                return seen, t
            assert len(mine) == 1, "a second shot flew before the first landed: the scene cannot follow one"
            seen.append((mine[0][P["x"]], mine[0][P["y"]]))
    raise AssertionError("no shot, or it never landed")


@pytest.mark.parametrize("who", ["musketeer", "tower"])
def test_a_troop_shot_and_a_tower_arrow_start_and_step_as_the_client_does(who):
    states, mus, tower = duel(NEW_ARM)
    shooter, target, team = (mus, tower, 0) if who == "musketeer" else (tower, mus, 1)
    radius, speed = (
        (MUSKETEER_START_RADIUS, MUSKETEER_SHOT_SPEED) if who == "musketeer" else (TOWER_START_RADIUS, ARROW_SPEED)
    )
    seen, gone_after = first_shot(states, team, target)
    ents = states[0][0]
    src = (ents[shooter][F["x"]] // SUB, ents[shooter][F["y"]] // SUB)
    aim = (ents[target][F["x"]] // SUB, ents[target][F["y"]] // SUB)
    assert all((s[0][mus][F["x"]], s[0][mus][F["y"]]) == (ents[mus][F["x"]], ents[mus][F["y"]]) for s in states), (
        "precondition: the Musketeer moved, so the aim is not fixed"
    )
    start, _ = toward(src, aim, radius)
    want, steps = law(start, aim, speed)
    got = [native(p) for p in seen]
    assert got == [start, *want], f"the {who}'s shot departs from the client's start point or step"
    assert len(got) == steps, f"the {who}'s shot landed after {len(got) - 1} steps, the law after {steps - 1}"
    assert gone_after == len(seen)


def test_the_fraction_carrying_arm_is_not_on_the_native_grid():
    """Old arm: today's shot starts and steps off the native grid (its first frame is a fraction of a native unit)."""
    states, _mus, tower = duel(OLD_ARM)
    seen, _ = first_shot(states, 0, tower)
    assert any(x % SUB or y % SUB for x, y in seen), "fraction_carry kept every position on the native grid"
