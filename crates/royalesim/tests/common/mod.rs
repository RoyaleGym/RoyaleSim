//! Shared helpers for the royalesim integration tests.
//!
//! THE DATA GATE: every battle here loads the real
//! data/derived/cards.json through the one card loader (`CardDb::load_repo`) and
//! panics if it is absent or came from the fallback set. A battle test on the
//! fallback cards would be simulating a different game and passing anyway.
//!
//! NOTHING HERE COPIES A CARD STAT OR A CALIBRATION VALUE. Where a test needs a
//! number (a range, a radius, a sight) it reads it from the loaded CardDb or
//! `Calib::shipped()`.
#![allow(dead_code)]

use royalesim::arena::Arena;
use royalesim::card::{CardDb, CardSource};
use royalesim::entity::EntityKind;
use royalesim::fixed::Vec2;
use royalesim::state::{footprint_of, BattleConfig, BattleState, EntityView};
use royalesim::{EntityId, Phase, Team, LEGACY_TICK_PHASES, TICK_PHASES};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// data/derived/cards.json, loaded once per test binary. Panics (never falls
/// back) if the derived data is missing: regenerate it with tools/extract_cards.py.
pub fn cards() -> CardDb {
    static CELL: OnceLock<CardDb> = OnceLock::new();
    CELL.get_or_init(|| {
        let db = CardDb::load_repo().unwrap_or_else(|e| {
            panic!("data/derived/cards.json unavailable ({e}); run tools/extract_cards.py -- refusing to test on fallback cards")
        });
        assert_eq!(db.source, CardSource::DerivedJson);
        assert!(!db.towers_from_fallback, "cards.json has no towers; the fallback towers are not the game's");
        assert!(db.cards.len() >= 20, "cards.json parsed to only {} simulable cards", db.cards.len());
        db
    })
    .clone()
}

pub fn config() -> BattleConfig {
    BattleConfig::with_cards(cards())
}

/// The shipped config with the FRAME-PLANNED pathfinder, the FIXED-DISTANCE
/// knockback and the OWN-FRAME formation clamp selected. The shipped search
/// (path16402.rs, pathfinding.PATH_SEARCH = client16402) reproduces the client's recorded
/// routes and is NOT seat-symmetric -- its scan and
/// neighbour orders are in absolute arena coordinates, so rotated twins can publish
/// different equal-cost routes. The shipped knockback (move16402.rs, the measured
/// ladder) is frame-free arithmetic except at two absolute-frame points the game
/// has: the zero-vector direction (knockback.ZERO_VECTOR_DIRECTION =
/// client16402_x_by_id_parity, +-x by id parity) and the water ejection's first-minimum
/// tie (rows from the top). Every seat-symmetry gate (tests/mirror.rs,
/// tests/setup_spawn_order.rs) measures the OTHER systems under this config; the
/// measured route asymmetry is pinned by mirror.rs
/// `the_shipped_search_is_absolute_grid_not_seat_symmetric`, the ladder's symmetry
/// off those two points by tests/knockback16402.rs.
pub fn symmetric_config() -> BattleConfig {
    let mut c = config();
    c.calib.path_search = royalesim::state::PathSearch::TraceFittedAstar;
    c.calib.knock_law = royalesim::state::KnockLaw::FixedDistance;
    c.calib.knock_stacking = royalesim::state::KnockStacking::VectorSum;
    c.calib.knock_zero_vector = royalesim::state::KnockZeroVector::CasterForward;
    // The summon formation's ground clamp (formation.GROUND_Y_CLAMP): the shipped
    // per-side formula is the measured one and is not the rotation of itself at the
    // back edge; the own-frame arm is (tests/formations.rs pins both).
    c.calib.formation_ground_y_clamp = royalesim::state::GroundYClamp::DeployColumnRangeOwnFrame;
    // The summon formation's deploy point (formation.GROUND_DEPLOY_POINT): the
    // measured one-unit offsets are a SEAT asymmetry in y and an ABSOLUTE-frame one
    // in x, on a ground summon only, so a rotation gate would be measuring them
    // rather than the system under test. `none` lays the ring on the tap itself,
    // which is what a flying summon measures on both seats.
    c.calib.formation_ground_deploy_point = royalesim::state::GroundDeployPoint::None;
    c
}

/// A position in hundredths of a tile.
pub fn t(x100: i32, y100: i32) -> Vec2 {
    Vec2::from_tiles_100(x100, y100)
}

/// The SEAT mirror of a Blue position for Red: the 180-degree rotation
/// (W - x, H - y). NOT the y-reflection (x, H - y), and not (W - x, y).
pub fn mirror(s: &BattleState, p: Vec2) -> Vec2 {
    let a = s.arena();
    Vec2::new(a.width - p.x, a.height - p.y)
}

