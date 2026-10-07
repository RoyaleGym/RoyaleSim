//! THE VINES: a striking area whose strikes are an action (card.rs `StrikePick::RankedCatches`, `strike_area_shape`;
//! spell.rs `selector_candidates`, `catch`, `deliver`; entity.rs `in_air`; state.rs `apply_effects`).
//!
//! THE LAW, measured on client 15.535.29 (the Vines runs with their controls, level 11), C the cast tick (k = 0
//! below, the first tick run after the play), C' a catch's tick:
//!   - three catches, on C + 18, C + 19 and C + 21 (the start delay 900 plus the Delays 0, 50 and 150, on the striking
//!     area's clock under spells.STRIKE_TIMER_LEFTOVER = carried);
//!   - each takes the enemy in reach with the highest current hp plus shield that the cast has not caught (a crown
//!     tower 3052 before a Mortar before a Cannon; a Dark Prince of 1200 + 240 shield before an Elite Barbarian of
//!     1341), picked again at each catch from the units in reach then (spells.MULTI_CATCH_RANKING = repick_each_catch);
//!   - the snare holds the victim from C' + 1 for its 2000 ms and pulses 153 on C' + 20 and C' + 40, 35 a pulse on a
//!     crown tower (its CrownTowerDamagePerHit, level scaled);
//!   - a caught flier is a ground unit until C' + 42: a Knight, which attacks ground only, targeted a caught Balloon
//!     and let it go on C' + 43 (spells.AIR_TO_GROUND_WINDOW = total_plus_both_transitions);
//!   - an idle Tesla hidden under ground is caught, held and pulsed twice, and stays hidden (its filter does not set
//!     FilterHidden: spells.TARGET_FILTER_ABSENT_FLAG).
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the Vines as ranked catches of the rows' numbers, one snare for the seven size options, and
//!      refuses a size select whose buffs differ and a filter flag it does not run;
//!   2. a crown tower, a Mortar and a Cannon in reach are caught on C + 18, 19 and 21, in that order;
//!   3. each loses its snare's pulse on C' + 20 and C' + 40 (the tower its crown-tower figure);
//!   4. a Dark Prince's shield counts: it is caught before a Knight with more hp than it has alone;
//!   5. a unit that appears in reach after the first catch is caught by the third;
//!   6. a caught Balloon is a ground unit's target from C' + 1 through C' + 42 and not on C' + 43; under
//!      total_duration it is let go on C' + 41;
//!   7. a hidden Tesla is caught, held and pulsed twice, and stays hidden;
//!   8. a caught flier's window and a snare that reaches a hidden building are state: a save edited only in either
//!      fails the load's hash self-check, and an unedited save resumes the battle hash for hash.
//!
//! Losses are read against a control battle run without the cast wherever a building's lifetime drains it.
//!
//! No measurement on disk: two Vines on one unit (status.BUFF_STACKING's named gap), collisions and ground splash on a
//! grounded flier, a vined king, a vined mount's rider (the snare's AttachedInheritAs is carried and not read), the
//! reach on a building (the troop law serves every kind; a Mortar was caught 62 beyond it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test vines`):
//!   * `strike_timer_restarts` (tests/strikes.rs's) -- the leftover dropped: catches on C + 18, 20 and 23: (2) and (3)
//!     go red.
//!   * `vines_delays_cumulative` -- the Delays read as gaps: catches on C + 18, 19 and 22: (2) and (3) go red.
//!   * `vines_rank_ignores_shield` -- the rank leaves the shield out: (4) goes red.
//!   * `vines_ranked_once` -- no unit that was not on the board at the first catch is caught later: (5) goes red.
//!   * `crown_per_hit_ignored` (tests/curse.rs's) -- a crown tower takes the percent route: (3) goes red.
//!   * `grounding_ignored` -- a caught flier stays in the air: (6) goes red, and its status bit 9 never shows.
//!   * `vines_skips_hidden` -- a blank FilterHidden read as set: (1) and (7) go red.
//!   * `hash_skips_grounded`, `hash_skips_reach_hidden` -- the window, the slot's reach not hashed: (8) goes red.
//!   * `vines_pulse_unread` -- the 16.402 Vines (a pulsing area whose pulses are the catches) refused: (9) goes red.
//!
//!   9. THE 16.402 VINES (option B item 26): HitSpeedOffset 900, HitSpeed 250, MaximumTargets 1, HitBiggestTargets,
//!      LifeDuration 1400, its block's offsets [0, 250, 500] and mode HitBiggestTargets (tools/extract_cards.py
//!      `ranked_catches_16402`), edited onto the shipped row: it loads as catches 900, 250 and 250 apart and catches the
//!      tower, the Mortar and the Cannon on C + 18, 23 and 28, as the client (Oracle, 16.402) caught a Knight, a Musketeer and an
//!      Archer (2026-10-07, sp-vines-3-s0, pack and 10-06 content). A pulse that disagrees with the block is refused.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, SpellShape, StrikePick};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AirToGroundWindow, BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn centre_distance(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// The shipped config, asserting the arm this file pins.
fn shipped() -> BattleConfig {
    let c = config();
    assert_eq!(c.calib.air_to_ground_window, AirToGroundWindow::TotalPlusBothTransitions, "the shipped spells.AIR_TO_GROUND_WINDOW this file pins");
    c
}

