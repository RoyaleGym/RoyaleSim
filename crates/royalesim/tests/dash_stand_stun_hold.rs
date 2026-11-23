//! combat.DASH_STAND_STUN_HOLD: what a hold (a stun, a freeze, a Hero Ice Wizard's cube) does to a dasher's stand (state.rs
//! `phase_path16402_for`'s frozen branch and dash block, Entities::dash_paused).
//!
//! THE LAW, measured on client 16.635 (parity r98, the oracle's il-mg-16635 set: 2 of 2 Mega Knight stands a cube caught):
//! the hold pauses the stand, its ms count (combat.DASH_STAND_CLOCK = client16635_hit_speed_scaled) kept, the landing tick
//! counted; nothing counts while it is held; the first unheld tick the unit holds a live target and no attack resumes it and
//! counts; the tick after a cube lets it go (no target decision) keeps it paused while it walks.
//!
//! WHAT IS PINNED, on tests/dash_stand_clock.rs's scene (a Blue Mega Knight put down inside its trigger of a Red Knight):
//!   1. a Zap on the standing Mega Knight: under the new arm it jumps 18 counted ticks after its first stand tick, the held
//!      ticks left out; under ends (the vacuity check) a fresh stand runs from its retake;
//!   2. a Hero Ice Wizard's cube on a standing Red Mega Knight: under the new arm it jumps sooner after the let-go than a fresh
//!      stand would;
//!   3. (1)'s Zap with the Knight brought into the Mega Knight's reach while the stun holds: under the new arm the paused stand
//!      takes no swing on the first unheld tick and stands again (combat.DASH_STAND_ATTACK = client16635_stand_holds_attack: a
//!      stand holds the attack); under ends (the vacuity check) he swings at the Knight on that tick.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! dash_stand_stun_hold`):
//!   * `dash_stand_hold_ends` -- the new arm still ends a held stand: (1), (2) and (3) go red.
//!   * `dash_hold_no_ice_grace` -- the let-go tick ends the paused stand: (2) goes red.
//!   * `dash_pause_swings` -- a paused stand swings at a target in reach on its first unheld tick: (3) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, FORM_HERO};
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{
    BattleConfig, BattleState, CardTable, DashAttack, DashFirstSightTrigger, DashStandClock, DashStandStunHold, IceReleaseTarget,
};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const MK_AT: (i32, i32) = (9500, 11500);
const KNIGHT_AT: (i32, i32) = (13126, 14653);

fn big_step(a: Vec2, b: Vec2) -> bool {
    a.dist2(b) > (200 * K as i64).pow(2)
}

