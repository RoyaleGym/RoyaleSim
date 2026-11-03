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
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleState, PulseEdge, PulsingAreaLife};
use royalesim::Team;

/// A Blue Knight held 6000 off a Red `spell` cast on (900, 1000) until `enter` ticks after the cast, then on its centre, under
/// spells.PULSING_AREA_LIFE = `arm`: (the last tick, counted from the cast's, its buff slot was seen after, the ticks its hp
/// dropped). A slot applied on tick N is last seen after N + 19 and still pulses on N + 20, before that tick's countdown.
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
/// longer; a Knight that entered on the phase whose pulse falls on the tick after that slot's last sight takes one pulse
/// more there, and on every other phase the pulses are the same. Plant: pulsing_area_life_exclusive.
#[test]
fn under_client16402_last_update_inclusive_a_poison_applies_once_more_and_pulses_on_its_tail() {
    let mut tails = 0;
    for enter in 0..20 {
        let (last_old, drops_old) = tail("Poison", PulsingAreaLife::LifeExclusive, enter);
        let (last_new, drops_new) = tail("Poison", PulsingAreaLife::Client16402LastUpdateInclusive, enter);
        assert_eq!(last_new, last_old + 5, "enter {enter}: the slot's last tick one HitSpeed later ({last_old} -> {last_new})");
        assert!(drops_new.len() >= drops_old.len() && drops_new[..drops_old.len()] == drops_old[..], "enter {enter}: the earlier pulses the same ({drops_old:?} -> {drops_new:?})");
        let extra = &drops_new[drops_old.len()..];
        if (last_new + 1 - drops_old[0]) % 20 == 0 {
            assert_eq!(extra, &[last_new + 1][..], "enter {enter}: the tail pulse on the tick after the slot's last sight ({drops_old:?} -> {drops_new:?})");
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

/// Under spells.PULSE_EDGE = `arm`: a Blue Poison cast about (600, 2200) (its centre read off the cloud: a cast lands off the point
/// it is cast on; the engine keeps unit positions on whole native units, so the centre must sit on them too); once it stands, a Red
/// Musketeer played 7,000 outside it and, from its first frame, held `off` subtiles outside the Poison's Radius (3500) + its own
/// radius, on the cloud's x axis, while it deploys (a deploying unit does not move, so it stays exactly there for the scans).
/// Whether the Poison's buff takes a slot on it on a deploy tick.
fn poisoned(arm: PulseEdge, off: i32) -> bool {
    let mut cfg = config();
    cfg.calib.pulse_edge = arm;
    let mut s = BattleState::new(5, cfg);
    s.spawn_unit(Team::Blue, "Poison", t(600, 2200), None).expect("the cast");
    let mut centre: Option<Vec2> = None;
    for _ in 0..30 {
        s.tick();
        centre = s.spells().iter().find_map(|sp| match &sp.motion {
            SpellMotion::Pulsing(p) if sp.team == Team::Blue => Some(p.pos),
            _ => None,
        });
        if centre.is_some() {
            break;
        }
    }
    let c = centre.expect("the scene drifted: no Poison cloud");
    assert!(c.x % K == 0 && c.y % K == 0, "the scene: the cloud's centre {c:?} is off the native grid, so no unit can sit on its exact edge");
    s.spawn_unit(Team::Red, "Musketeer", Vec2::new(c.x + 7000 * K, c.y), None).expect("the Musketeer");
    let mut on_edge = 0;
    for _ in 0..60 {
        let mut held = false;
        if let Some(m) = find_live(&s, Team::Red, "Musketeer").first().map(|e| (e.id, e.radius, e.deploying)) {
            if !m.2 {
                break;
            }
            assert!(s.debug_set_pos(m.0, Vec2::new(c.x + 3500 * K + m.1 + off, c.y)));
            held = true;
        }
        s.tick();
        if !held {
            continue;
        }
        let Some(v) = find_live(&s, Team::Red, "Musketeer").first().map(|e| (e.pos, e.radius, e.deploying, e.buffs.iter().any(|b| !b.is_empty()))) else { continue };
        if !v.2 {
            break;
        }
        assert_eq!(v.0, Vec2::new(c.x + 3500 * K + v.1 + off, c.y), "the scene drifted: the deploying Musketeer moved");
        on_edge += 1;
        if v.3 {
            return true;
        }
    }
    assert!(on_edge >= 12, "the scene drifted: the Musketeer stood on the edge {on_edge} deploy ticks (fewer than the Poison's scans need)");
    false
}

/// spells.PULSE_EDGE = client16402_exclusive (client 16.402, parity r83, live13: 9 of 9 units on a Poison's exact edge took no
/// pulse from the edge scans; the engine's edge scan poisoned 5 of 5). Plant: pulse_edge_inclusive.
#[test]
fn under_client16402_exclusive_a_poison_misses_a_unit_on_its_edge() {
    assert!(!poisoned(PulseEdge::Client16402Exclusive, 0), "client16402_exclusive: the edge was reached");
    assert!(poisoned(PulseEdge::Client16402Exclusive, -K), "client16402_exclusive: a native inside was missed");
    // NOT VACUOUS: inclusive reaches the edge
    assert!(poisoned(PulseEdge::Inclusive, 0), "inclusive: the edge was missed");
    assert!(!poisoned(PulseEdge::Inclusive, K), "the scene drifted: inclusive reached a native outside");
}
