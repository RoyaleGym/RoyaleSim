//! WHEN AN EVO SKELETONS GROUP'S ROOM FOR A COPY IS READ -- calibration spawner.EVO_COPY_COUNT, state.rs `evo_after_fire`,
//! `evo_copies`.
//!
//! THE READING (client 15.535.29): every hit of a group's member earns a copy while the group holds fewer than
//! GroupMaxSize (8). On the six ticks a group of 8 took a hit while one of its members died on the same tick, no copy came
//! (sp-esk-bank-5500-s0 t937, the scene's first divergence: the engine's copy that tick, the client's two ticks later on
//! the group's next hit; sp-il-8b9b t1521, t1615, t1661; sp-m4-towerhit-s0 t1330, t1390): the room is read at the hit,
//! where the dying member still counts, not after the tick's deaths.
//!
//! The scene is tests/evolution.rs's: blue Evo Skeletons on a red Giant (made too strong to die). Each time the group holds
//! 8, a member that has just hit (it cannot hit again on the next tick) is put to 0 hp before the tick, so it is reaped at
//! the tick's end; on such a tick that brings a hit, the copies made are counted.
//!
//! PLANT (regression): evo_copy_counted_after_reap -> `a_hit_on_a_full_group_with_a_member_dying_makes_no_copy` red.
//!   RUSTFLAGS='--cfg clash_plant="evo_copy_counted_after_reap"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test evo_copy_count
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, EvoCopyCount};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The copies made on each tick that a group of 8 took a hit while a member (put to 0 hp before it) died, up to three.
fn trials(arm: EvoCopyCount) -> Vec<usize> {
    let mut cfg = config();
    cfg.calib.evo_copy_count = arm;
    // the 15.535.29 scenes' copies, on the hit's tick (spawner.EVO_COPY_TICK's old arm)
    cfg.calib.evo_copy_tick = royalesim::state::EvoCopyTick::HitTick;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let form = s.cards().index("Skeletons_EV1").expect("the Evo Skeletons load");
    s.spawn_unit(Team::Red, "Giant", n(9000, 13500), None).unwrap();
    s.spawn_unit(Team::Blue, "Skeletons_EV1", n(9000, 12000), None).unwrap();
    s.tick();
    let giant = find_live(&s, Team::Red, "Giant")[0].id;
    let members = |s: &BattleState| -> Vec<(EntityId, AttackPhase)> { s.entities().filter(|e| e.team == Team::Blue && e.card_idx == form).map(|e| (e.id, e.attack_phase)).collect() };
    let mut out = Vec::new();
    for _ in 0..1200 {
        assert!(s.debug_set_hp(giant, 1_000_000));
        let before = members(&s);
        let victim = if before.len() == 8 { before.iter().find(|(_, p)| *p == AttackPhase::Cooldown).map(|(id, _)| *id) } else { None };
        if let Some(v) = victim {
            assert!(s.debug_set_hp(v, 0));
        }
        s.tick();
        let after = members(&s);
        let hits = after.iter().filter(|(_, p)| *p == AttackPhase::Cooldown).count();
        if let Some(v) = victim {
            assert!(s.entity(v).is_none(), "the member put to 0 hp was not reaped on its tick");
            if hits > 0 {
                out.push(after.iter().filter(|(id, _)| !before.iter().any(|(b, _)| b == id)).count());
                if out.len() == 3 {
                    break;
                }
            }
        }
    }
    assert!(!out.is_empty(), "the scene drifted: no hit on a full group's dying tick");
    out
}

/// Plant: evo_copy_counted_after_reap.
#[test]
fn a_hit_on_a_full_group_with_a_member_dying_makes_no_copy() {
    let out = trials(EvoCopyCount::Client15535AtHit);
    assert!(out.iter().all(|&c| c == 0), "copies made on a full group's dying tick: {out:?}");
}

#[test]
fn the_shipped_arm_counts_the_group_after_the_ticks_deaths() {
    assert_eq!(Calib::shipped().evo_copy_count, EvoCopyCount::AfterReap);
    let out = trials(EvoCopyCount::AfterReap);
    assert!(out.iter().all(|&c| c >= 1), "the shipped arm made no copy on a full group's dying tick: {out:?}");
}
