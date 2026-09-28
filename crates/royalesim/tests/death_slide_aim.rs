//! spawner.DEATH_SLIDE_AIM -- what a sliding death-spawn member steps toward (spawner.DEATH_SPAWN_PUSHBACK =
//! client_ring_slide; move16402.rs `death_slide_step`, `death_slide_toward`; state.rs `slide_end_points`,
//! `slide_ends`, `phase_path16402`; entity.rs `death_slide_end`).
//!
//! THE READING (arm fixed_end_point, status hypothesis; the old arm current_ray ships): each tick a member steps
//! move16402::DEATH_SLIDE_STEP toward its END POINT, fixed at birth -- the death point + its ring direction x
//! DeathSpawnRadius, the container ring's direction for a container's member -- through the walk's 1/256 direction
//! and truncation, and its slide ends on the step that reaches it; the contact push is added after, as before. So a
//! push that took it off its ring line is walked back onto the line. Under current_ray it steps out along the ray
//! through where it stands, and keeps the push's turn to the end. Read off every Lava Pup ring in the 16.402 corpus
//! and the 15.535.29 scenario fixtures (20260920-071744, ub-b1-tm2-air, the ub-ds3 family): from a Pup's 3rd frame,
//! the 1/256 step toward the end point is the captured step to the unit on 185 of 199 steps off the ring line; the
//! current ray gives none of them.
//!
//! THE LAST STEP IS THE 1/256 STEP TOO, so a member that arrives off an axis can stop a unit short of its end point:
//! the client's own 0-degree Pup of 20260920-071744 ends on (2500, -1) from the death point, its end (2500, 0), and
//! its 300-degree Pup on (1249, -2164), its end (1250, -2165). The end point itself is exact.
//!
//! WHAT IS PINNED, and the plant that turns each gate red (each compiled in with `--cfg clash_plant="..."`,
//! docs/contributing.md):
//!   1. a Pup the ring's own contact pushes off its line (the 180-degree one, as in 20260920-071744) is walked back
//!      onto it under fixed_end_point and ends its slide on its end point, the death point + (-2500, 0), to the last
//!      step's truncation (one unit an axis); under current_ray it ends more than 300 off that point
//!      -- death_slide_aim_ray;
//!   2. the control: a lone Pup (a Lava Hound doctored to leave one), which nothing pushes, ends exactly on
//!      (2500, 0) from the death point, on the same tick, under both arms;
//!   3. the end points are fixed at birth: under fixed_end_point every Pup carries the death point + its ring
//!      direction x 2500, and drops it when its slide ends; under current_ray none carries one;
//!   4. a container's members (the Skeleton Barrel's) carry their container ring's direction at DeathSpawnRadius,
//!      and still stop after their four ticks;
//!   5. the step itself, pure, on the client's own steps of 20260920-071744-B, where the current ray misses each;
//!   6. a snapshot mid-slide under fixed_end_point resumes hash for hash, and the end point is hashed under that arm
//!      and not under current_ray (whose battles hash as before the column) -- hash_skips_slide_end;
//!   7. the ledger ships current_ray.
//!
//! NOT PINNED: where the client ENDS a slide short of its end point. In every capture the four diagonal Pups stop
//! after 8 moves (1, 41, 96 and 126 short of their ends) and the two axis Pups take a 9th; neither arm has that stop.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::formation::sin1024;
use royalesim::move16402::{death_slide_to, death_slide_toward, DEATH_SLIDE_STEP};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, Calib, DeathSlideAim, DeathSpawnPushback};
use royalesim::{EntityId, Team};

const ARMS: [DeathSlideAim; 2] = [DeathSlideAim::CurrentRay, DeathSlideAim::FixedEndPoint];

/// The Lava Hound's DeathSpawnRadius, native: the Pups' measured end radius, written here rather than read off the
/// card so the end points are held to the measurement.
const PUP_RADIUS: i32 = 2500;

fn with_aim(mut cfg: BattleConfig, aim: DeathSlideAim) -> BattleConfig {
    cfg.calib.death_spawn_pushback = DeathSpawnPushback::ClientRingSlide;
    cfg.calib.death_slide_aim = aim;
    cfg
}

