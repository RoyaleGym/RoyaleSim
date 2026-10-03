//! THE HERO DARK PRINCE'S DISMOUNT BY THE RIVER: transform.DISMOUNT_LEAP_STEP (a press freed on the leap's last tick)
//! and transform.DISMOUNT_HOP_WATER (a hop onto the river). state.rs the dismount's trigger, `dismount_hops`,
//! `land_row_centre`.
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, sp-form-DarkPrince-hero-s0: the hero pressed mid-leap took
//! the leap's last step (160 toward its landing node) on its trigger tick t213, then hopped; its five hops onto the river
//! landed on (10676, 17250), the centre of the land row [17000, 17500).
//!
//! WHAT IS PINNED, and the plants that turn it red:
//!   1. dismount_rebinds_before_leap_step: a blue hero walking up from (10500, 13000) leaps the river; pressed mid-leap,
//!      under client15535_leap_lands_first its move on the trigger tick (its point less the hop's 200 back) is the leap's
//!      step, over 100; under rebind_first (the engine's, the vacuity check) a walk step, 70 at most.
//!   2. hop_keeps_water: a blue hero set down on (10500, 17250), just past the river, and pressed: under
//!      client15535_land_row_centre no hop leaves it on the river and one lands on y 17250 exactly; under keep_water it
//!      stands on the river.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DismountHopWater, DismountLeapStep};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["DarkPrince", "Knight", "Archers", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
const HERO: &str = "DarkPrince_hero";
const MOUNT: &str = "DarkPrinceHero_Mount";

/// A blue Hero Dark Prince played at (10500, 13000), standing: the battle and the hero.
fn hero(leap: DismountLeapStep, water: DismountHopWater) -> (BattleState, EntityId) {
    let mut cfg = config();
    cfg.calib.dismount_leap_step = leap;
    cfg.calib.dismount_hop_water = water;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "DarkPrince", n(10500, 13000)).expect("the play");
    for _ in 0..120 {
        s.tick();
        if let Some((id, deploying)) = find_live(&s, Team::Blue, HERO).first().map(|e| (e.id, e.deploying)) {
            if !deploying {
                return (s, id);
            }
        }
    }
    panic!("no hero stood up");
}

/// Pressed mid-leap: the hero's move on the trigger tick (native), its point less the hop's 200 back.
fn trigger_tick_move(leap: DismountLeapStep) -> i32 {
    let (mut s, h) = hero(leap, DismountHopWater::KeepWater);
    let mut k = 0;
    while !s.entity(h).expect("the hero").jumping {
        s.tick();
        k += 1;
        assert!(k < 300, "the scene drifted: the hero never leapt");
    }
    for _ in 0..3 {
        s.tick();
    }
    assert!(s.entity(h).expect("the hero").jumping, "the scene drifted: the leap ended before the press");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press, mid-leap");
    for _ in 0..80 {
        let before = s.entity(h).expect("the hero").pos;
        s.tick();
        if !find_live(&s, Team::Blue, MOUNT).is_empty() {
            let after = s.entity(h).expect("the hero").pos;
            let pre_hop = Vec2::new(after.x, after.y + 200 * K);
            return pre_hop.dist(before) / K;
        }
    }
    panic!("the scene drifted: no dismount");
}

/// Plant: dismount_rebinds_before_leap_step.
#[test]
fn a_hero_freed_on_its_leaps_last_tick_takes_the_leap_step_first_under_client15535_leap_lands_first() {
    let m = trigger_tick_move(DismountLeapStep::Client15535LeapLandsFirst);
    assert!(m > 100, "client15535_leap_lands_first: the trigger tick's move is {m}, not the leap's step");
    // NOT VACUOUS: the engine's arm walks it.
    let m = trigger_tick_move(DismountLeapStep::RebindFirst);
    assert!(m <= 70, "rebind_first: the trigger tick's move is {m}, not a walk step");
}

/// Set down past the river and pressed: the hero's points over the hops.
fn hop_points(water: DismountHopWater) -> (BattleState, Vec<Vec2>) {
    let (mut s, h) = hero(DismountLeapStep::RebindFirst, water);
    assert!(s.debug_set_pos(h, n(10500, 17250)));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut pts = Vec::new();
    for _ in 0..14 {
        s.tick();
        pts.push(s.entity(h).expect("the hero").pos);
    }
    (s, pts)
}

/// Plant: hop_keeps_water.
#[test]
fn a_hop_onto_the_river_lands_on_the_nearest_land_rows_centre_under_client15535_land_row_centre() {
    let (s, pts) = hop_points(DismountHopWater::Client15535LandRowCentre);
    let arena = &s.config().arena;
    assert!(pts.iter().all(|p| !arena.is_water(*p)), "client15535_land_row_centre: a hop left the hero on the river: {pts:?}");
    assert!(pts.iter().any(|p| p.y == 17250 * K), "client15535_land_row_centre: no hop landed on y 17250: {pts:?}");
    // NOT VACUOUS: the engine's arm hops it onto the river.
    let (s, pts) = hop_points(DismountHopWater::KeepWater);
    let arena = &s.config().arena;
    assert!(pts.iter().any(|p| arena.is_water(*p)), "keep_water: no hop reached the river: {pts:?}");
}
