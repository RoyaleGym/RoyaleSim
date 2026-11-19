//! THE PHOENIX EGG's two columns (card.rs `SpawnerDef::destroy_at_limit` / `with_deploy`): spawner.DESTROY_AT_LIMIT (the egg
//! leaves without a death on the tick after its hatch) and spawner.SPAWN_CHARACTER_WITH_DEPLOY (the reborn Phoenix deploys for
//! its own DeployTime).
//!
//! THE LAWS, measured on client 16.635 (parity r97, the oracle's il-mg-16635 set): every hatched egg, 5 of 5, had its last
//! frame on the reborn Phoenix's first (the hatch tick H); every reborn Phoenix, 5 of 5, stood deploying on H..H + 14, still on
//! H + 15, and first moved on H + 16.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test phoenix_egg`):
//!   * `destroy_at_limit_stays` -- the new arm leaves the egg standing: `under_client16635_leaves_after_last_firing_...` red.
//!   * `spawn_with_deploy_born_walking` -- the new arm's Phoenix is born walking: `under_client16635_own_deploy_time_...` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DestroyAtLimit, SpawnCharacterWithDeploy};
use royalesim::Team;

struct Hatch {
    egg_on_h: bool,
    egg_after: Vec<bool>,
    blue_on_h: usize,
    blue_after_1: usize,
    child: Vec<(Vec2, bool)>,
}

fn blue_count(s: &BattleState) -> usize {
    s.entities().filter(|e| e.team == Team::Blue).count()
}

/// A Blue Phoenix at (9000, 11000), out of every Red tower's reach, killed at once, under the two arms: the egg's presence
/// after its hatch tick H and after H + 1..=10, and the reborn Phoenix's (point, deploying) after H + 0..=16.
fn hatch(d: DestroyAtLimit, w: SpawnCharacterWithDeploy) -> Hatch {
    let mut cfg = config();
    cfg.calib.destroy_at_limit = d;
    cfg.calib.spawn_character_with_deploy = w;
    let mut s = BattleState::new(7, cfg);
    let p = s.scenario_spawn_now(Team::Blue, "Phoenix", Vec2::new(9000 * K, 11000 * K), None).expect("the Phoenix");
    assert!(s.debug_set_hp(p, 0));
    let mut egg = None;
    for _ in 0..10 {
        s.tick();
        if let Some(e) = find_live(&s, Team::Blue, "PhoenixEgg").first() {
            egg = Some(e.id);
            break;
        }
    }
    let egg = egg.expect("the scene drifted: no egg");
    let mut child = None;
    for _ in 0..300 {
        s.tick();
        if let Some(c) = find_live(&s, Team::Blue, "PhoenixNoRespawn").first() {
            child = Some(c.id);
            break;
        }
    }
    let child = child.expect("the scene drifted: no reborn Phoenix");
    let egg_on_h = s.entity(egg).is_some();
    let blue_on_h = blue_count(&s);
    let mut out = Hatch { egg_on_h, egg_after: Vec::new(), blue_on_h, blue_after_1: 0, child: Vec::new() };
    let c0 = s.entity(child).expect("the reborn Phoenix");
    out.child.push((c0.pos, c0.deploying));
    for k in 1..=16 {
        s.tick();
        if k <= 10 {
            out.egg_after.push(s.entity(egg).is_some());
        }
        if k == 1 {
            out.blue_after_1 = blue_count(&s);
        }
        let c = s.entity(child).expect("the reborn Phoenix stands");
        out.child.push((c.pos, c.deploying));
    }
    out
}

