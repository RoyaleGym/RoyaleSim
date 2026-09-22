//! THE 16.402 CONTACT AND LOCOMOTION LAW AS MEASURED: one movement update of a
//! ground unit, settled against every live capture of client 16.402.
//!
//! WHAT THE LAW DOES. Each tick every attack update runs, then every MOVE update in
//! creation order -- so unit i sees units j < i already moved and j > i where they
//! started -- while which units count as neighbours is decided on the positions
//! frozen at the start of the tick. A walking unit's update (the walk update):
//!   1. avoidance scan: every entity whose circle overlaps the circle of
//!      radius min(R, 500) around the look-ahead point `pos + facing` (facing has
//!      length 256) votes on which way to steer; statics win over movers, the last
//!      seen wins; the offset is set to +-200 or moved +-20 and clamped to +-200;
//!   2. decay: |offset| -= 10 toward zero, every walk tick;
//!   3. separation scan: every overlapping neighbour (centre distance <=
//!      R_me + R_e, both sides, buildings and towers included) adds a push of
//!      `min(299, trunc(min(overlap, 300) x M_e / M_me)) + 1` away from it to an
//!      accumulator, one count per neighbour;
//!   4. the step toward the current waypoint's centre: heading
//!      `trunc((aim - pos) << 8 / dist)`, step `trunc(heading x min(speed, dist, 250)
//!      / 256)` per axis, rotated by the offset (`v' = ((256 - |a|) v + a perp(v)) >> 8`,
//!      renormalised to the step length), plus the accumulator's mean capped at 150,
//!      written to the position, then the reached test
//!      `proj(aim - pos', segment) <= 1000` that pops the waypoint.
//!
//! THE KNOCKBACK LADDER (calibration knockback.DISPLACEMENT_LAW = client16402;
//! measured on the live 16.402 captures, settled step for step on the Giant of
//! capture 20260918-122757.b1 ticks 1216..1223): a landing push does not displace
//! the unit, it ARMS a countdown. `start_pushback` aims a target point `L` away from
//! the UNIT, further along the source-to-unit line (`target = unit + (tdiv(dx x L, d),
//! tdiv(dy x L, d))` with `(dx, dy) = unit - source`; `L = min(strength,
//! MAX_PUSHBACK_LENGTH)`) and loads the speed `25n` with `n` the smallest
//! `25n(n+1)/2 >= L`; every tick after that `pushback_step` REPLACES the walk: the
//! separation scan still runs (the unit is pushed by and pushes its neighbours),
//! the speed drops by 25 BEFORE the move, `move_towards(target, speed)` takes a
//! step of `min(speed, dist, 250)` -- so the ladder reads 150, 125, ..., 25, 0 and
//! then ONE 25-UNIT STEP BACKWARD (a negative speed is not clamped) -- with the
//! avoidance offset rotating the step but neither scanned nor decayed and the
//! facing untouched; the ladder is active while the speed is `>= 0`, and the tick
//! it turns negative drops the path (a replan follows). `start_pushback`,
//! `ladder_speed`, `pushback_step`, `blocked_or_water` and `nearest_land` below are
//! that law; state.rs `apply_effects` arms it and `phase_path16402` runs it.
//!
//! UNITS: native millitiles throughout, i32 arithmetic guarded against 32-bit
//! overflow (`tdiv` truncates toward zero, `>> 8` floors).
#![allow(unexpected_cfgs)]

use crate::fixed::isqrt as isqrt64;

/// Truncating division (toward zero).
#[inline]
pub fn tdiv(a: i32, b: i32) -> i32 {
    a / b
}

/// trunc(v / 256) toward zero, the way the step scaling rounds.
#[inline]
pub fn trunc_shr8(v: i32) -> i32 {
    if v < 0 {
        (v + 0xff) >> 8
    } else {
        v >> 8
    }
}

/// The overflow guard: true when |v| > 46340.
#[inline]
fn guard(v: i32) -> bool {
    !(-46340..=46340).contains(&v)
}

/// Guarded `x*x + y*y`, INT_MAX on overflow.
#[inline]
pub fn len_sq(x: i32, y: i32) -> i32 {
    if guard(x) || guard(y) {
        return i32::MAX;
    }
    let (xx, yy) = (x * x, y * y);
    if (yy as u32) > ((xx ^ i32::MAX) as u32) {
        return i32::MAX;
    }
    xx + yy
}

/// Integer square root; equal to floor(sqrt(n)) on the whole guarded range
/// (verified against math.isqrt for n <= 46340^2).
#[inline]
pub fn isqrt(n: i32) -> i32 {
    if n <= 0 {
        return 0;
    }
    isqrt64(n as i64) as i32
}

/// isqrt of the guarded squared distance.
#[inline]
pub fn distance(px: i32, py: i32, x: i32, y: i32) -> i32 {
    isqrt(len_sq(x - px, y - py))
}

