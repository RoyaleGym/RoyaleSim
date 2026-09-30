//! THE HERO VALKYRIE (card.rs `AbilityEffect::SpinChain`; state.rs `SpinRun`, `spin_seek`, `spin_pass`), against client
//! 15.535.29 at level 11.
//!
//! THE MEASUREMENTS (sp-form-Valkyrie-hero-s0; the press's first frame P = t197):
//!   - from P + 1 she walks 150 a tick (her 60 at the chain buff's 250 %) to the closest enemy ground troop within 5500;
//!   - her blow falls on P + 1 and every 5 ticks after, 14 in all (to P + 66): 97 (38 at level 1) on every enemy
//!     ground unit within 2500 + its radius of her;
//!   - on the tick she has her target in her attack reach (Range + both radii: 2200 from a Knight) she takes the
//!     closest one she has not reached; with none left she stands;
//!   - every hit on her from P + 1 to the last blow is cut 15 %, truncated (the Knight's 202 to 171, the Musketeer's 217
//!     to 184, a tower's 109 to 92);
//!   - after the last blow she has no target and does not attack for 8 ticks, takes one on P + 75 and hits it on P + 76
//!     (her load: progress 1450 on P + 75).
//!
//! Read off the table, not measured: with no enemy in the circle she waits, walking at the pending buff's 200 %.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_valkyrie`):
//!   - spin_never -> `her_blow_falls_fourteen_times_five_ticks_apart_from_p_plus_1` red;
//!   - spin_unguarded -> `every_hit_on_her_is_cut_15_percent_until_the_last_blow` red;
//!   - spin_retakes_reached -> `she_walks_her_chain_taking_the_next_as_she_reaches_one` red;
//!   - spin_rest_skipped -> `after_the_last_blow_she_rests_8_ticks_then_hits_from_her_load` red;
//!   - finish_column_unread -> `without_a_press_a_kill_costs_her_no_retarget_wait` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Valkyrie", "Knight", "Archer", "Giant", "Musketeer", "HogRider", "Fireball", "Zap"];

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
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

/// One frame: the hero's point, hp, target and attack progress, and each red unit's point and hp.
struct Frame {
    at: Vec2,
    hp: i32,
    target: Option<EntityId>,
    attack_ms: i32,
    reds: Vec<Option<(Vec2, i32)>>,
}

/// A scene: the hero put at `v` and held there until the press (after it too when `hold`), red `units` put at their
/// points and held at their homes (their hp topped up every tick when `full`), deployed; then the press and `frames`
/// frames, frame k being P + k + 1. `before(k, homes, at)` may move a red unit's home before frame k's tick, `at` the
/// hero's point then. Returns the red units' ids, the hero's hp drops over the 40 ticks before the press, and the frames.
fn scene(v: (i32, i32), hold: bool, full: bool, units: &[(&str, (i32, i32))], frames: usize, mut before: impl FnMut(usize, &mut Vec<Vec2>, Vec2)) -> (Vec<EntityId>, Vec<i32>, Vec<Frame>) {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Valkyrie_hero", n(v.0, v.1), None).expect("the hero");
    for (card, p) in units {
        s.spawn_unit_resolved(Team::Red, card, n(p.0, p.1), None).expect("a red unit");
    }
    s.tick();
    let hero = find_live(&s, Team::Blue, "Valkyrie_hero")[0].id;
    let ids: Vec<EntityId> = units
        .iter()
        .map(|(card, p)| s.entities().find(|e| e.team == Team::Red && e.card == *card && e.pos == n(p.0, p.1)).expect("the unit where it was put").id)
        .collect();
    let mut homes: Vec<Vec2> = units.iter().map(|(_, p)| n(p.0, p.1)).collect();
    let tops: Vec<i32> = ids.iter().map(|id| s.entity(*id).expect("the unit").max_hp).collect();
    let hold_all = |s: &mut BattleState, homes: &[Vec2], hero_too: bool| {
        if hero_too && s.entity(hero).is_some() {
            assert!(s.debug_set_pos(hero, n(v.0, v.1)));
        }
        for ((id, p), top) in ids.iter().zip(homes).zip(&tops) {
            if s.entity(*id).is_some() {
                assert!(s.debug_set_pos(*id, *p));
                if full {
                    assert!(s.debug_set_hp(*id, *top));
                }
            }
        }
    };
    let mut early = Vec::new();
    for _ in 0..40 {
        hold_all(&mut s, &homes, true);
        let hp = s.entity(hero).expect("the hero").hp;
        s.tick();
        early.push(hp - s.entity(hero).expect("the hero").hp);
    }
    assert!(s.entities().all(|e| !e.deploying), "the scene is still deploying");
    // Her last tick's step undone: she leaves her point on P + 1.
    hold_all(&mut s, &homes, true);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut out = Vec::new();
    for k in 0..frames {
        let at = s.entity(hero).expect("the hero lives").pos;
        before(k, &mut homes, at);
        hold_all(&mut s, &homes, hold);
        s.tick();
        let h = s.entity(hero).expect("the hero lives");
        let reds = ids.iter().map(|id| s.entity(*id).map(|e| (e.pos, e.hp))).collect();
        out.push(Frame { at: h.pos, hp: h.hp, target: h.target, attack_ms: h.attack_ms, reds });
    }
    (ids, early, out)
}

