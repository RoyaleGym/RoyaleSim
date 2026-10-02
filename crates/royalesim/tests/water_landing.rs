//! A GROUND POINT LEFT ON WATER (arena.rs `nearest_land_grid`; state.rs `phase_path16402_for`'s dash end, `soul_pass`):
//! put on the nearest land point of a 500 grid set half a step off it, as client 15.535.29 puts it. Oracle's fit off
//! the frames, 5 of 5: the Boss Bandit's dash in sp-champ-BossBandit-nopress-s0, and four Skeleton King copies in
//! sp-champ-SkeletonKing-s0 and -late-s0 (tests/skeleton_king.rs pins one in the battle).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! water_landing`): dash_ends_on_water -> `a_bandits_dash_that_ends_over_the_river_ends_on_land` red;
//! chain_hop_ends_on_water -> `a_golden_knights_hop_whose_blow_lands_over_the_river_ends_on_land` red;
//! land_ties_own_frame -> `a_red_units_point_takes_the_arenas_ties` red.
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

/// Eight Red Skeleton King copies drawn onto the water where the grid's offsets tie (client 15.535.29, sp-il-b5e2's two
/// runs and sp-il-323a; each draw from the King's frame and the radius the client drew, each landing as the frame has
/// it, within the sine table's rounding): (drawn, put).
const RED_MEASURED: [((i32, i32), (i32, i32)); 8] = [
    ((7741, 15156), (7491, 14906)),
    ((8592, 15786), (8342, 14536)),
    ((5144, 15107), (4894, 14857)),
    ((6394, 15508), (6144, 14758)),
    ((7831, 15813), (7581, 14563)),
    ((7156, 16982), (6906, 17232)),
    ((9926, 15458), (9676, 14708)),
    ((6610, 15933), (6360, 14683)),
];

/// Red's ties are the arena's, as Blue's: toward -x, then -y (Red's own frame would take +x, +y in every one).
#[test]
fn a_red_units_point_takes_the_arenas_ties() {
    let s = BattleState::try_new(0, config()).expect("the battle");
    let a = s.arena();
    for (w, want) in RED_MEASURED {
        assert!(!a.is_passable_ground(n(w)), "{w:?} is on water");
        let got = a.nearest_land_grid(n(w), Team::Red).map(|q| (q.x / K, q.y / K));
        assert_eq!(got, Some(want), "{w:?}");
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

/// A GOLDEN KNIGHT'S HOP THAT ENDS OVER WATER (state.rs, the dash chain's step): his blow lands with his point over the
/// river, and he stands on the grid rule's land point that tick (Oracle's GK scenes on client 15.535.29: s0 t213 and
/// the delay variants' landings; the target-death end, s0 t206, is the same hook). The scene: Blue's Golden Knight at
/// (6500, 13000), Red's Giant deploying at (6500, 18000) across the river, the press issued: the chain hops at the Giant,
/// and its blow lands within Range 1,200 + radii 800 and 750 of it, over the water.
#[test]
fn a_golden_knights_hop_whose_blow_lands_over_the_river_ends_on_land() {
    const DECK: [&str; 8] = ["GoldenKnight", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
    const GIANT: (i32, i32) = (6500, 18000);
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), ["Giant", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let gk = s.scenario_spawn_now(Team::Blue, "GoldenKnight", n((6500, 13000)), None).expect("the Golden Knight");
    s.spawn_unit(Team::Red, "Giant", n(GIANT), None).expect("the Giant");
    s.tick();
    let giant = s.entities().find(|e| e.team == Team::Red && e.card == "Giant").map(|e| e.id).expect("the Giant is down");
    let hp0 = s.entity(giant).expect("the Giant").hp;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut prev = s.entity(gk).expect("he lives").pos;
    for _ in 0..30 {
        s.tick();
        let at = s.entity(gk).expect("he lives").pos;
        if s.entity(giant).expect("the Giant lives").hp < hp0 {
            // Where the hop's sub-steps (250, then the rest of 400) would have ended it: the first within reach.
            let (p0, g) = ((prev.x / K, prev.y / K), GIANT);
            let reach = |q: (i32, i32)| {
                let (dx, dy) = (i64::from(q.0 - g.0), i64::from(q.1 - g.1));
                dx * dx + dy * dy <= 2750 * 2750
            };
            let (q1, _) = royalesim::move16402::dash_half_step(p0, g, 250);
            let q = if reach(q1) { q1 } else { royalesim::move16402::dash_half_step(q1, g, 150).0 };
            assert!(!s.arena().is_passable_ground(n(q)), "the scene drifted: the blow lands on land ({q:?})");
            let want = s.arena().nearest_land_grid(n(q), Team::Blue).expect("land within reach");
            assert_eq!((at.x / K, at.y / K), (want.x / K, want.y / K), "the blow tick: he stands on the grid rule's land point for {q:?}");
            return;
        }
        prev = at;
    }
    panic!("the scene drifted: the hop never landed its blow on the Giant");
}
