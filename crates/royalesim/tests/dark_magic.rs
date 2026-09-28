//! THE VOID (DarkMagic): a striking area whose strikes are an action (card.rs `StrikePick::CountTiers`,
//! `strike_area_shape`; spell.rs `selector_candidates`, `laser`, `tier_of`, `deliver`).
//!
//! THE LAW, measured on client 15.535.29 (the Void runs with their controls, level 11), C the cast tick (k = 0 below,
//! the first tick run after the play):
//!   - three strikes, on C + 30, C + 54 and C + 78 (the start delay 500 plus FirstHitDelay 1000, then HitFrequency
//!     1200, on the striking area's clock under spells.STRIKE_TIMER_LEFTOVER = carried); each strike's damage lands
//!     two ticks later, on C + 32, C + 56 and C + 80, as the one 100 ms pulse of the tier buff it hangs
//!     (status.BUFF_PULSE_TIMING = after_first_period);
//!   - every enemy within 2500 plus its own radius at a strike (spells.SELECTOR_REACH) takes the buff of the tier its
//!     COUNT falls in (spells.COUNT_TIER_RULE = by_count; MaxUnitPerActionList [1, 4]): alone 696, 97 on a crown tower;
//!     two to four 294 each, 51 on a crown tower; five and more 153 each;
//!   - buildings and crown towers are counted and struck; an idle Tesla hidden under ground is neither (FilterHidden);
//!   - the area's own Damage 100, with HitsGround and HitsAir false, never lands (spells.AREA_DAMAGE_WITHOUT_HIT_FLAGS).
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the Void as count tiers of the rows' numbers, and refuses an area whose own Damage hits;
//!   2. a lone crown tower loses 97 on C + 32, C + 56 and C + 80 and nothing else, and a lone Cannon 696 on C + 32;
//!   3. five buildings take 153 each at the first strike; one of them dies of it, and the four left take 294 each at
//!      the second and third;
//!   4. a crown tower beside a Cannon: 51 and 294;
//!   5. a Cannon beside a hidden Tesla is struck alone (696), the Tesla untouched;
//!   6. a Knight that appears inside after the first strike is struck at the second and the third, alone (696 each);
//!   7. a Knight whose centre stands between 2500 and 2500 plus its radius from the centre is struck.
//!
//! Losses are read against a control battle run without the cast wherever a building's lifetime drains it.
//!
//! No measurement on disk: a count of 3, a crown tower among five or more, air and underground units in the count,
//! two Voids on one unit.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test dark_magic`):
//!   * `void_area_damage_loaded` -- the area's own Damage is read onto the strike and dealt at each strike: (1) and (2)
//!     go red.
//!   * `strike_timer_restarts` (tests/strikes.rs's) -- the leftover dropped: the second strike falls on C + 55, so (2)
//!     and (6) go red.
//!   * `void_tier_threshold_shifted` -- a count equal to a limit takes the next tier: (2), (3) and (6) go red.
//!   * `crown_per_hit_ignored` (tests/curse.rs's) -- a crown tower takes the percent route: (2) and (4) go red.
//!   * `void_counts_hidden` -- FilterHidden not read: the Tesla is counted, so the Cannon takes 294 and (5) goes red.
//!   * `selector_centre_in_radius` -- the refuted centre-in-radius reach: (7) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, SpellShape, StrikePick};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, TapSnap};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn centre_distance(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// The shipped config. spells.COUNT_TIER_RULE, SELECTOR_REACH and TARGET_FILTER_ABSENT_FLAG have one arm each (the
/// only() rule), so there is no arm to assert.
fn shipped() -> BattleConfig {
    config()
}

