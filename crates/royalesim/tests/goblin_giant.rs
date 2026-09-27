//! THE GOBLIN GIANT: two riders on an arc behind their mount, and where each comes down when the mount dies (card.rs
//! `convert_attach`, `rider_shape`, `FormationDef::spawn_max_angle_deg`; formation.rs `rider_arc_offset`; state.rs
//! `spawn_riders`, `carry_riders`, `note_mount_facings`, `dismount_point`; calibration rider.OFFSET_LAW and
//! rider.DISMOUNT_POINT).
//!
//! THE ROWS (the 15.535.29 tables): the Giant's Spawn* block carries two SpearGoblinGiant riders with SpawnAttach and
//! SpawnRadius 900. The rider row flies (FlyingHeight 4000), sets SpawnAngleShift -22 and SpawnMaxAngle 90, and leaves
//! one SpearGoblin where it dies (DeathSpawnDeployTime 700). The card that serves as that unit is SpearGoblins: its own
//! row is SpearGoblin, so the card's name and the unit's differ.
//!
//! THE LAW, measured on client 15.535.29 (five runs, three dismount scenes):
//!   - rider k stands where the Giant stood a tick before, plus a 900 ring offset at the Giant's heading of that tick,
//!     rounded to a whole degree, plus 203 (k = 0) or 158 (k = 1), exact to the native unit at six headings: facing
//!     (0, 256) gives (351, -828) and (-337, -834); (181, 181) gives (-337, -834) and (-828, -351); (220, 129) gives
//!     (-541, -718) and (-891, -125); (5, 256) gives (337, -834) and (-351, -828); (12, 255) gives (307, -845) and
//!     (-380, -815); (164, 196) gives (-262, -860) and (-794, -422);
//!   - when the Giant dies each rider's Spear Goblin comes down at the Giant's position plus that rider's offset under
//!     the Giant's facing, full hp (133 at level 11), deploying 14 ticks, a target for enemies from its 8th frame.
//!
//! WHAT IS PINNED, each with its precondition (level 11):
//!   1. the loader takes the Giant's two riders 900 out, the flying rider row with its dismount, and puts the riders on
//!      the arc when the Giant is created;
//!   2. the arc at the six measured headings (a pure call), and on a walk: every tick a Blue and a Red rider stands at
//!      its Giant's position of the tick before plus the arc at the Giant's facing of that tick;
//!   3. a walking Giant killed by a Zap: each dismount Spear Goblin at the Giant's position as its death leaves it plus
//!      its rider's offset under the Giant's facing then (one step from where its rider stood), 133 hp, deploying 700
//!      ms, acquirable from its 8th frame. Where the Giant is taken to stand when it walks on its death tick is the
//!      engine's reading (after that tick's step; the ledger's open item): the measured scenes stood it still or killed
//!      it with a spell.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test goblin_giant`):
//!   * `rider_offset_ignored` -- every rider on its mount's centre: (1), (2) and (3) go red.
//!   * `dismount_at_rider_position` -- the dismount at the rider's own position: (3) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::UnitRef;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::formation::rider_arc_offset;
use royalesim::state::{BattleConfig, BattleState, RiderDismountPoint, RiderOffsetLaw};
use royalesim::{EntityId, Team};

/// The level of the measured runs.
const LEVEL: i32 = 11;

/// The rider row and the card that serves as its dismount.
const RIDER: &str = "SpearGoblinGiant";
const DISMOUNT: &str = "SpearGoblins";

/// The shipped config at level 11, asserting the arms this file pins.
fn shipped() -> BattleConfig {
    let mut c = config();
    c.card_level = [LEVEL, LEVEL];
    c.tower_level = [LEVEL, LEVEL];
    assert_eq!(c.calib.rider_offset_law, RiderOffsetLaw::ArcBehindMount, "the shipped rider.OFFSET_LAW");
    assert_eq!(c.calib.rider_dismount_point, RiderDismountPoint::MountPositionPlusOffset, "the shipped rider.DISMOUNT_POINT");
    c
}

/// A native point as engine subtiles.
fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// An engine vector as native units.
fn native(p: Vec2) -> (i32, i32) {
    (p.x / K, p.y / K)
}

/// Rider k's arc offset at the mount heading `facing`, native: the Goblin Giant's block (900, two riders) and rider row
/// (-22, 90).
fn arc(facing: Vec2, k: i32) -> (i32, i32) {
    native(rider_arc_offset(900 * K, facing, 2, k, -22, 90))
}

/// The riders of mount `m`, by rank (creation order).
fn riders(s: &BattleState, m: EntityId) -> Vec<EntityId> {
    let mut r: Vec<(u32, EntityId)> = s.entities().filter(|e| e.attached_to == Some(m)).map(|e| (e.team_seq, e.id)).collect();
    r.sort();
    r.into_iter().map(|(_, id)| id).collect()
}

// ---------------------------------------------------------------------------
// (1)

