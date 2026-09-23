//! SPELLS -- Fireball, Arrows, Zap, The Log, Goblin Barrel, and the two effects they
//! share, KNOCKBACK and STUN (src/spell.rs; docs/spell-spec.md).
//!
//! WHAT IT PINS: every spell mechanic the engine implements, driven through the
//! public tick loop on the real data/derived/cards.json. Every number it asserts is
//! READ from that file (2018 vintage) or from data/calibration.json, never typed in,
//! so a data regeneration moves the expectations with the data. Live-2026 values in
//! the spec are calibration evidence, not inputs, and appear nowhere here.
//!
//! HOW A SCENARIO IS BUILT: `cast_scenario` runs TWO battles from one seed -- the
//! spell battle and a CONTROL with everything identical except the cast -- and stops
//! both at the end of the spell's ARRIVAL tick (the tick whose Resolve applied it).
//! Damage is the hp difference to the control, a knockback is the SETTLED position
//! difference (the shipped knockback.DISPLACEMENT_LAW is the 16.402 ladder,
//! which carries the unit over n + 1 ticks after the landing: `Cast::settle_pushes`
//! runs both battles until every ladder is out, and the expected carry is derived
//! from the law by tests/common `knock_carry`, never pasted) -- a knockback is the position
//! difference, a stun is the timer. Victims that must stand still are DEPLOYING when
//! the spell lands (spawned a few ticks before arrival; deploying units are valid
//! spell victims), so a push is measured against an exactly stationary unit.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant
//! cargo test --test spells`). Each was measured over a green baseline (22/22); the
//! tests each one turns red, by test-name prefix:
//!   spell_launch_from_tap              fireball_flight_*, barrel_flight_*
//!   ct_floor                           fireball_damage_*, zap_damage_*, arrows_damage_*, log_pierces_*
//!   spell_damage_unscaled              fireball_damage_*, zap_damage_*, arrows_damage_*, log_pierces_*
//!   crown_pct_ignored                  fireball_damage_*, zap_damage_*, arrows_damage_*, log_pierces_*
//!   pow11_level (card.rs, existing)    fireball_damage_*, zap_damage_*, arrows_damage_*, barrel_releases_*
//!   aoe_centre_to_centre               aoe_radius_edge_*
//!   spells_never_hit_air               area_spells_hit_air_*
//!   spell_friendly_fire                area_spells_hit_air_*, log_hits_*, zap_stun_pauses_* (the
//!                                      Zap then also stuns the Knight's Cannon)
//!   rolling_hits_air                   log_hits_*
//!   no_knockback                       fireball_knockback_*, knockback_resets_a_windup
//!   zero_vector_plus_y                 fireball_knockback_* (the Red-caster half)
//!   ignore_flag_not_read               fireball_knockback_*
//!   push_buildings                     fireball_knockback_*
//!   knockback_keeps_windup             knockback_resets_a_windup
//!   pushback_respects_ignore           log_pushes_*
//!   rolling_push_radial_from_centre    log_never_pushes_* (the Giant at own along-offset
//!                                      -24300 pushed (0, -18000): BACKWARD toward the
//!                                      caster) and log_pushes_* (the off-axis Knight
//!                                      pushed (14515, 10644)). REGRESSION plant for the
//!                                      push that was overruled; it does not expire.
//!   rolling_push_from_tick_end         log_pushes_* -- ONLY COMPOSED with
//!                                      rolling_push_radial_from_centre. Measured dead
//!                                      alone: under travel_direction,
//!                                      the contact point it perturbs no longer feeds the
//!                                      direction, so it changes nothing. Run it as
//!                                      `--cfg clash_plant="rolling_push_radial_from_centre"
//!                                      --cfg clash_plant="rolling_push_from_tick_end"`,
//!                                      where it reds at along-offset 0 (a different
//!                                      offset from the radial plant alone, so the two
//!                                      stay distinguishable and the contact clamp in
//!                                      spell.rs stays certified).
//!   knockback_travel_direction_only    RETIRED. It forced the
//!                                      RadialFromCentre arm to return the travel
//!                                      direction; that IS the shipped behaviour, so
//!                                      it is measured stone dead (spells, mirror and
//!                                      knockback all green under it). Replaced by
//!                                      rolling_push_radial_from_centre, which puts the
//!                                      defect back instead of the fix.
//!   stun_decrement_at_status_start     zap_stun_freezes_*, zap_stun_pauses_*, zap_forces_*
//!   stun_resets_attack                 zap_stun_pauses_*
//!   relock_while_stunned               zap_stun_pauses_*
//!   no_retarget_after_stun             zap_forces_a_retarget_on_resume
//!   stun_replace                       zap_refresh_never_shortens_a_longer_stun
//!   airborne_skipped                   log_timing_*, log_pushes_*
//!   roll_range_from_airborne_start     log_timing_*, log_pierces_*
//!   rolling_centre_in_rect             log_width_*, log_timing_*
//!   rolling_rehit_every_tick           log_pierces_*
//!   waves_simultaneous                 arrows_waves_*
//!   barrel_instant_spawn               barrel_flight_*
//!   spawn_uses_character_deploy_time   barrel_releases_*
//!   spawn_count_one                    barrel_releases_*, barrel_flight_*, barrel_near_the_river_*
//!   spawn_level_local_one              barrel_releases_*
//!   formation_ignores_water            barrel_near_the_river_* (its first form, in formation_grid
//!                                      only, DID NOT LAND: released units pass through
//!                                      formation_points' own ejection; re-pointed there too)
//!   log_territory_anywhere             spell_placement_*
//!   barrel_anywhere_incl_water         spell_placement_*
//!   spells_rejected (card.rs)          every test here (no spell loads)
//! NO PLANT: "Arrows do not push" and "the deco projectile carries no damage" are data
//! facts with no engine code path to break (the loader reads the damage carrier and the
//! Pushback column; check_data.py's deco_damage plant gates the data side).
//!
//! WHAT IT CANNOT CATCH: whether any of this is the REAL game's behaviour. Launch
//! point, projectile speed units, the AOE edge rule, crown rounding, the knockback law
//! and duration, the Log's launch model and hit shape, the barrel formation: every
//! one is a LOW-confidence calibration key (docs/spell-spec.md "Unsettled") that
//! only a recording of the real game can promote. These tests pin what the engine
//! does under the shipped registry, and that the registry is READ.
mod common;

use royalesim::entity::{AttackPhase, EntityKind};
use royalesim::fixed::{milli, Vec2, SUBTILE, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, DeployError, KnockLaw};
use royalesim::{EntityId, Team};
use common::*;
use serde_json::Value;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// data, read from the files (never typed)

fn cards_doc() -> &'static Value {
    static DOC: OnceLock<Value> = OnceLock::new();
    DOC.get_or_init(|| {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("data/derived/cards.json (run tools/extract_cards.py)");
        serde_json::from_str(&text).expect("cards.json parses")
    })
}

fn raw_card(name: &str) -> &'static Value {
    cards_doc()["cards"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap_or_else(|| panic!("{name} not in cards.json"))
}

fn int(v: &Value) -> i32 {
    v.as_i64().unwrap_or_else(|| panic!("not an integer: {v}")) as i32
}

/// Level-scaled stat, computed from cards.json's OWN multiplier table for `card`
/// (independent of card.rs `CardDb::scaled`, which is what is under test): the
/// block's table entered at `base_level` (the 15.535 file: the unified level the
/// stats hold at, calibration combat.STAT_BASE_LEVEL), else at the card rarity's
/// local level 1 (the 2018 file).
fn scaled_from_json(s: &BattleState, card: &str, unified_level: i32, base: i32) -> i32 {
    let c = raw_card(card);
    let rarity = c["rarity"].as_str().unwrap();
    let ls = &c["level_scaling"];
    let base_level = ls["base_level"].as_i64().map_or_else(|| s.cards().rarity(rarity).unwrap().relative_level + 1, |b| b as i32);
    let table = ls["multiplier_percent_by_level"].as_array().unwrap();
    let m = int(&table[(unified_level - base_level) as usize]);
    ((base as i64) * (m as i64) / 100) as i32
}

/// ceil(amount * pct / 100): calibration combat.CROWN_TOWER_DAMAGE_ROUNDING =
/// ceil_kept_share, written out here rather than calling combat.rs.
fn crown_ceil(amount: i32, pct: i32) -> i32 {
    (((amount as i64) * (pct as i64) + 99) / 100) as i32
}

fn fireball_hit() -> &'static Value {
    &raw_card("Fireball")["projectile"]
}
/// The Arrows damage carrier: the CustomFirstProjectile when the row has one (2018:
/// the deco `projectile` deals nothing), else the Projectile itself (15.535).
fn arrows_hit() -> &'static Value {
    let c = raw_card("Arrows");
    if c["spell"]["first_projectile"].is_object() { &c["spell"]["first_projectile"] } else { &c["projectile"] }
}
/// ProjectileWaves of the row (absent = 1: the 2018 column does not exist).
fn arrows_waves() -> i32 {
    raw_card("Arrows")["spell"]["projectile_waves"].as_i64().map_or(1, |w| w as i32)
}
/// The disc one wave hits: the SPELL's Radius for a volley (MultipleProjectiles > 1:
/// the arrows spread over it; card.rs `convert_spell`), else the carrier's.
fn arrows_disc_milli() -> i32 {
    let c = raw_card("Arrows");
    if c["spell"]["multiple_projectiles"].as_i64().unwrap_or(1) > 1 { int(&c["spell"]["radius_milli"]) } else { int(&arrows_hit()["radius_milli"]) }
}
fn zap_aeo() -> &'static Value {
    &raw_card("Zap")["spell"]["area_effect_object"]
}
fn log_roll() -> &'static Value {
    &raw_card("Log")["projectile"]["spawn_projectile"]
}
fn log_air() -> &'static Value {
    &raw_card("Log")["projectile"]
}
fn barrel_proj() -> &'static Value {
    &raw_card("GoblinBarrel")["projectile"]
}

fn calib() -> Calib {
    Calib::shipped()
}

/// Shipped registry values these tests depend on, asserted so a registry change
/// reads as "re-point the test", never as a mysterious failure.
fn assert_registry(key: &str, got: String, want: &str) {
    assert_eq!(got, want, "calibration {key} changed: re-point tests/spells.rs at the new value");
}

