//! THE 16.402 PATHFINDER AS MEASURED, reproducing the live client node for node.
//!
//! WHAT IT REPRODUCES
//!     The published first-path node lists of the live 16.402 trace corpus
//!     (**435 of 438**) and of the offline 15.535 corpus (**128 of 128**), exactly.
//!     The three misses are units chasing a MOVING target whose goal cell the trace
//!     cannot recover: the sample carries the target's position from the previous
//!     tick. The trace-fitted search in path2026.rs reproduces 168 of 345.
//!
//! WHAT THE SEARCH DOES, in one paragraph. A walking unit asks for ONE goal cell
//! (`choose_goal_cell` below): the cell within `reach` of the
//! target centre that is not water and not inside a building box if any such cell
//! exists, nearest to the MOVER, scanned rows-ascending and columns ascending or
//! descending by which half of the arena the mover stands in. `find_path` then runs
//! `search`: a textbook A* over the 36x64 grid with a binary min-heap of
//! cell indices keyed on f alone, neighbours tried N, S, W, E, NW, SW, SE, NE, step
//! cost = the ENTERED cell's cost x 10 (x 14 on a diagonal), and
//! h = 5 x (10 max(dx,dy) + 4 min(dx,dy)) (HEURISTIC_METHOD 1). Cells cost
//! 5 on either lane, 8 elsewhere, 50 on water (BLOCKED, for walkers) and max(base, 50)
//! inside any building's box, both sides. Closed nodes are never reopened
//! (REOPEN_CLOSEDNODES = FALSE). The path is the parent chain from the goal, goal
//! first, without the start cell.
//!
//! WHY EVERY DETAIL MATTERS. All 425 live first paths are cost-optimal under the
//! cost model measured from traces alone, and 331 of them have several equally
//! cheap routes. Among equally cheap routes, the published route is the one this
//! ordering reproduces, and every element of the ordering is scored against the
//! corpus: the x10/x14 scaling (a plain diagonal is 112, not 11), water priced at 50
//! and expanded like any other cell rather than treated as impassable (306/388 when
//! it is impassable), the order neighbours are tried in, and the single goal cell (a
//! goal SET changes which expansion ends the search). None of it is seat-symmetric:
//! the scan and neighbour orders are in
//! ABSOLUTE arena coordinates, so a Red unit's path is NOT the rotation of its Blue
//! twin's -- visible in the corpus wherever the two sides' paths can be paired. This module
//! therefore plans in absolute coordinates and `path2026::plan_cells` un-rotates a
//! Red request before calling it.
//!
//! UNITS. Everything here is in the traces' native units (1 tile = 1000, one grid
//! cell = 500); only the boundary converts from the engine's
//! subtiles (18 per native unit).
#![allow(unexpected_cfgs)]

/// Sentinel the heap array is filled with between searches.
const HEAP_FILL: i32 = 0x07FF_FFFF;
const NO_PARENT: i32 = -1;
/// Cell size in native units (500 throughout).
pub const CELL: i32 = 500;

/// Integer division truncating toward zero.
#[inline]
fn div_trunc(a: i32, b: i32) -> i32 {
    a / b // Rust integer division also truncates toward zero
}

/// `(x1-x2)^2 + (y1-y2)^2`, INT_MAX when either delta is outside [-46340, 46340] or
/// the sum would overflow (guarded against 32-bit overflow).
#[inline]
pub fn dist2_capped(x1: i32, y1: i32, x2: i32, y2: i32) -> i32 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    if !(-46340..=46340).contains(&dx) || !(-46340..=46340).contains(&dy) {
        return i32::MAX;
    }
    let (dx2, dy2) = (dx * dx, dy * dy);
    if dy2 < i32::MAX - dx2 {
        dx2 + dy2
    } else {
        i32::MAX
    }
}

/// The per-cell terrain the cost function reads: the tilemap's lane bits (`& 3`) and
/// its water bit (`& 0x20`). No cell of the arena tilemap in `data/raw` sets bit 0x40,
/// and bit 16 (the king block / edge strips) does not change any price -- the king
/// block comes out of the king tower's own occlusion box.
#[derive(Clone, Debug)]
pub struct Terrain {
    pub cols: i32,
    pub rows: i32,
    /// 5 on a lane cell, 8 elsewhere, 50 on water.
    pub base: Vec<i32>,
    pub water: Vec<bool>,
}

