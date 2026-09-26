//! targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED = "projectile_attackers_only" with combat.RETARGET_PROGRESS =
//! keep_when_dead_or_in_reach, read off the engine: who keeps a target that leaves its reach mid-swing, and what the
//! swing does when the target changes (target.rs `decide`, state.rs `phase_target`).
//!
//! THE LAW, measured on client 15.535.29:
//!   - a direct striker's swing does not lock its target. It keeps it within Range + both radii +
//!     LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET, and past that rescans for the nearest enemy, which may be the same one;
//!   - a switch to an enemy already in reach keeps the swing: the next hit lands on it on the running cycle;
//!   - a projectile attacker, crown towers included, holds a started shot's target while its start-of-tick centre
//!     distance is within Range + both radii + 500 (target::PROJECTILE_HOLD_BEYOND_REACH), then rescans. A crown
//!     tower's rescan finds nothing past its range, so it drops the target.
//!
//! The scenes are the client's and tests/test_reach_loss_switch.py's. WHAT IS PINNED, each with its precondition:
//!   1. a Knight whose Hog Rider leaves its reach mid-swing takes a Cannon in reach on the next tick, never hits the
//!      Hog again, and hits the Cannon on the running cycle (the last hit + 24);
//!   2. with no other enemy, the Knight holds the Hog through the swing and hits it beyond reach, under both value
//!      pairs;
//!   3. a Musketeer holds a leaving Hog while a Cannon stands in reach, and takes the Cannon on the first tick whose
//!      start-of-tick Hog distance is past reach + 500;
//!   4. a Musketeer that launches at the Hog from beyond its reach takes the Cannon on the next tick, with the Hog
//!      still inside reach + 500;
//!   5. a princess tower drops a Knight walking out of its range on the first tick past Range + both radii + 500, and
//!      never fires from beyond it; under the shipped values it fires from beyond that and drops it later;
//!   6. the shipped values are true (every attacker) and keep_when_dead.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test reach_loss_switch`):
//!   * `preserve_scope_ignored` -- every attacker's swing locks under the scoped value too: (1) goes red.
//!   * `reach_switch_resets_swing` -- a switch to a target in reach restarts the swing: (1) goes red on the hit.
//!   * `projectile_hold_1500` -- a projectile attacker holds to the old cancel range: (3) and (5) go red.
//!   * `launch_beyond_ignored` -- a launch beyond reach does not end the hold: (4) goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, PreserveTargetScope, RetargetProgress};
use royalesim::{EntityId, Team};

/// (lock on, its scope, the progress arm)
type Arms = (bool, PreserveTargetScope, RetargetProgress);
const NEW: Arms = (true, PreserveTargetScope::ProjectileAttackersOnly, RetargetProgress::KeepWhenDeadOrInReach);
const OLD: Arms = (true, PreserveTargetScope::AllAttackers, RetargetProgress::KeepWhenDead);

fn with_arms(arms: Arms) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.preserve_target_if_hit_started = arms.0;
    cfg.calib.preserve_target_scope = arms.1;
    cfg.calib.retarget_progress = arms.2;
    cfg
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// A Knight's hit on the level these scenes play; a crown tower's arrow is 109.
const KNIGHT_HIT: i32 = 202;
const KNIGHT_PERIOD: u32 = 24;
/// Range + the attacker's radius + the Hog Rider's 600, and the keep-target extension.
const KNIGHT_REACH_ON_HOG: i64 = 1200 + 500 + 600;
const MUSKETEER_REACH_ON_HOG: i64 = 6000 + 500 + 600;
const KEEP_EXTENSION: i64 = 25;
const HOLD: i64 = 500;

/// One tick of a scene, read after it: the attacker's target, the Hog's START-of-tick distance from the attacker, and
/// which of the Hog and the Cannon took a hit of at least KNIGHT_HIT on the tick.
struct Row {
    target: Option<EntityId>,
    start_dist: i64,
    hog_hit: bool,
    cannon_hit: bool,
    /// the attacker launched (its attack phase reads the hit tick) at its target this tick
    fired: bool,
}

