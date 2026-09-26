//! A STRIKING AREA (card.rs `SpellShape::Strikes`, `StrikeDef`; spell.rs `strike`): Lightning.
//!
//! THE LAW, measured on client 15.535.29 (28 casts, 40 strikes), D the cast tick (k = 0 below, the first tick run
//! after the play):
//!   - strike k falls on D + floor(k x HitSpeed / 50): D + 9, D + 18, D + 27 for HitSpeed 460, at most 3
//!     (spells.STRIKE_TIMER_LEFTOVER = carried);
//!   - each strike picks the eligible enemy with the highest CURRENT hp (spells.STRIKE_HP_RANK), ties to the earliest
//!     created, never one the cast has already struck;
//!   - reach: the centre distance at most Radius + the target's radius + 200 (spells.STRIKE_REACH, an interim);
//!   - the damage (the PROJECTILE row's, level-scaled) and its ZapFreeze land on the tick after the strike;
//!   - a crown tower takes the projectile's 25 %, ceil (265 of 1057 at level 11); the area's own 100 is never read.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads Lightning as a striking area of the rows' numbers;
//!   2. three Knights of distinct hp each lose one strike's damage, on D + 10, D + 19 and D + 28, the highest hp
//!      first;
//!   3. two Knights of equal hp: the earlier created is struck first;
//!   4. a princess tower alone loses 25 % of one strike, once, over the whole cast;
//!   5. a target just beyond Radius + its radius, within the + 200, is struck;
//!   6. a striking area's clock is state: two saves differing only in it hash differently.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test strikes`):
//!   * `strike_timer_restarts` -- the leftover dropped: (2) goes red on D + 19.
//!   * `strike_tie_by_slot` -- ties to the highest slot: (3) goes red.
//!   * `strike_crown_from_area` -- the area's 100 %: (4) goes red.
//!   * `strike_repeats_target` -- the struck set is not read: (4) goes red (the tower is struck three times).
//!   * `strike_reach_bare` -- the refuted Radius + r: (5) goes red.
//!   * `hash_skips_strikes` -- the motion is not hashed: (6) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::SpellShape;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The strike's damage at the Blue side's level.
fn strike_damage(s: &BattleState) -> i32 {
    let idx = s.cards().index("Lightning").expect("Lightning loads");
    s.cards().scaled(idx, s.config().card_level[0], 413).expect("a valid level")
}

/// Cast a Blue Lightning at `tap` and run `ticks` ticks from the cast tick (k = 0); every (k, id, hp lost) of `ids`.
fn losses(s: &mut BattleState, ids: &[EntityId], tap: Vec2, ticks: u32) -> Vec<(u32, EntityId, i32)> {
    s.spawn_unit(Team::Blue, "Lightning", tap, None).expect("cast Lightning");
    let mut out = Vec::new();
    for k in 0..ticks {
        let before: Vec<Option<i32>> = ids.iter().map(|&id| s.entity(id).map(|v| v.hp)).collect();
        s.tick();
        for (&id, b) in ids.iter().zip(before) {
            let (Some(b), now) = (b, s.entity(id).map_or(0, |v| v.hp)) else { continue };
            if now < b {
                out.push((k, id, b - now));
            }
        }
    }
    out
}

/// Plants: strike_crown_from_area (the crown share it reads).
#[test]
fn the_loader_reads_lightning_as_a_striking_area() {
    let s = BattleState::new(0, config());
    let spell = card_stat(&s, "Lightning").spell.as_ref().expect("Lightning loads as a spell");
    let SpellShape::Strikes(d) = &spell.shape else { panic!("Lightning: {:?}", spell.shape) };
    assert_eq!((d.hit.radius, d.life_ms, d.gaps_ms.clone()), (3500 * K, 1500, vec![460, 460, 460]));
    assert_eq!((d.hit.damage, d.hit.crown_pct), (413, 25), "the PROJECTILE row's damage and crown share");
    assert!(d.hit.only_enemies && d.hit.hits_air && d.hit.hits_ground, "{:?}", d.hit);
    assert!(d.hit.buff.is_some(), "the projectile's ZapFreeze");
}

/// Plant: strike_timer_restarts.
#[test]
fn three_strikes_land_on_d_plus_10_19_and_28_the_highest_hp_first() {
    let mut s = BattleState::new(0, config());
    let ids = s
        .scenario_spawn_batch(&[
            (Team::Red, "Knight", at((8000, 22000)), Some(1200)),
            (Team::Red, "Knight", at((9000, 22000)), Some(1400)),
            (Team::Red, "Knight", at((10000, 22000)), Some(1300)),
        ])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let got = losses(&mut s, &ids, at((9000, 22000)), 40);
    let dmg = strike_damage(&s);
    assert!(dmg < 1200, "the scene drifted: a strike ({dmg}) would kill a Knight and hide the next pick");
    assert_eq!(got, vec![(10, ids[1], dmg), (19, ids[2], dmg), (28, ids[0], dmg)], "the strikes (tick, target, loss)");
}

