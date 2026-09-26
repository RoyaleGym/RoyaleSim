//! combat.DASH_ATTACK = client_dash, read off the engine: a unit with a dash block (card.rs `DashDef`) stands, then
//! dashes into its target (state.rs `phase_path16402`), discards damage while it dashes (combat.rs `resolve`), and
//! restarts its attack cycle after (state.rs `end_dashes`).
//!
//! THE LAW, measured on client 15.535.29 (the Bandit against a Knight, a Giant and a princess tower; the Mega Knight
//! against a Giant):
//!   - a walking unit stands from the first tick whose start-of-tick centre distance to its target is at most
//!     DashMaxRange + the target's radius, and enters the dash DashCooldown / 50 - 1 ticks later;
//!   - the Bandit is still on that entry tick, then moves in two half-steps of JumpSpeed / 2 a tick, stopping after the
//!     first whose edge gap to the target is within its Range, and deals DashDamage (389 at level 11) on that tick;
//!   - the Mega Knight moves JumpSpeed a tick to its goal cell's centre and deals DashDamage DashConstantTime / 50
//!     ticks after the entry;
//!   - the Bandit discards damage from its entry to the arrival + 1 (DashImmuneToDamageTime 100);
//!   - after the dash the attack cycle restarts from its load time: the first melee lands HitSpeed / 50 - 1 ticks
//!     after the first tick out;
//!   - a target whose edge gap was under DashMinRange when first seen is walked into.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the Bandit's and the Mega Knight's blocks, gives no other loaded card a dash, and loads a
//!      DashDef for every dash block that carries its speed (no block is dropped quietly);
//!   2. the shipped value is client_dash, and under none the Bandit walks in;
//!   3. the Bandit's stand, entry, half-steps, stop and blow against a Knight at a princess tower, from six starting
//!      points (the half-step stop is a property of every dash, not of one scene's geometry);
//!   4. the Bandit dashing at a princess tower takes none of the tower's arrows from its entry to the arrival + 1, and
//!      takes them before and after;
//!   5. the Bandit's first melee after the dash lands on the arrival + 19 (HitSpeed 1000), for its ordinary damage;
//!   6. a Bandit whose target is first seen inside DashMinRange walks in and hits for its ordinary damage;
//!   7. the Mega Knight's jump: it moves on the trigger + 17, about 250 a tick, rests on the goal cell's centre, and
//!      the Giant loses DashDamage on the entry + 16 and nothing from it before;
//!   8. a Mini P.E.K.K.A (no dash block) walks in under both values.
//!   9. a Mega Knight put down inside its trigger (the sweep's scene: 4,805 from a Knight, an edge of 3,555 over
//!      DashMinRange 3,500) stands from its first tick and jumps on its eighteenth (first active + 18).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test dash_attack`):
//!   * `dash_unread` -- the loader drops the dash block: (1), (3), (4), (5) and (7) go red.
//!   * `dash_trigger_centre` -- the trigger ignores the target's radius: (3) goes red on the stand.
//!   * `dash_first_tick_moves` -- the Bandit moves on its entry tick: (3) goes red on the stand's length.
//!   * `dash_whole_steps` -- one whole step and one Range test a tick: (3) goes red on the half-step stop.
//!   * `dash_not_immune` -- a dashing unit takes every hit: (4) goes red.
//!   * `dash_keeps_the_cycle` -- the load timer is not reset after the dash: (5) goes red.
//!   * `dash_first_sight_walks` -- a unit put down inside its trigger walks its first tick: (9) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DashAttack};
use royalesim::{EntityId, Team};

fn with(arm: DashAttack) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.dash_attack = arm;
    cfg
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Native distance between two subtile positions, truncated.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// One tick of a scene, read after it.
#[derive(Clone, Copy, Debug)]
struct Row {
    /// the tick's index (0 = the first tick run)
    t: u32,
    /// the attacker's move this tick, native
    step: i64,
    /// the centre distance at the start of the tick, native
    start: i64,
    /// the edge gap at the end of the tick, native
    edge: i64,
    /// hp the target lost this tick
    target_loss: i32,
    /// hp the attacker lost this tick
    own_loss: i32,
    /// the attacker's position after the tick
    pos: Vec2,
    /// the target's position at the start of the tick
    target_start: Vec2,
}

