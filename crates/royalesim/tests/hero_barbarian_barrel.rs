//! THE HERO BARBARIAN BARREL'S RE-ROLL (tools/extract_cards.py `spell_hero_card`, `reroll_button`; card.rs `ReRollDef`,
//! REROLL_ROLL_SHORT_STEPS; state.rs `RerollRun`, `reroll_pass`, `reroll_logs`, EARLY_TRIGGER_TICKS), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-BarbLog-hero-s0; the press issued t316, the cast from t317): the
//! Barbarian 142 back a tick on t318..t323, healed half its missing hitpoints on t324 (607 -> 661), on the log's point
//! of the tick before from t325 (200 a tick), 2800 on from t339, deploying t339..t358.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_barbarian_barrel`): reroll_never, reroll_never_lands, reroll_log_never, early_trigger_late,
//! reroll_hidden_at_trigger (parity r88); parity r98:
//!   - reroll_holders_drop_at_hide ->
//!     `under_client16635_holders_let_go_next_tick_its_holder_keeps_it_through_the_logs_tick` red;
//!   - reroll_log_hits_next_tick -> `under_client16635_log_tick_the_reroll_log_hits_on_its_own_tick` red;
//!
//! parity r99:
//!   - reroll_rider_is_body ->
//!     `under_client16402_no_body_ride_to_land_plus_1_the_rider_meets_no_body_through_its_landing_plus_1` and
//!     `under_client16402_no_body_ride_to_land_plus_1_the_logs_tick_is_still_a_body` red;
//!
//! parity r100:
//!   - reroll_cast_decides and reroll_standup_counts ->
//!     `under_client16635_cast_end_wait_a_kill_in_the_cast_walks_on_the_landing_plus_22` red;
//!   - reroll_clock_unread -> `under_client16635_cast_end_clock_the_stand_up_reenters_on_the_load_timer_the_ride_left`
//!     red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, RerollHideWindow, RerollHoldEnd, RerollLogFirstHit, RerollRideBody};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["BarbLog", "Knight", "Archers", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
const HERO: &str = "BarbLogBarbarianHero";

/// The hero Barbarian Barrel played at (3500, 9000) and its Barbarian stood up: the battle and the Barbarian.
fn barbarian() -> (BattleState, EntityId) {
    barbarian_under(RerollHideWindow::AtTrigger)
}

/// As `barbarian`, under spells.REROLL_HIDE_WINDOW = `arm`.
fn barbarian_under(arm: RerollHideWindow) -> (BattleState, EntityId) {
    barbarian_with(|c| c.reroll_hide_window = arm)
}

/// As `barbarian`, with `set` applied to the calibration first.
fn barbarian_with(set: impl FnOnce(&mut Calib)) -> (BattleState, EntityId) {
    let mut cfg = config();
    set(&mut cfg.calib);
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "BarbLog", n(3500, 9000)).expect("the play");
    for _ in 0..120 {
        s.tick();
        let found = find_live(&s, Team::Blue, HERO).first().map(|e| (e.id, e.deploying));
        if let Some((id, false)) = found {
            return (s, id);
        }
    }
    panic!("no Barbarian stood up");
}

#[test]
fn its_press_slides_it_back_heals_it_and_rides_it_2800_on() {
    let (mut s, b) = barbarian();
    let at = n(3500, 9500);
    for _ in 0..3 {
        assert!(s.debug_set_pos(b, at));
        s.tick();
    }
    let max = s.entity(b).expect("the Barbarian").max_hp;
    assert!(s.debug_set_hp(b, max - 200));
    assert!(s.debug_set_pos(b, at));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press, on the Barbarian");
    let log = s.cards().index("BarbLog_hero_reroll").expect("the re-roll's log record");
    let mut rows: Vec<(u32, i32, i32, bool)> = Vec::new();
    let mut logs: Vec<u32> = Vec::new();
    for _ in 0..30 {
        s.tick();
        let e = s.entity(b).expect("the Barbarian");
        rows.push((s.tick_count() - 1 - p, e.pos.y / K, e.hp, e.deploying));
        if s.spells().iter().any(|x| x.card == log) {
            logs.push(s.tick_count() - 1 - p);
        }
    }
    assert_eq!(logs.first(), Some(&8), "the log put down on P + 8, rolling from P + 9: {logs:?}");
    let y = |k: u32| rows.iter().find(|r| r.0 == k).expect("a row").1;
    let y0 = at.y / K;
    // The trigger P + 1 (its cast's start, TriggerDelay 50 less a tick); the slide P + 2 .. P + 7.
    for (k, want) in (2..=7).zip([142, 284, 426, 568, 710, 852]) {
        assert_eq!(y(k), y0 - want, "the slide on P + {k}: {rows:?}");
    }
    let heal = rows.iter().find(|r| r.0 == 8).expect("row 8").2;
    assert_eq!(heal, max - 100, "healed half of its missing 200 on P + 8: {rows:?}");
    assert_eq!(y(9), y0 - 852, "still on the log's first point on P + 9: {rows:?}");
    assert_eq!(y(10), y0 - 652, "one step on on P + 10: {rows:?}");
    assert_eq!(y(23), y0 - 852 + 2800, "2800 on on P + 23: {rows:?}");
    assert!(rows.iter().find(|r| r.0 == 23).expect("row 23").3, "deploying on P + 23: {rows:?}");
}

