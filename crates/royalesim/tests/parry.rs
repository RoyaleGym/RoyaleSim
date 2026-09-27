//! THE COUNTER (card.rs `ParryDef`, `parry_of`; state.rs `note_parry`, `parry_pass`, `parry_pick`, `land_buff`,
//! `fire_scheduled`; entity.rs `Entities::held`): the Ronin.
//!
//! THE LAW, measured on client 15.535.29 in the Ronin scenarios (5 counters, level 11; the 16.402 corpus holds no
//! Ronin). With H the tick a countered hit lands:
//!   - only a ground melee hit is countered (parry.COUNTERED_HITS = ground_melee): a Knight's and a Skeleton's were;
//!     Musketeer shots, princess tower arrows, a Zap and Bats' hits were not;
//!   - the Ronin takes nothing on H (DefenseScalar 0), and its hp is unchanged on H + 1;
//!   - the attacker is slowed, not stunned: its attack progress gains 2 a tick (5 % of 50) on H + 1 .. H + 10 and 50
//!     from H + 11, its load timer keeps falling 50 a tick, it keeps its target and does not step
//!     (status.FULL_STOP_BUFF_IS_STUN = stun_timer_speed_and_hit_speed_zero); a Knight's next hits land on H + 34 and
//!     H + 58, in full;
//!   - the attacker takes twice the countered hit (DamageScalar 200, not level-scaled) on H + 6, 300 ms after the
//!     counter (actions.SUB_ACTIONS_DELAY = from_group_start);
//!   - the Ronin's own swing restarts at the counter (parry.SELF_LOCK = attack_restart): its progress reads 0 on H and
//!     it runs a fresh cycle, so after a hit of its own 2 ticks before H its next hit lands on H + 25;
//!   - the counter is ready again 70 ticks after H (parry.COOLDOWN_START = parry_tick: a hit 67 ticks after landed in
//!     full, one 71 after was countered), and it is not limited to the Ronin's own target.
//!
//! THE DUEL most tests run: a Blue Ronin at (9000, 13500) and a Red Knight at (9000, 14700), both at level 11, from
//! tick 200, out of every tower's reach. The Ronin hits first, on tick 207 (its fresh entry, LoadTime 1000 less a
//! load timer of 0, plus 50, reaches HitSpeed 1400 on its 8th tick), and the Knight on 209 = H (750 on its 1st tick,
//! 1200 on its 10th): the measured runs' order, the Ronin's hit 2 ticks before the counter. Every tick below is read
//! off the run (the Knight's progress crossing its HitSpeed), never typed, and the precondition says when the scene
//! drifted. An hp drop is read between two ticks.
//!
//! WHAT IS PINNED, each test naming the arm it tests through its config:
//!   1. `a_ready_counter_takes_nothing_and_the_attacker_takes_twice_its_hit_six_ticks_later` (measured; the other
//!      arm of actions.SUB_ACTIONS_DELAY, cumulative, lands it on H + 7);
//!   2. `the_countered_attacker_keeps_its_target_and_swings_at_a_twentieth` (measured; under stun_timer the Knight's
//!      progress is held instead);
//!   3. `the_counter_restarts_the_ronins_own_swing` (measured, both creation orders; under SELF_LOCK = none its old
//!      cycle runs on and its next hit is one tick later);
//!   4. `the_reflect_is_not_scaled_by_the_ronins_level` (the tables' DAMAGE_TYPE row; no mixed-level measurement);
//!   5. `only_a_ground_melee_hit_is_countered` (measured: Musketeer shots, princess tower arrows, a Zap, a Bat; under
//!      COUNTERED_HITS = melee the Bat's first hit is countered);
//!   6. `a_reflect_is_never_itself_countered` (engine reading: the reflect is not written by an attack);
//!   7. `parry_pick_orders_by_creation_then_amount` (the pick alone, both arms; never measured),
//!      `same_tick_hits_are_picked_by_the_arm` (a Skeleton created first and a Knight landing on one tick) and
//!      `same_tick_hits_are_picked_by_creation_not_by_slot` (three attackers in reused slots, so the creation order,
//!      the slot order and the amounts all disagree);
//!   8. `a_counter_whose_attacker_dies_first_answers_nothing` (engine reading);
//!   9. `a_countered_hit_leaves_the_ronin_standing` (the countered hit never lands);
//!  10. `the_next_counter_is_ready_seventy_ticks_after_the_counter` (measured; under lock_end it is 80);
//!  11. `the_counter_is_ready_while_deploying` (READY_AT = spawn, a guess; under deploy_end the hit lands);
//!  12. `a_reflect_in_flight_survives_a_save` (engine invariant);
//!  13. `the_counter_cooldown_is_state` (the cooldown and the reflect in flight are hashed; the column is not hashed on
//!      a card without a counter);
//!  14. `a_row_that_stops_the_walk_but_not_the_clock_is_not_a_stun` (synthetic, both arms; a Zap still stuns under
//!      both; the Ronin's row is the only such row in the table);
//!  15. `the_loader_reads_the_ronins_counter` and `a_counter_graph_of_another_shape_is_refused` (the 15.535.29 table,
//!      and synthetic refusals).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test parry`):
//!   * `parry_never_counters` -- no counter ever takes a hit: 1, 2, 3, 4, 6, 8, 9, 10, 11, 12, 13 and 7's second
//!     test red.
//!   * `parry_pick_by_slot` -- the pick takes the hit written first whatever the arm: 7 red (all three tests; in the
//!     first scene only under largest_amount, the slot order being the creation order there; in the reused-slot
//!     scene under both arms).
//!   * `parry_seq_from_slot` -- a candidate carries its attacker's slot as its creation order: 7's reused-slot test
//!     red, under first_created_attacker. The other two stay green: the pick alone never reads the wiring, and in
//!     the first scene the slot order is the creation order.
//!   * `parry_reflect_level_scaled` -- the reflect scaled by the Ronin's level as a card stat: 1, 4, 6, 8, 11, 12 and
//!     7's second test red (every test that reads the reflect's amount; 5's Bat and 10's Skeleton die either way).
//!   * `parry_counters_air` -- a flying attacker's hit is countered under ground_melee: 5 red (the Bat).
//!   * `split_stop_is_stun` -- the Ronin's stun row holds the unit as a stun would: 2, 14 red. The rest stay green:
//!     held or slowed, the Knight's next hit comes on H + 34 either way.
//!   * `hash_skips_parry` -- the cooldown is not hashed: 13 red.
//!   * `parry_shape_unchecked` -- any graph that carries a counter block loads: 15's refusals red.
//!   * `scheduled_dropped_on_save` (tests/transform.rs's) -- the reflect in flight is lost across a save: 12 red.
//!   * `hash_skips_scheduled` (tests/transform.rs's) -- the scheduled list is not hashed: 13 red (the reflect).
//!
//! 5's shots, arrows and Zap have no plant: in this design a counter reads only the hits an attack writes in the
//! Attack phase, so there is no site where a shot could be counted. No existing gate should go red under any plant
//! above: no card loaded before this file carries a counter, and the Ronin's stun row is the only row the new arm of
//! status.FULL_STOP_BUFF_IS_STUN reads differently.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{
    parry_pick, BattleConfig, BattleState, FullStopBuff, ParryCand, ParryCooldownStart, ParryHits, ParryPick, ParryReadyAt, ParrySelf, ScheduledAction,
    SubActionsDelay, TickOrder,
};
use royalesim::status::{compose, Sel};
use royalesim::{EntityId, Team};