/// A `me_team` `attacker` at `me` and a `target` card of the other team at `them`, run for up to `ticks` ticks (or until
/// either dies). The target is the attacker's by construction of each scene; `radii` is both radii, native.
fn scene(arm: DashAttack, attacker: &str, me: (i32, i32), target: &str, them: (i32, i32), ticks: u32) -> (BattleState, EntityId, EntityId, Vec<Row>) {
    let mut s = BattleState::new(0, with(arm));
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, target, at(them), None), (Team::Blue, attacker, at(me), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (k, a) = (ids[0], ids[1]);
    let radii = (s.entity(a).unwrap().radius + s.entity(k).unwrap().radius) as i64 / K as i64;
    let mut rows = Vec::new();
    for t in 0..ticks {
        let (Some(av), Some(kv)) = (s.entity(a), s.entity(k)) else { break };
        let (a0, k0, ahp, khp) = (av.pos, kv.pos, av.hp, kv.hp);
        s.tick();
        let (Some(av), Some(kv)) = (s.entity(a), s.entity(k)) else { break };
        rows.push(Row {
            t,
            step: dist(a0, av.pos),
            start: dist(a0, k0),
            edge: dist(av.pos, kv.pos) - radii,
            target_loss: khp - kv.hp,
            own_loss: ahp - av.hp,
            pos: av.pos,
            target_start: k0,
        });
    }
    (s, a, k, rows)
}

/// The level the scenes play at, and a card's stat scaled to it.
fn scaled(s: &BattleState, card: &str, base: i32) -> i32 {
    let idx = s.cards().index(card).unwrap_or_else(|| panic!("{card} is not simulable"));
    s.cards().scaled(idx, s.config().card_level[0], base).expect("a valid level")
}

// ---- the Bandit against a Knight at the blue right princess tower (the 15.535.29 scenario's geometry)

/// A red Knight that walks to the blue right princess tower and stands hitting it, and a blue Bandit walking after
/// it from beyond the trigger distance.
const KNIGHT_AT: (i32, i32) = (14231, 9500);
const BANDIT_AT: (i32, i32) = (8200, 6200);
/// DashMaxRange 6000 + the Knight's radius 500, and the stand DashCooldown 800 / 50.
const BANDIT_TRIGGER: i64 = 6500;
const BANDIT_STAND: u32 = 16;
/// The Bandit's Range and JumpSpeed.
const BANDIT_RANGE: i64 = 750;
const HALF: i64 = 250;
/// A crown tower's arrow at the scenes' level.
const ARROW: i32 = 109;

fn trigger(rows: &[Row], at_most: i64) -> usize {
    rows.iter().position(|r| r.start <= at_most).expect("the scene drifted: the attacker never came within its trigger distance")
}

/// The dash's rows from the first move to the stop: every move after the stand until the first tick ending within
/// Range.
fn dash_rows(rows: &[Row], from: usize) -> &[Row] {
    let end = (from..rows.len()).find(|&j| rows[j].edge <= BANDIT_RANGE).expect("the dash never reached Range");
    &rows[from..=end]
}

