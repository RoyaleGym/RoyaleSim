//! THE PROTOCOL'S STATUS BITS 3 AND 4 (state.rs `view`; card.rs `is_evo_record`, `is_hero_body`): what each marks,
//! read from the row's place in the card table.
//!
//!   - bit 3 (8), an evolved unit: every row an evolution pushes of its own (the form, its members, summons and death
//!     spawns, a fall's grounded row), never a row the plain card shares;
//!   - bit 4 (16), a hero's body: the hero form's own row and the rows its button makes of that body (the Hero
//!     Wizard's lift, the Hero Bowler's siege, the Hero Dark Prince's walk) or releases as the hero (the Hero Barbarian
//!     Barrel's Barbarian); never a champion, never what a hero puts down beside itself.
//!
//! Before, the bits read the row's blocks (`evo`, `ability`): a champion carried bit 4 for its whole life, a hero lost
//! it on its lifted or siege row, and an evolution's own summons (the Evo Royal Ghost's pair) carried no bit 3.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! status_bits`):
//!   * `status_bits_from_blocks` -- the export reads the row's blocks again: `a_champion_on_the_board_carries_no_hero_bit`
//!     goes red here, and so do the bit checks in tests/fall_grounding.rs, tests/hero_wizard.rs and
//!     tests/hero_bowler.rs. The table checks here ask card.rs directly and stay green.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{AbilityEffect, CardDb};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn db() -> CardDb {
    BattleState::new(7, config()).cards().clone()
}

fn idx(db: &CardDb, name: &str) -> u16 {
    db.index(name).filter(|&i| db.get(i).name == name).unwrap_or_else(|| panic!("no row {name}"))
}

#[test]
fn every_hero_form_is_a_body_and_nothing_it_puts_down_is() {
    let db = db();
    assert!(db.hero_forms.len() >= 10, "the table loads its hero forms: {}", db.hero_forms.len());
    let (mut bodies, mut beside) = (0, 0);
    for &(_, f) in &db.hero_forms {
        let name = db.get(f).name.clone();
        assert!(db.is_hero_body(f), "{name}: the form's own row");
        assert!(!db.is_evo_record(f), "{name}: a hero form is no evolution's");
        let (body, side): (Vec<u16>, Vec<u16>) = match db.get(f).ability.as_ref().map(|a| &a.effect) {
            Some(AbilityEffect::GroundToAir { unit, .. }) => (vec![*unit], vec![]),
            Some(AbilityEffect::Siege(sg)) => (vec![sg.unit], vec![]),
            Some(AbilityEffect::ReRoll(r)) => (vec![r.unit], vec![]),
            Some(AbilityEffect::Dismount(d)) => (vec![d.walker], vec![d.mount]),
            Some(AbilityEffect::FlagSpawns(fl)) => (vec![], std::iter::once(fl.flag).chain(fl.spawns.iter().map(|s| s.unit)).collect()),
            Some(AbilityEffect::TombMonster(t)) => (vec![], vec![t.passive, t.active]),
            Some(AbilityEffect::SpawnAhead { unit, .. } | AbilityEffect::Throw { unit, .. }) => (vec![], vec![*unit]),
            Some(AbilityEffect::DecoyWarp(d)) => (vec![], vec![d.decoy]),
            _ => (vec![], vec![]),
        };
        for u in body {
            assert!(db.is_hero_body(u), "{name}: {} is the hero's body", db.get(u).name);
            bodies += 1;
        }
        for u in side {
            assert!(!db.is_hero_body(u), "{name}: {} is put down beside the hero", db.get(u).name);
            beside += 1;
        }
    }
    assert!(bodies >= 3 && beside >= 4, "the table holds bodies ({bodies}) and things beside them ({beside}) to check");
}

#[test]
fn no_champion_is_a_hero_body() {
    let db = db();
    let champions: Vec<u16> = (0..db.cards.len() as u16).filter(|&i| db.is_champion(i)).collect();
    assert!(champions.len() >= 4, "the table loads its champions: {}", champions.len());
    for c in champions {
        assert!(db.get(c).ability.is_some(), "{}: a champion carries its button", db.get(c).name);
        assert!(!db.is_hero_body(c), "{}: a champion is no hero", db.get(c).name);
    }
}

#[test]
fn an_evolutions_own_rows_are_evolved_and_a_shared_row_is_not() {
    let db = db();
    assert!(db.forms.len() >= 20, "the table loads its evolutions: {}", db.forms.len());
    for &(base, _, form) in &db.forms {
        assert!(db.is_evo_record(form), "{}: the form's own row", db.get(form).name);
        assert!(!db.is_evo_record(base), "{}: the base card's row", db.get(base).name);
    }
    // Rows of an evolution's own that carry no EvoDef: the reason the bit reads the table, not the row's blocks.
    for name in ["RoyalHog_EV1_Grounded", "Ghost_EV1_Summon_Left", "Ghost_EV1_Summon_Right", "GoblinCage_EV1_GoblinBrawler"] {
        let i = idx(&db, name);
        assert!(db.is_evo_record(i), "{name}: an evolution's own row");
    }
    assert!(db.get(idx(&db, "Ghost_EV1_Summon_Left")).evo.is_none(), "the pair carries no EvoDef");
    // Rows the plain cards share.
    for name in ["Goblin", "Skeleton", "GoblinBrawler"] {
        if let Some(i) = db.index(name).filter(|&i| db.get(i).name == name) {
            assert!(!db.is_evo_record(i), "{name}: a row the plain card shares");
        }
    }
}

#[test]
fn a_champion_on_the_board_carries_no_hero_bit() {
    let mut s = BattleState::new(7, config());
    past_deploy_lockout(&mut s);
    for (k, name) in ["GoldenKnight", "ArcherQueen"].into_iter().enumerate() {
        s.spawn_unit(Team::Blue, name, n(3500 + 4000 * k as i32, 8000), None).unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
    s.tick();
    for name in ["GoldenKnight", "ArcherQueen"] {
        let e = find_live(&s, Team::Blue, name);
        assert_eq!(e.len(), 1, "{name} on the board");
        assert_eq!(e[0].status_flags & 24, 0, "{name}: neither evolved nor a hero (status {})", e[0].status_flags);
    }
}

#[test]
fn status_bits_name_every_bit_the_export_sets() {
    // py.rs STATUS_BITS: name k is bit k. The values the export sets (entity.rs `status_flags`, state.rs `view`).
    let want = [
        ("underground", 1),
        ("invisible", 2),
        ("hidden", 4),
        ("evolved", 8),
        ("hero", 16),
        ("clone", 32),
        ("ability_windup", 64),
        ("ability_active", 128),
        ("charged", 256),
        ("grounded", 512),
    ];
    let got: Vec<(&str, i32)> = royalesim::py::STATUS_BITS.iter().enumerate().map(|(k, n)| (*n, 1 << k)).collect();
    assert_eq!(got, want.to_vec());
}
