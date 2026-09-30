//! THE HERO MINI PEKKA (card.rs `AbilityEffect::LevelUp`, `QuestDef`; state.rs `level_up`, `QuestRun`, `quest_pass`,
//! LEVEL_SET_EARLY_TICKS), against client 15.535.29 at level 11.
//!
//! THE MEASUREMENTS (sp-form-MiniPekka-hero-s0; a press issued on t187, P here, the cast from P + 1):
//!   - on P + 4 the hero reads level 12, max hitpoints 1525 (1390 before) and 1358 hitpoints (1173 the tick before);
//!   - its next blow took 828 off a Knight (295 at level 1 on the Rare ladder's 281 %; 755 at level 11).
//!
//! THE MEASUREMENTS (Oracle's sp-mph-hp-* and sp-mph-hits-*): six level sets on P + 4, the hitpoints before and after
//! (11 to 12: 1188 -> 1369, 986 -> 1214, 784 -> 1058, 1107 -> 1306, 1172 -> 1357; 11 to 13: 1063 -> 1400 of 1677): 30 %
//! of what it misses healed first, then the hitpoints kept in proportion to the max. Presses on the tick of its 1st to
//! 4th blow on a Golem (114, 146, 178 and 210 ticks after its creation) took it to 12, 12, 13 and 13; its blows took
//! 755, 828 and 911 at 11, 12 and 13.
//! Read off the table, not measured: the quest's clock (a bar of 22 s filling from 1 s after the hero's creation, 8 s
//! more for each hit, the stack up by one each time it fills; the Golem presses bound it and do not pin it) and the
//! gains past +2 (+3 and +5 levels for a stack of 2 and 3).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_mini_pekka`):
//!   - level_up_never -> every test here red;
//!   - level_set_on_trigger -> `the_press_sets_it_one_level_up_and_heals_30_percent_of_what_it_misses_on_p_plus_4` red;
//!   - level_set_heals_after -> `the_heal_comes_before_the_level_as_six_scenes_measure` red;
//!   - quest_hits_unread -> `each_hit_fills_8_seconds_of_its_bar` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

/// Blue's princess towers down (its king wakes; its reach stops short of y 12000): no crown tower reaches the scene.
fn blue_towers_down(s: &mut BattleState) {
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
}

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["MiniPekka", "Knight", "Archer", "Giant", "Musketeer", "HogRider", "Fireball", "Zap"];
const AT: (i32, i32) = (9500, 11500);

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

/// A red unit held on its point: its id, point and hitpoints.
type Red = (EntityId, (i32, i32), i32);

/// The hero put at AT and red `units` at their points, one tick run (the hero created on it). Returns the battle, the
/// hero and the red units.
fn start(units: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<Red>) {
    let mut s = battle();
    blue_towers_down(&mut s);
    s.spawn_unit(Team::Blue, "MiniPekka_hero", n(AT.0, AT.1), None).expect("the hero");
    for (card, p) in units {
        s.spawn_unit_resolved(Team::Red, card, n(p.0, p.1), None).expect("a red unit");
    }
    s.tick();
    let hero = find_live(&s, Team::Blue, "MiniPekka_hero")[0].id;
    let reds = units
        .iter()
        .map(|(card, p)| {
            let e = s.entities().find(|e| e.team == Team::Red && e.card == *card && e.pos == n(p.0, p.1)).expect("the unit where it was put");
            (e.id, *p, e.max_hp)
        })
        .collect();
    (s, hero, reds)
}

/// One tick with the hero held on AT and each red unit on its point, its hitpoints topped up; returns what each red
/// unit lost on the tick.
fn step(s: &mut BattleState, hero: EntityId, reds: &[Red]) -> Vec<i32> {
    assert!(s.debug_set_pos(hero, n(AT.0, AT.1)));
    for (id, p, top) in reds {
        assert!(s.debug_set_pos(*id, n(p.0, p.1)));
        assert!(s.debug_set_hp(*id, *top));
    }
    s.tick();
    reds.iter().map(|(id, _, top)| top - s.entity(*id).expect("a red unit held alive").hp).collect()
}

/// P + d is frame d - 1 (frame k: the k + 1-th tick after the press).
fn at(d: usize) -> usize {
    d - 1
}

