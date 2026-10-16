//! THE LOST TARGET'S SHOT (calibration combat.LOST_TARGET_SHOT; entity.rs `lost_target`; combat.rs `attack_step`; state.rs
//! phase_target_with's decision loop, `attack_recoil`), at level 11.
//!
//! THE MEASUREMENTS (client 16.402, parity's r74 batch-6 census of the ob13 set): a Firecracker whose target died in her first
//! attack fired at its point in 26 of 26 and recoiled from it; later attacks 0 of 308; homing shooters and the Bowler and
//! Princess (OverrideAttackFinishTime) do not. The engine cancelled 8 of 8.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! lost_target_shot`): lost_shot_cancelled -> `under_client16402_point_first_attack_fires_a_firecracker_fires_at_the_corpse`
//! red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, LostTargetShot};
use royalesim::{EntityId, Team};

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn step(a: (i32, i32), b: (i32, i32)) -> i64 {
    let (dx, dy) = (i64::from(a.0 - b.0), i64::from(a.1 - b.1));
    royalesim::fixed::isqrt(dx * dx + dy * dy)
}

/// One frame of the shooter: its point (native), its attack progress, its target, its shots in flight.
type Frame = ((i32, i32), i32, Option<EntityId>, usize);

/// A Blue `who` held on (9000, 14000), a Red Skeleton held 4000 north of it (its first target) and a Red Knight held on
/// (12000, 18000) (5000 off). When `who`'s first attack at the Skeleton reads its LoadTime + 250 (the full-credit entry,
/// progress + load timer = 2 x LoadTime within a tick), the Skeleton is set to 0 hp and `who` let go, under
/// combat.LOST_TARGET_SHOT = `arm`: its next 24 frames and the Knight.
fn lost(arm: LostTargetShot, who: &str) -> (Vec<Frame>, EntityId) {
    let mut cfg = config();
    cfg.calib.lost_target_shot = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let (fp, sp, kp) = (n((9000, 14000)), n((9000, 18000)), n((12000, 18000)));
    let fc = s.scenario_spawn_now(Team::Blue, who, fp, None).expect("the shooter");
    let sk = s.scenario_spawn_now(Team::Red, "Skeleton", sp, None).expect("the target");
    let kn = s.scenario_spawn_now(Team::Red, "Knight", kp, None).expect("the next target");
    let lt = card_stat(&s, who).load_time_ms;
    let mut at_kill = false;
    for _ in 0..400 {
        for (id, p) in [(fc, fp), (sk, sp), (kn, kp)] {
            assert!(s.debug_set_pos(id, p));
        }
        let e = s.entity(fc).expect("the shooter");
        if e.target == Some(sk) && e.attack_ms == lt + 250 && (e.attack_ms + e.attack_load_ms - 2 * lt).abs() <= 50 {
            at_kill = true;
            break;
        }
        s.tick();
    }
    assert!(at_kill, "{who}: the scene drifted: its first attack never read {} at the Skeleton", lt + 250);
    assert!(s.debug_set_hp(sk, 0));
    let shots = |s: &BattleState| s.projectiles().iter().filter(|p| p.firer == Some(fc)).count();
    let mut out = Vec::new();
    for _ in 0..24 {
        assert!(s.debug_set_pos(kn, kp));
        s.tick();
        let e = s.entity(fc).expect("the shooter");
        out.push(((e.pos.x / K, e.pos.y / K), e.attack_ms, e.target, shots(&s)));
    }
    (out, kn)
}

/// combat.LOST_TARGET_SHOT = client16402_point_first_attack_fires (client 16.402, parity's r74 batch-6 census: Firecrackers 26
/// of 26 fired at the corpse's point on their progress's HitSpeed, recoiled from it, no target until the hit + 6; the engine
/// cancelled 8 of 8): her swing runs on to its shot on the kill + 8 (2600 + 8 x 50 = 3000) with no target, she recoils straight
/// away from the corpse, holds no target through the hit + 5 and takes the Knight on + 6; under cancel (the vacuity check) her
/// swing is not the lost one: she takes a target inside it. Plant: lost_shot_cancelled.
#[test]
fn under_client16402_point_first_attack_fires_a_firecracker_fires_at_the_corpse() {
    let (r, kn) = lost(LostTargetShot::Client16402PointFirstAttackFires, "Firecracker");
    let f = r.iter().position(|x| x.3 > 0).unwrap_or_else(|| panic!("client16402_point_first_attack_fires: no shot: {r:?}"));
    assert_eq!(f, 7, "the shot on the kill + 8 (index 7): {r:?}");
    assert!((0..f).all(|k| r[k].2.is_none()), "no target through the swing: {r:?}");
    assert!((r[f].0 .0 - r[f - 1].0 .0).abs() <= 10 && r[f].0 .1 < r[f - 1].0 .1 && step(r[f].0, r[f - 1].0) >= 150, "the recoil straight away from the corpse (south): {:?}", &r[f - 1..f + 3]);
    assert!(r[..f + 6].iter().all(|x| x.2.is_none()), "no target through the hit + 5: {r:?}");
    assert_eq!(r[f + 6].2, Some(kn), "the Knight on the hit + 6: {r:?}");
    let (q, _) = lost(LostTargetShot::Cancel, "Firecracker");
    assert!(q[..f].iter().any(|x| x.2.is_some()), "cancel (the vacuity check): no target taken inside the swing: {q:?}");
}

/// The scope: a homing shooter (the Musketeer) and an OverrideAttackFinishTime point shooter (the Bowler) take no lost swing
/// under client16402_point_first_attack_fires (the census: 4 of 513, 0 of 8): each takes a target at once, as under cancel.
#[test]
fn under_client16402_point_first_attack_fires_a_homing_or_finish_override_shooter_still_cancels() {
    for who in ["Musketeer", "Bowler"] {
        let (r, _) = lost(LostTargetShot::Client16402PointFirstAttackFires, who);
        assert!(r[..3].iter().any(|x| x.2.is_some()), "{who}: held in a lost swing (no target) instead of cancelling it: {r:?}");
    }
}