/// The level every client measurement here was taken at.
const LEVEL: i32 = 11;
/// Measured on client 15.535.29 at level 11: a Knight's hit, the Ronin's, a Skeleton's, a Bat's, a Musketeer's shot
/// and a Zap.
const KNIGHT_HIT: i32 = 202;
const RONIN_HIT: i32 = 371;
const SKELETON_HIT: i32 = 81;
const BAT_HIT: i32 = 81;
const MUSKETEER_SHOT: i32 = 217;
const ZAP: i32 = 192;
/// Measured: the reflect lands this many ticks after the counter, the slowed attacker gains this much a tick for
/// this many ticks, the Ronin's next hit after its own hit 2 ticks before the counter, and the counter's re-arm.
const REFLECT_AFTER: u32 = 6;
const SLOW_STEP: i32 = 2;
const SLOW_TICKS: u32 = 10;
const RONIN_NEXT: u32 = 25;
const READY_AFTER: u32 = 70;

/// The duel (module doc).
const T0: u32 = 200;
const RONIN_AT: (i32, i32) = (9000, 13500);
const KNIGHT_AT: (i32, i32) = (9000, 14700);
/// Where the tape keeps the Ronin and the Knight of the duel.
const RONIN: usize = 0;
const KNIGHT: usize = 1;

const REGENERATE: &str = "cards.json carries no parry block on the Ronin: regenerate it with tools/extract_cards.py";

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [LEVEL, LEVEL];
    cfg.tower_level = [LEVEL, LEVEL];
    cfg
}

/// The shipped arms, asserted where a test says it pins them.
fn shipped() -> BattleConfig {
    let cfg = level11(config());
    let c = &cfg.calib;
    assert_eq!(c.parry_hits, ParryHits::GroundMelee, "the shipped parry.COUNTERED_HITS");
    assert_eq!(c.parry_pick, ParryPick::FirstCreatedAttacker, "the shipped parry.SAME_TICK_PICK");
    assert_eq!(c.parry_self, ParrySelf::AttackRestart, "the shipped parry.SELF_LOCK");
    assert_eq!(c.parry_cooldown_start, ParryCooldownStart::ParryTick, "the shipped parry.COOLDOWN_START");
    assert_eq!(c.parry_ready_at, ParryReadyAt::Spawn, "the shipped parry.READY_AT");
    assert_eq!(c.sub_actions_delay, SubActionsDelay::FromGroupStart, "the shipped actions.SUB_ACTIONS_DELAY");
    assert_eq!(c.full_stop_buff_is_stun, FullStopBuff::SpeedAndHitSpeedZero, "the shipped status.FULL_STOP_BUFF_IS_STUN");
    assert_eq!(c.tick_order, TickOrder::Client16402, "the shipped match.TICK_ORDER");
    cfg
}

fn hit_speed(s: &BattleState, card: &str) -> i32 {
    card_stat(s, card).hit_speed_ms
}

/// `card`'s Damage at the battle's level for `team`.
fn damage(s: &BattleState, card: &str, team: Team) -> i32 {
    let db = s.cards();
    let idx = db.index(card).unwrap_or_else(|| panic!("{card} loads"));
    db.scaled(idx, s.config().card_level[team as usize], db.get(idx).damage).expect("a valid level")
}

// ---------------------------------------------------------------------------
// the tape

/// What one entity showed after one tick.
#[derive(Clone, Copy, Debug)]
struct Row {
    hp: i32,
    attack_ms: i32,
    load_ms: i32,
    target: Option<EntityId>,
    pos: Vec2,
    stun_ms: i32,
    parry_ms: i32,
    buffed: bool,
    deploying: bool,
}

fn row(s: &BattleState, id: EntityId) -> Option<Row> {
    s.entity(id).map(|e| Row {
        hp: e.hp,
        attack_ms: e.attack_ms,
        load_ms: e.attack_load_ms,
        target: e.target,
        pos: e.pos,
        stun_ms: e.stun_ms,
        parry_ms: e.parry_ms,
        buffed: e.buffs.iter().any(|b| !b.is_empty()),
        deploying: e.deploying,
    })
}

/// `ids` before the next tick and after each of the `n` ticks that follow. Ticks are numbered as `tick_count` numbers
/// them: the first one run is `first`, and `at(k, t)` is entity `k` after tick `t` (`t = first - 1` is before it).
struct Tape {
    first: u32,
    rows: Vec<Vec<Option<Row>>>,
}

impl Tape {
    fn record(s: &mut BattleState, ids: &[EntityId], n: u32) -> Tape {
        let first = s.tick_count();
        let mut rows: Vec<Vec<Option<Row>>> = ids.iter().map(|id| vec![row(s, *id)]).collect();
        for _ in 0..n {
            s.tick();
            for (k, id) in ids.iter().enumerate() {
                rows[k].push(row(s, *id));
            }
        }
        Tape { first, rows }
    }

    fn last(&self) -> u32 {
        self.first + self.rows[0].len() as u32 - 2
    }

    fn get(&self, k: usize, t: u32) -> Option<Row> {
        let j = (t + 1).checked_sub(self.first).expect("a tick before the tape") as usize;
        self.rows[k].get(j).copied().flatten()
    }

    fn at(&self, k: usize, t: u32) -> Row {
        self.get(k, t).unwrap_or_else(|| panic!("entity {k} is gone after tick {t}"))
    }

    /// The hp entity `k` lost on tick `t`.
    fn lost(&self, k: usize, t: u32) -> i32 {
        self.at(k, t - 1).hp - self.at(k, t).hp
    }

    /// Every tick of `from..=to` on which entity `k` lost hp, with the amount.
    fn losses(&self, k: usize, from: u32, to: u32) -> Vec<(u32, i32)> {
        (from..=to).map(|t| (t, self.lost(k, t))).filter(|l| l.1 != 0).collect()
    }

    /// The ticks on which entity `k` hit: its attack progress crossed a multiple of its HitSpeed `hs`
    /// (combat.ATTACK_CYCLE = progress_credit).
    fn fires(&self, k: usize, hs: i32) -> Vec<u32> {
        (self.first..=self.last())
            .filter(|&t| match (self.get(k, t - 1), self.get(k, t)) {
                (Some(a), Some(b)) => b.attack_ms / hs > a.attack_ms / hs,
                _ => false,
            })
            .collect()
    }
}

/// The duel (module doc) under `cfg`, the Ronin at `ronin_hp` when given.
fn duel_with(cfg: BattleConfig, ronin_hp: Option<i32>) -> (BattleState, Vec<EntityId>) {
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tick(T0);
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Ronin", at(RONIN_AT), ronin_hp), (Team::Red, "Knight", at(KNIGHT_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    (s, ids)
}

/// The duel run for `n` ticks, and H: the tick the Knight's first hit lands. Checks the scene: the Ronin's own first
/// hit came 2 ticks before H, as in the measured runs whose next hit this file pins.
fn duel_tape(cfg: BattleConfig, n: u32) -> (BattleState, Vec<EntityId>, Tape, u32) {
    let (mut s, ids) = duel_with(cfg, None);
    let tape = Tape::record(&mut s, &ids, n);
    let h = *tape.fires(KNIGHT, hit_speed(&s, "Knight")).first().expect("the scene drifted: the Knight never hit");
    let ronin_first = *tape.fires(RONIN, hit_speed(&s, "Ronin")).first().expect("the scene drifted: the Ronin never hit");
    assert_eq!(ronin_first + 2, h, "the scene drifted: the Ronin's first hit is not 2 ticks before the Knight's");
    (s, ids, tape, h)
}

// ---------------------------------------------------------------------------
// (1), (2), (3), (4) the counter on a Knight

