//! combat.NET_INITIAL_COOLDOWN: the tick an Evo Hunter's net is first ready (state.rs `evo_created`'s net run).
//!
//! THE LAW, measured on client 15.535.29 (parity's net_ready_census.py, every first net of the Evo Hunter scenes): with a
//! target in his net's reach as his deploy ends, his first net comes 9 ticks after he takes it (held 5, cast 4), 29
//! after his creation (sp-f4-hunter-s0, -hunterBD-s0, -hunterK-s0): his InitialCooldown (1000 ms) runs from his
//! creation, through his 1000 ms deploy. The old arm readies it DeployTime + InitialCooldown after his creation, 15 ticks
//! later.
//!
//! The scene: Blue's Evo Hunter put down on (4000, 12500), a Red Knight held 4000 ahead from the start (2900 edge to
//! edge, inside the net's 4000), its hp topped up. WHAT IS PINNED, each with its precondition (he takes the Knight as his
//! deploy ends):
//!   1. client15535_from_creation: the first net comes 9 ticks after he takes the Knight;
//!   2. after_deploy: later than that (the old arm; its ready tick binds);
//!   3. the shipped value is after_deploy (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test net_initial_cooldown`):
//!   * `net_ready_after_deploy` -- the new arm's net still waits out the deploy first: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, NetInitialCooldown};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const AT: (i32, i32) = (4000, 12500);
const AHEAD: i32 = 4000;
/// The ticks watched after he is put down.
const WATCH: u32 = 120;

/// The nets in flight now (a no-damage shot of his carrying a buff).
fn nets(s: &BattleState, h: EntityId) -> usize {
    s.projectiles().iter().filter(|p| p.firer == Some(h) && p.damage == 0 && p.buff.is_some()).count()
}

/// Under `arm`: the tick (counted from his creation) he takes the Knight and the tick his first net is thrown.
fn first_net(arm: NetInitialCooldown) -> (u32, u32) {
    let mut cfg: BattleConfig = config();
    cfg.calib.net_initial_cooldown = arm;
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
    let (mut taken, mut thrown) = (None, None);
    for t in 1..=WATCH {
        assert!(s.debug_set_pos(h, n(AT.0, AT.1)) && s.debug_set_pos(k, kat));
        let top = s.entity(k).expect("the Knight").max_hp;
        assert!(s.debug_set_hp(k, top));
        s.tick();
        if taken.is_none() && s.entity(h).expect("the Hunter").target == Some(k) {
            taken = Some(t);
        }
        if thrown.is_none() && nets(&s, h) > 0 {
            thrown = Some(t);
        }
    }
    let taken = taken.unwrap_or_else(|| panic!("{arm:?}: the scene drifted: he never took the Knight"));
    let thrown = thrown.unwrap_or_else(|| panic!("{arm:?}: the scene drifted: no net in {WATCH} ticks"));
    assert!(taken <= 25, "{arm:?}: the scene drifted: he took the Knight {taken} ticks in, not as his deploy ended");
    (taken, thrown)
}

#[test]
fn his_first_net_comes_9_ticks_after_he_takes_a_target_in_reach_as_his_deploy_ends() {
    let (taken, thrown) = first_net(NetInitialCooldown::Client15535FromCreation);
    assert_eq!(thrown - taken, 9, "new: the first net {thrown} ticks in, he took the Knight {taken} ticks in");
}

#[test]
fn the_old_value_waits_out_the_deploy_first() {
    let (taken, thrown) = first_net(NetInitialCooldown::AfterDeploy);
    assert!(thrown - taken > 9, "old: the first net {thrown} ticks in, only 9 after he took the Knight ({taken})");
}

#[test]
fn the_shipped_value_is_after_deploy() {
    assert_eq!(Calib::shipped().net_initial_cooldown, NetInitialCooldown::AfterDeploy);
}
