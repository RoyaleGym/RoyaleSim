//! THE LITTLE PRINCE (tools/extract_cards.py `champion_ramp`, `champion_guard`; card.rs `RampDef`, `GuardDef`,
//! GUARD_SPAWN_OFFSET, GUARD_END_OFFSET, GUARD_DASH_START_TICKS, GUARD_STEP, GUARD_LANDING_TICKS; state.rs `RampRun`,
//! `ramp_pass`, `swing_started`, `ramp_shot`, `GuardRun`, `guard_release`, `guard_bind`, `guard_moves`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; sp-champ-LittlePrince-s0 and Oracle's sp-lp-*-s0): his shots 24, 24, 12, 12,
//! 12, 8, 8 ... ticks apart, a Zap's stun starting the ramp again; the guard's first frame the press + 18 at his point +
//! (18, -1944), its first step + 25, 398 a tick, its last + 37 at his point + (239, 3083); a Knight 2962 from its path
//! losing 256 once.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! little_prince`): ramp_never, ramp_never_resets, guard_never, guard_never_charges, guard_collides,
//! early_trigger_late, guard_lands_loaded, guard_push_once (`his_guards_charge_pushes_by_a_ladder_rearmed_every_tick`
//! red), guard_charge_rescaled (`his_guards_charge_steps_as_the_client_steps_it_under_client15535_substeps_to_aim` red),
//! ramp_grace_pushed (`his_ramp_holds_through_pushes_while_he_attacks_under_client15535_own_walk` red), ramp_stun_paused
//! (`a_stun_restarts_his_attack_under_client15535_restart` red), ramp_reset_at_zero
//! (`his_ramp_holds_through_six_moving_ticks_and_goes_on_the_seventh` red), ramp_press_resets and
//! ramp_held_by_press_unread (`under_client16402_tag_held_his_press_keeps_his_ramp` red), guard_charge_hit
//! (`under_client16402_charge_immune_his_guard_takes_no_hit_in_its_charge` red), guard_push_swing_frozen
//! (`under_client16402_reset_his_guards_push_resets_the_pushed_swing` red), landing_hits_end_at_arrival and
//! landing_hits_unbounded (`under_client16402_through_landing_ticks_his_guard_hits_through_its_arrival_plus_three` red),
//! press_hold_table_time (`under_client16402_three_ticks_longer_his_press_holds_him_three_ticks_past_its_time` red),
//! press_contact_pushed (`under_client16402_unpushed_a_knight_on_him_does_not_move_him_in_his_press` red),
//! guard_birth_fixed and guard_birth_no_step (`under_client16402_behind_2000_first_step_his_guard_is_born_2000_behind_and_steps` red),
//! guard_free_on_arrival_plus_1 (`under_client16402_held_through_arrival_plus_1_his_guard_targets_a_tick_later` red),
//! press_hold_pulled (`under_client16402_unpulled_through_hold_a_tornado_does_not_move_him_in_his_press` red).
//!
//! combat.RAMP_STUN_RESTART = client15535_restart (client 15.535.29, sp-lp-ramp-s0's Zap, his one recorded stun): the
//! stun's landing tick reads his progress 0; the resume's fresh start reads 450 with his load timer at 800, and he shoots
//! 15 ticks on, at 1200.
//!
//! combat.RAMP_GRACE_MOVE = client15535_own_walk (client 15.535.29, every Little Prince scene: 3 of 3 ramps kept through 6
//! to 8 pushes while he attacked, 6 of 6 reset by a walk): his grace runs down on his own walk alone.
//!
//! combat.GUARD_CHARGE_STEP = client15535_substeps_to_aim (client 15.535.29, the six charges, 78 of 78 steps): the charge
//! in pieces of 250 and 150 re-aimed at his point + (250, 3250); the engine's rescaled line steps (17, 397).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::card::CardDb;
use royalesim::state::{BattleConfig, BattleState, CardTable, GuardChargeImmunity, GuardChargeStep, GuardBirthPoint, GuardFreeTick, GuardLandingHits, GuardPressContact, GuardPressHold, GuardPushAttackReset, GuardWaterLanding, GuardBirthPassThrough, DashImmunePush, LoadTimerTargetLoss, RampGraceMove, RampGraceReset, RampHookReset, RampLevelDrop, RampPressHold, RampStunRestart, SpecialHook};
use royalesim::entity::AttackPhase;
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["LittlePrince", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const AT: (i32, i32) = (14500, 10500);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Little Prince put at AT and held there 30 ticks, red `reds` put and held: the battle, his id and theirs.
fn scene(reds: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<(EntityId, Vec2)>) {
    scene_with(GuardChargeStep::Rescaled, reds)
}

/// `scene` under combat.GUARD_CHARGE_STEP = `arm`.
fn scene_with(arm: GuardChargeStep, reds: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<(EntityId, Vec2)>) {
    // The 15.535.29 scenes (Oracle's sp-lp-* runs): the guard born at (18, -1944), combat.GUARD_BIRTH_POINT's old arm.
    scene_cfg(
        |c| {
            c.calib.guard_charge_step = arm;
            c.calib.guard_birth_point = GuardBirthPoint::Client15535FixedOffset;
            c.calib.guard_free_tick = GuardFreeTick::FreeOnArrivalPlus1;
        },
        reds,
    )
}

/// `scene` with `tweak` applied to the config first.
fn scene_cfg(tweak: impl FnOnce(&mut BattleConfig), reds: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<(EntityId, Vec2)>) {
    let mut cfg = config();
    tweak(&mut cfg);
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let lp = s.scenario_spawn_now(Team::Blue, "LittlePrince", n(AT), None).expect("the Little Prince");
    let reds: Vec<(EntityId, Vec2)> = reds.iter().map(|(c, p)| (s.scenario_spawn_now(Team::Red, c, n(*p), None).expect("a red"), n(*p))).collect();
    for _ in 0..30 {
        hold(&mut s, lp, &reds);
        s.tick();
    }
    (s, lp, reds)
}

fn hold(s: &mut BattleState, lp: EntityId, reds: &[(EntityId, Vec2)]) {
    if s.entity(lp).is_some() {
        assert!(s.debug_set_pos(lp, n(AT)));
    }
    for (id, p) in reds {
        if let Some(e) = s.entity(*id) {
            let full = e.max_hp;
            assert!(s.debug_set_pos(*id, *p));
            assert!(s.debug_set_hp(*id, full));
        }
    }
}

/// The ticks on which the Little Prince's shots are made, over `ticks`, `zap_at` ticks in a red Zap on him.
fn shots(s: &mut BattleState, lp: EntityId, reds: &[(EntityId, Vec2)], ticks: u32, zap_at: Option<u32>) -> Vec<u32> {
    let mine = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(lp)).count();
    let mut known = mine(s);
    let mut out = Vec::new();
    for k in 0..ticks {
        hold(s, lp, reds);
        if zap_at == Some(k) {
            s.spawn_unit(Team::Red, "Zap", n(AT), None).expect("the Zap");
        }
        s.tick();
        let m = mine(s);
        if m > known {
            out.push(s.tick_count() - 1);
        }
        known = m;
    }
    out
}

fn gaps(t: &[u32]) -> Vec<u32> {
    t.windows(2).map(|w| w[1] - w[0]).collect()
}

#[test]
fn his_shots_quicken_twice_after_his_third_and_sixth() {
    // A red Golem held 5000 ahead (in his reach, too big to die), put down after his hold: his first shot is counted.
    let (mut s, lp, _) = scene(&[]);
    let at = (AT.0, AT.1 + 5000);
    let reds = vec![(s.scenario_spawn_now(Team::Red, "Golem", n(at), None).expect("a red Golem"), n(at))];
    let t = shots(&mut s, lp, &reds, 220, None);
    let g = gaps(&t);
    assert!(g.len() >= 8, "his shots: {t:?}");
    assert_eq!(&g[..8], &[24, 24, 12, 12, 12, 8, 8, 8], "his shots' gaps: {t:?}");
}

/// The ticks of his shots at a red Golem held 5000 ahead over 220 ticks, his point put 150 aside on every other tick (a
/// push while he attacks, as the Golem walking into him gave him in sp-lp-ramp-s0), under combat.RAMP_GRACE_MOVE = `arm`.
fn pushed_shots(arm: RampGraceMove) -> Vec<u32> {
    let (mut s, lp, _) = scene_cfg(|c| c.calib.ramp_grace_move = arm, &[]);
    let at = (AT.0, AT.1 + 5000);
    let reds = vec![(s.scenario_spawn_now(Team::Red, "Golem", n(at), None).expect("a red Golem"), n(at))];
    let mine = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(lp)).count();
    let mut known = mine(&s);
    let mut out = Vec::new();
    for k in 0..220 {
        hold(&mut s, lp, &reds);
        if k % 2 == 1 {
            assert!(s.debug_set_pos(lp, n((AT.0 - 150, AT.1))));
        }
        s.tick();
        let m = mine(&s);
        if m > known {
            out.push(s.tick_count() - 1);
        }
        known = m;
    }
    out
}

/// Plant: ramp_grace_pushed.
#[test]
fn his_ramp_holds_through_pushes_while_he_attacks_under_client15535_own_walk() {
    let ramp = [24, 24, 12, 12, 12, 8, 8, 8];
    // NOT VACUOUS: under point_changed every push runs his grace down, so his ramp never gets past its first speed.
    let old = pushed_shots(RampGraceMove::PointChanged);
    let g = gaps(&old);
    assert!(g.len() >= 8 && g[..8] != ramp && g.iter().all(|&x| x >= 24), "point_changed: the pushes kept his ramp: {old:?}");
    let new = pushed_shots(RampGraceMove::Client15535OwnWalk);
    let g = gaps(&new);
    assert!(g.len() >= 8 && g[..8] == ramp, "client15535_own_walk: his ramp did not hold through the pushes: {g:?} ({new:?})");
}

