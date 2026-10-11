//! formation.LINE_LANE: the lane a line's members are laid on (state.rs `formation_members_with`, formation.rs
//! `member_offset`'s `mirror`), the tap's or that of the place `line_centre` found.
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, sp-rrtap-s0-9000-7000 and -9000-7500: Blue's Royal Recruits
//! tapped on the tile x 9500 and placed on (8500, 8500) stood with the left end of their zigzag high (y 8750), the left
//! lane's, as every line centred left of the middle does; the engine laid the tap's lane, the left end low (8250).
//!
//! THE SCENE: Blue's Royal Recruits played on (9000, 7500), the six members read on their first frame, by x.
//!
//! WHAT IS PINNED:
//!   1. the scene: the line placed on y 8500 with its members 250 either side, and its centre left of the middle;
//!   2. client15535_line_centre: the leftmost member high (8750); tap (the engine's, the vacuity check): low (8250);
//!   3. a tap the place search leaves in its lane, (9000, 8500): both arms lay the same line.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="line_lane_from_tap"' CARGO_TARGET_DIR=target/plant cargo test --test line_lane`):
//!   * `line_lane_from_tap` -- the new arm still lays the line on the tap's lane: (2) goes red.
//!   * `line_members_flipped` -- targeting.SUMMON_LANE_FLIP_SCOPE = client16402_rings_only still flips a line member's lane:
//!     `under_client16402_rings_only_a_line_member_keeps_its_own_lane` goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, LineLane, SummonLaneFlipScope};
use royalesim::Team;

/// The Royal Recruits' (x, y) (native, by x) after Blue's play on `at` under `arm`.
fn line(arm: LineLane, at: (i32, i32)) -> Vec<(i32, i32)> {
    let mut cfg: BattleConfig = config();
    cfg.calib.line_lane = arm;
    let deck: Vec<String> = ["RoyalRecruits", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|s| s.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(15, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "RoyalRecruits", Vec2::new(at.0 * K, at.1 * K)).expect("the play");
    let mut out = Vec::new();
    for _ in 0..30 {
        s.tick();
        out = s.entities().filter(|e| e.team == Team::Blue && e.card.starts_with("RoyalRecruit")).map(|e| (e.pos.x / K, e.pos.y / K)).collect();
        if out.len() >= 6 {
            break;
        }
    }
    assert_eq!(out.len(), 6, "the scene drifted: six recruits ({out:?})");
    out.sort();
    out
}

/// Plant: line_lane_from_tap.
#[test]
fn a_line_moved_over_the_middle_takes_its_centres_lane_under_client15535_line_centre() {
    let new = line(LineLane::Client15535LineCentre, (9000, 7500));
    let mean_x = new.iter().map(|p| p.0).sum::<i32>() / 6;
    assert!(new.iter().all(|p| (p.1 - 8500).abs() == 250) && mean_x < 9000, "the scene drifted: the line is not on y 8500 left of the middle ({new:?})");
    assert_eq!(new[0].1, 8750, "client15535_line_centre: the left end of the zigzag ({new:?})");
    let old = line(LineLane::Tap, (9000, 7500));
    // NOT VACUOUS: the tap's lane lays the left end low.
    assert_eq!(old[0].1, 8250, "tap: the left end of the zigzag ({old:?})");
}

/// A line the place search leaves in its lane is laid alike under both arms.
#[test]
fn a_line_kept_in_its_lane_is_laid_alike() {
    assert_eq!(line(LineLane::Client15535LineCentre, (9000, 8500)), line(LineLane::Tap, (9000, 8500)));
}

/// Blue's Royal Recruits played on (10500, 8500) under targeting.SUMMON_LANE_FLIP_SCOPE = `arm`: the leftmost member's x on
/// its first frame and 80 ticks later (native).
fn leftmost_recruit_walk(arm: SummonLaneFlipScope) -> (i32, i32) {
    let mut cfg: BattleConfig = config();
    cfg.calib.summon_lane_flip_scope = arm;
    let mut s = BattleState::new(5, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "RoyalRecruits", Vec2::new(10500 * K, 8500 * K), None).expect("the play");
    s.tick();
    let first = s.entities().filter(|e| e.team == Team::Blue && e.card.starts_with("RoyalRecruit")).min_by_key(|e| e.pos.x).map(|e| (e.id, e.pos.x / K)).expect("a recruit");
    for _ in 0..80 {
        s.tick();
    }
    (first.1, s.entity(first.0).expect("the recruit").pos.x / K)
}

/// targeting.SUMMON_LANE_FLIP_SCOPE = client16402_rings_only (client 16.402, parity's r71 census: the line members the flip
/// rule names kept their own lane, 3 of 3; 1,031 of the other 1,041 too): the leftmost Royal Recruit of a line centred on
/// x 10500, standing just right of the middle (x 9100), walks to its own right lane; under every_summon (the vacuity check)
/// its lane is flipped and it walks left. Plant: line_members_flipped.
#[test]
fn under_client16402_rings_only_a_line_member_keeps_its_own_lane() {
    let (x0, x_new) = leftmost_recruit_walk(SummonLaneFlipScope::Client16402RingsOnly);
    assert!((9050..=9150).contains(&x0), "the scene: the leftmost recruit stands at x {x0}, not on the flip band (9100)");
    assert!(x_new > x0 + 500, "client16402_rings_only: the recruit from x {x0} walked to x {x_new}, not to its own right lane");
    let (_, x_old) = leftmost_recruit_walk(SummonLaneFlipScope::EverySummon);
    assert!(x_old < x0 - 500, "every_summon (the vacuity check): the recruit from x {x0} walked to x {x_old}, not to the left lane");
}
