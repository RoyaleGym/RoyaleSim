//! STATUS EFFECTS: the buff a unit carries, what carrying it does to
//! its walk, its stomp clock and its attack cycle, and how a second copy of the same
//! buff composes -- status.rs, state.rs `buffed_speed` / `stomp_advance` /
//! `buff_pulse_pass` / `apply_effects`, combat.rs `attack_step_progress` / `fire`,
//! spell.rs `SpellMotion::Pulsing`; calibration movement.BUFF_SPEED_COMPOSITION,
//! movement.STOMP_PAUSE_SCHEDULE, combat.HIT_SPEED_BUFF, status.FULL_STOP_BUFF_IS_STUN,
//! status.BUFF_PULSE_AMOUNT, status.BUFF_PULSE_TIMING, status.TARGET_BUFF_ON_SPLASH,
//! spells.PULSING_AREA_EFFECT.
//!
//! THE LAW (the calibration keys carry the evidence and the alternatives; status.rs
//! the arithmetic):
//!   A. COMPOSITION (one loop, run on SpeedMultiplier and again on
//!      HitSpeedMultiplier): only the strongest positive and the strongest negative
//!      of the buffs a unit carries count, applied in that order, each a TRUNCATING
//!      percent -- `tdiv(max(0, min(100, 100 - maxneg)) * tdiv(maxpos * v, 100), 100)`.
//!      Two Rages are one Rage; a zero column is skipped, not read as 100.
//!   B. THE HOLD: Freeze ships -100 in all three multiplier columns, so its composed
//!      speed is 0 and the unit is HELD -- the engine's one stun path
//!      (status.FULL_STOP_BUFF_IS_STUN), the same one a Zap takes.
//!   C. THE STOMP CLOCK: a millisecond clock whose
//!      advance is `tdiv(compose(Speed, 100), 2)` per WALKING tick -- 50 unbuffed,
//!      65 under Rage -- against a fixed (Stop, Stop + Wait) window.
//!   D. THE ATTACK: the progress counter gains
//!      `compose(HitSpeed, TICK_MS)` per tick and the whole step is skipped when that
//!      is <= 0, while the LOAD timer always loses a flat TICK_MS.
//!
//! WHAT IS PINNED, every number read from the loaded CardDb, its buff table or
//! `Calib::shipped()` -- never pasted:
//!   1. the composition arithmetic on EVERY troop card's own Speed, under a Rage,
//!      under a slow and under both, against the law recomputed here;
//!   2. a Freeze is a whole-unit hold of exactly ceil(BuffTime / TICK_MS) ticks --
//!      no walk, no attack progress, no stomp-clock advance -- and the unit resumes;
//!   3. the Ice Spirit's projectile hands its victim that same Freeze (BuffTime
//!      1100 = 22 ticks), and the Ice Wizard's hands it IceWizardSlowDown, which
//!      scales the victim's walk and its attack advance by the composed percent;
//!   4. a stacking buff takes a SECOND slot and a non-stacking one refreshes the
//!      first by max (status.SAME_BUFF_REAPPLY);
//!   5. a Poison pulses `DamagePerSecond * HitFrequency / 1000` every HitFrequency
//!      while its area stands, with the buff's own crown-tower percent;
//!   6. the expiry alignment is the stun's (status.BUFF_EXPIRY_TICK_ALIGNMENT);
//!   7. a buff on Blue and its mirror on Red do the same thing (seat symmetry);
//!   8. every candidate of the switchable keys moves a measurable behaviour, and a
//!      candidate with no engine implementation is refused at load;
//!   9. an attack buff lands on the SPLASH VICTIMS and on nobody else -- not on the
//!      attacker's own team, not on a unit the disc missed;
//!  10. a buff's BuildingDamagePercent and CrownTowerDamagePercent are both simulated
//!      (an Earthquake pulse on a Cannon, a Poison pulse on a princess tower);
//!  11. a HELD unit's attack progress stands still while its LOAD timer keeps
//!      counting down (combat.ATTACK_CYCLE's boundary note).
//!
//! PLANTS (regression):
//!   * `buff_speed_unfloored` rounds the composition instead of truncating it: (1)
//!     and (3) go red.
//!   * `buff_on_raw_neighbours` gives `apply_attack_buff` back the RAW neighbour
//!     query `splash` started from, the earlier bug: (9) goes red.
//!     RUSTFLAGS='--cfg clash_plant="buff_speed_unfloored"' CARGO_TARGET_DIR=target/plant cargo test --test status

mod common;

use common::*;
use royalesim::card::{CardDef, CardKind, SpellShape};
use royalesim::entity::{AttackPhase, EntityKind};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE};
use royalesim::state::{
    BattleConfig, BattleState, BuffComposition, BuffExpiry, BuffReapply, Calib, FullStopBuff, HitSpeedBuff, PulseAmount, PulseTiming, PulsingArea,
    StompSchedule, TargetBuffScope,
};
use royalesim::status::{compose, BuffDef, Sel, MAX_BUFFS_PER_ENTITY};
use royalesim::{EntityId, Team};

fn calib() -> Calib {
    Calib::shipped()
}

fn bare(cfg: BattleConfig) -> BattleState {
    BattleState::new(5, cfg)
}

fn with_calib(f: impl FnOnce(&mut Calib)) -> BattleConfig {
    let mut cfg = config();
    f(&mut cfg.calib);
    cfg
}