/// A red Zap on the Little Prince, shooting at a red Golem held 5000 ahead, landing at the point of his cycle the client's
/// did (sp-lp-ramp-s0 t337: his load timer 500 the tick before, 450 on the landing tick), under combat.RAMP_STUN_RESTART
/// = `arm` and the client's combat.LOAD_TIMER_TARGET_LOSS (client15535_stands_while_held): his attack progress on the
/// landing tick; his progress and load timer on the resume tick (his first with a target again); the ticks from the
/// resume to his next shot.
fn zapped_clock(arm: RampStunRestart) -> (i32, i32, i32, u32) {
    let mine = |s: &BattleState, lp: EntityId| s.projectiles().iter().filter(|q| q.firer == Some(lp)).count();
    for k in 0..90 {
        let (mut s, lp, reds) = scene_cfg(
            |c| {
                c.calib.ramp_stun_restart = arm;
                c.calib.load_timer_target_loss = LoadTimerTargetLoss::Client15535StandsWhileHeld;
            },
            &[("Golem", (AT.0, AT.1 + 5000))],
        );
        for _ in 0..k {
            hold(&mut s, lp, &reds);
            s.tick();
        }
        let e = s.entity(lp).expect("the Little Prince");
        if e.target.is_none() || e.attack_load_ms != 500 || e.attack_ms == 0 {
            continue;
        }
        hold(&mut s, lp, &reds);
        s.spawn_unit(Team::Red, "Zap", n(AT), None).expect("the Zap");
        s.tick();
        let e = s.entity(lp).expect("the Little Prince");
        assert!(e.stun_ms > 0 && e.target.is_none(), "scene: the Zap did not hold him ({k} ticks in)");
        assert_eq!(e.attack_load_ms, 450, "scene: his load timer on the Zap's tick ({k} ticks in)");
        let landing = e.attack_ms;
        let mut known = mine(&s, lp);
        let mut resume: Option<(u32, i32, i32)> = None;
        for j in 1..80u32 {
            hold(&mut s, lp, &reds);
            s.tick();
            let e = s.entity(lp).expect("the Little Prince");
            if resume.is_none() && e.target.is_some() {
                resume = Some((j, e.attack_ms, e.attack_load_ms));
            }
            let m = mine(&s, lp);
            if m > known {
                let (r, p, l) = resume.expect("scene: a shot before his target came back");
                return (landing, p, l, j - r);
            }
            known = m;
        }
        panic!("scene: no shot after the Zap ({k} ticks in)");
    }
    panic!("precondition: no tick puts his load timer at 500 while he shoots");
}

/// Plant: ramp_stun_paused.
#[test]
fn a_stun_restarts_his_attack_under_client15535_restart() {
    assert_eq!(zapped_clock(RampStunRestart::Client15535Restart), (0, 450, 800, 15), "client15535_restart: progress on the Zap's tick, progress and load on the resume, ticks to the shot");
    // NOT VACUOUS: paused, his progress survives the stun.
    let (landing, ..) = zapped_clock(RampStunRestart::Paused);
    assert!(landing > 0, "paused: his progress did not survive the stun ({landing})");
}

#[test]
fn a_stun_starts_his_ramp_again() {
    let (mut s, lp, reds) = scene(&[("Golem", (AT.0, AT.1 + 5000))]);
    let p0 = s.tick_count();
    let t = shots(&mut s, lp, &reds, 260, Some(120));
    let g = gaps(&t);
    // From the first shot after the Zap's tick (p0 + 120; its stun holds the swing under way), the gaps run 24, 24, 12
    // again: measured, 363, 387, 411, 423 after a Zap on t336.
    let after = t.iter().position(|x| *x > p0 + 120).expect("shots after the Zap");
    let g2 = gaps(&t[after..]);
    assert!(g2.len() >= 3 && g2[..3] == [24, 24, 12], "after the Zap: {g2:?} (all: {g:?})");
}

#[test]
fn his_guard_appears_behind_him_and_charges_to_a_fixed_point_ahead() {
    let (mut s, lp, _) = scene(&[]);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows: Vec<(u32, Vec2, bool)> = Vec::new();
    for _ in 0..50 {
        assert!(s.debug_set_pos(lp, n(AT)));
        s.tick();
        if let Some(e) = find_live(&s, Team::Blue, "ChampionGuard").first() {
            rows.push((s.tick_count() - 1 - p, e.pos, e.deploying));
        }
    }
    let first = rows.first().expect("a guard");
    assert_eq!(first.0, 18, "its first frame, from the press: {rows:?}");
    assert_eq!(first.1, n((AT.0 + 18, AT.1 - 1944)), "its first point: {rows:?}");
    let at = |k: u32| rows.iter().find(|r| r.0 == k).map(|r| r.1).expect("a row");
    assert_eq!(at(24), first.1, "still on the press + 24: {rows:?}");
    assert!(at(25).y > first.1.y, "its first step on the press + 25: {rows:?}");
    assert_eq!(at(37), n((AT.0 + 239, AT.1 + 3083)), "its end on the press + 37: {rows:?}");
    assert_ne!(at(36), at(37), "its last step on the press + 37: {rows:?}");
}

/// THE GUARD'S FIRST HIT after its charge: a whole swing on, its load timer full as it comes free. Measured on client
/// 15.535.29 (Oracle's sp-lp-far-s0: the press P = t122, the guard free P + 39, a Knight at its end losing 232 (91 at level
/// 11) first on P + 62, then every 24 ticks).
#[test]
fn his_guards_first_hit_after_its_charge_is_a_whole_swing_on() {
    // A red Knight held 1500 beyond the guard's end (in its reach); Blue's princess towers down.
    let at = (AT.0 + 239, AT.1 + 3083 + 1500);
    let (mut s, lp, reds) = scene(&[("Knight", at)]);
    let knight = reds[0].0;
    let towers: Vec<EntityId> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut losses = Vec::new();
    for _ in 0..100 {
        assert!(s.debug_set_pos(lp, n(AT)));
        let full = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_pos(knight, n(at)));
        assert!(s.debug_set_hp(knight, full));
        s.tick();
        let lost = full - s.entity(knight).expect("the Knight").hp;
        if lost > 0 {
            losses.push((s.tick_count() - 1 - p, lost));
        }
    }
    // The guard's hits are 232; his own shots at the Knight the other losses (232 with one of them on a tick).
    let shots: Vec<i32> = losses.iter().map(|l| l.1).filter(|l| *l < 232).collect();
    let guard: Vec<u32> = losses.iter().filter(|l| l.1 == 232 || shots.contains(&(l.1 - 232))).map(|l| l.0).collect();
    assert_eq!(guard.first(), Some(&62), "the guard's first hit, from the press: {losses:?}");
}

#[test]
fn his_guards_charge_hits_a_knight_near_its_path_once() {
    // A red Knight held 2400 to the right of the guard's path.
    let (mut s, lp, reds) = scene(&[("Knight", (AT.0 + 2400, AT.1 + 1000))]);
    let knight = reds[0].0;
    // Blue's princess towers down (their arrows reach the Knight): his own shots are its other losses.
    let towers: Vec<EntityId> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut losses = Vec::new();
    for _ in 0..45 {
        assert!(s.debug_set_pos(lp, n(AT)));
        assert!(s.debug_set_pos(knight, reds[0].1));
        let before = s.entity(knight).expect("the Knight").hp;
        s.tick();
        let after = s.entity(knight).expect("the Knight").hp;
        if after < before {
            losses.push(before - after);
        }
    }
    // The charge's hit is the one loss of 256 or more: 256, or 256 and one of his shots on its tick.
    let (hits, shots): (Vec<i32>, Vec<i32>) = losses.iter().partition(|l| **l >= 256);
    assert_eq!(hits.len(), 1, "one charge hit: {losses:?}");
    assert!(hits[0] == 256 || shots.contains(&(hits[0] - 256)), "a charge hit of 256: {losses:?}");
}

/// THE GUARD'S PUSH (state.rs `guard_moves`, `rearm_ladder`): the knockback ladder, re-armed from the guard's point on
/// every tick of its charge for a troop whose centre is within 2500 of it, m = 2500 - d, the new ladder replacing the
/// running one unless its start is the smaller; after the charge the last ladder runs out. Read off the client code by
/// Oracle and checked tick for tick on client 15.535.29 (sp-lp-live-s0: 25 on t213, then 125, 150, 174, 199, 224 ...).
/// Here a red Knight beside the guard's path is held until the guard is first within 2500 of it, then each of its steps
/// is the ladder's from the frame before, to 3.
#[test]
fn his_guards_charge_pushes_by_a_ladder_rearmed_every_tick() {
    let (mut s, lp, reds) = scene(&[("Knight", (AT.0 + 800, AT.1 + 1500))]);
    let knight = reds[0].0;
    let dist = |a: Vec2, b: Vec2| -> i64 {
        let (dx, dy) = (((a.x - b.x) / K) as i64, ((a.y - b.y) / K) as i64);
        royalesim::fixed::isqrt(dx * dx + dy * dy)
    };
    s.press_ability_button(Team::Blue, 0).expect("the press");
    // (the Knight, the guard) at the end of each tick from the one the guard first stands within 2500 of it.
    let mut rows: Vec<(Vec2, Vec2)> = Vec::new();
    let mut held = true;
    for _ in 0..70 {
        if s.entity(lp).is_some() {
            assert!(s.debug_set_pos(lp, n(AT)));
        }
        if held {
            assert!(s.debug_set_pos(knight, reds[0].1));
        }
        s.tick();
        let Some(g) = find_live(&s, Team::Blue, "ChampionGuard").first().map(|e| e.pos) else { continue };
        let k = s.entity(knight).expect("the Knight").pos;
        if held && dist(g, k) < 2500 {
            held = false;
        }
        if !held {
            rows.push((k, g));
        }
    }
    assert!(rows.len() > 20, "the scene drifted: the guard never came within 2500 of the Knight");
    let mut cur = 0;
    let mut checked = Vec::new();
    for i in 1..rows.len() {
        let (k0, g0) = rows[i - 1];
        // The guard charged on frame i - 1 when it moved on it (the first row's frame it moved into reach).
        let charging = i == 1 || rows[i - 2].1 != g0;
        if charging && dist(g0, k0) < 2500 {
            let start = royalesim::move16402::ladder_speed(2500 - dist(g0, k0) as i32);
            if start >= cur {
                cur = start;
            }
        }
        cur -= 25;
        if cur < 0 {
            break;
        }
        let step = dist(rows[i].0, k0) as i32;
        assert!((step - cur).abs() <= 3, "row {i}: a step of {step}, the ladder's {cur}; checked {checked:?}");
        checked.push(step);
    }
    assert!(checked.len() >= 8, "the scene drifted: {} ladder steps checked: {checked:?}", checked.len());
}

