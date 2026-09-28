//! EVOLVED FORMS: Evo Skeletons, Evo Cannon and Evo Musketeer.
//!
//! A deck marks a card evolved (`BattleConfig::forms`, 1 on its entry). Counting only that card's own plays from the
//! battle's start, plays 1 and 2 put the base card down and play 3 its evolved form, and the count starts again
//! (state.rs `EVO_BASIC_PLAYS`, measured on client 15.535.29). A form loads as a card of its own (card.rs `EvoDef`),
//! after every card of the table, so a battle without forms runs and hashes as before (tests/hash_continuity.rs).
//!
//! One scene per form, each at level 11:
//!   - Evo Cannon: its nine bombs land where and when client 15.535.29 put them, 281 each, never twice on one unit;
//!   - Evo Skeletons: every second hit of the play's group makes one more evolved skeleton, up to 8 alive;
//!   - Evo Musketeer: her first three shots at a target far ahead in her lane are snipes, then plain shots.
//!
//! And the invariant that keeps every other battle: the forms take slots after every existing card and buff.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test evolution`):
//!   forms_load_first        the forms load before the spawned units: forms_take_slots_after_every_existing_card
//!                           goes red (and so does tests/hash_continuity.rs).
//!   evo_hashed_when_absent  the evolved units' state is hashed in every battle: tests/hash_continuity.rs goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource};
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{barrage_spells, BattleConfig, BattleState, EVO_BASIC_PLAYS};
use royalesim::Team;

/// A native point, in subtiles.
fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Level 11 for both sides, cards and towers.
fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

/// A battle at level 11 with the clock past the opening lockout.
fn battle(cfg: BattleConfig) -> BattleState {
    let mut s = BattleState::new(7, level11(cfg));
    past_deploy_lockout(&mut s);
    s
}

fn idx(s: &BattleState, name: &str) -> u16 {
    s.cards().index(name).unwrap_or_else(|| panic!("{name} does not load"))
}

#[test]
fn a_deck_card_marked_evolved_plays_its_form_every_third_play() {
    let mut cfg = config();
    cfg.decks = [vec!["Cannon".into(), "Knight".into(), "Mirror".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0, 0], Vec::new()];
    let mut s = battle(cfg);
    let (cannon, form) = (idx(&s, "Cannon"), idx(&s, "Cannon_EV1"));
    // Each play at 10 elixir, then one tick: the card that went down is the newest Blue unit. Each building on a spot
    // of its own, the Knight away from them all.
    let mut spot = 0;
    let mut play = |s: &mut BattleState, card: &str| -> String {
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        let before: Vec<_> = s.entities().map(|e| e.id).collect();
        let at = if card == "Knight" {
            n(2000, 11500)
        } else {
            spot += 1;
            n(3000 + 3000 * (spot % 4 + 1), 10000 + 3000 * (spot / 4))
        };
        s.deploy(Team::Blue, card, at).unwrap_or_else(|e| panic!("{card}: {e:?}"));
        s.tick();
        let new: Vec<String> = s.entities().filter(|e| e.team == Team::Blue && !before.contains(&e.id)).map(|e| e.card.to_string()).collect();
        assert_eq!(new.len(), 1, "{card} put down {new:?}");
        new[0].clone()
    };
    let counter = |s: &BattleState| s.evo_counters(Team::Blue)[0];
    let cost = |s: &BattleState| s.hand_costs(Team::Blue)[s.hand(Team::Blue).iter().position(|c| *c == "Cannon").unwrap()];
    assert_eq!(s.evo_counters(Team::Blue).len(), 1);
    assert_eq!((counter(&s).card, counter(&s).form), (cannon, form));
    assert_eq!(EVO_BASIC_PLAYS, 2);
    // Play 1 (from the opening hand) and play 2 are basic; the Knight between them does not count.
    assert_eq!(play(&mut s, "Cannon"), "Cannon");
    assert_eq!(play(&mut s, "Knight"), "Knight");
    assert_eq!(counter(&s).plays, 1);
    assert_eq!(play(&mut s, "Cannon"), "Cannon");
    assert!(counter(&s).next_evolved(), "two basic plays make the next one evolved");
    assert_eq!(cost(&s), 3, "the evolved play costs the base card's elixir");
    // Play 3 is the form, and the count starts again.
    assert_eq!(play(&mut s, "Cannon"), "Cannon_EV1");
    assert_eq!(counter(&s).plays, 0);
    // A Mirror after the evolved play copies the base card, and does not count as a Cannon play.
    assert_eq!(s.mirror_target(Team::Blue), Some(cannon));
    assert_eq!(play(&mut s, "Mirror"), "Cannon");
    assert_eq!(counter(&s).plays, 0);
    assert!(s.hand(Team::Blue).contains(&"Mirror"));
    // Plays 4 and 5 basic, play 6 evolved.
    assert_eq!(play(&mut s, "Cannon"), "Cannon");
    assert_eq!(play(&mut s, "Cannon"), "Cannon");
    assert_eq!(play(&mut s, "Cannon"), "Cannon_EV1");
    // The evolved unit reports status bit 3; the plain ones do not.
    for e in s.entities().filter(|e| e.team == Team::Blue && e.card.starts_with("Cannon")) {
        assert_eq!(e.status_flags & 8 != 0, e.card == "Cannon_EV1", "{} status {}", e.card, e.status_flags);
    }
    // The counter survives a save and a load.
    let back = BattleState::load(&s.save()).expect("the battle loads");
    assert_eq!(back.evo_counters(Team::Blue), s.evo_counters(Team::Blue));
    assert_eq!(back.state_hash(), s.state_hash());
}