/// Unit 0's hp lost on each frame (frame 0 against its hp before the press's first tick is not known: 0 there).
fn drops(f: &[Frame], unit: usize) -> Vec<i32> {
    (0..f.len()).map(|k| if k == 0 { 0 } else { f[k - 1].reds[unit].map_or(0, |r| r.1) - f[k].reds[unit].map_or(0, |r| r.1) }).collect()
}

/// A standing red Giant 2000 ahead of a held hero: it targets buildings alone, so only her hits touch it. She has it in
/// her reach on P + 1 (Range 1200 + 500 + its 750), so her chain ends there and she stands spinning.
fn giant_scene(frames: usize) -> (Vec<EntityId>, Vec<Frame>) {
    let (ids, _, f) = scene((9000, 12000), true, false, &[("Giant", (9000, 14000))], frames, |_, _, _| {});
    (ids, f)
}

#[test]
fn her_blow_falls_fourteen_times_five_ticks_apart_from_p_plus_1() {
    let (_, f) = giant_scene(72);
    let hp0 = f[0].reds[0].expect("the Giant").1;
    // Frame 0 is P + 1: the first blow.
    let before_first = hp0 + 97;
    let giant: Vec<i32> = f.iter().map(|x| x.reds[0].expect("the Giant").1).collect();
    let mut want = before_first;
    for (k, hp) in giant.iter().enumerate().take(71) {
        if k % 5 == 0 && k / 5 < 14 {
            want -= 97;
        }
        assert_eq!(*hp, want, "the Giant's hp on P + {} (a blow of 97 on P + 1 + 5k, 14 of them, and nothing else)", k + 1);
    }
    assert_eq!(before_first - giant[70], 14 * 97, "14 blows");
}

#[test]
fn every_hit_on_her_is_cut_15_percent_until_the_last_blow() {
    // A red Knight 1300 ahead of the held hero, held there with its hp topped up (her blows would kill it), swinging at
    // her.
    let (_, early, f) = scene((9000, 12000), true, true, &[("Knight", (9000, 13300))], 110, |_, _, _| {});
    let whole: Vec<i32> = early.iter().copied().filter(|d| *d > 0).collect();
    assert!(!whole.is_empty() && whole.iter().all(|d| *d == 202), "the Knight's whole hit before the press: {whole:?}");
    let mut during = Vec::new();
    let mut after = Vec::new();
    for k in 1..f.len() {
        let d = f[k - 1].hp - f[k].hp;
        if d > 0 {
            // Frame k is P + k + 1; the last blow falls on P + 66.
            if k < 66 { during.push(d) } else { after.push(d) }
        }
    }
    assert!(during.len() >= 2 && during.iter().all(|d| *d == 171), "202 cut 15 %, truncated, through the spin: {during:?}");
    assert!(!after.is_empty() && after.iter().all(|d| *d == 202), "whole again after the last blow: {after:?}");
}

#[test]
fn she_walks_her_chain_taking_the_next_as_she_reaches_one() {
    // Two red Ice Golems (they target buildings alone), held: A 3000 ahead, B 3000 beside A. The hero is held until the
    // press, so she has neither in reach before it.
    let (ids, _, f) = scene((9000, 11000), false, false, &[("IceGolemite", (9000, 14000)), ("IceGolemite", (12000, 14000))], 60, |_, _, _| {});
    let (a, b) = (ids[0], ids[1]);
    let start = n(9000, 11000);
    // P + 1: her first step, 150 (her 60 at 250 %), at A.
    assert_eq!(f[0].target, Some(a), "the closest first");
    let step = f[0].at.dist(start) / K;
    assert!((148..=151).contains(&step), "150 a tick from P + 1: {step}");
    for k in 1..8 {
        let s = f[k].at.dist(f[k - 1].at) / K;
        assert!((148..=151).contains(&s), "150 a tick on P + {}: {s}", k + 1);
    }
    // She switches to B on the first frame she has A in her reach: 1200 + her 500 + the golem's radius.
    let reach = |r: i32| 1200 * K + 500 * K + r;
    let ra = 700 * K;
    let switch = f.iter().position(|x| x.target == Some(b)).expect("she takes B");
    let pa = f[switch].reds[0].expect("A").0;
    assert!(f[switch].at.dist(pa) <= reach(ra), "A in her reach when she takes B");
    assert!(f[switch - 1].at.dist(f[switch - 1].reds[0].expect("A").0) > reach(ra), "not a frame before");
    assert!(f[..switch].iter().all(|x| x.target == Some(a)), "A until then");
    // B reached: her chain ends (no target that frame), and she does not walk on at the chain's speed.
    let end = (switch..f.len()).find(|&k| f[k].target != Some(b)).expect("her chain ends");
    assert_eq!(f[end].target, None, "no target on the frame her chain ends");
    assert!(f[end].at.dist(f[end].reds[1].expect("B").0) <= reach(ra), "B in her reach then");
    assert!(f[switch..end].iter().all(|x| x.target == Some(b)), "B until then, A never again");
}

