//! movement.KNOCKED_DOOMED_AVOIDANCE: whether a doomed troop whose knockback is under way in the move pass is static to
//! the avoidance scan (under movement.DYING_UNIT_VISIBILITY = client_doomed_static).
//!
//! THE LAW, measured on client 15.535.29: an Evo Cannon bomb lands before the move pass, and the troops it kills take
//! the first step of its push there and still push later movers by contact (tests/evolution.rs
//! `a_barrage_bomb_lands_before_the_move_pass`); but no scanner beside them turns on them: 8 of 8 kept their offset
//! (sp-m3-radius-s0 t986, t2787 to t2791), where the engine, meeting them as static obstacles, turned each +-190. Beside
//! every other death 148 of 153 scanners turned as the engine has them turn.
//!
//! WHAT IS PINNED, on that scene in miniature (deploying Red Skeletons killed by the 9000 bomb, a Red Knight created
//! after them just clear of the farthest one and facing it):
//!   1. under static the Knight starts an avoidance turn on the landing tick;
//!   2. under client15535_knocked_mover it keeps offset 0 there, and is still pushed out of the dying skeleton's stepped
//!      position by contact, as under static.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! knocked_doomed_avoidance`):
//!   * `knocked_doomed_static` -- the knocked doomed troop stays static under the new arm: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DyingUnitVisibility, KnockedDoomedAvoidance};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Knight's (avoidance offset, move) on the landing tick, and whether the far skeleton died there.
fn landing(arm: KnockedDoomedAvoidance) -> (i32, (i32, i32), bool) {
    let mut cfg = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dying_unit_visibility = DyingUnitVisibility::ClientDoomedStatic;
    cfg.calib.knocked_doomed_avoidance = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let play = s.tick_count();
    s.spawn_unit(Team::Blue, "Cannon_EV1", n(9000, 9500), None).unwrap();
    let bomb = n(9000, 18000);
    while s.tick_count() < play + 10 {
        s.tick();
    }
    s.spawn_unit(Team::Red, "Skeletons", n(9000, 19900), None).unwrap();
    while s.tick_count() < play + 20 {
        s.tick();
    }
    let skeletons: Vec<_> = s.entities().filter(|e| e.team == Team::Red && e.card == "Skeletons").map(|e| (e.id, e.pos)).collect();
    assert_eq!(skeletons.len(), 3, "the scene drifted: {skeletons:?}");
    let (far, at) = *skeletons.iter().max_by_key(|(_, p)| p.dist2(bomb)).unwrap();
    let (dx, dy) = ((at.x - bomb.x) / K, (at.y - bomb.y) / K);
    let len_milli = royalesim::fixed::isqrt((dx as i64 * dx as i64 + dy as i64 * dy as i64) * 1_000_000);
    let along = |d: i32| -> i32 {
        let num = d as i64 * 1010 * 1000 * 2;
        (if num >= 0 { (num + len_milli) / (2 * len_milli) } else { -((-num + len_milli) / (2 * len_milli)) }) as i32
    };
    // 1010 beyond the far skeleton along the bomb's line: clear of it, out of the bomb's reach, and facing it (Red's
    // forward is toward the bomb).
    let spot = Vec2::new(at.x + along(dx) * K, at.y + along(dy) * K);
    s.spawn_unit_resolved(Team::Red, "Knight", spot, None).unwrap();
    s.tick();
    let knight = s.entities().filter(|e| e.team == Team::Red && e.card == "Knight").map(|e| e.id).next().expect("the Knight");
    let mut before = None;
    while s.tick_count() < play + 26 {
        if s.tick_count() == play + 25 {
            let k = s.entity(knight).unwrap();
            assert_eq!(k.avoid_offset, 0, "the scene drifted: the Knight was turning before the landing");
            before = Some(k.pos);
        }
        s.tick();
    }
    let before = before.expect("the tick before the landing");
    let k = s.entity(knight).expect("the Knight lives");
    assert!(k.deploying && k.hp == k.max_hp, "the scene drifted: the Knight walked or the bomb reached it");
    (k.avoid_offset, ((k.pos.x - before.x) / K, (k.pos.y - before.y) / K), s.entity(far).is_none())
}

#[test]
fn a_barrage_victim_is_no_static_obstacle_to_the_avoidance_scan() {
    let (old_off, old_move, old_dead) = landing(KnockedDoomedAvoidance::Static);
    let (new_off, new_move, new_dead) = landing(KnockedDoomedAvoidance::Client15535KnockedMover);
    assert!(old_dead && new_dead, "the scene drifted: the bomb did not kill the far skeleton");
    assert_ne!(old_off, 0, "static: the Knight did not turn on the dying skeleton (precondition)");
    assert_eq!(new_off, 0, "client15535_knocked_mover: the Knight turned on a skeleton the barrage killed");
    assert_ne!(new_move, (0, 0), "client15535_knocked_mover: the dying skeleton no longer pushed the Knight by contact");
    assert_eq!(new_move, old_move, "the contact push differs between the arms");
}
