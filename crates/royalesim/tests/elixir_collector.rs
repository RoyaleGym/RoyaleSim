//! THE ELIXIR ECONOMY (card.rs `ManaDef`; state.rs `mana_pass`, `phase_reap`, `try_new`; calibration economy.*).
//!
//! THE LAW, measured on client 16.402 (capture 20260920-090204: two Elixir Collectors at level 11, 4 payouts, one
//! held at the cap, one capped):
//!   - the Collector deploys for 19 frames and its payout timer starts at the deploy end D: the first payout (one
//!     elixir, to its owner alone) is on D + 259, the next 260 ticks later;
//!   - a payout due while the owner holds 10 elixir is held and paid on the first tick below 10, with that tick's
//!     spend; the next is 259 ticks after the release (economy.PRODUCTION_AT_CAP);
//!   - a payout that would pass 10 fills to 10 (economy.PRODUCTION_OVERFLOW).
//!
//! Read from the 15.535.29 tables and not measured: ManaOnDeath on any death (economy.MANA_ON_DEATH_TRIGGER), a
//! stun holding the timer (economy.STUN_PAUSES_PRODUCTION), the interval in double elixir
//! (economy.PRODUCTION_RATE_IN_DOUBLE_ELIXIR), and the deal keeping OmitFromStartingHand cards out of the starting
//! hand (economy.OMIT_FROM_STARTING_HAND).
//!
//! HOW A PAYOUT IS READ: two battles, one with the Collector and one without, the owner's elixir set to the same
//! value before every tick; a payout is the difference after the tick, so the regen cancels.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the Collector loads as an inert building with its elixir columns and the omit flag, and never targets;
//!   2. the first payout on D + 259 (D = its first frame + 19), the next on D + 519, one elixir each;
//!   3. a payout due at 10 elixir is held and paid on the tick of the first spend;
//!   4. the next payout after that release comes 259 ticks later;
//!   5. an overflowing payout fills to 10 exactly;
//!   6. the opponent's elixir never moves with the Collector's;
//!   7. its death pays its owner one elixir, drained out or destroyed;
//!   8. a stun holds the timer under STUN_PAUSES_PRODUCTION = true and not under false;
//!   9. double elixir leaves the interval alone under fixed_interval and halves it under scaled_with_elixir_rate;
//!  10. the blank-hit-speed exemption needs a `mana` block on an inert building, and a payout on a troop or a half
//!      block is refused;
//!  11. the payout timer is state: two saves differing only in it hash differently;
//!  12. THE DEAL: an OmitFromStartingHand card is never in a starting hand (1000 seeds, shuffled);
//!  13. a deck without one deals exactly as under not_modelled (the rule reaches no other deck);
//!  14. unshuffled, the omitted card swaps with the first card behind the hand.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test elixir_collector`):
//!   * `mana_wasted_at_cap` -- a payout due at the cap is paid into the cap and wasted, the timer running on: (3) red.
//!   * `held_reload_next_tick` -- a held payout's timer restarts whole: (4) red on release + 260; (2) stays green,
//!     since a regular payout falls due on exactly 0.
//!   * `hash_skips_mana_timer` -- the timer is not hashed: (11) red.
//!   * `omit_ignored` -- the deal does not read the column: (12) and (14) red, (13) green.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardKind, CardSource, ManaDef};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, OmitRule, ProductionRate};
use royalesim::{EntityId, Team};

const COLLECTOR: &str = "Elixir Collector";

/// A blue building mid-way down its own half, native units, out of every tower's reach.
const AT: (i32, i32) = (9000, 8000);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Level 11, the level every figure here was measured at.
fn cfg() -> BattleConfig {
    let mut c = config();
    c.card_level = [11, 11];
    c.tower_level = [11, 11];
    c
}

/// One elixir in the engine's raw units.
fn one_elixir(s: &BattleState) -> i64 {
    s.elixir_raw(Team::Blue).1
}

/// What `paired` saw, per tick k (0 = the tick the Collector appears).
struct Paired {
    /// Blue's raw elixir with the Collector minus without.
    blue: Vec<i64>,
    /// The same for Red.
    red: Vec<i64>,
    /// The Collector's id while it stands.
    ids: Vec<Option<EntityId>>,
    /// One elixir, raw.
    unit: i64,
}