impl Terrain {
    pub fn new(cols: i32, rows: i32, is_lane: impl Fn(i32, i32) -> bool, is_water: impl Fn(i32, i32) -> bool, costs: &Costs) -> Terrain {
        let n = (cols * rows) as usize;
        let mut base = vec![costs.default; n];
        let mut water = vec![false; n];
        for row in 0..rows {
            for col in 0..cols {
                let i = (row * cols + col) as usize;
                if is_water(col, row) {
                    // walkers pay BLOCKED for water, and the occlusion max below is
                    // skipped for it (the function returns early).
                    base[i] = costs.blocked;
                    water[i] = true;
                } else if is_lane(col, row) {
                    // ROAD and MATCHINGROAD are both 5: which lane is the unit's own
                    // does not change the price.
                    base[i] = costs.road;
                }
            }
        }
        Terrain { cols, rows, base, water }
    }
}

/// The five cell prices (the pathfinding globals) plus the heuristic weight.
#[derive(Clone, Copy, Debug)]
pub struct Costs {
    pub water: i32,
    pub blocked: i32,
    pub building: i32,
    pub default: i32,
    pub road: i32,
    pub heuristic: i32,
}

impl Costs {
    /// The globals.csv values.
    pub const GLOBALS: Costs = Costs { water: 7, blocked: 50, building: 50, default: 8, road: 5, heuristic: 5 };
}

/// The cell box one building marks: centre snapped UP to the next
/// multiple of 500 on each axis, then the half-open box `[c - r, c + r)`, inclusive
/// cell bounds. A box that does not fit ENTIRELY inside the grid marks nothing.
/// Returns `(col_lo, col_hi, row_lo, row_hi)`.
pub fn occlusion_box(x: i32, y: i32, rx: i32, ry: i32, cols: i32, rows: i32) -> Option<(i32, i32, i32, i32)> {
    let c500 = div_trunc(x - 1, CELL) * CELL + CELL;
    let r500 = div_trunc(y - 1, CELL) * CELL + CELL;
    let xl = c500 - rx;
    if xl < 0 {
        return None;
    }
    let yl = r500 - ry;
    if yl < 0 {
        return None;
    }
    if c500 + rx >= cols * CELL {
        return None;
    }
    if r500 + ry >= rows * CELL {
        return None;
    }
    Some((xl / CELL, div_trunc(c500 + rx - 1, CELL), yl / CELL, div_trunc(r500 + ry - 1, CELL)))
}

/// Stamp `cost` over the box: `occ[i] = max(occ[i], cost)`.
pub fn stamp(occ: &mut [i32], cols: i32, b: Option<(i32, i32, i32, i32)>, cost: i32) {
    let Some((c0, c1, r0, r1)) = b else { return };
    if r0 > r1 || c0 > c1 {
        return;
    }
    for row in r0..=r1 {
        for col in c0..=c1 {
            let i = (row * cols + col) as usize;
            if cost > occ[i] {
                occ[i] = cost;
            }
        }
    }
}

/// The price of ENTERING a cell, or -1 out of bounds (the only case that is -1).
#[inline]
pub fn cell_cost(t: &Terrain, occ: &[i32], col: i32, row: i32) -> i32 {
    if col < 0 || row < 0 || t.cols <= col || t.rows <= row {
        return -1;
    }
    let i = (row * t.cols + col) as usize;
    if t.water[i] {
        return t.base[i]; // returns before the occlusion max
    }
    let o = occ[i];
    if o > t.base[i] {
        o
    } else {
        t.base[i]
    }
}

