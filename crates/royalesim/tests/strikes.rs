//! A STRIKING AREA (card.rs `SpellShape::Strikes`, `StrikeDef`; spell.rs `strike`): Lightning.
//!
//! THE LAW, measured on client 15.535.29 (28 casts, 40 strikes), D the cast tick (k = 0 below, the first tick run
//! after the play):
//!   - strike k falls on D + floor(k x HitSpeed / 50): D + 9, D + 18, D + 27 for HitSpeed 460, at most 3
//!     (spells.STRIKE_TIMER_LEFTOVER = carried);
//!   - each strike picks the eligible enemy with the highest CURRENT hp (spells.STRIKE_HP_RANK), ties to the earliest
//!     created, never one the cast has already struck;
//!   - reach: the centre distance minus the target's radius at most Radius + 170 (3670; spells.STRIKE_REACH, an
//!     interim inside the measured [3642.6, 3702.6)), now and on the predicted next position;
//!   - the damage (the PROJECTILE row's, level-scaled) and its ZapFreeze land on the tick after the strike;
//!   - a crown tower takes the projectile's 25 %, ceil (265 of 1057 at level 11); the area's own 100 is never read.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads Lightning as a striking area of the rows' numbers;
//!   2. three Knights of distinct hp each lose one strike's damage, on D + 10, D + 19 and D + 28, the highest hp
//!      first;
//!   3. two Knights of equal hp: the earlier created is struck first;
//!   4. a princess tower alone loses 25 % of one strike, once, over the whole cast;
//!   5. a target just beyond Radius + its radius, within the + 170, is struck;
//!   6. a striking area's clock is state: a save edited only in it fails the load's hash self-check.
//!   7. a Knight walking out of reach, inside it on the strike tick and outside it on the predicted next position, is
//!      not struck.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test strikes`):
//!   * `strike_timer_restarts` -- the leftover dropped: (2) goes red on D + 19.
//!   * `strike_tie_by_slot` -- ties to the highest slot: (3) goes red.
//!   * `strike_crown_from_area` -- the area's 100 %: (4) goes red.
//!   * `strike_repeats_target` -- the struck set is not read: (4) goes red (the tower is struck three times).
//!   * `strike_reach_bare` -- the refuted Radius + r: (5) goes red.
//!   * `strike_ignores_next_position` -- no test of the next position: (7) goes red.
//!   * `hash_skips_strikes` -- the motion is not hashed: (6) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::SpellShape;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, TapSnap};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// `config()` with placement.TAP_SNAP's old arm, none, for a scene that taps an exact point a measured distance from
/// its target (the reach's margin): the shipped tile-centre snap would move the tap.
fn unsnapped() -> BattleConfig {
    let mut c = config();
    c.calib.placement_tap_snap = TapSnap::None;
    c
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
    // One at a time, so the call order is the creation order. A batch creates in its own order, by the Red frame,
    // which is rotated: it would create the Knight at x 9500 first.
    let first = s.scenario_spawn_now(Team::Red, "Knight", at((8500, 22000)), Some(1300)).expect("spawn");
    let second = s.scenario_spawn_now(Team::Red, "Knight", at((9500, 22000)), Some(1300)).expect("spawn");
    assert!(s.entity(first).unwrap().team_seq < s.entity(second).unwrap().team_seq, "the scene drifted: the first spawn is not the earlier created");
    let ids = [first, second];
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
fn a_target_within_the_170_margin_is_struck() {
    let mut s = BattleState::new(0, unsnapped());
    let c = s.scenario_spawn_now(Team::Red, "Cannon", at((9000, 22000)), None).expect("spawn");
    let (cpos, r) = (s.entity(c).unwrap().pos, s.entity(c).unwrap().radius / K);
    // South of the Cannon, where no crown tower is in reach. East of it, the Red princess tower at (14500, 25500)
    // is in reach, and its hp outranks the Cannon's.
    let tap = Vec2::new(cpos.x, cpos.y - (3500 + r + 80) * K);
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
        // One strike can kill the Cannon (1057 against its 824 at level 11), so a Cannon gone counts as a loss.
        // The 100 skips its lifetime decay.
        if let Some(b) = b {
            if s.entity(c).map_or(0, |v| v.hp) < b - 100 {
                hit = Some(k);
            }
        }
    }
    let centre = centre.expect("no striking area stood");
    let (dx, dy) = ((centre.x / K - cpos.x / K) as i64, (centre.y / K - cpos.y / K) as i64);
    let d = isqrt(dx * dx + dy * dy);
    let bare = (3500 + r) as i64;
    assert!(d > bare && d <= bare + 170, "the scene drifted: the Cannon stands {d} from the strike centre, not in ({bare}, {}]", bare + 170);
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
    let hashed = edit_is_hashed(&s, |v| {
        let spells = v["spells"].as_array_mut().expect("the snapshot lists its spells");
        let sp = spells.iter_mut().find(|sp| sp["motion"].get("Strikes").is_some()).expect("the striking area is saved");
        let n = sp["motion"]["Strikes"]["next_ms"].as_i64().expect("its clock is saved");
        sp["motion"]["Strikes"]["next_ms"] = serde_json::Value::from(n + 1);
    });
    assert!(hashed, "a save edited only in a striking area's clock loads under the old hash: the clock is not hashed");
}

