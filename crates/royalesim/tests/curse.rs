//! A BUFF WHOSE CARRIER LEAVES A UNIT WHEN IT DIES (status.rs `BuffDeathSpawn`; state.rs `phase_reap`, `land_buff`,
//! `apply_effects`, `tick_status_timers`), on the Mother Witch: her VoodooCurse, hung by her projectile before its
//! damage (status.APPLY_BUFF_BEFORE_DAMAGE), leaves a VoodooHog for her side where its carrier dies.
//!
//! THE LAW, measured on client 15.535.29 (H the tick her impact curses a unit, D the tick a cursed unit dies):
//!   - every unit her first impact on it killed left a hog on that tick (status.APPLY_BUFF_BEFORE_DAMAGE =
//!     lands_on_a_unit_the_hit_kills);
//!   - a cursed unit that dies to anything leaves a hog, an uncursed one does not; the curse lasts its 5000 ms (a hog
//!     from a death on H + 97, with the Witch dead since H + 3; none on H + 210);
//!   - the hog is born 1100 ahead of a standing victim, along the victim's side's forward (the two radii, 500 + 600;
//!     status.BUFF_DEATH_SPAWN_POINT), deploys its 200 ms, 4 ticks (status.BUFF_DEATH_SPAWN_DEPLOY_TIME), and first
//!     steps on D + 5;
//!   - the curse never lands on a building (IgnoreBuildings) nor on a unit whose row lists it (the Golem's IgnoreBuff);
//!     the Golem's Golemites carry no such list.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the Witch with one curse (her TargetBuff and BuffOnDamage name one buff, loaded once), landing
//!      before the damage, releasing a VoodooHog for the other side; two different buffs keep today's refusal; two rows
//!      whose columns agree and whose death units differ intern apart; a death spawn of two units is refused;
//!   2. three Skeletons each killed by her first impact on it leave a hog on their death tick;
//!   3. a cursed unit killed by anything (set to 0, a Zap) leaves a hog, an uncursed one killed with it none;
//!   4. the hog is born 1100 ahead of a standing victim, at her level, deploying 4 ticks, first step D + 5;
//!   5. the curse lasts 5000 ms: a hog from a death on H + 97 (the Witch dead) and on H + 100 (the curse's last tick,
//!      the engine's boundary), none from H + 101 or H + 210;
//!   6. the Golem takes her hit and never the curse, and leaves no hog; its Golemite takes the curse and leaves one;
//!   7. a building she hits never carries the curse and leaves no hog;
//!   8. a cursed unit's own death spawn still happens beside the hog (the Giant Skeleton's bomb);
//!   9. the Red scene is the rotation of the Blue one; a shot's before-damage flag is state (a save edited only in it
//!      fails the load's hash self-check), and a battle saved with a curse live resumes it.
//!
//! No measurement on disk: the exact curse end in (97, 105], a side-1 Witch (the axis of the 1100), a refresh by a
//! second impact, the hog's level when the victim's differs, deaths over the river.
//!
//! THE SCENE (`witch_scene`): a Blue Cannon, a Red Knight attacking it (so the Knight stands), and a Blue Mother Witch
//! 5500 south of the Knight, who shoots it. Every point is on ground, and the hog's point is out of every Red tower's
//! reach.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test curse`):
//!   * `attack_buff_pair_refused` -- a TargetBuff and a BuffOnDamage naming one buff are refused: (1) goes red.
//!   * `buff_interns_without_death_unit` -- rows differing only in their death unit merge: (1) goes red.
//!   * `buff_death_spawn_unloaded` -- the loader never loads a buff's death unit, so the card is refused: (1) goes red.
//!   * `curse_survivors_only` -- a unit the impact kills takes no curse: (2) goes red.
//!   * `buff_death_spawn_dropped` -- no unit for a buff's death spawn: (2), (3) and (8) go red.
//!   * `buff_death_spawn_at_death_point` -- the hog on the death point: (4) goes red.
//!   * `curse_expires_before_reap` -- the slot of a unit that dies on the curse's last tick expires before Reap: (5)
//!     goes red.
//!   * `ignore_buff_not_read` -- IgnoreBuff is not read: (6) goes red.
//!   * `curse_on_buildings` -- the curse lands on buildings: (7) goes red.
//!   * `hash_skips_buff_source` -- a shot's before-damage flag and a slot's source level are not hashed: (9) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{ApplyBuffBeforeDamage, BattleConfig, BattleState, BuffDeathSpawnDeploy, BuffDeathSpawnPoint};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The shipped config, asserting the arms this file pins.
fn shipped() -> BattleConfig {
    let c = config();
    assert_eq!(c.calib.apply_buff_before_damage, ApplyBuffBeforeDamage::LandsOnAUnitTheHitKills, "the shipped status.APPLY_BUFF_BEFORE_DAMAGE");
    assert_eq!(c.calib.buff_death_spawn_point, BuffDeathSpawnPoint::SameLocationOrVictimForward, "the shipped status.BUFF_DEATH_SPAWN_POINT");
    assert_eq!(c.calib.buff_death_spawn_deploy, BuffDeathSpawnDeploy::UnitDeployTime, "the shipped status.BUFF_DEATH_SPAWN_DEPLOY_TIME");
    c
}

