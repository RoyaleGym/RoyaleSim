//! The battlefield: a 36 x 64 half-tile bitmask grid, loaded from
//! data/derived/arena.json (which tools/extract_arena.py derives from
//! Supercell's shipped tilemap and gates).
//!
//! WHY LOADED, NOT WRITTEN DOWN
//!     A hand-typed arena gets the river, the bridges and the king footprint
//!     subtly wrong, and the errors are invisible until a path disagrees with the
//!     real game. Nothing geometric in here is typed in by hand except the one
//!     fact the tilemap does not carry (princess tower y, see
//!     `PRINCESS_TOWER_Y_TILES_100`).
//!
//! THE MIRROR IS A 180-DEGREE ROTATION, NOT A REFLECTION
//!     Blue defends low y, Red high y. The mirror of Blue at (x, y) is Red at
//!     (W - x, H - y): each seat's view is the other's turned half a circle, so
//!     Red's own-LEFT is the engine's right. Treating it as a reflection in y
//!     alone (x untouched) is wrong and not cheaply wrong: under that reading a
//!     policy shared by both seats desynced on 80 of 144 multi-unit deploys.
//!     The shipped grid is exactly rotation-symmetric (row r col c == row 63 - r
//!     col 35 - c, with the two lane bits swapped), which `is_rotation_symmetric`
//!     asserts, because every symmetry guarantee in the engine leans on it. It is
//!     ALSO y-symmetric, but nothing may rely on that: a tie-break that is only
//!     reflection-invariant (lower engine x, engine lane) is a seat bias under the
//!     rotation.
//!
//! CELL BOUNDARIES ARE CLOSED ON BOTH SIDES
//!     A point exactly on the line between two cells is treated as touching
//!     BOTH. The obvious half-open convention (`cell = y / 9000`) is not
//!     mirror-symmetric: y = 270000 lands in water row 30 while its mirror
//!     y = 306000 lands in dry row 34, so a Blue unit and its mirrored Red twin
//!     would get different answers to "am I on water?". Closed cells make every
//!     grid query commute with the mirror (the argument is per axis, so it holds
//!     for the rotation exactly as it did for the reflection).
#![allow(unexpected_cfgs)]

use crate::fixed::{isqrt, Vec2, SUBTILE};
use crate::Team;
use serde::Deserialize;
use std::sync::OnceLock;

const SHIPPED_ARENA_JSON: &str = include_str!("../../../data/derived/arena.json");

/// Princess tower centre y, in hundredths of a tile, on Blue's side.
/// NOT derivable from the tilemap: princess towers are not in the static map
/// (see calibration.json collision.BUILDING_FOOTPRINT_MODEL). Their x IS
/// derived -- each sits on its bridge's centre line -- only y is typed in.
/// Source: community tile analysis, (3.5, 6.5) / (14.5, 6.5).
pub const PRINCESS_TOWER_Y_TILES_100: i32 = 650;

// TROOP TERRITORY -- THE SHIPPED MECHANIC, NOT A DEPTH
//
// Territory after a princess tower falls is often modelled as a fixed pocket depth
// past the far river bank (6 tiles, or the destroyed tower's centre row at 8.5).
// No such constant exists in any shipped data -- globals.csv, locations.csv,
// arenas.csv, game_modes.csv and the tilemap all lack it -- and the two candidate
// depths disagree with each other. The mechanic is instead carried by a column in
// buildings.csv: NoDeploySizeW/H, shipped on exactly
// KingTower (18, 16), PrincessTower (11, 21) and a NOTINUSE copy of the king. Read
// as TILES of an axis-aligned rectangle centred on the tower, all four landmarks
// are exact (the half-tile reading hits 0 of 4): the king rect spans x = [0, 18],
// the arena width; the left and right princess rects meet exactly at x = 9; each
// princess rect's far edge is the far river bank (Blue princess y 6.5 + 10.5 = 17).
// tools/check_data.py gates those landmarks. So the rule is: a troop may not be
// placed inside the closed rect of any ALIVE ENEMY crown tower. With every enemy
// tower up that forbids the whole enemy side; when one princess falls, what opens
// on that side is the band between the far bank and the enemy king rect -- 8
// half-rows, not 12. calibration.json arena.TERRITORY_MODEL records it, with the
// promotion rule (the old model predicts troops 2 tiles deeper).

/// Which territory rule a card's placement uses (protocol.py `Placement`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Territory {
    /// Buildings: own half only, even after a princess falls. UNSOURCED -- the
    /// NoDeploySize rects say where TROOPS may not go; nothing shipped says a
    /// building may use the opened ground, and the previous rule (own half) is
    /// kept rather than widened on no evidence.
    OwnHalf,
    /// Troops: outside every closed `enemy_rects` rectangle (the alive enemy crown
    /// towers' NoDeploySize rects), and never in the river band.
    EnemyTowerRects,
    /// Spells: anywhere strictly inside the arena.
    Anywhere,
    /// Spells that release units (Goblin Barrel) under calibration
    /// spells.SPAWNING_SPELL_WATER_RULE = refuse_touching_water: anywhere strictly
    /// inside the arena except where the TROOP water test refuses. No territory, no
    /// no-deploy cells (the king block is a legal barrel target), no footprints.
    AnywhereButWater,
}

/// calibration.json arena.TERRITORY_MODEL. One candidate is implemented; the enum
/// exists so the registry value is READ and an unknown one is refused, never
/// silently run as this one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum TerritoryModel {
    EnemyTowerNoDeployRects,
}

impl TerritoryModel {
    pub fn from_calibration_name(s: &str) -> Option<Self> {
        match s {
            "enemy_tower_no_deploy_rects" => Some(Self::EnemyTowerNoDeployRects),
            _ => None,
        }
    }
}

/// Why a position is not a legal deploy for a territory rule, in the order
/// the checks run (protocol.py `DeployStatus` 5..8 share this order).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZoneError {
    OutOfArena,
    Water,
    NoDeploy,
    OutOfTerritory,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Lane {
    Left = 0,
    Right = 1,
}

/// Which footprint a building presents to movement and collision.
/// calibration.json collision.BUILDING_FOOTPRINT_MODEL: status guess, three
/// candidates, all implemented so a measurement flips an enum instead of a file.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum FootprintModel {
    /// A circle of the building's CollisionRadius.
    CollisionRadiusCircle,
    /// An axis-aligned square whose half-extent is the CollisionRadius. The
    /// shipped TileSizeOverride column is not in cards.json, so the radius is
    /// the only size available; this reading gives princess 2x2, king 2.8x2.8.
    TileSizeOverrideBox,
    /// The king tower uses its no-deploy block from the shipped tilemap (3x3);
    /// buildings absent from the map use a box snapped outward to half-tiles.
    StaticBitmap,
}

impl FootprintModel {
    pub fn from_calibration_name(s: &str) -> Option<Self> {
        match s {
            "collision_radius_circle" => Some(Self::CollisionRadiusCircle),
            "tile_size_override_box" => Some(Self::TileSizeOverrideBox),
            "static_bitmap" => Some(Self::StaticBitmap),
            _ => None,
        }
    }
}

/// A closed axis-aligned rectangle in subtiles.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    /// Is `p` inside or on the boundary?
    #[inline]
    pub fn contains_closed(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
}

/// A static footprint that blocks ground movement.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    Circle { c: Vec2, r: i32 },
    Box(Rect),
}

/// `a * b / c` rounded AWAY from zero. Used for push-outs so a unit pushed to
/// "exactly touching" is never left one subtile inside. Away-from-zero (like
/// truncation) is odd-symmetric: f(-a) = -f(a), so it commutes with the mirror.
#[inline]
fn mul_div_away(a: i64, b: i64, c: i64) -> i32 {
    debug_assert!(c > 0);
    let n = a * b;
    let q = if n >= 0 { (n + c - 1) / c } else { (n - (c - 1)) / c };
    q as i32
}

#[inline]
fn clamp_i32(v: i32, lo: i32, hi: i32) -> i32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