/// The Void's tiers at the Blue side's level: (the pulse on a troop or a building, the crown-tower pulse), in list order
/// (alone, two to four, five and more), from the loaded buffs.
fn tiers(s: &BattleState) -> Vec<(i32, i32)> {
    let db = s.cards();
    let idx = db.index("DarkMagic").expect("DarkMagic loads");
    let level = s.config().card_level[0];
    let spell = db.get(idx).spell.as_ref().expect("DarkMagic is a spell");
    let SpellShape::Strikes(d) = &spell.shape else { panic!("DarkMagic: {:?}", spell.shape) };
    let sel = d.selector.as_ref().expect("the Void's selector");
    sel.buffs
        .iter()
        .map(|b| {
            let def = db.buffs[b.buff as usize];
            let pulse = def.pulse_amount(s.config().calib.buff_pulse_amount, |m| db.scaled(idx, level, m)).expect("a valid level");
            (pulse, db.scaled(idx, level, def.crown_hit).expect("a valid level"))
        })
        .collect()
}

/// `setup` puts the scene down (before the first tick) and returns the ids to watch; the Void is cast by Blue at `tap`
/// (C = k = 0) in one battle and not in a control. Every (k, id index, extra loss) where the cast battle's hp drop on
/// tick k exceeds the control's (a gone entity reads hp 0). Ids agree between the two battles: the cast makes no entity.
fn extra_losses(setup: impl Fn(&mut BattleState) -> Vec<EntityId>, tap: Vec2, ticks: u32) -> (Vec<(u32, usize, i32)>, BattleState) {
    let mut cast = BattleState::new(0, shipped());
    let mut control = BattleState::new(0, shipped());
    let ids = setup(&mut cast);
    assert_eq!(setup(&mut control), ids, "the two battles set up different entities");
    cast.spawn_unit(Team::Blue, "DarkMagic", tap, None).expect("cast DarkMagic");
    let hp = |s: &BattleState, id: EntityId| s.entity(id).map_or(0, |v| v.hp);
    let mut out = Vec::new();
    for k in 0..ticks {
        let before: Vec<(i32, i32)> = ids.iter().map(|&id| (hp(&cast, id), hp(&control, id))).collect();
        cast.tick();
        control.tick();
        for (n, (&id, (b_cast, b_control))) in ids.iter().zip(before).enumerate() {
            let extra = (b_cast - hp(&cast, id)) - (b_control - hp(&control, id));
            if extra != 0 {
                out.push((k, n, extra));
            }
        }
    }
    (out, cast)
}

/// The losses of `n` in `got`, as (k, loss).
fn of(got: &[(u32, usize, i32)], n: usize) -> Vec<(u32, i32)> {
    got.iter().filter(|g| g.1 == n).map(|g| (g.0, g.2)).collect()
}

/// A Red crown tower: (id, position). k 1 is the engine-left princess tower.
fn red_princess(s: &BattleState) -> (EntityId, Vec2) {
    let id = s.tower_ids(Team::Red)[1].expect("the Red left princess tower");
    (id, s.entity(id).unwrap().pos)
}

// ---------------------------------------------------------------------------
// (1)

/// Ok when `card` loads from `text`, else the reason it is refused.
fn refusal(text: &str, card: &str) -> Result<(), String> {
    let db = CardDb::from_json_str(text, CardSource::DerivedJson).map_err(|e| format!("the file does not parse: {e}"))?;
    match db.rejected.iter().find(|(n, _)| n == card) {
        Some((_, why)) => Err(why.clone()),
        None if db.index(card).is_some() => Ok(()),
        None => Err(format!("{card} neither loads nor is refused")),
    }
}

/// The Void's own card row from cards.json, with `edit` applied to its area, as a one-card file.
fn edited_void(edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("cards.json parses");
    let mut row = doc["cards"].as_array().unwrap().iter().find(|c| c["name"] == "DarkMagic").expect("the DarkMagic row").clone();
    edit(&mut row["spell"]["area_effect_object"]);
    serde_json::json!({ "cards": [row] }).to_string()
}