/// The VoodooCurse's `CardDb::buffs` index.
fn voodoo(s: &BattleState) -> u16 {
    card_stat(s, "WitchMother").attack_buff.expect("the Witch hangs a buff").buff
}

fn cursed(s: &BattleState, id: EntityId) -> bool {
    let b = voodoo(s);
    s.entity(id).is_some_and(|v| v.buffs.iter().any(|sl| sl.id == b + 1))
}

fn hogs(s: &BattleState) -> Vec<EntityId> {
    s.entities().filter(|e| e.card == "VoodooHog").map(|e| e.id).collect()
}

/// The standing-victim scene (THE SCENE in the header), Blue's Witch on side 0, or its rotation with the sides swapped
/// when `rotated`. Returns (battle, witch, knight, cannon).
fn witch_scene(cfg: BattleConfig, rotated: bool) -> (BattleState, EntityId, EntityId, EntityId) {
    let mut s = BattleState::new(0, cfg);
    let (own, other) = if rotated { (Team::Red, Team::Blue) } else { (Team::Blue, Team::Red) };
    let place = |s: &BattleState, p: (i32, i32)| if rotated { mirror(s, at(p)) } else { at(p) };
    let (pc, pk, pw) = (place(&s, (8000, 20000)), place(&s, (9900, 20000)), place(&s, (9900, 14500)));
    let cannon = s.scenario_spawn_now(own, "Cannon", pc, None).expect("spawn the Cannon");
    let knight = s.scenario_spawn_now(other, "Knight", pk, None).expect("spawn the Knight");
    let witch = s.scenario_spawn_now(own, "WitchMother", pw, None).expect("spawn the Witch");
    (s, witch, knight, cannon)
}

/// Tick until `id` carries the curse; the tick count from the scene's start (H is the last tick run).
fn until_cursed(s: &mut BattleState, id: EntityId) -> u32 {
    let n = run_until(s, 80, |s| cursed(s, id));
    assert!(cursed(s, id), "the scene drifted: the Witch's curse never landed in 80 ticks");
    n
}

// ---------------------------------------------------------------------------
// (1)

/// A synthetic troop that shoots `target_buff` (and hangs `buff_on_damage` by its own hit, when set).
fn troop(name: &str, target_buff: &str, buff_on_damage: &str) -> String {
    format!(
        r#"{{"name":"{name}","kind":"troop","elixir":4,"rarity":"Common","hitpoints":207,"hit_speed_ms":1000,"range_milli":5500,
        "collision_radius_milli":500,"projectile":{{"name":"{name}Shot","speed":600,"damage":52,"target_buff":{target_buff},
        "buff_time_ms":5000,"apply_buff_before_damage":true}}{buff_on_damage}}}"#
    )
}

/// A curse: a buff whose carrier leaves `count` of `unit` for the other side.
fn curse(name: &str, unit: &str, count: i32) -> String {
    format!(
        r#"{{"name":"{name}","death_spawn":{{"character":"{unit}","count":{count},"is_enemy":true,"deploy_delay":true,
        "same_location":false,"other_buff_death_spawn_allowed":true}},"ignore_buildings":true}}"#
    )
}

const UNITS: &str = r#""units":{
  "Pig":{"name":"Pig","rarity":"Common","hitpoints":246,"hit_speed_ms":1200,"range_milli":750,"collision_radius_milli":600,"speed":120,"deploy_time_ms":200},
  "Boar":{"name":"Boar","rarity":"Common","hitpoints":300,"hit_speed_ms":1200,"range_milli":750,"collision_radius_milli":600,"speed":120,"deploy_time_ms":200}}"#;