impl Shape {
    pub fn center(&self) -> Vec2 {
        match *self {
            Shape::Circle { c, .. } => c,
            Shape::Box(r) => Vec2::new((r.min.x + r.max.x) / 2, (r.min.y + r.max.y) / 2),
        }
    }

    /// Rotate 180 degrees about the centre of a `w` x `h` arena (the seat mirror).
    /// Exact: every coordinate maps by `v -> size - v`, no division. Not a
    /// reflection in y alone -- see the mirror note at the top of this file.
    pub fn rotated(&self, w: i32, h: i32) -> Shape {
        match *self {
            Shape::Circle { c, r } => Shape::Circle { c: Vec2::new(w - c.x, h - c.y), r },
            Shape::Box(b) => Shape::Box(Rect {
                min: Vec2::new(w - b.max.x, h - b.max.y),
                max: Vec2::new(w - b.min.x, h - b.min.y),
            }),
        }
    }

    /// Does a disc of radius `r` at `p` penetrate this shape (touching is fine)?
    pub fn penetrates(&self, p: Vec2, r: i32) -> bool {
        match *self {
            Shape::Circle { c, r: big } => {
                let need = (big as i64) + (r as i64);
                p.dist2(c) < need * need
            }
            Shape::Box(b) => {
                let q = Vec2::new(clamp_i32(p.x, b.min.x, b.max.x), clamp_i32(p.y, b.min.y, b.max.y));
                p.dist2(q) < (r as i64) * (r as i64) || q == p
            }
        }
    }

    /// If a disc (p, r) penetrates, return where it must move to just touch.
    ///
    /// `team` breaks the exactly-degenerate cases in that team's OWN frame: the
    /// disc centre on the shape's centre, or a dead-centre y tie in a box, pushes
    /// toward the unit's own side; a dead-centre x tie pushes toward its own-LEFT.
    /// Both commute with the 180-degree seat rotation. "Push toward -y" would push
    /// Blue home and Red into the enemy; "a dead-centre x tie goes to lower ENGINE
    /// x" is only reflection-invariant, and under the rotation sends Blue to its
    /// own-left and Red to its own-right.
    pub fn push_out(&self, p: Vec2, r: i32, team: Team) -> Option<Vec2> {
        let own_side_dy = Arena::own_side_dy(team);
        match *self {
            Shape::Circle { c, r: big } => {
                let need = (big as i64) + (r as i64);
                let d = p.sub(c);
                let d2 = d.len2();
                if d2 >= need * need {
                    return None;
                }
                if d2 == 0 {
                    return Some(Vec2::new(c.x, c.y + own_side_dy * need as i32));
                }
                let len = isqrt(d2).max(1);
                Some(Vec2::new(
                    c.x + mul_div_away(d.x as i64, need, len),
                    c.y + mul_div_away(d.y as i64, need, len),
                ))
            }
            Shape::Box(b) => {
                let q = Vec2::new(clamp_i32(p.x, b.min.x, b.max.x), clamp_i32(p.y, b.min.y, b.max.y));
                if q != p {
                    let d = p.sub(q);
                    let d2 = d.len2();
                    let rr = r as i64;
                    if d2 >= rr * rr {
                        return None;
                    }
                    let len = isqrt(d2).max(1);
                    return Some(Vec2::new(
                        q.x + mul_div_away(d.x as i64, rr, len),
                        q.y + mul_div_away(d.y as i64, rr, len),
                    ));
                }
                // Centre inside the box: leave by the shallowest face. Ties
                // prefer the y axis (the rotation maps each axis to itself); a
                // dead-centre y tie goes to the own side; a dead-centre x tie
                // goes to the own-left.
                let left = p.x - b.min.x;
                let right = b.max.x - p.x;
                let down = p.y - b.min.y;
                let up = b.max.y - p.y;
                let bx = left.min(right);
                let by = down.min(up);
                if by <= bx {
                    let y = if down < up {
                        b.min.y - r
                    } else if up < down {
                        b.max.y + r
                    } else if own_side_dy < 0 {
                        b.min.y - r
                    } else {
                        b.max.y + r
                    };
                    Some(Vec2::new(p.x, y))
                } else {
                    #[cfg(not(clash_plant = "reflection_box_tie"))]
                    let to_max = right < left || (right == left && Arena::own_left_dx(team) > 0);
                    #[cfg(clash_plant = "reflection_box_tie")]
                    let to_max = right < left; // PLANT: the reflection-only lower-engine-x tie.
                    let x = if to_max { b.max.x + r } else { b.min.x - r };
                    Some(Vec2::new(x, p.y))
                }
            }
        }
    }

    /// Does a disc of radius `r` at `p` overlap OR TOUCH this shape (closed)?
    ///
    /// The deploy-footprint test. Closed on purpose: protocol.py's mask refuses a
    /// placement at exactly `dist == R + r` (`<= r*r`), and a query that disagreed
    /// with the mask only at the touching distance would still be a mask that
    /// lies. A `penetrates(pos, 1)` test is NOT equivalent: it refuses
    /// `dist^2 <= R^2 + 2R`, a sliver the mask allows.
    pub fn covers_disc(&self, p: Vec2, r: i32) -> bool {
        match *self {
            Shape::Circle { c, r: big } => {
                let need = (big as i64) + (r as i64);
                p.dist2(c) <= need * need
            }
            Shape::Box(b) => {
                let q = Vec2::new(clamp_i32(p.x, b.min.x, b.max.x), clamp_i32(p.y, b.min.y, b.max.y));
                p.dist2(q) <= (r as i64) * (r as i64)
            }
        }
    }

    /// Does the segment a->b pass within `inflate` of this shape? Exact integer
    /// arithmetic (i128), so the answer is the same on every platform.
    pub fn segment_hits(&self, a: Vec2, b: Vec2, inflate: i32) -> bool {
        match *self {
            Shape::Circle { c, r } => {
                let rr = (r as i128) + (inflate as i128);
                seg_point_dist2_le(a, b, c, rr * rr)
            }
            Shape::Box(bx) => {
                let min = Vec2::new(bx.min.x - inflate, bx.min.y - inflate);
                let max = Vec2::new(bx.max.x + inflate, bx.max.y + inflate);
                segment_hits_aabb(a, b, min, max)
            }
        }
    }

    /// Radius of a circle centred on `center()` that contains the shape.
    pub fn bound_radius(&self) -> i32 {
        match *self {
            Shape::Circle { r, .. } => r,
            Shape::Box(b) => {
                let hx = (b.max.x - b.min.x) / 2 + 1;
                let hy = (b.max.y - b.min.y) / 2 + 1;
                isqrt((hx as i64) * (hx as i64) + (hy as i64) * (hy as i64)) as i32 + 1
            }
        }
    }
}

/// Is the closest distance from point p to segment ab squared <= limit2?
fn seg_point_dist2_le(a: Vec2, b: Vec2, p: Vec2, limit2: i128) -> bool {
    let abx = (b.x - a.x) as i128;
    let aby = (b.y - a.y) as i128;
    let apx = (p.x - a.x) as i128;
    let apy = (p.y - a.y) as i128;
    let ab2 = abx * abx + aby * aby;
    let dot = apx * abx + apy * aby;
    if ab2 == 0 || dot <= 0 {
        return apx * apx + apy * apy <= limit2;
    }
    if dot >= ab2 {
        let bpx = (p.x - b.x) as i128;
        let bpy = (p.y - b.y) as i128;
        return bpx * bpx + bpy * bpy <= limit2;
    }
    // dist2 = |ap|^2 - dot^2/|ab|^2, compared without dividing.
    (apx * apx + apy * apy) * ab2 - dot * dot <= limit2 * ab2
}

