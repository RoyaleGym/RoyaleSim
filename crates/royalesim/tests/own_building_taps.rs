//! A tap on an own building (placement.TROOP_BUILDING_TAPS), the Heal's point (placement.SPELL_AS_DEPLOY_TAPS) and a
//! scenario building laid from its tap (`BattleState::spawn_unit_tapped`), read off `resolve_point`, `spawn_unit` and
//! `spawn_unit_tapped` (the play path and the replay harness share that resolution).
//!
//! THE LAWS, measured on client 15.535.29 by parity (round 8, item 8, over all 1,237 scenario fixtures):
//!   * 86 of 86 taps whose snapped tile shares area with the box of an alive own Cannon land where the crown-tower
//!     relocation puts them: 58 on the -x tile, (14500, 2500) -> (13500, 2500) with a Cannon on (15500, 2500); 28 one
//!     tile past the box toward the river, (14500, 4500), where a second Cannon on (12500, 2500) covers the -x tile.
//!     Side 1 is the same in the rotation, (14500, 29500) -> (13500, 29500) and (14500, 27500). Both
//!     placement.TOWER_TAP_PUSH arms give those points. 10 of 10 taps whose tile only touches an own building's box
//!     are laid as tapped: Skeletons on (14500, 4500), flush above the Cannon; side 1's Goblins on (3500, 18500) and
//!     Minions on (3500, 19500), beside a Tesla on (5000, 20000). The client 16.402 corpus has one such tap, on a 2x2
//!     box: 20260919-144043 (A and B), side 0's Golem tapped on (3500, 1500) with an own Tesla on (3000, 2000) stands
//!     on (4499, 1500), the ring search's (4500, 1500) with the ground deploy point.
//!   * 15 of those taps are Heal casts, which released their Heal Spirit where the troops went.
//!   * A scenario building tapped on a standing Cannon (15500, 2500) stood on (12500, 2500); a Cannon tapped at (9000,
//!     14500), its box over the river, on (8500, 13500); side 1's Tesla tapped at (5500, 20500) on (5000, 20000); a
//!     Goblin Drill tapped at (9500, 21500) surfaced on (9000, 21000) (sweep-GoblinDrill).
//!
//! WHAT IS PINNED:
//!   1. TROOP_BUILDING_TAPS = as_tower_tap, both seats, both TOWER_TAP_PUSH arms, both TAP_SNAP arms: the two measured
//!      landings, the Spirits laid as a tap on the landing is, and the flush taps not moved; not_relocated lays every
//!      box tap on its tap (the arms differ on every box tap);
//!   2. SPELL_AS_DEPLOY_TAPS = troop_relocation (with as_tower_tap): a Heal on the Cannon releases its Heal Spirit on
//!      the two measured landings; spell_point leaves it on the tap; the Log, which may stand on buildings, is not read;
//!   3. `spawn_unit_tapped`: the two Cannons' and the Drill's measured points under both placement.SNAP_EVEN_CORNER
//!      arms, the Tesla's under absolute, the shipped arm since the 2026-09-28 placement batch (placer_frame puts it
//!      on (6000, 21000), the placer's corner, not the client's); `spawn_unit` puts each where it was tapped;
//!   4. the shipped values are the measured arms, as_tower_tap and troop_relocation (the 2026-09-28 placement batch);
//!   5. a tap whose relocation falls to the ring search (the axis push into the river) goes to the fitting tile nearest
//!      the RAW tap: measured on client 15.535.29 (Oracle's sp-esk-bank-*), Evo Skeletons tapped on an own Elixir
//!      Collector's box by the river: blue's raw (14639, 14500) by a Collector on (14500, 13500) laid on (16500, 14500),
//!      red's raw (3360, 17500) by one on (3500, 18500) on (1500, 17500); the snapped tile's two candidates tie at 2000.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! own_building_taps`), each reddening exactly its test:
//!   * `troop_building_taps_touching` -- an own building's box blocks a tile that only touches it: (1) goes red.
//!   * `spell_as_deploy_taps_troops_only` -- no spell is placed as a troop: (2) goes red.
//!   * `spawn_unit_tapped_keeps_the_tap` -- `spawn_unit_tapped` keeps a building on its tap: (3) goes red.
//!   * `ring_nearest_snapped_tap` -- the ring's nearest measured from the snapped tap: (5) goes red.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, PlacementSnapEven, SpellAsDeployTaps, TapSnap, TowerTapPush, TroopBuildingTaps};
use royalesim::Team;

