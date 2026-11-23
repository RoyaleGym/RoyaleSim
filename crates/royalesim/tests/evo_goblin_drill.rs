//! THE EVO GOBLIN DRILL (tools/extract_cards.py `drill_block`; card.rs `DrillDef`, DRILL_RISE_TICKS,
//! DRILL_SPAWNER_HOLD_TICKS; state.rs EvoBoard `drills`, `drill_pass`, the holds in `phase_status` and `spawner_pass`),
//! at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; Oracle's sp-f4-drill-s0): at 66 % its building went under for 39 ticks where it
//! stood, its hitpoints frozen through the tick it came up and draining the tick after; two Goblins a tick after it went
//! under, at x - 500 and x + 500; its regular Goblins 97 ticks apart across the hide.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_goblin_drill`):
//!   - drill_hide_never -> `at_two_thirds_its_building_goes_under_39_ticks_unhurt_and_puts_down_two_goblins` red;
//!   - drill_hide_drains -> the same red;
//!   - drill_spawner_unheld -> the same red;
//!   - drill_hide_collides -> the same red (its Goblins pushed off its footprint);
//!   - drill_hide_acquired_at_once -> `a_hides_goblins_are_targets_from_their_8th_frame` red;
//!   - evo_drill_death_ring_unlisted -> `its_buildings_death_goblins_are_laid_on_the_x_axis_ring` red;
//!   - drill_under_shot_lands -> `a_shot_in_flight_at_a_building_that_goes_under_lands_on_nothing_under_client15535_dropped` red;
//!   - drill_body_back_at_hide_time -> `its_body_returns_three_ticks_after_its_hide_time_under_client15535_hide_time_plus_3` red;
//!   - drill_rise_hidden -> `a_rising_building_is_a_target_again_under_client15535_targetable_on_rise` red;
//!   - drill_line_reads_hits -> `under_client16635_after_drain_before_hits_a_hits_crossing_goes_under_a_tick_later` red;
//!   - drill_dig_stays -> `under_client16635_keeps_tower_distance_the_building_digs_round_the_tower` red.
//!   - drill_ring_stays -> `under_client16635_princess_ring_the_building_walks_its_towers_ring` red.
//!   - drill_under_held_kept -> `under_client16635_dropped_its_holders_let_the_dug_building_go_on_the_under_tick_plus_1` red.
#![allow(unexpected_cfgs)]
// THE c4 FLIP (parity r66): this file's scenes were written on the engine before it, so they run its old arms
// (common::config_pre_flip); each flipped key's own tests read both arms.
mod common;

use common::*;
use royalesim::entity::HideState;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The building's Goblins not yet in `seen`: (their points, millitiles, whether they deploy).
fn fresh(s: &BattleState, d: EntityId, seen: &mut Vec<EntityId>) -> Vec<((i32, i32), bool)> {
    let mut out = Vec::new();
    for e in s.entities().filter(|e| e.spawned_by == Some(d)) {
        if !seen.contains(&e.id) {
            seen.push(e.id);
            out.push(((e.pos.x / K, e.pos.y / K), e.deploy_ms > 0));
        }
    }
    out
}

