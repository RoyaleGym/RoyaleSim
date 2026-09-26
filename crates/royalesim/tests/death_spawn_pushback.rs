//! THE DEATH-SPAWN SLIDE -- calibration spawner.DEATH_SPAWN_PUSHBACK; card.rs
//! `CardDef::death_spawn_pushback`, state.rs `death_spawn_points` (`fixed_slide_ring`) and
//! `phase_path16402`, move16402.rs `death_slide_to` / `death_slide_step`.
//!
//! THE LAW (arm client_ring_slide), measured on client 16.402 (3 Golem deaths and 1 Lava
//! Hound death, side 0): a dying unit whose row sets DeathSpawnPushback puts member k of n
//! 250 from the death point at the fixed angle -(k + 1) x 360 / n (0 = +x), whatever its
//! heading -- the Golemites at 180 and 0, the Pups at 300, 240, 180, 120, 60 and 0 -- and
//! the members then slide straight out 250 a tick, neither walking nor attacking, to exactly
//! DeathSpawnRadius. The Golemites' radius per tick: 250, 650, 900, 1150, 1400, 1500 and 250,
//! 601, 851, 1101, 1351, 1500. The Battle Ram, which leaves the column blank, keeps the
//! facing ring (its Barbarians at 600 on its axis, no slide). The key SHIPS at not_read,
//! today's engine, so every scene here selects its arm explicitly.
//!
//! WHAT IS PINNED, and the plant that turns each gate red (each one compiled in with
//! `--cfg clash_plant="..."`, docs/contributing.md):
//!   1. the loader reads the column off every row, absent reading false, and the Golem and
//!      the Lava Hound carry it where the Battle Ram does not
//!      -- death_spawn_pushback_unread;
//!   2. under the new arm the members are born on the fixed ring at 250, as a SET of the
//!      measured angles, for a Golem (2) and a Lava Hound (6) whose facing is off every
//!      ring angle -- death_ring_slide_facing;
//!   3. the Golemites slide: the measured tracks, tick for tick, then the slide state
//!      clears and the ordinary update takes over -- death_slide_never (and
//!      death_slide_unclamped on the last tick);
//!   4. the clamp: six Pups, contact and all, never pass DeathSpawnRadius and stop on it
//!      -- death_slide_unclamped;
//!   5. a sliding member takes no target, and takes one once the slide is over
//!      -- death_slide_targets;
//!   6. the control: the Battle Ram's Barbarians keep the facing ring at 600 under the new
//!      arm -- death_ring_slide_ignores_flag;
//!   7. the control: the old arm is today's engine (and it is the shipped one), the
//!      Golemites and the Pups at DeathSpawnRadius on the facing ring, no slide
//!      -- death_ring_slide_ignores_arm. That plant changes the shipped arm, so it also
//!      reddens tests/spawner.rs `golem_killed_leaves_two_golemites_on_the_radius_and_its_death_damage_lands`
//!      and `lava_hound_killed_leaves_six_flying_pups`, which pin the same shipped
//!      behaviour (expected, not a leak), and may redden that file's
//!      `a_symmetric_tombstone_and_golem_scene_stays_its_own_mirror_every_tick` (the
//!      absolute ring's creation order is not the seat rotation's); the sweep log records
//!      which;
//!   8. a snapshot taken mid-slide resumes hash for hash -- save_drops_death_slide (the
//!      load's own hash self-check refuses the blob);
//!   9. under both frame-planned Path arms (`phase_path_2026`, and `phase_path` under a
//!      legacy path model) the slide is the same radial step to the radius, without the
//!      contact law -- frame_planned_slide_ignored. (death_slide_unclamped cannot redden
//!      this scene: the Golemites step 500, 750, 1000, 1250, 1500 and the clamp never binds;
//!      test 10 holds the shared step to its clamp);
//!  10. the radial step itself, pure: it stops ON the radius, and a member already at or
//!      past it stays where it is and ends the slide -- death_slide_unclamped,
//!      death_slide_pulls_back;
//!  11. OPEN, the member order: member k in creation order sits at -(k + 1) x 360 / n and
//!      the first-created Golemite takes the 650 track. The measurement gives the members
//!      in key order, not shown to be creation order; pinned so a change is seen, since
//!      tests 2 and 3 compare sets -- death_ring_angle_sign (the Pups run the other way
//!      round, which every set check passes);
//!  12. OPEN, side 1: a Red death lays the same ABSOLUTE ring as a Blue one, its
//!      first-created Golemite at 180 on the 650 track, where the seat rotation would put
//!      it at 0. Every measured death is side 0; pinned so the flip's side-1 behaviour is
//!      seen -- death_ring_seat_rotated;
//!  13. a flagged row whose death spawn is a BUILDING (a doctored Golem; no loaded row)
//!      takes no slide, which only the troop Path loops would end -- death_slide_on_a_building
//!      (death_spawn_pushback_unread reddens it too, on its scene check that the doctored
//!      Golem carries the flag).
//!
//! Positions are compared in NATIVE units (subtiles / SUBTILE_PER_MILLITILE): the ring and
//! the slide are laid on the native grid. The death point is the mean of the members' first
//! positions, as the measurement took it: the ring is symmetric and so is its truncation,
//! so for an even count the mean IS the point, to the unit.
mod common;

