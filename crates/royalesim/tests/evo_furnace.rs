//! THE EVO FURNACE (tools/extract_cards.py `furnace_block`; card.rs `FurnaceDef`, FURNACE_SPIRIT_FLIGHT_TICKS,
//! FURNACE_DELAY_EXTRA_TICKS; state.rs EvoBoard `furnaces`, `furnace_pass`, the interval's hold in `spawner_pass`), at
//! level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; Oracle's sp-f4-furnace-s0 and sp-f4-furnwalk-s0): the quick spirits on the
//! attack's start + 22, then + 47, + 48, 1500 to either side of the Furnace and 1000 behind, the first to its owner's
//! right; the walking spawns 110 then 100 ticks apart; a walk of 20 ticks ended the quick spawn.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_furnace`):
//!   - furnace_quick_never -> `an_attack_starts_its_quick_spawn_22_ticks_on_then_every_47_and_48` red;
//!   - furnace_normal_spawn_unpaused -> `walking_its_spawns_come_110_then_100_ticks_apart` red;
//!   - furnace_quick_never_stops -> `a_walk_of_20_ticks_ends_its_quick_spawn` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["FirespiritHut".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// The Furnace's Fire Spirits not yet in `seen`, with the tick and their points (millitiles).
fn fresh(s: &BattleState, f: EntityId, seen: &mut Vec<EntityId>) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for e in s.entities().filter(|e| e.spawned_by == Some(f)) {
        if !seen.contains(&e.id) {
            seen.push(e.id);
            out.push((e.pos.x / K, e.pos.y / K));
        }
    }
    out
}

#[test]
fn an_attack_starts_its_quick_spawn_22_ticks_on_then_every_47_and_48() {
    // The form held at (9000, 10000), a red Golem held 4000 ahead (in its reach) and topped up: the attack's start S,
    // then the spirits: S + 22 at (x + 1500, y - 1000), S + 69 at (x - 1500, y - 1000), S + 117 on the first side again,
    // and nothing else (the interval held).
    let mut s = battle();
    let at = n(9000, 10000);
    s.spawn_unit(Team::Blue, "FirespiritHut_EV1", at, None).expect("the Furnace");
    s.tick();
    let f = find_live(&s, Team::Blue, "FirespiritHut_EV1").first().expect("the Furnace").id;
    let gat = n(9000, 14000);
    let (mut start, mut seen, mut got) = (None, Vec::new(), Vec::new());
    let mut golem = None;
    for k in 0..400usize {
        assert!(s.debug_set_pos(f, at));
        // The Golem comes once the Furnace has had its first interval spawn (from then on, the interval runs 100 ticks).
        if golem.is_none() && !seen.is_empty() {
            golem = Some(s.scenario_spawn_now(Team::Red, "Golem", gat, None).expect("a red Golem"));
        }
        if let Some(g) = golem {
            assert!(s.debug_set_pos(g, gat));
            let top = s.entity(g).expect("the Golem").max_hp;
            assert!(s.debug_set_hp(g, top));
        }
        s.tick();
        if start.is_none() && s.entity(f).expect("the Furnace").attack_phase != AttackPhase::Idle {
            start = Some(k);
        }
        for p in fresh(&s, f, &mut seen) {
            if start.is_some() {
                got.push((k, p));
            }
        }
        if start.is_some_and(|st| k >= st + 130) {
            break;
        }
    }
    let st = start.expect("the Furnace attacked the Golem");
    let want = [(st + 22, (10500, 9000)), (st + 69, (7500, 9000)), (st + 117, (10500, 9000))];
    let ticks: Vec<usize> = got.iter().map(|g| g.0).collect();
    assert_eq!(ticks, want.map(|w| w.0), "its quick spirits' ticks from its attack's start on {st}: {got:?}");
    // Each stands on its landing point and takes its first step there on that tick (spawner.SPAWNED_FIRST_STEP), as the
    // client's did: within a step (120) of the point.
    for (g, w) in got.iter().zip(want) {
        let (dx, dy) = (f64::from(g.1 .0 - w.1 .0), f64::from(g.1 .1 - w.1 .1));
        assert!((dx * dx + dy * dy).sqrt() <= 150.0, "a spirit off its landing point {:?}: {got:?}", w.1);
    }
}

#[test]
fn walking_its_spawns_come_110_then_100_ticks_apart() {
    // The form put down in blue's left lane with nothing red on the field: it walks; its interval's spawns, the second
    // 100 ticks on plus the moving check's hold of 10, the third 100 on.
    let mut s = battle();
    s.spawn_unit(Team::Blue, "FirespiritHut_EV1", n(3500, 3000), None).expect("the Furnace");
    s.tick();
    let f = find_live(&s, Team::Blue, "FirespiritHut_EV1").first().expect("the Furnace").id;
    let (mut seen, mut ticks) = (Vec::new(), Vec::new());
    for k in 0..300usize {
        s.tick();
        // Walking only: its spawns up to its first attack (a red crown tower in its reach).
        if s.entity(f).expect("the Furnace").attack_phase != AttackPhase::Idle {
            break;
        }
        if !fresh(&s, f, &mut seen).is_empty() {
            ticks.push(k);
        }
    }
    assert!(ticks.len() >= 3, "its spawns: {ticks:?}");
    assert_eq!((ticks[1] - ticks[0], ticks[2] - ticks[1]), (110, 100), "its spawns: {ticks:?}");
}

#[test]
fn a_walk_of_20_ticks_ends_its_quick_spawn() {
    // As the first scene, the Golem killed after the second quick spirit: the Furnace walks, and after 20 ticks of it
    // no quick spirit comes (the next would stand on the second + 48).
    let mut s = battle();
    let at = n(9000, 10000);
    s.spawn_unit(Team::Blue, "FirespiritHut_EV1", at, None).expect("the Furnace");
    s.tick();
    let f = find_live(&s, Team::Blue, "FirespiritHut_EV1").first().expect("the Furnace").id;
    let gat = n(9000, 14000);
    let (mut seen, mut quick, mut golem) = (Vec::new(), Vec::new(), None);
    let mut attacking = false;
    for _ in 0..400usize {
        if golem.is_none() && !seen.is_empty() {
            golem = Some(s.scenario_spawn_now(Team::Red, "Golem", gat, None).expect("a red Golem"));
        }
        if let Some(g) = golem.filter(|_| quick.len() < 2) {
            assert!(s.debug_set_pos(f, at));
            assert!(s.debug_set_pos(g, gat));
            let top = s.entity(g).expect("the Golem").max_hp;
            assert!(s.debug_set_hp(g, top));
        }
        s.tick();
        attacking |= s.entity(f).expect("the Furnace").attack_phase != AttackPhase::Idle;
        if attacking && !fresh(&s, f, &mut seen).is_empty() {
            quick.push(s.tick_count());
            if quick.len() == 2 {
                break;
            }
        }
    }
    assert_eq!(quick.len(), 2, "two quick spirits");
    // The Golem gone: the Furnace walks on from here.
    let g = golem.expect("the Golem");
    assert!(s.debug_set_hp(g, 0));
    let mut after = Vec::new();
    for _ in 0..60 {
        s.tick();
        if !fresh(&s, f, &mut seen).is_empty() {
            after.push(s.tick_count());
        }
    }
    assert!(!after.contains(&(quick[1] + 48)), "a quick spirit after the walk: {quick:?} then {after:?}");
}
