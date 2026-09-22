//! THE SUMMON LAYOUT: where a card's N summons stand around the tap and how long
//! each waits before its deploy countdown, as the live 16.402 corpus shows it.
//! Calibration `formation.LAYOUT = client16402`; state.rs `formation_members` is
//! the caller and owns the frames, the clamps and the queue.
//!
//! THE MODEL, in native units (1 tile = 1000) and integer degrees, every division a
//! truncation toward zero:
//!   radius   SummonRadius, or (blank) the primary character's CollisionRadius
//!            replaced by its SpawnRadius when that is set;
//!   lane     the lane of the NEAREST lane-marked half-tile cell to the tap
//!            (`nearest_lane`), which decides whether the ring is mirrored in x;
//!   offset   `member_offset`: a ring whose count keys the base angle and the
//!            mirrored lane, scaled by 577 / trunc(1000 sin(90 / SummonNumber) /
//!            1024); a LINE along x under SummonWidth; a spiral for eight or more; a
//!            second summon interleaved on the doubled ring;
//!   y clamp  a GROUND member's y into the deployable range of the tap's tile
//!            column (state.rs `ground_y_range`);
//!   bounds   every member into [250, W - 250] x [250, H - 250] native;
//!   stagger  `stagger_ms`: member k waits k x SummonDeployDelay before its
//!            DeployTime countdown; the second summon's j-th member (j + 1) x
//!            SummonDeployDelaySecond.
//! Trigonometry is the 91-entry table `SIN_1024` (round(sin x 1024) per degree,
//! `sin1024`). The caller converts subtiles to native and back (x18, exact both
//! ways for a native point).
//!
//! EVIDENCE (tests/formations.rs over tests/fixtures/formations/measured.json, made
//! by tools/make_formation_fixture.py): every clean multi-unit deploy of the live
//! corpus -- Skeletons, Goblins, Minions, Skeleton Warriors, Spear Goblins,
//! Barbarians, Minion Horde, Bats (SpawnAngleShift 45), Wall Breakers, Skeleton
//! Dragons, Archers, Goblin Gang (3 + 3), Rascals (1 + 2), Royal Hogs (the line)
//! and the 15-strong Skeleton Army's spiral, on both sides and both lanes -- is
//! reproduced member by member: 150 members exact (within 3 native) on every
//! member the contact law had not yet moved. The centred square grid the engine
//! laid before (`formation_grid`, the engine_grid arm) is 500-1500 native off on
//! every swarm.
//!
//! NOT MODELLED: a card whose units come from SummonCharactersList carries an
//! explicit per-member offsets table (SummonCharactersOffsetsX/Y,
//! CharactersOffsetsXMirrored -- the Three Musketeers); card.rs leaves such a card
//! on its `count` alone and calibration formation.LAYOUT records the gap.

use crate::arena::Arena;
use crate::fixed::Vec2;

/// `round(sin(d degrees) x 1024)` for d = 0..=90, the resolution the measured
/// rings have: in tests/fixtures/formations/measured.json the five Bats stand 1404
/// native from their tap (SummonRadius 750; `scale_radius` gives the ring 1405 and
/// the per-axis truncation of each offset lands the members at 1404) and the five
/// Barbarians 1311 (700, ring and members alike). tests/formations.rs re-derives
/// every entry from an integer-only series and pins the table to it.
pub const SIN_1024: [i16; 91] = [
    0, 18, 36, 54, 71, 89, 107, 125, 143, 160, 178, 195, 213, 230, 248, 265, 282, 299, 316, 333, 350, 367, 384, 400,
    416, 433, 449, 465, 481, 496, 512, 527, 543, 558, 573, 587, 602, 616, 630, 644, 658, 672, 685, 698, 711, 724, 737,
    749, 761, 773, 784, 796, 807, 818, 828, 839, 849, 859, 868, 878, 887, 896, 904, 912, 920, 928, 935, 943, 949, 956,
    962, 968, 974, 979, 984, 989, 994, 998, 1002, 1005, 1008, 1011, 1014, 1016, 1018, 1020, 1022, 1023, 1023, 1024,
    1024,
];

/// sin(deg) x 1024 for any integer degree: the angle reduced into [0, 360),
/// folded onto the table's quadrant, negated past 180.
pub fn sin1024(deg: i32) -> i32 {
    let a = deg.rem_euclid(360);
    let (e, neg) = if a >= 180 { (a - 180, true) } else { (a, false) };
    let d = if e < 91 { e } else { 180 - e };
    let v = SIN_1024[d as usize] as i32;
    if neg {
        -v
    } else {
        v
    }
}

/// `trunc(v / 1024)`, toward zero for a negative product too.
#[inline]
fn shr10_trunc(v: i32) -> i32 {
    v / 1024
}

/// Lane ids as the arena's lane bits name them: 1 = left, 2 = right, in the frame
/// the point is given in (state.rs passes the OWNER's frame, so 1 is own-left).
pub const LANE_NONE: u8 = 0;
pub const LANE_LEFT: u8 = 1;
pub const LANE_RIGHT: u8 = 2;

