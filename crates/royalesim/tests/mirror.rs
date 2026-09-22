//! SEAT SYMMETRY.
//!
//! WHY IT EXISTS: an engine that updates entities in insertion order lets one seat
//! win every mirror trade. An RL agent finds a systematic seat advantage in hours,
//! and self-play then trains on it forever. This engine claims symmetry BY
//! CONSTRUCTION (buffered damage, team-frame planning, seat-invariant keys); this
//! file is the measurement of that claim.
//!
//! THE MIRROR: Blue at (x, y) <-> Red at (W - x, H - y) -- the 180-degree ROTATION,
//! not the y-reflection Blue (x, y) <-> Red (x, H - y). The distinction is not
//! cosmetic: a suite that enforces the reflection passes while a policy shared by
//! both seats desyncs on 80 of 144 multi-unit deploys, with single-unit deploys
//! staying symmetric (0/240). That is why the multi-unit scenarios below exist, and
//! why the reflection is EXPECTED to break at ties.
//!
//! THE CHECK: at EVERY tick the multiset of Blue entities, written in Blue's
//! frame, equals the multiset of Red entities written in Red's (rotated) frame
//! (position, hp, shield, attack phase and timer, deploy timer, target lock,
//! movement carry, route, and what each is targeting); likewise projectiles,
//! tower hp in own-frame order, crowns, elixir and king activation. Slot indices
//! and team_seq are deliberately NOT compared -- see common::Canon -- so scenarios
//! can spawn the two teams in different orders.
//!
//! PLANTS THAT MUST TURN THIS FILE RED (each restores one reflection-only rule;
//! `RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test mirror`):
//!   id_tiebreak           raw slot index as the target tie-break
//!   reflection_frame      Red's frame is the y-reflection again
//!   reflection_formation  Red's formation offset is (dx, -dy) again
//!   reflection_centre_lane  the x = W/2 default-tower lane decided on engine x
//!   reflection_bridge_tie DiagonalLookahead equal-cost bridge -> lower ENGINE x
//!   reflection_push_tie   coincident same-team push split on engine x
//!   sibling_yield_key     formation siblings share a YieldKey again
//! SPELLS (the comparator also holds every live spell object in its caster's frame,
//! and each unit's stun, resume flag and knockback slide):
//!   zero_vector_plus_y              a push with no radial direction goes +y for BOTH seats
//!   rolling_forward_plus_y_for_both the Log rolls toward +y for both seats
//!   eject_tie_engine_frame          a unit pushed exactly mid-river is ejected by Blue's
//!                                   side preference for both seats
//!   reflection_formation            (above) -- also the Goblin Barrel's released units
//!
//! WHAT IT CANNOT CATCH: an asymmetry that only arises in a configuration no
//! scenario here reaches, and any bias that is symmetric (both seats equally
//! wrong). It says nothing about fidelity to the real game. Measured: the
//! `slot_order_blocker_tie` and `slot_order_obstacle_tie` plants (two blockers
//! stacked on one point) leave every test in this file GREEN -- that tie is gated by
//! tests/stacked_tie.rs. `red_reversed` here reorders `spawn_unit` calls, which
//! still assign team_seq in CALL order; setup-spawn order is gated by
//! tests/setup_spawn_order.rs.
#![allow(unexpected_cfgs)]
mod common;

use royalesim::entity::EntityKind;
use royalesim::state::PathSearch;
use royalesim::fixed::{Vec2, SUBTILE};
use royalesim::state::{BattleConfig, BattleState, Outcome};
use royalesim::{PathModel, PushModel, Team};
use common::*;

// THE PLANNER THIS FILE MEASURES UNDER: `common::symmetric_config`,
// the frame-planned trace-fitted search. The shipped pathfinder is the client's
// (path16402.rs) and is NOT seat-symmetric -- see that helper's comment and
// `the_shipped_search_is_absolute_grid_not_seat_symmetric` at the end of this file.

struct Unit {
    card: &'static str,
    /// Blue-side position, hundredths of a tile. Red's twin is its rotation.
    at: (i32, i32),
    /// Tick on which it is placed.
    tick: u32,
}

/// What a scenario actually exercised, so a mirror check that passed on an
/// idle board cannot be mistaken for evidence.
#[derive(Default, Debug)]
struct Engagement {
    /// Some troop ended a tick below max hp.
    troop_damaged: bool,
    /// Some troop died (the live troop count fell).
    troop_died: bool,
    /// Some non-crown building ended a tick below max hp.
    building_damaged: bool,
    /// Some crown tower ended a tick below max hp.
    tower_damaged: bool,
}

