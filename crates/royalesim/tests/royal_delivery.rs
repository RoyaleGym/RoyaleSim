//! THE ROYAL DELIVERY: a centre-aimed strike (card.rs `StrikePick::AreaCentre`, `centre_strike_shape`; spell.rs
//! `step_spells`, the Strikes and Flight arms).
//!
//! THE LAW, measured on client 15.535.29 (7 casts), C the cast tick (k = 0 below, the first tick run after the play):
//!   - the area (RoyalDeliveryArea: HitSpeed 2000, LifeDuration 2000, Radius 3000) strikes once, on the update whose
//!     clock reaches zero (spells.STRIKE_DUE = clock_at_or_below_zero): its crate is made on C + 39 and lands on C + 40;
//!   - the crate hits every enemy, air and ground, whose edge is within 3000 of the tap, for the PROJECTILE row's
//!     damage (150, 384 at level 11), and it hits an enemy building although the area row sets IgnoreBuildings
//!     (spells.AREA_PROJECTILE_IGNORE_BUILDINGS = projectile_row, one run);
//!   - it releases one DeliveryRecruit for the caster on the tap, deploying 250 ms (C + 40 to C + 44), first step C + 46.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the card as a centre-aimed strike of the rows' numbers, and refuses the shapes it does not
//!      read; the area's SpawnInitialDelay (2050) agrees with the modelled landing tick;
//!   2. the crate exists after C + 39 only, and its damage lands on C + 40;
//!   3. it hits by the edge, air and ground, and not its own side;
//!   4. it hits an enemy building under projectile_row and not under area_row;
//!   5. one Recruit for the caster on the tap, at the caster's level, deploying C + 40 to C + 44, first step C + 46,
//!      facing its side's forward; the Red cast is the rotation of the Blue one; it reports the card's catalogue id;
//!   6. a crate in the air is state: a save edited only in its chain depth fails the load's hash self-check, and an
//!      unedited save resumes it.
//!
//! THE SCENE. The tap is the middle of the arena a tile north of the river, where no crown tower reaches a unit of
//! radius 500 (the Red princess towers' reach is 7500 + 1000 + 500 = 9000, and the tap is 9301 from each). The victims
//! are deployed from hand 15 ticks before the landing, so each stands still (a deploying unit does) and each is an
//! area's victim (a deploying unit is). Every loss is read against a control battle run without the cast.
//!
//! No measurement on disk: crown towers, the Recruit's shield, levels other than 11, a tap on the enemy half (the scene
//! taps Blue's enemy half; the law is the engine's there).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test royal_delivery`):
//!   * `delivery_spawn_time_unchecked` -- the area's SpawnTime is not held against the projectile's: (1) goes red.
//!   * `strike_due_below_zero` -- the strike waits for the clock to fall below zero: (2) and (3) go red.
//!     tests/strikes.rs and tests/strike_timing16402.rs stay green under it (a strike that picks the highest hp,
//!     the Lightning's, does not read spells.STRIKE_DUE).
//!   * `area_projectile_reads_area_row` -- the delivery reads the area row's IgnoreBuildings: (4) goes red.
//!   * `delivery_release_dropped` -- the delivery releases nothing: (5) goes red.
//!   * `hash_skips_spell_depth` (tests/spell_summon.rs's) -- the chain depth is not hashed: (6) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, SpellPlacement, SpellShape, StrikePick};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{AreaProjectileIgnoreBuildings, BattleConfig, BattleState, StrikeDue};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The tap, native (THE SCENE in the header).
const TAP: (i32, i32) = (9000, 18000);

