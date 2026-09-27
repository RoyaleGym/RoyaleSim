//! cards.CLIENT16402_VALUES, the AreaBuffTime column: the BuffTime of the area a card puts down, a spell's own area (the
//! Freeze) or the area its death leaves (the Ice Golem's slow) (card.rs `CardDb::with_values`, `CardColumn`).
//!
//! THE MEASUREMENT, on the 16.402 corpus: a Freeze holds a walking troop for 70 ticks from the frame it lands, where
//! the 15.535.29 tables' BuffTime 4000 holds it 80 on the engine. The Knight of 20260920-005517 (cast on 703) stands
//! through 773 and walks on 774; the Knight of 20260920-010218 (cast on 312, both seats) the same; the engine holds
//! both through the cast + 80. A frozen Tombstone's spawn clock pauses the same 70 (20260920-070448, 3191 against the
//! engine's 3201). 3500 ms is 70 ticks.
//!
//! NOT LISTED. cards.CLIENT16402_VALUES ships client16402, and its value.values do not carry an AreaBuffTime: the
//! Freeze's 3500 is not scored yet. Every scene here that wants it lists it itself (`with`), as an override would.
//!
//! WHAT IS PINNED, each with its precondition (the Knight walks before the Freeze and again after it):
//!   1. with the Freeze's 3500 listed under client16402, the battle's Freeze row hangs 3500 ms and a walking Knight
//!      it lands on stands still 10 ticks fewer than unlisted;
//!   2. the same list under the old arm none runs the tables' 4000, and so does the shipped list;
//!   3. an Ice Golem's death area takes the column too: listed at 2500, its slow is 2500 ms (the tables: 2000);
//!   4. the column is refused on a card with no area that hangs a buff, and at 0.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="area_buff_time_unread"' CARGO_TARGET_DIR=target/plant cargo test --test
//! area_buff_time16402`): the column is accepted and the area keeps the tables' BuffTime: (1) and (3) go red, (2) and
//! (4) stay green.
mod common;

use common::*;
use royalesim::card::{CardColumn, CardValue, SpellShape};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, CardValuesArm, Calib};
use royalesim::Team;

/// The Freeze's BuffTime on client 16.402 (the 15.535.29 tables: 4000). Not in the shipped value.values.
const FREEZE_16402: i32 = 3500;
/// A value for the Ice Golem's death area, to show the column reaches it (the tables: 2000).
const GOLEM_SLOW: i32 = 2500;

/// The shipped config under `values`, with `listed` (card, AreaBuffTime) added to the values.
fn with(values: CardValuesArm, listed: &[(&str, i32)]) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.card_values = values;
    for (card, value) in listed {
        cfg.calib.card_value_overrides.push(CardValue { card: card.to_string(), column: CardColumn::AreaBuffTime, value: *value });
    }
    cfg
}

/// The BuffTime the battle's data hangs from `card`'s area: its own (a spell) or its death's.
fn area_buff_ms(s: &BattleState, card: &str) -> i32 {
    let c = card_stat(s, card);
    match c.spell.as_ref().or(c.death_area_effect.as_ref()).map(|d| &d.shape) {
        Some(SpellShape::AreaEffect { hit } | SpellShape::PulsingAreaEffect { hit, .. }) => hit.buff.unwrap_or_else(|| panic!("{card}: its area hangs no buff")).time_ms,
        other => panic!("{card}: not an area: {other:?}"),
    }
}

/// The ticks a walking Knight stands still under a Freeze cast on it, from the tick the Freeze holds it.
fn freeze_hold(cfg: BattleConfig) -> (i32, u32) {
    let mut s = BattleState::new(5, cfg);
    let buff = area_buff_ms(&s, "Freeze");
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", Vec2::new(3500 * K, 13000 * K), None).expect("put the Knight down");
    for _ in 0..30 {
        s.tick();
    }
    let before = s.entity(knight).unwrap().pos;
    s.tick();
    let at = s.entity(knight).unwrap().pos;
    assert_ne!(at, before, "the scene drifted: the Knight was not walking");
    s.spawn_unit(Team::Red, "Freeze", at, None).expect("cast the Freeze");
    let mut held = false;
    for _ in 0..40 {
        s.tick();
        if s.entity(knight).is_some_and(|v| v.stun_ms > 0) {
            held = true;
            break;
        }
    }
    assert!(held, "the scene drifted: the Freeze never held the Knight");
    let held_at = s.entity(knight).unwrap().pos;
    let mut still = 0;
    for _ in 0..200 {
        s.tick();
        if s.entity(knight).expect("the scene drifted: the Knight died").pos != held_at {
            return (buff, still);
        }
        still += 1;
    }
    panic!("the scene drifted: the Knight never walked again");
}

#[test]
fn a_listed_freeze_holds_for_its_16402_buff_time() {
    let (unlisted_ms, unlisted) = freeze_hold(with(CardValuesArm::Client16402, &[]));
    let (listed_ms, listed) = freeze_hold(with(CardValuesArm::Client16402, &[("Freeze", FREEZE_16402)]));
    assert_eq!(listed_ms, FREEZE_16402, "the listed Freeze row's BuffTime");
    assert_ne!(unlisted_ms, FREEZE_16402, "the tables already hold the value, so this test checks nothing");
    assert_eq!(listed + ((unlisted_ms - FREEZE_16402) / 50) as u32, unlisted, "listed at {FREEZE_16402} ms the Freeze held the Knight {listed} ticks, unlisted ({unlisted_ms} ms) {unlisted}");
}

#[test]
fn the_old_arm_and_the_shipped_list_run_the_tables_buff_time() {
    let shipped = freeze_hold(config());
    let old = freeze_hold(with(CardValuesArm::None, &[("Freeze", FREEZE_16402)]));
    assert_eq!(shipped, old, "the shipped list (ms, ticks held) against the old arm with the Freeze listed");
    assert!(
        !Calib::shipped().card_value_overrides.iter().any(|v| v.column == CardColumn::AreaBuffTime),
        "the shipped value.values list an AreaBuffTime: {:?}",
        Calib::shipped().card_value_overrides
    );
}

#[test]
fn a_listed_death_area_hangs_its_buff_time() {
    let unlisted = BattleState::new(5, with(CardValuesArm::Client16402, &[]));
    let listed = BattleState::new(5, with(CardValuesArm::Client16402, &[("IceGolemite", GOLEM_SLOW)]));
    assert_ne!(area_buff_ms(&unlisted, "IceGolemite"), GOLEM_SLOW, "the tables already hold the value, so this test checks nothing");
    assert_eq!(area_buff_ms(&listed, "IceGolemite"), GOLEM_SLOW, "the Ice Golem's death area");
}

#[test]
fn the_column_is_refused_where_no_area_hangs_a_buff_and_at_zero() {
    let db = cards();
    let e = db.with_values(&[CardValue { card: "Knight".into(), column: CardColumn::AreaBuffTime, value: 1000 }]).expect_err("a Knight's AreaBuffTime was taken");
    assert!(e.contains("Knight.AreaBuffTime: only an area that hangs a buff"), "{e}");
    let e = db.with_values(&[CardValue { card: "Freeze".into(), column: CardColumn::AreaBuffTime, value: 0 }]).expect_err("a BuffTime of 0 was taken");
    assert!(e.contains("is not a BuffTime"), "{e}");
}
