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
//!   - ice_death_unread -> `the_heros_death_while_the_press_waits_puts_the_cube_behind_the_closest_mark` red;
//!   - ice_find_reads_chill -> `under_client16635_shot_slow_only_a_press_inside_the_deploy_chill_waits_for_the_shot` red;
//!   - ice_release_attack_now -> `under_client16635_attack_next_tick_a_let_go_knight_strikes_a_tick_later` red;
//!   - ice_release_target_now -> `under_client16635_target_next_tick_a_let_go_knight_retakes_its_target_a_tick_later` red;
//!   - cube_loss_frees -> `under_client16635_cube_waits_a_musketeer_that_doomed_a_cube_serves_the_wait` red;
//!   - ice_cube_unpulled -> `under_client16635_row_speed_a_tornado_pulls_the_cube_144_a_tick` and
//!     `under_client16635_row_speed_a_cube_born_in_a_tornado_is_first_pulled_on_its_first_frame_plus_2` red;
//!   - ice_cube_pulled_at_birth -> `under_client16635_row_speed_a_cube_born_in_a_tornado_is_first_pulled_on_its_first_frame_plus_2` red;
//!   - ice_release_replans -> `under_client16635_route_kept_a_let_go_knight_walks_its_held_route` red;
//!   - ice_hold_masked ->
//!     `under_client16635_cube_scanned_a_cube_victim_facing_the_cube_holds_190_and_turns_its_let_go_step` red;
//!   - ice_press_on_let_go ->
//!     `under_client16635_press_next_tick_a_press_waiting_through_the_hold_starts_a_tick_after_the_let_go` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{AbilityEffect, CardDb, FORM_HERO};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, CardTable, CubeLossWait, HeldUnitAvoidance, IceCubeAttract, IceCubeHeroDeath, IceFindMarks, IceFindOwnMark, IceKillCubeTick, IceReleaseAttack, IceReleasePress, IceReleaseTarget, IceReleaseWalk};
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
    battle_with(|_| {})
}

