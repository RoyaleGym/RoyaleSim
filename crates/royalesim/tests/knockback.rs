//! KNOCKBACK SAFETY -- a push must never leave a ground unit outside the arena, and,
//! under the fixed_distance arm, never on water or with its centre inside a building
//! footprint; and two pushes landing on one unit in one tick follow the registry's
//! stacking rule under each arm.
//!
//! WHY IT EXISTS: walking can never put a unit on water (collide::dry_position) or
//! inside a footprint (collision push-out), and every other gate trusts that. A
//! knockback is NOT blocked by the river (docs/spell-spec.md: the Log pushes units
//! "over the river"; calibration knockback.WATER_RESOLUTION = eject_to_nearest_land),
//! so it is the first mechanic that can displace a unit to a place no walk reaches.
//! `spell::settle` resolves it under knockback.DISPLACEMENT_LAW = fixed_distance;
//! this file is the measurement that the resolution holds everywhere a push can go
//! wrong: both river banks, both bridges, the arena edges, and building footprints
//! (a Cannon, a princess tower, a king tower). UNDER THE SHIPPED LADDER
//! (client16402, tests/knockback16402.rs) the measured law applies: the grid
//! write clamps at the arena edge, the water ejection runs at the start of every
//! ladder tick with a positive speed, and nothing ejects a unit from a footprint or
//! from water its last two steps put it on -- so the sweeps run under BOTH arms and
//! assert each arm's own claims (the every-tick invariants know which they are).
//!
//! THE CHECKS
//!   1. `settle_*`: settle itself, exhaustively: every start point on a quarter-tile
//!      grid of legal ground, pushed 8 ways at both shipped Pushback distances.
//!   2. `sweep_*`: the same hazards through the real tick loop -- a Fireball cast on
//!      the dry side of a deploying Knight or Giant so the radial push points INTO
//!      the hazard -- with the every-tick invariants (common::Invariants) running for
//!      20 ticks after the push, under the shipped config and under
//!      `symmetric_config()` (the fixed_distance slide). Vacuity: most pushes must
//!      have had an illegal unresolved destination, or the sweep proved nothing, and
//!      every victim must have been pushed (displaced, or its ladder armed).
//!   3. `two_pushes_*`: two Fireballs landing on one unit in one tick, cast in both
//!      orders: identical state_hash traces under vector_sum (the fixed_distance arm),
//!      and under the shipped first_wins_while_active the Knight's track equals the
//!      FIRST-cast Fireball's alone under either order (and the two orders differ).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant
//! cargo test --test knockback`):
//!   no_water_resolution       -> settle_*, sweep_river_banks_and_bridges, sweep_building_footprints
//!                                (it returns before the footprint push-out too)
//!   knock_unclamped           -> settle_*, sweep_arena_edges. (Removing only the
//!                                clamp does NOT land: the water ejection also keeps
//!                                a push in bounds, so the plant has to bypass both.)
//!   knock_ignores_footprints  -> settle_*, sweep_building_footprints
//!   knock_last_wins           -> two_pushes_* (the vector_sum half)
//!   (knockback_instant_slide, the ladder's regression plant, leaves this file green:
//!   it replaces the ladder's steps with the whole length at once but keeps the
//!   gate, so first_wins_while_active still holds -- tests/knockback16402.rs and the
//!   settled-carry assertions of tests/spells.rs are where it lands, 8 + 3 red)
//!
//! WHAT IT CANNOT CATCH: whether the real game ejects, clamps or continues a push
//! across the river (knockback.WATER_RESOLUTION is a guess); pushes of FLYING units
//! (they may legally end over water; only the arena clamp applies).
mod common;

use royalesim::arena::Lane;
use royalesim::entity::EntityKind;
use royalesim::fixed::{milli, Vec2, SUBTILE};
use royalesim::path::Obstacle;
use royalesim::state::{footprint_of, BattleConfig, BattleState, KnockLaw};
use royalesim::{EntityId, Team};
use common::*;

