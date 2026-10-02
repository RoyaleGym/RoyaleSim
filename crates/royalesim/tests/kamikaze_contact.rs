//! A DYING KAMIKAZE PUSHES NOBODY (calibration movement.KAMIKAZE_DEATH_CONTACT, under movement.DYING_UNIT_VISIBILITY =
//! client_doomed_static; state.rs `phase_path16402_for`, `kamikaze_unpushing`). Measured on client 15.535.29: every ground
//! troop overlapping a kamikaze that dies at its own attack, its position exact on the tick before, stands where the
//! engine with the dying kamikaze out of the separation puts it on the death tick, 18 of 18 (sp-esk-bank t306: a
//! Skeleton 681 from a dying Fire Spirit moves (106, 57), into its place); its body stays in the avoidance scans
//! (sp-rage-4000-s0 t257: a Skeleton steers round the dying Electro Spirit).
//!
//! The scene: Blue's Battle Ram walks at the red right princess tower with an own Skeleton held 900 to its right (inside
//! the two radii, 750 + 500). On the tick its own hit kills it, the Skeleton takes no push under
//! client15535_avoided_not_pushed, and the Ram's static body pushes it under as_doomed.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! kamikaze_contact`): dying_kamikaze_pushes -> `a_dying_kamikaze_pushes_nobody` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DyingUnitVisibility, KamikazeDeathContact};
use royalesim::Team;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The push the Skeleton took on the tick the Ram's own hit killed it, native units.
fn push_on_death_tick(arm: KamikazeDeathContact) -> (i32, i32) {
    let mut cfg = config();
    let deck: Vec<String> = ["BattleRam", "Skeletons", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dying_unit_visibility = DyingUnitVisibility::ClientDoomedStatic;
    cfg.calib.kamikaze_death_contact = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let ram = s.scenario_spawn_now(Team::Blue, "BattleRam", at(14800, 21000), None).expect("the Battle Ram");
    let skel = s.scenario_spawn_now(Team::Blue, "Skeleton", at(15700, 21000), None).expect("the Skeleton");
    for _ in 0..200 {
        let Some(r) = s.entity(ram) else { break };
        let rp = r.pos;
        assert!(s.debug_set_pos(skel, Vec2::new(rp.x + 900 * K, rp.y)));
        s.tick();
        if s.entity(ram).is_none() {
            let k = s.entity(skel).expect("the Skeleton lives");
            return (k.push_applied.x, k.push_applied.y);
        }
    }
    panic!("the scene drifted: the Ram never died");
}

#[test]
fn a_dying_kamikaze_pushes_nobody() {
    assert_eq!(push_on_death_tick(KamikazeDeathContact::Client15535AvoidedNotPushed), (0, 0), "the dying Ram pushed the Skeleton");
}

#[test]
fn as_doomed_pushes_with_the_dying_kamikaze() {
    // the control: the old arm's doomed Ram is a static body, and the Skeleton held inside its radius is pushed
    assert_ne!(push_on_death_tick(KamikazeDeathContact::AsDoomed), (0, 0), "the Skeleton took no push from the Ram's static body here");
}
