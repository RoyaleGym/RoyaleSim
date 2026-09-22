//! THE REACH AND THE ATTACK CYCLE: how far a unit reaches, when its first hit
//! lands, and where its projectile is born -- target.rs `in_attack_range`,
//! combat.rs `attack_step_progress` / `fire` / `step_projectiles`, calibration
//! targeting.ATTACK_RANGE_RULE, combat.ATTACK_CYCLE, combat.PROJECTILE_LAUNCH,
//! charge.CHARGED_HIT_TIMING.
//!
//! THE THREE LAWS, measured on the live 16.402 corpus (the calibration keys carry
//! the counts):
//!   A. REACH: a target is in range when the CENTRE distance is <= Range + the
//!      ATTACKER's CollisionRadius (under ADD_CHARACTER_RANGE_TO_RADIUS) + the
//!      TARGET's. The goal cell of the walk is the SHORTER Range + own R to the
//!      target's centre (pathfinding.PATH_SEARCH), so a unit stops walking before
//!      it reaches its goal, the target's radius earlier. The same sum with
//!      SightRange is the sight scan.
//!   B. THE CYCLE: a fresh cycle is credited LoadTime and then ticks up by TICK_MS,
//!      and the hit lands on every multiple of HitSpeed -- so the FIRST hit on a
//!      fresh target lands (HitSpeed - LoadTime) / TICK_MS - 1 ticks after the tick
//!      that found it in range, and the rest HitSpeed apart. A CHARGED unit's
//!      progress is snapped to the next multiple instead, so its DamageSpecial hit
//!      lands on the first attack pass after the step into reach, with no windup.
//!   C. THE LAUNCH (measured on the live effects stream): a projectile is born
//!      ProjectileStartRadius from the attacker's centre toward the target, does not
//!      move on the fire tick, and then advances its Speed in native units per tick.
//!
//! WHAT IS PINNED, every number read from the loaded CardDb, `Calib::shipped()` or
//! the arena -- never pasted -- and checked against the live 16.402 corpus where the
//! corpus measured it (the reach gap of the corpus report):
//!   1. a Prince, a Dark Prince and a Knight walking at the enemy princess tower
//!      stop with their CENTRE inside Range + own R + the tower's R and outside it
//!      one step earlier, and that boundary is the live one: 3200 for the Prince
//!      (the game stopped it at 3135 on the sample battle and 3133 on capture
//!      20260920-002736, never inside 3126) and 2800 for the Dark Prince (live 2776);
//!   2. the same three against a BUILDING (a Cannon), whose radius differs from the
//!      tower's, so the stop distance moves with the target's radius too;
//!   3. the Prince's charged hit lands on the FIRST attack pass after the step into
//!      reach (the sample: the step at t328, the tower 3052 -> 2269 at t329), not a
//!      LoadTime windup later;
//!   4. the princess tower's first arrow leaves it (HitSpeed - LoadTime) / TICK_MS -
//!      1 ticks after it takes the target (the sample: state 2 at t280, the arrow at
//!      t295 = 15 ticks, HitSpeed 800 and a blank LoadTime), the next ones HitSpeed
//!      apart, and a Knight's first melee hit lands on the same rule;
//!   5. the arrow is born ProjectileStartRadius from the tower's centre toward the
//!      target, stands still on the fire tick and then advances its Speed in native
//!      units per tick (the live arrows: 299-300 then 599-600);
//!   6. every candidate of the four keys moves a measurable behaviour, each on a
//!      scene where the shipped arm gives a different number, and a candidate with
//!      no engine implementation is refused at load.
//!
//! PLANT (regression): `reach_without_own_radius` forces the earlier reach (Range +
//! the target's radius only): (1), (2) and (6) go red -- 3 of 7.
//!     RUSTFLAGS='--cfg clash_plant="reach_without_own_radius"' CARGO_TARGET_DIR=target/plant cargo test --test reach

mod common;

use common::*;
use royalesim::card::CardDef;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE};
use royalesim::state::{AttackCycle, AttackRangeRule, BattleConfig, BattleState, Calib, ChargedHitTiming, ProjectileLaunch};
use royalesim::{EntityId, Team};

/// Engine subtiles per native millitile (the recordings' unit).
const K: i32 = SUBTILE_PER_MILLITILE;