/// (x, H - y): the point on the SAME engine lane on the other side of the river.
/// A placement helper for head-on scenarios (two units meeting on one bridge),
/// NOT a symmetry: under the seat rotation a unit's twin is on the other bridge.
pub fn same_lane_opposite(s: &BattleState, p: Vec2) -> Vec2 {
    Vec2::new(p.x, s.arena().height - p.y)
}

/// How far a knockback ladder (move16402.rs `ladder_speed`, the 25-per-tick countdown,
/// the step `min(speed, dist, 250)`, the final back-step) carries a unit whose target
/// is `l` NATIVE units away, in native units -- derived from the law, never pasted:
/// `25n(n-1)/2 - 25` when the ladder covers `l`, less when a step is cut by the 250 cap.
pub fn ladder_travel(l: i32) -> i32 {
    let mut rem = royalesim::move16402::ladder_speed(l);
    let (mut dist, mut total) = (l, 0);
    loop {
        rem -= royalesim::move16402::PUSHBACK_DECEL;
        let step = rem.min(dist.max(1)).min(250);
        total += step;
        dist -= step;
        if rem < 0 {
            return total;
        }
    }
}

/// The ticks a ladder for `l` native units runs: one per speed value down to 0, plus
/// the back-step tick.
pub fn ladder_ticks(l: i32) -> i32 {
    royalesim::move16402::ladder_speed(l) / royalesim::move16402::PUSHBACK_DECEL + 1
}

/// The distance a landed push of `push` SUBTILES finally carries a unit, subtiles,
/// under `calib`'s knockback.DISPLACEMENT_LAW.
pub fn knock_carry(calib: &royalesim::state::Calib, push: i32) -> i32 {
    use royalesim::fixed::SUBTILE_PER_MILLITILE as K;
    match calib.knock_law {
        royalesim::state::KnockLaw::FixedDistance => push,
        royalesim::state::KnockLaw::Client16402 => ladder_travel(push / K) * K,
    }
}

pub fn card_stat<'a>(s: &'a BattleState, name: &str) -> &'a royalesim::card::CardDef {
    let db = s.cards();
    db.get(db.index(name).unwrap_or_else(|| panic!("card {name} not simulable")))
}

/// Tick until `pred` holds or `max` ticks pass. Returns ticks run.
pub fn run_until(s: &mut BattleState, max: u32, mut pred: impl FnMut(&BattleState) -> bool) -> u32 {
    for k in 0..max {
        if pred(s) {
            return k;
        }
        s.tick();
    }
    max
}

pub fn find_live<'a>(s: &'a BattleState, team: Team, card: &str) -> Vec<EntityView<'a>> {
    s.entities().filter(|e| e.team == team && e.card == card).collect()
}

// ---------------------------------------------------------------------------
// invariants

/// Overlap tolerances, stated so the gate can be argued with.
///
/// WHY PERSISTENCE AND NOT A CEILING: an enemy may legally be deployed on top of
/// a troop (only buildings block a deploy), which creates ANY overlap in the
/// Spawn phase, and SEPARATION_ITERATIONS is one pass (calibration, guess). So a
/// single-tick ceiling is not an invariant of this engine. What is: separation
/// CONVERGES -- a deep overlap does not survive long. Measured on the
/// scripted battle (seed 0xC1A5): max consecutive ticks a pair spent
/// above 25% / 50% / 100% of the smaller radius = 23 / 12 / 1, with a single
/// 180% tick from a Giant deployed onto a Hog-Valkyrie column. The limits below
/// are those measurements with margin, not a fit to the answer.
#[derive(Clone, Copy, Debug)]
pub struct Tolerance {
    /// (percent of the smaller radius, max consecutive ticks a pair may exceed it)
    pub limits: [(i64, u32); 2],
}

pub const DEFAULT_TOLERANCE: Tolerance = Tolerance { limits: [(50, 20), (100, 3)] };

/// THE LIVE GAME'S CROWDS OVERLAP MORE THAN A NAIVE SEPARATION MODEL ALLOWS.
/// Measured over every 16.402 capture in the trace corpus (troop pairs, deploying
/// units excluded): overlaps above 50 % of the smaller radius run up to 207 consecutive
/// ticks, above 100 % up to 42, above 150 % up to 24 (histogram: 811 pair-ticks in
/// 50-75 %, 275 in 100-125 %, 178 at 200 % or more). Under the shipped contact law
/// (move16402.rs: the mean push is capped at 150 native units per tick and buildings
/// carry no Mass) the engine reproduces that, so its invariant is a looser sanity
/// bound: nothing may sit at 100 % for longer than 60 ticks or at 150 % for 40.
pub const CLIENT16402_TOLERANCE: Tolerance = Tolerance { limits: [(100, 60), (150, 40)] };

