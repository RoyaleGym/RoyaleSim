"""A troop projectile with a ProjectileRange flies straight to its range and hits what it passes
(combat.RANGE_PROJECTILE).

WHAT THIS PINS. On client 15.535.29 the projectile of a troop whose projectile row has a ProjectileRange (the Bowler's
boulder, the Elite Archer's arrow, the Hunter's pellets) leaves ProjectileStartRadius from the attacker on the line to
its target's start-of-tick centre, and moves its Speed a tick along that line, whatever the target does. It hits each
enemy whose centre comes within its ProjectileRadius plus that enemy's radius once, and flies on: on the tick it is
created, against the launch point and start-of-tick positions; after that, against its moved position and post-move
positions.
It is last seen at the last point within ProjectileRange of the attacker. A boulder's Pushback moves the victim on the
pushback-1000 ladder, radially from the boulder's centre. Measured on the client 15.535.29 catalogue scenarios:
- Bowler: 3 boulders launched 999 out on the line to the Knight, 169.5-169.8 a tick, straight (direction wobble at most
  0.007), last seen 6933-6939 from the Bowler; 3 hits for 289, each first push step 199 radial from the boulder: two on
  the first tick within 1500 post-move (1456 after 1684, 1347 after 1576; (0.778, 0.628) and (0.805, 0.594) against
  (0.781, 0.625) and (0.803, 0.595)), one on the boulder's creation tick, 1428 from its launch point at the start of
  the tick ((0.819, 0.573) against (0.819, 0.573));
- Elite Archer: 5 arrows launched 799-800 out, 998.9-999.8 a tick, each hitting the Knight for 135 and flying on to
  10789-10796;
- Hunter: 13 pellets launched 999 out, about 549 a tick, straight, last seen up to 6456 from the Hunter;
- Executioner (a pingpong row, PingpongVisualTime 1500): the axe is 600 + 6400 sin(pi t / 30) from him t ticks after
  the throw, t = 0..30, out and back (within 5), and hits a target once a leg when the target's post-move centre is
  within ProjectileRadius + r (1500) of the axe's PREVIOUS-frame position (4 hits; 4 misses, the tightest 1519).
  He waits for it: his attack progress stands at the throw's value from the throw tick T through T + 31 and steps
  again on T + 32, so his throws are 49 ticks apart (247, 296, 345), not the 18 of HitSpeed 900. While he waits he
  neither walks nor takes a new target: when the Knight died on 359 his target read none until 377 = 345 + 32, and he
  walked on 377. The 16.402 corpus shows the same (one battle, two Executioners, 11 throws: 1403 and 1452, and 3477,
  3526, 3575 and 3624, are 49 apart; a target killed on 1366 while the axe of 1338 was out read none until 1370 =
  1338 + 32, and he walked on 1370).
Today's engine aims these shots at the target and ends them on it: the boulder curves after the Knight, lands late,
and pushes nothing; the Elite Archer's arrow stops on the first Knight.

WHY THE CONTROL IS HERE. A projectile without a ProjectileRange (a Musketeer's bullet) ends on its target on both
arms, so an implementation that sends every shot on to a range fails.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `range_projectile_unread` -- the new arm aims a range row's shot at its target, as to_target does:
    test_a_boulder_rolls_straight_to_its_range_and_pushes_radially, test_an_arrow_flies_on_through_its_first_victim,
    test_an_axe_flies_out_and_back_on_its_pingpong_path_and_hits_once_per_leg.
  * `range_shot_ends_on_first_hit` -- a straight shot ends on the first unit it hits, as a homing shot does:
    test_an_arrow_flies_on_through_its_first_victim.
  * `range_shot_unpushed` -- a straight shot's Pushback moves nothing:
    test_a_boulder_rolls_straight_to_its_range_and_pushes_radially.
  * `pingpong_thrower_unheld` -- the thrower attacks on every HitSpeed while its axe is out, and retargets and walks
    off as soon as its target dies: test_the_executioner_throws_again_only_49_ticks_after_a_throw[straight_to_range],
    test_the_executioner_stands_with_no_target_until_his_axe_is_back, and (a second axe's hit inside the first one's
    30 ticks) test_an_axe_flies_out_and_back_on_its_pingpong_path_and_hits_once_per_leg.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.RANGE_PROJECTILE"
NEW_ARM, OLD_ARM = "straight_to_range", "to_target"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
#: the Bowler row: ProjectileStartRadius 1000, Speed 170, ProjectileRange 7000, ProjectileRadius 1000, Pushback 1000
BOWLER_SCENE = ((9500, 11500), [(9500, 18499)])
BOULDER_START, BOULDER_SPEED, BOULDER_RANGE, BOULDER_HIT_REACH = 1000, 170, 7000, 1000 + 500
BOULDER_HIT = 289
LADDER = (199, 174, 149, 124, 99, 74, 49, 24)
#: the Elite Archer on the red half, two Knights in a column walking at it
ARCHER_SCENE = ((12500, 18500), [(12500, 23500), (12500, 24700)])
ARROW_HIT, ARROW_RANGE, ARROW_SPEED = 135, 11000, 1000
MUSKETEER_SCENE = ((12500, 18500), [(12500, 23500)])
#: the Executioner (AxeManProjectile: ProjectileStartRadius 600, ProjectileRange 7000, PingpongVisualTime 1500 ms,
#: ProjectileRadius 1000) throwing at a Knight walking at it
AXE_SCENE = ((12500, 18500), [(12500, 23500)])
AXE_START, AXE_RANGE, AXE_PERIOD, AXE_REACH, AXE_HIT = 600, 7000, 30, 1000 + 500, 179
#: he waits for his axe: the throw tick T and the 31 after it (client 15.535.29: 247-278, 296-327, 345-376; the 16.402
#: corpus the same), so a throw comes 49 ticks after the one before (247, 296, 345; 1403, 1452), not HitSpeed 900 / 50
#: = 18. Today's engine (the old arm, a homing axe) throws every 18.
AXE_WAIT, AXE_THROW_GAP, HIT_SPEED_TICKS = 32, 49, 18
#: a Knight the axe's way out kills (179 > 100), so the Executioner loses his target while the axe is out
WEAK_KNIGHT_HP = 100


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def native(x, y):
    return x // SUB, y // SUB


def run(arm, attacker, scene, ticks=70):
    """Per tick: the attacker (native), the Knights' rows by uid, and the attacker's projectiles (native x, y)."""
    at, knights = scene
    cards = ["Knight", attacker]
    b = royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    spawns = [(1, 0, x * SUB, y * SUB, -1) for x, y in knights] + [(0, 1, at[0] * SUB, at[1] * SUB, -1)]
    b.reset(0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, spawns)
    rows = []
    for _ in range(ticks + 1):
        st = json.loads(b.state_json())
        ents = {e[F["uid"]]: e for e in st["entities"] if e[F["tower_slot"]] < 0}
        me = next(e for e in ents.values() if e[F["team"]] == 0)
        ks = {u: e for u, e in ents.items() if e[F["team"]] == 1}
        shots = [native(p[P["x"]], p[P["y"]]) for p in st.get("projectiles", []) if p[P["firer_card_id"]] == 1]
        rows.append((native(me[F["x"]], me[F["y"]]), ks, shots))
        b.step([], 1)
    return rows


def dist(a, b):
    return ((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2) ** 0.5


def first_track(rows):
    """The first shot's positions tick by tick, followed by continuity from its first appearance."""
    start = next(t for t, (_, _, shots) in enumerate(rows) if shots)
    track, pos = [(start, rows[start][2][0])], rows[start][2][0]
    for t in range(start + 1, len(rows)):
        near = [s for s in rows[t][2] if dist(s, pos) <= 1100]
        if not near:
            break
        pos = min(near, key=lambda s: dist(s, pos))
        track.append((t, pos))
    return track


