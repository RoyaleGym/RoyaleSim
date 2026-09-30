//! THE EVO FIRECRACKER (tools/extract_cards.py `fireworks_block`; card.rs `FireworksDef`, EVO_FIREWORKS,
//! EVO_SPARK_FIREWORKS; combat.rs `step_projectiles`; state.rs `phase_projectile`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-Firecracker-evo-s0): its rocket landed on t964 by a Knight (the five
//! sparks' 64 each, 320); the Knight lost 12 on t974 and every 5 ticks after while it stood in the area.
//! Read off the table, not measured: the sparks' own fireworks, where their flights end.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_firecracker`):
//!   - fireworks_never -> both tests red;
//!   - fireworks_at_once -> `the_rockets_fireworks_pulse_12_from_ten_ticks_after_the_landing` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The form held at (9000, 12500), a red Knight held 5500 ahead (the rocket's target) and another held on the middle
/// spark's end, 5000 on along the rocket's line: (the landing frame, the first Knight's and the second's hitpoints).
fn scene() -> (usize, Vec<i32>, Vec<i32>) {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Firecracker".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let (at, k1_at, k2_at) = (n(9000, 12500), n(9000, 18000), n(9000, 23000));
    s.spawn_unit(Team::Blue, "Firecracker_EV1", at, None).expect("the Firecracker");
    let k1 = s.scenario_spawn_now(Team::Red, "Knight", k1_at, None).expect("a red Knight");
    let k2 = s.scenario_spawn_now(Team::Red, "Knight", k2_at, None).expect("a second red Knight");
    s.tick();
    let fc = find_live(&s, Team::Blue, "Firecracker_EV1").first().expect("the Firecracker").id;
    let top = s.entity(k1).expect("the Knight").max_hp;
    let (mut landed, mut h1, mut h2) = (None, Vec::new(), Vec::new());
    for k in 0..400 {
        assert!(s.debug_set_pos(fc, at));
        assert!(s.debug_set_pos(k1, k1_at));
        assert!(s.debug_set_pos(k2, k2_at));
        s.tick();
        h1.push(s.entity(k1).expect("the Knight").hp);
        h2.push(s.entity(k2).expect("the second Knight").hp);
        if landed.is_none() && h1[k] < top {
            landed = Some(k);
        }
        if landed.is_some_and(|l| k >= l + 100) {
            break;
        }
    }
    (landed.expect("the rocket landed by the Knight"), h1, h2)
}

#[test]
fn the_rockets_fireworks_pulse_12_from_ten_ticks_after_the_landing() {
    let (l, h1, _) = scene();
    let at_landing = h1[l];
    // Its next rocket comes 3000 ms on: through the landing + 50 the only damage is the fireworks' (12 every 5 ticks
    // from the landing + 10: the area hangs its buff one HitSpeed after it is made, the buff pulses a period later).
    for k in l..=l + 50 {
        let pulses = if k < l + 10 { 0 } else { ((k - l - 10) / 5 + 1) as i32 };
        assert_eq!(h1[k], at_landing - 12 * pulses, "frame {k} (landed on {l}): {:?}", &h1[l..=l + 50]);
    }
}

#[test]
fn a_sparks_fireworks_stand_where_its_flight_ends() {
    // The second Knight, on the middle spark's end, takes the small fireworks' pulses: 12 twice 5 ticks apart.
    let (l, _, h2) = scene();
    let two = (l + 1..l + 90).any(|k| h2[k - 1] - h2[k] == 12 && h2[k + 4] - h2[k + 5] == 12);
    assert!(two, "no fireworks pulses on the spark's end (landed on {l}): {:?}", &h2[l..]);
}