/// A point, native units, arena frame.
type P = (i32, i32);
/// A building put down before the tap: (its side, its card, its point).
type Standing = (Team, &'static str, P);
/// A scenario building row: (seat, the buildings standing, card, tap, where it stands under placement.SNAP_EVEN_CORNER =
/// placer_frame, under absolute).
type BuildingRow = (Team, Vec<Standing>, &'static str, P, P, P);

fn native(p: P) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The arms a scene runs under.
#[derive(Clone, Copy, Debug)]
struct Arms {
    building: TroopBuildingTaps,
    spell: SpellAsDeployTaps,
    push: TowerTapPush,
    snap: TapSnap,
    even: PlacementSnapEven,
}

const PUSHES: [TowerTapPush; 2] = [TowerTapPush::RingNearest, TowerTapPush::Client16402AxisPush];
const SNAPS: [TapSnap; 2] = [TapSnap::None, TapSnap::TileCentre];

/// A battle under `arms`, with `buildings` (team, card, point) put down by `spawn_unit` (a building stays where it was
/// put) and on the board.
fn board(arms: Arms, buildings: &[Standing]) -> BattleState {
    let mut cfg = config();
    cfg.calib.placement_troop_building_taps = arms.building;
    cfg.calib.placement_spell_as_deploy_taps = arms.spell;
    cfg.calib.placement_tower_tap_push = arms.push;
    cfg.calib.placement_tap_snap = arms.snap;
    cfg.calib.placement_snap_even = arms.even;
    let mut s = BattleState::new(1, cfg);
    for &(team, card, p) in buildings {
        s.spawn_unit(team, card, native(p), None).unwrap_or_else(|e| panic!("{card} at {p:?}: {e:?}"));
    }
    s.tick();
    for &(team, card, p) in buildings {
        let stands = s.entities().any(|e| e.team == team && s.cards().get(e.card_idx).name == card && e.pos == native(p));
        assert!(stands, "the scene drifted: no {card} of {team:?} stands on {p:?}");
    }
    s
}

/// Where `card` tapped by `team` at `tap` is resolved to, and the members `spawn_unit` lays for it, sorted.
fn tap(arms: Arms, buildings: &[Standing], team: Team, card: &str, at: P) -> (Vec2, Vec<Vec2>) {
    let mut s = board(arms, buildings);
    let idx = s.cards().index(card).unwrap_or_else(|| panic!("data: {card} loads"));
    let resolved = s.resolve_point(team, idx, native(at));
    let before = s.pending_spawns().len();
    s.spawn_unit(team, card, native(at), None).unwrap_or_else(|e| panic!("{card} at {at:?}: {e:?}"));
    let mut laid: Vec<Vec2> = s.pending_spawns()[before..].iter().map(|&(_, _, p)| p).collect();
    assert!(!laid.is_empty(), "{card} laid nothing");
    laid.sort_by_key(|p| (p.x, p.y));
    (resolved, laid)
}

fn cannon(team: Team, p: P) -> Standing {
    (team, "Cannon", p)
}

/// (seat, the Cannons standing, the tap, where the client laid it), arena frame.
fn box_taps() -> Vec<(Team, Vec<Standing>, P, P)> {
    vec![
        (Team::Blue, vec![cannon(Team::Blue, (15500, 2500))], (14500, 2500), (13500, 2500)),
        (Team::Blue, vec![cannon(Team::Blue, (15500, 2500)), cannon(Team::Blue, (12500, 2500))], (14500, 2500), (14500, 4500)),
        (Team::Red, vec![cannon(Team::Red, (15500, 29500))], (14500, 29500), (13500, 29500)),
        (Team::Red, vec![cannon(Team::Red, (15500, 29500)), cannon(Team::Red, (12500, 29500))], (14500, 29500), (14500, 27500)),
    ]
}

#[test]
fn a_troop_tap_on_an_own_cannon_goes_where_the_client_laid_it() {
    for push in PUSHES {
        for snap in SNAPS {
            let new = Arms { building: TroopBuildingTaps::AsTowerTap, spell: SpellAsDeployTaps::SpellPoint, push, snap, even: PlacementSnapEven::PlacerFrame };
            let old = Arms { building: TroopBuildingTaps::NotRelocated, ..new };
            for (team, cannons, at, client) in box_taps() {
                for card in ["IceSpirits", "FireSpirits", "Skeletons"] {
                    let (resolved, laid) = tap(new, &cannons, team, card, at);
                    assert_eq!(resolved, native(client), "{new:?} {card} of {team:?} tapped on {at:?}");
                    let (_, want) = tap(new, &cannons, team, card, client);
                    assert_eq!(laid, want, "{new:?} {card} of {team:?}: not laid as a tap on {client:?}");
                    // Not vacuous: the old arm lays the tap on the box.
                    let (kept, kept_laid) = tap(old, &cannons, team, card, at);
                    assert_eq!(kept, native(at), "{old:?} {card} of {team:?} moved the tap");
                    assert_ne!(kept_laid, laid, "{old:?} {card} of {team:?}: vacuous, the arms lay one formation");
                }
            }
            // A tile that only touches an own building's box is not on it: laid as tapped (the client's controls).
            let flush: [(Team, Vec<Standing>, &str, P); 3] = [
                (Team::Blue, vec![cannon(Team::Blue, (15500, 2500))], "Skeletons", (14500, 4500)),
                (Team::Red, vec![(Team::Red, "Tesla", (5000, 20000))], "Goblins", (3500, 18500)),
                (Team::Red, vec![(Team::Red, "Tesla", (5000, 20000))], "Minions", (3500, 19500)),
            ];
            for (team, standing, card, at) in flush {
                let (resolved, _) = tap(new, &standing, team, card, at);
                assert_eq!(resolved, native(at), "{new:?} {card} of {team:?} on the flush tile {at:?} moved");
            }
            // The 16.402 corpus on a 2x2 box: 20260919-144043 (A and B), side 0's Golem tapped on (3500, 1500) with an own
            // Tesla on (3000, 2000) stands on (4499, 1500), the tile (4500, 1500) with the ground deploy point.
            let tesla = [(Team::Blue, "Tesla", (3000, 2000))];
            let (resolved, laid) = tap(new, &tesla, Team::Blue, "Golem", (3500, 1500));
            assert_eq!(resolved, native((4500, 1500)), "{new:?} the corpus Golem");
            if snap == TapSnap::TileCentre {
                assert_eq!(laid, vec![native((4499, 1500))], "{new:?} the corpus Golem's creation point");
            }
            let (kept, _) = tap(old, &tesla, Team::Blue, "Golem", (3500, 1500));
            assert_eq!(kept, native((3500, 1500)), "{old:?} moved the corpus Golem");
        }
    }
}

/// (5), under the shipped arms (a precondition below).
#[test]
fn a_ring_relocation_goes_to_the_tile_nearest_the_raw_tap() {
    let c = config();
    assert_eq!(
        (c.calib.placement_troop_building_taps, c.calib.placement_tower_tap_push, c.calib.placement_tap_snap),
        (TroopBuildingTaps::AsTowerTap, TowerTapPush::Client16402AxisPush, TapSnap::TileCentre),
        "the shipped arms this test reads"
    );
    let shipped = Arms {
        building: c.calib.placement_troop_building_taps,
        spell: c.calib.placement_spell_as_deploy_taps,
        push: c.calib.placement_tower_tap_push,
        snap: c.calib.placement_tap_snap,
        even: c.calib.placement_snap_even,
    };
    let rows: [(Team, P, P, P); 2] = [(Team::Blue, (14500, 13500), (14639, 14500), (16500, 14500)), (Team::Red, (3500, 18500), (3360, 17500), (1500, 17500))];
    for (team, collector, at, client) in rows {
        let standing = [(team, "Elixir Collector", collector)];
        let (resolved, _) = tap(shipped, &standing, team, "IceSpirits", at);
        assert_eq!(resolved, native(client), "{team:?} tapped on {at:?} by an own Collector on {collector:?}");
        // Not vacuous: the snapped tile's two candidates stand equally far.
        let snapped = (at.0.div_euclid(1000) * 1000 + 500, at.1.div_euclid(1000) * 1000 + 500);
        let other = (2 * snapped.0 - client.0, client.1);
        assert_eq!((snapped.0 - client.0).abs(), (snapped.0 - other.0).abs(), "the scene drifted: no tie from the snapped tile {snapped:?}");
    }
}

#[test]
fn a_heal_cast_on_an_own_cannon_releases_its_spirit_where_the_troops_go() {
    for push in PUSHES {
        for snap in SNAPS {
            let new = Arms { building: TroopBuildingTaps::AsTowerTap, spell: SpellAsDeployTaps::TroopRelocation, push, snap, even: PlacementSnapEven::PlacerFrame };
            let old = Arms { spell: SpellAsDeployTaps::SpellPoint, ..new };
            for (team, cannons, at, client) in box_taps().into_iter().filter(|r| r.0 == Team::Blue) {
                let (resolved, laid) = tap(new, &cannons, team, "Heal", at);
                assert_eq!(resolved, native(client), "{new:?} Heal tapped on {at:?}");
                assert_eq!(laid, vec![native(client)], "{new:?}: the Heal Spirit is not released on {client:?}");
                // Not vacuous: spell_point leaves the cast on the Cannon's box.
                let (kept, kept_laid) = tap(old, &cannons, team, "Heal", at);
                assert_eq!(kept, native(at), "{old:?} moved the Heal");
                assert_ne!(kept_laid, laid, "{old:?}: vacuous, the arms release the spirit on one point");
                // The Log may stand on a building (CanPlaceOnBuildings): not placed as a troop.
                let (log, _) = tap(new, &cannons, team, "Log", at);
                assert_eq!(log, native(at), "{new:?} moved the Log");
            }
        }
    }
}

#[test]
fn a_scenario_building_is_laid_where_a_play_tapped_there_lands() {
    // (seat, standing, card, tap, where it stands under placement.SNAP_EVEN_CORNER = placer_frame, under absolute).
    // The Cannons (3x3) take a tile centre under both. Side 1's Tesla (2x2) takes a tile corner: the client's is
    // (5000, 20000), the arena's lower-left corner of the tapped tile, which only the absolute arm takes (shipped since
    // the 2026-09-28 placement batch); placer_frame takes the placer's lower-left, (6000, 21000). That key's promotion
    // case.
    let rows: [BuildingRow; 4] = [
        (Team::Blue, vec![cannon(Team::Blue, (15500, 2500))], "Cannon", (15500, 2500), (12500, 2500), (12500, 2500)),
        (Team::Blue, vec![], "Cannon_EV1", (9000, 14500), (8500, 13500), (8500, 13500)),
        (Team::Red, vec![], "Tesla", (5500, 20500), (6000, 21000), (5000, 20000)),
        // A card that tunnels into a building surfaces on its building's 2x2 footprint (placement.SPAWN_PATHFIND_DESTINATION).
        (Team::Blue, vec![], "GoblinDrill", (9500, 21500), (9000, 21000), (9000, 21000)),
    ];
    for even in [PlacementSnapEven::PlacerFrame, PlacementSnapEven::Absolute] {
        let arms = Arms { building: TroopBuildingTaps::NotRelocated, spell: SpellAsDeployTaps::SpellPoint, push: TowerTapPush::RingNearest, snap: TapSnap::None, even };
        for (team, standing, card, at, placer, absolute) in rows.clone() {
            let want = if even == PlacementSnapEven::Absolute { absolute } else { placer };
            let mut s = board(arms, &standing);
            let before = s.pending_spawns().len();
            s.spawn_unit_tapped(team, card, native(at), None).unwrap_or_else(|e| panic!("{card} tapped at {at:?}: {e:?}"));
            let laid: Vec<Vec2> = s.pending_spawns()[before..].iter().map(|&(_, _, p)| p).collect();
            assert_eq!(laid, vec![native(want)], "{even:?}: {card} of {team:?} tapped at {at:?}");
            // Not vacuous: spawn_unit keeps a building where it was put (the second Cannon stacked on the first).
            let mut kept = board(arms, &standing);
            let before = kept.pending_spawns().len();
            kept.spawn_unit(team, card, native(at), None).unwrap_or_else(|e| panic!("{card} at {at:?}: {e:?}"));
            let stacked: Vec<Vec2> = kept.pending_spawns()[before..].iter().map(|&(_, _, p)| p).collect();
            assert_eq!(stacked, vec![native(at)], "spawn_unit moved {card}");
        }
    }
    let arms = Arms { building: TroopBuildingTaps::NotRelocated, spell: SpellAsDeployTaps::SpellPoint, push: TowerTapPush::RingNearest, snap: TapSnap::None, even: PlacementSnapEven::PlacerFrame };
    // A troop goes down as through spawn_unit.
    let (_, want) = tap(arms, &[], Team::Blue, "Knight", (6500, 8500));
    let mut s = board(arms, &[]);
    s.spawn_unit_tapped(Team::Blue, "Knight", native((6500, 8500)), None).expect("a Knight");
    assert_eq!(s.pending_spawns().iter().map(|&(_, _, p)| p).collect::<Vec<_>>(), want);
}

#[test]
fn the_shipped_values_are_the_measured_arms() {
    let c = Calib::shipped();
    assert_eq!(c.placement_troop_building_taps, TroopBuildingTaps::AsTowerTap);
    assert_eq!(c.placement_spell_as_deploy_taps, SpellAsDeployTaps::TroopRelocation);
}
