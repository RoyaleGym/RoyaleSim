//! THE SKELETON KING'S BUTTON (tools/extract_cards.py `champion_soul_summon`; card.rs `SoulSummonDef`, `UnitUse::SoulUnit`;
//! state.rs `SoulKing`, `SoulRun`, `count_souls`, `soul_pass`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, Oracle's sp-champ-SkeletonKing-s0 and -late-s0; P the press's issue tick): his
//! area is made on the trigger P + 10 and puts down a copy of SkeletonKingSkeleton on P + 15, then every 5 ticks: 6
//! with no death before the press, 9 after three red Skeletons died; each 1 hitpoint of 1, deploying its first 8
//! frames; at exact directions around him, count - 1 evenly spaced, radii 2505 to 2978. The directions' start and order
//! and the radius are the client's draw (state.rs `client_rnd`), pinned here from the client's own state; a copy drawn
//! onto the river stands on land by the grid rule (arena.rs `nearest_land_grid`).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! skeleton_king`):
//!   - soul_summon_never -> both tests red;
//!   - souls_unread -> `three_deaths_before_the_press_make_nine` red;
//!   - souls_count_ignore_resurrect -> `an_ignore_resurrect_death_is_no_soul` red;
//!   - souls_not_copies -> `his_press_puts_down_six_one_hitpoint_copies_every_five_ticks` red;
//!   - souls_unshuffled -> `the_client_generator_shuffles_his_directions` and
//!     `from_the_clients_state_his_copies_stand_where_the_clients_did` red;
//!   - souls_axis_eject -> `a_copy_drawn_onto_the_river_stands_where_the_clients_did` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["SkeletonKing", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
/// On blue's side, out of every crown tower's reach.
const AT: (i32, i32) = (9000, 11000);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The King held on AT 40 ticks with `victims` red Skeletons put down in a knot 1300 ahead of him, which his first blow
/// (AreaDamageRadius 1300) kills before the press; then the press. Returns the battle, the King and P.
fn scene(victims: usize) -> (BattleState, EntityId, u32) {
    scene_at(victims, AT)
}

/// `scene` with the King held on `at`.
fn scene_at(victims: usize, at: (i32, i32)) -> (BattleState, EntityId, u32) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let king = s.scenario_spawn_now(Team::Blue, "SkeletonKing", n(at), None).expect("the Skeleton King");
    let mut doomed = Vec::new();
    for k in 0..victims {
        doomed.push(s.scenario_spawn_now(Team::Red, "Skeletons", n((at.0 - 200 + 200 * k as i32, at.1 + 1300)), None).expect("a red Skeleton"));
    }
    for _ in 0..40 {
        assert!(s.debug_set_pos(king, n(at)));
        s.tick();
    }
    for d in &doomed {
        assert!(s.entity(*d).is_none(), "the scene drifted: a red Skeleton lives at the press");
    }
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    (s, king, p)
}

/// The copies after the press: (first tick from P, point, hitpoints, ticks deploying), in creation order.
fn copies(s: &mut BattleState, king: EntityId, p: u32, ticks: u32) -> Vec<(u32, Vec2, i32, u32)> {
    copies_at(s, king, p, ticks, AT)
}

/// `copies` with the King held on `at`.
fn copies_at(s: &mut BattleState, king: EntityId, p: u32, ticks: u32, at: (i32, i32)) -> Vec<(u32, Vec2, i32, u32)> {
    let mut seen: Vec<(EntityId, u32, Vec2, i32, u32)> = Vec::new();
    for _ in 0..ticks {
        assert!(s.debug_set_pos(king, n(at)));
        s.tick();
        let k = s.tick_count() - 1 - p;
        for e in s.entities().filter(|e| e.team == Team::Blue && e.cloned) {
            match seen.iter_mut().find(|r| r.0 == e.id) {
                Some(r) => {
                    if e.deploying {
                        r.4 += 1;
                    }
                }
                None => seen.push((e.id, k, e.pos, e.hp, u32::from(e.deploying))),
            }
        }
    }
    seen.into_iter().map(|r| (r.1, r.2, r.3, r.4)).collect()
}