#[test]
fn evo_cannon_drops_its_barrage() {
    let mut s = battle(config());
    let form = idx(&s, "Cannon_EV1");
    // Three Golems on the far row, deploying (3000 ms) through the whole barrage, far from every other attack:
    //   (7000, 18000) 2000 from the 9000 bomb (lands I + 26) and from the 5000 bomb (I + 28): hit once, on I + 26;
    //   (13000, 18000) under the 13000 bomb (I + 28);
    //   (17000, 18000) under the 17000 bomb (I + 30).
    let golems = [(n(7000, 18000), 26), (n(13000, 18000), 28), (n(17000, 18000), 30)];
    let play = s.tick_count();
    s.spawn_unit(Team::Blue, "Cannon_EV1", n(9000, 9500), None).unwrap();
    for (at, _) in golems {
        s.spawn_unit(Team::Red, "Golem", at, None).unwrap();
    }
    s.tick();
    // The nine bombs: x absolute, y the cannon's plus 1500 and 8500 toward the enemy.
    let mut bombs: Vec<(i32, i32)> = s.spells().iter().filter(|sp| sp.card == form).map(|sp| match sp.motion {
        royalesim::spell::SpellMotion::Flight { pos, .. } => (pos.x / K, pos.y / K),
        ref m => panic!("a bomb in motion {m:?}"),
    }).collect();
    bombs.sort_unstable();
    let mut want: Vec<(i32, i32)> = [1500, 6500, 11500, 16500].iter().map(|x| (*x, 11000)).chain([1000, 5000, 9000, 13000, 17000].iter().map(|x| (*x, 18000))).collect();
    want.sort_unstable();
    assert_eq!(bombs, want);
    // Side 1's barrage: the same x, the y toward side 0.
    let def = s.cards().get(form).evo.as_ref().unwrap().barrage.as_ref().unwrap();
    let red: Vec<i32> = barrage_spells(s.cards(), &s.config().calib, def, Team::Red, form, 11, n(9000, 22500)).iter().map(|sp| match sp.motion {
        royalesim::spell::SpellMotion::Flight { pos, .. } => pos.y / K,
        _ => unreachable!(),
    }).collect();
    assert!(red.iter().all(|y| *y == 21000 || *y == 14000), "{red:?}");
    // Each Golem loses 281 once, on the tick measured for its bomb.
    let ids: Vec<_> = golems.iter().map(|(at, _)| s.entities().filter(|e| e.card == "Golem").min_by_key(|e| e.pos.dist2(*at)).expect("a Golem").id).collect();
    let full: Vec<i32> = ids.iter().map(|id| s.entity(*id).unwrap().hp).collect();
    let mut lost: Vec<Vec<(u32, i32)>> = vec![Vec::new(); 3];
    let mut last = full.clone();
    while s.tick_count() < play + 40 {
        s.tick();
        for (k, id) in ids.iter().enumerate() {
            let e = s.entity(*id).expect("the Golem lives");
            if k == 0 && s.tick_count() == play + 26 {
                assert!(e.deploying, "the scene needs the Golems standing");
            }
            if e.hp != last[k] {
                lost[k].push((s.tick_count() - play, last[k] - e.hp));
                last[k] = e.hp;
            }
        }
    }
    for (k, (_, tick)) in golems.iter().enumerate() {
        assert_eq!(lost[k], vec![(*tick, 281)], "Golem {k}");
    }
}

