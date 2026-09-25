//! Target selection, implementing the datamined globals rather than folklore.
//!
//! RULES (calibration.json targeting.*), in the order they are applied:
//!   1. LOGIC_PRESERVE_TARGET_IF_HIT_STARTED: while a windup is running the
//!      target is locked. The lock breaks only if the target gets
//!      LOGIC_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE beyond attack range (or dies);
//!      then the windup is cancelled and the unit rescans.
//!   2. LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET: an unlocked target is kept while it
//!      is within attack range + this extension. Without it a unit whose target
//!      sits exactly on the range boundary flips targets every tick.
//!   3. Otherwise rescan: the nearest valid enemy within sight.
//!
//! Ranges are EDGE to EDGE when ADD_CHARACTER_RANGE_TO_RADIUS is true -- and edge
//! to edge on BOTH sides (calibration targeting.ATTACK_RANGE_RULE =
//! range_plus_both_radii): a target is in range at centre distance <= Range + the
//! attacker's own CollisionRadius + the target's, and the sight scan sums
//! SightRange the same way. Measured on the live 16.402 corpus: a Prince (Range
//! 1600, R 600) stops 3135 native from a princess tower's centre (R 1000), the
//! first step inside 3200; a Dark Prince (Range 1200) at 2776 inside 2800; the
//! tower (Range 7500) acquires the Prince the tick after it steps inside 9100.
//! The old arm (the target's radius only) is runnable as range_plus_target_radius.
//!
//! EXTRA_SIGHT_RANGE_TO_CROWN_TOWERS -- THE READING CHOSEN
//!     The name is ambiguous. Default reading: a UNIT's sight is extended by it
//!     when the candidate is a crown tower ("sight range to crown towers",
//!     parallel to EXTRA_SIGHT_RANGE_TO_BUILDING, which can only mean a unit's
//!     sight to a building target). The other reading (towers see units farther)
//!     is implemented as `TowerSightReading::TowersSeeUnitsFarther`. Neither is
//!     verified.
//!
//! DETERMINISM AND SYMMETRY
//!     Candidates come out of the spatial hash sorted by slot index, but the
//!     choice never depends on that order: the winner is the minimum of a total
//!     key (edge distance, candidate x, candidate y in the ATTACKER's frame,
//!     candidate team_seq). The frame is the 180-degree rotation for Red, so
//!     "lower x" is the attacker's own-left. For a Blue attacker and its rotated
//!     Red twin, every component of every key is identical, so the twins pick
//!     rotated targets.
//!     A raw-EntityId tie-break would instead favour whichever team's deploy was
//!     processed first, which is a seat bias wearing a tie-break's clothes.
#![allow(unexpected_cfgs)]

use crate::arena::{Arena, Lane};
use crate::card::CardDb;
use crate::entity::{EntityKind, Entities, HideState, SpatialHash};
use crate::fixed::{in_range_edge, isqrt, Vec2};
use crate::state::{AttackRangeRule, Calib, CentreLaneFrame, RiseTrigger};
use crate::{EntityId, Team};

/// Which side EXTRA_SIGHT_RANGE_TO_CROWN_TOWERS extends. See module doc.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum TowerSightReading {
    UnitsSeeTowersFarther,
    TowersSeeUnitsFarther,
}

/// Crown tower handles per team: [king, left princess, right princess].
pub type TowerTable = [[Option<EntityId>; 3]; 2];

pub struct TargetCtx<'a> {
    pub ents: &'a Entities,
    pub hash: &'a SpatialHash,
    pub cards: &'a CardDb,
    pub arena: &'a Arena,
    pub calib: &'a Calib,
    pub reading: TowerSightReading,
    pub towers: &'a TowerTable,
    pub king_active: [bool; 2],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct TargetDecision {
    pub target: Option<EntityId>,
    /// A running windup must be abandoned (lock broken).
    pub cancel_attack: bool,
    /// This was the resume rescan after a stun (status.STUN_RETARGET_ON_RESUME); the
    /// caller clears the entity's retarget_on_resume flag.
    pub resumed: bool,
}

