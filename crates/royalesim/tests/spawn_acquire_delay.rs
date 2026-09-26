//! THE ACQUIRE DELAY -- calibration targeting.SPAWNED_UNIT_ACQUIRE_DELAY; state.rs
//! `delay_acquisition` (the one setter), target.rs `can_target` (the one reader), entity.rs
//! `acquirable_from`.
//!
//! THE LAW (arm client_8th_frame), measured on client 15.535.29: a troop created by a death
//! spawn is first targeted by an enemy on its 8th frame, F + 7, F being its first frame. Over
//! the 15.535.29 scenario runs, 35 death spawns were first targeted on exactly F + 7 and none
//! on F + 1 to F + 6; the witnesses include an idle Cannon beside a Goblin Cage's Brawler and
//! the Golemites of both seats. The Golemite row has no DeployDelay, so the delay is not that
//! column. Targeted at once, as measured: a hand-played troop (on its first frame), a
//! Tombstone's periodic Skeleton (a Knight switched to its first Skeleton on its 2nd frame) and
//! the Goblin Drill's building, which is not a death spawn (on F + 1). The engine exempts more
//! than that, unmeasured: every other periodic spawner's troops and any building a death spawn
//! creates. OPEN: the Barbarian Hut loads and is exempt with the Tombstone, yet its row carries
//! the same SpawnCharacter and SpawnInterval columns as the Goblin Hut's, whose waves wait for
//! F + 7. The key SHIPS at client_8th_frame. Under none, the engine before its flip, the same
//! idle Cannon targets the Brawler and a Golemite on F + 1; every scene here names its arm.
//!
//! FRAMES ARE COUNTED AS THE MEASUREMENT COUNTS THEM: a unit's first frame F is the state after
//! the tick that created it, and a looker "targets it on F + k" when its target is that unit in
//! the state k ticks later (`Watch`).
//!
//! WHAT IS PINNED, and the plant that turns each gate red (each one compiled in with
//! `--cfg clash_plant="..."`, docs/contributing.md):
//!   1. a Goblin Cage's Brawler and a Golem's Golemites, on either seat: no enemy targets one
//!      before F + 7, and the idle Cannon across the river first targets one on exactly F + 7
//!      -- acquire_delay_unread (F + 1), acquire_delay_one_short (F + 6);
//!   2. under spawner.RELEASE_TIMING = next_spawn_phase the Golemites are born a tick after
//!      the Golem's death, and the 7 ticks count from their own first frame, not the death
//!      -- acquire_delay_dropped_in_queue;
//!   3. with the death-spawn slide (spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide): the
//!      slide keeps a Golemite from TAKING a target until it reaches its radius, the delay
//!      keeps the Cannon off IT until F + 7, and a Golemite takes the Cannon inside its own
//!      delay -- acquire_delay_unread;
//!   4. the control: a hand-played Knight is targeted on its first frame under both arms
//!      -- acquire_delay_every_unit;
//!   5. one card, one parent: a Tombstone's periodic Skeleton is targeted on its 2nd frame
//!      under both arms, while the same Tombstone's death Skeletons carry the delay and no
//!      enemy targets one before its F + 7 -- acquire_delay_every_unit;
//!   6. OPEN, the engine's reading carried over from the Goblin Drill's building, which is not a
//!      death spawn: a death-spawned BUILDING is exempt. A doctored Golem that leaves two
//!      Cannons (no loaded row pairs a death spawn with a building) has them targeted on F + 1
//!      -- acquire_delay_on_buildings;
//!   7. the old arm, none: the Cannon on the Brawler and a Golemite on F + 1, and no unit
//!      carries a delay -- acquire_delay_ignores_arm; the shipped arm is client_8th_frame;
//!   8. OPEN, the engine's reading (the Phoenix egg is the only evidence): a Zap lands on a unit
//!      inside its delay -- acquire_delay_blocks_area;
//!   9. a snapshot taken mid-delay resumes hash for hash, and the Cannon still waits for F + 7
//!      -- save_drops_acquire_delay (the load's own hash self-check refuses the blob);
//!  10. OPEN, the same reading for a splash, which does not go through the spell path: an Ice
//!      Golem's death damage disc lands on a unit inside its delay -- acquire_delay_blocks_splash;
//!  11. OPEN, the engine's reading, unmeasured either way: the wake of a hidden building asks
//!      the same `can_target` as the scan, so a hidden Tesla rises for a death spawn on F + 7
//!      under the new arm and on F + 1 under the old -- acquire_delay_wakes_hidden;
//!  12. a snapshot taken while the death spawn still waits in the queue (spawner.RELEASE_TIMING
//!      = next_spawn_phase) resumes hash for hash, and the Cannon still waits for F + 7
//!      -- save_drops_queued_acquire_delay (the load's own hash self-check refuses the blob).
//!
//! THE SCENE. A Blue parent at 0 hp on its own half, two tiles short of the river, dies on the
//! first tick with no killer, so no attacker is in a post-kill wait. The idle Red Cannon stands
//! across the river 5.5 tiles from the death point and reaches 6.6 tiles to a 0.5-tile unit.
//! The Brawler, laid on the death point, and the front Golemite, 4 tiles from the Cannon, are
//! inside that reach; the rear Golemite, 7 tiles away, is not, and the gates read the nearest
//! member. Every Blue crown tower is 13 tiles or more away, outside it. The Red side is the
//! rotation of that.
mod common;

