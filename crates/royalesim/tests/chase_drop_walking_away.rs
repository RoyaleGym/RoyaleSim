//! targeting.CHASE_DROP_WALKING_AWAY: which troops past the chase-drop limit a chaser lets go and a rescan passes over
//! (target.rs `decide`, `scan_with`, `walks_away`), under targeting.CHASE_DROP_RANGE = client_sight_minus_1000.
//!
//! THE READING (parity, round 9 item 32 corrected): the client lets a held troop go past the limit only when it walks
//! away, and passes over a troop past the limit in a rescan only when it walks away (sweep-RoyalHogs t353: a Hog 6,071
//! away, walking away, not taken). Pinned here, each with its precondition:
//!   1. both arms: a Hog Rider outrunning a Knight up the lane is let go on the tick it crosses the Knight's limit;
//!   2. client_walking_away: a rescan passes over a Knight past the limit that walks away; any_target takes it;
//!   3. both arms: a rescan takes a Knight past the limit that is still deploying (not walking away);
//!   4. the shipped arm is any_target.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test chase_drop_walking_away`):
//!   chase_drop_any_growth  every troop past the limit counts as walking away, whatever it does: (3) goes red under
//!                          client_walking_away.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, ChaseDropRange, ChaseDropWalkingAway};
use royalesim::Team;

const ARMS: [ChaseDropWalkingAway; 2] = [ChaseDropWalkingAway::AnyTarget, ChaseDropWalkingAway::ClientWalkingAway];

fn with_arm(arm: ChaseDropWalkingAway) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.chase_drop_range = ChaseDropRange::ClientSightMinus1000;
    cfg.calib.chase_drop_walking_away = arm;
    cfg
}

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Knight's chase-drop limit and plain sight on a Knight, native: SightRange + both radii (- 1000).
fn knight_limits(s: &BattleState) -> (i32, i32) {
    let cards = &s.config().cards;
    let k = cards.get(cards.index("Knight").expect("Knight loads"));
    let sight = (k.sight_range + 2 * k.collision_radius) / K;
    (sight - 1000, sight)
}

#[test]
fn a_runner_walking_away_is_let_go_on_the_edge_under_both_arms() {
    for arm in ARMS {
        let mut s = BattleState::new(0, with_arm(arm));
        let ids = s.scenario_spawn_batch(&[(Team::Red, "Knight", n(14500, 12500), None), (Team::Blue, "HogRider", n(14500, 9500), None)]).unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
        let (knight, hog) = (ids[0], ids[1]);
        let mut held = false;
        let mut let_go = None;
        for tick in 1..=120u32 {
            s.tick();
            let target = s.entity(knight).and_then(|k| k.target);
            if target == Some(hog) {
                held = true;
            } else if held {
                let_go = Some(tick);
                break;
            }
        }
        assert!(held, "{arm:?}: the scene drifted: the Knight never held the Hog Rider");
        let t = let_go.unwrap_or_else(|| panic!("{arm:?}: the Knight never let the running Hog Rider go"));
        let (k, h) = (s.entity(knight).unwrap(), s.entity(hog).unwrap());
        let d = h.pos.sub(k.pos);
        assert!(d.y.abs() / K > knight_limits(&s).0 - 300, "{arm:?}: let go on {t} at {} native, short of the limit", d.y.abs() / K);
    }
}

/// A Blue Knight at (9000, 13250) put down deployed, and a Red Knight about 6,000 behind it near (9000, 7000), past the
/// Blue one's chase-drop limit and within its plain sight: whether the Blue Knight holds it after the first tick, whose
/// Target phase reads them both. `deploying` plays the Red one with its deploy timer running; else it is put down
/// deployed and walks away from the first tick.
fn takes_the_knight_behind(arm: ChaseDropWalkingAway, deploying: bool) -> bool {
    let mut s = BattleState::new(0, with_arm(arm));
    past_deploy_lockout(&mut s);
    let (limit, sight) = knight_limits(&s);
    let chaser = s.scenario_spawn_now(Team::Blue, "Knight", n(9000, 13250), None).expect("the chaser");
    let (red, start) = if deploying {
        s.spawn_unit(Team::Red, "Knight", n(9000, 7000), None).expect("the Red Knight");
        s.tick();
        let r = s.entities().find(|e| e.team == Team::Red && e.card == "Knight").expect("the Red Knight is on the board");
        (r.id, r.pos)
    } else {
        let id = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 7000), None).expect("the Red Knight");
        let start = s.entity(id).unwrap().pos;
        s.tick();
        (id, start)
    };
    let d = start.sub(s.entity(chaser).unwrap().pos);
    let m = d.x.abs().max(d.y.abs()) / K;
    assert!(limit < m && m < sight, "the scene drifted: {m} is not between the limit {limit} and plain sight {sight}");
    let r = s.entity(red).unwrap();
    assert_eq!(r.deploy_ms > 0, deploying, "the scene drifted: the Red Knight's deploy");
    assert!(r.facing.y < 0, "the scene drifted: the Red Knight does not face away from the chaser");
    s.entity(chaser).unwrap().target == Some(red)
}

#[test]
fn a_rescan_passes_over_a_troop_past_the_limit_that_walks_away() {
    assert!(takes_the_knight_behind(ChaseDropWalkingAway::AnyTarget, false), "any_target: the rescan did not take the Knight at plain sight");
    assert!(!takes_the_knight_behind(ChaseDropWalkingAway::ClientWalkingAway, false), "client_walking_away: the rescan took a Knight past the limit that walks away");
}

#[test]
fn a_rescan_takes_a_troop_past_the_limit_that_does_not_walk() {
    for arm in ARMS {
        assert!(takes_the_knight_behind(arm, true), "{arm:?}: the rescan passed over a deploying Knight past the limit");
    }
}

#[test]
fn the_shipped_arm_is_any_target() {
    assert_eq!(Calib::shipped().chase_drop_walking_away, ChaseDropWalkingAway::AnyTarget);
}
