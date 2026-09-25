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

/// THE KNOWN CROWD DEFECT, PINNED ON A CROWD THIS TEST BUILDS.
///
/// Under `movement.ATTACKING_UNIT_MOVEMENT = frozen`, the arm that shipped until
/// 2026-09-24 and that this test runs BY NAME, the engine skips the whole move pass for a
/// unit whose attack phase holds it, so an attacking crowd is never separated -- against a
/// game whose own crowds run six ticks above 150 % (222 pairs measured on the replay
/// corpus, one outlier at 120). The shipped arm is separation_only.
///
/// THIS USED TO MEASURE A PILE-UP THAT THE SCRIPTED BATTLE HAPPENED TO PRODUCE, pinned at 87
/// consecutive ticks above 150 %. That number was scenario luck, and two changes on
/// 2026-09-23 proved it -- neither of which touches `ATTACKING_UNIT_MOVEMENT`:
///
///     deploy lockout   retarget arm    worst run above 150 %
///     0 (old)          reset_always    87     <- what the 87 was measured on
///     0 (old)          keep_when_dead   2
///     90               reset_always     2
///     90               keep_when_dead   5     <- what ships now
///
/// Either change alone collapses it forty-fold, because both move the opening of the battle
/// and the clump formed in the opening. A characterisation two unrelated changes can erase
/// is not watching the defect; it is watching an accident, and it would have reported "the
/// crowd defect was fixed, tighten the tolerance" when nothing of the kind had happened.
///
/// `worst_pct` is no substitute, and that was checked rather than assumed: under
/// `separation_only` -- the arm that FIXES this -- the depth still reads 190 to 193 %. A
/// bound on the depth would have watched nothing at all.
///
/// So the crowd is now BUILT (`common::run_crowd`): fifteen skeletons placed on top of a
/// Giant, out of every tower's range. Measured across the same four cells, the built crowd
/// reads 55 ticks in ALL FOUR and 9 under `separation_only` in all four -- indifferent to
/// both changes, and separating the shipped arm from the fixed one six to one.
///
/// It still fails in BOTH directions. If the defect grows, `run_crowd`'s own invariant check
/// fails on CLIENT16402_TOLERANCE's 90-tick limit. If someone fixes it, the lower bound here
/// fails and says to tighten that limit toward the game's six.
#[test]
fn the_frozen_foil_still_packs_the_crowd() {
    // THE FOIL, RUN BY NAME. `frozen` was the shipped arm until 2026-09-24 and this test pinned
    // the crowd it leaves packed as "the known defect" of whatever shipped. With separation_only
    // shipped, reading the shipped arm would have asked the fixed engine to reproduce the bug.
    // It now asks the arm that HAS the defect, so the characterisation still characterises it.
    let mut cfg = config();
    cfg.calib.attacking_unit_movement = royalesim::state::AttackingUnitMovement::Frozen;
    let inv = common::run_crowd(cfg, 200);
    let worst = inv.worst_run[1];
    assert!(worst >= 40, "the frozen foil leaves the built crowd packed for only {worst} ticks above 150 %, against the 55 it was pinned at, so the foil no longer shows the defect it exists to show");
    assert!(inv.worst_pct >= 150, "no pair exceeds 150 % under the frozen foil ({} %), so this characterisation is watching nothing", inv.worst_pct);
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
fn the_shipped_arm_keeps_the_crowd_apart_where_frozen_does_not() {
    // THE SHIPPED ARM AGAINST THE FOIL, both by what they do. separation_only ships since
    // 2026-09-24 (movement.ATTACKING_UNIT_MOVEMENT, measured: 61.60 % against 58.99 % on the
    // corpus). If the ledger ever went back to frozen, shipped and foil would be the same arm and
    // this would fail, which is the point: the comparison is the guard on the value.
    //
    // A MARGIN, NOT A BARE `<`. On the emergent crowd this compared 2 against 5 and passed on
    // three ticks, which is indistinguishable from noise in a quantity that had been 87. On the
    // built crowd it is 9 against 55, so a factor of two is asked for and there is room for it.
    let mut foil = config();
    foil.calib.attacking_unit_movement = royalesim::state::AttackingUnitMovement::Frozen;
    let frozen = common::run_crowd(foil, 200);
    let shipped = common::run_crowd(config(), 200);
    println!("built crowd: shipped run {:?} pct {} against frozen run {:?} pct {}", shipped.worst_run, shipped.worst_pct, frozen.worst_run, frozen.worst_pct);
    assert!(shipped.worst_run[1] * 2 < frozen.worst_run[1], "the shipped arm leaves the crowd packed nearly as long as the frozen foil does: {} ticks above 150 % against {}", shipped.worst_run[1], frozen.worst_run[1]);
}
