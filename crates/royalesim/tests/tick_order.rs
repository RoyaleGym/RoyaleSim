//! THE TICK ORDER (calibration match.TICK_ORDER = client16402; lib.rs TICK_PHASES):
//! the order MEASURED on the live 16.402 captures -- every attack update before any
//! unit moves, the move updates one after the other in CREATION order, the per-unit
//! character update (deploy countdown) after the move pass -- and the three
//! transition rules the captures show:
//!
//!   1 -> 2  a unit entering its target's range never steps on the transition tick
//!           (819 ticks: 621 stood still, 198 were only pushed);
//!   2 -> 1  a unit whose target is gone walks the same tick (412 / 413);
//!   4 -> 1  a unit whose deploy time ends stands still that tick (398 / 445) and
//!           takes its first full step the next -- spawn + DeployTime / TICK_MS, where
//!           movement.DEPLOY_TIMING measured it.
//!
//! Plus the creation-order pass (a reused slot does not jump the queue) and the
//! dying-unit visibility (movement.DYING_UNIT_VISIBILITY): a troop the buffered
//! damage kills this tick is seen by the movers before it and not by the movers
//! after it (99 : 5 versus 35 : 0 decidable live cases).
//!
//! Every number is read from cards.json / calibration.json. The legacy order is
//! run through the same key (`legacy_move_before_attack`) where a test shows the
//! rule discriminates.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant
//! cargo test --test tick_order`):
//!   phase_order            the legacy list forced under the shipped key
//!       -> entering_range_does_not_step_on_the_transition_tick,
//!          a_unit_finishing_its_deploy_stands_still_that_tick_and_steps_on_spawn_plus_deploy_time,
//!          the_phase_trace_is_the_orders_own_list
//!   slot_order_move_pass   the pass back to (spawn tick, slot)
//!       -> the_move_pass_runs_in_creation_order_not_slot_order
mod common;

use royalesim::entity::AttackPhase;
use royalesim::fixed::{milli, Vec2};
use royalesim::state::{BattleConfig, BattleState, Calib, DyingUnitVisibility, PathSearch, TickOrder};
use royalesim::{EntityId, Team};
use common::*;

fn calib() -> Calib {
    Calib::shipped()
}

fn dt() -> i32 {
    calib().tick_ms
}

fn bare(cfg: BattleConfig) -> BattleState {
    BattleState::new(7, cfg)
}

fn with_calib(f: impl FnOnce(&mut Calib)) -> BattleConfig {
    let mut cfg = config();
    f(&mut cfg.calib);
    cfg
}

fn legacy_config() -> BattleConfig {
    with_calib(|c| c.tick_order = TickOrder::LegacyMoveBeforeAttack)
}

fn assert_shipped_arms() {
    let c = calib();
    assert_eq!(c.tick_order, TickOrder::Client16402, "the shipped order this file pins");
    assert_eq!(c.path_search, PathSearch::Client16402, "the sequential move pass this file pins");
    assert_eq!(c.dying_unit_visibility, DyingUnitVisibility::CreationOrderBeforeVictim, "the shipped visibility this file pins");
}

/// Per post-tick frame: (position, attack phase) of `id`, for `n` ticks.
fn track(s: &mut BattleState, id: EntityId, n: u32) -> Vec<(Vec2, AttackPhase)> {
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        s.tick();
        let e = s.entity(id).expect("the tracked unit is alive");
        out.push((e.pos, e.attack_phase));
    }
    out
}

// ---------------------------------------------------------------------------
// 1 -> 2: entering range

/// A Blue Knight on Red's half walking +y at the Red left princess tower: the
/// frame it first winds up, and the frames around it.
fn knight_into_tower_range(cfg: BattleConfig) -> (Vec<(Vec2, AttackPhase)>, usize) {
    let mut s = bare(cfg);
    let a = s.arena().clone();
    let tower = a.princess_tower_pos(Team::Red, royalesim::arena::Lane::Left);
    let start = Vec2::new(tower.x, tower.y - milli(5500));
    assert!(a.is_passable_ground(start));
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", start, None).unwrap();
    let frames = track(&mut s, knight, 200);
    let k = frames.iter().position(|(_, ph)| *ph != AttackPhase::Idle).expect("the Knight never wound up on the tower");
    assert!(k >= 3, "vacuous: the Knight was in range from the start (k = {k})");
    assert!(frames[k - 1].0 != frames[k - 2].0, "vacuous: the Knight was not walking the tick before the transition");
    (frames, k)
}

