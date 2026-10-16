//! THE BATTLE HEALER'S HEALS (card.rs `CardDef::spawn_area_effect`, `CardDef::hit_area`; spell.rs step_spells, part HIT_AREA;
//! state.rs `hit_areas`), at level 11.
//!
//! THE MEASUREMENTS (client 16.402, parity's r74 batch-7 census of the ob13 set): her hits heal her own troops in reach, air
//! and ground, to 3000 + their radius, four pulses on the hit + 6, 11, 16, 21 (78 of 78; the engine's 0); her spawn heal pulses
//! on her first frame + 6, 11, 16, 21 (10 of 19 full trains; the engine's + 4, 9, 14, 19).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! battle_healer`): hit_area_never -> `under_client16402_cast_at_attacker_her_hits_heal_her_troops_in_reach_air_and_ground` red;
//! spawn_pulse_15535 -> `under_client16402_two_ticks_later_her_spawn_heal_pulses_on_her_first_frame_plus_6` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AreaEffectOnHit, BattleState, SpawnAreaFirstPulse};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const AT: (i32, i32) = (9000, 9000);

/// Each tick's hp gain of `ids` over `ticks` ticks, all held on their points (`hold`); the first tick run is index 0.
fn gains(s: &mut BattleState, hold: &[(EntityId, Vec2)], ids: &[EntityId], ticks: u32) -> Vec<Vec<i32>> {
    let mut last: Vec<i32> = ids.iter().map(|id| s.entity(*id).expect("a friend").hp).collect();
    let mut out = vec![Vec::new(); ids.len()];
    for _ in 0..ticks {
        for (id, p) in hold {
            assert!(s.debug_set_pos(*id, *p));
        }
        s.tick();
        for (k, id) in ids.iter().enumerate() {
            let hp = s.entity(*id).expect("a friend").hp;
            out[k].push(hp - last[k]);
            last[k] = hp;
        }
    }
    out
}

/// The ticks (index) on which a gain series rose.
fn rises(g: &[i32]) -> Vec<(usize, i32)> {
    g.iter().enumerate().filter(|(_, v)| **v > 0).map(|(k, v)| (k, *v)).collect()
}

/// Her spawn heal under spells.SPAWN_AREA_FIRST_PULSE = `arm`: a Blue Knight at 1000 hp 1500 east of her point, no enemy near;
/// the Knight's rises from her first frame (index 0).
fn spawn_train(arm: SpawnAreaFirstPulse) -> Vec<(usize, i32)> {
    let mut cfg = config();
    cfg.calib.spawn_area_first_pulse = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(3, cfg);
    let kp = n(AT.0 + 1500, AT.1);
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", kp, Some(1000)).expect("the Knight");
    let bh = s.scenario_spawn_now(Team::Blue, "BattleHealer", n(AT.0, AT.1), None).expect("the Battle Healer");
    let g = gains(&mut s, &[(knight, kp), (bh, n(AT.0, AT.1))], &[knight], 30);
    rises(&g[0])
}

/// spells.SPAWN_AREA_FIRST_PULSE = client16402_two_ticks_later (client 16.402, parity's r74 batch-7 census: her spawn heal on her
/// first frame + 6, 11, 16, 21; the engine's + 4, 9, 14, 19): four pulses two ticks later than frequency_less_tick's, the same
/// amounts; under frequency_less_tick (the vacuity check) the 15.535.29 train. Plant: spawn_pulse_15535.
#[test]
fn under_client16402_two_ticks_later_her_spawn_heal_pulses_on_her_first_frame_plus_6() {
    let old = spawn_train(SpawnAreaFirstPulse::FrequencyLessTick);
    let new = spawn_train(SpawnAreaFirstPulse::Client16402TwoTicksLater);
    assert_eq!(old.len(), 4, "frequency_less_tick: four pulses: {old:?}");
    assert_eq!(new.len(), 4, "client16402_two_ticks_later: four pulses: {new:?}");
    assert!(old.iter().zip(&new).all(|(a, b)| b.0 == a.0 + 2 && b.1 == a.1), "the train two ticks later: {old:?} -> {new:?}");
    assert_eq!(new.iter().map(|r| r.0).collect::<Vec<_>>(), vec![6, 11, 16, 21], "client16402_two_ticks_later: from her first frame: {new:?}");
}

