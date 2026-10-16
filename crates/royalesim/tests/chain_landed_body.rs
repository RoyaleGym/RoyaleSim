//! movement.CHAIN_LANDED_BODY: whether a dash chain's champion is a contact body between his last landing and his
//! chain's end (state.rs `chain_pass`'s `Scratch::chain_ended`, the move pass's `collidable`).
//!
//! THE LAW, measured on client 15.535.29 (the ledger has the rows): after the Golden Knight's chain's last landing on L,
//! with a unit overlapping him, he moved by no contact on L + 1 and L + 2 and was pushed from L + 3 (3 of 3).
//!
//! THE SCENE (tests/dash_chain_aim.rs's, Blue's princess towers down): a Blue Golden Knight held on (3294, 9000) until he
//! stands, a red Giant held on
//! (3420, 14000); pressed, he dashes at the Giant and lands his blow (L: the Giant's hitpoints fall); on L a blue Knight
//! is put down 300 beside him (overlapping him by 1,000; an ally: no hop takes it) and held there. WHAT IS PINNED, each
//! with its precondition (the blow lands):
//!   1. client15535_no_body_to_end: he does not move on L + 1 nor on L + 2, and the Knight pushes him on L + 3 (a body
//!      again once the chain has ended);
//!   2. landed_body (the old arm, the vacuity check): the Knight pushes him on one of them;
//!   3. the shipped value is landed_body (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test chain_landed_body`):
//!   * `chain_landed_is_body` -- the new arm's landed champion is a body at once: (1) goes red.
//!   * `chain_ended_stale` -- the ended chains' list outlives its tick: (1)'s L + 3 check goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, ChainLandedBody};
use royalesim::Team;