#[test]
fn entering_range_does_not_step_on_the_transition_tick() {
    // Plant: phase_order. Under the measured order the attack update sees the
    // start-of-tick position: the tick the Knight winds up it does not step (621 of
    // the 819 live transition ticks stood still, the rest were only pushed). Under the
    // legacy order it steps into range and winds up in one tick, one frame earlier.
    assert_shipped_arms();
    let (frames, k) = knight_into_tower_range(config());
    assert_eq!(frames[k].1, AttackPhase::Windup);
    assert_eq!(frames[k].0, frames[k - 1].0, "the Knight stepped on the tick it started winding up (frame {k})");
    // and the whole windup is spent standing (a Knight in Windup is held by the pass)
    let load = card_stat(&bare(config()), "Knight").load_time_ms;
    let windup_ticks = ((load + dt() - 1) / dt()) as usize;
    for j in k..(k + windup_ticks - 1).min(frames.len() - 1) {
        assert_eq!(frames[j].0, frames[k].0, "the Knight moved during its windup (frame {j})");
    }
    let (legacy, kl) = knight_into_tower_range(legacy_config());
    assert_eq!(kl + 1, k, "the legacy order winds up one frame earlier (frame {kl} vs {k})");
    assert_ne!(legacy[kl].0, legacy[kl - 1].0, "the legacy order steps into range and winds up on one tick");
    assert_eq!(legacy[kl].0, frames[k].0, "both orders wind up at the same spot; only the tick differs");
}

// ---------------------------------------------------------------------------
// 2 -> 1: the target dies

#[test]
fn a_knight_whose_skeleton_target_dies_walks_the_tick_after_it_is_gone() {
    // The Knight's own hit kills the Skeleton (buffered in Attack, applied in
    // Resolve, despawned in Reap): the Knight stands on the hit tick (still
    // attacking a live target when Path looks) and on the tick the Skeleton is gone
    // Target drops it, Attack runs on in Cooldown and Path walks it -- the 2 -> 1
    // transition and the walk in one tick (412 / 413). Same under both orders.
    for cfg in [config(), legacy_config()] {
        let order = cfg.calib.tick_order;
        let mut s = bare(cfg);
        let knight_card = card_stat(&s, "Knight").clone();
        let skel_card = card_stat(&s, "Skeleton").clone();
        let k_at = t(900, 900);
        let s_at = Vec2::new(k_at.x, k_at.y - knight_card.range - skel_card.collision_radius);
        let knight = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).unwrap();
        let skel = s.scenario_spawn_now(Team::Blue, "Skeleton", s_at, Some(1)).unwrap();
        assert!(knight_card.damage >= s.entity(skel).unwrap().hp, "data: one Knight hit kills a Skeleton");
        // per post-tick frame: the Knight's (pos, phase) and whether the Skeleton lives;
        // two more frames after the one it is first gone from
        let mut frames: Vec<(Vec2, AttackPhase, bool)> = Vec::new();
        while frames.len() < 60 && !matches!(frames.get(frames.len().wrapping_sub(3)), Some((_, _, false))) {
            s.tick();
            let e = s.entity(knight).unwrap();
            frames.push((e.pos, e.attack_phase, s.entity(skel).is_some()));
        }
        let gone = frames.iter().position(|f| !f.2).expect("the Skeleton never died");
        assert!(gone >= 2 && gone + 2 < frames.len(), "{order:?}: death at frame {gone} of {}", frames.len());
        assert_eq!(frames[gone].1, AttackPhase::Cooldown, "{order:?}: the hit that killed it landed on that tick");
        assert!(frames[..=gone].iter().all(|f| f.0 == frames[0].0), "{order:?}: the Knight moved while it had a live target in range");
        assert_ne!(frames[gone + 1].0, frames[gone].0, "{order:?}: the Knight did not walk on the tick after its target was gone");
        assert!(frames[gone + 1].0.y < frames[gone].0.y, "{order:?}: a Red Knight walks -y toward Blue");
    }
}

// ---------------------------------------------------------------------------
// 4 -> 1: the deploy countdown

/// Deploy a Knight by command and record, per post-tick frame from the first it
/// exists in: (tick_count, deploy_ms, pos). Returns (S, D, frames) with S the
/// first frame and D = DeployTime / TICK_MS.
fn deployed_knight(cfg: BattleConfig) -> (u32, u32, Vec<(u32, i32, Vec2)>) {
    let mut s = bare(cfg);
    let d = (card_stat(&s, "Knight").deploy_time_ms / dt()) as u32;
    let at = t(900, 900);
    s.spawn_unit(Team::Blue, "Knight", at, None).unwrap();
    let mut frames = Vec::new();
    let mut id = None;
    for _ in 0..(d + 5) {
        s.tick();
        if id.is_none() {
            id = find_live(&s, Team::Blue, "Knight").first().map(|e| e.id);
        }
        if let Some(id) = id {
            let e = s.entity(id).unwrap();
            frames.push((s.tick_count(), e.deploy_ms, e.pos));
        }
    }
    (frames[0].0, d, frames)
}