use common::*;
use royalesim::card::{CardDb, CardKind, CardSource};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::formation::sin1024;
use royalesim::move16402::{death_slide_to, DEATH_SLIDE_STEP};
use royalesim::state::{BattleConfig, BattleState, Calib, DeathSpawnPushback};
use royalesim::{EntityId, PathModel, Team};
use std::collections::BTreeSet;

/// The measured start radius, native (the ledger key's `meaning`), written here rather than
/// read off move16402::DEATH_SLIDE_START so that the ring check holds the engine to the
/// measurement and not to itself.
const MEASURED_START: i32 = 250;

fn with_arm(mut cfg: BattleConfig, arm: DeathSpawnPushback) -> BattleConfig {
    cfg.calib.death_spawn_pushback = arm;
    cfg
}

fn slide_config() -> BattleConfig {
    with_arm(config(), DeathSpawnPushback::ClientRingSlide)
}

/// A spot on Blue's half, clear of every tower and of the river by more than a Lava Hound's
/// DeathSpawnRadius.
fn spot(s: &BattleState) -> Vec2 {
    let p = t(900, 900);
    assert!(s.arena().is_passable_ground(p), "scene: the spot is on dry ground");
    p
}

/// `card`'s death spawn: the unit's name, the count and DeathSpawnRadius in NATIVE units.
fn death_spawn(s: &BattleState, card: &str) -> (String, i32, i32) {
    let ds = card_stat(s, card).death_spawn.unwrap_or_else(|| panic!("data: {card} has no death spawn"));
    let r = ds.radius.unwrap_or_else(|| panic!("data: {card} has no DeathSpawnRadius"));
    (s.cards().get(ds.unit).name.clone(), ds.count, r / K)
}

/// The death-spawned members of `unit` on Blue, in creation order (a death spawn owes
/// nothing to a periodic spawner).
fn members(s: &BattleState, unit: &str) -> Vec<EntityId> {
    members_of(s, Team::Blue, unit)
}

fn members_of(s: &BattleState, team: Team, unit: &str) -> Vec<EntityId> {
    let mut m: Vec<(u32, EntityId)> = find_live(s, team, unit).into_iter().filter(|e| e.spawned_by.is_none()).map(|e| (e.team_seq, e.id)).collect();
    m.sort();
    m.into_iter().map(|(_, id)| id).collect()
}

/// A Blue `card` at the spot, with `extra` beside it, killed on the first tick. Returns
/// the battle on its death frame and the members, which exist on that frame
/// (spawner.RELEASE_TIMING = end_of_event_phase).
fn kill(cfg: BattleConfig, card: &str, extra: &[(Team, &str, Vec2)]) -> (BattleState, Vec<EntityId>) {
    kill_on(cfg, Team::Blue, card, extra)
}

