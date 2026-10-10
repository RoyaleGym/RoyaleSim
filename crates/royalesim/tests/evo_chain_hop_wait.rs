//! combat.EVO_CHAIN_HOP_WAIT: whether an Evo Electro Dragon's hops after the first wait on the unit they hit before they
//! fly (combat.rs `step_projectiles`, `EVO_CHAIN_HOP_WAIT_TICKS`).
//!
//! THE READING (client 15.535.29, Oracle's sp-f4-ed-s0, sp-f4-ed3-s0, sp-form-ElectroDragon-evo-s0): a chain's first hop
//! starts on its shot's landing tick, every later hop's track 2 ticks after the last one's landing (67 of 77).
//!
//! The scene: evo_electro_dragon.rs's (Oracle's sp-f4-ed3-s0 laid out): the form held 3499 from a red Knight, an Ice Golem
//! 1200 from it, a Valkyrie 3100 from the Ice Golem, all held and topped up. Pinned: the gaps between the chain's first
//! four hits; under client15535_two_ticks the first hop's gap is the old arm's and each later one 2 longer.
//!
//! PLANT (regression): evo_hop_at_once -> `every_hop_after_the_first_waits_two_ticks` red; evo_first_hop_unwaited ->
//! `under_client16402_every_hop_the_first_hop_waits_too` red.
//!   RUSTFLAGS='--cfg clash_plant="evo_hop_at_once"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   evo_chain_hop_wait
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, EvoChainHopWait};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The ticks of the chain's first four hits.
fn hit_ticks(arm: EvoChainHopWait) -> Vec<u32> {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["ElectroDragon".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.evo_chain_hop_wait = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    let at = n(14650, 15000);
    s.spawn_unit(Team::Blue, "ElectroDragon_EV1", at, None).expect("the Electro Dragon");
    s.tick();
    let ed = find_live(&s, Team::Blue, "ElectroDragon_EV1").first().expect("the Electro Dragon").id;
    let spots = [("Knight", n(14650, 18499)), ("IceGolemite", n(13449, 18499)), ("Valkyrie", n(10500, 19499))];
    let reds: Vec<(EntityId, Vec2)> = spots.iter().map(|(name, p)| (s.scenario_spawn_now(Team::Red, name, *p, None).expect("a red unit"), *p)).collect();
    let mut ticks = Vec::new();
    for _ in 0..320 {
        assert!(s.debug_set_pos(ed, at));
        let tops: Vec<i32> = reds
            .iter()
            .map(|(id, p)| {
                assert!(s.debug_set_pos(*id, *p));
                let top = s.entity(*id).expect("a red unit").max_hp;
                assert!(s.debug_set_hp(*id, top));
                top
            })
            .collect();
        s.tick();
        if reds.iter().zip(tops).any(|((id, _), top)| s.entity(*id).expect("a red unit").hp < top) {
            ticks.push(s.tick_count());
        }
        if ticks.len() >= 4 {
            break;
        }
    }
    assert_eq!(ticks.len(), 4, "the scene drifted: fewer than four hits");
    ticks
}

/// Plant: evo_hop_at_once.
#[test]
fn every_hop_after_the_first_waits_two_ticks() {
    let gaps = |t: &[u32]| -> Vec<u32> { t.windows(2).map(|w| w[1] - w[0]).collect() };
    let old = gaps(&hit_ticks(EvoChainHopWait::AtOnce));
    let new = gaps(&hit_ticks(EvoChainHopWait::Client15535TwoTicks));
    assert_eq!(new, vec![old[0], old[1] + 2, old[2] + 2], "the gaps between the first four hits, client15535_two_ticks against at_once {old:?}");
}

#[test]
fn the_shipped_arm_hops_at_once() {
    assert_eq!(Calib::shipped().evo_chain_hop_wait, EvoChainHopWait::AtOnce);
}

/// combat.EVO_CHAIN_HOP_WAIT = client16402_every_hop (client 16.402, parity's r63 census: each strong hop 1 + ceil(d / 2000)
/// ticks after the last hit, the first included): each of the first three gaps 2 longer than at_once's. Plant:
/// evo_first_hop_unwaited.
#[test]
fn under_client16402_every_hop_the_first_hop_waits_too() {
    let gaps = |t: &[u32]| -> Vec<u32> { t.windows(2).map(|w| w[1] - w[0]).collect() };
    let old = gaps(&hit_ticks(EvoChainHopWait::AtOnce));
    let new = gaps(&hit_ticks(EvoChainHopWait::Client16402EveryHop));
    assert_eq!(new, vec![old[0] + 2, old[1] + 2, old[2] + 2], "the gaps between the first four hits, client16402_every_hop against at_once {old:?}");
}
