//! A BATTLE'S END (calibration match.BATTLE_END; state.rs `phase_judge`).
//!
//! `rules` is the game's and the ledger's: a fallen King ends the battle at once, and a crown lead at regular time's
//! end ends it. `overtime_end` is a replay's: nothing ends the battle before overtime's end, so a replay scored against
//! a recorded battle plays on past a King the engine lost and the client did not, and a play after that point is put
//! down where `rules` refused it (DeployError::GameOver). At overtime's end the crowns decide as they would.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test battle_end`):
//!   battle_end_unread   every battle ends by the rules, whatever the key: the overtime_end assertions go red.
mod common;

use common::*;
use royalesim::state::{BattleConfig, BattleEnd, BattleState, Calib, DeployError, Outcome};
use royalesim::Team;

fn battle(end: BattleEnd) -> BattleState {
    let mut cfg = config();
    cfg.calib.battle_end = end;
    BattleState::new(5, cfg)
}

/// The tick whose Judge phase reaches `seconds` of battle time: elapsed_ms(tick + 1) >= seconds * 1000.
fn last_tick_of(cfg: &BattleConfig, seconds: i64) -> u32 {
    (seconds * 1000 / cfg.calib.tick_ms as i64 - 1) as u32
}

#[test]
fn the_ledger_ships_the_games_end() {
    assert_eq!(Calib::shipped().battle_end, BattleEnd::Rules);
}

#[test]
fn a_fallen_king_ends_the_battle_only_under_the_rules() {
    for (end, ends) in [(BattleEnd::Rules, true), (BattleEnd::OvertimeEnd, false)] {
        let mut s = battle(end);
        let king = s.tower_ids(Team::Red)[0].expect("Red's King");
        assert!(s.debug_set_hp(king, 0));
        run_until(&mut s, 5, |s| s.crowns()[0] == 3);
        assert_eq!(s.crowns(), [3, 0], "{end:?}: Red's King is down");
        assert_eq!(s.is_done(), ends, "{end:?}");
        let knight = s.spawn_unit(Team::Blue, "Knight", t(850, 1050), None);
        if ends {
            assert_eq!(s.outcome(), Some(Outcome::Winner(Team::Blue)));
            assert!(matches!(knight, Err(DeployError::GameOver)), "{knight:?}");
        } else {
            knight.expect("a play after the King's fall is put down");
            let c = s.config().calib.clone();
            let t = last_tick_of(s.config(), (c.regular_time_s + c.overtime_s) as i64);
            s.scenario_set_tick(t - 1);
            s.tick();
            assert!(!s.is_done(), "overtime_end: not before overtime's end");
            s.tick();
            assert_eq!(s.outcome(), Some(Outcome::Winner(Team::Blue)), "at overtime's end the crowns decide");
        }
    }
}

#[test]
fn a_crown_lead_at_regular_times_end_starts_overtime_under_overtime_end() {
    for end in [BattleEnd::Rules, BattleEnd::OvertimeEnd] {
        let mut s = battle(end);
        s.scenario_set_tower_hp(Team::Red, 1, 0).unwrap();
        assert_eq!(s.crowns(), [1, 0]);
        let t = last_tick_of(s.config(), s.config().calib.regular_time_s as i64);
        s.scenario_set_tick(t);
        s.tick();
        if end == BattleEnd::Rules {
            assert_eq!(s.outcome(), Some(Outcome::Winner(Team::Blue)), "the rules: a lead at regular time's end wins");
        } else {
            assert!(!s.is_done(), "overtime_end: the lead does not end it");
            assert!(s.is_overtime(), "overtime begins whatever the crowns");
        }
    }
}
