//! THE HEALTH-THRESHOLD TRANSFORMATION (card.rs `TransformDef`, `transform_of`; state.rs `health_triggers`,
//! `fire_scheduled`, `rebind_unit`, `lifetime_of`; entity.rs `Entities::rebind`).
//!
//! THE LAW, measured on client 15.535.29 in the Cannon Cart and Goblin Demolisher scenarios (six runs, level 11; the
//! 16.402 corpus holds neither card). With C the frame a hit takes the unit to half its hitpoints or below:
//!   - the threshold is read at the unit's own update, its turn in the Target phase (transform.HEALTH_TRIGGER_TIMING =
//!     own_update): C + 1 after a projectile's hit, C itself after a melee strike by a unit created before it under
//!     match.TICK_ORDER = client_sequential_strike;
//!   - the Cannon Cart becomes BrokenCannon (a building, LifeTime 30000) at once, in place: the same entity, hp and
//!     max hp kept (transform.ENTITY_CONTINUITY), its target and attack clock kept (transform.ATTACK_STATE), no second
//!     deploy (transform.REDEPLOY); it drains first on the next tick, 3 a tick, on its max hp 1809
//!     (lifetime.DRAIN_BASE_AFTER_TRANSFORM);
//!   - the Goblin Demolisher, 100 ms after its trigger (a `Scheduled` entry fired at the top of the Status phase),
//!     becomes its kamikaze form (a troop, LifeTime 20000, lifetime.TROOP_LIFETIME) with no target on its switch tick
//!     and a building the tick after; the form drains 3, 3, 3, 4 (325 hundredths on its max 1300);
//!   - both forms blast where they die, on the second frame after their last (spawner.DEATH_SPAWN_PROJECTILE =
//!     client_projectile, NOT the shipped arm): 404 at level 11 on a princess tower, in full, and on troops, which the
//!     blast pushes 250 on each of their first three ladder ticks.
//!
//! WHAT IS PINNED, each test naming the arm it tests through its config (every scene runs at level 11, the measured
//! level):
//!   1. `the_loader_reads_both_transformations` (the 15.535.29 tables);
//!   2. `a_cart_broken_by_a_tower_shot_becomes_its_cannon_in_place` (measured: the projectile crossing);
//!   3. `the_broken_cannon_drains_three_a_tick_on_its_max_hp` (measured);
//!   4. `a_melee_crossing_is_read_on_its_tick_under_the_sequential_strike_order` (measured under the sequential order;
//!      the shipped client16402's one tick later is the engine's reading, named as such);
//!   5. `the_threshold_compares_at_or_below` (a guess: no run landed on the line; both arms);
//!   6. `the_rebind_takes_every_row_column_from_the_new_row_and_keeps_the_entity` (engine contract, synthetic rows
//!      whose every column differs: the real rows share their hitpoints, which would hide a re-read);
//!   7. `a_demolisher_crossed_by_a_tower_shot_switches_two_ticks_after_its_trigger` (measured);
//!   8. `the_kamikaze_form_drains_3_25_a_tick` (measured);
//!   9. `the_kamikaze_blast_lands_two_ticks_after_its_last_frame` (measured; the new arm by override);
//!  10. `a_kamikaze_killed_mid_run_still_blasts` (measured; the new arm by override);
//!  11. `the_ranged_form_killed_from_above_half_in_one_tick_blasts_and_never_transforms` (measured; override);
//!  12. `the_shipped_arm_leaves_the_demolisher_without_its_blast` (the shipped `none`: the gap, pinned);
//!  13. `a_pending_transformation_survives_a_save` (engine invariant);
//!  14. `a_pending_transformation_is_hashed` (engine invariant);
//!  15. `both_seats_transform_on_the_same_tick` (the mirror gate);
//!  16. `a_unit_healed_back_above_the_line_schedules_nothing_twice` (engine invariant: a heal read while a change is
//!      pending cancels nothing, and a second crossing read while it is pending schedules nothing; on the Demolisher
//!      and on a synthetic row whose longer wait holds several updates);
//!  17. `a_transformation_graph_with_one_more_class_is_refused_with_todays_message` (synthetic);
//!  18. `only_the_demolishers_taunt_cancel_carries_an_action_taunt` (the no-op reading's precondition, over the data);
//!  19. `a_transformed_unit_reports_its_catalogue_card` (py.rs `ids_of_indices`);
//!  20. `the_crossing_tick_arm_changes_the_cart_on_its_crossing_tick` (the other timing arm);
//!  21. `the_reset_always_arm_drops_the_carts_target_on_the_switch` (the other attack arm);
//!  22. `the_new_row_deploy_time_arm_deploys_the_cannon` (the other redeploy arm).
//!
//! THE PROJECTILE CROSSINGS ARE REAL: a Blue princess tower's shot, which lands in Resolve, is what takes the unit to
//! half (the Cannon Cart placed at 905 hp, the Goblin Demolisher at 651, one above each line), because the two timing
//! arms part only on where damage lands. Where timing is not the subject (3, 8, 13, 14, 16) the hp is set between two
//! ticks, which reads as a hit landed at the end of the tick just run.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test transform`):
//!   * `transform_never_fires` -- no unit reads its threshold: 2, 4, 5, 6, 7, 8, 9, 13, 14, 15, 16, 20, 21, 22 red.
//!   * `transform_trigger_on_resolve` -- the threshold read in Resolve on the crossing tick: 2 (the Cart is a building
//!     on C), 4 (its client16402 half), 7 (the switch on C + 2), 21 (the target taken back on C + 1) red.
//!   * `transform_group_delay_ignored` -- the Demolisher changes on its trigger tick: 7, 8, 13, 14, 16 red.
//!   * `transform_hp_refilled` -- the new row's full hitpoints: 2, 3, 6, 7, 8 red.
//!   * `transform_attack_state_reset` -- every change drops the target and the attack: 2 red.
//!   * `transform_keeps_old_lifetime` -- the new row never drains: 2, 3, 4, 6, 7, 8, 20 red.
//!   * `troop_lifetime_ignored` -- a troop gets no LifeTime: 7, 8 red (and tests/lifetime.rs).
//!   * `scheduled_dropped_on_save` -- a pending change is lost across a save: 13, 14 red.
//!   * `hash_skips_scheduled` -- the scheduled list is not hashed: 14 red.
//!   * `transform_block_unchecked` -- any graph that carries a transformation block loads: 17 red.
//!   * `spawned_death_projectile_unread` -- the kamikaze form loads with no blast: 1, 9, 10 red.
//!   * `death_projectile_unread` (tests/test_death_spawn_projectile.py's) -- no death projectile fires under the new
//!     arm: 9, 10, 11 red.
//!   * `taunt_guard_blind` -- the data guard of 18 stops reading the area table: 18 red.
//!   * `unit_refs_skips_new_paths` (tests/unit_refs.rs's) -- `unit_refs` drops the transformation: 19 red.
//!   * `transform_heal_cancels` -- a read above the line drops the pending change: 16 red.
//!   * `transform_rescheduled_while_pending` -- a unit with a change pending schedules another: 16 red, and 7 and 8
//!     too (the second entry runs the change again on the kamikaze form a tick later).
//!
//! No existing gate should go red under the first ten or the last two: no card loaded before this file carries a
//! transformation.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::arena::Lane;
use royalesim::card::{CardDb, CardKind, CardSource, SpellDef, SpellShape};
use royalesim::entity::{AttackPhase, EntityKind};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{
    BattleConfig, BattleState, DeathSpawnProjectile, ScheduledAction, TickOrder, TransformAttackState, TransformCompare, TransformRedeploy,
    TransformTiming,
};
use royalesim::{EntityId, Team};

