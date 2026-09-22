//! Integer geometry. There are no floats in this engine and this module is why.
//!
//! WHY NO FLOATS
//!     A float clock accumulated per tick (`time += 0.033`) reads
//!     29.70000000000043 seconds after 900 ticks instead of 30. That is not a
//!     rounding curiosity: it means two runs of the same battle on two machines
//!     can diverge, which makes every test unreproducible and every "the sim
//!     disagrees with the game" report unattributable.
//!
//! THE UNIT
//!     1 tile = SUBTILE (18000) subtiles. See data/calibration.json for the full
//!     rationale; the short version is that 18000 makes per-tick displacement an
//!     exact integer under EVERY candidate hypothesis for tick rate and speed
//!     units, so the representation does not quietly pick a side in the two
//!     biggest open questions.
//!
//! ```text
//! 1 millitile (the unit all shipped game data uses) = 18 subtiles
//! Speed is tiles/min, 20 TPS -> Speed * 15 subtiles/tick
//! Speed is tiles/min, 30 TPS -> Speed * 10 subtiles/tick
//! Speed is tiles/min, 60 TPS -> Speed *  5 subtiles/tick
//! Speed is millitiles/50ms  -> Speed * 18 subtiles/tick
//! ```
//!
//! OVERFLOW
//!     Arena diagonal is ~37 tiles = 660k subtiles. Squaring that is 4.4e11,
//!     which does NOT fit in i32. Every squared distance here returns i64 and
//!     the types make it impossible to get wrong by accident.

/// Subtiles per tile. The engine's length unit.
pub const SUBTILE: i32 = 18_000;
/// Subtiles per millitile. All shipped game data is in millitiles.
pub const SUBTILE_PER_MILLITILE: i32 = 18;

/// Convert a shipped game-data distance (millitiles) into subtiles.
#[inline]
pub const fn milli(millitiles: i32) -> i32 {
    millitiles * SUBTILE_PER_MILLITILE
}

/// Convert whole tiles into subtiles.
#[inline]
pub const fn tiles(t: i32) -> i32 {
    t * SUBTILE
}

/// Convert hundredths of a tile into subtiles (SUBTILE / 100 = 180, exact). The
/// unit calibration charge.CHARGE_RANGE_UNIT = centitiles reads ChargeRange in.
#[inline]
pub const fn centi(centitiles: i32) -> i32 {
    centitiles * (SUBTILE / 100)
}

/// A position or vector in subtiles.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Vec2 {
    pub x: i32,
    pub y: i32,
}

impl Vec2 {
    #[inline]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Build from tile coordinates expressed in hundredths, so callers can say
    /// `Vec2::from_tiles_100(350, 650)` for tile (3.5, 6.5) without a float.
    #[inline]
    pub const fn from_tiles_100(x100: i32, y100: i32) -> Self {
        Self { x: x100 * (SUBTILE / 100), y: y100 * (SUBTILE / 100) }
    }

    #[inline]
    pub const fn add(self, o: Vec2) -> Vec2 {
        Vec2 { x: self.x + o.x, y: self.y + o.y }
    }

    #[inline]
    pub const fn sub(self, o: Vec2) -> Vec2 {
        Vec2 { x: self.x - o.x, y: self.y - o.y }
    }

    /// Squared distance. i64 because i32 overflows across the arena diagonal.
    #[inline]
    pub fn dist2(self, o: Vec2) -> i64 {
        let dx = (self.x - o.x) as i64;
        let dy = (self.y - o.y) as i64;
        dx * dx + dy * dy
    }

    #[inline]
    pub fn len2(self) -> i64 {
        let x = self.x as i64;
        let y = self.y as i64;
        x * x + y * y
    }

    /// Integer length, truncated. Uses isqrt so it is exact and platform-stable;
    /// a float sqrt would reintroduce exactly the nondeterminism this module exists
    /// to prevent.
    // A vector's magnitude, not a container's element count: `is_empty` has no
    // meaning here, so clippy's len-without-is_empty lint does not apply.
    #[allow(clippy::len_without_is_empty)]
    #[inline]
    pub fn len(self) -> i32 {
        isqrt(self.len2()) as i32
    }

    #[inline]
    pub fn dist(self, o: Vec2) -> i32 {
        isqrt(self.dist2(o)) as i32
    }

    /// Step `amount` subtiles from self toward `target`, never overshooting.
    ///
    /// Rounding is truncation toward zero and is applied to the STEP, not to the
    /// position, so a caller that accumulates a remainder (see `Mover`) gets an
    /// exact running total rather than a per-tick truncation that drifts. Naive
    /// per-tick truncation costs about 0.2 tile per minute, which over a 3-minute
    /// match is most of a tile -- enough to change whether a unit reaches a bridge
    /// before a spell lands.
    #[inline]
    pub fn step_toward(self, target: Vec2, amount: i32) -> Vec2 {
        let d = target.sub(self);
        let len = d.len();
        if len == 0 || amount >= len {
            return target;
        }
        Vec2 {
            x: self.x + mul_div(d.x, amount, len),
            y: self.y + mul_div(d.y, amount, len),
        }
    }
}