/// spawner.DESTROY_AT_LIMIT = client16635_leaves_after_last_firing (client 16.635, parity r97, the oracle's il-mg-16635 set: 5 of
/// 5): the egg stands on its hatch tick beside the reborn Phoenix and is gone, with nothing made, from the next tick; under
/// stays (the vacuity check) it stands on. Plant: destroy_at_limit_stays.
#[test]
fn under_client16635_leaves_after_last_firing_a_phoenix_egg_leaves_the_tick_after_its_hatch() {
    let old = hatch(DestroyAtLimit::Stays, SpawnCharacterWithDeploy::SpawnedDeployTime);
    assert!(old.egg_on_h && old.egg_after.iter().all(|x| *x), "stays (the vacuity check): the egg left {:?}", old.egg_after);
    let new = hatch(DestroyAtLimit::Client16635LeavesAfterLastFiring, SpawnCharacterWithDeploy::SpawnedDeployTime);
    assert!(new.egg_on_h, "client16635_leaves_after_last_firing: the egg was gone on its hatch tick");
    assert!(new.egg_after.iter().all(|x| !*x), "client16635_leaves_after_last_firing: the egg stood on {:?}", new.egg_after);
    assert_eq!(new.blue_after_1, new.blue_on_h - 1, "client16635_leaves_after_last_firing: the egg's leave made something");
}

/// spawner.SPAWN_CHARACTER_WITH_DEPLOY = client16635_own_deploy_time (client 16.635, parity r97, the oracle's il-mg-16635 set: 5
/// of 5): the reborn Phoenix deploys on H..H + 14, stands on H + 15 and first moves on H + 16; under spawned_deploy_time (the
/// vacuity check) it is born walking. Plant: spawn_with_deploy_born_walking.
#[test]
fn under_client16635_own_deploy_time_a_reborn_phoenix_deploys_for_its_own_deploy_time() {
    let old = hatch(DestroyAtLimit::Stays, SpawnCharacterWithDeploy::SpawnedDeployTime);
    assert!(!old.child[0].1 && old.child[1].0 != old.child[0].0, "spawned_deploy_time (the vacuity check): not born walking {:?}", &old.child[..2]);
    let new = hatch(DestroyAtLimit::Stays, SpawnCharacterWithDeploy::Client16635OwnDeployTime);
    for k in 0..=14 {
        assert!(new.child[k].1, "client16635_own_deploy_time: not deploying on H + {k}");
    }
    assert!(!new.child[15].1, "client16635_own_deploy_time: still deploying on H + 15");
    for k in 0..=15 {
        assert_eq!(new.child[k].0, new.child[0].0, "client16635_own_deploy_time: moved on H + {k}");
    }
    assert_ne!(new.child[16].0, new.child[15].0, "client16635_own_deploy_time: no first step on H + 16");
}

/// The loader reads both columns off the Phoenix egg's row, and off no other spawner's (the Tombstone's).
#[test]
fn the_loader_reads_destroy_at_limit_and_spawn_character_with_deploy_off_the_egg_row() {
    let s = BattleState::new(7, config());
    let db = s.cards();
    let egg = db.get(db.index("PhoenixEgg").expect("the egg loads")).spawner.expect("the egg's spawner");
    assert!(egg.destroy_at_limit && egg.with_deploy, "the egg's columns: {egg:?}");
    let tomb = db.get(db.index("Tombstone").expect("the Tombstone loads")).spawner.expect("the Tombstone's spawner");
    assert!(!tomb.destroy_at_limit && !tomb.with_deploy, "the Tombstone's: {tomb:?}");
}

/// Since parity r97 for clients 16.402 and 16.635 (stays before it; client 15.535.29 keeps stays).
#[test]
fn the_shipped_arm_is_client16635_leaves_after_last_firing() {
    assert_eq!(
        royalesim::state::Calib::shipped().destroy_at_limit,
        royalesim::state::DestroyAtLimit::Client16635LeavesAfterLastFiring
    );
    assert_eq!(config().calib.destroy_at_limit, royalesim::state::DestroyAtLimit::Client16635LeavesAfterLastFiring);
}

/// Since parity r97 for clients 16.402 and 16.635 (spawned_deploy_time before it; client 15.535.29 keeps
/// spawned_deploy_time).
#[test]
fn the_shipped_arm_is_client16635_own_deploy_time() {
    assert_eq!(
        royalesim::state::Calib::shipped().spawn_character_with_deploy,
        royalesim::state::SpawnCharacterWithDeploy::Client16635OwnDeployTime
    );
    assert_eq!(
        config().calib.spawn_character_with_deploy,
        royalesim::state::SpawnCharacterWithDeploy::Client16635OwnDeployTime
    );
}