/// Slab test with rational parameters kept as (num, den>0) pairs.
fn segment_hits_aabb(a: Vec2, b: Vec2, min: Vec2, max: Vec2) -> bool {
    // t in [lo, hi], each a fraction n/d with d > 0.
    let mut lo = (0i128, 1i128);
    let mut hi = (1i128, 1i128);
    let axes = [(a.x, b.x, min.x, max.x), (a.y, b.y, min.y, max.y)];
    for (a0, b0, mn, mx) in axes {
        let d = (b0 - a0) as i128;
        if d == 0 {
            if a0 < mn || a0 > mx {
                return false;
            }
            continue;
        }
        let (mut t1, mut t2) = (((mn - a0) as i128, d), ((mx - a0) as i128, d));
        if d < 0 {
            t1 = (-t1.0, -t1.1);
            t2 = (-t2.0, -t2.1);
            std::mem::swap(&mut t1, &mut t2);
        }
        // lo = max(lo, t1); hi = min(hi, t2)
        if t1.0 * lo.1 > lo.0 * t1.1 {
            lo = t1;
        }
        if t2.0 * hi.1 < hi.0 * t2.1 {
            hi = t2;
        }
        if lo.0 * hi.1 > hi.0 * lo.1 {
            return false;
        }
    }
    true
}

#[derive(Deserialize)]
struct RawBridge {
    half_cols: [i32; 2],
}

#[derive(Deserialize)]
struct RawKing {
    half_rows: [i32; 2],
    half_cols: [i32; 2],
}

#[derive(Deserialize)]
struct RawBits {
    #[serde(rename = "LANE_LEFT")]
    lane_left: u8,
    #[serde(rename = "LANE_RIGHT")]
    lane_right: u8,
    #[serde(rename = "NO_DEPLOY")]
    no_deploy: u8,
    #[serde(rename = "WATER")]
    water: u8,
}

#[derive(Deserialize)]
struct RawArena {
    half_tiles_per_tile: i32,
    tiles: [i32; 2],
    half_grid: [i32; 2],
    water_half_rows: [i32; 2],
    bridges: Vec<RawBridge>,
    king_blocks: Vec<RawKing>,
    bits: RawBits,
    grid: Vec<Vec<u8>>,
}

/// A bridge: the x-span (subtiles, closed) of dry cells across the river.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bridge {
    pub x_min: i32,
    pub x_max: i32,
    pub center_x: i32,
}

#[derive(Clone, Debug)]
pub struct Arena {
    pub cols: i32,
    pub rows: i32,
    /// Subtiles per half-tile cell.
    pub cell: i32,
    pub width: i32,
    pub height: i32,
    cells: Vec<u8>,
    pub bit_lane_left: u8,
    pub bit_lane_right: u8,
    pub bit_no_deploy: u8,
    pub bit_water: u8,
    /// Bridges sorted by x (left, right).
    pub bridges: Vec<Bridge>,
    /// River band, subtiles: water rows cover y in [water_y_min, water_y_max].
    pub water_y_min: i32,
    pub water_y_max: i32,
    /// King no-deploy blocks, sorted by y (Blue's first).
    pub king_blocks: Vec<Rect>,
}

impl Arena {
    /// Parse an arena.json document.
    pub fn from_json(s: &str) -> Result<Arena, String> {
        let raw: RawArena = serde_json::from_str(s).map_err(|e| format!("arena.json: {e}"))?;
        let [cols, rows] = raw.half_grid;
        if raw.half_tiles_per_tile <= 0 || SUBTILE % raw.half_tiles_per_tile != 0 {
            return Err(format!("half_tiles_per_tile {} does not divide SUBTILE", raw.half_tiles_per_tile));
        }
        let cell = SUBTILE / raw.half_tiles_per_tile;
        if raw.tiles[0] * raw.half_tiles_per_tile != cols || raw.tiles[1] * raw.half_tiles_per_tile != rows {
            return Err("tiles and half_grid disagree".into());
        }
        if raw.grid.len() != rows as usize || raw.grid.iter().any(|r| r.len() != cols as usize) {
            return Err(format!("grid is not {cols}x{rows}"));
        }
        let cells: Vec<u8> = raw.grid.iter().flat_map(|r| r.iter().copied()).collect();
        let mut bridges: Vec<Bridge> = raw
            .bridges
            .iter()
            .map(|b| {
                let x_min = b.half_cols[0] * cell;
                let x_max = (b.half_cols[1] + 1) * cell;
                Bridge { x_min, x_max, center_x: (x_min + x_max) / 2 }
            })
            .collect();
        bridges.sort_by_key(|b| b.x_min);
        let mut king_blocks: Vec<Rect> = raw
            .king_blocks
            .iter()
            .map(|k| Rect {
                min: Vec2::new(k.half_cols[0] * cell, k.half_rows[0] * cell),
                max: Vec2::new((k.half_cols[1] + 1) * cell, (k.half_rows[1] + 1) * cell),
            })
            .collect();
        king_blocks.sort_by_key(|r| r.min.y);
        if bridges.len() != 2 {
            return Err(format!("expected 2 bridges, got {}", bridges.len()));
        }
        if king_blocks.len() != 2 {
            return Err(format!("expected 2 king blocks, got {}", king_blocks.len()));
        }
        Ok(Arena {
            cols,
            rows,
            cell,
            width: cols * cell,
            height: rows * cell,
            cells,
            bit_lane_left: raw.bits.lane_left,
            bit_lane_right: raw.bits.lane_right,
            bit_no_deploy: raw.bits.no_deploy,
            bit_water: raw.bits.water,
            bridges,
            water_y_min: raw.water_half_rows[0] * cell,
            water_y_max: (raw.water_half_rows[1] + 1) * cell,
            king_blocks,
        })
    }

    /// The arena shipped in data/derived/arena.json, parsed once per process.
    pub fn shipped() -> Arena {
        static CELL: OnceLock<Arena> = OnceLock::new();
        CELL.get_or_init(|| Arena::from_json(SHIPPED_ARENA_JSON).expect("shipped arena.json must parse"))
            .clone()
    }

    #[inline]
    pub fn cell_bits(&self, col: i32, row: i32) -> u8 {
        if col < 0 || row < 0 || col >= self.cols || row >= self.rows {
            return 0;
        }
        self.cells[(row * self.cols + col) as usize]
    }

    /// Half-tile cell (col, row) containing `p`, half-open convention. For
    /// indexing only -- geometric queries use `touching_cells`.
    #[inline]
    pub fn subtile_to_half(&self, p: Vec2) -> (i32, i32) {
        (p.x.div_euclid(self.cell), p.y.div_euclid(self.cell))
    }

    /// Centre of a half-tile cell, in subtiles.
    #[inline]
    pub fn half_to_subtile_center(&self, col: i32, row: i32) -> Vec2 {
        Vec2::new(col * self.cell + self.cell / 2, row * self.cell + self.cell / 2)
    }

    /// The nearest point to `p` on passable ground along one AXIS (+-x or +-y), or
    /// None if no axis reaches any (never on the shipped map). `p` itself if it is
    /// already passable.
    ///
    /// WHY IT EXISTS: a knockback is not blocked by the river (calibration
    /// knockback.WATER_RESOLUTION = eject_to_nearest_land), so a pushed ground unit can
    /// end with its centre on water, which walking never allows. The candidates are the
    /// first subtile past each half-cell boundary (closed cells: a point ON a boundary
    /// touches both cells, so the boundary itself is never the answer).
    ///
    /// TIES are broken in `team`'s OWN frame -- own side, then own-left, then own-
    /// right, then the enemy side -- so a unit and its rotated twin eject to rotated
    /// points. The shipped arena is rotation-symmetric (`is_rotation_symmetric`), and
    /// the +1/-1 offsets map onto each other under v -> size - v, so the distances are
    /// identical for the twins; only the tie order needed a frame.
    pub fn nearest_passable_ground(&self, p: Vec2, team: Team) -> Option<Vec2> {
        if self.is_passable_ground(p) {
            return Some(p);
        }
        #[cfg(not(clash_plant = "eject_tie_engine_frame"))]
        let (side, left) = (Arena::own_side_dy(team), Arena::own_left_dx(team));
        #[cfg(clash_plant = "eject_tie_engine_frame")]
        let (side, left) = {
            let _ = team; // PLANT: ties broken in Blue's frame for both seats.
            (Arena::own_side_dy(Team::Blue), Arena::own_left_dx(Team::Blue))
        };
        // (distance, preference, point); preference is own side 0, own-left 1,
        // own-right 2, enemy side 3.
        let mut best: Option<(i32, u8, Vec2)> = None;
        let mut offer = |d: i32, pref: u8, q: Vec2| {
            if self.is_passable_ground(q) && best.map_or(true, |(bd, bp, _)| (d, pref) < (bd, bp)) {
                best = Some((d, pref, q));
            }
        };
        let pref_y = |dy: i32| if dy == side { 0 } else { 3 };
        let pref_x = |dx: i32| if dx == left { 1 } else { 2 };
        let c = self.cell;
        let (r0, c0) = (p.y.div_euclid(c), p.x.div_euclid(c));
        for k in 0..=self.rows {
            let up = (r0 + k) * c + 1;
            if up > p.y && up <= self.height {
                offer(up - p.y, pref_y(1), Vec2::new(p.x, up));
            }
            let down = (r0 - k) * c - 1;
            if down < p.y && down >= 0 {
                offer(p.y - down, pref_y(-1), Vec2::new(p.x, down));
            }
        }
        for k in 0..=self.cols {
            let right = (c0 + k) * c + 1;
            if right > p.x && right <= self.width {
                offer(right - p.x, pref_x(1), Vec2::new(right, p.y));
            }
            let lft = (c0 - k) * c - 1;
            if lft < p.x && lft >= 0 {
                offer(p.x - lft, pref_x(-1), Vec2::new(lft, p.y));
            }
        }
        best.map(|(_, _, q)| q)
    }

