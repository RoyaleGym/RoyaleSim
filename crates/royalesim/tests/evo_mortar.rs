//! THE EVO MORTAR (card.rs `ShotSpawnDef`; combat.rs `step_projectiles`, the shot's released unit), against client
//! 15.535.29 at level 11.
//!
//! THE MEASUREMENTS (sp-form-Mortar-evo-s0):
//!   - the third play of an evolved Mortar entry is the form (DarkElixirCost 2);
//!   - its first swing reads 3750 on its first attack frame (LoadTime 3700 + 50);
//!   - its shot's landing took 266 (104 at level 1) off a Knight and a Musketeer, and on that frame a Goblin (202
//!     hitpoints, deploying) stood on the landing point; its next shot's Goblin 96 frames later.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_mortar`):
//!   - shot_spawn_dropped -> `each_shot_puts_a_goblin_down_where_it_lands_on_its_landing_frame` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Mortar".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

#[test]
fn the_third_play_is_the_evolved_mortar() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's DarkElixirCost");
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "Mortar").expect("the Mortar in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    for (k, x) in [3500, 14500].into_iter().enumerate() {
        assert_eq!(next(&s), "Mortar", "a basic play {k}");
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "Mortar", n(x, 5500)).expect("the play");
        for _ in 0..20 {
            s.tick();
        }
    }
    assert_eq!(next(&s), "Mortar_EV1", "the third play is the form");
}

#[test]
fn each_shot_puts_a_goblin_down_where_it_lands_on_its_landing_frame() {
    // The form at (9500, 8500); a red Giant held 6000 in front of it (it walks at buildings; held, it never arrives), its
    // hp topped up.
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Mortar_EV1", n(9500, 8500), None).expect("the Mortar");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", n(9500, 14500), None).expect("a red Giant");
    let top = s.entity(giant).expect("the Giant").max_hp;
    // A landing: the Giant losing the shot's 266 or more on a frame (a Goblin's own hits are smaller). A Goblin: each
    // new blue Goblin, with its first frame, point, hp and deploy state.
    let mut landings: Vec<(usize, Vec2)> = Vec::new();
    let mut goblins_seen: Vec<(usize, Vec2, i32, bool)> = Vec::new();
    let mut known: Vec<royalesim::EntityId> = Vec::new();
    for k in 0..300 {
        assert!(s.debug_set_pos(giant, n(9500, 14500)));
        s.tick();
        let hp = s.entity(giant).expect("the Giant").hp;
        if top - hp >= 266 {
            landings.push((k, s.entity(giant).unwrap().pos));
        }
        assert!(s.debug_set_hp(giant, top));
        for g in find_live(&s, Team::Blue, "Goblin") {
            if !known.contains(&g.id) {
                known.push(g.id);
                goblins_seen.push((k, g.pos, g.hp, g.deploying));
            }
        }
    }
    assert!(landings.len() >= 2, "two landings: {landings:?}");
    let first_goblin = goblins_seen.first().expect("a Goblin");
    assert_eq!(first_goblin.0, landings[0].0, "the first Goblin on the first landing's frame");
    assert!(first_goblin.1.dist(n(9500, 14500)) <= 600 * K, "on the landing point: {:?}", first_goblin.1);
    assert_eq!(first_goblin.2, 202, "202 hitpoints at level 11");
    let second = goblins_seen.iter().find(|g| g.0 == landings[1].0).expect("a Goblin on the second landing's frame");
    assert!(second.3, "deploying");
    assert_eq!(landings[1].0 - landings[0].0, 94, "HitSpeed 4700 between shots at a standing target");
}
