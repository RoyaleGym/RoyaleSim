//! THE HERO FORMS: Hero Musketeer and Hero Ice Golem (cards.json `hero_forms`; card.rs `load_hero_forms`; state.rs
//! `resolve_play`, `press_ability_button`, `fire_ability`; spell.rs `SpellMotion::Attached`).
//!
//! A deck entry marked form 2 (`BattleConfig::forms`) plays its hero form on every play, and is one of its side's
//! ability buttons. Button k presses the one charge of the newest living hero of the k-th form-2 entry, for the
//! ability's ManaCost, from that hero's first frame; the press starts on the hero's first free tick after it (not
//! deploying, not held) and the effect comes TriggerDelay after that start. Every tick below is measured on client
//! 15.535.29 (the oracle's sp-h* scenes), as offsets from the press.
//!
//! THE CHECKS:
//!   1. `the_forms_load_after_every_other_slot`: every card, unit and buff of the table loads where it does without
//!      the forms; the forms and their units come after them;
//!   2. `hero_musketeer_plays_and_puts_her_turret_down`: the form every play, the button's refusals and its cost (a
//!      refused second press debits nothing, S3), the turret on P + 5 (S2), 2500 ahead of her in Blue's arena y,
//!      with its hitpoints and its blow;
//!   3. `hero_ice_golem_storm_rides_on_the_golem`: the storm made on P + 1, its three waves of 27 on the golem's
//!      ladder (69 at level 11) on P + 2, P + 32 and P + 62, on a Minion and on a Knight 4258 away (S10), its slow
//!      every tick, the centre on the golem, the last slow at its end, and a save taken mid-storm that resumes the
//!      same battle;
//!   4. `a_press_while_she_deploys_waits_for_her_deploy_end`: taken and paid on her first frame, the turret 4 ticks
//!      after her deploy end (sp-h4tower);
//!   5. `a_hero_killed_before_her_turret_gets_the_elixir_back` and `the_ice_golem_storm_is_not_refunded_once_made`
//!      (S6);
//!   6. `a_press_under_a_freeze_waits_for_the_thaw` (S7);
//!   7. `the_storm_outlives_the_golem` (S10);
//!   8. `the_hero_card_cycles_and_replays_while_she_lives` (S1, S5, S9);
//!   9. `the_turret_goes_down_in_the_river_and_across_the_bridge` (S4);
//!  10. `the_turret_is_at_the_heros_level` (S2 ABILITY_OBJECT_LEVEL).
//!
//! A battle with no forms hashes as it did before the forms: tests/hash_continuity.rs.
//!
//! PLANTS: `RUSTFLAGS='--cfg clash_plant="hero_forms_in_first_pass"' CARGO_TARGET_DIR=target/plant cargo test --test
//! hero_forms` -> 1 red (the forms load among the table's rows, so the towers and units after them move).
//! `--cfg clash_plant="hash_heroes_always"` -> tests/hash_continuity.rs red (the button list hashed in every battle).
//! `--cfg clash_plant="cast_releases_target"` -> `a_cast_keeps_her_target_and_restarts_her_attack_clock` red (the
//! cast lets her target go for a rescan at its end). `--cfg clash_plant="cast_blinds_target_search"` ->
//! `a_cast_does_not_stop_her_target_search` red (the cast holds her search as a stun does).
#![allow(unexpected_cfgs)]

mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, FORM_HERO};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::py::{apply_commands, ids_of_indices, state_json_text};
use royalesim::state::{BattleConfig, BattleState, DeployError, TapSnap, HAND_SIZE};
use royalesim::{EntityId as EntityIdOf, Team};
use std::collections::BTreeMap;

/// Both decks; the first four start in hand (unshuffled).
const DECK: [&str; 8] = ["Musketeer", "IceGolemite", "Knight", "Archer", "Giant", "Valkyrie", "HogRider", "Fireball"];

/// A battle of DECK against DECK with Blue's entries marked `blue_forms`, at the end of the opening lockout, both
/// sides at 10 elixir.
fn battle(blue_forms: Vec<u8>) -> BattleState {
    battle_on(config(), blue_forms)
}

