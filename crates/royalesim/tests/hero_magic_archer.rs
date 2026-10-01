//! THE HERO MAGIC ARCHER'S BUTTON (tools/extract_cards.py `decoy_warp_effect`; card.rs `DecoyWarpDef`, `PowerShotDef`;
//! state.rs EARLY_TRIGGER_TICKS, `MagicRun`, `magic_pass`, `magic_warps`; combat.rs `fire`, `power_copies`), at
//! level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-EliteArcher-hero-s0; the press issued t214, the cast from t215): the
//! decoy's first frame t219 on the hero's point, 271 hitpoints, deploying 20 frames; the Knight and a Skeleton that held
//! the hero took the decoy on t220; the hero 3,500 back on t221 (and its decoy's two pushes of 150), its target dropped;
//! its first shot after its cast, t249, the middle arrow (2,000 out, stepping 1,100), and on t250 two copies at its
//! first point 750 to each side, stepping 1,000; 48 from each on the Knight and the Musketeer; its next shot a plain
//! arrow of 135. With no enemy near (Oracle's sp-f2-ma-3500-9500-s0, sp-f2-ma-9500-13500-s0) the decoy is first seen
//! t125, last seen t264, gone on t265.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_magic_archer`): decoy_never, early_trigger_late, decoy_warp_never, power_shot_unread, decoy_never_killed,
//! decoy_pushed, warp_keeps_route (`after_its_warp_its_walk_is_planned_from_where_it_landed` red).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["EliteArcher", "Knight", "Archer", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
/// On blue's side, out of every red tower's reach.
const AT: (i32, i32) = (9000, 11000);

/// A point's distance from another, native.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = (((a.x - b.x) / K) as i64, ((a.y - b.y) / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// Blue's princess towers down (no crown tower reaches the scene), red `units` put and held on their points, and the hero
/// form put at AT and held there for 40 ticks: the battle, the hero and the reds.
fn start(units: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<(EntityId, Vec2)>) {
    let mut cfg = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    let reds: Vec<(EntityId, Vec2)> = units
        .iter()
        .map(|(card, p)| (s.scenario_spawn_now(Team::Red, card, n(p.0, p.1), None).expect("a red unit"), n(p.0, p.1)))
        .collect();
    s.spawn_unit(Team::Blue, "EliteArcher_hero", n(AT.0, AT.1), None).expect("the hero");
    s.tick();
    let hero = find_live(&s, Team::Blue, "EliteArcher_hero")[0].id;
    for _ in 0..40 {
        assert!(s.debug_set_pos(hero, n(AT.0, AT.1)));
        hold(&mut s, &reds);
        s.tick();
    }
    (s, hero, reds)
}

fn hold(s: &mut BattleState, reds: &[(EntityId, Vec2)]) {
    for (id, p) in reds {
        if s.entity(*id).is_some() {
            assert!(s.debug_set_pos(*id, *p));
        }
    }
}

/// The decoy's ids and points, alive.
fn decoys(s: &BattleState) -> Vec<(EntityId, Vec2, i32, bool)> {
    find_live(s, Team::Blue, "EliteArcherHero_Dummy").iter().map(|e| (e.id, e.pos, e.hp, e.deploying)).collect()
}

#[test]
fn its_decoy_stands_on_its_point_then_the_hero_hides_and_warps_back() {
    // A red Knight 3000 ahead, held: it takes the hero, then the decoy the tick the hero hides.
    let (mut s, hero, reds) = start(&[("Knight", (AT.0, AT.1 + 3000))]);
    let knight = reds[0].0;
    assert_eq!(s.entity(knight).and_then(|e| e.target), Some(hero), "the Knight holds the hero before the press");
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    let from = s.entity(hero).expect("the hero").pos;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    // The decoy's first frame P + 5 (the cast's start P + 1, its TriggerDelay 250 less a tick) on the hero's point, at
    // 271 hitpoints, deploying.
    let mut first = None;
    let mut rows = Vec::new();
    for _ in 0..12 {
        hold(&mut s, &reds);
        s.tick();
        let now = s.tick_count() - 1;
        let h = s.entity(hero).expect("the hero");
        rows.push((now - p, h.pos, h.target, s.entity(knight).and_then(|e| e.target), decoys(&s)));
        if first.is_none() && !decoys(&s).is_empty() {
            first = Some(now - p);
        }
    }
    assert_eq!(first, Some(5), "the decoy's first frame, from the press: {rows:?}");
    let (_, _, _, _, d) = &rows[4];
    let (decoy, at, hp, deploying) = d[0];
    assert!(dist(at, from) <= 1 && hp == 271 && deploying, "the decoy on the hero's point, 271, deploying: {rows:?}");
    // The Knight takes the decoy on P + 6, the tick the hero's buff hides it.
    assert_eq!(rows[5].3, Some(decoy), "the Knight's target on P + 6: {rows:?}");
    assert_eq!(rows[4].3, Some(hero), "the Knight's target on P + 5: {rows:?}");
    // The hero stands 3500 further back on P + 7 (Blue's back is -y), its target dropped; not before.
    let (y6, y7) = (rows[5].1.y / K, rows[6].1.y / K);
    assert!(from.y / K - y6 < 1000, "no warp before P + 7: {rows:?}");
    assert!((y6 - y7 - 3500).abs() <= 200, "the warp on P + 7 (3500 and a push): {rows:?}");
    assert_eq!(rows[6].2, None, "the hero's target dropped by the warp: {rows:?}");
    // NO_MOVE_ALLOW_ATTRACT: the decoy stands on its point while the hero and the Knight meet it (measured: on its point
    // from its first frame to its kill).
    let points: Vec<Vec2> = rows[4..].iter().filter_map(|r| r.4.first().map(|d| d.1)).collect();
    assert!(points.len() >= 7 && points.iter().all(|p| dist(*p, from) <= 1), "the decoy unmoved: {rows:?}");
}

