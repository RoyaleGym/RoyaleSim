//! pathfinding.HELD_PRESS_ROUTE: whether the press of a ground hero's button that holds it (CastTime > 0) drops the route
//! it walks (state.rs `start_ability`, `held_replan`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, every press after which the hero stood, 56 of 61: its route
//! read empty through the hold, and on its first walking frame its face pointed from its pre-move point at the centre of
//! a cell around it, the first node of a route planned afresh (sp-sk-souls-own-s0 t329: the Skeleton King after his cast,
//! from (14693, 16337) at (14750, 16750), where the engine walked on at its old waypoint (14750, 17750)).
//!
//! THE SCENE: the Monk (his button holds him 96 ticks: a 933 ms cast, then his deflect) put down at (3500, 12500) alone,
//! walked 38 ticks, then pressed; his facing on his first walking tick after the hold, the point he stood on before it,
//! and his facing the tick before the press.
//!
//! WHAT IS PINNED, and the plant that turns it red (held_press_route_kept):
//!   1. client15535_replanned: the first walking tick's facing points from the Monk's previous point at the centre of a
//!      cell around it (500-unit cells), and differs from kept's; kept (the engine's, the vacuity check): the facing
//!      the tick before the press, his route's;
//!   2. both arms hold him the same 96 ticks (the hold itself is untouched);
//!   3. the shipped value is kept (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, HeldPressRoute};
use royalesim::Team;

/// The engine's heading arithmetic: d * 256 / isqrt(|d|^2) per axis, truncated.
fn heading(from: (i64, i64), to: (i64, i64)) -> (i64, i64) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let l = isqrt(dx * dx + dy * dy);
    (dx * 256 / l, dy * 256 / l)
}

/// The first walking tick's facing after the hold, the facing the tick before the press, the Monk's point before that
/// walking tick (native units) and the ticks from the press to it.
type Walk = ((i64, i64), (i64, i64), (i64, i64), u32);

/// Under `arm`: its `Walk`.
fn first_walk(arm: HeldPressRoute) -> Walk {
    let mut cfg: BattleConfig = config();
    cfg.calib.held_press_route = arm;
    let deck: Vec<String> = ["Monk", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|s| s.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(15, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "Monk", Vec2::new(3500 * K, 12500 * K)).expect("the play");
    for _ in 0..38 {
        s.tick();
    }
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let m = find_live(&s, Team::Blue, "Monk")[0].id;
    let before = s.entity(m).expect("the Monk").facing;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let p = s.tick_count();
    for _ in 0..200 {
        let at = s.entity(m).expect("the Monk").pos;
        s.tick();
        let e = s.entity(m).expect("the Monk");
        if e.pos != at && s.tick_count() > p + 5 {
            let f = e.facing;
            return ((f.x as i64, f.y as i64), (before.x as i64, before.y as i64), ((at.x / K) as i64, (at.y / K) as i64), s.tick_count() - p);
        }
    }
    panic!("{arm:?}: the scene drifted: the Monk never walked again");
}

/// Plant: held_press_route_kept.
#[test]
fn a_held_heros_press_plans_its_route_afresh_under_client15535_replanned() {
    let (kept, kept_before, _, kept_hold) = first_walk(HeldPressRoute::Kept);
    // NOT VACUOUS: the engine's arm walks on along its route.
    assert_eq!(kept, kept_before, "kept: the first walking tick's facing is not the route's");
    let (fresh, _, at, fresh_hold) = first_walk(HeldPressRoute::Client15535Replanned);
    assert_eq!(fresh_hold, kept_hold, "the hold moved: kept {kept_hold}, client15535_replanned {fresh_hold}");
    assert_ne!(fresh, kept, "client15535_replanned: the first walking tick's facing is kept's ({kept:?})");
    let (col, row) = (at.0 / 500, at.1 / 500);
    let around: Vec<(i64, i64)> = (-1..=1).flat_map(|dr| (-1..=1).map(move |dc| ((col + dc) * 500 + 250, (row + dr) * 500 + 250))).collect();
    assert!(
        around.iter().any(|c| heading(at, *c) == fresh),
        "client15535_replanned: the first walking tick's facing {fresh:?} points at no cell around {at:?}"
    );
}

#[test]
fn the_shipped_value_is_kept() {
    assert_eq!(Calib::shipped().held_press_route, HeldPressRoute::Kept);
}

/// Under `arm`: a Blue Little Prince played on (3500, 9500), walking his lane, pressed: the route's length after every tick of
/// his stand, his first walking tick's facing, his point before it (native) and the ticks from the press to it.
fn lp_stand(arm: HeldPressRoute) -> (Vec<usize>, (i64, i64), (i64, i64), u32) {
    let mut cfg: BattleConfig = config();
    cfg.calib.held_press_route = arm;
    let deck: Vec<String> = ["LittlePrince", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|s| s.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(15, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "LittlePrince", Vec2::new(3500 * K, 9500 * K)).expect("the play");
    for _ in 0..40 {
        s.tick();
    }
    let lp = find_live(&s, Team::Blue, "LittlePrince")[0].id;
    assert!(!s.entity(lp).expect("him").route.is_empty(), "the scene drifted: he walks no route");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let p = s.tick_count();
    let mut lens = Vec::new();
    for _ in 0..200 {
        let at = s.entity(lp).expect("him").pos;
        s.tick();
        let e = s.entity(lp).expect("him");
        if e.pos != at && s.tick_count() > p + 5 {
            return (lens, (e.facing.x as i64, e.facing.y as i64), ((at.x / K) as i64, (at.y / K) as i64), s.tick_count() - p);
        }
        lens.push(e.route.len());
    }
    panic!("{arm:?}: the scene drifted: he never walked again");
}

/// pathfinding.HELD_PRESS_ROUTE = client16402_replanned_after_stand (client 16.402, parity r76, live13: 472 of 477 Little Prince
/// presses empty through the stand, 422 stepping at a fresh first node): his route stays empty through the stand and his first
/// step points at a cell around him; under client15535_replanned (the vacuity check) he plans inside the stand. Plant:
/// held_press_route_stand_plans.
#[test]
fn under_client16402_replanned_after_stand_a_little_prince_holds_no_route_through_his_stand() {
    let (old_lens, _, _, old_w) = lp_stand(HeldPressRoute::Client15535Replanned);
    assert!(old_lens.iter().any(|&n| n > 0), "client15535_replanned: no route inside the stand: {old_lens:?}");
    let (lens, face, at, w) = lp_stand(HeldPressRoute::Client16402ReplannedAfterStand);
    assert_eq!(w, old_w, "the stand's length moved");
    assert!(lens.iter().all(|&n| n == 0), "client16402_replanned_after_stand: a route inside the stand: {lens:?}");
    let (col, row) = (at.0 / 500, at.1 / 500);
    let around: Vec<(i64, i64)> = (-1..=1).flat_map(|dr| (-1..=1).map(move |dc| ((col + dc) * 500 + 250, (row + dr) * 500 + 250))).collect();
    assert!(around.iter().any(|c| heading(at, *c) == face), "his first step {face:?} points at no cell around {at:?}");
}