/// The shipped arms this file pins (asserted, so a registry change re-points the
/// file instead of silently moving every number).
fn assert_shipped_arms() {
    let c = calib();
    assert_eq!(c.buff_speed_composition, BuffComposition::StrongestUpAndDown, "the shipped arm this file pins");
    assert_eq!(c.hit_speed_buff, HitSpeedBuff::ProgressScaled, "the shipped arm this file pins");
    assert_eq!(c.full_stop_buff_is_stun, FullStopBuff::StunTimer, "the shipped arm this file pins");
    assert_eq!(c.stomp_schedule, StompSchedule::MsClock, "the shipped arm this file pins");
    assert_eq!(c.buff_pulse_amount, PulseAmount::PerSecondTimesFrequency, "the shipped arm this file pins");
    assert_eq!(c.buff_pulse_timing, PulseTiming::AfterFirstPeriod, "the shipped arm this file pins");
    assert_eq!(c.target_buff_on_splash, TargetBuffScope::WholeSplash, "the shipped arm this file pins");
    assert_eq!(c.pulsing_area_effect, PulsingArea::FromLanding, "the shipped arm this file pins");
    assert_eq!(c.buff_expiry, BuffExpiry::CeilFromNextTick, "the shipped alignment this file pins");
    assert_eq!(c.same_buff_reapply, BuffReapply::RefreshMax, "the shipped reapply rule this file pins");
}

/// ceil(a / b) for positive b.
fn ceil_div(a: i32, b: i32) -> i32 {
    (a + b - 1) / b
}

/// A spot on Blue's half clear of every tower footprint and the river.
fn spot() -> Vec2 {
    t(900, 1000)
}

/// THE BUFF a card's attack hangs on its victim, from the loaded CardDb: the def and
/// its BuffTime.
fn attack_buff_of(s: &BattleState, card: &str) -> (BuffDef, i32) {
    let c = card_stat(s, card);
    let b = c.attack_buff.unwrap_or_else(|| panic!("cards.json {card} carries no attack buff"));
    (s.cards().buffs[b.buff as usize], b.time_ms)
}

/// THE BUFF a spell's impact hangs on its victims, from the loaded CardDb.
fn spell_buff_of(s: &BattleState, card: &str) -> (BuffDef, i32) {
    let def = card_stat(s, card).spell.as_ref().unwrap_or_else(|| panic!("{card} is not a spell"));
    let hit = match &def.shape {
        SpellShape::AreaEffect { hit } => hit,
        SpellShape::PulsingAreaEffect { hit, .. } => hit,
        SpellShape::Projectile { hit: Some(hit), .. } => hit,
        other => panic!("{card}: {other:?} carries no buff"),
    };
    let b = hit.buff.unwrap_or_else(|| panic!("cards.json {card} carries no buff"));
    (s.cards().buffs[b.buff as usize], b.time_ms)
}

/// Every troop of the loaded CardDb that walks: its name, its raw Speed column and
/// its STOMPED speed (movement.STOMP_SPEED_RULE), derived from the card, not pasted.
/// Read off the CardDb directly, because a summon-only row or a card whose spell is
/// refused is still a walker with a Speed column and is not reachable by name.
fn walkers(s: &BattleState) -> Vec<(String, i32, i32)> {
    s.cards()
        .cards
        .iter()
        .filter(|c: &&CardDef| c.kind == CardKind::Troop && c.speed > 0)
        .map(|c| {
            let stomped = if c.stop_movement_after_ms > 0 { c.speed * (c.stop_movement_after_ms + c.wait_ms) / c.stop_movement_after_ms } else { c.speed };
            (c.name.clone(), c.speed, stomped)
        })
        .collect()
}

/// THE LAW, restated from the calibration key so the test can fail if the engine's
/// copy changes (status.rs `compose` is the engine's).
fn want_compose(multipliers: &[i32], value: i32) -> i32 {
    let mut maxpos = 100i64;
    let mut maxneg = 0i64;
    for &m in multipliers {
        if m > 0 {
            maxpos = maxpos.max(m as i64);
        } else if m < 0 {
            maxneg = maxneg.max(-m as i64);
        }
    }
    let r = maxpos * value as i64 / 100;
    ((100 - maxneg).clamp(0, 100) * r / 100) as i32
}

// ---------------------------------------------------------------- 1. the law

#[test]
fn the_composition_law_holds_on_every_walkers_speed() {
    assert_shipped_arms();
    let s = bare(config());
    let rage = 130;
    let slow = -30;
    let mut checked = 0;
    for (name, speed, _) in walkers(&s) {
        for set in [vec![rage], vec![slow], vec![rage, slow], vec![rage, rage], vec![slow, slow, rage]] {
            let defs: Vec<BuffDef> = set.iter().map(|m| BuffDef { speed_pct: *m, ..BuffDef::default() }).collect();
            let got = compose(defs.iter(), Sel::Speed, speed);
            assert_eq!(got, want_compose(&set, speed), "{name} (Speed {speed}) under {set:?}");
            checked += 1;
        }
        // ONLY THE STRONGEST OF EACH SIGN: a second, weaker buff of the same sign
        // changes nothing at all.
        let one = [BuffDef { speed_pct: rage, ..BuffDef::default() }];
        let two = [one[0], BuffDef { speed_pct: 110, ..BuffDef::default() }];
        assert_eq!(compose(one.iter(), Sel::Speed, speed), compose(two.iter(), Sel::Speed, speed), "{name}: a weaker Rage adds nothing");
        // A ZERO COLUMN IS SKIPPED, not read as 100.
        let zero = [BuffDef { hit_speed_pct: -100, ..BuffDef::default() }];
        assert_eq!(compose(zero.iter(), Sel::Speed, speed), speed, "{name}: a blank SpeedMultiplier does not touch the speed");
    }
    assert!(checked >= 50, "only {checked} cases; the card file looks empty");
}

#[test]
fn the_composition_truncates_where_rounding_would_differ() {
    // movement.BUFF_SPEED_RULE's evidence is the raged Ice Golem (stomped 52 -> 67,
    // where round and ceil both give 68); that card is a loader refusal (its death
    // area effect), so the property is pinned on every walker the file DOES load,
    // and at least one of them must separate truncation from rounding or the test
    // would pass under either.
    assert_shipped_arms();
    let s = bare(config());
    let rage = 130;
    let mut separators = 0;
    for (name, _speed, stomped) in walkers(&s) {
        let got = compose([BuffDef { speed_pct: rage, ..BuffDef::default() }].iter(), Sel::Speed, stomped);
        let exact = stomped * rage;
        assert_eq!(got, exact / 100, "{name}: the composition truncates");
        if exact % 100 != 0 {
            separators += 1;
            assert!(got < (exact + 50) / 100 || got < (exact + 99) / 100, "{name}: {got} is below the rounded {}", (exact + 50) / 100);
        }
    }
    assert!(separators > 0, "no loaded walker separates truncation from rounding under a Rage");
}