#[test]
fn his_press_puts_down_six_one_hitpoint_copies_every_five_ticks() {
    let (mut s, king, p) = scene(0);
    let got = copies(&mut s, king, p, 60);
    let firsts: Vec<u32> = got.iter().map(|c| c.0).collect();
    assert_eq!(firsts, vec![15, 20, 25, 30, 35, 40], "six copies from P + 15, every 5 ticks: {got:?}");
    for (k, (_, pos, hp, deploying)) in got.iter().enumerate() {
        assert_eq!(*hp, 1, "copy {k}: 1 hitpoint");
        assert_eq!(*deploying, 8, "copy {k}: deploying its first 8 frames");
        let (dx, dy) = ((pos.x / K - AT.0) as i64, (pos.y / K - AT.1) as i64);
        let r = isqrt(dx * dx + dy * dy);
        assert!((2495..=3001).contains(&r), "copy {k}: on the drawn ring (2500 + rnd(500)), {r} from him");
    }
}

/// One draw of the client's generator as Oracle read it off the client, written apart from the engine's: xorshift32
/// (s = -1 when 0; s ^= s << 13; s ^= s >> 17 arithmetic; s ^= s << 5), the draw |s| mod n.
fn reference_rnd(s: &mut u32, n: u32) -> u32 {
    let mut v = *s as i32;
    if v == 0 {
        v = -1;
    }
    v ^= v.wrapping_shl(13);
    v ^= v >> 17;
    v ^= v.wrapping_shl(5);
    *s = v as u32;
    v.unsigned_abs() % n
}

/// THE DRAW, against a reference written from Oracle's reading: the generator set to a known state after the press,
/// the area's first update shuffles the 5 directions with 250 pairs of draws, and copy k stands at perm[(k + 3) mod 5] x 72
/// degrees from +y toward +x, at 2500 + rnd(500), one draw per copy (x += sin(theta) r >> 10, y += sin(theta + 90) r >> 10).
#[test]
fn the_client_generator_shuffles_his_directions() {
    let (mut s, king, p) = scene(0);
    let seed = 0x1234_5678u32;
    s.scenario_set_client_rng(seed);
    let got = copies(&mut s, king, p, 45);
    let mut st = seed;
    let mut perm: Vec<i32> = (0..5).collect();
    for _ in 0..250 {
        let (i, j) = (reference_rnd(&mut st, 5) as usize, reference_rnd(&mut st, 5) as usize);
        if i != j {
            perm.swap(i, j);
        }
    }
    assert_ne!(perm, vec![0, 1, 2, 3, 4], "the scene drifted: this seed leaves the directions in place");
    let want: Vec<(i32, i32)> = (0..6)
        .map(|k| {
            let theta = perm[(k + 3) % 5] * 72;
            let r = 2500 + reference_rnd(&mut st, 500) as i32;
            ((royalesim::formation::sin1024(theta) * r) >> 10, (royalesim::formation::sin1024(theta + 90) * r) >> 10)
        })
        .collect();
    let have: Vec<(i32, i32)> = got.iter().map(|c| (c.1.x / K - AT.0, c.1.y / K - AT.1)).collect();
    assert_eq!(have, want, "each copy's offset from him, drawn as the client draws it");
}

/// THE CLIENT'S OWN DRAW (Oracle's rng_state pins, client 15.535.29 sp-champ-SkeletonKing-s0): the battle's generator
/// stood at 2180262551 from the start to the area's first update (no draw before it), the shuffle left 93151416 and perm
/// [3, 2, 1, 0, 4], and the six radii were 2875, 2541, 2519, 2931, 2935, 2559 (one draw each). From that state the copies
/// stand at 0, 288, 216, 144, 72, 0 degrees from +y toward +x; the four the scene left off the river's bank stood (-2416,
/// 784), (-1480, -2036), (1723, -2369) and (2791, 905) from the King, within the sine table's rounding (2).
#[test]
fn from_the_clients_state_his_copies_stand_where_the_clients_did() {
    let (mut s, king, p) = scene(0);
    s.scenario_set_client_rng(2_180_262_551);
    let got = copies(&mut s, king, p, 45);
    let plan = [(0, 2875), (288, 2541), (216, 2519), (144, 2931), (72, 2935), (0, 2559)];
    let want: Vec<(i32, i32)> = plan
        .iter()
        .map(|&(t, r)| ((royalesim::formation::sin1024(t) * r) >> 10, (royalesim::formation::sin1024(t + 90) * r) >> 10))
        .collect();
    let have: Vec<(i32, i32)> = got.iter().map(|c| (c.1.x / K - AT.0, c.1.y / K - AT.1)).collect();
    assert_eq!(have, want, "the client's directions and radii from its own state");
    let measured = [(-2416, 784), (-1480, -2036), (1723, -2369), (2791, 905)];
    for (k, m) in measured.iter().enumerate() {
        let h = have[k + 1];
        assert!((h.0 - m.0).abs() <= 2 && (h.1 - m.1).abs() <= 2, "copy {}: {h:?} against the client's {m:?}", k + 1);
    }
}

