//! THE HERO BOWLER'S SIEGE (tools/extract_cards.py `siege_effect`; card.rs `SiegeDef`, `CardDb::siege_of`; state.rs
//! `SiegeRun`, `siege_pass`, the swing's entry pick; combat.rs `fire`, the near entry's start), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-Bowler-hero-s0; the press issued t214): the cast t215..t263; its first
//! swing t267 at progress 2065; its shots at the Knight 2,581 away first seen 2000 out, stepping 400, 384 each (150 at
//! level 11), landing on t276 and t315, 39 ticks apart. On sp-bowler-siege-s0 (the press issued t120): the far shells
//! first seen 150 out, 38 and 39 ticks apart; the siege's end on t270, the trigger (t124: the cast's start t121 and its
//! TriggerDelay 200 less a tick, EARLY_TRIGGER_TICKS) + 146. Read off the table: the reach (11500).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_bowler`): siege_never, siege_never_ends, siege_always_far, early_trigger_late.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Bowler", "Knight", "Archer", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
/// On blue's side, out of every red tower's reach.
const AT: (i32, i32) = (9000, 9000);

/// A point's distance from another, native.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = (((a.x - b.x) / K) as i64, ((a.y - b.y) / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// Blue's princess towers down, red `units` put and held on their points, and the hero form put at AT and held there for
/// 40 ticks: the battle, the hero and the reds.
fn start(units: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<(EntityId, Vec2)>) {
    let mut cfg = config();
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
    s.spawn_unit(Team::Blue, "Bowler_hero", n(AT.0, AT.1), None).expect("the hero");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Bowler_hero")[0].id;
    for _ in 0..40 {
        assert!(s.debug_set_pos(hero, n(AT.0, AT.1)));
        hold(&mut s, &reds);
        s.tick();
    }
    (s, hero, reds)
}

/// The reds back on their points, their hitpoints topped up (the siege's shots would kill a Knight).
fn hold(s: &mut BattleState, reds: &[(EntityId, Vec2)]) {
    for (id, p) in reds {
        if let Some(e) = s.entity(*id) {
            let full = e.max_hp;
            assert!(s.debug_set_pos(*id, *p));
            assert!(s.debug_set_hp(*id, full));
        }
    }
}

/// Press, run `ticks`, and collect each new shot of the hero's: its tick, speed, damage and first point's distance from
/// the hero. Shots end long before the next is made (a swing is 39 ticks), so the list's new tail is the tick's new shots.
fn siege_shots(s: &mut BattleState, hero: EntityId, reds: &[(EntityId, Vec2)], ticks: u32) -> Vec<(u32, i32, i32, i64)> {
    let mine = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(hero)).map(|q| (q.speed, q.damage, q.pos)).collect::<Vec<_>>();
    let mut known = mine(s).len();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut out = Vec::new();
    for _ in 0..ticks {
        hold(s, reds);
        s.tick();
        let now = s.tick_count() - 1;
        let at = s.entity(hero).expect("the hero").pos;
        let m = mine(s);
        for (sp, d, p) in m.iter().skip(known.min(m.len())) {
            out.push((now, *sp, *d, dist(*p, at)));
        }
        known = m.len();
    }
    out
}

#[test]
fn its_siege_shoots_mortar_shells_from_its_near_entry_39_ticks_apart() {
    // A red Knight held 2561 off (the scene's 2,581): the near entry, the shell from 2000 out.
    let (mut s, hero, reds) = start(&[("Knight", (AT.0 + 1600, AT.1 + 2000))]);
    let db = s.cards().clone();
    let siege = db.index("BowlerHero_Siege").expect("the siege form");
    let conv = s.config().calib.projectile_speed_to_subtiles_per_tick;
    let shell = db.scaled(siege, 11, 150).unwrap();
    assert_eq!(shell, 384, "the shell at level 11");
    let shots = siege_shots(&mut s, hero, &reds, 140);
    let shells: Vec<_> = shots.iter().filter(|q| q.1 == 400 * conv).collect();
    assert!(shells.len() >= 2, "two shells: {shots:?}");
    for q in &shells {
        assert_eq!(q.2, shell, "a shell's damage: {shots:?}");
        assert!((q.3 - 2000).abs() <= 2, "a near shell from 2000 out: {shots:?}");
    }
    assert_eq!(shells[1].0 - shells[0].0, 39, "the shells 39 ticks apart (2500 at 65 a tick): {shots:?}");
    // It stands through the siege (140 ticks from the press, inside its 146 from the trigger).
    assert_eq!(dist(s.entity(hero).expect("the hero").pos, n(AT.0, AT.1)), 0, "the hero rooted");
}

#[test]
fn its_far_entry_reaches_a_knight_9000_off_from_150_out() {
    // A red Knight held 9000 ahead: past the boulder's 4000, within the siege's 11500.
    let (mut s, hero, reds) = start(&[("Knight", (AT.0, AT.1 + 9000))]);
    let conv = s.config().calib.projectile_speed_to_subtiles_per_tick;
    let shots = siege_shots(&mut s, hero, &reds, 120);
    let first = shots.iter().find(|q| q.1 == 400 * conv).unwrap_or_else(|| panic!("a shell at the far Knight: {shots:?}"));
    assert!((first.3 - 150).abs() <= 2, "a far shell from 150 out: {shots:?}");
}

#[test]
fn its_siege_form_lasts_its_buffs_7300_ms() {
    // No enemy near: the hero is its siege form from the trigger, P + 4 (the cast's start P + 1, its TriggerDelay 200 less
    // a tick), for 146 ticks, then its own row again.
    let (mut s, hero, _) = start(&[]);
    let p = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows = Vec::new();
    for _ in 0..200 {
        assert!(s.debug_set_pos(hero, n(AT.0, AT.1)));
        s.tick();
        rows.push((s.tick_count() - 1, s.entity(hero).expect("the hero").card.to_string()));
    }
    let on = rows.iter().find(|r| r.1 == "BowlerHero_Siege").expect("the siege form").0;
    assert_eq!(on, p + 4, "the siege's trigger, from the press");
    let off = rows.iter().find(|r| r.0 > on && r.1 == "Bowler_hero").map(|r| r.0);
    assert_eq!(off, Some(on + 146), "its own row again 7300 ms on");
}
