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

/// combat.TOWER_TROOP_DAMAGE_LADDER = client16402_card_ladder (client 16.402, parity's r63 census: the Duchess's dagger 171
/// at tower level 16 = 42 x 409 %, the Cannoneer's 511 = 125 x 409 %, the Chef's on the tower ladder): at tower level 16 a
/// Dagger Duchess tower's damage is its record's card-ladder value, a Chef's the tower ladder's; tower_ladder (the vacuity
/// check) scales the Duchess on the tower ladder. Plant: tower_troop_damage_tower_ladder.
#[test]
fn the_duchess_and_the_cannoneer_hit_on_their_card_ladder() {
    use royalesim::state::TowerTroopDamageLadder as L;
    // The damage of the first shot Blue's left tower fires at a red Golem held in its reach, and the record's card-ladder
    // value at 16.
    let damage = |troop: &str, arm: L| -> (i32, i32) {
        let mut cfg = config();
        cfg.tower_troops = [Some(troop.to_string()), None];
        cfg.tower_level = [16, 16];
        cfg.card_level = [16, 16];
        cfg.calib.tower_troop_damage_ladder = arm;
        let mut s = BattleState::try_new(0, cfg).unwrap_or_else(|e| panic!("{troop}: {e}"));
        let tid = s.entities().find(|e| e.team == Team::Blue && e.pos == n((3500, 6500))).map(|e| e.id).expect("Blue's left tower");
        let at = n((3500, 12000));
        let golem = s.scenario_spawn_now(Team::Red, "Golem", at, None).expect("a red Golem");
        let mut shot = None;
        for _ in 0..240 {
            assert!(s.debug_set_pos(golem, at));
            s.tick();
            if let Some(q) = s.projectiles().iter().find(|q| q.firer == Some(tid)) {
                shot = Some(q.damage);
                break;
            }
        }
        let card = s.cards().index(troop).expect("the troop's record");
        (shot.expect("the tower never fired"), s.cards().scaled(card, 16, s.cards().get(card).damage).expect("the card ladder at 16"))
    };
    for troop in ["DaggerDuchess", "Cannoneer"] {
        let (got, card_ladder) = damage(troop, L::Client16402CardLadder);
        assert_eq!(got, card_ladder, "{troop}: its damage on its card ladder at 16");
        let (old, _) = damage(troop, L::TowerLadder);
        assert_ne!(old, card_ladder, "{troop}: tower_ladder gives the card ladder's value too (the vacuity check)");
    }
    let (chef, chef_card) = damage("ChefTower", L::Client16402CardLadder);
    let (chef_old, _) = damage("ChefTower", L::TowerLadder);
    assert_eq!(chef, chef_old, "the Chef's damage stays on the tower ladder");
    assert_ne!(chef, chef_card, "the scene: the Chef's two ladders part at 16");
}