/// The client's charge steps (client 15.535.29: the same on all six charges), native.
const CLIENT_CHARGE: [(i32, i32); 13] = [(16, 398), (16, 398), (16, 398), (16, 398), (17, 398), (18, 398), (18, 398), (18, 398), (18, 398), (18, 398), (18, 398), (19, 399), (13, 250)];

/// The guard's first 13 moving steps after the press under `arm`, and its point after them.
fn charge_steps(arm: GuardChargeStep) -> (Vec<(i32, i32)>, (i32, i32)) {
    let (mut s, lp, _) = scene_with(arm, &[]);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut pts: Vec<(i32, i32)> = Vec::new();
    for _ in 0..45 {
        assert!(s.debug_set_pos(lp, n(AT)));
        s.tick();
        if let Some(e) = find_live(&s, Team::Blue, "ChampionGuard").first() {
            pts.push((e.pos.x / K, e.pos.y / K));
        }
    }
    let mut steps = Vec::new();
    let mut at = *pts.first().expect("a guard");
    for w in pts.windows(2) {
        let d = (w[1].0 - w[0].0, w[1].1 - w[0].1);
        if d != (0, 0) && steps.len() < 13 {
            steps.push(d);
            at = w[1];
        }
    }
    (steps, at)
}

/// Plant: guard_charge_rescaled.
#[test]
fn his_guards_charge_steps_as_the_client_steps_it_under_client15535_substeps_to_aim() {
    // NOT VACUOUS: the rescaled line steps (17, 397).
    let (old, _) = charge_steps(GuardChargeStep::Rescaled);
    assert_eq!(old.first(), Some(&(17, 397)), "rescaled: {old:?}");
    let (new, end) = charge_steps(GuardChargeStep::Client15535SubstepsToAim);
    assert_eq!(new, CLIENT_CHARGE.to_vec(), "client15535_substeps_to_aim: the steps");
    assert_eq!(end, (AT.0 + 239, AT.1 + 3083), "client15535_substeps_to_aim: its end");
}

/// His shots' gaps at a red Golem 5000 ahead after `walk` moving ticks with no target (the Golem far off, his point moved
/// 30 a tick, combat.RAMP_GRACE_MOVE = point_changed), his ramp at its third speed before the walk, under
/// combat.RAMP_GRACE_RESET = `arm`.
fn walked_gaps(arm: RampGraceReset, walk: i32) -> Vec<u32> {
    let (mut s, lp, _) = scene_cfg(
        |c| {
            c.calib.ramp_grace_reset = arm;
            c.calib.ramp_grace_move = RampGraceMove::PointChanged;
        },
        &[],
    );
    let at = (AT.0, AT.1 + 5000);
    let golem = s.scenario_spawn_now(Team::Red, "Golem", n(at), None).expect("a red Golem");
    let t = shots(&mut s, lp, &[(golem, n(at))], 160, None);
    assert!(gaps(&t).ends_with(&[8, 8]), "the scene: his ramp at its third speed before the walk {t:?}");
    let away = n((AT.0, 31000));
    for k in 1..=walk {
        assert!(s.debug_set_pos(lp, n((AT.0 + 30 * k, AT.1))));
        assert!(s.debug_set_pos(golem, away));
        s.tick();
    }
    let me = (AT.0 + 30 * walk, AT.1);
    let back = (me.0, me.1 + 5000);
    let mine = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(lp)).count();
    let mut known = mine(&s);
    let mut out = Vec::new();
    for _ in 0..120 {
        assert!(s.debug_set_pos(lp, n(me)));
        if let Some(e) = s.entity(golem) {
            let full = e.max_hp;
            assert!(s.debug_set_pos(golem, n(back)));
            assert!(s.debug_set_hp(golem, full));
        }
        s.tick();
        let m = mine(&s);
        if m > known {
            out.push(s.tick_count() - 1);
        }
        known = m;
    }
    gaps(&out)
}

/// combat.RAMP_GRACE_RESET = client16402_below_zero (parity's r62, item D). Client 16.402 (the live population): a gap of
/// 6 moving ticks keeps his ramp (45 of 46), 7 resets it (20 of 20). His grace is 300 ms, 50 a moving tick. Plant:
/// ramp_reset_at_zero.
#[test]
fn his_ramp_holds_through_six_moving_ticks_and_goes_on_the_seventh() {
    // NOT VACUOUS: under at_zero six moving ticks reset it.
    let old = walked_gaps(RampGraceReset::AtZero, 6);
    assert!(old.len() >= 2 && old[..2] == [24, 24], "at_zero: six moving ticks reset his ramp {old:?}");
    let six = walked_gaps(RampGraceReset::Client16402BelowZero, 6);
    assert!(six.len() >= 2 && six[..2] == [8, 8], "client16402_below_zero: his ramp holds through six moving ticks {six:?}");
    let seven = walked_gaps(RampGraceReset::Client16402BelowZero, 7);
    assert!(seven.len() >= 2 && seven[..2] == [24, 24], "client16402_below_zero: the seventh resets it {seven:?}");
}

/// Where the guard's charge ends under `arm`, the Little Prince held on `at` (native) and pressed alone.
fn charge_end_at(arm: GuardChargeStep, at: (i32, i32)) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.guard_charge_step = arm;
    cfg.calib.guard_birth_point = GuardBirthPoint::Client15535FixedOffset;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let lp = s.scenario_spawn_now(Team::Blue, "LittlePrince", n(at), None).expect("the Little Prince");
    for _ in 0..30 {
        assert!(s.debug_set_pos(lp, n(at)));
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut pts: Vec<(i32, i32)> = Vec::new();
    for _ in 0..60 {
        assert!(s.debug_set_pos(lp, n(at)));
        s.tick();
        if let Some(e) = find_live(&s, Team::Blue, "ChampionGuard").first() {
            pts.push((e.pos.x / K, e.pos.y / K));
        }
    }
    // the charge's end: the point after its last charge step (pieces of 250 and 150, over 150 a tick), before the guard
    // walks on at its own speed
    let mut end = *pts.first().expect("a guard");
    let mut charging = false;
    for w in pts.windows(2) {
        let (dx, dy) = ((w[1].0 - w[0].0) as i64, (w[1].1 - w[0].1) as i64);
        let big = dx * dx + dy * dy > 150 * 150;
        if big {
            charging = true;
            end = w[1];
        } else if charging {
            break;
        }
    }
    end
}

/// combat.GUARD_CHARGE_STEP = client16402_cell_aim (client 16.402, parity's r63 census: the truth's path cell 697 of 697, the
/// dash 660 of 697 exact): on a tile centre the client's charge (as client15535_substeps_to_aim's); off it, the charge ends
/// within 400 of the 500 cell's centre ahead of him ((14120, 10620): the cell holding (14121, 13820), centre (14250, 13750)),
/// where the 15.535.29 arm aims at his point + (250, 3250). Plant: guard_cell_aim_unread.
#[test]
fn under_client16402_cell_aim_the_charge_ends_by_the_cell_centre_ahead() {
    let (steps, end) = charge_steps(GuardChargeStep::Client16402CellAim);
    assert_eq!(steps, CLIENT_CHARGE.to_vec(), "client16402_cell_aim on a tile centre: the client's steps");
    assert_eq!(end, (AT.0 + 239, AT.1 + 3083), "client16402_cell_aim on a tile centre: its end");
    let d2 = |p: (i32, i32), q: (i32, i32)| ((p.0 - q.0) as i64).pow(2) + ((p.1 - q.1) as i64).pow(2);
    let new = charge_end_at(GuardChargeStep::Client16402CellAim, (14120, 10620));
    assert!(d2(new, (14250, 13750)) < 400 * 400, "client16402_cell_aim: the end {new:?} not within 400 of (14250, 13750)");
    let old = charge_end_at(GuardChargeStep::Client15535SubstepsToAim, (14120, 10620));
    assert!(old != new, "the vacuity check: the 15.535.29 arm ends where the new one does ({old:?})");
}

/// The 160402017 table's battle config (its Little Prince's ramp row sets grace_held_while, the guard's hold tag).
fn config_160402017() -> BattleConfig {
    let db = CardDb::load_table(CardTable::Client160402017).expect("the 160402017 table");
    let mut cfg = BattleConfig::with_cards(db);
    cfg.calib.card_table = CardTable::Client160402017;
    cfg
}

/// His shots at a red Golem held 5000 ahead over 260 ticks on the 160402017 table, his guard pressed `press_at` ticks
/// in, under combat.RAMP_PRESS_HOLD = `arm`: the shots' ticks and the press's.
fn pressed_shots(arm: RampPressHold, press_at: u32) -> (Vec<u32>, u32) {
    let (mut s, lp, reds) = scene_cfg(
        |c| {
            let mut k = config_160402017();
            k.calib.ramp_press_hold = arm;
            *c = k;
        },
        &[("Golem", (AT.0, AT.1 + 5000))],
    );
    assert!(s.config().cards.get(s.entity(lp).expect("him").card_idx).ramp.as_ref().is_some_and(|r| r.held_by_press), "precondition: the 160402017 row holds its ramp while tagged");
    let mine = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(lp)).count();
    let mut known = mine(&s);
    let (mut out, mut pressed) = (Vec::new(), 0);
    for k in 0..260 {
        hold(&mut s, lp, &reds);
        if k == press_at {
            pressed = s.tick_count();
            s.press_ability_button(Team::Blue, 0).expect("the press");
        }
        s.tick();
        let m = mine(&s);
        if m > known {
            out.push(s.tick_count() - 1);
        }
        known = m;
    }
    (out, pressed)
}