// ------------------------------------------------- 2. the hold, and who holds

/// Deploy `card` for Blue at `p`, run it past its deploy timer, and return its id.
fn placed(s: &mut BattleState, team: Team, card: &str, p: Vec2) -> EntityId {
    s.scenario_spawn_now(team, card, p, None).unwrap_or_else(|e| panic!("{card} at {p:?}: {e:?}"))
}

/// The position of `id` now.
fn pos(s: &BattleState, id: EntityId) -> Vec2 {
    s.entity(id).unwrap_or_else(|| panic!("{id:?} is gone")).pos
}

#[test]
fn a_freeze_is_a_whole_unit_hold_of_its_bufftime() {
    assert_shipped_arms();
    let mut s = bare(config());
    let knight = placed(&mut s, Team::Blue, "Knight", spot());
    // Let it get walking, so a hold is visible as a standstill.
    for _ in 0..30 {
        s.tick();
    }
    let before = pos(&s, knight);
    s.tick();
    assert_ne!(pos(&s, knight), before, "the Knight was not walking; the scene proves nothing");

    let (def, time_ms) = spell_buff_of(&s, "Freeze");
    assert_eq!(compose([def].iter(), Sel::Speed, 100), 0, "Freeze composes to a full stop");
    s.spawn_unit(Team::Red, "Freeze", pos(&s, knight), None).expect("cast Freeze");
    // Run to the impact: the area effect applies on its first update.
    let mut landed = None;
    for k in 0..40 {
        s.tick();
        if s.entity(knight).map(|v| v.stun_ms > 0).unwrap_or(false) {
            landed = Some(k);
            break;
        }
    }
    let landed = landed.expect("the Freeze never reached the Knight");
    let v = s.entity(knight).unwrap();
    assert_eq!(v.stun_ms, time_ms, "the hold timer is the buff's BuffTime");
    assert!(v.buffs.iter().any(|b| !b.is_empty() && b.ms == time_ms), "the buff is on the unit too, not only the timer");
    let clock_at_freeze = v.stomp_clock;
    let held_from = pos(&s, knight);
    let want_ticks = ceil_div(time_ms, calib().tick_ms);
    for k in 0..want_ticks {
        s.tick();
        let v = s.entity(knight).unwrap();
        assert_eq!(v.pos, held_from, "tick {k} of the hold: the Knight moved");
        assert_eq!(v.stomp_clock, clock_at_freeze, "tick {k} of the hold: the stomp clock advanced");
        assert_eq!(v.attack_phase, AttackPhase::Idle, "tick {k} of the hold: it was attacking");
    }
    assert_eq!(s.entity(knight).unwrap().stun_ms, 0, "the hold is over after ceil(BuffTime / TICK_MS) ticks");
    s.tick();
    assert_ne!(pos(&s, knight), held_from, "the Knight never resumed (landed at call {landed})");
}

#[test]
fn an_ice_spirit_hands_its_victim_the_same_freeze() {
    assert_shipped_arms();
    let (probe, _) = {
        let s = bare(config());
        attack_buff_of(&s, "IceSpirits")
    };
    assert_eq!(compose([probe].iter(), Sel::Speed, 100), 0, "the Ice Spirit's TargetBuff is a full stop");

    let mut s = bare(config());
    let (def, time_ms) = attack_buff_of(&s, "IceSpirits");
    assert_eq!(def, probe);
    let knight = placed(&mut s, Team::Blue, "Knight", spot());
    let kp = pos(&s, knight);
    // Two tiles up the board, so the Ice Spirit walks down onto the Knight.
    let spirit_at = Vec2::new(kp.x, kp.y + 2 * SUBTILE_PER_MILLITILE * 1000);
    let spirit = placed(&mut s, Team::Red, "IceSpirits", spirit_at);
    let _ = spirit;
    let mut held = None;
    for k in 0..200 {
        s.tick();
        let Some(v) = s.entity(knight) else { break };
        if v.stun_ms > 0 {
            held = Some((k, v.stun_ms));
            break;
        }
    }
    let (_, ms) = held.expect("the Ice Spirit never froze the Knight");
    assert_eq!(ms, time_ms, "the hold is the projectile's BuffTime");
    assert_eq!(ceil_div(time_ms, calib().tick_ms), 22, "the live 16.402 Ice Spirit hold, in ticks");
}

// ------------------------------------------------------- 3. a slow, not a stop

#[test]
fn a_slow_scales_the_walk_and_the_attack_advance() {
    assert_shipped_arms();
    let s0 = bare(config());
    let (slow, _) = attack_buff_of(&s0, "IceWizard");
    assert!(slow.speed_pct < 0 && slow.hit_speed_pct < 0, "IceWizardSlowDown slows both columns");
    assert_ne!(compose([slow].iter(), Sel::Speed, 100), 0, "a slow is not a hold");

    // The walk: a Knight's stored speed through the composition.
    let knight = card_stat(&s0, "Knight");
    let want = compose([slow].iter(), Sel::Speed, knight.speed);
    assert!(want < knight.speed && want > 0, "the slow moves the Knight's speed without stopping it");

    // The attack: the progress advance is the same composition on TICK_MS.
    let dt = calib().tick_ms;
    let want_adv = compose([slow].iter(), Sel::HitSpeed, dt);
    assert!(want_adv < dt && want_adv > 0, "a slowed attack advances, slower");

    // And a Rage on top composes the law's way: the speed-up first, then the slow.
    let rage = BuffDef { speed_pct: 130, hit_speed_pct: 130, ..BuffDef::default() };
    let both = compose([rage, slow].iter(), Sel::Speed, knight.speed);
    assert_eq!(both, want_compose(&[130, slow.speed_pct], knight.speed));
    assert_ne!(both, knight.speed * (100 + 130 - 100 + slow.speed_pct) / 100, "the two percentages are NOT added");
}

