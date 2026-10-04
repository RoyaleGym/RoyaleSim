//! spawner.SPECTRAL_PARENT_BLOCKER: whether an Evo Skeleton Army Spectral's first update (spawner.SPECTRAL_FIRST_UPDATE =
//! client15535_same_tick) meets the tick's dying soldiers as static blockers (state.rs `army_spectrals`, `first_update`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, the Spectrals born walking turn on their first frame
//! (avoidance offset +-190) where the engine's walk straight (14 of 16 in sp-esa-spectrals-s0 and
//! sp-form-SkeletonArmy-evo-s0); the scan meeting its dying soldier as a static body gives the client's sign on 30 of 37.
//!
//! THE SCENE (tests/spectral_first_update.rs's): the army 31 ticks on, one soldier put 4,000 left of its point (clear of
//! the live soldiers, whom the Spectral's scan meets under either arm: in the army it turns under none too) and set to 0
//! hp dies in the next tick's Reap and leaves a Spectral, a red Knight 3,000 above that point in its sight. WHAT IS
//! PINNED:
//!   1. client15535_static: on its first frame the Spectral's avoidance offset is turned (not 0);
//!   2. none (the old arm, the vacuity check): it is 0;
//!   3. the shipped value is none (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test spectral_parent_blocker`):
//!   * `spectral_parent_unseen` -- the new arm's first update still meets none of them: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, SpectralFirstUpdate, SpectralParentBlocker};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Spectral's avoidance offset on its first frame under `arm`.
fn first_offset(arm: SpectralParentBlocker) -> i32 {
    let mut cfg: BattleConfig = config();
    cfg.calib.spectral_first_update = SpectralFirstUpdate::Client15535SameTick;
    cfg.calib.spectral_parent_blocker = arm;
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
    let (victim, at) = {
        let v = find_live(&s, Team::Blue, "SkeletonArmy_EV1");
        (v[0].id, v[0].pos.sub(Vec2::new(4000 * K, 0)))
    };
    assert!(s.debug_set_pos(victim, at));
    s.scenario_spawn_now(Team::Red, "Knight", Vec2::new(at.x, at.y + 3000 * K), None).expect("a red Knight in the Spectral's sight");
    assert!(s.debug_set_hp(victim, 0));
    s.tick();
    assert!(s.entity(victim).is_none(), "{arm:?}: the scene drifted: the soldier lives");
    let sp = find_live(&s, Team::Blue, "SkeletonArmy_EV1_Spectral");
    assert_eq!(sp.len(), 1, "{arm:?}: the scene drifted: one Spectral on the frame the soldier is first gone");
    sp[0].avoid_offset
}

/// Plant: spectral_parent_unseen.
#[test]
fn a_spectral_turns_round_its_dying_soldier_on_its_first_frame_under_client15535_static() {
    assert_ne!(first_offset(SpectralParentBlocker::Client15535Static), 0, "new: the Spectral walked straight on its first frame");
}

#[test]
fn the_old_value_walks_straight() {
    assert_eq!(first_offset(SpectralParentBlocker::None), 0, "old: the Spectral turned on its first frame (vacuous otherwise)");
}

#[test]
fn the_shipped_value_is_none() {
    assert_eq!(Calib::shipped().spectral_parent_blocker, SpectralParentBlocker::None);
}
