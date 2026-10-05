# Engine architecture

This page is for contributors reading or changing `crates/royalesim`. It covers how one tick runs,
where the engine's numbers come from, and which file holds what.

`crates/royalesim` is a deterministic, integer-only reimplementation of the Clash Royale battle
tick, written in Rust and exposed to Python through PyO3 as the extension module `royalesim`.
It simulates the game and nothing else: it knows nothing about rewards, observations or training,
and it never talks to the real client.

## Representation

| Quantity | Representation | Notes |
|---|---|---|
| Position | `fixed::Vec2`, two `i32` | subtiles; `representation.SUBTILE_PER_TILE` = 18000 per tile |
| Native arena unit | 1 millitile = 18 subtiles exactly | the unit the real client's own data and traces use; 1000 per tile |
| Arena | x in [0, 18000), y in [0, 32000) native units | 18 x 32 tiles |
| Tick | `time.TICK_MS` = 50 ms | 20 ticks per second |
| Squared distances | `i64` | they reach 3.3e11 in subtiles; an `i32` squared distance is a silent overflow |

**No floats.** There is no `f32` or `f64` anywhere in `crates/royalesim/src/` or its tests, and a
grep for them is part of the gate list (see `contributing.md`). All arithmetic is integer, in
`fixed.rs`. Overflow checks stay **on** in release builds (`Cargo.toml`), so a wrapped
multiplication fails loudly instead of producing a plausible wrong position.

## The tick

`lib.rs::TICK_PHASES` holds the eleven phases, run in order:

```
Upkeep  Status  Spawn  Target  Attack  Path  Move  Projectile  Resolve  Reap  Judge
```

A unit test in `lib.rs` asserts four ordering invariants, because each one is a behaviour, not a
preference:

- **Attack precedes Move** (`match.TICK_ORDER = client16402`, measured). The corpus
  discriminates the two orders and attack-before-move is the one that reproduces it: 819 ticks
  where a unit went from walking to attacking and none of them stepped, 412 of 413 the other way
  that walked the same tick, and 398 of 445 that stood still on the tick their deploy ended. The
  earlier order stays runnable as `legacy_move_before_attack` (`lib.rs::LEGACY_TICK_PHASES`).
- **Resolve follows every writer of the damage buffer** (Attack and Projectile). Damage is
  buffered during the tick and applied in one pass.
- **Reap follows Resolve**, or a death effect would fire before the death.
- Spawns and deaths are likewise deferred to their own phases.

The consequence is the property the whole engine leans on: **simultaneous outcomes cannot depend
on entity order.** Two units that kill each other on the same tick both die; two units that
deploy on the same tick are placed by a canonical key, not by whoever was appended first.

## Determinism

The engine is deterministic and seedable. The RNG is a PCG32 owned by the battle state (never a
module global), and its state is part of the serialized snapshot; `state_hash` is computed every
tick and is what `tests/battle.rs` and `tests/save_load.rs` compare. A trace recorded from the
engine re-verifies bit-for-bit through `royalegym.replay`.

Supercell's own generator has never been publicly recovered, so reproducing a real match
tick-for-tick is explicitly **not** the goal (`rng.GENERATOR` in the ledger says so). The goal is
behavioural fidelity plus bit-identical determinism within a build, which is what every test and
every replay depends on.

Save and load use a serde_json snapshot with card and arena fingerprints taken over the parsed
data, plus a `state_hash` self-check on load, so a snapshot cannot be silently restored against
different card data or a different arena.

## Where the numbers come from

Nothing in the crate hardcodes a number that belongs to data:

| Source | Holds | Loaded by |
|---|---|---|
| `data/calibration.json` | every physics constant, with its evidence, and ten measured card values that replace the 15.535 table's (`cards.CLIENT16402_VALUES`) | compiled in with `include_str!`; parsed in `state.rs` into `Calib` |
| `data/derived/cards.json` | card stats, generated from `data/raw/` | read by `card.rs` (through the env layer for Python callers) |
| `data/derived/arena.json` | the arena grid and tower geometry | compiled in with `include_str!`; `arena.rs` |

`calibration.json` is the ledger, and it is the subject of `calibration.md`. Because the crate
compiles two of these files in, a build can go stale against the files on disk. The env layer's
`RustEngine` compares the compiled-in **values** with the files and refuses a stale build. See
`contributing.md` for the rebuild rule.

## Selectable model arms

Where the engine has two implementations of one law, the ledger selects which one runs, and both
stay compiled and tested. Four such choices matter today:

- `pathfinding.PATH_SEARCH`: `client16402` (`path16402.rs`, the search measured on client
  16.402) is selected; `trace_fitted_astar` (`path2026.rs`) is the earlier frame-planned arm,
  refuted as a model of the client but kept runnable.