/// Plants: parry_never_counters, parry_reflect_level_scaled.
#[test]
fn a_ready_counter_takes_nothing_and_the_attacker_takes_twice_its_hit_six_ticks_later() {
    let (s, _, tape, h) = duel_tape(shipped(), 40);
    assert_eq!(damage(&s, "Knight", Team::Red), KNIGHT_HIT, "the Knight's hit at level 11, from the data");
    // The Ronin's hp unchanged on H and H + 1 (measured, 5 of 5).
    assert_eq!((tape.lost(RONIN, h), tape.lost(RONIN, h + 1)), (0, 0), "the countered hit landed");
    // The Knight: twice its hit, on H + 6 and on no other tick of H .. H + 10 (measured, 5 of 5: 1395 -> 991).
    assert_eq!(tape.losses(KNIGHT, h, h + 10), vec![(h + REFLECT_AFTER, 2 * KNIGHT_HIT)]);
    assert_eq!(tape.at(KNIGHT, h - 1).hp, tape.at(KNIGHT, h + REFLECT_AFTER).hp + 2 * KNIGHT_HIT);

    // actions.SUB_ACTIONS_DELAY = cumulative reads the group's [50, 300, 150] as gaps: the reflect 350 ms out, H + 7.
    let mut cfg = shipped();
    cfg.calib.sub_actions_delay = SubActionsDelay::Cumulative;
    let (_, _, tape, h) = duel_tape(cfg, 40);
    assert_eq!(tape.losses(KNIGHT, h, h + 10), vec![(h + REFLECT_AFTER + 1, 2 * KNIGHT_HIT)], "cumulative");
}

/// Plants: split_stop_is_stun, parry_never_counters.
#[test]
fn the_countered_attacker_keeps_its_target_and_swings_at_a_twentieth() {
    let (s, ids, tape, h) = duel_tape(shipped(), 70);
    let (hs, dt) = (hit_speed(&s, "Knight"), s.config().calib.tick_ms);
    // The stun row's composed hit speed on one tick: 5 % of 50, truncated.
    let p = card_stat(&s, "Ronin").parry.expect(REGENERATE);
    let step = compose([s.cards().buffs[p.stun.buff as usize]].iter(), Sel::HitSpeed, dt);
    assert_eq!(step, SLOW_STEP, "the stun row's hit speed, from the data");
    let k = |t: u32| tape.at(KNIGHT, t);
    assert_eq!(k(h).attack_ms, hs, "precondition: the Knight's progress on its hit");
    for j in 1..=SLOW_TICKS {
        let r = k(h + j);
        assert_eq!(r.attack_ms, hs + step * j as i32, "the Knight's progress on H + {j} (measured: 1202 .. 1220)");
        assert_eq!(r.load_ms, (k(h).load_ms - dt * j as i32).max(0), "the load timer keeps falling on H + {j}");
        assert_eq!(r.stun_ms, 0, "the Knight is not stunned on H + {j}");
        // The row is on the Knight through H + 10's attack step: it goes in the decrement at the end of H + 10.
        assert!(k(h + j - 1).buffed, "the Knight carries the stun row into H + {j}");
        assert_eq!(r.target, Some(ids[RONIN]), "the Knight keeps its target on H + {j}");
        assert_eq!(r.pos, k(h).pos, "the Knight does not step on H + {j}");
    }
    assert_eq!(k(h + SLOW_TICKS + 1).attack_ms, hs + step * SLOW_TICKS as i32 + dt, "50 a tick again from H + 11");
    assert!(!k(h + SLOW_TICKS).buffed, "the 500 ms row is gone after H + 10");
    // Its next hits: H + 34 and H + 58, each in full on the Ronin, whose counter is spent (measured, 2 of 2).
    assert_eq!(tape.fires(KNIGHT, hs)[..3], [h, h + 34, h + 58]);
    assert_eq!((tape.lost(RONIN, h + 34), tape.lost(RONIN, h + 58)), (KNIGHT_HIT, KNIGHT_HIT));

    // status.FULL_STOP_BUFF_IS_STUN = stun_timer: the same row is a stun, and the Knight's progress is held.
    let mut cfg = shipped();
    cfg.calib.full_stop_buff_is_stun = FullStopBuff::StunTimer;
    let (_, _, tape, h) = duel_tape(cfg, 30);
    for j in 1..=SLOW_TICKS {
        assert_eq!(tape.at(KNIGHT, h + j).attack_ms, hs, "stun_timer holds the Knight's progress on H + {j}");
    }
    assert!(tape.at(KNIGHT, h + 1).stun_ms > 0, "stun_timer drives the hold timer");
}

/// Plants: parry_never_counters.
#[test]
fn the_counter_restarts_the_ronins_own_swing() {
    let (s, ids, tape, h) = duel_tape(shipped(), 40);
    let c = card_stat(&s, "Ronin");
    let (hs, lt, dt) = (c.hit_speed_ms, c.load_time_ms, s.config().calib.tick_ms);
    let r = |t: u32| tape.at(RONIN, t);
    // Its progress reads 0 on H (measured: from 1450), then a fresh cycle: the entry LoadTime less the load timer,
    // plus 50, the load timer reloaded to LoadTime (measured: 200 with 1000 on H + 1).
    assert_eq!(r(h).attack_ms, 0, "the Ronin's progress on the counter tick");
    let load_h = r(h).load_ms;
    assert_eq!(r(h + 1).attack_ms, lt - (load_h - dt) + dt, "the fresh entry on H + 1");
    assert_eq!(r(h + 1).load_ms, lt, "the load timer reloads to LoadTime");
    assert_eq!(r(h + 1).target, Some(ids[KNIGHT]), "the Ronin keeps its target");
    // Its next hit: H + (HitSpeed - LoadTime + its load on H - 50) / 50 = H + 25 (measured, 2 of 2).
    let next = |tape: &Tape, h: u32| tape.fires(RONIN, hs).into_iter().find(|t| *t > h).expect("the Ronin hit again");
    assert_eq!(((hs - lt + load_h - dt) / dt) as u32, RONIN_NEXT, "the formula's operands");
    assert_eq!(next(&tape, h), h + RONIN_NEXT);

    // Under the sequential order, with the Knight created first, the Ronin updates after the counter within the tick,
    // so it reads its fresh entry on H itself (measured: 150 with 1000) and still hits on H + 25.
    let mut cfg = shipped();
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tick(T0);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT_AT), None).expect("the Knight, created first");
    let ronin = s.scenario_spawn_now(Team::Blue, "Ronin", at(RONIN_AT), None).expect("the Ronin, created second");
    let tape = Tape::record(&mut s, &[ronin, knight], 40);
    let h = tape.fires(KNIGHT, hit_speed(&s, "Knight"))[0];
    let load_before = tape.at(RONIN, h - 1).load_ms;
    assert_eq!(tape.at(RONIN, h).attack_ms, lt - (load_before - dt) + dt, "the fresh entry on H under the sequential order");
    assert_eq!(tape.at(RONIN, h).load_ms, lt);
    assert_eq!(tape.at(RONIN, h + 1).attack_ms, tape.at(RONIN, h).attack_ms + dt);
    assert_eq!(next(&tape, h), h + RONIN_NEXT, "the sequential order's next hit");

    // parry.SELF_LOCK = none: the old cycle runs on (1500 on H) and the next hit is one tick later.
    let mut cfg = shipped();
    cfg.calib.parry_self = ParrySelf::None;
    let (_, _, tape, h) = duel_tape(cfg, 40);
    assert_eq!(tape.at(RONIN, h).attack_ms, tape.at(RONIN, h - 1).attack_ms + dt, "none leaves the swing running");
    assert_eq!(next(&tape, h), h + ((2 * hs - tape.at(RONIN, h).attack_ms) / dt) as u32, "none: the old cycle's next hit");
    assert_eq!(next(&tape, h), h + RONIN_NEXT + 1);
}

