//! royalesim -- a deterministic, integer-only Clash Royale battle simulator.
//!
//! READ data/calibration.json BEFORE CHANGING ANY NUMBER IN THIS CRATE.
//! Every physics constant lives there with its provenance and how well it is
//! known. A number hardcoded here is a number nobody can audit later.
//!
//! THE THREE INVARIANTS
//!
//! 1. NO FLOATS. Not in state, not in intermediate arithmetic, not in tests.
//!    `fixed.rs` is the whole geometry layer. A float in the tick path makes
//!    runs unreproducible, and an unreproducible run makes every disagreement
//!    with the real game unattributable.
//!
//! 2. SIMULTANEITY IS ORDER-INDEPENDENT BY CONSTRUCTION. Damage is accumulated
//!    into a buffer during the tick and applied in one pass at the end; spawns
//!    and deaths go on deferred queues. This is not stylistic. The previous
//!    engine updated entities in dict-insertion order and so player 0's units
//!    always resolved first -- two identical Knights fought to exactly 157 HP
//!    each and then player 0's survived, every time. That is a systematic
//!    asymmetry an RL agent finds in hours and exploits forever, and it poisons
//!    self-play at the root.
//!
//! 3. EVERY DISPUTED MECHANIC IS BEHIND A SWAPPABLE STRATEGY. The pathfinder,
//!    the collision push model and the building footprint model are all things
//!    no public source settles. They are enums, selected at runtime, with all
//!    candidates implemented -- so a measurement promotes a constant instead of
//!    triggering a rewrite.
//!
//! THE TICK, IN ORDER. This ordering was MEASURED on the live 16.402 captures
//! (the corpus of 2026-09-20): every attack update runs BEFORE any unit moves, the move updates
//! then run one after the other in creation order, and the per-unit character
//! update (state machine, deploy countdown) runs AFTER the move pass. `TICK_PHASES`
//! is that order; `LEGACY_TICK_PHASES` is the order this engine invented before the
//! measurement (Move before Attack, the countdown in Upkeep), kept runnable behind
//! calibration match.TICK_ORDER. It is documented and testable rather than
//! emergent, and it is built so that the parts nobody can verify cannot introduce a
//! bias.

pub mod fixed;
pub mod arena;
pub mod entity;
pub mod card;
pub mod target;
pub mod path;
pub mod path2026;
pub mod path16402;
pub mod jump16402;
pub mod move16402;
pub mod collide;
pub mod combat;
pub mod spell;
pub mod state;
pub mod py;

use pyo3::prelude::*;

/// Which side an entity belongs to. Blue defends the low-y side.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum Team {
    Blue = 0,
    Red = 1,
}

impl Team {
    #[inline]
    pub fn other(self) -> Team {
        match self {
            Team::Blue => Team::Red,
            Team::Red => Team::Blue,
        }
    }
}

/// A stable handle to an entity. Generational, so a handle to a dead entity
/// cannot be confused with a live one that reused its slot -- the bug class that
/// makes "my target died and I attacked its replacement" happen.
// Ord (by index, then generation) exists only so a SET of ids can be kept sorted
// (spell.rs rolling hit sets) and hash the same whatever order it was filled in. No
// tie-break may use it: slot order is not a seat-invariant quantity.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct EntityId {
    pub index: u32,
    pub generation: u32,
}

/// The phases of one logic tick, in the order they run.
///
/// Named so that "which phase does this belong to?" is answerable, and so a
/// future measurement of the real order changes a list here rather than the
/// shape of the code -- which is what happened when the order was measured on the
/// 16.402 captures (`TICK_PHASES`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Elixir accrual, card cycle. The deploy countdown lives here only under
    /// match.TICK_ORDER = legacy_move_before_attack; the measured one runs after Move.
    Upkeep,
    /// Status effect timers tick down; stun/freeze/slow/rage expire.
    Status,
    /// Spawners emit, death-spawns from the previous tick materialise.
    Spawn,
    /// Each unit picks or keeps a target. Hysteresis and target-lock apply here.
    Target,
    /// Path is recomputed if due; each unit proposes a movement delta. Under the
    /// 16.402 locomotion (state.rs `phase_path16402`) this is the whole sequential
    /// move pass: every ground troop's move update, one after the other in
    /// CREATION order, each seeing the ones before it already moved.
    Path,
    /// Proposed deltas are applied, then collision separation resolves overlap
    /// (the frame-planned arms only; the 16.402 one separated inside Path).
    /// Then, under match.TICK_ORDER = client16402, the deploy countdown, AFTER the
    /// move pass as measured: a unit whose deploy time ends this tick stands still
    /// this tick and walks the next.
    Move,
    /// Attack windups advance; attacks that complete write into the damage buffer.
    /// BEFORE Path and Move under the shipped order (every attack update runs
    /// before any move update, as the 16.402 captures show): a unit whose target is
    /// in range at the start of the tick winds up and does not
    /// step; a unit whose target is gone or out of range walks the same tick.
    Attack,
    /// Projectiles advance; those that arrive write into the damage buffer. Spells
    /// advance here too (spell.rs): flights land, rolls roll, one-shot area effects
    /// apply, writing damage, knockback and stun buffers and queuing released units.
    Projectile,
    /// The damage buffer is applied in one pass. Deaths are queued, not applied.
    /// Then stun timers tick (status.BUFF_EXPIRY_TICK_ALIGNMENT), the stun buffer
    /// merges by max and the knockback buffer sums and moves the survivors.
    Resolve,
    /// Queued deaths fire their effects; queued spawns are inserted.
    Reap,
    /// Win conditions, crown count, overtime.
    Judge,
}

