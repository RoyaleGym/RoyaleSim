//! THE HERO TOMBSTONE'S MONSTER (tools/extract_cards.py `tomb_group`, `tomb_button`; card.rs `TombMonsterDef`,
//! TOMB_KILL_TICKS, `CardDb::spent_unit`; state.rs `TombRun`, `tomb_status`, `tomb_resolve`, `note_tomb_play`), at level
//! 11.
//!
//! THE MEASUREMENTS (client 15.535.29, Oracle's sp-hero2-Tombstone-full-s0 / -s1 (red), -half-s0, -nopress-s0, -window-s0,
//! -death-s0; the press P on the monster's first active tick): the monster at 4224 of 4224 on P + 1 whatever its tomb's
//! hitpoints; one step (-106, -106) on P + 2 beside its standing tomb (red: (+106, +106)), then still until P + 41; the
//! tomb gone on P + 3 with four Skeletons; with no press, the monster gone 50 ticks after its tomb, a press 20 ticks
//! after the tomb accepted; the active monster's death putting four Skeletons down at its last point + (500, 0),
//! (-500, 0), (-500, -1000) and (500, -1000) on the tick after its last, deploying 10 ticks.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_tombstone`): tomb_monster_never, tomb_never, tomb_never_killed, tomb_window_unread, tomb_play_ignored,
//! tomb_monster_unstepped, ability_available_ignores_windows (the no-press test: the row reads the button available
//! after the tomb's window closes, while the monster waits), tomb_step_every_press (transform.TOMB_MONSTER_STEP_SCOPE's new
//! arm steps a monster pressed after its tomb's death), tomb_press_spawn_reads_moved (spawner.TOMB_PRESS_SPAWN_FIRST_UPDATE's
//! new arm's death spawn reads the moved board), hero_tomb_emission_radial (spawner.HERO_TOMB_EMISSION_NUDGE's new arm keeps
//! the radial push alone).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, HeroTombEmissionNudge, TombMonsterStepScope, TombPressSpawnFirstUpdate};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Tombstone", "Knight", "Archers", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
const TOMB: &str = "Tombstone_hero";
const WAITING: &str = "TombstoneHero_Monster_Passive";
const ACTIVE: &str = "TombstoneHero_Monster_Active";

fn battle() -> BattleState {
    battle_with(TombMonsterStepScope::EveryPress)
}

fn battle_with(scope: TombMonsterStepScope) -> BattleState {
    let mut cfg = config();
    cfg.calib.tomb_monster_step_scope = scope;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    s
}

/// The hero Tombstone played by `team` at `at`, its monster stood up: the battle, the tomb and the monster.
fn play(s: &mut BattleState, team: Team, at: Vec2) -> (EntityId, EntityId) {
    s.scenario_set_elixir_milli(team, 10_000);
    s.deploy(team, "Tombstone", at).expect("the play");
    for _ in 0..120 {
        s.tick();
        let (t, m) = (find_live(s, team, TOMB), find_live(s, team, WAITING));
        if let (Some(t), Some(m)) = (t.first(), m.first()) {
            if !m.deploying {
                return (t.id, m.id);
            }
        }
    }
    panic!("no tomb and monster stood up");
}

fn skeletons(s: &BattleState, team: Team) -> Vec<(EntityId, Vec2)> {
    s.entities().filter(|e| e.team == team && e.card.starts_with("Skeleton")).map(|e| (e.id, e.pos)).collect()
}