fn check_bandit_dash(start: (i32, i32)) {
    let (s, _, _, rows) = scene(DashAttack::ClientDash, "Assassin", start, "Knight", KNIGHT_AT, 80);
    assert!(rows[0].start > BANDIT_TRIGGER + 200, "the scene drifted: the Bandit started only {} away", rows[0].start);
    let nearest = rows.iter().map(|r| r.start).min().unwrap_or(i64::MAX);
    assert!(nearest <= BANDIT_TRIGGER, "the scene drifted from {start:?}: the Bandit came no nearer than {nearest} in {} ticks", rows.len());
    let d = trigger(&rows, BANDIT_TRIGGER);
    // THE STAND: no move from the trigger tick for DashCooldown / 50 ticks (the entry tick is still).
    let moved: Vec<(u32, i64)> = rows[d..d + BANDIT_STAND as usize].iter().filter(|r| r.step > 0).map(|r| (r.t, r.step)).collect();
    assert!(moved.is_empty(), "from {start:?}: the Bandit moved while it should stand, from the trigger tick {}: {moved:?}", rows[d].t);
    assert!(rows[d - 1].step > 0, "the scene drifted: the Bandit was not walking before the trigger");
    // THE DASH: whole moves of two half-steps, the last a half or a whole one.
    let dash = dash_rows(&rows, d + BANDIT_STAND as usize);
    let steps: Vec<i64> = dash.iter().map(|r| r.step).collect();
    assert!(dash.len() >= 2, "from {start:?}: a one-tick dash says nothing about the steps: {steps:?}");
    assert!(steps[..steps.len() - 1].iter().all(|&x| (2 * HALF - 6..=2 * HALF).contains(&x)), "from {start:?}: dash steps {steps:?}");
    let last = *dash.last().unwrap();
    assert!((HALF - 10..=HALF).contains(&last.step) || (2 * HALF - 6..=2 * HALF).contains(&last.step), "from {start:?}: the last move {} is neither a half-step nor a whole one: {steps:?}", last.step);
    // THE HALF-STEP STOP: the dash ends on the FIRST half-step within Range, so the point half a step back along the
    // last move is out of Range (on the target's start-of-tick position).
    let prev = dash[dash.len() - 2].pos;
    let (mx, my) = (last.pos.x / K - prev.x / K, last.pos.y / K - prev.y / K);
    let n = isqrt((mx * mx + my * my) as i64).max(1);
    let back = Vec2::new((last.pos.x / K - (mx as i64 * HALF / n) as i32) * K, (last.pos.y / K - (my as i64 * HALF / n) as i32) * K);
    let back_edge = dist(back, last.target_start) - (600 + 500);
    if last.step > HALF + 10 {
        assert!(back_edge > BANDIT_RANGE, "from {start:?}: the dash ended on a whole move whose first half was already within Range (edge {back_edge}): {steps:?}");
    }
    assert!(last.edge <= BANDIT_RANGE && dash[dash.len() - 2].edge > BANDIT_RANGE, "from {start:?}: the dash did not stop on the first move within Range");
    // THE BLOW: DashDamage at the level, on the stop tick, and no damage of the Bandit's before it.
    let blow = scaled(&s, "Assassin", 152);
    let early: Vec<(u32, i32)> = rows.iter().filter(|r| r.t < last.t && r.target_loss != 0 && r.target_loss != ARROW).map(|r| (r.t, r.target_loss)).collect();
    assert!(early.is_empty(), "from {start:?}: the Knight lost hp to the Bandit before the dash's last move: {early:?}");
    assert_eq!(last.target_loss, blow, "from {start:?}: the Knight lost {} on the dash's last move, not DashDamage {blow}", last.target_loss);
}

/// Plants: dash_unread, dash_trigger_centre, dash_first_tick_moves, dash_whole_steps.
#[test]
fn a_bandit_stands_then_dashes_into_its_target_stopping_on_the_first_half_step_within_range() {
    let s = BattleState::new(0, with(DashAttack::ClientDash));
    if s.config().card_level[0] == 11 {
        assert_eq!(scaled(&s, "Assassin", 152), 389, "DashDamage 152 at level 11 is the measured 389");
    }
    // Six starts on the scenario's bearing to the Knight, 23 nearer each: each dash ends on its own half-step phase.
    for k in 0..6 {
        check_bandit_dash((BANDIT_AT.0 + 20 * k, BANDIT_AT.1 + 11 * k));
    }
}