/// Stateful every-tick invariant checker.
///
/// 1. No ground troop's centre touches a water cell (or leaves the arena).
/// 2. No ground troop's centre is strictly inside a building footprint.
/// 3. No same-layer troop pair overlaps beyond a tolerance for longer than its
///    tick limit (see `Tolerance`).
/// 4. The phase trace of the tick just run equals TICK_PHASES exactly (if the
///    caller enabled tracing before the tick).
pub struct Invariants {
    pub tol: Tolerance,
    runs: BTreeMap<(u32, u32, u32, u32, usize), u32>,
    pub worst_pct: i64,
    pub worst_run: [u32; 2],
    pub ticks_checked: u32,
    pub ground_troop_ticks: u64,
}

impl Invariants {
    pub fn new(tol: Tolerance) -> Self {
        Invariants { tol, runs: BTreeMap::new(), worst_pct: 0, worst_run: [0, 0], ticks_checked: 0, ground_troop_ticks: 0 }
    }

    pub fn check(&mut self, s: &BattleState) -> Result<(), String> {
        self.ticks_checked += 1;
        let arena: &Arena = s.arena();
        let ents: Vec<EntityView> = s.entities().collect();
        let buildings: Vec<(EntityId, royalesim::arena::Shape)> = ents
            .iter()
            .filter(|e| e.kind.is_building())
            .map(|e| (e.id, footprint_of(s, e.id).expect("building has a footprint")))
            .collect();
        // UNDER THE SHIPPED CONTACT LAW (PATH_SEARCH = client16402) the
        // first two invariants are not the client's: buildings carry no Mass, so the
        // separation impulse pushes a unit out of a footprint by one native unit per
        // tick and melee units stand INSIDE a tower's box while attacking it (their
        // goal cell is within Range + own radius of the centre); and nothing ejects
        // a unit the crowd pushed onto a river cell (the game's water clamp only
        // runs in capture mode). Both stay asserted for the frame-planned arm.
        let client16402 = s.config().calib.path_search == royalesim::state::PathSearch::Client16402;
        if client16402 && self.ticks_checked == 1 {
            self.tol = CLIENT16402_TOLERANCE;
        }
        for e in ents.iter().filter(|e| e.kind == EntityKind::Troop && !e.flying) {
            self.ground_troop_ticks += 1;
            if client16402 {
                let inside = e.pos.x < 0 || e.pos.y < 0 || e.pos.x > arena.width || e.pos.y > arena.height;
                if inside {
                    return Err(format!("tick {}: ground troop {} {:?} at {:?} is out of bounds", s.tick_count(), e.card, e.id, e.pos));
                }
                continue;
            }
            if !arena.is_passable_ground(e.pos) {
                return Err(format!("tick {}: ground troop {} {:?} at {:?} is on water / out of bounds", s.tick_count(), e.card, e.id, e.pos));
            }
            for (bid, shape) in &buildings {
                if shape.penetrates(e.pos, 0) {
                    return Err(format!(
                        "tick {}: ground troop {} {:?} centre {:?} inside footprint of building {:?} {:?}",
                        s.tick_count(),
                        e.card,
                        e.id,
                        e.pos,
                        bid,
                        shape
                    ));
                }
            }
        }
        let troops: Vec<&EntityView> = ents.iter().filter(|e| e.kind == EntityKind::Troop).collect();
        let mut now = BTreeMap::new();
        for (a_i, a) in troops.iter().enumerate() {
            for b in troops.iter().skip(a_i + 1) {
                if a.flying != b.flying {
                    continue;
                }
                let need = (a.radius + b.radius) as i64;
                let d2 = a.pos.dist2(b.pos);
                if d2 >= need * need {
                    continue;
                }
                let overlap = need - royalesim::fixed::isqrt(d2);
                let pct = overlap * 100 / (a.radius.min(b.radius).max(1) as i64);
                self.worst_pct = self.worst_pct.max(pct);
                for (k, (limit_pct, limit_ticks)) in self.tol.limits.iter().enumerate() {
                    if pct <= *limit_pct {
                        continue;
                    }
                    let key = (a.id.index, a.id.generation, b.id.index, b.id.generation, k);
                    let run = self.runs.get(&key).copied().unwrap_or(0) + 1;
                    now.insert(key, run);
                    self.worst_run[k] = self.worst_run[k].max(run);
                    if run > *limit_ticks {
                        return Err(format!(
                            "tick {}: {} {:?} at {:?} and {} {:?} at {:?} overlap {}% of the smaller radius for {} consecutive ticks (limit {}% for {} ticks)",
                            s.tick_count(),
                            a.card,
                            a.id,
                            a.pos,
                            b.card,
                            b.id,
                            b.pos,
                            pct,
                            run,
                            limit_pct,
                            limit_ticks
                        ));
                    }
                }
            }
        }
        self.runs = now;
        if let Some(trace) = s.phase_trace() {
            let want = expected_phases(&s.config().calib);
            if trace != want.as_slice() {
                return Err(format!("tick {}: phase trace {:?} != the match.TICK_ORDER list {:?}", s.tick_count(), trace, want));
            }
        }
        Ok(())
    }
}

