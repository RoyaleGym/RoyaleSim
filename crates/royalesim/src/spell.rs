//! Spells in flight and on the ground: fixed-target projectiles (Fireball, Arrows,
//! Goblin Barrel), one-shot area effects (Zap), rolling projectiles (The Log), and the
//! two effects they share -- KNOCKBACK and STUN.
//!
//! SPEC: docs/spell-spec.md (PARTIALLY VERIFIED; its header explains the vintage
//! trap). Every card number comes from data/derived/cards.json through card.rs
//! `SpellDef`; every mechanic no column settles is a calibration.json key (spells.*,
//! knockback.*, status.*), read into `Calib`. Nothing here types a card stat.
//!
//! LIFECYCLE (lib.rs TICK_PHASES)
//!     deploy      validated like a troop (state.rs check_position), elixir paid,
//!                 card cycled, a PendingSpawn queued -- so a cast has exactly a
//!                 troop's latency and both seats' same-tick casts materialise in
//!                 the same Spawn phase.
//!     Spawn       `cast` turns it into `Spell` objects (one per Arrows wave).
//!     Reap        a death that carries card.rs `death_area_effect` (the Ice Golem's)
//!                 calls the SAME `cast` at the death point, under the dying card's
//!                 index and level, and its area applies in the next tick's
//!                 Projectile phase -- the same tick the death damage the same death
//!                 buffered is resolved, because Reap writes both after Resolve. A death
//!                 that carries `death_projectile` (the Phoenix's) stands that projectile
//!                 on the death point (`death_projectile`) under
//!                 spawner.DEATH_SPAWN_PROJECTILE, and it lands in that same next phase.
//!     Projectile  `step_spells`, after troop projectiles: flights advance, rolls
//!                 roll, area effects apply. Every hit test reads entity positions
//!                 AS THEY ARE ON THE ARRIVAL TICK (post-Move), never at cast. Hits go
//!                 to the DamageBuffer; knockbacks and stuns to their own buffers;
//!                 released units to the deferred spawn queue.
//!     Resolve     damage first; then (state.rs) stun timers tick, new stuns merge by
//!                 max, knockbacks sum per unit and move the survivors (the
//!                 fixed_distance arm) or ARM THE LADDER on them (the shipped
//!                 client16402 arm: a `Knock::Push` carries the source point
//!                 and the strength; move16402.rs `start_pushback` aims the target and
//!                 `phase_path16402` walks the 25n ladder from the next tick on).
//!     The one-shot area effect runs INSIDE the Projectile phase rather than in a new
//!     Phase::AreaEffect (the spec's aoe.PHASE_POSITION "after projectile, before
//!     resolve"): the observable order is identical, because nothing in a phase reads
//!     another writer's buffer before Resolve, and TICK_PHASES stays unchanged.
//!
//! ORDER INDEPENDENCE AND THE SEAT ROTATION, BY CONSTRUCTION
//!     * A spell reads only entity state (which does not change during the phase) and
//!       its own record, and writes only to buffers. So `spells` order cannot matter.
//!     * Knockbacks are SUMMED per unit and stuns MERGED BY MAX before anything moves
//!       (calibration knockback.STACKING = vector_sum, status.SAME_BUFF_REAPPLY); both
//!       commute. UNDER THE LADDER they do not sum: a push is refused while a ladder
//!       runs (STACKING = first_wins_while_active), so the FIRST buffered push of a
//!       tick lands and the rest are refused -- buffer order is cast order.
//!     * Directions are world-frame RELATIVE vectors scaled by integer division that
//!       truncates toward zero -- odd-symmetric, so the rotation of a push is the push
//!       of the rotation. Every fallback direction is the CASTER's forward axis (+y
//!       for Blue, -y for Red), never a fixed engine axis.
//!     * Flights advance with path::advance in world coordinates, whose sub-subtile
//!       carry flips sign under the rotation (tests/common canon_team knows).
//!     * Water ejection breaks ties in the pushed unit's own frame
//!       (Arena::nearest_passable_ground).
#![allow(unexpected_cfgs)]

use crate::arena::{Arena, Rect, Shape};
use crate::card::{CardDb, KnockbackDef, SpawnDef, SpellHit, SpellShape, StrikeDef};
use crate::status::{BuffApply, BuffHit, Pulse};
use crate::combat::{damage_against, DamageBuffer, Hit, Projectile};
use crate::entity::{EntityKind, Entities, SpatialHash};
use crate::fixed::{in_range_edge, isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use crate::path::{advance, Obstacle};
use crate::state::{AoeHitTest, AreaBuffSourceBinding, Calib, ChildAreaBirth, KnockLaw, KnockZeroVector, LaunchModel, OwnSideScope, PulsingArea, RollDirection, RollFirstStep, RollHitShape, StrikeAreaEnd, StrikeHpRank, StrikeLeftover, StrikeReach, SummonFuseStart, TargetBuffScope};
use crate::{EntityId, Team};

/// Where a spell is in its life.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SpellMotion {
    /// A fixed-target projectile. `delay_ms` > 0: a later wave not yet released.
    Flight { pos: Vec2, aim: Vec2, frac: Vec2, delay_ms: i32 },
    /// The Log's airborne phase: no hitbox. On arrival it becomes `Rolling` at
    /// `roll_start` with `roll_len` to go.
    Airborne { pos: Vec2, aim: Vec2, frac: Vec2, roll_start: Vec2, roll_len: i32 },
    /// A rolling projectile moving along its caster's forward axis. `hit` holds every
    /// entity it has already struck (each at most once), sorted.
    Rolling { pos: Vec2, travelled: i32, len: i32, hit: Vec<EntityId> },
    /// A one-shot area effect at `pos`, applied on its first update.
    Area { pos: Vec2 },
    /// A PULSING area effect standing at `pos` (Poison, Earthquake): it applies its
    /// hit every HitSpeed ms until its life runs out. `Pulse` carries both clocks.
    Pulsing(Pulse),
    /// A bottle standing out its fuse (`SpellShape::Fuse`): `ms` of it left.
    Fuse { pos: Vec2, ms: i32 },
    /// A STRIKING AREA at `pos` (`SpellShape::Strikes`; Lightning): `next_ms` to its next strike on the clock
    /// spells.STRIKE_TIMER_LEFTOVER names, `k` strikes done, `struck` every enemy it has struck, sorted (each at
    /// most once).
    Strikes { pos: Vec2, life_ms: i32, next_ms: i32, k: u8, struck: Vec<EntityId> },
}

/// One live spell object.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Spell {
    pub team: Team,
    /// The spell card's CardDb index.
    pub card: u16,
    /// Unified level it was cast at.
    pub level: i32,
    /// Level-scaled damage per victim (0 when it deals none).
    pub damage: i32,
    /// The level-scaled amount of ONE pulse of this spell's buff, positive for damage
    /// and negative for a heal (`BuffDef::pulse_base` through `CardDb::scaled`); 0
    /// when the spell carries no pulsing buff. Computed at cast, because the victim
    /// does not know the caster's level.
    pub pulse: i32,
    pub motion: SpellMotion,
    /// Which shape of the card's chain this object runs: 0 = the card's own shape (`shape_of`),
    /// k = k steps down `SpellShape::child`. A Rage cast is depth 0 (the bottle), its buff area
    /// depth 1, the damage area depth 2.
    #[serde(default)]
    pub depth: u8,
}

/// A unit a landing spell releases, for the deferred spawn queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Release {
    pub team: Team,
    pub unit: u16,
    pub level: i32,
    pub pos: Vec2,
    pub deploy_ms: Option<i32>,
    pub count: i32,
}

/// An area effect a landing object leaves where it lands: cast at `pos` under `card`'s
/// index and unified `level` (`cast`, so `shape_of(card)` names the area), as a death's
/// area effect is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AreaRelease {
    pub team: Team,
    pub card: u16,
    pub level: i32,
    pub pos: Vec2,
}

/// A container whose fuse ran out at `pos`: an order for the death spawn of `card`
/// (at unified `level`) to come out there. Nothing carries it out yet (`SpellOut`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuseEnd {
    pub team: Team,
    pub card: u16,
    pub level: i32,
    pub pos: Vec2,
}

/// A unit a spell copies: an order to copy `src` at the spell's unified `level`.
/// Nothing carries it out yet (`SpellOut`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloneOrder {
    pub src: EntityId,
    pub level: i32,
}

