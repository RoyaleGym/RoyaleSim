//! A STRAIGHT SHOT PASSES A UNIT UNDER GROUND AND A HIDDEN BUILDING (combat.rs `straight_hits`, `untouchable_now`).
//!
//! Under movement.SPAWN_PATHFIND_BODY = untouchable a unit under ground (a Miner or a Goblin Drill's dig on its way to
//! its tap; entity.rs `underground`) is out of every target scan, hit, area and push. Under
//! hide.HIDDEN_IMMUNE_TO_DAMAGE a hidden building (the Tesla under its hide) takes no damage. `resolve` drops a hit on
//! either, whatever wrote it, by one test (`untouchable_now`). A straight shot now passes both by that test, as it
//! passes an attached rider. It matters for a shot that stops on its first hit (combat.PROJECTILE_COLLISIONS =
//! client_columns: the Hunter's pellets). Before, such a shot stopped on the body and `resolve` dropped the hit, so the
//! pellet was spent on nothing.
//!
//! WHAT IS PINNED, each scene against the same volley with the body left out:
//!   1. `hunter_pellets_fly_through_a_miner_under_ground`: a Red Miner tunnels down the line of a volley at a Red
//!      Knight. The Knight takes the same 5 pellets as it does with no Miner. Before, every pellet that reached the
//!      Miner stopped on it, and the Knight took none.
//!   2. `hunter_pellets_fly_through_a_hidden_tesla`: a hidden Tesla stands on the path of a pellet that misses the
//!      Knight, where the Hunter is out of its wake range, with a Cannon behind it. The pellet flies past the Tesla
//!      and lands on the Cannon, as it does with no Tesla. Before, the Tesla stopped it. The Tesla can stand only near
//!      the end of a pellet's range and stay hidden (its wake range is 6,600 from the Hunter, a pellet's range 6,500),
//!      so the scene places it from the pellet's own path and asserts each distance it needs.
//! The pellets' damage and reach and every radius are read from the data. No client recording has a pellet crossing
//! a Miner under ground or an idle Tesla; the rule is the one every other reader of the two keys follows.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test straight_shot_bodies`):
//!   * `straight_shot_meets_under_ground` -- a straight shot stops on a unit under ground: (1) red.
//!   * `straight_shot_meets_hidden` -- a straight shot stops on a hidden building: (2) red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, MultipleProjectiles, ProjectileCollisions, RangeProjectile, SpawnPathfindBody};
use royalesim::{EntityId, Team};
use std::collections::BTreeMap;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Native distance between two world points.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// Every card and tower at level 11.
fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

/// The shipped arms of the keys these scenes stand on, asserted.
fn shipped() -> BattleConfig {
    let cfg = level11(config());
    let c = &cfg.calib;
    assert_eq!(c.spawn_pathfind_body, SpawnPathfindBody::Untouchable, "the shipped movement.SPAWN_PATHFIND_BODY");
    assert!(c.hide_hidden_immune, "the shipped hide.HIDDEN_IMMUNE_TO_DAMAGE");
    assert_eq!(c.range_projectile, RangeProjectile::StraightToRange, "the shipped combat.RANGE_PROJECTILE");
    assert_eq!(c.multiple_projectiles, MultipleProjectiles::ClientFan, "the shipped combat.MULTIPLE_PROJECTILES");
    assert_eq!(c.projectile_collisions, ProjectileCollisions::ClientColumns, "the shipped combat.PROJECTILE_COLLISIONS");
    cfg
}

/// A card's collision radius, native.
fn radius(s: &BattleState, card: &str) -> i64 {
    (card_stat(s, card).collision_radius / K) as i64
}

/// A Red Miner's tap on the Blue side. Its walk from the Red King runs straight down x 9249 to y 21601, 650 a tick,
/// before it turns for the bridge.
const MINER_TAP: (i32, i32) = (9000, 9500);

/// Every straight shot in the air by its aim point: (tick, position) after each tick it is alive.
type Tracks = BTreeMap<(i32, i32), Vec<(u32, Vec2)>>;

fn record_shots(s: &BattleState, tracks: &mut Tracks) {
    for p in s.projectiles().iter().filter(|p| p.straight.is_some()) {
        tracks.entry((p.aim.x / K, p.aim.y / K)).or_default().push((s.tick_count(), p.pos));
    }
}

/// (a pellet's damage, its ProjectileRadius native), read off the Hunter's first shot in the air.
fn pellet_of(s: &BattleState) -> Option<(i32, i64)> {
    s.projectiles().iter().find_map(|p| p.straight.as_ref().map(|st| (p.damage, (st.reach / K) as i64)))
}

/// How many pellets of `dmg` an hp series lost, one tick at a time. A building's lifetime drain (a few hitpoints a
/// tick) is left out by the division.
fn pellets_landed(hp: &[i32], dmg: i32) -> i32 {
    hp.windows(2).map(|w| (w[0] - w[1]).max(0) / dmg).sum()
}

