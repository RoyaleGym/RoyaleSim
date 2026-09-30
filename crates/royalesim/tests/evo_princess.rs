//! THE EVO PRINCESS (tools/extract_cards.py `freeze_volley_block`; card.rs `FreezeVolleyDef`; state.rs `evo_after_fire`,
//! EvoBoard `volleys`; combat.rs `step_projectiles`, part EVO_FREEZE_AREA), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; sp-form-Princess-evo-s0, Oracle's sp-f3-prinwalk2-s0 and sp-f3-prin-*): her 1st
//! and 3rd volleys froze (a 30 % slow, an area about 5500 ms where the arrow landed), the 2nd and 4th did not; her death
//! blow took 169-170 off a Cannon 1521 to 3515 from her centre 2 ticks after it, and nothing off one 4510 away.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_princess`):
//!   - freeze_volley_never -> `her_first_and_third_volleys_leave_a_freezing_area_the_second_does_not` red;
//!   - freeze_area_dropped -> `her_first_and_third_volleys_leave_a_freezing_area_the_second_does_not` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Blue's princess towers down (no crown tower reaches the scene) and the form put down on `at`: the battle and her.
fn battle(at: (i32, i32)) -> (BattleState, EntityId) {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Princess".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.spawn_unit(Team::Blue, "Princess_EV1", n(at.0, at.1), None).expect("the Princess");
    s.tick();
    let p = find_live(&s, Team::Blue, "Princess_EV1").first().expect("the Princess").id;
    (s, p)
}

fn freeze_areas(s: &BattleState) -> usize {
    s.spells().iter().filter(|sp| matches!(sp.motion, SpellMotion::Attached { part, .. } if part == royalesim::card::EVO_FREEZE_AREA)).count()
}

#[test]
fn her_first_and_third_volleys_leave_a_freezing_area_the_second_does_not() {
    // Held at (4000, 12500), a red Golem held 7000 ahead, topped up: each landing (a drop in its hitpoints) and each new
    // freezing area, by tick.
    let at = (4000, 12500);
    let (mut s, pr) = battle(at);
    let gat = n(at.0, at.1 + 7000);
    let g = s.scenario_spawn_now(Team::Red, "Golem", gat, None).expect("a red Golem");
    let (mut landings, mut made, mut areas) = (Vec::new(), Vec::new(), 0usize);
    for k in 0..260 {
        assert!(s.debug_set_pos(pr, n(at.0, at.1)) && s.debug_set_pos(g, gat));
        let top = s.entity(g).expect("the Golem").max_hp;
        assert!(s.debug_set_hp(g, top));
        s.tick();
        if s.entity(g).expect("the Golem").hp < top {
            landings.push(k);
        }
        let now = freeze_areas(&s);
        if now > areas {
            made.push(k);
        }
        areas = now;
    }
    assert!(landings.len() >= 3, "three volleys: {landings:?}");
    let near = |k: usize| made.iter().any(|m| m.abs_diff(k) <= 1);
    assert!(near(landings[0]) && !near(landings[1]) && near(landings[2]), "areas {made:?} at landings {landings:?}");
}

#[test]
fn her_death_leaves_a_blow_that_reaches_3000_past_the_edges() {
    // Red Golems held 2500 and 4600 from her (edges 1750 and 3850 with its 750): she dies; the near one loses the blow
    // (66 at level 1) within 3 ticks, the far one nothing.
    let at = (4000, 12500);
    let (mut s, pr) = battle(at);
    let (na, fa) = (n(at.0 + 2500, at.1), n(at.0, at.1 + 4600));
    let near = s.scenario_spawn_now(Team::Red, "Golem", na, None).expect("a near Golem");
    let far = s.scenario_spawn_now(Team::Red, "Golem", fa, None).expect("a far Golem");
    for _ in 0..3 {
        assert!(s.debug_set_pos(near, na) && s.debug_set_pos(far, fa) && s.debug_set_pos(pr, n(at.0, at.1)));
        s.tick();
    }
    let (h0, f0) = (s.entity(near).expect("near").hp, s.entity(far).expect("far").hp);
    assert!(s.debug_set_hp(pr, 0));
    let mut lost = Vec::new();
    for _ in 0..4 {
        assert!(s.debug_set_pos(near, na) && s.debug_set_pos(far, fa));
        s.tick();
        lost.push((h0 - s.entity(near).expect("near").hp, f0 - s.entity(far).expect("far").hp));
    }
    let (n_lost, f_lost) = lost[3];
    assert!((165..=172).contains(&n_lost) && f_lost == 0, "the blow: {lost:?}");
}
