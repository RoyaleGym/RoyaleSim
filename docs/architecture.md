# Engine architecture

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
Upkeep  Status  Spawn  Target  Path  Move  Attack  Projectile  Resolve  Reap  Judge
```

Four ordering invariants are asserted by a unit test in `lib.rs`, because each one is a
behaviour, not a preference:

- **Move precedes Attack** — a unit that moves into range attacks on the same tick, which is what
  the real game visibly does.
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
| `data/calibration.json` | every physics constant, with its evidence | compiled in with `include_str!`; parsed in `state.rs` into `Calib` |
| `data/derived/cards.json` | card stats, generated from `data/raw/` | read by `card.rs` (through the env layer for Python callers) |
| `data/derived/arena.json` | the arena grid and tower geometry | compiled in with `include_str!`; `arena.rs` |

`calibration.json` is the ledger, and it is the subject of `calibration.md`. Because the crate
compiles two of these files in, a build can go stale against the files on disk; the env layer's
`RustEngine` compares the compiled-in **values** with the files and refuses a stale build. See
`contributing.md` for the rebuild rule.

## Selectable model arms

Where the engine has two implementations of one law, the ledger selects which one runs, and both
stay compiled and tested. Two such choices matter today:

- `pathfinding.PATH_SEARCH` — `client16402` (`path16402.rs`, the search measured on client
  16.402) is selected; `trace_fitted_astar` (`path2026.rs`) is the earlier frame-planned arm,
  refuted as a model of the client but kept runnable.
- `collision.CONTACT_LAW` — `client16402` (`move16402.rs`, driven by `state.rs phase_path16402`)
  is selected, beside the engine's earlier separation model.

The earlier arm is not dead code kept out of sentiment: it is seat-symmetric, and the
seat-symmetry gates (`tests/mirror.rs`, `tests/setup_spawn_order.rs`, and the rotation tests in
the env layer) run under it so that they keep catching seat bias in everything else. See
`pathfinding.md`, "Seats and frames".

## Seats and frames

A Red seat is the Blue seat turned through **180 degrees**, not reflected in y. Every tie-break
that decides behaviour is evaluated in the acting team's own frame, so that a rule cannot quietly
favour one side of the board. `tests/mirror.rs` checks this every tick over scripted and random
games, including multi-unit deploys, the centre column, and both seats casting the same spell on
the same tick.

The one deliberate exception is routing. The client plans in **absolute arena coordinates**, so a
Red unit's route is not the rotation of its Blue twin's; the engine reproduces that under the
`client16402` arm, and the rotation gates run under the frame-planned arm instead. This is a
measured property of the game, not an engine convenience — `pathfinding.md` gives the evidence.

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
  `cards.json` order: 65 on 2026-09-21 (52 troops, 6 buildings, 7 spells); `catalogue_json()` lists
  them. `path_search="trace_fitted_astar"` selects the frame-planned arm (see "Selectable model
  arms"); the default is the ledger's `pathfinding.PATH_SEARCH`.
- **Deploy rules as data.** The alive-enemy-tower no-deploy rects, water, the arena bitmask and
  occupancy are queryable (`check_deploy`, `tower_no_deploy_rects`, `passable_half_cells`,
  `tower_positions`), so a learner's action mask is the engine's own answer rather than a
  reimplementation. `check_deploy(team, slot, x, y)` returns an index into `DEPLOY_REASONS`, which
  holds thirteen codes: `OK`, `BAD_TEAM`, `BAD_SLOT`, `EMPTY_SLOT`, `NOT_ENOUGH_ELIXIR`,
  `OUT_OF_ARENA`, `WATER`, `NO_DEPLOY`, `OUT_OF_TERRITORY`, `OCCUPIED`, `GAME_OVER`,
  `DUPLICATE_TEAM`, `ENGINE_ERROR`. For example, with a Giant in hand slot 0 at the start of a
  battle (re-run 2026-09-21):

  ```python
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
  state hash: roughly 7-11 KB for a mid-game board depending on what is on it (6.7 KB at 9 live
  entities, measured 2026-09-21). `SNAPSHOT_FORMAT` is 6.
