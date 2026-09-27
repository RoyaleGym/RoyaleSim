//! THE THREE MUSKETEERS: a deploy at explicit offsets and an attack selector (card.rs `SummonMemberDef`,
//! `AttackSelectDef`; state.rs `formation_members`, `select_attack`; combat.rs `fire`, `melee_chosen`).
//!
//! THE LAW, measured on client 15.535.29 and on the 16.402 corpus (one group):
//!   - the card puts down three members, each its own row: member 0 the card's, the second and the third their own
//!     records (the second's LoadTime is 650, the others' 700);
//!   - side 0 stands them at (0, +1000), (+1000, -1000), (-1000, -1000) from the tap on every tap column (x 3500 to
//!     14500): the table's SummonCharactersOffsetsX / Y negated (formation.EXPLICIT_OFFSETS_FRAME);
//!   - side 1 on its own right (absolute x 3500) stands them at the rotation of that; on its own left (absolute x
//!     14500) x is mirrored, which the shipped seat-symmetric arm does not do (the ledger marks it refuted there);
//!   - they leave their deploy state 100 ms apart (F + 19, 21, 23 on the corpus);
//!   - a swing's entry is chosen when the swing starts: the bayonet (an instant 314 = 123 x 256 % at level 11, no
//!     projectile) when the target is on the ground and its centre within 1600 + the musketeer's radius + the
//!     target's (2850 on a Giant: the bracket (2469.5, 2864]), else the shot; an air target always takes the shot
//!     (24 of 24 swings); a ground building takes the bayonet.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the card's three members, their offsets, and the selector on all three records;
//!   2. side 0's members at the negated offsets, exactly, on a right-half tap, and the same shape on a left-half tap;
//!   3. side 1: the shipped arm is the rotation on both lanes; the side-1 arm mirrors x on the own-left lane only;
//!   4. the members leave their deploy state 0, 2 and 4 ticks after the first, in list order, each its own record;
//!   5. a Cannon whose centre is exactly at the reach takes the bayonet (314, no shot); one native unit further, the
//!      shot; under centre_distance the reach is 1600;
//!   6. a Minion well inside the reach takes the shot;
//!   7. a Giant walking into the reach during the windup takes the shot under at_swing_start and the bayonet under
//!      at_fire;
//!   8. the entry of a swing under way survives a save and load: the two battles strike alike;
//!   9. the loader takes the selector and the members only in their one shape (synthetic file).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test three_musketeers`):
//!   * `members_share_first_unit` -- every member is the card's own unit: (4) goes red.
//!   * `explicit_offsets_as_ring` -- the members are laid on the ring: (2) and (3) go red.
//!   * `bayonet_never` -- the melee entry is never chosen: (5), (7) and (8) go red.
//!   * `bayonet_on_air` -- the ground clause dropped: (6) goes red.
//!   * `save_drops_attack_seq` -- the entry is lost across a save: (8) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AttackSelectMoment, AttackSelectRange, BattleConfig, BattleState, ExplicitOffsetsFrame, GroundDeployPoint};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn native(p: Vec2) -> (i32, i32) {
    (p.x / K, p.y / K)
}

const CARD: &str = "ThreeMusketeers";
/// Where the Blue musketeer of the strike tests stands: out of every crown tower's reach.
const MUSKETEER_AT: (i32, i32) = (9000, 11000);

// ---------------------------------------------------------------------------
// 1. the loader

