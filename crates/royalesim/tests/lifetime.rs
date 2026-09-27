//! A LIFETIME BUILDING'S HITPOINTS: calibration `lifetime.HP_DECAY`,
//! state.rs `lifetime_drain_per_tick` / `phase_status`.
//!
//! THE LAW, measured on the live corpus (the key's provenance: 92.2 % of 31113
//! building frames exact, every residual a hit the building took): a building with a
//! LifeTime does not keep its hitpoints and die at the end -- it BLEEDS them away over
//! that time, from its deploy-end tick, and dies when the pool runs out. The per-tick
//! rate is derived once, in HUNDREDTHS of a hitpoint, by two truncating integer
//! divisions:
//!
//! > drain = ((maxHitpoints * 100000) / LifeTime_ms) / 20
//!
//! and an accumulator (engine `lifetime_acc`) takes whole hitpoints off the hp pool
//! as it crosses 100.
//!
//! WHAT IS PINNED, every number read from the loaded CardDb (nothing pasted):
//!   1. the rate itself, on every deck building that ships a LifeTime, at two card
//!      levels, computed from the card's own LifeTime and the entity's own max hp.
//!      (The two divisions are the same integer as one -- nested floor division --
//!      so what makes the law fit is not them but the truncation of the RATE to
//!      hundredths before it is accumulated, which (2) pins; the exact fraction
//!      recomputed every tick fits 46 % of the live frames.);
//!   2. the hp curve tick by tick: hp = max_hp - (k x drain) / 100 for every tick k of
//!      the building's whole life, with the FIRST drain on the tick after its deploy
//!      end and none while it deploys;
//!   3. the death tick: ceil(max_hp x 100 / drain) ticks past the deploy end -- one or
//!      two past the LifeTime column, never before it -- and the building is gone that
//!      tick, not the next;
//!   4. damage and the drain share ONE pool: a Fireball on a draining Tesla takes its
//!      damage off the drained hp, and the Tesla dies that much earlier, by exactly
//!      the ticks the damage is worth;
//!   5. a death BY DRAIN goes through the normal death path: a Tombstone that bleeds
//!      out leaves its four Skeletons, as a killed one does;
//!   6. the other arm (expiry_hit, the earlier engine) is runnable and
//!      differs: full hp for the whole life, then one hit at the LifeTime tick;
//!   7. a card with no LifeTime never drains (a Knight, a crown tower);
//!   8. the kind guard: a LifeTime on a non-building is the Goblin Demolisher's kamikaze form's alone, a troop row
//!      that only a transformation reaches (card.rs; lifetime.TROOP_LIFETIME), and it drains;
//!   9. a troop with a LifeTime reached any other way is still refused (a synthetic death spawn), and so is the 2018
//!      Cannon Cart's BrokenCannon, a troop named by a death spawn in that table;
//!  10. a troop's LifeTime runs lifetime.TROOP_LIFETIME's arm: the building law (same_as_buildings, shipped,
//!      measured) or the expiry (expiry_at_lifetime), both by name.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test lifetime`):
//!   * `lifetime_expiry_hit`, run 2026-09-21, forces the expiry arm whatever the ledger says -- 5 of the 8 tests then
//!     here go red, (2) and (3) (the curve and the death tick), (4), (5) and the save, plus (6), whose own control is
//!     the shipped arm the plant also overrides. (1) and (7) and the kind guard stay green: the rate is arithmetic and
//!     a card with no LifeTime never drained under either arm.
//!   * `troop_lifetime_ignored` -- a troop never gets its LifeTime: (8) and (10) red.
//!   * `troop_lifetime_refusal_lifted` -- a troop with a LifeTime loads whatever reaches it: (9) red (and the 2018
//!     census in tests/loadable_census.rs).
//!   * `census_admits_one` (tests/loadable_census.rs's) -- the rejected 2018 Cannon Cart is kept: (9)'s 2018 half red.

mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, UnitRef};
use royalesim::entity::EntityKind;
use royalesim::fixed::Vec2;
use royalesim::state::{BattleConfig, BattleState, Calib, LifetimeDecay, TroopLifetime};
use royalesim::{EntityId, Team};

fn calib() -> Calib {
    Calib::shipped()
}

fn bare(cfg: BattleConfig) -> BattleState {
    BattleState::new(11, cfg)
}

fn with_calib(f: impl FnOnce(&mut Calib)) -> BattleConfig {
    let mut cfg = config();
    f(&mut cfg.calib);
    cfg
}

fn at_level(level: i32) -> BattleConfig {
    let mut cfg = config();
    cfg.card_level = [level, level];
    cfg
}

/// A spot on Blue's half clear of every tower footprint and the river.
fn spot() -> Vec2 {
    t(900, 1000)
}

/// THE MEASURED DERIVATION, restated here from the card data so the test can fail
/// if the engine's changes: hundredths of a hitpoint per tick, two truncating
/// divisions.
fn want_drain(max_hp: i32, lifetime_ms: i32) -> i32 {
    (max_hp as i64 * 100_000 / lifetime_ms as i64 / 20) as i32
}

/// ceil(a / b) for positive b (i64::div_ceil is not stable on this toolchain).
fn ceil_div(a: i64, b: i64) -> i64 {
    (a + b - 1) / b
}

/// Every DECK card that is a building with a LifeTime, with that LifeTime (the unit
/// rows a card summons carry the same names and are not deck cards, so `index` is
/// what decides).
fn lifetime_buildings(s: &BattleState) -> Vec<(String, i32)> {
    let db = s.cards();
    (0..db.cards.len())
        .map(|i| db.get(i as u16))
        .filter(|c| c.kind == royalesim::card::CardKind::Building)
        .filter(|c| db.index(&c.name).is_some())
        .filter_map(|c| c.lifetime_ms.map(|l| (c.name.clone(), l)))
        .collect()
}

// ---------------------------------------------------------------------------
// (1) the rate

#[test]
fn the_drain_rate_is_two_truncating_divisions_of_the_cards_own_numbers() {
    // Untouched by the plant: this is arithmetic, not the arm.
    let names: Vec<String> = lifetime_buildings(&bare(config())).into_iter().map(|(n, _)| n).collect();
    assert!(names.len() >= 4, "data: only {} lifetime buildings are simulable ({names:?})", names.len());
    let mut parted = 0;
    let mut seen = 0;
    // two LEVELS, so the rate is read against two different max_hp on the same card
    let levels = [config().card_level[0], config().card_level[0] - 2];
    for level in levels {
        let mut s = bare(at_level(level));
        for name in &names {
            let life = card_stat(&s, name).lifetime_ms.expect("a lifetime building");
            let Ok(id) = s.scenario_spawn_now(Team::Blue, name, spot(), None) else { continue };
            let max = s.entity(id).unwrap().max_hp;
            // the engine's own number, for the entity actually on the board
            assert_eq!(s.lifetime_drain(id), want_drain(max, life), "{name}: the drain of a {name} with {max} max hp");
            assert!(s.lifetime_drain(id) > 0, "{name}: a drain of 0 would never kill it");
            // The law is written as two divisions; nested floor division makes that the
            // same integer as one (max_hp x TICK_MS / LifeTime in hundredths), and the
            // test asserts the identity rather than pretending the two part.
            assert_eq!(want_drain(max, life), (max as i64 * 100 * 50 / life as i64) as i32, "{name}: the two divisions are not the one");
            // WHAT DOES matter is that the rate is truncated to hundredths ONCE and then
            // accumulated: a rate that is not a whole hitpoint per tick makes the
            // accumulator the thing, and the curve test below is where it is pinned.
            parted += u32::from(want_drain(max, life) % 100 != 0);
            seen += 1;
            assert!(s.debug_set_hp(id, 0));
            s.tick();
        }
    }
    assert!(seen >= 8, "vacuous: only {seen} buildings placed");
    assert!(parted > 0, "vacuous: every shipped building drains a whole number of hitpoints per tick");
}

