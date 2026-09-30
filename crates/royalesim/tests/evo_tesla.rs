//! THE EVO TESLA (card.rs `RingDef`; state.rs `RingRun`, `ring_pass`, the ring made in the hide pass), at level 11.
//!
//! THE MEASUREMENTS (sp-form-Tesla-evo-s0; the Tesla up, its first target taken, on t1103):
//!   - a Knight lost 148 (DamagePerSecond 58 at level 1, one hit: HitFrequency -1) and stood still from t1134 to t1143
//!     (the stop's 500 ms), moving again on t1144;
//!   - three enemies were taken on t1129, t1131 and t1134 and missed on the ticks before, at start-of-tick centre
//!     distances 5242, 5862 and 6260 against 5330, 5951 and 6317: radius = 1 + 5999 x age / 1500 from age 0 on t1104,
//!     plus the enemy's own radius (500).
//!
//! Read off the table, not measured: the ring made at the Tesla's creation.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_tesla`):
//!   - ring_never -> both tests red;
//!   - ring_full_size -> `the_ring_takes_each_enemy_once_as_it_grows` red.
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
    cfg.decks = [vec!["Tesla".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// An evolved Tesla put down at (9000, 11000) and left 40 ticks to deploy and go under; its id and point (native).
fn tesla() -> (BattleState, EntityId, (i32, i32)) {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Tesla_EV1", n(9000, 11000), None).expect("the Tesla");
    s.tick();
    let id = find_live(&s, Team::Blue, "Tesla_EV1")[0].id;
    for _ in 0..40 {
        s.tick();
    }
    let p = s.entity(id).expect("the Tesla").pos;
    (s, id, (p.x / K, p.y / K))
}

#[test]
fn the_ring_takes_each_enemy_once_as_it_grows() {
    // Red Knights held ahead of the Tesla at centre distances 2500, 4000, 6260 and 6700 (on land short of the river,
    // out of every crown tower's reach), their hitpoints topped up every tick. The ring reaches 2000, 3600 and 5800
    // (plus the Knight's 500) on ages 10, 18 and 29: the first update after the tick the Tesla takes its first target,
    // then 8 and 19 ticks on; 6700 is past 6000 + 500.
    let (mut s, t, (x, y)) = tesla();
    // (dx, dy) from the Tesla: 2500; 4000 (2400, 3200); 6260 (-4975, 3800); 6700 (5518, 3800).
    let at = [(0, 2500), (2400, 3200), (-4975, 3800), (5518, 3800)];
    let reds: Vec<(EntityId, Vec2, i32)> = at
        .iter()
        .map(|(dx, dy)| {
            let p = n(x + dx, y + dy);
            let id = s.scenario_spawn_now(Team::Red, "Knight", p, None).expect("a red Knight");
            (id, p, s.entity(id).expect("the Knight").max_hp)
        })
        .collect();
    let mut first_target = None;
    let mut hits: Vec<Vec<usize>> = vec![Vec::new(); at.len()];
    for f in 0..120 {
        for (id, p, top) in &reds {
            assert!(s.debug_set_pos(*id, *p));
            assert!(s.debug_set_hp(*id, *top));
        }
        s.tick();
        if first_target.is_none() && s.entity(t).expect("the Tesla").target.is_some() {
            first_target = Some(f);
        }
        for (k, (id, _, top)) in reds.iter().enumerate() {
            if top - s.entity(*id).expect("a red Knight held alive").hp == 148 {
                hits[k].push(f);
            }
        }
    }
    let up = first_target.expect("the Tesla comes up");
    assert_eq!(hits[0], [up + 11], "2500: once, on age 10 (the tick after the pop-up is age 0): {hits:?}");
    assert_eq!(hits[1], [up + 19], "4000: once, 8 ticks later: {hits:?}");
    assert_eq!(hits[2], [up + 30], "6260: once, 19 ticks later: {hits:?}");
    assert!(hits[3].is_empty(), "6700 is past the ring's 6000 + 500: {hits:?}");
}

#[test]
fn the_ring_stops_what_it_takes_for_500_ms() {
    // A red Knight walking at the Tesla from 4500 off (on land, out of the crown towers' reach, and past its own attack
    // reach when the stop ends): on the tick the ring takes it (148) it stands, and it stands
    // 10 ticks in all (500 ms), as the client's Knight did from t1134 to t1143.
    let (mut s, _, (x, y)) = tesla();
    let red = s.scenario_spawn_now(Team::Red, "Knight", n(x + 2700, y + 3600), None).expect("a red Knight");
    let mut f = Vec::new();
    for _ in 0..80 {
        let e = s.entity(red).expect("the Knight");
        let (before, hp) = (e.pos, e.hp);
        s.tick();
        let e = s.entity(red).expect("the Knight");
        f.push((hp - e.hp, e.pos != before));
    }
    let h = f.iter().position(|(lost, _)| *lost == 148).expect("the ring takes it");
    assert!(h >= 1 && f[h - 1].1, "it walked the tick before: {f:?}");
    assert!(f[h..h + 10].iter().all(|(_, moved)| !moved), "it stands 10 ticks from the hit: {f:?}");
    assert!(f[h + 10].1, "and walks on the 11th: {f:?}");
}
