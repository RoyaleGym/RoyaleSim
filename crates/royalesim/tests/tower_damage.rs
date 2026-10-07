//! THE CROWN TOWER'S OWN DAMAGE (16.402 tables on: an area's Damage is {BaseDamage, TowerDamage}; card.rs
//! `SpellHit::tower_damage`, spell.rs `impact`). The 160402017 client gives Zap BaseDamage 75 and TowerDamage 19 where
//! 15.535.29 gave Damage 75 and CrownTowerDamagePercent -75 (every pair across the two packs is round half up of
//! damage * share at level 1). A crown tower takes the TowerDamage scaled by the caster's level, as the damage is
//! (the Evo Cannon's barrage scales its crown damage the same way); a hit without one keeps the percent, so the
//! 15.535 tables play as before. Measured on client 16.402.19 (two Zaps and two Freezes on crown towers at level
//! 11): the tower lost 48 to each Zap and 38 to each Freeze, TowerDamage x 2.56 floored (19 -> 48.64, 15 -> 38.4);
//! no single level fits the old percent of the scaled damage.
//!
//! Each test casts Blue's Zap on Red's left princess tower at a given level, its hit's tower_damage set as the case
//! needs, and reads the tower's loss.
//!
//! PLANT: `RUSTFLAGS='--cfg clash_plant="tower_damage_unread"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//! --test tower_damage` -> `the_tower_takes_its_own_damage_scaled_by_level` and `a_zero_tower_damage_spares_the_tower`
//! red (the percent applies in place of the tower's own damage).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, SpellShape};
use royalesim::combat::damage_against;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

const PRINCESS: (i32, i32) = (3500, 25500);

fn zap_with(tower_damage: Option<i32>) -> (CardDb, u16) {
    let mut db = cards();
    let i = db.index("Zap").expect("Zap loads");
    match db.cards[i as usize].spell.as_mut().map(|s| &mut s.shape) {
        Some(SpellShape::AreaEffect { hit }) => hit.tower_damage = tower_damage,
        other => panic!("Zap is not one area: {other:?}"),
    }
    (db, i)
}

/// The hp Red's left princess tower loses to Blue's Zap cast on it at `level`.
fn tower_loss(db: CardDb, level: i32) -> i32 {
    let cfg = BattleConfig::with_cards(db);
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at = Vec2::new(PRINCESS.0 * K, PRINCESS.1 * K);
    let hp = |s: &BattleState| s.entities().find(|e| e.team == Team::Red && e.pos == at).map(|e| e.hp).expect("the tower");
    let before = hp(&s);
    s.spawn_unit(Team::Blue, "Zap", at, Some(level)).expect("the cast");
    for _ in 0..40 {
        s.tick();
    }
    before - hp(&s)
}

#[test]
fn without_a_tower_damage_the_percent_applies_as_before() {
    for level in [1, 11] {
        let (db, i) = zap_with(None);
        let SpellShape::AreaEffect { hit } = db.cards[i as usize].spell.as_ref().unwrap().shape.clone() else { unreachable!() };
        let scaled = db.scaled(i, level, hit.damage).unwrap();
        let want = damage_against(EntityKind::PrincessTower, scaled, hit.crown_pct, crown_rounding());
        assert_eq!(tower_loss(db, level), want, "level {level}: {} % of {scaled}", hit.crown_pct);
    }
}

#[test]
fn the_tower_takes_its_own_damage_scaled_by_level() {
    for level in [1, 11] {
        let (db, i) = zap_with(Some(19));
        let want = db.scaled(i, level, 19).unwrap();
        assert!(want > 0);
        assert_eq!(tower_loss(db, level), want, "level {level}: TowerDamage 19 scaled");
    }
}

#[test]
fn a_zero_tower_damage_spares_the_tower() {
    // 16.402's morph areas and the Evo Royal Giant's push carry TowerDamage 0: the tower takes nothing.
    let (db, _) = zap_with(Some(0));
    assert_eq!(tower_loss(db, 11), 0);
}

fn crown_rounding() -> royalesim::combat::CrownRounding {
    royalesim::state::Calib::shipped().crown_rounding
}
