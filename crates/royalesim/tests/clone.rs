//! THE CLONE (card.rs `SpellShape::Clone`, `clone_shape`, `CloneRules`; spell.rs `step_spells`, the Clone arm; state.rs
//! `apply_effects`, the hold, `materialise_clones`, `make_copy`, `phase_reap`, a copy's death spawns).
//!
//! THE LAW, measured on client 15.535.29 (32 runs, 25 copies, level 11 unless said), C the cast tick (k = 0 below, the
//! first tick run after the play):
//!   - the area acts once, on C (spells.ONE_SHOT_AREA_EFFECT_APPLICATION = first_update_only): every own troop whose
//!     edge is within its 3000 (spells.AOE_HIT_TEST) gets a copy on C; not an enemy, not a building, never a copy
//!     (CLONE_CLONED_UNITS FALSE), not a row that sets IgnoreClone; it deals nothing;
//!   - the copy: on the original's spot after its own step on C, hp 1 of 1 and a shield of 1 where its row has one
//!     (spells.CLONE_HITPOINTS), at the Clone's level (spells.CLONE_LEVEL = spell_level), fresh: no target until
//!     C + 11, empty attack timers, no charge (spells.CLONE_COPY_STATE = fresh);
//!   - the pair: both held C + 1 to C + 10 (spells.CLONE_HOLD_TARGETS = both), the original keeping its target, its
//!     attack progress (paused, not reset) and its charge; on each of those ticks the original moves 125 toward the
//!     enemy along its owner's y axis and the copy 125 the other way, 2500 apart on C + 10 (spells.CLONE_OFFSET);
//!   - a copy's death spawns are copies (spells.CLONE_DEATH_SPAWNS = cloned_at_clone_hitpoints): a copied Battle Ram
//!     killed by a Zap released two Barbarians of 1 hitpoint each.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the Clone as its action with the table's rules, reads the GlobalClone event's own Buff as its
//!      copies' hold (item 294), and refuses the rules the engine does not run and a rule the file lacks;
//!   2. on C, one copy of each own troop inside and none of the one just outside, the building or the enemy, each on
//!      its original's spot at hp 1 of 1;
//!   3. the pair slides 125 a tick apart for 10 ticks along the owner's y axis, for either side;
//!   4. a charged Prince keeps its charge through the hold, and its copy starts uncharged;
//!   5. in a fight, the original keeps its target and its paused attack progress through the hold, the enemy keeps its
//!      lock on it, and the copy takes no target before C + 11 and its own on C + 11;
//!   6. a second Clone over a pair copies the original alone;
//!   7. the copy takes the Clone's level under spell_level and the original's under original_level;
//!   8. a copied Guard carries a shield of 1: it survives one Zap and dies to the second;
//!   9. a copied Battle Ram's death spawns are two copies of 1 hitpoint;
//!  10. a unit that appears in the circle after C is not copied;
//!  11. a row that sets IgnoreClone is not copied (the chess Recruits: their card and unit rows have different names);
//!  12. a copy is state: a save edited only in its flag fails the load's hash self-check, and an unedited save taken
//!      during the slide resumes it hash for hash;
//!  13. the copy takes its original's buffs under copied_except_not_cloned and none but the hold under none (not
//!      measured: the one staging was confounded; the key is a hypothesis);
//!  14. the sliding pair is a body in its neighbours' contact scans: an enemy the original slides into is pushed off it,
//!      and the original stays on its slide's points (measured on client 15.535.29, sp-m5-clone-s0: an enemy Giant whose
//!      look circle met a sliding Skeleton on t818 turned and was pushed off it, the Skeleton on its points);
//!  15. spells.CLONE_OFFSET = client15535_column_edge_slide: each of the pair walks 125 a tick toward the centre of the
//!      edge cell of its own 500 column (the original the enemy's edge row, the copy its own side's), the step fixed on
//!      C (the law read from the original's point on C, the copy stepping off the axis), for Blue; Red the y-reflection
//!      (measured on client 15.535.29, 35 pairs, all side 0: 692 of 692 slide steps; ub-cl2-band C125 the copy (3, -124));
//!  16. spells.CLONE_HOLD_DEPLOY = client15535_covers_deploy: a Knight put down (deploying) 2 ticks before the cast, a
//!      Red Giant held 1,900 north of it: under hold_time the original, still deploying and no longer held, is pushed off
//!      the Giant from C + 11; under the new arm it stands on its slide's last point through C + 14, held for its deploy
//!      left (measured on client 15.535.29, sp-m5-clone-s0: three deploying Skeletons and their copies held to C + 14);
//!  17. spells.CLONE_HOLD_DEPLOY = client15535_covers_deploy_late_walk: the same scene; the original takes its target a
//!      tick after it would under client15535_covers_deploy, and is first pushed off the Giant a tick after that, where
//!      client15535_covers_deploy pushes it on its target's tick; its attack clock runs from its target's tick (measured on
//!      client 15.535.29, sp-m5-clone-s0: targets on C + 16, first steps on C + 17, first hits on C + 25 where
//!      client15535_covers_deploy's land on C + 24).
//!
//! THE SCENE. Blue's own half, the tap at (9000, 9000), where the slide stays on open ground and no crown tower is on
//! the pair's line. Units are put down already deployed unless said, so they walk from tick 0.
//!
//! No measurement on disk: air units, buildings and deploying units as originals, a copy's kamikaze, death damage and
//! death bomb, the slide against the river, a building or the arena edge, the copy's exact shield, anything on client
//! 16.402.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test clone`):
//!   * `clone_rules_unchecked` -- CLONE_LEVEL_OFFSET not held to 0: (1) goes red.
//!   * `clone_full_hp` -- a copy keeps its row's hitpoints and shield: (2), (8) and (9) go red.
//!   * `aoe_centre_to_centre` (tests/spells.rs's) -- the reach by the centre: (2) goes red (the Skeleton inside by its
//!     edge alone is not copied).
//!   * `clone_no_separation` -- the pair stands on one spot: (3) goes red.
//!   * `clone_hold_resets` -- the hold lands as a stun, with the stun's resets: (4) goes red.
//!   * `clone_copy_inherits_target` -- the copy takes its original's target: (5) goes red.
//!   * `clone_recloned` -- a copy is copied again: (6) goes red.
//!   * `clone_level_from_original` -- the copy takes the original's level under every arm: (7) goes red.
//!   * `clone_death_spawns_ordinary` -- a copy's death spawns are ordinary units: (9) goes red.
//!   * `clone_area_lingers` -- the area acts on every update: (2) and (10) go red.
//!   * `ignore_clone_unread` -- IgnoreClone not read: (11) goes red.
//!   * `clone_hash_skips_flag` -- the copy's flag not hashed: (12) goes red.
//!   * `clone_buffs_not_copied` -- no buff copied under either arm: (13) goes red.
//!   * `clone_slide_hidden` -- the sliding pair is out of its neighbours' scans: (14) goes red.
//!   * `clone_slide_on_axis` -- the new CLONE_OFFSET arm slides along the owner's y axis: (15) goes red.
//!   * `clone_hold_ends_in_deploy` -- the new CLONE_HOLD_DEPLOY arm holds the pair the hold's time alone: (16) goes red.
//!   * `clone_late_walk_unread` -- client15535_covers_deploy_late_walk holds as client15535_covers_deploy: (17) goes red.
//!   * `clone_walk_with_targets` -- the late-walk pair walks on its targets' tick: (17) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, SpellPlacement, SpellShape};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, CloneCopyBuffs, CloneDeathSpawns, CloneHoldDeploy, CloneLevel, CloneOffset, EntityView};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The tap of every Blue scene (THE SCENE in the header).
const TAP: (i32, i32) = (9000, 9000);

