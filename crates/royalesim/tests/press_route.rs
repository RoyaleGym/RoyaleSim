//! pathfinding.PRESS_ROUTE: whether a hero's button press drops the route it walks (state.rs `start_ability`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, every walking press frame of a ground hero whose button holds
//! it not at all, 22 of 22: its face points from its pre-move point at the centre of a cell around it, the first node of a
//! route planned afresh (sp-h10k-s0 t379: the Hero Ice Golem from (3372, 13411) at (3250, 13750), where the engine walked
//! on at its old waypoint (3250, 14750)).
//!
//! THE SCENE: sp-h10k-s0's play, the Hero Ice Golem put down at (3500, 12500) alone, walked 38 ticks, then pressed; its
//! facing on the press tick and the point it stood on before it.
//!
//! WHAT IS PINNED, and the plant that turns it red (press_route_kept):
//!   1. client15535_replanned: the press tick's facing points from the hero's previous point at the centre of a cell
//!      around it (500-unit cells), and differs from kept's; kept (the engine's, the vacuity check): the facing the tick
//!      before the press, its route's.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, PressRoute};
use royalesim::Team;

/// The engine's heading arithmetic: d * 256 / isqrt(|d|^2) per axis, truncated.
fn heading(from: (i64, i64), to: (i64, i64)) -> (i64, i64) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let l = isqrt(dx * dx + dy * dy);
    (dx * 256 / l, dy * 256 / l)
}

/// The press tick's facing, the facing the tick before and the hero's point before the press tick (native units).
fn press(arm: PressRoute) -> ((i64, i64), (i64, i64), (i64, i64)) {
    let mut cfg: BattleConfig = config();
    cfg.calib.press_route = arm;
    let deck: Vec<String> = ["IceGolemite", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|s| s.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(15, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "IceGolemite", Vec2::new(3500 * K, 12500 * K)).expect("the play");
    for _ in 0..38 {
        s.tick();
    }
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let g = find_live(&s, Team::Blue, "IceGolemite_hero")[0].id;
    let before = s.entity(g).expect("the hero").facing;
    let at = s.entity(g).expect("the hero").pos;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick();
    let f = s.entity(g).expect("the hero").facing;
    ((f.x as i64, f.y as i64), (before.x as i64, before.y as i64), ((at.x / K) as i64, (at.y / K) as i64))
}

/// Plant: press_route_kept.
#[test]
fn a_ground_heros_press_plans_its_route_afresh_under_client15535_replanned() {
    let (kept, kept_before, _) = press(PressRoute::Kept);
    // NOT VACUOUS: the engine's arm walks on along its route.
    assert_eq!(kept, kept_before, "kept: the press tick's facing is not the route's");
    let (fresh, _, at) = press(PressRoute::Client15535Replanned);
    assert_ne!(fresh, kept, "client15535_replanned: the press tick's facing is kept's ({kept:?})");
    let (col, row) = (at.0 / 500, at.1 / 500);
    let around: Vec<(i64, i64)> = (-1..=1).flat_map(|dr| (-1..=1).map(move |dc| ((col + dc) * 500 + 250, (row + dr) * 500 + 250))).collect();
    assert!(
        around.iter().any(|c| heading(at, *c) == fresh),
        "client15535_replanned: the press tick's facing {fresh:?} points at no cell around {at:?}"
    );
}