fn centre_distance(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// The delivery's damage at `level`: the projectile row's 150, scaled on the card's ladder.
fn delivery_damage(s: &BattleState, level: i32) -> i32 {
    let idx = s.cards().index("RoyalDelivery").expect("RoyalDelivery loads");
    s.cards().scaled(idx, level, 150).expect("a valid level")
}

/// The shipped config, asserting the arms this file pins.
fn shipped() -> BattleConfig {
    let c = config();
    assert_eq!(c.calib.strike_due, StrikeDue::ClockAtOrBelowZero, "the shipped spells.STRIKE_DUE this file pins");
    assert_eq!(c.calib.area_projectile_ignore_buildings, AreaProjectileIgnoreBuildings::ProjectileRow, "the shipped spells.AREA_PROJECTILE_IGNORE_BUILDINGS");
    c
}

/// The config with spells.AREA_PROJECTILE_IGNORE_BUILDINGS at `rule`.
fn with_building_rule(rule: AreaProjectileIgnoreBuildings) -> BattleConfig {
    let mut c = config();
    c.calib.area_projectile_ignore_buildings = rule;
    c
}

/// The depth-1 object a centre-aimed strike makes (the crate), if one stands.
fn crate_of(s: &BattleState) -> Option<Vec2> {
    s.spells().iter().find_map(|sp| match &sp.motion {
        SpellMotion::Flight { pos, .. } if sp.depth == 1 && s.cards().get(sp.card).name == "RoyalDelivery" => Some(*pos),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// (1)

/// A synthetic Royal Delivery: the area (SpawnTime `spawn_time`), its projectile (SpawnCharacterDeployTime
/// `deploy_time`, and `count`, spliced in: `,"spawn_character_count":1` or nothing) and the Guard it releases.
/// `area_extra` is spliced into the area's record.
fn synthetic(area_extra: &str, spawn_time: i32, deploy_time: i32, count: &str) -> String {
    format!(
        r#"{{"cards":[{{"name":"Crate","kind":"spell","elixir":3,"rarity":"Common",
        "spell":{{"spell_as_deploy":true,"can_place_on_buildings":true,"can_deploy_on_enemy_side":false,
        "area_effect_object":{{"name":"CrateArea","life_duration_ms":2000,"radius_milli":3000,"hit_speed_ms":2000,"only_enemies":true,
        "hits_ground":true,"hits_air":true,"ignore_buildings":true,"spawn_time_ms":{spawn_time}{area_extra},
        "projectile":{{"name":"CrateBox","speed":5000,"damage":150,"radius_milli":3000,"aoe_to_air":true,"aoe_to_ground":true,
        "only_enemies":true,"spawn_character":"Guard","spawn_character_deploy_time_ms":{deploy_time}{count}}}}}}}}}],
        "units":{{"Guard":{{"name":"Guard","rarity":"Common","hitpoints":214,"hit_speed_ms":1300,"range_milli":1600,
        "collision_radius_milli":500,"speed":60,"deploy_time_ms":1000}}}}}}"#
    )
}

/// Ok when `card` loads from `text`, else the reason it is refused.
fn refusal(text: &str, card: &str) -> Result<(), String> {
    let db = CardDb::from_json_str(text, CardSource::DerivedJson).map_err(|e| format!("the file does not parse: {e}"))?;
    match db.rejected.iter().find(|(n, _)| n == card) {
        Some((_, why)) => Err(why.clone()),
        None if db.index(card).is_some() => Ok(()),
        None => Err(format!("{card} neither loads nor is refused")),
    }
}

