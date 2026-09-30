//! THE EVO BARBARIANS (tools/extract_cards.py `hit_rage_block`; card.rs `EvoDef::hit_rage` on the form; state.rs
//! `evo_after_fire`), at level 11.
//!
//! Its units are the Evo Battle Ram's death spawn, Barbarian_EV1: after every hit (BuffAfterHitsCount 1) they rage for
//! 5000 ms (Barbarian_EVO_Rage: SpeedMultiplier and HitSpeedMultiplier 130). Measured on client 15.535.29 on the ram's
//! Barbarians (sp-ram-alone-s0): the first hit on progress 1400, the progress stepping 65 from the next tick (50 x 130 /
//! 100). Not measured on the form's own play.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_barbarians`):
//!   - form_hit_rage_dropped -> `its_barbarians_rage_from_the_tick_after_their_first_hit` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Barbarians".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

#[test]
fn its_barbarians_rage_from_the_tick_after_their_first_hit() {
    // The evolved Barbarians put down 1200 short of a red Knight held with its hitpoints topped up (out of every crown
    // tower's reach): the first Barbarian to hit steps its progress 65 from the next tick, 50 before.
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Barbarians_EV1", n(9000, 13000), None).expect("the Barbarians");
    let red = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 14200), None).expect("a red Knight");
    s.tick();
    let barbs: Vec<EntityId> = find_live(&s, Team::Blue, "Barbarians_EV1").iter().map(|e| e.id).collect();
    assert_eq!(barbs.len(), 5, "five Barbarians");
    let mut prog: Vec<Vec<i32>> = vec![Vec::new(); barbs.len()];
    for _ in 0..160 {
        assert!(s.debug_set_pos(red, n(9000, 14200)));
        let top = s.entity(red).expect("the Knight").max_hp;
        assert!(s.debug_set_hp(red, top));
        s.tick();
        for (m, b) in barbs.iter().enumerate() {
            prog[m].push(s.entity(*b).map_or(-1, |e| e.attack_ms));
        }
    }
    let (p, hit) = prog
        .iter()
        .filter_map(|p| p.iter().position(|v| *v >= 1400).map(|h| (p, h)))
        .min_by_key(|(_, h)| *h)
        .expect("a Barbarian hits");
    assert_eq!(p[hit + 1] - p[hit], 65, "raged from the tick after its first hit: {:?}", &p[hit.saturating_sub(3)..hit + 3]);
    assert!(p[hit.saturating_sub(4)..hit].windows(2).all(|w| w[1] - w[0] == 50), "unraged before it: {:?}", &p[hit.saturating_sub(4)..hit + 1]);
}