/// The snare's pulse at the Blue side's level: (on a troop or a building, on a crown tower), from the loaded buff.
fn snare(s: &BattleState) -> (i32, i32) {
    let db = s.cards();
    let idx = db.index("Vines").expect("Vines loads");
    let level = s.config().card_level[0];
    let SpellShape::Strikes(d) = &db.get(idx).spell.as_ref().expect("Vines is a spell").shape else { panic!("Vines is not a striking area") };
    let b = d.selector.as_ref().expect("the Vines' selector").buffs[0];
    let def = db.buffs[b.buff as usize];
    let pulse = def.pulse_amount(s.config().calib.buff_pulse_amount, |m| db.scaled(idx, level, m)).expect("a valid level");
    (pulse, db.scaled(idx, level, def.crown_hit).expect("a valid level"))
}

/// The first tick k after which each of `ids` holds a hold timer, in a battle where Blue casts the Vines at `tap` (C =
/// k = 0), run for `ticks` ticks.
fn first_held(s: &mut BattleState, ids: &[EntityId], tap: Vec2, ticks: u32) -> Vec<Option<u32>> {
    s.spawn_unit(Team::Blue, "Vines", tap, None).expect("cast Vines");
    let mut out = vec![None; ids.len()];
    for k in 0..ticks {
        s.tick();
        for (n, &id) in ids.iter().enumerate() {
            if out[n].is_none() && s.entity(id).is_some_and(|v| v.stun_ms > 0) {
                out[n] = Some(k);
            }
        }
    }
    out
}

/// The crown tower, Mortar and Cannon scene (the Red left princess tower and two buildings before it), set up on `s`:
/// their ids, and the tap that reaches all three.
fn three_buildings(s: &mut BattleState) -> (Vec<EntityId>, Vec2) {
    let tower = s.tower_ids(Team::Red)[1].expect("the Red left princess tower");
    let p = s.entity(tower).unwrap().pos;
    let mortar = s.scenario_spawn_now(Team::Red, "Mortar", Vec2::new(p.x + 1000 * K, p.y - 3000 * K), None).expect("spawn Mortar");
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", Vec2::new(p.x - 1000 * K, p.y - 3000 * K), None).expect("spawn Cannon");
    (vec![tower, mortar, cannon], Vec2::new(p.x, p.y - 2000 * K))
}

// ---------------------------------------------------------------------------
// (1)

