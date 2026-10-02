//! A HELD UNIT FACES ITS WAYPOINT (calibration movement.HELD_FACING; state.rs `phase_path16402_for`, the held unit's
//! speed-0 update, `held_faces_waypoint`). Measured on client 15.535.29: 143 of the 163 facing turns of held units point
//! at the route's next waypoint (sp-il-db5f t263: a frozen Hog Rider, its knockback ladder just ended, turns toward its
//! kept route's waypoint, and the next tick's avoidance scan looks along the new facing).
//!
//! The scene: Blue's Hog Rider walks its route on red's half toward the princess tower; a red Zap holds it. On the
//! first held tick it is put 700 to the side, as a push would; on the next held tick its facing is, under
//! client15535_toward_waypoint, the direction from its new point to its next waypoint (to 1/256), and under unchanged
//! the facing it had.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! held_facing`): held_facing_unchanged -> `a_held_unit_turns_its_facing_toward_its_waypoint` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, HeldFacing};
use royalesim::Team;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The scene under `arm`: (the facing before the push, the facing on the held tick after it, the direction from the
/// pushed point to the next waypoint).
fn held_turn(arm: HeldFacing) -> ((i32, i32), (i32, i32), (i32, i32)) {
    let mut cfg = config();
    let deck: Vec<String> = ["HogRider", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.held_facing = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let hog = s.scenario_spawn_now(Team::Blue, "HogRider", at(14500, 18000), None).expect("the Hog Rider");
    let mut walking = false;
    for _ in 0..60 {
        s.tick();
        let h = s.entity(hog).expect("the Hog Rider");
        if !h.deploying && !h.route.is_empty() && h.pos.y > 18500 * K {
            walking = true;
            break;
        }
    }
    assert!(walking, "the scene drifted: the Hog Rider never walked a route");
    let p = s.entity(hog).unwrap().pos;
    s.spawn_unit(Team::Red, "Zap", p, None).expect("the Zap");
    for _ in 0..10 {
        let was = s.entity(hog).expect("the Hog Rider").pos;
        s.tick();
        let h = s.entity(hog).expect("the Hog Rider lives");
        if h.pos == was {
            // the first held tick: put it 700 to the side, as a push would
            let before = (h.facing.x, h.facing.y);
            let moved = Vec2::new(h.pos.x + 700 * K, h.pos.y);
            assert!(s.debug_set_pos(hog, moved));
            s.tick();
            let h = s.entity(hog).expect("the Hog Rider lives");
            assert_eq!(h.pos, moved, "the scene drifted: the Hog Rider was not held on the tick after the push");
            let wp = *h.route.last().expect("the held Hog Rider keeps its route");
            let (dx, dy) = (i64::from(wp.x / K - moved.x / K), i64::from(wp.y / K - moved.y / K));
            let len = isqrt(dx * dx + dy * dy).max(1);
            return (before, (h.facing.x, h.facing.y), ((256 * dx / len) as i32, (256 * dy / len) as i32));
        }
    }
    panic!("the scene drifted: the Zap never held the Hog Rider");
}

#[test]
fn a_held_unit_turns_its_facing_toward_its_waypoint() {
    let (before, got, want) = held_turn(HeldFacing::Client15535TowardWaypoint);
    assert!((got.0 - want.0).abs() <= 1 && (got.1 - want.1).abs() <= 1, "held facing {got:?}, the waypoint gives {want:?}");
    assert!((before.0 - want.0).abs() > 1, "the push did not part the facing from the waypoint ({before:?} against {want:?})");
}

#[test]
fn unchanged_leaves_a_held_units_facing() {
    let (before, got, _) = held_turn(HeldFacing::Unchanged);
    assert_eq!(got, before, "the held unit's facing moved under unchanged");
}
