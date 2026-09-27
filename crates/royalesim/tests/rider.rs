//! THE ATTACHED RIDER (card.rs `AttachDef`; state.rs `spawn_riders`, `carry_riders`, `rider_deploy_lockstep`,
//! `riders_die_with_their_mounts`, `drop_deprioritized_targets`; target.rs `rider_untouchable`, `deprioritized`;
//! calibration rider.* and targeting.DEPRIORITIZED_TARGET_BUFF): the Ram Rider.
//!
//! THE ROWS (the 15.535.29 tables): the Ram's Spawn* block names the rider row RamRider with SpawnAttach,
//! SpawnNumber 1 and no SpawnPauseTime; the rider row has Hitpoints 232, TargetOnlyTroops, IgnoreTargetsWithBuff
//! BolaSnare with DeprioritizeTargetsWithBuff, Range 5500, LoadTime 700 and HitSpeed 1100, and its bola RamRiderBola
//! deals Damage 41 and hangs BolaSnare (SpeedMultiplier -70) for 2000 ms.
//!
//! THE LAW, measured on client 16.402 on the one Ram Rider of capture 20260920-003751:
//!   - the rider exists from the Ram's first frame, at its position, for its side and at its level (593 hp at level
//!     11, floor(232 x 2.56), beside the Ram's 1766, floor(690 x 2.56)), and deploys with it;
//!   - it stands where the Ram stood a tick before: 53 of 53 tick pairs in one seat, 12 of 12 in the other;
//!   - it enters its attack credited 750 (LoadTime + 50), launches its first bola 7 ticks later and then one every
//!     22, and each lands for 104 at level 11 (floor(41 x 2.56));
//!   - its target reads none exactly on each bola landing tick and is the same Musketeer on the next, the attack
//!     progress unbroken;
//!   - no enemy targeted it and its hp never moved over 146 ticks;
//!   - it is gone on the Ram's last tick, at full hp.
//! TargetOnlyTroops, the choice between a snared and an unsnared troop and an area landing on the pair were never
//! offered in that battle: those tests pin the engine's rule (the column, and the keys' hypotheses).
//!
//! WHAT IS PINNED:
//!   1. the loader takes the Ram's attached-rider block and the rider row, and refuses the shapes the rider law does
//!      not cover, the Goblin Giant's among them;
//!   2. the rider is born one creation after the Ram, at its point and level, and deploys with it; under
//!      rider.DEPLOY = own_deploy_time it counts its own DeployTime instead;
//!   3. the rider stands where the Ram stood a tick before, on every tick of a walk, a charge and a river leap;
//!   4. the rider takes a troop and never a building;
//!   5. nothing targets the rider and nothing lands on it (a Musketeer's shots, a Zap, Arrows); under
//!      rider.TARGETABLE_WHILE_ATTACHED = targetable_damageable the Zap lands on it;
//!   6. the rider dies with the Ram, at full hp;
//!   7. the bola cadence and damage;
//!   8. a bola landing clears the rider's target for one tick and keeps its progress; under rank_last_only it does
//!      not clear it;
//!   9. after a landing the rider takes an unsnared troop over the nearer snared one;
//!  10. a battle with a live rider resumes identically after a save;
//!  11. a Red Ram Rider is the rotation of a Blue one, rider included.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test rider`):
//!   * `rider_walks_itself` -- the rider is not carried and walks on its own: (3) and (11) go red.
//!   * `rider_copies_post_move` -- the rider copies the Ram after the Ram has moved: (3) and (11) go red.
//!   * `target_only_troops_ignored` -- TargetOnlyTroops not read: (4) goes red.
//!   * `rider_targetable` -- the rider is an ordinary target: (5) goes red.
//!   * `rider_outlives_mount` -- the rider outlives the Ram: (6) goes red.
//!   * `deprioritize_rule_off` -- a landing clears nothing: (8) and (9) go red.
//!   * `deprioritize_ignored` -- a carrier ranks by distance: (9) goes red.
//!   * `save_drops_rider_link` -- a save loses the rider's mount: (10) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource};
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DeprioritizedTargetBuff, EntityView, RiderDeploy, RiderTargetable};
use royalesim::{EntityId, Team};