// ---------------------------------------------------------------------------
// scenario machinery

fn bare(cfg: BattleConfig) -> BattleState {
    BattleState::new(1, cfg)
}

/// Tick calls, from `s` as it is, until every spell object is gone (at least one).
fn arrival_calls(s: &BattleState) -> u32 {
    let mut p = s.clone();
    for k in 1..=600 {
        p.tick();
        if p.spells().is_empty() {
            return k;
        }
    }
    panic!("spell never arrived within 600 ticks");
}

/// Tick calls until the FIRST spell object lands (a multi-wave spell keeps flying
/// after it): where the deploying victims of `cast_scenario` are timed from.
fn first_impact_calls(s: &BattleState, arrival: u32) -> u32 {
    let mut p = s.clone();
    let mut most = p.spells().len();
    for k in 1..arrival {
        p.tick();
        let n = p.spells().len();
        if most > 0 && n < most {
            return k;
        }
        most = most.max(n);
    }
    // One object, or one applied and gone within a tick (a one-shot area effect):
    // the first impact is the arrival.
    arrival
}

struct Cast {
    /// The battle with the spell, stopped at the end of its arrival tick.
    s: BattleState,
    /// Identical battle without the cast, stopped at the same tick.
    control: BattleState,
    /// Tick calls after the cast until arrival (the arrival tick index is arrival - 1).
    arrival: u32,
    /// Ids of `placed` then `deploying` victims, in argument order.
    ids: Vec<EntityId>,
}

/// See the module doc. `placed` are on the board (already deployed) before the
/// cast; `deploying` are spawned with `spawn_unit` 6 ticks before the FIRST impact
/// (or with the cast, if it arrives sooner), so they are stationary when it lands
/// -- and still deploying when a later wave does (the 15.535 Arrows: 8 ticks).
#[allow(clippy::type_complexity)]
fn cast_scenario(cfg: BattleConfig, caster: Team, spell: &str, tap: Vec2, level: Option<i32>, placed: &[(Team, &str, Vec2)], deploying: &[(Team, &str, Vec2)]) -> Cast {
    let mut s = bare(cfg);
    let mut ids = Vec::new();
    for &(team, card, p) in placed {
        ids.push(s.scenario_spawn_now(team, card, p, None).unwrap_or_else(|e| panic!("{card} at {p:?}: {e:?}")));
    }
    let mut control = s.clone();
    s.spawn_unit(caster, spell, tap, level).unwrap_or_else(|e| panic!("cast {spell}: {e:?}"));
    let arrival = arrival_calls(&s);
    let spawn_at = first_impact_calls(&s, arrival).saturating_sub(6);
    for k in 0..arrival {
        if k == spawn_at && !deploying.is_empty() {
            for st in [&mut s, &mut control] {
                for &(team, card, p) in deploying {
                    st.spawn_unit(team, card, p, None).unwrap_or_else(|e| panic!("{card} at {p:?}: {e:?}"));
                }
            }
        }
        s.tick();
        control.tick();
    }
    if !deploying.is_empty() {
        // Newly spawned entities, identical slots in both battles.
        let before: Vec<EntityId> = ids.clone();
        let mut fresh: Vec<(EntityId, Vec2, String)> = control
            .entities()
            .filter(|e| e.kind == EntityKind::Troop || e.kind == EntityKind::Building)
            .filter(|e| !before.contains(&e.id) && e.deploying)
            .map(|e| (e.id, e.pos, e.card.to_string()))
            .collect();
        fresh.sort_by_key(|f| (f.0.index, f.0.generation));
        for &(team, card, p) in deploying {
            let n = card_stat(&control, card).count.max(1) as usize;
            let mine: Vec<EntityId> = fresh.iter().filter(|f| f.2 == card && control.entity(f.0).unwrap().team == team && (n > 1 || f.1 == p)).map(|f| f.0).collect();
            assert!(!mine.is_empty(), "deploying victim {card} at {p:?} not found");
            ids.extend(mine.iter().take(n));
            fresh.retain(|f| !mine.iter().take(n).any(|m| *m == f.0));
        }
    }
    assert!(s.spells().is_empty(), "the spell had not arrived");
    Cast { s, control, arrival, ids }
}

impl Cast {
    fn hp_loss(&self, id: EntityId) -> i32 {
        let c = self.control.entity(id).unwrap_or_else(|| panic!("{id:?} not alive in control"));
        match self.s.entity(id) {
            Some(v) => c.hp - v.hp,
            None => c.hp, // died
        }
    }
    fn displacement(&self, id: EntityId) -> Vec2 {
        let c = self.control.entity(id).unwrap().pos;
        self.s.entity(id).unwrap_or_else(|| panic!("{id:?} died")).pos.sub(c)
    }
    fn deploying(&self, id: EntityId) -> bool {
        self.control.entity(id).unwrap().deploying
    }
    /// Whether `id` was pushed on the arrival tick: displaced (the fixed_distance
    /// slide) or its ladder armed (the shipped law; the first step comes next tick).
    fn pushed(&self, id: EntityId) -> bool {
        self.s.entity(id).unwrap().push_active || self.displacement(id) != Vec2::default()
    }
    /// Advance both battles until no knockback ladder runs any more (at most `max`
    /// ticks), so `displacement` reads the whole carry of a push. Returns the ticks
    /// run. Every deploying victim must still be deploying in the control afterwards
    /// for the displacement to stay attributable; the callers assert that.
    fn settle_pushes(&mut self, max: u32) -> u32 {
        let mut k = 0;
        while k < max && self.s.entities().any(|e| e.push_active) {
            self.s.tick();
            self.control.tick();
            k += 1;
        }
        k
    }
}

/// The spell stage used by most scenarios: Red half, (9, 19), out of every tower's
/// range (princess towers are 8.5 tiles away) and clear of the river.
fn stage() -> Vec2 {
    t(900, 1900)
}

fn king(s: &BattleState, team: Team) -> Vec2 {
    s.arena().king_tower_pos(team)
}

// ---------------------------------------------------------------------------
// FIREBALL (and the projectile-spell pipeline it shares with Arrows and the Barrel)

#[test]
fn fireball_flight_launches_from_the_caster_king_and_lands_on_distance_over_speed() {
    // A Red Cannon at the tap; its hp drops on the arrival tick and not before. The
    // tap is straight ahead of Blue's king, so the flight is an exact axis distance.
    // Plant: spell_launch_from_tap (lands on the cast tick).
    for mult in [calib().projectile_speed_to_subtiles_per_tick, calib().projectile_speed_to_subtiles_per_tick + 3] {
        let mut cfg = config();
        cfg.calib.projectile_speed_to_subtiles_per_tick = mult;
        let mut s = bare(cfg);
        let tap = Vec2::new(king(&s, Team::Blue).x, stage().y);
        let cannon = s.scenario_spawn_now(Team::Red, "Cannon", tap, None).unwrap();
        let full = s.entity(cannon).unwrap().hp;
        s.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
        let step = int(&fireball_hit()["speed"]) * mult;
        let dist = tap.y - king(&s, Team::Blue).y;
        let expect = ((dist + step - 1) / step) as u32; // tick calls to arrive
        // One tick in: the fireball is one step out of the KING, not at the tap.
        s.tick();
        assert!(!s.spells().is_empty(), "k={mult}: the Fireball landed on the cast tick -- no flight from the king");
        match &s.spells()[0].motion {
            royalesim::spell::SpellMotion::Flight { pos, .. } => {
                assert_eq!(*pos, Vec2::new(tap.x, king(&s, Team::Blue).y + step), "launch point / first step (k={mult})")
            }
            other => panic!("Fireball is not a flight: {other:?}"),
        }
        let mut hit_at = None;
        // a Cannon bleeds its own LifeTime away every tick (lifetime.HP_DECAY), so
        // only a drop bigger than one tick of that is the spell
        let step = drain_step(&s, cannon);
        for k in 1..=expect + 2 {
            if s.entity(cannon).unwrap().hp < full - step * k as i32 {
                hit_at = Some(k);
                break;
            }
            s.tick();
        }
        assert_eq!(hit_at, Some(expect), "k={mult}: distance {dist} / step {step}");
    }
}

#[test]
fn fireball_damage_to_troop_building_and_crown_tower_scales_with_level_and_rounds_by_the_registry() {
    // Tap on Red's engine-Left princess tower, with a deploying Red Knight and a Red
    // Cannon inside the radius. Troop and building take the level-scaled damage in
    // full; the crown tower takes ceil(damage * CrownTowerDamagePercent / 100).
    // Plants: ct_floor, spell_damage_unscaled, crown_pct_ignored.
    assert_registry("combat.CROWN_TOWER_DAMAGE_ROUNDING", format!("{:?}", calib().crown_rounding), "CeilKeptShare");
    for level in [9, 10] {
        let s0 = bare(config());
        let tower_pos = s0.arena().princess_tower_pos(Team::Red, royalesim::arena::Lane::Left);
        let tower = s0.tower_ids(Team::Red)[1].unwrap();
        let knight_at = Vec2::new(tower_pos.x + milli(2000), tower_pos.y - milli(1000));
        let cannon_at = Vec2::new(tower_pos.x, tower_pos.y - milli(2500));
        let c = cast_scenario(config(), Team::Blue, "Fireball", tower_pos, Some(level), &[(Team::Red, "Cannon", cannon_at)], &[(Team::Red, "Knight", knight_at)]);
        let dmg = scaled_from_json(&c.s, "Fireball", level, int(&fireball_hit()["damage"]));
        let pct = int(&fireball_hit()["crown_tower_damage_percent"]);
        assert!(pct < 100 && (dmg * pct) % 100 != 0, "level {level}: the rounding is not exercised ({dmg} at {pct}%)");
        assert_eq!(c.hp_loss(c.ids[1]), dmg, "level {level}: troop");
        assert_eq!(c.hp_loss(c.ids[0]), dmg, "level {level}: non-crown building takes full damage");
        assert_eq!(c.hp_loss(tower), crown_ceil(dmg, pct), "level {level}: crown tower ({dmg} at {pct}%)");
        // The key is READ: floor gives one less.
        let mut cfg = config();
        cfg.calib.crown_rounding = royalesim::combat::CrownRounding::Floor;
        let f = cast_scenario(cfg, Team::Blue, "Fireball", tower_pos, Some(level), &[], &[]);
        assert_eq!(f.hp_loss(tower), dmg * pct / 100, "level {level}: registry Floor");
    }
}