/// `battle` on `cfg`.
fn battle_on(mut cfg: BattleConfig, blue_forms: Vec<u8>) -> BattleState {
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
    // A press while she deploys is taken (`a_press_while_she_deploys_waits_for_her_deploy_end`); this one waits.
    run_until(&mut s, 40, |s| s.entity(hid).is_some_and(|e| e.deploy_ms == 0));
    let at = s.entity(hid).unwrap().pos;
    let before = elixir_milli(&s, Team::Blue);
    assert_eq!(press(&mut s, HAND_SIZE as i64), 0, "the press is taken");
    assert_eq!(before - elixir_milli(&s, Team::Blue), 3000, "the press pays its ManaCost at once");
    // S3 ABILITY_CHARGES: a second press is refused and costs nothing.
    let paid = elixir_milli(&s, Team::Blue);
    assert_eq!(press(&mut s, HAND_SIZE as i64), reason("ABILITY_SPENT"), "one charge");
    assert_eq!(elixir_milli(&s, Team::Blue), paid, "a refused press debits nothing");
    // A Red Knight 1000 in front of where the turret goes, deploying when the blow lands.
    let spot = at.add(native(0, 2500));
    s.spawn_unit(Team::Red, "Knight", spot.add(native(0, 1000)), None).unwrap();
    // THE TURRET (S2, sp-h2: press issued on 174, paid on 175, turret 179): the press starts on the next tick and
    // the turret comes 200 ms (4 ticks) after that, 5 ticks after the press; 2500 ahead of her in Blue's arena y, at
    // her level.
    for k in 1..=5 {
        s.tick();
        let there = find_live(&s, Team::Blue, "MusketeerTurret");
        assert_eq!(there.len(), usize::from(k == 5), "the turret on tick {k} after the press");
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
    // A Red Knight 2500 from the golem, inside the storm's 4000; a Red Minion near it (the storm hits air) and a
    // second Red Knight 4258 from it, centre to centre (the oracle's Knight at 4258 was hit on the first wave: the
    // storm reaches the victim's edge).
    let g = s.entity(gid).unwrap().pos;
    s.spawn_unit(Team::Red, "Knight", g.add(native(2500, 0)), None).unwrap();
    s.spawn_unit(Team::Red, "Minions", g.add(native(0, -1500)), None).unwrap();
    s.spawn_unit(Team::Red, "Knight", g.add(native(-4258, 0)), None).unwrap();
    s.tick();
    let knights: Vec<(EntityIdOf, i32)> = find_live(&s, Team::Red, "Knight").iter().map(|e| (e.id, e.pos.x)).collect();
    let knight = knights.iter().max_by_key(|k| k.1).unwrap().0;
    let far = knights.iter().min_by_key(|k| k.1).unwrap().0;
    let minion = s.entities().filter(|e| e.team == Team::Red && e.flying).map(|e| e.id).next().expect("a Minion");
    let (m0, f0) = (s.entity(minion).unwrap().hp, s.entity(far).unwrap().hp);
    let before = elixir_milli(&s, Team::Blue);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    assert_eq!(before - elixir_milli(&s, Team::Blue), 2000);
    let slowed = |s: &BattleState| -> i32 {
        let e = s.entity(knight).unwrap();
        e.buffs.iter().filter(|b| b.id > 0 && db.buff_names[b.id as usize - 1].contains("IceGolemiteHero_Slow")).map(|b| b.ms).max().unwrap_or(0)
    };
    // P = the press. The storm is made on P + 1 (k = 0) and acts from P + 2 (k = 1). Three waves of 27 on the golem's
    // ladder (69 at level 11: sp-h10, press 378, waves 380, 410, 440) on k = 1, 31 and 61; the slow on every tick
    // from k = 1; the centre on the golem.
    let wave = db.scaled(form, s.config().card_level[0], 27).unwrap();
    assert_eq!(wave, 69, "a wave at level 11");
    let mut drops: Vec<(u32, i32)> = Vec::new();
    let mut ms_after: Vec<i32> = Vec::new();
    let mut saved: Option<Vec<u8>> = None;
    for k in 0..=62u32 {
        let hp = s.entity(knight).unwrap().hp;
        s.tick();
        let now = s.entity(knight).unwrap().hp;
        if now < hp {
            drops.push((k, hp - now));
        }
        ms_after.push(slowed(&s));
        if k == 1 {
            assert_eq!(m0 - s.entity(minion).unwrap().hp, wave, "the first wave hits the Minion");
            assert_eq!(f0 - s.entity(far).unwrap().hp, wave, "the first wave hits the Knight 4258 away");
        }
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
    assert_eq!(drops, vec![(1, wave), (31, wave), (61, wave)], "the storm's waves on the Knight");
    assert_eq!(ms_after[0], 0, "the storm acts from the tick after it is made");
    assert!(ms_after[1..62].iter().all(|ms| *ms > 0), "the Knight is slowed on every tick of the storm");
    assert_eq!(ms_after[62], ms_after[61], "the final area puts the slow on again when the storm ends");
    // A save taken mid-storm resumes the same battle.
    let saved = saved.unwrap();
    let mut resumed = BattleState::load(&saved).expect("the mid-storm save loads");
    let mut again = BattleState::load(&saved).expect("the mid-storm save loads");
    for _ in 0..17 {
        resumed.tick();
        again.tick();
    }
    assert_eq!(resumed.state_hash(), again.state_hash());
    assert_eq!(resumed.tick_count(), s.tick_count());
    assert_eq!(resumed.state_hash(), s.state_hash(), "the resumed battle is the one that ran on");
    assert_eq!(s.press_ability_button(Team::Blue, 0), Err(DeployError::AbilitySpent));
}

/// A Blue hero of `card`'s form, played at `at` and ticked once: the battle and the hero, on her first frame.
fn hero_on_first_frame(cfg: BattleConfig, forms: Vec<u8>, card: &str, form: &str, at: Vec2) -> (BattleState, EntityIdOf) {
    let mut s = battle_on(cfg, forms);
    s.deploy(Team::Blue, card, at).expect("the play");
    s.tick();
    let hid = find_live(&s, Team::Blue, form)[0].id;
    (s, hid)
}

/// The ticks until `id` is gone, read on a copy of `s`.
fn ticks_until_gone(s: &BattleState, id: EntityIdOf) -> u32 {
    let mut probe = BattleState::load(&s.save()).unwrap();
    for k in 1..=300u32 {
        probe.tick();
        if probe.entity(id).is_none() {
            return k;
        }
    }
    panic!("the scene drifted: it never died");
}

#[test]
fn a_press_while_she_deploys_waits_for_her_deploy_end() {
    // sp-h4tower: her first frame 101, the press issued on 101 and paid at once, her deploy end 121, the turret 125.
    let (mut s, hid) = hero_on_first_frame(config(), vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], "Musketeer", "Musketeer_hero", t(900, 800));
    assert!(s.entity(hid).unwrap().deploy_ms > 0, "she is deploying");
    let b = s.ability_buttons(Team::Blue)[0];
    assert!(b.available && !b.spent, "the button is available from her first frame");
    let before = elixir_milli(&s, Team::Blue);
    s.press_ability_button(Team::Blue, 0).expect("a press while she deploys is taken");
    assert_eq!(before - elixir_milli(&s, Team::Blue), 3000, "and paid at once");
    let (mut deployed, mut turret) = (None, None);
    for k in 1..=40u32 {
        s.tick();
        if deployed.is_none() && s.entity(hid).unwrap().deploy_ms == 0 {
            deployed = Some(k);
        }
        if turret.is_none() && !find_live(&s, Team::Blue, "MusketeerTurret").is_empty() {
            turret = Some(k);
        }
    }
    // Her deploy timer reads 0 after 19 ticks, so the first Status phase that finds her deployed is the 20th tick's
    // (the oracle's deploy end, 121 - 101); the press starts there and the turret comes 4 ticks later (125 - 101).
    assert_eq!((deployed, turret), (Some(19), Some(24)), "her deploy end and the turret, from her first frame");
}

#[test]
fn a_hero_killed_before_her_turret_gets_the_elixir_back() {
    // S6 (sp-h6b): pressed 2 ticks before a Rocket kills her (press 186, gone 189): -3 on the press, +3 on the tick
    // after her death (190), no turret.
    let (mut s, hid) = hero_on_first_frame(config(), vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], "Musketeer", "Musketeer_hero", t(900, 800));
    run_until(&mut s, 40, |s| s.entity(hid).is_some_and(|e| e.deploy_ms == 0));
    let at = s.entity(hid).unwrap().pos;
    s.spawn_unit(Team::Red, "Rocket", at, None).expect("the Rocket");
    let dies = ticks_until_gone(&s, hid);
    assert!(dies > 3, "the Rocket is too quick for the scene ({dies})");
    for _ in 0..dies - 3 {
        s.tick();
    }
    s.scenario_set_elixir_milli(Team::Blue, 5000);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut el = vec![elixir_milli(&s, Team::Blue)];
    for _ in 0..3 {
        s.tick();
        el.push(elixir_milli(&s, Team::Blue));
    }
    assert!(s.entity(hid).is_none(), "she dies on the third tick after the press");
    s.tick();
    el.push(elixir_milli(&s, Team::Blue));
    let regen = el[3] - el[2];
    assert!((regen - (el[2] - el[1])).abs() <= 1, "no refund on the tick she dies: {el:?}");
    assert!((el[4] - el[3] - regen - 3000).abs() <= 1, "the 3 back on the tick after her death: {el:?}");
    for _ in 0..10 {
        s.tick();
        assert!(find_live(&s, Team::Blue, "MusketeerTurret").is_empty(), "no turret");
    }
}

