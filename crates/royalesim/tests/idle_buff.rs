//! THE IDLE BUFF AND THE DAMAGE REDUCTION (status.IDLE_BUFF, status.DAMAGE_REDUCTION; card.rs `IdleBuffDef`,
//! `idle_buff_of`, `RawBuff::convert`; combat.rs `idle_on`, `damage_reduction_of`, `reduce_hit`; state.rs
//! `idle_buff_pass`): the Super Knight's shield.
//!
//! THE LAW, measured on client 15.535.29 (sweep-SuperKnight, seed 15): a level-11 Super Knight and a level-11 Knight
//! (202 a hit) first hit each other on the same tick. The Knight's hit of that tick takes 1 (2030 -> 2029); its three
//! hits while the Super Knight keeps attacking take 202 each; the princess tower's three hits after the Super Knight
//! walks on take 1 each. The table's shape: BuffWhenNotAttacking SuperKnight_ShieldBuff, whose action puts down an
//! own-troop area (radius 8000) every 50 ms, whose buff is a DamageReduction of 100.
//!
//! WHAT IS PINNED:
//!   1. `the_super_knights_shield_leaves_1_while_it_idles_and_202_while_it_attacks`: a level-11 Blue Super Knight and
//!      a level-11 Red Knight meet on open ground. The Knight's first hit lands on or before the Super Knight's own
//!      first hit and takes 1; every hit after it, while the Super Knight keeps attacking, takes the Knight's 202.
//!      Under status.IDLE_BUFF = not_read, the engine before the key, every hit takes 202.
//!   2. `an_idle_evo_knight_takes_80_of_a_202_hit`: THE HYPOTHESIS ARM'S PIN. status.DAMAGE_REDUCTION's formula
//!      between 1 and 99 is not measured; this pins what the shipped arm (truncated_floor_one) gives. A synthetic card,
//!      the Knight row with the Evo Knight's BuffWhenNotAttacking as the extractor writes it (units.Knight_EV1:
//!      Knight_Fortify_EV1, a DamageReduction of 60 on the unit itself), takes 80 = max(1, 202 x 40 / 100) truncated
//!      from the Red Knight's first hit, which lands while it has not hit yet. ceil_floor_one would give 81.
//!   3. `the_loader_walks_the_idle_chain_through_the_buff_refusal`: the table's Super Knight loads its shield (an area
//!      every 50 ms: radius 8000, own troops, 80 ms, a buff of DamageReduction 100). The same file refuses the Super
//!      Knight when the shield area's buff carries a DamageMultiplier (a column the engine does not run), and when its
//!      DamageReduction is -100 (a damage increase), naming each; and when the card row has no `idle_buff` block, as
//!      the table before the extractor wrote it, it refuses the row as a BuffWhenNotAttacking nothing reads.
//!   4. `the_idle_buffs_return_is_state`: entity.rs `idle_back` is hashed on a unit with an idle buff: a save edited
//!      only there fails the load's self-check.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! idle_buff`):
//!   * `idle_shield_while_attacking` -- combat.rs `idle_on` never turns the buff off: (1) goes red, the hits while the
//!     Super Knight attacks taking 1.
//!   * `reduction_rounds_up` -- combat.rs `reduce_hit`'s shipped arm rounds a partial reduction up: (2) goes red (81);
//!     (1) stays green, a reduction of 100 still leaving 1.
//!   * `idle_area_buff_unrefused` -- card.rs keeps a unit whose idle area it cannot load, without the area: (3) goes
//!     red.
//!   * `hash_skips_idle_back` -- state.rs does not hash `idle_back`: (4) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, IdleBuffDef, SpellShape};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, IdleBuffLaw};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Level 11, the scene's level.
fn level11(cards: CardDb) -> BattleConfig {
    let mut c = BattleConfig::with_cards(cards);
    c.card_level = [11, 11];
    c.tower_level = [11, 11];
    c
}

/// The file the engine loads, as JSON, for a test to doctor.
fn table() -> serde_json::Value {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    serde_json::from_str(&text).expect("cards.json parses")
}

fn load(doc: &serde_json::Value) -> CardDb {
    CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).expect("the file parses")
}

/// The level-11 Knight's hit, read off the loaded card: the scene's 202.
fn knight_hit(s: &BattleState) -> i32 {
    let db = s.cards();
    let k = db.index("Knight").expect("the Knight loads");
    db.scaled(k, 11, db.get(k).damage).expect("level 11")
}