/// The level of the measured battle.
const LEVEL: i32 = 11;

fn cfg() -> BattleConfig {
    let mut c = config();
    c.card_level = [LEVEL, LEVEL];
    c.tower_level = [LEVEL, LEVEL];
    c
}

/// The Ram and its rider on `team`'s side (the rider is the entity whose `attached_to` names a live mount).
fn pair(s: &BattleState, team: Team) -> (EntityView<'_>, EntityView<'_>) {
    let rider = s.entities().find(|e| e.team == team && e.attached_to.is_some()).expect("a rider on the board");
    let ram = s.entity(rider.attached_to.expect("its mount")).expect("its mount stands");
    (ram, rider)
}

/// THE STANDING SCENE: a blue Ram Rider set up at (9, 19) tiles, 2 tiles from a red Cannon at (9, 21), which its
/// Ram (TargetOnlyBuildings; reach Range 800 + both radii 600 + 600 = 2000) attacks from where it stands, so the
/// pair does not move. The Ram's hp is raised so that the Cannon and the red princess towers do not kill it in the
/// test's time. `reds`: red troops (card, tiles x 100) set up beside, in that order.
fn standing(c: BattleConfig, reds: &[(&str, (i32, i32))]) -> (BattleState, EntityId, EntityId) {
    let mut s = BattleState::new(0, c);
    s.scenario_spawn_now(Team::Red, "Cannon", t(900, 2100), None).expect("the Cannon");
    let ram = s.scenario_spawn_now(Team::Blue, "RamRider", t(900, 1900), Some(50_000)).expect("the Ram Rider");
    for (card, (x, y)) in reds {
        s.scenario_spawn_now(Team::Red, card, t(*x, *y), None).expect("a red troop");
    }
    let rider = s.entities().find(|e| e.attached_to == Some(ram)).expect("the Ram's rider").id;
    (s, ram, rider)
}

/// The live red troop of `card` nearest `p`.
fn red(s: &BattleState, card: &str, p: Vec2) -> EntityId {
    s.entities().filter(|e| e.team == Team::Red && e.card == card).min_by_key(|e| e.pos.dist2(p)).expect("the red troop").id
}

// ---------------------------------------------------------------------------
// (1)