/// Plant: dash_unread.
#[test]
fn the_loader_reads_the_dash_blocks_and_drops_none() {
    let db = cards();
    let get = |n: &str| db.get(db.index(n).unwrap_or_else(|| panic!("{n} is not simulable")));
    let b = get("Assassin").dash.expect("the Bandit's dash block is read");
    assert_eq!((b.damage, b.min_range / K, b.max_range / K, b.cooldown_ms, b.speed), (152, 3500, 6000, 800, 500));
    assert_eq!((b.radius, b.pushback_raw, b.immune_ms, b.constant_time_ms, b.landing_time_ms), (None, None, Some(100), None, None));
    let m = get("MegaKnight").dash.expect("the Mega Knight's dash block is read");
    assert_eq!((m.damage, m.min_range / K, m.max_range / K, m.cooldown_ms, m.speed), (210, 3500, 5000, 900, 250));
    assert_eq!((m.radius.map(|r| r / K), m.pushback_raw, m.immune_ms, m.constant_time_ms, m.landing_time_ms), (Some(2200), Some(1000), None, Some(800), Some(300)));
    // No other loaded card dashes: the Golden Knight's and the event Hog Rider's dash start from an Ability and ride
    // in `triggered_dash`, which nothing reads.
    let dashers: Vec<&str> = db.cards.iter().filter(|c| c.dash.is_some()).map(|c| c.name.as_str()).collect();
    assert_eq!(dashers, ["Assassin", "MegaKnight"], "the loaded cards with a dash");
    // No block dropped quietly: every cards.json dash block with a speed key is a loaded DashDef (the 2018 file's
    // blocks, which have none, are the only ones the loader skips).
    let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).unwrap()).unwrap();
    for c in doc["cards"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let has_speed = c["dash"].as_object().is_some_and(|d| d.contains_key("speed"));
        if let (true, Some(i)) = (has_speed, db.index(name)) {
            assert!(db.get(i).dash.is_some(), "{name} carries a dash block with a speed and loads no dash");
        }
    }
}

#[test]
fn the_shipped_value_is_client_dash_and_under_none_the_bandit_walks_in() {
    let s = BattleState::new(0, config());
    assert_eq!(s.config().calib.dash_attack, DashAttack::ClientDash, "shipped combat.DASH_ATTACK");
    let (_, _, _, rows) = scene(DashAttack::None, "Assassin", BANDIT_AT, "Knight", KNIGHT_AT, 80);
    let d = trigger(&rows, BANDIT_TRIGGER);
    assert!(rows[d].step > 0, "none: the Bandit stood at the trigger distance");
    let long: Vec<(u32, i64)> = rows.iter().filter(|r| r.step > 150).map(|r| (r.t, r.step)).collect();
    assert!(long.is_empty(), "none: the Bandit took a step longer than a walk: {long:?}");
}

/// Plant: dash_keeps_the_cycle.
#[test]
fn the_bandits_first_melee_after_the_dash_lands_on_the_arrival_plus_19() {
    let (s, _, _, rows) = scene(DashAttack::ClientDash, "Assassin", BANDIT_AT, "Knight", KNIGHT_AT, 80);
    let d = trigger(&rows, BANDIT_TRIGGER);
    let arrival = dash_rows(&rows, d + BANDIT_STAND as usize).last().unwrap().t;
    let melee = scaled(&s, "Assassin", 76);
    let hits: Vec<(u32, i32)> = rows.iter().filter(|r| r.t > arrival && r.target_loss != 0 && r.target_loss != ARROW).map(|r| (r.t, r.target_loss)).collect();
    assert!(!hits.is_empty(), "the scene drifted: the Bandit never hit after its dash");
    // HitSpeed 1000: the progress reads 100 on the arrival + 1 and gains 50 a tick, so it reaches 1000 on the + 19.
    assert_eq!(hits[0], (arrival + 19, melee), "the first melee after the arrival {arrival}");
}

/// Plant: dash_not_immune.
#[test]
fn the_bandit_takes_no_tower_arrow_from_its_entry_to_the_arrival_plus_one() {
    // A blue Bandit walking at the red left princess tower (radius 1000, trigger 7000), which shoots it from the
    // walk on. The tower's first arrow lands before the stand ends; its period (16 ticks) puts one in the dash.
    let (_, _, _, rows) = scene_at_tower((3500, 17200), 60);
    let d = trigger(&rows, 7000);
    let entry = rows[d].t + BANDIT_STAND - 1;
    let arrival = dash_rows(&rows, d + BANDIT_STAND as usize).last().unwrap().t;
    let before: Vec<u32> = rows.iter().filter(|r| r.t < entry && r.own_loss > 0).map(|r| r.t).collect();
    let during: Vec<(u32, i32)> = rows.iter().filter(|r| (entry..=arrival + 1).contains(&r.t) && r.own_loss != 0).map(|r| (r.t, r.own_loss)).collect();
    let after: Vec<u32> = rows.iter().filter(|r| r.t > arrival + 1 && r.own_loss > 0).map(|r| r.t).collect();
    assert!(!before.is_empty() && !after.is_empty(), "the scene drifted: the tower did not shoot the Bandit before and after its dash ({before:?}, {after:?})");
    assert!(during.is_empty(), "the Bandit lost hp between its entry {entry} and its arrival {arrival} + 1: {during:?}");
}

