//! A COPY OF A BATTLE UNDER WAY (state.rs `scenario_set_king_wake`, `scenario_set_evo_plays`,
//! `scenario_set_last_played`, `scenario_spawn_batch_levels`, `scenario_spawn_copy`, `scenario_spell_in_flight`;
//! card.rs `copy_refusal`; spell.rs `in_flight_kind`;
//! py.rs `Battle.reset`'s king_wake_ms, evo_counts, last_played, unit_spawns and spells_in_flight): the pieces of
//! state a fresh battle does not have, set at the start.
//!   1. a king's wake clock: awake at the activation delay targets from the first ticks, one still waking wakes on its
//!      own clock, none is asleep, and none is refused with a princess down (that fall was a trigger);
//!   2. an evolved card's count makes its next play the one it says, and a count past its cycle, or for a card the deck
//!      does not carry evolved, is refused;
//!   3. a Mirror's last play makes the Mirror copy it;
//!   4. a summoned unit goes down by its record's name at the level it is given;
//!   5. two spawns that differ in their level alone go down in one order whatever the list's;
//!   6. a Fireball put on its flight lands on the tick the same flight from its launch does; one entry is one object
//!      with its own delay (an Arrows wave); a record that does not fly is refused by its kind;
//!   7. a copy's evolved unit starts its form as its play does (an Evo Musketeer snipes, where a setup spawn's does
//!      not), but casts none of the play's own effects (no Evo Cannon barrage); a hero's record its play makes stands
//!      with its button, and one a press makes, or the tomb's waiting monster, is refused;
//!   8. a copy's Evo Skeleton Army Spectral is invisible, as one a soldier's death makes; a copy's Evo Tesla rings when
//!      it comes up, not at its creation, as a setup spawn's does; the Evo Lumberjack's ghost is refused, the Hero
//!      Balloon's bomb is not.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! scenario_copy`):
//!   - copy_king_wake_unset -> 1 red;
//!   - copy_evo_plays_unset -> 2 red;
//!   - copy_last_played_unset -> 3 red;
//!   - copy_spawn_level_ignored -> 4 red;
//!   - copy_spell_flight_from_launch -> 6 red;
//!   - copy_form_unstarted -> 7 red (and tests/evo_mega_knight.rs `..._a_copys_knight_counts_as_a_played_one`);
//!   - copy_casts_barrage -> 7 red;
//!   - copy_spectral_as_soldier -> 8 red;
//!   - copy_rings_at_creation -> 8 red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["BabyDragon".into(), "Knight".into(), "Fireball".into(), "Mirror".into()], vec!["Knight".into(), "Golem".into()]];
    cfg.forms = [vec![1, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// The ticks, of `ticks`, on which Blue's king has a target, with a red Knight held in its reach (4500 from it) and
/// topped up so the princess towers' fire does not kill it.
fn king_targets(s: &mut BattleState, ticks: u32) -> Vec<u32> {
    let k = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 7500), None).expect("the Knight");
    let king = s.tower_ids(Team::Blue)[0].expect("the king");
    let mut on = Vec::new();
    for t in 1..=ticks {
        let top = s.entity(k).expect("the Knight").max_hp;
        assert!(s.debug_set_pos(k, n(9000, 7500)) && s.debug_set_hp(k, top));
        s.tick();
        if s.entity(king).expect("the king").target.is_some() {
            on.push(t);
        }
    }
    on
}

#[test]
fn a_kings_wake_clock_is_taken_as_given() {
    let wake = config().calib.king_activate_time_ms;
    let tick_ms = config().calib.tick_ms;
    let mut s = battle();
    assert!(king_targets(&mut s.clone(), 40).is_empty(), "the control: a fresh king sleeps");
    s.scenario_set_king_wake(Team::Blue, Some(wake)).expect("an awake king");
    assert_eq!(s.king_wake_ms(Team::Blue), Some(wake));
    let on = king_targets(&mut s, 40);
    assert!(on.first().is_some_and(|t| *t <= 10), "the awake king targets the Knight within 10 ticks: {on:?}");
    // waking: 1000 ms short of the delay, it wakes on its own clock 20 ticks on
    let mut s = battle();
    s.scenario_set_king_wake(Team::Blue, Some(wake - 1000)).expect("a waking king");
    let first = king_targets(&mut s, 60).first().copied();
    let due = (1000 / tick_ms) as u32;
    assert!(first.is_some_and(|t| t >= due && t <= due + 10), "the waking king targets from tick {due} on: {first:?}");
    // a princess down: asleep is refused, the fall was its trigger
    let mut s = battle();
    s.scenario_set_tower_hp(Team::Blue, 1, 0).expect("a princess down");
    assert!(s.scenario_set_king_wake(Team::Blue, None).is_err());
    assert!(s.scenario_set_king_wake(Team::Blue, Some(-1)).is_err(), "a negative clock");
}