/// Rescale to length L: `len = isqrt(len_sq)`; when len != 0, `v = v * L / len` per
/// axis (truncating). Returns len.
#[inline]
pub fn normalize_to(v: &mut (i32, i32), l: i32) -> i32 {
    let n = isqrt(len_sq(v.0, v.1));
    if n != 0 {
        v.0 = tdiv(v.0 * l, n);
        v.1 = tdiv(v.1 * l, n);
    }
    n
}

/// THE MASS EVERY ENTITY CARRIES, as loaded from the card data: an empty Mass
/// column (every tower and building) becomes
/// `tdiv(floor(R^2 / 250) x R, 62500)`, then every Mass is clamped to [1, 20]. King and
/// princess towers, Tombstone and Goblin Hut (R >= 1000) weigh 20, a Cannon 13, a
/// Tesla 8 -- which is why a Knight (Mass 6) touching a tower flies back at the 150
/// cap (583 of 584 live ticks against buildings reproduce with this).
#[inline]
pub fn loaded_mass(mass_column: i32, r: i32) -> i32 {
    let mut m = mass_column;
    if m == 0 {
        let t = (((r as i64) * (r as i64)) / 250) as i32;
        m = tdiv(t.wrapping_mul(r), 62500);
    }
    m.clamp(1, 20)
}

/// One entity as the contact law sees it. Positions are CURRENT (written back
/// between units, in update order); `start` is the start-of-tick position the
/// neighbour grouping below uses.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub x: i32,
    pub y: i32,
    /// Start-of-tick position (the grouping is fixed before anything moves).
    pub start_x: i32,
    pub start_y: i32,
    /// Team index (0 / 1).
    pub side: u8,
    /// CollisionRadius and Mass (the card-data columns, after `loaded_mass`).
    pub r: i32,
    pub mass: i32,
    /// Air units never touch ground units (mixed pairs are skipped).
    pub air: bool,
    /// Walks (troops); false for buildings and towers.
    pub mover: bool,
    /// Alive, and collidable (false while jumping / dashing / with NO_CHECKCOLLISIONS).
    pub alive: bool,
    pub collidable: bool,
    /// The neighbour's own avoidance offset (movers only; 0 otherwise).
    pub offset: i32,
    /// The neighbour's facing (length 256) and whether its heading counts in the
    /// avoidance dot product (states 8/0/2/10 and a busy special attack zero it).
    pub dir: (i32, i32),
    pub heading_counts: bool,
}

/// The order neighbours are visited in, which decides the steering: the vote below
/// keeps the LAST neighbour seen, and this order reproduces 242139 of 242232 live
/// offsets. Entities are grouped by a 1024-unit grid over the arena -- an entity
/// belongs to every group its circle (grown by 250 for a walker) covers, taken at its
/// START-OF-TICK position -- and the groups over the query circle are walked
/// column-outer, row-inner, each entity counted at its first sighting and only while
/// the circles strictly overlap at the CURRENT positions (`dist2 < (R_e + r)^2`).
pub struct Index {
    pub cols: i32,
    pub rows: i32,
}

impl Index {
    pub fn new(width_cells: i32, height_cells: i32) -> Index {
        Index { cols: (width_cells * 500 + 0x3ff) >> 10, rows: (height_cells * 500 + 0x3ff) >> 10 }
    }

    /// The group span an entity covers, clipped to the grid.
    fn span(&self, b: &Body) -> Option<(i32, i32, i32, i32)> {
        if b.r <= 0 {
            return None;
        }
        let rr = if b.mover { b.r + 250 } else { b.r };
        let (c0, c1) = ((b.start_x - rr) >> 10, (b.start_x + rr) >> 10);
        let (r0, r1) = ((b.start_y - rr) >> 10, (b.start_y + rr) >> 10);
        let (c0, c1) = (c0.max(0), c1.min(self.cols - 1));
        let (r0, r1) = (r0.max(0), r1.min(self.rows - 1));
        if c0 > c1 || r0 > r1 {
            return None;
        }
        Some((c0, c1, r0, r1))
    }

