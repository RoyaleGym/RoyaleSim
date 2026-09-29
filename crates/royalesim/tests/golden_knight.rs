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
//!      comes back combat.DASH_CHAIN_COOLDOWN after the chain's end, and never under "none".
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test golden_knight`):
//!   chain_whole_step    one range test a tick, after the whole JumpSpeed: (1) goes red.
//!   chain_blow_unscaled the blow at DashDamage's level-1 figure: (1) goes red.
//!   chain_retakes_hit   the chain takes a target it hit again: (3) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DeployError};
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
    cfg
}

/// The scene: the Golden Knight deployed, Red's `reds` put down deploying, the press issued. Returns the battle, his id
/// and theirs.
fn scene(cooldown: Option<i32>, reds: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<EntityId>) {
    let mut s = BattleState::try_new(0, cfg_with(cooldown)).expect("the decks load");
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
