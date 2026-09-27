"""The Sparky's charge runs from its deploy end, and each launch recoils it (combat.LOAD_FIRST_HIT and
knockback.ATTACK_PUSHBACK).

WHAT THIS PINS. Measured on client 15.535.29, in two Sparky-against-Giant scenarios (a Sparky that walks 36 ticks before
it locks, and one whose deploy ends with the Giant already in reach):
- The charge. A LoadFirstHit unit leaves its deploy with its load timer at LoadTime (3000), and the timer counts down 50
  a tick from then on, walking or not. On entering the attack its progress is LoadTime - (the timer on the tick before)
  + 100, and it launches when the progress reaches HitSpeed (4000). So the first launch comes 79 ticks after the deploy
  end in both scenarios: 43 ticks after a lock that followed a walk, 78 after a lock straight out of the deploy. Every
  other hand-played unit leaves its deploy with the timer at 0 (778 of 780; the other 2 are the two Sparkys).
- The recoil. On every launch (5 of 5) the Sparky is pushed away from its target over 9 ticks, starting on the launch
  tick: 175, 150, 125, 100, 75, 50, 25, then 0, then 25 back, a net of about 675 (the knockback ladder armed with the
  row's AttackPushBack, 750). It is out of the attack for the 8 ticks after the launch and re-enters it on launch + 9
  with progress 500, so launches are 79 ticks apart (3 of 3 gaps), not the 80 a HitSpeed of 4000 gives.
- The recoil on a second row. The Firecracker (AttackPushBack 1000, LoadTime 2350, HitSpeed 3000; its catalogue
  scenario against a Knight, 2 of 2 launches) is pushed away from its target over 10 ticks: 200, 175, ..., 25, then 0,
  then 25 back, the ladder for 1000. It is out of the attack for the 9 ticks after the launch and re-enters it on
  launch + 10 with progress 550 (2350 - 1900 + 100), so its launches are 59 ticks apart (1 of 1), not 60. The ladder
  and the re-entry follow the row, not the Sparky's numbers.
Today's engine ignores both columns: the first launch comes 19 ticks after the lock whatever came before, the Sparky
never moves when it fires, and its launches are 80 ticks apart.

WHY THE CONTROLS ARE HERE. A Musketeer (no LoadFirstHit, no AttackPushBack) played in the same spot keeps today's
behaviour on both arms: its first launch comes 13 ticks after a lock straight out of the deploy ((1000 - 300) / 50 - 1;
12 of 12 hand-played Musketeers on client 15.535.29), and it does not move over the 9 ticks from a launch (178 of 178
launches with no other unit within 2000). An implementation that starts every unit's load timer at LoadTime fails the
first; one that recoils every shooter fails the second.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `load_first_hit_from_lock` -- the load timer leaves the deploy at 0, so the first launch is timed from the lock:
    test_first_launch_after_a_walk_is_79_ticks_after_the_deploy_end,
    test_first_launch_straight_out_of_the_deploy_is_79_ticks_after_the_deploy_end.
  * `attack_pushback_unread` -- AttackPushBack is not read, so the Sparky never moves when it fires:
    test_launch_recoils_the_sparky_down_the_ladder, test_launch_recoils_the_firecracker_down_its_own_ladder.
  * `attack_pushback_keeps_cycle` -- the recoil leaves the cycle running, so launches are 88 ticks apart:
    test_recoil_makes_the_cycle_79_ticks, test_launch_recoils_the_firecracker_down_its_own_ladder.
"""

from __future__ import annotations

import itertools
import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
KEY_CHARGE = "combat.LOAD_FIRST_HIT"
KEY_RECOIL = "knockback.ATTACK_PUSHBACK"
KEYS = (KEY_CHARGE, KEY_RECOIL)
CHARGE_NEW, CHARGE_OLD = "load_time_from_deploy_end", "none"
RECOIL_NEW, RECOIL_OLD = "ladder_away_from_target", "none"
#: an arm is (charge, recoil); None leaves that key at the ledger's value
NEW_ARM, OLD_ARM = (CHARGE_NEW, RECOIL_NEW), (CHARGE_OLD, RECOIL_OLD)
CHARGE_ONLY, RECOIL_ONLY = (CHARGE_NEW, None), (None, RECOIL_NEW)

