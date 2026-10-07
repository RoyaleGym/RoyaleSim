//! THE GOLDEN KNIGHT'S BUTTON: the dash chain (card.rs `AbilityEffect::DashChain`, state.rs `chain_pass`) and the
//! champion button (state.rs `ability_buttons`, `check_ability_button`, combat.DASH_CHAIN_COOLDOWN).
//!
//! The law, measured on client 15.535.29 (parity, round 10 item 53; Oracle's GK battery and sp-champ-GoldenKnight-s0):
//!   - the press row: he takes his walk step, and dashes from the next tick at the CLOSEST enemy ground character
//!     within 5,500;
//!   - a dash tick moves JumpSpeed 400 in sub-steps of at most 250, each followed by the range test (Range 1,200 +
//!     both radii): the first within lands DashDamage (335 at level 11) on that tick and ends the motion (a Giant hit
//!     after 250: the tick moves 250);
//!   - after a blow he stands one tick, takes the closest ground character NOT YET HIT within 5,500 on the next and
//!     dashes from the one after; with none the chain ends that tick, and he attacks from the next.
//!
//! The button: available and charged from his first frame, the press takes the one charge, and a press while it is
//! out is refused (AbilityNotReady); whether and when it comes back is combat.DASH_CHAIN_COOLDOWN's open question.
//!
//! The scenes: a level-11 Golden Knight on Blue at (3500, 9000), deployed, and Red's units DEPLOYING (they stand for
//! their deploy, so the distances are the scene's), Blue's princess towers down so no tower reaches them. Pinned:
//!   1. the press row walks, the dash moves 400 a tick, and the hit tick 250, with the Giant's 335 on it;
//!   2. one tick standing, the chain's end, and his attack from the tick after;
//!   3. the chain takes the next closest ground character not yet hit, and never the Giant again;
//!   4. the button: charged from his first frame, the press takes it, a press while it is out is AbilityNotReady, it
//!      comes back combat.DASH_CHAIN_COOLDOWN after the chain's end, and never under "none";
//!   5. combat.DASH_CHAIN_END = client15535_no_target_two_ticks: at the chain's end (H + 2) he holds no target, on
//!      H + 3 still none, and on H + 4 he takes one by the ordinary rule; keep_last_target keeps the Knight;
//!   6. combat.DASH_CHAIN_PENDING = client15535_run_to_current_target: alone on the field, pressed with nothing in
//!      the charge's circle, he runs 2 x his walk toward his target (a princess tower) and dashes from the tick after
//!      the one whose run brings its centre within DashRange + its radius; wait_for_ground_character never dashes;
//!   7. the same arm re-decides his target while he runs: a Knight put down near him mid-run becomes his target, and
//!      his dash goes at it;
//!   8. and the trigger reads the target he held at the end of the last tick: on the tick the Knight, in reach, is
//!      first his target he runs (115-125), and he dashes from the next (client 15.535.29, item 311).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test golden_knight`):
//!   chain_whole_step    one range test a tick, after the whole JumpSpeed: (1) goes red.
//!   chain_blow_unscaled the blow at DashDamage's level-1 figure: (1) goes red.
//!   chain_retakes_hit   the chain takes a target it hit again: (3) goes red.
//!   chain_end_keeps_target the measured end keeps the last dash target: (5) goes red.
//!   pending_run_walks   the waiting press walks at his own speed: (6) goes red.
//!   pending_run_on_press_tick the pending run starts on the chain's first tick:
//!                       `the_pending_run_starts_the_tick_after_the_press_s_first` goes red.
//!   pending_trigger_without_radius the trigger reads DashRange alone: (6) goes red.
//!   pending_target_frozen the waiting press keeps its target from the press: (7) goes red.
//!   pending_trigger_reads_fresh_target the trigger reads the target this tick gave him: (8) goes red.
//!   chain_end_keeps_cycle the measured cycle keeps the one the chain left: under_the_measured_cycle_... goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DashChainAttackCycle, DashChainEnd, DashChainPending, DeployError, PostKillWait};
use royalesim::{EntityId, Team};