fn pushbacks() -> [i32; 2] {
    let db = cards();
    let fb = db.get(db.index("Fireball").unwrap()).spell.clone().unwrap();
    let lg = db.get(db.index("Log").unwrap()).spell.clone().unwrap();
    let dist = |s: &royalesim::card::SpellDef| match &s.shape {
        royalesim::card::SpellShape::Projectile { hit: Some(h), .. } => h.knockback.unwrap().distance,
        royalesim::card::SpellShape::Rolling { hit, .. } => hit.knockback.unwrap().distance,
        other => panic!("no knockback: {other:?}"),
    };
    [dist(&fb), dist(&lg)]
}

/// A board with buildings next to the hazards: Cannons on both banks and beside
/// both bridge exits, plus the six crown towers.
fn board() -> (BattleState, Vec<Obstacle>) {
    let mut s = BattleState::new(1, config());
    let a = s.arena().clone();
    let r = card_stat(&s, "Cannon").collision_radius;
    let bank_lo = a.water_y_min - r - milli(300);
    let bank_hi = a.water_y_max + r + milli(300);
    for (team, y) in [(Team::Blue, bank_lo), (Team::Red, bank_hi)] {
        for x in [t(900, 0).x, a.bridge(Lane::Left).x_max + r + milli(200), a.bridge(Lane::Right).x_min - r - milli(200)] {
            s.scenario_spawn_now(team, "Cannon", Vec2::new(x, y), None).unwrap();
        }
    }
    let obstacles = s
        .entities()
        .filter(|e| e.kind.is_building())
        .map(|e| Obstacle { id: e.id, shape: footprint_of(&s, e.id).unwrap(), radius: e.radius, key: (0, 0, 0, 0, 0), ally: false })
        .collect();
    (s, obstacles)
}

fn legal(s: &BattleState, obstacles: &[Obstacle], p: Vec2) -> bool {
    s.arena().is_passable_ground(p) && !obstacles.iter().any(|o| o.shape.penetrates(p, 0))
}

#[test]
fn settle_never_leaves_a_ground_unit_on_water_off_the_arena_or_in_a_footprint() {
    let (s, obstacles) = board();
    let a = s.arena().clone();
    let radius = card_stat(&s, "Knight").collision_radius;
    let q = SUBTILE / 4;
    let dirs = [(1, 0), (-1, 0), (0, 1), (0, -1), (3, 4), (-3, 4), (3, -4), (-3, -4)];
    let (mut tried, mut hazards, mut stayed) = (0u64, 0u64, 0u64);
    for push in pushbacks() {
        for yi in 0..=(a.height / q) {
            for xi in 0..=(a.width / q) {
                let old = Vec2::new(xi * q, yi * q);
                if !legal(&s, &obstacles, old) {
                    continue;
                }
                for team in [Team::Blue, Team::Red] {
                    for (dx, dy) in dirs {
                        let n = if dx != 0 && dy != 0 { 5 } else { 1 };
                        let desired = Vec2::new(old.x + dx * push / n, old.y + dy * push / n);
                        let got = royalesim::spell::settle(&a, &obstacles, team, radius, false, old, desired);
                        tried += 1;
                        hazards += u64::from(!legal(&s, &obstacles, desired));
                        stayed += u64::from(got == old);
                        assert!(legal(&s, &obstacles, got), "settle({old:?} -> {desired:?}, {team:?}) = {got:?}: water / off-arena / inside a footprint");
                    }
                }
            }
        }
    }
    println!("settle sweep: {tried} pushes, {hazards} with an illegal destination, {stayed} resolved by staying put");
    assert!(hazards * 10 > tried / 10, "vacuous: only {hazards} of {tried} pushes aimed at a hazard");
    // A resolution that simply refuses every hazardous push would pass the legality
    // check; the registry says EJECT, so staying put must be the rare fallback.
    assert!(stayed * 4 < hazards, "{stayed} of {hazards} hazardous pushes were resolved by not moving at all");
}

struct Item {
    victim: &'static str,
    at: Vec2,
    /// Unit direction the push should take, (dx, dy) in integer steps.
    dir: (i32, i32),
}

