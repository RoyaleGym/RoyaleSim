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
//!   * `own_area_skips_rider` and `rider_buff_dropped_in_resolve` -- rider.OWN_AREA_BUFFS_WHILE_ATTACHED = client16402_buffed
//!     still refuses the rider (spell.rs `eligible`; state.rs `apply_effects`):
//!     `under_client16402_buffed_an_own_rage_speeds_the_giants_riders` goes red.
//!   * `rider_facing_noted_at_tick_start` -- rider.OFFSET_FACING = client16635_after_attack still notes the mounts'
//!     facings at the top of the tick: `under_client16635_after_attack_the_riders_take_the_giants_attack_turn_on_its_tick`
//!     goes red.
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

/// The rider row and its dismount's record. The loader resolves a unit by NAME: no card is named SpearGoblin (the
/// Spear Goblins card is SpearGoblins, whose unit row is SpearGoblin), so the dismount is the summon-only SpearGoblin
/// record, not that card.
const RIDER: &str = "SpearGoblinGiant";
const DISMOUNT: &str = "SpearGoblin";

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
    assert_eq!((d.name.as_str(), d.unit_name.as_str(), d.summon_only), (DISMOUNT, "SpearGoblin", true), "the dismount unit: the summon-only SpearGoblin record");
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

/// A Blue Goblin Giant held on (9500, 10500), a Red Knight held 3,500 ahead (its riders' target), and on the 40th tick a Blue
/// Rage on the Giant, under rider.OWN_AREA_BUFFS_WHILE_ATTACHED = `arm` and rider.MOUNT_BUFFS_ON_CLOCK = own_buffs (the scene
/// isolates the area-buff arm; under the shipped client16635_mount_buffs a rider's clock reads the raged Giant's buffs too):
/// over the next 40 ticks, the riders' ticks whose attack progress stepped 65 (raged, x1.3) and those that stepped 50, and
/// whether a rider carried a buff.
fn raged_riders(arm: royalesim::state::OwnAreaBuffsWhileAttached) -> (u32, u32, bool) {
    let mut cfg = shipped();
    cfg.calib.own_area_buffs_while_attached = arm;
    cfg.calib.mount_buffs_on_clock = royalesim::state::MountBuffsOnClock::OwnBuffs;
    let mut s = BattleState::new(0, cfg);
    let (g_at, k_at) = (at((9500, 10500)), at((9500, 14000)));
    s.spawn_unit(Team::Blue, "GoblinGiant", g_at, None).expect("play the Goblin Giant");
    s.tick();
    let g = s.entities().find(|e| e.card == "GoblinGiant").expect("the Giant").id;
    let rs = riders(&s, g);
    assert_eq!(rs.len(), 2, "two riders");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).expect("the Knight");
    let hold = |s: &mut BattleState| {
        assert!(s.debug_set_pos(g, g_at));
        let full = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_pos(knight, k_at) && s.debug_set_hp(knight, full));
    };
    for _ in 0..40 {
        hold(&mut s);
        s.tick();
    }
    s.spawn_unit(Team::Blue, "Rage", g_at, None).expect("cast Rage");
    let (mut raged, mut plain, mut buffed) = (0, 0, false);
    let mut last: Vec<i32> = rs.iter().map(|r| s.entity(*r).expect("a rider").attack_ms).collect();
    for _ in 0..40 {
        hold(&mut s);
        s.tick();
        for (k, r) in rs.iter().enumerate() {
            let v = s.entity(*r).expect("a rider");
            buffed |= v.buffs.iter().any(|b| !b.is_empty());
            match v.attack_ms - last[k] {
                65 => raged += 1,
                50 => plain += 1,
                _ => {}
            }
            last[k] = v.attack_ms;
        }
    }
    (raged, plain, buffed)
}