#[test]
fn a_unit_finishing_its_deploy_stands_still_that_tick_and_steps_on_spawn_plus_deploy_time() {
    // Plant: phase_order. Both orders take the first full step on frame S + D
    // (movement.DEPLOY_TIMING, spawn-anchored); the measured order shows deploy_ms = 0
    // one frame EARLIER than that step (the 4 -> 1 flip before the first displacement:
    // the countdown runs after the move pass), the legacy order on the same frame.
    assert_shipped_arms();
    let (s0, d, frames) = deployed_knight(config());
    assert!(d >= 2, "data: the Knight's DeployTime is {d} ticks");
    let first_step = frames.iter().position(|f| f.2 != frames[0].2).expect("the Knight never moved");
    assert_eq!(frames[first_step].0, s0 + d, "the first step is on frame spawn + DeployTime / TICK_MS");
    let flip = frames.iter().position(|f| f.1 == 0).expect("deploy_ms never reached 0");
    assert_eq!(frames[flip].0, s0 + d - 1, "the countdown ends one frame before the first step (after the move pass)");
    assert_eq!(frames[flip].2, frames[0].2, "the Knight stood still on the frame its deploy ended");
    assert_eq!(frames[0].1, card_stat(&bare(config()), "Knight").deploy_time_ms - dt(), "the spawn tick's own countdown already ran");
    // the first step is a full-length one: the Knight's per-tick speed (less the
    // per-axis truncation of the step law, under two native units), heading +y
    let step = frames[first_step].2.sub(frames[first_step - 1].2);
    let speed = card_stat(&bare(config()), "Knight").move_speed() * calib().speed_to_subtiles_per_tick;
    let len = royalesim::fixed::isqrt(step.len2()) as i32;
    assert!(step.y > 0 && len <= speed && len >= speed - 2 * royalesim::fixed::SUBTILE_PER_MILLITILE, "first step {step:?} (len {len}) vs speed {speed}");

    let (s1, d1, legacy) = deployed_knight(legacy_config());
    assert_eq!(d1, d);
    let first_step = legacy.iter().position(|f| f.2 != legacy[0].2).expect("the Knight never moved");
    assert_eq!(legacy[first_step].0, s1 + d, "the legacy order steps on the same frame");
    let flip = legacy.iter().position(|f| f.1 == 0).unwrap();
    assert_eq!(legacy[flip].0, s1 + d, "the legacy order flips and steps on one frame");
    assert_eq!(legacy[0].1, card_stat(&bare(config()), "Knight").deploy_time_ms, "the legacy order counted nothing on the spawn tick");
}

// ---------------------------------------------------------------------------
// the phase trace

#[test]
fn the_phase_trace_is_the_orders_own_list() {
    // Plant: phase_order (the trace shows the legacy list under the shipped key).
    for cfg in [config(), legacy_config()] {
        let want = expected_phases(&cfg.calib);
        let mut s = bare(cfg);
        s.set_phase_trace(true);
        s.tick();
        assert_eq!(s.phase_trace().unwrap(), want.as_slice());
    }
    assert_ne!(expected_phases(&config().calib), expected_phases(&legacy_config().calib));
}

// ---------------------------------------------------------------------------
// creation order

/// Two overlapping Blue Skeletons X then Y walking at the Red tower, after `free`
/// slots below them were freed in that order (X reuses the last freed one). The
/// positions of (X, Y) after 12 ticks.
fn skeleton_pair_after_freeing(free: usize) -> (Vec2, Vec2) {
    let mut s = bare(config());
    let r = card_stat(&s, "Skeleton").collision_radius;
    let back = t(900, 300);
    let mut fillers = Vec::new();
    for k in 0..free {
        fillers.push(s.scenario_spawn_now(Team::Blue, "Knight", Vec2::new(back.x + (k as i32) * 2 * r, back.y), None).unwrap());
    }
    for f in &fillers {
        assert!(s.debug_set_hp(*f, 0));
        s.tick();
        assert!(s.entity(*f).is_none(), "the filler died");
    }
    for _ in 0..(2 - free) {
        s.tick(); // the same tick count either way
    }
    let x0 = t(900, 900);
    let x = s.scenario_spawn_now(Team::Blue, "Skeleton", Vec2::new(x0.x - r * 6 / 10, x0.y), None).unwrap();
    let y = s.scenario_spawn_now(Team::Blue, "Skeleton", Vec2::new(x0.x + r * 6 / 10, x0.y + r * 3 / 10), None).unwrap();
    if free == 2 {
        assert!(x.index > y.index, "vacuous: X did not take the later-freed higher slot ({:?} vs {:?})", x, y);
    } else {
        assert!(x.index < y.index);
    }
    for _ in 0..12 {
        s.tick();
    }
    (s.entity(x).unwrap().pos, s.entity(y).unwrap().pos)
}

