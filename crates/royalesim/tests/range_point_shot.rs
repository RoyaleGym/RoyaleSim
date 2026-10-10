//! THE RANGE POINT SHOT (combat.RANGE_POINT_SHOT; card.rs `CardDef::range_point`, combat.rs `fire`): a troop's shot whose
//! row carries a ProjectileRange and no ProjectileRadius (the Wall Breakers' WallbreakerProjectile, range 1) is aimed that
//! far from the attacker's own point toward its target and lands there on the tick after the fire.
//!
//! THE MEASUREMENTS (parity's r64 census, client 16.402 ob3 live set; client 15.535.29 sp-f2-wb-s0): every Wall Breaker
//! blast on a building lands the tick after the fire (the tick the Wall Breaker is gone), 62 of 62, on its own point; the
//! engine aimed it at the target's centre and landed it a tick later.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test range_point_shot`):
//!   - range_point_aims_target -> `a_wall_breaker_s_blast_lands_the_tick_after_its_fire` red;
//!   - range_point_unread -> `a_wall_breaker_s_blast_lands_the_tick_after_its_fire` and `only_the_wall_breakers_carry_a_range_point` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, RangePointShot};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// A lone Wall Breaker walking into Red's left princess tower under `arm` (its partner taken off at once, its hitpoints
/// held so the tower's shots cannot stop it): (F, the first tick it is gone; D, the first tick the tower has lost hitpoints).
fn blast(arm: RangePointShot) -> (u32, u32) {
    let mut cfg = config();
    cfg.calib.range_point_shot = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "Wallbreakers", n(3500, 22500), None).expect("the Wall Breakers");
    s.tick();
    let wbs: Vec<_> = find_live(&s, Team::Blue, "Wallbreakers").iter().map(|e| e.id).collect();
    assert_eq!(wbs.len(), 2, "the pair");
    assert!(s.debug_set_pos(wbs[1], n(15000, 3000)));
    assert!(s.debug_set_hp(wbs[1], 0));
    let full = s.tower_hp(Team::Red)[1];
    let (mut gone, mut hit) = (None, None);
    for _ in 0..400 {
        let _ = s.debug_set_hp(wbs[0], 300);
        s.tick();
        if gone.is_none() && s.entity(wbs[0]).is_none() {
            gone = Some(s.tick_count());
        }
        if hit.is_none() && s.tower_hp(Team::Red)[1] < full {
            hit = Some(s.tick_count());
        }
        if gone.is_some() && hit.is_some() {
            break;
        }
    }
    (gone.expect("the Wall Breaker never went"), hit.expect("the tower never lost hitpoints"))
}

/// combat.RANGE_POINT_SHOT = client_range_point: the tower loses its hitpoints the tick after the Wall Breaker fires (F + 1);
/// under to_target (the vacuity check) a tick later, the shot flown to the tower's centre. Plants: range_point_aims_target,
/// range_point_unread.
#[test]
fn a_wall_breaker_s_blast_lands_the_tick_after_its_fire() {
    let (f, d) = blast(RangePointShot::ClientRangePoint);
    assert_eq!(d, f + 1, "client_range_point: the blast not on F + 1 (F {f})");
    let (f, d) = blast(RangePointShot::ToTarget);
    assert_eq!(d, f + 2, "to_target: the blast not on F + 2 (F {f})");
}

/// The loader reads the range point off the Wall Breakers' row (range 1) and off no straight shot to a range (a range and a
/// radius) and no ordinary shot. Plant: range_point_unread.
#[test]
fn only_the_wall_breakers_carry_a_range_point() {
    let s = BattleState::new(7, config());
    let get = |name: &str| s.cards().get(s.cards().index(name).unwrap_or_else(|| panic!("{name}"))).range_point;
    assert!(get("Wallbreakers").is_some_and(|r| r > 0), "the Wall Breakers' range point");
    for name in ["Musketeer", "Archer", "Princess", "Bowler"] {
        assert_eq!(get(name), None, "{name} carries a range point");
    }
}