    /// The neighbours of a circle, in visit order: every collidable entity, both
    /// sides, no class filtered out. `bodies` is in update order (which breaks ties
    /// inside one group); returns indices.
    pub fn query(&self, bodies: &[Body], x: i32, y: i32, r: i32, out: &mut Vec<usize>) {
        out.clear();
        let (qc0, qc1) = (((x - r) >> 10).max(0), ((x + r) >> 10).min(self.cols - 1));
        let (qr0, qr1) = (((y - r) >> 10).max(0), ((y + r) >> 10).min(self.rows - 1));
        if qc0 > qc1 || qr0 > qr1 {
            return;
        }
        // (first group column, first group row, update index) of every entity whose
        // circle overlaps the query circle now
        let mut hits: Vec<(i32, i32, usize)> = Vec::new();
        for (i, b) in bodies.iter().enumerate() {
            let Some((c0, c1, r0, r1)) = self.span(b) else { continue };
            let (fc, lc) = (c0.max(qc0), c1.min(qc1));
            let (fr, lr) = (r0.max(qr0), r1.min(qr1));
            if fc > lc || fr > lr {
                continue;
            }
            let rr = b.r + r;
            if len_sq(x - b.x, y - b.y) < rr * rr {
                hits.push((fc, fr, i));
            }
        }
        hits.sort_unstable();
        out.extend(hits.into_iter().map(|h| h.2));
    }
}

/// A walking unit's contact state.
#[derive(Clone, Copy, Debug, Default)]
pub struct Contact {
    /// The accumulator and its count, filled by the
    /// separation scan and drained by `move_towards` in the same update.
    pub acc: (i32, i32),
    pub count: i32,
    /// The avoidance offset, multiples of 10 in [-190, 190] between ticks.
    pub offset: i32,
}

/// The avoidance scan. `me` is the index of the unit in
/// `bodies`; `waypoint` is the centre of the current waypoint when the path holds at
/// least two nodes (a static obstacle whose circle contains it drops that node --
/// the returned bool asks the caller to pop it).
pub fn avoidance_scan(index: &Index, bodies: &[Body], me: usize, con: &mut Contact, waypoint: Option<(i32, i32)>, charged: bool, scratch: &mut Vec<usize>) -> bool {
    let u = bodies[me];
    let (look_x, look_y) = (u.x + u.dir.0, u.y + u.dir.1);
    let radius = u.r.min(500);
    index.query(bodies, look_x, look_y, radius, scratch);
    let mut static_count = 0;
    let mut dyn_count = 0;
    let mut static_flag = true;
    let mut dyn_flag = true;
    let mut pop_waypoint = false;
    for &j in scratch.iter() {
        if j == me {
            continue;
        }
        let e = bodies[j];
        if u.air != e.air {
            continue;
        }
        if !e.collidable {
            continue;
        }
        // side = (e.x - x) * dir.y + (y - e.y) * dir.x
        let side = (e.x - u.x).wrapping_mul(u.dir.1).wrapping_add((u.y - e.y).wrapping_mul(u.dir.0));
        if !e.mover {
            // a static neighbour
            if let Some((wx, wy)) = waypoint {
                if len_sq(wx - e.x, wy - e.y) < e.r * e.r {
                    pop_waypoint = true;
                }
            }
            static_count += 1;
            static_flag = side < 0;
            continue;
        }
        // a moving neighbour
        let mut dot = u.dir.0.wrapping_mul(e.dir.0).wrapping_add(u.dir.1.wrapping_mul(e.dir.1));
        if !e.heading_counts {
            dot = 0;
        }
        if charged {
            if u.mass > e.mass {
                continue;
            }
            if dot > 0 {
                continue;
            }
        } else if dot > 0 {
            continue;
        }
        dyn_count += 1;
        dyn_flag = if e.offset != 0 { e.offset > 0 } else { side < 0 };
    }
    let flag = if static_count > 0 { static_flag } else { dyn_flag };
    if dyn_count + static_count > 0 {
        if con.offset == 0 {
            con.offset = if flag { 200 } else { -200 };
        } else if static_count > 0 {
            // Only a STATIC blocker refreshes a running offset; dynamic blockers
            // alone leave it to decay. Measured on the live 16.402 corpus:
            // 242139/242232 offsets exact.
            let c = if flag { con.offset + 20 } else { con.offset - 20 };
            con.offset = c.clamp(-200, 200);
        }
    }
    pop_waypoint
}

/// |offset| shrinks by 10 toward zero, every walk tick, right
/// after the scan.
#[inline]
pub fn decay_offset(con: &mut Contact) {
    let c = con.offset;
    con.offset = if c > 0 { c.max(10) - 10 } else { c.min(-10) + 10 };
}

