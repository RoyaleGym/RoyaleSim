//! movement.ATTACK_FACING, read off the engine: where a unit in its attack state faces.
//!
//! THE LAW, measured on client 15.535.29: on every tick a unit is in its attack state it faces its target, the move
//! law's integer normalize (length 256) of target - self on the START-OF-TICK positions. The scene is the client's: a
//! red Knight on the red half attacking a blue Hog Rider that runs north past its front, so the direction to the Hog
//! turns from tick to tick. Under the old value, kept, the Knight keeps the heading of its last walking tick.
//!
//! WHAT IS PINNED, each with a floor on the attack ticks and on the distinct directions, so neither passes on a scene
//! where the Knight never turned:
//!   1. toward_target: on every attack tick the Knight's facing is the normalize of (Hog - Knight) at the tick's start;
//!   2. kept: its facing does not move through the attack, while the direction to the Hog does;
//!   3. the shipped value is kept.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="attack_facing_kept"' CARGO_TARGET_DIR=target/plant cargo test --test
//! attack_facing`): toward_target keeps the walking heading, so (1) goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::move16402;
use royalesim::state::{AttackFacing, BattleState, Calib};
use royalesim::Team;

const KNIGHT_AT: (i32, i32) = (13300, 21000);
const HOG_AT: (i32, i32) = (14500, 19500);

/// Per attack tick of the Knight on the Hog: (the Knight's facing after the tick, the normalized direction to the Hog
/// at the tick's start).
fn attack_ticks(arm: AttackFacing) -> Vec<((i32, i32), (i32, i32))> {
    let mut cfg = config();
    cfg.calib.attack_facing = arm;
    let mut s = BattleState::new(0, cfg);
    let at = |p: (i32, i32)| Vec2::new(p.0 * K, p.1 * K);
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at(KNIGHT_AT), None), (Team::Blue, "HogRider", at(HOG_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (knight, hog) = (ids[0], ids[1]);
    let mut out = Vec::new();
    for _ in 0..40 {
        let (Some(k0), Some(h0)) = (s.entity(knight).map(|k| k.pos), s.entity(hog).map(|h| h.pos)) else { break };
        s.tick();
        let (Some(k), Some(_)) = (s.entity(knight), s.entity(hog)) else { break };
        if k.attack_phase == AttackPhase::Idle || k.target != Some(hog) {
            continue;
        }
        let mut v = (h0.x / K - k0.x / K, h0.y / K - k0.y / K);
        move16402::normalize_to(&mut v, 256);
        out.push(((k.facing.x, k.facing.y), v));
    }
    out
}

fn distinct(v: impl Iterator<Item = (i32, i32)>) -> usize {
    v.collect::<std::collections::BTreeSet<_>>().len()
}

#[test]
fn an_attacking_unit_faces_its_target_on_every_attack_tick_under_toward_target() {
    let rows = attack_ticks(AttackFacing::TowardTarget);
    assert!(rows.len() >= 10, "the scene drifted: the Knight attacked the Hog on {} ticks", rows.len());
    assert!(distinct(rows.iter().map(|r| r.1)) >= 5, "the scene drifted: the Hog did not cross the Knight's front");
    let wrong: Vec<_> = rows.iter().filter(|r| r.0 != r.1).collect();
    assert!(wrong.is_empty(), "(facing, toward the Hog at the tick's start): {:?}", &wrong[..wrong.len().min(5)]);
}

#[test]
fn under_kept_the_facing_does_not_follow_the_target() {
    let rows = attack_ticks(AttackFacing::Kept);
    assert!(rows.len() >= 10, "the scene drifted: the Knight attacked the Hog on {} ticks", rows.len());
    assert!(distinct(rows.iter().map(|r| r.1)) >= 5, "the scene drifted: the Hog did not cross the Knight's front");
    assert_eq!(distinct(rows.iter().map(|r| r.0)), 1, "kept: the facing moved during the attack: {rows:?}");
}

#[test]
fn the_shipped_value_is_kept() {
    assert_eq!(Calib::shipped().attack_facing, AttackFacing::Kept);
}