/// rider.OWN_AREA_BUFFS_WHILE_ATTACHED = client16402_buffed (client 16.402, parity's r73 census: the rider raged in 3 of 3
/// Rage casts, its progress 65 a tick; the engine's 50 on all 21 ticks): a Blue Rage on the Goblin Giant hangs its buff on
/// his riders and their attack progress steps 65; under immune (the vacuity check) no rider carries it and none steps 65.
/// Plant: own_area_skips_rider.
#[test]
fn under_client16402_buffed_an_own_rage_speeds_the_giants_riders() {
    use royalesim::state::OwnAreaBuffsWhileAttached as A;
    let (raged, plain, buffed) = raged_riders(A::Immune);
    assert!(plain > 0, "the scene drifted: the riders never attacked ({plain})");
    assert!(raged == 0 && !buffed, "immune: a rider carried the Rage ({raged} raged ticks, buffed {buffed})");
    let (raged, _, buffed) = raged_riders(A::Client16402Buffed);
    assert!(raged > 0 && buffed, "client16402_buffed: no rider raged ({raged} raged ticks, buffed {buffed})");
}

// ---------------------------------------------------------------------------
// (4) rider.OFFSET_FACING

use royalesim::entity::AttackPhase;
use royalesim::state::RiderOffsetFacing;

/// One tick of `attack_turn`: (the tick, the Giant's point, its facing, whether it is in its attack, its riders' points by
/// rank).
type TurnRow = (u32, Vec2, Vec2, bool, [Vec2; 2]);

/// A RED GOBLIN GIANT THAT TURNS TO ATTACK, under rider.OFFSET_FACING = `arm`: the Giant put down deployed on
/// (9000, 13000), out of every blue tower's sight, walks its lane for 11 ticks; then a blue Cannon is put down deployed
/// 1,500 to its side, square to its heading (inside its reach, and the nearest building), so it takes the Cannon and
/// starts its attack turned toward it (movement.ATTACK_FACING). (A, its first tick in its attack; per tick from its
/// first: `TurnRow`).
fn attack_turn(arm: RiderOffsetFacing) -> (u32, Vec<TurnRow>) {
    let mut cfg = shipped();
    cfg.calib.rider_offset_facing = arm;
    let mut s = BattleState::new(0, cfg);
    let g = s.scenario_spawn_now(Team::Red, "GoblinGiant", at((9000, 13000)), None).expect("the Giant");
    let rs = riders(&s, g);
    assert_eq!(rs.len(), 2, "two riders");
    let row = |s: &BattleState| -> TurnRow {
        let v = s.entity(g).expect("the Giant lives");
        let r = [0usize, 1].map(|k| s.entity(rs[k]).expect("a rider lives").pos);
        (s.tick_count() - 1, v.pos, v.facing, v.attack_phase != AttackPhase::Idle, r)
    };
    let mut rows: Vec<TurnRow> = Vec::new();
    for _ in 0..11 {
        s.tick();
        rows.push(row(&s));
    }
    let last = *rows.last().expect("a row");
    assert!(rows.iter().all(|r| !r.3), "the scene drifted: the Giant attacked before the Cannon");
    assert_ne!(rows[0].1, last.1, "the scene drifted: the Giant did not walk");
    let (p, f) = (native(last.1), last.2);
    let side = (-f.y * 1500 / 256, f.x * 1500 / 256);
    s.scenario_spawn_now(Team::Blue, "Cannon", at((p.0 + side.0, p.1 + side.1)), None).expect("the Cannon");
    let mut a: Option<u32> = None;
    for _ in 0..60 {
        s.tick();
        let r = row(&s);
        rows.push(r);
        if r.3 && a.is_none() {
            a = Some(r.0);
        }
        if a.is_some_and(|a| r.0 == a + 1) {
            break;
        }
    }
    let a = a.expect("the scene drifted: the Giant never attacked the Cannon");
    (a, rows)
}

/// The `attack_turn` row of tick `t`.
fn turn_row(rows: &[TurnRow], t: u32) -> TurnRow {
    *rows.iter().find(|r| r.0 == t).unwrap_or_else(|| panic!("no frame {t}"))
}

/// The scene's checks: the Giant walked into its attack on A, its facing turned there by enough to move each rider's arc
/// 50 or more, and it holds that facing on A + 1.
fn turned(prev: TurnRow, now: TurnRow, next: TurnRow) {
    assert!(!prev.3 && now.3, "the scene drifted: not the attack's first tick");
    assert_ne!(now.2, prev.2, "the scene drifted: the Giant's attack started without a turn");
    for k in 0..2 {
        let (a, b) = (arc(prev.2, k), arc(now.2, k));
        let (dx, dy) = (i64::from(a.0 - b.0), i64::from(a.1 - b.1));
        assert!(dx * dx + dy * dy >= 2500, "the scene drifted: rider {k}'s arc moves {a:?} -> {b:?} only");
    }
    assert_eq!(next.2, now.2, "the scene drifted: the Giant turned again on A + 1");
}