GIANT_TAP = (14500, 18500)  # red, played on the first tick
WALK_TAP = (11500, 9500)  # played on the first tick: the Sparky walks 36 ticks before it locks
DEPLOY_TAP = (11500, 10500)  # played on tick 99: the deploy ends with the Giant in reach
DEPLOY_TICK = 99
#: the first launch, counted from the deploy end, when the lock comes while the load timer is above 0
FIRST_LAUNCH = 79
CYCLE = 79
#: the recoil's step lengths from the launch tick on (the last one is back toward the target)
LADDER = (175, 150, 125, 100, 75, 50, 25, 0, 25)
MUSKETEER_FIRST = (1000 - 300) // 50 - 1
#: the Firecracker's (AttackPushBack 1000): its ladder's step lengths, and its launch-to-launch gap
FIRECRACKER_LADDER = (200, 175, 150, 125, 100, 75, 50, 25, 0, 25)
FIRECRACKER_CYCLE = 59
#: centre to centre, its Range 6000 plus its radius 500 and the Giant's 750: inside it the Firecracker can re-enter
FIRECRACKER_REACH = 6000 + 500 + 750
#: the Firecracker's rocket releases 5 sparks where it lands (combat.SPAWN_PROJECTILE). They are the card's projectiles
#: too, so the launch count must not see them. The Firecracker runs under both arms.
KEY_SPARKS = "combat.SPAWN_PROJECTILE"
SPARKS_NEW, SPARKS_OLD = "client_spark_fan", "not_read"
#: a shot starts ProjectileStartRadius (200 for the Firecracker) from its firer, and the firer recoils about 200 on the
#: launch tick. A spark starts at the landing point, on the target (about 4,000 away in these scenes).
LAUNCH_REACH = 1500


def overrides(arm, extra=None) -> dict:
    got = {key: json.dumps(value) for key, value in zip(KEYS, arm, strict=True) if value is not None}
    got.update({key: json.dumps(value) for key, value in (extra or {}).items()})
    return got


def run(card, tap, play_tick, arm, ticks=330, extra=None):
    """A blue `card` played at `tap` on `play_tick`, a red Giant at GIANT_TAP on tick 1. Returns (rows, launches):
    rows maps each tick to the blue unit's (deploy ticks left, attack phase, target is the Giant, x, y, Giant x,
    Giant y)
    in millitiles, Giant fields None once it is gone; launches are the ticks on which a projectile of the blue card
    appears within LAUNCH_REACH of the blue unit, where none was the tick before. A projectile born far from the unit
    is not a launch: the Firecracker's sparks appear where its rocket lands. `extra` sets more keys by name."""
    b = royalesim.Battle([card, "Giant"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm, extra))
    b.reset(15, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, [])
    rows, launches, before = {}, [], 0
    for t in range(1, ticks + 1):
        cmds = [(1, 0, GIANT_TAP[0] * SUB, GIANT_TAP[1] * SUB)] if t == 1 else []
        if t == play_tick:
            cmds.append((0, 0, tap[0] * SUB, tap[1] * SUB))
        b.step(cmds, 1)
        s = json.loads(b.state_json())
        units = [e for e in s["entities"] if e[F["tower_slot"]] < 0]
        me = [e for e in units if e[F["team"]] == 0]
        giant = [e for e in units if e[F["team"]] == 1]
        near = 0
        if me:
            ux, uy = me[0][F["x"]] / SUB, me[0][F["y"]] / SUB
            near = sum(
                p[P["firer_card_id"]] == 0 and math.hypot(p[P["x"]] / SUB - ux, p[P["y"]] / SUB - uy) <= LAUNCH_REACH
                for p in s["projectiles"]
            )
        if near > before:
            launches.append(t)
        before = near
        if me:
            e, g = me[0], (giant[0] if giant else None)
            rows[t] = (
                e[F["deploy_ticks"]],
                e[F["attack_phase"]],
                g is not None and e[F["target_uid"]] == g[F["uid"]],
                e[F["x"]] / SUB,
                e[F["y"]] / SUB,
                None if g is None else g[F["x"]] / SUB,
                None if g is None else g[F["y"]] / SUB,
            )
    return rows, launches


def deploy_end(rows) -> int:
    return min(t for t, r in rows.items() if r[0] == 0)


def lock(rows) -> int:
    return min(t for t, r in rows.items() if r[1] != 0 and r[2])


def steps(rows, first, n=9):
    return [(rows[t][3] - rows[t - 1][3], rows[t][4] - rows[t - 1][4]) for t in range(first, first + n)]


def first_launch_after(rows, launches):
    free, locked = deploy_end(rows), lock(rows)
    assert launches, "the unit never launched"
    assert launches[0] > locked, f"a launch on {launches[0]} before the lock on {locked}"
    return free, locked, launches[0]