#[test]
fn a_slowed_walker_actually_walks_slower_in_the_battle() {
    assert_shipped_arms();
    let mut s = bare(config());
    let (slow, _) = attack_buff_of(&s, "IceWizard");
    let giant = placed(&mut s, Team::Blue, "Giant", spot());
    for _ in 0..40 {
        s.tick();
    }
    let free = s.entity(giant).unwrap().speed_now;
    assert!(free > 0, "the Giant has no speed; the scene proves nothing");
    // Hand it the slow through the engine's own application path (a Snowball, whose
    // TargetBuff is the same row).
    let (sb, _) = spell_buff_of(&s, "Snowball");
    assert_eq!(sb, slow, "the Snowball and the Ice Wizard ship the same slow row");
    s.spawn_unit(Team::Red, "Snowball", pos(&s, giant), None).expect("cast Snowball");
    let mut slowed = None;
    for _ in 0..60 {
        s.tick();
        let v = s.entity(giant).unwrap();
        if v.buffs.iter().any(|b| !b.is_empty()) {
            slowed = Some(v.speed_now);
            break;
        }
    }
    let slowed = slowed.expect("the Snowball never slowed the Giant");
    assert!(slowed < free, "the slowed Giant walks at {slowed}, the free one at {free}");
    let spt = calib().speed_to_subtiles_per_tick;
    assert_eq!(slowed, compose([slow].iter(), Sel::Speed, free / spt) * spt, "the slowed speed is the composition of the free one");
}

// --------------------------------------------- 4. one slot per buff row

#[test]
fn a_second_application_refreshes_the_slot_it_never_adds_one() {
    assert_shipped_arms();
    let s = bare(config());
    // The data DOES distinguish the two rows -- EnableStacking is on the Poison and
    // not on the Freeze -- and the engine carries the column without acting on it
    // (status.BUFF_STACKING = one_slot_per_buff_row, a named gap).
    let (poison, _) = spell_buff_of(&s, "Poison");
    assert!(poison.enable_stacking, "the Poison row ships EnableStacking");
    let (freeze, _) = spell_buff_of(&s, "Freeze");
    assert!(!freeze.enable_stacking, "the Freeze row does not");

    // Two Freezes on one Knight: one slot, the longer time (status.SAME_BUFF_REAPPLY
    // = refresh_max).
    let mut s = bare(config());
    let knight = placed(&mut s, Team::Blue, "Knight", spot());
    s.spawn_unit(Team::Red, "Freeze", pos(&s, knight), None).expect("cast Freeze");
    for _ in 0..40 {
        s.tick();
        if s.entity(knight).unwrap().stun_ms > 0 {
            break;
        }
    }
    let first = s.entity(knight).unwrap();
    let slots = first.buffs.iter().filter(|b| !b.is_empty()).count();
    assert_eq!(slots, 1, "one Freeze, one slot");
    let ms_before = first.buffs.iter().find(|b| !b.is_empty()).unwrap().ms;
    s.tick();
    s.tick();
    let mid = s.entity(knight).unwrap().buffs.iter().find(|b| !b.is_empty()).unwrap().ms;
    assert!(mid < ms_before, "the slot is counting down");
    s.spawn_unit(Team::Red, "Freeze", pos(&s, knight), None).expect("cast a second Freeze");
    for _ in 0..40 {
        s.tick();
        let v = s.entity(knight).unwrap();
        if v.buffs.iter().find(|b| !b.is_empty()).map(|b| b.ms).unwrap_or(0) > mid {
            break;
        }
    }
    let after = s.entity(knight).unwrap();
    assert_eq!(after.buffs.iter().filter(|b| !b.is_empty()).count(), 1, "a non-stacking buff never takes a second slot");
    assert!(after.buffs.iter().find(|b| !b.is_empty()).unwrap().ms > mid, "the second Freeze refreshed the first");
    const { assert!(MAX_BUFFS_PER_ENTITY >= 2, "the column has room for two DIFFERENT rows") };
}

#[test]
fn a_pulsing_area_does_not_stack_with_itself() {
    // The gap status.BUFF_STACKING names, stated as a test: a Poison area re-applies
    // its buff every HitSpeed for its whole LifeDuration, and a per-application slot
    // would put LifeDuration / HitSpeed copies on one unit.
    assert_shipped_arms();
    let mut s = bare(config());
    let knight = placed(&mut s, Team::Blue, "Knight", spot());
    s.spawn_unit(Team::Red, "Poison", pos(&s, knight), None).expect("cast Poison");
    let mut seen = 0;
    for _ in 0..200 {
        s.tick();
        let Some(v) = s.entity(knight) else { break };
        let on = v.buffs.iter().filter(|b| !b.is_empty()).count();
        assert!(on <= 1, "{on} Poison slots on one Knight: the cloud stacked with itself");
        seen = seen.max(on);
    }
    assert_eq!(seen, 1, "the Poison never reached the Knight; the scene proves nothing");
}

// --------------------------------------------------------- 5. the DoT pulses

