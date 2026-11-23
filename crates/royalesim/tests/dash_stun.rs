//! combat.DASH_STUN: what a hold (a stun, a freeze, a Hero Ice Wizard's cube) does to a timed jump in flight (state.rs
//! `phase_path16402_for`, `flies` beside the frozen test).
//!
//! THE LAW, measured on client 16.635 (parity r98, the oracle's il-mg-16635 set: d07129d5 k51, an Evo Mega Knight jump a
//! cube caught in the air, the battle's first divergence): the hold neither ends nor slows a DashConstantTime jump; the jump
//! lands on its goal, its blow on its tick, and what is left of the hold holds the unit on its landing point.
//!
//! WHAT IS PINNED, on tests/dash_stand_clock.rs's scene (a Blue Mega Knight put down inside its trigger of a held Red
//! Knight): a Red Zap on the Mega Knight two ticks into its jump; under the new arm it lands where the unzapped jump lands and
//! its blow takes the Knight on the same tick; under ends_where_it_stands (the vacuity check) it stops in the air's path and
//! no blow lands. A Zap late in the jump, its stun outlasting the landing, holds the lander on its landing point.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test dash_stun`):
//!   * `dash_stun_stops_jump` -- the new arm still stops a held jump where it stands: both scenes go red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::entity::AttackPhase;
use royalesim::state::{BattleState, DashAttack, DashFirstSightTrigger, DashStun};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const MK_AT: (i32, i32) = (9500, 11500);
const KNIGHT_AT: (i32, i32) = (13126, 14653);

/// Each tick's (Mega Knight position, its stun_ms, its dash state, its attack phase, the Knight's hp loss that tick).
type Row = (Vec2, i32, i64, AttackPhase, i32);