#[test]
fn the_loader_takes_the_ram_riders_rider_and_refuses_the_shapes_it_does_not_simulate() {
    let db = cards();
    let ram = db.index("RamRider").unwrap_or_else(|| panic!("RamRider refused: {:?}", db.rejected.iter().find(|(n, _)| n == "RamRider")));
    let c = db.get(ram);
    assert_eq!(c.unit_name, "Ram", "the card's own row is the Ram");
    assert!(c.spawner.is_none(), "the SpawnAttach block was read as a periodic spawner");
    let at = c.attach.expect("the Ram's attached-rider block");
    assert_eq!((at.number, at.radius), (1, None), "SpawnNumber 1, no SpawnRadius");
    let r = db.get(at.unit);
    assert_eq!((r.name.as_str(), r.unit_name.as_str(), r.summon_only, r.hitpoints), ("units.RamRider", "RamRider", true, 232), "the rider row");
    assert!(r.target_only_troops, "TargetOnlyTroops");
    let bola = r.attack_buff.expect("the bola hangs BolaSnare");
    assert_eq!(bola.time_ms, 2000, "BuffTime");
    assert_eq!(db.buffs[bola.buff as usize].speed_pct, -70, "BolaSnare's SpeedMultiplier");
    assert_eq!(r.deprioritize_buff, Some(bola.buff), "the deprioritized buff is the bola's own BolaSnare");
    // The Goblin Giant carries two riders 900 from its centre: refused until rider.OFFSET_LAW is implemented.
    let why = db.rejected.iter().find(|(n, _)| n == "GoblinGiant").map(|(_, w)| w.as_str());
    assert_eq!(why, Some("attached rider SpearGoblinGiant: an offset from its mount (SpawnRadius 900) is not simulated"));

    // The shapes, on a synthetic file: one card per shape, all on the same rider row where they can be.
    let db = CardDb::from_json_str(
        r#"{"cards":[
        {"name":"Mount","kind":"troop","elixir":5,"rarity":"Common","hitpoints":700,"hit_speed_ms":1700,"range_milli":800,
         "collision_radius_milli":600,"spawner":{"character":"Rider","number":1,"attach":true}},
        {"name":"Cadence","kind":"troop","elixir":5,"rarity":"Common","hitpoints":700,"hit_speed_ms":1700,"range_milli":800,
         "collision_radius_milli":600,"spawner":{"character":"Rider","number":1,"pause_time_ms":5000,"attach":true}},
        {"name":"Offset","kind":"troop","elixir":5,"rarity":"Common","hitpoints":700,"hit_speed_ms":1700,"range_milli":800,
         "collision_radius_milli":600,"spawner":{"character":"Rider","number":2,"radius_milli":900,"attach":true}},
        {"name":"Dismount","kind":"troop","elixir":5,"rarity":"Common","hitpoints":700,"hit_speed_ms":1700,"range_milli":800,
         "collision_radius_milli":600,"spawner":{"character":"Dropper","number":1,"attach":true}},
        {"name":"Flyer","kind":"troop","elixir":5,"rarity":"Common","hitpoints":700,"hit_speed_ms":1700,"range_milli":800,
         "collision_radius_milli":600,"spawner":{"character":"Bird","number":1,"attach":true}},
        {"name":"Hut","kind":"building","elixir":5,"rarity":"Common","hitpoints":700,"hit_speed_ms":1700,
         "collision_radius_milli":1000,"spawner":{"character":"Rider","number":1,"attach":true}},
        {"name":"NoPause","kind":"troop","elixir":5,"rarity":"Common","hitpoints":700,"hit_speed_ms":1700,"range_milli":800,
         "collision_radius_milli":600,"spawner":{"character":"Rider","number":1}},
        {"name":"Ignorer","kind":"troop","elixir":3,"rarity":"Common","hitpoints":700,"hit_speed_ms":1700,"range_milli":800,
         "collision_radius_milli":600,"ignore_targets_with_buff":{"name":"Snare","speed_multiplier_raw":-70}}],
        "units":{
        "Rider":{"name":"Rider","rarity":"Common","hitpoints":232,"hit_speed_ms":1100,"range_milli":5500,"collision_radius_milli":600,
         "target_only_troops":true,"ignore_targets_with_buff":{"name":"Snare","speed_multiplier_raw":-70},
         "deprioritize_targets_with_buff":true},
        "Dropper":{"name":"Dropper","rarity":"Common","hitpoints":52,"hit_speed_ms":1600,"range_milli":5000,"collision_radius_milli":500,
         "death_spawn":{"character":"Rider","count":1}},
        "Bird":{"name":"Bird","rarity":"Common","hitpoints":52,"hit_speed_ms":1600,"range_milli":5000,"collision_radius_milli":500,
         "flying_height":4000}}}"#,
        CardSource::DerivedJson,
    )
    .expect("the synthetic file loads");
    let mount = db.index("Mount").unwrap_or_else(|| panic!("Mount refused: {:?}", db.rejected));
    let rider = db.get(db.get(mount).attach.expect("Mount's rider").unit);
    assert!(rider.summon_only && rider.target_only_troops && rider.deprioritize_buff.is_some(), "the rider row's columns");
    let why = |n: &str| db.rejected.iter().find(|(r, _)| r == n).map(|(_, w)| w.clone()).unwrap_or_else(|| panic!("{n} was not refused: {:?}", db.rejected));
    for (card, says) in [
        ("Cadence", "attached rider Rider: a periodic cadence"),
        ("Offset", "attached rider Rider: an offset from its mount (SpawnRadius 900)"),
        ("Dismount", "units.Dropper: an attached rider that leaves something on the board of its own"),
        ("Flyer", "units.Bird: an attached rider that flies (FlyingHeight 4000)"),
        ("Hut", "an attached rider on a building"),
        ("NoPause", "spawner Rider: no SpawnPauseTime"),
        ("Ignorer", "a unit that ignores every target carrying a buff"),
    ] {
        let w = why(card);
        assert!(w.starts_with(says), "{card}: {w}");
        assert!(db.index(card).is_none(), "{card} is still registered");
    }
}

// ---------------------------------------------------------------------------
// (2)