    /// Inclusive index range of cells on one axis that touch coordinate v.
    #[inline]
    fn axis_span(&self, v: i32, n: i32) -> (i32, i32) {
        let i = v.div_euclid(self.cell);
        let lo = if v.rem_euclid(self.cell) == 0 { i - 1 } else { i };
        (lo.max(0), i.min(n - 1))
    }

    /// Bitwise OR of every cell the point touches (closed cells).
    #[inline]
    pub fn touching_bits(&self, p: Vec2) -> u8 {
        if !self.in_bounds(p) {
            return 0;
        }
        let (x0, x1) = self.axis_span(p.x, self.cols);
        let (y0, y1) = self.axis_span(p.y, self.rows);
        let mut acc = 0u8;
        for row in y0..=y1 {
            for col in x0..=x1 {
                acc |= self.cell_bits(col, row);
            }
        }
        acc
    }

    #[inline]
    pub fn in_bounds(&self, p: Vec2) -> bool {
        p.x >= 0 && p.y >= 0 && p.x <= self.width && p.y <= self.height
    }

    #[inline]
    pub fn is_water(&self, p: Vec2) -> bool {
        self.touching_bits(p) & self.bit_water != 0
    }

    /// Can a ground unit's centre be here? In bounds and touching no water.
    /// Water is hard-blocked (calibration pathfinding.PATHFINDING_COSTS: no cost
    /// table is shipped because none can be sourced).
    #[inline]
    pub fn is_passable_ground(&self, p: Vec2) -> bool {
        self.in_bounds(p) && !self.is_water(p)
    }

    /// The tilemap's lane marking at a point. `None` off-lane, or when the point
    /// touches cells of both lanes (only possible on the centre line).
    pub fn lane_at(&self, p: Vec2) -> Option<Lane> {
        let b = self.touching_bits(p);
        let l = b & self.bit_lane_left != 0;
        let r = b & self.bit_lane_right != 0;
        match (l, r) {
            (true, false) => Some(Lane::Left),
            (false, true) => Some(Lane::Right),
            _ => None,
        }
    }

    /// Lane by x alone (LOGIC_XPOS_BASED_TOWER_TARGETING). The exact centre line
    /// goes Left. PASS A FRAME x when the answer feeds a decision: in a team's
    /// frame "Left" is that team's own-left, and the centre-line rule then sends
    /// both seats to their own-left. "x is mirror-invariant so it cannot favour a
    /// team" holds only for a y-reflection; on an ENGINE x the centre-line rule
    /// sends a Blue unit on x = 9 to its own-left and its rotated Red twin to its
    /// own-right.
    ///
    /// THAT WARNING WAS TESTED AND HELD, 2026-09-23: the engine-frame reading was shipped for
    /// about an hour and RoyaleGym's seat-rotation gate went red at x = W/2 exactly, on the one
    /// seed of three where a unit stood there. See targeting.CENTRE_LANE_FRAME, which keeps both
    /// arms runnable and records that the 15.535.29 measurement does NOT separate them.
    ///
    /// NOT EVERY CALLER WANTS A FRAME x, AND `path.rs` IS NOT A BUG. Bridge selection passes an
    /// ENGINE x deliberately: the bridges are symmetric about W/2 and that is a different law
    /// with its own evidence (the `reflection_bridge_tie` plant). Do not "fix" it to match the
    /// sentence above, and do not unify the two ties because they look alike -- that would move
    /// the bridge rule on the strength of a tower measurement, which says nothing about it.
    #[inline]
    pub fn lane_by_x(&self, x: i32) -> Lane {
        if x * 2 <= self.width {
            Lane::Left
        } else {
            Lane::Right
        }
    }

    /// The seat mirror: the 180-degree rotation (W - x, H - y). An involution.
    /// Not `(x, H - y)`: that is the y-reflection, which is not the seat mirror.
    #[inline]
    pub fn rotate(&self, p: Vec2) -> Vec2 {
        Vec2::new(self.width - p.x, self.height - p.y)
    }

    /// Transform into a team's frame: identity for Blue, the 180-degree rotation
    /// for Red. In a team's frame that team defends low y AND its own-left is low
    /// x. Anything computed in-frame from in-frame inputs is seat-symmetric by
    /// construction -- including every "lower x" tie-break inside the planners,
    /// which therefore means own-left for both teams.
    #[inline]
    pub fn to_frame(&self, team: Team, p: Vec2) -> Vec2 {
        #[cfg(clash_plant = "reflection_frame")]
        {
            // PLANT (regression): the pre-ruling y-reflection frame.
            return match team {
                Team::Blue => p,
                Team::Red => Vec2::new(p.x, self.height - p.y),
            };
        }
        #[allow(unreachable_code)]
        match team {
            Team::Blue => p,
            Team::Red => self.rotate(p),
        }
    }

    /// Inverse of `to_frame` (the rotation is an involution).
    #[inline]
    pub fn from_frame(&self, team: Team, p: Vec2) -> Vec2 {
        self.to_frame(team, p)
    }

    /// +1 / -1: the y direction pointing toward a team's own king.
    #[inline]
    pub fn own_side_dy(team: Team) -> i32 {
        match team {
            Team::Blue => -1,
            Team::Red => 1,
        }
    }

    /// -1 / +1: the engine x direction pointing toward a team's own-LEFT (as that
    /// player sees the arena). Every x tie-break decided outside a frame uses it.
    #[inline]
    pub fn own_left_dx(team: Team) -> i32 {
        match team {
            Team::Blue => -1,
            Team::Red => 1,
        }
    }

    /// A bridge by ENGINE lane (Left is low engine x).
    pub fn bridge(&self, lane: Lane) -> Bridge {
        self.bridges[lane as usize]
    }

    /// King tower centre, derived from the king no-deploy block.
    pub fn king_tower_pos(&self, team: Team) -> Vec2 {
        let r = self.king_blocks[team as usize];
        Vec2::new((r.min.x + r.max.x) / 2, (r.min.y + r.max.y) / 2)
    }

    /// Princess tower centre by ENGINE lane: x from that bridge's centre line
    /// (derived), y from `PRINCESS_TOWER_Y_TILES_100` (not in the map), H - y for
    /// Red. Engine lanes, not own-frame lanes, because the tower table and the
    /// bindings name towers by engine lane (py.rs `slot_of_k` maps them to the
    /// protocol's own-frame slots). The two bridges are symmetric about W/2, so
    /// Red's Left tower is the rotation of Blue's Right one.
    pub fn princess_tower_pos(&self, team: Team, lane: Lane) -> Vec2 {
        let y = PRINCESS_TOWER_Y_TILES_100 * (SUBTILE / 100);
        let y = match team {
            Team::Blue => y,
            Team::Red => self.height - y,
        };
        Vec2::new(self.bridge(lane).center_x, y)
    }

