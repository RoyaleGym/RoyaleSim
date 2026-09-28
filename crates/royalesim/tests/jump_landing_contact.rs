//! movement.JUMP_LANDING_CONTACT, read off the engine: whether a unit landing from a river jump is a contact body for
//! the units the move pass updates after it on its landing tick L (state.rs `phase_path16402_for`, the leap's landing).
//! Shipped at its old arm landing_tick; client_next_tick keeps the lander out of L's contact pass.
//!
//! THE EVIDENCE (the ledger has the rows): on client 15.535.29 (sweep-RoyalHogs t291) and client 16.402
//! (20260920-002736, both seats, t545) a Hog lands overlapping or beside another; neither is pushed on L and the pair
//! push apart on L + 1, where the engine pushes the neighbour on L.
//!
//! THE SCENE. A blue Hog Rider on the river centre (9500, 12500), the jump fixture's placement
//! (tests/fixtures/oracle2026/client16402_jumps.json), leaps and lands; a probe run reads its landing tick L and point.
//! The measured run puts a blue Knight on that point on L - 1 (after the Hog, so the pass updates it after the Hog),
//! and reads the Knight's push on L and L + 1. The Hog leaps the same in both runs: a leaping unit is out of the
//! contact pass.
//!
//! WHAT IS PINNED, and the plant that turns it red (`RUSTFLAGS='--cfg clash_plant="lander_collides_on_landing"'
//! CARGO_TARGET_DIR=target/plant cargo test --profile gate --test jump_landing_contact`):
//!   1. `a_lander_is_no_contact_body_on_its_landing_tick_under_client_next_tick`: under client_next_tick the Knight
//!      takes no push on L and the pair touch on L + 1; under landing_tick the Knight is pushed on L
//!      -- lander_collides_on_landing (the lander is a contact body on L under both arms);
//!   2. `landing_tick_ships`.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, JumpLandingContact};
use royalesim::{EntityId, Team};

const HOG_AT: (i32, i32) = (9500, 12500);

fn with_arm(arm: JumpLandingContact) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.jump_landing_contact = arm;
    cfg
}

/// The Hog put down, and the tick count at which its leap ends (the first tick after one it leapt on whose state shows
/// it landed), with its landing point.
fn probe(arm: JumpLandingContact) -> (u32, Vec2) {
    let mut s = BattleState::new(3, with_arm(arm));
    let hog = s.scenario_spawn_now(Team::Blue, "HogRider", Vec2::new(HOG_AT.0 * K, HOG_AT.1 * K), None).expect("the Hog");
    let mut leapt = false;
    for n in 1..300u32 {
        s.tick();
        let e = s.entity(hog).expect("the Hog is gone");
        if e.jumping {
            leapt = true;
        } else if leapt {
            return (n, e.pos);
        }
    }
    panic!("the Hog never leapt and landed from the river centre (leapt: {leapt})");
}

/// The measured run: the Knight on the landing point from L - 1; its push on L and on L + 1.
fn knight_pushes(arm: JumpLandingContact) -> (Vec2, Vec2) {
    let (landing, at) = probe(arm);
    let mut s = BattleState::new(3, with_arm(arm));
    let hog = s.scenario_spawn_now(Team::Blue, "HogRider", Vec2::new(HOG_AT.0 * K, HOG_AT.1 * K), None).expect("the Hog");
    for _ in 1..landing {
        s.tick();
    }
    assert!(s.entity(hog).is_some_and(|e| e.jumping), "the Hog is not in the air on L - 1");
    let knight: EntityId = s.scenario_spawn_now(Team::Blue, "Knight", at, None).expect("the Knight");
    s.tick();
    let h = s.entity(hog).expect("the Hog on L");
    assert!(!h.jumping, "the Knight moved the Hog's landing: it is still in the air on L");
    assert_eq!(h.pos, at, "the Knight moved the Hog's landing point");
    let on_l = s.entity(knight).expect("the Knight on L").push_applied;
    s.tick();
    let on_next = s.entity(knight).expect("the Knight on L + 1").push_applied;
    (on_l, on_next)
}

#[test]
fn a_lander_is_no_contact_body_on_its_landing_tick_under_client_next_tick() {
    let (new_l, new_next) = knight_pushes(JumpLandingContact::NextTick);
    assert_eq!(new_l, Vec2::default(), "client_next_tick: the Knight is pushed on the landing tick");
    // NOT VACUOUS: the old arm pushes the Knight on L, so the Knight stands inside the lander's contact.
    let (old_l, _) = knight_pushes(JumpLandingContact::LandingTick);
    assert_ne!(old_l, Vec2::default(), "landing_tick: the Knight takes no push on L; the scene does not overlap");
    // And under the new arm the pair do meet, one tick later.
    assert_ne!(new_next, Vec2::default(), "client_next_tick: the Knight takes no push on L + 1 either");
}

#[test]
fn landing_tick_ships() {
    assert_eq!(Calib::shipped().jump_landing_contact, JumpLandingContact::LandingTick, "movement.JUMP_LANDING_CONTACT ships landing_tick");
    assert_eq!(config().calib.jump_landing_contact, JumpLandingContact::LandingTick);
}