/// Run a mirror scenario, checking the rotation mirror at every tick.
/// `red_reversed` spawns Red's units of each tick in the reverse order of Blue's,
/// so slot indices of twins do not line up.
fn run_mirror(name: &str, units: &[Unit], red_reversed: bool, cfg: BattleConfig, max_ticks: Option<u32>) -> (BattleState, Engagement) {
    let mut s = BattleState::new(9, cfg);
    let last_spawn = units.iter().map(|u| u.tick).max().unwrap_or(0);
    let mut checked = 0u32;
    let mut peak = 0usize;
    let mut eng = Engagement::default();
    let mut last_troops = 0usize;
    while !s.is_done() && max_ticks.map_or(true, |m| s.tick_count() < m) {
        let now: Vec<&Unit> = units.iter().filter(|u| u.tick == s.tick_count()).collect();
        for u in &now {
            s.spawn_unit(Team::Blue, u.card, t(u.at.0, u.at.1), None)
                .unwrap_or_else(|e| panic!("{name}: Blue {} at {:?}: {e:?}", u.card, u.at));
        }
        let red: Vec<&&Unit> = if red_reversed { now.iter().rev().collect() } else { now.iter().collect() };
        for u in red {
            let p = mirror(&s, t(u.at.0, u.at.1));
            s.spawn_unit(Team::Red, u.card, p, None).unwrap_or_else(|e| panic!("{name}: Red {} at {:?}: {e:?}", u.card, p));
        }
        let spawned = !now.is_empty();
        s.tick();
        peak = peak.max(s.live_count());
        if let Err(e) = check_mirror(&s) {
            panic!("{name}: {e}\ncensus: {:?}", census(&s));
        }
        checked += 1;
        let troops = s.entities().filter(|e| e.kind == EntityKind::Troop).count();
        if troops < last_troops && !spawned {
            eng.troop_died = true;
        }
        last_troops = troops;
        for e in s.entities().filter(|e| e.hp < e.max_hp) {
            match e.kind {
                EntityKind::Troop => eng.troop_damaged = true,
                EntityKind::Building => eng.building_damaged = true,
                _ => eng.tower_damaged = true,
            }
        }
    }
    // Vacuity guards: the scenario must have run past its spawns with units in it.
    // A spell card is a cast, not a unit: it adds no entity.
    assert!(checked > last_spawn + 100, "{name}: only {checked} ticks checked");
    let bodies = units.iter().filter(|u| card_stat(&s, u.card).kind != royalesim::card::CardKind::Spell).count();
    assert!(peak >= 6 + 2 * bodies, "{name}: peak live count {peak} -- did the units spawn?");
    println!(
        "{name}: ticks={} outcome={:?} towers={:?} peak_live={peak}",
        s.tick_count(),
        s.outcome(),
        s.tower_hp(Team::Blue)
    );
    println!("{name}: {eng:?}");
    (s, eng)
}

fn assert_draw(name: &str, s: &BattleState) {
    assert_eq!(s.tower_hp(Team::Blue), own_frame_tower_hp(s, Team::Red), "{name}: own-frame tower hp differ");
    if s.is_done() {
        assert_eq!(s.outcome(), Some(Outcome::Draw), "{name}: a mirror battle must end in a Draw");
    }
}

#[test]
fn mirror_knight_vs_knight_single_unit() {
    // The single-unit control: a Knight each, twins on OPPOSITE lanes (the
    // rotation puts Red's twin of a left-lane Knight on the engine-right lane), so
    // each walks into the other side's tower fire.
    let units = [Unit { card: "Knight", at: (350, 1100), tick: 0 }];
    let (s, eng) = run_mirror("knight_single", &units, false, symmetric_config(), Some(900));
    assert_eq!(find_live(&s, Team::Blue, "Knight").len(), find_live(&s, Team::Red, "Knight").len());
    assert!(eng.troop_damaged || eng.tower_damaged, "knight_single: nothing was ever hit: {eng:?}");
    assert_draw("knight_single", &s);
}

#[test]
fn mirror_knights_meet_head_on() {
    // Two Knights per side placed so each Blue Knight meets a RED twin's lane-mate:
    // Blue at (3.5, 11) and (14.5, 21) means Red's twins are at (14.5, 21) and
    // (3.5, 11) -- a Blue and a Red Knight stacked on each lane. They fight.
    let units = [Unit { card: "Knight", at: (350, 1100), tick: 0 }, Unit { card: "Knight", at: (1450, 2100), tick: 0 }];
    let (s, eng) = run_mirror("knights_head_on", &units, true, symmetric_config(), Some(900));
    assert!(eng.troop_damaged, "knights_head_on: the Knights never fought: {eng:?}");
    assert_draw("knights_head_on", &s);
}

#[test]
fn mirror_knight_vs_knight_full_battle_is_a_draw() {
    let units = [Unit { card: "Knight", at: (350, 1100), tick: 0 }, Unit { card: "Knight", at: (1450, 1000), tick: 200 }];
    let (s, eng) = run_mirror("knight_full", &units, true, symmetric_config(), None);
    assert!(s.is_done());
    assert!(eng.troop_died, "knight_full: nothing died");
    assert_draw("knight_full", &s);
}

#[test]
fn mirror_splash_vs_swarm() {
    // Blue's swarms on its left lane, Blue's splash placed so that RED's splash
    // twins land on that same lane (and vice versa): Valkyrie (self-centred
    // splash), Wizard (projectile splash), Baby Dragon (air projectile splash)
    // against a Skeleton Army (14) and Minions (3, air). Red spawns in reverse.
    let units = [
        Unit { card: "SkeletonArmy", at: (350, 1150), tick: 0 },
        Unit { card: "Minions", at: (400, 1200), tick: 10 },
        Unit { card: "Valkyrie", at: (1450, 2000), tick: 10 },
        Unit { card: "Wizard", at: (1450, 2250), tick: 20 },
        Unit { card: "BabyDragon", at: (1500, 2150), tick: 20 },
    ];
    let (s, eng) = run_mirror("splash_swarm", &units, true, symmetric_config(), Some(2400));
    assert!(eng.troop_died, "splash_swarm: nothing died");
    assert_draw("splash_swarm", &s);
}