#[test]
fn a_card_with_no_lifetime_never_drains() {
    // A Knight and a crown tower: `lifetime_drain` is 0 and their hp does not move.
    let mut s = bare(config());
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", spot(), None).unwrap();
    let tower = s.tower_ids(Team::Blue)[0].unwrap();
    assert!(card_stat(&s, "Knight").lifetime_ms.is_none(), "data: the Knight has no LifeTime");
    assert_eq!((s.lifetime_drain(knight), s.lifetime_drain(tower)), (0, 0));
    let (kh, th) = (s.entity(knight).unwrap().hp, s.entity(tower).unwrap().hp);
    // forty ticks: the Knight is still walking up its own half, out of every tower's
    // reach, so nothing but a drain could move either number
    for _ in 0..40 {
        s.tick();
    }
    assert_eq!((s.entity(knight).unwrap().hp, s.entity(tower).unwrap().hp), (kh, th), "something with no LifeTime lost hp");
}

// ---------------------------------------------------------------------------
// (2), (3) the curve and the death tick

/// Deploy `card` WITH its deploy time and follow its hp to the tick it dies on.
/// Returns (deploy-end post-tick, the hp seen on every post-tick from 1, death tick).
fn life_of(cfg: BattleConfig, card: &str) -> (u32, Vec<i32>, u32) {
    let mut s = bare(cfg);
    s.spawn_unit(Team::Blue, card, spot(), None).unwrap();
    s.tick();
    let id = find_live(&s, Team::Blue, card)[0].id;
    let mut hp = vec![s.entity(id).unwrap().hp];
    let mut deploy_end = 0;
    let mut died = 0;
    for _ in 0..4000 {
        s.tick();
        match s.entity(id) {
            Some(e) => {
                if deploy_end == 0 && !e.deploying {
                    deploy_end = s.tick_count();
                }
                hp.push(e.hp);
            }
            None => {
                died = s.tick_count();
                break;
            }
        }
    }
    assert!(died > 0, "{card} never died");
    (deploy_end, hp, died)
}

#[test]
fn a_building_bleeds_its_lifetime_away_tick_by_tick_and_dies_when_the_pool_is_empty() {
    // Plant lifetime_expiry_hit: the hp stays full and the death tick is the column's.
    assert_eq!(calib().lifetime_hp_decay, LifetimeDecay::LinearDrain, "the shipped arm this test pins");
    let names: Vec<String> = lifetime_buildings(&bare(config())).into_iter().map(|(n, _)| n).collect();
    let mut checked = 0;
    for name in &names {
        let mut s0 = bare(config());
        if s0.spawn_unit(Team::Blue, name, spot(), None).is_err() {
            continue; // a summon-only row the deploy entry point will not place
        }
        let life = card_stat(&s0, name).lifetime_ms.expect("a lifetime building");
        let deploy = card_stat(&s0, name).deploy_time_ms;
        let (deploy_end, hp, died) = life_of(config(), name);
        let max = hp[0];
        let drain = want_drain(max, life);
        assert_eq!(deploy_end, (deploy as u32).div_ceil(calib().tick_ms as u32), "{name}: the deploy ended on the wrong tick");
        // hp[i] is the hp at post-tick i + 1. Nothing drains while it deploys; from the
        // deploy-end tick on, k ticks have drained (k x drain) / 100 whole hitpoints.
        for (i, got) in hp.iter().enumerate() {
            let tick = i as u32 + 1;
            let k = tick.saturating_sub(deploy_end) as i64;
            let want = max - (k * drain as i64 / 100) as i32;
            assert_eq!(*got, want, "{name}: hp at post-tick {tick} (deploy end {deploy_end}, drain {drain} hundredths)");
        }
        // the death tick: the first k at which the pool is empty
        let want_death = deploy_end + ceil_div(max as i64 * 100, drain as i64) as u32;
        assert_eq!(died, want_death, "{name}: died at post-tick {died}, want {want_death}");
        // never before the column, and past it by at most what the two truncations can
        // cost: the drain is under a hundredth of a hitpoint per tick slow, so after
        // the column's own ticks the pool still holds under (ticks / 100) hitpoints,
        // which is under (ticks / drain) further ticks
        let ticks = (life as u32).div_ceil(calib().tick_ms as u32);
        let column = deploy_end + ticks;
        assert!(died >= column, "{name}: died {died}, before the LifeTime column {column}");
        assert!(died - column <= ticks / drain as u32 + 1, "{name}: died {died}, {} past the LifeTime column {column} (drain {drain})", died - column);
        checked += 1;
    }
    assert!(checked >= 4, "vacuous: only {checked} buildings checked");
}