/// Ok when `card` loads from `text`, else the reason it is refused.
fn refusal(text: &str, card: &str) -> Result<(), String> {
    let db = CardDb::from_json_str(text, CardSource::DerivedJson).map_err(|e| format!("the file does not parse: {e}"))?;
    match db.rejected.iter().find(|(n, _)| n == card) {
        Some((_, why)) => Err(why.clone()),
        None if db.index(card).is_some() => Ok(()),
        None => Err(format!("{card} neither loads nor is refused")),
    }
}

/// The Vines' own card row from cards.json, with `edit` applied to its striking-area block, as a one-card file.
fn edited_vines(edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("cards.json parses");
    let mut row = doc["cards"].as_array().unwrap().iter().find(|c| c["name"] == "Vines").expect("the Vines row").clone();
    edit(&mut row["spell"]["area_effect_object"]["strike_area"]);
    serde_json::json!({ "cards": [row] }).to_string()
}

/// Plant: vines_skips_hidden.
#[test]
fn the_vines_load_as_ranked_catches_of_the_rows() {
    let s = BattleState::new(0, shipped());
    let spell = card_stat(&s, "Vines").spell.as_ref().expect("Vines loads as a spell");
    let SpellShape::Strikes(d) = &spell.shape else { panic!("Vines: {:?}", spell.shape) };
    assert_eq!((d.pick, d.life_ms, d.gaps_ms.clone()), (StrikePick::RankedCatches, 2000, vec![900, 50, 100]));
    assert_eq!((d.hit.radius, d.hit.damage), (2500 * K, 0), "the shape's circle, and no damage of the area's own");
    let sel = d.selector.as_ref().expect("the Vines' selector");
    assert_eq!(sel.buffs.len(), 1, "the seven size options are one snare");
    let b = sel.buffs[0];
    let def = s.cards().buffs[b.buff as usize];
    assert_eq!((def.speed_pct, def.hit_speed_pct, def.spawn_speed_pct), (-100, -100, -100), "the snare holds");
    assert_eq!((def.damage_per_second, def.hit_frequency_ms, def.crown_hit, b.time_ms), (60, 1000, 14, 2000));
    let g = sel.ground.expect("the catch's air-to-ground action");
    assert_eq!((g.transition_ms, g.total_ms), (50, 2000));
    let f = sel.filter;
    assert!(!f.skip_hidden, "enemy_troops_for_vines leaves FilterHidden out: a hidden building is caught");
    assert!(f.skip_underground && f.skip_dash_immune && f.skip_untargetable, "{f:?}");
    assert!(f.buildings && f.princess_towers && f.king_tower, "{f:?}");
    if s.config().card_level[0] == 11 {
        assert_eq!(snare(&s), (153, 35), "the measured figures at level 11");
    }
    // The shapes it refuses, on the Vines' own row.
    assert_eq!(refusal(&edited_vines(|_| {}), "Vines"), Ok(()), "the Vines' row must load on its own");
    let why = refusal(&edited_vines(|sa| sa["options"][3]["speed_multiplier_raw"] = serde_json::Value::from(-50)), "Vines").unwrap_err();
    assert!(why.contains("a size select whose buffs differ"), "one size option slower than the rest: {why}");
    let why = refusal(&edited_vines(|sa| sa["filter"]["filter_flying"] = serde_json::Value::Bool(true)), "Vines").unwrap_err();
    assert!(why.contains("FilterFlying is not simulated"), "a filter flag the engine does not run: {why}");
    let why = refusal(&edited_vines(|sa| sa["catch_offsets_ms"] = serde_json::json!([0, 150, 50])), "Vines").unwrap_err();
    assert!(why.contains("not ascending"), "catch offsets out of order: {why}");
}

// ---------------------------------------------------------------------------
// (2)