const DECK: [&str; 8] = ["GoldenKnight", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`: his moves (native) on the three ticks after his landing.
fn after_landing(arm: ChainLandedBody) -> [(i32, i32); 3] {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.chain_landed_body = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    // Blue's princess towers down: no tower shot reaches the Giant, so its first hp drop is his blow.
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    let gk = s.scenario_spawn_now(Team::Blue, "GoldenKnight", n(3294, 9000), None).expect("the Golden Knight");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", n(3420, 14000), None).expect("the Giant");
    for _ in 0..40 {
        assert!(s.debug_set_pos(gk, n(3294, 9000)) && s.debug_set_pos(giant, n(3420, 14000)));
        s.tick();
    }
    assert!(!s.entity(gk).expect("he lives").deploying, "{arm:?}: the scene drifted: he never stood up");
    assert!(s.debug_set_pos(gk, n(3294, 9000)) && s.debug_set_pos(giant, n(3420, 14000)));
    s.press_ability_button(Team::Blue, 0).expect("the press is taken");
    for _ in 0..30 {
        let top = s.entity(giant).expect("the Giant").max_hp;
        assert!(s.debug_set_pos(giant, n(3420, 14000)) && s.debug_set_hp(giant, top));
        s.tick();
        if s.entity(giant).expect("the Giant").hp < top {
            // L: the blow landed on this tick. A blue Knight put down 300 beside him, overlapping him, and held.
            let at = s.entity(gk).expect("he lives").pos;
            let ally = s.scenario_spawn_now(Team::Blue, "Knight", Vec2::new(at.x + 300 * K, at.y), None).expect("a blue Knight");
            let kp = s.entity(ally).expect("the Knight").pos;
            let mut moves = [(0, 0); 3];
            for m in &mut moves {
                let p0 = s.entity(gk).expect("he lives").pos;
                assert!(s.debug_set_pos(ally, kp) && s.debug_set_pos(giant, n(3420, 14000)));
                s.tick();
                let p1 = s.entity(gk).expect("he lives").pos;
                *m = ((p1.x - p0.x) / K, (p1.y - p0.y) / K);
            }
            return moves;
        }
    }
    panic!("{arm:?}: the scene drifted: his blow never landed");
}

#[test]
fn a_landed_champion_is_no_body_to_his_chains_end() {
    let moves = after_landing(ChainLandedBody::Client15535NoBodyToEnd);
    assert_eq!(moves[..2], [(0, 0), (0, 0)], "new: the landed Golden Knight moved by contact on L + 1 or L + 2: {moves:?}");
    assert_ne!(moves[2], (0, 0), "new: the Knight did not push him on L + 3, after his chain's end: {moves:?}");
}

#[test]
fn the_old_value_pushes_him_at_once() {
    let moves = after_landing(ChainLandedBody::LandedBody);
    assert_ne!(moves[0], (0, 0), "old: the overlapping Knight did not push the landed Golden Knight on L + 1: {moves:?}");
}

#[test]
fn the_shipped_value_is_client16402_no_body_in_poise() {
    assert_eq!(Calib::shipped().chain_landed_body, ChainLandedBody::Client16402NoBodyInPoise);
}

/// The scene with a second red Giant held on (3420, 16500), 2,500 beyond the first, so the chain hops on after its first blow, under `arm`: his
/// moves (native) on each tick after the first blow (L) until his next hop's first move (above 200), the Knight held on
/// him from L.
fn poise_moves(arm: ChainLandedBody) -> Vec<(i32, i32)> {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.chain_landed_body = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    let gk = s.scenario_spawn_now(Team::Blue, "GoldenKnight", n(3294, 9000), None).expect("the Golden Knight");
    let g1 = s.scenario_spawn_now(Team::Red, "Giant", n(3420, 14000), None).expect("the Giant");
    let g2 = s.scenario_spawn_now(Team::Red, "Giant", n(3420, 16500), None).expect("the second Giant");
    let hold = |s: &mut BattleState| {
        for (g, p) in [(g1, n(3420, 14000)), (g2, n(3420, 16500))] {
            let top = s.entity(g).expect("a Giant").max_hp;
            assert!(s.debug_set_pos(g, p) && s.debug_set_hp(g, top));
        }
    };
    for _ in 0..40 {
        assert!(s.debug_set_pos(gk, n(3294, 9000)));
        hold(&mut s);
        s.tick();
    }
    assert!(s.debug_set_pos(gk, n(3294, 9000)));
    s.press_ability_button(Team::Blue, 0).expect("the press is taken");
    for _ in 0..30 {
        let top = s.entity(g1).expect("the Giant").max_hp;
        hold(&mut s);
        s.tick();
        if s.entity(g1).expect("the Giant").hp < top {
            let at = s.entity(gk).expect("he lives").pos;
            let ally = s.scenario_spawn_now(Team::Blue, "Knight", Vec2::new(at.x + 300 * K, at.y), None).expect("a blue Knight");
            let kp = s.entity(ally).expect("the Knight").pos;
            let mut moves = Vec::new();
            let mut trace = Vec::new();
            for _ in 0..60 {
                let p0 = s.entity(gk).expect("he lives").pos;
                assert!(s.debug_set_pos(ally, kp));
                hold(&mut s);
                s.tick();
                let p1 = s.entity(gk).expect("he lives").pos;
                let m = ((p1.x - p0.x) / K, (p1.y - p0.y) / K);
                trace.push((native_pos(p1), s.entity(gk).expect("he lives").target));
                if m.0.abs().max(m.1.abs()) > 200 {
                    return moves;
                }
                moves.push(m);
            }
            panic!("{arm:?}: the scene drifted: no second hop within 60 ticks of his first blow (Giants {g1:?} {g2:?}; his point and target each tick: {trace:?})");
        }
    }
    panic!("{arm:?}: the scene drifted: his first blow never landed");
}

/// movement.CHAIN_LANDED_BODY = client16402_no_body_in_poise (client 16.402, parity's r74 census: unmoved on 135 of 135 Poise
/// ticks with an overlapping unit): between his first blow and his next hop the held Knight never moves him; under
/// client15535_no_body_to_end (the vacuity check) it pushes him in his Poise. Plant: chain_poise_is_body.
#[test]
fn under_client16402_no_body_in_poise_he_is_no_body_between_hops() {
    let old = poise_moves(ChainLandedBody::Client15535NoBodyToEnd);
    assert!(old.iter().any(|m| *m != (0, 0)), "client15535_no_body_to_end (the vacuity check): the Knight never moved him between hops: {old:?}");
    let new = poise_moves(ChainLandedBody::Client16402NoBodyInPoise);
    assert!(!new.is_empty() && new.iter().all(|m| *m == (0, 0)), "client16402_no_body_in_poise: he moved between hops: {new:?}");
}

/// A point, native.
fn native_pos(p: Vec2) -> (i32, i32) {
    (p.x / K, p.y / K)
}
