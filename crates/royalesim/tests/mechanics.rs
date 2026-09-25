//! Mechanic tests for the modules that had none: target.rs, collide.rs,
//! path.rs, combat.rs -- plus the local unit avoidance added after the first
//! battles deadlocked.
//!
//! Every number a test needs (ranges, radii, sight, masses, damage) is READ from
//! data/derived/cards.json or calibration.json. Positions are scenario geometry.
//! Each test names the plant that must turn it red.
#![allow(unexpected_cfgs)]
mod common;

use royalesim::arena::Arena;
use royalesim::card::{CardDb, CardSource};
use royalesim::collide::{self, CollideScratch};
use royalesim::entity::{AttackPhase, Entities, EntityKind, SpatialHash, SpawnInit};
use royalesim::fixed::{isqrt, milli, tiles, Vec2, SUBTILE};
use royalesim::path::{self, FrameWorld, NavRequest};
use royalesim::state::{BattleConfig, BattleState, Calib};
use royalesim::target::in_attack_range;
use royalesim::{EntityId, PathModel, PushModel, Rng, Team};
use common::*;

/// WHICH CROWN TOWER A ROUTE IS AIMED AT.
///
/// PathModel::Oracle2026 stores its route GOAL-FIRST (calibration
/// pathfinding.PATH_NODE_ENCODING) and its goal node is a half-tile CELL CENTRE
/// within Range + CollisionRadius of the target's centre -- not the target's own
/// position -- so `route.last() == tower.pos` (the start-first convention, where a
/// route ended exactly on its target) does not identify it. The tower is still
/// unambiguous: the candidates are 11 tiles apart and the goal cell is within 1.5
/// tiles of the one aimed at.
fn aimed_tower(s: &BattleState, route: &[Vec2], towers: &[Option<EntityId>; 3]) -> Option<EntityId> {
    let goal = *route.first()?;
    towers
        .iter()
        .filter_map(|t| *t)
        .filter_map(|id| s.entity(id).map(|e| (goal.dist2(e.pos), id)))
        .min_by_key(|(d, _)| *d)
        .map(|(_, id)| id)
}

fn id_of(s: &BattleState, team: Team, card: &str) -> EntityId {
    let v = find_live(s, team, card);
    assert_eq!(v.len(), 1, "expected exactly one live {team:?} {card}, found {}", v.len());
    v[0].id
}

/// A PLAIN attacking building that is not the Cannon, for scenes that need a second
/// building target: no hide (a Tesla goes under), no spawner, no death spawn. The
/// 2018 Bomb Tower; in 15.535 the Bomb Tower death-spawns a bomb (loaded as a timed
/// impact, card.rs `convert_death_bomb`), so it is no longer plain and the next row
/// stands in (the Inferno Tower: its damage ramp is not simulated, but here it is
/// only a target).
fn plain_building(s: &BattleState) -> &'static str {
    let db = s.cards();
    ["BombTower", "InfernoTower", "Mortar", "Xbow"]
        .into_iter()
        .find(|n| db.index(n).is_some_and(|i| { let c = db.get(i); c.hide.is_none() && c.spawner.is_none() && c.death_spawn.is_none() && c.damage > 0 }))
        .expect("no plain building loads from cards.json")
}

fn tick_n(s: &mut BattleState, n: u32) {
    for _ in 0..n {
        s.tick();
    }
}

/// Ticks of deploy time for a card, from the data.
fn deploy_ticks(s: &BattleState, card: &str) -> u32 {
    (card_stat(s, card).deploy_time_ms / s.config().calib.tick_ms) as u32
}

// ===========================================================================
// target.rs

#[test]
fn range_is_edge_to_edge_1600_vs_radius_500() {
    // The spec's own numbers: Range 1600 against a 500-radius target reaches at a
    // centre distance of 2.1 tiles and not at 2.2. Plant: centre_range.
    let c = Calib::shipped();
    assert!(c.add_character_range_to_radius, "globals ship ADD_CHARACTER_RANGE_TO_RADIUS = TRUE");
    let a = Vec2::new(0, 0);
    // a POINT attacker (own radius 0): the target's radius alone
    assert!(in_attack_range(&c, a, milli(1600), 0, Vec2::from_tiles_100(210, 0), milli(500)));
    assert!(!in_attack_range(&c, a, milli(1600), 0, Vec2::from_tiles_100(220, 0), milli(500)));
    // exactly at the boundary, and one subtile past it
    let edge = milli(1600) + milli(500);
    assert!(in_attack_range(&c, a, milli(1600), 0, Vec2::new(edge, 0), milli(500)));
    assert!(!in_attack_range(&c, a, milli(1600), 0, Vec2::new(edge + 1, 0), milli(500)));
    // the attacker's OWN radius is part of its reach (targeting.ATTACK_RANGE_RULE =
    // range_plus_both_radii): a 600-radius attacker reaches 600 farther.
    // Plant: reach_without_own_radius.
    let both = edge + milli(600);
    assert!(in_attack_range(&c, a, milli(1600), milli(600), Vec2::new(both, 0), milli(500)));
    assert!(!in_attack_range(&c, a, milli(1600), milli(600), Vec2::new(both + 1, 0), milli(500)));
}