#[test]
fn the_ice_golem_storm_is_not_refunded_once_made() {
    // S6 (sp-h6ib): pressed 1 tick before its death (press 189, gone 191): -2, not refunded: the storm was made on
    // the tick after the press, before the death.
    let (mut s, gid) = hero_on_first_frame(config(), vec![0, FORM_HERO, 0, 0, 0, 0, 0, 0], "IceGolemite", "IceGolemite_hero", t(900, 1200));
    let form = s.cards().index("IceGolemite_hero").unwrap();
    run_until(&mut s, 40, |s| s.entity(gid).is_some_and(|e| e.deploy_ms == 0));
    let at = s.entity(gid).unwrap().pos;
    s.spawn_unit(Team::Red, "Rocket", at, None).expect("the Rocket");
    let dies = ticks_until_gone(&s, gid);
    assert!(dies > 2, "the Rocket is too quick for the scene ({dies})");
    for _ in 0..dies - 2 {
        s.tick();
    }
    s.scenario_set_elixir_milli(Team::Blue, 5000);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut el = vec![elixir_milli(&s, Team::Blue)];
    s.tick();
    el.push(elixir_milli(&s, Team::Blue));
    assert!(s.spells().iter().any(|sp| sp.card == form), "the storm is made on the tick after the press");
    s.tick();
    el.push(elixir_milli(&s, Team::Blue));
    assert!(s.entity(gid).is_none(), "the golem dies on the second tick after the press");
    s.tick();
    el.push(elixir_milli(&s, Team::Blue));
    let regen = el[1] - el[0];
    assert!(el.windows(2).all(|w| (w[1] - w[0] - regen).abs() <= 1), "no refund: {el:?}");
}

