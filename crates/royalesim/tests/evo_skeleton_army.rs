//! THE EVO SKELETON ARMY (card.rs `ArmyDef`; state.rs `enqueue_with`, `army_deaths`, `army_spectrals`; combat.rs
//! `resolve`), against client 15.535.29 at level 11.
//!
//! THE MEASUREMENTS (sp-form-SkeletonArmy-evo-s0; Oracle's sp-esa-spectrals-s0):
//!   - the third play of an evolved Skeleton Army entry puts the form down (DarkElixirCost 2);
//!   - the play puts its 15 soldiers down and then the General, 1000 behind the tap (tap (9500, 11500), General
//!     (9500, 10500), side 0);
//!   - every soldier death while the General lives leaves one Spectral (hp 2) on the frame the soldier is first gone,
//!     near where it died, walking from that frame;
//!   - nothing ever targeted a Spectral (towers included), and a Zap left two of them at their 2 hp;
//!   - the General's death took every living Spectral on its own frame, and soldiers dying after it left none.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_skeleton_army`):
//!   - army_general_dropped -> `the_play_puts_its_general_down_behind_the_tap` red;
//!   - army_spectral_never -> `a_soldier_dying_while_the_general_lives_leaves_a_spectral` red;
//!   - army_spectrals_outlive_general -> `the_generals_death_takes_its_spectrals_and_ends_them` red;
//!   - spectral_takes_damage -> `a_spectral_takes_no_damage` red;
//!   - spectral_visible -> `nothing_targets_a_spectral` red;
//!   - obstacle_tag_unread -> `the_general_stands_on_its_point_through_its_deploy` red;
//!   - obstacle_tag_pushes_nothing -> `the_general_still_pushes_a_unit_off_it` red;
//!   - army_general_from_tap -> `the_general_is_laid_off_the_ring_centre` red (item 325, formation.ARMY_GENERAL_POINT).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{ArmyGeneralPoint, AvoidanceObstacleTag, BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const TAP: (i32, i32) = (9500, 11500);

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["SkeletonArmy".into(), "Knight".into()], vec!["Knight".into(), "Zap".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// The form played at TAP and ticked until its units have deployed: the battle, the soldiers' ids and the General's.
fn army() -> (BattleState, Vec<EntityId>, EntityId) {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "SkeletonArmy_EV1", n(TAP.0, TAP.1), None).expect("the play");
    s.tick();
    for _ in 0..30 {
        s.tick();
    }
    let soldiers: Vec<EntityId> = find_live(&s, Team::Blue, "SkeletonArmy_EV1").iter().map(|e| e.id).collect();
    let general = find_live(&s, Team::Blue, "SkeletonArmy_EV1_General")[0].id;
    (s, soldiers, general)
}

fn spectrals(s: &BattleState) -> Vec<(EntityId, Vec2, i32, bool, i32)> {
    find_live(s, Team::Blue, "SkeletonArmy_EV1_Spectral").iter().map(|e| (e.id, e.pos, e.hp, e.deploying, e.status_flags)).collect()
}

#[test]
fn the_third_play_is_the_evolved_army() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's DarkElixirCost");
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "SkeletonArmy").expect("the army in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    for _ in 0..2 {
        assert_eq!(next(&s), "SkeletonArmy", "a basic play");
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "SkeletonArmy", n(14500, 3500)).expect("the play");
        for _ in 0..20 {
            s.tick();
        }
    }
    assert_eq!(next(&s), "SkeletonArmy_EV1", "the third play is the form");
}

#[test]
fn the_play_puts_its_general_down_behind_the_tap() {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "SkeletonArmy_EV1", n(TAP.0, TAP.1), None).expect("the play");
    s.tick();
    let soldiers = find_live(&s, Team::Blue, "SkeletonArmy_EV1");
    let generals = find_live(&s, Team::Blue, "SkeletonArmy_EV1_General");
    assert_eq!((soldiers.len(), generals.len()), (15, 1), "15 soldiers and one General");
    // Put down 1000 behind the tap, in the owner's frame. The client holds it there through its deploy (12 frames on
    // (9500, 10500) exactly); the engine lets the soldiers' contact push it on its first tick (37 native here): its
    // row's AVOIDANCE_AS_OBSTACLE is not read (open).
    let off = generals[0].pos.dist(n(TAP.0, TAP.1 - 1000)) / K;
    assert!(off <= 50, "1000 behind the tap, in the owner's frame: {:?}, {off} off", generals[0].pos);
    assert!(soldiers.iter().all(|e| e.team_seq < generals[0].team_seq), "the General after the soldiers");
}

#[test]
fn a_soldier_dying_while_the_general_lives_leaves_a_spectral() {
    let (mut s, soldiers, _) = army();
    let victim = soldiers[0];
    let at = s.entity(victim).expect("the soldier").pos;
    assert!(s.debug_set_hp(victim, 0));
    s.tick();
    assert!(s.entity(victim).is_none(), "the soldier is gone");
    let sp = spectrals(&s);
    assert_eq!(sp.len(), 1, "one Spectral on the frame the soldier is first gone");
    let (_, pos, hp, deploying, _) = sp[0];
    assert_eq!((hp, deploying), (2, false), "hp 2 at level 11, walking from its first frame");
    assert!(pos.dist(at) < 200 * K, "where the soldier died: {:?} from {:?}", pos, at);
}

