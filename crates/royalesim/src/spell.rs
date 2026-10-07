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
use crate::card::{AbilityEffect, AttachedArea, CardDb, KnockbackDef, SelectorDef, SpawnDef, SpawnOffset, SpawnVia, SpellHit, SpellShape, StrikeDef, StrikePick};
use crate::status::{BuffApply, BuffHit, Pulse};
use crate::combat::{damage_against, DamageBuffer, Hit, Projectile};
use crate::entity::{EntityKind, Entities, HideState, SpatialHash};
use crate::fixed::{in_range_edge, isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use crate::path::{advance, Obstacle};
use crate::state::{
    AirToGroundWindow, AoeHitTest, AreaBuffSourceBinding, AreaProjectileIgnoreBuildings, AreaSpawnedAreaStart, BarrageReach, Calib, ChildAreaBirth, CrownPerHitScaling,
    BuildingSpellReach, CrownTowerSpellReach,
    DeathBombSpawnTiming, DeathPushbackScope, KnockLaw, KnockZeroVector, LaunchModel, OwnSideScope, PulsingArea, RollDirection, RollFirstStep, RollHitShape,
    StaggerWait, StrikeAreaEnd, StrikeDue, StrikeHpRank, StrikeLeftover, StrikeReach, SubTickDelayRounding, SummonFuseStart, TargetBuffScope,
};
use crate::{EntityId, Team};

/// ONE UNIT A BALL CAUGHT (`SpellMotion::CaptureRoll`): its distance to the ball at the capture, native, and whether it
/// has joined the ball.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Captive {
    pub id: EntityId,
    pub d0: i32,
    pub joined: bool,
}

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
    /// THE EVO GIANT SNOWBALL'S BALL (card.rs `SpellShape::CaptureRoll`): its point, the distance it rolled, its age in
    /// ticks from its first point, and its captives.
    CaptureRoll { pos: Vec2, travelled: i32, age: u32, captives: Vec<Captive> },
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
    /// A SCHEDULED AREA at `pos` (`SpellShape::ScheduledArea`; the Graveyard, the Suspicious Bush's death area): `born`
    /// the tick it was created on, which its entries' delays count from, and `fired` one bit per entry already
    /// released (bit k for entry k).
    Scheduled { pos: Vec2, born: u32, fired: u32 },
    /// AN AREA RIDING ON A UNIT (card.rs `AttachedArea`: a hero's button, the Hero Ice Golem's storm): area `part` of
    /// the card's ability, centred on `pos`, which is `parent`'s position on every update while the parent lives and
    /// the area follows it. `life_ms` left and `next_ms` to its next hit, as a pulsing area's clocks.
    Attached { parent: EntityId, pos: Vec2, part: u8, life_ms: i32, next_ms: i32 },
}

/// One live spell object.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Spell {
    pub team: Team,
    /// The CardDb index of the card whose play made this spell object: a spell card, or the card of the unit or
    /// form that made it (an evolution's effects, a death bomb, a deploy blow, a hero's ability).
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
    /// THE TICKS A FLIGHT HAS MOVED (`SpellMotion::Flight` only): 0 while it waits out its delay, then one more for
    /// each tick it travels. `state_json`'s spell rows carry it (`ticks_flown`), so an observer can date how long an
    /// arc has been in the air. `default` 0 for a spell saved before the field; not hashed (it follows from the
    /// motion the hash already carries).
    #[serde(default)]
    pub flown: u16,
}

/// A unit a scheduled area releases (`SpellMotion::Scheduled`): unit `unit` at unified `level` for `team`, at the
/// point `offset` gives from the area's `centre` (state.rs `scheduled_point`), deploying `deploy_ms` (None: its own
/// DeployTime), through the path `via` names (state.rs `phase_projectile`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledRelease {
    pub team: Team,
    pub unit: u16,
    pub level: i32,
    pub centre: Vec2,
    pub offset: SpawnOffset,
    pub deploy_ms: Option<i32>,
    pub via: SpawnVia,
    /// The spawning object's card, the released units' producer (entity.rs `source`).
    pub source: u16,
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
    /// The spell's card, the released units' producer (entity.rs `source`).
    pub source: u16,
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
    /// The shot's target, where the area comes from a shot (the Evo Ice Spirits' rides on it: card.rs
    /// `EvoDef::impact_area`); None for a spear's trail.
    pub target: Option<crate::EntityId>,
    /// The attached part it is made as (`attached_area`: the Evo Firecracker's fireworks, standing where they are made),
    /// or None for the card's own area (`cast`).
    pub part: Option<u8>,
}

/// A container whose fuse ran out at `pos` (a death bomb that carries a death spawn, the
/// Skeleton Barrel's): an order for the death spawn of `card` (at the container's unified
/// `level`) to come out there. Written by `step_spells` on the tick
/// spawner.DEATH_BOMB_SPAWN_TIMING names, carried out by state.rs `release_fuse_end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuseEnd {
    pub team: Team,
    pub card: u16,
    pub level: i32,
    pub pos: Vec2,
}

/// THE DELAY OF A CONTAINER THAT HAS RELEASED ITS UNITS before its hit (spawner.DEATH_BOMB_SPAWN_TIMING =
/// units_at_fuse_end): not above zero, so the object arrives on the next tick, and never reached by a fuse's own
/// countdown, which stops within one tick below zero, so the arrival knows the units are out already.
pub const FUSE_RELEASED: i32 = i32::MIN;

/// knockback.DEATH_PUSHBACK: the push a death bomb's hit gives the units it lands on (card.rs
/// `CardDef::death_pushback`, the row's DeathPushBack), or None. Measured on client 15.535.29 on the Skeleton
/// Barrel's container (DeathPushBack 1000): a deploying Minion on the axis below the death point stepped 200, 175,
/// 150, 125, 100, 75, 50, 25, 0 away from it from the tick after the hit, then 25 back, the knockback ladder's own
/// steps (`Knock::Push`). Only a bomb's row: every other spell carries its own Pushback in its hit.
fn death_bomb_push(ctx: &SpellCtx, def: &crate::card::CardDef) -> Option<KnockbackDef> {
    if def.death_bomb_fuse_ms().is_none() || def.death_pushback <= 0 {
        return None;
    }
    let reads = match ctx.calib.death_pushback {
        // A bomb a champion's button drops pushes too (`CardDef::dropped_by_ability`, the Mighty Miner's: measured, the
        // Knight it hit stepped the ladder of 1800).
        #[cfg(not(clash_plant = "lane_bomb_unpushed"))]
        DeathPushbackScope::ContainersLadder => def.death_spawn.is_some() || def.dropped_by_ability,
        #[cfg(clash_plant = "lane_bomb_unpushed")]
        DeathPushbackScope::ContainersLadder => def.death_spawn.is_some(), // PLANT (regression): the dropped bomb does not push.
        DeathPushbackScope::EveryDeathBombLadder => true,
        DeathPushbackScope::NotRead => false,
    };
    // PLANT death_pushback_unread (tests/skeleton_barrel.rs): no bomb pushes, whatever the key says.
    #[cfg(clash_plant = "death_pushback_unread")]
    let reads = {
        let _ = reads;
        false
    };
    reads.then_some(KnockbackDef { distance: def.death_pushback, all: false })
}

/// A unit a spell copies (`SpellShape::Clone`): an order to copy `src` at the spell's unified `level`, for the
/// Clone card `card` (whose shape holds the hold and the rules). state.rs `phase_projectile` hands it to Reap, which
/// makes the copy (`materialise_clones`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloneOrder {
    pub src: EntityId,
    pub level: i32,
    pub card: u16,
}

/// EVERYTHING ONE PROJECTILE PHASE HANDS ON to the rest of the tick, in one bundle so
/// a new kind of output is one field and not one more parameter on `step_spells`.
/// Nothing in it outlives the tick, so none of it is in a snapshot or in the state
/// hash. state.rs `phase_projectile` drains five of them before the phase ends, in
/// this order, and every object appended in steps 3-5 first acts on the next tick,
/// because this tick's steps have already run:
///   1. `released`: units a landing spell releases (laid out, then `release`);
///   2. `scheduled`: units a scheduled area puts down (each at its own point, then `release`);
///   3. `born`: spell objects a spell object makes (appended to the spell list);
///   4. `areas`: area effects a landing object leaves (cast, appended after 3's);
///   5. `launched`: projectiles a spell object fires (appended to the projectile list);
///   6. `fuse_ends`: containers whose fuse ended (`release_fuse_end`: their death spawn
///      laid out, then `release`, as step 1's units are).
///
/// `clones` (the Clone's copies) goes to the Reap phase of the same tick (state.rs
/// `materialise_clones`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpellOut {
    pub released: Vec<Release>,
    pub scheduled: Vec<ScheduledRelease>,
    pub born: Vec<Spell>,
    pub launched: Vec<Projectile>,
    pub areas: Vec<AreaRelease>,
    pub clones: Vec<CloneOrder>,
    pub fuse_ends: Vec<FuseEnd>,
    /// THE POINTS OF THE UNITS A BALL CARRIES this tick (`SpellMotion::CaptureRoll`), set after the spells step (state.rs
    /// `phase_projectile`).
    pub carried: Vec<(EntityId, Vec2)>,
    /// movement.CAPTURE_DRAG_FACING = client15535_faces_ball: the facings (256 long) of the units a ball drags this tick,
    /// toward its point, set with `carried`.
    pub faced: Vec<(EntityId, Vec2)>,
    /// spells.CAPTURE_ROUTE: the captives that joined their ball this tick (state.rs `phase_projectile` drops their routes
    /// under client15535_dropped_at_join).
    pub joined: Vec<EntityId>,
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
    /// forward axis of the zero-vector fallback). `now`: the ladder's first step is taken on the hit's own tick
    /// (state.rs `ladder_step_now`), which an Evo Cannon bomb does (measured on client 15.535.29: all 6 barrage
    /// pushes step on their damage tick, a Fireball's one tick after, as every other push here) and a combo's melee
    /// pushback (knockback.COMBO_PUSHBACK: the Monk's third hit, both measured pushes stepping on the hit's tick).
    Push {
        id: EntityId,
        src: Vec2,
        strength: i32,
        caster: Team,
        #[serde(default)]
        now: bool,
    },
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
    /// Hooks that landed this tick, (victim, thrower, the hook's point at the start of the tick) (calibration
    /// combat.SPECIAL_HOOK = client_hook_drag; combat.rs `step_projectiles`). Drained in Resolve by `state.rs
    /// apply_effects`, which starts the drag on a surviving victim, set onto the hook's point under
    /// combat.HOOK_LANDING = client_hook_point. Empty in a battle with no hook. `default` so a snapshot saved before
    /// it still loads.
    #[serde(default)]
    pub hooks: Vec<(EntityId, EntityId, Vec2)>,
    /// The Rune Giant's projectiles that landed on a live friend this tick, (friend, payload) (combat.rs
    /// `step_projectiles`). Drained in Resolve by `state.rs apply_effects`, which puts the enchant on a friend that
    /// survived it. Empty in every battle with no Rune Giant. `default` so a snapshot saved before it still loads.
    #[serde(default)]
    pub enchants: Vec<(EntityId, crate::combat::EnchantPayload)>,
    /// Fliers a catch holds to the ground this tick, (victim, window ms) (the Vines; `catch`). Drained in Resolve by
    /// `state.rs apply_effects`, which sets the survivor's `grounded_ms`. Empty in every battle with no Vines. `default`
    /// so a snapshot saved before it still loads.
    #[serde(default)]
    pub grounds: Vec<(EntityId, i32)>,
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
        .or(def.evo.as_ref().and_then(|v| v.barrage.as_ref()).map(|b| &b.shot))
        .or(def.evo.as_ref().and_then(|v| v.ghost.as_ref()).map(|g| &g.strike))
        .or(def.evo.as_ref().and_then(|v| v.fall.as_ref()).map(|f| &f.landing))
        .or(def.evo.as_ref().and_then(|v| v.shield_blast.as_ref()))
        .or(def.idle_area.as_ref())
        .or(def.deploy_spawn_area.as_ref())
        // The Hero Giant's landing blow (card.rs `SlapDef::landing`; state.rs `slap_pass`).
        .or(match def.ability.as_ref().map(|a| &a.effect) {
            Some(crate::card::AbilityEffect::Slap(s)) => Some(&s.landing),
            _ => None,
        })
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
    Ok(Spell { team, card, level, damage, pulse, motion: SpellMotion::Flight { pos: at, aim: at, frac: Vec2::default(), delay_ms: 0 }, depth: 0, flown: 0 })
}