#[test]
fn its_press_makes_its_monster_active_at_full_hitpoints_and_its_tomb_dies_on_the_press_plus_3() {
    let mut s = battle();
    let (tomb, m) = play(&mut s, Team::Blue, n(14500, 11500));
    assert_eq!(s.entity(m).expect("the monster").max_hp, 529, "the monster waiting: 207 at level 11");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    assert_eq!(s.press_ability_button(Team::Blue, 0).expect("the press"), m, "the press is the monster's");
    let mut rows: Vec<(u32, bool, i32, i32, bool, usize)> = Vec::new();
    for _ in 0..4 {
        s.tick();
        let e = s.entity(m).expect("the monster");
        rows.push((s.tick_count() - 1 - p, e.card == ACTIVE, e.hp, e.max_hp, s.entity(tomb).is_some(), skeletons(&s, Team::Blue).len()));
    }
    let r = |k: u32| *rows.iter().find(|x| x.0 == k).expect("a row");
    assert!(r(1).1 && r(1).2 == 4224 && r(1).3 == 4224, "active at 4224 of 4224 on P + 1: {rows:?}");
    assert!(r(2).4, "its tomb standing on P + 2: {rows:?}");
    assert!(!r(3).4, "its tomb gone on P + 3: {rows:?}");
    assert_eq!(r(3).5 - r(2).5, 4, "its tomb's four Skeletons on P + 3: {rows:?}");
}

#[test]
fn its_active_monster_steps_106_106_beside_its_tomb_then_stands_to_the_press_plus_40() {
    let mut s = battle();
    let (_, m) = play(&mut s, Team::Blue, n(14500, 11500));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows: Vec<(u32, i32, i32)> = Vec::new();
    for _ in 0..43 {
        s.tick();
        let e = s.entity(m).expect("the monster");
        rows.push((s.tick_count() - 1 - p, e.pos.x / K, e.pos.y / K));
    }
    let r = |k: u32| *rows.iter().find(|x| x.0 == k).expect("a row");
    assert_eq!((r(2).1 - r(1).1, r(2).2 - r(1).2), (-106, -106), "its one step on P + 2: {rows:?}");
    for k in 3..=40 {
        assert_eq!((r(k).1, r(k).2), (r(2).1, r(2).2), "standing on P + {k}: {rows:?}");
    }
    assert_ne!((r(41).1, r(41).2), (r(40).1, r(40).2), "walking on P + 41: {rows:?}");
}

#[test]
fn with_no_press_its_monster_dies_50_ticks_after_its_tomb_and_a_press_20_ticks_after_the_tomb_is_taken() {
    // No press: the monster's last frame is its tomb's death tick + 49.
    let mut s = battle();
    let (tomb, m) = play(&mut s, Team::Blue, n(14500, 11500));
    assert!(s.debug_set_hp(tomb, 0));
    s.tick();
    let d = s.tick_count() - 1;
    assert!(s.entity(tomb).is_none(), "the tomb dead on its tick");
    let mut last = d;
    for _ in 0..60 {
        s.tick();
        // The window closes on the tomb's death + 20 while the monster waits to + 49: the row reads it as the check does.
        assert_buttons_agree(&s, Team::Blue);
        if s.entity(m).is_some() {
            last = s.tick_count() - 1;
        }
    }
    assert_eq!(last - d, 49, "the waiting monster's last frame after its tomb's death");
    // A press 20 ticks after the tomb: taken, the monster active at full hitpoints.
    let mut s = battle();
    let (tomb, m) = play(&mut s, Team::Blue, n(14500, 11500));
    assert!(s.debug_set_hp(tomb, 0));
    for _ in 0..21 {
        s.tick();
    }
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press 20 ticks after the tomb");
    s.tick();
    let e = s.entity(m).expect("the monster");
    assert!(e.card == ACTIVE && e.hp == 4224, "active at full hitpoints: {} {}", e.card, e.hp);
    // A press 31 ticks after the tomb: refused.
    let mut s = battle();
    let (tomb, _) = play(&mut s, Team::Blue, n(14500, 11500));
    assert!(s.debug_set_hp(tomb, 0));
    for _ in 0..32 {
        s.tick();
    }
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    assert!(!s.ability_buttons(Team::Blue)[0].available, "the row reads the closed window as the check does");
    assert!(s.press_ability_button(Team::Blue, 0).is_err(), "the press 31 ticks after the tomb refused");
}