/// Plants: strike_timer_restarts, vines_delays_cumulative.
#[test]
fn a_tower_a_mortar_and_a_cannon_are_caught_on_c_plus_18_19_and_21() {
    let mut s = BattleState::new(0, shipped());
    let (ids, tap) = three_buildings(&mut s);
    let hp: Vec<i32> = ids.iter().map(|&id| s.entity(id).unwrap().hp).collect();
    assert!(hp[0] > hp[1] && hp[1] > hp[2], "the scene drifted: the hp order is not tower, Mortar, Cannon: {hp:?}");
    for &id in &ids {
        let d = centre_distance(s.entity(id).unwrap().pos, tap);
        let r = s.entity(id).unwrap().radius / K;
        assert!(d <= (2500 + r) as i64, "the scene drifted: a building {d} from the tap is out of reach");
    }
    assert_eq!(first_held(&mut s, &ids, tap, 30), vec![Some(18), Some(19), Some(21)], "the first held tick of the tower, the Mortar, the Cannon");
}

// ---------------------------------------------------------------------------
// (3)

/// Plants: crown_per_hit_ignored, strike_timer_restarts, vines_delays_cumulative.
#[test]
fn each_catch_pulses_twice_twenty_ticks_apart() {
    let mut cast = BattleState::new(0, shipped());
    let mut control = BattleState::new(0, shipped());
    let (ids, tap) = three_buildings(&mut cast);
    assert_eq!(three_buildings(&mut control).0, ids, "the two battles set up different entities");
    cast.spawn_unit(Team::Blue, "Vines", tap, None).expect("cast Vines");
    let hp = |s: &BattleState, id: EntityId| s.entity(id).map_or(0, |v| v.hp);
    let mut got: Vec<Vec<(u32, i32)>> = vec![Vec::new(); ids.len()];
    for k in 0..65u32 {
        let before: Vec<(i32, i32)> = ids.iter().map(|&id| (hp(&cast, id), hp(&control, id))).collect();
        cast.tick();
        control.tick();
        for (n, (&id, (bc, bk))) in ids.iter().zip(before).enumerate() {
            let extra = (bc - hp(&cast, id)) - (bk - hp(&control, id));
            if extra != 0 {
                got[n].push((k, extra));
            }
        }
    }
    let (pulse, crown) = snare(&cast);
    assert_eq!(got[0], vec![(38, crown), (58, crown)], "the tower, caught on C + 18");
    assert_eq!(got[1], vec![(39, pulse), (59, pulse)], "the Mortar, caught on C + 19");
    assert_eq!(got[2], vec![(41, pulse), (61, pulse)], "the Cannon, caught on C + 21");
}

// ---------------------------------------------------------------------------
// (4)

/// Plant: vines_rank_ignores_shield.
#[test]
fn a_shield_counts_in_the_rank() {
    let mut s = BattleState::new(0, shipped());
    // Two Red troops walking the same way from beside the tap: 18 ticks of walk leaves both well inside.
    let dp = s.scenario_spawn_now(Team::Red, "DarkPrince", at((8700, 22500)), Some(1200)).expect("spawn DarkPrince");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at((9300, 22500)), Some(1341)).expect("spawn Knight");
    let shield = s.entity(dp).unwrap().shield;
    assert!(shield > 141, "the scene drifted: the Dark Prince's shield ({shield}) does not lift it above the Knight's 1341");
    let got = first_held(&mut s, &[dp, knight], at((9000, 21500)), 20);
    assert_eq!(got, vec![Some(18), Some(19)], "1200 + {shield} is caught before 1341");
}

// ---------------------------------------------------------------------------
// (5)