/// The level every client measurement here was taken at.
const LEVEL: i32 = 11;
/// A Cannon Cart placed one hitpoint above its line (1809 x 50 % = 904.5), so a tower's first shot crosses it.
const CART_HP: i32 = 905;
/// A Goblin Demolisher placed one hitpoint above its line (1300 x 50 % = 650).
const DEMOLISHER_HP: i32 = 651;
/// How far in front of the Blue princess tower's centre, native, the Cart and the Demolisher stand: inside their own
/// reach of it (Range + both radii: 7000 and 6600) and inside its reach of them, so they attack it from the first tick
/// and it is the only thing that hits them.
const CART_AHEAD: i32 = 6000;
const DEMOLISHER_AHEAD: i32 = 5500;
/// The Mortar scene: a Blue Mortar, and a Red unit 3000 in front of it. The unit attacks the Mortar, and the Mortar,
/// whose MinimumRange (3500, edge to edge) the unit is inside, cannot answer: nothing hits the unit.
const MORTAR_AT: (i32, i32) = (9500, 11500);
const MORTAR_UNIT_AT: (i32, i32) = (9500, 14500);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// `cfg` at the measured level, cards and towers both.
fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [LEVEL, LEVEL];
    cfg.tower_level = [LEVEL, LEVEL];
    cfg
}

/// The shipped transformation arms, asserted where a test says it pins them.
fn shipped_arms() -> BattleConfig {
    let cfg = level11(config());
    let c = &cfg.calib;
    assert_eq!(c.transform_compare, TransformCompare::AtOrBelow, "the shipped transform.HEALTH_TRIGGER_COMPARE");
    assert_eq!(c.transform_timing, TransformTiming::OwnUpdate, "the shipped transform.HEALTH_TRIGGER_TIMING");
    assert_eq!(c.transform_attack_state, TransformAttackState::KeepUnlessResetTarget, "the shipped transform.ATTACK_STATE");
    assert_eq!(c.transform_redeploy, TransformRedeploy::None, "the shipped transform.REDEPLOY");
    assert_eq!(c.troop_lifetime, royalesim::state::TroopLifetime::SameAsBuildings, "the shipped lifetime.TROOP_LIFETIME");
    cfg
}

/// The shipped arms with spawner.DEATH_SPAWN_PROJECTILE = client_projectile, the measured arm the ledger does not ship.
fn with_death_projectile() -> BattleConfig {
    let mut cfg = shipped_arms();
    cfg.calib.death_spawn_projectile = DeathSpawnProjectile::ClientProjectile;
    cfg
}

/// What one frame shows of one entity.
#[derive(Clone, Debug)]
struct Frame {
    tick: u32,
    card: String,
    kind: EntityKind,
    hp: i32,
    max_hp: i32,
    speed: i32,
    target: Option<EntityId>,
    phase: AttackPhase,
    attack_ms: i32,
    deploying: bool,
    pos: Vec2,
}

fn frame(s: &BattleState, id: EntityId) -> Option<Frame> {
    s.entity(id).map(|e| Frame {
        tick: s.tick_count(),
        card: e.card.to_string(),
        kind: e.kind,
        hp: e.hp,
        max_hp: e.max_hp,
        speed: e.speed,
        target: e.target,
        phase: e.attack_phase,
        attack_ms: e.attack_ms,
        deploying: e.deploying,
        pos: e.pos,
    })
}

/// Tick `n` times, recording `id`'s frame after each (None once it is gone).
fn run(s: &mut BattleState, id: EntityId, n: u32) -> Vec<Option<Frame>> {
    (0..n)
        .map(|_| {
            s.tick();
            frame(s, id)
        })
        .collect()
}

/// The index of the first recorded frame whose hp is below `from`: the crossing frame C.
fn crossing(frames: &[Option<Frame>], from: i32) -> usize {
    frames
        .iter()
        .position(|f| f.as_ref().is_some_and(|f| f.hp < from))
        .unwrap_or_else(|| panic!("the scene drifted: nothing hit the unit in {} ticks", frames.len()))
}

fn live(frames: &[Option<Frame>], k: usize) -> &Frame {
    frames.get(k).and_then(|f| f.as_ref()).unwrap_or_else(|| panic!("the unit is gone on recorded frame {k}"))
}

/// A Red `card` at `hp`, `ahead` native units in front of the Blue engine-Left princess tower; returns the battle,
/// the unit and the tower.
fn tower_scene(cfg: BattleConfig, card: &str, ahead: i32, hp: i32) -> (BattleState, EntityId, EntityId) {
    let mut s = BattleState::new(0, cfg);
    let t = s.arena().princess_tower_pos(Team::Blue, Lane::Left);
    let id = s.scenario_spawn_now(Team::Red, card, Vec2::new(t.x, t.y + ahead * K), Some(hp)).unwrap_or_else(|e| panic!("place the {card}: {e:?}"));
    let tower = s.tower_ids(Team::Blue)[1 + Lane::Left as usize].expect("the Blue engine-Left princess tower");
    (s, id, tower)
}

/// A Blue Mortar and a Red `card` in front of it, which nothing hits; returns the battle, the unit and the Mortar.
fn mortar_scene(cfg: BattleConfig, card: &str) -> (BattleState, EntityId, EntityId) {
    let mut s = BattleState::new(0, cfg);
    let mortar = s.scenario_spawn_now(Team::Blue, "Mortar", at(MORTAR_AT), None).expect("place the Mortar");
    let id = s.scenario_spawn_now(Team::Red, card, at(MORTAR_UNIT_AT), None).unwrap_or_else(|e| panic!("place the {card}: {e:?}"));
    (s, id, mortar)
}

/// Tick the Mortar scene until the unit has attacked for a while, then set its hp between two ticks.
fn cross_in_mortar_scene(s: &mut BattleState, id: EntityId, hp: i32) {
    for _ in 0..30 {
        s.tick();
    }
    let e = s.entity(id).expect("the unit stands");
    assert!(e.target.is_some() && e.attack_phase != AttackPhase::Idle, "the scene drifted: the {} is not attacking the Mortar", e.card);
    assert_eq!(e.hp, e.max_hp, "the scene drifted: something hit the {}", e.card);
    assert!(s.debug_set_hp(id, hp));
}

/// The native distance between two engine points.
fn dist(a: Vec2, b: Vec2) -> i64 {
    royalesim::fixed::isqrt(a.dist2(b)) / K as i64
}