/// Plants: parry_reflect_level_scaled, parry_never_counters.
#[test]
fn the_reflect_is_not_scaled_by_the_ronins_level() {
    // A level-9 Ronin against a level-11 Knight: the reflect is still twice the Knight's hit. The damage type's
    // EnableLevelScaling is false; no measurement mixed levels.
    let mut cfg = shipped();
    cfg.card_level = [9, LEVEL];
    let (s, _, tape, h) = duel_tape(cfg, 40);
    assert_eq!(tape.losses(KNIGHT, h, h + 10), vec![(h + REFLECT_AFTER, 2 * KNIGHT_HIT)]);
    // The test discriminates: the same figure scaled as a card stat at level 9 is another number.
    let db = s.cards();
    let ronin = db.index("Ronin").expect("the Ronin loads");
    assert_ne!(db.scaled(ronin, 9, 2 * KNIGHT_HIT), Ok(2 * KNIGHT_HIT), "level 9 scales a card stat");
}

// ---------------------------------------------------------------------------
// (5) which hits

/// A Red Ronin standing at (9000, 14500), attacking a Blue Mortar at (9000, 12500) that cannot answer it (it stands
/// inside the Mortar's MinimumRange) and that has hp to last: nothing hits the Ronin, and its counter is ready.
fn mortar_scene(cfg: BattleConfig) -> (BattleState, EntityId) {
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tick(T0);
    let mortar = s.scenario_spawn_now(Team::Blue, "Mortar", at((9000, 12500)), Some(20000)).expect("place the Mortar");
    let ronin = s.scenario_spawn_now(Team::Red, "Ronin", at((9000, 14500)), None).expect("place the Ronin");
    for _ in 0..3 {
        s.tick();
    }
    let e = s.entity(ronin).expect("the Ronin stands");
    assert_eq!(e.target, Some(mortar), "the scene drifted: the Ronin is not attacking the Mortar");
    assert_eq!((e.hp, e.parry_ms), (e.max_hp, 0), "the scene drifted: something reached the Ronin");
    (s, ronin)
}

/// Plant: parry_counters_air (the Bat).
#[test]
fn only_a_ground_melee_hit_is_countered() {
    let cfg = shipped();
    // (a) Musketeer shots land in full, and the Musketeer takes nothing (measured: 7 x 217, no reset, no reflect).
    let (mut s, ronin) = mortar_scene(cfg.clone());
    let musk = s.scenario_spawn_now(Team::Blue, "Musketeer", at((9000, 9000)), None).expect("place the Musketeer");
    let tape = Tape::record(&mut s, &[ronin, musk], 120);
    let shots = tape.losses(0, tape.first, tape.last());
    assert!(shots.len() >= 3, "only {} shots landed, so this looked at nothing", shots.len());
    assert!(shots.iter().all(|l| l.1 == MUSKETEER_SHOT), "a shot was not taken in full: {shots:?}");
    for t in tape.first..=tape.last() {
        assert_eq!(tape.at(0, t).parry_ms, 0, "a shot spent the counter on tick {t}");
        let m = tape.at(1, t);
        assert!(m.hp == tape.at(1, tape.first - 1).hp && !m.buffed, "the Musketeer was answered on tick {t}");
    }

    // (b) Princess tower arrows land in full (measured: 15 x 109), the tower is never answered.
    let mut s = BattleState::new(0, cfg.clone());
    s.scenario_set_tick(T0);
    let t = s.arena().princess_tower_pos(Team::Blue, royalesim::arena::Lane::Left);
    let ronin = s.scenario_spawn_now(Team::Red, "Ronin", Vec2::new(t.x, t.y + 6000 * K), None).expect("place the Ronin");
    let tower = s.tower_ids(Team::Blue)[1].expect("the Blue engine-Left princess tower");
    let tape = Tape::record(&mut s, &[ronin, tower], 150);
    let arrows = tape.losses(0, tape.first, tape.last());
    assert!(arrows.len() >= 5, "only {} arrows landed, so this looked at nothing", arrows.len());
    assert!(arrows.iter().all(|l| l.1 == arrows[0].1 && l.1 > 0), "an arrow was not taken in full: {arrows:?}");
    let on_tower = tape.losses(1, tape.first, tape.last());
    assert!(!on_tower.is_empty(), "the Ronin never reached the tower, so the reach was not looked at");
    assert!(on_tower.iter().all(|l| l.1 == RONIN_HIT), "the tower lost something but the Ronin's hits: {on_tower:?}");
    for t in tape.first..=tape.last() {
        assert_eq!(tape.at(0, t).parry_ms, 0, "an arrow spent the counter on tick {t}");
        assert!(!tape.at(1, t).buffed, "the tower carries a buff on tick {t}");
    }

    // (c) A Zap lands and stuns, and the counter stays ready (measured: 192, stunned 10 ticks, no reset).
    let (mut s, ronin) = mortar_scene(cfg.clone());
    let pos = s.entity(ronin).unwrap().pos;
    s.spawn_unit(Team::Blue, "Zap", pos, None).expect("cast the Zap");
    let tape = Tape::record(&mut s, &[ronin], 20);
    let zapped = tape.losses(0, tape.first, tape.last());
    assert_eq!(zapped.iter().map(|l| l.1).collect::<Vec<_>>(), vec![ZAP], "the Zap, in full: {zapped:?}");
    assert!((tape.first..=tape.last()).any(|t| tape.at(0, t).stun_ms > 0), "the Zap stunned the Ronin");
    assert!((tape.first..=tape.last()).all(|t| tape.at(0, t).parry_ms == 0), "the Zap spent the counter");

    // (d) A Bat's hits land in full with the counter ready (measured: about 25, none countered). Air is out of
    // ground_melee.
    let bat_scene = |cfg: BattleConfig| {
        let (mut s, ronin) = mortar_scene(cfg);
        let bat = s.scenario_spawn_now(Team::Blue, "Bats", at((9000, 15600)), None).expect("place a Bat");
        let tape = Tape::record(&mut s, &[ronin, bat], 80);
        let fires = tape.fires(1, hit_speed(&s, "Bats"));
        (tape, fires)
    };
    // The shipped arm: every hit in full, then at least two of them. A countered hit reads as "not taken in full"
    // before the count is read.
    let (tape, fires) = bat_scene(cfg.clone());
    for f in &fires {
        assert_eq!(tape.lost(0, *f), BAT_HIT, "a Bat hit on tick {f} was not taken in full");
    }
    assert!(fires.len() >= 2, "the Bat hit only {} times, so this looked at nothing", fires.len());
    assert!((tape.first..=tape.last()).all(|t| tape.at(0, t).parry_ms == 0), "a Bat hit spent the counter");
    // parry.COUNTERED_HITS = melee: the first Bat hit is countered, and the reflect (162) kills the Bat 6 ticks later,
    // before its next hit (24 ticks). So this arm sees exactly one Bat hit, and the two the shipped arm needs cannot
    // be asked of it.
    let mut melee = cfg;
    melee.calib.parry_hits = ParryHits::Melee;
    let (tape, fires) = bat_scene(melee);
    let f = *fires.first().expect("the Bat never hit under melee, so this looked at nothing");
    assert_eq!(tape.lost(0, f), 0, "melee counters the Bat's hit");
    assert!(tape.get(1, f + REFLECT_AFTER - 1).is_some() && tape.get(1, f + REFLECT_AFTER).is_none(), "the reflect kills the Bat on its tick");
}

