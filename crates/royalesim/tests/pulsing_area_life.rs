//! spells.PULSING_AREA_LIFE: whether a standing pulsing area that pulses slower than every tick (and pulls nothing) also
//! updates on the update its life reads 0, so it lives LifeDuration + a tick (spell.rs `area_life_floor`).
//!
//! Parity's r71 census of the live8 set (client 16.402), C a Poison's first frame: every unit still in the cloud at its end
//! took one more pulse on C + 180, one HitFrequency after the cloud's last on C + 160, and walked slowed through C + 180
//! (22 of 22 clean ends); the 3 that took none had walked out. The engine's last application lands on C + 155 (its slow
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

/// A Blue Knight held at one spot under a Red `spell` cast on it, under spells.PULSING_AREA_LIFE = `arm`: (the last tick,
/// counted from the cast's, its buff slot stood, the ticks its hp dropped).
fn tail(spell: &str, arm: PulsingAreaLife) -> (u32, Vec<u32>) {
    let mut cfg = config();
    cfg.calib.pulsing_area_life = arm;
    let mut s = BattleState::new(5, cfg);
    let at = t(900, 1000);
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", at, None).expect("the Knight");
    s.spawn_unit(Team::Red, spell, at, None).expect("the cast");
    let cast = s.tick_count();
    let (mut last, mut drops, mut hp) = (0, Vec::new(), s.entity(knight).expect("the Knight").hp);
    for _ in 0..240 {
        assert!(s.debug_set_pos(knight, at));
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
/// its end takes one more pulse, one HitFrequency after the last, and stays slowed through it, 22 of 22): the Poison's
/// last application comes one HitSpeed (5 ticks) later than under life_exclusive, its slot stands to the tick of a pulse
/// one HitFrequency (20 ticks) after the old last one, and that pulse lands. Plant: pulsing_area_life_exclusive.
#[test]
fn under_client16402_last_update_inclusive_a_poison_applies_once_more_and_pulses_on_its_tail() {
    let (last_old, drops_old) = tail("Poison", PulsingAreaLife::LifeExclusive);
    let (last_new, drops_new) = tail("Poison", PulsingAreaLife::Client16402LastUpdateInclusive);
    assert_eq!(last_new, last_old + 5, "client16402_last_update_inclusive: the slot's last tick one HitSpeed later ({last_old} -> {last_new})");
    assert_eq!(drops_new.len(), drops_old.len() + 1, "client16402_last_update_inclusive: one pulse more ({drops_old:?} -> {drops_new:?})");
    assert_eq!(drops_new[..drops_old.len()], drops_old[..], "the earlier pulses are the same under both arms");
    let (old_end, new_end) = (*drops_old.last().unwrap(), *drops_new.last().unwrap());
    assert_eq!(new_end, old_end + 20, "the tail pulse one HitFrequency after the old last one");
    assert_eq!(new_end, last_new, "the tail pulse on the slot's last tick, as the client's (slowed through C + 180, the pulse there)");
    assert!(last_old < old_end + 20, "life_exclusive (the vacuity check): the slot gone before a tail pulse ({last_old}, last pulse {old_end})");
}

/// The Tornado pulses every tick (and pulls): spells.PULSING_AREA_LIFE leaves its life as it was under either arm.
#[test]
fn an_every_tick_area_keeps_its_life_under_either_arm() {
    assert_eq!(tail("Tornado", PulsingAreaLife::Client16402LastUpdateInclusive), tail("Tornado", PulsingAreaLife::LifeExclusive));
}
