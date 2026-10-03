//! combat.DIRECT_HIT_BUFF_COUNTDOWN: how long the buff an instant hit lands holds its victim (combat.rs `direct_buff`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, stun_freeze_census.py: a crown tower's attack counter stands
//! 9 ticks after an Electro Wizard's zap (17 of 17), 10 after the Electro Dragon's projectile.
//!
//! THE SCENE: a Blue Electro Wizard in reach of Red's left princess tower; the tower's attack counter (attack_ms) after
//! the zap's first landing tick (the tower's hp falls).
//!
//! WHAT IS PINNED, and the plant that turns it red (direct_hit_buff_full_time):
//!   1. resolve_landing (the engine's, the vacuity check): the counter stands 10 ticks; client15535_landing_tick: 9.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DirectHitBuffCountdown};
use royalesim::Team;

/// The ticks the tower's counter stands after the zap's first landing tick.
fn frozen(arm: DirectHitBuffCountdown) -> u32 {
    let mut cfg = config();
    cfg.calib.direct_hit_buff_countdown = arm;
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    let tower = s.entities().find(|e| e.team == Team::Red && e.card == "PrincessTower" && e.pos.x < 9000 * K).expect("Red's left princess tower").id;
    let p = s.entity(tower).unwrap().pos;
    s.scenario_spawn_now(Team::Blue, "ElectroWizard", Vec2::new(p.x, p.y - 5500 * K), None).expect("the Electro Wizard");
    let mut hp = s.entity(tower).unwrap().hp;
    for _ in 0..200 {
        s.tick();
        let t = s.entity(tower).expect("the tower stands");
        if t.hp < hp {
            let at = t.attack_ms;
            let mut n = 0;
            loop {
                s.tick();
                if s.entity(tower).expect("the tower stands").attack_ms != at || n > 60 {
                    return n;
                }
                n += 1;
            }
        }
        hp = t.hp;
    }
    panic!("the scene drifted: the Electro Wizard never hit the tower");
}

/// Plant: direct_hit_buff_full_time.
#[test]
fn an_instant_hits_stun_holds_a_tick_fewer_under_client15535_landing_tick() {
    // NOT VACUOUS: the engine's arm holds the tower 10 ticks, the projectile's measured hold.
    assert_eq!(frozen(DirectHitBuffCountdown::ResolveLanding), 10, "resolve_landing");
    assert_eq!(frozen(DirectHitBuffCountdown::Client15535LandingTick), 9, "client15535_landing_tick");
}

#[test]
fn the_shipped_value_is_resolve_landing() {
    assert_eq!(Calib::shipped().direct_hit_buff_countdown, DirectHitBuffCountdown::ResolveLanding);
}
