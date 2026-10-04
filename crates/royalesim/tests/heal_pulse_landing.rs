//! status.HEAL_PULSE_LANDING: whether a buff's heal pulse lands after the tick's buffered hits (a shot's landing) or
//! before them (state.rs `phase_resolve`, `land_buff_heals`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, sp-form-RoyalRecruits-evo-s0 t506: a full Recruit took a
//! princess tower's arrow on a Heal Spirit pulse and lost the whole arrow (the pulse lost to the cap); sweep-Heal t296: a
//! full Knight struck by a Knight's melee strike on a pulse kept the pulse (the strike lands at once, before Resolve).
//!
//! THE SCENE (tests/spell_summon.rs's Heal Spirit scene, with a shooter): on Blue's bank a full-hp Knight fights a red
//! Giant (which hits only buildings), a red Musketeer 8,000 behind it walks into its reach and shoots the Knight (from
//! 5,500 it shot on tick 19, before any pulse, and the Knight was never full on a pulse's shot after), and Blue's Heal
//! cast `d` ticks into the scene puts its spirit's heal area on the Giant beside the Knight. The cast delay is searched
//! (0..40) for the first one under which the two arms part on a tick that the Knight began at full hitpoints. WHAT IS
//! PINNED:
//!   1. on that tick the Knight is hit (it ends it short under both arms) and client15535_before_buffered_hits leaves it
//!      exactly the pulse (100 at level 11) lower than after_hits: the pulse lost to the cap before the shot;
//!   2. after_hits (the old arm, the vacuity check) keeps the pulse: the Knight ends that tick short by the shot less 100;
//!   3. the shipped value is after_hits (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test heal_pulse_landing`):
//!   * `heal_after_buffered_hits` -- the new arm's heals still land after the buffered hits: the arms never part, so
//!     (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, HealPulseLanding, PulseAmount};
use royalesim::Team;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Knight's hitpoints at the end of each of 200 ticks after the Heal cast, the cast `d` ticks into the scene, under
/// `arm`; and its maximum.
fn knight_hp(arm: HealPulseLanding, d: u32) -> (Vec<i32>, i32) {
    let mut cfg: BattleConfig = config();
    cfg.calib.buff_pulse_amount = PulseAmount::ScaledPerSecondTimesFrequency;
    cfg.calib.heal_pulse_landing = arm;
    let mut s = BattleState::new(0, cfg);
    let ids = s
        .scenario_spawn_batch(&[
            (Team::Blue, "Knight", at(9000, 12000), None),
            (Team::Red, "Giant", at(9000, 13300), None),
            (Team::Red, "Musketeer", at(9000, 20000), None),
        ])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let max = s.entity(ids[0]).expect("the Knight").max_hp;
    let mut hp = Vec::new();
    for _ in 0..d {
        s.tick();
        hp.push(s.entity(ids[0]).map_or(0, |v| v.hp));
    }
    s.spawn_unit(Team::Blue, "Heal", at(9000, 10500), None).expect("cast Heal");
    for _ in 0..200 {
        s.tick();
        hp.push(s.entity(ids[0]).map_or(0, |v| v.hp));
    }
    (hp, max)
}

/// The first cast delay under which the arms part on a tick the Knight began at full hitpoints: (the delay, the Knight's
/// maximum, its hitpoints that tick under after_hits, under client15535_before_buffered_hits).
fn first_full_parting() -> Option<(u32, i32, i32, i32)> {
    for d in 0..40 {
        let (old, max) = knight_hp(HealPulseLanding::AfterHits, d);
        let (new, _) = knight_hp(HealPulseLanding::Client15535BeforeBufferedHits, d);
        let Some(j) = (0..old.len().min(new.len())).find(|&j| old[j] != new[j]) else { continue };
        let before = if j == 0 { max } else { old[j - 1] };
        if before == max {
            return Some((d, max, old[j], new[j]));
        }
    }
    None
}

/// Plant: heal_after_buffered_hits.
#[test]
fn a_full_units_pulse_is_lost_before_a_shot_under_client15535_before_buffered_hits() {
    let (d, max, old, new) = first_full_parting().expect("new: under no cast delay did the arms part on a full Knight's tick");
    assert!(new < max - 100, "new (cast at {d}): the full Knight was not shot on the parting tick ({new} of {max})");
    assert_eq!(old - new, 100, "new (cast at {d}): the pulse was not lost to the cap before the shot (old {old}, new {new}, max {max})");
}

#[test]
fn the_old_value_keeps_the_pulse_after_the_shot() {
    let (d, max, old, _) = first_full_parting().expect("the scene drifted: under no cast delay did the arms part on a full Knight's tick");
    // The shot less the pulse: short of full, by less than the shot (the new arm's loss, pinned above).
    assert!(old < max, "old (cast at {d}): the Knight was not shot on the parting tick");
    let (new_hp, _) = knight_hp(HealPulseLanding::Client15535BeforeBufferedHits, d);
    let shot = max - new_hp.iter().zip(knight_hp(HealPulseLanding::AfterHits, d).0.iter()).find(|(n, o)| n != o).map(|(n, _)| *n).unwrap();
    assert_eq!(max - old, shot - 100, "old (cast at {d}): the Knight ended {old} of {max}, not the shot ({shot}) less the pulse");
}

#[test]
fn the_shipped_value_is_after_hits() {
    assert_eq!(Calib::shipped().heal_pulse_landing, HealPulseLanding::AfterHits);
}
