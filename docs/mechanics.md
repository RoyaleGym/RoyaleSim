# Mechanics: what is modelled, what is not, and what is known to be wrong

This page is the engine's coverage inventory. It tells you which mechanics the engine models, which
it leaves out, and which ones are known to be wrong, so you can judge whether it fits what you are
building. It is written against the engine as of 2026-09-21. Where a number appears, it was
measured, and the run that produced it is named.

Three things are worth stating up front, because they bound everything below:

- **The thin slice is 18 cards** (`data/derived/cards.json`, key `thin_slice`): Knight, Archer,
  Musketeer, Giant, Hog Rider, Minions, Baby Dragon, Valkyrie, Skeleton Army, Cannon, Tesla,
  Fireball, Zap, Arrows, The Log, Prince, Wizard, Goblin Barrel. Those are the cards the engine
  has been checked on. The catalogue is much larger and loads, but see "Cards outside the slice".
- **Card stats are the 15.535.29 client's own card data** (`tools/extract_cards.py`; the 2018
  table stays beside it as `cards-2018.json`, which the format-3 snapshot fixture reads), except
  ten values measured on the 16.402 corpus that the ledger overlays on this table only
  (`cards.CLIENT16402_VALUES`).
  Movement, pathfinding, level scaling and the crown-tower ladder are all measured against
  2026 recordings (`combat.STAT_BASE_LEVEL`, `combat.TOWER_HITPOINT_LADDER`).
- **The target is behavioural fidelity plus bit-identical determinism within a build**, not a
  tick-for-tick reproduction of a real match. The real intra-tick order and the real PRNG are out
  of reach.

## Modelled