/// `kill` for either seat: Blue's `card` at the spot, Red's at its seat mirror.
fn kill_on(cfg: BattleConfig, side: Team, card: &str, extra: &[(Team, &str, Vec2)]) -> (BattleState, Vec<EntityId>) {
    let mut s = BattleState::new(7, cfg);
    let pos = match side {
        Team::Blue => spot(&s),
        Team::Red => mirror(&s, spot(&s)),
    };
    assert!(s.arena().is_passable_ground(pos), "scene: the {side:?} spot is on dry ground");
    let id = s.scenario_spawn_now(side, card, pos, None).unwrap();
    for &(team, name, p) in extra {
        s.scenario_spawn_now(team, name, p, None).unwrap();
    }
    assert!(s.debug_set_hp(id, 0));
    s.tick();
    assert!(s.entity(id).is_none(), "the {card} died on the first tick");
    let (unit, n, _) = death_spawn(&s, card);
    let m = members_of(&s, side, &unit);
    assert_eq!(m.len(), n as usize, "the {card}'s {n} {unit}s exist on the death frame");
    (s, m)
}

fn native(p: Vec2) -> (i32, i32) {
    (p.x / K, p.y / K)
}

fn centre_of(s: &BattleState, ids: &[EntityId]) -> Vec2 {
    let n = ids.len() as i32;
    let sum = ids.iter().fold(Vec2::default(), |a, id| a.add(s.entity(*id).unwrap().pos));
    Vec2::new(sum.x / n, sum.y / n)
}

fn offset(s: &BattleState, id: EntityId, centre: Vec2) -> (i32, i32) {
    native(s.entity(id).expect("the member is alive").pos.sub(centre))
}

fn radius(o: (i32, i32)) -> i32 {
    isqrt((o.0 as i64) * (o.0 as i64) + (o.1 as i64) * (o.1 as i64)) as i32
}

/// The members' offsets at the MEASURED angles and the measured start radius, through the
/// engine's sine table, each axis truncated -- a set, since the corpus gives the members in
/// key order, which is not shown to be creation order.
fn measured_ring(angles: &[i32]) -> BTreeSet<(i32, i32)> {
    angles.iter().map(|&a| ring_point(a)).collect()
}

/// One member's offset at the measured angle `a` and the measured start radius.
fn ring_point(a: i32) -> (i32, i32) {
    (MEASURED_START * sin1024(a + 90) / 1024, MEASURED_START * sin1024(a) / 1024)
}

/// The members' radii from `c`, native, on the death frame and each of the next `ticks`.
fn tracks(s: &mut BattleState, ids: &[EntityId], c: Vec2, ticks: usize) -> Vec<Vec<i32>> {
    let mut t: Vec<Vec<i32>> = vec![Vec::new(); ids.len()];
    for tick in 0..=ticks {
        if tick > 0 {
            s.tick();
        }
        for (k, id) in ids.iter().enumerate() {
            t[k].push(radius(offset(s, *id, c)));
        }
    }
    t
}

// ---------------------------------------------------------------------------
// (1) the loader

#[test]
fn the_loader_reads_death_spawn_pushback_off_every_row() {
    // Plant death_spawn_pushback_unread: every row reads false.
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let db = cards();
    let row = |name: &str, summon_only: bool| -> Option<serde_json::Value> {
        if summon_only {
            doc["units"].get(name).cloned()
        } else {
            doc["cards"].as_array().unwrap().iter().chain(doc["towers"].as_array().unwrap().iter()).find(|c| c["name"] == name).cloned()
        }
    };
    let mut set = Vec::new();
    for i in 0..db.cards.len() {
        let c = db.get(i as u16);
        let Some(r) = row(&c.name, c.summon_only) else { continue };
        // absent (the 2018 file, a tower row, a spell) reads false
        let want = r.get("death_spawn_pushback").and_then(serde_json::Value::as_bool).unwrap_or(false);
        assert_eq!(c.death_spawn_pushback, want, "{}: the loader read {} where cards.json says {want}", c.name, c.death_spawn_pushback);
        if want {
            set.push(c.name.clone());
        }
    }
    // The measured cards and the control, by name, so a table that drops the column cannot
    // pass this by carrying nothing.
    for (card, want) in [("Golem", true), ("LavaHound", true), ("BattleRam", false)] {
        let idx = db.index(card).unwrap_or_else(|| panic!("{card} is not simulable"));
        assert_eq!(db.get(idx).death_spawn_pushback, want, "{card}'s DeathSpawnPushback");
    }
    assert!(set.len() >= 2, "vacuous: only {set:?} carry the column");
}

