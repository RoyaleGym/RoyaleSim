//! THE 2026 PATHFINDER AND LOCOMOTION LAW, measured on client 15.535.29.
//!
//! SOURCE OF TRUTH
//!     docs/pathfinder-spec.md states the rules and docs/movement-measurements.md
//!     states the evidence. Every constant comes from
//!     data/calibration.json -- the `pathfinding.*` and `movement.*` sections, all
//!     measured against the offline trace corpus (CR 15.535.29). Section
//!     references below (spec 3.2, spec 7.1, ...) are to the spec file.
//!
//! THE SPEC AND THE MEASUREMENTS REPORT ARE BOTH 15.535.29, RECORDED OFFLINE. Where
//! the LIVE 16.402 client disagrees, the live crosscheck wins and the ledger key
//! says so. Three rules below are settled that way and
//! are marked LIVE 16.402 where they appear: every building occludes, both sides,
//! not only the mover's own (OCCLUSION_MODEL / FRIENDLYONLY_OCCLUSIONS); an occluded
//! cell COSTS PATHFINDING_BUILDING_COST rather than blocking, which makes spec 4.6's
//! goal-cell exemption unnecessary (OCCLUDED_CELL_TREATMENT); and `h` is CHEBYSHEV
//! over the goal set, not octile (HEURISTIC_FORM).
//!
//! WHAT IS DIFFERENT FROM path.rs
//!     1. The route is a list of half-tile CELL CENTRES, stored GOAL-FIRST and
//!        popped from the back (spec 5.1). The other three models store waypoints
//!        start-first.
//!     2. Movement does not use `path::advance`. There is NO fractional carry: the
//!        sub-subtile remainder is discarded every tick (spec 7.2, calibration
//!        movement.POSITION_ROUNDING), which is measurably what the real engine
//!        does -- integrating the truncating law alone reproduces 106-308 ticks of
//!        every isolated walk bit-exactly, and a carry does not.
//!     3. Re-planning is event-driven, not periodic (spec 8, calibration
//!        pathfinding.REPLAN_TRIGGERS).
//!
//! PLANNED IN THE TEAM'S FRAME, like every other model (path.rs header). The
//! recorded `path_nodes` are ABSOLUTE arena cells, but that does not force the
//! engine's convention: the shipped grid is exactly rotation-symmetric with the two
//! lane bits swapped (arena.rs `is_rotation_symmetric`), this cost model reads
//! "either lane bit" so it is invariant under that swap, and the goal predicate,
//! the heuristic and the step law are all built from RELATIVE vectors, which the
//! rotation negates. So the plan for a Red unit is the rotation of the plan for its
//! Blue twin's rotated problem -- seat symmetry by construction rather than by
//! auditing (tests/mirror.rs measures the claim, path2026.rs tests check the
//! rotation of one plan directly).
//!
//! WHAT IS NOT MODELLED, and must not be quietly absorbed into a card constant:
//! avoidance (`avoidance_offset`), crowd separation and combat pushback. Two of
//! the three set no flag at all in the 15.535.29 traces, and 174 movement-law
//! failures corpus-wide are all in those regimes (calibration
//! movement.CONTACT_DOMAIN). Everything here is the ISOLATED-unit law.
#![allow(unexpected_cfgs)]

use crate::arena::Arena;
use crate::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE};
use crate::path::{FrameWorld, NavRequest, Pathfinder};
use crate::path16402;
use crate::state::{Calib, HeuristicForm, OccludedCells, PathSearch, TieBreak, WaypointArriveRule};
use crate::Team;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// TERRAIN the search may never enter -- not even as the goal cell. WATER, and only
/// water (calibration pathfinding.WATER_RULE_GROUND = "impassable", which
/// `Calib::from_json` refuses to load with any other value).
///
/// Kept DISTINCT from `OCCLUDED` because spec 4.6's goal-cell exemption is written
/// about occluders, and a shared sentinel silently extended it to the river: a
/// melee unit whose target stands on a bridge has in-reach cells two rows into the
/// water, and with one sentinel the route's final cell could legally be one of them.
/// No trace exercises that, so no gate caught it.
pub const IMPASSABLE: i32 = -1;

/// A cell the search refuses during expansion but ACCEPTS as the goal (spec 4.6).
///
/// UNDER THE MEASURED `OCCLUDED_CELL_TREATMENT = cost_50` ONLY THE BIT-16 BLOCK
/// CARRIES IT -- in this arena the two king footprints plus the deploy-restricted
/// border strips (`data/derived/arena.json`, rows 0-1/62-63, 28-29/34-35 at the
/// edges, and cols 15-20 of the king rows). A building's occlusion box is a COST
/// now, not a sentinel (`CostField::occlude`), so spec 4.6's exemption no longer
/// reaches any box: live, 97 of 785 paths cross a box interior and the exemption
/// scores 12 failures against 6 without it.
///
/// WHAT IS LEFT OF THE EXEMPTION IS THE KING BLOCK, and it stays because a melee
/// card's whole goal set can lie inside it -- reach 1300 native around the king
/// centre (9000, 29000) touches no cell outside cols 15-20, rows 55-60 -- so
/// refusing it outright would leave every short-reach unit unable to path to a king
/// tower. It is UNIDENTIFIABLE on the live corpus either way: all 13 bit-16 cells
/// any live path ever enters lie inside a king's R = 1400 box, where the box cost
/// takes precedence and this sentinel is never read; the other 104 (the arena-edge
/// strips) are never entered at all.
///
/// Selecting `OCCLUDED_CELL_TREATMENT = block` puts building boxes back on this
/// sentinel, exemption included -- the refuted 15.535 reading, kept runnable.
pub const OCCLUDED: i32 = -2;

/// Neighbour offsets, in the order the search expands them
/// (calibration pathfinding.TIE_BREAK).
///
/// UNVERIFIED AND KNOWN TO BE INSUFFICIENT -- a placeholder, not a finding. Where
/// several successors are equally optimal the recorded paths take the orthogonal one on
/// 1568 of 1672 ambiguous steps (93.8 %), but this order still reproduces only
/// 22/140 exact node sequences and no discipline tried exceeds 13/76 distinct
/// experiments (spec 3.6). This is THE key the ledger says is most likely to be
/// swapped, so it is driven from the ledger rather than hardcoded: the two rival
/// disciplines the registry lists are here and selecting one is a one-line edit to
/// calibration.json. The order reaches the result only through the push counter in
/// `OpenKey`.
fn neighbours(tie: TieBreak) -> [(i32, i32); 8] {
    match tie {
        // Orthogonals in N, S, W, E order, then the four diagonals.
        TieBreak::OrthoFirstPlaceholder => {
            [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)]
        }
        // The 3x3 neighbourhood scanned row-major, the order a nested
        // `for dy { for dx }` produces -- the shape a hand-written successor loop
        // most often has.
        TieBreak::RowMajor => {
            [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)]
        }
        // Diagonals first: the opposite of the measured 93.8 % preference, kept so
        // the discipline sweep has a control.
        TieBreak::DiagFirst => {
            [(-1, -1), (1, -1), (-1, 1), (1, 1), (0, -1), (0, 1), (-1, 0), (1, 0)]
        }
    }
}

/// The A* open-list key: (f, h, insertion counter, cell). Ordered by f, then by the
/// smaller h (prefer the node nearer the goal), then FIFO by insertion.
/// `REOPEN_CLOSEDNODES = FALSE` (datamined) is the pop-side `closed` check below.
type OpenKey = (i32, i32, u32, u32);

/// One team's view of the pathfinding grid: per-cell entry cost, already carrying
/// the terrain and every building on the board.
pub struct CostField {
    pub cols: i32,
    pub rows: i32,
    cost: Vec<i32>,
    /// Is this cell inside at least one building's occlusion box? Kept SEPARATELY
    /// from `cost` because under the measured `cost_50` an occluded cell is an
    /// ordinary price (50) and `refused` can no longer answer the question -- and
    /// "the box claims this cell" is exactly what the occlusion-shape gates
    /// (tests/oracle2026.rs) have to ask, whichever treatment is selected.
    boxed: Vec<bool>,
}

impl CostField {
    #[inline]
    pub fn at(&self, idx: usize) -> i32 {
        self.cost[idx]
    }

    /// Is this cell refused during expansion, for either reason? Both sentinels are
    /// negative and no real cost is, so one comparison covers them. Under `cost_50`
    /// a building box is NOT refused -- ask `in_building_box` for that.
    #[inline]
    pub fn refused(&self, idx: usize) -> bool {
        self.cost[idx] < 0
    }

    /// Does a building's half-open occlusion box cover this cell? True whatever
    /// `OCCLUDED_CELL_TREATMENT` says, so the measured SHAPE stays falsifiable
    /// independently of what the shape costs.
    #[inline]
    pub fn in_building_box(&self, idx: usize) -> bool {
        self.boxed[idx]
    }

    #[inline]
    pub fn idx(&self, col: i32, row: i32) -> usize {
        (row * self.cols + col) as usize
    }