#[test]
fn an_evo_count_makes_the_play_it_says() {
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "BabyDragon").expect("the dragon in hand");
        s.cards().get(s.resolve_play(Team::Blue, slot).expect("a play resolves").card).name.clone()
    };
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's cycle");
    assert_eq!(next(&s), "BabyDragon", "a fresh count: a basic play");
    s.scenario_set_evo_plays(Team::Blue, "BabyDragon", 2).expect("a count at its cycle");
    assert_eq!(next(&s), "BabyDragon_EV1", "two basic plays done: the next is the form");
    s.scenario_set_evo_plays(Team::Blue, "BabyDragon", 1).expect("a count below it");
    assert_eq!(next(&s), "BabyDragon", "one done: the next is basic");
    assert!(s.scenario_set_evo_plays(Team::Blue, "BabyDragon", 3).is_err(), "past its cycle");
    assert!(s.scenario_set_evo_plays(Team::Blue, "Knight", 1).is_err(), "a card the deck does not carry evolved");
    assert!(s.scenario_set_evo_plays(Team::Red, "BabyDragon", 1).is_err(), "the other side's deck");
}

#[test]
fn a_mirrors_last_play_is_taken() {
    let mirror = |s: &BattleState| {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "Mirror").expect("the Mirror in hand");
        s.resolve_play(Team::Blue, slot).map(|p| s.cards().get(p.card).name.clone())
    };
    let mut s = battle();
    assert!(mirror(&s).is_err(), "the control: nothing to copy in a fresh battle");
    s.scenario_set_last_played(Team::Blue, Some("Fireball")).expect("a last play");
    assert_eq!(mirror(&s), Ok("Fireball".to_string()), "the Mirror copies the Fireball");
    s.scenario_set_last_played(Team::Blue, None).expect("none");
    assert!(mirror(&s).is_err(), "none again");
}

#[test]
fn a_summoned_unit_goes_down_by_name_at_its_level() {
    let mut s = battle();
    let db = s.cards().clone();
    let g = db.index("Golemite").expect("the Golemite record");
    let hp_at = |level: i32| db.scaled(g, level, db.get(g).hitpoints).expect("the level");
    assert_ne!(hp_at(9), hp_at(11), "the levels differ in hitpoints");
    let ids = s
        .scenario_spawn_batch_levels(&[(Team::Red, "Golemite", n(5000, 20000), None, Some(9)), (Team::Red, "Golemite", n(13000, 20000), None, None)])
        .expect("two Golemites");
    let max = |s: &BattleState, id: EntityId| s.entity(id).expect("the Golemite").max_hp;
    assert_eq!((max(&s, ids[0]), max(&s, ids[1])), (hp_at(9), hp_at(11)), "level 9 as given; the other at the side's 11");
    assert_eq!(s.entity(ids[0]).expect("the Golemite").card, "Golemite");
    assert!(!s.entity(ids[0]).expect("the Golemite").deploying, "standing, deployed");
}

#[test]
fn spawns_that_differ_in_level_alone_go_down_in_one_order() {
    let at = n(9000, 20000);
    let hash_of = |order: [i32; 2]| {
        let mut s = battle();
        let specs: Vec<(Team, &str, Vec2, Option<i32>, Option<i32>)> = order.iter().map(|l| (Team::Red, "Golemite", at, Some(100), Some(*l))).collect();
        s.scenario_spawn_batch_levels(&specs).expect("two Golemites");
        s.state_hash()
    };
    assert_eq!(hash_of([9, 11]), hash_of([11, 9]), "the list's order does not reach the battle");
}

/// The first Flight object's point after each tick, and the tick the last Flight object left.
fn flight(mut s: BattleState) -> (Vec<Vec2>, u32) {
    let mut at = Vec::new();
    for t in 1..=200u32 {
        s.tick();
        match s.spells().iter().find_map(|sp| match &sp.motion {
            SpellMotion::Flight { pos, .. } => Some(*pos),
            _ => None,
        }) {
            Some(p) => at.push(p),
            None => return (at, t),
        }
    }
    panic!("the spell never landed");
}

