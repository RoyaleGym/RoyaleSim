//! STACKED TIES -- two blockers on one exact point must not fall back to slot order.
//!
//! WHY IT EXISTS: choosing the troop to walk around by (distance, frame x, frame y)
//! alone leaves equal keys to be broken by neighbour-list order, which is slot
//! order, and slot order differs between seats as soon as slots are reused. It is
//! rare -- about one random game in a thousand -- and it looks like this: a freshly
//! deployed Skeleton lands exactly on a Skeleton Army unit, the mover's `YieldKey`
//! falls between the two blockers' keys, and "which blocker" becomes "which side".
//! `path::first_blocker` has the same tie for two BUILDINGS on one centre. No
//! scenario in tests/mirror.rs ever puts two blockers on one point, and a mirror
//! gate that never reaches the tie certifies nothing about it.
//!
//! THE CHECKS: `constructed_*` rebuild the tie from nothing -- so a
//! snapshot-format change cannot retire the gate -- for troops and for buildings.
//! The tie is forced: two blockers stacked on one point, colinear ahead of a mover,
//! and the two seats' blocker pairs in OPPOSITE slot order (freed tower slots are
//! reused LIFO), with each blocker's identity chosen so the pick changes the answer.
//!
//! RETIRED 2026-09-21, OUT LOUD: `fixture_384_rotation_holds_after_the_stacked_deploy`
//! and `fixture_384_format3_migration_is_self_checked`, and with them the helpers
//! that loaded the snapshot against the 2018 card table.
//!   WHAT THEY COVERED. The first replayed the recorded board of game 384 from tick
//!   2970 -- one tick before the divergence that found this tie -- issuing the same
//!   deploy for both seats and holding the rotation for 50 ticks, over a full game
//!   state rather than a built one. The second was the only gate on
//!   `state.rs::migrate_v3`, the format-3 -> current snapshot migration: it tampered
//!   with a hashed field and with the card fingerprint and required each to be
//!   refused by name, then required the untampered load to re-save and round-trip.
//!   WHY THEY GO. The snapshot no longer loads: it is refused on the format-3 card
//!   fingerprint. `migrate_v3` rebuilds that fingerprint by stripping the CardDef
//!   fields DECLARED AFTER format 3, which only reaches fields at the END of the
//!   struct, so a change to any value format 3 also printed puts the rebuilt text
//!   permanently out of reach of the hash saved in the file. The board cannot be
//!   re-recorded: it came from a random-game driver that is not in this repo, and
//!   there is no maker for it under tools/ (every tools/make_*_fixture.py writes a
//!   different fixture). Skipping it would have left a green suite certifying
//!   nothing, so it is deleted instead.
//!   WHAT STILL COVERS THE TIE. `constructed_stacked_troops_keep_the_rotation` and
//!   `constructed_stacked_buildings_keep_the_rotation`, both green. They were
//!   written for exactly this event -- they build the tie from nothing, and they
//!   reach both choosers the fixture reached (`YieldKey` for troops,
//!   `path::first_blocker` for buildings). Each plant below still names one.
//!   WHAT WENT WITH IT, unreplaced: the tie exercised inside a real board (its
//!   entity population, elixir and cycle state), and any gate at all on
//!   `migrate_v3`. The snapshot is still at tests/fixtures/snap_384_tick2970.bin if
//!   the migration is ever re-pointed at it.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant
//! cargo test --test stacked_tie`):
//!   slot_order_blocker_tie   blocker key back to (dist2, x, y)
//!       -> constructed_stacked_troops_keep_the_rotation
//!   slot_order_obstacle_tie  obstacle key back to (dist2, x, y)
//!       -> constructed_stacked_buildings_keep_the_rotation
//!
//! WHAT IT CANNOT CATCH: a tie in some other chooser that no scenario here
//! reaches; LaneSnap only (the colinear case is built on its horizontal lane leg --
//! the tie-break code is shared by every model, the approach geometry is not).
mod common;

use royalesim::arena::FootprintModel;
use royalesim::fixed::Vec2;
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, PathModel, Team};
use common::*;

/// Kill all four princess towers (symmetric: 2 crowns each) so the next spawns
/// reuse slots 5, 4, 2, 1 (LIFO), then allocate fresh ones. Spawning Blue's
/// (mover, A, B) then Red's gives Blue A > B in slot order and Red A < B.
fn free_tower_slots(s: &mut BattleState) {
    for (team, k) in [(Team::Blue, 1), (Team::Blue, 2), (Team::Red, 2), (Team::Red, 1)] {
        s.scenario_set_tower_hp(team, k, 0).unwrap();
    }
    assert_eq!(s.crowns(), [2, 2]);
}

fn lanesnap_circle_config() -> BattleConfig {
    let mut cfg = config();
    cfg.path_model = PathModel::LaneSnap;
    cfg.footprint_model = FootprintModel::CollisionRadiusCircle;
    cfg
}

