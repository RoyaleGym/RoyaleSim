//! THE HERO ICE WIZARD (card.rs `IceCubeDef`, `AbilityEffect::IceCube`; state.rs `IceRun`, `ice_finds`, `ice_pass`,
//! `ice_kills`, `ice_reap`, `ice_let_go`; arena.rs `is_valid_position`), on the current client's table, at level 11.
//!
//! THE MEASUREMENTS (client 16.402: 34 live cubes in 16 battles, 2026-10-02 to 10-10; F the find's tick):
//!   - the press waits for the closest enemy carrying a mark (the card's deploy chill or the hero's shot slow), found
//!     from the hero after the tick's move;
//!   - the cube's point is the target's x and its y + (its radius + 1000) toward its own king, or + its radius alone
//!     where that point is on water (34 of 34; 4 fallbacks);
//!   - its victims stand from F + 5, the cube's first frame; the freeze's damage (18 level-scaled, 46 at level 11) on
//!     F + 7;
//!   - the cube deploys 20 ticks, stands, never attacks, and is gone 100 ticks after its creation (7 of 7 left alone),
//!     or 42 ticks after the death of the last unit it froze (1 case);
//!   - its victims walk again 2 ticks after its last frame (40 of 47), slowed 30 % for 40 ticks (7 of 7).
//!
//! Read off the table, not measured: the kill branch (a kill by the hero while the press waits fires at once behind
//! the dead unit) and the death branch (the hero's death while the press waits puts the cube itself behind the
//! closest mark).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_ice_wizard`):
//!   - hero_form_deploy_area_refused, ice_marks_interned, ice_cube_ignore_buffs_unread ->
//!     `the_form_and_its_cube_load_on_the_current_clients_table` red;
//!   - ice_cube_never, ice_bridge_never, ice_ahead_unread, ice_life_unread, ice_freeze_kept, ice_after_slow_never,
//!     ice_cube_walks, ice_cube_attacks -> `a_press_on_a_marked_knight_freezes_it_behind_the_knight` red;
//!   - ice_find_ignores_marks -> `an_unmarked_enemy_is_not_found_and_the_press_waits` red;
//!   - ice_valid_position_unread -> `a_far_point_on_water_falls_back_to_the_radius` red;
//!   - ice_linger_unread -> `the_cube_dies_42_ticks_after_its_last_victim` red;
//!   - ice_kill_unread -> `a_kill_while_the_press_waits_fires_behind_the_dead_unit` red;
//!   - ice_death_unread -> `the_heros_death_while_the_press_waits_puts_the_cube_behind_the_closest_mark` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{AbilityEffect, CardDb, FORM_HERO};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, CardTable};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["IceWizard", "Knight", "Archer", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
const HERO: &str = "IceWizard_hero";
const CUBE: &str = "IceWizardHero_IceCube";
/// On blue's side, out of every red tower's reach.
const AT: (i32, i32) = (9000, 9000);

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// A point's native millitiles.
fn native(p: Vec2) -> (i32, i32) {
    (p.x.div_euclid(K), p.y.div_euclid(K))
}

fn db() -> CardDb {
    CardDb::load_table(CardTable::Client160402017).expect("the 160402017 table")
}

/// A battle on the current client's table: blue's deck holds the hero form, both sides at level 11, blue's princess
/// towers down so no crown tower reaches the scene.
fn battle() -> BattleState {
    let mut cfg = BattleConfig::with_cards(db());
    cfg.calib.card_table = CardTable::Client160402017;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    s
}

/// The hero put down at AT, deployed, held on AT past the match's opening lockout (match.DEPLOY_LOCKOUT_TICKS: no
/// press before it).
fn hero_in(s: &mut BattleState) -> EntityId {
    s.spawn_unit(Team::Blue, HERO, n(AT.0, AT.1), None).expect("the hero");
    s.tick();
    let hero = find_live(s, Team::Blue, HERO)[0].id;
    while s.tick_count() < 95 {
        assert!(s.debug_set_pos(hero, n(AT.0, AT.1)));
        s.tick();
    }
    hero
}

fn cube(s: &BattleState) -> Option<(EntityId, Vec2)> {
    find_live(s, Team::Blue, CUBE).first().map(|e| (e.id, e.pos))
}

