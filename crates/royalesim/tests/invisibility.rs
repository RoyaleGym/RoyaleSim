//! INVISIBLE WHEN IDLE (card.rs `CardDef::invisible_when_idle`; target.rs `invisible`; targeting.INVISIBILITY): the
//! Royal Ghost.
//!
//! THE LAW, measured on client 15.535.29 (the corpus battle and four scenario runs):
//!   - invisible from its deploy: no enemy targets it before its first hit, however near;
//!   - its own hit reveals it: enemies may target it from the tick after the hit;
//!   - it hides again 46 ticks after its last hit, which drops an enemy's lock;
//!   - area damage lands on it while it is invisible.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the Ghost's idle invisibility (2000 ms) and no hide timer;
//!   2. a red princess tower first targets a Blue Ghost walking at it on the tick after the Ghost's first hit, though
//!      the Ghost stood within the tower's reach before;
//!   3. the tower that took the Ghost on a hit holds it through hit + 45 and drops it on hit + 46, the Ghost walking
//!      and not hitting between;
//!   4. a Zap lands on an invisible Ghost;
//!   5. the Ghost's reveal is state: a save edited only in it fails the load's hash self-check.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test invisibility`):
//!   * `invisible_targetable` -- invisibility not read: (2) goes red.
//!   * `reveal_never` -- a hit reveals nothing: (2) and (3) go red.
//!   * `rehide_never` -- once revealed, visible for good: (3) goes red.
//!   * `hash_skips_reveal` -- the reveal is not hashed: (5) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

#[test]
fn the_loader_reads_the_ghosts_idle_invisibility() {
    let s = BattleState::new(0, config());
    let g = card_stat(&s, "Ghost");
    assert_eq!(g.invisible_when_idle, Some(2000), "BuffWhenNotAttackingTime");
}

/// The red princess tower nearest `p`.
fn red_tower_near(s: &BattleState, p: Vec2) -> (EntityId, Vec2) {
    s.entities().filter(|v| v.team == Team::Red && v.card == "PrincessTower").map(|v| (v.id, v.pos)).min_by_key(|v| dist(v.1, p)).expect("a red princess tower")
}

/// Per tick after the play (k = 0 the Ghost's creation tick): whether `tower` targets the Ghost, the Ghost's
/// distance to it, and whether the Ghost hit something (a red unit lost hp while the tower did not shoot the Ghost).
struct Tick {
    targets_ghost: bool,
    dist: i64,
    ghost_hit: bool,
}

fn run(s: &mut BattleState, ghost_at: (i32, i32), tower: (EntityId, Vec2), ticks: u32) -> Vec<Tick> {
    s.spawn_unit(Team::Blue, "Ghost", at(ghost_at), None).expect("play the Ghost");
    let mut out = Vec::new();
    for _ in 0..ticks {
        let red_hp: i64 = s.entities().filter(|v| v.team == Team::Red).map(|v| v.hp as i64).sum();
        s.tick();
        let Some(g) = s.entities().find(|v| v.card == "Ghost").map(|v| (v.id, v.pos)) else { break };
        let red_after: i64 = s.entities().filter(|v| v.team == Team::Red).map(|v| v.hp as i64).sum();
        let t = s.entity(tower.0);
        out.push(Tick { targets_ghost: t.and_then(|t| t.target) == Some(g.0), dist: dist(g.1, tower.1), ghost_hit: red_after < red_hp });
    }
    out
}

/// Plants: invisible_targetable, reveal_never.
#[test]
fn the_tower_first_targets_the_ghost_on_the_tick_after_its_first_hit() {
    let mut s = BattleState::new(0, config());
    let tower = red_tower_near(&s, at((14500, 26000)));
    let ticks = run(&mut s, (14500, 12000), tower, 400);
    let h = ticks.iter().position(|t| t.ghost_hit).expect("the scene drifted: the Ghost never hit");
    let t = ticks.iter().position(|t| t.targets_ghost).expect("the tower never targeted the Ghost");
    assert!(ticks[..h].iter().any(|t| t.dist <= 9000), "the scene drifted: the Ghost was never within the tower's reach before its first hit");
    assert_eq!(t, h + 1, "the tower first targets the Ghost on {t}; its first hit was on {h}");
}

/// Plants: reveal_never, rehide_never.
#[test]
fn the_tower_drops_the_ghost_46_ticks_after_its_last_hit() {
    let mut s = BattleState::new(0, config());
    let tower = red_tower_near(&s, at((14500, 26000)));
    // a red Skeleton behind the tower, on the Ghost's lane. It cannot see the invisible Ghost, so it walks out down
    // the lane toward Blue's towers. The Ghost's first hit kills it inside the tower's reach, and the Ghost then walks
    // on toward the tower for longer than 46 ticks without hitting. A Skeleton at (14500, 19000) walks to meet the
    // Ghost on Blue's side, 12003 from the tower, and the tower cannot take it there.
    s.scenario_spawn_now(Team::Red, "Skeleton", at((14500, 28500)), None).expect("spawn the Skeleton");
    let ticks = run(&mut s, (14500, 12000), tower, 400);
    let h = ticks.iter().position(|t| t.ghost_hit).expect("the scene drifted: the Ghost never hit");
    assert!(ticks.len() > h + 47, "the scene drifted: the Ghost died before hit + 46");
    assert!(ticks[h + 1..=h + 46].iter().all(|t| !t.ghost_hit), "the scene drifted: the Ghost hit again before hit + 46");
    assert!(ticks[h + 1].targets_ghost, "the scene drifted: the tower did not take the Ghost on hit + 1 (it stood {} away)", ticks[h + 1].dist);
    let held: Vec<usize> = (h + 1..=h + 46).filter(|&k| ticks[k].targets_ghost).map(|k| k - h).collect();
    assert_eq!(held, (1..=45).collect::<Vec<usize>>(), "the ticks after the hit on which the tower held the Ghost");
}

#[test]
fn a_zap_lands_on_an_invisible_ghost() {
    let mut s = BattleState::new(0, config());
    s.spawn_unit(Team::Blue, "Ghost", at((9000, 8000)), None).expect("play the Ghost");
    for _ in 0..30 {
        s.tick();
    }
    let (g, p, hp) = s.entities().find(|v| v.card == "Ghost").map(|v| (v.id, v.pos, v.hp)).expect("the Ghost stands");
    s.spawn_unit(Team::Red, "Zap", p, None).expect("cast Zap");
    for _ in 0..3 {
        s.tick();
    }
    assert!(s.entity(g).map_or(0, |v| v.hp) < hp, "the Zap did not land on the invisible Ghost");
}

/// Plant: hash_skips_reveal.
#[test]
fn the_ghosts_reveal_is_state() {
    let mut s = BattleState::new(0, config());
    s.spawn_unit(Team::Blue, "Ghost", at((9000, 8000)), None).expect("play the Ghost");
    for _ in 0..3 {
        s.tick();
    }
    let g = s.entities().find(|v| v.card == "Ghost").map(|v| v.id).expect("the Ghost stands");
    let hashed = edit_is_hashed(&s, |v| {
        let col = v["ents"]["reveal_from"].as_array_mut().expect("the snapshot carries the reveal");
        col[g.index as usize] = serde_json::Value::from(7);
    });
    assert!(hashed, "a save edited only in the Ghost's reveal loads under the old hash: the reveal is not hashed");
}
