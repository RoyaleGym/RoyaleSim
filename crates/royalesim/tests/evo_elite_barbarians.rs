//! THE EVO ELITE BARBARIANS (card.rs `SpearDef`; state.rs `spear_pass`, `evo_after_fire`; combat.rs `fire`,
//! `step_projectiles`), against client 15.535.29 at level 11.
//!
//! THE MEASUREMENTS:
//!   - sp-ec-AngryBarbarians: the second play of an evolved Elite Barbarians entry puts the form down (its
//!     spells_evolved DarkElixirCost, 1; 42 of 42 evo-cycle runs agree with that column);
//!   - sp-form-AngryBarbarians-evo-s0: the two members (AngryBarbarian_EV1, LoadTime 850, and AngryBarbarian_EV1_2,
//!     900) come down 1400 apart and leave their deploys 2 ticks apart (SummonSpawnDelay 100). Each took a Knight 5,300
//!     to 6,000 away, stood from its first attacking frame at progress LoadTime + 50 and threw on the frame the
//!     progress reached HitSpeed 1400: 10 frames for the first, 9 for the second. The spear left 196-200 ahead of it,
//!     stepped 600 a tick and took 284 (111 on the ladder) off the Knight. Each thrower walked at its Knight on the
//!     next frame and walked raged (89 -> 116) from 11 frames after its throw: the Rage area its spear left on step 4
//!     of its flight, 1800 along it, first applies one HitSpeed (300) later;
//!   - sp-rage-5000-s0 and sp-rage-4000-s0: neither threw at an Elixir Collector 4000 or 5000 ahead, centre to centre,
//!     which is inside the 3500 minimum as an attack reach (range plus both radii).
//!
//! THE SCENE here: an evolved Elite Barbarians put down by the spawner on Blue's side, a red Elixir Collector (it never
//! strikes back) straight ahead on the same side of the river, far enough that both members stand outside the minimum
//! and inside the maximum; Blue's princess towers down and the Collector out of the king's reach, so nothing else
//! strikes. The thrower walks up its spear's line after the throw, through the trail, as the scene's did.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_elite_barbarians`):
//!   - evo_cycles_constant -> `the_second_play_is_the_evolved_one` red (the third play, as the old constant said);
//!   - spear_never_thrown -> `each_member_throws_its_spear_at_the_end_of_its_windup` red (they walk and strike);
//!   - spear_trail_dropped -> `the_thrower_walks_raged_from_11_frames_after_its_throw` red (no area near it);
//!   - spear_area_pulses_at_once -> `the_thrower_walks_raged_from_11_frames_after_its_throw` red (raged too early);
//!   - spear_window_every_tick -> `a_throw_under_way_is_thrown_though_its_target_leaves_the_window` red (no throw).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