/// Plant: rider_offset_ignored.
#[test]
fn the_loader_takes_the_giants_two_riders_and_puts_them_on_the_arc() {
    let s = BattleState::new(0, shipped());
    let db = s.cards();
    let gg = db.index("GoblinGiant").unwrap_or_else(|| panic!("GoblinGiant refused: {:?}", db.rejected.iter().find(|(n, _)| n == "GoblinGiant")));
    let at_ = db.get(gg).attach.expect("the Giant's attached-rider block");
    assert_eq!((at_.number, at_.radius), (2, Some(900 * K)), "SpawnNumber 2, SpawnRadius 900");
    assert!(db.get(gg).spawner.is_none(), "the SpawnAttach block read as a periodic spawner");
    let r = db.get(at_.unit);
    assert_eq!((r.name.as_str(), r.unit_name.as_str(), r.summon_only), (RIDER, RIDER, true), "the rider row");
    assert!(r.is_flying(), "the rider row flies");
    assert_eq!((r.formation.spawn_angle_shift_deg, r.formation.spawn_max_angle_deg), (-22, 90), "SpawnAngleShift, SpawnMaxAngle");
    let ds = r.death_spawn.expect("the rider's dismount");
    let d = db.get(ds.unit);
    assert_eq!((d.name.as_str(), d.unit_name.as_str(), d.summon_only), (DISMOUNT, "SpearGoblin", false), "the dismount unit: a card whose row is SpearGoblin");
    assert_eq!((ds.count, ds.deploy_time_ms), (1, Some(700)), "DeathSpawnCount, DeathSpawnDeployTime");
    assert_eq!(db.unit_refs(gg), vec![(UnitRef::Attach, at_.unit, None)], "the Giant puts its riders on the board");
    assert_eq!(db.unit_refs(at_.unit), vec![(UnitRef::DeathSpawn, ds.unit, None)], "a rider puts its dismount on the board");
    db.check_levels(gg, LEVEL).expect("the Giant, its riders and their dismounts at level 11");
    // A Blue Giant played: its two riders from its first frame, right after it, on the arc at its first heading.
    let mut s = BattleState::new(0, shipped());
    s.spawn_unit(Team::Blue, "GoblinGiant", at((9500, 10500)), None).expect("play the Goblin Giant");
    s.tick();
    let g = s.entities().find(|e| e.card == "GoblinGiant").expect("the Giant on its first frame");
    let rs = riders(&s, g.id);
    assert_eq!(rs.len(), 2, "two riders");
    assert_eq!(g.facing, Vec2::new(0, 256), "a fresh Blue unit faces +y");
    for (k, id) in rs.iter().enumerate() {
        let v = s.entity(*id).unwrap();
        assert_eq!(v.team_seq, g.team_seq + 1 + k as u32, "rider {k} created right after the Giant, in rank order");
        assert_eq!((v.max_hp, v.hp), (133, 133), "rider {k} at level 11");
        let off = native(v.pos.sub(g.pos));
        assert_eq!(off, [(351, -828), (-337, -834)][k], "rider {k}'s offset from the Giant on its first frame");
    }
}

// ---------------------------------------------------------------------------
// (2)

/// Plant: rider_offset_ignored.
#[test]
fn the_arc_at_the_six_measured_headings() {
    let measured = [
        ((0, 256), (351, -828), (-337, -834)),
        ((181, 181), (-337, -834), (-828, -351)),
        ((220, 129), (-541, -718), (-891, -125)),
        ((5, 256), (337, -834), (-351, -828)),
        ((12, 255), (307, -845), (-380, -815)),
        ((164, 196), (-262, -860), (-794, -422)),
    ];
    for ((fx, fy), r0, r1) in measured {
        let f = Vec2::new(fx, fy);
        assert_eq!((arc(f, 0), arc(f, 1)), (r0, r1), "facing ({fx}, {fy})");
        // A Red Giant's heading is the rotation, and so are its riders' offsets.
        assert_eq!((arc(Vec2::new(-fx, -fy), 0), arc(Vec2::new(-fx, -fy), 1)), ((-r0.0, -r0.1), (-r1.0, -r1.1)), "facing ({}, {})", -fx, -fy);
    }
}

/// One rider on one tick: (its native offset from where its Giant stood a tick before, the arc's offset there).
type RiderRow = ((i32, i32), (i32, i32));