    /// Terrain only (spec 2.2): lane bits 1|2 are road, water is `IMPASSABLE`, the
    /// bit-16 arena-edge/king block is `OCCLUDED`, everything else is the default.
    ///
    /// WATER IS A HARD BLOCK FOR GROUND even though `PATHFINDING_WATER_COST = 7`
    /// ships: modelling water as traversable at 7 makes 25 of the 150 recorded 15.535.29 first
    /// paths strictly dearer than the optimum -- the game refused water shortcuts
    /// it would have taken (calibration pathfinding.WATER_RULE_GROUND), and 51 of 785
    /// live ones. That key is pinned to "impassable" at load, so
    /// `OCCLUDED_CELL_TREATMENT` does NOT reach it: the two keys used to share this
    /// branch, and `cost_50` -- which is now the measured, shipped value -- quietly
    /// made the river walkable. One ledger key overriding another that the loader
    /// refuses to read any other way.
    ///
    /// The 2026-only marker bits (128 on 8 cells, 256 on the two bridge cells, 512
    /// on the two centre cells) are NOT road; adding them to the road set changes
    /// nothing, and the engine's arena.json carries none of them anyway.
    ///
    /// BIT 16 IS A HARD BLOCK REGARDLESS OF `OCCLUDED_CELL_TREATMENT`, and that is a
    /// LIVE 16.402 finding, not an oversight. The two keys
    /// used to share this branch, so selecting `cost_50` made the arena-edge strips
    /// walkable as a side effect of a statement about BUILDINGS. On the live corpus
    /// bit-16 carries no pathfinding meaning of its own that can be measured: every
    /// bit-16 cell a path enters is inside a king's box, where `occlude` overwrites
    /// this value with the box cost, and the 104 edge-strip cells are never entered.
    /// So the flag is unidentifiable and harmless -- kept impassable, which is the
    /// conservative reading, and kept out of the building key's reach.
    pub fn terrain(arena: &Arena, calib: &Calib) -> CostField {
        let lanes = arena.bit_lane_left | arena.bit_lane_right;
        let mut cost = vec![calib.path_cost_default; (arena.cols * arena.rows) as usize];
        for row in 0..arena.rows {
            for col in 0..arena.cols {
                let bits = arena.cell_bits(col, row);
                let i = (row * arena.cols + col) as usize;
                cost[i] = if bits & arena.bit_water != 0 {
                    IMPASSABLE
                } else if bits & arena.bit_no_deploy != 0 {
                    OCCLUDED
                } else if bits & lanes != 0 {
                    calib.path_cost_road
                } else {
                    calib.path_cost_default
                };
            }
        }
        CostField { cols: arena.cols, rows: arena.rows, cost, boxed: vec![false; (arena.cols * arena.rows) as usize] }
    }

    /// Stamp one building's occlusion box (spec 4.1-4.3).
    ///
    /// HALF-OPEN, AXIS-ALIGNED, PER-AXIS: cells overlapping
    /// `[cx - R, cx + R) x [cy - R, cy + R)`, floor division on both edges. A
    /// CLOSED box blocks 57 cells the recorded paths use; a circle-overlap test
    /// fails 45 of 216; a Euclidean centre test fails 23.
    ///
    /// NO MOVER PAD. The term is exactly zero: a pad of even ONE native unit blocks
    /// 155 cells the recorded paths use, because 3500 - 1000 = 2500 lands
    /// exactly on a cell boundary and the Giant's control path runs up column 4 at
    /// rows 11-16.
    ///
    /// THE BOX IS A PRICE, NOT A WALL (LIVE 16.402, calibration
    /// OCCLUDED_CELL_TREATMENT = cost_50): 97 of 785 live paths cross a box
    /// interior -- always exactly one cell, always the cell before the goal, always
    /// inside the enemy king's R = 1400 box. A hard block makes those 97 infeasible;
    /// `PATHFINDING_BUILDING_COST = 50` leaves 6 failures. The exact price is only
    /// bracketed (12 -> 114 failures, 13 -> 99, anything >= 14 -> 6), and the shipped
    /// 50 is inside the window, so the shipped constant is used rather than a fitted
    /// one.
    ///
    /// AND IT TAKES PRECEDENCE OVER THE TERRAIN FLAG -- that is the actionable half
    /// the live 16.402 finding. Resolving terrain first and letting an impassable
    /// bit-16 cell win costs 39 of 337 live paths, because a king's box IS the
    /// bit-16 king block cell for cell.
    pub fn occlude(&mut self, arena: &Arena, calib: &Calib, centre: Vec2, radius: i32) {
        if radius <= 0 {
            return;
        }
        let cell = arena.cell;
        let c0 = (centre.x - radius).div_euclid(cell).max(0);
        let c1 = (centre.x + radius - 1).div_euclid(cell).min(self.cols - 1);
        let r0 = (centre.y - radius).div_euclid(cell).max(0);
        let r1 = (centre.y + radius - 1).div_euclid(cell).min(self.rows - 1);
        let v = match calib.occluded_cells {
            OccludedCells::Block => OCCLUDED,
            OccludedCells::Cost50 => calib.path_cost_building,
        };
        for row in r0..=r1 {
            for col in c0..=c1 {
                let i = (row * self.cols + col) as usize;
                self.boxed[i] = true;
                // A box that overlaps the river must not DOWNGRADE water -- neither
                // to the goal-exempt sentinel nor to a payable 50. A building beside
                // a bridge would otherwise hand the search a legal route through the
                // water, which WATER_RULE_GROUND refuses on its own evidence (25 of
                // 150 recorded 15.535.29 first paths, and 51 of 785 live ones, go wrong at
                // cost 7).
                if self.cost[i] != IMPASSABLE {
                    self.cost[i] = v;
                }
            }
        }
    }

    /// Terrain plus EVERY BUILDING ON THE BOARD, both sides (LIVE 16.402,
    /// calibration pathfinding.OCCLUSION_MODEL). `ignore` is the building the unit
    /// is walking up to attack, and it is still exempt.
    ///
    /// `PATHFINDING_FRIENDLYONLY_OCCLUSIONS = TRUE` remains datamined and true as a
    /// globals value; it is the MEASURED pathfinder that disagrees with it. The
    /// offline corpus could not tell -- no interior cell of any of its first paths
    /// lay inside an enemy tower box, so friendly-only scored identically (0/0/0).
    /// Live, friendly-only fails 119 of 785 first paths and adding the other side's
    /// buildings fixes 113 and breaks 0. About 107 of those are the enemy KING
    /// footprint and only ~6 are non-king enemy buildings, reproduced cell for cell
    /// on three to five witnesses -- enough to act on, thin enough
    /// to say so.
    ///
    /// THE TARGET BUILDING STAYS EXEMPT, which the live model does NOT do: the
    /// The scored reference model occludes the target too,
    /// and that is how the 97 enemy-king crossings were scored. Keeping `ignore`
    /// costs nothing on any gate here and is the older, separately-argued rule (a
    /// unit must be able to reach the thing it is attacking); recorded as an open
    /// divergence in calibration pathfinding.OCCLUSION_MODEL `engine_divergence_target_building`.
    ///
    /// THE TERRAIN FIELD IS REBUILT PER PLAN, deliberately. It used to be memoised in
    /// a process-global `OnceLock` keyed on NOTHING, so the first `(Arena, Calib)` a
    /// process ever planned with won for every later battle: `PATHFINDING_COSTS`,
    /// `OCCLUDED_CELL_TREATMENT` and the arena itself all became unreadable after the
    /// first plan, and `Calib` is a per-battle value (`BattleConfig.calib` is public,
    /// `load_with` restores the snapshot's own). A plan must be a function of its
    /// inputs, not of process history. The 2304 bit tests are ~12 % on top of the
    /// chamfer pass this function's caller already runs over the same 2304 cells.
    pub fn for_mover(world: &FrameWorld, calib: &Calib, req: &NavRequest) -> CostField {
        let mut f = CostField::terrain(world.arena, calib);
        for o in world.obstacles.iter() {
            if Some(o.id) == req.ignore {
                continue;
            }
            f.occlude(world.arena, calib, o.shape.center(), o.radius);
        }
        f
    }
}

/// Cell centre in subtiles (spec 2.1: `col*500 + 250` native).
#[inline]
pub fn cell_centre(arena: &Arena, col: i32, row: i32) -> Vec2 {
    arena.half_to_subtile_center(col, row)
}

/// The cell containing `p`, clamped into the grid.
#[inline]
pub fn cell_of(arena: &Arena, p: Vec2) -> (i32, i32) {
    let (c, r) = arena.subtile_to_half(p);
    (c.clamp(0, arena.cols - 1), r.clamp(0, arena.rows - 1))
}