/// The separation scan over the CURRENT
/// positions of every overlapping neighbour.
pub fn separation_scan(index: &Index, bodies: &[Body], me: usize, con: &mut Contact, scratch: &mut Vec<usize>) {
    let u = bodies[me];
    if u.r == 0 || !u.collidable {
        return;
    }
    index.query(bodies, u.x, u.y, u.r + 20, scratch); // R + 20
    let r_static = u.r.min(500);
    for &j in scratch.iter() {
        if j == me {
            continue;
        }
        let e = bodies[j];
        if u.air != e.air || !e.collidable || !e.alive {
            continue;
        }
        // (NO_PUSHED_BY_ALLY / _ENEMY are not modelled: no card in data/derived/cards.json
        // carries them)
        let my_r = if e.mover { u.r } else { r_static };
        let sum_r = e.r + my_r;
        let (mut dx, mut dy) = (u.x - e.x, u.y - e.y);
        if dx.abs() > sum_r || dy.abs() > sum_r {
            continue;
        }
        let mut d2 = dx * dx + dy * dy;
        if d2 == 0 {
            dy = if u.side == 0 { -1 } else { 1 };
            dx = 0;
            d2 = 1;
        }
        if d2 > sum_r * sum_r {
            continue; // touching counts as overlap
        }
        let dist = isqrt(d2).max(1);
        let overlap = (sum_r - dist).clamp(0, 300);
        let mut mag = tdiv(overlap * e.mass, u.mass.max(1));
        if mag >= 299 {
            mag = 299;
        }
        mag += 1; // -> 1..300
        con.acc.0 += tdiv(dx * mag, dist);
        con.acc.1 += tdiv(dy * mag, dist);
        con.count += 1;
    }
    // LIMIT: no tick of the corpus has a unit sharing its exact x or y with two
    // static neighbours at once, so what happens then is unmeasured here.
}

/// What `move_towards` did.
#[derive(Clone, Copy, Debug, Default)]
pub struct Moved {
    pub x: i32,
    pub y: i32,
    /// The new facing (length 256) when the heading was set.
    pub dir: Option<(i32, i32)>,
    /// The waypoint is reached (pop it).
    pub reached: bool,
}

/// The step toward `(tx, ty)` for an ordinary walking unit (no external hit
/// displacement): heading, step, the avoidance rotation, the collision mean, the
/// position write, the reached test. `seg` is the frozen segment direction;
/// `state4_ground` turns a water cell edge into a wall (deploying ground units).
#[allow(clippy::too_many_arguments)]
pub fn move_towards(
    u: (i32, i32),
    tx: i32,
    ty: i32,
    speed: i32,
    set_dir: bool,
    con: &mut Contact,
    seg: (i32, i32),
    state4_ground: bool,
    is_water: impl Fn(i32, i32) -> bool,
    width_cells: i32,
    height_cells: i32,
) -> Moved {
    let (x, y) = u;
    let dist = distance(x, y, tx, ty).max(1);
    let step = speed.min(dist).min(250);
    let (dx, dy) = (tx - x, ty - y);
    let dirx = tdiv(dx << 8, dist);
    let diry = tdiv(dy << 8, dist);
    let mut sx = trunc_shr8(dirx * step);
    let mut sy = trunc_shr8(diry * step);
    let mut new_dir = None;
    if set_dir {
        let mut d = (dx, dy);
        if normalize_to(&mut d, 256) != 0 {
            new_dir = Some(d);
        }
    }
    if con.offset != 0 {
        // the avoidance rotation
        let a = con.offset.clamp(-256, 256);
        let k = 256 - a.abs();
        let vx = ((sx * k) >> 8) + ((a * sy) >> 8);
        let vy = ((k * sy) >> 8) + ((-(sx * a)) >> 8);
        let mut t = (vx, vy);
        normalize_to(&mut t, step);
        sx = t.0;
        sy = t.1;
    }
    if con.count > 0 {
        // the collision mean, capped at 150
        let mut t = (tdiv(con.acc.0, con.count), tdiv(con.acc.1, con.count));
        if len_sq(t.0, t.1) >= 22501 {
            normalize_to(&mut t, 150);
        }
        con.count = 0;
        con.acc = (0, 0);
        sx += t.0;
        sy += t.1;
    }
    // the position write (flag = deploying ground unit only)
    let (nx, ny) = grid_move(x, y, sx, sy, state4_ground, &is_water, width_cells, height_cells);
    // the reached test
    let (rx, ry) = (tx - nx, ty - ny);
    let proj = trunc_shr8(rx.wrapping_mul(seg.0)) + trunc_shr8(ry.wrapping_mul(seg.1));
    Moved { x: nx, y: ny, dir: new_dir, reached: proj <= 1000 }
}