/// Area `part` of card `card`'s ability (card.rs `AbilityEffect::Areas`), or None.
pub(crate) fn attached_def(cards: &CardDb, card: u16, part: u8) -> Option<&AttachedArea> {
    // THE AREA AN ATTACK MAKES ON ITS UNIT (card.rs `AttackAreaDef`, the Evo Valkyrie's tornado): off its EvoDef.
    if part == crate::card::EVO_ATTACK_AREA {
        return cards.cards.get(card as usize).and_then(|c| c.evo.as_ref()).and_then(|v| v.attack_area.as_ref()).map(|a| &a.area);
    }
    // THE EVO FIRECRACKER'S FIREWORKS (card.rs `FireworksDef`): where its rocket lands, where a spark's flight ends.
    if part == crate::card::EVO_FIREWORKS || part == crate::card::EVO_SPARK_FIREWORKS {
        let fw = cards.cards.get(card as usize).and_then(|c| c.evo.as_ref()).and_then(|v| v.fireworks.as_ref())?;
        return Some(if part == crate::card::EVO_FIREWORKS { &fw.big } else { &fw.small });
    }
    // THE EVO PRINCESS'S FREEZING AREA (card.rs `FreezeVolleyDef::area`), where her freezing arrow landed.
    if part == crate::card::EVO_FREEZE_AREA {
        return cards.cards.get(card as usize).and_then(|c| c.evo.as_ref()).and_then(|v| v.freeze_volley.as_ref()).map(|f| &f.area);
    }
    // THE AREA RIDING A SHOT'S TARGET (card.rs `EvoDef::impact_area`, the Evo Ice Spirits').
    if part == crate::card::EVO_IMPACT_AREA {
        return cards.cards.get(card as usize).and_then(|c| c.evo.as_ref()).and_then(|v| v.impact_area.as_ref());
    }
    match cards.cards.get(card as usize).and_then(|c| c.ability.as_ref()).map(|a| &a.effect) {
        Some(AbilityEffect::Areas { areas, .. }) => areas.get(part as usize),
        // The Hero Valkyrie's blow (`AbilityEffect::SpinChain`): her spin's one area.
        Some(AbilityEffect::SpinChain { area, .. }) if part == 0 => Some(area),
        _ => None,
    }
}

/// AREA `part` OF CARD `card`'S ABILITY, made at `pos` on `parent` for `team` at unified `level` (`SpellMotion::Attached`):
/// its damage fixed now, on the card's ladder only when its damage type scales with level, and its first hit
/// `first_ms` of its age away (its first update is TICK_MS old).
#[allow(clippy::too_many_arguments)]
pub fn attached_area(cards: &CardDb, calib: &Calib, team: Team, card: u16, level: i32, parent: EntityId, pos: Vec2, part: u8) -> Result<Spell, String> {
    let a = attached_def(cards, card, part).ok_or_else(|| format!("{} has no ability area {part}", cards.get(card).name))?;
    let damage = if a.level_scaled { cards.scaled(card, level, a.hit.damage)? } else { a.hit.damage };
    let pulse = match a.hit.buff {
        None => 0,
        Some(b) => cards.buffs[b.buff as usize].pulse_amount(calib.buff_pulse_amount, |m| cards.scaled(card, level, m))?,
    };
    let motion = SpellMotion::Attached { parent, pos, part, life_ms: a.life_ms, next_ms: a.first_ms - calib.tick_ms };
    Ok(Spell { team, card, level, damage, pulse, motion, depth: 0, flown: 0 })
}

/// Turn one accepted cast -- or one death that releases an area effect -- into its
/// spell objects, created on tick `now`. Pure; `level` already validated.
#[allow(clippy::too_many_arguments)]
pub fn cast(cards: &CardDb, calib: &Calib, arena: &Arena, team: Team, card: u16, level: i32, tap: Vec2, now: u32) -> Result<Vec<Spell>, String> {
    let def = cards.get(card);
    let spell = shape_of(def).ok_or_else(|| format!("{} is not a spell", def.name))?;
    objects_for(cards, calib, Some(arena), team, card, level, 0, &spell.shape, tap, now)
}

/// The objects one shape of card `card`'s chain makes at `tap`, at chain depth `depth`, on tick `now`: `cast` for
/// depth 0, and the Projectile phase for what a fuse releases or a pulsing area makes. `arena` is
/// needed only by a Projectile (its king-tower launch point); an object down a chain is never one. `now` is read only
/// by a scheduled area, whose delays count from its creation tick.
#[allow(clippy::too_many_arguments)]
pub(crate) fn objects_for(cards: &CardDb, calib: &Calib, arena: Option<&Arena>, team: Team, card: u16, level: i32, depth: u8, shape: &SpellShape, tap: Vec2, now: u32) -> Result<Vec<Spell>, String> {
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
                out.push(Spell { team, card, level, damage, pulse, motion: SpellMotion::Flight { pos: launch, aim: tap, frac: Vec2::default(), delay_ms }, depth, flown: 0 });
            }
        }
        SpellShape::AreaEffect { hit } => {
            out.push(Spell { team, card, level, damage: scaled(hit)?, pulse: pulse_of(hit)?, motion: SpellMotion::Area { pos: tap }, depth, flown: 0 });
        }
        // The Evo Giant Snowball's flight, as a projectile spell's (spells.LAUNCH_POINT), carrying its roll's hit.
        SpellShape::CaptureRoll(d) => {
            let launch = arena.ok_or("a capture roll down a spell chain has no launch point")?.king_tower_pos(team);
            out.push(Spell { team, card, level, damage: scaled(&d.hit)?, pulse: 0, motion: SpellMotion::Flight { pos: launch, aim: tap, frac: Vec2::default(), delay_ms: 0 }, depth, flown: 0 });
        }
        // An echoing area (card.rs `SpellShape::Echo`, the Evo Zap): its own hit as an `AreaEffect`'s, and its second
        // strike's fuse, made with it at the same point.
        SpellShape::Echo { hit, then } => {
            out.push(Spell { team, card, level, damage: scaled(hit)?, pulse: pulse_of(hit)?, motion: SpellMotion::Area { pos: tap }, depth, flown: 0 });
            // The second area is made on the cast tick + floor(delay / TICK_MS) (actions.SUB_ACTIONS_DELAY, as a
            // scheduled area's entries are) and first acts the tick after, as an area another area makes does: one
            // tick past the fuse's own release (spells.SUMMON_FUSE_START). Measured on client 15.535.29
            // (sp-form-Zap-evo-s0): the second strike 30 frames after the first, for a delay of 1450.
            let mut rest = objects_for(cards, calib, arena, team, card, level, depth + 1, then, tap, now)?;
            for o in rest.iter_mut() {
                if let SpellMotion::Fuse { ms, .. } = &mut o.motion {
                    *ms += calib.tick_ms;
                }
            }
            out.extend(rest);
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
            #[cfg(not(any(clash_plant = "pulsing_offset_by_hit_speed", clash_plant = "pulsing_offset_listed_hit_speed")))]
            let offset_of = |name: &str| calib.pulsing_area_offsets.iter().find(|(c, _)| c == name).map_or(0, |(_, ms)| *ms);
            #[cfg(clash_plant = "pulsing_offset_by_hit_speed")]
            let offset_of = |_: &str| *hit_speed_ms; // PLANT: the offset arm waits one HitSpeed, as client 15.535.29 does.
            // PLANT (regression, tests/status.rs): a listed card waits one HitSpeed, whatever the value listed for it.
            #[cfg(clash_plant = "pulsing_offset_listed_hit_speed")]
            let offset_of = |name: &str| if calib.pulsing_area_offsets.iter().any(|(c, _)| c == name) { *hit_speed_ms } else { 0 };
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
                flown: 0,
            });
            // spells.CHILD_AREA_BIRTH = with_parent: the child is made with its parent, so it acts
            // on the parent's first update. The measured arm makes it ON that update (`step_spells`).
            #[cfg(not(clash_plant = "child_area_with_parent"))]
            let with_parent = calib.child_area_birth == ChildAreaBirth::WithParent;
            #[cfg(clash_plant = "child_area_with_parent")]
            let with_parent = true; // PLANT: the child is born with its parent whatever the key says.
            if let (true, Some(c)) = (with_parent, child) {
                out.extend(objects_for(cards, calib, arena, team, card, level, depth + 1, c, tap, now)?);
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
            out.push(Spell { team, card, level, damage, pulse, motion, depth, flown: 0 });
        }
        // A bottle: it deals nothing itself and releases `then` when its fuse runs out. A ZERO fuse is an area whose
        // action makes another area (card.rs `area_spawns_area`, the Goblin Curse): spells.AREA_SPAWNED_AREA_START =
        // on_parent_first_update releases the child on the fuse's first update, so it first acts the next tick with
        // its whole life (measured on client 15.535.29: the curse circle applies from the cast tick + 1 to + 120);
        // with_parent makes the child here, with its parent, so it acts on the cast tick.
        SpellShape::Fuse { fuse_ms, then } => {
            #[cfg(not(clash_plant = "curse_child_with_parent"))]
            let with_parent = calib.area_spawned_area_start == AreaSpawnedAreaStart::WithParent;
            #[cfg(clash_plant = "curse_child_with_parent")]
            let with_parent = true; // PLANT: the child is made with its parent whatever the key says.
            if *fuse_ms == 0 && with_parent {
                out.extend(objects_for(cards, calib, arena, team, card, level, depth + 1, then, tap, now)?);
            } else {
                out.push(Spell { team, card, level, damage: 0, pulse: 0, motion: SpellMotion::Fuse { pos: tap, ms: *fuse_ms }, depth, flown: 0 });
            }
        }
        // Never cast: state.rs `enqueue` puts the unit down as a troop deploy.
        SpellShape::Summon { .. } => {}
        // Never cast either, and an Err rather than no objects: state.rs `resolve_play` plays the card the Mirror
        // copies or the form the variant card chooses, and a cast that made nothing would be the silent "debited,
        // nothing put down" play. Every path that could enqueue one refuses it first (`spawn_unit`,
        // `formation_preview`, `load_with`).
        SpellShape::Mirror => return Err(format!("{} replays its side's last play; it is never cast", cards.get(card).name)),
        SpellShape::Variant { .. } => return Err(format!("{} chooses a form at the play; it is never cast", cards.get(card).name)),
        // THE CLONE: a one-shot area at the tap, which copies on its first update (`step_spells`). It deals nothing.
        SpellShape::Clone { .. } => {
            out.push(Spell { team, card, level, damage: 0, pulse: 0, motion: SpellMotion::Area { pos: tap }, depth, flown: 0 });
        }
        // A striking area: its damage is the strike's, scaled once here; nothing happens on the cast tick.
        SpellShape::Strikes(d) => {
            let motion = SpellMotion::Strikes { pos: tap, life_ms: d.life_ms, next_ms: d.gaps_ms[0], k: 0, struck: Vec::new() };
            out.push(Spell { team, card, level, damage: scaled(&d.hit)?, pulse: 0, motion, depth, flown: 0 });
        }
        // A scheduled area: it deals nothing; its clock is its creation tick (`step_spells`).
        SpellShape::ScheduledArea { .. } => {
            out.push(Spell { team, card, level, damage: 0, pulse: 0, motion: SpellMotion::Scheduled { pos: tap, born: now, fired: 0 }, depth, flown: 0 });
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
    /// The tick being stepped: what a scheduled area's age is counted against, the creation tick of what a
    /// fuse or a pulsing area makes, and the tick a selector's FilterDashImmune reads a unit's dash immunity at
    /// (entity.rs `dash_immune`).
    pub tick: u32,
}

