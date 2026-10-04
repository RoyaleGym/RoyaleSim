//! The post-overtime tiebreak (calibration match.OVERTIME_TIEBREAK).
//!
//! WHY IT EXISTS: the engine used to score a match that survived overtime level
//! on crowns as a Draw whatever the towers looked like (2 of 60 random-policy
//! games reached that state, both with a decidable gap). Every scenario here
//! starts at the last tick of overtime with hand-set tower hp, so the verdict is
//! a pure function of the rule and the towers; nothing moves.
//!
//! THE SHIPPED RULE IS THE CLIENT'S DRAIN (client_hp_drain, measured on client 15.535.29): the match goes on past
//! overtime's end, every crown tower drains, and the first fall decides it 1-0 (the last two tests).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test tiebreak`):
//!   tiebreak_decided_at_overtime_end   the drain never runs: the drain tests go red.
mod common;

use common::*;
use royalesim::state::{tiebreak_drain_step, BattleConfig, BattleState, Outcome, OvertimeTiebreak};
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
fn shipped_calibration_selects_the_clients_drain() {
    // The registry's value is what a training run plays under; pin it so a silent
    // edit of calibration.json is a test failure, not a surprise.
    let s = BattleState::new(1, config());
    assert_eq!(s.config().calib.overtime_tiebreak, OvertimeTiebreak::ClientHpDrain);
}

/// A battle under the drain, parked on the last overtime tick with the given tower hp. Returns, per processed tick,
/// (tick, the hp of `watch`'s tower `k`) until the match ends or `limit` ticks pass.
fn drained(blue: [i32; 3], red: [i32; 3], watch: (Team, usize), limit: u32) -> (BattleState, Vec<(u32, i32)>) {
    let mut cfg = config();
    cfg.calib.overtime_tiebreak = OvertimeTiebreak::ClientHpDrain;
    let mut s = BattleState::new(7, cfg);
    for (team, hp) in [(Team::Blue, blue), (Team::Red, red)] {
        for (k, h) in hp.iter().enumerate() {
            s.scenario_set_tower_hp(team, k, *h).unwrap();
        }
    }
    s.scenario_set_tick(last_overtime_tick(s.config()));
    let mut seen = Vec::new();
    for _ in 0..limit {
        let t = s.tick_count();
        s.tick();
        let hp = s.tower_ids(watch.0)[watch.1].and_then(|id| s.entity(id)).map_or(0, |e| e.hp);
        seen.push((t, hp));
        if s.is_done() {
            break;
        }
    }
    (s, seen)
}

/// THE CLIENT'S DRAIN, tick for tick (the oracle's sp-tiebreak-dmg-s0, client 15.535.29): one princess at 2880, the
/// rest untouched. Nothing moves until t6067; then 50 a tick (6067..6104), 40 (6105..6117), 20 (6118..6131), 10
/// (6132..6147) and 1 (6148..6167), when the princess falls and her opponent wins 1-0.
/// PLANT tiebreak_decided_at_overtime_end: the match ends at overtime's end with no crown; this goes red.
#[test]
fn the_drain_runs_as_the_client_and_the_first_fall_wins() {
    let max = tower_max();
    let red = [max[0], max[1], 2880];
    let (s, seen) = drained(max, red, (Team::Red, 2), 400);
    let first_loss = seen.windows(2).find(|w| w[1].1 < w[0].1).map(|w| w[1].0);
    assert_eq!(first_loss, Some(6067), "the drain starts on t6067: {:?}", &seen[..seen.len().min(80)]);
    let step_on = |t: u32| {
        let i = seen.iter().position(|(x, _)| *x == t).unwrap();
        seen[i - 1].1 - seen[i].1
    };
    for (t, step) in [(6067, 50), (6104, 50), (6105, 40), (6117, 40), (6118, 20), (6131, 20), (6132, 10), (6147, 10), (6148, 1), (6166, 1)] {
        assert_eq!(step_on(t), step, "the step on t{t}");
    }
    assert_eq!(seen.last().map(|x| x.0), Some(6167), "the princess falls on t6167");
    assert_eq!(s.crowns(), [1, 0], "the fall is a crown");
    assert_eq!(s.outcome(), Some(Outcome::Winner(Team::Blue)));
}

/// THE EXACT TIE (sp-tiebreak-idle-s0): every tower level. One tick of drain on t6067, then a Draw on t6147.
#[test]
fn level_towers_drain_one_tick_and_draw() {
    let max = tower_max();
    let (s, seen) = drained(max, max, (Team::Blue, 1), 400);
    let lost: Vec<u32> = seen.windows(2).filter(|w| w[1].1 < w[0].1).map(|w| w[1].0).collect();
    assert_eq!(lost, vec![6067], "one tick of drain");
    assert_eq!(seen.last().map(|x| x.0), Some(6147), "the draw on t6147");
    assert_eq!(s.outcome(), Some(Outcome::Draw));
    assert_eq!(s.crowns(), [0, 0]);
}

/// THE SIDES' LOWEST TOWERS LEVEL, THE OTHERS NOT (client 15.535.29, Oracle's sp-tiebreak-eqlow-s0: both right
/// princesses on 2880, the left ones on 3052 and 3004): the rule compares each side's lowest standing tower, so this is
/// the exact tie too, one drain tick on t6067 (-50 on all six) and a draw on t6147, not a drain to a simultaneous fall.
#[test]
fn lowest_towers_level_with_the_others_unequal_is_the_exact_draw() {
    let king = tower_max()[0];
    let (s, seen) = drained([king, 3052, 2880], [king, 3004, 2880], (Team::Red, 1), 400);
    let lost: Vec<u32> = seen.windows(2).filter(|w| w[1].1 < w[0].1).map(|w| w[1].0).collect();
    assert_eq!(lost, vec![6067], "one tick of drain on Red's 3004 tower");
    assert_eq!(seen.last().map(|x| x.0), Some(6147), "the draw on t6147");
    assert_eq!(s.outcome(), Some(Outcome::Draw));
}