#[test]
fn a_poison_pulses_its_per_second_damage_once_a_second() {
    assert_shipped_arms();
    let mut s = bare(config());
    let (poison, buff_time) = spell_buff_of(&s, "Poison");
    assert!(poison.damage_per_second > 0 && poison.hit_frequency_ms > 0, "the Poison row pulses");
    let want_pulse = poison.damage_per_second * poison.hit_frequency_ms / 1000;
    assert_eq!(poison.pulse_base(), want_pulse, "the engine's per-pulse figure is dps x HitFrequency / 1000");

    let (life_ms, hit_speed_ms) = match &card_stat(&s, "Poison").spell.as_ref().unwrap().shape {
        SpellShape::PulsingAreaEffect { life_ms, hit_speed_ms, .. } => (*life_ms, *hit_speed_ms),
        other => panic!("Poison loaded as {other:?}"),
    };
    assert!(hit_speed_ms > 0 && hit_speed_ms < buff_time, "the area refreshes its buff faster than the buff expires");

    let knight = placed(&mut s, Team::Blue, "Knight", spot());
    // A CONTROL BATTLE without the cast, so the Poison's damage is the difference and
    // nothing else the Knight met on the way is counted (the technique of spells.rs).
    let mut control = s.clone();
    s.spawn_unit(Team::Red, "Poison", pos(&s, knight), None).expect("cast Poison");
    // Run the whole cloud out, plus the buff's tail. The Knight WALKS, so it leaves
    // the cloud partway through and the pulse count is bounded, not fixed.
    let ticks = (life_ms + buff_time) / calib().tick_ms + 4;
    let mut drops = 0;
    let mut last = 0;
    for _ in 0..ticks {
        s.tick();
        control.tick();
        let (Some(v), Some(c)) = (s.entity(knight), control.entity(knight)) else { break };
        let lost = c.hp - v.hp;
        if lost > last {
            drops += 1;
            last = lost;
        }
    }
    assert!(drops > 0, "the Poison never damaged the Knight");
    let total = last;
    // EVERY DROP IS ONE WHOLE PULSE, level-scaled by the caster
    // (status.BUFF_PULSE_AMOUNT = per_second_times_frequency).
    let scaled = s.cards().scaled(s.cards().index("Poison").expect("Poison loads"), config().card_level[Team::Red as usize], want_pulse).expect("level validated");
    assert_eq!(total, drops * scaled, "{drops} drops totalling {total}, one pulse is {scaled}");
    // status.BUFF_STACKING = one_slot_per_buff_row: ONE Poison on the unit however
    // often its area re-applies, so the count is bounded by the cloud's life over the
    // BUFF's HitFrequency -- not by that times LifeDuration / the AREA's HitSpeed,
    // which is what a self-stacking cloud would give.
    let ceiling = (life_ms + buff_time) / poison.hit_frequency_ms + 1;
    assert!(drops <= ceiling, "{drops} pulses over a {life_ms} ms cloud at {} ms: at most {ceiling}", poison.hit_frequency_ms);
    assert!(drops * poison.hit_frequency_ms * 4 >= life_ms, "{drops} pulses is too few for a Knight that stood in the cloud at all");
}

/// A pulsing spell cast at `at`, run against a CONTROL copy of the same battle, and
/// the SIZE OF ONE hp drop the cast caused on `victim`. `None` when the cast never
/// bit. Buildings drain under lifetime.HP_DECAY and towers take other damage, so only
/// the difference against the control is this spell's, and only the STEP between two
/// differences is one pulse.
fn one_pulse_drop(mut s: BattleState, spell: &str, at: Vec2, victim: EntityId, ticks: u32) -> Option<i32> {
    let mut control = s.clone();
    s.spawn_unit(Team::Red, spell, at, None).unwrap_or_else(|e| panic!("cast {spell}: {e:?}"));
    let mut last = 0;
    let mut step = None;
    for _ in 0..ticks {
        s.tick();
        control.tick();
        let (Some(v), Some(c)) = (s.entity(victim), control.entity(victim)) else { break };
        let lost = c.hp - v.hp;
        if lost > last {
            let d = lost - last;
            // Every drop must be the SAME size: a pulse is one whole amount.
            if let Some(prev) = step {
                assert_eq!(d, prev, "{spell} dealt {d} after dealing {prev}: the pulses are not one amount");
            }
            step = Some(d);
            last = lost;
        }
    }
    step
}

/// The level-scaled amount of ONE pulse of `spell`'s buff, from the loaded CardDb.
fn scaled_pulse(s: &BattleState, spell: &str) -> i32 {
    let (def, _) = spell_buff_of(s, spell);
    let idx = s.cards().index(spell).unwrap_or_else(|| panic!("{spell} loads"));
    s.cards().scaled(idx, config().card_level[Team::Red as usize], def.pulse_base()).expect("level validated")
}

#[test]
fn a_poison_leaves_a_crown_tower_its_own_percent() {
    assert_shipped_arms();
    let s = bare(config());
    // THE COLUMNS, as a data guard: what the rest of this test simulates against.
    let (poison, _) = spell_buff_of(&s, "Poison");
    assert!(poison.crown_pct < 100, "the Poison row reduces its crown-tower damage");
    assert!(poison.building_pct == 100, "the Poison row ships no BuildingDamagePercent");

    // AND THE BEHAVIOUR. ~~the column assertions alone~~ -- they never ran a tick, so
    // they could not see `buff_pulse_pass` dropping the percent it had just computed
    // (a review finding).
    let s = bare(config());
    let (tower, tp) = s
        .entities()
        .find(|v| v.team == Team::Blue && v.kind == EntityKind::PrincessTower)
        .map(|v| (v.id, v.pos))
        .expect("the arena ships crown towers");
    let full = scaled_pulse(&s, "Poison");
    assert!(full > 0, "the Poison's pulse is level-scaled to something");
    let want = royalesim::combat::damage_against(EntityKind::PrincessTower, full, poison.crown_pct, calib().crown_rounding);
    assert!(want < full, "the scene proves nothing unless the percent changes the number ({want} of {full})");
    let got = one_pulse_drop(s, "Poison", tp, tower, 400);
    assert_eq!(got, Some(want), "one Poison pulse on a princess tower: {full} through the row's {} %", poison.crown_pct);
}