/// The phase list a battle under `calib` must run: lib.rs `TICK_PHASES` (the
/// measured order, match.TICK_ORDER = client16402) or `LEGACY_TICK_PHASES`.
/// The regression plant `phase_order` runs the legacy list under the shipped
/// key, which is what turns every phase-traced battle red.
pub fn expected_phases(calib: &royalesim::state::Calib) -> Vec<Phase> {
    match calib.tick_order {
        royalesim::state::TickOrder::Client16402 => TICK_PHASES.to_vec(),
        royalesim::state::TickOrder::LegacyMoveBeforeAttack => LEGACY_TICK_PHASES.to_vec(),
    }
}

/// How much of a unit's deploy time is gone by the end of the tick it
/// materialises in (calibration match.TICK_ORDER): under the measured order the
/// countdown runs after the move pass of that very tick, so
/// the first frame already shows DeployTime - TICK_MS; under the legacy order the
/// countdown ran in Upkeep, before the Spawn phase, and the first frame shows the
/// full DeployTime. Either way the first step is on spawn + DeployTime / TICK_MS.
pub fn spawn_tick_countdown(calib: &royalesim::state::Calib) -> i32 {
    match calib.tick_order {
        royalesim::state::TickOrder::Client16402 => calib.tick_ms,
        royalesim::state::TickOrder::LegacyMoveBeforeAttack => 0,
    }
}

/// The most hitpoints an entity can lose in ONE tick to its own LifeTime drain
/// (calibration lifetime.HP_DECAY = linear_drain; 0 for anything without a LifeTime,
/// and 0 under the expiry_hit arm). A drop BIGGER than this is damage; a drop at or
/// below it may be a building bleeding its own life away, which most tests must not
/// read as a hit.
pub fn drain_step(s: &BattleState, id: royalesim::EntityId) -> i32 {
    (s.lifetime_drain(id) + 99) / 100
}

// ---------------------------------------------------------------------------
// mirror comparison

/// One entity reduced to what must be identical to its mirror twin, written in
/// the entity's OWN TEAM FRAME (Red rotated 180 degrees), with no slot index, no
/// generation and no team_seq. Two states are mirror images iff the multiset of
/// Blue tuples equals the multiset of Red tuples.
///
/// Matching by multiset rather than by (team, team_seq) is deliberate: it keeps
/// the comparison valid when the two teams spawn their units in different
/// orders, which is exactly what makes slot indices differ between twins and
/// what the id-tiebreak plant needs in order to bite.
pub type Canon = (
    (String, u8, (i32, i32), i32, i32, u8, i32, i32, bool, (i32, i32), Vec<(i32, i32)>, Option<(String, (i32, i32))>),
    // Spell state on the entity: stun timer, resume-retarget flag, and the
    // knockback slide (timer, and the remaining displacement -- a WORLD vector, so
    // both components flip sign under the rotation, like a projectile's carry); the
    // knockback ladder: active, speed, and the target POINT in the team's frame
    // (native units); the river leap's state-5 flag (a jumper on one seat only
    // would otherwise pass a rotation gate).
    (i32, bool, i32, (i32, i32), bool, i32, (i32, i32), bool),
    // Hide state (Tesla): the state code and its timer, both frame-free
    // scalars.
    (u8, i32),
    // Spawner state: ms to the next emission and the units left in the
    // current wave, frame-free scalars. The owner id (`spawned_by`) differs between
    // twins and is not compared.
    (i32, i32),
    // Charge state (Prince): the charged flag and the run-up progress,
    // frame-free scalars, plus the effective step. Without them every mirror test
    // would stay green over a seat-asymmetric charge; a frame transform applied to
    // either IS the bug.
    (bool, i32, i32),
);

/// A position in `team`'s own frame: identity for Blue, the 180-degree rotation for
/// Red. Written out here, NOT read from `Arena::to_frame`: the instrument must not
/// share code with the thing it measures, or a plant that breaks the engine's frame
/// (`reflection_frame`) breaks the checker the same way and they agree.
fn frame(s: &BattleState, team: Team, p: Vec2) -> (i32, i32) {
    let a = s.arena();
    match team {
        Team::Blue => (p.x, p.y),
        Team::Red => (a.width - p.x, a.height - p.y),
    }
}