/// `battle()` with `f` applied to its config first.
fn battle_with(f: impl FnOnce(&mut BattleConfig)) -> BattleState {
    let mut cfg = BattleConfig::with_cards(db());
    cfg.calib.card_table = CardTable::Client160402017;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    f(&mut cfg);
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
    assert_eq!(d.pull_speed, 40, "the cube's row Speed is kept for a pull");
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
    // spawner.ICE_CUBE_HERO_DEATH = cube_comes, the arm this scene measures: it kills the hero on the find's tick + 1
    // so no shot of its lands, and under the shipped client16635_death_window a hero dead on F or F + 1 puts the cube
    // down on its death tick (under_client16635_death_window_*).
    let mut s = battle_with(|c| c.calib.ice_cube_hero_death = IceCubeHeroDeath::CubeComes);
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
    // put down after the hero settles and held from its first tick, so no shot of the hero ever reached it
    let hero = hero_in(&mut s);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
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
    // spawner.ICE_CUBE_HERO_DEATH = cube_comes, the arm this scene measures: it kills the hero on the find's tick + 1
    // so no shot of its lands, and under the shipped client16635_death_window a hero dead on F or F + 1 puts the cube
    // down on its death tick (under_client16635_death_window_*).
    let mut s = battle_with(|c| c.calib.ice_cube_hero_death = IceCubeHeroDeath::CubeComes);
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
    // spawner.ICE_CUBE_HERO_DEATH = cube_comes, the arm this scene measures: it kills the hero on the find's tick + 1
    // so no shot of its lands, and under the shipped client16635_death_window a hero dead on F or F + 1 puts the cube
    // down on its death tick (under_client16635_death_window_*).
    let mut s = battle_with(|c| c.calib.ice_cube_hero_death = IceCubeHeroDeath::CubeComes);
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
    // spawner.ICE_KILL_CUBE_TICK = kill_plus_3, the arm this scene measures (the shipped client16635_kill_plus_2 puts
    // the cube a tick sooner: under_client16635_kill_plus_2_*).
    let mut s = battle_with(|c| c.calib.ice_kill_cube_tick = IceKillCubeTick::KillPlus3);
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

/// A red Knight held 2500 ahead of AT from the start (inside the hero's deploy chill, short of its reach to strike back), the
/// hero put down on AT on tick 70 and held there, the press on tick 95 (the opening lockout's end: 25 ticks after the deploy,
/// before the hero's first shot lands), under status.ICE_FIND_MARKS = `arm`: (the press's tick, the cube's first frame).
fn chilled_press(arm: IceFindMarks) -> (u32, u32) {
    let mut s = battle_with(|c| c.calib.ice_find_marks = arm);
    let kat = n(AT.0, AT.1 + 2500);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    while s.tick_count() < 70 {
        assert!(s.debug_set_pos(knight, kat));
        s.tick();
    }
    s.spawn_unit(Team::Blue, HERO, n(AT.0, AT.1), None).expect("the hero");
    s.tick();
    let hero = find_live(&s, Team::Blue, HERO)[0].id;
    while s.tick_count() < 95 {
        assert!(s.debug_set_pos(hero, n(AT.0, AT.1)) && s.debug_set_pos(knight, kat));
        s.tick();
    }
    let press = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..80 {
        assert!(s.debug_set_pos(knight, kat));
        if s.entity(hero).is_some() {
            assert!(s.debug_set_pos(hero, n(AT.0, AT.1)));
        }
        s.tick();
        if cube(&s).is_some() {
            return (press, s.tick_count() - 1);
        }
    }
    panic!("the scene drifted: no cube within 80 ticks of the press");
}

/// status.ICE_FIND_MARKS = client16635_shot_slow_only (client 16.635, parity r95, the oracle's il-mg-16635 set: 11 of 11 presses 30
/// ticks after the deploy with an enemy inside the deploy chill found on the hero's first shot's landing + 1): the press finds
/// the chilled Knight later than under either_mark (the vacuity check: found at once, the cube on the press + 5). Plant:
/// ice_find_reads_chill.
#[test]
fn under_client16635_shot_slow_only_a_press_inside_the_deploy_chill_waits_for_the_shot() {
    let (p_old, c_old) = chilled_press(IceFindMarks::EitherMark);
    assert_eq!(c_old, p_old + 5, "either_mark (the vacuity check): the chilled Knight is found at once");
    let (p_new, c_new) = chilled_press(IceFindMarks::Client16635ShotSlowOnly);
    assert_eq!(p_new, p_old, "the scene: one press tick");
    assert!(c_new > c_old, "client16635_shot_slow_only: the cube came on {c_new}, not after either_mark's {c_old}");
}

/// A red Knight held on AT + (0, 4000) striking a blue Giant held 1000 behind it (its hitpoints kept), marked by the hero's
/// shots, frozen by the press's cube (the hero killed after the find), under status.ICE_RELEASE_ATTACK = `arm` and
/// status.ICE_RELEASE_TARGET = with_the_walk (the scene isolates the attack arm; the shipped client16635_target_next_tick moves
/// the retake too): (the cube's last frame, the Knight's first strike on the Giant after it).
fn released_strike(arm: IceReleaseAttack) -> (u32, u32) {
    let mut s = battle_with(|c| {
        c.calib.ice_release_attack = arm;
        c.calib.ice_release_target = IceReleaseTarget::WithTheWalk;
    });
    let (kat, gat) = (n(AT.0, AT.1 + 4000), n(AT.0, AT.1 + 3000));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let giant = s.scenario_spawn_now(Team::Blue, "Giant", gat, None).expect("the Giant");
    let hero = hero_in(&mut s);
    let gmax = s.entity(giant).expect("the Giant").max_hp;
    first_hit(&mut s, knight, kat, 120);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick();
    assert!(s.debug_set_hp(hero, 0), "the hero killed after the find");
    let (mut seen, mut last) = (false, None);
    for _ in 0..200 {
        assert!(s.debug_set_pos(knight, kat) && s.debug_set_pos(giant, gat) && s.debug_set_hp(giant, gmax));
        s.tick();
        let t = s.tick_count() - 1;
        let here = cube(&s).is_some();
        if here {
            seen = true;
        } else if seen && last.is_none() {
            last = Some(t - 1);
        }
        if let Some(l) = last {
            if s.entity(giant).expect("the Giant").hp < gmax {
                return (l, t);
            }
        }
    }
    panic!("the scene drifted: no cube came and went, or no strike after it (seen {seen}, last frame {last:?})");
}

/// status.ICE_RELEASE_ATTACK = client16635_attack_next_tick (client 16.635, parity r95, the oracle's il-mg-16635 set: 37 let-go
/// victims' attack clocks resumed on the cube's last frame + 3, the engine's on + 2): the Knight frozen mid-attack strikes the
/// Giant one tick later after the cube than under with_the_walk (the vacuity check). Plant: ice_release_attack_now.
#[test]
fn under_client16635_attack_next_tick_a_let_go_knight_strikes_a_tick_later() {
    let (l_old, h_old) = released_strike(IceReleaseAttack::WithTheWalk);
    let (l_new, h_new) = released_strike(IceReleaseAttack::Client16635AttackNextTick);
    assert_eq!(l_new, l_old, "the scene: one cube life under both arms");
    assert_eq!(h_new, h_old + 1, "client16635_attack_next_tick: the first strike after the cube's last frame {l_new} on {h_new}, not with_the_walk's {h_old} + 1");
}

/// `released_strike`'s scene under status.ICE_RELEASE_ATTACK = client16635_attack_next_tick and status.ICE_RELEASE_TARGET =
/// `arm`: the Knight's target on the cube's last frame + 2 and + 3, and the Giant.
fn released_target(arm: IceReleaseTarget) -> (Option<EntityId>, Option<EntityId>, EntityId) {
    let mut s = battle_with(|c| {
        c.calib.ice_release_attack = IceReleaseAttack::Client16635AttackNextTick;
        c.calib.ice_release_target = arm;
    });
    let (kat, gat) = (n(AT.0, AT.1 + 4000), n(AT.0, AT.1 + 3000));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let giant = s.scenario_spawn_now(Team::Blue, "Giant", gat, None).expect("the Giant");
    let hero = hero_in(&mut s);
    let gmax = s.entity(giant).expect("the Giant").max_hp;
    first_hit(&mut s, knight, kat, 120);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick();
    assert!(s.debug_set_hp(hero, 0), "the hero killed after the find");
    let (mut seen, mut last, mut at2) = (false, None, None);
    for _ in 0..200 {
        assert!(s.debug_set_pos(knight, kat) && s.debug_set_pos(giant, gat) && s.debug_set_hp(giant, gmax));
        s.tick();
        let t = s.tick_count() - 1;
        let here = cube(&s).is_some();
        if here {
            seen = true;
        } else if seen && last.is_none() {
            last = Some(t - 1);
        }
        if let Some(l) = last {
            let tg = s.entity(knight).expect("the Knight").target;
            if t == l + 2 {
                at2 = Some(tg);
            } else if t == l + 3 {
                return (at2.expect("the last frame + 2 was read"), tg, giant);
            }
        }
    }
    panic!("the scene drifted: no cube came and went (seen {seen}, last frame {last:?})");
}

/// status.ICE_RELEASE_TARGET = client16635_target_next_tick (client 16.635, parity r96, the oracle's il-mg-16635 set: 101 of 107
/// units a cube let go took their next target one tick after the engine's): the Knight frozen mid-attack has no target on the
/// cube's last frame + 2 and takes the Giant on + 3; under with_the_walk (the vacuity check) it has taken the Giant on + 2.
/// Plant: ice_release_target_now.
#[test]
fn under_client16635_target_next_tick_a_let_go_knight_retakes_its_target_a_tick_later() {
    let (old2, _, giant) = released_target(IceReleaseTarget::WithTheWalk);
    assert_eq!(old2, Some(giant), "with_the_walk (the vacuity check): the Knight's target on the last frame + 2");
    let (new2, new3, giant) = released_target(IceReleaseTarget::Client16635TargetNextTick);
    assert_eq!(new2, None, "client16635_target_next_tick: the Knight decided on the cube's last frame + 2");
    assert_eq!(new3, Some(giant), "client16635_target_next_tick: the Knight's target on the last frame + 3");
}

/// A red Knight held on AT + (0, 4000), frozen by the press's cube (the hero killed after the find), under
/// combat.CUBE_LOSS_WAIT = `arm`: once the cube has stood its 20-tick deploy, a red Musketeer held 3,000 beyond it comes and
/// takes it; once it holds the cube, a blue Golem held 2,500 on the cube's other side comes as its next target (a Golem
/// there from the start is the Musketeer's first target, the deploying cube no one's). When the Musketeer's shot is in
/// flight at the cube, the cube is set to 1 hitpoint (the shot dooms it): (the cube's loss tick L, the first tick after L
/// the Musketeer holds the Golem).
fn cube_loss(arm: CubeLossWait) -> (u32, u32) {
    let mut s = battle_with(|c| c.calib.cube_loss_wait = arm);
    let kat = n(AT.0, AT.1 + 4000);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let hero = hero_in(&mut s);
    first_hit(&mut s, knight, kat, 120);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick();
    assert!(s.debug_set_hp(hero, 0), "the hero killed after the find");
    let mut cube_at = None;
    for _ in 0..40 {
        assert!(s.debug_set_pos(knight, kat));
        s.tick();
        if let Some(c) = cube(&s) {
            cube_at = Some(c);
            break;
        }
    }
    let (cid, cpos) = cube_at.expect("the scene drifted: no cube");
    for _ in 0..21 {
        assert!(s.debug_set_pos(knight, kat));
        s.tick();
    }
    assert!(s.entity(cid).is_some(), "the scene drifted: the cube went inside its deploy");
    let (mat, gat) = (Vec2::new(cpos.x, cpos.y + 3000 * K), Vec2::new(cpos.x, cpos.y - 2500 * K));
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", mat, None).expect("the Musketeer");
    let mut golem = None;
    let (mut doomed, mut lost) = (false, None);
    for _ in 0..300 {
        assert!(s.debug_set_pos(musk, mat));
        if let Some(g) = golem {
            assert!(s.debug_set_pos(g, gat));
        }
        if s.entity(knight).is_some() {
            assert!(s.debug_set_pos(knight, kat));
        }
        if !doomed && s.entity(cid).is_some() && s.projectiles().iter().any(|p| p.target == cid) {
            assert!(s.debug_set_hp(cid, 1));
            doomed = true;
        }
        s.tick();
        let t = s.tick_count() - 1;
        if golem.is_none() && s.entity(cid).is_some() && s.entity(musk).and_then(|e| e.target) == Some(cid) {
            golem = Some(s.scenario_spawn_now(Team::Blue, "Golem", gat, None).expect("the Golem"));
        }
        if lost.is_none() && s.entity(cid).is_none() {
            assert!(doomed, "the scene drifted: the cube went before the Musketeer's shot doomed it");
            assert!(golem.is_some(), "the scene drifted: the cube went before the Musketeer took it");
            lost = Some(t);
        }
        if let (Some(l), Some(g)) = (lost, golem) {
            if s.entity(musk).expect("the Musketeer").target == Some(g) {
                return (l, t);
            }
        }
    }
    panic!("the scene drifted: no doomed cube lost, or the Musketeer never took the Golem (doomed {doomed}, lost {lost:?}, golem {golem:?})");
}

/// combat.CUBE_LOSS_WAIT = client16635_cube_waits (client 16.635, parity r96, the oracle's il-mg-16635 set: 153 of 153
/// projectile attackers of a doomed cube served the wait): the Musketeer whose shot doomed the cube takes the Golem on the
/// loss + 6; under doomed_frees (the vacuity check) it is freed and takes it on the loss + 1. Plant: cube_loss_frees.
#[test]
fn under_client16635_cube_waits_a_musketeer_that_doomed_a_cube_serves_the_wait() {
    let (l_old, t_old) = cube_loss(CubeLossWait::DoomedFrees);
    assert_eq!(t_old, l_old + 1, "doomed_frees (the vacuity check): the Musketeer took the Golem on {t_old}, the cube lost on {l_old}");
    let (l_new, t_new) = cube_loss(CubeLossWait::Client16635CubeWaits);
    assert_eq!(t_new, l_new + 6, "client16635_cube_waits: the Musketeer took the Golem on {t_new}, the cube lost on {l_new}");
}

/// Since parity r96 for clients 16.402 and 16.635 (either_mark before it; client 15.535.29 keeps either_mark).
#[test]
fn the_shipped_arm_is_client16635_shot_slow_only() {
    assert_eq!(
        royalesim::state::Calib::shipped().ice_find_marks,
        royalesim::state::IceFindMarks::Client16635ShotSlowOnly
    );
    assert_eq!(config().calib.ice_find_marks, royalesim::state::IceFindMarks::Client16635ShotSlowOnly);
}

/// Since parity r96 for clients 16.402 and 16.635 (with_the_walk before it; client 15.535.29 keeps with_the_walk).
#[test]
fn the_shipped_arm_is_client16635_attack_next_tick() {
    assert_eq!(
        royalesim::state::Calib::shipped().ice_release_attack,
        royalesim::state::IceReleaseAttack::Client16635AttackNextTick
    );
    assert_eq!(config().calib.ice_release_attack, royalesim::state::IceReleaseAttack::Client16635AttackNextTick);
}

/// Since parity r96 for clients 16.402 and 16.635 (with_the_walk before it; client 15.535.29 keeps with_the_walk).
#[test]
fn the_shipped_arm_is_client16635_target_next_tick() {
    assert_eq!(
        royalesim::state::Calib::shipped().ice_release_target,
        royalesim::state::IceReleaseTarget::Client16635TargetNextTick
    );
    assert_eq!(config().calib.ice_release_target, royalesim::state::IceReleaseTarget::Client16635TargetNextTick);
}

/// Since parity r96 for clients 16.402 and 16.635 (doomed_frees before it; client 15.535.29 keeps doomed_frees).
#[test]
fn the_shipped_arm_is_client16635_cube_waits() {
    assert_eq!(royalesim::state::Calib::shipped().cube_loss_wait, royalesim::state::CubeLossWait::Client16635CubeWaits);
    assert_eq!(config().calib.cube_loss_wait, royalesim::state::CubeLossWait::Client16635CubeWaits);
}


/// status.ICE_FIND_OWN_MARK = `arm`: a red Knight held at AT + (0, 4000) is slowed by hero A's first shot; A dies; a second hero
/// B, held at (3000, 4000) out of its reach, presses inside A's slow. The press tick and the first cube's tick within 20 ticks.
fn own_mark_cube(arm: IceFindOwnMark) -> (u32, Option<u32>) {
    let mut s = battle_with(|c| c.calib.ice_find_own_mark = arm);
    let kat = n(AT.0, AT.1 + 4000);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let a = hero_in(&mut s);
    first_hit(&mut s, knight, kat, 120);
    assert!(s.debug_set_hp(a, 0), "hero A killed after its mark");
    assert!(s.debug_set_pos(knight, kat));
    s.tick();
    let bat = n(3000, 4000);
    s.spawn_unit(Team::Blue, HERO, bat, None).expect("hero B");
    s.tick();
    let b = find_live(&s, Team::Blue, HERO).first().map(|e| e.id).expect("hero B stands");
    for _ in 0..25 {
        assert!(s.debug_set_pos(b, bat) && s.debug_set_pos(knight, kat));
        s.tick();
    }
    let p = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("B's press");
    let mut seen = None;
    for _ in 0..20 {
        assert!(s.debug_set_pos(b, bat) && s.debug_set_pos(knight, kat));
        s.tick();
        if seen.is_none() && cube(&s).is_some() {
            seen = Some(s.tick_count() - 1);
        }
    }
    (p, seen)
}

/// status.ICE_FIND_OWN_MARK = client16635_own_shot (client 16.635, parity r97, the oracle's il-mg-16635 set: 5 first
/// divergences, 0 against): hero B's press finds nothing on a Knight only dead hero A's shot slowed; under any_hero (the vacuity
/// check) it fires on it. Plant: ice_find_any_hero.
#[test]
fn under_client16635_own_shot_a_mark_by_the_dead_first_hero_is_not_found() {
    let (_, old) = own_mark_cube(IceFindOwnMark::AnyHero);
    assert!(old.is_some(), "any_hero (the vacuity check): no cube on A's mark");
    let (_, new) = own_mark_cube(IceFindOwnMark::Client16635OwnShot);
    assert_eq!(new, None, "client16635_own_shot: B's press fired on A's mark");
}

/// `a_kill_while_the_press_waits_fires_behind_the_dead_unit`'s scene under spawner.ICE_KILL_CUBE_TICK = `arm`: (the kill's tick,
/// the cube's first frame, the cube's point, the dead Skeleton's last point and radius).
fn kill_cube(arm: IceKillCubeTick) -> (u32, u32, Vec2, Vec2, i32) {
    let mut s = battle_with(|c| c.calib.ice_kill_cube_tick = arm);
    hero_in(&mut s);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..3 {
        s.tick();
    }
    assert!(cube(&s).is_none());
    let sat = n(AT.0, AT.1 + 4000);
    let skel = s.scenario_spawn_now(Team::Red, "Skeleton", sat, None).expect("the Skeleton");
    let (mut last, mut died) = (None, None);
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
    (k, c, cp, sp, sr)
}

/// spawner.ICE_KILL_CUBE_TICK = client16635_kill_plus_2 (client 16.635, parity r97, the oracle's il-mg-16635 set: 5 of 5 in
/// sync): a kill-fired cube's first frame on the kill's tick + 2, behind the dead Skeleton; under kill_plus_3 (the vacuity
/// check) on + 3. Plant: ice_kill_cube_late.
#[test]
fn under_client16635_kill_plus_2_a_kill_fired_cube_comes_a_tick_sooner() {
    let (k, c, _, _, _) = kill_cube(IceKillCubeTick::KillPlus3);
    assert_eq!(c, k + 3, "kill_plus_3 (the vacuity check): the cube's first frame");
    let (k, c, cp, sp, sr) = kill_cube(IceKillCubeTick::Client16635KillPlus2);
    assert_eq!(c, k + 2, "client16635_kill_plus_2: the cube's first frame");
    let (x, y) = native(sp);
    assert_eq!(native(cp), (x, y + sr / K + 1000), "client16635_kill_plus_2: behind the dead Skeleton");
}

/// spawner.ICE_CUBE_HERO_DEATH = `arm`: a red Knight at AT + (0, 4000) marked by the hero's first shot, the press found on F
/// (the Knight free to walk from then), the hero set to 0 hitpoints after F + `die_after` (dying on F + die_after + 1). (F, every
/// cube frame (tick, point) through F + 20, the Knight's point after each tick from F.)
#[allow(clippy::type_complexity)]
fn death_window(arm: IceCubeHeroDeath, die_after: u32) -> (u32, Vec<(u32, (i32, i32))>, Vec<(u32, (i32, i32))>) {
    let mut s = battle_with(|c| c.calib.ice_cube_hero_death = arm);
    let kat = n(AT.0, AT.1 + 4000);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let hero = hero_in(&mut s);
    first_hit(&mut s, knight, kat, 120);
    assert!(s.debug_set_pos(knight, kat));
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick();
    let f = s.tick_count() - 1;
    let mut kpos = vec![(f, native(s.entity(knight).expect("the Knight").pos))];
    let mut cubes = Vec::new();
    for k in 1..=20 {
        if k == die_after + 1 {
            assert!(s.debug_set_hp(hero, 0), "the hero killed");
        }
        s.tick();
        let t = s.tick_count() - 1;
        kpos.push((t, native(s.entity(knight).expect("the Knight lives").pos)));
        if let Some((_, cp)) = cube(&s) {
            cubes.push((t, native(cp)));
        }
    }
    (f, cubes, kpos)
}

/// spawner.ICE_CUBE_HERO_DEATH = client16635_death_window (client 16.635, parity r97, the oracle's il-mg-16635 set: 5 of 5): a
/// hero dying on its find + 1 puts the cube down on that tick, behind the Knight's point of that tick; under cube_comes (the
/// vacuity check) the cube comes on F + 5 behind its find's point. Plant: ice_death_fired_unread.
#[test]
fn under_client16635_death_window_a_hero_dead_on_the_find_plus_1_puts_the_cube_down_on_its_death_tick() {
    let (f, cubes, kpos) = death_window(IceCubeHeroDeath::CubeComes, 0);
    assert!(!cubes.iter().any(|(t, _)| *t == f + 1), "cube_comes (the vacuity check): a cube on F + 1");
    let (kx, ky) = kpos[0].1;
    assert_eq!(cubes.first().copied(), Some((f + 5, (kx, ky + 1500))), "cube_comes (the vacuity check): the cube on F + 5");
    let (f, cubes, kpos) = death_window(IceCubeHeroDeath::Client16635DeathWindow, 0);
    let (kx, ky) = kpos[1].1;
    assert_eq!(cubes.first().copied(), Some((f + 1, (kx, ky + 1500))), "client16635_death_window: the cube on the death tick F + 1");
    assert!(cubes.iter().all(|(_, p)| *p == (kx, ky + 1500)), "client16635_death_window: a second cube {cubes:?}");
}

/// spawner.ICE_CUBE_HERO_DEATH = client16635_death_window (client 16.635, parity r97, the oracle's il-mg-16635 set: 2 of 2): a hero
/// dying on its find + 4 puts no cube down; under cube_comes (the vacuity check) the cube comes on F + 5. Plant:
/// ice_death_fired_unread.
#[test]
fn under_client16635_death_window_a_hero_dead_on_the_find_plus_4_puts_no_cube_down() {
    let (f, cubes, _) = death_window(IceCubeHeroDeath::CubeComes, 3);
    assert_eq!(cubes.first().map(|(t, _)| *t), Some(f + 5), "cube_comes (the vacuity check): the cube on F + 5");
    let (_, cubes, _) = death_window(IceCubeHeroDeath::Client16635DeathWindow, 3);
    assert!(cubes.is_empty(), "client16635_death_window: a cube after a death on F + 4 {cubes:?}");
}

/// Since parity r97 for clients 16.402 and 16.635 (any_hero before it; client 15.535.29 keeps any_hero).
#[test]
fn the_shipped_arm_is_client16635_own_shot() {
    assert_eq!(
        royalesim::state::Calib::shipped().ice_find_own_mark,
        royalesim::state::IceFindOwnMark::Client16635OwnShot
    );
    assert_eq!(config().calib.ice_find_own_mark, royalesim::state::IceFindOwnMark::Client16635OwnShot);
}

/// Since parity r97 for clients 16.402 and 16.635 (kill_plus_3 before it; client 15.535.29 keeps kill_plus_3).
#[test]
fn the_shipped_arm_is_client16635_kill_plus_2() {
    assert_eq!(
        royalesim::state::Calib::shipped().ice_kill_cube_tick,
        royalesim::state::IceKillCubeTick::Client16635KillPlus2
    );
    assert_eq!(config().calib.ice_kill_cube_tick, royalesim::state::IceKillCubeTick::Client16635KillPlus2);
}

/// Since parity r97 for clients 16.402 and 16.635 (cube_comes before it; client 15.535.29 keeps cube_comes).
#[test]
fn the_shipped_arm_is_client16635_death_window() {
    assert_eq!(
        royalesim::state::Calib::shipped().ice_cube_hero_death,
        royalesim::state::IceCubeHeroDeath::Client16635DeathWindow
    );
    assert_eq!(config().calib.ice_cube_hero_death, royalesim::state::IceCubeHeroDeath::Client16635DeathWindow);
}

/// `pulled_cube`'s result: the cube's first frame, the cast tick and each tick's cube step.
type PulledCube = (u32, u32, Vec<(u32, (i32, i32))>);

/// The marked-Knight scene (a_press_on_a_marked_knight_freezes_it_behind_the_knight's) under status.ICE_CUBE_ATTRACT = `arm`,
/// with a red Tornado cast on (9500, 10500), 4,031 from the cube's point, on the press's tick + `cast`: (the cube's first
/// frame c, the cast tick D, each tick's cube step from the tick before, native).
fn pulled_cube(arm: IceCubeAttract, cast: u32) -> PulledCube {
    let mut s = battle_with(|c| {
        c.calib.ice_cube_hero_death = IceCubeHeroDeath::CubeComes;
        c.calib.ice_cube_attract = arm;
        c.decks[1] = ["IceWizard", "Knight", "Archer", "Giant", "Musketeer", "Minions", "Fireball", "Tornado"].iter().map(|n| n.to_string()).collect();
    });
    let kat = n(AT.0, AT.1 + 4000);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let hero = hero_in(&mut s);
    first_hit(&mut s, knight, kat, 120);
    assert!(s.debug_set_pos(knight, kat));
    let f = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick();
    assert!(s.debug_set_hp(hero, 0), "the hero killed after the find");
    let (mut c, mut d, mut last, mut steps) = (None, None, None, Vec::new());
    for _ in 0..90 {
        if s.tick_count() == f + cast {
            s.spawn_unit(Team::Red, "Tornado", n(9500, 10500), None).expect("the Tornado");
            d = Some(s.tick_count());
        }
        s.tick();
        let t = s.tick_count() - 1;
        if let Some((_, p)) = cube(&s) {
            let p = native(p);
            if c.is_none() {
                c = Some(t);
            } else if let Some(q) = last {
                let q: (i32, i32) = q;
                steps.push((t, (p.0 - q.0, p.1 - q.1)));
            }
            last = Some(p);
        }
    }
    (c.expect("the scene drifted: no cube"), d.expect("the scene drifted: no Tornado"), steps)
}

fn len(v: (i32, i32)) -> i64 {
    royalesim::fixed::isqrt(i64::from(v.0) * i64::from(v.0) + i64::from(v.1) * i64::from(v.1))
}

/// status.ICE_CUBE_ATTRACT = client16635_row_speed (client 16.635, parity r98, the oracle's il-mg-16635 set: 10 of 10 cubes
/// inside a Tornado pulled 142 to 146 a tick): a Tornado cast 25 ticks after the cube's first frame pulls it 144 a tick (its
/// row Speed 40 x the Tornado's 360 %) toward the Tornado's centre from the cast + 2 to + 22, and not after; under unpulled
/// (the vacuity check) the cube never moves. Plant: ice_cube_unpulled.
#[test]
fn under_client16635_row_speed_a_tornado_pulls_the_cube_144_a_tick() {
    let (_, _, old) = pulled_cube(IceCubeAttract::Unpulled, 30);
    assert!(old.iter().all(|(_, v)| *v == (0, 0)), "unpulled (the vacuity check): the cube moved: {old:?}");
    let (c, d, new) = pulled_cube(IceCubeAttract::Client16635RowSpeed, 30);
    assert_eq!(d, c + 25, "the scene drifted: the cast is not on the cube's first frame + 25");
    let at = |t: u32| new.iter().find(|(x, _)| *x == t).map(|(_, v)| *v).expect("a cube frame");
    assert_eq!((at(d), at(d + 1)), ((0, 0), (0, 0)), "no pull on the cast and the cast + 1");
    let mut total = (0, 0);
    for t in d + 2..=d + 22 {
        let v = at(t);
        assert!((142..=144).contains(&len(v)) && v.1 < 0, "a pull of 142-144 toward the Tornado on {t}: {v:?}");
        total = (total.0 + v.0, total.1 + v.1);
    }
    assert!(len(total) >= 21 * 142, "the cube's displacement over the pull: {total:?}");
    assert!(new.iter().filter(|(t, _)| *t > d + 22).all(|(_, v)| *v == (0, 0)), "no pull after the cast + 22");
}

/// status.ICE_CUBE_ATTRACT = client16635_row_speed (2 of 2 cubes born inside a live Tornado first pulled on their first
/// frame + 2): a cube put down inside a Tornado cast on the press's tick + 1 takes no step on its first frame + 1 and a pull
/// on + 2; under unpulled (the vacuity check) it takes none. Plant: ice_cube_pulled_at_birth.
#[test]
fn under_client16635_row_speed_a_cube_born_in_a_tornado_is_first_pulled_on_its_first_frame_plus_2() {
    let (_, _, old) = pulled_cube(IceCubeAttract::Unpulled, 1);
    assert!(old.iter().all(|(_, v)| *v == (0, 0)), "unpulled (the vacuity check): the cube moved: {old:?}");
    let (c, _, new) = pulled_cube(IceCubeAttract::Client16635RowSpeed, 1);
    let at = |t: u32| new.iter().find(|(x, _)| *x == t).map(|(_, v)| *v).expect("a cube frame");
    assert_eq!(at(c + 1), (0, 0), "no pull on the cube's first frame + 1");
    assert!((142..=144).contains(&len(at(c + 2))), "a pull on the cube's first frame + 2: {:?}", at(c + 2));
}

/// Since parity r98 for clients 16.402 and 16.635 (unpulled before it; client 15.535.29 keeps unpulled).
#[test]
fn the_shipped_arm_is_client16635_row_speed() {
    assert_eq!(
        royalesim::state::Calib::shipped().ice_cube_attract,
        royalesim::state::IceCubeAttract::Client16635RowSpeed
    );
    assert_eq!(config().calib.ice_cube_attract, royalesim::state::IceCubeAttract::Client16635RowSpeed);
}

/// A red Knight held on (9000, 13000) walking at a blue Giant held on (6000, 13000) (its nearest enemy, out of its reach), frozen
/// by the press's cube (the hero killed after the find), let go under status.ICE_RELEASE_WALK = `arm`; the Knight is held until
/// the cube's first frame and free after. When `kill_giant`, the Giant dies on the cube's first frame + 10. (the Knight's route
/// on the cube's last frame + 1, its points on the last frame, + 1 and + 2, its route on + 2).
fn let_go(arm: IceReleaseWalk, gat: (i32, i32), kill_giant: bool) -> (Vec<Vec2>, Vec2, Vec2, Vec2, Vec<Vec2>) {
    let mut s = battle_with(|c| {
        c.calib.ice_release_attack = IceReleaseAttack::Client16635AttackNextTick;
        c.calib.ice_release_target = IceReleaseTarget::Client16635TargetNextTick;
        c.calib.ice_release_walk = arm;
    });
    let (kat, gat) = (n(AT.0, AT.1 + 4000), n(gat.0, gat.1));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let giant = s.scenario_spawn_now(Team::Blue, "Giant", gat, None).expect("the Giant");
    let hero = hero_in(&mut s);
    let gmax = s.entity(giant).expect("the Giant").max_hp;
    first_hit(&mut s, knight, kat, 120);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick();
    assert!(s.debug_set_hp(hero, 0), "the hero killed after the find");
    let (mut first, mut last, mut rows) = (None, None, Vec::new());
    for _ in 0..220 {
        if s.entity(giant).is_some() {
            assert!(s.debug_set_pos(giant, gat) && s.debug_set_hp(giant, gmax));
        }
        if first.is_none() {
            assert!(s.debug_set_pos(knight, kat));
        }
        s.tick();
        let t = s.tick_count() - 1;
        let k = s.entity(knight).expect("the Knight");
        rows.push((t, k.pos, k.route.to_vec()));
        let here = cube(&s).is_some();
        if here && first.is_none() {
            first = Some(t);
        } else if !here && first.is_some() && last.is_none() {
            last = Some(t - 1);
        }
        if kill_giant && first.is_some_and(|f| t == f + 10) && s.entity(giant).is_some() {
            assert!(s.debug_set_hp(giant, 0));
        }
        if last.is_some_and(|l| t == l + 2) {
            break;
        }
    }
    let l = last.expect("the scene drifted: no cube came and went");
    let row = |t: u32| rows.iter().find(|r| r.0 == t).cloned().expect("a frame");
    (row(l + 1).2, row(l).1, row(l + 1).1, row(l + 2).1, row(l + 2).2)
}

/// status.ICE_RELEASE_WALK = client16635_route_kept (client 16.635, parity r98, the oracle's il-mg-16635 set: 107 of 107 ground
/// units let go with a route stepped at its next waypoint; 64 of 64 whose engine replanned walked the route they held): on
/// the cube's last frame + 2 the Knight walks the route it held toward the Giant (-x, its route's goal unchanged); under
/// replans (the vacuity check) it plans afresh toward blue's King at (9000, 3000) (-y). Plant: ice_release_replans.
#[test]
fn under_client16635_route_kept_a_let_go_knight_walks_its_held_route() {
    let step = |a: Vec2, b: Vec2| (native(b).0 - native(a).0, native(b).1 - native(a).1);
    let (held, l0, l1, g, route_g) = let_go(IceReleaseWalk::Replans, (6000, 13000), false);
    assert!(!held.is_empty(), "the scene drifted: the frozen Knight holds no route");
    assert_eq!(l1, l0, "the Knight still stands on the cube's last frame + 1");
    let v = step(l1, g);
    assert!(v.1 <= -30 && v.0.abs() < v.1.abs(), "replans (the vacuity check): toward the King, not the held route: {v:?}");
    assert_ne!(route_g.last(), held.last(), "replans: a fresh plan (another goal)");
    for arm in [IceReleaseWalk::Client16635RouteKept, IceReleaseWalk::Client16635RouteKeptFliersToo] {
        let (held, l0, l1, g, route_g) = let_go(arm, (6000, 13000), false);
        assert_eq!(l1, l0, "{arm:?}: the Knight still stands on the cube's last frame + 1");
        let v = step(l1, g);
        assert!(v.0 <= -30 && v.1.abs() < v.0.abs(), "{arm:?}: along the held route toward the Giant: {v:?}");
        assert_eq!(route_g.last(), held.last(), "{arm:?}: no replan (the route's goal kept)");
    }
}

/// Since parity r98 for clients 16.402 and 16.635 (replans before it; client 15.535.29 keeps replans).
#[test]
fn the_shipped_arm_is_client16635_route_kept() {
    assert_eq!(
        royalesim::state::Calib::shipped().ice_release_walk,
        royalesim::state::IceReleaseWalk::Client16635RouteKept
    );
    assert_eq!(config().calib.ice_release_walk, royalesim::state::IceReleaseWalk::Client16635RouteKept);
}

/// Per tick: (tick, a unit's point, an i32 column of it).
type TickRows = Vec<(u32, Vec2, i32)>;

/// The row of tick `t` in `rows`.
fn row_on(rows: &[(u32, Vec2, i32)], t: u32) -> (u32, Vec2, i32) {
    rows.iter().find(|r| r.0 == t).copied().unwrap_or_else(|| panic!("no frame {t}: {rows:?}"))
}

/// The native step in `rows` into tick `t`.
fn step_into(rows: &[(u32, Vec2, i32)], t: u32) -> (i32, i32) {
    let (a, b) = (native(row_on(rows, t - 1).1), native(row_on(rows, t).1));
    (b.0 - a.0, b.1 - a.1)
}

/// The first tick in `rows` whose i32 column reads `level`.
fn first_level(rows: &[(u32, Vec2, i32)], level: i32) -> Option<u32> {
    rows.iter().find(|r| r.2 == level).map(|r| r.0)
}

/// A HERO ICE WIZARD CUBE'S VICTIM FACING THE CUBE, under collision.HELD_UNIT_AVOIDANCE = `arm` (every other ice key at its
/// shipped arm; spawner.ICE_CUBE_HERO_DEATH = cube_comes, the arm a_press_on_a_marked_knight_freezes_it_behind_the_knight
/// measures, so the hero killed after the find leaves the cube's first frame on F + 5). The hero is put down first; then
/// red Knight A on (9000, 12500), the find's target, held there until the hero's shot marks it, and red Knight B on
/// (9000, 14950), 2,450 behind it (outside the shot's 1,500 splash, so unmarked; out of the hero's sight; on dry ground
/// short of the river's bank on y 15000), walking -y. B is put back on its point before every tick through F + 4, so its
/// facing is its walk's and its one F + 4 step, the bridge's tick, leaves the cube (1,500 behind A's point at the find)
/// inside its look circle and out of its body's reach. (the cube's first frame c, its last frame L, and per tick from
/// F + 1: (tick, B's point, B's avoid_offset)).
fn cube_look(arm: HeldUnitAvoidance) -> (u32, u32, TickRows) {
    let mut s = battle_with(|c| {
        c.calib.held_unit_avoidance = arm;
        c.calib.ice_cube_hero_death = IceCubeHeroDeath::CubeComes;
    });
    let hero = hero_in(&mut s);
    let (kat, bat) = (n(9000, 12500), n(9000, 14950));
    let a = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("Knight A");
    let b = s.scenario_spawn_now(Team::Red, "Knight", bat, None).expect("Knight B");
    let a_hp = s.entity(a).expect("Knight A").hp;
    let mut marked = false;
    for _ in 0..120 {
        assert!(s.debug_set_pos(a, kat) && s.debug_set_pos(b, bat));
        s.tick();
        if s.entity(a).expect("Knight A").hp < a_hp {
            marked = true;
            break;
        }
    }
    assert!(marked, "the scene drifted: no shot of the hero's reached Knight A in 120 ticks");
    let bv = s.entity(b).expect("Knight B");
    assert_eq!(bv.hp, bv.max_hp, "the scene drifted: Knight B was hit (and marked)");
    assert!(s.debug_set_pos(a, kat) && s.debug_set_pos(b, bat));
    let f = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick(); // F: the press is debited, fires and finds in the tick it lands in
    let at_find = native(s.entity(a).expect("Knight A").pos);
    assert!(s.debug_set_hp(hero, 0), "the hero killed after the find");
    let (mut c, mut last): (Option<u32>, Option<u32>) = (None, None);
    let mut cube_at: Option<(i32, i32)> = None;
    let mut rows: TickRows = Vec::new();
    let mut look: Vec<(u32, Vec2, bool)> = Vec::new();
    for _ in 0..150 {
        if s.tick_count() <= f + 4 {
            assert!(s.debug_set_pos(b, bat));
        }
        s.tick();
        let t = s.tick_count() - 1;
        let e = s.entity(b).expect("Knight B");
        rows.push((t, e.pos, e.avoid_offset));
        look.push((t, e.facing, !e.route.is_empty()));
        match cube(&s) {
            Some((_, cp)) if c.is_none() => {
                c = Some(t);
                cube_at = Some(native(cp));
            }
            None if c.is_some() && last.is_none() => last = Some(t - 1),
            _ => {}
        }
        if last.is_some_and(|l| t == l + 3) {
            break;
        }
    }
    let c = c.expect("the scene drifted: no cube came");
    let l = last.expect("the scene drifted: the cube never went");
    let cp = cube_at.expect("seen on c");
    assert_eq!(c, f + 5, "the scene drifted: the cube's first frame is not F + 5");
    assert_eq!(cp, (at_find.0, at_find.1 + 1500), "the scene drifted: the cube is not 1,500 behind Knight A's point at the find");
    let facing = look.iter().find(|r| r.0 == c).map(|r| r.1).expect("a frame on c");
    let routed = look.iter().find(|r| r.0 == l + 1).map(|r| r.2).expect("a frame on L + 1");
    let held = native(row_on(&rows, l + 1).1);
    let from_cube = |p: (i32, i32)| len((p.0 - cp.0, p.1 - cp.1));
    assert!(from_cube(held) > 835, "the scene drifted: Knight B touches the cube on L + 1 ({held:?}, the cube {cp:?})");
    let ahead = (held.0 + facing.x, held.1 + facing.y);
    assert!(from_cube(ahead) < 835, "the scene drifted: the cube is out of Knight B's look circle ({held:?} facing {facing:?}, the cube {cp:?})");
    assert_eq!(row_on(&rows, c + 2).1, row_on(&rows, l + 1).1, "the scene drifted: Knight B moved in the hold");
    assert!(routed, "the scene drifted: Knight B holds no route on L + 1");
    (c, l, rows)
}

/// collision.HELD_UNIT_AVOIDANCE = client16635_cube_scanned (client 16.635, parity r99, the oracle's il-mg-16635 raw
/// frames: 35 of 35 walking victims whose look circle held the cube took the static answer on its first frame + 1, and
/// 1,244 later ticks were held at 190): Knight B, held by the cube's bridge and freeze with the cube in its look circle,
/// meets nothing on the cube's first frame c (the cube is no body in c's move pass), starts on c + 1 at 190 and holds it
/// through the cube's death tick L + 1; on the let-go tick L + 2 the offset is 180 and turns its step (about 67 degrees
/// off its -y route). Under masked (the vacuity check) its offset stays 0 and the let-go step runs along its route.
/// Plant: ice_hold_masked.
#[test]
fn under_client16635_cube_scanned_a_cube_victim_facing_the_cube_holds_190_and_turns_its_let_go_step() {
    let (c, l, old) = cube_look(HeldUnitAvoidance::Masked);
    for t in c..=l + 2 {
        assert_eq!(row_on(&old, t).2, 0, "masked (the vacuity check): an offset on {t} (c {c}, L {l})");
    }
    let v = step_into(&old, l + 2);
    assert!(v.1 < 0 && v.0.abs() < v.1.abs(), "masked (the vacuity check): the let-go step along its -y route: {v:?}");
    let (c, l, new) = cube_look(HeldUnitAvoidance::Client16635CubeScanned);
    assert_eq!(row_on(&new, c).2, 0, "client16635_cube_scanned: an offset on the cube's first frame c");
    let first = row_on(&new, c + 1).2;
    assert_eq!(first.abs(), 190, "client16635_cube_scanned: the static answer on c + 1");
    for t in c + 1..=l + 1 {
        assert_eq!(row_on(&new, t).2, first, "client16635_cube_scanned: the offset held through the cube's death tick (tick {t}, c {c}, L {l})");
    }
    assert_eq!(row_on(&new, l + 2).2, first.signum() * 180, "client16635_cube_scanned: the decay on the let-go tick");
    let v = step_into(&new, l + 2);
    assert!(v != (0, 0) && v.0.abs() > v.1.abs(), "client16635_cube_scanned: the let-go step turned off its -y route: {v:?}");
}

/// A HERO'S PRESS WAITING THROUGH A CUBE'S HOLD, under status.ICE_RELEASE_PRESS = `arm` (every other ice key at its
/// shipped arm; spawner.ICE_CUBE_HERO_DEATH = cube_comes, as in `cube_look`): red's deck holds the Hero Mini PEKKA. The
/// Ice Wizard hero is put down first; then the Mini PEKKA on (9000, 13000), held there until the hero's shot marks it; the
/// press finds it on F (the hero killed after) and the cube holds it; red presses its button on the cube's first frame
/// c + 10, inside the hold. (the cube's last frame L, and per tick from F + 1: (tick, the Mini PEKKA's point, its level)).
fn held_press(arm: IceReleasePress) -> (u32, TickRows) {
    let mut s = battle_with(|c| {
        c.calib.ice_release_press = arm;
        c.calib.ice_cube_hero_death = IceCubeHeroDeath::CubeComes;
        c.decks[1][0] = "MiniPekka".into();
        c.forms[1] = vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0];
    });
    let hero = hero_in(&mut s);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    let kat = n(9000, 13000);
    s.spawn_unit(Team::Red, "MiniPekka_hero", kat, None).expect("the Mini PEKKA");
    s.tick();
    let mp = find_live(&s, Team::Red, "MiniPekka_hero")[0].id;
    first_hit(&mut s, mp, kat, 120);
    assert!(s.debug_set_pos(mp, kat));
    let f = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.tick(); // F
    assert!(s.debug_set_hp(hero, 0), "the hero killed after the find");
    let (mut c, mut last): (Option<u32>, Option<u32>) = (None, None);
    let mut rows: TickRows = Vec::new();
    for _ in 0..160 {
        if c.is_some_and(|c0| s.tick_count() == c0 + 10) {
            s.press_ability_button(Team::Red, 0).expect("the Mini PEKKA's press, inside the hold");
        }
        s.tick();
        let t = s.tick_count() - 1;
        let e = s.entity(mp).expect("the Mini PEKKA lives");
        rows.push((t, e.pos, e.level));
        let here = cube(&s).is_some();
        if here && c.is_none() {
            c = Some(t);
        } else if !here && c.is_some() && last.is_none() {
            last = Some(t - 1);
        }
        if last.is_some_and(|l| t == l + 8) {
            break;
        }
    }
    let c = c.expect("the scene drifted: no cube came");
    let l = last.expect("the scene drifted: the cube never went");
    assert_eq!(c, f + 5, "the scene drifted: the cube's first frame is not F + 5");
    let held = row_on(&rows, c + 2).1;
    for t in c + 2..=l + 1 {
        assert_eq!(row_on(&rows, t).1, held, "the scene drifted: the Mini PEKKA moved in the hold (tick {t})");
    }
    assert!(rows.iter().filter(|r| r.0 <= l + 2).all(|r| r.2 == 11), "the scene drifted: a level set by the let-go tick: {rows:?}");
    (l, rows)
}

