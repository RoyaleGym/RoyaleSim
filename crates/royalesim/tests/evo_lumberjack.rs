//! THE EVO LUMBERJACK'S GHOST (tools/extract_cards.py `rage_ghost_block`; card.rs `RageGhostDef`,
//! RAGE_GHOST_APPEAR_TICKS, RAGE_GHOST_LAPSE_EXTRA_TICKS; state.rs `RageGhostRun`, `rage_ghost_release`,
//! `rage_ghost_bind`, `rage_ghost_pass`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; Oracle's sp-lumber-fight-s0, -arrows-s0, -tower-s0 and -base-s0; D the tick the
//! Lumberjack is gone): the ghost first seen D+12 on his death point at 2 hitpoints, deploying 20 frames; standing on
//! the Rage's point its last frame is its first + 109; flying off (2860 from the point at its first + 60, 3574 at + 66)
//! its last frame is its first + 84; the base Lumberjack leaves none.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_lumberjack`): rage_ghost_never, rage_ghost_never_killed.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// A point's distance from another, native.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = (((a.x - b.x) / K) as i64, ((a.y - b.y) / K) as i64);
    isqrt(dx * dx + dy * dy)
}

const AT: (i32, i32) = (9000, 12500);
const GHOST: &str = "RageBarbarianEvoGhost";

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["RageBarbarian".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
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

/// `unit` put at AT on 1 hitpoint with a red Knight held 1200 ahead, until the Knight's blow kills it: the battle, the
/// Knight and D (the first tick it is gone).
fn killed(unit: &str) -> (BattleState, EntityId, u32) {
    let mut s = battle();
    let (at, k_at) = (n(AT.0, AT.1), n(AT.0, AT.1 + 1200));
    s.spawn_unit(Team::Blue, unit, at, None).expect("the Lumberjack");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).expect("the Knight");
    s.tick();
    let lj = find_live(&s, Team::Blue, unit).first().expect("the Lumberjack").id;
    for _ in 0..200 {
        assert!(s.debug_set_pos(lj, at));
        assert!(s.debug_set_pos(knight, k_at));
        assert!(s.debug_set_hp(lj, 1));
        s.tick();
        if s.entity(lj).is_none() {
            let died = s.tick_count() - 1;
            return (s, knight, died);
        }
    }
    panic!("{unit}: not killed in 200 ticks");
}

/// Each tick from D: the ghost's point and whether it deploys, while `hold` puts it back on AT (None: no ghost).
fn ghost_rows(s: &mut BattleState, knight: EntityId, ticks: u32, hold: bool) -> Vec<(u32, Option<(Vec2, bool)>)> {
    let mut rows = Vec::new();
    for _ in 0..ticks {
        // The Knight out of the scene: far off on red's side, held there.
        assert!(s.debug_set_pos(knight, n(3500, 28000)));
        if hold {
            if let Some(g) = find_live(s, Team::Blue, GHOST).first().map(|e| e.id) {
                assert!(s.debug_set_pos(g, n(AT.0, AT.1)));
            }
        }
        s.tick();
        let g = find_live(s, Team::Blue, GHOST).first().map(|e| (e.pos, e.deploying));
        rows.push((s.tick_count() - 1, g));
    }
    rows
}

#[test]
fn its_ghost_appears_on_his_death_point_twelve_ticks_on_and_lives_110_ticks_on_the_rage() {
    let (mut s, knight, d) = killed("RageBarbarian_EV1");
    let rows = ghost_rows(&mut s, knight, 150, true);
    let seen: Vec<_> = rows.iter().filter(|r| r.1.is_some()).collect();
    let first = seen.first().map(|r| r.0).expect("a ghost");
    assert_eq!(first, d + 12, "the ghost's first frame, from D {d}");
    let (at, deploying) = seen[0].1.expect("seen");
    assert!(dist(at, n(AT.0, AT.1)) <= 1 && deploying, "on his death point, deploying: {:?}", seen[0]);
    let hp = find_live(&s, Team::Blue, GHOST).first().map(|e| e.max_hp);
    let last = seen.last().expect("seen").0;
    assert_eq!(last, first + 109, "its last frame, held on the Rage's point (max hp {hp:?})");
}

#[test]
fn its_ghost_out_of_the_rage_goes_a_second_and_five_ticks_after_the_last_pulse_that_reached_it() {
    let (mut s, knight, _) = killed("RageBarbarian_EV1");
    let rows = ghost_rows(&mut s, knight, 150, false);
    let seen: Vec<_> = rows.iter().filter_map(|r| r.1.map(|g| (r.0, g.0))).collect();
    let first = seen.first().expect("a ghost").0;
    // The Rage's pulses reach it at ages 0, 6, 12, ... while its centre stands within 3000 of the point.
    let last_in = seen.iter().filter(|(t, p)| (t - first) % 6 == 0 && t - first < 110 && dist(*p, n(AT.0, AT.1)) <= 3000).map(|(t, _)| *t).max().expect("a pulse reached it");
    assert!(last_in < first + 84, "it left the Rage before its life ran out: {seen:?}");
    let last = seen.last().expect("seen").0;
    assert_eq!(last, last_in + 24, "its last frame, from its last reached pulse {last_in}");
}

#[test]
fn the_base_lumberjack_leaves_no_ghost() {
    let (mut s, knight, _) = killed("RageBarbarian");
    let rows = ghost_rows(&mut s, knight, 40, false);
    assert!(rows.iter().all(|r| r.1.is_none()), "a ghost from the base: {rows:?}");
}