/// The canonical phase order (calibration match.TICK_ORDER = client16402), as
/// measured on the live 16.402 captures -- the attack updates for every entity,
/// THEN the move updates in creation order, then the per-unit character update
/// (deploy countdown) after the move pass. A test asserts the engine runs exactly
/// this.
///
/// WHY THIS ORDER REPRODUCES THE THREE MEASURED TRANSITION RULES:
///   1->2 walking -> attacking takes effect BEFORE the move (819 transition ticks,
///       none stepped): Target then Attack run before Path, so a unit in range at
///       the start of the tick is in Windup when Path looks at it and is skipped.
///   2->1 attacking -> walking walks the SAME tick (412 / 413): Target drops a
///       dead target and Attack sees an out-of-range one before Path decides, so
///       Path walks it this tick.
///   4->1 deploy finished takes effect AFTER the move (398 / 445 stood still): the
///       countdown runs at the end of Move, so the tick it reaches 0 the unit was
///       still deploying in Path; it walks the next tick, on spawn + DeployTime /
///       TICK_MS exactly as movement.DEPLOY_TIMING measured (the countdown now
///       starts on the spawn tick itself, after the move pass the new unit sat out).
pub const TICK_PHASES: [Phase; 11] = [
    Phase::Upkeep,
    Phase::Status,
    Phase::Spawn,
    Phase::Target,
    Phase::Attack,
    Phase::Path,
    Phase::Move,
    Phase::Projectile,
    Phase::Resolve,
    Phase::Reap,
    Phase::Judge,
];

/// The order this engine ran before the measurement (calibration match.TICK_ORDER =
/// legacy_move_before_attack): Move before Attack -- "a unit that moves into range
/// attacks the same tick", a guess the live captures refuted -- with the deploy
/// countdown in Upkeep, so a unit walked on the tick its deploy time ended. Kept
/// runnable so the refutation stays runnable; the regression plant `phase_order`
/// forces it.
pub const LEGACY_TICK_PHASES: [Phase; 11] = [
    Phase::Upkeep,
    Phase::Status,
    Phase::Spawn,
    Phase::Target,
    Phase::Path,
    Phase::Move,
    Phase::Attack,
    Phase::Projectile,
    Phase::Resolve,
    Phase::Reap,
    Phase::Judge,
];

/// Deterministic RNG owned by battle state.
///
/// PCG32. Supercell's own generator has never been publicly recovered, so this
/// is explicitly NOT an attempt to reproduce real matches -- see
/// calibration.json `rng.GENERATOR`. What it does guarantee is that our own
/// runs are bit-identical given a seed, which is what every test depends on.
///
/// The state is part of the serialized battle state. It is never a module
/// global: a module-global generator cannot be seeded per battle, saved with a
/// snapshot or replayed, so nothing drawn from one is reproducible.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Rng { state: 0, inc: (seed << 1) | 1 };
        r.next_u32();
        r.state = r.state.wrapping_add(seed);
        r.next_u32();
        r
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in [0, n). Rejection-sampled so it is unbiased; a modulo would
    /// skew, and a skew in spawn placement is a skew in every swarm card.
    #[inline]
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let threshold = n.wrapping_neg() % n;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return r % n;
            }
        }
    }

    /// Uniform in [lo, hi], inclusive.
    #[inline]
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + self.below((hi - lo + 1) as u32) as i32
    }
}

