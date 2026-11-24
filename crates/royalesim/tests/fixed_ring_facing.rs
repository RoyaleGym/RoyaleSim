//! spawner.FIXED_RING_FACING: a building on spawner.DEATH_SPAWN_RING's fixed-ring list whose row has no damage source (the
//! Goblin Drill's GoblinDrill and GoblinDrill_EV1; card.rs `CardDef::inert_attack_row`), under client16635_building_target
//! (target.rs `fixed_ring_attacker`, `can_target`; state.rs `phase_attack_for`, `phase_reap`).
//!
//! THE LAW, measured on client 16.635 (parity r99, the oracle's il-mg-16635 set): the building takes targets with its row's
//! attack sides (Range and SightRange 2000, ground only), faces its target on every tick of its attack and never lands its
//! swing (its attack progress runs on past HitSpeed); its fixed ring's two Goblins start with its facing (208 of 222
//! members). Under forward (the engine before the key) it is inert (no target, its side's forward) and the Goblins face
//! their side's forward.
//!
//! THE SCENE: a blue Goblin Drill building put down deployed at (9000, 20000) on red's half, and a red Tombstone at
//! (10200, 21200), 1,697 off centre to centre (the direction (181, 181) at length 256), out of reach of blue's towers;
//! every troop (the building's Goblins, the Tombstone's Skeletons) struck to 0 and the building's hitpoints topped up
//! (red's towers reach it) before each tick.
//! WHAT IS PINNED:
//!   1. client16635_building_target: the building holds the Tombstone, faces it, and stays in its attack's windup past
//!      HitSpeed with no tick on a hit; the Tombstone's hitpoints are the old arm's tick for tick (its lifetime drain).
//!      Under forward (the vacuity check) it holds nothing and faces blue's forward;
//!   2. the building struck to 0: under client16635_building_target its death pair faces where it faced, on its first
//!      frame and its second (they deploy); under forward (the vacuity check) blue's forward;
//!   3. the shipped value is forward.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! fixed_ring_facing`):
//!   * `drill_building_inert` -- the new arm's building stays inert: (1) goes red.
//!   * `fixed_ring_swing_lands` -- the new arm's building lands its swing: (1) goes red.
//!   * `fixed_ring_members_face_forward` -- the new arm's death pair faces blue's forward: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::{AttackPhase, EntityKind};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, FixedRingFacing};
use royalesim::{EntityId, Team};

const DRILL: (i32, i32) = (9000, 20000);
const TOMB: (i32, i32) = (10200, 21200);
/// The direction from the building to the Tombstone at length 256, native (move16402.rs `normalize_to`).
const TOWARD: (i32, i32) = (181, 181);

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Within 1 of `want` on each axis.
fn faces(v: Vec2, want: (i32, i32)) -> bool {
    (v.x - want.0).abs() <= 1 && (v.y - want.1).abs() <= 1
}

/// One tick of the scene, read after it ran: the building's facing, target, attack phase and progress; the Tombstone's
/// hitpoints.
#[derive(Clone, Copy, Debug)]
struct Frame {
    facing: Vec2,
    target: Option<EntityId>,
    phase: AttackPhase,
    progress: i32,
    tomb_hp: i32,
}

struct Scene {
    s: BattleState,
    drill: EntityId,
    tomb: EntityId,
    frames: Vec<Frame>,
}

/// Every troop on the board struck to 0.
fn clear_troops(s: &mut BattleState) {
    let ids: Vec<EntityId> = s.entities().filter(|e| e.kind == EntityKind::Troop).map(|e| e.id).collect();
    for id in ids {
        assert!(s.debug_set_hp(id, 0));
    }
}