#[test]
fn at_two_thirds_its_building_goes_under_39_ticks_unhurt_and_puts_down_two_goblins() {
    // The form's building put down on blue's side at (9000, 10000), nothing red near. After its first regular Goblin its
    // hitpoints are set to 67 % (880 of 1313): the drain takes it to 66 % on T.
    let mut cfg: BattleConfig = config_pre_flip();
    cfg.decks = [vec!["GoblinDrill".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at = n(9000, 10000);
    let d = s.scenario_spawn_now(Team::Blue, "units.GoblinDrill_EV1", at, None).expect("the drill's building");
    let top = s.entity(d).expect("the building").max_hp;
    assert_eq!(top, 1313, "513 at level 1");
    let mut seen = Vec::new();
    let mut regular: Vec<usize> = Vec::new();
    let mut set = false;
    let (mut under, mut hp, mut goblins) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..200usize {
        s.tick();
        let e = s.entity(d).expect("the building");
        under.push(e.hide_state == HideState::Hidden);
        hp.push(e.hp);
        for (p, deploying) in fresh(&s, d, &mut seen) {
            if p.1 == 10000 {
                goblins.push((k, p, deploying));
            } else {
                regular.push(k);
            }
        }
        if !set && !regular.is_empty() {
            assert!(s.debug_set_hp(d, 880));
            set = true;
        }
    }
    let t = under.iter().position(|u| *u).expect("it went under");
    assert!(hp[t] * 100 / top <= 66 && hp[t - 1] * 100 / top > 66, "under on the tick it reached 66 %: {:?}", &hp[t - 2..=t]);
    assert!(under[t..t + 39].iter().all(|u| *u) && !under[t + 39], "under T .. T + 38: {:?}", &under[t..t + 41]);
    assert!(hp[t..=t + 39].iter().all(|h| *h == hp[t]) && hp[t + 40] < hp[t], "frozen T .. T + 39: {:?}", &hp[t..t + 42]);
    assert_eq!(goblins[..2], [(t + 1, (9500, 10000), true), (t + 1, (8500, 10000), true)], "its first hide's Goblins: {goblins:?}");
    // Its second line (33 %) went under later in the run: one Goblin, at x + 500.
    assert!(goblins.len() == 3 && goblins[2].1 == (9500, 10000) && goblins[2].2, "its second hide's Goblin: {goblins:?}");
    let across: Vec<usize> = regular.windows(2).filter(|w| w[0] < t && w[1] > t).map(|w| w[1] - w[0]).collect();
    assert_eq!(across, [97], "its regular Goblins across the hide: {regular:?}");
}


/// targeting.SPAWNED_UNIT_ACQUIRE_DELAY: a hide's Goblins are an action's spawn, targets for enemies from their 8th frame
/// (client 15.535.29: none first targeted before F + 7; sp-form-GoblinDrill-evo-s0 t1146, three enemies passed over one on
/// its 2nd frame); the regular Goblins, a Spawn* spawner's, from their first. Plant: drill_hide_acquired_at_once.
#[test]
fn a_hides_goblins_are_targets_from_their_8th_frame() {
    let mut cfg: BattleConfig = config_pre_flip();
    cfg.decks = [vec!["GoblinDrill".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let d = s.scenario_spawn_now(Team::Blue, "units.GoblinDrill_EV1", n(9000, 10000), None).expect("the drill's building");
    let mut seen: Vec<EntityId> = Vec::new();
    let (mut hide, mut regular) = (Vec::new(), Vec::new());
    let mut set = false;
    for _ in 0..200 {
        s.tick();
        let born = s.tick_count() - 1;
        let fresh: Vec<(EntityId, i32, u32)> =
            s.entities().filter(|e| e.spawned_by == Some(d) && !seen.contains(&e.id)).map(|e| (e.id, e.pos.y / K, e.acquirable_from)).collect();
        for (id, y, from) in fresh {
            seen.push(id);
            if y == 10000 {
                hide.push((born, from));
            } else {
                regular.push((born, from));
            }
        }
        if !set && !regular.is_empty() {
            assert!(s.debug_set_hp(d, 880));
            set = true;
        }
    }
    assert!(hide.len() >= 2, "the scene drifted: no hide put its Goblins down ({hide:?})");
    assert!(hide.iter().all(|(born, from)| *from == born + 7), "a hide's Goblins: an enemy may target them from their 8th frame: {hide:?}");
    // NOT VACUOUS: the regular Goblins are targets at once.
    assert!(!regular.is_empty() && regular.iter().all(|(_, from)| *from == 0), "the regular Goblins carry a delay: {regular:?}");
}

/// spawner.DEATH_SPAWN_RING (item 284: GoblinDrill_EV1 on the list): the Evo building's death pair is laid as the base
/// building's, at (x - 500, y) and (x + 500, y), the first-created Goblin on -x (client 15.535.29: 3 of 3 Evo deaths; the
/// lower ordinal on -x in all 16 Drill deaths); DEATH_SPAWN_LAYOUT, the engine's before, laid them at (x, y -+ 500).
/// Plant: evo_drill_death_ring_unlisted.
#[test]
fn its_buildings_death_goblins_are_laid_on_the_x_axis_ring() {
    let mut cfg: BattleConfig = config_pre_flip();
    cfg.decks = [vec!["GoblinDrill".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let d = s.scenario_spawn_now(Team::Blue, "units.GoblinDrill_EV1", n(9000, 10000), None).expect("the drill's building");
    for _ in 0..3 {
        s.tick();
    }
    let before: Vec<EntityId> = s.entities().map(|e| e.id).collect();
    assert!(s.debug_set_hp(d, 0));
    s.tick();
    s.tick();
    assert!(s.entity(d).is_none(), "the scene drifted: the building still stands");
    // The troops new since the kill, within 1,000 of its point, in creation order.
    let mut new: Vec<(u32, (i32, i32))> = s
        .entities()
        .filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::Troop && !before.contains(&e.id))
        .map(|e| (e.team_seq, (e.pos.x / K, e.pos.y / K)))
        .filter(|(_, p)| (p.0 - 9000).abs() <= 1000 && (p.1 - 10000).abs() <= 1000)
        .collect();
    new.sort_unstable();
    let points: Vec<(i32, i32)> = new.iter().map(|(_, p)| *p).collect();
    // Within the native unit the ring's trigonometry rounds (the base building's pair reads 8499 for 8500 too).
    let want = [(8500, 10000), (9500, 10000)];
    assert!(points.len() == 2 && points.iter().zip(want).all(|(p, w)| (p.0 - w.0).abs() <= 1 && (p.1 - w.1).abs() <= 1), "its death pair, in creation order: {new:?}");
}

/// A level-11 battle with the Evo Goblin Drill's form, `set` applied to its config, its building put down at (9000, 10000)
/// and three ticks run: the battle and the building.
fn drill_battle(set: impl FnOnce(&mut BattleConfig)) -> (BattleState, EntityId) {
    let mut cfg: BattleConfig = config_pre_flip();
    cfg.decks = [vec!["GoblinDrill".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    set(&mut cfg);
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let d = s.scenario_spawn_now(Team::Blue, "units.GoblinDrill_EV1", n(9000, 10000), None).expect("the drill's building");
    for _ in 0..3 {
        s.tick();
    }
    (s, d)
}

/// Every blue troop but `keep` killed (the building's Goblins out of the scene).
fn clear_blue_troops(s: &mut BattleState, keep: &[EntityId]) {
    let ids: Vec<EntityId> =
        s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::Troop && !keep.contains(&e.id)).map(|e| e.id).collect();
    for id in ids {
        assert!(s.debug_set_hp(id, 0));
    }
}

/// Under `arm` (and hide.SHOT_AT_HIDING_BUILDING = client15535_lands, the Tesla's, as a 15.535.29 capture runs): a red
/// Musketeer held 4,500 off (on dry ground below the river) shoots the building; with a shot in flight two steps or more from its aim the building is
/// taken to its 66 % line and goes under. The building's hitpoints the tick after it went under, and once no shot is
/// flying at it.
fn shot_through_hide(arm: royalesim::state::DrillUnderShot) -> (i32, i32) {
    let (mut s, d) = drill_battle(|c| {
        c.calib.drill_under_shot = arm;
        c.calib.shot_at_hiding_building = royalesim::state::ShotAtHidingBuilding::Client15535Lands;
    });
    let m = s.scenario_spawn_now(Team::Red, "Musketeer", n(9000, 14500), None).expect("the Musketeer");
    let flying = |s: &BattleState| s.projectiles().iter().any(|p| p.team == Team::Red && p.target == d);
    let mut shot = false;
    for _ in 0..200 {
        clear_blue_troops(&mut s, &[]);
        assert!(s.debug_set_pos(m, n(9000, 14500)));
        s.tick();
        if s.projectiles().iter().any(|p| p.team == Team::Red && p.target == d && (p.aim.y - p.pos.y).abs() > 2 * p.speed) {
            shot = true;
            break;
        }
    }
    assert!(shot, "the scene drifted: no shot at the building in flight");
    assert!(s.debug_set_hp(d, 866));
    s.tick();
    assert_eq!(s.entity(d).expect("the building").hide_state, HideState::Hidden, "the scene drifted: it did not go under");
    s.tick();
    let held = s.entity(d).expect("the building").hp;
    for _ in 0..30 {
        if !flying(&s) {
            break;
        }
        s.tick();
    }
    assert!(!flying(&s), "the scene drifted: the shot never landed");
    (held, s.entity(d).expect("the building").hp)
}

/// hide.DRILL_UNDER_SHOT (item 285; client 15.535.29, sp-f4-drill-s0 t1011: the arrow in flight lost its target on the
/// hide tick and no hitpoint moved). Plant: drill_under_shot_lands.
#[test]
fn a_shot_in_flight_at_a_building_that_goes_under_lands_on_nothing_under_client15535_dropped() {
    let (held, after) = shot_through_hide(royalesim::state::DrillUnderShot::Client15535Dropped);
    assert_eq!(after, held, "client15535_dropped: the shot landed on the building under ground");
    // NOT VACUOUS: under lands (the Tesla's rule) the same shot takes its damage off the building.
    let (held, after) = shot_through_hide(royalesim::state::DrillUnderShot::Lands);
    assert!(after < held, "lands: the shot did not land ({held} -> {after})");
}

/// Under `arm`: a blue Knight held 650 from the building's centre (an overlap of 350), deployed before the building is
/// taken under (to its 66 % line) on T, the building's Goblins out of the scene: from T + 18 to T + 26, (the tick less T,
/// whether the Knight met a body's push).
fn rise_pushes(arm: royalesim::state::DrillRiseBody) -> Vec<(u32, bool)> {
    let (mut s, d) = drill_battle(|c| c.calib.drill_rise_body = arm);
    let kn = s.scenario_spawn_now(Team::Blue, "Knight", n(9650, 10000), None).expect("the Knight");
    let hold = |s: &mut BattleState| {
        clear_blue_troops(s, &[kn]);
        assert!(s.debug_set_pos(kn, n(9650, 10000)));
    };
    for _ in 0..25 {
        hold(&mut s);
        s.tick();
    }
    assert!(!s.entity(kn).expect("the Knight").deploying, "the scene drifted: the Knight still deploys");
    assert!(s.debug_set_hp(d, 866));
    hold(&mut s);
    s.tick();
    assert_eq!(s.entity(d).expect("the building").hide_state, HideState::Hidden, "the scene drifted: it did not go under");
    let mut rows = Vec::new();
    for k in 1..=26u32 {
        hold(&mut s);
        s.tick();
        if k >= 18 {
            let e = s.entity(kn).expect("the Knight lives");
            rows.push((k, e.push_neighbours > 0 && (e.push_applied.x != 0 || e.push_applied.y != 0)));
        }
    }
    rows
}

/// collision.DRILL_RISE_BODY (item 286; client 15.535.29: 2 of 2 units overlapping the building took no push on
/// T + 20 .. T + 22 and the capped push on T + 23). Plant: drill_body_back_at_hide_time.
#[test]
fn its_body_returns_three_ticks_after_its_hide_time_under_client15535_hide_time_plus_3() {
    let rows = rise_pushes(royalesim::state::DrillRiseBody::Client15535HideTimePlus3);
    let first = rows.iter().find(|r| r.1).map(|r| r.0);
    assert_eq!(first, Some(23), "client15535_hide_time_plus_3: the first push from the building: {rows:?}");
    // NOT VACUOUS: under hide_time the Knight meets its body on T + 20.
    let rows = rise_pushes(royalesim::state::DrillRiseBody::HideTime);
    assert_eq!(rows.iter().find(|r| r.1).map(|r| r.0), Some(20), "hide_time: the first push from the building: {rows:?}");
}

/// Under `arm`: the building taken under (to its 66 % line) on T; a red Musketeer put down 4,500 off on T + 1 and held
/// there, the building's Goblins out of the scene: the first tick (less T) it holds the building.
fn rise_pick(arm: royalesim::state::DrillRiseTargetable) -> u32 {
    let (mut s, d) = drill_battle(|c| c.calib.drill_rise_targetable = arm);
    assert!(s.debug_set_hp(d, 866));
    s.tick();
    let t = s.tick_count() - 1;
    assert_eq!(s.entity(d).expect("the building").hide_state, HideState::Hidden, "the scene drifted: it did not go under");
    let m = s.scenario_spawn_now(Team::Red, "Musketeer", n(9000, 14500), None).expect("the Musketeer");
    for _ in 0..60 {
        clear_blue_troops(&mut s, &[]);
        assert!(s.debug_set_pos(m, n(9000, 14500)));
        s.tick();
        if s.entity(m).and_then(|e| e.target) == Some(d) {
            return s.tick_count() - 1 - t;
        }
    }
    panic!("the Musketeer never took the building");
}

/// targeting.DRILL_RISE_TARGETABLE (item 287; client 15.535.29: 2 of 2 picks with the rising building nearest took it, the
/// earliest on T + 28). Plant: drill_rise_hidden.
#[test]
fn a_rising_building_is_a_target_again_under_client15535_targetable_on_rise() {
    let new = rise_pick(royalesim::state::DrillRiseTargetable::Client15535TargetableOnRise);
    assert!((20..=28).contains(&new), "client15535_targetable_on_rise: the Musketeer took the rising building on T + {new}");
    // NOT VACUOUS: under hidden_through_rise it is no target before T + 40.
    let old = rise_pick(royalesim::state::DrillRiseTargetable::HiddenThroughRise);
    assert!(old >= 40, "hidden_through_rise: the Musketeer took the building on T + {old}");
}

/// The blue building at (9000, 10000) under hide.DRILL_LINE_TEST = `arm`, a red Giant held beside it striking it: until a
/// strike lands its hitpoints are set to 892 before each tick (above its 66 % line, 879 of 1313, after one drain step), so
/// the strike's tick C crosses the line: (whether it is under at the end of C, at the end of C + 1).
fn hit_crossing(arm: royalesim::state::DrillLineTest) -> (bool, bool) {
    let (mut s, d) = drill_battle(|c| c.calib.drill_line_test = arm);
    let gat = n(9000, 11300);
    let giant = s.scenario_spawn_now(Team::Red, "Giant", gat, None).expect("the Giant");
    for _ in 0..300 {
        assert!(s.debug_set_pos(giant, gat) && s.debug_set_hp(d, 892));
        s.tick();
        let e = s.entity(d).expect("the building");
        if 892 - e.hp > 30 {
            let c = e.hide_state == HideState::Hidden;
            assert!(s.debug_set_pos(giant, gat));
            s.tick();
            let c1 = s.entity(d).expect("the building").hide_state == HideState::Hidden;
            return (c, c1);
        }
    }
    panic!("the scene drifted: the Giant never struck the building");
}

/// hide.DRILL_LINE_TEST = client16635_after_drain_before_hits (client 16.635, parity r96, the oracle's il-mg-16635 set: 22 of 22
/// crossings a hit made went under on the next tick): the Giant's strike takes the building across its 66 % line on C; it is
/// up at the end of C and under at the end of C + 1; under post_hit_hitpoints (the vacuity check) it is under at the end of
/// C. Plant: drill_line_reads_hits.
#[test]
fn under_client16635_after_drain_before_hits_a_hits_crossing_goes_under_a_tick_later() {
    use royalesim::state::DrillLineTest as D;
    assert!(hit_crossing(D::PostHitHitpoints).0, "post_hit_hitpoints (the vacuity check): up at the end of the strike's tick");
    assert_eq!(hit_crossing(D::Client16635AfterDrainBeforeHits), (false, true), "client16635_after_drain_before_hits: (under at C, under at C + 1)");
}

/// A red building put at (6000, 6000) on blue's half (2,550 from blue's left princess tower at (3500, 6500)) under
/// hide.DRILL_DIG_POINT = `arm`, its hitpoints taken under each line in turn: its native point on each of 600 ticks, and the
/// ticks it went under.
fn dig_path(arm: royalesim::state::DrillDigPoint) -> (Vec<(i32, i32)>, Vec<usize>) {
    dig_path_at(arm, (6000, 6000))
}

/// `dig_path` with the building made on `start`.
fn dig_path_at(arm: royalesim::state::DrillDigPoint, start: (i32, i32)) -> (Vec<(i32, i32)>, Vec<usize>) {
    let mut cfg: BattleConfig = config_pre_flip();
    cfg.decks = [vec!["Knight".into()], vec!["GoblinDrill".into(), "Knight".into()]];
    cfg.forms = [Vec::new(), vec![1, 0]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.drill_dig_point = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let d = s.scenario_spawn_now(Team::Red, "units.GoblinDrill_EV1", n(start.0, start.1), None).expect("the drill's building");
    let (mut path, mut unders, mut was) = (Vec::new(), Vec::new(), false);
    for k in 0..600usize {
        let Some(e) = s.entity(d) else { break };
        let (hp, top, hidden) = (e.hp, e.max_hp, e.hide_state == HideState::Hidden);
        if !hidden && unders.len() < 2 {
            // just above the next line: the drain takes it across on its own
            let line = if unders.is_empty() { 66 } else { 33 };
            let at = (line + 1) * top / 100 + 1;
            if hp > at {
                assert!(s.debug_set_hp(d, at));
            }
        }
        s.tick();
        let Some(e) = s.entity(d) else { break };
        let hidden_now = e.hide_state == HideState::Hidden;
        if hidden_now && !was {
            unders.push(k);
        }
        was = hidden_now;
        path.push((e.pos.x / K, e.pos.y / K));
    }
    (path, unders)
}

/// hide.DRILL_DIG_POINT = client16635_keeps_tower_distance (client 16.635, parity r96, the oracle's il-mg-16635 set: 32 of 32
/// hides on the enemy half moved on the under tick + 18 round the nearest tower, 08393a3e (6000, 6000) -> (4000, 9000) ->
/// (1000, 7000)): the building stands on (6000, 6000) on its first hide's + 17 and on (4000, 9000) on + 18 (the tie with
/// (3000, 4000) to the farther from blue's King); on its second hide's + 18 on (1000, 7000), not back on (6000, 6000); under
/// stays (the vacuity check) on (6000, 6000) on + 18. Plant: drill_dig_stays.
#[test]
fn under_client16635_keeps_tower_distance_the_building_digs_round_the_tower() {
    use royalesim::state::DrillDigPoint as D;
    let (old, u) = dig_path(D::Stays);
    assert!(!u.is_empty() && old.len() > u[0] + 18, "the scene drifted: no first hide ({u:?})");
    assert_eq!(old[u[0] + 18], (6000, 6000), "stays (the vacuity check): the building moved");
    let (new, u) = dig_path(D::Client16635KeepsTowerDistance);
    assert!(u.len() == 2 && new.len() > u[1] + 18, "the scene drifted: the hides {u:?}");
    assert_eq!((new[u[0] + 17], new[u[0] + 18]), ((6000, 6000), (4000, 9000)), "client16635_keeps_tower_distance: the first dig");
    assert_eq!((new[u[1] + 17], new[u[1] + 18]), ((4000, 9000), (1000, 7000)), "client16635_keeps_tower_distance: the second dig");
}

/// Since parity r96 for clients 16.402 and 16.635 (post_hit_hitpoints before it; client 15.535.29 keeps
/// post_hit_hitpoints).
#[test]
fn the_shipped_arm_is_client16635_after_drain_before_hits() {
    assert_eq!(
        royalesim::state::Calib::shipped().drill_line_test,
        royalesim::state::DrillLineTest::Client16635AfterDrainBeforeHits
    );
    assert_eq!(config().calib.drill_line_test, royalesim::state::DrillLineTest::Client16635AfterDrainBeforeHits);
}

/// hide.DRILL_DIG_POINT = client16635_princess_ring (client 16.635, parity r97, the oracle's il-mg-16635 set: 34 of 34 hides): a Red
/// building made on Blue's left princess tower's (3500, 6500) dig ring walks 5 ring steps a hide on the under tick + 18: from
/// (6000, 6000) forward to (4000, 9000) then (1000, 7000) (08393a3e's path); from (3000, 4000) backward to (1000, 7000) then
/// (4000, 9000) (883c1ff8 k43); from (6000, 8000), off the 2,550 points, to (2000, 9000) then (1000, 5000); on no ring point,
/// (5000, 5000), it stays. Under stays and under client16635_keeps_tower_distance (the vacuity checks) the off-2,550 start
/// stands. Plant: drill_ring_stays.
#[test]
fn under_client16635_princess_ring_the_building_walks_its_towers_ring() {
    use royalesim::state::DrillDigPoint as P;
    let walk = |arm: P, start: (i32, i32)| {
        let (path, u) = dig_path_at(arm, start);
        assert_eq!(u.len(), 2, "the scene drifted: {start:?} hid {u:?}");
        assert!(path.len() > u[1] + 18, "the scene drifted: {start:?}'s building gone");
        (path[u[0] + 17], path[u[0] + 18], path[u[1] + 18])
    };
    for (start, first, second) in [((6000, 6000), (4000, 9000), (1000, 7000)), ((3000, 4000), (1000, 7000), (4000, 9000)), ((6000, 8000), (2000, 9000), (1000, 5000))] {
        let (before, a, b) = walk(P::Client16635PrincessRing, start);
        assert_eq!(before, start, "client16635_princess_ring: {start:?} moved before its first hide's + 18");
        assert_eq!((a, b), (first, second), "client16635_princess_ring: {start:?}'s walk");
    }
    let (_, a, _) = walk(P::Client16635PrincessRing, (5000, 5000));
    assert_eq!(a, (5000, 5000), "client16635_princess_ring: a building on no ring point moved");
    for arm in [P::Stays, P::Client16635KeepsTowerDistance] {
        let (_, a, _) = walk(arm, (6000, 8000));
        assert_eq!(a, (6000, 8000), "{arm:?} (the vacuity check): the off-2,550 start moved");
    }
}

/// Since parity r97 for clients 16.402 and 16.635 (stays before it; client 15.535.29 keeps stays).
#[test]
fn the_shipped_arm_is_client16635_princess_ring() {
    assert_eq!(
        royalesim::state::Calib::shipped().drill_dig_point,
        royalesim::state::DrillDigPoint::Client16635PrincessRing
    );
    assert_eq!(config().calib.drill_dig_point, royalesim::state::DrillDigPoint::Client16635PrincessRing);
}

/// Under hide.DRILL_UNDER_HELD = `arm` (and hide.HELD_THROUGH_HIDE = client16402_kept): a red Musketeer held 4,500 off and a red
/// Knight held beside the building hold it; the building taken under (to its 66 % line) on U: (the Musketeer holds it, the
/// Knight holds it) at the end of U and at the end of U + 1.
fn dug_holders(arm: royalesim::state::DrillUnderHeld) -> ((bool, bool), (bool, bool)) {
    let (mut s, d) = drill_battle(|c| {
        c.calib.drill_under_held = arm;
        c.calib.hide_held_through = royalesim::state::HeldThroughHide::Client16402Kept;
    });
    let (m_at, k_at) = (n(9000, 14500), n(9000, 11300));
    let m = s.scenario_spawn_now(Team::Red, "Musketeer", m_at, None).expect("the Musketeer");
    let k = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).expect("the Knight");
    let top = s.entity(d).expect("the building").max_hp;
    let hold = |s: &mut BattleState| {
        clear_blue_troops(s, &[]);
        assert!(s.debug_set_pos(m, m_at) && s.debug_set_pos(k, k_at));
    };
    let mut both = false;
    for _ in 0..60 {
        hold(&mut s);
        assert!(s.debug_set_hp(d, top));
        s.tick();
        if s.entity(m).and_then(|e| e.target) == Some(d) && s.entity(k).and_then(|e| e.target) == Some(d) {
            both = true;
            break;
        }
    }
    assert!(both, "the scene drifted: the Musketeer and the Knight never both held the building");
    let holds = |s: &BattleState| (s.entity(m).and_then(|e| e.target) == Some(d), s.entity(k).and_then(|e| e.target) == Some(d));
    hold(&mut s);
    assert!(s.debug_set_hp(d, 866));
    s.tick();
    assert_eq!(s.entity(d).expect("the building").hide_state, HideState::Hidden, "the scene drifted: it did not go under");
    let at_u = holds(&s);
    hold(&mut s);
    s.tick();
    (at_u, holds(&s))
}

/// hide.DRILL_UNDER_HELD = client16635_dropped (client 16.635, parity r98, the oracle's il-mg-16635 set: 62 of 62 holders had
/// let a dug building go by the under tick + 1): both holders still hold it at the end of the under tick U (their Target phase
/// ran before the hide) and neither on U + 1; under kept (the vacuity check) both still hold it on U + 1. Plant:
/// drill_under_held_kept.
#[test]
fn under_client16635_dropped_its_holders_let_the_dug_building_go_on_the_under_tick_plus_1() {
    let (u, u1) = dug_holders(royalesim::state::DrillUnderHeld::Kept);
    assert_eq!((u, u1), ((true, true), (true, true)), "kept (the vacuity check): both keep it through U + 1");
    let (u, u1) = dug_holders(royalesim::state::DrillUnderHeld::Client16635Dropped);
    assert_eq!(u, (true, true), "client16635_dropped: both still hold it on U");
    assert_eq!(u1, (false, false), "client16635_dropped: neither holds it on U + 1");
}

#[test]
fn the_shipped_arm_is_kept() {
    assert_eq!(royalesim::state::Calib::shipped().drill_under_held, royalesim::state::DrillUnderHeld::Kept);
}