#[test]
fn aoe_radius_edge_is_inclusive_to_the_subtile_under_the_registry() {
    // Two Red Cannons on the x axis through the impact: one with its hitbox edge
    // exactly on the radius (dist = R + r), one a subtile farther. Fireball, Arrows,
    // Zap. Then the other registry candidate, centre_in_radius, moves the boundary
    // to dist = R. Plant: aoe_centre_to_centre.
    assert_registry("spells.AOE_HIT_TEST", format!("{:?}", calib().aoe_hit_test), "EdgeInclusive");
    let r_cannon = card_stat(&bare(config()), "Cannon").collision_radius;
    for (spell, radius) in [("Fireball", milli(int(&fireball_hit()["radius_milli"]))), ("Arrows", milli(arrows_disc_milli())), ("Zap", milli(int(&zap_aeo()["radius_milli"])))] {
        let tap = stage();
        let inside = Vec2::new(tap.x + radius + r_cannon, tap.y);
        let outside = Vec2::new(tap.x - radius - r_cannon - 1, tap.y);
        let c = cast_scenario(config(), Team::Blue, spell, tap, None, &[(Team::Red, "Cannon", inside), (Team::Red, "Cannon", outside)], &[]);
        assert!(c.hp_loss(c.ids[0]) > 0, "{spell}: a Cannon whose edge touches the radius was missed");
        assert_eq!(c.hp_loss(c.ids[1]), 0, "{spell}: a Cannon one subtile beyond the radius was hit");
        let mut cfg = config();
        cfg.calib.aoe_hit_test = royalesim::state::AoeHitTest::CentreInRadius;
        let centre_in = Vec2::new(tap.x + radius, tap.y);
        let c = cast_scenario(cfg, Team::Blue, spell, tap, None, &[(Team::Red, "Cannon", inside), (Team::Red, "Cannon", centre_in)], &[]);
        assert_eq!(c.hp_loss(c.ids[0]), 0, "{spell}: centre_in_radius still hit by the edge");
        assert!(c.hp_loss(c.ids[1]) > 0, "{spell}: centre_in_radius missed a centre on the radius");
    }
}

#[test]
fn area_spells_hit_air_and_ground_enemies_and_never_friends() {
    // Red Minions (air) and a Red Knight, and a Blue Knight, all deploying inside the
    // radius. Fireball and Zap and Arrows ship AoeToAir/AoeToGround (or HitsAir/Ground)
    // TRUE and OnlyEnemies TRUE. Plants: spells_never_hit_air, spell_friendly_fire.
    for spell in ["Fireball", "Arrows", "Zap"] {
        let tap = stage();
        let c = cast_scenario(
            config(),
            Team::Blue,
            spell,
            tap,
            None,
            &[],
            &[(Team::Red, "Minions", Vec2::new(tap.x, tap.y + SUBTILE)), (Team::Red, "Knight", Vec2::new(tap.x - SUBTILE, tap.y)), (Team::Blue, "Knight", Vec2::new(tap.x + SUBTILE, tap.y))],
        );
        let n_min = card_stat(&c.s, "Minions").count as usize;
        let minions = &c.ids[0..n_min];
        let (red_knight, blue_knight) = (c.ids[n_min], c.ids[n_min + 1]);
        assert!(minions.iter().all(|m| c.deploying(*m) && c.control.entity(*m).unwrap().flying), "{spell}: victims not deploying/flying");
        for m in minions {
            assert!(c.hp_loss(*m) > 0, "{spell}: an air enemy was not hit");
        }
        assert!(c.hp_loss(red_knight) > 0, "{spell}: a ground enemy was not hit");
        assert_eq!(c.hp_loss(blue_knight), 0, "{spell}: friendly fire");
        assert_eq!(c.displacement(blue_knight), Vec2::default(), "{spell}: a friend was pushed");
        assert_eq!(c.s.entity(blue_knight).unwrap().stun_ms, 0, "{spell}: a friend was stunned");
    }
}

#[test]
fn fireball_knockback_is_radial_fixed_distance_troops_only_and_respects_ignore_pushback() {
    // Deploying (stationary) victims, so the displacement against the control is the
    // push alone. Plants: no_knockback, ignore_flag_not_read, push_buildings,
    // zero_vector_plus_y (the Red-caster half).
    assert_registry("knockback.DURATION_MS", calib().knock_duration_ms.to_string(), "0");
    assert_registry("knockback.AFFECTS_DEPLOYING_UNITS", calib().knock_affects_deploying.to_string(), "true");
    let push = milli(int(&fireball_hit()["pushback_milli"]));
    assert!(!fireball_hit()["pushback_all"].as_bool().unwrap());
    let tap = stage();
    let c = cast_scenario(
        config(),
        Team::Blue,
        "Fireball",
        tap,
        None,
        &[(Team::Red, "Cannon", Vec2::new(tap.x, tap.y - milli(1500)))],
        &[(Team::Red, "Knight", Vec2::new(tap.x + SUBTILE, tap.y)), (Team::Red, "Giant", Vec2::new(tap.x - milli(2000), tap.y)), (Team::Red, "Knight", Vec2::new(tap.x - milli(600), tap.y + milli(800)))],
    );
    let (cannon, knight, giant, diag) = (c.ids[0], c.ids[1], c.ids[2], c.ids[3]);
    assert!(c.deploying(knight) && c.deploying(giant) && c.deploying(diag));
    assert!(c.pushed(knight) && c.pushed(diag), "the Knights were not pushed on the landing tick");
    assert!(!c.pushed(giant) && !c.pushed(cannon));
    // The whole carry: the ladder's `25n(n-1)/2 - 25` along the line (tests/common
    // knock_carry, from the law), Pushback itself under the fixed_distance arm.
    let mut c = c;
    let ticks = c.settle_pushes(40);
    assert!(c.deploying(knight) && c.deploying(giant) && c.deploying(diag), "the victims must still be deploying after {ticks} ladder ticks");
    let carry = knock_carry(&calib(), push);
    assert!(carry > 0 && carry <= push, "carry {carry} for Pushback {push}");
    assert_eq!(c.displacement(knight), Vec2::new(carry, 0), "radial along +x by exactly the carry");
    let d = c.displacement(diag);
    // Along (-0.6, 0.8): the carry, to within the per-axis truncation of every ladder
    // step (under two native units a step: the heading is a 1/256 truncation and the
    // per-axis step a truncation of it) -- one subtile under the instant slide.
    let tol = match calib().knock_law {
        KnockLaw::FixedDistance => 1,
        KnockLaw::Client16402 => 2 * ladder_ticks(push / K) * K,
    };
    assert!((d.x + carry * 3 / 5).abs() <= tol && (d.y - carry * 4 / 5).abs() <= tol, "diagonal push {d:?} for carry {carry} (tolerance {tol})");
    assert!(card_stat(&c.s, "Giant").ignore_pushback, "data: Giant ships IgnorePushback (2018)");
    assert_eq!(c.displacement(giant), Vec2::default(), "IgnorePushback without PushbackAll must not move the Giant");
    assert!(c.hp_loss(giant) > 0, "the Giant still takes the damage");
    assert_eq!(c.displacement(cannon), Vec2::default(), "buildings are never pushed");
    // ZERO VECTOR: a victim exactly on the impact. Shipped (registry
    // knockback.ZERO_VECTOR_DIRECTION = client16402_x_by_id_parity):
    // ABSOLUTE +-x by the parity of team_seq -- the same way for both seats' first
    // Knight. The caster_forward arm (the CASTER's forward axis, +y Blue, -y Red)
    // stays runnable and is measured under symmetric_config().
    assert_registry("knockback.ZERO_VECTOR_DIRECTION", format!("{:?}", calib().knock_zero_vector), "Client16402XByIdParity");
    let mut zero_way = Vec::new();
    for caster in [Team::Blue, Team::Red] {
        let victim_team = caster.other();
        let at = if caster == Team::Blue { tap } else { t(900, 1300) };
        let mut z = cast_scenario(config(), caster, "Fireball", at, None, &[], &[(victim_team, "Knight", at)]);
        assert!(z.pushed(z.ids[0]), "{caster:?} caster: the on-centre Knight was not pushed");
        z.settle_pushes(40);
        assert!(z.deploying(z.ids[0]));
        let d = z.displacement(z.ids[0]);
        let seq = z.s.entity(z.ids[0]).unwrap().team_seq;
        let sign = if seq & 1 == 1 { -1 } else { 1 };
        assert_eq!(d, Vec2::new(sign * carry, 0), "{caster:?} caster: zero-vector push is +-x by team_seq parity ({seq})");
        zero_way.push(d);
    }
    assert_eq!(zero_way[0], zero_way[1], "both seats' first Knight (team_seq equal) go the same ABSOLUTE way");
    for (caster, sign) in [(Team::Blue, 1), (Team::Red, -1)] {
        let victim_team = caster.other();
        let at = if caster == Team::Blue { tap } else { t(900, 1300) };
        let mut z = cast_scenario(symmetric_config(), caster, "Fireball", at, None, &[], &[(victim_team, "Knight", at)]);
        z.settle_pushes(40);
        assert_eq!(z.displacement(z.ids[0]), Vec2::new(0, sign * push), "{caster:?} caster: caster_forward zero-vector push under the slide arm");
    }
}

