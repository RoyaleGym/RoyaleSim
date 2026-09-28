//! THE HERO FORMS: Hero Musketeer and Hero Ice Golem (cards.json `hero_forms`; card.rs `load_hero_forms`; state.rs
//! `resolve_play`, `press_ability_button`, `fire_ability`; spell.rs `SpellMotion::Attached`).
//!
//! A deck entry marked form 2 (`BattleConfig::forms`) plays its hero form on every play, and is one of its side's
//! ability buttons. Button k presses the one charge of the newest living hero of the k-th form-2 entry, for the
//! ability's ManaCost, once that hero has deployed.
//!
//! THE CHECKS:
//!   1. `the_forms_load_after_every_other_slot`: every card, unit and buff of the table loads where it does without
//!      the forms; the forms and their units come after them;
//!   2. `hero_musketeer_plays_and_puts_her_turret_down`: the form every play, the button's refusals and its cost,
//!      the turret 200 ms after the press, 2500 ahead of her, with its hitpoints and its blow;
//!   3. `hero_ice_golem_storm_rides_on_the_golem`: the storm's three flat waves of 27, its slow every tick, the centre
//!      on the golem, the last slow at its end, and a save taken mid-storm that resumes the same battle.
//!
//! A battle with no forms hashes as it did before the forms: tests/hash_continuity.rs.
//!
//! PLANTS: `RUSTFLAGS='--cfg clash_plant="hero_forms_in_first_pass"' CARGO_TARGET_DIR=target/plant cargo test --test
//! hero_forms` -> 1 red (the forms load among the table's rows, so the towers and units after them move).
//! `--cfg clash_plant="hash_heroes_always"` -> tests/hash_continuity.rs red (the button list hashed in every battle).
#![allow(unexpected_cfgs)]

mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, FORM_HERO};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::py::{apply_commands, ids_of_indices, state_json_text};
use royalesim::state::{BattleState, DeployError, HAND_SIZE};
use royalesim::Team;
use std::collections::BTreeMap;

/// Both decks; the first four start in hand (unshuffled).
const DECK: [&str; 8] = ["Musketeer", "IceGolemite", "Knight", "Archer", "Giant", "Valkyrie", "HogRider", "Fireball"];

/// A battle of DECK against DECK with Blue's entries marked `blue_forms`, at the end of the opening lockout, both
/// sides at 10 elixir.
fn battle(blue_forms: Vec<u8>) -> BattleState {
    let mut cfg = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [blue_forms, Vec::new()];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    // Blue's princess towers down, so no tower reaches the scenes (its king reaches none either).
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    s
}

fn elixir_milli(s: &BattleState, team: Team) -> i64 {
    let (m, u) = s.elixir_raw(team);
    m * 1000 / u
}

/// A native offset in subtiles.
fn native(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

#[test]
fn the_forms_load_after_every_other_slot() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json");
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e} (run tools/extract_cards.py)"));
    let full = CardDb::from_json_str(&text, CardSource::DerivedJson).expect("the table loads");
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(doc.as_object_mut().unwrap().remove("hero_forms").is_some(), "the table carries no hero_forms");
    let bare = CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).expect("the table without its forms loads");
    // Every slot and buff the table has without its forms is where it was, record for record.
    let n = bare.cards.len();
    assert!(full.cards.len() > n, "no hero form loaded: {:?}", full.rejected_forms);
    for (k, c) in bare.cards.iter().enumerate() {
        assert_eq!(format!("{c:?}"), format!("{:?}", full.cards[k]), "slot {k} ({}) moved or changed", c.name);
    }
    assert_eq!(bare.buffs[..], full.buffs[..bare.buffs.len()], "a buff index moved");
    assert_eq!(bare.rejected, full.rejected);
    // After them: the two forms, each with its base, and the turret; every one summon-only, so no deck or catalogue
    // names one.
    assert!(full.rejected_forms.is_empty(), "refused forms: {:?}", full.rejected_forms);
    let names: Vec<&str> = full.cards[n..].iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Musketeer_hero", "MusketeerTurret", "IceGolemite_hero"]);
    assert!(full.cards[n..].iter().all(|c| c.summon_only));
    for base in ["Musketeer", "IceGolemite"] {
        let b = full.index(base).unwrap();
        let f = full.form_card(b, FORM_HERO).unwrap_or_else(|| panic!("{base} has no hero form"));
        assert_eq!(full.get(f).form_of, Some(b));
        assert_eq!((full.get(f).elixir, full.get(f).kind), (full.get(b).elixir, full.get(b).kind));
    }
}

