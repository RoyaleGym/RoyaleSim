//! targeting.PROJECTILE_HOLD_SCOPE, read off the engine: whether a projectile troop holds its target past its keep
//! reach while it walks.
//!
//! THE LAW, measured on the 16.402 corpus: a projectile troop holds its target to Range + both radii + 500 only while
//! it is in its attack. Walking to a target past its keep reach (Range + both radii + 25) and inside that hold, with a
//! valid enemy nearer by centre in sight, it took the nearer enemy on the tick in 8 of 8 samples (7 events, 4
//! battles; in 20260918-112751 on tick 1590 an Archer walking to a Goblin Hut 225.3 past its reach takes the Hut's new
//! wave member on its 8th frame). In its attack it kept the target on 13 of 14 (the 14th had launched at it from
//! beyond its reach).
//!
//! The scenes are tests/test_projectile_hold_scope.py's: a blue Archer at (3500, 9000) walking up the left lane to a
//! red Giant walking down it from (3500, 21000), and a red Knight set down at (6500, 17500) on the first tick that
//! starts with the Giant past the Archer's keep reach and inside its hold. Pinned here, each with its preconditions:
//!   1. client_troop_in_attack: the walking Archer takes the Knight on the Knight's first tick or the next, while the
//!      Giant still stands inside the hold;
//!   2. every_tick: the walking Archer keeps the Giant on every tick that starts with it inside the hold;
//!   3. both values: with no Knight the walking Archer keeps the Giant on every such tick and then attacks it (a
//!      rescan returns the same target);
//!   4. both values: a red Musketeer in its attack keeps a blue Hog Rider that runs out of its reach, against a nearer
//!      blue Cannon, while the Hog stands inside the hold (tests/reach_loss_switch.rs's projectile scene);
//!   5. the shipped value is every_tick.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test projectile_hold_scope`):
//!   * `projectile_hold_while_walking` -- client_troop_in_attack still holds a walking troop's target past its keep
//!     reach: (1) goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, ProjectileHoldScope};
use royalesim::{EntityId, Team};

const NEW: ProjectileHoldScope = ProjectileHoldScope::ClientTroopInAttack;
const OLD: ProjectileHoldScope = ProjectileHoldScope::EveryTick;

const ARCHER_AT: (i32, i32) = (3500, 9000);
const GIANT_AT: (i32, i32) = (3500, 21000);
const KNIGHT_AT: (i32, i32) = (6500, 17500);
/// The Archer's reach on the Giant: Range 5000 + radii 500 and 750, native.
const REACH_ON_GIANT: i64 = 5000 + 500 + 750;
/// The Archer's sight on the Knight: SightRange 5500 + radii 500 and 500.
const SIGHT_ON_KNIGHT: i64 = 5500 + 500 + 500;
const KNIGHT_R: i64 = 500;
const GIANT_R: i64 = 750;
const KEEP_EXTENSION: i64 = 25;
const HOLD: i64 = 500;
const MUSKETEER_AT: (i32, i32) = (14500, 12500);
const HOG_AT: (i32, i32) = (14500, 17000);
const CANNON_AT: (i32, i32) = (10000, 14000);
/// The Musketeer's reach on the Hog Rider: Range 6000 + radii 500 and 600.
const REACH_ON_HOG: i64 = 6000 + 500 + 600;

fn with_arm(arm: ProjectileHoldScope) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.projectile_hold_scope = arm;
    cfg
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Centre distance, native units.
fn dist(a: Vec2, b: Vec2) -> i64 {
    isqrt(a.dist2(b)) / K as i64
}

/// Past the keep reach and inside the hold.
fn inside_hold(d: i64, reach: i64) -> bool {
    d > reach + KEEP_EXTENSION && d <= reach + HOLD
}

/// One tick of the Archer scene: the START-of-tick state its Target phase reads, and the target after the tick.
struct Row {
    start_giant: i64,
    start_phase: AttackPhase,
    start_target: Option<EntityId>,
    /// the Knight's centre distance from the Archer at the tick's start, once it stands on the board
    start_knight: Option<i64>,
    target: Option<EntityId>,
    phase: AttackPhase,
}

/// The Archer scene under `arm`: (the Giant, the Knight if it was set down, the rows).
fn walk(arm: ProjectileHoldScope, with_knight: bool, ticks: u32) -> (EntityId, Option<EntityId>, Vec<Row>) {
    let mut s = BattleState::new(0, with_arm(arm));
    s.scenario_set_tick(200);
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Archer", at(ARCHER_AT), None), (Team::Red, "Giant", at(GIANT_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (archer, giant) = (ids[0], ids[1]);
    let mut knight: Option<EntityId> = None;
    let mut rows = Vec::new();
    for _ in 0..ticks {
        let (Some(a), Some(g)) = (s.entity(archer), s.entity(giant)) else { break };
        let (a_pos, start_giant, start_phase, start_target) = (a.pos, dist(a.pos, g.pos), a.attack_phase, a.target);
        // The Knight is set down between two ticks, before the first tick that starts with the Archer walking to the
        // Giant inside its hold, so that tick's Target phase is the first to see it.
        if with_knight
            && knight.is_none()
            && start_target == Some(giant)
            && start_phase == AttackPhase::Idle
            && inside_hold(start_giant, REACH_ON_GIANT)
        {
            let k = s
                .scenario_spawn_batch(&[(Team::Red, "Knight", at(KNIGHT_AT), None)])
                .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
            knight = Some(k[0]);
        }
        let start_knight = knight.and_then(|k| s.entity(k)).map(|kv| dist(a_pos, kv.pos));
        s.tick();
        let Some(a) = s.entity(archer) else { break };
        rows.push(Row { start_giant, start_phase, start_target, start_knight, target: a.target, phase: a.attack_phase });
    }
    (giant, knight, rows)
}