#[test]
fn the_loader_reads_three_members_their_offsets_and_the_selector() {
    let s = BattleState::new(0, config());
    let db = s.cards();
    let idx = db.index(CARD).expect("the Three Musketeers load");
    let c = db.get(idx);
    let ms = c.summon_members.as_ref().expect("the card carries its members");
    let got: Vec<(&str, i32, i32)> = ms.iter().map(|m| (db.get(m.unit).name.as_str(), m.offset_x, m.offset_y)).collect();
    assert_eq!(
        got,
        [(CARD, 0, -1000), ("ThreeMusketeer_Rework_Character_2", -1000, 1000), ("ThreeMusketeer_Rework_Character_3", 1000, 1000)],
        "member 0 is the card's own row, the others their own records, at the table's offsets"
    );
    assert_eq!(ms[0].unit, idx);
    assert!(c.summon_offsets_x_mirrored, "CharactersOffsetsXMirrored is read");
    assert!(ms[1..].iter().all(|m| db.get(m.unit).summon_only), "the second and third are released, not played");
    // Each member's own row: the second's LoadTime is its own.
    let loads: Vec<i32> = ms.iter().map(|m| db.get(m.unit).load_time_ms).collect();
    assert_eq!(loads, [700, 650, 700]);
    // The selector on every member: melee 1600, ground only, 123 at level 1 = 314 at level 11.
    for m in ms {
        let sel = db.get(m.unit).attack_select.unwrap_or_else(|| panic!("{} carries no selector", db.get(m.unit).name));
        assert_eq!((sel.melee_range, sel.ground_only, sel.melee_damage), (1600 * K, true, 123));
        assert_eq!(db.scaled(m.unit, 11, sel.melee_damage), Ok(BAYONET_L11), "{}: the bayonet at level 11", db.get(m.unit).name);
        assert!(db.get(m.unit).projectile.is_some(), "the ranged entry is the row's own projectile");
    }
}

// ---------------------------------------------------------------------------
// 2 and 3. the offsets

/// The members' absolute native points for `team` deploying at native `tap`, under `frame`, with the one-unit ground
/// deploy point either shipped (`ground_point`) or off.
fn members(team: Team, tap: (i32, i32), frame: ExplicitOffsetsFrame, ground_point: bool) -> Vec<(String, (i32, i32), i32)> {
    let mut cfg = config();
    cfg.calib.explicit_offsets_frame = frame;
    if !ground_point {
        cfg.calib.formation_ground_deploy_point = GroundDeployPoint::None;
    }
    let s = BattleState::new(0, cfg);
    s.formation_preview(team, CARD, at(tap)).expect("a preview of the Three Musketeers").into_iter().map(|(n, p, ms)| (n, native(p), ms)).collect()
}

/// Members 1 and 2 relative to member 0, absolute native: the shape, free of the one-unit ground point.
fn shape(m: &[(String, (i32, i32), i32)]) -> [(i32, i32); 2] {
    let o = m[0].1;
    [(m[1].1 .0 - o.0, m[1].1 .1 - o.1), (m[2].1 .0 - o.0, m[2].1 .1 - o.1)]
}

/// Plant: explicit_offsets_as_ring.
#[test]
fn side_0_stands_its_members_at_the_negated_offsets() {
    // A right-half tap: the ground deploy point moves nothing for side 0 there, so the members stand on the tap
    // plus the negated offsets exactly.
    let tap = (12500, 11000);
    let got = members(Team::Blue, tap, ExplicitOffsetsFrame::OwnerFrameNegated, true);
    let points: Vec<(i32, i32)> = got.iter().map(|m| m.1).collect();
    assert_eq!(points, [(tap.0, tap.1 + 1000), (tap.0 + 1000, tap.1 - 1000), (tap.0 - 1000, tap.1 - 1000)], "{got:?}");
    // Every tap column: the same shape on the left half and the centre.
    for x in [3500, 9500, 14500] {
        let m = members(Team::Blue, (x, 11000), ExplicitOffsetsFrame::OwnerFrameNegated, true);
        assert_eq!(shape(&m), [(1000, -2000), (-1000, -2000)], "side 0 at x {x}: {m:?}");
    }
}