// ---------------------------------------------------------------------------
// (6) the reflect is not an attack

/// Plant: parry_never_counters.
#[test]
fn a_reflect_is_never_itself_countered() {
    // A Blue Ronin A hits a Red Ronin B that is still deploying (READY_AT = spawn: B's counter works). B counters,
    // and A takes twice its own hit 6 ticks later in full, though A's own counter is ready: the reflect is not
    // written by an attack. B cannot swing back before then: it deploys for 20 ticks.
    let mut s = BattleState::new(0, shipped());
    s.scenario_set_tick(T0);
    let a = s.scenario_spawn_now(Team::Blue, "Ronin", at((9500, 13000)), None).expect("place A");
    s.spawn_unit(Team::Red, "Ronin", at((9500, 14500)), None).expect("play B");
    s.tick();
    let b = find_live(&s, Team::Red, "Ronin").first().map(|e| e.id).expect("B is on the board");
    assert!(s.entity(b).unwrap().deploying, "precondition: B deploys");
    let tape = Tape::record(&mut s, &[a, b], 25);
    let f = tape.fires(0, hit_speed(&s, "Ronin"))[0];
    assert!(tape.at(1, f).deploying, "the scene drifted: B finished deploying before A's hit");
    assert_eq!(tape.lost(1, f), 0, "B countered A's hit");
    let hit = damage(&s, "Ronin", Team::Blue);
    assert_eq!(hit, RONIN_HIT);
    assert_eq!(tape.losses(0, f, f + REFLECT_AFTER), vec![(f + REFLECT_AFTER, 2 * hit)], "the reflect on A, in full");
    for t in f..=f + REFLECT_AFTER {
        assert_eq!(tape.at(0, t).parry_ms, 0, "A's counter was spent on tick {t}");
    }
    assert!(tape.at(1, f).parry_ms > 0, "B's counter is spent");
}

// ---------------------------------------------------------------------------
// (7) the pick

fn cand(k: usize, seq: u32, amount: i32) -> ParryCand {
    ParryCand { d: 0, k, attacker: EntityId { index: k as u32 + 10, generation: 1 }, seq, amount }
}

/// Plant: parry_pick_by_slot.
#[test]
fn parry_pick_orders_by_creation_then_amount() {
    // Written first, created last, a middle hit; created first, the smallest; the largest. Each arm picks its own,
    // and neither picks the one written first. Never measured (parry.SAME_TICK_PICK is a guess).
    let three = [cand(0, 5, 100), cand(1, 3, 50), cand(2, 4, 300)];
    assert_eq!(parry_pick(ParryPick::FirstCreatedAttacker, &three), 1);
    assert_eq!(parry_pick(ParryPick::LargestAmount, &three), 2);
    // Ties: one attacker's two hits go by the one written first; equal amounts by creation.
    let ties = [cand(3, 7, 300), cand(1, 3, 50), cand(2, 3, 50), cand(0, 9, 300)];
    assert_eq!(parry_pick(ParryPick::FirstCreatedAttacker, &ties), 1);
    assert_eq!(parry_pick(ParryPick::LargestAmount, &ties), 0);
}

/// Plants: parry_pick_by_slot (the largest_amount half: the slot order is the creation order here),
/// parry_never_counters.
#[test]
fn same_tick_hits_are_picked_by_the_arm() {
    // A Skeleton created first and a Knight created second, both in reach of the Ronin, both landing their first hit
    // on H (650 and 750 on their first tick, 1100 and 1200 on their 10th). The Ronin targets the nearer Knight.
    let run = |arm: ParryPick| {
        let mut cfg = shipped();
        cfg.calib.parry_pick = arm;
        let mut s = BattleState::new(0, cfg);
        s.scenario_set_tick(T0);
        let skel = s.scenario_spawn_now(Team::Red, "Skeletons", at((8000, 14500)), None).expect("the Skeleton, created first");
        let knight = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT_AT), None).expect("the Knight, created second");
        let ronin = s.scenario_spawn_now(Team::Blue, "Ronin", at(RONIN_AT), None).expect("the Ronin");
        let tape = Tape::record(&mut s, &[ronin, skel, knight], 20);
        let hs = |c: &str| hit_speed(&s, c);
        let (fs, fk) = (tape.fires(1, hs("Skeletons"))[0], tape.fires(2, hs("Knight"))[0]);
        assert_eq!(fs, fk, "the scene drifted: the two first hits are not on one tick");
        assert_eq!(tape.at(0, fk - 1).target, Some(knight), "the scene drifted: the Ronin is not on the Knight");
        (tape, fk)
    };
    // first_created_attacker: the Skeleton's hit is taken, the Knight's lands; the reflect (162) kills the Skeleton.
    let (tape, h) = run(ParryPick::FirstCreatedAttacker);
    assert_eq!(tape.lost(0, h), KNIGHT_HIT, "first_created_attacker counters the Skeleton");
    assert!(tape.get(1, h + REFLECT_AFTER - 1).is_some() && tape.get(1, h + REFLECT_AFTER).is_none(), "the Skeleton dies of the reflect");
    // largest_amount: the Knight's hit is taken, the Skeleton's lands; the Knight takes 404.
    let (tape, h) = run(ParryPick::LargestAmount);
    assert_eq!(tape.lost(0, h), SKELETON_HIT, "largest_amount counters the Knight");
    assert_eq!(tape.lost(2, h + REFLECT_AFTER), 2 * KNIGHT_HIT);
}