/// A duel on open Blue ground, out of every crown tower's reach: a Blue `blue` at (9000, 11000) and a Red Knight at
/// (9000, 14500), both set down deployed. Per tick after the setup (k = 1 the first tick run): what each lost.
struct Duel {
    /// (k, hp the Blue unit lost on tick k), every tick it lost some.
    blue_losses: Vec<(u32, i32)>,
    /// The first tick the Red Knight lost hp: the Blue unit's first hit.
    blue_first_hit: Option<u32>,
    state: BattleState,
    blue: EntityId,
}

fn duel(cfg: BattleConfig, blue: &str, ticks: u32) -> Duel {
    let mut s = BattleState::new(0, cfg);
    let b = s.scenario_spawn_now(Team::Blue, blue, at((9000, 11000)), None).expect("the Blue unit stands");
    let r = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 14500)), None).expect("the Red Knight stands");
    let hp = |s: &BattleState, id: EntityId| s.entity(id).map_or(0, |e| e.hp);
    let (mut blue_losses, mut blue_first_hit) = (Vec::new(), None);
    for k in 1..=ticks {
        let (hb, hr) = (hp(&s, b), hp(&s, r));
        s.tick();
        if hp(&s, b) < hb {
            blue_losses.push((k, hb - hp(&s, b)));
        }
        if hp(&s, r) < hr && blue_first_hit.is_none() {
            blue_first_hit = Some(k);
        }
        if s.entity(r).is_none() || s.entity(b).is_none() {
            break;
        }
    }
    Duel { blue_losses, blue_first_hit, state: s, blue: b }
}

// ---------------------------------------------------------------------------
// (1)

/// Plant: idle_shield_while_attacking.
#[test]
fn the_super_knights_shield_leaves_1_while_it_idles_and_202_while_it_attacks() {
    let d = duel(level11(cards()), "SuperKnight", 400);
    let full = knight_hit(&d.state);
    assert_eq!(full, 202, "the level-11 Knight's hit, the measured scene's");
    let first = d.blue_first_hit.expect("the Super Knight never hit the Knight");
    let (k0, l0) = *d.blue_losses.first().expect("the Knight never hit the Super Knight");
    // Precondition: the Knight's first hit lands while the Super Knight has not hit yet, or on its first-hit tick.
    assert!(k0 <= first, "the Knight first hit on k{k0}, after the Super Knight's first hit on k{first}: {:?}", d.blue_losses);
    assert_eq!(l0, 1, "the Knight's hit on an idle Super Knight: {:?}", d.blue_losses);
    let attacking: Vec<(u32, i32)> = d.blue_losses.iter().copied().filter(|(k, _)| *k > first).collect();
    assert!(attacking.len() >= 2, "vacuous: {} Knight hits after the Super Knight's first hit: {:?}", attacking.len(), d.blue_losses);
    assert!(attacking.iter().all(|(_, l)| *l == full), "hits while the Super Knight attacks: {attacking:?}");
    // The engine before the key: the shield never runs, every hit is whole.
    let mut old = level11(cards());
    old.calib.idle_buff = IdleBuffLaw::NotRead;
    let d = duel(old, "SuperKnight", 400);
    assert!(d.blue_losses.len() >= 3 && d.blue_losses.iter().all(|(_, l)| *l == full), "not_read: {:?}", d.blue_losses);
}

// ---------------------------------------------------------------------------
// (2)

/// THE HYPOTHESIS ARM'S PIN (status.DAMAGE_REDUCTION between 1 and 99). Plant: reduction_rounds_up.
#[test]
fn an_idle_evo_knight_takes_80_of_a_202_hit() {
    let mut doc = table();
    let block = doc["units"]["Knight_EV1"]["idle_buff"].clone();
    assert_eq!(block["buff"]["damage_reduction"], 60, "the scene drifted: units.Knight_EV1's idle buff is not Knight_Fortify_EV1's 60");
    let knight = doc["cards"].as_array().unwrap().iter().find(|c| c["name"] == "Knight").cloned().expect("a Knight row");
    let mut evo = knight;
    evo["name"] = "EvoKnightTest".into();
    evo["display_name"] = "Evo Knight (test)".into();
    evo["summon_character"] = "Knight_EV1".into();
    evo["idle_buff"] = block;
    doc["cards"].as_array_mut().unwrap().push(evo);
    let db = load(&doc);
    let ib = db.get(db.index("EvoKnightTest").unwrap_or_else(|| panic!("the synthetic Evo Knight is refused: {:?}", db.rejected))).idle_buff;
    let own = ib.and_then(|b| b.own).expect("the Evo Knight's idle buff is its own");
    assert_eq!(db.buffs[own as usize].damage_reduction, 60);
    let d = duel(level11(db), "EvoKnightTest", 400);
    let first = d.blue_first_hit.expect("the Evo Knight never hit the Knight");
    let (k0, l0) = *d.blue_losses.first().expect("the Knight never hit the Evo Knight");
    assert!(k0 <= first, "the Knight first hit on k{k0}, after the Evo Knight's first hit on k{first}: {:?}", d.blue_losses);
    assert_eq!(knight_hit(&d.state), 202);
    assert_eq!(l0, 80, "max(1, 202 x (100 - 60) / 100), truncated: {:?}", d.blue_losses);
}

