//! THE HERO MEGA MINION (card.rs `WarpDef`, `AbilityEffect::Warp`, WARP_FIRST_STEP; state.rs `WarpBoard`, `warp_pass`,
//! the warp's step in the 16402 move pass, `land_warps`, `strike_after_fire`), at level 11.
//!
//! THE MEASUREMENTS (sp-form-MegaMinion-hero-s0; the press issued on t200, the cast t201-t204):
//!   - the hero stood on t205 and stepped 343, 742, 1144, 1557, 1159, 1548 and 682 on t206-t212 to the furthest of the
//!     lowest-max-hitpoint enemies (a Skeleton 7200 off), each step aimed at its centre after its own move;
//!   - it stood on that centre on t212, swung on t213 (progress 1500) and the Skeleton was gone on t214.
//!
//! Read off the table, not measured: the strike shot's damage (156 at level 1) and its crown share (25 %), the later
//! shots' crown share (50 %), the hiding until the strike, the button from 1500 ms after the hero's creation.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_mega_minion`):
//!   - ability_available_ignores_windows -> `its_button_waits_1500_ms_and_a_pick` red (the row reads available from the
//!     hero's first tick while the check refuses the press; common.rs `assert_buttons_agree`);
//!   - warp_swing_held_a_tick -> `the_warp_steps_up_to_its_speed_and_stands_on_its_pick` red;
//!   - warp_never -> `the_warp_steps_up_to_its_speed_and_stands_on_its_pick` and `its_strike_shot_takes_156` red;
//!   - warp_full_speed -> `the_warp_steps_up_to_its_speed_and_stands_on_its_pick` red;
//!   - warp_no_instant_hit -> the same red (the kill's tick);
//!   - strike_shot_unread -> `its_strike_shot_takes_156` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DeployError};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["MegaMinion", "Knight", "Archer", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
/// On blue's side, out of every red tower's reach.
const AT: (i32, i32) = (9000, 9000);

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s
}

/// Red `units` (card, point) put and held on their points, then the hero put at AT (created after them, so in the move
/// pass they step before it, as the client's Skeleton did) and held there for 40 ticks.
fn start(units: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<(EntityId, Vec2)>) {
    let mut s = battle();
    // Blue's princess towers down (its king wakes; its reach stops short of y 12000): no crown tower reaches the scene.
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    let reds: Vec<(EntityId, Vec2)> = units
        .iter()
        .map(|(card, p)| (s.scenario_spawn_now(Team::Red, card, n(p.0, p.1), None).expect("a red unit"), n(p.0, p.1)))
        .collect();
    s.spawn_unit(Team::Blue, "MegaMinion_hero", n(AT.0, AT.1), None).expect("the hero");
    s.tick();
    let hero = find_live(&s, Team::Blue, "MegaMinion_hero")[0].id;
    for _ in 0..40 {
        assert!(s.debug_set_pos(hero, n(AT.0, AT.1)));
        for (id, p) in &reds {
            assert!(s.debug_set_pos(*id, *p));
        }
        s.tick();
    }
    (s, hero, reds)
}

