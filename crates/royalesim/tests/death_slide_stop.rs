//! spawner.DEATH_SLIDE_STOP -- when a dying troop's sliding member's slide ends under spawner.DEATH_SLIDE_AIM =
//! fixed_end_point (state.rs `phase_reap`, `slide_move_count`; entity.rs `death_slide_until`, `death_slide_capped`).
//!
//! THE READING (move_count; on_reach is the shipped, old arm): the slide makes N = ceil(max(|dx|, |dy|) / 250) moves,
//! (dx, dy) the member's end point less its birth point, and ends after move N wherever the member stands (or earlier,
//! on the step that reaches its end point). Read off client 16.402 and 15.535.29 (20260920-071744, ub-b1-tm2-air, the
//! ub-ds3 family): the axis Lava Pups (birth 250 out, end 2500 out) make 9 moves, the diagonal ones 8, three of those
//! ending 41, 95 and 127 short of their end points after a contact push bent their paths; the Golemites (250 to 1500)
//! make 5, the measured 650, 900, 1150, 1400, 1500.
//!
//! THE SCENE: a Blue Lava Hound at (9000, 9000) killed on the first tick. Its Pups push each other as they slide (the
//! 180-degree one goes more than 100 off its line on the way, tests/death_slide_aim.rs).
//!
//! WHAT IS PINNED (a member's "moves" are the ticks its slide runs after its birth tick):
//!   1. move_count: each Pup makes exactly its move count (8 for a diagonal Pup, 9 for an axis one); the three whose
//!      paths the neighbours' pushes bent (240, 120 and 60 degrees) end 38, 96 and 123 short of their end points,
//!      where on_reach (the old arm, non-vacuity) walks them on to their end points with a 9th move;
//!   2. both arms: a Golem's Golemites make 5 moves and end on their end points, (-1500, 0) and (1500, 0);
//!   3. move_count is inert under current_ray: every Pup's slide is the same under both stop arms;
//!   4. the shipped value is on_reach.
//!
//! PLANT (regression): `death_slide_count_ignored` lays no cap under move_count: (1) goes red.
//!     RUSTFLAGS='--cfg clash_plant="death_slide_count_ignored"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test death_slide_stop
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, DeathSlideAim, DeathSlideStop, DeathSpawnPushback};
use royalesim::{EntityId, Team};

const NEW: DeathSlideStop = DeathSlideStop::MoveCount;
const OLD: DeathSlideStop = DeathSlideStop::OnReach;

fn with(stop: DeathSlideStop, aim: DeathSlideAim) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.death_spawn_pushback = DeathSpawnPushback::ClientRingSlide;
    cfg.calib.death_slide_aim = aim;
    cfg.calib.death_slide_stop = stop;
    cfg
}

fn native(v: Vec2) -> (i32, i32) {
    (v.x / K, v.y / K)
}

fn members(s: &BattleState, unit: &str) -> Vec<EntityId> {
    let mut m: Vec<(u32, EntityId)> = find_live(s, Team::Blue, unit).into_iter().filter(|e| e.spawned_by.is_none()).map(|e| (e.team_seq, e.id)).collect();
    m.sort();
    m.into_iter().map(|(_, id)| id).collect()
}

/// Per member, in creation order: the number of moves its slide made (the ticks after the death tick up to the one its
/// slide ended on) and its offset from the death point when the slide ended.
fn slides(cfg: BattleConfig, parent: &str, unit: &str) -> Vec<(usize, (i32, i32))> {
    let mut s = BattleState::new(7, cfg);
    let id = s.scenario_spawn_now(Team::Blue, parent, t(900, 900), None).expect("the parent");
    assert!(s.debug_set_hp(id, 0));
    s.tick();
    assert!(s.entity(id).is_none(), "the {parent} died on the first tick");
    let ids = members(&s, unit);
    assert!(!ids.is_empty(), "the {parent} left no {unit}");
    let c = s.entity(ids[0]).unwrap().death_slide_centre;
    let mut out: Vec<Option<(usize, (i32, i32))>> = vec![None; ids.len()];
    for n in 1..=20 {
        s.tick();
        for (k, m) in ids.iter().enumerate() {
            let e = s.entity(*m).expect("a member died mid-slide");
            if out[k].is_none() && e.death_slide_radius == 0 {
                out[k] = Some((n, native(e.pos.sub(c))));
            }
        }
    }
    out.into_iter().enumerate().map(|(k, o)| o.unwrap_or_else(|| panic!("member {k} still slides after 20 ticks"))).collect()
}

/// Plant: death_slide_count_ignored.
#[test]
fn a_pup_stops_after_its_move_count_short_of_its_end_point_when_pushes_bent_its_path() {
    let got = slides(with(NEW, DeathSlideAim::FixedEndPoint), "LavaHound", "LavaPups");
    // 300, 240, 180, 120, 60 and 0 degrees: the axis Pups make 9 moves, the diagonal ones 8; the 240-, 120- and
    // 60-degree Pups, pushed by their neighbours, end short of (-1250, -2165), (-1250, 2165) and (1250, 2165)
    assert_eq!(
        got,
        vec![(8, (1249, -2164)), (8, (-1228, -2134)), (9, (-2500, -1)), (8, (-1209, 2078)), (8, (1180, 2063)), (9, (2500, -1))],
        "move_count: each Pup's moves and where its slide ended, from the death point"
    );
    // on_reach, the old arm: the three bent Pups take a 9th move onto their end points
    let old = slides(with(OLD, DeathSlideAim::FixedEndPoint), "LavaHound", "LavaPups");
    assert_eq!(
        old,
        vec![(8, (1249, -2164)), (9, (-1249, -2164)), (9, (-2500, -1)), (9, (-1249, 2165)), (9, (1249, 2164)), (9, (2500, -1))],
        "on_reach: each Pup's moves and where its slide ended, from the death point"
    );
}

#[test]
fn the_golemites_make_five_moves_to_their_end_points_under_both_arms() {
    for arm in [NEW, OLD] {
        assert_eq!(slides(with(arm, DeathSlideAim::FixedEndPoint), "Golem", "Golemite"), vec![(5, (-1500, 0)), (5, (1500, 0))], "{arm:?}: the Golemites' slides");
    }
}

#[test]
fn the_move_count_is_inert_under_current_ray() {
    let new = slides(with(NEW, DeathSlideAim::CurrentRay), "LavaHound", "LavaPups");
    let old = slides(with(OLD, DeathSlideAim::CurrentRay), "LavaHound", "LavaPups");
    assert_eq!(new, old, "current_ray: the Pups' slides differ between the stop arms");
    // not vacuous: the pushes bend these slides too (the 180-degree Pup ends 342 off its axis)
    assert_eq!(new[2], (9, (-2476, -342)), "current_ray: the 180-degree Pup's slide");
}

#[test]
fn the_shipped_value_is_the_old_arm() {
    assert_eq!(Calib::shipped().death_slide_stop, OLD);
}