/// Plant: explicit_offsets_as_ring.
#[test]
fn side_1_is_the_rotation_under_the_shipped_arm_and_mirrors_x_on_its_own_left_under_the_other() {
    // Absolute x 3500 is side 1's own RIGHT lane, absolute x 14500 its own LEFT.
    let right = (3500, 21000);
    let left = (14500, 21000);
    // The shipped arm: the rotation of side 0 on both lanes. On the own right this is the client's layout.
    for tap in [right, left] {
        let m = members(Team::Red, tap, ExplicitOffsetsFrame::OwnerFrameNegated, true);
        assert_eq!(shape(&m), [(-1000, 2000), (1000, 2000)], "side 1 at {tap:?} under the shipped arm: {m:?}");
    }
    // With the ground point off, the shipped arm is exactly the rotation of side 0's deploy at the rotated tap.
    let (w, h) = {
        let s = BattleState::new(0, config());
        (s.arena().width / K, s.arena().height / K)
    };
    let blue = members(Team::Blue, (w - left.0, h - left.1), ExplicitOffsetsFrame::OwnerFrameNegated, false);
    let red = members(Team::Red, left, ExplicitOffsetsFrame::OwnerFrameNegated, false);
    let rotated: Vec<(i32, i32)> = blue.iter().map(|m| (w - m.1 .0, h - m.1 .1)).collect();
    assert_eq!(red.iter().map(|m| m.1).collect::<Vec<_>>(), rotated, "the shipped arm is seat-symmetric");
    // The side-1 arm: the own right unchanged, the own left x-mirrored (the client's layout there).
    let arm = ExplicitOffsetsFrame::OwnerFrameNegatedSide1LeftLaneXMirror;
    assert_eq!(shape(&members(Team::Red, right, arm, true)), [(-1000, 2000), (1000, 2000)]);
    assert_eq!(shape(&members(Team::Red, left, arm, true)), [(1000, 2000), (-1000, 2000)]);
    // Side 0 never mirrors under either arm.
    assert_eq!(shape(&members(Team::Blue, (3500, 11000), arm, true)), [(1000, -2000), (-1000, -2000)]);
}

// ---------------------------------------------------------------------------
// 4. the deploy

/// Plant: members_share_first_unit.
#[test]
fn the_members_leave_their_deploy_state_two_ticks_apart_each_its_own_record() {
    let mut s = BattleState::new(0, config());
    let tick_ms = s.config().calib.tick_ms;
    s.spawn_unit(Team::Blue, CARD, at((9500, 8500)), None).expect("deploy the Three Musketeers");
    let mut ends: Vec<(EntityId, String, u32)> = Vec::new();
    let mut seen: Vec<(EntityId, String, u32)> = Vec::new();
    for _ in 0..60 {
        s.tick();
        let mut live: Vec<_> = s.entities().filter(|v| v.team == Team::Blue && v.card.starts_with("ThreeMusketeer")).collect();
        live.sort_by_key(|v| v.team_seq);
        for v in live {
            if !seen.iter().any(|x| x.0 == v.id) {
                seen.push((v.id, v.card.to_string(), s.tick_count()));
            }
            if !v.deploying && !ends.iter().any(|x| x.0 == v.id) {
                ends.push((v.id, v.card.to_string(), s.tick_count()));
            }
        }
    }
    let names: Vec<&str> = seen.iter().map(|x| x.1.as_str()).collect();
    assert_eq!(names, [CARD, "ThreeMusketeer_Rework_Character_2", "ThreeMusketeer_Rework_Character_3"], "creation order");
    let order: Vec<EntityId> = seen.iter().map(|x| x.0).collect();
    let mut by_member: Vec<u32> = order.iter().map(|id| ends.iter().find(|e| e.0 == *id).expect("every member leaves its deploy state").2).collect();
    let first = by_member[0];
    by_member.iter_mut().for_each(|t| *t -= first);
    let step = (100 / tick_ms) as u32;
    assert_eq!(by_member, [0, step, 2 * step], "SummonDeployDelay 100 between members (F + 19, 21, 23 on the corpus)");
}

// ---------------------------------------------------------------------------
// 5 to 8. the selector

/// The bayonet at level 11, measured on client 15.535.29 (123 x 256 %).
const BAYONET_L11: i32 = 314;

/// What the first swing of a Blue musketeer at a red `target` standing `d` native north of it did.
#[derive(Debug)]
struct FirstStrike {
    /// Ticks from the set-down to the tick the swing landed: the target lost at least a bayonet, or a projectile the
    /// musketeer fired at it appeared. A crown tower's bolt is neither (its hit is smaller, its firer another card).
    k: u32,
    /// The target's hp lost on that tick; its own drain is at most `drain` of it.
    dropped: i32,
    drain: i32,
    /// A projectile the musketeer fired at the target stands after that tick.
    shot: bool,
    /// The entry the swing ran (`attack_entry`) on the tick before it landed.
    entry: Option<u8>,
    /// The target's centre distance from the musketeer when the swing started and when it landed, native.
    d_start: i64,
    d_hit: i64,
}