fn calib() -> Calib {
    Calib::shipped()
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
    assert!(c.add_character_range_to_radius, "globals ship ADD_CHARACTER_RANGE_TO_RADIUS = TRUE");
    assert_eq!(c.attack_range_rule, AttackRangeRule::RangePlusBothRadii, "the shipped arm this file pins");
    assert_eq!(c.attack_cycle, AttackCycle::ProgressCredit, "the shipped arm this file pins");
    assert_eq!(c.charged_hit_timing, ChargedHitTiming::FirstAttackPassNoWindup, "the shipped arm this file pins");
    assert_eq!(c.projectile_launch, ProjectileLaunch::StartRadiusNextTick, "the shipped arm this file pins");
}

/// Native millitiles between two engine points (the unit every live measurement in
/// the doc comments is in).
fn native_dist(a: Vec2, b: Vec2) -> i32 {
    a.sub(b).len() / K
}

/// The lane a Blue attacker walks up: Red's engine-Left princess tower, tower slot 1.
fn red_left_tower(s: &BattleState) -> EntityId {
    s.tower_ids(Team::Red)[1].expect("Red's engine-Left princess tower stands at the start")
}

/// A spot in that lane, far enough from the tower that the walker takes many steps.
fn lane_spot() -> Vec2 {
    t(375, 850)
}

/// What one approach measured. `engaged` is the post-tick the attacker first ran
/// its attack cycle -- the tick AFTER the step that brought the victim into reach,
/// because the Attack phase runs before the Move phase (match.TICK_ORDER), which is
/// what the live frames show too (the sample: the step at t328, the hit at t329).
/// `at` is the centre distance it stopped at, `before` the one it had before that
/// last step, and `step` the length of that step -- all in native millitiles.
struct Approach {
    id: EntityId,
    engaged: u32,
    at: i32,
    before: i32,
    step: i32,
    /// Range + the attacker's own CollisionRadius + the victim's, from the loaded
    /// card data and the victim's own radius -- law A spelled out here, so the test
    /// does not call the engine's own predicate for the number it pins.
    reach: i32,
}

/// Walk `card` from `from` at `victim` until it engages, and read the approach.
fn approach_from(s: &mut BattleState, card: &str, from: Vec2, victim: EntityId, max: u32) -> Approach {
    let id = s.scenario_spawn_now(Team::Blue, card, from, None).unwrap();
    let vpos = s.entity(victim).unwrap().pos;
    let vr = s.entity(victim).unwrap().radius;
    let stat: &CardDef = card_stat(s, card);
    let (range, own_r) = (stat.range, stat.collision_radius);
    assert!(own_r > 0 && vr > 0, "{card}: a point attacker or a point victim cannot show the rule");
    let reach = (range + own_r + vr) / K;
    let mut seen = vec![native_dist(s.entity(id).unwrap().pos, vpos)];
    for _ in 0..max {
        s.tick();
        let Some(e) = s.entity(id) else { panic!("{card} died on the way in") };
        seen.push(native_dist(e.pos, vpos));
        if e.attack_phase != AttackPhase::Idle {
            let at = *seen.last().unwrap();
            let before = *seen.iter().rev().find(|d| **d != at).unwrap_or_else(|| panic!("{card} never moved"));
            return Approach { id, engaged: s.tick_count(), at, before, step: before - at, reach };
        }
    }
    panic!("{card} never engaged its victim (stopped {} native out, reach {reach})", seen.last().unwrap());
}

/// The same up the lane at a crown tower.
fn approach(s: &mut BattleState, card: &str, victim: EntityId, max: u32) -> Approach {
    approach_from(s, card, lane_spot(), victim, max)
}

/// A spot no crown tower can reach (a princess tower's own reach is SightRange +
/// its radius + the victim's, and the king is asleep at the start): the centre
/// column just short of the river, so a building there is shot by nobody and the
/// only reach in the scene is the walker's.
fn tower_free_spot() -> Vec2 {
    t(900, 1400)
}

fn tower_free_walker_spot() -> Vec2 {
    t(900, 1000)
}

// ---------------------------------------------------------------------------
// (1) + (2): the stop distance is Range + BOTH radii