/// Plant: delivery_spawn_time_unchecked.
#[test]
fn royal_delivery_loads_as_a_centre_strike_that_releases_one_recruit() {
    let s = BattleState::new(0, shipped());
    let spell = card_stat(&s, "RoyalDelivery").spell.as_ref().expect("RoyalDelivery loads as a spell");
    assert_eq!(spell.placement, SpellPlacement::TroopTerritory { on_buildings: true }, "SpellAsDeploy without CanDeployOnEnemySide");
    let SpellShape::Strikes(d) = &spell.shape else { panic!("RoyalDelivery: {:?}", spell.shape) };
    assert_eq!((d.pick, d.life_ms, d.gaps_ms.clone()), (StrikePick::AreaCentre, 2000, vec![2000]));
    assert_eq!((d.hit.damage, d.hit.radius), (150, 3000 * K), "the projectile row's damage over the area's radius");
    assert!(d.hit.only_enemies && d.hit.hits_air && d.hit.hits_ground && d.hit.ignore_buildings, "the area's filters: {:?}", d.hit);
    let Some(SpellShape::Projectile { hit: Some(h), spawn: Some(sp), waves: 1, .. }) = d.delivery.as_deref() else { panic!("the delivery: {:?}", d.delivery) };
    assert_eq!((h.damage, h.radius), (150, 3000 * K));
    assert!(h.only_enemies && h.hits_air && h.hits_ground && !h.ignore_buildings, "the projectile row's filters: {h:?}");
    assert_eq!((sp.count, sp.deploy_time_ms), (1, Some(250)), "one unit, deploying SpawnCharacterDeployTime");
    let unit = s.cards().get(sp.unit);
    assert_eq!((unit.name.as_str(), unit.summon_only), ("DeliveryRecruit", true), "the released unit");
    // The data against the model: the area row's SpawnInitialDelay says when the unit comes out, and the model's
    // landing tick (the strike on update HitSpeed / TICK_MS, the crate one tick later) must agree with it.
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("cards.json parses");
    let row = doc["cards"].as_array().unwrap().iter().find(|c| c["name"] == "RoyalDelivery").expect("the RoyalDelivery row");
    let initial = row["spell"]["area_effect_object"]["spawn_initial_delay_ms"].as_i64().expect("SpawnInitialDelay") as i32;
    let tick = s.config().calib.tick_ms;
    assert_eq!(initial / tick - 1, d.gaps_ms[0] / tick, "SpawnInitialDelay {initial} against the modelled landing tick");
    // The shapes it refuses, on a synthetic copy that loads.
    let one = r#","spawn_character_count":1"#;
    assert_eq!(refusal(&synthetic("", 250, 250, one), "Crate"), Ok(()), "the synthetic copy must load");
    let buff = r#","buff":{"name":"Slow","speed_multiplier_raw":-30},"buff_time_ms":1000"#;
    let why = refusal(&synthetic(buff, 250, 250, one), "Crate").unwrap_err();
    assert!(why.contains("the area carries its own Damage, Buff"), "a Buff on the area: {why}");
    let why = refusal(&synthetic("", 250, 300, one), "Crate").unwrap_err();
    assert!(why.contains("SpawnTime 250") && why.contains("disagree"), "SpawnTime against SpawnCharacterDeployTime: {why}");
    let why = refusal(&synthetic("", 250, 250, ""), "Crate").unwrap_err();
    assert!(why.contains("with no count"), "a blank SpawnCharacterCount: {why}");
}

// ---------------------------------------------------------------------------
// the scene

/// One victim: its side, its card, where it is deployed (native).
type Victim<'a> = (Team, &'a str, (i32, i32));

/// A battle in `cfg`, the Royal Delivery cast by Blue at the tap when `cast` (the control otherwise), and the victims
/// deployed from hand after tick 24 (they appear on C + 25 and deploy for 20 ticks, through the landing on C + 40).
/// Runs to C + 40 and returns the battle and each victim's id, found by the point it appeared on.
fn scene(cfg: BattleConfig, cast: bool, victims: &[Victim]) -> (BattleState, Vec<EntityId>) {
    let mut s = BattleState::new(0, cfg);
    if cast {
        s.spawn_unit(Team::Blue, "RoyalDelivery", at(TAP), None).expect("cast RoyalDelivery");
    }
    let mut ids = Vec::new();
    for k in 0..41u32 {
        s.tick();
        if k == 24 {
            for &(team, card, p) in victims {
                s.spawn_unit(team, card, at(p), None).unwrap_or_else(|e| panic!("deploy {card}: {e:?}"));
            }
        }
        if k == 25 {
            ids = victims
                .iter()
                .map(|&(team, card, p)| {
                    s.entities()
                        .filter(|e| e.team == team && e.card == card)
                        .min_by_key(|e| centre_distance(e.pos, at(p)))
                        .map(|e| e.id)
                        .unwrap_or_else(|| panic!("the victim {card} did not appear"))
                })
                .collect();
        }
    }
    (s, ids)
}

