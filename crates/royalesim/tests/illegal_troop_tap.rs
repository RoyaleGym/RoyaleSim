//! A TROOP TAPPED OUTSIDE ITS TERRITORY -- calibration placement.ILLEGAL_TROOP_TAP, state.rs `resolve_point_judged`,
//! `check_position`, `bridge_into_open_lane`.
//!
//! THE READING (client 15.535.29): a troop tapped outside its territory goes down on the first legal tile back along its
//! tile column, then snapped as any tap. Oracle's tapedge scenes: Blue Skeletons tapped on the right bridge at
//! (14500 | 14499, 15000 | 15001), every tower up, played exactly as a tap at (.., 14999): the truth is identical over all
//! 62 ticks, 4 of 4 (sp-sk-souls-own-s0 t256 is the same tap). sp-il-208a: three Blue singles (an Ice Golemite, a
//! Musketeer, a Hog Rider) tapped at (8500, 20500), every tower up, were created on (8499, 14500). A tap on a bridge into
//! a lane whose enemy princess tower is down stands: sp-il-04cb t3211, Red Skeletons on the left bridge (3500, 15500),
//! Blue's left princess down since t1634, created on (3499, 15499). A tap on river water is refused (the Mirror's
//! battery, match.MIRROR_PLACEMENT). The engine's arm, refuse, refuses the play; a scenario play (`spawn_unit`) goes down
//! where it was tapped.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! illegal_troop_tap`):
//!   - troop_tap_unclamped -> `a_bridge_tap_plays_as_the_tap_behind_the_river`,
//!     `a_single_on_the_enemy_half_stands_on_the_own_front_row` and `the_play_path_takes_the_tap` red;
//!   - bridge_tap_clamped -> `a_bridge_tap_into_an_open_lane_stands` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, DeployError, IllegalTroopTap};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const CLAMP: IllegalTroopTap = IllegalTroopTap::Client15535ClampToLegalEdge;

fn battle(arm: IllegalTroopTap) -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.calib.illegal_troop_tap = arm;
    let deck: Vec<String> = ["Knight", "Skeletons", "Archers", "Giant", "Musketeer", "Fireball", "Arrows", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    let mut s = BattleState::new(3, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// Where `card`'s units of `team` stand on the tick after a scenario play at `tap` (native), sorted.
fn laid(arm: IllegalTroopTap, team: Team, card: &str, unit: &str, tap: (i32, i32), setup: impl Fn(&mut BattleState)) -> Vec<(i32, i32)> {
    let mut s = battle(arm);
    setup(&mut s);
    s.spawn_unit(team, card, n(tap.0, tap.1), None).expect("the play");
    s.tick();
    s.tick();
    let mut v: Vec<(i32, i32)> = find_live(&s, team, unit).iter().map(|e| (e.pos.x / K, e.pos.y / K)).collect();
    v.sort();
    assert!(!v.is_empty(), "nothing of {unit} stands");
    v
}

/// Plant: troop_tap_unclamped.
#[test]
fn a_bridge_tap_plays_as_the_tap_behind_the_river() {
    let behind = laid(CLAMP, Team::Blue, "Skeletons", "Skeletons", (14500, 14999), |_| {});
    for tap in [(14500, 15000), (14499, 15001)] {
        assert_eq!(laid(CLAMP, Team::Blue, "Skeletons", "Skeletons", tap, |_| {}), behind, "the bridge tap {tap:?}");
    }
    assert_ne!(laid(IllegalTroopTap::Refuse, Team::Blue, "Skeletons", "Skeletons", (14500, 15000), |_| {}), behind, "vacuous: the old arm lays it there too");
}

/// sp-il-208a: a single tapped deep in the enemy half, every tower up, is created on the own front row of its column.
/// Plant: troop_tap_unclamped.
#[test]
fn a_single_on_the_enemy_half_stands_on_the_own_front_row() {
    assert_eq!(laid(CLAMP, Team::Blue, "Knight", "Knight", (8500, 20500), |_| {}), vec![(8499, 14500)]);
    assert_eq!(laid(CLAMP, Team::Red, "Knight", "Knight", (9500, 11500), |_| {}), laid(CLAMP, Team::Red, "Knight", "Knight", (9500, 17500), |_| {}), "Red, mirrored");
}

/// sp-il-04cb t3211: a bridge tap into a lane whose enemy princess tower is down stands where it was tapped, as the old
/// arm lays it. Plant: bridge_tap_clamped.
#[test]
fn a_bridge_tap_into_an_open_lane_stands() {
    let open = |s: &mut BattleState| {
        let left = s.tower_ids(Team::Blue)[1].expect("Blue's left princess");
        assert!(s.debug_set_hp(left, 0));
        s.tick();
    };
    let clamped = laid(CLAMP, Team::Red, "Skeletons", "Skeletons", (3500, 15500), open);
    assert_eq!(clamped, laid(IllegalTroopTap::Refuse, Team::Red, "Skeletons", "Skeletons", (3500, 15500), open));
    assert_ne!(clamped, laid(CLAMP, Team::Red, "Skeletons", "Skeletons", (3500, 15500), |_| {}), "vacuous: the closed lane's tap stands there too");
}

/// The play path: the clamp arm takes an enemy-half and a bridge tap the old arm refuses; river water stays refused.
/// Plant: troop_tap_unclamped.
#[test]
fn the_play_path_takes_the_tap() {
    assert_eq!(Calib::shipped().illegal_troop_tap, IllegalTroopTap::Refuse);
    for (arm, want) in [(CLAMP, Ok(())), (IllegalTroopTap::Refuse, Err(DeployError::OutOfTerritory))] {
        let mut s = battle(arm);
        s.scenario_set_elixir_milli(Team::Blue, 10000);
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "Knight").expect("the Knight in Blue's hand");
        assert_eq!(s.check_deploy_slot(Team::Blue, slot, n(8500, 20500)), want, "{arm:?}: the enemy half");
        assert_eq!(s.check_deploy_slot(Team::Blue, slot, n(14500, 15001)), want, "{arm:?}: the bridge");
        assert_eq!(s.check_deploy_slot(Team::Blue, slot, n(9500, 15500)), Err(DeployError::Water), "{arm:?}: river water");
    }
}
