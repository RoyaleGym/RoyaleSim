//! THE EVO WALL BREAKERS (tools/extract_cards.py `death_action_block`; card.rs the form's death-spawn unit,
//! `CardDef::death_crown_pct`; state.rs `phase_reap`), at level 11 (330 max hitpoints).
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-Wallbreakers-evo-s0): each evolved Wall Breaker killed (on t879 and
//! t933) left a runner of 163 max hitpoints on its point, and the one killed beside a Knight took 192 (75 at level 1)
//! off it on the next tick (t880).
//! Oracle's sp-f2-wb-s0: two that reached a princess tower (kamikaze) left no unit, and the tower took 281 each alone.
//! Read off the table, not measured: the blow's 86 % on a crown tower.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_wall_breakers`):
//!   - death_action_dropped -> both tests red;
//!   - death_crown_pct_unread -> `the_blow_takes_86_percent_off_a_crown_tower` red;
//!   - death_action_on_kamikaze -> `a_kamikaze_leaves_no_runner_and_no_blow` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The form's pair played at (9000, 12500), the second moved far off and killed at once: (the battle, the first).
fn battle() -> (BattleState, EntityId) {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Wallbreakers".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "Wallbreakers_EV1", n(9000, 12500), None).expect("the Wall Breakers");
    s.tick();
    let wbs: Vec<EntityId> = find_live(&s, Team::Blue, "Wallbreakers_EV1").iter().map(|e| e.id).collect();
    assert_eq!(wbs.len(), 2, "the form's pair");
    assert!(s.debug_set_pos(wbs[1], n(3000, 5000)));
    assert!(s.debug_set_hp(wbs[1], 0));
    s.tick();
    (s, wbs[0])
}

/// Kill `wb` held on `at`: (the kill's tick count from here, the dead unit's point).
fn kill(s: &mut BattleState, wb: EntityId, at: Vec2) {
    assert!(s.debug_set_pos(wb, at));
    assert!(s.debug_set_hp(wb, 0));
    s.tick();
    assert!(s.entity(wb).is_none(), "the Wall Breaker died");
}

#[test]
fn a_killed_one_leaves_a_runner_on_its_point_and_a_blow_of_192_on_the_next_tick() {
    let (mut s, wb) = battle();
    let at = n(9000, 12500);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 13900), None).expect("a red Knight");
    s.tick();
    let before = find_live(&s, Team::Blue, "Wallbreaker_mini").len();
    let top = s.entity(knight).expect("the Knight").max_hp;
    assert!(s.debug_set_hp(knight, top));
    assert!(s.debug_set_pos(knight, n(9000, 13900)));
    kill(&mut s, wb, at);
    // The runner: one more, 163 max hitpoints (64 at level 11), within a step of the point.
    let runners: Vec<_> = find_live(&s, Team::Blue, "Wallbreaker_mini").into_iter().filter(|e| (e.pos.y / K - 12500).abs() < 400).collect();
    assert_eq!(find_live(&s, Team::Blue, "Wallbreaker_mini").len(), before + 1, "one runner more");
    assert!(runners.len() == 1 && runners[0].max_hp == 163, "the runner on the point: {:?}", runners.iter().map(|e| (e.pos.x / K, e.pos.y / K, e.max_hp)).collect::<Vec<_>>());
    // The blow lands on the next tick: nothing on the kill's tick, 192 on the next.
    assert_eq!(s.entity(knight).expect("the Knight").hp, top, "no blow on the kill's tick");
    s.tick();
    assert_eq!(s.entity(knight).expect("the Knight").hp, top - 192, "the blow on the next tick");
}

#[test]
fn the_blow_takes_86_percent_off_a_crown_tower() {
    // Killed 2500 from the red princess tower at (14500, 25500): 192 at 86 %, 165.
    let (mut s, wb) = battle();
    let tower = s.entities().find(|e| e.team == Team::Red && e.kind == royalesim::entity::EntityKind::PrincessTower && e.pos.x / K == 14500).map(|e| e.id).expect("the red right princess tower");
    let top = s.entity(tower).expect("the tower").hp;
    kill(&mut s, wb, n(14500, 23000));
    assert_eq!(s.entity(tower).expect("the tower").hp, top, "no blow on the kill's tick");
    s.tick();
    assert_eq!(s.entity(tower).expect("the tower").hp, top - 165, "the blow's 86 % of 192 on the next tick");
}

#[test]
fn a_kamikaze_leaves_no_runner_and_no_blow() {
    // Put down 2500 short of the red right princess tower and let be: it walks in and fires, its 281 (110 at level 1) the
    // tower's only loss beside its own shots' targets, and no runner more.
    let (mut s, wb) = battle();
    let tower = s.entities().find(|e| e.team == Team::Red && e.kind == royalesim::entity::EntityKind::PrincessTower && e.pos.x / K == 14500).map(|e| e.id).expect("the red right princess tower");
    let top = s.entity(tower).expect("the tower").hp;
    let runners = find_live(&s, Team::Blue, "Wallbreaker_mini").len();
    assert!(s.debug_set_pos(wb, n(14500, 23000)));
    let mut gone = None;
    for k in 0..200 {
        s.tick();
        if gone.is_none() && s.entity(wb).is_none() {
            gone = Some(k);
        }
        if gone.is_some_and(|g| k >= g + 20) {
            break;
        }
    }
    assert!(gone.is_some(), "the Wall Breaker never reached the tower");
    assert_eq!(s.entity(tower).expect("the tower").hp, top - 281, "the kamikaze's 281 alone");
    assert_eq!(find_live(&s, Team::Blue, "Wallbreaker_mini").len(), runners, "a runner after a kamikaze");
}