#[test]
fn melee_unit_attacks_from_range_plus_both_radii_without_moving() {
    // In a battle: a Knight placed exactly (range + its own radius + the target's
    // radius) from a Cannon centre attacks without taking a step; one subtile
    // farther and it walks (targeting.ATTACK_RANGE_RULE; before it the reach was
    // range + the target's radius alone).
    // Plants: centre_range, reach_without_own_radius.
    for extra in [0, 1] {
        let mut s = BattleState::new(1, config());
        let knight = card_stat(&s, "Knight").range;
        let knight_r = card_stat(&s, "Knight").collision_radius;
        let cannon_r = card_stat(&s, "Cannon").collision_radius;
        let c = t(900, 2200);
        let k = Vec2::new(c.x, c.y - knight - knight_r - cannon_r - extra);
        s.spawn_unit(Team::Red, "Cannon", c, None).unwrap();
        s.spawn_unit(Team::Blue, "Knight", k, None).unwrap();
        let n = deploy_ticks(&s, "Knight") + 2;
        tick_n(&mut s, n);
        let kv = find_live(&s, Team::Blue, "Knight")[0];
        if extra == 0 {
            assert_eq!(kv.pos, k, "Knight in edge range moved");
            assert_ne!(kv.attack_phase, AttackPhase::Idle, "Knight in edge range did not attack");
        } else {
            assert_ne!(kv.pos, k, "Knight one subtile out of edge range did not move");
        }
    }
}

#[test]
fn giant_targets_only_buildings_and_never_hits_a_troop() {
    // Plant: remove target_only_buildings from can_target (giant_hits_troops).
    let mut s = BattleState::new(1, config());
    s.spawn_unit(Team::Blue, "Giant", t(900, 2000), None).unwrap();
    s.spawn_unit(Team::Red, "Knight", t(900, 2110), None).unwrap();
    s.spawn_unit(Team::Red, "Cannon", t(900, 2500), None).unwrap();
    let mut targeted_building = false;
    // A TARGET CAN BE DEAD FOR A TICK: the field is cleared in the next Target phase, not when
    // the target dies. This read `s.entity(tid).expect("target alive")` and failed the day the
    // Giant first killed its Cannon inside the 400 ticks (movement.ATTACKING_UNIT_MOVEMENT =
    // separation_only moved it). Skipping a dead target would let a Giant that targeted a TROOP
    // which then died pass unseen, so a dead target must be one already verified as a building
    // while it was alive.
    let mut verified_buildings = std::collections::BTreeSet::new(); // EntityId is Ord, not Hash
    for _ in 0..400 {
        s.tick();
        let Some(g) = find_live(&s, Team::Blue, "Giant").first().copied() else { break };
        if let Some(tid) = g.target {
            match s.entity(tid) {
                Some(tv) => {
                    assert!(tv.kind.is_building(), "Giant targeted a {} ({:?})", tv.card, tv.kind);
                    verified_buildings.insert(tid);
                    targeted_building = true;
                }
                None => assert!(verified_buildings.contains(&tid), "the Giant holds a dead target it was never seen to hold alive, so its kind was never checked"),
            }
        }
        if let Some(k) = find_live(&s, Team::Red, "Knight").first() {
            assert_eq!(k.hp, k.max_hp, "the Knight lost hp; only the Giant could have hit it");
        }
    }
    assert!(targeted_building, "the Giant never targeted anything -- vacuous");
}

/// Place a Cannon so its EDGE distance from `from` is `edge` (subtiles), straight
/// ahead in +y. Returns its centre.
fn cannon_at_edge(s: &BattleState, from: Vec2, edge: i32) -> Vec2 {
    Vec2::new(from.x, from.y + edge + card_stat(s, "Cannon").collision_radius)
}

#[test]
fn hog_rider_retargets_to_a_cannon_exactly_when_it_enters_sight() {
    // Hog walking the right lane. Case IN: a Cannon placed 0.25 tile INSIDE its
    // sight (+ EXTRA_SIGHT_RANGE_TO_BUILDING) is its target on the very next
    // Target phase. Case OUT: placed 0.25 tile OUTSIDE, it is NOT targeted while
    // it stays outside, and becomes the target on the tick it enters.
    // Plant: sight_ignored (scan ignores sight).
    let base = {
        let mut s = BattleState::new(3, config());
        s.spawn_unit(Team::Blue, "HogRider", t(1450, 1000), None).unwrap();
        let n = deploy_ticks(&s, "HogRider") + 5;
        tick_n(&mut s, n);
        s
    };
    let hog0 = find_live(&base, Team::Blue, "HogRider")[0];
    assert!(hog0.target.is_none(), "precondition: Hog has no target yet (towers out of sight)");
    let calib = base.config().calib.clone();
    // the sight scan sums SightRange + the extra + the HOG's own radius + the
    // candidate's (targeting.ATTACK_RANGE_RULE; `cannon_at_edge` adds the Cannon's)
    let sight = card_stat(&base, "HogRider").sight_range + calib.extra_sight_range_to_building + hog0.radius;
    let quarter = SUBTILE / 4;

    // IN
    let mut s = base.clone();
    let c_in = cannon_at_edge(&s, hog0.pos, sight - quarter);
    s.spawn_unit(Team::Red, "Cannon", c_in, None).unwrap();
    s.tick();
    let hog = find_live(&s, Team::Blue, "HogRider")[0];
    let cannon = id_of(&s, Team::Red, "Cannon");
    assert_eq!(hog.target, Some(cannon), "Cannon 0.25 tile inside sight was not targeted");

    // OUT
    let mut s = base.clone();
    let c_out = cannon_at_edge(&s, hog0.pos, sight + quarter);
    s.spawn_unit(Team::Red, "Cannon", c_out, None).unwrap();
    let cannon_r = card_stat(&s, "Cannon").collision_radius;
    let mut outside_ticks = 0;
    let mut entered_tick = None;
    for _ in 0..40 {
        // position used by the coming Target phase = position now
        let hog_before = find_live(&s, Team::Blue, "HogRider")[0].pos;
        let inside = in_attack_range(&calib, hog_before, sight - hog0.radius, hog0.radius, c_out, cannon_r);
        s.tick();
        let hog = find_live(&s, Team::Blue, "HogRider")[0];
        let cannon = id_of(&s, Team::Red, "Cannon");
        if inside {
            assert_eq!(hog.target, Some(cannon), "Cannon entered sight but the Hog did not retarget");
            entered_tick = Some(s.tick_count());
            break;
        }
        assert_ne!(hog.target, Some(cannon), "Hog targeted a Cannon outside its sight range");
        outside_ticks += 1;
    }
    assert!(outside_ticks >= 1, "the OUT case was never outside -- vacuous");
    assert!(entered_tick.is_some(), "the Hog never walked the Cannon into sight -- vacuous");
}