#[test]
fn hero_musketeer_plays_and_puts_her_turret_down() {
    let mut s = battle(vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0]);
    let db = s.cards().clone();
    let (mus, form) = (db.index("Musketeer").unwrap(), db.index("Musketeer_hero").unwrap());
    let turret = db.index("MusketeerTurret").unwrap();
    let ability = db.get(form).ability.clone().expect("the form carries its button");
    assert_eq!((ability.cost, ability.cast_ms, ability.trigger_ms), (3, 950, 200));
    // The Python layer's commands: slot HAND_SIZE is button 0, reported under the base card's catalogue id.
    let catalogue: Vec<u16> = DECK.iter().map(|n| db.index(n).unwrap()).collect();
    let ids = ids_of_indices(&db, &catalogue);
    assert_eq!(ids[form as usize], 0, "the form's units report under the Musketeer's catalogue id");
    assert_eq!(ids[turret as usize], 0);
    let press = |s: &mut BattleState, slot: i64| apply_commands(s, &[(0, slot, 0, 0)], &ids).unwrap()[0].1;
    let reason = |name: &str| royalesim::py::DEPLOY_REASONS.iter().position(|r| *r == name).unwrap_or_else(|| panic!("no reason {name}")) as u8;
    assert_eq!(press(&mut s, HAND_SIZE as i64), reason("NO_HERO"), "no hero yet");
    assert_eq!(press(&mut s, HAND_SIZE as i64 + 1), reason("NO_HERO"), "button 1 has no form-2 entry");

    // THE PLAY: the hand card is the Musketeer; the play puts the form down for her 4 elixir and cycles the card.
    let before = elixir_milli(&s, Team::Blue);
    s.deploy(Team::Blue, "Musketeer", t(900, 800)).expect("the play");
    assert_eq!(before - elixir_milli(&s, Team::Blue), 4000);
    assert_eq!(s.queue_cards(Team::Blue).last(), Some(&mus), "the hand card goes to the back of the queue");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Musketeer_hero");
    assert_eq!(hero.len(), 1, "the play put the hero form down");
    let (hid, level) = (hero[0].id, s.config().card_level[0]);
    assert_eq!(hero[0].max_hp, db.scaled(form, level, db.get(form).hitpoints).unwrap());
    assert_eq!(hero[0].status_flags & 16, 16, "status bit 4: a hero unit");
    // While she deploys the button is not ready; once her deploy ends, it is.
    assert_eq!(press(&mut s, HAND_SIZE as i64), reason("ABILITY_NOT_READY"));
    run_until(&mut s, 40, |s| s.entity(hid).is_some_and(|e| e.deploy_ms == 0));
    let at = s.entity(hid).unwrap().pos;
    let before = elixir_milli(&s, Team::Blue);
    assert_eq!(press(&mut s, HAND_SIZE as i64), 0, "the press is taken");
    assert_eq!(before - elixir_milli(&s, Team::Blue), 3000, "the press pays its ManaCost at once");
    assert_eq!(press(&mut s, HAND_SIZE as i64), reason("ABILITY_SPENT"), "one charge");
    // A Red Knight 1000 in front of where the turret goes, deploying when the blow lands.
    let spot = at.add(native(0, 2500));
    s.spawn_unit(Team::Red, "Knight", spot.add(native(0, 1000)), None).unwrap();
    // THE TURRET: 200 ms after the press (4 ticks), 2500 ahead of her along Blue's forward, at her level.
    for k in 1..=4 {
        s.tick();
        let there = find_live(&s, Team::Blue, "MusketeerTurret");
        assert_eq!(there.len(), usize::from(k == 4), "the turret on tick {k} after the press");
        assert_eq!(s.entity(hid).unwrap().pos, at, "the cast holds her");
    }
    let (tpos, tmax, tdeploy) = find_live(&s, Team::Blue, "MusketeerTurret").iter().map(|e| (e.pos, e.max_hp, e.deploy_ms)).next().unwrap();
    assert_eq!(tpos, spot, "2500 ahead of her");
    assert_eq!(tmax, db.scaled(turret, level, db.get(turret).hitpoints).unwrap());
    assert!(tdeploy > 0, "the turret deploys its own DeployTime");
    // ITS BLOW: the Knight's first loss is the turret's knockback projectile, 80 on the turret's ladder.
    let knight = find_live(&s, Team::Red, "Knight")[0].id;
    let hp0 = s.entity(knight).unwrap().hp;
    run_until(&mut s, 30, |s| s.entity(knight).is_some_and(|e| e.hp < hp0));
    let blow = db.scaled(turret, level, 80).unwrap();
    assert_eq!(hp0 - s.entity(knight).unwrap().hp, blow, "the blow");
    // The observation: Blue's one button, spent.
    let json = state_json_text(&s, &db, &ids, &[[0, 1, 2], [0, 1, 2]], &BTreeMap::new()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["players"][0]["abilities"], serde_json::json!([[0, 1, 3]]));
    assert_eq!(v["players"][1]["abilities"], serde_json::json!([]));
}

