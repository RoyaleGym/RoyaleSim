//! A DEPLOYING NEIGHBOUR'S HEADING -- calibration movement.DEPLOYING_HEADING = kept.
//!
//! THE SCENE is the client 15.535.29 archer-alone scenario: two Blue Archers tapped on the tile centre
//! (12500, 13500), issued on tick 100. Both clients agree on every number pinned here (client 15.535.29
//! and four of four comparable 16.402 corpus events, three of them mirrored). The first member
//! leaves deploy on 120 and walks; the second is still deploying through 121 and leaves on 122.
//!
//!   frame 121   walker (12017,13556)   idle (13016,13500)   -- the scene reproduces client 15.535.29
//!   frame 122   walker step (31,51)    idle step (27,-3)    -- the walker goes STRAIGHT; the idle
//!                                                               member is pushed from the walker's
//!                                                               post-move position
//!   frame 123   idle step (79,17)
//!
//! Under the earlier reading (`zeroed`) the deploying member's heading counts for nothing, the
//! walker's avoidance counts it as a blocker and turns 72 degrees, (-39,45), and the idle member
//! is not pushed at all. The foil test pins that, so the pair only passes together if the key
//! switches the behaviour.
//!
//! THE MECHANISM, and why the premise is asserted. "A deploying unit keeps its forward heading"
//! and "deploying units are skipped by avoidance" agree whenever the two face the same way, as
//! here. A 15.535.29 scenario built to split them (a Knight chasing backward into a deploying Giant
//! that faces forward) steers at look reach, before contact, so the heading counts and the skip is
//! ruled out (ledger provenance). This test still asserts the premise it relies on -- the
//! deploying member faces the walker's way -- so a scene that stops meeting it fails rather than
//! passing for another reason.
mod common;

use common::{config, find_live};
use royalesim::fixed::Vec2;
use royalesim::state::{BattleConfig, BattleState, DeployingHeading};
use royalesim::{EntityId, Team};
use std::collections::BTreeMap;

const K: i32 = 18;

fn native(p: Vec2) -> (i32, i32) {
    (p.x / K, p.y / K)
}

struct Frame {
    walker: (i32, i32),
    idle: (i32, i32),
    idle_deploying: bool,
    walker_facing: Vec2,
    idle_facing: Vec2,
}

/// The 15.535.29 archer-alone scenario, frames 101..=124. The walker is the member that spawned at the
/// smaller x (15.535.29: 11999; the other at 13000).
fn archer_alone(cfg: BattleConfig) -> BTreeMap<u32, Frame> {
    let mut s = BattleState::new(0, cfg);
    while s.tick_count() < 100 {
        s.tick();
    }
    s.spawn_unit(Team::Blue, "Archer", Vec2::new(12500 * K, 13500 * K), Some(11)).expect("the Archers are issued");
    s.tick();
    let mut ids: Vec<(i32, EntityId)> = find_live(&s, Team::Blue, "Archer").iter().map(|e| (e.pos.x, e.id)).collect();
    assert_eq!(ids.len(), 2, "archer-alone deploys two Archers");
    ids.sort();
    let (walker, idle) = (ids[0].1, ids[1].1);
    let mut out = BTreeMap::new();
    while s.tick_count() <= 124 {
        let (w, i) = (s.entity(walker).expect("walker alive"), s.entity(idle).expect("idle member alive"));
        out.insert(
            s.tick_count(),
            Frame { walker: native(w.pos), idle: native(i.pos), idle_deploying: i.deploy_ms > 0, walker_facing: w.facing, idle_facing: i.facing },
        );
        s.tick();
    }
    out
}

fn step(f: &BTreeMap<u32, Frame>, t: u32, walker: bool) -> (i32, i32) {
    let (a, b) = if walker { (f[&(t - 1)].walker, f[&t].walker) } else { (f[&(t - 1)].idle, f[&t].idle) };
    (b.0 - a.0, b.1 - a.1)
}

#[test]
fn a_walker_goes_straight_past_a_same_facing_neighbour_that_is_still_deploying() {
    assert_eq!(config().calib.deploying_heading, DeployingHeading::Kept, "the shipped arm this test pins");
    let f = archer_alone(config());
    // THE SCENE, before the split: if these move, the numbers below are about another scene.
    assert_eq!((f[&121].walker, f[&121].idle), ((12017, 13556), (13016, 13500)), "the scene no longer reproduces client 15.535.29 on frame 121, before the split");
    // THE PREMISE the data cannot split without: on the tick the walker's move pass reads, the
    // idle member is still deploying AND faces the walker's way.
    let fr = &f[&121];
    assert!(fr.idle_deploying, "the idle member has finished deploying before tick 122's move pass; the scene no longer tests a DEPLOYING neighbour");
    let dot = fr.walker_facing.x as i64 * fr.idle_facing.x as i64 + fr.walker_facing.y as i64 * fr.idle_facing.y as i64;
    assert!(dot > 0, "the deploying member's facing {:?} does not face the walker's way {:?}; with dot <= 0 the key is moot here", fr.idle_facing, fr.walker_facing);
    // THE LAW: both clients, to the native unit.
    assert_eq!(step(&f, 122, true), (31, 51), "the walker's step on 122: both clients go straight");
    assert_eq!(step(&f, 122, false), (27, -3), "the idle member's step on 122: pushed 27 from the walker's post-move position, not a walk (TICK_ORDER 4: a unit does not walk on the tick it leaves deploy)");
    assert_eq!(step(&f, 123, false), (79, 17), "the idle member's step on 123");
}

#[test]
fn under_the_zeroed_foil_the_walker_turns_round_the_deploying_member() {
    let mut cfg = config();
    cfg.calib.deploying_heading = DeployingHeading::Zeroed;
    let f = archer_alone(cfg);
    assert_eq!((f[&121].walker, f[&121].idle), ((12017, 13556), (13016, 13500)), "the foil diverges before the split, so it is not the same scene");
    assert_eq!(step(&f, 122, true), (-39, 45), "the earlier reading's deflection (72 degrees) is gone; the foil no longer shows what the key replaced");
    assert_eq!(step(&f, 122, false), (0, 0), "under the foil the idle member is not pushed on 122");
}