/// The ONE cell the search is handed. `actor` and `target` in native units, `reach` = Range + the
/// mover's own CollisionRadius. `avoid_buildings` follows the
/// datamined global `KS_POS_TO_TARGET_GROUND_AVOID_BUILDINGS`, and holding it true
/// scores 435/438 on the corpus against 421/425 without it.
///
/// Scans rows ascending over `target_cell +- (reach/500 + 1)`, columns ascending
/// when the ACTOR's x is in the left half of the arena and descending otherwise;
/// keeps the highest category (2 = dry and unboxed, 1 = water or boxed) and within
/// it the strictly smallest squared distance to the ACTOR, so the first scanned
/// cell wins a tie.
pub fn choose_goal_cell(
    t: &Terrain,
    occ: &[i32],
    actor: (i32, i32),
    target: (i32, i32),
    reach: i32,
    avoid_buildings: bool,
    building_cost: i32,
) -> Option<(i32, i32)> {
    let (ax, ay) = actor;
    let (tx, ty) = target;
    let tcol = div_trunc(tx, CELL);
    let trow = div_trunc(ty, CELL);
    let rr = div_trunc(reach, CELL);
    let col_lo = (tcol - (rr + 1)).max(0);
    let col_hi = (tcol + rr + 1).min(t.cols - 1);
    let row_lo = (trow - (rr + 1)).max(0);
    let row_hi = (trow + rr + 1).min(t.rows - 1);
    if row_lo > row_hi || col_lo > col_hi {
        return None;
    }
    let reach2 = reach * reach;
    let mut best_cat = 0;
    let mut best_d = i32::MAX;
    let mut best: Option<(i32, i32)> = None;
    let ascending = ax < t.cols * (CELL / 2);
    let mut visit = |col: i32, row: i32| {
        let cx = col * CELL + CELL / 2;
        let cy = row * CELL + CELL / 2;
        if dist2_capped(tx, ty, cx, cy) > reach2 {
            return;
        }
        let d_actor = dist2_capped(cx, cy, ax, ay);
        let i = (row * t.cols + col) as usize;
        let cat = if t.water[i] {
            1
        } else if avoid_buildings && occ[i] >= building_cost {
            1
        } else {
            2
        };
        if cat > best_cat {
            best_cat = cat;
            best = Some((col, row));
            best_d = d_actor;
        } else if cat == best_cat && d_actor < best_d {
            best = Some((col, row));
            best_d = d_actor;
        }
    };
    for row in row_lo..=row_hi {
        if ascending {
            for col in col_lo..=col_hi {
                visit(col, row);
            }
        } else {
            for col in (col_lo..=col_hi).rev() {
                visit(col, row);
            }
        }
    }
    if best_d == i32::MAX {
        return None;
    }
    best
}

/// The search state.
#[derive(Clone, Debug)]
pub struct PathFinder {
    cols: i32,
    rows: i32,
    heuristic_cost: i32,
    /// 0 unvisited, 1 open (in the heap), 2 closed.
    state: Vec<u8>,
    /// Kept between searches; `search` resets the start's and the goal's entries.
    parent: Vec<i32>,
    /// Accumulated step cost.
    g: Vec<i32>,
    /// g + h x heuristicCost, the heap key.
    f: Vec<i32>,
    /// Binary min-heap of cell indices, and its size.
    heap: Vec<i32>,
    heap_size: i32,
    /// The result, goal first.
    path: Vec<i32>,
    path_len: usize,
    goal_col: i32,
    goal_row: i32,
}

impl PathFinder {
    pub fn new(cols: i32, rows: i32, heuristic_cost: i32) -> PathFinder {
        let n = (cols * rows) as usize;
        PathFinder {
            cols,
            rows,
            heuristic_cost,
            state: vec![0; n],
            parent: vec![0; n],
            g: vec![0; n],
            f: vec![0; n],
            heap: vec![0; n],
            heap_size: 0,
            path: vec![0; n],
            path_len: 0,
            goal_col: -1,
            goal_row: -1,
        }
    }

    /// HEURISTIC_METHOD = 1: `10 (dx + dy) - 6 min(dx, dy)`, i.e. an
    /// octile estimate with a 14/10 diagonal, in the same x10 units as the step cost.
    #[inline]
    fn heuristic(&self, col: i32, row: i32) -> i32 {
        let dx = (self.goal_col - col).abs();
        let dy = (self.goal_row - row).abs();
        (dx + dy) * 10 - dx.min(dy) * 6
    }