/// Plant: vines_ranked_once.
#[test]
fn a_unit_that_appears_after_the_first_catch_is_caught_by_the_third() {
    let tap = at((9000, 21500));
    let mut s = BattleState::new(0, shipped());
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Giant", at((8700, 22000)), None), (Team::Red, "Knight", at((9300, 22000)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    s.spawn_unit(Team::Blue, "Vines", tap, None).expect("cast Vines");
    let mut held: Vec<(u32, String)> = Vec::new();
    let mut seen: Vec<EntityId> = Vec::new();
    for k in 0..23u32 {
        if k == 19 {
            // Enqueued before tick 19: the Skeletons appear in its Spawn phase, after the first catch, and stand
            // deploying at the tap.
            s.spawn_unit(Team::Red, "Skeletons", tap, None).expect("deploy Skeletons");
        }
        s.tick();
        for v in s.entities().filter(|v| v.team == Team::Red && v.stun_ms > 0) {
            if !seen.contains(&v.id) {
                seen.push(v.id);
                held.push((k, v.card.to_string()));
            }
        }
    }
    assert!(s.entity(ids[0]).is_some() && s.entity(ids[1]).is_some(), "the scene drifted: the Giant or the Knight is gone");
    assert_eq!(held, vec![(18, "Giant".to_string()), (19, "Knight".to_string()), (21, "Skeletons".to_string())], "the catches (tick, card)");
}

// ---------------------------------------------------------------------------
// (6)

/// The Balloon scene over the river's middle, out of every crown tower's reach: a Red Balloon over the water, a Blue
/// Knight on the bank below it, the Vines at the Balloon. The ticks on which the Knight's target is the Balloon.
fn balloon_targeted(cfg: BattleConfig) -> (Vec<u32>, EntityId, BattleState) {
    let mut s = BattleState::new(0, cfg);
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Balloon", at((9000, 15600)), None), (Team::Blue, "Knight", at((9000, 14300)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (balloon, knight) = (ids[0], ids[1]);
    s.spawn_unit(Team::Blue, "Vines", at((9000, 15600)), None).expect("cast Vines");
    let mut on = Vec::new();
    for k in 0..64u32 {
        s.tick();
        if s.entity(knight).is_some_and(|v| v.target == Some(balloon)) {
            on.push(k);
        }
    }
    (on, balloon, s)
}

/// Plant: grounding_ignored.
#[test]
fn a_caught_balloon_is_a_ground_target_until_c_plus_60() {
    let (on, balloon, s) = balloon_targeted(shipped());
    assert!(s.entity(balloon).is_some(), "the scene drifted: the Balloon died inside the run");
    assert!(!on.is_empty(), "the Knight never targeted the caught Balloon");
    assert!(on.iter().all(|&k| k > 18), "the Knight targeted the Balloon before the catch: {on:?}");
    assert_eq!(on.last(), Some(&60), "a Balloon caught on C + 18 is a ground target through C' + 42: {on:?}");
    // The other arm: TotalDuration alone lets it go two ticks sooner.
    let mut c = config();
    c.calib.air_to_ground_window = AirToGroundWindow::TotalDuration;
    let (on, _, _) = balloon_targeted(c);
    assert_eq!(on.last(), Some(&58), "under total_duration, through C' + 40: {on:?}");
}

/// Plant: grounding_ignored. The export's status bit 9 (512) is the same predicate (entity.rs `in_air`): set on the
/// caught Balloon exactly while its window runs, and not before the catch.
#[test]
fn a_caught_balloon_reports_status_bit_9_while_it_is_held() {
    let mut s = BattleState::new(0, shipped());
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Balloon", at((9000, 15600)), None), (Team::Blue, "Knight", at((9000, 14300)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    s.spawn_unit(Team::Blue, "Vines", at((9000, 15600)), None).expect("cast Vines");
    let mut held = Vec::new();
    for k in 0..64u32 {
        s.tick();
        let v = s.entity(ids[0]).expect("the scene drifted: the Balloon died inside the run");
        assert!(v.flying, "the Balloon is a flier throughout");
        assert_eq!(v.status_flags & 512 != 0, v.grounded_ms > 0, "tick {k}: bit 9 is the hold (window {})", v.grounded_ms);
        if v.status_flags & 512 != 0 {
            held.push(k);
        }
    }
    assert!(held.first().is_some_and(|&k| k >= 18) && held.len() >= 30, "held on the ticks {held:?}");
    assert_eq!(held.last().copied(), Some(held[0] + held.len() as u32 - 1), "one unbroken hold: {held:?}");
}

