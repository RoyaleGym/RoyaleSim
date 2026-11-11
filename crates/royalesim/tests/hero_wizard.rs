//! THE HERO WIZARD (card.rs `AbilityEffect::GroundToAir`, `CardDef::projectile_area_ahead`; state.rs `LiftRun`,
//! `lift_pass`; status.rs `BuffDef::no_pushed_by_ally`), against client 15.535.29 at level 11.
//!
//! THE MEASUREMENTS (sp-form-Wizard-hero-s0; the press's first frame P = t215; the Wizard standing at (9500, 11500),
//! attacking a red Knight about 6100 away at the press, which walked to about 5300 by its shot):
//!   - the cast holds it P .. P + 17 (CastTime 950); a red Knight attacking it takes a tower on P + 5: the Wizard is in
//!     the air. Here the Knight lets it go on P + 4, a tick sooner (open: the trigger's own tick, state.rs
//!     LEVEL_SET_EARLY_TICKS);
//!   - its first shot after the cast (an instant hit: its target in reach), first seen on P + 19, takes 238 (93 at
//!     level 1) off the Knight on P + 25, and on P + 26 every enemy within 4000 of the point 1000 ahead of the hit (the owner's forward) loses 43
//!     (17 at level 1): the Knight and a Musketeer beside it;
//!   - from P + 27 both are pulled toward that point (the Knight +91 a tick in y, the Musketeer -149).
//!
//! Read off the table, not measured: the descent (the Wizard died on P + 80, in the air) and the 150 % walk.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_wizard`):
//!   - lift_never -> `a_ground_only_attacker_drops_it_on_the_trigger` red;
//!   - lift_timing_unread -> `under_client_next_tick_late_descent_the_knight_keeps_it_through_p_plus_4` red;
//!   - instant_hit_dropped -> `its_first_shot_after_the_cast_hits_on_p_plus_25` red;
//!   - ally_push_kept -> `the_unit_its_shot_strikes_is_not_pushed_by_its_own_side` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AttractOnset, BattleConfig, BattleState, LiftTiming};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Wizard", "Knight", "Archer", "Giant", "Musketeer", "HogRider", "Fireball", "Zap"];
const WIZARD: (i32, i32) = (9500, 11500);

thread_local! {
    /// `under_client_next_tick_late_descent_...`'s arm for `battle` (None: the shipped one).
    static LIFT_ARM: std::cell::Cell<Option<LiftTiming>> = const { std::cell::Cell::new(None) };
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    // Client 15.535.29's pull onset (this file's measurements; client 16.402's client16402_area_plus_1 ships).
    cfg.calib.attract_onset = AttractOnset::ClientNextTick;
    if let Some(a) = LIFT_ARM.with(|a| a.get()) {
        cfg.calib.lift_timing = a;
    }
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s
}

/// One frame: the Wizard's card name and target, and each red unit's point, hp and target.
struct Frame {
    card: String,
    /// The Wizard's exported status bits (state.rs `view`); 0 once it is gone.
    status: i32,
    reds: Vec<Option<(Vec2, i32, Option<EntityId>)>>,
}

/// The Wizard held at `at`, red `units` held at their points (their hp topped up when `full`) until frame `free`
/// (none: always), deployed; then the press and `frames` frames, frame k being P + k (P the press's first frame, as
/// the client's frames are counted). Returns the Wizard's id, the red units' ids and the frames.
fn scene(at: (i32, i32), units: &[(&str, (i32, i32))], frames: usize, free: Option<usize>, full: bool) -> (EntityId, Vec<EntityId>, Vec<Frame>) {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Wizard_hero", n(at.0, at.1), None).expect("the hero");
    for (card, p) in units {
        s.spawn_unit_resolved(Team::Red, card, n(p.0, p.1), None).expect("a red unit");
    }
    s.tick();
    let hero = find_live(&s, Team::Blue, "Wizard_hero")[0].id;
    let ids: Vec<EntityId> = units
        .iter()
        .map(|(card, p)| s.entities().find(|e| e.team == Team::Red && e.card == *card && e.pos == n(p.0, p.1)).expect("the unit where it was put").id)
        .collect();
    let tops: Vec<i32> = ids.iter().map(|id| s.entity(*id).expect("the unit").max_hp).collect();
    let hold = |s: &mut BattleState, reds: bool| {
        if s.entity(hero).is_some() {
            assert!(s.debug_set_pos(hero, n(at.0, at.1)));
        }
        for ((id, (_, p)), top) in ids.iter().zip(units).zip(&tops) {
            if s.entity(*id).is_some() {
                if reds {
                    assert!(s.debug_set_pos(*id, n(p.0, p.1)));
                }
                if full {
                    assert!(s.debug_set_hp(*id, *top));
                }
            }
        }
    };
    for _ in 0..40 {
        hold(&mut s, true);
        s.tick();
    }
    assert!(s.entities().all(|e| !e.deploying), "the scene is still deploying");
    hold(&mut s, true);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut out = Vec::new();
    for k in 0..frames {
        hold(&mut s, free.map_or(true, |f| k < f));
        s.tick();
        let card = s.entity(hero).map(|e| e.card.to_string()).unwrap_or_default();
        let status = s.entity(hero).map_or(0, |e| e.status_flags);
        let reds = ids.iter().map(|id| s.entity(*id).map(|e| (e.pos, e.hp, e.target))).collect();
        out.push(Frame { card, status, reds });
    }
    (hero, ids, out)
}