use common::*;
use royalesim::card::{CardDb, CardKind, CardSource};
use royalesim::entity::HideState;
use royalesim::fixed::Vec2;
use royalesim::state::{BattleConfig, BattleState, Calib, DeathSpawnPushback, ReleaseTiming, SpawnedUnitAcquireDelay};
use royalesim::{EntityId, Team};
use std::collections::{BTreeMap, BTreeSet};

/// The measured first-target frame of a death spawn, F + 7. Written here rather than read off
/// target.rs `ACQUIRE_DELAY_TICKS`, so that the gates hold the engine to the measurement and not
/// to itself.
const EIGHTH: u32 = 7;

const NEW: SpawnedUnitAcquireDelay = SpawnedUnitAcquireDelay::Client8thFrame;
const OLD: SpawnedUnitAcquireDelay = SpawnedUnitAcquireDelay::None;

fn with_arm(mut cfg: BattleConfig, arm: SpawnedUnitAcquireDelay) -> BattleConfig {
    cfg.calib.spawned_unit_acquire_delay = arm;
    cfg
}

/// Where a Blue parent dies (module doc).
fn death_point() -> Vec2 {
    t(900, 1300)
}

/// Where the idle Red Cannon stands (module doc).
fn cannon_point() -> Vec2 {
    t(900, 1850)
}

/// `parent`'s death spawn: the unit's card name and the count.
fn death_spawn_of(s: &BattleState, parent: &str) -> (String, i32) {
    let ds = card_stat(s, parent).death_spawn.unwrap_or_else(|| panic!("data: {parent} has no death spawn"));
    (s.cards().get(ds.unit).name.clone(), ds.count)
}

/// One unit that was not on the board when the watch began, as it was on its first frame.
#[derive(Clone, Debug)]
struct Born {
    first: u32,
    team: Team,
    card: String,
    spawned_by: Option<EntityId>,
    acquirable_from: u32,
}

/// Who targets whom, frame by frame, from the state the watch began in.
#[derive(Clone, Debug, Default)]
struct Watch {
    /// every entity of the starting state: none of them is a newborn
    known: BTreeSet<EntityId>,
    born: BTreeMap<EntityId, Born>,
    /// (newborn, enemy looker) -> the first frame the looker's target is the newborn
    looks: BTreeMap<(EntityId, EntityId), u32>,
    /// newborn -> the first frame it has a target of its own
    takes: BTreeMap<EntityId, u32>,
    names: BTreeMap<EntityId, String>,
}

impl Watch {
    fn new(s: &BattleState) -> Self {
        Watch { known: s.entities().map(|e| e.id).collect(), ..Watch::default() }
    }