#[test]
fn knockback_resets_a_windup() {
    // A Red Musketeer (already deployed) shooting a Blue Cannon is hit by a Blue
    // Fireball on a tick where the CONTROL Musketeer is still mid-windup: the pushed
    // one is Idle with its timer at zero (registry knockback.ATTACK_RESET: the
    // attack resets, MEASURED on the Knight of capture 20260920-081819-B,
    // load 300 -> 700 on the hit tick 3535).
    // Plant: knockback_keeps_windup.
    assert_registry("knockback.ATTACK_RESET", format!("{:?}", calib().knock_attack_reset), "ResetAttackKeepTarget");
    let tap = stage();
    let musk_at = Vec2::new(tap.x + SUBTILE, tap.y);
    let cannon_at = Vec2::new(musk_at.x - milli(4000), musk_at.y);
    let musk = card_stat(&bare(config()), "Musketeer").clone();
    let (load, hs) = (musk.load_time_ms, musk.hit_speed_ms);
    let tick = calib().tick_ms;
    // "MID-SWING" in the shipped cycle (combat.ATTACK_CYCLE = progress_credit):
    // the progress counter is past the start of a cycle and will not
    // cross the next HitSpeed multiple on the coming tick, so a reset is visible.
    // Under the old windup arm it was "the windup is running and will not finish
    // next tick".
    let mid_swing = |v: &royalesim::state::EntityView<'_>| match calib().attack_cycle {
        royalesim::state::AttackCycle::ProgressCredit => v.attack_phase == AttackPhase::Windup && v.attack_ms > 0 && v.attack_ms % hs + tick < hs,
        royalesim::state::AttackCycle::WindupLoadTime => v.attack_phase == AttackPhase::Windup && v.attack_ms + tick < load,
    };
    let mut found = 0;
    for delay in 0..40u32 {
        let mut s = bare(config());
        let m = s.scenario_spawn_now(Team::Red, "Musketeer", musk_at, Some(1_000_000)).unwrap();
        s.scenario_spawn_now(Team::Blue, "Cannon", cannon_at, None).unwrap();
        for _ in 0..delay {
            s.tick();
        }
        let mut control = s.clone();
        s.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
        let arrival = arrival_calls(&s);
        for _ in 0..arrival {
            s.tick();
            control.tick();
        }
        let cv = control.entity(m).unwrap();
        if !mid_swing(&cv) {
            continue;
        }
        found += 1;
        let v = s.entity(m).unwrap();
        assert_eq!((v.attack_phase, v.attack_ms), (AttackPhase::Idle, 0), "delay {delay}: control is mid-swing at {} ms, the pushed Musketeer is not reset", cv.attack_ms);
        // and its LOAD TIMER is back to a full LoadTime (the Knight's 300 ->
        // 700 on the hit tick; the windup arm does not carry the column)
        if calib().attack_cycle == royalesim::state::AttackCycle::ProgressCredit {
            assert_eq!(v.attack_load_ms, load, "delay {delay}: the push did not reload the timer");
        }
        assert_eq!(v.target, cv.target, "delay {delay}: the target is kept (reset_windup_keep_target)");
        assert!(v.push_active || v.pos != cv.pos, "delay {delay}: it was not pushed");
    }
    assert!(found >= 3, "vacuous: only {found} arrivals landed mid-windup");
}

#[test]
fn knockback_interrupts_a_unit_between_shots_and_the_old_arm_freezes_its_cooldown() {
    // THE MEASURED HALF the community reading missed (the Bomber of capture
    // 20260920-081819-B, between hits with its swing counter at 3500
    // when the Bandit hit it at tick 2020: counter 0 from 2021, state 1 through the
    // ladder, a FRESH LoadTime windup on re-entering range at 2037): a Fireball on a
    // Musketeer BETWEEN SHOTS (Cooldown) under the shipped reset_attack_keep_target
    // leaves it Idle at 0 -- its next shot is a LoadTime windup once the ladder ends
    // and the target is in range again -- where the superseded
    // reset_windup_keep_target keeps the cooldown (frozen through the ladder by
    // Entities::knocked, resumed after it), so in the engine's cooldown-then-windup
    // cycle its next shot comes LATER by the cooldown that was left. Both arms keep
    // the target. Plant knockback_keeps_windup: the shipped arm's Cooldown is not
    // reset either.
    let tap = stage();
    let musk_at = Vec2::new(tap.x + SUBTILE, tap.y);
    let cannon_at = Vec2::new(musk_at.x - milli(4000), musk_at.y);
    let mut found = 0;
    let mut sooner = 0;
    for delay in 0..60u32 {
        let arms = [
            ("shipped", config()),
            ("old", {
                let mut c = config();
                c.calib.knock_attack_reset = royalesim::state::KnockAttackReset::ResetWindupKeepTarget;
                c
            }),
        ];
        let mut next_shot = Vec::new();
        for (name, cfg) in arms {
            let mut s = bare(cfg);
            let m = s.scenario_spawn_now(Team::Red, "Musketeer", musk_at, Some(1_000_000)).unwrap();
            // a Cannon that outlives the sweep (its hp is the shot clock below)
            let cannon = s.scenario_spawn_now(Team::Blue, "Cannon", cannon_at, Some(1_000_000)).unwrap();
            for _ in 0..delay {
                s.tick();
            }
            let mut control = s.clone();
            s.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
            let arrival = arrival_calls(&s);
            for _ in 0..arrival {
                s.tick();
                control.tick();
            }
            let cv = control.entity(m).unwrap();
            if cv.attack_phase != AttackPhase::Cooldown || cv.attack_ms == 0 {
                break; // not a between-shots landing on this delay
            }
            let v = s.entity(m).unwrap();
            assert!(v.push_active, "{name} delay {delay}: the Fireball did not push the Musketeer");
            assert_eq!(v.target, cv.target, "{name} delay {delay}: the target is kept");
            match name {
                "shipped" => assert_eq!((v.attack_phase, v.attack_ms), (AttackPhase::Idle, 0), "delay {delay}: the shipped arm interrupts the cooldown (the Bomber between hits)"),
                _ => assert_eq!((v.attack_phase, v.attack_ms), (cv.attack_phase, cv.attack_ms), "delay {delay}: the old arm leaves the cooldown as it was"),
            }
            // the next shot: ticks from the landing to the Musketeer's next FIRE (the
            // Windup -> Cooldown transition; the Cannon's hp would also show the shot
            // already in flight when the Fireball landed)
            let _ = cannon;
            let mut k = 0;
            let mut prev = s.entity(m).unwrap().attack_phase;
            loop {
                s.tick();
                k += 1;
                let now = s.entity(m).unwrap().attack_phase;
                if prev == AttackPhase::Windup && now == AttackPhase::Cooldown {
                    break;
                }
                prev = now;
                assert!(k < 400, "{name} delay {delay}: no shot after the push");
            }
            next_shot.push(k);
        }
        if next_shot.len() == 2 {
            found += 1;
            if next_shot[0] < next_shot[1] {
                sooner += 1;
            }
            assert!(next_shot[0] <= next_shot[1], "delay {delay}: the shipped arm's next shot ({}) is later than the old arm's ({}), which still owes its cooldown", next_shot[0], next_shot[1]);
        }
    }
    assert!(found >= 3, "vacuous: only {found} arrivals landed between shots");
    assert!(sooner >= 1, "vacuous: the two arms never parted on the next shot ({found} landings)");
}

// ---------------------------------------------------------------------------
// ARROWS

#[test]
fn arrows_damage_scales_hits_once_per_wave_and_does_not_push() {
    // One damage carrier (2018: the CustomFirstProjectile, the deco projectile deals
    // nothing; 15.535: the Projectile itself), each wave one hit per victim over the
    // spell's disc, no pushback. The scenario ends when the last wave has landed, so a
    // victim has taken exactly `waves` hits. Plants: ct_floor, crown_pct_ignored.
    let s0 = bare(config());
    let tower_pos = s0.arena().princess_tower_pos(Team::Red, royalesim::arena::Lane::Right);
    let tower = s0.tower_ids(Team::Red)[2].unwrap();
    let knight_at = Vec2::new(tower_pos.x - milli(3000), tower_pos.y - milli(2000));
    let c = cast_scenario(config(), Team::Blue, "Arrows", tower_pos, None, &[], &[(Team::Red, "Knight", knight_at)]);
    let level = c.s.config().card_level[0];
    let dmg = scaled_from_json(&c.s, "Arrows", level, int(&arrows_hit()["damage"]));
    let pct = int(&arrows_hit()["crown_tower_damage_percent"]);
    let waves = arrows_waves();
    assert!((dmg * pct) % 100 != 0, "rounding not exercised");
    assert_eq!(c.hp_loss(c.ids[0]), dmg * waves, "{waves} wave(s) of {dmg}");
    assert_eq!(c.hp_loss(tower), crown_ceil(dmg, pct) * waves, "{waves} wave(s) of ceil({dmg} x {pct} %)");
    assert_eq!(c.displacement(c.ids[0]), Vec2::default(), "Arrows ship no Pushback");
    if raw_card("Arrows")["spell"]["first_projectile"].is_object() {
        assert!(raw_card("Arrows")["projectile"]["damage"].is_null(), "data: the deco projectile carries no damage");
    } else {
        assert!(raw_card("Arrows")["spell"]["multiple_projectiles"].as_i64().unwrap_or(1) > 1, "data: the one carrier is a volley");
    }
}

