//! transform.DISMOUNT_MOUNT_BIRTH: where the Hero Dark Prince's mount is born and whether it takes a creation-tick update
//! (state.rs the Dismount effect, `dismount_hops`, `materialise_released`).
//!
//! THE LAW, measured on client 15.535.29 (the ledger has the rows): born on the hero's point before its first hop, taking
//! a full update that tick which meets the hero's body: sp-hero2-DarkPrince-still-s0, the hero standing on (14500, 11500),
//! its mount's first frame (14451, 11414) with an avoidance offset of -190.
//!
//! THE SCENE (tests/hero_dark_prince.rs's): Blue's level-11 hero Dark Prince stood up and held on (14500, 11500), pressed.
//! WHAT IS PINNED, on the mount's first frame:
//!   1. client15535_hero_point: it stands on (14451, 11414) with an avoidance offset of -190;
//!   2. fitted_offset (the old arm, the vacuity check): its avoidance offset is 0 (it took no update);
//!   3. the shipped value is fitted_offset.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test dismount_mount_birth`):
//!   * `mount_meets_no_hero` -- the new arm's mount meets no hero on its first update: (1) goes red.
//!   * `mount_born_on_trigger` -- the new arm still puts the mount down on the trigger: (1) goes red.
//!   * `mount_acquired_at_once` -- transform.DISMOUNT_MOUNT_ACQUIRE's new arm releases the mount with no delay:
//!     `the_mount_waits_for_its_8th_frame_under_client15535_8th_frame` goes red.
//!   * `mount_hold_collides` -- transform.DISMOUNT_MOUNT_HOLD_BODY's new arm's held mount still meets bodies:
//!     `the_held_mount_meets_no_body_under_client15535_no_body` goes red.
//!
//! transform.DISMOUNT_MOUNT_HOLD_BODY = client15535_no_body (client 15.535.29, sp-form-DarkPrince-hero-s0: the pair touching
//! from the hero's 1000 ms on, and neither moved before T + 23): the held mount meets no body until its hold ends.
//!
//! transform.DISMOUNT_MOUNT_ACQUIRE = client15535_8th_frame (client 15.535.29: every enemy's first take of a mount on its
//! F + 7 or later): no enemy may take the mount before its F + 7; at_once from its first frame on.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DismountMountAcquire, DismountMountBirth, DismountMountHoldBody};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["DarkPrince", "Knight", "Archers", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
const HERO: &str = "DarkPrince_hero";
const MOUNT: &str = "DarkPrinceHero_Mount";

/// The mount's first-frame point (native) and avoidance offset under `arm`.
fn mount_first_frame(arm: DismountMountBirth) -> ((i32, i32), i32) {
    let mut cfg = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dismount_mount_birth = arm;
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let at = n(14500, 11500);
    s.deploy(Team::Blue, "DarkPrince", at).expect("the play");
    let mut stood = false;
    for _ in 0..120 {
        s.tick();
        let found = find_live(&s, Team::Blue, HERO).first().map(|e| (e.id, e.deploying));
        if let Some((id, deploying)) = found {
            assert!(s.debug_set_pos(id, at));
            if !deploying {
                stood = true;
                break;
            }
        }
    }
    assert!(stood, "{arm:?}: the scene drifted: no hero stood up");
    s.press_ability_button(Team::Blue, 0).expect("the press, on the hero");
    for _ in 0..40 {
        s.tick();
        if let Some(m) = find_live(&s, Team::Blue, MOUNT).first() {
            return ((m.pos.x / K, m.pos.y / K), m.avoid_offset);
        }
    }
    panic!("{arm:?}: the scene drifted: no mount in 40 ticks");
}

/// Plants: mount_meets_no_hero, mount_born_on_trigger.
#[test]
fn the_mount_is_born_on_the_hero_and_takes_its_first_update_under_client15535_hero_point() {
    let (p, offset) = mount_first_frame(DismountMountBirth::Client15535HeroPoint);
    assert_eq!(offset, -190, "new: the mount's first-frame turn off the hero's body (at {p:?})");
    assert_eq!(p, (14451, 11414), "new: the mount's first-frame point (offset {offset})");
}

#[test]
fn the_old_value_takes_no_first_update() {
    assert_eq!(mount_first_frame(DismountMountBirth::FittedOffset).1, 0, "old: the mount turned on its first frame (vacuous otherwise)");
}

#[test]
fn the_shipped_value_is_fitted_offset() {
    assert_eq!(Calib::shipped().dismount_mount_birth, DismountMountBirth::FittedOffset);
}