/// THE DRAIN'S STEPS AT THEIR MEASURED EDGES (client 15.535.29: 500 drained 40, 460 drained 20, 200 drained 20, 180
/// drained 10, 30 drained 10, 20 drained 1; client 16.402, six live level overtimes: 1018 drained 50, 968 drained 40,
/// 516 drained 40, 488 drained 20, 208 drained 20, 196 drained 10, 21 drained 10, 18 drained 1).
#[test]
fn the_drain_steps_sit_where_both_clients_measured_them() {
    for (lowest, step) in [(1018, 50), (968, 40), (516, 40), (500, 40), (488, 20), (460, 20), (208, 20), (200, 20), (196, 10), (180, 10), (30, 10), (21, 10), (20, 1), (18, 1), (1, 1)] {
        assert_eq!(tiebreak_drain_step(lowest), step, "the step at a lowest tower of {lowest}");
    }
}

/// THE CLIENT'S FREEZE AT A LEVEL OVERTIME'S END (client 16.402, six live level overtimes): units still act on t6000
/// and t6001, every unit, building and spell is gone at the head of t6002 with only the crown towers left, and no play
/// is accepted from t6000. Nothing but the drain touches a tower after that.
/// PLANT tiebreak_board_kept: play goes on past overtime's end; this goes red.
#[test]
fn a_level_overtime_freezes_and_clears_the_board() {
    use royalesim::fixed::Vec2;
    use royalesim::state::DeployError;
    let mut cfg = config();
    cfg.calib.overtime_tiebreak = OvertimeTiebreak::ClientHpDrain;
    let mut s = BattleState::new(7, cfg);
    s.scenario_set_tick(last_overtime_tick(s.config()) - 5);
    let t = |x: i32, y: i32| Vec2::new(x * 18_000, y * 18_000);
    s.spawn_unit(Team::Blue, "Knight", t(4, 10), None).expect("a Blue Knight");
    s.spawn_unit(Team::Red, "Knight", t(14, 22), None).expect("a Red Knight");
    s.spawn_unit(Team::Blue, "Cannon", t(9, 6), None).expect("a Blue Cannon");
    let towers = |s: &BattleState| [Team::Blue, Team::Red].iter().flat_map(|tm| s.tower_ids(*tm)).flatten().count();
    let others = |s: &BattleState| s.entities().count() - towers(s);
    while s.tick_count() < 6000 {
        s.tick();
    }
    assert!(!s.is_done(), "a level overtime does not end at t6000");
    assert_eq!(s.check_deploy_slot(Team::Blue, 0, t(9, 8)), Err(DeployError::GameOver), "no play from t6000");
    s.tick(); // t6000
    s.tick(); // t6001
    assert!(others(&s) >= 3, "units still stand through t6001: {}", others(&s));
    s.tick(); // t6002: the clear at its head
    assert_eq!(others(&s), 0, "only the crown towers stand from t6002");
    let hp = |s: &BattleState| [Team::Blue, Team::Red].iter().flat_map(|tm| s.tower_ids(*tm)).flatten().map(|id| s.entity(id).unwrap().hp).collect::<Vec<_>>();
    let before = hp(&s);
    while s.tick_count() < 6067 {
        s.tick();
    }
    assert_eq!(hp(&s), before, "nothing touches a tower from t6002 until the drain");
}

/// A DELAYED COMMAND STILL WAITING AT THE CLEAR is dropped and reported (`commands_run`, refused GameOver), so a
/// caller that charged it when it was accepted can give it back: a play accepted on t5990 with a 20-tick delay is due
/// on t6010, after the clear at the head of t6002.
#[test]
fn a_command_waiting_at_the_clear_is_reported_dropped() {
    use royalesim::fixed::Vec2;
    use royalesim::state::DeployError;
    let mut cfg = config();
    cfg.calib.overtime_tiebreak = OvertimeTiebreak::ClientHpDrain;
    cfg.command_delay_ticks = [20, 0];
    let deck: Vec<String> = ["Knight", "Archer", "Giant", "Musketeer", "Fireball", "Valkyrie", "HogRider", "Minions"].iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    let mut s = BattleState::new(7, cfg);
    s.scenario_set_tick(5990);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let at = Vec2::new(9 * 18_000, 8 * 18_000);
    s.deploy_slot(Team::Blue, 0, at).expect("the play is accepted, to run in 20 ticks");
    assert_eq!(s.pending_commands(Team::Blue).len(), 1, "scene: the play waits");
    while s.tick_count() < 6002 {
        s.tick();
        assert!(s.commands_run().is_empty(), "nothing ran or dropped before the clear (t{})", s.tick_count() - 1);
    }
    s.tick(); // t6002: the clear at its head
    let run = s.commands_run();
    assert_eq!(run.len(), 1, "the waiting play is reported on the clear's tick: {run:?}");
    assert_eq!((run[0].command.team, run[0].result.clone().map(|_| ())), (Team::Blue, Err(DeployError::GameOver)));
    assert!(s.pending_commands(Team::Blue).is_empty(), "and no longer waits");
}
