//! spells.CROWN_SQUARE_HALF_SIDE: the half-side of a crown tower's square (spell.rs `crown_square_half`), read where
//! spells.CROWN_TOWER_SPELL_REACH and combat.TROOP_SPLASH_CROWN_REACH reach a crown tower by its square.
//!
//! THE LAW, measured on client 16.635 (parity r97, the oracle's il-mg-16635 set): every spell cast landing between the enemy
//! King's 1000-square and 1400-square reach took the King, 3 of 3 (a Snowball, a Zap and a Fireball 2550 from the 1000-square
//! and 2102 from the 1400-square), on the tick the same cast took the princess tower in both engines.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! crown_square_half_side`):
//!   * `king_square_1000` -- the new arm keeps the fixed 1000: both tests go red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, CrownSquareHalfSide};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn king(s: &BattleState, team: Team) -> EntityId {
    s.entities().find(|e| e.team == team && e.card == "KingTower").map(|e| e.id).expect("the King")
}

/// `spell` cast by `team` beside the enemy King, under spells.CROWN_SQUARE_HALF_SIDE = `arm`: 2550 from the King's 1000-square,
/// 2102 from its 1400-square, on a held enemy Knight (so the cast lands): (the King's loss, the Knight's loss) in 60 ticks.
fn cast(arm: CrownSquareHalfSide, spell: &str, team: Team) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.crown_square_half_side = arm;
    let mut s = BattleState::new(0, cfg);
    let foe = team.other();
    let k = king(&s, foe);
    let kp = s.entity(k).expect("the King").pos;
    let (aim, kn) = if team == Team::Red { ((12500, 1500), (12500, 2000)) } else { ((5500, 30500), (5500, 30000)) };
    let kc = if team == Team::Red { (9000, 3000) } else { (9000, 29000) };
    assert_eq!((kp.x / K, kp.y / K), kc, "the scene drifted: the King's point");
    assert_eq!(s.entity(k).expect("the King").radius / K, 1400, "the scene drifted: the King's collision radius");
    let knight = s.scenario_spawn_now(foe, "Knight", at(kn), None).expect("the Knight");
    let (k0, n0) = (s.entity(k).expect("the King").hp, s.entity(knight).expect("the Knight").max_hp);
    s.spawn_unit(team, spell, at(aim), None).expect("the cast");
    let mut nloss = 0;
    for _ in 0..60 {
        if let Some(e) = s.entity(knight) {
            nloss = nloss.max(n0 - e.hp);
            assert!(s.debug_set_pos(knight, at(kn)));
        }
        s.tick();
    }
    if let Some(e) = s.entity(knight) {
        nloss = nloss.max(n0 - e.hp);
    }
    (k0 - s.entity(k).expect("the King stands").hp, nloss)
}

/// spells.CROWN_SQUARE_HALF_SIDE = client16635_collision_radius (client 16.635, parity r97, il-mg d6fe1c0f t3431, 6bd1b5b5
/// t3371): a Zap or a Snowball 2550 from the King's 1000-square takes the King, on both seats; under fixed_1000 (the vacuity
/// check) it misses. Plant: king_square_1000.
#[test]
fn under_client16635_collision_radius_a_spell_beside_the_king_reaches_its_square() {
    for spell in ["Zap", "Snowball"] {
        for team in [Team::Red, Team::Blue] {
            let (k_old, n_old) = cast(CrownSquareHalfSide::Fixed1000, spell, team);
            assert!(n_old > 0, "the scene drifted: {spell} by {team:?} never landed on the Knight");
            assert_eq!(k_old, 0, "fixed_1000 (the vacuity check): {spell} by {team:?} took the King");
            let (k_new, _) = cast(CrownSquareHalfSide::Client16635CollisionRadius, spell, team);
            assert!(k_new > 0, "client16635_collision_radius: {spell} by {team:?} missed the King");
        }
    }
}

/// Red's engine-Left princess down; a red Knight held at (-2650, -1300) from Red's King (2951 from its centre, past the Ice
/// Wizard's splash 1500 + 1400; 1677 from its 1000-square, 1250 from its 1400-square) and a blue Ice Wizard held 4000 south of
/// it, both at full hitpoints, under spells.CROWN_SQUARE_HALF_SIDE = `arm` (TROOP_SPLASH_CROWN_REACH as shipped): the King's
/// loss in 120 ticks.
fn splash_beside_king(arm: CrownSquareHalfSide) -> i32 {
    let mut cfg = config();
    cfg.calib.crown_square_half_side = arm;
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tower_hp(Team::Red, 1, 0).expect("Red's engine-Left princess down");
    let k = king(&s, Team::Red);
    let (kat, wat) = (at((6350, 27700)), at((6350, 23700)));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let wiz = s.scenario_spawn_now(Team::Blue, "IceWizard", wat, None).expect("the Ice Wizard");
    let k0 = s.entity(k).expect("the King").hp;
    for t in 0..120u32 {
        for (id, p) in [(knight, kat), (wiz, wat)] {
            if let Some(e) = s.entity(id) {
                let m = e.max_hp;
                assert!(s.debug_set_pos(id, p) && s.debug_set_hp(id, m));
            }
        }
        s.tick();
        if t == 30 {
            assert_eq!(s.entity(wiz).expect("the Ice Wizard").target, Some(knight), "the scene drifted: the Wizard's target");
        }
    }
    k0 - s.entity(k).expect("the King stands").hp
}

/// spells.CROWN_SQUARE_HALF_SIDE = client16635_collision_radius with combat.TROOP_SPLASH_CROWN_REACH = client16635_crown_square_strict:
/// an Ice Wizard's splash 1250 from the King's 1400-square takes the King; under fixed_1000 (the vacuity check) it does not.
/// Plant: king_square_1000.
#[test]
fn under_client16635_collision_radius_an_ice_wizard_splash_beside_the_king_reaches_it() {
    assert_eq!(splash_beside_king(CrownSquareHalfSide::Fixed1000), 0, "fixed_1000 (the vacuity check): the King lost hitpoints");
    assert!(splash_beside_king(CrownSquareHalfSide::Client16635CollisionRadius) > 0, "client16635_collision_radius: the King lost none");
}