// ---------------------------------------------------------------------------
// (1) the Hunter's pellets and a Miner under ground

/// A Blue Hunter and a Red Knight 5,000 apart on the Miner's line, clear of every tower. The Hunter fires its first
/// volley on tick 14; the Knight walks at it meanwhile.
const HUNTER_AT: (i32, i32) = (9250, 19500);
const KNIGHT_AT: (i32, i32) = (9250, 24500);
/// The Miner is played before this tick. Played before ticks 1 to 9 it is under the pellets, and the Knight took no
/// pellet at all before the fix; from tick 12 it comes too late.
const MINER_PLAYED: u32 = 5;
const VOLLEY_TICKS: u32 = 40;

struct Volley {
    /// The Knight's hp before the first tick and after each.
    knight: Vec<i32>,
    shots: Tracks,
    /// The Miner after each tick it is on the board: (tick, position, under ground, hp, max hp).
    miner: Vec<(u32, Vec2, bool, i32, i32)>,
    pellet: (i32, i64),
}

fn hunter_volley(miner_played: Option<u32>) -> Volley {
    let mut s = BattleState::new(0, shipped());
    s.scenario_spawn_now(Team::Blue, "Hunter", at(HUNTER_AT), None).expect("place the Hunter");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT_AT), None).expect("place the Knight");
    let mut v = Volley { knight: vec![s.entity(knight).expect("the Knight").hp], shots: Tracks::new(), miner: Vec::new(), pellet: (0, 0) };
    let mut pellet = None;
    for t in 0..VOLLEY_TICKS {
        if miner_played == Some(t) {
            s.spawn_unit(Team::Red, "Miner", at(MINER_TAP), None).expect("play the Miner");
        }
        s.tick();
        v.knight.push(s.entity(knight).expect("the scene drifted: the Knight died").hp);
        record_shots(&s, &mut v.shots);
        pellet = pellet.or_else(|| pellet_of(&s));
        for m in find_live(&s, Team::Red, "Miner") {
            v.miner.push((s.tick_count(), m.pos, m.tunnel_dest.is_some(), m.hp, m.max_hp));
        }
    }
    v.pellet = pellet.expect("the scene drifted: the Hunter never fired");
    v
}

/// Plant: straight_shot_meets_under_ground.
#[test]
fn hunter_pellets_fly_through_a_miner_under_ground() {
    let control = hunter_volley(None);
    let with = hunter_volley(Some(MINER_PLAYED));
    let (dmg, reach) = control.pellet;
    let s = BattleState::new(0, shipped());
    let r_miner = radius(&s, "Miner");
    // the Miner was under ground where a pellet flew, on that pellet's tick (the control's flight: nothing in it
    // depends on a Miner, which nothing meets)
    let crossed = with
        .miner
        .iter()
        .filter(|m| m.2)
        .filter(|m| control.shots.values().flatten().any(|(t, p)| *t == m.0 && dist(*p, m.1) <= reach + r_miner))
        .count();
    assert!(crossed >= 2, "the scene drifted: the Miner was under ground on a pellet's path on {crossed} ticks");
    assert!(with.miner.iter().filter(|m| m.2).all(|m| m.3 == m.4), "the Miner lost hitpoints under ground");
    let (landed, alone) = (pellets_landed(&with.knight, dmg), pellets_landed(&control.knight, dmg));
    assert_eq!(alone, 5, "the scene drifted: the volley with no Miner lands {alone} pellets on the Knight");
    assert_eq!(landed, alone, "the Knight took {landed} pellets with a Miner under ground on their line, {alone} with none");
}

// ---------------------------------------------------------------------------
// (2) the Hunter's pellets and a hidden Tesla

/// The Hunter and the Knight of the second scene, 5,000 apart on x 9000, clear of every tower. The volley's outer
/// pellets miss the Knight and fly to their range.
const HUNTER2_AT: (i32, i32) = (9000, 18000);
const KNIGHT2_AT: (i32, i32) = (9000, 23000);
/// How far past a pellet's second-last point the Tesla stands, and past its last point the Cannon, along its path.
/// The Tesla is then inside the pellet's reach at that point and farther from the Hunter than its wake range; the
/// Cannon is out of the pellet's reach at the second-last point and inside it at the last.
const TESLA_PAST: i64 = 750;
const CANNON_PAST: i64 = 525;

struct TeslaVolley {
    shots: Tracks,
    /// The Cannon's hp before the first tick and after each.
    cannon: Vec<i32>,
    /// Whether the Tesla was hidden after each tick.
    hidden: Vec<bool>,
    pellet: (i32, i64),
}