#[test]
fn the_move_pass_runs_in_creation_order_not_slot_order() {
    // Plant: slot_order_move_pass. X is created before Y in both runs; in one of
    // them X reuses a freed slot ABOVE Y's. The sequential pass (unit i sees units
    // before it already moved) must give the same positions either way.
    assert_shipped_arms();
    let (xa, ya) = skeleton_pair_after_freeing(0);
    let (xb, yb) = skeleton_pair_after_freeing(2);
    assert_ne!(xa, ya, "vacuous: the pair did not separate");
    assert_eq!(xa, xb, "X's walk depends on which slot it got");
    assert_eq!(ya, yb, "Y's walk depends on which slot X got");
}

#[test]
fn a_reloaded_battle_keeps_the_creation_order() {
    // creation_seq is state: a snapshot taken after a slot reuse resumes the same
    // pass order (the hash sequence matches for 30 ticks).
    let mut s = bare(config());
    let r = card_stat(&s, "Skeleton").collision_radius;
    let fillers: Vec<EntityId> = (0..2).map(|k| s.scenario_spawn_now(Team::Blue, "Knight", Vec2::new(t(900, 300).x + k * 2 * r, t(900, 300).y), None).unwrap()).collect();
    for f in fillers {
        assert!(s.debug_set_hp(f, 0));
        s.tick();
    }
    let x0 = t(900, 900);
    let x = s.scenario_spawn_now(Team::Blue, "Skeleton", Vec2::new(x0.x - r * 6 / 10, x0.y), None).unwrap();
    let y = s.scenario_spawn_now(Team::Blue, "Skeleton", Vec2::new(x0.x + r * 6 / 10, x0.y + r * 3 / 10), None).unwrap();
    assert!(x.index > y.index, "vacuous: no slot reuse");
    s.tick();
    let blob = s.save();
    let mut l = BattleState::load(&blob).expect("format 12 loads");
    assert_eq!(l.state_hash(), s.state_hash());
    for k in 0..30 {
        s.tick();
        l.tick();
        assert_eq!(s.state_hash(), l.state_hash(), "diverged {k} ticks after the reload");
    }
}

// ---------------------------------------------------------------------------
// dying-unit visibility

/// Three Blue Skeletons S1, S2, S3 (created in that order) held every tick at
/// spots a Knight's range away from a Red Knight, S2 dead ahead of it and S1 / S3
/// overlapping S2 from either side; the Knight's first hit lands on S2 at tick T.
/// Returns the positions of (S1, S3) after tick T, and whether S2 was gone by
/// then, with S2's hp forced to `s2_hp` before every tick (a Knight's hit kills
/// a Skeleton at any level, so the surviving run needs a padded one).
fn crowd_around_a_landing_hit(cfg: BattleConfig, s2_hp: i32) -> ((Vec2, Vec2), bool, u32) {
    let mut s = bare(cfg);
    let knight_card = card_stat(&s, "Knight").clone();
    let r = card_stat(&s, "Skeleton").collision_radius;
    let k_at = t(900, 900);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).unwrap();
    let ahead = knight_card.range + r; // edge of the Knight's range
    let spots = |k: Vec2| [Vec2::new(k.x - r * 12 / 10, k.y - ahead), Vec2::new(k.x, k.y - ahead), Vec2::new(k.x + r * 12 / 10, k.y - ahead)];
    let sp = spots(k_at);
    let s1 = s.scenario_spawn_now(Team::Blue, "Skeleton", sp[0], None).unwrap();
    let s2 = s.scenario_spawn_now(Team::Blue, "Skeleton", sp[1], None).unwrap();
    let s3 = s.scenario_spawn_now(Team::Blue, "Skeleton", sp[2], None).unwrap();
    // overlaps: S1-S2 and S2-S3 by 0.8 r, S1-S3 none (2.4 r apart)
    assert!(sp[0].dist2(sp[1]) < (2 * r as i64) * (2 * r as i64) && sp[0].dist2(sp[2]) > (2 * r as i64) * (2 * r as i64));
    let hold = |s: &mut BattleState| {
        let k = s.entity(knight).unwrap().pos;
        let sp = spots(k);
        for (id, p) in [(s1, sp[0]), (s2, sp[1]), (s3, sp[2])] {
            assert!(s.debug_set_pos(id, p));
        }
    };
    // T: the tick the Knight's windup on S2 completes
    let mut t_fire = None;
    for k in 0..40u32 {
        hold(&mut s);
        assert!(s.debug_set_hp(s2, s2_hp)); // nothing but the kill reads it
        let before = s.entity(knight).unwrap();
        if k > 0 {
            assert_eq!(before.target, Some(s2), "the Knight's target is S2 (tick {k})");
        }
        let was_windup = before.attack_phase == AttackPhase::Windup;
        s.tick();
        let after = s.entity(knight).unwrap().attack_phase;
        if was_windup && after == AttackPhase::Cooldown {
            t_fire = Some(k);
            break;
        }
    }
    let t_fire = t_fire.expect("the Knight never fired");
    let gone = s.entity(s2).is_none();
    ((s.entity(s1).unwrap().pos, s.entity(s3).unwrap().pos), gone, t_fire)
}