/// The heuristic's step weights (spec 3.3, calibration pathfinding.HEURISTIC_FORM):
/// what `h` charges for one orthogonal and one diagonal cell of separation. The
/// orthogonal weight is `PATHFINDING_DEFAULTHEURISTIC_COST` under both forms.
///
/// CHEBYSHEV IS THE MEASURED ONE (LIVE 16.402): both weights are H, so
/// `h = 5 * max(|dc|, |dr|)`. Handed the recorded goal cell, exact node-sequence
/// reproduction on the live corpus is 192/292 for Chebyshev at W in 3..5 (the three
/// weights are indistinguishable; 5 is the datamined constant), against 171/292 for
/// Dijkstra and 78/292 for the octile form this engine used before. Offline the same
/// sweep on lane_sweep_Knight reads 36/128 against 14/128.
///
/// STILL ADMISSIBLE, which is what lets `REOPEN_CLOSEDNODES = FALSE` stand: the
/// cheapest cell in the field costs `road` = 5 to enter and the cheapest diagonal
/// 5*1414/1000 = 7, so any route of Chebyshev distance N costs at least 5N = h. It
/// is CONSISTENT too -- h changes by at most 5 across any neighbour, and a diagonal
/// step costs at least 7 -- so a closed node is never reached more cheaply later.
///
/// The octile form (5 straight, 7 diagonal) is the refuted hypothesis, kept
/// selectable so the ledger's other candidate names an arm that actually runs.
#[inline]
fn heuristic_weights(calib: &Calib) -> (i32, i32) {
    let straight = calib.path_cost_heuristic;
    match calib.heuristic_form {
        HeuristicForm::ChebyshevOverGoalSet => (straight, straight),
        HeuristicForm::OctileOverGoalSet => (straight, straight * calib.diag_num / calib.diag_den),
    }
}

/// The weighted-chamfer distance `h = straight*(max - min) + diag*min` -- Chebyshev
/// when the two weights are equal, octile when the diagonal is `x sqrt(2)`. The
/// DEFINITION the chamfer transform in `goal_set_heuristic` computes; only the test
/// that proves the two agree calls it directly.
#[cfg(test)]
#[inline]
fn chamfer_distance(calib: &Calib, dc: i32, dr: i32) -> i32 {
    let (a, b) = (dc.abs(), dr.abs());
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    let (straight, diag) = heuristic_weights(calib);
    straight * (hi - lo) + diag * lo
}

/// The goal predicate (spec 6.1): is this cell's CENTRE within `reach` of the
/// target's CENTRE POINT? i64 throughout -- a squared distance in subtiles reaches
/// 3.3e11 and an i32 would silently overflow (spec 1).
///
/// Public so the goal rule can be asked of a cell without running a plan -- the tests
/// score it against the recorded goal cells that way. The tick loop tells an
/// arrived unit (walk the last gap) from a walled-in one (hold position) by
/// `plan_cells`'s own feasibility flag instead, which is exact.
#[inline]
pub fn in_reach(arena: &Arena, col: i32, row: i32, target: Vec2, reach: i32) -> bool {
    let c = cell_centre(arena, col, row);
    c.dist2(target) <= (reach as i64) * (reach as i64)
}

/// `h` for every cell at once: the heuristic distance to the NEAREST cell of the
/// goal set (spec 3.3, 3.5). THE GOAL-SET TREATMENT SURVIVES THE 16.402 CHEBYSHEV
/// promotion unchanged -- only the metric inside it changed.
///
/// A two-pass chamfer transform, not a loop over the goal set: both forms ARE the
/// 3x3 chamfer metric with weights `(straight, diag)`, and a chamfer transform is
/// exact whenever `a <= b <= 2a` -- 5 <= 5 <= 10 for Chebyshev, 5 <= 7 <= 10 for
/// octile. So this gives EXACTLY `min over goal cells of h(cell, goal)` in O(cells)
/// instead of O(cells x goal cells), which matters because a long-range unit's goal
/// set is hundreds of cells (a Musketeer's reach is 13 cells) and this runs per
/// plan. `heuristic_matches_the_brute_force_minimum` is the proof, for both forms.
fn goal_set_heuristic(arena: &Arena, calib: &Calib, goal_mask: &[bool]) -> Vec<i32> {
    let (cols, rows) = (arena.cols, arena.rows);
    let (straight, diag) = heuristic_weights(calib);
    const INF: i32 = i32::MAX / 4;
    let mut h = vec![INF; (cols * rows) as usize];
    for (i, g) in goal_mask.iter().enumerate() {
        if *g {
            h[i] = 0;
        }
    }
    let relax = |h: &mut Vec<i32>, col: i32, row: i32, dc: i32, dr: i32| {
        let (nc, nr) = (col + dc, row + dr);
        if nc < 0 || nr < 0 || nc >= cols || nr >= rows {
            return;
        }
        let w = if dc != 0 && dr != 0 { diag } else { straight };
        let v = h[(nr * cols + nc) as usize].saturating_add(w);
        let i = (row * cols + col) as usize;
        if v < h[i] {
            h[i] = v;
        }
    };
    for row in 0..rows {
        for col in 0..cols {
            for (dc, dr) in [(-1, -1), (0, -1), (1, -1), (-1, 0)] {
                relax(&mut h, col, row, dc, dr);
            }
        }
    }
    for row in (0..rows).rev() {
        for col in (0..cols).rev() {
            for (dc, dr) in [(1, 1), (0, 1), (-1, 1), (1, 0)] {
                relax(&mut h, col, row, dc, dr);
            }
        }
    }
    h
}

/// Plan the route: half-tile cell centres, GOAL-FIRST, WITHOUT the start cell.
///
/// Returns an empty route when the unit's own cell already satisfies the goal
/// predicate (it has arrived) and when no route exists at all. The caller must not
/// confuse the two; both mean "do not move this tick", and `plan_cells` reports
/// which through its second return value.
///
/// SPEC 5.2: the start cell is dropped, and only the start cell. The Chebyshev-2
/// gap you see between a unit and the first published node is NOT a "drop two
/// cells" rule -- it is rule 7.5 firing once after the first move, and it is
/// Chebyshev-1 for both `repath_Giant` first paths, which a literal drop-2 gets
/// wrong.
pub fn plan_cells(world: &FrameWorld, calib: &Calib, req: &NavRequest) -> (Vec<(i32, i32)>, bool) {
    if calib.path_search == PathSearch::Client16402 {
        return plan_cells16402(world, calib, req);
    }
    let arena = world.arena;
    let field = CostField::for_mover(world, calib, req);
    let (sc, sr) = cell_of(arena, req.pos);
    let n = (arena.cols * arena.rows) as usize;
    let start = field.idx(sc, sr);

    // The goal SET, and the heuristic toward it (spec 3.5 -- the admissibility
    // trap). An h computed to the TARGET cell over-estimates the cost to the actual
    // goal by up to reach/cell * H and is non-zero AT goal cells, so A* then pops a
    // goal that is not the cheapest one. Measured: with h to the target cell the
    // search returns column 7 (cost 152) on every one of the six walk traces where
    // the recorded unit takes column 6 (cost 150).
    let mut any_goal = false;
    let mut goal_mask = vec![false; n];
    for row in 0..arena.rows {
        for col in 0..arena.cols {
            if in_reach(arena, col, row, req.goal, req.reach) {
                goal_mask[(row * arena.cols + col) as usize] = true;
                any_goal = true;
            }
        }
    }
    if !any_goal {
        // No cell is within reach of the target -- nothing to walk to. Only
        // reachable with a reach smaller than half a cell diagonal.
        return (Vec::new(), false);
    }
    if goal_mask[start] {
        return (Vec::new(), true);
    }
    let hcache = goal_set_heuristic(arena, calib, &goal_mask);
    let h = |col: i32, row: i32| -> i32 { hcache[(row * arena.cols + col) as usize] };

    let mut g = vec![i32::MAX; n];
    let mut parent = vec![u32::MAX; n];
    let mut closed = vec![false; n];
    let mut open: BinaryHeap<Reverse<OpenKey>> = BinaryHeap::new();
    let mut counter: u32 = 0;
    g[start] = 0;
    let h0 = h(sc, sr);
    open.push(Reverse((h0, h0, counter, start as u32)));

    let mut found: Option<usize> = None;
    while let Some(Reverse((_, _, _, cur))) = open.pop() {
        let cur = cur as usize;
        if closed[cur] {
            continue; // REOPEN_CLOSEDNODES = FALSE
        }
        closed[cur] = true;
        let (cc, cr) = ((cur as i32) % arena.cols, (cur as i32) / arena.cols);
        if in_reach(arena, cc, cr, req.goal, req.reach) {
            found = Some(cur);
            break;
        }
        for (dc, dr) in neighbours(calib.tie_break) {
            let (nc, nr) = (cc + dc, cr + dr);
            if nc < 0 || nr < 0 || nc >= arena.cols || nr >= arena.rows {
                continue;
            }
            let ni = field.idx(nc, nr);
            if closed[ni] {
                continue;
            }
            let mut step = field.at(ni);
            if step == IMPASSABLE {
                // WATER. No exemption: a ground unit may not finish a route in the
                // river, whatever its reach says (calibration WATER_RULE_GROUND).
                continue;
            }
            if step == OCCLUDED {
                // SPEC 4.6's goal-cell exemption. UNDER THE MEASURED `cost_50` NO
                // BUILDING BOX REACHES HERE any more (`CostField::occlude` writes a
                // price), so what is left is the bit-16 king block, and the
                // exemption is what keeps a short-reach card able to path to a king
                // tower at all -- its whole goal set can lie inside that block.
                //
                // Live, the exemption ON building boxes is REFUTED, which is why it
                // no longer applies to them: 12 failures with it against 6 without,
                // and 97 of 785 live paths cross a box interior that it cannot
                // explain (they are priced instead). It is charged the plain cell
                // cost; the choice cannot matter, because the search stops on
                // popping it.
                if !in_reach(arena, nc, nr, req.goal, req.reach) {
                    continue;
                }
                step = calib.path_cost_default;
            }
            if dc != 0 && dr != 0 {
                // SPEC 3.2: a diagonal pays sqrt(2) times the ENTERED cell's cost.
                // Integer arithmetic: 5 -> 7, 8 -> 11. Load-bearing -- with a
                // uniform diagonal the road discount is unidentifiable and a flat
                // cost 8 then fails 96 of 150 paths.
                step = step * calib.diag_num / calib.diag_den;
            }
            // No corner-cutting restriction (spec 3.1): corner-cut-strict fails 6
            // of 140, and over ~486 000 adjacent node pairs in the recorded
            // paths zero are non-8-neighbours.
            let ng = g[cur].saturating_add(step);
            if ng < g[ni] {
                g[ni] = ng;
                parent[ni] = cur as u32;
                counter += 1;
                let hv = h(nc, nr);
                open.push(Reverse((ng + hv, hv, counter, ni as u32)));
            }
        }
    }

    let Some(goal) = found else { return (Vec::new(), false) };
    // Walk the parent chain from the goal: that IS goal-first order (spec 5.1).
    let mut out = Vec::new();
    let mut i = goal;
    while i != start {
        out.push(((i as i32) % arena.cols, (i as i32) / arena.cols));
        let p = parent[i];
        if p == u32::MAX {
            break;
        }
        i = p as usize;
    }
    (out, true)
}