    /// Sift-up: swap upward only on STRICT `<` (equal keys stop it).
    fn sift_up(&mut self, mut pos: i32) {
        if pos <= 0 {
            return;
        }
        let node = self.heap[pos as usize];
        loop {
            let r8 = pos - 1;
            let par = r8 >> 1;
            if self.f[node as usize] >= self.f[self.heap[par as usize] as usize] {
                return;
            }
            self.heap[pos as usize] = self.heap[par as usize];
            self.heap[par as usize] = node;
            pos = par;
            if r8 <= 1 {
                return;
            }
        }
    }

    /// `heap[size++] = idx; sift up`.
    fn push(&mut self, idx: i32) {
        let pos = self.heap_size;
        self.heap_size = pos + 1;
        self.heap[pos as usize] = idx;
        self.sift_up(pos);
    }

    /// Pop: remove the root, move the last element up, sift down.
    /// RIGHT child is compared first, both comparisons are STRICT `>`, so on a tie
    /// the element stays where it is.
    fn pop_min(&mut self) -> i32 {
        let top = self.heap[0];
        let size = self.heap_size - 1;
        self.heap_size = size;
        let moved = self.heap[size as usize];
        self.heap[0] = moved;
        let mut pos = 0;
        loop {
            let right = 2 * pos + 2;
            let mut best = pos;
            if right < size && self.f[moved as usize] > self.f[self.heap[right as usize] as usize] {
                best = right;
            }
            let left = 2 * pos + 1;
            if left < size && self.f[self.heap[best as usize] as usize] > self.f[self.heap[left as usize] as usize] {
                best = left;
            }
            if best == pos {
                break;
            }
            self.heap[pos as usize] = self.heap[best as usize];
            self.heap[best as usize] = moved;
            pos = best;
        }
        top
    }

    /// Relax one neighbour under the datamined globals (REFRESH_OPENNODES TRUE,
    /// REOPEN_CLOSEDNODES FALSE, NEW_PATHFINDING_CODE TRUE).
    fn relax(&mut self, from: i32, ncol: i32, nrow: i32, nidx: i32, factor: i32, cost: &impl Fn(i32, i32) -> i32) {
        if nrow < 0 || ncol < 0 || self.rows <= nrow || self.cols <= ncol {
            return;
        }
        let c = cost(ncol, nrow);
        if c == -1 {
            return;
        }
        let n = nidx as usize;
        match self.state[n] {
            0 => {
                let hh = self.heuristic(ncol, nrow) * self.heuristic_cost;
                let gg = c * factor + self.g[from as usize];
                self.state[n] = 1;
                self.parent[n] = from;
                self.g[n] = gg;
                self.f[n] = hh + gg;
                self.push(nidx);
            }
            2 => {} // REOPEN_CLOSEDNODES = FALSE: a closed node is never revisited
            _ => {
                // open: refresh on a STRICT improvement of f
                let hh = self.heuristic(ncol, nrow) * self.heuristic_cost;
                let gg = c * factor + self.g[from as usize];
                let ff = hh + gg;
                if ff >= self.f[n] {
                    return;
                }
                self.parent[n] = from;
                self.g[n] = gg;
                self.f[n] = ff;
                let size = self.heap_size;
                if size <= 0 {
                    return;
                }
                let mut pos = -1;
                for i in 0..size {
                    if self.heap[i as usize] == nidx {
                        pos = i;
                        break;
                    }
                }
                if pos > 0 {
                    self.sift_up(pos);
                }
            }
        }
    }

    /// The eight neighbours, in this order, orthogonals at factor 10 and diagonals
    /// at 14.
    fn expand(&mut self, idx: i32, cost: &impl Fn(i32, i32) -> i32) {
        let w = self.cols;
        let row = div_trunc(idx, w);
        let col = idx - row * w;
        self.relax(idx, col, row - 1, idx - w, 10, cost); // N
        self.relax(idx, col, row + 1, idx + w, 10, cost); // S
        self.relax(idx, col - 1, row, idx - 1, 10, cost); // W
        self.relax(idx, col + 1, row, idx + 1, 10, cost); // E
        self.relax(idx, col - 1, row - 1, idx - w - 1, 14, cost); // NW
        self.relax(idx, col - 1, row + 1, idx - 1 + w, 14, cost); // SW
        self.relax(idx, col + 1, row + 1, idx + 1 + w, 14, cost); // SE
        self.relax(idx, col + 1, row - 1, idx + 1 - w, 14, cost); // NE
    }

