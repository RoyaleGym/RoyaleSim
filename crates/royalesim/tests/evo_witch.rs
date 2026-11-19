//! THE EVO WITCH (tools/extract_cards.py `soul_drain_block`; card.rs `SoulDrainDef`; state.rs EvoBoard `souls`,
//! `soul_heals`), at level 11 (839 max hitpoints).
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-Witch-evo-s0): four of her Skeletons died on t880, t900, t911 and t922
//! and she healed 153 on t901, t921, t932 and t943, to 992, 1145, 1298 and 1451 (173 % of 839, floored). Oracle's
//! sp-f2-witch-s0: her waves of 4 on her creation + 38 (her row's), + 179, + 319 and + 459.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_witch`):
//!   - soul_drain_never -> `each_skeleton_she_spawned_that_dies_heals_her_153_on_its_death_plus_21` red;
//!   - overheal_capped_at_max -> the same red;
//!   - witch_waves_never -> `her_interval_waves_come_on_her_creation_plus_179_then_every_140` red;
//!   - interval_units_send_souls -> `a_skeleton_of_her_interval_waves_sends_no_soul` red;
//!   - witch_wave_clock_unbuffed -> `under_client16635_buffed_a_freeze_on_her_puts_her_next_interval_wave_back` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, EvoWitchWaveSpawnSpeed, RingCreationOrder};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn hers(s: &BattleState, w: EntityId) -> Vec<EntityId> {
    s.entities().filter(|e| e.team == Team::Blue && e.spawned_by == Some(w)).map(|e| e.id).collect()
}

/// Kill `sk` and tick: the Witch's hitpoints on the death's tick and on each of the 25 after it (her point held, and
/// each of `hold`'s living units on its own: her Skeletons would walk into the red tower's reach).
fn kill(s: &mut BattleState, w: EntityId, at: Vec2, sk: EntityId, hold: &[(EntityId, Vec2)]) -> Vec<i32> {
    assert!(s.debug_set_hp(sk, 0), "{sk:?} is alive to kill");
    let mut hp = Vec::new();
    for _ in 0..=25 {
        assert!(s.debug_set_pos(w, at));
        for (id, p) in hold {
            let _ = s.debug_set_pos(*id, *p);
        }
        s.tick();
        hp.push(s.entity(w).expect("the Witch").hp);
    }
    hp
}

#[test]
fn each_skeleton_she_spawned_that_dies_heals_her_153_on_its_death_plus_21() {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Witch".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at = n(9000, 11500);
    s.spawn_unit(Team::Blue, "Witch_EV1", at, None).expect("the Witch");
    s.tick();
    let w = find_live(&s, Team::Blue, "Witch_EV1").first().expect("the Witch").id;
    let top = s.entity(w).expect("the Witch").max_hp;
    assert_eq!(top, 839, "the form's 839 at level 11");
    for _ in 0..200 {
        if hers(&s, w).len() >= 4 {
            break;
        }
        assert!(s.debug_set_pos(w, at));
        s.tick();
    }
    let first = hers(&s, w);
    assert_eq!(first.len(), 4, "her first wave");
    let hold: Vec<(EntityId, Vec2)> = first.iter().map(|id| (*id, s.entity(*id).expect("her Skeleton").pos)).collect();
    // A Skeleton she did not spawn heals her nothing.
    let other = s.scenario_spawn_now(Team::Blue, "Skeleton", n(12000, 11500), None).expect("a Skeleton");
    let hp = kill(&mut s, w, at, other, &hold);
    assert!(hp.iter().all(|h| *h == top), "a stranger's death healed her: {hp:?}");
    // Hers, one at a time: 153 on the death + 21, past her maximum, to 173 % of it.
    for (k, want) in [992, 1145, 1298, 1451].into_iter().enumerate() {
        let before = s.entity(w).expect("the Witch").hp;
        let hp = kill(&mut s, w, at, first[k], &hold);
        assert!(hp[..21].iter().all(|h| *h == before), "Skeleton {k}: moved before the death + 21: {hp:?}");
        assert_eq!(hp[21], want, "Skeleton {k}: the heal on the death + 21: {hp:?}");
    }
    // At the cap a fifth (her second wave's) heals nothing more.
    for _ in 0..200 {
        if !hers(&s, w).is_empty() {
            break;
        }
        assert!(s.debug_set_pos(w, at));
        s.tick();
    }
    let next = *hers(&s, w).first().expect("her second wave");
    let hp = kill(&mut s, w, at, next, &[]);
    assert!(hp.iter().all(|h| *h == 1451), "past the cap: {hp:?}");
}