/// The mount's first frame and the first tick an enemy may take it (EntityView::acquirable_from), under
/// transform.DISMOUNT_MOUNT_ACQUIRE = `arm` (DISMOUNT_MOUNT_BIRTH at the measured arm).
fn mount_acquirable(arm: DismountMountAcquire) -> (u32, u32) {
    let mut cfg = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dismount_mount_birth = DismountMountBirth::Client15535HeroPoint;
    cfg.calib.dismount_mount_acquire = arm;
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let at = n(14500, 11500);
    s.deploy(Team::Blue, "DarkPrince", at).expect("the play");
    let mut stood = false;
    for _ in 0..120 {
        s.tick();
        let found = find_live(&s, Team::Blue, HERO).first().map(|e| (e.id, e.deploying));
        if let Some((id, deploying)) = found {
            assert!(s.debug_set_pos(id, at));
            if !deploying {
                stood = true;
                break;
            }
        }
    }
    assert!(stood, "{arm:?}: the scene drifted: no hero stood up");
    s.press_ability_button(Team::Blue, 0).expect("the press, on the hero");
    for _ in 0..40 {
        s.tick();
        if let Some(m) = find_live(&s, Team::Blue, MOUNT).first() {
            return (s.tick_count() - 1, m.acquirable_from);
        }
    }
    panic!("{arm:?}: the scene drifted: no mount in 40 ticks");
}

/// Plant: mount_acquired_at_once.
#[test]
fn the_mount_waits_for_its_8th_frame_under_client15535_8th_frame() {
    let (f, old) = mount_acquirable(DismountMountAcquire::AtOnce);
    // NOT VACUOUS: at_once lets an enemy take it within a tick of its first frame.
    assert!(old <= f + 1, "at_once: the mount is not acquirable at once (first frame {f}, acquirable from {old})");
    let (f, new) = mount_acquirable(DismountMountAcquire::Client15535EighthFrame);
    assert_eq!(new, f + 7, "client15535_8th_frame: the mount is not acquirable from its F + 7 (first frame {f})");
}

/// The ticks, from the mount's first frame F, on which it moved, F + 2 .. F + 30, the hero put back on its point before
/// every tick (its hops undone, as the river's edge undid them in sp-form-DarkPrince-hero-s0) so that the pair stand
/// touching, under transform.DISMOUNT_MOUNT_HOLD_BODY = `arm` (the birth and the acquire delay at the measured arms).
fn mount_moves(arm: DismountMountHoldBody) -> Vec<u32> {
    let mut cfg = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dismount_mount_birth = DismountMountBirth::Client15535HeroPoint;
    cfg.calib.dismount_mount_acquire = DismountMountAcquire::Client15535EighthFrame;
    cfg.calib.dismount_mount_hold_body = arm;
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let at = n(14500, 11500);
    s.deploy(Team::Blue, "DarkPrince", at).expect("the play");
    let mut hero = None;
    for _ in 0..120 {
        s.tick();
        if let Some((id, deploying)) = find_live(&s, Team::Blue, HERO).first().map(|e| (e.id, e.deploying)) {
            assert!(s.debug_set_pos(id, at));
            if !deploying {
                hero = Some(id);
                break;
            }
        }
    }
    let hero = hero.unwrap_or_else(|| panic!("{arm:?}: the scene drifted: no hero stood up"));
    s.press_ability_button(Team::Blue, 0).expect("the press, on the hero");
    let (mut first, mut last, mut moves) = (None, None, Vec::new());
    for _ in 0..60 {
        if s.entity(hero).is_some() {
            assert!(s.debug_set_pos(hero, at));
        }
        s.tick();
        let Some(m) = find_live(&s, Team::Blue, MOUNT).first().map(|e| e.pos) else { continue };
        let now = s.tick_count() - 1;
        let f = *first.get_or_insert(now);
        if last.is_some_and(|p| p != m) && now >= f + 2 && now <= f + 30 {
            moves.push(now - f);
        }
        last = Some(m);
    }
    assert!(first.is_some(), "{arm:?}: the scene drifted: no mount");
    moves
}

/// Plant: mount_hold_collides.
#[test]
fn the_held_mount_meets_no_body_under_client15535_no_body() {
    // The client's: no move before F + 23 (sp-form-DarkPrince-hero-s0: first frame t213, walking t236).
    let new = mount_moves(DismountMountHoldBody::Client15535NoBody);
    assert_eq!(new.first(), Some(&23), "client15535_no_body: the held mount moved before its hold ended ({new:?})");
    // NOT VACUOUS: collides pushes it off the hero during its hold, once the hero's 1000 ms are over.
    let old = mount_moves(DismountMountHoldBody::Collides);
    assert!(old.first().is_some_and(|&k| k < 23), "collides: the held mount was not pushed during its hold ({old:?})");
}
