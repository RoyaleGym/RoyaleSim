//! THE EVO ELECTRO DRAGON (tools/extract_cards.py `evo_chain_block`; card.rs `EvoChainDef`; combat.rs `EvoHop`,
//! `chain_next_remember`; state.rs `evo_after_fire`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; Oracle's sp-f4-ed-s0 and sp-f4-ed3-s0): 192 on his target and the next two
//! hits, then 64 on every hop without end; a Knight, an Ice Golem 1200 from it and a Valkyrie 3100 from the Ice Golem
//! and 4260 from the Knight took K, I, V, I, K, I, V; his next shot a new chain.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_electro_dragon`):
//!   - evo_chain_never -> `his_chain_hops_by_its_last_two_hits_without_end_and_his_next_shot_starts_another` red;
//!   - evo_chain_no_repeat -> the same red.
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

#[test]
fn his_chain_hops_by_its_last_two_hits_without_end_and_his_next_shot_starts_another() {
    // Oracle's sp-f4-ed3-s0 laid out: the form held at (14650, 15000), 3499 from a red Knight at (14650, 18499) (his
    // target: the closest); a red Ice Golem 1200 from the Knight, a red Valkyrie 3100 from the Ice Golem and 4260 from the
    // Knight; all held and topped up. Blue's princess towers down (no crown tower reaches the scene).
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["ElectroDragon".into(), "Knight".into()], vec!["Knight".into()]];
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
    let at = n(14650, 15000);
    s.spawn_unit(Team::Blue, "ElectroDragon_EV1", at, None).expect("the Electro Dragon");
    s.tick();
    let ed = find_live(&s, Team::Blue, "ElectroDragon_EV1").first().expect("the Electro Dragon").id;
    let spots = [("Knight", n(14650, 18499)), ("IceGolemite", n(13449, 18499)), ("Valkyrie", n(10500, 19499))];
    let reds: Vec<(char, EntityId, Vec2)> = spots
        .iter()
        .map(|(name, p)| (name.chars().next().unwrap_or('?'), s.scenario_spawn_now(Team::Red, name, *p, None).expect("a red unit"), *p))
        .collect();
    let mut hits: Vec<(char, i32)> = Vec::new();
    for _ in 0..320 {
        assert!(s.debug_set_pos(ed, at));
        let tops: Vec<i32> = reds
            .iter()
            .map(|(_, id, p)| {
                assert!(s.debug_set_pos(*id, *p));
                let top = s.entity(*id).expect("a red unit").max_hp;
                assert!(s.debug_set_hp(*id, top));
                top
            })
            .collect();
        s.tick();
        for ((c, id, _), top) in reds.iter().zip(tops) {
            let lost = top - s.entity(*id).expect("a red unit").hp;
            if lost > 0 {
                hits.push((*c, lost));
            }
        }
        if hits.len() >= 24 {
            break;
        }
    }
    let order: String = hits.iter().take(7).map(|h| h.0).collect();
    assert_eq!(order, "KIVIKIV", "the chain's hits: {hits:?}");
    let first: Vec<i32> = hits.iter().take(7).map(|h| h.1).collect();
    assert_eq!(first, [192, 192, 192, 64, 64, 64, 64], "the chain's damage: {hits:?}");
    // His next shot starts a new chain: three strong hits in a row, the old chain's hops gone.
    let next = hits.iter().skip(3).position(|h| h.1 == 192).map(|k| k + 3).expect("his next shot");
    assert!(hits.len() >= next + 3 && hits[next..next + 3].iter().all(|h| h.1 == 192), "a new chain at his next shot: {hits:?}");
}