def test_a_boulder_rolls_straight_to_its_range_and_pushes_radially():
    rows = run(NEW_ARM, "Bowler", BOWLER_SCENE, ticks=90)
    track = first_track(rows)
    t0 = track[0][0]
    me, ks, _ = rows[t0 - 1]
    ((ku, k0),) = ks.items()
    kpos = native(k0[F["x"]], k0[F["y"]])
    d = dist(kpos, me)
    launch = (me[0] + BOULDER_START * (kpos[0] - me[0]) / d, me[1] + BOULDER_START * (kpos[1] - me[1]) / d)
    steps = [dist(track[i][1], track[i - 1][1]) for i in range(1, len(track))]
    slow_or_fast = [round(s) for s in steps if not BOULDER_SPEED - 6 <= s <= BOULDER_SPEED + 2]
    assert slow_or_fast == [], f"boulder steps off its Speed: {slow_or_fast}"
    off_line = [
        round(abs((p[0] - launch[0]) * (kpos[1] - me[1]) - (p[1] - launch[1]) * (kpos[0] - me[0])) / d)
        for _, p in track
    ]
    assert max(off_line) <= 30, f"the boulder left its launch line (off by {max(off_line)})"
    assert BOULDER_RANGE - BOULDER_SPEED < dist(track[-1][1], me) <= BOULDER_RANGE, (
        f"last seen {dist(track[-1][1], me):.0f} from the Bowler"
    )

    def knight_at(t):
        return native(rows[t][1][ku][F["x"]], rows[t][1][ku][F["y"]])

    # the creation tick tests the launch point against the start-of-tick position, later ticks the post-move one
    if dist(knight_at(t0 - 1), launch) <= BOULDER_HIT_REACH:
        hit = t0
    else:
        hit = next(t for t, p in track if t > t0 and dist(knight_at(t), p) <= BOULDER_HIT_REACH)
    loss = rows[hit - 1][1][ku][F["hp"]] - rows[hit][1][ku][F["hp"]]
    assert loss == BOULDER_HIT, f"the Knight lost {loss} on {hit}, the first tick within {BOULDER_HIT_REACH}"
    first = hit - 1 if hit == t0 else hit
    kp = [knight_at(t) for t in range(first, first + len(LADDER) + 1)]
    pushes = [round(dist(kp[i], kp[i - 1])) for i in range(1, len(kp))]
    assert all(abs(a - b) <= 3 for a, b in zip(pushes, LADDER, strict=True)), f"push steps {pushes}"
    boulder = launch if hit == t0 else dict(track)[hit]
    here = knight_at(hit - 1) if hit == t0 else kp[0]
    radial = ((here[0] - boulder[0]) / dist(here, boulder), (here[1] - boulder[1]) / dist(here, boulder))
    step1 = ((kp[1][0] - kp[0][0]) / pushes[0], (kp[1][1] - kp[0][1]) / pushes[0])
    assert max(abs(a - b) for a, b in zip(step1, radial, strict=True)) <= 0.03, f"push {step1} against radial {radial}"