#[test]
fn evo_skeletons_copy_on_every_second_group_hit() {
    let mut s = battle(config());
    let form = idx(&s, "Skeletons_EV1");
    s.spawn_unit(Team::Red, "Giant", n(9000, 13500), None).unwrap();
    s.spawn_unit(Team::Blue, "Skeletons_EV1", n(9000, 12000), None).unwrap();
    let skeletons = |s: &BattleState| s.entities().filter(|e| e.team == Team::Blue && e.card_idx == form).count() as u32;
    let mut hits = 0u32;
    let mut first_copy: Option<(Vec2, Vec<Vec2>)> = None;
    let mut capped_hits = 0;
    for _ in 0..400 {
        let before: Vec<_> = s.entities().filter(|e| e.card_idx == form).map(|e| (e.id, e.pos)).collect();
        s.tick();
        let hitters: Vec<_> = s.entities().filter(|e| e.card_idx == form && e.attack_phase == AttackPhase::Cooldown).map(|e| e.id).collect();
        hits += hitters.len() as u32;
        let alive = skeletons(&s);
        assert_eq!(alive, (3 + hits / 2).min(8), "after {hits} group hits");
        for e in s.entities().filter(|e| e.card_idx == form) {
            assert_eq!(e.status_flags & 8, 8, "an evolved skeleton reports status bit 3");
            if !before.iter().any(|(id, _)| *id == e.id) && s.tick_count() > 1 && hits >= 2 {
                assert_eq!(e.hp, e.max_hp, "a copy starts at full hp");
                assert!(e.target.is_some(), "a copy has a target on its first frame");
                if first_copy.is_none() {
                    let from: Vec<Vec2> = before.iter().filter(|(id, _)| hitters.contains(id)).map(|(_, p)| *p).collect();
                    first_copy = Some((e.pos, from));
                }
            }
        }
        if alive == 8 {
            capped_hits += hitters.len();
        }
        if capped_hits >= 4 {
            break;
        }
    }
    assert!(capped_hits >= 4, "the group never held 8 through four more hits ({hits} hits)");
    // The first copy stands 1000 ahead (toward side 1) of the skeleton whose hit made it.
    let (at, from) = first_copy.expect("a copy was made");
    assert!(from.iter().any(|p| *p == Vec2::new(at.x, at.y - 1000 * K)), "copy at {at:?}, hitters at {from:?}");
}

#[test]
fn evo_musketeer_spends_three_snipes() {
    let mut s = battle(config());
    let form = idx(&s, "Musketeer_EV1");
    let mult = s.config().calib.projectile_speed_to_subtiles_per_tick;
    // A Golem 12000 ahead of her in her lane, deploying 3000 ms: far past her Range of 6000.
    s.spawn_unit(Team::Blue, "Musketeer_EV1", n(3500, 9000), None).unwrap();
    s.spawn_unit(Team::Red, "Golem", n(3500, 21000), None).unwrap();
    s.tick();
    let me = s.entities().find(|e| e.card_idx == form).unwrap().id;
    let golem = s.entities().find(|e| e.card == "Golem").unwrap().id;
    let mut shots: Vec<(i32, i32, bool)> = Vec::new();
    for _ in 0..800 {
        s.tick();
        let m = s.entity(me).expect("she lives");
        if m.attack_phase == AttackPhase::Cooldown {
            let p = s.projectiles().iter().rev().find(|p| p.firer_card == Some(form)).expect("her shot");
            shots.push((p.speed / mult, p.damage, p.target == golem));
        }
        if shots.len() >= 4 {
            break;
        }
    }
    assert!(shots.len() >= 4, "{shots:?}");
    // Three snipes at the Golem: Speed 2650, 153 x 256 % = 391 at level 11.
    assert_eq!(&shots[..3], &[(2650, 391, true); 3]);
    // Then her plain shot: Speed 1000, 85 x 256 % = 217.
    assert_eq!((shots[3].0, shots[3].1), (1000, 217));
}

/// The table with the top-level lists `lists` removed.
fn without(lists: &[&str]) -> CardDb {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json");
    let mut doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for list in lists {
        doc.as_object_mut().unwrap().remove(*list);
    }
    CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).unwrap()
}

#[test]
fn forms_take_slots_after_every_existing_card() {
    // The table with its evolved forms against the table without them, both without the hero forms: every card of
    // the parent build, and nothing else. The hero pass loads after the evolved forms (tests/hero_forms.rs holds it),
    // and may add its row's name to a buff the table already has, which is why it is left out here.
    let (db, db0) = (without(&["hero_forms"]), without(&["evolutions", "hero_forms"]));
    assert_eq!(db.rejected_evolutions, Vec::<(String, String)>::new());
    assert_eq!(db0.forms.len(), 0);
    let n0 = db0.cards.len();
    let names = |d: &CardDb, n: usize| d.cards[..n].iter().map(|c| format!("{c:?}")).collect::<Vec<_>>();
    assert_eq!(names(&db, n0), names(&db0, n0), "a card slot of the table without forms moved");
    assert_eq!(db.buffs[..db0.buffs.len()], db0.buffs[..], "a buff slot moved");
    assert_eq!(db.buff_names[..db0.buff_names.len()], db0.buff_names[..], "a buff's names moved");
    let forms: Vec<(String, String)> = db.forms.iter().map(|(b, _, f)| (db.get(*b).name.clone(), db.get(*f).name.clone())).collect();
    assert_eq!(
        forms,
        [("Skeletons", "Skeletons_EV1"), ("Cannon", "Cannon_EV1"), ("Musketeer", "Musketeer_EV1")].map(|(a, b)| (a.to_string(), b.to_string()))
    );
    assert!(db.forms.iter().all(|(_, _, f)| *f as usize >= n0));
    assert_eq!(db.cards.len(), n0 + 3);
    // The whole table: the hero pass leaves every one of those slots where it was, the forms included, and loads only
    // after them.
    let full = cards();
    let n = db.cards.len();
    assert_eq!(names(&full, n), names(&db, n), "the hero pass moved a slot");
    assert_eq!(full.forms, db.forms);
    assert!((n..full.cards.len()).all(|k| full.is_hero_record(k as u16)), "a slot after the evolved forms that the hero pass did not load");
}
