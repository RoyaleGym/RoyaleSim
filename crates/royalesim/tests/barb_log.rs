//! A ROLL THAT RELEASES UNITS (card.rs `SpellShape::Rolling { spawn }`, `SpellShape::release`; spell.rs
//! `release_units`): the Barbarian Barrel.
//!
//! THE LAW, measured on the Barbarian Barrel (4 of 4 casts on the 16.402 corpus, the 15.535.29 scenario runs):
//!   - the roll is the Log's (airborne from MinDistance behind the landing, then rolling ProjectileRange along the
//!     caster's forward axis);
//!   - where it stops it releases ONE Barbarian (the row's SpawnCharacterCount is blank: one unit on a roll), with the
//!     row's SpawnCharacterDeployTime, at the spell's level;
//!   - spells.ROLL_FIRST_STEP: the roll stands unmoved on the landing tick and first steps on the next
//!     (tick_after_landing, measured); the shipped on_landing_tick steps at once, so the roll stops, and releases,
//!     one tick earlier.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the Barbarian Barrel as a roll releasing one Barbarian;
//!   2. a cast releases exactly one Barbarian, on the roll's end point, on the tick the roll ends;
//!   3. under tick_after_landing the release comes exactly one tick later than under on_landing_tick;
//!   4. the Barbarian stands its SpawnCharacterDeployTime (20 ticks) before its first step.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test barb_log`):
//!   * `roll_spawn_unread` -- the loader drops the roll's SpawnCharacter: (1) and (2) go red.
//!   * `roll_release_dropped` -- the roll stops and releases nothing: (2) goes red.
//!   * `roll_steps_on_landing` -- the first step on the landing tick whatever the key: (3) goes red.
//!   * `rolling_box_swept` -- spells.ROLLING_HIT_SWEEP = client16402_current_box still sweeps from the previous centre:
//!     `under_client16402_current_box_a_unit_born_behind_the_logs_box_is_not_hit` goes red.
//!   * `rolling_hits_leapers` -- spells.ROLLING_SPARES_LEAPERS = client16402_spared_mid_leap still hits a leaper:
//!     `under_client16402_spared_mid_leap_a_hog_rider_in_its_river_leap_is_not_hit` goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{SpellPlacement, SpellShape};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, RollFirstStep};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Plant: roll_spawn_unread.
#[test]
fn the_loader_reads_the_barbarian_barrel_as_a_roll_releasing_one_barbarian() {
    let s = BattleState::new(0, config());
    let spell = card_stat(&s, "BarbLog").spell.as_ref().expect("the Barbarian Barrel loads as a spell");
    assert_eq!(spell.placement, SpellPlacement::TroopTerritory { on_buildings: true });
    let SpellShape::Rolling { range, hit, spawn, .. } = &spell.shape else { panic!("BarbLog: {:?}", spell.shape) };
    assert_eq!((*range, hit.damage), (4500 * K, 91), "the roll row's ProjectileRange and Damage");
    let sp = spawn.as_ref().expect("the roll releases a unit");
    assert_eq!((s.cards().get(sp.unit).name.as_str(), sp.count, sp.deploy_time_ms), ("Barbarian", 1, Some(1000)));
    assert!(s.cards().get(sp.unit).summon_only, "the Barbarian is a unit the spell releases, not a card");
    assert_eq!(spell.shape.release().map(|r| r.count), Some(1), "`release` reads the roll's spawn");
}

/// One Blue Barbarian Barrel cast at `tap` under `first`: (the release tick k, counted from the cast tick k = 0; the
/// Barbarian; the roll's end point), and the Barbarian's position on each of the 30 ticks after it appears.
fn cast(first: RollFirstStep, tap: (i32, i32)) -> (u32, EntityId, Vec2, Vec<Vec2>) {
    let mut cfg: BattleConfig = config();
    cfg.calib.roll_first_step = first;
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(Team::Blue, "BarbLog", at(tap), None).expect("cast the Barbarian Barrel");
    let mut last_roll = None;
    for k in 0..80u32 {
        s.tick();
        // the roll's end point: where it stands plus what it has left, along the caster's forward axis
        let fwd = royalesim::spell::forward_dy(Team::Blue);
        let rolling = s.spells().iter().find_map(|sp| match &sp.motion {
            SpellMotion::Rolling { pos, travelled, len, .. } => Some(Vec2::new(pos.x, pos.y + fwd * (len - travelled))),
            _ => None,
        });
        let barbs: Vec<(EntityId, Vec2)> = s.entities().filter(|v| v.card == "Barbarian").map(|v| (v.id, v.pos)).collect();
        if let Some(p) = rolling {
            last_roll = Some(p);
            assert!(barbs.is_empty(), "a Barbarian stands while the roll still rolls (tick {k})");
        }
        if !barbs.is_empty() {
            assert_eq!(barbs.len(), 1, "the barrel released {} Barbarians", barbs.len());
            let (id, _) = barbs[0];
            let mut walk = Vec::new();
            for _ in 0..30 {
                walk.push(s.entity(id).map_or(Vec2::default(), |v| v.pos));
                s.tick();
            }
            return (k, id, last_roll.expect("the roll never stood"), walk);
        }
    }
    panic!("the barrel released no Barbarian in 80 ticks");
}