    /// The king's shipped no-deploy block for a team.
    pub fn king_block(&self, team: Team) -> Rect {
        self.king_blocks[team as usize]
    }

    /// May `team` deploy at `p` under `territory`? Err says why, first failing
    /// check wins: out of arena, water, no-deploy, out of territory.
    ///
    /// CELL RULE, NOT POINT RULE. Territory is decided per half-cell, and a point
    /// must have EVERY cell it touches inside territory (closed cells, as for
    /// water). This is protocol.py's rule (mock_engine._check / action.
    /// PlacementOracle). A point rule on `to_frame(p).y` is NOT equivalent: it
    /// disagrees with the mask on the exact bank line y = 17 tiles on a bridge
    /// column (the point touches bridge row 33, which is not territory).
    ///
    /// TROOPS (`EnemyTowerRects`). Not in the river band (any touched cell), and
    /// not inside any closed rectangle of `enemy_rects` -- the NoDeploySize rects
    /// of the ALIVE enemy crown towers (see the TROOP TERRITORY note at the top of
    /// this file). The rects replace the older model, in which a fallen princess
    /// opened a fixed pocket depth past the far bank on the engine-column side.
    /// The rect test is a POINT rule; it equals the cell rule above exactly when
    /// every rect edge lies on a half-cell boundary, which the shipped sizes do
    /// (arena.rs test `rect_point_rule_equals_the_cell_rule`).
    ///
    /// THE RIVER BAND STAYS CLOSED TO TROOPS. UNSOURCED, carried from the previous
    /// rule: the rects alone would open the four dry bridge half-columns (16 cells
    /// per bridge) once that lane's princess falls, because the tilemap marks
    /// bridge cells neither WATER nor NO_DEPLOY. Whether the live game lets a troop
    /// be placed on the bridge then is a question to measure, not a data one.
    ///
    /// MIRROR. Red's rows are counted from the top (`rows - 1 - row`), and the rects
    /// are built from tower positions that the rotation maps onto each other, so
    /// the verdict for Red at (W - x, H - y) equals Blue's at (x, y) with every
    /// tower rotated -- no lane or column enters the troop rule at all.
    pub fn deploy_zone(&self, p: Vec2, team: Team, territory: Territory, enemy_rects: &[Rect]) -> Result<(), ZoneError> {
        if !(p.x > 0 && p.y > 0 && p.x < self.width && p.y < self.height) {
            return Err(ZoneError::OutOfArena);
        }
        if territory == Territory::Anywhere {
            return Ok(());
        }
        let bits = self.touching_bits(p);
        if bits & self.bit_water != 0 {
            return Err(ZoneError::Water);
        }
        if territory == Territory::AnywhereButWater {
            return Ok(());
        }
        if bits & self.bit_no_deploy != 0 {
            return Err(ZoneError::NoDeploy);
        }
        self.territory_zone(p, team, territory, enemy_rects)
    }

    /// The TERRITORY half of `deploy_zone` alone: the river band and the enemy
    /// rects, with no bounds, water-touch or NO_DEPLOY-bit test. The per-tile mask
    /// the summon formation's column clamp scans (state.rs `ground_y_range`;
    /// calibration formation.GROUND_Y_CLAMP): the live Goblin Gang tapped on
    /// (3500, 1500) puts a Spear Goblin on y 346 in the bit-16 corner strip, so
    /// that strip is inside the clamp's range.
    pub fn territory_zone(&self, p: Vec2, team: Team, territory: Territory, enemy_rects: &[Rect]) -> Result<(), ZoneError> {
        let water_lo = self.water_y_min / self.cell;
        let water_hi = self.water_y_max / self.cell - 1;
        let (y0, y1) = self.axis_span(p.y, self.rows);
        for row in y0..=y1 {
            let own_row = match team {
                Team::Blue => row,
                Team::Red => self.rows - 1 - row,
            };
            let refused = match territory {
                Territory::OwnHalf => own_row >= water_lo,
                Territory::EnemyTowerRects => own_row >= water_lo && own_row <= water_hi,
                Territory::Anywhere | Territory::AnywhereButWater => false,
            };
            if refused {
                return Err(ZoneError::OutOfTerritory);
            }
        }
        if territory == Territory::EnemyTowerRects && enemy_rects.iter().any(|r| r.contains_closed(p)) {
            return Err(ZoneError::OutOfTerritory);
        }
        Ok(())
    }

    /// A tower's NoDeploySize rectangle: centred on `center`, full size `size`
    /// (subtiles). Sizes are whole tiles in the data, so `size / 2` is exact.
    pub fn no_deploy_rect(center: Vec2, size: Vec2) -> Rect {
        let half = Vec2::new(size.x / 2, size.y / 2);
        Rect { min: center.sub(half), max: center.add(half) }
    }

    /// Footprint of a building under a model. `king_of` names the team whose
    /// king this is, when it is one (StaticBitmap uses the shipped block).
    pub fn building_shape(&self, model: FootprintModel, pos: Vec2, radius: i32, king_of: Option<Team>) -> Shape {
        match model {
            FootprintModel::CollisionRadiusCircle => Shape::Circle { c: pos, r: radius },
            FootprintModel::TileSizeOverrideBox => Shape::Box(Rect {
                min: Vec2::new(pos.x - radius, pos.y - radius),
                max: Vec2::new(pos.x + radius, pos.y + radius),
            }),
            FootprintModel::StaticBitmap => {
                if let Some(team) = king_of {
                    return Shape::Box(self.king_block(team));
                }
                // Not in the map: snap the radius box outward to half-tiles.
                let half = (radius + self.cell - 1) / self.cell * self.cell;
                Shape::Box(Rect {
                    min: Vec2::new(pos.x - half, pos.y - half),
                    max: Vec2::new(pos.x + half, pos.y + half),
                })
            }
        }
    }

    /// Is the grid invariant under the 180-degree seat rotation? Every cell must
    /// equal its rotated cell, with the two LANE bits swapped (a lane is named in
    /// engine x, and the rotation maps the left lane onto the right one). Also the
    /// bridges must be symmetric about W/2 and the king blocks rotations of each
    /// other. The seat-symmetry guarantees depend on all of it.
    /// y-symmetry alone (`is_y_symmetric`) is the precondition of a REFLECTION and
    /// is not sufficient here.
    pub fn is_rotation_symmetric(&self) -> bool {
        let lanes = self.bit_lane_left | self.bit_lane_right;
        let swap = |v: u8| {
            (v & !lanes)
                | if v & self.bit_lane_left != 0 { self.bit_lane_right } else { 0 }
                | if v & self.bit_lane_right != 0 { self.bit_lane_left } else { 0 }
        };
        let cells = (0..self.rows)
            .all(|r| (0..self.cols).all(|c| self.cell_bits(c, r) == swap(self.cell_bits(self.cols - 1 - c, self.rows - 1 - r))));
        let bridges = self.bridges.len() == 2
            && self.bridges[0].x_min == self.width - self.bridges[1].x_max
            && self.bridges[0].x_max == self.width - self.bridges[1].x_min
            && self.bridges[0].center_x == self.width - self.bridges[1].center_x;
        let kb = |b: Rect| Rect { min: self.rotate(b.max), max: self.rotate(b.min) };
        let kings = self.king_blocks.len() == 2 && kb(self.king_blocks[0]) == self.king_blocks[1];
        cells && bridges && kings
    }
}

// BUILDING PLACEMENT: THE TILE FOOTPRINT
//
// A building occupies a whole number of TILES when it is placed, and that box is
// a different thing from the CollisionRadius circle movement uses
// (collision.BUILDING_FOOTPRINT_MODEL). Measured over 114 recorded placements:
// troop centres sit inside a building's box routinely -- inside a Cannon's on 394
// of 943 nearby frames and inside a princess tower's on 11923 of 49173 -- so the
// box never blocks a unit. It decides where a building may be PUT.
//
// See calibration.json placement.* for the sizes, the snap, the legality rule and
// what the recordings could not discriminate.