/// Is `target` within `range` of `from`? Edge-to-edge when the shipped global
/// says so; centre-to-centre otherwise. `own_radius` is the ATTACKER's
/// CollisionRadius, added to the range under targeting.ATTACK_RANGE_RULE =
/// range_plus_both_radii (module doc) and ignored under range_plus_target_radius.
#[inline]
pub fn in_attack_range(calib: &Calib, from: Vec2, range: i32, own_radius: i32, target: Vec2, target_radius: i32) -> bool {
    #[cfg(clash_plant = "centre_range")]
    {
        // PLANT: centre-to-centre range, ignoring both radii.
        let _ = (calib, target_radius, own_radius);
        return in_range_edge(from, target, range, 0);
    }
    #[cfg(clash_plant = "reach_without_own_radius")]
    let rule = AttackRangeRule::RangePlusTargetRadius; // PLANT (regression): the earlier reach.
    #[cfg(not(clash_plant = "reach_without_own_radius"))]
    let rule = calib.attack_range_rule;
    #[allow(unreachable_code)]
    if calib.add_character_range_to_radius {
        let reach = match rule {
            AttackRangeRule::RangePlusBothRadii => range + own_radius,
            AttackRangeRule::RangePlusTargetRadius => range,
        };
        in_range_edge(from, target, reach, target_radius)
    } else {
        in_range_edge(from, target, range, 0)
    }
}

/// Is entity `c` untargetable because of its hide state (Tesla under ground, or
/// rising under hide.TARGETABLE_WHILE_RISING = false)? The one definition; `can_target`
/// applies it, so a unit already locked on a building that goes under drops it on
/// the next Target phase and rescans (target.rs `decide`, the dead-target path).
#[inline]
pub fn hidden_from_targeting(calib: &Calib, e: &Entities, c: usize) -> bool {
    match e.hide[c] {
        HideState::Up => false,
        HideState::Hidden => true,
        HideState::Rising => !calib.hide_targetable_while_rising,
    }
}

/// Can attacker `a` ever target `c` (ignoring distance)?
#[inline]
pub fn can_target(ctx: &TargetCtx, a: usize, c: usize) -> bool {
    let e = ctx.ents;
    if !e.alive[c] || e.team[c] == e.team[a] || e.hp[c] <= 0 {
        return false;
    }
    #[cfg(not(clash_plant = "hidden_targetable"))]
    if hidden_from_targeting(ctx.calib, e, c) {
        return false;
    }
    // formation.STAGGER_WAIT: a member still waiting out its deploy stagger is nobody's target
    // (0 of 972,681 corpus target rows point at one). The one definition, so the scan, a locked
    // target and a hidden building's wake all read it.
    if ctx.calib.formation_stagger_wait == crate::state::StaggerWait::Client16402 && e.stagger_ms[c] > 0 {
        return false;
    }
    let card = ctx.cards.get(e.card[a]);
    #[cfg(not(clash_plant = "giant_hits_troops"))]
    if card.target_only_buildings && !e.kind[c].is_building() {
        return false;
    }
    if e.flying[c] {
        card.attacks_air
    } else {
        card.attacks_ground
    }
}

/// Attacker a's sight radius toward candidate c.
#[inline]
fn sight_toward(ctx: &TargetCtx, a: usize, c: usize) -> i32 {
    let e = ctx.ents;
    let mut s = ctx.cards.get(e.card[a]).sight_range;
    match ctx.reading {
        TowerSightReading::UnitsSeeTowersFarther => {
            if e.kind[c].is_crown_tower() && !e.kind[a].is_crown_tower() {
                s += ctx.calib.extra_sight_range_to_crown_towers;
            }
        }
        TowerSightReading::TowersSeeUnitsFarther => {
            if e.kind[a].is_crown_tower() && !e.kind[c].is_crown_tower() {
                s += ctx.calib.extra_sight_range_to_crown_towers;
            }
        }
    }
    if e.kind[c] == EntityKind::Building {
        s += ctx.calib.extra_sight_range_to_building;
    }
    s
}

