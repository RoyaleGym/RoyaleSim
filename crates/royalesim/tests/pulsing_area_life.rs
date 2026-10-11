//! spells.PULSING_AREA_LIFE: whether a standing pulsing area that pulses slower than every tick (and pulls nothing) also
//! updates on the update its life reads 0, so it lives LifeDuration + a tick (spell.rs `area_life_floor`).
//!
//! Parity's r71 census of the live8 set (client 16.402), C a Poison's first frame: every unit still in the cloud at its end
//! took one more pulse on C + 180, one HitFrequency after the cloud's last on C + 160, and walked slowed through C + 180
//! (22 of 22 clean ends: every unit whose pulses fell on C + 20k); the 3 that took none had walked out. The engine's last application lands on C + 155 (its slow
//! over from C + 176), the client's on C + 160.
//!
//! PLANTS (regression):
//!   * `pulsing_area_life_exclusive` -- the new arm still ends the area on its last whole update:
//!     `under_client16402_last_update_inclusive_a_poison_applies_once_more_and_pulses_on_its_tail` goes red.
//!     RUSTFLAGS='--cfg clash_plant="pulsing_area_life_exclusive"' CARGO_TARGET_DIR=target/plant cargo test --test pulsing_area_life
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::state::{BattleState, PulsingAreaLife};
use royalesim::Team;

/// A Blue Knight held 6000 off a Red `spell` cast on (900, 1000) until `enter` ticks after the cast, then on its centre, under
/// spells.PULSING_AREA_LIFE = `arm`: (the last tick, counted from the cast's, its buff slot stood, the ticks its hp dropped).
fn tail(spell: &str, arm: PulsingAreaLife, enter: u32) -> (u32, Vec<u32>) {
    let mut cfg = config();
    cfg.calib.pulsing_area_life = arm;
    let mut s = BattleState::new(5, cfg);
    let (at, away) = (t(900, 1000), t(300, 1000));
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", away, None).expect("the Knight");
    s.spawn_unit(Team::Red, spell, at, None).expect("the cast");
    let cast = s.tick_count();
    let (mut last, mut drops, mut hp) = (0, Vec::new(), s.entity(knight).expect("the Knight").hp);
    for k in 0..240 {
        assert!(s.debug_set_pos(knight, if k < enter { away } else { at }));
        s.tick();
        let age = s.tick_count() - 1 - cast;
        let v = s.entity(knight).expect("the Knight lives");
        if v.buffs.iter().any(|b| !b.is_empty()) {
            last = age;
        }
        if v.hp < hp {
            drops.push(age);
        }
        hp = v.hp;
    }
    assert!(!drops.is_empty() && last > 0, "{spell}: the cast never reached the Knight");
    (last, drops)
}

/// spells.PULSING_AREA_LIFE = client16402_last_update_inclusive (client 16.402, parity's r71 census: a unit in the cloud at
/// its end whose pulse falls one HitFrequency after the cloud's last takes it and stays slowed through it, 22 of 22): the
/// Poison's last application comes one HitSpeed (5 ticks) later than under life_exclusive, so its slot stands 5 ticks
/// longer; a Knight that entered on the phase whose pulse falls on that new last tick takes one pulse more there, and on
/// every other phase the pulses are the same. Plant: pulsing_area_life_exclusive.
#[test]
fn under_client16402_last_update_inclusive_a_poison_applies_once_more_and_pulses_on_its_tail() {
    let mut tails = 0;
    for enter in 0..20 {
        let (last_old, drops_old) = tail("Poison", PulsingAreaLife::LifeExclusive, enter);
        let (last_new, drops_new) = tail("Poison", PulsingAreaLife::Client16402LastUpdateInclusive, enter);
        assert_eq!(last_new, last_old + 5, "enter {enter}: the slot's last tick one HitSpeed later ({last_old} -> {last_new})");
        assert!(drops_new.len() >= drops_old.len() && drops_new[..drops_old.len()] == drops_old[..], "enter {enter}: the earlier pulses the same ({drops_old:?} -> {drops_new:?})");
        let extra = &drops_new[drops_old.len()..];
        if (last_new - drops_old[0]) % 20 == 0 {
            assert_eq!(extra, &[last_new][..], "enter {enter}: the tail pulse on the slot's last tick ({drops_old:?} -> {drops_new:?})");
            tails += 1;
        } else {
            assert!(extra.is_empty(), "enter {enter}: a pulse off the tail's phase ({drops_old:?} -> {drops_new:?})");
        }
    }
    assert!(tails > 0, "no entry put the Knight's pulse on the slot's last tick: the scene proves nothing");
}

/// The Tornado pulses every tick (and pulls): spells.PULSING_AREA_LIFE leaves its life as it was under either arm.
#[test]
fn an_every_tick_area_keeps_its_life_under_either_arm() {
    assert_eq!(tail("Tornado", PulsingAreaLife::Client16402LastUpdateInclusive, 0), tail("Tornado", PulsingAreaLife::LifeExclusive, 0));
}
