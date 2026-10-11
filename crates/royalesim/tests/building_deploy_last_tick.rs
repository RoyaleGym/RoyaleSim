//! combat.BUILDING_DEPLOY_LAST_TICK (target.rs `acts_on_last_deploy_tick`; decide's and phase_attack's deploy gates;
//! state.rs `hide_on_deployed`): a building acts on its deploy's last tick, and a Tesla that holds a target when its deploy
//! ends starts up.
//!
//! Measured on client 16.402 (parity r68, the ob6 live set): a building's deploy state lasts 19 frames from its first, and on
//! its first + 19 it can attack already (146 of 176 Teslas and 4 Cannons stand attacking a target there); the engine before
//! the key attacked from + 20 (41 of 47 traced buildings), its countdown ending at the end of Move.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! building_deploy_last_tick`):
//!   - building_waits_countdown -> every test red;
//!   - tesla_hides_with_target -> `under_client16402_acts_on_last_tick_a_tesla_with_a_target_at_its_deploy_end_stays_up` red;
//!   - hiders_arm_reads_every_building -> `under_client16402_hiders_only_a_cannon_waits_and_a_tesla_acts` red.
//!
//! client16402_hiders_only (parity r71, the live8 set): first attack on the deploy end E + 1 for the Cannon 541 of 568, the
//! Inferno Tower 44 of 44, the X-Bow 23 of 23, the Mortar 18 of 18, the Bomb Tower 27 of 29, the Goblin Cage 22 of 23; the
//! Tesla, the hiding building, on E 133 of 137.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::HideState;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, BuildingDeployLastTick};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// A Blue `card` played on (9000, 10000) with a Red Knight held 2500 above it (in its range through its deploy, its hp held
/// full), under combat.BUILDING_DEPLOY_LAST_TICK = `arm`: the building's first frame with a target and its hide state on
/// that frame, counted from its first frame.
fn first_target(card: &str, arm: BuildingDeployLastTick) -> (u32, HideState) {
    let mut cfg: BattleConfig = config();
    cfg.calib.building_deploy_last_tick = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let knight_at = n(9000, 12500);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", knight_at, None).expect("the Knight");
    s.spawn_unit(Team::Blue, card, n(9000, 10000), None).expect("the building");
    let mut first: Option<u32> = None;
    for _ in 0..80 {
        if let Some(full) = s.entity(knight).map(|k| k.max_hp) {
            assert!(s.debug_set_pos(knight, knight_at));
            assert!(s.debug_set_hp(knight, full));
        }
        s.tick();
        let t = s.tick_count() - 1;
        let Some((target, hide)) = find_live(&s, Team::Blue, card).first().map(|b| (b.target, b.hide_state)) else { continue };
        let f = *first.get_or_insert(t);
        if target.is_some() {
            return (t - f, hide);
        }
    }
    panic!("the {card} never targeted the Knight");
}

/// client16402_acts_on_last_tick: a Cannon whose deploy ends with a Knight in its range takes it on its first frame + 19
/// (its countdown's last tick); under after_countdown on + 20 (the vacuity check). Plant: building_waits_countdown.
#[test]
fn under_client16402_acts_on_last_tick_a_cannon_takes_its_target_on_its_deploys_last_tick() {
    let (new, _) = first_target("Cannon", BuildingDeployLastTick::Client16402ActsOnLastTick);
    let (old, _) = first_target("Cannon", BuildingDeployLastTick::AfterCountdown);
    assert_eq!(old, 20, "after_countdown: the Cannon's first target on its first frame + 20");
    assert_eq!(new, 19, "client16402_acts_on_last_tick: the Cannon's first target on its first frame + 19");
}

/// client16402_acts_on_last_tick: a Tesla whose deploy ends with a Knight in its range takes it on + 19 and stays up when
/// its countdown ends (the client's Teslas attack on their first free frame and never go under); under after_countdown it
/// goes under at its deploy end and takes the Knight a tick later. Plants: building_waits_countdown, tesla_hides_with_target.
#[test]
fn under_client16402_acts_on_last_tick_a_tesla_with_a_target_at_its_deploy_end_stays_up() {
    let mut cfg: BattleConfig = config();
    cfg.calib.building_deploy_last_tick = BuildingDeployLastTick::Client16402ActsOnLastTick;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let knight_at = n(9000, 12500);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", knight_at, None).expect("the Knight");
    s.spawn_unit(Team::Blue, "Tesla", n(9000, 10000), None).expect("the Tesla");
    let mut first: Option<u32> = None;
    let mut seen = Vec::new();
    for _ in 0..40 {
        if let Some(full) = s.entity(knight).map(|k| k.max_hp) {
            assert!(s.debug_set_pos(knight, knight_at));
            assert!(s.debug_set_hp(knight, full));
        }
        s.tick();
        let t = s.tick_count() - 1;
        let Some((target, hide)) = find_live(&s, Team::Blue, "Tesla").first().map(|b| (b.target, b.hide_state)) else { continue };
        let f = *first.get_or_insert(t);
        seen.push((t - f, hide, target.is_some()));
    }
    let at = |k: u32| seen.iter().find(|r| r.0 == k).copied().unwrap_or_else(|| panic!("no frame + {k}: {seen:?}"));
    assert!(at(19).2, "the Tesla holds the Knight on its first frame + 19: {seen:?}");
    for k in 20..=24 {
        assert_eq!((at(k).1, at(k).2), (HideState::Up, true), "+ {k}: the Tesla stays up on the Knight: {seen:?}");
    }
    let (old, _) = first_target("Tesla", BuildingDeployLastTick::AfterCountdown);
    assert!(old > 19, "after_countdown (the vacuity check): the Tesla took the Knight on + {old}");
}

/// client16402_hiders_only: the Cannon takes the Knight on its first frame + 20, as under after_countdown, and the Tesla (a
/// hiding building) on + 19, as under client16402_acts_on_last_tick. Plant: hiders_arm_reads_every_building.
#[test]
fn under_client16402_hiders_only_a_cannon_waits_and_a_tesla_acts() {
    let arm = BuildingDeployLastTick::Client16402HidersOnly;
    assert_eq!(first_target("Cannon", arm).0, 20, "client16402_hiders_only: the Cannon's first target on its first frame + 20");
    assert_eq!(first_target("Tesla", arm).0, 19, "client16402_hiders_only: the Tesla's first target on its first frame + 19");
}
