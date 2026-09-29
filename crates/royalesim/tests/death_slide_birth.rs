//! spawner.DEATH_SLIDE_BIRTH -- where a sliding death-spawn member is born (spawner.DEATH_SPAWN_PUSHBACK =
//! client_ring_slide; state.rs `death_spawn_points`, `slide_birth_toward_end`, `slide_end_points`; move16402.rs
//! `death_slide_toward`).
//!
//! THE READING (step_toward_end; sine_table is the shipped, old arm): member k is born one slide step, 250, from the
//! death point toward its end point (the ring point at DeathSpawnRadius, through the sine table), through the walk's
//! 1/256 direction and truncation. Read off client 16.402, the Lava Hound's death in 20260920-071744-B on t1798: its
//! six Pups first stand on (125, -215), (-125, -215), (-250, 0), (-125, 215), (125, 215) and (250, 0) from the death
//! point; the sine table at 250 lays the four diagonal ones a unit further out in y, (+-125, +-216). Read off client
//! 15.535.29, the Skeleton Barrel's container ring (side 0, a death on the left half): (-153, -196), (55, -243),
//! (226, -105), (224, 109), (51, 244), (-157, 194), (-250, 0); the sine table gives (-153, -197), (56, -243) and
//! (52, 244) for three of them. The end points stay the sine table's under both arms: the client's 300-degree Pup's last
//! move lands on (1249, -2164), the 1/256 step toward (1250, -2165).
//!
//! WHAT IS PINNED:
//!   1. step_toward_end: a Lava Hound's six Pups are born on the client's six points from the death point, in creation
//!      order, and under sine_table on the sine table's six (the old arm, non-vacuity: the diagonals differ by a unit);
//!   2. step_toward_end: a Skeleton Barrel's seven container members (side 0, the left half) on the measured seven,
//!      and under sine_table on the sine table's seven (three differ);
//!   3. both arms: the Pups' end points are the same (the death point + the sine table's ring direction x 2500);
//!   4. both arms: a Golem's two Golemites, on the axis, are born on (-250, 0) and (250, 0) (the control);
//!   5. the shipped value is step_toward_end (since the 2026-09-28 round 9 lanes flip).
//!
//! PLANT (regression): `death_birth_sine_table` lays the sine table's ring under step_toward_end: (1) and (2) go red.
//!     RUSTFLAGS='--cfg clash_plant="death_birth_sine_table"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test death_slide_birth
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, DeathSlideAim, DeathSlideBirth, DeathSpawnPushback};
use royalesim::{EntityId, Team};

const NEW: DeathSlideBirth = DeathSlideBirth::StepTowardEnd;
const OLD: DeathSlideBirth = DeathSlideBirth::SineTable;

/// The client's Pups, from the death point, in creation order (20260920-071744-B t1798).
const PUPS_CLIENT: [(i32, i32); 6] = [(125, -215), (-125, -215), (-250, 0), (-125, 215), (125, 215), (250, 0)];
/// The sine table's ring at 250, the old arm.
const PUPS_SINE: [(i32, i32); 6] = [(125, -216), (-125, -216), (-250, 0), (-125, 216), (125, 216), (250, 0)];
/// The client's container ring, side 0, a death on the left half (client 15.535.29).
const RING_CLIENT: [(i32, i32); 7] = [(-153, -196), (55, -243), (226, -105), (224, 109), (51, 244), (-157, 194), (-250, 0)];
/// The sine table's container ring at 250, the old arm.
const RING_SINE: [(i32, i32); 7] = [(-153, -197), (56, -243), (226, -105), (224, 109), (52, 244), (-157, 194), (-250, 0)];

fn with(birth: DeathSlideBirth) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.death_spawn_pushback = DeathSpawnPushback::ClientRingSlide;
    cfg.calib.death_slide_aim = DeathSlideAim::FixedEndPoint;
    cfg.calib.death_slide_birth = birth;
    cfg
}

fn native(v: Vec2) -> (i32, i32) {
    (v.x / K, v.y / K)
}

/// The death-spawned `unit`s of Blue with no owner, in creation order.
fn members(s: &BattleState, unit: &str) -> Vec<EntityId> {
    let mut m: Vec<(u32, EntityId)> = find_live(s, Team::Blue, unit).into_iter().filter(|e| e.spawned_by.is_none()).map(|e| (e.team_seq, e.id)).collect();
    m.sort();
    m.into_iter().map(|(_, id)| id).collect()
}

/// A Blue `parent` at (9000, 9000) killed on the first tick: its `unit` members' offsets from the death point on the
/// death frame, and their end points' offsets.
/// Native offsets from the death point, one per member in creation order.
type Offsets = Vec<(i32, i32)>;

