//! THE SUMMON FORMATION AND THE DEPLOY STAGGER: where a card's N summons stand
//! around the tap and when each leaves its deploy state -- formation.rs (the
//! layout measured on the live 16.402 corpus), state.rs `formation_members` /
//! `ground_y_range`, card.rs `FormationDef`, calibration formation.LAYOUT /
//! DEPLOY_STAGGER / GROUND_Y_CLAMP.
//!
//! WHAT IS PINNED, every number read from the loaded CardDb, `Calib::shipped()`, the
//! arena or the committed measurement (never pasted):
//!   1. the sine table formation.rs carries is round(sin x 1024) on every degree,
//!      re-derived by an integer-only series;
//!   2. EVERY clean multi-unit deploy group of the live 16.402 corpus
//!      (tests/fixtures/formations/measured.json, tools/make_formation_fixture.py:
//!      16 cards, both seats, both lanes, 52 groups, the towers the game still had)
//!      is reproduced member by member and in creation order -- exactly (within 3
//!      native) on every member that overlaps no sibling and no tower at spawn,
//!      within the contact law's reach on those that do -- and every member's
//!      deploy END tick is the group's first member's plus the engine's own stagger
//!      (or one frame later: a capture's skipped frame); a group the game centred
//!      away from the placement log's tap (a deploy snapped off a footprint, a touch
//!      off the tile centre) is matched up to that one shift, re-previewed through
//!      the engine, and counted; the one card on the unmodelled offsets-table
//!      layout (ThreeMusketeers, which the loader refuses anyway) is asserted NOT to
//!      match, so modelling it moves it out of the exception list;
//!   3. the engine's members leave `deploying` on spawn + (DeployTime + k x
//!      SummonDeployDelay) / TICK_MS, member by member (Goblins: 200 ms steps),
//!      and a second summon's members on the second delay (Rascals);
//!   4. a Goblin Gang deploys 3 + 3 (the Spear Goblins as the summon-only unit on the
//!      card's level), the Rascals 1 + 2, on the one hexagon / square the fixture
//!      measures;
//!   5. the ring is seat-symmetric under `symmetric_config`: Red's members at the
//!      rotated tap are the rotations of Blue's, in the same order, with the same
//!      timers, own-left, centre column and own-right, on the bank and on the back
//!      row; and the SHIPPED ground clamp is the measured per-side formula, which is
//!      not (Red's rear pair on its back row's near edge, one native unit tighter
//!      at the river), the own-frame arm restoring the rotation;
//!   6. the lane classifier is "left of the centre column" on every cell of the
//!      shipped map and swaps under the rotation;
//!   7. the ground clamp holds a bank deploy's forward Skeleton on the bank row and
//!      the bounds clamp holds a back-row Bat 250 native inside the edge; the `none`
//!      arm ejects instead;
//!   8. the engine_grid arm is the old square grid, the `none` stagger arm one tick
//!      for all: every candidate moves a behaviour;
//!   9. a whole scripted battle with swarms in both decks keeps every tests/common
//!      invariant (no member ever on water).
//!
//! PLANT (regression): `formation_grid_legacy` forces the square grid on every
//! deploy: (2), (4), both halves of (7) and (8) go red -- 5 of 10.
//!     RUSTFLAGS='--cfg clash_plant="formation_grid_legacy"' CARGO_TARGET_DIR=target/plant cargo test --test formations

mod common;

use common::*;
use royalesim::formation::{member_offset, nearest_lane, sin1024, Layout, LANE_LEFT, LANE_RIGHT, SIN_1024};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE};
use royalesim::state::{BattleState, DeployStagger, FormationLayout, GroundYClamp};
use royalesim::Team;
use std::collections::BTreeMap;

