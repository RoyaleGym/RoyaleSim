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
//!     key (the distance targeting.TARGET_RANK_DISTANCE names, candidate x,
//!     candidate y in the ATTACKER's frame, candidate team_seq). The frame is the 180-degree rotation for Red, so
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
use crate::state::{
    AttackRangeRule, Calib, CentreLaneFrame, ChaseDropRange, DeprioritizedTargetBuff, LeapingUnitTargetability, MinimumRange,
    PreserveTargetScope, RiderTargetable, RiseLaw, RiseTrigger, TowerCancelRange,
};
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
    /// The tick being run (`BattleState::tick`), which a candidate's `acquirable_from` is
    /// compared with (targeting.SPAWNED_UNIT_ACQUIRE_DELAY, `can_target`).
    pub tick: u32,
    /// targeting.DOOMED_TARGET_DROP = projectile_attackers: per slot, whether the unit is doomed by the
    /// shots in flight at the tick's start (combat.rs `doomed_by_shots_in_flight`). Empty under keep and
    /// outside the Target phase.
    pub doomed: &'a [bool],
}

/// targeting.DOOMED_TARGET_DROP: damage that lands later than this does not doom its target. The client
/// 15.535.29 global LOGIC_PENDING_DAMAGE_IGNORE_IF_DURATION_LESS, 600 ms; the drops read 600 and the keeps
/// 650 and above.
pub const DOOMED_ETA_LIMIT_MS: i32 = 600;

/// How far past its reach (Range + both radii) a projectile attacker holds its target, NATIVE units (millitiles; `decide`
/// scales it to subtiles), under
/// targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED = "projectile_attackers_only" (`decide`). Measured on client
/// 15.535.29: a Musketeer and Minions let go of a Hog Rider leaving their reach past H in [499.45, 544.52) (33 of
/// 33 let-go frames, at every frame residue), and a princess tower drops a walking Knight past H in (487.2, 500.5]
/// (three sweep scenarios). The brackets meet at 500.
#[cfg(not(clash_plant = "projectile_hold_1500"))]
pub const PROJECTILE_HOLD_BEYOND_REACH: i32 = 500;
#[cfg(clash_plant = "projectile_hold_1500")]
pub const PROJECTILE_HOLD_BEYOND_REACH: i32 = 1500; // PLANT (regression): the old cancel range.

/// targeting.SPAWNED_UNIT_ACQUIRE_DELAY = client_8th_frame: the ticks from a death-spawned
/// troop's first tick F to the first Target phase that may give it to an enemy. Measured on
/// client 15.535.29, in frames: 35 death spawns first targeted on exactly F + 7, their 8th
/// frame, and none on F + 1 to F + 6. Not the DeployDelay column: the Golemite row has none
/// and waits the same 7. Read by state.rs `delay_acquisition` alone.
#[cfg(not(clash_plant = "acquire_delay_one_short"))]
pub const ACQUIRE_DELAY_TICKS: u32 = 7;
#[cfg(clash_plant = "acquire_delay_one_short")]
pub const ACQUIRE_DELAY_TICKS: u32 = 6; // PLANT: the first target lands on F + 6.

/// targeting.CHASE_DROP_RANGE = client_sight_minus_1000: how far short of SightRange + both collision radii a walking
/// troop lets go of a troop it chases, NATIVE units (millitiles; `beyond_chase_limit` scales it to subtiles).
/// Measured on client 15.535.29: 18 of 18 lane drops of a Hog Rider or a Battle Ram by a P.E.K.K.A, a Knight, a
/// Prince and a Mini P.E.K.K.A fit one constant between 990 and 1008.
pub const CHASE_DROP_SHORT_OF_SIGHT: i32 = 1000;