#[test]
fn the_second_play_is_the_evolved_one() {
    let mut cfg = config();
    cfg.decks = [vec!["AngryBarbarians".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    let mut s = BattleState::new(7, level11(cfg));
    past_deploy_lockout(&mut s);
    let counter = |s: &BattleState| s.evo_counters(Team::Blue)[0];
    assert_eq!(counter(&s).cycles, 1, "the form's DarkElixirCost");
    let play = |s: &mut BattleState, x: i32| -> Vec<String> {
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        let before: Vec<EntityId> = s.entities().map(|e| e.id).collect();
        s.deploy(Team::Blue, "AngryBarbarians", n(x, 9500)).expect("the play");
        s.tick();
        let mut new: Vec<String> = s.entities().filter(|e| e.team == Team::Blue && !before.contains(&e.id)).map(|e| e.card.to_string()).collect();
        new.sort();
        new
    };
    assert_eq!(play(&mut s, 3500), ["AngryBarbarians", "AngryBarbarians"], "the first play is the base card");
    assert!(counter(&s).next_evolved(), "one basic play makes the next one evolved");
    assert_eq!(play(&mut s, 14500), ["AngryBarbarian_EV1_2", "AngryBarbarians_EV1"], "the second play is the form, both members");
    assert_eq!(counter(&s).plays, 0);
}

/// One frame of a member: its step, its attack progress, its target.
#[derive(Clone, Copy, Debug)]
struct Frame {
    pos: Vec2,
    attack_ms: i32,
    deploying: bool,
    target: Option<EntityId>,
}

/// A scene's record: each member's frames, every spear on the frame it is thrown, and the battle after.
type Scene = (Vec<Vec<Frame>>, Vec<(u32, Vec2, i32)>, BattleState);

/// The scene: the form put down at (9000, 8500) at level 11, a red Elixir Collector at (9000, 14500) put down first;
/// `frames` frames of each member (the form's own unit first), and every spear on the frame it is thrown (a shot with a
/// trail that has not stepped yet): (frame, its point, its damage).
fn scene(frames: u32) -> Scene {
    scene_with(frames, |_, _, _| {})
}

/// `scene`, with `edit` run before each frame's tick (the frame, the battle, the members' ids).
fn scene_with(frames: u32, mut edit: impl FnMut(u32, &mut BattleState, &[EntityId])) -> Scene {
    let mut s = BattleState::new(7, level11(config()));
    past_deploy_lockout(&mut s);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    s.scenario_spawn_now(Team::Red, "Elixir Collector", n(9000, 14500), None).expect("the Collector");
    s.spawn_unit(Team::Blue, "AngryBarbarians_EV1", n(9000, 8500), None).expect("the form");
    s.tick();
    let form = s.cards().index("AngryBarbarians_EV1").unwrap();
    let second = s.cards().index("AngryBarbarian_EV1_2").unwrap();
    let ids: Vec<EntityId> = [form, second].iter().map(|c| s.entities().find(|e| e.card_idx == *c).expect("a member").id).collect();
    let mut out = vec![Vec::new(); 2];
    let mut spears: Vec<(u32, Vec2, i32)> = Vec::new();
    for k in 0..frames {
        edit(k, &mut s, &ids);
        s.tick();
        for (m, id) in ids.iter().enumerate() {
            if let Some(e) = s.entity(*id) {
                out[m].push(Frame { pos: e.pos, attack_ms: e.attack_ms, deploying: e.deploying, target: e.target });
            }
        }
        for p in s.projectiles().iter().filter(|p| p.trail.is_some_and(|t| t.steps == 0)) {
            spears.push((k, p.pos, p.damage));
        }
    }
    (out, spears, s)
}

#[test]
fn each_member_throws_its_spear_at_the_end_of_its_windup() {
    let (members, spears, _) = scene(60);
    // Both members throw one spear each, of 284 at level 11.
    assert_eq!(spears.len(), 2, "two spears: {spears:?}");
    assert!(spears.iter().all(|sp| sp.2 == 284), "each spear deals 111 on the ladder, 284 at level 11: {spears:?}");
    for (m, load) in [(0usize, 850), (1usize, 900)] {
        let fr = &members[m];
        let first = fr.iter().position(|f| f.attack_ms > 0).expect("the member attacks") as u32;
        assert_eq!(fr[first as usize].attack_ms, load + 50, "member {m}'s first attacking frame reads LoadTime + 50");
        // The throw: the frame its progress reaches HitSpeed 1400, (1400 - load - 50) / 50 frames on.
        let throw = first + ((1400 - load - 50) / 50) as u32;
        assert!(spears.iter().any(|sp| sp.0 == throw), "member {m} throws on frame {throw}: {spears:?}");
        // It stands from its first attacking frame through the throw, and the spear leaves 200 ahead of it.
        let still = fr[first as usize..=throw as usize].windows(2).all(|w| w[0].pos == w[1].pos);
        assert!(still, "member {m} stands through its windup");
        let sp = spears.iter().find(|sp| sp.0 == throw).unwrap();
        let d = sp.1.sub(fr[throw as usize].pos);
        let gap = isqrt(((d.x / K) as i64).pow(2) + ((d.y / K) as i64).pow(2)) as i32;
        assert!((195..=205).contains(&gap), "member {m}'s spear leaves {gap} from it");
        assert!(!fr[throw as usize].deploying);
        let _ = fr[throw as usize].target;
    }
}

#[test]
fn a_throw_under_way_is_thrown_though_its_target_leaves_the_window() {
    // The window is read as the windup starts. Two frames into the first member's windup the Collector is moved
    // straight back along the member's line to 5,200 edge to edge, past the 5000 maximum; the member still stands and
    // throws on the frame its progress reaches HitSpeed, as the scene's second member did at its walking Knight (t778
    // at 4,969, thrown on t787 at 5,054).
    let mut start: Option<u32> = None;
    let mut gap_after: Option<i32> = None;
    let (members, spears, _) = scene_with(60, |k, s, ids| {
        if start.is_none() && s.entity(ids[0]).is_some_and(|e| e.attack_ms > 0) {
            start = Some(k - 1);
        }
        if start.is_some_and(|f| k == f + 3) {
            let c = s.entities().find(|e| e.card == "Elixir Collector").expect("the Collector");
            let (cid, cp, cr) = (c.id, c.pos, c.radius);
            let m = s.entity(ids[0]).unwrap();
            let (mp, mr) = (m.pos, m.radius);
            let d = cp.sub(mp);
            let (dx, dy) = ((d.x / K) as i64, (d.y / K) as i64);
            let len = isqrt(dx * dx + dy * dy).max(1);
            let to = 5200 + ((mr + cr) / K) as i64;
            let at = Vec2::new(mp.x + (dx * to / len) as i32 * K, mp.y + (dy * to / len) as i32 * K);
            assert!(s.debug_set_pos(cid, at));
            let e = at.sub(mp);
            let centre = isqrt(((e.x / K) as i64).pow(2) + ((e.y / K) as i64).pow(2)) as i32;
            gap_after = Some(centre - (mr + cr) / K);
        }
    });
    let first = start.expect("the first member attacks");
    let gap = gap_after.expect("the Collector is moved");
    assert!((5150..=5250).contains(&gap), "the Collector stands just past the maximum once moved ({gap})");
    let fr = &members[0];
    assert_eq!(fr[first as usize].attack_ms, 900, "its first attacking frame reads LoadTime + 50");
    let throw = first + (1400 - 900) / 50;
    assert!(spears.iter().any(|sp| sp.0 == throw), "the first member throws on frame {throw}: {spears:?}");
    let still = fr[first as usize..=throw as usize].windows(2).all(|w| w[0].pos == w[1].pos);
    assert!(still, "it stands through its windup");
}

#[test]
fn the_thrower_walks_raged_from_11_frames_after_its_throw() {
    let (members, spears, _) = scene(80);
    assert!(!spears.is_empty(), "a spear");
    let fr = &members[0];
    let throw = spears[0].0 as usize;
    let step = |k: usize| {
        let d = fr[k].pos.sub(fr[k - 1].pos);
        let (dx, dy) = ((d.x / K) as i64, (d.y / K) as i64);
        isqrt(dx * dx + dy * dy) as i32
    };
    let steps: Vec<i32> = (throw + 1..throw + 16).map(step).collect();
    // Unraged 89-90 through throw + 10, raged 115-117 from throw + 11.
    assert!(steps[..10].iter().all(|v| (88..=90).contains(v)), "unraged steps after the throw: {steps:?}");
    assert!(steps[10..].iter().all(|v| (115..=117).contains(v)), "raged from throw + 11: {steps:?}");
}