/// Plants: parry_pick_by_slot (both arms), parry_seq_from_slot (first_created_attacker).
#[test]
fn same_tick_hits_are_picked_by_creation_not_by_slot() {
    // Three attackers whose first hits land on one tick, in slots that disagree with their creation order. Three
    // filler Knights take three fresh slots, low to high, and die one a tick: low, then high, then middle. The free
    // list hands slots back last freed first, so Skeleton A (created first) takes the middle slot, the Knight
    // (created second) the high one and Skeleton B (created third) the low one. The Attack pass writes hits in slot
    // order, so B's hit is written first, A's second and the Knight's last, and the Knight's is the largest.
    // first_created_attacker takes A's hit, largest_amount the Knight's, and a pick by slot B's.
    const A: usize = 1;
    const KN: usize = 2;
    const B: usize = 3;
    let run = |arm: ParryPick| {
        let mut cfg = shipped();
        cfg.calib.parry_pick = arm;
        let mut s = BattleState::new(0, cfg);
        s.scenario_set_tick(T0);
        let fillers: Vec<EntityId> =
            [(3000, 5000), (4500, 5000), (6000, 5000)].iter().map(|p| s.scenario_spawn_now(Team::Blue, "Knight", at(*p), None).expect("a filler Knight")).collect();
        let slots: Vec<u32> = fillers.iter().map(|f| f.index).collect();
        assert!(slots[0] < slots[1] && slots[1] < slots[2], "precondition: the fillers take rising slots: {slots:?}");
        for k in [0, 2, 1] {
            assert!(s.debug_set_hp(fillers[k], 0));
            s.tick();
            assert!(s.entity(fillers[k]).is_none(), "precondition: filler {k} died and freed its slot");
        }
        let skel_a = s.scenario_spawn_now(Team::Red, "Skeletons", at((8000, 14500)), None).expect("Skeleton A, created first");
        let knight = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT_AT), None).expect("the Knight, created second");
        let skel_b = s.scenario_spawn_now(Team::Red, "Skeletons", at((10000, 14500)), None).expect("Skeleton B, created third");
        assert_eq!([skel_b.index, skel_a.index, knight.index], [slots[0], slots[1], slots[2]], "precondition: the slots run B, A, the Knight");
        let ronin = s.scenario_spawn_now(Team::Blue, "Ronin", at(RONIN_AT), None).expect("the Ronin");
        let tape = Tape::record(&mut s, &[ronin, skel_a, knight, skel_b], 20);
        let hs = |c: &str| hit_speed(&s, c);
        let h = tape.fires(KN, hs("Knight"))[0];
        for k in [A, B] {
            assert_eq!(tape.fires(k, hs("Skeletons")).first(), Some(&h), "the scene drifted: a Skeleton's first hit is not on the Knight's tick");
        }
        assert_eq!(tape.at(0, h - 1).target, Some(knight), "the scene drifted: the Ronin is not on the Knight");
        (tape, h)
    };
    // first_created_attacker: A's hit is taken, B's and the Knight's land, and the reflect (162) kills A alone.
    let (tape, h) = run(ParryPick::FirstCreatedAttacker);
    assert!(tape.get(A, h + REFLECT_AFTER - 1).is_some() && tape.get(A, h + REFLECT_AFTER).is_none(), "Skeleton A, created first, dies of the reflect");
    assert!(tape.get(B, h + REFLECT_AFTER).is_some(), "Skeleton B, in the first slot, took no reflect");
    assert_eq!(tape.lost(KN, h + REFLECT_AFTER), 0, "the Knight took no reflect");
    assert_eq!(tape.lost(0, h), KNIGHT_HIT + SKELETON_HIT, "first_created_attacker counters Skeleton A alone");
    // largest_amount: the Knight's hit is taken, both Skeletons' land, and the Knight takes 404.
    let (tape, h) = run(ParryPick::LargestAmount);
    assert_eq!(tape.lost(0, h), 2 * SKELETON_HIT, "largest_amount counters the Knight, written last");
    assert_eq!(tape.lost(KN, h + REFLECT_AFTER), 2 * KNIGHT_HIT);
    assert!(tape.get(A, h + REFLECT_AFTER).is_some() && tape.get(B, h + REFLECT_AFTER).is_some(), "no Skeleton took the reflect");
}

// ---------------------------------------------------------------------------
// (8), (9) the attacker's death and the Ronin's

/// Plant: parry_never_counters.
#[test]
fn a_counter_whose_attacker_dies_first_answers_nothing() {
    let (mut s, ids, tape, h) = duel_tape(shipped(), 12);
    assert_eq!(tape.last(), h + 2, "precondition: the tape ends two ticks after the counter");
    let pending: Vec<ScheduledAction> = s.scheduled().iter().map(|e| e.action).collect();
    assert_eq!(pending, vec![ScheduledAction::Damage { target: ids[KNIGHT], amount: 2 * KNIGHT_HIT, crown_pct: 100 }], "the reflect waits");
    // The Knight dies on H + 3, before its reflect is due.
    assert!(s.debug_set_hp(ids[KNIGHT], 0));
    s.tick();
    assert!(s.entity(ids[KNIGHT]).is_none(), "the Knight died on H + 3");
    for t in h + 4..=h + 8 {
        let before: Vec<(EntityId, i32)> = s.entities().map(|e| (e.id, e.hp)).collect();
        s.tick();
        for (id, hp) in before {
            if let Some(e) = s.entity(id) {
                assert_eq!(e.hp, hp, "{} lost hp on tick {t}, with nothing hitting it", e.card);
            }
        }
    }
    assert!(s.scheduled().is_empty(), "the reflect is dropped with its target");
}

/// Plant: parry_never_counters.
#[test]
fn a_countered_hit_leaves_the_ronin_standing() {
    let (mut s, ids) = duel_with(shipped(), Some(150));
    let tape = Tape::record(&mut s, &ids, 12);
    let h = tape.fires(KNIGHT, hit_speed(&s, "Knight"))[0];
    assert!(damage(&s, "Knight", Team::Red) > 150, "the hit would kill it");
    assert_eq!(tape.get(RONIN, h).map(|r| r.hp), Some(150), "the Ronin stands at 150 after the countered hit");
}

// ---------------------------------------------------------------------------
// (10) the cooldown

/// Plant: parry_never_counters.
#[test]
fn the_next_counter_is_ready_seventy_ticks_after_the_counter() {
    // The duel, a second Knight placed after the first tick, whose first hit lands on H + 1, and a Skeleton placed
    // after H + 60, whose first hit lands on H + 70 and which the Ronin is not targeting. Two tapes: ticks 201 .. 269
    // (the second Knight's hits on H + 1, 25 and 49, the first Knight's on H + 34 and 58) and 270 .. 289.
    let scene = |cfg: BattleConfig| {
        let (mut s, ids) = duel_with(cfg, None);
        s.tick();
        let knight2 = s.scenario_spawn_now(Team::Red, "Knight", at((10200, 14700)), None).expect("the second Knight");
        let head = Tape::record(&mut s, &[ids[RONIN], ids[KNIGHT], knight2], 69);
        let h = head.fires(KNIGHT, hit_speed(&s, "Knight"))[0];
        assert_eq!(s.tick_count(), h + 61, "the scene drifted: the first Knight's hit is not on tick 209");
        let skel = s.scenario_spawn_now(Team::Red, "Skeletons", at((8000, 14500)), None).expect("the Skeleton");
        let tail = Tape::record(&mut s, &[ids[RONIN], ids[KNIGHT], knight2, skel], 20);
        (s, head, tail, h, ids[KNIGHT])
    };
    let (s, head, tail, h, knight) = scene(shipped());
    let dt = s.config().calib.tick_ms;
    let p = card_stat(&s, "Ronin").parry.expect(REGENERATE);
    assert_eq!(p.cooldown_ms / dt, READY_AFTER as i32, "Cooldown 3500 is 70 ticks");
    // Every hit from H + 1 to H + 60 lands in full (measured: a hit on H + 1, and hits 34 and 58 ticks after).
    let full = vec![(h + 1, KNIGHT_HIT), (h + 25, KNIGHT_HIT), (h + 34, KNIGHT_HIT), (h + 49, KNIGHT_HIT), (h + 58, KNIGHT_HIT)];
    assert_eq!(head.losses(RONIN, h + 1, h + 60), full, "the hits while the counter is spent");
    assert_eq!(head.at(RONIN, h).parry_ms, p.cooldown_ms - dt, "the cooldown, first counted down on H");
    // Ready on H + 70: 50 ms left after H + 68, none after H + 69, and nothing lands between.
    assert_eq!(tail.at(RONIN, h + 68).parry_ms, dt);
    assert_eq!(tail.at(RONIN, h + 69).parry_ms, 0);
    assert_eq!(tail.losses(RONIN, h + 61, h + 69), vec![], "no hit between H + 61 and H + 69");
    let f = tail.fires(3, hit_speed(&s, "Skeletons"))[0];
    assert_eq!(f, h + READY_AFTER, "the scene drifted: the Skeleton's first hit is not on H + 70");
    // The Skeleton's hit is countered though the Ronin targets the first Knight, and its reflect (162) kills it.
    assert_eq!(tail.lost(RONIN, f), 0, "the Skeleton's hit on H + 70 is countered");
    assert_eq!(tail.at(RONIN, f).target, Some(knight), "the Ronin's target is the first Knight, not the Skeleton");
    assert!(tail.get(3, f + REFLECT_AFTER - 1).is_some() && tail.get(3, f + REFLECT_AFTER).is_none(), "the reflect kills the Skeleton on H + 76");
    // The second Knight's hit on H + 73 lands in full: the counter is spent again.
    assert_eq!(tail.lost(RONIN, h + 73), KNIGHT_HIT, "the counter is spent again on H + 73");

    // parry.COOLDOWN_START = lock_end: counted from the end of the 500 ms forced animation, 80 ticks, so the
    // Skeleton's hit on H + 70 lands in full.
    let mut cfg = shipped();
    cfg.calib.parry_cooldown_start = ParryCooldownStart::LockEnd;
    let (_, head, tail, h, _) = scene(cfg);
    assert_eq!(head.at(RONIN, h).parry_ms, p.cooldown_ms + p.self_lock_ms - dt, "lock_end: 80 ticks");
    assert_eq!(tail.lost(RONIN, h + READY_AFTER), SKELETON_HIT, "lock_end: the hit on H + 70 lands in full");
}