#[test]
fn target_is_locked_once_the_windup_has_started() {
    // Knight winding up on a Cannon. Mid-windup the Cannon is moved 0.5 tile
    // beyond range (past LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET, inside
    // LOGIC_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE) and a ~~Tesla~~ Bomb Tower is nearer:
    // the lock must hold and the swing must land on the Cannon. Moved 2 tiles beyond
    // (past the cancel range) the windup must be cancelled with no damage.
    // Not a Tesla: a Tesla hides (tests/hide.rs) and cannot be the
    // nearer target this scene needs; the Bomb Tower is a plain building.
    // Plant: no_target_lock.
    let calib = Calib::shipped();
    assert!(calib.preserve_target_if_hit_started);
    let half = SUBTILE / 2;
    assert!(milli(25) < half && half < calib.cancel_hit_from_long_distance_range, "scenario assumes keep-ext < 0.5 tile < cancel range");
    for (beyond, expect_hit) in [(half, true), (2 * SUBTILE, false)] {
        let mut s = BattleState::new(1, config());
        // the reach is Range + the Knight's own radius + the target's
        // (targeting.ATTACK_RANGE_RULE = range_plus_both_radii)
        let range = card_stat(&s, "Knight").range + card_stat(&s, "Knight").collision_radius;
        let cr = card_stat(&s, "Cannon").collision_radius;
        let building = plain_building(&s);
        let tr = card_stat(&s, building).collision_radius;
        let k = t(900, 2000);
        let c = Vec2::new(k.x, k.y + range + cr);
        // The plain building nearer than the moved Cannon will be, but out of range now.
        let tesla = Vec2::new(k.x + range + tr + SUBTILE * 3 / 10, k.y);
        s.spawn_unit(Team::Red, "Cannon", c, None).unwrap();
        s.spawn_unit(Team::Red, building, tesla, None).unwrap();
        s.spawn_unit(Team::Blue, "Knight", k, None).unwrap();
        let cannon = {
            s.tick();
            id_of(&s, Team::Red, "Cannon")
        };
        // Wait for the windup.
        let started = run_until(&mut s, 200, |s| find_live(s, Team::Blue, "Knight")[0].attack_phase == AttackPhase::Windup);
        assert!(started < 200, "Knight never wound up");
        let kv = find_live(&s, Team::Blue, "Knight")[0];
        assert_eq!(kv.target, Some(cannon));
        assert!(kv.target_locked, "windup started but target_locked is false");
        let hp0 = s.entity(cannon).unwrap().hp;
        assert!(s.debug_set_pos(cannon, Vec2::new(c.x, c.y + beyond)));
        // Run through the rest of the windup.
        let load_ticks = (card_stat(&s, "Knight").load_time_ms / calib.tick_ms) as u32 + 1;
        // A Cannon is a LifeTime building, so its hp falls on its own every tick
        // (lifetime.HP_DECAY = linear_drain). Only a drop bigger than the whole
        // window's drain is the Knight's hit.
        let drift = load_ticks as i32 * ((s.lifetime_drain(cannon) + 99) / 100);
        let mut hit = false;
        for _ in 0..load_ticks {
            s.tick();
            let kv = find_live(&s, Team::Blue, "Knight")[0];
            if s.entity(cannon).map_or(true, |v| v.hp < hp0 - drift) {
                hit = true;
                break;
            }
            if expect_hit {
                assert_eq!(kv.target, Some(cannon), "lock broke at 0.5 tile beyond range (tick {})", s.tick_count());
            }
        }
        assert_eq!(hit, expect_hit, "beyond={beyond}: hit={hit}");
        if !expect_hit {
            let kv = find_live(&s, Team::Blue, "Knight")[0];
            assert_ne!(kv.target, Some(cannon), "windup should have been cancelled and the target dropped");
        }
    }
}

