//! THE EVO GOBLIN GIANT (tools/extract_cards.py `spawn_below_block`; card.rs `SpawnerDef::below_hp_pct`; state.rs
//! `spawner_pass`, EvoBoard `spawn_gates`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; Oracle's sp-f3-gg-s0 and -s1, and sp-form-GoblinGiant-evo-s0): hit below half on
//! C, it makes a Goblin on C + 2, C + 45 and C + 89 (StartCounterAt 0 fired 50 ms over, then Interval 2200 with the
//! leftover carried), each walking from birth, 2500 behind it in its owner's frame (seen 2380 behind after its first
//! step), on either side.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_goblin_giant`):
//!   - spawn_gate_never -> every test red;
//!   - spawn_gate_ignored -> every test red;
//!   - spawn_gate_clock_late -> `below_half_it_makes_a_goblin_behind_it_2_ticks_on_then_43_and_44_ticks_apart` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The form on `team`'s half, held at `at` past its deploy: the battle and the Giant.
fn giant(team: Team, at: (i32, i32)) -> (BattleState, EntityId) {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["GoblinGiant".into(), "Knight".into()], vec!["GoblinGiant".into(), "Knight".into()]];
    cfg.forms = [vec![1, 0], vec![1, 0]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(team, "GoblinGiant_EV1", n(at.0, at.1), None).expect("the Giant");
    s.tick();
    let g = find_live(&s, team, "GoblinGiant_EV1").first().expect("the Giant").id;
    for _ in 0..40 {
        assert!(s.debug_set_pos(g, n(at.0, at.1)));
        s.tick();
    }
    (s, g)
}

/// Down to half less one after tick C: each Goblin's first tick after it (C + k) and its first point, over 95 ticks.
fn goblins(s: &mut BattleState, g: EntityId, team: Team, at: (i32, i32)) -> Vec<(usize, (i32, i32))> {
    assert!(find_live(s, team, "Goblin").is_empty(), "a Goblin above half");
    let max = s.entity(g).expect("the Giant").max_hp;
    assert!(s.debug_set_hp(g, max / 2 - 1));
    let (mut seen, mut born): (Vec<EntityId>, Vec<(usize, (i32, i32))>) = (Vec::new(), Vec::new());
    for k in 1..=95 {
        assert!(s.debug_set_pos(g, n(at.0, at.1)));
        s.tick();
        for gob in find_live(s, team, "Goblin") {
            if !seen.contains(&gob.id) {
                seen.push(gob.id);
                born.push((k, (gob.pos.x / K, gob.pos.y / K)));
            }
        }
    }
    born
}

#[test]
fn below_half_it_makes_a_goblin_behind_it_2_ticks_on_then_43_and_44_ticks_apart() {
    let at = (11279, 9000);
    let (mut s, g) = giant(Team::Blue, at);
    let born = goblins(&mut s, g, Team::Blue, at);
    let ks: Vec<usize> = born.iter().map(|b| b.0).collect();
    assert_eq!(ks, vec![2, 45, 89], "the Goblins' ticks: {born:?}");
    // 2500 behind (its own king's way), less the first step forward.
    assert!(born.iter().all(|(_, p)| (2300..=2500).contains(&(at.1 - p.1)) && (p.0 - at.0).abs() <= 150), "points: {born:?}");
}

#[test]
fn on_the_red_side_behind_is_up() {
    let at = (11279, 23000);
    let (mut s, g) = giant(Team::Red, at);
    let born = goblins(&mut s, g, Team::Red, at);
    assert_eq!(born.first().map(|b| b.0), Some(2), "the first Goblin: {born:?}");
    assert!(born.iter().all(|(_, p)| (2300..=2500).contains(&(p.1 - at.1)) && (p.0 - at.0).abs() <= 150), "points: {born:?}");
}