    /// Record the state `s` is in now: call it after every tick.
    fn see(&mut self, s: &BattleState) {
        let now = s.tick_count();
        for e in s.entities() {
            self.names.entry(e.id).or_insert_with(|| e.card.to_string());
            if !self.known.contains(&e.id) && !self.born.contains_key(&e.id) {
                let b = Born { first: now, team: e.team, card: e.card.to_string(), spawned_by: e.spawned_by, acquirable_from: e.acquirable_from };
                self.born.insert(e.id, b);
            }
        }
        for e in s.entities() {
            let Some(target) = e.target else { continue };
            if self.born.contains_key(&e.id) {
                self.takes.entry(e.id).or_insert(now);
            }
            if self.born.get(&target).is_some_and(|b| b.team != e.team) {
                self.looks.entry((target, e.id)).or_insert(now);
            }
        }
    }

    fn tick(&mut self, s: &mut BattleState, n: u32) {
        for _ in 0..n {
            s.tick();
            self.see(s);
        }
    }

    /// The newborns of `team` whose card is `card`, in id order.
    fn newborns(&self, card: &str, team: Team) -> Vec<EntityId> {
        self.born.iter().filter(|(_, b)| b.card == card && b.team == team).map(|(id, _)| *id).collect()
    }

    /// The earliest k such that `looker` (any enemy when None) targets one of `units` on that
    /// unit's own F + k.
    fn first_look(&self, units: &[EntityId], looker: Option<EntityId>) -> Option<u32> {
        self.looks
            .iter()
            .filter(|((u, w), _)| units.contains(u) && (looker.is_none() || looker == Some(*w)))
            .map(|((u, _), at)| at - self.born[u].first)
            .min()
    }

    /// Every look at one of `units` before its 8th frame, as "looker on unit at F + k".
    fn early(&self, units: &[EntityId]) -> Vec<String> {
        self.looks
            .iter()
            .filter(|((u, _), at)| units.contains(u) && **at - self.born[u].first < EIGHTH)
            .map(|((u, w), at)| format!("{} on {u:?} at F + {}", self.names[w], at - self.born[u].first))
            .collect()
    }
}

/// A death scene: the battle, its watch, the idle Cannon and the parent's death spawn.
struct Scene {
    s: BattleState,
    w: Watch,
    cannon: EntityId,
    kids: Vec<EntityId>,
}

impl Scene {
    fn run(&mut self, n: u32) {
        self.w.tick(&mut self.s, n);
    }

    /// The death spawn's first frame (its members share it).
    fn first(&self) -> u32 {
        let firsts: BTreeSet<u32> = self.kids.iter().map(|k| self.w.born[k].first).collect();
        assert_eq!(firsts.len(), 1, "scene: the members were born on different frames: {firsts:?}");
        *firsts.first().unwrap()
    }

    /// The k of the Cannon's first look at any member, on that member's F + k.
    fn cannon_first(&self) -> Option<u32> {
        self.w.first_look(&self.kids, Some(self.cannon))
    }
}

/// `side`'s `parent` at the death point (Red's at its rotation) at 0 hp, and the idle Cannon of
/// the other seat. Ticks until the death spawn exists; the watch has seen every state since the
/// parent was placed.
fn death_scene(cfg: BattleConfig, side: Team, parent: &str) -> Scene {
    let mut s = BattleState::new(7, cfg);
    let (at, cannon_at) = match side {
        Team::Blue => (death_point(), cannon_point()),
        Team::Red => (mirror(&s, death_point()), mirror(&s, cannon_point())),
    };
    let cannon = s.scenario_spawn_now(side.other(), "Cannon", cannon_at, None).unwrap();
    let p = s.scenario_spawn_now(side, parent, at, None).unwrap();
    assert!(s.debug_set_hp(p, 0));
    let (unit, n) = death_spawn_of(&s, parent);
    let mut w = Watch::new(&s);
    for _ in 0..3 {
        w.tick(&mut s, 1);
        if !w.newborns(&unit, side).is_empty() {
            break;
        }
    }
    assert!(s.entity(p).is_none(), "scene: the {side:?} {parent} did not die on the first tick");
    let kids = w.newborns(&unit, side);
    assert_eq!(kids.len(), n as usize, "scene: the {side:?} {parent}'s {n} {unit}");
    Scene { s, w, cannon, kids }
}

