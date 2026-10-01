//! GOBLINSTEIN'S TETHER (tools/extract_cards.py `champion_tether`; card.rs `TetherDef`, TETHER_FIRST_HIT_TICKS,
//! `CardDb::button_unit`; state.rs `TetherRun`, `tether_pass`, the Doctor holding the button), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, Oracle's sp-champ-Goblinstein-s0; the press issued t214 on the Doctor): the Monster
//! on the tap, the Doctor 1000 left and 3500 behind; a Knight near the line losing 94 (37 at level 11) on t234 and every
//! 10 ticks after; a Musketeer 2279 from the line hit, 2803 from it missed (TetherWidth 2000 and its radius).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! goblinstein`): tether_never, early_trigger_late.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Goblinstein", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const TAP: (i32, i32) = (9500, 11500);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Goblinstein played at TAP and let deploy: the battle, the Monster and the Doctor.
fn scene() -> (BattleState, EntityId, EntityId) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "Goblinstein", n(TAP)).expect("the play");
    for _ in 0..30 {
        s.tick();
    }
    let monster = find_live(&s, Team::Blue, "Goblinstein").first().expect("the Monster").id;
    let doctor = find_live(&s, Team::Blue, "goblinstein_doctor").first().expect("the Doctor").id;
    (s, monster, doctor)
}

#[test]
fn its_doctor_stands_1000_left_and_3500_behind_its_monster() {
    // Each unit's first point (the Doctor is put down 100 ms after the Monster), both deploying there.
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "Goblinstein", n(TAP)).expect("the play");
    let (mut m, mut d) = (None, None);
    for _ in 0..10 {
        s.tick();
        m = m.or_else(|| find_live(&s, Team::Blue, "Goblinstein").first().map(|e| e.pos));
        d = d.or_else(|| find_live(&s, Team::Blue, "goblinstein_doctor").first().map(|e| e.pos));
    }
    let (m, d) = (m.expect("the Monster"), d.expect("the Doctor"));
    assert!(((m.x / K) - TAP.0).abs() <= 150 && ((m.y / K) - TAP.1).abs() <= 150, "the Monster on the tap: {m:?}");
    let off = ((d.x - m.x) / K, (d.y - m.y) / K);
    assert!((off.0 + 1000).abs() <= 150 && (off.1 + 3500).abs() <= 150, "the Doctor's offset from the Monster: {off:?}");
}

#[test]
fn its_tether_hits_what_stands_by_the_line_every_10_ticks_from_the_press_plus_20() {
    let (mut s, monster, doctor) = scene();
    let (m_at, d_at) = (n((9500, 14000)), n((9500, 9000)));
    // One Knight 1000 off the line (in its reach: 2000 and its radius), one 4000 off (out of it).
    let near = s.scenario_spawn_now(Team::Red, "Knight", n((10500, 11500)), None).expect("a Knight");
    let far = s.scenario_spawn_now(Team::Red, "Knight", n((13500, 11500)), None).expect("a Knight");
    let hold = |s: &mut BattleState| {
        assert!(s.debug_set_pos(monster, m_at));
        assert!(s.debug_set_pos(doctor, d_at));
        for (k, at) in [(near, (10500, 11500)), (far, (13500, 11500))] {
            let full = s.entity(k).expect("a Knight").max_hp;
            assert!(s.debug_set_pos(k, n(at)));
            assert!(s.debug_set_hp(k, full));
        }
    };
    for _ in 0..20 {
        hold(&mut s);
        s.tick();
    }
    // Blue's princess towers down: an arrow landing on a tether's tick would hide that hit.
    let towers: Vec<EntityId> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press, on the Doctor");
    let (mut hits_near, mut hits_far) = (Vec::new(), Vec::new());
    for _ in 0..110 {
        hold(&mut s);
        let (a, b) = (s.entity(near).expect("near").hp, s.entity(far).expect("far").hp);
        s.tick();
        let now = s.tick_count() - 1;
        // A tether hit is 94; the Doctor's own shot (135) may land on the same tick (the Monster hits buildings only).
        if matches!(a - s.entity(near).expect("near").hp, 94 | 229) {
            hits_near.push(now - p);
        }
        if matches!(b - s.entity(far).expect("far").hp, 94 | 229) {
            hits_far.push(now - p);
        }
    }
    assert_eq!(hits_near, vec![20, 30, 40, 50, 60, 70, 80, 90], "the near Knight's tether hits, from the press");
    assert!(hits_far.is_empty(), "the far Knight out of the tether's reach: {hits_far:?}");
}