/// A blue Bandit at `me` whose target is the red left princess tower.
fn scene_at_tower(me: (i32, i32), ticks: u32) -> (BattleState, EntityId, EntityId, Vec<Row>) {
    let mut s = BattleState::new(0, with(DashAttack::ClientDash));
    let a = s.scenario_spawn_now(Team::Blue, "Assassin", at(me), None).expect("spawn");
    let tower = s
        .tower_ids(Team::Red)
        .into_iter()
        .flatten()
        .find(|t| s.entity(*t).is_some_and(|v| v.pos.x < 9000 * K && v.kind == royalesim::entity::EntityKind::PrincessTower))
        .expect("the red left princess tower");
    let radii = (s.entity(a).unwrap().radius + s.entity(tower).unwrap().radius) as i64 / K as i64;
    let mut rows = Vec::new();
    for t in 0..ticks {
        let (Some(av), Some(kv)) = (s.entity(a), s.entity(tower)) else { break };
        let (a0, k0, ahp, khp) = (av.pos, kv.pos, av.hp, kv.hp);
        s.tick();
        let (Some(av), Some(kv)) = (s.entity(a), s.entity(tower)) else { break };
        rows.push(Row { t, step: dist(a0, av.pos), start: dist(a0, k0), edge: dist(av.pos, kv.pos) - radii, target_loss: khp - kv.hp, own_loss: ahp - av.hp, pos: av.pos, target_start: k0 });
    }
    (s, a, tower, rows)
}

#[test]
fn a_target_first_seen_inside_dash_min_range_is_walked_into() {
    // The Knight at the tower as above, the Bandit put down 4000 from it: an edge gap of 2900, under DashMinRange 3500.
    let me = (KNIGHT_AT.0 - 3200, KNIGHT_AT.1 - 2400);
    let (s, _, _, rows) = scene(DashAttack::ClientDash, "Assassin", me, "Knight", KNIGHT_AT, 60);
    assert!(rows[0].start < 3500 + 1100, "the scene drifted: the Bandit started {} away", rows[0].start);
    let long: Vec<(u32, i64)> = rows.iter().filter(|r| r.step > 150).map(|r| (r.t, r.step)).collect();
    assert!(long.is_empty(), "the Bandit dashed at a target first seen inside DashMinRange: {long:?}");
    let first = rows.iter().find(|r| r.target_loss != 0 && r.target_loss != ARROW).expect("the scene drifted: the Bandit never hit");
    assert_eq!(first.target_loss, scaled(&s, "Assassin", 76), "the Bandit's first hit was not its ordinary one");
}

// ---- the Mega Knight's jump against a Giant hitting the blue right princess tower

const MK_AT: (i32, i32) = (9500, 12500);
const GIANT_AT: (i32, i32) = (14731, 9439);
/// DashMaxRange 5000 + the Giant's 750; the entry DashCooldown 900 / 50 - 1 after the trigger; the blow
/// DashConstantTime 800 / 50 after the entry.
const MK_TRIGGER: i64 = 5750;
const MK_ENTRY: u32 = 17;
const MK_BLOW: u32 = 16;