fn native(v: Vec2) -> (i32, i32) {
    (v.x / K, v.y / K)
}

/// The length of a native offset, integer square root.
fn len(o: (i32, i32)) -> i32 {
    isqrt((o.0 as i64) * (o.0 as i64) + (o.1 as i64) * (o.1 as i64)) as i32
}

/// The Pups' ring direction x 2500 at the measured angle `a`, native, through the engine's sine table, each axis
/// truncated: the end point's offset from the death point.
fn end_offset(a: i32) -> (i32, i32) {
    (PUP_RADIUS * sin1024(a + 90) / 1024, PUP_RADIUS * sin1024(a) / 1024)
}

/// The death-spawned members of `unit` on Blue, in creation order.
fn members(s: &BattleState, unit: &str) -> Vec<EntityId> {
    let mut m: Vec<(u32, EntityId)> = find_live(s, Team::Blue, unit).into_iter().filter(|e| e.spawned_by.is_none()).map(|e| (e.team_seq, e.id)).collect();
    m.sort();
    m.into_iter().map(|(_, id)| id).collect()
}

/// A Blue Lava Hound at (9000, 9000) killed on the first tick, under `cfg`. Returns the battle on its death frame,
/// the Pups in creation order and the death point (the slide's centre, which every member carries).
fn kill_hound(cfg: BattleConfig) -> (BattleState, Vec<EntityId>, Vec2) {
    let mut s = BattleState::new(7, cfg);
    let p = t(900, 900);
    assert!(s.arena().is_passable_ground(p), "scene: the spot is on dry ground");
    let id = s.scenario_spawn_now(Team::Blue, "LavaHound", p, None).unwrap();
    assert!(s.debug_set_hp(id, 0));
    s.tick();
    assert!(s.entity(id).is_none(), "the Lava Hound died on the first tick");
    let ids = members(&s, "LavaPups");
    assert!(!ids.is_empty(), "the Lava Hound left no Pups");
    let c = s.entity(ids[0]).unwrap().death_slide_centre;
    for id in &ids {
        let e = s.entity(*id).unwrap();
        assert_eq!((e.death_slide_centre, e.death_slide_radius), (c, PUP_RADIUS * K), "scene: a Pup is not born sliding from the death point");
    }
    (s, ids, c)
}

/// Member `id`'s slide from its death frame: its native offset from `c` on each tick while it slides, the first
/// entry its birth and the last the tick its slide ended. Panics if it still slides after 20 ticks.
fn slide_track(s: &mut BattleState, id: EntityId, c: Vec2) -> Vec<(i32, i32)> {
    let mut tr = vec![native(s.entity(id).unwrap().pos.sub(c))];
    for _ in 0..20 {
        s.tick();
        let e = s.entity(id).expect("the member is alive");
        tr.push(native(e.pos.sub(c)));
        if e.death_slide_radius == 0 {
            return tr;
        }
    }
    panic!("the member still slides after 20 ticks: {tr:?}");
}

// ---------------------------------------------------------------------------
// (1) a pushed Pup

#[test]
fn a_pup_pushed_off_its_ring_line_ends_on_its_end_point_under_fixed_end_point_and_off_it_under_current_ray() {
    // Plant death_slide_aim_ray: fixed_end_point steps along the current ray, and the Pup ends 300 and more off.
    let mut ends = Vec::new();
    for aim in ARMS {
        let (mut s, ids, c) = kill_hound(with_aim(config(), aim));
        assert_eq!(ids.len(), 6, "data: the Lava Hound's six Pups");
        // member 2 in creation order sits at 180 degrees (tests/death_spawn_pushback.rs pins the order)
        let tr = slide_track(&mut s, ids[2], c);
        assert_eq!(tr[0], (-DEATH_SLIDE_STEP, 0), "{aim:?}: scene: the 180-degree Pup is not born 250 on -x");
        // THE PUSH: its neighbours' contact takes it off the -x axis mid-slide, under both arms (the corpus Pup of
        // 20260920-071744 went to (2310, 20153), 132 off its line, on the second tick)
        let off = tr.iter().map(|p| p.1.abs()).max().unwrap();
        assert!(off >= 100, "{aim:?}: vacuous: the Pup was never pushed off its line ({off} at most): {tr:?}");
        ends.push((aim, *tr.last().unwrap(), tr));
    }
    let want = end_offset(180);
    assert_eq!(want, (-PUP_RADIUS, 0));
    let (_, got, tr) = &ends[1];
    // fixed_end_point: back on the line and on its end point, to the last 1/256 step's truncation
    assert!((got.0 - want.0).abs() <= 1 && (got.1 - want.1).abs() <= 1, "fixed_end_point: the pushed Pup ended on {got:?}, not on its end point {want:?}: {tr:?}");
    // current_ray: the push's turn is kept to the end of the slide
    let (_, old, tr) = &ends[0];
    let miss = len((old.0 - want.0, old.1 - want.1));
    assert!(miss > 300, "current_ray: the pushed Pup ended {miss} from the end point ({old:?}); the arms do not separate: {tr:?}");
}