/// Two battles from `c`, one with a blue Collector played at AT and one without. `blue_milli(k)` sets Blue's elixir
/// before tick k when Some; `before(k, with)` may act on the battle with the Collector before tick k.
fn paired(c: BattleConfig, ticks: u32, blue_milli: impl Fn(u32) -> Option<i64>, mut before: impl FnMut(u32, &mut BattleState)) -> Paired {
    let mut with = BattleState::new(0, c.clone());
    let mut without = BattleState::new(0, c);
    with.spawn_unit(Team::Blue, COLLECTOR, at(AT), None).expect("play the Collector");
    let unit = one_elixir(&with);
    let (mut blue, mut red, mut ids) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..ticks {
        if let Some(m) = blue_milli(k) {
            with.scenario_set_elixir_milli(Team::Blue, m);
            without.scenario_set_elixir_milli(Team::Blue, m);
        }
        before(k, &mut with);
        with.tick();
        without.tick();
        blue.push(with.elixir_raw(Team::Blue).0 - without.elixir_raw(Team::Blue).0);
        red.push(with.elixir_raw(Team::Red).0 - without.elixir_raw(Team::Red).0);
        ids.push(with.entities().find(|v| v.card == COLLECTOR).map(|v| v.id));
    }
    Paired { blue, red, ids, unit }
}

/// The ticks on which Blue gained over the battle without the Collector, with the amount.
fn payouts(blue: &[i64]) -> Vec<(u32, i64)> {
    blue.iter().enumerate().filter(|(_, d)| **d != 0).map(|(k, d)| (k as u32, *d)).collect()
}

/// 1. The Collector's row, read.
#[test]
fn the_collector_loads_as_an_inert_building_that_never_targets() {
    let mut s = BattleState::new(0, cfg());
    let c = card_stat(&s, COLLECTOR).clone();
    assert_eq!(c.kind, CardKind::Building);
    assert_eq!((c.hit_speed_ms, c.damage, c.range), (0, 0, 0), "an inert building: no hit speed, no damage, no range");
    assert_eq!(c.mana, Some(ManaDef { collect: Some((1, 13000)), on_death: 1, on_death_for_opponent: 0 }), "the 15.535.29 elixir columns");
    assert!(c.omit_from_starting_hand, "OmitFromStartingHand");
    assert_eq!(c.lifetime_ms, Some(93000));
    s.spawn_unit(Team::Blue, COLLECTOR, at(AT), None).expect("play the Collector");
    s.scenario_spawn_now(Team::Red, "Knight", at((AT.0, AT.1 + 2500)), None).expect("an enemy beside it");
    for _ in 0..60 {
        s.tick();
        let v = s.entities().find(|v| v.card == COLLECTOR).expect("the Collector stands");
        assert!(v.target.is_none(), "the Collector took a target");
        assert_eq!(v.max_hp, 1070, "418 at level 11 on the Common ladder (x256 %): 1070, as measured");
    }
}

/// 2. The first payout on D + 259, the next 260 ticks later. The owner is held at 2 elixir, far from the cap.
#[test]
fn first_payout_259_ticks_after_the_deploy_end_then_every_260() {
    let Paired { blue, ids, unit, .. } = paired(cfg(), 560, |_| Some(2000), |_, _| {});
    assert!(ids[0].is_some(), "the Collector is on the board on its first tick");
    let mut s = BattleState::new(0, cfg());
    s.spawn_unit(Team::Blue, COLLECTOR, at(AT), None).unwrap();
    let mut deploying = 0;
    for _ in 0..40 {
        s.tick();
        if s.entities().find(|v| v.card == COLLECTOR).is_some_and(|v| v.deploying) {
            deploying += 1;
        }
    }
    assert_eq!(deploying, 19, "19 deploying frames, so D = the first frame + 19");
    assert_eq!(payouts(&blue), vec![(19 + 259, unit), (19 + 519, unit)], "payouts (tick after the first frame, raw elixir)");
}