/// Plant: void_area_damage_loaded.
#[test]
fn the_void_loads_as_count_tiers_of_the_rows() {
    let s = BattleState::new(0, shipped());
    let spell = card_stat(&s, "DarkMagic").spell.as_ref().expect("DarkMagic loads as a spell");
    let SpellShape::Strikes(d) = &spell.shape else { panic!("DarkMagic: {:?}", spell.shape) };
    assert_eq!((d.pick, d.life_ms, d.gaps_ms.clone()), (StrikePick::CountTiers, 4000, vec![1500, 1200, 1200]));
    assert_eq!(d.hit.radius, 2500 * K, "DetectionRadius");
    assert_eq!(d.hit.damage, 0, "the area's own Damage 100 is not read: it hits neither ground nor air");
    assert!(d.hit.only_enemies && d.hit.hits_air && d.hit.hits_ground && !d.hit.ignore_buildings, "{:?}", d.hit);
    let sel = d.selector.as_ref().expect("the Void's selector");
    assert_eq!(sel.limits, vec![1, 4], "MaxUnitPerActionList");
    let f = sel.filter;
    assert!(f.skip_hidden && !f.skip_underground && !f.skip_dash_immune && f.skip_untargetable, "ForcedCharacterTargets: {f:?}");
    assert!(f.buildings && f.princess_towers && f.king_tower, "{f:?}");
    let rows: Vec<(i32, i32, i32, i32)> = sel
        .buffs
        .iter()
        .map(|b| {
            let def = s.cards().buffs[b.buff as usize];
            (def.damage_per_second, def.hit_frequency_ms, def.crown_hit, b.time_ms)
        })
        .collect();
    assert_eq!(rows, vec![(2720, 100, 38, 100), (1150, 100, 20, 100), (600, 100, 14, 100)], "the tiers in list order: lv3, lv2, lv1");
    if s.config().card_level[0] == 11 {
        assert_eq!(tiers(&s), vec![(696, 97), (294, 51), (153, 35)], "the measured figures at level 11 (35 not measured)");
    }
    // The shapes it refuses, on the Void's own row.
    assert_eq!(refusal(&edited_void(|_| {}), "DarkMagic"), Ok(()), "the Void's row must load on its own");
    let why = refusal(&edited_void(|a| a["hits_ground"] = serde_json::Value::Bool(true)), "DarkMagic").unwrap_err();
    assert!(why.contains("its own Damage and its strikes"), "an area whose own Damage hits: {why}");
    let why = refusal(&edited_void(|a| a["strike_area"]["max_units_per_list"] = serde_json::json!([4, 1])), "DarkMagic").unwrap_err();
    assert!(why.contains("count limits"), "limits that are not ascending: {why}");
    let why = refusal(&edited_void(|a| a["strike_area"]["filter"]["filter_invisible"] = serde_json::Value::Bool(true)), "DarkMagic").unwrap_err();
    assert!(why.contains("FilterInvisible is not simulated"), "a filter flag the engine does not run: {why}");
}

// ---------------------------------------------------------------------------
// (2)

/// Plants: void_area_damage_loaded, strike_timer_restarts, void_tier_threshold_shifted, crown_per_hit_ignored.
#[test]
fn a_lone_crown_tower_loses_one_tier_on_c_plus_32_56_and_80() {
    let probe = BattleState::new(0, shipped());
    let (tower, pos) = red_princess(&probe);
    let (got, s) = extra_losses(|_| vec![tower], pos, 90);
    let (_, crown_alone) = tiers(&s)[0];
    if s.config().card_level[0] == 11 {
        assert_eq!(crown_alone, 97, "the measured 97 at level 11");
    }
    assert_eq!(of(&got, 0), vec![(32, crown_alone), (56, crown_alone), (80, crown_alone)], "the tower's losses (tick, loss)");
}

/// Plants: void_area_damage_loaded, void_tier_threshold_shifted.
#[test]
fn a_lone_cannon_loses_the_first_tier_on_c_plus_32() {
    let (got, s) = extra_losses(|s| vec![s.scenario_spawn_now(Team::Red, "Cannon", at((9000, 22000)), None).expect("spawn")], at((9000, 22000)), 33);
    let (alone, _) = tiers(&s)[0];
    if s.config().card_level[0] == 11 {
        assert_eq!(alone, 696, "the measured 696 at level 11");
    }
    assert_eq!(of(&got, 0), vec![(32, alone)], "the Cannon's losses (tick, loss)");
}

