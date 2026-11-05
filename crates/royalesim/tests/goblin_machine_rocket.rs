//! combat.GOBLIN_MACHINE_ROCKET -- the Goblin Machine's OnStartingAction goblin_machine_rocket, an ActionTargetIndicatorAttack
//! (state.rs `rocket_pass`). Its clock loads from the tick after the machine's 1000 ms deploy; with no rocket up and its
//! cooldown spent it marks the closest enemy between MinimumRange 2500 and Range 5000 (plus both radii) and fires 1000 ms
//! later at the mark: 119 on the Common ladder (304 at level 11), radius 1500, one rocket every 100 ticks.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, GoblinMachineRocket};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The machine (Blue, level 11) on (3500, 9500), a Red Knight 2,000 ahead (inside the rocket's minimum range: the machine's
/// melee target) and a Red Golem 4,500 ahead (inside its range), each held on its point every tick (the machine and the
/// Knight kept at full hp; Blue's princess towers down, so nothing else reaches the Golem): the ticks, from the machine's
/// first tick on the board, on which the Golem lost exactly one level-11 rocket (304), over 240 ticks.
fn golem_rockets(arm: GoblinMachineRocket) -> Vec<u32> {
    let mut cfg: BattleConfig = config();
    cfg.calib.goblin_machine_rocket = arm;
    cfg.decks = [vec!["GoblinMachine".into(), "Knight".into()], vec!["Golem".into(), "Knight".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    let (m_at, k_at, g_at) = (n(3500, 9500), n(3500, 11500), n(3500, 14000));
    let golem = s.scenario_spawn_now(Team::Red, "Golem", g_at, None).expect("a red Golem");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).expect("a red Knight");
    s.spawn_unit(Team::Blue, "GoblinMachine", m_at, None).expect("the Goblin Machine");
    let (mut machine, mut out) = (None, Vec::new());
    for t in 0..260u32 {
        if machine.is_none() {
            machine = find_live(&s, Team::Blue, "GoblinMachine").first().map(|e| (e.id, t));
        }
        if let Some((m, _)) = machine {
            assert!(s.debug_set_pos(m, m_at));
            let full = s.entity(m).expect("the machine").max_hp;
            assert!(s.debug_set_hp(m, full));
        }
        assert!(s.debug_set_pos(knight, k_at) && s.debug_set_pos(golem, g_at));
        let full = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_hp(knight, full));
        let before = s.entity(golem).expect("the Golem").hp;
        s.tick();
        let after = s.entity(golem).expect("the Golem").hp;
        if let Some((_, c)) = machine {
            if before - after == 304 {
                out.push(t - c);
            }
            if t - c >= 240 {
                break;
            }
        }
    }
    assert!(machine.is_some(), "the scene drifted: no Goblin Machine on the board");
    out
}

/// combat.GOBLIN_MACHINE_ROCKET = client16402_indicator_attack (client 16.402, parity r86 agents26, live14: 34 of 35 matched
/// rockets landed on the predicted tick: the first search on the machine's first frame + 50, the fire 20 ticks later, one
/// rocket every 100 ticks while a target is in reach): the Golem 4,500 ahead takes a rocket about C + 87 (the fire on C + 70,
/// from 1200 behind the machine: 5,700 at the rocket's speed) and the next 100 ticks later; under unbuilt no rocket lands.
/// Plant: goblin_rocket_unbuilt.
#[test]
fn under_client16402_indicator_attack_the_machine_rockets_the_golem_every_100_ticks() {
    let new = golem_rockets(GoblinMachineRocket::Client16402IndicatorAttack);
    assert!(new.len() >= 2, "client16402_indicator_attack: rockets on the Golem {new:?}");
    assert!((84..=90).contains(&new[0]), "client16402_indicator_attack: the first rocket landed on C + {} (expected about C + 87): {new:?}", new[0]);
    assert_eq!(new[1] - new[0], 100, "client16402_indicator_attack: rockets 100 ticks apart: {new:?}");
    // NOT VACUOUS: unbuilt fires nothing
    let old = golem_rockets(GoblinMachineRocket::Unbuilt);
    assert!(old.is_empty(), "unbuilt: a rocket's 304 on the Golem: {old:?}");
}
