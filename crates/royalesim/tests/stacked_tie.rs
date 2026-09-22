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
//! THE CHECKS:
//!   1. `fixture_384_*` replays the captured snapshot (tick 2970, one tick before
//!      the divergence) with the same deploy for both seats, and holds the rotation
//!      for 50 ticks.
//!   2. `constructed_*` rebuild the tie from nothing -- so a snapshot-format change
//!      cannot retire the gate -- for troops and for buildings. The tie is forced:
//!      two blockers stacked on one point, colinear ahead of a mover, and the two
//!      seats' blocker pairs in OPPOSITE slot order (freed tower slots are reused
//!      LIFO), with each blocker's identity chosen so the pick changes the answer.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant
//! cargo test --test stacked_tie`):
//!   slot_order_blocker_tie   blocker key back to (dist2, x, y)
//!       -> fixture_384_rotation_holds_after_the_stacked_deploy,
//!          constructed_stacked_troops_keep_the_rotation
//!   slot_order_obstacle_tie  obstacle key back to (dist2, x, y)
//!       -> constructed_stacked_buildings_keep_the_rotation
//!
//! WHAT IT CANNOT CATCH: a tie in some other chooser that no scenario here
//! reaches; LaneSnap only (the colinear case is built on its horizontal lane leg --
//! the tie-break code is shared by every model, the approach geometry is not).
mod common;

use royalesim::arena::FootprintModel;
use royalesim::entity::EntityKind;
use royalesim::fixed::Vec2;
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, PathModel, Team};
use common::*;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/snap_384_tick2970.bin");

/// THE FIXTURE WAS RECORDED AGAINST THE 2018 CARD DATA. The engine's
/// data/derived/cards.json is now the 15.535.29 vintage, whose stats (and card list) the
/// format-3 fingerprint cannot match; the 2018 file lives beside it as
/// data/derived/cards-2018.json (`tools/extract_cards.py --vintage 2018`, byte-identical
/// to the file the fixture was saved against) and these two tests load through it.
/// Missing, they REFUSE (never skip): regenerate the file.
fn cards_2018() -> std::sync::Arc<royalesim::card::CardDb> {
    let db = royalesim::card::CardDb::load_repo_file("cards-2018.json")
        .unwrap_or_else(|e| panic!("the 2018 card data the format-3 fixture needs is unavailable ({e}); run tools/extract_cards.py --vintage 2018 -- refusing to skip"));
    std::sync::Arc::new(db)
}

fn load_2018(blob: &[u8]) -> Result<BattleState, String> {
    BattleState::load_with(blob, cards_2018(), royalesim::arena::Arena::shipped())
}

#[test]
fn fixture_384_rotation_holds_after_the_stacked_deploy() {
    // The snapshot is tied to SNAPSHOT_FORMAT and to the 2018 cards.json (`cards_2018`).
    // If it no longer loads, this REFUSES rather than skipping: rebuild the fixture
    // (the constructed tests below carry the gate meanwhile) or retire it out loud.
    let blob = std::fs::read(FIXTURE).unwrap_or_else(|e| panic!("{FIXTURE}: {e}"));
    let mut s = load_2018(&blob).unwrap_or_else(|e| {
        panic!("fixture snap_384_tick2970.bin no longer loads ({e}); regenerate it or retire this test OUT LOUD -- not a skip")
    });
    assert_eq!(s.tick_count(), 2970);
    check_mirror(&s).unwrap_or_else(|e| panic!("the fixture must start rotation-symmetric: {e}"));
    // The commands from the captured game: hand slot 3 (Skeletons) at
    // (81000, 207000) and its rotation.
    assert_eq!(s.hand(Team::Blue)[3], "Skeletons");
    assert_eq!(s.hand(Team::Red)[3], "Skeletons");
    let a = s.arena().clone();
    s.deploy_slot(Team::Blue, 3, Vec2::new(81000, 207000)).unwrap();
    s.deploy_slot(Team::Red, 3, Vec2::new(a.width - 81000, a.height - 207000)).unwrap();
    // Vacuity: the tie really is in this state -- a pending Skeleton lands EXACTLY
    // on a live troop of its own team, for both seats.
    for team in [Team::Blue, Team::Red] {
        let stacked = s
            .pending_spawns()
            .iter()
            .filter(|(t, _, p)| *t == team && s.entities().any(|e| e.team == team && e.kind == EntityKind::Troop && e.pos == *p))
            .count();
        assert!(stacked >= 1, "{team:?}: no pending spawn lands on a live troop; the fixture no longer reaches the tie");
    }
    for _ in 0..50 {
        s.tick();
        check_mirror(&s).unwrap_or_else(|e| panic!("game 384 replay: {e}\ncensus: {:?}", census(&s)));
    }
    assert_eq!(s.tick_count(), 3020);
}