// ---------------------------------------------------------------------------
// (2)

/// Plant: strike_due_below_zero.
#[test]
fn the_crate_lands_on_c_plus_40() {
    let (tx, ty) = TAP;
    let mut s = BattleState::new(0, shipped());
    let mut control = BattleState::new(0, shipped());
    s.spawn_unit(Team::Blue, "RoyalDelivery", at(TAP), None).expect("cast RoyalDelivery");
    let (mut seen, mut lost) = (Vec::new(), Vec::new());
    let mut knight: Option<EntityId> = None;
    for k in 0..44u32 {
        s.tick();
        control.tick();
        if k == 24 {
            // A Red Knight deployed at the tap, standing through the landing (it deploys 20 ticks from C + 25).
            for b in [&mut s, &mut control] {
                b.spawn_unit(Team::Red, "Knight", at((tx, ty + 1000)), None).expect("deploy a Knight");
            }
        }
        if k == 25 {
            knight = s.entities().find(|e| e.team == Team::Red && e.card == "Knight").map(|e| e.id);
        }
        if crate_of(&s).is_some() {
            seen.push(k);
        }
        if let Some(id) = knight {
            let (a, b) = (s.entity(id).map_or(0, |v| v.hp), control.entity(id).map_or(0, |v| v.hp));
            if b > a {
                lost.push((k, b - a));
            }
        }
    }
    assert!(knight.is_some(), "the scene drifted: the Knight did not appear");
    assert_eq!(seen, vec![39], "the crate stands after C + 39 only");
    let dmg = delivery_damage(&s, s.config().card_level[0]);
    // the loss is the landing's, and stays: one entry per tick from C + 40 on, all of it the delivery's
    assert_eq!(lost.first(), Some(&(40, dmg)), "the delivery's damage lands on C + 40, against the control: {lost:?}");
}

// ---------------------------------------------------------------------------
// (3)

/// Plant: strike_due_below_zero.
#[test]
fn it_hits_by_the_edge_air_and_ground_and_not_its_own_side() {
    let (tx, ty) = TAP;
    let s0 = BattleState::new(0, config());
    let r_knight = card_stat(&s0, "Knight").collision_radius / K;
    let r_mega = card_stat(&s0, "MegaMinion").collision_radius / K;
    // (victim, hit?): the measured centre distances inside the edge, and misses a margin past it.
    let cases: Vec<(Victim, bool)> = vec![
        ((Team::Red, "Knight", (tx, ty + 2999)), true),
        ((Team::Red, "Knight", (tx + 3161, ty)), true),
        ((Team::Red, "Knight", (tx + 3000 + r_knight + 300, ty + 1200)), false),
        ((Team::Red, "MegaMinion", (tx - 2979, ty)), true),
        ((Team::Red, "MegaMinion", (tx - (3000 + r_mega + 300), ty + 1500)), false),
        ((Team::Blue, "Knight", (tx, ty - 500)), false),
    ];
    let victims: Vec<Victim> = cases.iter().map(|c| c.0).collect();
    let (cast, ids) = scene(shipped(), true, &victims);
    let (control, ids_c) = scene(shipped(), false, &victims);
    assert_eq!(ids, ids_c, "the scene drifted: the two battles numbered their victims apart");
    let dmg = delivery_damage(&cast, cast.config().card_level[0]);
    let centre = at(TAP);
    for ((v, hit), id) in cases.iter().zip(&ids) {
        let e = cast.entity(*id).unwrap_or_else(|| panic!("{} {:?} died", v.1, v.2));
        let reach = (3000 + e.radius / K) as i64;
        let d = centre_distance(e.pos, centre);
        assert!(e.deploying, "the scene drifted: {} at {:?} is no longer deploying", v.1, v.2);
        if *hit {
            assert!(d <= reach, "the scene drifted: {} stands {d} from the tap, past its reach {reach}", v.1);
        } else if v.0 == Team::Red {
            assert!(d > reach, "the scene drifted: {} stands {d} from the tap, inside its reach {reach}", v.1);
        }
        let c = control.entity(*id).unwrap_or_else(|| panic!("{} died in the control", v.1));
        let want = if *hit { dmg } else { 0 };
        assert_eq!(c.hp - e.hp, want, "{:?} {} {d} from the tap (reach {reach}): its loss against the control", v.0, v.1);
        assert_eq!(e.pos, c.pos, "{} was displaced: the delivery pushes nothing", v.1);
    }
}