// ---------------------------------------------------------------------------
// (7)

/// Plant: vines_skips_hidden.
#[test]
fn a_hidden_tesla_is_caught_held_and_pulsed_and_stays_hidden() {
    let tap = at((9000, 22000));
    let mut cast = BattleState::new(0, shipped());
    let mut control = BattleState::new(0, shipped());
    let tesla = cast.scenario_spawn_now(Team::Red, "Tesla", tap, None).expect("spawn Tesla");
    assert_eq!(control.scenario_spawn_now(Team::Red, "Tesla", tap, None).expect("spawn Tesla"), tesla);
    cast.spawn_unit(Team::Blue, "Vines", tap, None).expect("cast Vines");
    let hp = |s: &BattleState| s.entity(tesla).map_or(0, |v| v.hp);
    let (mut losses, mut held_from, mut surfaced) = (Vec::new(), None, None);
    for k in 0..62u32 {
        let (bc, bk) = (hp(&cast), hp(&control));
        cast.tick();
        control.tick();
        let extra = (bc - hp(&cast)) - (bk - hp(&control));
        if extra != 0 {
            losses.push((k, extra));
        }
        let v = cast.entity(tesla).expect("the Tesla lives");
        if held_from.is_none() && v.stun_ms > 0 {
            held_from = Some(k);
        }
        if !v.hidden && surfaced.is_none() {
            surfaced = Some(k);
        }
    }
    assert_eq!(surfaced, None, "the Tesla came up");
    assert_eq!(held_from, Some(18), "the hidden Tesla, the only enemy in reach, is caught on C + 18");
    let (pulse, _) = snare(&cast);
    assert_eq!(losses, vec![(38, pulse), (58, pulse)], "both pulses land on it under ground");
}

// ---------------------------------------------------------------------------
// (8)

/// Plants: hash_skips_grounded, hash_skips_reach_hidden.
#[test]
fn a_grounded_flier_and_a_snare_that_reaches_a_hidden_building_are_state() {
    // The Balloon scene, saved while the caught Balloon is on the ground.
    let mut s = BattleState::new(0, shipped());
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Balloon", at((9000, 15600)), None), (Team::Blue, "Knight", at((9000, 14300)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let balloon = ids[0].index as usize;
    s.spawn_unit(Team::Blue, "Vines", at((9000, 15600)), None).expect("cast Vines");
    for _ in 0..30 {
        s.tick();
    }
    assert!(s.entity(ids[0]).is_some_and(|v| v.grounded_ms > 0), "the scene drifted: the Balloon is not grounded on C + 29");
    let hashed = edit_is_hashed(&s, |v| {
        let g = v["ents"]["grounded_ms"][balloon].as_i64().expect("the window is saved");
        v["ents"]["grounded_ms"][balloon] = serde_json::Value::from(g + 50);
    });
    assert!(hashed, "a save edited only in a grounded flier's window loads under the old hash: the window is not hashed");
    // An unedited save resumes it, hash for hash.
    let mut loaded = BattleState::load(&s.save()).expect("the save loads");
    for k in 0..40 {
        s.tick();
        loaded.tick();
        assert_eq!(loaded.state_hash(), s.state_hash(), "the loaded battle parts from the saved one on tick {k} after the save");
    }
    // The hidden Tesla's snare, saved while it holds.
    let mut t = BattleState::new(0, shipped());
    let tesla = t.scenario_spawn_now(Team::Red, "Tesla", at((9000, 22000)), None).expect("spawn Tesla");
    t.spawn_unit(Team::Blue, "Vines", at((9000, 22000)), None).expect("cast Vines");
    for _ in 0..25 {
        t.tick();
    }
    let slots = t.entity(tesla).unwrap().buffs.to_vec();
    let k = slots.iter().position(|b| b.reach_hidden).expect("the scene drifted: no slot on the Tesla reaches it under ground");
    let m = royalesim::status::MAX_BUFFS_PER_ENTITY;
    let hashed = edit_is_hashed(&t, |v| {
        v["ents"]["buffs"][tesla.index as usize * m + k]["reach_hidden"] = serde_json::Value::Bool(false);
    });
    assert!(hashed, "a save edited only in a slot's reach loads under the old hash: the reach is not hashed");
}

