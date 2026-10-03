//! THE EVO GOBLIN DRILL (tools/extract_cards.py `drill_block`; card.rs `DrillDef`, DRILL_RISE_TICKS,
//! DRILL_SPAWNER_HOLD_TICKS; state.rs EvoBoard `drills`, `drill_pass`, the holds in `phase_status` and `spawner_pass`),
//! at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; Oracle's sp-f4-drill-s0): at 66 % its building went under for 39 ticks where it
//! stood, its hitpoints frozen through the tick it came up and draining the tick after; two Goblins a tick after it went
//! under, at x - 500 and x + 500; its regular Goblins 97 ticks apart across the hide.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_goblin_drill`):
//!   - drill_hide_never -> `at_two_thirds_its_building_goes_under_39_ticks_unhurt_and_puts_down_two_goblins` red;
//!   - drill_hide_drains -> the same red;
//!   - drill_spawner_unheld -> the same red;
//!   - drill_hide_collides -> the same red (its Goblins pushed off its footprint);
//!   - drill_hide_acquired_at_once -> `a_hides_goblins_are_targets_from_their_8th_frame` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::HideState;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The building's Goblins not yet in `seen`: (their points, millitiles, whether they deploy).
fn fresh(s: &BattleState, d: EntityId, seen: &mut Vec<EntityId>) -> Vec<((i32, i32), bool)> {
    let mut out = Vec::new();
    for e in s.entities().filter(|e| e.spawned_by == Some(d)) {
        if !seen.contains(&e.id) {
            seen.push(e.id);
            out.push(((e.pos.x / K, e.pos.y / K), e.deploy_ms > 0));
        }
    }
    out
}

#[test]
fn at_two_thirds_its_building_goes_under_39_ticks_unhurt_and_puts_down_two_goblins() {
    // The form's building put down on blue's side at (9000, 10000), nothing red near. After its first regular Goblin its
    // hitpoints are set to 67 % (880 of 1313): the drain takes it to 66 % on T.
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["GoblinDrill".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at = n(9000, 10000);
    let d = s.scenario_spawn_now(Team::Blue, "units.GoblinDrill_EV1", at, None).expect("the drill's building");
    let top = s.entity(d).expect("the building").max_hp;
    assert_eq!(top, 1313, "513 at level 1");
    let mut seen = Vec::new();
    let mut regular: Vec<usize> = Vec::new();
    let mut set = false;
    let (mut under, mut hp, mut goblins) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..200usize {
        s.tick();
        let e = s.entity(d).expect("the building");
        under.push(e.hide_state == HideState::Hidden);
        hp.push(e.hp);
        for (p, deploying) in fresh(&s, d, &mut seen) {
            if p.1 == 10000 {
                goblins.push((k, p, deploying));
            } else {
                regular.push(k);
            }
        }
        if !set && !regular.is_empty() {
            assert!(s.debug_set_hp(d, 880));
            set = true;
        }
    }
    let t = under.iter().position(|u| *u).expect("it went under");
    assert!(hp[t] * 100 / top <= 66 && hp[t - 1] * 100 / top > 66, "under on the tick it reached 66 %: {:?}", &hp[t - 2..=t]);
    assert!(under[t..t + 39].iter().all(|u| *u) && !under[t + 39], "under T .. T + 38: {:?}", &under[t..t + 41]);
    assert!(hp[t..=t + 39].iter().all(|h| *h == hp[t]) && hp[t + 40] < hp[t], "frozen T .. T + 39: {:?}", &hp[t..t + 42]);
    assert_eq!(goblins[..2], [(t + 1, (9500, 10000), true), (t + 1, (8500, 10000), true)], "its first hide's Goblins: {goblins:?}");
    // Its second line (33 %) went under later in the run: one Goblin, at x + 500.
    assert!(goblins.len() == 3 && goblins[2].1 == (9500, 10000) && goblins[2].2, "its second hide's Goblin: {goblins:?}");
    let across: Vec<usize> = regular.windows(2).filter(|w| w[0] < t && w[1] > t).map(|w| w[1] - w[0]).collect();
    assert_eq!(across, [97], "its regular Goblins across the hide: {regular:?}");
}


/// targeting.SPAWNED_UNIT_ACQUIRE_DELAY: a hide's Goblins are an action's spawn, targets for enemies from their 8th frame
/// (client 15.535.29: none first targeted before F + 7; sp-form-GoblinDrill-evo-s0 t1146, three enemies passed over one on
/// its 2nd frame); the regular Goblins, a Spawn* spawner's, from their first. Plant: drill_hide_acquired_at_once.
#[test]
fn a_hides_goblins_are_targets_from_their_8th_frame() {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["GoblinDrill".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let d = s.scenario_spawn_now(Team::Blue, "units.GoblinDrill_EV1", n(9000, 10000), None).expect("the drill's building");
    let mut seen: Vec<EntityId> = Vec::new();
    let (mut hide, mut regular) = (Vec::new(), Vec::new());
    let mut set = false;
    for _ in 0..200 {
        s.tick();
        let born = s.tick_count() - 1;
        let fresh: Vec<(EntityId, i32, u32)> =
            s.entities().filter(|e| e.spawned_by == Some(d) && !seen.contains(&e.id)).map(|e| (e.id, e.pos.y / K, e.acquirable_from)).collect();
        for (id, y, from) in fresh {
            seen.push(id);
            if y == 10000 {
                hide.push((born, from));
            } else {
                regular.push((born, from));
            }
        }
        if !set && !regular.is_empty() {
            assert!(s.debug_set_hp(d, 880));
            set = true;
        }
    }
    assert!(hide.len() >= 2, "the scene drifted: no hide put its Goblins down ({hide:?})");
    assert!(hide.iter().all(|(born, from)| *from == born + 7), "a hide's Goblins: an enemy may target them from their 8th frame: {hide:?}");
    // NOT VACUOUS: the regular Goblins are targets at once.
    assert!(!regular.is_empty() && regular.iter().all(|(_, from)| *from == 0), "the regular Goblins carry a delay: {regular:?}");
}