#[test]
fn after_the_last_blow_she_rests_8_ticks_then_hits_from_her_load() {
    let (_, f) = giant_scene(80);
    let d = drops(&f, 0);
    // The last blow on P + 66 (frame 65); then no target P + 66 to P + 74, none of her hits; the Giant taken on P + 75
    // at her load's progress (1450) and hit on P + 76 for her 266 (104 at level 1).
    assert_eq!(d[65], 97, "the last blow on P + 66");
    for (k, fr) in f.iter().enumerate().take(74).skip(65) {
        assert_eq!(fr.target, None, "no target on P + {}", k + 1);
    }
    assert!(d[66..75].iter().all(|x| *x == 0), "no hit on P + 67 to P + 75: {:?}", &d[66..75]);
    assert!(f[74].target.is_some(), "a target on P + 75");
    assert_eq!(f[74].attack_ms, 1450, "her swing from her load on P + 75");
    assert_eq!(d[75], 266, "her hit on P + 76");
}

#[test]
fn with_none_in_the_circle_she_waits_then_spins_from_the_tick_one_comes() {
    // On the red side, out of every blue tower's reach: the hero walks at the red princess tower; a red Giant is held
    // far behind the red line (out of her 5500 circle) until frame 10, then put 2000 ahead of her.
    let (_, _, f) = scene((3500, 19000), false, false, &[("Giant", (14000, 29000))], 20, |k, homes, at| {
        if k == 10 {
            homes[0] = at.add(n(0, 2000));
        }
    });
    // Waiting: no blow, the Giant untouched, and she walks at 120 (the pending buff's 200 %) from P + 2.
    let giant0 = f[0].reds[0].expect("the Giant").1;
    assert!(f[..10].iter().all(|x| x.reds[0].expect("the Giant").1 == giant0), "no blow while she waits");
    for k in 2..10 {
        let s = f[k].at.dist(f[k - 1].at) / K;
        assert!((118..=121).contains(&s), "120 a tick while she waits, P + {}: {s}", k + 1);
    }
    // The Giant put 2000 ahead of her before frame 10's tick: her spin begins on that tick, and its first blow with it.
    let d = drops(&f, 0);
    assert_eq!(d[10], 97, "the first blow on the tick one comes into the circle");
    assert_eq!(d[11..16].iter().sum::<i32>(), 97, "the next 5 ticks on");
}

#[test]
fn without_a_press_a_kill_costs_her_no_retarget_wait() {
    // Her row inherits the Valkyrie's OverrideAttackFinishTime (combat.POST_KILL_RETARGET_WAIT's clause (a), read off
    // the row, card.rs `CardDef::override_attack_finish`): on client 15.535.29 (sp-form-Valkyrie-hero-nopress-s0) she
    // walked on at her next target on t208, the tick after her Skeleton died, as the base Valkyrie does. A red Skeleton
    // beside her, a red Knight held out of her reach with its hp topped up; no press.
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Valkyrie_hero", n(9500, 11500), None).expect("the hero");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Valkyrie_hero")[0].id;
    for _ in 0..25 {
        assert!(s.debug_set_pos(hero, n(9500, 11500)));
        s.tick();
    }
    let skeleton = s.scenario_spawn_now(Team::Red, "Skeleton", n(9500, 12600), None).expect("a red Skeleton");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(9500, 14500), None).expect("a red Knight");
    let top = s.entity(knight).expect("the Knight").max_hp;
    let mut targets = Vec::new();
    let mut gone = None;
    for k in 0..60 {
        assert!(s.debug_set_pos(knight, n(9500, 14500)));
        assert!(s.debug_set_hp(knight, top));
        s.tick();
        if gone.is_none() && s.entity(skeleton).is_none() {
            gone = Some(k);
        }
        targets.push(s.entity(hero).expect("the hero").target);
    }
    let k = gone.expect("her blow kills the Skeleton");
    assert!(targets[k..k + 2].contains(&Some(knight)), "the Knight her target within a tick of the kill: {:?}", &targets[k..k + 6]);
    assert!(targets[k + 2..k + 6].iter().all(|t| *t == Some(knight)), "no wait with no target: {:?}", &targets[k..k + 6]);
}