/// Plant: strike_tie_by_slot.
#[test]
fn a_tie_goes_to_the_earlier_created() {
    let mut s = BattleState::new(0, config());
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at((8500, 22000)), Some(1300)), (Team::Red, "Knight", at((9500, 22000)), Some(1300))])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let got = losses(&mut s, &ids, at((9000, 22000)), 12);
    assert_eq!(got.first().map(|g| (g.0, g.1)), Some((10, ids[0])), "the first strike: {got:?}");
}

/// Plants: strike_crown_from_area, strike_repeats_target.
#[test]
fn a_princess_tower_alone_loses_a_quarter_of_one_strike_once() {
    let mut s = BattleState::new(0, config());
    let tower = s.entities().find(|v| v.team == Team::Red && v.card == "PrincessTower").map(|v| (v.id, v.pos)).expect("a red princess tower");
    let got = losses(&mut s, &[tower.0], tower.1, 40);
    let dmg = strike_damage(&s);
    let want = (dmg * 25 + 99) / 100;
    if s.config().card_level[0] == 11 {
        assert_eq!(want, 265, "the measured 265 at level 11");
    }
    assert_eq!(got, vec![(10, tower.0, want)], "one strike, 25 % of it, ceil");
}

/// Plant: strike_reach_bare.
#[test]
fn a_target_within_the_200_margin_is_struck() {
    let mut s = BattleState::new(0, config());
    let c = s.scenario_spawn_now(Team::Red, "Cannon", at((9000, 22000)), None).expect("spawn");
    let (cpos, r) = (s.entity(c).unwrap().pos, s.entity(c).unwrap().radius / K);
    let tap = Vec2::new(cpos.x + (3500 + r + 100) * K, cpos.y);
    s.spawn_unit(Team::Blue, "Lightning", tap, None).expect("cast Lightning");
    let mut hit = None;
    let mut centre = None;
    for k in 0..12u32 {
        let b = s.entity(c).map(|v| v.hp);
        s.tick();
        if centre.is_none() {
            centre = s.spells().iter().find_map(|sp| match &sp.motion {
                SpellMotion::Strikes { pos, .. } => Some(*pos),
                _ => None,
            });
        }
        if let (Some(b), Some(v)) = (b, s.entity(c)) {
            if v.hp < b - 100 {
                hit = Some(k);
            }
        }
    }
    let centre = centre.expect("no striking area stood");
    let (dx, dy) = ((centre.x / K - cpos.x / K) as i64, (centre.y / K - cpos.y / K) as i64);
    let d = isqrt(dx * dx + dy * dy);
    let bare = (3500 + r) as i64;
    assert!(d > bare && d <= bare + 200, "the scene drifted: the Cannon stands {d} from the strike centre, not in ({bare}, {}]", bare + 200);
    assert_eq!(hit, Some(10), "a Cannon {d} away (Radius + r = {bare}) is struck on D + 9, its loss on D + 10");
}

/// Plant: hash_skips_strikes.
#[test]
fn a_striking_areas_clock_is_state() {
    let mut s = BattleState::new(0, config());
    let k = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 22000)), None).expect("spawn");
    s.spawn_unit(Team::Blue, "Lightning", s.entity(k).unwrap().pos, None).expect("cast Lightning");
    for _ in 0..3 {
        s.tick();
    }
    let bytes = s.save();
    let mut v: serde_json::Value = serde_json::from_slice(&bytes).expect("a snapshot is JSON");
    let spells = v["spells"].as_array_mut().expect("the snapshot lists its spells");
    let sp = spells.iter_mut().find(|sp| sp["motion"].get("Strikes").is_some()).expect("the striking area is saved");
    let n = sp["motion"]["Strikes"]["next_ms"].as_i64().expect("its clock is saved");
    sp["motion"]["Strikes"]["next_ms"] = serde_json::Value::from(n + 1);
    let edited = serde_json::to_vec(&v).unwrap();
    let a = BattleState::load(&bytes).expect("the save loads");
    let b = BattleState::load(&edited).expect("the edited save loads");
    assert_ne!(a.state_hash(), b.state_hash(), "two states differing only in a striking area's clock hash alike");
}