/// THE FIXTURE IS SNAPSHOT FORMAT 3 and loads through `state.rs::migrate_v3`
/// (the spell keys made the engine format 4). The migration is only evidence if
/// its self-check can fail: the format-3 hash of the migrated state must equal the
/// hash saved in the file, and the card fingerprint must be format 3's. Tamper each
/// in the loaded bytes and the load must refuse, naming the format-3 check; untampered, the
/// migrated battle must re-save as the current format and round-trip exactly.
#[test]
fn fixture_384_format3_migration_is_self_checked() {
    let blob = std::fs::read(FIXTURE).unwrap_or_else(|e| panic!("{FIXTURE}: {e}"));
    let text = String::from_utf8(blob.clone()).expect("fixture is utf-8 json");
    assert!(text.starts_with("{\"format\":3,"), "the fixture is no longer format 3; this gate certifies the migration and must be re-pointed");
    // A hashed field (the tick) changed: the format-3 self-check must refuse.
    let tampered = text.replacen("\"tick\":2970", "\"tick\":2971", 1);
    assert_ne!(tampered, text, "tamper did not apply -- a plant must verify its own edit");
    let err = load_2018(tampered.as_bytes()).map(|_| ()).expect_err("a tampered format-3 snapshot loaded");
    assert!(err.contains("format-3 hash"), "refused for the wrong reason: {err}");
    // A foreign card fingerprint: refused before any state is built.
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let fp = v["cards_fingerprint"].as_u64().unwrap();
    let foreign = text.replacen(&format!("\"cards_fingerprint\":{fp}"), &format!("\"cards_fingerprint\":{}", fp ^ 1), 1);
    assert_ne!(foreign, text);
    let err = load_2018(foreign.as_bytes()).map(|_| ()).expect_err("a foreign-card format-3 snapshot loaded");
    assert!(err.contains("format-3 fingerprint"), "refused for the wrong reason: {err}");
    // And against the ENGINE's own (15.535) card data the fixture is refused on its
    // fingerprint too: a format-3 blob never runs on stats it was not saved against.
    let err = BattleState::load(&blob).map(|_| ()).expect_err("the 2018 fixture loaded against the 15.535 card data");
    assert!(err.contains("format-3 fingerprint"), "refused for the wrong reason: {err}");
    // Untampered: loads, re-saves as the current format, and that round-trips.
    let s = load_2018(&blob).unwrap();
    let again = s.save();
    assert!(String::from_utf8_lossy(&again).starts_with(&format!("{{\"format\":{},", royalesim::state::SNAPSHOT_FORMAT)));
    let l = load_2018(&again).unwrap();
    assert_eq!(l.state_hash(), s.state_hash());
    // The remap put every card where its NAME says: Blue's hand slot 3 is still Skeletons
    // (also asserted by the replay test), and no board unit resolved to a spell.
    assert!(s.entities().all(|e| s.cards().get(e.card_idx).spell.is_none()), "a migrated board unit points at a spell card");
}

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
