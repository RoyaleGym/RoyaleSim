//! THE 2026 PATHFINDER, against the paths recorded on client 15.535.29.
//!
//! The fixture `fixtures/oracle2026/first_paths.json` holds 37 first paths the LIVE
//! 2026 game published (7 walk, 16 lane-sweep, 12 building, 2 repath), each with the
//! position the pathfinder was called from, the target, the mover's reach and the
//! occluders that were on the board. Everything here is scored against those.
//!
//! THE FIXTURE IS GENERATED, by `tools/make_oracle2026_fixture.py`, and `--check`
//! on that script re-derives every field from the trace corpus and fails on a
//! difference. That matters more than it looks: a hand-maintained fixture drifted
//! here once, carrying a deploy COMMAND's coordinate as the occluding building's
//! centre rather than the position the trace records (the game snaps a deploy to a
//! tile), and three cases then looked like the occlusion model refusing a path the
//! game took. Regenerating from the trace is the only way that stays honest.
//!
//! WHAT IS GATED, and why only this much. docs/pathfinder-spec.md lists the
//! gates in increasing strictness; exact node sequences are not gated here, because
//! this model reaches only 13 of 76 distinct experiments (the search measured on
//! client 16.402, path16402.rs, is the one that reproduces them):
//!
//!   G1 COST      the engine's path costs exactly what the recorded path costs, on
//!                the engine's own grid with the engine's own occluders. This is the
//!                tie-break-INDEPENDENT gate and it is the one that catches a wrong
//!                cost constant, a wrong diagonal weight or a wrong occlusion box.
//!   G3 LEGALITY  the recorded path is legal under the engine's model: every
//!                step 8-connected, no interior cell occluded, no cell impassable.
//!   G4 GOAL      the reach rule holds two-sided on the recorded list -- the goal is
//!                in reach and its predecessor is not.
//!   G5 SHAPE     the engine's path is 8-connected and never revisits a cell.
//!
//! NOT gated: which of several equally-cheap paths comes back (spec 3.6).
//!
//! The reach in the fixture is the LIVE card data's (Range + the mover's own
//! CollisionRadius from csv_logic/characters/*.toml of 15.535.29). cards.json is
//! now the 15.535 vintage too and `the_fixtures_reach_is_the_card_datas` pins that
//! the two agree on every card here (the 2018 file parted on the Giant, the Knight,
//! the Mini P.E.K.K.A and the Royal Giant). Passing the reach in still keeps this a
//! test of the PATHFINDER.
#![allow(unexpected_cfgs)]

use royalesim::arena::Arena;
use royalesim::fixed::{milli, Vec2};
use royalesim::path::{FrameWorld, NavRequest};
use royalesim::path2026::{self, CostField, IMPASSABLE, OCCLUDED};
use royalesim::state::Calib;

const FIXTURE: &str = include_str!("fixtures/oracle2026/first_paths.json");