#[test]
fn a_press_under_a_freeze_waits_for_the_thaw() {
    // S7 (sp-h7): pressed under a Freeze, taken and paid at once; the turret comes 4 ticks after her first free tick
    // (thaw 183, turret 187).
    let (mut s, hid) = hero_on_first_frame(config(), vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], "Musketeer", "Musketeer_hero", t(900, 800));
    run_until(&mut s, 40, |s| s.entity(hid).is_some_and(|e| e.deploy_ms == 0));
    let at = s.entity(hid).unwrap().pos;
    s.spawn_unit(Team::Red, "Freeze", at, None).expect("the Freeze");
    run_until(&mut s, 60, |s| s.entity(hid).is_some_and(|e| e.stun_ms > 0));
    assert!(s.entity(hid).unwrap().stun_ms > 0, "the scene drifted: the Freeze never held her");
    let before = elixir_milli(&s, Team::Blue);
    s.press_ability_button(Team::Blue, 0).expect("a frozen hero may press");
    assert_eq!(before - elixir_milli(&s, Team::Blue), 3000, "paid at once");
    let (mut free, mut turret) = (None, None);
    for k in 1..=200u32 {
        let held = s.entity(hid).unwrap().stun_ms > 0;
        s.tick();
        // Her first free tick: the first whose Status phase found her unheld (the hold she carried into it is over).
        if free.is_none() && !held {
            free = Some(k);
        }
        if !find_live(&s, Team::Blue, "MusketeerTurret").is_empty() {
            turret = Some(k);
            break;
        }
    }
    let (free, turret) = (free.expect("she thaws"), turret.expect("the turret comes"));
    assert!(free > 10, "the scene drifted: she was free at once ({free})");
    assert_eq!(turret, free + 4, "the turret 4 ticks after her first free tick");
}