const K: i32 = SUBTILE_PER_MILLITILE;
const FIXTURE: &str = include_str!("fixtures/formations/measured.json");
/// Cards whose corpus formation the engine is KNOWN not to reproduce: the
/// SummonCharactersList offsets-table layout formation.rs does not model (the Three
/// Musketeers, which the loader refuses on its action graph anyway).
const NOT_MODELLED: &[&str] = &["ThreeMusketeers"];
/// A member that overlaps no sibling at spawn must land within this many native
/// units of the measurement (the placement log's tap tile and the game's centre
/// differ by one on some deploys; nothing else may).
const EXACT_NATIVE: i64 = 3;
/// A member that overlaps a sibling at spawn is moved by the contact law before
/// its first frame (the Minions' 577-ring at CollisionRadius 500 by 2, the Skeleton
/// Army's inner spiral by up to ~300).
const PUSHED_NATIVE: i64 = 400;

#[derive(serde::Deserialize)]
struct Fixture {
    groups: Vec<Group>,
}

#[derive(serde::Deserialize, Clone)]
struct Group {
    fixture: String,
    tick: u32,
    side: u8,
    card: String,
    source: String,
    tap: [i32; 2],
    /// Crown towers (side, slot) already destroyed on the group's tick.
    towers_down: Vec<[u8; 2]>,
    members: Vec<Member>,
}

#[derive(serde::Deserialize, Clone)]
struct Member {
    offset: [i32; 2],
    stagger: i32,
}

fn fixture() -> Fixture {
    let f: Fixture = serde_json::from_str(FIXTURE).expect("measured.json parses");
    assert!(f.groups.len() >= 20, "vacuous fixture: {} groups", f.groups.len());
    f
}

fn team_of(side: u8) -> Team {
    if side == 0 {
        Team::Blue
    } else {
        Team::Red
    }
}

fn native(p: Vec2) -> (i64, i64) {
    ((p.x / K) as i64, (p.y / K) as i64)
}

fn dist(a: (i64, i64), b: (i64, i64)) -> i64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    // integer hypot, rounded down: fine for a tolerance test
    isqrt(dx * dx + dy * dy)
}

fn isqrt(v: i64) -> i64 {
    // Newton's integer square root (no floats, in tests either).
    if v <= 0 {
        return 0;
    }
    let mut x = v;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + v / x) / 2;
    }
    x
}

/// The engine's own members for a fixture group: (unit card, absolute point,
/// deploy timer ms) at the group's tap (plus `shift`, native), on a board holding
/// only the towers the game still had on that tick.
fn preview(group: &Group, shift: (i64, i64)) -> Vec<(String, Vec2, i32)> {
    try_preview(group, shift).unwrap_or_else(|e| panic!("{}: {e:?}", group.card))
}

fn try_preview(group: &Group, shift: (i64, i64)) -> Result<Vec<(String, Vec2, i32)>, royalesim::state::DeployError> {
    let mut s = BattleState::new(1, config());
    for [side, slot] in &group.towers_down {
        s.scenario_set_tower_hp(team_of(*side), *slot as usize, 0).expect("a princess tower can start destroyed");
    }
    let pos = Vec2::new((group.tap[0] as i64 + shift.0) as i32 * K, (group.tap[1] as i64 + shift.1) as i32 * K);
    s.formation_preview(team_of(group.side), &group.card, pos)
}

// ---------------------------------------------------------------------------
// 1. the sine table

#[test]
fn sine_table_is_round_sin_times_1024() {
    // sin(x) by its series in Q60 fixed point, i128 all the way (no floats: the
    // rule holds for tests too). pi x 2^60 as an integer.
    const PI_Q60: i128 = 3_622_009_729_038_561_280;
    let sin_q60 = |deg: i128| -> i128 {
        let x = deg * PI_Q60 / 180;
        let x2 = (x * x) >> 60;
        let mut term = x;
        let mut total = x;
        for k in 1..12i128 {
            term = -((term * x2) >> 60) / ((2 * k) * (2 * k + 1));
            total += term;
        }
        total
    };
    for d in 0..=90i128 {
        let want = ((sin_q60(d) * 1024 + (1i128 << 59)) >> 60) as i32;
        assert_eq!(SIN_1024[d as usize] as i32, want, "table[{d}]");
        assert_eq!(sin1024(d as i32), want);
    }
    // The fold: every quadrant from the one table.
    for d in -720i32..720 {
        let a = d.rem_euclid(360);
        let want = match a {
            0..=90 => SIN_1024[a as usize] as i32,
            91..=180 => SIN_1024[(180 - a) as usize] as i32,
            181..=270 => -(SIN_1024[(a - 180) as usize] as i32),
            _ => -(SIN_1024[(360 - a) as usize] as i32),
        };
        assert_eq!(sin1024(d), want, "sin1024({d})");
    }
}

