//! combat.CHAIN_HOP_REACH: which units a chained shot's hop reaches from the unit it hit (combat.rs `chain_next`, the base
//! chain of card.rs `ChainHitDef`; the Electro Dragon's ChainedHitRadius 4000, centre to centre).
//!
//! THE LAW, measured on client 16.635 (parity r101, the oracle's il-mg-16635 set; the ledger has the rows): a unit at
//! exactly the radius is passed over, 2 of 2 (an Electro Dragon's hop 4000 and an Electro Spirit's 3000), 0 against.
//!
//! THE SCENE (tests/evo_chain_hop_reach.rs's, with the base Electro Dragon): Blue's Electro Dragon held on (14650, 15000),
//! a red Cannon held 3499 north of him (his target), and a second red Cannon held `off` west of the first, out of the
//! dragon's range, both topped up; a hit is a loss of more than 20 (a Cannon's lifetime takes 1 or 2 a tick). WHAT IS
//! PINNED:
//!   1. client16635_strict: the second Cannon at exactly 4000 is never hit in the 60 ticks after the first one's hit;
//!   2. inclusive (the old arm, the vacuity check): it is;
//!   3. at 3999 both arms hop to it (the reach itself works);
//!   4. the shipped value is inclusive.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test chain_hop_reach`):
//!   * `chain_hop_reach_inclusive` -- the new arm still reaches the radius: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, ChainHopReach};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`, the second Cannon `off` west of the first: whether it is hit within 60 ticks of the first one's hit.
fn second_hit(arm: ChainHopReach, off: i32) -> bool {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["ElectroDragon".into(), "Knight".into()], vec!["Cannon".into(), "Knight".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.chain_hop_reach = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    let at = n(14650, 15000);
    s.spawn_unit(Team::Blue, "ElectroDragon", at, None).expect("the Electro Dragon");
    s.tick();
    let ed = find_live(&s, Team::Blue, "ElectroDragon").first().expect("the Electro Dragon").id;
    let (kp, k2p) = (n(14650, 18499), n(14650 - off, 18499));
    let k1 = s.scenario_spawn_now(Team::Red, "Cannon", kp, None).expect("the first red Cannon");
    let k2 = s.scenario_spawn_now(Team::Red, "Cannon", k2p, None).expect("the second red Cannon");
    let mut first = None;
    for k in 0..260 {
        assert!(s.debug_set_pos(ed, at) && s.debug_set_pos(k1, kp) && s.debug_set_pos(k2, k2p));
        let (t1, t2) = (s.entity(k1).expect("k1").max_hp, s.entity(k2).expect("k2").max_hp);
        assert!(s.debug_set_hp(k1, t1) && s.debug_set_hp(k2, t2));
        s.tick();
        if s.entity(k2).expect("k2").hp < t2 - 20 {
            return true;
        }
        if first.is_none() && s.entity(k1).expect("k1").hp < t1 - 20 {
            first = Some(k);
        }
        if first.is_some_and(|f| k > f + 60) {
            return false;
        }
    }
    panic!("{arm:?}: the scene drifted: the first Cannon was never hit");
}

/// Plant: chain_hop_reach_inclusive.
#[test]
fn under_client16635_strict_a_hop_passes_over_a_unit_at_exactly_its_radius() {
    assert!(!second_hit(ChainHopReach::Client16635Strict, 4000), "client16635_strict: the Cannon at exactly 4000 was hit");
}

#[test]
fn under_inclusive_a_hop_reaches_the_radius() {
    assert!(second_hit(ChainHopReach::Inclusive, 4000), "inclusive (the vacuity check): the Cannon at exactly 4000 was never hit");
}

#[test]
fn both_arms_reach_inside_the_radius() {
    assert!(second_hit(ChainHopReach::Client16635Strict, 3999), "client16635_strict: the Cannon at 3999 was never hit");
    assert!(second_hit(ChainHopReach::Inclusive, 3999), "inclusive: the Cannon at 3999 was never hit");
}

/// combat.CHAIN_HOP_REACH ships at inclusive (client16635_strict is a candidate until scored).
#[test]
fn the_shipped_arm_is_inclusive() {
    assert_eq!(Calib::shipped().chain_hop_reach, ChainHopReach::Inclusive);
    assert_eq!(config().calib.chain_hop_reach, ChainHopReach::Inclusive);
}