def test_first_launch_after_a_walk_is_79_ticks_after_the_deploy_end():
    rows, launches = run("ZapMachine", WALK_TAP, 1, CHARGE_ONLY)
    free, locked, first = first_launch_after(rows, launches)
    assert 2 <= locked - free <= 60, (
        f"the scenario drifted: the lock is on deploy end + {locked - free}, not after a walk with the timer above 0"
    )
    assert first - free == FIRST_LAUNCH, (
        f"first launch on deploy end + {first - free} (lock + {first - locked}; deploy end {free}, lock {locked}), "
        f"not deploy end + {FIRST_LAUNCH}"
    )


def test_first_launch_straight_out_of_the_deploy_is_79_ticks_after_the_deploy_end():
    rows, launches = run("ZapMachine", DEPLOY_TAP, DEPLOY_TICK, CHARGE_ONLY)
    free, locked, first = first_launch_after(rows, launches)
    assert locked - free == 1, f"the scenario drifted: the lock is on deploy end + {locked - free}, not + 1"
    assert first - free == FIRST_LAUNCH, (
        f"first launch on deploy end + {first - free} (lock + {first - locked}), not deploy end + {FIRST_LAUNCH}"
    )


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_control_musketeer_first_launch_is_timed_from_the_lock(arm):
    """Control: no LoadFirstHit, so the load timer leaves the deploy at 0 and the first launch is 13 after the lock."""
    rows, launches = run("Musketeer", DEPLOY_TAP, DEPLOY_TICK, arm)
    free, locked, first = first_launch_after(rows, launches)
    assert locked - free == 1, f"the scenario drifted: the lock is on deploy end + {locked - free}, not + 1"
    assert first - locked == MUSKETEER_FIRST, (
        f"the Musketeer launched on lock + {first - locked}, not + {MUSKETEER_FIRST}"
    )


def test_launch_recoils_the_sparky_down_the_ladder():
    rows, launches = run("ZapMachine", DEPLOY_TAP, DEPLOY_TICK, RECOIL_ONLY)
    first = launches[0] if launches else None
    assert first is not None, "the Sparky never launched"
    pre = rows[first - 1]
    assert pre[2], "the scenario drifted: the Sparky was not on the Giant before its launch"
    assert pre[5] is not None, "the scenario drifted: the Giant was not alive before the Sparky's launch"
    got = steps(rows, first)
    lens = [round(math.hypot(*s), 1) for s in got]
    # 2.5, not 1.5: the recoil keeps its direction to 1/256 and cuts each step to whole native units, so a step
    # falls short of the ladder by up to about sqrt(2) x (1 + step / 256), 2.4 for 175, depending on the heading.
    # In this scene the second step is (-114, -95), 148.4 long; the client's recorded launches had other headings.
    assert all(abs(a - b) <= 2.5 for a, b in zip(lens, LADDER, strict=True)), (
        f"step lengths from the launch tick {first}: {lens}, not {list(LADDER)}"
    )
    to_target = (pre[5] - pre[3], pre[6] - pre[4])
    norm = math.hypot(*to_target)
    cos_away = [(s[0] * to_target[0] + s[1] * to_target[1]) / (math.hypot(*s) * norm) for s in got[:7]]
    assert max(cos_away) <= -0.99, f"the first 7 steps do not point away from the target (cosines {cos_away})"
    back = (got[8][0] * got[0][0] + got[8][1] * got[0][1]) / (math.hypot(*got[8]) * math.hypot(*got[0]))
    assert back <= -0.99, f"the 9th step does not come back along the recoil line (cosine {back})"
    net = math.hypot(rows[first + 8][3] - pre[3], rows[first + 8][4] - pre[4])
    assert 665 <= net <= 680, f"the net recoil over 9 ticks is {net}, not about 675"


def test_recoil_makes_the_cycle_79_ticks():
    """The recoil takes the Sparky out of the attack for 8 ticks; it re-enters on launch + 9 and launches 70 later."""
    rows, launches = run("ZapMachine", WALK_TAP, 1, RECOIL_ONLY)
    assert len(launches) >= 3, f"the scenario drifted: only {len(launches)} launches"
    for t in launches[1:3]:
        assert rows[t - 1][2], f"the scenario drifted: the Sparky was not on the Giant before its launch on {t}"
    gaps = [b - a for a, b in itertools.pairwise(launches[:3])]
    assert gaps == [CYCLE, CYCLE], f"launches {launches[:3]}: gaps {gaps}, not {[CYCLE, CYCLE]}"
    for t in launches[:2]:
        phases = [rows[t + j][1] for j in range(1, 10)]
        assert phases[:8] == [0] * 8, f"after the launch on {t} the attack phase on +1..+8 is {phases[:8]}, not out"
        assert phases[8] != 0, f"after the launch on {t} the attack phase on +9 is {phases[8]}, not back in the attack"