/// Under spells.REROLL_HIDE_WINDOW = `arm`, the press as in the scene above: per tick from P + 1, (P + k, invisible to enemies
/// (its re-roll's buff; status bit 1), deploying, whether the tick just run's targeting could take it).
fn hide_rows(arm: RerollHideWindow) -> Vec<(u32, bool, bool, bool)> {
    let (mut s, b) = barbarian_under(arm);
    let at = n(3500, 9500);
    for _ in 0..3 {
        assert!(s.debug_set_pos(b, at));
        s.tick();
    }
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press, on the Barbarian");
    let mut rows = Vec::new();
    for _ in 0..30 {
        s.tick();
        let now = s.tick_count();
        let e = s.entity(b).expect("the Barbarian");
        rows.push((now - 1 - p, e.status_flags & 2 != 0, e.deploying, e.acquirable_from < now));
    }
    rows
}

/// spells.REROLL_HIDE_WINDOW = client16402_log_tick_to_land_plus_2 (parity r88, the oracle's replays of 100 battles on client
/// 16.402, 43 re-rolls: hittable through the back-slide in 22, the engine's 0; hidden from the log's tick; nobody's target
/// before the landing + 2): the Barbarian is not hidden on its slide (P + 2 .. P + 7), is hidden from the heal's tick (P + 8)
/// to its landing, and is not acquirable on the landing's tick nor the next; under at_trigger it is hidden on P + 2. Plant:
/// reroll_hidden_at_trigger.
#[test]
fn under_client16402_log_tick_to_land_plus_2_the_barbarian_is_hittable_on_its_slide_and_hidden_from_its_log() {
    let rows = hide_rows(RerollHideWindow::Client16402LogTickToLandPlus2);
    let row = |k: u32| *rows.iter().find(|r| r.0 == k).unwrap_or_else(|| panic!("no row P + {k}: {rows:?}"));
    for k in 2..=7 {
        assert!(!row(k).1, "client16402_log_tick_to_land_plus_2: hidden on its slide, P + {k}: {rows:?}");
    }
    assert!(row(9).1, "client16402_log_tick_to_land_plus_2: not hidden the tick after the heal's, P + 9: {rows:?}");
    let land = rows.iter().find(|r| r.2).expect("the scene drifted: it never stood up").0;
    assert!(!row(land).3 && !row(land + 1).3 && row(land + 2).3, "client16402_log_tick_to_land_plus_2: acquirable from the landing + 2 (landing P + {land}): {rows:?}");
    // NOT VACUOUS: the old arm hides it on its slide
    let old = hide_rows(RerollHideWindow::AtTrigger);
    assert!(old.iter().find(|r| r.0 == 2).is_some_and(|r| r.1), "at_trigger: not hidden on P + 2: {old:?}");
}

// spells.REROLL_HIDE_WINDOW ships client16635_holders_let_go_next_tick since parity r99 (client16402_log_tick_to_land_plus_2
// from r88): its pin is below, the_shipped_arm_is_client16635_holders_let_go_next_tick.

/// `holder_rows`' rows: whether the Musketeer held the Barbarian at the end of P + `k`.
fn held_on(rows: &[(u32, bool)], k: u32) -> bool {
    rows.iter().find(|r| r.0 == k).unwrap_or_else(|| panic!("no row P + {k}: {rows:?}")).1
}

