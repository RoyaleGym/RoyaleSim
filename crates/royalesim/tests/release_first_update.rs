//! A SPELL'S RELEASED UNITS TAKE THEIR FIRST UPDATE ON THEIR CREATION TICK (state.rs, the release loop's
//! `release_first`; spawner.SCHEDULED_UNIT_FIRST_UPDATE = client_creation_tick): deploying, they do not walk, but a
//! neighbour's contact push moves them. Measured on client 15.535.29 (sp-form-GoblinBarrel-evo-s0 t117).
//!
//! The scene, the measured one: Blue's Goblin Barrel tapped on (15500, 2500), and a Blue Skeleton held on (15392, 2115).
//! On the tick its Goblins appear, the one born on (16000, 2212) stands on (16148, 2235), the Skeleton's push of 150
//! along (608, 97). Pinned: its x. Open: its y (the engine's push comes out along x alone, 23 short).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! release_first_update`): release_first_update_next_tick -> `a_goblin_born_on_a_skeleton_is_pushed_on_its_first_frame` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::Team;

const DECK: [&str; 8] = ["GoblinBarrel", "Skeletons", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Zap"];

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

#[test]
fn a_goblin_born_on_a_skeleton_is_pushed_on_its_first_frame() {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), DECK.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let skel_at = n((15392, 2115));
    let skel = s.scenario_spawn_now(Team::Blue, "Skeleton", skel_at, None).expect("the Skeleton");
    s.spawn_unit(Team::Blue, "GoblinBarrel", n((15500, 2500)), None).expect("the barrel");
    for _ in 0..60 {
        assert!(s.debug_set_pos(skel, skel_at));
        s.tick();
        let goblins: Vec<(i32, i32)> = s.entities().filter(|e| e.team == Team::Blue && e.card == "Goblin").map(|e| (e.pos.x / K, e.pos.y / K)).collect();
        if goblins.is_empty() {
            continue;
        }
        let right = *goblins.iter().max_by_key(|p| p.0).expect("a Goblin");
        assert_eq!(goblins.len(), 3, "the scene drifted: {goblins:?}");
        // Pinned: the push's x, the client's to the native unit. OPEN: its y. The client's Goblin stands on y 2235 (the
        // Skeleton's push along (608, 97)), the engine's on 2212: its push comes out along x alone, 23 short.
        assert!((right.0 - 16148).abs() <= 3, "the Goblin born beside the Skeleton stands on x {} on its first frame, not on the client's 16148: {goblins:?}", right.0);
        return;
    }
    panic!("the scene drifted: no Goblin appeared");
}