/// rider.OFFSET_FACING = client16635_after_attack (client 16.635, parity r100, the oracle's il-mg-16635 set: 38 of 38
/// attack-phase turns of a Goblin Giant within 16, 24 exact; the old arm 78 to 541 off on all 26 attack starts): on the
/// tick A its attack starts, each rider stands where the Giant stood on A - 1 plus the arc at the facing A's Attack phase
/// gave it, and on A + 1 where it stood on A plus the same arc. Plant: rider_facing_noted_at_tick_start.
#[test]
fn under_client16635_after_attack_the_riders_take_the_giants_attack_turn_on_its_tick() {
    let (a, rows) = attack_turn(RiderOffsetFacing::Client16635AfterAttack);
    let (prev, now, next) = (turn_row(&rows, a - 1), turn_row(&rows, a), turn_row(&rows, a + 1));
    turned(prev, now, next);
    for (k, (r, r1)) in now.4.into_iter().zip(next.4).enumerate() {
        assert_eq!(native(r.sub(prev.1)), arc(now.2, k as i32), "client16635_after_attack: rider {k} on A ({a})");
        assert_eq!(native(r1.sub(now.1)), arc(now.2, k as i32), "client16635_after_attack: rider {k} on A + 1");
    }
}

/// rider.OFFSET_FACING = tick_start (the vacuity check): on A each rider stands on the arc at the Giant's facing of A - 1,
/// its walking heading; on A + 1 on the arc at the turned facing.
#[test]
fn under_tick_start_they_take_it_a_tick_later() {
    let (a, rows) = attack_turn(RiderOffsetFacing::TickStart);
    let (prev, now, next) = (turn_row(&rows, a - 1), turn_row(&rows, a), turn_row(&rows, a + 1));
    turned(prev, now, next);
    for (k, (r, r1)) in now.4.into_iter().zip(next.4).enumerate() {
        assert_eq!(native(r.sub(prev.1)), arc(prev.2, k as i32), "tick_start: rider {k} on A ({a})");
        assert_eq!(native(r1.sub(now.1)), arc(now.2, k as i32), "tick_start: rider {k} on A + 1");
    }
}

/// A walking Giant's riders read its facing as the last tick left it under both arms of rider.OFFSET_FACING: the move
/// pass turns it after the Path phase's note. Plant: rider_offset_ignored.
#[test]
fn under_both_offset_facing_arms_a_walking_giants_riders_take_its_last_ticks_facing() {
    for arm in [RiderOffsetFacing::TickStart, RiderOffsetFacing::Client16635AfterAttack] {
        let mut cfg = shipped();
        cfg.calib.rider_offset_facing = arm;
        for (team, p) in [(Team::Blue, (9500, 10500)), (Team::Red, (8500, 21500))] {
            let (rows, walked, headings) = ride(cfg.clone(), team, p);
            assert!(walked >= 30, "{arm:?}: vacuous: the {team:?} Giant walked on {walked} ticks");
            assert!(headings.len() >= 2, "{arm:?}: vacuous: the {team:?} Giant walked at one heading: {headings:?}");
            for (t, row) in rows.iter().enumerate() {
                for (k, (got, want)) in row.iter().enumerate() {
                    assert_eq!(got, want, "{arm:?}: {team:?} rider {k} on the Giant's tick {}", t + 1);
                }
            }
        }
    }
}

/// rider.OFFSET_FACING ships at tick_start (client16635_after_attack is a candidate until scored).
#[test]
fn the_shipped_arm_is_tick_start() {
    assert_eq!(royalesim::state::Calib::shipped().rider_offset_facing, RiderOffsetFacing::TickStart);
    assert_eq!(shipped().calib.rider_offset_facing, RiderOffsetFacing::TickStart);
}
