//! movement.DEFLECT_CONTACT: whether a Monk whose deflect is active is a body to his neighbours' separation (state.rs
//! phase_path16402, `deflect_off` and `deflect_hides`).
//!
//! THE LAW, measured on client 15.535.29 (the ledger has the rows): he pushes them. sp-champ-Monk-recharge-q20-s0
//! t229..t265, a Giant sliding round him took his push on 34 of 34 frames inside both radii, and he stood on his point.
//!
//! THE SCENE (tests/monk_deflect.rs's): Blue's level-11 Monk held on (3500, 12500) until a red Giant walking down his lane
//! comes within 2,200; pressed; from the trigger (P + 18) to P + 90, each tick that starts with the Giant inside both radii
//! of him (its only neighbour), measured native (a Giant's push off a Monk is often under a millitile). WHAT IS PINNED:
//!   1. client15535_pushes: on every such tick the Giant meets one neighbour and takes no push toward him, a push away
//!      from him whenever it stands 100 or more inside both radii (a shallower overlap's push may round to nothing), and
//!      he stands where the active window began;
//!   2. hidden (the old arm, the vacuity check): the Giant takes no push on any of them;
//!   3. the shipped value is hidden (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test deflect_contact`):
//!   * `deflect_pushes_nothing` -- the new arm still hides him from his neighbours' separation: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DeflectContact};
use royalesim::Team;

const DECK: [&str; 8] = ["Monk", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const RED: [&str; 8] = ["Giant", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// One tick of the active window that started with the Giant on him: (P + k, how far inside both radii, the Giant's
/// neighbours met, its push, the Monk -> Giant offset, all four native; the Monk's point, in millitiles).
type Row = (u32, i64, i32, (i32, i32), (i32, i32), (i32, i32));

fn on_him(arm: DeflectContact) -> Vec<Row> {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), RED.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.deflect_contact = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let at = n((3500, 12500));
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", at, None).expect("the Monk");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", n((3500, 17500)), None).expect("the Giant");
    let off = |s: &BattleState| {
        let (a, b) = (s.entity(monk).expect("the Monk").pos, s.entity(giant).expect("the Giant").pos);
        (b.x - a.x, b.y - a.y)
    };
    let gap = |o: (i32, i32)| isqrt(i64::from(o.0) * i64::from(o.0) + i64::from(o.1) * i64::from(o.1));
    let reach = i64::from(s.entity(monk).expect("the Monk").radius + s.entity(giant).expect("the Giant").radius);
    let mut waited = 0;
    while gap(off(&s)) > 2200 * i64::from(K) {
        assert!(s.debug_set_pos(monk, at));
        s.tick();
        waited += 1;
        assert!(waited < 400, "{arm:?}: the scene drifted: the Giant never came near ({})", gap(off(&s)));
    }
    assert!(s.debug_set_pos(monk, at));
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows = Vec::new();
    while s.tick_count() - 1 < p + 90 {
        let o = off(&s);
        s.tick();
        let k = s.tick_count() - 1 - p;
        let (g, m) = (s.entity(giant).expect("the Giant"), s.entity(monk).expect("the Monk"));
        if k >= 18 && gap(o) < reach {
            rows.push((k, reach - gap(o), g.push_neighbours, (g.push_applied.x, g.push_applied.y), o, (m.pos.x / K, m.pos.y / K)));
        }
    }
    assert!(rows.len() >= 3, "{arm:?}: the scene drifted: the Giant stood inside {reach} of him on {} ticks of the active window", rows.len());
    rows
}

#[test]
fn a_deflecting_monk_pushes_a_giant_on_him_under_client15535_pushes() {
    let rows = on_him(DeflectContact::Client15535Pushes);
    let first = rows[0].5;
    for r in &rows {
        let (push, o) = (r.3, r.4);
        let away = i64::from(push.0) * i64::from(o.0) + i64::from(push.1) * i64::from(o.1);
        assert!(r.2 == 1 && away >= 0 && (away > 0 || r.1 < 100), "P + {}: the Giant, {} inside, was not pushed off him: {rows:?}", r.0, r.1);
        assert_eq!(r.5, first, "P + {}: he stands where the active window began: {rows:?}", r.0);
    }
}

#[test]
fn the_old_value_pushes_nothing() {
    let rows = on_him(DeflectContact::Hidden);
    for r in &rows {
        assert!(r.2 == 0 && r.3 == (0, 0), "P + {}: old: the Giant, {} inside, was pushed (vacuous otherwise): {rows:?}", r.0, r.1);
    }
}

#[test]
fn the_shipped_value_is_hidden() {
    assert_eq!(Calib::shipped().deflect_contact, DeflectContact::Hidden);
}