// ---------------------------------------------------------------------------
// (4)

/// Plant: area_projectile_reads_area_row.
#[test]
fn it_hits_an_enemy_building() {
    let (tx, ty) = TAP;
    // The Red Cannon's hp after C + 39 and after C + 40.
    let run = |cfg: BattleConfig, cast: bool| -> (i32, i32) {
        let mut s = BattleState::new(0, cfg);
        let c = s.scenario_spawn_now(Team::Red, "Cannon", at((tx + 2550, ty)), None).expect("spawn a Red Cannon");
        if cast {
            s.spawn_unit(Team::Blue, "RoyalDelivery", at(TAP), None).expect("cast RoyalDelivery");
        }
        let mut hp = (0, 0);
        for k in 0..41u32 {
            s.tick();
            let now = s.entity(c).map_or(0, |v| v.hp);
            if k == 39 {
                hp.0 = now;
            }
            if k == 40 {
                hp.1 = now;
            }
        }
        hp
    };
    let s = BattleState::new(0, config());
    let dmg = delivery_damage(&s, s.config().card_level[0]);
    let control = run(shipped(), false);
    let projectile_row = run(shipped(), true);
    let area_row = run(with_building_rule(AreaProjectileIgnoreBuildings::AreaRow), true);
    assert_eq!(projectile_row.0, control.0, "the scene drifted: the Cannon lost hp to the cast before the landing");
    assert_eq!(control.1 - projectile_row.1, dmg, "projectile_row (shipped): the Cannon 2550 from the tap takes the delivery's full hit");
    assert_eq!(control.1 - area_row.1, 0, "area_row: the area's IgnoreBuildings spares the Cannon");
}

// ---------------------------------------------------------------------------
// (5)

/// The one Recruit's (pos, deploying, hp, max hp, facing, team) after each tick from C + 40 to C + 46, a cast by
/// `team` at `tap` in `cfg`.
fn recruit_track(cfg: BattleConfig, team: Team, tap: Vec2) -> Vec<(Vec2, bool, i32, i32, Vec2, Team)> {
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(team, "RoyalDelivery", tap, None).expect("cast RoyalDelivery");
    let mut out = Vec::new();
    for k in 0..47u32 {
        s.tick();
        let got: Vec<_> = s.entities().filter(|e| e.card == "DeliveryRecruit").collect();
        if k < 40 {
            assert!(got.is_empty(), "a Recruit before the landing, on C + {k}");
            continue;
        }
        assert_eq!(got.len(), 1, "C + {k}: one Recruit");
        let e = &got[0];
        out.push((e.pos, e.deploying, e.hp, e.max_hp, e.facing, e.team));
    }
    out
}