/// combat.RAMP_PRESS_HOLD = client16402_tag_held (client 16.402, parity's r65 census over the ob5 live set: 322 of 324
/// presses kept the count): pressed at his top level (x3, 8 ticks a shot), he comes back from the cast's hold still at
/// x3; under reset he starts again at x1 (24, 24, 12). Plants: ramp_press_resets, ramp_held_by_press_unread.
#[test]
fn under_client16402_tag_held_his_press_keeps_his_ramp() {
    let gaps_after = |arm: RampPressHold| {
        let (t, p) = pressed_shots(arm, 150);
        let before = gaps(&t.iter().copied().filter(|x| *x < p).collect::<Vec<_>>());
        assert!(before.last() == Some(&8), "precondition ({arm:?}): at x3 before the press: {before:?} (all {t:?})");
        let after: Vec<u32> = t.iter().copied().filter(|x| *x > p).collect();
        assert!(after.len() >= 4, "{arm:?}: shots after the press: {t:?}");
        gaps(&after)
    };
    let kept = gaps_after(RampPressHold::Client16402TagHeld);
    assert_eq!(&kept[..3], &[8, 8, 8], "client16402_tag_held: still x3 after the press: {kept:?}");
    let reset = gaps_after(RampPressHold::Reset);
    assert_eq!(&reset[..3], &[24, 24, 12], "reset (the vacuity check): x1 again after the press: {reset:?}");
}

/// The guard's hp losses by its age (ticks from its first frame), a red Zap cast on its point at each age in `zaps`,
/// under combat.GUARD_CHARGE_IMMUNITY = `arm`: the losses and its arrival's age (its last step).
fn guard_zapped(arm: GuardChargeImmunity, zaps: &[u32]) -> (Vec<(u32, i32)>, u32) {
    let (mut s, lp, _) = scene_cfg(|c| c.calib.guard_charge_immunity = arm, &[]);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let (mut first, mut last, mut arrived, mut hp_was): (Option<u32>, Option<Vec2>, Option<u32>, Option<i32>) = (None, None, None, None);
    let mut losses = Vec::new();
    for _ in 0..90 {
        assert!(s.debug_set_pos(lp, n(AT)));
        if let (Some(f), Some(p)) = (first, find_live(&s, Team::Blue, "ChampionGuard").first().map(|e| e.pos)) {
            if zaps.contains(&(s.tick_count() - f)) {
                s.spawn_unit(Team::Red, "Zap", p, None).expect("the Zap");
            }
        }
        s.tick();
        let Some((p, hp)) = find_live(&s, Team::Blue, "ChampionGuard").first().map(|e| (e.pos, e.hp)) else { continue };
        let f = *first.get_or_insert(s.tick_count() - 1);
        let age = s.tick_count() - 1 - f;
        if let Some(h) = hp_was.filter(|h| hp < *h) {
            losses.push((age, h - hp));
        }
        if arrived.is_none() && age > crate_dash_start() && last == Some(p) {
            arrived = Some(age - 1);
        }
        (last, hp_was) = (Some(p), Some(hp));
    }
    (losses, arrived.expect("the guard arrived"))
}

fn crate_dash_start() -> u32 {
    royalesim::card::GUARD_DASH_START_TICKS
}

/// combat.GUARD_CHARGE_IMMUNITY = client16402_charge_immune (client 16.402, parity's r65 census over the ob5 live set: 0
/// hp drops on 10,464 charge frames, 0 of 715 on the frame after, back on the second after): a Zap cast on the charging
/// guard (its age 10) takes nothing; one cast after its arrival + 5 takes hp; under none the first Zap takes hp too (the
/// vacuity check). Plant: guard_charge_hit.
#[test]
fn under_client16402_charge_immune_his_guard_takes_no_hit_in_its_charge() {
    let (losses, a) = guard_zapped(GuardChargeImmunity::Client16402ChargeImmune, &[10, 40]);
    assert!(a + 6 <= 40, "precondition: the second Zap after its arrival + 5 ({a})");
    let window = |l: &(u32, i32)| l.0 + 1 >= crate_dash_start() && l.0 <= a + 1;
    assert!(!losses.iter().any(window), "client16402_charge_immune: a loss in the charge (arrival {a}): {losses:?}");
    assert!(losses.iter().any(|l| l.0 >= 40), "client16402_charge_immune: the Zap after its arrival takes hp: {losses:?}");
    let (old, _) = guard_zapped(GuardChargeImmunity::None, &[10, 40]);
    assert!(old.iter().any(|l| (10..=12).contains(&l.0)), "none (the vacuity check): the Zap in the charge takes hp: {old:?}");
}

/// A red Knight swinging at a blue Golem (both held) beside the guard's path, let go when the guard first comes within
/// 2500 of it, under knockback.GUARD_PUSH_ATTACK_RESET = `arm`: the Knight's (attack phase, swing ms) on the frame before
/// its first push and on the push's frame.
fn pushed_swing(arm: GuardPushAttackReset) -> ((AttackPhase, i32), (AttackPhase, i32)) {
    let (mut s, lp, reds) = scene_cfg(|c| c.calib.guard_push_attack_reset = arm, &[("Knight", (AT.0 + 800, AT.1 + 1500))]);
    let knight = reds[0].0;
    let golem_at = n((AT.0 + 800, AT.1 + 2600));
    let golem = s.scenario_spawn_now(Team::Blue, "Golem", golem_at, None).expect("a blue Golem");
    // Everyone's hp held full (the Knight swings at the Golem, the Little Prince shoots the Knight).
    let full = |s: &BattleState, id: EntityId| s.entity(id).expect("a unit of the scene").max_hp;
    let (lp_hp, knight_hp, golem_hp) = (full(&s, lp), full(&s, knight), full(&s, golem));
    // Let the Knight take the Golem and swing.
    for _ in 0..40 {
        assert!(s.debug_set_pos(lp, n(AT)) && s.debug_set_hp(lp, lp_hp));
        assert!(s.debug_set_pos(knight, reds[0].1) && s.debug_set_hp(knight, knight_hp));
        assert!(s.debug_set_pos(golem, golem_at) && s.debug_set_hp(golem, golem_hp));
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut before = None;
    for _ in 0..70 {
        assert!(s.debug_set_pos(lp, n(AT)) && s.debug_set_hp(lp, lp_hp));
        assert!(s.debug_set_pos(golem, golem_at) && s.debug_set_hp(golem, golem_hp));
        assert!(s.debug_set_hp(knight, knight_hp));
        let k = s.entity(knight).expect("the Knight");
        let was = (k.attack_phase, k.attack_ms, k.push_active);
        if !was.2 {
            assert!(s.debug_set_pos(knight, reds[0].1));
        }
        s.tick();
        let k = s.entity(knight).expect("the Knight");
        if k.push_active && !was.2 {
            before = Some((was.0, was.1));
            return (before.expect("the frame before"), (k.attack_phase, k.attack_ms));
        }
    }
    panic!("the scene drifted: the guard never pushed the Knight ({before:?})");
}

/// knockback.GUARD_PUSH_ATTACK_RESET = client16402_reset (client 16.402, parity's r65 census over the ob5 live set: 143 of
/// 147 attacking ground troops lost their swing): the Knight swinging at the Golem goes Idle at 0 on the push's frame;
/// under frozen its swing stands (the vacuity check). Plant: guard_push_swing_frozen.
#[test]
fn under_client16402_reset_his_guards_push_resets_the_pushed_swing() {
    let (was, now) = pushed_swing(GuardPushAttackReset::Client16402Reset);
    assert!(was.0 != AttackPhase::Idle, "precondition: the Knight was swinging before the push: {was:?}");
    assert_eq!(now, (AttackPhase::Idle, 0), "client16402_reset: the swing on the push's frame (before {was:?})");
    let (was, now) = pushed_swing(GuardPushAttackReset::Frozen);
    assert!(was.0 != AttackPhase::Idle, "precondition (frozen): the Knight was swinging before the push: {was:?}");
    assert!(now.0 != AttackPhase::Idle && now.1 > 0, "frozen (the vacuity check): the swing stands on the push's frame: {now:?} (before {was:?})");
}

/// The guard charging with a red Knight held 8000 left of the Little Prince and 3000 ahead (out of the guard's reach all
/// through its charge), Blue's princess towers down, under combat.GUARD_LANDING_HITS = `arm`. With `enter` = Some(k) the
/// Knight is put 1500 ahead of the guard's end point on the guard's arrival + k (its last step's tick, `a`) and held
/// there. Returns `a`, the guard's end point and the ticks after `a`, through `a` + 8, on which the Knight lost the
/// guard's hit (256 or more: the push damage, 256 at level 11, or it and one of his shots).
fn landing_scene(arm: GuardLandingHits, enter: Option<(u32, Vec2)>) -> (u32, Vec2, Vec<u32>) {
    let far = (AT.0 - 8000, AT.1 + 3000);
    let (mut s, lp, reds) = scene_cfg(|c| c.calib.guard_landing_hits = arm, &[("Knight", far)]);
    let knight = reds[0].0;
    let towers: Vec<EntityId> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let (mut first, mut last, mut arrived, mut end): (Option<u32>, Option<Vec2>, Option<u32>, Option<Vec2>) = (None, None, None, None);
    let mut spot = n(far);
    let mut hits = Vec::new();
    for _ in 0..90 {
        assert!(s.debug_set_pos(lp, n(AT)));
        // `enter`'s tick k is the one about to run: the Knight stands in the guard's reach through tick k.
        if let Some((k, at)) = enter {
            if s.tick_count() == k {
                spot = at;
            }
        }
        let full = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_pos(knight, spot));
        assert!(s.debug_set_hp(knight, full));
        s.tick();
        let t = s.tick_count() - 1;
        let lost = full - s.entity(knight).expect("the Knight").hp;
        // The guard's arrival: past its dash start, the first tick it stood still; its last step was the tick before.
        let p = find_live(&s, Team::Blue, "ChampionGuard").first().map(|e| e.pos);
        if p.is_some() && first.is_none() {
            first = Some(t);
        }
        if arrived.is_none() && p.is_some() && last == p && first.is_some_and(|f| t > f + crate_dash_start()) {
            arrived = Some(t - 1);
            end = p;
        }
        last = p;
        if let Some(a) = arrived {
            if lost >= 256 && t <= a + 8 {
                hits.push(t - a);
            }
        }
    }
    let a = arrived.expect("the guard arrived");
    (a, end.expect("its end point"), hits)
}

