//! cards.CLIENT16402_VALUES, the Lightning's AreaHitSpeed, and spells.STRIKE_AREA_END: when a striking area strikes on
//! client 16.402, and when it ends (card.rs `strike_gaps`, `CardDb::with_values`; spell.rs `step_spells`, the Strikes
//! arm).
//!
//! THE MEASUREMENT, on the 16.402 corpus: one Lightning cast strikes (20260920-070448, seen from both seats). Cast on
//! the elixir drop D = tick 2982 at (14500, 21500), it makes its strike objects on D + 10 (the red right princess
//! tower, 3052 hp) and D + 20 (a Knight, 1766 hp), and each loss lands a tick later: 265 on D + 11 and 1057 on D + 21
//! (level 11). The 15.535.29 tables' HitSpeed 460 gives D + 9 and D + 18 on the engine's clock
//! (spells.STRIKE_TIMER_LEFTOVER = carried, which fits 67 of 67 client 15.535.29 strike objects); the client 16.402
//! row's 500 gives D + 10 and D + 20 on the same clock.
//!
//! NOT LISTED. cards.CLIENT16402_VALUES ships client16402, and its value.values do not carry the Lightning's
//! AreaHitSpeed: the 500 rests on one cast and is not scored, so the shipped Lightning is the tables' 460 row. Every
//! client16402 scene here adds the 500 to the calibration's values itself (`with_arms`), as an override of the key
//! would.
//!
//! THE THIRD STRIKE, inferred: the 500 row schedules strikes at 500, 1000 and 1500 ms, the third due exactly at the
//! 1500 ms LifeDuration, so on D + 30. The corpus cast had no enemy in reach then. On client 15.535.29 every scheduled
//! slot with a candidate strikes (35 of 35 over 47 casts) and none strikes past the schedule; there the third slot
//! (1380 ms) comes before the life ends. Under spells.STRIKE_AREA_END = with_last_strike the area ends with its last
//! scheduled strike and makes the third; under the shipped at_life_end it ends on the update its life reaches 0 (the
//! area's 30th, k = 29) and loses it.
//!
//! The scenes, at level 11. k = 0 is the first tick run after the cast, so D + n is k = n.
//!   * The corpus cast: a Blue Lightning at (14500, 21500), the red right princess tower at (14500, 25500), and a red
//!     Knight of 1766 hp at (14735, 20500).
//!   * Three targets (the strikes.rs scene): red Knights of 1200, 1400 and 1300 hp at (8000, 22000), (9000, 22000) and
//!     (10000, 22000), a Blue Lightning at (9000, 22000). Struck highest hp first.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the corpus cast under client16402: the tower loses one strike's crown share on k = 11 and the Knight one strike
//!      on k = 21, nothing else, under either end (no third enemy in reach);
//!   2. the corpus cast under none: the same two losses on k = 10 and k = 19 (the tables' 460);
//!   3. the data each arm runs with the 500 added: the Lightning's gaps [500, 500, 500] under client16402 and
//!      [460, 460, 460] under none, LifeDuration 1500 under both;
//!   4. an AreaHitSpeed is refused on a card that is not a striking area, at 0, and past the LifeDuration;
//!   5. the shipped engine runs the tables' 460 row and at_life_end: the ledger's values do not list the Lightning, and
//!      the corpus cast strikes as under none;
//!   6. three targets under client16402 and with_last_strike: losses on k = 11, 21 and 31;
//!   7. three targets under client16402 and at_life_end: losses on k = 11 and 21, the third lost;
//!   8. the area stands after k = 0..29 under client16402 and with_last_strike, 0..28 under at_life_end, and 0..26
//!      under none;
//!   9. three targets under none: the same battle under either end, tick by tick (state_hash), losses on k = 10, 19
//!      and 28;
//!  10. three targets under client16402 and with_last_strike: the same battle under either spells.STRIKE_DUE arm, tick
//!      by tick. That key is the centre-aimed strike's (the Royal Delivery's, tests/royal_delivery.rs); the 500 row's
//!      clock reaches exactly zero on k = 9, 19 and 29, and a strike that picks the highest hp waits for the update
//!      that takes it below zero. Red when the Strikes arm reads the key for every pick: clock_at_or_below_zero then
//!      moves the losses to k = 10, 20 and 30.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test strike_timing16402`):
//!   * `card_values_unread` -- client16402 runs the tables' values: (1), (3), (6), (7) and (8) go red; (5) stays green.
//!   * `strike_timer_restarts` -- the leftover dropped: (1) goes red on k = 22, (2) and (5) on k = 20, (6) on k = 22
//!     and 33, (7) on k = 22, (8) (the area stands to k = 31 under client16402, to k = 28 under none), (9) on k = 20
//!     and 30.
//!   * `strike_area_ends_at_life` -- the life ends the area whatever the key: (6) and (8) go red, (9) stays green.
mod common;

