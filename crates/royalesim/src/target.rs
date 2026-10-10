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
use crate::card::{CardDb, CardDef};
use crate::entity::{EntityKind, Entities, HideState, SpatialHash};
use crate::fixed::{in_range_edge, isqrt, Vec2};
use crate::state::{
    AttackRangeRule, Calib, CentreLaneFrame, ScanReach, ChaseDropMeasure, ChaseDropRange, ChaseDropWalkingAway, ChaseHoldPastLimit, ChaseRescanPassOver, DeprioritizedTargetBuff, EqualDistanceTie, KnockedLostTarget, KnockedTargetHold, LeapingUnitTargetability,
    MinimumRange, PreserveTargetScope, SlapFlightSightHold, SlapFlightTargetability, RiderTargetable, RiseLaw, RiseTrigger, TowerCancelRange, WalkingKeepReach, ChaseHoldScope,
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
    /// targeting.DOOMED_LANE_TOWER, the new arms: per slot, the doomed set the latest Target pass read (state.rs
    /// `Scratch::lane_doomed`), handed to the Path phase for `default_tower` (`lane_fallen`). Empty under standing and
    /// in every other ctx.
    pub lane_doomed: &'a [bool],
    /// targeting.SLAP_FLIGHT_TARGETABILITY = client15535_airborne: per slot, whether the unit is in a Hero Giant's slap
    /// flight (state.rs `slap_air_mask`), which `can_target` reads as a river leap. Empty under ground and outside the
    /// Target phase.
    pub slap_air: &'a [bool],
    /// targeting.UPPERCUT_FLIGHT_TARGETABILITY = client15535_airborne: per slot, whether the unit is in an Evo Mega Knight's
    /// uppercut flight (state.rs `upper_air_mask`), read by `can_target` as a slap's flight. Empty under ground and outside
    /// the Target phase.
    pub upper_air: &'a [bool],
    /// targeting.CHASE_DROP_WALKING_AWAY = client15535_growing_away: per slot, whether the unit walked as the tick's Target
    /// phase began (state.rs `Scratch::chase_walked`, `walking_now`). Empty under the other arms and outside the phase,
    /// where `walks_away` reads the units as they stand.
    pub chase_walked: &'a [bool],
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

/// A BUILDINGS-ONLY WALKER'S SCAN IGNORES A FAR BUILDING (`scan_with`): a player building (not a crown tower) whose x
/// lies more than this from the walker's own x is no candidate, however near by centre. NATIVE units. Measured on
/// client 15.535.29 (Oracle's 18 building scenes: a Giant or an Ice Golem against a Cannon or a Bomb Tower, on the
/// lanes and off them): with that building ignored, plain straight-line sight and centre ranking fit every switch.
/// Taken at |dx| 2049, 3236, about 5000, 6212 to 6223 and 6231 (x4); never taken at 7231 (x4), 7232, and 7269 to 7984
/// while it stood in sight and nearer than the tower. A Hog Rider off the river narrows the band (sp-il-925e, client
/// 15.535.29, its first divergence): landing from its jump with a Blue Cannon at (9500, 13500) in sight and nearer than
/// its princess tower, it kept the tower at |dx| 6972 (where 7000 took the Cannon), then 6890 down to 6811 over eight
/// ticks, and took the Cannon on t1455 at 6697. A second Hog Rider fits it (sp-il-db5f t1935 to t1942): kept its tower
/// over a Blue Cannon at (8500, 10500) at a start-of-tick |dx| of 6972 down to 6832, took the Cannon at 6686. The cut-off
/// lies in [6697, 6811); 6750 is a value inside that band, not a measured one. Off a lane the start-of-tick x decides it. Unmeasured: troops that target anything (no scene has one
/// past 6600 of sight); the Giant (sight 7500), the Ice Golem (7000) and the Hog Rider (9500) all fit one value.
#[cfg(not(clash_plant = "building_scan_dx_7000"))]
pub const BUILDING_SCAN_DX: i32 = 6750;
/// PLANT (regression) building_scan_dx_7000: the value before the Hog Rider's band, which took its Cannon at 6972.
#[cfg(clash_plant = "building_scan_dx_7000")]
pub const BUILDING_SCAN_DX: i32 = 7000;
/// targeting.BUILDING_SCAN_X_CUT = client16402_edge_6700_melee: a melee buildings-only walker ignores a building when
/// |dx| + its radius - the building's radius passes this, native. Measured on client 16.402 (the live population, parity's
/// bscan census, truth only): the Royal Hog (r 600) takes a Cannon (600) at |dx| 6700 and keeps its tower from 6704; the
/// Balloon (500) at 6793 / 6802; the Skeleton Balloon (500) at 6800 / 6816: 6700, 6693 / 6702, 6700 / 6716 by the rule.
/// Client 15.535.29's Hog Rider (600, a Cannon) took at 6697 and kept at 6811.
pub const BUILDING_SCAN_EDGE_DX: i32 = 6700;

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

/// targeting.VARIABLE_DAMAGE_WALK_REACH: the attacker radius a WALKING unit's reach adds, where `own` is its
/// collision radius (subtiles). Under client16402_no_own_radius_walking, 0 for a FLYING card whose row sets
/// VariableDamage2 (card.rs `CardDef::variable_damage`, `is_flying`): in the 15.535.29 tables that is the Inferno
/// Dragon's row alone. Under client15535_no_own_radius_walking_every_row, 0 for EVERY card whose row sets
/// VariableDamage2, ground or flying: a ramp (`variable_damage`: the Inferno Dragon, the Mighty Miner) or a combo
/// (`combo`: the Monk, the Mega Monk). Measured on client 15.535.29: a walking Monk's attack started 1,575, 1,652
/// and 1,696 from a target of radius 500 (Range 1,200 + the target's radius: 1,700; Range + both radii: 2,200) and
/// a Mighty Miner's 2,050 (Range 1,600 + 500: 2,100), where the Golden Knight, the Skeleton King, the Knight, the
/// Skeletons, the Musketeer, the Mini P.E.K.K.A and the Valkyrie started at Range + both radii. `own` for every other
/// card and under the old arm. The caller decides what "walking" is (the Path phase's goal cell and
/// direct aim, which run only for a unit about to walk; a unit holding a walking goal, entity.rs `route_goal`, in the
/// Path phase's in-range test and the attack cycle's range gate). A STANDING unit's reach adds its own radius under
/// both arms.
///
/// Measured on the 16.402 corpus (one Inferno Dragon battle, one seat) and on client 15.535.29 (the Inferno Dragon
/// sweep scene): a walking Inferno Dragon walked on through 31 + 16 ticks that started with its target inside Range +
/// both radii but outside Range + the target's radius, and stopped on none; it stopped 4 + 2 times, each on the first
/// tick that started inside Range + the target's radius. Walkers of every other card stood on 979 + 235 such ticks
/// (and walked on 277 + 38). Standing, it held its place through 42 + 17 ticks with the target in that band, and its
/// attack gate let go only past Range + both radii (the sweep's 4,506 from a Knight, 6 past it). Its goal cells lie
/// within Range of the target's centre (the largest 3,523 of 20 choices on the corpus seat, 3,509 of 5 in the
/// sweep); every other flyer's reach Range + its radius.
#[inline]
pub fn walking_own_radius(calib: &Calib, card: &CardDef, own: i32) -> i32 {
    use crate::state::VariableDamageWalkReach as W;
    #[cfg(not(clash_plant = "walk_reach_keeps_own_radius"))]
    let no_own = match calib.variable_damage_walk_reach {
        W::RangePlusBothRadii => false,
        W::Client16402NoOwnRadiusWalking => card.variable_damage.is_some() && card.is_flying(),
        #[cfg(not(clash_plant = "walk_reach_every_row_flyers_only"))]
        W::Client15535NoOwnRadiusWalkingEveryRow => card.variable_damage.is_some() || card.combo.is_some(),
        // PLANT (regression): the every-row arm still reads flyers alone, so the Monk and the Mighty Miner walk to
        // Range + both radii.
        #[cfg(clash_plant = "walk_reach_every_row_flyers_only")]
        W::Client15535NoOwnRadiusWalkingEveryRow => card.variable_damage.is_some() && card.is_flying(),
    };
    #[cfg(clash_plant = "walk_reach_keeps_own_radius")]
    let no_own = {
        let _ = (calib, card);
        false // PLANT (regression): the new arm still adds the walker's own radius.
    };
    if no_own {
        0
    } else {
        own
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
    // A CARRIED INVISIBLE BUFF (status.rs `BuffDef::invisible`; the Archer Queen's cape), whatever hung it: no enemy
    // may target the carrier while it lasts, a kept target included. Measured on client 15.535.29
    // (sp-champ-ArcherQueen-s0): the Knight, the Skeleton and the Musketeer that held her all took a tower on the
    // cape's first frame; a shot already flying still landed.
    #[cfg(not(clash_plant = "buff_invisible_targetable"))]
    if ents.buff_slots(c).iter().any(|s| !s.is_empty() && cards.buffs[(s.id - 1) as usize].invisible) {
        return true;
    }
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
    if ctx.calib.formation_stagger_wait.waits() && e.stagger_ms[c] > 0 {
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
    // TargetOnlyKingTower (card.rs `target_only_king_tower`, the Goblin Rocket Silo's last stage): the enemy king alone.
    // Measured on client 15.535.29 (sp-event-GoblinRocketSilo-s0): GoblinRocketSilo2 took side 1's king, 17,678 off, on
    // the frame it became that row, with a princess tower and a Knight nearer.
    #[cfg(not(clash_plant = "king_only_unread"))]
    if ctx.cards.get(e.card[a]).target_only_king_tower && e.kind[c] != crate::entity::EntityKind::KingTower {
        return false;
    }
    // A SPAWNER'S BOTTLE'S BODY (card.rs `bottle_body`, the Super Mini PEKKA's pancake) is nobody's target.
    #[cfg(not(clash_plant = "bottle_body_targetable"))]
    if ctx.cards.get(e.card[c]).untargetable {
        return false;
    }
    let card = ctx.cards.get(e.card[a]);
    // targeting.DOOMED_TARGET_DROP: a projectile attacker neither keeps nor takes a unit the shots already in flight
    // will kill (`ctx.doomed`) unless it has shot at it (`drops_when_doomed`).
    if drops_when_doomed(ctx, a, c, keeping) && ctx.doomed.get(c).copied().unwrap_or(false) {
        return false;
    }
    // targeting.DOOMED_LANE_TOWER = walkers_take_king: nor does a walking troop keep or take a doomed princess tower.
    if walker_refuses_doomed_princess(ctx, a, c) {
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
    // targeting.SLAP_FLIGHT_TARGETABILITY = client15535_airborne: a unit in a Hero Giant's slap flight, from the throw to
    // its landing, the same (`TargetCtx::slap_air`; client 15.535.29: 5 of 5 ground-only holders let it go on the first
    // flight step, 6 of 6 that attack air kept it).
    // targeting.UPPERCUT_FLIGHT_TARGETABILITY = client15535_airborne: a unit in an Evo Mega Knight's uppercut flight, the same
    // (`TargetCtx::upper_air`, built under that arm alone).
    let thrown = (ctx.calib.slap_flight_targetability == SlapFlightTargetability::Client15535Airborne && ctx.slap_air.get(c).copied().unwrap_or(false))
        || ctx.upper_air.get(c).copied().unwrap_or(false);
    #[cfg(not(clash_plant = "leap_targetable_by_ground"))]
    let airborne = (ctx.calib.leaping_unit_targetability == LeapingUnitTargetability::Airborne && e.jumping[c]) || thrown;
    #[cfg(clash_plant = "leap_targetable_by_ground")]
    let airborne = thrown; // PLANT (regression): a leaping troop stays a ground target under the new arm too.
    if e.in_air(c) || airborne {
        card.attacks_air
    } else {
        card.attacks_ground
    }
}

/// targeting.DOOMED_TARGET_DROP: is attacker `a` one that lets go of `c`, and does not take it, while the shots in
/// flight doom it? Under projectile_attackers, an attacker whose card fires a projectile, unless it has launched a shot at
/// `c` since acquiring it (entity.rs `fired_at`). So it drops the target on the tick after the doom and does not take it
/// back while it lives. Under projectile_attackers_rescan that exemption covers KEEPING only: a scan never takes a doomed
/// unit, the one the attacker has just shot at included (client 15.535.29: 4 of 4 launches beyond reach at a doomed
/// target were followed by a drop, none retaken). Under projectile_attackers_walk_drop `decide` passes `keeping` only
/// while the attacker may still keep a unit it has shot at (`keeps_fired`: in its attack, or the target within its keep
/// reach). False under keep. Whether `c` IS doomed is the caller's reading (`can_target`, `lane_fallen`).
#[inline]
fn drops_when_doomed(ctx: &TargetCtx, a: usize, c: usize, keeping: bool) -> bool {
    let e = ctx.ents;
    let card = ctx.cards.get(e.card[a]);
    #[cfg(clash_plant = "doomed_rescan_takes_fired")]
    let keeping = true; // PLANT (regression): a rescan takes back a doomed unit the attacker has shot at.
    // targeting.DOOMED_DROP_SPEAR_MEMBERS = client15535_projectile: an Evo Elite Barbarian member (its row's projectile
    // taken off by the loader) is a projectile attacker here (client 15.535.29: post-throw rescans passed the spear's doomed
    // victim 6 of 6; sp-form-AngryBarbarians-evo-s0 t885).
    // PLANT (regression) spear_members_scan_as_melee: the new arm's members take doomed units as melee units do.
    #[cfg(not(clash_plant = "spear_members_scan_as_melee"))]
    let spear = ctx.calib.doomed_drop_spear_members == crate::state::DoomedDropSpearMembers::Client15535Projectile
        && card.evo.as_ref().is_some_and(|v| v.spear.is_some());
    #[cfg(clash_plant = "spear_members_scan_as_melee")]
    let spear = false;
    #[cfg(not(clash_plant = "doomed_drop_every_attacker"))]
    let applies = card.projectile.is_some() || spear;
    #[cfg(clash_plant = "doomed_drop_every_attacker")]
    let applies = {
        let _ = spear;
        true // PLANT (regression): an attacker with no projectile drops it too.
    };
    #[cfg(not(clash_plant = "doomed_drop_ignores_fired"))]
    let exempt = e.fired_at[a] == Some(e.id_of(c))
        && (keeping || ctx.calib.doomed_target_drop == crate::state::DoomedTargetDrop::ProjectileAttackers);
    #[cfg(clash_plant = "doomed_drop_ignores_fired")]
    let exempt = false; // PLANT (regression): an attacker that has fired drops it too.
    ctx.calib.doomed_target_drop.drops() && applies && !exempt
}

/// targeting.DOOMED_LANE_TOWER = walkers_take_king, parity's proposal as written: does WALKING troop `a` (not in its
/// attack: its attack phase, which the previous tick's Attack phase left, is Idle, as `keeps_fired` reads it) refuse
/// `c`, a princess tower in the doomed set (`ctx.doomed`)? Its scan then finds the king or nothing in sight, and its
/// default tower is the king (`lane_fallen`). An attacker keeps it, as targeting.DOOMED_TARGET_DROP says. False under
/// every other arm: under projectile_walkers_take_king the walkers that let go of a doomed princess are the ones
/// DOOMED_TARGET_DROP already makes (`drops_when_doomed`), and a walker without a projectile keeps it.
#[inline]
fn walker_refuses_doomed_princess(ctx: &TargetCtx, a: usize, c: usize) -> bool {
    #[cfg(not(clash_plant = "doomed_lane_walker_keeps"))]
    let arm = ctx.calib.doomed_lane_tower == crate::state::DoomedLaneTower::WalkersTakeKing;
    #[cfg(clash_plant = "doomed_lane_walker_keeps")]
    let arm = false; // PLANT (regression): under walkers_take_king a walking troop still keeps a doomed princess tower.
    let e = ctx.ents;
    arm && e.kind[c] == EntityKind::PrincessTower
        && e.kind[a] == EntityKind::Troop
        && e.attack_phase[a] == crate::entity::AttackPhase::Idle
        && ctx.doomed.get(c).copied().unwrap_or(false)
}

/// targeting.DOOMED_LANE_TOWER: is princess tower `t`, standing, fallen to unit `a` as the tower it walks to
/// (`default_tower`)? Under projectile_walkers_take_king, when `t` is in the doomed set the latest Target pass read
/// (`ctx.lane_doomed`) and `a` is one DOOMED_TARGET_DROP makes let go of it (`drops_when_doomed`); under
/// walkers_take_king, when `t` is in that set (every unit that walks to a default tower is walking). Then its default
/// tower is the next one, the king. A king is never fallen here: it has no next tower. Always false under standing.
///
/// Read off client 15.535.29 and the 16.402 corpus with the engine's doomed set (the ledger's provenance): every walker
/// with a projectile at a doomed princess took the king on the tick after the engine's set first held it, even where
/// the other princess stood nearer (scene-c's and m6's far spirits), and every walker without one kept the princess
/// (16.402: 20260918-112751's two Tombstone Skeletons for the ten ticks to its fall).
#[inline]
fn lane_fallen(ctx: &TargetCtx, a: usize, t: EntityId) -> bool {
    use crate::state::DoomedLaneTower;
    #[cfg(not(clash_plant = "doomed_lane_tower_standing"))]
    let arm = ctx.calib.doomed_lane_tower;
    #[cfg(clash_plant = "doomed_lane_tower_standing")]
    let arm = DoomedLaneTower::Standing; // PLANT (regression): the new arms still walk on to a doomed princess tower.
    let e = ctx.ents;
    let c = t.index as usize;
    if arm == DoomedLaneTower::Standing || e.kind[c] != EntityKind::PrincessTower || !ctx.lane_doomed.get(c).copied().unwrap_or(false) {
        return false;
    }
    match arm {
        DoomedLaneTower::ProjectileWalkersTakeKing => drops_when_doomed(ctx, a, c, false),
        DoomedLaneTower::WalkersTakeKing => true,
        DoomedLaneTower::Standing => false,
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

/// targeting.DOOMED_DROP_SWING = client_keep_in_reach: is `a`'s live target `t` let go of ONLY because the shots in
/// flight doom it (targeting.DOOMED_TARGET_DROP: `can_target` refuses it, and would take it with no unit doomed)? Such a
/// drop is a switch away from a live target, not a broken lock, so `decide` does not cancel the swing for it and
/// combat.RETARGET_PROGRESS decides (state.rs `phase_target_with`): under keep_when_dead_or_in_reach a switch to an enemy
/// already in reach keeps the swing, and one to an enemy out of reach, or to nothing, cancels it. Measured on the 16.402
/// corpus, one seat per battle (38 battles): a projectile attacker attacking a target that the shots in flight doom, and
/// that has not fired at it, switches on the next tick; when the new target stands in its reach on the start-of-tick
/// positions its attack progress runs on (57 of 57: 38 troops, 19 crown towers; 20260920-072148 tick 1035: a Minion at
/// progress 1050 of 1200 switches from a Skeleton doomed by a Musketeer's shot to another in reach, reads 1100 and 1150,
/// and fires on 1037), and when it does not the attacker walks with progress 0 (44 of 44 troops; 19 of 19 towers go
/// idle). Client 15.535.29 battery: 31 of 31 in reach kept. Always false under cancel, today's engine.
#[inline]
fn dropped_for_doom(ctx: &TargetCtx, a: usize, t: EntityId) -> bool {
    #[cfg(not(clash_plant = "doomed_drop_cancels_swing"))]
    let keep_swing = ctx.calib.doomed_drop_swing == crate::state::DoomedDropSwing::ClientKeepInReach;
    #[cfg(clash_plant = "doomed_drop_cancels_swing")]
    let keep_swing = false; // PLANT (regression): the new arm still cancels the swing when a doomed target is dropped.
    if !keep_swing {
        return false;
    }
    let e = ctx.ents;
    let c = t.index as usize;
    let undoomed = TargetCtx { doomed: &[], ..*ctx };
    e.is_alive(t) && ctx.doomed.get(c).copied().unwrap_or(false) && can_target(&undoomed, a, c, keeps_fired(ctx, a, c))
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
    past_chase_limit(ctx.calib, ctx.cards, ctx.ents, a, c)
}

/// Does unit `i` WALK as it stands: not in an attack, not deploying, not held, not sliding under a knockback?
/// (targeting.CHASE_DROP_WALKING_AWAY; state.rs `chase_pass_start` reads it for every unit as the Target phase begins.)
#[inline]
pub fn walking_now(e: &Entities, i: usize) -> bool {
    e.attack_phase[i] == crate::entity::AttackPhase::Idle && e.deploy_ms[i] <= 0 && e.stun_ms[i] <= 0 && e.knock_ms[i] <= 0 && !e.push_active[i]
}

/// targeting.CHASE_DROP_WALKING_AWAY: does troop `c` WALK AWAY from `a`? Always true under any_target, so the edge and
/// the rescan read as before. client_walking_away: it walks (`walking_now`) and its facing has a positive component
/// along (c - a), on the start-of-tick positions. client15535_growing_away: it walked as the Target phase began
/// (`TargetCtx::chase_walked`), its own step since the last Target phase (entity.rs `chase_last_pos`) points away from
/// `a` along the measure (targeting.CHASE_DROP_MEASURE: the component along (c - a), or along its y under
/// client15535_lane_dy), and the measured distance grew since then (`chase_measure`).
#[inline]
fn walks_away(ctx: &TargetCtx, a: usize, c: usize) -> bool {
    let e = ctx.ents;
    match ctx.calib.chase_drop_walking_away {
        ChaseDropWalkingAway::AnyTarget => return true,
        ChaseDropWalkingAway::ClientWalkingAway => {}
        ChaseDropWalkingAway::Client15535GrowingAway => {
            // PLANT (regression) chase_walk_read_live: the walking mark read as the unit stands (a target whose own turn
            // started its attack earlier in the sequential pass no longer walks away).
            #[cfg(not(clash_plant = "chase_walk_read_live"))]
            let walked = ctx.chase_walked.get(c).copied().unwrap_or_else(|| walking_now(e, c));
            #[cfg(clash_plant = "chase_walk_read_live")]
            let walked = walking_now(e, c);
            let d = e.pos[c].sub(e.pos[a]);
            let step = e.pos[c].sub(e.chase_last_pos[c]);
            let away = match ctx.calib.chase_drop_measure {
                ChaseDropMeasure::MaxAbs => (step.x as i64) * (d.x as i64) + (step.y as i64) * (d.y as i64) > 0,
                ChaseDropMeasure::Client15535LaneDy => (step.y as i64) * (d.y as i64) > 0,
            };
            // PLANT (regression) chase_growth_unread: a troop walking away counts whether its distance grew or fell.
            #[cfg(not(clash_plant = "chase_growth_unread"))]
            let grew = chase_measure(ctx.calib, d) > chase_measure(ctx.calib, e.chase_last_pos[c].sub(e.chase_last_pos[a]));
            #[cfg(clash_plant = "chase_growth_unread")]
            let grew = true;
            return walked && away && grew;
        }
    }
    #[cfg(clash_plant = "chase_drop_any_growth")]
    return true; // PLANT (regression): every troop past the limit counts as walking away, whatever it does.
    #[allow(unreachable_code)]
    {
        let d = e.pos[c].sub(e.pos[a]);
        let f = e.facing[c];
        walking_now(e, c) && (f.x as i64) * (d.x as i64) + (f.y as i64) * (d.y as i64) > 0
    }
}

/// targeting.CHASE_RESCAN_PASS_OVER = client15535_receding_lane_walk (read under targeting.CHASE_DROP_WALKING_AWAY =
/// client15535_growing_away): does `a`'s rescan pass over troop `c` because `a` walked into its last Target phase and
/// left it holding no target (entity.rs `chase_lane_walk`) and the measured distance (targeting.CHASE_DROP_MEASURE) grew
/// since that phase (entity.rs `chase_last_pos`), whatever `c` does? The caller reads it past the limit alone.
///
/// A troop not on the board at the start of the tick before (`spawn_tick`: made on it, or since) never recedes: its
/// `chase_last_pos` is its birth point, which no frame showed, where the client reads the growth frame to frame. Measured
/// on client 15.535.29 (item 313; sp-form-Tombstone-hero-nopress-s0 t154: a Red Skeleton pushed back 75 by its own
/// neighbour, |dy| 99,450 -> 99,648 to the hero Tombstone's first Skeleton born the tick before, took it with the other
/// two); the 2 receding pass-overs of the 426 scored scenes were both this one (tools/recede_census.py).
#[inline]
fn recedes_from_lane_walk(ctx: &TargetCtx, a: usize, c: usize) -> bool {
    // PLANT (regression) chase_rescan_drop_tick_only: the new arm still passes over on the drop tick alone.
    #[cfg(clash_plant = "chase_rescan_drop_tick_only")]
    return false;
    #[allow(unreachable_code)]
    {
        let e = ctx.ents;
        #[cfg(not(clash_plant = "recede_reads_newborn"))]
        let seen_before = e.spawn_tick[c].saturating_add(1) < ctx.tick;
        // PLANT (regression) recede_reads_newborn: a troop born the tick before recedes from its birth point.
        #[cfg(clash_plant = "recede_reads_newborn")]
        let seen_before = true;
        matches!(
            ctx.calib.chase_rescan_pass_over,
            ChaseRescanPassOver::Client15535RecedingLaneWalk | ChaseRescanPassOver::Client15535RecedingOrBehind | ChaseRescanPassOver::Client16402RecedingOrBehindEveryRescan
        ) && e.chase_lane_walk.get(a).copied().unwrap_or(false)
            && seen_before
            && chase_measure(ctx.calib, e.pos[c].sub(e.pos[a])) > chase_measure(ctx.calib, e.chase_last_pos[c].sub(e.chase_last_pos[a]))
    }
}

/// targeting.CHASE_RESCAN_PASS_OVER = client15535_receding_or_behind: does `a`'s rescan pass over troop `c` because `c`
/// stands BEHIND `a`, toward `a`'s own side along y on the start-of-tick positions, whatever either does, on a rescan
/// that is not a chase drop's own (`dropped`, `after_drop`: the drop tick's rescan keeps its own rules)? The caller reads
/// it past the limit alone. Measured on client 15.535.29: a troop behind past the limit was taken in 0 of 1,326 rescans.
#[inline]
fn behind_on_rescan(ctx: &TargetCtx, a: usize, c: usize, dropped: Option<EntityId>, after_drop: bool) -> bool {
    // PLANT (regression) rescan_behind_taken: the new arm takes a troop behind past the limit as the receding arm does.
    #[cfg(clash_plant = "rescan_behind_taken")]
    return false;
    #[allow(unreachable_code)]
    {
        let e = ctx.ents;
        let dy = e.pos[c].y as i64 - e.pos[a].y as i64;
        let behind = if e.team[a] == Team::Blue { dy < 0 } else { dy > 0 };
        // client16402_receding_or_behind_every_rescan: the chase drop's own rescan too (client 16.402: sp-hogs-cannon-s0
        // t344, 4 of 4 drop ticks with the troop behind).
        // PLANT (regression) rescan_behind_drop_tick_taken: the new arm still exempts the drop tick.
        #[cfg(not(clash_plant = "rescan_behind_drop_tick_taken"))]
        let every = ctx.calib.chase_rescan_pass_over == ChaseRescanPassOver::Client16402RecedingOrBehindEveryRescan;
        #[cfg(clash_plant = "rescan_behind_drop_tick_taken")]
        let every = false;
        let old = matches!(ctx.calib.chase_rescan_pass_over, ChaseRescanPassOver::Client15535RecedingOrBehind | ChaseRescanPassOver::Client16402RecedingOrBehindEveryRescan);
        behind && (every || (old && dropped.is_none() && !after_drop))
    }
}

/// targeting.CHASE_DROP_MEASURE: the measure of offset `d` (c - a) the chase-drop limit is compared with: max(|dx|,
/// |dy|) under max_abs, |dy| under client15535_lane_dy.
#[inline]
pub fn chase_measure(calib: &Calib, d: Vec2) -> i64 {
    match calib.chase_drop_measure {
        ChaseDropMeasure::MaxAbs => (d.x as i64).abs().max((d.y as i64).abs()),
        // PLANT (regression) chase_measure_max_abs: client15535_lane_dy still measures max(|dx|, |dy|).
        #[cfg(not(clash_plant = "chase_measure_max_abs"))]
        ChaseDropMeasure::Client15535LaneDy => (d.y as i64).abs(),
        #[cfg(clash_plant = "chase_measure_max_abs")]
        ChaseDropMeasure::Client15535LaneDy => (d.x as i64).abs().max((d.y as i64).abs()),
    }
}

/// `beyond_chase_limit` on the current positions, for a caller without a `TargetCtx`.
#[inline]
fn past_chase_limit(calib: &Calib, cards: &CardDb, e: &Entities, a: usize, c: usize) -> bool {
    let limit = cards.get(e.card[a]).sight_range as i64 + e.radius[a] as i64 + e.radius[c] as i64
        - (CHASE_DROP_SHORT_OF_SIGHT as i64) * (crate::fixed::SUBTILE_PER_MILLITILE as i64);
    chase_measure(calib, e.pos[c].sub(e.pos[a])) > limit
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
            && !past_chase_limit(calib, cards, e, a, c)
    })
}

/// targeting.LAUNCH_BEYOND_POSITION = client15535_after_move: is unit `a` a crown tower whose launch is judged on the next
/// tick's start positions (its flag set at every launch, `launched_out` reading it)?
#[inline]
pub fn launch_judged_after_move(calib: &Calib, e: &Entities, a: usize) -> bool {
    #[cfg(not(clash_plant = "launch_judged_on_launch_tick"))]
    let on = calib.launch_beyond_position == crate::state::LaunchBeyondPosition::Client15535AfterMove && e.kind[a].is_crown_tower();
    // PLANT (regression) launch_judged_on_launch_tick: the new arm still judges the launch on the launch tick's start.
    #[cfg(clash_plant = "launch_judged_on_launch_tick")]
    let on = {
        let _ = (calib, e, a);
        false
    };
    on
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

/// targeting.KNOCKED_TARGET_HOLD = client15535_sight_keep: does unit `a`, in a knockback (the fixed-distance slide or the
/// ladder, its own attack's recoil or a push; not a hook's drag), let its live target `t` go? It keeps a crown tower, and
/// any other target while the centre distance is within its sight toward it (`sight_toward`) + both radii +
/// LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET, on the start-of-tick positions (this is the Target phase). Measured on client
/// 15.535.29, units in a knock ladder holding a target that is not a crown tower: kept on every tick within that reach
/// (1,450 ticks, the largest +24 past round sight: a Musketeer pushed by a hero Bowler's boulder), let go on the first
/// tick past it, 9 of 9, at +70 to +170 past round sight (a Sparky in its own recoil: sp-il-8b9b t512, t989, t1079 and
/// sp-il-04cb t1926; an AxeMan pushed: sp-il-2c29 t1454; Musketeers pushed: sp-il-db5f t1700 and both
/// sp-form-Bowler-hero scenes t228). Each took the princess tower on the tick it let go. The rescan's own edge is plain
/// sight: the client's fresh acquisitions reach 0 past round sight and no further (1,153 troop and 261 building ones).
/// Always false under held_while_knocked, the engine's: a knocked unit keeps whatever it had.
fn knocked_lets_go(ctx: &TargetCtx, a: usize, ti: usize) -> bool {
    let e = ctx.ents;
    #[cfg(not(clash_plant = "knocked_target_held"))]
    let arm = ctx.calib.knocked_target_hold == KnockedTargetHold::Client15535SightKeep;
    #[cfg(clash_plant = "knocked_target_held")]
    let arm = false; // PLANT (regression): the new arm still keeps a knocked unit's target at any distance.
    arm && (e.knock_ms[a] > 0 || e.push_active[a])
        && !e.kind[ti].is_crown_tower()
        && !in_attack_range(ctx.calib, e.pos[a], sight_toward(ctx, a, ti) + ctx.calib.range_extension_to_keep_target, e.radius[a], e.pos[ti], e.radius[ti])
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
///
/// client16402_centre ranks by the EXACT centre distance, compared as its square: the integer root ties two enemies
/// whose distances differ by less than a subtile, and the client does not. 20260918-124946 tick 1923: a Tombstone's
/// Skeleton at (4151, 10363) comes off its post-kill wait between two Goblins 2,503.95 and 2,503.99 away, (2022, 9045)
/// and (4338, 7866); their subtile distances both root to 45,071, the next component (x in the attacker's frame)
/// picked the farther, and the client took the nearer.
#[inline]
fn key(ctx: &TargetCtx, a: usize, c: usize) -> (i64, i32, i32, u32) {
    let e = ctx.ents;
    let d2 = e.pos[a].dist2(e.pos[c]);
    let centre = isqrt(d2) as i32;
    #[cfg(not(clash_plant = "rank_centre_minus_radius"))]
    let by_centre = ctx.calib.target_rank_distance == crate::state::TargetRankDistance::Client16402Centre;
    #[cfg(clash_plant = "rank_centre_minus_radius")]
    let by_centre = false; // PLANT (regression): client16402_centre still ranks by centre minus the candidate's radius.
    let edge = if by_centre {
        d2
    } else if ctx.calib.add_character_range_to_radius {
        (centre - e.radius[c]) as i64
    } else {
        centre as i64
    };
    let f = ctx.arena.to_frame(e.team[a], e.pos[c]);
    #[cfg(clash_plant = "id_tiebreak")]
    {
        // PLANT: tie-break on raw slot index -- a deploy-order asymmetry.
        return (edge, 0, 0, c as u32);
    }
    // targeting.EQUAL_DISTANCE_TIE = client15535_later_created: at one distance, the later created first, in the
    // client's creation order (`client_creation_rank`).
    #[cfg(not(clash_plant = "tie_later_created_unread"))]
    if ctx.calib.equal_distance_tie == EqualDistanceTie::Client15535LaterCreated {
        return (edge, 0, 0, u32::MAX - client_creation_rank(ctx, c));
    }
    // targeting.EQUAL_DISTANCE_TIE: at one distance, the lower own-frame x first, or under own_frame_high_x the higher.
    #[cfg(not(clash_plant = "equal_distance_tie_low_x"))]
    let fx = if ctx.calib.equal_distance_tie == EqualDistanceTie::OwnFrameHighX { -f.x } else { f.x };
    #[cfg(clash_plant = "equal_distance_tie_low_x")]
    let fx = f.x; // PLANT (regression): the lower own-frame x first, whatever the arm.
    #[allow(unreachable_code)]
    (edge, fx, f.y, e.team_seq[c])
}

/// A candidate's place in client 15.535.29's creation order (targeting.EQUAL_DISTANCE_TIE = client15535_later_created):
/// its `team_seq`, except a side's princess towers, which the client creates the lower arena x first for both sides (the
/// captures' keys 1 and 2 Blue's (3500, 6500) and (14500, 6500), 4 and 5 Red's (3500, 25500) and (14500, 25500)), where
/// the engine creates each side's own-left one first so that team_seq is rotation-invariant (state.rs, the towers'
/// spawn): Red's are the other way round. Each side's king is its first (0), its princess towers 1 and 2, every unit
/// after them. sweep-GoblinDrill t328: a Blue Goblin made on x 9000 between Red's princess towers took the right one,
/// the client's later created; by team_seq the engine took the left.
#[inline]
fn client_creation_rank(ctx: &TargetCtx, c: usize) -> u32 {
    let e = ctx.ents;
    // PLANT (regression) tie_tower_order_engine: the princess towers ranked by the engine's own-left-first team_seq.
    #[cfg(not(clash_plant = "tie_tower_order_engine"))]
    if e.kind[c] == EntityKind::PrincessTower {
        return if e.pos[c].x < ctx.arena.width / 2 { 1 } else { 2 };
    }
    e.team_seq[c]
}

/// A candidate's rank in `scan_with`: (deprioritized, `key`), lowest first.
type ScanKey = (bool, (i64, i32, i32, u32));

/// Nearest valid enemy in sight, or None.
pub fn scan(ctx: &TargetCtx, a: usize, scratch: &mut Vec<u32>) -> Option<EntityId> {
    scan_with(ctx, a, scratch, ctx.ents.chase_dropped[a], false)
}

/// `scan`, with `dropped` the troop the chase drop let go of (targeting.CHASE_DROP_RANGE = client_sight_minus_1000):
/// it is a candidate only within the chase-drop limit, measured as the drop measures it, while every other enemy is
/// one at plain sight. Measured on client 15.535.29: a Tornado dragging a dropped Hog Rider back gets it taken again
/// on the first tick it is inside the drop limit, 3 of 3 runs, and not while it is back in plain sight only; a
/// fresh enemy farther than the dropped runner is taken on the drop tick. A per-target range, not a timed exclusion.
/// `after_drop`: this is the rescan of the drop's own tick (`decide`'s edge), where targeting.CHASE_DROP_WALKING_AWAY =
/// client15535_growing_away also passes over every other troop past the limit that walks away.
fn scan_with(ctx: &TargetCtx, a: usize, scratch: &mut Vec<u32>, dropped: Option<EntityId>, after_drop: bool) -> Option<EntityId> {
    let e = ctx.ents;
    let card = ctx.cards.get(e.card[a]);
    let extra = ctx.calib.extra_sight_range_to_crown_towers.max(0) + ctx.calib.extra_sight_range_to_building.max(0);
    // targeting.SCAN_REACH = client15535_plus_own_radius: the broad phase reaches the scanner's own radius too, so every
    // candidate the narrow test below admits is tested (the crown-tower band).
    // PLANT (regression) scan_reach_without_own_radius: the new arm's query still leaves the scanner's radius out.
    #[cfg(not(clash_plant = "scan_reach_without_own_radius"))]
    let own = if ctx.calib.scan_reach == ScanReach::Client15535PlusOwnRadius { e.radius[a] } else { 0 };
    #[cfg(clash_plant = "scan_reach_without_own_radius")]
    let own = 0;
    let query = card.sight_range + extra + ctx.hash.max_radius() + own;
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
        // A buildings-only walker ignores a player building too far across in x (targeting.BUILDING_SCAN_X_CUT): under
        // centre_6750 more than BUILDING_SCAN_DX centre to centre; under client16402_edge_6700_melee a walker that attacks
        // with a projectile has no cut, and a melee one ignores it past BUILDING_SCAN_EDGE_DX of |dx| + its radius - the
        // building's.
        #[cfg(not(clash_plant = "building_scan_dx_unbounded"))]
        if card.target_only_buildings && e.kind[c] == EntityKind::Building && {
            let dx = (e.pos[c].x - e.pos[a].x).abs();
            let k = crate::fixed::SUBTILE_PER_MILLITILE;
            #[cfg(not(clash_plant = "building_scan_cut_centre"))]
            let edge = ctx.calib.building_scan_x_cut == crate::state::BuildingScanXCut::Client16402Edge6700Melee;
            #[cfg(clash_plant = "building_scan_cut_centre")]
            let edge = false; // PLANT (regression): the new arm still cuts every walker at 6750 centre to centre.
            // client_sight_clip_side: the walker's own SightClipSide, |dx| past SightRange - it + both radii.
            if ctx.calib.building_scan_x_cut == crate::state::BuildingScanXCut::ClientSightClipSide {
                card.sight_clip_side.is_some_and(|cs| dx > card.sight_range - cs + e.radius[a] + e.radius[c])
            } else if edge {
                card.projectile.is_none() && dx + e.radius[a] - e.radius[c] > BUILDING_SCAN_EDGE_DX * k
            } else {
                dx > BUILDING_SCAN_DX * k
            }
        } {
            continue;
        }
        // targeting.MINIMUM_RANGE = client16402_edge_distance: never TAKE a target inside the minimum range.
        if inside_minimum_range(ctx, a, c) {
            continue;
        }
        // targeting.CHASE_DROP_WALKING_AWAY: under any_target the troop just let go; under client_walking_away every
        // troop past the limit that walks away (`walks_away`), let go or never held; under client15535_growing_away
        // the troop let go, whatever it does, and in the drop tick's own rescan every other that walks away (client
        // 15.535.29: sweep-RoyalHogs t353, the Knight letting Hog 7 go passed over Hog 8, 6,071 -> 6,245 and walking
        // away; ub-sd14-a2 t307, a Knight's first pick out of its deploy, took a Knight walking away 5,665 -> 5,725).
        #[cfg(not(clash_plant = "chase_drop_rescan_admits"))]
        if (match ctx.calib.chase_drop_walking_away {
            ChaseDropWalkingAway::AnyTarget => dropped == Some(e.id_of(c)),
            ChaseDropWalkingAway::ClientWalkingAway => walks_away(ctx, a, c),
            // PLANT (regression) chase_pass_over_every_rescan: every rescan passes over a troop walking away.
            #[cfg(not(clash_plant = "chase_pass_over_every_rescan"))]
            ChaseDropWalkingAway::Client15535GrowingAway => {
                dropped == Some(e.id_of(c)) || (after_drop && walks_away(ctx, a, c)) || recedes_from_lane_walk(ctx, a, c) || behind_on_rescan(ctx, a, c, dropped, after_drop)
            }
            #[cfg(clash_plant = "chase_pass_over_every_rescan")]
            ChaseDropWalkingAway::Client15535GrowingAway => {
                dropped == Some(e.id_of(c)) || walks_away(ctx, a, c) || recedes_from_lane_walk(ctx, a, c) || behind_on_rescan(ctx, a, c, dropped, after_drop)
            }
        }) && chase_drop_applies(ctx, a, c)
            && beyond_chase_limit(ctx, a, c)
        {
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

/// targeting.PROJECTILE_HOLD_SCOPE: does projectile attacker `a` hold its target past its keep reach on this tick, to
/// Range + both radii + PROJECTILE_HOLD_BEYOND_REACH (`decide`, under targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED =
/// "projectile_attackers_only")? Under every_tick (today's engine), always. Under client_troop_in_attack, a building
/// and a crown tower always, and a TROOP only while it is in its attack: its attack phase, which the previous tick's
/// Attack phase left, stands it (`Calib::attack_holds`). A walking troop then keeps its target only within Range + both
/// radii + LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET, as a direct striker does, and rescans past it.
///
/// Measured on the 16.402 corpus (one seat per battle, 40 battles): a projectile troop walking to a target that
/// stands past its keep reach and inside the hold, with a valid enemy nearer by centre in sight, took the nearer
/// enemy on the tick in 8 of 8 samples (7 events: an Archer or a Bomber walking to a Goblin Hut takes a wave member on
/// its 8th frame, 4 times; 20260918-112751 tick 1590 is one); in its attack it kept the target on 13 of 14, and the
/// 14th had launched at it from beyond its reach on the tick before. Client 15.535.29: a Mega Minion walking to a
/// Goblin Hut 287.8 past its reach takes a wave member on the member's 8th frame; attackers in their attack kept the
/// target on 118 of 120 ticks inside the hold with a nearer enemy in sight (Musketeers and Minions against a leaving
/// Hog Rider and a nearer Cannon, most of them; 1 took the Cannon after a launch beyond reach, 1 Ram Rider dropped to
/// no target).
#[inline]
fn holds_past_reach(ctx: &TargetCtx, a: usize) -> bool {
    #[cfg(not(clash_plant = "projectile_hold_while_walking"))]
    let scoped = ctx.calib.projectile_hold_scope == crate::state::ProjectileHoldScope::ClientTroopInAttack;
    #[cfg(clash_plant = "projectile_hold_while_walking")]
    let scoped = {
        let _ = ctx.calib.projectile_hold_scope;
        false // PLANT (regression): client_troop_in_attack still holds a walking troop's target past its keep reach.
    };
    let e = ctx.ents;
    !scoped || e.kind[a] != EntityKind::Troop || ctx.calib.attack_holds(e.attack_phase[a])
}

/// targeting.WALKING_KEEP_REACH: the attacker radius `decide`'s keep tests add for holder `a`. own_radius (the engine's):
/// its CollisionRadius, walking or standing. client15535_walking_reach: a holder that WALKS (`walking_now`: not in its
/// attack, not held) adds the radius it walks to (`walking_own_radius`: none for a VariableDamage2 or combo row under
/// targeting.VARIABLE_DAMAGE_WALK_REACH), so it keeps its target within Range + LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET + the
/// target's radius and past that rescans; a standing holder adds its own radius under both arms (the standing Inferno
/// Dragon's measured hold). Measured on client 15.535.29 (keep_band_truth.py: every walking holder whose target stood
/// past Range + 25 + the target's radius and within that + its own radius, with another enemy it could take strictly
/// nearer): sp-form-InfernoDragon-evo-s0 t1094, the one Inferno Dragon, let a Skeleton at 4,299 go for one at 4,283
/// (band 4,025 .. 4,525), where the engine kept the first and walked on; every holder of another row kept, 258 of 258
/// (68 of 68 on the 16.402 corpus), as `walking_own_radius` leaves their own radius in.
fn keep_own_radius(ctx: &TargetCtx, a: usize, card: &CardDef) -> i32 {
    let e = ctx.ents;
    // PLANT (regression) walking_keep_reach_own_radius: the new arm's walking holder still keeps to both radii.
    #[cfg(not(clash_plant = "walking_keep_reach_own_radius"))]
    // walking: not in its attack and no swing under way (a standing attacker between swings reads attack_ms > 0)
    let walking = ctx.calib.walking_keep_reach == WalkingKeepReach::Client15535WalkingReach && walking_now(e, a) && e.attack_ms[a] == 0;
    #[cfg(clash_plant = "walking_keep_reach_own_radius")]
    let walking = false;
    if walking {
        walking_own_radius(ctx.calib, card, e.radius[a])
    } else {
        e.radius[a]
    }
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
        let held = cur.filter(|t| e.is_alive(*t));
        // targeting.KNOCKED_TARGET_HOLD = client15535_sight_keep: past its keep test a knocked unit lets go and rescans.
        if e.stun_ms[a] == 0 && !sliding && held.is_some_and(|t| knocked_lets_go(ctx, a, t.index as usize)) {
            return TargetDecision { target: scan(ctx, a, scratch), cancel_attack: false, resumed: false, chase_dropped: None };
        }
        // targeting.KNOCKED_LOST_TARGET = client15535_rescans: a unit on a ladder (not stunned, hooked or sliding) holding
        // no live target decides below as a unit off the ladder would (the post-kill wait, then the scan); measured on
        // client 15.535.29, 28 of 28 ladders whose unit's target was gone took a new one before the ladder ended.
        // PLANT (regression) knocked_lost_target_held: the new arm still holds none to the ladder's end.
        #[cfg(not(clash_plant = "knocked_lost_target_held"))]
        let rescans = ctx.calib.knocked_lost_target == KnockedLostTarget::Client15535Rescans;
        #[cfg(clash_plant = "knocked_lost_target_held")]
        let rescans = false;
        let on_ladder = e.stun_ms[a] == 0 && !sliding && e.hooked_by[a].is_none() && (e.push_active[a] || e.knock_ms[a] > 0);
        if !(rescans && on_ladder && held.is_none()) {
            return TargetDecision { target: held, cancel_attack: false, resumed: false, chase_dropped: None };
        }
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
    // The target the rescan below falls back to when it finds nothing in sight (`held_past_sight`).
    let mut held_past_sight: Option<EntityId> = None;
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
                // targeting.PROJECTILE_HOLD_SCOPE = client_troop_in_attack: a troop that is not in its attack (it
                // walks) holds nothing past its keep reach (`holds_past_reach`) and falls to the rescan below.
                let reach_past = if holds_past_reach(ctx, a) {
                    PROJECTILE_HOLD_BEYOND_REACH * crate::fixed::SUBTILE_PER_MILLITILE
                } else {
                    ctx.calib.range_extension_to_keep_target
                };
                let hold = card.range + reach_past;
                // targeting.LAUNCH_BEYOND_POSITION = client15535_after_move: a crown tower's launch flag (set at every
                // launch) is judged here, on this tick's start positions -- the target where the launch tick's move left
                // it -- against the tower's reach; beyond it, the hold ends.
                let launched_out = e.launched_beyond[a]
                    && (!launch_judged_after_move(ctx.calib, e, a) || !in_attack_range(ctx.calib, e.pos[a], card.range, e.radius[a], e.pos[ti], e.radius[ti]));
                if !launched_out && in_attack_range(ctx.calib, e.pos[a], hold, keep_own_radius(ctx, a, card), e.pos[ti], e.radius[ti]) {
                    return TargetDecision { target: Some(t), cancel_attack: false, resumed: false, chase_dropped: None };
                }
                // targeting.LAUNCH_BEYOND_KEEP = client15535_plain_keep: a troop's launch beyond its reach ends the hold past
                // reach, not the plain keep (Range + both radii + LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET).
                // PLANT (regression) launch_beyond_drops_plain_keep: the new arm still rescans past the reach.
                #[cfg(not(clash_plant = "launch_beyond_drops_plain_keep"))]
                let plain = ctx.calib.launch_beyond_keep == crate::state::LaunchBeyondKeep::Client15535PlainKeep;
                #[cfg(clash_plant = "launch_beyond_drops_plain_keep")]
                let plain = false;
                if launched_out
                    && plain
                    && e.kind[a] == EntityKind::Troop
                    && in_attack_range(ctx.calib, e.pos[a], card.range + ctx.calib.range_extension_to_keep_target, keep_own_radius(ctx, a, card), e.pos[ti], e.radius[ti])
                {
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
                // targeting.WALKING_KEEP_REACH: a walking holder's radius is the one it walks to (`keep_own_radius`).
                let keep = card.range + ctx.calib.range_extension_to_keep_target;
                if in_attack_range(ctx.calib, e.pos[a], keep, keep_own_radius(ctx, a, card), e.pos[ti], e.radius[ti]) {
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
            //
            // targeting.CHASE_DROP_KNOCKED_TARGET = client_holds_knocked: a troop target SLIDING under a knockback (the
            // slide or the ladder; not a hook's drag), still in sight, is held through the slide: neither the chase
            // drop nor the rescan below lets it go, as the early return above holds an attacker's own target while the
            // attacker slides. After the slide it stands where the push left it; past the limit it has not been inside
            // since (the Target phase records `chase_inside` only within the limit), so it is walked after and
            // rescanned as any troop taken past the limit. Measured on the 16.402 corpus: 3 of 3 pushes that carried a
            // held troop target across the limit kept it, all three by the holder's own boulder (a Bowler; a Bomber
            // twice, attacking, and a Knight, walking). A push by anything else is inferred.
            #[cfg(not(clash_plant = "chase_drop_knocked_dropped"))]
            let holds_sliding = ctx.calib.chase_drop_knocked != crate::state::ChaseDropKnocked::DropsKnocked
                && chase_drop_applies(ctx, a, ti)
                && (e.knock_ms[ti] > 0 || e.push_active[ti]);
            #[cfg(clash_plant = "chase_drop_knocked_dropped")]
            let holds_sliding = false; // PLANT (regression): client_holds_knocked still lets a sliding target go.
            if holds_sliding && in_attack_range(ctx.calib, e.pos[a], sight_toward(ctx, a, ti), e.radius[a], e.pos[ti], e.radius[ti]) {
                // targeting.CHASE_DROP_KNOCKED_TARGET = client15535_holds_unless_nearer: the rescan still takes an enemy it
                // ranks before the sliding target (`scan`'s own key), which the hold does not shield.
                // PLANT (regression) knocked_hold_ignores_nearer: the new arm holds against a nearer enemy too.
                #[cfg(not(clash_plant = "knocked_hold_ignores_nearer"))]
                let rescans = ctx.calib.chase_drop_knocked == crate::state::ChaseDropKnocked::Client15535HoldsUnlessNearer;
                #[cfg(clash_plant = "knocked_hold_ignores_nearer")]
                let rescans = false;
                if rescans {
                    if let Some(n) = scan(ctx, a, scratch).filter(|n| *n != t) {
                        let ni = n.index as usize;
                        if (deprioritized(ctx, a, ni), key(ctx, a, ni)) < (deprioritized(ctx, a, ti), key(ctx, a, ti)) {
                            return TargetDecision { target: Some(n), cancel_attack: cancel, resumed: false, chase_dropped: None };
                        }
                    }
                }
                return TargetDecision { target: Some(t), cancel_attack: cancel, resumed: false, chase_dropped: None };
            }
            #[cfg(not(any(clash_plant = "chase_drop_ignored", clash_plant = "chase_drop_level_triggered")))]
            let dropped = chase_drop_applies(ctx, a, ti) && e.chase_inside[a] == Some(t) && beyond_chase_limit(ctx, a, ti) && walks_away(ctx, a, ti);
            #[cfg(all(clash_plant = "chase_drop_level_triggered", not(clash_plant = "chase_drop_ignored")))]
            let dropped = chase_drop_applies(ctx, a, ti) && beyond_chase_limit(ctx, a, ti); // PLANT (regression): a target taken past the limit is let go on the next tick.
            #[cfg(clash_plant = "chase_drop_ignored")]
            let dropped = false; // PLANT (regression): the chaser keeps the runner to its plain sight, as today.
            if dropped {
                return TargetDecision { target: scan_with(ctx, a, scratch, Some(t), true), cancel_attack: cancel, resumed: false, chase_dropped: Some(t) };
            }
            // THE CHASE HOLDS A TARGET PAST ROUND SIGHT (targeting.CHASE_DROP_RANGE = client_sight_minus_1000): a target
            // past the attacker's round sight is kept when the rescan finds nothing in sight; a unit the rescan finds is
            // nearer and taken as before. The chase drop above, which lets go of a target that has been inside the limit
            // and walks past it, returns first. Measured on client 15.535.29, walkers holding a troop target past round
            // sight: kept 158 of 159 inside the limit and 135 of 139 past it when the pair had never been inside, and 25
            // of 25 building targets (sp-champ-SkeletonKing-s0 t177: a Skeleton chasing the Skeleton King at 7,021, round
            // sight 7,000; sp-champ-LittlePrince-s0 t190: a Skeleton holding the Little Prince at 6,508 round, 6,019
            // square against a limit of 5,500, never inside). A target still in round sight is not held: the rescan
            // passes over one on purpose (a doomed one: sweep-Minions t336, a Knight whose death the Minion's shot in
            // flight has settled, left for the tower on both clients). Past the limit the hold is the client's
            // (targeting.CHASE_HOLD_PAST_LIMIT): inside_only lets it go there (the 16.402 corpus, 3 of 4);
            // client15535_troops_kept keeps it for a troop (client 15.535.29, 135 of 139), and a building (a crown tower)
            // still lets go (tests/reach_loss_switch.rs `a_crown_tower_drops_a_started_shot_500_past_its_reach`).
            #[cfg(not(clash_plant = "chase_lost_past_round_sight"))]
            if ctx.calib.chase_drop_range == ChaseDropRange::ClientSightMinus1000
                && !in_attack_range(ctx.calib, e.pos[a], sight_toward(ctx, a, ti), e.radius[a], e.pos[ti], e.radius[ti])
            {
                // PLANT (regression) chase_held_only_inside_limit: the hold stops at the chase-drop limit, as r24 had it.
                #[cfg(clash_plant = "chase_held_only_inside_limit")]
                let holds = !past_chase_limit(ctx.calib, ctx.cards, e, a, ti);
                #[cfg(not(clash_plant = "chase_held_only_inside_limit"))]
                let holds = !past_chase_limit(ctx.calib, ctx.cards, e, a, ti)
                    || (ctx.calib.chase_hold_past_limit == ChaseHoldPastLimit::Client15535TroopsKept && !e.kind[a].is_building());
                // targeting.CHASE_HOLD_SCOPE = client15535_walkers_only: a holder still in its attack (its projectile hold
                // just ended by a launch from beyond reach) lets the target go: client 15.535.29, 16 of 19 such holders
                // took a tower (sp-form-RoyalHogs-evo-s0 t1268: a Musketeer's shot at an Evo Royal Hog 7,263 away).
                // PLANT (regression) chase_hold_while_attacking: the new arm still holds for a holder in its attack.
                #[cfg(not(clash_plant = "chase_hold_while_attacking"))]
                // in its attack: not walking, or a swing under way (between swings the phase is idle, attack_ms > 0)
                let attacking = ctx.calib.chase_hold_scope == ChaseHoldScope::Client15535WalkersOnly && (!walking_now(e, a) || e.attack_ms[a] > 0);
                #[cfg(clash_plant = "chase_hold_while_attacking")]
                let attacking = false;
                // targeting.SLAP_FLIGHT_SIGHT_HOLD = client15535_let_go: a target in a Hero Giant's slap flight (`slap_air`) is
                // not held past round sight: client 15.535.29, 4 of 4 Bats chasing a thrown Ice Golemite let it go on the
                // first tick past SightRange + both radii (sp-il-04cb t1885 to t1899).
                // PLANT (regression) slap_flight_held_past_sight: the new arm still holds a thrown target past sight.
                #[cfg(not(clash_plant = "slap_flight_held_past_sight"))]
                let thrown_let_go = ctx.calib.slap_flight_sight_hold == SlapFlightSightHold::Client15535LetGo && ctx.slap_air.get(ti).copied().unwrap_or(false);
                #[cfg(clash_plant = "slap_flight_held_past_sight")]
                let thrown_let_go = false;
                if holds && !attacking && !thrown_let_go {
                    held_past_sight = Some(t);
                }
            }
        } else if (e.target_locked[a] && !dropped_for_doom(ctx, a, t)) || too_close {
            cancel = true;
        }
    }
    TargetDecision { target: scan(ctx, a, scratch).or(held_past_sight), cancel_attack: cancel, resumed: false, chase_dropped: None }
}

/// Where a unit with no target walks: the enemy crown tower chosen by x
/// (LOGIC_XPOS_BASED_TOWER_TARGETING), falling back to the king once that lane's
/// princess tower is down (or, under targeting.DOOMED_LANE_TOWER's new arms, doomed: `lane_fallen`). With the global
/// off, the nearest enemy crown tower.
pub fn default_tower(ctx: &TargetCtx, a: usize) -> Option<EntityId> {
    let e = ctx.ents;
    let enemy = e.team[a].other() as usize;
    // targeting.DOOMED_LANE_TOWER: under its new arms a princess tower the shots in flight doom is fallen to the units
    // `lane_fallen` names, in every pick below, as a destroyed one is (never under standing, never the king).
    // An enemy tower its own side holds by a charm (status.rs `BuffDef::switch_team`) is out of the picks while it is
    // held, as a fallen one is. Measured on client 15.535.29 (sp-event-SuperEliteArcher-s0): the charmed red Knight
    // walked for the red king on the tick the right princess tower was charmed too (t286), and again from t398.
    #[cfg(not(clash_plant = "default_tower_ignores_charm"))]
    let held = |id: EntityId| e.team[id.index as usize] == e.team[a];
    #[cfg(clash_plant = "default_tower_ignores_charm")]
    let held = |_: EntityId| false; // PLANT: a charmed enemy tower stays a pick.
    let live = |t: Option<EntityId>| t.filter(|id| e.is_alive(*id) && !lane_fallen(ctx, a, *id) && !held(*id));
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
        // targeting.FALLEN_LANE_TOWER_PICK = client16402_spawn_lane_king: after the window, a troop whose SPAWN lane's
        // enemy princess tower is down walks to the king, wherever it stands; one whose spawn-lane tower stands takes
        // the tower of its current x below, as today. Read under both of FIRST_TOWER_PICK's spawn-lane arms, the ones
        // that keep `spawn_lane`. Measured on the 16.402 corpus (one seat per battle): 5 default picks, by 5 troops in
        // 3 battles, made with one enemy princess tower down while the troop stood on the other side of the centre
        // from its spawn lane, took the king, 0 the princess tower of their x (20260920-082459 tick 2635: an Inferno
        // Dragon created at x 8500, at x 9460 after a chase, flew to the king past a standing right princess tower).
        // No client 15.535.29 run separates the rules (10 default picks with one tower down, all where they agree).
        #[cfg(not(clash_plant = "fallen_lane_by_x"))]
        let fallen_lane_king = ctx.calib.fallen_lane_tower_pick == crate::state::FallenLaneTowerPick::Client16402SpawnLaneKing;
        #[cfg(clash_plant = "fallen_lane_by_x")]
        let fallen_lane_king = false; // PLANT (regression): the new arm still takes the tower of the current x.
        if fallen_lane_king && ctx.calib.first_tower_pick.spawn_lane() && e.spawn_lane[a] != 0 {
            let lane = if e.spawn_lane[a] & ctx.arena.bit_lane_left != 0 { Lane::Left } else { Lane::Right };
            if live(ctx.towers[enemy][1 + lane as usize]).is_none() && live(ctx.towers[enemy][0]).is_some() {
                return live(ctx.towers[enemy][0]);
            }
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

    let mut best: Option<((i64, i32, i32, u32), EntityId)> = None;
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