/// Tick until `id`'s hitpoints first drop (its first hit), holding it on `at`; the tick of the hit.
fn first_hit(s: &mut BattleState, id: EntityId, at: Vec2, ticks: u32) -> u32 {
    let hp0 = s.entity(id).expect("the unit").hp;
    for _ in 0..ticks {
        assert!(s.debug_set_pos(id, at));
        s.tick();
        if s.entity(id).expect("the unit").hp < hp0 {
            return s.tick_count() - 1;
        }
    }
    panic!("the scene drifted: no hit on the unit in {ticks} ticks");
}

/// THE FORM LOADS on the current client's table: the hero form, its cube (standing, not attacking, curse-immune), its two
/// marks rows of their own (not the base Ice Wizard's chill, whose columns they share), and its freeze damage 18.
/// Plants: hero_form_deploy_area_refused, ice_marks_interned, ice_cube_ignore_buffs_unread, ice_cube_walks,
/// ice_cube_attacks.
#[test]
fn the_form_and_its_cube_load_on_the_current_clients_table() {
    let db = db();
    let base = db.index("IceWizard").expect("the Ice Wizard");
    let form = db.form_card(base, FORM_HERO).unwrap_or_else(|| panic!("the hero form loads: {:?}", db.rejected_forms));
    assert_eq!(db.get(form).name, HERO);
    let d = match db.get(form).ability.as_ref().map(|a| &a.effect) {
        Some(AbilityEffect::IceCube(d)) => *d,
        _ => panic!("its button is the ice cube"),
    };
    let cube = db.get(d.unit);
    assert_eq!(cube.name, CUBE);
    assert_eq!((cube.speed, cube.hit_speed_ms), (0, 0), "the cube stands and never attacks");
    assert_eq!(cube.lifetime_ms, Some(5000));
    assert!(!cube.ignore_buffs.is_empty(), "the cube ignores the curses");
    // the base Ice Wizard's chill (the same -30 columns): not a mark
    let base_chill = match db.get(base).deploy_area_effect.as_ref().map(|s| &s.shape) {
        Some(royalesim::card::SpellShape::AreaEffect { hit }) => hit.buff.map(|b| b.buff),
        _ => None,
    };
    assert!(base_chill.is_some(), "the base card chills");
    assert!(!d.marks.contains(&base_chill.expect("checked")), "the marks are rows of their own: {:?} {:?}", d.marks, base_chill);
    assert_ne!(d.marks[0], d.marks[1]);
    assert_ne!(d.after.buff, d.marks[0]);
    assert_ne!(d.after.buff, d.marks[1]);
    assert_eq!((d.freeze.hit.damage, d.freeze.hit.tower_damage), (18, Some(15)));
    assert_eq!(d.ahead, 1000);
    // the pre-patch table: 35
    let old = CardDb::load_table(CardTable::Client160402017Install).expect("the install table");
    let of = old.form_card(old.index("IceWizard").expect("the Ice Wizard"), FORM_HERO).expect("the form loads there too");
    let o = match old.get(of).ability.as_ref().map(|a| &a.effect) {
        Some(AbilityEffect::IceCube(o)) => *o,
        _ => panic!("its button"),
    };
    assert_eq!(o.freeze.hit.damage, 35);
}