- `collision.CONTACT_LAW`: `client16402` (`move16402.rs`, driven by `state.rs phase_path16402`)
  is selected, beside the engine's earlier separation model.
- `match.TICK_ORDER`: `client16402` (`lib.rs::TICK_PHASES`, attack updates before move updates,
  the move pass in `Entities::creation_seq` order) is selected; `legacy_move_before_attack`
  (`LEGACY_TICK_PHASES`) is the refuted order, kept compiled and tested.
- `formation.GROUND_Y_CLAMP`: `client16402_deploy_column_range` is selected. A multi-unit
  summon's GROUND members are held inside the tap column's deployable y range, and that range is
  measured per SIDE, so side 1's is not the rotation of side 0's. `deploy_column_range_own_frame`
  reads side 0's formula in the owner's frame for both seats, and `none` drops the clamp.

The alternative arm is not dead code kept out of sentiment: it is seat-symmetric, and the
seat-symmetry gates (`tests/mirror.rs`, `tests/setup_spawn_order.rs`, and the rotation tests in
the env layer) run under it so that they keep catching seat bias in everything else. See
`pathfinding.md`, "Seats and frames".

## Seats and frames

A Red seat is the Blue seat turned through **180 degrees**, not reflected in y. Every tie-break
that decides behaviour is evaluated in the acting team's own frame, so that a rule cannot quietly
favour one side of the board. `tests/mirror.rs` checks this every tick over scripted and random
games, including multi-unit deploys, the centre column, and both seats casting the same spell on
the same tick.

Some laws are deliberate exceptions, each a measured property of the game rather than an engine
convenience. `symmetric_config` in `crates/royalesim/tests/common/mod.rs` lists every arm the
rotation gates select instead. Three of them follow. Routing: the client plans in **absolute arena
coordinates**, so a Red unit's route is not the rotation of its Blue twin's. The engine reproduces
that under the `client16402` arm, and the rotation gates run under the frame-planned arm instead.
`pathfinding.md` gives the evidence. The summon ground clamp (`formation.GROUND_Y_CLAMP`): the
deployable y range a multi-unit summon's ground members are held inside is measured per side, and
side 1's is not the rotation of side 0's. So a Red multi-unit GROUND deploy is not the rotation of
its Blue twin either. Flying members are untouched by the clamp, so they are the rotation of their
twins, and a single ground unit is held only at its column's back bound (the next law). The
rotation gates select `deploy_column_range_own_frame` for the same reason they select the
frame-planned search: so that the exception does not answer for everything else. Troop taps at the
own crown towers (`placement.TROOP_TOWER_TAPS`): the own king's no-deploy block is half-open in
absolute arena coordinates, so a troop tapped on its edge can be legal for one seat and refused to
the rotated twin. Side 1's single ground unit tapped behind its column's back bound stands on that
bound; side 0's bound is the arena's edge. The rotation gates select `closed_block`.

## Module map

| File | What it holds |
|---|---|
| `lib.rs` | phases, `Team`, the RNG, the top-level types |
| `state.rs` | the tick itself: every phase body, `Calib` (the parsed ledger), deploy validation, snapshots |
| `fixed.rs` | integer vector and distance math |
| `arena.rs` | the grid, water, deploy zones and territory, building footprints |
| `entity.rs` | entity storage, generational ids, the spatial hash |
| `card.rs` | card loading and level scaling |
| `target.rs` | target selection, range, sight, target lock and hysteresis |
| `path.rs` | the shared pathfinding interfaces and the earlier `PathModel` arms |
| `formation.rs` | where a card's N summons stand around the tap, and the deploy stagger |
| `jump16402.rs` | the measured river hop of a `JumpEnabled` troop |
| `status.rs` | the per-entity buff list and the two arithmetics that read it |
| `path2026.rs` | the trace-fitted A* arm |
| `path16402.rs` | the search measured on client 16.402 (selected) |
| `move16402.rs` | the 16.402 contact law: separation, avoidance and the step (selected) |
| `collide.rs` | the earlier separation and push-out passes |
| `combat.rs` | attacks, windup and cooldown, splash, projectiles |
| `spell.rs` | spell shapes, area effects, knockback settling |
| `py.rs` | the Python surface of the crate |

## The Python surface

`py.rs` exposes `Battle` and the supporting types as the `royalesim` extension module. The env
layer's `royalegym.rust_engine.RustEngine` wraps it, and the env layer is also where the data
directory is resolved (`royalegym.protocol.data_dir()`, overridable with `ROYALESIM_DATA_DIR`).
The crate itself has no Python dependency and builds alone.

- **One Rust call per env step.** `step(commands, ticks)` validates, applies and ticks without
  returning to Python, with the GIL released (`py.allow_threads`), so a threaded vectorised env is
  not serialised by it. Bulk state crosses as one JSON byte string (`state_json()`), which the env
  layer decodes with msgspec's typed C decoder.
