//! spawner.SPECTRAL_BIRTH_POINT: where an Evo Skeleton Army Spectral is made against its soldier's death point Q (state.rs
//! `army_spectrals`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, a Spectral's first frame's x2/y2 (its birth point) is Q + (1, 0)
//! for the 8 whose Q lies on a WATER half-tile cell and Q for the 33 over land (sp-esa-spectrals-s0 64, 65, 67, 73;
//! sp-form-SkeletonArmy-evo-s0 70, 72, 74; sp-il-b5e2 129).
//!
//! THE SCENE (tests/spectral_first_update.rs's army, 31 ticks on): one soldier moved into the river at (9250, 16250)
//! (cell (18, 32), water) or onto land at (5000, 11900), set to 0 hp; it dies in the next tick's Reap. Under
//! spawner.SPECTRAL_FIRST_UPDATE = none the Spectral stands where it was made. WHAT IS PINNED:
//!   1. soldier_point (the engine's, the vacuity check): the Spectral over water stands on a water cell, Q;
//!   2. client15535_water_nudge: it stands 1 native unit right of soldier_point's point; over land, on the same point.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="spectral_born_on_water_point"' CARGO_TARGET_DIR=target/plant cargo test --test
//! spectral_birth_point`): the new arm makes it on the death point: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, SpectralBirthPoint, SpectralFirstUpdate};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The point the Spectral is made on under `arm`, its soldier moved to `at` and killed, and whether its cell is water.
fn made_at(arm: SpectralBirthPoint, at: Vec2) -> (Vec2, bool) {
    let mut cfg: BattleConfig = config();
    cfg.calib.spectral_first_update = SpectralFirstUpdate::None;
    cfg.calib.spectral_birth_point = arm;
    cfg.decks = [vec!["SkeletonArmy".into(), "Knight".into()], vec!["Knight".into(), "Zap".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "SkeletonArmy_EV1", n(9500, 11500), None).expect("the play");
    for _ in 0..31 {
        s.tick();
    }
    let victim = find_live(&s, Team::Blue, "SkeletonArmy_EV1")[0].id;
    assert!(s.debug_set_pos(victim, at));
    assert!(s.debug_set_hp(victim, 0));
    s.tick();
    assert!(s.entity(victim).is_none(), "{arm:?}: the scene drifted: the soldier lives");
    let sp = find_live(&s, Team::Blue, "SkeletonArmy_EV1_Spectral");
    assert_eq!(sp.len(), 1, "{arm:?}: the scene drifted: one Spectral on the frame the soldier is first gone");
    let p = sp[0].pos;
    let a = &s.config().arena;
    (p, a.cell_bits(p.x.div_euclid(a.cell), p.y.div_euclid(a.cell)) & a.bit_water != 0)
}

/// Plant: spectral_born_on_water_point.
#[test]
fn a_spectral_whose_soldier_died_over_water_is_made_one_to_its_right_under_client15535_water_nudge() {
    let (q, wet) = made_at(SpectralBirthPoint::SoldierPoint, n(9250, 16250));
    // NOT VACUOUS: the soldier died over water.
    assert!(wet, "soldier_point: the scene drifted: the soldier's point {q:?} is not over water");
    let (b, _) = made_at(SpectralBirthPoint::Client15535WaterNudge, n(9250, 16250));
    assert_eq!(b, q.add(Vec2::new(K, 0)), "client15535_water_nudge: the Spectral over water is not made 1 right of {q:?}");
    let (q, wet) = made_at(SpectralBirthPoint::SoldierPoint, n(5000, 11900));
    assert!(!wet, "soldier_point: the scene drifted: the land soldier's point {q:?} is over water");
    let (b, _) = made_at(SpectralBirthPoint::Client15535WaterNudge, n(5000, 11900));
    assert_eq!(b, q, "client15535_water_nudge: the Spectral over land moved off its soldier's point");
}