/// Plant: strike_ignores_next_position.
#[test]
fn a_knight_about_to_leave_reach_is_not_struck() {
    // A red Knight walks south and east, away from a Lightning cast north of it. It gains about 40 on the edge each
    // tick, so it starts from a band of edges (3070 to 3670): on some of them the strike tick finds it inside reach
    // and its next step outside. The tap is 6000 from the Red king tower (radius 1400), out of its reach: a tower in
    // reach outranks the Knight and takes the strike, whatever the Knight does.
    let (mut found, mut control) = (false, false);
    for off in (0..=600).step_by(10) {
        let tap = at((9000, 23000));
        let r = card_stat(&BattleState::new(0, config()), "Knight").collision_radius / K;
        let start = Vec2::new(tap.x, tap.y - (3500 + 170 + r - 600 + off) * K);
        let mut s = BattleState::new(0, unsnapped());
        let k = s.scenario_spawn_now(Team::Red, "Knight", start, None).expect("spawn");
        s.spawn_unit(Team::Blue, "Lightning", tap, None).expect("cast Lightning");
        let mut centre = None;
        let (mut at9, mut lost10) = (None, false);
        for t in 0..11u32 {
            let before = s.entity(k).map(|v| (v.pos, v.hp));
            s.tick();
            if centre.is_none() {
                centre = s.spells().iter().find_map(|sp| match &sp.motion {
                    SpellMotion::Strikes { pos, .. } => Some(*pos),
                    _ => None,
                });
            }
            let now = s.entity(k).map(|v| (v.pos, v.hp));
            if t == 9 {
                at9 = now.map(|n| (n.0, before.map_or(n.0, |b| b.0)));
            }
            if t == 10 {
                lost10 = matches!((before, now), (Some(b), Some(n)) if n.1 < b.1);
            }
        }
        let (Some(c), Some((p9, p8))) = (centre, at9) else { continue };
        let edge = |p: Vec2| {
            let (dx, dy) = ((p.x / K - c.x / K) as i64, (p.y / K - c.y / K) as i64);
            isqrt(dx * dx + dy * dy) - r as i64
        };
        let next = Vec2::new(p9.x + (p9.x - p8.x), p9.y + (p9.y - p8.y));
        if edge(p9) <= 3670 && edge(next) > 3670 {
            found = true;
            assert!(!lost10, "offset {off}: the Knight, {} inside the edge now and {} outside next, was struck", 3670 - edge(p9), edge(next) - 3670);
        }
        // The control: inside reach now and next, the Knight is struck. Without it, a strike taken by something else
        // would pass the case above.
        if edge(p9) <= 3670 && edge(next) <= 3670 {
            control = true;
            assert!(lost10, "offset {off}: the Knight, inside reach now and next, was not struck");
        }
    }
    assert!(found, "the scene drifted: no start put the walking Knight inside reach on the strike tick and outside next");
    assert!(control, "the scene drifted: no start kept the walking Knight inside reach now and next");
}