const GK_AT: (i32, i32) = (3500, 9000);
const GIANT_AT: (i32, i32) = (3500, 13500);
const DECK: [&str; 8] = ["GoldenKnight", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn dist(a: Vec2, b: Vec2) -> i64 {
    royalesim::fixed::isqrt(a.dist2(b)) / K as i64
}

fn cfg_with(cooldown: Option<i32>) -> BattleConfig {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dash_chain_cooldown_ms = cooldown;
    // The arms (1) to (4) were written on, before combat.DASH_CHAIN_END and DASH_CHAIN_PENDING flipped: the tests of
    // those keys name theirs.
    cfg.calib.dash_chain_end = DashChainEnd::KeepLastTarget;
    cfg.calib.dash_chain_pending = DashChainPending::WaitForGroundCharacter;
    cfg.calib.dash_chain_attack_cycle = royalesim::state::DashChainAttackCycle::Kept;
    cfg
}

/// The scene: the Golden Knight deployed, Red's `reds` put down deploying, the press issued. Returns the battle, his id
/// and theirs.
fn scene(cooldown: Option<i32>, reds: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<EntityId>) {
    scene_in(cfg_with(cooldown), reds)
}

/// `scene` under a config of the caller's.
fn scene_in(cfg: BattleConfig, reds: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<EntityId>) {
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    let gk = s.scenario_spawn_now(Team::Blue, "GoldenKnight", n(GK_AT), None).expect("the Golden Knight");
    let mut ids = Vec::new();
    for (card, at) in reds {
        s.spawn_unit(Team::Red, card, n(*at), None).expect("a Red unit");
        s.tick();
        let id = s.entities().filter(|e| e.team == Team::Red && e.card == *card).map(|e| e.id).last().expect("it is on the board");
        ids.push(id);
    }
    s.press_ability_button(Team::Blue, 0).expect("the press is taken");
    (s, gk, ids)
}

/// Per tick after the press: his point, his target, his attack phase, and each Red unit's hp.
struct Row {
    at: Vec2,
    target: Option<EntityId>,
    phase: AttackPhase,
    hp: Vec<i32>,
}

fn run(s: &mut BattleState, gk: EntityId, reds: &[EntityId], ticks: u32) -> Vec<Row> {
    (0..ticks)
        .map(|_| {
            s.tick();
            let g = s.entity(gk).expect("he lives");
            Row { at: g.pos, target: g.target, phase: g.attack_phase, hp: reds.iter().map(|r| s.entity(*r).map_or(0, |e| e.hp)).collect() }
        })
        .collect()
}

/// The moves of each tick, native, the first against `start`.
fn moves(start: Vec2, rows: &[Row]) -> Vec<i64> {
    let mut prev = start;
    rows.iter()
        .map(|r| {
            let d = dist(prev, r.at);
            prev = r.at;
            d
        })
        .collect()
}

#[test]
fn the_press_row_walks_and_the_dash_moves_400_a_tick_to_a_250_hit_with_the_blow() {
    let (mut s, gk, reds) = scene(Some(11_000), &[("Giant", GIANT_AT)]);
    let start = s.entity(gk).unwrap().pos;
    let giant_hp = s.entity(reds[0]).unwrap().hp;
    assert!(s.entity(reds[0]).unwrap().deploy_ms > 200, "the scene drifted: the Giant is not deploying");
    let rows = run(&mut s, gk, &reds, 12);
    let m = moves(start, &rows);
    // The press row: his walk step, toward the Giant.
    assert!((50..=65).contains(&m[0]), "the press row: a walk step, not {} (moves {m:?})", m[0]);
    let hit = rows.iter().position(|r| r.hp[0] < giant_hp).unwrap_or_else(|| panic!("the Giant was never hit: moves {m:?}"));
    assert!(m[1..hit].iter().all(|x| (397..=400).contains(x)), "the dash ticks before the hit move 400: {m:?}");
    assert_eq!(m[hit], 250, "the hit tick: the first 250 sub-step brought him within range, and the tick moves 250: {m:?}");
    assert_eq!(giant_hp - rows[hit].hp[0], 335, "the blow: DashDamage 131 at level 11");
    assert!(dist(rows[hit].at, n(GIANT_AT)) <= 1200 + 800 + 750, "he stands within Range + both radii after the hit");
    assert_eq!(rows[hit].target, Some(reds[0]), "his target through the dash is the Giant");
}

#[test]
fn one_tick_standing_then_the_chain_ends_and_he_attacks_from_the_next() {
    let (mut s, gk, reds) = scene(Some(11_000), &[("Giant", GIANT_AT)]);
    let giant_hp = s.entity(reds[0]).unwrap().hp;
    let rows = run(&mut s, gk, &reds, 14);
    let h = rows.iter().position(|r| r.hp[0] < giant_hp).expect("the Giant was hit");
    assert_eq!(rows[h + 1].at, rows[h].at, "H + 1: he stands");
    assert_eq!(rows[h + 2].at, rows[h].at, "H + 2: the chain ends; he is in reach and does not walk");
    assert_eq!(rows[h + 1].phase, AttackPhase::Idle, "H + 1: no attack while the chain runs");
    assert_eq!(rows[h + 2].phase, AttackPhase::Idle, "H + 2: the chain's end, no attack yet");
    assert_ne!(rows[h + 3].phase, AttackPhase::Idle, "H + 3: he attacks the Giant");
    assert_eq!(rows[h + 3].target, Some(reds[0]), "H + 3: his target is the Giant");
}

#[test]
fn the_chain_takes_the_closest_target_not_yet_hit_and_never_the_same_one_twice() {
    // The Knight lies beyond the Giant from his start (5,852, outside the charge's 5,500) and within 5,500 of where the
    // first blow leaves him.
    let (mut s, gk, reds) = scene(Some(11_000), &[("Giant", GIANT_AT), ("Knight", (5500, 14500))]);
    let (giant_hp, knight_hp) = (s.entity(reds[0]).unwrap().hp, s.entity(reds[1]).unwrap().hp);
    let rows = run(&mut s, gk, &reds, 24);
    let h1 = rows.iter().position(|r| r.hp[0] < giant_hp).expect("the Giant was hit first");
    assert!(rows[..h1].iter().all(|r| r.hp[1] == knight_hp), "the Knight was hit before the Giant");
    assert_eq!(rows[h1 + 1].target, Some(reds[0]), "H + 1: still on the Giant");
    assert_eq!(rows[h1 + 2].target, Some(reds[1]), "H + 2: the next closest target not yet hit, the Knight");
    assert_eq!(rows[h1 + 2].at, rows[h1].at, "H + 2: he stands while he takes it");
    let h2 = rows.iter().position(|r| r.hp[1] < knight_hp).expect("the Knight was hit");
    assert!(h2 > h1 + 2, "the second dash starts on H + 3");
    assert_eq!(knight_hp - rows[h2].hp[1], 335, "the second blow");
    assert_eq!(rows[h2].hp[0], rows[h1].hp[0], "the Giant was hit again");
    assert_eq!(rows[h2 + 2].target, Some(reds[1]), "with none left the chain ends on the Knight");
}

#[test]
fn the_button_is_charged_from_his_first_frame_and_a_press_takes_the_charge() {
    let mut s = BattleState::try_new(0, cfg_with(Some(1_000))).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "GoldenKnight", n(GK_AT)).expect("the play");
    s.tick();
    let b = s.ability_buttons(Team::Blue);
    assert_eq!(b.len(), 1, "one button: the champion's");
    assert!(b[0].champion && b[0].available && !b[0].spent && b[0].cost == 1, "charged from his first frame: {:?}", b[0]);
    s.press_ability_button(Team::Blue, 0).expect("a press on his first frame is taken");
    let b = s.ability_buttons(Team::Blue)[0];
    assert!(!b.available && !b.spent, "the charge is out, and a champion's is never spent: {b:?}");
    assert_eq!(s.check_ability_button(Team::Blue, 0), Err(DeployError::AbilityNotReady), "a press while the charge is out");
}