/// Cast a Fireball so its radial push on a deploying victim at `at` points along
/// `dir`; run 20 ticks past the landing with the invariants on. Returns whether the
/// UNRESOLVED destination was illegal (a hazard) and whether the victim was pushed
/// (displaced on the landing tick under the slide; its ladder armed under the
/// shipped law, whose first step comes the tick after).
fn run_item(item: &Item, cfg: &BattleConfig) -> (bool, bool) {
    let mut s = BattleState::new(1, cfg.clone());
    let push = knock_carry(&cfg.calib, pushbacks()[0]);
    let caster = if item.at.y > s.arena().height / 2 { Team::Blue } else { Team::Red };
    let victim_team = caster.other();
    let back = milli(1000);
    // dir is (1, 0)-like or a (3, 4)-like Pythagorean pair: length 1 or 5.
    let n = if item.dir.0 != 0 && item.dir.1 != 0 { 5 } else { 1 };
    let impact = Vec2::new(item.at.x - item.dir.0 * back / n, item.at.y - item.dir.1 * back / n);
    let impact = Vec2::new(impact.x.clamp(1, s.arena().width - 1), impact.y.clamp(1, s.arena().height - 1));
    s.spawn_unit(caster, "Fireball", impact, None).unwrap();
    let mut probe = s.clone();
    let mut arrival = 0;
    while !probe.spells().is_empty() || arrival == 0 {
        probe.tick();
        arrival += 1;
    }
    let mut inv = Invariants::new(DEFAULT_TOLERANCE);
    let mut victim: Option<(EntityId, Vec2)> = None;
    let mut hazard = false;
    let mut moved = false;
    for k in 0..arrival + 20 {
        if k + 5 == arrival {
            s.spawn_unit(victim_team, item.victim, item.at, None).unwrap_or_else(|e| panic!("{} at {:?}: {e:?}", item.victim, item.at));
        }
        s.tick();
        if victim.is_none() {
            victim = s.entities().find(|e| e.team == victim_team && e.kind == EntityKind::Troop).map(|e| (e.id, e.pos));
            if let Some((_, p)) = victim {
                // The destination the push WOULD reach unresolved, from the same law.
                let d = p.sub(impact);
                let len = royalesim::fixed::isqrt(d.len2()).max(1);
                let dest = Vec2::new(p.x + (d.x as i64 * push as i64 / len) as i32, p.y + (d.y as i64 * push as i64 / len) as i32);
                let obstacles: Vec<Obstacle> =
                    s.entities().filter(|e| e.kind.is_building()).map(|e| Obstacle { id: e.id, shape: footprint_of(&s, e.id).unwrap(), radius: e.radius, key: (0, 0, 0, 0, 0), ally: false }).collect();
                hazard = !legal(&s, &obstacles, dest);
            }
        } else if let Some((id, p)) = victim {
            // k == arrival - 1 is the landing tick: still deploying, so any change is the push.
            if k + 1 == arrival {
                moved = s.entity(id).is_some_and(|v| v.pos != p || v.push_active);
            }
        }
        inv.check(&s).unwrap_or_else(|e| panic!("{} at {:?} pushed {:?}: {e}", item.victim, item.at, item.dir));
    }
    (hazard, moved)
}

fn run_items(name: &str, items: &[Item]) {
    for (arm, cfg) in [("shipped ladder", config()), ("fixed_distance slide", symmetric_config())] {
        let (mut hazards, mut moved) = (0, 0);
        for it in items {
            let (h, m) = run_item(it, &cfg);
            hazards += usize::from(h);
            moved += usize::from(m);
        }
        println!("{name} ({arm}): {} pushes, {hazards} aimed at an illegal destination, {moved} pushed", items.len());
        assert!(hazards * 2 >= items.len(), "{name} ({arm}): vacuous: only {hazards} of {} pushes aimed at a hazard", items.len());
        assert!(moved * 2 >= items.len(), "{name} ({arm}): only {moved} of {} victims were pushed at all", items.len());
    }
}

