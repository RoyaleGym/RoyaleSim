//! A ROLL THAT RELEASES UNITS (card.rs `SpellShape::Rolling { spawn }`, `SpellShape::release`; spell.rs
//! `release_units`): the Barbarian Barrel.
//!
//! THE LAW, measured on the Barbarian Barrel (4 of 4 casts on the 16.402 corpus, the 15.535.29 scenario runs):
//!   - the roll is the Log's (airborne from MinDistance behind the landing, then rolling ProjectileRange along the
//!     caster's forward axis);
//!   - where it stops it releases ONE Barbarian (the row's SpawnCharacterCount is blank: one unit on a roll), with the
//!     row's SpawnCharacterDeployTime, at the spell's level;
//!   - spells.ROLL_FIRST_STEP: the roll stands unmoved on the landing tick and first steps on the next
//!     (tick_after_landing, measured); the shipped on_landing_tick steps at once, so the roll stops, and releases,
//!     one tick earlier.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the Barbarian Barrel as a roll releasing one Barbarian;
//!   2. a cast releases exactly one Barbarian, on the roll's end point, on the tick the roll ends;
//!   3. under tick_after_landing the release comes exactly one tick later than under on_landing_tick;
//!   4. the Barbarian stands its SpawnCharacterDeployTime (20 ticks) before its first step.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test barb_log`):
//!   * `roll_spawn_unread` -- the loader drops the roll's SpawnCharacter: (1) and (2) go red.
//!   * `roll_release_dropped` -- the roll stops and releases nothing: (2) goes red.
//!   * `roll_steps_on_landing` -- the first step on the landing tick whatever the key: (3) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{SpellPlacement, SpellShape};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, RollFirstStep};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Plant: roll_spawn_unread.
#[test]
fn the_loader_reads_the_barbarian_barrel_as_a_roll_releasing_one_barbarian() {
    let s = BattleState::new(0, config());
    let spell = card_stat(&s, "BarbLog").spell.as_ref().expect("the Barbarian Barrel loads as a spell");
    assert_eq!(spell.placement, SpellPlacement::TroopTerritory { on_buildings: true });
    let SpellShape::Rolling { range, hit, spawn, .. } = &spell.shape else { panic!("BarbLog: {:?}", spell.shape) };
    assert_eq!((*range, hit.damage), (4500 * K, 91), "the roll row's ProjectileRange and Damage");
    let sp = spawn.as_ref().expect("the roll releases a unit");
    assert_eq!((s.cards().get(sp.unit).name.as_str(), sp.count, sp.deploy_time_ms), ("Barbarian", 1, Some(1000)));
    assert!(s.cards().get(sp.unit).summon_only, "the Barbarian is a unit the spell releases, not a card");
    assert_eq!(spell.shape.release().map(|r| r.count), Some(1), "`release` reads the roll's spawn");
}

/// One Blue Barbarian Barrel cast at `tap` under `first`: (the release tick k, counted from the cast tick k = 0; the
/// Barbarian; the roll's end point), and the Barbarian's position on each of the 30 ticks after it appears.
fn cast(first: RollFirstStep, tap: (i32, i32)) -> (u32, EntityId, Vec2, Vec<Vec2>) {
    let mut cfg: BattleConfig = config();
    cfg.calib.roll_first_step = first;
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(Team::Blue, "BarbLog", at(tap), None).expect("cast the Barbarian Barrel");
    let mut last_roll = None;
    for k in 0..80u32 {
        s.tick();
        // the roll's end point: where it stands plus what it has left, along the caster's forward axis
        let fwd = royalesim::spell::forward_dy(Team::Blue);
        let rolling = s.spells().iter().find_map(|sp| match &sp.motion {
            SpellMotion::Rolling { pos, travelled, len, .. } => Some(Vec2::new(pos.x, pos.y + fwd * (len - travelled))),
            _ => None,
        });
        let barbs: Vec<(EntityId, Vec2)> = s.entities().filter(|v| v.card == "Barbarian").map(|v| (v.id, v.pos)).collect();
        if let Some(p) = rolling {
            last_roll = Some(p);
            assert!(barbs.is_empty(), "a Barbarian stands while the roll still rolls (tick {k})");
        }
        if !barbs.is_empty() {
            assert_eq!(barbs.len(), 1, "the barrel released {} Barbarians", barbs.len());
            let (id, _) = barbs[0];
            let mut walk = Vec::new();
            for _ in 0..30 {
                walk.push(s.entity(id).map_or(Vec2::default(), |v| v.pos));
                s.tick();
            }
            return (k, id, last_roll.expect("the roll never stood"), walk);
        }
    }
    panic!("the barrel released no Barbarian in 80 ticks");
}

/// Plant: roll_release_dropped.
#[test]
fn the_roll_releases_one_barbarian_on_its_end_point() {
    let (_, _, end, walk) = cast(RollFirstStep::OnLandingTick, (9000, 6000));
    assert_eq!(walk[0], end, "the Barbarian stands where the roll stopped");
}

/// Plant: roll_steps_on_landing.
#[test]
fn under_tick_after_landing_the_release_comes_one_tick_later() {
    let (a, _, end_a, _) = cast(RollFirstStep::OnLandingTick, (9000, 6000));
    let (b, _, end_b, _) = cast(RollFirstStep::TickAfterLanding, (9000, 6000));
    assert_eq!(end_a, end_b, "the roll stops on the same point under both arms");
    assert_eq!(b, a + 1, "on_landing_tick releases on {a}, tick_after_landing on {b}");
}

#[test]
fn the_barbarian_stands_its_spawn_deploy_time() {
    let (_, _, _, walk) = cast(RollFirstStep::OnLandingTick, (9000, 6000));
    let first_move = walk.windows(2).position(|w| w[0] != w[1]);
    assert!(first_move.is_some_and(|m| m >= 20), "the Barbarian moved {first_move:?} ticks after it appeared; its deploy is 1000 ms");
}