// ---------------------------------------------------------------------------
// 2. the corpus

#[test]
fn every_measured_corpus_formation_is_reproduced_member_by_member() {
    let f = fixture();
    let db = cards();
    let calib = royalesim::state::Calib::shipped();
    let mut checked = 0usize;
    let mut shifted: Vec<String> = Vec::new();
    let mut exact_members = 0usize;
    let mut cards_seen = std::collections::BTreeSet::new();
    let mut failures: Vec<String> = Vec::new();
    for g in &f.groups {
        let label = format!("{} side {} {} at {:?} ({}, {} t{})", g.card, g.side, g.source, g.tap, g.fixture, g.source, g.tick);
        if NOT_MODELLED.contains(&g.card.as_str()) {
            // The unmodelled offsets-table layout: the card is refused by the loader,
            // or its ring must NOT match the corpus -- else this exception is stale.
            if let Ok(got) = try_preview(g, (0, 0)) {
                let worst = got.iter().zip(&g.members).map(|((_, p, _), m)| dist(native(*p), (g.tap[0] as i64 + m.offset[0] as i64, g.tap[1] as i64 + m.offset[1] as i64))).max().unwrap_or(0);
                assert!(worst > EXACT_NATIVE, "{label}: matches the corpus ({worst} native): move it out of NOT_MODELLED");
            }
            continue;
        }
        let got = preview(g, (0, 0));
        assert_eq!(got.len(), g.members.len(), "{label}: member count");
        cards_seen.insert(g.card.clone());
        // Which engine members overlap a sibling or an alive crown tower at spawn
        // (the contact law moves those before the truth's first frame).
        let radius_of = |name: &str| db.get(db.index(name).unwrap()).collision_radius as i64 / K as i64;
        let towers: Vec<((i64, i64), i64)> = {
            let mut s = BattleState::new(1, config());
            for [side, slot] in &g.towers_down {
                s.scenario_set_tower_hp(team_of(*side), *slot as usize, 0).unwrap();
            }
            s.entities().filter(|e| matches!(e.kind, royalesim::entity::EntityKind::KingTower | royalesim::entity::EntityKind::PrincessTower)).map(|e| (native(e.pos), e.radius as i64 / K as i64)).collect()
        };
        let overlaps: Vec<bool> = (0..got.len())
            .map(|i| {
                let me = native(got[i].1);
                let sibling = (0..got.len()).any(|j| j != i && dist(me, native(got[j].1)) < radius_of(&got[i].0) + radius_of(&got[j].0));
                let tower = towers.iter().any(|(c, r)| (me.0 - c.0).abs() < *r && (me.1 - c.1).abs() < *r);
                sibling || tower
            })
            .collect();
        let measured: Vec<(i64, i64)> = g.members.iter().map(|m| (g.tap[0] as i64 + m.offset[0] as i64, g.tap[1] as i64 + m.offset[1] as i64)).collect();
        let errors: Vec<i64> = got.iter().zip(&measured).map(|((_, p, _), m)| dist(native(*p), *m)).collect();
        let tol = |i: usize| if overlaps[i] { PUSHED_NATIVE } else { EXACT_NATIVE };
        let ok = errors.iter().enumerate().all(|(i, e)| *e <= tol(i));
        if !ok {
            // A tap the game centred elsewhere (snapped off a footprint, or a touch
            // that did not land on the tile centre): ONE shift of the tap, at least
            // 100 native, re-previewed through the engine (so the clamps apply at
            // the shifted point), must explain every member.
            let mut candidates: Vec<(i64, i64)> = got.iter().zip(&measured).map(|((_, p, _), m)| (m.0 - native(*p).0, m.1 - native(*p).1)).collect();
            candidates.sort();
            candidates.dedup();
            let found = candidates.iter().copied().filter(|c| c.0.abs() >= 100 || c.1.abs() >= 100).find(|&c| {
                let again = preview(g, c);
                again.iter().zip(&measured).enumerate().all(|(i, ((_, p, _), m))| dist(native(*p), *m) <= tol(i))
            });
            if let Some(c) = found {
                shifted.push(format!("{label}: the game centred it {c:?} off the log's tap"));
                checked += 1;
                continue;
            }
            failures.push(format!(
                "{label}\n    measured {:?}\n    engine   {:?}\n    errors   {errors:?} (overlaps {overlaps:?})",
                measured,
                got.iter().map(|(_, p, _)| native(*p)).collect::<Vec<_>>()
            ));
            continue;
        }
        exact_members += errors.iter().enumerate().filter(|(i, e)| !overlaps[*i] && **e <= EXACT_NATIVE).count();
        // The stagger: each member's deploy END relative to the first member's. A
        // capture skips frames, so a transition can be SEEN one tick late (never
        // early): the measured gap is the engine's or one more.
        let t0 = g.members[0].stagger;
        for (k, (m, (_, _, ms))) in g.members.iter().zip(&got).enumerate() {
            let want = (ms - got[0].2) / calib.tick_ms;
            let seen = m.stagger - t0;
            assert!(seen == want || seen == want + 1, "{label}: member {k} deploy end {} vs first {t0}: engine stagger {} ms", m.stagger, ms - got[0].2);
        }
        checked += 1;
    }
    println!("formations: {checked} groups reproduced ({} exact members), {} tap-shifted: {:#?}", exact_members, shifted.len(), shifted);
    assert!(failures.is_empty(), "{} groups off the game's layout:\n{}", failures.len(), failures.join("\n"));
    assert!(cards_seen.len() >= 12, "only {} cards covered: {cards_seen:?}", cards_seen.len());
    assert!(exact_members >= 120, "only {exact_members} exactly placed members: not evidence");
    assert!(shifted.len() * 4 <= checked, "{} of {checked} groups needed a tap shift", shifted.len());
}

