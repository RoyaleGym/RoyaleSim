//! RELEASED UNITS EXIST ON THE EVENT'S OWN FRAME AND ARE INERT ON IT -- calibration
//! spawner.RELEASE_TIMING = end_of_event_phase.
//!
//! A released unit is a spell's SpawnCharacter (the Goblin Barrel's goblins) or a death spawn.
//! Both clients show it on the frame of the event that released it (the barrel's vanishing
//! frame; the tick after the parent's last live frame) and do nothing with it there: no deploy
//! countdown, no step. So it deploys for exactly DeployTime / TICK_MS frames and first steps one
//! tick after -- ONE FRAME MORE than an ordinary deploy of the same DeployTime, which counts down
//! on its own first frame. That one frame is the discriminator every test here reads:
//!
//!   15.535.29 barrel (1100 ms):   first 147, deploying 147..168 (22), first step 170 = first + 23
//!   ordinary deploy (1000 ms): first 101, deploying 101..119 (19), first step 121 = first + 20
//!   corpus BattleRam death:    Barbarians (1000 ms) first on the ram's last live frame + 1,
//!                              deploying 20 frames
//!   corpus Golem death:        Golemites (no deploy time) first on last live + 1, moving, first
//!                              displaced on first + 1
//!
//! The tests pin the RELATIONS, not the 15.535.29 absolute ticks (those depend on the cast tick
//! and the tap snap). Every DeployTime is read from the data. The foil runs the earlier arm,
//! `next_spawn_phase`, by name, and pins what it did: every release a tick late, with the
//! countdown already run on its first frame. Plant `release_deferred` turns the first three red.
mod common;

use common::{config, find_live, t};
use royalesim::fixed::Vec2;
use royalesim::state::{BattleConfig, BattleState, ReleaseTiming};
use royalesim::{EntityId, Team};
use std::collections::BTreeMap;

const K: i32 = 18;

/// A step off the spawn point, native units: more than the 2-3 native separation nudges a
/// deploying unit takes (the 15.535.29 barrel's two nudged members on its second frame).
const STEP_NATIVE: i32 = 20;

fn cards_json() -> serde_json::Value {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    serde_json::from_str(&text).unwrap()
}

fn card_json(name: &str) -> serde_json::Value {
    cards_json()["cards"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap_or_else(|| panic!("{name} in cards.json")).clone()
}

/// One released or deployed unit's first frames.
#[derive(Debug, Clone, Copy)]
struct Life {
    first: u32,
    first_deploy_ms: i32,
    first_deploying: bool,
    deploying_frames: u32,
    first_not_deploying: Option<u32>,
    first_step: Option<u32>,
}

/// Tick `s` until `until`, following every live `team` `card` unit that did NOT exist in
/// `before`, from the frame it first exists.
fn follow(s: &mut BattleState, team: Team, card: &str, before: &[EntityId], until: u32) -> BTreeMap<EntityId, Life> {
    let mut lives: BTreeMap<EntityId, (Life, Vec2)> = BTreeMap::new();
    while s.tick_count() < until {
        s.tick();
        let now = s.tick_count();
        for e in find_live(s, team, card) {
            if before.contains(&e.id) {
                continue;
            }
            let (life, at) = lives.entry(e.id).or_insert((
                Life { first: now, first_deploy_ms: e.deploy_ms, first_deploying: e.deploying, deploying_frames: 0, first_not_deploying: None, first_step: None },
                e.pos,
            ));
            if e.deploying {
                life.deploying_frames += 1;
            } else if life.first_not_deploying.is_none() {
                life.first_not_deploying = Some(now);
            }
            let moved = (e.pos.x - at.x).abs().max((e.pos.y - at.y).abs()) / K;
            if moved > STEP_NATIVE && life.first_step.is_none() {
                life.first_step = Some(now);
            }
        }
    }
    lives.into_iter().map(|(id, (l, _))| (id, l)).collect()
}

fn with_release(timing: ReleaseTiming) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.release_timing = timing;
    cfg
}

