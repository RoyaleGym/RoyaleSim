//! THE HERO DARK PRINCE'S DISMOUNT (tools/extract_cards.py `dismount_effect`; card.rs `DismountDef`,
//! DISMOUNT_MOUNT_OFFSET, DISMOUNT_MOUNT_HOLD_FROM_TICKS; state.rs `DismountRun`, `dismount_pass`, `dismount_hops`,
//! EARLY_TRIGGER_TICKS), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, Oracle's sp-hero2-DarkPrince-still-s0, -still-s1 (red), -melee-s0; the press P on
//! the hero's first active tick): the hero 200 back on each of P + 1 .. P + 10, held to P + 40, walking on P + 41, its 1200
//! hitpoints kept; the mount's first frame P + 1 at the hero's point + (-49, -86) (blue) or (+60, +119) (red), standing
//! P + 3 .. P + 23 and walking from P + 24; a Knight 2200 ahead losing 307 on P + 12 and knocked back from the next tick
//! over 10 ticks (249, 224, ... 25 on -melee-s0, a little off the blow's axis); one 4000 off untouched.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_dark_prince`): dismount_never, dismount_never_hops, mount_never_held, mount_blow_never, dismount_collides,
//! press_during_jump,
//! early_trigger_late.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["DarkPrince", "Knight", "Archers", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
const HERO: &str = "DarkPrince_hero";
const WALKER: &str = "DarkPrinceHero_Walking";
const MOUNT: &str = "DarkPrinceHero_Mount";

/// The hero Dark Prince played by `team` at `at` and let stand up there, held on it: the battle and the hero.
fn hero(team: Team, at: Vec2) -> (BattleState, EntityId) {
    let mut cfg = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(team, 10_000);
    s.deploy(team, "DarkPrince", at).expect("the play");
    for _ in 0..120 {
        s.tick();
        let found = find_live(&s, team, HERO).first().map(|e| (e.id, e.deploying));
        if let Some((id, deploying)) = found {
            assert!(s.debug_set_pos(id, at));
            if !deploying {
                return (s, id);
            }
        }
    }
    panic!("no hero stood up");
}

/// One row a tick from the press: (ticks since P, the hero's point, its hp, whether it is its walking row, the mount's
/// point once there).
type Row = (u32, Vec2, i32, bool, Option<Vec2>);

fn press_and_watch(s: &mut BattleState, team: Team, ticks: u32) -> Vec<Row> {
    s.scenario_set_elixir_milli(team, 10_000);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    let h = s.press_ability_button(team, 0).expect("the press, on the hero");
    let mut rows = Vec::new();
    for _ in 0..ticks {
        s.tick();
        let e = s.entity(h).expect("the hero");
        let mount = find_live(s, team, MOUNT).first().map(|m| m.pos);
        rows.push((s.tick_count() - 1 - p, e.pos, e.hp, e.card == WALKER, mount));
    }
    rows
}

fn row(rows: &[Row], k: u32) -> Row {
    *rows.iter().find(|r| r.0 == k).unwrap_or_else(|| panic!("no row {k}"))
}

/// A PRESS WHILE THE HERO LEAPS THE RIVER waits for its landing: measured on client 15.535.29
/// (sp-form-DarkPrince-hero-s0: the press issued mid-leap on t100, the leap's last frame t112, the dismount's first
/// frame, its mount's, t113).
#[test]
fn a_press_while_it_leaps_the_river_waits_for_its_landing() {
    // Off both bridges with the river ahead: it walks up to it and leaps.
    let at = n(10500, 13000);
    let (mut s, h) = hero(Team::Blue, at);
    let mut k = 0;
    while !s.entity(h).expect("the hero").jumping {
        s.tick();
        k += 1;
        assert!(k < 300, "the hero never leapt");
    }
    // The leap's end with no press, on a twin: its first tick off the leap.
    let mut twin = s.clone();
    let mut landed = None;
    for _ in 0..80 {
        twin.tick();
        if !twin.entity(h).expect("the hero").jumping {
            landed = Some(twin.tick_count() - 1);
            break;
        }
    }
    let landed = landed.expect("the leap's end");
    let p = s.tick_count() - 1;
    assert!(landed > p + 2, "a press mid-leap: pressed on {p}, landing on {landed}");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press, mid-leap");
    let mut mount = None;
    for _ in 0..80 {
        s.tick();
        if mount.is_none() && !find_live(&s, Team::Blue, MOUNT).is_empty() {
            mount = Some(s.tick_count() - 1);
        }
    }
    assert_eq!(mount, Some(landed), "the dismount on the leap's first tick off it, {landed} (pressed on {p})");
}

#[test]
fn its_press_hops_it_2000_back_in_ten_ticks_and_holds_it_to_the_press_plus_40() {
    let at = n(14500, 11500);
    let (mut s, h) = hero(Team::Blue, at);
    let hp = s.entity(h).expect("the hero").hp;
    let rows = press_and_watch(&mut s, Team::Blue, 45);
    for k in 1..=10 {
        let r = row(&rows, k);
        assert_eq!((r.1.x / K, r.1.y / K), (14500, 11500 - 200 * k as i32), "the hop on P + {k}: {rows:?}");
        assert!(r.3, "its walking row from the trigger, P + {k}");
    }
    for k in 11..=40 {
        assert_eq!(row(&rows, k).1, row(&rows, 10).1, "held on P + {k}: {rows:?}");
    }
    assert_ne!(row(&rows, 41).1, row(&rows, 40).1, "walking on P + 41: {rows:?}");
    assert!(rows.iter().all(|r| r.2 == hp), "its hitpoints kept: {rows:?}");
}