/// The position write: `pos += step`, then, with the flag, an axis that
/// crossed into a water cell (or out of the grid) is clamped to the edge of the cell
/// it started in. Without the flag only the grid edge clamps.
/// (The grid test and the water test are kept as two arms per axis so each
/// clamp reads on its own, even where the two arms coincide.)
#[allow(clippy::too_many_arguments, clippy::if_same_then_else)]
pub fn grid_move(ox: i32, oy: i32, dx: i32, dy: i32, flag: bool, is_water: &impl Fn(i32, i32) -> bool, width_cells: i32, height_cells: i32) -> (i32, i32) {
    let (col, row) = (tdiv(ox, 500), tdiv(oy, 500));
    let (cx0, cy0) = (col * 500, row * 500);
    let (xoff, yoff) = (ox - cx0, oy - cy0);
    let (mut px, mut py) = (ox + dx, oy + dy);
    let nx = xoff + dx;
    if dx > 0 && nx >= 500 {
        let ncol = col + 1;
        if (row | ncol) < 0 || ncol >= width_cells || row >= height_cells {
            px = cx0 + 499;
        } else if flag && is_water(ncol, row) {
            px = cx0 + 499;
        }
    } else if dx < 0 && nx < 0 {
        let ncol = col - 1;
        if (row | ncol) < 0 || width_cells < col || row >= height_cells {
            px = cx0;
        } else if flag && is_water(ncol, row) {
            px = cx0;
        }
    }
    let ny = yoff + dy;
    if dy > 0 && ny >= 500 {
        let nrow = row + 1;
        if (nrow | col) < 0 || col >= width_cells || nrow >= height_cells {
            py = cy0 + 499;
        } else if flag && is_water(col, nrow) {
            py = cy0 + 499;
        }
    } else if dy < 0 && ny < 0 {
        let nrow = row - 1;
        if (nrow | col) < 0 || col >= width_cells || height_cells < row {
            py = cy0;
        } else if flag && is_water(col, nrow) {
            py = cy0;
        }
    }
    (px, py)
}

/// The DIRECT AIM of a unit with an empty path and a target out of range: the point
/// `reach` away from the target on the line toward the unit; a zero vector aims
/// straight down the y axis.
#[inline]
pub fn direct_aim(unit: (i32, i32), target: (i32, i32), reach: i32) -> (i32, i32) {
    let mut v = (unit.0 - target.0, unit.1 - target.1);
    if isqrt(len_sq(v.0, v.1)) == 0 {
        v.1 = 1;
    }
    normalize_to(&mut v, reach);
    (target.0 + v.0, target.1 + v.1)
}

/// The segment direction from the position to the LAST node's centre,
/// normalised to 256 (unchanged when the vector is zero).
#[inline]
pub fn segment_dir(x: i32, y: i32, node_centre: (i32, i32)) -> (i32, i32) {
    let mut d = (node_centre.0 - x, node_centre.1 - y);
    normalize_to(&mut d, 256);
    d
}

// ---------------------------------------------------------------------------
// knockback: the ladder (calibration knockback.DISPLACEMENT_LAW = client16402)

/// The ladder's decrement per tick and its speed quantum. A constant of the law, like
/// the 250 step cap and the 150 impulse cap above -- not a column and not a ledger
/// value: the live Giant's ladder reads 150, 125, ..., 25, 0, -25.
pub const PUSHBACK_DECEL: i32 = 25;

/// What `start_pushback` armed: the target point (native units) and the first speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PushStart {
    pub target: (i32, i32),
    pub speed: i32,
}

/// The ladder's first speed for a push of length `l` -- `25n` with `n` the smallest
/// such that `25n(n+1)/2 >= l` (`do { v += 25; s += v } while (s < l)`). `l` in
/// (525, 700] gives 175 (the live Giant: 150, 125, ..., 0, -25); the 2018 Fireball's
/// 1800 gives 300 (n = 12: 1950 >= 1800 > 1650).
#[inline]
pub fn ladder_speed(l: i32) -> i32 {
    let (mut v, mut s) = (0, 0);
    loop {
        v += PUSHBACK_DECEL;
        s += v;
        if s >= l {
            break;
        }
    }
    v
}

/// ARMING a push a PROJECTILE carries: `L = min(strength, MAX)`, and no running
/// ladder is ever compared against (the gate refused a second push already; see
/// state.rs `arm_ladder`). `pos` is the unit's native position, `src` the source
/// point, `max_len` MAX_PUSHBACK_LENGTH. `zero_dir` is the unit direction taken
/// when the unit stands ON the source (`d == 0`): calibration
/// knockback.ZERO_VECTOR_DIRECTION selects it (the shipped value is +-x by the
/// parity of the unit's id); `None` leaves the unit unpushed.
///
/// The target is `pos + (tdiv(dx x L, d), tdiv(dy x L, d))` and the speed
/// `ladder_speed(L)`; `None` when `d == 0` with no direction or `L <= 0`.
pub fn start_pushback(pos: (i32, i32), src: (i32, i32), strength: i32, max_len: i32, zero_dir: Option<(i32, i32)>) -> Option<PushStart> {
    let (mut dx, mut dy) = (pos.0 - src.0, pos.1 - src.1); // AWAY from the source
    // the square sum is unguarded 32-bit; the arena keeps it in range
    let mut d = isqrt(dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)));
    if d == 0 {
        let (zx, zy) = zero_dir?;
        dx = zx;
        dy = zy;
        d = 1;
    }
    let l = strength.min(max_len);
    if d > 0 && l > 0 {
        Some(PushStart { target: (pos.0 + tdiv(dx.wrapping_mul(l), d), pos.1 + tdiv(dy.wrapping_mul(l), d)), speed: ladder_speed(l) })
    } else {
        None
    }
}