fn canon_entity(s: &BattleState, e: &EntityView) -> Canon {
    let target = e.target.and_then(|id| s.entity(id)).map(|t| (t.card.to_string(), frame(s, e.team, t.pos)));
    let knock_rem = match e.team {
        Team::Blue => (e.knock_rem.x, e.knock_rem.y),
        Team::Red => (-e.knock_rem.x, -e.knock_rem.y),
    };
    // the ladder's target is a NATIVE point, meaningful while the ladder runs: the
    // frame transform in native units (and (0, 0) when it does not)
    let push_target = {
        let k = royalesim::fixed::SUBTILE_PER_MILLITILE;
        let a = s.arena();
        match (e.push_active, e.team) {
            (false, _) => (0, 0),
            (true, Team::Blue) => (e.push_target.x, e.push_target.y),
            (true, Team::Red) => (a.width / k - e.push_target.x, a.height / k - e.push_target.y),
        }
    };
    (
        (
            e.card.to_string(),
            e.kind as u8,
            frame(s, e.team, e.pos),
            e.hp,
            e.shield,
            e.attack_phase as u8,
            e.attack_ms,
            e.deploy_ms,
            e.target_locked,
            (e.move_frac.x, e.move_frac.y),
            e.route.iter().map(|p| frame(s, e.team, *p)).collect(),
            target,
        ),
        (e.stun_ms, e.retarget_on_resume, e.knock_ms, knock_rem, e.push_active, e.push_speed, push_target, e.jumping),
        (e.hide_state as u8, e.hide_ms),
        (e.spawn_ms, e.spawn_wave_left),
        (e.charged, e.charge_progress, e.effective_speed),
    )
}

/// A live spell object reduced like `Canon`, in its CASTER's frame: (card, level,
/// damage, motion kind, [position, aim, carry, roll start], [delay, travelled, length],
/// number of entities a rolling spell has already hit). The carry is a world vector
/// (both components flip for Red); the hit SET holds entity ids, which differ between
/// twins, so only its size is comparable -- who was hit shows up in hp and positions.
pub type CanonSpell = (String, i32, i32, u8, [(i32, i32); 4], [i32; 3], usize);

pub fn canon_spells(s: &BattleState, team: Team) -> Vec<CanonSpell> {
    use royalesim::spell::SpellMotion;
    let carry = |f: Vec2| match team {
        Team::Blue => (f.x, f.y),
        Team::Red => (-f.x, -f.y),
    };
    let mut out: Vec<CanonSpell> = s
        .spells()
        .iter()
        .filter(|sp| sp.team == team)
        .map(|sp| {
            let name = s.cards().get(sp.card).name.clone();
            let (kind, pts, nums, hits) = match &sp.motion {
                SpellMotion::Flight { pos, aim, frac, delay_ms } => (0, [frame(s, team, *pos), frame(s, team, *aim), carry(*frac), (0, 0)], [*delay_ms, 0, 0], 0),
                SpellMotion::Airborne { pos, aim, frac, roll_start, roll_len } => {
                    (1, [frame(s, team, *pos), frame(s, team, *aim), carry(*frac), frame(s, team, *roll_start)], [0, 0, *roll_len], 0)
                }
                SpellMotion::Rolling { pos, travelled, len, hit } => (2, [frame(s, team, *pos), (0, 0), (0, 0), (0, 0)], [0, *travelled, *len], hit.len()),
                SpellMotion::Area { pos } => (3, [frame(s, team, *pos), (0, 0), (0, 0), (0, 0)], [0, 0, 0], 0),
                SpellMotion::Pulsing(p) => (4, [frame(s, team, p.pos), (0, 0), (0, 0), (0, 0)], [p.life_ms, p.next_ms, 0], 0),
            };
            (name, sp.level, sp.damage, kind, pts, nums, hits)
        })
        .collect();
    out.sort();
    out
}

/// Per-team sorted canonical tuples, plus the projectile multiset per team.
/// A projectile reduced like `Canon`: (pos, aim, speed, damage, splash, carry).
pub type CanonProjectile = ((i32, i32), (i32, i32), i32, i32, i32, (i32, i32));

pub fn canon_team(s: &BattleState, team: Team) -> (Vec<Canon>, Vec<CanonProjectile>) {
    let mut ents: Vec<Canon> = s.entities().filter(|e| e.team == team).map(|e| canon_entity(s, &e)).collect();
    ents.sort();
    let mut proj: Vec<_> = s
        .projectiles()
        .iter()
        .filter(|p| p.team == team)
        // Projectiles steer in WORLD coordinates (combat::step_projectiles), so
        // their sub-subtile carry is a world-frame vector: BOTH components flip
        // sign under the rotation, which the frame transform of positions does not
        // do (under a y-reflection only y would flip).
        .map(|p| {
            let f = match team {
                Team::Blue => (p.frac.x, p.frac.y),
                Team::Red => (-p.frac.x, -p.frac.y),
            };
            (frame(s, team, p.pos), frame(s, team, p.aim), p.speed, p.damage, p.splash, f)
        })
        .collect();
    proj.sort();
    (ents, proj)
}

