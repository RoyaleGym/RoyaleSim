"""A landed push leaves the victim's load timer running (knockback.PUSH_LOAD_TIMER).

WHAT THIS PINS. knockback.ATTACK_RESET = reset_attack_keep_target: a push that lands on a unit in its attack zeroes its
swing and keeps its target. On client 15.535.29 and the 16.402 corpus the push does NOT touch the load timer: it reads
its value before minus 50 (or 0) on the first ladder frame and runs down through the ladder, 23 of 23 push landings
where that differs from a reset (25 records: 15 Knights on client 15.535.29, 16 records, and 8 landings on the 16.402
corpus, 9 records). The re-entry after the ladder then takes the ordinary progress credit off what is left
(combat.ATTACK_CYCLE = progress_credit), 24 of 24 re-entries after a push. Today's engine sets the timer to LoadTime
on the landing tick, so a unit that re-enters within LoadTime of the push swings late.

THE SCENES.
  * The Bowler's: a blue Bowler at (14735, 17126) and a red Bomber at (13669, 21900) that its first boulder pushes
    after the Bomber's first launch; both spawned standing, with combat.RANGE_PROJECTILE = straight_to_range (the
    boulder pushes only there). The Bomber re-enters its attack more than LoadTime after its first launch, so under the
    measured law its timer has run out and its first launch after the push comes as many ticks after the re-entry as
    its first launch came after its first entry (the client's Bomber of 20260920-081051: re-entry 963, launch 966).
  * The client's Mega Knight deploy scenario, with combat.DEPLOY_PROJECTILE = client_on_landing: blue plays a Mega
    Knight at (14600, 13489) beside a red Knight at (14189, 13535). The Knight attacks it at once, the deploy blow lands
    6 ticks after the Mega Knight's first frame with 400 of the Knight's 700 left and pushes it down the ladder; the
    timer runs out during the ladder, the Knight re-enters with progress 750 (LoadTime + 50) and hits 9 ticks later
    (the client: 276 and 285). Today's engine hits 12 ticks after the re-entry, 3 ticks late.

WHY THE CONTROLS ARE HERE. A red Knight in melee reach of a Bowler is pushed before its first hit and walks back in
more than LoadTime after the push, so its timer has run out under both values and its first hit after the re-entry
comes the first-entry gap later on both. And after the deploy scenario's Knight's first hit its cycle is HitSpeed (24
ticks) under both values: the key moves the first hit after a push, not the attack cycle. An implementation that
shortens every re-entry or every cycle, or times the re-entry from the push, fails them.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `push_load_timer_reset` -- client_runs_on still sets the load timer to LoadTime on the landing tick:
    test_a_pushed_unit_reenters_with_its_load_timer_run_down, test_the_knight_hits_nine_ticks_after_its_re_entry.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "knockback.PUSH_LOAD_TIMER"
NEW_ARM, OLD_ARM = "client_runs_on", "reset_to_load_time"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
BASE = {"combat.RANGE_PROJECTILE": "straight_to_range"}
BOWLER_AT = (14735, 17126)
#: the Bomber: pushed after its first launch, back in its attack more than LoadTime (1600 ms, 32 ticks) after it
BOMBER_AT, BOMBER_LOAD_TICKS = (13669, 21900), 1600 // 50
#: the control: a Knight in melee reach of a Bowler, pushed before its first hit
CONTROL_BOWLER_AT, KNIGHT_AT, KNIGHT_LOAD_TICKS = (9500, 12000), (9500, 14200), 700 // 50
IDLE, FIRED = 0, 2  # attack_phase: not attacking; attacking with its hit (a launch) on this tick
#: the two readings of the re-entry gap
NEW_GAP, OLD_GAP = "the first-entry gap", "longer than the first-entry gap"
#: the Mega Knight deploy scenario
MK_AT, MK_KNIGHT_AT = (14600, 13489), (14189, 13535)
KNIGHT_HIT = 202  # the Knight's damage at level 11
DEPLOY_BLOW = 430  # MegaKnightAppear at level 11
#: HitSpeed 1200 / 50 - LoadTime 700 / 50 - 1: the first hit after an entry with the timer run out
AFTER_ENTRY = 9


def overrides(arm, base=None) -> dict:
    """`arm` None runs the build's own value of KEY."""
    over = dict(BASE if base is None else base)
    if arm is not None:
        over[KEY] = arm
    return {k: json.dumps(v) for k, v in over.items()}