// ---------------------------------------------------------------------------
// 3. the stagger in the engine's own tick loop

fn deploy_end_ticks(card: &str, pos: Vec2) -> (Vec<(String, u32)>, u32) {
    let mut s = BattleState::new(3, config());
    let spawn_call = s.tick_count();
    s.spawn_unit(Team::Blue, card, pos, None).unwrap();
    let mut ends: BTreeMap<(u32, u32), (String, u32)> = BTreeMap::new();
    let mut order: Vec<(u32, u32)> = Vec::new();
    for _ in 0..200 {
        s.tick();
        let mut live: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && !matches!(e.kind, royalesim::entity::EntityKind::KingTower | royalesim::entity::EntityKind::PrincessTower)).collect();
        live.sort_by_key(|e| e.team_seq);
        for e in live {
            let key = (e.id.index, e.id.generation);
            if !order.contains(&key) {
                order.push(key);
            }
            if !e.deploying {
                ends.entry(key).or_insert((e.card.to_string(), s.tick_count()));
            }
        }
        if !order.is_empty() && order.iter().all(|k| ends.contains_key(k)) {
            break;
        }
    }
    (order.iter().map(|k| ends[k].clone()).collect(), spawn_call)
}

#[test]
fn members_leave_deploy_state_k_delays_apart() {
    let db = cards();
    let calib = royalesim::state::Calib::shipped();
    for (card, at) in [("Goblins", t(1450, 300)), ("MinionHorde", t(350, 300)), ("Skeletons", t(900, 500)), ("Barbarians", t(1450, 800))] {
        let c = db.get(db.index(card).unwrap());
        let delay = c.formation.summon_deploy_delay_ms;
        let (ends, spawn_call) = deploy_end_ticks(card, at);
        assert_eq!(ends.len(), c.count as usize, "{card}: members");
        let first = ends[0].1;
        assert_eq!(first as i32 - spawn_call as i32, c.deploy_time_ms / calib.tick_ms, "{card}: the first member's deploy end");
        for (k, (_, end)) in ends.iter().enumerate() {
            assert_eq!(*end as i32 - first as i32, k as i32 * delay / calib.tick_ms, "{card}: member {k} (SummonDeployDelay {delay})");
        }
        if delay > 0 {
            assert!(ends.last().unwrap().1 > first, "{card}: no stagger at all");
        }
    }
    // The second summon's own delay: the Rascals' Girls (SummonDeployDelay blank,
    // SummonDeployDelaySecond set).
    let c = db.get(db.index("Rascals").unwrap());
    let second = c.formation.second_summon.expect("Rascals carry a second summon");
    assert_eq!(c.formation.summon_deploy_delay_ms, 0);
    let d2 = c.formation.summon_deploy_delay_second_ms;
    assert!(d2 > 0);
    let (ends, _) = deploy_end_ticks("Rascals", t(900, 500));
    assert_eq!(ends.len(), (c.count + second.count) as usize);
    assert_eq!(ends[0].0, "Rascals");
    for j in 0..second.count as usize {
        let (name, end) = &ends[c.count as usize + j];
        assert_eq!(name, &db.get(second.unit).name);
        assert_eq!(*end as i32 - ends[0].1 as i32, (j as i32 + 1) * d2 / calib.tick_ms, "Girl {j}");
    }
}

