//! A GROUND POINT LEFT ON WATER (arena.rs `nearest_land_grid`; state.rs `phase_path16402_for`'s dash end, `soul_pass`):
//! put on the nearest land point of a 500 grid set half a step off it, as client 15.535.29 puts it. Oracle's fit off
//! the frames, 5 of 5: the Boss Bandit's dash in sp-champ-BossBandit-nopress-s0, and four Skeleton King copies in
//! sp-champ-SkeletonKing-s0 and -late-s0 (tests/skeleton_king.rs pins one in the battle).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! water_landing`): dash_ends_on_water -> `a_bandits_dash_that_ends_over_the_river_ends_on_land` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::Team;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The five measured points: (left on water, where the client put it). All Blue's.
const MEASURED: [((i32, i32), (i32, i32)); 5] = [
    ((11043, 15703), (10793, 14953)),
    ((11212, 16174), (10962, 14924)),
    ((10006, 15623), (9756, 14873)),
    ((11892, 16480), (11642, 17230)),
    ((11438, 16328), (11188, 17078)),
];

#[test]
fn each_measured_point_lands_where_the_clients_did() {
    let s = BattleState::try_new(0, config()).expect("the battle");
    let a = s.arena();
    for (w, want) in MEASURED {
        assert!(!a.is_passable_ground(n(w)), "{w:?} is on water");
        let got = a.nearest_land_grid(n(w), Team::Blue).map(|q| (q.x / K, q.y / K));
        assert_eq!(got, Some(want), "{w:?}");
    }
    let dry = n((9000, 11000));
    assert_eq!(a.nearest_land_grid(dry, Team::Blue), Some(dry), "a point on land stays");
}

/// Red's twin of each point lands on the twin of Blue's answer: the ties are taken in the unit's own frame.
#[test]
fn a_rotated_point_lands_on_the_rotated_point() {
    let s = BattleState::try_new(0, config()).expect("the battle");
    let a = s.arena();
    let (w, h) = (a.width / K, a.height / K);
    for (p, want) in MEASURED {
        let got = a.nearest_land_grid(n((w - p.0, h - p.1)), Team::Red).map(|q| (q.x / K, q.y / K));
        assert_eq!(got, Some((w - want.0, h - want.1)), "{p:?}");
    }
}

/// A Bandit (its row Assassin) dashing at a Knight held across the river strikes it from over the water; on that tick
/// it stands on land, as the client's Boss Bandit did (its blow on t178, and itself put from (11043, 15703) onto
/// (10793, 14953) that tick).
#[test]
fn a_bandits_dash_that_ends_over_the_river_ends_on_land() {
    let mut cfg = config();
    let deck: Vec<String> = ["Assassin", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    const KNIGHT: (i32, i32) = (11400, 17400);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(KNIGHT), None).expect("the Knight");
    let bandit = s.scenario_spawn_now(Team::Blue, "Assassin", n((10500, 12500)), None).expect("the Bandit");
    let full = s.entity(knight).expect("the Knight").hp;
    let mut before = None;
    for _ in 0..120 {
        assert!(s.debug_set_pos(knight, n(KNIGHT)));
        s.tick();
        let at = s.entity(bandit).expect("the Bandit").pos;
        if s.entity(knight).expect("the Knight").hp < full {
            let prev: Vec2 = before.expect("the scene drifted: a blow on the first tick");
            assert!(prev.y / K > 13000, "the scene drifted: no dash before the blow ({:?})", (prev.x / K, prev.y / K));
            let wet = (15000..17000).contains(&(at.y / K)) || !s.arena().is_passable_ground(at);
            assert!(!wet, "the blow's tick: the Bandit on water at {:?}", (at.x / K, at.y / K));
            return;
        }
        before = Some(at);
    }
    panic!("the scene drifted: no blow on the Knight");
}