/// EVERYTHING ONE PROJECTILE PHASE HANDS ON to the rest of the tick, in one bundle so
/// a new kind of output is one field and not one more parameter on `step_spells`.
/// Nothing in it outlives the tick, so none of it is in a snapshot or in the state
/// hash. Only `released` has a writer today (`step_spells`); the other fields stay
/// empty. state.rs `phase_projectile` drains four of them before the phase ends, in
/// this order, and every object appended in steps 2-4 first acts on the next tick,
/// because this tick's steps have already run:
///   1. `released`: units a landing spell releases (laid out, then `release`);
///   2. `born`: spell objects a spell object makes (appended to the spell list);
///   3. `areas`: area effects a landing object leaves (cast, appended after 2's);
///   4. `launched`: projectiles a spell object fires (appended to the projectile list).
///
/// `fuse_ends` and `clones` have no consumer yet: `phase_projectile` stops the battle
/// if either is non-empty, so an order can never be dropped in silence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpellOut {
    pub released: Vec<Release>,
    pub born: Vec<Spell>,
    pub launched: Vec<Projectile>,
    pub areas: Vec<AreaRelease>,
    pub clones: Vec<CloneOrder>,
    pub fuse_ends: Vec<FuseEnd>,
}

/// One buffered knockback. Which variant a spell writes is calibration
/// knockback.DISPLACEMENT_LAW (`impact` / `roll`); state.rs `apply_effects` consumes
/// it in Resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Knock {
    /// fixed_distance: a displacement, WORLD subtiles, summed per unit.
    Displacement(EntityId, Vec2),
    /// client16402: the push as the ladder's arming receives it --
    /// the source point in NATIVE units (the impact centre; for the Log, the point
    /// knockback.DIRECTION_ROLLING names: under the shipped radial_from_projectile_centre
    /// the Log's centre where it first touched the victim, under travel_direction the
    /// point one unit behind the victim on the roll axis, so the source-to-victim line
    /// IS the travel direction), the strength (Pushback, native) and the caster (the
    /// forward axis of the zero-vector fallback).
    Push { id: EntityId, src: Vec2, strength: i32, caster: Team },
}

impl Knock {
    #[inline]
    pub fn id(&self) -> EntityId {
        match *self {
            Knock::Displacement(id, _) => id,
            Knock::Push { id, .. } => id,
        }
    }
}

/// Everything a tick of spells produced, applied later by state.rs.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EffectBuffer {
    /// Knockbacks, in buffer order (spell order = cast order).
    pub knocks: Vec<Knock>,
    /// Stun durations, ms; merged by max per unit in Resolve. Written by nothing
    /// but a test: every stun the data ships is a -100 BUFF
    /// (`buffs` below), and `state.rs apply_effects` derives the timer from it under
    /// status.FULL_STOP_BUFF_IS_STUN. Kept because the merge it documents is the one
    /// the buff path reuses and because a caller may still buffer a bare stun.
    pub stuns: Vec<(EntityId, i32)>,
    /// Buff applications, in buffer order (spell order = cast order). Drained in
    /// Resolve by `state.rs apply_effects`.
    pub buffs: Vec<BuffHit>,
    /// Hooks that landed this tick, (victim, thrower) (calibration combat.SPECIAL_HOOK =
    /// client_hook_drag; combat.rs `step_projectiles`). Drained in Resolve by `state.rs
    /// apply_effects`, which starts the drag on a surviving victim. Empty under the shipped
    /// not_read. `default` so a snapshot saved before it still loads.
    #[serde(default)]
    pub hooks: Vec<(EntityId, EntityId)>,
}

/// The caster's forward axis: +1 for Blue (toward high y), -1 for Red.
#[inline]
pub fn forward_dy(team: Team) -> i32 {
    -Arena::own_side_dy(team)
}

/// The area effect / projectile / roll a card index runs: a SPELL card's own shape,
/// or -- for a troop or building whose DEATH leaves an area effect standing (card.rs
/// `death_area_effect`, the Ice Golem's) -- that block; or -- for a troop whose card
/// carries a DEPLOY PROJECTILE (card.rs `deploy_projectile`, the Mega Knight's) -- that
/// impact; or the projectile a death releases (`death_projectile`, the Phoenix's), the
/// area a card IS when it spawns its character (`deploy_area_effect`, the Electro
/// Wizard's) or the area a unit puts down where it appears (`spawn_area_effect`, the
/// Battle Healer's). A card carries at most one of the six: `convert_spell` reads no death
/// or deploy column, `convert` builds no spell, `convert` refuses a unit with both a
/// deploy projectile and a death area effect, and the loader refuses a card that would
/// carry two of the others. Both `cast` and `step_spells` resolve a `Spell`'s card this
/// way, so a death release, a deploy blow and every other release are the SAME object,
/// the same `impact` and the same phase a Zap gets.
///
/// CRATE-VISIBLE because the answer is also the snapshot's: state.rs `load_with`
/// refuses a saved `Spell` whose card runs no shape, and it has to ask the question
/// the same way the step does. Asking it as `def.spell.is_some()` refused every
/// battle saved on the one tick a death release is in the air.
#[inline]
pub(crate) fn shape_of(def: &crate::card::CardDef) -> Option<&crate::card::SpellDef> {
    def.spell
        .as_ref()
        .or(def.death_area_effect.as_ref())
        .or(def.deploy_projectile.as_ref())
        .or(def.death_projectile.as_ref())
        .or(def.deploy_area_effect.as_ref())
        .or(def.spawn_area_effect.as_ref())
        .or(def.projectile_area.as_ref())
}

/// The shape `depth` steps down `root`'s chain (`SpellShape::child`), or None past its end.
pub(crate) fn shape_at(root: &SpellShape, depth: u8) -> Option<&SpellShape> {
    let mut s = root;
    for _ in 0..depth {
        s = s.child()?;
    }
    Some(s)
}

/// THE PROJECTILE A DEATH LEAVES (card.rs `death_projectile`; spawner.DEATH_SPAWN_PROJECTILE =
/// client_projectile; the Phoenix's PhoenixFireball): ONE `Flight` object standing on the death
/// point `at` and aimed at it with no delay, under the dying card's index and unified `level`.
/// `step_spells` lands it on its first update -- `path::advance` arrives on a zero-length leg
/// whatever the speed, as a death bomb's does -- so its impact and its release are a cast
/// projectile's arrival, on the tick after the death. Damage and the buff's pulse are scaled by
/// the dying card's level here, once, as `cast` scales a spell's. Pure; `level` already validated.
pub fn death_projectile(cards: &CardDb, calib: &Calib, team: Team, card: u16, level: i32, at: Vec2) -> Result<Spell, String> {
    let def = cards.get(card);
    let Some(crate::card::SpellDef { shape: SpellShape::Projectile { hit, .. }, .. }) = &def.death_projectile else {
        return Err(format!("{} leaves no death projectile", def.name));
    };
    let (damage, pulse) = match hit {
        None => (0, 0),
        Some(h) => {
            let damage = cards.scaled(card, level, h.damage)?;
            let pulse = match h.buff {
                None => 0,
                Some(b) => cards.buffs[b.buff as usize].pulse_amount(calib.buff_pulse_amount, |m| cards.scaled(card, level, m))?,
            };
            (damage, pulse)
        }
    };
    Ok(Spell { team, card, level, damage, pulse, motion: SpellMotion::Flight { pos: at, aim: at, frac: Vec2::default(), delay_ms: 0 }, depth: 0 })
}

/// Turn one accepted cast -- or one death that releases an area effect -- into its
/// spell objects. Pure; `level` already validated.
pub fn cast(cards: &CardDb, calib: &Calib, arena: &Arena, team: Team, card: u16, level: i32, tap: Vec2) -> Result<Vec<Spell>, String> {
    let def = cards.get(card);
    let spell = shape_of(def).ok_or_else(|| format!("{} is not a spell", def.name))?;
    objects_for(cards, calib, Some(arena), team, card, level, 0, &spell.shape, tap)
}