#[test]
fn an_earthquake_deals_a_building_its_own_percent() {
    assert_shipped_arms();
    let s0 = bare(config());
    let (eq, _) = spell_buff_of(&s0, "Earthquake");
    assert!(eq.building_pct > 100, "the Earthquake row multiplies its BUILDING damage");

    let mut s = bare(config());
    let cannon = placed(&mut s, Team::Blue, "Cannon", spot());
    assert_eq!(s.entity(cannon).unwrap().kind, EntityKind::Building, "the witness must be an ordinary building, not a crown tower");
    let at = pos(&s, cannon);
    let full = scaled_pulse(&s, "Earthquake");
    // The building percent is a plain truncating scale (state.rs `buff_pulse_pass`);
    // `damage_against` is the CROWN reduction and returns `full` untouched here, which
    // is exactly the bug this pins.
    let want = full * eq.building_pct / 100;
    assert!(want > full, "the scene proves nothing unless the percent changes the number ({want} of {full})");
    assert_eq!(
        royalesim::combat::damage_against(EntityKind::Building, full, eq.building_pct, calib().crown_rounding),
        full,
        "damage_against must stay the crown-tower reduction: a building's percent cannot ride it"
    );
    let got = one_pulse_drop(s, "Earthquake", at, cannon, 400);
    assert_eq!(got, Some(want), "one Earthquake pulse on a Cannon: {full} through the row's {} %", eq.building_pct);
}

#[test]
fn an_attack_buff_lands_on_the_splash_victims_only() {
    assert_shipped_arms();
    let mut s = bare(config());
    let splash_r = card_stat(&s, "IceWizard").area_damage_radius;
    assert!(splash_r > 0, "the Ice Wizard's shot splashes; without that this scene tests nothing");

    // The victim, a FRIEND OF THE ATTACKER standing on top of it, and a second enemy
    // far enough out that the splash disc misses it. THE FAR ONE IS SIZED FROM THE
    // BOARD: the splash queries `splash_r + the largest radius alive` around the
    // impact and then tests `splash_r + the VICTIM's own radius`, so the window that
    // proves anything is between those two, and the witness goes in the middle of it.
    let victim = placed(&mut s, Team::Blue, "Knight", spot());
    let vp = pos(&s, victim);
    let own_r = s.entity(victim).expect("the victim is alive").radius;
    let board_r = s.entities().map(|v| v.radius).max().expect("the arena is not empty");
    assert!(board_r > own_r, "every radius on the board is the Knight's: the far witness has nowhere to stand");
    let out_of_disc = splash_r + (own_r + board_r) / 2;
    let friend = placed(&mut s, Team::Red, "Knight", Vec2::new(vp.x + splash_r / 2, vp.y));
    let far = placed(&mut s, Team::Blue, "Knight", Vec2::new(vp.x + out_of_disc, vp.y));
    let wizard = placed(&mut s, Team::Red, "IceWizard", Vec2::new(vp.x, vp.y + 3 * SUBTILE_PER_MILLITILE * 1000));
    let _ = wizard;

    let mut landed = None;
    for k in 0..300 {
        s.tick();
        let Some(v) = s.entity(victim) else { break };
        if v.buffs.iter().any(|b| !b.is_empty()) {
            landed = Some(k);
            break;
        }
    }
    landed.expect("the Ice Wizard never hit the Knight; the scene proves nothing");

    // THE QUERY the splash runs is `splash_r + the largest radius on the board` around
    // the impact, so both witnesses have to be inside THAT and outside the disc --
    // otherwise the earlier code would not have reached them either and this
    // test would pass for the wrong reason.
    let max_radius = s.entities().map(|v| v.radius).max().expect("the arena is not empty");
    let centre = pos(&s, victim);
    let dist2 = |id: EntityId| pos(&s, id).sub(centre).len2();
    let sq = |r: i32| (r as i64) * (r as i64);
    for (id, what) in [(friend, "the attacker's own team-mate"), (far, "an enemy outside the splash disc")] {
        assert!(dist2(id) <= sq(splash_r + max_radius), "{what} is outside the raw neighbour query: the scene proves nothing");
        let v = s.entity(id).expect("witness alive");
        assert!(v.buffs.iter().all(|b| b.is_empty()), "{what} took the Ice Wizard's slow");
        assert_eq!(v.stun_ms, 0, "{what} was held by the Ice Wizard's slow");
    }
    assert!(dist2(far) > sq(splash_r + s.entity(far).unwrap().radius), "the far Knight is inside the splash disc: the scene proves nothing");
    assert!(s.entity(victim).unwrap().buffs.iter().any(|b| !b.is_empty()), "the victim lost its buff before the witnesses were read");
}

#[test]
fn a_held_unit_still_runs_its_load_timer_down() {
    assert_shipped_arms();
    // The load timer counts down FIRST THING in the attack step, unconditionally;
    // only the PROGRESS step is skipped when the composed advance is <= 0
    // (combat.ATTACK_CYCLE's boundary note). An earlier version held everything.
    let mut s = bare(config());
    let knight = placed(&mut s, Team::Blue, "Knight", spot());
    let vp = pos(&s, knight);
    let foe = placed(&mut s, Team::Red, "Knight", Vec2::new(vp.x, vp.y + SUBTILE_PER_MILLITILE * 1100));
    let _ = foe;
    let load_time = card_stat(&s, "Knight").load_time_ms;
    assert!(load_time > calib().tick_ms, "a Knight's LoadTime must outlast one tick for this scene to show anything");

    // Run to just after a hit, where the load timer is full.
    let mut armed = false;
    for _ in 0..400 {
        s.tick();
        if s.entity(knight).map(|v| v.attack_load_ms == load_time).unwrap_or(false) {
            armed = true;
            break;
        }
    }
    assert!(armed, "the Knight never landed a hit; the scene proves nothing");

    s.spawn_unit(Team::Red, "Freeze", pos(&s, knight), None).expect("cast Freeze");
    let mut held = None;
    for _ in 0..60 {
        s.tick();
        let v = s.entity(knight).expect("the Knight is alive");
        if v.stun_ms > 0 {
            held = Some((v.attack_load_ms, v.attack_ms));
            break;
        }
    }
    let (load_at_freeze, progress_at_freeze) = held.expect("the Freeze never landed");
    assert!(load_at_freeze > 0, "the Knight was not mid-reload when it froze; the scene proves nothing");

    let mut prev = load_at_freeze;
    let mut ticks = 0;
    while s.entity(knight).map(|v| v.stun_ms > 0).unwrap_or(false) {
        s.tick();
        let v = s.entity(knight).expect("the Knight is alive");
        assert_eq!(v.attack_ms, progress_at_freeze, "the attack PROGRESS moved during the hold");
        assert_eq!(v.attack_load_ms, (prev - calib().tick_ms).max(0), "the load timer did not lose its flat TICK_MS on hold tick {ticks}");
        prev = v.attack_load_ms;
        ticks += 1;
    }
    assert!(ticks > 0, "the hold was over before it began");
    assert!(prev < load_at_freeze, "the load timer stood still for the whole hold ({load_at_freeze} ms)");
}