#[test]
fn a_melee_unit_stops_at_range_plus_both_radii_from_a_crown_tower() {
    // Plant: reach_without_own_radius (each unit walks its own radius further in).
    assert_shipped_arms();
    let mut seen = Vec::new();
    for card in ["Prince", "DarkPrince", "Knight"] {
        let mut s = BattleState::new(7, config());
        let tower = red_left_tower(&s);
        let a = approach(&mut s, card, tower, 400);
        let stat = card_stat(&s, card).clone();
        // the step into reach, and no step before it: the stop is AT the boundary
        assert!(a.at <= a.reach, "{card}: stopped {} native out, reach {}", a.at, a.reach);
        assert!(a.before > a.reach, "{card}: was already inside the reach one tick earlier ({} <= {})", a.before, a.reach);
        assert!(a.at + a.step > a.reach, "{card}: overshot the boundary by more than one step ({} + {} <= {})", a.at, a.step, a.reach);
        // the attacker's OWN radius is what this test is about: without it the same
        // walk would end a full radius further in, which is a different number
        let without_own = (stat.range + s.entity(tower).unwrap().radius) / K;
        assert!(a.reach - without_own == stat.collision_radius / K && stat.collision_radius / K > 0, "{card}: the own-radius term is not visible in the reach");
        // and the unit is attacking there, not walking on
        let e = s.entity(a.id).unwrap();
        assert_eq!(e.target, Some(tower), "{card}: stopped without taking the tower");
        seen.push((card, a.at, a.reach, a.step));
    }
    // THE LIVE FIGURES (the reach gap of the corpus report; captures
    // 20260920-003751 and 20260920-002736): the game stopped its Prince 3126-3200
    // native from the princess tower's centre (3135 on the sample battle, 3133 on
    // 002736) and its Dark Prince at 2776 -- so the BOUNDARY the game holds
    // is 3200 for the Prince and 2800 for the Dark Prince, which is what the reach
    // computed above must be. Where in the last step's width a given walk lands
    // depends on where it started (this scene: 3090 and 2745, both one step inside),
    // so the boundary is pinned and the landing point is bounded by the step.
    // The earlier engine stopped the Prince in [2526, 2646).
    let prince = seen.iter().find(|r| r.0 == "Prince").unwrap();
    assert_eq!(prince.2, 3200, "the Prince's reach on a princess tower; live: it stopped at 3135 and never inside 3126");
    assert!(prince.1 > 3126 - prince.3, "the Prince stops {} native out, more than one step ({}) inside the live 3126", prince.1, prince.3);
    let dark = seen.iter().find(|r| r.0 == "DarkPrince").unwrap();
    assert_eq!(dark.2, 2800, "the Dark Prince's reach on a princess tower; live: it stopped at 2776");
    assert!(dark.1 > 2776 - dark.3, "the Dark Prince stops {} native out, more than one step ({}) inside the live 2776", dark.1, dark.3);
}

#[test]
fn the_same_reach_holds_against_a_building_whose_radius_differs() {
    // A Cannon on the centre column just short of the river, which no crown tower
    // reaches, so the only reach in the scene is the walker's.
    // Plant: reach_without_own_radius.
    assert_shipped_arms();
    for card in ["Prince", "DarkPrince", "Knight"] {
        let mut s = BattleState::new(7, config());
        s.scenario_spawn_now(Team::Red, "Cannon", tower_free_spot(), None).unwrap();
        let cannon = find_live(&s, Team::Red, "Cannon")[0].id;
        let full = s.entity(cannon).unwrap().hp;
        let a = approach_from(&mut s, card, tower_free_walker_spot(), cannon, 400);
        assert_eq!(s.entity(cannon).unwrap().hp, full, "scene: something other than the {card} is shooting the Cannon");
        assert!(a.at <= a.reach && a.before > a.reach, "{card} vs Cannon: {} / {} against reach {}", a.at, a.before, a.reach);
        assert!(a.at + a.step > a.reach, "{card} vs Cannon: stopped {} native out, more than one step ({}) inside the reach {}", a.at, a.step, a.reach);
        let cannon_r = s.entity(cannon).unwrap().radius;
        // the same card stops FURTHER from the bigger crown tower: the target's
        // radius is in the sum as well
        let mut s2 = BattleState::new(7, config());
        let tower = red_left_tower(&s2);
        let b = approach(&mut s2, card, tower, 400);
        let dr = (s2.entity(tower).unwrap().radius - cannon_r) / K;
        assert!(dr > 0, "vacuous: the crown tower and the Cannon have the same radius");
        assert_eq!(b.reach - a.reach, dr, "{card}: the reach did not move with the target's radius");
    }
}

// ---------------------------------------------------------------------------
// (3): the charged hit lands on the first attack pass after the step into reach