/// KS_POS_TO_TARGET_GROUND_AVOID_BUILDINGS (globals.csv TRUE), as the goal-cell
/// choice reads it: the goal prefers a cell outside every building box. The rule
/// ANDs it with the target not flying (FlyingHeight > 0), which
/// `avoid_buildings16402` does; every corpus target is a ground unit or a tower, and
/// `true` scores 439/442 against 421/425 for `false`.
pub const AVOID_BUILDINGS_16402: bool = true;

/// The flag the goal-cell choice is handed for THIS target: the global, off for a
/// flying target. A ground unit whose target hovers over a building box walks to the
/// nearest in-reach cell, boxed or not.
#[inline]
pub fn avoid_buildings16402(target_flying: bool) -> bool {
    #[cfg(clash_plant = "goal_ignores_flying_target")]
    let target_flying = false; // PLANT (regression): the global passed unconditionally.
    AVOID_BUILDINGS_16402 && !target_flying
}

/// The grid's five prices and heuristic weight, from the ledger (the game copies
/// them from its pathfinding globals). WATER_COST is paid by hovering / JumpEnabled
/// movers (`NavRequest.jumper`, path16402.rs `cell_cost_for`); walkers pay BLOCKED
/// for water.
pub fn costs16402(calib: &Calib) -> path16402::Costs {
    path16402::Costs {
        water: calib.path_cost_water,
        blocked: calib.path_cost_blocked,
        building: calib.path_cost_building,
        default: calib.path_cost_default,
        road: calib.path_cost_road,
        heuristic: calib.path_cost_heuristic,
    }
}

/// The static half of the cost field (the game's cost function reads only the
/// lane bits and the water bit of the tilemap; bit 16 is not consulted).
pub fn terrain16402(arena: &Arena, costs: &path16402::Costs) -> path16402::Terrain {
    let lanes = arena.bit_lane_left | arena.bit_lane_right;
    path16402::Terrain::new(
        arena.cols,
        arena.rows,
        |c, r| arena.cell_bits(c, r) & lanes != 0,
        |c, r| arena.cell_bits(c, r) & arena.bit_water != 0,
        costs,
    )
}

/// THE 16.402 SEARCH (calibration pathfinding.PATH_SEARCH =
/// client16402; path16402.rs is the implementation, settled on the live
/// 16.402 captures).
///
/// PLANNED IN ABSOLUTE ARENA COORDINATES, not the team's frame. The game's goal-cell
/// scan runs rows-ascending in absolute y and columns by which half of the arena
/// the mover stands in, its neighbour order is N S W E NW SW SE NE in absolute
/// directions, and its heap tie-breaks fall out of that -- so a Red unit's route is
/// NOT the rotation of its Blue twin's, and the one asymmetric rot180
/// pair the live corpus pairs up is the rule, not noise. This un-rotates a Red request (position,
/// target and every obstacle; `Arena::to_frame` is an involution), plans, and
/// rotates the cells back into the frame the caller expects.
///
/// EVERY BUILDING ON THE BOARD IS STAMPED, the target included: the game does not
/// exempt the building a unit walks up to attack (its occlusion rebuild takes
/// every building of both sides), and the goal-cell scan keeps the
/// goal out of the box instead whenever a dry, unboxed cell is within reach. So
/// `req.ignore` is not read here.
///
/// UNITS: the game computes in native millitiles; the engine's subtile is 18 of
/// them and every engine position is a whole number of them (the movement law
/// truncates in native units), so the boundary division is exact.
fn plan_cells16402(world: &FrameWorld, calib: &Calib, req: &NavRequest) -> (Vec<(i32, i32)>, bool) {
    use crate::fixed::SUBTILE_PER_MILLITILE as K;
    let arena = world.arena;
    let team = req.team;
    let abs = |p: Vec2| -> (i32, i32) {
        let a = arena.from_frame(team, p);
        (a.x / K, a.y / K)
    };
    let costs = costs16402(calib);
    let terrain = terrain16402(arena, &costs);
    let occluders: Vec<path16402::Occluder> = world
        .obstacles
        .iter()
        .map(|o| {
            let (x, y) = abs(o.shape.center());
            path16402::Occluder { x, y, r: o.radius / K }
        })
        .collect();
    let occ = path16402::occlusion(&terrain, &occluders, &costs);
    let mut pf = path16402::PathFinder::new(arena.cols, arena.rows, costs.heuristic);
    let request = path16402::Request {
        actor: abs(req.pos),
        target: abs(req.goal),
        reach: req.reach / K,
        avoid_buildings: avoid_buildings16402(req.target_flying),
        jumper: req.jumper,
    };
    let back = |(c, r): (i32, i32)| -> (i32, i32) {
        match team {
            Team::Blue => (c, r),
            Team::Red => (arena.cols - 1 - c, arena.rows - 1 - r),
        }
    };
    match path16402::plan(&terrain, &occ, &mut pf, &request, &costs) {
        path16402::Plan::Route(cells) => (cells.into_iter().map(back).collect(), true),
        path16402::Plan::Arrived => (Vec::new(), true),
        path16402::Plan::NoGoal | path16402::Plan::Unreachable => (Vec::new(), false),
    }
}

/// The route as the engine stores it: cell centres in subtiles, goal-first, plus
/// `plan_cells`'s feasibility flag.
///
/// THE FLAG IS PART OF THE RETURN, not a detail: this used to drop it, so "A* found
/// no route" and "already arrived" both surfaced as an empty Vec and the tick loop
/// treated both as "arrived" -- it then abandoned the grid and walked the unit
/// straight at its target, through cells its own cost field marks impassable.
pub fn plan_waypoints(world: &FrameWorld, calib: &Calib, req: &NavRequest) -> (Vec<Vec2>, bool) {
    let (cells, ok) = plan_cells(world, calib, req);
    (cells.into_iter().map(|(c, r)| cell_centre(world.arena, c, r)).collect(), ok)
}

/// The goal CELL rule 6.4 predicts, used ONLY as the replan trigger (spec 6.5, 8.2).
///
/// WHY A SEPARATE FUNCTION. The goal cell the search actually stops at is an OUTPUT
/// of the expansion order (spec 6.2): between 12 and 52 cells satisfy the reach
/// predicate per sample and the recorded choice is not the cheapest one in 114 of
/// 140 samples. So the search keeps the reach PREDICATE, and this -- the observed
/// shape of the goal cell, 98.96 % per tick -- is only used to notice that the goal
/// has moved. Re-running the search every tick to compare would cost an A* per unit
/// per tick; this costs a handful of comparisons.
///
/// The column is the unit's own column clamped into the target's collision box
/// (`target_x +- CollisionRadius(target)`, half-open like the occlusion box). The
/// row is the first row, walking from the unit's own row TOWARD the target, whose
/// cell centre is within `reach` of the target's centre -- which carries spec 6.3's
/// sign for free: a target below the unit gives a goal row below it.
pub fn trigger_goal_cell(
    arena: &Arena,
    pos: Vec2,
    target: Vec2,
    target_radius: i32,
    reach: i32,
) -> (i32, i32) {
    let (uc, ur) = cell_of(arena, pos);
    let lo = (target.x - target_radius).div_euclid(arena.cell).clamp(0, arena.cols - 1);
    let hi = (target.x + target_radius - 1).div_euclid(arena.cell).clamp(0, arena.cols - 1);
    let col = uc.clamp(lo.min(hi), hi.max(lo));
    let (tc, tr) = cell_of(arena, target);
    let step = if tr >= ur { 1 } else { -1 };
    let mut row = ur;
    loop {
        if in_reach(arena, col, row, target, reach) {
            return (col, row);
        }
        if row == tr {
            break;
        }
        row += step;
    }
    // Reachable only when `reach` is below half a cell diagonal, so no cell of the
    // column -- not even the target's own -- passes the test. The target's cell is
    // then the best answer there is, and the trigger only has to be STABLE.
    let _ = tc;
    (col, tr)
}

