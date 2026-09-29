//! TWO MORE WAYS INTO AND THROUGH THE POST-KILL WAIT -- calibration combat.RETARGET_WAIT_REACH_LOSS and
//! combat.RETARGET_WAIT_WHILE_HELD, state.rs `phase_target` (combat.POST_KILL_RETARGET_WAIT's hold).
//!
//! THE WAIT (combat.POST_KILL_RETARGET_WAIT, shipped): a unit whose target died holds with no target and does not walk
//! for five Target phases from the one that finds the loss, its attack progress zeroed on the fifth, and takes the
//! decision's target on the sixth.
//!
//! REACH LOSS (client_after_reach_loss, shipped at its old arm kill_only): a unit whose row sets VariableDamage (the
//! Inferno Dragon), in its attack, whose LIVE target has left its attack reach, and which the decision neither keeps nor
//! replaces by an enemy already in that reach, starts the same wait, its progress 0 at once. Read off the 16.402 corpus, 20260920-082459 (both seats): an Inferno Dragon loses
//! a Giant that walks out of its reach on t3012, reads no target t3012..t3016, and takes the Skeletons on t3017 without
//! moving; the engine took them on t3012, out of its reach, and flew at them. One event. It refutes the reading that
//! takes only an enemy already in reach: the Skeletons stand inside the Dragon's reach on them (3,500 + 500 + 500) from
//! the t3013 frame, so that reading takes them on t3014.
//! A new target already in reach is taken at once: client 15.535.29's reach-loss scenarios show a Knight whose Hog Rider
//! ran out of its reach switch to a Cannon in its reach on the next tick, its swing unbroken. And a unit without the
//! inferno retargets at once: on the 16.402 corpus a Spear Goblin and a Skeleton whose targets left their reach took
//! the next at once, and the arm read on every unit lost 3,207 within 250 there.
//!
//! HELD (client_paused, shipped at its old arm runs_through): the wait counts only Target phases on which its unit is not
//! held (a stun or a freeze). Read off client 15.535.29's sp-scene-b-s1: a princess tower that lost its target on t316
//! and was frozen t317..t338 read progress 0 on t343 and took its next target on t344; the engine ran the wait through the
//! freeze and took it on t340, the tick after. One event.
//!
//! THE SCENES:
//!   * reach: a Blue Inferno Dragon at (9000, 13500) burning a Red Giant held 1,500 south of it; on k 40 the Giant is
//!     set down 4,800 south (25 past the Dragon's keep reach on it), alive, and walks on away. A Red Cannon due north
//!     is the nearer enemy then, 100 outside the Dragon's reach on it (or, for the null, 100 inside). The Cannon hits
//!     ground only, so nothing touches the Dragon;
//!   * held: a Blue Knight at (9000, 12500), out of every crown tower's reach, with a Red Skeleton 900 north of it and a
//!     Red Knight 2,000 north. The Blue Knight kills the Skeleton; on the tick after, a Red Freeze is cast on it, which
//!     holds it through the rest of its wait and beyond; the Red Knight, which walks up to it, is its next target.
//!
//! WHAT IS PINNED (L: the first tick the unit reads no target after the loss):
//!   1. reach, kill_only: the Dragon's next target is the Cannon on L itself, and it flies at it;
//!   2. reach, client_after_reach_loss: no target on L..L + 4 and no move, the Cannon on L + 5;
//!   3. reach, the null: with the Cannon in reach both arms take it on L;
//!   4. reach, the scope: a Knight in the Dragon's place (no VariableDamage), with the Giant and the Cannon set to its
//!      reach, takes the Cannon out of its reach on L under both arms;
//!   5. held, runs_through: the Knight's next target lands on the first tick after the freeze;
//!   6. held, client_paused: it lands the unfrozen ticks the wait still owed later, pinned on the tick;
//!   7. both shipped values are the old arms.
//!
//! PLANTS (regression):
//!   * `reach_loss_no_wait` -- the new arm retargets at once after a reach loss: (2) goes red.
//!   * `reach_loss_any_unit` -- every unit waits after a reach loss, not the inferno's alone: (4) goes red.
//!   * `retarget_wait_runs_while_held` -- the new arm counts the held ticks: (6) goes red.
//!     RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!     retarget_wait_arms
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, RetargetWaitReachLoss, RetargetWaitWhileHeld};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Dragon's reach on the Cannon (Range + both radii, native), and the tick count at which the Giant is moved away.
const CANNON_REACH: i32 = 3500 + 500 + 600;
const MOVED_AT: usize = 40;
/// Where the Dragon stands.
const DRAGON: (i32, i32) = (9000, 13500);

/// Per tick after the tick: (the attacker's target, its position, whether the Giant is on the board).
type ReachRows = Vec<(Option<EntityId>, Vec2, bool)>;