/// A team's tower hp as [king, own-left, own-right]. `tower_hp` is in ENGINE lane
/// order for both teams, and Red's own-left tower is its engine-Right one, so the
/// rotation twin of Blue's [k, l, r] is Red's [k, r, l].
pub fn own_frame_tower_hp(s: &BattleState, team: Team) -> [i32; 3] {
    let h = s.tower_hp(team);
    match team {
        Team::Blue => h,
        Team::Red => [h[0], h[2], h[1]],
    }
}

/// Err describing the first difference if `s` is not its own mirror image.
pub fn check_mirror(s: &BattleState) -> Result<(), String> {
    let (be, bp) = canon_team(s, Team::Blue);
    let (re, rp) = canon_team(s, Team::Red);
    if be.len() != re.len() {
        return Err(format!("tick {}: Blue has {} entities, Red has {}", s.tick_count(), be.len(), re.len()));
    }
    for (b, r) in be.iter().zip(re.iter()) {
        if b != r {
            return Err(format!("tick {}: mirror mismatch\n  Blue {:?}\n  Red  {:?}", s.tick_count(), b, r));
        }
    }
    if bp != rp {
        return Err(format!("tick {}: projectile mismatch\n  Blue {:?}\n  Red  {:?}", s.tick_count(), bp, rp));
    }
    let (bs, rs) = (canon_spells(s, Team::Blue), canon_spells(s, Team::Red));
    if bs != rs {
        return Err(format!("tick {}: spell mismatch\n  Blue {:?}\n  Red  {:?}", s.tick_count(), bs, rs));
    }
    if s.tower_hp(Team::Blue) != own_frame_tower_hp(s, Team::Red) {
        return Err(format!("tick {}: tower hp {:?} vs Red own-frame {:?}", s.tick_count(), s.tower_hp(Team::Blue), own_frame_tower_hp(s, Team::Red)));
    }
    let (cb, cr) = (s.crowns()[0], s.crowns()[1]);
    if cb != cr {
        return Err(format!("tick {}: crowns {cb} vs {cr}", s.tick_count()));
    }
    if s.elixir_raw(Team::Blue) != s.elixir_raw(Team::Red) {
        return Err(format!("tick {}: elixir differs", s.tick_count()));
    }
    if s.king_active(Team::Blue) != s.king_active(Team::Red) {
        return Err(format!("tick {}: king activation differs", s.tick_count()));
    }
    Ok(())
}

/// Count of every card on the field per team, for readable failure output.
pub fn census(s: &BattleState) -> BTreeMap<(u8, String), usize> {
    let mut m = BTreeMap::new();
    for e in s.entities() {
        *m.entry((e.team as u8, e.card.to_string())).or_insert(0) += 1;
    }
    m
}

// ---------------------------------------------------------------------------
// the scripted battle (shared by battle.rs and mechanics.rs)

/// Hard cap: regulation + overtime from calibration, plus a margin. Read, not typed.
pub fn tick_cap(cfg: &BattleConfig) -> u32 {
    let c = &cfg.calib;
    (((c.regular_time_s + c.overtime_s) as i64 * 1000 / c.tick_ms as i64) + 20) as u32
}

pub const BLUE_DECK: [&str; 8] = ["Knight", "Archer", "Musketeer", "Giant", "HogRider", "Minions", "BabyDragon", "Valkyrie"];
pub const RED_DECK: [&str; 8] = ["SkeletonArmy", "Cannon", "Tesla", "Prince", "Wizard", "Knight", "Giant", "Minions"];

/// Deterministic scripted policy. Every `period` ticks each team tries the card
/// at a rotating hand slot; if it is affordable it goes to a lane position in the
/// team's own half chosen by card kind and a per-team alternating lane. All
/// positions are written for Blue and rotated (W - x, H - y) for Red, so the
/// alternation is own-left / own-right for both seats.
pub struct Script {
    pub period: u32,
    pub plays: [u32; 2],
    pub rejected: [u32; 2],
    /// Accepted spell casts per team (a subset of `plays`).
    pub spell_casts: [u32; 2],
}

/// Spell decks for the scripted battle WITH spells: every thin-slice spell appears,
/// and both sides carry at least three.
pub const SPELL_BLUE_DECK: [&str; 8] = ["Knight", "Musketeer", "Giant", "Fireball", "Zap", "Log", "HogRider", "Valkyrie"];
pub const SPELL_RED_DECK: [&str; 8] = ["Arrows", "GoblinBarrel", "Minions", "Fireball", "Wizard", "Log", "Prince", "Cannon"];