// ---------------------------------------------------------------------------
// the locomotion law (spec 7)

/// Integer divide truncating TOWARD ZERO -- Rust's `/` already does this, named so
/// the intent is not mistaken for `div_euclid` (spec 7.2: floor matches 0 of ~1000
/// discriminating ticks, because `dir.x < 0` on every moving tick of every walk
/// unit).
#[inline]
fn trunc_div(a: i64, b: i64) -> i32 {
    (a / b) as i32
}

/// THE LAW'S QUANTUM IS THE NATIVE ARENA UNIT, NOT THE SUBTILE, and that is not a
/// presentation detail -- every rounding below happens at millitile granularity and
/// the engine's 18x finer grid gives a DIFFERENT answer if it rounds there instead.
///
/// The Knight's first moving tick is the whole argument. d = (-249, +750) native:
/// `isqrt(249^2 + 750^2) = 790` and the heading is `(-80, +243)`. The same vector in
/// subtiles is (-4482, +13500) with `isqrt = 14224`, which is 4 MORE than 790 x 18,
/// and the heading comes out `(-80, +242)` -- one unit of y, every tick, forever.
/// The step has the same shape: `trunc(60 * 243 / 256) = 56` native = 1008 subtiles,
/// where `trunc(1080 * 243 / 256) = 1025` subtiles.
///
/// So: convert the relative vector to native, do the arithmetic there, and scale the
/// result back. Truncation toward zero is odd-symmetric, so this commutes with the
/// seat rotation exactly as the rest of the law does.
#[inline]
fn to_native(v: Vec2) -> Vec2 {
    Vec2::new(v.x / SUBTILE_PER_MILLITILE, v.y / SUBTILE_PER_MILLITILE)
}

/// The heading (spec 7.1, calibration movement.HEADING_LAW): a 1/256 direction from
/// the PRE-move position to the waypoint centre, with a FLOORED length.
///
/// `|dir|` legitimately ranges 254.678..256.236 because each axis truncates
/// independently. DO NOT renormalise or clamp to 256 -- over 1258 walk ticks the
/// floored length matches 1258, nearest-integer 1179, scale-then-sqrt 191, ceil 173.
#[inline]
pub fn norm256(d_subtiles: Vec2) -> Vec2 {
    let d = to_native(d_subtiles);
    let len = isqrt(d.len2());
    if len == 0 {
        return Vec2::default();
    }
    Vec2::new(trunc_div((d.x as i64) * 256, len), trunc_div((d.y as i64) * 256, len))
}

/// One tick of displacement (spec 7.2, calibration movement.POSITION_ROUNDING):
/// per axis, independently, truncating toward zero, remainder DISCARDED. `speed` is
/// the engine's subtiles-per-tick (`Speed * time.SPEED_TO_SUBTILES_PER_TICK`); the
/// truncation happens on the raw `Speed` (see `to_native`).
///
/// A BUFFED SPEED FOLLOWS THE SAME LAW WITH A LARGER INTEGER `S`, and ONE trace says
/// so: `data/oracle-native/rage/Knight_rage_seed2.jsonl.gz` drops a Rage at tick 140,
/// and the free per-unit integer fit over its isolated ticks gives S = 60 on
/// t121..155, S = 78 on t156..217 and S = 60 again on t218..340 -- 78 = 60 x 1.3
/// exactly. THE ROUNDING IS NOW MEASURED, AND IT IS FLOOR (LIVE 16.402, calibration
/// movement.BUFF_SPEED_RULE): the Knight (60 -> 78) and Skeletons (90 -> 117) are
/// exact products and prove nothing, but the raged ICE GOLEM walks at exactly 67,
/// where its stomped S = 52 gives floor(52 x 130/100) = 67 and round and ceil both
/// give 68. The multiplier lives on the buff, not the area effect
/// (`[BUFF.Rage] SpeedMultiplier = 130`).
///
/// THE ENGINE STILL DOES NOT ENTER THAT REGIME: nothing here multiplies `speed`, so
/// the constant is recorded rather than implemented, and its composition with the
/// stomp is still open (floor(52 x 1.3) and floor(floor(45 x 1.3) x 550/470) are
/// both 67).
#[inline]
pub fn step_delta(speed: i32, dir: Vec2) -> Vec2 {
    let s = (speed / SUBTILE_PER_MILLITILE) as i64;
    Vec2::new(
        trunc_div(s * (dir.x as i64), 256) * SUBTILE_PER_MILLITILE,
        trunc_div(s * (dir.y as i64), 256) * SUBTILE_PER_MILLITILE,
    )
}

/// Is this tick a stomp pause (spec 7.4, calibration movement.STOMP_PAUSE_SCHEDULE)?
///
/// `k` is the unit's MOVING-TICK INDEX -- 0 on its first moving tick, and NEVER
/// reset. Strict `>` beats `>=` (304/308 against 308/308 on the Giant), `(k+1)`
/// beats `k` (268/308), and every phase offset was brute-forced with only 0 fitting.
/// A naive "move for Stop ms then wait for Wait ms" countdown gives 13 moving ticks
/// in the Giant's first block where the 15.535.29 traces show 12.
#[inline]
pub fn stomp_paused(tick_ms: i32, stop_ms: i32, wait_ms: i32, k: u32) -> bool {
    if stop_ms <= 0 {
        return false;
    }
    let period = (stop_ms as i64) + (wait_ms as i64);
    let phase = ((k as i64 + 1) * tick_ms as i64).rem_euclid(period);
    phase > stop_ms as i64
}

/// THE STOMP CLOCK (calibration movement.STOMP_PAUSE_SCHEDULE = ms_clock, measured
/// on the live raged Golem), one walking tick. `clock` is the unit's clock in
/// milliseconds and `advance` is this tick's increment -- `tdiv(compose(Speed, 100),
/// 2)`, i.e. TICK_MS unbuffed and 65 under Rage. Returns (the new clock, whether
/// this tick is PAUSED):
///
/// ```text
///   clock += advance
///   Stop <= 0 or clock <= Stop            -> walk
///   rem = clock - (Stop + Wait);  rem >= 0 -> clock = rem, walk
///   else                                   -> paused
/// ```
///
/// With `advance == TICK_MS` this is exactly `stomp_paused` above on the unit's
/// moving-tick index, because the clock is then ((k + 1) x TICK_MS) mod the period:
/// the two calibration candidates agree on every unbuffed tick and part only under a
/// buff, where the residue drifts and a pause group can run four ticks long.
#[inline]
pub fn stomp_clock_step(clock: i32, advance: i32, stop_ms: i32, wait_ms: i32) -> (i32, bool) {
    if stop_ms <= 0 {
        return (clock, false);
    }
    let clock = clock + advance;
    if clock <= stop_ms {
        return (clock, false);
    }
    let rem = clock - (stop_ms + wait_ms);
    if rem >= 0 {
        (rem, false)
    } else {
        (clock, true)
    }
}

/// Has the unit arrived at `node` (spec 7.5, calibration
/// pathfinding.WAYPOINT_ARRIVE_RULE / WAYPOINT_ARRIVE_RADIUS)? Evaluated on the
/// POST-move position; at most one node is consumed per tick (0 of 3851 tail-drop
/// ticks dropped more than one).
///
/// `segment_projection` is the measured rule: the remaining distance ALONG THE
/// SEGMENT, `dot(node - pos, seg_dir) / 256` with integer truncation, where
/// `seg_dir` is the 1/256 direction frozen when the node was assigned (spec 7.7 --
/// the engine publishes it as `path_segment_direction`, and it is exact on the walk
/// traces with zero exceptions). Scored over all 34 644 stable-list tick pairs:
/// 16 errors, against 156 for the plain Euclidean radius, and all 10 of its early
/// fires carry `avoidance_offset != 0`, i.e. the regime no law here models. No
/// radius on the post-move distance can be right at all: restricted to isolated
/// units the Knight corpus holds a DROP at distance 1004.478 and a KEEP at 1000.648.
///
/// `euclid_post_move` is spec rule 7.5 exactly as written, kept selectable.
#[inline]
pub fn arrived(rule: WaypointArriveRule, radius: i32, node: Vec2, pos: Vec2, seg_dir: Vec2) -> bool {
    // Native units, like every other rounding in this file (`to_native`): the
    // threshold is one tile and the comparison is on a TRUNCATED integer, so doing
    // it 18x finer moves the boundary.
    let d = to_native(node.sub(pos));
    let r = (radius / SUBTILE_PER_MILLITILE) as i64;
    match rule {
        WaypointArriveRule::EuclidPostMove => isqrt(d.len2()) <= r,
        WaypointArriveRule::SegmentProjection => {
            if seg_dir.x == 0 && seg_dir.y == 0 {
                // No segment yet (the node was assigned this tick by a replan):
                // fall back to the distance, which is the same thing for a fresh
                // segment because the projection is then the full length.
                return isqrt(d.len2()) <= r;
            }
            let dot = (d.x as i64) * (seg_dir.x as i64) + (d.y as i64) * (seg_dir.y as i64);
            dot.div_euclid(256) <= r
        }
    }
}