/// Is victim `v` a legal target of `hit` cast by `team`? Alive, hp > 0, team and
/// air/ground filters, building filters. Deploying units ARE victims (spec: every
/// family).
#[inline]
fn eligible(ents: &Entities, v: usize, team: Team, hit: &SpellHit, calib: &Calib) -> bool {
    if !ents.alive[v] || ents.hp[v] <= 0 {
        return false;
    }
    // movement.SPAWN_PATHFIND_BODY = untouchable: no area, roll or strike reaches a unit under ground
    // (entity.rs `underground`; client 15.535.29: Zap and Arrows left a Miner and a dig as in the control).
    #[cfg(not(clash_plant = "tunnel_targetable"))]
    if calib.spawn_pathfind_body == crate::state::SpawnPathfindBody::Untouchable && ents.underground(v) {
        return false;
    }
    // rider.TARGETABLE_WHILE_ATTACHED = untargetable_immune: no area, strike or knockback of a spell reaches an
    // attached rider (target.rs `rider_untouchable`).
    if crate::target::rider_untouchable(calib, ents, v) {
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
    if ents.in_air(v) {
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
    // A flier a Vines catch holds to the ground is a ground unit here (entity.rs `in_air`, spells.AIR_TO_GROUND_WINDOW).
    if ents.in_air(v) {
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
    // An attached rider is never pushed under any arm: it stands where its mount stood (entity.rs `attached`).
    if e.attached(v) {
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
    e.deploy_ms[v] <= 0 || ctx.calib.knock_affects_deploying
}

/// ONE PUSH FROM A POINT, for a hit that is not a spell's impact: a troop's straight shot
/// (combat.rs `straight_hits`, calibration combat.RANGE_PROJECTILE = straight_to_range),
/// which pushes its victim radially from the projectile's centre. The same eligibility
/// (`pushable`) and the same two laws `impact` runs: under client16402 the ladder armed
/// from `centre` in native units with Pushback, under fixed_distance the displacement
/// along the radial with `impact`'s zero-vector fallback.
///
/// `now`: the ladder's first step is taken on the hit's own tick (`Knock::Push`), which a combo's melee pushback
/// does (knockback.COMBO_PUSHBACK); false for every other caller. The fixed_distance law has no ladder to step.
pub(crate) fn push_from(ctx: &SpellCtx, team: Team, v: usize, centre: Vec2, k: &KnockbackDef, now: bool, fx: &mut EffectBuffer) {
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
        KnockLaw::Client16402 => fx.knocks.push(Knock::Push { id, src: Vec2::new(centre.x / K, centre.y / K), strength: k.distance / K, caster: team, now }),
    }
}

/// status.ATTRACT_ONSET = client_next_tick, and `hit` pulls (one of its buffs carries an AttractPercentage): the area is
/// kept one tick past its life for its last pull (`step_spells`). False under area_first_tick and for every other area.
fn attract_lags(ctx: &SpellCtx, hit: &SpellHit) -> bool {
    ctx.calib.attract_onset == crate::state::AttractOnset::ClientNextTick
        && [hit.buff, hit.buff2].into_iter().flatten().any(|b| ctx.cards.buffs.get(b.buff as usize).is_some_and(|d| d.attract_pct != 0))
}

/// A PULSING AREA'S CLOCK at one application (status.AREA_BUFF_SOURCE_BINDING): where it stands,
/// its age -- TICK_MS on its landing tick, one TICK_MS more every tick after -- the life it has
/// left, this tick included, and its HitSpeed, the period between two of its applications (0 for
/// an area that applies once).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AreaClock {
    pub pos: Vec2,
    pub age_ms: i32,
    pub left_ms: i32,
    pub hit_speed_ms: i32,
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
/// CapBuffTimeToAreaEffectTime (the Earthquake's and the Rage's areas): an application lasts no
/// longer than the area's life left AFTER this tick plus one HitSpeed, the time at which the
/// area's next application would fall. Buffs expire under status.BUFF_EXPIRY_TICK_ALIGNMENT =
/// ceil_from_next_tick. Measured on two areas:
///   - the Earthquake (HitSpeed 100, client 15.535.29): its last application, on the area's
///     final tick (no life left after it), lasts 100 ms, and the last slowed step is L + 61,
///     where the bare remaining life gives L + 60 and BuffTime L + 78;
///   - the Rage (HitSpeed 300, client 16.402, 8 units over 4 casts): its last application, on
///     the cast + 94, has 250 ms left after that tick and lasts 550 ms, so a unit that stays
///     inside takes its last raged step on the cast + 105 (BuffTime would give 114).
///
/// The earlier rule, the life left this tick included plus one tick, was fitted to the
/// Earthquake alone: it gives the same 100 ms there and 350 ms on the Rage, whose last raged
/// step then fell on the cast + 101. The two rules part wherever HitSpeed is not 100, by up
/// to HitSpeed - 100 ms, on every application late enough for either cap to bind, not only
/// the last one:
///   - the eight HitSpeed 300 areas (the Rage and the other Rage-type areas, and the evolved
///     Princess's two slowing areas) up to four ticks longer. A unit that walks out of a Rage
///     after its application on the cast + 82 (900 ms left: 1,000 ms against 950) ends a
///     tick later, as three Skeletons of the 16.402 corpus do (cast + 104, 103 and 102);
///   - Event_HolidayFeast_AEO (HitSpeed 500) up to 400 ms longer;
///   - the six areas that apply every tick (HitSpeed 50) one tick shorter.
///
/// Only the Rage's and the Earthquake's are measured.
///
/// ControlsBuff / ControlledByParent: the slot is bound to the area (`BuffSlot::source`) and
/// taken away when the area ends (state.rs `release_orphaned_buffs`).
fn area_bound(ctx: &SpellCtx, hit: &SpellHit, b: BuffApply, area: Option<AreaClock>) -> (i32, Option<i32>, Option<Vec2>) {
    let Some(a) = area else { return (b.time_ms, None, None) };
    if ctx.calib.area_buff_source_binding != AreaBuffSourceBinding::ClientSourceBound {
        return (b.time_ms, None, None);
    }
    let Some(def) = ctx.cards.buffs.get(b.buff as usize).copied() else { return (b.time_ms, None, None) };
    #[cfg(not(any(clash_plant = "area_cap_unread", clash_plant = "area_cap_one_tick")))]
    let time_ms = if hit.caps_buff_time { b.time_ms.min(a.left_ms - ctx.calib.tick_ms + a.hit_speed_ms.max(ctx.calib.tick_ms)) } else { b.time_ms };
    #[cfg(clash_plant = "area_cap_one_tick")]
    let time_ms = if hit.caps_buff_time { b.time_ms.min(a.left_ms + ctx.calib.tick_ms) } else { b.time_ms }; // PLANT (regression): the cap is the life left plus one tick, the Earthquake-only fit.
    #[cfg(clash_plant = "area_cap_unread")]
    let time_ms = b.time_ms; // PLANT (regression): CapBuffTimeToAreaEffectTime unread, the buff lives BuffTime.
    #[cfg(not(clash_plant = "hit_tick_own_clock"))]
    let first_pulse_ms = (def.hit_tick_from_source && def.hit_frequency_ms > 0).then(|| def.hit_frequency_ms - a.age_ms.rem_euclid(def.hit_frequency_ms));
    #[cfg(clash_plant = "hit_tick_own_clock")]
    let first_pulse_ms = None; // PLANT (regression): HitTickFromSource unread, the buff pulses on its own clock.
    let source = (hit.controls_buff && def.controlled_by_parent).then_some(a.pos);
    (time_ms, first_pulse_ms, source)
}

/// WHAT ONE PULSE OF BUFF `b` DEALS A CROWN TOWER when its row sets CrownTowerDamagePerHit (`BuffDef::crown_hit`),
/// hung by card `card` at unified `level`: status.CROWN_TOWER_DAMAGE_PER_HIT_SCALING = level_scaled scales it like
/// every other level-1 figure (the Goblin Curse's 4 is 10 at level 11), unscaled takes it as the row ships it. 0 for a
/// buff without the column, whose pulse takes the crown-tower percent (state.rs `buff_pulse_pass`). The one site this is
/// computed.
fn crown_pulse(ctx: &SpellCtx, card: u16, level: i32, b: BuffApply) -> i32 {
    let per_hit = ctx.cards.buffs.get(b.buff as usize).map_or(0, |d| d.crown_hit);
    if per_hit <= 0 {
        return 0;
    }
    match ctx.calib.crown_per_hit_scaling {
        // Level validated when the cast was accepted.
        CrownPerHitScaling::LevelScaled => ctx.cards.scaled(card, level, per_hit).unwrap_or(per_hit),
        CrownPerHitScaling::Unscaled => per_hit,
    }
}

/// spells.CROWN_TOWER_SPELL_REACH = client_square_1000_strict: the half-side of a crown tower's square, native, both
/// kinds (parity, round 9 item 35: a king square of 1,400 would have taken a Zap the client's king did not).
pub const CROWN_SQUARE_HALF_MILLI: i32 = 1000;

/// The distance from a crown tower's centre to its square's corner, native, rounded up (the neighbour query's reach).
const CROWN_SQUARE_CORNER_MILLI: i32 = 1415;

/// Whether an impact of `radius` at `centre` reaches the crown tower at `tower` under client_square_1000_strict: the
/// distance from `centre` to the square of half-side `CROWN_SQUARE_HALF_MILLI` round `tower` is STRICTLY below
/// `radius`. The client's casts on the square's edge miss (a Rocket 2000 from it, three Freezes 3000 from it).
pub fn in_crown_square(centre: Vec2, tower: Vec2, radius: i32) -> bool {
    in_square(centre, tower, CROWN_SQUARE_HALF_MILLI * K, radius)
}

/// Whether an impact of `radius` at `centre` reaches the square of half-side `half` (subtiles) round `at`: the distance
/// from `centre` to the square is STRICTLY below `radius`. A crown tower's (`in_crown_square`) and, under
/// spells.BUILDING_SPELL_REACH = client_square_radius_strict, an ordinary building's, whose half-side is its collision
/// radius.
pub fn in_square(centre: Vec2, at: Vec2, half: i32, radius: i32) -> bool {
    let half = half as i64;
    let dx = ((centre.x as i64) - (at.x as i64)).abs().saturating_sub(half).max(0);
    let dy = ((centre.y as i64) - (at.y as i64)).abs().saturating_sub(half).max(0);
    dx * dx + dy * dy < (radius as i64) * (radius as i64)
}

/// A hit's crown-tower damage of its own (`SpellHit::tower_damage`), or None for the percent path. Plant
/// tower_damage_unread: the 16.402 value is ignored and the percent applies (Zap takes 100 % on a 16.402 table).
fn tower_damage_of(hit: &SpellHit) -> Option<i32> {
    #[cfg(not(clash_plant = "tower_damage_unread"))]
    return hit.tower_damage;
    #[cfg(clash_plant = "tower_damage_unread")]
    {
        let _ = hit;
        None // PLANT: the crown tower's own damage unread.
    }
}

/// Apply one circular impact of `hit` at `centre` for `team`, of card `card` cast at unified `level`. `damage` is
/// level-scaled. `area`: the clock of the pulsing area this impact is one application of (`area_bound`), None for every
/// other impact. Each buff the impact hangs (`buff`, then `buff2`) carries `level` (`BuffHit::src_level`) and its
/// crown-tower pulse (`crown_pulse`).
#[allow(clippy::too_many_arguments)]
fn impact(ctx: &SpellCtx, team: Team, card: u16, level: i32, centre: Vec2, hit: &SpellHit, damage: i32, pulse: i32, dmg: &mut DamageBuffer, fx: &mut EffectBuffer, nb: &mut Vec<u32>, area: Option<AreaClock>) {
    let e = ctx.ents;
    // `buff` with the spell's pulse, then `buff2`, which never pulses (the loader puts the pulsing one first).
    #[cfg(not(clash_plant = "curse_mark_dropped"))]
    let second = hit.buff2;
    #[cfg(clash_plant = "curse_mark_dropped")]
    let second: Option<BuffApply> = None; // PLANT: the impact never hangs its second buff.
    let hung: [(Option<BuffApply>, i32); 2] = [(hit.buff, pulse), (second, 0)];
    let application = |id: EntityId, b: BuffApply, pulse: i32| -> BuffHit {
        let (time_ms, first_pulse_ms, source) = area_bound(ctx, hit, b, area);
        BuffHit { first_pulse_ms, source, src_level: level, crown_amount: crown_pulse(ctx, card, level, b), ..BuffHit::plain(id, b.buff, time_ms, pulse) }
    };
    // spells.CROWN_TOWER_SPELL_REACH = client_square_1000_strict reaches a tower's corner, 1415 from its centre.
    let square = ctx.calib.crown_tower_spell_reach == CrownTowerSpellReach::Square1000Strict;
    // spells.BUILDING_SPELL_REACH = client_square_radius_strict reaches an ordinary building's corner, its radius x 1.415.
    let building_square = ctx.calib.building_spell_reach == BuildingSpellReach::SquareRadiusStrict;
    let wide = if square { ctx.hash.max_radius().max(CROWN_SQUARE_CORNER_MILLI * K) } else { ctx.hash.max_radius() };
    let wide = if building_square { wide.max(ctx.hash.max_radius() * 1415 / 1000 + 1) } else { wide };
    ctx.hash.neighbours_within(e, centre, hit.radius + wide, nb);
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
    // THE EVO CANNON'S BARRAGE (card.rs `BarrageDef`): its buff lands as a mark, and a bomb passes a unit that carries
    // it or took it from another bomb this tick, so no unit is hit twice (measured on client 15.535.29: one hit of 281
    // at level 11 per unit, none twice). On a crown tower a bomb deals the buff's CrownTowerDamagePerHit, level-scaled
    // (unmeasured).
    let barrage = ctx.cards.get(card).evo.as_ref().and_then(|v| v.barrage.as_ref());
    let mark = barrage.and(hit.buff).map(|b| b.buff);
    for &v in nb.iter() {
        let v = v as usize;
        if !eligible(e, v, team, hit, ctx.calib) {
            continue;
        }
        if let Some(m) = mark {
            let id = e.id_of(v);
            if e.buff_slots(v).iter().any(|s| s.id == m + 1) || fx.buffs.iter().any(|h| h.target == id && h.buff == m) {
                continue;
            }
        }
        // PLANT invisible_area_immune (tests/invisibility.rs): a unit invisible when idle (the Royal Ghost) is out of
        // every area, where the client lands area damage on it (AllowAreaDmgWhenInvisible).
        #[cfg(clash_plant = "invisible_area_immune")]
        if ctx.cards.get(e.card[v]).invisible_when_idle.is_some() {
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
        // spells.BARRAGE_REACH = data_radius_edge: a barrage bomb reaches as any area does, its projectile's own Radius
        // (card.rs `BARRAGE_DATA_RADIUS_MILLI`) read by spells.AOE_HIT_TEST; under centre_2500 it reaches 2500 centre to
        // centre (`BARRAGE_REACH_MILLI`).
        #[cfg(not(clash_plant = "barrage_reach_centre_2500"))]
        let data_edge = barrage.is_some() && ctx.calib.barrage_reach == BarrageReach::DataRadiusEdge;
        #[cfg(clash_plant = "barrage_reach_centre_2500")]
        let data_edge = false; // PLANT: the bomb reaches 2500 centre to centre under the new arm too.
        let edge = if barrage.is_some() && !data_edge { 0 } else { edge };
        let reach = if data_edge { crate::fixed::milli(crate::card::BARRAGE_DATA_RADIUS_MILLI) } else { hit.radius };
        // spells.CROWN_TOWER_SPELL_REACH = client_square_1000_strict: a crown tower is a square, reached strictly.
        #[cfg(not(clash_plant = "crown_tower_spell_disc"))]
        let tower_square = square && barrage.is_none() && e.kind[v].is_crown_tower();
        #[cfg(clash_plant = "crown_tower_spell_disc")]
        let tower_square = false; // PLANT (regression): a crown tower is a disc to every spell, whatever the arm.
        // spells.BUILDING_SPELL_REACH = client_square_radius_strict: an ordinary building (not a crown tower) is a
        // square of half-side its collision radius, reached strictly, as a crown tower is (measured on client 15.535.29,
        // item 57: a Fireball 3,162 from a Cannon's centre killed it, the square 2,433 away, where the disc misses at
        // 3,100). A troop keeps the disc.
        #[cfg(not(clash_plant = "building_spell_disc"))]
        let on_building_square = building_square && barrage.is_none() && e.kind[v] == EntityKind::Building;
        #[cfg(clash_plant = "building_spell_disc")]
        let on_building_square = false; // PLANT (regression): an ordinary building is a disc to every spell, whatever the arm.
        let reached = if tower_square {
            in_crown_square(centre, e.pos[v], hit.radius)
        } else if on_building_square {
            in_square(centre, e.pos[v], e.radius[v], hit.radius)
        } else {
            in_range_edge(centre, e.pos[v], reach, edge)
        };
        if !reached {
            continue;
        }
        let id = e.id_of(v);
        if let (Some(b), true) = (barrage, e.kind[v].is_crown_tower()) {
            let amount = ctx.cards.scaled(card, level, b.crown_damage).unwrap_or(b.crown_damage);
            if amount > 0 {
                dmg.hits.push(Hit { target: id, amount, ignores_hide: false, own: false });
            }
        } else if let (Some(tower), true) = (tower_damage_of(hit), e.kind[v].is_crown_tower()) {
            // THE CROWN TOWER'S OWN DAMAGE (16.402 tables: `SpellHit::tower_damage`, Zap 19, Freeze 15): level-1, scaled
            // by the caster's level as the damage is (the Evo Cannon's barrage scales its crown damage the same way),
            // in place of `crown_pct` of the damage.
            let amount = ctx.cards.scaled(card, level, tower).unwrap_or(tower);
            if amount > 0 {
                dmg.hits.push(Hit { target: id, amount, ignores_hide: false, own: false });
            }
        } else if damage > 0 {
            #[cfg(not(clash_plant = "crown_pct_ignored"))]
            let pct = hit.crown_pct;
            #[cfg(clash_plant = "crown_pct_ignored")]
            let pct = 100; // PLANT: crown towers take full spell damage.
            dmg.hits.push(Hit { target: id, amount: damage_against(e.kind[v], damage, pct, ctx.calib.crown_rounding), ignores_hide: false, own: false });
        }
        if hit.buff.is_some() || second.is_some() {
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
                    for (b, p) in hung {
                        if let Some(b) = b {
                            fx.buffs.push(application(id, b, p));
                        }
                    }
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
                    KnockLaw::Client16402 => fx.knocks.push(Knock::Push { id, src: Vec2::new(centre.x / K, centre.y / K), strength: k.distance / K, caster: team, now: barrage.is_some() }),
                }
            }
        }
    }
    if let Some((_, id)) = primary {
        for (b, p) in hung {
            if let Some(b) = b {
                fx.buffs.push(application(id, b, p));
            }
        }
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
fn strike(ctx: &SpellCtx, team: Team, card: u16, level: i32, damage: i32, def: &StrikeDef, pos: Vec2, struck: &mut Vec<EntityId>, launched: &mut Vec<Projectile>, nb: &mut Vec<u32>) {
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
        firer: None,
        deflected: false,
        straight: None,
        hook: None,
        carrier: None,
        fixed: false,
        release: None,
        buff_first: false,
        src_level: level,
        enchant: None,
        bonus: 0,
        bonus_crown: 0,
        trail: None,
        chain: None,
        bounce: None,
    });
}

/// THE FIRST HOP OF A DEATH PROJECTILE THAT HOPS ON (card.rs `HopDef`, the Super Lava Hound's fire wall), launched where
/// its carrier landed (`at`) on the carrier's landing tick: a shot of the hop row standing there, aimed `range` along its
/// owner's forward axis, fixed, at the hop row's speed, splashing the hop row's damage (on its own rarity's ladder at the
/// dead unit's level) over its radius. Its BounceHop (combat.rs, the Evo Bomber's) carries the landings left after it,
/// the line (from `at`) and `hit`, the units the carrier hit, which every landing of this chain spares. It first steps on
/// the next tick, as every shot a spell launches does (state.rs `phase_projectile`). Measured on client 15.535.29
/// (sp-event-SuperLavaHound-s0): the hops stood on the death point on the carrier's landing tick, stepped 600 from the
/// next, landed 2000 on 4 ticks later, and the second landing spared the tower the first had hit.
#[allow(clippy::too_many_arguments)]
fn launch_hop(ctx: &SpellCtx, team: Team, card: u16, level: i32, hd: &crate::card::HopDef, at: Vec2, hit: Vec<crate::EntityId>, launched: &mut Vec<Projectile>) {
    #[cfg(not(clash_plant = "hop_on_card_ladder"))]
    let damage = ctx.cards.rarity_scaled(&hd.rarity, level, hd.damage);
    #[cfg(clash_plant = "hop_on_card_ladder")]
    let damage = ctx.cards.scaled(card, level, hd.damage); // PLANT: the hop on the dead unit's ladder.
    let Ok(damage) = damage else { return };
    let aim = Vec2::new(at.x, at.y + forward_dy(team) * hd.range);
    launched.push(Projectile {
        team,
        pos: at,
        // No unit: the hop flies to its point (`fixed`) and splashes there.
        target: crate::EntityId { index: u32::MAX, generation: 0 },
        aim,
        speed: (hd.speed * ctx.calib.projectile_speed_to_subtiles_per_tick).max(1),
        damage,
        crown_pct: hd.crown_pct,
        splash: hd.radius,
        hits_air: hd.hits_air,
        hits_ground: hd.hits_ground,
        frac: Vec2::default(),
        fresh: false,
        buff: None,
        pulse: 0,
        firer_card: Some(card),
        firer: None,
        deflected: false,
        straight: None,
        hook: None,
        carrier: None,
        fixed: true,
        release: None,
        buff_first: false,
        src_level: level,
        enchant: None,
        bonus: 0,
        bonus_crown: 0,
        trail: None,
        chain: None,
        bounce: Some(crate::combat::BounceHop { left: hd.count.saturating_sub(1), range: hd.range, from: at, hit }),
    });
}

/// THE CANDIDATES OF AN ACTION'S SELECTOR at `pos` (`StrikeDef::selector`: the Vines' catches, the Void's strikes), by
/// index in ascending `team_seq` (one team, so the order is the creation order and the same for both seats):
/// - every unit `eligible` admits for the strike's hit (alive, an enemy of `team`, air or ground, not under ground
///   under the untouchable body, not an attached rider);
/// - of a kind the filter takes (troops always; buildings, the princess towers and the king tower by its flags), and
///   not left out by its flags: FilterHidden a building hidden under ground, FilterUnderground a unit under ground,
///   FilterDashImmune a unit whose dash makes it immune, the UNTARGETABLE tag a formation member waiting out its deploy
///   stagger (the predicate target.rs `can_target` reads for it). An invisible unit is taken: no filter sets
///   FilterInvisible (spells.TARGET_FILTER_ABSENT_FLAG);
/// - within the strike's radius plus the unit's own radius of `pos`, on this tick's post-move position
///   (spells.SELECTOR_REACH = radius_plus_target_radius; measured on client 15.535.29: the Vines caught a Skeleton at
///   centre 2633-2718 from their 2500 circle, the Void hit a Knight at 2874-2993 and missed it at 3352).
fn selector_candidates(ctx: &SpellCtx, team: Team, def: &StrikeDef, sel: &SelectorDef, pos: Vec2, nb: &mut Vec<u32>) -> Vec<usize> {
    let e = ctx.ents;
    let f = sel.filter;
    #[cfg(not(clash_plant = "void_counts_hidden"))]
    let skip_hidden = f.skip_hidden;
    #[cfg(clash_plant = "void_counts_hidden")]
    let skip_hidden = false; // PLANT: FilterHidden not read, so the Void counts and strikes a hidden Tesla.
    ctx.hash.neighbours_within(e, pos, def.hit.radius + ctx.hash.max_radius(), nb);
    let mut out: Vec<usize> = nb
        .iter()
        .map(|&v| v as usize)
        .filter(|&v| {
            if !eligible(e, v, team, &def.hit, ctx.calib) {
                return false;
            }
            let kind_taken = match e.kind[v] {
                EntityKind::Troop => true,
                EntityKind::Building => f.buildings,
                EntityKind::PrincessTower => f.princess_towers,
                EntityKind::KingTower => f.king_tower,
            };
            if !kind_taken {
                return false;
            }
            if (skip_hidden && e.hide[v] == HideState::Hidden)
                || (f.skip_underground && e.underground(v))
                || (f.skip_dash_immune && e.dash_immune(v, ctx.tick))
                || (f.skip_untargetable && ctx.calib.formation_stagger_wait == StaggerWait::Client16402 && e.stagger_ms[v] > 0)
            {
                return false;
            }
            #[cfg(not(clash_plant = "selector_centre_in_radius"))]
            let edge = e.radius[v];
            #[cfg(clash_plant = "selector_centre_in_radius")]
            let edge = 0; // PLANT: the refuted centre-in-radius reach.
            in_range_edge(pos, e.pos[v], def.hit.radius, edge)
        })
        .collect();
    out.sort_by_key(|&v| e.team_seq[v]);
    out
}

/// A SELECTOR'S BUFF ON ONE VICTIM (the Vines' snare, a Void tier): `b` for its time, with the pulse and the
/// crown-tower pulse the caster's level gives it (status.BUFF_PULSE_AMOUNT, `crown_pulse`), into this tick's effect
/// buffer, so it lands in this tick's Resolve and pulses by status.BUFF_PULSE_TIMING from there: a Void tier (one
/// 100 ms pulse) deals its damage two ticks after its strike, a Vines snare 20 and 40 ticks after its catch.
/// `reach_hidden`: the buff lands on a building hidden under ground too (a selector whose filter does not leave one
/// out: the Vines hold an idle Tesla; `BuffHit::reach_hidden`).
fn deliver(ctx: &SpellCtx, card: u16, level: i32, target: EntityId, b: BuffApply, reach_hidden: bool, fx: &mut EffectBuffer) {
    let Some(def) = ctx.cards.buffs.get(b.buff as usize) else { return };
    // Level validated when the cast was accepted.
    let pulse = def.pulse_amount(ctx.calib.buff_pulse_amount, |m| ctx.cards.scaled(card, level, m)).unwrap_or(0);
    fx.buffs.push(BuffHit { src_level: level, crown_amount: crown_pulse(ctx, card, level, b), reach_hidden, ..BuffHit::plain(target, b.buff, b.time_ms, pulse) });
}

/// ONE CATCH OF THE VINES (`StrikePick::RankedCatches`; `k` catches done before it). Measured on client 15.535.29 (the
/// Vines runs with their controls, level 11, C the cast tick):
/// - the catches fall on C + 18, C + 19 and C + 21 (the start delay 900 plus the Delays 0, 50, 150, on the striking
///   area's clock, spells.STRIKE_TIMER_LEFTOVER = carried);
/// - each takes the candidate (`selector_candidates`) with the highest current hp plus shield that this cast has not
///   caught (a crown tower 3052 before a Mortar before a Cannon; a Dark Prince of 1200 + 240 shield before an Elite
///   Barbarian of 1341), the earlier created on a tie, picked again at each catch from the units in reach then
///   (spells.MULTI_CATCH_RANKING = repick_each_catch: a Skeleton inside at the first catch and outside at the third's
///   tick was not caught);
/// - the snare holds the victim from the next tick for its 2000 ms and pulses 153 on C' + 20 and C' + 40 (C' the catch
///   tick), 35 a pulse on a crown tower (its CrownTowerDamagePerHit, level scaled); a hidden Tesla is caught, held and
///   pulsed, and stays hidden;
/// - a caught flier is a ground unit for the air-to-ground window (`EffectBuffer::grounds`, spells.AIR_TO_GROUND_WINDOW):
///   a Knight, which attacks ground only, targeted and hit a caught Balloon and let it go on C' + 43.
///
/// No candidate: nothing is caught, and the catch is spent.
#[allow(clippy::too_many_arguments)]
fn catch(ctx: &SpellCtx, team: Team, card: u16, level: i32, def: &StrikeDef, sel: &SelectorDef, pos: Vec2, k: u8, life_ms: i32, struck: &mut Vec<EntityId>, fx: &mut EffectBuffer, nb: &mut Vec<u32>) {
    let e = ctx.ents;
    let Some(&snare) = sel.buffs.first() else { return };
    let candidates = selector_candidates(ctx, team, def, sel, pos, nb);
    #[cfg(not(clash_plant = "vines_ranked_once"))]
    let _ = (k, life_ms);
    // PLANT (vines_ranked_once): the order is the first catch's, so a unit that was not on the board at the first
    // catch is never caught by a later one.
    #[cfg(clash_plant = "vines_ranked_once")]
    let first_catch_tick = {
        let tick = ctx.calib.tick_ms.max(1);
        let cast = (ctx.tick + 1).saturating_sub(((def.life_ms - life_ms) / tick) as u32);
        cast + (def.gaps_ms[0] / tick) as u32
    };
    // (current hp plus shield, -team_seq, index): the greatest wins.
    let mut best: Option<(i64, i64, usize)> = None;
    for v in candidates {
        let id = e.id_of(v);
        if struck.binary_search(&id).is_ok() {
            continue;
        }
        #[cfg(clash_plant = "vines_ranked_once")]
        if k > 0 && e.spawn_tick[v] > first_catch_tick {
            continue;
        }
        #[cfg(not(clash_plant = "vines_rank_ignores_shield"))]
        let rank = e.hp[v] as i64 + e.shield[v].max(0) as i64;
        #[cfg(clash_plant = "vines_rank_ignores_shield")]
        let rank = e.hp[v] as i64; // PLANT: the shield left out of the rank.
        let key = (rank, -(e.team_seq[v] as i64), v);
        if best.map_or(true, |b| (key.0, key.1) > (b.0, b.1)) {
            best = Some(key);
        }
    }
    let Some((_, _, v)) = best else { return };
    let id = e.id_of(v);
    if let Err(at) = struck.binary_search(&id) {
        struck.insert(at, id);
    }
    deliver(ctx, card, level, id, snare, !sel.filter.skip_hidden, fx);
    if let (true, Some(g)) = (e.flying[v], sel.ground) {
        let window = match ctx.calib.air_to_ground_window {
            AirToGroundWindow::TotalPlusBothTransitions => g.total_ms + 2 * g.transition_ms,
            AirToGroundWindow::TotalDuration => g.total_ms,
        };
        fx.grounds.push((id, window));
    }
}

/// The tier a count of `n` falls in (spells.COUNT_TIER_RULE = by_count): the first whose limit it is at most, else
/// the last (the Void's [1, 4]: one alone, two to four, five and more).
fn tier_of(n: i32, limits: &[i32]) -> usize {
    #[cfg(not(clash_plant = "void_tier_threshold_shifted"))]
    let within = |l: i32| n <= l;
    #[cfg(clash_plant = "void_tier_threshold_shifted")]
    let within = |l: i32| n < l; // PLANT: a count equal to a limit takes the next tier.
    limits.iter().position(|&l| within(l)).unwrap_or(limits.len())
}

/// ONE STRIKE OF THE VOID (`StrikePick::CountTiers`). Measured on client 15.535.29 (the Void runs with their
/// controls, level 11, C the cast tick):
/// - the strikes fall on C + 30, C + 54 and C + 78 (the start delay 500 plus FirstHitDelay 1000, then HitFrequency
///   1200) and their damage lands two ticks later, on C + 32, C + 56 and C + 80 (`deliver`);
/// - every candidate (`selector_candidates`) at that strike takes the buff of the tier its COUNT falls in
///   (spells.COUNT_TIER_RULE = by_count): alone 696 (a Giant, a Cannon, a Knight), 97 on a crown tower; with two, 294
///   each (51 on a crown tower); with four, 294 each; with five, 153 each. The rank reading (the first the most) is
///   refuted at five;
/// - buildings and a crown tower are counted and struck; an idle Tesla hidden under ground is neither (FilterHidden).
///
/// No candidate: nothing happens, and the strike is spent.
#[allow(clippy::too_many_arguments)]
fn laser(ctx: &SpellCtx, team: Team, card: u16, level: i32, damage: i32, def: &StrikeDef, sel: &SelectorDef, pos: Vec2, dmg: &mut DamageBuffer, fx: &mut EffectBuffer, nb: &mut Vec<u32>) {
    let e = ctx.ents;
    let candidates = selector_candidates(ctx, team, def, sel, pos, nb);
    if candidates.is_empty() {
        return;
    }
    let tier = tier_of(candidates.len() as i32, &sel.limits);
    let Some(&b) = sel.buffs.get(tier) else { return };
    #[cfg(not(clash_plant = "void_area_damage_loaded"))]
    let _ = (damage, &dmg);
    for v in candidates {
        let id = e.id_of(v);
        // PLANT (void_area_damage_loaded): the area's own Damage, read onto the strike by the loader, lands on each victim.
        #[cfg(clash_plant = "void_area_damage_loaded")]
        dmg.hits.push(Hit { target: id, amount: damage_against(e.kind[v], damage, def.hit.crown_pct, ctx.calib.crown_rounding), ignores_hide: false, own: false });
        deliver(ctx, card, level, id, b, !sel.filter.skip_hidden, fx);
    }
}

/// spells.ROLLING_HIT_SHAPE = client15535_max_y_edge_open: `Shape::covers_disc` on the swept rectangle `b`, except that
/// its max-y edge is exclusive -- a disc whose centre is at or beyond that edge is hit only when strictly within its
/// radius of the rectangle. So a roll toward +y (side 0) misses a victim its front edge only touches, and a roll
/// toward -y (side 1), whose front is the min-y edge, hits it.
fn covers_disc_max_y_open(b: Rect, p: Vec2, r: i32) -> bool {
    let q = Vec2::new(p.x.clamp(b.min.x, b.max.x), p.y.clamp(b.min.y, b.max.y));
    let (d2, r2) = (p.dist2(q), (r as i64) * (r as i64));
    if p.y >= b.max.y {
        d2 < r2
    } else {
        d2 <= r2
    }
}

/// One tick of a rolling projectile: move, sweep, hit each new victim once.
/// Returns true while it still has distance to roll. `still`: the landing tick's sweep of
/// spells.ROLL_FIRST_STEP = client15535_hit_on_landing_tick_step_after -- a step of 0, so the
/// rectangle is the roll's own cross-section on the landing point.
#[allow(clippy::too_many_arguments)]
fn roll(ctx: &SpellCtx, team: Team, card: u16, damage: i32, pos: &mut Vec2, travelled: &mut i32, len: i32, hit_set: &mut Vec<EntityId>, dmg: &mut DamageBuffer, fx: &mut EffectBuffer, nb: &mut Vec<u32>, still: bool) -> bool {
    let Some(crate::card::SpellDef { shape: SpellShape::Rolling { speed, half_width, half_depth, hit, .. }, .. }) = &ctx.cards.get(card).spell else {
        return false;
    };
    let (speed, half_width, half_depth) = (*speed, *half_width, *half_depth);
    let e = ctx.ents;
    let fwd = forward_dy(team);
    let step = if still { 0 } else { (speed * ctx.calib.projectile_speed_to_subtiles_per_tick).min(len - *travelled).max(0) };
    let prev = *pos;
    #[cfg(not(clash_plant = "rolling_forward_plus_y_for_both"))]
    let cur = Vec2::new(prev.x, prev.y + fwd * step);
    #[cfg(clash_plant = "rolling_forward_plus_y_for_both")]
    let cur = Vec2::new(prev.x, prev.y + step); // PLANT: rolls toward +y for both seats.
    *pos = cur;
    *travelled += step;
    // The swept rectangle: the roll's cross-section from its previous to its current
    // centre, extended by the half-depth fore and aft. A closed Rect, so a victim
    // touching it is hit (spells.ROLLING_HIT_SHAPE; client15535_max_y_edge_open
    // excludes the max-y edge) -- and swept, so the result does not depend on TICK_MS.
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
            #[cfg(not(clash_plant = "rolling_max_y_edge_closed"))]
            RollHitShape::ClientMaxYEdgeOpen => covers_disc_max_y_open(rect, e.pos[v], e.radius[v]),
            #[cfg(clash_plant = "rolling_max_y_edge_closed")]
            RollHitShape::ClientMaxYEdgeOpen => shape.covers_disc(e.pos[v], e.radius[v]), // PLANT: the max-y edge closed.
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
        dmg.hits.push(Hit { target: id, amount: damage_against(e.kind[v], damage, pct, ctx.calib.crown_rounding), ignores_hide: false, own: false });
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
                    RollHitShape::RectVsCircleEdge | RollHitShape::ClientMaxYEdgeOpen => e.radius[v],
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
                        // client15535_radial_from_tick_end_centre: away from the roll's centre at the END of this tick
                        // (measured on client 15.535.29, item 55: a Hog hit mid-roll steps (70, -131) from the centre
                        // (14500, 22500), where the contact point (14500, 22627) gives (65, -134)). It pairs with
                        // spells.ROLL_FIRST_STEP's landing-tick hit, whose tick-end centre is the landing point: a unit
                        // just ahead of the tap is hit there and goes forward.
                        #[cfg(not(clash_plant = "roll_tick_end_arm_from_contact"))]
                        RollDirection::RadialFromTickEndCentre => Vec2::new(cur.x / K, cur.y / K),
                        // PLANT (regression): the tick-end arm pushes from the contact point.
                        #[cfg(clash_plant = "roll_tick_end_arm_from_contact")]
                        RollDirection::RadialFromTickEndCentre => Vec2::new(contact.x / K, contact.y / K),
                    };
                    fx.knocks.push(Knock::Push { id, src, strength: k.distance / K, caster: team, now: false });
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
                    // The same source as the client16402 law's: the tick-end centre.
                    RollDirection::RadialFromTickEndCentre => push_along(e.pos[v].sub(cur), k.distance, Some(along)),
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
        released.push(Release { team, unit: sp.unit, level, pos: at, deploy_ms, count, source: card });
    }
}