// ---------------------------------------------------------------------------
// (9) the 16.402 Vines

/// The shipped table with the Vines' area in the 16.402 table's shape (`edit` applied after).
fn vines_16402(edit: impl FnOnce(&mut serde_json::Value)) -> CardDb {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let mut v: serde_json::Value = serde_json::from_str(&text).expect("cards.json parses");
    let row = v["cards"].as_array_mut().expect("cards").iter_mut().find(|c| c["name"] == "Vines").expect("the Vines row");
    let a = &mut row["spell"]["area_effect_object"];
    a["hit_speed_ms"] = serde_json::Value::from(250);
    a["hit_speed_offset_ms"] = serde_json::Value::from(900);
    a["maximum_targets"] = serde_json::Value::from(1);
    a["hit_biggest_targets"] = serde_json::Value::Bool(true);
    a["life_duration_ms"] = serde_json::Value::from(1400);
    a["strike_area"]["catch_offsets_ms"] = serde_json::json!([0, 250, 500]);
    a["strike_area"]["selection_mode"] = serde_json::Value::from("HitBiggestTargets");
    edit(a);
    CardDb::from_json_str(&v.to_string(), CardSource::DerivedJson).expect("the edited table parses")
}

/// Plant: vines_pulse_unread.
#[test]
fn the_16402_vines_reads_its_pulse_as_the_catch_clock() {
    let db = vines_16402(|_| {});
    let i = db.index("Vines").filter(|&i| db.get(i).spell.is_some()).unwrap_or_else(|| panic!("refused: {:?}", db.rejected.iter().find(|(n, _)| n == "Vines")));
    let SpellShape::Strikes(d) = &db.get(i).spell.as_ref().unwrap().shape else { panic!("Vines: not strikes") };
    assert_eq!((d.pick, d.life_ms, d.gaps_ms.clone()), (StrikePick::RankedCatches, 1400, vec![900, 250, 250]));
    // A pulse that disagrees with the block's clock is refused, naming the column it carries.
    let off = vines_16402(|a| a["hit_speed_offset_ms"] = serde_json::Value::from(800));
    let why = off.rejected.iter().find(|(n, _)| n == "Vines").map(|(_, w)| w.clone()).unwrap_or_default();
    assert!(why.contains("MaximumTargets") || why.contains("HitSpeed"), "a HitSpeedOffset off the start: {why:?}");
    let gap = vines_16402(|a| a["strike_area"]["catch_offsets_ms"] = serde_json::json!([0, 250, 450]));
    assert!(gap.rejected.iter().any(|(n, _)| n == "Vines"), "offsets that are not the HitSpeed's multiples");
}

/// Plant: vines_pulse_unread.
#[test]
fn the_16402_vines_catches_on_c_plus_18_23_and_28() {
    let mut s = BattleState::new(0, BattleConfig::with_cards(vines_16402(|_| {})));
    let (ids, tap) = three_buildings(&mut s);
    let hp: Vec<i32> = ids.iter().map(|&id| s.entity(id).unwrap().hp).collect();
    assert!(hp[0] > hp[1] && hp[1] > hp[2], "the scene drifted: the hp order is not tower, Mortar, Cannon: {hp:?}");
    assert_eq!(first_held(&mut s, &ids, tap, 40), vec![Some(18), Some(23), Some(28)], "biggest first, 250 ms apart, the last at the life's end");
}