def run(arm, victim, victim_at, bowler_at, ticks=110):
    """Per tick t (1..): the red victim's (attack_phase, knockback_ticks) after the tick."""
    b = royalesim.Battle(["Bowler", victim], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    spawns = [(0, 0, bowler_at[0] * SUB, bowler_at[1] * SUB, -1), (1, 1, victim_at[0] * SUB, victim_at[1] * SUB, -1)]
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, spawns)
    rows = []
    for _ in range(ticks):
        b.step([], 1)
        red = [e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0 and e[F["team"]] == 1]
        if not red:
            break
        rows.append((red[0][F["attack_phase"]], red[0][F["knockback_ticks"]]))
    return rows


def timeline(rows):
    """(first entry E, first hit H0, push P, re-entry R, first hit after the push H1), 1-based ticks, None if absent."""
    at = [i + 1 for i in range(len(rows))]

    def first(pred, after=0):
        return next((t for t, r in zip(at, rows, strict=True) if t > after and pred(r)), None)

    e = first(lambda r: r[0] != IDLE)
    h0 = first(lambda r: r[0] == FIRED)
    p = first(lambda r: r[1] > 0)
    end = first(lambda r: r[1] == 0, p) if p else None
    r = first(lambda r: r[0] != IDLE and r[1] == 0, end) if end else None
    h1 = first(lambda r: r[0] == FIRED, r) if r else None
    return e, h0, p, r, h1


def assert_first_launch_after_the_push(arm, reading):
    e, h0, p, r, h1 = timeline(run(arm, "Bomber", BOMBER_AT, BOWLER_AT))
    assert None not in (e, h0, p, r, h1), (
        f"{arm}: the scene drifted: entry, launch, push, re-entry, launch = {e, h0, p, r, h1}"
    )
    assert h0 < p, f"{arm}: the scene drifted: the push on {p} came before the Bomber's first launch on {h0}"
    assert r - h0 > BOMBER_LOAD_TICKS, (
        f"{arm}: the scene drifted: the Bomber re-entered on {r}, {r - h0} ticks after its launch on {h0}, before its "
        f"load timer ({BOMBER_LOAD_TICKS} ticks) could run out"
    )
    first_gap, gap = h0 - e, h1 - r
    if reading == NEW_GAP:
        assert gap == first_gap, (
            f"{arm}: the Bomber launched {gap} ticks after re-entering its attack on {r} (pushed on {p}), not the "
            f"{first_gap} of its first entry: its load timer did not run on through the push"
        )
    else:
        assert gap > first_gap, (
            f"{arm}: the Bomber launched {gap} ticks after re-entering on {r}, not later than {first_gap}"
        )


def test_a_pushed_unit_reenters_with_its_load_timer_run_down():
    assert_first_launch_after_the_push(NEW_ARM, NEW_GAP)


def test_old_arm_is_todays_engine():
    assert_first_launch_after_the_push(OLD_ARM, OLD_GAP)


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_unit_back_after_its_load_time_swings_the_same_on_both_arms(arm):
    e, _, p, r, h1 = timeline(run(arm, "Knight", KNIGHT_AT, CONTROL_BOWLER_AT))
    assert None not in (e, p, r, h1), f"{arm}: the scene drifted: entry, push, re-entry, hit = {e, p, r, h1}"
    assert r - p > KNIGHT_LOAD_TICKS, (
        f"{arm}: the scene drifted: the Knight re-entered on {r}, within LoadTime of the push on {p}"
    )
    # Its first entry's gap: the Knight was pushed before its first hit, so it is read off the re-entry of a unit
    # whose timer ran out on both arms: HitSpeed - LoadTime, less the one tick of the entry itself.
    first_gap = (1200 - 700) // 50 - 1
    assert h1 - r == first_gap, f"{arm}: the Knight hit {h1 - r} ticks after re-entering on {r}, not {first_gap}"


