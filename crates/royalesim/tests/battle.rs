//! A full scripted battle on the real card data, with every-tick invariants,
//! and the determinism gate.
//!
//! WHAT IT PINS: that the whole tick loop -- Target -> Attack -> Path -> Move ->
//! Resolve (the measured 16.402 order: the attacks before the moves) -- runs end to
//! end on the real card data with real units in it, that the every-tick invariants
//! hold for the length of a battle, and that two runs from one seed are
//! bit-identical. Unit tests of the individual phases do not reach any of that.
//!
//! WHAT IT CANNOT CATCH: whether the battle is RIGHT. "Plausible crowns" is a smoke
//! bound, not fidelity. Fidelity is measured against recorded ground truth, not
//! here; see tests/oracle2026.rs and data/calibration.json.
mod common;

use royalesim::state::Outcome;
use royalesim::Team;
use common::*;

#[test]
fn scripted_battle_finishes_with_plausible_result() {
    let r = run_scripted(0xC1A5, true, None);
    let s = &r.final_state;
    let cap = tick_cap(s.config());
    println!(
        "scripted battle: ticks={} outcome={:?} crowns={:?} towers blue={:?} red={:?} plays={:?} max_live={} worst_overlap={}% worst_runs(>50%,>100%)={:?} ticks_checked={} ground_troop_ticks={}",
        s.tick_count(),
        s.outcome(),
        s.crowns(),
        s.tower_hp(Team::Blue),
        s.tower_hp(Team::Red),
        r.plays,
        r.max_live,
        r.inv.worst_pct,
        r.inv.worst_run,
        r.inv.ticks_checked,
        r.inv.ground_troop_ticks
    );
    // Vacuity guard: the invariants must have seen a real battle.
    assert_eq!(r.inv.ticks_checked, s.tick_count(), "invariants skipped ticks");
    assert!(r.inv.ground_troop_ticks > 10_000, "invariants saw almost no ground troops: {}", r.inv.ground_troop_ticks);
    assert!(s.is_done(), "battle did not finish within the cap of {cap} ticks");
    assert!(r.plays[0] >= 10 && r.plays[1] >= 10, "script barely played: {:?}", r.plays);
    let cr = s.crowns();
    assert!(cr[0] <= 3 && cr[1] <= 3);
    match s.outcome().unwrap() {
        Outcome::Winner(Team::Blue) => assert!(cr[0] > cr[1], "Blue won with crowns {cr:?}"),
        Outcome::Winner(Team::Red) => assert!(cr[1] > cr[0], "Red won with crowns {cr:?}"),
        Outcome::Draw => assert_eq!(cr[0], cr[1], "draw with crowns {cr:?}"),
    }
    // Units must actually have reached and hit towers on both sides; a battle in
    // which no tower took damage means movement or targeting never connected.
    let full = |team: Team| {
        let db = s.cards();
        let k = db.get(db.index("KingTower").unwrap()).hitpoints;
        let p = db.get(db.index("PrincessTower").unwrap()).hitpoints;
        let hp = s.tower_hp(team);
        hp[0] == k && hp[1] == p && hp[2] == p
    };
    assert!(!full(Team::Blue) && !full(Team::Red), "a side's towers were never damaged");
}

#[test]
fn determinism_same_seed_same_script_identical_hash_every_tick() {
    let a = run_scripted(77, false, None);
    let b = run_scripted(77, false, None);
    assert!(a.hashes.len() > 1000, "battle too short to be a determinism test: {}", a.hashes.len());
    assert_eq!(a.hashes.len(), b.hashes.len(), "runs ended on different ticks");
    if let Some(k) = a.hashes.iter().zip(b.hashes.iter()).position(|(x, y)| x != y) {
        panic!("state_hash diverged at tick {k} of {}", a.hashes.len());
    }
    // The hash must actually move: a constant hash would make this test vacuous.
    let distinct: std::collections::BTreeSet<u64> = a.hashes.iter().copied().collect();
    assert!(distinct.len() * 10 > a.hashes.len() * 9, "state_hash barely changes: {} distinct of {}", distinct.len(), a.hashes.len());
}