/// `a * b / c` computed in i64 so the intermediate cannot overflow.
#[inline]
pub fn mul_div(a: i32, b: i32, c: i32) -> i32 {
    debug_assert!(c != 0, "mul_div by zero");
    ((a as i64) * (b as i64) / (c as i64)) as i32
}

/// Integer square root, truncated. Newton's method on integers: exact, branch-
/// deterministic, and identical on every platform.
#[inline]
pub fn isqrt(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    if n < 4 {
        return 1;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Are two circles overlapping or touching? Pure integer, no sqrt.
#[inline]
pub fn circles_overlap(a: Vec2, ra: i32, b: Vec2, rb: i32) -> bool {
    let r = (ra as i64) + (rb as i64);
    a.dist2(b) <= r * r
}

/// Is `target` within `range` of `from`, measured EDGE to EDGE?
///
/// globals.csv ships ADD_CHARACTER_RANGE_TO_RADIUS = TRUE, so the game measures
/// to the target's hitbox edge, not its centre. The old engine measured centre to
/// centre, which made every melee unit under-reach by the target's radius -- half
/// a tile against most troops and a full 1.4 tiles against a king tower.
#[inline]
pub fn in_range_edge(from: Vec2, target: Vec2, range: i32, target_radius: i32) -> bool {
    let r = (range as i64) + (target_radius as i64);
    from.dist2(target) <= r * r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isqrt_is_exact_on_perfect_squares() {
        for n in 0..2000i64 {
            assert_eq!(isqrt(n * n), n, "isqrt({})", n * n);
        }
    }

    #[test]
    fn isqrt_truncates_and_never_overshoots() {
        for n in 0..50_000i64 {
            let r = isqrt(n);
            assert!(r * r <= n, "isqrt({n}) = {r} overshoots");
            assert!((r + 1) * (r + 1) > n, "isqrt({n}) = {r} undershoots");
        }
    }

    #[test]
    fn isqrt_handles_arena_scale_without_overflow() {
        // Arena diagonal: 18 x 32 tiles.
        let d = Vec2::new(0, 0).dist2(Vec2::new(tiles(18), tiles(32)));
        assert!(d > i32::MAX as i64, "arena diagonal squared must exceed i32 -- \
            if this ever fits, someone shrank the unit and dist2 could be i32 again");
        let r = isqrt(d);
        assert!(r > tiles(36) as i64 && r < tiles(37) as i64, "got {r}");
    }

    #[test]
    fn millitile_conversion_is_exact() {
        assert_eq!(milli(1000), SUBTILE); // 1000 millitiles == 1 tile
        assert_eq!(milli(500), SUBTILE / 2); // Knight collision radius 0.5 tiles
        assert_eq!(milli(5500), SUBTILE * 55 / 10); // Knight sight 5.5 tiles
    }

    #[test]
    fn speed_conversion_is_exact_under_every_hypothesis() {
        // The whole point of SUBTILE = 18000. If any of these ever leaves a
        // remainder, the representation has started taking sides in an open
        // question and the calibration registry is lying.
        for speed in [30, 45, 60, 90, 120] {
            // tiles/min at 20/30/60 TPS
            for (tps, mult) in [(20, 15), (30, 10), (60, 5)] {
                let exact = (speed as i64) * (SUBTILE as i64) / (60 * tps as i64);
                assert_eq!(exact, (speed * mult) as i64, "speed {speed} at {tps} TPS");
            }
            // millitiles per 50 ms tick
            assert_eq!(milli(speed) as i64, (speed * 18) as i64, "speed {speed} millitile reading");
        }
    }

    #[test]
    fn step_toward_never_overshoots() {
        let a = Vec2::new(0, 0);
        let b = Vec2::new(tiles(3), tiles(4)); // 5 tiles away exactly
        assert_eq!(a.dist(b), tiles(5));
        let s = a.step_toward(b, tiles(1));
        assert!(s.dist(b) <= tiles(4), "stepped to {s:?}");
        // a step longer than the gap lands exactly on the target
        assert_eq!(a.step_toward(b, tiles(99)), b);
    }

    #[test]
    fn in_range_edge_accounts_for_target_radius() {
        let a = Vec2::new(0, 0);
        let b = Vec2::new(tiles(2), 0);
        // 1.6-tile range against a 0.5-tile-radius target reaches 2.1 tiles.
        assert!(in_range_edge(a, b, milli(1600), milli(500)));
        // Centre-to-centre it would NOT reach: ignoring the target radius is the
        // classic off-by-a-radius in this test.
        assert!(!in_range_edge(a, b, milli(1600), 0));
    }
}