/// combat.GUARD_LANDING_HITS = client16402_through_landing_ticks (client 16.402, parity's r68 census over the ob6 live
/// set's 737 guards: entrants on its last dash frame, the arrival + 1, and the two after hit 10 of 10, on the third after
/// 1 of 10, later none): a Knight put in the guard's reach on its arrival + 1, + 2 or + 3 takes its hit on that tick; one
/// put there on its arrival + 4 is not hit. Under charge_steps_only the Knight put there on the arrival + 1 is not hit (the
/// vacuity check). Plants: landing_hits_end_at_arrival, landing_hits_unbounded.
#[test]
fn under_client16402_through_landing_ticks_his_guard_hits_through_its_arrival_plus_three() {
    let arm = GuardLandingHits::Client16402ThroughLandingTicks;
    let (a, end, none) = landing_scene(arm, None);
    assert!(none.is_empty(), "precondition: the Knight out of the guard's reach all through its charge: {none:?}");
    let spot = Vec2::new(end.x, end.y + 1500 * K);
    for k in 1..=3 {
        let (a2, _, hits) = landing_scene(arm, Some((a + k, spot)));
        assert_eq!(a2, a, "the scene's arrival moved");
        assert_eq!(hits, [k], "put in reach on the arrival + {k}: hit on that tick, once (arrival {a})");
    }
    let (_, _, hits) = landing_scene(arm, Some((a + 4, spot)));
    assert!(hits.is_empty(), "put in reach on the arrival + 4: not hit (arrival {a}): {hits:?}");
    let (a1, _, hits) = landing_scene(GuardLandingHits::ChargeStepsOnly, Some((a + 1, spot)));
    assert_eq!(a1, a, "the old arm's arrival moved");
    assert!(hits.is_empty(), "charge_steps_only (the vacuity check): hit after the arrival: {hits:?}");
}

/// The Little Prince let walk (no enemy in sight) 10 ticks after `scene`'s hold, then pressed, under combat.GUARD_PRESS_HOLD
/// = `arm`: the ticks from the press's tick (the last one run before it) to his first tick of moving after it.
fn first_step_after_press(arm: GuardPressHold) -> u32 {
    let (mut s, lp, _) = scene_cfg(|c| c.calib.guard_press_hold = arm, &[]);
    let mut last = s.entity(lp).expect("him").pos;
    let mut walked = 0;
    for _ in 0..10 {
        s.tick();
        let p = s.entity(lp).expect("him").pos;
        walked += u32::from(p != last);
        last = p;
    }
    assert!(walked >= 5, "precondition: he walks before the press ({walked} moving ticks of 10)");
    let p0 = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    // The press's first tick holds him: skip the ticks he still walks on before it lands, then find his first step after.
    let mut held = false;
    for _ in 0..80 {
        s.tick();
        let p = s.entity(lp).expect("him").pos;
        let moving = p != last;
        last = p;
        if !moving {
            held = true;
        } else if held {
            return s.tick_count() - 1 - p0;
        }
    }
    panic!("he never walked again after the press");
}

/// combat.GUARD_PRESS_HOLD = client16402_three_ticks_longer (client 16.402, parity's r69 census over the ob6 live set: his
/// first step after a press on the press + 33 in 39 presses, never + 30; the engine's table time stepped on + 30): his
/// first step comes three ticks later than under table_time (the vacuity check: the old arm's own step). Plant:
/// press_hold_table_time.
#[test]
fn under_client16402_three_ticks_longer_his_press_holds_him_three_ticks_past_its_time() {
    let old = first_step_after_press(GuardPressHold::TableTime);
    let new = first_step_after_press(GuardPressHold::Client16402ThreeTicksLonger);
    assert!((25..=35).contains(&old), "precondition: under table_time his hold of 1500 ms ends about 30 ticks on ({old})");
    assert_eq!(new, old + 3, "client16402_three_ticks_longer: his first step after the press (table_time's {old})");
}

/// The Little Prince pressed at AT (held there before), a Blue Knight put 400 ahead of him (overlapping) on the press's
/// tick + 2 and held there, under collision.GUARD_PRESS_CONTACT = `arm`: his greatest distance from his press point over
/// the press + 3 .. + 15 (his hold, before his guard appears), native.
fn press_pushed(arm: GuardPressContact) -> i64 {
    let (mut s, lp, _) = scene_cfg(|c| c.calib.guard_press_contact = arm, &[]);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let p0 = s.entity(lp).expect("him").pos;
    let knight_at = Vec2::new(p0.x, p0.y + 400 * K);
    let mut knight = None;
    let mut most = 0i64;
    for k in 1..=15 {
        if k == 2 {
            knight = Some(s.scenario_spawn_now(Team::Blue, "Knight", knight_at, None).expect("the Knight"));
        }
        if let Some(id) = knight {
            assert!(s.debug_set_pos(id, knight_at));
        }
        s.tick();
        if k >= 3 {
            let p = s.entity(lp).expect("him").pos;
            let (dx, dy) = (((p.x - p0.x) / K) as i64, ((p.y - p0.y) / K) as i64);
            most = most.max(dx.abs().max(dy.abs()));
        }
    }
    most
}

/// collision.GUARD_PRESS_CONTACT = client16402_unpushed (client 16.402, parity's r69 contact study: in his press's hold with
/// a unit overlapping him he stood still on 266 frames, moved on 4, all knockbacks): a Knight held on him does not move
/// him in his hold; under pushed (the vacuity check) it pushes him. Plant: press_contact_pushed.
#[test]
fn under_client16402_unpushed_a_knight_on_him_does_not_move_him_in_his_press() {
    assert_eq!(press_pushed(GuardPressContact::Client16402Unpushed), 0, "client16402_unpushed: he moved in his press");
    assert!(press_pushed(GuardPressContact::Pushed) > 0, "pushed (the vacuity check): the Knight never moved him");
}

/// The guard's first frame under combat.GUARD_BIRTH_POINT = `arm`, the Little Prince pressed at `at` (held there): (ticks
/// from the press, its point less his, native, in his side's frame; Blue's frame is the arena's), and the route it holds on
/// its first dash frame (its birth + GUARD_DASH_START_TICKS).
fn guard_birth_at(arm: GuardBirthPoint, at: (i32, i32)) -> (u32, (i32, i32), usize) {
    let mut cfg = config();
    cfg.calib.guard_birth_point = arm;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let lp = s.scenario_spawn_now(Team::Blue, "LittlePrince", n(at), None).expect("the Little Prince");
    for _ in 0..30 {
        assert!(s.debug_set_pos(lp, n(at)));
        s.tick();
    }
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut first: Option<(u32, (i32, i32))> = None;
    for _ in 0..40 {
        assert!(s.debug_set_pos(lp, n(at)));
        s.tick();
        let age = s.tick_count() - 1 - p;
        if let Some(e) = find_live(&s, Team::Blue, "ChampionGuard").first() {
            if first.is_none() {
                first = Some((age, ((e.pos.x / K) - at.0, (e.pos.y / K) - at.1)));
            }
            let (born, _) = first.expect("its first frame");
            if age == born + 7 {
                return (born, first.expect("its first frame").1, e.route.len());
            }
        }
    }
    panic!("no guard's first dash frame in 40 ticks")
}

/// The guard's first step from 2000 behind him, native: (dx, dy, its squared length).
fn birth_step(off: (i32, i32)) -> (i32, i32, i64) {
    let (sx, sy) = (off.0, off.1 + 2000);
    (sx, sy, i64::from(sx).pow(2) + i64::from(sy).pow(2))
}

/// One walk step's length, 55 to 61 native, squared.
const STEP2: std::ops::RangeInclusive<i64> = 55 * 55..=61 * 61;

/// combat.GUARD_BIRTH_POINT = client16402_behind_2000_first_step (client 16.402, parity's r69 contact study: 689 of 694 stepping
/// births at his point + (0, -2000) and one walk step of 59-60 toward a neighbouring cell's centre; parity's agents11 trace:
/// no target on the birth frame, its route dropped as the dash starts): the guard's first frame, on the press + 18 as
/// before, stands one step (55 to 61) from 2000 behind him, and it holds no route on its first dash frame. At his usual
/// point the step is (18, 56): client 15.535.29's measured (18, -1944) is this birth and this step. Near the arena's left
/// edge (x 1500) the route to the lane heads right, and the step is not (18, 56), where client15535_fixed_offset (the
/// vacuity check) stands on (18, -1944) in both. Pressed at (14500, 9000) his guard is born 500 from his own princess
/// tower's centre: its step is its route's, (18, 56), unsteered (client 16.402 steps straight toward its waypoint; the
/// engine's avoidance scan had steered round the tower on 35 of 43 step mismatches). Plants: guard_birth_fixed (the edge
/// scene), guard_birth_no_step (both), guard_route_kept (the dash frame's route), guard_birth_sees_all (by the tower: its
/// step steered to (-49, 35)).
#[test]
fn under_client16402_behind_2000_first_step_his_guard_is_born_2000_behind_and_steps() {
    let new = GuardBirthPoint::Client16402Behind2000FirstStep;
    let (t, off, route) = guard_birth_at(new, AT);
    assert_eq!(t, 18, "its first frame, from the press");
    let (sx, sy, len) = birth_step(off);
    assert!(STEP2.contains(&len), "client16402_behind_2000_first_step: its first frame is sqrt({len}) from 2000 behind him ({off:?})");
    assert_eq!((sx, sy), (18, 56), "at his usual point: the step ahead and a little right ({off:?})");
    assert_eq!(route, 0, "client16402_behind_2000_first_step: its first dash frame holds the birth step's route");
    let edge = (1500, AT.1);
    let (t, off, _) = guard_birth_at(new, edge);
    assert_eq!(t, 18, "its first frame, from the press (the edge scene)");
    let (sx, sy, len) = birth_step(off);
    assert!(STEP2.contains(&len) && (sx, sy) != (18, 56), "client16402_behind_2000_first_step: by the edge the step is not its route's ({off:?}, squared {len})");
    let (t, off, _) = guard_birth_at(new, (14500, 9000));
    assert_eq!(t, 18, "its first frame, from the press (by his princess tower)");
    let (sx, sy, _) = birth_step(off);
    assert_eq!((sx, sy), (18, 56), "by his own princess tower: the step is its route's, unsteered ({off:?})");
    for at in [AT, edge] {
        let (t, off, _) = guard_birth_at(GuardBirthPoint::Client15535FixedOffset, at);
        assert_eq!((t, off), (18, (18, -1944)), "client15535_fixed_offset (the vacuity check) at {at:?}");
    }
}