/// The objects one shape of card `card`'s chain makes at `tap`, at chain depth `depth`: `cast` for
/// depth 0, and the Projectile phase for what a fuse releases or a pulsing area makes. `arena` is
/// needed only by a Projectile (its king-tower launch point); an object down a chain is never one.
#[allow(clippy::too_many_arguments)]
pub(crate) fn objects_for(cards: &CardDb, calib: &Calib, arena: Option<&Arena>, team: Team, card: u16, level: i32, depth: u8, shape: &SpellShape, tap: Vec2) -> Result<Vec<Spell>, String> {
    #[cfg(not(clash_plant = "spell_damage_unscaled"))]
    let scaled = |h: &SpellHit| cards.scaled(card, level, h.damage);
    #[cfg(clash_plant = "spell_damage_unscaled")]
    let scaled = |h: &SpellHit| cards.scaled(card, level, h.damage).map(|_| h.damage); // PLANT: level-1 damage at every level.
    // THE PULSE AMOUNT of whatever buff this spell carries, level-scaled by the
    // caster once (status.rs `BuffDef::pulse_amount`, status.BUFF_PULSE_AMOUNT). Zero
    // for a spell whose buff does not pulse, and for one with no buff at all.
    let pulse_of = |hit: &SpellHit| -> Result<i32, String> {
        let Some(b) = hit.buff else { return Ok(0) };
        cards.buffs[b.buff as usize].pulse_amount(calib.buff_pulse_amount, |m| cards.scaled(card, level, m))
    };
    let mut out = Vec::new();
    match shape {
        SpellShape::Projectile { hit, waves, wave_interval_ms, .. } => {
            let damage = match hit {
                Some(h) => scaled(h)?,
                None => 0,
            };
            let pulse = match hit {
                Some(h) => pulse_of(h)?,
                None => 0,
            };
            // calibration spells.LAUNCH_POINT = caster_king_tower_centre (the only
            // implemented candidate; Calib refuses the others).
            #[cfg(not(clash_plant = "spell_launch_from_tap"))]
            let launch = arena.ok_or("a projectile down a spell chain has no launch point")?.king_tower_pos(team);
            #[cfg(clash_plant = "spell_launch_from_tap")]
            let launch = tap; // PLANT: no flight -- the impact lands on the cast tick.
            for w in 0..*waves {
                #[cfg(not(clash_plant = "waves_simultaneous"))]
                let delay_ms = w * wave_interval_ms;
                #[cfg(clash_plant = "waves_simultaneous")]
                let delay_ms = {
                    let _ = (w, wave_interval_ms); // PLANT: every wave released at once.
                    0
                };
                out.push(Spell { team, card, level, damage, pulse, motion: SpellMotion::Flight { pos: launch, aim: tap, frac: Vec2::default(), delay_ms }, depth });
            }
        }
        SpellShape::AreaEffect { hit } => {
            out.push(Spell { team, card, level, damage: scaled(hit)?, pulse: pulse_of(hit)?, motion: SpellMotion::Area { pos: tap }, depth });
        }
        // A PULSING area effect is born at the tap with its first application DUE
        // (`next_ms` 0), so it applies on the tick it lands and every HitSpeed after --
        // spells.PULSING_AREA_EFFECT = hit_speed_period_from_landing. Under
        // hit_speed_period_delayed it is due one HitSpeed after it lands, the landing tick
        // counting as its first TICK_MS: `next_ms` HitSpeed - TICK_MS, so the first application
        // falls on L + HitSpeed / TICK_MS - 1 (measured on client 15.535.29: Poison, HitSpeed
        // 250, on L + 4; the Earthquake, 100, on L + 1; the Tornado, 50, on L). Under
        // hit_speed_offset the wait is the card's HitSpeedOffset (Calib::pulsing_area_offsets),
        // counted the same way, and a card with none applies on L: on the 16.402 corpus the Rage
        // buffs on L, the Earthquake pulses in (L + 18, L + 20] and the Poison (HitSpeedOffset 250)
        // first in (L + 22, L + 24].
        SpellShape::PulsingAreaEffect { hit, life_ms, hit_speed_ms, child } => {
            #[cfg(not(clash_plant = "pulsing_area_applies_on_landing"))]
            let arm = calib.pulsing_area_effect;
            #[cfg(clash_plant = "pulsing_area_applies_on_landing")]
            let arm = PulsingArea::FromLanding; // PLANT (regression): every arm applies on the landing tick.
            #[cfg(not(clash_plant = "pulsing_offset_by_hit_speed"))]
            let offset_of = |name: &str| calib.pulsing_area_offsets.iter().find(|(c, _)| c == name).map_or(0, |(_, ms)| *ms);
            #[cfg(clash_plant = "pulsing_offset_by_hit_speed")]
            let offset_of = |_: &str| *hit_speed_ms; // PLANT: the offset arm waits one HitSpeed, as client 15.535.29 does.
            let wait_ms = match arm {
                PulsingArea::FromLanding => 0,
                PulsingArea::Delayed => *hit_speed_ms,
                PulsingArea::HitSpeedOffset => offset_of(cards.get(card).name.as_str()),
            };
            let next_ms = (wait_ms - calib.tick_ms).max(0);
            out.push(Spell {
                team,
                card,
                level,
                damage: scaled(hit)?,
                pulse: pulse_of(hit)?,
                motion: SpellMotion::Pulsing(Pulse { pos: tap, life_ms: *life_ms, next_ms }),
                depth,
            });
            // spells.CHILD_AREA_BIRTH = with_parent: the child is made with its parent, so it acts
            // on the parent's first update. The measured arm makes it ON that update (`step_spells`).
            #[cfg(not(clash_plant = "child_area_with_parent"))]
            let with_parent = calib.child_area_birth == ChildAreaBirth::WithParent;
            #[cfg(clash_plant = "child_area_with_parent")]
            let with_parent = true; // PLANT: the child is born with its parent whatever the key says.
            if let (true, Some(c)) = (with_parent, child) {
                out.extend(objects_for(cards, calib, arena, team, card, level, depth + 1, c, tap)?);
            }
        }
        SpellShape::Rolling { airborne_min_distance, range, hit, .. } => {
            let damage = scaled(hit)?;
            let pulse = pulse_of(hit)?;
            let fwd = forward_dy(team);
            let ahead = |d: i32| Vec2::new(tap.x, tap.y + fwd * d);
            #[cfg(not(clash_plant = "airborne_skipped"))]
            let launch = calib.spell_as_deploy_launch;
            #[cfg(clash_plant = "airborne_skipped")]
            let launch = LaunchModel::InstantRollAtTap; // PLANT: no airborne phase whatever the registry says.
            #[cfg(not(clash_plant = "roll_range_from_airborne_start"))]
            let behind_len = *range;
            #[cfg(clash_plant = "roll_range_from_airborne_start")]
            let behind_len = range - airborne_min_distance; // PLANT: the roll ends ProjectileRange past the airborne START.
            let motion = match launch {
                LaunchModel::AirborneFromBehind => SpellMotion::Airborne {
                    pos: ahead(-airborne_min_distance),
                    aim: tap,
                    frac: Vec2::default(),
                    roll_start: tap,
                    roll_len: behind_len,
                },
                LaunchModel::AirborneFromTapForward => SpellMotion::Airborne {
                    pos: tap,
                    aim: ahead(*airborne_min_distance),
                    frac: Vec2::default(),
                    roll_start: ahead(*airborne_min_distance),
                    roll_len: (range - airborne_min_distance).max(0),
                },
                LaunchModel::InstantRollAtTap => SpellMotion::Rolling { pos: tap, travelled: 0, len: *range, hit: Vec::new() },
            };
            out.push(Spell { team, card, level, damage, pulse, motion, depth });
        }
        // A bottle: it deals nothing itself and releases `then` when its fuse runs out.
        SpellShape::Fuse { fuse_ms, .. } => {
            out.push(Spell { team, card, level, damage: 0, pulse: 0, motion: SpellMotion::Fuse { pos: tap, ms: *fuse_ms }, depth });
        }
        // Never cast: state.rs `enqueue` puts the unit down as a troop deploy.
        SpellShape::Summon { .. } => {}
        // A striking area: its damage is the strike's, scaled once here; nothing happens on the cast tick.
        SpellShape::Strikes(d) => {
            let motion = SpellMotion::Strikes { pos: tap, life_ms: d.life_ms, next_ms: d.gaps_ms[0], k: 0, struck: Vec::new() };
            out.push(Spell { team, card, level, damage: scaled(&d.hit)?, pulse: 0, motion, depth });
        }
    }
    Ok(out)
}

/// Read-only world a spell step needs.
pub struct SpellCtx<'a> {
    pub ents: &'a Entities,
    pub hash: &'a SpatialHash,
    pub cards: &'a CardDb,
    pub calib: &'a Calib,
    /// This tick's locomotion step of each entity, by index (state.rs `scratch.deltas`; empty where no Move phase
    /// ran). Read by `strike` for a candidate's predicted next position; an index past the end steps nothing.
    pub steps: &'a [Vec2],
}