fn scheduled_for(s: &BattleState, id: EntityId) -> Vec<i32> {
    s.scheduled().iter().filter(|e| matches!(e.action, ScheduledAction::Transform { entity, .. } if entity == id)).map(|e| e.ms).collect()
}

// ---------------------------------------------------------------------------
// (1) the loader

/// Plant: spawned_death_projectile_unread.
#[test]
fn the_loader_reads_both_transformations() {
    let s = BattleState::new(0, level11(config()));
    let db = s.cards();
    let cart = card_stat(&s, "MovingCannon").transform_at_hp.expect("the Cannon Cart carries its transformation");
    assert_eq!((cart.pct, cart.reset_target, cart.delay_ms), (50, false, 0));
    let cannon = db.get(cart.unit);
    assert_eq!(cannon.name, "BrokenCannon");
    assert!(cannon.summon_only && cannon.kind == CardKind::Building, "BrokenCannon is a building row, never played");
    assert_eq!(cannon.lifetime_ms, Some(30000));
    let demo = card_stat(&s, "GoblinDemolisher").transform_at_hp.expect("the Goblin Demolisher carries its transformation");
    assert_eq!((demo.pct, demo.reset_target, demo.delay_ms), (50, true, 100));
    let form = db.get(demo.unit);
    assert_eq!(form.name, "GoblinDemolisher_kamikaze_form");
    assert!(form.summon_only && form.kind == CardKind::Troop, "the kamikaze form is a troop row, never played");
    assert_eq!(form.lifetime_ms, Some(20000), "the one troop row with a LifeTime the loader takes");
    assert!(form.kamikaze && form.target_only_buildings);
    assert!(form.transform_at_hp.is_none(), "a transformation target carries no transformation of its own");
    // Both forms blast on death: the card row's projectile and the kamikaze form's own.
    for (who, c) in [("the Goblin Demolisher", card_stat(&s, "GoblinDemolisher")), ("its kamikaze form", form)] {
        match &c.death_projectile {
            Some(SpellDef { shape: SpellShape::Projectile { hit: Some(h), spawn: None, .. }, .. }) => {
                assert_eq!((h.damage, h.radius), (158, royalesim::fixed::milli(2500)), "{who}'s death projectile");
                assert!(h.knockback.is_some_and(|k| k.distance == royalesim::fixed::milli(2000)), "{who}'s blast pushes 2000");
            }
            other => panic!("{who} carries no damage-only death projectile: {other:?}"),
        }
    }
    // Every other card carries none.
    let carriers: Vec<&str> = db.cards.iter().filter(|c| c.transform_at_hp.is_some()).map(|c| c.name.as_str()).collect();
    assert_eq!(carriers, ["MovingCannon", "GoblinDemolisher"], "the cards that carry a transformation");
}

// ---------------------------------------------------------------------------
// (2), (3) the Cannon Cart

/// Plants: transform_never_fires, transform_trigger_on_resolve, transform_hp_refilled, transform_attack_state_reset,
/// transform_keeps_old_lifetime.
#[test]
fn a_cart_broken_by_a_tower_shot_becomes_its_cannon_in_place() {
    let (mut s, cart, tower) = tower_scene(shipped_arms(), "MovingCannon", CART_AHEAD, CART_HP);
    let frames = run(&mut s, cart, 120);
    let c = crossing(&frames, CART_HP);
    assert!(c + 46 < frames.len(), "the scene drifted: the crossing came on recorded frame {c}");
    let fc = live(&frames, c);
    assert_eq!(fc.max_hp, 1809, "precondition: the measured max hp, 707 at level 11");
    assert!(fc.hp <= 904, "precondition: one shot takes it to the line or below ({})", fc.hp);
    // C: still the Cart. A shot's damage lands at the end of the tick; the Cart reads it at its next update.
    assert_eq!((fc.card.as_str(), fc.kind), ("MovingCannon", EntityKind::Troop), "on the crossing frame");
    assert_eq!(fc.target, Some(tower), "precondition: the Cart attacks the tower");
    // C + 1: the same entity is its cannon, a building, at the same hp and max hp, standing.
    let f1 = live(&frames, c + 1);
    assert_eq!((f1.card.as_str(), f1.kind), ("BrokenCannon", EntityKind::Building), "on C + 1");
    assert_eq!((f1.hp, f1.max_hp), (fc.hp, 1809), "hp and max hp kept, no drain yet on C + 1");
    assert_eq!(f1.speed, 0, "the cannon does not walk");
    // Its target and its attack clock go on: progress + 50 a tick across the change.
    assert_eq!(f1.target, Some(tower), "the target is kept (no ResetTarget)");
    assert_eq!(f1.attack_ms, fc.attack_ms + 50, "the attack progress runs on across the change");
    let f2 = live(&frames, c + 2);
    assert_eq!(f2.attack_ms, f1.attack_ms + 50);
    // C + 2: the first drain, 3 a tick until the tower's next shot (16 ticks after the crossing one).
    for k in c + 2..c + 16 {
        assert_eq!(live(&frames, k - 1).hp - live(&frames, k).hp, 3, "the drain on C + {}", k - c);
    }
    // A launch every 18 ticks straight through: one before the change and two after.
    let launches: Vec<u32> = frames.iter().flatten().take(c + 46).filter(|f| f.phase == AttackPhase::Cooldown).map(|f| f.tick).collect();
    let (before, after) = (launches.iter().filter(|t| **t < fc.tick).count(), launches.iter().filter(|t| **t > f1.tick).count());
    assert!(before >= 1 && after >= 2, "the scene drifted: launches {launches:?} around the change on {}", f1.tick);
    assert!(launches.windows(2).all(|w| w[1] - w[0] == 18), "the launches keep one 18-tick cadence across the change: {launches:?}");
    // One entity from first to last: every frame is read through the Cart's own id, so that id resolves on each frame
    // until the unit falls. The tower's shots (109 every 16 ticks) and the drain (3 a tick) bring the cannon down inside
    // this run, about 85 ticks after the crossing, so the check reads the frames and not the battle after the run.
    let standing = frames.iter().take_while(|f| f.is_some()).count();
    assert!(standing > c + 46, "one entity from first to last: the Cart's id stopped resolving on recorded frame {standing}");
}

/// Plant: transform_keeps_old_lifetime.
#[test]
fn the_broken_cannon_drains_three_a_tick_on_its_max_hp() {
    let (mut s, cart, _) = mortar_scene(shipped_arms(), "MovingCannon");
    cross_in_mortar_scene(&mut s, cart, 860);
    s.tick();
    assert_eq!(s.entity(cart).expect("the cannon").card, "BrokenCannon", "precondition: the change on the next tick");
    // 301 hundredths = 1809 x 100000 / 30000 / 20: the max hp is the base, not the 860 it broke at (143).
    assert_eq!(s.lifetime_drain(cart), 301);
    let mut hp = s.entity(cart).unwrap().hp;
    assert_eq!(hp, 860, "no drain on the change tick");
    for k in 1..=20 {
        s.tick();
        let now = s.entity(cart).expect("the cannon stands").hp;
        assert_eq!(hp - now, 3, "the drain on the {k}th tick after the change");
        hp = now;
    }
}