// ---------------------------------------------------------------------------
// (1) the law

#[test]
fn a_death_spawn_is_first_targeted_on_its_8th_frame() {
    // Plant acquire_delay_unread: the Cannon takes the Brawler and a Golemite on F + 1. Plant
    // acquire_delay_one_short: on F + 6.
    for parent in ["GoblinCage", "Golem"] {
        for side in [Team::Blue, Team::Red] {
            let mut sc = death_scene(with_arm(config(), NEW), side, parent);
            sc.run(3 * EIGHTH);
            let early = sc.w.early(&sc.kids);
            assert!(early.is_empty(), "{side:?} {parent}: targeted before its 8th frame: {early:?}");
            assert_eq!(sc.cannon_first(), Some(EIGHTH), "{side:?} {parent}: the idle Cannon's first target on the death spawn, as F + k");
        }
    }
}

// ---------------------------------------------------------------------------
// (2) the delay counts from the unit's own first frame

#[test]
fn under_next_spawn_phase_the_delay_counts_from_the_members_own_first_frame() {
    // Plant acquire_delay_dropped_in_queue: the queued Golemites are targeted on their first
    // frame. A delay counted from the parent's death would land a frame early, on F + 6.
    let mut cfg = with_arm(config(), NEW);
    cfg.calib.release_timing = ReleaseTiming::NextSpawnPhase;
    let mut sc = death_scene(cfg, Team::Blue, "Golem");
    // the Golem is gone from frame 1, the state after the first tick; the queue holds its
    // Golemites until the next Spawn phase
    assert!(sc.first() > 1, "scene: under next_spawn_phase the Golemites are born a tick after the death, not on frame {}", sc.first());
    sc.run(3 * EIGHTH);
    let early = sc.w.early(&sc.kids);
    assert!(early.is_empty(), "targeted before the 8th frame: {early:?}");
    assert_eq!(sc.cannon_first(), Some(EIGHTH), "the Cannon's first target on a queued Golemite, as F + k");
}

// ---------------------------------------------------------------------------
// (3) with the death-spawn slide

#[test]
fn with_the_death_spawn_slide_the_two_rules_run_side_by_side() {
    // Plant acquire_delay_unread: the Cannon takes a sliding Golemite on F + 1.
    let mut cfg = with_arm(config(), NEW);
    cfg.calib.death_spawn_pushback = DeathSpawnPushback::ClientRingSlide;
    let mut sc = death_scene(cfg, Team::Blue, "Golem");
    for k in &sc.kids {
        assert!(sc.s.entity(*k).unwrap().death_slide_radius > 0, "scene: a Golemite is not born sliding");
    }
    let first = sc.first();
    sc.run(3 * EIGHTH);
    let early = sc.w.early(&sc.kids);
    assert!(early.is_empty(), "targeted before the 8th frame: {early:?}");
    assert_eq!(sc.cannon_first(), Some(EIGHTH), "the Cannon's first target on a Golemite, as F + k");
    // The delay holds the ENEMIES' scan and nothing of the unit's own: once its slide is over,
    // a Golemite takes the Cannon while the Cannon still cannot take it.
    let took = sc.kids.iter().filter_map(|k| sc.w.takes.get(k)).min().map(|at| at - first);
    assert!(took.is_some_and(|k| k < EIGHTH), "no Golemite took a target inside its own delay (first on F + {took:?})");
}

// ---------------------------------------------------------------------------
// (4), (5) the exempt troops

