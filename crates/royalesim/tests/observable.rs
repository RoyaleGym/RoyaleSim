//! WHAT A PLAYER SEES ON A UNIT (owner rule 2026-10-03: the bot must have all information a human can see or deduce;
//! agreed with RoyaleTraining and RoyaleGym): status bits 5 a Clone's copy, 6 an ability winding up, 7 an ability
//! active, 8 fully charged; the entity row's `charge` (permille), `dest_x` / `dest_y` and `ability_ticks`
//! (py.rs ENTITY_FIELDS, after `mount_uid`; state.rs `BattleState::ability_state`, `charge_permille`).
//!
//! PINNED, each on a scene that shows it from both sides of the change:
//!   1. a Prince's run-up climbs from 0 and reads 1000 with bit 8 once charged; a Knight reads 0 throughout;
//!   2. an Inferno Tower's ramp climbs on its target, 0 with no target;
//!   3. a Miner under ground carries its landing point, where it comes up (one step on), and -1 once up;
//!   4. a Clone's copy carries bit 5, its original not;
//!   5. a Monk's button: bit 7 and ticks left that count down while his deflect runs, none before the press;
//!   6. the JSON row carries the four columns after `mount_uid`.

#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::py::{ids_of_indices, state_json_text, ENTITY_FIELDS};
use royalesim::state::BattleState;
use royalesim::Team;
use std::collections::BTreeMap;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn view_of<'a>(s: &'a BattleState, team: Team, card: &str) -> Option<royalesim::state::EntityView<'a>> {
    s.entities().find(|e| e.team == team && e.card == card)
}

#[test]
fn a_princes_run_up_is_watched_and_a_knights_is_not() {
    let mut s = BattleState::new(0, config());
    s.spawn_unit(Team::Blue, "Prince", at((3500, 6000)), None).unwrap();
    s.spawn_unit(Team::Blue, "Knight", at((14500, 6000)), None).unwrap();
    let mut seen = Vec::new();
    for _ in 0..200 {
        s.tick();
        let Some(p) = view_of(&s, Team::Blue, "Prince") else { break };
        let k = view_of(&s, Team::Blue, "Knight").expect("the Knight");
        assert_eq!((k.charge_permille, k.status_flags & 256), (0, 0), "a Knight has no run-up");
        seen.push((p.charge_permille, p.status_flags & 256 != 0));
        if p.status_flags & 256 != 0 {
            break;
        }
    }
    assert!(seen.iter().any(|(c, _)| *c > 0 && *c < 1000), "the run-up passes through the middle: {seen:?}");
    assert_eq!(seen.last(), Some(&(1000, true)), "charged: 1000 and bit 8");
    assert!(seen.windows(2).all(|w| w[1].0 >= w[0].0), "the run-up only climbs while he walks: {seen:?}");
}

#[test]
fn an_inferno_towers_ramp_climbs_on_its_target() {
    let mut s = BattleState::new(0, config());
    s.spawn_unit(Team::Blue, "InfernoTower", at((9000, 9000)), None).unwrap();
    let idle: Vec<i32> = (0..30).map(|_| {
        s.tick();
        view_of(&s, Team::Blue, "InfernoTower").map_or(-9, |e| e.charge_permille)
    }).collect();
    assert!(idle.iter().all(|c| *c == 0), "no target, no ramp: {idle:?}");
    s.scenario_spawn_now(Team::Red, "Golem", at((9000, 13000)), None).unwrap();
    let mut ramp = Vec::new();
    for _ in 0..200 {
        s.tick();
        ramp.push(view_of(&s, Team::Blue, "InfernoTower").map_or(-9, |e| e.charge_permille));
    }
    assert!(ramp.iter().any(|c| *c > 0 && *c < 1000) && ramp.contains(&1000), "the ramp climbs to its last stage: {ramp:?}");
}

#[test]
fn a_miner_under_ground_carries_its_landing_point() {
    let mut s = BattleState::new(0, config());
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "Miner", at((4500, 24500)), None).unwrap();
    let mut under = Vec::new();
    let mut up = None;
    for _ in 0..200 {
        s.tick();
        let Some(m) = view_of(&s, Team::Blue, "Miner") else { continue };
        if m.status_flags & 1 != 0 {
            under.push(m.tunnel_dest);
        } else if !under.is_empty() {
            up = Some((m.pos, m.tunnel_dest));
            break;
        }
    }
    assert!(!under.is_empty(), "scene: the Miner went under ground");
    let dest = under[0].expect("a tunnelling Miner carries its destination");
    assert!(under.iter().all(|d| *d == Some(dest)), "one fixed destination for the dig");
    let (pos, after) = up.expect("scene: the Miner came up");
    // It comes up at the point it carried and walks on that same tick, so its first frame above ground is one step from
    // it (measured: 1,908 subtiles on each axis, toward Red's tower).
    let off = pos.sub(dest);
    assert!(off.x.abs() <= 2500 && off.y.abs() <= 2500, "it comes up one step from the point it carried: {off:?}");
    assert_eq!(after, None, "and carries none once up (the row reads -1)");
}