use common::*;
use royalesim::card::{CardColumn, CardValue, SpellShape};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, CardValuesArm, StrikeAreaEnd, StrikeDue};
use royalesim::{EntityId, Team};

const NEW: CardValuesArm = CardValuesArm::Client16402;
const OLD: CardValuesArm = CardValuesArm::None;
const LAST: StrikeAreaEnd = StrikeAreaEnd::WithLastStrike;
const LIFE: StrikeAreaEnd = StrikeAreaEnd::AtLifeEnd;

/// The corpus cast's tap, the red right princess tower, and the Knight about where it stood on D.
const TAP: (i32, i32) = (14500, 21500);
const TOWER_AT: (i32, i32) = (14500, 25500);
const KNIGHT_AT: (i32, i32) = (14735, 20500);
const KNIGHT_HP: i32 = 1766;
const TICKS: u32 = 40;

/// The three-target scene: the tap, and each Knight's (position, hp).
const THREE_TAP: (i32, i32) = (9000, 22000);
const THREE: [((i32, i32), i32); 3] = [((8000, 22000), 1200), ((9000, 22000), 1400), ((10000, 22000), 1300)];
const THREE_TICKS: u32 = 45;

/// The Lightning's AreaHitSpeed on client 16.402 (the 15.535.29 tables: 460). Not in the shipped value.values.
const AREA_HIT_SPEED_16402: i32 = 500;

/// The shipped config under `values` and `end`, the Lightning's 500 added to the values (they apply under client16402
/// only), cards and towers at the corpus's level 11.
fn with_arms(values: CardValuesArm, end: StrikeAreaEnd) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.card_values = values;
    cfg.calib.strike_area_end = end;
    cfg.calib.card_value_overrides.push(CardValue {
        card: "Lightning".to_string(),
        column: CardColumn::AreaHitSpeed,
        value: AREA_HIT_SPEED_16402,
    });
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

/// The shipped config under `values`, the shipped end.
fn with_arm(values: CardValuesArm) -> BattleConfig {
    with_arms(values, config().calib.strike_area_end)
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Lightning's striking area as the battle runs it: (LifeDuration, gaps, one strike's damage at the Blue side's
/// level, a crown tower's share of it, ceil).
fn lightning(s: &BattleState) -> (i32, Vec<i32>, i32, i32) {
    let spell = card_stat(s, "Lightning").spell.as_ref().expect("Lightning loads as a spell");
    let SpellShape::Strikes(d) = &spell.shape else { panic!("Lightning: {:?}", spell.shape) };
    let idx = s.cards().index("Lightning").expect("Lightning loads");
    let dmg = s.cards().scaled(idx, s.config().card_level[0], d.hit.damage).expect("a valid level");
    (d.life_ms, d.gaps_ms.clone(), dmg, (dmg * d.hit.crown_pct + 99) / 100)
}

/// What a run from the cast recorded.
struct Run {
    /// Every (k, victim, hp lost).
    lost: Vec<(u32, EntityId, i32)>,
    /// The ticks k after which the striking area still stands.
    standing: Vec<u32>,
    /// The state hash after each tick.
    hashes: Vec<u64>,
}

/// Run `ticks` ticks from the cast, watching `ids`.
fn run(s: &mut BattleState, ids: &[EntityId], ticks: u32) -> Run {
    let (mut lost, mut standing, mut hashes) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..ticks {
        let before: Vec<Option<i32>> = ids.iter().map(|&id| s.entity(id).map(|v| v.hp)).collect();
        s.tick();
        for (&id, b) in ids.iter().zip(before) {
            let (Some(b), now) = (b, s.entity(id).map_or(0, |v| v.hp)) else { continue };
            if now < b {
                lost.push((k, id, b - now));
            }
        }
        if s.spells().iter().any(|sp| matches!(sp.motion, SpellMotion::Strikes { .. })) {
            standing.push(k);
        }
        hashes.push(s.state_hash());
    }
    Run { lost, standing, hashes }
}

/// Cast the corpus's Lightning under (`values`, `end`) and run TICKS ticks: (the battle, [the tower, the Knight], the
/// run).
fn cast(values: CardValuesArm, end: StrikeAreaEnd) -> (BattleState, [EntityId; 2], Run) {
    cast_with(with_arms(values, end))
}

/// The corpus cast under `cfg`.
fn cast_with(cfg: BattleConfig) -> (BattleState, [EntityId; 2], Run) {
    let mut s = BattleState::new(0, cfg);
    let tower = s
        .tower_ids(Team::Red)
        .into_iter()
        .flatten()
        .find(|t| s.entity(*t).is_some_and(|v| v.pos == at(TOWER_AT)))
        .expect("the red right princess tower stands at (14500, 25500)");
    let knight = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at(KNIGHT_AT), Some(KNIGHT_HP))])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"))[0];
    let tower_hp = s.entity(tower).map_or(0, |v| v.hp);
    assert!(tower_hp > KNIGHT_HP, "the scene drifted: the tower ({tower_hp}) must outrank the Knight ({KNIGHT_HP}), as in the corpus");
    s.spawn_unit(Team::Blue, "Lightning", at(TAP), None).expect("cast Lightning");
    let got = run(&mut s, &[tower, knight], TICKS);
    (s, [tower, knight], got)
}

