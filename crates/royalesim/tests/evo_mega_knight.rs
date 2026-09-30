//! THE EVO MEGA KNIGHT (tools/extract_cards.py `uppercut_block`; card.rs `UppercutDef`; state.rs `uppercut_count`,
//! `UppercutRun`, `uppercut_pass`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-MegaKnight-evo-s0): after his deploy's dash blow, his first hit (268 off a
//! Knight, 105 on the ladder) threw it from the hit's tick + 2 toward its king tower, 250 a tick for 8 ticks, then 225,
//! 200, 175, 150, 125 ...; his next hit threw nothing, the one after threw it again the same way.
//! Read off the table, not measured: the root after the throw (no move from 1000 ms after the hit for 400 ms), and the
//! ladder's tail past 125.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_mega_knight`):
//!   - uppercut_never -> `every_second_hit_throws_its_target_toward_its_king` red;
//!   - uppercut_throw_never -> the same red.
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
    cfg.decks = [vec!["MegaKnight".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    // Blue's princess towers down (its king wakes; its reach stops short of y 12000): no crown tower reaches the scene.
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    s
}

#[test]
fn every_second_hit_throws_its_target_toward_its_king() {
    // The form held at (9000, 12500), a red Knight 1400 ahead in his reach and clear of his body (750 + 500; its king
    // straight ahead at (9000, 29000)), held and topped up until his second hit, then let go (topped up still). No dash:
    // the Knight is inside his jump's minimum range.
    let mut s = battle();
    let (at, kn_at) = (n(9000, 12500), n(9000, 13900));
    s.spawn_unit(Team::Blue, "MegaKnight_EV1", at, None).expect("the Mega Knight");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    s.tick();
    let mk = find_live(&s, Team::Blue, "MegaKnight_EV1").first().expect("the Mega Knight").id;
    let (mut hits, mut ys) = (Vec::new(), Vec::new());
    for k in 0..200 {
        assert!(s.debug_set_pos(mk, at));
        let top = s.entity(knight).expect("the Knight").max_hp;
        if hits.len() < 2 {
            assert!(s.debug_set_pos(knight, kn_at));
        }
        assert!(s.debug_set_hp(knight, top));
        s.tick();
        let e = s.entity(knight).expect("the Knight");
        if top - e.hp == 268 {
            hits.push(k);
        }
        ys.push((e.pos.x / K, e.pos.y / K));
    }
    assert!(hits.len() >= 2, "two hits of 268: {hits:?}");
    let (h1, h2) = (hits[0], hits[1]);
    assert!(ys[h1 + 1..h2].iter().all(|p| *p == (9000, 13900)), "no throw after the first hit: {:?}", &ys[h1 + 1..h2]);
    // From h2 + 2: 8 steps of 250, then 225 down to 25, 0, and the step back of 25, straight at its king (x unchanged).
    let mut want: Vec<i32> = vec![250; 8];
    want.extend((1..=9).map(|j| 250 - 25 * j));
    want.extend([0, -25]);
    for (j, w) in want.iter().enumerate() {
        let k = h2 + 2 + j;
        let (dx, dy) = (ys[k].0 - ys[k - 1].0, ys[k].1 - ys[k - 1].1);
        assert_eq!((dx, dy), (0, *w), "step {j} (frame {k}): {:?}", &ys[h2..h2 + 24]);
    }
    // The root: from 1000 ms after the hit, 400 ms, its point kept.
    let root = h2 + 20;
    assert!(ys[root..root + 8].iter().all(|p| *p == ys[root]), "rooted h2 + 20 .. h2 + 27: {:?}", &ys[root..root + 10]);
}
