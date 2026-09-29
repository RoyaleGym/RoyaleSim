//! THE EVO ZAP (card.rs `SpellShape::Echo`, `echo_area`; spell.rs `objects_for`), against client 15.535.29 at level
//! 11.
//!
//! THE MEASUREMENTS:
//!   - sp-ec-Zap: the third play of an evolved Zap entry is the form (its spells_evolved DarkElixirCost, 2);
//!   - sp-form-Zap-evo-s0: the evolved Zap (Zap_EV1: the Zap's row with LifeDuration 5000, whose OnStartingAction
//!     spawns a visual at 50 ms and Zap_EV1_SpawnAOE_medium, the Zap with Radius + 500, at 1450 ms) took 192 off a
//!     Knight and a Musketeer on its cast frame and 192 again 30 frames later, at the same point.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_zap`):
//!   - echo_dropped -> `the_evolved_zap_strikes_again_30_frames_later_and_wider` red (one strike);
//!   - evo_cycles_constant -> nothing here (the Zap's column is the old constant, 2): tests/evo_elite_barbarians.rs.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

/// A battle whose Blue deck is the Zap, evolved, and a Knight; both sides at level 11, past the opening lockout.
fn battle() -> BattleState {
    let mut cfg = config();
    cfg.decks = [vec!["Zap".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    let mut s = BattleState::new(7, level11(cfg));
    past_deploy_lockout(&mut s);
    s
}

/// Play the Zap at `at`, then one tick: the names of the spell objects its play made.
fn play_zap(s: &mut BattleState, at: Vec2) -> Vec<String> {
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let before = s.spells().len();
    s.deploy(Team::Blue, "Zap", at).expect("the play");
    s.tick();
    let cards = s.cards().clone();
    let mut names: Vec<String> = s.spells().iter().skip(before).map(|sp| cards.get(sp.card).name.clone()).collect();
    names.dedup();
    names
}

#[test]
fn the_third_play_is_the_evolved_zap() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's DarkElixirCost");
    // The card each Zap play resolves to (state.rs `resolve_play`), read before the play.
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "Zap").expect("the Zap in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    let at = n(9000, 20000);
    for _ in 0..2 {
        assert_eq!(next(&s), "Zap", "a basic play");
        play_zap(&mut s, at);
        for _ in 0..20 {
            s.tick();
        }
    }
    assert_eq!(next(&s), "Zap_EV1", "the third play is the form");
    play_zap(&mut s, at);
    assert_eq!(next(&s), "Zap", "the count starts again");
}

/// The hp of a red Elixir Collector at `near` and one at `far`, frame by frame for `frames` frames after the evolved
/// Zap's play at (9000, 20000).
fn zap_scene(near: (i32, i32), far: (i32, i32), frames: u32) -> Vec<[Option<i32>; 2]> {
    let mut s = battle();
    let at = n(9000, 20000);
    // Two basic plays first, so the third is the form.
    for _ in 0..2 {
        play_zap(&mut s, n(3000, 28000));
        for _ in 0..20 {
            s.tick();
        }
    }
    let ids: Vec<EntityId> = [near, far]
        .iter()
        .map(|p| s.scenario_spawn_now(Team::Red, "Elixir Collector", n(p.0, p.1), None).expect("a Collector"))
        .collect();
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    // Frame 0 is the state before the play's tick; frame k the state after its k-th tick.
    let mut out = vec![[s.entity(ids[0]).map(|e| e.hp), s.entity(ids[1]).map(|e| e.hp)]];
    s.deploy(Team::Blue, "Zap", at).expect("the evolved play");
    for _ in 0..frames {
        s.tick();
        out.push([s.entity(ids[0]).map(|e| e.hp), s.entity(ids[1]).map(|e| e.hp)]);
    }
    out
}

/// The frames (1 the first after the play) on which unit `u` lost more than its lifetime drain (a Collector loses 1 or
/// 2 a tick to its LifeTime), and how much beyond one drain step.
fn losses(fr: &[[Option<i32>; 2]], u: usize) -> Vec<(u32, i32)> {
    let mut out = Vec::new();
    for k in 1..fr.len() {
        if let (Some(a), Some(b)) = (fr[k - 1][u], fr[k][u]) {
            if a - b > 10 {
                out.push((k as u32, a - b));
            }
        }
    }
    out
}

#[test]
fn the_evolved_zap_strikes_again_30_frames_later_and_wider() {
    // An Elixir Collector (it never moves or strikes back) on the tap: struck twice, 30 frames apart.
    let fr = zap_scene((9000, 20000), (3000, 20000), 40);
    let near = losses(&fr, 0);
    // 75 on the ladder, 192 at level 11, plus the drain step of the same tick (0 to 2).
    let zap = |l: &(u32, i32)| (192..=194).contains(&l.1);
    assert_eq!(near.len(), 2, "two strikes on the Collector at the tap: {near:?}");
    assert!(near.iter().all(zap), "each strike takes 192: {near:?}");
    assert_eq!(near[1].0 - near[0].0, 30, "the second strike 30 frames after the first: {near:?}");
    // The second area is the Zap's Radius + 500: a Collector moved out along x, 100 at a time, leaves the first
    // strike's reach 500 before it leaves the second's, whatever the reach rule for a building is.
    let mut last = [0i32; 2];
    for d in (2500..=5000).step_by(100) {
        let fr = zap_scene((3000, 20000), (9000 + d, 20000), 40);
        let far = losses(&fr, 1);
        for (w, hit) in [near[0].0, near[1].0].iter().map(|k| far.iter().any(|l| l.0 == *k)).enumerate() {
            if hit {
                last[w] = d;
            }
        }
    }
    assert!(last[0] > 0, "the first strike reaches a Collector 2500 out");
    assert_eq!(last[1] - last[0], 500, "the second strike reaches 500 further: {last:?}");
}