#[test]
fn mirror_multi_unit_archers_minions_skeletons() {
    // THE CASE A SINGLE-UNIT SUITE MISSES: multi-unit deploys. Formation siblings are spawned
    // in team_seq order, and every sibling tie-break (target key, coincident push,
    // yield key) reads that order -- so the formation must be laid out in each
    // team's own frame. Archers (2), Minions (3) and a Skeleton Army (14), each
    // side, placed so the two sides' formations collide on both lanes.
    let units = [
        Unit { card: "Archer", at: (400, 1000), tick: 0 },
        Unit { card: "SkeletonArmy", at: (1450, 2050), tick: 0 },
        Unit { card: "Minions", at: (350, 1250), tick: 15 },
        Unit { card: "Minions", at: (1400, 1950), tick: 15 },
        Unit { card: "Archer", at: (1500, 2300), tick: 40 },
    ];
    let (s, eng) = run_mirror("multi_unit", &units, true, symmetric_config(), Some(1500));
    assert!(eng.troop_died, "multi_unit: nothing died: {eng:?}");
    assert_draw("multi_unit", &s);
}

#[test]
fn mirror_multi_unit_on_the_centre_line() {
    // Deploys EXACTLY on x = W/2, where every "lower x" / engine-lane rule used to
    // pick engine-left for both seats. A Minions formation of 3 puts its third
    // Minion exactly on x = 9; a Knight and a Giant stand on it; Archers straddle it
    // at 8.5 / 9.5 (a sibling pair whose order is the formation's). Nothing in
    // sight, so each walks to the default tower its own lane picks.
    let units = [
        Unit { card: "Knight", at: (900, 1100), tick: 0 },
        Unit { card: "Minions", at: (900, 1300), tick: 0 },
        Unit { card: "Archer", at: (900, 900), tick: 5 },
        Unit { card: "Giant", at: (900, 1200), tick: 30 },
        Unit { card: "SkeletonArmy", at: (900, 1000), tick: 60 },
    ];
    for pm in [PathModel::LaneSnap, PathModel::GridAStar, PathModel::DiagonalLookahead] {
        let mut cfg = symmetric_config();
        cfg.path_model = pm;
        let name = format!("centre_line {pm:?}");
        let (s, eng) = run_mirror(&name, &units, true, cfg, Some(1400));
        // They engage their rotated twins near the centre before reaching a tower
        // (measured: no tower or building hit in 1400 ticks under LaneSnap); the
        // centre-line decisions are still compared every tick through each unit's
        // route and target. `centre_line_units_really_start_on_the_centre_line`
        // is the vacuity check for x = W/2.
        assert!(eng.troop_damaged, "{name}: did not engage: {eng:?}");
        assert_draw(&name, &s);
    }
}

#[test]
fn centre_line_units_really_start_on_the_centre_line() {
    // The scenario above is only a plant target if some unit stands exactly on
    // x = W/2 after the formation is laid out. Measure it.
    let mut s = BattleState::new(1, symmetric_config());
    s.spawn_unit(Team::Blue, "Minions", t(900, 1300), None).unwrap();
    s.spawn_unit(Team::Red, "Minions", mirror(&s, t(900, 1300)), None).unwrap();
    s.spawn_unit(Team::Blue, "Archer", t(900, 900), None).unwrap();
    s.tick();
    let w = s.arena().width;
    for team in [Team::Blue, Team::Red] {
        let on = find_live(&s, team, "Minions").iter().filter(|e| e.pos.x * 2 == w).count();
        assert_eq!(on, 1, "{team:?}: expected exactly one Minion on x = W/2");
    }
    let archers: Vec<i32> = find_live(&s, Team::Blue, "Archer").iter().map(|e| e.pos.x).collect();
    assert_eq!(archers.len(), 2);
    assert!(archers.iter().all(|x| (x * 2 - w).abs() == SUBTILE), "Archers should straddle x = W/2: {archers:?}");
}

