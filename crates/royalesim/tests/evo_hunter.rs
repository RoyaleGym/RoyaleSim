//! THE EVO HUNTER (tools/extract_cards.py `net_block`; card.rs `NetDef`, NET_TARGET_HOLD_TICKS; state.rs EvoBoard `nets`,
//! `net_pass`, `throw_net`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; Oracle's sp-f4-hunter*-s0): his net 9 ticks after he takes a target already in
//! his net's reach (held 5, cast 4), 105 ticks throw to throw on one target; what it lands on held still 3000 ms.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_hunter`):
//!   - net_never -> every test red;
//!   - net_without_hold -> `his_net_comes_9_ticks_after_he_takes_a_target_in_reach_then_every_105` red;
//!   - net_snare_dropped -> `what_his_net_lands_on_stands_still_3000_ms` red;
//!   - net_at_buildings -> `a_building_he_holds_takes_no_net` red.
//!
//! His net action's TargetFilter is default_character_targets_no_buildings (item 310): a building he holds, a crown tower
//! included, takes no net (client 15.535.29, sp-f4-hunterG80-s0 t1451: the princess tower he shot shot on unstopped).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const AT: (i32, i32) = (4000, 12500);

/// Blue's princess towers down, the form put down on AT and let deploy (and his net's InitialCooldown run): the battle
/// and him.
fn battle() -> (BattleState, EntityId) {
    let mut cfg: BattleConfig = config();
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
    let h = find_live(&s, Team::Blue, "Hunter_EV1").first().expect("the Hunter").id;
    for _ in 0..60 {
        assert!(s.debug_set_pos(h, n(AT.0, AT.1)));
        s.tick();
    }
    (s, h)
}

/// The nets in flight now (a no-damage shot of his carrying a buff).
fn nets(s: &BattleState, h: EntityId) -> usize {
    s.projectiles().iter().filter(|p| p.firer == Some(h) && p.damage == 0 && p.buff.is_some()).count()
}

#[test]
fn his_net_comes_9_ticks_after_he_takes_a_target_in_reach_then_every_105() {
    // A red Golem put down 4000 ahead (2650 edge to edge) and held there, topped up: the tick he takes it and each throw.
    let (mut s, h) = battle();
    let gat = n(AT.0, AT.1 + 4000);
    let g = s.scenario_spawn_now(Team::Red, "Golem", gat, None).expect("a red Golem");
    let (mut taken, mut throws, mut flying) = (None, Vec::new(), 0usize);
    for k in 0..260 {
        assert!(s.debug_set_pos(h, n(AT.0, AT.1)) && s.debug_set_pos(g, gat));
        let top = s.entity(g).expect("the Golem").max_hp;
        assert!(s.debug_set_hp(g, top));
        s.tick();
        if taken.is_none() && s.entity(h).expect("the Hunter").target == Some(g) {
            taken = Some(k);
        }
        let now = nets(&s, h);
        if now > flying {
            throws.push(k);
        }
        flying = now;
    }
    let t0 = taken.expect("he takes the Golem");
    assert!(throws.len() >= 3, "three nets: {throws:?}");
    assert_eq!(throws[0] - t0, 9, "the first net 9 ticks after he takes it: {t0} {throws:?}");
    assert!(throws.windows(2).all(|w| w[1] - w[0] == 105), "105 ticks throw to throw: {throws:?}");
}

#[test]
fn what_his_net_lands_on_stands_still_3000_ms() {
    // A red Knight put down 4500 ahead walks in; from the tick his first net lands (the net gone) it stands still.
    let (mut s, h) = battle();
    let k = s.scenario_spawn_now(Team::Red, "Knight", n(AT.0, AT.1 + 4500), None).expect("a red Knight");
    let (mut thrown, mut landed) = (false, None);
    let mut pts = Vec::new();
    for t in 0..200 {
        assert!(s.debug_set_pos(h, n(AT.0, AT.1)));
        let top = s.entity(k).expect("the Knight").max_hp;
        assert!(s.debug_set_hp(k, top));
        s.tick();
        let e = s.entity(k).expect("the Knight");
        pts.push(e.pos);
        let now = nets(&s, h) > 0;
        if now {
            thrown = true;
        } else if thrown && landed.is_none() {
            landed = Some(t);
        }
    }
    let l = landed.expect("the net lands");
    assert!(pts[l + 1..=l + 58].iter().all(|p| *p == pts[l + 1]), "held from the landing: {:?}", &pts[l..l + 4]);
    assert!(pts[l + 65] != pts[l + 1], "free again after 3000 ms");
}

/// Plant: net_at_buildings. A red Cannon put down 3000 ahead (in his net's reach) and held, both topped up, no troop: he
/// takes it and shoots it, and throws no net at it.
#[test]
fn a_building_he_holds_takes_no_net() {
    let (mut s, h) = battle();
    let cat = n(AT.0, AT.1 + 3000);
    let c = s.scenario_spawn_now(Team::Red, "Cannon", cat, None).expect("a red Cannon");
    let (mut held, mut thrown) = (0, 0);
    for _ in 0..240 {
        assert!(s.debug_set_pos(h, n(AT.0, AT.1)) && s.debug_set_pos(c, cat));
        let top = s.entity(c).expect("the Cannon").max_hp;
        let his = s.entity(h).expect("the Hunter").max_hp;
        assert!(s.debug_set_hp(c, top) && s.debug_set_hp(h, his));
        s.tick();
        if s.entity(h).expect("the Hunter").target == Some(c) {
            held += 1;
        }
        thrown += nets(&s, h);
    }
    assert!(held > 150, "the scene drifted: he held the Cannon {held} ticks of 240");
    assert_eq!(thrown, 0, "a net in flight on {thrown} ticks, at a building");
}