/// Cast at THREE_TAP over the three Knights under (`values`, `end`) and run THREE_TICKS ticks: (the battle, the
/// Knights in THREE's order, the run).
fn three(values: CardValuesArm, end: StrikeAreaEnd) -> (BattleState, Vec<EntityId>, Run) {
    three_with(with_arms(values, end))
}

/// The three-target scene under `cfg`.
fn three_with(cfg: BattleConfig) -> (BattleState, Vec<EntityId>, Run) {
    let mut s = BattleState::new(0, cfg);
    let spawns: Vec<(Team, &str, Vec2, Option<i32>)> = THREE.iter().map(|&(p, hp)| (Team::Red, "Knight", at(p), Some(hp))).collect();
    let ids = s.scenario_spawn_batch(&spawns).unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    s.spawn_unit(Team::Blue, "Lightning", at(THREE_TAP), None).expect("cast Lightning");
    let got = run(&mut s, &ids, THREE_TICKS);
    (s, ids, got)
}

/// Plants: card_values_unread, strike_timer_restarts.
#[test]
fn under_client16402_the_strikes_land_on_d_plus_11_and_21() {
    for end in [LIFE, LAST] {
        let (s, [tower, knight], got) = cast(NEW, end);
        let (_, _, dmg, share) = lightning(&s);
        assert!(dmg < KNIGHT_HP, "the scene drifted: a strike ({dmg}) would kill the Knight");
        assert_eq!(
            got.lost,
            vec![(11, tower, share), (21, knight, dmg)],
            "{end:?}: client 16.402 strikes the tower on D + 10 and the Knight on D + 20: losses of {share} on D + 11 and {dmg} on D + 21, and no third enemy is in reach on D + 30"
        );
    }
}

/// Plant: strike_timer_restarts.
#[test]
fn under_none_the_tables_460_strikes_on_d_plus_10_and_19() {
    let (s, [tower, knight], got) = cast(OLD, LIFE);
    let (_, _, dmg, share) = lightning(&s);
    assert_eq!(
        got.lost,
        vec![(10, tower, share), (19, knight, dmg)],
        "the tables' HitSpeed 460 strikes on D + 9 and D + 18: losses of {share} on D + 10 and {dmg} on D + 19"
    );
}

/// Plant: card_values_unread.
#[test]
fn each_arm_runs_its_own_lightning_row() {
    let (life, gaps, _, _) = lightning(&BattleState::new(0, with_arm(NEW)));
    assert_eq!((life, gaps), (1500, vec![500, 500, 500]), "client16402");
    let (life, gaps, _, _) = lightning(&BattleState::new(0, with_arm(OLD)));
    assert_eq!((life, gaps), (1500, vec![460, 460, 460]), "none: the 15.535.29 row");
}

#[test]
fn an_area_hit_speed_off_a_striking_area_is_refused() {
    let db = cards();
    for (card, value) in [("Fireball", 500), ("Knight", 500), ("Lightning", 0), ("Lightning", 2000)] {
        let got = db.with_values(&[CardValue { card: card.to_string(), column: CardColumn::AreaHitSpeed, value }]);
        assert!(got.is_err(), "{card} AreaHitSpeed {value} was accepted");
    }
    assert!(
        db.with_values(&[CardValue { card: "Lightning".to_string(), column: CardColumn::AreaHitSpeed, value: 500 }]).is_ok(),
        "the Lightning's 500 is refused"
    );
}