#[test]
fn the_rider_is_born_one_creation_after_the_ram_at_its_point_and_level_and_deploys_with_it() {
    let mut s = BattleState::new(0, cfg());
    s.spawn_unit(Team::Blue, "RamRider", t(900, 900), None).expect("play the Ram Rider");
    s.tick();
    let (ram_id, rider_id) = {
        let (ram, rider) = pair(&s, Team::Blue);
        assert_eq!(rider.team_seq, ram.team_seq + 1, "the rider is created right after the Ram");
        assert_eq!(rider.pos, ram.pos, "the rider is born on the Ram");
        assert_eq!((ram.max_hp, rider.max_hp), (1766, 593), "the Ram and the rider at level {LEVEL}");
        assert!(ram.deploying && rider.deploying, "both deploy");
        (ram.id, rider.id)
    };
    let mut ticks = 0;
    loop {
        let (m, r) = (s.entity(ram_id).expect("the Ram"), s.entity(rider_id).expect("the rider"));
        assert_eq!(r.deploy_ms, m.deploy_ms, "tick {ticks}: one deploy timer");
        assert_eq!(r.deploying, m.deploying, "tick {ticks}");
        if !m.deploying {
            break;
        }
        ticks += 1;
        assert!(ticks < 40, "the Ram never finished deploying");
        s.tick();
    }
    // A setup spawn puts the Ram down deployed: under mirror_mount its rider is deployed with it, under
    // own_deploy_time the rider counts its own DeployTime.
    for (arm, deploying) in [(RiderDeploy::MirrorMount, false), (RiderDeploy::OwnDeployTime, true)] {
        let mut c = cfg();
        c.calib.rider_deploy = arm;
        let mut s = BattleState::new(0, c);
        s.scenario_spawn_now(Team::Blue, "RamRider", t(900, 900), None).expect("the Ram Rider");
        let (ram, rider) = pair(&s, Team::Blue);
        assert!(!ram.deploying, "a setup spawn is deployed");
        assert_eq!(rider.deploying, deploying, "{arm:?}");
    }
}

// ---------------------------------------------------------------------------
// (3)

#[test]
fn the_rider_stands_where_the_ram_stood_a_tick_before() {
    // From the point the shipped Hog Rider leaps the river from (tests/jump16402.rs), with the same jump block: the
    // Ram walks, charges on its 43rd walking tick, leaps and runs on to the tower.
    let mut s = BattleState::new(3, cfg());
    let ram = s.scenario_spawn_now(Team::Blue, "RamRider", Vec2::new(9500 * K, 12500 * K), Some(50_000)).expect("the Ram Rider");
    let rider = s.entities().find(|e| e.attached_to == Some(ram)).expect("the Ram's rider").id;
    let mut last = s.entity(ram).expect("the Ram").pos;
    assert_eq!(s.entity(rider).expect("the rider").pos, last, "the rider is born on the Ram");
    let (mut moved, mut charged, mut leapt) = (0, false, false);
    for k in 1..=220u32 {
        s.tick();
        let (m, r) = (s.entity(ram).expect("the Ram lives"), s.entity(rider).expect("the rider lives"));
        assert_eq!(r.pos, last, "tick {k}: the rider stands at {:?}, the Ram stood at {last:?} a tick before (it is at {:?})", r.pos, m.pos);
        if m.pos != last {
            moved += 1;
        }
        charged |= m.charged;
        leapt |= m.jumping;
        last = m.pos;
    }
    assert!(moved >= 100, "vacuous: the Ram moved on {moved} ticks");
    assert!(charged, "vacuous: the Ram never charged");
    assert!(leapt, "vacuous: the Ram never leapt the river");
}

// ---------------------------------------------------------------------------
// (4)