/// Under spells.REROLL_HIDE_WINDOW = `arm`: a red Musketeer held on (3500, 14000), 4,500 ahead of the Barbarian (held on
/// (3500, 9500) and topped up), takes it; then the press. Per tick from P + 1: (P + k, whether the Musketeer holds the
/// Barbarian at the end of P + k). The slide's 852 keeps the Barbarian inside the Musketeer's reach.
fn holder_rows(arm: RerollHideWindow) -> Vec<(u32, bool)> {
    let (mut s, b) = barbarian_under(arm);
    let (at, m_at) = (n(3500, 9500), n(3500, 14000));
    let m = s.scenario_spawn_now(Team::Red, "Musketeer", m_at, None).expect("the Musketeer");
    let max = s.entity(b).expect("the Barbarian").max_hp;
    let holds = |s: &BattleState| s.entity(m).is_some_and(|e| e.target == Some(b));
    let mut took = false;
    for _ in 0..40 {
        assert!(s.debug_set_pos(b, at) && s.debug_set_hp(b, max) && s.debug_set_pos(m, m_at));
        s.tick();
        if s.entity(m).is_some_and(|e| !e.deploying && e.target == Some(b)) {
            took = true;
            break;
        }
    }
    assert!(took, "the scene drifted: the Musketeer never took the Barbarian");
    assert!(s.debug_set_pos(b, at) && s.debug_set_pos(m, m_at));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press, on the Barbarian");
    let mut rows = Vec::new();
    for _ in 0..12 {
        assert!(s.debug_set_pos(m, m_at));
        s.tick();
        rows.push((s.tick_count() - 1 - p, holds(&s)));
    }
    rows
}

/// spells.REROLL_HIDE_WINDOW = client16635_holders_let_go_next_tick (client 16.635, parity r98, the oracle's il-mg-16635 set:
/// 18 of 18 holders in 8 re-rolls held the Barbarian on its log's tick and let it go on the next): the Musketeer holds it
/// through the slide and the log's tick (P + 2 .. P + 8) and lets it go on P + 9; under client16402_log_tick_to_land_plus_2
/// (the vacuity check) it holds it on P + 7 and lets it go on P + 8. Plant: reroll_holders_drop_at_hide.
#[test]
fn under_client16635_holders_let_go_next_tick_its_holder_keeps_it_through_the_logs_tick() {
    let rows = holder_rows(RerollHideWindow::Client16635HoldersLetGoNextTick);
    for k in 2..=8 {
        assert!(held_on(&rows, k), "client16635_holders_let_go_next_tick: let go on P + {k}: {rows:?}");
    }
    assert!(!held_on(&rows, 9), "client16635_holders_let_go_next_tick: still held on P + 9: {rows:?}");
    // NOT VACUOUS: the shipped arm lets it go on the log's tick
    let old = holder_rows(RerollHideWindow::Client16402LogTickToLandPlus2);
    assert!(held_on(&old, 7) && !held_on(&old, 8), "client16402_log_tick_to_land_plus_2 (the vacuity check): held on P + 7, let go on P + 8: {old:?}");
}

/// Under spells.REROLL_LOG_FIRST_HIT = `arm` (and spells.REROLL_HIDE_WINDOW = client16402_log_tick_to_land_plus_2): the press
/// as in the slide scene; at the end of P + 7 a red Knight with 1 hitpoint is put down on (4000, 8648), 500 beside the log's
/// first point (the Barbarian's, 852 back) and inside its 1,300 half-width: (the Knight stands at the end of P + 8, at the
/// end of P + 9). The log stands in the spells at the end of P + 8 under both arms.
fn log_start_victim(arm: RerollLogFirstHit) -> (bool, bool) {
    let (mut s, b) = barbarian_with(|c| {
        c.reroll_hide_window = RerollHideWindow::Client16402LogTickToLandPlus2;
        c.reroll_log_first_hit = arm;
    });
    let at = n(3500, 9500);
    for _ in 0..3 {
        assert!(s.debug_set_pos(b, at));
        s.tick();
    }
    assert!(s.debug_set_pos(b, at));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press, on the Barbarian");
    for _ in 0..7 {
        s.tick();
    }
    assert_eq!(s.tick_count() - 1, p + 7);
    let k = s.scenario_spawn_now(Team::Red, "Knight", n(4000, 8648), None).expect("the Knight");
    assert!(s.debug_set_hp(k, 1));
    let log = s.cards().index("BarbLog_hero_reroll").expect("the re-roll's log record");
    let stands = |s: &BattleState| s.entity(k).is_some_and(|e| e.hp > 0);
    s.tick();
    assert!(s.spells().iter().any(|x| x.card == log), "the scene drifted: no re-roll log at the end of P + 8 ({arm:?})");
    let on_log_tick = stands(&s);
    s.tick();
    (on_log_tick, stands(&s))
}