#[test]
fn default_tower_follows_lane_by_x_and_falls_back_to_the_king() {
    // A Knight with nothing in sight walks to the enemy princess tower on its x
    // side (LOGIC_XPOS_BASED_TOWER_TARGETING), and to the king once that tower
    // is down. Centre line x = 9 goes Left. Plant: default_tower_nearest.
    let red_towers = BattleState::new(1, config()).tower_ids(Team::Red);
    for (x100, slot) in [(350, 1usize), (1450, 2), (900, 1), (910, 2)] {
        let mut s = BattleState::new(1, config());
        s.spawn_unit(Team::Blue, "Knight", t(x100, 900), None).unwrap();
        let n = deploy_ticks(&s, "Knight") + 2;
        tick_n(&mut s, n);
        let kv = find_live(&s, Team::Blue, "Knight")[0];
        assert!(kv.target.is_none(), "precondition: nothing in sight");
        assert_eq!(
            aimed_tower(&s, kv.route, &red_towers),
            red_towers[slot],
            "Knight at x={x100} should head for tower slot {slot}"
        );
        // Tower down: fall back to the king.
        assert!(s.debug_set_hp(red_towers[slot].unwrap(), 0));
        tick_n(&mut s, 2);
        let kv = find_live(&s, Team::Blue, "Knight")[0];
        assert_eq!(
            aimed_tower(&s, kv.route, &red_towers),
            red_towers[0],
            "x={x100}: princess down, should head for the king"
        );
    }
    assert!(red_towers.iter().all(|t| t.is_some()));
}

#[test]
fn default_tower_is_decided_in_the_units_own_frame() {
    // The seat rotation: a RED Knight's own-left is the
    // engine's right. Off the centre line nothing changes -- a Red Knight at engine
    // x = 14.5 heads for the Blue tower on that side (engine-Right, slot 2). A
    // Knight EXACTLY on x = 9 goes to its OWN left: engine-Left (slot 1) for Blue
    // (the test above), engine-Right (slot 2) for Red.
    // "Centre line x = 9 goes Left for both seats" is the reflection-only rule, and
    // is what the plant below restores.
    // Plant: reflection_centre_lane (turns the Red x = 9 row red).
    let blue_towers = BattleState::new(1, config()).tower_ids(Team::Blue);
    for (x100, slot) in [(350, 1usize), (1450, 2), (900, 2), (890, 1)] {
        let mut s = BattleState::new(1, config());
        s.spawn_unit(Team::Red, "Knight", t(x100, 2300), None).unwrap();
        // Check the route on the FIRST tick it exists, while the Knight still
        // stands where it was placed. Waiting even two ticks past the deploy
        // hollows this out: by then a centre-line Knight has stepped off x = 9 and
        // re-planned from its new x, and the `reflection_centre_lane` plant no
        // longer lands here at all.
        let placed = t(x100, 2300);
        let ran = run_until(&mut s, 200, |s| find_live(s, Team::Red, "Knight").first().is_some_and(|k| !k.route.is_empty()));
        assert!(ran < 200, "the Knight never planned a route");
        let kv = find_live(&s, Team::Red, "Knight")[0];
        // Planned from the placed position; the same Path phase then took one step.
        assert!(kv.pos.dist(placed) <= kv.speed, "precondition: planned from {placed:?}, now at {:?}", kv.pos);
        assert!(kv.target.is_none(), "precondition: nothing in sight");
        assert_eq!(
            aimed_tower(&s, kv.route, &blue_towers),
            blue_towers[slot],
            "Red Knight at x={x100} should head for Blue tower slot {slot}"
        );
    }
}

// ===========================================================================
// collide.rs

fn unit(team: Team, label: i32, pos: Vec2, radius: i32, mass: i32, speed: i32) -> SpawnInit {
    SpawnInit {
        team,
        kind: EntityKind::Troop,
        card: 0,
        level: label,
        pos,
        hp: 100,
        shield: 0,
        damage: 0,
        death_damage: 0,
        radius,
        mass: Some(mass),
        speed,
        flying: false,
        deploy_ms: 0,
        spawn_tick: 0,
    }
}

fn separate_once(ents: &mut Entities, push: PushModel) {
    let arena = Arena::shipped();
    let mut hash = SpatialHash::new(arena.width, arena.height, SUBTILE);
    hash.rebuild(ents);
    let mut scratch = CollideScratch::default();
    collide::separate(ents, &mut hash, &arena, &[], push, 1, &mut scratch);
}

#[test]
fn mass_weighted_golem_barely_moves_skeleton_moves_far() {
    // Plant: mass_share_inverted.
    let db = cards();
    let g = db.get(db.index("Golem").unwrap());
    let k = db.get(db.index("Skeletons").unwrap());
    let (gm, km) = (g.mass.unwrap(), k.mass.unwrap());
    assert!(gm > 5 * km, "data: Golem mass {gm} vs Skeleton {km}");
    let gp = t(900, 1000);
    let overlap = SUBTILE / 4;
    let kp = Vec2::new(gp.x, gp.y + g.collision_radius + k.collision_radius - overlap);
    let mut e = Entities::new();
    let gi = e.spawn(unit(Team::Blue, 1, gp, g.collision_radius, gm, 1));
    let ki = e.spawn(unit(Team::Red, 2, kp, k.collision_radius, km, 1));
    separate_once(&mut e, PushModel::MassWeighted);
    let gd = e.pos[gi.index as usize].dist(gp);
    let kd = e.pos[ki.index as usize].dist(kp);
    println!("MassWeighted: Golem moved {gd}, Skeleton moved {kd} (overlap {overlap}, masses {gm}/{km})");
    assert!(kd > 5 * gd.max(1), "Skeleton {kd} should move far more than Golem {gd}");
    // Shares are m_other / (m_i + m_j) of the overlap, truncated.
    let expect_k = (overlap as i64 * gm as i64 / (gm + km) as i64) as i32;
    assert!((kd - expect_k).abs() <= 2, "Skeleton moved {kd}, expected ~{expect_k}");
}