#[test]
fn arrows_waves_come_from_the_data() {
    // ProjectileWaves / ProjectileWaveInterval are the row's: the 15.535 Arrows fires
    // 3 waves 200 ms apart, the 2018 row had no such column (one wave). Both shapes go
    // through the SAME engine: the file's own row, and a copy with the columns cleared
    // (a MECHANIC probe, not a stat the battles run on) -- a stationary Red Cannon
    // loses exactly one wave's damage on each of T, T + gap, T + 2 gap, and once with
    // the columns cleared. Plant: waves_simultaneous.
    let tick = calib().tick_ms;
    let run = |doc: &Value| -> Vec<(u32, i32)> {
        let db = royalesim::card::CardDb::from_json_str(&doc.to_string(), royalesim::card::CardSource::DerivedJson).unwrap();
        let mut s = bare(BattleConfig::with_cards(db));
        let tap = stage();
        let cannon = s.scenario_spawn_now(Team::Red, "Cannon", tap, None).unwrap();
        // The Cannon bleeds its own LifeTime away every tick (lifetime.HP_DECAY), so
        // the DAMAGE is read against a control battle with no Arrows in it: the two
        // drain in step, and every difference between them is the spell.
        let mut ctl = s.clone();
        s.spawn_unit(Team::Blue, "Arrows", tap, None).unwrap();
        let mut last = 0;
        let mut drops = Vec::new();
        for k in 1..=200 {
            s.tick();
            ctl.tick();
            let taken = ctl.entity(cannon).map_or(0, |v| v.hp) - s.entity(cannon).map_or(0, |v| v.hp);
            if taken != last {
                drops.push((k, taken - last));
                last = taken;
            }
        }
        drops
    };
    let doc = cards_doc().clone();
    let (waves, interval) = (arrows_waves(), raw_card("Arrows")["spell"]["projectile_wave_interval_ms"].as_i64().unwrap_or(0) as i32);
    assert!(waves >= 2 && interval > 0 && interval % tick == 0, "data: the Arrows row's waves {waves} x {interval} ms do not exercise the mechanic");
    let many = run(&doc);
    let mut one_wave = doc.clone();
    let arrows = one_wave["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "Arrows").unwrap();
    arrows["spell"]["projectile_waves"] = Value::Null;
    arrows["spell"]["projectile_wave_interval_ms"] = Value::Null;
    let one = run(&one_wave);
    assert_eq!(one.len(), 1, "with the columns cleared Arrows must hit once: {one:?}");
    let gap = (interval / tick) as u32;
    assert_eq!(many.len(), waves as usize, "{waves} waves: {many:?}");
    assert_eq!(many[0].0, one[0].0, "wave 0 lands when the single wave did");
    assert!(many.windows(2).all(|w| w[1].0 - w[0].0 == gap), "waves {interval} ms apart: {many:?}");
    assert!(many.iter().all(|w| w.1 == one[0].1), "each wave deals one wave's damage: {many:?} vs {one:?}");
}

// ---------------------------------------------------------------------------
// ZAP

#[test]
fn zap_damage_scales_and_crown_towers_take_the_reduced_share() {
    // Plants: ct_floor, spell_damage_unscaled, crown_pct_ignored.
    let s0 = bare(config());
    let tower_pos = s0.arena().princess_tower_pos(Team::Red, royalesim::arena::Lane::Left);
    let tower = s0.tower_ids(Team::Red)[1].unwrap();
    for level in [9, 11] {
        let c = cast_scenario(config(), Team::Blue, "Zap", tower_pos, Some(level), &[], &[(Team::Red, "Knight", Vec2::new(tower_pos.x + milli(1600), tower_pos.y))]);
        assert_eq!(c.arrival, 1, "Zap applies on the tick it is cast (no flight)");
        let dmg = scaled_from_json(&c.s, "Zap", level, int(&zap_aeo()["damage"]));
        let pct = int(&zap_aeo()["crown_tower_damage_percent"]);
        assert_eq!(c.hp_loss(c.ids[0]), dmg, "level {level}");
        assert_eq!(c.hp_loss(tower), crown_ceil(dmg, pct), "level {level}: {dmg} at {pct}%");
        assert_eq!(c.s.entity(c.ids[0]).unwrap().stun_ms, int(&zap_aeo()["buff_time_ms"]), "stun set to BuffTime");
        assert_eq!(c.s.entity(tower).unwrap().stun_ms, int(&zap_aeo()["buff_time_ms"]), "crown towers are stunned too");
    }
    assert!((scaled_from_json(&s0, "Zap", 9, int(&zap_aeo()["damage"])) * int(&zap_aeo()["crown_tower_damage_percent"])) % 100 != 0);
}

/// A Red Knight in melee with a Blue Cannon; returns the tick indices on which the
/// Cannon's hp dropped (a Knight hit), running `ticks`, with a Blue Zap applied on
/// tick `zap` if given.
/// (tick, attack phase, attack ms, target locked, retarget on resume, stun ms) of the
/// Knight at the end of each tick.
type KnightTrace = (u32, AttackPhase, i32, bool, bool, i32);

fn knight_vs_cannon(zap: Option<u32>, ticks: u32) -> (Vec<u32>, Vec<KnightTrace>) {
    let mut s = bare(config());
    let k_at = stage();
    let kr = card_stat(&s, "Knight").collision_radius;
    let cr = card_stat(&s, "Cannon").collision_radius;
    let knight = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).unwrap();
    let cannon = s.scenario_spawn_now(Team::Blue, "Cannon", Vec2::new(k_at.x, k_at.y - kr - cr - milli(200)), None).unwrap();
    let mut last = s.entity(cannon).unwrap().hp;
    // the Cannon's own LifeTime drain (lifetime.HP_DECAY) is not a Knight hit
    let step = drain_step(&s, cannon);
    let mut hits = Vec::new();
    let mut trace = Vec::new();
    for k in 0..ticks {
        if zap == Some(k) {
            s.spawn_unit(Team::Blue, "Zap", k_at, None).unwrap();
        }
        s.tick();
        let v = s.entity(knight).unwrap();
        trace.push((k, v.attack_phase, v.attack_ms, v.target_locked, v.retarget_on_resume, v.stun_ms));
        let hp = s.entity(cannon).map_or(0, |c| c.hp);
        if last - hp > step {
            hits.push(k);
        }
        if hp != last {
            last = hp;
        }
    }
    (hits, trace)
}

#[test]
fn zap_stun_pauses_windup_and_cooldown_for_exactly_ceil_duration_ticks() {
    // For every application tick N in a window covering whole attack cycles: the
    // Knight's next hit after N lands exactly ceil(BuffTime / TICK_MS) ticks after the
    // control's (PAUSE, registry status.STUN_ATTACK_TIMER_MODEL), whether N fell in
    // the windup or the cooldown; a hit landing ON tick N is unaffected (Attack runs
    // before Resolve). On application: lock released, retarget flagged, phase and
    // timer untouched. Plants: stun_resets_attack, relock_while_stunned.
    assert_registry("status.STUN_ATTACK_TIMER_MODEL", format!("{:?}", calib().stun_attack_timer), "Pause");
    assert_registry("status.BUFF_EXPIRY_TICK_ALIGNMENT", format!("{:?}", calib().buff_expiry), "CeilFromNextTick");
    let buff = int(&zap_aeo()["buff_time_ms"]);
    let held = ((buff + calib().tick_ms - 1) / calib().tick_ms) as u32;
    let knight = card_stat(&bare(config()), "Knight").clone();
    let (hs, load) = (knight.hit_speed_ms, knight.load_time_ms);
    let (ctl, ctl_trace) = knight_vs_cannon(None, 120);
    assert!(ctl.len() >= 4, "vacuous: the Knight hit the Cannon only {} times", ctl.len());
    let (mut in_windup, mut in_cooldown) = (0, 0);
    for n in 30..30 + 2 * 22 {
        let (z, tr) = knight_vs_cannon(Some(n), 120);
        let after = |h: &Vec<u32>| h.iter().copied().find(|k| *k > n);
        let (Some(c_next), Some(z_next)) = (after(&ctl), after(&z)) else { continue };
        assert_eq!(z_next, c_next + held, "zap on tick {n}: next hit {z_next}, control {c_next}, stun holds {held} ticks");
        assert_eq!(ctl.iter().filter(|k| **k <= n).count(), z.iter().filter(|k| **k <= n).count(), "zap on tick {n} changed a hit at or before N");
        // State at the end of the application tick vs the control.
        let (c, a) = (ctl_trace[n as usize], tr[n as usize]);
        assert_eq!((a.1, a.2), (c.1, c.2), "tick {n}: the stun changed the attack phase/timer (pause, not reset)");
        assert!(!a.3 && a.4 && a.5 == buff, "tick {n}: on application lock {} retarget {} stun {}", a.3, a.4, a.5);
        for k in n + 1..n + held {
            assert!(!tr[k as usize].3, "tick {k}: a stunned unit was re-locked");
        }
        // EARLY / LATE IN THE CYCLE, the arm-independent reading of the old
        // "windup or cooldown" split: under the shipped cycle
        // (combat.ATTACK_CYCLE = progress_credit) there is no
        // cooldown STATE -- one progress counter runs from the credit to the hit --
        // so the guard asks that the application ticks cover both halves of it.
        if c.1 != AttackPhase::Idle {
            let into = match calib().attack_cycle {
                royalesim::state::AttackCycle::ProgressCredit => c.2 % hs,
                royalesim::state::AttackCycle::WindupLoadTime => {
                    if c.1 == AttackPhase::Windup {
                        c.2
                    } else {
                        load + c.2
                    }
                }
            };
            if 2 * into < hs {
                in_windup += 1;
            } else {
                in_cooldown += 1;
            }
        }
    }
    assert!(in_windup >= 3 && in_cooldown >= 3, "vacuous: {in_windup} early and {in_cooldown} late application ticks");
}

#[test]
fn zap_stun_freezes_movement_for_exactly_ceil_duration_ticks() {
    // A walking Red Knight (nothing in sight): frozen on ticks N+1 ..= N+held, moving
    // again on N+held+1. Plant: stun_decrement_at_status_start (one tick short).
    let buff = int(&zap_aeo()["buff_time_ms"]);
    let held = ((buff + calib().tick_ms - 1) / calib().tick_ms) as u32;
    let mut s = bare(config());
    let knight = s.scenario_spawn_now(Team::Red, "Knight", t(900, 2100), None).unwrap();
    let n = 10u32;
    let mut pos = Vec::new();
    for k in 0..n + held + 3 {
        if k == n {
            s.spawn_unit(Team::Blue, "Zap", s.entity(knight).unwrap().pos, None).unwrap();
        }
        s.tick();
        pos.push(s.entity(knight).unwrap().pos);
    }
    let n = n as usize;
    let held = held as usize;
    assert_ne!(pos[n], pos[n - 1], "the Knight must be walking before the stun (tick N itself still moves)");
    for k in n + 1..=n + held {
        assert_eq!(pos[k], pos[n], "tick {k}: a stunned unit moved");
    }
    assert_ne!(pos[n + held + 1], pos[n], "tick {}: still frozen after the stun expired", n + held + 1);
}

