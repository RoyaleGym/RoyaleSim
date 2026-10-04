//! spawner.SOUL_OFFSET_ROUNDING: how a Skeleton King copy's drawn offset comes down to native units (state.rs
//! `soul_pass`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, the seven Skeleton King scenes: a copy whose offset from the
//! King is negative on an axis stood 1 nearer him than the engine's (26 times), level on its positive axes (43).
//!
//! THE SCENE (tests/soul_point_base.rs's): the King held on one point before every tick, his press, and his first six
//! copies under each arm. Both arms draw from one generator state, so each copy's draw is the same. WHAT IS PINNED:
//!   1. client15535_toward_zero: no copy stands below the floor arm's on an axis, a copy stands 1 above it on an axis
//!      only where its offset from the King is negative, and at least one does;
//!   2. floor (the old arm, the vacuity check): on every positive axis the two arms stand level;
//!   3. the shipped value is floor (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test soul_offset_rounding`):
//!   * `soul_offset_floor` -- the new arm still shifts: no copy stands 1 above, so (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, SoulOffsetRounding};
use royalesim::Team;

const DECK: [&str; 8] = ["SkeletonKing", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const AT: (i32, i32) = (9000, 11000);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The King's first six copies' points (native), in the order they were made, under `arm`.
fn copies(arm: SoulOffsetRounding) -> Vec<(i32, i32)> {
    let mut cfg = config();
    cfg.calib.soul_offset_rounding = arm;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let king = s.scenario_spawn_now(Team::Blue, "SkeletonKing", n(AT), None).expect("the Skeleton King");
    for _ in 0..40 {
        assert!(s.debug_set_pos(king, n(AT)));
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut seen = Vec::new();
    let mut out: Vec<(i32, i32)> = Vec::new();
    for _ in 0..120 {
        assert!(s.debug_set_pos(king, n(AT)));
        s.tick();
        let mut fresh: Vec<(u32, Vec2)> = s.entities().filter(|e| e.team == Team::Blue && e.cloned && !seen.contains(&e.id)).map(|e| (e.team_seq, e.pos)).collect();
        seen.extend(s.entities().filter(|e| e.team == Team::Blue && e.cloned).map(|e| e.id));
        fresh.sort_by_key(|c| c.0);
        out.extend(fresh.iter().map(|c| (c.1.x / K, c.1.y / K)));
        if out.len() >= 6 {
            out.truncate(6);
            return out;
        }
    }
    panic!("{arm:?}: the scene drifted: {} copies within 120 ticks of the press", out.len());
}

/// Plant: soul_offset_floor.
#[test]
fn a_copys_negative_offset_divides_toward_zero_under_client15535_toward_zero() {
    let (old, new) = (copies(SoulOffsetRounding::Floor), copies(SoulOffsetRounding::Client15535TowardZero));
    let mut raised = 0;
    for (o, w) in old.iter().zip(&new) {
        for (ov, wv, at) in [(o.0, w.0, AT.0), (o.1, w.1, AT.1)] {
            let gap = wv - ov;
            assert!(gap == 0 || gap == 1, "new: a copy stands {gap} from the floor arm's ({o:?} -> {w:?})");
            if gap == 1 {
                assert!(ov < at, "new: a copy rose on an axis whose offset is not negative ({o:?} -> {w:?})");
                raised += 1;
            }
        }
    }
    assert!(raised > 0, "new: no copy stood 1 nearer the King on a negative axis: {old:?} / {new:?}");
}

#[test]
fn the_old_value_and_the_new_agree_on_every_positive_axis() {
    let (old, new) = (copies(SoulOffsetRounding::Floor), copies(SoulOffsetRounding::Client15535TowardZero));
    let mut positive = 0;
    for (o, w) in old.iter().zip(&new) {
        for (ov, wv, at) in [(o.0, w.0, AT.0), (o.1, w.1, AT.1)] {
            if ov > at {
                positive += 1;
                assert_eq!(ov, wv, "a positive axis parted between the arms ({o:?} -> {w:?})");
            }
        }
    }
    assert!(positive > 0, "the scene drifted: no copy on a positive axis");
}

#[test]
fn the_shipped_value_is_floor() {
    assert_eq!(Calib::shipped().soul_offset_rounding, SoulOffsetRounding::Floor);
}
