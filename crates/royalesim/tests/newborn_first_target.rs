//! A NEWBORN'S FIRST TARGET AND THE TICK'S LANDING SHOTS -- calibration targeting.NEWBORN_FIRST_TARGET, state.rs
//! `create_emissions`, `first_update`, `tick_landing_deaths`.
//!
//! THE READING (client 15.535.29, 966 newborns): a newborn never takes an enemy that is gone on its birth tick (19 of 19
//! passed over; 0 of 966 named an absent id). The engine's periodic emission scans at the end of Move, before the
//! Projectile phase, so it can take an enemy a shot kills later in the tick (sp-form-Tombstone-hero-nopress-s0 t242).
//!
//! The scene: a Blue Tombstone emitting, a Red Knight held near its emission point (hitpoints topped up) and a Red Giant
//! held a little farther; a Blue Musketeer shoots the Knight. A probe finds the Musketeer's spawn delay for which one of
//! its shots lands on a tick the Tombstone emits; on that tick the Knight is set to the shot's damage, so the landing
//! kills it. The newborn's target after that tick:
//!   client15535_tick_survivors: the Giant; scan_at_emission: not the Giant (the dying Knight).
//!
//! PLANT (regression): newborn_takes_tick_deaths -> `a_newborn_passes_over_an_enemy_the_ticks_landing_shot_kills_under_client15535_tick_survivors` red.
//!   RUSTFLAGS='--cfg clash_plant="newborn_takes_tick_deaths"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test newborn_first_target
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, NewbornFirstTarget};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const TOMB: (i32, i32) = (9000, 10000);
const KNIGHT: (i32, i32) = (9000, 13300);
const GIANT: (i32, i32) = (9000, 13900);
const MUSKETEER: (i32, i32) = (13000, 11000);

struct Run {
    /// Ticks (k) on which a Blue Skeleton was emitted.
    emits: Vec<usize>,
    /// Ticks on which a Musketeer shot landed on the Knight.
    lands: Vec<usize>,
    /// The Skeletons emitted on `kill` and their targets after it.
    newborn: Vec<Option<EntityId>>,
    giant: EntityId,
}

/// The scene with the Musketeer spawned `delay` ticks in, under `arm`; `kill`: the tick whose start sets the Knight to
/// the shot's damage.
fn run(arm: NewbornFirstTarget, delay: usize, kill: Option<usize>) -> Run {
    let mut cfg = config();
    cfg.calib.newborn_first_target = arm;
    let mut s = BattleState::new(5, cfg);
    // Blue's princess towers down: no tower shot lands on the Knight (the king's reach stops short of it).
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.scenario_spawn_now(Team::Blue, "Tombstone", at(TOMB), None).expect("the Tombstone");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT), None).expect("the Knight");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", at(GIANT), None).expect("the Giant");
    let mut musk = None;
    let mut out = Run { emits: Vec::new(), lands: Vec::new(), newborn: Vec::new(), giant };
    let skels = |s: &BattleState| s.entities().filter(|e| e.team == Team::Blue && e.card == "Skeleton").map(|e| e.id).collect::<Vec<_>>();
    for k in 0..260usize {
        if k == delay {
            musk = Some(s.scenario_spawn_now(Team::Blue, "Musketeer", at(MUSKETEER), None).expect("the Musketeer"));
        }
        if s.entity(knight).is_none() {
            break;
        }
        assert!(s.debug_set_pos(knight, at(KNIGHT)) && s.debug_set_pos(giant, at(GIANT)));
        if let Some(m) = musk {
            assert!(s.debug_set_pos(m, at(MUSKETEER)));
        }
        let top = s.entity(knight).expect("the Knight").max_hp;
        let dmg = musk.and_then(|m| s.projectiles().iter().find(|p| p.firer == Some(m)).map(|p| p.damage)).unwrap_or(1);
        let hp = if kill == Some(k) { dmg } else { top };
        assert!(s.debug_set_hp(knight, hp));
        let before = skels(&s);
        s.tick();
        let now = skels(&s);
        let new: Vec<EntityId> = now.iter().copied().filter(|id| !before.contains(id)).collect();
        if !new.is_empty() {
            out.emits.push(k);
        }
        if s.entity(knight).map_or(0, |e| e.hp) < hp {
            out.lands.push(k);
        }
        if kill == Some(k) {
            out.newborn = new.iter().map(|id| s.entity(*id).and_then(|e| e.target)).collect();
        }
    }
    out
}

/// The probe: a Musketeer spawn delay and a tick on which his shot lands as the Tombstone emits.
fn probe() -> (usize, usize) {
    for delay in 0..80 {
        let r = run(NewbornFirstTarget::ScanAtEmission, delay, None);
        if let Some(&t) = r.emits.iter().find(|t| **t > delay + 30 && r.lands.contains(t)) {
            return (delay, t);
        }
    }
    panic!("the scene drifted: no shot lands on an emission tick");
}

/// Plant: newborn_takes_tick_deaths.
#[test]
fn a_newborn_passes_over_an_enemy_the_ticks_landing_shot_kills_under_client15535_tick_survivors() {
    let (delay, t) = probe();
    let new = run(NewbornFirstTarget::Client15535TickSurvivors, delay, Some(t));
    assert!(!new.newborn.is_empty(), "the scene drifted: no Skeleton emitted on the kill's tick");
    assert!(new.newborn.iter().all(|x| *x == Some(new.giant)), "client15535_tick_survivors: the newborn did not take the Giant: {:?}", new.newborn);
    // NOT VACUOUS: scan_at_emission takes the dying Knight.
    let old = run(NewbornFirstTarget::ScanAtEmission, delay, Some(t));
    assert!(old.newborn.iter().any(|x| *x != Some(old.giant)), "scan_at_emission: the newborn took the Giant anyway: {:?}", old.newborn);
}

#[test]
fn the_shipped_arm_scans_at_emission() {
    assert_eq!(Calib::shipped().newborn_first_target, NewbornFirstTarget::ScanAtEmission);
}