/// The trait impl exists only so `PathModel` stays total behind one interface. The
/// 2026 model's route is GOAL-FIRST and its movement is not `steer` + `advance`;
/// state.rs `phase_path_2026` drives it and never calls this.
pub struct Oracle2026;

impl Pathfinder for Oracle2026 {
    /// REFUSED, on purpose. `Pathfinder::plan` has no `&Calib`, so this could only
    /// plan under `Calib::shipped()` -- the ledger FILE rather than the battle's own
    /// constants. It used to do exactly that, silently, and `pathfinder_for` is
    /// public, so a test reaching for the trait would have got a plan made with the
    /// wrong calibration. Call `path2026::plan_cells` / `plan_waypoints` with the
    /// battle's calib instead.
    fn plan(&self, _world: &FrameWorld, _req: &NavRequest) -> Vec<Vec2> {
        unreachable!("PathModel::Oracle2026 is driven by state.rs phase_path_2026, not by Pathfinder::plan")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Calib;

    fn req(pos: Vec2, goal: Vec2, reach: i32) -> NavRequest {
        NavRequest {
            #[cfg(clash_plant = "reflection_bridge_tie")]
            red: false,
            team: crate::Team::Blue,
            pos,
            goal,
            radius: 9000,
            sight: 99_000,
            step: 1080,
            reach,
            flying: false,
            target_flying: false,
            jumper: false,
            ignore: None,
        }
    }

    #[test]
    fn cell_geometry_matches_the_oracle_encoding() {
        // spec 2.1: 36 x 64 cells of 500 native units; centre = col*500 + 250.
        let a = Arena::shipped();
        assert_eq!((a.cols, a.rows), (36, 64));
        assert_eq!(a.cell, 9000, "one cell must be 500 native units = 9000 subtiles");
        assert_eq!(cell_centre(&a, 0, 0), Vec2::new(4500, 4500));
        assert_eq!(cell_centre(&a, 6, 47), Vec2::new(6 * 9000 + 4500, 47 * 9000 + 4500));
        assert_eq!(cell_of(&a, Vec2::new(3499 * 18, 8500 * 18)), (6, 17));
    }

    #[test]
    fn terrain_costs_are_road_five_plain_eight_water_blocked() {
        let a = Arena::shipped();
        let c = Calib::shipped();
        let f = CostField::terrain(&a, &c);
        // Column 6 up the left lane is road for the whole walk (spec 2.2).
        for row in 17..=29 {
            assert_eq!(f.at(f.idx(6, row)), c.path_cost_road, "col 6 row {row}");
        }
        // Column 3 beside it is plain.
        assert_eq!(f.at(f.idx(3, 20)), c.path_cost_default);
        // The river is IMPASSABLE -- refused even as a goal cell.
        assert_eq!(f.at(f.idx(3, 31)), IMPASSABLE);
        // The king block (bit 16) is OCCLUDED -- refused during expansion, allowed
        // as the goal, which is the only way a melee unit can path to a king tower.
        assert_eq!(f.at(f.idx(17, 5)), OCCLUDED);
        // AND IT STAYS OCCLUDED UNDER EITHER OCCLUDED_CELL_TREATMENT. The key is
        // about BUILDING boxes; it used to drive this branch too, so selecting
        // `cost_50` -- which is now the shipped value -- quietly made the
        // deploy-restricted arena-edge strips walkable at 50. One ledger key may not
        // decide another's question (the same defect WATER_RULE_GROUND had).
        for t in [OccludedCells::Block, OccludedCells::Cost50] {
            let mut c2 = Calib::shipped();
            c2.occluded_cells = t;
            let f2 = CostField::terrain(&a, &c2);
            assert_eq!(f2.at(f2.idx(17, 5)), OCCLUDED, "{t:?}: the king block");
            assert_eq!(f2.at(f2.idx(0, 0)), OCCLUDED, "{t:?}: the arena-edge strip");
        }
    }

    #[test]
    fn a_route_may_end_on_the_king_block_but_never_in_the_river() {
        // The two sentinels exist to be different here. spec 4.6's exemption is
        // written about OCCLUDERS; sharing one sentinel with the terrain silently
        // extended it to water.
        let a = Arena::shipped();
        let c = Calib::shipped();
        let world = FrameWorld { arena: &a, obstacles: &[] };
        let f = CostField::terrain(&a, &c);

        // A short-reach melee unit walking at the enemy KING: every cell within
        // reach of (9000, 29000) native lies inside the king block, so without the
        // exemption there would be no path at all.
        let king = Vec2::new(9000 * 18, 29000 * 18);
        let (p, ok) = plan_cells(&world, &c, &req(Vec2::new(9000 * 18, 20000 * 18), king, 1300 * 18));
        assert!(ok && !p.is_empty(), "a melee unit must still be able to path to a king tower");
        assert_eq!(f.at(f.idx(p[0].0, p[0].1)), OCCLUDED, "its goal cell is inside the king block");
        assert!(p[1..].iter().all(|&(col, row)| f.at(f.idx(col, row)) != OCCLUDED), "no INTERIOR cell may be");

        // A target on the left bridge, reach wide enough that in-reach cells reach
        // two rows into the river. No cell of the route may be one of them.
        let bridge = Vec2::new(3250 * 18, 16250 * 18);
        let (q, ok2) = plan_cells(&world, &c, &req(Vec2::new(3250 * 18, 10250 * 18), bridge, 1700 * 18));
        assert!(ok2 && !q.is_empty());
        let water: Vec<_> = q.iter().filter(|&&(col, row)| f.at(f.idx(col, row)) == IMPASSABLE).collect();
        assert!(water.is_empty(), "route runs through the river: {water:?}");
    }

    #[test]
    fn every_tie_break_the_ledger_accepts_is_actually_wired_to_the_search() {
        // The ledger calls TIE_BREAK the swappable placeholder behind the
        // unreproduced expansion order (spec 3.6), so selecting one of its other
        // candidates has to actually change what the search does. It used to be read
        // into `Calib` and then ignored, with one hardcoded order underneath: two of
        // the three values the loader accepts named a discipline that never ran.
        //
        // WHAT THE TIE-BREAK MAY AND MAY NOT DO: it may pick a different member of a
        // set of equally-cheap paths; it may NOT change what the cheapest cost is.
        // Both halves are asserted, over a sweep, because whether a given problem
        // even HAS a tie depends on the terrain.
        //
        // TIE_BREAK ONLY REACHES THE TRACE-FITTED ARM: under
        // `PATH_SEARCH = client16402` the order is the measured 16.402 one
        // (path16402.rs) and no knob may move it, so this test selects the arm the
        // knob belongs to.
        let a = Arena::shipped();
        let mut calibs = Vec::new();
        for t in [TieBreak::OrthoFirstPlaceholder, TieBreak::RowMajor, TieBreak::DiagFirst] {
            let mut c = Calib::shipped();
            c.path_search = PathSearch::TraceFittedAstar;
            c.tie_break = t;
            calibs.push((t, c));
        }
        let world = FrameWorld { arena: &a, obstacles: &[] };
        let cost_of = |f: &CostField, c: &Calib, cells: &[(i32, i32)], from: (i32, i32)| -> i64 {
            let mut total = 0i64;
            let mut prev = from;
            for &(col, row) in cells.iter().rev() {
                let mut step = f.at(f.idx(col, row));
                if step < 0 {
                    step = c.path_cost_default;
                }
                if col != prev.0 && row != prev.1 {
                    step = step * c.diag_num / c.diag_den;
                }
                total += step as i64;
                prev = (col, row);
            }
            total
        };
        // WHERE IT SHOWS, measured by sweep: over 6802 (start, goal, reach) problems
        // on the shipped grid only 25 come out differently -- the open-list key
        // (f, then the smaller h, then FIFO) already decides most of them, and the
        // neighbour order reaches the result ONLY through the push counter. So it
        // takes a case with a MULTI-CELL goal set to see it at all. This is the
        // first one the sweep finds; the second pair is a control with a single-cell
        // goal, where all three agree.
        let mut differing = 0;
        for (sc, sr, gc, gr, reach) in [(1, 44, 6, 48, 1000 * 18), (12, 12, 20, 20, 18)] {
            let pos = cell_centre(&a, sc, sr).add(Vec2::new(1, 1));
            let goal = cell_centre(&a, gc, gr);
            let mut seen: Vec<Vec<(i32, i32)>> = Vec::new();
            let mut costs: Vec<i64> = Vec::new();
            for (t, c) in &calibs {
                let (p, ok) = plan_cells(&world, c, &req(pos, goal, reach));
                assert!(ok && !p.is_empty(), "{t:?}: no path ({sc},{sr}) -> ({gc},{gr})");
                let f = CostField::terrain(&a, c);
                costs.push(cost_of(&f, c, &p, (sc, sr)));
                seen.push(p);
            }
            assert!(costs.iter().all(|c| *c == costs[0]), "the tie-break changed the COST: {costs:?}");
            if seen.iter().any(|p| *p != seen[0]) {
                differing += 1;
            }
        }
        assert_eq!(differing, 1, "the ledger's tie-break must reach the search, and must not change the cost");
    }

    #[test]
    fn occlusion_is_a_half_open_box_of_the_collision_radius() {
        // spec 4.1: a Cannon (CollisionRadius 600 native) at a tile centre blocks
        // cols (x-600)/500 ..= (x+599)/500. At x = 3500, y = 9500 that is cols 5..7,
        // rows 17..20.
        //
        // THE SHAPE IS ASSERTED THROUGH `in_building_box`, NOT THROUGH `refused`.
        // Under the measured OCCLUDED_CELL_TREATMENT = cost_50 an occluded cell is a
        // PRICE, so `refused` is false on every one of them and the shape assertion
        // would pass vacuously -- which is exactly how a wrong box would get through.
        let a = Arena::shipped();
        let c = Calib::shipped();
        let mut f = CostField::terrain(&a, &c);
        f.occlude(&a, &c, Vec2::new(3500 * 18, 9500 * 18), 600 * 18);
        // The box is [2900, 4100) x [8900, 10100) native, so it overlaps cols 5..8
        // and rows 17..20 -- col 8 starts at 4000, inside the box's open upper edge.
        for col in 5..=8 {
            for row in 17..=20 {
                assert!(f.in_building_box(f.idx(col, row)), "({col},{row})");
            }
        }
        assert!(!f.in_building_box(f.idx(4, 18)), "col 4 is outside the box");
        assert!(!f.in_building_box(f.idx(9, 18)), "col 9 is outside the box");
        assert!(!f.in_building_box(f.idx(6, 21)), "row 21 is outside the box");
        // HALF-OPEN, not closed: a closed box would reach col 9 ([4500, 5000) starts
        // at 4500 > 4100, so use the row edge, where 10100 lands exactly on row 20's
        // upper boundary) -- the discriminator measured on the princess tower, whose
        // 1000 radius puts both edges exactly on cell boundaries.
        let mut closed_probe = CostField::terrain(&a, &c);
        closed_probe.occlude(&a, &c, Vec2::new(3500 * 18, 6500 * 18), 1000 * 18);
        assert!(closed_probe.in_building_box(closed_probe.idx(5, 11)), "tower box lower edge");
        assert!(!closed_probe.in_building_box(closed_probe.idx(5, 15)), "a closed box would block row 15");

        // WHAT THE BOX COSTS, under both candidates the ledger lists: 50 under the
        // measured `cost_50` (PATHFINDING_BUILDING_COST, and the entered-cell cost is
        // what a diagonal multiplies), the goal-exempt sentinel under `block`.
        for (t, want) in [(OccludedCells::Cost50, c.path_cost_building), (OccludedCells::Block, OCCLUDED)] {
            let mut c2 = Calib::shipped();
            c2.occluded_cells = t;
            let mut g = CostField::terrain(&a, &c2);
            g.occlude(&a, &c2, Vec2::new(3500 * 18, 9500 * 18), 600 * 18);
            assert_eq!(g.at(g.idx(6, 18)), want, "{t:?}");
        }
        // PRECEDENCE OVER THE TERRAIN FLAG: a box over a
        // bit-16 cell costs what the box costs. Resolving terrain first and letting
        // the impassable flag win costs 39 of 337 live paths, because a king's box
        // IS the bit-16 king block cell for cell. The river is the one exception --
        // a box may not make water walkable.
        let mut king = CostField::terrain(&a, &c);
        assert_eq!(king.at(king.idx(17, 5)), OCCLUDED, "the king block starts as bit-16 terrain");
        king.occlude(&a, &c, Vec2::new(9000 * 18, 3000 * 18), 1400 * 18);
        assert_eq!(king.at(king.idx(17, 5)), c.path_cost_building, "the box cost must win over bit 16");
        let mut bridge = CostField::terrain(&a, &c);
        assert_eq!(bridge.at(bridge.idx(3, 31)), IMPASSABLE);
        bridge.occlude(&a, &c, Vec2::new(1750 * 18, 15750 * 18), 1000 * 18);
        assert_eq!(bridge.at(bridge.idx(3, 31)), IMPASSABLE, "a box may not make the river walkable");
    }

    #[test]
    fn the_heading_law_is_floored_and_truncated() {
        // The Knight's first moving tick in walk/Knight_x3.5_y8.5_seed2:
        // pos (3499, 8500), node centre (3250, 9250) -> dir (-80, 243), step
        // (-18, +56) at S = 60. Native units x 18 = subtiles.
        let d = Vec2::new((3250 - 3499) * 18, (9250 - 8500) * 18);
        let dir = norm256(d);
        assert_eq!(dir, Vec2::new(-80, 243));
        let s = step_delta(60 * 18, dir);
        assert_eq!(s, Vec2::new(-18 * 18, 56 * 18));
        // Rounding at SUBTILE granularity instead gives (-80, 242) and a step of
        // 1025 subtiles -- the whole reason `to_native` exists.
        assert_eq!(isqrt(d.len2()), 14224);
        assert_eq!(isqrt(Vec2::new(-249, 750).len2()), 790);
    }

    #[test]
    fn the_stomp_schedule_gives_the_giant_twelve_ticks_then_two() {
        // spec 7.4: Giant Stop 640 / Wait 100 -> move on k = 0..11, pause 12 and 13.
        let moved: Vec<u32> = (0..16).filter(|k| !stomp_paused(50, 640, 100, *k)).collect();
        assert_eq!(moved, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 14, 15]);
        // A card without the columns never pauses.
        assert!((0..40).all(|k| !stomp_paused(50, 0, 0, k)));
    }