/// The Zap scene under `arm`: (k, the stand ticks counted before the hold; s0, the first held tick; e0, the first unheld tick
/// after it; r, the first tick from e0 the Mega Knight holds the Knight; j, its jump).
fn zapped(arm: DashStandStunHold) -> (u32, u32, u32, u32, u32) {
    let mut cfg = config();
    cfg.calib.dash_attack = DashAttack::ClientDash;
    cfg.calib.dash_first_sight_trigger = DashFirstSightTrigger::FirstSightTick;
    cfg.calib.dash_stand_clock = DashStandClock::Client16635HitSpeedScaled;
    cfg.calib.dash_stand_stun_hold = arm;
    let mut s = BattleState::new(0, cfg);
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at(KNIGHT_AT), None), (Team::Blue, "MegaKnight", at(MK_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (knight, mk) = (ids[0], ids[1]);
    let kmax = s.entity(knight).expect("the Knight").max_hp;
    let mut rows: Vec<(Vec2, i32, Option<EntityId>, i64)> = Vec::new();
    let mut prev = s.entity(mk).expect("the Mega Knight").pos;
    let mut steps = Vec::new();
    for t in 0..60u32 {
        if t == 5 {
            let p = s.entity(mk).expect("the Mega Knight").pos;
            s.spawn_unit(Team::Red, "Zap", p, None).expect("the Zap");
        }
        assert!(s.debug_set_pos(knight, at(KNIGHT_AT)) && s.debug_set_hp(knight, kmax));
        s.tick();
        let m = s.entity(mk).expect("the Mega Knight");
        steps.push(big_step(prev, m.pos));
        prev = m.pos;
        rows.push((m.pos, m.stun_ms, m.target, s.entity_extras(mk).expect("its extras")[2]));
    }
    let k = rows.iter().take_while(|r| r.3 == 1).count() as u32;
    let s0 = rows.iter().position(|r| r.1 > 0).expect("the scene drifted: the Zap held nothing") as u32;
    let e0 = (s0 as usize..rows.len()).find(|&t| rows[t].1 == 0).expect("the hold never ended") as u32;
    let r = (e0 as usize..rows.len()).find(|&t| rows[t].2 == Some(knight)).expect("the Knight never retaken") as u32;
    let j = (r as usize..rows.len()).find(|&t| steps[t]).expect("no jump after the retake") as u32;
    (k, s0, e0, r, j)
}

/// combat.DASH_STAND_STUN_HOLD = client16635_stand_kept_while_held: a Zap's stun on the standing Mega Knight pauses its stand;
/// it jumps when the ticks it counted before the hold and from its retake make 18 (900 ms at 50); under ends (the vacuity
/// check) a fresh stand runs 18 ticks from the retake. Plant: dash_stand_hold_ends.
#[test]
fn under_client16635_stand_kept_while_held_a_zapped_stand_runs_on_after_the_stun() {
    let (k0, s00, e00, r0, j0) = zapped(DashStandStunHold::Ends);
    assert!((3..=12).contains(&k0) && e00 >= s00 + 5 && j0 > r0, "the scene drifted (ends): k {k0} s0 {s00} e0 {e00} r {r0} j {j0}");
    assert_eq!(j0, r0 + 17, "ends (the vacuity check): a fresh stand from the retake (k {k0} s0 {s00} e0 {e00} r {r0})");
    let (k, s0, e0, r, j) = zapped(DashStandStunHold::Client16635StandKeptWhileHeld);
    assert_eq!((k, s0, e0, r), (k0, s00, e00, r0), "the same scene up to the retake under both arms");
    // The stun's clock reads 0 after tick e0, but its last 50 ms held e0's path phase: e0 + 1 is the first unheld tick, and
    // the Zap's cleared target is retaken on it.
    assert_eq!(r, e0 + 1, "the Knight is held again on the first unheld tick (s0 {s0} e0 {e0})");
    assert_eq!(j, r + 17 - k, "client16635_stand_kept_while_held: 18 counted ticks in all (k {k} before the hold, r {r}, j {j})");
}

const DECK: [&str; 8] = ["IceWizard", "Knight", "Archer", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
const HERO: &str = "IceWizard_hero";
const CUBE: &str = "IceWizardHero_IceCube";
/// On blue's side, the hero 4,000 ahead of blue's King, the scene short of the river.
const AT: (i32, i32) = (9000, 7000);

/// A battle on the current client's table: blue's deck holds the Hero Ice Wizard, blue's princess towers down
/// (tests/hero_ice_wizard.rs's `battle_with`).
fn ice_battle(arm: DashStandStunHold) -> BattleState {
    let mut cfg = BattleConfig::with_cards(CardDb::load_table(CardTable::Client160402017).expect("the 160402017 table"));
    cfg.calib.card_table = CardTable::Client160402017;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dash_attack = DashAttack::ClientDash;
    cfg.calib.dash_first_sight_trigger = DashFirstSightTrigger::FirstSightTick;
    cfg.calib.dash_stand_clock = DashStandClock::Client16635HitSpeedScaled;
    cfg.calib.ice_release_target = IceReleaseTarget::Client16635TargetNextTick;
    cfg.calib.dash_stand_stun_hold = arm;
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    s
}

/// What the cube scene gave: the first cube frame, the cube's last frame, whether the Mega Knight stood on the tick before the
/// cube, its target and step on the last frame + 2 (the let-go tick), and its jump after the let-go.
#[derive(Debug, PartialEq)]
struct Cubed {
    first: u32,
    last: u32,
    stood: bool,
    target2: Option<EntityId>,
    moved2: bool,
    jump: u32,
}

/// The cube scene under `arm`, the Red Mega Knight put down at `m` (native millitiles; (9000, 12450) is 5,450 from the hero),
/// a Blue Giant held at `g` (5,300 from it, nearer than the hero, inside its trigger): the hero
/// shoots the deploying Mega Knight (its mark), Blue presses on the shot's landing, and the cube freezes the Mega Knight's
/// stand. None
/// when the scene does not give a stand the cube caught.
fn cubed(arm: DashStandStunHold, m: (i32, i32), g: (i32, i32)) -> Option<Cubed> {
    let mut s = ice_battle(arm);
    s.spawn_unit(Team::Blue, HERO, at(AT), None).expect("the hero");
    s.tick();
    let hero = find_live(&s, Team::Blue, HERO)[0].id;
    while s.tick_count() < 95 {
        assert!(s.debug_set_pos(hero, at(AT)));
        s.tick();
    }
    let giant = s.scenario_spawn_now(Team::Blue, "Giant", at(g), None).expect("the Giant");
    let gmax = s.entity(giant).expect("the Giant").max_hp;
    // put down deploying (its deploy outlasts the hero's first shot, which marks it), so its stand begins after the mark
    s.spawn_unit_resolved(Team::Red, "MegaKnight", at(m), None).expect("the Mega Knight");
    s.tick();
    let mk = find_live(&s, Team::Red, "MegaKnight").first().map(|e| e.id).expect("the Mega Knight");
    let hp0 = s.entity(mk).expect("the Mega Knight").max_hp;
    let (mut pressed, mut first, mut last, mut stood) = (false, None, None, false);
    let (mut target2, mut moved2) = (None, false);
    let mut prev_state = 0;
    let mut prev = s.entity(mk).expect("the Mega Knight").pos;
    for _ in 0..260 {
        assert!(s.debug_set_pos(giant, at(g)) && s.debug_set_hp(giant, gmax) && s.debug_set_pos(hero, at(AT)));
        s.tick();
        let t = s.tick_count() - 1;
        let me = s.entity(mk)?;
        let (pos, hp, target) = (me.pos, me.hp, me.target);
        let state = s.entity_extras(mk)?[2];
        let step = big_step(prev, pos);
        let moved = prev != pos;
        prev = pos;
        if !pressed && hp < hp0 {
            s.press_ability_button(Team::Blue, 0).expect("the press");
            pressed = true;
        }
        let here = find_live(&s, Team::Blue, CUBE).first().is_some();
        if here && first.is_none() {
            first = Some(t);
            stood = prev_state == 1;
        } else if !here && first.is_some() && last.is_none() {
            last = Some(t - 1);
        }
        prev_state = state;
        if let Some(l) = last {
            if t == l + 2 {
                target2 = target;
                moved2 = moved;
            }
            if t > l + 2 && step {
                return Some(Cubed { first: first?, last: l, stood, target2, moved2, jump: t });
            }
        } else if first.is_none() && step {
            return None; // it jumped before the cube came
        }
    }
    None
}

/// combat.DASH_STAND_STUN_HOLD = client16635_stand_kept_while_held, the let-go tick: a Hero Ice Wizard's cube catches a
/// standing Red Mega Knight; on the cube's last frame + 2 it holds no target and walks under both arms; under the new arm it
/// jumps sooner after the let-go than the fresh stand of ends (the vacuity check). Plants: dash_stand_hold_ends,
/// dash_hold_no_ice_grace.
#[test]
fn under_client16635_stand_kept_while_held_a_cube_let_go_tick_keeps_the_stand() {
    let g = (14300, 12450);
    let mut found = None;
    for my in [12450, 12650, 12850, 12250, 13050] {
        let m = (9000, my);
        if let Some(old) = cubed(DashStandStunHold::Ends, m, g).filter(|c| c.stood) {
            found = Some((m, old));
            break;
        }
    }
    let (m, old) = found.expect("the scene drifted: no Mega Knight stand caught by the cube");
    assert!(old.target2.is_none() && old.moved2, "ends: the let-go tick holds no target and walks: {old:?}");
    let new = cubed(DashStandStunHold::Client16635StandKeptWhileHeld, m, g).expect("the scene under the new arm");
    assert_eq!((new.first, new.last, new.stood), (old.first, old.last, old.stood), "the same cube under both arms");
    assert!(new.target2.is_none() && new.moved2, "client16635_stand_kept_while_held: the let-go tick walks: {new:?}");
    assert!(new.jump < old.jump, "client16635_stand_kept_while_held: the kept stand jumps sooner ({new:?} against {old:?})");
}

#[test]
fn the_shipped_arm_is_ends() {
    assert_eq!(royalesim::state::Calib::shipped().dash_stand_stun_hold, DashStandStunHold::Ends);
}

/// (3) The Zap scene with the Knight held 1,500 above the Mega Knight from the tick after the Zap: (e0, the first tick from
/// the Zap's landing with no stun left; the Mega Knight's attack phase and dash state after the first unheld tick, e0 + 1).
fn zapped_in_reach(arm: DashStandStunHold) -> (u32, AttackPhase, i64) {
    let mut cfg = config();
    cfg.calib.dash_attack = DashAttack::ClientDash;
    cfg.calib.dash_first_sight_trigger = DashFirstSightTrigger::FirstSightTick;
    cfg.calib.dash_stand_clock = DashStandClock::Client16635HitSpeedScaled;
    cfg.calib.dash_stand_stun_hold = arm;
    let mut s = BattleState::new(0, cfg);
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at(KNIGHT_AT), None), (Team::Blue, "MegaKnight", at(MK_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (knight, mk) = (ids[0], ids[1]);
    let kmax = s.entity(knight).expect("the Knight").max_hp;
    let mut spot = at(KNIGHT_AT);
    let mut rows: Vec<(i32, AttackPhase, i64)> = Vec::new();
    for t in 0..60u32 {
        if t == 5 {
            let p = s.entity(mk).expect("the Mega Knight").pos;
            s.spawn_unit(Team::Red, "Zap", p, None).expect("the Zap");
        }
        if t == 6 {
            let p = s.entity(mk).expect("the Mega Knight").pos;
            spot = Vec2::new(p.x, p.y + 1500 * K);
        }
        assert!(s.debug_set_pos(knight, spot) && s.debug_set_hp(knight, kmax));
        s.tick();
        let m = s.entity(mk).expect("the Mega Knight");
        rows.push((m.stun_ms, m.attack_phase, s.entity_extras(mk).expect("its extras")[2]));
    }
    let s0 = rows.iter().position(|r| r.0 > 0).expect("the scene drifted: the Zap held nothing");
    assert!(rows[..s0].iter().all(|r| r.2 == 1), "the scene drifted: the Mega Knight was not standing before the Zap: {rows:?}");
    let e0 = (s0..rows.len()).find(|&t| rows[t].0 == 0).expect("the hold never ended");
    (e0 as u32, rows[e0 + 1].1, rows[e0 + 1].2)
}

/// combat.DASH_STAND_STUN_HOLD = client16635_stand_kept_while_held with combat.DASH_STAND_ATTACK = client16635_stand_holds_attack:
/// a paused stand holds the attack, so the Knight taken in reach on its first unheld tick is not swung at and the stand
/// resumes; under ends the Mega Knight swings on that tick (the vacuity check: the Knight is in his reach). Plant:
/// dash_pause_swings, dash_stand_hold_ends.
#[test]
fn under_client16635_stand_kept_while_held_a_paused_stand_takes_no_swing_on_its_first_unheld_tick() {
    let (e00, phase0, _) = zapped_in_reach(DashStandStunHold::Ends);
    assert_ne!(phase0, AttackPhase::Idle, "ends (the vacuity check): the Mega Knight took no swing at the Knight in reach (e0 {e00})");
    let (e0, phase, dash) = zapped_in_reach(DashStandStunHold::Client16635StandKeptWhileHeld);
    assert_eq!(e0, e00, "the same scene up to the hold's end under both arms");
    assert_eq!((phase, dash), (AttackPhase::Idle, 1), "client16635_stand_kept_while_held: the paused stand swung or did not stand again (e0 {e0})");
}