// ---------------------------------------------------------------------------
// (2) an unpushed Pup, the control

#[test]
fn a_lone_pup_that_nothing_pushes_ends_exactly_on_its_end_point_under_both_arms() {
    // A Lava Hound doctored to leave ONE Pup, at -(0 + 1) x 360 / 1 = 0 degrees, with no neighbour to push it.
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let hound = doc["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "LavaHound").expect("data: a LavaHound row");
    assert_eq!(hound["death_spawn"]["count"], 6, "data: the Lava Hound's count");
    hound["death_spawn"]["count"] = serde_json::Value::from(1);
    let db = CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).expect("the doctored file still parses");
    let mut got = Vec::new();
    for aim in ARMS {
        let (mut s, ids, c) = kill_hound(with_aim(BattleConfig::with_cards(db.clone()), aim));
        assert_eq!(ids.len(), 1, "scene: the doctored Lava Hound leaves one Pup");
        let tr = slide_track(&mut s, ids[0], c);
        assert!(tr.iter().all(|p| p.1 == 0), "{aim:?}: vacuous: the lone Pup left the +x axis: {tr:?}");
        assert_eq!(*tr.last().unwrap(), end_offset(0), "{aim:?}: the lone Pup did not end on (2500, 0) from the death point: {tr:?}");
        got.push(tr);
    }
    assert_eq!(got[0], got[1], "an unpushed Pup's slide differs between the arms");
}

// ---------------------------------------------------------------------------
// (3) the end points, fixed at birth

#[test]
fn every_pup_carries_its_end_point_from_birth_under_fixed_end_point_and_none_under_current_ray() {
    let angles = [300, 240, 180, 120, 60, 0];
    for aim in ARMS {
        let (mut s, ids, c) = kill_hound(with_aim(config(), aim));
        for (k, id) in ids.iter().enumerate() {
            let e = s.entity(*id).unwrap();
            let want = match aim {
                DeathSlideAim::FixedEndPoint => {
                    let (x, y) = end_offset(angles[k]);
                    c.add(Vec2::new(x * K, y * K))
                }
                DeathSlideAim::CurrentRay => Vec2::default(),
            };
            assert_eq!(e.death_slide_end, want, "{aim:?}: Pup {k} ({} degrees) carries the wrong end point", angles[k]);
        }
        // the end point stays fixed while the slide runs (pushes and all) and is dropped with it
        let born: Vec<Vec2> = ids.iter().map(|id| s.entity(*id).unwrap().death_slide_end).collect();
        let mut ended = 0;
        for _ in 0..20 {
            s.tick();
            for (k, id) in ids.iter().enumerate() {
                let e = s.entity(*id).unwrap();
                if e.death_slide_radius > 0 {
                    assert_eq!(e.death_slide_end, born[k], "{aim:?}: Pup {k}'s end point moved mid-slide");
                } else {
                    assert_eq!(e.death_slide_end, Vec2::default(), "{aim:?}: Pup {k} kept its end point after its slide");
                }
            }
            ended = ids.iter().filter(|id| s.entity(**id).unwrap().death_slide_radius == 0).count();
        }
        assert_eq!(ended, ids.len(), "{aim:?}: vacuous: a Pup never ended its slide");
    }
}