/// ONE LADDER TICK from its countdown on: `remaining -= 25` BEFORE the move, then
/// `move_towards(target, remaining)` with the facing untouched (`set_dir = false`)
/// and the avoidance offset in `con` rotating the step as it stands (the scan and
/// the decay are the walk tick's and do not run here). The caller runs the
/// separation scan first when the pre-decrement remaining is `> 0`, reads
/// `active = remaining >= 0` afterwards and drops the path the tick it turns
/// negative. `state4_ground` is the position write's deploying flag: the water edge
/// clamps a deploying ground unit only.
#[allow(clippy::too_many_arguments)]
pub fn pushback_step(
    u: (i32, i32),
    target: (i32, i32),
    remaining: &mut i32,
    con: &mut Contact,
    seg: (i32, i32),
    state4_ground: bool,
    is_water: impl Fn(i32, i32) -> bool,
    width_cells: i32,
    height_cells: i32,
) -> Moved {
    *remaining -= PUSHBACK_DECEL;
    move_towards(u, target.0, target.1, *remaining, false, con, seg, state4_ground, is_water, width_cells, height_cells)
}

/// The octagonal length `max(|a|, |b|) + ((min(|a|, |b|) x 53) >> 7)`, the metric
/// `nearest_land` ranks its candidates by.
#[inline]
pub fn octagonal_len(a: i32, b: i32) -> i32 {
    let (a, b) = (a.wrapping_abs(), b.wrapping_abs());
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    hi.wrapping_add(((lo.wrapping_mul(0x35) as u32) >> 7) as i32)
}

/// A position a ground unit cannot stand on: out of the grid, or its cell carries
/// any of the blocked bits, or the water bit. `bits` is the cell byte in the
/// CALLER'S encoding (arena.rs `cell_bits`, whose bit values are arena.json's
/// `bits`, not the tilemap's), so the caller names the water bit and the blocked
/// mask in that encoding: the engine passes `arena.bit_water` and an EMPTY blocked
/// mask, since arena.json marks no cell blocked for this purpose
/// (knockback.WATER_RESOLUTION not_modelled).
pub fn blocked_or_water(x: i32, y: i32, width_cells: i32, height_cells: i32, water_bit: u8, blocked_mask: u8, bits: impl Fn(i32, i32) -> u8) -> bool {
    if (x | y) < 0 || x >= width_cells * 500 || y >= height_cells * 500 {
        return true;
    }
    let b = bits(tdiv(x, 500), tdiv(y, 500));
    b & blocked_mask != 0 || b & water_bit != 0
}