#[test]
fn hero_ice_golem_storm_rides_on_the_golem() {
    let mut s = battle(vec![0, FORM_HERO, 0, 0, 0, 0, 0, 0]);
    let db = s.cards().clone();
    let form = db.index("IceGolemite_hero").unwrap();
    s.deploy(Team::Blue, "IceGolemite", t(900, 1200)).expect("the play");
    s.tick();
    let gid = find_live(&s, Team::Blue, "IceGolemite_hero")[0].id;
    run_until(&mut s, 40, |s| s.entity(gid).is_some_and(|e| e.deploy_ms == 0));
    // A Red Knight 2500 from the golem, inside the storm's 4000.
    let g = s.entity(gid).unwrap().pos;
    s.spawn_unit(Team::Red, "Knight", g.add(native(2500, 0)), None).unwrap();
    s.tick();
    let knight = find_live(&s, Team::Red, "Knight")[0].id;
    let before = elixir_milli(&s, Team::Blue);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    assert_eq!(before - elixir_milli(&s, Team::Blue), 2000);
    let slowed = |s: &BattleState| -> i32 {
        let e = s.entity(knight).unwrap();
        e.buffs.iter().filter(|b| b.id > 0 && db.buff_names[b.id as usize - 1].contains("IceGolemiteHero_Slow")).map(|b| b.ms).max().unwrap_or(0)
    };
    // L = the first tick after the press (TriggerDelay 0). Three waves of 27, flat at every level, on L, L + 30 and
    // L + 60; the slow on every tick; the centre on the golem.
    let mut drops: Vec<(u32, i32)> = Vec::new();
    let mut ms_after: Vec<i32> = Vec::new();
    let mut saved: Option<Vec<u8>> = None;
    for k in 0..=61u32 {
        let hp = s.entity(knight).unwrap().hp;
        s.tick();
        let now = s.entity(knight).unwrap().hp;
        if now < hp {
            drops.push((k, hp - now));
        }
        ms_after.push(slowed(&s));
        if k < 60 {
            let areas: Vec<Vec2> = s
                .spells()
                .iter()
                .filter(|sp| sp.card == form)
                .map(|sp| match sp.motion {
                    royalesim::spell::SpellMotion::Attached { pos, .. } => pos,
                    _ => panic!("a storm object that is not attached"),
                })
                .collect();
            assert_eq!(areas.len(), 2, "the damage and the slow areas (the inert knockback area is not made)");
            let at = s.entity(gid).unwrap().pos;
            assert!(areas.iter().all(|p| *p == at), "tick {k}: the storm is not on the golem");
        }
        if k == 45 {
            saved = Some(s.save());
        }
    }
    assert_eq!(drops, vec![(0, 27), (30, 27), (60, 27)], "the storm's waves on the Knight");
    assert!(ms_after[..61].iter().all(|ms| *ms > 0), "the Knight is slowed on every tick of the storm");
    assert_eq!(ms_after[61], ms_after[60], "the final area puts the slow on again when the storm ends");
    // A save taken mid-storm resumes the same battle.
    let saved = saved.unwrap();
    let mut resumed = BattleState::load(&saved).expect("the mid-storm save loads");
    let mut again = BattleState::load(&saved).expect("the mid-storm save loads");
    for _ in 0..16 {
        resumed.tick();
        again.tick();
    }
    assert_eq!(resumed.state_hash(), again.state_hash());
    assert_eq!(resumed.tick_count(), s.tick_count());
    assert_eq!(resumed.state_hash(), s.state_hash(), "the resumed battle is the one that ran on");
    assert_eq!(s.press_ability_button(Team::Blue, 0), Err(DeployError::AbilitySpent));
}