/// A red `attacker` at `me`, a blue Hog Rider at `hog` running north, and (unless None) a blue Cannon at `cannon`.
fn scene(arms: Arms, attacker: &str, me: (i32, i32), hog: (i32, i32), cannon: Option<(i32, i32)>, ticks: u32) -> (EntityId, Option<EntityId>, Vec<Row>) {
    let mut s = BattleState::new(0, with_arms(arms));
    let mut spawns = vec![(Team::Red, attacker, at(me), None), (Team::Blue, "HogRider", at(hog), None)];
    if let Some(c) = cannon {
        spawns.push((Team::Blue, "Cannon", at(c), None));
    }
    let ids = s.scenario_spawn_batch(&spawns).unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (a, h, c) = (ids[0], ids[1], ids.get(2).copied());
    let mut rows = Vec::new();
    for _ in 0..ticks {
        let (Some(av), Some(hv)) = (s.entity(a), s.entity(h)) else { break };
        let start_dist = dist(av.pos, hv.pos);
        let (hog_hp, cannon_hp) = (hv.hp, c.and_then(|c| s.entity(c)).map_or(0, |v| v.hp));
        s.tick();
        let Some(av) = s.entity(a) else { break };
        let hog_now = s.entity(h).map_or(0, |v| v.hp);
        let cannon_now = c.and_then(|c| s.entity(c)).map_or(0, |v| v.hp);
        let fired = av.attack_phase == AttackPhase::Cooldown;
        rows.push(Row { target: av.target, start_dist, hog_hit: hog_hp - hog_now >= KNIGHT_HIT, cannon_hit: cannon_hp - cannon_now >= KNIGHT_HIT, fired });
    }
    (h, c, rows)
}

const KNIGHT_AT: (i32, i32) = (13300, 21000);
const HOG_BY_KNIGHT: (i32, i32) = (14500, 19500);
const CANNON_BY_KNIGHT: (i32, i32) = (11300, 21600);

/// The first tick (index), after the Knight has taken the Hog, that STARTS with the Hog past the Knight's keep radius
/// (so its Target phase is the first to see it there), and the Knight's last hit on the Hog before it. The Knight
/// holds the Hog on the tick before.
fn knight_leave(rows: &[Row], hog: EntityId) -> (usize, usize) {
    let engaged = rows.iter().position(|r| r.target == Some(hog)).expect("the scene drifted: the Knight never took the Hog");
    let out = (engaged + 1..rows.len())
        .find(|&t| rows[t].start_dist > KNIGHT_REACH_ON_HOG + KEEP_EXTENSION)
        .expect("the scene drifted: the Hog never left the Knight's keep radius");
    assert_eq!(rows[out - 1].target, Some(hog), "the scene drifted: the Knight had let go of the Hog before it left");
    let last = (0..out).rev().find(|&t| rows[t].hog_hit).expect("the scene drifted: the Knight never hit the Hog");
    (out, last)
}

#[test]
fn a_direct_striker_switches_to_an_enemy_in_reach_and_keeps_its_swing() {
    let (hog, cannon, rows) = scene(NEW, "Knight", KNIGHT_AT, HOG_BY_KNIGHT, Some(CANNON_BY_KNIGHT), 60);
    let cannon = cannon.unwrap();
    // `out` is the first tick that starts with the Hog past the keep radius: its Target phase rescans.
    let (out, last) = knight_leave(&rows, hog);
    let due = last + KNIGHT_PERIOD as usize;
    assert!(last < out && out < due, "the scene drifted: the Hog left on {out}, not inside the swing {last}..{due}");
    assert_eq!(rows[out].target, Some(cannon), "the Knight does not take the Cannon on the tick the Hog starts past its keep radius");
    let late: Vec<usize> = (out..rows.len()).filter(|&t| rows[t].hog_hit).collect();
    assert!(late.is_empty(), "the Knight still hit the leaving Hog on {late:?}");
    let first_cannon = rows.iter().position(|r| r.cannon_hit);
    assert_eq!(first_cannon, Some(due), "the first hit on the Cannon is not on the running cycle (last hit {last} + {KNIGHT_PERIOD})");
}

#[test]
fn with_no_other_enemy_the_swing_finishes_on_the_leaving_target() {
    for arms in [NEW, OLD] {
        let (hog, _, rows) = scene(arms, "Knight", KNIGHT_AT, HOG_BY_KNIGHT, None, 60);
        let (out, last) = knight_leave(&rows, hog);
        let due = last + KNIGHT_PERIOD as usize;
        let let_go: Vec<usize> = (out..=due).filter(|&t| rows[t].target != Some(hog)).collect();
        assert!(let_go.is_empty(), "{arms:?}: the Knight let go of the Hog on {let_go:?} with no other enemy in sight");
        assert!(rows[due].hog_hit, "{arms:?}: no hit on the Hog on {due}, the running cycle beyond reach");
    }
}

#[test]
fn a_projectile_attacker_holds_a_leaving_target_to_reach_plus_500() {
    let (hog, cannon, rows) = scene(NEW, "Musketeer", (14500, 12500), (14500, 17000), Some((10000, 14000)), 80);
    let cannon = cannon.unwrap();
    assert!(rows.iter().any(|r| r.target == Some(hog)), "the scene drifted: the Musketeer never took the Hog");
    let switch = rows.iter().position(|r| r.target == Some(cannon)).expect("the Musketeer never took the Cannon");
    let limit = MUSKETEER_REACH_ON_HOG + HOLD;
    assert!(rows[switch].start_dist > limit, "the Musketeer took the Cannon on {switch} with the Hog {} away at the start of the tick", rows[switch].start_dist);
    assert!(rows[switch - 1].start_dist <= limit, "the Musketeer held the Hog past reach + {HOLD} (tick {}, {})", switch - 1, rows[switch - 1].start_dist);
    assert!(rows[switch - 1].start_dist > MUSKETEER_REACH_ON_HOG, "precondition: the Hog had not left reach before the switch");
}