/// The mirror-symmetric preference key. Smaller is better.
#[inline]
fn key(ctx: &TargetCtx, a: usize, c: usize) -> (i32, i32, i32, u32) {
    let e = ctx.ents;
    let centre = isqrt(e.pos[a].dist2(e.pos[c])) as i32;
    let edge = if ctx.calib.add_character_range_to_radius { centre - e.radius[c] } else { centre };
    let f = ctx.arena.to_frame(e.team[a], e.pos[c]);
    #[cfg(clash_plant = "id_tiebreak")]
    {
        // PLANT: tie-break on raw slot index -- a deploy-order asymmetry.
        return (edge, 0, 0, c as u32);
    }
    #[allow(unreachable_code)]
    (edge, f.x, f.y, e.team_seq[c])
}

/// Nearest valid enemy in sight, or None.
pub fn scan(ctx: &TargetCtx, a: usize, scratch: &mut Vec<u32>) -> Option<EntityId> {
    let e = ctx.ents;
    let card = ctx.cards.get(e.card[a]);
    let extra = ctx.calib.extra_sight_range_to_crown_towers.max(0) + ctx.calib.extra_sight_range_to_building.max(0);
    let query = card.sight_range + extra + ctx.hash.max_radius();
    ctx.hash.neighbours_within(e, e.pos[a], query, scratch);
    let mut best: Option<((i32, i32, i32, u32), usize)> = None;
    for &c in scratch.iter() {
        let c = c as usize;
        if !can_target(ctx, a, c) {
            continue;
        }
        #[cfg(not(clash_plant = "sight_ignored"))]
        if !in_attack_range(ctx.calib, e.pos[a], sight_toward(ctx, a, c), e.radius[a], e.pos[c], e.radius[c]) {
            continue;
        }
        let k = key(ctx, a, c);
        if best.as_ref().map_or(true, |(bk, _)| k < *bk) {
            best = Some((k, c));
        }
    }
    best.map(|(_, c)| e.id_of(c))
}

/// THE WAKE TRIGGER of a hidden building (calibration hide.RISE_TRIGGER): is any
/// enemy that `a` could target within `a`'s sight of it (enemy_in_sight_range: the
/// same `sight_toward` a scan uses) or within its attack range
/// (enemy_in_attack_range)? Reuses `can_target`, so an air unit does not wake a
/// ground-only building and a hidden enemy Tesla does not wake this one, and the
/// same edge-to-edge rule as every range test. A boolean over the neighbour set:
/// order-free, frame-free.
pub fn enemy_in_wake_range(ctx: &TargetCtx, a: usize, scratch: &mut Vec<u32>) -> bool {
    let e = ctx.ents;
    let card = ctx.cards.get(e.card[a]);
    let extra = ctx.calib.extra_sight_range_to_crown_towers.max(0) + ctx.calib.extra_sight_range_to_building.max(0);
    let query = card.sight_range.max(card.range) + extra + ctx.hash.max_radius();
    ctx.hash.neighbours_within(e, e.pos[a], query, scratch);
    scratch.iter().any(|&c| {
        let c = c as usize;
        if !can_target(ctx, a, c) {
            return false;
        }
        let reach = match ctx.calib.hide_rise_trigger {
            RiseTrigger::EnemyInSightRange => sight_toward(ctx, a, c),
            RiseTrigger::EnemyInAttackRange => card.range,
        };
        in_attack_range(ctx.calib, e.pos[a], reach, e.radius[a], e.pos[c], e.radius[c])
    })
}