/// Ticks from the press until the button reads available again, under `cooldown`, or None within `max`.
fn recharge(cooldown: Option<i32>, max: u32) -> (Option<u32>, u32) {
    let (mut s, _, _) = scene(cooldown, &[("Giant", GIANT_AT)]);
    let mut out_since = None;
    for k in 1..=max {
        s.tick();
        let b = s.ability_buttons(Team::Blue)[0];
        if b.cooldown_ticks > 0 && out_since.is_none() {
            out_since = Some(k);
        }
        if b.available {
            return (Some(k), out_since.unwrap_or(0));
        }
    }
    (None, out_since.unwrap_or(0))
}

#[test]
fn the_charge_comes_back_the_cooldown_after_the_chain_ends_and_never_under_none() {
    let (back, ended) = recharge(Some(1_000), 200);
    let back = back.expect("the charge came back under a 1000 ms cooldown");
    assert!(ended > 0, "the button never read a cooldown");
    assert!((19..=21).contains(&(back - ended)), "back about 20 ticks (1000 ms) after the chain's end: ended {ended}, back {back}");
    assert_eq!(recharge(None, 400).0, None, "under none the charge never comes back");
}

#[test]
fn under_the_measured_end_he_holds_no_target_for_two_ticks_then_takes_one_by_the_ordinary_rule() {
    let reds = [("Giant", GIANT_AT), ("Knight", (5500, 14500))];
    let mut cfg = cfg_with(Some(11_000));
    cfg.calib.dash_chain_end = DashChainEnd::ClientNoTargetTwoTicks;
    let (mut s, gk, ids) = scene_in(cfg, &reds);
    let knight_hp = s.entity(ids[1]).unwrap().hp;
    let rows = run(&mut s, gk, &ids, 30);
    let h2 = rows.iter().position(|r| r.hp[1] < knight_hp).expect("the Knight was hit");
    assert_eq!(rows[h2 + 1].target, Some(ids[1]), "H + 1: the chain still runs on the Knight");
    assert_eq!((rows[h2 + 2].target, rows[h2 + 3].target), (None, None), "H + 2 and H + 3: no target");
    assert!(rows[h2 + 4].target.is_some(), "H + 4: a target by the ordinary rule");
    assert_eq!(rows[h2 + 3].phase, AttackPhase::Idle, "H + 3: no attack without a target");
    // keep_last_target: the chain ends on the Knight and he keeps it.
    let (mut s, gk, ids) = scene(Some(11_000), &reds);
    let rows = run(&mut s, gk, &ids, 30);
    let h2 = rows.iter().position(|r| r.hp[1] < knight_hp).expect("the Knight was hit");
    assert_eq!((rows[h2 + 2].target, rows[h2 + 3].target), (Some(ids[1]), Some(ids[1])), "keep_last_target keeps the Knight");
}