/// THE WATER EJECTION a ladder tick applies to a ground unit standing on a blocked
/// or water cell at the start of a tick with remaining `> 0` (flying and hovering
/// units are exempt; calibration knockback.WATER_RESOLUTION, a hypothesis until a
/// push ends on the river). The position is clamped 250 inside the grid; if its own
/// cell is not water it is returned as clamped; else the candidates
/// `(xc - 2250 + 500k, yc + 250 + 500j)` for `j = -5..=5` and `k = 0..=10`, each
/// inside the grid and not on water, are ranked by the octagonal length of their
/// offset and the FIRST minimum in scan order (rows outer from -5, columns inner
/// from the left, strict `<`) wins -- an absolute-frame tie, like the search's, not
/// a seat frame. The two range guards test the candidate 250 further out than the
/// point they admit.
pub fn nearest_land(x: i32, y: i32, width_cells: i32, height_cells: i32, is_water: impl Fn(i32, i32) -> bool) -> (i32, i32) {
    let xc = x.max(250).min(width_cells * 500 - 250);
    let yc = y.max(250).min(height_cells * 500 - 250);
    if !is_water(tdiv(xc, 500), tdiv(yc, 500)) {
        return (xc, yc);
    }
    let mut best = i32::MAX;
    let (mut bx, mut by) = (xc, yc);
    for j in -5..=5 {
        if j * 500 + yc < -250 {
            continue;
        }
        let cy = j * 500 + yc + 250;
        let row = tdiv(cy, 500);
        let dy = cy - yc;
        for k in 0..=10 {
            let off = k * 500;
            if off + xc - 2500 < -250 {
                continue;
            }
            let cx = off + xc - 2250;
            if cx >= width_cells * 500 || cy >= height_cells * 500 {
                continue;
            }
            if is_water(tdiv(cx, 500), row) {
                continue;
            }
            let dist = octagonal_len(off - 2250, dy);
            if dist < best {
                best = dist;
                by = cy;
                bx = cx;
            }
        }
    }
    (bx, by)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(x: i32, y: i32, r: i32, mass: i32, mover: bool, side: u8) -> Body {
        Body { x, y, start_x: x, start_y: y, side, r, mass, air: false, mover, alive: true, collidable: true, offset: 0, dir: (0, 256), heading_counts: true }
    }

    #[test]
    fn the_spawn_push_of_battle_f_sc2_is_reproduced() {
        // Battle F sc2: three Skeletons (R 500, Mass 1) at 807 from a Knight (R 500,
        // Mass 6) on their tile; each Skeleton is shoved exactly 150 straight away,
        // the Knight does not move (its three pushes cancel).
        let index = Index::new(36, 64);
        let knight = body(9500, 8500, 500, 6, true, 0);
        let s1 = body(9500, 9307, 500, 1, true, 0);
        let s2 = body(10199, 8097, 500, 1, true, 0);
        let s3 = body(8801, 8097, 500, 1, true, 0);
        let bodies = vec![s1, s2, s3, knight];
        let mut scratch = Vec::new();
        for (i, expect) in [(0, (0, 150)), (1, (130, -75)), (2, (-130, -75))] {
            let mut con = Contact::default();
            separation_scan(&index, &bodies, i, &mut con, &mut scratch);
            assert_eq!(con.count, 1, "skeleton {i} sees only the Knight");
            let b = bodies[i];
            let m = move_towards((b.x, b.y), b.x, b.y, 0, false, &mut con, (0, 0), false, |_, _| false, 36, 64);
            assert_eq!((m.x - b.x, m.y - b.y), expect, "skeleton {i}");
        }
        let mut con = Contact::default();
        separation_scan(&index, &bodies, 3, &mut con, &mut scratch);
        assert_eq!(con.count, 3);
        let m = move_towards((9500, 8500), 9500, 8500, 0, false, &mut con, (0, 0), false, |_, _| false, 36, 64);
        assert!((m.x - 9500).abs() <= 1 && (m.y - 8500).abs() <= 1, "the Knight's three pushes cancel: {:?}", (m.x, m.y));
    }

    #[test]
    fn an_isolated_walk_step_is_the_measured_law() {
        // Knight S = 60 heading straight up the lane: dir (0,256), step (0,60)
        let mut con = Contact::default();
        let m = move_towards((3481, 8556), 3250, 9750, 60, true, &mut con, (0, 0), false, |_, _| false, 36, 64);
        assert_eq!((m.x - 3481, m.y - 8556), (tdiv(tdiv((3250 - 3481) << 8, distance(3481, 8556, 3250, 9750)) * 60, 256), tdiv(tdiv((9750 - 8556) << 8, distance(3481, 8556, 3250, 9750)) * 60, 256)));
        assert!(m.dir.is_some());
    }

    #[test]
    fn the_rotation_turns_the_step_by_atan2_of_the_offset() {
        // offset 190 on a step of (0, 60): ~70.8 degrees, length kept at 60 (to
        // within the integer renormalisation)
        let mut con = Contact { offset: 190, ..Default::default() };
        let m = move_towards((5000, 5000), 5000, 15000, 60, false, &mut con, (0, 256), false, |_, _| false, 36, 64);
        let (sx, sy) = (m.x - 5000, m.y - 5000);
        // 70.84 degrees off the +y axis is a tangent of 2.857, i.e. sx/sy = 2.857.
        // The +-2 degree window is a ratio in [2.58, 3.24], asserted in hundredths.
        assert!(sx > 0 && sy > 0, "the step turned out of the quadrant: {sx},{sy}");
        assert!((258..=324).contains(&(sx * 100 / sy)), "ratio {sx}/{sy}");
        assert!((58..=60).contains(&isqrt(sx * sx + sy * sy)), "length {}", isqrt(sx * sx + sy * sy));
    }

    #[test]
    fn loaded_mass_gives_buildings_their_mass() {
        assert_eq!(loaded_mass(0, 1400), 20, "king tower");
        assert_eq!(loaded_mass(0, 1000), 20, "princess tower / Tombstone / hut");
        assert_eq!(loaded_mass(0, 600), 13, "Cannon");
        assert_eq!(loaded_mass(0, 500), 8, "Tesla");
        assert_eq!(loaded_mass(28, 750), 20, "MegaMonk is clamped");
        assert_eq!(loaded_mass(6, 500), 6, "a Knight keeps its column");
        assert_eq!(loaded_mass(1, 500), 1, "a Skeleton keeps its column");
    }

    #[test]
    fn decay_walks_the_offset_to_zero_by_tens() {
        let mut c = Contact { offset: 200, ..Default::default() };
        decay_offset(&mut c);
        assert_eq!(c.offset, 190);
        c.offset = -10;
        decay_offset(&mut c);
        assert_eq!(c.offset, 0);
        decay_offset(&mut c);
        assert_eq!(c.offset, 0);
    }

    #[test]
    fn the_ladder_speed_is_the_smallest_triangular_cover() {
        // 25n(n+1)/2: 25, 75, 150, 250, 375, 525, 700, 900, ... the live Giant's L
        // sits in (525, 700] -> 175; the 2018 Fireball's 1800 in (1650, 1950] -> 300
        assert_eq!(ladder_speed(1), 25);
        assert_eq!(ladder_speed(25), 25);
        assert_eq!(ladder_speed(26), 50);
        assert_eq!(ladder_speed(525), 150);
        assert_eq!(ladder_speed(526), 175);
        assert_eq!(ladder_speed(700), 175);
        assert_eq!(ladder_speed(701), 200);
        assert_eq!(ladder_speed(1800), 300);
        assert_eq!(ladder_speed(40000), 1425, "n = 57: 25 x 57 x 58 / 2 = 41325 >= 40000 > 39900");
    }

    #[test]
    fn a_push_aims_l_away_along_the_source_line_and_caps_at_max() {
        let p = start_pushback((5000, 5000), (5000, 4000), 600, 40000, None).unwrap();
        assert_eq!(p, PushStart { target: (5000, 5600), speed: 175 });
        let p = start_pushback((5000, 5000), (4000, 5000), 100_000, 40000, None).unwrap();
        assert_eq!(p, PushStart { target: (45000, 5000), speed: ladder_speed(40000) });
        // d == 0: the unit direction given, distance 1, L along it exactly
        assert_eq!(start_pushback((5000, 5000), (5000, 5000), 1800, 40000, Some((-1, 0))).unwrap().target, (3200, 5000));
        assert_eq!(start_pushback((5000, 5000), (5000, 5000), 1800, 40000, None), None);
        assert_eq!(start_pushback((5000, 5000), (4000, 5000), 0, 40000, None), None);
    }

    #[test]
    fn the_ladder_reads_150_down_to_zero_then_one_step_back() {
        // the live Giant: L in (525, 700], v0 175, steps 150 125 100 75 50 25 0 -25
        let start = start_pushback((5000, 5000), (5000, 4400), 600, 40000, None).unwrap();
        let mut pos = (5000, 5000);
        let mut rem = start.speed;
        let mut steps = Vec::new();
        let mut active = true;
        while active {
            let mut con = Contact::default();
            let m = pushback_step(pos, start.target, &mut rem, &mut con, (0, 0), false, |_, _| false, 36, 64);
            steps.push(m.y - pos.1);
            assert!(m.dir.is_none(), "the facing is never touched");
            pos = (m.x, m.y);
            active = rem >= 0;
        }
        assert_eq!(steps, vec![150, 125, 100, 75, 50, 25, 0, -25]);
        assert_eq!(pos, (5000, 5500));
    }

    #[test]
    fn nearest_land_returns_the_clamped_point_off_water_and_the_first_octagonal_minimum_on_it() {
        // a river band on rows 30..=33 of a 36 x 64 grid
        let water = |_c: i32, r: i32| (30..=33).contains(&r);
        assert_eq!(nearest_land(5000, 5000, 36, 64, water), (5000, 5000));
        assert_eq!(nearest_land(100, 100, 36, 64, water), (250, 250), "clamped 250 inside the grid");
        // on the river's upper half: the row above (j = -1 -> yc - 250 lands on row 29 when
        // yc is in the first 250 of row 30)
        let (x, y) = nearest_land(9000, 15100, 36, 64, water);
        assert!(!water(tdiv(x, 500), tdiv(y, 500)));
        assert_eq!((x, y), (8750, 14850), "j = -1, k = 4: offset (-250, -250), the first octagonal minimum");
        // an encoding of the caller's own choosing: the function must read the two
        // masks it is handed and nothing else, so these use values the engine never
        // passes (water 4, blocked 8 | 1)
        assert!(blocked_or_water(9000, 15100, 36, 64, 4, 9, |_, _| 4));
        assert!(blocked_or_water(-1, 100, 36, 64, 4, 9, |_, _| 0));
        assert!(blocked_or_water(100, 100, 36, 64, 4, 9, |_, _| 8), "a bit of the blocked mask counts");
        assert!(!blocked_or_water(100, 100, 36, 64, 4, 9, |_, _| 2));
        // in the engine's encoding (arena.json WATER 32, NO_DEPLOY 16, no blocked mask):
        // a NO_DEPLOY cell is not "blocked" for the teleport
        assert!(blocked_or_water(100, 100, 36, 64, 32, 0, |_, _| 32));
        assert!(!blocked_or_water(100, 100, 36, 64, 32, 0, |_, _| 16));
        assert_eq!(octagonal_len(3, 4), 4 + ((3 * 53) >> 7));
        assert_eq!(octagonal_len(-1000, 0), 1000);
    }
}