/// spells.REROLL_LOG_FIRST_HIT = client16635_log_tick (client 16.635, parity r98, the oracle's il-mg-16635 set: 4 hits in 2
/// re-rolls on the log's tick, the engine's a tick later): the log, put down before the log tick's spell pass, sweeps its first
/// point there, so the Knight beside it falls on P + 8; under next_tick (the vacuity check) it stands through P + 8 and falls on
/// P + 9. Plant: reroll_log_hits_next_tick.
#[test]
fn under_client16635_log_tick_the_reroll_log_hits_on_its_own_tick() {
    assert_eq!(log_start_victim(RerollLogFirstHit::NextTick), (true, false), "next_tick (the vacuity check): it falls on P + 9");
    assert_eq!(log_start_victim(RerollLogFirstHit::Client16635LogTick), (false, false), "client16635_log_tick: it falls on the log's tick, P + 8");
}

/// Since parity r98 for clients 16.402 and 16.635 (next_tick before it; client 15.535.29 keeps next_tick).
#[test]
fn the_shipped_arm_is_client16635_log_tick() {
    assert_eq!(
        royalesim::state::Calib::shipped().reroll_log_first_hit,
        royalesim::state::RerollLogFirstHit::Client16635LogTick
    );
    assert_eq!(config().calib.reroll_log_first_hit, royalesim::state::RerollLogFirstHit::Client16635LogTick);
}

/// One tick of `ride_rows`: P + `k`; the Barbarian's point (native), the contact push applied to it on the tick (native)
/// and its avoidance offset; the Knight's point (native) and whether it still deploys.
#[derive(Clone, Copy, Debug)]
struct RideRow {
    k: u32,
    barb: (i32, i32),
    push: (i32, i32),
    offset: i32,
    knight: (i32, i32),
    knight_deploying: bool,
}

/// `ride_rows`' row for P + `k`.
fn ride_row(rows: &[RideRow], k: u32) -> RideRow {
    *rows.iter().find(|r| r.k == k).unwrap_or_else(|| panic!("no row P + {k}: {rows:?}"))
}

/// Under collision.REROLL_RIDE_BODY = `arm` (and spells.REROLL_HIDE_WINDOW = client16402_log_tick_to_land_plus_2): the
/// Barbarian held on (3500, 9500) and topped up, then the press (P). At the end of P + `put` a red Knight is put down
/// mid-deploy (1000 ms: it stands, deploying, through P + `put` + 19) on `kn_at`, and held there before every tick after.
/// A row per tick from P + `put` + 1 to P + `last`. The press's times, as
/// `its_press_slides_it_back_heals_it_and_rides_it_2800_on` pins them: the trigger P + 1, the log's tick P + 8, the
/// landing P + 23 on (3500, 11448).
fn ride_rows(arm: RerollRideBody, kn_at: Vec2, put: u32, last: u32) -> Vec<RideRow> {
    let (mut s, b) = barbarian_with(|c| {
        c.reroll_hide_window = RerollHideWindow::Client16402LogTickToLandPlus2;
        c.reroll_ride_body = arm;
    });
    let at = n(3500, 9500);
    for _ in 0..3 {
        assert!(s.debug_set_pos(b, at));
        s.tick();
    }
    let max = s.entity(b).expect("the Barbarian").max_hp;
    assert!(s.debug_set_hp(b, max) && s.debug_set_pos(b, at));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press, on the Barbarian");
    for _ in 0..put {
        s.tick();
    }
    assert_eq!(s.tick_count() - 1, p + put);
    let kn = s
        .scenario_spawn_copy_deploying(&[(Team::Red, "Knight", kn_at, None, None)], &[1000])
        .unwrap_or_else(|(_, e)| panic!("the Knight: {e:?}"))[0];
    let native = |v: Vec2| (v.x / K, v.y / K);
    let mut rows = Vec::new();
    while s.tick_count() - 1 < p + last {
        assert!(s.debug_set_pos(kn, kn_at), "the Knight died");
        s.tick();
        let e = s.entity(b).expect("the Barbarian");
        let kv = s.entity(kn).expect("the Knight");
        rows.push(RideRow {
            k: s.tick_count() - 1 - p,
            barb: native(e.pos),
            push: (e.push_applied.x, e.push_applied.y),
            offset: e.avoid_offset,
            knight: native(kv.pos),
            knight_deploying: kv.deploying,
        });
    }
    rows
}

