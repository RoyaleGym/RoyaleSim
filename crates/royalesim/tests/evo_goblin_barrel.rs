//! THE EVO GOBLIN BARREL (tools/extract_cards.py `mirror_block`; card.rs `EvoDef::mirror`; state.rs `phase_spawn`), at
//! level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-GoblinBarrel-evo-s0): played at (14500, 13500), its three Goblins and, about
//! (3500, 13500), three GoblinDummies of 81 hitpoints stood on one tick, laid out alike.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_goblin_barrel`):
//!   - barrel_decoy_never -> `its_decoy_barrel_lands_on_the_mirrored_point_with_three_goblin_dummies` red;
//!   - release_ground_point_shifts_decoy -> the same red (spells.RELEASE_GROUND_POINT's one-unit shift taken by the decoy,
//!     on the left half: its dummies one lower in x than the Goblins' points mirrored).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

#[test]
fn its_decoy_barrel_lands_on_the_mirrored_point_with_three_goblin_dummies() {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["GoblinBarrel".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    // The card the next Goblin Barrel play resolves to (state.rs `resolve_play`).
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "GoblinBarrel").expect("the Goblin Barrel in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    // The basic plays far off, until the next is the form.
    let mut plays = 0;
    while next(&s) != "GoblinBarrel_EV1" {
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "GoblinBarrel", n(3000, 28000)).expect("a basic play");
        for _ in 0..40 {
            s.tick();
        }
        plays += 1;
        assert!(plays < 6, "no evolved play after {plays}");
    }
    let mut seen: Vec<EntityId> = s.entities().map(|e| e.id).collect();
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "GoblinBarrel", n(14500, 13500)).expect("the evolved play");
    let mut first: Vec<(usize, String, i32, (i32, i32))> = Vec::new();
    for k in 0..80usize {
        s.tick();
        for e in s.entities().filter(|e| e.team == Team::Blue && !seen.contains(&e.id)) {
            first.push((k, e.card.to_string(), e.max_hp, (e.pos.x / K, e.pos.y / K)));
        }
        seen = s.entities().map(|e| e.id).collect();
    }
    // Its Goblins about its point (a basic play's may land late, far off).
    let goblins: Vec<_> = first.iter().filter(|f| f.1 == "Goblin" && (f.3 .1 - 13500).abs() < 2000).collect();
    let dummies: Vec<_> = first.iter().filter(|f| f.1 == "GoblinDummy").collect();
    assert_eq!((goblins.len(), dummies.len()), (3, 3), "its units: {first:?}");
    assert!(goblins.iter().chain(&dummies).all(|f| f.0 == goblins[0].0), "one landing tick: {first:?}");
    assert!(dummies.iter().all(|f| f.2 == 81), "the dummies' 81: {dummies:?}");
    let mut want: Vec<(i32, i32)> = goblins.iter().map(|f| (18000 - f.3 .0, f.3 .1)).collect();
    let mut got: Vec<(i32, i32)> = dummies.iter().map(|f| f.3).collect();
    want.sort_unstable();
    got.sort_unstable();
    assert_eq!(got, want, "the dummies on the Goblins' points mirrored: {first:?}");
}