// ---------------------------------------------------------------------------
// (2) the ring

#[test]
fn the_members_are_born_on_the_fixed_ring_at_250_whatever_the_heading() {
    // Plant death_ring_slide_facing: they are born at DeathSpawnRadius on the facing ring.
    for (card, angles) in [("Golem", &[180, 0][..]), ("LavaHound", &[300, 240, 180, 120, 60, 0][..])] {
        let (s, ids) = kill(slide_config(), card, &[]);
        let (_, n, r) = death_spawn(&s, card);
        assert_eq!(angles.len(), n as usize, "data: {card}'s count");
        assert!(r > MEASURED_START, "data: {card}'s DeathSpawnRadius {r} leaves nothing to slide");
        let c = centre_of(&s, &ids);
        let got: BTreeSet<(i32, i32)> = ids.iter().map(|id| offset(&s, *id, c)).collect();
        assert_eq!(got, measured_ring(angles), "{card}: the members are not on the fixed ring at {MEASURED_START}");
        // The discrimination: the parent faces up the arena, toward Red, so a ring that
        // turned with it (the shipped facing ring) would sit near 90 + k x 360 / n -- 90
        // degrees off the pair's fixed angles and 30 off the six's. No member sits there.
        let facing: BTreeSet<(i32, i32)> = measured_ring(&(0..n).map(|k| 90 + k * 360 / n).collect::<Vec<_>>());
        assert!(got.is_disjoint(&facing), "{card}: a member sits where the facing ring would put it");
        for id in &ids {
            let e = s.entity(*id).unwrap();
            assert_eq!((e.death_slide_centre, e.death_slide_radius), (c, r * K), "{card}: the member carries its slide");
        }
    }
}

// ---------------------------------------------------------------------------
// (3) the slide

#[test]
fn the_golemites_slide_out_as_measured_then_walk() {
    // Plant death_slide_never: they walk from 250. Plant death_slide_unclamped: 1650 / 1601
    // on the fifth tick.
    let (mut s, ids) = kill(slide_config(), "Golem", &[]);
    let (_, _, r) = death_spawn(&s, "Golem");
    let c = centre_of(&s, &ids);
    let mut tracks: Vec<Vec<i32>> = ids.iter().map(|id| vec![radius(offset(&s, *id, c))]).collect();
    let mut sliding: Vec<Vec<bool>> = ids.iter().map(|id| vec![s.entity(*id).unwrap().death_slide_radius > 0]).collect();
    for _ in 0..7 {
        s.tick();
        for (k, id) in ids.iter().enumerate() {
            tracks[k].push(radius(offset(&s, *id, c)));
            sliding[k].push(s.entity(*id).unwrap().death_slide_radius > 0);
        }
    }
    // THE MEASURED TRACKS, as a set (the corpus's member order is key order): the first
    // step carries the contact law's push between the two newborns, 150 for the one that
    // moves first and 101 for the other, and every later step is the slide's own 250.
    let measured: BTreeSet<Vec<i32>> = [vec![250, 650, 900, 1150, 1400, 1500], vec![250, 601, 851, 1101, 1351, 1500]].into_iter().collect();
    let got: BTreeSet<Vec<i32>> = tracks.iter().map(|t| t[..6].to_vec()).collect();
    assert_eq!(got, measured, "the Golemites' radius per tick");
    assert_eq!(r, 1500, "data: the measured tracks end at the Golem's DeathSpawnRadius");
    for (k, t) in tracks.iter().enumerate() {
        // the law, stated without the numbers: after the first tick every step is the
        // slide's until the clamp, and the clamp lands on the radius itself
        for w in t[1..6].windows(2) {
            if w[0] + DEATH_SLIDE_STEP < r {
                assert_eq!(w[1] - w[0], DEATH_SLIDE_STEP, "member {k}: {t:?}");
            }
        }
        assert_eq!(t[5], r, "member {k} ends the slide on DeathSpawnRadius: {t:?}");
        assert_eq!(&sliding[k][..6], &[true, true, true, true, true, false], "member {k}: the slide ends on the tick it reaches the radius");
        assert!(sliding[k][6..].iter().all(|x| !x), "member {k}: the slide came back");
        // then the ordinary update: a walk, not another 250
        assert!((t[6] - t[5]).abs() < DEATH_SLIDE_STEP, "member {k} still slides after the radius: {t:?}");
    }
}

