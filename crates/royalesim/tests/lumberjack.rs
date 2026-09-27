//! THE LUMBERJACK (RageBarbarian): a death area whose one action puts down a bottle, loaded as that bottle's fuse over
//! the bottle's rage (card.rs `area_spawns_bottle`, `bottle_of`; state.rs `phase_reap`; calibration
//! spells.DEATH_FUSE_START).
//!
//! THE ROWS (the 15.535.29 tables): RageBarbarian's DeathAreaEffect RageBarbarianDummyForSpawn (LifeDuration 50, no
//! hit) runs one ActionSpawn of RageBarbarianBottle, a building row with DeployTime 500, no Hitpoints and the death area
//! BarbarianRage: an own-side pulsing area (Radius 3000, LifeDuration 5500, HitSpeed 300) of the Rage buff, capped to
//! its time, whose first update makes BarbarianRageDamage (Damage 70, crown share 30, enemies only). The Rage card's
//! bottle is the same shape.
//!
//! THE LAW, measured on client 15.535.29 (5 deaths; X the first tick the Lumberjack is absent, the tick it dies on
//! here): the rage's damage lands on X + 13, 179 on a troop at level 11 and 54 on a princess tower (ceil of 53.7); an
//! own unit inside walks raged (a Knight's step 60 -> 78). A fuse born in the death's Reap and counted like the Rage's
//! bottle would land the damage on X + 12: spells.DEATH_FUSE_START = one_tick_hop starts it a tick later.
//!
//! WHAT IS PINNED, each with its precondition (level 11; the Lumberjack killed by `debug_set_hp` to 0 before tick X,
//! which queues its death in that tick's Resolve like any hit):
//!   1. the loader reads the death area as a 500 ms fuse over the rage, loads no bottle record, and refuses a death area
//!      whose spawned row is not a bottle;
//!   2. an enemy Knight walking past the death point loses 179 on X + 13 and nothing else; under next_tick on X + 12;
//!   3. an enemy princess tower beside it loses 54 on X + 13;
//!   4. an own Knight walking inside takes its first raged step on X + 13, 74 to 79 a step, and unraged steps before.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test lumberjack`):
//!   * `death_bottle_unread` -- the death area stays an action graph the loader does not read: (1) to (4) go red.
//!   * `death_fuse_hop_shifted` -- the fuse starts the tick after the death whatever the key: (2), (3) and (4) go red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, SpellShape};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DeathFuseStart};
use royalesim::{EntityId, Team};

/// The level of the measured deaths.
const LEVEL: i32 = 11;

/// The shipped config at level 11, asserting the arm this file pins.
fn shipped() -> BattleConfig {
    let mut c = config();
    c.card_level = [LEVEL, LEVEL];
    c.tower_level = [LEVEL, LEVEL];
    assert_eq!(c.calib.death_fuse_start, DeathFuseStart::OneTickHop, "the shipped spells.DEATH_FUSE_START");
    c
}

/// A native point as engine subtiles.
fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Rage buff the Lumberjack's rage hangs, as a `CardDb::buffs` index.
fn rage_buff(s: &BattleState) -> u16 {
    let area = card_stat(s, "RageBarbarian").death_area_effect.as_ref().expect("the Lumberjack's death area");
    let SpellShape::Fuse { then, .. } = &area.shape else { panic!("the death area is not a fuse: {:?}", area.shape) };
    let SpellShape::PulsingAreaEffect { hit, .. } = then.as_ref() else { panic!("the fuse releases no pulsing area: {then:?}") };
    hit.buff.expect("the rage carries a buff").buff
}