#[test]
fn the_warp_steps_up_to_its_speed_and_stands_on_its_pick() {
    // A red Knight 3000 ahead (1766 max hitpoints) and a red Skeleton (81) held 7200 off on the ground, as the client's
    // (a flier would share the hero's air and push it off its centre): the pick is the Skeleton. After the cast and the trigger's tick the hero steps 343, then 400 more a tick while under 1500 and 400
    // less while over: 343, 743, 1143, 1543, 1143, 1543, and a last step onto the Minion's centre (where the move pass has
    // it, after its own step). It swings on the next tick, and the Minion is gone on the one after.
    let minion = (AT.0 - 4320, AT.1 + 5760);
    let (mut s, hero, reds) = start(&[("Knight", (AT.0, AT.1 + 3000)), ("Skeleton", minion)]);
    let target = reds[1].0;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut steps = Vec::new();
    let mut prev = s.entity(hero).expect("the hero").pos;
    let (mut arrived, mut gone) = (None, None);
    for f in 0..30 {
        // Held until the hero stands on its pick; then let go, as the client's (a held Minion would push the hero off it).
        for (id, p) in reds.iter().filter(|_| arrived.is_none()) {
            if s.entity(*id).is_some() {
                assert!(s.debug_set_pos(*id, *p));
            }
        }
        s.tick();
        let now = s.entity(hero).expect("the hero").pos;
        let (dx, dy) = (((now.x - prev.x) / K) as i64, ((now.y - prev.y) / K) as i64);
        steps.push(isqrt(dx * dx + dy * dy));
        prev = now;
        match s.entity(target) {
            Some(t) if arrived.is_none() && t.pos == now => arrived = Some(f),
            None if gone.is_none() => gone = Some(f),
            _ => {}
        }
    }
    let first = steps.iter().position(|d| *d > 0).expect("the hero warps");
    let want = [343, 743, 1143, 1543, 1143, 1543];
    for (k, w) in want.iter().enumerate() {
        // Each step truncated to whole native units on each axis: a few units short of the law along a diagonal.
        assert!((steps[first + k] - w).abs() <= 3, "step {k}: {} for {w}: {steps:?}", steps[first + k]);
    }
    let a = arrived.expect("on the Minion's centre");
    assert_eq!(a, first + want.len(), "the seventh step lands it: {steps:?}");
    // Its swing on a + 1 (the instant hit), the Minion gone on a + 2 (measured: t212, t213, t214).
    assert_eq!(gone, Some(a + 2), "the Minion gone on the arrival + 2: {steps:?}");
}

#[test]
fn its_strike_shot_takes_156() {
    // A red Knight alone (held, its hitpoints not topped up): the warp's strike takes 156 on the hero's ladder, the shot
    // after it 122, its ordinary shot.
    let (mut s, hero, reds) = start(&[("Knight", (AT.0, AT.1 + 4000))]);
    let knight = reds[0].0;
    let db = s.cards().clone();
    let form = db.index("MegaMinion_hero").expect("the form");
    let (strike, plain) = (db.scaled(form, 11, 156).unwrap(), db.scaled(form, 11, 122).unwrap());
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut losses = Vec::new();
    for _ in 0..120 {
        assert!(s.debug_set_pos(knight, reds[0].1));
        let before = s.entity(knight).expect("the Knight").hp;
        s.tick();
        let after = s.entity(knight).expect("the Knight").hp;
        if after < before {
            losses.push(before - after);
        }
        let _ = hero;
    }
    assert!(losses.len() >= 2, "two shots in 120 ticks: {losses:?}");
    assert_eq!(losses[..2], [strike, plain], "the strike, then an ordinary shot: {losses:?}");
}

#[test]
fn its_button_waits_1500_ms_and_a_pick() {
    // Its button is refused before 1500 ms after its creation, and with no enemy troop to pick.
    let mut s = battle();
    s.spawn_unit(Team::Blue, "MegaMinion_hero", n(AT.0, AT.1), None).expect("the hero");
    s.scenario_spawn_now(Team::Red, "Knight", n(AT.0, AT.1 + 4000), None).expect("a red Knight");
    // The row reads the wait and the pick as the check does, every tick (RoyaleGym, 2026-10-05: it read available from
    // the hero's first tick while every press was refused).
    for _ in 0..20 {
        s.tick();
        assert_buttons_agree(&s, Team::Blue);
    }
    assert!(matches!(s.check_ability_button(Team::Blue, 0), Err(DeployError::AbilityNotReady)), "20 ticks after its creation");
    assert!(!s.ability_buttons(Team::Blue)[0].available, "the row: not ready 20 ticks after its creation");
    for _ in 0..15 {
        s.tick();
        assert_buttons_agree(&s, Team::Blue);
    }
    assert!(s.ability_buttons(Team::Blue)[0].available, "the row: ready with a Knight to pick");
    assert!(s.check_ability_button(Team::Blue, 0).is_ok(), "35 ticks after, with a Knight to pick");
    let mut t = battle();
    t.spawn_unit(Team::Blue, "MegaMinion_hero", n(AT.0, AT.1), None).expect("the hero");
    for _ in 0..40 {
        t.tick();
        assert_buttons_agree(&t, Team::Blue);
    }
    assert!(matches!(t.check_ability_button(Team::Blue, 0), Err(DeployError::AbilityNotReady)), "no enemy troop to pick");
    assert!(!t.ability_buttons(Team::Blue)[0].available, "the row: no enemy troop to pick");
}