#[test]
fn a_second_play_of_its_card_kills_a_monster_still_waiting() {
    let mut s = battle();
    let (_, m) = play(&mut s, Team::Blue, n(14500, 11500));
    s.spawn_unit(Team::Blue, "Tombstone", n(3500, 11500), None).expect("the second play");
    for _ in 0..3 {
        s.tick();
    }
    assert!(s.entity(m).is_none(), "the first monster gone after the second play");
}

#[test]
fn its_active_monsters_death_puts_four_skeletons_down_around_its_last_point_on_the_next_tick() {
    let mut s = battle();
    let (_, m) = play(&mut s, Team::Blue, n(14500, 11500));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..10 {
        s.tick();
    }
    let at = s.entity(m).expect("the monster").pos;
    let before: Vec<EntityId> = skeletons(&s, Team::Blue).into_iter().map(|x| x.0).collect();
    // The tomb's own Skeletons gone with it: nothing but the four stands by the point.
    for id in &before {
        assert!(s.debug_set_hp(*id, 0));
    }
    assert!(s.debug_set_hp(m, 0));
    s.tick();
    assert!(s.entity(m).is_none(), "the monster dead on its tick");
    s.tick();
    let mut new: Vec<(i32, i32)> =
        skeletons(&s, Team::Blue).into_iter().filter(|x| !before.contains(&x.0)).map(|(_, p)| ((p.x - at.x) / K, (p.y - at.y) / K)).collect();
    new.sort_unstable();
    assert_eq!(new, vec![(-500, -1000), (-500, 0), (500, -1000), (500, 0)], "the four Skeletons around its last point");
}

/// transform.TOMB_MONSTER_STEP_SCOPE (client 15.535.29: 6 of 6 presses beside a standing tomb stepped (-106, -106); the
/// one press after the tomb's death, sp-hero2-Tombstone-window-s0 t320, did not): the tomb taken, the press 10 ticks after,
/// the monster's move from the trigger's tick to P + 3. Plant: tomb_step_every_press.
fn pressed_after_the_tomb(scope: TombMonsterStepScope) -> (i32, i32) {
    let mut s = battle_with(scope);
    let (tomb, m) = play(&mut s, Team::Blue, n(14500, 11500));
    assert!(s.debug_set_hp(tomb, 0));
    for _ in 0..10 {
        s.tick();
    }
    assert!(s.entity(tomb).is_none(), "the scene drifted: the tomb stands");
    let a = s.entity(m).expect("the monster").pos;
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press 10 ticks after the tomb");
    for _ in 0..3 {
        s.tick();
    }
    let b = s.entity(m).expect("the monster").pos;
    ((b.x - a.x) / K, (b.y - a.y) / K)
}

#[test]
fn a_monster_pressed_after_its_tombs_death_takes_no_step_under_client15535_tomb_standing() {
    assert_eq!(pressed_after_the_tomb(TombMonsterStepScope::Client15535TombStanding), (0, 0), "client15535_tomb_standing: it moved");
    // NOT VACUOUS: every_press steps it.
    assert_eq!(pressed_after_the_tomb(TombMonsterStepScope::EveryPress), (-106, -106), "every_press: not its one step");
}

#[test]
fn a_monster_pressed_beside_its_standing_tomb_steps_under_both_arms() {
    for scope in [TombMonsterStepScope::EveryPress, TombMonsterStepScope::Client15535TombStanding] {
        let mut s = battle_with(scope);
        let (_, m) = play(&mut s, Team::Blue, n(14500, 11500));
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        let a = s.entity(m).expect("the monster").pos;
        s.press_ability_button(Team::Blue, 0).expect("the press");
        for _ in 0..3 {
            s.tick();
        }
        let b = s.entity(m).expect("the monster").pos;
        assert_eq!(((b.x - a.x) / K, (b.y - a.y) / K), (-106, -106), "{scope:?}: not its one step");
    }
}