/// Decide attacker a's target for this tick. Reads only; the caller applies.
pub fn decide(ctx: &TargetCtx, a: usize, scratch: &mut Vec<u32>) -> TargetDecision {
    let e = ctx.ents;
    let cur = e.target[a];
    if e.deploy_ms[a] > 0 {
        return TargetDecision { target: None, cancel_attack: false, resumed: false };
    }
    // Under ground or still coming up: no target at all, and any windup it had is
    // cancelled (it cannot have one; belt and braces for a building that went
    // under mid-swing under hide.HIDE_DELAY_MEANING = time_since_last_shot).
    if e.hide[a] != HideState::Up {
        return TargetDecision { target: None, cancel_attack: true, resumed: false };
    }
    // Stunned or mid-knockback (the slide, or the 16.402 ladder): keep what it
    // had, scan nothing.
    if e.stun_ms[a] > 0 || e.knocked(a) {
        return TargetDecision { target: cur.filter(|t| e.is_alive(*t)), cancel_attack: false, resumed: false };
    }
    if e.kind[a] == EntityKind::KingTower && !ctx.king_active[e.team[a] as usize] {
        return TargetDecision { target: None, cancel_attack: false, resumed: false };
    }
    // RESUME: the first tick after a stun, a fresh scan that ignores the target lock
    // and keep-target hysteresis (Supercell 2017-03-13: stuns 'pause the target's
    // attack, causing them to retarget when they resume').
    #[cfg(not(clash_plant = "no_retarget_after_stun"))]
    if e.retarget_on_resume[a] {
        return TargetDecision { target: scan(ctx, a, scratch), cancel_attack: false, resumed: true };
    }
    let card = ctx.cards.get(e.card[a]);
    let mut cancel = false;
    if let Some(t) = cur {
        if e.is_alive(t) && can_target(ctx, a, t.index as usize) {
            let ti = t.index as usize;
            #[cfg(not(clash_plant = "no_target_lock"))]
            let locked = e.target_locked[a];
            #[cfg(clash_plant = "no_target_lock")]
            let locked = false; // PLANT: the windup lock never holds.
            if locked && ctx.calib.preserve_target_if_hit_started {
                let hold = card.range + ctx.calib.cancel_hit_from_long_distance_range;
                if in_attack_range(ctx.calib, e.pos[a], hold, e.radius[a], e.pos[ti], e.radius[ti]) {
                    return TargetDecision { target: Some(t), cancel_attack: false, resumed: false };
                }
                cancel = true;
            } else {
                let keep = card.range + ctx.calib.range_extension_to_keep_target;
                if in_attack_range(ctx.calib, e.pos[a], keep, e.radius[a], e.pos[ti], e.radius[ti]) {
                    return TargetDecision { target: Some(t), cancel_attack: false, resumed: false };
                }
            }
        } else if e.target_locked[a] {
            cancel = true;
        }
    }
    TargetDecision { target: scan(ctx, a, scratch), cancel_attack: cancel, resumed: false }
}

