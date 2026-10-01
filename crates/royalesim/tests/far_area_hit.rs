//! A FAR AREA HIT IS VOID (combat.rs, the splash branch; combat.HIT_BEYOND_CANCEL_RANGE = no_damage): a direct area
//! hit centred on its target deals nothing when the target stands more than LOGIC_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE
//! (1500) past the attacker's reach at the hit, as a single-target hit does. Measured on client 15.535.29
//! (sp-sk-souls-ignore-s0 t405: a Skeleton King's swing completes with a Battle Ram 2,391 past reach, and the Ram
//! keeps its hp).
//!
//! The scene: a blue Skeleton King held at (9000, 14800) and a red Knight held at (9000, 17200), 2400 apart, inside
//! reach (Range 1200 + radii 1000 and 500 = 2700). Both are out of every crown tower's range. After the King's first
//! hit and two more ticks (the swing under way), the Knight is held farther off until the next hit: 1000 past reach
//! (the hit lands) or 2000 past (it is void).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! far_area_hit`): area_hit_beyond_cancel_deals_damage -> `a_skeleton_kings_hit_2000_past_reach_is_void` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::Team;

const BLUE: [&str; 8] = ["SkeletonKing", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const RED: [&str; 8] = ["Knight", "Giant", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const KING: (i32, i32) = (9000, 14800);
const NEAR: (i32, i32) = (9000, 17200);
/// Range 1200 + the King's radius 1000 + the Knight's 500.
const REACH: i32 = 2700;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Knight's hp lost on the King's first hit (in reach) and on its second, with the Knight held `past` beyond reach
/// from two ticks after the first hit.
fn hits(past: i32) -> (i32, i32) {
    let mut cfg = config();
    cfg.decks = [BLUE.iter().map(|c| c.to_string()).collect(), RED.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let king = s.scenario_spawn_now(Team::Blue, "SkeletonKing", n(KING), None).expect("the King");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(NEAR), None).expect("the Knight");
    let far = (KING.0, KING.1 + REACH + past);
    let mut at = NEAR;
    let mut first: Option<(u32, i32)> = None;
    for t in 0..200 {
        assert!(s.debug_set_pos(king, n(KING)));
        assert!(s.debug_set_pos(knight, n(at)));
        let hp = s.entity(knight).expect("the Knight").hp;
        s.tick();
        let k = s.entity(knight).expect("the Knight lives");
        let fired = s.entity(king).expect("the King").attack_phase == AttackPhase::Cooldown;
        if fired {
            match first {
                None => first = Some((t, hp - k.hp)),
                Some((_, lost)) => return (lost, hp - k.hp),
            }
        }
        if let Some((f, _)) = first {
            if t >= f + 2 {
                at = far;
            }
        }
    }
    panic!("the scene drifted: the King did not hit twice in 200 ticks (first {first:?})");
}

#[test]
fn a_skeleton_kings_hit_1000_past_reach_lands() {
    let (first, second) = hits(1000);
    assert!(first > 0, "the scene drifted: the King's first hit, in reach, dealt nothing");
    assert_eq!(second, first, "the hit 1000 past reach (inside the cancel range) did not deal the King's damage");
}

#[test]
fn a_skeleton_kings_hit_2000_past_reach_is_void() {
    let (first, second) = hits(2000);
    assert!(first > 0, "the scene drifted: the King's first hit, in reach, dealt nothing");
    assert_eq!(second, 0, "the hit 2000 past reach (beyond the cancel range) dealt {second}");
}
