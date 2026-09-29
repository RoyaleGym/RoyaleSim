//! TRIPLE ELIXIR (state.rs `triple_elixir`, match.MANA_REGEN_MS_OVERTIME, match.MANA_TRIPLE_AFTER_OVERTIME_S).
//!
//! Measured on client 15.535.29 (a Ladder match run to t6092 with both bars below full): the regen gains 178 of the
//! game's 1/10000 elixir a tick from t1, 357 from the step into t2401 and 537 from the step into t4801, 60 s into
//! overtime. The engine's steps are 178.57, 357.14 and 537.5 of that unit, each the game's in its whole part (its own
//! unit keeps the first two exact; the ledger's notes name the remainder).
//!
//! The scene: a battle with no play, which ties 0-0 and runs into overtime; both bars emptied on t4780. Pinned: every
//! step into t4781..t4800 is the double rate, every step from the one into t4801 the triple rate, on both seats.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test triple_elixir`):
//!   triple_elixir_never   overtime runs the double rate to its end: the steps from t4801 go red.
mod common;

use common::*;
use royalesim::state::BattleState;
use royalesim::Team;

/// A team's elixir in the game's 1/10000 unit, times 1000 (the engine's unit is finer).
fn raw_milli(s: &BattleState, t: Team) -> i64 {
    let (m, unit) = s.elixir_raw(t);
    m * 10_000_000 / unit
}

#[test]
fn the_regen_triples_from_the_step_into_tick_4801() {
    let mut s = BattleState::try_new(0, config()).expect("the default battle");
    while s.tick_count() < 4780 {
        s.tick();
    }
    assert!(s.outcome().is_none(), "a battle with no play runs into overtime");
    for t in [Team::Blue, Team::Red] {
        s.scenario_set_elixir_milli(t, 0);
    }
    // (tick after the step, step in 1/1000 of the game's unit) per team.
    let mut steps: Vec<(u32, i64, i64)> = Vec::new();
    let mut prev = (raw_milli(&s, Team::Blue), raw_milli(&s, Team::Red));
    for _ in 0..40 {
        s.tick();
        let now = (raw_milli(&s, Team::Blue), raw_milli(&s, Team::Red));
        steps.push((s.tick_count(), now.0 - prev.0, now.1 - prev.1));
        prev = now;
    }
    for &(tick, blue, red) in &steps {
        let want = if tick <= 4800 { 357 } else { 537 };
        assert_eq!((blue / 1000, red / 1000), (want, want), "the step into t{tick}, in the game's whole unit: {steps:?}");
    }
}