/// A Goblin Giant of `team` played at `p` (native); per tick after its first frame, each rider's native offset from
/// the Giant's position of the tick before, and that offset as the arc gives it at the Giant's facing of the tick
/// before. Also the Giant's walking ticks and the distinct headings it walked at.
fn ride(cfg: BattleConfig, team: Team, p: (i32, i32)) -> (Vec<[RiderRow; 2]>, u32, Vec<Vec2>) {
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(team, "GoblinGiant", at(p), None).expect("play the Goblin Giant");
    s.tick();
    let g = s.entities().find(|e| e.card == "GoblinGiant" && e.team == team).map(|e| e.id).expect("the Giant");
    let rs = riders(&s, g);
    assert_eq!(rs.len(), 2, "two riders");
    let (mut prev_pos, mut prev_facing) = (s.entity(g).unwrap().pos, s.entity(g).unwrap().facing);
    let (mut out, mut walked, mut headings) = (Vec::new(), 0, Vec::new());
    for t in 1..=70u32 {
        s.tick();
        let gv = s.entity(g).unwrap_or_else(|| panic!("the scene drifted: the Giant died on its tick {t}"));
        let row = [0usize, 1].map(|k| {
            let r = s.entity(rs[k]).unwrap_or_else(|| panic!("rider {k} died on tick {t}"));
            (native(r.pos.sub(prev_pos)), arc(prev_facing, k as i32))
        });
        out.push(row);
        if gv.pos != prev_pos {
            walked += 1;
        }
        if !headings.contains(&prev_facing) {
            headings.push(prev_facing);
        }
        (prev_pos, prev_facing) = (gv.pos, gv.facing);
    }
    (out, walked, headings)
}

/// Plant: rider_offset_ignored.
#[test]
fn every_tick_a_rider_stands_on_the_arc_behind_where_its_giant_stood_a_tick_before() {
    for (team, p) in [(Team::Blue, (9500, 10500)), (Team::Red, (8500, 21500))] {
        let (rows, walked, headings) = ride(shipped(), team, p);
        assert!(walked >= 30, "vacuous: the {team:?} Giant walked on {walked} ticks");
        assert!(headings.len() >= 2, "vacuous: the {team:?} Giant walked at one heading: {headings:?}");
        for (t, row) in rows.iter().enumerate() {
            for (k, (got, want)) in row.iter().enumerate() {
                assert_eq!(got, want, "{team:?} rider {k} on the Giant's tick {}: its offset from where the Giant stood a tick before", t + 1);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// (3)

/// Plants: rider_offset_ignored, dismount_at_rider_position.
#[test]
fn a_giant_killed_walking_leaves_each_riders_spear_goblin_at_its_position_plus_the_offset() {
    // A Blue Giant set up walking on its own half, at 100 hp so that a red Zap kills it; the same battle without the Zap
    // is the control, which shows where the Giant stands and faces as its death tick X leaves it.
    let setup = || {
        let mut s = BattleState::new(0, shipped());
        let g = s.scenario_spawn_now(Team::Blue, "GoblinGiant", at((9000, 10000)), Some(100)).expect("the Giant");
        for _ in 0..6 {
            s.tick();
        }
        (s, g)
    };
    let (mut zapped, g) = setup();
    let (mut control, g2) = setup();
    assert_eq!(g, g2);
    let before = zapped.entity(g).expect("the Giant walks").pos;
    zapped.spawn_unit(Team::Red, "Zap", before, None).expect("cast the Zap");
    let rs = riders(&control, g);
    zapped.tick(); // X
    control.tick();
    let x = zapped.tick_count() - 1;
    assert!(zapped.entity(g).is_none(), "the scene drifted: the Giant outlived the Zap");
    let gv = control.entity(g).expect("the control's Giant");
    assert!(gv.pos != before, "the scene drifted: the Giant did not step on its death tick");
    // Each rider's dismount, by rank: the Giant's position plus the arc at its facing, as X leaves them.
    let want: Vec<(i32, i32)> = (0..2).map(|k| native(gv.pos.add(Vec2::new(arc(gv.facing, k).0 * K, arc(gv.facing, k).1 * K)))).collect();
    let riders_at: Vec<(i32, i32)> = rs.iter().map(|r| native(control.entity(*r).expect("the control's rider").pos)).collect();
    for k in 0..2 {
        assert_ne!(want[k], riders_at[k], "the scene drifted: rider {k} stood on its dismount point");
    }
    let mut got: Vec<(i32, i32, EntityId)> = zapped.entities().filter(|e| e.card == DISMOUNT && e.team == Team::Blue).map(|e| (native(e.pos).0, native(e.pos).1, e.id)).collect();
    assert_eq!(got.len(), 2, "two Spear Goblins come down on X");
    got.sort();
    let mut want_sorted = want.clone();
    want_sorted.sort();
    assert_eq!(got.iter().map(|g| (g.0, g.1)).collect::<Vec<_>>(), want_sorted, "the dismount points (the riders stood at {riders_at:?})");
    for (_, _, id) in &got {
        let v = zapped.entity(*id).unwrap();
        assert_eq!((v.max_hp, v.hp), (133, 133), "a dismount Spear Goblin at level 11, full");
        assert!(v.deploying && v.deploy_ms == 700, "a dismount deploys DeathSpawnDeployTime 700: {} ms", v.deploy_ms);
        assert_eq!(v.acquirable_from, x + 7, "a dismount is a target for enemies from its 8th frame");
    }
}
