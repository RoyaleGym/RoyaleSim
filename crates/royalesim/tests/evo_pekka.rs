//! THE EVO P.E.K.K.A. (tools/extract_cards.py `kill_heal_block`; card.rs `KillHealDef`, `BuffDef::over_heal_pct`;
//! state.rs `Scratch::kills`, `kill_heals`, `land_buff_heals`), at level 11 (3760 max hitpoints).
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-Pekka-evo-s0): a Skeleton she killed on t1305 healed her 168 on t1315, a
//! Knight she killed on t1376 healed her 320 on t1386, and nothing else moved her hitpoints up.
//! Read off the table, not measured: the third heal (a victim of 1990 or more at level 10), the overheal (to 150 % of
//! her maximum), and the victim's size read at level 10.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_pekka`):
//!   - kill_heal_never -> every test red;
//!   - overheal_capped_at_max -> `a_kill_heals_her_past_her_maximum_up_to_half_again` red;
//!   - kill_size_at_own_level -> `the_victims_size_is_its_hitpoints_at_level_10` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle(red_level: u8) -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Pekka".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, red_level.into()];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    // Blue's princess towers down (its king wakes; its reach stops short of y 12000): no crown tower reaches the scene.
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    s
}

/// The form held at (9000, 12500) on `hp` hitpoints, a red `victim` held 1200 ahead on 1 hitpoint until her first blow
/// kills it: her hitpoints on the kill's tick and on each of the 30 after it.
fn after_a_kill(red_level: u8, victim: &str, hp: i32) -> Vec<i32> {
    let mut s = battle(red_level);
    let (at, v_at) = (n(9000, 12500), n(9000, 13700));
    s.spawn_unit(Team::Blue, "Pekka_EV1", at, None).expect("the P.E.K.K.A.");
    let v = s.scenario_spawn_now(Team::Red, victim, v_at, None).expect("the victim");
    s.tick();
    let pk = find_live(&s, Team::Blue, "Pekka_EV1").first().expect("the P.E.K.K.A.").id;
    let mut killed = false;
    for _ in 0..200 {
        assert!(s.debug_set_pos(pk, at));
        assert!(s.debug_set_pos(v, v_at));
        assert!(s.debug_set_hp(v, 1));
        assert!(s.debug_set_hp(pk, hp));
        s.tick();
        if s.entity(v).is_none() {
            killed = true;
            break;
        }
    }
    assert!(killed, "{victim}: no kill in 200 ticks");
    let mut hps = vec![s.entity(pk).expect("the P.E.K.K.A.").hp];
    for _ in 0..30 {
        assert!(s.debug_set_pos(pk, at));
        s.tick();
        hps.push(s.entity(pk).expect("the P.E.K.K.A.").hp);
    }
    hps
}

/// Her hitpoints after a kill: unchanged for 9 ticks, `heal` more on the 10th, unchanged again after it.
fn assert_one_heal(victim: &str, hps: &[i32], heal: i32) {
    let k = hps[0];
    assert!(hps[1..10].iter().all(|h| *h == k), "{victim}: moved before the kill + 10: {hps:?}");
    assert_eq!(hps[10], k + heal, "{victim}: the heal on the kill + 10: {hps:?}");
    assert!(hps[11..].iter().all(|h| *h == k + heal), "{victim}: moved after the heal: {hps:?}");
}

#[test]
fn a_kill_heals_her_by_the_victims_size_one_heal_10_ticks_on() {
    // A Skeleton (under 990 at level 10): the first heal, 132 a second, 337 at level 11, halved every 500 ms: 168. A
    // Knight (under 1990): 250, 640, 320. A Giant: 474, 1213, 606.
    for (victim, heal) in [("Skeleton", 168), ("Knight", 320), ("Giant", 606)] {
        assert_one_heal(victim, &after_a_kill(11, victim, 2000), heal);
    }
}

#[test]
fn a_kill_heals_her_past_her_maximum_up_to_half_again() {
    // From her maximum, 3760: a Giant's 606 lands whole (4366); from 5400 it stops at 150 % of 3760, 5640.
    assert_one_heal("Giant", &after_a_kill(11, "Giant", 3760), 606);
    assert_one_heal("Giant", &after_a_kill(11, "Giant", 5400), 240);
}

#[test]
fn the_victims_size_is_its_hitpoints_at_level_10() {
    // A level-14 Knight is over 1990 at its own level and under it at level 10: the second heal, 320.
    let mut s = battle(14);
    let kn = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 20000), None).expect("a red Knight");
    assert!(s.entity(kn).expect("the Knight").max_hp >= 1990, "the level-14 Knight is not over 1990");
    assert_one_heal("Knight", &after_a_kill(14, "Knight", 2000), 320);
}