    /// The search. Returns the parent chain from the
    /// goal, GOAL FIRST, stopping before the start cell; empty when the goal was
    /// not reached. `goal` must be a valid cell index here (there is no flood mode).
    pub fn search(&mut self, start: i32, goal: i32, cost: &impl Fn(i32, i32) -> i32) -> &[i32] {
        let w = self.cols;
        self.goal_row = div_trunc(goal, w);
        self.goal_col = goal - self.goal_row * w;
        self.heap_size = 0;
        for i in 0..self.state.len() {
            self.state[i] = 0;
            self.g[i] = 0;
            self.f[i] = 0;
            self.heap[i] = HEAP_FILL;
        }
        self.path_len = 1;
        self.path[0] = start;
        self.parent[start as usize] = NO_PARENT;
        self.parent[goal as usize] = NO_PARENT;
        self.expand(start, cost); // before the start is closed
        self.state[start as usize] = 2;
        if self.heap_size != 0 {
            loop {
                let cur = self.pop_min();
                self.state[cur as usize] = 2;
                self.expand(cur, cost);
                if self.state[goal as usize] == 2 {
                    break; // the goal has been popped AND expanded
                }
                if self.heap_size <= 0 {
                    break;
                }
            }
        }
        self.path_len = 0;
        let mut p = self.parent[goal as usize];
        if p != NO_PARENT {
            let mut node = goal;
            loop {
                self.path[self.path_len] = node;
                self.path_len += 1;
                node = p;
                p = self.parent[node as usize];
                if p == NO_PARENT {
                    break;
                }
            }
        }
        &self.path[..self.path_len]
    }

    /// The path request: an in-grid start and goal go straight to `search`; an
    /// out-of-grid goal is snapped to the nearest in-grid cell inside a square of
    /// half-side isqrt(dist2) when `nearest` (rows outer, cols inner, strict `<`,
    /// first wins). Returns the goal-first chain, empty on any failure.
    pub fn find_path(&mut self, scol: i32, srow: i32, mut gcol: i32, mut grow: i32, nearest: bool, cost: &impl Fn(i32, i32) -> i32) -> &[i32] {
        let (w, h) = (self.cols, self.rows);
        if scol < 0 || srow < 0 || scol >= w || srow >= h {
            self.path_len = 0;
            return &self.path[..0];
        }
        if cost(gcol, grow) == -1 {
            if !nearest {
                self.path_len = 0;
                return &self.path[..0];
            }
            let r = crate::fixed::isqrt(((gcol - scol) * (gcol - scol) + (grow - srow) * (grow - srow)) as i64) as i32;
            let clamp = |v: i32, hi: i32| if v <= 0 { 0 } else { v.min(hi) };
            let (clo, chi) = (clamp(gcol - r, w), clamp(gcol + r, w));
            let (rlo, rhi) = (clamp(grow - r, h), clamp(grow + r, h));
            if rlo >= rhi || clo >= chi {
                self.path_len = 0;
                return &self.path[..0];
            }
            let mut best = i32::MAX;
            let (mut bc, mut br) = (-1, -1);
            for row in rlo..rhi {
                let dy2 = (row - grow) * (row - grow);
                for col in clo..chi {
                    if cost(col, row) == -1 {
                        continue;
                    }
                    let d = (col - gcol) * (col - gcol) + dy2;
                    if d < best {
                        best = d;
                        bc = col;
                        br = row;
                    }
                }
            }
            if bc == -1 {
                self.path_len = 0;
                return &self.path[..0];
            }
            gcol = bc;
            grow = br;
        }
        self.search(srow * w + scol, grow * w + gcol, cost)
    }
}

/// The SAMEPATH decision. After a replan forced only by the occluder set changing (goal
/// cell unchanged), the unit keeps the OLD list unless this says the
/// change actually touched it: an old node that is boxed now and was not before,
/// or a new node that was boxed before and is free now. `prev` is the array
/// stamped on the previous tick, `cur` this tick's; an empty list on either side
/// counts as changed.
pub fn path_touches_changed_occlusion(prev: &[i32], cur: &[i32], old: &[i32], new: &[i32]) -> bool {
    if old.is_empty() || new.is_empty() {
        return true;
    }
    let n = prev.len().min(cur.len());
    for &c in old {
        let c = c as usize;
        if c < n && prev[c] == 0 && cur[c] > 0 {
            return true;
        }
    }
    for &c in new {
        let c = c as usize;
        if c < n && prev[c] > 0 && cur[c] == 0 {
            return true;
        }
    }
    false
}