#[test]
fn equal_split_moves_both_equally() {
    let db = cards();
    let g = db.get(db.index("Golem").unwrap());
    let k = db.get(db.index("Skeletons").unwrap());
    let gp = t(900, 1000);
    let overlap = SUBTILE / 4;
    let kp = Vec2::new(gp.x, gp.y + g.collision_radius + k.collision_radius - overlap);
    let mut e = Entities::new();
    let gi = e.spawn(unit(Team::Blue, 1, gp, g.collision_radius, g.mass.unwrap(), 1));
    let ki = e.spawn(unit(Team::Red, 2, kp, k.collision_radius, k.mass.unwrap(), 1));
    separate_once(&mut e, PushModel::EqualSplit);
    let gd = e.pos[gi.index as usize].dist(gp);
    let kd = e.pos[ki.index as usize].dist(kp);
    assert_eq!(gd, kd, "EqualSplit: Golem {gd} vs Skeleton {kd}");
    assert!(gd > 0);
}

#[test]
fn separation_is_independent_of_insertion_order() {
    // 40 overlapping units, inserted in 6 different orders: every unit (tracked
    // by a label) must end at the same position. Plant: sequential_separation.
    let mut rng = Rng::new(4242);
    let arena = Arena::shipped();
    let mut specs: Vec<(Team, i32, Vec2, i32, i32)> = Vec::new();
    let mut label = 0;
    while specs.len() < 40 {
        let p = Vec2::new(rng.range(tiles(6), tiles(12)), rng.range(tiles(8), tiles(12)));
        if !arena.is_passable_ground(p) || specs.iter().any(|s| s.2 == p) {
            continue;
        }
        let team = if rng.below(2) == 0 { Team::Blue } else { Team::Red };
        specs.push((team, label, p, milli(300 + 100 * rng.range(1, 5)), rng.range(1, 20)));
        label += 1;
    }
    let mut reference: Option<Vec<Vec2>> = None;
    let mut moved_any = false;
    for perm in 0..6 {
        let mut order: Vec<usize> = (0..specs.len()).collect();
        if perm > 0 {
            for i in (1..order.len()).rev() {
                let j = rng.below((i + 1) as u32) as usize;
                order.swap(i, j);
            }
        }
        // Team-seq tie-breaks only matter for coincident units, which the spec
        // list excludes; spawn per-team in the permuted order.
        for push in [PushModel::MassWeighted] {
            let mut e = Entities::new();
            for &k in &order {
                let (team, lbl, p, r, m) = specs[k];
                e.spawn(unit(team, lbl, p, r, m, 600));
            }
            for _ in 0..3 {
                separate_once(&mut e, push);
            }
            let mut by_label = vec![Vec2::default(); specs.len()];
            for i in e.live_indices() {
                by_label[e.level[i] as usize] = e.pos[i];
            }
            moved_any |= by_label.iter().zip(specs.iter()).any(|(a, s)| *a != s.2);
            match &reference {
                None => reference = Some(by_label),
                Some(r) => assert_eq!(r, &by_label, "insertion order {perm} changed the result"),
            }
        }
    }
    assert!(moved_any, "nothing overlapped -- vacuous");
}

// ===========================================================================
// local unit avoidance (path::avoid_units)

#[test]
fn opposing_giants_pass_each_other_on_a_bridge() {
    // Without local avoidance, two Giants meeting head-on at the bridge centre stand
    // there for the rest of the battle. Any difference between them (here a 23-tick
    // deploy offset) must let them pass. Plant: no_unit_avoidance.
    //
    // THE SCENARIO ONLY BITES UNDER THE CLIENT SEARCH: a frame-planned search runs in
    // each team's rotated frame, so the two Giants pick mirror-image bridge columns
    // and never meet at all. The game's search is absolute-grid (path16402.rs), and
    // under it they do meet.
    let mut s = BattleState::new(1, config());
    s.spawn_unit(Team::Blue, "Giant", t(350, 1100), None).unwrap();
    tick_n(&mut s, 23);
    let p = same_lane_opposite(&s, t(350, 1100));
    s.spawn_unit(Team::Red, "Giant", p, None).unwrap();
    let mid = s.arena().height / 2;
    let passed = run_until(&mut s, 1000, |s| {
        let b = find_live(s, Team::Blue, "Giant");
        let r = find_live(s, Team::Red, "Giant");
        !b.is_empty() && !r.is_empty() && b[0].pos.y > mid + SUBTILE && r[0].pos.y < mid - SUBTILE
    });
    assert!(passed < 1000, "the Giants never got past each other");
}