/// DISABLE_PHYSICAL_INTERACTIONS_WITH_OBJECTS for the dismount's first 1000 ms (its table's; not measured apart): the
/// hero hops through a Knight of its own side standing in its way, and neither pushes the other.
#[test]
fn it_hops_through_a_knight_in_its_way_for_its_first_second() {
    let at = n(14500, 11500);
    let (mut s, h) = hero(Team::Blue, at);
    let k_at = n(14500, 10500);
    let k = s.scenario_spawn_now(Team::Blue, "Knight", k_at, None).expect("a Knight");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows = Vec::new();
    for _ in 0..16 {
        assert!(s.debug_set_pos(k, k_at));
        s.tick();
        let e = s.entity(h).expect("the hero");
        rows.push((s.tick_count() - 1 - p, e.pos.x / K, e.pos.y / K));
    }
    for (kk, x, y) in &rows {
        let want = 11500 - 200 * (*kk).clamp(0, 10) as i32;
        assert_eq!((*x, *y), (14500, want), "the hero on P + {kk}, the Knight met nowhere: {rows:?}");
    }
}

#[test]
fn its_mount_appears_behind_it_on_the_trigger_and_stands_from_its_third_frame_to_the_press_plus_23() {
    let at = n(14500, 11500);
    let (mut s, _) = hero(Team::Blue, at);
    let rows = press_and_watch(&mut s, Team::Blue, 30);
    let first = row(&rows, 1).4.expect("the mount on P + 1");
    assert_eq!((first.x / K, first.y / K), (14500 - 49, 11500 - 86), "the mount's first point: {rows:?}");
    let stand = row(&rows, 3).4.expect("the mount");
    for k in 3..=23 {
        assert_eq!(row(&rows, k).4, Some(stand), "the mount standing on P + {k}: {rows:?}");
    }
    assert_ne!(row(&rows, 24).4, Some(stand), "the mount walking on P + 24: {rows:?}");
}

#[test]
fn a_red_heros_hops_go_up_and_its_mount_appears_at_its_own_offset() {
    let at = n(14500, 20499);
    let (mut s, _) = hero(Team::Red, at);
    let rows = press_and_watch(&mut s, Team::Red, 12);
    for k in 1..=10 {
        let r = row(&rows, k);
        assert_eq!((r.1.x / K, r.1.y / K), (14500, 20499 + 200 * k as i32), "the red hop on P + {k}: {rows:?}");
    }
    let first = row(&rows, 1).4.expect("the mount on P + 1");
    assert_eq!((first.x / K, first.y / K), (14500 + 60, 20499 + 119), "the red mount's first point: {rows:?}");
}

#[test]
fn its_mounts_blow_takes_307_off_a_knight_2200_away_on_the_press_plus_12_and_knocks_it_back_from_the_next_tick() {
    let at = n(14500, 11500);
    let (mut s, _) = hero(Team::Blue, at);
    let (near_at, far_at) = (n(14500, 13700), n(10500, 11500));
    let near = s.scenario_spawn_now(Team::Red, "Knight", near_at, None).expect("a Knight");
    let far = s.scenario_spawn_now(Team::Red, "Knight", far_at, None).expect("a Knight");
    let full = s.entity(near).expect("near").max_hp;
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows: Vec<(u32, i32, i32, i32)> = Vec::new();
    for _ in 0..24 {
        let k = s.tick_count() - p;
        // Both Knights held where they stand until the blow's tick.
        if k <= 11 {
            for (id, pos) in [(near, near_at), (far, far_at)] {
                assert!(s.debug_set_pos(id, pos));
                assert!(s.debug_set_hp(id, full));
            }
        }
        s.tick();
        let (a, b) = (s.entity(near).expect("near"), s.entity(far).expect("far"));
        rows.push((s.tick_count() - 1 - p, a.hp, a.pos.y / K, b.hp));
    }
    let r = |k: u32| *rows.iter().find(|x| x.0 == k).expect("a row");
    assert_eq!(r(11).1, full, "untouched on P + 11: {rows:?}");
    assert_eq!(full - r(12).1, 307, "the blow on P + 12: {rows:?}");
    // On the blow's axis here: 250 down to 25 (the scene's Knight, a little off it, 249, 224, 199, 175, ...).
    let steps: Vec<i32> = (13..=22).map(|k| r(k).2 - r(k - 1).2).collect();
    assert_eq!(steps, vec![250, 225, 200, 175, 150, 125, 100, 75, 50, 25], "the knock-back, P + 13 .. P + 22: {rows:?}");
    assert!(rows.iter().all(|x| x.3 == full), "the Knight 4000 off untouched: {rows:?}");
}