/// Per tick after the tick: (the attacker's target, its position, whether the Giant is on the board), and the Giant's and
/// the Cannon's ids. The attacker stands on DRAGON; the Giant is held 1,500 south of it, then on MOVED_AT set down
/// `giant_at` south of it, where it walks on south, away; the Cannon stands at `cannon`, an offset from the attacker.
fn reach_scene(arm: RetargetWaitReachLoss, attacker: &str, giant_at: i32, cannon: (i32, i32)) -> (ReachRows, EntityId, EntityId) {
    let mut cfg: BattleConfig = config();
    cfg.calib.retarget_wait_reach_loss = arm;
    let mut s = BattleState::new(0, cfg);
    let d = s.scenario_spawn_now(Team::Blue, attacker, at(DRAGON), None).expect("the attacker");
    let g = s.scenario_spawn_now(Team::Red, "Giant", at((DRAGON.0, DRAGON.1 - 1500)), None).expect("the Giant");
    let c = s.scenario_spawn_now(Team::Red, "Cannon", at((DRAGON.0 + cannon.0, DRAGON.1 + cannon.1)), None).expect("the Cannon");
    let mut rows = Vec::new();
    for k in 0..90 {
        let spot = if k < MOVED_AT { (DRAGON.0, DRAGON.1 - 1500) } else { (DRAGON.0, DRAGON.1 - giant_at) };
        if k <= MOVED_AT {
            s.debug_set_pos(g, at(spot));
        }
        s.tick();
        let v = s.entity(d).expect("the scene drifted: the attacker died");
        rows.push((v.target, v.pos, s.entity(g).is_some()));
    }
    (rows, g, c)
}

/// The Dragon's scene: the Giant set down 4,800 away (25 past its keep reach on it, 3,500 + 500 + 750 + 25 = 4,775)
/// and the Cannon due north, `cannon_off` past the Dragon's reach on it.
fn dragon_scene(arm: RetargetWaitReachLoss, cannon_off: i32) -> (ReachRows, EntityId, EntityId) {
    reach_scene(arm, "InfernoDragon", 4800, (0, CANNON_REACH + cannon_off))
}

/// L: the first row after one holding the Giant whose target is not the Giant; the Giant is still on the board there.
fn reach_loss(rows: &ReachRows, g: EntityId, what: &str) -> usize {
    let first = rows.iter().position(|r| r.0 == Some(g)).unwrap_or_else(|| panic!("{what}: the scene drifted: the Dragon never took the Giant"));
    let l = first + rows[first..].iter().position(|r| r.0 != Some(g)).unwrap_or_else(|| panic!("{what}: the scene drifted: the Dragon never lost the Giant"));
    assert!(rows[l].2, "{what}: the scene drifted: the Giant died on L = {l} (a kill, not a reach loss)");
    assert_eq!(l, MOVED_AT, "{what}: the scene drifted: the loss is not on the tick after the Giant was moved");
    l
}

#[test]
fn the_old_arm_takes_the_cannon_on_the_reach_loss() {
    let (rows, g, c) = dragon_scene(RetargetWaitReachLoss::KillOnly, 100);
    let l = reach_loss(&rows, g, "kill_only");
    assert_eq!(rows[l].0, Some(c), "kill_only: the target on L = {l}");
    assert_ne!(rows[l + 1].1, rows[l].1, "kill_only: the Dragon does not fly at the Cannon out of its reach");
}

/// The null: with the Cannon inside the Dragon's reach at the loss, the new arm switches at once, as the old one does
/// (and as client 15.535.29's reach-loss scenarios show a Knight doing with a Cannon in reach).
#[test]
fn a_new_target_in_reach_is_taken_at_once_under_both_arms() {
    for arm in [RetargetWaitReachLoss::KillOnly, RetargetWaitReachLoss::ClientAfterReachLoss] {
        let (rows, g, c) = dragon_scene(arm, -100);
        let l = reach_loss(&rows, g, "in reach");
        assert_eq!(rows[l].0, Some(c), "{arm:?}: the target on L = {l}, the Cannon in reach");
    }
}

/// Plant: reach_loss_no_wait.
#[test]
fn the_new_arm_waits_five_ticks_then_takes_the_cannon_where_it_stands() {
    let (rows, g, c) = dragon_scene(RetargetWaitReachLoss::ClientAfterReachLoss, 100);
    let l = reach_loss(&rows, g, "client_after_reach_loss");
    let targets: Vec<Option<EntityId>> = rows[l..l + 6].iter().map(|r| r.0).collect();
    assert_eq!(targets, vec![None, None, None, None, None, Some(c)], "client_after_reach_loss: the targets on L..L + 5 (L = {l})");
    assert!(rows[l - 1..l + 5].iter().all(|r| r.1 == rows[l - 1].1), "client_after_reach_loss: the Dragon moved during the wait");
    assert_ne!(rows[l + 5].1, rows[l + 4].1, "client_after_reach_loss: the Dragon does not fly at the Cannon out of its reach on L + 5");
}

