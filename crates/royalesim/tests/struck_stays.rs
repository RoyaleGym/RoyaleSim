//! movement.DOOMED_OWN_UPDATE = client15535_struck_stays: a troop struck down in the tick's sequential pass takes no update
//! of its own when a troop's strike felled it (state.rs `phase_path16402_for`, `struck_stays`; Scratch::strike_lethal_by_troop).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, a death-spawner felled by a troop's melee strike laid its
//! children on its last point 9 of 9 whatever the striker's age (sp-f2-cagefb-s0 t928: a Golem felled by a newer Goblin
//! Brawler, its Golemites on its last point where the engine stepped it 54 first).
//!
//! THE SCENE (match.TICK_ORDER = client_sequential_strike, movement.DYING_UNIT_VISIBILITY = client_doomed_static): Blue's
//! Golem set down first at (9000, 9000), walking north; a red Knight set down after it at (9000, 10600), in its path. Once
//! the Knight holds the Golem in its attack, `wait` ticks on, the Golem is set to 1 hp: the Knight's next blow fells it.
//! On the tick it is gone, its Golemites' centroid against its last point.
//!
//! The same scene with a red Valkyrie in the Knight's place: a SPLASH strike (her row sets AreaDamageRadius), by a
//! striker created after the Golem. Oracle's sp-splashkill-Valkyrie-ElixirGolem-v0/v1-s0: a walking Elixir Golem felled by
//! a newer Valkyrie moved on its death tick, where the Mini PEKKA control's Golem stayed.
//!
//! WHAT IS PINNED, and the plants that turn it red:
//!   1. client15535_struck_stays, the Knight: the centroid is the Golem's last point, at every `wait` tried
//!      (struck_troop_walks);
//!   2. client15535_kamikaze_stays (the arm before it, the vacuity check): at some `wait` the doomed Golem moves first;
//!   3. client15535_struck_stays, the Valkyrie: at some `wait` the Golem moves first, walking in its own turn
//!      (struck_splash_stays).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DoomedOwnUpdate, DyingUnitVisibility, TickOrder};
use royalesim::Team;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`, the Golem felled `wait` ticks after the Knight first holds it in its attack: (its last point, its
/// Golemites' centroid), native units.
fn golem_death(arm: DoomedOwnUpdate, striker: &str, wait: u32) -> ((i32, i32), (i32, i32)) {
    let mut cfg = config();
    let deck: Vec<String> = ["Golem", "Knight", "Valkyrie", "Archers", "Musketeer", "Fireball", "Arrows", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dying_unit_visibility = DyingUnitVisibility::ClientDoomedStatic;
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    cfg.calib.doomed_own_update = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let golem = s.scenario_spawn_now(Team::Blue, "Golem", at(9000, 9000), None).expect("the Golem");
    let knight = s.scenario_spawn_now(Team::Red, striker, at(9000, 10600), None).expect("the striker");
    let mut held_since = None;
    for k in 0..400u32 {
        let Some(g) = s.entity(golem) else { panic!("{arm:?}: the scene drifted: the Golem died early") };
        let last = (g.pos.x / K, g.pos.y / K);
        if let Some(kn) = s.entity(knight) {
            if held_since.is_none() && kn.target == Some(golem) && kn.attack_phase != AttackPhase::Idle {
                held_since = Some(k);
            }
        }
        if held_since.is_some_and(|h| k >= h + wait) {
            assert!(s.debug_set_hp(golem, 1));
        }
        s.tick();
        if s.entity(golem).is_none() {
            let born: Vec<(i32, i32)> = s.entities().filter(|e| e.team == Team::Blue && e.card == "Golemite").map(|e| (e.pos.x / K, e.pos.y / K)).collect();
            assert_eq!(born.len(), 2, "{arm:?}: the scene drifted: the Golem left {} Golemites", born.len());
            return (last, ((born[0].0 + born[1].0) / 2, (born[0].1 + born[1].1) / 2));
        }
    }
    panic!("{arm:?}: the scene drifted: the Knight never felled the Golem");
}

const WAITS: [u32; 6] = [0, 3, 7, 11, 15, 19];

/// Plant: struck_troop_walks.
#[test]
fn a_troop_a_troop_strikes_down_stays_on_its_last_point_under_client15535_struck_stays() {
    for w in WAITS {
        let (last, centre) = golem_death(DoomedOwnUpdate::Client15535StruckStays, "Knight", w);
        assert!((centre.0 - last.0).abs() <= 1 && (centre.1 - last.1).abs() <= 1, "wait {w}: the Golemites' centre {centre:?}, the Golem's last point {last:?}");
    }
}

#[test]
fn the_arm_before_it_moves_the_struck_golem_first() {
    let moved = WAITS.iter().any(|&w| {
        let (last, centre) = golem_death(DoomedOwnUpdate::Client15535KamikazeStays, "Knight", w);
        (centre.0 - last.0).abs() > 1 || (centre.1 - last.1).abs() > 1
    });
    assert!(moved, "kamikaze_stays: the struck Golem never moved on its death tick (vacuous otherwise)");
}

/// Plant: struck_splash_stays.
#[test]
fn a_troop_a_newer_splash_strikes_down_walks_under_client15535_struck_stays() {
    let moved = WAITS.iter().any(|&w| {
        let (last, centre) = golem_death(DoomedOwnUpdate::Client15535StruckStays, "Valkyrie", w);
        (centre.0 - last.0).abs() > 1 || (centre.1 - last.1).abs() > 1
    });
    assert!(moved, "struck_stays: a Golem felled by a newer Valkyrie's splash stayed at every wait");
}