/// status.ICE_RELEASE_PRESS = client16635_press_next_tick (client 16.635, parity r99, the oracle's il-mg-16635 set: 7 of 7
/// presses waiting through a cube's hold did nothing on the let-go tick G, the cube's last frame + 2; a Hero Mini PEKKA's
/// stood from G + 2 and healed on G + 4): the Mini PEKKA steps on G + 1, stands from G + 2, and its level set lands on
/// G + 4; under with_the_walk (the vacuity check) its press starts on G, it stands from G + 1, and the level set lands on
/// G + 3. Plant: ice_press_on_let_go.
#[test]
fn under_client16635_press_next_tick_a_press_waiting_through_the_hold_starts_a_tick_after_the_let_go() {
    let (l, old) = held_press(IceReleasePress::WithTheWalk);
    let g = l + 2;
    assert_eq!(first_level(&old, 12), Some(g + 3), "with_the_walk (the vacuity check): the level set on G + 3 (G {g})");
    assert_eq!(row_on(&old, g + 2).1, row_on(&old, g + 1).1, "with_the_walk: in its cast hold from G + 1");
    let (l, new) = held_press(IceReleasePress::Client16635PressNextTick);
    let g = l + 2;
    assert_eq!(first_level(&new, 12), Some(g + 4), "client16635_press_next_tick: the level set on G + 4 (G {g})");
    assert_ne!(row_on(&new, g + 1).1, row_on(&new, g).1, "client16635_press_next_tick: it steps on G + 1");
    assert_eq!(row_on(&new, g + 2).1, row_on(&new, g + 1).1, "client16635_press_next_tick: in its cast hold from G + 2");
}

/// Since parity r99 for clients 16.402 and 16.635 (with_the_walk before it; client 15.535.29 keeps with_the_walk).
#[test]
fn the_shipped_arm_is_client16635_press_next_tick() {
    assert_eq!(
        royalesim::state::Calib::shipped().ice_release_press,
        royalesim::state::IceReleasePress::Client16635PressNextTick
    );
    assert_eq!(config().calib.ice_release_press, royalesim::state::IceReleasePress::Client16635PressNextTick);
}

/// Since parity r99 for clients 16.402 and 16.635 (masked before it; client 15.535.29 keeps scanned).
#[test]
fn the_shipped_arm_is_client16635_cube_scanned() {
    assert_eq!(
        royalesim::state::Calib::shipped().held_unit_avoidance,
        royalesim::state::HeldUnitAvoidance::Client16635CubeScanned
    );
    assert_eq!(config().calib.held_unit_avoidance, royalesim::state::HeldUnitAvoidance::Client16635CubeScanned);
}