// ---------------------------------------------------------------------------
// 4. the second summon

#[test]
fn goblin_gang_and_rascals_summon_both_units_on_one_ring() {
    let db = cards();
    let f = fixture();
    for (card, unit) in [("GoblinGang", "SpearGoblin"), ("Rascals", "RascalGirl")] {
        let c = db.get(db.index(card).unwrap());
        let second = c.formation.second_summon.unwrap_or_else(|| panic!("{card} has no second summon"));
        assert_eq!(db.get(second.unit).name, unit);
        assert!(db.get(second.unit).summon_only);
        let g = f.groups.iter().find(|g| g.card == card).unwrap_or_else(|| panic!("no measured {card}"));
        let got = preview(g, (0, 0));
        assert_eq!(got.len(), (c.count + second.count) as usize);
        assert!(got[..c.count as usize].iter().all(|(n, _, _)| n == card));
        assert!(got[c.count as usize..].iter().all(|(n, _, _)| n == unit));
        // Live: the units exist, on the team, at the card's level, and the ring's
        // circumradius is one number for every member (a regular polygon around the tap).
        let mut s = BattleState::new(5, config());
        let pos = Vec2::new(g.tap[0] * K, g.tap[1] * K);
        s.spawn_unit(team_of(g.side), card, pos, None).unwrap();
        s.tick();
        let live: Vec<_> = s.entities().filter(|e| e.team == team_of(g.side) && (e.card == card || e.card == unit)).collect();
        assert_eq!(live.len(), got.len());
        // The unit's level is the card's: its max hp is the unit's hitpoints scaled
        // like the card's own members at the deploying side's level.
        let level = s.config().card_level[team_of(g.side) as usize];
        let want_hp = db.scaled(second.unit, level, db.get(second.unit).hitpoints).unwrap();
        assert_eq!(s.entities().find(|e| e.card == unit).unwrap().max_hp, want_hp, "{unit}: not at the card's level");
        // The preview's points (before the contact law touches anything: a Girl next
        // to the princess tower is pushed on her first tick) lie on ONE ring.
        let radii: Vec<i64> = got.iter().map(|(_, p, _)| dist(native(*p), (g.tap[0] as i64, g.tap[1] as i64))).collect();
        let (lo, hi) = (*radii.iter().min().unwrap(), *radii.iter().max().unwrap());
        assert!(hi - lo <= 2, "{card}: not one ring: {radii:?}");
        assert!(lo > 0);
    }
}

// ---------------------------------------------------------------------------
// 5. seat symmetry

