//! THE HERO KNIGHT (card.rs `AbilityAction::SetShield`, `AbilityAction::Taunt`, `TauntDef`,
//! `RawHeroForm::spawn_shield_pct`; state.rs `TauntBoard`, `taunt_pass`, `taunt_override`), at level 11.
//!
//! THE MEASUREMENTS (sp-form-Knight-hero-s0; a press issued on t196):
//!   - the hero comes with no shield: a Musketeer's 217 took it from 1766 to 1549 before the press (its row's
//!     OnStartingAction sets the shield to 0 %);
//!   - after the press its hitpoints stood at 1549 through a Musketeer's 217 on t214, a Skeleton's 81 and a 217 on
//!     t234, and a Knight's 202 took them on t246: the press set a shield of 512 (ShieldHitpoints 200 on the ladder at
//!     11), and the hit that breaks it takes nothing past it.
//! Read off the table, not measured (every enemy in the scene was on the hero already): the taunt, an area on the hero
//! for 1100 ms that turns each enemy within 6500 on the hero for 4000 ms.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_knight`):
//!   - shield_start_ignored -> `it_comes_with_no_shield_and_the_press_sets_512` and
//!     `shots_after_the_press_fall_on_the_shield_until_one_breaks_it` red;
//!   - shield_press_ignored -> the same two red;
//!   - taunt_never -> `the_taunt_turns_an_enemy_on_the_hero` red;
//!   - taunt_unread -> the same red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Knight", "Archer", "Giant", "Musketeer", "MiniPekka", "HogRider", "Fireball", "Zap"];
/// On blue's side, out of every red tower's reach.
const AT: (i32, i32) = (9000, 10000);

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

/// A unit held on its point, and whether its hitpoints are topped up every tick.
type Held = (EntityId, (i32, i32), bool);

/// The hero put at AT, and `units` (side, card, point, topped up) put on their points; one tick run.
fn start(units: &[(Team, &str, (i32, i32), bool)]) -> (BattleState, EntityId, Vec<Held>) {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Knight_hero", n(AT.0, AT.1), None).expect("the hero");
    for (team, card, p, _) in units {
        s.spawn_unit_resolved(*team, card, n(p.0, p.1), None).expect("a unit");
    }
    s.tick();
    let hero = find_live(&s, Team::Blue, "Knight_hero")[0].id;
    let held = units
        .iter()
        .map(|(team, card, p, top)| {
            // The placement may move a unit a unit or so off its point (formation.GROUND_DEPLOY_POINT): the nearest.
            let e = s.entities().filter(|e| e.team == *team && e.card == *card).min_by_key(|e| e.pos.dist2(n(p.0, p.1))).expect("the unit where it was put");
            (e.id, *p, *top)
        })
        .collect();
    (s, hero, held)
}

/// One tick with the hero on AT and every unit on its point (the topped-up ones at full hitpoints).
fn step(s: &mut BattleState, hero: EntityId, held: &[Held]) {
    assert!(s.debug_set_pos(hero, n(AT.0, AT.1)));
    for (id, p, top) in held {
        assert!(s.debug_set_pos(*id, n(p.0, p.1)));
        if *top {
            let max = s.entity(*id).expect("a held unit").max_hp;
            assert!(s.debug_set_hp(*id, max));
        }
    }
    s.tick();
}

#[test]
fn it_comes_with_no_shield_and_the_press_sets_512() {
    let (mut s, hero, held) = start(&[]);
    for _ in 0..40 {
        step(&mut s, hero, &held);
        assert_eq!(s.entity(hero).expect("the hero").shield, 0, "no shield before the press");
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..10 {
        step(&mut s, hero, &held);
    }
    assert_eq!(s.entity(hero).expect("the hero").shield, 512, "200 on the ladder at 11, set by the press");
}

#[test]
fn shots_after_the_press_fall_on_the_shield_until_one_breaks_it() {
    // A red Musketeer 4500 ahead of the hero, held and topped up: 217 a shot. One shot on the hitpoints before the
    // press; after it 512 -> 295 -> 78 -> 0 on the shield (the third takes the last 78 and nothing past it), then 217 on
    // the hitpoints again.
    let (mut s, hero, held) = start(&[(Team::Red, "Musketeer", (AT.0, AT.1 + 4500), true)]);
    let hp = |s: &BattleState| {
        let e = s.entity(hero).expect("the hero");
        (e.hp, e.shield)
    };
    let mut k = 0;
    while hp(&s).0 == 1766 {
        step(&mut s, hero, &held);
        k += 1;
        assert!(k < 200, "no shot on the hero");
    }
    assert_eq!(hp(&s), (1549, 0), "the first shot on the hitpoints: no shield before the press");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut seen = vec![hp(&s)];
    for _ in 0..120 {
        step(&mut s, hero, &held);
        if hp(&s) != *seen.last().expect("one") {
            seen.push(hp(&s));
        }
    }
    assert!(seen.len() >= 6, "six readings in 120 ticks: {seen:?}");
    assert_eq!(seen[..6], [(1549, 0), (1549, 512), (1549, 295), (1549, 78), (1549, 0), (1332, 0)], "{seen:?}");
}

#[test]
fn the_taunt_turns_an_enemy_on_the_hero() {
    // A red Knight beside a blue Archer (on it), 4717 from the hero: from the press's trigger on it must target the
    // hero, for 4000 ms. Everyone held, the two topped up.
    let (mut s, hero, held) = start(&[(Team::Blue, "Archer", (5000, 13500), true), (Team::Red, "Knight", (5000, 12500), true)]);
    let (archer, red) = (held[0].0, held[1].0);
    for _ in 0..40 {
        step(&mut s, hero, &held);
    }
    assert_eq!(s.entity(red).expect("the red Knight").target, Some(archer), "on the Archer before the press");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut on = Vec::new();
    for _ in 0..80 {
        step(&mut s, hero, &held);
        on.push(s.entity(red).expect("the red Knight").target == Some(hero));
    }
    let first = on.iter().position(|x| *x).expect("the taunt turns it on the hero");
    assert!(first <= 6, "within the trigger's ticks of the press: {first}");
    assert!(on[first..].iter().all(|x| *x), "held on the hero while the taunt lasts: {on:?}");
}