// ---------------------------------------------------------------------------
// (4) a melee crossing and the tick order

/// A Blue Knight created BEFORE a Red Cannon Cart at 1001 hp, in melee reach of each other and out of every tower's:
/// the Knight's strike (202 at level 11) takes the Cart to 799. Returns the recorded frames and the crossing index.
fn melee_crossing(cfg: BattleConfig) -> (Vec<Option<Frame>>, usize) {
    let mut s = BattleState::new(0, cfg);
    s.scenario_spawn_now(Team::Blue, "Knight", at((9500, 13000)), None).expect("the Knight, created first");
    let cart = s.scenario_spawn_now(Team::Red, "MovingCannon", at((9500, 14500)), Some(1001)).expect("the Cart, created second");
    let frames = run(&mut s, cart, 60);
    let c = crossing(&frames, 1001);
    assert_eq!(live(&frames, c).hp, 799, "precondition: one Knight strike of 202 at level 11 took the Cart from 1001");
    (frames, c)
}

/// Plants: transform_never_fires, transform_trigger_on_resolve (the client16402 half).
#[test]
fn a_melee_crossing_is_read_on_its_tick_under_the_sequential_strike_order() {
    // match.TICK_ORDER = client_sequential_strike: the Knight's strike lands in its own turn, and the Cart, later in
    // the order, reads its threshold in its own: it is its cannon on the crossing frame and drains first on C + 1.
    // Measured on client 15.535.29 (one run: 1001 -> 799 on C, 796 on C + 1).
    let mut cfg = shipped_arms();
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    let (frames, c) = melee_crossing(cfg);
    assert_eq!(live(&frames, c).card, "BrokenCannon", "under the sequential order the change is on the crossing frame");
    assert_eq!(live(&frames, c + 1).hp, 796, "the first drain on C + 1");
    // The shipped match.TICK_ORDER = client16402: every strike lands at the end of the tick, so the Cart reads it on
    // C + 1 and drains first on C + 2. The engine's reading; the client's order is the other.
    let cfg = shipped_arms();
    assert_eq!(cfg.calib.tick_order, TickOrder::Client16402, "the shipped match.TICK_ORDER");
    let (frames, c) = melee_crossing(cfg);
    assert_eq!(live(&frames, c).card, "MovingCannon", "under client16402 the Cart is still itself on the crossing frame");
    assert_eq!(live(&frames, c + 1).card, "BrokenCannon");
    assert_eq!((live(&frames, c + 1).hp, live(&frames, c + 2).hp), (799, 796), "the first drain on C + 2");
}

// ---------------------------------------------------------------------------
// (5), (6) synthetic rows

/// Shifter: a troop that becomes Husk at 50 % of its hitpoints. Husk differs from it in every column the rebind
/// reads: hitpoints, damage, radius, speed, range, sight, kind, mass, DeployTime and LifeTime.
const SHIFTER: &str = r#"{ "version": "test", "cards": [
 { "name":"Shifter", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":1000, "damage":40, "hit_speed_ms":1000,
   "load_time_ms":500, "speed":60, "range_milli":1500, "sight_range_milli":5500, "collision_radius_milli":500, "mass":5,
   "deploy_time_ms":1000,
   "action_graph":{"roots":{"OnStartingAction":"ShifterAtHealth"},"class_types":["ActionChangeGameObjectData","ActionRunActionAtHealth"],"spawns":[],"mechanic":true},
   "transform_at_hp":{"into":"Husk","reset_target":false,"group_delays_ms":[],"at":0,"pct":50,"noop_spawns":[]} }
], "units": {
 "Husk": { "name":"Husk", "source_table":"buildings", "rarity":"Common", "hitpoints":3000, "damage":90, "hit_speed_ms":2000,
   "load_time_ms":700, "speed":0, "range_milli":4000, "sight_range_milli":4500, "collision_radius_milli":900, "mass":7,
   "deploy_time_ms":1500, "lifetime_ms":40000 }
} }"#;

fn shifter_db(json: &str) -> CardDb {
    CardDb::from_json_str(json, CardSource::DerivedJson).expect("the synthetic file parses")
}

/// A battle on `json` at level 1 (Common's first: every multiplier 100 %) with one Blue Shifter far from everything.
fn shifter_battle(json: &str, f: impl FnOnce(&mut BattleConfig)) -> (BattleState, EntityId) {
    let mut cfg = BattleConfig::with_cards(shifter_db(json));
    cfg.card_level = [1, 1];
    f(&mut cfg);
    let mut s = BattleState::new(0, cfg);
    let id = s.scenario_spawn_now(Team::Blue, "Shifter", t(900, 1000), None).expect("place the Shifter");
    assert_eq!(s.entity(id).unwrap().max_hp, 1000, "precondition: the row's hitpoints at level 1");
    (s, id)
}

/// Plant: transform_never_fires.
#[test]
fn the_threshold_compares_at_or_below() {
    // No client run landed a hit exactly on the line: at_or_below is a guess (the column's name). Both arms, by name.
    for (compare, on_the_line) in [(TransformCompare::AtOrBelow, true), (TransformCompare::StrictlyBelow, false)] {
        let (mut s, id) = shifter_battle(SHIFTER, |c| c.calib.transform_compare = compare);
        s.tick();
        assert!(s.debug_set_hp(id, 500), "exactly on the line: 500 x 100 = 1000 x 50");
        s.tick();
        let changed = s.entity(id).unwrap().card == "Husk";
        assert_eq!(changed, on_the_line, "{compare:?} at exactly half");
        // One hitpoint below the line changes it under either arm.
        assert!(s.debug_set_hp(id, 499));
        s.tick();
        assert_eq!(s.entity(id).unwrap().card, "Husk", "{compare:?} below the line");
    }
}

/// Plants: transform_never_fires, transform_hp_refilled, transform_keeps_old_lifetime.
#[test]
fn the_rebind_takes_every_row_column_from_the_new_row_and_keeps_the_entity() {
    let (mut s, id) = shifter_battle(SHIFTER, |_| {});
    s.tick();
    let before = frame(&s, id).unwrap();
    assert!(s.debug_set_hp(id, 400));
    s.tick();
    let e = s.entity(id).expect("the same entity");
    assert_eq!((e.card, e.kind, e.id), ("Husk", EntityKind::Building, id), "the entity is the new row, in place");
    // KEPT: hp and max hp (the new row's 3000 would be a new entity's), team, position.
    assert_eq!((e.hp, e.max_hp, e.team), (400, 1000, Team::Blue), "hp and max hp kept");
    assert_eq!(e.pos, before.pos, "a building does not move, and the change moves nothing");
    // FROM THE NEW ROW: radius, speed; the deploy timer is kept (transform.REDEPLOY = none).
    assert_eq!(e.radius, royalesim::fixed::milli(900));
    assert_eq!(e.speed, 0);
    assert!(!e.deploying, "no second deploy under transform.REDEPLOY = none");
    // The columns the view does not carry, off the snapshot: the damage at the entity's level (90 at level 1) and
    // the mass.
    let snap: serde_json::Value = serde_json::from_slice(&s.save()).expect("a snapshot is JSON");
    let i = id.index as usize;
    assert_eq!(snap["ents"]["damage"][i], serde_json::json!(90), "the new row's damage");
    assert_eq!(snap["ents"]["mass"][i], serde_json::json!(7), "the new row's mass");
    // The new row's LifeTime, on the kept max hp: 1000 x 100000 / 40000 / 20 = 125 hundredths a tick.
    assert_eq!(s.lifetime_drain(id), 125, "the new row's LifeTime drains the kept max hp");
    // The old arm of transform.REDEPLOY serves the new row's DeployTime instead.
    let (mut s, id) = shifter_battle(SHIFTER, |c| c.calib.transform_redeploy = TransformRedeploy::NewRowDeployTime);
    s.tick();
    assert!(s.debug_set_hp(id, 400));
    s.tick();
    let e = s.entity(id).unwrap();
    assert_eq!(e.card, "Husk");
    assert!(e.deploying && e.deploy_ms <= 1500 && e.deploy_ms >= 1450, "new_row_deploy_time: Husk's 1500 ms, less at most one countdown ({})", e.deploy_ms);
}