#[test]
fn mirror_multi_unit_deploys_through_deploy_with_red_first() {
    // The path a reflection-only mirror rule breaks: real `deploy` calls (hand,
    // elixir, territory,
    // formation), Red deploying FIRST, multi-unit cards only, alternating own-left,
    // centre line and own-right placements.
    let deck = ["Archer", "Minions", "SkeletonArmy", "Archer", "Minions", "SkeletonArmy", "Knight", "Giant"];
    let mut cfg = symmetric_config();
    cfg.decks = [deck.iter().map(|x| x.to_string()).collect(), deck.iter().map(|x| x.to_string()).collect()];
    let mut s = BattleState::new(11, cfg);
    let mut plays = 0u32;
    let mut multi = 0u32;
    while !s.is_done() && s.tick_count() < 3000 {
        if s.tick_count() % 29 == 0 {
            let hand: Vec<String> = s.hand(Team::Blue).iter().map(|x| x.to_string()).collect();
            let pick = hand[(s.tick_count() / 29) as usize % hand.len()].clone();
            let x = [350, 900, 1450, 900][(plays % 4) as usize];
            let blue = t(x, 1050);
            let r1 = s.deploy(Team::Red, &pick, mirror(&s, blue));
            let r2 = s.deploy(Team::Blue, &pick, blue);
            assert_eq!(r1.is_ok(), r2.is_ok(), "tick {}: rotated deploys disagreed: {r1:?} vs {r2:?}", s.tick_count());
            if r2.is_ok() {
                plays += 1;
                multi += u32::from(card_stat(&s, &pick).count > 1);
            }
        }
        s.tick();
        if let Err(e) = check_mirror(&s) {
            panic!("multi-unit deploys: {e}\ncensus: {:?}", census(&s));
        }
    }
    println!("multi-unit deploys: ticks={} plays={plays} multi={multi} towers={:?}", s.tick_count(), s.tower_hp(Team::Blue));
    assert!(multi >= 10, "only {multi} multi-unit plays: not evidence");
    assert_draw("multi-unit deploys", &s);
}

#[test]
fn mirror_building_targeter_vs_defensive_building() {
    // Giant and Hog Rider placed ALREADY PAST the river (spawn_unit ignores deploy
    // zones), facing a Cannon and a Tesla; Musketeer support. Every
    // building-targeter must pick the rotated building at the same tick.
    let units = [
        Unit { card: "Cannon", at: (700, 1000), tick: 0 },
        Unit { card: "Tesla", at: (1100, 1000), tick: 0 },
        Unit { card: "Giant", at: (350, 1900), tick: 30 },
        Unit { card: "HogRider", at: (1450, 2000), tick: 30 },
        Unit { card: "Musketeer", at: (400, 1850), tick: 40 },
    ];
    let (s, eng) = run_mirror("buildings", &units, true, symmetric_config(), Some(3000));
    assert!(eng.building_damaged && eng.troop_died, "buildings: did not engage: {eng:?}");
    assert_draw("buildings", &s);
}

#[test]
fn twin_giants_on_one_bridge_is_not_a_symmetric_configuration() {
    // A MEASUREMENT, pinned so it cannot change silently. "Exact mirror twins meet
    // head-on on the bridge and stay there, because reflection symmetry leaves no
    // rule to decide which one sidesteps" is true under a y-reflection, where a
    // unit's twin is on the SAME bridge. Under the rotation its twin is on the
    // OTHER bridge, so two identical Giants meeting head-on on one bridge are not
    // twins, and no symmetry forbids them passing. What happens is measured here,
    // not argued. Under `path::avoid_units` (this engine's own avoidance term, which
    // PathModel::Oracle2026 does not apply) the pair DEADLOCKS: seed 9, 1200 ticks,
    // both Giants at exactly the bridge centre x = 63000, at y 274500 and 301500. The
    // cause is geometry, not symmetry -- with equal YieldKeys that rule tries "toward
    // the centre line" then "away from it" for both, and a Giant's detour point
    // (blocker radius + own radius + clearance = 1.875 tiles beside the blocker) is
    // off the 2-tile bridge on both sides, so both fall back to aiming straight
    // ahead. A rule that would break it symmetrically -- each unit keeps to its OWN
    // right, which the rotation allows and the reflection does not -- is a mechanic
    // choice that wants measuring against the real game first. The assertion below is
    // the measured outcome of the CURRENT model.
    let mut s = BattleState::new(9, symmetric_config());
    s.spawn_unit(Team::Blue, "Giant", t(350, 1100), None).unwrap();
    s.spawn_unit(Team::Red, "Giant", same_lane_opposite(&s, t(350, 1100)), None).unwrap();
    let mid = s.arena().height / 2;
    // THE INVARIANT THAT SURVIVES WHATEVER THE AVOIDANCE MODEL IS: two ground discs
    // never overlap. The head-on OUTCOME below is a re-measured pin (they used to
    // deadlock, they now pass), so it protects nothing on its own; this does. It is
    // collide.rs's separation pass that owes it, and the pass is what resolves the
    // head-on pair now that PathModel::Oracle2026 applies no avoidance term.
    let (mut min_d2, mut need) = (i64::MAX, 0i64);
    let passed = run_until(&mut s, 1200, |s| {
        let b = find_live(s, Team::Blue, "Giant");
        let r = find_live(s, Team::Red, "Giant");
        if !b.is_empty() && !r.is_empty() {
            need = (b[0].radius + r[0].radius) as i64;
            min_d2 = min_d2.min(b[0].pos.dist2(r[0].pos));
        }
        !b.is_empty() && !r.is_empty() && b[0].pos.y > mid + SUBTILE && r[0].pos.y < mid - SUBTILE
    });
    // SLACK is one NATIVE unit (18 subtiles), not zero: the separation pass runs once
    // per tick, after the move, so it leaves a bounded integer residual rather than an
    // exact touch. Measured on this run the pair comes within 26 997.5 subtiles of a
    // required 27 000 -- an overlap of 2.5 subtiles, 0.14 native units. What the
    // assertion forbids is the thing that would be a real defect: two discs passing
    // THROUGH each other, which shows up as a gap of tiles, not of a seventh of a unit.
    const SLACK: i64 = 18;
    let floor = (need - SLACK) * (need - SLACK);
    assert!(
        min_d2 >= floor,
        "the two Giants overlapped by more than one native unit: closest dist2 = {min_d2}, \
         (r1+r2-1 native)^2 = {floor}"
    );
    let b = find_live(&s, Team::Blue, "Giant");
    let r = find_live(&s, Team::Red, "Giant");
    println!("twin giants on one bridge: passed_at={passed} blue={:?} red={:?}", b.first().map(|e| e.pos), r.first().map(|e| e.pos));
    // MEASURED, seed 9: under PathModel::Oracle2026 they PASS, at tick 174, Blue
    // ending half a tile right of the bridge centre and Red half a tile left of it.
    // (Under the older pathfinder plus `path::avoid_units` they deadlocked for all
    // 1200 ticks.) PathModel::Oracle2026 does NOT apply that avoidance term
    // (state.rs phase_path_2026): the
    // live game's avoidance is UNMEASURED -- it sets `avoidance_offset` to +-190 and
    // then walks it by +-10 per tick in a way no decay explains, and 16 of 36 runs
    // ramp back up before coming down (calibration movement.CONTACT_DOMAIN). An
    // invented deflection on top of the one law that IS measured would corrupt it.
    // So the pair is now resolved by collide.rs's separation pass alone, which
    // pushes two exactly head-on discs apart along the axis they are NOT stacked on,
    // and they slide past. Nothing forbids it: under the seat ROTATION a Giant's
    // twin is on the OTHER bridge, so two Giants on one bridge are not a symmetric
    // configuration (the paragraph above). The mirror comparators, which do compare
    // twins, stay green.
    assert_eq!(passed, 174, "the head-on resolution changed");
    assert_eq!((b.len(), r.len()), (1, 1));
    assert!(b[0].pos.y > mid && r[0].pos.y < mid, "both Giants should have crossed: {:?} {:?}", b[0].pos, r[0].pos);
}