/// The guard after its charge under combat.GUARD_FREE_TICK = `arm`, a red Knight held to its side: (its arrival's age, from
/// its first frame; the age of its first target after the arrival).
fn guard_landing(arm: GuardFreeTick) -> (u32, u32) {
    let (mut s, lp, reds) = scene_cfg(|c| c.calib.guard_free_tick = arm, &[("Knight", (11500, 12500))]);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let (mut first, mut last, mut arrived): (Option<u32>, Option<Vec2>, Option<u32>) = (None, None, None);
    for _ in 0..90 {
        hold(&mut s, lp, &reds);
        s.tick();
        let Some((p, target)) = find_live(&s, Team::Blue, "ChampionGuard").first().map(|e| (e.pos, e.target)) else { continue };
        let f = *first.get_or_insert(s.tick_count() - 1);
        let age = s.tick_count() - 1 - f;
        match arrived {
            None if age > crate_dash_start() && last == Some(p) => arrived = Some(age - 1),
            Some(a) if target.is_some() => return (a, age),
            _ => {}
        }
        last = Some(p);
    }
    panic!("{arm:?}: the guard never took a target after its charge")
}

/// combat.GUARD_FREE_TICK = client16402_held_through_arrival_plus_1 (client 16.402, parity's r71 census: the first target on
/// L + 2 in 621 of 628 landings, never on L + 1; the engine's on L + 1): the guard takes its first target after the arrival a
/// tick later than under free_on_arrival_plus_1, the arrival itself the same. Plant: guard_free_on_arrival_plus_1.
#[test]
fn under_client16402_held_through_arrival_plus_1_his_guard_targets_a_tick_later() {
    let (a_old, t_old) = guard_landing(GuardFreeTick::FreeOnArrivalPlus1);
    let (a_new, t_new) = guard_landing(GuardFreeTick::Client16402HeldThroughArrivalPlus1);
    assert_eq!(a_old, a_new, "the arrival is the same under both arms");
    assert_eq!(t_new, t_old + 1, "client16402_held_through_arrival_plus_1: the first target a tick later ({t_old} -> {t_new}, arrival {a_new})");
}

/// The Little Prince pressed at AT (held there before) with a Red Tornado cast 2,000 to his side on the press's own gap,
/// under status.GUARD_PRESS_ATTRACT = `arm`: his greatest distance from his press point over the press + 1 .. + 15 (his cast
/// and hold, before his guard appears), native.
fn press_tornado(arm: royalesim::state::GuardPressAttract) -> i64 {
    let (mut s, lp, _) = scene_cfg(|c| c.calib.guard_press_attract = arm, &[]);
    let p0 = s.entity(lp).expect("him").pos;
    s.spawn_unit(Team::Red, "Tornado", Vec2::new(p0.x + 2000 * K, p0.y), None).expect("the Tornado");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut most = 0i64;
    for _ in 1..=15 {
        s.tick();
        let p = s.entity(lp).expect("him").pos;
        let (dx, dy) = (((p.x - p0.x) / K) as i64, ((p.y - p0.y) / K) as i64);
        most = most.max(dx.abs().max(dy.abs()));
    }
    most
}

/// status.GUARD_PRESS_ATTRACT = client16402_unpulled_through_hold (client 16.402, parity's r73 census: in a Tornado through
/// his press's cast and hold he stood still on 40 of 41 ticks; the engine pulled him on 38 of 38): a Tornado beside him does
/// not move him in his press; under pulled (the vacuity check) it does. Plant: press_hold_pulled.
#[test]
fn under_client16402_unpulled_through_hold_a_tornado_does_not_move_him_in_his_press() {
    use royalesim::state::GuardPressAttract as G;
    assert!(press_tornado(G::Pulled) > 0, "pulled (the vacuity check): the Tornado never moved him");
    assert_eq!(press_tornado(G::Client16402UnpulledThroughHold), 0, "client16402_unpulled_through_hold: he moved in his press");
}

/// The Little Prince at AT pressed with a Red Knight held 3,000 ahead (he attacks it from his cast's end), a Blue Knight put
/// 400 ahead of him on the press + 19 and held there, under collision.GUARD_PRESS_CONTACT = `arm`: (his greatest distance
/// from his point on the press + 19 over the press + 20 .. + 32, native; whether he attacked on the press + 20).
fn press_attacking_pushed(arm: GuardPressContact) -> (i64, bool) {
    let red_at = (AT.0, AT.1 + 3000);
    let (mut s, lp, reds) = scene_cfg(|c| c.calib.guard_press_contact = arm, &[("Knight", red_at)]);
    let hold_reds = |s: &mut BattleState| {
        for (id, p) in &reds {
            if let Some(e) = s.entity(*id) {
                let full = e.max_hp;
                assert!(s.debug_set_pos(*id, *p) && s.debug_set_hp(*id, full));
            }
        }
    };
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 1..=19 {
        hold_reds(&mut s);
        s.tick();
    }
    let p0 = s.entity(lp).expect("him").pos;
    let knight_at = Vec2::new(p0.x, p0.y + 400 * K);
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", knight_at, None).expect("the Knight");
    let (mut most, mut attacking) = (0i64, false);
    for k in 20..=32 {
        hold_reds(&mut s);
        assert!(s.debug_set_pos(knight, knight_at));
        s.tick();
        let v = s.entity(lp).expect("him");
        if k == 20 {
            attacking = v.attack_phase != AttackPhase::Idle;
        }
        let (dx, dy) = (((v.pos.x - p0.x) / K) as i64, ((v.pos.y - p0.y) / K) as i64);
        most = most.max(dx.abs().max(dy.abs()));
    }
    (most, attacking)
}

/// collision.GUARD_PRESS_CONTACT = client16402_unpushed_attacking (client 16.402, parity's r74 census: attacking in his hold he
/// moved on 3 of 282 overlap ticks; the engine pushed him on 35 of 67, from the press + 18): a Knight held on him while he
/// attacks in his hold does not move him; under client16402_unpushed (the vacuity check) it does. Plant: press_attacking_pushed.
#[test]
fn under_client16402_unpushed_attacking_a_knight_on_him_does_not_move_him_while_he_attacks_in_his_hold() {
    let (old, attacking) = press_attacking_pushed(GuardPressContact::Client16402Unpushed);
    assert!(attacking, "the scene drifted: he was not attacking on the press + 20");
    assert!(old > 0, "client16402_unpushed (the vacuity check): the Knight never moved him");
    let (new, _) = press_attacking_pushed(GuardPressContact::Client16402UnpushedAttacking);
    assert_eq!(new, 0, "client16402_unpushed_attacking: he moved while he attacked in his hold");
}

/// combat.GUARD_CHARGE_IMMUNITY = client16402_through_last_dash_plus_1 (client 16.402, parity's r74 census: no hit on the frame
/// after the guard's last dash frame, the engine's 4): a Zap landing on its arrival + 2 takes nothing, one on its arrival + 3
/// takes hp; under client16402_charge_immune (the vacuity check) the arrival + 2's Zap takes hp. Plant:
/// guard_immunity_tick_short.
#[test]
fn under_client16402_through_last_dash_plus_1_his_guard_is_immune_a_tick_longer() {
    let (_, a) = guard_zapped(GuardChargeImmunity::Client16402ThroughLastDashPlus1, &[]);
    let (old, _) = guard_zapped(GuardChargeImmunity::Client16402ChargeImmune, &[a + 2]);
    assert!(old.iter().any(|l| l.0 == a + 2), "client16402_charge_immune (the vacuity check): the Zap on its arrival + 2 took nothing: {old:?}");
    let (new, _) = guard_zapped(GuardChargeImmunity::Client16402ThroughLastDashPlus1, &[a + 2]);
    assert!(!new.iter().any(|l| l.0 == a + 2), "client16402_through_last_dash_plus_1: the Zap on its arrival + 2 took hp: {new:?}");
    let (later, _) = guard_zapped(GuardChargeImmunity::Client16402ThroughLastDashPlus1, &[a + 3]);
    assert!(later.iter().any(|l| l.0 == a + 3), "client16402_through_last_dash_plus_1: the Zap on its arrival + 3 took nothing: {later:?}");
}

