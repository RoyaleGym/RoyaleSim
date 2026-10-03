//! spawner.SPECTRAL_FIRST_UPDATE: whether an Evo Skeleton Army Spectral, made at the end of Reap where its soldier died
//! (state.rs `army_deaths`, `army_spectrals`), takes its first update on that tick (`first_update`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, all 43 Spectrals of sp-form-SkeletonArmy-evo-s0,
//! sp-esa-spectrals-s0 and sp-il-b5e2 hold a target on their first frame, the walking ones already turning (avoidance
//! offset +-190); where the scene agrees, one Spectral step (speed 60) from the soldier's point: sp-esa-spectrals-s0 t829
//! (-16, -58), sp-form-SkeletonArmy-evo-s0 t947 (-10, 60).
//!
//! THE SCENE: evo_skeleton_army.rs's army (the form played at (9500, 11500), 31 ticks on); one soldier set to 0 hp dies in
//! the next tick's Reap and leaves a Spectral, and a Red Knight put down 3,000 to that soldier's right stands in the
//! Spectral's sight (5,500): the engine's scan holds no target for a unit with no enemy in sight (it walks to its lane
//! tower), where the client's target field holds that tower. The scene is the same under both arms up to that Reap, so
//! the engine's arm's Spectral stands on the point both arms make it.
//!
//! WHAT IS PINNED, and the plant that turns it red (spectral_stands_first_tick):
//!   1. client15535_same_tick: on its first frame the Spectral holds a target and stands off the point it was made, at
//!      most one step (60) and one contact push (150) from it; none (the engine's, the vacuity check): no target, on the
//!      point.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, SpectralFirstUpdate};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Spectral's first frame under `arm`: its point and its target.
fn first_frame(arm: SpectralFirstUpdate) -> (Vec2, Option<EntityId>) {
    let mut cfg: BattleConfig = config();
    cfg.calib.spectral_first_update = arm;
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
        (v[0].id, v[0].pos)
    };
    s.scenario_spawn_now(Team::Red, "Knight", Vec2::new(at.x + 3000 * K, at.y), None).expect("a red Knight in the Spectral's sight");
    assert!(s.debug_set_hp(victim, 0));
    s.tick();
    assert!(s.entity(victim).is_none(), "the scene drifted: the soldier lives");
    let sp = find_live(&s, Team::Blue, "SkeletonArmy_EV1_Spectral");
    assert_eq!(sp.len(), 1, "the scene drifted: one Spectral on the frame the soldier is first gone");
    (sp[0].pos, sp[0].target)
}

/// Plant: spectral_stands_first_tick.
#[test]
fn a_spectral_takes_its_first_update_on_its_birth_tick_under_client15535_same_tick() {
    let (made_at, target) = first_frame(SpectralFirstUpdate::None);
    // NOT VACUOUS: the engine's arm leaves it on its point with no target.
    assert!(target.is_none(), "none: the Spectral acquired on its birth tick ({target:?})");
    let (pos, target) = first_frame(SpectralFirstUpdate::Client15535SameTick);
    assert!(target.is_some(), "client15535_same_tick: the Spectral holds no target on its first frame");
    let off = pos.dist(made_at) / K;
    assert!(pos != made_at && off <= 60 + 150, "client15535_same_tick: the Spectral stands {off} off the point it was made ({pos:?} from {made_at:?})");
}
