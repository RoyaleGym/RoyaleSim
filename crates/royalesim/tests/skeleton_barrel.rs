//! THE SKELETON BARREL (card.rs `Hitpointless::BombWithDeathSpawn`, `CardDef::kamikaze_time_ms`,
//! `CardDef::death_pushback`; spell.rs `step_spells`, `FuseEnd`, `death_bomb_push`; state.rs `release_fuse_end`,
//! `container_slide_ring`, `start_kamikaze_drain`, `kamikaze_drain`; entity.rs `death_slide_until`, `kamikaze_from`).
//!
//! THE LAW, measured on client 15.535.29 (18 barrel runs, level 11). T is the first tick the barrel is absent, C its
//! point after its move on T:
//!   - it flies to a building and fires on the tick after its first attacking tick; from that fire it loses 53 a tick
//!     (floor(532 x 50 / 500), nothing carried) and is gone on the fire + 10. Its own hits deal 0;
//!   - its container (SkeletonContainerNew) is no entity. On T + 12 it hits for 145 (57 at level 11) within 2000 of C
//!     to the victim's edge, air and ground, deploying units too, and its seven Skeletons (81 hp) appear;
//!   - the seven appear 250 from C: side 0 on the left half at (-153, -196), (55, -243), (226, -105), (224, 109),
//!     (51, 244), (-157, 194), (-250, 0) in creation order, x negated on the right half, y negated on side 1. They slide
//!     out on T + 13 to T + 16, to DeathSpawnRadius 1480 at most, and not after: the last-created rests at 1301;
//!   - a slot on the river is put on the bank, x kept;
//!   - from T + 13 the units the hit landed on are pushed by the knockback ladder for DeathPushBack 1000: 200, 175,
//!     150, 125, 100, 75, 50, 25, 0, then 25 back;
//!   - the Skeletons deploy on T + 12 to T + 21 and first step on T + 23; an enemy first targets one on T + 19.
//!
//! THE ARMS. combat.KAMIKAZE_TIME ships flat_drain_to_zero, spawner.DEATH_BOMB_SPAWN_TIMING at_fuse_end and
//! knockback.DEATH_PUSHBACK containers_ladder, the measured arms (no card that loaded before this change reads them).
//! spawner.DEATH_SPAWN_PUSHBACK ships client_ring_slide, the measured arm, since parity scored its flip (it had shipped
//! not_read, its old arm, while the slide's move of the Golem and the Lava Hound waited for that score). The ring tests
//! still select client_ring_slide by name (`ring_slide`), and the old arm's test selects not_read (`old_ring`).
//! Every test names its arms through its config (`shipped`, `with`, `ring_slide`, `old_ring`).
//!
//! WHAT IS PINNED, and the plants that turn each red
//! (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test skeleton_barrel`):
//!   1. the barrel and its container load, with the container's columns; the 2018 barrel and two other hitpoint-less
//!      rows stay refused -- container_not_a_bomb;
//!   2. the drain: 53 a tick from the fire, gone on the fire + 10, the Cannon it attacks untouched (shipped
//!      flat_drain_to_zero) -- kamikaze_time_not_taken, kamikaze_drain_carries_remainder;
//!   3. flat_drain_then_expire gives the same for a full-hp barrel -- kamikaze_time_not_taken,
//!      kamikaze_drain_carries_remainder;
//!   4. not_taken, the old arm: the barrel keeps attacking and keeps its hitpoints;
//!   5. the Skeletons and the hit on T + 12 (shipped at_fuse_end) -- container_release_with_hit, release_deferred;
//!   6. units_at_fuse_end: the Skeletons on T + 12, the hit on T + 13 -- container_release_with_hit, release_deferred;
//!   7. with_the_hit, the old arm: both on T + 13 -- release_deferred;
//!   8. the ring on both sides and both halves (client_ring_slide) -- container_ring_no_mirror,
//!      container_ring_troop_orientation (and death_ring_slide_facing, which lays the ring at 1480);
//!   9. the slide: four ticks, the last-created member at 1250-1400 (client_ring_slide) -- container_slide_uncapped,
//!      death_slide_never;
//!  10. not_read, the old arm: the Skeletons at DeathSpawnRadius, no slide -- death_ring_slide_ignores_arm;
//!  11. the hit's reach: 2434 to a 500-radius Knight's centre hit, 3086 missed, a deploying flier hit, the ring at 81
//!      -- aoe_centre_to_centre;
//!  12. the push (shipped containers_ladder) -- death_pushback_unread;
//!  13. not_read, the old arm: no push;
//!  14. every_death_bomb_ladder pushes from the Giant Skeleton's bomb and the shipped arm does not
//!      -- death_pushback_unread;
//!  15. an enemy Cannon first targets a Skeleton on T + 19 -- container_members_acquire_at_once;
//!  16. the Skeletons deploy on T + 12 to T + 21 and first step on T + 23 -- release_deferred;
//!  17. a river slot on the bank, x kept (client_ring_slide) -- container_water_unejected;
//!  18. saved mid-fuse, mid-slide and mid-drain, a battle resumes hash for hash, and the slide's cap and the drain's
//!      start are state (an edited save fails the load's hash self-check) -- hash_skips_slide_deadline,
//!      save_drops_death_slide (the mid-slide one), hash_skips_kamikaze (the mid-drain one);
//!  19. a container a unit covers bursts where the contact law pushes it, its members' slides ending around its own
//!      point (spawner.CONTAINER_BURST_PUSH = client15535_contact_push) -- container_burst_unpushed,
//!      container_slide_from_pushed.
//!
//! THE SCENES. A barrel is put down with `scenario_spawn_now` and killed with `debug_set_hp` on the next tick (T); C is
//! read off its container's spell, since a barrel with no building in reach moves on that tick. Victims are played
//! with `spawn_unit` on T, so they deploy (and stand still) through T + 19, and are then put at their exact offset
//! from C. Enemy victims are Red units on Red's own half, where no crown tower of their own shoots them and Blue's are
//! out of reach; a Blue scene with no victim is on Blue's own half.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardKind};
use royalesim::fixed::{isqrt, milli, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{ContainerBurstPush, AttackCycle, BattleConfig, BattleState, Calib, DeathBombSpawnTiming, DeathPushbackScope, DeathSlideBirth, DeathSpawnPushback, KamikazeTime};
use royalesim::{EntityId, Team};

const BARREL: &str = "SkeletonBalloon";

/// Every measurement is at level 11.
const LEVEL: i32 = 11;

/// The seven offsets from C, native, in creation order: side 0, a death on the left half. Measured on client
/// 15.535.29.
const RING_SIDE0_LEFT: [(i32, i32); 7] = [(-153, -196), (55, -243), (226, -105), (224, 109), (51, 244), (-157, 194), (-250, 0)];

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn native(v: Vec2) -> (i32, i32) {
    (v.x / K, v.y / K)
}

fn radius_of(o: (i32, i32)) -> i32 {
    isqrt((o.0 as i64) * (o.0 as i64) + (o.1 as i64) * (o.1 as i64)) as i32
}

/// The shipped arms of the barrel's keys, asserted, at level 11, so a ledger change re-points this file instead of
/// silently moving its numbers.
fn shipped() -> BattleConfig {
    let mut cfg = config();
    cfg.card_level = [LEVEL, LEVEL];
    let c = &cfg.calib;
    assert_eq!(c.kamikaze_time, KamikazeTime::FlatDrainToZero, "the shipped combat.KAMIKAZE_TIME");
    assert_eq!(c.death_bomb_spawn_timing, DeathBombSpawnTiming::AtFuseEnd, "the shipped spawner.DEATH_BOMB_SPAWN_TIMING");
    assert_eq!(c.death_pushback, DeathPushbackScope::ContainersLadder, "the shipped knockback.DEATH_PUSHBACK");
    assert_eq!(c.death_spawn_pushback, DeathSpawnPushback::ClientRingSlide, "the shipped spawner.DEATH_SPAWN_PUSHBACK");
    assert_eq!(c.attack_cycle, AttackCycle::ProgressCredit, "the shipped combat.ATTACK_CYCLE, whose counter the fire is read on");
    // The ring points were pinned with the members born through the sine table (spawner.DEATH_SLIDE_BIRTH =
    // sine_table); the shipped birth one step toward the end point moves the diagonal ones by a unit.
    cfg.calib.death_slide_birth = DeathSlideBirth::SineTable;
    cfg
}

/// The shipped config with the arms the test names.
fn with(f: impl FnOnce(&mut Calib)) -> BattleConfig {
    let mut cfg = shipped();
    f(&mut cfg.calib);
    cfg
}

/// spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide, the container ring's measured arm (the shipped one).
fn ring_slide() -> BattleConfig {
    with(|c| c.death_spawn_pushback = DeathSpawnPushback::ClientRingSlide)
}

/// spawner.DEATH_SPAWN_PUSHBACK = not_read, the old arm.
fn old_ring() -> BattleConfig {
    with(|c| c.death_spawn_pushback = DeathSpawnPushback::NotRead)
}

/// The container record the barrel's death leaves.
fn container_idx(s: &BattleState) -> u16 {
    let db = s.cards();
    db.get(db.index(BARREL).expect("the Skeleton Barrel loads")).death_spawn.expect("its death spawn").unit
}

/// The point the container stands on, while it waits out its fuse.
fn container_point(s: &BattleState) -> Option<Vec2> {
    let c = container_idx(s);
    s.spells().iter().find(|sp| sp.card == c).map(|sp| match &sp.motion {
        SpellMotion::Flight { aim, .. } => *aim,
        m => panic!("the container is not a Flight: {m:?}"),
    })
}

/// A `team` Skeleton Barrel put at `p` (native) and killed on the next tick. Returns T, the tick it died on (the first
/// tick it is absent), and C, the point its container stands on.
fn kill_barrel(s: &mut BattleState, team: Team, p: (i32, i32)) -> (u32, Vec2) {
    let id = s.scenario_spawn_now(team, BARREL, at(p), None).expect("the barrel is placed");
    assert!(s.debug_set_hp(id, 0));
    let t = s.tick_count();
    s.tick();
    assert!(s.entity(id).is_none(), "the barrel died on its tick");
    let c = container_point(s).expect("the barrel's death left its container");
    (t, c)
}

/// Tick until the battle stands after tick `tick`.
fn after(s: &mut BattleState, tick: u32) {
    while s.tick_count() <= tick {
        s.tick();
    }
}

/// The container's Skeletons of `team`, in creation order.
fn members(s: &BattleState, team: Team) -> Vec<EntityId> {
    let mut m: Vec<(u32, EntityId)> = s.entities().filter(|v| v.team == team && v.card == "Skeleton" && v.spawned_by.is_none()).map(|v| (v.team_seq, v.id)).collect();
    m.sort();
    m.into_iter().map(|(_, id)| id).collect()
}

/// Entity `id`'s offset from `c`, native.
fn offset(s: &BattleState, id: EntityId, c: Vec2) -> (i32, i32) {
    native(s.entity(id).expect("the member is alive").pos.sub(c))
}

/// The one live `card` of `team`.
fn the(s: &BattleState, team: Team, card: &str) -> EntityId {
    let v: Vec<EntityId> = s.entities().filter(|v| v.team == team && v.card == card).map(|v| v.id).collect();
    assert_eq!(v.len(), 1, "one {card} of {team:?}: {v:?}");
    v[0]
}

fn hp(s: &BattleState, id: EntityId) -> i32 {
    s.entity(id).map_or(0, |v| v.hp)
}

// ---------------------------------------------------------------------------
// (1) the loader

#[test]
fn the_barrel_and_its_container_load() {
    // Plant container_not_a_bomb: the container is refused on its hitpoints, and the barrel with it.
    let db = cards();
    let idx = db.index(BARREL).unwrap_or_else(|| panic!("{BARREL} refused: {:?}", db.rejected.iter().find(|(n, _)| n == BARREL)));
    let b = db.get(idx);
    assert!(!b.kamikaze, "a delayed kamikaze is not the death on the fire (combat.KAMIKAZE_DEATH)");
    assert_eq!(b.kamikaze_time_ms, 500, "KamikazeTime");
    assert!(b.is_flying() && b.target_only_buildings, "it flies and goes for buildings");
    assert_eq!(db.scaled(idx, LEVEL, b.hitpoints).unwrap(), 532, "its hitpoints at level 11, measured 532");
    let ds = b.death_spawn.expect("its death spawn");
    assert_eq!(ds.count, 1, "one container");
    let c = db.get(ds.unit);
    assert_eq!(c.name, "SkeletonContainerNew");
    assert!(c.summon_only && c.kind == CardKind::Building, "the container is a summon-only building record");
    assert_eq!(c.death_bomb_fuse_ms(), Some(600), "a death bomb whose fuse is its DeployTime");
    assert_eq!(c.death_damage, 57);
    assert_eq!(db.scaled(ds.unit, LEVEL, c.death_damage).unwrap(), 145, "its hit at level 11, measured 145");
    assert_eq!(c.death_damage_radius, milli(2000), "DeathDamageRadius");
    assert_eq!(c.death_pushback, milli(1000), "DeathPushBack");
    assert!(c.death_spawn_pushback, "DeathSpawnPushback");
    let inner = c.death_spawn.expect("the container's own death spawn");
    assert_eq!(db.get(inner.unit).name, "Skeleton");
    assert_eq!((inner.count, inner.radius, inner.deploy_time_ms), (7, Some(milli(1480)), Some(500)), "count, DeathSpawnRadius, DeathSpawnDeployTime");
    assert_eq!(db.scaled(inner.unit, LEVEL, db.get(inner.unit).hitpoints).unwrap(), 81, "a Skeleton at level 11, measured 81");
    // The other hitpoint-less rows keep their refusals: a bottle is no container (the Super Mini PEKKA's pancake, a
    // spawner's bottle with a body, loads as `bottle_body`'s, item 298).
    for (card, why) in [("SuperHogRider", "units.SantaPresent")] {
        let got = db.rejected.iter().find(|(n, _)| n == card).map(|(_, w)| w.clone());
        assert!(got.as_deref().is_some_and(|w| w.starts_with(why)), "{card}: {got:?}");
    }
    // The 2018 barrel's container has no DeathDamage and a Spawn* block: no bomb, so the barrel stays refused.
    let old = CardDb::load_repo_file("cards-2018.json").unwrap_or_else(|e| panic!("{e} (tools/extract_cards.py --vintage 2018)"));
    assert!(old.index(BARREL).is_none(), "the 2018 barrel loads");
    let why = old.rejected.iter().find(|(n, _)| n == BARREL).map(|(_, w)| w.clone());
    assert!(why.as_deref().is_some_and(|w| w.starts_with("units.SkeletonContainer")), "the 2018 barrel: {why:?}");
}

// ---------------------------------------------------------------------------
// (2)-(4) the drain

/// The drain scene: a side-1 Cannon at (7500, 18500) and a side-0 barrel standing where the client's stopped,
/// (7893, 17176), in reach of it already (Range 350 + the radii 500 and 600 = 1450; it is 1381 away). The Cannon
/// shoots only ground units, and no crown tower reaches the barrel there (a princess tower's 7500 + 1000 + 500 falls
/// short of its 9413).
fn drain_scene(cfg: BattleConfig, barrel: bool) -> (BattleState, EntityId, Option<EntityId>) {
    let mut s = BattleState::new(3, cfg);
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", at((7500, 18500)), None).expect("the Cannon");
    let b = if barrel { Some(s.scenario_spawn_now(Team::Blue, BARREL, at((7893, 17176)), None).expect("the barrel")) } else { None };
    (s, cannon, b)
}

/// Per tick run, from the first: the barrel's (hp, attack_ms, attack_load_ms) or None once it is gone, and the
/// Cannon's hp.
type Track = Vec<(Option<(i32, i32, i32)>, i32)>;

fn drain_track(cfg: BattleConfig, barrel: bool, ticks: u32) -> Track {
    let (mut s, cannon, b) = drain_scene(cfg, barrel);
    let mut out = Vec::new();
    for _ in 0..ticks {
        s.tick();
        let bv = b.and_then(|id| s.entity(id)).map(|v| (v.hp, v.attack_ms, v.attack_load_ms));
        out.push((bv, hp(&s, cannon)));
    }
    out
}

/// The index in `track` of the barrel's fire: its first tick below full hitpoints.
fn fire_of(track: &Track, full: i32) -> usize {
    track.iter().position(|(b, _)| b.is_some_and(|b| b.0 < full)).expect("the barrel never lost a hitpoint: its drain never started")
}

/// The measured drain of a full-hp barrel from its fire `d`: 479 ... 2 on d .. d + 9, gone on d + 10.
fn assert_measured_drain(track: &Track, d: usize, arm: &str) {
    assert!(d >= 1, "{arm}: the barrel lost hitpoints on its first tick");
    assert_eq!(track[d - 1].0.map(|b| (b.0, b.1, b.2)), Some((532, 250, 200)), "{arm}: the tick before the fire, full hp, progress 250 and load 200");
    assert_eq!(track[d].0.map(|b| b.1), Some(300), "{arm}: the drain starts on the fire (progress 300)");
    let want: Vec<i32> = (1..=10).map(|k| 532 - 53 * k).collect();
    let got: Vec<i32> = (d..d + 10).map(|k| track[k].0.unwrap_or_else(|| panic!("{arm}: the barrel died early, on the fire + {}", k - d)).0).collect();
    assert_eq!(got, want, "{arm}: the barrel's hitpoints from its fire (measured 479, 426, ..., 55, 2)");
    assert!(track[d + 10].0.is_none(), "{arm}: the barrel is not gone on the fire + 10");
}

#[test]
fn the_kamikaze_drains_53_a_tick_from_its_fire_and_is_gone_ten_ticks_later() {
    // Shipped flat_drain_to_zero. Plants kamikaze_time_not_taken (no drain at all) and kamikaze_drain_carries_remainder
    // (54 on the fire + 4).
    let track = drain_track(shipped(), true, 40);
    let control = drain_track(shipped(), false, 40);
    let d = fire_of(&track, 532);
    assert_measured_drain(&track, d, "flat_drain_to_zero");
    // Its hits deal 0: the Cannon it attacks holds the hitpoints of the same Cannon with no barrel, tick for tick,
    // through the barrel's death.
    for (k, (here, alone)) in track.iter().zip(&control).take(d + 11).enumerate() {
        assert_eq!(here.1, alone.1, "tick {k}: the Cannon lost hitpoints to the barrel");
    }
}

#[test]
fn the_expire_arm_drains_a_full_hp_barrel_the_same() {
    // combat.KAMIKAZE_TIME = flat_drain_then_expire: ten drains counting the fire's, then the rest in one hit, which at
    // full hp is the same last tick. Plants kamikaze_time_not_taken, kamikaze_drain_carries_remainder.
    let track = drain_track(with(|c| c.kamikaze_time = KamikazeTime::FlatDrainThenExpire), true, 40);
    let d = fire_of(&track, 532);
    assert_measured_drain(&track, d, "flat_drain_then_expire");
}

#[test]
fn the_old_kamikaze_arm_keeps_the_barrel_attacking_at_full_hitpoints() {
    // combat.KAMIKAZE_TIME = not_taken, the engine before the key: KamikazeTime is not read.
    let track = drain_track(with(|c| c.kamikaze_time = KamikazeTime::NotTaken), true, 40);
    for (k, (b, _)) in track.iter().enumerate() {
        let b = b.unwrap_or_else(|| panic!("tick {k}: the barrel died under not_taken"));
        assert_eq!(b.0, 532, "tick {k}: the barrel lost hitpoints under not_taken");
    }
    let last = track.last().and_then(|t| t.0).map(|b| b.1).unwrap_or(0);
    assert!(last > 900, "vacuous: the barrel did not keep attacking (progress {last} after 40 ticks)");
}

// ---------------------------------------------------------------------------
// (5)-(7) the release and the hit

/// The release scene: a Red Knight deploying 1500 above C on Red's half, a Blue barrel killed at (9000, 20000).
/// Returns the battle after T, T, and the Knight with its full hp.
fn release_scene(cfg: BattleConfig) -> (BattleState, u32, EntityId, i32) {
    let mut s = BattleState::new(5, cfg);
    s.spawn_unit(Team::Red, "Knight", at((9000, 21500)), None).expect("a Red Knight");
    let (t, c) = kill_barrel(&mut s, Team::Blue, (9000, 20000));
    let knight = the(&s, Team::Red, "Knight");
    assert!(s.debug_set_pos(knight, c.add(at((0, 1500)))));
    let (deploying, full) = s.entity(knight).map(|v| (v.deploying, v.hp)).expect("the Knight");
    assert!(deploying, "scene: the Knight deploys, so it stands still");
    (s, t, knight, full)
}

/// (Skeletons, the Knight's loss) after each of T + 11, T + 12 and T + 13.
fn release_track(cfg: BattleConfig) -> [(usize, i32); 3] {
    let (mut s, t, knight, full) = release_scene(cfg);
    let mut out = [(0, 0); 3];
    for (k, o) in out.iter_mut().enumerate() {
        after(&mut s, t + 11 + k as u32);
        *o = (members(&s, Team::Blue).len(), full - hp(&s, knight));
    }
    out
}

#[test]
fn the_container_releases_its_skeletons_and_hits_on_t_plus_12() {
    // Shipped at_fuse_end. Plants container_release_with_hit (both on T + 13) and release_deferred (the Skeletons on
    // T + 13).
    let (mut s, t, knight, full) = release_scene(shipped());
    after(&mut s, t + 11);
    assert!(members(&s, Team::Blue).is_empty(), "a Skeleton before T + 12");
    assert_eq!(hp(&s, knight), full, "the hit before T + 12");
    after(&mut s, t + 12);
    let m = members(&s, Team::Blue);
    assert_eq!(m.len(), 7, "the seven Skeletons on T + 12");
    for id in &m {
        let v = s.entity(*id).expect("a Skeleton");
        assert_eq!((v.hp, v.max_hp), (81, 81), "a Skeleton at level 11");
    }
    assert_eq!(full - hp(&s, knight), 145, "the container's hit on T + 12");
}

#[test]
fn under_units_at_fuse_end_the_skeletons_come_a_tick_before_the_hit() {
    // spawner.DEATH_BOMB_SPAWN_TIMING = units_at_fuse_end. Plants container_release_with_hit, release_deferred.
    let got = release_track(with(|c| c.death_bomb_spawn_timing = DeathBombSpawnTiming::UnitsAtFuseEnd));
    assert_eq!(got, [(0, 0), (7, 0), (7, 145)], "(Skeletons, the Knight's loss) after T + 11, T + 12, T + 13");
}

#[test]
fn under_the_old_timing_both_come_on_t_plus_13() {
    // spawner.DEATH_BOMB_SPAWN_TIMING = with_the_hit, the engine before the key (a plain bomb's landing).
    let got = release_track(with(|c| c.death_bomb_spawn_timing = DeathBombSpawnTiming::WithTheHit));
    assert_eq!(got, [(0, 0), (0, 0), (7, 145)], "(Skeletons, the Knight's loss) after T + 11, T + 12, T + 13");
}

// ---------------------------------------------------------------------------
// (8)-(10) the ring and the slide

#[test]
fn the_container_ring_is_the_measured_seven_mirrored_by_half_and_turned_by_side() {
    // client_ring_slide. Plants container_ring_no_mirror (the left-half deaths), container_ring_troop_orientation.
    for (team, p, sx, sy) in [(Team::Blue, (6000, 10000), 1, 1), (Team::Blue, (12000, 10000), -1, 1), (Team::Red, (6000, 22000), 1, -1), (Team::Red, (12000, 22000), -1, -1)] {
        let mut s = BattleState::new(9, ring_slide());
        let (t, c) = kill_barrel(&mut s, team, p);
        assert_eq!(native(c).0 < 9000, p.0 < 9000, "scene: {team:?} {p:?} died on the other half");
        after(&mut s, t + 12);
        let m = members(&s, team);
        assert_eq!(m.len(), 7, "{team:?} {p:?}: the seven Skeletons on T + 12");
        for (k, id) in m.iter().enumerate() {
            let got = offset(&s, *id, c);
            let want = (sx * RING_SIDE0_LEFT[k].0, sy * RING_SIDE0_LEFT[k].1);
            assert!((got.0 - want.0).abs() <= 1 && (got.1 - want.1).abs() <= 1, "{team:?} {p:?}: member {k} at {got:?} from C, measured {want:?}");
        }
    }
}

#[test]
fn the_container_ring_slides_out_for_four_ticks_and_stops() {
    // client_ring_slide. Plants container_slide_uncapped (the last-created member slides on to 1480 on T + 17),
    // death_slide_never (nothing moves on T + 13).
    let mut s = BattleState::new(9, ring_slide());
    let (t, c) = kill_barrel(&mut s, Team::Blue, (6000, 10000));
    after(&mut s, t + 12);
    let m = members(&s, Team::Blue);
    assert_eq!(m.len(), 7, "the seven Skeletons on T + 12");
    let mut track: Vec<Vec<(i32, i32)>> = m.iter().map(|id| vec![offset(&s, *id, c)]).collect();
    for _ in 13..=21 {
        s.tick();
        for (k, id) in m.iter().enumerate() {
            track[k].push(offset(&s, *id, c));
        }
    }
    for (k, tr) in track.iter().enumerate() {
        let r0 = radius_of(tr[0]);
        assert!((249..=250).contains(&r0), "member {k}: {r0} from C on T + 12, measured 249-250");
        for (j, w) in tr.windows(2).enumerate() {
            let tick = 13 + j;
            if tick <= 16 {
                assert_ne!(w[1], w[0], "member {k} did not slide on T + {tick}");
            } else {
                assert_eq!(w[1], w[0], "member {k} moved on T + {tick}, after its four slide ticks");
            }
        }
        for (j, p) in tr.iter().enumerate() {
            assert!(radius_of(*p) <= 1480 + 150, "member {k} at {} from C on T + {}", radius_of(*p), 12 + j);
        }
    }
    let last = radius_of(track[6][4]);
    assert!((1250..=1400).contains(&last), "the last-created member rests at {last}, measured 1301");
}

#[test]
fn the_old_arm_lays_the_container_ring_at_its_radius_with_no_slide() {
    // not_read, the old arm (DEATH_SPAWN_LAYOUT's ring at DeathSpawnRadius). Plant death_ring_slide_ignores_arm (the
    // ring starts at 250 and slides).
    let mut s = BattleState::new(9, old_ring());
    let (t, c) = kill_barrel(&mut s, Team::Blue, (6000, 10000));
    after(&mut s, t + 12);
    let m = members(&s, Team::Blue);
    assert_eq!(m.len(), 7, "the seven Skeletons on T + 12");
    let first: Vec<(i32, i32)> = m.iter().map(|id| offset(&s, *id, c)).collect();
    for (k, o) in first.iter().enumerate() {
        assert!((1477..=1480).contains(&radius_of(*o)), "member {k} at {} from C, not on DeathSpawnRadius 1480", radius_of(*o));
    }
    for tick in 13..=21 {
        s.tick();
        for (k, id) in m.iter().enumerate() {
            assert_eq!(offset(&s, *id, c), first[k], "member {k} moved on T + {tick} under not_read");
        }
    }
}

// ---------------------------------------------------------------------------
// (11) the hit's reach

#[test]
fn the_container_hits_to_the_edge_air_and_deploying_units_and_spares_its_ring() {
    // Shipped (spells.AOE_HIT_TEST = edge_inclusive). Plant aoe_centre_to_centre (the Knight at 2434 is missed).
    let mut s = BattleState::new(5, shipped());
    s.spawn_unit(Team::Red, "Knight", at((11000, 20000)), None).expect("a Red Knight");
    s.spawn_unit(Team::Red, "Knight", at((9000, 23000)), None).expect("a second Red Knight");
    s.spawn_unit(Team::Red, "MegaMinion", at((9000, 18500)), None).expect("a Red Mega Minion");
    let (t, c) = kill_barrel(&mut s, Team::Blue, (9000, 20000));
    let mut knights: Vec<(u32, EntityId)> = s.entities().filter(|v| v.team == Team::Red && v.card == "Knight").map(|v| (v.team_seq, v.id)).collect();
    knights.sort();
    assert_eq!(knights.len(), 2, "scene: two Red Knights");
    let (near, far) = (knights[0].1, knights[1].1);
    let flier = the(&s, Team::Red, "MegaMinion");
    assert!(s.debug_set_pos(near, c.add(at((2434, 0)))));
    assert!(s.debug_set_pos(far, c.add(at((0, 3086)))));
    assert!(s.debug_set_pos(flier, c.add(at((0, -1500)))));
    for id in [near, far] {
        assert_eq!(s.entity(id).expect("a Knight").radius / K, 500, "scene: the victim's radius is the measured 500");
    }
    let full: Vec<i32> = [near, far, flier].iter().map(|id| hp(&s, *id)).collect();
    after(&mut s, t + 11);
    for id in [near, far, flier] {
        assert!(s.entity(id).expect("a victim").deploying, "scene: every victim still deploys on T + 11, so none has moved");
    }
    after(&mut s, t + 12);
    let lost: Vec<i32> = [near, far, flier].iter().zip(&full).map(|(id, f)| f - hp(&s, *id)).collect();
    assert_eq!(lost, vec![145, 0, 145], "the losses on T + 12 of a Knight 2434 from C, one 3086 from C and a deploying Mega Minion 1500 from C");
    for id in members(&s, Team::Blue) {
        assert_eq!(hp(&s, id), 81, "the container's own Skeleton lost hitpoints");
    }
}

// ---------------------------------------------------------------------------
// (12)-(14) the push

/// A Red Mega Minion deploying 2159 below C on the axis (the client's Minion). Returns its distance from C, native,
/// after each of T + 12 .. T + 22.
fn push_track(cfg: BattleConfig) -> Vec<i32> {
    let mut s = BattleState::new(5, cfg);
    s.spawn_unit(Team::Red, "MegaMinion", at((9000, 18500)), None).expect("a Red Mega Minion");
    let (t, c) = kill_barrel(&mut s, Team::Blue, (9000, 20000));
    let flier = the(&s, Team::Red, "MegaMinion");
    assert!(s.debug_set_pos(flier, c.sub(at((0, 2159)))));
    assert!(!card_stat(&s, "MegaMinion").ignore_pushback, "scene: the victim takes pushes");
    let mut out = Vec::new();
    for k in 12..=22 {
        after(&mut s, t + k);
        out.push(radius_of(offset(&s, flier, c)));
    }
    assert!(hp(&s, flier) > 0, "scene: the victim lives");
    out
}

fn steps(track: &[i32]) -> Vec<i32> {
    track.windows(2).map(|w| w[1] - w[0]).collect()
}

#[test]
fn the_death_pushback_is_the_knockback_ladder_from_the_death_point() {
    // Shipped containers_ladder. Plant death_pushback_unread (it never moves).
    let track = push_track(shipped());
    assert_eq!(track[0], 2159, "scene: the Mega Minion stands 2159 from C on T + 12");
    assert_eq!(steps(&track), vec![200, 175, 150, 125, 100, 75, 50, 25, 0, -25], "its steps away from C on T + 13 .. T + 22 (measured 200, 175, ..., 25, 0, then 25 back)");
}

#[test]
fn the_old_push_arm_moves_nothing() {
    // knockback.DEATH_PUSHBACK = not_read, the engine before the key: the deploying victim stays where it stood.
    let track = push_track(with(|c| c.death_pushback = DeathPushbackScope::NotRead));
    assert_eq!(&steps(&track)[..6], &[0; 6], "a push under not_read");
}

/// The Giant Skeleton's bomb (DeathPushBack 1800), under `cfg`: a Red Mega Minion played 50 ticks after the death, so it
/// still deploys when the bomb lands (fuse 3000: 61 ticks), 2000 from the bomb. Returns how far it moved on the six
/// ticks after the landing.
fn giant_skeleton_push(cfg: BattleConfig) -> i32 {
    let mut s = BattleState::new(5, cfg);
    let gs = s.scenario_spawn_now(Team::Blue, "GiantSkeleton", at((9000, 21000)), None).expect("a Blue Giant Skeleton");
    assert!(s.debug_set_hp(gs, 0));
    s.tick();
    let bomb_idx = s.cards().get(s.cards().index("GiantSkeleton").expect("the Giant Skeleton loads")).death_spawn.expect("its bomb").unit;
    assert!(s.cards().get(bomb_idx).death_pushback > 0 && s.cards().get(bomb_idx).death_spawn.is_none(), "scene: a plain bomb with a DeathPushBack");
    let c = s.spells().iter().find(|sp| sp.card == bomb_idx).map(|sp| match &sp.motion {
        SpellMotion::Flight { aim, .. } => *aim,
        m => panic!("the bomb is not a Flight: {m:?}"),
    });
    let c = c.expect("the Giant Skeleton left its bomb");
    let fuse_ticks = (s.cards().get(bomb_idx).deploy_time_ms / s.config().calib.tick_ms) as u32;
    after(&mut s, 50);
    s.spawn_unit(Team::Red, "MegaMinion", at((9000, 18500)), None).expect("a Red Mega Minion");
    s.tick();
    let flier = the(&s, Team::Red, "MegaMinion");
    assert!(s.debug_set_pos(flier, c.sub(at((0, 2000)))));
    let land = 1 + fuse_ticks; // the bomb lands fuse / TICK_MS + 1 ticks after the death tick 0
    after(&mut s, land);
    assert!(s.spells().iter().all(|sp| sp.card != bomb_idx), "scene: the bomb has not landed on its tick");
    assert!(s.entity(flier).expect("the Mega Minion").deploying, "scene: the victim still deploys when the bomb lands");
    let before = offset(&s, flier, c);
    after(&mut s, land + 6);
    radius_of(offset(&s, flier, c)) - radius_of(before)
}

#[test]
fn only_the_arm_that_reaches_every_bomb_pushes_from_the_giant_skeletons() {
    // Shipped containers_ladder leaves the Giant Skeleton's bomb as it was; every_death_bomb_ladder pushes from it.
    // Plant death_pushback_unread (every_death_bomb_ladder pushes nothing either).
    assert_eq!(giant_skeleton_push(shipped()), 0, "the shipped arm pushes from a plain bomb");
    let moved = giant_skeleton_push(with(|c| c.death_pushback = DeathPushbackScope::EveryDeathBombLadder));
    assert!(moved > 0, "every_death_bomb_ladder: the victim moved {moved} from the Giant Skeleton's bomb");
}

// ---------------------------------------------------------------------------
// (15)-(17) the Skeletons

#[test]
fn an_enemy_first_targets_a_ring_skeleton_on_t_plus_19() {
    // Shipped (targeting.SPAWNED_UNIT_ACQUIRE_DELAY = client_8th_frame_action_buildings, whose death-spawn half this
    // is). Plant container_members_acquire_at_once
    // (the Cannon takes one on T + 13).
    let mut s = BattleState::new(5, shipped());
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", at((9000, 22500)), None).expect("a Red Cannon");
    let (t, _) = kill_barrel(&mut s, Team::Blue, (9000, 20000));
    let mut first: Option<u32> = None;
    for k in 1..=25 {
        after(&mut s, t + k);
        let ring = members(&s, Team::Blue);
        if let Some(tg) = s.entity(cannon).expect("the Cannon").target {
            assert!(ring.contains(&tg), "T + {k}: the Cannon holds a target that is not a ring Skeleton");
            first.get_or_insert(k);
        }
    }
    assert_eq!(first, Some(19), "the Cannon's first target among the Skeletons (measured T + 19, their 8th frame)");
}

#[test]
fn the_ring_skeletons_deploy_ten_frames_and_first_step_on_t_plus_23() {
    // Shipped (client_ring_slide: the ring slides on T + 13 .. T + 16, the_container_ring_slides_out_for_four_ticks_
    // and_stops, then rests). Plant release_deferred (every frame one tick late).
    let mut s = BattleState::new(9, shipped());
    let (t, _) = kill_barrel(&mut s, Team::Blue, (6000, 10000));
    after(&mut s, t + 12);
    let m = members(&s, Team::Blue);
    assert_eq!(m.len(), 7, "the seven Skeletons on T + 12");
    let mut born: Vec<Vec2> = Vec::new();
    for k in 12..=22 {
        after(&mut s, t + k);
        if k == 16 {
            born = m.iter().map(|id| s.entity(*id).expect("a Skeleton").pos).collect();
        }
        for (j, id) in m.iter().enumerate() {
            let v = s.entity(*id).expect("a Skeleton");
            assert_eq!(v.deploying, k <= 21, "member {j}: deploying {} on T + {k} (measured T + 12 .. T + 21)", v.deploying);
            if k > 16 {
                assert_eq!(v.pos, born[j], "member {j} moved on T + {k}, after the slide and before its first step");
            }
        }
    }
    after(&mut s, t + 23);
    for (j, id) in m.iter().enumerate() {
        assert_ne!(s.entity(*id).expect("a Skeleton").pos, born[j], "member {j} did not step on T + 23");
    }
}

#[test]
fn a_container_slot_on_the_river_is_put_on_the_bank() {
    // client_ring_slide. Plant container_water_unejected (slot 0 stays on the water).
    let mut s = BattleState::new(3, ring_slide());
    s.scenario_spawn_now(Team::Red, "Cannon", at((7500, 18500)), None).expect("the Cannon");
    let (t, c) = kill_barrel(&mut s, Team::Blue, (7893, 17176));
    assert_eq!(native(c), (7893, 17176), "scene: the barrel stood in reach of the Cannon and did not move");
    assert!(!s.arena().is_passable_ground(at((7740, 16979))), "scene: slot 0's ring point is on the river");
    after(&mut s, t + 12);
    let m = members(&s, Team::Blue);
    assert_eq!(m.len(), 7, "the seven Skeletons on T + 12");
    for (k, want) in [(0usize, (7740, 17000)), (1, (7948, 17000))] {
        let got = native(s.entity(m[k]).expect("a Skeleton").pos);
        assert!((got.0 - want.0).abs() <= 1 && (got.1 - want.1).abs() <= 1, "slot {k} created at {got:?}, measured {want:?}");
    }
}

// ---------------------------------------------------------------------------
// (18) the state

/// Save `s`, load it, and run both `n` ticks, hash for hash.
fn resumes(s: &mut BattleState, n: u32, what: &str) {
    let mut back = BattleState::load(&s.save()).unwrap_or_else(|e| panic!("{what}: the save does not load: {e}"));
    assert_eq!(back.state_hash(), s.state_hash(), "{what}: the loaded battle hashes differently");
    for k in 0..n {
        s.tick();
        back.tick();
        assert_eq!(back.state_hash(), s.state_hash(), "{what}: the resumed battle diverged {} ticks on", k + 1);
    }
}

#[test]
fn a_barrel_saved_mid_fuse_resumes_hash_for_hash() {
    let mut s = BattleState::new(9, shipped());
    let (t, _) = kill_barrel(&mut s, Team::Blue, (6000, 10000));
    after(&mut s, t + 5);
    resumes(&mut s, 20, "mid-fuse");
    assert_eq!(members(&s, Team::Blue).len(), 7, "vacuous: the container never released");
}

#[test]
fn a_ring_saved_mid_slide_resumes_and_its_cap_is_state() {
    // client_ring_slide. Plants hash_skips_slide_deadline (the edited save loads), save_drops_death_slide (the save
    // does not load).
    let mut s = BattleState::new(9, ring_slide());
    let (t, _) = kill_barrel(&mut s, Team::Blue, (6000, 10000));
    after(&mut s, t + 14);
    let v: serde_json::Value = serde_json::from_slice(&s.save()).expect("a snapshot is JSON");
    let col = v["ents"]["death_slide_until"].as_array().expect("the snapshot carries the slide caps");
    let k = col.iter().position(|x| x.as_u64().unwrap_or(0) != 0).expect("scene: a capped slide runs on T + 14");
    let until = col[k].as_u64().expect("a tick");
    assert!(
        edit_is_hashed(&s, |v| v["ents"]["death_slide_until"][k] = serde_json::json!(until + 1)),
        "a save edited only in a slide's cap loads under the old hash: the cap is not hashed"
    );
    resumes(&mut s, 10, "mid-slide");
}

#[test]
fn a_barrel_saved_mid_drain_resumes_and_its_drain_is_state() {
    // Shipped. Plant hash_skips_kamikaze (the edited save loads).
    let (mut s, _, b) = drain_scene(shipped(), true);
    let b = b.expect("the barrel");
    let mut d = None;
    for k in 0..40 {
        s.tick();
        if s.entity(b).is_some_and(|v| v.hp < 532) {
            d = Some(k);
            break;
        }
    }
    assert!(d.is_some(), "scene: the barrel never fired");
    for _ in 0..4 {
        s.tick();
    }
    assert!(s.entity(b).is_some(), "scene: the barrel lives on its fire + 4");
    let i = b.index as usize;
    let v: serde_json::Value = serde_json::from_slice(&s.save()).expect("a snapshot is JSON");
    let from = v["ents"]["kamikaze_from"][i].as_u64().expect("the snapshot carries the drain's start");
    assert!(from != 0, "scene: the drain has not started");
    assert!(
        edit_is_hashed(&s, |v| v["ents"]["kamikaze_from"][i] = serde_json::json!(from + 1)),
        "a save edited only in the drain's start loads under the old hash: it is not hashed"
    );
    resumes(&mut s, 30, "mid-drain");
    assert!(s.entity(b).is_none(), "vacuous: the barrel outlived the resume");
}

/// spawner.CONTAINER_BURST_PUSH: client 15.535.29 (sp-form-SkeletonBalloon-evo-s0 t368) burst a container with a Skeleton
/// 217 from it, at (189, -107) after its move, and laid its seven members around (-130, +73) from its point: the
/// separation scan's push on a body of no radius and no mass. Here a Blue Skeleton is held at that offset from the
/// container of a Blue barrel through its fuse; it takes its walk step on the burst tick, so the push is computed from where
/// that step left it, with the scan's arithmetic. The slides still end around the point the container fell on: each
/// member's end is the same under both arms (client 15.535.29, t372, the slide's last tick: 6 of 7 nearer the ends around
/// the unpushed point). Plants: container_burst_unpushed, container_slide_from_pushed.
#[test]
fn a_container_bursts_where_a_covering_unit_pushes_it() {
    let mut ends: Vec<Vec<Vec2>> = Vec::new();
    for arm in [ContainerBurstPush::Client15535ContactPush, ContainerBurstPush::None] {
        let mut s = BattleState::new(9, with(|c| c.container_burst_push = arm));
        let (t, c) = kill_barrel(&mut s, Team::Blue, (6000, 10000));
        let sk = s.scenario_spawn_now(Team::Blue, "Skeleton", c.add(at((189, -107))), None).expect("the Skeleton");
        while s.tick_count() <= t + 12 {
            assert!(s.debug_set_pos(sk, c.add(at((189, -107)))), "the held Skeleton died");
            s.tick();
        }
        // The push the scan gives a body of no radius and no mass at C from the Skeleton (radius 500, mass 1).
        let (dx, dy) = native(c.sub(s.entity(sk).expect("the held Skeleton").pos));
        let dist = isqrt((dx * dx + dy * dy) as i64).max(1) as i32;
        assert!(dist < 500, "the scene drifted: the Skeleton does not cover the point ({dist})");
        let mag = (500 - dist).clamp(0, 300).min(299) + 1;
        let (mut px, mut py) = (dx * mag / dist, dy * mag / dist);
        let len = isqrt((px * px + py * py) as i64) as i32;
        if px * px + py * py >= 22501 {
            px = px * 150 / len;
            py = py * 150 / len;
        }
        let shift = if arm == ContainerBurstPush::None { (0, 0) } else { (px, py) };
        assert!(arm == ContainerBurstPush::None || shift.0.abs() + shift.1.abs() >= 100, "vacuous: no push ({shift:?})");
        let m: Vec<EntityId> = members(&s, Team::Blue).into_iter().filter(|id| *id != sk).collect();
        assert_eq!(m.len(), 7, "{arm:?}: the seven Skeletons on T + 12");
        for (k, id) in m.iter().enumerate() {
            let got = offset(&s, *id, c);
            let want = (RING_SIDE0_LEFT[k].0 + shift.0, RING_SIDE0_LEFT[k].1 + shift.1);
            assert!((got.0 - want.0).abs() <= 2 && (got.1 - want.1).abs() <= 2, "{arm:?}: member {k} at {got:?} from C, want {want:?}");
        }
        ends.push(m.iter().map(|id| s.entity(*id).expect("the member is alive").death_slide_end).collect());
    }
    assert!(ends[1].iter().all(|e| *e != Vec2::default()), "vacuous: the members have no slide end: {:?}", ends[1]);
    assert_eq!(ends[0], ends[1], "the pushed burst's slides end elsewhere than the unpushed one's");
}

/// spawner.CONTAINER_RING_WATER (client 15.535.29: 14 of 14 members of the two rings burst on water born where the water
/// clamp from the burst point takes them, 12 of them on the river; ub-sb2-water-mid t219, a container at (6326, 15982), all
/// 7 in cell (12, 31)): a Blue barrel killed over the river at (6326, 15982): its members' points on T + 12, native.
/// Plant: container_ring_water_ejected.
/// Returns the members' points and the burst point's 500 cell (its low corner, native): the barrel takes its step on its
/// death tick, so the container falls where that step left it, still on the river.
fn wet_ring(arm: royalesim::state::ContainerRingWater) -> (Vec<(i32, i32)>, (i32, i32)) {
    let mut cfg = ring_slide();
    cfg.calib.container_ring_water = arm;
    let mut s = BattleState::new(3, cfg);
    let (t, c) = kill_barrel(&mut s, Team::Blue, (6326, 15982));
    assert!(!s.arena().is_passable_ground(c), "scene: the burst point is on the river");
    let (cx, cy) = native(c);
    after(&mut s, t + 12);
    let m = members(&s, Team::Blue);
    assert_eq!(m.len(), 7, "the seven Skeletons on T + 12");
    (m.iter().map(|id| native(s.entity(*id).expect("a Skeleton").pos)).collect(), (cx / 500 * 500, cy / 500 * 500))
}

#[test]
fn a_container_burst_on_the_river_keeps_its_members_in_its_cell_under_client15535_clamp_from_burst_point() {
    let (new, cell) = wet_ring(royalesim::state::ContainerRingWater::Client15535ClampFromBurstPoint);
    let inside = |p: &(i32, i32)| (cell.0..=cell.0 + 499).contains(&p.0) && (cell.1..=cell.1 + 499).contains(&p.1);
    assert!(new.iter().all(inside), "client15535_clamp_from_burst_point: a member left the burst point's cell {cell:?}: {new:?}");
    // NOT VACUOUS: eject_nearest_land puts members on land, out of the cell.
    let (old, cell_old) = wet_ring(royalesim::state::ContainerRingWater::EjectNearestLand);
    assert_eq!(cell_old, cell, "the scene drifted between the arms");
    assert!(!old.iter().all(inside), "eject_nearest_land: every member stayed in the cell: {old:?}");
}