/// spawner.TOMB_PRESS_SPAWN_FIRST_UPDATE (client 15.535.29: 16 of 16 press-kill members pushed from the neighbour's
/// start-of-tick point): the tomb pressed with a Blue Knight held 980 south of its death spawn's birth point at every
/// tick's start (overlap 20 with a member there: a push of 121), walking north on each tick (60: overlap 80, the push's
/// cap). The death spawn's first-frame points, sorted; `knight`: whether the Knight is there.
fn press_spawn_first_frame(arm: TombPressSpawnFirstUpdate, knight: bool) -> Vec<(i32, i32)> {
    let mut cfg = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.tomb_press_spawn_first_update = arm;
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    let (tomb, _) = play(&mut s, Team::Blue, n(14500, 11500));
    let held = n(14500, 13000 - 980);
    let k = if knight { Some(s.scenario_spawn_now(Team::Blue, "Knight", held, None).expect("the Knight")) } else { None };
    // The tomb's Skeletons so far gone, so nothing else walks by the birth point.
    let before: Vec<EntityId> = skeletons(&s, Team::Blue).into_iter().map(|x| x.0).collect();
    for id in &before {
        assert!(s.debug_set_hp(*id, 0));
    }
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..6 {
        if let Some(k) = k {
            assert!(s.debug_set_pos(k, held));
        }
        s.tick();
        if s.entity(tomb).is_none() {
            let mut out: Vec<(i32, i32)> = skeletons(&s, Team::Blue).into_iter().filter(|x| !before.contains(&x.0)).map(|(_, p)| (p.x / K, p.y / K)).collect();
            out.sort_unstable();
            assert_eq!(out.len(), 4, "{arm:?}: the scene drifted: not four death-spawn Skeletons ({out:?})");
            return out;
        }
    }
    panic!("{arm:?}: the scene drifted: the tomb outlived the press");
}

/// Plant: tomb_press_spawn_reads_moved.
#[test]
fn a_press_kills_death_spawn_reads_the_start_of_tick_board_under_client15535_start_of_tick() {
    // NOT VACUOUS: with the Knight walking through the kill's tick, the boards part.
    let new = press_spawn_first_frame(TombPressSpawnFirstUpdate::Client15535StartOfTick, true);
    let old = press_spawn_first_frame(TombPressSpawnFirstUpdate::PostMove, true);
    assert_ne!(new, old, "the arms put the death spawn on the same points with the Knight walking");
    // With nothing moving by the birth point the boards agree.
    let new = press_spawn_first_frame(TombPressSpawnFirstUpdate::Client15535StartOfTick, false);
    let old = press_spawn_first_frame(TombPressSpawnFirstUpdate::PostMove, false);
    assert_eq!(new, old, "the arms part with nothing moving by the birth point");
}

/// spawner.HERO_TOMB_EMISSION_NUDGE (client 15.535.29: 15 of 15 hero emissions touching their tomb alone landed (1, 1) off
/// the walk, side 1 (-1, -1), where the radial push gives (0, 1) / (0, -1)): `team`'s hero Tombstone stood up alone, its
/// first emitted Skeleton's first-frame point. Plant: hero_tomb_emission_radial.
fn first_emission(team: Team, nudge: HeroTombEmissionNudge) -> Vec2 {
    let mut cfg = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.hero_tomb_emission_nudge = nudge;
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    let at = if team == Team::Blue { n(14500, 11500) } else { n(3500, 20500) };
    s.scenario_set_elixir_milli(team, 10_000);
    s.deploy(team, "Tombstone", at).expect("the play");
    for _ in 0..200 {
        s.tick();
        if let Some((_, p)) = skeletons(&s, team).first() {
            return *p;
        }
    }
    panic!("{team:?}: the scene drifted: no Skeleton emitted");
}

#[test]
fn a_hero_tombs_emission_touching_it_alone_lands_one_to_its_right_under_client15535_right_one() {
    for (team, right) in [(Team::Blue, 1), (Team::Red, -1)] {
        let new = first_emission(team, HeroTombEmissionNudge::Client15535RightOne);
        let old = first_emission(team, HeroTombEmissionNudge::None);
        assert_eq!(((new.x - old.x) / K, (new.y - old.y) / K), (right, 0), "{team:?}: the nudge against the radial push alone");
    }
}