// ---------------------------------------------------------------------------
// (4) a container's members

/// The Skeleton Barrel's container point once its barrel is killed (a Flight spell's aim), and the tick it died on.
fn kill_barrel(s: &mut BattleState, p: (i32, i32)) -> (u32, Vec2) {
    let id = s.scenario_spawn_now(Team::Blue, "SkeletonBalloon", Vec2::new(p.0 * K, p.1 * K), None).expect("the barrel is placed");
    assert!(s.debug_set_hp(id, 0));
    let t = s.tick_count();
    s.tick();
    assert!(s.entity(id).is_none(), "the barrel died on its tick");
    let db = s.cards();
    let container = db.get(db.index("SkeletonBalloon").expect("the Skeleton Barrel loads")).death_spawn.expect("its death spawn").unit;
    let c = s
        .spells()
        .iter()
        .find(|sp| sp.card == container)
        .map(|sp| match &sp.motion {
            SpellMotion::Flight { aim, .. } => *aim,
            m => panic!("the container is not a Flight: {m:?}"),
        })
        .expect("the barrel's death left its container");
    (t, c)
}

#[test]
fn a_containers_members_carry_their_container_rings_direction_at_its_radius() {
    // fixed_end_point. The Skeleton Barrel's seven on the left half of side 0, born on the measured container ring
    // (tests/skeleton_barrel.rs): each end point is DeathSpawnRadius (1480) from C on its member's birth direction, and
    // the four-tick cap still stops the slide.
    let mut s = BattleState::new(9, with_aim(config(), DeathSlideAim::FixedEndPoint));
    let (t, c) = kill_barrel(&mut s, (6000, 10000));
    while s.tick_count() <= t + 12 {
        s.tick();
    }
    let mut m: Vec<(u32, EntityId)> = s.entities().filter(|v| v.team == Team::Blue && v.card == "Skeleton" && v.spawned_by.is_none()).map(|v| (v.team_seq, v.id)).collect();
    m.sort();
    let ids: Vec<EntityId> = m.into_iter().map(|(_, id)| id).collect();
    assert_eq!(ids.len(), 7, "the seven Skeletons on T + 12");
    let r = s.entity(ids[0]).unwrap().death_slide_radius / K;
    assert_eq!(r, 1480, "data: the container's DeathSpawnRadius");
    for (k, id) in ids.iter().enumerate() {
        let e = s.entity(*id).unwrap();
        let (b, end) = (native(e.pos.sub(c)), native(e.death_slide_end.sub(c)));
        // on the radius, to the per-axis truncation
        let elen = len(end);
        assert!((elen - r).abs() <= 2, "Skeleton {k}: end point {end:?} is {elen} from C, not {r}");
        // on the member's birth direction: |b x end| / (|b| |end|), the sine of the angle between them, under 1/100
        let cross = (b.0 as i64) * (end.1 as i64) - (b.1 as i64) * (end.0 as i64);
        let dot = (b.0 as i64) * (end.0 as i64) + (b.1 as i64) * (end.1 as i64);
        assert!(dot > 0 && cross.abs() * 100 < (len(b) as i64) * (elen as i64), "Skeleton {k}: end point {end:?} is off its birth direction {b:?}");
    }
    let mut moved_after_cap = false;
    for tick in 13..=21 {
        let before: Vec<Vec2> = ids.iter().map(|id| s.entity(*id).unwrap().pos).collect();
        s.tick();
        if tick > 16 {
            moved_after_cap |= ids.iter().zip(&before).any(|(id, b)| s.entity(*id).unwrap().pos != *b);
        }
    }
    assert!(!moved_after_cap, "a Skeleton slid past its four ticks under fixed_end_point");
}

// ---------------------------------------------------------------------------
// (5) the step, pure