#[test]
fn a_clones_copy_carries_bit_5() {
    let mut s = BattleState::new(0, config());
    s.spawn_unit(Team::Blue, "Knight", at((9500, 9500)), None).unwrap();
    for _ in 0..25 {
        s.tick();
    }
    let p = view_of(&s, Team::Blue, "Knight").expect("the Knight").pos;
    s.spawn_unit(Team::Blue, "Clone", p, None).unwrap();
    for _ in 0..3 {
        s.tick();
    }
    let knights: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.card == "Knight").collect();
    assert_eq!(knights.len(), 2, "scene: the Knight and its copy");
    for k in knights {
        assert_eq!(k.status_flags & 32 != 0, k.cloned, "bit 5 is the copy's alone");
    }
}

#[test]
fn a_monks_deflect_is_active_with_its_ticks_left() {
    let mut cfg = config();
    let deck: Vec<String> = ["Monk", "Archer", "Giant", "Musketeer", "Fireball", "Valkyrie", "HogRider", "Minions"].iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy_slot(Team::Blue, 0, at((9500, 9500))).expect("play the Monk");
    let mut pressed = false;
    let mut ticks = Vec::new();
    for _ in 0..200 {
        s.tick();
        let Some(m) = view_of(&s, Team::Blue, "Monk") else { continue };
        if !pressed {
            assert_eq!((m.status_flags & 128, m.ability_ticks), (0, 0), "nothing active before the press");
            s.scenario_set_elixir_milli(Team::Blue, 10_000);
            pressed = s.press_ability_button(Team::Blue, 0).is_ok();
            continue;
        }
        if m.status_flags & 128 != 0 {
            ticks.push(m.ability_ticks);
        } else if !ticks.is_empty() {
            break;
        }
    }
    assert!(pressed, "scene: the Monk's button was pressed");
    assert!(ticks.len() > 2 && ticks[0] > 0, "the deflect is active with ticks left: {ticks:?}");
    assert!(ticks.windows(2).all(|w| w[1] < w[0]), "and they count down: {ticks:?}");
}

#[test]
fn the_row_carries_the_four_columns_after_the_mount() {
    let db = cards();
    let catalogue: Vec<u16> = (0..db.cards.len() as u16).filter(|i| !db.get(*i).summon_only && db.get(*i).evo.is_none()).collect();
    let ids = ids_of_indices(&db, &catalogue);
    let mut s = BattleState::new(0, config());
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "Miner", at((4500, 24500)), None).unwrap();
    for _ in 0..3 {
        s.tick();
    }
    let v: serde_json::Value = serde_json::from_str(&state_json_text(&s, &db, &ids, &[[0, 1, 2], [0, 2, 1]], &BTreeMap::new()).unwrap()).unwrap();
    let col = |n: &str| ENTITY_FIELDS.iter().position(|f| *f == n).unwrap();
    assert_eq!(col("charge"), col("mount_uid") + 1);
    let rows = v["entities"].as_array().unwrap();
    assert!(rows.iter().all(|r| r.as_array().unwrap().len() == ENTITY_FIELDS.len()), "every row is ENTITY_FIELDS long");
    let miner = s.entities().position(|e| e.card == "Miner").expect("the Miner");
    let r = &rows[miner];
    let d = s.entities().nth(miner).unwrap().tunnel_dest.expect("under ground");
    assert_eq!((r[col("dest_x")].as_i64(), r[col("dest_y")].as_i64()), (Some(d.x as i64), Some(d.y as i64)));
    let tower = rows.iter().find(|r| r[col("tower_slot")].as_i64().unwrap() >= 0).unwrap();
    assert_eq!((tower[col("dest_x")].as_i64(), tower[col("ability_ticks")].as_i64(), tower[col("charge")].as_i64()), (Some(-1), Some(0), Some(0)));
}

/// THE CATALOGUE'S `evo_cycle` (13th value): the basic plays before each evolved play of the card's evolution, 0 for a
/// card with none, so an enemy's evolution charge can be counted from the plays a player sees: the Evo Barbarians 1,
/// the Evo Skeletons 2, the Giant (no evolution) 0.
#[test]
fn the_catalogue_names_each_cards_evolution_cycle() {
    use royalesim::py::{catalogue_rows, CATALOGUE_FIELDS};
    let db = cards();
    let calib = royalesim::state::Calib::shipped();
    let catalogue: Vec<u16> = ["Barbarians", "Skeletons", "Giant"].iter().map(|n| db.index(n).unwrap()).collect();
    let rows: serde_json::Value = serde_json::from_str(&catalogue_rows(&db, &calib, &catalogue, 11).unwrap()).unwrap();
    let col = CATALOGUE_FIELDS.iter().position(|f| *f == "evo_cycle").expect("the column");
    assert_eq!(col, CATALOGUE_FIELDS.len() - 1, "the last column");
    let got: Vec<i64> = rows.as_array().unwrap().iter().map(|r| r[col].as_i64().unwrap()).collect();
    assert_eq!(got, vec![1, 2, 0]);
}