/// A lone Golden Knight on Red's side of the river, pressed with no Red unit on the field, under `pending`:
/// per tick after the press, his move and his centre distance to his target (both native), and his target.
fn lone_press(pending: DashChainPending) -> (Vec<(i64, i64)>, i64) {
    let mut cfg = cfg_with(Some(11_000));
    cfg.calib.dash_chain_pending = pending;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    // Red's side, on the princess tower's line, 8,000 short of it: a straight run, no bridge.
    let gk = s.scenario_spawn_now(Team::Blue, "GoldenKnight", n((3500, 17500)), None).expect("the Golden Knight");
    s.tick();
    s.press_ability_button(Team::Blue, 0).expect("the press is taken with nothing in reach");
    let tower_r = {
        let t = s.entity(gk).unwrap().target.expect("he targets a tower");
        (s.entity(t).unwrap().radius / K) as i64
    };
    let mut prev = s.entity(gk).unwrap().pos;
    let rows = (0..60)
        .map(|_| {
            s.tick();
            let g = s.entity(gk).expect("he lives");
            let t = g.target.and_then(|t| s.entity(t)).expect("a target");
            let row = (dist(prev, g.pos), dist(g.pos, t.pos));
            prev = g.pos;
            row
        })
        .collect();
    (rows, tower_r)
}

#[test]
fn a_press_with_nothing_in_reach_runs_him_at_twice_his_walk_and_dashes_at_his_target_in_reach() {
    let (rows, tower_r) = lone_press(DashChainPending::ClientRunToCurrentTarget);
    let reach = 5500 + tower_r;
    let d = rows.iter().position(|r| r.0 >= 390).unwrap_or_else(|| panic!("he never dashed: {rows:?}"));
    assert!(d >= 3, "the scene drifted: he dashed within two ticks of the press ({rows:?})");
    assert!(rows[d - 1].1 <= reach && rows[d - 2].1 > reach, "the dash starts the tick after his run brings the target within {reach}: {rows:?}");
    assert!(rows[1..d].iter().all(|r| (115..=125).contains(&r.0)), "he runs 2 x his walk until the dash: {rows:?}");
    let (rows, _) = lone_press(DashChainPending::WaitForGroundCharacter);
    assert!(rows.iter().all(|r| r.0 < 390), "wait_for_ground_character dashed at a tower: {rows:?}");
}