#[test]
fn the_step_toward_the_end_point_is_the_clients_step_where_the_current_ray_is_not() {
    // 20260920-071744-B, the Lava Hound's death on (3292, 20285), offsets from it, native. The end points are
    // (1250, -2165), (-1250, -2165), (2500, 0) and (-2500, 0) (`end_offset`); the client's positions on each tick
    // are the fixture's.
    for (who, from, end, client, last) in [
        // Pup 57 (300 degrees), t1801 -> t1802: the 1/256 step (125, -215); exact v x 250 / d gives (126, -216)
        ("Pup 57 t1802", (644, -1127), end_offset(300), (769, -1342), false),
        // Pup 58 (240 degrees), pushed to (2914, 19383) on t1800, t1800 -> t1801: back toward its line
        ("Pup 58 t1801", (-378, -902), end_offset(240), (-519, -1107), false),
        // Pup 57's last step, t1805 -> t1806: the truncation leaves it a unit short on both axes, and it stops
        ("Pup 57 t1806", (1144, -1987), end_offset(300), (1249, -2164), true),
        // Pup 62 (0 degrees), t1806 -> t1807: a unit short on y
        ("Pup 62 t1807", (2286, -10), end_offset(0), (2500, -1), true),
        // Pup 59 (180 degrees), the case item 12 was opened on, t1806 -> t1807: on the end point exactly
        ("Pup 59 t1807", (-2476, -3), end_offset(180), (-2500, 0), true),
    ] {
        assert_eq!(death_slide_toward(from, end, DEATH_SLIDE_STEP), (client, last), "{who}");
        let (ray, _) = death_slide_to(from, (0, 0), PUP_RADIUS, DEATH_SLIDE_STEP);
        assert_ne!(ray, client, "{who}: the current ray gives the client's step too, so the case does not discriminate");
    }
    // a member already on its end point stays there, and its slide ends
    assert_eq!(death_slide_toward((-2500, 0), (-2500, 0), DEATH_SLIDE_STEP), ((-2500, 0), true));
    // a push that carried it past its end point is walked back, not held (the current ray would hold it)
    assert_eq!(death_slide_toward((-2600, 0), (-2500, 0), DEATH_SLIDE_STEP), ((-2500, 0), true));
}

// ---------------------------------------------------------------------------
// (6) save / load

#[test]
fn a_snapshot_mid_slide_resumes_and_the_end_point_is_hashed_under_fixed_end_point_alone() {
    // Plant hash_skips_slide_end: the save edited in an end point loads under fixed_end_point.
    for aim in ARMS {
        let (mut s, ids, _) = kill_hound(with_aim(config(), aim));
        for _ in 0..3 {
            s.tick();
        }
        let k = ids.iter().position(|id| s.entity(*id).unwrap().death_slide_radius > 0).expect("vacuous: no Pup is mid-slide at the save");
        let slot = ids[k].index as usize;
        let hashed = edit_is_hashed(&s, |v| {
            let x = v["ents"]["death_slide_end"][slot]["x"].as_i64().expect("the snapshot carries the end points");
            v["ents"]["death_slide_end"][slot]["x"] = serde_json::json!(x + 1);
        });
        match aim {
            DeathSlideAim::FixedEndPoint => assert!(hashed, "a save edited only in a Pup's end point loads: the end point is not hashed"),
            // current_ray never reads the column, and a battle at that arm hashes as it did before the column
            DeathSlideAim::CurrentRay => assert!(!hashed, "current_ray hashes the end point it never reads"),
        }
        let mut l = BattleState::load(&s.save()).unwrap_or_else(|e| panic!("{aim:?}: a mid-slide snapshot does not load: {e}"));
        for n in 0..20 {
            s.tick();
            l.tick();
            assert_eq!(l.state_hash(), s.state_hash(), "{aim:?}: diverged {n} ticks after the load");
            for id in &ids {
                let (a, b) = (s.entity(*id).unwrap(), l.entity(*id).unwrap());
                assert_eq!((a.pos, a.death_slide_end), (b.pos, b.death_slide_end), "{aim:?}: a Pup's slide, {n} ticks after the load");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// (7) the ledger

#[test]
fn the_ledger_ships_current_ray() {
    assert_eq!(Calib::shipped().death_slide_aim, DeathSlideAim::CurrentRay, "current_ray ships until parity scores fixed_end_point");
    assert_eq!(config().calib.death_slide_aim, DeathSlideAim::CurrentRay);
}