/// actions.SUB_TICK_DELAY_ROUNDING: the whole ticks, counted from its object's creation tick, after which an action
/// due `ms` after that creation acts. floor_from_creation: floor(ms / TICK_MS), measured on client 15.535.29 (the
/// Suspicious Bush's goblins at 625 and 675 ms act 12 and 13 ticks after the death, 12 of 12); ceil_to_tick rounds up.
pub fn delay_ticks(calib: &Calib, ms: i32) -> u32 {
    let t = calib.tick_ms.max(1);
    let ms = ms.max(0);
    #[cfg(not(clash_plant = "sub_tick_delay_ceil"))]
    let rule = calib.sub_tick_delay_rounding;
    #[cfg(clash_plant = "sub_tick_delay_ceil")]
    let rule = SubTickDelayRounding::CeilToTick; // PLANT: every delay rounded up to a whole tick.
    (match rule {
        SubTickDelayRounding::FloorFromCreation => ms / t,
        SubTickDelayRounding::CeilToTick => (ms + t - 1) / t,
    }) as u32
}

/// ONE TICK OF THE EVO GIANT SNOWBALL'S BALL (card.rs `CaptureRollDef`), after its first point: it rolls `roll_speed`
/// along the owner's forward (to `roll_len`); on CAPTURE_DELAY_TICKS it strikes and takes every troop it struck; each
/// captive then steps at the ball's point by its capture distance times the tick's share, or rides on it once joined
/// (within `hide_distance`, or after the drag's last tick); on the tick after the ball's last step every captive is let
/// go with the release buff and the ball is gone. The captives' points come out in `out.carried`. Each capture hangs
/// the hold until the release's tick, so a captive stands (held: no walk, no attack) until then.
#[allow(clippy::too_many_arguments)]
fn capture_roll(
    ctx: &SpellCtx,
    team: Team,
    card: u16,
    level: i32,
    damage: i32,
    d: &crate::card::CaptureRollDef,
    pos: &mut Vec2,
    travelled: &mut i32,
    age: &mut u32,
    captives: &mut Vec<Captive>,
    dmg: &mut DamageBuffer,
    fx: &mut EffectBuffer,
    out: &mut SpellOut,
    nb: &mut Vec<u32>,
) -> bool {
    use crate::card::{CAPTURE_DELAY_TICKS, CAPTURE_DRAG_PER_10000, RELEASE_SPREAD};
    let e = ctx.ents;
    let fwd = forward_dy(team);
    let step = d.roll_speed.max(1);
    let last = ((d.roll_len + step - 1) / step) as u32;
    *age += 1;
    if *age > last {
        // THE RELEASE: every live captive let go, a lone one on the ball's point, two either side of it.
        #[cfg(not(clash_plant = "capture_never_released"))]
        {
            let mut live: Vec<(u32, EntityId)> = captives.iter().filter(|c| e.is_alive(c.id)).map(|c| (e.creation_seq[c.id.index as usize], c.id)).collect();
            live.sort_unstable();
            let n = live.len();
            for (k, (_, id)) in live.into_iter().enumerate() {
                let off = if n >= 2 && k < 2 { if k == 0 { RELEASE_SPREAD } else { -RELEASE_SPREAD } } else { 0 };
                out.carried.push((id, Vec2::new(pos.x, pos.y + fwd * off * K)));
                fx.buffs.push(BuffHit::plain(id, d.release.buff, d.release.time_ms, 0));
            }
        }
        return false;
    }
    let s = step.min(d.roll_len - *travelled).max(0);
    pos.y += fwd * s;
    *travelled += s;
    if *age == CAPTURE_DELAY_TICKS {
        // THE CAPTURE: the hit, and every troop it struck taken, held to the release's tick.
        let from = dmg.hits.len();
        impact(ctx, team, card, level, *pos, &d.hit, damage, 0, dmg, fx, nb, None);
        let hold_ms = (last + 1 - *age) as i32 * ctx.calib.tick_ms;
        let mut taken: Vec<EntityId> = Vec::new();
        for h in &dmg.hits[from..] {
            let v = h.target.index as usize;
            if e.is_alive(h.target) && e.kind[v] == EntityKind::Troop && !taken.contains(&h.target) && (taken.len() as i32) < d.max_units {
                taken.push(h.target);
            }
        }
        for id in taken {
            let v = id.index as usize;
            let d0 = isqrt(((e.pos[v].x / K - pos.x / K) as i64).pow(2) + ((e.pos[v].y / K - pos.y / K) as i64).pow(2)) as i32;
            fx.buffs.push(BuffHit::plain(id, d.hold, hold_ms, 0));
            captives.push(Captive { id, d0, joined: false });
        }
        return true;
    }
    // THE DRAG, then the ride.
    let k = age.saturating_sub(CAPTURE_DELAY_TICKS) as usize;
    for c in captives.iter_mut().filter(|c| e.is_alive(c.id)) {
        let v = c.id.index as usize;
        #[cfg(not(clash_plant = "capture_snaps_at_once"))]
        let snapped = c.joined || k > CAPTURE_DRAG_PER_10000.len();
        #[cfg(clash_plant = "capture_snaps_at_once")]
        let snapped = true; // PLANT (regression): the captive rides on the ball from the tick after the capture.
        if snapped {
            if !c.joined {
                out.joined.push(c.id);
            }
            c.joined = true;
            out.carried.push((c.id, *pos));
            continue;
        }
        let (cx, cy) = (e.pos[v].x / K, e.pos[v].y / K);
        let (dx, dy) = ((pos.x / K - cx) as i64, (pos.y / K - cy) as i64);
        let dist = isqrt(dx * dx + dy * dy);
        let len = (c.d0 as i64 * CAPTURE_DRAG_PER_10000[k - 1] as i64 / 10_000) as i32;
        let at = if dist <= len as i64 {
            *pos
        } else {
            Vec2::new((cx + (dx * len as i64 / dist) as i32) * K, (cy + (dy * len as i64 / dist) as i32) * K)
        };
        // movement.CAPTURE_DRAG_FACING = client15535_faces_ball: a dragged captive faces the ball's point from its new one
        // (client 15.535.29: 52 of 52 drag frames); on the point it keeps the last.
        // PLANT (regression) drag_keeps_walk_facing: the new arm leaves the captive's facing as it was.
        #[cfg(not(clash_plant = "drag_keeps_walk_facing"))]
        let faces = ctx.calib.capture_drag_facing == crate::state::CaptureDragFacing::Client15535FacesBall;
        #[cfg(clash_plant = "drag_keeps_walk_facing")]
        let faces = false;
        if faces && at != *pos {
            let mut f = ((pos.x - at.x) / K, (pos.y - at.y) / K);
            if crate::move16402::normalize_to(&mut f, 256) != 0 {
                out.faced.push((c.id, Vec2::new(f.0, f.1)));
            }
        }
        if at.dist(*pos) <= d.hide_distance {
            out.joined.push(c.id);
            c.joined = true;
            out.carried.push((c.id, *pos));
        } else {
            out.carried.push((c.id, at));
        }
    }
    true
}

