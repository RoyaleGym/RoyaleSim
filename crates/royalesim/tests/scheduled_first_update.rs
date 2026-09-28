//! A GRAVEYARD'S SKELETON TAKES ITS FIRST UPDATE ON ITS CREATION TICK -- calibration spawner.SCHEDULED_UNIT_FIRST_UPDATE,
//! state.rs `phase_projectile` (the scheduled area's units) and `materialise_released` / `first_update`.
//!
//! THE READING (client_creation_tick, shipped at its old arm next_tick): a unit a scheduled area puts down takes its
//! first update on the tick it is created, as a death spawn and a spawner's emission do under
//! spawner.SPAWNED_FIRST_STEP. A deploying Skeleton does not walk, so on its first frame the update shows only as a
//! neighbour's contact push. Read off client 15.535.29's sweep-Graveyard: of 12 Skeletons, the one created on t318
//! stands on its first frame at (14639, 8945), its slot (14500, 9000) plus one contact push from a Knight 290 away,
//! where the engine put it on the slot and gave it the same push a tick later. The other 11 had no neighbour and stand
//! on their slots under either arm. One event, so the old arm ships.
//!
//! THE SCENE: a Blue Graveyard cast on (4500, 11000), in Blue's own half, with no enemy on the board. The first Skeleton
//! it puts down is found on a bare run; a second run stands a Blue Knight, still deploying and so standing still, 290
//! north of that Skeleton's slot two ticks before the Skeleton is created.
//!
//! WHAT IS PINNED:
//!   1. the null: with no neighbour both arms put the first Skeleton on its slot on its first frame, and run the same;
//!   2. client_creation_tick, with the Knight: the first frame already carries the push, (-52, -141) off the slot, the
//!      contact law's cap of 150;
//!   3. next_tick (the old arm), with the Knight: the first frame is the slot, and the push of 150 comes on the second
//!      frame, (-60, -137). The two pushes are not the same vector: the Knight has taken a push of its own by then;
//!   4. the shipped value is next_tick.
//!
//! PLANT (regression):
//!   * `scheduled_first_update_unread` -- the new arm's units take their first update on the next tick: (2) goes red.
//!     RUSTFLAGS='--cfg clash_plant="scheduled_first_update_unread"' CARGO_TARGET_DIR=target/plant cargo test --profile
//!     gate --test scheduled_first_update
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, ScheduledUnitFirstUpdate};
use royalesim::{EntityId, Team};

const NEW: ScheduledUnitFirstUpdate = ScheduledUnitFirstUpdate::ClientCreationTick;
const OLD: ScheduledUnitFirstUpdate = ScheduledUnitFirstUpdate::NextTick;
const CAST: (i32, i32) = (4500, 11000);
/// The Skeleton row the Graveyard puts down.
const SKELETON: &str = "Graveyard_rework_Skeleton";
/// The Knight's offset north of the Skeleton's slot, native: the sweep's Knight stood 290 away.
const NEIGHBOUR_DY: i32 = 290;

fn with(arm: ScheduledUnitFirstUpdate) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.scheduled_unit_first_update = arm;
    cfg
}

fn native(p: Vec2) -> (i32, i32) {
    (p.x / K, p.y / K)
}

fn skeletons(s: &BattleState) -> Vec<EntityId> {
    s.entities().filter(|v| v.team == Team::Blue && v.card == SKELETON).map(|v| v.id).collect()
}

/// The first Skeleton: (the tick count after the cast on which it is first on the board, its first three frames).
/// `neighbour`: the tick count and point at which to stand the Blue Knight.
fn first_skeleton(arm: ScheduledUnitFirstUpdate, neighbour: Option<(u32, (i32, i32))>) -> (u32, Vec<(i32, i32)>) {
    let mut s = BattleState::new(0, with(arm));
    s.spawn_unit(Team::Blue, "Graveyard", Vec2::new(CAST.0 * K, CAST.1 * K), None).expect("cast the Graveyard");
    for k in 1..200u32 {
        if let Some((at, p)) = neighbour {
            if k == at {
                s.scenario_spawn_now(Team::Blue, "Knight", Vec2::new(p.0 * K, p.1 * K), None).expect("the Knight");
            }
        }
        s.tick();
        if let Some(&id) = skeletons(&s).first() {
            let mut frames = vec![native(s.entity(id).unwrap().pos)];
            for _ in 0..2 {
                s.tick();
                frames.push(native(s.entity(id).expect("the Skeleton died").pos));
            }
            return (k, frames);
        }
    }
    panic!("the scene drifted: the Graveyard put no Skeleton down in 200 ticks");
}

fn with_knight(arm: ScheduledUnitFirstUpdate) -> (u32, Vec<(i32, i32)>, (i32, i32)) {
    let (born, bare) = first_skeleton(OLD, None);
    let slot = bare[0];
    let (k, frames) = first_skeleton(arm, Some((born - 2, (slot.0, slot.1 + NEIGHBOUR_DY))));
    assert_eq!(k, born, "{arm:?}: the scene drifted: the Knight moved the Skeleton's creation tick");
    (born, frames, slot)
}

#[test]
fn with_no_neighbour_both_arms_put_the_skeleton_on_its_slot() {
    let (born_old, old) = first_skeleton(OLD, None);
    let (born_new, new) = first_skeleton(NEW, None);
    assert_eq!((born_old, &old), (born_new, &new), "with no neighbour the arms ran differently");
    assert_eq!(old[0], old[1], "the scene drifted: the bare Skeleton moved on its second frame (it deploys)");
}

fn off(p: (i32, i32), slot: (i32, i32)) -> ((i32, i32), i64) {
    let d = (p.0 - slot.0, p.1 - slot.1);
    (d, royalesim::fixed::isqrt(d.0 as i64 * d.0 as i64 + d.1 as i64 * d.1 as i64))
}

/// Plant: scheduled_first_update_unread.
#[test]
fn the_new_arm_pushes_the_skeleton_on_its_creation_tick() {
    let (_, frames, slot) = with_knight(NEW);
    assert_eq!(slot, (1000, 11000), "the scene drifted: the first slot");
    assert_eq!(off(frames[0], slot), ((-52, -141), 150), "client_creation_tick: the first frame {:?}", frames[0]);
}

#[test]
fn the_old_arm_takes_the_push_a_tick_later() {
    let (_, old, slot) = with_knight(OLD);
    assert_eq!(old[0], slot, "next_tick: the first frame is off the slot");
    assert_eq!(off(old[1], slot), ((-60, -137), 149), "next_tick: the second frame {:?}", old[1]);
}

#[test]
fn the_shipped_value_is_next_tick() {
    assert_eq!(Calib::shipped().scheduled_unit_first_update, OLD);
}