def test_an_arrow_flies_on_through_its_first_victim():
    rows = run(NEW_ARM, "EliteArcher", ARCHER_SCENE, ticks=40)
    track = first_track(rows)
    lost = {}
    for t in range(track[0][0], track[-1][0] + 2):
        for u, e in rows[t][1].items():
            if u in rows[t - 1][1] and rows[t - 1][1][u][F["hp"]] - e[F["hp"]] == ARROW_HIT:
                lost.setdefault(u, t)
    assert len(lost) == 2, f"the first arrow hit {len(lost)} Knight(s): {lost}"
    assert track[-1][0] > max(lost.values()), "the arrow was not seen after its last victim"
    me = rows[track[0][0] - 1][0]
    assert ARROW_RANGE - ARROW_SPEED < dist(track[-1][1], me) <= ARROW_RANGE, (
        f"last seen {dist(track[-1][1], me):.0f} from the Elite Archer"
    )


# The Executioner's HitSpeed is 900 ms (18 ticks) and an axe flies for 30. On the client the next throw waits for the
# axe to come back (combat.rs `throwers_out`, read in state.rs `phase_attack_for`), so no second axe hits the Knight
# inside this window. Without that hold a second axe hits on its way out and the two-hit count below fails (plant
# pingpong_thrower_unheld).
def test_an_axe_flies_out_and_back_on_its_pingpong_path_and_hits_once_per_leg():
    rows = run(NEW_ARM, "AxeMan", AXE_SCENE, ticks=60)
    track = first_track(rows)
    t0 = track[0][0]
    me = rows[t0 - 1][0]
    path = dict(track)
    want = {t: AXE_START + (AXE_RANGE - AXE_START) * math.sin(math.pi * t / AXE_PERIOD) for t in range(AXE_PERIOD + 1)}
    seen = {t: dist(path[t0 + t], me) for t in want if t0 + t in path}
    off = [
        (t, seen.get(t) and round(seen[t]), round(w))
        for t, w in want.items()
        if t != AXE_PERIOD // 2 and (t not in seen or abs(seen[t] - w) > 5)
    ]
    assert off == [], f"(t, distance from the Executioner, 600 + 6400 sin(pi t/30)) off by more than 5: {off[:6]}"
    ((ku, _),) = rows[t0][1].items()

    def knight_at(t):
        return native(rows[t][1][ku][F["x"]], rows[t][1][ku][F["y"]])

    losses = {t: rows[t - 1][1][ku][F["hp"]] - rows[t][1][ku][F["hp"]] for t in range(t0 + 1, t0 + AXE_PERIOD + 1)}
    hits = [t for t, loss in losses.items() if loss == AXE_HIT]
    assert len(hits) == 2, f"hits of {AXE_HIT} on {hits}, not two"
    assert hits[0] <= t0 + AXE_PERIOD // 2 < hits[1], f"hits on {hits}: not one on each leg (apex {t0 + 15})"
    for h in hits:
        near_now = dist(knight_at(h), path[h - 1])
        near_before = dist(knight_at(h - 1), path[h - 2]) if h - 2 in path else None
        assert near_now <= AXE_REACH, f"the hit on {h} came {near_now:.0f} from the axe's previous position"
        assert near_before is None or near_before > AXE_REACH, (
            f"on {h - 1} the Knight was {near_before:.0f} from the axe's previous position, within reach, and not hit"
        )


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_shot_without_a_range_ends_on_its_target(arm):
    rows = run(arm, "Musketeer", MUSKETEER_SCENE, ticks=40)
    track = first_track(rows)
    ((ku, _),) = rows[track[0][0]][1].items()
    end = track[-1][0]
    loss = rows[end][1][ku][F["hp"]] - rows[end + 1][1][ku][F["hp"]]
    assert loss > 0, f"the bullet vanished on {end + 1} without damaging its target"