#[test]
fn the_expiry_arm_keeps_the_hitpoints_and_kills_at_the_column() {
    // The earlier arm, kept runnable: full hp for the whole life, then one
    // hit for everything it has when LifeTime is up.
    let cfg = with_calib(|c| c.lifetime_hp_decay = LifetimeDecay::ExpiryHit);
    let s0 = bare(cfg.clone());
    let life = card_stat(&s0, "Tesla").lifetime_ms.expect("data: the Tesla has a LifeTime");
    let (deploy_end, hp, died) = life_of(cfg, "Tesla");
    assert!(hp.iter().all(|h| *h == hp[0]), "expiry_hit: the hp moved");
    // the old arm's clock runs from the tick the building LANDS (phase_status pays no
    // attention to the deploy timer), not from its deploy end
    assert!(deploy_end > 1, "vacuous: the Tesla had no deploy window");
    assert_eq!(died, 1 + (life as u32).div_ceil(calib().tick_ms as u32), "expiry_hit: the death tick is the column's, from the landing");
    // and the shipped arm differs on both counts
    let (_, shipped_hp, shipped_died) = life_of(config(), "Tesla");
    assert!(shipped_hp.last() < shipped_hp.first(), "vacuous: the shipped arm did not drain");
    assert_ne!(shipped_died, died, "vacuous: the two arms die on the same tick");
}

// ---------------------------------------------------------------------------
// (4) damage and the drain share one pool

#[test]
fn damage_comes_off_the_drained_pool_and_brings_the_death_forward_by_its_own_ticks() {
    // Plant lifetime_expiry_hit: a Fireball changes nothing about the death tick.
    // A CANNON, not a Tesla: a hidden Tesla is immune to the Fireball (hide.rs).
    let mut s = bare(config());
    let tesla = s.scenario_spawn_now(Team::Blue, "Cannon", spot(), None).unwrap();
    let drain = s.lifetime_drain(tesla);
    let max = s.entity(tesla).unwrap().max_hp;
    let mut control = s.clone();
    // A Red Fireball on it; both copies run on from the same tick.
    s.spawn_unit(Team::Red, "Fireball", spot(), None).unwrap();
    let flight = run_until(&mut s, 200, |s| s.spells().is_empty() && s.tick_count() > 0);
    assert!(flight < 200, "the Fireball never landed");
    for _ in 0..flight {
        control.tick();
    }
    let hit = control.entity(tesla).unwrap().hp - s.entity(tesla).unwrap().hp;
    assert!(hit > 0, "vacuous: the Fireball did not damage the Tesla");
    // From here the two drain at the same rate, so the damage stands as a constant
    // gap -- one pool, not two.
    for _ in 0..40 {
        s.tick();
        control.tick();
        assert_eq!(control.entity(tesla).unwrap().hp - s.entity(tesla).unwrap().hp, hit, "the gap moved: the drain and the damage are not one pool");
    }
    // and it dies earlier by exactly the ticks that damage is worth
    let die = |mut s: BattleState| -> u32 {
        let n = run_until(&mut s, 4000, |s| s.entity(tesla).is_none());
        assert!(n < 4000, "the Tesla never died");
        s.tick_count()
    };
    let (hurt, whole) = (die(s), die(control));
    let want = ceil_div(max as i64 * 100, drain as i64) - ceil_div((max - hit) as i64 * 100, drain as i64);
    assert_eq!((whole - hurt) as i64, want, "a {hit}-point Fireball should shorten the Tesla by {want} ticks");
}