// ------------------------------------------------------- 6. expiry alignment

#[test]
fn a_buff_expires_on_the_stuns_alignment() {
    assert_shipped_arms();
    // ceil_from_next_tick: a D-ms buff applied in tick N is gone after
    // ceil(D / TICK_MS) further ticks, and present on every one of them.
    let mut s = bare(config());
    let knight = placed(&mut s, Team::Blue, "Knight", spot());
    let (_, time_ms) = spell_buff_of(&s, "Freeze");
    s.spawn_unit(Team::Red, "Freeze", pos(&s, knight), None).expect("cast Freeze");
    for _ in 0..40 {
        s.tick();
        if s.entity(knight).unwrap().stun_ms > 0 {
            break;
        }
    }
    let want = ceil_div(time_ms, calib().tick_ms);
    for k in 1..=want {
        s.tick();
        let v = s.entity(knight).unwrap();
        let on = v.buffs.iter().any(|b| !b.is_empty());
        if k < want {
            assert!(on, "the buff was gone on tick {k} of {want}");
        } else {
            assert!(!on, "the buff outlived tick {want}");
        }
        assert_eq!(v.stun_ms > 0, on, "the hold timer and the buff slot expire together on tick {k}");
    }
}

// --------------------------------------------------------- 7. seat symmetry

#[test]
fn a_freeze_does_the_same_on_both_seats() {
    assert_shipped_arms();
    let mut s = bare(symmetric_config());
    let blue = placed(&mut s, Team::Blue, "Knight", spot());
    let red_spot = mirror(&s, spot());
    let red = placed(&mut s, Team::Red, "Knight", red_spot);
    for _ in 0..20 {
        s.tick();
    }
    s.spawn_unit(Team::Red, "Freeze", pos(&s, blue), None).expect("cast Freeze on Blue");
    s.spawn_unit(Team::Blue, "Freeze", pos(&s, red), None).expect("cast Freeze on Red");
    let mut both = 0;
    for _ in 0..120 {
        s.tick();
        let (b, r) = (s.entity(blue), s.entity(red));
        let (Some(b), Some(r)) = (b, r) else { break };
        assert_eq!(b.stun_ms, r.stun_ms, "the two seats' holds parted");
        assert_eq!(
            b.buffs.iter().map(|x| (x.id, x.ms)).collect::<Vec<_>>(),
            r.buffs.iter().map(|x| (x.id, x.ms)).collect::<Vec<_>>(),
            "the two seats' buff lists parted"
        );
        if b.stun_ms > 0 {
            both += 1;
        }
    }
    assert!(both > 10, "only {both} frozen ticks; the scene proves nothing");
}

// ------------------------------------------- 8. the keys, and their refusals

#[test]
fn every_switchable_candidate_moves_something() {
    assert_shipped_arms();
    let dt = calib().tick_ms;
    // movement.BUFF_SPEED_COMPOSITION: the two arms part the moment two buffs meet.
    // The Rage row the game ships is 130 in ALL THREE multiplier columns.
    let rage = BuffDef { speed_pct: 130, hit_speed_pct: 130, spawn_speed_pct: 130, ..BuffDef::default() };
    let slow = BuffDef { speed_pct: -15, hit_speed_pct: -15, ..BuffDef::default() };
    let both = compose([rage, slow].iter(), Sel::Speed, 45);
    let single = 45 * 130 / 100;
    assert_ne!(both, single, "the composition and the single-multiplier reading differ on a slowed raged unit");

    // combat.HIT_SPEED_BUFF: the advance is TICK_MS under `none` and the composition
    // under the shipped arm.
    assert_ne!(compose([rage].iter(), Sel::HitSpeed, dt), dt, "a Rage would not move an unbuffed attack advance");

    // movement.STOMP_PAUSE_SCHEDULE: unbuffed the two agree tick for tick, and that
    // is the point -- the clock is the generalisation, not a different law.
    let s = bare(config());
    let golem = card_stat(&s, "Golem");
    assert!(golem.stop_movement_after_ms > 0, "the Golem stomps");
    let (stop, wait) = (golem.stop_movement_after_ms, golem.wait_ms);
    let mut clock = 0;
    for k in 0..400u32 {
        let (next, paused) = royalesim::path2026::stomp_clock_step(clock, dt, stop, wait);
        clock = next;
        assert_eq!(paused, royalesim::path2026::stomp_paused(dt, stop, wait, k), "the two arms parted at k = {k} unbuffed");
    }
    // Under a Rage they do NOT agree: the clock advances 65 ms a tick.
    let raged = compose([rage].iter(), Sel::Speed, 2 * dt) / 2;
    assert_eq!(raged, 65, "the raged advance the key names");
    let mut clock = 0;
    let mut parted = false;
    for k in 0..400u32 {
        let (next, paused) = royalesim::path2026::stomp_clock_step(clock, raged, stop, wait);
        clock = next;
        parted |= paused != royalesim::path2026::stomp_paused(dt, stop, wait, k);
    }
    assert!(parted, "a raged stomp card's pauses must differ from the unbuffed index schedule");
}