#[test]
fn a_spell_on_its_flight_lands_when_the_same_flight_does() {
    let mut fresh = battle();
    fresh.scenario_set_elixir_milli(Team::Blue, 10_000);
    // the cast's aim is the point the play resolved to (its tile centre)
    let aim = fresh.deploy(Team::Blue, "Fireball", n(9000, 22000)).expect("the cast");
    let (path, land) = flight(fresh);
    assert!(path.len() > 10, "the scene drifted: a flight of {} ticks", path.len());
    // put on its way at the point the fresh flight had reached after tick m: it lands land - m ticks later
    let m = path.len() / 2;
    let mut copy = battle();
    copy.scenario_spell_in_flight(Team::Blue, "Fireball", None, path[m - 1], aim, 0).expect("the Fireball in flight");
    let (_, copy_land) = flight(copy);
    assert_eq!(copy_land, land - m as u32, "the copy from tick {m}'s point ({:?})", path[m - 1]);
    // one entry is one object, with its own delay: an Arrows wave waiting 400 ms stands for 8 ticks, then flies
    let mut s = battle();
    let start = n(9000, 3000);
    s.scenario_spell_in_flight(Team::Blue, "Arrows", None, start, aim, 400).expect("one Arrows wave");
    assert_eq!(s.spells().len(), 1, "one object for one entry");
    for t in 1..=10u32 {
        s.tick();
        let p = s.spells().iter().find_map(|sp| match &sp.motion {
            SpellMotion::Flight { pos, .. } => Some(*pos),
            _ => None,
        });
        assert_eq!(p == Some(start), t <= 8, "tick {t}: the wave waits out its 400 ms, then moves ({p:?})");
    }
    // refused by its kind, asked first as a copy's builder does (spell.rs `in_flight_kind`)
    let e = s.scenario_spell_in_flight(Team::Blue, "Zap", None, aim, aim, 0).expect_err("a Zap does not fly");
    assert!(e.contains("does not fly (at_once)"), "{e}");
    let e = s.scenario_spell_in_flight(Team::Blue, "Log", None, aim, aim, 0).expect_err("the Log rolls");
    assert!(e.contains("does not fly (rolls)"), "{e}");
    let db = s.cards().clone();
    let kind = |n: &str| royalesim::spell::in_flight_kind(db.get(db.index(n).unwrap_or_else(|| panic!("{n}"))));
    assert_eq!(
        ["Fireball", "Rocket", "Arrows", "Snowball", "Log", "BarbLog", "Snowball_EV1", "Zap", "Knight"].map(kind),
        ["flies", "flies", "flies", "flies", "rolls", "rolls", "rolls", "at_once", "not_a_spell"]
    );
}

/// The Evo Musketeer's snipes (shots at the snipe's Speed 2650) of 260 ticks after she is put down at (5499, 5500), a red
/// Giant at (3499, 17499) walking down to her: by a copy (`scenario_spawn_copy`) or by a setup spawn.
fn snipes_put(copy: bool) -> usize {
    let mut s = battle();
    let form = s.cards().index("Musketeer_EV1").expect("the evolved Musketeer loads");
    let mult = s.config().calib.projectile_speed_to_subtiles_per_tick;
    let her = [(Team::Blue, "Musketeer_EV1", n(5499, 5500), None, None)];
    let id = if copy { s.scenario_spawn_copy(&her) } else { s.scenario_spawn_batch_levels(&her) }.expect("the Musketeer")[0];
    assert_eq!(s.snipes_left(), vec![(id, if copy { 3 } else { 0 })], "her snipes left as she is put down");
    s.scenario_spawn_now(Team::Red, "Giant", n(3499, 17499), None).expect("the Giant");
    let mut snipes = 0;
    for _ in 0..260 {
        s.tick();
        let Some(m) = s.entity(id) else { break };
        if m.attack_phase == royalesim::entity::AttackPhase::Cooldown {
            let shot = s.projectiles().iter().rev().find(|p| p.firer_card == Some(form)).expect("her shot");
            snipes += usize::from(shot.speed / mult == 2650);
        }
    }
    assert_eq!(s.snipes_left().first().map(|(_, k)| *k), Some(0), "spent at the end ({snipes} fired)");
    snipes
}