#[derive(serde::Deserialize)]
struct Case {
    /// Does the measured occlusion box ADMIT this recorded path -- is every INTERIOR
    /// cell of the path the game published outside every friendly box? Derived by
    /// the generator from the trace, and true on all 37, which is the claim
    /// `the_occlusion_box_admits_every_path_the_game_took` asserts.
    occlusion_model_admits_this_path: bool,
    trace: String,
    card: String,
    start_native: [i32; 2],
    target_native: [i32; 2],
    reach_native: i32,
    occluders_native: Vec<[i32; 3]>,
    oracle_cells_goal_first: Vec<[i32; 2]>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

fn cases() -> Vec<Case> {
    serde_json::from_str::<Fixture>(FIXTURE).expect("fixture parses").cases
}

fn sub(v: i32) -> i32 {
    milli(v)
}

/// The cost field the engine would plan on for this case.
fn field(arena: &Arena, calib: &Calib, c: &Case) -> CostField {
    let mut f = CostField::terrain(arena, calib);
    for o in &c.occluders_native {
        f.occlude(arena, calib, Vec2::new(sub(o[0]), sub(o[1])), sub(o[2]));
    }
    f
}

/// Cost of walking `cells` (start-first) under `f`, paying for the cell ENTERED.
/// `None` when a step is not 8-connected or enters an impassable cell that is not
/// the goal.
///
/// A BUILDING BOX IS NOT REFUSED HERE ANY MORE: under the measured
/// `OCCLUDED_CELL_TREATMENT = cost_50` it is an ordinary price (50, x DIAGONAL_COST_
/// RATIO on a diagonal) that `f.at` already carries, which is exactly the live
/// finding -- 97 of 785 live paths cross a box interior and a block makes them
/// infeasible. What is still refused is water (never, not even as the goal) and the
/// bit-16 block outside any box (goal only, path2026.rs `OCCLUDED`). That the
/// engine's paths do not NEED to cross a box on this corpus is asserted separately,
/// by `the_occlusion_box_admits_every_path_the_game_took`.
fn cost_of(f: &CostField, calib: &Calib, cells: &[(i32, i32)]) -> Option<i64> {
    let mut total = 0i64;
    for (k, (&(c0, r0), &(c1, r1))) in cells.iter().zip(cells.iter().skip(1)).enumerate() {
        let (dc, dr) = ((c1 - c0).abs(), (r1 - r0).abs());
        if dc > 1 || dr > 1 || dc + dr == 0 {
            return None;
        }
        let mut step = f.at(f.idx(c1, r1));
        if step == IMPASSABLE {
            return None; // water: refused even as the goal (WATER_RULE_GROUND)
        }
        if step == OCCLUDED {
            if k + 2 != cells.len() {
                return None;
            }
            step = calib.path_cost_default;
        }
        if dc == 1 && dr == 1 {
            step = step * calib.diag_num / calib.diag_den;
        }
        total += step as i64;
    }
    Some(total)
}

/// The engine's path between the RECORDED PATH'S OWN ENDPOINTS.
///
/// WHY NOT FROM THE UNIT'S CELL: the recorded `path_nodes` is a POST-TICK snapshot
/// and the unit consumed its first node in the same tick it planned (spec 5.2 /
/// 7.5), so the list starts one or two cells ahead of where the unit stood. Scoring
/// between the list's own endpoints makes G1 independent of both the dropped prefix
/// and the goal rule -- which is exactly how the cost model was fitted in the first
/// place.
fn plan_between(arena: &Arena, calib: &Calib, c: &Case, from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    let mut cc = Case {
        trace: c.trace.clone(),
        card: c.card.clone(),
        start_native: [0, 0],
        target_native: [0, 0],
        // A reach of one subtile admits exactly the target's own cell: the next
        // cell centre is a whole cell (9000 subtiles) away.
        reach_native: 0,
        occluders_native: c.occluders_native.clone(),
        oracle_cells_goal_first: Vec::new(),
        occlusion_model_admits_this_path: true,
    };
    let start = arena.half_to_subtile_center(from.0, from.1);
    let goal = arena.half_to_subtile_center(to.0, to.1);
    cc.start_native = [start.x / 18, start.y / 18];
    cc.target_native = [goal.x / 18, goal.y / 18];
    plan_with_reach(arena, calib, &cc, 1)
}

fn plan(arena: &Arena, calib: &Calib, c: &Case) -> Vec<(i32, i32)> {
    plan_with_reach(arena, calib, c, sub(c.reach_native))
}

fn plan_with_reach(arena: &Arena, calib: &Calib, c: &Case, reach: i32) -> Vec<(i32, i32)> {
    // The occluders enter through the cost field, so the planner is handed a world
    // with no obstacle list and a pre-stamped field would not fit `plan_cells`'s
    // signature -- instead the case's occluders are passed as FrameWorld obstacles.
    let obstacles: Vec<royalesim::path::Obstacle> = c
        .occluders_native
        .iter()
        .enumerate()
        .map(|(i, o)| royalesim::path::Obstacle {
            id: royalesim::EntityId { index: i as u32, generation: 0 },
            shape: royalesim::arena::Shape::Circle { c: Vec2::new(sub(o[0]), sub(o[1])), r: sub(o[2]) },
            radius: sub(o[2]),
            key: (0, 0, 0, 0, i as u32),
            ally: true,
        })
        .collect();
    let world = FrameWorld { arena, obstacles: &obstacles };
    let req = NavRequest {
        #[cfg(clash_plant = "reflection_bridge_tie")]
        red: false,
        team: royalesim::Team::Blue,
        pos: Vec2::new(sub(c.start_native[0]), sub(c.start_native[1])),
        goal: Vec2::new(sub(c.target_native[0]), sub(c.target_native[1])),
        radius: 0,
        sight: 0,
        step: 0,
        reach,
        flying: false,
        target_flying: false,
        jumper: jumps(&c.card),
        ignore: None,
    };
    let (cells, ok) = path2026::plan_cells(&world, calib, &req);
    assert!(ok, "{}: no path at all", c.trace);
    cells
}

#[test]
fn the_fixtures_reach_is_the_card_datas() {
    // The fixture carries the live client's own reach; data/derived/cards.json (the
    // 15.535 vintage) must give the same Range + CollisionRadius for every card the
    // traces recorded -- the 2018 file disagreed on four of them, which is why the walk
    // gate (tools/oracle_diff.py) scored 19 of 21 first-path goal cells: the Mini
    // P.E.K.K.A (1050 + 450) and the Royal Giant (6500 + 750) aimed at other cells.
    let db = common_cards();
    let mut seen = std::collections::BTreeMap::new();
    for c in cases() {
        let idx = db.index(&c.card).unwrap_or_else(|| panic!("{}: {} is not simulable", c.trace, c.card));
        let card = db.get(idx);
        let mine = (card.range + card.collision_radius) / royalesim::fixed::SUBTILE_PER_MILLITILE;
        assert_eq!(mine, c.reach_native, "{} ({}): cards.json Range + CollisionRadius {mine} vs the live reach {}", c.trace, c.card, c.reach_native);
        seen.insert(c.card.clone(), c.reach_native);
    }
    assert!(seen.len() >= 6 && seen.contains_key("MiniPekka") && seen.contains_key("RoyalGiant"), "vacuous: {seen:?}");
}

fn common_cards() -> royalesim::card::CardDb {
    let db = royalesim::card::CardDb::load_repo().expect("data/derived/cards.json (run tools/extract_cards.py)");
    assert_eq!(db.source, royalesim::card::CardSource::DerivedJson);
    db
}

/// Whether the mover of a case is JumpEnabled (cards.json `jump` block): the search
/// prices water at PATHFINDING_COSTS.water for such a mover (path16402.rs
/// `cell_cost_for`), so the request carries the card's flag exactly as state.rs
/// builds it -- the offline corpus has one such mover, the Hog Rider of
/// walk/HogRider_x3.5_y8.5_seed2 (its route stays on the bridge column either way,
/// which is why the gates never saw the flag).
fn jumps(card: &str) -> bool {
    use std::sync::OnceLock;
    static DB: OnceLock<royalesim::card::CardDb> = OnceLock::new();
    let db = DB.get_or_init(common_cards);
    match db.index(card) {
        Some(idx) => db.get(idx).jump.is_some(),
        // the spawned troops of the live corpus (HutSpearGoblin, TombSkeleton, the
        // hero form) are not cards: none jumps
        None => false,
    }
}

#[test]
fn g1_engine_path_costs_exactly_what_the_oracle_path_costs() {
    let arena = Arena::shipped();
    let calib = Calib::shipped();
    let mut checked = 0;
    for c in cases() {
        assert!(c.occlusion_model_admits_this_path, "{}: see the_occlusion_box_admits_every_path_the_game_took", c.trace);
        let f = field(&arena, &calib, &c);
        let theirs: Vec<(i32, i32)> = c.oracle_cells_goal_first.iter().rev().map(|v| (v[0], v[1])).collect();
        let (from, to) = (theirs[0], *theirs.last().unwrap());
        let mut mine: Vec<(i32, i32)> = plan_between(&arena, &calib, &c, from, to).into_iter().rev().collect();
        mine.insert(0, from);
        let their_cost = cost_of(&f, &calib, &theirs)
            .unwrap_or_else(|| panic!("{}: the RECORDED path is illegal under this model", c.trace));
        let my_cost = cost_of(&f, &calib, &mine)
            .unwrap_or_else(|| panic!("{}: the engine's path is illegal under its own model", c.trace));
        assert_eq!(
            my_cost, their_cost,
            "{} ({}): engine path costs {my_cost}, recorded {their_cost}\n  engine {mine:?}\n  recorded {theirs:?}",
            c.trace, c.card
        );
        checked += 1;
    }
    // 33 before the fixture was regenerated from the traces: the four cases whose
    // occluder came from the deploy command were either skipped outright or scored
    // against a phantom box. Nothing is skipped now.
    assert_eq!(checked, 37, "every case must be scored");
}

#[test]
fn g4_the_goal_rule_holds_two_sided_on_every_oracle_path() {
    // spec 6.1: the goal cell's CENTRE is within Range + own CollisionRadius of the
    // TARGET'S CENTRE, and its predecessor's is not. 150/150 in the measurement;
    // these 36 are the subset the fixture carries.
    let arena = Arena::shipped();
    for c in cases() {
        let target = Vec2::new(sub(c.target_native[0]), sub(c.target_native[1]));
        let reach = sub(c.reach_native) as i64;
        let d2 = |cell: &[i32; 2]| {
            arena.half_to_subtile_center(cell[0], cell[1]).dist2(target)
        };
        let goal = &c.oracle_cells_goal_first[0];
        assert!(d2(goal) <= reach * reach, "{}: goal cell {goal:?} is out of reach", c.trace);
        if let Some(prev) = c.oracle_cells_goal_first.get(1) {
            assert!(d2(prev) > reach * reach, "{}: the goal's predecessor {prev:?} is already in reach", c.trace);
        }
    }
}

#[test]
fn g3_the_oracle_paths_are_legal_under_the_engines_grid() {
    // Every step 8-connected, no interior cell occluded or impassable. This is what
    // fails first if the occlusion box is the wrong shape: a CLOSED box blocks 57
    // cells the recorded paths use, and a mover pad of one native unit blocks
    // 155 (calibration pathfinding.OCCLUSION_MODEL).
    let arena = Arena::shipped();
    let calib = Calib::shipped();
    for c in cases() {
        let f = field(&arena, &calib, &c);
        let cells: Vec<(i32, i32)> = c.oracle_cells_goal_first.iter().rev().map(|v| (v[0], v[1])).collect();
        assert!(cost_of(&f, &calib, &cells).is_some(), "{}: illegal under the engine's grid: {cells:?}", c.trace);
    }
}

#[test]
fn g5_the_engine_path_is_eight_connected_and_never_revisits_a_cell() {
    let arena = Arena::shipped();
    let calib = Calib::shipped();
    for c in cases() {
        let cells = plan(&arena, &calib, &c);
        let mut seen = std::collections::HashSet::new();
        for w in cells.windows(2) {
            let (dc, dr) = ((w[0].0 - w[1].0).abs(), (w[0].1 - w[1].1).abs());
            assert!(dc <= 1 && dr <= 1 && dc + dr > 0, "{}: step {:?} -> {:?}", c.trace, w[1], w[0]);
        }
        for cell in &cells {
            assert!(seen.insert(*cell), "{}: cell {cell:?} appears twice", c.trace);
        }
        assert!(!cells.is_empty(), "{}: empty path", c.trace);
    }
}

#[test]
fn a_box_is_priced_and_only_the_goal_may_sit_on_the_bit_sixteen_block() {
    // Spec 4.6's goal-cell
    // exemption does not apply to building boxes, because they are no longer
    // refused (LIVE 16.402, calibration OCCLUDED_CELL_TREATMENT = cost_50: 97 of 785
    // live paths cross a box interior, and the exemption scores 12 failures against
    // 6 without it). What the offline corpus saw -- four of 150 goal cells inside a
    // tower box, and 81 published paths ending inside one -- is now explained by the
    // PRICE rather than by an exemption, and the load-bearing half survives
    // unchanged: on this corpus no INTERIOR cell of an engine path needs a box.
    //
    // The exemption itself survives only on the bit-16 block, which is what keeps a
    // short-reach card able to path to a king tower at all, so that half is asserted
    // here too: an OCCLUDED cell may be the goal and nothing else.
    let arena = Arena::shipped();
    let calib = Calib::shipped();
    let (mut ended_inside, mut crossed_inside) = (0, 0);
    for c in cases() {
        let f = field(&arena, &calib, &c);
        let cells = plan(&arena, &calib, &c);
        for (i, cell) in cells.iter().enumerate() {
            let idx = f.idx(cell.0, cell.1);
            assert_ne!(f.at(idx), IMPASSABLE, "{}: cell {cell:?} is water", c.trace);
            assert!(f.at(idx) != OCCLUDED || i == 0, "{}: interior cell {cell:?} is on the bit-16 block", c.trace);
            if f.in_building_box(idx) {
                if i == 0 {
                    ended_inside += 1;
                } else {
                    crossed_inside += 1;
                }
            }
        }
    }
    assert_eq!(crossed_inside, 0, "no engine path on this corpus should need to cross a box interior");
    println!("engine paths ending inside an occlusion box: {ended_inside}/37");
}

#[test]
fn the_occlusion_box_admits_every_path_the_game_took() {
    // THE MEASURED SHAPE, re-derived here from the boxes rather than re-read off the
    // fixture's own flag: no INTERIOR cell of any of the 37 published paths lies
    // inside a friendly building's half-open AABB at its CollisionRadius, with no
    // mover pad. That is what "0 suboptimal / 0 infeasible over the corpus" means,
    // and it now holds on the four late traces too -- three of them used to look
    // like refusals only because the fixture put the occluder at the deploy
    // command's coordinate instead of where the building stands (the game snaps a
    // deploy to a tile: cannon_dx-0.5 commanded 3000 and stands at 3500, a tile
    // centre, squarely inside the measurement's stated domain). A Tesla's radius is
    // 500 in csv_logic, not the 600 that was assumed.
    // ASKED THROUGH `in_building_box`, NOT `refused`. Under the measured `cost_50` a
    // box refuses nothing at all, so `refused` would pass this vacuously -- and a
    // vacuous shape gate is how a wrong box gets in. The SHAPE claim is independent
    // of what the shape costs, and this is the assertion that keeps it so.
    let arena = Arena::shipped();
    let calib = Calib::shipped();
    let mut checked = 0;
    for c in cases() {
        let f = field(&arena, &calib, &c);
        for cell in c.oracle_cells_goal_first.iter().skip(1) {
            // skip(1): 81 published paths END inside a box (the goal cell). What may
            // not happen is an INTERIOR cell inside one -- that is the half the
            // measured shape rests on, and it holds on the live corpus too: the 97
            // live box crossings are all the cell before the goal, inside the ENEMY
            // king's box, which no case here contains.
            assert!(
                !f.in_building_box(f.idx(cell[0], cell[1])),
                "{}: the recorded unit walked through {cell:?}, which the occlusion box claims",
                c.trace
            );
        }
        assert!(c.occlusion_model_admits_this_path, "{}: the fixture's own flag disagrees", c.trace);
        checked += 1;
    }
    assert_eq!(checked, 37);
}

#[test]
fn the_half_tile_cannon_offsets_are_duplicate_experiments() {
    // THE BUILDING CORPUS IS 10 DISTINCT BOARDS, NOT 12. The game snaps a deploy to a
    // tile, so `cannon_dx-0.5` (commanded 3000) and `cannon_dx+0.5` (commanded 4000)
    // land on the SAME cannon positions as `cannon_dx+0.0` (3500) and
    // `cannon_dx+1.0` (4500). Pinned because the fixture used to carry the COMMAND
    // coordinate, which made the half-tile runs look like two extra experiments --
    // one of them a probe of a building on a cell boundary, which is precisely the
    // case the occlusion measurement scoped itself out of and which the corpus in
    // fact does not contain. A half-tile building sweep is still owed
    // (calibration pathfinding.OCCLUSION_MODEL `limits`).
    let by = |name: &str| cases().into_iter().find(|c| c.trace.ends_with(name)).expect(name).occluders_native;
    assert_eq!(by("cannon_dx-0.5_seed2.jsonl.gz"), by("cannon_dx+0.0_seed2.jsonl.gz"));
    assert_eq!(by("cannon_dx+0.5_seed2.jsonl.gz"), by("cannon_dx+1.0_seed2.jsonl.gz"));
    // WHERE THE DROPPED BUILDINGS ACTUALLY STAND. The measurement scoped itself to
    // "a building ON A TILE CENTRE", and the corpus is nearly all that -- but the two
    // Tesla runs are not: commanded (3500, 9500) and (2500, 9500), the Tesla stands
    // at (3000, 9000) and (2000, 9000), a tile CORNER. That is the one off-centre
    // occluder there is, and the measured half-open box admits both its paths
    // (`the_occlusion_box_admits_every_path_the_game_took`), so the scoping caveat is
    // narrower than it was thought to be. Crown towers are excluded here: a king sits
    // at (9000, 3000), also a corner, and its footprint is unmeasured either way.
    let tower_radius = [1000, 1400];
    let mut off_centre: Vec<String> = Vec::new();
    for c in cases() {
        for o in c.occluders_native.iter().filter(|o| !tower_radius.contains(&o[2])) {
            assert_eq!((o[0] % 500, o[1] % 500), (0, 0), "{}: occluder {o:?} is off the half-tile grid", c.trace);
            if (o[0] % 1000, o[1] % 1000) != (500, 500) {
                off_centre.push(c.trace.clone());
            }
        }
    }
    off_centre.sort();
    assert_eq!(
        off_centre,
        vec![
            "building_Giant/Tesla_dx+0.0_seed2.jsonl.gz".to_string(),
            "building_Giant/Tesla_dx-1.0_seed2.jsonl.gz".to_string(),
        ]
    );
}

#[test]
fn the_two_rival_occlusion_shapes_are_the_ones_the_corpus_refutes() {
    // The measurement's own discriminators, as a test rather than as prose: a CLOSED
    // box and a one-native-unit MOVER PAD each claim cells the recorded paths
    // walk through. Without this, "half-open, no pad" is an unfalsifiable label.
    let arena = Arena::shipped();
    let calib = Calib::shipped();
    let (mut closed_hits, mut pad_hits) = (0, 0);
    for c in cases() {
        for (dpad, counter) in [(0, &mut closed_hits), (1, &mut pad_hits)] {
            let mut f = CostField::terrain(&arena, &calib);
            for o in &c.occluders_native {
                // A CLOSED box is the half-open box of R+1 on the upper edge; the
                // pad variant is the half-open box of R + one native unit.
                f.occlude(&arena, &calib, Vec2::new(sub(o[0]), sub(o[1])), sub(o[2]) + if dpad == 0 { 1 } else { milli(1) });
            }
            for cell in c.oracle_cells_goal_first.iter().skip(1) {
                // `in_building_box`, not `refused`: under `cost_50` neither rival
                // shape refuses anything, and this test would report 0 and 0 and
                // assert nothing. "Claims" is a statement about the BOX.
                if f.in_building_box(f.idx(cell[0], cell[1])) {
                    *counter += 1;
                }
            }
        }
    }
    assert!(closed_hits > 0, "a closed box would have to claim cells the recorded paths use");
    assert!(pad_hits > 0, "a one-unit mover pad would have to claim cells the recorded paths use");
    println!("cells of the recorded paths claimed by a closed box: {closed_hits}; by a 1-unit pad: {pad_hits}");
}

// --------------------------------------------------------------------------------
// G6: THE SHIPPED SEARCH REPRODUCES THE PUBLISHED NODE SEQUENCE, every case
// --------------------------------------------------------------------------------

const CLIENT16402_FIXTURE: &str = include_str!("fixtures/oracle2026/client16402_first_paths.json");

#[derive(serde::Deserialize)]
struct Client16402Case {
    name: String,
    group: String,
    card: String,
    side: i32,
    start_native: [i32; 2],
    target_native: [i32; 2],
    reach_native: i32,
    /// `[x, y, CollisionRadius, owner side]` for every building of both sides. The owner
    /// is on the same 0/1 scale as `side`, so a building is friendly when the two match.
    occluders_native: Vec<[i32; 4]>,
    oracle_cells_goal_first: Vec<[i32; 2]>,
    moving_target: bool,
}

#[derive(serde::Deserialize)]
struct Client16402Fixture {
    cases: Vec<Client16402Case>,
}

/// Plan one fixture case THROUGH `path2026::plan_cells`, in the mover's team frame
/// exactly as state.rs builds the request (native side 1 defends high y, which is
/// the engine's Red), and hand back the cells in ABSOLUTE arena coordinates.
fn plan_client16402_case(arena: &Arena, calib: &Calib, c: &Client16402Case) -> (Vec<(i32, i32)>, bool) {
    plan_client16402_occluded(arena, calib, c, false)
}

/// `plan_client16402_case` with a choice of what is stamped: every building of both
/// sides (`friendly_only = false`, what the engine does), or only the mover's own (the
/// control arm).
fn plan_client16402_occluded(arena: &Arena, calib: &Calib, c: &Client16402Case, friendly_only: bool) -> (Vec<(i32, i32)>, bool) {
    use royalesim::Team;
    let team = if c.side == 1 { Team::Red } else { Team::Blue };
    let obstacles: Vec<royalesim::path::Obstacle> = c
        .occluders_native
        .iter()
        .filter(|o| !friendly_only || o[3] == c.side)
        .enumerate()
        .map(|(i, o)| {
            let centre = arena.to_frame(team, Vec2::new(sub(o[0]), sub(o[1])));
            royalesim::path::Obstacle {
                id: royalesim::EntityId { index: i as u32, generation: 0 },
                shape: royalesim::arena::Shape::Circle { c: centre, r: sub(o[2]) },
                radius: sub(o[2]),
                key: (0, 0, 0, 0, i as u32),
                ally: o[3] == c.side,
            }
        })
        .collect();
    let world = FrameWorld { arena, obstacles: &obstacles };
    let req = NavRequest {
        #[cfg(clash_plant = "reflection_bridge_tie")]
        red: team == Team::Red,
        team,
        pos: arena.to_frame(team, Vec2::new(sub(c.start_native[0]), sub(c.start_native[1]))),
        goal: arena.to_frame(team, Vec2::new(sub(c.target_native[0]), sub(c.target_native[1]))),
        radius: 0,
        sight: 0,
        step: 0,
        reach: sub(c.reach_native),
        flying: false,
        target_flying: false,
        jumper: jumps(&c.card),
        ignore: None,
    };
    let (cells, ok) = path2026::plan_cells(&world, calib, &req);
    // back from the frame to absolute cells
    let back = |(col, row): (i32, i32)| match team {
        Team::Blue => (col, row),
        Team::Red => (arena.cols - 1 - col, arena.rows - 1 - row),
    };
    (cells.into_iter().map(back).collect(), ok)
}

/// The one published sequence the engine does not reproduce (see the gate comment).
const KNOWN_DIVERGENCE: &str = "auto-20260920-072831-A";
#[test]
fn g6_the_client_search_reproduces_every_published_node_sequence() {
    // THE GATE THE SPEC SAID WAS OUT OF REACH (section 11: "exact node sequences are
    // NOT yet achievable"). It is, once the search is the client's: every first
    // path the live 16.402 client published (619 in the fixture) and every offline
    // 15.535 lane-sweep path (128), the exact list, goal first. The three skips are
    // the units chasing a moving troop (fixture `moving_target`), whose goal cell the
    // trace cannot recover; they are named here so a new one cannot hide behind them.
    // So 616 live cases are scored and 615 reproduce exactly.
    //
    // KNOWN DIVERGENCE, 1 case, named so it cannot hide and so a second one fails the
    // gate. It surfaces only on the larger fixture (747 cases, not 570).
    // The engine walks the lane straight where the client drifts one column sideways:
    //   auto-20260920-072831-A:8:Giant  client (6,16) (5,17) (5,18) (5,19) (5,20) (4,21)
    //                                   engine (6,16) (6,17) (6,18) (6,19) (6,20) (6,21)
    // Both reach the goal; they differ in WHICH lane route is taken, which is the open
    // A* expansion-order item (README, Status). A recorded gap, not an accepted one:
    // the assertion below fails if the set of diverging cases changes at all.
    let arena = Arena::shipped();
    let calib = Calib::shipped();
    assert_eq!(calib.path_search, royalesim::state::PathSearch::Client16402);
    let fx: Client16402Fixture = serde_json::from_str(CLIENT16402_FIXTURE).expect("client16402 fixture parses");
    let mut skipped = Vec::new();
    let (mut live, mut offline) = (0, 0);
    let mut failures = Vec::new();
    for c in &fx.cases {
        if c.moving_target {
            skipped.push(c.name.clone());
            continue;
        }
        let (mine, ok) = plan_client16402_case(&arena, &calib, c);
        assert!(ok, "{}: the search found no route", c.name);
        let oracle: Vec<(i32, i32)> = c.oracle_cells_goal_first.iter().map(|c| (c[0], c[1])).collect();
        // goal-first on both sides: the RECORDED list is shorter by the nodes the game
        // had already popped on the publishing tick, so compare its length from the head
        let hit = mine.len() >= oracle.len() && mine[..oracle.len()] == oracle[..];
        if hit {
            if c.group == "live_16402" {
                live += 1;
            } else {
                offline += 1;
            }
        } else {
            failures.push(format!("{}: recorded {:?}.. mine {:?}..", c.name, &oracle[..oracle.len().min(6)], &mine[..mine.len().min(6)]));
        }
    }
    assert_eq!(skipped, vec!["20260918-115249.b1:44:Goblins", "20260918-115249.b1:45:Goblins", "20260918-124946:69:Goblins"]);
    let diverging: Vec<&str> = failures.iter().map(|f| f.split(':').next().unwrap_or("")).collect();
    assert_eq!(diverging, vec![KNOWN_DIVERGENCE],
        "the set of diverging cases changed ({} now):
{}", failures.len(), failures.join("
"));
    println!("exact node sequences: live {live}, offline {offline}, skipped {}, diverging 1", skipped.len());
    assert!(live >= 615 && offline == 128, "the fixture shrank: live {live}, offline {offline}");
}

#[test]
fn g6_fixture_each_king_is_owned_by_the_side_whose_half_it_stands_in() {
    // The friendly-only arm below filters on the owner tag, so the tag is checked
    // against something the fixture maker never reads: where a king stands. Side 0
    // defends low y, so its king is at (9000, 3000); side 1's is at (9000, 29000). Both
    // kings are on the board on every published path. A swapped tag fails here, and so
    // does every building tagged with the mover's own side.
    let fx: Client16402Fixture = serde_json::from_str(CLIENT16402_FIXTURE).expect("client16402 fixture parses");
    let mut movers = [0usize; 2];
    for c in &fx.cases {
        assert!(c.side == 0 || c.side == 1, "{}: mover side {}", c.name, c.side);
        for o in &c.occluders_native {
            assert!(o[3] == 0 || o[3] == 1, "{}: building {o:?} has owner side {}", c.name, o[3]);
        }
        for (y, owner) in [(3000, 0), (29000, 1)] {
            let king = c
                .occluders_native
                .iter()
                .find(|o| o[0] == 9000 && o[1] == y && o[2] == 1400)
                .unwrap_or_else(|| panic!("{}: no king at (9000, {y})", c.name));
            assert_eq!(king[3], owner, "{}: the king at (9000, {y}) is tagged side {}", c.name, king[3]);
        }
        movers[c.side as usize] += 1;
    }
    assert!(movers[0] > 0 && movers[1] > 0, "vacuous: movers of one side only {movers:?}");
}

#[test]
fn g6_friendly_only_occlusion_is_a_control_the_published_paths_refute() {
    // THE CONTROL ARM for the side-blind stamping (calibration
    // pathfinding.PATHFINDING_FRIENDLYONLY_OCCLUSIONS): the gate's cases, planned
    // twice, once with every building of both sides stamped and once with only the
    // mover's own. If the published paths came out as well without the enemy's
    // buildings, the corpus could not tell the two apart, and "side-blind" would be a
    // label rather than a measurement. Scored exactly as the gate is.
    //
    // Three claims, each with a defect it catches:
    //   live      friendly-only reproduces FEWER paths (the enemy's buildings matter);
    //   offline   the lane sweep scores the same either way: its walkers never come near
    //             an enemy building, so a filter that kept the ENEMY's instead of the
    //             mover's own would show up here;
    //   subset    friendly-only reproduces no path the side-blind arm misses (stamping
    //             the enemy's buildings breaks nothing).
    let arena = Arena::shipped();
    let calib = Calib::shipped();
    assert_eq!(calib.path_search, royalesim::state::PathSearch::Client16402);
    let fx: Client16402Fixture = serde_json::from_str(CLIENT16402_FIXTURE).expect("client16402 fixture parses");
    let (mut n_live, mut n_offline) = (0, 0);
    let (mut blind_live, mut blind_offline) = (0, 0);
    let (mut own_live, mut own_offline) = (0, 0);
    let mut own_only_hits = Vec::new();
    for c in fx.cases.iter().filter(|c| !c.moving_target) {
        let oracle: Vec<(i32, i32)> = c.oracle_cells_goal_first.iter().map(|c| (c[0], c[1])).collect();
        let exact = |mine: &[(i32, i32)]| mine.len() >= oracle.len() && mine[..oracle.len()] == oracle[..];
        let (blind, _) = plan_client16402_occluded(&arena, &calib, c, false);
        let (own, _) = plan_client16402_occluded(&arena, &calib, c, true);
        let (b, o) = (exact(&blind[..]), exact(&own[..]));
        let live = c.group == "live_16402";
        if live {
            n_live += 1;
            blind_live += b as usize;
            own_live += o as usize;
        } else {
            n_offline += 1;
            blind_offline += b as usize;
            own_offline += o as usize;
        }
        if o && !b {
            own_only_hits.push(c.name.clone());
        }
    }
    println!(
        "friendly-only occlusion: live {own_live}/{n_live}, offline {own_offline}/{n_offline} exact \
         (every building stamped: live {blind_live}/{n_live}, offline {blind_offline}/{n_offline})"
    );
    assert!(n_live > 0 && n_offline > 0, "vacuous: live {n_live}, offline {n_offline}");
    assert!(own_live < blind_live, "friendly-only must lose on the live paths: {own_live} vs {blind_live}");
    assert_eq!(own_offline, blind_offline, "the lane sweep cannot tell the arms apart, so they must agree there");
    assert!(own_only_hits.is_empty(), "friendly-only reproduces paths the side-blind arm misses: {own_only_hits:?}");
}

#[test]
fn g6_the_trace_fitted_arm_is_still_runnable_and_still_wrong_about_the_order() {
    // The refuted arm must keep running under its own key, and must NOT silently
    // have become exact: if it ever scores 100 % something is wrong with the gate.
    let arena = Arena::shipped();
    let mut calib = Calib::shipped();
    calib.path_search = royalesim::state::PathSearch::TraceFittedAstar;
    let fx: Client16402Fixture = serde_json::from_str(CLIENT16402_FIXTURE).expect("client16402 fixture parses");
    let mut hits = 0;
    let mut n = 0;
    for c in fx.cases.iter().filter(|c| c.group == "offline_15535_lane_sweep") {
        n += 1;
        let (mine, ok) = plan_client16402_case(&arena, &calib, c);
        assert!(ok, "{}: the trace-fitted search found no route", c.name);
        let oracle: Vec<(i32, i32)> = c.oracle_cells_goal_first.iter().map(|c| (c[0], c[1])).collect();
        if mine.len() >= oracle.len() && mine[..oracle.len()] == oracle[..] {
            hits += 1;
        }
    }
    println!("trace-fitted arm, offline lane sweep: {hits}/{n} exact");
    assert!(hits < n, "the trace-fitted arm cannot be exact on the lane sweep (it measured 15/128)");
}