/// The shot's scene moved west by SHOT_SHIFT: in place, the blue princess tower reaches its Knight.
const SHOT_SHIFT: i32 = 4000;

fn shifted(p: (i32, i32)) -> (i32, i32) {
    (p.0 - SHOT_SHIFT, p.1)
}

/// P + d is frame d.
fn at(d: usize) -> usize {
    d
}

#[test]
fn a_ground_only_attacker_drops_it_on_the_trigger() {
    // A red Knight beside the Wizard, swinging at it, held there with its hp topped up.
    let (hero, _, f) = scene(WIZARD, &[("Knight", (9500, 12800))], 20, None, true);
    for d in 0..4 {
        assert_eq!(f[at(d)].reds[0].unwrap().2, Some(hero), "the Knight keeps the Wizard on P + {d}");
    }
    // (P + 4 unasserted: the client's Knight still has it, this one has let it go; see the header.)
    for d in 5..20 {
        assert_ne!(f[at(d)].reds[0].unwrap().2, Some(hero), "the Wizard in the air on P + {d}");
    }
    assert_eq!(f[at(10)].card, "WizardHero_air", "its air form by P + 10 (the trigger + 200 ms)");
    // Status bit 4 (card.rs `is_hero_body`): the hero on its own row and on its lifted row alike.
    assert!(f.iter().all(|x| x.status & 16 == 16), "the hero bit on every frame: {:?}", f.iter().map(|x| (x.card.as_str(), x.status)).collect::<Vec<_>>());
}

#[test]
fn its_first_shot_after_the_cast_hits_on_p_plus_25() {
    // The scene's Knight held where it walked by the shot (about 5300 away, in the Wizard's reach), the Musketeer
    // where it stood, both held through P + 26: the scene 4000 west (SHOT_SHIFT), out of every crown tower's reach.
    let (_, _, f) = scene(shifted(WIZARD), &[("Knight", shifted((13640, 14790))), ("Musketeer", shifted((13770, 17015)))], 40, Some(27), false);
    let hp = |d: usize, u: usize| f[at(d)].reds[u].unwrap().1;
    // (a shot it fired before the press lands by P + 8; from its cast's end the Knight is untouched until the air shot)
    assert!((19..25).all(|d| hp(d, 0) == hp(18, 0)), "nothing on the Knight P + 19 .. P + 24");
    assert_eq!(hp(24, 0) - hp(25, 0), 238, "the air shot, 93 at level 1, on P + 25");
    assert_eq!(hp(25, 0) - hp(26, 0), 43, "the damage area on P + 26 (17 at level 1)");
    assert_eq!(hp(25, 1) - hp(26, 1), 43, "on the Musketeer beside it too");
}

#[test]
fn its_areas_pull_toward_the_point_1000_ahead_of_the_hit() {
    let (_, _, f) = scene(shifted(WIZARD), &[("Knight", shifted((13640, 14790))), ("Musketeer", shifted((13770, 17015)))], 40, Some(27), false);
    let pos = |d: usize, u: usize| f[at(d)].reds[u].unwrap().0;
    // Held through P + 26; the pull moves them from P + 27: the Knight up the arena, the Musketeer down, toward the
    // point 1000 ahead of the Knight (y 15790).
    let (k0, k1) = (pos(26, 0), pos(27, 0));
    let (m0, m1) = (pos(26, 1), pos(27, 1));
    assert!(k1.y > k0.y, "the Knight pulled up the arena: {k0:?} -> {k1:?}");
    assert!(m1.y < m0.y, "the Musketeer pulled down: {m0:?} -> {m1:?}");
}

