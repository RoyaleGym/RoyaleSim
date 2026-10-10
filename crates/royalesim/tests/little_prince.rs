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
//! (`his_ramp_holds_through_six_moving_ticks_and_goes_on_the_seventh` red).
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
use royalesim::state::{BattleConfig, BattleState, GuardChargeStep, LoadTimerTargetLoss, RampGraceMove, RampGraceReset, RampStunRestart};
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
    scene_cfg(|c| c.calib.guard_charge_step = arm, reds)
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
    let d = |p: (i32, i32), q: (i32, i32)| (((p.0 - q.0) as i64).pow(2) + ((p.1 - q.1) as i64).pow(2)) as f64;
    let new = charge_end_at(GuardChargeStep::Client16402CellAim, (14120, 10620));
    assert!(d(new, (14250, 13750)).sqrt() < 400.0, "client16402_cell_aim: the end {new:?} not within 400 of (14250, 13750)");
    let old = charge_end_at(GuardChargeStep::Client15535SubstepsToAim, (14120, 10620));
    assert!(old != new, "the vacuity check: the 15.535.29 arm ends where the new one does ({old:?})");
}