#[test]
fn an_unimplemented_candidate_is_refused_at_load() {
    // The three keys whose second candidate has no engine arm are refused by name,
    // not silently run as the shipped one (state.rs Calib::from_json `only`).
    let base = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/calibration.json")).expect("calibration.json");
    for (section, key, bad) in [
        ("status", "FULL_STOP_BUFF_IS_STUN", "buff_only"),
        ("status", "BUFF_PULSE_AMOUNT", "per_pulse"),
        ("spells", "PULSING_AREA_EFFECT", "hit_speed_period_delayed"),
    ] {
        let mut v: serde_json::Value = serde_json::from_str(&base).expect("parse");
        v[section][key]["value"] = serde_json::Value::String(bad.into());
        let err = Calib::from_json(&serde_json::to_string(&v).unwrap()).expect_err("{section}.{key} = {bad} must be refused");
        assert!(err.contains(key), "the refusal must name the key: {err}");
    }
    // And a candidate that IS implemented loads.
    for (section, key, good) in [
        ("movement", "BUFF_SPEED_COMPOSITION", "single_multiplier_floor"),
        ("combat", "HIT_SPEED_BUFF", "none"),
        ("movement", "STOMP_PAUSE_SCHEDULE", "k_plus_1_times_tick_ms_mod_period_strictly_greater_than_stop"),
        ("status", "BUFF_PULSE_TIMING", "on_application"),
        ("status", "TARGET_BUFF_ON_SPLASH", "primary_target_only"),
    ] {
        let mut v: serde_json::Value = serde_json::from_str(&base).expect("parse");
        v[section][key]["value"] = serde_json::Value::String(good.into());
        Calib::from_json(&serde_json::to_string(&v).unwrap()).unwrap_or_else(|e| panic!("{section}.{key} = {good}: {e}"));
    }
}

/// What one run of the foil scene leaves behind: the two Snowballed Knights' walk
/// and buff state, and the NEAR one's attack progress against the Red Knight it is
/// fighting. Every runnable foil of this file has to move at least one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FoilOut {
    near: (Vec2, i32),
    near_attack: (i32, i32),
    /// The Red Knight's hitpoints: what the near one's attack CYCLE did, which is
    /// what the hit-speed arm changes and the progress counter alone may not show.
    foe_hp: i32,
    near_buffs: usize,
    far_buffs: usize,
}

#[test]
fn the_engine_runs_the_old_arms_too() {
    // Each runnable foil plays a battle without panicking, and each is asserted
    // against the shipped arm -- EQUAL where the two arms provably agree on this
    // scene, DIFFERENT otherwise -- so a before / after on the harness is meaningful.
    // ~~three foils were computed and discarded with `let _ = ...`~~: that let
    // `primary_target_only` be a no-op in spell.rs for a whole pass (a review
    // finding).
    let scene = |cfg: BattleConfig| {
        let mut s = bare(cfg);
        let near = placed(&mut s, Team::Blue, "Knight", spot());
        let np = pos(&s, near);
        // A SECOND victim inside the Snowball's disc, so `whole_splash` and
        // `primary_target_only` part; and a Red Knight for the near one to fight, so
        // the hit-speed arm has an attack cycle to scale.
        let radius = match &card_stat(&s, "Snowball").spell.as_ref().expect("Snowball is a spell").shape {
            SpellShape::Projectile { hit: Some(hit), .. } => hit.radius,
            other => panic!("Snowball loaded as {other:?}"),
        };
        let far = placed(&mut s, Team::Blue, "Knight", Vec2::new(np.x + radius / 2, np.y));
        let foe = placed(&mut s, Team::Red, "Knight", Vec2::new(np.x, np.y + SUBTILE_PER_MILLITILE * 1100));
        for _ in 0..30 {
            s.tick();
        }
        s.spawn_unit(Team::Red, "Snowball", pos(&s, near), None).expect("cast Snowball");
        // 40 ticks, not 80: the Blue towers join in on the Red Knight and it is dead
        // by the 70th, which would read every arm's attack cycle back as Idle at 0.
        for _ in 0..40 {
            s.tick();
        }
        let live = |id: EntityId| s.entity(id);
        let buffs = |id: EntityId| live(id).map(|v| v.buffs.iter().filter(|b| !b.is_empty()).count()).unwrap_or(0);
        FoilOut {
            near: live(near).map(|v| (v.pos, v.speed_now)).unwrap_or((Vec2::default(), 0)),
            near_attack: live(near).map(|v| (v.attack_ms, v.attack_load_ms)).unwrap_or((0, 0)),
            foe_hp: live(foe).map(|v| v.hp).unwrap_or(0),
            near_buffs: buffs(near),
            far_buffs: buffs(far),
        }
    };
    let shipped = scene(config());
    assert_eq!(shipped.near_buffs, 1, "the Snowball never slowed the near Knight; the scene proves nothing");
    assert_eq!(shipped.far_buffs, 1, "whole_splash must slow BOTH Knights in the disc");
    assert!(shipped.foe_hp > 0, "the Red Knight died before the read; every arm would read Idle at 0");
    assert!(shipped.near_attack.0 > 0, "the near Knight never started an attack cycle; the hit-speed foil proves nothing");

    let no_buff_speed = scene(with_calib(|c| c.buff_speed_composition = BuffComposition::SingleMultiplierFloor));
    // The Snowball's slow is a single buff, so the two compositions AGREE: the foil
    // is honest about that rather than pretending a difference.
    assert_eq!(shipped, no_buff_speed, "one buff: the two compositions are the same law");

    let primary = scene(with_calib(|c| c.target_buff_on_splash = TargetBuffScope::PrimaryTargetOnly));
    assert_eq!(primary.far_buffs, 0, "primary_target_only must leave the second Knight of the disc alone");
    assert_ne!(shipped, primary, "the primary_target_only foil changed nothing");

    let unbuffed_attack = scene(with_calib(|c| c.hit_speed_buff = HitSpeedBuff::None));
    assert_ne!(
        (shipped.near_attack, shipped.foe_hp),
        (unbuffed_attack.near_attack, unbuffed_attack.foe_hp),
        "a slowed Knight's attack cycle must run faster with the hit-speed buff off"
    );

    let old_clock = scene(with_calib(|c| c.stomp_schedule = StompSchedule::TickIndexMod));
    assert_eq!(shipped, old_clock, "a Knight does not stomp: the two schedules agree on it");
}