| Area | Where | Notes |
|---|---|---|
| Arena from the shipped 36x64 bitmask | `arena.rs` | water hard-blocks ground and is free for air under the earlier arm; priced rather than refused under the 16.402 arm (`pathfinding.md`) |
| Troop territory | `arena.rs::deploy_zone`, `tests/territory.rs` | a troop may not be placed inside the closed rect of any **alive enemy** crown tower. That rect is King 18x16 tiles and Princess 11x21 tiles, centred on the tower (`arena.TERRITORY_MODEL`). The pocket after a princess falls is 8 half-rows past the far bank |
| Troop taps at your own crown towers | `arena.rs::deploy_zone_king_half_open`, `state.rs::resolve_point` | the own king's no-deploy block is half-open in arena coordinates. A tap is taken at its tile's centre (`placement.TAP_SNAP`). A troop tapped on an own crown tower is moved off it to a free tile; where it lands is `placement.TOWER_TAP_PUSH`'s axis push from the raw tap, which lands all 42 measured princess-box taps and the king's 3 ties. Side 1's single ground unit tapped behind its column's back bound stands on it (`placement.TROOP_TOWER_TAPS = client16402_half_open_relocate`, measured on client 15.535.29). A troop tapped on your own building, and a Heal cast there, are moved off it the same way (`placement.TROOP_BUILDING_TAPS`, `placement.SPELL_AS_DEPLOY_TAPS`, measured on client 15.535.29 on the Cannon). An even-sized building's tap takes the arena's tile corner, not the placer's (`placement.SNAP_EVEN_CORNER`, measured on side 1's Teslas) |
| Entity storage, generational ids, spatial hash | `entity.rs` | the hash is verified against brute force |
| Card loading and level scaling | `card.rs` | per-rarity multipliers from `rarities.csv`; projectile damage is authoritative over the character row |
| Targeting | `target.rs` | edge-to-edge range, target lock once windup starts (a stun or a freeze landing on the unit clears its target and releases the lock, `status.STUN_CLEARS_TARGET`), keep-target hysteresis, lane/x default tower, building-only targeters, sight range |
| Pathfinding, two arms | `path16402.rs` (selected), `path2026.rs`, `path.rs` | the measured 16.402 search; see `pathfinding.md` |
| Movement and contact | `move16402.rs` (selected), `collide.rs` | the measured 16.402 separation, avoidance and step law |
| Melee and ranged attacks | `combat.rs`, `state.rs::phase_target` | windup, cooldown, splash, `self_as_aoe_center`. Integer damage; the rounding law is a guess (`combat.DAMAGE_ARITHMETIC`). **Replacing a dead target does not by itself reset the attack** (`combat.RETARGET_PROGRESS = keep_when_dead_or_in_reach`, measured): replacing a target that DIED keeps the attack progress when the new target already stands in reach (`combat.CORPSE_SWITCH_REACH`), so a crown tower facing one-shot victims fires on its own `HitSpeed`. Switching away from a target still ALIVE keeps the swing too when the new target already stands in reach, as dropping a doomed target for an enemy in reach does (`targeting.DOOMED_DROP_SWING`); a switch to an enemy out of reach walks on with the swing at 0. Under the earlier `reset_always` a princess tower killing skeletons reloaded in 19 ticks against its own 16, once per target change and so once per shot against a swarm. `tests/test_retarget_cadence.py`. **A unit pauses after a kill** (`combat.POST_KILL_RETARGET_WAIT = client16402_attack_finish`, measured on the 16.402 corpus): a unit whose target dies does not walk and takes no target for 6 ticks, even with another enemy in range (a tick on which a stun or a freeze holds it does not count, `combat.RETARGET_WAIT_WHILE_HELD`), and its attack starts again from 0. It skips the pause if its attack had not started, if it fires projectiles and homing shots already in flight were enough to kill the victim, or if it is a Valkyrie, Bowler, Princess, Electro Wizard, Zappy or Electro Dragon (six of the units whose data overrides the attack-finish time). So a crown tower or a shooter whose own homing shot kills keeps its cycle, and a Knight that kills with its sword does not. `tests/test_post_kill_retarget_condition.py` |
| Projectiles, shields, death damage, lifetime expiry | `combat.rs`, `state.rs` | |
| Multi-unit deploy formation | `formation.rs`, `state.rs::formation_members` | the ring, line and spiral the corpus shows, with the per-member deploy stagger and the ground-column clamp (`formation.LAYOUT` / `DEPLOY_STAGGER` / `GROUND_Y_CLAMP`, measured). The old square grid stays runnable as the `engine_grid` arm. `tests/formations.rs` |
| Deploy time | `state.rs` | units are inactive while `deploy_ms > 0` |
| Hide (Tesla) | `state.rs::hide_pass`, `entity.rs::HideState`, `target.rs`, `combat.rs::resolve` | `HidesWhenNotAttacking` / `HideTimeMs` / `UpTimeMs` from `cards.json`: under at deploy end; when a targetable enemy comes into sight it surfaces straight into its attack, taking its target and running its attack cycle on that tick while no enemy may target it, and it is up from the next tick (`hide.RISE_LAW = client16402_surface_attacking`, measured on client 15.535.29 and the 16.402 corpus; the older arm rises for `UpTimeMs` first); under again after the attack-finish wait of `combat.POST_KILL_RETARGET_WAIT` without a target (6 ticks, 300 ms, where the older arm waits `HideTimeMs`; the 6-tick hide is measured on the 16.402 corpus only); hidden = untargetable and immune (except lifetime expiry), stun and knockback pass over it. Rules the columns do not settle are the `hide.*` ledger keys: `hide.RISE_LAW` and `hide.TARGETABLE_WHILE_RISING` are measured, the rest are community-sourced. `tests/hide.rs` |
| Periodic spawners, death spawn | `state.rs::spawner_pass`, `state.rs::phase_reap`, `card.rs::SpawnerDef` / `DeathSpawnDef` | the `Spawn*` / `DeathSpawn*` columns (huts, Witch, Dark Witch; Tombstone, Golem, Lava Hound, Battle Ram): waves on the data's cadence, each unit created in the tick its timer runs out, stun pauses the timer, `SpawnLimit` counts the spawner's own live units, death spawns on a ring of `DeathSpawnRadius` on the dying unit's facing (`spawner.DEATH_SPAWN_LAYOUT = facing_ring_rounded`, measured), except a `DeathSpawnPushback` row's (below), and except the Tombstone's and the Barbarian Hut's, which appear together on the point the spawner emits at (`spawner.DEATH_SPAWN_AT_EMISSION_POINT`). A unit a spawner emits, and a death spawn's unit, takes its first step on the tick it appears, or enters its attack if an enemy is in range (`spawner.SPAWNED_FIRST_STEP`). A unit dying on that same tick still pushes and steers it on that step, as in the game (`spawner.FIRST_STEP_DYING_BODIES = client16402_seen`, measured on client 16.402). The children of a row with `DeathSpawnPushback` (the Golem's, the Lava Hound's) appear 250 from the death point on a fixed ring and slide straight out to `DeathSpawnRadius` (`spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide`, measured), and stand still on their first tick, as in the game. Each child steps toward its own end point on that ring (`spawner.DEATH_SLIDE_AIM`, measured on the Lava Pups), but only for as many moves as the straight slide takes, and then stops wherever it stands (`spawner.DEATH_SLIDE_STOP`). So a push that knocks it off its line is steered back toward the end point and can leave it short. No enemy targets a death spawn's troop before its 8th frame, nor the turret the Hero Musketeer's ability puts down (`targeting.SPAWNED_UNIT_ACQUIRE_DELAY`, measured on client 15.535.29); a hand-played troop, a played building and a Tombstone's periodic Skeletons are targeted at once. Thirty-two `spawner.*` keys are measured: DEATH_SPAWN_PUSHBACK, DEATH_SLIDE_AIM, DEATH_SLIDE_BIRTH, DEATH_SLIDE_STOP, SCHEDULED_UNIT_FIRST_UPDATE, ABILITY_UNIT_FIRST_UPDATE, FIRST_WAVE, START_TIME_ORIGIN, TIMER_LEFTOVER, SPAWN_POINT, RELATIVE_SPAWN_OFFSET, DEATH_SPAWN_RADIUS_DEFAULT, DEATH_SPAWN_LAYOUT, DEATH_SPAWN_RING, DEATH_SPAWN_AT_EMISSION_POINT, DEATH_SPAWN_DEPLOY_TIME_DEFAULT, DEATH_BOMB_SPAWN_TIMING, EMISSION_TIMING, RELEASE_TIMING, SPAWNED_DEPLOY_TIME, SPAWNED_FIRST_STEP, FIRST_STEP_DYING_BODIES, EMISSION_WATER_TURN, SPAWN_AREA_OBJECT_SCOPE, SPAWN_AREA_OBJECT_TIMING, SPAWN_TO_LOCATION_OFFSET, LIFE_STATE_WAKE_REACH, LIFE_STATE_WAKE_TARGETS, LIFE_STATE_FIRST_UPDATE, LIFE_STATE_WAVE_POINT, ACTION_SPAWNER_SPAWN_SPEED, SPAWN_SPAWNER_SPAWN_SPEED; PAUSE_ANCHOR, DEATH_SPAWN_PROJECTILE, INTERVAL_START_ORIGIN, LIFE_STATE_FIRST_LOOK_AIM and LIFE_STATE_AIM_REPICK are hypotheses, STUN_PAUSES_SPAWNER is datamined, LIMIT_RULE is a guess. A ring wave (the Witch's Skeletons, the Night Witch's Bats) is created last angle first, which sets the order its members update in (`spawner.RING_CREATION_ORDER`, measured on 37 of 37 waves). `tests/spawner.rs`, `tests/death_spawn_pushback.rs`, `tests/spawned_first_step.rs`, `tests/first_step_dying_bodies.rs`, `tests/spawn_acquire_delay.rs` |
| Death area effect (Ice Golem) | `state.rs::phase_reap`, `spell.rs::cast`, `card.rs::convert_area_effect` | `DeathAreaEffect` names a row of `cards.json`'s `area_effect_objects`. The death leaves that area standing where the unit stood, and it runs the engine's own area-effect path. That is the same object, the same disc test and the same buff a Zap gets. The Ice Golem's is a 2-tile disc that hangs a 30 % slow on enemies for 2 s and carries no damage of its own. It is a **second** effect of the same death, beside the `DeathDamage` disc: the two carry their own radius, damage and crown percent, and the Super Ice Golem ships them with different values for each. An area whose action is its mechanic loads when the loader reads that action: the Lumberjack's puts down a bottle, which loads as that bottle's fuse over its rage, and the Suspicious Bush's puts two goblins down on a schedule. Any other area the loader cannot read is refused card and all, with the reason. `tests/death_area_effect.rs`, `tests/lumberjack.rs`, `tests/suspicious_bush.rs` |
| Death bomb (Balloon, Giant Skeleton, Bomb Tower) | `card.rs::convert_death_bomb`, `state.rs::phase_reap`, `spell.rs::step_spells` | Their `DeathSpawnCharacter` names a row with no hitpoints, no damage, no hit speed and no LifeTime. It carries only `DeployTime` 3000, `DeathDamage` and `DeathDamageRadius`. That is not a unit, so it is not spawned as one: the death leaves **one area hit on a timer** at the point of death, carried by the same spell object an Arrows wave waits in. It is untargetable and blocks nothing, because it is not on the board at all. The impact is the engine's ordinary `DeathDamage` disc with a fuse: enemies only, air and ground per the row, crown towers at the row's percent. The fuse is `DeployTime` and that is measured, not read off the column name: in capture `20260920-083112` (both seats) a level-11 Balloon's bomb takes 240 off a King Tower 1987 native units away **61 ticks** after its last live frame, and 240 is `BalloonBomb`'s `DeathDamage` 94 on the Common ladder at level 11. `DeathPushBack` (Giant Skeleton, 1800) is carried into `cards.json` but not applied to a plain bomb, the same gap every other `DeathDamage` row has. The Skeleton Barrel's container is a death bomb that also carries a death spawn. On the tick its 600 ms fuse runs out it hits for 145 at level 11, releases its seven Skeletons and pushes what it hit with its `DeathPushBack` 1000 on the knockback ladder (`spawner.DEATH_BOMB_SPAWN_TIMING`, `knockback.DEATH_PUSHBACK`, both measured on client 15.535.29). The barrel itself drains from its first attack and dies ten ticks later (`combat.KAMIKAZE_TIME`). `tests/death_bomb.rs`, `tests/skeleton_barrel.rs` |
| Charge (Prince, Dark Prince, Battle Ram) | `state.rs::charge_pass`, `effective_speed`, `combat.rs::fire` | `ChargeRange` / `DamageSpecial` / `ChargeSpeedMultiplier`: the run-up accumulates `tdiv(L x 1000, ChargeRange)` permille per walking tick from the requested step `L = min(S, dist, 250)`, charged at 10000; the speed doubles from the next walking tick (measured on the corpus: the Prince's 43rd walking tick) and the next landed hit deals `DamageSpecial`; consumed by the hit, reset by a stun or a landed knockback. The `charge.*` keys hold what the corpus has not yet separated. `Kamikaze` is read (`combat.KAMIKAZE_DEATH = at_fire`): a Battle Ram dies on the tick its one hit lands and breaks into its two Barbarians; a delayed kamikaze (`KamikazeTime`, the Skeleton Barrel) drains from its first fire instead (`combat.KAMIKAZE_TIME`, `tests/skeleton_barrel.rs`). `tests/charge.rs` |
| Dash (Bandit, Mega Knight) | `state.rs::phase_path16402`, `card.rs::DashDef` | the `Dash*` columns and `JumpSpeed` (`combat.DASH_ATTACK`, measured on client 15.535.29). A unit walking after its target stands for `DashCooldown`, then dashes. The Bandit stops on the first half-step within her range and hits for `DashDamage`, and takes no damage while she dashes. The Mega Knight jumps to his target, lands `DashDamage` around him and pushes the troops it hits down the knockback ladder (`combat.DASH_PUSHBACK`). A dasher that first sees its target inside its trigger distance triggers on that tick (`combat.DASH_FIRST_SIGHT_TRIGGER`) |
| The Golden Knight's button | `state.rs::chain_pass`, `card.rs::AbilityEffect::DashChain` | a press dashes him at the closest enemy ground unit within 5.5 tiles, hits it for his `DashDamage`, and after each blow dashes on at the closest one he has not hit yet within 5.5 tiles of where he stands, up to `DashCount` (measured on client 15.535.29: the dash moves 400 a tick in steps of at most 250 and stops on the first step in range). After the chain he holds no target for two ticks, then takes the nearest enemy (`combat.DASH_CHAIN_END`). A press with no enemy ground unit within 5.5 tiles is taken and paid: he keeps his ordinary target, runs toward it at twice his walking speed, and dashes once it is within 5.5 tiles plus its radius (`combat.DASH_CHAIN_PENDING`; both measured on client 15.535.29). His first ordinary hit after a chain lands one hit-speed after the tick that follows his last blow (`combat.DASH_CHAIN_ATTACK_CYCLE`, measured on 12 of 12 chains). The button's charge comes back `combat.DASH_CHAIN_COOLDOWN` after the chain ends: 11,000 ms, a hypothesis, not measured. The Monk's button runs too (his row below), and the Archer Queen's: after her cast her cape (`card.rs::AbilityEffect::SelfBuff`) hides her for 3.5 seconds, no enemy may target her (`status.rs::BuffDef::invisible`, `target.rs::invisible_at`; area damage still lands), and she attacks at 2.8 times her rate, one charge (measured on client 15.535.29). The Skeleton King's and the Mighty Miner's buttons are not run, and those two play as plain troops |
| The Monk's combo | `combat.rs::stage_damage`, `card.rs::ComboDef` | his hits run 55, 55, 165 in turn (140, 140, 422 at level 11), and the count carries across targets; his third hit pushes its target 1,800 straight away from him down the knockback ladder, from the hit's own tick (`combat.ATTACK_COMBO`, `knockback.COMBO_PUSHBACK`, measured on client 15.535.29). A walking Monk or Mighty Miner stops at Range plus the target's radius, as the Inferno Dragon does (`targeting.VARIABLE_DAMAGE_WALK_REACH`). His Deflect button (`card.rs::AbilityEffect::Deflect`): after its cast he stands for 4 seconds, every hit on him lands at 35 %, and every enemy shot that lands on him also flies back at its shooter for its full damage; one charge (measured on client 15.535.29). A cast holds its unit for its CastTime in whole ticks, 933 as 900 (`state.rs::whole_ticks_ms`) |
| Reflect (Electro Giant) | `state.rs::reflect_melee_hit`, `card.rs::ReflectDef` | a melee hit on him from close by is answered with `ReflectedAttackDamage` and a stun on the attacker (`combat.REFLECT_ATTACK`, measured on client 15.535.29) |
| Several projectiles or bolts per attack | `combat.rs::fire`, `combat.rs::fan_aim` | the Hunter fires its fan of pellets (`combat.MULTIPLE_PROJECTILES = client_fan`), the Princess fires her damaging first arrow (`combat.CUSTOM_FIRST_PROJECTILE = client_first_of_volley`), and the Electro Wizard's bolts hit one target each (`combat.MULTIPLE_TARGETS = client_bolts_per_target`) |
| Inferno damage that grows | `combat.rs`, `card.rs` (`variable_damage`) | the Inferno Tower and the Inferno Dragon raise their damage twice while they hold a target, by their `VariableDamage2` / `VariableDamage3` and their times (`combat.VARIABLE_DAMAGE = client16402_attack_progress_stages`, measured on the 16.402 corpus) |
| The Mortar's minimum range | `target.rs` | the Mortar drops a target whose edge distance falls below its `MinimumRange` (3.5 tiles) and never takes it again while it stays that close (`targeting.MINIMUM_RANGE = client16402_edge_distance`, measured on the 16.402 corpus) |
| Leaps and chases | `target.rs::can_target`, `target.rs::chase_drop_applies` | a unit in its river leap (Hog Rider, Prince, Dark Prince, Royal Hogs, Battle Ram) can be targeted only by an attacker that hits air (`targeting.LEAPING_UNIT_TARGETABILITY = airborne`). A unit landing from its leap pushes and is pushed from the tick after it lands, not on the landing tick itself (`movement.JUMP_LANDING_CONTACT`, read on two Royal Hogs landings). A walking troop drops a troop it chases once the gap passes its sight range plus both radii, less 1000 (`targeting.CHASE_DROP_RANGE = client_sight_minus_1000`). The leap rule is measured on client 15.535.29 only for the Hog Rider against melee ground-only attackers; for the other leaping cards and for ranged attackers and buildings it is inferred. The chase drop is measured on client 15.535.29 along a lane |
| Launch recoil (Sparky, Firecracker) | `state.rs::attack_recoil` | a launch pushes the unit away from its target down the knockback ladder, by its `AttackPushBack` (`knockback.ATTACK_PUSHBACK = ladder_away_from_target`, measured on client 15.535.29) |
| Range projectiles (Bowler, Elite Archer, Executioner, Hunter) | `combat.rs::fire`, `combat.rs::step_straight`, `combat.rs::throwers_out` | a shot whose row has a `ProjectileRange` flies straight along the launch line to that range and hits each enemy it passes once. The Bowler's boulder pushes on its `Pushback`. The Executioner's axe flies out and back, and he stands and takes no new target while it is out, so his throws are 49 ticks apart (`combat.RANGE_PROJECTILE = straight_to_range`, measured on client 15.535.29) |
| The Hunter's pellets | `combat.rs::fire`, `combat.rs::step_straight` | a pellet is gone on the tick it hits (`CheckCollisions`) and waits its `RandomDelay` before its first step. A pellet's first-tick hit test reaches its `ProjectileStartExtraRadius` further (`combat.PROJECTILE_COLLISIONS = client_columns`, measured on client 15.535.29) |
| Deploy effects (Electro Wizard, Ice Wizard, Mega Knight, Battle Healer) | `state.rs::phase_spawn`, `state.rs::deploy_blow`, `state.rs::spawn_now`, `spell.rs::cast` | the Electro Wizard's and the Ice Wizard's area acts where the unit appears, on its first tick (`spells.DEPLOY_AREA_EFFECT = client_area_effect`). The Mega Knight's deploy blow lands 6 ticks after he appears (`combat.DEPLOY_PROJECTILE`). Both belong to playing the card: a unit the scenario setup puts down casts neither. The Battle Healer's spawn heal goes down where she appears, set up or played (`spawner.SPAWN_AREA_OBJECT_SCOPE = every_row`). All measured on client 15.535.29. The blow of the turret the Hero Musketeer's ability puts down lands 2 ticks after the turret appears (`combat.DEPLOY_PROJECTILE = client_on_landing_action_at_2`), measured on client 15.535.29 too, but on only a few hits in its hero scenes. The two Wizards take one tick longer to deploy than a troop played by hand, as in the game (`spells.DEPLOY_AREA_EFFECT_DEPLOY_TIME = client_one_tick_longer`, measured on client 16.402 and 15.535.29) |
| Stun | `entity.rs`, `state.rs`, `status.*` keys | honoured by move and attack; applied by Zap. Attack reset, retarget-on-resume and the deploy-pause question are each their own ledger key |
| Status effects | `status.rs`, `state.rs::buff_pulse_pass` | a per-entity buff list: `SpeedMultiplier` and `HitSpeedMultiplier` compose into the move and attack arithmetic (`status.BUFF_STACKING`, `status.SAME_BUFF_REAPPLY`, `movement.BUFF_SPEED_COMPOSITION`), a full-stop buff drives the hold (`status.FULL_STOP_BUFF_IS_STUN`), and damage over time and heal pulse on their own clock (`status.BUFF_PULSE_AMOUNT` / `BUFF_PULSE_TIMING`; a pulse is its share of the level-scaled per-second figure, `scaled_per_second_times_frequency`, measured on client 15.535.29), so Poison and Earthquake load as pulsing areas. `tests/status.rs` |
| Damage reduction and the Super Knight's shield | `combat.rs::reduce_hit`, `state.rs::idle_buff_pass` | a unit carrying a buff with a `DamageReduction` takes each hit scaled by (100 - reduction) / 100, rounded down, and never less than 1 (`status.DAMAGE_REDUCTION`). The Super Knight's shield is on while it is not attacking: its own hit turns it off from the next tick, and it comes back after the Super Knight stops attacking (`status.IDLE_BUFF`). While the shield is on, the Super Knight puts down an area every 50 ms that gives its own side within 8 tiles a reduction of 100, so a hit takes 1. Measured on client 15.535.29 at 100: 1 against the Knight's 202, and the full 202 while it attacks. The 65 of the Monk's Deflect is measured on client 15.535.29, truncating (a Musketeer's 217 lands at 75), and runs (his row above). The 60 of the Evo Knight and the 15 of the hero Valkyrie are read on 16.402, where the hits truncate; neither runs in the engine |
| Command delay | `state.rs::run_due_commands`, `BattleConfig::command_delay_ticks` | per side, 0 by default (the engine as it ran before). A play or a button press accepted on tick T waits (`PendingCommand`) and runs at the top of tick T + k through the ordinary deploy or press, checked again in full then: the hand and the elixir change only when it runs, a waiting card or button is refused a second time (`DeployError::CardPending`, reason 18), a waiting command's cost counts against the side's elixir when the next is accepted, and a building whose tile is taken when it runs goes where a play then would put it. Measured on the live client (device clock): a tap runs 1072..1099 ms later, 21-22 ticks, for every player; several plays wait at once; a tapped card's slot empties at the touch; a second play the elixir covers only without the first's cost is never sent; a Goblin Hut tapped on a waiting Tombstone's tile went down 3 tiles toward the river, paid. Unmeasured: two buildings run on the SAME tick on one tile both go down there (neither has spawned when the other is placed) |
| Elixir, hand, cycle | `state.rs` | double elixir when `MANA_SPEED_UP_WHEN_REMAINING_SECONDS` remain and into overtime, triple elixir from `MANA_TRIPLE_AFTER_OVERTIME_S` (60 s) into overtime (`triple_elixir`; measured on client 15.535.29: the first triple step is the one into t4801) |
| Match timing | `state.rs::phase_judge` | regular time, 120 s sudden-death overtime (`match.OVERTIME_S`, measured by a third party), 3-crown instant win |
| Post-overtime tiebreak | `state.rs::overtime_tiebreak` | when overtime runs out level on crowns, the side whose weakest standing crown tower is weaker loses (`match.OVERTIME_TIEBREAK`, community-sourced, `lowest_tower_hp_absolute`); an exact tie stays a Draw. `tests/tiebreak.rs` |
| King activation | `state.rs` | on king damage or a lost princess tower, after `match.KING_ACTIVATE_TIME_MS` |
| Spells | `spell.rs`, `card.rs`, `state.rs` | `SpellShape` Projectile / AreaEffect / Rolling / PulsingAreaEffect (Poison, Earthquake) / Fuse (Rage's bottle) / Strikes (Lightning) / Summon (the Heal card). Fireball, Arrows, Zap, The Log and Goblin Barrel are specced card by card; Rocket, Freeze, Poison, Earthquake, Snowball, Tornado, Lightning, Rage, Heal, Barbarian Barrel and WarmSpell load and are not. Fed the 15.535 card data. Territory rules per card. A spell reaches a crown tower, and an ordinary building, by a square round its centre rather than a disc (`spells.CROWN_TOWER_SPELL_REACH`, `spells.BUILDING_SPELL_REACH`, measured). Spec and sourcing: `spell-spec.md` |
| Knockback | `move16402.rs::start_pushback` / `pushback_step`, armed in `state.rs::arm_ladder` | the measured ladder (`knockback.DISPLACEMENT_LAW = client16402`): a speed of 25n native units per tick toward a point `min(Pushback, MAX_PUSHBACK_LENGTH)` away, falling by 25 each tick, with one 25-unit back-step at the end and the path dropped when it stops. `knockback.STACKING = first_wins_while_active`; `IgnorePushback` / `PushbackAll` are honoured; a landed push resets the attack and keeps the target (`knockback.ATTACK_RESET`, measured) and leaves the victim's load timer running (`knockback.PUSH_LOAD_TIMER`, measured on 23 push landings). The Mega Knight's jump blow pushes the troops it hits down the same ladder (`combat.DASH_PUSHBACK`). The earlier fixed-distance slide stays runnable under the same key. See "Knockback" below |
| Crown tower damage reduction | `combat.rs` | `combat.CROWN_TOWER_DAMAGE_ROUNDING = ceil_kept_share`, measured on the 16.402 corpus and client 15.535.29 |
| Determinism, save/load | `lib.rs`, `state.rs` | seeded PCG32, `state_hash` every tick, snapshot fingerprints |
| Seat symmetry | `tests/mirror.rs` and friends | 180-degree rotation, checked every tick; see `architecture.md` |
| The Python surface | `py.rs` | the env layer runs a full 18-card battle on it with no changes of its own |
| A held unit's contact | `state.rs::phase_path16402_for` | a unit held by a freeze or a stun keeps its contact update at speed 0: a walker pushes it, and with nothing overlapping it stands (`collision.HELD_UNIT_CONTACT`) |
| A swing across a kill or a doomed target | `target.rs` | after a kill, a swing under way carries over only onto a new target already in reach (`combat.CORPSE_SWITCH_REACH`); dropping a doomed target for an enemy in reach keeps the swing (`targeting.DOOMED_DROP_SWING`). A walker whose card fires a projectile treats a doomed princess tower as already fallen and walks to the king; a walker without one keeps walking to the princess (`targeting.DOOMED_LANE_TOWER`) |
| A knocked target past the chase-drop limit | `target.rs` | a troop keeps a target that a knockback slides past its limit (`targeting.CHASE_DROP_KNOCKED_TARGET`, 3 of 3 slides, all the holder's own Bowler boulder) |
| The tower after a lane's princess falls | `target.rs` | a troop whose spawn lane's princess tower is down walks to the king (`targeting.FALLEN_LANE_TOWER_PICK`) |
| A dasher that first sees its target inside its trigger | `combat.rs` | it triggers on that tick, not the next (`combat.DASH_FIRST_SIGHT_TRIGGER`, 12 first-sight dashes on client 15.535.29) |

## Not modelled

Each row says what a caller sees instead. That is what matters when you are deciding whether the
engine is usable for your purpose.

| Mechanic | What happens instead |
|---|---|
| `DeathSpawnMinRadius` | Not read: nothing holds a death spawn's units off a minimum radius (the Skeleton Barrel's container is the only row that carries it). `DeathSpawnPushback` is read since the round-6 flip (`spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide`) |
| The Mega Knight's `DashLandingTime` | not read (his jump push, `DashPushBack`, is read: see Dash above) |
| Morph (all but the Goblin Drill's) | absent. The one morph modelled is the Goblin Drill's: its dig turns into its building where it comes up (`state.rs::surface`). The river hop IS modelled (`jump16402.rs`, `movement.JUMP_WATER_HOP`) |
| The Mega Monk's combo, and the Inferno ramp on the 2018 table | the Mega Monk's sequence does not read cleanly from its table and is not loaded (the Monk's runs: see its row above). The 2018 table carries no ramp columns at all, so on it the Inferno Tower and the Inferno Dragon keep their first-stage damage |
| Rage and Heal, the parts not measured | both load. Rage's bottle stands for its DeployTime and then leaves an area that speeds up your own side and damages enemies once. The Heal card puts down a Heal Spirit like a one-unit troop, and its shot leaves a heal on your own troops. Three things are not measured. Whether a Rage also speeds up your own buildings and crown towers is the `spells.OWN_SIDE_AREA_SCOPE` key's guess. A Rage speeds up a periodic spawner's waves here too (a Tombstone's, for one), because `spawner.SPAWN_SPAWNER_SPAWN_SPEED` ships `buffed`; whether a Rage does that in the game is not measured. Two Heal Spirits on one unit heal as one, not two. A Rage does speed up the Goblin Hut's waves, and that is measured on client 15.535.29 (`spawner.ACTION_SPAWNER_SPAWN_SPEED`). A slow reaches a periodic spawner too: in one 16.402 battle a Tombstone waits 15 ticks longer after an Ice Golem's death slow, and `spawner.SPAWN_SPAWNER_SPAWN_SPEED` ships `buffed`, which runs a spawner's clock at its composed SpawnSpeed |
| Evolutions, hero forms and champions beyond the ones that run; tower troops | eighteen evolutions run (the Evo Skeletons, the Evo Cannon, the Evo Musketeer, the Evo Elite Barbarians, the Evo Zap, the Evo Battle Ram, the Evo Inferno Dragon, the Evo Baby Dragon, the Evo Royal Ghost, the Evo Skeleton Army, the Evo Giant Snowball, the Evo Skeleton Barrel, the Evo Mortar, the Evo Royal Hogs and the Evo Minion Horde, whose ghost after a first hit lasts the 3000 ms its table gives, not measured, the Evo Tesla, the Evo Royal Recruits, whose charge after the shield's loss is read off its table, not measured, and the Evo Wizard, whose blast at the shield's loss is read off its table, not measured) and ten hero forms (the Hero Musketeer, the Hero Ice Golem, the Hero Berserker, the Hero Balloon, the Hero Valkyrie, the Hero Wizard, the Hero Mini P.E.K.K.A., whose quest bar and level gains past the first are read off its table, not measured, the Hero Knight, whose taunt is read off its table, not measured, and the Hero Mega Minion, whose warp's strike shot and crown share are read off its table, not measured, and the Hero Giant, whose throw's landing blow and stun are read off its table, not measured), each marked per deck with `reset(..., forms=)`; no other evolution or hero form loads. A Clone of the Evo Royal Ghost makes a plain Royal Ghost here; in the game that copy stays invisible for good. Of the champions' buttons the Golden Knight's, the Monk's and the Archer Queen's run (above). The Skeleton King and the Mighty Miner load and play as plain troops with no button, and the Little Prince, Goblinstein and the Boss Bandit are refused (`CardDb::rejected` gives why). No tower troop runs: the crown towers are the princess and king towers |
| A troop's own projectile knockback (`Pushback` on the projectile row) | read for a range projectile only: the Bowler's boulder pushes (`combat.RANGE_PROJECTILE = straight_to_range`). Any other troop projectile is loaded as speed, damage, splash radius and a buff. So Zappies push in the game and do not here. The Mega Knight's two blows do push, each read on a path of its own: the deploy projectile he lands as he appears (`combat.DEPLOY_PROJECTILE`) and his jump's blow (`combat.DASH_PUSHBACK`). A SPELL's knockback is read (`spell.rs`, the measured ladder) |
| The splash layer filter (`AoeToAir` / `AoeToGround` on the projectile row) | read for a range projectile only (the Bowler, the Hunter, the Elite Archer, the Executioner, under `combat.RANGE_PROJECTILE = straight_to_range`). For any other projectile the engine filters a splash by the ATTACKER's `AttacksAir` / `AttacksGround` (`combat.rs`). The two agree on every card the slice reaches; they disagree on Wall Breakers, whose blast covers air in the data and only ground here (`tools/check_card_reads.py` names every row where they part) |
| `ReflectAttackCrownTowerDamage` | not read: a shot on the Electro Giant is not answered, a crown tower's included |
| The real intra-tick order and the real PRNG | out of reach, and not a goal |

### Cards outside the slice

`data/derived/cards.json` holds **144 cards** (102 troops, 15 buildings, 27 spells) and 334
units. The engine loads every simulable non-tower card of it;
`Battle(card_names=None).catalogue_json()` lists what loaded and `CardDb::rejected` what did
not, with the reason. That default includes the Mirror (code 6) and the cards that travel under
ground (the Miner and the Goblin Drill, code 5), which RoyaleGym places. Some of the loaded
cards carry a mechanic `card.rs` never parses, so an 8-card deck drawn uniformly from the whole
catalogue is much more likely than not to hold one.
If you are picking decks programmatically, draw from `cards.json`'s own `thin_slice` key rather
than from the catalogue. Parse that key, never hand-copy it.

`tools/check_data.py` asserts the *data* is present (it even asserts Prince's
`charge.damage_special > 0`) without asking whether the engine reads it. The gate that asks the
other question is `tools/check_card_reads.py` (`docs/contributing.md`). It builds the set of
`cards.json` fields `card.rs` consumes and compares it both ways against what the data carries. It
fails when a thin-slice card carries an unread mechanic, and it reports per card over the rest of
the catalogue. On the 15.535 table, 2026-09-22
(`python tools/check_card_reads.py`), **82 of the 95 loaded cards carry at least one column or key
the loader never reads**, 69 of them outside the thin slice; a further 3 are flagged only by the
coarser per-object pass. Inside the slice 13 of the 18 cards carry one, every one on the tool's
named list of open gaps; the other five (Knight, Giant, Prince, Fireball, Zap) carry none, though
the coarse pass still names Fireball's projectile deflect columns. Loading a card is
still not the same as running it, and the gate does not make it so; what it does is stop the
difference from being invisible.

## Known defects

One defect in the collide layer is open, and one entry that used to be here has been retired
because the recordings show the game doing the same thing.

### A unit standing inside a building's tile box is not a defect

RETIRED 2026-09-22. This section used to report a defect: a unit could stand with its centre
inside a building's footprint for up to 47 ticks. The recordings say the real game does the same
thing, so the engine was being measured against a rule the game does not have.

What the recordings show, over 42 recorded battles. Troop centres are inside a building's TILE BOX
constantly: inside a Cannon's on 394 of 943 nearby frames, inside a princess tower's on 11923 of
49173, inside a king tower's on 6501 of 12953. The tile box is where a building may be PUT
(`data/calibration.json` `placement.*`), and it never blocks a unit.

Inside the CollisionRadius CIRCLE is a different matter, and there the game is nearly strict: four
unit-building pairs in the whole corpus, none lasting more than three frames. Three of those four
are a building appearing on top of a unit that was already standing there, and the unit leaves at
about 150 per tick, attackers included. The fourth is a Miner surfacing. Closest approaches, minimum
and 5th percentile: Cannon 814 / 1141, Tesla 1139 / 1213, Tombstone 947 / 1551, princess tower
1170 / 1580, king tower 1245 / 1904.

The engine agrees at legal placements. Under the shipped arms, a Skeleton Army, Goblins or a Goblin
Barrel dropped around a riverside Cannon put a unit inside the circle only while it is still
deploying, and for at most three ticks; the old 47-tick repro no longer reproduces at all, because
the shipped movement arm does not run the push-out that produced it. The longer runs that remain
belong to Cannon sites whose circle reaches past the river edge, and the placement rule now refuses
those sites.

**What is left was a real defect of the earlier `frozen` arm of
`movement.ATTACKING_UNIT_MOVEMENT`, and it is the opposite one.** That arm skips the whole move
pass for an ATTACKING unit, so separation never reaches it. Under it, place a Cannon on a Knight
that is attacking something: with the Knight 814 from the Cannon's centre it stays at 814 for more
than 58 ticks, and starting inside the circle at 316 it stays there for more than 80. The game
pushes an attacking unit out. One recorded Skeleton goes 814, 955, 1104 over three ticks while
attacking. So that arm was not too permissive here; it was too rigid, and only for units that are
attacking. The shipped arm, `separation_only` (measured on the 16.402 corpus), lets neighbours
push an attacking unit apart. The Cannon-on-a-Knight case has not been re-measured under it.

### Push-outs are summed, not selected

Both `spell::settle` and the static pass of `collide::separate` **add** the push-out vector of
every obstacle the unit's disc penetrates. Aligned pushes double-count; opposed pushes cancel, the
4-round loop oscillates, and the whole knockback is dropped.

The severity is smaller than "a Fireball throws a Knight across the river", and it is different in
kind. The big throws reproduce identically under sum, deepest-single, clamp and sequential, so they
come from the eject-fully-outside-then-iterate model chaining tower to Cannon, not from the
summing. The summing's own measured signature, over 1,678,112 sampled pushes:

- **7,040** resolve somewhere other than the deepest-single result,
- up to **0.222 tile/tick** of static over-push (Giant between two Cannons at minimum legal
  separation),
- **74** pushes silently cancelled, because `settle` exhausts its rounds and returns the old
  position.

It happens without any scenario API: two Cannons at the minimum separation `check_deploy` allows
leave **129** legal stand points where a Giant's disc penetrates both, and a Cannon legally placed
beside your own princess tower leaves **127**. Crown towers alone never stack: 0 of 58,101 grid
points penetrate two of them.

The sharpest case is a wedge. A Knight at (7.6001, 10) between Cannons at (7.0, 10) and
(8.2001, 10) has per-obstacle penetration depths of 0.49994 and 0.50000 tiles; the summed push is
**0.0001 tile**, the deepest single push is 0.5 tile. Two hundred consecutive static passes
oscillate between two adjacent subtiles and the unit stays half inside both buildings forever. No
invariant sees it, because rule 2 checks the centre and the centre is outside both.

A contributing defect sits in the same loop: the push predicate and the exit predicate disagree.
The loop pushes for **disc** clearance and exits on **centre** clearance, so it can return a point
whose disc still overlaps two buildings, and it cannot terminate at all for a unit wedged in a gap
narrower than its own diameter.

A fix has a frame hazard worth naming in advance: both call sites are handed the Blue-frame
obstacle list even for a Red mover, and `Obstacle.ally` in that list means "owned by Blue". So
`ally` is unusable as a tie-break for a Red mover, and any ordering on engine x/y is
seat-asymmetric, which the 180-degree seat symmetry forbids. Selection has to be by a total order
evaluated in the mover's own frame.

### A unit keeps its target and route through a knockback, and nothing has shown the game does not

The engine holds a unit's target and its planned route for the whole of a knockback ladder. A troop
whose target the slide carries past its chase-drop limit keeps it too (`targeting.CHASE_DROP_KNOCKED_TARGET`,
measured on 3 slides, all the holder's own Bowler boulder).

**This section used to say the corpus refuted that, and it was wrong.** The evidence was the
Golem of capture 20260920-071744-B going target 6, then no target at t2043, then target 4 at
t2044, with its path node count going 3 to 14 -- six steps before the back-step of a clean
ten-step ladder. Read again by frame index rather than by tick, the same transition happens on
the same ticks to two things that cannot be knocked back at all:

    key 71  Golem          side 0   6  6  6 -1  4  4  4     path_n  3  3  3  3 14 14 14
    key  3  PrincessTower  side 0   6  6  6 -1  4  4  4     path_n  0  0  0  0  0  0  0
    key  1  KingTower      side 0   6  6  6 -1 -1 -1 -1     path_n  0  0  0  0  0  0  0
    key  6  PrincessTower  side 1  72 72 -1  .  .  .  .     <- its last frame

Key 6 is the tower the Golem was walking at, and that is the tick it died on. Everything naming
it fell back at once, the two crown towers included. The Golem's node count moved because its
destination moved.

The cause of the misreading is worth more than the correction. The recording's `target` is the
client's own pointer, and the client aims it at the **destination crown tower** whenever a unit
is not fighting something. The engine's `target` is only ever an attack target: the default
tower is computed as a local walk goal at four sites and never stored. So a field that changes
when a tower dies was read as a field that changes when a unit retargets -- the same confusion
that makes the replay harness's `target_match` column score about 87 per cent automatic misses.

The hold is now **unrefuted rather than refuted**, which is not the same as confirmed. It rests
on the one Giant it always rested on. A capture of a unit pushed while it walks at a tower that
stays alive would settle it; `knockback.DISPLACEMENT_LAW`'s open item 4 carries the detail.

### Smaller open items

- The 8th-frame delay on death spawns (`targeting.SPAWNED_UNIT_ACQUIRE_DELAY`) has unmeasured edges. The engine exempts every periodic spawner's troops, the Barbarian Hut's and the Witch's among them, and any building a death spawn creates. The one building that waits is the Hero Musketeer's turret, which an ability makes, not a play. In the game the Goblin Hut's waves do wait, so the Barbarian Hut is an open case. On the 15.535 table the Goblin Hut, which loads, waits as in the game: its life-state controller's waves carry the delay. (The 2018 table's Goblin Hut is an ordinary periodic spawner and is exempt.) A hidden Tesla does not rise for a unit inside its delay, and area damage still hits the unit.
- `spawn_unit` / `deploy` called several times for one team within a tick assigns `team_seq` in
  call order. This is reachable only from the Rust test API; the Python surface applies at most
  one per team per step.
- 0.27% of live unit-ticks are still not reproduced, counted over the offline trace corpus of
  client 15.535.29 (`pathfinding.md` holds the run and the counts).

## Invariants the game does not have

Three invariants the engine used to enforce were relaxed under the 16.402 arm, each because a
measurement refutes it. Every figure below comes from the 16.402 capture corpus,
troop pairs with deploying units excluded. They are listed here so nobody re-adds them as "obviously correct":

- Troops may stand **inside a building footprint**. Melee attackers do, because their goal cell
  is within reach of the building's centre.
- Troops may stand **on a water cell**. Live troops do, for up to 1070 consecutive ticks, and
  none is ejected anywhere in the corpus.
- Crowds **overlap** more than the old tolerance allowed: live, more than 100% of the smaller
  radius for up to 42 consecutive ticks, and more than 50% for 207.

`tests/common/mod.rs CLIENT16402_TOLERANCE` and the `dry` gate in `tools/watch_battle.py` carry
the relaxed forms.

## Knockback

The engine runs the measured ladder (`knockback.DISPLACEMENT_LAW = client16402`): a speed of
25n native units per tick toward a target point `L = min(Pushback, MAX_PUSHBACK_LENGTH)` away
from the source, falling by 25 each tick, with one 25-unit back-step at the end. The evidence is
the Giant of capture 20260918-122757.b1, ticks 1216..1223: steps of 150, 125, 100, 75, 50, 25, 0
and then -25. `knockback.ATTACK_RESET` is measured with it. The earlier fixed-distance slide
stays runnable under the same key, and the seat-symmetry gates use it.

The Log hits on its landing tick, standing on the tap, and rolls from the next tick
(`spells.ROLL_FIRST_STEP`); every hit pushes a troop away from the Log's centre as it stands at the
end of that tick (`knockback.DIRECTION_ROLLING = client15535_radial_from_tick_end_centre`; both
measured on client 15.535.29, and they run as one law). So a troop standing behind the tap is pushed
back, toward the caster, and one just ahead of it is pushed forward. A Log whose front edge only
touches a unit misses it rolling up the arena and hits it rolling down (`spells.ROLLING_HIT_SHAPE`,
measured; the seat-symmetry gates select the closed edge). Measured on client 15.535.29, in the log-behind scenarios: Knights 500 and
1000 native units (half a tile and a tile) behind the tap were pushed back by the full ladder, on
both sides, and a Knight 1500 (a tile and a half) behind was not touched. This corrects an
earlier observation that the Log never pushes backward. Off the roll axis, the angle of the push
depends on the tap being snapped to its tile centre, which the shipped build does
(`placement.TAP_SNAP = client16402_tile_centre`).

## Open questions

These need a recording or a decision, not more code:

1. **Exact twins deadlock on a bridge.** Two identical units meeting head-on at the same instant
   in a mirror-symmetric position cannot pass: no symmetric rule can choose a side. Pinned by
   `twin_giants_on_one_bridge_is_not_a_symmetric_configuration` (1200 ticks). A symmetric breaker
   exists under the rotation (each unit yields to its own right) and is deliberately not adopted,
   because it would be an invented mechanic. Whether the real game breaks the tie with its PRNG or
   with something else is unknown. Note that under the 16.402 search the engine no longer needs it
   for the ordinary case: both Giants take the same bridge column, as they do live, and the
   measured contact law passes them.
2. **The river band after a princess falls.** The tilemap marks bridge cells neither water nor
   no-deploy, so a pure `NoDeploySize` model would let a troop deploy on that lane's bridge once
   its princess is down. Both engines keep the whole river band closed to troops, which is
   **unsourced**. One recording settles it.
3. **`ChargeRange`'s unit.** Only three cards carry it (Prince 250, Dark Prince 250, Battle Ram
   300). Every other distance column in that file is millitiles and every time column is ms, and
   250 is neither under the file's own conventions. Worse, all three charge cards ship `Speed 60`,
   so the distance reading (250 centitiles = 2.5 tiles) and the time reading (250 centiseconds =
   2.5 s) are algebraically degenerate in this data. Both come out at 50 ticks. It is a
   calibration key with candidates and a deciding observation, not a judgement call. What the
   shipped data *does* settle: `DamageSpecial` is exactly 2x `Damage` on all three, matching
   `DashDamage` on Assassin and Mega Knight where replacement is unambiguous, so the charged hit
   **replaces** rather than adds. `globals.csv` ships `CLONE_RESET_CHARGE=FALSE` and
   `CLONE_INHERIT_CHARGE=FALSE`, so the shipped data tells resetting charge apart from inheriting
   it, and charge therefore survives across the events those two flags name. The shipped data
   also settles by absence that the charged hit has no special range, min-range, load time,
   post-hit pause, attack interval or knockback. Measured live on client 16.402: charge progress
   accumulates `tdiv(step * 1000, ChargeRange)` per walking tick with `step = min(S, dist, 250)`,
   which doubles the unit's speed from its 43rd walking tick.
4. **Vintage differences between the two card tables.** `cards.json` is the 15.535.29 table and
   `cards-2018.json` the older one beside it; they disagree wherever a flag or a radius changed
   in between (Baby Dragon's `IgnorePushback` true in 2018 and false now; the Prince's collision
   radius 650 then and 600 now). Anything loaded from the 2018 table carries the 2018 answers;
   the shipped default is the 15.535 one.
5. **Card level.** The engine's default is `CardDb::lowest_level_valid_for_every_rarity`, the
   lowest unified level that exists for every rarity in the loaded table: 11 on the 15.535 table
   (Champion's `RelativeLevel` is 10) and 9 on the 2018 one. 11 is also the live game's
   tournament standard. The default is overridable.