#[test]
fn her_interval_waves_come_on_her_creation_plus_179_then_every_140() {
    // Nothing red on the field: the frames on which a new unit of hers first stands, counted from her creation's.
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Witch".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at = n(2500, 2500);
    s.spawn_unit(Team::Blue, "Witch_EV1", at, None).expect("the Witch");
    s.tick();
    let w = find_live(&s, Team::Blue, "Witch_EV1").first().expect("the Witch").id;
    let mut seen: Vec<EntityId> = Vec::new();
    let mut arrivals: Vec<(usize, usize)> = Vec::new();
    for k in 1..=480 {
        assert!(s.debug_set_pos(w, at));
        s.tick();
        let fresh: Vec<EntityId> = hers(&s, w).into_iter().filter(|id| !seen.contains(id)).collect();
        if !fresh.is_empty() {
            arrivals.push((k, fresh.len()));
            seen.extend(fresh);
        }
    }
    // Her row's wave first (its tick is the base Witch's spawner's), then the interval's on + 179, + 319, + 459.
    let later: Vec<(usize, usize)> = arrivals.iter().copied().skip(1).collect();
    assert_eq!(later, vec![(179, 4), (319, 4), (459, 4)], "her waves: {arrivals:?}");
}

/// ONLY HER ROW'S WAVE SENDS SOULS (state.rs, her soul in Reap): a Skeleton of her interval waves dying heals her nothing,
/// below her cap; one of her row's wave, after it, heals her 153 on its death + 21. Measured on client 15.535.29: row-wave
/// deaths healed her 8 of 8, interval-wave deaths 0 of 8 (sp-f2-witch-s0's second and third waves).
#[test]
fn a_skeleton_of_her_interval_waves_sends_no_soul() {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Witch".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at = n(9000, 11500);
    s.spawn_unit(Team::Blue, "Witch_EV1", at, None).expect("the Witch");
    s.tick();
    let w = find_live(&s, Team::Blue, "Witch_EV1").first().expect("the Witch").id;
    let top = s.entity(w).expect("the Witch").max_hp;
    let mut hold: Vec<(EntityId, Vec2)> = Vec::new();
    let mut row: Vec<EntityId> = Vec::new();
    for _ in 0..260 {
        let mine = hers(&s, w);
        if row.is_empty() && mine.len() >= 4 {
            row = mine.clone();
        }
        if !row.is_empty() && mine.len() >= 8 {
            break;
        }
        for id in &mine {
            if !hold.iter().any(|(h, _)| h == id) {
                hold.push((*id, s.entity(*id).expect("hers").pos));
            }
        }
        assert!(s.debug_set_pos(w, at));
        for (id, p) in &hold {
            let _ = s.debug_set_pos(*id, *p);
        }
        s.tick();
    }
    let interval: Vec<EntityId> = hers(&s, w).into_iter().filter(|id| !row.contains(id)).collect();
    assert_eq!((row.len(), interval.len()), (4, 4), "the scene drifted: her row's wave and her first interval wave");
    for id in &interval {
        if !hold.iter().any(|(h, _)| h == id) {
            hold.push((*id, s.entity(*id).expect("hers").pos));
        }
    }
    assert_eq!(s.entity(w).expect("the Witch").hp, top, "the scene drifted: she is not at her maximum");
    let hp = kill(&mut s, w, at, interval[0], &hold);
    assert!(hp.iter().all(|h| *h == top), "an interval wave's Skeleton healed her: {hp:?}");
    let hp = kill(&mut s, w, at, row[0], &hold);
    assert_eq!(hp[21], top + 153, "the control: her row's Skeleton heals her 153 on its death + 21: {hp:?}");
}