/// A Blue Goblin Barrel on the Red half: (the first frame the barrel is gone, the goblins' lives).
/// Two identical runs, the engine being deterministic: one finds the vanishing frame, the other
/// follows the goblins from wherever they first exist.
fn barrel(cfg: BattleConfig) -> (u32, BTreeMap<EntityId, Life>) {
    let unit = card_json("GoblinBarrel")["spell"]["spawn"]["character"].as_str().unwrap().to_string();
    let mut s = BattleState::new(1, cfg.clone());
    s.spawn_unit(Team::Blue, "GoblinBarrel", t(900, 1900), None).unwrap();
    let mut seen = false;
    let mut vanish = None;
    for _ in 0..400 {
        s.tick();
        if !s.spells().is_empty() {
            seen = true;
        } else if seen {
            vanish = Some(s.tick_count());
            break;
        }
    }
    let vanish = vanish.expect("the barrel flew and vanished");
    let mut s = BattleState::new(1, cfg);
    s.spawn_unit(Team::Blue, "GoblinBarrel", t(900, 1900), None).unwrap();
    let lives = follow(&mut s, Team::Blue, &unit, &[], vanish + 30);
    (vanish, lives)
}

/// An ordinary Blue deploy of `card`, alone, followed from its first frame.
fn ordinary(card: &str) -> Life {
    let mut s = BattleState::new(1, config());
    s.spawn_unit(Team::Blue, card, t(900, 900), None).unwrap();
    let lives = follow(&mut s, Team::Blue, card, &[], 80);
    assert_eq!(lives.len(), 1, "one {card}");
    *lives.values().next().unwrap()
}

/// A Blue `card` placed at once and killed: (the tick it died in, the death spawn's name, the
/// death spawn's DeathSpawnDeployTime if the row sets one, its units' lives).
fn death(cfg: BattleConfig, card: &str) -> (u32, String, Option<i32>, BTreeMap<EntityId, Life>) {
    let mut s = BattleState::new(7, cfg);
    let idx = s.cards().index(card).unwrap();
    let ds = s.cards().get(idx).death_spawn.expect("data: a death spawn");
    let unit = s.cards().get(ds.unit).name.clone();
    let parent = s.scenario_spawn_now(Team::Blue, card, t(900, 900), None).unwrap();
    for _ in 0..3 {
        s.tick();
    }
    let before: Vec<EntityId> = find_live(&s, Team::Blue, &unit).iter().map(|e| e.id).collect();
    assert!(s.debug_set_hp(parent, 0));
    let died = s.tick_count() + 1;
    let lives = follow(&mut s, Team::Blue, &unit, &before, died + 40);
    assert!(s.entity(parent).is_none(), "the {card} died");
    (died, unit, ds.deploy_time_ms, lives)
}

#[test]
fn a_barrels_goblins_exist_on_its_vanishing_frame_and_deploy_one_frame_longer_than_a_deploy() {
    assert_eq!(config().calib.release_timing, ReleaseTiming::EndOfEventPhase, "the shipped arm this test pins");
    let tick_ms = config().calib.tick_ms;
    let d = card_json("GoblinBarrel")["spell"]["spawn"]["deploy_time_ms"].as_i64().unwrap() as i32;
    let (vanish, lives) = barrel(config());
    let count = card_json("GoblinBarrel")["spell"]["spawn"]["count"].as_i64().unwrap() as usize;
    assert_eq!(lives.len(), count, "the barrel released SpawnCharacterCount units");
    for l in lives.values() {
        assert_eq!(l.first, vanish, "a goblin first exists on {}, the barrel vanished on {vanish}: both clients show them on the vanishing frame", l.first);
        assert_eq!((l.first_deploy_ms, l.first_deploying), (d, true), "on its first frame a released unit has its whole SpawnCharacterDeployTime left: nothing counted down");
        assert_eq!(l.deploying_frames as i32, d / tick_ms, "a released unit deploys DeployTime / TICK_MS frames (15.535.29: 22 for 1100 ms)");
        assert_eq!(l.first_not_deploying, Some(l.first + (d / tick_ms) as u32), "first frame not deploying");
        assert_eq!(l.first_step, Some(l.first + (d / tick_ms) as u32 + 1), "first step one tick after the deploy ends (15.535.29: first + 23)");
    }
    // THE DISCRIMINATOR: an ordinary deploy counts down on its own first frame, so it deploys
    // one frame FEWER than its DeployTime / TICK_MS -- the 15.535.29 figure, 19 frames for 1000 ms.
    let knight_ms = cards_json()["units"]["Knight"]["deploy_time_ms"].as_i64().unwrap() as i32;
    let k = ordinary("Knight");
    assert_eq!(k.deploying_frames as i32, knight_ms / tick_ms - 1, "an ordinary deploy deploys DeployTime / TICK_MS - 1 frames");
    assert_eq!(k.first_step, Some(k.first + (knight_ms / tick_ms) as u32), "an ordinary deploy first steps on first + DeployTime / TICK_MS");
}

