//! THE EVO EXECUTIONER (tools/extract_cards.py `axe_block`; card.rs `AxeDef`; combat.rs `straight_hits`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; Oracle's sp-axe-* grid and sp-form-AxeMan-evo-s0): his axe hits 240 a victim
//! whose edge is within 2500 of his and 179 beyond, out and back alike (a Golem strong at 3834 from his centre, normal
//! at 3888); a strong hit on the way out pushes its victim about 820 away from him over 6 ticks, the back hit does not.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_executioner`):
//!   - axe_strong_never -> every test red;
//!   - axe_strong_by_centre -> `the_axe_is_strong_on_a_victim_whose_edge_is_within_2500_of_his` red;
//!   - axe_push_never -> `a_strong_hit_on_the_way_out_pushes_its_victim_away_and_the_back_hit_does_not` red;
//!   - axe_push_both_legs -> `a_strong_hit_on_the_way_out_pushes_its_victim_away_and_the_back_hit_does_not` red.
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

/// Where he stands: clear of blue's king's reach once blue's princess towers are down.
const AT: (i32, i32) = (4000, 12500);

/// Blue's princess towers down, the form put down on AT and a red `victim` `d` to his right: the battle, him and it.
fn battle(victim: &str, d: i32) -> (BattleState, EntityId, EntityId) {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["AxeMan".into(), "Knight".into()], vec!["Knight".into()]];
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
    s.spawn_unit(Team::Blue, "AxeMan_EV1", n(AT.0, AT.1), None).expect("the Executioner");
    let v = s.scenario_spawn_now(Team::Red, victim, n(AT.0 + d, AT.1), None).expect("the victim");
    s.tick();
    let ax = find_live(&s, Team::Blue, "AxeMan_EV1").first().expect("the Executioner").id;
    (s, ax, v)
}

fn hp(s: &BattleState, v: EntityId) -> i32 {
    s.entity(v).expect("the victim").hp
}

#[test]
fn the_axe_is_strong_on_a_victim_whose_edge_is_within_2500_of_his() {
    // His radius 600 and a Golem's 750: strong to 3850 from his centre. Both held; the first throw's two hits.
    for (d, amount) in [(2000, 240), (3800, 240), (3900, 179), (5500, 179)] {
        let (mut s, ax, g) = battle("Golem", d);
        let (at, gat) = (n(AT.0, AT.1), n(AT.0 + d, AT.1));
        let (mut got, mut last) = (Vec::new(), hp(&s, g));
        for _ in 0..150 {
            assert!(s.debug_set_pos(ax, at) && s.debug_set_pos(g, gat));
            s.tick();
            let h = hp(&s, g);
            if h < last {
                got.push(last - h);
            }
            last = h;
        }
        assert!(got.len() >= 2 && got[..2].iter().all(|a| *a == amount), "a Golem {d} away: {got:?}");
    }
}

#[test]
fn a_strong_hit_on_the_way_out_pushes_its_victim_away_and_the_back_hit_does_not() {
    // A Knight 2000 to his right, held until the first hit: it is more than 600 further away 6 ticks on; let walk, it
    // comes back, and the throw's back hit (strong too) moves it no further away.
    let (mut s, ax, kn) = battle("Knight", 2000);
    let (at, kat) = (n(AT.0, AT.1), n(AT.0 + 2000, AT.1));
    let x = |s: &BattleState| s.entity(kn).expect("the Knight").pos.x / K - AT.0;
    let mut last = hp(&s, kn);
    let mut out = None;
    for _ in 0..150 {
        assert!(s.debug_set_pos(ax, at) && s.debug_set_pos(kn, kat));
        s.tick();
        let h = hp(&s, kn);
        if h < last {
            out = Some(last - h);
            last = h;
            break;
        }
        last = h;
    }
    assert_eq!(out, Some(240), "the out hit");
    let (mut xs, mut back) = (vec![x(&s)], None);
    for k in 1..=60 {
        assert!(s.debug_set_pos(ax, at));
        s.tick();
        xs.push(x(&s));
        let h = hp(&s, kn);
        if h < last && back.is_none() {
            back = Some((k, last - h));
        }
        last = h;
    }
    assert!(xs[6] - 2000 > 600, "pushed away by the out hit: {:?}", &xs[..8]);
    let (b, amount) = back.expect("the back hit");
    assert_eq!(amount, 240, "the back hit");
    assert!(b + 6 < xs.len() && xs[b + 6] <= xs[b] + 100, "not pushed by the back hit: {:?}", &xs[b.saturating_sub(1)..(b + 8).min(xs.len())]);
}