fn centre_distance(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// The shipped config, asserting the arms this file pins.
fn shipped() -> BattleConfig {
    let c = config();
    assert_eq!(c.calib.clone_level, CloneLevel::SpellLevel, "the shipped spells.CLONE_LEVEL this file pins");
    assert_eq!(c.calib.clone_copy_buffs, CloneCopyBuffs::CopiedExceptNotCloned, "the shipped spells.CLONE_COPY_BUFFS");
    assert_eq!(c.calib.clone_death_spawns, CloneDeathSpawns::ClonedAtCloneHitpoints, "the shipped spells.CLONE_DEATH_SPAWNS");
    assert_eq!(c.calib.clone_hitpoints, (1, 1), "the shipped spells.CLONE_HITPOINTS: 1 hitpoint, a shield of 1");
    c
}

/// The copies on the board, in team_seq order.
fn copies(s: &BattleState) -> Vec<EntityView<'_>> {
    let mut v: Vec<EntityView> = s.entities().filter(|e| e.cloned).collect();
    v.sort_by_key(|e| (e.team as u8, e.team_seq));
    v
}

/// Entity `id`'s unified level, read from a save (the entity view does not carry it).
fn level_of(s: &BattleState, id: EntityId) -> i64 {
    let v: serde_json::Value = serde_json::from_slice(&s.save()).expect("a snapshot is JSON");
    v["ents"]["level"][id.index as usize].as_i64().expect("the level column")
}

// ---------------------------------------------------------------------------
// (1)

/// A synthetic Clone: its area and action as the extractor writes them, with `globals` as the table's globals.
fn synthetic(spawn_type: &str, globals: &serde_json::Value) -> String {
    serde_json::json!({
        "cards": [{
            "name": "C", "kind": "spell", "elixir": 3, "rarity": "Epic",
            "spell": {"can_place_on_buildings": true, "can_deploy_on_enemy_side": true,
                "area_effect_object": {"name": "CA", "life_duration_ms": 1000, "radius_milli": 3000, "only_enemies": false,
                    "only_own_troops": true, "hits_ground": true, "hits_air": true, "ignore_buildings": true, "clone": true,
                    "clone_action": {"on_cloned": {"spawn_type": spawn_type, "spawn": "Clone", "spawn_time_ms": 500,
                        "buff": {"name": "Clone", "speed_multiplier_raw": -100, "hit_speed_multiplier_raw": -100,
                            "spawn_speed_multiplier_raw": -100, "clone": true}}}}}
        }],
        "globals": globals
    })
    .to_string()
}

