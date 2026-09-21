//! THROUGHPUT -- ticks per second, single-threaded, with the live entity count
//! beside every number.
//!
//! Ignored by default (a timing is not a pass/fail property of a debug build).
//! Run it on the release profile:
//!
//! ```text
//! cargo test --release --test throughput -- --ignored --nocapture
//! ```
//!
//! Each scenario runs for a bounded wall time of well under a second.
mod common;

use royalesim::state::BattleState;
use royalesim::{PathModel, Team};
use common::*;
use std::time::Instant;

/// Per-phase share of the time, when built with --cfg clash_profile.
#[allow(unexpected_cfgs)]
fn phase_breakdown() {
    #[cfg(clash_profile)]
    {
        let ns = royalesim::state::profile::take();
        let total: u128 = ns.iter().sum::<u128>().max(1);
        let parts: Vec<String> = royalesim::TICK_PHASES
            .iter()
            .zip(ns.iter())
            .map(|(p, n)| format!("{p:?} {}%", n * 100 / total))
            .collect();
        println!("    phases: {}", parts.join(", "));
    }
}

struct Sample {
    ticks: u64,
    entity_ticks: u64,
    secs_nanos: u128,
}

fn report(name: &str, s: &Sample) {
    let tps = (s.ticks as u128) * 1_000_000_000 / s.secs_nanos.max(1);
    let mean_live = s.entity_ticks / s.ticks.max(1);
    let ns_per_entity_tick = s.secs_nanos / (s.entity_ticks.max(1) as u128);
    println!("{name}: {} ticks in {} ms = {tps} ticks/s, mean live entities {mean_live}, {ns_per_entity_tick} ns per entity-tick", s.ticks, s.secs_nanos / 1_000_000);
}

/// Hold roughly `troops_per_side` troops per side in a brawl at the centre of
/// the arena, re-spawning as they die, and time `ticks` ticks.
fn brawl(model: PathModel, troops_per_side: usize, ticks: u32) -> Sample {
    let mut cfg = config();
    cfg.path_model = model;
    let mut s = BattleState::new(7, cfg);
    let roster = ["Knight", "Valkyrie", "Musketeer", "Archer", "Giant", "Minions", "Wizard", "HogRider"];
    let mut k = 0usize;
    let mut sample = Sample { ticks: 0, entity_ticks: 0, secs_nanos: 0 };
    for _ in 0..ticks {
        // Top up outside the timed region.
        for team in [Team::Blue, Team::Red] {
            let have = s.entities().filter(|e| e.team == team && !e.kind.is_building()).count();
            if have < troops_per_side {
                let card = roster[k % roster.len()];
                k += 1;
                let x100 = 300 + ((k * 137) % 1200) as i32;
                let blue = t(x100, 1300);
                let p = if team == Team::Blue { blue } else { mirror(&s, blue) };
                let _ = s.spawn_unit(team, card, p, None);
            }
        }
        let live = s.live_count() as u64;
        let t0 = Instant::now();
        s.tick();
        sample.secs_nanos += t0.elapsed().as_nanos();
        sample.ticks += 1;
        sample.entity_ticks += live;
        if s.is_done() {
            break;
        }
    }
    sample
}

#[test]
#[ignore = "timing; run with --release -- --ignored --nocapture"]
fn throughput_scripted_battle() {
    let cfg = scripted_config();
    let mut s = BattleState::new(0xC1A5, cfg);
    let mut script = Script::new(40);
    let mut sample = Sample { ticks: 0, entity_ticks: 0, secs_nanos: 0 };
    let mut peak = 0;
    while !s.is_done() {
        script.step(&mut s);
        let live = s.live_count();
        peak = peak.max(live);
        let t0 = Instant::now();
        s.tick();
        sample.secs_nanos += t0.elapsed().as_nanos();
        sample.ticks += 1;
        sample.entity_ticks += live as u64;
    }
    report(&format!("scripted battle (full 6 min, peak live {peak})"), &sample);
}

#[test]
#[ignore = "timing; run with --release -- --ignored --nocapture"]
fn throughput_brawl_20_to_40_entities() {
    for model in [PathModel::LaneSnap, PathModel::DiagonalLookahead, PathModel::GridAStar] {
        for per_side in [8usize, 16] {
            let smp = brawl(model, per_side, 3000);
            report(&format!("brawl {model:?}, {per_side} troops/side + towers"), &smp);
            phase_breakdown();
        }
    }
}

#[test]
#[ignore = "timing; run with --release -- --ignored --nocapture"]
fn throughput_scripted_battle_with_spells() {
    let mut s = BattleState::new(0xC1A5, spell_scripted_config());
    let mut script = Script::new(40);
    let mut sample = Sample { ticks: 0, entity_ticks: 0, secs_nanos: 0 };
    let (mut peak, mut spell_ticks) = (0, 0u64);
    while !s.is_done() {
        script.step(&mut s);
        let live = s.live_count();
        peak = peak.max(live);
        let t0 = Instant::now();
        s.tick();
        sample.secs_nanos += t0.elapsed().as_nanos();
        sample.ticks += 1;
        sample.entity_ticks += live as u64;
        spell_ticks += u64::from(!s.spells().is_empty());
    }
    report(&format!("scripted battle WITH SPELLS (peak live {peak}, spells live on {spell_ticks} ticks, casts {:?})", script.spell_casts), &sample);
}