/// The first row that starts with the Knight on the board, its preconditions checked.
fn knight_row(rows: &[Row], giant: EntityId, what: &str) -> usize {
    let i = rows
        .iter()
        .position(|r| r.start_knight.is_some())
        .unwrap_or_else(|| panic!("{what}: the scene drifted: the Knight was never set down"));
    let r = &rows[i];
    assert!(
        r.start_target == Some(giant) && r.start_phase == AttackPhase::Idle && inside_hold(r.start_giant, REACH_ON_GIANT),
        "{what}: the scene drifted: row {i} does not start with the Archer walking to the Giant inside its hold ({} away)",
        r.start_giant
    );
    let dk = r.start_knight.unwrap();
    assert!(
        dk <= SIGHT_ON_KNIGHT && dk < r.start_giant && dk - KNIGHT_R < r.start_giant - GIANT_R,
        "{what}: the scene drifted: the Knight stands {dk} from the Archer and the Giant {}",
        r.start_giant
    );
    i
}

#[test]
fn a_walking_troop_takes_a_nearer_enemy_inside_its_hold() {
    let (giant, knight, rows) = walk(NEW, true, 90);
    let knight = knight.expect("the scene drifted: the Knight was never set down");
    let i = knight_row(&rows, giant, "client_troop_in_attack");
    let switch = (i..rows.len()).find(|&j| rows[j].target == Some(knight));
    assert!(
        switch.is_some_and(|j| j <= i + 1),
        "the walking Archer kept the Giant {} away (inside its hold) with the Knight {} away in sight; it took the Knight on \
         row {switch:?} (the Knight came before row {i})",
        rows[i].start_giant,
        rows[i].start_knight.unwrap()
    );
    let j = switch.unwrap();
    assert!(
        rows[j].start_phase == AttackPhase::Idle && inside_hold(rows[j].start_giant, REACH_ON_GIANT),
        "the switch on row {j} did not come while the Archer walked with the Giant inside its hold"
    );
}

#[test]
fn the_old_value_holds_the_walking_troops_target() {
    let (giant, _, rows) = walk(OLD, true, 90);
    let i = knight_row(&rows, giant, "every_tick");
    let held: Vec<usize> = (i..rows.len())
        .filter(|&j| rows[j].start_target == Some(giant) && rows[j].start_phase == AttackPhase::Idle && inside_hold(rows[j].start_giant, REACH_ON_GIANT))
        .collect();
    assert!(!held.is_empty(), "the scene drifted: no row with the Knight on the board starts with the Giant inside the hold");
    let let_go: Vec<usize> = held.iter().copied().filter(|&j| rows[j].target != Some(giant)).collect();
    assert!(let_go.is_empty(), "every_tick: the walking Archer let the Giant go inside its hold on rows {let_go:?}");
}

#[test]
fn with_no_nearer_enemy_the_walker_keeps_its_target() {
    for arm in [NEW, OLD] {
        let (giant, _, rows) = walk(arm, false, 90);
        let band: Vec<usize> = (0..rows.len())
            .filter(|&j| rows[j].start_target == Some(giant) && rows[j].start_phase == AttackPhase::Idle && inside_hold(rows[j].start_giant, REACH_ON_GIANT))
            .collect();
        assert!(band.len() >= 2, "{arm:?}: the scene drifted: the Archer walked inside its hold on {} ticks", band.len());
        let lost: Vec<usize> = band.iter().copied().filter(|&j| rows[j].target != Some(giant)).collect();
        assert!(lost.is_empty(), "{arm:?}: with no other enemy the walking Archer let the Giant go on rows {lost:?}");
        let last = *band.last().unwrap();
        let attacks = rows[last..rows.len().min(last + 10)].iter().any(|r| r.target == Some(giant) && r.phase != AttackPhase::Idle);
        assert!(attacks, "{arm:?}: the Archer never attacked the Giant after walking up to it");
    }
}

#[test]
fn a_troop_in_its_attack_holds_a_leaving_target() {
    for arm in [NEW, OLD] {
        let mut s = BattleState::new(0, with_arm(arm));
        let ids = s
            .scenario_spawn_batch(&[
                (Team::Red, "Musketeer", at(MUSKETEER_AT), None),
                (Team::Blue, "HogRider", at(HOG_AT), None),
                (Team::Blue, "Cannon", at(CANNON_AT), None),
            ])
            .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
        let (m, hog) = (ids[0], ids[1]);
        let mut band = 0;
        for tick in 1..=60u32 {
            let (Some(mv), Some(hv)) = (s.entity(m), s.entity(hog)) else { break };
            let (d, phase, target) = (dist(mv.pos, hv.pos), mv.attack_phase, mv.target);
            s.tick();
            let Some(mv) = s.entity(m) else { break };
            if target == Some(hog) && inside_hold(d, REACH_ON_HOG) {
                band += 1;
                assert_ne!(phase, AttackPhase::Idle, "{arm:?}: the scene drifted: the Musketeer was not in its attack on {tick}");
                assert_eq!(mv.target, Some(hog), "{arm:?}: the Musketeer in its attack let the leaving Hog go {d} away on {tick}");
            }
        }
        assert!(band >= 3, "{arm:?}: the scene drifted: the Hog stood inside the hold on {band} ticks");
    }
}

#[test]
fn the_shipped_value_is_the_old_one() {
    assert_eq!(Calib::shipped().projectile_hold_scope, OLD);
}