/// The 15.535.29 CLONE_* globals, from cards.json.
fn table_globals() -> serde_json::Value {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("cards.json parses");
    let g = doc["globals"].as_object().expect("cards.json carries its globals");
    serde_json::Value::Object(g.iter().filter(|(k, _)| k.starts_with("CLONE_")).map(|(k, v)| (k.clone(), v.clone())).collect())
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

/// Plant: clone_rules_unchecked.
#[test]
fn the_clone_loads_as_its_action_with_the_tables_rules() {
    let s = BattleState::new(0, shipped());
    let spell = card_stat(&s, "Clone").spell.as_ref().expect("Clone loads as a spell");
    assert_eq!(spell.placement, SpellPlacement::Anywhere);
    let SpellShape::Clone { hit, hold, rules } = &spell.shape else { panic!("Clone: {:?}", spell.shape) };
    assert_eq!((hit.radius, hit.damage), (3000 * K, 0), "the area's Radius; no damage");
    assert!(hit.only_own_troops && !hit.only_enemies && hit.ignore_buildings && hit.hits_air && hit.hits_ground, "{hit:?}");
    let def = s.cards().buffs[hold.buff as usize];
    assert!(def.clone_hold, "the hold is the Clone buff");
    assert_eq!((def.speed_pct, def.hit_speed_pct, def.spawn_speed_pct, hold.time_ms), (-100, -100, -100, 500));
    assert_eq!(rules.distance_y, 250, "CLONE_DISTANCE_Y");
    assert!(rules.preserve_shield && !rules.reset_target && !rules.reset_charge && rules.death_spawns, "{rules:?}");
    // The GlobalClone event (item 294): the Clone's shape with Radius 30000, its own Buff read as the copies' hold.
    let g_spell = card_stat(&s, "GlobalClone").spell.as_ref().expect("GlobalClone loads as a spell");
    let SpellShape::Clone { hit: g_hit, hold: g_hold, .. } = &g_spell.shape else { panic!("GlobalClone: {:?}", g_spell.shape) };
    assert_eq!(g_hit.radius, 30000 * K, "the event's Radius");
    assert_eq!(g_hit.buff.map(|b| (b.buff, b.time_ms)), Some((g_hold.buff, g_hold.time_ms)), "its own Buff is the copies' hold");
    // The synthetic copy loads with the table's rules, and the rules the engine does not run are refused.
    let g = table_globals();
    assert_eq!(g.as_object().map(|m| m.len()), Some(11), "the eleven CLONE_* globals: {g}");
    assert_eq!(refusal(&synthetic("BuffType", &g), "C"), Ok(()), "the synthetic Clone must load");
    let with = |k: &str, v: serde_json::Value| {
        let mut g = g.clone();
        g[k] = v;
        g
    };
    let why = refusal(&synthetic("BuffType", &with("CLONE_LEVEL_OFFSET", 1.into())), "C").unwrap_err();
    assert!(why.contains("Clone rules: CLONE_LEVEL_OFFSET 1 is not simulated"), "{why}");
    let why = refusal(&synthetic("BuffType", &with("CLONE_DISTANCE_X", 300.into())), "C").unwrap_err();
    assert!(why.contains("Clone rules: CLONE_DISTANCE_X 300 is not simulated"), "{why}");
    let why = refusal(&synthetic("BuffType", &with("CLONE_CLONED_UNITS", true.into())), "C").unwrap_err();
    assert!(why.contains("Clone rules: CLONE_CLONED_UNITS TRUE is not simulated"), "{why}");
    let mut missing = g.clone();
    missing.as_object_mut().unwrap().remove("CLONE_RESET_CHARGE");
    let why = refusal(&synthetic("BuffType", &missing), "C").unwrap_err();
    assert!(why.contains("Clone rules missing from cards.json: CLONE_RESET_CHARGE"), "{why}");
    let why = refusal(&synthetic("AreaEffectType", &g), "C").unwrap_err();
    assert!(why.contains("not a buff"), "an OnClonedAction that spawns an area: {why}");
}

// ---------------------------------------------------------------------------
// (2)

/// Plants: clone_full_hp, aoe_centre_to_centre, clone_area_lingers.
#[test]
fn each_own_troop_inside_is_copied_once_on_the_cast_tick() {
    let mut s = BattleState::new(0, shipped());
    let r = card_stat(&s, "Skeletons").collision_radius / K;
    // A Skeleton 150 inside the edge and one 150 outside it: its step on C (at most 90) keeps each on its side.
    let (tx, ty) = TAP;
    let ids = s
        .scenario_spawn_batch(&[
            (Team::Blue, "Knight", at(TAP), None),
            (Team::Blue, "Skeletons", at((tx + 3000 + r - 150, ty)), None),
            (Team::Blue, "Skeletons", at((tx - (3000 + r + 150), ty)), None),
            (Team::Blue, "Cannon", at((tx, ty + 1500)), None),
            (Team::Red, "Knight", at((tx, ty - 1500)), None),
        ])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    s.spawn_unit(Team::Blue, "Clone", at(TAP), None).expect("cast Clone");
    s.tick();
    // The reach is read on the positions after the step on C.
    let edge = |id: EntityId| centre_distance(s.entity(id).unwrap().pos, at(TAP)) - (s.entity(id).unwrap().radius / K) as i64;
    assert!(edge(ids[1]) <= 3000 && edge(ids[2]) > 3000, "the scene drifted: the Skeletons' edges on C are {} and {}", edge(ids[1]), edge(ids[2]));
    let got = copies(&s);
    let want: Vec<(Team, &str, Vec2)> = vec![(Team::Blue, "Knight", s.entity(ids[0]).unwrap().pos), (Team::Blue, "Skeletons", s.entity(ids[1]).unwrap().pos)];
    let seen: Vec<(Team, &str, Vec2)> = got.iter().map(|c| (c.team, c.card, c.pos)).collect();
    assert_eq!(seen, want, "the copies on C: the Knight's and the inner Skeleton's, each on its original's spot");
    for c in &got {
        assert_eq!((c.hp, c.max_hp, c.shield), (1, 1, 0), "a copy of {} on C", c.card);
        assert!(!c.deploying, "a copy appears deployed");
    }
    // Nothing more, later. Every copy ever seen is counted: the enemy Knight may kill one of these two.
    let mut seen: Vec<EntityId> = got.iter().map(|c| c.id).collect();
    for _ in 0..12 {
        s.tick();
        for c in copies(&s) {
            if !seen.contains(&c.id) {
                seen.push(c.id);
            }
        }
    }
    assert_eq!(seen.len(), 2, "the area acted after C");
}

// ---------------------------------------------------------------------------
// (3)

/// The original and its copy's positions after each of ticks 0 to 11, a Clone cast on a lone Knight of `team` at `p`.
fn slide(team: Team, p: Vec2) -> Vec<(Vec2, Vec2)> {
    slide_in(shipped(), team, p)
}

/// `slide` under the config `cfg`.
fn slide_in(cfg: BattleConfig, team: Team, p: Vec2) -> Vec<(Vec2, Vec2)> {
    let mut s = BattleState::new(0, cfg);
    let k = s.scenario_spawn_now(team, "Knight", p, None).expect("spawn Knight");
    s.spawn_unit(team, "Clone", p, None).expect("cast Clone");
    let mut out = Vec::new();
    for _ in 0..12 {
        s.tick();
        let c = copies(&s);
        assert_eq!(c.len(), 1, "one copy");
        out.push((s.entity(k).expect("the original lives").pos, c[0].pos));
    }
    out
}

/// Plant: clone_no_separation.
#[test]
fn the_pair_slides_125_a_tick_apart_for_ten_ticks_along_the_owner_axis() {
    for (team, fwd) in [(Team::Blue, 1), (Team::Red, -1)] {
        let p = if team == Team::Blue { at(TAP) } else { at((18000 - TAP.0, 32000 - TAP.1)) };
        let got = slide(team, p);
        let (o0, c0) = got[0];
        assert_eq!(o0, c0, "{team:?}: the copy appears on its original's spot on C");
        for (k, &(o, c)) in got.iter().enumerate().take(11).skip(1) {
            let d = 125 * k as i32 * K;
            assert_eq!(o, Vec2::new(o0.x, o0.y + fwd * d), "{team:?}: the original on C + {k}");
            assert_eq!(c, Vec2::new(o0.x, o0.y - fwd * d), "{team:?}: the copy on C + {k}");
        }
        let (o, c) = got[10];
        assert_eq!(centre_distance(o, c), 2500, "{team:?}: 2500 apart on C + 10");
    }
}

// ---------------------------------------------------------------------------
// (15)

/// The client's slide step (client15535_column_edge_slide) from `p` (native) toward the centre of the cell at `edge_y` in
/// p's own 500 column: the heading d * 256 / isqrt(|d|^2) per axis, truncated, times 125 / 256, truncated.
fn edge_step(p: (i32, i32), edge_y: i32) -> (i32, i32) {
    let (dx, dy) = ((p.0 / 500 * 500 + 250 - p.0) as i64, (edge_y - p.1) as i64);
    let l = isqrt(dx * dx + dy * dy);
    let h = (dx * 256 / l, dy * 256 / l);
    ((h.0 * 125 / 256) as i32, (h.1 * 125 / 256) as i32)
}

/// Plant: clone_slide_on_axis.
#[test]
fn under_client15535_column_edge_slide_each_of_the_pair_walks_toward_its_columns_edge_cell() {
    for (team, fwd) in [(Team::Blue, 1), (Team::Red, -1)] {
        let p = if team == Team::Blue { at(TAP) } else { at((18000 - TAP.0, 32000 - TAP.1)) };
        let mut cfg = shipped();
        cfg.calib.clone_offset = CloneOffset::Client15535ColumnEdgeSlide;
        let got = slide_in(cfg, team, p);
        let (o0, c0) = got[0];
        assert_eq!(o0, c0, "{team:?}: the copy appears on its original's spot on C");
        let n = (o0.x / K, o0.y / K);
        let (ahead, behind) = if fwd > 0 { (31750, 250) } else { (250, 31750) };
        let (so, sc) = (edge_step(n, ahead), edge_step(n, behind));
        // NOT VACUOUS: the copy's step is not the axis slide's.
        assert_ne!(sc, (0, -125 * fwd), "{team:?}: the scene drifted: the copy's edge step is the axis slide's");
        for (k, &(o, c)) in got.iter().enumerate().take(11).skip(1) {
            let k = k as i32;
            assert_eq!(o, Vec2::new(o0.x + so.0 * k * K, o0.y + so.1 * k * K), "{team:?}: the original on C + {k}");
            assert_eq!(c, Vec2::new(o0.x + sc.0 * k * K, o0.y + sc.1 * k * K), "{team:?}: the copy on C + {k}");
        }
    }
}

// ---------------------------------------------------------------------------
// (16)

/// The original's points on C to C + 15 under `arm`: a Blue Knight put down deploying at the tap 2 ticks before the cast,
/// a Red Giant held 1,900 north of the tap (the slide brings the original into it from C + 6, as in (14)).
fn deploying_original(arm: CloneHoldDeploy) -> Vec<Vec2> {
    let giant_at = at((TAP.0, TAP.1 + 1900));
    let mut cfg = shipped();
    cfg.calib.clone_hold_deploy = arm;
    let mut s = BattleState::new(0, cfg);
    let g = s.scenario_spawn_now(Team::Red, "Giant", giant_at, None).expect("spawn Giant");
    s.spawn_unit(Team::Blue, "Knight", at(TAP), None).expect("put down the Knight");
    for _ in 0..2 {
        s.debug_set_pos(g, giant_at);
        s.tick();
    }
    let k = s.entities().find(|e| e.team == Team::Blue && e.card == "Knight").map(|e| e.id).expect("the Knight");
    assert!(s.entity(k).expect("the Knight").deploy_ms > 750, "{arm:?}: the scene drifted: the Knight's deploy left is not past the hold and C + 14");
    s.spawn_unit(Team::Blue, "Clone", at(TAP), None).expect("cast Clone");
    (0..16)
        .map(|_| {
            s.debug_set_pos(g, giant_at);
            s.tick();
            s.entity(k).expect("the original lives").pos
        })
        .collect()
}

/// Plant: clone_hold_ends_in_deploy.
#[test]
fn under_client15535_covers_deploy_a_deploying_pair_is_held_through_its_deploy() {
    let old = deploying_original(CloneHoldDeploy::HoldTime);
    // NOT VACUOUS: the engine's arm lets the deploying original be pushed off the Giant once the hold is over.
    assert_ne!(old[11], old[10], "hold_time: the deploying original was not pushed on C + 11 ({:?} on C + 10)", old[10]);
    let new = deploying_original(CloneHoldDeploy::Client15535CoversDeploy);
    assert_eq!(&new[..=10], &old[..=10], "client15535_covers_deploy: the slide itself moved");
    for (k, p) in new.iter().enumerate().take(15).skip(11) {
        assert_eq!(*p, new[10], "client15535_covers_deploy: the original moved on C + {k} while its deploy holds it");
    }
}

// ---------------------------------------------------------------------------
// (17)

/// The original's (point, target, attack progress) on C to C + 23 under `arm`, in (16)'s scene.
fn deploying_pair(arm: CloneHoldDeploy) -> Vec<(Vec2, Option<royalesim::EntityId>, i32)> {
    let giant_at = at((TAP.0, TAP.1 + 1900));
    let mut cfg = shipped();
    cfg.calib.clone_hold_deploy = arm;
    let mut s = BattleState::new(0, cfg);
    let g = s.scenario_spawn_now(Team::Red, "Giant", giant_at, None).expect("spawn Giant");
    s.spawn_unit(Team::Blue, "Knight", at(TAP), None).expect("put down the Knight");
    for _ in 0..2 {
        s.debug_set_pos(g, giant_at);
        s.tick();
    }
    let k = s.entities().find(|e| e.team == Team::Blue && e.card == "Knight").map(|e| e.id).expect("the Knight");
    s.spawn_unit(Team::Blue, "Clone", at(TAP), None).expect("cast Clone");
    (0..24)
        .map(|_| {
            s.debug_set_pos(g, giant_at);
            s.tick();
            let e = s.entity(k).expect("the original lives");
            (e.pos, e.target, e.attack_ms)
        })
        .collect()
}

/// The first tick (from C) with a target, and the first after the slide (C + 11 on) whose point moved.
fn first_target_and_move(p: &[(Vec2, Option<royalesim::EntityId>, i32)]) -> (usize, usize) {
    let t = p.iter().position(|(_, t, _)| t.is_some()).expect("the original never took a target");
    let m = (11..p.len()).find(|&k| p[k].0 != p[k - 1].0).expect("the original never moved after the slide");
    (t, m)
}

/// Plants: clone_late_walk_unread, clone_walk_with_targets.
#[test]
fn under_client15535_covers_deploy_late_walk_a_deploying_pair_targets_a_tick_late_and_walks_a_tick_after() {
    let old = deploying_pair(CloneHoldDeploy::Client15535CoversDeploy);
    let (t_old, m_old) = first_target_and_move(&old);
    // NOT VACUOUS: under client15535_covers_deploy the original is pushed off the Giant on the tick it takes its target.
    assert_eq!(m_old, t_old, "client15535_covers_deploy: the original took its target on C + {t_old} and first moved on C + {m_old}");
    let new = deploying_pair(CloneHoldDeploy::Client15535CoversDeployLateWalk);
    let (t_new, m_new) = first_target_and_move(&new);
    assert_eq!(&new[..t_old], &old[..t_old], "client15535_covers_deploy_late_walk: the slide or the hold moved");
    assert_eq!(t_new, t_old + 1, "client15535_covers_deploy_late_walk: the target came on C + {t_new}, not C + {}", t_old + 1);
    assert_eq!(m_new, t_new + 1, "client15535_covers_deploy_late_walk: the first move came on C + {m_new}, not C + {}", t_new + 1);
    // its attack clock runs from its target's tick (the Giant in its reach there), its walk's wait aside
    assert!(new[t_new].2 > 0, "client15535_covers_deploy_late_walk: no attack progress on its target's tick C + {t_new}: {:?}", &new[t_new - 1..=t_new + 1]);
}

// ---------------------------------------------------------------------------
// (14)

/// Plant: clone_slide_hidden. A Red Giant (it targets buildings, so it walks and runs its contact update) held 1,900 north
/// of a Blue Knight; the Clone's original slides north into it from C + 6 (1,900 - 125 x 6 < 750 + 500).
#[test]
fn the_sliding_pair_pushes_its_neighbours_and_is_not_pushed() {
    let giant_at = at((TAP.0, TAP.1 + 1900));
    let mut s = BattleState::new(0, shipped());
    assert_eq!(s.config().calib.knock_duration_ms, 0, "the shipped knockback.DURATION_MS: no knockback slides");
    let k = s.scenario_spawn_now(Team::Blue, "Knight", at(TAP), None).expect("spawn Knight");
    let g = s.scenario_spawn_now(Team::Red, "Giant", giant_at, None).expect("spawn Giant");
    s.spawn_unit(Team::Blue, "Clone", at(TAP), None).expect("cast Clone");
    let mut o0 = None;
    for step in 0..11 {
        s.debug_set_pos(g, giant_at);
        s.tick();
        let o = s.entity(k).expect("the original lives");
        let gv = s.entity(g).expect("the Giant lives");
        let o0 = *o0.get_or_insert(o.pos);
        assert_eq!(o.pos, Vec2::new(o0.x, o0.y + 125 * step * K), "the original on its slide's point on C + {step}: contact does not move it");
        let overlap = centre_distance(o.pos, giant_at) < (o.radius / K + gv.radius / K) as i64;
        if (6..=10).contains(&step) {
            assert!(overlap, "the scene drifted: the original does not reach the Giant on C + {step}");
        }
        // from C + 7 the original overlaps the Giant at the Giant's update whether its slide steps before the move
        // pass or after it
        if (7..=10).contains(&step) {
            assert!(gv.push_neighbours >= 1 && gv.push_applied.y > 0, "the Giant is pushed off the sliding original on C + {step}: {:?} from {}", gv.push_applied, gv.push_neighbours);
        }
    }
}

// ---------------------------------------------------------------------------
// (4)

/// Plant: clone_hold_resets.
#[test]
fn a_charged_prince_keeps_its_charge_and_its_copy_starts_uncharged() {
    let mut s = BattleState::new(0, shipped());
    let prince = s.scenario_spawn_now(Team::Blue, "Prince", at((9000, 7500)), None).expect("spawn Prince");
    let charged_by = run_until(&mut s, 80, |s| s.entity(prince).is_some_and(|v| v.charged));
    assert!(charged_by < 80, "the scene drifted: the Prince never charged");
    let p = s.entity(prince).unwrap().pos;
    s.spawn_unit(Team::Blue, "Clone", p, None).expect("cast Clone");
    for k in 0..13u32 {
        s.tick();
        assert!(s.entity(prince).is_some_and(|v| v.charged), "the original lost its charge on C + {k}");
        let c = copies(&s);
        assert_eq!(c.len(), 1, "one copy on C + {k}");
        assert!(!c[0].charged, "the copy is charged on C + {k}");
    }
}

// ---------------------------------------------------------------------------
// (5)

/// Plant: clone_copy_inherits_target.
#[test]
fn in_a_fight_the_original_keeps_its_target_and_the_copy_starts_fresh() {
    let mut s = BattleState::new(0, shipped());
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Knight", at((9000, 10000)), None), (Team::Red, "Knight", at((9000, 11100)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (blue, red) = (ids[0], ids[1]);
    for _ in 0..30 {
        s.tick();
    }
    let b = s.entity(blue).expect("the Blue Knight lives");
    assert_eq!(b.target, Some(red), "the scene drifted: the Blue Knight is not on the Red one before the cast");
    assert_eq!(s.entity(red).map(|v| v.target), Some(Some(blue)), "the scene drifted: the Red Knight is not on the Blue one");
    s.spawn_unit(Team::Blue, "Clone", b.pos, None).expect("cast Clone");
    let mut paused = None;
    for k in 0..12u32 {
        s.tick();
        let o = s.entity(blue).expect("the original lives");
        let r = s.entity(red).expect("the Red Knight lives");
        let c = copies(&s);
        assert_eq!(c.len(), 1, "one copy on C + {k}");
        assert_eq!(o.target, Some(red), "the original's target on C + {k}");
        assert_eq!(r.target, Some(blue), "the enemy's lock on C + {k}");
        match k {
            0 => paused = Some(o.attack_ms),
            1..=10 => assert_eq!(Some(o.attack_ms), paused, "the original's attack progress on C + {k}: paused"),
            _ => {}
        }
        if k <= 10 {
            assert_eq!(c[0].target, None, "the copy's target on C + {k}");
            assert_eq!((c[0].attack_ms, c[0].attack_load_ms), (0, 0), "the copy's attack timers on C + {k}");
        } else {
            assert_eq!(c[0].target, Some(red), "the copy takes its own target on C + 11");
        }
    }
}

// ---------------------------------------------------------------------------
// (6)

/// Plant: clone_recloned.
#[test]
fn a_second_clone_over_a_pair_copies_the_original_alone() {
    let mut s = BattleState::new(0, shipped());
    let original = s.scenario_spawn_now(Team::Blue, "Knight", at(TAP), None).expect("spawn Knight");
    s.spawn_unit(Team::Blue, "Clone", at(TAP), None).expect("cast Clone");
    for _ in 0..12 {
        s.tick();
    }
    let first = copies(&s);
    assert_eq!(first.len(), 1, "one copy after the first Clone");
    let first = first[0].id;
    for id in [original, first] {
        let d = centre_distance(s.entity(id).expect("both live").pos, at(TAP));
        assert!(d < 3000, "the scene drifted: a unit of the pair stands {d} from the second tap");
    }
    s.spawn_unit(Team::Blue, "Clone", at(TAP), None).expect("cast a second Clone");
    s.tick();
    let knights = s.entities().filter(|v| v.team == Team::Blue && v.card == "Knight").count();
    let got = copies(&s);
    assert_eq!((knights, got.len()), (3, 2), "one new copy");
    let second = got.iter().find(|c| c.id != first).expect("the second copy");
    assert_eq!(second.pos, s.entity(original).unwrap().pos, "the second copy is the original's");
}

// ---------------------------------------------------------------------------
// (7)

/// A level-12 Knight, then a level-11 Clone on it: the copy's level.
fn copy_level(cfg: BattleConfig) -> i64 {
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(Team::Blue, "Knight", at(TAP), Some(12)).expect("deploy a level-12 Knight");
    for _ in 0..25 {
        s.tick();
    }
    let k = s.entities().find(|v| v.team == Team::Blue && v.card == "Knight").map(|v| (v.id, v.pos)).expect("the Knight");
    assert_eq!(level_of(&s, k.0), 12, "the scene drifted: the Knight is not level 12");
    s.spawn_unit(Team::Blue, "Clone", k.1, Some(11)).expect("cast a level-11 Clone");
    s.tick();
    let c = copies(&s);
    assert_eq!(c.len(), 1, "one copy");
    level_of(&s, c[0].id)
}

/// Plant: clone_level_from_original.
#[test]
fn the_copy_takes_the_clones_level() {
    assert_eq!(copy_level(shipped()), 11, "spell_level: a level-11 Clone on a level-12 Knight makes a level-11 copy");
    let mut c = config();
    c.calib.clone_level = CloneLevel::OriginalLevel;
    assert_eq!(copy_level(c), 12, "original_level");
}

// ---------------------------------------------------------------------------
// (8)

/// Plant: clone_full_hp.
#[test]
fn a_copied_guard_survives_one_zap_and_dies_to_the_second() {
    let mut s = BattleState::new(0, shipped());
    let guard = s.scenario_spawn_now(Team::Blue, "SkeletonWarriors", at(TAP), None).expect("spawn a Guard");
    assert!(s.entity(guard).unwrap().shield > 0, "the scene drifted: the Guard has no shield");
    s.spawn_unit(Team::Blue, "Clone", at(TAP), None).expect("cast Clone");
    for _ in 0..12 {
        s.tick();
    }
    let c = copies(&s);
    assert_eq!(c.len(), 1, "one copy");
    let (copy, pos) = (c[0].id, c[0].pos);
    assert_eq!((c[0].hp, c[0].max_hp, c[0].shield), (1, 1, 1), "a copied Guard: hp 1 of 1 and a shield of 1");
    s.spawn_unit(Team::Red, "Zap", pos, None).expect("cast a Red Zap");
    s.tick();
    let v = s.entity(copy).expect("the copy survived the first Zap: its shield took it");
    assert_eq!((v.hp, v.shield), (1, 0), "after the first Zap");
    let pos = v.pos;
    s.spawn_unit(Team::Red, "Zap", pos, None).expect("cast a second Red Zap");
    s.tick();
    assert!(s.entity(copy).is_none(), "the copy survived the second Zap");
}

// ---------------------------------------------------------------------------
// (9)

/// Plant: clone_death_spawns_ordinary.
#[test]
fn a_copied_battle_rams_death_spawns_are_copies() {
    let mut s = BattleState::new(0, shipped());
    let ram = s.scenario_spawn_now(Team::Blue, "BattleRam", at(TAP), None).expect("spawn a Battle Ram");
    s.spawn_unit(Team::Blue, "Clone", at(TAP), None).expect("cast Clone");
    for _ in 0..3 {
        s.tick();
    }
    let c = copies(&s);
    assert_eq!(c.len(), 1, "one copy");
    let (copy, pos) = (c[0].id, c[0].pos);
    // A Red Zap on the copy on C + 3, as measured: the copy dies of it, the original does not.
    s.spawn_unit(Team::Red, "Zap", pos, None).expect("cast a Red Zap");
    s.tick();
    assert!(s.entity(copy).is_none(), "the scene drifted: the copy outlived the Zap");
    assert!(s.entity(ram).is_some(), "the scene drifted: the original died");
    let spawns: Vec<(i32, i32, bool)> = s
        .entities()
        .filter(|v| v.team == Team::Blue && v.card != "BattleRam" && v.kind == royalesim::entity::EntityKind::Troop)
        .map(|v| (v.hp, v.max_hp, v.cloned))
        .collect();
    assert_eq!(spawns, vec![(1, 1, true), (1, 1, true)], "the copy's death spawns");
}

// ---------------------------------------------------------------------------
// (10)

/// Plant: clone_area_lingers.
#[test]
fn a_unit_that_appears_after_the_cast_is_not_copied() {
    let mut s = BattleState::new(0, shipped());
    s.scenario_spawn_now(Team::Blue, "Knight", at(TAP), None).expect("spawn Knight");
    s.spawn_unit(Team::Blue, "Clone", at(TAP), None).expect("cast Clone");
    s.tick();
    // Enqueued after C: it appears on C + 1, deploying, in the middle of the circle.
    s.spawn_unit(Team::Blue, "Knight", at((TAP.0, TAP.1 + 500)), None).expect("deploy a second Knight");
    for _ in 0..12 {
        s.tick();
    }
    let knights = s.entities().filter(|v| v.team == Team::Blue && v.card == "Knight").count();
    assert_eq!((knights, copies(&s).len()), (3, 1), "the first Knight's copy alone");
}

// ---------------------------------------------------------------------------
// (11)

/// Plant: ignore_clone_unread.
#[test]
fn a_row_that_sets_ignore_clone_is_not_copied() {
    let mut s = BattleState::new(0, shipped());
    for (card, flag) in [("RoyalRecruits_Chess", true), ("GoblinDrill", true), ("Knight", false)] {
        assert_eq!(card_stat(&s, card).ignore_clone, flag, "{card}'s IgnoreClone");
    }
    let (tx, ty) = TAP;
    s.scenario_spawn_now(Team::Blue, "RoyalRecruits_Chess", at(TAP), None).expect("spawn a chess Recruit");
    s.scenario_spawn_now(Team::Blue, "Knight", at((tx + 1000, ty)), None).expect("spawn Knight");
    s.spawn_unit(Team::Blue, "Clone", at(TAP), None).expect("cast Clone");
    s.tick();
    let got: Vec<&str> = copies(&s).iter().map(|c| c.card).collect();
    assert_eq!(got, vec!["Knight"], "the Knight is copied, the chess Recruit is not");
}

// ---------------------------------------------------------------------------
// (12)

/// Plant: clone_hash_skips_flag.
#[test]
fn a_copy_is_state() {
    let mut s = BattleState::new(0, shipped());
    s.scenario_spawn_now(Team::Blue, "Knight", at(TAP), None).expect("spawn Knight");
    s.spawn_unit(Team::Blue, "Clone", at(TAP), None).expect("cast Clone");
    for _ in 0..4 {
        s.tick();
    }
    let copy = copies(&s)[0].id.index as usize;
    let hashed = edit_is_hashed(&s, |v| {
        assert_eq!(v["ents"]["cloned"][copy], serde_json::Value::Bool(true), "the copy's flag is saved");
        v["ents"]["cloned"][copy] = serde_json::Value::Bool(false);
    });
    assert!(hashed, "a save edited only in a copy's flag loads under the old hash: the flag is not hashed");
    // Saved during the slide, the battle resumes it hash for hash.
    let mut loaded = BattleState::load(&s.save()).expect("the save loads");
    for k in 0..12 {
        s.tick();
        loaded.tick();
        assert_eq!(loaded.state_hash(), s.state_hash(), "the loaded battle parts from the saved one on tick {k} after the save");
    }
}

// ---------------------------------------------------------------------------
// (13)

/// A raged Knight, then a Clone on it: (the original's buff rows before the cast, the copy's after C).
fn raged_copy(cfg: BattleConfig) -> (Vec<u16>, Vec<u16>) {
    let mut s = BattleState::new(0, cfg);
    let k = s.scenario_spawn_now(Team::Blue, "Knight", at(TAP), None).expect("spawn Knight");
    s.spawn_unit(Team::Blue, "Rage", at(TAP), None).expect("cast Rage");
    let ids = |s: &BattleState, id: EntityId| -> Vec<u16> { s.entity(id).map_or(Vec::new(), |v| v.buffs.iter().filter(|b| !b.is_empty()).map(|b| b.id).collect()) };
    let raged = run_until(&mut s, 40, |s| !ids(s, k).is_empty());
    assert!(raged < 40, "the scene drifted: the Rage never reached the Knight");
    let before = ids(&s, k);
    let p = s.entity(k).unwrap().pos;
    s.spawn_unit(Team::Blue, "Clone", p, None).expect("cast Clone");
    s.tick();
    let c = copies(&s);
    assert_eq!(c.len(), 1, "one copy");
    (before, ids(&s, c[0].id))
}

/// Plant: clone_buffs_not_copied.
#[test]
fn the_copy_takes_its_originals_buffs_under_each_arm() {
    let (before, copy) = raged_copy(shipped());
    assert!(before.iter().all(|b| copy.contains(b)), "copied_except_not_cloned: the copy carries {copy:?}, the original carried {before:?}");
    let mut c = config();
    c.calib.clone_copy_buffs = CloneCopyBuffs::None;
    let (before, copy) = raged_copy(c);
    assert!(before.iter().all(|b| !copy.contains(b)), "none: the copy carries {copy:?}, the original carried {before:?}");
    assert_eq!(copy.len(), 1, "none: the copy carries the hold alone: {copy:?}");
}