/// Measured on client 15.535.29 (sp-champ-GK-empty-s0, pressed on P = t161, nothing in reach): his step on P + 1 is his
/// walk, (+42, +42), and from P + 2 twice it, (+84, +84).
#[test]
fn the_pending_run_starts_the_tick_after_the_press_s_first() {
    let (rows, _) = lone_press(DashChainPending::ClientRunToCurrentTarget);
    assert!((55..=65).contains(&rows[0].0), "P + 1: his own walk: {rows:?}");
    assert!((115..=125).contains(&rows[1].0), "P + 2: twice his walk: {rows:?}");
}

#[test]
fn a_waiting_press_re_decides_its_target_each_tick_and_dashes_at_a_knight_put_down_mid_run() {
    let mut cfg = cfg_with(Some(11_000));
    cfg.calib.dash_chain_pending = DashChainPending::ClientRunToCurrentTarget;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let gk = s.scenario_spawn_now(Team::Blue, "GoldenKnight", n((3500, 17500)), None).expect("the Golden Knight");
    s.tick();
    s.press_ability_button(Team::Blue, 0).expect("the press is taken with nothing in reach");
    for _ in 0..3 {
        s.tick();
    }
    let tower = s.entity(gk).unwrap().target.expect("he runs at a tower");
    // Nearer than the tower, inside DashRange + its radius (6,000), outside his melee reach (Range 1,200 + both
    // radii): he takes it, and his first dash step is a whole 400.
    s.spawn_unit(Team::Red, "Knight", n((5500, 21500)), None).expect("a Red Knight");
    let mut knight = None;
    let mut dashed_at_knight = false;
    let mut seen = Vec::new();
    for _ in 0..12 {
        let before = s.entity(gk).unwrap().pos;
        s.tick();
        knight = knight.or_else(|| s.entities().find(|e| e.team == Team::Red && e.card == "Knight").map(|e| e.id));
        let g = s.entity(gk).unwrap();
        let step = dist(before, g.pos);
        seen.push((g.target == knight && knight.is_some(), g.target == Some(tower), step));
        if knight.is_some() && g.target == knight && step >= 390 {
            dashed_at_knight = true;
            break;
        }
    }
    assert!(knight.is_some(), "the scene drifted: no Knight on the board");
    assert_ne!(Some(tower), knight, "the scene drifted");
    assert!(dashed_at_knight, "his target was not re-decided to the Knight, or he never dashed at it: (on the Knight, on the tower, step) {seen:?}");
}

/// (8) Plant: pending_trigger_reads_fresh_target. Measured on client 15.535.29 (sp-champ-GK-empty-knight40-s0): a
/// Knight placed on t201, his target on t202 and inside 5,500 + its 500 on t201's positions; t202 a run (+118), and
/// the dash from t203.
#[test]
fn a_waiting_press_dashes_the_tick_after_the_knight_is_first_his_target() {
    let mut cfg = cfg_with(Some(11_000));
    cfg.calib.dash_chain_pending = DashChainPending::ClientRunToCurrentTarget;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let gk = s.scenario_spawn_now(Team::Blue, "GoldenKnight", n((3500, 17500)), None).expect("the Golden Knight");
    s.tick();
    s.press_ability_button(Team::Blue, 0).expect("the press is taken with nothing in reach");
    for _ in 0..3 {
        s.tick();
    }
    let tower = s.entity(gk).unwrap().target.expect("he runs at a tower");
    s.spawn_unit(Team::Red, "Knight", n((5500, 21500)), None).expect("a Red Knight");
    let mut steps = Vec::new();
    let mut first = None;
    for k in 0..12 {
        let before = s.entity(gk).unwrap().pos;
        s.tick();
        let knight = s.entities().find(|e| e.team == Team::Red && e.card == "Knight").map(|e| (e.id, e.pos));
        let g = s.entity(gk).unwrap();
        steps.push(dist(before, g.pos));
        if let (None, Some((kid, kpos))) = (first, knight) {
            if g.target == Some(kid) {
                assert!(dist(before, kpos) < 6000, "the scene drifted: the Knight out of reach on the tick before ({steps:?})");
                first = Some(k);
            }
        }
    }
    let f = first.unwrap_or_else(|| panic!("the Knight never became his target: {steps:?}"));
    assert_ne!(Some(tower), s.entity(gk).unwrap().target, "the scene drifted");
    assert!(f + 1 < steps.len(), "the scene drifted: {steps:?}");
    assert!((115..=125).contains(&steps[f]), "the tick the Knight is first his target he runs: {f} {steps:?}");
    assert!(steps[f + 1] >= 390, "and he dashes from the next: {f} {steps:?}");
}

