//! A CLONE'S COPY OF A DEPLOYING UNIT WAITS OUT ITS ORIGINAL'S DEPLOY -- calibration spells.CLONE_COPY_DEPLOY, state.rs
//! `materialise_clones`.
//!
//! THE READING (client_original_remaining, shipped at its old arm deployed): a copy is born with what is left of its
//! original's deploy, so a pair cloned mid-deploy takes its targets and walks when the original's deploy ends, not when
//! the Clone's hold ends. Read off client 15.535.29's sp-m5-clone-s0: three Skeletons played on t807 (deploying to
//! t827) are cloned on t812 (C); all six stand with no target through t826 (C + 14) and take their targets from t827
//! (C + 15), where the engine's copies, born deployed, took theirs and walked on t823 (C + 11), when the hold ends. The
//! only one of the 24 Clone casts in the scenario fixtures whose originals were still deploying, so the old arm ships.
//!
//! THE SCENE: Blue's own half, (9000, 9000), no enemy troop on the board. A Blue Knight is played there (it deploys its
//! 1,000 ms) and a Blue Clone is cast on it while it deploys; the copy appears on the Clone's cast tick C, and the pair
//! slides apart on C + 1..C + 10 (spells.CLONE_OFFSET). A walker with no enemy troop holds no target, so the observable
//! is the first step after the slide.
//!
//! WHAT IS PINNED (k counts the ticks after the Knight's play; C is k 6):
//!   1. both arms: the original leaves its deploy with 0 left on k 20 and first walks on k 21, C + 15;
//!   2. client_original_remaining: the copy is born with the original's 700 ms left and first walks on k 21 too;
//!   3. deployed (the old arm): the copy is born deployed and first walks on k 17, C + 11, when the hold ends;
//!   4. the shipped value is deployed.
//!
//! PLANT (regression):
//!   * `clone_copy_born_deployed` -- the new arm's copy is born deployed: (2) goes red.
//!     RUSTFLAGS='--cfg clash_plant="clone_copy_born_deployed"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!     --test clone_copy_deploy
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, CloneCopyDeploy};
use royalesim::Team;

const NEW: CloneCopyDeploy = CloneCopyDeploy::ClientOriginalRemaining;
const OLD: CloneCopyDeploy = CloneCopyDeploy::Deployed;
const AT: (i32, i32) = (9000, 9000);
/// The Clone is cast before the tick with this k (its Spawn phase casts it).
const CLONE_AT: u32 = 6;
/// The Clone's hold and slide: 10 ticks after the cast tick.
const SLIDE: u32 = 10;

fn with(arm: CloneCopyDeploy) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.clone_copy_deploy = arm;
    cfg
}

struct Run {
    /// k of the copy's first frame (the cast tick C).
    born: u32,
    /// The copy's deploy left on its first frame, ms.
    copy_deploy_ms: i32,
    /// The original's deploy left on k 20.
    original_deploy_at_20: i32,
    /// The first k after the slide on which each moved.
    original_walks: u32,
    copy_walks: u32,
}

/// Per tick after the tick: (k, the unit's position).
type Track = Vec<(u32, Vec2)>;

fn run(arm: CloneCopyDeploy) -> Run {
    let mut s = BattleState::new(0, with(arm));
    s.spawn_unit(Team::Blue, "Knight", Vec2::new(AT.0 * K, AT.1 * K), None).expect("play the Knight");
    let (mut original, mut copy, mut born, mut copy_deploy_ms, mut original_deploy_at_20) = (None, None, None, None, None);
    let (mut op, mut cp): (Track, Track) = (Vec::new(), Vec::new());
    for k in 1..50u32 {
        if k == CLONE_AT {
            let o = original.expect("the scene drifted: the Knight is not on the board before the Clone");
            let p = s.entity(o).unwrap().pos;
            s.spawn_unit(Team::Blue, "Clone", p, None).expect("cast the Clone");
        }
        s.tick();
        if original.is_none() {
            original = s.entities().find(|v| v.team == Team::Blue && v.card == "Knight" && !v.cloned).map(|v| v.id);
        }
        if copy.is_none() {
            if let Some(c) = s.entities().find(|v| v.cloned) {
                (copy, born, copy_deploy_ms) = (Some(c.id), Some(k), Some(c.deploy_ms));
            }
        }
        if let Some(v) = original.and_then(|o| s.entity(o)) {
            op.push((k, v.pos));
            if k == 20 {
                original_deploy_at_20 = Some(v.deploy_ms);
            }
        }
        if let Some(v) = copy.and_then(|c| s.entity(c)) {
            cp.push((k, v.pos));
        }
    }
    let born = born.expect("the scene drifted: no copy");
    let first_walk = |track: &[(u32, Vec2)], what: &str| {
        track.windows(2).find(|w| w[1].0 > born + SLIDE && w[1].1 != w[0].1).map(|w| w[1].0).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the {what} never walked"))
    };
    Run {
        born,
        copy_deploy_ms: copy_deploy_ms.unwrap(),
        original_deploy_at_20: original_deploy_at_20.expect("the scene drifted: the original is gone by k 20"),
        original_walks: first_walk(&op, "original"),
        copy_walks: first_walk(&cp, "copy"),
    }
}

#[test]
fn the_original_first_walks_when_its_deploy_ends_under_both_arms() {
    for arm in [NEW, OLD] {
        let r = run(arm);
        assert_eq!(r.born, CLONE_AT, "{arm:?}: the scene drifted: the copy's first frame");
        assert_eq!((r.original_deploy_at_20, r.original_walks), (0, 21), "{arm:?}: the original's deploy on k 20 and its first walk");
    }
}

/// Plant: clone_copy_born_deployed.
#[test]
fn the_new_arms_copy_waits_out_its_originals_deploy() {
    let r = run(NEW);
    assert_eq!(r.copy_deploy_ms, 700, "client_original_remaining: the copy's deploy left on its first frame");
    assert_eq!(r.copy_walks, 21, "client_original_remaining: the copy's first walk, C + 15 with the original");
}

#[test]
fn the_old_arms_copy_walks_when_the_hold_ends() {
    let r = run(OLD);
    assert_eq!(r.copy_deploy_ms, 0, "deployed: the copy's deploy left on its first frame");
    assert_eq!(r.copy_walks, CLONE_AT + SLIDE + 1, "deployed: the copy's first walk, C + 11");
}

#[test]
fn the_shipped_value_is_deployed() {
    assert_eq!(Calib::shipped().clone_copy_deploy, OLD);
}
