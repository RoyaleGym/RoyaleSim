//! THE EVO HUNTER'S NET AND HIS SHOTS -- calibration combat.NET_CAST_WINDOW, state.rs `net_pass` and `net_shot_window`.
//!
//! THE LAW (client 15.535.29, all 19 Evo Hunter nets in the captures): a ready net is cast on the first tick, his target
//! within the net's reach, outside his shot window: the tick his load timer is set to LoadTime (a shot, or his attack's
//! entry) and the 4 after it, and while he attacks the 4 before his next shot. No hold on the target. The engine's old
//! arm casts once he has held his target 5 ticks, which agrees on a net ready far from a shot (15 of 19) and casts inside
//! the window otherwise (sp-f4-hunterG40-s0: ready on his shot tick t1224, the client cast on t1229).
//!
//! The scene: Blue's Evo Hunter on (4000, 12500), a Red Knight held 4,000 ahead (in his reach and his net's), its
//! hitpoints topped up, for 1,500 ticks: some 13 nets, each ready 101 ticks after the last throw while his shots run every
//! 44, so the ready ticks fall at every point of his cycle. The cast tick is the throw less TrapCastTime's 4; the shot
//! ticks are those his load timer reads LoadTime (1,500) after.
//!   client15535_shot_window: no cast inside a window, and some net cast later than under the old arm;
//!   target_hold (NOT VACUOUS): some cast inside a window.
//!
//! PLANT (regression): net_window_unread -> `an_evo_hunters_net_is_cast_outside_his_shot_window_under_client15535_shot_window` red.
//!   RUSTFLAGS='--cfg clash_plant="net_window_unread"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   net_cast_window
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, NetCastWindow};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const AT: (i32, i32) = (4000, 12500);
const AHEAD: i32 = 4000;
const WATCH: u32 = 1500;
/// TrapCastTime 200 ms: the throw is 4 ticks after the cast.
const CAST_TICKS: u32 = 4;
const W: u32 = 4;

/// The nets in flight now (a no-damage shot of his carrying a buff).
fn nets(s: &BattleState, h: EntityId) -> usize {
    s.projectiles().iter().filter(|p| p.firer == Some(h) && p.damage == 0 && p.buff.is_some()).count()
}

/// Under `arm`: the cast ticks of his nets and the ticks his load timer reads LoadTime after (his entry and his shots).
fn run(arm: NetCastWindow) -> (Vec<u32>, Vec<u32>) {
    let mut cfg: BattleConfig = config();
    cfg.calib.net_cast_window = arm;
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
    let kat = n(AT.0, AT.1 + AHEAD);
    let k = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("a red Knight");
    s.spawn_unit(Team::Blue, "Hunter_EV1", n(AT.0, AT.1), None).expect("the Hunter");
    s.tick();
    let h = find_live(&s, Team::Blue, "Hunter_EV1").first().expect("the Hunter").id;
    let (mut casts, mut loads) = (Vec::new(), Vec::new());
    let mut flying = 0;
    for t in 1..=WATCH {
        assert!(s.debug_set_pos(h, n(AT.0, AT.1)) && s.debug_set_pos(k, kat));
        let top = s.entity(k).expect("the Knight").max_hp;
        assert!(s.debug_set_hp(k, top));
        s.tick();
        let now = nets(&s, h);
        if now > flying {
            casts.push(t - CAST_TICKS);
        }
        flying = now;
        if s.entity(h).expect("the Hunter").attack_load_ms == 1500 {
            loads.push(t);
        }
    }
    assert!(casts.len() >= 10, "{arm:?}: the scene drifted: {} nets in {WATCH} ticks", casts.len());
    assert!(loads.len() >= 20, "{arm:?}: the scene drifted: {} shots in {WATCH} ticks", loads.len());
    (casts, loads)
}

/// Whether tick `c` lies in a shot window: on or within W after a load set, or within W before a shot (a load set after
/// the first, his entry).
fn in_window(c: u32, loads: &[u32]) -> bool {
    loads.iter().any(|&s| c >= s && c <= s + W) || loads.iter().skip(1).any(|&s| c + W >= s && c < s)
}

/// Plant: net_window_unread.
#[test]
fn an_evo_hunters_net_is_cast_outside_his_shot_window_under_client15535_shot_window() {
    let (new, loads) = run(NetCastWindow::Client15535ShotWindow);
    let inside: Vec<u32> = new.iter().copied().filter(|&c| in_window(c, &loads)).collect();
    assert!(inside.is_empty(), "client15535_shot_window: nets cast inside a shot window: {inside:?} (casts {new:?}, loads {loads:?})");
    // NOT VACUOUS: the hold casts inside one, and the window delays some net.
    let (old, loads_old) = run(NetCastWindow::TargetHold);
    assert!(old.iter().any(|&c| in_window(c, &loads_old)), "target_hold: no net cast inside a shot window (casts {old:?}, loads {loads_old:?})");
    assert_ne!(new, old, "the arms cast every net on the same tick");
}

#[test]
fn the_shipped_arm_holds_the_target() {
    assert_eq!(Calib::shipped().net_cast_window, NetCastWindow::TargetHold);
}
