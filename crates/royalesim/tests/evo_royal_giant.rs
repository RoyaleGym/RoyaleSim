//! THE EVO ROYAL GIANT (tools/extract_cards.py `attack_area_block`; card.rs `AttackAreaDef`; state.rs `attack_areas`; spell.rs
//! `SpellMotion::Attached`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, Oracle's sp-rg-evo-attack-s0, five shots at a crown tower): a Knight 1722-2443 from
//! the giant lost 81 (32 on the ladder) on the tick after each shot left, and slid away from him from the tick after
//! that, 198, 174, 149, 125, ... (the Pushback 1000 ladder). The row's AreaEffectOnHit (Freeze) did nothing: the tower
//! kept its cadence through his hits.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_royal_giant`):
//!   - attack_area_never -> `each_shot_pushes_the_enemies_around_him_on_the_next_tick` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

/// Blue's princess towers down (its king wakes; its reach stops short of y 12000): no crown tower reaches the scene.
fn blue_towers_down(s: &mut BattleState) {
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
}

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["RoyalGiant".into(), "Knight".into()], vec!["Knight".into(), "Cannon".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// `card` held at (9000, 11000) shooting a red Cannon held at (11000, 14000); a red Knight held 2000 above him (within
/// the push's 3000), both topped up, blue's princess towers down (no crown tower reaches the scene). Per frame: whether
/// a shot of his left on it, what the Knight lost, and its move off its point.
fn scene(card: &str) -> Vec<(bool, i32, Vec2)> {
    let mut s = battle();
    blue_towers_down(&mut s);
    let (at, kn_at, cannon_at) = (n(9000, 11000), n(9000, 13000), n(11000, 14000));
    s.spawn_unit(Team::Blue, card, at, None).expect("the Royal Giant");
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", cannon_at, None).expect("a red Cannon");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    s.tick();
    let giant = find_live(&s, Team::Blue, card).first().expect("the Royal Giant").id;
    let mut out = Vec::new();
    let mut shots = 0;
    for _ in 0..200 {
        assert!(s.debug_set_pos(giant, at));
        assert!(s.debug_set_pos(knight, kn_at));
        let (kt, ct) = (s.entity(knight).expect("the Knight").max_hp, s.entity(cannon).expect("the Cannon").max_hp);
        assert!(s.debug_set_hp(knight, kt));
        assert!(s.debug_set_hp(cannon, ct));
        s.tick();
        let now = s.projectiles().iter().filter(|p| p.firer == Some(giant)).count();
        let k = s.entity(knight).expect("the Knight");
        out.push((now > shots, kt - k.hp, k.pos.sub(kn_at)));
        shots = now;
    }
    out
}

#[test]
fn each_shot_pushes_the_enemies_around_him_on_the_next_tick() {
    let evo = scene("RoyalGiant_EV1");
    let base = scene("RoyalGiant");
    let fired: Vec<usize> = (0..evo.len()).filter(|k| evo[*k].0).collect();
    assert!(fired.len() >= 3, "three shots: {fired:?}");
    assert_eq!(fired, (0..base.len()).filter(|k| base[*k].0).collect::<Vec<_>>(), "the form shoots when the Royal Giant does");
    for f in fired.iter().copied().take(3) {
        let extra = |k: usize| evo[k].1 - base[k].1;
        assert_eq!(extra(f), 0, "nothing on the shot's own tick {f}");
        assert_eq!(extra(f + 1), 81, "the push's 81 (32 at level 1) on the tick after the shot {f}");
        assert_eq!(evo[f + 1].2, base[f + 1].2, "no slide on the push's own tick {}", f + 1);
        let slide = evo[f + 2].2.sub(base[f + 2].2);
        assert!(slide.y > 0 && slide.x.abs() * 4 <= slide.y, "the slide away from him from the tick after: {slide:?}");
    }
}
