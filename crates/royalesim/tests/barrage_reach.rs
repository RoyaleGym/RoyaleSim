//! spells.BARRAGE_REACH: how far an Evo Cannon's barrage bomb reaches (spell.rs `impact`).
//!
//! THE LAW, measured on client 15.535.29 over every barrage in the captures: a bomb takes a ground troop whose centre lies
//! within the bomb projectile's own Radius (2000) plus the troop's collision radius, as any area's impact does under
//! spells.AOE_HIT_TEST = edge_inclusive. Radius-500 victims were hit at edge distances up to 1979 and missed from 2001; an
//! Ice Golem (radius 700) was hit 2,516 from its bomb's centre (sp-il-db5f t1878) and an Electro Spirit (radius 400)
//! missed at 2,456 (sp-m3-radius-s0 t2789). The old arm, centre_2500, reaches 2500 from the bomb to the victim's
//! centre whatever its radius, and reads both of those the other way.
//!
//! WHAT IS PINNED, on a Blue Evo Cannon's barrage (bombs on (9000, 18000), landing I + 26, and (13000, 18000), I + 28)
//! with a Red Ice Golem 2,516 behind the first and a Red Electro Spirit 2,456 behind the second, both standing in deploy
//! state through the landings:
//!   1. under data_radius_edge the Ice Golem loses hitpoints and the Electro Spirit none;
//!   2. under centre_2500 the reverse.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! barrage_reach`):
//!   * `barrage_reach_centre_2500` -- the bomb reaches 2500 centre to centre under the new arm too: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BarrageReach, BattleState, TapSnap};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// (the Ice Golem lost hitpoints, the Electro Spirit lost hitpoints) by I + 32.
fn barrage(arm: BarrageReach) -> (bool, bool) {
    let mut cfg = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    // Exact points: placement.TAP_SNAP's old arm, none (the shipped tile-centre snap would move each to its tile's centre).
    cfg.calib.placement_tap_snap = TapSnap::None;
    cfg.calib.barrage_reach = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let play = s.tick_count();
    s.spawn_unit(Team::Blue, "Cannon_EV1", n(9000, 9500), None).unwrap();
    while s.tick_count() < play + 10 {
        s.tick();
    }
    // Played on I + 10 so they stand in deploy state through both landings.
    s.spawn_unit(Team::Red, "IceGolemite", n(9000, 18000 + 2516), None).unwrap();
    s.spawn_unit(Team::Red, "ElectroSpirit", n(13000, 18000 + 2456), None).unwrap();
    s.tick();
    let golem = s.entities().find(|e| e.team == Team::Red && e.card == "IceGolemite").expect("the Ice Golem");
    let spirit = s.entities().find(|e| e.team == Team::Red && e.card == "ElectroSpirit").expect("the Electro Spirit");
    let (gid, gat, ghp) = (golem.id, golem.pos, golem.hp);
    let (sid, sat, shp) = (spirit.id, spirit.pos, spirit.hp);
    assert_eq!((gat, sat), (n(9000, 20516), n(13000, 20456)), "the scene drifted: the units did not go down where placed");
    while s.tick_count() < play + 25 {
        s.tick();
    }
    let (g, sp) = (s.entity(gid).expect("the Ice Golem"), s.entity(sid).expect("the Electro Spirit"));
    assert!(g.deploying && sp.deploying && g.pos == gat && sp.pos == sat, "the scene drifted: a unit moved or deployed before the landings");
    while s.tick_count() < play + 32 {
        s.tick();
    }
    let lost = |id, full: i32| s.entity(id).map_or(true, |e| e.hp < full);
    (lost(gid, ghp), lost(sid, shp))
}

#[test]
fn a_barrage_bomb_reaches_its_radius_plus_the_victims() {
    assert_eq!(barrage(BarrageReach::DataRadiusEdge), (true, false), "data_radius_edge: (the Ice Golem hit, the Electro Spirit hit)");
    assert_eq!(barrage(BarrageReach::Centre2500), (false, true), "centre_2500: (the Ice Golem hit, the Electro Spirit hit)");
}

#[test]
fn the_ledger_ships_data_radius_edge() {
    assert_eq!(royalesim::state::Calib::shipped().barrage_reach, BarrageReach::DataRadiusEdge);
}