#[test]
fn the_charged_hit_lands_on_the_arrival_tick_with_no_load_time_windup() {
    // The tick the Prince engages is read from the run, never pasted, so the plant
    // (which moves the walk) fails on the amount and the timing, not on a number.
    assert_shipped_arms();
    let mut s = BattleState::new(7, config());
    let tower = red_left_tower(&s);
    let before_hp = s.tower_hp(Team::Red)[1];
    let a = approach(&mut s, "Prince", tower, 400);
    // The Attack phase of the engage tick found the tower in reach and the charged
    // snap fired it on that very pass (calibration charge.CHARGED_HIT_TIMING): the
    // live sample steps at t328 and the tower is 3052 -> 2269 at t329.
    let drop = before_hp - s.tower_hp(Team::Red)[1];
    let ch = card_stat(&s, "Prince").charge.expect("cards.json Prince has a charge block");
    let level = s.config().card_level[Team::Blue as usize];
    let special = s.cards().scaled(s.cards().index("Prince").unwrap(), level, ch.damage_special).unwrap();
    let pct = card_stat(&s, "Prince").crown_tower_damage_percent;
    let want = royalesim::combat::damage_against(royalesim::entity::EntityKind::PrincessTower, special, pct, s.config().calib.crown_rounding);
    assert_eq!(drop, want, "the charged hit did not land for DamageSpecial on the engage tick {} ", a.engaged);
    assert!(!s.entity(a.id).unwrap().charged, "the snap must consume the charge");
    let load = card_stat(&s, "Prince").load_time_ms;
    let hs = card_stat(&s, "Prince").hit_speed_ms;
    assert!(load > 0 && hs > load, "data: the Prince's LoadTime / HitSpeed cannot show a windup");
    // the same scene under the foil: the charged unit waits out the ordinary
    // first-hit delay of the cycle instead, and hits for the same amount then
    let mut f = BattleState::new(7, with_calib(|c| c.charged_hit_timing = ChargedHitTiming::AfterLoadTimeWindup));
    let ftower = red_left_tower(&f);
    let fhp = f.tower_hp(Team::Red)[1];
    let fa = approach(&mut f, "Prince", ftower, 400);
    assert_eq!(f.tower_hp(Team::Red)[1], fhp, "the foil hit on the engage tick too: the snap is not what fired it");
    assert_eq!(fa.engaged, a.engaged, "the two arms must part on the HIT tick only, not on the walk");
    let wait = (hs - load) / calib().tick_ms - 1;
    for _ in 0..wait - 1 {
        f.tick();
    }
    assert_eq!(f.tower_hp(Team::Red)[1], fhp, "the foil hit before (HitSpeed - LoadTime) of the engage tick");
    f.tick();
    assert_eq!(fhp - f.tower_hp(Team::Red)[1], want, "the foil's hit is the same DamageSpecial, {wait} ticks later");
}

// ---------------------------------------------------------------------------
// (4): the first hit of a fresh cycle

/// Post-ticks at which `id`'s attack first stopped being idle and at which the
/// number of projectiles in flight first grew, driving at most `max` ticks.
fn entry_and_launch(s: &mut BattleState, id: EntityId, max: u32) -> (u32, u32) {
    let (mut entry, mut launch) = (None, None);
    let mut in_flight = s.projectiles().len();
    for _ in 0..max {
        s.tick();
        if entry.is_none() && s.entity(id).is_some_and(|e| e.attack_phase != AttackPhase::Idle) {
            entry = Some(s.tick_count());
        }
        if launch.is_none() && s.projectiles().len() > in_flight {
            launch = Some(s.tick_count());
        }
        in_flight = s.projectiles().len();
        if entry.is_some() && launch.is_some() {
            break;
        }
    }
    (entry.expect("the attacker never entered its cycle"), launch.expect("the attacker never launched"))
}

