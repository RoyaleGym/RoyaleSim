//! A HEALING VICTIM IS NOT DOOMED BY A SHOT SHORT OF ITS MAXIMUM (targeting.DOOMED_HEAL_BUFF; combat.rs
//! `doomed_by_shots_in_flight`, state.rs `phase_target_with`'s doomed test): combat.POST_KILL_RETARGET_WAIT's clause (c)
//! frees an attacker whose target died doomed by the homing shots in flight at it. Under client16402_max_hp_while_healing a
//! target carrying a buff with HealPerSecond > 0 (an Evo Bat for 1,000 ms after each of its hits) is doomed only when those
//! shots also cover its maximum hitpoints; a crown tower whose arrow does not serves the wait.
//!
//! THE MEASUREMENT (client 16.402, parity r85, live14): crown towers that killed a heal-buffed Evo Bat whose maximum one
//! arrow does not cover, with no other shooter on it, waited 192 of 208 times; buffed bats an arrow covers and unbuffed bats
//! freed them 136 of 151 and 337 of 387; the engine freed the first group 20 of 22.
//!
//! THE SCENE: the red left princess tower shoots a blue Evo Bat (level 11: 122 maximum hitpoints; the tower's arrow 109)
//! held in its reach on 240 hitpoints, so no arrow kills it; with `knight` the bat hits a red Knight held 1,200 below it
//! (topped up; a Knight cannot hit a flier) and carries its heal; a blue Knight is held out of reach. A probe reads the
//! tower's arrows at the bat and the bat's heal each tick. The scene is then run to a chosen tick L, an arrow's landing tick
//! (the bat's heal on with `knight`, off without): the bat's hitpoints are set to 1 and a red Zap is cast on it (it dies in
//! L), and the blue Knight is moved into the tower's reach, clear of the Zap.
//!
//! WHAT IS PINNED: under client16402_max_hp_while_healing (and _wait_only) the tower holds no target on L + 1; under ignored
//! it takes the blue Knight on L + 1; without the red Knight (no hit, no heal) it takes the blue Knight under every arm.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test doomed_heal_buff`):
//!   * `doom_heal_clause_unread` -- the new arm's doom reads no buff: the first assertion goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::arena::Lane;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, DoomedHealBuff};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

struct Run {
    /// Per tick k: the tower's arrows at the bat in flight at k's start.
    arrows: Vec<usize>,
    /// Per tick k: whether the bat carries a buff with HealPerSecond > 0 at k's start.
    healing: Vec<bool>,
    /// Per tick k: the tower's target after k.
    target: Vec<Option<EntityId>>,
    blue_knight: EntityId,
}

fn heals(s: &BattleState, id: EntityId) -> bool {
    s.entity(id).is_some_and(|e| e.buffs.iter().any(|b| !b.is_empty() && s.config().cards.buffs[b.id as usize - 1].heal_per_second > 0))
}