/// A Blue Lumberjack set up at `lj` (native), killed before the first tick, and `victim`'s hp losses per tick after
/// its death tick X, as (k after X, lost), over X + 1 .. X + 20. `victim` is set up with the Lumberjack.
fn losses_after_death(cfg: BattleConfig, lj: (i32, i32), victim: (Team, &str, (i32, i32))) -> Vec<(u32, i32)> {
    let mut s = BattleState::new(0, cfg);
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "RageBarbarian", at(lj), None), (victim.0, victim.1, at(victim.2), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (jack, v) = (ids[0], ids[1]);
    assert!(s.debug_set_hp(jack, 0), "the Lumberjack stands");
    s.tick(); // X
    assert!(s.entity(jack).is_none(), "the scene drifted: the Lumberjack outlived tick X");
    let mut out = Vec::new();
    let mut hp = s.entity(v).expect("the victim stands").hp;
    for k in 1..=20u32 {
        s.tick();
        let now = s.entity(v).unwrap_or_else(|| panic!("the scene drifted: the victim died on X + {k}")).hp;
        if now < hp {
            out.push((k, hp - now));
        }
        hp = now;
    }
    out
}

// ---------------------------------------------------------------------------
// (1)

/// Plant: death_bottle_unread.
#[test]
fn the_loader_reads_the_death_area_as_the_bottles_fuse_over_the_rage() {
    let s = BattleState::new(0, shipped());
    let db = s.cards();
    let lj = db.index("RageBarbarian").unwrap_or_else(|| panic!("RageBarbarian refused: {:?}", db.rejected.iter().find(|(n, _)| n == "RageBarbarian")));
    let area = db.get(lj).death_area_effect.as_ref().expect("the Lumberjack's death area");
    let SpellShape::Fuse { fuse_ms, then } = &area.shape else { panic!("the death area: {:?}", area.shape) };
    assert_eq!(*fuse_ms, 500, "RageBarbarianBottle's DeployTime");
    let SpellShape::PulsingAreaEffect { hit, life_ms, hit_speed_ms, child } = then.as_ref() else { panic!("{then:?}") };
    assert_eq!((*life_ms, *hit_speed_ms, hit.radius), (5500, 300, 3000 * K), "BarbarianRage's LifeDuration, HitSpeed and Radius");
    assert!(hit.only_own_troops && !hit.only_enemies && hit.caps_buff_time, "an own-side rage, capped to its time: {hit:?}");
    let Some(child) = child else { panic!("the rage makes no damage area") };
    let SpellShape::AreaEffect { hit: dmg } = child.as_ref() else { panic!("{child:?}") };
    assert_eq!((dmg.damage, dmg.crown_pct, dmg.only_enemies), (70, 30, true), "BarbarianRageDamage");
    // The same buff row as the Rage card's.
    let SpellShape::Fuse { then: rage, .. } = &card_stat(&s, "Rage").spell.as_ref().expect("Rage loads").shape else { panic!("Rage is not a fuse") };
    let SpellShape::PulsingAreaEffect { hit: rh, .. } = rage.as_ref() else { panic!("{rage:?}") };
    assert_eq!(hit.buff.map(|b| b.buff), rh.buff.map(|b| b.buff), "the Lumberjack's rage hangs the Rage card's buff row");
    assert_eq!(rage_buff(&s), hit.buff.unwrap().buff);
    // No bottle record: nothing puts one on the board.
    assert!(db.cards.iter().all(|c| c.unit_name != "RageBarbarianBottle"), "a RageBarbarianBottle record loaded");
    assert!(db.unit_refs(lj).is_empty(), "the Lumberjack puts no unit on the board");
    assert_eq!(db.scaled(lj, LEVEL, 70), Ok(179), "the rage's damage at level 11");
    // A death area whose one spawn is not a bottle keeps the refusal: the same file with the bottle given hitpoints.
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let mut doc: serde_json::Value = serde_json::from_str(&text).expect("cards.json parses");
    assert!(doc["units"]["RageBarbarianBottle"].is_object(), "the scene drifted: no RageBarbarianBottle row");
    doc["units"]["RageBarbarianBottle"]["hitpoints"] = serde_json::json!(100);
    let db = CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).expect("the doctored file parses");
    let why = db.rejected.iter().find(|(n, _)| n == "RageBarbarian").map(|(_, w)| w.clone()).expect("the doctored Lumberjack loads");
    assert!(why.starts_with("death area effect RageBarbarianDummyForSpawn: area effect RageBarbarianDummyForSpawn runs an action graph this loader does not read"), "{why}");
}

// ---------------------------------------------------------------------------
// (2)