#[test]
fn the_ring_is_seat_symmetric_under_the_rotation() {
    // symmetric_config: the shipped ground clamp is the measured per-side formula,
    // which is not the rotation of itself at the back edge (the next test pins that);
    // the layout under it is compared here.
    let s = BattleState::new(7, symmetric_config());
    assert_eq!(s.config().calib.formation_ground_y_clamp, GroundYClamp::DeployColumnRangeOwnFrame);
    let a = s.arena().clone();
    let taps = [t(350, 500), t(900, 500), t(1450, 500), t(350, 1450), t(1450, 1450), t(900, 100), t(1750, 200), t(850, 1440)];
    let mut compared = 0;
    for card in ["Skeletons", "Goblins", "Minions", "Barbarians", "MinionHorde", "Bats", "GoblinGang", "Rascals", "RoyalHogs", "SkeletonArmy", "Wallbreakers", "Archer"] {
        for tap in taps {
            let blue = s.formation_preview(Team::Blue, card, tap).unwrap();
            let red = s.formation_preview(Team::Red, card, a.rotate(tap)).unwrap();
            assert_eq!(blue.len(), red.len(), "{card} at {tap:?}");
            for (k, (b, r)) in blue.iter().zip(&red).enumerate() {
                assert_eq!(b.0, r.0, "{card} member {k}: unit");
                assert_eq!(a.rotate(b.1), r.1, "{card} at {tap:?} member {k}: Red is not Blue's rotation ({:?} vs {:?})", b.1, r.1);
                assert_eq!(b.2, r.2, "{card} member {k}: timer");
                compared += 1;
            }
        }
    }
    assert!(compared > 200);
}

#[test]
fn the_shipped_ground_clamp_is_the_measured_per_side_formula() {
    // The measured asymmetry (calibration formation.GROUND_Y_CLAMP): side 1's rear
    // members on a back-row tap are held on the highest deployable row's NEAR edge
    // (absolute 31000 on the shipped map), side 0's on the lowest row's near edge
    // (0, which the bounds clamp lifts); at the river the two are one native unit
    // apart. Measured live on the Red Goblins of capture 20260918-164951.
    let s = BattleState::new(1, config());
    assert_eq!(s.config().calib.formation_ground_y_clamp, GroundYClamp::Client16402DeployColumnRange);
    let a = s.arena().clone();
    let tile = a.cell * 2;
    let back_blue = Vec2::new(a.width / 4, tile / 2); // row 0's centre
    let blue = s.formation_preview(Team::Blue, "Goblins", back_blue).unwrap();
    let red = s.formation_preview(Team::Red, "Goblins", a.rotate(back_blue)).unwrap();
    let blue_rear = blue.iter().map(|(_, p, _)| p.y).min().unwrap();
    let red_rear = red.iter().map(|(_, p, _)| p.y).max().unwrap();
    assert_eq!(blue_rear, a.cell / 2, "Blue's rear pair sits on the bounds clamp");
    assert_eq!(red_rear, a.height - tile, "Red's rear pair sits on its back row's near edge");
    assert_ne!(a.rotate(Vec2::new(0, blue_rear)).y, red_rear, "the shipped arm is not seat-symmetric at the back edge");
    // At the river: one native unit apart.
    let bank_row = a.water_y_min / a.cell - 1;
    let bank = Vec2::new(a.width / 4, bank_row * a.cell + a.cell / 2);
    let blue = s.formation_preview(Team::Blue, "Skeletons", bank).unwrap();
    let red = s.formation_preview(Team::Red, "Skeletons", a.rotate(bank)).unwrap();
    let blue_fwd = blue.iter().map(|(_, p, _)| p.y).max().unwrap();
    let red_fwd = red.iter().map(|(_, p, _)| p.y).min().unwrap();
    assert_eq!(a.rotate(Vec2::new(0, blue_fwd)).y - red_fwd, K, "Red's river bound is one native unit tighter");
    // The own-frame arm makes the two seats rotations of each other on both.
    let sym = BattleState::new(1, symmetric_config());
    for (card, tap) in [("Goblins", back_blue), ("Skeletons", bank)] {
        let b = sym.formation_preview(Team::Blue, card, tap).unwrap();
        let r = sym.formation_preview(Team::Red, card, a.rotate(tap)).unwrap();
        for (x, y) in b.iter().zip(&r) {
            assert_eq!(a.rotate(x.1), y.1, "{card}: own-frame arm");
        }
    }
}