#[test]
fn the_rider_takes_a_troop_and_never_a_building() {
    // The Cannon alone within the rider's reach: no target, ever.
    let (mut s, ram, rider) = standing(cfg(), &[]);
    let cannon = s.entities().find(|e| e.card == "Cannon").expect("the Cannon").id;
    for k in 1..=40u32 {
        s.tick();
        let (m, r, cn) = (s.entity(ram).expect("the Ram"), s.entity(rider).expect("the rider"), s.entity(cannon).expect("the Cannon"));
        assert_eq!(m.target, Some(cannon), "the scene drifted: the Ram is not on the Cannon on tick {k}");
        let reach = (card_stat(&s, "units.RamRider").range + r.radius + cn.radius) as i64;
        assert!(r.pos.dist2(cn.pos) <= reach * reach, "vacuous: the Cannon is beyond the rider's reach on tick {k}");
        assert_eq!(r.target, None, "tick {k}: the rider took a building");
    }
    // A Musketeer beyond the Cannon: the rider takes the Musketeer.
    let (mut s, _, rider) = standing(cfg(), &[("Musketeer", (900, 2400))]);
    let musk = red(&s, "Musketeer", t(900, 2400));
    s.tick();
    assert_eq!(s.entity(rider).expect("the rider").target, Some(musk), "the rider took something other than the troop");
}

// ---------------------------------------------------------------------------
// (5)

/// The standing scene with a red Musketeer shooting the pair, a red Zap cast on it on tick 5 and red Arrows on tick
/// 10, run 60 ticks: (the rider's lowest hp less its max, whether it was ever stunned, whether anything ever targeted
/// it, whether the Zap stunned the Ram).
fn under_fire(arm: RiderTargetable) -> (i32, bool, bool, bool) {
    let mut c = cfg();
    c.calib.rider_targetable = arm;
    let (mut s, ram, rider) = standing(c, &[("Musketeer", (900, 2400))]);
    let (mut lost, mut stunned, mut targeted, mut ram_stunned) = (0, false, false, false);
    for k in 0..60u32 {
        let at = s.entity(ram).expect("the Ram").pos;
        if k == 5 {
            s.spawn_unit(Team::Red, "Zap", at, None).expect("cast Zap");
        }
        if k == 10 {
            s.spawn_unit(Team::Red, "Arrows", at, None).expect("cast Arrows");
        }
        s.tick();
        let Some(r) = s.entity(rider) else { break };
        lost = lost.max(r.max_hp - r.hp);
        stunned |= r.stun_ms > 0;
        targeted |= s.entities().any(|e| e.target == Some(rider));
        ram_stunned |= s.entity(ram).is_some_and(|m| m.stun_ms > 0);
    }
    (lost, stunned, targeted, ram_stunned)
}

#[test]
fn nothing_targets_the_rider_and_nothing_lands_on_it() {
    let (lost, stunned, targeted, ram_stunned) = under_fire(RiderTargetable::UntargetableImmune);
    assert!(ram_stunned, "vacuous: the Zap never stunned the Ram, so it may not have landed on the pair");
    assert_eq!(lost, 0, "the rider lost hp");
    assert!(!stunned, "the rider was stunned");
    assert!(!targeted, "an enemy targeted the rider");
    // The other arm: the Zap and the Arrows land on the rider too.
    let (lost, stunned, _, _) = under_fire(RiderTargetable::TargetableDamageable);
    assert!(lost > 0 && stunned, "targetable_damageable: the rider lost {lost} and stunned {stunned}");
}

// ---------------------------------------------------------------------------
// (6)

#[test]
fn the_rider_dies_with_the_ram_at_full_hp() {
    let mut s = BattleState::new(0, cfg());
    s.scenario_spawn_now(Team::Red, "Cannon", t(900, 2100), None).expect("the Cannon");
    let ram = s.scenario_spawn_now(Team::Blue, "RamRider", t(900, 1900), Some(1)).expect("the Ram Rider at 1 hp");
    let rider = s.entities().find(|e| e.attached_to == Some(ram)).expect("the Ram's rider").id;
    for k in 1..=200u32 {
        s.tick();
        match (s.entity(ram), s.entity(rider)) {
            (Some(_), Some(r)) => assert_eq!(r.hp, r.max_hp, "tick {k}: the rider lost hp before the Ram fell"),
            (None, Some(_)) => panic!("tick {k}: the rider outlived the Ram"),
            (Some(_), None) => panic!("tick {k}: the rider fell before the Ram"),
            (None, None) => {
                assert!(k > 1, "vacuous: the Ram never stood a tick");
                return;
            }
        }
    }
    panic!("the scene drifted: the Ram at 1 hp stood 200 ticks beside the Cannon");
}

// ---------------------------------------------------------------------------
// (7), (8)

