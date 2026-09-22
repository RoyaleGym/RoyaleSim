//! THE JumpEnabled WATER HOP AS MEASURED (CR 16.402): how a Hog Rider, a Prince or a
//! Royal Hog crosses the river.
//!
//! EVIDENCE
//!     The five river hops of live capture 20260920-002736 (Hog Rider, Prince, three
//!     Royal Hogs), replayed step for step: tests/jump16402.rs runs them from
//!     tests/fixtures/oracle2026/client16402_jumps.json, and the Hog Rider's and the
//!     Prince's battles end to end through the engine with every published node list.
//!     Ledger: calibration.json movement.JUMP_WATER_HOP = client16402, with the losing
//!     alternatives and their scores.
//!
//! THE LAW. A JumpEnabled troop (Hog Rider, and on 16.402 the Prince, Dark Prince, Ram
//! and Royal Hog) plans like any walker -- its search prices water at WATER_COST 7
//! instead of BLOCKED 50 (path16402.rs `cell_cost_for`), which on the shipped arena
//! still routes it over a bridge from every position the corpus starts a hop from --
//! and the hop is a property of the WALK, not of the plan:
//!
//!   * TRIGGER: on the tick a waypoint is reached and popped, if the card is
//!     JumpEnabled, at least two nodes remain and the NEW last node -- the next
//!     waypoint -- is a water cell, the list is replaced by ONE node: the first
//!     non-water node scanning from the node after it toward the goal, or the goal
//!     itself when every remaining node is water. The segment is refreshed from the
//!     unit's POST-move position, the distance from that position to the node's centre
//!     is the length of the leap, and the unit enters the leap (movement state 5 in
//!     the captures). The trigger tick itself is an ordinary walking tick (the step
//!     before the pop was a walk step at Speed).
//!
//!   * THE LEAP, every tick while in state 5: no path request, no avoidance scan (but
//!     the offset still decays), and the unit is NON-COLLIDABLE for everyone (its own
//!     separation scan is skipped and every neighbour's skips it). `speed` = JumpSpeed
//!     raw (no buff, no charge multiplier; 0 under a movement hold, in which case the
//!     unit hangs). Then the ordinary step law of move16402.rs toward the node's
//!     centre, facing set, the avoidance rotation applied to whatever offset is left,
//!     and the LANDING test on the POST-move position: `n_rem = tdiv(dist(centre, pos'),
//!     JumpSpeed) <= 1` -> the walk resumes and requests a fresh path from the landing
//!     position at once. The unit lands wherever that leaves it -- the third Royal Hog
//!     of the corpus lands ON a water cell (9, 30) and walks off it. No pop and no
//!     reached test in state 5.
//!
//!   * THE ARC. JumpHeight draws the visual parabola only; it touches no position, so
//!     it is not modelled.
//!
//! OPEN (recorded on the ledger key): the attack side of a leap (the engine holds the
//! jumper's attack and keeps it targetable), and a movement hold on a jumper (the
//! engine cancels the hop and replans when the hold ends -- no capture has a stunned
//! jumper).
//!
//! UNITS: native millitiles throughout, like path16402.rs and move16402.rs.
#![allow(unexpected_cfgs)]

use crate::move16402::{distance, tdiv};
use crate::path16402::CELL;

/// The cell centre the single node is written from (`col * 500 + 250`).
#[inline]
pub fn cell_centre(col: i32, row: i32) -> (i32, i32) {
    (col * CELL + CELL / 2, row * CELL + CELL / 2)
}

/// THE TRIGGER'S SCAN, on a list that has just been popped. `nodes` is goal first,
/// `nodes[len - 1]` the next waypoint. Returns the landing node when the hop fires:
/// `None` unless at least two nodes remain and the next waypoint is water; then the
/// first non-water node walking from `nodes[len - 2]` down to `nodes[0]`, or `nodes[0]`
/// when they are all water.
pub fn landing_node(nodes: &[(i32, i32)], is_water: impl Fn(i32, i32) -> bool) -> Option<(i32, i32)> {
    let n = nodes.len();
    if n < 2 {
        return None;
    }
    let next = nodes[n - 1];
    if !is_water(next.0, next.1) {
        return None;
    }
    let mut k = n - 2;
    loop {
        let c = nodes[k];
        if !is_water(c.0, c.1) {
            return Some(c);
        }
        if k == 0 {
            return Some(nodes[0]); // every remaining node is water
        }
        k -= 1;
    }
}

/// THE LANDING TEST of a leaping tick, on the POST-move position:
/// `tdiv(dist(centre, pos'), JumpSpeed) <= 1`. `jump_speed` is JumpSpeed raw, native
/// units per tick, and is positive here (a zero speed returns before the move).
#[inline]
pub fn landed(pos: (i32, i32), centre: (i32, i32), jump_speed: i32) -> bool {
    debug_assert!(jump_speed > 0);
    #[cfg(clash_plant = "jump_never_lands")]
    {
        // PLANT (regression): the reached flag decides, as for a walk -- the unit
        // never comes down (proj <= 1000 is never consulted in state 5).
        let _ = (pos, centre, jump_speed);
        return false;
    }
    #[allow(unreachable_code)]
    {
        tdiv(distance(pos.0, pos.1, centre.0, centre.1), jump_speed) <= 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_landing_node_is_the_first_dry_node_past_the_water_run_or_the_goal() {
        // rows 30..=33 are water; nodes goal first
        let water = |_c: i32, r: i32| (30..=33).contains(&r);
        let nodes = [(27, 40), (27, 34), (27, 33), (26, 32), (25, 31), (24, 30), (23, 29)];
        // next waypoint (23, 29) is dry: no hop
        assert_eq!(landing_node(&nodes, water), None);
        // popped: next waypoint (24, 30) is water; (25, 31), (26, 32), (27, 33) water too
        // (col 27 is the bridge live, but this grid says water) -> (27, 34)
        assert_eq!(landing_node(&nodes[..6], water), Some((27, 34)));
        // the bridge cell (27, 33) dry -> it is the landing node (the corpus's Hog, Prince and first Royal Hog)
        let bridge = |c: i32, r: i32| (30..=33).contains(&r) && !(27..=28).contains(&c);
        assert_eq!(landing_node(&nodes[..6], bridge), Some((27, 33)));
        // every remaining node water -> the goal
        assert_eq!(landing_node(&[(24, 30), (25, 31)], water), Some((24, 30)));
        // fewer than two nodes: never
        assert_eq!(landing_node(&[(24, 30)], water), None);
    }

    #[test]
    fn the_landing_test_is_two_jump_speeds_short_of_the_centre_after_the_move() {
        // the corpus's third Royal Hog: (4506, 15067) is 315 from (4250, 15250) -> lands;
        // (4636, 14975) is 474 away -> one more leap tick (JumpSpeed 160)
        assert!(landed((4506, 15067), (4250, 15250), 160));
        assert!(!landed((4636, 14975), (4250, 15250), 160));
        // its second: 321 away is NOT landed, 162 is
        assert!(!landed((13529, 16516), (13750, 16750), 160));
        assert!(landed((13639, 16632), (13750, 16750), 160));
    }
}