#[test]
fn a_launch_beyond_reach_ends_the_hold_on_the_next_tick() {
    // The Musketeer scene moved to the centre column (no crown tower in its sight there), with the Hog 1441 farther
    // north at the start (5941 away), so the Musketeer's first shot
    // leaves with the Hog beyond its reach and inside reach + 500; the Cannon 6198 away, in reach but farther than
    // the Hog, so the Musketeer takes the Hog first.
    let (hog, cannon, rows) = scene(NEW, "Musketeer", (9000, 12500), (9000, 18441), Some((3100, 14400)), 40);
    assert_eq!(rows[0].target, Some(hog), "the scene drifted: the Musketeer did not take the Hog first: {:?}", rows.iter().take(6).map(|r| (r.target.map(|t| t == hog), r.start_dist)).collect::<Vec<_>>());
    let cannon = cannon.unwrap();
    let limit = MUSKETEER_REACH_ON_HOG + HOLD;
    let shot = rows
        .iter()
        .position(|r| r.fired && r.target == Some(hog))
        .expect("the scene drifted: the Musketeer never fired at the Hog");
    let d = rows[shot].start_dist;
    assert!(d > MUSKETEER_REACH_ON_HOG && d <= limit, "the scene drifted: the first shot left with the Hog {d} away, not beyond reach and inside reach + {HOLD}");
    assert!(rows[shot + 1].start_dist <= limit, "the scene drifted: the Hog was past reach + {HOLD} on the tick after the shot");
    assert_eq!(rows[shot + 1].target, Some(cannon), "the Musketeer did not take the Cannon on the tick after its shot beyond reach");
}

/// The blue right princess tower on a red Knight chasing a blue Giant north out of its range: per tick, whether the
/// tower targets the Knight, whether it fired at it this tick, and the Knight's start-of-tick distance to the tower.
fn tower_track(arms: Arms) -> Vec<(bool, bool, i64)> {
    let mut s = BattleState::new(0, with_arms(arms));
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Giant", at((14500, 18800)), None), (Team::Red, "Knight", at((14500, 14500)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let knight = ids[1];
    let tower = s.entities().find(|e| e.team == Team::Blue && e.pos == at((14500, 6500))).expect("the blue right princess tower").id;
    let mut rows = Vec::new();
    for _ in 0..60 {
        let (Some(tv), Some(kv)) = (s.entity(tower), s.entity(knight)) else { break };
        let d = dist(tv.pos, kv.pos);
        s.tick();
        let tv = s.entity(tower).unwrap();
        let on = tv.target == Some(knight);
        rows.push((on, on && tv.attack_phase == AttackPhase::Cooldown, d));
    }
    rows
}

/// Range 7500 + the tower's radius 1000 + the Knight's 500, and the 500 held past it.
const TOWER_LIMIT: i64 = 7500 + 1000 + 500 + 500;

#[test]
fn a_crown_tower_drops_a_started_shot_500_past_its_reach() {
    let rows = tower_track(NEW);
    let first = rows.iter().position(|r| r.0).expect("the scene drifted: the tower never took the Knight");
    let late: Vec<(usize, i64)> = rows.iter().enumerate().filter(|(_, r)| r.1 && r.2 > TOWER_LIMIT).map(|(t, r)| (t, r.2)).collect();
    assert!(late.is_empty(), "the tower fired from beyond {TOWER_LIMIT}: (tick, start-of-tick distance) {late:?}");
    let drop = (first..rows.len()).find(|&t| !rows[t].0).expect("the tower never dropped the Knight");
    assert!(rows[drop].2 > TOWER_LIMIT && rows[drop - 1].2 <= TOWER_LIMIT, "dropped on {drop} at {}, held at {}", rows[drop].2, rows[drop - 1].2);
    // and under the shipped values it fires from beyond that limit (the old cancel range), the precondition that
    // gives this scene something to separate
    let old = tower_track(OLD);
    assert!(old.iter().any(|r| r.1 && r.2 > TOWER_LIMIT), "precondition: the old values never fire from beyond {TOWER_LIMIT}");
}

#[test]
fn the_shipped_values_are_projectile_attackers_only_and_keep_when_dead_or_in_reach() {
    let c = Calib::shipped();
    assert!(c.preserve_target_if_hit_started);
    assert_eq!(c.preserve_target_scope, PreserveTargetScope::ProjectileAttackersOnly);
    assert_eq!(c.retarget_progress, RetargetProgress::KeepWhenDeadOrInReach);
}