#[test]
fn the_generals_death_takes_its_spectrals_and_ends_them() {
    let (mut s, soldiers, general) = army();
    for v in &soldiers[..2] {
        assert!(s.debug_set_hp(*v, 0));
    }
    s.tick();
    assert_eq!(spectrals(&s).len(), 2, "two Spectrals");
    assert!(s.debug_set_hp(general, 0));
    s.tick();
    assert!(s.entity(general).is_none() && spectrals(&s).is_empty(), "the General's death takes both on its own frame");
    assert!(s.debug_set_hp(soldiers[2], 0));
    s.tick();
    assert!(spectrals(&s).is_empty(), "a soldier dying after the General leaves none");
}

#[test]
fn a_spectral_takes_no_damage() {
    let (mut s, soldiers, _) = army();
    assert!(s.debug_set_hp(soldiers[0], 0));
    s.tick();
    let (id, pos, _, _, _) = spectrals(&s)[0];
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s.deploy(Team::Red, "Zap", pos).expect("a red Zap on it");
    let mut hit_a_soldier = false;
    for _ in 0..10 {
        let before: Vec<i32> = soldiers[1..].iter().filter_map(|v| s.entity(*v)).map(|e| e.hp).collect();
        s.tick();
        let after: Vec<i32> = soldiers[1..].iter().filter_map(|v| s.entity(*v)).map(|e| e.hp).collect();
        hit_a_soldier |= after.len() < before.len() || after.iter().zip(&before).any(|(a, b)| a < b);
        assert_eq!(s.entity(id).map(|e| e.hp), Some(2), "the Spectral keeps its 2 hp");
    }
    assert!(hit_a_soldier, "the Zap landed: a soldier beside it was hit");
}

#[test]
fn nothing_targets_a_spectral() {
    let (mut s, soldiers, _) = army();
    for v in &soldiers {
        assert!(s.debug_set_hp(*v, 0));
    }
    s.tick();
    let sp = spectrals(&s);
    assert_eq!(sp.len(), soldiers.len(), "a Spectral for every soldier");
    assert!(sp.iter().all(|x| x.4 & 2 != 0), "every Spectral invisible to enemies from its first frame");
    // A red Knight put 900 beyond the Spectrals' middle, away from the General 1000 behind the tap: the Spectrals are
    // its closest enemies. Held there with its hp topped up (the Spectrals strike it), it never takes one.
    let n_sp = sp.len() as i32;
    let mid = Vec2::new(sp.iter().map(|x| x.1.x).sum::<i32>() / n_sp, sp.iter().map(|x| x.1.y).sum::<i32>() / n_sp);
    let at = mid.add(n(0, 900));
    s.spawn_unit_resolved(Team::Red, "Knight", at, None).expect("a red Knight");
    s.tick();
    let k = find_live(&s, Team::Red, "Knight")[0].id;
    let ids: Vec<EntityId> = sp.iter().map(|x| x.0).collect();
    for _ in 0..60 {
        assert!(s.debug_set_pos(k, at) && s.debug_set_hp(k, 1766), "the Knight held");
        s.tick();
        let t = s.entity(k).and_then(|e| e.target);
        assert!(!t.is_some_and(|t| ids.contains(&t)), "the Knight never targets a Spectral: {t:?}");
    }
}

/// movement.AVOIDANCE_OBSTACLE_TAG: the General's row sets AVOIDANCE_AS_OBSTACLE. Client 15.535.29 held it on its point,
/// 1000 behind the tap, through every overlapped tick of its deploy (93 of 93 over four scenes), where the soldiers'
/// contact pushes the engine's under the old arm. Plant: obstacle_tag_unread.
#[test]
fn the_general_stands_on_its_point_through_its_deploy() {
    for (arm, still) in [(AvoidanceObstacleTag::Client15535UnpushedObstacle, true), (AvoidanceObstacleTag::NotRead, false)] {
        let mut cfg: BattleConfig = config();
        cfg.calib.avoidance_obstacle_tag = arm;
        cfg.decks = [vec!["SkeletonArmy".into(), "Knight".into()], vec!["Knight".into(), "Zap".into()]];
        cfg.forms = [vec![1, 0], Vec::new()];
        cfg.card_level = [11, 11];
        cfg.tower_level = [11, 11];
        let mut s = BattleState::new(7, cfg);
        past_deploy_lockout(&mut s);
        assert!(s.config().cards.get(s.cards().index("SkeletonArmy_EV1_General").expect("the General loads")).avoidance_as_obstacle, "the General's row carries the tag");
        s.spawn_unit(Team::Blue, "SkeletonArmy_EV1", n(TAP.0, TAP.1), None).expect("the play");
        s.tick();
        let g = find_live(&s, Team::Blue, "SkeletonArmy_EV1_General")[0].id;
        let start = s.entity(g).expect("the General").pos;
        let mut moved = false;
        for _ in 0..18 {
            s.tick();
            let e = s.entity(g).expect("the General");
            if !e.deploying {
                break;
            }
            moved |= e.pos != start;
        }
        assert_eq!(!moved, still, "{arm:?}: the General moved while deploying: {moved}");
    }
}