/// The shipped engine strikes as before: the Lightning's 500 is not listed, so whatever arm cards.CLIENT16402_VALUES
/// ships, the Lightning runs the tables' 460 row, whose third strike is due inside its life; spells.STRIKE_AREA_END
/// ships with_last_strike (measured on client 16.402: the 16.402 Vines' catch due at its 1400 ms life falls, Oracle's
/// Oracle 2026-10-07), which moves nothing for the 460 row. Plant: strike_timer_restarts.
#[test]
fn the_shipped_engine_runs_the_tables_lightning_and_with_last_strike() {
    let shipped = config();
    assert_eq!(shipped.calib.strike_area_end, LAST, "spells.STRIKE_AREA_END ships with_last_strike");
    let listed: Vec<&CardValue> = shipped.calib.card_value_overrides.iter().filter(|v| v.column == CardColumn::AreaHitSpeed).collect();
    assert!(listed.is_empty(), "cards.CLIENT16402_VALUES lists an AreaHitSpeed before it is scored: {listed:?}");
    let mut at_11 = shipped;
    at_11.card_level = [11, 11];
    at_11.tower_level = [11, 11];
    let (s, [tower, knight], got) = cast_with(at_11);
    let (life, gaps, dmg, share) = lightning(&s);
    assert_eq!((life, gaps), (1500, vec![460, 460, 460]), "the shipped Lightning is the 15.535.29 row");
    assert_eq!(got.lost, vec![(10, tower, share), (19, knight, dmg)], "the shipped engine strikes the corpus cast as under none");
}

/// Plants: strike_area_ends_at_life, card_values_unread, strike_timer_restarts.
#[test]
fn under_client16402_and_with_last_strike_three_knights_lose_on_d_plus_11_21_and_31() {
    let (s, ids, got) = three(NEW, LAST);
    let (_, _, dmg, _) = lightning(&s);
    assert!(dmg < THREE[0].1, "the scene drifted: a strike ({dmg}) would kill a Knight and hide the next pick");
    assert_eq!(
        got.lost,
        vec![(11, ids[1], dmg), (21, ids[2], dmg), (31, ids[0], dmg)],
        "the 500 row's three strikes, the third due at the 1500 ms LifeDuration: D + 10, D + 20 and D + 30, highest hp first"
    );
}

/// Plants: card_values_unread, strike_timer_restarts.
#[test]
fn under_client16402_and_at_life_end_the_third_strike_is_lost() {
    let (s, ids, got) = three(NEW, LIFE);
    let (_, _, dmg, _) = lightning(&s);
    assert_eq!(
        got.lost,
        vec![(11, ids[1], dmg), (21, ids[2], dmg)],
        "at_life_end: the area ends on D + 29, before the third strike's D + 30"
    );
}

/// Plants: strike_area_ends_at_life, card_values_unread, strike_timer_restarts.
#[test]
fn the_area_ends_with_its_last_strike_or_its_life() {
    for (values, end, last) in [(NEW, LAST, 29), (NEW, LIFE, 28), (OLD, LAST, 26), (OLD, LIFE, 26)] {
        let (_, _, got) = three(values, end);
        assert_eq!(got.standing, (0..=last).collect::<Vec<u32>>(), "{values:?} / {end:?}: the area stands after k = 0..{last}");
    }
}

/// Plant: strike_timer_restarts (the strikes move under both arms alike). strike_area_ends_at_life and
/// card_values_unread leave it green: none runs the tables' 460 either way.
#[test]
fn the_15_535_29_row_runs_the_same_battle_under_either_end() {
    let (s, ids, a) = three(OLD, LIFE);
    let (_, _, b) = three(OLD, LAST);
    let (_, _, dmg, _) = lightning(&s);
    assert_eq!(a.lost, vec![(10, ids[1], dmg), (19, ids[2], dmg), (28, ids[0], dmg)], "none: D + 9, D + 18 and D + 27");
    assert_eq!((&a.lost, &a.standing), (&b.lost, &b.standing), "none: the same strikes and the same end under either arm");
    assert_eq!(a.hashes, b.hashes, "none: the same state after every tick under either arm");
}

/// spells.STRIKE_DUE does not move the Lightning: the 500 row's clock reaches exactly zero on k = 9, 19 and 29, and
/// under either arm the strikes land on k = 11, 21 and 31, in the same battle tick by tick (WHAT IS PINNED, 10).
#[test]
fn the_lightning_does_not_read_spells_strike_due() {
    let under = |due: StrikeDue| {
        let mut cfg = with_arms(NEW, LAST);
        cfg.calib.strike_due = due;
        three_with(cfg)
    };
    let (s, ids, at_zero) = under(StrikeDue::ClockAtOrBelowZero);
    let (_, _, below) = under(StrikeDue::ClockBelowZero);
    let (_, _, dmg, _) = lightning(&s);
    assert_eq!(
        at_zero.lost,
        vec![(11, ids[1], dmg), (21, ids[2], dmg), (31, ids[0], dmg)],
        "clock_at_or_below_zero: the Lightning's strikes wait for the clock to fall below zero, D + 10, D + 20 and D + 30"
    );
    assert_eq!((&at_zero.lost, &at_zero.standing), (&below.lost, &below.standing), "the same strikes and the same end under either arm");
    assert_eq!(at_zero.hashes, below.hashes, "the same state after every tick under either arm");
}
