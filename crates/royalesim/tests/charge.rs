//! CHARGE (Prince, DarkPrince, BattleRam): the ChargeRange /
//! DamageSpecial / ChargeSpeedMultiplier columns (card.rs `ChargeDef`; state.rs
//! `charge_pass`, `effective_speed`, `phase_attack`, `apply_effects`,
//! `phase_target`; combat.rs `fire`; calibration `charge.*`).
//!
//! WHAT IS PINNED, every number read from the loaded CardDb and `Calib::shipped()`:
//!   0. THE 16.402 RUN-UP (calibration charge.ACCUMULATOR =
//!      client16402_progress_permille): the Prince (Speed 60,
//!      ChargeRange 250 in cards.json) gains tdiv(60 x 1000, 250) = 240 permille per
//!      walking tick, is charged after the 42nd (10080 >= 10000) and walks at the
//!      doubled S from the 43RD WALKING TICK -- the measured onset (gen 26 of
//!      frames-20260918-122757.b1: walking from tick 1258, 119..120/tick from 1300);
//!   1. a Prince alone on a straight lane walks at its base step for exactly
//!      ceil(10000 / gain) ticks, is charged at that post-tick, and from then on
//!      walks at floor(S x mult / 100) x spt -- every per-tick delta equal to the
//!      straight-segment step `(0, effective)` (move_towards: trunc_shr8(256 x S) = S,
//!      which path2026::step_delta gives too);
//!   2. its FIRST landed hit on the enemy princess tower removes DamageSpecial
//!      scaled to its level and the SECOND removes Damage (at the default level and
//!      at rarity-local level 1, where DamageSpecial == 2 x Damage in the data);
//!   3. a Zap mid-run-up zeroes the progress in the tick it lands and the Prince
//!      walks the FULL run-up again before it charges;
//!   4. a Knight in the lane takes the charged hit instead of the tower, its second
//!      hit is the plain Damage, and the Prince re-arms on the walk to the tower;
//!   5. DarkPrince and BattleRam load with their charge blocks and charge on the
//!      tick their own ChargeRange predicts;
//!   6. every card without a charge block has effective_speed == speed, every stored
//!      speed is a multiple of SPEED_TO_SUBTILES_PER_TICK, and a scripted battle
//!      with no charge card never moves a charge column -- so such a battle is
//!      bit-identical to the engine before this mechanic (tests/oracle2026.rs is
//!      the exactness gate for the locomotion law itself);
//!   7. a symmetric two-seat Prince scene with a mid-run-up Zap from both seats is
//!      its own mirror every tick (the Canon carries charged / progress / step);
//!   8. a snapshot mid-run-up and one while charged resume hash-for-hash, and both
//!      columns are in the hash;
//!   9. every calibration charge.* candidate moves a measurable behaviour, each on
//!      a scene where the shipped arm gives a different number;
//!  10. the loader refuses a charge block with any of its three
//!      columns missing or non-positive, naming the column, and a charge block on a
//!      building;
//!  11. the Battle Ram's Kamikaze column is NOT implemented: the
//!      Ram re-arms after its charged hit and keeps hitting at Damage -- pinned as a
//!      known gap so a Kamikaze implementation flips it.
//!
//! TICK ALIGNMENT (state.rs `charge_pass`): the run-up gains at the END of the Move
//! phase, after separation, from the step the Path phase REQUESTED in the same tick
//! (`L = min(S, dist, 250)` in native units, recorded by phase_path16402 -- the
//! same place in the tick as the step itself); `effective_speed` is read in the
//! Path phase, BEFORE that tick's gain (the speed reads `progress >= 10000` before
//! the step adds to it). So a unit whose run-up completes in
//! tick index E takes its last base-speed step in E, is `charged` at post-tick E + 1
//! ("post-tick k" = the state after the k-th `tick()` call, `tick_count() == k`) and
//! takes its first doubled step in tick index E + 1. A scenario spawn walks from
//! tick index 0, so a straight run-up of N steps means charged at post-tick N and
//! the first doubled delta between post-ticks N and N + 1.
//!
//! The accumulator used to be walk_delta_length with CHARGE_RANGE_UNIT = centitiles
//! (45000 subtiles); the four displacement accumulators are foils now, and this file
//! reads the run-up in permille. The numbers did not move: 250 centitiles was the
//! same 2.5 tiles as 10 x 250 native.
//!
//! PLANTS: charge_never_ready (the earlier engine, a Prince
//! that never charges) -- (1), (2), (3), (4), (5), (7), (8), (9) go red (8 of 9; (6)
//! stays green because it is the statement that nothing changes WITHOUT a charge
//! card, which the plant does not touch); charge_damage_ignores_level (the level-1
//! DamageSpecial at every level) -- (2), (4), (9) go red with 490 against the
//! level-9 651 (3 of 9).

mod common;

use common::*;
use royalesim::card::{CardDb, CardKind, CardSource, ChargeDef, SpellShape};
use royalesim::combat::damage_against;
use royalesim::entity::EntityKind;
use royalesim::fixed::{centi, Vec2, SUBTILE, SUBTILE_PER_MILLITILE};
use royalesim::path2026;
use royalesim::state::{BattleConfig, BattleState, Calib, ChargeAccumulator, ChargeLevelScaling, ChargeMultiplier, ChargeRangeUnit, ChargeStopRule};
use royalesim::{EntityId, Team};

// ---------------------------------------------------------------------------
// data, read not typed

fn calib() -> Calib {
    Calib::shipped()
}

fn spt() -> i32 {
    calib().speed_to_subtiles_per_tick
}

fn bare(cfg: BattleConfig) -> BattleState {
    BattleState::new(7, cfg)
}

fn with_calib(f: impl FnOnce(&mut Calib)) -> BattleConfig {
    let mut cfg = config();
    f(&mut cfg.calib);
    cfg
}

fn charge_of(s: &BattleState, card: &str) -> ChargeDef {
    card_stat(s, card).charge.unwrap_or_else(|| panic!("cards.json {card} has no charge block"))
}

/// The shipped arms this file pins (asserted, so a registry change re-points this
/// file instead of silently moving every number).
fn assert_shipped_arms() {
    let c = calib();
    assert_eq!(c.charge_accumulator, ChargeAccumulator::Client16402ProgressPermille, "the shipped arm this file pins");
    assert_eq!(c.charge_range_unit, ChargeRangeUnit::Centitiles, "the displacement foils' unit this file pins");
    assert_eq!(c.charge_multiplier_meaning, ChargeMultiplier::MovementWhenCharged, "the shipped arm this file pins");
}

/// The run-up threshold under the shipped accumulator: 10000 permille of
/// ChargeRange (`progress >= 10000`).
fn need_permille() -> i32 {
    assert_shipped_arms();
    10_000
}