#[test]
fn a_hand_played_troop_is_targeted_on_its_first_frame_under_both_arms() {
    // Plant acquire_delay_every_unit: under client_8th_frame the Knight waits for F + 7.
    for arm in [NEW, OLD] {
        let mut s = BattleState::new(7, with_arm(scripted_config(), arm));
        let cannon = s.scenario_spawn_now(Team::Red, "Cannon", cannon_point(), None).unwrap();
        past_deploy_lockout(&mut s);
        let mut w = Watch::new(&s);
        if let Err(e) = s.deploy(Team::Blue, "Knight", death_point()) {
            panic!("scene: the Knight was refused: {e:?}");
        }
        w.tick(&mut s, 3);
        let knight = w.newborns("Knight", Team::Blue);
        assert_eq!(knight.len(), 1, "scene: one hand-played Knight");
        assert_eq!(w.first_look(&knight, Some(cannon)), Some(0), "{arm:?}: the Cannon's first target on the hand-played Knight, as F + k");
        assert_eq!(w.born[&knight[0]].acquirable_from, 0, "{arm:?}: the hand-played Knight carries a delay");
    }
}

#[test]
fn a_tombstones_periodic_skeleton_is_targeted_at_once_and_its_death_skeletons_wait() {
    // Plant acquire_delay_every_unit: the periodic Skeleton waits for F + 7 as well.
    //
    // The Cannon stands 7.6 tiles in front of the Tombstone: the Tombstone is outside its sight
    // (5.5 tiles + both radii, 7.1), the Skeletons it emits and leaves 1.5 tiles in front of it
    // are inside (6.6 for a Skeleton), so the Cannon is idle until the first one appears.
    for arm in [NEW, OLD] {
        let mut s = BattleState::new(7, with_arm(config(), arm));
        let cannon = s.scenario_spawn_now(Team::Red, "Cannon", t(900, 1860), None).unwrap();
        let tomb = s.scenario_spawn_now(Team::Blue, "Tombstone", t(900, 1100), None).unwrap();
        let (unit, n) = death_spawn_of(&s, "Tombstone");
        let periodic_unit = card_stat(&s, "Tombstone").spawner.expect("data: the Tombstone is a periodic spawner").unit;
        assert_eq!(s.cards().get(periodic_unit).name, unit, "data: the Tombstone's periodic and death units are one card");
        let mut w = Watch::new(&s);
        let periodic = |w: &Watch| -> Vec<EntityId> { w.newborns(&unit, Team::Blue).into_iter().filter(|k| w.born[k].spawned_by.is_some()).collect() };
        for _ in 0..4 {
            w.tick(&mut s, 1);
            if !periodic(&w).is_empty() {
                break;
            }
        }
        let first_periodic = periodic(&w);
        assert_eq!(first_periodic.len(), 1, "scene: the Tombstone's first periodic {unit}");
        // Killed at once: its death Skeletons are born on the next frame, after the Cannon's
        // Target phase has had the periodic one alone to look at.
        assert!(s.debug_set_hp(tomb, 0));
        w.tick(&mut s, 3 * EIGHTH);
        assert!(s.entity(tomb).is_none(), "scene: the Tombstone did not die");
        let dead: Vec<EntityId> = w.newborns(&unit, Team::Blue).into_iter().filter(|k| w.born[k].spawned_by.is_none()).collect();
        assert_eq!(dead.len(), n as usize, "scene: the Tombstone's {n} death {unit}s");
        // Measured: a Knight switched to a Tombstone's first Skeleton on its 2nd frame.
        assert_eq!(w.first_look(&first_periodic, Some(cannon)), Some(1), "{arm:?}: the Cannon's first target on the periodic {unit}, as F + k");
        for k in w.newborns(&unit, Team::Blue) {
            let b = &w.born[&k];
            // F is the state after the creating tick, so the unit's first tick is F - 1
            let want = if arm == NEW && b.spawned_by.is_none() { b.first - 1 + EIGHTH } else { 0 };
            assert_eq!(b.acquirable_from, want, "{arm:?}: {unit} {k:?} (periodic: {})", b.spawned_by.is_some());
        }
        if arm == NEW {
            let early = w.early(&dead);
            assert!(early.is_empty(), "a death {unit} targeted before its 8th frame: {early:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// (6) OPEN: a death-spawned building

/// The shipped cards with the Golem's death spawn swapped for two Cannons. No loaded row pairs a
/// death spawn with a building; this one is doctored to have one.
fn golem_leaving_cannons() -> CardDb {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let golem = doc["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "Golem").expect("data: a Golem row");
    golem["death_spawn"]["character"] = serde_json::Value::from("Cannon");
    let db = CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).expect("the doctored file still parses");
    let g = db.index("Golem").unwrap_or_else(|| panic!("the doctored Golem does not load: {:?}", db.rejected.iter().find(|(n, _)| n == "Golem")));
    let ds = db.get(g).death_spawn.expect("scene: the doctored Golem has a death spawn");
    assert_eq!((db.get(ds.unit).name.as_str(), db.get(ds.unit).kind), ("Cannon", CardKind::Building), "scene: the doctored death spawn");
    db
}

#[test]
fn open_a_death_spawned_building_is_targeted_at_once() {
    // OPEN: the engine's reading. The one spawned building measured, the Goblin Drill's, is not
    // a death spawn; this carries its F + 1 over. Plant acquire_delay_on_buildings: the Cannons
    // wait for F + 7.
    let mut sc = death_scene(with_arm(BattleConfig::with_cards(golem_leaving_cannons()), NEW), Team::Blue, "Golem");
    sc.run(EIGHTH + 2);
    assert_eq!(sc.cannon_first(), Some(1), "the Red Cannon's first target on a death-spawned Cannon, as F + k");
    for k in &sc.kids {
        assert_eq!(sc.w.born[k].acquirable_from, 0, "a death-spawned building carries a delay");
    }
}

// ---------------------------------------------------------------------------
// (7) the old arm

#[test]
fn the_old_arm_targets_on_f1_and_the_new_arm_ships() {
    // Plant acquire_delay_ignores_arm: the Cannon waits for F + 7 under none as well.
    assert_eq!(Calib::shipped().spawned_unit_acquire_delay, NEW, "the ledger ships client_8th_frame");
    assert_eq!(config().calib.spawned_unit_acquire_delay, NEW);
    for parent in ["GoblinCage", "Golem"] {
        let mut sc = death_scene(with_arm(config(), OLD), Team::Blue, parent);
        sc.run(3 * EIGHTH);
        assert_eq!(sc.cannon_first(), Some(1), "{parent}: the idle Cannon's first target on the death spawn under none, as F + k");
        for (k, b) in &sc.w.born {
            assert_eq!(b.acquirable_from, 0, "{parent}: {} {k:?} carries a delay under none", b.card);
        }
        assert!(sc.s.entities().all(|e| e.acquirable_from == 0), "{parent}: a unit carries a delay under none");
    }
}

// ---------------------------------------------------------------------------
// (8) OPEN: area damage

#[test]
fn open_area_damage_lands_on_a_unit_inside_its_delay() {
    // OPEN: the engine's reading. Area damage is not a target scan, and the Phoenix egg, the one
    // witness, suggests it lands; nothing cleaner is measured. Plant acquire_delay_blocks_area:
    // the Zap spares the Golemites.
    let mut sc = death_scene(with_arm(config(), NEW), Team::Blue, "Golem");
    let first = sc.first();
    let hp0: BTreeMap<EntityId, i32> = sc.kids.iter().map(|k| (*k, sc.s.entity(*k).unwrap().hp)).collect();
    sc.s.spawn_unit(Team::Red, "Zap", death_point(), None).unwrap();
    let mut hit = None;
    for _ in 1..EIGHTH {
        sc.run(1);
        let lost = sc.kids.iter().any(|k| sc.s.entity(*k).is_some_and(|e| e.hp < hp0[k]));
        if lost && hit.is_none() {
            hit = Some(sc.s.tick_count() - first);
        }
    }
    assert!(hit.is_some(), "the Zap took nothing off a Golemite before F + {EIGHTH}");
    let early = sc.w.early(&sc.kids);
    assert!(early.is_empty(), "the damage came with a target (F + {hit:?}): {early:?}");
}

// ---------------------------------------------------------------------------
// (9) save / load

#[test]
fn a_snapshot_mid_delay_resumes_hash_for_hash() {
    // Plant save_drops_acquire_delay: the load's hash self-check refuses the blob.
    let mut sc = death_scene(with_arm(config(), NEW), Team::Blue, "Golem");
    sc.run(2);
    let now = sc.s.tick_count();
    let pending = sc.kids.iter().filter(|k| sc.s.entity(**k).is_some_and(|e| e.acquirable_from > now)).count();
    assert!(pending > 0, "vacuous: no Golemite is inside its delay at the save");
    let blob = sc.s.save();
    let mut l = BattleState::load(&blob).unwrap_or_else(|e| panic!("a mid-delay snapshot does not load: {e}"));
    assert_eq!(l.state_hash(), sc.s.state_hash());
    let mut lw = sc.w.clone();
    for k in 0..(2 * EIGHTH) {
        sc.run(1);
        lw.tick(&mut l, 1);
        assert_eq!(l.state_hash(), sc.s.state_hash(), "diverged {k} ticks after the load");
        for id in &sc.kids {
            let (a, b) = (sc.s.entity(*id), l.entity(*id));
            assert_eq!(a.map(|e| (e.acquirable_from, e.target)), b.map(|e| (e.acquirable_from, e.target)), "a Golemite, {k} ticks after the load");
        }
    }
    assert_eq!(lw.first_look(&sc.kids, Some(sc.cannon)), Some(EIGHTH), "the loaded battle's Cannon on a Golemite, as F + k");
}

// ---------------------------------------------------------------------------
// (10) OPEN: a death damage disc

#[test]
fn open_a_death_damage_disc_lands_on_a_unit_inside_its_delay() {
    // OPEN: the engine's reading, as (8). A splash (combat.rs `splash`) is not a target scan
    // either, and it does not go through the spell path (8) pins. Plant
    // acquire_delay_blocks_splash: the Ice Golem's disc spares the Golemites. The disc is the
    // one damage in the scene: the Ice Golem's death area is a slow with no Damage of its own
    // (tests/death_area_effect.rs), and the Cannon cannot target a Golemite inside its delay.
    let mut sc = death_scene(with_arm(config(), NEW), Team::Blue, "Golem");
    let first = sc.first();
    let ice_golem = card_stat(&sc.s, "IceGolemite").clone();
    assert!(ice_golem.death_damage > 0 && ice_golem.death_damage_radius > 0, "data: the Ice Golem carries a death damage disc");
    let hp0: BTreeMap<EntityId, i32> = sc.kids.iter().map(|k| (*k, sc.s.entity(*k).unwrap().hp)).collect();
    // A Red Ice Golem at 0 hp on the death point, 1.5 tiles from each Golemite and so inside
    // its disc: it dies on the next tick, whose Reap buffers the disc, and the tick after
    // resolves it.
    let ice = sc.s.scenario_spawn_now(Team::Red, "IceGolemite", death_point(), None).unwrap();
    assert!(sc.s.debug_set_hp(ice, 0));
    let mut hit = None;
    for _ in 1..EIGHTH {
        sc.run(1);
        let lost = sc.kids.iter().any(|k| sc.s.entity(*k).is_some_and(|e| e.hp < hp0[k]));
        if lost && hit.is_none() {
            hit = Some(sc.s.tick_count() - first);
        }
    }
    assert!(sc.s.entity(ice).is_none(), "scene: the Ice Golem did not die");
    assert!(hit.is_some(), "the Ice Golem's disc took nothing off a Golemite before F + {EIGHTH}");
    let early = sc.w.early(&sc.kids);
    assert!(early.is_empty(), "the damage came with a target (F + {hit:?}): {early:?}");
}

// ---------------------------------------------------------------------------
// (11) OPEN: a hidden building's wake

#[test]
fn open_a_hidden_tesla_rises_for_a_death_spawn_once_its_delay_is_over() {
    // OPEN: the engine's reading, unmeasured either way. The wake of a hidden building
    // (target.rs `enemy_in_wake_range`) asks the same `can_target` as the scan, so a death
    // spawn inside its delay does not raise it. Plant acquire_delay_wakes_hidden: under
    // client_8th_frame the Tesla rises on F + 1.
    //
    // The Red Tesla stands where the Cannon stands in the other scenes. Its wake reaches 6.5
    // tiles to a 0.5-tile unit (5.5 + both radii), so the front Golemite, 4 tiles away, wakes
    // it, and no other Blue unit comes near. The Golem at 0 hp is nobody's target. The old arm
    // is the control: there the same Golemites raise it on F + 1.
    for arm in [NEW, OLD] {
        let mut s = BattleState::new(7, with_arm(config(), arm));
        let tesla = s.scenario_spawn_now(Team::Red, "Tesla", cannon_point(), None).unwrap();
        let golem = s.scenario_spawn_now(Team::Blue, "Golem", death_point(), None).unwrap();
        assert!(s.debug_set_hp(golem, 0));
        let (unit, _) = death_spawn_of(&s, "Golem");
        assert_eq!(s.entity(tesla).unwrap().hide_state, HideState::Hidden, "scene: the Tesla does not start under");
        let mut w = Watch::new(&s);
        let mut rose = None;
        for _ in 0..(3 * EIGHTH) {
            w.tick(&mut s, 1);
            if rose.is_none() && s.entity(tesla).is_some_and(|e| e.hide_state != HideState::Hidden) {
                rose = Some(s.tick_count());
            }
        }
        assert!(s.entity(golem).is_none(), "scene: the Golem did not die");
        let kids = w.newborns(&unit, Team::Blue);
        assert!(!kids.is_empty(), "scene: the Golem left no {unit}");
        let first = w.born[&kids[0]].first;
        let want = if arm == NEW { EIGHTH } else { 1 };
        assert_eq!(rose.map(|r| r - first), Some(want), "{arm:?}: the hidden Tesla's rise, as F + k of the {unit}");
    }
}

// ---------------------------------------------------------------------------
// (12) save / load with the death spawn still queued

#[test]
fn a_snapshot_with_the_death_spawn_still_queued_resumes_hash_for_hash() {
    // Plant save_drops_queued_acquire_delay: the load's hash self-check refuses the blob. The
    // flag is all the queued members carry of the delay; the column is set only when they are
    // created, after the load.
    let mut cfg = with_arm(config(), NEW);
    cfg.calib.release_timing = ReleaseTiming::NextSpawnPhase;
    let mut s = BattleState::new(7, cfg);
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", cannon_point(), None).unwrap();
    let golem = s.scenario_spawn_now(Team::Blue, "Golem", death_point(), None).unwrap();
    assert!(s.debug_set_hp(golem, 0));
    let (unit, n) = death_spawn_of(&s, "Golem");
    let unit_idx = s.cards().index(&unit).unwrap_or_else(|| panic!("data: {unit} is not in the card data"));
    s.tick();
    assert!(s.entity(golem).is_none(), "scene: the Golem did not die on the first tick");
    let queued = s.pending_spawns().iter().filter(|(team, card, _)| *team == Team::Blue && *card == unit_idx).count();
    assert_eq!(queued, n as usize, "scene: the Golem's {n} {unit} are not waiting in the queue at the save");
    let blob = s.save();
    let mut l = BattleState::load(&blob).unwrap_or_else(|e| panic!("a snapshot with a queued death spawn does not load: {e}"));
    assert_eq!(l.state_hash(), s.state_hash());
    let (mut w, mut lw) = (Watch::new(&s), Watch::new(&l));
    for k in 0..(3 * EIGHTH) {
        w.tick(&mut s, 1);
        lw.tick(&mut l, 1);
        assert_eq!(l.state_hash(), s.state_hash(), "diverged {k} ticks after the load");
    }
    for (name, watch) in [("saved", &w), ("loaded", &lw)] {
        let kids = watch.newborns(&unit, Team::Blue);
        assert_eq!(kids.len(), n as usize, "scene: the {name} battle's {n} {unit}");
        assert_eq!(watch.first_look(&kids, Some(cannon)), Some(EIGHTH), "the {name} battle's Cannon on a Golemite, as F + k");
    }
}
