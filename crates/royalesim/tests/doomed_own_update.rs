//! A DOOMED TROOP STAYS WHERE THE TICK FOUND IT (calibration movement.DOOMED_OWN_UPDATE, under
//! movement.DYING_UNIT_VISIBILITY = client_doomed_static; state.rs `phase_path16402_for`, `doomed_stays`). Measured on
//! client 15.535.29: a Battle Ram killed by its own hit lays its two Barbarians centred exactly on its last point, 29 of
//! 29, where the engine pushed it first (sp-ram-alone-s0 t309: (7, 3) by an own Skeleton's overlap).
//!
//! The scene: Blue's Battle Ram walks at the red right princess tower with an own Skeleton held against its side every
//! tick (the push): 1,150 to its right, inside the two radii (750 + 500) by 100, and about 1,300 from either of the
//! Barbarians it leaves (500 + 500), so the Skeleton pushes the Ram and never a newborn Barbarian. On the tick its own hit kills it, its Barbarians' centroid is, under client15535_skipped, its last
//! point; under walks it is pushed off it first.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! doomed_own_update`): doomed_own_update_kept -> `a_ram_killed_by_its_own_hit_lays_its_barbarians_on_its_last_point` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DoomedOwnUpdate, DyingUnitVisibility};
use royalesim::Team;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// (the Ram's last point, the centroid of the Barbarians born on the tick it died), native units.
fn ram_death(arm: DoomedOwnUpdate) -> ((i32, i32), (i32, i32)) {
    let mut cfg = config();
    let deck: Vec<String> = ["BattleRam", "Skeletons", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dying_unit_visibility = DyingUnitVisibility::ClientDoomedStatic;
    cfg.calib.doomed_own_update = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let ram = s.scenario_spawn_now(Team::Blue, "BattleRam", at(14800, 21000), None).expect("the Battle Ram");
    let skel = s.scenario_spawn_now(Team::Blue, "Skeleton", at(15200, 21000), None).expect("the Skeleton");
    let mut last = None;
    for _ in 0..200 {
        let Some(r) = s.entity(ram) else { break };
        let rp = r.pos;
        last = Some((rp.x / K, rp.y / K));
        // the push: the Skeleton held against the Ram's side, clear of where its Barbarians will stand
        if s.entity(skel).is_some() {
            assert!(s.debug_set_pos(skel, Vec2::new(rp.x + 1150 * K, rp.y)));
        }
        s.tick();
        if s.entity(ram).is_none() {
            let born: Vec<(i32, i32)> = s
                .entities()
                .filter(|e| e.team == Team::Blue && e.card == "Barbarian")
                .map(|e| (e.pos.x / K, e.pos.y / K))
                .collect();
            assert_eq!(born.len(), 2, "the scene drifted: the Ram left {} Barbarians", born.len());
            let c = ((born[0].0 + born[1].0) / 2, (born[0].1 + born[1].1) / 2);
            return (last.expect("the Ram lived a tick"), c);
        }
    }
    panic!("the scene drifted: the Ram never died");
}

#[test]
fn a_ram_killed_by_its_own_hit_lays_its_barbarians_on_its_last_point() {
    let (last, centre) = ram_death(DoomedOwnUpdate::Client15535Skipped);
    assert!((centre.0 - last.0).abs() <= 1 && (centre.1 - last.1).abs() <= 1, "the Barbarians' centre {centre:?}, the Ram's last point {last:?}");
}

#[test]
fn walks_pushes_the_doomed_ram_first() {
    // the control: the old arm moves the doomed Ram on its death tick, so the centre leaves its last point
    let (last, centre) = ram_death(DoomedOwnUpdate::Walks);
    assert!((centre.0 - last.0).abs() > 1 || (centre.1 - last.1).abs() > 1, "the push did not move the doomed Ram here ({centre:?} on {last:?})");
}
