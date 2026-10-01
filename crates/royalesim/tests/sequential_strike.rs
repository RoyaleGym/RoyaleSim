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
//! (4) A WALKER WHOSE TARGET STANDS IN ITS ATTACK REACH reads the pass as it began, as an attacker does (client
//!     15.535.29: 8 walkers created after the striker read their attack and no target on the kill frame, then waited).
//! (5) THE PASS RUNS THE TICK'S OWN PASSES once before its first unit (the hide pass, the Evo Teslas' rings, the
//!     taunts), as the Target phase does: the hero Knight's taunt turns an enemy on the same tick under both orders.
//!
//! The scenes: Knights on Blue's half, out of every tower's reach. A Red Knight of 100 hitpoints is the victim; Blue's
//! first Knight kills it with its first blow.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! sequential_strike`): attacker_reads_pass_kills -> `an_attackers_post_kill_wait_runs_as_under_client16402` red;
//! struck_victim_skips_turn -> `a_unit_struck_down_in_the_pass_still_strikes` red;
//! reach_walker_reads_pass_kills -> `a_walker_with_its_target_in_reach_reads_the_pass_as_it_began` red;
//! sequential_skips_tick_passes -> `the_pass_takes_the_taunt_on_the_tick_the_target_phase_does` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
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

/// The walker scene, the striker created first: (state, striker, walker, victim).
fn walker_scene() -> (BattleState, EntityId, EntityId, EntityId) {
    let mut s = battle(TickOrder::ClientSequentialStrike);
    let k1 = knight(&mut s, Team::Blue, (9000, 12100), None);
    let w = knight(&mut s, Team::Blue, (9000, 9000), None);
    let v = knight(&mut s, Team::Red, (9000, 13600), Some(100));
    (s, k1, w, v)
}

#[test]
fn a_walker_with_its_target_in_reach_reads_the_pass_as_it_began() {
    // The walker created after the striker, put in its attack reach of the victim (1,200 off, its attack not begun)
    // for the kill tick: it keeps the victim to the tick's end, as an attacker does, where out of reach it lets go.
    let kill = {
        let (mut s, _, _, v) = walker_scene();
        until_gone(&mut s, v, 80)
    };
    let (mut s, _, w, v) = walker_scene();
    for _ in 1..kill {
        s.tick();
    }
    let e = s.entity(w).expect("the walker");
    assert_eq!((e.target, e.attack_phase), (Some(v), AttackPhase::Idle), "the scene drifted: the walker was not walking at the victim");
    assert!(s.debug_set_pos(w, at(10200, 13600)));
    s.tick();
    assert!(s.entity(v).is_none(), "the scene drifted: the victim did not fall on the kill tick");
    assert_eq!(s.entity(w).expect("the walker").target, Some(v), "in reach on the kill tick, the walker read the pass as it began");
}

const HERO_DECK: [&str; 8] = ["Knight", "Archer", "Giant", "Musketeer", "MiniPekka", "HogRider", "Fireball", "Zap"];

/// The hero Knight's taunt scene (tests/hero_knight.rs `the_taunt_turns_an_enemy_on_the_hero`) under `order`: the tick
/// after the press on which the red Knight first targets the hero.
fn taunted(order: TickOrder) -> Option<usize> {
    const AT: (i32, i32) = (9000, 10000);
    const ARCHER: (i32, i32) = (5000, 13500);
    const RED: (i32, i32) = (5000, 12500);
    let mut cfg = config();
    let deck: Vec<String> = HERO_DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.tick_order = order;
    let mut s = BattleState::try_new(7, cfg).expect("the deck loads");
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s.spawn_unit(Team::Blue, "Knight_hero", at(AT.0, AT.1), None).expect("the hero");
    s.spawn_unit_resolved(Team::Blue, "Archer", at(ARCHER.0, ARCHER.1), None).expect("the Archer");
    s.spawn_unit_resolved(Team::Red, "Knight", at(RED.0, RED.1), None).expect("the red Knight");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Knight_hero")[0].id;
    let archer = find_live(&s, Team::Blue, "Archer")[0].id;
    let red = find_live(&s, Team::Red, "Knight")[0].id;
    let hold = |s: &mut BattleState| {
        for (id, p) in [(hero, AT), (archer, ARCHER), (red, RED)] {
            assert!(s.debug_set_pos(id, at(p.0, p.1)));
        }
        for id in [archer, red] {
            let max = s.entity(id).expect("a held unit").max_hp;
            assert!(s.debug_set_hp(id, max));
        }
    };
    for _ in 0..40 {
        hold(&mut s);
        s.tick();
    }
    assert_eq!(s.entity(red).expect("the red Knight").target, Some(archer), "the scene drifted: the red Knight was not on the Archer");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for k in 0..40 {
        hold(&mut s);
        s.tick();
        if s.entity(red).expect("the red Knight").target == Some(hero) {
            return Some(k);
        }
    }
    None
}

#[test]
fn the_pass_takes_the_taunt_on_the_tick_the_target_phase_does() {
    let old = taunted(TickOrder::Client16402);
    assert!(old.is_some(), "the scene drifted: the taunt never turned the red Knight under client16402");
    assert_eq!(taunted(TickOrder::ClientSequentialStrike), old, "the taunt's tick under the sequential order");
}