/// One building as the grid sees it: absolute native centre and CollisionRadius.
#[derive(Clone, Copy, Debug)]
pub struct Occluder {
    pub x: i32,
    pub y: i32,
    pub r: i32,
}

/// The whole request a walking unit makes: absolute native units.
#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub actor: (i32, i32),
    pub target: (i32, i32),
    /// Range + the mover's own CollisionRadius.
    pub reach: i32,
    pub avoid_buildings: bool,
}

/// What the walking unit gets back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// No cell within reach of the target satisfies the goal rule.
    NoGoal,
    /// The mover's own cell is the goal cell: nothing to walk.
    Arrived,
    /// The search found no route (cannot happen on the arena in `data/`: every cell is
    /// priced, none is impassable).
    Unreachable,
    /// Cells GOAL FIRST, without the start cell.
    Route(Vec<(i32, i32)>),
}

/// The occlusion array for one plan: every building of BOTH sides stamped at
/// BUILDING cost through the snapped half-open box (rebuilt per tick from every
/// building of both sides, as measured).
pub fn occlusion(t: &Terrain, occluders: &[Occluder], costs: &Costs) -> Vec<i32> {
    let mut occ = vec![0; (t.cols * t.rows) as usize];
    for o in occluders {
        stamp(&mut occ, t.cols, occlusion_box(o.x, o.y, o.r, o.r, t.cols, t.rows), costs.building);
    }
    occ
}

