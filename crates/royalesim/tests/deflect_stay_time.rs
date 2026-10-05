//! movement.DEFLECT_STAY_TIME: how long the Monk's Deflect keeps him unpushed (state.rs, the Deflect effect's `stay`).
//!
//! THE LAW, measured on client 15.535.29 (the ledger has the row): his own separation first moves him one tick sooner than
//! the engine's (the replay's t267 against t268, the press t170: P + 97 against P + 98 on the replay's count).
//!
//! THE SCENE (tests/deflect_contact.rs's): Blue's level-11 Monk held on (3500, 12500) until a red Giant walking down his
//! lane comes within 2,200; pressed (P); from P + 85 the Giant is put 1,000 above him each tick (inside both radii,
//! 1,250), and the tick his own point first changes is read (P the press's issue tick, as the press tests count it; this
//! scene reads both arms one tick later than the replay's count). WHAT IS PINNED:
//!   1. client15535_tick_short: he is first moved on P + 98, one tick before the old arm;
//!   2. active_time (the old arm, the vacuity check): on P + 99;
//!   3. the shipped value is active_time (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="deflect_stay_full_time"' CARGO_TARGET_DIR=target/plant cargo test --test
//! deflect_stay_time`):
//!   * `deflect_stay_full_time` -- the new arm still keeps him unpushed the whole active time: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DeflectContact, DeflectStayTime};
use royalesim::Team;

const DECK: [&str; 8] = ["Monk", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const RED: [&str; 8] = ["Giant", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The tick (P + k) the Monk's point first changes after P + 85 under `arm`, the Giant held 1,000 above him.
fn first_moved(arm: DeflectStayTime) -> u32 {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), RED.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.deflect_contact = DeflectContact::Client15535Pushes;
    cfg.calib.deflect_stay_time = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let at = n((3500, 12500));
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", at, None).expect("the Monk");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", n((3500, 17500)), None).expect("the Giant");
    let gap = |s: &BattleState| {
        let (a, b) = (s.entity(monk).expect("the Monk").pos, s.entity(giant).expect("the Giant").pos);
        isqrt(i64::from(b.x - a.x) * i64::from(b.x - a.x) + i64::from(b.y - a.y) * i64::from(b.y - a.y))
    };
    let mut waited = 0;
    while gap(&s) > 2200 * i64::from(K) {
        assert!(s.debug_set_pos(monk, at));
        s.tick();
        waited += 1;
        assert!(waited < 400, "{arm:?}: the scene drifted: the Giant never came near");
    }
    assert!(s.debug_set_pos(monk, at));
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    while s.tick_count() - 1 < p + 85 {
        s.tick();
    }
    let stood = s.entity(monk).expect("the Monk").pos;
    while s.tick_count() - 1 < p + 110 {
        let m = s.entity(monk).expect("the Monk").pos;
        assert!(s.debug_set_pos(giant, Vec2::new(m.x, m.y + 1000 * K)));
        s.tick();
        let k = s.tick_count() - 1 - p;
        if s.entity(monk).expect("the Monk").pos != stood {
            return k;
        }
    }
    panic!("{arm:?}: the scene drifted: the Monk never moved by P + 110");
}

/// Plant: deflect_stay_full_time.
#[test]
fn the_monk_is_first_pushed_a_tick_sooner_under_client15535_tick_short() {
    let (new, old) = (first_moved(DeflectStayTime::Client15535TickShort), first_moved(DeflectStayTime::ActiveTime));
    assert_eq!(new, 98, "new: the Monk's first push after his deflect");
    assert_eq!(new + 1, old, "new: one tick before the old arm's");
}

#[test]
fn the_old_value_pushes_him_on_p_plus_99() {
    assert_eq!(first_moved(DeflectStayTime::ActiveTime), 99, "old: the Monk's first push after his deflect (vacuous otherwise)");
}

#[test]
fn the_shipped_value_is_active_time() {
    assert_eq!(Calib::shipped().deflect_stay_time, DeflectStayTime::ActiveTime);
}