/// The lane bits of the nearest lane-marked half-tile cell to `p`'s cell,
/// Euclidean on the cell grid, the FIRST minimum winning in the scan order columns
/// outer / rows inner; 0 when no cell carries a lane bit. On the shipped map this
/// is simply "left of the centre column" (tests/formations.rs pins it and the
/// rotation symmetry of the scan), which is all the corpus can tell apart.
pub fn nearest_lane(arena: &Arena, p: Vec2) -> u8 {
    let lanes = arena.bit_lane_left | arena.bit_lane_right;
    let (cx, cy) = arena.subtile_to_half(p);
    let mut best: Option<(i64, u8)> = None;
    for i in 0..arena.cols {
        for j in 0..arena.rows {
            let bits = arena.cell_bits(i, j) & lanes;
            if bits == 0 {
                continue;
            }
            let dx = (i - cx) as i64;
            let dy = (j - cy) as i64;
            let d2 = dx * dx + dy * dy;
            if best.map_or(true, |(b, _)| d2 < b) {
                best = Some((d2, bits));
            }
        }
    }
    best.map_or(LANE_NONE, |(_, b)| b)
}

/// The layout's inputs, in the caller's frame (state.rs: the owner's).
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    /// SummonNumber as passed (the primaries), >= 1.
    pub primaries: i32,
    /// SummonCharacterSecondCount (0 without a second summon).
    pub seconds: i32,
    /// The ring's radius input, native (SummonRadius / SpawnRadius / CollisionRadius).
    pub radius: i32,
    /// SummonWidth, native (0 = a ring, else a line).
    pub width: i32,
    /// SpawnAngleShift of the primary character, degrees.
    pub angle_shift: i32,
    /// The tap's lane (`nearest_lane`), 1 / 2 / 0.
    pub lane: u8,
    /// globals.csv LOGIC_LANE_ID_BASED_DEPLOY_SEQUENCE.
    pub lane_mirror: bool,
}

/// THE RING COUNT the layout keys on: SummonNumber alone, or, with a second
/// summon, twice the larger count (the Goblin Gang's 3 + 3 stand on a hexagon,
/// the Rascals' 1 + 2 on a square).
pub fn ring_count(primaries: i32, seconds: i32) -> i32 {
    if seconds > 0 {
        2 * primaries.max(seconds)
    } else {
        primaries + seconds
    }
}

/// Member `k`'s offset from the tap, native, for SIDE 0 (y toward the enemy). The
/// caller computes in the owner's frame and rotates, so both seats get the
/// placement the corpus shows (tests/formations.rs: Red's rings are the
/// rotations of Blue's, under the shipped globals.csv
/// LOGIC_LANE_ID_BASED_DEPLOY_SEQUENCE = TRUE).
pub fn member_offset(l: Layout, k: i32) -> Vec2 {
    let n = l.primaries.max(1);
    let s = l.seconds.max(0);
    let mut ring = ring_count(n, s);
    let mut k = k;
    let mut radius = l.radius;
    let g = l.lane_mirror;
    // Per ring count: base angle, mirrored lane, ring count.
    let (mut base, mirror_lane): (i32, u8) = match ring {
        1 => return Vec2::default(),
        2 => {
            // A pair: unscaled radius, the angle shift added once (Archers stand
            // 500 either side of the tap).
            let mut mirror = l.lane == LANE_LEFT && g;
            return finish(l, n, s, ring, k, radius, l.angle_shift + 90, &mut mirror);
        }
        3 => (180, LANE_LEFT),
        4 => (45, LANE_RIGHT),
        5 => (180, LANE_RIGHT),
        6 => (0, LANE_RIGHT),
        7 => {
            // One at the tap, the other six on a hexagon that no lane mirrors.
            if k == 0 {
                return Vec2::default();
            }
            k -= 1;
            ring = 6;
            let mut mirror = false;
            radius = scale_radius(radius, n, l.width, ring);
            return finish(l, n, s, ring, k, radius, l.angle_shift, &mut mirror);
        }
        _ => (0, LANE_RIGHT), // eight or more keep their count
    };
    let mut mirror = l.lane == mirror_lane && g;
    radius = scale_radius(radius, n, l.width, ring);
    base += l.angle_shift;
    if ring >= 7 {
        // The big swarm's spiral -- member k sits at radius x ((3k) mod 7) / 6
        // (Skeleton Army: 0, 1/2, 1, 1/3, 5/6, 1/6, 2/3, 0, ...).
        radius = radius * ((3 * k) % 7) / 6;
    }
    finish(l, n, s, ring, k, radius, base, &mut mirror)
}

/// With no SummonWidth and three or more on the ring, the radius becomes
/// `radius x 577 / trunc(1000 x sin(90 / SummonNumber) / 1024)` -- so three
/// Skeletons at SummonRadius 700 stand 807 from the tap, six Minions at 600 stand
/// 1341, five Barbarians at 700 stand 1311 (all measured live). `90 / SummonNumber`
/// is an integer degree (four Goblins: sin 22, not 22.5). A SummonNumber above 90
/// would make the divisor zero, so it is floored at 1 here.
fn scale_radius(radius: i32, primaries: i32, width: i32, ring: i32) -> i32 {
    if width != 0 || ring < 3 {
        return radius;
    }
    let s = sin1024(90 / primaries.max(1));
    let denom = shr10_trunc(1000 * s).max(1);
    radius * 577 / denom
}

