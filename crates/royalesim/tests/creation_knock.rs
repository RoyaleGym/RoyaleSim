//! knockback.CREATION_HIT_KNOCK = client16635_at_once: a Bowler's boulder that hits a unit created after the Bowler on its
//! creation tick (combat.rs `fire`, the Attack phase's test on its launch point) moves the unit on that tick, where next_tick
//! moves it on the next.
//!
//! Plant: creation_knock_next_tick (`under_client16635_at_once_a_boulder_s_creation_tick_hit_moves_its_victim_on_that_tick` red).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, CreationHitKnock};
use royalesim::Team;

/// A blue Bowler held at (9000, 12000) (out of every tower's reach, short of the river) and, created after him, a red Knight held
/// `d` ahead of him (native) until he loses
/// hitpoints, under knockback.CREATION_HIT_KNOCK = `arm`: (the Knight's first hit tick, the first tick he moved more than 120
/// native from where he was held, whether a projectile was born on the hit's tick). None when the scene gives no hit or no move.
fn boulder(arm: CreationHitKnock, d: i32) -> Option<(u32, u32, bool)> {
    let mut cfg = config();
    cfg.calib.creation_hit_knock = arm;
    let mut s = BattleState::new(0, cfg);
    let (b_at, k_at) = (Vec2::new(9000 * K, 12000 * K), Vec2::new(9000 * K, (12000 + d) * K));
    let bowler = s.scenario_spawn_now(Team::Blue, "Bowler", b_at, None).expect("the Bowler");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).expect("the Knight");
    let (mut hit, mut born) = (None, false);
    for t in 0..200u32 {
        if s.entity(bowler).is_some() {
            let hb = s.entity(bowler).expect("him").max_hp;
            assert!(s.debug_set_pos(bowler, b_at) && s.debug_set_hp(bowler, hb));
        }
        if hit.is_none() {
            let hk = s.entity(knight)?.max_hp;
            assert!(s.debug_set_pos(knight, k_at) && s.debug_set_hp(knight, hk));
        }
        let shots = s.projectiles().len();
        s.tick();
        let k = s.entity(knight)?;
        if hit.is_none() && k.hp < k.max_hp {
            hit = Some(t);
            born = s.projectiles().len() > shots;
        }
        if hit.is_some() {
            let m = k.pos.sub(k_at);
            let (dx, dy) = ((m.x / K) as i64, (m.y / K) as i64);
            if dx * dx + dy * dy > 120 * 120 {
                return hit.map(|h| (h, t, born));
            }
        }
    }
    None
}

/// knockback.CREATION_HIT_KNOCK = client16635_at_once (client 16.635, parity r94, live15n: 4 of 4 Bowler creation-tick knocks on
/// units created after him moved them on the hit's tick; the engine moved them on the next): for a Knight a boulder hits on the
/// tick it is born (the nearest distance whose hit is a creation-tick one), the new arm moves him on the hit's tick, where
/// next_tick (the vacuity check) moves him on the tick after. Plant: creation_knock_next_tick.
#[test]
fn under_client16635_at_once_a_boulder_s_creation_tick_hit_moves_its_victim_on_that_tick() {
    let d = (1200..=2900)
        .step_by(100)
        .find(|&d| boulder(CreationHitKnock::NextTick, d).is_some_and(|(_, _, born)| born))
        .expect("the scene drifted: no Knight distance gives a boulder hit on the tick it is born");
    let (h_old, m_old, _) = boulder(CreationHitKnock::NextTick, d).expect("next_tick: the scene");
    let (h_new, m_new, born) = boulder(CreationHitKnock::Client16635AtOnce, d).expect("client16635_at_once: the scene");
    assert!(born, "the scene: client16635_at_once's hit is not on the boulder's creation tick");
    assert_eq!(h_new, h_old, "the scene: the two arms hit the Knight on one tick");
    assert_eq!(m_old, h_old + 1, "the vacuity check: next_tick moves him on the tick after the hit (d = {d})");
    assert_eq!(m_new, h_new, "client16635_at_once: the Knight hit on tick {h_new} moved on tick {m_new}, not on the hit's tick (d = {d})");
}