/// Per tick of the standing scene with red Musketeers set up at `reds` (the first is the one watched), under
/// `arm`: the rider's target, its attack phase and progress, and the hp the watched Musketeer lost on the tick.
struct Row {
    target: Option<EntityId>,
    phase: AttackPhase,
    progress: i32,
    lost: i32,
}

fn bola_rows(arm: DeprioritizedTargetBuff, reds: &[(&str, (i32, i32))], ticks: u32) -> (Vec<Row>, Vec<EntityId>) {
    let mut c = cfg();
    c.calib.deprioritized_target_buff = arm;
    let (mut s, _, rider) = standing(c, reds);
    let ids: Vec<EntityId> = reds.iter().map(|(card, (x, y))| red(&s, card, t(*x, *y))).collect();
    let mut hp = s.entity(ids[0]).expect("the watched Musketeer").hp;
    let mut rows = Vec::new();
    for _ in 0..ticks {
        s.tick();
        let r = s.entity(rider).expect("the rider lives");
        let now = s.entity(ids[0]).map_or(0, |m| m.hp);
        rows.push(Row { target: r.target, phase: r.attack_phase, progress: r.attack_ms, lost: hp - now });
        hp = now;
    }
    (rows, ids)
}

/// The launch cadence of the shipped attack cycle on the rider's rows: enters credited LoadTime + 50 = 750, fires
/// when the progress reaches HitSpeed 1100, 7 ticks later, and every HitSpeed / 50 = 22 after.
#[test]
fn the_rider_enters_at_750_and_launches_7_ticks_later_then_every_22_each_bola_landing_for_104() {
    let (rows, _) = bola_rows(DeprioritizedTargetBuff::RescanOnLandingKeepProgress, &[("Musketeer", (900, 2400))], 80);
    let entry = rows.iter().position(|r| r.phase != AttackPhase::Idle).expect("the rider never entered its attack");
    assert_eq!(rows[entry].progress, 750, "the entry credit");
    let launches: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r.phase == AttackPhase::Cooldown).map(|(k, _)| k).collect();
    assert!(launches.len() >= 3, "vacuous: {} launches", launches.len());
    assert_eq!(&launches[..3], &[entry + 7, entry + 29, entry + 51], "the launch ticks (entry on {entry})");
    let losses: Vec<i32> = rows.iter().map(|r| r.lost).filter(|l| *l > 0).collect();
    assert!(losses.len() >= 3, "vacuous: {} bolas landed", losses.len());
    assert_eq!(&losses[..3], &[104, 104, 104], "a bola at level {LEVEL}");
}

#[test]
fn a_bola_landing_clears_the_riders_target_for_one_tick_and_keeps_its_progress() {
    let reds = [("Musketeer", (900, 2400))];
    let (rows, ids) = bola_rows(DeprioritizedTargetBuff::RescanOnLandingKeepProgress, &reds, 80);
    let musk = Some(ids[0]);
    let landings: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r.lost > 0).map(|(k, _)| k).collect();
    assert!(landings.len() >= 3 && landings[2] + 1 < rows.len(), "vacuous: {} landings", landings.len());
    for &l in &landings[..3] {
        assert_eq!(rows[l].target, None, "landing tick {l}: the rider kept its target");
        assert_eq!(rows[l + 1].target, musk, "tick {}: the rider did not take the Musketeer back", l + 1);
        assert_eq!(rows[l + 1].progress, rows[l].progress + 50, "tick {}: the attack progress broke", l + 1);
    }
    let first = rows.iter().position(|r| r.target.is_some()).expect("the rider never took a target");
    for (k, r) in rows.iter().enumerate().take(landings[2] + 2).skip(first) {
        if !landings.contains(&k) {
            assert_eq!(r.target, musk, "tick {k}: the rider's target off a landing tick");
        }
    }
    // The other arm keeps the target through every landing.
    let (rows, ids) = bola_rows(DeprioritizedTargetBuff::RankLastOnly, &reds, 80);
    let first = rows.iter().position(|r| r.target.is_some()).expect("the rider never took a target");
    assert!(rows.iter().filter(|r| r.lost > 0).count() >= 3, "vacuous: rank_last_only landed fewer than 3 bolas");
    assert!(rows[first..].iter().all(|r| r.target == Some(ids[0])), "rank_last_only: the rider dropped its target");
}