// ---------------------------------------------------------------------------
// (7), (8) the Goblin Demolisher

/// Plants: transform_never_fires, transform_trigger_on_resolve, transform_group_delay_ignored.
#[test]
fn a_demolisher_crossed_by_a_tower_shot_switches_two_ticks_after_its_trigger() {
    let (mut s, demo, tower) = tower_scene(shipped_arms(), "GoblinDemolisher", DEMOLISHER_AHEAD, DEMOLISHER_HP);
    let frames = run(&mut s, demo, 80);
    let c = crossing(&frames, DEMOLISHER_HP);
    assert!(c + 5 < frames.len(), "the scene drifted: the crossing came on recorded frame {c}");
    let fc = live(&frames, c);
    assert_eq!(fc.max_hp, 1300, "precondition: the measured max hp, 508 at level 11");
    assert!(fc.hp <= 650);
    // C + 1: the trigger, read at its own update; C + 2: the 100 ms delay still running. The ranged row both times.
    for k in [c, c + 1, c + 2] {
        assert_eq!(live(&frames, k).card, "GoblinDemolisher", "still the ranged form on C + {}", k - c);
    }
    // C + 3: the kamikaze form, same entity, max hp kept, no target on its switch tick, and its first drain.
    let f3 = live(&frames, c + 3);
    assert_eq!((f3.card.as_str(), f3.kind), ("GoblinDemolisher_kamikaze_form", EntityKind::Troop), "the switch on C + 3");
    assert_eq!(f3.max_hp, 1300);
    assert_eq!(f3.target, None, "no target on the switch tick (ResetTarget)");
    assert_eq!(live(&frames, c + 2).hp - f3.hp, 3, "the first drain on the switch tick");
    // C + 4: a building target.
    let f4 = live(&frames, c + 4);
    assert_eq!(f4.target, Some(tower), "a building the tick after the switch");
    assert!(f4.speed > live(&frames, c + 2).speed, "the kamikaze form's Speed 120 against the ranged form's 60");
}

/// Plants: transform_never_fires, troop_lifetime_ignored, transform_keeps_old_lifetime.
#[test]
fn the_kamikaze_form_drains_3_25_a_tick() {
    let (mut s, demo, _) = mortar_scene(shipped_arms(), "GoblinDemolisher");
    cross_in_mortar_scene(&mut s, demo, 585);
    let frames = run(&mut s, demo, 6);
    // the trigger on the next tick and the switch at the top of the one after the next: the recorded frame 2 is C + 3
    assert_eq!(live(&frames, 2).card, "GoblinDemolisher_kamikaze_form", "precondition: the switch on C + 3");
    // 325 hundredths = 1300 x 100000 / 20000 / 20, and 585 -> 582, 579, 576, 572 as measured.
    assert_eq!(s.lifetime_drain(demo), 325);
    let hps: Vec<i32> = (1..=5).map(|k| live(&frames, k).hp).collect();
    assert_eq!(hps, [585, 582, 579, 576, 572], "the kamikaze form's hp from C + 2");
}

// ---------------------------------------------------------------------------
// (9)-(12) the blast

/// The per-tick steps (native) of a unit whose positions from index `from` on are `positions`, from the first tick
/// it moved.
fn steps_from(positions: &[Vec2], from: usize) -> Vec<i64> {
    positions[from..].windows(2).map(|w| dist(w[0], w[1])).skip_while(|d| *d == 0).collect()
}

/// Plants: spawned_death_projectile_unread, death_projectile_unread.
#[test]
fn the_kamikaze_blast_lands_two_ticks_after_its_last_frame() {
    // The whole chain, the new death-projectile arm by override: a tower shot crosses the Demolisher, it becomes its
    // kamikaze form, walks to the tower and dies at its own attack. On the second frame after its last the tower
    // loses 404 in full (crown 100 on the row), and a Knight 1761 from the death point loses 404 and is pushed 250 on
    // each of its first three ladder ticks. Measured on client 15.535.29 (the two runs where the form reached a tower).
    let (mut s, demo, tower) = tower_scene(with_death_projectile(), "GoblinDemolisher", DEMOLISHER_AHEAD, DEMOLISHER_HP);
    let mut knight: Option<EntityId> = None;
    let mut last: Option<(u32, Vec2)> = None;
    let mut tower_hp: Vec<(u32, i32)> = Vec::new();
    let mut knight_at: Vec<(u32, i32, Vec2)> = Vec::new();
    for _ in 0..200 {
        s.tick();
        tower_hp.push((s.tick_count(), s.entity(tower).expect("the tower stands").hp));
        if let Some(k) = knight {
            let e = s.entity(k).expect("the Knight lives");
            knight_at.push((s.tick_count(), e.hp, e.pos));
        }
        match s.entity(demo) {
            Some(e) => {
                last = Some((s.tick_count(), e.pos));
                // The kamikaze form in its attack at the tower: put a Knight 1761 from it (it stands until it dies).
                if knight.is_none() && e.card == "GoblinDemolisher_kamikaze_form" && e.attack_phase != AttackPhase::Idle {
                    let p = Vec2::new(e.pos.x + 1761 * K, e.pos.y);
                    knight = Some(s.scenario_spawn_now(Team::Blue, "Knight", p, None).expect("place the Knight"));
                }
            }
            None if last.is_some() && knight_at.len() > 8 => break,
            None => {}
        }
    }
    let (l, death_point) = last.expect("the Demolisher was seen");
    assert!(knight.is_some(), "the scene drifted: the kamikaze form never attacked the tower");
    assert!(s.entity(demo).is_none(), "the scene drifted: the kamikaze form never died");
    let hp_on = |t: u32| tower_hp.iter().find(|(x, _)| *x == t).map(|(_, h)| *h).expect("a recorded tower frame");
    assert_eq!(hp_on(l + 1), hp_on(l), "nothing lands on the tower on the frame after the last");
    assert_eq!(hp_on(l + 1) - hp_on(l + 2), 404, "the blast on the tower, in full, on the second frame after the last");
    let knight_on = |t: u32| knight_at.iter().find(|(x, _, _)| *x == t).copied().expect("a recorded Knight frame");
    let full = knight_on(l).1;
    assert_eq!(knight_on(l + 1).1, full, "the Knight untouched before the blast");
    assert_eq!(full - knight_on(l + 2).1, 404, "the blast on the Knight");
    assert!(dist(knight_on(l).2, death_point) <= 1761 + 1, "precondition: the Knight stood 1761 from the death point");
    let positions: Vec<Vec2> = knight_at.iter().filter(|(t, _, _)| *t >= l + 2).map(|(_, _, p)| *p).collect();
    let steps = steps_from(&positions, 0);
    assert!(steps.len() >= 3 && steps[..3].iter().all(|d| (249..=250).contains(d)), "the first three ladder steps: {steps:?}");
}

