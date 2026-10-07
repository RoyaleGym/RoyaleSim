//! THE SUPER ELITE ARCHER EVENT CARD (item 299; status.rs `BuffDef::switch_team`, state.rs `apply_effects` and
//! `tick_status_timers`, entity.rs `home_team`, target.rs `default_tower`): its arrow (the Magic Archer's straight shot, 1
//! damage) charms what it hits for 4000 ms: the unit fights for the other side, and gets its home side back when the charm
//! ends.
//!
//! THE MEASUREMENT (client 15.535.29, sp-event-SuperEliteArcher-s0, level 11): 4 of 4 flips (the red Knight twice, the red
//! right princess tower twice) on the frame of the hit, each 80 ticks (BuffTime 4000); the charmed Knight let the Archer
//! go on the next tick and walked for the red king, the charmed tower shot its own king; in the flip-back tick the unit
//! still acted charmed.
//!
//! WHAT IS PINNED: (1) a Blue Super Elite Archer and a red Knight in front of it, both held: the Knight turns Blue on the
//! hit's tick, 1 hitpoint down, drops the Archer as its target on the next, and is red again exactly 80 ticks after the
//! hit. (2) The arrow flies on to the red right princess tower and charms it; a blue Knight walking in that lane while the
//! tower is held takes the red king, not the tower; and a blue Knight deep in its own half on that lane, with no enemy in
//! its sight (so it walks to `default_tower`), plans its route to the red king, not to the held tower.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test super_elite_archer`):
//!   * `charm_never_flips` -- the charm changes no side: (1) and (2) go red;
//!   * `charm_never_expires` -- the side is never given back: (1) goes red;
//!   * `default_tower_ignores_charm` -- a charmed enemy tower stays a pick: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn level11() -> BattleConfig {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

const SEA: (i32, i32) = (14500, 14500);
const KNIGHT: (i32, i32) = (14500, 17500);

/// The red right princess tower.
fn red_right(s: &BattleState) -> EntityId {
    s.entities()
        .filter(|e| e.team == Team::Red && e.kind.is_crown_tower() && e.pos.x > at((9000, 0)).x && e.pos.y < at((0, 28000)).y)
        .map(|e| e.id)
        .next()
        .expect("the red right princess tower")
}

/// (1) Plants: charm_never_flips, charm_never_expires.
#[test]
fn a_charmed_knight_turns_blue_on_the_hit_and_red_80_ticks_later() {
    let mut s = BattleState::new(15, level11());
    past_deploy_lockout(&mut s);
    let sea = s.scenario_spawn_now(Team::Blue, "SuperEliteArcher", at(SEA), None).expect("the Super Elite Archer loads and stands");
    let kn = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT), None).expect("the Knight");
    let full = s.entity(kn).expect("the Knight").max_hp;
    let mut hit: Option<u32> = None;
    let mut back: Option<u32> = None;
    let mut dropped = false;
    for _ in 0..200 {
        s.debug_set_pos(sea, at(SEA));
        s.debug_set_pos(kn, at(KNIGHT));
        s.tick();
        let t = s.tick_count() - 1;
        let k = s.entity(kn).expect("the Knight");
        match hit {
            None if k.team == Team::Blue => {
                assert_eq!(k.hp, full - 1, "the arrow's 1 damage lands with the charm");
                hit = Some(t);
            }
            Some(h) if t == h + 1 => dropped = k.target != Some(sea),
            Some(_) if back.is_none() && k.team == Team::Red => back = Some(t),
            _ => {}
        }
        if back.is_some() {
            break;
        }
    }
    let h = hit.expect("the Knight was never charmed");
    assert!(dropped, "the charmed Knight kept the Archer, now an ally, on the tick after the hit");
    assert_eq!(back.map(|b| b - h), Some(80), "red again 80 ticks after the hit (BuffTime 4000)");
}

/// (2) Plants: charm_never_flips, default_tower_ignores_charm.
#[test]
fn a_walker_skips_an_enemy_tower_its_side_holds_by_a_charm() {
    let mut s = BattleState::new(15, level11());
    past_deploy_lockout(&mut s);
    let sea = s.scenario_spawn_now(Team::Blue, "SuperEliteArcher", at(SEA), None).expect("the Super Elite Archer");
    let kn = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT), None).expect("the Knight");
    let tower = red_right(&s);
    let mut walker: Option<EntityId> = None;
    let mut picks: Vec<Option<EntityId>> = Vec::new();
    // The far walker: no enemy in its sight, so its goal is `default_tower`; its route's goal end while the tower is held
    // (the route's first node: it is walked from its end).
    let mut far: Option<EntityId> = None;
    let mut far_ends: Vec<(i32, i32)> = Vec::new();
    for _ in 0..200 {
        s.debug_set_pos(sea, at(SEA));
        if s.entity(kn).is_some() {
            s.debug_set_pos(kn, at(KNIGHT));
        }
        s.tick();
        // Read while both the tower and the red Knight are charmed: the Knight, red again, would be the walker's pick.
        let held = s.entity(tower).is_some_and(|e| e.team == Team::Blue) && s.entity(kn).is_none_or(|e| e.team == Team::Blue);
        match walker {
            None if held => {
                walker = Some(s.scenario_spawn_now(Team::Blue, "Knight", at((15500, 19000)), None).expect("the walker"));
                far = Some(s.scenario_spawn_now(Team::Blue, "Knight", at((15500, 9000)), None).expect("the far walker"));
            }
            Some(w) if held => {
                picks.push(s.entity(w).and_then(|e| e.target));
                if let Some(f) = far.and_then(|f| s.entity(f)) {
                    if !f.deploying && f.target.is_none() {
                        if let Some(end) = f.route.first() {
                            far_ends.push((end.x / K, end.y / K));
                        }
                    }
                }
            }
            Some(_) => break,
            None => {}
        }
    }
    assert!(walker.is_some(), "the red right princess tower was never charmed");
    let king = s.entities().find(|e| e.team == Team::Red && e.kind == royalesim::entity::EntityKind::KingTower).map(|e| e.id);
    // Every pick it makes while the tower is held is the king, and it makes one.
    let made: Vec<Option<EntityId>> = picks.iter().copied().filter(Option::is_some).collect();
    assert!(made.len() >= 10 && made.iter().all(|p| *p == king), "the walker's picks while the tower is held: {picks:?} (king {king:?}, tower {tower:?})");
    // The far walker's goal while the tower is held: its route ends nearer the red king (9000, 29000) than the held tower.
    let d2 = |a: (i32, i32), b: (i32, i32)| i64::from(a.0 - b.0).pow(2) + i64::from(a.1 - b.1).pow(2);
    let tower_at = s.entity(tower).map(|e| (e.pos.x / K, e.pos.y / K)).expect("the red right princess tower");
    assert!(!far_ends.is_empty(), "the far walker never walked a route with no target while the tower was held");
    assert!(far_ends.iter().all(|e| d2(*e, (9000, 29000)) < d2(*e, tower_at)), "the far walker's route ends: {far_ends:?} (tower at {tower_at:?})");
}
