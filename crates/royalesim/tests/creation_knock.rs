//! knockback.CREATION_HIT_KNOCK = client16635_at_once: a Bowler's boulder that hits a unit created after the Bowler on its
//! creation tick (combat.rs `fire`, the Attack phase's test on its launch point) moves the unit on that tick, where next_tick
//! moves it on the next. client16635_at_once_any_age (a candidate, parity r98): a unit created before the Bowler too.
//! knockback.CREATION_STEP_OFFSET = client16635_start_offset (a candidate, parity r98): that first step turns by the victim's
//! start-of-tick avoidance offset.
//!
//! Plants: creation_knock_next_tick (`under_client16635_at_once_a_boulder_s_creation_tick_hit_moves_its_victim_on_that_tick`
//! red); creation_knock_younger_only (`under_client16635_at_once_any_age_an_older_victim_moves_on_the_hit_tick` red);
//! creation_step_unturned (`under_client16635_start_offset_a_creation_tick_step_turns_by_the_victims_offset` red).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, CreationHitKnock, CreationStepOffset};
use royalesim::Team;

/// One boulder scene's outcome: the Knight's first hit tick, the first tick he moved more than 120 native from where he was
/// held, whether a projectile was born on the hit's tick, and his displacement on the hit's tick (native, from his held point).
#[derive(Debug, Clone, Copy)]
struct Hit {
    hit: u32,
    moved: u32,
    born: bool,
    step: (i32, i32),
}

/// A blue Bowler held at (9000, 12000) (out of every tower's reach, short of the river) and a red Knight held `d` ahead of him
/// (native) until he loses hitpoints, created before the Bowler when `older` (after him otherwise), his avoidance offset set to
/// `offset` before every tick until the hit, under knockback.CREATION_HIT_KNOCK = `arm` and CREATION_STEP_OFFSET = `turn`.
/// None when the scene gives no hit or no move.
fn boulder(arm: CreationHitKnock, turn: CreationStepOffset, d: i32, older: bool, offset: i32) -> Option<Hit> {
    let mut cfg = config();
    cfg.calib.creation_hit_knock = arm;
    cfg.calib.creation_step_offset = turn;
    let mut s = BattleState::new(0, cfg);
    let (b_at, k_at) = (Vec2::new(9000 * K, 12000 * K), Vec2::new(9000 * K, (12000 + d) * K));
    let (bowler, knight) = if older {
        let knight = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).expect("the Knight");
        (s.scenario_spawn_now(Team::Blue, "Bowler", b_at, None).expect("the Bowler"), knight)
    } else {
        let bowler = s.scenario_spawn_now(Team::Blue, "Bowler", b_at, None).expect("the Bowler");
        (bowler, s.scenario_spawn_now(Team::Red, "Knight", k_at, None).expect("the Knight"))
    };
    let (mut hit, mut born, mut step) = (None, false, (0, 0));
    for t in 0..200u32 {
        if s.entity(bowler).is_some() {
            let hb = s.entity(bowler).expect("him").max_hp;
            assert!(s.debug_set_pos(bowler, b_at) && s.debug_set_hp(bowler, hb));
        }
        if hit.is_none() {
            let hk = s.entity(knight)?.max_hp;
            assert!(s.debug_set_pos(knight, k_at) && s.debug_set_hp(knight, hk));
            if offset != 0 {
                assert!(s.debug_set_avoid_offset(knight, offset));
            }
        }
        let shots = s.projectiles().len();
        s.tick();
        let k = s.entity(knight)?;
        let m = k.pos.sub(k_at);
        let (dx, dy) = ((m.x / K) as i64, (m.y / K) as i64);
        if hit.is_none() && k.hp < k.max_hp {
            hit = Some(t);
            born = s.projectiles().len() > shots;
            step = (dx as i32, dy as i32);
        }
        if hit.is_some() && dx * dx + dy * dy > 120 * 120 {
            return hit.map(|h| Hit { hit: h, moved: t, born, step });
        }
    }
    None
}

/// The nearest Knight distance (native, 1200..=2900 by 100) whose boulder hit is on the boulder's creation tick under
/// next_tick, the Knight created before the Bowler when `older`.
fn creation_distance(older: bool) -> i32 {
    (1200..=2900)
        .step_by(100)
        .find(|&d| boulder(CreationHitKnock::NextTick, CreationStepOffset::Unrotated, d, older, 0).is_some_and(|h| h.born))
        .expect("the scene drifted: no Knight distance gives a boulder hit on the tick it is born")
}

/// knockback.CREATION_HIT_KNOCK = client16635_at_once (client 16.635, parity r94, live15n: 4 of 4 Bowler creation-tick knocks on
/// units created after him moved them on the hit's tick; the engine moved them on the next): for a Knight a boulder hits on the
/// tick it is born (the nearest distance whose hit is a creation-tick one), the new arm moves him on the hit's tick, where
/// next_tick (the vacuity check) moves him on the tick after. Plant: creation_knock_next_tick.
#[test]
fn under_client16635_at_once_a_boulder_s_creation_tick_hit_moves_its_victim_on_that_tick() {
    let d = creation_distance(false);
    let old = boulder(CreationHitKnock::NextTick, CreationStepOffset::Unrotated, d, false, 0).expect("next_tick: the scene");
    let new = boulder(CreationHitKnock::Client16635AtOnce, CreationStepOffset::Unrotated, d, false, 0).expect("client16635_at_once: the scene");
    assert!(new.born, "the scene: client16635_at_once's hit is not on the boulder's creation tick");
    assert_eq!(new.hit, old.hit, "the scene: the two arms hit the Knight on one tick");
    assert_eq!(old.moved, old.hit + 1, "the vacuity check: next_tick moves him on the tick after the hit (d = {d})");
    assert_eq!(new.moved, new.hit, "client16635_at_once: the Knight hit on tick {} moved on tick {}, not on the hit's tick (d = {d})", new.hit, new.moved);
}

