//! WHO MAY TARGET A UNIT IN A SLAP'S FLIGHT -- calibration targeting.SLAP_FLIGHT_TARGETABILITY, target.rs `can_target`,
//! state.rs `slap_air_mask`.
//!
//! THE READING (client 15.535.29): a Hero Giant's slap throws its pick 250 a step along x. On the first flight step every
//! holder that does not attack air lets it go (5 of 5: a Sparky and a Mini P.E.K.K.A on an Ice Spirit, sp-il-8b9b t880;
//! Sparkies and a Mini P.E.K.K.A on Ice Golemites, sp-il-04cb t1870 and t3350) and every holder that attacks air keeps it
//! (6 of 6: five Bats, a princess tower). A ground-only Sparky took one again after its landing (sp-il-04cb t3392).
//!
//! The scene is tests/hero_giant.rs's: the blue Hero Giant at (13000, 12000) throws a red Knight from 2,800 ahead; a blue
//! Knight (ground only) and a blue Musketeer (air and ground), held still, hold the red Knight before the throw.
//!
//! PLANT (regression): slap_flight_ground_target -> `a_ground_only_holder_lets_a_thrown_unit_go` red.
//!   RUSTFLAGS='--cfg clash_plant="slap_flight_ground_target"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test slap_flight_targets
//! PLANT (regression): slap_flight_held_past_sight -> `a_chasing_bat_lets_a_thrown_unit_go_past_its_sight_under_client15535_let_go` red.
//!
//! targeting.SLAP_FLIGHT_SIGHT_HOLD = client15535_let_go (client 15.535.29, sp-il-04cb t1885-t1899: 4 of 4 Bats chasing a
//! thrown Ice Golemite let it go on the first tick past SightRange + both radii): a thrown target is not held past it.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::fixed::isqrt;
use royalesim::state::{BattleConfig, BattleState, Calib, ChaseDropMeasure, ChaseHoldPastLimit, SlapFlightSightHold, SlapFlightTargetability};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Giant", "Knight", "Archer", "Musketeer", "MiniPekka", "HogRider", "Fireball", "Zap"];
const AT: (i32, i32) = (13000, 12000);
const RED: (i32, i32) = (13000, 14800);
const BLUE_KNIGHT: (i32, i32) = (11500, 12500);
const BLUE_MUSKETEER: (i32, i32) = (14500, 11500);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// From the first flight step on, `frames` frames: (the blue Knight holds the red one, the blue Musketeer does).
fn holds(arm: SlapFlightTargetability, frames: usize) -> Vec<(bool, bool)> {
    let mut cfg: BattleConfig = config();
    cfg.calib.slap_flight_targetability = arm;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let red = s.scenario_spawn_now(Team::Red, "Knight", n(RED), Some(100_000)).expect("the red Knight");
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", n(BLUE_KNIGHT), None).expect("the blue Knight");
    let musketeer = s.scenario_spawn_now(Team::Blue, "Musketeer", n(BLUE_MUSKETEER), None).expect("the blue Musketeer");
    s.spawn_unit(Team::Blue, "Giant_hero", n(AT), None).expect("the hero");
    s.tick();
    let giant = find_live(&s, Team::Blue, "Giant_hero")[0].id;
    let hold = |s: &mut BattleState| {
        for (id, p) in [(giant, AT), (red, RED), (knight, BLUE_KNIGHT), (musketeer, BLUE_MUSKETEER)] {
            assert!(s.debug_set_pos(id, n(p)));
        }
    };
    for _ in 0..40 {
        hold(&mut s);
        s.tick();
    }
    let on = |s: &BattleState, id: EntityId| s.entity(id).expect("a blue unit").target == Some(red);
    assert!(on(&s, knight) && on(&s, musketeer), "the scene drifted: the blue units do not hold the red Knight");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut out = Vec::new();
    let mut last = s.entity(red).expect("the red Knight").pos.x;
    for _ in 0..60 {
        for (id, p) in [(knight, BLUE_KNIGHT), (musketeer, BLUE_MUSKETEER)] {
            assert!(s.debug_set_pos(id, n(p)));
        }
        s.tick();
        let x = s.entity(red).expect("the red Knight").pos.x;
        if out.is_empty() && x - last != -250 * K {
            last = x;
            continue;
        }
        last = x;
        out.push((on(&s, knight), on(&s, musketeer)));
        if out.len() == frames {
            break;
        }
    }
    assert_eq!(out.len(), frames, "the scene drifted: the red Knight was not thrown");
    out
}

/// Plant: slap_flight_ground_target.
#[test]
fn a_ground_only_holder_lets_a_thrown_unit_go() {
    let f = holds(SlapFlightTargetability::Client15535Airborne, 5);
    assert!(!f[0].0, "the blue Knight kept the thrown red Knight on its first flight step: {f:?}");
    assert!(f.iter().all(|&(_, m)| m), "the blue Musketeer, which attacks air, let it go: {f:?}");
}