/// Her hits' heal under combat.AREA_EFFECT_ON_HIT = `arm`: her spawn heal over (40 ticks), a Red Golem held before her (it
/// targets buildings: she hits it, it hits nobody), a Blue Knight 1500 east and a Blue Minion 1500 south (air) at a third of
/// their hp, a Blue Knight 3800 west (out of reach): from her first hit on the Golem h, each friend's rises over h + 1 .. h + 25
/// (index 0 = h + 1).
fn hit_trains(arm: AreaEffectOnHit) -> Vec<Vec<(usize, i32)>> {
    let mut cfg = config();
    cfg.calib.area_effect_on_hit = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(3, cfg);
    let (bp, gp, kp, mp, fp) = (n(AT.0, AT.1), n(AT.0, AT.1 + 1600), n(AT.0 + 1500, AT.1), n(AT.0, AT.1 - 1500), n(AT.0 - 3800, AT.1));
    let bh = s.scenario_spawn_now(Team::Blue, "BattleHealer", bp, None).expect("the Battle Healer");
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", kp, None).expect("the Knight");
    let minion = s.scenario_spawn_now(Team::Blue, "Minions", mp, None).expect("the Minion");
    let far = s.scenario_spawn_now(Team::Blue, "Knight", fp, None).expect("the far Knight");
    let hold = [(bh, bp), (knight, kp), (minion, mp), (far, fp)];
    for _ in 0..40 {
        for (id, p) in &hold {
            assert!(s.debug_set_pos(*id, *p));
        }
        s.tick();
    }
    for id in [knight, minion, far] {
        let top = s.entity(id).expect("a friend").max_hp;
        assert!(s.debug_set_hp(id, top / 3));
    }
    let golem = s.scenario_spawn_now(Team::Red, "Golem", gp, None).expect("the Golem");
    let mut hold_g = hold.to_vec();
    hold_g.push((golem, gp));
    let mut prev = s.entity(golem).expect("the Golem").hp;
    for _ in 0..200 {
        for (id, p) in &hold_g {
            assert!(s.debug_set_pos(*id, *p));
        }
        s.tick();
        let hp = s.entity(golem).expect("the Golem").hp;
        if hp < prev {
            let g = gains(&mut s, &hold_g, &[knight, minion, far], 25);
            return g.iter().map(|v| rises(v)).collect();
        }
        prev = hp;
    }
    panic!("the scene drifted: she never hit the Golem");
}

/// combat.AREA_EFFECT_ON_HIT = client16402_cast_at_attacker (client 16.402, parity's r74 batch-7 census: 78 of 78 friends in reach
/// healed, air 19 of 19, on the hit + 6, 11, 16, 21; out of reach 0 of 17; the engine's 0): after her hit the Knight and the
/// Minion each take her heal's four pulses (25 at level 11) on the hit + 6, 11, 16, 21, the far Knight none; under not_read (the
/// vacuity check) nobody rises. Plant: hit_area_never.
#[test]
fn under_client16402_cast_at_attacker_her_hits_heal_her_troops_in_reach_air_and_ground() {
    let p = 25;
    let train = vec![(5, p), (10, p), (15, p), (20, p)];
    let t = hit_trains(AreaEffectOnHit::Client16402CastAtAttacker);
    assert_eq!(t[0], train, "client16402_cast_at_attacker: the Knight (ground, 1500 off) on the hit + 6, 11, 16, 21: {t:?}");
    assert_eq!(t[1], train, "client16402_cast_at_attacker: the Minion (air, 1500 off): {t:?}");
    assert!(t[2].is_empty(), "client16402_cast_at_attacker: the far Knight (3800 off) healed: {t:?}");
    let o = hit_trains(AreaEffectOnHit::NotRead);
    assert!(o.iter().all(|v| v.is_empty()), "not_read (the vacuity check): a friend rose after her hit: {o:?}");
}