/// THE BUTTON'S TIMELINE on a marked red Knight 4000 ahead of the hero: the press fires and finds in the tick it lands in
/// (F, after the move); the Knight stands from F + 5 (the bridge), the cube's first frame is F + 5 at the Knight's x and its y + 1500
/// (its radius 500 + 1000, toward red's king); the freeze takes 46 on F + 7; the cube never moves nor takes a target and
/// is gone on its creation + 100; the Knight's first step comes on the cube's last frame + 2, slower than its walk
/// later. The hero is killed after the find: the flight comes whatever it does, and no shot of its lands after.
/// Plants: ice_cube_never, ice_bridge_never, ice_ahead_unread, ice_life_unread, ice_freeze_kept, ice_after_slow_never,
/// ice_cube_walks, ice_cube_attacks.
#[test]
fn a_press_on_a_marked_knight_freezes_it_behind_the_knight() {
    let mut s = battle();
    let kat = n(AT.0, AT.1 + 4000);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let hero = hero_in(&mut s);
    first_hit(&mut s, knight, kat, 120);
    assert!(s.debug_set_pos(knight, kat));
    let f = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick(); // F: the press is debited, fires and finds in the tick it lands in
    let at_find = s.entity(knight).expect("the Knight").pos;
    assert!(s.debug_set_hp(hero, 0), "the hero killed after the find");
    let mut pos = Vec::new();
    let mut hp = Vec::new();
    let mut seen: Option<u32> = None;
    let mut cube_at = None;
    let mut cube_pos = Vec::new();
    for _ in 0..170 {
        s.tick();
        let t = s.tick_count() - 1;
        let k = s.entity(knight).expect("the Knight lives");
        pos.push((t, k.pos));
        hp.push((t, k.hp));
        if let Some((c, cp)) = cube(&s) {
            if seen.is_none() {
                seen = Some(t);
                cube_at = Some(cp);
            }
            let e = s.entity(c).expect("the cube");
            assert_eq!(e.target, None, "the cube takes no target");
            cube_pos.push(cp);
        }
    }
    let c = seen.expect("a cube is put down");
    assert_eq!(c, f + 5, "the cube's first frame on F + 5");
    let (kx, ky) = native(at_find);
    assert_eq!(native(cube_at.expect("seen")), (kx, ky + 1500), "behind the Knight, its radius + 1000 toward red's king");
    assert!(cube_pos.iter().all(|p| *p == cube_at.expect("seen")), "the cube never moves");
    let at = |t: u32| pos.iter().find(|(x, _)| *x == t).map(|(_, p)| *p).expect("a frame");
    let hpt = |t: u32| hp.iter().find(|(x, _)| *x == t).map(|(_, h)| *h).expect("a frame");
    assert_ne!(at(c - 1), at(c - 2), "the Knight's last step on F + 4");
    for t in c - 1..c + 99 {
        assert_eq!(at(t + 1), at(t), "the Knight stands from F + 5 to the thaw (tick {t})");
    }
    assert_eq!(hpt(c + 1) - hpt(c + 2), 46, "the freeze takes 46 on F + 7");
    let last = c + 99;
    assert!(cube(&s).is_none(), "the cube is gone");
    assert_eq!(at(last + 1), at(last), "still frozen on the tick the cube is gone");
    assert_ne!(at(last + 2), at(last + 1), "the first step on the cube's last frame + 2");
    let step = |t: u32| at(t).dist2(at(t - 1));
    assert!(step(last + 2) < step(last + 50), "the first steps slowed: {} {}", step(last + 2), step(last + 50));
}

/// AN UNMARKED ENEMY IS NOT FOUND: a red Knight 7800 off (out of the hero's reach, so its shot never marks it): the press
/// waits, and no cube comes. Plant: ice_find_ignores_marks.
#[test]
fn an_unmarked_enemy_is_not_found_and_the_press_waits() {
    let mut s = battle();
    let kat = n(AT.0 - 6000, AT.1 + 5000);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let hero = hero_in(&mut s);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..20 {
        assert!(s.debug_set_pos(knight, kat));
        assert!(s.debug_set_pos(hero, n(AT.0, AT.1)));
        s.tick();
        assert!(cube(&s).is_none(), "no cube for an unmarked enemy");
    }
}

/// A FAR POINT ON WATER FALLS BACK TO THE RADIUS: a marked red Knight on y 14000 (x 9000, between the bridges): its far
/// point y 15500 is in the river, so the cube stands on y 14500. Plant: ice_valid_position_unread.
#[test]
fn a_far_point_on_water_falls_back_to_the_radius() {
    let mut s = battle();
    let kat = n(9000, 14000);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let hero = hero_in(&mut s);
    first_hit(&mut s, knight, kat, 120);
    assert!(s.debug_set_pos(knight, kat));
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick();
    let (kx, ky) = native(s.entity(knight).expect("the Knight").pos);
    assert!(s.debug_set_hp(hero, 0));
    for _ in 0..8 {
        s.tick();
    }
    let (_, cp) = cube(&s).expect("a cube");
    assert_eq!(native(cp), (kx, ky + 500), "the radius alone: the far point is on water");
}