/// One walking unit's whole request, start to published list.
pub fn plan(t: &Terrain, occ: &[i32], pf: &mut PathFinder, req: &Request, costs: &Costs) -> Plan {
    let Some((gcol, grow)) = choose_goal_cell(t, occ, req.actor, req.target, req.reach, req.avoid_buildings, costs.building) else {
        return Plan::NoGoal;
    };
    let scol = div_trunc(req.actor.0, CELL);
    let srow = div_trunc(req.actor.1, CELL);
    if (scol, srow) == (gcol, grow) {
        return Plan::Arrived;
    }
    let cost = |c: i32, r: i32| cell_cost(t, occ, c, r);
    let cols = t.cols;
    let chain = pf.find_path(scol, srow, gcol, grow, true, &cost);
    if chain.is_empty() {
        return Plan::Unreachable;
    }
    // the published list carries no consecutive duplicates
    let mut out: Vec<(i32, i32)> = Vec::with_capacity(chain.len());
    let mut prev = -1;
    for &n in chain {
        if n != prev {
            out.push((n % cols, n / cols));
        }
        prev = n;
    }
    Plan::Route(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(cols: i32, rows: i32) -> Terrain {
        Terrain::new(cols, rows, |_, _| false, |_, _| false, &Costs::GLOBALS)
    }

    #[test]
    fn the_box_is_half_open_and_snaps_the_centre_up() {
        // a Tesla the game snapped to the vertex (14000, 12000), R 500: cols 27-28,
        // rows 23-24 (battle H sc6b, the designed witness)
        assert_eq!(occlusion_box(14000, 12000, 500, 500, 36, 64), Some((27, 28, 23, 24)));
        // a Tombstone on a tile centre, R 1000: four cells each way
        assert_eq!(occlusion_box(14500, 19500, 1000, 1000, 36, 64), Some((27, 30, 37, 40)));
        // a building whose box would leave the grid marks nothing at all
        assert_eq!(occlusion_box(17500, 8500, 500, 500, 36, 64), None);
        // an off-centre building is snapped UP to the next multiple of 500 first
        assert_eq!(occlusion_box(3250, 3250, 500, 500, 36, 64), occlusion_box(3500, 3500, 500, 500, 36, 64));
    }

    #[test]
    fn water_is_priced_not_refused_and_ignores_the_box() {
        let t = Terrain::new(4, 4, |c, _| c == 0, |_, r| r == 1, &Costs::GLOBALS);
        let mut occ = vec![0; 16];
        stamp(&mut occ, 4, Some((0, 3, 0, 3)), 50);
        assert_eq!(cell_cost(&t, &occ, 0, 1), 50);
        assert_eq!(cell_cost(&t, &occ, 0, 0), 50);
        assert_eq!(cell_cost(&t, &vec![0; 16], 0, 0), 5);
        assert_eq!(cell_cost(&t, &vec![0; 16], 1, 0), 8);
        assert_eq!(cell_cost(&t, &occ, 4, 0), -1);
    }

    #[test]
    fn heap_ties_stay_put_and_the_right_child_is_tried_first() {
        // three equal keys: the pop order is the heap's, not FIFO
        let mut pf = PathFinder::new(4, 4, 5);
        for (i, k) in [(0, 7), (1, 7), (2, 7), (3, 3)] {
            pf.f[i as usize] = k;
            pf.push(i);
        }
        // pushes: [0], [0,1] (equal key does not move up), [0,1,2], then 3 sifts to
        // the root through slot 1: [3,0,2,1]
        assert_eq!(&pf.heap[..4], &[3, 0, 2, 1]);
        // pop 3: slot 3 (cell 1) moves to the root; right child (cell 2) and left
        // child (cell 0) both tie it, strict `>` never swaps -> cell 1 is next
        assert_eq!(pf.pop_min(), 3);
        assert_eq!(&pf.heap[..3], &[1, 0, 2]);
        assert_eq!(pf.pop_min(), 1);
        assert_eq!(pf.pop_min(), 2);
        assert_eq!(pf.pop_min(), 0);
        assert_eq!(pf.heap_size, 0);
    }

    #[test]
    fn a_straight_walk_on_a_flat_grid_is_the_parent_chain_goal_first() {
        let t = flat(6, 6);
        let occ = vec![0; 36];
        let mut pf = PathFinder::new(6, 6, 5);
        let cost = |c: i32, r: i32| cell_cost(&t, &occ, c, r);
        let chain = pf.search(0, 3, &cost).to_vec();
        assert_eq!(chain, vec![3, 2, 1]);
    }

    #[test]
    fn the_samepath_test_only_fires_when_the_change_touched_a_list() {
        // prev: nothing boxed; cur: cell 7 boxed
        let prev = vec![0; 16];
        let mut cur = vec![0; 16];
        cur[7] = 50;
        assert!(path_touches_changed_occlusion(&prev, &cur, &[3, 7, 11], &[3, 6, 11]), "an old node got boxed");
        assert!(!path_touches_changed_occlusion(&prev, &cur, &[3, 6, 11], &[3, 6, 11]), "neither list touches the change");
        // a new node that was boxed before and is free now also counts
        assert!(path_touches_changed_occlusion(&cur, &prev, &[3, 6, 11], &[3, 7, 11]), "a new node got freed");
        assert!(path_touches_changed_occlusion(&prev, &cur, &[], &[3]), "an empty list is a change");
    }

    #[test]
    fn the_goal_cell_prefers_dry_unboxed_cells_nearest_the_mover() {
        let t = flat(36, 64);
        let mut occ = vec![0; 36 * 64];
        // a princess tower at (3500, 25500), R 1000, and a Knight (reach 1700) walking
        // up from below: the goal is a clean cell just outside the box, nearest the mover
        stamp(&mut occ, 36, occlusion_box(3500, 25500, 1000, 1000, 36, 64), 50);
        let g = choose_goal_cell(&t, &occ, (3250, 9750), (3500, 25500), 1700, true, 50).unwrap();
        assert!(occ[(g.1 * 36 + g.0) as usize] < 50, "goal {g:?} sits in the tower box");
        let c = (g.0 * 500 + 250, g.1 * 500 + 250);
        assert!(dist2_capped(c.0, c.1, 3500, 25500) <= 1700 * 1700);
        // without avoidance the nearest in-reach cell wins whatever the box says
        let g2 = choose_goal_cell(&t, &occ, (3250, 9750), (3500, 25500), 1700, false, 50).unwrap();
        assert!(g2.1 <= g.1);
    }
}
