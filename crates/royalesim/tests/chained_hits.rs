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
//!   - chain_hop_no_wait -> `each_hop_waits_three_ticks_then_flies_at_the_shot_s_speed` red;
//!   - dead_hop_ends -> `under_client16402_hops_on_a_hop_whose_target_died_goes_on_from_its_point` red;
//!   - dead_hop_uncounted -> `under_client16402_hops_on_a_hop_whose_target_died_goes_on_from_its_point` red.
//!
//! combat.CHAIN_HOP_DEAD_TARGET (client 16.402, parity r68: 5 of 5 Electro Spirit hops whose target died before they
//! landed went on): such a hop lands on the dead target's last point and goes on from there; the dead target counts.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, ChainHopDeadTarget};
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
    assert!(last.contains(&Some(0)), "one untouched (9 targets): {last:?}");
}

/// A blue Electro Dragon (3 targets) at (6500, 14500) and four red Knights on a row from (6500, 18000), the second 2000 on,
/// the third 1800 past it and the fourth 2000 past that (a hop from the dead Knight's frozen point to the walking third is
/// one 2000 step), held and topped up, but the second is killed on the tick after the first is hit, while the shot's hop
/// to it waits on the first (CHAIN_HOP_WAIT_TICKS). Under combat.CHAIN_HOP_DEAD_TARGET = `arm`: the frames (counted from the first hit) on
/// which the first, third and fourth Knight lost hp, in the 40 ticks after the first hit.
fn dead_hop_scene(arm: ChainHopDeadTarget) -> [Vec<usize>; 3] {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["ElectroDragon".into(), "ElectroSpirit".into()], vec!["Knight".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.chain_hop_dead_target = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at = n(6500, 14500);
    let dragon = s.scenario_spawn_now(Team::Blue, "ElectroDragon", at, None).expect("the dragon");
    let knights: Vec<(EntityId, Vec2, i32)> = [(6500, 18000), (8500, 18000), (10300, 18000), (12300, 18000)]
        .iter()
        .map(|p| {
            let id = s.scenario_spawn_now(Team::Red, "Knight", n(p.0, p.1), None).expect("a Knight");
            (id, n(p.0, p.1), s.entity(id).expect("the Knight").max_hp)
        })
        .collect();
    let mut hits: [Vec<usize>; 3] = Default::default();
    let mut first: Option<usize> = None;
    for f in 0..200 {
        if s.entity(dragon).is_some() {
            assert!(s.debug_set_pos(dragon, at));
        }
        for (k, (id, p, top)) in knights.iter().enumerate() {
            if s.entity(*id).is_some() {
                assert!(s.debug_set_pos(*id, *p));
                assert!(s.debug_set_hp(*id, if k == 1 && first.is_some_and(|a| f == a + 1) { 0 } else { *top }));
            }
        }
        s.tick();
        for (slot, k) in [0usize, 2, 3].into_iter().enumerate() {
            let (id, _, top) = knights[k];
            if s.entity(id).is_some_and(|e| e.hp < top) {
                hits[slot].push(f - first.unwrap_or(f));
            }
        }
        if first.is_none() && !hits[0].is_empty() {
            first = Some(f);
        }
        if first.is_some_and(|a| f >= a + 40) {
            break;
        }
    }
    assert!(first.is_some(), "precondition: the dragon's shot hit the first Knight");
    assert!(s.entity(knights[1].0).is_none(), "precondition: the second Knight was killed");
    hits
}

#[test]
fn under_client16402_hops_on_a_hop_whose_target_died_goes_on_from_its_point() {
    // The hop to the second Knight lands on its point 3 + 1 ticks after the first hit; from there it goes on to the third
    // (1800 on) 3 + 1 ticks later, frame 8. The dead Knight counts as the chain's second hit, so the third is its last:
    // the fourth is never hit.
    let [_, third, fourth] = dead_hop_scene(ChainHopDeadTarget::Client16402HopsOnFromLastPoint);
    assert_eq!(third, [8], "the third Knight hit once, 8 ticks after the first: {third:?}");
    assert!(fourth.is_empty(), "the fourth Knight never hit (the dead hop counted): {fourth:?}");
    // ends (the vacuity check): the chain ends on the dead Knight.
    let [_, third, fourth] = dead_hop_scene(ChainHopDeadTarget::Ends);
    assert!(third.is_empty() && fourth.is_empty(), "ends: the chain went past the dead Knight: {third:?} {fourth:?}");
}