// ---------------------------------------------------------------------------
// (5) a death by drain is a death

#[test]
fn a_tombstone_that_bleeds_out_still_leaves_its_death_spawn() {
    // Plant lifetime_expiry_hit: the Skeletons come on a different tick (the test
    // fails on the tick, not on their absence -- the expiry arm kills it too).
    let mut s = bare(config());
    let ds = card_stat(&s, "Tombstone").death_spawn.expect("data: the Tombstone death-spawns");
    let unit = s.cards().get(ds.unit).name.clone();
    let tomb = s.scenario_spawn_now(Team::Blue, "Tombstone", spot(), None).unwrap();
    let drain = s.lifetime_drain(tomb);
    let max = s.entity(tomb).unwrap().max_hp;
    let want_death = ceil_div(max as i64 * 100, drain as i64) as u32;
    let died = run_until(&mut s, want_death + 10, |s| s.entity(tomb).is_none());
    assert_eq!(s.tick_count(), want_death, "the Tombstone bled out on post-tick {died}");
    // the burst, told from the periodic cadence by its spawner: a death spawn owes nothing to
    // one. (spawner.RELEASE_TIMING = end_of_event_phase has the burst exist on the death tick
    // itself; one more tick lets the earlier arm's queued burst materialise too.)
    s.tick();
    let burst = find_live(&s, Team::Blue, &unit).into_iter().filter(|e| e.spawned_by.is_none()).count();
    assert_eq!(burst, ds.count as usize, "a death by drain must go through the death path");
}

// ---------------------------------------------------------------------------
// the save

#[test]
fn a_snapshot_mid_drain_resumes_hitpoint_for_hitpoint() {
    // The drain's remainder (engine `lifetime_acc`) is state: a save
    // taken between two whole hitpoints must carry it, or the resumed building drifts.
    // A TOMBSTONE, whose drain is under a whole hitpoint per tick, so there really are
    // ticks on which the accumulator carries the whole of it.
    let mut s = bare(config());
    let tesla = s.scenario_spawn_now(Team::Blue, "Tombstone", spot(), None).unwrap();
    let drain = s.lifetime_drain(tesla);
    assert!(drain % 100 != 0, "data: the Tombstone's drain {drain} is whole hitpoints, so the remainder is never exercised");
    // a tick the accumulator did NOT cross a whole hitpoint on: the remainder it is
    // carrying is the state the save has to keep
    let mut last = s.entity(tesla).unwrap().hp;
    let mut held = false;
    for _ in 0..50 {
        s.tick();
        let hp = s.entity(tesla).unwrap().hp;
        held = hp == last;
        last = hp;
        if held {
            break;
        }
    }
    assert!(held, "vacuous: the Tombstone never sat between two whole hitpoints");
    let blob = s.save();
    let mut l = BattleState::load(&blob).unwrap();
    assert_eq!(l.state_hash(), s.state_hash());
    for k in 0..200 {
        s.tick();
        l.tick();
        assert_eq!(l.state_hash(), s.state_hash(), "diverged {k} ticks after the load");
    }
    assert!(s.entity(tesla).unwrap().hp < s.entity(tesla).unwrap().max_hp, "vacuous: nothing drained after the load");
}

// ---------------------------------------------------------------------------
// (8)-(10) the kind guard, and the one troop that carries a LifeTime