/// Plants: attack_buff_pair_refused, buff_interns_without_death_unit, buff_death_spawn_unloaded.
#[test]
fn the_witch_loads_with_one_curse() {
    let s = BattleState::new(0, shipped());
    let w = card_stat(&s, "WitchMother");
    let b = w.attack_buff.expect("her curse");
    assert_eq!(b.time_ms, 5000, "BuffTime / BuffOnDamageTime");
    assert!(w.attack_buff_first, "her projectile lands its buff before its damage");
    let def = s.cards().buffs[b.buff as usize];
    assert!(def.ignore_buildings && !def.pulses(), "VoodooCurse: {def:?}");
    let ds = def.death_spawn.expect("the curse releases a unit");
    assert_eq!((ds.count, ds.for_other_side, ds.deploy_delay, ds.same_location), (1, true, true, false));
    let hog = s.cards().get(ds.unit);
    assert_eq!((hog.name.as_str(), hog.summon_only), ("VoodooHog", true), "the curse's unit");
    assert!(hog.ignore_buffs.contains(&b.buff), "the hog's row lists the curse it came from (IgnoreBuff)");
    let catalogue: Vec<u16> = ["Knight", "WitchMother", "Zap"].iter().map(|n| s.cards().index(n).expect("loads")).collect();
    assert_eq!(royalesim::py::ids_of_indices(s.cards(), &catalogue)[ds.unit as usize], 1, "the hog reports the Witch's catalogue id");
    // Synthetic rows. Two different buffs from one unit keep today's refusal.
    let slow = r#"{"name":"Slow","speed_multiplier_raw":-30}"#;
    let text = format!(r#"{{"cards":[{}],{UNITS}}}"#, troop("Hexer", &curse("PigCurse", "Pig", 1), &format!(r#","buff_on_damage":{{"buff":{slow},"time_ms":5000}}"#)));
    let db = CardDb::from_json_str(&text, CardSource::DerivedJson).unwrap();
    let why = &db.rejected.iter().find(|(n, _)| n == "Hexer").expect("two different buffs are refused").1;
    assert!(why.contains("the unit carries both a projectile TargetBuff and a BuffOnDamage"), "{why}");
    // Two rows whose columns agree and whose death units differ intern apart, each with its own unit.
    let text = format!(r#"{{"cards":[{},{}],{UNITS}}}"#, troop("PigWitch", &curse("PigCurse", "Pig", 1), ""), troop("BoarWitch", &curse("BoarCurse", "Boar", 1), ""));
    let db = CardDb::from_json_str(&text, CardSource::DerivedJson).unwrap();
    let buff_of = |n: &str| db.get(db.index(n).unwrap_or_else(|| panic!("{n} refused: {:?}", db.rejected))).attack_buff.expect("a curse").buff;
    let (pig, boar) = (buff_of("PigWitch"), buff_of("BoarWitch"));
    assert_ne!(pig, boar, "two curses whose death units differ merged into one buff");
    let unit_of = |b: u16| db.get(db.buffs[b as usize].death_spawn.expect("a death spawn").unit).name.clone();
    assert_eq!((unit_of(pig), unit_of(boar)), ("Pig".to_string(), "Boar".to_string()));
    // A death spawn of two units is refused.
    let text = format!(r#"{{"cards":[{}],{UNITS}}}"#, troop("Pigs2", &curse("TwoPigs", "Pig", 2), ""));
    let db = CardDb::from_json_str(&text, CardSource::DerivedJson).unwrap();
    let why = &db.rejected.iter().find(|(n, _)| n == "Pigs2").expect("a death spawn of two is refused").1;
    assert!(why.contains("a buff death spawn of 2 units"), "{why}");
}

// ---------------------------------------------------------------------------
// (2)

/// Plants: curse_survivors_only, buff_death_spawn_dropped.
#[test]
fn the_impact_that_kills_also_curses() {
    let mut s = BattleState::new(0, shipped());
    // The Witch on Red's half just north of the river, out of every tower's reach; three Red Skeletons 4000 to 4600
    // north of her walk at her, and each dies to her first impact on it (133 at level 11 against 81).
    let witch = s.scenario_spawn_now(Team::Blue, "WitchMother", at((9900, 17500)), None).expect("spawn the Witch");
    let skeletons = s
        .scenario_spawn_batch(&[(Team::Red, "Skeletons", at((9900, 21500)), None), (Team::Red, "Skeletons", at((9100, 22000)), None), (Team::Red, "Skeletons", at((10700, 22000)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (mut deaths, mut born) = (Vec::new(), Vec::new());
    let mut known = hogs(&s);
    for t in 0..150u32 {
        let alive_before: Vec<EntityId> = skeletons.iter().copied().filter(|id| s.entity(*id).is_some()).collect();
        s.tick();
        let died = alive_before.iter().filter(|id| s.entity(**id).is_none()).count();
        let new: Vec<EntityId> = hogs(&s).into_iter().filter(|h| !known.contains(h)).collect();
        known.extend(new.iter().copied());
        if died > 0 {
            deaths.push((t, died));
        }
        if !new.is_empty() {
            born.push((t, new.len()));
        }
    }
    assert!(s.entity(witch).is_some(), "the scene drifted: the Witch died");
    assert_eq!(deaths.iter().map(|d| d.1).sum::<usize>(), 3, "the scene drifted: {deaths:?} Skeleton deaths");
    assert_eq!(born, deaths, "a hog on each tick a Skeleton died to the impact that cursed it");
}

// ---------------------------------------------------------------------------
// (3)

/// Plant: buff_death_spawn_dropped.
#[test]
fn a_cursed_unit_that_dies_to_anything_leaves_a_hog_and_an_uncursed_one_does_not() {
    for by_zap in [false, true] {
        let (mut s, _, knight, _) = witch_scene(shipped(), false);
        until_cursed(&mut s, knight);
        // An uncursed Red Knight, far from the Witch, killed on the same tick.
        let other = s.scenario_spawn_now(Team::Red, "Knight", at((4000, 22000)), None).expect("spawn");
        let before = hogs(&s);
        if by_zap {
            assert!(s.debug_set_hp(knight, 1) && s.debug_set_hp(other, 1));
            let p = s.entity(knight).unwrap().pos;
            s.spawn_unit(Team::Blue, "Zap", p, None).expect("cast a Zap on the cursed Knight");
            let q = s.entity(other).unwrap().pos;
            s.spawn_unit(Team::Blue, "Zap", q, None).expect("cast a Zap on the other");
        } else {
            assert!(s.debug_set_hp(knight, 0) && s.debug_set_hp(other, 0));
        }
        s.tick();
        assert!(s.entity(knight).is_none() && s.entity(other).is_none(), "the scene drifted: a Knight outlived its death (by_zap {by_zap})");
        let new: Vec<EntityId> = hogs(&s).into_iter().filter(|h| !before.contains(h)).collect();
        assert_eq!(new.len(), 1, "one hog, the cursed Knight's (by_zap {by_zap})");
    }
}

// ---------------------------------------------------------------------------
// (4)

/// The hog a standing cursed Knight leaves when set to 0: (its point and the Knight's, its track (pos, deploying) after
/// D to D + 5, its hp, its team).
fn hog_of_a_standing_victim(cfg: BattleConfig, rotated: bool) -> (Vec2, Vec2, Vec<(Vec2, bool)>, i32, Team) {
    let (mut s, _, knight, _) = witch_scene(cfg, rotated);
    until_cursed(&mut s, knight);
    let kp = s.entity(knight).unwrap().pos;
    let before = hogs(&s);
    assert!(s.debug_set_hp(knight, 0));
    s.tick(); // D
    let new: Vec<EntityId> = hogs(&s).into_iter().filter(|h| !before.contains(h)).collect();
    assert_eq!(new.len(), 1, "the scene drifted: {} hogs", new.len());
    let h = new[0];
    let (hp, team, pos) = (s.entity(h).unwrap().hp, s.entity(h).unwrap().team, s.entity(h).unwrap().pos);
    let mut track = vec![(pos, s.entity(h).unwrap().deploying)];
    for _ in 1..=5u32 {
        s.tick();
        let v = s.entity(h).expect("the scene drifted: the hog died");
        track.push((v.pos, v.deploying));
    }
    (pos, kp, track, hp, team)
}

/// Plant: buff_death_spawn_at_death_point.
#[test]
fn the_hog_is_born_1100_ahead_of_a_standing_victim() {
    let (pos, kp, track, hp, team) = hog_of_a_standing_victim(shipped(), false);
    let s = BattleState::new(0, config());
    let hog = s.cards().index("VoodooHog").expect("the hog loads as a unit");
    let r = (card_stat(&s, "Knight").collision_radius + s.cards().get(hog).collision_radius) / K;
    assert_eq!(r, 1100, "the scene drifted: the two radii are not 500 + 600");
    assert_eq!(team, Team::Blue, "the hog is the Witch's side's");
    assert_eq!(pos, Vec2::new(kp.x, kp.y - r * K), "the hog is born 1100 ahead of the Red victim, along Red's forward (-y)");
    let want_hp = s.cards().scaled(hog, s.config().card_level[0], s.cards().get(hog).hitpoints).expect("a valid level");
    if s.config().card_level[0] == 11 {
        assert_eq!(want_hp, 629, "the hog's 246 at level 11");
    }
    assert_eq!(hp, want_hp, "the hog at the Witch's level (status.BUFF_DEATH_SPAWN_LEVEL = source_level)");
    let deploying: Vec<bool> = track.iter().map(|t| t.1).collect();
    assert_eq!(deploying, vec![true, true, true, true, false, false], "the hog deploys D to D + 3 (200 ms)");
    assert!(track[..5].iter().all(|t| t.0 == pos), "the hog stands through D + 4: {track:?}");
    assert_ne!(track[5].0, pos, "the hog's first step is D + 5");
}

// ---------------------------------------------------------------------------
// (5)

/// Plant: curse_expires_before_reap.
#[test]
fn a_curse_lasts_its_5000_ms() {
    for (after, want) in [(97u32, 1usize), (100, 1), (101, 0), (210, 0)] {
        let (mut s, witch, knight, cannon) = witch_scene(shipped(), false);
        until_cursed(&mut s, knight); // H
        // The Witch and the Cannon gone, so nothing else curses or holds the Knight.
        assert!(s.debug_set_hp(witch, 0) && s.debug_set_hp(cannon, 0));
        let before = hogs(&s);
        for _ in 1..after {
            s.tick();
        }
        // A death on H + `after`: the Knight set to 0 after H + after - 1, dead in H + after's Resolve.
        if s.entity(knight).is_some() {
            assert!(s.debug_set_hp(knight, 0));
        }
        s.tick();
        let new = hogs(&s).into_iter().filter(|h| !before.contains(h)).count();
        assert_eq!(new, want, "a death on H + {after}: {new} hogs");
    }
}

// ---------------------------------------------------------------------------
// (6)

/// Plant: ignore_buff_not_read.
#[test]
fn the_golem_takes_the_hit_but_never_the_curse_and_its_golemite_does() {
    let mut s = BattleState::new(0, shipped());
    s.scenario_spawn_now(Team::Blue, "WitchMother", at((9900, 14500)), None).expect("spawn the Witch");
    // A Golem deployed from hand stands for its 3000 ms of deploy, in her reach.
    s.spawn_unit(Team::Red, "Golem", at((9900, 20000)), None).expect("deploy a Golem");
    s.tick();
    let golem = s.entities().find(|e| e.team == Team::Red && e.card == "Golem").map(|e| e.id).expect("the Golem appeared");
    let full = s.entity(golem).unwrap().hp;
    let n = run_until(&mut s, 60, |s| s.entity(golem).is_some_and(|v| v.hp < full));
    assert!(n < 60, "the scene drifted: the Witch never hit the Golem");
    assert!(!cursed(&s, golem), "the Golem took the curse: its row lists VoodooCurse (IgnoreBuff)");
    let before = hogs(&s);
    let others: Vec<EntityId> = s.entities().map(|e| e.id).collect();
    assert!(s.debug_set_hp(golem, 0));
    s.tick();
    assert_eq!(hogs(&s), before, "the Golem left a hog");
    let golemites: Vec<EntityId> = s.entities().filter(|e| e.team == Team::Red && !others.contains(&e.id)).map(|e| e.id).collect();
    assert!(!golemites.is_empty(), "the scene drifted: the Golem left no Golemites");
    // Her next target is a Golemite, whose row lists nothing: it takes the curse, and leaves a hog.
    let n = run_until(&mut s, 120, |s| golemites.iter().any(|g| cursed(s, *g)));
    assert!(n < 120, "a Golemite never took the curse");
    let g = *golemites.iter().find(|g| cursed(&s, **g)).unwrap();
    assert!(s.debug_set_hp(g, 0));
    s.tick();
    assert_eq!(hogs(&s).len(), before.len() + 1, "the cursed Golemite left a hog");
}

// ---------------------------------------------------------------------------
// (7)

/// Plant: curse_on_buildings.
#[test]
fn a_building_she_kills_leaves_no_hog() {
    let mut s = BattleState::new(0, shipped());
    s.scenario_spawn_now(Team::Blue, "WitchMother", at((9900, 14500)), None).expect("spawn the Witch");
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", at((9900, 19500)), None).expect("spawn a Red Cannon");
    let mut hit = false;
    for _ in 0..60u32 {
        let b = s.entity(cannon).map_or(0, |v| v.hp);
        s.tick();
        // her 133, well above the Cannon's lifetime drain of a tick
        if s.entity(cannon).is_some_and(|v| b - v.hp > 50) {
            hit = true;
            break;
        }
    }
    assert!(hit, "the scene drifted: the Witch never hit the Cannon");
    assert!(!cursed(&s, cannon), "the Cannon took the curse (IgnoreBuildings)");
    assert!(s.debug_set_hp(cannon, 0));
    s.tick();
    assert!(hogs(&s).is_empty(), "a building left a hog");
}

// ---------------------------------------------------------------------------
// (8)

/// Plant: buff_death_spawn_dropped.
#[test]
fn the_carriers_own_death_spawn_still_happens() {
    let mut s = BattleState::new(0, shipped());
    s.scenario_spawn_now(Team::Blue, "WitchMother", at((9900, 14500)), None).expect("spawn the Witch");
    let gs = s.scenario_spawn_now(Team::Red, "GiantSkeleton", at((9900, 20000)), None).expect("spawn a Giant Skeleton");
    let ds = card_stat(&s, "GiantSkeleton").death_spawn.expect("the scene drifted: the Giant Skeleton has no death spawn");
    let own = s.cards().get(ds.unit).name.clone();
    until_cursed(&mut s, gs);
    let before = hogs(&s);
    assert!(s.debug_set_hp(gs, 0));
    s.tick();
    let bombs = s.spells().iter().filter(|sp| s.cards().get(sp.card).name == own).count();
    let units = s.entities().filter(|e| e.card == own).count();
    assert!(bombs + units > 0, "the Giant Skeleton's own death spawn ({own}) did not happen");
    assert_eq!(hogs(&s).len(), before.len() + 1, "the cursed Giant Skeleton left a hog beside its own death spawn");
}

// ---------------------------------------------------------------------------
// (9)

/// Plant: hash_skips_buff_source.
#[test]
fn the_red_scene_is_the_rotation_and_a_curse_survives_a_save() {
    // The rotation: the Red Witch's hog from a standing Blue victim, under the seat-symmetric config.
    let s = BattleState::new(0, symmetric_config());
    let (bpos, bk, btrack, bhp, bteam) = hog_of_a_standing_victim(symmetric_config(), false);
    let (rpos, rk, rtrack, rhp, rteam) = hog_of_a_standing_victim(symmetric_config(), true);
    assert_eq!((bteam, rteam), (Team::Blue, Team::Red));
    assert_eq!((mirror(&s, bk), mirror(&s, bpos), bhp), (rk, rpos, rhp), "the Red hog is not the rotation of the Blue one");
    assert_eq!(rpos.y - rk.y, bk.y - bpos.y, "the Blue victim's hog is ahead of it along Blue's forward (+y)");
    for (b, r) in btrack.iter().zip(&rtrack) {
        assert_eq!((mirror(&s, b.0), b.1), *r, "the hog's track is not the rotation");
    }
    // A shot's before-damage flag is state.
    let (mut s, _, _, _) = witch_scene(shipped(), false);
    let b = voodoo(&s);
    let n = run_until(&mut s, 60, |s| s.projectiles().iter().any(|p| p.buff.is_some_and(|x| x.buff == b)));
    assert!(n < 60, "the scene drifted: the Witch never fired");
    let hashed = edit_is_hashed(&s, |v| {
        let shots = v["projectiles"].as_array_mut().expect("the snapshot lists its projectiles");
        let shot = shots.iter_mut().find(|p| p["buff_first"].as_bool() == Some(true)).expect("the Witch's shot is saved");
        shot["buff_first"] = serde_json::Value::from(false);
    });
    assert!(hashed, "a save edited only in a shot's before-damage flag loads under the old hash: the flag is not hashed");
    // A battle saved with the curse live resumes it: the cursed Knight's death leaves a hog in both.
    let (mut s, _, knight, _) = witch_scene(shipped(), false);
    until_cursed(&mut s, knight);
    let mut resumed = BattleState::load(&s.save()).expect("the save loads");
    for b in [&mut s, &mut resumed] {
        assert!(b.debug_set_hp(knight, 0));
        b.tick();
    }
    assert_eq!(resumed.state_hash(), s.state_hash(), "the resumed battle parts from the one that never saved");
    assert_eq!(hogs(&resumed).len(), 1, "the resumed curse left its hog");
}
