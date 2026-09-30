//! THE EVO BATS (tools/extract_cards.py `hit_rage_block`; card.rs `HitRageDef`, `BuffDef::over_heal_pct`; state.rs
//! `evo_after_fire`, `land_buff_heals`), at level 11 (122 max hitpoints: the Bat's 150 %).
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-Bats-evo-s0): a bat's hit on t879 healed it 38 on t888; a bat's hit on
//! t885 healed it 38 on t894 and 38 on t904, to 198, over its maximum.
//! Read off the table, not measured: the cap at 200 % of the maximum.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_bats`):
//!   - hit_heal_no_pulse -> both tests red;
//!   - hit_heal_a_period_out -> both tests red;
//!   - overheal_capped_at_max -> `each_hit_heals_it_38_nine_and_nineteen_ticks_on_past_its_maximum` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// One Evo Bat held at (9000, 12500) on `hp` hitpoints until its first hit on a red Knight held 1200 ahead (topped up;
/// a Knight cannot hit a flier), then let be: (the first hit's frame, its hitpoints on every frame).
fn after_a_hit(hp: i32) -> (usize, Vec<i32>) {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Bats".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    // Blue's princess towers down (its king wakes; its reach stops short of y 12000): no crown tower reaches the scene.
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    let (at, kn_at) = (n(9000, 12500), n(9000, 13700));
    s.spawn_unit(Team::Blue, "Bats_EV1", at, None).expect("the bats");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    s.tick();
    let bats: Vec<_> = find_live(&s, Team::Blue, "Bats_EV1").iter().map(|e| e.id).collect();
    let bat = bats[0];
    for b in &bats[1..] {
        assert!(s.debug_set_hp(*b, 0));
    }
    assert_eq!(s.entity(bat).expect("the bat").max_hp, 122, "the Evo Bat's 150 % of 81");
    let (mut first, mut hps) = (None, Vec::new());
    for k in 0..200 {
        assert!(s.debug_set_pos(bat, at));
        assert!(s.debug_set_pos(knight, kn_at));
        let top = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_hp(knight, top));
        if first.is_none() {
            assert!(s.debug_set_hp(bat, hp));
        }
        s.tick();
        hps.push(s.entity(bat).expect("the bat").hp);
        if first.is_none() && s.entity(knight).expect("the Knight").hp < top {
            first = Some(k);
        }
        if first.is_some_and(|f| k >= f + 23) {
            break;
        }
    }
    (first.expect("a hit on the Knight"), hps)
}

#[test]
fn each_hit_heals_it_38_nine_and_nineteen_ticks_on_past_its_maximum() {
    // BatsEV1_Heal's 30 a second: 76 at level 11, half of it every 500 ms for 1000 ms. From 100: 138 on the hit + 9,
    // 176 on the hit + 19, over its 122; its next hit is the hit + 24.
    let (h, hps) = after_a_hit(100);
    let want = |k: usize| if k < h + 9 { 100 } else if k < h + 19 { 138 } else { 176 };
    for k in h..=h + 23 {
        assert_eq!(hps[k], want(k), "frame {k} (the hit on {h}): {:?}", &hps[h..]);
    }
}

#[test]
fn the_heal_stops_at_twice_its_maximum() {
    // From 230: 244 (twice 122) on the hit + 9, and no more on the hit + 19.
    let (h, hps) = after_a_hit(230);
    assert_eq!((hps[h + 8], hps[h + 9], hps[h + 19], hps[h + 23]), (230, 244, 244, 244), "the hit on {h}: {:?}", &hps[h..]);
}