/// Plant: troop_lifetime_ignored.
#[test]
fn only_buildings_and_transformation_targets_carry_a_lifetime() {
    // card.rs refuses a TROOP with a LifeTime (`units.{unit} is a troop with a LifeTime`) unless a transformation is
    // what reaches it: the Goblin Demolisher's kamikaze form, and nothing else in the 15.535.29 table.
    let mut s = bare(config());
    let db = s.cards();
    let troops: Vec<u16> = (0..db.cards.len() as u16).filter(|i| db.get(*i).kind != royalesim::card::CardKind::Building && db.get(*i).lifetime_ms.is_some()).collect();
    let names: Vec<&str> = troops.iter().map(|i| db.get(*i).name.as_str()).collect();
    assert_eq!(names, ["GoblinDemolisher_kamikaze_form"], "the non-buildings with a LifeTime");
    let form = troops[0];
    assert!(db.get(form).summon_only, "the kamikaze form is never played");
    let named_by: Vec<UnitRef> = (0..db.cards.len() as u16).flat_map(|i| db.unit_refs(i)).filter(|(_, u, _)| *u == form).map(|(path, _, _)| path).collect();
    assert!(!named_by.is_empty() && named_by.iter().all(|p| *p == UnitRef::Transform), "the kamikaze form is reached through a transformation alone: {named_by:?}");
    // It drains as a building does (lifetime.TROOP_LIFETIME = same_as_buildings).
    let id = s.scenario_spawn_now(Team::Blue, "GoblinDemolisher_kamikaze_form", spot(), None).unwrap();
    assert_eq!(s.entity(id).unwrap().kind, EntityKind::Troop);
    assert!(s.lifetime_drain(id) > 0, "a troop with a LifeTime does not drain");
    // A building drains, whatever its card's kind column says (`lifetime_of`).
    let id = s.scenario_spawn_now(Team::Blue, "Tesla", t(900, 1400), None).unwrap();
    assert_eq!(s.entity(id).unwrap().kind, EntityKind::Building);
    assert!(s.lifetime_drain(id) > 0);
    // a dead entity has no drain
    assert!(s.debug_set_hp(id, 0));
    s.tick();
    assert_eq!(s.lifetime_drain(id), 0, "a dead entity still reported a drain");
    let _: EntityId = id;
}

/// Mourner death-spawns Wraith, a troop with a LifeTime; Shifter becomes it at half its hitpoints.
const WRAITH: &str = r#"{ "version": "test", "cards": [
 { "name":"Mourner", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":300, "hit_speed_ms":1000, "range_milli":1000,
   "collision_radius_milli":500, "death_spawn":{"character":"Wraith", "count":1} }
], "units": {
 "Wraith": { "name":"Wraith", "rarity":"Common", "hitpoints":1000, "damage":10, "hit_speed_ms":1000, "range_milli":500,
   "sight_range_milli":5500, "collision_radius_milli":500, "speed":60, "mass":4, "deploy_time_ms":1000, "lifetime_ms":2000 }
} }"#;

fn wraith_by_transformation() -> String {
    WRAITH.replace(
        r#""death_spawn":{"character":"Wraith", "count":1} }"#,
        r#""action_graph":{"roots":{"OnStartingAction":"MournerAtHealth"},"class_types":["ActionChangeGameObjectData","ActionRunActionAtHealth"],"spawns":[],"mechanic":true},
   "transform_at_hp":{"into":"Wraith","reset_target":false,"group_delays_ms":[],"at":0,"pct":50,"noop_spawns":[]} }"#,
    )
}