// ---------------------------------------------------------------------------
// 6. the lane classifier

#[test]
fn nearest_lane_is_left_of_the_centre_column_and_swaps_under_the_rotation() {
    let s = BattleState::new(1, config());
    let a = s.arena();
    let mut left = 0;
    let mut right = 0;
    for row in 0..a.rows {
        for col in 0..a.cols {
            let p = a.half_to_subtile_center(col, row);
            let lane = nearest_lane(a, p);
            let want = if col * 2 < a.cols { LANE_LEFT } else { LANE_RIGHT };
            assert_eq!(lane, want, "cell ({col}, {row})");
            let rot = nearest_lane(a, a.rotate(p));
            assert_eq!(rot, 3 - lane, "cell ({col}, {row}) rotated");
            if lane == LANE_LEFT {
                left += 1
            } else {
                right += 1
            }
        }
    }
    assert_eq!(left, right);
    // The centre line itself: x = W/2 is the first cell of the right half.
    assert_eq!(nearest_lane(a, Vec2::new(a.width / 2, a.height / 2)), LANE_RIGHT);
    assert_eq!(nearest_lane(a, Vec2::new(a.width / 2 - 1, a.height / 2)), LANE_LEFT);
}

// ---------------------------------------------------------------------------
// 7. the clamps

#[test]
fn the_ground_clamp_holds_a_bank_deploy_on_the_bank_and_the_bounds_clamp_the_edge() {
    let db = cards();
    let calib = royalesim::state::Calib::shipped();
    assert_eq!(calib.formation_ground_y_clamp, GroundYClamp::Client16402DeployColumnRange);
    let s = BattleState::new(1, config());
    let a = s.arena().clone();
    // Blue Skeletons on the last dry row before the river: the forward member
    // would land in the water; the game holds it on that row's centre.
    let bank_row = a.water_y_min / a.cell - 1; // the last dry half-row
    let tap = Vec2::new(a.width * 3 / 4, bank_row * a.cell + a.cell / 2);
    let got = s.formation_preview(Team::Blue, "Skeletons", tap).unwrap();
    let forward = got.iter().map(|(_, p, _)| p.y).max().unwrap();
    assert!(forward <= tap.y, "the forward Skeleton went past the tap row: {forward} > {}", tap.y);
    assert!(got.iter().all(|(_, p, _)| a.is_passable_ground(*p)));
    // The two rear members are the ring's (unclamped).
    let c = db.get(db.index("Skeletons").unwrap());
    let ring = member_offset(
        Layout { primaries: c.count, seconds: 0, radius: c.formation.summon_radius / K, width: 0, angle_shift: 0, lane: LANE_RIGHT, lane_mirror: true },
        1,
    );
    assert_eq!(native(got[1].1), (native(tap).0 + ring.x as i64, native(tap).1 + ring.y as i64));
    // Red at the rotated tap under the own-frame arm: the rotation of Blue's.
    let sym = BattleState::new(1, symmetric_config());
    let red = sym.formation_preview(Team::Red, "Skeletons", a.rotate(tap)).unwrap();
    for (b, r) in got.iter().zip(&red) {
        assert_eq!(a.rotate(b.1), r.1);
    }
    // Under the `none` arm the forward member is ejected to passable ground
    // instead, and lands somewhere else than the clamp put it.
    let mut cfg = config();
    cfg.calib.formation_ground_y_clamp = GroundYClamp::None;
    let s2 = BattleState::new(1, cfg);
    let none = s2.formation_preview(Team::Blue, "Skeletons", tap).unwrap();
    assert!(none.iter().all(|(_, p, _)| a.is_passable_ground(*p)));
    assert_ne!(none[0].1, got[0].1, "the clamp arm changed nothing");
    // The bounds clamp: Bats on the back row (SpawnAngleShift 45, ring 1405) --
    // one member would leave the arena; it stops half a cell inside.
    let back = Vec2::new(a.width * 3 / 4, a.cell * 3);
    let bats = s.formation_preview(Team::Blue, "Bats", back).unwrap();
    let min_y = bats.iter().map(|(_, p, _)| p.y).min().unwrap();
    assert_eq!(min_y, a.cell / 2, "the lowest Bat is not on the bounds clamp");
    assert!(bats.iter().any(|(_, p, _)| p.y > a.cell / 2 + a.cell), "every Bat on the edge: not a ring");
}