/// Plant: delivery_release_dropped.
#[test]
fn one_recruit_for_the_caster_on_the_landing_point() {
    let s = BattleState::new(0, config());
    let level = s.config().card_level[0];
    let idx = s.cards().index("DeliveryRecruit").expect("the Recruit loads as a unit");
    let hp = s.cards().scaled(idx, level, s.cards().get(idx).hitpoints).expect("a valid level");
    if level == 11 {
        assert_eq!(hp, 547, "the Recruit's 214 at level 11");
    }
    let blue = recruit_track(shipped(), Team::Blue, at(TAP));
    let (pos0, _, hp0, max0, facing0, team0) = blue[0];
    assert_eq!(team0, Team::Blue, "the Recruit is the caster's");
    assert_eq!(pos0, at(TAP), "the Recruit stands on the tap");
    assert_eq!((hp0, max0), (hp, hp), "the Recruit at the cast's level");
    assert_eq!(facing0, Vec2::new(0, 256), "facing the Blue side's forward");
    let deploying: Vec<bool> = blue.iter().map(|b| b.1).collect();
    assert_eq!(deploying, vec![true, true, true, true, true, false, false], "deploying on C + 40 to C + 44");
    assert!(blue[..6].iter().all(|b| b.0 == pos0), "the Recruit stands through C + 45: {:?}", blue.iter().map(|b| b.0).collect::<Vec<_>>());
    assert_ne!(blue[6].0, pos0, "the Recruit's first step is on C + 46");
    // The Red cast is the rotation of the Blue one, under the seat-symmetric config.
    let blue_sym = recruit_track(symmetric_config(), Team::Blue, at(TAP));
    let red_sym = recruit_track(symmetric_config(), Team::Red, mirror(&s, at(TAP)));
    assert_eq!(red_sym[0].5, Team::Red, "the Red cast's Recruit is Red's");
    assert_eq!(red_sym[0].4, Vec2::new(0, -256), "facing the Red side's forward");
    for (k, (b, r)) in blue_sym.iter().zip(&red_sym).enumerate() {
        assert_eq!((mirror(&s, b.0), b.1, b.2), (r.0, r.1, r.2), "C + {}: the Red Recruit is not the rotation of the Blue one", 40 + k);
    }
    // The Recruit reports the card that released it (py.rs `ids_of_indices`), never -1.
    let db = s.cards();
    let catalogue: Vec<u16> = ["Knight", "RoyalDelivery", "Zap"].iter().map(|n| db.index(n).expect("loads")).collect();
    let ids = royalesim::py::ids_of_indices(db, &catalogue);
    assert_eq!(ids[idx as usize], 1, "the Recruit reports the Royal Delivery's catalogue id");
}

// ---------------------------------------------------------------------------
// (6)

/// Plant: hash_skips_spell_depth.
#[test]
fn a_battle_saved_with_the_crate_in_the_air_resumes_it() {
    let mut s = BattleState::new(0, shipped());
    s.spawn_unit(Team::Blue, "RoyalDelivery", at(TAP), None).expect("cast RoyalDelivery");
    for _ in 0..40u32 {
        s.tick();
    }
    assert!(crate_of(&s).is_some(), "the scene drifted: no crate after C + 39");
    let hashed = edit_is_hashed(&s, |v| {
        let spells = v["spells"].as_array_mut().expect("the snapshot lists its spells");
        let sp = spells.iter_mut().find(|sp| sp["depth"].as_u64() == Some(1)).expect("the crate is saved");
        // depth 0 still names a shape (the striking area), so the load reaches its hash self-check
        sp["depth"] = serde_json::Value::from(0);
    });
    assert!(hashed, "a save edited only in the crate's chain depth loads under the old hash: the depth is not hashed");
    let mut resumed = BattleState::load(&s.save()).expect("the save loads");
    s.tick();
    resumed.tick();
    assert_eq!(resumed.state_hash(), s.state_hash(), "the resumed battle parts from the one that never saved");
    assert_eq!(resumed.entities().filter(|e| e.card == "DeliveryRecruit").count(), 1, "the resumed crate landed and released its Recruit");
}
