//! targeting.SNIPE_REPICK: the target an Evo Musketeer's next snipe takes once she has let her last snipe's target go.
//!
//! Client 15.535.29 (sp-il-2142 t2386): her second snipe's target, a Cannon 11,916 ahead of her, was taken again for her
//! third over two Skeletons standing nearer ahead inside the side clip; the engine's nearest-ahead pick took a Skeleton.
//!
//! The scene: she stands at (3499, 9500); a Giant is put down 12,000 ahead with her, and a Knight 8,000 ahead 25 ticks
//! later, while she aims her first snipe at the Giant (a target she aims at is kept, so the Knight does not take it).
//! Her first snipe leaves at the Giant; on her next pick the Knight, still deploying, stands nearer ahead than the Giant
//! and past her attack reach (6000 plus both radii). Plant: snipe_last_target_unread.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, SnipeRepick};
use royalesim::Team;

/// A native point, in subtiles.
fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The cards her snipes (Speed 2650) leave at, in order, over `ticks` ticks under `arm`.
fn snipe_targets(arm: SnipeRepick, ticks: u32) -> Vec<String> {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.snipe_repick = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let form = s.cards().index("Musketeer_EV1").expect("the evolved Musketeer loads");
    let mult = s.config().calib.projectile_speed_to_subtiles_per_tick;
    s.spawn_unit(Team::Blue, "Musketeer_EV1", n(3499, 9500), None).unwrap();
    let mut out = Vec::new();
    for k in 0..ticks {
        if k == 0 {
            s.spawn_unit(Team::Red, "Giant", n(3499, 21499), None).unwrap();
        }
        if k == 25 {
            s.spawn_unit(Team::Red, "Knight", n(3499, 17499), None).unwrap();
        }
        s.tick();
        let Some(m) = s.entities().find(|e| e.card_idx == form) else { continue };
        if m.deploying || m.attack_phase != AttackPhase::Cooldown {
            continue;
        }
        let p = s.projectiles().iter().rev().find(|p| p.firer_card == Some(form)).expect("her shot");
        if p.speed / mult == 2650 {
            out.push(s.entity(p.target).expect("a shot's target lives as it leaves").card.to_string());
        }
    }
    out
}

#[test]
fn the_engines_next_snipe_takes_the_nearest_ahead() {
    // The scene discriminates: under the old arm her second snipe leaves at the nearer Knight.
    let t = snipe_targets(SnipeRepick::NearestAhead, 80);
    assert!(t.len() >= 2, "{t:?}");
    assert_eq!((t[0].as_str(), t[1].as_str()), ("Giant", "Knight"), "{t:?}");
}

#[test]
fn the_clients_next_snipe_takes_her_last_target_first() {
    let t = snipe_targets(SnipeRepick::Client15535LastTargetFirst, 80);
    assert!(t.len() >= 2, "{t:?}");
    assert_eq!((t[0].as_str(), t[1].as_str()), ("Giant", "Giant"), "{t:?}");
}