/// collision.REROLL_RIDE_BODY = client16402_no_body_ride_to_land_plus_1 (clients 16.635 and 16.402, parity r99, the
/// oracle's il-mg-16635 set and its 16.402 replays: no rider pushed in 28 and 145 ride overlap ticks, nor on its landing
/// or the tick after; every overlap resolved two ticks after the landing; the rider's own offset first set there): a red
/// Knight held, deploying, 700 beside the landing point (3500, 11448), the radius sum 1000, so the rider overlaps it from
/// P + 20 (922 apart). From P + 9 to P + 24 the rider is pushed on no tick and rides on x 3500, it lands on its point,
/// scans nothing on its landing and the tick after (offset 0), and the Knight is not pushed; on P + 25 the two part.
/// Under held_body (the vacuity check) the held rider is pushed on P + 20 and scans on its landing, P + 23. Plant:
/// reroll_rider_is_body.
#[test]
fn under_client16402_no_body_ride_to_land_plus_1_the_rider_meets_no_body_through_its_landing_plus_1() {
    let kn_at = n(4200, 11448);
    let rows = ride_rows(RerollRideBody::Client16402NoBodyRideToLandPlus1, kn_at, 8, 25);
    assert!(rows.iter().all(|r| r.knight_deploying), "the scene drifted: the Knight stood up: {rows:?}");
    assert_eq!(ride_row(&rows, 23).barb, (3500, 11448), "the scene drifted: no landing on (3500, 11448) on P + 23: {rows:?}");
    for k in 9..=24 {
        let r = ride_row(&rows, k);
        assert!(
            r.barb.0 == 3500 && r.push == (0, 0) && r.knight == (4200, 11448),
            "client16402_no_body_ride_to_land_plus_1: a body on P + {k}: {r:?}"
        );
    }
    for k in [23, 24] {
        assert_eq!(ride_row(&rows, k).offset, 0, "client16402_no_body_ride_to_land_plus_1: an avoidance offset on P + {k}: {rows:?}");
    }
    let r = ride_row(&rows, 25);
    assert!(r.barb.0 < 3500 && r.knight.0 > 4200, "client16402_no_body_ride_to_land_plus_1: the overlap does not part on P + 25: {r:?}");
    // NOT VACUOUS: the held rider is pushed on its ride and scans on its landing.
    let old = ride_rows(RerollRideBody::HeldBody, kn_at, 8, 23);
    assert_ne!(ride_row(&old, 20).push, (0, 0), "held_body (the vacuity check): the rider unpushed on P + 20: {old:?}");
    assert_ne!(ride_row(&old, 23).offset, 0, "held_body (the vacuity check): no avoidance offset on its landing, P + 23: {old:?}");
}

/// collision.REROLL_RIDE_BODY = client16402_no_body_ride_to_land_plus_1 (client 16.402, parity r99, the oracle's replays:
/// 4 of 4 clean overlaps on the log's tick pushed; the start on the tick after it read off the re-roll's tag word): a
/// red Knight held, deploying, 700 beside the log's point (3500, 8648) from P + 8 on. The rider is pushed on its log's
/// tick, P + 8, and on none of P + 9 .. P + 11; under held_body (the vacuity check) it is pushed on P + 9 too. Plant:
/// reroll_rider_is_body.
#[test]
fn under_client16402_no_body_ride_to_land_plus_1_the_logs_tick_is_still_a_body() {
    let kn_at = n(4200, 8648);
    let rows = ride_rows(RerollRideBody::Client16402NoBodyRideToLandPlus1, kn_at, 7, 11);
    assert!(rows.iter().all(|r| r.knight_deploying), "the scene drifted: the Knight stood up: {rows:?}");
    assert_ne!(ride_row(&rows, 8).push, (0, 0), "client16402_no_body_ride_to_land_plus_1: no body on the log's tick, P + 8: {rows:?}");
    for k in 9..=11 {
        assert_eq!(ride_row(&rows, k).push, (0, 0), "client16402_no_body_ride_to_land_plus_1: a body on P + {k}: {rows:?}");
    }
    // NOT VACUOUS: the held rider is pushed on the tick after its log's too.
    let old = ride_rows(RerollRideBody::HeldBody, kn_at, 7, 9);
    assert_ne!(ride_row(&old, 9).push, (0, 0), "held_body (the vacuity check): the rider unpushed on P + 9: {old:?}");
}

