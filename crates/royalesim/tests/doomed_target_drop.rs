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
//!      does not doom is taken back under both (client 15.535.29: 4 of 4 such launches were followed by a drop).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test doomed_target_drop`):
//!   * `doomed_eta_ignored` -- damage counts whenever it lands: (2) goes red.
//!   * `doomed_drop_ignores_fired` -- an attacker that has fired drops it too: (3) goes red.
//!   * `doomed_drop_every_attacker` -- an attacker with no projectile drops it too: (4) goes red.
//!   * `doomed_rescan_takes_fired` -- a rescan takes back a doomed unit the attacker has shot at: (5) goes red.
mod common;

use common::*;
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

#[test]
fn the_shipped_value_is_projectile_attackers_rescan() {
    assert_eq!(Calib::shipped().doomed_target_drop, DoomedTargetDrop::ProjectileAttackersRescan);
}
