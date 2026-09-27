//! knockback.PUSH_LOAD_TIMER, read off the engine: what a landed push (knockback.ATTACK_RESET =
//! reset_attack_keep_target) does to the victim's load timer (state.rs `apply_effects`).
//!
//! THE LAW, measured on client 15.535.29 and the 16.402 corpus: the push zeroes the swing and leaves the load timer as
//! it was; it runs down through the ladder (23 of 23 push landings where that differs from a reset, 25 records: 15
//! Knights on client 15.535.29, 16 records, and 8 landings on the 16.402 corpus, 9 records), and the re-entry takes
//! the ordinary progress credit off what is left (24 of 24 re-entries after a push). A Bomber pushed at load 0
//! re-enters and launches 3 ticks later (20260920-081051, 963 and 966), as on its first entry. The old arm
//! (reset_to_load_time) sets the timer back to LoadTime on the landing tick.
//!
//! THE SCENES.
//!   * The Bowler's, as tests/test_push_load_timer.py plays them, with combat.RANGE_PROJECTILE = straight_to_range (the
//!     boulder pushes only there): a blue Bowler at (14735, 17126) whose first boulder pushes a red Bomber at
//!     (13669, 21900) after the Bomber's first launch; and, as the control, a red Knight at (9500, 14200) in melee
//!     reach of a blue Bowler at (9500, 12000), pushed before its first hit.
//!   * tests/spells.rs `knockback_resets_a_windup`'s: a red Musketeer shooting a blue Cannon 4,000 away, and a blue
//!     Fireball that lands on it, timed over 40 delays, with a CONTROL copy without the Fireball beside it.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. client_runs_on: the Bomber re-enters more than LoadTime after its first launch and launches as many ticks after
//!      the re-entry as it did after its first entry;
//!   2. reset_to_load_time: the same launch comes later than that (the old arm);
//!   3. both values: the Knight, back in its attack more than LoadTime after the push, hits HitSpeed / 50 - LoadTime /
//!      50 - 1 ticks after the re-entry, as a unit on its first entry does;
//!   4. client_runs_on: on every Fireball landing where the control Musketeer is mid-swing, the pushed Musketeer's
//!      load timer equals the control's, and on at least 3 of them the control's is not LoadTime (so the arms part);
//!   5. reset_to_load_time: the pushed Musketeer's load timer is LoadTime on every such landing;
//!   6. the shipped value is client_runs_on.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test push_load_timer`):
//!   * `push_load_timer_reset` -- client_runs_on still sets the load timer to LoadTime on the landing tick: (1) and
//!     (4) go red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{milli, Vec2, SUBTILE, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AttackCycle, BattleConfig, BattleState, Calib, PushLoadTimer, RangeProjectile};
use royalesim::Team;

const NEW: PushLoadTimer = PushLoadTimer::ClientRunsOn;
const OLD: PushLoadTimer = PushLoadTimer::ResetToLoadTime;

const BOWLER_AT: (i32, i32) = (14735, 17126);
const BOMBER_AT: (i32, i32) = (13669, 21900);
const CONTROL_BOWLER_AT: (i32, i32) = (9500, 12000);
const KNIGHT_AT: (i32, i32) = (9500, 14200);
const TICKS: u32 = 110;

fn with_arm(arm: PushLoadTimer) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.range_projectile = RangeProjectile::StraightToRange;
    cfg.calib.push_load_timer = arm;
    cfg
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// A card's LoadTime and HitSpeed in ticks, from cards.json.
fn ticks_of(name: &str) -> (u32, u32) {
    let db = cards();
    let c = db.get(db.index(name).unwrap_or_else(|| panic!("{name} in cards.json")));
    ((c.load_time_ms / 50) as u32, (c.hit_speed_ms / 50) as u32)
}

/// The victim's timeline, ticks: its first entry into the attack, its first hit, the push (the first tick it is
/// sliding), its re-entry after the ladder, and its first hit after that.
#[derive(Debug)]
struct Timeline {
    entry: u32,
    hit: Option<u32>,
    push: u32,
    reentry: u32,
    hit_after: u32,
}

fn run(arm: PushLoadTimer, victim: &str, victim_at: (i32, i32), bowler_at: (i32, i32)) -> Timeline {
    let mut s = BattleState::new(0, with_arm(arm));
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Bowler", at(bowler_at), None), (Team::Red, victim, at(victim_at), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let v = ids[1];
    // (tick, phase, sliding) after each tick
    let mut rows = Vec::new();
    for tick in 1..=TICKS {
        s.tick();
        let Some(ev) = s.entity(v) else { break };
        rows.push((tick, ev.attack_phase, ev.push_active || ev.knock_ms > 0));
    }
    let find = |after: u32, pred: &dyn Fn(AttackPhase, bool) -> bool| rows.iter().find(|r| r.0 > after && pred(r.1, r.2)).map(|r| r.0);
    let entry = find(0, &|p, _| p != AttackPhase::Idle).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the {victim} never attacked"));
    let hit = find(0, &|p, _| p == AttackPhase::Cooldown);
    let push = find(0, &|_, k| k).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the {victim} was never pushed"));
    let end = find(push, &|_, k| !k).expect("the ladder never ended");
    let reentry = find(end - 1, &|p, k| p != AttackPhase::Idle && !k).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: no re-entry"));
    let hit_after = find(reentry, &|p, _| p == AttackPhase::Cooldown).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: no hit after the re-entry"));
    Timeline { entry, hit, push, reentry, hit_after }
}