fn scene(arm: DoomedHealBuff, knight: bool, kill: Option<usize>, ticks: usize) -> Run {
    let mut cfg: BattleConfig = config();
    cfg.calib.doomed_heal_buff = arm;
    cfg.decks = [vec!["Bats".into(), "Knight".into()], vec!["Knight".into(), "Zap".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let pt = s.arena().princess_tower_pos(Team::Red, Lane::Left);
    let tower = s.entities().find(|e| e.team == Team::Red && e.kind.is_crown_tower() && e.pos == pt).map(|e| e.id).expect("the red left princess tower");
    let p = (pt.x / K, pt.y / K);
    let (bat_at, kn_at, far, near) = ((p.0, p.1 - 6000), (p.0, p.1 - 7200), (14500, 5000), (p.0 + 3500, p.1 - 3500));
    s.spawn_unit(Team::Blue, "Bats_EV1", at(bat_at), None).expect("the bats");
    let red_knight = knight.then(|| s.scenario_spawn_now(Team::Red, "Knight", at(kn_at), None).expect("a red Knight"));
    let blue_knight = s.scenario_spawn_now(Team::Blue, "Knight", at(far), None).expect("a blue Knight");
    s.tick();
    let bats: Vec<_> = find_live(&s, Team::Blue, "Bats_EV1").iter().map(|e| e.id).collect();
    let bat = bats[0];
    for b in &bats[1..] {
        assert!(s.debug_set_hp(*b, 0));
    }
    assert_eq!(s.entity(bat).expect("the bat").max_hp, 122, "the Evo Bat's 150 % of 81 at level 11");
    let (mut arrows, mut healing, mut target) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..ticks {
        let in_reach = kill.is_some_and(|l| k >= l);
        assert!(s.debug_set_pos(blue_knight, at(if in_reach { near } else { far })));
        let top = s.entity(blue_knight).expect("the blue Knight").max_hp;
        assert!(s.debug_set_hp(blue_knight, top));
        if let Some(rk) = red_knight {
            assert!(s.debug_set_pos(rk, at(kn_at)));
            let top = s.entity(rk).expect("the red Knight").max_hp;
            assert!(s.debug_set_hp(rk, top));
        }
        if s.entity(bat).is_some() {
            assert!(s.debug_set_pos(bat, at(bat_at)));
            assert!(s.debug_set_hp(bat, if kill == Some(k) { 1 } else { 240 }));
            if kill == Some(k) {
                s.spawn_unit(Team::Red, "Zap", at(bat_at), None).expect("the Zap");
            }
        }
        arrows.push(s.projectiles().iter().filter(|q| q.firer == Some(tower) && q.target == bat).count());
        healing.push(heals(&s, bat));
        s.tick();
        target.push(s.entity(tower).expect("the tower stands").target);
    }
    Run { arrows, healing, target, blue_knight }
}

/// The probe: an arrow's landing tick past 60 (in flight at its start, gone from the next tick's start) with the bat's heal
/// on at its start and the tick before it under `knight`, off without.
fn probe(knight: bool) -> usize {
    let r = scene(DoomedHealBuff::Ignored, knight, None, 400);
    let n = &r.arrows;
    assert!(n.iter().all(|&c| c <= 1), "the scene drifted: two tower arrows in flight at once: {n:?}");
    assert!(n.contains(&1), "the scene drifted: the tower never shot the bat");
    if knight {
        assert!(r.healing.iter().any(|&h| h), "the scene drifted: the bat never carried its heal");
    } else {
        assert!(r.healing.iter().all(|&h| !h), "the scene drifted: the bat healed with no Knight to hit");
    }
    (61..n.len() - 1)
        .find(|&k| n[k] == 1 && n[k + 1] == 0 && r.healing[k] == knight && r.healing[k - 1] == knight)
        .expect("the scene drifted: no arrow landing with the bat's heal as wanted")
}

/// The tower's target on L + 1, with the bat dying in L (zapped at 1 hitpoint).
fn after(arm: DoomedHealBuff, knight: bool, l: usize) -> (Option<EntityId>, EntityId) {
    let r = scene(arm, knight, Some(l), l + 3);
    (r.target[l + 1], r.blue_knight)
}

/// targeting.DOOMED_HEAL_BUFF = client16402_max_hp_while_healing (client 16.402, parity r85, live14: 192 of 208 waits).
/// Plant: doom_heal_clause_unread.
#[test]
fn under_client16402_max_hp_while_healing_a_tower_waits_after_killing_a_healing_evo_bat() {
    let l = probe(true);
    let (t, _) = after(DoomedHealBuff::Client16402MaxHpWhileHealing, true, l);
    assert_eq!(t, None, "client16402_max_hp_while_healing: the tower was freed though its 109 arrow is short of the healing bat's 122");
    let (t, _) = after(DoomedHealBuff::Client16402MaxHpWhileHealingWaitOnly, true, l);
    assert_eq!(t, None, "client16402_max_hp_while_healing_wait_only: the tower was freed");
    // NOT VACUOUS: ignored frees it on L + 1
    let (t, bk) = after(DoomedHealBuff::Ignored, true, l);
    assert_eq!(t, Some(bk), "ignored: the tower waited though its arrow covers the bat's 1 hitpoint");
    // a bat without its heal (no hit) is doomed by the arrow under every arm
    let l0 = probe(false);
    for arm in [DoomedHealBuff::Ignored, DoomedHealBuff::Client16402MaxHpWhileHealing, DoomedHealBuff::Client16402MaxHpWhileHealingWaitOnly] {
        let (t, bk) = after(arm, false, l0);
        assert_eq!(t, Some(bk), "{arm:?}: a bat without its heal was not doomed by the arrow");
    }
}

#[test]
fn the_shipped_arm_reads_no_heal_buff() {
    assert_eq!(Calib::shipped().doomed_heal_buff, DoomedHealBuff::Ignored);
}