/// Where a scripted `team` casts `card` right now, best first (engine coordinates):
/// an area spell on the enemy non-tower entity with the most hp (or, with none, the
/// enemy princess tower of the lane); the Log a tile and a half in front of the enemy
/// troop deepest in `team`'s half (or in front of its own princess); a Goblin Barrel on
/// an enemy princess tower. Deterministic (slot order breaks hp ties); not symmetric
/// between seats, and not meant to be -- it is a smoke policy, not a strategy.
pub fn spell_targets(s: &BattleState, team: Team, card: &str, lane_right: bool) -> Vec<Vec2> {
    let a = s.arena();
    let enemy = team.other();
    let lane = |right: bool| if right { royalesim::arena::Lane::Right } else { royalesim::arena::Lane::Left };
    let towers: Vec<EntityId> = s.tower_ids(enemy).iter().flatten().copied().collect();
    let princess = |t: Team, right: bool| a.princess_tower_pos(t, lane(right));
    let fwd = -royalesim::arena::Arena::own_side_dy(team);
    let mut out = Vec::new();
    match card {
        "Log" => {
            let own_half = |p: Vec2| (p.y - a.height / 2) * fwd < 0;
            let deepest = s
                .entities()
                .filter(|e| e.team == enemy && e.kind == EntityKind::Troop && !e.flying && own_half(e.pos))
                .min_by_key(|e| (e.pos.y * fwd, e.id.index))
                .map(|e| e.pos);
            if let Some(p) = deepest {
                out.push(Vec2::new(p.x, p.y - fwd * 3 * royalesim::fixed::SUBTILE / 2));
            }
            let pr = princess(team, lane_right);
            out.push(Vec2::new(pr.x, pr.y + fwd * 3 * royalesim::fixed::SUBTILE));
        }
        "GoblinBarrel" => {
            out.push(princess(enemy, lane_right));
            out.push(princess(enemy, !lane_right));
            out.push(a.king_tower_pos(enemy));
        }
        _ => {
            let best = s
                .entities()
                .filter(|e| e.team == enemy && !towers.contains(&e.id))
                .max_by_key(|e| (e.hp, std::cmp::Reverse(e.id.index)))
                .map(|e| e.pos);
            out.extend(best);
            out.push(princess(enemy, lane_right));
            out.push(a.king_tower_pos(enemy));
        }
    }
    out
}

impl Script {
    pub fn new(period: u32) -> Self {
        Script { period, plays: [0, 0], rejected: [0, 0], spell_casts: [0, 0] }
    }

    pub fn blue_pos(s: &BattleState, card: &str, lane_right: bool) -> Vec2 {
        let c = card_stat(s, card);
        let x = if lane_right { 1450 } else { 350 };
        match c.kind {
            royalesim::card::CardKind::Building => t(900, 1000),
            _ if c.is_flying() => t(x, 1350),
            _ if c.target_only_buildings => t(x, 1400),
            _ if c.range >= royalesim::fixed::milli(4000) => t(x, 900),
            _ => t(x, 1200),
        }
    }

    pub fn step(&mut self, s: &mut BattleState) {
        // A finished battle refuses every deploy with GameOver, which the "refused
        // at every offset" panic below would report as a scenario bug. With
        // time.SPEED_TO_SUBTILES_PER_TICK at 18 the scripted battle ENDS inside the
        // window save_load.rs drives, so the script has to notice.
        if s.is_done() {
            return;
        }
        if s.tick_count() % self.period != 0 {
            return;
        }
        for team in [Team::Blue, Team::Red] {
            let ti = team as usize;
            let hand: Vec<String> = s.hand(team).iter().map(|x| x.to_string()).collect();
            if hand.is_empty() {
                continue;
            }
            let pick = hand[((s.tick_count() / self.period) as usize + ti) % hand.len()].clone();
            let lane_right = self.plays[ti] % 2 == 1;
            // A building may already stand on the preferred spot (buildings live
            // 30-40 s), so the script offers a short fixed list of alternatives.
            // Only when EVERY alternative is refused is it a defect.
            let base = Self::blue_pos(s, &pick, lane_right);
            let mut placed = false;
            let mut last_err = None;
            let is_spell = card_stat(s, &pick).kind == royalesim::card::CardKind::Spell;
            let candidates: Vec<Vec2> = if is_spell {
                spell_targets(s, team, &pick, lane_right)
            } else {
                [0, -250, 250, -500, 500]
                    .iter()
                    .map(|dx100| {
                        let blue = Vec2::new(base.x + dx100 * (royalesim::fixed::SUBTILE / 100), base.y);
                        match team {
                            Team::Blue => blue,
                            Team::Red => mirror(s, blue),
                        }
                    })
                    .collect()
            };
            for pos in candidates {
                match s.deploy(team, &pick, pos) {
                    // The Ok now carries WHERE the card went down, which this walker does
                    // not need: it counts plays.
                    Ok(_) => {
                        self.plays[ti] += 1;
                        if is_spell {
                            self.spell_casts[ti] += 1;
                        }
                        placed = true;
                    }
                    Err(royalesim::state::DeployError::NotEnoughElixir { .. }) => placed = true,
                    Err(e) => last_err = Some((pos, e)),
                }
                if placed {
                    break;
                }
            }
            if !placed {
                self.rejected[ti] += 1;
                panic!("tick {}: scripted deploy {team:?} {pick} refused at every offset: {last_err:?}", s.tick_count());
            }
        }
    }
}