#[test]
fn its_next_shot_is_the_middle_arrow_and_two_copies_beside_it() {
    // A red Giant held 7300 ahead of where the warp puts the hero (its first shot's target; the river from 15000), and
    // nothing else.
    let (mut s, hero, reds) = start(&[("Giant", (AT.0 + 1500, AT.1 + 3800))]);
    let db = s.cards().clone();
    let form = db.index("EliteArcher_hero").expect("the form");
    let conv = s.config().calib.projectile_speed_to_subtiles_per_tick;
    let (power, plain) = (db.scaled(form, 11, 19).unwrap(), db.scaled(form, 11, 53).unwrap());
    assert_eq!((power, plain), (48, 135), "the arrows' damage at level 11");
    // Each new shot of the hero's, in creation order: its tick, speed, damage and first point. Shots only end many ticks
    // after the next is made (a flight is 10 to 14 ticks, a swing 22), so the list's new tail is the tick's new shots.
    let mine = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(hero)).map(|q| (q.speed, q.damage, q.pos)).collect::<Vec<_>>();
    let mut known = mine(&s).len();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut seen: Vec<(u32, i32, i32, Vec2)> = Vec::new();
    for _ in 0..140 {
        hold(&mut s, &reds);
        s.tick();
        let now = s.tick_count() - 1;
        let m = mine(&s);
        for (sp, d, at) in m.iter().skip(known.min(m.len())) {
            seen.push((now, *sp, *d, *at));
        }
        known = m.len();
        if seen.len() >= 4 {
            break;
        }
    }
    assert!(seen.len() >= 4, "the hero's first two shots: {seen:?}");
    let (t0, sp0, d0, p0) = seen[0];
    assert_eq!((sp0, d0), (1100 * conv, power), "the middle arrow: {seen:?}");
    let copies: Vec<_> = seen[1..3].to_vec();
    for (t, sp, d, at) in &copies {
        assert_eq!((*sp, *d), (1000 * conv, power), "a copy: {seen:?}");
        assert!(*t == t0 || *t == t0 + 1, "a copy with the middle: {seen:?}");
        assert!((dist(*at, p0) - 750).abs() <= 2, "a copy 750 from the middle's first point: {seen:?}");
    }
    assert!((dist(copies[0].3, copies[1].3) - 1500).abs() <= 2, "the copies 1500 apart: {seen:?}");
    let (_, sp3, d3, _) = seen[3];
    assert_eq!((sp3, d3), (1000 * conv, plain), "the next shot is its own arrow: {seen:?}");
}

#[test]
fn its_decoy_is_killed_7000_ms_after_its_creation() {
    // No enemy near: the decoy stands until its interval's kill, 140 ticks on (measured on Oracle's sp-f2-ma-*: first
    // seen t125, last seen t264, gone on t265).
    let (mut s, _hero, _) = start(&[]);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut made = None;
    let mut gone = None;
    for _ in 0..200 {
        s.tick();
        let now = s.tick_count() - 1;
        let here = !decoys(&s).is_empty();
        if made.is_none() && here {
            made = Some(now);
        }
        if made.is_some() && gone.is_none() && !here {
            gone = Some(now);
        }
    }
    let made = made.expect("a decoy");
    assert_eq!(gone, Some(made + 140), "the decoy's last tick is its creation + 139");
}

/// THE WARP DROPS THE ROUTE (measured on client 15.535.29, sp-f2-ma-9500-13500-s0, no enemy near: 16 path nodes planned
/// from the warped point, where the route from before the warp held 13): once the hero walks again after its warp, its
/// next waypoint lies near where it landed, not near the point it left 3500 ahead. No enemy here either: one in its reach
/// after the warp would be shot at, and the hero would not walk.
#[test]
fn after_its_warp_its_walk_is_planned_from_where_it_landed() {
    let (mut s, hero, reds) = start(&[]);
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut seen = None;
    for _ in 0..160 {
        hold(&mut s, &reds);
        s.tick();
        let k = s.tick_count() - 1 - p;
        let h = s.entity(hero).expect("the hero");
        if k >= 8 && h.pos.y / K < AT.1 - 3000 {
            if let Some(next) = h.route.last() {
                seen = Some((k, dist(*next, h.pos)));
                break;
            }
        }
    }
    let (k, d) = seen.expect("the scene drifted: the hero never walked again after its warp");
    assert!(d < 2000, "P + {k}: its next waypoint {d} from it, as if planned from the point it left");
}