#[test]
fn the_pups_never_pass_the_radius_and_stop_on_it() {
    // Plant death_slide_unclamped: a Pup lands past 2500.
    let (mut s, ids) = kill(slide_config(), "LavaHound", &[]);
    let (_, _, r) = death_spawn(&s, "LavaHound");
    let c = centre_of(&s, &ids);
    let mut last: Vec<i32> = ids.iter().map(|id| radius(offset(&s, *id, c))).collect();
    let mut was: Vec<bool> = ids.iter().map(|id| s.entity(*id).unwrap().death_slide_radius > 0).collect();
    assert!(was.iter().all(|x| *x), "every Pup is born sliding");
    let mut ended = vec![None; ids.len()];
    for tick in 1..=14 {
        s.tick();
        for (k, id) in ids.iter().enumerate() {
            let now = radius(offset(&s, *id, c));
            let still = s.entity(*id).unwrap().death_slide_radius > 0;
            if was[k] {
                assert!(now > last[k], "Pup {k} tick {tick}: the slide went {} -> {now}, not outward", last[k]);
                assert!(now <= r, "Pup {k} tick {tick}: {now} past DeathSpawnRadius {r}");
                if !still {
                    // on the radius, to the per-axis truncation of an off-axis member
                    assert!(r - now <= 1, "Pup {k} ended its slide at {now}, not {r}");
                    ended[k] = Some(tick);
                }
            }
            last[k] = now;
            was[k] = still;
        }
    }
    assert!(ended.iter().all(Option::is_some), "a Pup never finished its slide: {ended:?}");
}

// ---------------------------------------------------------------------------
// (5) neither walking nor attacking

#[test]
fn a_sliding_member_takes_no_target_until_the_slide_is_over() {
    // Plant death_slide_targets: the Pups take the Knight on the first tick.
    let s0 = BattleState::new(7, slide_config());
    let at = spot(&s0);
    // A Red Knight 3500 native up the arena from the death point: in every Pup's sight, a
    // ground unit that cannot hit them and that they can hit, and nothing else of Red's in
    // sight. It is not in contact with any Pup (air and ground do not touch).
    let knight_at = Vec2::new(at.x, at.y + 3500 * K);
    assert!(s0.arena().is_passable_ground(knight_at), "scene: the Knight's spot is dry ground");
    let (mut s, ids) = kill(slide_config(), "LavaHound", &[(Team::Red, "Knight", knight_at)]);
    let knight = find_live(&s, Team::Red, "Knight")[0].id;
    let mut was: Vec<bool> = ids.iter().map(|id| s.entity(*id).unwrap().death_slide_radius > 0).collect();
    let mut took_it = false;
    let mut slid = 0;
    for tick in 1..=20 {
        s.tick();
        for (k, id) in ids.iter().enumerate() {
            let e = s.entity(*id).unwrap();
            if was[k] {
                // the Target phase ran while the member was still sliding
                slid += 1;
                assert_eq!(e.target, None, "Pup {k} took a target on tick {tick}, mid-slide");
            } else if e.target == Some(knight) {
                took_it = true;
            }
            was[k] = e.death_slide_radius > 0;
        }
    }
    assert!(slid > 0, "vacuous: no Pup was ever sliding");
    assert!(took_it, "vacuous: no Pup took the Knight once its slide was over");
}

// ---------------------------------------------------------------------------
// (6), (7) the controls