/// Is victim `v` a legal target of `hit` cast by `team`? Alive, hp > 0, team and
/// air/ground filters, building filters. Deploying units ARE victims (spec: every
/// family).
#[inline]
fn eligible(ents: &Entities, v: usize, team: Team, hit: &SpellHit, calib: &Calib) -> bool {
    if !ents.alive[v] || ents.hp[v] <= 0 {
        return false;
    }
    // OnlyOwnTroops (card.rs SpellHit `only_own_troops`: the Battle Healer's spawn heal, Rage, the
    // Heal Spirit's heal): the releaser's side only, and within it the kinds
    // spells.OWN_SIDE_AREA_SCOPE names.
    #[cfg(not(clash_plant = "own_area_hits_both_sides"))]
    if hit.only_own_troops && ents.team[v] != team {
        return false;
    }
    if hit.only_own_troops {
        let k = ents.kind[v];
        let refused = match calib.own_side_area_scope {
            OwnSideScope::AllKinds => false,
            OwnSideScope::ExceptCrownTowers => k.is_crown_tower(),
            OwnSideScope::TroopsOnly => k != EntityKind::Troop,
        };
        if refused {
            return false;
        }
    }
    #[cfg(not(clash_plant = "spell_friendly_fire"))]
    if hit.only_enemies && ents.team[v] == team {
        return false;
    }
    #[cfg(clash_plant = "spell_friendly_fire")]
    let _ = (team, hit.only_enemies); // PLANT: OnlyEnemies ignored.
    #[cfg(clash_plant = "spells_never_hit_air")]
    if ents.flying[v] {
        return false; // PLANT: AoeToAir / HitsAir ignored.
    }
    #[cfg(clash_plant = "acquire_delay_blocks_area")]
    if ents.acquirable_from[v] > 0 {
        return false; // PLANT: a unit under targeting.SPAWNED_UNIT_ACQUIRE_DELAY is spared by an area.
    }
    let kind = ents.kind[v];
    if hit.ignore_buildings && kind != EntityKind::Troop {
        return false;
    }
    if hit.no_effect_to_crown_towers && kind.is_crown_tower() {
        return false;
    }
    if ents.flying[v] {
        hit.hits_air
    } else {
        hit.hits_ground
    }
}

/// A knockback vector of length `k.distance` along `d` (world, relative), or along the
/// fallback when `d` is zero. Exact to well under a subtile: the length is taken at
/// 1/256-subtile precision, like path::advance, so a short `d` cannot inflate the push.
#[inline]
fn push_along(d: Vec2, distance: i32, fallback: Option<Vec2>) -> Option<Vec2> {
    let d2 = d.len2();
    if d2 == 0 {
        return fallback;
    }
    let len256 = isqrt(d2 << 16).max(1) as i128;
    let s = (distance as i128) << 8;
    Some(Vec2::new(((d.x as i128) * s / len256) as i32, ((d.y as i128) * s / len256) as i32))
}

/// Should victim `v` be displaced by knockback `k`? Troops only (buildings and crown
/// towers have no Speed column and are never moved); IgnorePushback unless PushbackAll;
/// deploying units per knockback.AFFECTS_DEPLOYING_UNITS.
#[inline]
fn pushable(ctx: &SpellCtx, v: usize, k: &KnockbackDef) -> bool {
    let e = ctx.ents;
    if e.kind[v] != EntityKind::Troop {
        #[cfg(not(clash_plant = "push_buildings"))]
        return false;
    }
    #[cfg(not(clash_plant = "ignore_flag_not_read"))]
    if !k.all && ctx.cards.get(e.card[v]).ignore_pushback {
        return false;
    }
    #[cfg(clash_plant = "ignore_flag_not_read")]
    let _ = k.all; // PLANT: IgnorePushback never consulted.
    #[cfg(clash_plant = "pushback_respects_ignore")]
    if ctx.cards.get(e.card[v]).ignore_pushback {
        return false; // PLANT: PushbackAll no longer overrides IgnorePushback.
    }
    !(e.deploy_ms[v] > 0 && !ctx.calib.knock_affects_deploying)
}

/// ONE PUSH FROM A POINT, for a hit that is not a spell's impact: a troop's straight shot
/// (combat.rs `straight_hits`, calibration combat.RANGE_PROJECTILE = straight_to_range),
/// which pushes its victim radially from the projectile's centre. The same eligibility
/// (`pushable`) and the same two laws `impact` runs: under client16402 the ladder armed
/// from `centre` in native units with Pushback, under fixed_distance the displacement
/// along the radial with `impact`'s zero-vector fallback.
pub(crate) fn push_from(ctx: &SpellCtx, team: Team, v: usize, centre: Vec2, k: &KnockbackDef, fx: &mut EffectBuffer) {
    if !pushable(ctx, v, k) {
        return;
    }
    let e = ctx.ents;
    let id = e.id_of(v);
    match ctx.calib.knock_law {
        KnockLaw::FixedDistance => {
            let fallback = match ctx.calib.knock_zero_vector {
                KnockZeroVector::CasterForward => Some(Vec2::new(0, forward_dy(team))),
                KnockZeroVector::Client16402XByIdParity => Some(Vec2::new(if e.team_seq[v] & 1 == 1 { -1 } else { 1 }, 0)),
                KnockZeroVector::NoPush => None,
            };
            if let Some(d) = push_along(e.pos[v].sub(centre), k.distance, fallback.map(|f| Vec2::new(f.x * k.distance, f.y * k.distance))) {
                fx.knocks.push(Knock::Displacement(id, d));
            }
        }
        KnockLaw::Client16402 => fx.knocks.push(Knock::Push { id, src: Vec2::new(centre.x / K, centre.y / K), strength: k.distance / K, caster: team }),
    }
}

/// A PULSING AREA'S CLOCK at one application (status.AREA_BUFF_SOURCE_BINDING): where it stands,
/// its age -- TICK_MS on its landing tick, one TICK_MS more every tick after -- and the life it has
/// left, this tick included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AreaClock {
    pub pos: Vec2,
    pub age_ms: i32,
    pub left_ms: i32,
}

/// HOW A BUFF THE PULSING AREA `area` HANGS IS BOUND TO IT (status.AREA_BUFF_SOURCE_BINDING =
/// client_source_bound): (the application's time, the pulse clock a new slot starts with, the area
/// it is bound to). Under not_read, and for a buff no pulsing area hangs, (BuffTime, None, None):
/// today's application. Measured on client 15.535.29, with L the landing tick.
///
/// HitTickFromSource (the Earthquake's buff): the pulses fall when the AREA's age crosses a
/// multiple of HitFrequency -- a new slot applied at age a starts at HitFrequency - a mod
/// HitFrequency -- so the Earthquake, first applied on L + 1 (age 100), pulses on L + 19,
/// L + 39 and L + 59, not a period after the application (L + 20, L + 40, L + 60).
///
/// CapBuffTimeToAreaEffectTime (the Earthquake's area): an application lasts no longer than
/// the area's life left at it, this tick included, plus one tick. The last application, on
/// the area's final tick, then holds the two ticks after it under
/// status.BUFF_EXPIRY_TICK_ALIGNMENT = ceil_from_next_tick: the measured last slowed step is
/// L + 61, where the bare remaining life gives L + 60 and BuffTime L + 78. The one-tick
/// margin is fitted to that step; the rule it stands for is not measured further.
///
/// ControlsBuff / ControlledByParent: the slot is bound to the area (`BuffSlot::source`) and
/// taken away when the area ends (state.rs `release_orphaned_buffs`).
fn area_bound(ctx: &SpellCtx, hit: &SpellHit, b: BuffApply, area: Option<AreaClock>) -> (i32, Option<i32>, Option<Vec2>) {
    let Some(a) = area else { return (b.time_ms, None, None) };
    if ctx.calib.area_buff_source_binding != AreaBuffSourceBinding::ClientSourceBound {
        return (b.time_ms, None, None);
    }
    let Some(def) = ctx.cards.buffs.get(b.buff as usize).copied() else { return (b.time_ms, None, None) };
    #[cfg(not(clash_plant = "area_cap_unread"))]
    let time_ms = if hit.caps_buff_time { b.time_ms.min(a.left_ms + ctx.calib.tick_ms) } else { b.time_ms };
    #[cfg(clash_plant = "area_cap_unread")]
    let time_ms = b.time_ms; // PLANT (regression): CapBuffTimeToAreaEffectTime unread, the buff lives BuffTime.
    #[cfg(not(clash_plant = "hit_tick_own_clock"))]
    let first_pulse_ms = (def.hit_tick_from_source && def.hit_frequency_ms > 0).then(|| def.hit_frequency_ms - a.age_ms.rem_euclid(def.hit_frequency_ms));
    #[cfg(clash_plant = "hit_tick_own_clock")]
    let first_pulse_ms = None; // PLANT (regression): HitTickFromSource unread, the buff pulses on its own clock.
    let source = (hit.controls_buff && def.controlled_by_parent).then_some(a.pos);
    (time_ms, first_pulse_ms, source)
}