#[test]
fn the_unit_its_shot_strikes_is_not_pushed_by_its_own_side() {
    // The table's reading (the air shot's target buff HeroWizardNoMove sets NO_PUSHED_BY_ALLY), unmeasured: the Knight
    // the shot strikes walks the same pull whether or not a red Skeleton is put down on its point after the hit. The
    // shot's scene, west of the towers; the Knight held through P + 26, the Skeleton put down on P + 27.
    let run = |with_ally: bool| -> Vec<Vec2> {
        let mut s = battle();
        let (w, kp) = (shifted(WIZARD), shifted((13640, 14790)));
        s.spawn_unit(Team::Blue, "Wizard_hero", n(w.0, w.1), None).expect("the hero");
        s.spawn_unit_resolved(Team::Red, "Knight", n(kp.0, kp.1), None).expect("the Knight");
        s.tick();
        let hero = find_live(&s, Team::Blue, "Wizard_hero")[0].id;
        let knight = find_live(&s, Team::Red, "Knight")[0].id;
        let hold = |s: &mut BattleState, knight_too: bool| {
            if s.entity(hero).is_some() {
                assert!(s.debug_set_pos(hero, n(w.0, w.1)));
            }
            if knight_too {
                assert!(s.debug_set_pos(knight, n(kp.0, kp.1)));
            }
        };
        for _ in 0..40 {
            hold(&mut s, true);
            s.tick();
        }
        hold(&mut s, true);
        s.press_ability_button(Team::Blue, 0).expect("the press");
        let mut out = Vec::new();
        for k in 0..40 {
            hold(&mut s, k < 27);
            if with_ally && k == 27 {
                let at = s.entity(knight).expect("the Knight").pos;
                s.scenario_spawn_now(Team::Red, "Skeleton", at, None).expect("a red Skeleton on the Knight");
            }
            s.tick();
            out.push(s.entity(knight).map(|e| e.pos).unwrap_or_default());
        }
        out
    };
    let (alone, beside) = (run(false), run(true));
    assert_ne!(alone[at(27)], alone[at(34)], "the Knight is pulled");
    assert_eq!(alone[at(27)..at(35)], beside[at(27)..at(35)], "its own side does not push it while the buff lasts");
}

/// status.LIFT_TIMING = client_next_tick_late_descent (client 16.402 and 15.535.29: ground-only attackers let the lifted
/// hero go on P + 5, 7 of 7): `a_ground_only_attacker_drops_it_on_the_trigger`'s Knight keeps the Wizard through P + 4 and
/// lets it go on P + 5; under engine_reading (the vacuity check) on P + 4. Plant: lift_timing_unread.
#[test]
fn under_client_next_tick_late_descent_the_knight_keeps_it_through_p_plus_4() {
    let keeps = |arm: LiftTiming| -> Vec<bool> {
        LIFT_ARM.with(|a| a.set(Some(arm)));
        let (hero, _, f) = scene(WIZARD, &[("Knight", (9500, 12800))], 8, None, true);
        LIFT_ARM.with(|a| a.set(None));
        (0..8).map(|d| f[at(d)].reds[0].unwrap().2 == Some(hero)).collect()
    };
    let new = keeps(LiftTiming::ClientNextTickLateDescent);
    assert!(new[..5].iter().all(|k| *k) && !new[5], "client_next_tick_late_descent: kept through P + 4, let go on P + 5: {new:?}");
    let old = keeps(LiftTiming::EngineReading);
    assert!(!old[4], "engine_reading: kept on P + 4: {old:?}");
}

