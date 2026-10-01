//! TWO LAWS ON A TICK'S EDGE (state.rs `phase_path16402_for`'s `fallen`, `phase_target`'s carry).
//!
//! (1) A BUILDING WHOSE DEATH IS SETTLED BEFORE THE MOVE PASS takes part in no scan that tick: a troop overlapping it is
//! not pushed by it on the tick it falls. Read off client 15.535.29 by Oracle (sp-hogs-musk-s0 t641). Client 15.535.29's
//! only: it runs under movement.DYING_UNIT_VISIBILITY = client_doomed_static, and the 16.402 corpus counts the building
//! under the shipped whole_tick (the test pins both). Under match.TICK_ORDER = client_sequential_strike, whose strike lands
//! in the striker's own turn and leaves no hit in the buffer, the building has fallen all the same.
//! The scene: a blue Knight held 900 from a red Cannon at 150 hitpoints, one blow (out of every tower's reach), attacking it. While the Cannon stands, its body
//! pushes the Knight every tick; on the tick the Knight's blow kills it, the Knight meets no one.
//!
//! (2) A PAUSED WINDUP CARRIES ONTO THE RESUME RESCAN'S TARGET ONLY IN REACH (status.RESUME_RETARGET_WINDUP = carry, as
//! combat.CORPSE_SWITCH_REACH): measured on client 15.535.29 (sp-il-8e134db4 t545).
//! The scene: a blue Knight mid-swing on a red Knight is held by a red Freeze; its Knight dies under the hold; the only
//! enemy left, a red Giant held 5,000 off, is out of reach. On the tick the hold ends and he takes the Giant, his
//! progress is 0, and he walks.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! fall_and_resume`): doomed_building_collides -> `a_troop_on_a_building_that_falls_this_tick_is_not_pushed_by_it` red;
//! struck_building_collides -> the same red (its sequential-order half);
//! resume_carry_out_of_reach -> `a_windup_paused_by_a_freeze_is_dropped_when_the_new_target_is_out_of_reach` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DyingUnitVisibility, TickOrder};
use royalesim::Team;

const DECK: [&str; 8] = ["Knight", "Giant", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const RED: [&str; 8] = ["Cannon", "Knight", "Giant", "Freeze", "Fireball", "Arrows", "Minions", "Zap"];

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn battle() -> BattleState {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), RED.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s
}

#[test]
fn a_troop_on_a_building_that_falls_this_tick_is_not_pushed_by_it() {
    let (met, standing) = fall(DyingUnitVisibility::ClientDoomedStatic, TickOrder::Client16402);
    assert!(standing >= 3, "the scene drifted: the Cannon pushed the Knight on {standing} ticks before it fell");
    assert_eq!(met, 0, "client 15.535.29's arm: the tick the Cannon falls, the Knight met it");
    let (met, standing) = fall(DyingUnitVisibility::ClientDoomedStatic, TickOrder::ClientSequentialStrike);
    assert!(standing >= 3, "the scene drifted under the sequential order: the Cannon pushed the Knight on {standing} ticks");
    assert_eq!(met, 0, "client 15.535.29's arm under the sequential order: the Knight's blow felled the Cannon in its turn, and the Knight met it");
    let (met, _) = fall(DyingUnitVisibility::WholeTick, TickOrder::ClientSequentialStrike);
    assert!(met >= 1, "the control: whole_tick under the sequential order still meets the Cannon on its fall tick ({met})");
    let (met, _) = fall(DyingUnitVisibility::WholeTick, TickOrder::Client16402);
    assert!(met >= 1, "the shipped whole_tick (16.402's): the Cannon still meets the Knight on the tick it falls");
}

/// The fall scene under `arm` and `order`: (bodies the Knight met on the tick the Cannon fell, ticks the Cannon pushed it
/// before).
fn fall(arm: DyingUnitVisibility, order: TickOrder) -> (i32, i32) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), RED.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dying_unit_visibility = arm;
    cfg.calib.tick_order = order;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", n((9000, 14200)), None).expect("the Cannon");
    let knight_at = n((9000, 13300));
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", knight_at, None).expect("the Knight");
    let (rk, rc) = (s.entity(knight).unwrap().radius / K, s.entity(cannon).unwrap().radius / K);
    assert!(900 < rc + rk.min(500), "the scene drifted: the Knight does not overlap the Cannon's body ({rc} + {rk})");
    assert!(s.debug_set_hp(cannon, 150), "the Cannon at 150, one Knight blow");
    let mut met_while_standing = 0;
    for _ in 0..80 {
        assert!(s.debug_set_pos(knight, knight_at));
        s.tick();
        let k = s.entity(knight).expect("the Knight lives");
        if s.entity(cannon).is_none() {
            return (k.push_neighbours, met_while_standing);
        }
        if k.push_neighbours >= 1 {
            met_while_standing += 1;
        }
    }
    panic!("the scene drifted: the Knight never killed the Cannon");
}

#[test]
fn a_windup_paused_by_a_freeze_is_dropped_when_the_new_target_is_out_of_reach() {
    let mut s = battle();
    let blue = s.scenario_spawn_now(Team::Blue, "Knight", n((9000, 12000)), None).expect("the blue Knight");
    let red = s.scenario_spawn_now(Team::Red, "Knight", n((9000, 13600)), None).expect("the red Knight");
    let giant_at = n((9000, 17600));
    let giant = s.scenario_spawn_now(Team::Red, "Giant", giant_at, None).expect("the Giant");
    let hold = |s: &mut BattleState| {
        let _ = s.debug_set_pos(giant, giant_at);
    };
    // His swing under way on the red Knight: progress past a multiple of HitSpeed by more than a tick.
    let hs = 1200;
    let mut under_way = false;
    for _ in 0..120 {
        hold(&mut s);
        s.tick();
        let b = s.entity(blue).expect("the blue Knight");
        if b.target == Some(red) && b.attack_phase == AttackPhase::Windup && b.attack_ms % hs > 300 {
            under_way = true;
            break;
        }
    }
    assert!(under_way, "the scene drifted: the blue Knight's swing was never under way on the red Knight");
    s.spawn_unit(Team::Red, "Freeze", n((9000, 12000)), None).expect("the Freeze");
    // Under the hold: the red Knight dies, and the blue Knight's progress stands.
    let mut frozen_at = None;
    let mut resumed = None;
    for k in 0..200 {
        hold(&mut s);
        if k == 10 {
            assert!(s.debug_set_hp(red, 0), "the red Knight is alive to kill");
        }
        let before = s.entity(blue).expect("the blue Knight").pos;
        s.tick();
        let b = s.entity(blue).expect("the blue Knight lives");
        if k == 5 {
            frozen_at = Some(b.attack_ms);
        }
        if k > 12 && b.target == Some(giant) {
            resumed = Some((k, b.attack_ms, before));
            break;
        }
    }
    let (k, progress, before) = resumed.expect("the scene drifted: he never took the Giant after the hold");
    assert!(frozen_at.is_some_and(|p| p % hs > 50), "the scene drifted: no swing was paused under the hold ({frozen_at:?})");
    assert_eq!(progress, 0, "the hold's end (row {k}): the paused windup is dropped for the Giant out of reach");
    let mut walked = false;
    let mut last = before;
    for _ in 0..4 {
        hold(&mut s);
        s.tick();
        let p = s.entity(blue).expect("the blue Knight").pos;
        walked |= p != last;
        last = p;
    }
    assert!(walked, "he walks at the Giant once the windup is dropped");
}