/// Apply one circular impact of `hit` at `centre` for `team`. `damage` is level-scaled. `area`: the
/// clock of the pulsing area this impact is one application of (`area_bound`), None for every
/// other impact.
#[allow(clippy::too_many_arguments)]
fn impact(ctx: &SpellCtx, team: Team, centre: Vec2, hit: &SpellHit, damage: i32, pulse: i32, dmg: &mut DamageBuffer, fx: &mut EffectBuffer, nb: &mut Vec<u32>, area: Option<AreaClock>) {
    let e = ctx.ents;
    ctx.hash.neighbours_within(e, centre, hit.radius + ctx.hash.max_radius(), nb);
    // the fixed_distance arm's zero-vector fallback, a unit axis per victim
    let fallback = |v: usize| match ctx.calib.knock_zero_vector {
        #[cfg(not(clash_plant = "zero_vector_plus_y"))]
        KnockZeroVector::CasterForward => Some(Vec2::new(0, forward_dy(team))),
        #[cfg(clash_plant = "zero_vector_plus_y")]
        KnockZeroVector::CasterForward => Some(Vec2::new(0, 1)), // PLANT: crforge's fixed +y for both seats.
        // absolute x by id parity (team_seq is the id)
        KnockZeroVector::Client16402XByIdParity => Some(Vec2::new(if e.team_seq[v] & 1 == 1 { -1 } else { 1 }, 0)),
        KnockZeroVector::NoPush => None,
    };
    // status.TARGET_BUFF_ON_SPLASH's `primary_target_only` arm: the nearest victim to
    // the impact centre, which is the unit a projectile's TargetBuff would have been
    // locked to. Tracked as (distance^2, victim) so the loop stays one pass.
    let mut primary: Option<(i64, EntityId)> = None;
    for &v in nb.iter() {
        let v = v as usize;
        if !eligible(e, v, team, hit, ctx.calib) {
            continue;
        }
        let edge = match ctx.calib.aoe_hit_test {
            AoeHitTest::EdgeInclusive => e.radius[v],
            AoeHitTest::CentreInRadius => 0,
        };
        #[cfg(clash_plant = "aoe_centre_to_centre")]
        let edge = {
            let _ = edge; // PLANT: centre-in-radius whatever the registry says.
            0
        };
        if !in_range_edge(centre, e.pos[v], hit.radius, edge) {
            continue;
        }
        let id = e.id_of(v);
        if damage > 0 {
            #[cfg(not(clash_plant = "crown_pct_ignored"))]
            let pct = hit.crown_pct;
            #[cfg(clash_plant = "crown_pct_ignored")]
            let pct = 100; // PLANT: crown towers take full spell damage.
            dmg.hits.push(Hit { target: id, amount: damage_against(e.kind[v], damage, pct, ctx.calib.crown_rounding), ignores_hide: false });
        }
        if let Some(b) = hit.buff {
            // THE BUFF RIDES THE IMPACT and lands on every victim the impact lands on
            // (calibration status.TARGET_BUFF_ON_SPLASH = whole_splash): a Snowball's
            // slow is on everything in its 2500 disc, not on one unit. `pulse_amount`
            // is the caster's level-scaled per-pulse figure, computed once here
            // because the victim does not know the caster's level. The key's own
            // provenance names the Snowball, so the SPELL path dispatches on it too;
            // an earlier version cited the key in a comment and never read it, so the
            // `primary_target_only` foil was a no-op for every spell that carries a
            // buff until the two were joined.
            match ctx.calib.target_buff_on_splash {
                TargetBuffScope::WholeSplash => {
                    let (time_ms, first_pulse_ms, source) = area_bound(ctx, hit, b, area);
                    fx.buffs.push(BuffHit { target: id, buff: b.buff, time_ms, pulse_amount: pulse, first_pulse_ms, source })
                }
                TargetBuffScope::PrimaryTargetOnly => {
                    let d = e.pos[v].sub(centre);
                    let d2 = (d.x as i64) * (d.x as i64) + (d.y as i64) * (d.y as i64);
                    let nearer = match primary {
                        Some((best, _)) => d2 < best,
                        None => true,
                    };
                    if nearer {
                        primary = Some((d2, id));
                    }
                }
            }
        }
        if let Some(k) = hit.knockback {
            #[cfg(clash_plant = "no_knockback")]
            let _ = k; // PLANT: nothing is ever pushed.
            #[cfg(not(clash_plant = "no_knockback"))]
            if pushable(ctx, v, &k) {
                match ctx.calib.knock_law {
                    KnockLaw::FixedDistance => {
                        // Unit direction scaled up first: the fallback is a unit axis.
                        let dir = push_along(e.pos[v].sub(centre), k.distance, fallback(v).map(|f| Vec2::new(f.x * k.distance, f.y * k.distance)));
                        if let Some(d) = dir {
                            fx.knocks.push(Knock::Displacement(id, d));
                        }
                    }
                    // the projectile's hit arms the ladder from the impact point with
                    // Pushback; the zero-vector direction is resolved where the ladder
                    // is armed
                    KnockLaw::Client16402 => fx.knocks.push(Knock::Push { id, src: Vec2::new(centre.x / K, centre.y / K), strength: k.distance / K, caster: team }),
                }
            }
        }
    }
    if let (Some((_, id)), Some(b)) = (primary, hit.buff) {
        let (time_ms, first_pulse_ms, source) = area_bound(ctx, hit, b, area);
        fx.buffs.push(BuffHit { target: id, buff: b.buff, time_ms, pulse_amount: pulse, first_pulse_ms, source });
    }
}

/// spells.STRIKE_REACH = edge_within_radius_plus_170_now_and_next: the reach beyond the Radius, native (the interim
/// 3670 for Lightning's 3500; measured within [3642.6, 3702.6) on client 15.535.29).
pub const STRIKE_REACH_EXTRA: i32 = 170;

/// ONE STRIKE of a striking area at `pos` (`SpellShape::Strikes`; Lightning). Measured on client 15.535.29 (28 casts,
/// 40 strikes):
/// - the candidates are the enemies `eligible` admits within spells.STRIKE_REACH of `pos` (on this tick's post-move
///   position and on the predicted next one), less every enemy this cast has struck;
/// - the pick is the highest hp (spells.STRIKE_HP_RANK), ties to the earliest created (`team_seq`: every candidate is
///   of one team, so this is the creation order and the same for both seats);
/// - the strike is a projectile born on the victim, appended after this tick's projectiles stepped, so its damage and
///   its buff land on the next tick; a victim that dies before then wastes it (combat.rs `step_projectiles`).
///
/// No candidate: no strike, and the strike is spent.
#[allow(clippy::too_many_arguments)]
fn strike(ctx: &SpellCtx, team: Team, card: u16, damage: i32, def: &StrikeDef, pos: Vec2, struck: &mut Vec<EntityId>, launched: &mut Vec<Projectile>, nb: &mut Vec<u32>) {
    let e = ctx.ents;
    #[cfg(not(clash_plant = "strike_reach_bare"))]
    let reach_rule = ctx.calib.strike_reach;
    #[cfg(clash_plant = "strike_reach_bare")]
    let reach_rule = StrikeReach::RadiusPlusTarget; // PLANT: the refuted Radius + r.
    let extra = match reach_rule {
        StrikeReach::EdgeNowAndNext => STRIKE_REACH_EXTRA * K,
        StrikeReach::RadiusPlusTarget => 0,
    };
    ctx.hash.neighbours_within(e, pos, def.hit.radius + extra + ctx.hash.max_radius(), nb);
    // (rank hp, -team_seq, index): the greatest wins
    let mut best: Option<(i32, i64, usize)> = None;
    for &v in nb.iter() {
        let v = v as usize;
        if !eligible(e, v, team, &def.hit, ctx.calib) {
            continue;
        }
        let id = e.id_of(v);
        #[cfg(not(clash_plant = "strike_repeats_target"))]
        if struck.binary_search(&id).is_ok() {
            continue;
        }
        let reach = ((def.hit.radius + e.radius[v] + extra) / K) as i64;
        let within = |p: Vec2| {
            let (dx, dy) = ((p.x / K - pos.x / K) as i64, (p.y / K - pos.y / K) as i64);
            dx * dx + dy * dy <= reach * reach
        };
        if !within(e.pos[v]) {
            continue;
        }
        // ...and on the predicted next position, under the measured arm: a target about to leave is not picked
        #[cfg(not(clash_plant = "strike_ignores_next_position"))]
        if reach_rule == StrikeReach::EdgeNowAndNext && !within(e.pos[v].add(ctx.steps.get(v).copied().unwrap_or_default())) {
            continue;
        }
        let hp = match ctx.calib.strike_hp_rank {
            StrikeHpRank::CurrentHp => e.hp[v],
            StrikeHpRank::MaxHp => e.max_hp[v],
        };
        #[cfg(not(clash_plant = "strike_tie_by_slot"))]
        let order = -(e.team_seq[v] as i64);
        #[cfg(clash_plant = "strike_tie_by_slot")]
        let order = v as i64; // PLANT: ties to the highest slot, which is not the creation order.
        let key = (hp, order, v);
        if best.map_or(true, |b| (key.0, key.1) > (b.0, b.1)) {
            best = Some(key);
        }
    }
    let Some((_, _, v)) = best else { return };
    let id = e.id_of(v);
    if let Err(at) = struck.binary_search(&id) {
        struck.insert(at, id);
    }
    let at = e.pos[v];
    launched.push(Projectile {
        team,
        pos: at,
        target: id,
        aim: at,
        speed: (def.speed * ctx.calib.projectile_speed_to_subtiles_per_tick).max(1),
        damage,
        crown_pct: def.hit.crown_pct,
        splash: 0,
        hits_air: true,
        hits_ground: true,
        frac: Vec2::default(),
        fresh: false,
        buff: def.hit.buff,
        pulse: 0,
        firer_card: Some(card),
        straight: None,
        hook: None,
        carrier: None,
        release: None,
    });
}