/// The Bomber's first launch after the push, as ticks after its re-entry, beside its first entry's gap.
fn bomber_gaps(arm: PushLoadTimer) -> (Timeline, u32, u32) {
    let (load, _) = ticks_of("Bomber");
    let t = run(arm, "Bomber", BOMBER_AT, BOWLER_AT);
    let hit = t.hit.unwrap_or_else(|| panic!("{arm:?}: the scene drifted: no launch before the push: {t:?}"));
    assert!(hit < t.push, "{arm:?}: the scene drifted: the push came before the first launch: {t:?}");
    assert!(t.reentry - hit > load, "{arm:?}: the scene drifted: re-entry within LoadTime of the first launch: {t:?}");
    let (first, after) = (hit - t.entry, t.hit_after - t.reentry);
    (t, first, after)
}

/// Plant: push_load_timer_reset.
#[test]
fn a_pushed_unit_reenters_with_its_load_timer_run_down() {
    let (t, first, after) = bomber_gaps(NEW);
    assert_eq!(after, first, "the Bomber launched {after} ticks after re-entering on {} (pushed on {}), not the {first} of its first entry", t.reentry, t.push);
}

#[test]
fn the_old_value_resets_the_timer_and_swings_late() {
    let (t, first, after) = bomber_gaps(OLD);
    assert!(after > first, "the old value launched {after} ticks after re-entering on {}, not later than {first}", t.reentry);
}

#[test]
fn a_unit_back_after_its_load_time_swings_the_same_on_both_values() {
    let (load, hit_speed) = ticks_of("Knight");
    for arm in [NEW, OLD] {
        let t = run(arm, "Knight", KNIGHT_AT, CONTROL_BOWLER_AT);
        assert!(t.reentry - t.push > load, "{arm:?}: the scene drifted: the Knight re-entered within LoadTime of the push: {t:?}");
        assert_eq!(t.hit_after - t.reentry, hit_speed - load - 1, "{arm:?}: the Knight's first hit after the re-entry: {t:?}");
    }
}

/// Tick calls, from `s` as it is, until every spell object is gone.
fn arrival_calls(s: &BattleState) -> u32 {
    let mut p = s.clone();
    for k in 1..=600 {
        p.tick();
        if p.spells().is_empty() {
            return k;
        }
    }
    panic!("spell never arrived within 600 ticks");
}

/// The Musketeer scene of tests/spells.rs over 40 delays, without the Bowler's projectile override: for each landing
/// where the control is mid-swing, (the pushed Musketeer's load timer, the control's). Asserts on each that the push
/// landed and reset the attack.
fn musketeer_landings(arm: PushLoadTimer) -> (Vec<(i32, i32)>, i32) {
    assert_eq!(config().calib.attack_cycle, AttackCycle::ProgressCredit, "these tests read the progress-credit cycle");
    let tap = t(900, 1900);
    let musk_at = Vec2::new(tap.x + SUBTILE, tap.y);
    let cannon_at = Vec2::new(musk_at.x - milli(4000), musk_at.y);
    let musk = card_stat(&BattleState::new(1, config()), "Musketeer").clone();
    let (load, hs) = (musk.load_time_ms, musk.hit_speed_ms);
    let tick = config().calib.tick_ms;
    let mut out = Vec::new();
    for delay in 0..40u32 {
        let mut cfg = config();
        cfg.calib.push_load_timer = arm;
        let mut s = BattleState::new(1, cfg);
        let m = s.scenario_spawn_now(Team::Red, "Musketeer", musk_at, Some(1_000_000)).unwrap();
        s.scenario_spawn_now(Team::Blue, "Cannon", cannon_at, None).unwrap();
        for _ in 0..delay {
            s.tick();
        }
        let mut control = s.clone();
        s.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
        for _ in 0..arrival_calls(&s) {
            s.tick();
            control.tick();
        }
        let cv = control.entity(m).unwrap();
        if !(cv.attack_phase == AttackPhase::Windup && cv.attack_ms > 0 && cv.attack_ms % hs + tick < hs) {
            continue;
        }
        let v = s.entity(m).unwrap();
        assert!(v.push_active || v.pos != cv.pos, "delay {delay}: the Fireball did not push the Musketeer");
        assert_eq!((v.attack_phase, v.attack_ms), (AttackPhase::Idle, 0), "delay {delay}: the push did not reset the attack");
        out.push((v.attack_load_ms, cv.attack_load_ms));
    }
    assert!(out.len() >= 3, "vacuous: only {} landings came mid-swing", out.len());
    (out, load)
}

/// Plant: push_load_timer_reset.
#[test]
fn a_pushed_musketeer_keeps_the_unpushed_controls_load_timer() {
    let (rows, load) = musketeer_landings(NEW);
    let parted = rows.iter().filter(|r| r.1 != load).count();
    assert!(parted >= 3, "vacuous: on only {parted} landings was the control's timer not LoadTime {load}");
    for (pushed, control) in rows {
        assert_eq!(pushed, control, "client_runs_on: the pushed Musketeer's load timer is {pushed}, the control's {control}");
    }
}

#[test]
fn the_old_value_reloads_the_pushed_musketeers_timer() {
    let (rows, load) = musketeer_landings(OLD);
    for (pushed, _) in rows {
        assert_eq!(pushed, load, "reset_to_load_time: the pushed Musketeer's load timer is {pushed}, not LoadTime");
    }
}

#[test]
fn the_shipped_value_is_the_new_one() {
    assert_eq!(Calib::shipped().push_load_timer, NEW);
}