/// Tiles on a side of a building's placement footprint, from its CollisionRadius
/// (both in subtiles): `ceil(2R / tile) + 1`.
///
/// R 500 gives 2 (Tesla), R 600 gives 3 (Cannon, Bomb Tower, Inferno Tower,
/// Mortar, X-Bow), R 1000 gives 3 (Tombstone, the huts, Goblin Cage, Elixir
/// Collector, a princess tower) and R 1400 gives 4 (a king tower). Pinned per card
/// on the recordings for Cannon, Tombstone, Goblin Hut, Bomb Tower, Goblin Cage,
/// Barbarian Hut and Tesla; the others follow the rule and are not individually
/// pinned. `floor + 1` and `round + 1` both fail on R 600.
#[inline]
pub fn placement_tiles(collision_radius: i32) -> i32 {
    let t = crate::fixed::tiles(1);
    (2 * collision_radius + t - 1) / t + 1
}

impl Rect {
    /// Do the two rectangles share POSITIVE AREA? Touching edges and corners do
    /// not. The recordings are unambiguous: over 191 near pairs of live boxes,
    /// none overlaps and 93 touch exactly, and one recorded Cannon stands flush
    /// against a princess box, a king block and a Goblin Cage at the same time.
    #[inline]
    pub fn overlaps_open(&self, other: &Rect) -> bool {
        self.min.x < other.max.x && other.min.x < self.max.x && self.min.y < other.max.y && other.min.y < self.max.y
    }
}

impl Arena {
    /// Where a tap puts an `n`-tile building's CENTRE.
    ///
    /// An odd `n` lands on the centre of the tapped tile, an even `n` on one of
    /// its corners. Every one of the 102 odd-rule placements recorded sits on a
    /// tile centre and every one of the 24 even-rule placements on a corner.
    ///
    /// WHICH corner is UNMEASURED: every even-size placement in the corpus was
    /// made by one seat, so "the arena's own lower-left corner" and "the placer's
    /// lower-left corner" fit it equally. This takes the placer's, so the two
    /// seats mirror; the rival arm and the observation that would separate them
    /// are in calibration.json placement.SNAP_EVEN_CORNER.
    pub fn snap_placement(&self, team: Team, tap: Vec2, n: i32) -> Vec2 {
        let t = crate::fixed::tiles(1);
        let f = self.to_frame(team, tap);
        let half = if n % 2 == 1 { t / 2 } else { 0 };
        let snapped = Vec2::new(f.x.div_euclid(t) * t + half, f.y.div_euclid(t) * t + half);
        self.from_frame(team, snapped)
    }

    /// The `n` x `n` tile box centred on `centre`. Its edges land on tile
    /// boundaries for the centres `snap_placement` produces, whatever `n`'s parity.
    #[inline]
    pub fn placement_box(centre: Vec2, n: i32) -> Rect {
        let half = n * crate::fixed::tiles(1) / 2;
        Rect { min: Vec2::new(centre.x - half, centre.y - half), max: Vec2::new(centre.x + half, centre.y + half) }
    }

    /// Bitwise OR of every half-cell the box covers with positive area. A box
    /// flush against a cell's edge does not cover it, which is what lets a legal
    /// box sit against the river line and the back no-deploy strip.
    pub fn box_bits(&self, b: Rect) -> u8 {
        let c = self.cell;
        let col0 = b.min.x.div_euclid(c).max(0);
        let col1 = (b.max.x - 1).div_euclid(c).min(self.cols - 1);
        let row0 = b.min.y.div_euclid(c).max(0);
        let row1 = (b.max.y - 1).div_euclid(c).min(self.rows - 1);
        let mut acc = 0u8;
        for row in row0..=row1 {
            for col in col0..=col1 {
                acc |= self.cell_bits(col, row);
            }
        }
        acc
    }