fn centre_distance(s: &BattleState, a: EntityId, b: EntityId) -> i64 {
    let (p, q) = (s.entity(a).expect("alive").pos, s.entity(b).expect("alive").pos);
    let (dx, dy) = ((p.x - q.x) as i64, (p.y - q.y) as i64);
    royalesim::fixed::isqrt(dx * dx + dy * dy) / K as i64
}

fn first_strike(cfg: BattleConfig, target: &str, d: i32) -> FirstStrike {
    let mut s = BattleState::new(0, cfg);
    let m = s.scenario_spawn_now(Team::Blue, CARD, at(MUSKETEER_AT), None).expect("set the musketeer down");
    let t = s.scenario_spawn_now(Team::Red, target, at((MUSKETEER_AT.0, MUSKETEER_AT.1 + d)), None).expect("set the target down");
    let firer = s.entity(m).expect("the musketeer stands").card_idx;
    let mut entry = None;
    let mut d_start = None;
    for k in 1..80 {
        let before = s.entity(t).expect("the target stands").hp;
        let drain = drain_step(&s, t);
        let dist = centre_distance(&s, m, t);
        s.tick();
        let v = s.entity(m).expect("the musketeer stands");
        if d_start.is_none() && v.attack_phase != royalesim::entity::AttackPhase::Idle {
            d_start = Some(dist);
        }
        // A target gone counts its whole hp: under a defect that stabs a Minion, the stab kills it.
        let dropped = before - s.entity(t).map_or(0, |v| v.hp);
        let shot = s.projectiles().iter().any(|p| p.firer_card == Some(firer) && p.target == t);
        if dropped >= BAYONET_L11 || shot {
            return FirstStrike { k, dropped, drain, shot, entry, d_start: d_start.unwrap_or(-1), d_hit: dist };
        }
        assert!(s.entity(t).is_some(), "the {target} at {d} died to something that was neither the bayonet nor a shot");
        entry = s.attack_entry(m);
    }
    panic!("the musketeer never struck the {target} at {d}");
}

/// The selector's reach on `target` under the shipped range reading, native: 1600 + both radii, read off the
/// loaded rows.
fn reach(target: &str) -> i32 {
    let s = BattleState::new(0, config());
    let sel = card_stat(&s, CARD).attack_select.expect("the selector");
    (sel.melee_range + card_stat(&s, CARD).collision_radius + card_stat(&s, target).collision_radius) / K
}

/// Plant: bayonet_never.
#[test]
fn a_ground_building_at_the_reach_takes_the_bayonet_and_one_unit_further_the_shot() {
    // The reach reading the client's bracket admits: 2850 on a Giant, inside (2469.5, 2864].
    let giant = reach("Giant");
    assert!(2469 < giant && giant <= 2864, "1600 + both radii on a Giant is {giant}, outside the measured bracket");
    let r = reach("Cannon");
    let at_edge = first_strike(config(), "Cannon", r);
    assert_eq!(at_edge.entry, Some(1), "the swing at the reach runs the melee entry: {at_edge:?}");
    assert!(!at_edge.shot, "the bayonet launches nothing: {at_edge:?}");
    assert!(at_edge.dropped >= BAYONET_L11 && at_edge.dropped <= BAYONET_L11 + at_edge.drain, "the bayonet deals its level-11 hit: {at_edge:?}");
    let beyond = first_strike(config(), "Cannon", r + 1);
    assert_eq!(beyond.entry, Some(0), "one native unit beyond the reach runs the shot: {beyond:?}");
    assert!(beyond.shot, "the shot is a projectile: {beyond:?}");
    // centre_distance: the reach is the variable alone.
    let mut cfg = config();
    cfg.calib.attack_select_range = AttackSelectRange::CentreDistance;
    let inside = first_strike(cfg.clone(), "Cannon", 1600);
    assert_eq!((inside.entry, inside.shot), (Some(1), false), "{inside:?}");
    let outside = first_strike(cfg, "Cannon", 1601);
    assert_eq!((outside.entry, outside.shot), (Some(0), true), "{outside:?}");
}