#[test]
fn the_battle_rams_barbarians_keep_the_facing_ring_under_the_new_arm() {
    // Plant death_ring_slide_ignores_flag: the Barbarians are born at 250 and slide.
    let mut s = BattleState::new(7, slide_config());
    let (unit, _, r) = death_spawn(&s, "BattleRam");
    assert!(!card_stat(&s, "BattleRam").death_spawn_pushback, "data: the Battle Ram leaves DeathSpawnPushback blank");
    let ram = s.scenario_spawn_now(Team::Blue, "BattleRam", spot(&s), None).unwrap();
    let mut death = None;
    for _ in 0..600 {
        let seen = s.entity(ram).map(|e| (e.pos, e.target));
        s.tick();
        if s.entity(ram).is_none() {
            let (pos, target) = seen.expect("the ram existed the tick before it died");
            let aim = target.map(|t| native(s.entity(t).unwrap().pos.sub(pos))).expect("the ram died without a target");
            death = Some((pos, aim));
            break;
        }
    }
    let (pos, aim) = death.expect("the ram never died");
    let ids = members(&s, &unit);
    assert_eq!(ids.len(), 2, "the ram's two {unit}s");
    let tol = (r / 100).max(4);
    for id in &ids {
        let e = s.entity(*id).unwrap();
        let o = native(e.pos.sub(pos));
        assert!((radius(o) - r).abs() <= tol, "a {unit} sits {} from the death point, not {r}", radius(o));
        // on the axis to the tower it hit: |o x aim| / |aim| is its distance from that line
        let cross = (o.0 as i64) * (aim.1 as i64) - (o.1 as i64) * (aim.0 as i64);
        let off = (cross.abs() / (radius(aim).max(1) as i64)) as i32;
        assert!(off <= tol, "a {unit} is {off} off the ram's axis");
        assert_eq!(e.death_slide_radius, 0, "a {unit} slides");
    }
}

#[test]
fn the_old_arm_is_todays_engine_and_ships() {
    // Plant death_ring_slide_ignores_arm: the Golemites and the Pups take the small ring and
    // slide under not_read too.
    assert_eq!(Calib::shipped().death_spawn_pushback, DeathSpawnPushback::NotRead, "the ledger ships the old arm, today's engine");
    assert_eq!(config().calib.death_spawn_pushback, DeathSpawnPushback::NotRead);
    for card in ["Golem", "LavaHound"] {
        let (mut s, ids) = kill(config(), card, &[]);
        let (_, _, r) = death_spawn(&s, card);
        let c = centre_of(&s, &ids);
        let tol = (r / 100).max(4);
        let first: Vec<i32> = ids.iter().map(|id| radius(offset(&s, *id, c))).collect();
        for (k, id) in ids.iter().enumerate() {
            assert!((first[k] - r).abs() <= tol, "{card}: member {k} at {} from the death point, not DeathSpawnRadius {r}", first[k]);
            assert_eq!(s.entity(*id).unwrap().death_slide_radius, 0, "{card}: member {k} slides under not_read");
        }
        s.tick();
        for (k, id) in ids.iter().enumerate() {
            let now = radius(offset(&s, *id, c));
            assert!((now - first[k]).abs() < DEATH_SLIDE_STEP, "{card}: member {k} moved {} -> {now} in one tick", first[k]);
        }
    }
}

// ---------------------------------------------------------------------------
// (8) save / load

#[test]
fn a_snapshot_mid_slide_resumes_hash_for_hash() {
    // Plant save_drops_death_slide: the load's hash self-check refuses the blob.
    let (mut s, ids) = kill(slide_config(), "LavaHound", &[]);
    for _ in 0..3 {
        s.tick();
    }
    let mid = ids.iter().filter(|id| s.entity(**id).unwrap().death_slide_radius > 0).count();
    assert!(mid > 0, "vacuous: no Pup is mid-slide at the save");
    let blob = s.save();
    let mut l = BattleState::load(&blob).unwrap_or_else(|e| panic!("a mid-slide snapshot does not load: {e}"));
    assert_eq!(l.state_hash(), s.state_hash());
    for k in 0..20 {
        s.tick();
        l.tick();
        assert_eq!(l.state_hash(), s.state_hash(), "diverged {k} ticks after the load");
        for id in &ids {
            let (a, b) = (s.entity(*id).unwrap(), l.entity(*id).unwrap());
            assert_eq!((a.pos, a.death_slide_centre, a.death_slide_radius), (b.pos, b.death_slide_centre, b.death_slide_radius), "a Pup's slide, {k} ticks after the load");
        }
    }
}

// ---------------------------------------------------------------------------
// (9) the frame-planned search

