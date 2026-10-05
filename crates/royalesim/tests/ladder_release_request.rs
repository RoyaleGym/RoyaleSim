//! knockback.LADDER_PATH_REQUEST = client15535_on_release: a knocked unit with no route asks for one when the knock
//! releases it from its fight, wherever it stands from its target (state.rs `phase_path16402_for`'s pushback tick,
//! `BattleState::ladder_asks`).
//!
//! THE LAW, measured on client 15.535.29 (the ledger has the rows): one request on the unit's first update with the
//! ladder armed, with no reach test (33 of 166 ladders planned within Range + both radii).
//!
//! THE SCENE (tests/ladder_path_request.rs's, the Knights 2,150 apart: within the blue Knight's reach of Range + both
//! radii, 2,200, but out of its goal reach of Range + its own radius, 1,700): on Blue's bank a blue Knight fights a red
//! Knight; a red Fireball cast on the blue Knight knocks it back. WHAT IS PINNED:
//!   1. client15535_on_release: the blue Knight holds a route on its ladder's first tick;
//!   2. client15535_on_ladder (the reach-gated arm, the vacuity check): it holds none on that tick (still in reach);
//!   3. the shipped value is ladder_end;
//!   4. client15535_on_release, the red Knight killed on the ladder's second tick: the blue Knight, holding no live
//!      target, asks for its default tower: its route grows from the short one to the red Knight to a long one over the
//!      river (sp-form-Bowler-hero-s0 t228: the client's knocked Musketeer took Red's tower and a 14-node route that tick).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test ladder_release_request`):
//!   * `ladder_release_reach_gated` -- the new arm still asks only out of reach: (1) goes red.
//!   * `ladder_release_no_tower` -- the new arm asks nothing of a unit holding no target: (4) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, LadderPathRequest};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The blue Knight's route length on its ladder's first tick under `arm`.
fn first_ladder_route(arm: LadderPathRequest) -> usize {
    let mut cfg: BattleConfig = config();
    cfg.calib.ladder_path_request = arm;
    cfg.decks = [vec!["Knight".into(), "Zap".into()], vec!["Knight".into(), "Fireball".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(3, cfg);
    past_deploy_lockout(&mut s);
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Knight", n(9000, 12000), None), (Team::Red, "Knight", n(9000, 14150), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let blue = ids[0];
    for _ in 0..60 {
        s.tick();
    }
    let k = s.entity(blue).expect("the blue Knight");
    assert!(k.route.is_empty() && k.target.is_some(), "{arm:?}: the scene drifted: the blue Knight does not fight in place");
    let at = k.pos;
    s.spawn_unit(Team::Red, "Fireball", at, None).expect("the Fireball");
    let mut rows: Vec<(Vec2, usize)> = vec![(at, 0)];
    for _ in 0..90 {
        s.tick();
        let Some(k) = s.entity(blue) else { break };
        rows.push((k.pos, k.route.len()));
    }
    let step = |j: usize| {
        let (a, b) = (rows[j - 1].0, rows[j].0);
        let (dx, dy) = (((b.x - a.x) / K) as i64, ((b.y - a.y) / K) as i64);
        isqrt(dx * dx + dy * dy)
    };
    let j = (1..rows.len()).find(|&j| step(j) >= 100).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: no knockback"));
    rows[j].1
}

/// Plant: ladder_release_reach_gated.
#[test]
fn a_released_unit_asks_on_its_first_ladder_tick_in_reach_under_client15535_on_release() {
    assert!(first_ladder_route(LadderPathRequest::Client15535OnRelease) > 0, "new: no route on the ladder's first tick");
}

#[test]
fn the_reach_gated_value_asks_nothing_in_reach() {
    assert_eq!(first_ladder_route(LadderPathRequest::Client15535OnLadder), 0, "on_ladder: a route on the first tick in reach (vacuous otherwise)");
}

#[test]
fn the_shipped_value_is_ladder_end() {
    assert_eq!(Calib::shipped().ladder_path_request, LadderPathRequest::LadderEnd);
}

/// Plant: ladder_release_no_tower. The blue Knight's route on its ladder's first tick, and 3 ticks after the red Knight
/// is killed on the ladder's second tick, under client15535_on_release.
#[test]
fn a_released_unit_with_no_target_asks_for_its_default_tower_under_client15535_on_release() {
    let mut cfg: BattleConfig = config();
    cfg.calib.ladder_path_request = LadderPathRequest::Client15535OnRelease;
    cfg.decks = [vec!["Knight".into(), "Zap".into()], vec!["Knight".into(), "Fireball".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(3, cfg);
    past_deploy_lockout(&mut s);
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Knight", n(9000, 12000), None), (Team::Red, "Knight", n(9000, 14150), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (blue, red) = (ids[0], ids[1]);
    for _ in 0..60 {
        s.tick();
    }
    let at = s.entity(blue).expect("the blue Knight").pos;
    s.spawn_unit(Team::Red, "Fireball", at, None).expect("the Fireball");
    let mut prev = at;
    let mut first = None;
    for _ in 0..90 {
        s.tick();
        let k = s.entity(blue).expect("the blue Knight");
        let (dx, dy) = (((k.pos.x - prev.x) / K) as i64, ((k.pos.y - prev.y) / K) as i64);
        prev = k.pos;
        if isqrt(dx * dx + dy * dy) >= 100 {
            first = Some(k.route.len());
            break;
        }
    }
    let first = first.expect("the scene drifted: no knockback");
    assert!((1..=3).contains(&first), "the scene drifted: the route to the red Knight on the ladder's first tick is {first} nodes");
    assert!(s.debug_set_hp(red, 0));
    for _ in 0..4 {
        s.tick();
    }
    assert!(s.entity(red).is_none(), "the scene drifted: the red Knight lives");
    let k = s.entity(blue).expect("the blue Knight");
    assert!(k.target.is_none(), "the scene drifted: the blue Knight took a target on its ladder");
    assert!(k.route.len() > 5, "new: holding no target, the blue Knight asked for no route to its tower ({} nodes)", k.route.len());
}
