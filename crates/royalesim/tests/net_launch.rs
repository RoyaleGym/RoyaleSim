//! combat.NET_LAUNCH: where an Evo Hunter's net starts and when it first steps (state.rs `throw_net`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, three nets' tracks in the raw frames: first seen 800 from
//! the Hunter (his collision radius 600 + the net's ProjectileStartExtraRadius 200) and 600 on along the line on the next
//! frame.
//!
//! THE SCENE (tests/evo_hunter.rs's): the evolved Hunter held on (4000, 12500), Blue's princess towers down, a red Golem
//! held 4,000 ahead and topped up; his first net. WHAT IS PINNED:
//!   1. client15535_edge_next_tick: the net's first frame stands 800 from him (to 2) and its second frame has
//!      moved on;
//!   2. extra_and_wait (the old arm, the vacuity check): 1,200 from him, and its second frame where its first was;
//!   3. the shipped value is extra_and_wait (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test net_launch`):
//!   * `net_launch_extra_and_wait` -- the new arm's net still starts the extra out and waits: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, NetLaunch};
use royalesim::{EntityId, Team};

const AT: (i32, i32) = (4000, 12500);

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`: the first net's distance from the Hunter on its first frame (native), and whether its second frame
/// stands elsewhere.
fn first_net(arm: NetLaunch) -> (i64, bool) {
    let mut cfg: BattleConfig = config();
    cfg.calib.net_launch = arm;
    cfg.decks = [vec!["Hunter".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.spawn_unit(Team::Blue, "Hunter_EV1", n(AT.0, AT.1), None).expect("the Hunter");
    s.tick();
    let h: EntityId = find_live(&s, Team::Blue, "Hunter_EV1").first().expect("the Hunter").id;
    for _ in 0..60 {
        assert!(s.debug_set_pos(h, n(AT.0, AT.1)));
        s.tick();
    }
    let gat = n(AT.0, AT.1 + 4000);
    let g = s.scenario_spawn_now(Team::Red, "Golem", gat, None).expect("a red Golem");
    for _ in 0..200 {
        assert!(s.debug_set_pos(h, n(AT.0, AT.1)) && s.debug_set_pos(g, gat));
        let max = s.entity(g).expect("the Golem").max_hp;
        assert!(s.debug_set_hp(g, max));
        s.tick();
        let net = s.projectiles().iter().find(|p| p.firer == Some(h) && p.damage == 0 && p.buff.is_some()).map(|p| p.pos);
        if let Some(first) = net {
            let (dx, dy) = (i64::from((first.x - n(AT.0, AT.1).x) / K), i64::from((first.y - n(AT.0, AT.1).y) / K));
            assert!(s.debug_set_pos(h, n(AT.0, AT.1)) && s.debug_set_pos(g, gat));
            s.tick();
            let second = s.projectiles().iter().find(|p| p.firer == Some(h) && p.damage == 0 && p.buff.is_some()).map(|p| p.pos);
            return (isqrt(dx * dx + dy * dy), second != Some(first));
        }
    }
    panic!("{arm:?}: the scene drifted: no net within 200 ticks");
}

/// Plant: net_launch_extra_and_wait.
#[test]
fn his_net_starts_at_his_edge_plus_its_extra_and_steps_next_tick_under_client15535_edge_next_tick() {
    let (d, moved) = first_net(NetLaunch::Client15535EdgeNextTick);
    assert!((d - 800).abs() <= 2, "new: the net's first frame stood {d} from him");
    assert!(moved, "new: the net did not step on the tick after its throw");
}

#[test]
fn the_old_value_starts_it_the_extra_out_and_waits() {
    let (d, moved) = first_net(NetLaunch::ExtraAndWait);
    assert!((d - 1200).abs() <= 2, "old: the net's first frame stood {d} from him");
    assert!(!moved, "old: the net stepped on the tick after its throw (vacuous otherwise)");
}

#[test]
fn the_shipped_value_is_extra_and_wait() {
    assert_eq!(Calib::shipped().net_launch, NetLaunch::ExtraAndWait);
}