#[test]
fn a_battle_rams_barbarians_exist_on_the_tick_it_dies_and_deploy_their_whole_time() {
    let tick_ms = config().calib.tick_ms;
    let (died, unit, deploy, lives) = death(config(), "BattleRam");
    let d = deploy.expect("data: the BattleRam's death spawn sets DeathSpawnDeployTime");
    assert!(!lives.is_empty(), "the BattleRam left its {unit}s");
    for l in lives.values() {
        assert_eq!(l.first, died, "a {unit} first exists on {}, the ram died in tick {died} (the corpus: the ram's last live frame + 1)", l.first);
        assert_eq!(l.first_deploy_ms, d, "its whole DeathSpawnDeployTime on its first frame");
        assert_eq!(l.deploying_frames as i32, d / tick_ms, "the corpus's 20 deploying frames for 1000 ms: one more than an ordinary deploy");
    }
}

#[test]
fn a_golems_golemites_exist_on_the_tick_it_dies_moving_and_step_on_the_next() {
    let (died, unit, deploy, lives) = death(config(), "Golem");
    assert!(deploy.is_none(), "data: the Golem's death spawn leaves DeathSpawnDeployTime blank");
    assert!(!lives.is_empty(), "the Golem left its {unit}s");
    for l in lives.values() {
        assert_eq!(l.first, died, "a {unit} first exists on {}, the Golem died in tick {died}", l.first);
        assert!(!l.first_deploying, "born moving: not deploying on its first frame");
        assert_eq!(l.deploying_frames, 0, "never deploying");
    }
    // born moving and inert on the death frame: the first displacement is on the NEXT tick
    let first_steps: Vec<Option<u32>> = lives.values().map(|l| l.first_step).collect();
    assert!(first_steps.iter().all(|s| s.is_none_or(|t| t > died)), "a {unit} stepped on its own first frame: {first_steps:?}");
}

#[test]
fn under_the_next_spawn_phase_foil_every_release_is_a_tick_late_and_already_counting() {
    let tick_ms = config().calib.tick_ms;
    let d = card_json("GoblinBarrel")["spell"]["spawn"]["deploy_time_ms"].as_i64().unwrap() as i32;
    let (vanish, lives) = barrel(with_release(ReleaseTiming::NextSpawnPhase));
    assert!(!lives.is_empty(), "the foil released no goblins");
    for l in lives.values() {
        assert_eq!(l.first, vanish + 1, "under the foil the goblins appear the tick AFTER the barrel vanished");
        assert_eq!(l.first_deploy_ms, d - tick_ms, "and the countdown already ran on that first frame");
    }
    let (died, _, _, lives) = death(with_release(ReleaseTiming::NextSpawnPhase), "BattleRam");
    for l in lives.values() {
        assert_eq!(l.first, died + 1, "under the foil a death spawn appears a tick late");
    }
}