/// Where a unit with no target walks: the enemy crown tower chosen by x
/// (LOGIC_XPOS_BASED_TOWER_TARGETING), falling back to the king once that lane's
/// princess tower is down. With the global off, the nearest enemy crown tower.
pub fn default_tower(ctx: &TargetCtx, a: usize) -> Option<EntityId> {
    let e = ctx.ents;
    let enemy = e.team[a].other() as usize;
    let live = |t: Option<EntityId>| t.filter(|id| e.is_alive(*id));
    #[cfg(not(clash_plant = "default_tower_nearest"))]
    let by_x = ctx.calib.xpos_based_tower_targeting;
    #[cfg(clash_plant = "default_tower_nearest")]
    let by_x = false; // PLANT: ignore LOGIC_XPOS_BASED_TOWER_TARGETING.
    if by_x {
        // THE LANE IS DECIDED IN THE ATTACKER'S OWN FRAME, with the centre line going
        // own-left. targeting.CENTRE_LANE_FRAME, and the ledger entry carries the history.
        //
        // WHAT THE NATIVE KERNEL ACTUALLY SETTLED, 2026-09-23, 12 scenarios over 2 cards x 2
        // seats x deploy x in {8999, 9000, 9001}: from engine x = 8500 both seats walk to the
        // engine-LEFT princess and from x = 9500 both walk to the engine-RIGHT one. **THE
        // OWN-FRAME RULE REPRODUCES ALL TWELVE ROWS.** So does an engine-frame rule. The
        // experiment does not discriminate them, and for a while this file said it did.
        //
        // THE TWO RULES DIFFER AT EXACTLY ONE POINT IN THE ARENA: Blue standing exactly on
        // x = W/2. Checked over every integer x, both seats. Nowhere else, for either seat.
        // And x = W/2 is the one value a deploy cannot produce, because a tap snaps to its
        // tile centre: x=9000 and x=9001 both land a unit at 9500. No scenario in that
        // experiment ever stood on the centre line, so nothing measured the point where the
        // rules disagree.
        //
        // WHY THIS ARM AND NOT THE OTHER, given the kernel is silent. The own-frame rule is
        // seat-symmetric at every x; the engine-frame rule breaks the 180-degree rotation at
        // x = W/2 exactly. That asymmetry is not free: both seats share one policy head and
        // the observation is built in the acting side's own frame precisely so that Blue and
        // Red are the same problem, so an absolute-direction tie makes the same board a
        // different game depending on colour. RoyaleGym's rotation gate measured the break on
        // build 1b8fc1eace21feb7 (one seed of three -- a tie is rare, which is its signature).
        // So: no evidence for the engine frame, a measured cost against it, and a structural
        // prior for this arm.
        //
        // WHAT LOOKED LIKE A DEFECT HERE WAS A DEPLOY POSITION. The replay harness spawned
        // the oracle's fixtures at their raw tap (x = 9000, a tile CORNER) where the kernel
        // spawns at the tile centre (9500). At 9500 this rule already returns engine-right and
        // already agrees. The engine only walked the wrong way because it was standing where
        // the game never puts a unit. Confirmed by the shape of the disagreement: across all
        // 21 fixtures the ONLY lane divergence was side 0 at x=9000, while x=8999, x=9001 and
        // every side-1 row agreed -- which is what "the rules differ only at 9000" predicts.
        // Changing this rule to compensate would move the engine to agree with a fixture.
        let team = e.team[a];
        let frame = ctx.calib.centre_lane_frame;
        // ONE function for the unit and for the candidate tower. Reading them in different
        // frames compares an own-frame lane with an engine-frame one, and then the rule is
        // neither of its two candidates.
        let lane_of = |p: Vec2| match frame {
            CentreLaneFrame::OwnFrameTieLeft => ctx.arena.lane_by_x(ctx.arena.to_frame(team, p).x),
            // KEPT RUNNABLE, NOT SHIPPED: flip the ledger to this and the seat-rotation gates
            // go red at x = W/2. Whoever measures what the client does with a unit standing on
            // the exact centre line -- which needs a unit that WALKS onto it, since no tap can
            // -- settles it by changing one value.
            CentreLaneFrame::EngineFrameTieRight => {
                if p.x * 2 < ctx.arena.width {
                    Lane::Left
                } else {
                    Lane::Right
                }
            }
        };
        #[cfg(not(clash_plant = "reflection_centre_lane"))]
        let own = lane_of(e.pos[a]);
        // PLANT (regression): the centre line goes ENGINE-left for Red too, which in Red's
        // own frame is its right. Off the line nothing differs.
        #[cfg(clash_plant = "reflection_centre_lane")]
        let own = if team == Team::Red && e.pos[a].x * 2 == ctx.arena.width {
            Lane::Right
        } else {
            lane_of(e.pos[a])
        };
        let slot = [Lane::Left, Lane::Right]
            .into_iter()
            .find(|l| lane_of(ctx.arena.princess_tower_pos(team.other(), *l)) == own)
            .map_or(1, |l| 1 + l as usize);
        return live(ctx.towers[enemy][slot]).or_else(|| live(ctx.towers[enemy][0]));
    }

    let mut best: Option<((i32, i32, i32, u32), EntityId)> = None;
    for t in ctx.towers[enemy].iter().filter_map(|t| live(*t)) {
        let k = key(ctx, a, t.index as usize);
        if best.as_ref().map_or(true, |(bk, _)| k < *bk) {
            best = Some((k, t));
        }
    }
    best.map(|(_, t)| t)
}

/// Team whose crown tower this is (for callers holding only a TowerTable).
pub fn tower_owner(towers: &TowerTable, id: EntityId) -> Option<Team> {
    for (ti, row) in towers.iter().enumerate() {
        if row.contains(&Some(id)) {
            return Some(if ti == 0 { Team::Blue } else { Team::Red });
        }
    }
    None
}
