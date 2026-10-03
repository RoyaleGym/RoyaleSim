//! targeting.EQUAL_DISTANCE_TIE: which of two candidates at one ranked distance a scan takes first (target.rs `key`).
//!
//! The client's event (parity, round 9 item 36; the 16.402 corpus, sweep-GoblinDrill): a Goblin Drill at (9000, 21000)
//! makes a Goblin on (9000, 22000) exactly, both Red princess towers 6,519 from it, and the client's Goblin holds the
//! RIGHT tower on its first frame. Pinned here, each with its precondition:
//!   1. own_frame_high_x: that Goblin's first target is Red's right princess tower (14500, 25500);
//!   2. own_frame_low_x, the old arm: the left one (3500, 25500), the engine's first divergence there;
//!   3. both arms are seat-symmetric: a Red drill's Goblin on x 9000 takes the tower the Blue one's rotation names;
//!   4. the shipped arm is own_frame_high_x (since the 2026-09-28 round 9 arms flip);
//!   5. client15535_later_created (client 15.535.29, every exact troop tie at an acquisition, 5 of 5): a Musketeer
//!      between two enemy Knights mirrored about its x, the right one created first, takes the later (left) one, for
//!      either seat, where own_frame_high_x takes the right one for Blue.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test equal_distance_tie`):
//!   equal_distance_tie_low_x  the lower own-frame x first, whatever the arm: (1) goes red;
//!   tie_later_created_unread  the new arm ranks by the own frame: (5) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, EqualDistanceTie};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The first Goblin `team`'s drill at (9000, `drill_y`) makes: its creation point and the point of its first target.
fn first_goblin(arm: EqualDistanceTie, team: Team, drill_y: i32) -> (Vec2, Vec2) {
    let mut cfg = config();
    cfg.calib.equal_distance_tie = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(team, "GoblinDrill", n(9000, drill_y), None).expect("the drill");
    for _ in 0..400 {
        let before: Vec<_> = s.entities().filter(|e| e.team == team && e.card == "Goblin").map(|e| e.id).collect();
        s.tick();
        if let Some(g) = s.entities().find(|e| e.team == team && e.card == "Goblin" && !before.contains(&e.id)) {
            let target = g.target.and_then(|t| s.entity(t)).map(|t| t.pos).expect("the Goblin has a target on its first frame");
            // Its creation point: its first frame, less its first step, is not read here; the drill's wave point is.
            return (g.pos, target);
        }
    }
    panic!("the scene drifted: no Goblin in 400 ticks");
}

const BLUE_DRILL_Y: i32 = 21000;
const RED_DRILL_Y: i32 = 11000;

#[test]
fn a_blue_goblin_on_the_centre_line_takes_the_right_tower_under_own_frame_high_x() {
    let (at, target) = first_goblin(EqualDistanceTie::OwnFrameHighX, Team::Blue, BLUE_DRILL_Y);
    assert!((at.x - 9000 * K).abs() <= 250 * K, "the scene drifted: the Goblin's first frame is at x {}", at.x / K);
    assert_eq!(target, n(14500, 25500), "the Goblin on x 9000 did not take Red's right princess tower");
}

#[test]
fn the_old_arm_takes_the_left_tower() {
    let (_, target) = first_goblin(EqualDistanceTie::OwnFrameLowX, Team::Blue, BLUE_DRILL_Y);
    assert_eq!(target, n(3500, 25500), "own_frame_low_x: the Goblin on x 9000 did not take Red's left princess tower");
}

#[test]
fn both_arms_are_seat_symmetric() {
    for arm in [EqualDistanceTie::OwnFrameLowX, EqualDistanceTie::OwnFrameHighX] {
        let (_, blue) = first_goblin(arm, Team::Blue, BLUE_DRILL_Y);
        let (_, red) = first_goblin(arm, Team::Red, RED_DRILL_Y);
        // Red's frame is Blue's rotated half a turn: engine (x, y) -> (18000 - x, 32000 - y).
        assert_eq!(red, n(18000 - blue.x / K, 32000 - blue.y / K), "{arm:?}: the seats break the tie differently");
    }
}

#[test]
fn the_shipped_arm_is_own_frame_high_x() {
    assert_eq!(Calib::shipped().equal_distance_tie, EqualDistanceTie::OwnFrameHighX);
}

/// A `team` Musketeer on (9000, y0) and two enemy Knights 1,000 either side of its x and 4,000 ahead (in that team's
/// forward), the one on x 10000 created first, held there: the x of the Musketeer's first target.
fn musketeer_first_target_x(arm: EqualDistanceTie, team: Team) -> i32 {
    let mut cfg = config();
    cfg.calib.equal_distance_tie = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let (y0, ahead) = if team == Team::Blue { (9000, 13000) } else { (23000, 19000) };
    let foe = if team == Team::Blue { Team::Red } else { Team::Blue };
    let m = s.scenario_spawn_now(team, "Musketeer", n(9000, y0), None).expect("the Musketeer");
    let first = s.scenario_spawn_now(foe, "Knight", n(10000, ahead), None).expect("the first Knight");
    let later = s.scenario_spawn_now(foe, "Knight", n(8000, ahead), None).expect("the later Knight");
    for _ in 0..80 {
        for (id, x) in [(first, 10000), (later, 8000)] {
            assert!(s.debug_set_pos(id, n(x, ahead)));
        }
        assert!(s.debug_set_pos(m, n(9000, y0)));
        s.tick();
        if let Some(t) = s.entity(m).expect("the Musketeer").target {
            assert!(t == first || t == later, "the scene drifted: the Musketeer took another target");
            // which Knight, by id: each has walked from its held point by the end of the tick
            return if t == first { 10000 } else { 8000 };
        }
    }
    panic!("the scene drifted: the Musketeer took no target in 80 ticks");
}

/// Plant: tie_later_created_unread.
#[test]
fn a_tie_between_two_troops_goes_to_the_later_created() {
    assert_eq!(musketeer_first_target_x(EqualDistanceTie::Client15535LaterCreated, Team::Blue), 8000, "client15535_later_created, Blue");
    assert_eq!(musketeer_first_target_x(EqualDistanceTie::Client15535LaterCreated, Team::Red), 8000, "client15535_later_created, Red");
    assert_eq!(musketeer_first_target_x(EqualDistanceTie::OwnFrameHighX, Team::Blue), 10000, "own_frame_high_x, Blue: vacuous otherwise");
}
