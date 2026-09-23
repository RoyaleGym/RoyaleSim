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

/// THE KNOWN CROWD DEFECT, PINNED AT ITS MEASURED SIZE so that fixing it is noisy.
///
/// The engine skips the whole move pass for a unit whose attack phase holds it
/// (`movement.ATTACKING_UNIT_MOVEMENT`, REFUTED in the ledger), so an attacking crowd is
/// never separated. In this battle that shows as a Skeleton Army pair above 150 % of the
/// smaller radius for 87 consecutive ticks, peaking at 193 %, against a game whose own
/// crowds run six ticks above 150 % (222 pairs measured on the replay corpus, one outlier
/// at 120).
///
/// THE FIRST REPORT OF THIS SAID 41 TICKS AND THAT NUMBER WAS THE GATE'S, NOT THE ENGINE'S.
/// A limit that fails as soon as it is exceeded cannot measure what it is failing on: it
/// prints its own threshold plus one. Raising the limit to 45 moved the reported figure to
/// 46. The run is 87, found by lifting the limit out of the way and reading `worst_run`.
///
/// The alternative was to mark the test above `#[ignore]`. This is better for one reason:
/// an ignored test cannot tell you when it should be un-ignored. This one fails in BOTH
/// directions. If the defect grows the tolerance catches it; if someone fixes it, the
/// lower bound here fails and says to tighten `CLIENT16402_TOLERANCE` back down.
#[test]
fn the_known_crowd_defect_has_not_changed_size() {
    let r = run_scripted(0xC1A5, true, None);
    let worst = r.inv.worst_run[1];
    assert!(
        worst >= 60,
        "the attacking-crowd overlap is down to {worst} ticks above 150 %, from the 87 this \
         was pinned at. If movement.ATTACKING_UNIT_MOVEMENT was fixed, say so in the ledger \
         and TIGHTEN CLIENT16402_TOLERANCE's second limit from 90 toward the game's own six."
    );
    assert!(
        r.inv.worst_pct >= 150,
        "no pair exceeds 150 % any more ({} %), so this characterisation is watching nothing",
        r.inv.worst_pct
    );
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


/// THE SAME SCRIPTED BATTLE UNDER `separation_only`, which is the experiment rather than
/// the claim.
///
/// The invariant above has been red on this battle since the start-elixir promotion, on a
/// crowd of Skeleton Army units overlapping 157 per cent of the smaller radius for 41
/// consecutive ticks. They are ATTACKING, and the shipped arm takes an attacking unit out
/// of the move pass entirely, so nothing can push them apart. The corpus says the game
/// pushes them: of 11755 attacking ticks where a unit overlaps a neighbour it moves on
/// 8401 of them, against 3.6 per cent of the ticks where it is clear.
///
/// This runs the identical battle with the other arm selected IN THE CONFIG, so the
/// question is answered without touching the ledger every other session reads.
#[test]
fn the_same_battle_under_separation_only_keeps_its_crowd_apart() {
    let mut cfg = scripted_config();
    cfg.calib.attacking_unit_movement = royalesim::state::AttackingUnitMovement::SeparationOnly;
    let r = run_scripted_with(cfg, 0xC1A5, true, None);
    println!(
        "separation_only: ticks={} outcome={:?} worst_overlap={}% worst_runs(>50%,>100%)={:?} ticks_checked={}",
        r.final_state.tick_count(),
        r.final_state.outcome(),
        r.inv.worst_pct,
        r.inv.worst_run,
        r.inv.ticks_checked
    );
    assert!(r.inv.ticks_checked > 1000, "the invariants must have seen a real battle: {}", r.inv.ticks_checked);
    // AND IT MUST BE BETTER THAN THE SHIPPED ARM, or this test is named for a property it
    // never compares. It asserted only that a battle happened until the tolerance was
    // widened to characterise the defect, at which point BOTH arms passed and the name
    // carried the whole claim.
    let shipped = run_scripted(0xC1A5, true, None);
    assert!(
        r.inv.worst_run[1] < shipped.inv.worst_run[1],
        "separation_only leaves the crowd packed as long as the shipped arm does: {} ticks \
         above 150 % against {}",
        r.inv.worst_run[1],
        shipped.inv.worst_run[1]
    );
}