@pytest.mark.parametrize("sparks", [SPARKS_NEW, SPARKS_OLD])
def test_launch_recoils_the_firecracker_down_its_own_ladder(sparks):
    """The same law on a second AttackPushBack, the Firecracker's 1000: a 10-tick ladder from the launch tick, 200
    first; out of the attack for the 9 ticks after the launch, back in on launch + 10, and the next launch 59 ticks
    after the first (HitSpeed 3000 gives 60). A recoil or a hold sized to the Sparky's 750 fails it. The sparks' arm
    is named: the recoil and the cycle are the same under both, and the launch count must not see the sparks."""
    rows, launches = run("Firecracker", DEPLOY_TAP, DEPLOY_TICK, RECOIL_ONLY, extra={KEY_SPARKS: sparks})
    assert len(launches) >= 2, f"the scenario drifted: only {len(launches)} Firecracker launches"
    first = launches[0]
    pre = rows[first - 1]
    assert pre[2], f"the scenario drifted: the Firecracker was not on the Giant on {first - 1}"
    assert pre[5] is not None, f"the scenario drifted: the Giant was not alive on {first - 1}"
    got = steps(rows, first, n=len(FIRECRACKER_LADDER))
    lens = [round(math.hypot(*s), 1) for s in got]
    assert all(abs(a - b) <= 2 for a, b in zip(lens, FIRECRACKER_LADDER, strict=True)), (
        f"step lengths from the launch tick {first}: {lens}, not {list(FIRECRACKER_LADDER)}"
    )
    to_target = (pre[5] - pre[3], pre[6] - pre[4])
    norm = math.hypot(*to_target)
    cos_away = [(s[0] * to_target[0] + s[1] * to_target[1]) / (math.hypot(*s) * norm) for s in got[:8]]
    assert max(cos_away) <= -0.99, f"the first 8 steps do not point away from the target (cosines {cos_away})"
    phases = [rows[first + j][1] for j in range(1, 11)]
    assert phases[:9] == [0] * 9, f"after the launch on {first} the attack phase on +1..+9 is {phases[:9]}, not out"
    assert rows[first + 10][2], f"the scenario drifted: the Firecracker was not on the Giant on {first + 10}"
    back = rows[first + 9]
    reach = math.hypot(back[5] - back[3], back[6] - back[4])
    assert reach <= FIRECRACKER_REACH, f"the scenario drifted: the Giant was {reach:.0f} away on {first + 9}"
    assert phases[9] != 0, f"after the launch on {first} the attack phase on +10 is {phases[9]}, not back in"
    assert launches[1] - first == FIRECRACKER_CYCLE, (
        f"launches {launches[:2]} are {launches[1] - first} apart, not {FIRECRACKER_CYCLE}"
    )


def test_both_keys_give_the_measured_launch_ticks():
    """Both scenarios as measured on client 15.535.29: launches on deploy end + 79, + 158, + 237 (walk: ticks 99, 178,
    257) and + 79, + 158 (straight out of the deploy: ticks 197, 276)."""
    for tap, tick, n in ((WALK_TAP, 1, 3), (DEPLOY_TAP, DEPLOY_TICK, 2)):
        rows, launches = run("ZapMachine", tap, tick, NEW_ARM)
        free = deploy_end(rows)
        got = [t - free for t in launches[:n]]
        assert got == [FIRST_LAUNCH + CYCLE * k for k in range(n)], (
            f"tap {tap}: launches {launches[:n]} are deploy end ({free}) + {got}"
        )


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_control_musketeer_does_not_recoil(arm):
    """Control: no AttackPushBack, so the Musketeer stands through the 9 ticks from its first launch."""
    rows, launches = run("Musketeer", DEPLOY_TAP, DEPLOY_TICK, arm)
    assert launches, "the Musketeer never launched"
    first = launches[0]
    assert all(rows[first + j][2] and rows[first + j][1] != 0 for j in range(9)), (
        "the scenario drifted: the Musketeer did not keep attacking the Giant through the 9 ticks"
    )
    got = steps(rows, first)
    assert all(s == (0, 0) for s in got), f"the Musketeer moved after its launch on {first}: {got}"


def test_old_arm_is_todays_engine():
    rows, launches = run("ZapMachine", WALK_TAP, 1, OLD_ARM)
    free, locked, first = first_launch_after(rows, launches)
    assert (free, locked, first) == (20, 56, 75), f"old arm: deploy end {free}, lock {locked}, first launch {first}"
    assert launches[1] - launches[0] == 80, f"old arm: launches {launches[:2]} are not 80 apart"
    assert all(s == (0, 0) for s in steps(rows, first)), f"old arm: the Sparky moved after its launch on {first}"