/// targeting.FIRST_TOWER_PICK = client_spawn_lane: how long after its deploy ends a troop's default tower still comes
/// from its spawn lane, ms (state.rs `on_deployed`). Measured on client 15.535.29: 6 of 6 re-picks by x fall 10
/// ticks after the first pick.
pub const FIRST_PICK_LANE_WINDOW_MS: i32 = 500;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct TargetDecision {
    pub target: Option<EntityId>,
    /// A running windup must be abandoned (lock broken).
    pub cancel_attack: bool,
    /// This was the resume rescan after a stun (status.STUN_RETARGET_ON_RESUME); the
    /// caller clears the entity's retarget_on_resume flag.
    pub resumed: bool,
    /// targeting.CHASE_DROP_RANGE = client_sight_minus_1000: the troop this decision let go of past the chase-drop
    /// limit; the caller records it (entity.rs `chase_dropped`). None under the old arm.
    pub chase_dropped: Option<EntityId>,
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
///
/// Under hide.RISE_LAW = client16402_surface_attacking the one Rising tick is the SURFACING
/// tick (`surfaces_attacking`), and no enemy may target the building on it, whatever
/// hide.TARGETABLE_WHILE_RISING says: on client 15.535.29 the Knight locks on the tick after the
/// Tesla surfaces (241 for 240), as on the 16.402 corpus (675 for 674, 1981 for 1980).
#[inline]
pub fn hidden_from_targeting(calib: &Calib, e: &Entities, c: usize) -> bool {
    match e.hide[c] {
        HideState::Up => false,
        HideState::Hidden => true,
        #[cfg(not(clash_plant = "tesla_surface_tick_targetable"))]
        HideState::Rising => surfaces_attacking(calib) || !calib.hide_targetable_while_rising,
        // PLANT (regression): the surfacing tick is targetable under the new arm, as a rise is under the old.
        #[cfg(clash_plant = "tesla_surface_tick_targetable")]
        HideState::Rising => !calib.hide_targetable_while_rising,
    }
}

/// INVISIBLE WHEN IDLE (targeting.INVISIBILITY = client_until_hit; card.rs `CardDef::invisible_when_idle`), measured on
/// client 15.535.29 on the Royal Ghost: invisible from its deploy; its own hit reveals it, and enemies may target it
/// from the tick after (`reveal_from`, set in the attack pass: the enemies' targeting on the hit tick runs before the
/// hit); it hides again INVIS_VISIBLE_AFTER_HIT_EXTRA ticks past its idle time after that (one sample: hit + 46 for the
/// 2000 ms idle time, which attack end + 40 fits equally). Only targeting reads it: area damage lands on an invisible
/// unit.
#[inline]
pub fn invisible(ctx: &TargetCtx, c: usize) -> bool {
    invisible_at(ctx.calib, ctx.cards, ctx.ents, ctx.tick, c)
}

/// `invisible` from its parts, for a reader with no TargetCtx: the export's `status_flags` bit 1 (state.rs `view`),
/// asked with the tick the next targeting runs on, so the bit is the predicate that targeting will act on.
pub fn invisible_at(calib: &Calib, cards: &CardDb, ents: &Entities, tick: u32, c: usize) -> bool {
    if calib.invisibility != crate::state::Invisibility::ClientUntilHit {
        return false;
    }
    let Some(idle_ms) = cards.get(ents.card[c]).invisible_when_idle else { return false };
    let from = ents.reveal_from[c];
    if from == 0 {
        return true;
    }
    #[cfg(not(clash_plant = "rehide_never"))]
    let until = from + (idle_ms / calib.tick_ms.max(1)) as u32 + INVIS_VISIBLE_AFTER_HIT_EXTRA;
    #[cfg(clash_plant = "rehide_never")]
    let until = {
        let _ = idle_ms;
        u32::MAX // PLANT: once revealed, never hidden again.
    };
    tick < from || tick >= until
}

/// targeting.INVISIBILITY: the ticks an invisible unit stays visible after its idle time, counted from the tick after
/// its hit. Measured on client 15.535.29 (one sample): an enemy tower held the Royal Ghost on hit + 1 .. hit + 45 and
/// dropped it on hit + 46, with an idle time of 2000 ms (40 ticks).
pub const INVIS_VISIBLE_AFTER_HIT_EXTRA: u32 = 5;

/// rider.TARGETABLE_WHILE_ATTACHED = untargetable_immune: are attached riders (card.rs `AttachDef`) out of reach of
/// every target scan, hit, area, buff, stun and push? The one reading of the key, passed to combat.rs `resolve`.
#[inline]
pub fn riders_immune(calib: &Calib) -> bool {
    #[cfg(not(clash_plant = "rider_targetable"))]
    let on = calib.rider_targetable == RiderTargetable::UntargetableImmune;
    #[cfg(clash_plant = "rider_targetable")]
    let on = {
        let _ = calib;
        false // PLANT (regression): a rider is an ordinary target and takes every hit.
    };
    on
}

/// Is entity `c` an ATTACHED RIDER that nothing may target or touch (`riders_immune`; entity.rs `attached`)? The
/// one definition: `can_target` (a scan, a kept target, a building's wake), spell.rs `eligible` (every area, strike
/// and knockback of a spell), combat.rs `straight_hits`, the Goblin Hut's wakers and state.rs `apply_effects`
/// read it, and combat.rs `resolve` drops a hit written on one. Measured on client 16.402 on one Ram Rider: no enemy
/// targeted the rider on any of 146 ticks and its hp never moved; the Ram's attackers were locked on the Ram, which
/// sticky targeting explains as well, and no area damage landed on the pair, so the arm is a hypothesis.
#[inline]
pub fn rider_untouchable(calib: &Calib, e: &Entities, c: usize) -> bool {
    riders_immune(calib) && e.attached(c)
}

/// targeting.DEPRIORITIZED_TARGET_BUFF (rescan_on_landing_keep_progress or rank_last_only): does attacker `a`'s card
/// deprioritize a buff that candidate `c` carries (card.rs `CardDef::deprioritize_buff`, the Ram Rider's rider and
/// BolaSnare)? Such a candidate is ranked after every other in `scan_with`. False under not_read and for every
/// attacker that deprioritizes nothing.
#[inline]
fn deprioritized(ctx: &TargetCtx, a: usize, c: usize) -> bool {
    #[cfg(not(clash_plant = "deprioritize_ignored"))]
    let read = ctx.calib.deprioritized_target_buff != DeprioritizedTargetBuff::NotRead;
    #[cfg(clash_plant = "deprioritize_ignored")]
    let read = false; // PLANT (regression): a carrier is ranked by distance like any candidate.
    if !read {
        return false;
    }
    let Some(b) = ctx.cards.get(ctx.ents.card[a]).deprioritize_buff else { return false };
    ctx.ents.buff_slots(c).iter().any(|s| !s.is_empty() && u32::from(s.id) == u32::from(b) + 1)
}

/// hide.RISE_LAW = client16402_surface_attacking: a hidden building that wakes surfaces straight
/// into its attack. Its wake enters Rising with no timer (state.rs `hide_pass`), and on that one
/// tick it takes its target and runs its attack cycle as an Up building does (`hide_acts`), while
/// no enemy may target it (`hidden_from_targeting`); it is Up from the next tick. Under
/// engine_rising_phase a Rising building does nothing for UpTimeMs.
#[inline]
pub fn surfaces_attacking(calib: &Calib) -> bool {
    #[cfg(not(clash_plant = "tesla_rise_kept"))]
    let on = calib.hide_rise_law == RiseLaw::Client16402SurfaceAttacking;
    #[cfg(clash_plant = "tesla_rise_kept")]
    let on = {
        let _ = calib;
        false // PLANT (regression): the new arm keeps the UpTimeMs rise with no target and no attack.
    };
    on
}

/// May hiding entity `i` take a target and attack in its current hide state? Up, or its surfacing
/// tick under hide.RISE_LAW = client16402_surface_attacking (`surfaces_attacking`). Every entity
/// that does not hide is Up for life.
#[inline]
pub fn hide_acts(calib: &Calib, e: &Entities, i: usize) -> bool {
    match e.hide[i] {
        HideState::Up => true,
        HideState::Hidden => false,
        HideState::Rising => surfaces_attacking(calib),
    }
}

/// Can attacker `a` ever target `c` (ignoring distance)? `keeping` is true when `c` is `a`'s current
/// target being kept (`decide`), false when `c` is a candidate of a scan or a wake test.
#[inline]
pub fn can_target(ctx: &TargetCtx, a: usize, c: usize, keeping: bool) -> bool {
    let e = ctx.ents;
    if !e.alive[c] || e.team[c] == e.team[a] || e.hp[c] <= 0 {
        return false;
    }
    #[cfg(not(clash_plant = "hidden_targetable"))]
    if hidden_from_targeting(ctx.calib, e, c) {
        return false;
    }
    // targeting.INVISIBILITY = client_until_hit: an invisible unit (the Royal Ghost) is nobody's target, a kept one
    // included -- going invisible drops a lock (measured on client 15.535.29).
    #[cfg(not(clash_plant = "invisible_targetable"))]
    if invisible(ctx, c) {
        return false;
    }
    // movement.SPAWN_PATHFIND_BODY = untouchable: a unit under ground (a Miner, a Goblin Drill's dig on its way to
    // its tap; entity.rs `underground`) is nobody's target, a kept one included (client 15.535.29: never targeted,
    // even inside a princess tower's footprint).
    #[cfg(not(clash_plant = "tunnel_targetable"))]
    if ctx.calib.spawn_pathfind_body == crate::state::SpawnPathfindBody::Untouchable && e.underground(c) {
        return false;
    }
    // rider.TARGETABLE_WHILE_ATTACHED = untargetable_immune: an attached rider is nobody's target, a kept one included.
    if rider_untouchable(ctx.calib, e, c) {
        return false;
    }
    // formation.STAGGER_WAIT: a member still waiting out its deploy stagger is nobody's target
    // (0 of 972,681 corpus target rows point at one). The one definition, so the scan, a locked
    // target and a hidden building's wake all read it.
    if ctx.calib.formation_stagger_wait == crate::state::StaggerWait::Client16402 && e.stagger_ms[c] > 0 {
        return false;
    }
    // targeting.SPAWNED_UNIT_ACQUIRE_DELAY = client_8th_frame: a troop a death spawn created is
    // nobody's target before its `acquirable_from` tick, its 8th frame (state.rs
    // `delay_acquisition` sets it; the column is 0 on every other unit and under `none`). Here,
    // in the one definition, like the stagger wait above, so the scan, a locked target and a
    // hidden building's wake all read it. Area damage never asks this function, so it still
    // lands (spell.rs `eligible`, combat.rs `splash`). The wake's reading and the landing are
    // both the engine's, not measured; tests/spawn_acquire_delay.rs pins them as OPEN.
    #[cfg(not(clash_plant = "acquire_delay_unread"))]
    if e.acquire_delayed(c, ctx.tick) {
        return false;
    }
    let card = ctx.cards.get(e.card[a]);
    // targeting.DOOMED_TARGET_DROP = projectile_attackers: an attacker whose card fires a projectile
    // neither keeps nor takes a unit the shots already in flight will kill (`ctx.doomed`), unless it has
    // launched a shot at that unit since acquiring it (entity.rs `fired_at`). So it drops the target on
    // the tick after the doom and does not take it back while it lives. Under
    // projectile_attackers_rescan that exemption covers KEEPING only: a scan never takes a doomed unit,
    // the one the attacker has just shot at included (client 15.535.29: 4 of 4 launches beyond reach
    // at a doomed target were followed by a drop, none retaken). Under projectile_attackers_walk_drop
    // `decide` passes `keeping` only while the attacker may still keep a unit it has shot at
    // (`keeps_fired`: in its attack, or the target within its keep reach).
    #[cfg(clash_plant = "doomed_rescan_takes_fired")]
    let keeping = true; // PLANT (regression): a rescan takes back a doomed unit the attacker has shot at.
    #[cfg(not(clash_plant = "doomed_drop_every_attacker"))]
    let applies = card.projectile.is_some();
    #[cfg(clash_plant = "doomed_drop_every_attacker")]
    let applies = true; // PLANT (regression): an attacker with no projectile drops it too.
    #[cfg(not(clash_plant = "doomed_drop_ignores_fired"))]
    let exempt = e.fired_at[a] == Some(e.id_of(c))
        && (keeping || ctx.calib.doomed_target_drop == crate::state::DoomedTargetDrop::ProjectileAttackers);
    #[cfg(clash_plant = "doomed_drop_ignores_fired")]
    let exempt = false; // PLANT (regression): an attacker that has fired drops it too.
    if applies && !exempt && ctx.doomed.get(c).copied().unwrap_or(false) {
        return false;
    }
    #[cfg(not(clash_plant = "giant_hits_troops"))]
    if card.target_only_buildings && !e.kind[c].is_building() {
        return false;
    }
    // TargetOnlyTroops (card.rs `CardDef::target_only_troops`; the Ram Rider's rider): never a building or a crown
    // tower. A column semantic, the mirror of TargetOnlyBuildings above.
    #[cfg(not(clash_plant = "target_only_troops_ignored"))]
    if card.target_only_troops && e.kind[c].is_building() {
        return false;
    }
    // targeting.LEAPING_UNIT_TARGETABILITY = airborne: a troop in its river leap is a target only for an attacker
    // that attacks air. `jumping` is set and cleared by the Path phase, which runs after this one, so this reads the
    // leap state the previous tick left: a ground-only attacker drops the leaper one tick after the hop and may take
    // it again one tick after the landing, as measured on client 15.535.29.
    #[cfg(not(clash_plant = "leap_targetable_by_ground"))]
    let airborne = ctx.calib.leaping_unit_targetability == LeapingUnitTargetability::Airborne && e.jumping[c];
    #[cfg(clash_plant = "leap_targetable_by_ground")]
    let airborne = false; // PLANT (regression): a leaping troop stays a ground target under the new arm too.
    if e.in_air(c) || airborne {
        card.attacks_air
    } else {
        card.attacks_ground
    }
}

/// targeting.DOOMED_TARGET_DROP = projectile_attackers_walk_drop: may attacker `a`, keeping its current target `c`,
/// still keep it through the fired-at exemption (`can_target`'s `keeping`)? Under every other arm, yes. Under
/// projectile_attackers_walk_drop only while `a` is in its attack (its attack phase, which the previous tick's Attack
/// phase left, is not Idle) or `c` stands within its keep reach, Range + both radii +
/// LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET, on the start-of-tick positions. So a WALKING attacker beyond it drops a doomed
/// target it has shot at on the tick after the doom and, its rescans skipping every doomed unit, does not take it back.
/// Measured on client 15.535.29, the Skeleton Dragons sweep scene: a Skeleton Dragon spat at the Knight
/// from 5356 (keep reach 4925), kept it and walked after it, and on 348, 5041 from it, dropped it for the tower, the
/// tick after the tower's arrow joined the spit in flight (151 + 109 against 248 hp). Attackers in their attack that
/// had fired kept a doomed target in 1,007 of 1,007 episodes of the 15.535.29 records, one of them 234 beyond reach.
/// A knocked or recoiling unit never gets here (`decide` keeps its target without deciding).
#[inline]
fn keeps_fired(ctx: &TargetCtx, a: usize, c: usize) -> bool {
    if ctx.calib.doomed_target_drop != crate::state::DoomedTargetDrop::ProjectileAttackersWalkDrop {
        return true;
    }
    let e = ctx.ents;
    #[cfg(not(any(clash_plant = "doomed_walker_keeps_fired", clash_plant = "doomed_walk_drop_ignores_phase")))]
    let attacking = e.attack_phase[a] != crate::entity::AttackPhase::Idle;
    #[cfg(clash_plant = "doomed_walker_keeps_fired")]
    let attacking = true; // PLANT (regression): a walking attacker keeps a doomed target it has shot at.
    #[cfg(all(clash_plant = "doomed_walk_drop_ignores_phase", not(clash_plant = "doomed_walker_keeps_fired")))]
    let attacking = false; // PLANT (regression): the keep reach alone decides, so an attacker in its attack drops too.
    let keep = ctx.cards.get(e.card[a]).range + ctx.calib.range_extension_to_keep_target;
    attacking || in_attack_range(ctx.calib, e.pos[a], keep, e.radius[a], e.pos[c], e.radius[c])
}

/// targeting.MINIMUM_RANGE = client16402_edge_distance: does `c` stand inside attacker `a`'s MinimumRange, its edge
/// distance (centre distance less both collision radii) below it? Start-of-tick positions (the Target phase). The
/// measured law names both radii, so this does not follow targeting.ATTACK_RANGE_RULE. False for a card without
/// MinimumRange and under not_read.
#[inline]
pub fn inside_minimum_range(ctx: &TargetCtx, a: usize, c: usize) -> bool {
    let e = ctx.ents;
    let min = ctx.cards.get(e.card[a]).minimum_range;
    if ctx.calib.minimum_range != MinimumRange::Client16402EdgeDistance || min <= 0 {
        return false;
    }
    let r = (min as i64) + (e.radius[a] as i64) + (e.radius[c] as i64);
    e.pos[a].dist2(e.pos[c]) < r * r
}

/// Does targeting.CHASE_DROP_RANGE = client_sight_minus_1000 reach attacker `a` chasing `c`? A troop after a troop:
/// the measured chasers were troops and so were the runners, and whether a building target is ever dropped this way
/// is open.
#[inline]
fn chase_drop_applies(ctx: &TargetCtx, a: usize, c: usize) -> bool {
    ctx.calib.chase_drop_range == ChaseDropRange::ClientSightMinus1000
        && ctx.ents.kind[a] == EntityKind::Troop
        && ctx.ents.kind[c] == EntityKind::Troop
}

/// Does `c` stand past attacker `a`'s chase-drop limit, SightRange + both collision radii -
/// CHASE_DROP_SHORT_OF_SIGHT, on max(|dx|, |dy|) of the two start-of-tick centres? The measure is the proposal's
/// choice among those the measurements leave: along a lane |dy| and max(|dx|, |dy|) agree, and a diagonal chase on
/// client 15.535.29 refutes the Euclidean distance.
#[inline]
fn beyond_chase_limit(ctx: &TargetCtx, a: usize, c: usize) -> bool {
    past_chase_limit(ctx.cards, ctx.ents, a, c)
}

/// `beyond_chase_limit` on the current positions, for a caller without a `TargetCtx`.
#[inline]
fn past_chase_limit(cards: &CardDb, e: &Entities, a: usize, c: usize) -> bool {
    let limit = cards.get(e.card[a]).sight_range as i64 + e.radius[a] as i64 + e.radius[c] as i64
        - (CHASE_DROP_SHORT_OF_SIGHT as i64) * (crate::fixed::SUBTILE_PER_MILLITILE as i64);
    let d = e.pos[c].sub(e.pos[a]);
    (d.x as i64).abs().max((d.y as i64).abs()) > limit
}

/// targeting.CHASE_DROP_RANGE = client_sight_minus_1000, THE EDGE: `target` when troop `a` holds it, a live troop,
/// within the chase-drop limit on the current positions; else None, and None under the old arm. The Target phase's
/// caller records it for the target each decision leaves the unit holding, before anything moves, so on the positions
/// the decision read (entity.rs `chase_inside`); the next `decide` lets a target go past the limit only when this
/// named it. Measured on client 15.535.29 in the chase scenarios: on 35 of 35 ticks where a walking chaser's troop
/// target first stood past the limit the chaser let it go, and on 31 of 31 ticks where a troop it had taken past the
/// limit still stood past it the chaser kept it (walking and deploying targets; one walked away for 16 ticks).
pub fn chase_inside(calib: &Calib, cards: &CardDb, e: &Entities, a: usize, target: Option<EntityId>) -> Option<EntityId> {
    target.filter(|t| {
        let c = t.index as usize;
        calib.chase_drop_range == ChaseDropRange::ClientSightMinus1000
            && e.is_alive(*t)
            && e.kind[a] == EntityKind::Troop
            && e.kind[c] == EntityKind::Troop
            && !past_chase_limit(cards, e, a, c)
    })
}

/// How far past its reach a LOCKED attacker holds its target (the locked branch of `decide`): for a crown tower,
/// targeting.TOWER_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE when it names a number; for everything else, and for a tower
/// under "global", targeting.LOGIC_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE.
#[inline]
fn locked_hold_beyond(ctx: &TargetCtx, a: usize) -> i32 {
    #[cfg(not(clash_plant = "tower_cancel_global"))]
    let scoped = ctx.ents.kind[a].is_crown_tower();
    #[cfg(clash_plant = "tower_cancel_global")]
    let scoped = false; // PLANT (regression): a crown tower holds its shot to the global cancel range, as today.
    match ctx.calib.tower_cancel_range {
        TowerCancelRange::Beyond(n) if scoped => n,
        _ => ctx.calib.cancel_hit_from_long_distance_range,
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
///
/// Its first component is the distance targeting.TARGET_RANK_DISTANCE names. client16402_centre: the centre distance,
/// crown towers included. Measured on the 16.402 corpus: a troop walking to its crown tower takes the nearest enemy in
/// sight on the first tick that enemy's start-of-tick centre distance is below the tower's (577 of 578 walking
/// switches), and where centre and centre-minus-radius name different enemies the client took the centre-nearest (21
/// of 21 acquisitions). centre_minus_target_radius (today's engine): the centre distance less the candidate's radius,
/// which counts a princess tower 1000 nearer than it stands, so a walker keeps its tower against a nearer troop or
/// building (20260918-124946 tick 941: a Goblin 6,578.4 from a Cannon and 6,840.8 from its tower keeps the tower).
#[inline]
fn key(ctx: &TargetCtx, a: usize, c: usize) -> (i32, i32, i32, u32) {
    let e = ctx.ents;
    let centre = isqrt(e.pos[a].dist2(e.pos[c])) as i32;
    #[cfg(not(clash_plant = "rank_centre_minus_radius"))]
    let by_centre = ctx.calib.target_rank_distance == crate::state::TargetRankDistance::Client16402Centre;
    #[cfg(clash_plant = "rank_centre_minus_radius")]
    let by_centre = false; // PLANT (regression): client16402_centre still ranks by centre minus the candidate's radius.
    let edge = if !by_centre && ctx.calib.add_character_range_to_radius { centre - e.radius[c] } else { centre };
    let f = ctx.arena.to_frame(e.team[a], e.pos[c]);
    #[cfg(clash_plant = "id_tiebreak")]
    {
        // PLANT: tie-break on raw slot index -- a deploy-order asymmetry.
        return (edge, 0, 0, c as u32);
    }
    #[allow(unreachable_code)]
    (edge, f.x, f.y, e.team_seq[c])
}

/// A candidate's rank in `scan_with`: (deprioritized, `key`), lowest first.
type ScanKey = (bool, (i32, i32, i32, u32));

/// Nearest valid enemy in sight, or None.
pub fn scan(ctx: &TargetCtx, a: usize, scratch: &mut Vec<u32>) -> Option<EntityId> {
    scan_with(ctx, a, scratch, ctx.ents.chase_dropped[a])
}

/// `scan`, with `dropped` the troop the chase drop let go of (targeting.CHASE_DROP_RANGE = client_sight_minus_1000):
/// it is a candidate only within the chase-drop limit, measured as the drop measures it, while every other enemy is
/// one at plain sight. Measured on client 15.535.29: a Tornado dragging a dropped Hog Rider back gets it taken again
/// on the first tick it is inside the drop limit, 3 of 3 runs, and not while it is back in plain sight only; a
/// fresh enemy farther than the dropped runner is taken on the drop tick. A per-target range, not a timed exclusion.
fn scan_with(ctx: &TargetCtx, a: usize, scratch: &mut Vec<u32>, dropped: Option<EntityId>) -> Option<EntityId> {
    let e = ctx.ents;
    let card = ctx.cards.get(e.card[a]);
    let extra = ctx.calib.extra_sight_range_to_crown_towers.max(0) + ctx.calib.extra_sight_range_to_building.max(0);
    let query = card.sight_range + extra + ctx.hash.max_radius();
    ctx.hash.neighbours_within(e, e.pos[a], query, scratch);
    // targeting.DEPRIORITIZED_TARGET_BUFF: a carrier of the buff the attacker deprioritizes ranks after every other
    // candidate (`deprioritized`: false for every attacker that deprioritizes nothing, so the key is today's).
    let mut best: Option<(ScanKey, usize)> = None;
    for &c in scratch.iter() {
        let c = c as usize;
        if !can_target(ctx, a, c, false) {
            continue;
        }
        #[cfg(not(clash_plant = "sight_ignored"))]
        if !in_attack_range(ctx.calib, e.pos[a], sight_toward(ctx, a, c), e.radius[a], e.pos[c], e.radius[c]) {
            continue;
        }
        // targeting.MINIMUM_RANGE = client16402_edge_distance: never TAKE a target inside the minimum range.
        if inside_minimum_range(ctx, a, c) {
            continue;
        }
        #[cfg(not(clash_plant = "chase_drop_rescan_admits"))]
        if dropped == Some(e.id_of(c)) && chase_drop_applies(ctx, a, c) && beyond_chase_limit(ctx, a, c) {
            continue;
        }
        #[cfg(clash_plant = "chase_drop_rescan_admits")]
        let _ = dropped; // PLANT (regression): the dropped troop is a candidate again at plain sight.
        let k = (deprioritized(ctx, a, c), key(ctx, a, c));
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
    #[cfg(clash_plant = "acquire_delay_wakes_hidden")]
    let ctx = &TargetCtx { tick: u32::MAX, ..*ctx }; // PLANT: a unit inside its acquire delay wakes a hidden building.
    let e = ctx.ents;
    let card = ctx.cards.get(e.card[a]);
    let extra = ctx.calib.extra_sight_range_to_crown_towers.max(0) + ctx.calib.extra_sight_range_to_building.max(0);
    let query = card.sight_range.max(card.range) + extra + ctx.hash.max_radius();
    ctx.hash.neighbours_within(e, e.pos[a], query, scratch);
    scratch.iter().any(|&c| {
        let c = c as usize;
        if !can_target(ctx, a, c, false) {
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
        return TargetDecision { target: None, cancel_attack: false, resumed: false, chase_dropped: None };
    }
    // Under ground or still coming up: no target at all, and any windup it had is
    // cancelled (it cannot have one; belt and braces for a building that went
    // under mid-swing under hide.HIDE_DELAY_MEANING = time_since_last_shot). Its
    // surfacing tick under hide.RISE_LAW = client16402_surface_attacking is not
    // coming up: it takes its target on it (`hide_acts`).
    if !hide_acts(ctx.calib, e, a) {
        return TargetDecision { target: None, cancel_attack: true, resumed: false, chase_dropped: None };
    }
    // Stunned or mid-knockback (the slide, or the 16.402 ladder): keep what it
    // had, scan nothing. The same for a death-spawn member still sliding out
    // (spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide): born with no target, it takes
    // none until the slide ends.
    #[cfg(not(clash_plant = "death_slide_targets"))]
    let sliding = e.death_sliding(a);
    #[cfg(clash_plant = "death_slide_targets")]
    let sliding = false; // PLANT: a sliding member scans and takes a target.
    if e.stun_ms[a] > 0 || e.knocked(a) || sliding {
        return TargetDecision { target: cur.filter(|t| e.is_alive(*t)), cancel_attack: false, resumed: false, chase_dropped: None };
    }
    if e.kind[a] == EntityKind::KingTower && !ctx.king_active[e.team[a] as usize] {
        return TargetDecision { target: None, cancel_attack: false, resumed: false, chase_dropped: None };
    }
    // RESUME: the first tick after a stun, a fresh scan that ignores the target lock
    // and keep-target hysteresis (Supercell 2017-03-13: stuns 'pause the target's
    // attack, causing them to retarget when they resume').
    #[cfg(not(clash_plant = "no_retarget_after_stun"))]
    if e.retarget_on_resume[a] {
        return TargetDecision { target: scan(ctx, a, scratch), cancel_attack: false, resumed: true, chase_dropped: None };
    }
    let card = ctx.cards.get(e.card[a]);
    let mut cancel = false;
    if let Some(t) = cur {
        // targeting.DOOMED_TARGET_DROP = projectile_attackers_walk_drop: a walking attacker beyond its keep reach no
        // longer keeps a doomed target through the fired-at exemption (`keeps_fired`); true under every other arm.
        let targetable = e.is_alive(t) && can_target(ctx, a, t.index as usize, keeps_fired(ctx, a, t.index as usize));
        // targeting.MINIMUM_RANGE = client16402_edge_distance: a target inside the attacker's MinimumRange is dropped
        // on this tick with its swing cancelled (idle, progress 0), as a lost target and not a kill, so no post-kill
        // wait follows; the rescan below does not take it back while it stands there. Always false under not_read.
        let too_close = targetable && inside_minimum_range(ctx, a, t.index as usize);
        if targetable && !too_close {
            let ti = t.index as usize;
            #[cfg(not(clash_plant = "no_target_lock"))]
            let locked = e.target_locked[a];
            #[cfg(clash_plant = "no_target_lock")]
            let locked = false; // PLANT: the windup lock never holds.
            if ctx.calib.locks_target(card) && ctx.calib.preserve_target_scope == PreserveTargetScope::ProjectileAttackersOnly {
                // targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED = "projectile_attackers_only": a projectile
                // attacker holds its target to PROJECTILE_HOLD_BEYOND_REACH past reach, on the start-of-tick
                // positions (this is the Target phase), on EVERY tick and not only while a swing locks it: on
                // client 15.535.29 the let-go frame follows the distance at every phase of the attack cycle. A
                // launch at the target from beyond reach ends the hold on the next tick (`launched_beyond`). Past
                // either, a rescan: the nearest enemy in sight, which may be the same target, and the shot is not
                // cancelled (combat.RETARGET_PROGRESS decides what a switch does to it). A crown tower's rescan
                // finds nothing past its range, so it drops the target.
                let hold = card.range + PROJECTILE_HOLD_BEYOND_REACH * crate::fixed::SUBTILE_PER_MILLITILE;
                if !e.launched_beyond[a] && in_attack_range(ctx.calib, e.pos[a], hold, e.radius[a], e.pos[ti], e.radius[ti]) {
                    return TargetDecision { target: Some(t), cancel_attack: false, resumed: false, chase_dropped: None };
                }
            } else if locked && ctx.calib.locks_target(card) {
                // targeting.TOWER_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE: a crown tower's own hold when the key names one.
                let hold = card.range + locked_hold_beyond(ctx, a);
                if in_attack_range(ctx.calib, e.pos[a], hold, e.radius[a], e.pos[ti], e.radius[ti]) {
                    return TargetDecision { target: Some(t), cancel_attack: false, resumed: false, chase_dropped: None };
                }
                cancel = true;
            } else {
                let keep = card.range + ctx.calib.range_extension_to_keep_target;
                if in_attack_range(ctx.calib, e.pos[a], keep, e.radius[a], e.pos[ti], e.radius[ti]) {
                    return TargetDecision { target: Some(t), cancel_attack: false, resumed: false, chase_dropped: None };
                }
            }
            // targeting.CHASE_DROP_RANGE = client_sight_minus_1000: the target is a troop outside the attacker's attack
            // reach (no branch above kept it), so the attacker is walking after it. Past the chase-drop limit on the
            // start-of-tick positions it lets go and rescans, and that rescan and the later ones admit the dropped
            // troop only within the limit (`scan_with`). The attack-reach keeps above are untouched. A troop just
            // out of a swing takes this test on its first walking tick, as measured (a Prince at -990).
            // THE DROP IS AN EDGE: only a target the previous Target phase found within the limit (entity.rs
            // `chase_inside` names it) is let go. One taken past the limit, at plain sight, is walked after and
            // rescanned as today until it has been inside (client 15.535.29, the chase scenarios: 31 of 31 such ticks).
            #[cfg(not(any(clash_plant = "chase_drop_ignored", clash_plant = "chase_drop_level_triggered")))]
            let dropped = chase_drop_applies(ctx, a, ti) && e.chase_inside[a] == Some(t) && beyond_chase_limit(ctx, a, ti);
            #[cfg(all(clash_plant = "chase_drop_level_triggered", not(clash_plant = "chase_drop_ignored")))]
            let dropped = chase_drop_applies(ctx, a, ti) && beyond_chase_limit(ctx, a, ti); // PLANT (regression): a target taken past the limit is let go on the next tick.
            #[cfg(clash_plant = "chase_drop_ignored")]
            let dropped = false; // PLANT (regression): the chaser keeps the runner to its plain sight, as today.
            if dropped {
                return TargetDecision { target: scan_with(ctx, a, scratch, Some(t)), cancel_attack: cancel, resumed: false, chase_dropped: Some(t) };
            }
        } else if e.target_locked[a] || too_close {
            cancel = true;
        }
    }
    TargetDecision { target: scan(ctx, a, scratch), cancel_attack: cancel, resumed: false, chase_dropped: None }
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
        // targeting.FIRST_TOWER_PICK = client_spawn_lane: while the unit deploys and for FIRST_PICK_LANE_WINDOW_MS
        // after, only the enemy princess tower of its spawn lane is a candidate (entity.rs `spawn_lane`, an ENGINE
        // lane: the tilemap names lanes in engine x), the king once that tower is down. After the window, the pick
        // below by its current x, as today. `spawn_lane` is 0 on every unit under the old arm.
        #[cfg(not(clash_plant = "first_pick_by_x"))]
        let lane_pick = ctx.calib.first_tower_pick.spawn_lane() && e.spawn_lane[a] != 0 && ctx.tick <= e.lane_window_end[a];
        #[cfg(clash_plant = "first_pick_by_x")]
        let lane_pick = false; // PLANT (regression): the first pick by the unit's current x, as today.
        if lane_pick {
            let lane = if e.spawn_lane[a] & ctx.arena.bit_lane_left != 0 { Lane::Left } else { Lane::Right };
            return live(ctx.towers[enemy][1 + lane as usize]).or_else(|| live(ctx.towers[enemy][0]));
        }
        // THE LANE IS DECIDED IN THE ATTACKER'S OWN FRAME, with the centre line going
        // own-left. targeting.CENTRE_LANE_FRAME, and the ledger entry carries the history.
        //
        // WHAT CLIENT 15.535.29 ACTUALLY SETTLED, 2026-09-23, 12 scenarios over 2 cards x 2
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
        // WHY THIS ARM AND NOT THE OTHER, given the 15.535.29 scenarios are silent. The own-frame rule is
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
        // the scenario fixtures at their raw tap (x = 9000, a tile CORNER) where the client
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