/// Plant: roll_release_dropped.
#[test]
fn the_roll_releases_one_barbarian_on_its_end_point() {
    let (_, _, end, walk) = cast(RollFirstStep::OnLandingTick, (9000, 6000));
    assert_eq!(walk[0], end, "the Barbarian stands where the roll stopped");
}

/// Plant: roll_steps_on_landing.
#[test]
fn under_tick_after_landing_the_release_comes_one_tick_later() {
    let (a, _, end_a, _) = cast(RollFirstStep::OnLandingTick, (9000, 6000));
    let (b, _, end_b, _) = cast(RollFirstStep::TickAfterLanding, (9000, 6000));
    assert_eq!(end_a, end_b, "the roll stops on the same point under both arms");
    assert_eq!(b, a + 1, "on_landing_tick releases on {a}, tick_after_landing on {b}");
}

#[test]
fn the_barbarian_stands_its_spawn_deploy_time() {
    let (_, _, _, walk) = cast(RollFirstStep::OnLandingTick, (9000, 6000));
    let first_move = walk.windows(2).position(|w| w[0] != w[1]);
    assert!(first_move.is_some_and(|m| m >= 20), "the Barbarian moved {first_move:?} ticks after it appeared; its deploy is 1000 ms");
}

/// A Blue Log cast at (9000, 11500) (out of the Blue princess towers' reach where it rolls) under spells.ROLLING_HIT_SWEEP =
/// `arm`: 8 ticks into its roll a Red Skeleton is put with
/// its front edge 100 ahead of the current box's back edge, so the next tick's step leaves it behind that tick's current box
/// but inside the box swept from this centre: the Skeleton's hp loss over the next 10 ticks.
fn wake_newborn_loss(arm: royalesim::state::RollingHitSweep) -> i32 {
    let mut cfg: BattleConfig = config();
    cfg.calib.rolling_hit_sweep = arm;
    let mut s = BattleState::new(0, cfg);
    let Some(royalesim::card::SpellDef { shape: SpellShape::Rolling { half_depth, .. }, .. }) = &card_stat(&s, "Log").spell else { panic!("the Log rolls") };
    let half_depth = *half_depth;
    s.spawn_unit(Team::Blue, "Log", at((9000, 11500)), None).expect("cast the Log");
    let roll = |s: &BattleState| s.spells().iter().find_map(|sp| match &sp.motion {
        SpellMotion::Rolling { pos, .. } => Some(*pos),
        _ => None,
    });
    let mut k = 0;
    while k < 8 || roll(&s).is_none() {
        s.tick();
        k += 1;
        assert!(k < 60, "the Log never rolled");
    }
    let c = roll(&s).expect("the roll");
    let sk = s.scenario_spawn_now(Team::Red, "Skeleton", c, None).expect("the Skeleton");
    let r = s.entity(sk).expect("the Skeleton").radius;
    // the current box's back edge is c.y - half_depth (a Blue roll goes +y); the front of the Skeleton 100 ahead of it, which
    // the next step (more than 100) passes
    assert!(s.debug_set_pos(sk, Vec2::new(c.x, c.y - half_depth + 100 * K - r)));
    let hp0 = s.entity(sk).expect("the Skeleton").hp;
    for _ in 0..10 {
        s.tick();
    }
    s.entity(sk).map_or(hp0, |v| hp0 - v.hp)
}

/// spells.ROLLING_HIT_SWEEP = client16402_current_box (client 16.402, parity's r73 census: units born behind a roll's centre
/// were hit 0 of 10; the engine's swept box hit 6 on their birth + 1): a Skeleton put just behind the Log's box is not hit;
/// under swept_prev_to_cur (the vacuity check) the next tick's sweep reaches it. Plant: rolling_box_swept.
#[test]
fn under_client16402_current_box_a_unit_born_behind_the_logs_box_is_not_hit() {
    use royalesim::state::RollingHitSweep as R;
    assert!(wake_newborn_loss(R::SweptPrevToCur) > 0, "swept_prev_to_cur (the vacuity check): the sweep never reached the Skeleton");
    assert_eq!(wake_newborn_loss(R::Client16402CurrentBox), 0, "client16402_current_box: the Skeleton behind the box was hit");
}