/// Since parity r99 for clients 16.402 and 16.635 (held_body before it; client 15.535.29 keeps held_body).
#[test]
fn the_shipped_arm_is_client16402_no_body_ride_to_land_plus_1() {
    assert_eq!(
        royalesim::state::Calib::shipped().reroll_ride_body,
        royalesim::state::RerollRideBody::Client16402NoBodyRideToLandPlus1
    );
    assert_eq!(config().calib.reroll_ride_body, royalesim::state::RerollRideBody::Client16402NoBodyRideToLandPlus1);
}

/// Since parity r99 for clients 16.402 and 16.635 (client16402_log_tick_to_land_plus_2 before it; client 15.535.29
/// keeps at_trigger).
#[test]
fn the_shipped_arm_is_client16635_holders_let_go_next_tick() {
    assert_eq!(
        royalesim::state::Calib::shipped().reroll_hide_window,
        royalesim::state::RerollHideWindow::Client16635HoldersLetGoNextTick
    );
    assert_eq!(config().calib.reroll_hide_window, royalesim::state::RerollHideWindow::Client16635HoldersLetGoNextTick);
}

/// One tick of `standup_rows`: P + `k`; the Barbarian's point (native), its swing's progress and whether it deploys.
#[derive(Clone, Copy, Debug)]
struct StandRow {
    k: u32,
    at: (i32, i32),
    attack_ms: i32,
    deploying: bool,
}

/// `standup_rows`' row for P + `k`.
fn stand_row(rows: &[StandRow], k: u32) -> StandRow {
    *rows.iter().find(|r| r.k == k).unwrap_or_else(|| panic!("no row P + {k}: {rows:?}"))
}

/// The scene of `standup_rows` and `clock_rows`, under spells.REROLL_HOLD_END = `arm`: the Barbarian held on (3500, 9500)
/// and topped up, a red Knight of 5000 hitpoints held on (3500, 10700), 1200 ahead, until the Barbarian holds it with a
/// swing under way (and, with `loaded`, a load timer of 600 or more); then the press, issued on P. The battle, the
/// Barbarian, the Knight, its point and P. The press's times, as `its_press_slides_it_back_heals_it_and_rides_it_2800_on`
/// pins them: the trigger P + 1, its cast (CastTime 950) to P + 19, the cast's end P + 20, the landing P + 23.
fn pressed_on_a_knight(arm: RerollHoldEnd, loaded: bool) -> (BattleState, EntityId, EntityId, Vec2, u32) {
    let (mut s, b) = barbarian_with(|c| c.reroll_hold_end = arm);
    let (at, kn_at) = (n(3500, 9500), n(3500, 10700));
    let kn = s.scenario_spawn_now(Team::Red, "Knight", kn_at, Some(5000)).expect("the Knight");
    let max = s.entity(b).expect("the Barbarian").max_hp;
    let mut ready = false;
    for _ in 0..60 {
        assert!(
            s.debug_set_pos(b, at) && s.debug_set_hp(b, max) && s.debug_set_pos(kn, kn_at) && s.debug_set_hp(kn, 5000),
            "the scene drifted: the Knight died before the press"
        );
        s.tick();
        if s.entity(b).is_some_and(|e| e.target == Some(kn) && e.attack_ms > 0 && (!loaded || e.attack_load_ms >= 600)) {
            ready = true;
            break;
        }
    }
    assert!(ready, "the scene drifted: the Barbarian never swung at the Knight (loaded: {loaded})");
    assert!(s.debug_set_pos(b, at) && s.debug_set_pos(kn, kn_at));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press, on the Barbarian");
    (s, b, kn, kn_at, p)
}