pub fn scripted_config() -> BattleConfig {
    let mut cfg = config();
    cfg.decks = [BLUE_DECK.iter().map(|s| s.to_string()).collect(), RED_DECK.iter().map(|s| s.to_string()).collect()];
    cfg
}

pub struct BattleRun {
    pub hashes: Vec<u64>,
    pub final_state: BattleState,
    pub inv: Invariants,
    pub max_live: usize,
    pub plays: [u32; 2],
    pub spell_casts: [u32; 2],
    /// Ticks on which at least one spell object was live.
    pub spell_ticks: u32,
}

pub fn spell_scripted_config() -> BattleConfig {
    let mut cfg = config();
    cfg.decks = [SPELL_BLUE_DECK.iter().map(|s| s.to_string()).collect(), SPELL_RED_DECK.iter().map(|s| s.to_string()).collect()];
    cfg
}

/// Run the scripted battle to completion (or the cap), checking invariants on
/// every tick when `invariants` is set, and recording state_hash after every tick.
pub fn run_scripted(seed: u64, invariants: bool, perturb: Option<u32>) -> BattleRun {
    run_scripted_with(scripted_config(), seed, invariants, perturb)
}

/// `run_scripted` on a caller-supplied config (decks must be set).
pub fn run_scripted_with(cfg: BattleConfig, seed: u64, invariants: bool, perturb: Option<u32>) -> BattleRun {
    let cap = tick_cap(&cfg);
    let mut s = BattleState::new(seed, cfg);
    let mut script = Script::new(40);
    let mut hashes = vec![s.state_hash()];
    let mut max_live = 0;
    let mut inv = Invariants::new(DEFAULT_TOLERANCE);
    let mut spell_ticks = 0;
    while !s.is_done() && s.tick_count() < cap {
        script.step(&mut s);
        if perturb == Some(s.tick_count()) {
            // PLANT for the determinism gate: nudge one live troop by ONE subtile.
            // A FREE troop: not deploying and touching nobody, so the nudge cannot be
            // undone on the same tick by the separation scan of a unit it overlaps
            // (a Skeleton Army member inside the spiral, the first troop of the
            // scripted battle at tick 300, walks back onto the unperturbed point
            // within one tick).
            let all: Vec<(EntityId, Vec2, i32)> = s.entities().map(|e| (e.id, e.pos, e.radius)).collect();
            let id = s
                .entities()
                .find(|e| {
                    e.kind == royalesim::entity::EntityKind::Troop
                        && !e.deploying
                        && all.iter().all(|(oid, opos, orad)| *oid == e.id || opos.dist2(e.pos) > ((e.radius + orad + 2 * royalesim::fixed::SUBTILE_PER_MILLITILE) as i64).pow(2))
                })
                .map(|e| (e.id, e.pos));
            if let Some((id, pos)) = id {
                let before = s.state_hash();
                // one NATIVE unit (18 subtiles): the game's positions are native
                // integers and the shipped law re-quantises every write, so a
                // sub-native nudge would be erased on the same tick
                assert!(s.debug_set_pos(id, Vec2::new(pos.x + royalesim::fixed::SUBTILE_PER_MILLITILE, pos.y)), "perturbation did not apply");
                assert_ne!(before, s.state_hash(), "a plant must verify its own edit landed");
            } else {
                panic!("perturbation found no troop to perturb at tick {}", s.tick_count());
            }
        }
        if invariants {
            s.set_phase_trace(true);
        }
        s.tick();
        hashes.push(s.state_hash());
        spell_ticks += u32::from(!s.spells().is_empty());
        max_live = max_live.max(s.live_count());
        if invariants {
            if let Err(e) = inv.check(&s) {
                panic!("{e}\ncensus: {:?}", census(&s));
            }
        }
    }
    BattleRun { hashes, final_state: s, inv, max_live, plays: script.plays, spell_casts: script.spell_casts, spell_ticks }
}

