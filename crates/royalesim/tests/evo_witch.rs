//! THE EVO WITCH (tools/extract_cards.py `soul_drain_block`; card.rs `SoulDrainDef`; state.rs EvoBoard `souls`,
//! `soul_heals`), at level 11 (839 max hitpoints).
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-Witch-evo-s0): four of her Skeletons died on t880, t900, t911 and t922
//! and she healed 153 on t901, t921, t932 and t943, to 992, 1145, 1298 and 1451 (173 % of 839, floored).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_witch`):
//!   - soul_drain_never -> `each_skeleton_she_spawned_that_dies_heals_her_153_on_its_death_plus_21` red;
//!   - overheal_capped_at_max -> the same red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn hers(s: &BattleState, w: EntityId) -> Vec<EntityId> {
    s.entities().filter(|e| e.team == Team::Blue && e.spawned_by == Some(w)).map(|e| e.id).collect()
}

/// Kill `sk` and tick: the Witch's hitpoints on the death's tick and on each of the 25 after it (her point held).
fn kill(s: &mut BattleState, w: EntityId, at: Vec2, sk: EntityId) -> Vec<i32> {
    assert!(s.debug_set_hp(sk, 0));
    let mut hp = Vec::new();
    for _ in 0..=25 {
        assert!(s.debug_set_pos(w, at));
        s.tick();
        hp.push(s.entity(w).expect("the Witch").hp);
    }
    hp
}

#[test]
fn each_skeleton_she_spawned_that_dies_heals_her_153_on_its_death_plus_21() {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Witch".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at = n(9000, 11500);
    s.spawn_unit(Team::Blue, "Witch_EV1", at, None).expect("the Witch");
    s.tick();
    let w = find_live(&s, Team::Blue, "Witch_EV1").first().expect("the Witch").id;
    let top = s.entity(w).expect("the Witch").max_hp;
    assert_eq!(top, 839, "the form's 839 at level 11");
    for _ in 0..200 {
        if hers(&s, w).len() >= 4 {
            break;
        }
        assert!(s.debug_set_pos(w, at));
        s.tick();
    }
    let first = hers(&s, w);
    assert_eq!(first.len(), 4, "her first wave");
    // A Skeleton she did not spawn heals her nothing.
    let other = s.scenario_spawn_now(Team::Blue, "Skeleton", n(12000, 11500), None).expect("a Skeleton");
    let hp = kill(&mut s, w, at, other);
    assert!(hp.iter().all(|h| *h == top), "a stranger's death healed her: {hp:?}");
    // Hers, one at a time: 153 on the death + 21, past her maximum, to 173 % of it.
    for (k, want) in [992, 1145, 1298, 1451].into_iter().enumerate() {
        let before = s.entity(w).expect("the Witch").hp;
        let hp = kill(&mut s, w, at, first[k]);
        assert!(hp[..21].iter().all(|h| *h == before), "Skeleton {k}: moved before the death + 21: {hp:?}");
        assert_eq!(hp[21], want, "Skeleton {k}: the heal on the death + 21: {hp:?}");
    }
    // At the cap a fifth (her second wave's) heals nothing more.
    for _ in 0..200 {
        if !hers(&s, w).is_empty() {
            break;
        }
        assert!(s.debug_set_pos(w, at));
        s.tick();
    }
    let next = *hers(&s, w).first().expect("her second wave");
    let hp = kill(&mut s, w, at, next);
    assert!(hp.iter().all(|h| *h == 1451), "past the cap: {hp:?}");
}
