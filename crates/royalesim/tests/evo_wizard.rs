//! THE EVO WIZARD (card.rs `EvoDef::shield_blast`; spell.rs `shape_of`; combat.rs the resolve's `shield_broke`; state.rs
//! `shield_blasts`), at level 11.
//!
//! Read off the table (characters_evo Wizard_EV1; area_effect_objects_evo Wizard_EV1_ShieldLostExplosion), not measured
//! (the client's scene never broke the shield): the form's ShieldHitpoints 75, and the tick its shield goes to 0 a blast
//! on its point: Damage 110 on its ladder, Radius 3000, air and ground, enemies, Pushback 3000.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_wizard`):
//!   - shield_blast_never -> `its_shields_loss_blasts_3000_around_it` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// On blue's side short of the river: the Knights 2500 and 3600 off stand over 8200 from both blue princess towers and
/// out of every red tower's reach.
const AT: (i32, i32) = (9000, 14500);

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Wizard".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

#[test]
fn its_shields_loss_blasts_3000_around_it() {
    // The evolved Wizard put down (still deploying: it fires nothing), a red Knight 2500 off and one 3600 off, held. Its
    // shield is 75 on its ladder; set to 1, a red Zap takes it to 0, and the blast takes 110 on the Wizard's ladder off
    // the near Knight and nothing off the far one (3000 + its 500 short of it).
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Wizard_EV1", n(AT.0, AT.1), None).expect("the Wizard");
    let near = s.scenario_spawn_now(Team::Red, "Knight", n(AT.0 + 2500, AT.1), None).expect("a red Knight");
    let far = s.scenario_spawn_now(Team::Red, "Knight", n(AT.0 - 3600, AT.1), None).expect("a red Knight");
    s.tick();
    let wiz = find_live(&s, Team::Blue, "Wizard_EV1")[0].id;
    let db = s.cards().clone();
    let form = db.index("Wizard_EV1").expect("the form");
    assert_eq!(s.entity(wiz).expect("the Wizard").shield, db.scaled(form, 11, 75).unwrap(), "75 on its ladder");
    let blast = db.scaled(form, 11, 110).unwrap();
    let top = |s: &BattleState, id| s.entity(id).expect("a Knight").hp;
    let (n0, f0) = (top(&s, near), top(&s, far));
    assert!(s.debug_set_shield(wiz, 1));
    let at = s.entity(wiz).expect("the Wizard").pos;
    s.spawn_unit(Team::Red, "Zap", at, None).expect("a Zap");
    for _ in 0..4 {
        assert!(s.debug_set_pos(near, n(AT.0 + 2500, AT.1)));
        assert!(s.debug_set_pos(far, n(AT.0 - 3600, AT.1)));
        s.tick();
    }
    assert_eq!(s.entity(wiz).expect("the Wizard").shield, 0, "the Zap took the shield");
    assert_eq!(n0 - top(&s, near), blast, "the blast on the near Knight");
    assert_eq!(f0 - top(&s, far), 0, "nothing on the far one");
}
