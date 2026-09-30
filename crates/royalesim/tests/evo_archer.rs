//! THE EVO ARCHER (tools/extract_cards.py `far_shot_block`; card.rs `FarShotDef`; state.rs `select_attack`; combat.rs
//! `fire`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-Archer-evo-s0, two evolved Archers, 14 arrows): each arrow took 140 (55
//! on the ladder) or 112 (44) off its target; 140 where the swing's entry was chosen with the target 6051 or more from
//! her centre, 112 where it was chosen at 5498 or less, the entry chosen as the Three Musketeers' is (at the swing's
//! start, or at the hit that ends the swing before; reach 4500 + both radii).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_archer`):
//!   - far_shot_never -> every test red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Archer".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// The form held at (9000, 9000) shooting a red Knight held straight above her, `first` from her centre until her
/// first arrow leaves and `then` after it (out of every crown tower's reach), topped up every tick. What each of her
/// first `k` arrows took off it, in order.
fn arrows(first: i32, then: i32, k: usize) -> Vec<i32> {
    let mut s = battle();
    let at = n(9000, 9000);
    s.spawn_unit(Team::Blue, "Archer_EV1", at, None).expect("the Archer");
    s.tick();
    let archer = find_live(&s, Team::Blue, "Archer_EV1").first().expect("the Archer").id;
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 9000 + first), None).expect("a red Knight");
    let (mut out, mut fired) = (Vec::new(), false);
    for _ in 0..400 {
        let y = if fired { then } else { first };
        assert!(s.debug_set_pos(archer, at));
        assert!(s.debug_set_pos(knight, n(9000, 9000 + y)));
        let top = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_hp(knight, top));
        s.tick();
        fired |= s.projectiles().iter().any(|p| p.firer == Some(archer));
        let lost = top - s.entity(knight).expect("the Knight").hp;
        if lost > 0 {
            out.push(lost);
            if out.len() == k {
                return out;
            }
        }
    }
    panic!("{} arrows landed, {k} wanted: {out:?}", out.len());
}

#[test]
fn a_target_beyond_her_reach_takes_the_power_arrow() {
    // 5700 from her centre: beyond 4500 + her 500 + the Knight's 500.
    assert_eq!(arrows(5700, 5700, 4), vec![140; 4]);
}

#[test]
fn a_target_within_the_reach_and_both_radii_takes_her_plain_arrow() {
    // 5400: beyond 4500 from her centre, within 4500 + both radii (5500), the selector's reach.
    assert_eq!(arrows(5400, 5400, 4), vec![112; 4]);
}

#[test]
fn the_next_swing_s_entry_is_chosen_at_the_hit_that_ends_this_one() {
    // The Knight at 5700 until her first arrow leaves, then at 5400: her second arrow was chosen at her first's release,
    // with the Knight still at 5700, and is the power arrow; the third is the plain one.
    assert_eq!(arrows(5700, 5400, 3), vec![140, 140, 112]);
}