/// Advance every spell by one tick. What the step hands on comes back in `out`
/// (`SpellOut`, drained by the caller): units released by landing spells in
/// `out.released`, in `spells` order (deterministic; team_seq is per team, and a
/// team's spells keep their cast order).
pub fn step_spells(ctx: &SpellCtx, spells: &mut Vec<Spell>, dmg: &mut DamageBuffer, fx: &mut EffectBuffer, out: &mut SpellOut, nb: &mut Vec<u32>) {
    let tick = ctx.calib.tick_ms;
    let mult = ctx.calib.projectile_speed_to_subtiles_per_tick;
    spells.retain_mut(|s| {
        // AN AREA RIDING ON A UNIT (a hero's button): its own def, not the card's spell shape. It follows its parent
        // (post-Move), stays where the parent died when its row says so, hits like a pulsing area, and makes its end
        // area where it stands on the update its life runs out (that area acts from the next tick).
        if let SpellMotion::Attached { parent, pos, part, life_ms, next_ms } = &mut s.motion {
            let Some(a) = attached_def(ctx.cards, s.card, *part) else { return false };
            let alive = ctx.ents.is_alive(*parent);
            if !alive && !a.stay {
                return false;
            }
            // combat.EVO_IMPACT_AREA_ANCHOR = client15535_follows_target: the Evo Ice Spirits' impact area rides the unit its
            // shot landed on (client 15.535.29, sp-f2-ice-s0 t752: the Hog Rider hit 4,200 off the landing point).
            // PLANT (regression) impact_area_stands: the new arm's area stays where the shot landed.
            #[cfg(not(clash_plant = "impact_area_stands"))]
            let follow = a.follow
                || (*part == crate::card::EVO_IMPACT_AREA && ctx.calib.evo_impact_area_anchor == crate::state::EvoImpactAreaAnchor::Client15535FollowsTarget);
            #[cfg(clash_plant = "impact_area_stands")]
            let follow = a.follow;
            if alive && follow {
                *pos = ctx.ents.pos[parent.index as usize];
            }
            let was_live = *life_ms > 0;
            while *next_ms <= 0 && *life_ms > 0 {
                let clock = AreaClock { pos: *pos, age_ms: a.life_ms - *life_ms + tick, left_ms: *life_ms, hit_speed_ms: a.hit_speed_ms };
                impact(ctx, s.team, s.card, s.level, *pos, &a.hit, s.damage, s.pulse, dmg, fx, nb, Some(clock));
                // HitSpeed 0: one hit, on the first update, and the area is gone.
                if a.hit_speed_ms == 0 {
                    return false;
                }
                *next_ms += a.hit_speed_ms.max(tick);
            }
            *next_ms -= tick;
            *life_ms -= tick;
            if *life_ms > 0 {
                return true;
            }
            // The update after its last: it pulled on this tick (status.ATTRACT_ONSET = client_next_tick) and goes.
            if !was_live {
                return false;
            }
            if let Some(end) = a.end {
                if let Ok(v) = attached_area(ctx.cards, ctx.calib, s.team, s.card, s.level, *parent, *pos, end) {
                    out.born.push(v);
                }
            }
            // A pulling area stays one tick past its life, applying nothing, as a standing pulsing area does
            // (`attract_lags`): its last update's pull moves its victims on the next tick.
            return attract_lags(ctx, &a.hit);
        }
        let def = ctx.cards.get(s.card);
        let Some(shape) = shape_of(def).and_then(|d| shape_at(&d.shape, s.depth)) else { return false };
        match (&mut s.motion, shape) {
            (SpellMotion::Flight { pos, aim, frac, delay_ms }, SpellShape::Projectile { speed, hit, spawn, area, .. }) => {
                // A CONTAINER (a death bomb that carries a death spawn: card.rs `Hitpointless::BombWithDeathSpawn`, the
                // Skeleton Barrel's) releases its units when its fuse ends (`FuseEnd`), and its hit and its units
                // come on the ticks spawner.DEATH_BOMB_SPAWN_TIMING names. A plain bomb and every other flight keep
                // the arrival on the tick after the delay runs out.
                let container = def.death_bomb_fuse_ms().is_some() && def.death_spawn.is_some();
                // PLANT bomb_timing_reaches_plain_bombs (tests/death_bomb.rs): the container's timing on every bomb.
                // spawner.DEATH_BOMB_TIMING_SCOPE = client15535_every_bomb: a plain bomb is timed as a container is; a bomb a
                // button drops (the Mighty Miner's, `CardDef::dropped_by_ability`) keeps the tick after its fuse.
                // PLANT (regression) ability_bomb_timed: the new arm times the Mighty Miner's bomb too.
                #[cfg(not(clash_plant = "ability_bomb_timed"))]
                let dropped = def.dropped_by_ability;
                #[cfg(clash_plant = "ability_bomb_timed")]
                let dropped = false;
                #[cfg(not(any(clash_plant = "bomb_timing_reaches_plain_bombs", clash_plant = "plain_bomb_lands_late")))]
                let timed = container
                    || (def.death_bomb_fuse_ms().is_some()
                        && !dropped
                        && ctx.calib.death_bomb_timing_scope == crate::state::DeathBombTimingScope::Client15535EveryBomb);
                #[cfg(any(clash_plant = "bomb_timing_reaches_plain_bombs", clash_plant = "plain_bomb_lands_late"))]
                let _ = dropped;
                #[cfg(clash_plant = "bomb_timing_reaches_plain_bombs")]
                let timed = def.death_bomb_fuse_ms().is_some();
                #[cfg(clash_plant = "plain_bomb_lands_late")]
                let timed = container; // PLANT (regression): the new arm's plain bomb lands on the tick after its fuse.
                #[cfg(not(clash_plant = "container_release_with_hit"))]
                let timing = ctx.calib.death_bomb_spawn_timing;
                // PLANT container_release_with_hit (tests/skeleton_barrel.rs): the old arm whatever the key says.
                #[cfg(clash_plant = "container_release_with_hit")]
                let timing = DeathBombSpawnTiming::WithTheHit;
                if *delay_ms > 0 {
                    *delay_ms -= tick;
                    if !(timed && *delay_ms <= 0) {
                        return true;
                    }
                    match timing {
                        DeathBombSpawnTiming::WithTheHit => return true,
                        // The units now, the hit on the next tick; the delay is marked so the arrival releases nothing.
                        DeathBombSpawnTiming::UnitsAtFuseEnd => {
                            if container {
                                out.fuse_ends.push(FuseEnd { team: s.team, card: s.card, level: s.level, pos: *aim });
                            }
                            *delay_ms = FUSE_RELEASED;
                            return true;
                        }
                        // Both now: the arrival below runs on this tick.
                        DeathBombSpawnTiming::AtFuseEnd => {}
                    }
                }
                // combat.PROJECTILE_STEP: exact steps with the remainder carried, or the
                // client's truncated native step (combat.rs `projectile_advance`).
                let np = crate::combat::projectile_advance(ctx.calib.projectile_step, *pos, *aim, speed * mult, frac, s.team);
                *pos = np;
                // One more tick in the air (`Spell::flown`).
                s.flown = s.flown.saturating_add(1);
                if np != *aim {
                    return true;
                }
                // A CENTRE-AIMED STRIKE'S DELIVERY (the Royal Delivery's crate, the chain's depth-1 object): its hit is
                // the projectile row's, whose buildings filter spells.AREA_PROJECTILE_IGNORE_BUILDINGS names. Measured
                // on client 15.535.29: an enemy building 2550 from the tap took the full hit, although the area row sets
                // IgnoreBuildings (projectile_row).
                let area_ignores = match shape_of(def).map(|d| &d.shape) {
                    Some(SpellShape::Strikes(d)) if d.pick == StrikePick::AreaCentre && s.depth == 1 => Some(d.hit.ignore_buildings),
                    _ => None,
                };
                #[cfg(not(clash_plant = "area_projectile_reads_area_row"))]
                let rule = ctx.calib.area_projectile_ignore_buildings;
                #[cfg(clash_plant = "area_projectile_reads_area_row")]
                let rule = AreaProjectileIgnoreBuildings::AreaRow; // PLANT: the area row's IgnoreBuildings, whatever the key.
                if let Some(h) = hit {
                    let h = match (area_ignores, rule) {
                        (Some(ignore), AreaProjectileIgnoreBuildings::AreaRow) => SpellHit { ignore_buildings: ignore, ..*h },
                        _ => *h,
                    };
                    // knockback.DEATH_PUSHBACK: a death bomb's row's DeathPushBack, on the ladder from the bomb.
                    let h = match death_bomb_push(ctx, def) {
                        Some(k) => SpellHit { knockback: Some(k), ..h },
                        None => h,
                    };
                    let first = dmg.hits.len();
                    impact(ctx, s.team, s.card, s.level, *aim, &h, s.damage, s.pulse, dmg, fx, nb, None);
                    // A DEATH PROJECTILE THAT HOPS ON (card.rs `HopDef`, the Super Lava Hound's fire wall): its first hop stands
                    // on the landing point, sparing what this landing hit (`launch_hop`).
                    #[cfg(not(clash_plant = "fire_wall_never_hops"))]
                    if let Some(hd) = def.death_hop.as_ref().filter(|_| s.depth == 0) {
                        let mut hit: Vec<crate::EntityId> = dmg.hits[first..].iter().map(|x| x.target).collect();
                        hit.sort();
                        hit.dedup();
                        launch_hop(ctx, s.team, s.card, s.level, hd, *aim, hit, &mut out.launched);
                    }
                }
                // The container's units, after its hit, unless they came out on the fuse's last tick already.
                if container && *delay_ms != FUSE_RELEASED {
                    out.fuse_ends.push(FuseEnd { team: s.team, card: s.card, level: s.level, pos: *aim });
                }
                #[cfg(not(clash_plant = "delivery_release_dropped"))]
                let released = spawn.as_ref();
                #[cfg(clash_plant = "delivery_release_dropped")]
                let released = spawn.as_ref().filter(|_| area_ignores.is_none()); // PLANT: a delivery releases nothing.
                if let Some(sp) = released {
                    release_units(ctx, s.team, s.card, s.level, sp, *aim, &mut out.released);
                }
                // A LANDING THAT MAKES AN AREA (card.rs `SpellShape::Projectile::area`, the Goblin Party Rocket): made on the
                // aim on the landing tick, it acts from the next tick, as what a fuse releases does. Level validated when the
                // cast was accepted.
                #[cfg(not(clash_plant = "party_rocket_lands_nothing"))]
                if let Some(a) = area {
                    if let Ok(v) = objects_for(ctx.cards, ctx.calib, None, s.team, s.card, s.level, s.depth + 1, a, *aim, ctx.tick) {
                        out.born.extend(v);
                    }
                }
                #[cfg(clash_plant = "party_rocket_lands_nothing")]
                let _ = area; // PLANT: the landing makes nothing.
                false
            }
            // THE EVO GIANT SNOWBALL'S FLIGHT: as a projectile spell's (combat.PROJECTILE_STEP); on the tick it arrives the
            // ball stands on the tap, its first point.
            (SpellMotion::Flight { pos, aim, frac, .. }, SpellShape::CaptureRoll(d)) => {
                let np = crate::combat::projectile_advance(ctx.calib.projectile_step, *pos, *aim, d.flight_speed * mult, frac, s.team);
                *pos = np;
                if np == *aim {
                    s.motion = SpellMotion::CaptureRoll { pos: np, travelled: 0, age: 0, captives: Vec::new() };
                }
                true
            }
            (SpellMotion::CaptureRoll { pos, travelled, age, captives }, SpellShape::CaptureRoll(d)) => {
                capture_roll(ctx, s.team, s.card, s.level, s.damage, d, pos, travelled, age, captives, dmg, fx, out, nb)
            }
            (SpellMotion::Area { pos }, SpellShape::AreaEffect { hit } | SpellShape::Echo { hit, .. }) => {
                let first_new = fx.buffs.len();
                impact(ctx, s.team, s.card, s.level, *pos, hit, s.damage, s.pulse, dmg, fx, nb, None);
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
            // THE CLONE (card.rs `SpellShape::Clone`): on its first update, which is the cast tick's, it copies every
            // own unit its hit takes (the own side, troops, air and ground; its edge within the Radius by
            // spells.AOE_HIT_TEST), except a copy (never copied again) and a unit whose row sets IgnoreClone, and hangs
            // its hold on each. The orders go to this tick's Reap (state.rs `materialise_clones`), in team_seq order.
            // Measured on client 15.535.29: it acts once, on the cast tick C (a Knight walking in on C + 6 was not
            // copied: spells.ONE_SHOT_AREA_EFFECT_APPLICATION = first_update_only), copies own troops only and never a
            // copy, and deals nothing.
            (SpellMotion::Area { pos }, SpellShape::Clone { hit, hold, .. }) => {
                let e = ctx.ents;
                ctx.hash.neighbours_within(e, *pos, hit.radius + ctx.hash.max_radius(), nb);
                let mut picked: Vec<usize> = nb
                    .iter()
                    .map(|&v| v as usize)
                    .filter(|&v| {
                        if !eligible(e, v, s.team, hit, ctx.calib) {
                            return false;
                        }
                        #[cfg(not(clash_plant = "clone_recloned"))]
                        if e.cloned[v] {
                            return false;
                        }
                        #[cfg(not(clash_plant = "ignore_clone_unread"))]
                        if ctx.cards.get(e.card[v]).ignore_clone {
                            return false;
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
                        in_range_edge(*pos, e.pos[v], hit.radius, edge)
                    })
                    .collect();
                picked.sort_by_key(|&v| e.team_seq[v]);
                // THE AREA'S OWN BUFF (the GlobalClone event's, card.rs `clone_shape_of`: the copies' hold): it lands on the
                // own units in reach the action does not copy too (an earlier copy, an IgnoreClone unit), once each.
                // PLANT (regression) global_clone_buff_on_picked_only: the area's own Buff reaches the copied units alone.
                #[cfg(not(clash_plant = "global_clone_buff_on_picked_only"))]
                if let Some(b) = hit.buff.as_ref() {
                    let edge_of = |v: usize| match ctx.calib.aoe_hit_test {
                        AoeHitTest::EdgeInclusive => e.radius[v],
                        AoeHitTest::CentreInRadius => 0,
                    };
                    let mut passed: Vec<usize> = nb
                        .iter()
                        .map(|&v| v as usize)
                        .filter(|&v| !picked.contains(&v) && eligible(e, v, s.team, hit, ctx.calib) && in_range_edge(*pos, e.pos[v], hit.radius, edge_of(v)))
                        .collect();
                    passed.sort_by_key(|&v| e.team_seq[v]);
                    for v in passed {
                        fx.buffs.push(BuffHit::plain(e.id_of(v), b.buff, b.time_ms, 0));
                    }
                }
                for v in picked {
                    let id = e.id_of(v);
                    out.clones.push(CloneOrder { src: id, level: s.level, card: s.card });
                    fx.buffs.push(BuffHit::plain(id, hold.buff, hold.time_ms, 0));
                }
                // PLANT (clone_area_lingers): the area acts on every update, so a unit that walks in later is copied, and an
                // original again.
                #[cfg(clash_plant = "clone_area_lingers")]
                return true;
                #[allow(unreachable_code)]
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
                    if let Ok(v) = objects_for(ctx.cards, ctx.calib, None, s.team, s.card, s.level, s.depth + 1, c, p.pos, ctx.tick) {
                        out.born.extend(v);
                    }
                }
                while p.next_ms <= 0 && p.life_ms > 0 {
                    let clock = AreaClock { pos: p.pos, age_ms: *total_ms - p.life_ms + tick, left_ms: p.life_ms, hit_speed_ms: *hit_speed_ms };
                    impact(ctx, s.team, s.card, s.level, p.pos, hit, s.damage, s.pulse, dmg, fx, nb, Some(clock));
                    p.next_ms += (*hit_speed_ms).max(tick);
                }
                p.next_ms -= tick;
                p.life_ms -= tick;
                // status.ATTRACT_ONSET = client_next_tick: a pulling area stays one tick past its life, applying nothing
                // (the loop above needs life left), so that its last update's pull moves its victims on the next tick
                // (state.rs `phase_path16402`); it goes on that tick's update.
                p.life_ms > 0 || (attract_lags(ctx, hit) && p.life_ms > -tick)
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
                // client15535_hit_on_landing_tick_step_after: it stands unmoved on the landing point, as under
                // tick_after_landing, and its rectangle there hits this tick (a step of 0); it steps from the next
                // tick. Measured on client 15.535.29 (item 55): a Log lands on its tap and does not move on the landing
                // tick, rolls 200 a tick from the next, and a Hog next to the tap loses its hp on the landing tick and
                // is pushed from the next.
                if first == RollFirstStep::HitOnLandingTickStepAfter {
                    #[cfg(not(clash_plant = "roll_landing_tick_unswept"))]
                    let more = roll(ctx, s.team, s.card, s.damage, &mut p, &mut travelled, len, &mut hit, dmg, fx, nb, true);
                    // PLANT (regression): the landing tick sweeps nothing, as under tick_after_landing.
                    #[cfg(clash_plant = "roll_landing_tick_unswept")]
                    let more = true;
                    s.motion = SpellMotion::Rolling { pos: p, travelled, len, hit };
                    return more;
                }
                let more = roll(ctx, s.team, s.card, s.damage, &mut p, &mut travelled, len, &mut hit, dmg, fx, nb, false);
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
                let more = roll(ctx, s.team, s.card, s.damage, pos, travelled, *len, hit, dmg, fx, nb, false);
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
            // That below-zero update is the Lightning's (`StrikePick::HighestHp`), and it does not read
            // spells.STRIKE_DUE: its client 16.402 row (500, exactly 10 ticks) strikes on the cast tick + 10 and
            // + 20 on the 16.402 corpus, one update after its clock reaches zero; the tables' 460 never brings the
            // clock to exactly zero. A centre-aimed strike (`StrikePick::AreaCentre`, the Royal Delivery) reads
            // spells.STRIKE_DUE: the update whose clock falls to zero strikes (clock_at_or_below_zero; measured on
            // client 15.535.29, 7 casts: HitSpeed 2000 is exactly 40 ticks, and the crate is made on the cast tick
            // + 39), or only one that falls below it. Its HitSpeed is its LifeDuration, so under
            // clock_at_or_below_zero it strikes on the update its life reaches 0 under either spells.STRIKE_AREA_END
            // arm. It makes its delivery on the centre, born after this tick's spells stepped, so the delivery lands
            // on the next tick.
            (SpellMotion::Strikes { pos, life_ms, next_ms, k, struck }, SpellShape::Strikes(def)) => {
                *next_ms -= tick;
                *life_ms -= tick;
                #[cfg(not(clash_plant = "strike_due_below_zero"))]
                let due_rule = ctx.calib.strike_due;
                #[cfg(clash_plant = "strike_due_below_zero")]
                let due_rule = StrikeDue::ClockBelowZero; // PLANT: the strike waits for the clock to fall below zero.
                // A selector's strike (the Vines' catches, the Void's strikes) takes the Lightning's below-zero update:
                // measured on client 15.535.29, the catches on C + 18, 19 and 21 and the Void's strikes on C + 30, 54
                // and 78 are the updates that take the clock below zero under STRIKE_TIMER_LEFTOVER = carried.
                let due = match (def.pick, due_rule) {
                    (StrikePick::AreaCentre, StrikeDue::ClockAtOrBelowZero) => *next_ms <= 0,
                    (StrikePick::HighestHp | StrikePick::RankedCatches | StrikePick::CountTiers, _) | (StrikePick::AreaCentre, StrikeDue::ClockBelowZero) => *next_ms < 0,
                };
                if due && (*k as usize) < def.gaps_ms.len() {
                    match (def.pick, def.selector.as_deref()) {
                        (StrikePick::HighestHp, _) => strike(ctx, s.team, s.card, s.level, s.damage, def, *pos, struck, &mut out.launched, nb),
                        (StrikePick::AreaCentre, _) => out.born.push(Spell {
                            team: s.team,
                            card: s.card,
                            level: s.level,
                            damage: s.damage,
                            pulse: 0,
                            motion: SpellMotion::Flight { pos: *pos, aim: *pos, frac: Vec2::default(), delay_ms: 0 },
                            depth: s.depth + 1,
                            flown: 0,
                        }),
                        (StrikePick::RankedCatches, Some(sel)) => catch(ctx, s.team, s.card, s.level, def, sel, *pos, *k, *life_ms, struck, fx, nb),
                        (StrikePick::CountTiers, Some(sel)) => laser(ctx, s.team, s.card, s.level, s.damage, def, sel, *pos, dmg, fx, nb),
                        // The loader gives a selector's pick its selector; never reached.
                        (StrikePick::RankedCatches | StrikePick::CountTiers, None) => {}
                    }
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
                if let Ok(v) = objects_for(ctx.cards, ctx.calib, None, s.team, s.card, s.level, s.depth + 1, then, *pos, ctx.tick) {
                    out.born.extend(v);
                }
                false
            }
            // A SCHEDULED AREA (the Graveyard; the Suspicious Bush's death area). Measured on client 15.535.29: entry k
            // acts on the area's creation tick + floor(delay_k / 50), the delay read from the group's start
            // (actions.SUB_ACTIONS_DELAY, actions.SUB_TICK_DELAY_ROUNDING). A cast's area is created in the Spawn phase
            // of its cast tick C and steps that same tick, so its Skeletons come on C + 44 ... C + 164 (120 of 120); a
            // death's area is created in the Reap of the death tick D and first steps on D + 1, so the Bush's goblins
            // come on D + 12 (625 ms) and D + 13 (675 ms) (12 of 12). `born` carries that difference. The area ends
            // once its LifeDuration, counted the same way, has run; an entry not yet due then never acts.
            //
            // THE AREA'S OWN SpawnCharacter (`SpawnVia::OwnSpawn`, the Tri Wizards' TriWizard) comes one tick after an
            // action of the same delay would: its clock first runs on the tick after the area's creation. Measured on
            // client 15.535.29 (sweep-TriWizards, the play's cast tick C): the TriWizard, due at SpawnInterval 300 -
            // SpawnTime 100, first stands on C + 5, and the area's two actions (SubActionsDelay 300) make the wizards'
            // areas on C + 6, which put the wizards down on C + 7 (`SpawnVia::DeployArea`, state.rs `phase_projectile`).
            (SpellMotion::Scheduled { pos, born, fired }, SpellShape::ScheduledArea { life_ms, schedule }) => {
                let age = ctx.tick.saturating_sub(*born);
                // The group's delays are its actions' alone: an own spawn keeps its own clock (`SpawnVia::OwnSpawn`).
                let delays: Vec<i32> = schedule.iter().filter(|e| e.via != SpawnVia::OwnSpawn).map(|e| e.delay_ms).collect();
                let mut a = 0usize;
                for (k, e) in schedule.iter().enumerate() {
                    let own = e.via == SpawnVia::OwnSpawn;
                    let at = a;
                    if !own {
                        a += 1;
                    }
                    #[cfg(not(clash_plant = "schedule_cumulative"))]
                    let delay = if own { e.delay_ms } else { crate::state::sub_action_delay_ms(ctx.calib, &delays, at) };
                    #[cfg(clash_plant = "schedule_cumulative")]
                    let delay: i32 = if own { e.delay_ms } else { delays[..=at].iter().sum() }; // PLANT: the delays read as gaps.
                    let late = u32::from(own);
                    if *fired & (1 << k) != 0 || delay_ticks(ctx.calib, delay) + late > age {
                        continue;
                    }
                    *fired |= 1 << k;
                    // The entry's unit at the area's level (the owner's unified level; levels validated at deploy).
                    if let Ok(level) = ctx.cards.unit_level(s.card, e.unit, None, s.level) {
                        out.scheduled.push(ScheduledRelease { team: s.team, unit: e.unit, level, centre: *pos, offset: e.offset, deploy_ms: e.deploy_time_ms, via: e.via, source: s.card });
                    }
                }
                age + 1 < delay_ticks(ctx.calib, *life_ms).max(1)
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