/// The scene under spawner.FIXED_RING_FACING = `arm`, `ticks` ticks run after the Tombstone is put down.
fn drill_scene(arm: FixedRingFacing, ticks: usize) -> Scene {
    let mut cfg: BattleConfig = config();
    cfg.calib.fixed_ring_facing = arm;
    cfg.decks = [vec!["GoblinDrill".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let drill = s.scenario_spawn_now(Team::Blue, "units.GoblinDrill", n(DRILL.0, DRILL.1), None).expect("the drill's building");
    let top = s.entity(drill).expect("the drill's building").max_hp;
    // Its spawn area (the drill's emergence blow) strikes before the Tombstone stands.
    for _ in 0..3 {
        s.tick();
    }
    let tomb = s.scenario_spawn_now(Team::Red, "Tombstone", n(TOMB.0, TOMB.1), None).expect("the Tombstone");
    let mut frames = Vec::new();
    for k in 0..ticks {
        clear_troops(&mut s);
        // Red's princess towers reach the building: its hitpoints are topped up, so it stands through the scene.
        assert!(s.debug_set_hp(drill, top));
        s.tick();
        let d = s.entity(drill).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the building fell on tick {k}"));
        let t = s.entity(tomb).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the Tombstone fell on tick {k}"));
        frames.push(Frame { facing: d.facing, target: d.target, phase: d.attack_phase, progress: d.attack_ms, tomb_hp: t.hp });
    }
    Scene { s, drill, tomb, frames }
}

/// (1) Plants: drill_building_inert, fixed_ring_swing_lands.
#[test]
fn under_client16635_building_target_the_drill_faces_its_target_and_never_hits() {
    let new = drill_scene(FixedRingFacing::Client16635BuildingTarget, 40);
    let old = drill_scene(FixedRingFacing::Forward, 40);
    let hit_speed = card_stat(&new.s, "units.GoblinDrill").hit_speed_ms;
    let last = *new.frames.last().expect("the frames");
    assert_eq!(last.target, Some(new.tomb), "client16635_building_target: the building does not hold the Tombstone: {last:?}");
    assert!(faces(last.facing, TOWARD), "client16635_building_target: the building does not face the Tombstone: {last:?}");
    let k0 = new.frames.iter().position(|f| f.phase != AttackPhase::Idle).expect("client16635_building_target: the building never attacked");
    assert!(
        new.frames[k0..].iter().all(|f| f.target == Some(new.tomb) && f.phase == AttackPhase::Windup),
        "client16635_building_target: the building's attack left its windup (a hit) or its target: {:?}",
        &new.frames[k0..]
    );
    assert!(last.progress > hit_speed, "the scene drifted: its attack did not run past HitSpeed ({} <= {hit_speed})", last.progress);
    let hp = |sc: &Scene| sc.frames.iter().map(|f| f.tomb_hp).collect::<Vec<i32>>();
    assert_eq!(hp(&new), hp(&old), "client16635_building_target: the Tombstone lost more than its lifetime drain");
    // NOT VACUOUS: under forward the building is inert.
    assert!(
        old.frames.iter().all(|f| f.target.is_none() && f.facing == Vec2::new(0, 256)),
        "forward: the building took a target or turned: {:?}",
        old.frames.last()
    );
}

/// The scene run 40 ticks under `arm`, then the building struck to 0: (its facing as it fell; its death pair's facing on
/// their first frame and on their second, in creation order).
fn death_pair(arm: FixedRingFacing) -> (Vec2, Vec<(Vec2, Vec2)>) {
    let mut sc = drill_scene(arm, 40);
    let drill = sc.drill;
    let fell = sc.s.entity(drill).expect("the building").facing;
    let before: Vec<EntityId> = sc.s.entities().map(|e| e.id).collect();
    assert!(sc.s.debug_set_hp(drill, 0));
    let near = |p: Vec2| (p.x / K - DRILL.0).abs() <= 1500 && (p.y / K - DRILL.1).abs() <= 1500;
    let mut pair: Vec<(u32, EntityId, Vec2)> = Vec::new();
    for _ in 0..3 {
        sc.s.tick();
        pair = sc
            .s
            .entities()
            .filter(|e| e.team == Team::Blue && e.kind == EntityKind::Troop && !before.contains(&e.id) && e.spawned_by != Some(drill) && near(e.pos))
            .map(|e| (e.team_seq, e.id, e.facing))
            .collect();
        if !pair.is_empty() {
            break;
        }
    }
    assert!(sc.s.entity(drill).is_none() && pair.len() == 2, "{arm:?}: the scene drifted: no death pair ({pair:?})");
    pair.sort_unstable_by_key(|g| g.0);
    sc.s.tick();
    let out: Vec<(Vec2, Vec2)> = pair.iter().map(|&(_, id, first)| (first, sc.s.entity(id).expect("a member stands").facing)).collect();
    (fell, out)
}

/// (2) Plant: fixed_ring_members_face_forward.
#[test]
fn under_client16635_building_target_the_drills_goblins_face_where_it_faced() {
    let (fell, pair) = death_pair(FixedRingFacing::Client16635BuildingTarget);
    assert!(faces(fell, TOWARD), "the scene drifted: the building did not face the Tombstone as it fell ({fell:?})");
    assert!(
        pair.iter().all(|(a, b)| *a == fell && *b == fell),
        "client16635_building_target: the death pair does not face where the building faced ({fell:?}): {pair:?}"
    );
    // NOT VACUOUS: under forward the pair faces blue's forward.
    let (_, old) = death_pair(FixedRingFacing::Forward);
    let forward = Vec2::new(0, 256);
    assert!(old.iter().all(|(a, b)| *a == forward && *b == forward), "forward: the death pair does not face blue's forward: {old:?}");
}

/// Since parity r99 for clients 16.402 and 16.635 (forward before it; client 15.535.29 keeps forward).
#[test]
fn the_shipped_arm_is_client16635_building_target() {
    assert_eq!(
        royalesim::state::Calib::shipped().fixed_ring_facing,
        royalesim::state::FixedRingFacing::Client16635BuildingTarget
    );
    assert_eq!(config().calib.fixed_ring_facing, royalesim::state::FixedRingFacing::Client16635BuildingTarget);
}