#[test]
fn zap_forces_a_retarget_on_resume() {
    // A Red Musketeer shooting Blue Cannon A; Blue Cannon B is placed much nearer
    // after the lock. Control: it keeps A (target lock / keep-target hysteresis). Zap:
    // on the first unstunned tick it rescans and takes B -- not before.
    // Plant: no_retarget_after_stun.
    assert_registry("status.STUN_RETARGET_ON_RESUME", calib().stun_retarget_on_resume.to_string(), "true");
    let held = ((int(&zap_aeo()["buff_time_ms"]) + calib().tick_ms - 1) / calib().tick_ms) as u32;
    let build = |zap: Option<u32>| {
        let mut s = bare(config());
        let m_at = stage();
        let m = s.scenario_spawn_now(Team::Red, "Musketeer", m_at, Some(1_000_000)).unwrap();
        // Cannon A outlives the 80 ticks whatever the vintage's numbers (the 15.535
        // Musketeer at level 11 would fell a level-11 Cannon inside them).
        let a = s.scenario_spawn_now(Team::Blue, "Cannon", Vec2::new(m_at.x, m_at.y - milli(5500)), Some(1_000_000)).unwrap();
        let mut b = None;
        let mut targets = Vec::new();
        for k in 0..80u32 {
            if k == 20 {
                b = Some(s.scenario_spawn_now(Team::Blue, "Cannon", Vec2::new(m_at.x + milli(2200), m_at.y), None).unwrap());
            }
            if zap == Some(k) {
                s.spawn_unit(Team::Blue, "Zap", m_at, None).unwrap();
            }
            s.tick();
            targets.push(s.entity(m).unwrap().target);
        }
        (targets, a, b.unwrap())
    };
    let (ctl, a, _b) = build(None);
    assert!(ctl[19] == Some(a) && ctl[20..].iter().all(|t| *t == Some(a)), "control must keep Cannon A after B appears: {:?}", &ctl[18..30]);
    let n = 30u32;
    let (z, a2, b2) = build(Some(n));
    assert_eq!(a, a2);
    for (k, target) in z.iter().enumerate().take((n + held) as usize + 1) {
        assert_eq!(*target, Some(a), "tick {k}: retargeted before the stun ended");
    }
    assert_eq!(z[(n + held + 1) as usize], Some(b2), "tick {}: no rescan on resume", n + held + 1);
}

#[test]
fn zap_refresh_never_shortens_a_longer_stun() {
    // A Zap landing on a unit that still has a LONGER stun left (from a copy of the Zap
    // row whose buff lasts 4x as long -- a mechanic probe, not a battle stat) cannot
    // cut it short. Registry status.SAME_BUFF_REAPPLY = refresh_max. Plant: stun_replace.
    assert_registry("status.SAME_BUFF_REAPPLY", format!("{:?}", calib().same_buff_reapply), "RefreshMax");
    let buff = int(&zap_aeo()["buff_time_ms"]);
    let tick = calib().tick_ms;
    // A Zap whose buff lasts 4x as long, from a copy of the data (a mechanic probe).
    let mut doc = cards_doc().clone();
    {
        let cards = doc["cards"].as_array_mut().unwrap();
        let mut long = cards.iter().find(|c| c["name"] == "Zap").unwrap().clone();
        long["name"] = Value::from("LongZap");
        long["display_name"] = Value::from("LongZap");
        long["spell"]["area_effect_object"]["buff_time_ms"] = Value::from(buff * 4);
        cards.push(long);
    }
    let db = royalesim::card::CardDb::from_json_str(&doc.to_string(), royalesim::card::CardSource::DerivedJson).unwrap();
    let mut s = bare(BattleConfig::with_cards(db));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", t(900, 2100), None).unwrap();
    s.spawn_unit(Team::Blue, "LongZap", s.entity(knight).unwrap().pos, None).unwrap();
    s.tick();
    assert_eq!(s.entity(knight).unwrap().stun_ms, buff * 4);
    s.tick();
    s.tick();
    let before = s.entity(knight).unwrap().stun_ms;
    assert_eq!(before, buff * 4 - 2 * tick);
    s.spawn_unit(Team::Blue, "Zap", s.entity(knight).unwrap().pos, None).unwrap();
    s.tick();
    assert_eq!(s.entity(knight).unwrap().stun_ms, before - tick, "a short stun cut a longer one");
}

// ---------------------------------------------------------------------------
// THE LOG

/// A Blue Log cast at `tap` (x = 9, Blue side) rolling +y through Red Cannons.
fn log_tap() -> Vec2 {
    t(900, 1300)
}

fn log_numbers(s: &BattleState) -> (i32, i32, i32, i32, i32, i32) {
    let k = s.config().calib.projectile_speed_to_subtiles_per_tick;
    (
        int(&log_air()["speed"]) * k,
        milli(int(&log_air()["min_distance_milli"])),
        int(&log_roll()["speed"]) * k,
        milli(int(&log_roll()["projectile_range_milli"])),
        milli(int(&log_roll()["projectile_radius_milli"])),
        milli(int(&log_roll()["projectile_radius_y_milli"])),
    )
}

/// Run a Blue Log over Red Cannons at `cannons`; per Cannon, the list of tick CALLS
/// (1-based) on which it lost hp.
fn log_over_cannons(cfg: BattleConfig, cannons: &[Vec2], level: Option<i32>) -> (Vec<Vec<u32>>, Vec<i32>) {
    let mut s = bare(cfg);
    let ids: Vec<EntityId> = cannons.iter().map(|p| s.scenario_spawn_now(Team::Red, "Cannon", *p, None).unwrap()).collect();
    // The Cannons bleed their own LifeTime away every tick (lifetime.HP_DECAY), so
    // the DAMAGE is read against a control battle with no Log in it.
    let mut ctl = s.clone();
    let mut last = vec![0; ids.len()];
    let mut hits = vec![Vec::new(); ids.len()];
    let mut loss = vec![0; ids.len()];
    s.spawn_unit(Team::Blue, "Log", log_tap(), level).unwrap();
    for k in 1..=120u32 {
        s.tick();
        ctl.tick();
        for (j, id) in ids.iter().enumerate() {
            let taken = ctl.entity(*id).map_or(0, |v| v.hp) - s.entity(*id).map_or(0, |v| v.hp);
            if taken != last[j] {
                hits[j].push(k);
                loss[j] += taken - last[j];
                last[j] = taken;
            }
        }
    }
    (hits, loss)
}

#[test]
fn log_timing_airborne_then_roll_from_the_tap_at_speed_to_range() {
    // Registry spells.SPELL_AS_DEPLOY_LAUNCH_MODEL = airborne_from_behind_lands_on_tap:
    // an airborne log (no hitbox) lands on the tap after ceil(MinDistance / airborne
    // step) ticks and rolls ProjectileRange along the caster's forward axis. A Cannon
    // on the axis is first hit on the first step whose swept rectangle touches it; the
    // last Cannon whose disc touches the final rectangle is hit, one a subtile farther
    // never is. Plants: airborne_skipped, roll_range_from_airborne_start.
    assert_registry("spells.SPELL_AS_DEPLOY_LAUNCH_MODEL", format!("{:?}", calib().spell_as_deploy_launch), "AirborneFromBehind");
    let s0 = bare(config());
    let (air_step, min_d, step, range, _hw, hd) = log_numbers(&s0);
    let r = card_stat(&s0, "Cannon").collision_radius;
    let land = ((min_d + air_step - 1) / air_step) as u32;
    let tap = log_tap();
    // Along-axis offsets of Cannon centres: at the tap, mid-roll (on the far bank), at
    // the very end, one subtile past the end.
    let mid = s0.arena().water_y_max + SUBTILE - tap.y;
    let end = range + hd + r;
    let offsets = [0, mid, end];
    let cannons: Vec<Vec2> = offsets.iter().map(|d| Vec2::new(tap.x, tap.y + d)).collect();
    let (hits, _) = log_over_cannons(config(), &cannons, None);
    // First step j (1-based, the landing tick is j = 1) whose front face reaches the
    // disc: j * step + hd >= d - r.
    let expect = |d: i32| -> u32 { land + ((d - r - hd).max(0) + step - 1).div_euclid(step).max(1) as u32 - 1 };
    for (j, d) in offsets.iter().enumerate() {
        assert_eq!(hits[j].first().copied(), Some(expect(*d)), "Cannon {j} at along-offset {d}: first hit (land {land}, step {step}, hd {hd}, r {r})");
    }
    assert_eq!(expect(end), land + ((range + step - 1) / step) as u32 - 1, "the end Cannon is reached on the final step");
    let (past, _) = log_over_cannons(config(), &[Vec2::new(tap.x, tap.y + end + 1)], None);
    assert!(past[0].is_empty(), "a Cannon one subtile beyond the roll's reach was hit at {:?}", past[0]);
}

#[test]
fn log_width_edge_is_rect_vs_circle_to_the_subtile() {
    // Lateral offsets: disc edge exactly on the half-width (hit), one subtile beyond
    // (miss). Registry spells.ROLLING_HIT_SHAPE = rect_vs_circle_edge. Plant:
    // rolling_centre_in_rect.
    assert_registry("spells.ROLLING_HIT_SHAPE", format!("{:?}", calib().rolling_hit_shape), "RectVsCircleEdge");
    let s0 = bare(config());
    let (_, _, _, _, hw, _) = log_numbers(&s0);
    let r = card_stat(&s0, "Cannon").collision_radius;
    let tap = log_tap();
    let y = s0.arena().water_y_max + 3 * SUBTILE;
    let (hits, _) = log_over_cannons(config(), &[Vec2::new(tap.x + hw + r, y), Vec2::new(tap.x - hw - r - 1, y)], None);
    assert!(!hits[0].is_empty(), "a disc touching the half-width was missed");
    assert!(hits[1].is_empty(), "a disc one subtile outside the half-width was hit");
}

#[test]
fn log_pierces_hits_each_victim_once_and_damage_scales() {
    // Three Cannons along the axis (each overlapped by the swept rectangle for several
    // ticks) and a Red princess tower in the path: each loses exactly one hit. Plants:
    // rolling_rehit_every_tick, ct_floor, spell_damage_unscaled, crown_pct_ignored.
    let s0 = bare(config());
    let tap = log_tap();
    let y0 = s0.arena().water_y_max + 2 * SUBTILE;
    let cannons = [Vec2::new(tap.x, y0), Vec2::new(tap.x, y0 + 2 * SUBTILE), Vec2::new(tap.x, y0 + 4 * SUBTILE)];
    for level in [9, 10] {
        let (hits, loss) = log_over_cannons(config(), &cannons, Some(level));
        let dmg = scaled_from_json(&s0, "Log", level, int(&log_roll()["damage"]));
        for j in 0..3 {
            assert_eq!(hits[j].len(), 1, "level {level} Cannon {j}: hit on ticks {:?}", hits[j]);
            assert_eq!(loss[j], dmg, "level {level} Cannon {j}");
        }
    }
    // The crown tower: a Log cast in front of Red's engine-Left princess.
    let tower_pos = s0.arena().princess_tower_pos(Team::Red, royalesim::arena::Lane::Left);
    let tower = s0.tower_ids(Team::Red)[1].unwrap();
    let level = 10;
    let mut s = bare(config());
    let full = s.entity(tower).unwrap().hp;
    s.spawn_unit(Team::Blue, "Log", Vec2::new(tower_pos.x, tower_pos.y - 3 * SUBTILE), Some(level)).unwrap();
    let mut drops = 0;
    let mut last = full;
    let step = drain_step(&s, tower);
    for _ in 0..120 {
        s.tick();
        let hp = s.entity(tower).unwrap().hp;
        drops += u32::from(last - hp > step);
        last = hp;
    }
    let dmg = scaled_from_json(&s, "Log", level, int(&log_roll()["damage"]));
    let pct = int(&log_roll()["crown_tower_damage_percent"]);
    assert!((dmg * pct) % 100 != 0, "rounding not exercised");
    assert_eq!((drops, full - last), (1, crown_ceil(dmg, pct)), "princess tower: one hit of the reduced share");
}