/// The Little Prince at AT pressed with a Red Knight held at `red_at`, near his guard's end point, let go once the guard comes
/// within 2,500 of it, under combat.GUARD_LANDING_PUSH = `arm`: the Knight's point after each of the 60 ticks from the press,
/// and the tick (from the press) the guard first stood still after it moved (its arrival).
fn landing_pushed(arm: royalesim::state::GuardLandingPush, red_at: (i32, i32)) -> (Vec<Vec2>, u32) {
    let (mut s, lp, reds) = scene_cfg(|c| c.calib.guard_landing_push = arm, &[("Knight", red_at)]);
    let knight = reds[0].0;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let (mut out, mut held, mut last, mut arrived, mut charged) = (Vec::new(), true, None, None, false);
    for k in 1..=60u32 {
        if s.entity(lp).is_some() {
            assert!(s.debug_set_pos(lp, n(AT)));
        }
        if held {
            let full = s.entity(knight).expect("the Knight").max_hp;
            assert!(s.debug_set_pos(knight, n(red_at)) && s.debug_set_hp(knight, full));
        }
        s.tick();
        out.push(s.entity(knight).expect("the Knight").pos);
        if let Some(g) = find_live(&s, Team::Blue, "ChampionGuard").first().map(|e| e.pos) {
            let kp = s.entity(knight).unwrap().pos;
            let (dx, dy) = (((g.x - kp.x) / K) as i64, ((g.y - kp.y) / K) as i64);
            if held && dx * dx + dy * dy < 2500 * 2500 {
                held = false;
            }
            if last.is_some_and(|l: Vec2| l != g) {
                charged = true;
            }
            if arrived.is_none() && charged && last == Some(g) {
                arrived = Some(k - 1);
            }
            last = Some(g);
        }
    }
    (out, arrived.expect("the guard never arrived"))
}

/// combat.GUARD_LANDING_PUSH = client16402_through_landing_ticks (client 16.402, parity's r74 census: 26 victims repeat a ladder
/// step inside the guard's arrival .. + 3, the engine's 0): over Knight points about the guard's end, the Knight's points agree
/// with stops_at_arrival's through the arrival, and on at least one the new arm moves it farther over the 10 ticks after it
/// (its ladder re-armed) than stops_at_arrival (the vacuity check), whose ladder only runs down. Plant: landing_push_stops.
#[test]
fn under_client16402_through_landing_ticks_his_guards_push_rearms_after_its_arrival() {
    use royalesim::state::GuardLandingPush as G;
    let dist = |p: Vec2, q: Vec2| {
        let (dx, dy) = (((p.x - q.x) / K) as i64, ((p.y - q.y) / K) as i64);
        royalesim::fixed::isqrt(dx * dx + dy * dy)
    };
    let (mut farther, mut seen) = (0, Vec::new());
    for (dx, dy) in [(600, 3600), (0, 3600), (300, 3300), (600, 3000), (900, 3900), (0, 4200), (600, 4200), (300, 4500)] {
        let red_at = (AT.0 + dx, AT.1 + dy);
        let (old, a) = landing_pushed(G::StopsAtArrival, red_at);
        let (new, a2) = landing_pushed(G::Client16402ThroughLandingTicks, red_at);
        assert_eq!(a, a2, "{red_at:?}: the guard's arrival moved between the arms");
        let k = a as usize;
        assert!(k + 9 < old.len(), "{red_at:?}: the guard arrived too late to read ({a})");
        assert_eq!(old[..k], new[..k], "{red_at:?}: the arms parted before the guard's arrival");
        let (o, nw) = (dist(old[k - 1], old[k + 9]), dist(new[k - 1], new[k + 9]));
        seen.push((red_at, a, o, nw));
        if nw > o {
            farther += 1;
        }
    }
    assert!(farther > 0, "client16402_through_landing_ticks: the Knight never pushed farther after the arrival (point, arrival, old, new): {seen:?}");
}

/// The shipped config (the 16.402 charge: cell aim, the birth behind him, held through the arrival + 1) under `arm`: the point
/// the guard's charge ends on with the Little Prince held on `at` (as `charge_end_at` finds it: after its last step over
/// 150), and whether that point is off water.
fn guard_end_live(arm: GuardWaterLanding, at: (i32, i32)) -> ((i32, i32), bool) {
    let mut cfg = config();
    cfg.calib.guard_water_landing = arm;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let lp = s.scenario_spawn_now(Team::Blue, "LittlePrince", n(at), None).expect("the Little Prince");
    for _ in 0..30 {
        assert!(s.debug_set_pos(lp, n(at)));
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut pts: Vec<(i32, i32)> = Vec::new();
    for _ in 0..60 {
        assert!(s.debug_set_pos(lp, n(at)));
        s.tick();
        if let Some(e) = find_live(&s, Team::Blue, "ChampionGuard").first() {
            pts.push((e.pos.x / K, e.pos.y / K));
        }
    }
    let mut end = *pts.first().expect("a guard");
    let mut charging = false;
    for w in pts.windows(2) {
        let (dx, dy) = ((w[1].0 - w[0].0) as i64, (w[1].1 - w[0].1) as i64);
        if dx * dx + dy * dy > 150 * 150 {
            charging = true;
            end = w[1];
        } else if charging {
            break;
        }
    }
    (end, s.config().arena.is_valid_position(n(end)))
}

/// combat.GUARD_WATER_LANDING = client16402_nearest_land_lattice (client 16.402, parity r75, live13: 41 of 41 water landings
/// on the lattice, 1092 of 1092 land landings unmoved): from (6777, 13222) his charge's cell aim, (6750, 16250), lies in the
/// river; the old arm leaves the guard there; the new arm puts him on the nearest point of the landing's 250-odd lattice off
/// water (every nearer lattice point water; ties to the lower y, then x). From AT (a land aim) both arms agree. The vacuity
/// check: the old arm's end is water. Plant: guard_lands_in_water.
#[test]
fn under_client16402_nearest_land_lattice_a_guard_landing_in_the_river_stands_on_the_nearest_land_point() {
    let at = (6777, 13222);
    let (old, old_land) = guard_end_live(GuardWaterLanding::InWater, at);
    assert!(!old_land && (15000..17000).contains(&old.1), "the scene: the old arm lands in the river ({old:?})");
    let (new, new_land) = guard_end_live(GuardWaterLanding::Client16402NearestLandLattice, at);
    assert!(new_land, "client16402_nearest_land_lattice: the guard stands on water ({new:?})");
    let (dx, dy) = (new.0 - old.0, new.1 - old.1);
    assert!(dx % 500 != 0 && dx % 250 == 0 && dy % 500 != 0 && dy % 250 == 0, "not on the landing's lattice: ({dx}, {dy})");
    let s = BattleState::new(0, config());
    let d2 = |x: i32, y: i32| i64::from(x) * i64::from(x) + i64::from(y) * i64::from(y);
    for i in -6..6 {
        for j in -6..6 {
            let (ox, oy) = (250 * (2 * i + 1), 250 * (2 * j + 1));
            if (d2(ox, oy), old.1 + oy, old.0 + ox) < (d2(dx, dy), new.1, new.0) {
                assert!(!s.config().arena.is_valid_position(n((old.0 + ox, old.1 + oy))), "a nearer land point ({ox}, {oy}) than ({dx}, {dy})");
            }
        }
    }
    let (land_old, _) = guard_end_live(GuardWaterLanding::InWater, AT);
    assert_eq!(guard_end_live(GuardWaterLanding::Client16402NearestLandLattice, AT).0, land_old, "a land landing moved");
}

/// Under combat.RAMP_HOOK_RESET = `arm` (combat.SPECIAL_HOOK = client_hook_drag; Blue's princess towers down): his ramp at
/// its third speed on a red Golem held 5000 ahead, then a red Fisherman (held, topped up) 6800 off hooks him; after the drag
/// (his steps of about 510) the gaps of his hits on the Golem, which he shoots on from where the drag left him (its
/// hitpoints' drops: one shot is always in flight from there, so the projectile count cannot tell his shots).
fn hooked_gaps(arm: RampHookReset) -> Vec<u32> {
    let (mut s, lp, _) = scene_cfg(
        |c| {
            c.calib.ramp_hook_reset = arm;
            c.calib.special_hook = SpecialHook::ClientHookDrag;
        },
        &[],
    );
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    let at = (AT.0, AT.1 + 5000);
    let golem = s.scenario_spawn_now(Team::Red, "Golem", n(at), None).expect("a red Golem");
    let t = shots(&mut s, lp, &[(golem, n(at))], 160, None);
    assert!(gaps(&t).ends_with(&[8, 8]), "the scene: his ramp at its third speed before the hook {t:?}");
    let f_at = n((AT.0 - 6500, AT.1 + 2000));
    let fisher = s.scenario_spawn_now(Team::Red, "Fisherman", f_at, None).expect("the Fisherman");
    let (mut out, mut dragged, mut done) = (Vec::new(), 0, None);
    let mut last = s.entity(lp).expect("him").pos;
    for k in 0..260 {
        for (id, p) in [(golem, n(at)), (fisher, f_at)] {
            if let Some(e) = s.entity(id) {
                let full = e.max_hp;
                assert!(s.debug_set_hp(id, full));
                if done.is_none() || id == golem {
                    assert!(s.debug_set_pos(id, p));
                }
            }
        }
        let full = s.entity(lp).expect("him").max_hp;
        assert!(s.debug_set_hp(lp, full));
        s.tick();
        let now = s.entity(lp).expect("him").pos;
        let (dx, dy) = (i64::from((now.x - last.x) / K), i64::from((now.y - last.y) / K));
        last = now;
        if (480 * 480..=540 * 540).contains(&(dx * dx + dy * dy)) {
            dragged += 1;
        } else if dragged > 0 && done.is_none() {
            done = Some(k);
        }
        let g = s.entity(golem).expect("the Golem");
        if done.is_some() && g.hp < g.max_hp {
            out.push(s.tick_count() - 1);
        }
    }
    assert!(dragged >= 3 && done.is_some(), "the scene drifted: no hook drag ({dragged} drag ticks)");
    gaps(&out)
}

/// combat.RAMP_HOOK_RESET = client16402_hook_disables (client 16.402, parity r75, live13: 5 of 5 ramped hooks came out at
/// x1, the engine kept the level 5 of 5): after the drag his shots are 24 apart (his first speed); under kept 8 apart (the
/// vacuity check). Plant: ramp_hook_kept.
#[test]
fn under_client16402_hook_disables_a_hook_resets_his_ramp() {
    // The first gap may start from a shot fired before the drag that lands after it.
    let old = hooked_gaps(RampHookReset::Kept);
    assert!(old.len() >= 3 && old[1..3] == [8, 8], "kept: his ramp after the drag {old:?}");
    let new = hooked_gaps(RampHookReset::Client16402HookDisables);
    assert!(new.len() >= 3 && (new[..2] == [24, 24] || new[1..3] == [24, 24]), "client16402_hook_disables: his ramp after the drag {new:?}");
}

/// Under combat.RAMP_LEVEL_DROP = `arm` (RAMP_GRACE_MOVE = point_changed, RAMP_GRACE_RESET = client16402_below_zero): his
/// ramp at its third speed on a red Golem 5000 ahead, then `walk` moving ticks (his point moved 30 a tick), the Golem away
/// for all but the last, put back 5000 ahead of him on the last (the walk's last tick is his entry's): his shot ticks after,
/// from the walk's last tick.
fn walked_shots(arm: RampLevelDrop, walk: i32) -> Vec<u32> {
    let (mut s, lp, _) = scene_cfg(
        |c| {
            c.calib.ramp_level_drop = arm;
            c.calib.ramp_grace_reset = RampGraceReset::Client16402BelowZero;
            c.calib.ramp_grace_move = RampGraceMove::PointChanged;
        },
        &[],
    );
    let at = (AT.0, AT.1 + 5000);
    let golem = s.scenario_spawn_now(Team::Red, "Golem", n(at), None).expect("a red Golem");
    let t = shots(&mut s, lp, &[(golem, n(at))], 160, None);
    assert!(gaps(&t).ends_with(&[8, 8]), "the scene: his ramp at its third speed before the walk {t:?}");
    let away = n((AT.0, 31000));
    let me = (AT.0 + 30 * walk, AT.1);
    let back = (me.0, me.1 + 5000);
    for k in 1..=walk {
        assert!(s.debug_set_pos(lp, n((AT.0 + 30 * k, AT.1))));
        assert!(s.debug_set_pos(golem, if k == walk { n(back) } else { away }));
        s.tick();
    }
    let end = s.tick_count() - 1;
    let mine = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(lp)).count();
    let mut known = mine(&s);
    let mut out = Vec::new();
    for _ in 0..120 {
        assert!(s.debug_set_pos(lp, n(me)));
        if let Some(e) = s.entity(golem) {
            let full = e.max_hp;
            assert!(s.debug_set_pos(golem, n(back)));
            assert!(s.debug_set_hp(golem, full));
        }
        s.tick();
        let m = mine(&s);
        if m > known {
            out.push(s.tick_count() - 1 - end);
        }
        known = m;
    }
    out
}