// ---------------------------------------------------------------------------
// 8. the candidates

#[test]
fn every_formation_candidate_moves_a_behaviour() {
    let db = cards();
    let calib = royalesim::state::Calib::shipped();
    assert_eq!(calib.formation_layout, FormationLayout::Client16402);
    assert_eq!(calib.formation_deploy_stagger, DeployStagger::Client16402);
    let tap = t(1450, 800);
    let ring = BattleState::new(1, config()).formation_preview(Team::Blue, "Skeletons", tap).unwrap();
    // engine_grid: the old centred grid, one collision diameter apart, rows toward
    // the enemy first.
    let mut cfg = config();
    cfg.calib.formation_layout = FormationLayout::EngineGrid;
    let grid = BattleState::new(1, cfg).formation_preview(Team::Blue, "Skeletons", tap).unwrap();
    let c = db.get(db.index("Skeletons").unwrap());
    let spacing = c.collision_radius * 2;
    let want = [Vec2::new(-spacing / 2, spacing / 2), Vec2::new(spacing / 2, spacing / 2), Vec2::new(0, -spacing / 2)];
    for (k, (_, p, _)) in grid.iter().enumerate() {
        assert_eq!(p.sub(tap), want[k], "grid member {k}");
    }
    assert!(ring.iter().zip(&grid).any(|(r, g)| r.1 != g.1), "the layout arm changed nothing");
    // The grid arm still spawns the second summon (on the same grid).
    let mut cfg = config();
    cfg.calib.formation_layout = FormationLayout::EngineGrid;
    assert_eq!(BattleState::new(1, cfg).formation_preview(Team::Blue, "GoblinGang", tap).unwrap().len(), 6);
    // none: every member on one timer.
    let mut cfg = config();
    cfg.calib.formation_deploy_stagger = DeployStagger::None;
    let flat = BattleState::new(1, cfg).formation_preview(Team::Blue, "Goblins", tap).unwrap();
    assert!(flat.iter().all(|m| m.2 == flat[0].2));
    let staggered = BattleState::new(1, config()).formation_preview(Team::Blue, "Goblins", tap).unwrap();
    assert!(staggered.iter().any(|m| m.2 != staggered[0].2), "the stagger arm changed nothing");
    // A single summon keeps the exact subtile tap under both layouts.
    let odd = Vec2::new(tap.x + 7, tap.y + 5);
    assert_eq!(BattleState::new(1, config()).formation_preview(Team::Blue, "Knight", odd).unwrap()[0].1, odd);
}

// ---------------------------------------------------------------------------
// 9. a whole battle

#[test]
fn a_scripted_battle_with_swarms_keeps_every_invariant() {
    let mut cfg = scripted_config();
    let blue = ["Skeletons", "Goblins", "MinionHorde", "GoblinGang", "Barbarians", "Bats", "Rascals", "RoyalHogs"];
    let red = ["SkeletonArmy", "Minions", "Wallbreakers", "Goblins", "Skeletons", "Barbarians", "GoblinGang", "Archer"];
    cfg.decks = [blue.iter().map(|s| s.to_string()).collect(), red.iter().map(|s| s.to_string()).collect()];
    let run = run_scripted_with(cfg, 21, true, None);
    assert!(run.hashes.len() > 500, "the battle ended at {}", run.hashes.len());
    assert!(run.plays[0] + run.plays[1] >= 12, "only {:?} plays: not a swarm battle", run.plays);
    assert!(run.max_live >= 20, "at most {} live: not a swarm battle", run.max_live);
}