#[test]
fn the_shipped_values_are_the_measured_arms_since_the_round_10_ship() {
    let c = royalesim::state::Calib::shipped();
    assert_eq!(c.dash_chain_end, DashChainEnd::ClientNoTargetTwoTicks);
    assert_eq!(c.dash_chain_pending, DashChainPending::ClientRunToCurrentTarget);
    assert_eq!(c.dash_chain_attack_cycle, DashChainAttackCycle::ClientRestartFromLoad, "since the round-12 flip");
}

#[test]
fn without_a_post_kill_wait_the_measured_end_only_drops_the_target() {
    // combat.POST_KILL_WAIT = none counts nothing, so the two-tick hold cannot run: the configuration is still accepted,
    // the chain's end drops the target (H + 2), and the next Target phase decides (H + 3).
    let reds = [("Giant", GIANT_AT), ("Knight", (5500, 14500))];
    let mut cfg = cfg_with(Some(11_000));
    cfg.calib.dash_chain_end = DashChainEnd::ClientNoTargetTwoTicks;
    cfg.calib.post_kill_wait = PostKillWait::None;
    assert!(cfg.calib.validate().is_ok(), "the measured end refused a configuration with no post-kill wait");
    let (mut s, gk, ids) = scene_in(cfg, &reds);
    let knight_hp = s.entity(ids[1]).unwrap().hp;
    let rows = run(&mut s, gk, &ids, 30);
    let h2 = rows.iter().position(|r| r.hp[1] < knight_hp).expect("the Knight was hit");
    assert_eq!(rows[h2 + 2].target, None, "H + 2: the target is dropped");
    assert!(rows[h2 + 3].target.is_some(), "H + 3: the next Target phase decides, no hold");
}

/// The Golden Knight alone on the Giant under the shipped chain end (client15535_no_target_two_ticks) and `cycle`: the
/// ticks from the dash's blow on the Giant to the next hp it loses (his first ordinary hit).
fn first_hit_after_the_chain(cycle: DashChainAttackCycle) -> usize {
    let mut cfg = cfg_with(Some(11_000));
    cfg.calib.dash_chain_end = DashChainEnd::ClientNoTargetTwoTicks;
    cfg.calib.dash_chain_attack_cycle = cycle;
    let (mut s, gk, ids) = scene_in(cfg, &[("Giant", GIANT_AT)]);
    let giant_hp = s.entity(ids[0]).unwrap().hp;
    let rows = run(&mut s, gk, &ids, 60);
    let h = rows.iter().position(|r| r.hp[0] < giant_hp).expect("the dash's blow on the Giant");
    let f = (h + 1..rows.len()).find(|&k| rows[k].hp[0] < rows[h].hp[0]).expect("an ordinary hit after the chain");
    f - h
}

#[test]
fn under_the_measured_cycle_his_first_ordinary_hit_lands_on_h_plus_19() {
    // combat.DASH_CHAIN_ATTACK_CYCLE (parity's item 62). Measured on client 15.535.29, 12 of 12 chains: the chain's end
    // restarts his attack cycle from LoadTime, as a dash's end does, and his first ordinary hit lands on H + 19, one
    // HitSpeed (900) after H + 1. Under kept the cycle runs on from the chain and the hit comes early. Plant:
    // chain_end_keeps_cycle.
    assert_eq!(first_hit_after_the_chain(DashChainAttackCycle::ClientRestartFromLoad), 19, "client15535_restart_from_load: H + 19");
    let kept = first_hit_after_the_chain(DashChainAttackCycle::Kept);
    assert!(kept < 19, "vacuous: under kept the first ordinary hit is not early (H + {kept})");
}
