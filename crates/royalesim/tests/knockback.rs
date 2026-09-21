//! KNOCKBACK SAFETY -- a push must never leave a ground unit on water, outside the
//! arena, or with its centre inside a building footprint; and two pushes landing on
//! one unit in one tick must not depend on the order they were buffered in.
//!
//! WHY IT EXISTS: walking can never put a unit on water (collide::dry_position) or
//! inside a footprint (collision push-out), and every other gate trusts that. A
//! knockback is NOT blocked by the river (docs/spell-spec.md: the Log pushes units
//! "over the river"; calibration knockback.WATER_RESOLUTION = eject_to_nearest_land),
//! so it is the first mechanic that can displace a unit to a place no walk reaches.
//! `spell::settle` resolves it; this file is the measurement that the resolution
//! holds everywhere a push can go wrong: both river banks, both bridges, the arena
//! edges, and building footprints (a Cannon, a princess tower, a king tower).
//!
//! THE CHECKS
//!   1. `settle_*`: settle itself, exhaustively: every start point on a quarter-tile
//!      grid of legal ground, pushed 8 ways at both shipped Pushback distances.
//!   2. `sweep_*`: the same hazards through the real tick loop -- a Fireball cast on
//!      the dry side of a deploying Knight or Giant so the radial push points INTO
//!      the hazard -- with the every-tick invariants (common::Invariants) running for
//!      20 ticks after the push. Vacuity: most pushes must have had an illegal
//!      unresolved destination, or the sweep proved nothing.
//!   3. `two_pushes_*`: two Fireballs landing on one unit in one tick, cast in both
//!      orders, give the same state_hash (registry knockback.STACKING = vector_sum).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant
//! cargo test --test knockback`):
//!   no_water_resolution       -> settle_*, sweep_river_banks_and_bridges, sweep_building_footprints
//!                                (it returns before the footprint push-out too)
//!   knock_unclamped           -> settle_*, sweep_arena_edges. (Removing only the
//!                                clamp does NOT land: the water ejection also keeps
//!                                a push in bounds, so the plant has to bypass both.)
//!   knock_ignores_footprints  -> settle_*, sweep_building_footprints
//!   knock_last_wins           -> two_pushes_*
//!
//! WHAT IT CANNOT CATCH: whether the real game ejects, clamps or continues a push
//! across the river (knockback.WATER_RESOLUTION is a guess); pushes of FLYING units
//! (they may legally end over water; only the arena clamp applies).
mod common;

use royalesim::arena::Lane;
use royalesim::entity::EntityKind;
use royalesim::fixed::{milli, Vec2, SUBTILE};
use royalesim::path::Obstacle;
use royalesim::state::{footprint_of, BattleState};
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
/// UNRESOLVED destination was illegal (a hazard) and whether the victim moved.
fn run_item(item: &Item) -> (bool, bool) {
    let mut s = BattleState::new(1, config());
    let push = pushbacks()[0];
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
                moved = s.entity(id).is_some_and(|v| v.pos != p);
            }
        }
        inv.check(&s).unwrap_or_else(|e| panic!("{} at {:?} pushed {:?}: {e}", item.victim, item.at, item.dir));
    }
    (hazard, moved)
}

fn run_items(name: &str, items: &[Item]) {
    let (mut hazards, mut moved) = (0, 0);
    for it in items {
        let (h, m) = run_item(it);
        hazards += usize::from(h);
        moved += usize::from(m);
    }
    println!("{name}: {} pushes, {hazards} aimed at an illegal destination, {moved} displaced", items.len());
    assert!(hazards * 2 >= items.len(), "{name}: vacuous: only {hazards} of {} pushes aimed at a hazard", items.len());
    assert!(moved * 2 >= items.len(), "{name}: only {moved} of {} victims were displaced at all", items.len());
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

#[test]
fn two_pushes_on_one_unit_in_one_tick_do_not_depend_on_cast_order() {
    // Two Blue Fireballs landing on the SAME tick (impacts mirror-symmetric about the
    // Blue king's x, so the flights are equally long), both covering a deploying Red
    // Knight on the far bank, cast in both orders. The pushes sum to straight down the
    // bank into the river, so settle takes part. From the landing tick on, the two
    // battles must hash identically (registry knockback.STACKING = vector_sum).
    // Plant: knock_last_wins.
    let s0 = BattleState::new(1, config());
    let a = s0.arena().clone();
    let kx = a.king_tower_pos(Team::Blue).x;
    let knight_at = Vec2::new(kx, a.water_y_max + SUBTILE);
    let f1 = Vec2::new(kx - SUBTILE, knight_at.y + SUBTILE);
    let f2 = Vec2::new(kx + SUBTILE, knight_at.y + SUBTILE);
    let mut runs = Vec::new();
    for order in [[f1, f2], [f2, f1]] {
        let mut s = BattleState::new(1, config());
        for p in order {
            s.spawn_unit(Team::Blue, "Fireball", p, None).unwrap();
        }
        let mut per_tick = Vec::new();
        let mut landed = None;
        let mut before = None;
        for k in 0..80u32 {
            if k == 25 {
                s.spawn_unit(Team::Red, "Knight", knight_at, None).unwrap();
            }
            let in_flight = s.spells().len();
            s.tick();
            if landed.is_none() && s.spells().is_empty() && k > 0 {
                assert_eq!(in_flight, 2, "the two Fireballs did not land on one tick");
                landed = Some(k);
            }
            if landed.is_none() {
                before = find_live(&s, Team::Red, "Knight").first().map(|e| (e.pos, e.deploying));
            } else {
                per_tick.push(s.state_hash());
            }
        }
        let after = find_live(&s, Team::Red, "Knight").first().map(|e| e.pos);
        runs.push((landed, per_tick, before, after));
    }
    let (landed, _, before, _) = &runs[0];
    assert!(before.is_some_and(|b| b.1), "the Knight must be deploying when both land (landed {landed:?}, before {before:?})");
    assert_eq!(runs[0].0, runs[1].0);
    assert_eq!(runs[0].1, runs[1].1, "cast order changed the battle after two simultaneous pushes");
    assert_ne!(runs[0].3, before.map(|b| b.0), "vacuous: the Knight was not displaced");
}