/// The angles, the trig, the line, the side negation and the lane mirror. `ring`
/// is the ring count after the case-7 adjustment.
#[allow(clippy::too_many_arguments)]
fn finish(l: Layout, n: i32, s: i32, ring: i32, k: i32, radius: i32, base: i32, mirror: &mut bool) -> Vec2 {
    let w = l.width;
    // SummonWidth with exactly one second summon -- (+-W, -+radius), no trig.
    if w != 0 && s == 1 {
        if k == 0 {
            return Vec2::default();
        }
        let x = if *mirror { -w } else { w };
        let y = -radius; // side 0
        return Vec2::new(x, y);
    }
    let mut base = base;
    let (ay, ax, ysign) = if s > 0 {
        // A second summon interleaves on the doubled ring.
        let half = (360 / ring) / 2;
        let (mut a, mut esi) = (half, half);
        if n < s {
            a = (180 / s) / 2 + half;
        } else if n > s {
            esi = (180 / n) / 2 + half;
        }
        let mut edi = if k > 0 { a } else { 90 };
        if n == 1 {
            base = 0;
        } else {
            edi = a;
        }
        if n > 1 && s == 1 {
            base = 0;
            if k < n {
                edi = 90 / n;
            }
        }
        if k < n {
            let ang = k * 360 / ring;
            edi += base;
            (edi + ang, edi + ang + 90, -1)
        } else {
            let ang = (k - n) * 360 / ring;
            esi += base;
            (ang + esi + 180, ang + esi + 270, -1)
        }
    } else {
        // The plain ring.
        let ang = k * 360 / ring;
        (ang + base + 90, ang + base + 180, 1)
    };
    let mut x = shr10_trunc(sin1024(ax) * radius);
    let mut y = sin1024(ay) * radius / (ysign * 1024);
    if w != 0 {
        // The LINE -- W x k / (ring - 1) - W / 2 along x, the odd members `radius`
        // off the even ones in y (the Royal Hogs: four abreast over 3500); side 0
        // negates both and the mirror becomes "left lane", without the global.
        x = w * k / (ring - 1).max(1) - w / 2;
        y = (k % 2) * radius - radius / 2;
        x = -x;
        y = -y;
        *mirror = l.lane == LANE_LEFT;
    } else {
        y = -y; // side 0
    }
    if *mirror {
        x = -x;
    }
    Vec2::new(x, y)
}

/// How long member `k` waits before its DeployTime countdown starts, ms (measured
/// on the corpus' deploy-end ticks: Goblins 200 ms apart, the Rascals' Girls on
/// the second delay). `primary_is_building`: a summoned BUILDING's members all
/// take the flat SummonDeployDelay -- a hypothesis, no such card is in the slice.
pub fn stagger_ms(k: i32, primaries: i32, delay_ms: i32, delay_second_ms: i32, primary_is_building: bool) -> i32 {
    if delay_ms > 0 && primary_is_building {
        return delay_ms;
    }
    if k != 0 && delay_ms > 0 {
        return delay_ms * k;
    }
    if delay_second_ms > 0 && k >= primaries {
        return delay_second_ms * (k - primaries + 1);
    }
    0 // plain deploy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_table_shape() {
        assert_eq!(sin1024(0), 0);
        assert_eq!(sin1024(30), 512);
        assert_eq!(sin1024(90), 1024);
        assert_eq!(sin1024(180), 0);
        assert_eq!(sin1024(270), -1024);
        assert_eq!(sin1024(450), 1024);
        assert_eq!(sin1024(-90), -1024);
        assert_eq!(sin1024(120), sin1024(60));
        assert!(SIN_1024.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn ring_count_doubles_with_a_second_summon() {
        assert_eq!(ring_count(3, 0), 3);
        assert_eq!(ring_count(3, 3), 6);
        assert_eq!(ring_count(1, 2), 4);
        assert_eq!(ring_count(15, 0), 15);
    }

    #[test]
    fn stagger_rules() {
        // primaries: k x delay; k = 0 plain
        assert_eq!(stagger_ms(0, 4, 200, 0, false), 0);
        assert_eq!(stagger_ms(3, 4, 200, 0, false), 600);
        // no delay at all
        assert_eq!(stagger_ms(2, 3, 0, 0, false), 0);
        // the second summon's own delay when the first is blank
        assert_eq!(stagger_ms(0, 1, 0, 100, false), 0);
        assert_eq!(stagger_ms(1, 1, 0, 100, false), 100);
        assert_eq!(stagger_ms(2, 1, 0, 100, false), 200);
        // a second summon under the first delay: k x delay carries on
        assert_eq!(stagger_ms(5, 3, 100, 0, false), 500);
        // a summoned building: flat
        assert_eq!(stagger_ms(0, 2, 100, 0, true), 100);
        assert_eq!(stagger_ms(1, 2, 100, 0, true), 100);
    }
}