#[test]
fn hog_rider_is_not_bulldozed_back_across_the_river_by_a_giant() {
    // Found by running: a Giant (mass 18) pushed a head-on Hog Rider (mass 4)
    // nine tiles backwards. The Hog must never be driven more than 2 tiles back
    // from the farthest point it has reached, and must cross the river.
    // Plant: no_unit_avoidance (measured under it: pushed back 6.2 tiles by tick 600).
    let mut s = BattleState::new(1, config());
    s.spawn_unit(Team::Blue, "HogRider", t(350, 1100), None).unwrap();
    let p = same_lane_opposite(&s, t(350, 1100));
    s.spawn_unit(Team::Red, "Giant", p, None).unwrap();
    let mut max_y = 0;
    let mut worst_setback = 0;
    for _ in 0..900 {
        s.tick();
        if let Some(h) = find_live(&s, Team::Blue, "HogRider").first() {
            max_y = max_y.max(h.pos.y);
            worst_setback = worst_setback.max(max_y - h.pos.y);
            assert!(max_y - h.pos.y <= 2 * SUBTILE, "tick {}: Hog pushed back to y={} from its farthest y={max_y}", s.tick_count(), h.pos.y);
        }
    }
    println!("hog worst setback {worst_setback} subtiles");
    assert!(max_y > s.arena().height / 2 + SUBTILE, "the Hog never crossed the river");
}

#[test]
fn a_unit_pushed_into_a_footprint_is_pushed_back_out() {
    // The static pass of collide::separate. A Knight teleported into the middle
    // of a princess tower must be outside the footprint again after one tick,
    // and never inside one during a long walk into it.
    // Plant: no_building_block.
    //
    // THE FRAME-PLANNED ARM ONLY: this is the engine's own footprint push, not the
    // game's. As shipped (move16402.rs), a building carries no Mass, so the real
    // separation impulse moves a unit out of a footprint by ONE native unit per tick
    // and melee attackers stand inside a tower's box for the whole fight.
    let mut s = BattleState::new(1, symmetric_config());
    s.spawn_unit(Team::Blue, "Knight", t(350, 900), None).unwrap();
    let n = deploy_ticks(&s, "Knight") + 2;
    tick_n(&mut s, n);
    let k = id_of(&s, Team::Blue, "Knight");
    let tower = s.tower_ids(Team::Blue)[1].unwrap();
    let shape = royalesim::state::footprint_of(&s, tower).expect("tower footprint");
    let centre = s.entity(tower).unwrap().pos;
    assert!(s.debug_set_pos(k, centre), "teleport failed");
    assert!(shape.penetrates(s.entity(k).unwrap().pos, 0), "plant check: the Knight must start inside");
    s.tick();
    let after = s.entity(k).unwrap().pos;
    assert!(!shape.penetrates(after, 0), "the Knight is still inside the tower footprint at {after:?}");
    // And it stays out while it walks away and back into contact.
    let mut inv = Invariants::new(DEFAULT_TOLERANCE);
    for _ in 0..400 {
        s.tick();
        inv.check(&s).unwrap();
    }
}

// ===========================================================================
// path.rs

fn giant_track(model: PathModel, ticks: u32) -> Vec<Vec2> {
    let mut cfg = config();
    cfg.path_model = model;
    let mut s = BattleState::new(1, cfg);
    s.spawn_unit(Team::Blue, "Giant", t(600, 900), None).unwrap();
    let mut track = Vec::new();
    for _ in 0..ticks {
        s.tick();
        match find_live(&s, Team::Blue, "Giant").first() {
            Some(g) => track.push(g.pos),
            None => break,
        }
    }
    track
}

#[test]
fn the_three_path_models_produce_distinguishable_tracks() {
    // What the oracle discriminates between: a Giant from (6, 9) to the enemy left
    // princess tower, across the left bridge. Two coinciding models could never be
    // told apart by a recording. Threshold: 0.5 tile max separation, the stated
    // track-extraction error budget (UNVERIFIED -- the oracle's extractor does not
    // exist yet). Plant: lanesnap_is_diagonal.
    let ticks = 500;
    let models = [PathModel::LaneSnap, PathModel::GridAStar, PathModel::DiagonalLookahead];
    let tracks: Vec<Vec<Vec2>> = models.iter().map(|m| giant_track(*m, ticks)).collect();
    for tr in &tracks {
        assert!(tr.len() as u32 == ticks, "Giant died or vanished");
        assert!(tr.last().unwrap().y > tiles(17), "Giant never crossed the river: {:?}", tr.last());
    }
    let mut report = Vec::new();
    for a in 0..3 {
        for b in (a + 1)..3 {
            let max = tracks[a].iter().zip(tracks[b].iter()).map(|(p, q)| p.dist(*q)).max().unwrap();
            report.push((models[a], models[b], max));
        }
    }
    for (a, b, max) in &report {
        println!("track separation {a:?} vs {b:?}: max {} subtiles = {}.{:02} tiles", max, max / SUBTILE, max % SUBTILE * 100 / SUBTILE);
    }
    for (a, b, max) in &report {
        assert!(*max > SUBTILE / 2, "{a:?} and {b:?} tracks never separate by more than 0.5 tile ({max}); the oracle cannot tell them apart");
    }
}