// ---------------------------------------------------------------------------
// (11) while deploying

/// Plant: parry_never_counters.
#[test]
fn the_counter_is_ready_while_deploying() {
    // A Red Knight in place and a Blue Ronin played beside it: the Knight's first hit lands while the Ronin deploys.
    // DeployActive is true on the row; no measured hit landed on a deploying Ronin (parry.READY_AT is a guess).
    let run = |arm: ParryReadyAt| {
        let mut cfg = shipped();
        cfg.calib.parry_ready_at = arm;
        let mut s = BattleState::new(0, cfg);
        s.scenario_set_tick(T0);
        let knight = s.scenario_spawn_now(Team::Red, "Knight", at((9500, 14700)), None).expect("place the Knight");
        s.spawn_unit(Team::Blue, "Ronin", at((9500, 13500)), None).expect("play the Ronin");
        s.tick();
        let ronin = find_live(&s, Team::Blue, "Ronin").first().map(|e| e.id).expect("the Ronin is on the board");
        let tape = Tape::record(&mut s, &[ronin, knight], 20);
        let f = tape.fires(1, hit_speed(&s, "Knight"))[0];
        assert!(tape.at(0, f).deploying, "the scene drifted: the Ronin finished deploying before the Knight's hit");
        (tape, f)
    };
    let (tape, f) = run(ParryReadyAt::Spawn);
    assert_eq!(tape.lost(0, f), 0, "spawn: the deploying Ronin counters");
    assert_eq!(tape.losses(1, f, f + 10), vec![(f + REFLECT_AFTER, 2 * KNIGHT_HIT)]);
    let (tape, f) = run(ParryReadyAt::DeployEnd);
    assert_eq!(tape.lost(0, f), KNIGHT_HIT, "deploy_end: the hit lands in full");
    assert_eq!(tape.losses(1, f, f + 10), vec![], "deploy_end: no reflect");
}

// ---------------------------------------------------------------------------
// (12), (13) state

/// Plants: scheduled_dropped_on_save, parry_never_counters.
#[test]
fn a_reflect_in_flight_survives_a_save() {
    let (mut s, ids, tape, h) = duel_tape(shipped(), 13);
    assert_eq!(tape.last(), h + 3, "precondition: saved three ticks after the counter");
    let mut l = BattleState::load(&s.save()).expect("the save loads");
    assert_eq!(l.state_hash(), s.state_hash());
    for k in 1..=5 {
        let before = l.entity(ids[KNIGHT]).unwrap().hp;
        s.tick();
        l.tick();
        assert_eq!(l.state_hash(), s.state_hash(), "diverged {k} ticks after the load");
        let lost = before - l.entity(ids[KNIGHT]).unwrap().hp;
        let want = if h + 3 + k == h + REFLECT_AFTER { 2 * KNIGHT_HIT } else { 0 };
        assert_eq!(lost, want, "the loaded Knight on H + {}", 3 + k);
    }
}

/// Plants: hash_skips_parry, hash_skips_scheduled (tests/transform.rs's), parry_never_counters.
#[test]
fn the_counter_cooldown_is_state() {
    let (s, ids, tape, h) = duel_tape(shipped(), 11);
    assert_eq!(tape.last(), h + 1);
    let (ri, ki) = (ids[RONIN].index as usize, ids[KNIGHT].index as usize);
    assert!(tape.at(RONIN, h + 1).parry_ms > 0, "precondition: the counter is spent");
    let hashed = edit_is_hashed(&s, |v| {
        let ms = v["ents"]["parry_ms"][ri].as_i64().expect("the snapshot carries the cooldown");
        v["ents"]["parry_ms"][ri] = serde_json::Value::from(ms + 50);
    });
    assert!(hashed, "a save edited only in the Ronin's cooldown loads under the old hash: it is not hashed");
    let hashed = edit_is_hashed(&s, |v| {
        let a = &mut v["scheduled"][0]["action"]["Damage"]["amount"];
        let x = a.as_i64().expect("the snapshot carries the reflect in flight");
        *a = serde_json::Value::from(x + 1);
    });
    assert!(hashed, "a save edited only in the reflect in flight loads under the old hash: it is not hashed");
    // The column is hashed only on a card that carries a counter, so a battle without one hashes as before it.
    let hashed = edit_is_hashed(&s, |v| v["ents"]["parry_ms"][ki] = serde_json::Value::from(50));
    assert!(!hashed, "the Knight's cooldown column is hashed, so every battle without a Ronin moved");
}

// ---------------------------------------------------------------------------
// (14) the split row

/// Slower: a melee troop whose hit hangs a row of speed -100, hit speed -95 and spawn speed -100 for 500 ms (the
/// Ronin's stun row's columns, on BuffOnDamage). Dummy: a melee troop it fights.
const SPLIT: &str = r#"{ "version": "test", "cards": [
 { "name":"Slower", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":5000, "damage":10, "hit_speed_ms":1000,
   "load_time_ms":500, "speed":60, "range_milli":1200, "sight_range_milli":5500, "collision_radius_milli":500, "mass":5,
   "deploy_time_ms":1000,
   "buff_on_damage":{"buff":{"name":"SplitStop","speed_multiplier_raw":-100,"hit_speed_multiplier_raw":-95,"spawn_speed_multiplier_raw":-100},"time_ms":500} },
 { "name":"Dummy", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":5000, "damage":10, "hit_speed_ms":1200,
   "load_time_ms":700, "speed":60, "range_milli":1200, "sight_range_milli":5500, "collision_radius_milli":500, "mass":5,
   "deploy_time_ms":1000 }
], "units": {} }"#;