/// 3 and 4. A payout due while the owner holds 10 elixir waits for the first spend.
fn held(release: u32) -> Vec<(u32, i64)> {
    let Paired { blue, .. } = paired(cfg(), release + 270, move |k| Some(if k < release { 10_000 } else if k == release { 7_000 } else { 2_000 }), |_, _| {});
    payouts(&blue)
}

/// 3. Plant: mana_wasted_at_cap.
#[test]
fn a_payout_due_at_ten_elixir_is_held_and_paid_with_the_first_spend() {
    // Due on 278 with the owner at 10 through 290; the spend comes before tick 291 (measured: due 765, at 10 through
    // 778, paid on 779 beside a 3-elixir play).
    let release = 19 + 259 + 13;
    let got = held(release);
    let one = BattleState::new(0, cfg()).elixir_raw(Team::Blue).1;
    assert_eq!(got.first(), Some(&(release, one)), "the held payout: one elixir on the release tick, nothing on its due tick 278: {got:?}");
}

/// 4. Plant: held_reload_next_tick.
#[test]
fn the_next_payout_is_259_ticks_after_the_release() {
    let release = 19 + 259 + 13;
    let got = held(release);
    assert_eq!(got.get(1).map(|p| p.0), Some(release + 259), "the payout after a release (measured 779 -> 1038): {got:?}");
}

/// 5. An overflowing payout fills to 10.
#[test]
fn an_overflowing_payout_fills_to_ten() {
    let due = 19 + 259;
    let mut s = BattleState::new(0, cfg());
    s.spawn_unit(Team::Blue, COLLECTOR, at(AT), None).unwrap();
    for k in 0..=due {
        s.scenario_set_elixir_milli(Team::Blue, if k == due { 9_582 } else { 2_000 });
        s.tick();
    }
    let (raw, unit) = s.elixir_raw(Team::Blue);
    let cap = s.config().calib.max_mana as i64 * unit;
    assert_eq!(raw, cap, "9.582 elixir and a payout: exactly 10 (measured 95828 -> 100000)");
}

/// 6. The opponent is untouched.
#[test]
fn the_opponent_gains_nothing() {
    let Paired { blue, red, .. } = paired(cfg(), 560, |_| Some(2000), |_, _| {});
    assert_eq!(payouts(&blue).len(), 2, "the scene paid nothing to compare against");
    assert!(red.iter().all(|d| *d == 0), "Red's elixir moved with Blue's Collector");
}

/// 7. ManaOnDeath pays the owner on any death (economy.MANA_ON_DEATH_TRIGGER = any_death).
#[test]
fn its_death_pays_its_owner_one_elixir_drained_out_or_destroyed() {
    for destroyed in [false, true] {
        let mut with = BattleState::new(0, cfg());
        let mut without = BattleState::new(0, cfg());
        // At 1 hp the lifetime drain (57 hundredths a tick at level 11) takes it out on its second drain tick; at 150
        // the drain needs over 260 ticks and a red Zap (192 at level 11) does it first.
        let hp = if destroyed { 150 } else { 1 };
        let id = with.scenario_spawn_now(Team::Blue, COLLECTOR, at(AT), Some(hp)).expect("the Collector");
        if destroyed {
            with.spawn_unit(Team::Red, "Zap", at(AT), None).expect("the Zap");
            without.spawn_unit(Team::Red, "Zap", at(AT), None).expect("the Zap");
        }
        let unit = one_elixir(&with);
        let mut died = None;
        for k in 0..60u32 {
            with.scenario_set_elixir_milli(Team::Blue, 2000);
            without.scenario_set_elixir_milli(Team::Blue, 2000);
            with.tick();
            without.tick();
            if died.is_none() && with.entity(id).is_none() {
                died = Some(k);
                let gain = with.elixir_raw(Team::Blue).0 - without.elixir_raw(Team::Blue).0;
                assert_eq!(gain, unit, "destroyed {destroyed}: the owner's elixir on the death tick, over the battle without it");
                assert_eq!(with.elixir_raw(Team::Red).0, without.elixir_raw(Team::Red).0, "destroyed {destroyed}: the opponent gained");
            }
        }
        assert!(died.is_some(), "destroyed {destroyed}: the Collector never died, so nothing was tested");
    }
}