/// One tick of a rolling projectile: move, sweep, hit each new victim once.
/// Returns true while it still has distance to roll.
#[allow(clippy::too_many_arguments)]
fn roll(ctx: &SpellCtx, team: Team, card: u16, damage: i32, pos: &mut Vec2, travelled: &mut i32, len: i32, hit_set: &mut Vec<EntityId>, dmg: &mut DamageBuffer, fx: &mut EffectBuffer, nb: &mut Vec<u32>) -> bool {
    let Some(crate::card::SpellDef { shape: SpellShape::Rolling { speed, half_width, half_depth, hit, .. }, .. }) = &ctx.cards.get(card).spell else {
        return false;
    };
    let (speed, half_width, half_depth) = (*speed, *half_width, *half_depth);
    let e = ctx.ents;
    let fwd = forward_dy(team);
    let step = (speed * ctx.calib.projectile_speed_to_subtiles_per_tick).min(len - *travelled).max(0);
    let prev = *pos;
    #[cfg(not(clash_plant = "rolling_forward_plus_y_for_both"))]
    let cur = Vec2::new(prev.x, prev.y + fwd * step);
    #[cfg(clash_plant = "rolling_forward_plus_y_for_both")]
    let cur = Vec2::new(prev.x, prev.y + step); // PLANT: rolls toward +y for both seats.
    *pos = cur;
    *travelled += step;
    // The swept rectangle: the roll's cross-section from its previous to its current
    // centre, extended by the half-depth fore and aft. A closed Rect, so a victim
    // touching it is hit (spells.ROLLING_HIT_SHAPE) -- and swept, so the result does
    // not depend on TICK_MS.
    let rect = Rect {
        min: Vec2::new(cur.x - half_width, prev.y.min(cur.y) - half_depth),
        max: Vec2::new(cur.x + half_width, prev.y.max(cur.y) + half_depth),
    };
    let shape = Shape::Box(rect);
    let reach = half_width.max(half_depth + step) + ctx.hash.max_radius();
    let mid = Vec2::new(cur.x, (prev.y + cur.y) / 2);
    ctx.hash.neighbours_within(e, mid, reach + half_width + half_depth, nb);
    for &v in nb.iter() {
        let v = v as usize;
        #[cfg(not(clash_plant = "rolling_hits_air"))]
        let ok = eligible(e, v, team, hit, ctx.calib);
        #[cfg(clash_plant = "rolling_hits_air")]
        let ok = eligible(e, v, team, &SpellHit { hits_air: true, ..*hit }, ctx.calib); // PLANT: AoeToAir blank read as TRUE.
        if !ok {
            continue;
        }
        let id = e.id_of(v);
        #[cfg(not(clash_plant = "rolling_rehit_every_tick"))]
        if hit_set.binary_search(&id).is_ok() {
            continue;
        }
        #[cfg(not(clash_plant = "rolling_centre_in_rect"))]
        let hit_shape = ctx.calib.rolling_hit_shape;
        #[cfg(clash_plant = "rolling_centre_in_rect")]
        let hit_shape = RollHitShape::RectContainsCentre; // PLANT: the victim's centre must be inside.
        let touched = match hit_shape {
            RollHitShape::RectVsCircleEdge => shape.covers_disc(e.pos[v], e.radius[v]),
            RollHitShape::RectContainsCentre => rect.contains_closed(e.pos[v]),
        };
        if !touched {
            continue;
        }
        // Kept sorted by (index, generation) so the set -- and so state_hash -- is a
        // function of which entities were hit, not of neighbour order.
        if let Err(at) = hit_set.binary_search(&id) {
            hit_set.insert(at, id);
        }
        #[cfg(not(clash_plant = "crown_pct_ignored"))]
        let pct = hit.crown_pct;
        #[cfg(clash_plant = "crown_pct_ignored")]
        let pct = 100; // PLANT: crown towers take full spell damage.
        dmg.hits.push(Hit { target: id, amount: damage_against(e.kind[v], damage, pct, ctx.calib.crown_rounding), ignores_hide: false });
        if let Some(k) = hit.knockback {
            if pushable(ctx, v, &k) {
                let along = Vec2::new(0, fwd * k.distance);
                // RADIAL FROM THE LOG'S CENTRE WHEN IT FIRST TOUCHED THIS VICTIM, not from
                // where the centre is at the end of the tick. The sweep exists so a hit
                // does not depend on TICK_MS; a direction taken from the tick-end centre
                // did: a unit standing just ahead of the tap when the log lands was pushed
                // BACKWARD (toward the caster), because the centre had already rolled one
                // step past it. The contact point along the roll is where the front face
                // (centre + half-depth) meets the victim's near edge, clamped to this
                // tick's sweep. Taking the direction from the tick-end centre instead
                // (`push_along(e.pos[v].sub(cur), ..)`) is what produces that backward
                // push; pinned under the shipped arm by
                // log_behind_the_tap_is_pushed_back_toward_the_caster_in_either_seat (its
                // Knight half a roll step ahead of the tap).
                let edge = match hit_shape {
                    RollHitShape::RectVsCircleEdge => e.radius[v],
                    RollHitShape::RectContainsCentre => 0,
                };
                let (a_prev, a_cur, a_v) = (prev.y * fwd, cur.y * fwd, e.pos[v].y * fwd);
                #[cfg(not(clash_plant = "rolling_push_from_tick_end"))]
                let contact = Vec2::new(cur.x, (a_v - half_depth - edge).clamp(a_prev.min(a_cur), a_prev.max(a_cur)) * fwd);
                #[cfg(clash_plant = "rolling_push_from_tick_end")]
                let contact = {
                    let _ = (a_prev, a_cur, a_v, edge); // PLANT (regression): the tick-end centre.
                    cur
                };
                if ctx.calib.knock_law == KnockLaw::Client16402 {
                    // THE ARM AND THE LADDER COMPOSE THROUGH THE SOURCE POINT: the ladder
                    // pushes AWAY from a point. Under radial_from_projectile_centre (the
                    // shipped arm, measured on client 15.535.29: a troop behind the tap goes
                    // back toward the caster) that point is the contact point itself, and a
                    // victim ON it takes the zero-vector rule. Under travel_direction it is
                    // the point one native unit BEHIND the victim on the roll axis --
                    // `(dx, dy) = (0, fwd)`, `d = 1`, the target exactly Pushback down the
                    // axis, no sideways component, whatever the victim's offset from the
                    // log's centre.
                    let vn = Vec2::new(e.pos[v].x / K, e.pos[v].y / K);
                    let src = match ctx.calib.knock_direction_rolling {
                        #[cfg(not(clash_plant = "rolling_push_radial_from_centre"))]
                        RollDirection::TravelDirection => Vec2::new(vn.x, vn.y - fwd),
                        // PLANT (regression): the radial push on the travel_direction arm.
                        #[cfg(clash_plant = "rolling_push_radial_from_centre")]
                        RollDirection::TravelDirection => Vec2::new(contact.x / K, contact.y / K),
                        #[cfg(not(clash_plant = "rolling_push_travel_direction"))]
                        RollDirection::RadialFromCentre => Vec2::new(contact.x / K, contact.y / K),
                        // PLANT (regression): the travel direction on the shipped arm.
                        #[cfg(clash_plant = "rolling_push_travel_direction")]
                        RollDirection::RadialFromCentre => Vec2::new(vn.x, vn.y - fwd),
                        // On the roll axis, one disc-sum behind the victim: always behind it, so
                        // the push is never toward the caster.
                        RollDirection::RadialFromContactPoint => {
                            Vec2::new(contact.x / K, vn.y - fwd * (ctx.calib.knock_rolling_contact_radius + e.radius[v] / K))
                        }
                    };
                    fx.knocks.push(Knock::Push { id, src, strength: k.distance / K, caster: team });
                    continue;
                }
                let d = match ctx.calib.knock_direction_rolling {
                    // SELECTED (knockback.DIRECTION_ROLLING, measured on client 15.535.29):
                    // away from the log's centre where it first touched the victim, so a
                    // victim behind the tap goes BACK toward the caster, in both seats.
                    #[cfg(not(clash_plant = "rolling_push_travel_direction"))]
                    RollDirection::RadialFromCentre => push_along(e.pos[v].sub(contact), k.distance, Some(along)),
                    // PLANT (regression): the travel direction on the shipped arm.
                    #[cfg(clash_plant = "rolling_push_travel_direction")]
                    RollDirection::RadialFromCentre => Some(along),
                    // The same source as the client16402 law's, in world units.
                    RollDirection::RadialFromContactPoint => {
                        let src = Vec2::new(contact.x, e.pos[v].y - fwd * (ctx.calib.knock_rolling_contact_radius * K + e.radius[v]));
                        push_along(e.pos[v].sub(src), k.distance, Some(along))
                    }
                    // NOT SELECTED. Kept implemented because it is a listed candidate
                    // of knockback.DIRECTION_ROLLING and state.rs::pick refuses a
                    // candidate string with no engine implementation at load -- deleting
                    // this arm makes the registry unloadable, it does not tidy anything.
                    // Every victim it touches goes exactly Pushback along the caster's
                    // forward axis, with no sideways component, including one caught
                    // behind the tap by the landing log's back edge, which the
                    // measurement pushes back toward the caster instead.
                    #[cfg(not(clash_plant = "rolling_push_radial_from_centre"))]
                    RollDirection::TravelDirection => Some(along),
                    // PLANT (regression): the radial push on the travel_direction arm.
                    #[cfg(clash_plant = "rolling_push_radial_from_centre")]
                    RollDirection::TravelDirection => push_along(e.pos[v].sub(contact), k.distance, Some(along)),
                };
                if let Some(d) = d {
                    fx.knocks.push(Knock::Displacement(id, d));
                }
            }
        }
    }
    *travelled < len
}