/// The per-walking-tick gain of a unit whose subtile step is `step`:
/// tdiv(S x 1000, ChargeRange) with S the NATIVE speed (`step / 18`).
fn gain_per_tick(step: i32, ch: ChargeDef) -> i32 {
    assert_eq!(step % SUBTILE_PER_MILLITILE, 0, "a stored speed is S x 18 exactly");
    (step / SUBTILE_PER_MILLITILE) * 1000 / ch.range_raw
}

/// The distance in SUBTILES a straight free run-up covers: 10 x ChargeRange native
/// units (= `centi(range_raw)`, the earlier reading, the same distance).
fn need_subtiles(s: &BattleState, card: &str) -> i32 {
    assert_shipped_arms();
    let raw = charge_of(s, card).range_raw;
    assert_eq!(centi(raw), 10 * raw * SUBTILE_PER_MILLITILE, "centitiles and 10 x native are the same distance");
    centi(raw)
}

/// ceil(need / gain): the base-speed steps of a straight run-up.
fn run_up_ticks(need: i32, gain: i32) -> u32 {
    ((need + gain - 1) / gain) as u32
}

/// The measured buff law (calibration movement.BUFF_SPEED_RULE) on the NATIVE speed:
/// floor(S x mult / 100), back to subtiles.
fn charged_step(base: i32, ch: ChargeDef) -> i32 {
    (base / spt() * ch.speed_multiplier_percent / 100) * spt()
}

/// The lane spot: x on a half-tile CELL CENTRE so the 2026 route is a straight
/// +y walk (a cell boundary gives the first segment a lateral component and the
/// per-axis truncation shortens the step -- which is exactly what separates the
/// accumulator arms in (9)).
fn lane_spot() -> Vec2 {
    t(375, 850)
}

/// Assert the unit's route is a straight +y walk in its own frame (every node on
/// the unit's x), so the step is `(0, effective)` and the run-up arithmetic of the
/// module doc applies. A vacuity guard, not a claim about the game.
fn assert_straight_lane(s: &BattleState, id: EntityId) {
    let e = s.entity(id).unwrap();
    assert!(!e.route.is_empty(), "vacuous: no route planned");
    assert!(e.route.iter().all(|p| p.x == e.pos.x), "scene: the route is not a straight lane walk ({:?} from {:?})", e.route, e.pos);
}

fn zap_stun_ms(s: &BattleState) -> i32 {
    match card_stat(s, "Zap").spell.clone().unwrap().shape {
        SpellShape::AreaEffect { hit } => hit.stun_ms,
        other => panic!("Zap is not an area effect: {other:?}"),
    }
}

/// Run until `read` changes value `n` times (or `max` ticks): (post-tick, drop) per change.
fn first_drops(s: &mut BattleState, n: usize, max: u32, read: impl Fn(&BattleState) -> i32) -> Vec<(u32, i32)> {
    let mut last = read(s);
    let mut out = Vec::new();
    for _ in 0..max {
        s.tick();
        let now = read(s);
        if now != last {
            out.push((s.tick_count(), last - now));
            last = now;
            if out.len() == n {
                break;
            }
        }
    }
    out
}