#[test]
fn no_path_model_ever_puts_a_ground_unit_on_water() {
    // 30 ground units dropped at random dry points on both sides, every model,
    // 600 ticks, water / footprint / overlap invariants on every tick.
    // Plant: no_water_block.
    for model in [PathModel::LaneSnap, PathModel::GridAStar, PathModel::DiagonalLookahead] {
        let mut cfg = config();
        cfg.path_model = model;
        let mut s = BattleState::new(11, cfg);
        let mut rng = Rng::new(99);
        let cards = ["Knight", "Giant", "Valkyrie", "HogRider", "Musketeer", "Prince"];
        let mut placed = 0;
        while placed < 30 {
            let p = Vec2::new(rng.range(tiles(1), tiles(17)), rng.range(tiles(12), tiles(20)));
            let team = if rng.below(2) == 0 { Team::Blue } else { Team::Red };
            let card = cards[rng.below(cards.len() as u32) as usize];
            if s.spawn_unit(team, card, p, None).is_ok() {
                placed += 1;
            }
        }
        let mut inv = Invariants::new(DEFAULT_TOLERANCE);
        for _ in 0..600 {
            s.tick();
            if let Err(e) = inv.check(&s) {
                panic!("{model:?}: {e}");
            }
        }
        assert!(inv.ground_troop_ticks > 3000, "{model:?}: too few ground-troop ticks checked");
    }
}

#[test]
fn air_units_ignore_the_grid() {
    // Minions from (9, 10) fly straight over the river; every model plans a single
    // waypoint for them and the track touches water cells. Plant: air_uses_grid.
    for model in [PathModel::LaneSnap, PathModel::GridAStar, PathModel::DiagonalLookahead] {
        let mut cfg = config();
        cfg.path_model = model;
        let mut s = BattleState::new(1, cfg);
        s.spawn_unit(Team::Blue, "Minions", t(900, 1000), None).unwrap();
        let mut over_water = false;
        let mut max_route = 0;
        for _ in 0..300 {
            s.tick();
            for m in find_live(&s, Team::Blue, "Minions") {
                assert!(m.flying);
                over_water |= s.arena().is_water(m.pos);
                max_route = max_route.max(m.route.len());
            }
        }
        assert!(over_water, "{model:?}: Minions never flew over water");
        assert!(max_route <= 1, "{model:?}: an air unit got a multi-waypoint route ({max_route})");
    }
}

#[test]
fn planners_agree_air_is_one_straight_leg_and_ground_is_not() {
    // The plan() level of the same property, straight through the trait.
    let arena = Arena::shipped();
    let world = FrameWorld { arena: &arena, obstacles: &[] };
    for model in [PathModel::LaneSnap, PathModel::GridAStar, PathModel::DiagonalLookahead] {
        let pf = path::pathfinder_for(model);
        let mut req = NavRequest {
            #[cfg(clash_plant = "reflection_bridge_tie")]
            red: false,
            team: royalesim::Team::Blue,
            pos: t(900, 1000),
            goal: t(350, 2550),
            radius: milli(500),
            sight: milli(5500),
            step: 900,
            // Only PathModel::Oracle2026 reads `reach`, and it is not in this
            // list: its route is goal-first and its air case is a single leg by
            // construction, so "air is one straight leg" says nothing about it.
            reach: 0,
            flying: true,
            target_flying: false,
            jumper: false,
            ignore: None,
        };
        assert_eq!(pf.plan(&world, &req), vec![req.goal], "{model:?} air");
        req.flying = false;
        let ground = pf.plan(&world, &req);
        assert!(ground.len() > 1, "{model:?}: ground route across the river is a single leg");
        assert_eq!(*ground.last().unwrap(), req.goal);
    }
}

// ===========================================================================
// combat.rs

/// A card set with an archer whose CHARACTER damage (999) differs from its
/// PROJECTILE damage (41), and an inert dummy target. Built as cards.json text so
/// it goes through the one card loader.
fn projectile_db() -> CardDb {
    CardDb::from_json_str(
        r#"{"cards":[
        {"name":"TestArcher","kind":"troop","rarity":"Common","hitpoints":500,"damage":999,
         "hit_speed_ms":1200,"load_time_ms":500,"speed":0,"range_milli":5000,"sight_range_milli":5500,
         "collision_radius_milli":500,"mass":3,"deploy_time_ms":1000,"attacks_air":true,"attacks_ground":true,
         "projectile":{"speed":600,"damage":41}},
        {"name":"Dummy","kind":"troop","rarity":"Common","hitpoints":100000,"damage":0,
         "hit_speed_ms":1000,"speed":0,"range_milli":100,"sight_range_milli":100,
         "collision_radius_milli":500,"mass":3,"deploy_time_ms":1000,"attacks_air":false,"attacks_ground":false}
        ]}"#,
        CardSource::DerivedJson,
    )
    .unwrap()
}

