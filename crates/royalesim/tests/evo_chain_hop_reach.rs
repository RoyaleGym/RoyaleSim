//! combat.EVO_CHAIN_HOP_REACH: which units an Evo Electro Dragon's hop reaches from the unit it hit (combat.rs
//! `chain_next_remember`, ChainedHitRadius 4000 centre to centre).
//!
//! THE LAW, measured on client 15.535.29 (the ledger has the rows): a unit at exactly the radius is passed over
//! (sp-f4-ed-s0 t1165, an Ice Golem 4,000 off).
//!
//! THE SCENE (tests/evo_chain_shot_launch.rs's): Blue's evolved Electro Dragon held on (14650, 15000), a red Cannon held
//! 3499 north of him (his target), and a second red Cannon held `off` west of the first, out of the dragon's range,
//! both topped up; a hit is a loss of more than 20 (a Cannon's lifetime takes 1 or 2 a tick). Cannons, not Knights: a
//! held Knight still walks inside the tick, so the hop met it a little nearer than `off`. WHAT IS PINNED:
//!   1. client15535_strict: the second Cannon at exactly 4000 is never hit in the 60 ticks after the first one's hit;
//!   2. inclusive (the old arm, the vacuity check): it is;
//!   3. at 3999 both arms hop to it (the reach itself works);
//!   4. the shipped value is inclusive (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test evo_chain_hop_reach`):
//!   * `hop_reach_inclusive` -- the new arm still reaches the radius: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, EvoChainHopReach};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`, the second Cannon `off` west of the first: whether it is hit within 60 ticks of the first one's hit.
fn second_hit(arm: EvoChainHopReach, off: i32) -> bool {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["ElectroDragon".into(), "Knight".into()], vec!["Cannon".into(), "Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.evo_chain_hop_reach = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    let at = n(14650, 15000);
    s.spawn_unit(Team::Blue, "ElectroDragon_EV1", at, None).expect("the Electro Dragon");
    s.tick();
    let ed = find_live(&s, Team::Blue, "ElectroDragon_EV1").first().expect("the Electro Dragon").id;
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

#[test]
fn a_hop_passes_over_a_unit_at_exactly_its_radius_under_client15535_strict() {
    assert!(!second_hit(EvoChainHopReach::Client15535Strict, 4000), "new: the Cannon at exactly 4000 was hit");
}

#[test]
fn the_old_value_reaches_the_radius() {
    assert!(second_hit(EvoChainHopReach::Inclusive, 4000), "old: the Cannon at exactly 4000 was never hit (vacuous otherwise)");
}

#[test]
fn both_values_reach_inside_it() {
    assert!(second_hit(EvoChainHopReach::Client15535Strict, 3999), "new: the Cannon at 3999 was never hit");
    assert!(second_hit(EvoChainHopReach::Inclusive, 3999), "old: the Cannon at 3999 was never hit");
}

#[test]
fn the_shipped_value_is_inclusive() {
    assert_eq!(Calib::shipped().evo_chain_hop_reach, EvoChainHopReach::Inclusive);
}