#[test]
fn a_cast_keeps_her_target_and_restarts_her_attack_clock() {
    // sp-h2 in miniature (client 15.535.29): she holds a far Knight when pressed and a nearer Knight stands in sight;
    // she comes out of the 950 ms cast still on the far Knight, her attack progress and load timer 0 through the cast,
    // and enters a fresh cycle on it (progress LoadTime + 50 = 350, load 300). The plant cast_releases_target (the lock
    // let go for a rescan at the hold's end) turns this red.
    let (mut s, hid) = hero_on_first_frame(config(), vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], "Musketeer", "Musketeer_hero", t(900, 800));
    run_until(&mut s, 40, |s| s.entity(hid).is_some_and(|e| e.deploy_ms == 0));
    let p = s.entity(hid).unwrap().pos;
    s.spawn_unit(Team::Red, "Knight", p.add(native(0, 5500)), None).expect("the far Knight");
    s.tick();
    let far = find_live(&s, Team::Red, "Knight")[0].id;
    run_until(&mut s, 60, |s| s.entity(hid).is_some_and(|e| e.target == Some(far)));
    assert_eq!(s.entity(hid).unwrap().target, Some(far), "the scene drifted: she never took the far Knight");
    s.spawn_unit(Team::Red, "Knight", p.add(native(-2000, 2500)), None).expect("the near Knight");
    s.tick();
    let near = find_live(&s, Team::Red, "Knight").iter().map(|e| e.id).find(|id| *id != far).expect("the near Knight");
    run_until(&mut s, 40, |s| s.entity(near).is_some_and(|e| e.deploy_ms == 0));
    let dist = |s: &BattleState, id| s.entity(hid).unwrap().pos.dist2(s.entity(id).unwrap().pos);
    assert!(dist(&s, near) < dist(&s, far), "the scene drifted: the near Knight is not the nearer");
    assert_eq!(s.entity(hid).unwrap().target, Some(far), "the scene drifted: she left the far Knight before the press");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let (mut held, mut first_attack) = (0, None);
    for _ in 0..60 {
        s.tick();
        let h = s.entity(hid).unwrap();
        assert_eq!(h.target, Some(far), "she keeps the far Knight through the cast and after it");
        if h.stun_ms > 0 {
            held += 1;
            assert_eq!((h.attack_ms, h.attack_load_ms), (0, 0), "the cast restarts her attack clock");
        } else if held > 0 && h.attack_ms > 0 {
            first_attack = Some((h.attack_ms, h.attack_load_ms));
            break;
        }
    }
    assert!(held > 0, "the scene drifted: the cast never held her");
    assert_eq!(first_attack, Some((350, 300)), "a fresh cycle on the kept target, as the client's");
    assert!(dist(&s, near) < dist(&s, far), "the near Knight was the nearer all along");
}

#[test]
fn a_cast_does_not_stop_her_target_search() {
    // sp-scene-d in miniature (client 15.535.29): her target is lost mid-cast (there on t154, to her turret's blow)
    // and she takes the enemy left in her sight on the next tick (t155), while the cast still holds her, not when the
    // cast ends. The plant cast_blinds_target_search (the cast holds her search as a stun does) turns this red.
    let (mut s, hid) = hero_on_first_frame(config(), vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], "Musketeer", "Musketeer_hero", t(900, 800));
    run_until(&mut s, 40, |s| s.entity(hid).is_some_and(|e| e.deploy_ms == 0));
    let p = s.entity(hid).unwrap().pos;
    let lost = s.scenario_spawn_now(Team::Red, "Knight", p.add(native(-1500, 4000)), Some(1)).expect("the Knight she loses");
    run_until(&mut s, 20, |s| s.entity(hid).is_some_and(|e| e.target == Some(lost)));
    assert_eq!(s.entity(hid).unwrap().target, Some(lost), "the scene drifted: she never took the first Knight");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let left = s.scenario_spawn_now(Team::Red, "Knight", p.add(native(2500, 5000)), None).expect("the Knight left in her sight");
    run_until(&mut s, 5, |s| s.entity(hid).is_some_and(|e| e.stun_ms > 0));
    assert!(s.entity(hid).unwrap().stun_ms > 0, "the scene drifted: the cast never held her");
    let at = s.entity(lost).expect("the scene drifted: the first Knight fell before the cast").pos;
    s.spawn_unit(Team::Blue, "Zap", at, None).expect("the Zap on the first Knight alone");
    let (mut gone, mut taken) = (None, None);
    for k in 1..=30u32 {
        s.tick();
        let h = s.entity(hid).unwrap();
        if h.stun_ms == 0 {
            break;
        }
        if gone.is_none() && s.entity(lost).is_none() {
            gone = Some(k);
        }
        if gone.is_some() && taken.is_none() && h.target == Some(left) {
            taken = Some(k);
        }
    }
    let gone = gone.expect("the scene drifted: the first Knight outlived the cast");
    assert!(s.entity(left).is_some(), "the scene drifted: the Zap reached the other Knight");
    assert_eq!(taken, Some(gone + 1), "she takes the Knight left in her sight on the tick after the loss, still casting");
}