/// knockback.CREATION_HIT_KNOCK = client16635_at_once_any_age (client 16.635, parity r98, the oracle's il-mg-16635 set: 3 of 3
/// boulder victims created before the Bowler stepped on the hit's tick): a Knight created before the Bowler, hit on the
/// boulder's creation tick, moves on the hit's tick under the new arm; under client16635_at_once (the vacuity check, the
/// shipped arm) on the tick after. A younger Knight moves on the hit's tick under both. Plant: creation_knock_younger_only.
#[test]
fn under_client16635_at_once_any_age_an_older_victim_moves_on_the_hit_tick() {
    // The hit tick's displacement (from the held point) is compared between the arms: at the nearest creation-tick distance
    // the older Knight overlaps the Bowler, and the contact push of that tick is in both.
    let d = creation_distance(true);
    let nt = boulder(CreationHitKnock::NextTick, CreationStepOffset::Unrotated, d, true, 0).expect("next_tick: the scene");
    let old = boulder(CreationHitKnock::Client16635AtOnce, CreationStepOffset::Unrotated, d, true, 0).expect("client16635_at_once: the scene");
    let new = boulder(CreationHitKnock::Client16635AtOnceAnyAge, CreationStepOffset::Unrotated, d, true, 0).expect("any_age: the scene");
    assert!(new.born && new.hit == old.hit && nt.hit == old.hit, "the scene: one creation-tick hit under the arms ({nt:?}, {old:?}, {new:?})");
    assert_eq!(old.step, nt.step, "client16635_at_once (the vacuity check): the older Knight takes no step on the hit's tick, as next_tick (d = {d})");
    let (gx, gy) = (new.step.0 - old.step.0, new.step.1 - old.step.1);
    assert!(gx.abs() <= 2 && (190..=205).contains(&gy), "client16635_at_once_any_age: the older Knight takes its first ladder step (about 199 ahead) on the hit's tick ({old:?} -> {new:?}, d = {d})");
    let dy = creation_distance(false);
    for arm in [CreationHitKnock::Client16635AtOnce, CreationHitKnock::Client16635AtOnceAnyAge] {
        let y = boulder(arm, CreationStepOffset::Unrotated, dy, false, 0).expect("the younger Knight's scene");
        assert_eq!(y.moved, y.hit, "{arm:?}: the younger Knight moves on the hit's tick (d = {dy})");
    }
}

/// knockback.CREATION_STEP_OFFSET = client16635_start_offset (client 16.635, parity r98, the oracle's il-mg-16635 set: 5 of 7
/// boulder first steps on victims holding an offset exact once turned): a younger Knight holding an avoidance offset of +100,
/// hit on the boulder's creation tick, takes his first ladder step turned by it: the push straight ahead (+y, about 199) turned
/// by +100 in the move pass's rotation is about (107, 168), so against the unrotated arm (the vacuity check) the hit tick's
/// displacement gains about 107 on x and loses about 31 on y. With no offset the two arms step alike. Plant:
/// creation_step_unturned.
#[test]
fn under_client16635_start_offset_a_creation_tick_step_turns_by_the_victims_offset() {
    let d = creation_distance(false);
    let arm = CreationHitKnock::Client16635AtOnce;
    let old = boulder(arm, CreationStepOffset::Unrotated, d, false, 100).expect("unrotated: the scene");
    let new = boulder(arm, CreationStepOffset::Client16635StartOffset, d, false, 100).expect("client16635_start_offset: the scene");
    assert!(new.born && new.hit == old.hit && new.moved == new.hit, "the scene: a creation-tick step on the hit's tick ({old:?}, {new:?})");
    let (gx, gy) = (new.step.0 - old.step.0, old.step.1 - new.step.1);
    assert!((gx - 107).abs() <= 4 && (gy - 31).abs() <= 4, "client16635_start_offset: the step turned by +100 ({old:?} -> {new:?}: +{gx} x, -{gy} y)");
    let old0 = boulder(arm, CreationStepOffset::Unrotated, d, false, 0).expect("unrotated, no offset");
    let new0 = boulder(arm, CreationStepOffset::Client16635StartOffset, d, false, 0).expect("start_offset, no offset");
    assert_eq!(new0.step, old0.step, "no offset: the two arms step alike");
}

/// Since parity r94 (next_tick before it; client 15.535.29 keeps next_tick).
#[test]
fn the_shipped_arm_is_client16635_at_once() {
    use royalesim::state::Calib;
    assert_eq!(Calib::shipped().creation_hit_knock, CreationHitKnock::Client16635AtOnce);
    assert_eq!(config().calib.creation_hit_knock, CreationHitKnock::Client16635AtOnce);
}

#[test]
fn the_shipped_arm_is_unrotated() {
    assert_eq!(royalesim::state::Calib::shipped().creation_step_offset, CreationStepOffset::Unrotated);
}