/// Plant: reach_loss_any_unit. A Knight in the Dragon's place: the Giant set down 2,550 away (75 past its keep reach,
/// 1,200 + 500 + 750 + 25), and the Cannon 2,400 east, 100 past its reach on it (1,200 + 500 + 600) and nearer than the
/// Giant. The Knight has no VariableDamage: it takes the Cannon on L under both arms, and walks at it.
#[test]
fn a_unit_without_the_inferno_takes_the_cannon_at_once_under_both_arms() {
    for arm in [RetargetWaitReachLoss::KillOnly, RetargetWaitReachLoss::ClientAfterReachLoss] {
        let (rows, g, c) = reach_scene(arm, "Knight", 2550, (2400, 0));
        let l = reach_loss(&rows, g, "Knight");
        assert_eq!(rows[l].0, Some(c), "{arm:?}: the Knight's target on L = {l}");
    }
}

/// Per tick after the tick: the Knight's target; and the ticks (k) the Knight is frozen.
fn held_scene(arm: RetargetWaitWhileHeld) -> (Vec<Option<EntityId>>, u32, Vec<u32>) {
    let mut cfg: BattleConfig = config();
    cfg.calib.retarget_wait_while_held = arm;
    let mut s = BattleState::new(0, cfg);
    let ids = s
        .scenario_spawn_batch(&[
            (Team::Blue, "Knight", at((9000, 12500)), None),
            (Team::Red, "Skeleton", at((9000, 13400)), None),
            (Team::Red, "Knight", at((9000, 14500)), None),
        ])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (knight, first) = (ids[0], ids[1]);
    let (mut rows, mut killed, mut frozen) = (Vec::new(), None, Vec::new());
    for k in 0..150u32 {
        if killed.is_some_and(|t| k == t + 1) {
            let p = s.entity(knight).unwrap().pos;
            s.spawn_unit(Team::Red, "Freeze", p, None).expect("cast the Freeze");
        }
        s.tick();
        let v = s.entity(knight).expect("the scene drifted: the Knight died");
        if v.stun_ms > 0 || v.speed_now == 0 {
            frozen.push(k);
        }
        rows.push(v.target);
        if killed.is_none() && s.entity(first).is_none() {
            killed = Some(k);
        }
    }
    (rows, killed.expect("the scene drifted: the Knight never killed the first Skeleton"), frozen)
}

/// (the kill's k, the first and last k the Knight reads frozen after the tick, the k of its next target).
fn held_timeline(arm: RetargetWaitWhileHeld) -> (u32, u32, u32, u32) {
    let (rows, killed, frozen) = held_scene(arm);
    let (first, last) = (*frozen.first().expect("the scene drifted: the Knight was never frozen"), *frozen.last().unwrap());
    assert_eq!(frozen.len() as u32, last - first + 1, "{arm:?}: the scene drifted: the freeze is not one run of ticks");
    let next = (killed as usize + 1..rows.len()).find(|&k| rows[k].is_some()).unwrap_or_else(|| panic!("{arm:?}: no next target")) as u32;
    (killed, first, last, next)
}

/// The Knight's victim dies on k 9 and the Freeze holds it from k 10 (its Target phase of k 10 runs before the Freeze
/// lands) through k 80's Target phase. The wait, found on k 10, would end on k 15, inside the freeze.
#[test]
fn the_old_arm_takes_the_next_target_when_the_freeze_ends() {
    assert_eq!(held_timeline(RetargetWaitWhileHeld::RunsThrough), (9, 10, 79, 81), "runs_through: (kill, frozen, next target)");
}

/// Plant: retarget_wait_runs_while_held. The wait counts k 10, pauses through the freeze and counts k 81..84: the next
/// target lands four ticks after the old arm's, as sp-scene-b-s1's tower took its Skeleton on t344 against t340.
#[test]
fn the_new_arm_serves_the_rest_of_the_wait_after_the_freeze() {
    assert_eq!(held_timeline(RetargetWaitWhileHeld::ClientPaused), (9, 10, 79, 85), "client_paused: (kill, frozen, next target)");
}

#[test]
fn the_shipped_values_are_the_old_arms() {
    let c = Calib::shipped();
    assert_eq!(c.retarget_wait_reach_loss, RetargetWaitReachLoss::KillOnly);
    assert_eq!(c.retarget_wait_while_held, RetargetWaitWhileHeld::RunsThrough);
}