#[test]
fn a_copys_form_starts_as_its_play_does() {
    assert_eq!(snipes_put(true), 3, "a copy's Evo Musketeer snipes three times (AmmoCount 3)");
    assert_eq!(snipes_put(false), 0, "the control: a setup spawn's stands as if played earlier, its snipes spent");
    // the play's own effects are not cast: a played Evo Cannon's barrage is on the board a tick after it; a copy's
    // spell objects are a setup spawn's (none of the play's)
    let spells = |how: u8| {
        let mut s = battle();
        let at = [(Team::Blue, "Cannon_EV1", n(9000, 12000), None, None)];
        match how {
            0 => s.spawn_unit(Team::Blue, "Cannon_EV1", at[0].2, None).expect("the Cannon"),
            1 => drop(s.scenario_spawn_copy(&at).expect("the Cannon")),
            _ => drop(s.scenario_spawn_batch_levels(&at).expect("the Cannon")),
        }
        (0..3)
            .map(|_| {
                s.tick();
                s.spells().len()
            })
            .max()
            .unwrap_or(0)
    };
    let (played, copy, setup) = (spells(0), spells(1), spells(2));
    assert!(played >= setup + 9, "the control: a played Evo Cannon's nine bombs ({played} against {setup})");
    assert_eq!(copy, setup, "a copy's Evo Cannon casts no barrage");
    // a hero's record its play makes stands with its button; one a press makes is refused, by its kind
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Knight".into(), "Fireball".into()], vec!["Knight".into()]];
    cfg.forms = [vec![2, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let hero = s.scenario_spawn_copy(&[(Team::Blue, "Knight_hero", n(9000, 12000), None, None)]).expect("the Hero Knight")[0];
    let button = s.ability_buttons(Team::Blue)[0];
    assert_eq!((button.hero, button.spent), (Some(hero), false), "the button is on him, its charge unspent");
    let db = s.cards().clone();
    for (unit, kind) in [("MusketeerTurret", "by_press"), ("TombstoneHero_Monster_Active", "by_press"), ("TombstoneHero_Monster_Passive", "by_its_tomb")] {
        let idx = db.index(unit).unwrap_or_else(|| panic!("{unit}"));
        assert_eq!(db.copy_refusal(idx).map(|(k, _)| k), Some(kind), "{unit}");
        assert!(s.scenario_spawn_copy(&[(Team::Blue, unit, n(9000, 14000), None, None)]).is_err(), "{unit} refused");
        assert!(s.clone().scenario_spawn_batch_levels(&[(Team::Blue, unit, n(9000, 14000), None, None)]).is_ok(), "{unit}: a setup spawn takes it");
    }
    assert_eq!(["Knight_hero", "Musketeer_EV1", "SkeletonArmy_EV1", "Knight"].map(|u| db.copy_refusal(db.index(u).expect(u))), [None; 4]);
}

/// The ticks, of 120 from an Evo Tesla's creation, on which a red Knight held 2500 ahead of it loses a whole number of
/// ring hits (148 at level 11 each; the Tesla's own shots take 218 or 220), and that loss: the Tesla put down by a copy
/// or by a setup spawn. Both come up on the first tick, so a ring made then and one made at the creation reach the
/// Knight on the same tick.
fn ring_hits(copy: bool) -> Vec<(u32, i32)> {
    let mut s = battle();
    let at = n(9000, 11000);
    let tesla = [(Team::Blue, "Tesla_EV1", at, None, None)];
    (if copy { s.scenario_spawn_copy(&tesla) } else { s.scenario_spawn_batch_levels(&tesla) }).expect("the Tesla");
    let p = n(9000, 13500);
    let k = s.scenario_spawn_now(Team::Red, "Knight", p, None).expect("a red Knight");
    let top = s.entity(k).expect("the Knight").max_hp;
    let mut hits = Vec::new();
    for f in 0..120 {
        assert!(s.debug_set_pos(k, p) && s.debug_set_hp(k, top));
        s.tick();
        let lost = top - s.entity(k).expect("the Knight held alive").hp;
        if lost > 0 && lost % 148 == 0 {
            hits.push((f, lost));
        }
    }
    hits
}

#[test]
fn a_copys_evolved_unit_is_as_made() {
    // a Spectral: invisible from its first frame, as a soldier's death makes it; a setup spawn's is not
    let invisible = |copy: bool| {
        let mut s = battle();
        let sp = [(Team::Blue, "SkeletonArmy_EV1_Spectral", n(9000, 12000), None, None)];
        let id = if copy { s.scenario_spawn_copy(&sp) } else { s.scenario_spawn_batch_levels(&sp) }.expect("the Spectral")[0];
        s.entity(id).expect("the Spectral").status_flags & 2 != 0
    };
    assert!(invisible(true), "a copy's Spectral is invisible to its enemies");
    assert!(!invisible(false), "the control: a setup spawn's is not");
    // the Evo Tesla: its creation ring is the play's, so a copy's rings once (as it comes up), a setup spawn's twice, its
    // two rings on the Knight together
    assert_eq!((ring_hits(true), ring_hits(false)), (vec![(11, 148)], vec![(11, 296)]), "(copy, setup spawn)");
    // the Evo Lumberjack's ghost lives on its Rage's run, which no copy has; the Hero Balloon's bomb is its death's
    let s = battle();
    let db = s.cards().clone();
    let kind = |u: &str| db.copy_refusal(db.index(u).unwrap_or_else(|| panic!("{u}"))).map(|(k, _)| k);
    assert_eq!(kind("RageBarbarianEvoGhost"), Some("by_death"));
    assert!(s.clone().scenario_spawn_copy(&[(Team::Blue, "RageBarbarianEvoGhost", n(9000, 12000), None, None)]).is_err());
    assert_eq!(kind("BalloonHero_Bomb"), None, "the Hero Balloon's bomb: its death's, as the plain Balloon's is");
}
