//! knockback.TROOP_DEATH_PUSHBACK: whether a dying TROOP whose row sets DeathPushBack (the Golem 1800, the Golemite 900)
//! pushes the units its death blow hits (state.rs `phase_reap`).
//!
//! THE LAW, measured on client 15.535.29: every enemy troop within reach of a dying Golem or Golemite was pushed straight
//! out of the death point on the knockback ladder of the row's DeathPushBack, 43 of 43 (sp-f2-cagefb-s0 t928: a Goblin
//! Brawler 1,342 from the Golem stepped 248, 249, 223 ... away). The first step comes on the death tick when a unit struck
//! the troop down in the sequential pass (27 of 29), on the next tick when a shot killed it (14 of 14).
//!
//! WHAT IS PINNED, on a Red Golem of 1 hitpoint standing in deploy state, under match.TICK_ORDER = client_sequential_strike:
//!   1. a Blue Knight that strikes it dead is pushed on the death tick, straight out of the death point, a ladder step
//!      (more than 200), and again on the next;
//!   2. a Blue Musketeer whose shot kills it is not moved on the death tick and pushed from the next, straight out;
//!   3. under not_read neither moves on either tick;
//!   4. under client16402_with_blow (client 16.402: armed with the blow at the next tick's Resolve) every first step on
//!      T + 2, the Knight's (struck down in the pass) as the Musketeer's (a shot kill).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! troop_death_pushback`):
//!   * `troop_death_pushback_unread` -- no dying troop pushes under the new arm: (1) and (2) go red.
//!   * `troop_death_push_armed_in_reap` -- client16402_with_blow arms the pushes on the death tick: (4) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, TickOrder, TroopDeathPushback};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Knight", "Musketeer", "Golem", "Giant", "Fireball", "Arrows", "Minions", "Zap"];

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The attacker's moves on the Golem's death tick and the next, each projected on the line out of the death point
/// (native, outward positive), with the death tick's step length.
fn scene(arm: TroopDeathPushback, attacker: &str, at: (i32, i32)) -> (i32, i32) {
    let (t, t1, _) = scene3(arm, attacker, at);
    (t, t1)
}

/// `scene`'s moves on the death tick, the next and the one after.
fn scene3(arm: TroopDeathPushback, attacker: &str, at: (i32, i32)) -> (i32, i32, i32) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), DECK.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    cfg.calib.troop_death_pushback = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    // Out of every tower's reach: the Golem on Red's half, 8,500 from the nearest princess tower.
    let golem = s.scenario_spawn_now(Team::Red, "Golem", n(9000, 19500), Some(1)).expect("the Golem");
    let a: EntityId = s.scenario_spawn_now(Team::Blue, attacker, n(at.0, at.1), None).expect("the attacker");
    let death = s.entity(golem).expect("the Golem").pos;
    let mut before = None;
    for _ in 0..200 {
        let p = s.entity(a).expect("the attacker").pos;
        s.tick();
        if s.entity(golem).is_none() {
            before = Some(p);
            break;
        }
    }
    let p0 = before.expect("the scene drifted: the Golem stood 200 ticks");
    let p1 = s.entity(a).expect("the attacker").pos;
    s.tick();
    let p2 = s.entity(a).expect("the attacker").pos;
    s.tick();
    let p3 = s.entity(a).expect("the attacker").pos;
    let out = |from: Vec2, to: Vec2| -> i32 {
        let (ox, oy) = ((from.x - death.x) as i64, (from.y - death.y) as i64);
        let len = royalesim::fixed::isqrt(ox * ox + oy * oy).max(1);
        ((((to.x - from.x) as i64) * ox + ((to.y - from.y) as i64) * oy) / len / K as i64) as i32
    };
    (out(p0, p1), out(p1, p2), out(p2, p3))
}

#[test]
fn a_golem_struck_down_in_the_pass_pushes_on_its_death_tick() {
    let (t, t1) = scene(TroopDeathPushback::Client15535Ladder, "Knight", (9000, 18300));
    assert!(t > 200 && t1 > 200, "client15535_ladder: the Knight's moves out of the death point on T and T + 1: {t}, {t1}");
    // not_read: no ladder step (the Golemites laid on the death point may nudge it by contact: 3 native on T)
    let (ot, ot1) = scene(TroopDeathPushback::NotRead, "Knight", (9000, 18300));
    assert!(ot < 50 && ot1 < 50, "not_read: the Knight was pushed: {ot}, {ot1}");
}

#[test]
fn a_golem_shot_dead_pushes_from_the_next_tick() {
    let (t, t1) = scene(TroopDeathPushback::Client15535Ladder, "Musketeer", (9000, 17500));
    assert!(t == 0 && t1 > 200, "client15535_ladder: the Musketeer's moves out of the death point on T and T + 1: {t}, {t1}");
    let (ot, ot1) = scene(TroopDeathPushback::NotRead, "Musketeer", (9000, 17500));
    assert!(ot <= 0 && ot1 <= 0, "not_read: the Musketeer was pushed: {ot}, {ot1}");
}

/// client16402_with_blow: the pushes are armed with the death blow at the next tick's Resolve, every first step on T + 2
/// however the troop died (client 16.402, parity's r63 census: 72 of 72 Golem and Golemite deaths, 15 of 15 melee kills
/// and 15 of 15 shot kills). Plant: troop_death_push_armed_in_reap.
#[test]
fn under_client16402_with_blow_every_first_step_comes_on_t_plus_2() {
    let (t, t1, t2) = scene3(TroopDeathPushback::Client16402WithBlow, "Musketeer", (9000, 17500));
    assert!(t == 0 && t1.abs() < 100 && t2 > 200, "client16402_with_blow: the Musketeer's moves out on T, T + 1, T + 2: {t}, {t1}, {t2}");
    let (k, k1, k2) = scene3(TroopDeathPushback::Client16402WithBlow, "Knight", (9000, 18300));
    assert!(k < 50 && k1.abs() < 100 && k2 > 200, "client16402_with_blow: the Knight's moves out on T, T + 1, T + 2: {k}, {k1}, {k2}");
    // the old arm on the same scenes: the Musketeer from T + 1 (the vacuity check)
    let (_, o1, _) = scene3(TroopDeathPushback::Client15535Ladder, "Musketeer", (9000, 17500));
    assert!(o1 > 200, "client15535_ladder: the Musketeer's move out on T + 1: {o1}");
}