/// UNITS A SPELL RELEASES at `at` (a Goblin Barrel's landing, a Barbarian Barrel's roll end): at the spell's level
/// through `CardDb::spawn_level`, with the release's DeployTime override and count. The one release law every spell
/// shares, with its two plants.
fn release_units(ctx: &SpellCtx, team: Team, card: u16, level: i32, sp: &SpawnDef, at: Vec2, released: &mut Vec<Release>) {
    // Level validated when the cast was accepted (state.rs).
    if let Ok(level) = ctx.cards.spawn_level(card, level) {
        #[cfg(not(clash_plant = "spawn_uses_character_deploy_time"))]
        let deploy_ms = sp.deploy_time_ms;
        #[cfg(clash_plant = "spawn_uses_character_deploy_time")]
        let deploy_ms = None; // PLANT: the unit's own DeployTime.
        #[cfg(not(clash_plant = "spawn_count_one"))]
        let count = sp.count;
        #[cfg(clash_plant = "spawn_count_one")]
        let count = 1; // PLANT: SpawnCharacterCount ignored.
        released.push(Release { team, unit: sp.unit, level, pos: at, deploy_ms, count });
    }
}

/// Advance every spell by one tick. What the step hands on comes back in `out`
/// (`SpellOut`, drained by the caller): units released by landing spells in
/// `out.released`, in `spells` order (deterministic; team_seq is per team, and a
/// team's spells keep their cast order).
pub fn step_spells(ctx: &SpellCtx, spells: &mut Vec<Spell>, dmg: &mut DamageBuffer, fx: &mut EffectBuffer, out: &mut SpellOut, nb: &mut Vec<u32>) {
    let tick = ctx.calib.tick_ms;
    let mult = ctx.calib.projectile_speed_to_subtiles_per_tick;
    spells.retain_mut(|s| {
        let def = ctx.cards.get(s.card);
        let Some(shape) = shape_of(def).and_then(|d| shape_at(&d.shape, s.depth)) else { return false };
        match (&mut s.motion, shape) {
            (SpellMotion::Flight { pos, aim, frac, delay_ms }, SpellShape::Projectile { speed, hit, spawn, .. }) => {
                if *delay_ms > 0 {
                    *delay_ms -= tick;
                    return true;
                }
                // combat.PROJECTILE_STEP: exact steps with the remainder carried, or the
                // client's truncated native step (combat.rs `projectile_advance`).
                let np = crate::combat::projectile_advance(ctx.calib.projectile_step, *pos, *aim, speed * mult, frac, s.team);
                *pos = np;
                if np != *aim {
                    return true;
                }
                if let Some(h) = hit {
                    impact(ctx, s.team, *aim, h, s.damage, s.pulse, dmg, fx, nb, None);
                }
                if let Some(sp) = spawn {
                    release_units(ctx, s.team, s.card, s.level, sp, *aim, &mut out.released);
                }
                false
            }
            (SpellMotion::Area { pos }, SpellShape::AreaEffect { hit }) => {
                let first_new = fx.buffs.len();
                impact(ctx, s.team, *pos, hit, s.damage, s.pulse, dmg, fx, nb, None);
                // A UNIT'S SPAWN AREA (card.rs `spawn_area_effect`, the Battle Healer's BattleHealerSpawnHeal; cast only
                // under spawner.SPAWN_AREA_OBJECT_SCOPE = every_row, state.rs `spawn_now`): the tick its buff lands
                // counts on the buff's pulse clock, so the first pulse falls HitFrequency - TICK_MS after it. Measured
                // on client 15.535.29: her heal pulses on her first frame + 4, 9, 14 and 19 (HitFrequency 250), where
                // a Poison's buff first pulses a whole HitFrequency after its area applies it.
                #[cfg(not(clash_plant = "spawn_area_pulse_full_period"))]
                let spawn_area = s.depth == 0
                    && matches!((shape_of(def), def.spawn_area_effect.as_ref()), (Some(a), Some(b)) if std::ptr::eq(a, b));
                #[cfg(clash_plant = "spawn_area_pulse_full_period")]
                let spawn_area = false; // PLANT: the spawn area's buff first pulses a whole HitFrequency after it lands.
                if spawn_area {
                    for h in fx.buffs[first_new..].iter_mut().filter(|h| h.first_pulse_ms.is_none()) {
                        if let Some(b) = ctx.cards.buffs.get(h.buff as usize) {
                            h.first_pulse_ms = Some((b.hit_frequency_ms - tick).max(0));
                        }
                    }
                }
                false
            }
            // A PULSING AREA EFFECT (Poison, Earthquake; calibration
            // spells.PULSING_AREA_EFFECT = hit_speed_period_from_landing): it lands,
            // applies at once, and re-applies every HitSpeed ms until LifeDuration is
            // spent (under hit_speed_period_delayed the first application waits, `cast`).
            // Every application is one `impact`, so the buff refresh, the eligibility
            // filters and the hit test are the one-shot ones. The area's clock rides
            // along for status.AREA_BUFF_SOURCE_BINDING (`area_bound`): TICK_MS old on
            // the landing tick, with `life_ms` left, this tick included.
            (SpellMotion::Pulsing(p), SpellShape::PulsingAreaEffect { hit, hit_speed_ms, life_ms: total_ms, child }) => {
                // THE CHILD AREA (spells.CHILD_AREA_BIRTH = on_parent_first_update): made on the
                // parent's first update, the only one whose life is still whole; it acts next tick.
                #[cfg(not(clash_plant = "child_area_with_parent"))]
                let on_update = ctx.calib.child_area_birth == ChildAreaBirth::OnParentFirstUpdate;
                #[cfg(clash_plant = "child_area_with_parent")]
                let on_update = false; // PLANT: born with the parent instead (`objects_for`).
                if let (true, Some(c), true) = (on_update, child, p.life_ms == *total_ms) {
                    if let Ok(v) = objects_for(ctx.cards, ctx.calib, None, s.team, s.card, s.level, s.depth + 1, c, p.pos) {
                        out.born.extend(v);
                    }
                }
                while p.next_ms <= 0 && p.life_ms > 0 {
                    let clock = AreaClock { pos: p.pos, age_ms: *total_ms - p.life_ms + tick, left_ms: p.life_ms };
                    impact(ctx, s.team, p.pos, hit, s.damage, s.pulse, dmg, fx, nb, Some(clock));
                    p.next_ms += (*hit_speed_ms).max(tick);
                }
                p.next_ms -= tick;
                p.life_ms -= tick;
                p.life_ms > 0
            }
            (SpellMotion::Airborne { pos, aim, frac, roll_start, roll_len }, SpellShape::Rolling { airborne_speed, .. }) => {
                let (np, _) = advance(*pos, *aim, airborne_speed * mult, frac);
                *pos = np;
                if np != *aim {
                    return true;
                }
                // Landed: the rolling projectile is made on the landing point. spells.ROLL_FIRST_STEP: it runs
                // its first step this tick (on_landing_tick), or stands unmoved until the next (tick_after_landing,
                // measured on the Barbarian Barrel: 4 of 4 on the 16.402 corpus).
                let (mut p, mut travelled, len, mut hit) = (*roll_start, 0, *roll_len, Vec::new());
                #[cfg(not(clash_plant = "roll_steps_on_landing"))]
                let first = ctx.calib.roll_first_step;
                #[cfg(clash_plant = "roll_steps_on_landing")]
                let first = RollFirstStep::OnLandingTick; // PLANT: the first step on the landing tick, whatever the key.
                if first == RollFirstStep::TickAfterLanding {
                    s.motion = SpellMotion::Rolling { pos: p, travelled, len, hit };
                    return true;
                }
                let more = roll(ctx, s.team, s.card, s.damage, &mut p, &mut travelled, len, &mut hit, dmg, fx, nb);
                if !more {
                    if let Some(sp) = shape.release() {
                        release_units(ctx, s.team, s.card, s.level, sp, p, &mut out.released);
                    }
                }
                s.motion = SpellMotion::Rolling { pos: p, travelled, len, hit };
                more
            }
            // A ROLL THAT RELEASES UNITS (the Barbarian Barrel) releases them where it stops, on the tick it stops.
            (SpellMotion::Rolling { pos, travelled, len, hit }, SpellShape::Rolling { spawn, .. }) => {
                let more = roll(ctx, s.team, s.card, s.damage, pos, travelled, *len, hit, dmg, fx, nb);
                #[cfg(not(clash_plant = "roll_release_dropped"))]
                if !more {
                    if let Some(sp) = spawn {
                        release_units(ctx, s.team, s.card, s.level, sp, *pos, &mut out.released);
                    }
                }
                more
            }
            // A STRIKING AREA (spells.STRIKE_TIMER_LEFTOVER): each update takes a tick off its clock, and the
            // update whose clock falls below zero strikes (`strike`). Under carried the next strike is timed from
            // the strike time itself, so strike k falls on the cast tick + floor(k x HitSpeed / TICK_MS); under
            // dropped from the end of the strike's tick. The object goes with its last scheduled strike, and under
            // spells.STRIKE_AREA_END = at_life_end also on the update its life reaches 0, the strike check first. A
            // strike due at exactly the LifeDuration (HitSpeed 500 of 1500: the third, on the cast tick + 30) falls
            // one update after that one, so at_life_end loses it and with_last_strike makes it. With HitSpeed 460
            // the last strike (the cast tick + 27) comes before the life ends, and the two arms do the same.
            (SpellMotion::Strikes { pos, life_ms, next_ms, k, struck }, SpellShape::Strikes(def)) => {
                *next_ms -= tick;
                *life_ms -= tick;
                if *next_ms < 0 && (*k as usize) < def.gaps_ms.len() {
                    strike(ctx, s.team, s.card, s.damage, def, *pos, struck, &mut out.launched, nb);
                    *k += 1;
                    let gap = def.gaps_ms.get(*k as usize).copied().unwrap_or(0);
                    #[cfg(not(clash_plant = "strike_timer_restarts"))]
                    let rule = ctx.calib.strike_timer_leftover;
                    #[cfg(clash_plant = "strike_timer_restarts")]
                    let rule = StrikeLeftover::Dropped; // PLANT: the leftover dropped, D+9, D+19, D+29.
                    *next_ms = match rule {
                        StrikeLeftover::Carried => *next_ms + gap,
                        StrikeLeftover::Dropped => gap,
                    };
                }
                #[cfg(not(clash_plant = "strike_area_ends_at_life"))]
                let end = ctx.calib.strike_area_end;
                #[cfg(clash_plant = "strike_area_ends_at_life")]
                let end = StrikeAreaEnd::AtLifeEnd; // PLANT: the life ends the area before a strike due at its end.
                let more = (*k as usize) < def.gaps_ms.len();
                match end {
                    StrikeAreaEnd::AtLifeEnd => more && *life_ms > 0,
                    StrikeAreaEnd::WithLastStrike => more,
                }
            }
            // THE BOTTLE (spells.SUMMON_FUSE_START = unit_deploy_law): the fuse is counted like a
            // unit's DeployTime, from the cast tick's own update, and releases on the update it reaches
            // 0: a 500 ms bottle cast on tick C releases on C + 9, and what it releases first acts on
            // C + 10. death_bomb_flight is the arithmetic a death bomb's delayed flight runs (one tick
            // later).
            (SpellMotion::Fuse { pos, ms }, SpellShape::Fuse { then, .. }) => {
                #[cfg(not(clash_plant = "fuse_counts_from_next_tick"))]
                let start = ctx.calib.summon_fuse_start;
                #[cfg(clash_plant = "fuse_counts_from_next_tick")]
                let start = SummonFuseStart::DeathBombFlight; // PLANT: the death bomb's arithmetic, one tick later.
                match start {
                    SummonFuseStart::UnitDeployLaw => {
                        *ms -= tick;
                        if *ms > 0 {
                            return true;
                        }
                    }
                    SummonFuseStart::DeathBombFlight => {
                        if *ms > 0 {
                            *ms -= tick;
                            return true;
                        }
                    }
                }
                // Level validated when the cast was accepted.
                if let Ok(v) = objects_for(ctx.cards, ctx.calib, None, s.team, s.card, s.level, s.depth + 1, then, *pos) {
                    out.born.extend(v);
                }
                false
            }
            _ => false,
        }
    });
}