    /// May a building of footprint `b` stand here? The checks run in `deploy_zone`'s
    /// order so the reason a player sees does not depend on which rule is asked
    /// first: inside the arena (flush against a wall is fine), no water half-cell,
    /// no no-deploy half-cell, then the territory band in the placer's own frame.
    ///
    /// Seat symmetry: the band is judged on the box's forward edge in the team's
    /// own frame, and the rotation maps one seat's box onto the other's.
    pub fn box_zone(&self, b: Rect, team: Team, territory: Territory) -> Result<(), ZoneError> {
        if b.min.x < 0 || b.min.y < 0 || b.max.x > self.width || b.max.y > self.height {
            return Err(ZoneError::OutOfArena);
        }
        if territory == Territory::Anywhere {
            return Ok(());
        }
        let bits = self.box_bits(b);
        if bits & self.bit_water != 0 {
            return Err(ZoneError::Water);
        }
        if territory == Territory::AnywhereButWater {
            return Ok(());
        }
        if bits & self.bit_no_deploy != 0 {
            return Err(ZoneError::NoDeploy);
        }
        // The forward edge in the placer's frame. `to_frame` rotates for Red, so
        // the box's own max-y corner maps to its min-y corner and back.
        let c0 = self.to_frame(team, b.min);
        let c1 = self.to_frame(team, b.max);
        let forward_y = c0.y.max(c1.y);
        let water_lo = self.water_y_min / self.cell;
        let water_hi = self.water_y_max / self.cell - 1;
        // The last row the box covers with positive area.
        let row = (forward_y - 1).div_euclid(self.cell);
        let refused = match territory {
            Territory::OwnHalf => row >= water_lo,
            Territory::EnemyTowerRects => row >= water_lo && row <= water_hi,
            Territory::Anywhere | Territory::AnywhereButWater => false,
        };
        if refused {
            return Err(ZoneError::OutOfTerritory);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixed::tiles;

    fn t100(x: i32, y: i32) -> Vec2 {
        Vec2::from_tiles_100(x, y)
    }

    #[test]
    fn shipped_arena_has_the_gated_geometry() {
        let a = Arena::shipped();
        assert_eq!((a.cols, a.rows, a.cell), (36, 64, 9000));
        assert_eq!((a.width, a.height), (tiles(18), tiles(32)));
        assert_eq!((a.water_y_min, a.water_y_max), (tiles(15), tiles(17)));
        assert_eq!(a.bridges[0], Bridge { x_min: 45000, x_max: 81000, center_x: 63000 });
        assert_eq!(a.king_tower_pos(Team::Blue), t100(900, 300));
        assert_eq!(a.king_tower_pos(Team::Red), t100(900, 2900));
        assert_eq!(a.princess_tower_pos(Team::Blue, Lane::Left), t100(350, 650));
        assert_eq!(a.princess_tower_pos(Team::Red, Lane::Right), t100(1450, 2550));
        assert!(a.is_rotation_symmetric());
        // A reflection-only arena must NOT pass: shift one bridge by a half-cell.
        let mut bent = a.clone();
        bent.bridges[0].x_min += a.cell;
        assert!(!bent.is_rotation_symmetric(), "the symmetry check cannot fail");
    }

    #[test]
    fn water_and_bridges() {
        let a = Arena::shipped();
        assert!(a.is_water(t100(900, 1600)));
        assert!(!a.is_passable_ground(t100(900, 1600)));
        assert!(a.is_passable_ground(t100(350, 1600)), "left bridge centre is dry");
        assert!(a.is_passable_ground(t100(1450, 1600)));
        // bridge edge x = 2.5 touches water, x = 2.51 does not
        assert!(a.is_water(t100(250, 1600)));
        assert!(!a.is_water(Vec2::new(45001, tiles(16))));
    }

    #[test]
    fn closed_cells_make_water_rotation_symmetric() {
        let a = Arena::shipped();
        for y in [tiles(15), tiles(15) - 1, tiles(15) + 1, tiles(17), tiles(17) + 1, tiles(17) - 1] {
            for x in [0, 1000, 45000, 45001, 63000, 81000, 162000, 243000, 279000] {
                let p = Vec2::new(x, y);
                assert_eq!(a.is_water(p), a.is_water(a.rotate(p)), "at {p:?}");
            }
        }
    }

    /// Alive-enemy rects for `team` from the repository's cards.json (never a
    /// copied size): [king, engine-left princess, engine-right princess].
    fn enemy_rects(a: &Arena, team: Team) -> [Rect; 3] {
        let db = crate::card::CardDb::load_repo().expect("data/derived/cards.json (run tools/extract_cards.py)");
        let size = |n: &str| db.get(db.index(n).unwrap()).no_deploy_size.expect("crown tower NoDeploySize");
        let e = team.other();
        [
            Arena::no_deploy_rect(a.king_tower_pos(e), size(crate::card::KING_TOWER)),
            Arena::no_deploy_rect(a.princess_tower_pos(e, Lane::Left), size(crate::card::PRINCESS_TOWER)),
            Arena::no_deploy_rect(a.princess_tower_pos(e, Lane::Right), size(crate::card::PRINCESS_TOWER)),
        ]
    }

    #[test]
    fn deploy_zones() {
        use Territory::*;
        let a = Arena::shipped();
        let rb = enemy_rects(&a, Team::Blue);
        let rr = enemy_rects(&a, Team::Red);
        let all_b = rb.to_vec();
        let left_down_b = vec![rb[0], rb[2]];
        let z = |p, team, terr, rects: &[Rect]| a.deploy_zone(p, team, terr, rects);
        assert_eq!(z(t100(900, 1000), Team::Blue, EnemyTowerRects, &all_b), Ok(()));
        assert_eq!(z(t100(900, 1000), Team::Red, EnemyTowerRects, &rr), Err(ZoneError::OutOfTerritory));
        assert_eq!(z(t100(900, 2200), Team::Red, EnemyTowerRects, &rr), Ok(()));
        assert_eq!(z(t100(900, 300), Team::Blue, EnemyTowerRects, &all_b), Err(ZoneError::NoDeploy), "king block");
        assert_eq!(z(t100(900, 1600), Team::Blue, EnemyTowerRects, &[]), Err(ZoneError::Water));
        assert_eq!(z(Vec2::new(0, 5), Team::Blue, Anywhere, &all_b), Err(ZoneError::OutOfArena));
        assert_eq!(z(t100(900, 2200), Team::Blue, Anywhere, &all_b), Ok(()), "spells go anywhere");
        assert_eq!(z(t100(350, 2000), Team::Blue, EnemyTowerRects, &all_b), Err(ZoneError::OutOfTerritory));
        assert_eq!(z(t100(350, 2000), Team::Blue, EnemyTowerRects, &left_down_b), Ok(()), "pocket opens");
        assert_eq!(z(t100(350, 2000), Team::Blue, OwnHalf, &left_down_b), Err(ZoneError::OutOfTerritory), "no pocket for buildings");
        assert_eq!(z(t100(1450, 2000), Team::Blue, EnemyTowerRects, &left_down_b), Err(ZoneError::OutOfTerritory));
        // Depth: the enemy king rect starts at y = 21 (closed), so the pocket is
        // y in (17, 21) -- 8 half-rows, where a 12-half-row pocket would reach 23 tiles.
        assert_eq!(z(Vec2::new(tiles(3), tiles(21) - 1), Team::Blue, EnemyTowerRects, &left_down_b), Ok(()));
        assert_eq!(z(Vec2::new(tiles(3), tiles(21)), Team::Blue, EnemyTowerRects, &left_down_b), Err(ZoneError::OutOfTerritory));
        // The side edge: the surviving right princess rect starts at x = 9 (closed).
        assert_eq!(z(Vec2::new(tiles(9) - 1, tiles(19)), Team::Blue, EnemyTowerRects, &left_down_b), Ok(()));
        assert_eq!(z(Vec2::new(tiles(9), tiles(19)), Team::Blue, EnemyTowerRects, &left_down_b), Err(ZoneError::OutOfTerritory));
        // The bank line on a bridge column touches bridge row 33: the river band.
        assert_eq!(z(t100(350, 1700), Team::Blue, EnemyTowerRects, &left_down_b), Err(ZoneError::OutOfTerritory));
        assert_eq!(z(t100(350, 1600), Team::Blue, EnemyTowerRects, &left_down_b), Err(ZoneError::OutOfTerritory), "bridge stays closed");
        // Rotation: Red at (W - x, H - y) with Blue's ENGINE-RIGHT tower down (the
        // rotation of Red's engine-left) gets exactly Blue's verdicts.
        let right_down_r = vec![rr[0], rr[1]];
        for (bx, by) in [(350, 2000), (1450, 2000), (350, 1700), (900, 1000), (300, 2099), (300, 2100), (899, 1900), (900, 1900)] {
            let p = t100(bx, by);
            assert_eq!(
                z(p, Team::Blue, EnemyTowerRects, &left_down_b),
                z(a.rotate(p), Team::Red, EnemyTowerRects, &right_down_r),
                "at {p:?}"
            );
        }
    }

    #[test]
    fn rect_point_rule_equals_the_cell_rule() {
        // The rect test is a point rule; the mask's territory is a cell rule. They
        // agree iff every rect edge is on a half-cell boundary. Check it for the
        // shipped sizes, at every half-cell centre AND corner.
        let a = Arena::shipped();
        let mut checked = 0;
        for team in [Team::Blue, Team::Red] {
            let rects = enemy_rects(&a, team);
            for r in rects {
                for v in [r.min.x, r.min.y, r.max.x, r.max.y] {
                    assert_eq!(v.rem_euclid(a.cell), 0, "rect edge {v} is not on a half-cell boundary: {r:?}");
                }
            }
            for gy in 0..=2 * a.rows {
                for gx in 0..=2 * a.cols {
                    let p = Vec2::new(gx * a.cell / 2, gy * a.cell / 2);
                    let point = rects.iter().any(|r| r.contains_closed(p));
                    let (x0, x1) = a.axis_span(p.x, a.cols);
                    let (y0, y1) = a.axis_span(p.y, a.rows);
                    let cell = (y0..=y1).any(|row| {
                        (x0..=x1).any(|col| {
                            let (lx, ly) = (col * a.cell, row * a.cell);
                            rects.iter().any(|r| lx < r.max.x && lx + a.cell > r.min.x && ly < r.max.y && ly + a.cell > r.min.y)
                        })
                    });
                    assert_eq!(point, cell, "{team:?} at {p:?}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 10_000, "vacuous: {checked} points");
    }

    #[test]
    fn push_out_dead_centre_ties_go_to_each_teams_own_side_and_own_left() {
        let wide = Shape::Box(Rect { min: Vec2::new(-18000, -9000), max: Vec2::new(18000, 9000) });
        let tall = Shape::Box(Rect { min: Vec2::new(-9000, -18000), max: Vec2::new(9000, 18000) });
        let c = Vec2::new(0, 0);
        assert_eq!(tall.push_out(c, 100, Team::Blue), Some(Vec2::new(-9100, 0)), "Blue's own-left is -x");
        assert_eq!(tall.push_out(c, 100, Team::Red), Some(Vec2::new(9100, 0)), "Red's own-left is +x");
        assert_eq!(wide.push_out(c, 100, Team::Blue), Some(Vec2::new(0, -9100)));
        assert_eq!(wide.push_out(c, 100, Team::Red), Some(Vec2::new(0, 9100)));
    }

    #[test]
    fn lanes() {
        let a = Arena::shipped();
        assert_eq!(a.lane_at(t100(350, 1000)), Some(Lane::Left));
        assert_eq!(a.lane_at(t100(1450, 1000)), Some(Lane::Right));
        assert_eq!(a.lane_at(t100(900, 1000)), None);
        assert_eq!(a.lane_by_x(tiles(9)), Lane::Left);
        assert_eq!(a.lane_by_x(tiles(9) + 1), Lane::Right);
    }

    #[test]
    fn push_out_leaves_disc_touching_not_inside() {
        let s = Shape::Circle { c: Vec2::new(0, 0), r: 18000 };
        let p = s.push_out(Vec2::new(3000, 4000), 9000, Team::Blue).unwrap();
        assert!(!s.penetrates(p, 9000), "{p:?}");
        let b = Shape::Box(Rect { min: Vec2::new(-18000, -18000), max: Vec2::new(18000, 18000) });
        let q = b.push_out(Vec2::new(1000, 0), 9000, Team::Red).unwrap();
        assert!(!b.penetrates(q, 9000), "{q:?}");
        let q2 = b.push_out(Vec2::new(20000, 20000), 9000, Team::Red).unwrap();
        assert!(!b.penetrates(q2, 9000), "{q2:?}");
    }

    #[test]
    fn segment_tests() {
        let s = Shape::Circle { c: Vec2::new(0, 0), r: 1000 };
        assert!(s.segment_hits(Vec2::new(-5000, 500), Vec2::new(5000, 500), 0));
        assert!(!s.segment_hits(Vec2::new(-5000, 1500), Vec2::new(5000, 1500), 0));
        assert!(s.segment_hits(Vec2::new(-5000, 1500), Vec2::new(5000, 1500), 600));
        let b = Shape::Box(Rect { min: Vec2::new(-1000, -1000), max: Vec2::new(1000, 1000) });
        assert!(b.segment_hits(Vec2::new(-5000, 0), Vec2::new(5000, 0), 0));
        assert!(!b.segment_hits(Vec2::new(-5000, 2000), Vec2::new(5000, 3000), 0));
        assert!(!b.segment_hits(Vec2::new(-5000, 0), Vec2::new(-2000, 0), 0));
    }

    /// Catches a size rule that rounds the wrong way. `floor + 1` gives a Cannon
    /// 2 tiles and `round + 1` gives it 2 as well; the recordings pin 3.
    #[test]
    fn placement_size_comes_from_the_collision_radius() {
        use crate::fixed::milli;
        assert_eq!(placement_tiles(milli(500)), 2, "Tesla");
        assert_eq!(placement_tiles(milli(600)), 3, "Cannon");
        assert_eq!(placement_tiles(milli(1000)), 3, "Tombstone and a princess tower");
        assert_eq!(placement_tiles(milli(1400)), 4, "a king tower");
        // The rule is CEILING: R 600 is the witness, because 2R is 1.2 tiles.
        assert_eq!(2 * milli(600) / tiles(1), 1, "floor of 2R would give 2 tiles");
    }

    /// Catches a snap that keeps the tap, or that puts an even-size building on a
    /// tile centre, and a box whose edges miss the tile grid.
    #[test]
    fn placement_snaps_to_the_grid_and_the_box_is_tile_aligned() {
        let a = Arena::shipped();
        let t = tiles(1);
        for (n, want) in [(3, t / 2), (2, 0)] {
            let c = a.snap_placement(Team::Blue, Vec2::new(t * 5 + 1234, t * 9 + 17999), n);
            assert_eq!((c.x - want) % t, 0, "n {n} x off grid: {c:?}");
            assert_eq!((c.y - want) % t, 0, "n {n} y off grid: {c:?}");
            let b = Arena::placement_box(c, n);
            assert_eq!(b.max.x - b.min.x, n * t, "n {n} width");
            assert_eq!(b.max.y - b.min.y, n * t, "n {n} height");
            assert_eq!(b.min.x % t, 0, "n {n} box left off the tile grid");
            assert_eq!(b.min.y % t, 0, "n {n} box bottom off the tile grid");
        }
    }

    /// Catches an even-size snap decided in engine coordinates. The corner a 2x2
    /// takes is the PLACER's, so one seat's answer must be the other's rotation.
    /// An absolute `floor` puts the two seats' boxes a tile apart.
    #[test]
    fn the_snap_is_the_same_for_both_seats() {
        let a = Arena::shipped();
        for n in [2, 3] {
            for tap in [Vec2::new(63500, 121500), Vec2::new(1, 1), Vec2::new(100_000, 240_000)] {
                let blue = a.snap_placement(Team::Blue, tap, n);
                let red = a.snap_placement(Team::Red, a.rotate(tap), n);
                assert_eq!(a.rotate(blue), red, "n {n} tap {tap:?}");
            }
        }
    }

    /// Catches an overlap test written with `<=`, which would refuse the flush
    /// placements the recordings show standing (93 exact touches, 0 overlaps).
    #[test]
    fn boxes_may_touch_but_not_overlap() {
        let t = tiles(1);
        let at = |x: i32, y: i32, n: i32| Arena::placement_box(Vec2::new(x * t + t / 2, y * t + t / 2), n);
        let cannon = at(5, 5, 3);
        assert!(!cannon.overlaps_open(&at(8, 5, 3)), "edge to edge is legal");
        assert!(!cannon.overlaps_open(&at(8, 8, 3)), "corner to corner is legal");
        assert!(cannon.overlaps_open(&at(7, 5, 3)), "one tile of shared area is not");
        assert!(cannon.overlaps_open(&cannon), "a box overlaps itself");
    }

    /// Catches the P0 itself: a check that tests only the tap point. Each tap here
    /// is a legal POINT whose 3x3 box is not.
    #[test]
    fn a_three_by_three_box_is_judged_whole() {
        let a = Arena::shipped();
        let t = tiles(1);
        let boxed = |x: i32, y: i32| Arena::placement_box(a.snap_placement(Team::Blue, Vec2::new(x, y), 3), 3);
        // The back wall: the tap sits in row 0, so the box would leave the arena.
        assert_eq!(a.box_zone(boxed(t * 9 + t / 2, t / 2), Team::Blue, Territory::OwnHalf), Err(ZoneError::OutOfArena));
        // The side wall, at mid-field where no no-deploy strip hides the defect.
        assert_eq!(a.box_zone(boxed(t / 2, t * 8 + t / 2), Team::Blue, Territory::OwnHalf), Err(ZoneError::OutOfArena));
        // The river bank: the box would cross into the water rows.
        assert_eq!(a.box_zone(boxed(t * 9 + t / 2, t * 14 + t / 2), Team::Blue, Territory::OwnHalf), Err(ZoneError::Water));
        // The king's own no-deploy block is a THIRD reason a box can be refused,
        // and it sits directly in front of the king. A box that covers any of it
        // is NoDeploy, not OutOfArena.
        assert_eq!(a.box_zone(boxed(t * 9 + t / 2, t + t / 2), Team::Blue, Territory::OwnHalf), Err(ZoneError::NoDeploy));
        // Open ground is legal, and so is a box sitting flush on the river line
        // (own-frame rows up to 15). These are the controls: without them the
        // test would pass on a box_zone that refused everything.
        assert_eq!(a.box_zone(boxed(t + t / 2, t * 8 + t / 2), Team::Blue, Territory::OwnHalf), Ok(()));
        assert_eq!(a.box_zone(boxed(t * 6 + t / 2, t * 13 + t / 2), Team::Blue, Territory::OwnHalf), Ok(()));
        assert_eq!(a.box_zone(boxed(t * 6 + t / 2, t * 8 + t / 2), Team::Blue, Territory::OwnHalf), Ok(()));
    }

    /// Catches a box zone rule that reads engine y instead of the placer's.
    #[test]
    fn the_box_zone_gives_both_seats_the_same_verdict() {
        let a = Arena::shipped();
        let t = tiles(1);
        for n in [2, 3] {
            for (x, y) in [(9, 0), (0, 8), (9, 14), (9, 1), (6, 13), (3, 6)] {
                let tap = Vec2::new(x * t + t / 2, y * t + t / 2);
                let blue = a.box_zone(Arena::placement_box(a.snap_placement(Team::Blue, tap, n), n), Team::Blue, Territory::OwnHalf);
                let red = a.box_zone(
                    Arena::placement_box(a.snap_placement(Team::Red, a.rotate(tap), n), n),
                    Team::Red,
                    Territory::OwnHalf,
                );
                assert_eq!(blue, red, "n {n} at tile ({x}, {y})");
            }
        }
    }
}