#[test]
fn mirror_equidistant_target_tie() {
    // An EXACT distance tie, built so only the tie-break decides: a Blue Giant at
    // (9, 18) with two Red Cannons at the rotations of (7, 10) and (11, 10), i.e.
    // (11, 22) and (7, 22) -- edge distances identical to the subtile -- and the
    // rotation of all of it. The key (edge, candidate x, candidate y in the
    // attacker's frame, team_seq) sends each Giant to its OWN-LEFT Cannon: engine
    // (7, 22) for Blue, engine (11, 10) for Red. Blue's Cannons spawn x=7 first,
    // Red's in reverse, so a raw-slot tie-break sends the Giants to rotationally
    // DIFFERENT Cannons: the predecessor's bug, and the `id_tiebreak` plant.
    let units = [
        Unit { card: "Cannon", at: (700, 1000), tick: 0 },
        Unit { card: "Cannon", at: (1100, 1000), tick: 0 },
        Unit { card: "Giant", at: (900, 1800), tick: 0 },
    ];
    let (s, eng) = run_mirror("equidistant_tie", &units, true, symmetric_config(), Some(1200));
    assert!(eng.building_damaged, "equidistant_tie: the Giants never hit a Cannon: {eng:?}");
    assert_draw("equidistant_tie", &s);
}

#[test]
fn equidistant_tie_is_really_a_tie() {
    // The scenario above is only a plant target if the tie is exact. Measure it.
    // The Red Giant stands at the rotation of Blue (9, 18) = (9, 14).
    let giant = t(900, 1400);
    let (a, b) = (t(700, 1000), t(1100, 1000));
    assert_eq!(giant.dist2(a), giant.dist2(b));
    assert_ne!(a.x, b.x, "the tie must be between different x, or the real key cannot break it either");
    let s = BattleState::new(1, symmetric_config());
    assert_eq!(mirror(&s, t(900, 1800)), giant, "x = 9 is fixed by the rotation");
}

#[test]
fn mirror_holds_under_every_path_and_push_model() {
    // Symmetry must be a property of the architecture, not of the one model the
    // calibration currently selects.
    let units = [
        Unit { card: "Giant", at: (350, 1300), tick: 0 },
        Unit { card: "Knight", at: (1450, 1900), tick: 5 },
        Unit { card: "Archer", at: (300, 900), tick: 5 },
        Unit { card: "Cannon", at: (900, 1000), tick: 5 },
        Unit { card: "SkeletonArmy", at: (1450, 1100), tick: 40 },
        Unit { card: "Minions", at: (900, 1400), tick: 40 },
    ];
    for pm in [PathModel::LaneSnap, PathModel::GridAStar, PathModel::DiagonalLookahead] {
        for push in [PushModel::MassWeighted, PushModel::SpeedWeighted, PushModel::EqualSplit] {
            let mut cfg = symmetric_config();
            cfg.path_model = pm;
            cfg.push_model = push;
            let name = format!("models {pm:?}/{push:?}");
            let (s, eng) = run_mirror(&name, &units, true, cfg, Some(1600));
            assert!(eng.troop_died, "{name}: did not engage: {eng:?}");
            assert_draw(&name, &s);
        }
    }
}