/// Plant: split_stop_is_stun.
#[test]
fn a_row_that_stops_the_walk_but_not_the_clock_is_not_a_stun() {
    let run = |arm: FullStopBuff| {
        let mut cfg = BattleConfig::with_cards(CardDb::from_json_str(SPLIT, CardSource::DerivedJson).expect("the synthetic file parses"));
        cfg.card_level = [1, 1];
        cfg.calib.full_stop_buff_is_stun = arm;
        let mut s = BattleState::new(0, cfg);
        s.scenario_set_tick(T0);
        let ids = s
            .scenario_spawn_batch(&[(Team::Blue, "Slower", at(RONIN_AT), None), (Team::Red, "Dummy", at(KNIGHT_AT), None)])
            .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
        let tape = Tape::record(&mut s, &ids, 25);
        let f = (tape.first..=tape.last()).find(|t| tape.at(1, *t).buffed).expect("the Slower's hit never hung its row");
        (s, ids, tape, f)
    };
    let (s, ids, tape, f) = run(FullStopBuff::SpeedAndHitSpeedZero);
    let dt = s.config().calib.tick_ms;
    let step = compose([s.cards().buffs[s.cards().buffs.iter().position(|b| b.hit_speed_pct == -95).expect("the row")]].iter(), Sel::HitSpeed, dt);
    for t in f + 1..=f + 9 {
        let (a, b) = (tape.at(1, t - 1), tape.at(1, t));
        assert_eq!(b.stun_ms, 0, "the row drove the hold timer on tick {t}");
        assert_eq!(b.attack_ms - a.attack_ms, step, "the Dummy's clock on tick {t}");
        assert_eq!(b.target, Some(ids[0]), "the Dummy dropped its target on tick {t}");
    }
    // stun_timer: the same row is a stun.
    let (_, _, tape, f) = run(FullStopBuff::StunTimer);
    assert!(tape.at(1, f).stun_ms > 0, "stun_timer: the row drives the hold timer");
    assert_eq!(tape.at(1, f + 1).attack_ms, tape.at(1, f).attack_ms, "stun_timer: the clock is held");

    // A Zap on a Knight stuns it for its 500 ms under both arms (the -100 / -100 / -100 row).
    for arm in [FullStopBuff::SpeedAndHitSpeedZero, FullStopBuff::StunTimer] {
        let mut cfg = shipped();
        cfg.calib.full_stop_buff_is_stun = arm;
        let mut s = BattleState::new(0, cfg);
        s.scenario_set_tick(T0);
        let knight = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT_AT), None).expect("place the Knight");
        s.spawn_unit(Team::Blue, "Zap", at(KNIGHT_AT), None).expect("cast the Zap");
        let tape = Tape::record(&mut s, &[knight], 10);
        let most = (tape.first..=tape.last()).map(|t| tape.at(0, t).stun_ms).max().unwrap();
        assert_eq!(most, 500, "{arm:?}: the Zap's stun");
    }

    // The arms part on the Ronin's row alone: every other row the table loads whose speed composes to 0 has its hit
    // speed at 0 too, so no card loaded before the Ronin moves.
    let s = BattleState::new(0, shipped());
    let db = s.cards();
    let split: Vec<&str> = db
        .buffs
        .iter()
        .zip(&db.buff_names)
        .filter(|(b, _)| compose([**b].iter(), Sel::Speed, 100) == 0 && compose([**b].iter(), Sel::HitSpeed, 100) != 0)
        .map(|(_, n)| n.as_str())
        .collect();
    assert_eq!(split, ["ronin_reflect_stun_buff"], "the rows that stop the walk and not the clock");
}

// ---------------------------------------------------------------------------
// (15) the loader

#[test]
fn the_loader_reads_the_ronins_counter() {
    let s = BattleState::new(0, shipped());
    let p = card_stat(&s, "Ronin").parry.expect(REGENERATE);
    assert_eq!((p.cooldown_ms, p.ready_at_deploy, p.taken_pct, p.reflect_pct, p.self_lock_ms), (3500, true, 0, 200, 500));
    assert_eq!((p.delays_ms, p.group_len, p.stun_at, p.reflect_at), ([50, 300, 150, 0], 3, 0, 1));
    assert_eq!(p.stun.time_ms, 500);
    let b = s.cards().buffs[p.stun.buff as usize];
    assert_eq!((b.speed_pct, b.hit_speed_pct, b.spawn_speed_pct), (-100, -95, -100), "the stun row");
    let carriers: Vec<&str> = s.cards().cards.iter().filter(|c| c.parry.is_some()).map(|c| c.name.as_str()).collect();
    assert_eq!(carriers, ["Ronin"], "the 15.535.29 table sets an ActionCounter on one row");
}

/// Parrier: the Ronin's counter block on a synthetic row, with the Ronin's graph.
const PARRIER: &str = r#"{ "version": "test", "cards": [
 { "name":"Parrier", "kind":"troop", "elixir":5, "rarity":"Common", "hitpoints":1000, "damage":100, "hit_speed_ms":1400,
   "load_time_ms":1000, "speed":90, "range_milli":1200, "sight_range_milli":5500, "collision_radius_milli":600, "mass":6,
   "deploy_time_ms":1000,
   "action_graph":{"roots":{"OnStartingAction":"group"},"class_types":["ActionCounter","ActionDealDamage","ActionGroup","ActionPlayEffect","ActionRunForcedAnimationOnce","ActionSpawn","ActionWithDuration"],"spawns":["BuffType:Slow"],"mechanic":true},
   "parry":{"counter_cooldown_ms":3500,"deploy_active":true,"damage_scalar_pct":200,"defense_scalar_pct":0,"root_delays_ms":[0,0],"counter_at":0,"self_delays_ms":[0,0],"self_forced_ms":500,"self_tag_ms":3500,"instigator_delays_ms":[50,300,150],"stun_at":0,"reflect_at":1,"stun":{"name":"Slow","speed_multiplier_raw":-100,"hit_speed_multiplier_raw":-95,"spawn_speed_multiplier_raw":-100},"stun_time_ms":500,"reflect_level_scaling":false} }
], "units": {
 "Imp": { "name":"Imp", "rarity":"Common", "hitpoints":80, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 }
} }"#;

/// Plant: parry_shape_unchecked (the first two refusals).
#[test]
fn a_counter_graph_of_another_shape_is_refused() {
    let why = |json: &str| -> Option<String> {
        let db = CardDb::from_json_str(json, CardSource::DerivedJson).expect("the synthetic file parses");
        match db.index("Parrier") {
            Some(i) => {
                assert!(db.get(i).parry.is_some(), "loaded without its counter");
                None
            }
            None => Some(db.rejected.iter().find(|(n, _)| n == "Parrier").map(|(_, w)| w.clone()).expect("Parrier is listed as rejected")),
        }
    };
    assert_eq!(why(PARRIER), None, "the control loads");
    let today = "the unit runs an action graph this loader does not read (";
    // One more class in the graph.
    let extra = PARRIER.replace(r#""ActionGroup","ActionPlayEffect""#, r#""ActionGroup","ActionHeal","ActionPlayEffect""#);
    let got = why(&extra).expect("a graph with one more class is refused");
    assert!(got.starts_with(today) && got.contains("ActionHeal"), "{got}");
    // A second spawn.
    let spawn = PARRIER.replace(r#""spawns":["BuffType:Slow"]"#, r#""spawns":["BuffType:Slow","BuffType:Other"]"#);
    let got = why(&spawn).expect("a second spawn is refused");
    assert!(got.starts_with(today) && got.contains("BuffType:Other"), "{got}");
    // A level-scaled reflect.
    let scaled = PARRIER.replace(r#""reflect_level_scaling":false"#, r#""reflect_level_scaling":true"#);
    assert_eq!(why(&scaled).as_deref(), Some("the unit's counter: a reflect whose level scaling is Some(true) has no reading here; not simulated"));
    // A cooldown tag that is not the Cooldown.
    let tag = PARRIER.replace(r#""self_tag_ms":3500"#, r#""self_tag_ms":3000"#);
    assert_eq!(why(&tag).as_deref(), Some("the unit's counter: the cooldown tag lasts Some(3000) and the Cooldown is 3500; not simulated"));
    // A life-state controller and a counter on one row.
    let two = PARRIER.replace(
        r#""parry":"#,
        r#""life_state_spawner":{"action_delay_ms":1000,"spawn_interval_ms":2200,"character":"Imp","number":1,"offset_milli":1200,"offset_angle_deg":20,"object_filter":"DefaultCharacterTargets"}, "parry":"#,
    );
    assert_eq!(why(&two).as_deref(), Some("the unit carries more than one action block (a life-state controller and a counter); not simulated"));
}