/// Where a ground unit pushed from `old` toward `desired` comes to rest: clamped to
/// the arena, pushed out of building footprints as collision does, ejected from water
/// (knockback.WATER_RESOLUTION = eject_to_nearest_land), repeated until both hold. If
/// they cannot be satisfied together in a few rounds the unit stays at `old` -- which
/// the every-tick invariants already hold to be legal ground.
pub fn settle(arena: &Arena, obstacles: &[Obstacle], team: Team, radius: i32, flying: bool, old: Vec2, desired: Vec2) -> Vec2 {
    #[cfg(clash_plant = "knock_unclamped")]
    if !arena.in_bounds(desired) {
        // PLANT: a push may leave the arena. (Removing only the clamp below does NOT
        // land: the water ejection also returns in-bounds points, so the clamp is
        // defence in depth, not the only guard.)
        return desired;
    }
    let clamp = |p: Vec2| Vec2::new(p.x.clamp(0, arena.width), p.y.clamp(0, arena.height));
    let mut p = clamp(desired);
    if flying {
        return p;
    }
    #[cfg(clash_plant = "no_water_resolution")]
    {
        let _ = (obstacles, team, radius, old); // PLANT: the push ends wherever it ends.
        return p;
    }
    #[allow(unreachable_code)]
    for _ in 0..4 {
        let mut total = Vec2::default();
        #[cfg(clash_plant = "knock_ignores_footprints")]
        let obstacles: &[Obstacle] = &[]; // PLANT: pushes pass through buildings.
        for o in obstacles {
            if let Some(q) = o.shape.push_out(p, radius, team) {
                total = total.add(q.sub(p));
            }
        }
        if total != Vec2::default() {
            p = clamp(p.add(total));
        }
        if !arena.is_passable_ground(p) {
            match arena.nearest_passable_ground(p, team) {
                Some(q) => p = q,
                None => return old,
            }
        }
        if arena.is_passable_ground(p) && !obstacles.iter().any(|o| o.shape.penetrates(p, 0)) {
            return p;
        }
    }
    old
}
