//! THE SUPER MINI PEKKA EVENT CARD (item 298; card.rs `bottle_body`, target.rs `can_target`): its Spawn* block puts down a
//! pancake, a bottle with a body, every 3000 ms from its deploy end + 1000: a one-hitpoint building nobody targets and no hit
//! reaches, standing for its 1000 ms fuse, whose death leaves a one-shot heal area for its own troops.
//!
//! THE MEASUREMENT (client 15.535.29, sp-event-SuperMiniPekka-s0, level 11): emissions on its activation A + 20 + 60k (the
//! own Knight shoved on t346 and t406); the Knight healed +53 on E + 25, 30, 35 and 40 of each (12 of 12, 100 a second every
//! 250 ms on the pancake's Rare ladder at 212 %); an enemy tower 2600 off gained nothing.
//!
//! WHAT IS PINNED: a lone Blue Super Mini PEKKA held in place: its pancakes, Rare buildings of one hitpoint, appear 19 ticks
//! after its deploy ends (its activation + 20) and every 60 ticks after, about 900 ahead of it, and each stands 19 ticks; a damaged Blue Knight beside the
//! point gains 53 on E + 25, 30, 35 and 40 and on no other tick; a damaged Blue Cannon there gains nothing; a Red Knight
//! held 1500 from the point never targets a pancake.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test super_mini_pekka`):
//!   * `spawned_bottle_refused` -- the card is refused again: every test goes red;
//!   * `bottle_body_targetable` -- the pancake is a target: (2) goes red;
//!   * `bottle_body_full_fuse` -- the body lives its whole fuse: the heals come a tick late, (1) red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn level11() -> BattleConfig {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

const SMP: (i32, i32) = (9000, 9000);

/// A held Blue Super Mini PEKKA and `extra` (team, card, point, hp; each held), run 160 ticks past its deploy end: the
/// deploy end tick, each pancake (id, first tick, last tick, point, card name, max hp), and each extra unit's hp by tick.
struct Run {
    ready: u32,
    pancakes: Vec<(EntityId, u32, u32, Vec2, String, i32)>,
    hp: Vec<Vec<(u32, i32)>>,
    targeted_pancake: bool,
}

/// An extra unit: its side, its card, its native point and its hp (None: the card's).
type Extra<'a> = (Team, &'a str, (i32, i32), Option<i32>);

fn run(extra: &[Extra<'_>]) -> Run {
    let mut s = BattleState::new(27, level11());
    past_deploy_lockout(&mut s);
    let smp = s.scenario_spawn_now(Team::Blue, "SuperMiniPekka", at(SMP), None).expect("the Super Mini PEKKA loads and stands");
    let ids: Vec<EntityId> = extra
        .iter()
        .map(|&(team, card, p, hp)| {
            let id = s.scenario_spawn_now(team, card, at(p), None).unwrap_or_else(|e| panic!("{card}: {e:?}"));
            if let Some(h) = hp {
                assert!(s.debug_set_hp(id, h));
            }
            id
        })
        .collect();
    let mut ready: Option<u32> = None;
    let mut pancakes: Vec<(EntityId, u32, u32, Vec2, String, i32)> = Vec::new();
    let mut hp: Vec<Vec<(u32, i32)>> = vec![Vec::new(); ids.len()];
    let mut targeted_pancake = false;
    for _ in 0..240 {
        s.debug_set_pos(smp, at(SMP));
        for (k, &id) in ids.iter().enumerate() {
            s.debug_set_pos(id, at(extra[k].2));
        }
        s.tick();
        let t = s.tick_count() - 1;
        if ready.is_none() && !s.entity(smp).expect("the Super Mini PEKKA").deploying {
            ready = Some(t);
        }
        for e in s.entities().filter(|e| e.spawned_by == Some(smp)) {
            match pancakes.iter_mut().find(|p| p.0 == e.id) {
                Some(p) => p.2 = t,
                None => pancakes.push((e.id, t, t, e.pos, e.card.to_string(), e.max_hp)),
            }
        }
        for (k, &id) in ids.iter().enumerate() {
            if let Some(e) = s.entity(id) {
                hp[k].push((t, e.hp));
                if e.target.is_some_and(|tg| pancakes.iter().any(|p| p.0 == tg)) {
                    targeted_pancake = true;
                }
            }
        }
        if ready.is_some_and(|r| t >= r + 160) {
            break;
        }
    }
    Run { ready: ready.expect("its deploy ended"), pancakes, hp, targeted_pancake }
}

/// The ticks an hp series rose on, and by how much.
fn gains(series: &[(u32, i32)]) -> Vec<(u32, i32)> {
    series.windows(2).filter(|w| w[1].1 > w[0].1).map(|w| (w[1].0, w[1].1 - w[0].1)).collect()
}

/// (1) The pancakes and the heal. Plant: spawned_bottle_refused.
#[test]
fn pancakes_every_60_ticks_heal_own_troops_53_four_times() {
    let r = run(&[(Team::Blue, "Knight", (10000, 9900), Some(1000)), (Team::Blue, "Cannon", (8000, 11000), Some(300))]);
    let a = r.ready;
    let firsts: Vec<u32> = r.pancakes.iter().map(|p| p.1 - a).collect();
    // 19 after the first tick it is not deploying, as the Super Witch's waves (the same SpawnStartTime 1000): its
    // activation + 20, measured.
    assert_eq!(firsts, [19, 79, 139], "a pancake on its activation + 20, then every 60: {:?}", r.pancakes);
    for p in &r.pancakes[..2] {
        assert_eq!(p.2 - p.1, 18, "each pancake stands 19 ticks (its area acts on the 20th): {p:?}");
        // Ahead of it at about its radius plus the pancake's (450 + 450) along its facing, rounded to a degree
        // (spawner.SPAWN_POINT): the facing is its walk's, so the point is not pinned.
        let (dx, dy) = ((p.3.x - at(SMP).x) / K, (p.3.y - at(SMP).y) / K);
        let d = royalesim::fixed::isqrt((dx * dx + dy * dy) as i64) as i32;
        assert!(dy > 0 && (880..=960).contains(&d), "a pancake {d} from it at ({dx}, {dy})");
        // Its one hitpoint on its Rare ladder: 2 at level 11; its LifeTime drains it in 20 ticks whatever the number.
        assert_eq!((p.4.as_str(), p.5), ("SuperMiniPekkaPancakes", 2), "a pancake of one hitpoint at level 1");
    }
    let e: Vec<u32> = r.pancakes.iter().map(|p| p.1).collect();
    let want: Vec<(u32, i32)> = e[..2].iter().flat_map(|&e| [25, 30, 35, 40].map(|d| (e + d, 53))).collect();
    let got: Vec<(u32, i32)> = gains(&r.hp[0]).into_iter().filter(|g| g.0 <= e[1] + 50).collect();
    assert_eq!(got, want, "the Knight gains 53 on E + 25, 30, 35, 40 of each pancake, and on no other tick");
    assert!(gains(&r.hp[1]).is_empty(), "the Cannon (a building) gains nothing: {:?}", gains(&r.hp[1]));
}

/// (2) Nobody targets a pancake. Plants: spawned_bottle_refused, bottle_body_targetable.
#[test]
fn an_enemy_beside_a_pancake_never_targets_it() {
    let r = run(&[(Team::Red, "Knight", (9000, 11400), None)]);
    assert!(!r.pancakes.is_empty(), "no pancake came");
    assert!(!r.targeted_pancake, "the Red Knight, 1500 from the pancakes, targeted one");
}