/// The Wizard (the shot's scene, SHOT_SHIFT west) pressed after `40 + wait` ticks with a second red Knight 1,300 ahead, his
/// target and in his instant range, under status.LIFT_CAST_END_SHOT = `arm`; the first Knight held 9,500 ahead of him, out of his
/// sight, until P + 15 and from there 5,300 off, in his reach, so no target replaces the near one at once. The near Knight is put
/// to 0 hitpoints before frame `kill`; or, when `low`, held at 1 hitpoint from the press, so the first hit landing on it (a shot,
/// after the frame's target pass) kills it. Returns the frame (P + d) on which the first Knight first loses hitpoints, the first
/// frame after which his swing reads primed (a windup one tick short of his HitSpeed: his trigger's), and the near Knight's death
/// frame.
fn primed_run(arm: royalesim::state::LiftCastEndShot, kill: usize, wait: usize, low: bool) -> (Option<usize>, Option<usize>, Option<usize>) {
    let mut cfg: BattleConfig = config();
    cfg.calib.attract_onset = AttractOnset::ClientNextTick;
    cfg.calib.lift_cast_end_shot = arm;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let (wiz, near, far, reach) = (shifted(WIZARD), shifted((9500, 12800)), (shifted(WIZARD).0, WIZARD.1 + 9500), shifted((13640, 14790)));
    s.spawn_unit(Team::Blue, "Wizard_hero", n(wiz.0, wiz.1), None).expect("the hero");
    s.spawn_unit_resolved(Team::Red, "Knight", n(near.0, near.1), None).expect("the near Knight");
    s.spawn_unit_resolved(Team::Red, "Knight", n(far.0, far.1), None).expect("the far Knight");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Wizard_hero")[0].id;
    let find = |s: &BattleState, p: (i32, i32)| s.entities().find(|e| e.team == Team::Red && e.card == "Knight" && e.pos == n(p.0, p.1)).expect("the unit where it was put").id;
    let (sk, kn) = (find(&s, near), find(&s, far));
    // `d`: the frame about to run (None before the press).
    let hold = |s: &mut BattleState, d: Option<usize>| {
        if s.entity(hero).is_some() {
            assert!(s.debug_set_pos(hero, n(wiz.0, wiz.1)));
        }
        let kn_at = if d.is_some_and(|d| d >= 15) { reach } else { far };
        for (id, p) in [(sk, near), (kn, kn_at)] {
            if let Some(full) = s.entity(id).map(|e| e.max_hp) {
                assert!(s.debug_set_pos(id, n(p.0, p.1)));
                let hp = match d {
                    None => full,
                    Some(_) if id == sk && low => 1,
                    Some(d) if d < kill => full,
                    _ => continue,
                };
                assert!(s.debug_set_hp(id, hp));
            }
        }
    };
    for _ in 0..40 + wait {
        hold(&mut s, None);
        s.tick();
    }
    hold(&mut s, None);
    assert_eq!(s.entity(hero).expect("the hero").target, Some(sk), "the scene drifted: the Wizard does not hold the near Knight at the press");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let top = s.entity(kn).expect("the Knight").max_hp;
    let tick_ms = s.config().calib.tick_ms;
    let (mut primed, mut death) = (None, None);
    for d in 0..90 {
        hold(&mut s, Some(d));
        if d == kill && !low {
            assert!(s.debug_set_hp(sk, 0), "the near Knight stands to frame {kill}");
        }
        s.tick();
        if let Some(h) = s.entity(hero) {
            let hs = s.config().cards.get(h.card_idx).hit_speed_ms;
            if primed.is_none() && h.attack_phase == royalesim::entity::AttackPhase::Windup && h.attack_ms == (hs - tick_ms).max(tick_ms) {
                primed = Some(d);
            }
        }
        if death.is_none() && s.entity(sk).is_none() {
            death = Some(d);
        }
        if s.entity(kn).is_some_and(|e| e.hp < top) {
            return (Some(d), primed, death);
        }
    }
    (None, primed, death)
}

/// status.LIFT_CAST_END_SHOT = client16635_primed_shot_survives_retarget (client 16.635, parity r93, live15n 20261103-183005-A:
/// pressed with a Bat in reach that the takeoff killed on the frame before his trigger, the Hero Wizard fired at the Miner on press
/// + 19): (1) his target killed on the trigger's frame (primed as ever), the cast keeps the swing through the loss and he hits
/// the Knight he takes from his cast's end before P + 30, where dropped (the vacuity check) loads afresh, after it (plant
/// primed_shot_dropped); (2) his target killed by a hit landing on the frame before the trigger (the dead target still held there),
/// the swing is primed all the same, so the same (plant lost_target_unprimed).
#[test]
fn under_client16635_primed_shot_survives_retarget_his_primed_swing_fires_at_his_next_target() {
    use royalesim::state::LiftCastEndShot as L;
    let trigger = primed_run(L::Dropped, 999, 0, false).1.expect("the scene drifted: the Wizard's swing was never primed with his target alive");
    assert!(trigger >= 1, "the scene drifted: primed on the press's own frame");
    let hit = |arm, kill, wait, low| primed_run(arm, kill, wait, low).0;
    let (new, old) = (hit(L::Client16635PrimedShotSurvivesRetarget, trigger, 0, false), hit(L::Dropped, trigger, 0, false));
    assert!(new.is_some_and(|n| n < 30), "client16635_primed_shot_survives_retarget: his target killed on the trigger's frame {trigger}, his first hit on the Knight came on {new:?}");
    assert!(old.map_or(true, |o| o >= 30), "dropped (the vacuity check): his target killed on the trigger's frame, his first hit on the Knight came on {old:?}");
    let wait = (0..40)
        .find(|&w| primed_run(L::Dropped, 999, w, true).2 == Some(trigger - 1))
        .unwrap_or_else(|| panic!("the scene drifted: no press time lands a hit on the near Knight on the frame before the trigger ({trigger})"));
    let (new, old) = (hit(L::Client16635PrimedShotSurvivesRetarget, 999, wait, true), hit(L::Dropped, 999, wait, true));
    assert!(new.is_some_and(|n| n < 30), "client16635_primed_shot_survives_retarget: his target killed on the frame before the trigger (wait {wait}), his first hit on the Knight came on {new:?}");
    assert!(old.map_or(true, |o| o >= 30), "dropped (the vacuity check): his target killed on the frame before the trigger (wait {wait}), his first hit on the Knight came on {old:?}");
}