/// 8. economy.STUN_PAUSES_PRODUCTION: a red Freeze on the Collector mid-period.
#[test]
fn a_stunned_collector_holds_its_timer_under_true_and_not_under_false() {
    let first = |pauses: bool, freeze: bool| -> u32 {
        let mut c = cfg();
        c.calib.stun_pauses_production = pauses;
        let Paired { blue, .. } = paired(c, 400, |_| Some(2000), |k, s| {
            if freeze && k == 100 {
                s.spawn_unit(Team::Red, "Freeze", at(AT), None).expect("the Freeze");
            }
        });
        payouts(&blue).first().map(|p| p.0).expect("a payout")
    };
    let free = first(true, false);
    assert_eq!(free, 19 + 259);
    assert!(first(true, true) > free, "a frozen Collector paid on time under true");
    assert_eq!(first(false, true), free, "a frozen Collector was delayed under false");
}

/// 9. economy.PRODUCTION_RATE_IN_DOUBLE_ELIXIR, inside the last 60 s of regulation.
#[test]
fn double_elixir_leaves_the_interval_alone_under_fixed_interval() {
    let first = |rate: ProductionRate| -> u32 {
        let mut c = cfg();
        c.calib.production_rate = rate;
        let mut with = BattleState::new(0, c.clone());
        let mut without = BattleState::new(0, c);
        for s in [&mut with, &mut without] {
            s.scenario_set_tick(2500); // 125 s in: the regen is double from 120 s
        }
        with.spawn_unit(Team::Blue, COLLECTOR, at(AT), None).unwrap();
        for k in 0..400u32 {
            with.scenario_set_elixir_milli(Team::Blue, 2000);
            without.scenario_set_elixir_milli(Team::Blue, 2000);
            with.tick();
            without.tick();
            if with.elixir_raw(Team::Blue).0 != without.elixir_raw(Team::Blue).0 {
                return k;
            }
        }
        panic!("no payout");
    };
    assert_eq!(first(ProductionRate::FixedInterval), 19 + 259, "fixed_interval: the single-elixir cadence");
    assert_eq!(first(ProductionRate::ScaledWithElixirRate), 19 + 129, "scaled_with_elixir_rate: 100 ms a tick");
}