- **The catalogue.** `Battle(card_names=None, ...)` loads every simulable non-tower card in
  `cards.json` order. The table holds 145 cards (103 troops, 15 buildings, 27 spells)
  and 335 units, the 15.535.29 client's 144 and the Minion Giant added last (`data/client_additions`), and `catalogue_json()` lists the ones that loaded while `CardDb::rejected` names
  the rest with the reason. That includes the Mirror (code 6) and the cards that travel under
  ground (the Miner and the Goblin Drill, code 5), which RoyaleGym places. `path_search="trace_fitted_astar"`
  selects the frame-planned arm and `ground_y_clamp="deploy_column_range_own_frame"` the own-frame
  summon clamp (see "Selectable model arms"); each defaults to the ledger's value,
  `pathfinding.PATH_SEARCH` and `formation.GROUND_Y_CLAMP`.
- **Units and their levels.** Each entity row of `state_json()` ends with `level`, `mount_uid` (the
  unit a rider rides, -1 for any other entity), `charge`, `dest_x`, `dest_y`, `ability_ticks` and
  `unit_type` (`ENTITY_FIELDS` names them all). `level` is the unified level
  the entity plays at: a played unit's card level, a Mirror's copy one above it, a Clone's copy the
  Clone's, a unit another unit puts down its parent's, a crown tower its tower level. `card_id` is the
  card that put the unit down; `unit_type` is the unit's own record, an index into
  `unit_types_json()`. That list holds every troop and building unit name in the loaded card table,
  towers included, sorted, so one unit type has one id whichever card made it, and
  `unit_types_digest()` (FNV-1a 64 of the list's JSON) tells two tables' numberings apart.
  `unit_hitpoints(card_id, level)` lists every unit a card puts on the board, as (role, unit name,
  hitpoints) at the level each one takes. The card's own row comes first; the other roles are
  `second_summon`, `spawn`, `death_spawn` and `release` (`royalesim.UNIT_ROLES`). So a unit that
  matches no catalogue row by its hitpoints can still be priced.
- **Deploy rules as data.** The alive-enemy-tower no-deploy rects, water, the arena bitmask and
  occupancy are queryable (`check_deploy`, `tower_no_deploy_rects`, `passable_half_cells`,
  `tower_positions`), so a learner can build its action mask from the engine's own numbers and
  check it against `check_deploy`, which gives the engine's answer for any tap. `check_deploy(team, slot, x, y)` returns an index into `DEPLOY_REASONS`, which
  holds eighteen codes: `OK`, `BAD_TEAM`, `BAD_SLOT`, `EMPTY_SLOT`, `NOT_ENOUGH_ELIXIR`,
  `OUT_OF_ARENA`, `WATER`, `NO_DEPLOY`, `OUT_OF_TERRITORY`, `OCCUPIED`, `GAME_OVER`,
  `DUPLICATE_TEAM`, `ENGINE_ERROR`, `TOO_EARLY`, `NOTHING_TO_MIRROR`, and three that answer a
  press of an ability button rather than a tap: `NO_HERO`, `ABILITY_NOT_READY` (a champion's
  charge has not come back yet) and `ABILITY_SPENT` (a hero has used its charge). A match refuses every
  deploy for its opening `match.DEPLOY_LOCKOUT_TICKS` (90 ticks), so at tick 0 every answer is
  `TOO_EARLY`. For example, with a Giant in hand slot 0, once those 90 ticks have passed (re-run
  2026-09-29 on RoyaleSim `2245f9f`):

  ```python
  import json, royalesim

  deck = ["Giant", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"]
  b = royalesim.Battle(card_names=deck, slot_of_k=[[0, 1, 2], [0, 1, 2]])
  b.reset(seed=1, decks=[list(range(8))] * 2, shuffle=0, start_tick=0,
          elixir_milli=[10_000, 10_000], tower_hp=None, spawns=[])
  calib = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
  b.step([], calib["match"]["DEPLOY_LOCKOUT_TICKS"]["value"])   # wait out the opening lockout

  T, R = royalesim.SUBTILE, royalesim.DEPLOY_REASONS
  for label, (x, y) in [("own half", (5, 10)), ("the river", (9, 16)),
                        ("enemy half", (9, 25)), ("off the map", (40, 10))]:
      print(f"{label:12s} {R[b.check_deploy(0, 0, x * T, y * T)]}")
  ```

  ```
  own half     OK
  the river    WATER
  enemy half   OUT_OF_TERRITORY
  off the map  OUT_OF_ARENA
  ```

- **Snapshots.** `save()` / `load()` round-trip a battle to a byte string and back to the identical
  state hash; the size grows with the number of live entities. `royalesim.SNAPSHOT_FORMAT` names
  the current format, and `state.rs` lists every format and what each one changed.
