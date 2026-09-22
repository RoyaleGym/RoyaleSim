//! The post-overtime tiebreak (calibration match.OVERTIME_TIEBREAK).
//!
//! WHY IT EXISTS: the engine used to score a match that survived overtime level
//! on crowns as a Draw whatever the towers looked like (2 of 60 random-policy
//! games reached that state, both with a decidable gap). Every scenario here
//! starts at the last tick of overtime with hand-set tower hp, so the verdict is
//! a pure function of the rule and the towers; nothing moves.
mod common;

use common::*;
use royalesim::state::{BattleConfig, BattleState, Outcome, OvertimeTiebreak};
use royalesim::Team;

/// The tick whose Judge phase ends overtime: elapsed_ms(tick + 1) >= overtime_end.
fn last_overtime_tick(cfg: &BattleConfig) -> u32 {
    let c = &cfg.calib;
    (((c.regular_time_s + c.overtime_s) as i64 * 1000 / c.tick_ms as i64) - 1) as u32
}

/// A battle parked on the final overtime tick with the given tower hp
/// (`[king, engine-left princess, engine-right princess]` per side).
fn parked(rule: OvertimeTiebreak, blue: [i32; 3], red: [i32; 3]) -> BattleState {
    let mut cfg = config();
    cfg.calib.overtime_tiebreak = rule;
    let mut s = BattleState::new(7, cfg);
    for (team, hp) in [(Team::Blue, blue), (Team::Red, red)] {
        for (k, h) in hp.iter().enumerate() {
            s.scenario_set_tower_hp(team, k, *h).unwrap();
        }
    }
    let t = last_overtime_tick(s.config());
    s.scenario_set_tick(t);
    assert!(s.is_overtime(), "tick {t} must already be overtime");
    assert!(!s.is_done());
    s.tick();
    assert!(s.is_done(), "the Judge phase of tick {t} must end the match");
    assert_eq!(s.crowns(), [0, 0], "no tower fell: the verdict is the tiebreak's alone");
    s
}

/// A fresh battle's tower max hp `[king, princess, princess]` at the configured
/// tower level -- read from the entities, never from a card constant.
fn tower_max() -> [i32; 3] {
    let s = BattleState::new(1, config());
    let ids = s.tower_ids(Team::Blue);
    let m = |k: usize| s.entity(ids[k].unwrap()).unwrap().max_hp;
    [m(0), m(1), m(2)]
}

#[test]
fn none_draw_is_the_old_engine_a_level_match_stays_a_draw() {
    let s = parked(OvertimeTiebreak::NoneDraw, [1000, 100, 1000], [1000, 1000, 1000]);
    assert_eq!(s.outcome(), Some(Outcome::Draw));
}

#[test]
fn absolute_the_side_with_the_weaker_weakest_tower_loses() {
    // Blue's weakest tower is a princess at 100; Red's weakest is a princess at 900.
    let s = parked(OvertimeTiebreak::LowestTowerHpAbsolute, [4000, 100, 1500], [4000, 900, 1200]);
    assert_eq!(s.outcome(), Some(Outcome::Winner(Team::Red)));
    // And the other way round, same numbers swapped: Blue wins. The verdict is
    // decided by the towers, not by the seat.
    let s = parked(OvertimeTiebreak::LowestTowerHpAbsolute, [4000, 900, 1200], [4000, 100, 1500]);
    assert_eq!(s.outcome(), Some(Outcome::Winner(Team::Blue)));
}

#[test]
fn absolute_compares_a_king_against_a_princess_in_raw_points() {
    // Blue: king down to 500 (its weakest), princesses full. Red: everything full
    // but one princess at 600. 500 < 600 in points, so Blue loses under absolute...
    let [k, p, _] = tower_max();
    let s = parked(OvertimeTiebreak::LowestTowerHpAbsolute, [500, p, p], [k, 600, p]);
    assert_eq!(s.outcome(), Some(Outcome::Winner(Team::Red)));
    // ...while under the fraction rule Blue's king at 500/max_king may be the
    // healthier fraction. Both readings are candidates; the data cannot tell.
    let s = parked(OvertimeTiebreak::LowestTowerHpFraction, [500, p, p], [k, 600, p]);
    let (k, p) = (k as i64, p as i64);
    let expect = match (500 * p).cmp(&(600 * k)) {
        std::cmp::Ordering::Greater => Outcome::Winner(Team::Blue),
        std::cmp::Ordering::Less => Outcome::Winner(Team::Red),
        std::cmp::Ordering::Equal => Outcome::Draw,
    };
    assert_eq!(s.outcome(), Some(expect));
}