    #[test]
    fn the_arrive_predicate_uses_the_frozen_segment() {
        let c = Calib::shipped();
        let radius = c.waypoint_arrive_radius;
        // The Giant's tick 137 in walk/Giant_x3.5_y8.5_seed2: post-move (3384, 9258),
        // node (6, 20) centre (3250, 10250), segment direction (-32, 253). The
        // recorded unit CONSUMES the node here; the Euclidean distance is 1001.0095, which
        // a plain radius of 1000 refuses.
        let node = Vec2::new(3250 * 18, 10250 * 18);
        let pos = Vec2::new(3384 * 18, 9258 * 18);
        let seg = Vec2::new(-32, 253);
        assert!(!arrived(WaypointArriveRule::EuclidPostMove, radius, node, pos, seg));
        assert!(arrived(WaypointArriveRule::SegmentProjection, radius, node, pos, seg));
    }

    #[test]
    fn an_occluded_cell_is_a_price_the_search_will_pay_and_a_block_is_not() {
        // THE 16.402 PROMOTION, as a test rather than as prose. `block` and `cost_50`
        // were indistinguishable on 15.535.29 -- no recorded path ever needed to cross a
        // building -- so nothing in this repo could tell them apart either, and a
        // ledger value nothing can falsify is the failure mode calibration.json's
        // own $comment exists to prevent.
        //
        // THE DISCRIMINATOR IS THE LIVE ONE: a short-reach unit walking at an enemy
        // KING. 97 of 785 live paths cross exactly one box-interior cell, always the
        // cell before the goal, always inside that king's R = 1400 box -- (17,8) x68
        // and (18,8) x17 one way, (18,55) x9 and (17,55) x3 the other. Here the
        // Skeletons' reach 1000 puts the shallowest in-reach cell at row 56, one cell
        // deep, so its only approaches are box cells: a block makes the whole problem
        // infeasible and a price does not.
        let a = Arena::shipped();
        let king = Vec2::new(9000 * 18, 29000 * 18);
        let obstacles = [crate::path::Obstacle {
            id: crate::EntityId { index: 7, generation: 0 },
            shape: crate::arena::Shape::Circle { c: king, r: 1400 * 18 },
            radius: 1400 * 18,
            key: (0, 0, 0, 0, 7),
            // ENEMY, and it still occludes: friendly-only fails 119 of 785 live
            // first paths and both sides fails 6.
            ally: false,
        }];
        let world = FrameWorld { arena: &a, obstacles: &obstacles };
        let start = Vec2::new(9000 * 18, 20000 * 18);

        // OCCLUDED_CELL_TREATMENT is a knob of the trace-fitted arm; the measured
        // 16.402 search prices every box at BUILDING cost unconditionally (path16402.rs).
        let mut cost50 = Calib::shipped();
        cost50.path_search = PathSearch::TraceFittedAstar;
        cost50.occluded_cells = OccludedCells::Cost50;
        let (p, ok) = plan_cells(&world, &cost50, &req(start, king, 1000 * 18));
        assert!(ok && !p.is_empty(), "cost_50: a melee unit must be able to walk at an enemy king");
        let f = CostField::for_mover(&world, &cost50, &req(start, king, 1000 * 18));
        let crossed: Vec<_> = p[1..].iter().filter(|&&(c, r)| f.in_building_box(f.idx(c, r))).collect();
        assert_eq!(crossed.len(), 1, "exactly one box cell before the goal, as live: {p:?}");
        assert_eq!(f.at(f.idx(crossed[0].0, crossed[0].1)), cost50.path_cost_building, "and it is priced at 50");
        // PRECEDENCE: that cell carries the tilemap's bit-16
        // king block, and the box cost is what overrides it. Resolving terrain first
        // and letting the impassable flag win costs 39 of 337 live paths.
        assert_eq!(
            CostField::terrain(&a, &cost50).at(f.idx(crossed[0].0, crossed[0].1)),
            OCCLUDED,
            "the cell the route crosses is bit-16 terrain underneath"
        );

        let mut block = Calib::shipped();
        block.path_search = PathSearch::TraceFittedAstar;
        block.occluded_cells = OccludedCells::Block;
        let (_q, ok2) = plan_cells(&world, &block, &req(start, king, 1000 * 18));
        assert!(!ok2, "block: the same problem has no route at all -- that is the 97 live paths it loses");

        // THE ENGINE'S TARGET EXEMPTION REPRODUCES THE BLOCK'S ANSWER, and that is
        // the open divergence calibration pathfinding.OCCLUSION_MODEL
        // `engine_divergence_target_building` records: with the king passed as
        // `ignore` its box is never stamped, so the bit-16 block underneath is all
        // that is left and the route is refused. Pinned here rather than left as
        // prose, because it is the one place the engine and the scored reference model that
        // scores 779/785 give different answers.
        let mut r = req(start, king, 1000 * 18);
        r.ignore = Some(obstacles[0].id);
        let (_s, ok3) = plan_cells(&world, &cost50, &r);
        assert!(!ok3, "if this starts passing, the divergence is closed -- update the ledger");
    }

