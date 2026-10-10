//! spawner.DEATH_BOMB_TIMING_SCOPE: which death bombs spawner.DEATH_BOMB_SPAWN_TIMING times (spell.rs `step_spells`,
//! the Flight arm's `timed`).
//!
//! THE READING (client 15.535.29): every plain bomb that hit a unit in the captures, 5 of 5 (three Balloons', two Giant
//! Skeletons'), landed 61 ticks after its parent's last frame: the fuse, 60 ticks, on its last tick, as a container lands
//! under at_fuse_end (the shipped value). The engine's landed on the tick after.
//!
//! The scene: tests/death_bomb.rs's: a Blue Giant Skeleton put down on Red's princess tower and killed. Pinned: the ticks
//! from the death to the tower's hit, under client15535_every_bomb the fuse's ticks, under containers one more.
//!
//! A BOMB A BUTTON DROPS (the Mighty Miner's, `CardDef::dropped_by_ability`) keeps the tick after its fuse under both arms
//! (client 15.535.29, sp-champ-MightyMiner-s0 t226): tests/mighty_miner.rs's scene, its bomb's 332 on the Knight on P + 30.
//!
//! PLANT (regression): plain_bomb_lands_late -> `a_plain_bomb_lands_on_its_fuses_last_tick` red;
//!   ability_bomb_timed -> `a_bomb_a_button_drops_lands_on_the_tick_after_its_fuse_under_either_arm` red.
//!   RUSTFLAGS='--cfg clash_plant="plain_bomb_lands_late"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   death_bomb_timing_scope
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DeathBombTimingScope};
use royalesim::Team;

/// (the ticks from the Giant Skeleton's death tick to its bomb's hit on the tower, the bomb's fuse in ticks).
fn scene(arm: DeathBombTimingScope) -> (u32, u32) {
    let mut cfg = config();
    cfg.calib.death_bomb_timing_scope = arm;
    // The file's scenes are client 15.535.29's: a container's units and its hit on T + 12 (at_fuse_end, the 15.535.29
    // capture's arm; the shipped units_at_fuse_end is client 16.402's, tests/skeleton_barrel.rs).
    cfg.calib.death_bomb_spawn_timing = royalesim::state::DeathBombSpawnTiming::AtFuseEnd;
    let tick = cfg.calib.tick_ms;
    let mut s = BattleState::new(11, cfg);
    let tower = s.tower_ids(Team::Red)[1].expect("the tower stands at setup");
    let p = s.entity(tower).expect("a live tower").pos;
    let before = s.entity(tower).expect("a live tower").hp;
    let id = s.scenario_spawn_now(Team::Blue, "GiantSkeleton", p, None).expect("the Giant Skeleton");
    let db = cards();
    let ds = db.get(db.index("GiantSkeleton").expect("the card loads")).death_spawn.expect("its death spawn");
    let fuse = db.get(ds.unit).death_bomb_fuse_ms().expect("its bomb") / tick;
    assert!(s.debug_set_hp(id, 0), "could not kill it");
    s.tick();
    let death = s.tick_count();
    assert!(s.entity(id).is_none(), "the scene drifted: it did not die");
    let mut waited = 0;
    while s.entity(tower).expect("the tower outlives the bomb").hp == before {
        assert!(waited < 400, "the scene drifted: the bomb never went off");
        s.tick();
        waited += 1;
    }
    (s.tick_count() - death, fuse as u32)
}

/// Plant: plain_bomb_lands_late.
#[test]
fn a_plain_bomb_lands_on_its_fuses_last_tick() {
    let (ticks, fuse) = scene(DeathBombTimingScope::Client15535EveryBomb);
    assert_eq!(ticks, fuse, "client15535_every_bomb: the bomb landed {ticks} ticks after the death (fuse {fuse} ticks)");
}

#[test]
fn the_old_arm_lands_it_on_the_tick_after() {
    let (ticks, fuse) = scene(DeathBombTimingScope::Containers);
    assert_eq!(ticks, fuse + 1, "containers: the bomb landed {ticks} ticks after the death (fuse {fuse} ticks)");
}

#[test]
fn the_shipped_arm_times_containers_alone() {
    assert_eq!(Calib::shipped().death_bomb_timing_scope, DeathBombTimingScope::Containers);
}

/// tests/mighty_miner.rs's scene under `arm`: the row (P + k) on which the held Knight first loses the bomb's 332 (441 with
/// a tower arrow).
fn miner_bomb_row(arm: DeathBombTimingScope) -> u32 {
    let deck: Vec<String> = ["MightyMiner", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|s| s.to_string()).collect();
    let mut cfg = config();
    cfg.calib.death_bomb_timing_scope = arm;
    // the 15.535.29 scene's fuse (spawner.ABILITY_BOMB_FUSE's old arm; tests/mighty_miner.rs holds the new one)
    cfg.calib.ability_bomb_fuse = royalesim::state::AbilityBombFuse::Client15535TickAfterFuse;
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let (at, kat) = (Vec2::new(11314 * K, 13178 * K), Vec2::new(9314 * K, 13178 * K));
    let mm = s.scenario_spawn_now(Team::Blue, "MightyMiner", at, None).expect("the Mighty Miner");
    let kn = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("a red Knight");
    for _ in 0..40 {
        assert!(s.debug_set_pos(mm, at) && s.debug_set_pos(kn, kat));
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for k in 1..=40u32 {
        let before = s.entity(kn).expect("the Knight lives").hp;
        assert!(s.debug_set_pos(kn, kat));
        s.tick();
        let lost = before - s.entity(kn).expect("the Knight lives").hp;
        if lost == 332 || lost == 441 {
            return k;
        }
    }
    panic!("the scene drifted: the bomb never struck the Knight");
}

/// Plant: ability_bomb_timed.
#[test]
fn a_bomb_a_button_drops_lands_on_the_tick_after_its_fuse_under_either_arm() {
    assert_eq!(miner_bomb_row(DeathBombTimingScope::Containers), 30, "containers: the Mighty Miner's bomb on P + 30");
    assert_eq!(miner_bomb_row(DeathBombTimingScope::Client15535EveryBomb), 30, "client15535_every_bomb: the Mighty Miner's bomb on P + 30");
}