fn troop_ring(birth: DeathSlideBirth, parent: &str, unit: &str) -> (Offsets, Offsets) {
    let mut s = BattleState::new(7, with(birth));
    let id = s.scenario_spawn_now(Team::Blue, parent, t(900, 900), None).expect("the parent");
    assert!(s.debug_set_hp(id, 0));
    s.tick();
    assert!(s.entity(id).is_none(), "the {parent} died on the first tick");
    let ids = members(&s, unit);
    assert!(!ids.is_empty(), "the {parent} left no {unit}");
    let c = s.entity(ids[0]).unwrap().death_slide_centre;
    let born = ids.iter().map(|m| native(s.entity(*m).unwrap().pos.sub(c))).collect();
    let ends = ids.iter().map(|m| native(s.entity(*m).unwrap().death_slide_end.sub(c))).collect();
    (born, ends)
}

/// Plant: death_birth_sine_table.
#[test]
fn the_lava_pups_are_born_one_step_toward_their_end_points() {
    let (born, _) = troop_ring(NEW, "LavaHound", "LavaPups");
    assert_eq!(born, PUPS_CLIENT.to_vec(), "step_toward_end: the Pups' births from the death point");
    let (old, _) = troop_ring(OLD, "LavaHound", "LavaPups");
    assert_eq!(old, PUPS_SINE.to_vec(), "sine_table: the Pups' births from the death point");
}

#[test]
fn the_end_points_are_the_sine_tables_under_both_arms() {
    let (_, new) = troop_ring(NEW, "LavaHound", "LavaPups");
    let (_, old) = troop_ring(OLD, "LavaHound", "LavaPups");
    assert_eq!(new, vec![(1250, -2165), (-1250, -2165), (-2500, 0), (-1250, 2165), (1250, 2165), (2500, 0)], "the Pups' end points");
    assert_eq!(new, old, "the end points differ between the birth arms");
}

#[test]
fn the_golemites_on_the_axis_are_born_on_the_same_points_under_both_arms() {
    for arm in [NEW, OLD] {
        let (born, _) = troop_ring(arm, "Golem", "Golemite");
        assert_eq!(born, vec![(-250, 0), (250, 0)], "{arm:?}: the Golemites' births");
    }
}

/// A Blue Skeleton Barrel at (6000, 10000), the left half, killed on its tick: its seven container members' offsets
/// from the container's point on the tick they appear (T + 12).
fn container_ring(birth: DeathSlideBirth) -> Vec<(i32, i32)> {
    let mut cfg = with(birth);
    cfg.card_level = [11, 11];
    let mut s = BattleState::new(9, cfg);
    let id = s.scenario_spawn_now(Team::Blue, "SkeletonBalloon", Vec2::new(6000 * K, 10000 * K), None).expect("the barrel");
    assert!(s.debug_set_hp(id, 0));
    let t0 = s.tick_count();
    s.tick();
    assert!(s.entity(id).is_none(), "the barrel died on its tick");
    let c = {
        let db = s.cards();
        let unit = db.get(db.index("SkeletonBalloon").expect("the barrel loads")).death_spawn.expect("its death spawn").unit;
        s.spells()
            .iter()
            .find(|sp| sp.card == unit)
            .map(|sp| match &sp.motion {
                royalesim::spell::SpellMotion::Flight { aim, .. } => *aim,
                m => panic!("the container is not a Flight: {m:?}"),
            })
            .expect("the barrel's death left its container")
    };
    while s.tick_count() <= t0 + 12 {
        s.tick();
    }
    let mut m: Vec<(u32, EntityId)> = s.entities().filter(|v| v.team == Team::Blue && v.card == "Skeleton" && v.spawned_by.is_none()).map(|v| (v.team_seq, v.id)).collect();
    m.sort();
    assert_eq!(m.len(), 7, "the seven Skeletons on T + 12");
    m.iter().map(|(_, id)| native(s.entity(*id).unwrap().pos.sub(c))).collect()
}

/// Plant: death_birth_sine_table.
#[test]
fn the_container_members_are_born_one_step_toward_their_end_points() {
    assert_eq!(container_ring(NEW), RING_CLIENT.to_vec(), "step_toward_end: the container ring's births from its point");
    assert_eq!(container_ring(OLD), RING_SINE.to_vec(), "sine_table: the container ring's births from its point");
}

#[test]
fn the_shipped_value_is_the_new_arm() {
    assert_eq!(Calib::shipped().death_slide_birth, NEW);
}