#[test]
fn fraction_is_exact_integer_arithmetic_not_a_rounded_percentage() {
    // Two princesses one point apart out of the same max: the fraction rule must
    // still see the difference (a rounded percentage would not).
    let [k, p, _] = tower_max();
    let s = parked(OvertimeTiebreak::LowestTowerHpFraction, [k, p - 1, p], [k, p, p]);
    assert_eq!(s.outcome(), Some(Outcome::Winner(Team::Red)));
}

#[test]
fn an_exact_tie_is_a_draw_under_every_rule() {
    for rule in [OvertimeTiebreak::LowestTowerHpAbsolute, OvertimeTiebreak::LowestTowerHpFraction, OvertimeTiebreak::NoneDraw] {
        let s = parked(rule, [3000, 700, 1500], [3000, 1500, 700]);
        assert_eq!(s.outcome(), Some(Outcome::Draw), "{rule:?}");
    }
}

#[test]
fn a_destroyed_princess_does_not_count_as_a_zero_hp_tower() {
    // Blue lost a princess before the clock ran out (1 crown to Red), Red lost
    // none: crowns decide, the tiebreak is never consulted. The tiebreak only
    // ranks ALIVE towers, so this test also pins that a missing tower is not a
    // 0-hp entry -- set up a level-crown case with one princess gone per side.
    let mut cfg = config();
    cfg.calib.overtime_tiebreak = OvertimeTiebreak::LowestTowerHpAbsolute;
    let mut s = BattleState::new(3, cfg);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Red, 2, 0).unwrap();
    // Blue's standing towers: king 4000, princess 800. Red's: king 4000, princess 900.
    s.scenario_set_tower_hp(Team::Blue, 0, 4000).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 800).unwrap();
    s.scenario_set_tower_hp(Team::Red, 0, 4000).unwrap();
    s.scenario_set_tower_hp(Team::Red, 1, 900).unwrap();
    assert_eq!(s.crowns(), [1, 1]);
    let t = last_overtime_tick(s.config());
    s.scenario_set_tick(t);
    s.tick();
    assert!(s.is_done());
    assert_eq!(s.outcome(), Some(Outcome::Winner(Team::Red)), "800 < 900 among the towers still standing");
}

#[test]
fn the_tiebreak_fires_only_when_overtime_runs_out() {
    // One tick before the end, the same board is still undecided.
    let mut cfg = config();
    cfg.calib.overtime_tiebreak = OvertimeTiebreak::LowestTowerHpAbsolute;
    let mut s = BattleState::new(11, cfg);
    s.scenario_set_tower_hp(Team::Blue, 1, 100).unwrap();
    let t = last_overtime_tick(s.config());
    s.scenario_set_tick(t - 1);
    s.tick();
    assert!(!s.is_done(), "tick {} is not the last overtime tick", t - 1);
    s.tick();
    assert_eq!(s.outcome(), Some(Outcome::Winner(Team::Red)));
}

#[test]
fn the_verdict_is_the_same_from_both_seats() {
    // Rotation symmetry (tests/mirror.rs): swap the two sides' tower
    // tables and the winner swaps, for every rule and a spread of boards.
    let boards: [([i32; 3], [i32; 3]); 4] = [
        ([4000, 100, 1500], [4000, 900, 1200]),
        ([500, 2000, 2000], [4000, 600, 2000]),
        ([3000, 700, 1500], [3000, 1500, 700]),
        ([2500, 2500, 2500], [2500, 2499, 2500]),
    ];
    for rule in [OvertimeTiebreak::LowestTowerHpAbsolute, OvertimeTiebreak::LowestTowerHpFraction, OvertimeTiebreak::NoneDraw] {
        for (b, r) in boards {
            let x = parked(rule, b, r).outcome().unwrap();
            let y = parked(rule, r, b).outcome().unwrap();
            let swapped = match x {
                Outcome::Winner(t) => Outcome::Winner(t.other()),
                Outcome::Draw => Outcome::Draw,
            };
            assert_eq!(y, swapped, "{rule:?} {b:?} vs {r:?}");
        }
    }
}

#[test]
fn shipped_calibration_selects_the_absolute_rule() {
    // The registry's value is what a training run plays under; pin it so a silent
    // edit of calibration.json is a test failure, not a surprise.
    let s = BattleState::new(1, config());
    assert_eq!(s.config().calib.overtime_tiebreak, OvertimeTiebreak::LowestTowerHpAbsolute);
}
