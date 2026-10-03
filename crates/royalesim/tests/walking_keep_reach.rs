//! targeting.WALKING_KEEP_REACH: the reach within which a WALKING holder keeps its target (target.rs `keep_own_radius`,
//! read by `decide`'s keep tests).
//!
//! THE LAW, measured on client 15.535.29 (parity's keep_band_truth.py): a walking unit whose target stands past Range +
//! LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET + the target's radius but within that + its own radius, with another enemy it
//! could take strictly nearer, keeps its target when its row walks with its own radius (258 of 258 holders; 68 of 68 on
//! the 16.402 corpus) and takes the nearer when its row walks without it (targeting.VARIABLE_DAMAGE_WALK_REACH): the
//! Inferno Dragon of sp-form-InfernoDragon-evo-s0 t1094, 1 of 1.
//!
//! The scene: a Blue Inferno Dragon at (9500, 8000) takes the only enemy, a Red Knight at (9500, 12700), and walks after
//! it; once the Knight stands in the band a second Red Knight is put down NEARER away off to the Dragon's left. WHAT IS
//! PINNED, each with its preconditions (the Dragon holds the far Knight, walks and is not in its attack, the far Knight
//! stands in the band, the near one strictly nearer and outside the reach the Dragon walks to):
//!   1. client15535_walking_reach: the Dragon takes the near Knight on the next tick;
//!   2. own_radius: it keeps the far Knight (the old arm);
//!   3. the shipped value is own_radius (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test walking_keep_reach`):
//!   * `walking_keep_reach_own_radius` -- the new arm's walking holder still keeps to both radii: (1) goes red.
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, WalkingKeepReach};
use royalesim::{EntityId, Team};

const FLYER_AT: (i64, i64) = (9500, 8000);
const FAR_AT: (i64, i64) = (9500, 12700);
/// How much nearer than the far Knight the near one is put down (native).
const NEARER: i64 = 200;
/// The ticks the Dragon may take to stand walking with the far Knight in the band.
const SETTLE: u32 = 20;

fn native(v: Vec2) -> (i64, i64) {
    ((v.x / K) as i64, (v.y / K) as i64)
}

fn at(p: (i64, i64)) -> Vec2 {
    Vec2::new(p.0 as i32 * K, p.1 as i32 * K)
}

fn dist(a: (i64, i64), b: (i64, i64)) -> i64 {
    isqrt((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2))
}

/// The scene under `arm`: the Dragon's target on the tick after the near Knight is put down, the far Knight, the near.
fn scene(arm: WalkingKeepReach) -> (Option<EntityId>, EntityId, EntityId) {
    let mut cfg = config();
    cfg.calib.walking_keep_reach = arm;
    let ext = (cfg.calib.range_extension_to_keep_target / K) as i64;
    let mut s = BattleState::new(0, cfg);
    let dragon = s.scenario_spawn_now(Team::Blue, "InfernoDragon", at(FLYER_AT), None).expect("spawn the Inferno Dragon");
    let far = s.scenario_spawn_now(Team::Red, "Knight", at(FAR_AT), None).expect("spawn the far Knight");
    let range = (card_stat(&s, "InfernoDragon").range / K) as i64;
    let own = (s.entity(dragon).expect("the Dragon stands").radius / K) as i64;
    let knight = (s.entity(far).expect("the far Knight stands").radius / K) as i64;
    let (lo, hi) = (range + ext + knight, range + ext + knight + own);
    let mut last = native(s.entity(dragon).expect("the Dragon stands").pos);
    let mut settled = None;
    for _ in 0..SETTLE {
        s.tick();
        let (d, k) = (s.entity(dragon).expect("the Dragon stands"), s.entity(far).expect("the far Knight stands"));
        let p = native(d.pos);
        let gap = dist(p, native(k.pos));
        let walked = p != last;
        last = p;
        if d.target == Some(far) && walked && gap > lo && gap <= hi {
            settled = Some((p, gap));
            break;
        }
    }
    let (p, gap) = settled.unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the Dragon never walked after the far Knight with it in ({lo}, {hi}]"));
    // the near Knight, NEARER closer, off to the Dragon's left (cos 60, sin 60 in thousandths), outside the reach it walks to
    let r = gap - NEARER;
    let near_at = (p.0 - r * 866 / 1000, p.1 + r * 500 / 1000);
    let near = s.scenario_spawn_now(Team::Red, "Knight", at(near_at), None).expect("spawn the near Knight");
    let dn = dist(p, native(s.entity(near).expect("the near Knight stands").pos));
    assert!(dn < gap, "{arm:?}: the scene drifted: the near Knight stands {dn}, not nearer than the far one's {gap}");
    assert!(dn > range + knight, "{arm:?}: the scene drifted: the near Knight stands {dn}, inside the reach the Dragon walks to");
    s.tick();
    let now = s.entity(dragon).expect("the Dragon stands").target;
    (now, far, near)
}

#[test]
fn a_walking_inferno_dragon_takes_a_nearer_enemy_past_the_reach_it_walks_to() {
    let (now, far, near) = scene(WalkingKeepReach::Client15535WalkingReach);
    assert_eq!(now, Some(near), "new: the walking Dragon kept {far:?} past Range + 25 + its radius with {near:?} nearer");
}

#[test]
fn the_old_value_keeps_the_far_target() {
    let (now, far, near) = scene(WalkingKeepReach::OwnRadius);
    assert_eq!(now, Some(far), "old: the walking Dragon let {far:?} go for {near:?} inside Range + 25 + both radii");
}

#[test]
fn the_shipped_value_is_own_radius() {
    assert_eq!(Calib::shipped().walking_keep_reach, WalkingKeepReach::OwnRadius);
}