/// The levels a press gains, pressed `ticks` ticks after the hero's creation with nothing in its reach.
fn gain_after(ticks: usize) -> i32 {
    let (mut s, hero, reds) = start(&[]);
    for _ in 0..ticks {
        step(&mut s, hero, &reds);
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..6 {
        step(&mut s, hero, &reds);
    }
    s.entity(hero).expect("the hero").level - 11
}

#[test]
fn the_press_sets_it_one_level_up_and_heals_30_percent_of_what_it_misses_on_p_plus_4() {
    // Nothing in its reach; deployed, its quest's stack still 0 (2 s), and 1173 of its 1390 hitpoints left.
    let (mut s, hero, reds) = start(&[]);
    for _ in 0..40 {
        step(&mut s, hero, &reds);
    }
    assert!(s.debug_set_hp(hero, 1173));
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut f = Vec::new();
    for _ in 0..8 {
        step(&mut s, hero, &reds);
        let e = s.entity(hero).expect("the hero");
        f.push((e.level, e.max_hp, e.hp));
    }
    for d in 1..4 {
        assert_eq!(f[at(d)], (11, 1390, 1173), "level 11 on P + {d}");
    }
    assert_eq!(f[at(4)], (12, 1525, 1358), "level 12 on P + 4: 1173 + 30 % of the 217 it misses = 1238, x 1525 / 1390");
    assert_eq!(f[at(8)], (12, 1525, 1358), "one level set");
}

#[test]
fn the_heal_comes_before_the_level_as_six_scenes_measure() {
    // Each measured hitpoint count set on the hero before a press with its quest's stack still 0 (2 s in): the level set
    // on P + 4 leaves what the client read.
    for (before, after) in [(1188, 1369), (986, 1214), (784, 1058), (1107, 1306), (1172, 1357)] {
        let (mut s, hero, reds) = start(&[]);
        for _ in 0..40 {
            step(&mut s, hero, &reds);
        }
        assert!(s.debug_set_hp(hero, before));
        s.press_ability_button(Team::Blue, 0).expect("the press");
        for _ in 0..4 {
            step(&mut s, hero, &reds);
        }
        let e = s.entity(hero).expect("the hero");
        assert_eq!((e.level, e.max_hp, e.hp), (12, 1525, after), "{before} before the level set");
    }
}

#[test]
fn its_blows_after_the_press_take_the_new_levels_damage() {
    // A red Knight in its reach, held there with its hitpoints topped up (the hero's too, so it outlives the scene); the
    // press on the tick of its first blow, as the client's one-blow Golem scene (a stack of 0: one level).
    let (mut s, hero, reds) = start(&[("Knight", (9500, 13100))]);
    let top = s.entity(hero).expect("the hero").max_hp;
    let mut before = Vec::new();
    for _ in 0..100 {
        assert!(s.debug_set_hp(hero, top));
        before.extend(step(&mut s, hero, &reds).into_iter().filter(|d| *d > 300));
        if !before.is_empty() {
            break;
        }
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut after = Vec::new();
    for _ in 0..100 {
        let top = s.entity(hero).expect("the hero").max_hp;
        assert!(s.debug_set_hp(hero, top));
        after.extend(step(&mut s, hero, &reds).into_iter().filter(|d| *d > 300));
    }
    assert!(!before.is_empty() && before.iter().all(|d| *d == 755), "755 a blow at level 11: {before:?}");
    assert!(!after.is_empty() && after.iter().all(|d| *d == 828), "828 a blow at level 12 (the client's): {after:?}");
}

#[test]
fn the_quest_fills_a_stack_in_22_seconds_from_its_first_second() {
    // The table's reading: 1 s, then 22 s of bar. 20 s in, the stack is 0 (one level); 25 s in, 1 (two).
    assert_eq!(gain_after(400), 1, "20 s in");
    assert_eq!(gain_after(500), 2, "25 s in");
}

#[test]
fn each_hit_fills_8_seconds_of_its_bar() {
    // The table's reading: a red Golem in its reach (it walks at buildings, held, so it never strikes back), its
    // hitpoints topped up. Three blows by about 5.5 s: 4.5 s of time and 24 s of hits fill the bar once, where the time
    // alone leaves the stack at 0 (`the_quest_fills_a_stack_in_22_seconds_from_its_first_second`).
    let (mut s, hero, reds) = start(&[("Golem", (9500, 13300))]);
    let mut hits = 0;
    let mut ticks = 0;
    while hits < 3 && ticks < 300 {
        hits += step(&mut s, hero, &reds).iter().filter(|d| **d == 755).count();
        ticks += 1;
    }
    assert_eq!(hits, 3, "three blows on the Golem");
    assert!(ticks < 150, "by 7.5 s: {ticks} ticks");
    for _ in 0..3 {
        step(&mut s, hero, &reds);
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..6 {
        step(&mut s, hero, &reds);
    }
    assert_eq!(s.entity(hero).expect("the hero").level, 13, "a stack of 1: two levels");
}