/// The scene under `arm`, a Red Zap cast on the Mega Knight's point on tick `zap` (none if None); the Knight held on its point
/// and topped up until the jump: the rows of 60 ticks and the jump tick.
fn jump(arm: DashStun, zap: Option<u32>) -> (Vec<Row>, u32) {
    let mut cfg = config();
    cfg.calib.dash_attack = DashAttack::ClientDash;
    cfg.calib.dash_first_sight_trigger = DashFirstSightTrigger::FirstSightTick;
    cfg.calib.dash_stun = arm;
    let mut s = BattleState::new(0, cfg);
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at(KNIGHT_AT), None), (Team::Blue, "MegaKnight", at(MK_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (knight, mk) = (ids[0], ids[1]);
    let kmax = s.entity(knight).expect("the Knight").max_hp;
    let mut rows = Vec::new();
    let mut prev = s.entity(mk).expect("the Mega Knight").pos;
    let mut j = None;
    for t in 0..60u32 {
        if Some(t) == zap {
            let p = s.entity(mk).expect("the Mega Knight").pos;
            s.spawn_unit(Team::Red, "Zap", p, None).expect("the Zap");
        }
        assert!(s.debug_set_pos(knight, at(KNIGHT_AT)));
        if j.is_none() {
            assert!(s.debug_set_hp(knight, kmax));
        }
        let khp = s.entity(knight).map_or(0, |k| k.hp);
        s.tick();
        let m = s.entity(mk).expect("the Mega Knight");
        let khp1 = s.entity(knight).map_or(0, |k| k.hp);
        if j.is_none() && prev.dist2(m.pos) > (200 * K as i64).pow(2) {
            j = Some(t);
        }
        prev = m.pos;
        rows.push((m.pos, m.stun_ms, s.entity_extras(mk).expect("its extras")[2], m.attack_phase, khp - khp1));
    }
    (rows, j.expect("the scene drifted: the Mega Knight never jumped"))
}

/// The unzapped jump: (J0, E0 the first tick after J0 the dash state reads 0, L0 its position then, B0 the tick the Knight
/// loses the blow, the blow's damage).
fn baseline() -> (u32, u32, Vec2, u32, i32) {
    let (rows, j0) = jump(DashStun::EndsWhereItStands, None);
    let e0 = (j0 as usize + 1..rows.len()).find(|&t| rows[t].2 == 0).expect("the jump never ended");
    // the blow: the Knight's biggest loss in the jump (a blue tower's shot lands on him too, once he is no longer topped up)
    let b0 = (j0 as usize..=e0).max_by_key(|&t| (rows[t].4, std::cmp::Reverse(t))).filter(|&t| rows[t].4 > 0).expect("no blow on the Knight");
    (j0, e0 as u32, rows[e0].0, b0 as u32, rows[b0].4)
}

/// combat.DASH_STUN = client16635_flies_to_goal: a Zap two ticks into the jump; the jump lands where the unzapped one lands
/// and its blow takes the Knight on the same tick; under ends_where_it_stands (the vacuity check) it stops at the hold, its
/// rest point at least 2,000 from the landing, and no blow lands. Plant: dash_stun_stops_jump.
#[test]
fn under_client16635_flies_to_goal_a_zapped_mega_knight_lands_on_its_goal() {
    let (j0, e0, l0, b0, drop0) = baseline();
    let zap = j0 + 2;
    let (old, _) = jump(DashStun::EndsWhereItStands, Some(zap));
    // the stun lands after the move of its first tick H: the jump ends where it stands on H + 1, and it stands there while held
    let held = (zap as usize..e0 as usize).find(|&t| old[t].1 > 0).expect("the scene drifted: the Zap held nothing in the jump");
    let rest = old[held].0;
    let hold: Vec<usize> = (held + 1..old.len()).take_while(|&t| old[t].1 > 0).collect();
    assert!(hold.len() >= 5, "the scene drifted: a short hold ({hold:?})");
    assert!(hold.iter().all(|&t| old[t].0 == rest && old[t].2 == 0), "ends_where_it_stands: it stops where the hold finds it");
    assert!(rest.dist2(l0) >= (2000 * K as i64).pow(2), "ends_where_it_stands: its rest point is short of the landing");
    assert_eq!(old[b0 as usize].4, 0, "ends_where_it_stands: no blow lands on B0");
    let (new, _) = jump(DashStun::Client16635FliesToGoal, Some(zap));
    assert!((zap as usize..e0 as usize).any(|t| new[t].1 > 0 && new[t].2 == 2), "the scene: the Zap held it while it was dashing");
    assert_eq!(new[e0 as usize].0, l0, "client16635_flies_to_goal: it lands where the unzapped jump lands (E0 {e0})");
    assert_eq!(new[b0 as usize].4, drop0, "client16635_flies_to_goal: its blow takes the Knight on B0 {b0}");
}

/// combat.DASH_STUN = client16635_flies_to_goal: a Zap late in the jump, its stun outlasting the landing: the jump lands on
/// its goal, its blow on B0, and while the stun lasts after the landing the Mega Knight stands on its landing point,
/// attacking nothing. Plant: dash_stun_stops_jump.
#[test]
fn under_client16635_flies_to_goal_the_hold_applies_after_the_landing() {
    let (j0, e0, l0, b0, drop0) = baseline();
    let zap = j0 + 12;
    assert!(zap < e0, "the scene drifted: the jump ends before the late Zap (E0 {e0})");
    let (new, _) = jump(DashStun::Client16635FliesToGoal, Some(zap));
    assert_eq!(new[e0 as usize].0, l0, "it lands where the unzapped jump lands");
    assert_eq!(new[b0 as usize].4, drop0, "its blow takes the Knight on B0 {b0}");
    let after: Vec<usize> = (e0 as usize + 1..new.len()).take_while(|&t| new[t].1 > 0).collect();
    assert!(!after.is_empty(), "the scene drifted: the stun did not outlast the landing");
    for t in after {
        assert_eq!((new[t].0, new[t].3), (l0, AttackPhase::Idle), "held on its landing point, attacking nothing, on {t}");
    }
}

#[test]
fn the_shipped_arm_is_ends_where_it_stands() {
    assert_eq!(royalesim::state::Calib::shipped().dash_stun, DashStun::EndsWhereItStands);
}