#[test]
fn sweep_river_banks_and_bridges() {
    let s = BattleState::new(1, config());
    let a = s.arena().clone();
    let mut items = Vec::new();
    for victim in ["Knight", "Giant"] {
        let r = card_stat(&s, victim).collision_radius;
        // Both banks, every tile column, pushed straight and diagonally into the river.
        for xt in 0..18 {
            let x = xt * SUBTILE + SUBTILE / 2;
            for (y, dy) in [(a.water_y_min - r / 2 - 1, 1), (a.water_y_max + r / 2 + 1, -1)] {
                let at = Vec2::new(x, y);
                if !a.is_passable_ground(at) {
                    continue;
                }
                for dx in [0, 3, -3] {
                    let dir = if dx == 0 { (0, dy) } else { (dx, 4 * dy) };
                    items.push(Item { victim, at, dir });
                }
            }
        }
        // Both bridges: on the deck, pushed sideways into the water.
        for lane in [Lane::Left, Lane::Right] {
            let b = a.bridge(lane);
            for y in [a.water_y_min + SUBTILE / 2, (a.water_y_min + a.water_y_max) / 2, a.water_y_max - SUBTILE / 2] {
                for (x, dx) in [(b.x_min + milli(200), -1), (b.x_max - milli(200), 1), (b.center_x, 1), (b.center_x, -1)] {
                    let at = Vec2::new(x, y);
                    if a.is_passable_ground(at) {
                        items.push(Item { victim, at, dir: (dx, 0) });
                    }
                }
            }
        }
    }
    assert!(items.len() >= 150, "vacuous sweep: {} items", items.len());
    run_items("river banks and bridges", &items);
}

#[test]
fn sweep_arena_edges() {
    let s = BattleState::new(1, config());
    let a = s.arena().clone();
    let mut items = Vec::new();
    for victim in ["Knight", "Giant"] {
        let r = card_stat(&s, victim).collision_radius;
        for yt in [9, 11, 13, 19, 21, 23] {
            let y = yt * SUBTILE;
            items.push(Item { victim, at: Vec2::new(r, y), dir: (-1, 0) });
            items.push(Item { victim, at: Vec2::new(a.width - r, y), dir: (1, 0) });
            items.push(Item { victim, at: Vec2::new(r, y), dir: (-3, 4) });
            items.push(Item { victim, at: Vec2::new(a.width - r, y), dir: (3, -4) });
        }
        for xt in [1, 6, 12, 17] {
            let x = xt * SUBTILE;
            items.push(Item { victim, at: Vec2::new(x, r), dir: (0, -1) });
            items.push(Item { victim, at: Vec2::new(x, a.height - r), dir: (0, 1) });
        }
    }
    run_items("arena edges", &items);
}

#[test]
fn sweep_building_footprints() {
    // Deploying victims ringed around a crown tower and around the king, the push
    // pointed at the building's centre. (Cannons are covered by settle_*.)
    let s = BattleState::new(1, config());
    let a = s.arena().clone();
    let mut items = Vec::new();
    let towers = [
        (a.princess_tower_pos(Team::Red, Lane::Left), card_stat(&s, "PrincessTower").collision_radius),
        (a.princess_tower_pos(Team::Blue, Lane::Right), card_stat(&s, "PrincessTower").collision_radius),
        (a.king_tower_pos(Team::Red), card_stat(&s, "KingTower").collision_radius),
    ];
    for victim in ["Knight", "Giant"] {
        let r = card_stat(&s, victim).collision_radius;
        for (c, big) in towers {
            // Beyond the largest footprint candidate (a box of half-size ~1.5 tiles).
            let ring = big.max(3 * SUBTILE / 2) + r + milli(200);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (3, 4), (-3, 4), (3, -4), (-3, -4)] {
                let n = if dx != 0 && dy != 0 { 5 } else { 1 };
                let at = Vec2::new(c.x + dx * ring / n, c.y + dy * ring / n);
                if a.is_passable_ground(at) && at.x > r && at.x < a.width - r {
                    items.push(Item { victim, at, dir: (-dx, -dy) });
                }
            }
        }
    }
    assert!(items.len() >= 30, "vacuous sweep: {} items", items.len());
    run_items("building footprints", &items);
}