#[test]
fn the_storm_outlives_the_golem() {
    // S10 (sp-h10k): the golem died on P + 52 and the P + 62 wave still landed.
    // The Rocket is aimed at the golem's exact point and the Knight stands 3000 to its side, exact points:
    // placement.TAP_SNAP's old arm, none (the shipped tile-centre snap moves the Rocket's tap and the Knight).
    let mut cfg = config();
    cfg.calib.placement_tap_snap = TapSnap::None;
    let (mut s, gid) = hero_on_first_frame(cfg, vec![0, FORM_HERO, 0, 0, 0, 0, 0, 0], "IceGolemite", "IceGolemite_hero", t(900, 1200));
    run_until(&mut s, 40, |s| s.entity(gid).is_some_and(|e| e.deploy_ms == 0));
    let g = s.entity(gid).unwrap().pos;
    // A Red Knight 3000 to the golem's side: in the storm, out of a Rocket's reach on the golem.
    s.spawn_unit(Team::Red, "Knight", g.add(native(-3000, 0)), None).unwrap();
    s.tick();
    let knight = find_live(&s, Team::Red, "Knight")[0].id;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut drops: Vec<u32> = Vec::new();
    let mut gone = None;
    for k in 0..=61u32 {
        if k == 10 {
            let at = s.entity(gid).unwrap().pos;
            s.spawn_unit(Team::Red, "Rocket", at, None).expect("the Rocket");
        }
        let hp = s.entity(knight).unwrap().hp;
        s.tick();
        if s.entity(knight).unwrap().hp < hp {
            drops.push(k);
        }
        if gone.is_none() && s.entity(gid).is_none() {
            gone = Some(k);
        }
    }
    let gone = gone.expect("the scene drifted: the Rocket did not kill the golem");
    assert!(gone < 61, "the golem died at {gone}");
    assert_eq!(drops.last(), Some(&61), "the last wave lands after the golem's death: {drops:?}");
}

#[test]
fn the_hero_card_cycles_and_replays_while_she_lives() {
    // S1 (every play of a form-2 entry is the hero), S9 (the card cycles as any card, not as a champion) and S5 (a
    // replay with the first hero alive is taken; sp-h5b: played 100, replayed 394 with the first alive).
    let mut s = battle(vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0]);
    s.deploy(Team::Blue, "Musketeer", t(900, 800)).expect("the first play");
    s.tick();
    let first = find_live(&s, Team::Blue, "Musketeer_hero")[0].id;
    // Four fillers bring the Musketeer back: the hand card went to the back of the queue.
    for card in ["Knight", "Archer", "Giant", "Valkyrie"] {
        assert!(!s.hand(Team::Blue).contains(&"Musketeer"), "back in hand before {card}");
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, card, t(300, 500)).unwrap_or_else(|e| panic!("{card}: {e:?}"));
        for _ in 0..4 {
            s.tick();
        }
    }
    assert!(s.hand(Team::Blue).contains(&"Musketeer"), "the hero card is back after four plays");
    assert!(s.entity(first).is_some(), "the first hero lives");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "Musketeer", t(1500, 800)).expect("a replay with the first hero alive");
    s.tick();
    let heroes = find_live(&s, Team::Blue, "Musketeer_hero");
    assert_eq!(heroes.len(), 2, "both heroes, the replay the hero form too");
    let second = heroes.iter().map(|e| e.id).find(|id| *id != first).unwrap();
    assert_eq!(s.ability_buttons(Team::Blue)[0].hero, Some(second), "the button presses the newest hero");
}

