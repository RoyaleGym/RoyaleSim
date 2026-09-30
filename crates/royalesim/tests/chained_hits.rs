//! CHAINED HITS (projectiles.csv ChainedHitCount / ChainedHitRadius; card.rs `ChainHitDef`; combat.rs `ChainHop`,
//! `chain_next`, the hop in `step_projectiles`): the Electro Dragon's shot hits 3 targets and the Electro Spirit's 9,
//! each hop going on from the target it landed on to the closest enemy within 4000 that it has not hit.
//!
//! Measured on client 15.535.29 (Oracle's sp-chain-*): the hops go from the last unit hit (an Ice Spirit 3606 from the
//! Knight hit first and 6325 from the Ice Golem hit second was never hit); a hop takes a crown tower; each next hit comes
//! 3 + ceil(hop / speed) ticks after the last (CHAIN_HOP_WAIT_TICKS).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! chained_hits`):
//!   - chain_never -> every test red;
//!   - chain_hop_no_wait -> `each_hop_waits_three_ticks_then_flies_at_the_shot_s_speed` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["ElectroDragon".into(), "ElectroSpirit".into()], vec!["Knight".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// A blue `attacker` held at `at`, red `units` held at their points with their hp topped up every tick; `frames` ticks.
/// Returns, per frame, what each red unit lost on the tick (None once it is gone).
fn scene(attacker: &str, at: (i32, i32), units: &[(&str, (i32, i32))], frames: usize) -> Vec<Vec<Option<i32>>> {
    let mut s = battle();
    let hero = s.scenario_spawn_now(Team::Blue, attacker, n(at.0, at.1), None).expect("the attacker");
    let reds: Vec<(EntityId, (i32, i32), i32)> = units
        .iter()
        .map(|(card, p)| {
            let id = s.scenario_spawn_now(Team::Red, card, n(p.0, p.1), None).expect("a red unit");
            (id, *p, s.entity(id).expect("the red unit").max_hp)
        })
        .collect();
    let mut out = Vec::new();
    for _ in 0..frames {
        if s.entity(hero).is_some() {
            assert!(s.debug_set_pos(hero, n(at.0, at.1)));
        }
        for (id, p, top) in &reds {
            if s.entity(*id).is_some() {
                assert!(s.debug_set_pos(*id, n(p.0, p.1)));
                assert!(s.debug_set_hp(*id, *top));
            }
        }
        s.tick();
        out.push(reds.iter().map(|(id, _, top)| s.entity(*id).map(|e| top - e.hp)).collect());
    }
    out
}

#[test]
fn an_electro_dragon_shot_hits_three_along_a_line_of_knights() {
    // Four red Knights 1500 apart on one row of red's half, the first 3500 ahead of the dragon (in its reach; blue's
    // half), out of every crown tower's reach: its first shot hits the first, then the second (1500 on) and the third,
    // never the fourth. 75 at level 1 is 192 at 11, on each.
    let row = [(6500, 18000), (8000, 18000), (9500, 18000), (11000, 18000)];
    let units: Vec<(&str, (i32, i32))> = row.iter().map(|p| ("Knight", *p)).collect();
    let f = scene("ElectroDragon", (6500, 14500), &units, 120);
    let first = f.iter().position(|x| x[0] == Some(192)).expect("the dragon's first shot on the first Knight");
    // Up to the next shot (HitSpeed 2100: 42 ticks), each of the first three loses 192 once, the fourth nothing.
    let window = &f[first..(first + 40).min(f.len())];
    for (k, name) in ["first", "second", "third"].iter().enumerate() {
        let hits: Vec<i32> = window.iter().filter_map(|x| x[k]).filter(|d| *d > 0).collect();
        assert_eq!(hits, [192], "the {name} Knight hit once for 192: {hits:?}");
    }
    assert!(window.iter().all(|x| x[3] == Some(0)), "the fourth Knight never hit (3 targets)");
}

#[test]
fn each_hop_waits_three_ticks_then_flies_at_the_shot_s_speed() {
    // Three red Knights on the row, 2000 and then 3000 apart (the third 5000 from the first: only a hop from the second
    // reaches it). The dragon's shot (2000 a tick) hits the second 3 + 1 ticks after the first, the third 3 + 2 after.
    let units = [("Knight", (6500, 18000)), ("Knight", (8500, 18000)), ("Knight", (11500, 18000))];
    let f = scene("ElectroDragon", (6500, 14500), &units, 120);
    let first = |u: usize| f.iter().position(|x| x[u] == Some(192)).unwrap_or_else(|| panic!("Knight {u} hit by the chain"));
    let (a, b, c) = (first(0), first(1), first(2));
    assert_eq!((b - a, c - b), (4, 5), "hops of 2000 and 3000: frames {a}, {b}, {c}");
}

#[test]
fn an_electro_spirit_hits_nine_along_a_line_of_skeletons() {
    // Ten red Skeletons 1700 apart on one row of red's half, out of every crown tower's reach (81 hitpoints: each hit,
    // 39 at level 1 or 100 at 11, kills), the spirit on the left bridge 2500 from the second: nine die, one stands.
    let units: Vec<(&str, (i32, i32))> = (0..10).map(|k| ("Skeleton", (1800 + 1700 * k, 18000))).collect();
    let f = scene("ElectroSpirit", (3500, 15500), &units, 120);
    let last = f.last().expect("frames");
    let dead = last.iter().filter(|x| x.is_none()).count();
    assert_eq!(dead, 9, "nine killed: {last:?}");
    assert!(last.iter().any(|x| *x == Some(0)), "one untouched (9 targets): {last:?}");
}