/// Her first interval wave (her creation + 179) under spawner.RING_CREATION_ORDER = `arm`: each Skeleton's direction from
/// her, in creation (index) order.
fn interval_wave_order(arm: RingCreationOrder) -> Vec<(i32, i32)> {
    let mut cfg: BattleConfig = config();
    cfg.calib.ring_creation_order = arm;
    cfg.decks = [vec!["Witch".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at = n(2500, 2500);
    s.spawn_unit(Team::Blue, "Witch_EV1", at, None).expect("the Witch");
    s.tick();
    let w = find_live(&s, Team::Blue, "Witch_EV1").first().expect("the Witch").id;
    let mut seen: Vec<EntityId> = hers(&s, w);
    for k in 1..=179 {
        assert!(s.debug_set_pos(w, at));
        s.tick();
        let mut fresh: Vec<EntityId> = hers(&s, w).into_iter().filter(|id| !seen.contains(id)).collect();
        if k == 179 {
            assert_eq!(fresh.len(), 4, "precondition: her first interval wave on + 179");
            fresh.sort_by_key(|id| id.index);
            let p = s.entity(w).expect("the Witch").pos;
            return fresh
                .iter()
                .map(|id| {
                    let q = s.entity(*id).expect("a Skeleton").pos;
                    let (dx, dy) = (q.x - p.x, q.y - p.y);
                    if dx.abs() > dy.abs() {
                        (dx.signum(), 0)
                    } else {
                        (0, dy.signum())
                    }
                })
                .collect();
        }
        seen.extend(fresh);
    }
    unreachable!()
}

/// spawner.RING_CREATION_ORDER = client_descending_angle reaches her interval waves (state.rs `witch_wave_pass`): south,
/// west, north, east, as her row's wave (client 16.402, parity's r66 census of the ob5 live set: 1253 of 1253 Witch
/// waves, 415 of them Evo interval waves); under ascending_angle east, north, west, south (the vacuity check). Plant:
/// ring_created_ascending.
#[test]
fn her_interval_waves_are_created_south_west_north_east_under_client_descending_angle() {
    assert_eq!(interval_wave_order(RingCreationOrder::ClientDescendingAngle), vec![(0, -1), (-1, 0), (0, 1), (1, 0)], "client_descending_angle");
    assert_eq!(interval_wave_order(RingCreationOrder::AscendingAngle), vec![(1, 0), (0, 1), (-1, 0), (0, -1)], "ascending_angle");
}

/// `her_interval_waves_come_on_her_creation_plus_179_then_every_140`'s scene under spawner.EVO_WITCH_WAVE_SPAWN_SPEED = `arm`,
/// a red Freeze cast on her point on frame `freeze` when Some: the frames her waves after her row's first stand on.
fn wave_frames(arm: EvoWitchWaveSpawnSpeed, freeze: Option<usize>) -> Vec<(usize, usize)> {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Witch".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.evo_witch_wave_spawn_speed = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at = n(2500, 2500);
    s.spawn_unit(Team::Blue, "Witch_EV1", at, None).expect("the Witch");
    s.tick();
    let w = find_live(&s, Team::Blue, "Witch_EV1").first().expect("the Witch").id;
    let mut seen: Vec<EntityId> = Vec::new();
    let mut arrivals: Vec<(usize, usize)> = Vec::new();
    for k in 1..=480 {
        if freeze == Some(k) {
            s.spawn_unit(Team::Red, "Freeze", at, None).expect("the Freeze");
        }
        if s.entity(w).is_some() {
            let hw = s.entity(w).expect("her").max_hp;
            assert!(s.debug_set_pos(w, at) && s.debug_set_hp(w, hw));
        }
        s.tick();
        let fresh: Vec<EntityId> = hers(&s, w).into_iter().filter(|id| !seen.contains(id)).collect();
        if !fresh.is_empty() {
            arrivals.push((k, fresh.len()));
            seen.extend(fresh);
        }
    }
    arrivals.into_iter().skip(1).collect()
}

/// spawner.EVO_WITCH_WAVE_SPAWN_SPEED = client16635_buffed (client 16.635, parity r96, the oracle's il-mg-16635 set: 22 of 22
/// interval gaps with a spawn-speed change on her clock moved that way, 89 of 90 without one were 140 ticks): unbuffed her
/// interval waves come on + 179, + 319 and + 459 as under tick_ms; a Freeze on her from + 200 puts the + 319 wave back,
/// where tick_ms (the vacuity check) keeps it. Plant: witch_wave_clock_unbuffed.
#[test]
fn under_client16635_buffed_a_freeze_on_her_puts_her_next_interval_wave_back() {
    use EvoWitchWaveSpawnSpeed as E;
    assert_eq!(wave_frames(E::Client16635Buffed, None), vec![(179, 4), (319, 4), (459, 4)], "client16635_buffed unbuffed: the seed");
    let old = wave_frames(E::TickMs, Some(200));
    assert!(old.len() >= 2 && old[0].0 == 179 && old[1].0 == 319, "tick_ms (the vacuity check): her waves {old:?}");
    let new = wave_frames(E::Client16635Buffed, Some(200));
    assert!(new.len() >= 2 && new[0].0 == 179 && new[1].0 > 319, "client16635_buffed: her waves {new:?}");
}

/// Since parity r96 for clients 16.402 and 16.635 (tick_ms before it; client 15.535.29 keeps tick_ms).
#[test]
fn the_shipped_arm_is_client16635_buffed() {
    assert_eq!(
        royalesim::state::Calib::shipped().evo_witch_wave_spawn_speed,
        royalesim::state::EvoWitchWaveSpawnSpeed::Client16635Buffed
    );
    assert_eq!(config().calib.evo_witch_wave_spawn_speed, royalesim::state::EvoWitchWaveSpawnSpeed::Client16635Buffed);
}