// ---------------------------------------------------------------------------
// (3)

/// Plant: void_tier_threshold_shifted.
#[test]
fn the_tier_follows_the_count_at_each_strike() {
    let tap = at((9000, 22000));
    let spots = [(9000, 22000), (7400, 22000), (10600, 22000), (9000, 20400)];
    let (got, s) = extra_losses(
        |s| {
            let mut ids: Vec<EntityId> = ["InfernoTower", "Xbow", "BombTower", "Mortar"]
                .iter()
                .zip(spots)
                .map(|(card, p)| s.scenario_spawn_now(Team::Red, card, at(p), None).unwrap_or_else(|e| panic!("spawn {card}: {e:?}")))
                .collect();
            // The fifth, low enough that the first strike kills it.
            ids.push(s.scenario_spawn_now(Team::Red, "Cannon", at((9000, 23600)), Some(100)).expect("spawn Cannon"));
            ids
        },
        tap,
        82,
    );
    let t = tiers(&s);
    let (five, four) = (t[2].0, t[1].0);
    assert!(!s.entities().any(|v| v.card == "Cannon"), "the scene drifted: the low Cannon outlived the first strike");
    for n in 0..4 {
        assert_eq!(of(&got, n), vec![(32, five), (56, four), (80, four)], "building {n}: five inside at the first strike, four after");
    }
    if s.config().card_level[0] == 11 {
        assert_eq!((five, four), (153, 294), "the measured figures at level 11");
    }
}

// ---------------------------------------------------------------------------
// (4)

/// Plants: crown_per_hit_ignored, void_area_damage_loaded.
#[test]
fn a_crown_tower_beside_a_cannon_takes_the_second_tier() {
    let probe = BattleState::new(0, shipped());
    let (tower, pos) = red_princess(&probe);
    let cannon_at = Vec2::new(pos.x, pos.y - 2500 * K);
    let tap = Vec2::new(pos.x, pos.y - 1000 * K);
    let (got, s) = extra_losses(|s| vec![tower, s.scenario_spawn_now(Team::Red, "Cannon", cannon_at, None).expect("spawn")], tap, 33);
    let (two, crown_two) = tiers(&s)[1];
    if s.config().card_level[0] == 11 {
        assert_eq!((two, crown_two), (294, 51), "the measured figures at level 11");
    }
    assert_eq!((of(&got, 0), of(&got, 1)), (vec![(32, crown_two)], vec![(32, two)]), "the tower's and the Cannon's losses");
}

// ---------------------------------------------------------------------------
// (5)

/// Plant: void_counts_hidden.
#[test]
fn a_hidden_tesla_is_neither_counted_nor_struck() {
    let (got, s) = extra_losses(
        |s| {
            vec![
                s.scenario_spawn_now(Team::Red, "Cannon", at((8500, 22000)), None).expect("spawn Cannon"),
                s.scenario_spawn_now(Team::Red, "Tesla", at((9700, 22000)), None).expect("spawn Tesla"),
            ]
        },
        at((9000, 22000)),
        33,
    );
    let tesla = s.entities().find(|v| v.card == "Tesla").expect("the Tesla lives");
    assert!(tesla.hidden, "the scene drifted: the Tesla is not hidden under ground");
    let (alone, _) = tiers(&s)[0];
    assert_eq!(of(&got, 0), vec![(32, alone)], "the Cannon, counted alone");
    assert_eq!(of(&got, 1), vec![], "the hidden Tesla");
}

// ---------------------------------------------------------------------------
// (6)