/// Plant: bayonet_on_air.
#[test]
fn an_air_target_inside_the_reach_takes_the_shot() {
    let got = first_strike(config(), "Minions", 1500);
    assert!(got.d_hit < reach("Minions") as i64, "the Minion must stand inside the melee reach: {got:?}");
    assert_eq!(got.entry, Some(0), "an air target runs the shot: {got:?}");
    assert!(got.shot, "{got:?}");
}

/// Plant: bayonet_never (the at_fire half).
#[test]
fn the_entry_is_chosen_when_the_swing_starts() {
    // A Giant 100 beyond the reach walks toward the Blue towers; during the windup it crosses into the reach.
    let r = reach("Giant");
    let shipped = first_strike(config(), "Giant", r + 100);
    assert!(shipped.d_start > r as i64 && shipped.d_hit <= r as i64, "the scene must cross the reach within the windup: {shipped:?} (reach {r})");
    assert_eq!((shipped.entry, shipped.shot), (Some(0), true), "at_swing_start: the shot the swing began with: {shipped:?}");
    let mut cfg = config();
    cfg.calib.attack_select_moment = AttackSelectMoment::AtFire;
    let at_fire = first_strike(cfg, "Giant", r + 100);
    assert!(!at_fire.shot && at_fire.dropped >= BAYONET_L11, "at_fire: the bayonet at the hit: {at_fire:?}");
    assert_eq!(at_fire.k, shipped.k, "the arms part on the entry, not on the swing's timing");
}

/// Plants: save_drops_attack_seq, bayonet_never.
#[test]
fn a_swing_under_way_keeps_its_entry_across_a_save() {
    let mut s = BattleState::new(0, config());
    let m = s.scenario_spawn_now(Team::Blue, CARD, at(MUSKETEER_AT), None).expect("set the musketeer down");
    let t = s.scenario_spawn_now(Team::Red, "Cannon", at((MUSKETEER_AT.0, MUSKETEER_AT.1 + 2000)), None).expect("set the Cannon down");
    let n = run_until(&mut s, 40, |s| s.attack_entry(m) == Some(1) && s.entity(m).is_some_and(|v| v.attack_phase != royalesim::entity::AttackPhase::Idle));
    assert!(n < 40, "the musketeer never began a bayonet swing");
    s.tick();
    s.tick();
    let hp = s.entity(t).expect("the Cannon stands").hp;
    let mut b = BattleState::load(&s.save()).expect("the save loads");
    assert_eq!(b.attack_entry(m), Some(1), "the entry of the swing under way is state");
    assert_eq!(b.state_hash(), s.state_hash());
    let mut struck = false;
    for _ in 0..30 {
        s.tick();
        b.tick();
        assert_eq!(b.state_hash(), s.state_hash(), "tick {}: the loaded battle parts from the one it was saved from", s.tick_count());
        let now = s.entity(t).expect("the Cannon stands").hp;
        struck |= hp - now >= BAYONET_L11;
    }
    assert!(struck, "the swing never landed its bayonet within 30 ticks of the save");
    // The column is hashed: a save edited only in it fails the snapshot's self-check.
    let bytes = s.save();
    let mut v: serde_json::Value = serde_json::from_slice(&bytes).expect("a snapshot is JSON");
    let col = v["ents"]["attack_seq"].as_array_mut().expect("the snapshot carries the entry column");
    let i = m.index as usize;
    let e = col[i].as_u64().expect("an entry");
    col[i] = serde_json::Value::from(1 - e);
    let edited = serde_json::to_vec(&v).unwrap();
    assert!(BattleState::load(&edited).is_err(), "two states differing only in a swing's entry hash alike");
}

// ---------------------------------------------------------------------------
// 9. the loader's one shape