fn tesla_volley(tesla: Option<Vec2>, cannon: Option<Vec2>) -> TeslaVolley {
    let mut s = BattleState::new(0, shipped());
    s.scenario_spawn_now(Team::Blue, "Hunter", at(HUNTER2_AT), None).expect("place the Hunter");
    s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT2_AT), None).expect("place the Knight");
    let t: Option<EntityId> = tesla.map(|p| s.scenario_spawn_now(Team::Red, "Tesla", p, None).expect("place the Tesla"));
    let c: Option<EntityId> = cannon.map(|p| s.scenario_spawn_now(Team::Red, "Cannon", p, None).expect("place the Cannon"));
    let hp = |s: &BattleState| c.and_then(|c| s.entity(c)).map_or(0, |e| e.hp);
    let mut v = TeslaVolley { shots: Tracks::new(), cannon: vec![hp(&s)], hidden: Vec::new(), pellet: (0, 0) };
    let mut pellet = None;
    for _ in 0..VOLLEY_TICKS {
        s.tick();
        record_shots(&s, &mut v.shots);
        pellet = pellet.or_else(|| pellet_of(&s));
        v.cannon.push(hp(&s));
        v.hidden.push(t.and_then(|t| s.entity(t)).is_some_and(|e| e.hidden));
    }
    v.pellet = pellet.expect("the scene drifted: the Hunter never fired");
    v
}

/// The point `past` native units beyond `b` on the line from `a` through `b`.
fn beyond(a: Vec2, b: Vec2, past: i64) -> Vec2 {
    let (dx, dy) = ((b.x / K - a.x / K) as i64, (b.y / K - a.y / K) as i64);
    let len = isqrt(dx * dx + dy * dy).max(1);
    Vec2::new((b.x / K + (dx * past / len) as i32) * K, (b.y / K + (dy * past / len) as i32) * K)
}

/// Plant: straight_shot_meets_hidden.
#[test]
fn hunter_pellets_fly_through_a_hidden_tesla() {
    // the pellet whose last point is farthest from the Hunter: one that missed the Knight and flew to its range
    let probe = tesla_volley(None, None);
    let hunter = at(HUNTER2_AT);
    let (aim, track) = probe.shots.iter().max_by_key(|(_, tr)| dist(tr.last().expect("a track").1, hunter)).expect("the Hunter fired");
    assert!(track.len() >= 4, "the scene drifted: the pellet flew {} ticks", track.len());
    let (p1, p2) = (track[track.len() - 2].1, track[track.len() - 1].1);
    let (p0, step) = (track[track.len() - 3].1, dist(p1, p2));
    let tesla = beyond(p0, p1, TESLA_PAST);
    let cannon = beyond(p1, p2, CANNON_PAST);
    let s = BattleState::new(0, shipped());
    let (dmg, reach) = probe.pellet;
    let (r_tesla, r_cannon, r_hunter) = (radius(&s, "Tesla"), radius(&s, "Cannon"), radius(&s, "Hunter"));
    let wake = (card_stat(&s, "Tesla").sight_range / K) as i64 + r_tesla + r_hunter;
    // the geometry, from the data: the pellet reaches the Tesla at p1 and not at p0; the Cannon at p2 and not at p1;
    // and the Hunter stands out of the Tesla's wake range
    assert!(step > 0 && dist(p0, tesla) > reach + r_tesla, "the pellet reaches the Tesla before its second-last point");
    assert!(dist(p1, tesla) <= reach + r_tesla, "the pellet never reaches the Tesla: {} from p1", dist(p1, tesla));
    assert!(dist(p1, cannon) > reach + r_cannon && dist(p2, cannon) <= reach + r_cannon, "the Cannon is not reached at the last point only");
    assert!(dist(tesla, hunter) > wake, "the Tesla stands {} from the Hunter, inside its wake range {wake}", dist(tesla, hunter));
    // the control: the pellet flies its path to the last point and stops there on the Cannon
    let control = tesla_volley(None, Some(cannon));
    let to_cannon = &track[..track.len() - 1];
    assert_eq!(control.shots.get(aim).map(|t| &t[..]), Some(to_cannon), "the scene drifted: the pellet did not stop on the Cannon at its last point");
    let alone = pellets_landed(&control.cannon, dmg);
    assert!(alone >= 1, "the scene drifted: no pellet lands on the Cannon with no Tesla");
    // the Tesla, hidden all the while, stops nothing
    let with = tesla_volley(Some(tesla), Some(cannon));
    let end = track.last().expect("a track").0 as usize;
    assert!(with.hidden[..end].iter().all(|&h| h), "the scene drifted: the Tesla was up while the pellet flew");
    assert_eq!(
        with.shots.get(aim).map(|t| &t[..]),
        Some(to_cannon),
        "the pellet did not fly past the hidden Tesla: it was last seen {:?}",
        with.shots.get(aim).and_then(|t| t.last())
    );
    let landed = pellets_landed(&with.cannon, dmg);
    assert_eq!(landed, alone, "the Cannon took {landed} pellets behind a hidden Tesla, {alone} with none");
}