    #[test]
    fn heuristic_matches_the_brute_force_minimum() {
        // The chamfer transform must equal min-over-the-goal-set h EXACTLY, or the
        // heuristic stops being the one spec 3.5 requires. Swept over BOTH forms the
        // ledger names: the transform is exact for any weights with
        // `straight <= diag <= 2*straight`, and both sit inside that window
        // (Chebyshev 5/5, octile 5/7), so neither may be assumed.
        let a = Arena::shipped();
        let n = (a.cols * a.rows) as usize;
        for form in [HeuristicForm::ChebyshevOverGoalSet, HeuristicForm::OctileOverGoalSet] {
            let mut c = Calib::shipped();
            c.heuristic_form = form;
            for reach in [1000 * 18, 1950 * 18, 6500 * 18] {
                let target = Vec2::new(3500 * 18, 25500 * 18);
                let mut mask = vec![false; n];
                let mut set = Vec::new();
                for row in 0..a.rows {
                    for col in 0..a.cols {
                        if in_reach(&a, col, row, target, reach) {
                            mask[(row * a.cols + col) as usize] = true;
                            set.push((col, row));
                        }
                    }
                }
                assert!(!set.is_empty());
                let h = goal_set_heuristic(&a, &c, &mask);
                for row in 0..a.rows {
                    for col in 0..a.cols {
                        let brute =
                            set.iter().map(|&(gc, gr)| chamfer_distance(&c, col - gc, row - gr)).min().unwrap();
                        assert_eq!(h[(row * a.cols + col) as usize], brute, "{form:?} reach {reach} ({col},{row})");
                    }
                }
            }
        }
    }

    #[test]
    fn the_measured_heuristic_is_chebyshev_and_both_forms_are_admissible() {
        // WHAT THE 16.402 PROMOTION MAY AND MAY NOT DO. It may change the expansion
        // order, which is the whole reason it was promoted (192/292 exact node
        // sequences against 78/292). It may NOT change what the cheapest path costs:
        // both forms are admissible against this cost field -- the cheapest cell is
        // road 5 and the cheapest diagonal 5*1414/1000 = 7, so 5*Chebyshev and the
        // octile form both under-estimate -- and an inadmissible h with
        // REOPEN_CLOSEDNODES = FALSE returns strictly dearer paths without saying so.
        let a = Arena::shipped();
        let shipped = Calib::shipped();
        assert_eq!(shipped.heuristic_form, HeuristicForm::ChebyshevOverGoalSet, "the ledger's measured value");
        assert_eq!(heuristic_weights(&shipped), (5, 5), "h = 5 * Chebyshev");
        let mut oct = Calib::shipped();
        oct.heuristic_form = HeuristicForm::OctileOverGoalSet;
        assert_eq!(heuristic_weights(&oct), (5, 7));

        let world = FrameWorld { arena: &a, obstacles: &[] };
        let cost_of = |c: &Calib, cells: &[(i32, i32)], from: (i32, i32)| -> i64 {
            let f = CostField::terrain(&a, c);
            let (mut total, mut prev) = (0i64, from);
            for &(col, row) in cells.iter().rev() {
                let mut step = f.at(f.idx(col, row));
                if step < 0 {
                    step = c.path_cost_default;
                }
                if col != prev.0 && row != prev.1 {
                    step = step * c.diag_num / c.diag_den;
                }
                total += step as i64;
                prev = (col, row);
            }
            total
        };
        for (sc, sr, gc, gr, reach) in
            [(1, 44, 6, 48, 1000 * 18), (12, 12, 20, 20, 18), (6, 17, 7, 47, 1950 * 18), (2, 50, 30, 14, 1400 * 18)]
        {
            let pos = cell_centre(&a, sc, sr).add(Vec2::new(1, 1));
            let goal = cell_centre(&a, gc, gr);
            let (p, ok) = plan_cells(&world, &shipped, &req(pos, goal, reach));
            let (q, ok2) = plan_cells(&world, &oct, &req(pos, goal, reach));
            assert!(ok && ok2 && !p.is_empty() && !q.is_empty(), "({sc},{sr}) -> ({gc},{gr})");
            assert_eq!(
                cost_of(&shipped, &p, (sc, sr)),
                cost_of(&oct, &q, (sc, sr)),
                "the heuristic changed the COST on ({sc},{sr}) -> ({gc},{gr})\n  chebyshev {p:?}\n  octile    {q:?}"
            );
        }
    }

    #[test]
    fn a_plan_is_the_rotation_of_the_rotated_plan() {
        // SEAT SYMMETRY, directly: plan a problem, then plan the SAME problem rotated
        // 180 degrees about the arena centre, and the second plan must be the first
        // one's cells rotated. That is what makes planning in the team's frame safe
        // (module header): a Red unit's plan is its Blue twin's rotated.
        //
        // This test used to apply `rot` TWICE, which is the identity -- it planned the
        // same problem twice and asserted determinism. Nothing was checking the
        // rotation at the plan level.
        let a = Arena::shipped();
        let c = Calib::shipped();
        let world = FrameWorld { arena: &a, obstacles: &[] };
        let rot = |v: Vec2| Vec2::new(a.width - v.x, a.height - v.y);
        // The cell rotation the position rotation induces: `width = cols*cell`, so
        // `width - x` lands in column `cols - 1 - col` -- FOR ANY x STRICTLY INSIDE
        // the column. On a cell boundary it does not: `floor(x/c)` and
        // `floor((W-x)/c)` both round the same way, so a start exactly on a boundary
        // rotates into the NEXT cell and the plans differ by that one cell. That is
        // a property of the half-open cell convention, not of the planner, and it is
        // why the engine rotates the POSITION into the team's frame and plans there
        // (module header) rather than rotating cells. The starts below are chosen
        // strictly inside their cells so the cell map is exactly rotation-symmetric.
        let rot_cell = |(col, row): (i32, i32)| (a.cols - 1 - col, a.rows - 1 - row);

        for (pos, goal, reach) in [
            (Vec2::new(3499 * 18, 8501 * 18), Vec2::new(3500 * 18, 25500 * 18), 1700 * 18),
            (Vec2::new(1499 * 18, 2501 * 18), Vec2::new(14500 * 18, 25500 * 18), 1950 * 18),
        ] {
            assert_ne!(pos.x % a.cell, 0, "the start must not sit on a cell boundary");
            assert_ne!(pos.y % a.cell, 0, "the start must not sit on a cell boundary");
            let (p, ok) = plan_cells(&world, &c, &req(pos, goal, reach));
            assert!(ok && !p.is_empty());
            let (q, ok2) = plan_cells(&world, &c, &req(rot(pos), rot(goal), reach));
            assert!(ok2, "the rotated problem has no plan");
            let rotated: Vec<(i32, i32)> = p.iter().map(|&cell| rot_cell(cell)).collect();
            assert_eq!(
                q, rotated,
                "the rotated problem's plan is not the rotation of the plan\n  rot(p) {rotated:?}\n  q      {q:?}"
            );
        }
    }

    #[test]
    fn the_walk_plan_is_the_oracles_column_six() {
        // walk/*: deploy (3500, 8500) native, target the enemy left princess tower
        // at (3500, 25500). The recorded path is column 6 from row 19 up; the plan
        // here is the same column from row 18 (rule 7.5 pops (6, 18) on the first
        // moving tick, which is why the recorded list starts at 19).
        let a = Arena::shipped();
        let c = Calib::shipped();
        let world = FrameWorld { arena: &a, obstacles: &[] };
        let pos = Vec2::new(3499 * 18, 8500 * 18);
        let goal = Vec2::new(3500 * 18, 25500 * 18);
        for (reach_native, goal_row) in [(1950, 47), (1700, 48), (1500, 48), (1400, 48), (1250, 49), (1000, 49)] {
            let (p, ok) = plan_cells(&world, &c, &req(pos, goal, reach_native * 18));
            assert!(ok, "reach {reach_native}");
            assert_eq!(p[0], (6, goal_row), "reach {reach_native} goal cell");
            assert_eq!(*p.last().unwrap(), (6, 18), "reach {reach_native} first step");
            assert!(p.iter().all(|(col, _)| *col == 6), "reach {reach_native} stays in column 6");
        }
    }
}