#[test]
fn log_hits_ground_enemies_only() {
    // A RED Log (rolling -y) over deploying victims on the stage row, out of every
    // tower's reach: Blue Minions (air) and a Red Knight (friend) are untouched, a
    // Blue Knight (ground enemy) is hit and pushed. Compared with a control battle on
    // every tick while all three are still deploying (stationary, so nothing but the
    // Log can make them differ). Plants: rolling_hits_air, spell_friendly_fire.
    // Row y = 18: every Blue victim's hitbox edge is beyond Red's princess range, so no
    // tower shot can differ between the two battles.
    let row = stage().y - SUBTILE;
    let tap = Vec2::new(stage().x, row + 3 * SUBTILE / 2);
    let victims = [(Team::Blue, "Minions", Vec2::new(tap.x, row)), (Team::Red, "Knight", Vec2::new(tap.x - 3 * SUBTILE / 2, row)), (Team::Blue, "Knight", Vec2::new(tap.x + 3 * SUBTILE / 2, row))];
    let mut s = bare(config());
    let mut control = s.clone();
    s.spawn_unit(Team::Red, "Log", tap, None).unwrap();
    let mut enemy_hit = false;
    let mut checked = 0;
    for k in 0..40u32 {
        if k == 2 {
            for st in [&mut s, &mut control] {
                for &(team, card, p) in &victims {
                    st.spawn_unit(team, card, p, None).unwrap();
                }
            }
        }
        s.tick();
        control.tick();
        let views: Vec<_> = control.entities().filter(|e| e.kind == EntityKind::Troop).map(|e| (e.id, e.team, e.card.to_string(), e.hp, e.pos, e.deploying)).collect();
        if views.is_empty() || !views.iter().all(|v| v.5) {
            continue;
        }
        checked += 1;
        for (id, team, card, hp, pos, _) in views {
            let Some(v) = s.entity(id) else {
                panic!("tick {k}: {team:?} {card} was killed by the Log (friend or air unit touched)");
            };
            match (team, card.as_str()) {
                (Team::Blue, "Minions") => assert_eq!((v.hp, v.pos), (hp, pos), "tick {k}: an air unit was touched by the Log"),
                (Team::Red, _) => assert_eq!((v.hp, v.pos), (hp, pos), "tick {k}: friendly fire from the Log"),
                (Team::Blue, _) => enemy_hit |= v.hp < hp && v.pos != pos,
            }
        }
    }
    assert!(checked >= 10, "vacuous: only {checked} ticks compared while deploying");
    assert!(enemy_hit, "vacuous: the Log never hit and pushed the Blue Knight while it deployed");
}

#[test]
fn log_pushes_forward_and_moves_ignore_pushback_units() {
    // PushbackAll: a deploying Red Giant (IgnorePushback) in the path is pushed by
    // exactly Pushback along the roll, forward -- including one standing exactly on
    // the tap, which the landing log covers from behind its centre.
    // Plants: pushback_respects_ignore, rolling_push_radial_from_centre, and
    // rolling_push_from_tick_end COMPOSED with it (the tick-end plant is dead alone
    // under travel_direction -- see the header).
    assert_registry("knockback.DIRECTION_ROLLING", format!("{:?}", calib().knock_direction_rolling), "TravelDirection");
    assert!(log_roll()["pushback_all"].as_bool().unwrap(), "data: the rolling Log ships PushbackAll");
    let push = milli(int(&log_roll()["pushback_milli"]));
    // the ladder's whole carry down the axis (an exact per-step (0, 256) heading);
    // Pushback itself under the fixed_distance arm
    let carry = knock_carry(&calib(), push);
    // A tap where a 1-tile push stays on dry ground (log_tap's +1.5 would reach the river).
    let tap = t(900, 1100);
    for offset in [0, 3 * SUBTILE / 2] {
        let at = Vec2::new(tap.x, tap.y + offset);
        let moved = log_push_on(Team::Blue, tap, "Giant", at).unwrap_or_else(|| panic!("Giant at along-offset {offset}: never pushed"));
        assert_eq!(moved, Vec2::new(0, carry), "Giant at along-offset {offset}: push {moved:?}");
    }
    // OFF-AXIS: a deploying Red Knight 1.5 tiles to the side is pushed PURE FORWARD by
    // Pushback, with ZERO sideways component (registry travel_direction).
    // A radial push (radial_from_projectile_centre) would instead move it away from
    // the axis as well as forward, which a loose assertion -- d.x > 0 && d.y > 0 and a
    // length within 2 subtiles of Pushback -- cannot tell apart from the real rule.
    // Hence the exact vector below.
    // Plant (regression): rolling_push_radial_from_centre.
    let at = Vec2::new(tap.x + 3 * SUBTILE / 2, tap.y + 3 * SUBTILE / 2);
    let d = log_push_on(Team::Blue, tap, "Knight", at).expect("the off-axis Knight was never pushed");
    assert_eq!(d, Vec2::new(0, carry), "off-axis push {d:?}: under travel_direction it is pure own-forward, exactly the carry, ZERO sideways");
}

/// Run one Log cast and return the SETTLED displacement `victim` suffers, or None if
/// it was never touched. Compared against a control battle running the same script
/// without the Log, so the only thing that can move the victim is the push; the
/// victim is asserted to be still DEPLOYING in the control, i.e. exactly stationary,
/// which is what makes the displacement attributable. The victims drop two ticks
/// before the landing (~~eight~~ -- the shipped ladder needs n + 1 ticks to carry a
/// Pushback, and the deploy window has to outlast it), and the displacement is read
/// once the ladder is out.
fn log_push_on(caster: Team, tap: Vec2, victim: &str, at: Vec2) -> Option<Vec2> {
    let s0 = bare(config());
    let (air_step, min_d, _, _, _, _) = log_numbers(&s0);
    let land = ((min_d + air_step - 1) / air_step) as u32;
    let foe = match caster {
        Team::Blue => Team::Red,
        Team::Red => Team::Blue,
    };
    let mut s = bare(config());
    let mut control = s.clone();
    s.spawn_unit(caster, "Log", tap, None).unwrap();
    let mut touched = false;
    for k in 0..land + 40 {
        if k + 2 == land {
            for st in [&mut s, &mut control] {
                st.spawn_unit(foe, victim, at, None).unwrap();
            }
        }
        s.tick();
        control.tick();
        if let Some(v) = control.entities().find(|e| e.card == victim) {
            let a = s.entity(v.id).unwrap();
            let d = a.pos.sub(v.pos);
            touched |= a.push_active || d != Vec2::default();
            if !v.deploying {
                return None; // the window closed; see the vacuity note in the test
            }
            if touched && !a.push_active {
                return Some(d);
            }
        }
    }
    None
}

#[test]
fn log_never_pushes_a_victim_backward_or_sideways_in_either_seat() {
    // In the real game the Log never pushes a troop BACKWARD toward the caster: it
    // is always forward (calibration knockback.DIRECTION_ROLLING, whose provenance
    // records the observation). This is the gate that makes that enforceable across
    // the whole reachable range of offsets. Sampling only along-offsets 0 and
    // +1.5 tiles -- as the test above does -- cannot see the failure: it is the
    // offsets from -1 subtile back to the landing log's reach that a radial rule
    // throws a full tile toward the caster. Every push here must be EXACTLY
    // (0, own-forward Pushback) -- one vector, no tolerance.
    // Plant (regression): rolling_push_radial_from_centre.
    assert_registry("knockback.DIRECTION_ROLLING", format!("{:?}", calib().knock_direction_rolling), "TravelDirection");
    let s0 = bare(config());
    let push = knock_carry(&calib(), milli(int(&log_roll()["pushback_milli"])));
    let hd = milli(int(&log_roll()["projectile_radius_y_milli"]));
    // The deepest a victim can stand behind the tap and still be touched by the
    // landing log's back edge: half-depth plus its own radius. DERIVED from the data,
    // never typed, so a cards.json regeneration moves the boundary with it.
    let giant_r = card_stat(&s0, "Giant").collision_radius;
    let deepest = -(hd + giant_r);
    let blue_tap = t(900, 1100);
    let along = [deepest, -19800, -18000, -9000, -1800, -1, 0, 1800, 18000, 27000];
    let lateral = [-27000, -18000, -1800, 0, 1800, 18000, 27000];
    let mut checked = 0;
    for caster in [Team::Blue, Team::Red] {
        // Own-forward is +y for Blue, -y for Red, and Red's tap is the seat ROTATION
        // of Blue's, so both seats run the identical own-frame scenario.
        let (tap, fwd) = match caster {
            Team::Blue => (blue_tap, 1),
            Team::Red => (mirror(&s0, blue_tap), -1),
        };
        let want = Vec2::new(0, fwd * push);
        for off in along {
            // Giant: IgnorePushback, so these cases also keep PushbackAll certified.
            let at = Vec2::new(tap.x, tap.y + fwd * off);
            let d = log_push_on(caster, tap, "Giant", at)
                .unwrap_or_else(|| panic!("{caster:?} Log: the Giant at own along-offset {off} was never pushed while it stood still"));
            assert_eq!(d, want, "{caster:?} Log, Giant at own along-offset {off}");
            checked += 1;
        }
        for lat in lateral {
            let at = Vec2::new(tap.x + fwd * lat, tap.y + fwd * 27000);
            let d = log_push_on(caster, tap, "Knight", at)
                .unwrap_or_else(|| panic!("{caster:?} Log: the Knight at own lateral offset {lat} was never pushed while it stood still"));
            assert_eq!(d, want, "{caster:?} Log, Knight at own lateral offset {lat}");
            checked += 1;
        }
        // NEGATIVE CONTROL: one NATIVE unit (18 subtiles) beyond the reach boundary
        // is not touched at all. Without it the sweep could be measuring an artefact
        // rather than reach. It has to be a whole native unit, not one subtile: the
        // shipped movement law keeps every position a whole number of native units,
        // so a one-subtile offset is truncated back onto the boundary on the unit's
        // first update.
        let past = Vec2::new(tap.x, tap.y + fwd * (deepest - royalesim::fixed::SUBTILE_PER_MILLITILE));
        assert_eq!(log_push_on(caster, tap, "Giant", past), None, "{caster:?} Log: a Giant one native unit past the back reach was pushed");
    }
    assert_eq!(checked, 2 * (along.len() + lateral.len()), "vacuity: not every victim was measured");
}