/// Plant: death_fuse_hop_shifted.
#[test]
fn an_enemy_knight_beside_the_death_loses_179_on_x_plus_13() {
    // On the red half, out of every Blue tower's reach: a red Knight 1500 from the death point walks toward it and the
    // river, inside the rage's 3000 through X + 20.
    let victim = (Team::Red, "Knight", (9000, 21500));
    assert_eq!(losses_after_death(shipped(), (9000, 20000), victim), vec![(13, 179)], "one_tick_hop: the Knight's losses after X");
    let mut c = shipped();
    c.calib.death_fuse_start = DeathFuseStart::NextTick;
    assert_eq!(losses_after_death(c, (9000, 20000), victim), vec![(12, 179)], "next_tick: the Knight's losses after X");
}

// ---------------------------------------------------------------------------
// (3)

/// Plant: death_fuse_hop_shifted.
#[test]
fn an_enemy_princess_tower_beside_the_death_loses_54_on_x_plus_13() {
    // A Blue Lumberjack 2000 in front of the red left princess tower, killed before the first tick.
    let mut s = BattleState::new(0, shipped());
    let w = s.arena().width / 2;
    let (t, tp): (EntityId, Vec2) = s.entities().find(|v| v.team == Team::Red && v.card == "PrincessTower" && v.pos.x < w).map(|v| (v.id, v.pos)).expect("the red left princess tower");
    let jack = s.scenario_spawn_now(Team::Blue, "RageBarbarian", Vec2::new(tp.x, tp.y - 2000 * K), None).expect("the Lumberjack");
    assert!(s.debug_set_hp(jack, 0), "the Lumberjack stands");
    s.tick(); // X
    assert!(s.entity(jack).is_none(), "the scene drifted: the Lumberjack outlived tick X");
    let mut got = Vec::new();
    let mut hp = s.entity(t).expect("the tower stands").hp;
    for k in 1..=20u32 {
        s.tick();
        let now = s.entity(t).expect("the tower stands").hp;
        if now < hp {
            got.push((k, hp - now));
        }
        hp = now;
    }
    assert_eq!(got, vec![(13, 54)], "the princess tower's losses after X: the crown share of 179, rounded up");
}

// ---------------------------------------------------------------------------
// (4)

/// Plant: death_fuse_hop_shifted.
#[test]
fn an_own_knight_inside_walks_raged_from_x_plus_13() {
    let mut s = BattleState::new(0, shipped());
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "RageBarbarian", at((9000, 11500)), None), (Team::Blue, "Knight", at((9000, 9500)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (jack, knight) = (ids[0], ids[1]);
    let buff = rage_buff(&s);
    assert!(s.debug_set_hp(jack, 0), "the Lumberjack stands");
    s.tick(); // X
    assert!(s.entity(jack).is_none(), "the scene drifted: the Lumberjack outlived tick X");
    let centre = at((9000, 11500));
    let mut steps = Vec::new();
    let mut raged = Vec::new();
    let mut last = s.entity(knight).expect("the Knight").pos;
    for k in 1..=16u32 {
        s.tick();
        let v = s.entity(knight).unwrap_or_else(|| panic!("the scene drifted: the Knight died on X + {k}"));
        let (dx, dy) = ((v.pos.x / K - last.x / K) as i64, (v.pos.y / K - last.y / K) as i64);
        steps.push(isqrt(dx * dx + dy * dy));
        if v.buffs.iter().any(|b| b.id == buff + 1 && b.ms > 0) {
            raged.push(k);
        }
        let (cx, cy) = ((v.pos.x / K - centre.x / K) as i64, (v.pos.y / K - centre.y / K) as i64);
        assert!(isqrt(cx * cx + cy * cy) < 3000, "the scene drifted: the Knight left the rage on X + {k}");
        last = v.pos;
    }
    assert_eq!(raged.first(), Some(&12), "the rage first lands on the Knight on X + 12: {raged:?}");
    assert!(steps[..12].iter().all(|&x| (55..=61).contains(&x)), "unraged steps on X + 1 .. X + 12: {steps:?}");
    assert!(steps[12..].iter().all(|&x| (74..=79).contains(&x)), "raged steps from X + 13: {steps:?}");
}
