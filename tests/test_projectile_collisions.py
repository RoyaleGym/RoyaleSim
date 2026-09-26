"""A straight shot reads its row's CheckCollisions, ProjectileStartExtraRadius and RandomDelay
(combat.PROJECTILE_COLLISIONS).

WHAT THIS PINS. On client 15.535.29 the Hunter's pellet (HunterProjectile: CheckCollisions, ProjectileRadius 300,
ProjectileStartExtraRadius 650, RandomDelay 200) is gone on the tick it hits: 134 pellets that hit a Knight or a Giant
in 7 recordings were each last seen the tick before the victim lost 84, while pellets that missed flew to their range.
A whole volley hit on its creation tick, no pellet of it ever seen, when the victim's centre was 1280-1295 (a Knight)
or 1555 (a Giant) from the launch point, and none did at 2009 or 1834: the creation-tick reach less the victim's
radius lies in 805-1084 (ProjectileRadius + ProjectileStartExtraRadius = 950). Each pellet first moves 1-5 ticks after
its creation: of 136 delays one is 1, and 2-5 come about equally often, the spread of 1 + ceil(U / 50) for U uniform
on 0..RandomDelay (200). This file pins the range and that all but a rare pellet wait 2 or more; the order is the
client's own random stream. Rows without CheckCollisions fly on after a hit (the Elite Archer's arrow, the Bowler's
boulder, the Firecracker's spark). The pellets ride on
combat.RANGE_PROJECTILE = straight_to_range and combat.MULTIPLE_PROJECTILES = client_fan, so every case names all
three keys.

THE KILL INSIDE A TICK. One point-blank volley met a Knight that fewer than 10 pellets kill (347 hp; one scene,
recorded from both sides). The first five pellets in creation order (offsets 0, +7, -7, +14, -14) hit it for 420 on
the creation tick. The other five (+21, -21, +28, -28, +35) were seen at the launch point on that tick and flew on to
their range. The client applies each pellet's hit before it tests the next, so the pellets after a kill pass the dead
unit. The engine buffers a tick's damage for Resolve, so the new arm reads the hits already written this tick.

WHY THE CONTROL IS HERE. An Elite Archer's arrow (no CheckCollisions) passes through the Knight it hits under both
arms of this key, which refuses "client_columns stops every straight shot".

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `check_collisions_fly_on` -- the new arm lets a CheckCollisions shot fly on after its hit, as not_read:
    test_a_pellet_that_hits_is_gone_on_that_tick.
  * `start_extra_radius_unread` -- the new arm's creation-tick test reaches ProjectileRadius alone:
    test_a_point_blank_volley_lands_on_its_creation_tick.
  * `random_delay_unread` -- the new arm reads no delay, so every pellet steps on the tick after its creation:
    test_each_pellet_waits_one_to_five_ticks.
  * `kill_inside_tick_unread` -- the new arm stops every pellet in reach on a unit killed earlier in the tick:
    test_the_pellets_after_a_kill_fly_on.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.PROJECTILE_COLLISIONS"
KEYS = ("combat.RANGE_PROJECTILE", "combat.MULTIPLE_PROJECTILES", KEY)
NEW = ("straight_to_range", "client_fan", "client_columns")
MID = ("straight_to_range", "client_fan", "not_read")
OLD = ("to_target", "one", "not_read")
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
PELLET = 84  # HunterProjectile Damage 33 at level 11
STEP = 550
#: the fan's offsets in creation order, degrees (combat.MULTIPLE_PROJECTILES = client_fan)
FAN = (0, 7, -7, 14, -14, 21, -21, 28, -28, 35)
#: a blue attacker on the red half and a red Knight walking at it, clear of every tower
AT_RANGE = ((9000, 18000), (9000, 23000))
#: the Knight 2200 from the Hunter, inside its own reach, so it stands: 1200 from the pellets' launch point
POINT_BLANK = ((9000, 18000), (9000, 20200))


def overrides(arms) -> dict:
    return {k: json.dumps(v) for k, v in zip(KEYS, arms, strict=True)}


def run(arms, attacker, scene, ticks=60, knight_hp=-1):
    """Per tick: the Knight (native position and hp, None once gone), the attacker's projectiles (x, y, aim) and the
    attacker's native position (None once gone). `knight_hp` is the Knight's starting hp (-1: full)."""
    (ax, ay), (kx, ky) = scene
    b = royalesim.Battle(["Knight", attacker], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arms))
    b.reset(
        0,
        [[1] * 8, [0] * 8],
        0,
        200,
        [10_000, 10_000],
        None,
        [(1, 0, kx * SUB, ky * SUB, knight_hp), (0, 1, ax * SUB, ay * SUB, -1)],
    )
    rows = []
    for _ in range(ticks + 1):
        st = json.loads(b.state_json())
        kn = next((e for e in st["entities"] if e[F["tower_slot"]] < 0 and e[F["team"]] == 1), None)
        me = next((e for e in st["entities"] if e[F["tower_slot"]] < 0 and e[F["team"]] == 0), None)
        shots = [
            (p[P["x"]] // SUB, p[P["y"]] // SUB, p[P["aim_x"]] // SUB, p[P["aim_y"]] // SUB)
            for p in st.get("projectiles", [])
            if p[P["firer_card_id"]] == 1
        ]
        rows.append(
            (
                None if kn is None else (kn[F["x"]] // SUB, kn[F["y"]] // SUB, kn[F["hp"]]),
                shots,
                None if me is None else (me[F["x"]] // SUB, me[F["y"]] // SUB),
            )
        )
        b.step([], 1)
    return rows


def losses(rows):
    """{tick: the Knight's hp loss on that tick}."""
    out = {}
    for t in range(1, len(rows)):
        if rows[t][0] is not None and rows[t - 1][0] is not None and rows[t][0][2] < rows[t - 1][0][2]:
            out[t] = rows[t - 1][0][2] - rows[t][0][2]
    return out


def first_volley(rows):
    """(creation tick, {aim: [(tick, x, y), ...]}) for the pellets created on the first tick with projectiles."""
    t0 = next(i for i, r in enumerate(rows) if r[1])
    tracks = {(s[2], s[3]): [] for s in rows[t0][1]}
    for t in range(t0, len(rows)):
        for s in rows[t][1]:
            if (s[2], s[3]) in tracks:
                tracks[(s[2], s[3])].append((t, s[0], s[1]))
    return t0, tracks


def test_a_pellet_that_hits_is_gone_on_that_tick():
    rows = run(NEW, "Hunter", AT_RANGE)
    t0, tracks = first_volley(rows)
    assert len(tracks) == 10, f"the first volley on {t0} has {len(tracks)} pellets"
    ends = {}  # tick a pellet is first missing -> pellets gone short of their range
    reached = 0
    for (aim_x, aim_y), tr in tracks.items():
        t_last, x, y = tr[-1]
        if t_last == len(rows) - 1:
            continue
        if math.dist((x, y), (aim_x, aim_y)) <= STEP + 5:
            reached += 1
        else:
            ends[t_last + 1] = ends.get(t_last + 1, 0) + 1
    lost = {t: v for t, v in losses(rows).items() if t > t0 and t <= max(tr[-1][0] for tr in tracks.values()) + 1}
    assert ends, f"no pellet of the volley on {t0} ended short of its range; the Knight lost {lost}"
    assert reached, "no pellet of the volley flew to its range, so the fan never missed"
    # every loss is 84 a pellet gone that tick, and every pellet gone short of its range is such a loss
    assert lost == {t: PELLET * n for t, n in ends.items()}, f"Knight losses {lost}, pellets gone short {ends}"


def test_a_point_blank_volley_lands_on_its_creation_tick():
    rows = run(NEW, "Hunter", POINT_BLANK, ticks=40)
    lost = losses(rows)
    assert lost, "the Hunter never hit the Knight"
    t, first = min(lost.items())
    seen = [i for i, r in enumerate(rows[: t + 1]) if r[1]]
    assert not seen, f"Hunter projectiles seen on {seen}, up to the Knight's first loss on {t}"
    assert first == 10 * PELLET, f"the Knight's first loss is {first} on {t}, 10 x {PELLET} expected"


@pytest.mark.parametrize("hp", [347, 336, 100])
def test_the_pellets_after_a_kill_fly_on(hp):
    """A point-blank volley on a Knight that fewer than 10 pellets kill: the first ceil(hp / 84) in creation order hit
    it on the creation tick and the rest fly on. 347 is the Knight measured on client 15.535.29 (5 hit it, 5 flew on).
    336 is 4 pellets exactly: the fourth takes it to 0, which is death, so 6 fly on. 100 leaves 8."""
    rows = run(NEW, "Hunter", POINT_BLANK, ticks=40, knight_hp=hp)
    died = next((t for t, r in enumerate(rows) if r[0] is None), None)
    assert died is not None, f"the Knight with {hp} hp outlived the window"
    before = rows[died - 1][0]
    assert before[2] == hp, f"the scenario drifted: the Knight had {before[2]} hp on {died - 1}, not {hp}"
    seen = [t for t, r in enumerate(rows[:died]) if r[1]]
    assert not seen, f"the scenario drifted: Hunter projectiles seen on {seen}, before the Knight died on {died}"
    hits = -(-hp // PELLET)
    # the fan turns about the bearing from the Hunter's start-of-tick centre to the Knight's
    ax, ay = rows[died - 1][2]
    bearing = math.degrees(math.atan2(before[1] - ay, before[0] - ax))
    flown = rows[died][1]
    offsets = sorted(
        round(abs((math.degrees(math.atan2(s[3] - ay, s[2] - ax)) - bearing + 180) % 360 - 180)) for s in flown
    )
    want = sorted(abs(d) for d in FAN[hits:])
    assert offsets == want, (
        f"the Knight ({hp} hp) died on {died} with {len(flown)} pellets in flight at offsets {offsets}; "
        f"{10 - hits} expected at {want}: the first {hits} in creation order hit it and the rest fly on"
    )
    aims = {(s[2], s[3]) for s in flown}
    later = [s for s in rows[died + 6][1] if (s[2], s[3]) in aims]
    assert len(later) == len(aims), f"{len(aims) - len(later)} of the pellets that passed the kill were gone 6 ticks on"


def test_each_pellet_waits_one_to_five_ticks():
    rows = run(NEW, "Hunter", AT_RANGE)
    t0, tracks = first_volley(rows)
    assert len(tracks) == 10, f"the first volley on {t0} has {len(tracks)} pellets"
    delays = []
    for tr in tracks.values():
        start = tr[0][1:]
        moved = next((t for t, x, y in tr if (x, y) != start), tr[-1][0] + 1)  # gone before moving: hit on release
        delays.append(moved - t0)
    assert all(1 <= d <= 5 for d in delays), f"release delays {delays}"
    # U = 0 is 1 draw in 200: all but a rare pellet wait 2 ticks or more
    assert sum(d >= 2 for d in delays) >= 8, f"too few pellets held 2 ticks or more: {delays}"


@pytest.mark.parametrize("arms", [NEW, MID])
def test_a_row_without_check_collisions_flies_on(arms):
    rows = run(arms, "EliteArcher", AT_RANGE, ticks=40)
    lost = losses(rows)
    assert lost, "the Elite Archer never hit the Knight"
    t = min(lost)
    before = rows[t - 1][1]
    assert before, f"no arrow in flight on {t - 1}, the tick before the Knight's first loss"
    aims = {(s[2], s[3]) for s in before}
    after = [s for s in rows[t][1] if (s[2], s[3]) in aims]
    assert after, f"the arrow that hit the Knight on {t} is gone on that tick: it did not fly on"


def test_old_arm_pellets_fly_on():
    """combat.PROJECTILE_COLLISIONS = not_read on the range and fan arms: a pellet that hits flies on."""
    rows = run(MID, "Hunter", AT_RANGE)
    t0, tracks = first_volley(rows)
    lost = losses(rows)
    t = min(u for u in lost if u > t0)
    alive = [tr for tr in tracks.values() if tr[-1][0] >= t]
    assert len(alive) == len(tracks), f"{len(tracks) - len(alive)} pellet(s) gone on the Knight's first loss ({t})"


def test_old_arms_are_todays_engine():
    rows = run(OLD, "Hunter", AT_RANGE)
    t0, tracks = first_volley(rows)
    assert len(tracks) == 1, f"old arms: the Hunter's first shot on {t0} is {len(tracks)} projectiles"