/// 10. The loader's side of the exemption, on a synthetic file.
#[test]
fn the_blank_hit_speed_exemption_needs_a_mana_block_on_an_inert_building() {
    let row = |name: &str, kind: &str, extra: &str| {
        format!(r#"{{"name":"{name}","kind":"{kind}","elixir":6,"rarity":"Common","hitpoints":400,"collision_radius_milli":1000,"deploy_time_ms":1000,"lifetime_ms":90000{extra}}}"#)
    };
    let text = format!(
        r#"{{"cards":[{},{},{},{}]}}"#,
        row("Pump", "building", r#","mana":{"collect_amount":1,"generate_time_ms":13000,"on_death":1}"#),
        row("DeadPump", "building", ""),
        row("WalkingPump", "troop", r#","hit_speed_ms":1000,"range_milli":1000,"mana":{"collect_amount":1,"generate_time_ms":13000}"#),
        row("HalfPump", "building", r#","mana":{"collect_amount":1}"#),
    );
    let db = CardDb::from_json_str(&text, CardSource::DerivedJson).expect("the file parses");
    let pump = db.get(db.index("Pump").unwrap_or_else(|| panic!("Pump refused: {:?}", db.rejected)));
    assert_eq!((pump.hit_speed_ms, pump.mana.and_then(|m| m.collect)), (0, Some((1, 13000))));
    let why = |n: &str| db.rejected.iter().find(|(r, _)| r == n).map(|(_, w)| w.clone()).unwrap_or_else(|| panic!("{n} loaded"));
    assert_eq!(why("DeadPump"), "missing hit_speed_ms", "an inert building with no mana block still needs its HitSpeed");
    assert!(why("WalkingPump").contains("elixir production"), "a producing troop: {}", why("WalkingPump"));
    assert!(why("HalfPump").contains("elixir production"), "a half block: {}", why("HalfPump"));
}

/// 11. Plant: hash_skips_mana_timer.
#[test]
fn the_payout_timer_is_state() {
    let mut s = BattleState::new(0, cfg());
    s.spawn_unit(Team::Blue, COLLECTOR, at(AT), None).unwrap();
    for _ in 0..30 {
        s.tick();
    }
    let id = s.entities().find(|v| v.card == COLLECTOR).map(|v| v.id).expect("the Collector stands");
    assert_eq!(s.mana_timer(id), Some(13000 - 11 * 50), "loaded at D = 19 and stepped on D..=29");
    let i = id.index as usize;
    // The payout timer is hashed: a save edited only in it fails the load's hash self-check (tests/common
    // edit_is_hashed). Were it not hashed, the edited save would load under the old hash.
    let hashed = edit_is_hashed(&s, |v| {
        let col = v["ents"]["mana_ms"].as_array_mut().expect("the snapshot carries the payout timer");
        let n = col[i].as_i64().expect("a timer");
        col[i] = serde_json::Value::from(n + 1);
    });
    assert!(hashed, "a save edited only in the Collector's timer loads under the old hash: the timer is not hashed");
}

// ---------------------------------------------------------------------------
// THE DEAL (economy.OMIT_FROM_STARTING_HAND)

const DECK: [&str; 8] = [COLLECTOR, "Knight", "Archer", "Giant", "Musketeer", "Fireball", "Zap", "Valkyrie"];
const PLAIN: [&str; 8] = ["Knight", "Archer", "Giant", "Musketeer", "Fireball", "Zap", "Valkyrie", "MiniPekka"];

fn deal(deck: &[&str; 8], seed: u64, shuffle: bool, rule: OmitRule) -> (Vec<String>, Vec<u16>) {
    let mut c = cfg();
    c.decks = [deck.iter().map(|s| s.to_string()).collect(), deck.iter().map(|s| s.to_string()).collect()];
    c.shuffle_decks = shuffle;
    c.calib.omit_from_starting_hand = rule;
    let s = BattleState::new(seed, c);
    (s.hand(Team::Blue).iter().map(|n| n.to_string()).collect(), s.queue_cards(Team::Blue))
}

/// 12. Plant: omit_ignored.
#[test]
fn an_omitted_card_is_never_dealt_into_the_starting_hand() {
    let mut would = 0;
    for seed in 0..1000u64 {
        let (hand, _) = deal(&DECK, seed, true, OmitRule::SwapWithFirstEligibleInQueue);
        assert!(!hand.iter().any(|n| n == COLLECTOR), "seed {seed}: the Collector was dealt into {hand:?}");
        let (old, _) = deal(&DECK, seed, true, OmitRule::NotModelled);
        would += usize::from(old.iter().any(|n| n == COLLECTOR));
    }
    // A shuffle puts it in the first four about half the time; the test is empty if it never does.
    assert!(would > 300, "the shuffle alone dealt the Collector into only {would} of 1000 hands");
}

/// 13. The rule reaches no deck without such a card.
#[test]
fn a_deck_without_one_deals_exactly_as_before() {
    for seed in 0..200u64 {
        for shuffle in [false, true] {
            assert_eq!(
                deal(&PLAIN, seed, shuffle, OmitRule::SwapWithFirstEligibleInQueue),
                deal(&PLAIN, seed, shuffle, OmitRule::NotModelled),
                "seed {seed}, shuffle {shuffle}"
            );
        }
    }
}

/// 14. Plant: omit_ignored.
#[test]
fn unshuffled_the_omitted_card_swaps_with_the_first_card_behind_the_hand() {
    let (hand, queue) = deal(&DECK, 0, false, OmitRule::SwapWithFirstEligibleInQueue);
    assert_eq!(hand, ["Musketeer", "Knight", "Archer", "Giant"], "the Collector's slot takes the fifth card");
    let db = cfg().cards;
    let names: Vec<&str> = queue.iter().map(|i| db.get(*i).name.as_str()).collect();
    assert_eq!(names, [COLLECTOR, "Fireball", "Zap", "Valkyrie"], "the Collector heads the queue");
}