def test_old_arm_is_todays_engine():
    rows = run(OLD_ARM, "EliteArcher", ARCHER_SCENE, ticks=40)
    track = first_track(rows)
    hit_now = [u for u, e in rows[track[-1][0] + 1][1].items() if rows[track[-1][0]][1][u][F["hp"]] - e[F["hp"]]]
    assert len(hit_now) == 1, f"old arm: the arrow's end damaged {hit_now}"


def run_axe(arm, knight_hp=-1, ticks=100):
    """AXE_SCENE with the Knight at `knight_hp` (-1: full). Per tick: the Executioner's (x, y) and target uid (-1:
    none), the Knight's hp (None once gone), and the positions of his axes in flight. Stops if he dies."""
    at, ((kx, ky),) = AXE_SCENE
    b = royalesim.Battle(["Knight", "AxeMan"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    spawns = [(1, 0, kx * SUB, ky * SUB, knight_hp), (0, 1, at[0] * SUB, at[1] * SUB, -1)]
    b.reset(0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, spawns)
    rows = []
    for _ in range(ticks + 1):
        st = json.loads(b.state_json())
        ents = [e for e in st["entities"] if e[F["tower_slot"]] < 0]
        me = next((e for e in ents if e[F["team"]] == 0), None)
        if me is None:
            break
        ks = [e for e in ents if e[F["team"]] == 1 and e[F["hp"]] > 0]
        axes = [native(p[P["x"]], p[P["y"]]) for p in st.get("projectiles", []) if p[P["firer_card_id"]] == 1]
        rows.append((native(me[F["x"]], me[F["y"]]), me[F["target_uid"]], ks[0][F["hp"]] if ks else None, axes))
        b.step([], 1)
    return rows


def throw_ticks(rows):
    """The ticks a new axe of his appears: the count of his axes in flight rises."""
    return [t for t in range(1, len(rows)) if len(rows[t][3]) > len(rows[t - 1][3])]


@pytest.mark.parametrize(("arm", "gap"), [(NEW_ARM, AXE_THROW_GAP), (OLD_ARM, HIT_SPEED_TICKS)])
def test_the_executioner_throws_again_only_49_ticks_after_a_throw(arm, gap):
    rows = run_axe(arm)
    throws = throw_ticks(rows)
    assert len(throws) >= 2, f"{arm}: throws on {throws} in {len(rows) - 1} ticks"
    assert throws[1] - throws[0] == gap, f"{arm}: throws on {throws[:3]}, {throws[1] - throws[0]} apart, not {gap}"
    if arm == NEW_ARM:
        most = max(len(rows[t][3]) for t in range(throws[0], throws[1] + 1))
        assert most == 1, f"{most} of his axes in flight at once between the throws on {throws[:2]}"
        back = [t for t in range(throws[0], throws[1]) if not rows[t][3]]
        assert back, f"his first axe was still out on {throws[1]}"
        assert back[0] == throws[0] + AXE_WAIT - 1, f"his first axe is gone from {back[0]}, not T + 31"


def test_the_executioner_stands_with_no_target_until_his_axe_is_back():
    rows = run_axe(NEW_ARM, knight_hp=WEAK_KNIGHT_HP, ticks=90)
    throws = throw_ticks(rows)
    assert throws, "no throw"
    t0 = throws[0]
    assert len(rows) > t0 + AXE_WAIT, f"the run ended on {len(rows) - 1}"
    dead = next((t for t in range(t0, t0 + AXE_WAIT) if rows[t][2] is None), None)
    assert dead is not None, f"the Knight ({WEAK_KNIGHT_HP} hp) outlived the axe thrown on {t0}"
    assert dead <= t0 + AXE_PERIOD // 2, f"the Knight died on {dead}, not on the way out of the axe thrown on {t0}"
    me = rows[t0][0]
    moved = [t for t in range(t0, t0 + AXE_WAIT) if rows[t][0] != me]
    assert moved == [], f"he moved on {moved[:4]} while his axe of {t0} was out (back on {t0 + AXE_WAIT})"
    took = {t: rows[t][1] for t in range(dead, t0 + AXE_WAIT) if rows[t][1] != -1}
    assert took == {}, f"he had a target after the Knight died on {dead} and before his axe was back: {took}"
    assert rows[t0 + AXE_WAIT][0] != me, f"he did not walk on {t0 + AXE_WAIT} (T + 32), the tick his axe was gone"