/// A Red kamikaze form walking at the Blue engine-Left princess tower from 7000 in front of it, killed between two
/// ticks after `walk` ticks, with a Blue Knight put `knight_off` native units beside it on the same frame. Returns
/// the Knight's (tick, hp) from the kill frame on and the kill frame.
fn killed_mid_run(cfg: BattleConfig, walk: u32, knight_off: i32) -> (Vec<(u32, i32)>, u32) {
    let (mut s, form, _) = tower_scene(cfg, "GoblinDemolisher_kamikaze_form", 7000, 600);
    let start = s.entity(form).unwrap().pos;
    for _ in 0..walk {
        s.tick();
    }
    let e = s.entity(form).expect("the kamikaze form walks");
    assert!(dist(start, e.pos) > 0 && e.attack_phase == AttackPhase::Idle, "precondition: killed mid-run, not at its attack");
    let p = e.pos;
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", Vec2::new(p.x + knight_off * K, p.y), None).expect("place the Knight");
    assert!(s.debug_set_hp(form, 0), "the kill");
    let kill = s.tick_count();
    let mut out = vec![(kill, s.entity(knight).unwrap().hp)];
    for _ in 0..4 {
        s.tick();
        assert!(s.entity(form).is_none(), "the kamikaze form died on the tick after its kill");
        out.push((s.tick_count(), s.entity(knight).expect("the Knight lives").hp));
    }
    (out, kill)
}

/// Plants: spawned_death_projectile_unread, death_projectile_unread.
#[test]
fn a_kamikaze_killed_mid_run_still_blasts() {
    // Killed mid-run, a Knight 2545 from the death point loses 404 on the second frame after the last. Measured on
    // client 15.535.29 (the run where a Knight killed it on its way). The kill frame is the last one the form is seen alive on.
    let (hp, kill) = killed_mid_run(with_death_projectile(), 5, 2545);
    assert_eq!(hp[1].1, hp[0].1, "nothing lands on the frame after the last ({kill} + 1)");
    assert_eq!(hp[1].1 - hp[2].1, 404, "the blast on the second frame after the last");
}

/// Plant: death_projectile_unread.
#[test]
fn the_ranged_form_killed_from_above_half_in_one_tick_blasts_and_never_transforms() {
    // Killed from full hp in one tick (a Rocket in the measured run), the ranged form never reaches its threshold and
    // its own death projectile goes off: a Knight 2280 away loses 404 on the second frame after its last. Measured on
    // client 15.535.29 (the run where a Rocket killed it).
    let (mut s, demo, _) = mortar_scene(with_death_projectile(), "GoblinDemolisher");
    for _ in 0..30 {
        s.tick();
        assert_eq!(s.entity(demo).expect("the Demolisher stands").card, "GoblinDemolisher");
    }
    let p = s.entity(demo).unwrap().pos;
    assert_eq!(s.entity(demo).unwrap().hp, 1300, "precondition: at full hp");
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", Vec2::new(p.x + 2280 * K, p.y), None).expect("place the Knight");
    let full = s.entity(knight).unwrap().hp;
    assert!(s.debug_set_hp(demo, 0), "the one-tick kill from above half");
    s.tick();
    assert!(s.entity(demo).is_none() && s.scheduled().is_empty(), "dead, and never scheduled a change");
    assert_eq!(s.entity(knight).unwrap().hp, full, "nothing on the frame after the last");
    s.tick();
    assert_eq!(full - s.entity(knight).unwrap().hp, 404, "the ranged form's blast on the second frame after its last");
}

/// Plant: death_projectile_unread (the new arm's own test above goes red with it; this one is the shipped arm).
#[test]
fn the_shipped_arm_leaves_the_demolisher_without_its_blast() {
    // spawner.DEATH_SPAWN_PROJECTILE ships `none`, today's engine: the Goblin Demolisher loads and dies with nothing
    // following. The gap, pinned: the flip is a ledger change of its own (it also moves the loaded Phoenix and Goblin
    // Party Hut), and this test changes with it.
    let cfg = shipped_arms();
    assert_eq!(cfg.calib.death_spawn_projectile, DeathSpawnProjectile::None, "the shipped spawner.DEATH_SPAWN_PROJECTILE");
    let (hp, _) = killed_mid_run(cfg, 5, 2545);
    assert!(hp.iter().all(|(_, h)| *h == hp[0].1), "no blast under the shipped none: {hp:?}");
}

// ---------------------------------------------------------------------------
// (13), (14) the pending change is state

/// The Mortar scene's Demolisher one tick after its trigger: one entry pending.
fn pending() -> (BattleState, EntityId) {
    let (mut s, demo, _) = mortar_scene(shipped_arms(), "GoblinDemolisher");
    cross_in_mortar_scene(&mut s, demo, 585);
    s.tick();
    assert_eq!(scheduled_for(&s, demo), [100], "precondition: the trigger scheduled the change 100 ms out");
    (s, demo)
}

/// Plants: scheduled_dropped_on_save, transform_group_delay_ignored.
#[test]
fn a_pending_transformation_survives_a_save() {
    let (mut s, demo) = pending();
    let mut l = BattleState::load(&s.save()).expect("the save loads");
    assert_eq!(l.state_hash(), s.state_hash());
    for k in 1..=4 {
        s.tick();
        l.tick();
        assert_eq!(l.state_hash(), s.state_hash(), "diverged {k} ticks after the load");
    }
    assert_eq!(l.entity(demo).unwrap().card, "GoblinDemolisher_kamikaze_form", "the loaded battle switched too");
}

/// Plants: hash_skips_scheduled, scheduled_dropped_on_save, transform_group_delay_ignored.
#[test]
fn a_pending_transformation_is_hashed() {
    let (s, _) = pending();
    let hashed = edit_is_hashed(&s, |v| {
        let ms = v["scheduled"][0]["ms"].as_i64().expect("the snapshot carries the pending change");
        v["scheduled"][0]["ms"] = serde_json::Value::from(ms + 50);
    });
    assert!(hashed, "a save edited only in the pending change's clock loads under the old hash: it is not hashed");
}

// ---------------------------------------------------------------------------
// (15), (16) the mirror and the second crossing

