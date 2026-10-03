//! transform.FALL_GROUNDING: when a falling evolved flier (card.rs `FallDef`, the Evo Royal Hogs) leaves the air, and
//! what it keeps of its walk (state.rs `fall_pass`, `fall_ground_late`).
//!
//! THE LAW, measured on client 15.535.29 (the Evo Royal Hogs' three scenes), L the landing tick (the trigger,
//! transition_ms and a tick): through L a hog still meets the fliers' contact (sp-hogs-cannon-s0 t1162, two hogs frozen
//! in the river pushed by a flying neighbour one tick more than the engine's), ground-only enemies take it on L + 2, 4 of
//! 4 (the engine's on L), and its avoidance offset runs on through the landing, 2 of 2 (110, 100, 90 ... where the
//! engine's went to 0). So it leaves the air on L + 1, after the Target phase and before the move pass, its walk kept.
//!
//! WHAT IS PINNED, on the first hog of an evolved play over the river, set under its health line as a blow in the tick
//! before frame 30 would leave it (L = frame 41, its blow on frame 42), its points held:
//!   1. client15535_late_kept_walk: in the air through frame 41 and on the ground (its grounded row) from frame 42, the
//!      blow on frame 42; status_rebind: on the ground from frame 41;
//!   2. a red Knight held beside it takes it as its target from frame 43 (L + 2); under status_rebind from frame 41;
//!   3. a red Balloon held ahead of it, coming its way (an air blocker): the hog's avoidance offset after frame 42 is its
//!      offset after frame 41 less 10 (the decay; on the ground it no longer sees the Balloon); under status_rebind it is
//!      0 after frame 41.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! fall_grounding`):
//!   * `fall_lands_in_status` -- the new arm lands the hog on L in the Status phase: (1) and (2) go red;
//!   * `fall_landing_resets_walk` -- the late landing drops the walk: (3) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, FallGrounding};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Over the river, out of every crown tower's reach (no tower shot trips the health line).
const AT: (i32, i32) = (9500, 15500);

/// One frame (after its tick): the first hog's card, whether it is in the air, its avoidance offset; the red units'
/// targets and what each lost.
struct Frame {
    card: String,
    flying: bool,
    offset: i32,
    targets: Vec<Option<EntityId>>,
    losses: Vec<i32>,
}

/// The four hogs of one evolved play put down at AT, held on their first points (all but the first moved west along
/// AT's line); red `units` put (dx, dy) from the first hog and held there, their hitpoints topped up; the first hog set
/// to 828 of its 837 before frame 30's tick.
fn scene(arm: FallGrounding, units: &[(&str, (i32, i32))]) -> (EntityId, Vec<Frame>) {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["RoyalHogs".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.fall_grounding = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "RoyalHogs_EV1", n(AT.0, AT.1), None).expect("the hogs");
    s.tick();
    let mut hogs: Vec<(EntityId, Vec2)> = find_live(&s, Team::Blue, "RoyalHogs_EV1").iter().map(|e| (e.id, e.pos)).collect();
    assert_eq!(hogs.len(), 4, "four hogs");
    for (k, h) in hogs.iter_mut().enumerate().skip(1) {
        h.1 = n(2500 + 1500 * k as i32, AT.1);
    }
    let first = hogs[0].1;
    let reds: Vec<(EntityId, Vec2, i32)> = units
        .iter()
        .map(|(card, (dx, dy))| {
            let p = Vec2::new(first.x + dx * K, first.y + dy * K);
            let id = s.scenario_spawn_now(Team::Red, card, p, None).expect("a red unit");
            (id, p, s.entity(id).expect("the red unit").max_hp)
        })
        .collect();
    let mut out = Vec::new();
    for k in 0..50 {
        for (id, p) in &hogs {
            if s.entity(*id).is_some() {
                assert!(s.debug_set_pos(*id, *p));
            }
        }
        for (id, p, top) in &reds {
            assert!(s.debug_set_pos(*id, *p));
            assert!(s.debug_set_hp(*id, *top));
        }
        if k == 30 {
            assert!(s.debug_set_hp(hogs[0].0, 828));
        }
        s.tick();
        let h = s.entity(hogs[0].0).expect("the first hog");
        out.push(Frame {
            card: h.card.to_string(),
            flying: h.flying,
            offset: h.avoid_offset,
            targets: reds.iter().map(|(id, _, _)| s.entity(*id).expect("a red unit held alive").target).collect(),
            losses: reds.iter().map(|(id, _, top)| top - s.entity(*id).expect("a red unit held alive").hp).collect(),
        });
    }
    (hogs[0].0, out)
}

#[test]
fn the_hog_leaves_the_air_on_the_tick_after_its_landing_tick() {
    let (_, f) = scene(FallGrounding::Client15535LateKeptWalk, &[("Knight", (0, 1700))]);
    for (k, x) in f.iter().enumerate().take(42) {
        assert_eq!((x.card.as_str(), x.flying), ("RoyalHogs_EV1", true), "client15535_late_kept_walk: in the air on frame {k}");
    }
    for (k, x) in f.iter().enumerate().skip(42) {
        assert_eq!((x.card.as_str(), x.flying), ("RoyalHog_EV1_Grounded", false), "client15535_late_kept_walk: on the ground on frame {k}");
    }
    assert!(f[..42].iter().all(|x| x.losses[0] == 0), "nothing on the Knight before the blow");
    assert_eq!(f[42].losses[0], 43, "the blow on the tick after the landing tick, as under either arm");
    let (_, g) = scene(FallGrounding::StatusRebind, &[("Knight", (0, 1700))]);
    assert_eq!((g[40].flying, g[41].flying), (true, false), "status_rebind: on the ground from frame 41");
}

#[test]
fn a_ground_only_enemy_takes_the_landed_hog_two_ticks_after_its_landing_tick() {
    let first_on = |arm: FallGrounding| -> usize {
        let (hog, f) = scene(arm, &[("Knight", (0, 1700))]);
        assert!(f[..30].iter().all(|x| x.targets[0] != Some(hog)), "{arm:?}: the Knight took the hog in the air");
        f.iter().position(|x| x.targets[0] == Some(hog)).expect("the Knight never took the landed hog")
    };
    assert_eq!(first_on(FallGrounding::Client15535LateKeptWalk), 43, "client15535_late_kept_walk: the Knight's first frame on the hog");
    assert_eq!(first_on(FallGrounding::StatusRebind), 41, "status_rebind: the Knight's first frame on the hog");
}

#[test]
fn the_landing_keeps_the_avoidance_offset() {
    let (_, f) = scene(FallGrounding::Client15535LateKeptWalk, &[("Balloon", (0, 900))]);
    let before = f[41].offset;
    assert!(before.abs() > 10, "the scene drifted: the hog's offset after frame 41 is {before}, nothing to keep");
    let decayed = if before > 0 { before - 10 } else { before + 10 };
    assert_eq!(f[42].offset, decayed, "client15535_late_kept_walk: the offset after the grounding tick (after frame 41: {before})");
    let (_, g) = scene(FallGrounding::StatusRebind, &[("Balloon", (0, 900))]);
    assert!(g[40].offset != 0, "the scene drifted: under status_rebind the hog's offset after frame 40 is 0");
    assert_eq!(g[41].offset, 0, "status_rebind: the transformation drops the offset, and the grounded hog no longer sees the Balloon");
}