/// Under spells.REROLL_HOLD_END = `arm`, `pressed_on_a_knight`'s scene: the Knight held on its point while it lives, and
/// left 1 hitpoint at the end of P + 5 (the re-roll's log sweeps it about P + 13). A row per tick from P + 1 to P + 50,
/// and whether the Knight was gone at the end of P + 18.
fn standup_rows(arm: RerollHoldEnd) -> (Vec<StandRow>, bool) {
    let (mut s, b, kn, kn_at, p) = pressed_on_a_knight(arm, false);
    let native = |v: Vec2| (v.x / K, v.y / K);
    let (mut rows, mut gone) = (Vec::new(), false);
    while s.tick_count() - 1 < p + 50 {
        if s.entity(kn).is_some() {
            assert!(s.debug_set_pos(kn, kn_at));
        }
        s.tick();
        let k = s.tick_count() - 1 - p;
        if k == 5 {
            assert!(s.debug_set_hp(kn, 1), "the scene drifted: the Knight died before the end of P + 5");
        }
        if k == 18 {
            gone = s.entity(kn).is_none();
        }
        let e = s.entity(b).expect("the Barbarian");
        rows.push(StandRow { k, at: native(e.pos), attack_ms: e.attack_ms, deploying: e.deploying });
    }
    (rows, gone)
}

/// spells.REROLL_HOLD_END = client16635_cast_end_wait (client 16.635 and its 16.402 replays, parity r100: 9 of 9 re-rolled
/// Barbarians whose target died in the cast with a swing under way walked on the 22nd tick after their landing, the
/// progress kept through the deploy and 0 on the 21st; 3b7dab80 t275, the engine's t274): the Knight dies in the cast;
/// the Barbarian finds the loss on its cast's end (P + 20), its wait counts P + 20 to its landing (P + 23) and stands
/// through its stand-up deploy; its swing is kept to P + 43 and zeroed on P + 44; it stands on its landing point to
/// P + 44 and walks on P + 45. The same under client16635_cast_end_clock. Under held_to_landing (the vacuity check) the
/// swing is 0 on P + 25 and it walks on P + 44. Plants: reroll_cast_decides, reroll_standup_counts.
#[test]
fn under_client16635_cast_end_wait_a_kill_in_the_cast_walks_on_the_landing_plus_22() {
    for arm in [RerollHoldEnd::Client16635CastEndWait, RerollHoldEnd::Client16635CastEndClock] {
        let (rows, gone) = standup_rows(arm);
        assert!(gone, "{arm:?}: the scene drifted: the Knight stands at the end of P + 18: {rows:?}");
        assert_eq!(rows.iter().find(|r| r.deploying).map(|r| r.k), Some(23), "{arm:?}: the scene drifted: the landing is not P + 23: {rows:?}");
        for k in 23..=43 {
            assert!(stand_row(&rows, k).attack_ms > 0, "{arm:?}: the swing not kept through the stand-up, P + {k}: {rows:?}");
        }
        assert_eq!(stand_row(&rows, 44).attack_ms, 0, "{arm:?}: the swing not zeroed on the landing + 21, P + 44: {rows:?}");
        let landed = stand_row(&rows, 23).at;
        for k in 23..=44 {
            assert_eq!(stand_row(&rows, k).at, landed, "{arm:?}: it moved before the landing + 22, P + {k}: {rows:?}");
        }
        assert_ne!(stand_row(&rows, 45).at, stand_row(&rows, 44).at, "{arm:?}: it does not walk on the landing + 22, P + 45: {rows:?}");
    }
    // NOT VACUOUS: the engine's arm finds the loss in the cast, so the wait ends early in the deploy.
    let (old, _) = standup_rows(RerollHoldEnd::HeldToLanding);
    assert_eq!(stand_row(&old, 25).attack_ms, 0, "held_to_landing (the vacuity check): the swing not zeroed on P + 25: {old:?}");
    assert_ne!(stand_row(&old, 44).at, stand_row(&old, 43).at, "held_to_landing (the vacuity check): it does not walk on P + 44: {old:?}");
}

/// One tick of `clock_rows`: P + `k`; the Barbarian's swing's progress, its load timer and whether it deploys.
#[derive(Clone, Copy, Debug)]
struct ClockRow {
    k: u32,
    attack_ms: i32,
    load: i32,
    deploying: bool,
}

/// `clock_rows`' row for P + `k`.
fn clock_row(rows: &[ClockRow], k: u32) -> ClockRow {
    *rows.iter().find(|r| r.k == k).unwrap_or_else(|| panic!("no row P + {k}: {rows:?}"))
}