#[test]
fn the_princess_towers_first_arrow_leaves_hit_speed_minus_load_time_after_it_takes_the_target() {
    // LIVE (the sample battle, capture 20260920-003751): the princess tower starts
    // attacking at t280 with progress 50 and its first arrow appears at t295 -- 15
    // ticks, = (HitSpeed 800 - a blank LoadTime) / 50 - 1. The earlier engine fired
    // on the acquisition tick itself (a blank LoadTime meant no windup at all), 15
    // ticks early on every fresh target, and 4-8 ticks early on the Prince rows of
    // capture 20260920-080208.
    assert_shipped_arms();
    let mut s = BattleState::new(7, config());
    let tower = red_left_tower(&s);
    let stat = card_stat(&s, "PrincessTower").clone();
    assert_eq!(stat.load_time_ms, 0, "data: the princess tower's LoadTime is blank, which is what makes this case the pure HitSpeed one");
    let tick = calib().tick_ms;
    let want = (stat.hit_speed_ms - stat.load_time_ms) / tick - 1;
    assert!(want > 0, "data: HitSpeed {} is not longer than LoadTime {}", stat.hit_speed_ms, stat.load_time_ms);
    // a Giant walks in: it never shoots back, so every projectile in flight is the
    // tower's
    s.scenario_spawn_now(Team::Blue, "Giant", lane_spot(), None).unwrap();
    let (entry, launch) = entry_and_launch(&mut s, tower, 600);
    assert_eq!(launch - entry, want as u32, "the tower's first arrow left {} ticks after it took the target, want {want}", launch - entry);
    assert_eq!(launch - entry, 15, "live: the princess tower's first arrow is 15 ticks after it starts attacking");
    // and the cadence is HitSpeed: the next launch is want + 1 ticks later
    let mut in_flight = s.projectiles().len();
    let mut next = None;
    for _ in 0..(2 * stat.hit_speed_ms / tick) {
        s.tick();
        if s.projectiles().len() > in_flight {
            next = Some(s.tick_count());
            break;
        }
        in_flight = s.projectiles().len();
    }
    assert_eq!(next.map(|n| n - launch), Some((stat.hit_speed_ms / tick) as u32), "the tower's cadence is not HitSpeed");
}

#[test]
fn a_melee_first_hit_lands_hit_speed_minus_load_time_after_the_target_came_into_range() {
    // The same rule with a LoadTime that is not blank: a Knight on a Cannon, out of
    // every tower's reach. Under the earlier windup the first hit came
    // LoadTime after the entry instead, which is a different tick whenever
    // 2 x LoadTime != HitSpeed.
    assert_shipped_arms();
    let stat = {
        let s = BattleState::new(7, config());
        card_stat(&s, "Knight").clone()
    };
    let tick = calib().tick_ms;
    let want = (stat.hit_speed_ms - stat.load_time_ms) / tick - 1;
    assert!(stat.load_time_ms > 0 && 2 * stat.load_time_ms != stat.hit_speed_ms, "data: the Knight's LoadTime cannot tell the two arms apart");
    let run = |cfg: BattleConfig| {
        let mut s = BattleState::new(7, cfg);
        s.scenario_spawn_now(Team::Red, "Cannon", tower_free_spot(), None).unwrap();
        let cannon = find_live(&s, Team::Red, "Cannon")[0].id;
        let knight = s.scenario_spawn_now(Team::Blue, "Knight", tower_free_walker_spot(), None).unwrap();
        let hp = s.entity(cannon).unwrap().hp;
        let (mut entry, mut hit) = (None, None);
        for _ in 0..600 {
            s.tick();
            if entry.is_none() && s.entity(knight).is_some_and(|e| e.attack_phase != AttackPhase::Idle) {
                entry = Some(s.tick_count());
            }
            if hit.is_none() && s.entity(cannon).is_some_and(|e| e.hp < hp) {
                hit = Some(s.tick_count());
                break;
            }
        }
        (entry.expect("the Knight never entered its cycle"), hit.expect("the Knight never hit"))
    };
    let (entry, hit) = run(config());
    assert_eq!(hit - entry, want as u32, "the Knight's first hit landed {} ticks after the entry, want {want}", hit - entry);
    let (fentry, fhit) = run(with_calib(|c| {
        c.attack_cycle = AttackCycle::WindupLoadTime;
        c.charged_hit_timing = ChargedHitTiming::AfterLoadTimeWindup;
    }));
    // the windup arm starts its clock with one TICK_MS already on it, so its first
    // hit lands LoadTime / TICK_MS - 1 ticks after the entry (the Knight's LoadTime
    // is a whole number of ticks; hide.rs says the same of the Tesla)
    assert_eq!(fhit - fentry, (stat.load_time_ms.div_euclid(tick)) as u32 - 1, "the foil is not the LoadTime windup");
    assert_ne!(fhit - fentry, hit - entry, "vacuous: the two arms agree on this card");
}

// ---------------------------------------------------------------------------
// (5): where the projectile is born and when it first moves