#[test]
fn the_shipped_arm_leaves_a_thrown_unit_a_ground_target() {
    assert_eq!(Calib::shipped().slap_flight_targetability, SlapFlightTargetability::Ground);
    let f = holds(SlapFlightTargetability::Ground, 5);
    assert!(f.iter().all(|&(k, m)| k && m), "the shipped arm let the thrown red Knight go: {f:?}");
}

/// Where the blue Bats wait before the throw: 4,000 to the red Knight's right, on its row.
const BATS_AT: (i32, i32) = (RED.0 + 4000, RED.1);

/// From the throw on, every flight frame and every blue Bat holding the red Knight on the frame before: (its start-of-tick
/// centre distance to the red Knight less its round sight, SightRange 5,500 + both radii, native; whether it still held the
/// red Knight after the tick), under targeting.SLAP_FLIGHT_SIGHT_HOLD = `arm` and the client's chase arms
/// (SLAP_FLIGHT_TARGETABILITY airborne, CHASE_HOLD_PAST_LIMIT troops kept, CHASE_DROP_MEASURE lane |dy|). The Bats wait
/// held at BATS_AT and chase from the throw, falling back 130 a tick on the 250 a tick flight.
fn bat_holds(arm: SlapFlightSightHold) -> Vec<(i64, bool)> {
    let mut cfg: BattleConfig = config();
    cfg.calib.slap_flight_targetability = SlapFlightTargetability::Client15535Airborne;
    cfg.calib.chase_hold_past_limit = ChaseHoldPastLimit::Client15535TroopsKept;
    cfg.calib.chase_drop_measure = ChaseDropMeasure::Client15535LaneDy;
    cfg.calib.slap_flight_sight_hold = arm;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let red = s.scenario_spawn_now(Team::Red, "Knight", n(RED), Some(100_000)).expect("the red Knight");
    s.scenario_spawn_now(Team::Blue, "Bats", n(BATS_AT), None).expect("the blue Bats");
    s.spawn_unit(Team::Blue, "Giant_hero", n(AT), None).expect("the hero");
    s.tick();
    let giant = find_live(&s, Team::Blue, "Giant_hero")[0].id;
    let bats: Vec<(EntityId, Vec2)> = find_live(&s, Team::Blue, "Bats").iter().map(|e| (e.id, e.pos)).collect();
    assert!(!bats.is_empty(), "the scene drifted: no blue Bats");
    for _ in 0..40 {
        assert!(s.debug_set_pos(giant, n(AT)) && s.debug_set_pos(red, n(RED)));
        for (id, p) in &bats {
            assert!(s.debug_set_pos(*id, *p));
        }
        s.tick();
    }
    let holding = |s: &BattleState| -> Vec<EntityId> { bats.iter().map(|b| b.0).filter(|b| s.entity(*b).is_some_and(|e| e.target == Some(red))).collect() };
    assert!(!holding(&s).is_empty(), "the scene drifted: no Bat holds the red Knight before the throw");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut out = Vec::new();
    let mut last = s.entity(red).expect("the red Knight").pos.x;
    let mut flying = false;
    for _ in 0..80 {
        let before: Vec<(EntityId, i64)> = holding(&s)
            .into_iter()
            .map(|b| {
                let (bp, rp) = (s.entity(b).expect("a Bat").pos, s.entity(red).expect("the red Knight").pos);
                let (dx, dy) = ((rp.x - bp.x) as i64, (rp.y - bp.y) as i64);
                (b, isqrt(dx * dx + dy * dy) / K as i64 - 6500)
            })
            .collect();
        s.tick();
        let x = s.entity(red).expect("the red Knight").pos.x;
        let step = x - last;
        last = x;
        if step.abs() == 250 * K {
            flying = true;
            for (b, past) in before {
                out.push((past, s.entity(b).is_some_and(|e| e.target == Some(red))));
            }
        } else if flying {
            break;
        }
    }
    assert!(flying, "the scene drifted: the red Knight was not thrown");
    out
}

/// Plant: slap_flight_held_past_sight.
#[test]
fn a_chasing_bat_lets_a_thrown_unit_go_past_its_sight_under_client15535_let_go() {
    let new = bat_holds(SlapFlightSightHold::Client15535LetGo);
    assert!(new.iter().any(|&(past, _)| past > 0), "vacuous: no Bat held the thrown Knight past its sight: {new:?}");
    assert!(new.iter().all(|&(past, held)| past <= 0 || !held), "client15535_let_go: a Bat held the thrown Knight past its sight: {new:?}");
    // NOT VACUOUS: held keeps it past sight.
    let old = bat_holds(SlapFlightSightHold::Held);
    assert!(old.iter().any(|&(past, held)| past > 0 && held), "held: no Bat held the thrown Knight past its sight: {old:?}");
}
