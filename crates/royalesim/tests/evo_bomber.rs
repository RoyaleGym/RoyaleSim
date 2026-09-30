//! THE EVO BOMBER (tools/extract_cards.py `bounce_block`; card.rs `BounceDef`; combat.rs `BounceHop`, the bounce in
//! `step_projectiles`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-Bomber-evo-s0, two throws):
//!   - its bomb (400 a tick) landed, and on that frame a bomb stood on the landing point;
//!   - that bomb moved from the next frame, 400 a tick along the throw's line (within 0.01 of its bearing), and landed
//!     2500 on (2499 read) 7 frames after the first landing; a second did the same 7 frames later (SpawnChain 2).
//!
//! Read off the table, not measured: each landing's splash (the bounce's row extends the bomb's).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_bomber`):
//!   - bounce_never -> `the_bomb_bounces_twice_along_its_line_2500_each` red.
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
    cfg.decks = [vec!["Bomber".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// The sine of the angle between `a` and `b`, in thousandths.
fn sin_milli(a: Vec2, b: Vec2) -> i64 {
    let cross = (i64::from(a.x) * i64::from(b.y) - i64::from(a.y) * i64::from(b.x)).abs();
    cross * 1000 / (i64::from(a.len()) * i64::from(b.len())).max(1)
}

#[test]
fn the_bomb_bounces_twice_along_its_line_2500_each() {
    // The form held at (8000, 9800); a red Giant held 3992 away at (9500, 13500) (it walks at buildings), out of every
    // crown tower's reach; a red Knight held on the red side where the second bounce should land. Both topped up.
    let mut s = battle();
    let at = n(8000, 9800);
    let (giant_at, far) = (n(9500, 13500), n(11378, 18134));
    s.spawn_unit(Team::Blue, "Bomber_EV1", at, None).expect("the Bomber");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", giant_at, None).expect("a red Giant");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", far, None).expect("a red Knight");
    s.tick();
    let bomber = find_live(&s, Team::Blue, "Bomber_EV1").first().expect("the Bomber").id;
    // Per frame: the bouncing bombs (point, aim, bounces left) and what the Giant and the Knight lost.
    let mut bombs: Vec<Vec<(Vec2, Vec2, u8)>> = Vec::new();
    let mut lost: Vec<(i32, i32)> = Vec::new();
    for _ in 0..200 {
        assert!(s.debug_set_pos(bomber, at));
        assert!(s.debug_set_pos(giant, giant_at));
        assert!(s.debug_set_pos(knight, far));
        let (gt, kt) = (s.entity(giant).expect("the Giant").max_hp, s.entity(knight).expect("the Knight").max_hp);
        assert!(s.debug_set_hp(giant, gt));
        assert!(s.debug_set_hp(knight, kt));
        s.tick();
        bombs.push(s.projectiles().iter().filter_map(|p| p.bounce.map(|b| (p.pos, p.aim, b.left))).collect());
        lost.push((gt - s.entity(giant).expect("the Giant").hp, kt - s.entity(knight).expect("the Knight").hp));
    }
    let with = |k: usize, left: u8| bombs[k].iter().find(|b| b.2 == left).copied();
    // The throw: a bomb with both bounces left.
    let thrown = (0..bombs.len()).find(|k| with(*k, 2).is_some()).expect("a thrown bomb");
    let b1 = (thrown..bombs.len()).find(|k| with(*k, 1).is_some()).expect("a first bounce");
    let landing = with(b1 - 1, 2).expect("the bomb in flight the frame before").1;
    let first = with(b1, 1).unwrap();
    assert!(with(b1, 2).is_none(), "the thrown bomb is gone on its landing frame");
    assert_eq!(first.0, landing, "the first bounce stands on the landing point on the landing frame");
    assert!(lost[b1].0 > 0, "the landing splashes the Giant: {:?}", &lost[b1 - 1..=b1]);
    let step = with(b1 + 1, 1).expect("the first bounce on the next frame").0;
    assert!((step.dist(landing) - 400 * K).abs() <= K, "400 a tick from the next frame: {}", step.dist(landing));
    let hop = first.1.dist(landing);
    assert!((2499 * K..=2500 * K).contains(&hop), "2500 on: {hop}");
    assert!(sin_milli(first.1.sub(landing), landing.sub(at)) <= 20, "along the throw's line: {:?} {:?}", first.1.sub(landing), landing.sub(at));
    // The second bounce: 7 frames later, from where the first landed, on along the same line.
    let b2 = (b1..bombs.len()).find(|k| with(*k, 0).is_some()).expect("a second bounce");
    assert_eq!(b2, b1 + 7, "the first bounce lands 7 frames after the throw's landing");
    let second = with(b2, 0).unwrap();
    assert_eq!(second.0, first.1, "on the first bounce's landing point");
    assert!((2499 * K..=2500 * K).contains(&second.1.dist(first.1)), "2500 on: {}", second.1.dist(first.1));
    assert!(sin_milli(second.1.sub(first.1), first.1.sub(landing)) <= 2, "the same line");
    // It lands 7 frames later on the Knight, for what the throw's landing took off the Giant, and bounces no more.
    assert!(with(b2 + 6, 0).is_some() && with(b2 + 7, 0).is_none(), "the second bounce lands on frame {}", b2 + 7);
    assert_eq!(lost[b2 + 7].1, lost[b1].0, "the second bounce splashes the Knight as the bomb did the Giant");
    assert!(lost[b1..b2 + 7].iter().all(|l| l.1 == 0), "nothing reached the Knight before: {:?}", &lost[b1..b2 + 7]);
    assert!((b2 + 7..b2 + 20).all(|k| bombs[k].iter().all(|b| b.2 == 2)), "no third bounce");
}