/// The first post-tick at which `id` is charged, ticking at most `max` times.
fn charged_at(s: &mut BattleState, id: EntityId, max: u32) -> Option<u32> {
    for _ in 0..max {
        s.tick();
        if s.entity(id).is_some_and(|e| e.charged) {
            return Some(s.tick_count());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// (1): the run-up and the doubled step

#[test]
fn a_prince_alone_walks_the_run_up_at_base_speed_then_at_the_multiplied_speed() {
    // Plant charge_never_ready: never charged, the step never doubles.
    let mut s = bare(config());
    let ch = charge_of(&s, "Prince");
    let need = need_permille();
    let id = s.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
    let base = s.entity(id).unwrap().speed;
    assert_eq!(base, card_stat(&s, "Prince").move_speed() * spt(), "data: the base step is Speed x SPEED_TO_SUBTILES_PER_TICK");
    let gain = gain_per_tick(base, ch);
    let fast = charged_step(base, ch);
    assert!(fast > base, "data: ChargeSpeedMultiplier {} does not raise the speed", ch.speed_multiplier_percent);
    // The locomotion law on a straight +y segment: the step IS the speed.
    let up = Vec2::new(0, 256);
    assert_eq!(path2026::step_delta(base, up), Vec2::new(0, base));
    assert_eq!(path2026::step_delta(fast, up), Vec2::new(0, fast));
    let n = run_up_ticks(need, gain);
    assert!(n > 1, "vacuous: a one-tick run-up");
    let mut before = s.entity(id).unwrap();
    let mut pos = before.pos;
    for k in 1..=n + 20 {
        let eff = before.effective_speed;
        s.tick();
        if k == 1 {
            assert_straight_lane(&s, id);
        }
        let e = s.entity(id).unwrap();
        let delta = e.pos.sub(pos);
        // The delta of tick index k - 1 is step_delta(the effective speed read in
        // its Path phase, straight up).
        assert_eq!(delta, path2026::step_delta(eff, up), "post-tick {k}: the step is not the effective speed's step_delta");
        if k < n {
            assert!(!e.charged, "post-tick {k}: charged before the run-up ({need} permille at {gain}/tick) is covered");
            assert_eq!(e.charge_progress, (k as i32) * gain, "post-tick {k}: the run-up counts the requested step");
            assert_eq!(e.effective_speed, base, "post-tick {k}: buffed before the charge");
            assert_eq!(delta, Vec2::new(0, base));
        } else {
            assert!(e.charged, "post-tick {k}: not charged after {n} steps of {gain} permille (need {need})");
            assert_eq!(e.charge_progress, 0, "post-tick {k}: the run-up is zeroed when the charge completes");
            assert_eq!(e.effective_speed, fast, "post-tick {k}: the charged speed is floor(S x mult / 100) x spt");
            if k > n {
                assert_eq!(delta, Vec2::new(0, fast), "post-tick {k}: the charged step");
            } else {
                assert_eq!(delta, Vec2::new(0, base), "post-tick {k}: the tick that completes the run-up still steps at base");
            }
        }
        before = e;
        pos = e.pos;
    }
}

// ---------------------------------------------------------------------------
// (0): the 16.402 run-up, pinned from the data

#[test]
fn the_prince_walks_42_ticks_at_speed_60_and_doubles_from_the_43rd_walking_tick_as_measured() {
    // Plant charge_never_ready: never doubles. The 16.402 arm:
    // `progress += tdiv(L x 1000, ChargeRange)` per walking tick, L the
    // requested step min(S, dist, 250); the speed doubles at
    // `progress >= 10000` from the NEXT tick. Every number below is read from
    // cards.json (Prince Speed 60, ChargeRange 250, ChargeSpeedMultiplier 200); the
    // 42 / 43 are asserted so the file names the measured onset in the open.
    let mut s = bare(config());
    assert_eq!(calib().charge_accumulator, ChargeAccumulator::Client16402ProgressPermille, "the shipped arm this test pins");
    let c = card_stat(&s, "Prince");
    let ch = charge_of(&s, "Prince");
    let native_s = c.move_speed();
    assert_eq!((native_s, ch.range_raw, ch.speed_multiplier_percent), (60, 250, 200), "data: the 2018 Prince row the live onset was measured against (16.402 ships the same three)");
    let gain = tdiv16402(native_s * 1000, ch.range_raw);
    assert_eq!(gain, 240, "tdiv(60 x 1000, 250)");
    let charged_after = ((10_000 + gain - 1) / gain) as u32;
    assert_eq!(charged_after, 42, "42 x 240 = 10080 >= 10000, 41 x 240 = 9840 < 10000");
    let first_doubled = charged_after + 1;
    assert_eq!(first_doubled, 43, "the 43rd walking tick is the first at S 120");
    let id = s.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
    // Walking ticks are counted from the unit's own first moving tick, as the
    // capture measurement counts them.
    let mut walking = 0u32;
    let mut pos = s.entity(id).unwrap().pos;
    let mut seen_doubled = 0;
    for _ in 0..charged_after + 30 {
        s.tick();
        let e = s.entity(id).unwrap();
        let d = e.pos.sub(pos);
        pos = e.pos;
        if d == Vec2::default() {
            continue;
        }
        walking += 1;
        assert_eq!(d.x, 0, "walking tick {walking}: not a straight lane walk ({d:?})");
        assert_eq!(d.y % SUBTILE_PER_MILLITILE, 0, "walking tick {walking}: the step is not a whole native unit");
        let native_step = d.y / SUBTILE_PER_MILLITILE;
        if walking < first_doubled {
            assert_eq!(native_step, native_s, "walking tick {walking}: S before the charge");
            assert_eq!(e.charged, walking >= charged_after, "walking tick {walking}: charged");
            assert_eq!(e.charge_progress, if walking < charged_after { walking as i32 * gain } else { 0 }, "walking tick {walking}: the permille run-up");
        } else {
            assert_eq!(native_step, native_s * ch.speed_multiplier_percent / 100, "walking tick {walking}: the doubled S from the 43rd walking tick on");
            assert!(e.charged, "walking tick {walking}");
            seen_doubled += 1;
        }
    }
    assert!(seen_doubled >= 20, "vacuous: {seen_doubled} doubled walking ticks seen ({walking} walking ticks)");
}

/// The 16.402 `tdiv` (truncating division), spelled out so the test does not depend
/// on the engine's helper for the number it pins.
fn tdiv16402(a: i32, b: i32) -> i32 {
    a / b
}

// ---------------------------------------------------------------------------
// (2): the charged hit on the princess tower

/// (post-tick, drop) of the enemy princess tower's first two hits from a Prince at
/// `level`, plus (special, damage) scaled by the CardDb for that level.
fn tower_hits(cfg: BattleConfig, level: Option<i32>) -> (Vec<(u32, i32)>, i32, i32) {
    let mut cfg = cfg;
    if let Some(l) = level {
        cfg.card_level = [l, l];
    }
    let mut s = bare(cfg);
    let level = s.config().card_level[Team::Blue as usize];
    let ch = charge_of(&s, "Prince");
    let db = s.cards();
    let idx = db.index("Prince").unwrap();
    let special = db.scaled(idx, level, ch.damage_special).unwrap();
    let damage = db.scaled(idx, level, card_stat(&s, "Prince").damage).unwrap();
    let pct = card_stat(&s, "Prince").crown_tower_damage_percent;
    let rounding = s.config().calib.crown_rounding;
    let want = |d: i32| damage_against(EntityKind::PrincessTower, d, pct, rounding);
    let id = s.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
    // The lane's engine-Left tower (k = 1).
    let hits = first_drops(&mut s, 2, 600, |s| s.tower_hp(Team::Red)[1]);
    assert_eq!(hits.len(), 2, "vacuous: the Prince did not land two hits: {hits:?}");
    // Consumed: not charged any more after the first hit, and it was charged right
    // before it (the hit tick's Attack phase read `charged`).
    let _ = id;
    (hits, want(special), want(damage))
}

#[test]
fn the_first_landed_hit_on_the_princess_tower_is_damage_special_scaled_and_the_second_is_damage() {
    // Plants charge_never_ready (both hits plain Damage) and
    // charge_damage_ignores_level (the first hit 490 at a level whose scaled value is not).
    let s0 = bare(config());
    let ch = charge_of(&s0, "Prince");
    let damage = card_stat(&s0, "Prince").damage;
    assert_eq!(ch.damage_special, 2 * damage, "data: DamageSpecial is exactly 2 x Damage on the 2018 Prince row (the 'replace, not add' reading rests on it)");
    let db = s0.cards();
    let idx = db.index("Prince").unwrap();
    let rel = db.rarity("Epic").unwrap().relative_level;
    let level1 = rel + 1;
    let default = s0.config().card_level[0];
    assert!(default > level1, "vacuous: the default level is rarity-local 1, where every scaling agrees");
    assert_eq!(calib().charge_special_level_scaling, ChargeLevelScaling::ScaleSpecialBase, "the shipped arm this test pins");
    assert_ne!(db.scaled(idx, default, ch.damage_special).unwrap(), ch.damage_special, "vacuous: the level does not scale DamageSpecial at the default level");
    for level in [None, Some(level1)] {
        let (hits, special, plain) = tower_hits(config(), level);
        assert_eq!(hits[0].1, special, "level {level:?}: the first landed hit is DamageSpecial scaled to the level ({hits:?})");
        assert_eq!(hits[1].1, plain, "level {level:?}: the second landed hit is Damage ({hits:?})");
        assert!(hits[1].0 > hits[0].0);
    }
}

// ---------------------------------------------------------------------------
// (3): a Zap resets the run-up

#[test]
fn a_zap_on_the_charging_prince_zeroes_the_run_up_and_it_walks_the_full_run_up_again() {
    // Plant charge_never_ready: never charged at all.
    assert!(calib().charge_reset_on_stun, "the shipped arm this test pins");
    assert_eq!(calib().charge_progress_on_stop, ChargeStopRule::Reset, "the shipped arm this test pins");
    assert!(zap_stun_ms(&bare(config())) > 0, "data: Zap stuns");
    let mut s = bare(config());
    let ch = charge_of(&s, "Prince");
    let need = need_subtiles(&s, "Prince");
    let id = s.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
    let base = s.entity(id).unwrap().speed;
    let gain = gain_per_tick(base, ch);
    let n = run_up_ticks(need_permille(), gain);
    let z = n / 2;
    for _ in 0..z {
        s.tick();
    }
    let e = s.entity(id).unwrap();
    assert!(!e.charged && e.charge_progress == (z as i32) * gain, "vacuous: no run-up in progress at post-tick {z}");
    s.spawn_unit(Team::Red, "Zap", e.pos, None).unwrap();
    s.tick();
    let e = s.entity(id).unwrap();
    assert!(e.stun_ms > 0, "the Zap did not land in the tick it materialised");
    assert_eq!(e.charge_progress, 0, "the stun did not zero the run-up");
    assert!(!e.charged);
    let y_reset = e.pos.y;
    // Held: no walk, no gain, progress stays at zero (Reset on a zero is a no-op).
    let mut resumed = None;
    for _ in 0..40 {
        s.tick();
        let e = s.entity(id).unwrap();
        if e.charge_progress > 0 {
            resumed = Some(s.tick_count());
            break;
        }
        assert_eq!(e.pos.y, y_reset, "moved while stunned");
    }
    let m = resumed.expect("vacuous: the Prince never resumed walking");
    assert!(m > z + 2, "vacuous: the stun held no tick");
    // From the first walking post-tick m, a full run-up of n steps: charged at m + n - 1.
    let at = charged_at(&mut s, id, n + 5).expect("never charged after the stun");
    assert_eq!(at, m + n - 1, "the Prince did not walk the full run-up again after the stun");
    assert!(s.entity(id).unwrap().pos.y - y_reset >= need, "charged after less than the run-up ({} of {need})", s.entity(id).unwrap().pos.y - y_reset);
}

// ---------------------------------------------------------------------------
// (4): a Knight in the lane

#[test]
fn a_knight_in_the_lane_takes_the_charged_hit_instead_of_the_tower_and_the_prince_re_arms() {
    // Plants charge_never_ready and charge_damage_ignores_level.
    let mut s = bare(config());
    let ch = charge_of(&s, "Prince");
    let level = s.config().card_level[0];
    let db = s.cards();
    let idx = db.index("Prince").unwrap();
    let special = db.scaled(idx, level, ch.damage_special).unwrap();
    let plain = db.scaled(idx, level, card_stat(&s, "Prince").damage).unwrap();
    let p = s.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
    // Far enough up the lane that the Prince covers its run-up before the two meet.
    let k = s.scenario_spawn_now(Team::Red, "Knight", Vec2::new(lane_spot().x, lane_spot().y + 25 * SUBTILE / 2), None).unwrap();
    assert!(s.entity(k).unwrap().hp > special, "vacuous: the Knight would die to the charged hit");
    let mut charged_before_hit = false;
    let mut last = s.entity(k).unwrap().hp;
    let mut knight_hits = Vec::new();
    let mut tower_last = s.tower_hp(Team::Red)[1];
    let mut tower_hits = Vec::new();
    for _ in 0..600 {
        let was = s.entity(p).is_some_and(|e| e.charged);
        s.tick();
        let hp = s.entity(k).map_or(0, |e| e.hp);
        if hp != last {
            if knight_hits.is_empty() {
                charged_before_hit = was;
            }
            knight_hits.push((s.tick_count(), last - hp));
            last = hp;
        }
        let th = s.tower_hp(Team::Red)[1];
        if th != tower_last {
            tower_hits.push((s.tick_count(), tower_last - th));
            tower_last = th;
        }
        if tower_hits.len() >= 2 {
            break;
        }
    }
    assert!(knight_hits.len() >= 2, "vacuous: the Knight took {knight_hits:?}");
    assert!(charged_before_hit, "vacuous: the Prince was not charged when it swung at the Knight");
    assert_eq!(knight_hits[0].1, special, "the Knight did not take the charged hit ({knight_hits:?})");
    assert_eq!(knight_hits[1].1, plain, "the Knight's second hit is the plain Damage ({knight_hits:?})");
    assert!(tower_hits[0].0 > knight_hits[0].0, "the tower was hit before the Knight");
    assert_eq!(tower_hits[0].1, special, "the Prince did not re-arm on the walk from the Knight to the tower ({tower_hits:?})");
    assert_eq!(tower_hits[1].1, plain, "{tower_hits:?}");
}

// ---------------------------------------------------------------------------
// (5): the other two charge cards

#[test]
fn dark_prince_and_battle_ram_load_with_charge_and_charge_on_their_own_run_up() {
    // Plant charge_never_ready.
    let s0 = bare(config());
    for name in ["Prince", "DarkPrince", "BattleRam"] {
        let c = card_stat(&s0, name);
        let ch = charge_of(&s0, name);
        assert_eq!(c.kind, CardKind::Troop);
        assert_eq!(ch.damage_special, 2 * c.damage, "data: {name} DamageSpecial is 2 x Damage");
        assert!(ch.range_raw > 0 && ch.speed_multiplier_percent > 100, "data: {name} {ch:?}");
        assert!(c.ignore_pushback, "data: {name} ships IgnorePushback (the Fireball asymmetry in (9) rests on it)");
    }
    let mut seen = Vec::new();
    for name in ["DarkPrince", "BattleRam"] {
        let mut s = bare(config());
        let ch = charge_of(&s, name);
        let need = need_permille();
        let id = s.scenario_spawn_now(Team::Blue, name, lane_spot(), None).unwrap();
        let base = s.entity(id).unwrap().speed;
        let n = run_up_ticks(need, gain_per_tick(base, ch));
        let at = charged_at(&mut s, id, n + 5).unwrap_or_else(|| panic!("{name} never charged"));
        assert_straight_lane(&s, id);
        assert_eq!(at, n, "{name}: charged at post-tick {at}, its ChargeRange {} predicts {n}", ch.range_raw);
        assert_eq!(s.entity(id).unwrap().effective_speed, charged_step(base, ch), "{name}: the charged speed");
        seen.push((name, n));
    }
    // The two run-ups differ in the data (250 vs 300), so the tick is the card's own.
    assert_ne!(seen[0].1, seen[1].1, "vacuous: {seen:?}");
}

// ---------------------------------------------------------------------------
// (6): nothing changes for a unit without a charge block

fn no_charge_decks() -> BattleConfig {
    let mut cfg = config();
    let red: Vec<String> = RED_DECK.iter().map(|c| if *c == "Prince" { "Valkyrie".to_string() } else { c.to_string() }).collect();
    cfg.decks = [BLUE_DECK.iter().map(|s| s.to_string()).collect(), red];
    for d in &cfg.decks {
        for name in d {
            assert!(card_stat(&bare(config()), name).charge.is_none(), "scene: {name} charges");
        }
    }
    cfg
}

#[test]
fn every_unit_without_a_charge_block_has_effective_speed_equal_to_speed_on_every_card_and_every_tick() {
    let s0 = bare(config());
    let db = s0.cards();
    let mut checked = 0;
    let mut chargers = 0;
    for c in &db.cards {
        if c.spell.is_some() || c.summon_only || db.index(&c.name) != Some(db.cards.iter().position(|x| std::ptr::eq(x, c)).unwrap() as u16) {
            continue;
        }
        let mut s = bare(config());
        let Ok(id) = s.scenario_spawn_now(Team::Blue, &c.name, t(900, 900), None) else { continue };
        let e = s.entity(id).unwrap();
        assert_eq!(e.speed % spt(), 0, "{}: a stored speed that is not a multiple of SPEED_TO_SUBTILES_PER_TICK", c.name);
        // At spawn nothing is charged, so the hook is the identity on every card.
        assert_eq!(e.effective_speed, e.speed, "{}: effective_speed differs from speed at spawn", c.name);
        assert!(!e.charged && e.charge_progress == 0);
        if c.charge.is_some() {
            chargers += 1;
        } else {
            for _ in 0..30 {
                s.tick();
                let e = s.entity(id).unwrap();
                assert_eq!(e.effective_speed, e.speed, "{}: effective_speed moved on a card without a charge block", c.name);
                assert!(!e.charged && e.charge_progress == 0, "{}: a charge column moved without a charge block", c.name);
            }
        }
        checked += 1;
    }
    assert!(checked >= 20 && chargers == 3, "vacuous: {checked} cards checked, {chargers} chargers");
    // A scripted battle with no charge card: the hook is the identity on every entity
    // of every tick and no charge column ever moves -- the analytic statement that
    // such a battle is bit-identical to the engine before this mechanic (the only
    // engine paths touched read `charged` or a charge block first).
    let cfg = no_charge_decks();
    let mut s = BattleState::new(0xC1A5, cfg);
    let mut script = Script::new(40);
    let mut troop_ticks = 0u64;
    while !s.is_done() && s.tick_count() < 2000 {
        script.step(&mut s);
        s.tick();
        for e in s.entities() {
            assert_eq!(e.effective_speed, e.speed, "tick {}: {} effective_speed != speed", s.tick_count(), e.card);
            assert!(!e.charged && e.charge_progress == 0, "tick {}: {} has charge state", s.tick_count(), e.card);
            troop_ticks += u64::from(e.kind == EntityKind::Troop);
        }
    }
    assert!(troop_ticks > 5_000, "vacuous: {troop_ticks} troop-ticks");
}

// ---------------------------------------------------------------------------
// (7): mirror symmetry

#[test]
fn a_symmetric_two_prince_scene_with_a_mid_run_up_zap_from_both_seats_stays_its_own_mirror_every_tick() {
    // A cell-BOUNDARY x, so the walk has a lateral component: an accumulator that
    // read an engine axis instead of a frame-free scalar would desync here. Under
    // `symmetric_config` (the trace-fitted search): the shipped search is the
    // game's absolute-grid one and is not seat-symmetric (tests/common
    // `symmetric_config`, mirror.rs `the_shipped_search_is_absolute_grid_not_seat_symmetric`),
    // and a boundary x is exactly where its floor-division start cell differs
    // between the seats.
    let mut s = bare(symmetric_config());
    let blue_at = t(350, 850);
    let red_at = mirror(&s, blue_at);
    let ids = s.scenario_spawn_batch(&[(Team::Red, "Prince", red_at, None), (Team::Blue, "Prince", blue_at, None)]).unwrap();
    check_mirror(&s).unwrap();
    let n = run_up_ticks(need_permille(), gain_per_tick(s.entity(ids[1]).unwrap().speed, charge_of(&s, "Prince")));
    let (mut both_charged, mut both_reset, mut tower_hit) = (false, false, false);
    for k in 0..330 {
        if k == n / 2 {
            // Each seat zaps the OTHER seat's Prince, on the same tick.
            let (b, r) = (s.entity(ids[1]).unwrap().pos, s.entity(ids[0]).unwrap().pos);
            s.spawn_unit(Team::Blue, "Zap", r, None).unwrap();
            s.spawn_unit(Team::Red, "Zap", b, None).unwrap();
        }
        s.tick();
        check_mirror(&s).unwrap_or_else(|e| panic!("{e}\ncensus: {:?}", census(&s)));
        let (b, r) = (s.entity(ids[1]), s.entity(ids[0]));
        if let (Some(b), Some(r)) = (b, r) {
            both_charged |= b.charged && r.charged;
            both_reset |= k > n / 2 && b.stun_ms > 0 && r.stun_ms > 0 && b.charge_progress == 0;
        }
        tower_hit |= s.tower_hp(Team::Red)[1] < s.tower_hp(Team::Red)[2];
        if tower_hit && both_charged && both_reset {
            break;
        }
    }
    assert!(both_charged && both_reset && tower_hit, "vacuous: charged {both_charged}, reset {both_reset}, tower hit {tower_hit}");
}

// ---------------------------------------------------------------------------
// (8): save / load

#[test]
fn a_snapshot_mid_run_up_and_one_while_charged_resume_hash_for_hash_and_both_columns_are_hashed() {
    // Plant charge_never_ready: the charged half fails on vacuity.
    let mut s = bare(config());
    let id = s.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
    let n = run_up_ticks(need_permille(), gain_per_tick(s.entity(id).unwrap().speed, charge_of(&s, "Prince")));
    for _ in 0..n / 2 {
        s.tick();
    }
    let e = s.entity(id).unwrap();
    assert!(e.charge_progress > 0 && !e.charged, "vacuous: not mid-run-up");
    let resume = |s: &mut BattleState, label: &str| {
        let blob = s.save();
        let mut l = BattleState::load(&blob).unwrap();
        assert_eq!(l.state_hash(), s.state_hash(), "{label}: load");
        let (a, b) = (s.entity(id).unwrap(), l.entity(id).unwrap());
        assert_eq!((a.charged, a.charge_progress), (b.charged, b.charge_progress), "{label}: the columns did not round-trip");
        for k in 0..300 {
            s.tick();
            l.tick();
            assert_eq!(l.state_hash(), s.state_hash(), "{label}: diverged {k} ticks after the load");
        }
    };
    // Hash sensitivity, on a clone so the run is untouched.
    let mut c = s.clone();
    let h = c.state_hash();
    assert!(c.debug_set_charge(id, e.charge_progress + 1, e.charged));
    assert_ne!(c.state_hash(), h, "charge_progress is not in the hash");
    assert!(c.debug_set_charge(id, e.charge_progress, !e.charged));
    assert_ne!(c.state_hash(), h, "charged is not in the hash");
    assert!(c.debug_set_charge(id, e.charge_progress, e.charged));
    assert_eq!(c.state_hash(), h);
    resume(&mut s, "mid-run-up");
    // A fresh scene taken while charged (the first one has fought the tower by now).
    let mut s = bare(config());
    let id = s.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
    assert_eq!(charged_at(&mut s, id, n + 5), Some(n));
    s.tick();
    assert!(s.entity(id).unwrap().charged, "vacuous: not charged at the save");
    resume(&mut s, "charged");
}

// ---------------------------------------------------------------------------
// (9): every candidate moves a measurable behaviour

/// The post-tick a lone Prince at `at` is charged, and its first two deltas.
fn charge_tick_at(cfg: BattleConfig, at: Vec2) -> (Option<u32>, Vec<i32>) {
    let mut s = bare(cfg);
    let id = s.scenario_spawn_now(Team::Blue, "Prince", at, None).unwrap();
    let mut steps = Vec::new();
    let mut pos = at;
    let mut charged = None;
    for _ in 0..120 {
        s.tick();
        let e = s.entity(id).unwrap();
        if steps.len() < 2 {
            steps.push(e.pos.sub(pos).len());
        }
        pos = e.pos;
        if charged.is_none() && e.charged {
            charged = Some(s.tick_count());
        }
    }
    (charged, steps)
}

/// A Prince body-blocked behind a friendly Giant in its lane: the post-tick it
/// charges (None: not within 200 ticks -- under the 16.402 contact law the
/// Giant's stomp pauses push the Prince BACK a step every period, which an
/// accumulator that floors a backward walk at zero reads as a stop).
fn blocked_charge_tick(cfg: BattleConfig) -> Option<u32> {
    let mut s = bare(cfg);
    let giant = s.scenario_spawn_now(Team::Blue, "Giant", Vec2::new(lane_spot().x, lane_spot().y + 3 * SUBTILE / 2), None).unwrap();
    let p = s.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
    let at = charged_at(&mut s, p, 200);
    let (pp, gp) = (s.entity(p).unwrap().pos, s.entity(giant).unwrap().pos);
    assert!(gp.y > pp.y && gp.y - pp.y < 2 * SUBTILE, "vacuous: the Prince is not behind the Giant ({pp:?} / {gp:?})");
    at
}

/// A charged Prince walking the lane hit by a Red spell cast `ahead` tiles up the lane
/// once it is charged: (was pushed, charged after the hit, hp dropped).
fn charged_prince_hit_by(cfg: BattleConfig, spell: &str, ahead: i32) -> (bool, bool, bool) {
    let mut s = bare(cfg);
    let mut control = bare(config());
    let id = s.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
    control.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
    let n = run_up_ticks(need_permille(), gain_per_tick(s.entity(id).unwrap().speed, charge_of(&s, "Prince")));
    for _ in 0..n + 2 {
        s.tick();
        control.tick();
    }
    let e = s.entity(id).unwrap();
    assert!(e.charged, "vacuous: not charged before the {spell}");
    let (tap, full) = (Vec2::new(e.pos.x, e.pos.y + ahead * SUBTILE), e.hp);
    s.spawn_unit(Team::Red, spell, tap, None).unwrap();
    for _ in 0..80 {
        s.tick();
        control.tick();
        let (a, b) = (s.entity(id).unwrap(), control.entity(id).unwrap());
        if a.hp < full {
            return (a.pos != b.pos, a.charged, true);
        }
    }
    (false, s.entity(id).unwrap().charged, false)
}

/// A charged Prince with the tower as its live target, a Red Knight dropped beside
/// it: charged after it switches to the Knight.
fn charged_after_retarget(cfg: BattleConfig) -> bool {
    let mut s = bare(cfg);
    let p = s.scenario_spawn_now(Team::Blue, "Prince", Vec2::new(lane_spot().x, lane_spot().y + 9 * SUBTILE), None).unwrap();
    s.tick();
    let e = s.entity(p).unwrap();
    let tower = s.tower_ids(Team::Red)[1].unwrap();
    assert_eq!(e.target, Some(tower), "vacuous: the tower is not the live target");
    let beside = Vec2::new(e.pos.x + 3 * SUBTILE, e.pos.y + 2 * SUBTILE);
    assert!(s.debug_set_charge(p, 0, true));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", beside, None).unwrap();
    s.tick();
    let e = s.entity(p).unwrap();
    assert_eq!(e.target, Some(knight), "vacuous: the Prince did not switch to the Knight");
    e.charged
}

#[test]
fn every_charge_candidate_moves_a_measurable_behaviour() {
    // Plant charge_never_ready: nothing charges under any arm.
    let s0 = bare(config());
    let ch = charge_of(&s0, "Prince");
    let base = card_stat(&s0, "Prince").move_speed() * spt();
    let gain = gain_per_tick(base, ch);
    let n = run_up_ticks(need_permille(), gain);
    let (shipped, steps) = charge_tick_at(config(), lane_spot());
    assert_eq!(shipped, Some(n));
    assert_eq!(steps, vec![base, base]);
    let arm = |a: ChargeAccumulator| with_calib(move |c| c.charge_accumulator = a);

    // CHARGE_RANGE_UNIT: read by the four displacement foils only (the 16.402
    // accumulator divides by the raw column and never converts it): under
    // walk_delta_length, millitiles is a run-up of 18 subtiles per raw unit and
    // centitiles the same 2.5 tiles as the shipped 10 x native; under the shipped arm
    // the key is inert.
    let (milli, _) = charge_tick_at(
        with_calib(|c| {
            c.charge_accumulator = ChargeAccumulator::WalkDeltaLength;
            c.charge_range_unit = ChargeRangeUnit::Millitiles;
        }),
        lane_spot(),
    );
    assert_eq!(milli, Some(run_up_ticks(royalesim::fixed::milli(ch.range_raw), base)));
    assert!(milli.unwrap() < n, "millitiles: {milli:?} vs centitiles {n}");
    assert_eq!(charge_tick_at(arm(ChargeAccumulator::WalkDeltaLength), lane_spot()).0, Some(n), "centitiles under walk_delta_length is the shipped distance");
    let (inert, _) = charge_tick_at(with_calib(|c| c.charge_range_unit = ChargeRangeUnit::Millitiles), lane_spot());
    assert_eq!(inert, Some(n), "the shipped accumulator does not read CHARGE_RANGE_UNIT");

    // ACCUMULATOR. On the cell-centre lane all five agree (the degeneracy the ledger
    // names); each foil parts from the shipped arm on its own scene.
    for a in [ChargeAccumulator::WalkDeltaLength, ChargeAccumulator::WalkDeltaTowardTarget, ChargeAccumulator::NetMoveLength, ChargeAccumulator::TimeMoving] {
        assert_eq!(charge_tick_at(arm(a), lane_spot()).0, Some(n), "{a:?}: the free straight walk is the degenerate case");
    }
    // Body-blocked behind a Giant, the net move is the Giant's pace: the shipped arm
    // counts the REQUESTED step (L, not the displacement) and
    // charges on the free-walk tick; the length readings charge later. Under the
    // 16.402 locomotion the Path phase's delta IS the position write, so
    // walk_delta_length and net_move_length are the same foil there (they part
    // under the trace-fitted arm only); walk_delta_toward_target reads every
    // stomp-period back-push as a stop and may never charge (None).
    let blocked: Vec<(ChargeAccumulator, Option<u32>)> = [
        ChargeAccumulator::Client16402ProgressPermille,
        ChargeAccumulator::WalkDeltaLength,
        ChargeAccumulator::WalkDeltaTowardTarget,
        ChargeAccumulator::NetMoveLength,
        ChargeAccumulator::TimeMoving,
    ]
    .into_iter()
    .map(|a| (a, blocked_charge_tick(arm(a))))
    .collect();
    println!("body-blocked Prince, charge post-tick per accumulator: {blocked:?}");
    assert_eq!(blocked[0].1, Some(n), "client16402_progress_permille: a body-blocked Prince still gains the full requested step ({blocked:?})");
    assert!(blocked[1].1.unwrap() > n, "walk_delta_length does not see the block: {blocked:?}");
    assert!(blocked[3].1.unwrap() > n, "net_move_length does not see the block: {blocked:?}");
    // time_moving: a cell-BOUNDARY x gives the walk a lateral component, the per-axis
    // truncation shortens each step below S, and the length reading needs one more
    // tick; the shipped arm reads the requested step, which the truncation never touches.
    let boundary = t(350, 850);
    let (len_at, _) = charge_tick_at(arm(ChargeAccumulator::WalkDeltaLength), boundary);
    let (time_at, _) = charge_tick_at(arm(ChargeAccumulator::TimeMoving), boundary);
    let (shipped_at, _) = charge_tick_at(config(), boundary);
    assert!(time_at.unwrap() < len_at.unwrap(), "time_moving vs walk_delta_length on a lateral walk: {time_at:?} vs {len_at:?}");
    assert_eq!(shipped_at, Some(n), "client16402_progress_permille on a lateral walk: the requested step is S regardless of heading");
    // walk_delta_toward_target: a route that is not aimed at the tower counts less.
    let corner = t(100, 850);
    let (len_at, _) = charge_tick_at(arm(ChargeAccumulator::WalkDeltaLength), corner);
    let (toward_at, _) = charge_tick_at(arm(ChargeAccumulator::WalkDeltaTowardTarget), corner);
    assert!(toward_at.unwrap() > len_at.unwrap(), "walk_delta_toward_target on a detour: {toward_at:?} vs {len_at:?}");

    // MULTIPLIER_MEANING.
    let (always_at, always_steps) = charge_tick_at(with_calib(|c| c.charge_multiplier_meaning = ChargeMultiplier::MovementAlways), lane_spot());
    assert_eq!(always_steps, vec![charged_step(base, ch); 2], "movement_always: the first steps are the multiplied ones");
    assert!(always_at.unwrap() < n, "movement_always covers the run-up faster: {always_at:?}");
    let mut s = bare(with_calib(|c| c.charge_multiplier_meaning = ChargeMultiplier::AccumulationRate));
    let id = s.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
    let rate_need = (need_permille() as i64 * 100 / ch.speed_multiplier_percent as i64) as i32;
    assert_eq!(charged_at(&mut s, id, n + 5), Some(run_up_ticks(rate_need, gain)), "accumulation_rate: the run-up fills at the multiplier's rate");
    s.tick();
    assert_eq!(s.entity(id).unwrap().effective_speed, base, "accumulation_rate: the speed never changes");

    // PROGRESS_ON_STOP, seen through a stun that does not reset (RESET_ON_STUN false).
    let stopped = |rule: ChargeStopRule| -> (i32, Option<u32>) {
        let mut s = bare(with_calib(|c| {
            c.charge_reset_on_stun = false;
            c.charge_progress_on_stop = rule;
        }));
        let id = s.scenario_spawn_now(Team::Blue, "Prince", lane_spot(), None).unwrap();
        for _ in 0..n / 2 {
            s.tick();
        }
        let pos = s.entity(id).unwrap().pos;
        s.spawn_unit(Team::Red, "Zap", pos, None).unwrap();
        s.tick();
        s.tick();
        let e = s.entity(id).unwrap();
        assert!(e.stun_ms > 0 && !e.charged, "vacuous: not held");
        (e.charge_progress, charged_at(&mut s, id, 2 * n))
    };
    let (held_progress, held_at) = stopped(ChargeStopRule::Hold);
    let (reset_progress, reset_at) = stopped(ChargeStopRule::Reset);
    assert!(held_progress > 0 && reset_progress == 0, "hold keeps the run-up through a stop, reset zeroes it: {held_progress} / {reset_progress}");
    assert!(held_at.unwrap() < reset_at.unwrap(), "{held_at:?} vs {reset_at:?}");

    // RESET_ON_ATTACK: false keeps the charge through every hit.
    let (hits, special, _) = tower_hits(with_calib(|c| c.charge_reset_on_attack = false), None);
    assert_eq!((hits[0].1, hits[1].1), (special, special), "RESET_ON_ATTACK = false: every hit is DamageSpecial ({hits:?})");

    // RESET_ON_STUN: a Zap on a CHARGED Prince.
    for (reset, want) in [(true, false), (false, true)] {
        let (_, charged, hit) = charged_prince_hit_by(with_calib(|c| c.charge_reset_on_stun = reset), "Zap", 0);
        assert!(hit, "vacuous: the Zap missed");
        assert_eq!(charged, want, "RESET_ON_STUN = {reset}: charged after the stun");
    }

    // RESET_ON_KNOCKBACK: The Log (PushbackAll) lands on a Prince despite IgnorePushback;
    // a Fireball (no PushbackAll) never moves it and so never resets it under either arm.
    for (reset, want) in [(true, false), (false, true)] {
        let (pushed, charged, hit) = charged_prince_hit_by(with_calib(|c| c.charge_reset_on_knockback = reset), "Log", 4);
        assert!(hit && pushed, "vacuous: the Log did not push the Prince (hit {hit}, pushed {pushed})");
        assert_eq!(charged, want, "RESET_ON_KNOCKBACK = {reset}: charged after a landed push");
        let (pushed, charged, hit) = charged_prince_hit_by(with_calib(|c| c.charge_reset_on_knockback = reset), "Fireball", 2);
        assert!(hit && !pushed && charged, "a Fireball on an IgnorePushback unit neither moves nor resets it (hit {hit}, pushed {pushed}, charged {charged})");
    }

    // RESET_ON_RETARGET.
    assert!(charged_after_retarget(config()), "RESET_ON_RETARGET = false (shipped): a switch keeps the charge");
    assert!(!charged_after_retarget(with_calib(|c| c.charge_reset_on_retarget = true)), "RESET_ON_RETARGET = true: a switch between live targets clears it");

    // SPECIAL_LEVEL_SCALING: at Epic local level 2 the two truncations part by one point.
    let db = s0.cards();
    let idx = db.index("Prince").unwrap();
    let level2 = db.rarity("Epic").unwrap().relative_level + 2;
    let plain = card_stat(&s0, "Prince").damage;
    let scaled_special = db.scaled(idx, level2, ch.damage_special).unwrap();
    let twice = (db.scaled(idx, level2, plain).unwrap() as i64 * ch.damage_special as i64 / plain as i64) as i32;
    assert_ne!(scaled_special, twice, "vacuous: the two scalings agree at local level 2");
    let (hits, _, _) = tower_hits(with_calib(|c| c.charge_special_level_scaling = ChargeLevelScaling::ScaleSpecialBase), Some(level2));
    assert_eq!(hits[0].1, scaled_special, "scale_special_base at level {level2}: {hits:?}");
    let (hits, _, _) = tower_hits(with_calib(|c| c.charge_special_level_scaling = ChargeLevelScaling::TwiceScaledDamage), Some(level2));
    assert_eq!(hits[0].1, twice, "twice_scaled_damage at level {level2}: {hits:?}");
}

// ---------------------------------------------------------------------------
// (10): the loader refuses a broken charge block

#[test]
fn the_loader_refuses_a_partial_or_non_positive_charge_block_and_a_charging_building() {
    let s = bare(config());
    let ch = charge_of(&s, "Prince");
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let raw = &doc["cards"].as_array().unwrap().iter().find(|c| c["name"] == "Prince").unwrap()["charge"];
    assert_eq!(ch.range_raw as i64, raw["charge_range_raw"].as_i64().unwrap());
    assert_eq!(ch.damage_special as i64, raw["damage_special"].as_i64().unwrap());
    assert_eq!(ch.speed_multiplier_percent as i64, raw["charge_speed_multiplier_percent"].as_i64().unwrap());
    // Each column missing, zero and negative: rejected, the column named.
    for (field, column) in [("charge_range_raw", "ChargeRange"), ("damage_special", "DamageSpecial"), ("charge_speed_multiplier_percent", "ChargeSpeedMultiplier")] {
        for value in [serde_json::Value::Null, serde_json::Value::from(0), serde_json::Value::from(-250)] {
            let mut d = doc.clone();
            d["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "Prince").unwrap()["charge"][field] = value.clone();
            let db = CardDb::from_json_str(&d.to_string(), CardSource::DerivedJson).unwrap();
            assert!(db.index("Prince").is_none(), "{field} = {value}: a Prince with a broken charge block loaded as a card");
            let (_, why) = db.rejected.iter().find(|(n, _)| n == "Prince").unwrap_or_else(|| panic!("{field} = {value}: Prince neither loaded nor rejected"));
            assert!(why.contains("charge") && why.contains(column), "{field} = {value}: rejected for the wrong reason: {why}");
        }
    }
    // A building carrying a charge block is refused: only a troop charges.
    let mut d = doc.clone();
    d["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "Cannon").unwrap()["charge"] = raw.clone();
    let db = CardDb::from_json_str(&d.to_string(), CardSource::DerivedJson).unwrap();
    assert!(db.index("Cannon").is_none(), "a Cannon with a charge block loaded as a card");
    let (_, why) = db.rejected.iter().find(|(n, _)| n == "Cannon").unwrap();
    assert!(why.contains("only a troop charges"), "{why}");
    // And the Prince, untouched, loads with the block whole.
    let db = CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).unwrap();
    assert_eq!(db.get(db.index("Prince").unwrap()).charge, Some(ch));
}

// ---------------------------------------------------------------------------
// (11): the Battle Ram's Kamikaze is a known gap

#[test]
fn the_battle_ram_survives_its_charged_hit_and_keeps_hitting_because_kamikaze_is_not_read() {
    // The 2018 and 15.535 rows ship `Kamikaze = true` (the Ram dies on its hit and
    // its two Barbarians take over); the extractor does not carry the column and
    // nothing in the crate reads it, so the Ram re-arms every HitSpeed and hits at
    // Damage. THIS TEST PINS THE GAP: a Kamikaze implementation must flip it (the
    // Ram gone after the first landed hit, the Barbarians on the board).
    let mut s = bare(config());
    let ch = charge_of(&s, "BattleRam");
    let c = card_stat(&s, "BattleRam").clone();
    let level = s.config().card_level[0];
    let db = s.cards();
    let idx = db.index("BattleRam").unwrap();
    let special = db.scaled(idx, level, ch.damage_special).unwrap();
    let plain = db.scaled(idx, level, c.damage).unwrap();
    let pct = c.crown_tower_damage_percent;
    let rounding = s.config().calib.crown_rounding;
    let want = |d: i32| damage_against(EntityKind::PrincessTower, d, pct, rounding);
    let ram = s.scenario_spawn_now(Team::Blue, "BattleRam", lane_spot(), None).unwrap();
    let hits = first_drops(&mut s, 3, 800, |s| s.tower_hp(Team::Red)[1]);
    assert_eq!(hits.len(), 3, "vacuous: the Ram did not land three hits: {hits:?}");
    assert_eq!(hits[0].1, want(special), "the first landed hit is DamageSpecial ({hits:?})");
    assert_eq!((hits[1].1, hits[2].1), (want(plain), want(plain)), "the Ram keeps hitting at Damage ({hits:?})");
    assert!(s.entity(ram).is_some(), "KAMIKAZE LANDED: the Ram died on its hit -- update this test and the mechanics.md gap row");
    assert!(find_live(&s, Team::Blue, "Barbarian").is_empty(), "the Ram's death spawn appeared without a death");
}