#[test]
fn three_deaths_before_the_press_make_nine() {
    let (mut s, king, p) = scene(3);
    let got = copies(&mut s, king, p, 70);
    assert_eq!(got.len(), 9, "6 and a soul for each death: {got:?}");
}

/// A Golem (IgnoreResurrect) and two Skeletons, red, held in a knot before him at 1 hitpoint, die to his first blow before
/// the press: 8 copies, the Golem's death no soul (measured: a Battle Ram's left the count at 6; the Golemites it leaves
/// live through the window).
#[test]
fn an_ignore_resurrect_death_is_no_soul() {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let king = s.scenario_spawn_now(Team::Blue, "SkeletonKing", n(AT), None).expect("the Skeleton King");
    let knot = [("Golem", (AT.0, AT.1 + 1600)), ("Skeletons", (AT.0 - 300, AT.1 + 1300)), ("Skeletons", (AT.0 + 300, AT.1 + 1300))];
    let reds: Vec<(EntityId, (i32, i32))> = knot.iter().map(|(c, p)| (s.scenario_spawn_now(Team::Red, c, n(*p), None).expect("a red unit"), *p)).collect();
    for (id, _) in &reds {
        assert!(s.debug_set_hp(*id, 1));
    }
    for _ in 0..40 {
        assert!(s.debug_set_pos(king, n(AT)));
        for (id, p) in &reds {
            let _ = s.debug_set_pos(*id, n(*p));
        }
        s.tick();
    }
    for (id, _) in &reds {
        assert!(s.entity(*id).is_none(), "the scene drifted: a red unit of the knot lives at the press");
    }
    let golemites = s.entities().filter(|e| e.team == Team::Red && e.kind == royalesim::entity::EntityKind::Troop).count();
    assert!(golemites >= 2, "the scene drifted: the Golem left no Golemites ({golemites})");
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let got = copies(&mut s, king, p, 60);
    assert_eq!(got.len(), 8, "6 and the two Skeletons' souls, not the Golem's: {got:?}");
}

/// His button is a champion's (state.rs `champion_button`): reported so, and charged, before the press.
#[test]
fn his_button_is_a_champions() {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_spawn_now(Team::Blue, "SkeletonKing", n(AT), None).expect("the champion");
    for _ in 0..30 {
        s.tick();
    }
    let b = s.ability_buttons(Team::Blue);
    assert_eq!(b.len(), 1, "one button: his");
    assert!(b[0].champion && b[0].available, "a champion's button, charged: {:?}", b[0]);
}

/// A COPY DRAWN ONTO THE RIVER (Oracle's fit, client 15.535.29 sp-champ-SkeletonKing-s0): the King on (11212, 13299),
/// the client's generator at 2180262551, his first copy drawn at 0 degrees and 2875, onto (11212, 16174) on the water,
/// stood on (10962, 14924): the nearest land point of the 500 grid set half a step off the draw, the tie between 1250
/// down and 1250 up to his own side. Every copy stands on land.
#[test]
fn a_copy_drawn_onto_the_river_stands_where_the_clients_did() {
    const KING: (i32, i32) = (11212, 13299);
    let (mut s, king, p) = scene_at(0, KING);
    s.scenario_set_client_rng(2_180_262_551);
    let got = copies_at(&mut s, king, p, 45, KING);
    assert_eq!(got.len(), 6, "the scene drifted: {got:?}");
    assert_eq!((got[0].1.x / K, got[0].1.y / K), (10962, 14924), "the first copy, drawn onto (11212, 16174)");
    for (k, c) in got.iter().enumerate() {
        assert!(s.arena().is_passable_ground(c.1), "copy {k} on water: {:?}", (c.1.x / K, c.1.y / K));
    }
}
