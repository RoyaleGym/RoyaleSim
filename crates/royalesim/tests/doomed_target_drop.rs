//! targeting.DOOMED_TARGET_DROP, read off the engine: a projectile attacker drops a target that the shots already in
//! flight will kill.
//!
//! THE LAW, measured on client 15.535.29: an attacker whose card fires a projectile and that has not launched a shot
//! at its target since acquiring it drops the target on the tick after it is doomed, and does not take it back while
//! it lives. Doomed: the shots in flight at it cover its hitpoints, and the one that lands last does so within 600 ms.
//! An attacker with no projectile keeps it, and so does one that has fired at it.
//!
//! The scenes are the client's, and tests/test_doomed_target_drop.py's, which also pins the windup, a crown tower and a
//! Musketeer, a ranged flyer without a projectile, the sum of two shots and damage below the hitpoints. Pinned here,
//! each with its preconditions checked:
//!   1. walking Minions drop a Knight doomed by a tower arrow on the next tick and never take it back, and under keep
//!      they keep it until it dies;
//!   2. the 600 ms gate: with a slow arrow they keep the Knight while the ETA read at the end of the previous tick is
//!      above 600 ms, and drop it on the first tick after it reads 600 or less;
//!   3. a Minion that has fired at its target and is in reach keeps it once it is doomed;
//!   4. a Knight, which has no projectile, walking at the doomed Knight keeps it;
//!   5. projectile_attackers_rescan: a Minion that spits at a doomed Knight from beyond its reach re-evaluates on the
//!      next tick and never takes the Knight back, while projectile_attackers takes it back and a Knight the spit
//!      does not doom is taken back under both (client 15.535.29: 4 of 4 such launches were followed by a drop);
//!   6. projectile_attackers_walk_drop: a Skeleton Dragon that spat from beyond its keep reach and walks after the
//!      Knight drops it on the tick after a tower arrow dooms it, while projectile_attackers_rescan keeps it and a
//!      Knight the shots do not doom is kept (client 15.535.29, the Skeleton Dragons sweep scene);
//!   7. a Skeleton Dragon still in its attack keeps a doomed Knight it has shot at from beyond its keep reach, under
//!      projectile_attackers_walk_drop and projectile_attackers_rescan (1,007 of 1,007 such keeps on client 15.535.29).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test doomed_target_drop`):
//!   * `doomed_eta_ignored` -- damage counts whenever it lands: (2) goes red.
//!   * `doomed_drop_ignores_fired` -- an attacker that has fired drops it too: (3) goes red.
//!   * `doomed_drop_every_attacker` -- an attacker with no projectile drops it too: (4) goes red.
//!   * `doomed_rescan_takes_fired` -- a rescan takes back a doomed unit the attacker has shot at: (5) goes red.
//!   * `doomed_walker_keeps_fired` -- a walking attacker keeps a doomed target it has shot at: (6) goes red.
//!   * `doomed_walk_drop_ignores_phase` -- the keep reach alone decides, so an attacker in its attack drops too: (7)
//!     goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DoomedTargetDrop};
use royalesim::{EntityId, Team};

const TICK_MS: usize = 50;
const ETA_LIMIT_MS: usize = 600;

/// One tick's state of the spawned units, by spawn index.
struct Tick {
    alive: Vec<bool>,
    target: Vec<Option<EntityId>>,
    shots_at: Vec<usize>,
    hp: Vec<i32>,
    phase: Vec<Option<AttackPhase>>,
    pos: Vec<Vec2>,
}