#[test]
fn a_tower_arrow_is_born_at_the_start_radius_and_takes_its_first_step_the_next_tick() {
    // LIVE (the effects stream of the whole 16.402 corpus): the arrow first appears
    // 299-300 native from the tower's centre toward its target (ProjectileStartRadius
    // 300), does not move on that frame, then advances 599-600 per tick (Speed 600).
    assert_shipped_arms();
    let mut s = BattleState::new(7, config());
    let tower = red_left_tower(&s);
    let stat = card_stat(&s, "PrincessTower").clone();
    let start_r = stat.projectile_start_radius;
    assert!(start_r > 0, "data: the princess tower's ProjectileStartRadius is blank");
    let pspeed = stat.projectile.expect("the princess tower shoots a projectile").speed;
    s.scenario_spawn_now(Team::Blue, "Giant", lane_spot(), None).unwrap();
    let (_, _launch) = entry_and_launch(&mut s, tower, 600);
    let tpos = s.entity(tower).unwrap().pos;
    let p0 = s.projectiles()[0].pos;
    // (the direction is scaled by integer division, so the birth point can sit one
    // native unit inside the radius -- which is exactly the live 299-300)
    let born = native_dist(p0, tpos);
    assert!((born - start_r / K).abs() <= 1, "the arrow was born {born} native from the tower's centre, ProjectileStartRadius {}", start_r / K);
    // it stands still on the fire tick and steps on the next
    s.tick();
    let p1 = s.projectiles().first().map(|p| p.pos).expect("the arrow vanished on its second tick");
    let step = native_dist(p1, p0);
    let want = pspeed * calib().projectile_speed_to_subtiles_per_tick / K;
    assert!((step - want).abs() <= 1, "the arrow's first step is {step} native, want {want} (its Speed)");
    // under the foil it is born at the centre and has already moved once
    let mut f = BattleState::new(7, with_calib(|c| c.projectile_launch = ProjectileLaunch::AttackerCentreSameTick));
    let ftower = red_left_tower(&f);
    f.scenario_spawn_now(Team::Blue, "Giant", lane_spot(), None).unwrap();
    let (_, _) = entry_and_launch(&mut f, ftower, 600);
    let fp = f.projectiles()[0].pos;
    assert_ne!(native_dist(fp, f.entity(ftower).unwrap().pos), start_r / K, "the foil is not distinguishable from the shipped arm");
}

// ---------------------------------------------------------------------------
// (6): every candidate moves a behaviour, and an unimplemented one is refused

#[test]
fn every_reach_and_cycle_candidate_moves_a_measurable_behaviour_and_the_loader_refuses_the_rest() {
    assert_shipped_arms();
    // ATTACK_RANGE_RULE: the Prince's stop distance moves by its own radius.
    let mut s = BattleState::new(7, config());
    let tower = red_left_tower(&s);
    let shipped = approach(&mut s, "Prince", tower, 400);
    let mut f = BattleState::new(7, with_calib(|c| c.attack_range_rule = AttackRangeRule::RangePlusTargetRadius));
    let ftower = red_left_tower(&f);
    let fa = approach(&mut f, "Prince", ftower, 400);
    let own_r = card_stat(&f, "Prince").collision_radius / K;
    assert!(shipped.at > fa.at, "the two reach arms stop the Prince at {} / {} native (own radius {own_r})", shipped.at, fa.at);
    assert!(
        (shipped.at - fa.at - own_r).abs() <= shipped.step.max(fa.step),
        "the old arm's Prince should stop its own radius ({own_r}) further in, within one step: {} vs {}",
        shipped.at,
        fa.at
    );
    // ATTACK_CYCLE and CHARGED_HIT_TIMING move the hit ticks: pinned in (3) and (4).
    // PROJECTILE_LAUNCH moves the birth point: pinned in (5).
    // A candidate the engine does not implement is refused at load, never mapped.
    for (needle, bad, key) in [
        ("\"value\": \"range_plus_both_radii\"", "\"value\": \"edge_to_edge_only\"", "targeting.ATTACK_RANGE_RULE"),
        ("\"value\": \"progress_credit\"", "\"value\": \"instant_hit\"", "combat.ATTACK_CYCLE"),
        ("\"value\": \"start_radius_next_tick\"", "\"value\": \"muzzle_offset\"", "combat.PROJECTILE_LAUNCH"),
        ("\"value\": \"first_attack_pass_no_windup\"", "\"value\": \"contact_same_tick\"", "charge.CHARGED_HIT_TIMING"),
    ] {
        let edited = royalesim::py::EMBEDDED_CALIBRATION_JSON.replacen(needle, bad, 1);
        assert_ne!(edited, royalesim::py::EMBEDDED_CALIBRATION_JSON, "{key}: edit did not apply; the JSON layout changed");
        let err = Calib::from_json(&edited).expect_err("an unimplemented candidate loaded");
        assert!(err.contains(key) && err.contains("no engine implementation"), "{key}: {err}");
    }
}