/// movement.AVOIDANCE_OBSTACLE_TAG: the General still pushes its neighbours. Client 15.535.29,
/// sp-form-SkeletonArmy-evo-s0 t920: the soldiers inside both radii stood where its push puts them (one 176 off with the
/// General out of its scan). A soldier made 450 behind the deploying General counts it among the units that push it, the
/// push pointing away from it, against the same scene with the General removed first. Plant: obstacle_tag_pushes_nothing.
#[test]
fn the_general_still_pushes_a_unit_off_it() {
    let mut pushes = Vec::new();
    for general in [true, false] {
        let mut cfg: BattleConfig = config();
        cfg.calib.avoidance_obstacle_tag = AvoidanceObstacleTag::Client15535UnpushedObstacle;
        cfg.decks = [vec!["SkeletonArmy".into(), "Knight".into()], vec!["Knight".into(), "Zap".into()]];
        cfg.forms = [vec![1, 0], Vec::new()];
        cfg.card_level = [11, 11];
        cfg.tower_level = [11, 11];
        let mut s = BattleState::new(7, cfg);
        past_deploy_lockout(&mut s);
        s.spawn_unit(Team::Blue, "SkeletonArmy_EV1", n(TAP.0, TAP.1), None).expect("the play");
        s.tick();
        let g = find_live(&s, Team::Blue, "SkeletonArmy_EV1_General")[0].id;
        let gp = s.entity(g).expect("the General").pos;
        if !general {
            assert!(s.debug_set_hp(g, 0), "the General removed");
            s.tick();
            assert!(s.entity(g).is_none(), "the General is gone");
        }
        let p = s.scenario_spawn_now(Team::Blue, "SkeletonArmy_EV1", gp.add(n(0, -450)), None).expect("the probe soldier");
        s.tick();
        let e = s.entity(p).expect("the probe");
        pushes.push((e.push_neighbours, e.push_applied));
        if general {
            assert_eq!(s.entity(g).expect("the General").pos, gp, "the General was moved by contact");
        }
    }
    assert_eq!(pushes[0].0, pushes[1].0 + 1, "the General is not among the probe's pushers (with, without): {pushes:?}");
    assert!(pushes[0].1.y < pushes[1].1.y, "the General's push does not point away from it (with, without): {pushes:?}");
}

/// formation.ARMY_GENERAL_POINT = client15535_ground_point (item 325): the General is laid off the soldiers' ring centre, the
/// play's point moved one native unit by formation.GROUND_DEPLOY_POINT (x on the left half, y for side 1). Client 15.535.29:
/// sp-il-b5e2 t2981, side 1 tapped on (3500, 17500), the General on (3499, 18499); sp-ec-SkeletonArmy t732, side 0 on the
/// left half, on (3499, 8500). Read on the play's first frame with the General held (movement.AVOIDANCE_OBSTACLE_TAG's client
/// arm: it stands on its point through its deploy). Plant: army_general_from_tap.
#[test]
fn the_general_is_laid_off_the_ring_centre() {
    let general_at = |arm: ArmyGeneralPoint, team: Team, tap: (i32, i32)| -> (i32, i32) {
        let mut cfg: BattleConfig = config();
        cfg.calib.army_general_point = arm;
        cfg.calib.avoidance_obstacle_tag = AvoidanceObstacleTag::Client15535UnpushedObstacle;
        cfg.card_level = [11, 11];
        cfg.tower_level = [11, 11];
        let mut s = BattleState::new(7, cfg);
        past_deploy_lockout(&mut s);
        s.spawn_unit(team, "SkeletonArmy_EV1", n(tap.0, tap.1), None).expect("the play");
        s.tick();
        let g = find_live(&s, team, "SkeletonArmy_EV1_General")[0].pos;
        (g.x / K, g.y / K)
    };
    let new = ArmyGeneralPoint::Client15535GroundPoint;
    assert_eq!(general_at(new, Team::Red, (3500, 17500)), (3499, 18499), "side 1 on the left half (sp-il-b5e2 t2981)");
    assert_eq!(general_at(new, Team::Blue, (3500, 9500)), (3499, 8500), "side 0 on the left half (sp-ec-SkeletonArmy t732)");
    assert_eq!(general_at(new, Team::Blue, TAP), (9500, 10500), "side 0 on the right half: as off the tap");
    // NOT VACUOUS: the old arm lays it off the play's point.
    assert_eq!(general_at(ArmyGeneralPoint::Tap, Team::Red, (3500, 17500)), (3500, 18500), "tap: side 1 on the left half");
}
