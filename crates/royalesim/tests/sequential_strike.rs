//! THE SEQUENTIAL STRIKE (match.TICK_ORDER = client_sequential_strike; state.rs `phase_target_attack_sequential`):
//! the Target and Attack phases as one pass over the units in creation order, a direct strike landing at once.
//! (1) A WALKER created after a striker reads its kill on the kill tick, one created before it on the next (client
//!     15.535.29: 11 of 11 and 9 of 9 in the ledger's events; 114 of 124 and 72 of 75 over every unit death with one
//!     landed blow).
//! (2) AN ATTACKING UNIT reads the pass as it began: a kill earlier in the pass reaches it on the next tick, so its
//!     post-kill wait runs as under client16402 (client 15.535.29: 187 of 187 attackers created after the striker,
//!     on the sixth frame after the kill, none on the fifth).
//! (3) A UNIT STRUCK DOWN EARLIER IN THE PASS still takes its turn: its strike lands on the frame it dies (3 of 3 on the
//!     16.402 corpus, 2 of 2 on client 15.535.29).
//!
//! The scenes: Knights on Blue's half, out of every tower's reach. A Red Knight of 100 hitpoints is the victim; Blue's
//! first Knight kills it with its first blow.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! sequential_strike`): attacker_reads_pass_kills -> `an_attackers_post_kill_wait_runs_as_under_client16402` red;
//! struck_victim_skips_turn -> `a_unit_struck_down_in_the_pass_still_strikes` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, TickOrder};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Knight", "Archers", "Giant", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle(order: TickOrder) -> BattleState {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), DECK.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.tick_order = order;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s
}

fn knight(s: &mut BattleState, team: Team, p: (i32, i32), hp: Option<i32>) -> EntityId {
    s.scenario_spawn_now(team, "Knight", at(p.0, p.1), hp).expect("a Knight")
}

/// Tick until `v` is gone; the tick count it took.
fn until_gone(s: &mut BattleState, v: EntityId, max: u32) -> u32 {
    for n in 1..=max {
        s.tick();
        if s.entity(v).is_none() {
            return n;
        }
    }
    panic!("the scene drifted: the victim stood {max} ticks");
}

#[test]
fn a_walker_reads_a_kill_on_its_tick_when_created_after_the_striker() {
    // (striker first?, the walker's target at the end of the kill tick is still the victim?)
    for (striker_first, keeps) in [(true, false), (false, true)] {
        let mut s = battle(TickOrder::ClientSequentialStrike);
        let (k1, w) = if striker_first {
            let k1 = knight(&mut s, Team::Blue, (9000, 12100), None);
            (k1, knight(&mut s, Team::Blue, (9000, 9000), None))
        } else {
            let w = knight(&mut s, Team::Blue, (9000, 9000), None);
            (knight(&mut s, Team::Blue, (9000, 12100), None), w)
        };
        let v = knight(&mut s, Team::Red, (9000, 13600), Some(100));
        let mut before = None;
        for _ in 0..80 {
            before = s.entity(w).and_then(|e| e.target);
            s.tick();
            if s.entity(v).is_none() {
                break;
            }
        }
        assert!(s.entity(v).is_none(), "the scene drifted: the victim stood");
        assert_eq!(before, Some(v), "the scene drifted: the walker was not walking at the victim");
        let wv = s.entity(w).expect("the walker");
        assert_eq!(wv.attack_phase, AttackPhase::Idle, "the scene drifted: the walker reached the victim");
        assert!(s.entity(k1).is_some());
        assert_eq!(wv.target == Some(v), keeps, "striker created first: {striker_first}; the walker's target {:?}", wv.target);
    }
    // client16402: the walker reads the kill on the next tick, whatever the order.
    let mut s = battle(TickOrder::Client16402);
    knight(&mut s, Team::Blue, (9000, 12100), None);
    let w = knight(&mut s, Team::Blue, (9000, 9000), None);
    let v = knight(&mut s, Team::Red, (9000, 13600), Some(100));
    until_gone(&mut s, v, 80);
    assert_eq!(s.entity(w).expect("the walker").target, Some(v), "client16402: the kill reaches the walker a tick late");
}

/// The second attacker's (pos, target, attack phase, progress) from the tick before the kill to 12 after, under `order`.
fn second_attacker(order: TickOrder) -> Vec<(Vec2, Option<EntityId>, AttackPhase, i32)> {
    let mut s = battle(order);
    let k1 = knight(&mut s, Team::Blue, (8300, 12200), None);
    let v = knight(&mut s, Team::Red, (9000, 13300), Some(100));
    for _ in 0..8 {
        s.tick();
    }
    let k2 = knight(&mut s, Team::Blue, (9700, 12200), None);
    let mut prev = None;
    for _ in 0..80 {
        let e = s.entity(k2).expect("the second Knight");
        prev = Some((e.target, e.attack_phase, s.entity(k1).map(|k| k.attack_ms)));
        s.tick();
        if s.entity(v).is_none() {
            break;
        }
    }
    assert!(s.entity(v).is_none(), "the scene drifted: the victim stood");
    let (t, ph, _) = prev.expect("ticked");
    assert_eq!(t, Some(v), "the scene drifted: the second Knight was not on the victim before the kill");
    assert_ne!(ph, AttackPhase::Idle, "the scene drifted: the second Knight was not attacking before the kill");
    let mut out = Vec::new();
    for _ in 0..12 {
        let e = s.entity(k2).expect("the second Knight");
        out.push((e.pos, e.target, e.attack_phase, e.attack_ms));
        s.tick();
    }
    out
}

#[test]
fn an_attackers_post_kill_wait_runs_as_under_client16402() {
    let seq = second_attacker(TickOrder::ClientSequentialStrike);
    let old = second_attacker(TickOrder::Client16402);
    assert!(seq.iter().any(|r| r.1.is_none()), "the scene drifted: the second Knight never let go of the corpse");
    assert_eq!(seq, old, "an attacker created after the striker: the sequential order against client16402, tick by tick from the kill");
}

/// (Blue's Knight's hp at the end of the kill tick, its max hp), Blue's Knight created first, both Knights in reach of
/// each other from their first tick, so both blows fall on one tick and Blue's kills.
fn duel(order: TickOrder) -> (i32, i32) {
    let mut s = battle(order);
    let k = knight(&mut s, Team::Blue, (9000, 12100), None);
    let v = knight(&mut s, Team::Red, (9000, 13600), Some(100));
    until_gone(&mut s, v, 80);
    let e = s.entity(k).expect("Blue's Knight");
    (e.hp, e.max_hp)
}

#[test]
fn a_unit_struck_down_in_the_pass_still_strikes() {
    let (hp, max) = duel(TickOrder::ClientSequentialStrike);
    assert!(hp < max, "the Red Knight struck down on its own strike tick did not strike: Blue's Knight at {hp} of {max}");
    assert_eq!(duel(TickOrder::Client16402).0, hp, "the same blow under client16402");
}