/// THE LINGER: the frozen Knight killed on K (after the freeze landed): the cube's last frame is K + 41, gone on K + 42.
/// Plant: ice_linger_unread.
#[test]
fn the_cube_dies_42_ticks_after_its_last_victim() {
    let mut s = battle();
    let kat = n(AT.0, AT.1 + 4000);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let hero = hero_in(&mut s);
    first_hit(&mut s, knight, kat, 120);
    assert!(s.debug_set_pos(knight, kat));
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick();
    s.tick();
    assert!(s.debug_set_hp(hero, 0));
    for _ in 0..10 {
        s.tick();
    }
    assert!(cube(&s).is_some(), "the cube stands, its freeze landed");
    assert!(s.debug_set_hp(knight, 0), "the Knight killed");
    let k = s.tick_count();
    let mut gone = None;
    for _ in 0..60 {
        s.tick();
        if gone.is_none() && cube(&s).is_none() {
            gone = Some(s.tick_count() - 1);
        }
    }
    assert_eq!(gone, Some(k + 42), "gone on the victim's death tick + 42");
}

/// THE KILL BRANCH (read off the table): no mark at the press; the hero's shot kills a red Skeleton (81, a shot is 89)
/// on K; the cube's first frame is K + 3, behind the dead Skeleton (its radius + 1000). Plant: ice_kill_unread.
#[test]
fn a_kill_while_the_press_waits_fires_behind_the_dead_unit() {
    let mut s = battle();
    hero_in(&mut s);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..3 {
        s.tick();
    }
    assert!(cube(&s).is_none());
    let sat = n(AT.0, AT.1 + 4000);
    let skel = s.scenario_spawn_now(Team::Red, "Skeleton", sat, None).expect("the Skeleton");
    let mut last = None;
    let mut died = None;
    for _ in 0..120 {
        if s.entity(skel).is_some() {
            assert!(s.debug_set_pos(skel, sat));
        }
        s.tick();
        match s.entity(skel) {
            Some(e) => last = Some((e.pos, e.radius)),
            None if died.is_none() => died = Some(s.tick_count() - 1),
            None => {}
        }
        if died.is_some() {
            break;
        }
    }
    let k = died.expect("the hero's shot kills the Skeleton");
    let (sp, sr) = last.expect("seen");
    let mut seen = None;
    for _ in 0..8 {
        s.tick();
        if seen.is_none() {
            seen = cube(&s).map(|(_, p)| (s.tick_count() - 1, p));
        }
    }
    let (c, cp) = seen.expect("a kill fires the cube");
    assert_eq!(c, k + 3, "the cube's first frame on the kill's tick + 3");
    let (x, y) = native(sp);
    assert_eq!(native(cp), (x, y + sr / K + 1000), "behind the dead Skeleton");
}

/// THE DEATH BRANCH (read off the table): no mark at the press; a red Knight is marked by the hero's shot on X and the
/// hero dies on X, before the next tick's find: the cube itself (no bridge) behind the Knight, its first frame X.
/// Plant: ice_death_unread.
#[test]
fn the_heros_death_while_the_press_waits_puts_the_cube_behind_the_closest_mark() {
    let mut s = battle();
    let hero = hero_in(&mut s);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..3 {
        s.tick();
    }
    let kat = n(AT.0, AT.1 + 4000);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    // a dry run finds the tick its first hit lands
    let mut dry = s.clone();
    let x = first_hit(&mut dry, knight, kat, 120);
    while s.tick_count() < x {
        assert!(s.debug_set_pos(knight, kat));
        s.tick();
    }
    assert!(s.debug_set_pos(knight, kat));
    assert!(s.debug_set_hp(hero, 0), "the hero dies on X, its shot landing");
    s.tick();
    assert_eq!(s.tick_count() - 1, x);
    assert!(s.entity(hero).is_none(), "the hero is gone");
    let (_, cp) = cube(&s).expect("the death puts the cube down at once");
    let (kx, ky) = native(s.entity(knight).expect("the Knight").pos);
    assert_eq!(native(cp), (kx, ky + 1500));
}