#[test]
fn determinism_gate_detects_a_one_unit_perturbation() {
    // The gate above is only evidence if a one-unit difference in state shows
    // up in the hash sequence. This is the in-test plant: one subtile, once.
    let a = run_scripted(77, false, None);
    let b = run_scripted(77, false, Some(300));
    let k = a.hashes.iter().zip(b.hashes.iter()).position(|(x, y)| x != y);
    assert!(k.is_some(), "a 1-subtile perturbation at tick 300 never changed state_hash");
    assert!(k.unwrap() <= 301, "perturbation at tick 300 first detected only at tick {}", k.unwrap());
}

#[test]
fn different_seeds_are_allowed_to_differ_only_through_the_rng() {
    // With shuffle_decks off and no rng consumer in the tick, two seeds must
    // produce the same battle except for the rng bit-state in the hash. This
    // pins down that nothing else in the tick reads the seed.
    let a = run_scripted(1, false, None);
    let b = run_scripted(2, false, None);
    assert_eq!(a.final_state.tick_count(), b.final_state.tick_count());
    assert_eq!(a.final_state.tower_hp(Team::Blue), b.final_state.tower_hp(Team::Blue));
    assert_eq!(a.final_state.tower_hp(Team::Red), b.final_state.tower_hp(Team::Red));
    assert_ne!(a.hashes[0], b.hashes[0], "the rng state must be part of the hash");
}

#[test]
fn scripted_battle_with_spells_finishes_and_is_deterministic() {
    // Both sides carry spells (every thin-slice spell appears across the two decks)
    // and the script casts them at enemy units and towers, with the every-tick
    // invariants on. Two runs of one seed must hash identically on every tick.
    // WHAT IT DOES NOT GATE, measured: the knockback-safety plants
    // no_water_resolution and knock_ignores_footprints leave this test GREEN -- this
    // script never pushes a unit toward the river or a building hard enough to matter.
    // Knockback safety is tests/knockback.rs, which those plants do turn red.
    let r = run_scripted_with(spell_scripted_config(), 0xC1A5, true, None);
    let s = &r.final_state;
    println!(
        "scripted battle WITH SPELLS: ticks={} outcome={:?} crowns={:?} towers blue={:?} red={:?} plays={:?} spell_casts={:?} spell_ticks={} max_live={} worst_overlap={}% worst_runs={:?}",
        s.tick_count(),
        s.outcome(),
        s.crowns(),
        s.tower_hp(Team::Blue),
        s.tower_hp(Team::Red),
        r.plays,
        r.spell_casts,
        r.spell_ticks,
        r.max_live,
        r.inv.worst_pct,
        r.inv.worst_run
    );
    assert!(s.is_done(), "the spell battle did not finish");
    assert_eq!(r.inv.ticks_checked, s.tick_count());
    assert!(r.spell_casts[0] >= 10 && r.spell_casts[1] >= 10, "vacuous: spell casts {:?}", r.spell_casts);
    assert!(r.spell_ticks > 500, "vacuous: spells were live on only {} ticks", r.spell_ticks);
    let cr = s.crowns();
    match s.outcome().unwrap() {
        Outcome::Winner(Team::Blue) => assert!(cr[0] > cr[1]),
        Outcome::Winner(Team::Red) => assert!(cr[1] > cr[0]),
        Outcome::Draw => assert_eq!(cr[0], cr[1]),
    }
    let again = run_scripted_with(spell_scripted_config(), 0xC1A5, false, None);
    assert_eq!(r.hashes.len(), again.hashes.len());
    if let Some(k) = r.hashes.iter().zip(again.hashes.iter()).position(|(x, y)| x != y) {
        panic!("spell battle state_hash diverged at tick {k}");
    }
}