/// One run of the two-Fireball scene: `casts` in that order, the Red Knight dropped
/// on tick 25. Returns the landing tick, the state_hash per tick from the landing on,
/// the Knight's position track over the same ticks, and (position, deploying) just
/// before the landing.
#[allow(clippy::type_complexity)]
fn two_pushes(cfg: &BattleConfig, casts: &[Vec2], knight_at: Vec2, expect_in_flight: usize) -> (Option<u32>, Vec<u64>, Vec<Vec2>, Option<(Vec2, bool)>) {
    let mut s = BattleState::new(1, cfg.clone());
    for &p in casts {
        s.spawn_unit(Team::Blue, "Fireball", p, None).unwrap();
    }
    let (mut hashes, mut track) = (Vec::new(), Vec::new());
    let mut landed = None;
    let mut before = None;
    for k in 0..80u32 {
        if k == 25 {
            s.spawn_unit(Team::Red, "Knight", knight_at, None).unwrap();
        }
        let in_flight = s.spells().len();
        s.tick();
        if landed.is_none() && s.spells().is_empty() && k > 0 {
            assert_eq!(in_flight, expect_in_flight, "the Fireballs did not land on one tick");
            landed = Some(k);
        }
        if landed.is_none() {
            before = find_live(&s, Team::Red, "Knight").first().map(|e| (e.pos, e.deploying));
        } else {
            hashes.push(s.state_hash());
            track.push(find_live(&s, Team::Red, "Knight").first().map(|e| e.pos).expect("the Knight lives"));
        }
    }
    (landed, hashes, track, before)
}

#[test]
fn two_pushes_on_one_unit_in_one_tick_follow_the_stacking_rule_of_each_arm() {
    // Two Blue Fireballs landing on the SAME tick (impacts mirror-symmetric about the
    // Blue king's x, so the flights are equally long), both covering a deploying Red
    // Knight on the far bank, cast in both orders. Under the fixed_distance slide the
    // pushes SUM to straight down the bank into the river (settle takes part) and the
    // two orders hash identically from the landing on (knockback.STACKING =
    // vector_sum; plant knock_last_wins). Under the shipped ladder the FIRST push of
    // the buffer -- the first cast -- arms the ladder and the second is refused by the
    // gate (STACKING = first_wins_while_active), so the
    // Knight's track under [f1, f2] is f1's alone and under [f2, f1] f2's alone, and
    // the two orders part.
    let s0 = BattleState::new(1, config());
    let a = s0.arena().clone();
    let kx = a.king_tower_pos(Team::Blue).x;
    let knight_at = Vec2::new(kx, a.water_y_max + SUBTILE);
    let f1 = Vec2::new(kx - SUBTILE, knight_at.y + SUBTILE);
    let f2 = Vec2::new(kx + SUBTILE, knight_at.y + SUBTILE);

    // the fixed_distance slide: commutative
    let cfg = symmetric_config();
    assert_eq!(cfg.calib.knock_law, KnockLaw::FixedDistance);
    let (l12, h12, _, before) = two_pushes(&cfg, &[f1, f2], knight_at, 2);
    let (l21, h21, t21, _) = two_pushes(&cfg, &[f2, f1], knight_at, 2);
    assert!(before.is_some_and(|b| b.1), "the Knight must be deploying when both land (landed {l12:?}, before {before:?})");
    assert_eq!(l12, l21);
    assert_eq!(h12, h21, "vector_sum: cast order changed the battle after two simultaneous pushes");
    assert_ne!(t21.last().copied(), before.map(|b| b.0), "vacuous: the Knight was not displaced");

    // the shipped ladder: the first cast wins, the second is refused
    let cfg = config();
    assert_eq!(cfg.calib.knock_law, KnockLaw::Client16402, "re-point this test at the new shipped law");
    let (l12, _, t12, before) = two_pushes(&cfg, &[f1, f2], knight_at, 2);
    let (l21, _, t21, _) = two_pushes(&cfg, &[f2, f1], knight_at, 2);
    let (l1, _, t1, _) = two_pushes(&cfg, &[f1], knight_at, 1);
    let (l2, _, t2, _) = two_pushes(&cfg, &[f2], knight_at, 1);
    assert!(before.is_some_and(|b| b.1), "the Knight must be deploying when both land");
    assert_eq!((l12, l21), (l1, l2), "the single casts land on the pair's tick");
    assert_eq!(t12, t1, "first_wins_while_active: [f1, f2] is f1 alone");
    assert_eq!(t21, t2, "first_wins_while_active: [f2, f1] is f2 alone");
    assert_ne!(t12, t21, "the two orders push the Knight different ways");
    assert_ne!(t12.last().copied(), before.map(|b| b.0), "vacuous: the Knight was not displaced");
}