/// Plant: dash_unread.
#[test]
fn a_mega_knight_jumps_to_its_goal_cell_and_lands_its_blow_on_the_constant_time() {
    let (s, _, _, rows) = scene(DashAttack::ClientDash, "MegaKnight", MK_AT, "Giant", GIANT_AT, 60);
    let d = trigger(&rows, MK_TRIGGER);
    let onset = rows.iter().position(|r| r.step > 200).expect("the Mega Knight never jumped: no move longer than 200");
    assert_eq!(rows[onset].t, rows[d].t + MK_ENTRY, "the jump began on {}, not on the trigger {} + {MK_ENTRY}", rows[onset].t, rows[d].t);
    // The goal: the 500-cell holding the point (both radii, 1500) short of the Giant, from the onset's positions.
    let (mk, g) = (rows[onset - 1].pos, rows[onset].target_start);
    let (dx, dy) = ((mk.x / K - g.x / K) as i64, (mk.y / K - g.y / K) as i64);
    let n = isqrt(dx * dx + dy * dy);
    let (px, py) = (g.x as i64 / K as i64 + dx * 1500 / n, g.y as i64 / K as i64 + dy * 1500 / n);
    let cell = ((px.div_euclid(500) * 500 + 250) as i32, (py.div_euclid(500) * 500 + 250) as i32);
    let moves: Vec<(u32, i64)> = rows[onset..].iter().take(20).filter(|r| r.step > 0).map(|r| (r.t, r.step)).collect();
    assert!(moves[..moves.len() - 1].iter().all(|&(_, x)| (240..=250).contains(&x)), "the jump's moves: {moves:?}");
    let rest = rows.iter().find(|r| r.t == moves.last().unwrap().0).unwrap().pos;
    assert_eq!((rest.x / K, rest.y / K), cell, "the jump came to rest off the goal cell's centre");
    let blow = scaled(&s, "MegaKnight", 210);
    let hit = rows[onset + MK_BLOW as usize];
    assert!(hit.target_loss == blow || hit.target_loss - blow == ARROW, "the Giant lost {} on the onset + {MK_BLOW}, not DashDamage {blow}", hit.target_loss);
    let early: Vec<(u32, i32)> = rows[onset..onset + MK_BLOW as usize].iter().filter(|r| r.target_loss != 0 && r.target_loss != ARROW).map(|r| (r.t, r.target_loss)).collect();
    assert!(early.is_empty(), "the Giant lost hp before the blow: {early:?}");
}

#[test]
fn a_melee_unit_without_a_dash_walks_into_range_under_both_values() {
    for arm in [DashAttack::None, DashAttack::ClientDash] {
        let (_, _, _, rows) = scene(arm, "MiniPekka", (10500, 7000), "Knight", KNIGHT_AT, 80);
        let first = rows.iter().position(|r| r.target_loss != 0 && r.target_loss != ARROW).expect("the scene drifted: the Mini P.E.K.K.A never hit the Knight");
        let long: Vec<(u32, i64)> = rows.iter().filter(|r| r.step > 150).map(|r| (r.t, r.step)).collect();
        assert!(long.is_empty(), "{arm:?}: the Mini P.E.K.K.A took steps longer than a walk: {long:?}");
        let (mut run, mut longest) = (0, 0);
        for r in &rows[..first] {
            run = if r.step == 0 { run + 1 } else { 0 };
            longest = longest.max(run);
        }
        assert!(longest < BANDIT_STAND, "{arm:?}: the Mini P.E.K.K.A stood {longest} ticks before its first hit");
    }
}

/// Plant: dash_first_sight_walks.
#[test]
fn a_mega_knight_put_down_inside_its_trigger_stands_from_its_first_tick_and_jumps_on_the_eighteenth() {
    let (_, _, _, rows) = scene(DashAttack::ClientDash, "MegaKnight", (9500, 11500), "Knight", (13126, 14653), 40);
    assert!(rows[0].start < MK_TRIGGER && rows[0].start > 3500 + 750 + 500, "the scene drifted: the Mega Knight started {} away", rows[0].start);
    let first = rows.iter().position(|r| r.step > 0).expect("the Mega Knight never moved");
    // first sight on tick 0 (it stands), the trigger on tick 1, the entry DashCooldown 900 / 50 - 1 = 17 later
    assert_eq!(rows[first].t, 1 + MK_ENTRY, "the Mega Knight first moved on {}", rows[first].t);
    assert!(rows[first].step > 200, "its first move is not the jump: {}", rows[first].step);
}
