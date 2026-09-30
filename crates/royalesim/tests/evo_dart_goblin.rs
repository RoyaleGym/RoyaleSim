//! THE EVO DART GOBLIN (tools/extract_cards.py `dart_poison_block`; card.rs `DartPoisonDef`, EVO_DART_POISON; state.rs
//! EvoBoard `poisons`, `dart_hit`, `poison_pass`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; Oracle's sp-f4-dart-s0 and sp-f4-dart2-s0): the first poison 25 ticks after the
//! first dart, then every 20: 64, then 128 from the 4th dart and 307 from the 7th; a unit 1000 beside the darted one
//! lost the same on the same ticks.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_dart_goblin`):
//!   - dart_poison_never -> `his_darts_poison_their_target_and_its_neighbour_every_20_ticks_by_the_stack` red;
//!   - dart_poison_target_only -> `his_darts_poison_their_target_and_its_neighbour_every_20_ticks_by_the_stack` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

#[test]
fn his_darts_poison_their_target_and_its_neighbour_every_20_ticks_by_the_stack() {
    // The form at (4000, 12500), blue's princess towers down; a red Golem held 5000 ahead (his target) and a red Ice Golem
    // held 1000 beside it, both topped up: the Golem's first loss (a dart) and the Ice Golem's losses (poison alone).
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["BlowdartGoblin".into(), "Knight".into()], vec!["Knight".into()]];
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
    let at = n(4000, 12500);
    s.spawn_unit(Team::Blue, "BlowdartGoblin_EV1", at, None).expect("the Dart Goblin");
    s.tick();
    let dg = find_live(&s, Team::Blue, "BlowdartGoblin_EV1").first().expect("the Dart Goblin").id;
    let (gat, iat) = (n(4000, 17500), n(5000, 17500));
    let g = s.scenario_spawn_now(Team::Red, "Golem", gat, None).expect("a red Golem");
    let ig = s.scenario_spawn_now(Team::Red, "IceGolemite", iat, None).expect("a red Ice Golem");
    let (mut first_dart, mut poison) = (None, Vec::new());
    for k in 0..400usize {
        assert!(s.debug_set_pos(dg, at) && s.debug_set_pos(g, gat) && s.debug_set_pos(ig, iat));
        let (gt, it) = (s.entity(g).expect("the Golem").max_hp, s.entity(ig).expect("the Ice Golem").max_hp);
        assert!(s.debug_set_hp(g, gt) && s.debug_set_hp(ig, it));
        s.tick();
        if first_dart.is_none() && s.entity(g).expect("the Golem").hp < gt {
            first_dart = Some(k);
        }
        let lost = it - s.entity(ig).expect("the Ice Golem").hp;
        if lost > 0 {
            poison.push((k, lost));
        }
    }
    let d0 = first_dart.expect("a dart lands");
    assert!(poison.len() >= 8, "poison on the neighbour: {poison:?}");
    assert_eq!(poison[0], (d0 + 25, 64), "the first poison 25 ticks after the first dart: {d0} {poison:?}");
    assert!(poison.windows(2).all(|w| w[1].0 - w[0].0 == 20), "every 20 ticks: {poison:?}");
    let amounts: Vec<i32> = poison.iter().map(|p| p.1).collect();
    assert!(amounts.windows(2).all(|w| w[1] >= w[0]) && amounts.contains(&128) && amounts.last() == Some(&307), "64, 128, 307: {amounts:?}");
}