/// Which pathfinding model to run. All candidates are implemented because no
/// public source settles which one the real game uses -- and the version every
/// public source describes is the pre-2025 one Supercell replaced.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathModel {
    /// Pre-2025: move horizontally into the unit's own lane, then advance along it.
    LaneSnap,
    /// The community reading of "weighted grid A*": uniform octile 10/14 step
    /// costs, no cost table, mover-radius-inflated blocking, string-pulled
    /// waypoints. REFUTED for the 2026 game (calibration pathfinding.ALGORITHM
    /// candidate_meanings) and kept only so the refutation stays runnable.
    GridAStar,
    /// Post-2025: diagonal movement with look-ahead building avoidance.
    DiagonalLookahead,
    /// THE MEASURED 2026 MODEL (path2026.rs): 8-connected weighted A* over the
    /// 36 x 64 half-tile grid with the calibrated cell costs, the sqrt(2) diagonal,
    /// friendly-only half-open AABB occlusions and the reach goal predicate --
    /// plus its own locomotion law (truncating steps, no carry, stomp schedule).
    /// It does NOT share the per-tick loop of the three above; see
    /// state.rs `phase_path_2026`.
    Oracle2026,
}

/// How two overlapping units push each other apart. Unsettled -- the data ships
/// a Mass column but no public source gives the formula, and the best public
/// reimplementation ignores Mass and weights by speed instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PushModel {
    MassWeighted,
    SpeedWeighted,
    EqualSplit,
}

#[pymodule]
fn royalesim(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__doc__", "Deterministic integer-only Clash Royale battle core.")?;
    m.add("SUBTILE", fixed::SUBTILE)?;
    m.add("SUBTILE_PER_MILLITILE", fixed::SUBTILE_PER_MILLITILE)?;
    py::register(m)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_reproducible_from_a_seed() {
        let a: Vec<u32> = { let mut r = Rng::new(12345); (0..64).map(|_| r.next_u32()).collect() };
        let b: Vec<u32> = { let mut r = Rng::new(12345); (0..64).map(|_| r.next_u32()).collect() };
        assert_eq!(a, b, "same seed must give the same stream");
    }

    #[test]
    fn rng_differs_across_seeds() {
        let a: Vec<u32> = { let mut r = Rng::new(1); (0..16).map(|_| r.next_u32()).collect() };
        let b: Vec<u32> = { let mut r = Rng::new(2); (0..16).map(|_| r.next_u32()).collect() };
        assert_ne!(a, b);
    }

    #[test]
    fn rng_below_is_in_bounds_and_covers_the_range() {
        let mut r = Rng::new(7);
        let mut seen = [0u32; 6];
        for _ in 0..60_000 {
            let v = r.below(6);
            assert!(v < 6);
            seen[v as usize] += 1;
        }
        // Unbiased to well within noise at this sample size; a modulo bias on
        // n=6 over u32 is far too small to catch here, so this checks coverage
        // rather than uniformity. The rejection bound is what makes it unbiased.
        for (i, c) in seen.iter().enumerate() {
            assert!(*c > 9_000, "bucket {i} got {c}, suspiciously few");
        }
    }

    #[test]
    fn phase_order_is_the_documented_one() {
        // A test that exists so the order is a decision with a name, not an
        // accident of what someone typed first.
        assert_eq!(TICK_PHASES.len(), 11);
        assert_eq!(TICK_PHASES[0], Phase::Upkeep);
        assert_eq!(TICK_PHASES[3], Phase::Target);
        let pos = |p: Phase| TICK_PHASES.iter().position(|q| *q == p).unwrap();
        // ~~Move must precede Attack: a unit that moves into range attacks the
        // same tick, which is what the real game visibly does.~~ -- MEASURED on the
        // live 16.402 captures: every attack update runs before any move update, and
        // a 1 -> 2 transition never steps (819 / 819).
        assert!(pos(Phase::Target) < pos(Phase::Attack));
        assert!(pos(Phase::Attack) < pos(Phase::Path));
        assert!(pos(Phase::Path) < pos(Phase::Move));
        // Resolve must follow every writer of the damage buffer.
        assert!(pos(Phase::Attack) < pos(Phase::Resolve));
        assert!(pos(Phase::Projectile) < pos(Phase::Resolve));
        // Reap must follow Resolve, or a death effect fires before the death.
        assert!(pos(Phase::Resolve) < pos(Phase::Reap));
        // The legacy order is the same list with Attack after Move, nothing else.
        let legacy = |p: Phase| LEGACY_TICK_PHASES.iter().position(|q| *q == p).unwrap();
        assert!(legacy(Phase::Move) < legacy(Phase::Attack));
        let shipped_sans_attack: Vec<Phase> = TICK_PHASES.iter().copied().filter(|p| *p != Phase::Attack).collect();
        let legacy_sans_attack: Vec<Phase> = LEGACY_TICK_PHASES.iter().copied().filter(|p| *p != Phase::Attack).collect();
        assert_eq!(shipped_sans_attack, legacy_sans_attack);
    }

    #[test]
    fn team_other_is_an_involution() {
        assert_eq!(Team::Blue.other(), Team::Red);
        assert_eq!(Team::Red.other().other(), Team::Red);
    }
}
