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
  table stays beside it as `cards-2018.json`, which the format-3 snapshot fixture reads).
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
| Entity storage, generational ids, spatial hash | `entity.rs` | the hash is verified against brute force |
| Card loading and level scaling | `card.rs` | per-rarity multipliers from `rarities.csv`; projectile damage is authoritative over the character row |
| Targeting | `target.rs` | edge-to-edge range, target lock once windup starts, keep-target hysteresis, lane/x default tower, building-only targeters, sight range |
| Pathfinding, two arms | `path16402.rs` (selected), `path2026.rs`, `path.rs` | the measured 16.402 search; see `pathfinding.md` |
| Movement and contact | `move16402.rs` (selected), `collide.rs` | the measured 16.402 separation, avoidance and step law |
| Melee and ranged attacks | `combat.rs` | windup, cooldown, splash, `self_as_aoe_center`. Integer damage; the rounding law is a guess (`combat.DAMAGE_ARITHMETIC`) |
| Projectiles, shields, death damage, lifetime expiry | `combat.rs`, `state.rs` | |
| Multi-unit deploy formation | `formation.rs`, `state.rs::formation_members` | the ring, line and spiral the corpus shows, with the per-member deploy stagger and the ground-column clamp (`formation.LAYOUT` / `DEPLOY_STAGGER` / `GROUND_Y_CLAMP`, measured). The old square grid stays runnable as the `engine_grid` arm. `tests/formations.rs` |
| Deploy time | `state.rs` | units are inactive while `deploy_ms > 0` |
| Hide (Tesla) | `state.rs::hide_pass`, `entity.rs::HideState`, `target.rs`, `combat.rs::resolve` | `HidesWhenNotAttacking` / `HideTimeMs` / `UpTimeMs` from `cards.json`: under at deploy end, rising for `UpTimeMs` when a targetable enemy is in sight, under again after `HideTimeMs` without a target; hidden = untargetable and immune (except lifetime expiry), stun and knockback pass over it. Rules the columns do not settle are the `hide.*` ledger keys, community-sourced. `tests/hide.rs` |
| Periodic spawners, death spawn | `state.rs::spawner_pass`, `state.rs::phase_reap`, `card.rs::SpawnerDef` / `DeathSpawnDef` | the `Spawn*` / `DeathSpawn*` columns (huts, Witch, Dark Witch; Tombstone, Golem, Lava Hound, Battle Ram): waves on the data's cadence, one-tick emission latency, stun pauses the timer, `SpawnLimit` counts the spawner's own live units, death spawns on a ring of `DeathSpawnRadius` on the dying unit's facing (`spawner.DEATH_SPAWN_LAYOUT = facing_ring`, measured). Six `spawner.*` keys are measured: EMISSION_TIMING, FIRST_WAVE, START_TIME_ORIGIN, SPAWNED_DEPLOY_TIME, DEATH_SPAWN_DEPLOY_TIME_DEFAULT, DEATH_SPAWN_LAYOUT; SPAWN_POINT, PAUSE_ANCHOR, LIMIT_RULE and DEATH_SPAWN_RADIUS_DEFAULT are still community / hypothesis / guess. `tests/spawner.rs` |
| Death area effect (Ice Golem) | `state.rs::phase_reap`, `spell.rs::cast`, `card.rs::convert_area_effect` | `DeathAreaEffect` names a row of `cards.json`'s `area_effect_objects`. The death leaves that area standing where the unit stood, and it runs the engine's own area-effect path. That is the same object, the same disc test and the same buff a Zap gets. The Ice Golem's is a 2-tile disc that hangs a 30 % slow on enemies for 2 s and carries no damage of its own. It is a **second** effect of the same death, beside the `DeathDamage` disc: the two carry their own radius, damage and crown percent, and the Super Ice Golem ships them with different values for each. Refused areas are refused card and all, with the reason (the Rage Barbarian's and the Suspicious Bush's are spawn scripts). `tests/death_area_effect.rs` |
| Death bomb (Balloon, Giant Skeleton, Bomb Tower) | `card.rs::convert_death_bomb`, `state.rs::phase_reap`, `spell.rs::step_spells` | Their `DeathSpawnCharacter` names a row with no hitpoints, no damage, no hit speed and no LifeTime. It carries only `DeployTime` 3000, `DeathDamage` and `DeathDamageRadius`. That is not a unit, so it is not spawned as one: the death leaves **one area hit on a timer** at the point of death, carried by the same spell object an Arrows wave waits in. It is untargetable and blocks nothing, because it is not on the board at all. The impact is the engine's ordinary `DeathDamage` disc with a fuse: enemies only, air and ground per the row, crown towers at the row's percent. The fuse is `DeployTime` and that is measured, not read off the column name: in capture `20260920-083112` (both seats) a level-11 Balloon's bomb takes 240 off a King Tower 1987 native units away **61 ticks** after its last live frame, and 240 is `BalloonBomb`'s `DeathDamage` 94 on the Common ladder at level 11. `DeathPushBack` (Giant Skeleton, 1800) is not carried into `cards.json` and is not applied, the same gap every other `DeathDamage` row has. `tests/death_bomb.rs` |
| Charge (Prince, Dark Prince, Battle Ram) | `state.rs::charge_pass`, `effective_speed`, `combat.rs::fire` | `ChargeRange` / `DamageSpecial` / `ChargeSpeedMultiplier`: the run-up accumulates `tdiv(L x 1000, ChargeRange)` permille per walking tick from the requested step `L = min(S, dist, 250)`, charged at 10000; the speed doubles from the next walking tick (measured on the corpus: the Prince's 43rd walking tick) and the next landed hit deals `DamageSpecial`; consumed by the hit, reset by a stun or a landed knockback. The `charge.*` keys hold what the corpus has not yet separated. `Kamikaze` is read (`combat.KAMIKAZE_DEATH = at_fire`): a Battle Ram dies on the tick its one hit lands and breaks into its two Barbarians; a delayed kamikaze (`KamikazeTime`) is the named gap. `tests/charge.rs` |
| Stun | `entity.rs`, `state.rs`, `status.*` keys | honoured by move and attack; applied by Zap. Attack reset, retarget-on-resume and the deploy-pause question are each their own ledger key |
| Status effects | `status.rs`, `state.rs::buff_pulse_pass` | a per-entity buff list: `SpeedMultiplier` and `HitSpeedMultiplier` compose into the move and attack arithmetic (`status.BUFF_STACKING`, `status.SAME_BUFF_REAPPLY`, `movement.BUFF_SPEED_COMPOSITION`), a full-stop buff drives the hold (`status.FULL_STOP_BUFF_IS_STUN`), and damage over time and heal pulse on their own clock (`status.BUFF_PULSE_AMOUNT` / `BUFF_PULSE_TIMING`), so Poison and Earthquake load as pulsing areas. `tests/status.rs` |
| Elixir, hand, cycle | `state.rs` | double elixir when `MANA_SPEED_UP_WHEN_REMAINING_SECONDS` remain, and in overtime |
| Match timing | `state.rs::phase_judge` | regular time, 60 s sudden-death overtime, 3-crown instant win |
| Post-overtime tiebreak | `state.rs::overtime_tiebreak` | when overtime runs out level on crowns, the side whose weakest standing crown tower is weaker loses (`match.OVERTIME_TIEBREAK`, community-sourced, `lowest_tower_hp_absolute`); an exact tie stays a Draw. `tests/tiebreak.rs` |
| King activation | `state.rs` | on king damage or a lost princess tower, after `match.KING_ACTIVATE_TIME_MS` |
| Spells | `spell.rs`, `card.rs`, `state.rs` | `SpellShape` Projectile / AreaEffect / Rolling / PulsingAreaEffect (Poison, Earthquake). Fireball, Arrows, Zap, The Log and Goblin Barrel are specced card by card; Rocket, Freeze, Poison, Earthquake and Snowball load and are not. Fed the 15.535 card data. Territory rules per card. Spec and sourcing: `spell-spec.md` |
| Knockback | `move16402.rs::start_pushback` / `pushback_step`, armed in `state.rs::arm_ladder` | the measured ladder (`knockback.DISPLACEMENT_LAW = client16402`): a speed of 25n native units per tick toward a point `min(Pushback, MAX_PUSHBACK_LENGTH)` away, falling by 25 each tick, with one 25-unit back-step at the end and the path dropped when it stops. `knockback.STACKING = first_wins_while_active`; `IgnorePushback` / `PushbackAll` are honoured; a landed push resets the attack and keeps the target (`knockback.ATTACK_RESET`, measured). The earlier fixed-distance slide stays runnable under the same key. See "Knockback" below |
| Crown tower damage reduction | `combat.rs` | `combat.CROWN_TOWER_DAMAGE_ROUNDING`, ceil, community-sourced |
| Determinism, save/load | `lib.rs`, `state.rs` | seeded PCG32, `state_hash` every tick, snapshot fingerprints |
| Seat symmetry | `tests/mirror.rs` and friends | 180-degree rotation, checked every tick; see `architecture.md` |
| The Python surface | `py.rs` | the env layer runs a full 18-card battle on it with no changes of its own |

## Not modelled

Each row says what a caller sees instead. That is what matters when you are deciding whether the
engine is usable for your purpose.

| Mechanic | What happens instead |
|---|---|
| `DeathSpawnPushback`, `DeathSpawnMinRadius` | not read: a death spawn's units land on the facing ring and nothing pushes them apart or holds them off a minimum radius, so a crowded death point leaves them closer together than the game does |
| Dash, morph, chained hits, multiple projectiles | absent, and so is the attack jump (Mega Knight, Assassin). The river hop IS modelled (`jump16402.rs`, `movement.JUMP_WATER_HOP`) |
| Rage and Heal | refused by the loader, and out loud: neither card row carries an area effect or a projectile at all. Each works by summoning a bottle whose death releases an own-troop area, and the bottle has no hitpoints, so the engine will not put it on the board. A death that releases an area IS modelled (see the Modelled table); an area that buffs the releaser's own side is not, because `impact` has no filter for it |
| Evolutions, champions, tower troops | post-2023; no public data |
| A troop's own projectile knockback (`Pushback` on the projectile row) | not read: a troop's projectile is loaded as speed, damage, splash radius and a buff, and nothing else. Bowler, Zappies and the Mega Knight's landing hit push in the game and do not here. A SPELL's knockback is read (`spell.rs`, the measured ladder) |
| The splash layer filter (`AoeToAir` / `AoeToGround` on the projectile row) | not read: the engine filters a splash by the ATTACKER's `AttacksAir` / `AttacksGround` (`combat.rs`). The two agree on every card the slice reaches; they disagree on Wall Breakers, whose blast covers air in the data and only ground here (`tools/check_card_reads.py` names every row where they part) |
| The real intra-tick order and the real PRNG | out of reach, and not a goal |

### Cards outside the slice

`data/derived/cards.json` holds **144 cards** (101 troops, 16 buildings, 27 spells) and 334
units. The engine loads every simulable non-tower card of it;
`Battle(card_names=None).catalogue_json()` lists what loaded and `CardDb::rejected` what did
not, with the reason. Some of the loaded cards carry a mechanic `card.rs` never parses, so an
8-card deck drawn uniformly from the whole catalogue is much more likely than not to hold one.
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

**What is left is a real defect, and it is the opposite one.** The engine skips the whole move pass
for an ATTACKING unit, so separation never reaches it. Place a Cannon on a Knight that is attacking
something: with the Knight 814 from the Cannon's centre it stays at 814 for more than 58 ticks, and
starting inside the circle at 316 it stays there for more than 80. The game pushes an attacking unit
out. One recorded Skeleton goes 814, 955, 1104 over three ticks while attacking. So the engine is
not too permissive here; it is too rigid, and only for units that are attacking.

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

The engine holds a unit's target and its planned route for the whole of a knockback ladder.

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

One part of the current model sits at `owner_ruling` rather than `measured` (see
`calibration.md`). `knockback.DIRECTION_ROLLING = travel_direction` is HIGH confidence on the
**sign** and LOW on the **vector** for an off-axis victim. The sign is settled: no victim is ever
pushed backward, toward the caster. The vector is not. The code also asserts zero sideways
component for a troop standing to the side of The Log's roll, and that half is unobserved. A
single recording settles it.

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