/// A Blue Hero Musketeer put down at native (x, y), pressed on her first frame: her point when the turret comes, and
/// the turret's.
fn turret_from(x: i32, y: i32) -> (Vec2, Vec2) {
    let mut s = battle(vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0]);
    s.spawn_unit(Team::Blue, "Musketeer_hero", native(x, y), None).unwrap();
    s.tick();
    let hid = find_live(&s, Team::Blue, "Musketeer_hero")[0].id;
    s.press_ability_button(Team::Blue, 0).expect("the press is never refused for its point");
    for _ in 0..40 {
        let at = s.entity(hid).unwrap().pos;
        s.tick();
        if let Some(t) = find_live(&s, Team::Blue, "MusketeerTurret").first() {
            return (at, t.pos);
        }
    }
    panic!("no turret from ({x}, {y})");
}

/// `turret_from`, in a battle whose Blue princess towers stand (`battle` takes them down).
fn turret_with_towers(x: i32, y: i32) -> (Vec2, Vec2) {
    let mut cfg = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.spawn_unit(Team::Blue, "Musketeer_hero", native(x, y), None).unwrap();
    s.tick();
    let hid = find_live(&s, Team::Blue, "Musketeer_hero")[0].id;
    s.press_ability_button(Team::Blue, 0).expect("the press is never refused for its point");
    for _ in 0..40 {
        let at = s.entity(hid).unwrap().pos;
        s.tick();
        if let Some(t) = find_live(&s, Team::Blue, "MusketeerTurret").first() {
            return (at, t.pos);
        }
    }
    panic!("no turret from ({x}, {y})");
}

#[test]
fn the_turret_is_placed_as_a_building_off_its_own_towers() {
    // S4 grid (sp-h4grid, sp-h4tower and its variants, sp-h4king8500): the point hero + (0, 2500) on an own crown
    // tower is placed as a building twice, the placeholder's and then the turret's, x kept. The princess box's side
    // columns land on y 3000, its centre column on y 2000 (the placeholder, pushed first, blocks the turret's first
    // candidate row), the king's inner columns on y 500; off every box the point stands. The plant
    // ability_point_unvalidated (the point never placed as a building) turns this red.
    for (x, y, lands) in [(2517, 3556, Some(3000)), (4481, 3556, Some(3000)), (3481, 3556, Some(2000)), (8457, 542, Some(500)), (9542, 542, Some(500)), (5481, 3556, None)] {
        let (hero, turret) = turret_with_towers(x, y);
        assert_eq!(turret.x, hero.x, "x kept, from ({x}, {y})");
        let want = lands.map_or(hero.y + 2500 * K, |v| v * K);
        assert_eq!(turret.y, want, "from ({x}, {y}): turret at native y {}", turret.y / K);
    }
}

#[test]
fn the_turret_goes_down_in_the_river_and_across_the_bridge() {
    // S4 (sp-h4river, sp-h4enemy): hero (3274, 13054) gives (3274, 15554); (3269, 15514) gives (3269, 18014).
    for (x, y) in [(3274, 13054), (3269, 15514)] {
        let (hero, turret) = turret_from(x, y);
        assert_eq!(turret, hero.add(native(0, 2500)), "from ({x}, {y})");
    }
}

#[test]
fn the_turret_is_at_the_heros_level() {
    // S2 ABILITY_OBJECT_LEVEL (sp-h2, sp-h2l9): the turret's max_hp is 1536 in a level-11 battle and 1272 in a level-9.
    for (level, hp) in [(11, 1536), (9, 1272)] {
        let mut cfg = config();
        let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
        cfg.decks = [deck.clone(), deck];
        cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
        cfg.card_level = [level, level];
        let mut s = BattleState::try_new(7, cfg).unwrap();
        let lockout = s.config().calib.deploy_lockout_ticks as u32;
        s.scenario_set_tick(lockout);
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "Musketeer", t(900, 800)).expect("the play");
        s.tick();
        s.press_ability_button(Team::Blue, 0).expect("the press");
        let got = (0..40)
            .find_map(|_| {
                s.tick();
                find_live(&s, Team::Blue, "MusketeerTurret").first().map(|e| e.max_hp)
            })
            .expect("the turret");
        assert_eq!(got, hp, "the turret's max_hp at level {level}");
    }
}