/// A Blue Hog Rider put at (9500, 12500) under spells.ROLLING_SPARES_LEAPERS = `arm` (it leaps the river: tests/
/// jump_landing_contact.rs's scene): once it leaps it is held on that point (still in its leap) and a Red Barbarian Barrel is
/// cast 2,500 ahead of it to roll back over it: its hp loss over the next 30 ticks.
fn leaper_loss(arm: royalesim::state::RollingSparesLeapers) -> i32 {
    let mut cfg: BattleConfig = config();
    cfg.calib.rolling_spares_leapers = arm;
    let mut s = BattleState::new(3, cfg);
    let hog = s.scenario_spawn_now(Team::Blue, "HogRider", at((9500, 12500)), None).expect("the Hog Rider");
    let mut k = 0;
    while !s.entity(hog).expect("the Hog Rider").jumping {
        s.tick();
        k += 1;
        assert!(k < 300, "the scene drifted: the Hog Rider never leapt");
    }
    let p = s.entity(hog).expect("the Hog Rider").pos;
    s.spawn_unit(Team::Red, "BarbLog", Vec2::new(p.x, p.y + 2500 * K), None).expect("cast the Barbarian Barrel");
    let hp0 = s.entity(hog).expect("the Hog Rider").hp;
    for _ in 0..30 {
        assert!(s.debug_set_pos(hog, p));
        s.tick();
        assert!(s.entity(hog).expect("the Hog Rider").jumping, "the scene drifted: the Hog Rider landed");
    }
    hp0 - s.entity(hog).expect("the Hog Rider").hp
}

/// spells.ROLLING_SPARES_LEAPERS = client16402_spared_mid_leap (client 16.402, parity's r73 census: a Barbarian Barrel over a
/// unit mid leap or mid jump hit 0 of 4; the engine's 4 of 4): the Barrel rolls over a Hog Rider held in its river leap and
/// does not hit it; under hits (the vacuity check) it does. Plant: rolling_hits_leapers.
#[test]
fn under_client16402_spared_mid_leap_a_hog_rider_in_its_river_leap_is_not_hit() {
    use royalesim::state::RollingSparesLeapers as R;
    assert!(leaper_loss(R::Hits) > 0, "hits (the vacuity check): the Barrel never hit the Hog Rider");
    assert_eq!(leaper_loss(R::Client16402SparedMidLeap), 0, "client16402_spared_mid_leap: the Barrel hit the Hog Rider mid leap");
}

/// One Blue Barbarian Barrel cast at (4600, 11800) under spells.ROLL_RELEASE_WATER_POINT = `arm`: its roll ends on the river
/// (x 4600 is east of the left bridge's columns): (the roll's end point, the Barbarian's first point).
fn released_on_water(arm: royalesim::state::RollReleaseWaterPoint) -> (Vec2, Vec2) {
    let mut cfg: BattleConfig = config();
    cfg.calib.roll_release_water_point = arm;
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(Team::Blue, "BarbLog", at((4600, 11800)), None).expect("cast the Barbarian Barrel");
    let mut end = None;
    for _ in 0..80 {
        s.tick();
        let fwd = royalesim::spell::forward_dy(Team::Blue);
        if let Some(p) = s.spells().iter().find_map(|sp| match &sp.motion {
            SpellMotion::Rolling { pos, travelled, len, .. } => Some(Vec2::new(pos.x, pos.y + fwd * (len - travelled))),
            _ => None,
        }) {
            end = Some(p);
        }
        if let Some(b) = s.entities().find(|v| v.card == "Barbarian") {
            return (end.expect("the roll never stood"), b.pos);
        }
    }
    panic!("the barrel released no Barbarian in 80 ticks");
}