/// Plant: transform_never_fires.
#[test]
fn both_seats_transform_on_the_same_tick() {
    let mut s = BattleState::new(0, level11(symmetric_config()));
    let t = s.arena().princess_tower_pos(Team::Blue, Lane::Left);
    let red = Vec2::new(t.x, t.y + DEMOLISHER_AHEAD * K);
    let blue = mirror(&s, red);
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "GoblinDemolisher", red, Some(DEMOLISHER_HP)), (Team::Blue, "GoblinDemolisher", blue, Some(DEMOLISHER_HP))])
        .expect("place both");
    let mut switched = 0;
    for _ in 0..60 {
        s.tick();
        check_mirror(&s).unwrap_or_else(|e| panic!("{e}"));
        let forms = ids.iter().filter(|id| s.entity(**id).is_some_and(|e| e.card == "GoblinDemolisher_kamikaze_form")).count();
        assert!(forms != 1, "tick {}: one seat switched and the other did not", s.tick_count());
        switched += u32::from(forms == 2);
    }
    assert!(switched > 0, "vacuous: neither Demolisher switched");
}

/// Plants: transform_never_fires, transform_group_delay_ignored, transform_heal_cancels,
/// transform_rescheduled_while_pending.
#[test]
fn a_unit_healed_back_above_the_line_schedules_nothing_twice() {
    // THE DEMOLISHER. Its change is scheduled 100 ms out on C + 1 and fires at the top of C + 3's Status phase, before
    // that tick's Target phase reads the threshold, so one update reads it with the change pending: C + 2's.
    // (a) Still below the line on that update: nothing more is scheduled.
    let (mut s, demo) = pending();
    s.tick();
    assert_eq!(scheduled_for(&s, demo), [50], "one entry, 50 ms left");
    // (b) Healed back above the line before that update (a Heal Spirit in the game): the update reads the unit above
    // the line, and the pending change stands. Below the line again, the change fires on its own tick, once.
    let (mut s, demo) = pending();
    assert!(s.debug_set_hp(demo, 1000));
    s.tick();
    assert_eq!(scheduled_for(&s, demo), [50], "the heal cancelled the pending change, or scheduled another");
    assert_eq!(s.entity(demo).unwrap().card, "GoblinDemolisher", "still itself while the change is pending");
    assert!(s.debug_set_hp(demo, 585));
    s.tick();
    assert_eq!(s.entity(demo).unwrap().card, "GoblinDemolisher_kamikaze_form", "the change read once is carried out on its tick");
    assert!(s.scheduled().is_empty(), "a second change was scheduled");

    // A LONGER WAIT, so that a heal and a second crossing are both read with the change pending. No loaded row has
    // one: the Demolisher's window holds one update, and the Cannon Cart changes on the update that reads its crossing,
    // so nothing of it is pending. The synthetic Shifter with a 300 ms group delay: scheduled on the update that reads
    // its crossing, T, and changed at the top of T + 6's Status phase.
    let slow = SHIFTER
        .replace(r#""ActionChangeGameObjectData","ActionRunActionAtHealth""#, r#""ActionChangeGameObjectData","ActionGroup","ActionRunActionAtHealth""#)
        .replace(r#""group_delays_ms":[],"at":0"#, r#""group_delays_ms":[300],"at":0"#);
    let (mut s, id) = shifter_battle(&slow, |_| {});
    s.tick();
    assert!(s.debug_set_hp(id, 400), "below the line: 400 x 100 < 1000 x 50");
    s.tick();
    let t = s.tick_count();
    assert_eq!(scheduled_for(&s, id), [300], "precondition: the crossing scheduled the change 300 ms out");
    // Healed above the line, below it again, above, below: every update reads the unit with the change pending.
    for (k, hp) in [(1, 800), (2, 400), (3, 800), (4, 400)] {
        assert!(s.debug_set_hp(id, hp));
        s.tick();
        assert_eq!(scheduled_for(&s, id), [300 - 50 * k], "T + {k}, read at {hp}: the one entry, counting down");
        assert_eq!(s.entity(id).unwrap().card, "Shifter", "T + {k}: changed before its entry came due");
    }
    s.tick();
    assert_eq!(s.entity(id).unwrap().card, "Shifter", "T + 5: changed before its entry came due");
    s.tick();
    assert_eq!(s.tick_count(), t + 6);
    assert_eq!(s.entity(id).expect("the same entity").card, "Husk", "the change on T + 6, the first entry's tick");
    assert!(s.scheduled().is_empty(), "a second change is pending");
}

// ---------------------------------------------------------------------------
// (17) the refusals

/// Plant: transform_block_unchecked.
#[test]
fn a_transformation_graph_with_one_more_class_is_refused_with_todays_message() {
    let why = |json: &str| -> Option<String> {
        let db = shifter_db(json);
        match db.index("Shifter") {
            Some(i) => {
                assert!(db.get(i).transform_at_hp.is_some(), "loaded without its block");
                None
            }
            None => Some(db.rejected.iter().find(|(n, _)| n == "Shifter").map(|(_, w)| w.clone()).expect("Shifter is listed as rejected")),
        }
    };
    assert_eq!(why(SHIFTER), None, "the control loads");
    let today = "the unit runs an action graph this loader does not read (";
    // One more class in the graph.
    let extra = SHIFTER.replace(r#""class_types":["ActionChangeGameObjectData","#, r#""class_types":["ActionChangeGameObjectData","ActionDealDamage","#);
    let got = why(&extra).expect("a graph with one more class is refused");
    assert!(got.starts_with(today) && got.contains("ActionDealDamage"), "{got}");
    // A spawn the block does not list as a no-op.
    let spawn = SHIFTER.replace(r#""ActionRunActionAtHealth"],"spawns":[]"#, r#""ActionRunActionAtHealth","ActionSpawn"],"spawns":["AreaEffectType:Taunter"]"#);
    let got = why(&spawn).expect("an unlisted spawn is refused");
    assert!(got.starts_with(today) && got.contains("AreaEffectType:Taunter"), "{got}");
    // A group whose SubActionsDelay reads two ways: 100 before the change, whose own entry is 0.
    let two_ways = SHIFTER
        .replace(r#""ActionChangeGameObjectData","ActionRunActionAtHealth""#, r#""ActionChangeGameObjectData","ActionGroup","ActionRunActionAtHealth""#)
        .replace(r#""group_delays_ms":[],"at":0"#, r#""group_delays_ms":[100,0],"at":1"#);
    assert_eq!(why(&two_ways).as_deref(), Some("the transformation's SubActionsDelay [100, 0] reads two ways; not simulated"));
    // A target row that carries a periodic spawner (state a rebind would not start).
    let spawner = SHIFTER
        .replace(r#""deploy_time_ms":1500, "lifetime_ms":40000 }"#, r#""deploy_time_ms":1500, "lifetime_ms":40000, "spawner":{"character":"Imp","number":1,"pause_time_ms":5000} },
 "Imp": { "name":"Imp", "rarity":"Common", "hitpoints":80, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 }"#);
    assert_eq!(why(&spawner).as_deref(), Some("units.Husk: a transformation into a row with a periodic spawner is not simulated"));
    // Two action blocks on one row.
    let two_blocks = SHIFTER.replace(
        r#""transform_at_hp":"#,
        r#""life_state_spawner":{"action_delay_ms":1000,"spawn_interval_ms":2200,"character":"Husk","number":1,"offset_milli":1200,"offset_angle_deg":20,"object_filter":"DefaultCharacterTargets"}, "transform_at_hp":"#,
    );
    assert_eq!(
        why(&two_blocks).as_deref(),
        Some("the unit carries more than one action block (a life-state controller and a transformation); not simulated")
    );
}

// ---------------------------------------------------------------------------
// (18) the taunt cancel is a no-op only while nothing taunts

/// Every row of a cards.json document whose action graph runs an ActionTaunt, as "table.name".
fn taunting_rows(doc: &serde_json::Value) -> Vec<String> {
    let taunts = |row: &serde_json::Value| {
        row.pointer("/action_graph/class_types").and_then(|c| c.as_array()).is_some_and(|c| c.iter().any(|x| x == "ActionTaunt"))
    };
    let mut out = Vec::new();
    for table in ["cards", "towers"] {
        for row in doc[table].as_array().into_iter().flatten() {
            if taunts(row) || row.get("projectile").is_some_and(taunts) {
                out.push(format!("{table}.{}", row["name"].as_str().unwrap_or("?")));
            }
        }
    }
    #[cfg(not(clash_plant = "taunt_guard_blind"))]
    let tables = ["units", "projectiles", "area_effect_objects"];
    #[cfg(clash_plant = "taunt_guard_blind")]
    let tables = ["units", "projectiles"]; // PLANT: the guard stops reading the area table.
    for table in tables {
        for (name, row) in doc[table].as_object().into_iter().flatten() {
            if taunts(row) {
                out.push(format!("{table}.{name}"));
            }
        }
    }
    out
}

/// Plant: taunt_guard_blind.
#[test]
fn only_the_demolishers_taunt_cancel_carries_an_action_taunt() {
    // The loader takes the Goblin Demolisher's CancelTauntAEO spawn as a no-op because the engine has no taunt: that
    // reading is only sound while no row the engine could load taunts. Over the repo cards.json.
    let path = format!("{}/../../data/derived/cards.json", env!("CARGO_MANIFEST_DIR"));
    let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).expect("cards.json");
    assert_eq!(taunting_rows(&doc), ["area_effect_objects.CancelTauntAEO"]);
    // The one spawn of it is the Goblin Demolisher's, on its card row and its unit row.
    let spawners: Vec<String> = doc["cards"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r.pointer("/action_graph/spawns").and_then(|s| s.as_array()).is_some_and(|s| s.iter().any(|x| x == "AreaEffectType:CancelTauntAEO")))
        .map(|r| r["name"].as_str().unwrap_or("?").to_string())
        .collect();
    assert_eq!(spawners, ["GoblinDemolisher"]);
    // The guard sees a taunt anywhere: a card row planted with one is reported.
    let mut planted = doc.clone();
    planted["cards"][0]["action_graph"] = serde_json::json!({"roots": {}, "class_types": ["ActionTaunt"], "spawns": [], "mechanic": true});
    assert_eq!(taunting_rows(&planted).len(), 2, "the guard is blind to a planted taunt");
}

// ---------------------------------------------------------------------------
// (19) the catalogue

/// Plant: unit_refs_skips_new_paths.
#[test]
fn a_transformed_unit_reports_its_catalogue_card() {
    let s = BattleState::new(0, level11(config()));
    let db = s.cards();
    let catalogue = [db.index("MovingCannon").unwrap(), db.index("GoblinDemolisher").unwrap()];
    let ids = royalesim::py::ids_of_indices(db, &catalogue);
    let cannon = db.get(catalogue[0]).transform_at_hp.unwrap().unit;
    let form = db.get(catalogue[1]).transform_at_hp.unwrap().unit;
    assert_eq!((ids[cannon as usize], ids[form as usize]), (0, 1), "BrokenCannon reports the Cannon Cart, the kamikaze form the Goblin Demolisher");
}

// ---------------------------------------------------------------------------
// the old arms

/// Plants: transform_never_fires; the arm is the plant transform_trigger_on_resolve's.
#[test]
fn the_crossing_tick_arm_changes_the_cart_on_its_crossing_tick() {
    // transform.HEALTH_TRIGGER_TIMING = crossing_tick_resolve: read right after the tick's damage lands, so the Cart is
    // its cannon on the crossing frame and drains first on C + 1, one tick before what was measured.
    let mut cfg = shipped_arms();
    cfg.calib.transform_timing = TransformTiming::CrossingTickResolve;
    let (mut s, cart, _) = tower_scene(cfg, "MovingCannon", CART_AHEAD, CART_HP);
    let frames = run(&mut s, cart, 80);
    let c = crossing(&frames, CART_HP);
    assert_eq!(live(&frames, c).card, "BrokenCannon", "the change on the crossing frame");
    assert_eq!(live(&frames, c).hp - live(&frames, c + 1).hp, 3, "the first drain on C + 1");
}

/// Plant: transform_never_fires.
#[test]
fn the_reset_always_arm_drops_the_carts_target_on_the_switch() {
    // transform.ATTACK_STATE = reset_always: even a change without ResetTarget drops the target and the attack, and
    // the unit decides again on the next tick.
    let mut cfg = shipped_arms();
    cfg.calib.transform_attack_state = TransformAttackState::ResetAlways;
    let (mut s, cart, tower) = tower_scene(cfg, "MovingCannon", CART_AHEAD, CART_HP);
    let frames = run(&mut s, cart, 80);
    let c = crossing(&frames, CART_HP);
    let f1 = live(&frames, c + 1);
    assert_eq!(f1.card, "BrokenCannon");
    assert_eq!((f1.target, f1.phase, f1.attack_ms), (None, AttackPhase::Idle, 0), "no target and a fresh attack on the switch");
    assert_eq!(live(&frames, c + 2).target, Some(tower), "the tower taken again the tick after");
}

/// Plant: transform_never_fires.
#[test]
fn the_new_row_deploy_time_arm_deploys_the_cannon() {
    // transform.REDEPLOY = new_row_deploy_time: the cannon serves BrokenCannon's DeployTime (1000) and drains nothing
    // while it deploys.
    let mut cfg = shipped_arms();
    cfg.calib.transform_redeploy = TransformRedeploy::NewRowDeployTime;
    let (mut s, cart, _) = tower_scene(cfg, "MovingCannon", CART_AHEAD, CART_HP);
    let frames = run(&mut s, cart, 80);
    let c = crossing(&frames, CART_HP);
    let hp = live(&frames, c).hp;
    for k in c + 1..c + 16 {
        let f = live(&frames, k);
        assert_eq!(f.card, "BrokenCannon");
        assert!(f.deploying, "deploying on C + {}", k - c);
        assert_eq!(f.hp, hp, "no drain while it deploys (C + {})", k - c);
    }
}