def run_deploy(arm, ticks=60):
    """The Mega Knight deploy scenario. Per tick: (tick, the Knight's attack phase, the Mega Knight's hp loss, the
    Knight's hp loss)."""
    b = royalesim.Battle(
        ["MegaKnight", "Knight"],
        [[0, 1, 2], [0, 1, 2]],
        calibration_overrides=overrides(arm, {"combat.DEPLOY_PROJECTILE": "client_on_landing"}),
    )
    b.reset(
        0,
        [[0, 1, 0, 1, 0, 1, 0, 1]] * 2,
        0,
        200,
        [10_000, 10_000],
        None,
        [(1, 1, MK_KNIGHT_AT[0] * SUB, MK_KNIGHT_AT[1] * SUB, -1)],
    )
    out = b.step([(0, 0, MK_AT[0] * SUB, MK_AT[1] * SUB)], 1)
    assert out, "the Mega Knight play returned nothing"
    assert out[0][1] == 0, f"the Mega Knight was refused: {out}"
    rows, prev = [], None
    for _ in range(ticks):
        s = json.loads(b.state_json())
        mk = next((e for e in s["entities"] if e[F["team"]] == 0 and e[F["tower_slot"]] < 0), None)
        kn = next((e for e in s["entities"] if e[F["team"]] == 1 and e[F["tower_slot"]] < 0), None)
        if prev is not None and mk is not None and kn is not None:
            losses = (prev[0][F["hp"]] - mk[F["hp"]], prev[1][F["hp"]] - kn[F["hp"]])
            rows.append((s["tick"], kn[F["attack_phase"]], *losses))
        prev = (mk, kn) if mk is not None and kn is not None else None
        b.step([], 1)
    return rows


def deploy_phases(rows):
    """The blow tick, the Knight's re-entry after the ladder (its first attacking tick after the blow + 1) and its
    first hit on the Mega Knight after that."""
    blow = next((t for t, _, _, kl in rows if kl == DEPLOY_BLOW), None)
    assert blow is not None, "the scene drifted: the deploy blow never landed on the Knight"
    back = next((t for t, ph, _, _ in rows if t > blow + 1 and ph != 0), None)
    assert back is not None, "the scene drifted: the Knight never re-entered its attack"
    hit = next((t for t, _, ml, _ in rows if t >= back and ml == KNIGHT_HIT), None)
    assert hit is not None, "the scene drifted: the Knight never hit the Mega Knight after the push"
    return blow, back, hit


def test_the_knight_hits_nine_ticks_after_its_re_entry():
    blow, back, hit = deploy_phases(run_deploy(NEW_ARM))
    assert back - blow >= 9, f"the scene drifted: the Knight re-entered {back - blow} ticks after the blow"
    want = back + AFTER_ENTRY
    assert hit == want, f"{NEW_ARM}: the Knight re-entered on {back} and hit on {hit}, not on {want}"


def test_the_deploy_scene_old_arm_hits_late():
    _, back, hit = deploy_phases(run_deploy(OLD_ARM))
    assert hit - back > AFTER_ENTRY, f"{OLD_ARM}: the Knight hit {hit - back} ticks after its re-entry"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_the_cycle_after_the_first_hit_is_the_same(arm):
    rows = run_deploy(arm, ticks=80)
    _, _, hit = deploy_phases(rows)
    nxt = next((t for t, _, ml, _ in rows if t > hit and ml == KNIGHT_HIT), None)
    assert nxt is not None, "the scene drifted: the Knight did not hit twice"
    assert nxt - hit == 24, f"{arm}: the Knight's second hit came {nxt - hit} ticks after its first"