/// Spawn (mover, blocker A, blocker B) for both seats, Blue first, Red at the
/// rotation. Returns [[mover, a, b]; 2].
fn spawn_trio(s: &mut BattleState, mover: (&str, Vec2, Option<i32>), a: (&str, Vec2, Option<i32>), b: (&str, Vec2, Option<i32>)) -> [[EntityId; 3]; 2] {
    let mut out = [[EntityId::default(); 3]; 2];
    for (ti, team) in [Team::Blue, Team::Red].into_iter().enumerate() {
        for (k, (card, p, hp)) in [mover, a, b].into_iter().enumerate() {
            let at = if team == Team::Blue { p } else { mirror(s, p) };
            out[ti][k] = s.scenario_spawn_now(team, card, at, hp).unwrap_or_else(|e| panic!("{team:?} {card} at {at:?}: {e:?}"));
        }
    }
    let order = |ids: &[EntityId; 3]| ids[1].index < ids[2].index;
    assert_ne!(order(&out[0]), order(&out[1]), "the two seats' stacked pairs must be in OPPOSITE slot order: {out:?}");
    out
}

fn run_checked(name: &str, s: &mut BattleState, ticks: u32) {
    for _ in 0..ticks {
        s.tick();
        check_mirror(s).unwrap_or_else(|e| panic!("{name}: {e}\ncensus: {:?}", census(s)));
    }
}

#[test]
fn constructed_stacked_troops_keep_the_rotation() {
    // A Blue Knight at (8, 12) walks LaneSnap's horizontal leg toward the left
    // bridge (-x). Two more Blue Knights stand stacked on ONE point exactly ahead
    // of it on the same y, one step beyond touching. hp makes the three YieldKeys
    // A < mover < B, so the colinear rule sends the mover to opposite sides
    // depending on which of A and B it picks. Red has the rotation of all of it,
    // with its pair in the opposite slot order.
    let mut s = BattleState::new(3, lanesnap_circle_config());
    free_tower_slots(&mut s);
    let knight = card_stat(&s, "Knight").clone();
    let full = s.cards().scaled(s.cards().index("Knight").unwrap(), s.config().card_level[0], knight.hitpoints).unwrap();
    let (r, step) = (knight.collision_radius, knight.speed * s.config().calib.speed_to_subtiles_per_tick);
    let mover_at = t(800, 1200);
    let gap = 2 * r + step;
    let stack = Vec2::new(mover_at.x - gap, mover_at.y);
    // Vacuity: the stack is inside avoid_units' probe window (2 steps + half a
    // radius, inflated by both radii) and not yet overlapping the mover.
    assert!(gap > 2 * r && gap - (2 * step + r / 2) < 2 * r, "gap {gap} outside the avoidance window (r {r}, step {step})");
    let ids = spawn_trio(&mut s, ("Knight", mover_at, Some(full / 2)), ("Knight", stack, Some(full / 4)), ("Knight", stack, Some(full * 3 / 4)));
    s.tick();
    check_mirror(&s).unwrap_or_else(|e| panic!("stacked troops, tick 1: {e}"));
    // Vacuity: the mover really side-stepped (the avoidance fired), not just walked.
    let m = s.entity(ids[0][0]).unwrap().pos;
    assert_ne!(m.y, mover_at.y, "the mover did not side-step: the scenario never consulted the blockers");
    run_checked("stacked troops", &mut s, 300);
}

#[test]
fn constructed_stacked_buildings_keep_the_rotation() {
    // The same geometry for `first_blocker`: a Cannon and a Tesla (different
    // collision radii, so a different detour reach) on ONE centre, ahead of a Knight
    // on LaneSnap's horizontal leg, one step beyond touching the larger footprint.
    let mut s = BattleState::new(3, lanesnap_circle_config());
    free_tower_slots(&mut s);
    let knight = card_stat(&s, "Knight").clone();
    let (big, small) = (card_stat(&s, "Cannon").collision_radius, card_stat(&s, "Tesla").collision_radius);
    let (r, step) = (knight.collision_radius, knight.speed * s.config().calib.speed_to_subtiles_per_tick);
    assert_ne!(big, small, "the two buildings must differ in reach, or the pick cannot change the answer");
    let (big_card, small_card, big, small) = if big > small { ("Cannon", "Tesla", big, small) } else { ("Tesla", "Cannon", small, big) };
    let mover_at = t(800, 1200);
    let gap = big + r + step;
    let probe = 2 * step + r / 2;
    // Vacuity: LaneSnap's contact probe reaches BOTH footprints (so both compete)
    // and the mover starts clear of the larger one.
    assert!(gap - probe < small + r, "the probe does not reach the smaller building: gap {gap} probe {probe} small {small} r {r}");
    let stack = Vec2::new(mover_at.x - gap, mover_at.y);
    let ids = spawn_trio(&mut s, ("Knight", mover_at, None), (big_card, stack, None), (small_card, stack, None));
    s.tick();
    check_mirror(&s).unwrap_or_else(|e| panic!("stacked buildings, tick 1: {e}"));
    let m = s.entity(ids[0][0]).unwrap().pos;
    assert_ne!(m.y, mover_at.y, "the mover did not detour: the scenario never consulted the buildings");
    run_checked("stacked buildings", &mut s, 300);
}