#[test]
fn under_the_frame_planned_search_the_slide_is_the_plain_radial_step() {
    // Plant frame_planned_slide_ignored: the Golemites stay at 250 and never finish. These
    // arms have no contact law inside the pass, so the first step is not the measured one;
    // the radial step is the same code as the 16.402 pass's (test 10 holds it to its clamp,
    // which this scene never reaches: 500, 750, 1000, 1250 and then exactly 1500).
    // `phase_path_2026` (the trace-fitted search) and the legacy `phase_path` each carry
    // their own copy of the slide branch, so both run.
    let planned = with_arm(symmetric_config(), DeathSpawnPushback::ClientRingSlide);
    let mut legacy = planned.clone();
    legacy.path_model = PathModel::LaneSnap;
    for (arm, cfg) in [("phase_path_2026", planned), ("phase_path, lane_snap", legacy)] {
        let (mut s, ids) = kill(cfg, "Golem", &[]);
        let (_, _, r) = death_spawn(&s, "Golem");
        let c = centre_of(&s, &ids);
        let sub = |s: &BattleState, id: EntityId| -> i32 {
            let o = s.entity(id).unwrap().pos.sub(c);
            isqrt((o.x as i64) * (o.x as i64) + (o.y as i64) * (o.y as i64)) as i32
        };
        let (step, rr) = (DEATH_SLIDE_STEP * K, r * K);
        let mut last: Vec<i32> = ids.iter().map(|id| sub(&s, *id)).collect();
        assert!(last.iter().all(|x| (x - MEASURED_START * K).abs() <= 1), "{arm}: born at the start radius: {last:?}");
        let mut done = vec![false; ids.len()];
        for tick in 1..=8 {
            let before: Vec<bool> = ids.iter().map(|id| s.entity(*id).unwrap().death_slide_radius > 0).collect();
            s.tick();
            for (k, id) in ids.iter().enumerate() {
                let now = sub(&s, *id);
                if before[k] {
                    assert!(now <= rr + 1, "{arm}: member {k} tick {tick}: {now} past {rr}");
                    if tick > 1 && last[k] + step < rr {
                        assert!((now - last[k] - step).abs() <= 1, "{arm}: member {k} tick {tick}: step {} not {step}", now - last[k]);
                    }
                    if s.entity(*id).unwrap().death_slide_radius == 0 {
                        assert!((now - rr).abs() <= 1, "{arm}: member {k} ended at {now}, not {rr}");
                        done[k] = true;
                    }
                }
                last[k] = now;
            }
        }
        assert!(done.iter().all(|x| *x), "{arm}: a member never finished its slide");
    }
}

// ---------------------------------------------------------------------------
// (10) the radial step, pure

#[test]
fn the_radial_step_stops_on_the_radius_and_never_pulls_a_member_back_in() {
    // Plant death_slide_unclamped: (1400, 0) steps to 1650. Plant death_slide_pulls_back:
    // (1800, 0) is pulled back to 1500 in one tick. The helper is the one both arms run.
    // a step short of the radius is the plain step, and not the end
    assert_eq!(death_slide_to((500, 0), (0, 0), 1500, 250), ((750, 0), false));
    // a step that lands exactly on the radius ends the slide there
    assert_eq!(death_slide_to((1250, 0), (0, 0), 1500, 250), ((1500, 0), true));
    // THE CLAMP: a step that would pass the radius stops on it, on either side of the centre
    assert_eq!(death_slide_to((1400, 0), (0, 0), 1500, 250), ((1500, 0), true));
    assert_eq!(death_slide_to((-1400, 0), (0, 0), 1500, 250), ((-1500, 0), true));
    // off the axis and off the origin: on the ray (1000, 1000), on the radius to the per-axis
    // truncation (1500 x 1000 / 1414 = 1060.8 -> 1060 on each axis, 1499 from the centre)
    let (centre, p) = ((100, -200), (1100, 800));
    let (q, done) = death_slide_to(p, centre, 1500, 250);
    let o = (q.0 - centre.0, q.1 - centre.1);
    assert!(done, "the clamped step ends the slide");
    assert_eq!(o.0, o.1, "off the ray: {o:?}");
    assert!((radius(o) - 1500).abs() <= 1, "{o:?} is {} from the centre", radius(o));
    // a member on the radius, or already past it, stays where it is and ends the slide
    assert_eq!(death_slide_to((1500, 0), (0, 0), 1500, 250), ((1500, 0), true));
    assert_eq!(death_slide_to((1800, 0), (0, 0), 1500, 250), ((1800, 0), true));
    // a member on the centre has no ray: it stays, and the slide ends
    assert_eq!(death_slide_to((5, 5), (5, 5), 1500, 250), ((5, 5), true));
}