/// spells.ROLL_RELEASE_WATER_POINT = client16402_half_cell_centre (client 16.402, parity's r74 census: 59 of 85 releases on water
/// exactly on the nearest dry half-cell's centre; the engine's 0): the Barbarian stands on a dry half-cell's centre, and no dry
/// centre is nearer the roll's end; under nearest_passable_ground (the vacuity check) on the nearest land along an axis,
/// not a centre. Plant: release_water_axis.
#[test]
fn under_client16402_half_cell_centre_a_barbarian_released_on_water_stands_on_the_nearest_dry_centre() {
    use royalesim::state::RollReleaseWaterPoint as R;
    let a = BattleState::new(0, config()).arena().clone();
    let (end, old) = released_on_water(R::NearestPassableGround);
    assert!(a.is_water(end), "the scene drifted: the roll's end {end:?} is not on the river");
    let centre = |p: Vec2| p.x.rem_euclid(a.cell) == a.cell / 2 && p.y.rem_euclid(a.cell) == a.cell / 2;
    assert!(!centre(old), "nearest_passable_ground (the vacuity check): a half-cell centre {old:?}");
    let (_, new) = released_on_water(R::Client16402HalfCellCentre);
    assert!(centre(new) && !a.is_water(new), "client16402_half_cell_centre: {new:?} is not a dry half-cell's centre");
    let d2 = |p: Vec2| ((p.x - end.x) as i64).pow(2) + ((p.y - end.y) as i64).pow(2);
    for r in 0..a.rows {
        for c in 0..a.cols {
            let q = a.half_to_subtile_center(c, r);
            if a.cell_bits(c, r) & a.bit_water == 0 {
                assert!(d2(q) >= d2(new), "client16402_half_cell_centre: the dry centre {q:?} is nearer the end than {new:?}");
            }
        }
    }
}

/// A Blue Goblin Hut at (9000, 8000) kept awake by a Red Knight held 4,500 north, and a Red Log cast 3,000 north of the hut
/// `off` ticks after it came out, under spells.LIVE_SPAWN_ROLL_BIRTH = `arm`: (the hut-made units seen at the end of a tick,
/// those of them that lost hp on the tick they were first seen). A newborn the Log kills on its birth tick is never seen.
fn hut_under_log(arm: royalesim::state::LiveSpawnRollBirth, off: u32) -> (usize, usize) {
    let mut cfg: BattleConfig = config();
    cfg.calib.live_spawn_roll_birth = arm;
    let mut s = BattleState::new(3, cfg);
    let (h_at, k_at) = (at((9000, 8000)), at((9000, 12500)));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).expect("the Knight");
    let hut = s.scenario_spawn_now(Team::Blue, "GoblinHut", h_at, None).expect("the Goblin Hut");
    let hold = |s: &mut BattleState| {
        let full = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_pos(knight, k_at) && s.debug_set_hp(knight, full));
    };
    for _ in 0..(40 + off) {
        hold(&mut s);
        s.tick();
    }
    let before: std::collections::BTreeSet<EntityId> = s.entities().filter(|e| e.team == Team::Blue && e.id != hut && e.kind == royalesim::entity::EntityKind::Troop).map(|e| e.id).collect();
    s.spawn_unit(Team::Red, "Log", at((9000, 11000)), None).expect("cast the Log");
    let mut seen: std::collections::BTreeSet<EntityId> = Default::default();
    let mut hurt_at_birth = 0;
    for _ in 0..60 {
        hold(&mut s);
        s.tick();
        for e in s.entities().filter(|e| e.team == Team::Blue && e.id != hut && e.kind == royalesim::entity::EntityKind::Troop && !before.contains(&e.id)) {
            if seen.insert(e.id) && e.hp < e.max_hp {
                hurt_at_birth += 1;
            }
        }
    }
    (seen.len(), hurt_at_birth)
}

/// spells.LIVE_SPAWN_ROLL_BIRTH = client16402_unseen_on_birth_tick (client 16.402, parity's r74 census: 3 of 3 live spawner
/// newborns unhit by a roll on their birth tick; the engine killed them on it): over 44 cast offsets (a wave period) no
/// hut-made unit is hurt on the tick it is first seen under the new arm, and more of them are seen than under
/// hit_on_birth_tick (the vacuity check: there the Log kills some on their birth tick, unseen). Plant: roll_sees_newborn.
#[test]
fn under_client16402_unseen_on_birth_tick_a_log_misses_a_live_spawn_on_its_birth_tick() {
    use royalesim::state::LiveSpawnRollBirth as L;
    let (mut old_seen, mut new_seen, mut new_hurt) = (0, 0, 0);
    for off in 0..44 {
        old_seen += hut_under_log(L::HitOnBirthTick, off).0;
        let (seen, hurt) = hut_under_log(L::Client16402UnseenOnBirthTick, off);
        new_seen += seen;
        new_hurt += hurt;
    }
    assert!(new_seen > old_seen, "vacuous: the Log killed no newborn on its birth tick over 44 offsets ({old_seen} seen, the new arm {new_seen})");
    assert_eq!(new_hurt, 0, "client16402_unseen_on_birth_tick: a newborn was hurt on its birth tick");
}