/// A synthetic trio: card Trio (member 0, row Trio_1), units TrioB and TrioC, with the selector block on the card.
fn trio() -> serde_json::Value {
    let row = |name: &str| {
        serde_json::json!({ "name": name, "rarity": "Common", "hitpoints": 300, "hit_speed_ms": 1300, "load_time_ms": 700,
            "range_milli": 6000, "collision_radius_milli": 500, "projectile": {"name": "TrioShot", "speed": 1000, "damage": 80} })
    };
    let mut card = row("Trio");
    let o = card.as_object_mut().unwrap();
    o.insert("kind".into(), "troop".into());
    o.insert("elixir".into(), 9.into());
    o.insert("rarity".into(), "Rare".into());
    o.insert("count".into(), 3.into());
    o.insert("summon_character".into(), "Trio_1".into());
    o.insert(
        "action_graph".into(),
        serde_json::json!({"class_types": ["ActionDealDamage", "ActionFilter", "ActionPlayEffect", "ActionRunOnInstigator", "ActionSetAttackSequenceIndex"], "spawns": [], "mechanic": true}),
    );
    o.insert("attack_select".into(), serde_json::json!({"melee_range_milli": 1600, "melee_ground_only": true, "melee_damage": 123, "melee_index": 1, "ranged_index": 0}));
    o.insert(
        "summon_members".into(),
        serde_json::json!([
            {"character": "Trio_1", "offset_x_milli": 0, "offset_y_milli": -1000},
            {"character": "TrioB", "offset_x_milli": -1000, "offset_y_milli": 1000},
            {"character": "TrioC", "offset_x_milli": 1000, "offset_y_milli": 1000}
        ]),
    );
    o.insert("summon_offsets_x_mirrored".into(), true.into());
    serde_json::json!({ "version": "test", "cards": [card], "units": { "TrioB": row("TrioB"), "TrioC": row("TrioC") } })
}

fn load(v: &serde_json::Value) -> CardDb {
    CardDb::from_json_str(&v.to_string(), CardSource::DerivedJson).expect("the synthetic file parses")
}

fn refusal(v: &serde_json::Value) -> String {
    let db = load(v);
    assert!(db.index("Trio").is_none(), "Trio loads: {v}");
    db.rejected.iter().find(|(n, _)| n == "Trio").map(|(_, w)| w.clone()).expect("Trio is listed as rejected")
}

#[test]
fn the_loader_takes_the_selector_and_the_members_only_in_their_one_shape() {
    let good = trio();
    let db = load(&good);
    let i = db.index("Trio").unwrap_or_else(|| panic!("the trio is refused: {:?}", db.rejected));
    let ms = db.get(i).summon_members.clone().expect("members");
    assert_eq!(ms[0].unit, i, "member 0 is the card itself");
    assert_eq!([db.get(ms[1].unit).name.as_str(), db.get(ms[2].unit).name.as_str()], ["TrioB", "TrioC"]);
    assert!(db.get(i).attack_select.is_some());
    let edit = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut v = trio();
        f(&mut v["cards"][0]);
        v
    };
    for (what, v, want) in [
        ("no graph", edit(&|c| c["action_graph"] = serde_json::Value::Null), "attack selector"),
        ("a graph that spawns", edit(&|c| c["action_graph"]["class_types"].as_array_mut().unwrap().push("ActionSpawn".into())), "attack selector"),
        ("a graph without the damage", edit(&|c| c["action_graph"]["class_types"].as_array_mut().unwrap().retain(|x| x != "ActionDealDamage")), "attack selector"),
        ("the entries swapped", edit(&|c| c["attack_select"]["ranged_index"] = 1.into()), "attack selector"),
        ("a melee entry on air", edit(&|c| c["attack_select"]["melee_ground_only"] = false.into()), "attack selector"),
        ("no projectile", edit(&|c| c["projectile"] = serde_json::Value::Null), "attack selector"),
        ("two members for three", edit(&|c| c["summon_members"].as_array_mut().unwrap().truncate(2)), "summon_members"),
        ("member 0 not the card's row", edit(&|c| c["summon_members"][0]["character"] = "TrioB".into()), "summon_members[0]"),
        ("a blank offset", edit(&|c| c["summon_members"][2]["offset_y_milli"] = serde_json::Value::Null), "summon_members[2]"),
        ("a line beside the members", edit(&|c| c["summon_width_milli"] = 1000.into()), "summon_members"),
    ] {
        let why = refusal(&v);
        assert!(why.contains(want), "{what}: refused for {why:?}, not the named block");
    }
}