/// Plant: troop_lifetime_refusal_lifted.
#[test]
fn a_troop_with_a_lifetime_is_still_refused_as_a_death_spawn() {
    let db = CardDb::from_json_str(WRAITH, CardSource::DerivedJson).unwrap();
    assert!(db.index("Mourner").is_none(), "a death spawn of a troop with a LifeTime loaded");
    let why = db.rejected.iter().find(|(n, _)| n == "Mourner").map(|(_, w)| w.as_str());
    assert_eq!(why, Some("units.Wraith is a troop with a LifeTime; not simulated"));
    // The same row reached by a transformation loads.
    let db = CardDb::from_json_str(&wraith_by_transformation(), CardSource::DerivedJson).unwrap();
    let i = db.index("Mourner").unwrap_or_else(|| panic!("Mourner refused: {:?}", db.rejected));
    let wraith = db.get(db.get(i).transform_at_hp.expect("the transformation").unit);
    assert_eq!((wraith.name.as_str(), wraith.lifetime_ms), ("Wraith", Some(2000)));
}

/// Plants: census_admits_one, troop_lifetime_refusal_lifted.
#[test]
fn the_2018_cannon_cart_stays_refused() {
    // The 2018 table's Cannon Cart death-spawns BrokenCannon, a troop with a LifeTime there: refused as before.
    let path = format!("{}/../../data/derived/cards-2018.json", env!("CARGO_MANIFEST_DIR"));
    if !std::path::Path::new(&path).is_file() {
        if std::env::var_os("CI").is_some() {
            panic!("{path} is absent -- and CI is set, where a skip would read as a pass");
        }
        let line = format!("\nSKIP lifetime::the_2018_cannon_cart_stays_refused: {path} is absent (tools/extract_cards.py --vintage 2018 writes it). The 2018 Cannon Cart was NOT checked: this is not a pass.\n");
        let _ = std::io::Write::write_all(&mut std::io::stderr(), line.as_bytes());
        return;
    }
    let db = CardDb::load_repo_file("cards-2018.json").expect("cards-2018.json loads");
    assert!(db.index("MovingCannon").is_none(), "the 2018 Cannon Cart loaded");
    let why = db.rejected.iter().find(|(n, _)| n == "MovingCannon").map(|(_, w)| w.as_str()).unwrap_or("");
    assert!(why.contains("units.BrokenCannon is a troop with a LifeTime"), "the 2018 Cannon Cart's reason: {why:?}");
}

/// Plant: troop_lifetime_ignored.
#[test]
fn a_troop_lifetime_expires_under_the_expiry_arm() {
    // A troop with a LifeTime (Wraith, 2000 ms, 1000 hp at level 1) runs lifetime.TROOP_LIFETIME's arm, by name: the
    // building law drains it 25 a tick (1000 x 100000 / 2000 / 20 = 2500 hundredths), the expiry keeps it whole and
    // takes it all at once. Both empty it on the 40th tick.
    for arm in [TroopLifetime::SameAsBuildings, TroopLifetime::ExpiryAtLifetime] {
        let mut cfg = BattleConfig::with_cards(CardDb::from_json_str(&wraith_by_transformation(), CardSource::DerivedJson).unwrap());
        cfg.card_level = [1, 1];
        cfg.calib.troop_lifetime = arm;
        let mut s = bare(cfg);
        let id = s.scenario_spawn_now(Team::Blue, "Wraith", spot(), None).expect("place the Wraith");
        assert_eq!(s.entity(id).unwrap().max_hp, 1000, "precondition: the row's hitpoints at level 1");
        let mut hp = vec![1000];
        for _ in 0..45 {
            s.tick();
            match s.entity(id) {
                Some(e) => hp.push(e.hp),
                None => break,
            }
        }
        assert_eq!(hp.len(), 40, "{arm:?}: the Wraith lived {} ticks, not 39", hp.len() - 1);
        match arm {
            TroopLifetime::SameAsBuildings => assert!(hp.windows(2).all(|w| w[0] - w[1] == 25), "{arm:?}: {hp:?}"),
            TroopLifetime::ExpiryAtLifetime => assert!(hp.iter().all(|h| *h == 1000), "{arm:?}: {hp:?}"),
        }
    }
}