/// A fully mirrored scripted battle: identical decks, identical policy, Red's
/// placements rotated and Red deploying FIRST on every play (so slot indices of
/// twins never line up), run to the end of overtime.
#[test]
fn mirror_scripted_full_battle_is_a_draw() {
    let deck = ["Knight", "Archer", "Musketeer", "Giant", "HogRider", "Minions", "Valkyrie", "Cannon"];
    let mut cfg = symmetric_config();
    cfg.decks = [deck.iter().map(|x| x.to_string()).collect(), deck.iter().map(|x| x.to_string()).collect()];
    let mut s = BattleState::new(5, cfg);
    let mut plays = 0u32;
    let mut checked = 0u32;
    while !s.is_done() {
        if s.tick_count() % 37 == 0 {
            let hand: Vec<String> = s.hand(Team::Blue).iter().map(|x| x.to_string()).collect();
            let pick = hand[(s.tick_count() / 37) as usize % hand.len()].clone();
            let c = card_stat(&s, &pick);
            let lane_x = if plays % 2 == 0 { 350 } else { 1450 };
            let blue = match c.kind {
                royalesim::card::CardKind::Building => t(900, 900 + 100 * (plays % 3) as i32),
                _ if c.target_only_buildings => t(lane_x, 1400),
                _ => t(lane_x, 1000),
            };
            let red = mirror(&s, blue);
            let r1 = s.deploy(Team::Red, &pick, red);
            let r2 = s.deploy(Team::Blue, &pick, blue);
            assert_eq!(r1.is_ok(), r2.is_ok(), "tick {}: mirrored deploys disagreed: {r1:?} vs {r2:?}", s.tick_count());
            if r2.is_ok() {
                plays += 1;
            }
        }
        s.tick();
        if let Err(e) = check_mirror(&s) {
            panic!("scripted mirror: {e}\ncensus: {:?}", census(&s));
        }
        checked += 1;
    }
    println!("scripted mirror: ticks={checked} plays={plays} outcome={:?} towers={:?}", s.outcome(), s.tower_hp(Team::Blue));
    assert!(plays >= 20, "only {plays} plays");
    assert_draw("scripted mirror", &s);
}

#[test]
fn mirror_check_itself_detects_an_asymmetry() {
    // The comparator is only evidence if it goes red on a real asymmetry: move
    // one Blue unit by one subtile and it must fire on that tick.
    let mut s = BattleState::new(1, symmetric_config());
    s.spawn_unit(Team::Blue, "Knight", t(350, 1100), None).unwrap();
    let p = mirror(&s, t(350, 1100));
    s.spawn_unit(Team::Red, "Knight", p, None).unwrap();
    for _ in 0..40 {
        s.tick();
        check_mirror(&s).unwrap();
    }
    let k = find_live(&s, Team::Blue, "Knight")[0];
    let (id, pos) = (k.id, k.pos);
    assert!(s.debug_set_pos(id, Vec2::new(pos.x, pos.y + 1)));
    assert!(check_mirror(&s).is_err(), "a one-subtile asymmetry passed the mirror check");
}

#[test]
fn mirror_check_rejects_the_reflection() {
    // The comparator must be the ROTATION and not accept the reflection: a Blue
    // Knight at (3.5, 11) and a Red Knight at the REFLECTION (3.5, 21) is not a
    // mirror position, and must fail on the first tick they exist.
    let mut s = BattleState::new(1, symmetric_config());
    s.spawn_unit(Team::Blue, "Knight", t(350, 1100), None).unwrap();
    s.spawn_unit(Team::Red, "Knight", same_lane_opposite(&s, t(350, 1100)), None).unwrap();
    s.tick();
    assert!(check_mirror(&s).is_err(), "the mirror check accepted a y-reflection");
}

// ---------------------------------------------------------------------------
// SPELLS under the rotation. Every scenario below casts with `spawn_unit`, so a Blue
// cast at (x, y) has a Red twin at the rotation, on the SAME tick -- both seats'
// spells materialise in one Spawn phase and land in one Projectile phase.

#[test]
fn mirror_fireballs_land_on_each_others_troops_on_one_tick() {
    // Each seat's Fireball lands on the other seat's Knight and Minions, which deploy
    // a few ticks before; one Knight stands EXACTLY on the impact (no radial direction:
    // the zero-vector rule, which must use the caster's forward axis), and a Giant
    // (IgnorePushback) takes damage only. Plant: zero_vector_plus_y.
    let units = [
        Unit { card: "Fireball", at: (900, 1900), tick: 0 },
        Unit { card: "Knight", at: (900, 1300), tick: 26 },
        Unit { card: "Minions", at: (1000, 1250), tick: 26 },
        Unit { card: "Giant", at: (750, 1350), tick: 26 },
    ];
    let (s, eng) = run_mirror("fireball_cross", &units, true, symmetric_config(), Some(400));
    assert!(eng.troop_damaged, "fireball_cross: the Fireballs hit nothing: {eng:?}");
    assert_draw("fireball_cross", &s);
}