// ---------------------------------------------------------------------------
// (9)

#[test]
fn after_a_landing_the_rider_takes_an_unsnared_troop_over_the_nearer_snared_one() {
    // A nearer (3.35 tiles from the rider) and B farther (4.92), both within its reach of 6.6 and both shooting the
    // Ram, so both stand.
    let reds = [("Musketeer", (1050, 2200)), ("Musketeer", (700, 2350))];
    let mut c = cfg();
    c.calib.deprioritized_target_buff = DeprioritizedTargetBuff::RescanOnLandingKeepProgress;
    let (mut s, _, rider) = standing(c, &reds);
    let (a, b) = (red(&s, "Musketeer", t(1050, 2200)), red(&s, "Musketeer", t(700, 2350)));
    let snare = card_stat(&s, "units.RamRider").deprioritize_buff.expect("the rider deprioritizes BolaSnare");
    s.tick();
    assert_eq!(s.entity(rider).expect("the rider").target, Some(a), "the scene drifted: the rider did not take the nearer Musketeer first");
    let mut hp = s.entity(a).expect("A").hp;
    for k in 2..=60u32 {
        s.tick();
        let now = s.entity(a).expect("A lives").hp;
        if now < hp {
            // the landing tick: the next Target phase rescans with A snared
            s.tick();
            let snared = s.entity(a).expect("A lives").buffs.iter().any(|sl| u32::from(sl.id) == u32::from(snare) + 1);
            assert!(snared, "vacuous: A does not carry BolaSnare after the landing");
            assert_eq!(s.entity(rider).expect("the rider").target, Some(b), "tick {}: the rider did not take the unsnared Musketeer", k + 1);
            return;
        }
        hp = now;
    }
    panic!("the scene drifted: no bola landed on A in 60 ticks");
}

// ---------------------------------------------------------------------------
// (10)

#[test]
fn a_battle_with_a_live_rider_resumes_identically_after_a_save() {
    let (mut s, _, rider) = standing(cfg(), &[("Musketeer", (900, 2400))]);
    for _ in 0..12 {
        s.tick();
    }
    let rider_card = s.entity(rider).expect("the rider").card_idx;
    assert!(s.projectiles().iter().any(|p| p.firer_card == Some(rider_card)), "vacuous: no bola in flight at the save");
    let blob = s.save();
    let mut live = s.clone();
    let mut loaded = BattleState::load(&blob).unwrap_or_else(|e| panic!("the save does not load: {e}"));
    for k in 1..=80u32 {
        live.tick();
        loaded.tick();
        assert_eq!(live.state_hash(), loaded.state_hash(), "the resumed battle diverged {k} ticks after the load");
    }
}

// ---------------------------------------------------------------------------
// (11)

#[test]
fn a_red_ram_rider_is_the_rotation_of_a_blue_one() {
    let mut c = symmetric_config();
    c.card_level = [LEVEL, LEVEL];
    c.tower_level = [LEVEL, LEVEL];
    let mut s = BattleState::new(0, c);
    let p = t(350, 1000);
    let q = mirror(&s, p);
    s.scenario_spawn_batch(&[(Team::Blue, "RamRider", p, Some(50_000)), (Team::Red, "RamRider", q, Some(50_000))]).expect("the two Ram Riders");
    let mut last: Option<(Vec2, Vec2)> = None;
    let mut moved = 0;
    for k in 1..=150u32 {
        s.tick();
        let (bram, brider) = pair(&s, Team::Blue);
        let (rram, rrider) = pair(&s, Team::Red);
        assert_eq!(rram.pos, mirror(&s, bram.pos), "tick {k}: the Rams");
        assert_eq!(rrider.pos, mirror(&s, brider.pos), "tick {k}: the riders");
        if let Some((b, r)) = last {
            assert_eq!((brider.pos, rrider.pos), (b, r), "tick {k}: a rider is not where its Ram stood a tick before");
            if bram.pos != b {
                moved += 1;
            }
        }
        last = Some((bram.pos, rram.pos));
    }
    assert!(moved >= 50, "vacuous: the Rams moved on {moved} ticks");
}