/// Plants: void_tier_threshold_shifted, strike_timer_restarts.
#[test]
fn a_knight_that_appears_after_the_first_strike_is_struck_by_the_next_two() {
    let tap = at((9000, 22000));
    let mut s = BattleState::new(0, shipped());
    s.spawn_unit(Team::Blue, "DarkMagic", tap, None).expect("cast DarkMagic");
    let mut knight: Option<EntityId> = None;
    let mut losses = Vec::new();
    let mut at78 = None;
    for k in 0..82u32 {
        if k == 41 {
            // Enqueued before tick 41: the Knight appears in its Spawn phase and deploys, standing, for 20 ticks.
            s.spawn_unit(Team::Red, "Knight", tap, None).expect("deploy Knight");
        }
        let before = knight.and_then(|id| s.entity(id)).map(|v| v.hp);
        s.tick();
        if knight.is_none() {
            knight = s.entities().find(|v| v.team == Team::Red && v.card == "Knight").map(|v| v.id);
        }
        if let (Some(b), Some(id)) = (before, knight) {
            let now = s.entity(id).map_or(0, |v| v.hp);
            if now < b {
                losses.push((k, b - now));
            }
        }
        if k == 78 {
            at78 = knight.and_then(|id| s.entity(id)).map(|v| centre_distance(v.pos, tap));
        }
    }
    let r = card_stat(&s, "Knight").collision_radius / K;
    let d = at78.expect("the scene drifted: the Knight is gone by the third strike");
    assert!(d <= (2500 + r) as i64, "the scene drifted: the Knight walked out ({d} from the centre) before the third strike");
    let (alone, _) = tiers(&s)[0];
    assert_eq!(losses, vec![(56, alone), (80, alone)], "the Knight's losses (tick, loss)");
}

// ---------------------------------------------------------------------------
// (7)

/// Plant: selector_centre_in_radius.
#[test]
fn a_knight_inside_by_its_radius_alone_is_struck() {
    let tap = at((9000, 22000));
    // The Knight stands 2800 from the tap, inside a 500-wide band: exact points, placement.TAP_SNAP's old arm, none
    // (the shipped tile-centre snap moves the tap and the Knight).
    let mut cfg = shipped();
    cfg.calib.placement_tap_snap = TapSnap::None;
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(Team::Blue, "DarkMagic", tap, None).expect("cast DarkMagic");
    let mut knight: Option<EntityId> = None;
    let mut lost = None;
    let mut at30 = None;
    for k in 0..33u32 {
        if k == 15 {
            // Deploying from tick 15 to tick 35, it stands where it was put through the first strike on C + 30.
            s.spawn_unit(Team::Red, "Knight", at((9000, 19200)), None).expect("deploy Knight");
        }
        let before = knight.and_then(|id| s.entity(id)).map(|v| v.hp);
        s.tick();
        if knight.is_none() {
            knight = s.entities().find(|v| v.team == Team::Red && v.card == "Knight").map(|v| v.id);
        }
        if k == 30 {
            let centre = s.spells().iter().find_map(|sp| match &sp.motion {
                SpellMotion::Strikes { pos, .. } => Some(*pos),
                _ => None,
            });
            at30 = match (centre, knight.and_then(|id| s.entity(id))) {
                (Some(c), Some(v)) => Some((centre_distance(v.pos, c), v.deploying)),
                _ => None,
            };
        }
        if let (Some(b), Some(id)) = (before, knight) {
            if s.entity(id).map_or(0, |v| v.hp) < b {
                lost = lost.or(Some(k));
            }
        }
    }
    let r = card_stat(&s, "Knight").collision_radius / K;
    let (d, deploying) = at30.expect("the scene drifted: no Knight or no striking area on the first strike");
    assert!(deploying, "the scene drifted: the Knight was not standing in its deploy at the first strike");
    assert!(d > 2500 && d <= (2500 + r) as i64, "the scene drifted: the Knight stands {d} from the centre, not in (2500, {}]", 2500 + r);
    assert_eq!(lost, Some(32), "a Knight {d} from the centre (2500 + r = {}) is struck at the first strike", 2500 + r);
}