// ---------------------------------------------------------------------------
// GOBLIN BARREL

#[test]
fn barrel_flight_releases_nothing_until_it_lands() {
    // No Blue Goblin exists before the arrival tick; after the following Spawn phase
    // exactly SpawnCharacterCount do. Plants: barrel_instant_spawn, spell_launch_from_tap.
    let mut s = bare(config());
    let tap = Vec2::new(king(&s, Team::Blue).x, stage().y);
    s.spawn_unit(Team::Blue, "GoblinBarrel", tap, None).unwrap();
    let step = int(&barrel_proj()["speed"]) * calib().projectile_speed_to_subtiles_per_tick;
    let dist = tap.y - king(&s, Team::Blue).y;
    let land = ((dist + step - 1) / step) as u32;
    let unit = raw_card("GoblinBarrel")["spell"]["spawn"]["character"].as_str().unwrap().to_string();
    let count = int(&raw_card("GoblinBarrel")["spell"]["spawn"]["count"]) as usize;
    for k in 1..=land {
        s.tick();
        assert_eq!(find_live(&s, Team::Blue, &unit).len(), 0, "call {k}: a {unit} exists before the barrel landed on call {land}");
    }
    assert!(s.spells().is_empty(), "the barrel did not land on call {land}");
    s.tick();
    assert_eq!(find_live(&s, Team::Blue, &unit).len(), count, "released units after the Spawn following the landing");
}

#[test]
fn barrel_releases_goblins_at_the_barrel_level_with_the_barrel_deploy_time() {
    // Plants: spawn_uses_character_deploy_time, spawn_count_one, spawn_level_local_one.
    let spawn = &raw_card("GoblinBarrel")["spell"]["spawn"];
    let unit = spawn["character"].as_str().unwrap();
    let urec = &cards_doc()["units"][unit];
    for level in [9, 10] {
        let mut s = bare(config());
        let tap = stage();
        s.spawn_unit(Team::Blue, "GoblinBarrel", tap, Some(level)).unwrap();
        let mut seen = Vec::new();
        for _ in 0..200 {
            s.tick();
            seen = find_live(&s, Team::Blue, unit);
            if !seen.is_empty() {
                break;
            }
        }
        assert_eq!(seen.len(), int(&spawn["count"]) as usize, "level {level}: count");
        // Unit level: the SpawnCharacterLevelIndex reading gives the barrel's unified
        // level for every shipped row (docs/spell-spec.md); the unit's Common table from
        // a Common card in cards.json.
        let common = raw_card("Knight");
        assert_eq!(common["rarity"], urec["rarity"], "data: Goblin and Knight share a rarity table");
        let rel = s.cards().rarity(urec["rarity"].as_str().unwrap()).unwrap().relative_level;
        let m = int(&common["level_scaling"]["multiplier_percent_by_level"][(level - rel - 1) as usize]);
        let hp = int(&urec["hitpoints"]) * m / 100;
        for g in &seen {
            assert_eq!(g.max_hp, hp, "level {level}: Goblin hp");
            // less the countdown that already ran on the release tick (match.TICK_ORDER)
            assert_eq!(g.deploy_ms + spawn_tick_countdown(&calib()), int(&spawn["deploy_time_ms"]), "level {level}: SpawnCharacterDeployTime, not the unit's own");
        }
        assert_ne!(int(&spawn["deploy_time_ms"]), int(&urec["deploy_time_ms"]), "data: the override must differ or the test is vacuous");
    }
}

#[test]
fn barrel_near_the_river_never_releases_a_goblin_onto_water() {
    // Cast on the last dry half-row before the river on a non-bridge column: the
    // engine grid formation puts a Goblin on water, which must be kept on land (the
    // formation fallback shared with troops, state.rs formation_grid), checked by the
    // every-tick invariants. Plant: formation_ignores_water.
    let mut s = bare(config());
    let a = s.arena().clone();
    let tap = Vec2::new(t(900, 0).x, a.water_y_min - milli(100));
    assert!(a.is_passable_ground(tap));
    s.spawn_unit(Team::Blue, "GoblinBarrel", tap, None).unwrap();
    let mut inv = Invariants::new(DEFAULT_TOLERANCE);
    let mut goblins = 0;
    for _ in 0..200 {
        s.tick();
        goblins = goblins.max(find_live(&s, Team::Blue, "Goblin").len());
        inv.check(&s).unwrap();
    }
    assert!(goblins >= 3, "vacuous: {goblins} goblins");
}

// ---------------------------------------------------------------------------
// PLACEMENT

#[test]
fn spell_placement_follows_the_card_data() {
    // Fireball/Arrows/Zap: anywhere strictly inside the arena (river, enemy side, on
    // buildings). The Log (SpellAsDeploy, no CanDeployOnEnemySide): troop territory,
    // but on buildings (CanPlaceOnBuildings). Goblin Barrel: anywhere but water.
    // Plants: log_territory_anywhere, barrel_anywhere_incl_water.
    let deck: Vec<String> = ["Fireball", "Arrows", "Zap", "Log", "GoblinBarrel", "Knight", "Giant", "Cannon"].iter().map(|x| x.to_string()).collect();
    let mut cfg = config();
    cfg.decks = [deck.clone(), deck];
    let mut s = BattleState::new(1, cfg);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let a = s.arena().clone();
    let river = Vec2::new(a.width / 2, (a.water_y_min + a.water_y_max) / 2);
    let enemy = t(900, 2400);
    let own_tower = a.princess_tower_pos(Team::Blue, royalesim::arena::Lane::Left);
    let enemy_king = a.king_tower_pos(Team::Red);
    let probe = |s: &BattleState, card: &str, p: Vec2| {
        // Rotate the card into slot 0 by name lookup.
        s.check_deploy(Team::Blue, card, p)
    };
    // Put every spell into the hand, one at a time, by cycling.
    let mut verdicts = std::collections::BTreeMap::new();
    // THIS TEST USED TO COVER ITS DECK BY ACCIDENT. It deployed twice a round, once
    // onto its own tower, and the walk over the hand only reached all eight cards
    // because some of those deploys FAILED. When the building placement rule
    // started relocating that tower tap instead of refusing it, the walk became a
    // three-card cycle and two cards were never probed at all, while the
    // assertions below still passed for the six that were. The coverage assertion
    // after the loop is what makes that say so rather than pass quietly.
    for _ in 0..8 {
        // Refill BEFORE probing, not after. This test is about placement rules, and
        // a probe must not be able to fail on elixir: the cycling deploy below spends
        // up to 5 of the 10, and whether it spends anything at all depends on whether
        // it is accepted, which is exactly what placement decides. Once the building
        // placement rule started relocating an unfittable tap instead of refusing it,
        // the second deploy this test used to make onto its own tower began to
        // succeed and spend, and a Fireball probe on the next round failed with 2
        // elixir against 4. That second deploy is gone now; one is enough.
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        let hand: Vec<String> = s.hand(Team::Blue).iter().map(|x| x.to_string()).collect();
        for card in &hand {
            for (label, p) in [("river", river), ("enemy", enemy), ("own_tower", own_tower), ("enemy_king", enemy_king)] {
                verdicts.insert((card.clone(), label), probe(&s, card, p));
            }
        }
        // EXACTLY ONE deploy per round, and at a point every card in this deck can
        // legally take. Slot 0 then advances by exactly one card per round, so the
        // walk over the queue is deterministic instead of depending on which
        // deploys happen to fail. The probe loop above reads the whole hand, so the
        // three cards that never leave slots 1..3 are covered every round.
        s.deploy_slot(Team::Blue, 0, t(900, 1000)).expect("the cycling deploy must be legal for every card in this deck");
        s.tick();
    }
    // Every deck card must have been probed, or the assertions below are vacuous
    // for the ones that were not.
    for card in ["Fireball", "Arrows", "Zap", "Log", "GoblinBarrel", "Knight", "Giant", "Cannon"] {
        assert!(
            verdicts.contains_key(&(card.to_string(), "river")),
            "{card} never reached the hand in the 8 rounds this test runs, so it covers less than it claims"
        );
    }
    let v = |c: &str, l: &str| verdicts.get(&(c.to_string(), l)).cloned().unwrap_or_else(|| panic!("{c} never reached the hand"));
    for spell in ["Fireball", "Arrows", "Zap"] {
        for l in ["river", "enemy", "own_tower", "enemy_king"] {
            assert_eq!(v(spell, l), Ok(()), "{spell} at {l}");
        }
    }
    assert_eq!(v("Log", "river"), Err(DeployError::Water));
    assert_eq!(v("Log", "enemy"), Err(DeployError::OutOfTerritory));
    assert_eq!(v("Log", "own_tower"), Ok(()), "the Log ships CanPlaceOnBuildings");
    assert_eq!(v("GoblinBarrel", "river"), Err(DeployError::Water));
    assert_eq!(v("GoblinBarrel", "enemy"), Ok(()));
    assert_eq!(v("GoblinBarrel", "enemy_king"), Ok(()), "a barrel may land on the king");
    assert_eq!(v("Knight", "own_tower"), Err(DeployError::Occupied), "control: a troop is refused on a building");
}
