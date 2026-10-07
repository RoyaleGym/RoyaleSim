//! targeting.SNIPE_LOCK_RELEASE (item 292): when an Evo Musketeer lets her snipe target go as it comes near (state.rs
//! `snipe_pass`).
//!
//! Client 15.535.29 (sp-scene-c-s0-v2 t921, sp-m6d-s0 t1130): the snipe target was let go at 6,481 and 6,446 ahead, below
//! SnipeMinRange 6000 + its radius 500, her windup ran on, and her next shot was a plain one; no held step of a sniped run
//! came below 6,788. The engine let go only below 6000 and restarted her windup.
//!
//! The scene: she stands at (3499, 4000) with Blue's princess towers down; a red Knight put down 7,500 ahead (past her
//! ordinary reach, 6000 plus both radii) is taken as her snipe target, then set 6,300 ahead and held there. Under the new arm she lets it go and her first shot is plain
//! (not the snipe's Speed 2650); under the old arm she keeps it and her first shot is a snipe. Plant: snipe_release_at_min.
//!
//! (2) The released target held on as an ordinary one dies in her windup (sp-m6d-s0 t1135): she snipes the next one, a
//! second Knight held 9,500 ahead, at once, where the post-kill wait it started, never counted down under the snipe pass,
//! froze her for good. Plant: snipe_pick_keeps_kill_wait.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, SnipeLockRelease};
use royalesim::Team;

/// A native point, in subtiles.
fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`: whether her first shot after the Knight is set 6,300 ahead is a snipe (Speed 2650).
fn first_shot_is_snipe(arm: SnipeLockRelease) -> bool {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.snipe_lock_release = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    let form = s.cards().index("Musketeer_EV1").expect("the evolved Musketeer loads");
    let mult = s.config().calib.projectile_speed_to_subtiles_per_tick;
    let at = n(3499, 4000);
    s.spawn_unit(Team::Blue, "Musketeer_EV1", at, None).unwrap();
    // A play is on the board after the tick that resolves it.
    for _ in 0..5 {
        if s.entities().any(|e| e.card_idx == form) {
            break;
        }
        s.tick();
    }
    let mut knight = None;
    let mut near = false;
    for k in 0..120 {
        let m = s.entities().find(|e| e.card_idx == form).expect("the Musketeer").id;
        assert!(s.debug_set_pos(m, at));
        if k == 25 {
            knight = Some(s.scenario_spawn_now(Team::Red, "Knight", n(3499, 11500), None).expect("the Knight"));
        }
        if let Some(kn) = knight {
            assert!(s.debug_set_pos(kn, if near { n(3499, 10300) } else { n(3499, 11500) }));
        }
        s.tick();
        let me = s.entity(m).expect("the Musketeer");
        if !near && knight.is_some() && me.target == knight && me.attack_ms > 0 {
            near = true;
        }
        if near {
            if let Some(p) = s.projectiles().iter().find(|p| p.firer == Some(m)) {
                return p.speed / mult == 2650;
            }
        }
    }
    panic!("the scene drifted: no shot (near {near})");
}

/// Plant: snipe_release_at_min.
#[test]
fn a_snipe_target_inside_min_plus_radius_is_shot_plain_under_client15535_min_plus_radius_keep_windup() {
    assert!(!first_shot_is_snipe(SnipeLockRelease::Client15535MinPlusRadiusKeepWindup), "client15535_min_plus_radius_keep_windup: a snipe at 6,300 ahead");
    // NOT VACUOUS: the old arm keeps the lock at 6,300 (6000 or more ahead) and snipes.
    assert!(first_shot_is_snipe(SnipeLockRelease::AheadBelowMin), "ahead_below_min: a plain shot at 6,300 ahead");
}

/// (2) Under the new arm: the snipe she fires at a second Knight held 9,500 ahead after the first, released 6,300 ahead and
/// held on as an ordinary target, is killed in her windup; None if she fires nothing in 80 ticks.
fn snipe_after_held_target_dies() -> Option<bool> {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.snipe_lock_release = SnipeLockRelease::Client15535MinPlusRadiusKeepWindup;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    let form = s.cards().index("Musketeer_EV1").expect("the evolved Musketeer loads");
    let mult = s.config().calib.projectile_speed_to_subtiles_per_tick;
    let at = n(3499, 4000);
    s.spawn_unit(Team::Blue, "Musketeer_EV1", at, None).unwrap();
    for _ in 0..5 {
        if s.entities().any(|e| e.card_idx == form) {
            break;
        }
        s.tick();
    }
    let (mut first, mut second) = (None, None);
    let mut near = false;
    let mut near_at = 0;
    let mut killed: Option<u32> = None;
    for k in 0..200 {
        let m = s.entities().find(|e| e.card_idx == form).expect("the Musketeer").id;
        assert!(s.debug_set_pos(m, at));
        if k == 25 {
            first = Some(s.scenario_spawn_now(Team::Red, "Knight", n(3499, 11500), None).expect("the first Knight"));
            second = Some(s.scenario_spawn_now(Team::Red, "Knight", n(3499, 13500), None).expect("the second Knight"));
        }
        if let Some(kn) = first.filter(|_| killed.is_none()) {
            assert!(s.debug_set_pos(kn, if near { n(3499, 10300) } else { n(3499, 11500) }));
        }
        if let Some(kn) = second {
            assert!(s.debug_set_pos(kn, n(3499, 13500)));
        }
        s.tick();
        let me = s.entity(m).expect("the Musketeer");
        if !near && first.is_some() && me.target == first && me.attack_ms > 0 {
            near = true;
            near_at = s.tick_count();
        }
        // 4 ticks after it was set 6,300 ahead, released and held on as an ordinary target, in her windup: a Zap kills it
        if near && killed.is_none() && s.tick_count() == near_at + 4 {
            assert!(me.target == first && me.attack_ms > 0 && s.projectiles().iter().all(|p| p.firer != Some(m)), "the scene drifted: not in her windup on the first Knight ({:?}, {})", me.target, me.attack_ms);
            assert!(s.debug_set_hp(first.unwrap(), 1));
            s.spawn_unit(Team::Blue, "Zap", n(3499, 10300), None).expect("the Zap");
            killed = Some(s.tick_count());
        }
        if let Some(at_kill) = killed {
            if let Some(p) = s.projectiles().iter().find(|p| p.firer == Some(m)) {
                assert_eq!(Some(p.target), second, "her shot after the kill is not at the second Knight");
                return Some(p.speed / mult == 2650);
            }
            if s.tick_count() > at_kill + 80 {
                return None;
            }
        }
    }
    panic!("the scene drifted: the first Knight was never killed in her windup (near {near})");
}

/// Plant: snipe_pick_keeps_kill_wait.
#[test]
fn under_client15535_min_plus_radius_keep_windup_she_snipes_on_when_the_held_target_dies() {
    assert_eq!(snipe_after_held_target_dies(), Some(true), "client15535_min_plus_radius_keep_windup: no snipe at the second Knight after the first died");
}
