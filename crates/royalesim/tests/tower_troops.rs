//! CROWN TOWER TROOPS (card.rs `CardDb::load_tower_troops`, TOWER_TROOPS; state.rs `BattleConfig::tower_troops`): a side's
//! princess towers made with the Dagger Duchess's, the Cannoneer's or the Chef's record, the PrincessTower's own tower
//! record with the troop's attack. Measured on client 16.402 (the live population, parity's r62 item G): 48 of 1,084
//! tower sides field one (Cannoneer 20, Duchess 18, Chef 10), told apart by max_hp (1270 / 1200 / 1240 against 1400 on
//! the tower ladder); the engine shot as a Princess, 50 every 16 ticks, where the Duchess threw 42 every ~9.5 and the
//! Cannoneer fired 125 every 44: 28 first divergences (the tower troop shot the first diverging unit).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! tower_troops`): tower_troops_unloaded -> `a_side_fields_its_tower_troop` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::Team;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Blue fielding `troop` (None: the Princess) at tower level 11: Blue's left tower's record name and hitpoints, and the
/// ticks of its first shots at a red Golem held 5500 ahead of it (in its reach, too big to die).
fn left_tower(troop: Option<&str>) -> (String, i32, Vec<u32>) {
    let mut cfg = config();
    cfg.tower_level = [11, 11];
    cfg.card_level = [11, 11];
    cfg.tower_troops = [troop.map(str::to_string), None];
    let mut s = BattleState::try_new(0, cfg).expect("the battle");
    let (tid, card, max_hp) = s
        .entities()
        .find(|e| e.team == Team::Blue && e.pos == n((3500, 6500)))
        .map(|e| (e.id, e.card.to_string(), e.max_hp))
        .expect("Blue's left tower");
    let at = n((3500, 12000));
    let golem = s.scenario_spawn_now(Team::Red, "Golem", at, None).expect("a red Golem");
    let mine = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(tid)).count();
    let mut known = mine(&s);
    let mut out = Vec::new();
    for _ in 0..240 {
        if let Some(e) = s.entity(golem) {
            let full = e.max_hp;
            assert!(s.debug_set_pos(golem, at));
            assert!(s.debug_set_hp(golem, full));
        }
        s.tick();
        let m = mine(&s);
        if m > known {
            out.push(s.tick_count() - 1);
        }
        known = m;
    }
    (card, max_hp, out)
}

fn gaps(t: &[u32]) -> Vec<u32> {
    t.windows(2).map(|w| w[1] - w[0]).collect()
}

/// Plant: tower_troops_unloaded.
#[test]
fn a_side_fields_its_tower_troop() {
    let (card, princess_hp, t) = left_tower(None);
    assert_eq!(card, "PrincessTower", "no troop: the Princess");
    let g = gaps(&t);
    assert!(g.len() >= 3 && g[1..3] == [16, 16], "the Princess shoots every 16 ticks (800 ms): {t:?}");
    for (troop, hp_base, gap) in [("DaggerDuchess", 1270, 10), ("Cannoneer", 1200, 44), ("ChefTower", 1240, 20)] {
        let (card, hp, t) = left_tower(Some(troop));
        assert_eq!(card, troop, "Blue's princess towers are made with the {troop}'s record");
        // the crown tower ladder scales the troop's hitpoints as it scales the Princess's 1400
        assert_eq!(hp, princess_hp * hp_base / 1400, "{troop}: hitpoints {hp} at tower level 11 (the Princess {princess_hp})");
        let g = gaps(&t);
        assert!(g.len() >= 2 && g[1] == gap, "{troop} shoots every {gap} ticks: {t:?}");
    }
}

#[test]
fn a_tower_troop_that_is_not_a_record_is_refused() {
    let mut cfg = config();
    cfg.tower_troops = [None, Some("Knight".into())];
    assert!(BattleState::try_new(0, cfg).is_err(), "a troop that is not a tower troop record is refused");
}