#[test]
fn ranged_damage_comes_from_the_projectile_and_lands_on_arrival() {
    // Plant: character_damage_wins (card damage over projectile damage).
    let db = projectile_db();
    let mut cfg = BattleConfig::with_cards(db);
    cfg.tower_level = [cfg.card_level[0], cfg.card_level[1]];
    let mut s = BattleState::new(1, cfg);
    let a_idx = s.cards().index("TestArcher").unwrap();
    let expected = s.cards().scaled(a_idx, s.config().card_level[0], 41).unwrap();
    // Out of every tower's range (a princess tower reaches Range 7500 + its own
    // 1000 + the target's radius: 9 tiles from its centre for a 500-radius unit,
    // targeting.ATTACK_RANGE_RULE): archer on Red's side facing a dummy on Blue's,
    // both 9.7 tiles from the nearest princess tower.
    s.spawn_unit(Team::Blue, "TestArcher", t(900, 1750), None).unwrap();
    s.spawn_unit(Team::Red, "Dummy", t(900, 1450), None).unwrap();
    let dummy = {
        s.tick();
        id_of(&s, Team::Red, "Dummy")
    };
    let max = s.entity(dummy).unwrap().max_hp;
    let mut fired_tick = None;
    let mut hit_tick = None;
    for _ in 0..200 {
        s.tick();
        if fired_tick.is_none() && !s.projectiles().is_empty() {
            fired_tick = Some(s.tick_count());
            assert_eq!(s.projectiles()[0].damage, expected, "projectile carries the wrong damage");
            assert_eq!(s.entity(dummy).unwrap().hp, max, "damage landed on the fire tick, not on arrival");
        }
        let hp = s.entity(dummy).unwrap().hp;
        if hp < max {
            hit_tick = Some(s.tick_count());
            assert_eq!(max - hp, expected, "first hit dealt {} (card damage scaled would be {})", max - hp, s.cards().scaled(a_idx, 9, 999).unwrap());
            break;
        }
    }
    let (f, h) = (fired_tick.expect("never fired"), hit_tick.expect("never hit"));
    assert!(h > f, "hit on tick {h} is not after fire on tick {f}");
}

#[test]
fn two_units_that_kill_each_other_on_the_same_tick_both_die() {
    // Damage is buffered and applied in Resolve. Two Knights spawned on the same
    // tick facing each other wind up together; at 1 hp each, both must die on
    // the same tick. Plant: inline_damage.
    let mut s = BattleState::new(1, config());
    s.spawn_unit(Team::Blue, "Knight", t(900, 1350), None).unwrap();
    s.spawn_unit(Team::Red, "Knight", t(900, 1450), None).unwrap();
    let n = deploy_ticks(&s, "Knight") + 1;
        tick_n(&mut s, n);
    let (b, r) = (id_of(&s, Team::Blue, "Knight"), id_of(&s, Team::Red, "Knight"));
    assert!(s.debug_set_hp(b, 1) && s.debug_set_hp(r, 1));
    let mut death_tick = [None, None];
    for _ in 0..100 {
        s.tick();
        if death_tick[0].is_none() && s.entity(b).is_none() {
            death_tick[0] = Some(s.tick_count());
        }
        if death_tick[1].is_none() && s.entity(r).is_none() {
            death_tick[1] = Some(s.tick_count());
        }
        if death_tick[0].is_some() && death_tick[1].is_some() {
            break;
        }
    }
    assert!(death_tick[0].is_some() && death_tick[1].is_some(), "not both died: {death_tick:?}");
    assert_eq!(death_tick[0], death_tick[1], "Knights died on different ticks: {death_tick:?}");
}

#[test]
fn valkyrie_splash_is_centred_on_the_valkyrie_not_her_target() {
    // cards.json: Valkyrie self_as_aoe_center = true. A ~~Tesla~~ Bomb Tower in front
    // is her target; a Cannon behind her is inside her splash radius from HER centre
    // but outside it from the Bomb Tower's. Only a self-centred splash can hurt the
    // Cannon. Not a Tesla: a Tesla hides (tests/hide.rs) and is not a
    // target while under; the Bomb Tower is a plain building. Plant: aoe_centre_on_target.
    let mut s = BattleState::new(1, config());
    let v = card_stat(&s, "Valkyrie");
    assert!(v.self_as_aoe_center, "data: Valkyrie self_as_aoe_center");
    let (range, splash) = (v.range, v.area_damage_radius);
    let building = plain_building(&s);
    let tr = card_stat(&s, building).collision_radius;
    let cr = card_stat(&s, "Cannon").collision_radius;
    let valk = t(900, 1900);
    let tesla = Vec2::new(valk.x, valk.y - range - tr); // exactly in edge range, toward the river
    let behind_edge = splash - SUBTILE / 2; // cannon edge 0.5 tile inside the splash from Valkyrie
    let cannon = Vec2::new(valk.x, valk.y + behind_edge + cr);
    // from the Bomb Tower the Cannon is out of splash reach:
    assert!(tesla.dist(cannon) > splash + cr, "geometry: Cannon must be out of a target-centred splash");
    assert!(s.arena().is_passable_ground(tesla));
    s.spawn_unit(Team::Red, building, tesla, None).unwrap();
    s.spawn_unit(Team::Red, "Cannon", cannon, None).unwrap();
    s.spawn_unit(Team::Blue, "Valkyrie", valk, None).unwrap();
    s.tick();
    let (tid, cid) = (id_of(&s, Team::Red, building), id_of(&s, Team::Red, "Cannon"));
    let cmax = s.entity(cid).unwrap().max_hp;
    let tmax = s.entity(tid).unwrap().max_hp;
    let hit = run_until(&mut s, 200, |s| s.entity(tid).map_or(true, |t| t.hp < tmax));
    assert!(hit < 200, "the Valkyrie never hit the Bomb Tower");
    let vv = find_live(&s, Team::Blue, "Valkyrie")[0];
    assert_eq!(vv.target, Some(tid), "precondition: the Bomb Tower is the target");
    assert!(s.entity(cid).unwrap().hp < cmax, "the Cannon behind the Valkyrie took no splash");
    let _ = isqrt(0);
}