#[test]
fn fireball_cross_really_lands_on_a_deploying_knight_at_the_impact() {
    // Vacuity for the scenario above: the Blue Fireball's arrival tick falls inside the
    // Red Knight's deploy window, the Knight sits exactly on the impact, and it moves
    // by exactly the caster-forward push.
    let mut s = BattleState::new(9, symmetric_config());
    let impact = t(900, 1900);
    s.spawn_unit(Team::Blue, "Fireball", impact, None).unwrap();
    let mut before: Option<(Vec2, bool)> = None;
    for k in 0..60u32 {
        if k == 26 {
            s.spawn_unit(Team::Red, "Knight", impact, None).unwrap();
        }
        let flying = !s.spells().is_empty();
        s.tick();
        let kn = find_live(&s, Team::Red, "Knight").first().map(|e| (e.pos, e.deploying));
        if flying && s.spells().is_empty() {
            let (b, d) = before.expect("the Knight did not exist before the Fireball landed");
            assert!(d, "the Knight was not deploying when the Fireball landed");
            assert_eq!(b, impact);
            let after = kn.unwrap().0;
            assert_eq!(after.x, impact.x);
            assert!(after.y > impact.y, "Blue's zero-vector push must go along Blue's forward (+y): {after:?}");
            return;
        }
        before = kn;
    }
    panic!("the Fireball never landed");
}

#[test]
fn mirror_push_to_exactly_mid_river_ejects_to_each_seats_own_bank() {
    // A Knight pushed straight across the bank by exactly Pushback so it stops on the
    // river's centre line, away from both bridges: the two banks are EXACTLY equidistant
    // and the tie is broken toward the unit's own side, in its own frame. The Red Knight
    // is Blue's victim and vice versa. Plant: eject_tie_engine_frame.
    let s0 = BattleState::new(9, symmetric_config());
    let a = s0.arena().clone();
    let mid = (a.water_y_min + a.water_y_max) / 2;
    // The pushing spell: the first whose Pushback carries a bank-side victim to the
    // centre line from dry ground -- the 2018 Fireball (1800 native); the 15.535
    // Fireball pushes 1000, one tile, exactly the half-river, so the Rocket (1800)
    // stands in there. Read from the loaded cards, never pasted.
    let db = s0.cards();
    let push_of = |name: &str| match &db.get(db.index(name).unwrap()).spell.as_ref().unwrap().shape {
        royalesim::card::SpellShape::Projectile { hit: Some(h), .. } => h.knockback.map_or(0, |k| k.distance),
        other => panic!("{other:?}"),
    };
    let (spell, push) = ["Fireball", "Rocket"]
        .into_iter()
        .map(|n| (n, push_of(n)))
        .find(|(_, p)| a.is_passable_ground(Vec2::new(t(900, 0).x, mid + p)))
        .expect("no simulable spell pushes a victim from dry ground to the river's centre line");
    // Red victim on the Red bank: centre at mid + push, the impact one tile behind it.
    let victim = Vec2::new(t(900, 0).x, mid + push);
    assert!(a.is_passable_ground(victim) && !a.is_passable_ground(Vec2::new(victim.x, mid)), "geometry: victim on dry ground, centre line on water");
    let impact = Vec2::new(victim.x, victim.y + SUBTILE);
    // Blue frame hundredths for the Unit table (the Red victim is the rotation of a
    // Blue unit at (W - x, H - y); the Blue cast lands on the Red side at `impact`).
    let h = |p: Vec2| ((p.x * 100 / SUBTILE), (p.y * 100 / SUBTILE));
    assert_eq!(Vec2::from_tiles_100(h(impact).0, h(impact).1), impact, "impact must be on the hundredth-tile grid");
    let blue_victim = mirror(&s0, victim);
    assert_eq!(Vec2::from_tiles_100(h(blue_victim).0, h(blue_victim).1), blue_victim, "victim must be on the hundredth-tile grid");
    let units = [Unit { card: spell, at: h(impact), tick: 0 }, Unit { card: "Knight", at: h(blue_victim), tick: 26 }];
    let (s, eng) = run_mirror("mid_river_eject", &units, true, symmetric_config(), Some(300));
    assert!(eng.troop_damaged, "mid_river_eject: nothing hit: {eng:?}");
    assert_draw("mid_river_eject", &s);
}

#[test]
fn mirror_logs_roll_through_each_other_on_the_centre_column() {
    // Both seats' Logs on x = 9 (fixed by the rotation), rolling toward each other
    // through deploying enemy troops on both sides of the river and past each other.
    // Plant: rolling_forward_plus_y_for_both.
    let units = [
        Unit { card: "Log", at: (900, 1100), tick: 0 },
        Unit { card: "Knight", at: (900, 1950), tick: 2 },
        Unit { card: "Giant", at: (1050, 2150), tick: 2 },
        Unit { card: "Skeletons", at: (800, 1800), tick: 6 },
    ];
    let (s, eng) = run_mirror("log_vs_log", &units, true, symmetric_config(), Some(500));
    assert!(eng.troop_damaged || eng.troop_died, "log_vs_log: the Logs hit nothing: {eng:?}");
    assert_draw("log_vs_log", &s);
}