#[test]
fn a_unit_dying_this_tick_is_seen_by_earlier_movers_and_not_by_later_ones() {
    // movement.DYING_UNIT_VISIBILITY. Two runs identical up to the hit tick except
    // S2's hp: at 1 the Knight's hit dooms it. S1 (before S2 in creation order) is
    // pushed by S2 in both runs; S3 (after it) is pushed by S2 only when it lives.
    // Under `whole_tick` S3 is pushed in both.
    assert_shipped_arms();
    let sturdy = 100 * card_stat(&bare(config()), "Knight").damage;
    let ((s1_live, s3_live), gone_live, t_live) = crowd_around_a_landing_hit(config(), sturdy);
    let ((s1_dead, s3_dead), gone_dead, t_dead) = crowd_around_a_landing_hit(config(), 1);
    assert!(!gone_live && gone_dead, "vacuous: S2 must survive the hit when padded and die at 1 hp");
    assert_eq!(t_live, t_dead, "the two runs fire on the same tick");
    assert_eq!(s1_live, s1_dead, "S1, before the victim in creation order, still saw it");
    assert_ne!(s3_live, s3_dead, "S3, after the victim in creation order, was still pushed by it");
    let whole = with_calib(|c| c.dying_unit_visibility = DyingUnitVisibility::WholeTick);
    let ((w1_live, w3_live), _, _) = crowd_around_a_landing_hit(whole.clone(), sturdy);
    let ((w1_dead, w3_dead), gone, _) = crowd_around_a_landing_hit(whole, 1);
    assert!(gone);
    assert_eq!(w1_live, w1_dead);
    assert_eq!(w3_live, w3_dead, "under whole_tick every mover sees the dying unit");
    assert_eq!(s3_live, w3_live, "the live run is the same under both candidates");
}

#[test]
fn every_candidate_runs_the_scripted_battle_deterministically_and_the_orders_differ() {
    let scripted = |f: fn(&mut Calib)| {
        let mut cfg = scripted_config();
        f(&mut cfg.calib);
        cfg
    };
    let shipped = scripted(|_| {});
    let legacy = scripted(|c| c.tick_order = TickOrder::LegacyMoveBeforeAttack);
    let whole = scripted(|c| c.dying_unit_visibility = DyingUnitVisibility::WholeTick);
    for cfg in [shipped.clone(), legacy.clone(), whole.clone()] {
        let a = run_scripted_with(cfg.clone(), 0xC1A5, true, None);
        let b = run_scripted_with(cfg, 0xC1A5, false, None);
        assert!(a.hashes.len() > 1000);
        assert_eq!(a.hashes, b.hashes);
        assert!(a.final_state.is_done());
    }
    // the two orders, and the two visibilities, are different battles
    let a = run_scripted_with(shipped, 0xC1A5, false, None);
    let b = run_scripted_with(legacy, 0xC1A5, false, None);
    let c = run_scripted_with(whole, 0xC1A5, false, None);
    assert_ne!(a.hashes.last(), b.hashes.last(), "the legacy order ran the same battle as the measured one");
    assert_ne!(a.hashes.last(), c.hashes.last(), "whole_tick ran the same battle as creation_order_before_victim");
}