// ---------------------------------------------------------------------------
// (3)

/// The loader's refusal of `doc`'s Super Knight, or None when it loads.
fn super_knight_refusal(doc: &serde_json::Value) -> Option<String> {
    let db = load(doc);
    match db.index("SuperKnight") {
        Some(_) => None,
        None => Some(db.rejected.iter().find(|(n, _)| n == "SuperKnight").map(|(_, w)| w.clone()).expect("the Super Knight is neither loaded nor refused")),
    }
}

/// Plant: idle_area_buff_unrefused.
#[test]
fn the_loader_walks_the_idle_chain_through_the_buff_refusal() {
    // The table's Super Knight: its shield, read.
    let db = cards();
    let sk = db.get(db.index("SuperKnight").unwrap_or_else(|| panic!("the Super Knight is refused: {:?}", db.rejected)));
    assert_eq!(sk.idle_buff, Some(IdleBuffDef { own: None, idle_ms: 50, area_every_ms: 50 }), "BuffWhenNotAttackingTime 50, an area every 50 ms");
    let area = sk.idle_area.as_ref().expect("the shield area");
    let SpellShape::PulsingAreaEffect { hit, life_ms, hit_speed_ms, child: None } = &area.shape else { panic!("the shield area: {:?}", area.shape) };
    assert_eq!((*life_ms, *hit_speed_ms, hit.radius), (80, 50, 8000 * K), "SuperKnight_ShieldAEO's LifeDuration, HitSpeed and Radius");
    assert!(hit.only_own_troops && hit.hits_ground && hit.hits_air && hit.caps_buff_time, "an own-troop area capped to its time: {hit:?}");
    let b = hit.buff.expect("the shield area hangs a buff");
    assert_eq!(db.buffs[b.buff as usize].damage_reduction, 100, "SuperKnight_ShieldBuff_other");
    // The chain meets the buff refusal: a column the engine does not run on the area's buff refuses the card.
    let mut doc = table();
    assert_eq!(doc["area_effect_objects"]["SuperKnight_ShieldAEO"]["buff"]["damage_reduction"], 100, "the scene drifted");
    doc["area_effect_objects"]["SuperKnight_ShieldAEO"]["buff"]["damage_multiplier"] = 150.into();
    let why = super_knight_refusal(&doc).expect("a Super Knight whose shield carries a DamageMultiplier loads");
    assert!(why.starts_with("idle buff area SuperKnight_ShieldAEO: ") && why.contains("carries DamageMultiplier"), "{why}");
    // A DamageReduction the formula does not cover: refused too.
    let mut doc = table();
    doc["area_effect_objects"]["SuperKnight_ShieldAEO"]["buff"]["damage_reduction"] = (-100).into();
    let why = super_knight_refusal(&doc).expect("a Super Knight whose shield increases damage loads");
    assert!(why.contains("DamageReduction -100, outside 1..=100"), "{why}");
    // The table before the extractor wrote the block: the column is still on the row, and nothing reads it.
    let mut doc = table();
    let row = doc["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "SuperKnight").expect("a SuperKnight row");
    assert!(row.as_object_mut().unwrap().remove("idle_buff").is_some(), "the scene drifted: the Super Knight's row has no idle_buff");
    assert_eq!(doc["units"]["SuperKnight"]["raw"]["BuffWhenNotAttacking"], "SuperKnight_ShieldBuff");
    let why = super_knight_refusal(&doc).expect("a Super Knight whose BuffWhenNotAttacking nothing reads loads");
    assert_eq!(why, "the unit's BuffWhenNotAttacking SuperKnight_ShieldBuff is read by nothing in this file; not simulated");
}

// ---------------------------------------------------------------------------
// (4)

/// Plant: hash_skips_idle_back.
#[test]
fn the_idle_buffs_return_is_state() {
    let d = duel(level11(cards()), "SuperKnight", 400);
    let i = d.blue.index as usize;
    let v: serde_json::Value = serde_json::from_slice(&d.state.save()).unwrap();
    let back = v["ents"]["idle_back"][i].as_u64().expect("the snapshot carries the idle buff's return");
    assert!(back > 0, "vacuous: the Super Knight never hit, so its return is 0");
    assert!(edit_is_hashed(&d.state, |v| v["ents"]["idle_back"][i] = serde_json::Value::from(back + 1)), "idle_back is not hashed");
}