/// Under spells.REROLL_HOLD_END = `arm`, `pressed_on_a_knight`'s scene pressed with a load timer of 600 or more, the Knight
/// held on its point and topped up to 5000 every tick, so it lives: a row per tick from P + 1 to P + 50, and the
/// Barbarian's LoadTime.
fn clock_rows(arm: RerollHoldEnd) -> (Vec<ClockRow>, i32) {
    let (mut s, b, kn, kn_at, p) = pressed_on_a_knight(arm, true);
    let load_time = s.cards().get(s.entity(b).expect("the Barbarian").card_idx).load_time_ms;
    let mut rows = Vec::new();
    while s.tick_count() - 1 < p + 50 {
        assert!(s.debug_set_pos(kn, kn_at) && s.debug_set_hp(kn, 5000), "the Knight died");
        s.tick();
        let e = s.entity(b).expect("the Barbarian");
        rows.push(ClockRow { k: s.tick_count() - 1 - p, attack_ms: e.attack_ms, load: e.attack_load_ms, deploying: e.deploying });
    }
    (rows, load_time)
}

/// spells.REROLL_HOLD_END = client16635_cast_end_clock (client 16.635, parity r100, the oracle's il-mg-16635 set and its
/// 16.402 replays: the swing zeroed on the cast's end in 22 of 22 with a live target or none; the load timer run 50 a tick
/// from the cast's end to the landing and stood through the stand-up deploy in 27 of 27; the first swing after it on the
/// entry's credit in 35 of 35; 3352f71b t1144 progress 500, the engine's 1300): with the Knight alive, the swing is 0 on
/// the cast's end (P + 20); the load timer l of P + 19 runs 50 a tick to the landing (P + 23) and stands to P + 43; the
/// first swing after it enters on P + 44 at LoadTime less the load timer's next value, and a tick. Under
/// client16635_cast_end_wait and held_to_landing (the vacuity check) the kept swing runs on: P + 19's progress and 50 on
/// P + 44. Plant: reroll_clock_unread.
#[test]
fn under_client16635_cast_end_clock_the_stand_up_reenters_on_the_load_timer_the_ride_left() {
    let tick = config().calib.tick_ms;
    let (rows, load_time) = clock_rows(RerollHoldEnd::Client16635CastEndClock);
    let r = |k: u32| clock_row(&rows, k);
    assert_eq!(rows.iter().find(|x| x.deploying).map(|x| x.k), Some(23), "the scene drifted: the landing is not P + 23: {rows:?}");
    let l = r(19).load;
    assert_eq!(r(20).attack_ms, 0, "client16635_cast_end_clock: the swing not started over on the cast's end, P + 20: {rows:?}");
    for k in 20..=23 {
        let want = (l - tick * (k as i32 - 19)).max(0);
        assert_eq!(r(k).load, want, "client16635_cast_end_clock: the load timer on P + {k} ({l} on P + 19): {rows:?}");
    }
    assert!(r(23).load >= 300, "the scene drifted: the ride left a load timer of {} on P + 23: {rows:?}", r(23).load);
    for k in 24..=43 {
        assert_eq!(r(k).load, r(23).load, "client16635_cast_end_clock: the load timer moved in the stand-up deploy, P + {k}: {rows:?}");
    }
    let entry = rows.iter().find(|x| x.k >= 44 && x.attack_ms > 0).map(|x| x.k);
    assert_eq!(entry, Some(44), "client16635_cast_end_clock: the first swing after the stand-up is not on P + 44: {rows:?}");
    let credit = load_time - (r(43).load - tick).max(0) + tick;
    assert_eq!(r(44).attack_ms, credit, "client16635_cast_end_clock: the first swing's entry credit on P + 44: {rows:?}");
    // NOT VACUOUS: the other arms keep the swing through the ride, and it runs on after the stand-up.
    for arm in [RerollHoldEnd::Client16635CastEndWait, RerollHoldEnd::HeldToLanding] {
        let (old, _) = clock_rows(arm);
        let want = clock_row(&old, 19).attack_ms + tick;
        assert_eq!(clock_row(&old, 44).attack_ms, want, "{arm:?} (the vacuity check): the kept swing does not run on to P + 44: {old:?}");
    }
}

/// spells.REROLL_HOLD_END ships held_to_landing: its new arms are parity r100 candidates (client 15.535.29 keeps
/// held_to_landing).
#[test]
fn the_shipped_arm_is_held_to_landing() {
    assert_eq!(Calib::shipped().reroll_hold_end, RerollHoldEnd::HeldToLanding);
    assert_eq!(config().calib.reroll_hold_end, RerollHoldEnd::HeldToLanding);
}