type Spawn = (Team, &'static str, (i32, i32), Option<i32>);

/// Spawn (team, card, native point, hp) from tick 200 under `arm` and run `ticks` ticks; returns the ids and the state
/// after each tick (index 0 is before the first).
fn play(arm: DoomedTargetDrop, spawns: &[Spawn], ticks: u32) -> (Vec<EntityId>, Vec<Tick>) {
    let mut cfg = config();
    cfg.calib.doomed_target_drop = arm;
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tick(200);
    let specs: Vec<(Team, &str, Vec2, Option<i32>)> = spawns.iter().map(|&(t, c, p, h)| (t, c, Vec2::new(p.0 * K, p.1 * K), h)).collect();
    let ids = s.scenario_spawn_batch(&specs).unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let snap = |s: &BattleState| Tick {
        alive: ids.iter().map(|id| s.entity(*id).is_some()).collect(),
        target: ids.iter().map(|id| s.entity(*id).and_then(|e| e.target)).collect(),
        shots_at: ids.iter().map(|id| s.projectiles().iter().filter(|p| p.target == *id).count()).collect(),
        hp: ids.iter().map(|id| s.entity(*id).map_or(0, |e| e.hp)).collect(),
        phase: ids.iter().map(|id| s.entity(*id).map(|e| e.attack_phase)).collect(),
        pos: ids.iter().map(|id| s.entity(*id).map_or(Vec2::default(), |e| e.pos)).collect(),
    };
    let mut out = vec![snap(&s)];
    for _ in 0..ticks {
        s.tick();
        out.push(snap(&s));
    }
    (ids, out)
}

/// (D, K) for the first shot at `victim` from tick `since` on: launched on tick D, the victim gone on tick K.
fn doom(run: &[Tick], victim: usize, since: usize) -> (usize, usize) {
    let d = (since..run.len()).find(|&t| run[t].shots_at[victim] > 0).expect("no shot ever flew at the victim: the scene drifted");
    let k = (0..run.len()).find(|&t| !run[t].alive[victim]).expect("the victim never died: the scene drifted");
    assert!(d < k, "the victim died on tick {k} before any shot flew at it");
    (d, k)
}

/// The (spawn index, tick) pairs in `ticks` on which one of `units` targets `victim`.
fn targeting(run: &[Tick], ids: &[EntityId], units: &[usize], victim: usize, ticks: std::ops::Range<usize>) -> Vec<(usize, usize)> {
    units.iter().flat_map(|&u| ticks.clone().filter(move |&t| run[t].target[u] == Some(ids[victim])).map(move |t| (u, t))).collect()
}

/// The (spawn index, tick) pairs in `ticks` on which one of `units` does NOT target `victim`.
fn not_targeting(run: &[Tick], ids: &[EntityId], units: &[usize], victim: usize, ticks: std::ops::Range<usize>) -> Vec<(usize, usize)> {
    units.iter().flat_map(|&u| ticks.clone().filter(move |&t| run[t].target[u] != Some(ids[victim])).map(move |t| (u, t))).collect()
}

/// A red Knight attacking the blue right princess tower, and three blue Minions in sight of it and out of reach.
const WALK: [Spawn; 4] = [
    (Team::Red, "Knight", (14500, 9150), Some(60)),
    (Team::Blue, "Minions", (8700, 9150), None),
    (Team::Blue, "Minions", (8800, 8000), None),
    (Team::Blue, "Minions", (8900, 10400), None),
];

#[test]
fn walking_minions_drop_a_knight_doomed_by_a_tower_arrow_on_the_next_tick() {
    let (ids, run) = play(DoomedTargetDrop::ProjectileAttackers, &WALK, 40);
    let (d, k) = doom(&run, 0, 0);
    assert!((k - d) * TICK_MS <= ETA_LIMIT_MS, "precondition: the arrow lands {} ms after D", (k - d) * TICK_MS);
    let minions = [1, 2, 3];
    assert!(not_targeting(&run, &ids, &minions, 0, d..d + 1).is_empty(), "precondition: a Minion was not after the Knight at D");
    let held = targeting(&run, &ids, &minions, 0, d + 1..k);
    assert!(held.is_empty(), "still (or again) after the doomed Knight, (spawn, tick) with D={d}, K={k}: {held:?}");
    // and under keep, the old arm, they keep it until it dies
    let (ids, run) = play(DoomedTargetDrop::Keep, &WALK, 40);
    let (d, k) = doom(&run, 0, 0);
    let lost = not_targeting(&run, &ids, &minions, 0, d + 1..k);
    assert!(lost.is_empty(), "keep: let go of the Knight, (spawn, tick) with D={d}, K={k}: {lost:?}");
}

#[test]
fn walking_minions_keep_the_target_until_the_eta_reaches_600_ms() {
    // The slow-arrow scene: the Knight attacks a Goblin Cage 8500 from the tower, so the arrow flies long.
    let spawns: [Spawn; 5] = [
        (Team::Red, "Knight", (14500, 15300), Some(60)),
        (Team::Blue, "GoblinCage", (14500, 13500), None),
        (Team::Blue, "Minions", (10500, 10500), None),
        (Team::Blue, "Minions", (10000, 11000), None),
        (Team::Blue, "Minions", (11000, 10000), None),
    ];
    let (ids, run) = play(DoomedTargetDrop::ProjectileAttackers, &spawns, 40);
    let (d, k) = doom(&run, 0, 0);
    let eta = |t: usize| (k - t) * TICK_MS;
    assert!(eta(d) > ETA_LIMIT_MS, "precondition: the arrow's ETA at launch is {} ms, so this scene cannot test the gate", eta(d));
    let drop = (d + 1..k).find(|&t| eta(t - 1) <= ETA_LIMIT_MS).expect("the ETA never reached 600 ms");
    let minions = [2, 3, 4];
    let lost = not_targeting(&run, &ids, &minions, 0, d + 1..drop);
    assert!(lost.is_empty(), "let go of the Knight while its lethal damage was more than 600 ms away, (spawn, tick): {lost:?}");
    let held = targeting(&run, &ids, &minions, 0, drop..k);
    assert!(held.is_empty(), "kept the Knight after the ETA read 600 ms on {}, (spawn, tick): {held:?}", drop - 1);
}

#[test]
fn a_minion_that_has_fired_at_its_target_keeps_it_once_it_is_doomed() {
    // The Minion is in reach from the start and spits first (150 -> 43); then the tower's arrow dooms the Knight.
    let spawns: [Spawn; 2] = [(Team::Red, "Knight", (14500, 9150), Some(150)), (Team::Blue, "Minions", (11000, 9150), None)];
    for arm in [DoomedTargetDrop::ProjectileAttackers, DoomedTargetDrop::Keep] {
        let (ids, run) = play(arm, &spawns, 40);
        let landed = (1..run.len()).find(|&t| run[t].hp[0] < 150).expect("nothing hit the Knight");
        let (d, k) = doom(&run, 0, landed);
        assert!(run[landed].hp[0] > 0, "precondition: the first hit killed the Knight");
        let lost = not_targeting(&run, &ids, &[1], 0, 1..k);
        assert!(lost.is_empty(), "{arm:?}: the Minion that fired let go of the Knight, (spawn, tick) with D={d}, K={k}: {lost:?}");
    }
}

#[test]
fn a_knight_with_no_projectile_keeps_a_doomed_target() {
    let spawns: [Spawn; 2] = [(Team::Red, "Knight", (14500, 9150), Some(60)), (Team::Blue, "Knight", (11000, 11000), None)];
    for arm in [DoomedTargetDrop::ProjectileAttackers, DoomedTargetDrop::Keep] {
        let (ids, run) = play(arm, &spawns, 40);
        let (d, k) = doom(&run, 0, 0);
        assert!(not_targeting(&run, &ids, &[1], 0, d..d + 1).is_empty(), "{arm:?}: precondition: the blue Knight was not after the red one at D");
        let lost = not_targeting(&run, &ids, &[1], 0, d + 1..k);
        assert!(lost.is_empty(), "{arm:?}: the blue Knight let go of the doomed Knight, (spawn, tick) with D={d}, K={k}: {lost:?}");
    }
}

/// A red Knight walking down the right lane and a blue Minion 5000 to its left: the Minion begins its swing in range
/// and spits once the Knight has walked out of its reach (Range 2500 + radii 1000 + 25), so it re-evaluates on the
/// next tick. The spit (107) dooms a 60-hp Knight and not a 300-hp one.
fn lane(knight_hp: i32) -> [Spawn; 2] {
    [(Team::Red, "Knight", (14500, 20000), Some(knight_hp)), (Team::Blue, "Minions", (9500, 20000), None)]
}

#[test]
fn a_rescan_never_takes_a_doomed_unit_the_attacker_has_shot_at() {
    let (ids, run) = play(DoomedTargetDrop::ProjectileAttackersRescan, &lane(60), 45);
    let (d, k) = doom(&run, 0, 0);
    assert!(d + 1 < k, "precondition: the Knight died on {k}, before the tick after the spit ({})", d + 1);
    assert!(not_targeting(&run, &ids, &[1], 0, d..d + 1).is_empty(), "precondition: the Minion was not after the Knight at D={d}");
    let held = targeting(&run, &ids, &[1], 0, d + 1..k);
    assert!(held.is_empty(), "took the doomed Knight back, (spawn, tick) with D={d}, K={k}: {held:?}");
    // projectile_attackers, the old arm: its fired-at exemption covers the rescan, so the Knight is taken back on D+1
    let (ids, run) = play(DoomedTargetDrop::ProjectileAttackers, &lane(60), 45);
    let (d, _) = doom(&run, 0, 0);
    assert!(not_targeting(&run, &ids, &[1], 0, d + 1..d + 2).is_empty(), "projectile_attackers: let go of the Knight on D+1={}", d + 1);
    // control: a Knight the spit does not doom is taken back under the new arm
    let (ids, run) = play(DoomedTargetDrop::ProjectileAttackersRescan, &lane(300), 45);
    let d = (0..run.len()).find(|&t| run[t].shots_at[0] > 0).expect("no shot ever flew at the Knight: the scene drifted");
    assert!(not_targeting(&run, &ids, &[1], 0, d + 1..d + 2).is_empty(), "control: let go of a Knight that was not doomed on {}", d + 1);
}

/// Cases 6 and 7, tests/test_doomed_target_walk_drop.py's scenes: a red Knight walks down the right lane to the blue
/// right princess tower and a blue Skeleton Dragon attacks it. In case 6 the dragon, from (9351, 12287), spits first
/// from beyond its keep reach, so it re-evaluates, keeps the Knight and walks after it; the tower's first arrow then
/// joins the spit in flight, which dooms a 248-hp Knight and not a 400-hp one.
fn walk_scene(knight_hp: i32) -> [Spawn; 2] {
    [(Team::Red, "Knight", (14231, 10500), Some(knight_hp)), (Team::Blue, "SkeletonDragons", (9351, 12287), None)]
}

/// Case 7: the dragon, from (9551, 11787), spits once in reach and stays in its attack while the 330-hp Knight walks
/// on out of its keep reach, where the tower's second arrow dooms it (70 hp against 109).
const ATTACK_SCENE: [Spawn; 2] = [(Team::Red, "Knight", (14231, 11000), Some(330)), (Team::Blue, "SkeletonDragons", (9551, 11787), None)];

/// Is spawn `a` farther than the dragon's keep reach on the Knight (Range + both radii +
/// LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET, from the loaded cards and the shipped ledger) from spawn `b` after tick `t`?
fn beyond_keep_reach(run: &[Tick], t: usize, a: usize, b: usize) -> bool {
    let db = cards();
    let dragon = db.get(db.index("SkeletonDragons").expect("SkeletonDragons not simulable"));
    let knight = db.get(db.index("Knight").expect("Knight not simulable"));
    let keep = (dragon.range + dragon.collision_radius + knight.collision_radius + Calib::shipped().range_extension_to_keep_target) as i64;
    let (p, q) = (run[t].pos[a], run[t].pos[b]);
    let (dx, dy) = ((p.x - q.x) as i64, (p.y - q.y) as i64);
    dx * dx + dy * dy > keep * keep
}

/// Case 6's preconditions: (L, D), the dragon's first spit, from beyond its keep reach, and the tick an arrow joins it
/// in flight, with the dragon walking after the Knight in between and still beyond its keep reach on D.
fn walk_preconditions(run: &[Tick], ids: &[EntityId]) -> (usize, usize) {
    let l = (1..run.len()).find(|&t| run[t].phase[1] == Some(AttackPhase::Cooldown)).expect("the dragon never fired: the scene drifted");
    assert!(run[l].shots_at[0] == 1 && run[l].target[1] == Some(ids[0]), "precondition: the spit on {l} is not the only shot at the Knight");
    assert!(beyond_keep_reach(run, l, 0, 1), "precondition: the spit on {l} left from within the dragon's keep reach");
    let d = (l + 1..run.len()).find(|&t| run[t].shots_at[0] == 2).expect("precondition: no arrow ever joined the spit in flight");
    let off: Vec<usize> = (l + 1..=d).filter(|&t| run[t].phase[1] != Some(AttackPhase::Idle) || run[t].target[1] != Some(ids[0])).collect();
    assert!(off.is_empty(), "precondition: the dragon was not walking after the Knight on {off:?} (spit {l}, arrow {d})");
    assert!(beyond_keep_reach(run, d, 0, 1), "precondition: on the arrow's tick {d} the dragon is within its keep reach");
    (l, d)
}

/// (D, K): the Knight is gone on K and D is the last tick a shot at it was launched, so the shots in flight on D
/// killed it; they land within the 600 ms gate.
fn realised_doom(run: &[Tick]) -> (usize, usize) {
    let k = (0..run.len()).find(|&t| !run[t].alive[0]).expect("the Knight never died: the scene drifted");
    let d = (1..k).rfind(|&t| run[t].shots_at[0] > run[t - 1].shots_at[0]).expect("no shot ever flew at the Knight");
    assert!((k - d) * TICK_MS <= ETA_LIMIT_MS, "precondition: the lethal shots land {} ms after D={d}", (k - d) * TICK_MS);
    assert!(d + 1 < k, "precondition: the Knight died on {k}, before the tick after the doom ({})", d + 1);
    (d, k)
}

#[test]
fn a_walking_attacker_drops_a_doomed_target_it_has_shot_at() {
    let (ids, run) = play(DoomedTargetDrop::ProjectileAttackersWalkDrop, &walk_scene(248), 45);
    let (_, arrow) = walk_preconditions(&run, &ids);
    let (d, k) = realised_doom(&run);
    assert_eq!(d, arrow, "precondition: the Knight was doomed on {d}, not on the arrow's tick {arrow}");
    let held = targeting(&run, &ids, &[1], 0, d + 1..k);
    assert!(held.is_empty(), "the walking dragon kept (or took back) the doomed Knight, (spawn, tick) with D={d}, K={k}: {held:?}");
    // projectile_attackers_rescan, the old arm: its fired-at exemption covers every keep, so the dragon keeps it on D+1
    let (ids, run) = play(DoomedTargetDrop::ProjectileAttackersRescan, &walk_scene(248), 45);
    let (_, d) = walk_preconditions(&run, &ids);
    assert!(not_targeting(&run, &ids, &[1], 0, d + 1..d + 2).is_empty(), "projectile_attackers_rescan: let go of the Knight on D+1={}", d + 1);
    // control: a Knight the spit and the arrow do not doom is kept by the walking dragon under the new arm
    let (ids, run) = play(DoomedTargetDrop::ProjectileAttackersWalkDrop, &walk_scene(400), 45);
    let (_, d) = walk_preconditions(&run, &ids);
    assert!(not_targeting(&run, &ids, &[1], 0, d + 1..d + 2).is_empty(), "control: let go of a Knight that was not doomed on {}", d + 1);
}

#[test]
fn an_attacker_in_its_attack_keeps_a_doomed_target_beyond_its_keep_reach() {
    for arm in [DoomedTargetDrop::ProjectileAttackersWalkDrop, DoomedTargetDrop::ProjectileAttackersRescan] {
        let (ids, run) = play(arm, &ATTACK_SCENE, 45);
        let (d, k) = realised_doom(&run);
        let fired = (1..d).any(|t| run[t].phase[1] == Some(AttackPhase::Cooldown) && run[t].target[1] == Some(ids[0]));
        assert!(fired, "{arm:?}: precondition: the dragon had not fired at the Knight before D={d}");
        assert!(run[d].phase[1] != Some(AttackPhase::Idle), "{arm:?}: precondition: the dragon was not in its attack on D={d}");
        assert!(beyond_keep_reach(&run, d, 0, 1), "{arm:?}: precondition: on D={d} the dragon is within its keep reach");
        let lost = not_targeting(&run, &ids, &[1], 0, d + 1..k);
        assert!(lost.is_empty(), "{arm:?}: the dragon in its attack let go of the doomed Knight, (spawn, tick) with D={d}, K={k}: {lost:?}");
    }
}

#[test]
fn the_shipped_value_is_projectile_attackers_walk_drop() {
    assert_eq!(Calib::shipped().doomed_target_drop, DoomedTargetDrop::ProjectileAttackersWalkDrop);
}