// ---------------------------------------------------------------------------
// (11), (12) OPEN pins: the engine's reading where the measurement does not reach

#[test]
fn open_member_k_in_creation_order_takes_angle_k_and_the_first_golemite_the_650_track() {
    // OPEN. Plant death_ring_angle_sign: the Pups run the other way round (member 0 at 60),
    // which tests 2 and 3, comparing sets, pass.
    for (card, angles) in [("Golem", &[180, 0][..]), ("LavaHound", &[300, 240, 180, 120, 60, 0][..])] {
        let (s, ids) = kill(slide_config(), card, &[]);
        let c = centre_of(&s, &ids);
        let got: Vec<(i32, i32)> = ids.iter().map(|id| offset(&s, *id, c)).collect();
        let want: Vec<(i32, i32)> = angles.iter().map(|&a| ring_point(a)).collect();
        assert_eq!(got, want, "{card}: member k in creation order is not at -(k + 1) x 360 / n");
    }
    // the first created moves first in the creation-order pass: it takes the 150 cap
    let (mut s, ids) = kill(slide_config(), "Golem", &[]);
    let c = centre_of(&s, &ids);
    let got = tracks(&mut s, &ids, c, 5);
    assert_eq!(got, vec![vec![250, 650, 900, 1150, 1400, 1500], vec![250, 601, 851, 1101, 1351, 1500]], "the Golemites' tracks in creation order");
}

#[test]
fn open_a_red_death_lays_the_same_absolute_ring_as_a_blue_one() {
    // OPEN: every measured death is side 0. Plant death_ring_seat_rotated: Red's ring is the
    // seat rotation of Blue's, its first-created Golemite at 0.
    let (mut s, ids) = kill_on(slide_config(), Team::Red, "Golem", &[]);
    let c = centre_of(&s, &ids);
    let got: Vec<(i32, i32)> = ids.iter().map(|id| offset(&s, *id, c)).collect();
    assert_eq!(got, vec![ring_point(180), ring_point(0)], "Red's Golemites in creation order, in the absolute native frame");
    let t = tracks(&mut s, &ids, c, 5);
    assert_eq!(t, vec![vec![250, 650, 900, 1150, 1400, 1500], vec![250, 601, 851, 1101, 1351, 1500]], "Red's Golemites' tracks in creation order");
}

// ---------------------------------------------------------------------------
// (13) a building death spawn

#[test]
fn a_flagged_row_whose_death_spawn_is_a_building_takes_no_slide() {
    // Plant death_slide_on_a_building: the Cannons are born on the small ring carrying a
    // slide that only the troop Path loops would end. No loaded row is like this: the Golem
    // is doctored to leave two Cannons.
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let golem = doc["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "Golem").expect("data: a Golem row");
    golem["death_spawn"]["character"] = serde_json::Value::from("Cannon");
    golem["death_spawn_pushback"] = serde_json::Value::Bool(true);
    let db = CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).expect("the doctored file still parses");
    let g = db.index("Golem").unwrap_or_else(|| panic!("the doctored Golem does not load: {:?}", db.rejected.iter().find(|(n, _)| n == "Golem")));
    let ds = db.get(g).death_spawn.expect("scene: the doctored Golem has a death spawn");
    assert_eq!((db.get(ds.unit).name.as_str(), db.get(ds.unit).kind), ("Cannon", CardKind::Building), "scene: the doctored death spawn");
    assert!(db.get(g).death_spawn_pushback, "scene: the doctored Golem carries the flag");
    let (s, ids) = kill(with_arm(BattleConfig::with_cards(db), DeathSpawnPushback::ClientRingSlide), "Golem", &[]);
    for id in &ids {
        let e = s.entity(*id).unwrap();
        assert_eq!((e.death_slide_centre, e.death_slide_radius), (Vec2::default(), 0), "a Cannon carries a slide");
    }
}