#[test]
fn mirror_zap_arrows_and_goblin_barrels_from_both_seats() {
    // A Knight each fighting a Cannon, both zapped on one tick; Arrows onto each side's
    // Minions; Goblin Barrels onto each other's own-left princess tower (formation laid
    // out in each caster's frame). Plant: reflection_formation.
    let units = [
        Unit { card: "Cannon", at: (900, 1000), tick: 0 },
        Unit { card: "Knight", at: (900, 1990), tick: 0 },
        Unit { card: "Zap", at: (900, 1990), tick: 30 },
        Unit { card: "Minions", at: (1400, 2000), tick: 10 },
        Unit { card: "Arrows", at: (1400, 2000), tick: 20 },
        Unit { card: "GoblinBarrel", at: (350, 2550), tick: 40 },
    ];
    let (s, eng) = run_mirror("zap_arrows_barrel", &units, true, symmetric_config(), Some(900));
    assert!(eng.troop_damaged && eng.tower_damaged, "zap_arrows_barrel: did not engage: {eng:?}");
    assert!(s.entities().any(|e| e.card == "Goblin") || eng.troop_died, "no Goblin was ever released");
    assert_draw("zap_arrows_barrel", &s);
}

#[test]
fn the_shipped_search_is_absolute_grid_not_seat_symmetric() {
    // THE MEASURED EXCEPTION to this file's claim, pinned so nobody "fixes" it back:
    // the game's pathfinder (path16402.rs) scans and expands in absolute arena
    // coordinates, so the rotated twin of a Blue problem is answered with a route
    // that is NOT the rotation of Blue's answer whenever several routes tie on
    // cost. Every Knight start cell of the lane-sweep experiment is tried from
    // both seats with the six crown towers on the board; some routes must differ
    // from their twin's rotation (and some even end on a different, equally near
    // goal cell). Under the trace-fitted arm the same sweep is 100% rotated.
    use royalesim::arena::{Arena, Shape};
    use royalesim::path::{FrameWorld, NavRequest, Obstacle};
    use royalesim::path2026;
    use royalesim::state::Calib;
    use royalesim::EntityId;
    let arena = Arena::shipped();
    let towers = [(9000, 3000, 1400), (3500, 6500, 1000), (14500, 6500, 1000), (9000, 29000, 1400), (3500, 25500, 1000), (14500, 25500, 1000)];
    let plan = |calib: &Calib, team: Team, pos_abs: Vec2, goal_abs: Vec2| -> Vec<(i32, i32)> {
        let obstacles: Vec<Obstacle> = towers
            .iter()
            .enumerate()
            .map(|(i, &(x, y, r))| {
                let c = arena.to_frame(team, Vec2::new(x * 18, y * 18));
                Obstacle { id: EntityId { index: i as u32, generation: 0 }, shape: Shape::Circle { c, r: r * 18 }, radius: r * 18, key: (0, 0, 0, 0, i as u32), ally: true }
            })
            .collect();
        let world = FrameWorld { arena: &arena, obstacles: &obstacles };
        let req = NavRequest {
            #[cfg(clash_plant = "reflection_bridge_tie")]
            red: team == Team::Red,
            team,
            pos: arena.to_frame(team, pos_abs),
            goal: arena.to_frame(team, goal_abs),
            radius: 500 * 18,
            sight: 0,
            step: 0,
            reach: 1700 * 18,
            flying: false,
            target_flying: false,
            jumper: false,
            ignore: None,
        };
        let (cells, ok) = path2026::plan_cells(&world, calib, &req);
        assert!(ok);
        // back to absolute cells
        cells.into_iter().map(|(c, r)| match team { Team::Blue => (c, r), Team::Red => (arena.cols - 1 - c, arena.rows - 1 - r) }).collect()
    };
    let rot = |cells: &[(i32, i32)]| -> Vec<(i32, i32)> { cells.iter().map(|&(c, r)| (arena.cols - 1 - c, arena.rows - 1 - r)).collect() };
    for (arm, expect_differing) in [(PathSearch::Client16402, true), (PathSearch::TraceFittedAstar, false)] {
        let mut calib = Calib::shipped();
        calib.path_search = arm;
        let (mut differing, mut length_differing, mut total) = (0, 0, 0);
        for col in (0..18).step_by(2) {
            for row in (2..14).step_by(2) {
                // mid-cell, never on a cell boundary: a point ON a boundary rotates
                // into the neighbouring cell (500 -> 17500 is the next cell's edge)
                // and the twin would start one cell over, which is not a twin
                let blue_pos = Vec2::new((col * 1000 + 750) * 18, (row * 1000 + 750) * 18);
                let blue_goal = Vec2::new(3500 * 18, 25500 * 18);
                let red_pos = arena.rotate(blue_pos);
                let red_goal = arena.rotate(blue_goal);
                let b = plan(&calib, Team::Blue, blue_pos, blue_goal);
                let r = plan(&calib, Team::Red, red_pos, red_goal);
                total += 1;
                if rot(&b) != r {
                    differing += 1;
                    if b.len() != r.len() {
                        // the goal CELL differs too: the game's goal scan breaks equidistant
                        // goal candidates by absolute scan order, so a twin can be
                        // sent to a different (equally near) cell and walk one more
                        // or one fewer step
                        length_differing += 1;
                    }
                }
            }
        }
        println!("{arm:?}: {differing}/{total} rotated twins take a different route ({length_differing} of them to a different goal cell)");
        if expect_differing {
            assert!(differing > 0, "the shipped search became seat-symmetric -- it is not, in the game");
        } else {
            assert_eq!(differing, 0, "the trace-fitted arm must stay seat-symmetric by construction");
        }
    }
}