/// combat.RAMP_LEVEL_DROP = client16402_after_the_attack (client 16.402, parity r75, live13: 25 of 25 entries the tick after
/// his 7th walking tick ran at the old level, x1 after; 31 of 31 after 8 at x1): an entry on the grace's reset tick runs
/// at his old (third) speed, so his first shot comes sooner than under with_the_count (the vacuity check), and his shots
/// after are at his first speed under both; an entry a tick after the reset is the same under both. Plant:
/// ramp_level_drops_with_count.
#[test]
fn under_client16402_after_the_attack_an_entry_on_the_reset_tick_runs_at_his_old_speed() {
    let (o7, n7) = (walked_shots(RampLevelDrop::WithTheCount, 7), walked_shots(RampLevelDrop::Client16402AfterTheAttack, 7));
    assert!(o7.len() >= 3 && n7.len() >= 3, "the scene drifted: {o7:?} {n7:?}");
    assert!(n7[0] < o7[0], "client16402_after_the_attack: the entry on the reset tick ran at his first speed ({n7:?} against {o7:?})");
    assert_eq!(gaps(&o7)[..2], [24, 24], "with_the_count: his shots after the reset {o7:?}");
    assert_eq!(gaps(&n7)[..2], [24, 24], "client16402_after_the_attack: his shots after the entry {n7:?}");
    assert_eq!(walked_shots(RampLevelDrop::WithTheCount, 8), walked_shots(RampLevelDrop::Client16402AfterTheAttack, 8), "an entry a tick after the reset");
}

/// Under combat.GUARD_BIRTH_PASS_THROUGH = `arm`: the Little Prince held on AT and pressed; a red Log cast 4000 north of his
/// guard's birth point `cast_at` ticks after the press (it rolls south at it), or a red Zap on the guard's point on its age 1
/// (`zap`): the guard's hitpoint losses by its age (ticks from its first frame).
fn guard_hit(arm: GuardBirthPassThrough, cast_at: u32, zap: bool) -> Vec<(u32, i32)> {
    let mut cfg = config();
    cfg.calib.guard_birth_pass_through = arm;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), ["Log", "Zap", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions"].iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let lp = s.scenario_spawn_now(Team::Blue, "LittlePrince", n(AT), None).expect("the Little Prince");
    for _ in 0..30 {
        assert!(s.debug_set_pos(lp, n(AT)));
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let (mut first, mut hp_was, mut losses) = (None::<u32>, None::<i32>, Vec::new());
    for k in 0..60 {
        assert!(s.debug_set_pos(lp, n(AT)));
        if !zap && k == cast_at {
            s.spawn_unit(Team::Red, "Log", n((AT.0, AT.1 - 2000 + 4000)), None).expect("the Log");
        }
        if let (true, Some(f)) = (zap, first) {
            if s.tick_count() - f == 1 {
                let p = find_live(&s, Team::Blue, "ChampionGuard").first().expect("the guard").pos;
                s.spawn_unit(Team::Red, "Zap", p, None).expect("the Zap");
            }
        }
        s.tick();
        let Some(hp) = find_live(&s, Team::Blue, "ChampionGuard").first().map(|e| e.hp) else { continue };
        let f = *first.get_or_insert(s.tick_count() - 1);
        if let Some(h) = hp_was.filter(|h| hp < *h) {
            losses.push((s.tick_count() - 1 - f, h - hp));
        }
        hp_was = Some(hp);
    }
    losses
}

/// combat.GUARD_BIRTH_PASS_THROUGH = client16402_passed_over (client 16.402, parity r77, live13: Log rolls over guards on their
/// birth frames hit 0 of 7; engine-only birth-frame hits on 7 of 369 guards): a Log whose roll crosses the guard before its
/// first charge frame takes nothing off it, where hit (the vacuity check) does; a Zap's area on its age 1 lands under both.
/// Plant: guard_birth_pass_through_hit.
#[test]
fn under_client16402_passed_over_a_log_rolls_over_his_guard_on_its_birth_frames() {
    let birth = |l: &[(u32, i32)]| l.iter().any(|(age, _)| *age < crate_dash_start());
    let cast = (0..30).find(|&c| birth(&guard_hit(GuardBirthPassThrough::Hit, c, false))).expect("the scene drifted: no cast tick lands the Log's roll on the guard's birth frames under hit");
    let new = guard_hit(GuardBirthPassThrough::Client16402PassedOver, cast, false);
    assert!(!birth(&new), "client16402_passed_over: the Log hit the guard on its birth frames (cast {cast}): {new:?}");
    assert!(birth(&guard_hit(GuardBirthPassThrough::Client16402PassedOver, 0, true)), "client16402_passed_over: the Zap's area missed the guard on its birth frames");
}

/// The guard's points by its age 0..30 (ticks from its first frame), a red Log cast 2500 ahead of it at its age `log_at` (it rolls
/// back into the charge, inside the guard's shipped immunity), under knockback.DASH_IMMUNE_PUSH = `arm`; and whether its hp ever dropped.
fn guard_track(arm: DashImmunePush, log_at: Option<u32>) -> (Vec<Vec2>, bool) {
    let (mut s, lp, _) = scene_cfg(|c| c.calib.dash_immune_push = arm, &[]);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let (mut first, mut hp_was, mut lost): (Option<u32>, Option<i32>, bool) = (None, None, false);
    let mut track = Vec::new();
    for _ in 0..90 {
        assert!(s.debug_set_pos(lp, n(AT)));
        if let (Some(f), Some(p)) = (first, find_live(&s, Team::Blue, "ChampionGuard").first().map(|e| e.pos)) {
            if log_at == Some(s.tick_count() - f) {
                s.spawn_unit(Team::Red, "Log", Vec2::new(p.x, p.y + 2500 * K), None).expect("the Log");
            }
        }
        s.tick();
        let Some((p, hp)) = find_live(&s, Team::Blue, "ChampionGuard").first().map(|e| (e.pos, e.hp)) else { continue };
        let f = *first.get_or_insert(s.tick_count() - 1);
        if s.tick_count() - 1 - f >= 30 {
            break;
        }
        lost |= hp_was.is_some_and(|h| hp < h);
        hp_was = Some(hp);
        track.push(p);
    }
    (track, lost)
}

/// knockback.DASH_IMMUNE_PUSH = client16402_unpushed_while_immune (client 16.402, parity r78, live13: 16 of 16 pushback hits
/// inside a guard's immunity left its steps unchanged): a Log rolled back into the charging guard (cast 2500 ahead of it at its age 10) leaves its track as
/// without the Log; under pushed it moves it (the vacuity check). Its hp is unchanged under both (the shipped immunity).
/// Plant: immune_dash_pushed.
#[test]
fn under_client16402_unpushed_while_immune_a_log_does_not_push_his_charging_guard() {
    let (bare, _) = guard_track(DashImmunePush::Client16402UnpushedWhileImmune, None);
    assert_eq!(bare.len(), 30, "the scene drifted: the guard's track");
    let (logged, lost) = guard_track(DashImmunePush::Client16402UnpushedWhileImmune, Some(10));
    assert!(!lost, "precondition: the Log landed inside the guard's immunity");
    assert_eq!(logged, bare, "client16402_unpushed_while_immune: the Log moved the charging guard");
    let (old, old_lost) = guard_track(DashImmunePush::Pushed, Some(10));
    assert!(!old_lost, "precondition (pushed): the Log landed inside the guard's immunity");
    assert_ne!(old, guard_track(DashImmunePush::Pushed, None).0, "pushed (the vacuity check): the Log did not move the guard");
}
