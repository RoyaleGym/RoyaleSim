# The calibration ledger

`data/calibration.json` holds every physics constant the engine runs on, and each one carries how
well it is known. Nothing in the Rust core or in the Python layer may hardcode a number that
appears in this file.

## Why it exists

This project's predecessor spent weeks trying to make pathfinding match the real game. The cause
was not difficulty: plausible numbers and measured numbers were stored the same way, so a wrong
guess was indistinguishable from a fact, and later tuning absorbed the error instead of revealing
it. A constant fitted to the one case anybody checked makes a wrong **law** read as right, on
every board at once.

The ledger's job is to keep those two kinds of number apart, permanently and visibly.

## What an entry looks like

```jsonc
"PATH_SEARCH": {
  "value": "client16402",
  "status": "measured",
  "confidence": "HIGH",
  "candidates": ["client16402", "trace_fitted_astar"],
  "provenance": "...the client version, the captures, and what was compared...",
  "promotion_rules": "...the observation that would settle or overturn it...",
  "engine_contract": "...which code reads it, and what it changes..."
}
```

| Field | Meaning |
|---|---|
| `value` | what the engine runs on. Compiled into the crate; changing it needs a rebuild |
| `status` | how the value is known — the vocabulary below |
| `confidence` | HIGH / MEDIUM / LOW, and which *part* of a key is which when a source settles only part of it |
| `candidates` | the other values the engine implements. `state.rs::pick` refuses a candidate string with no implementation, so a candidate is always runnable |
| `provenance` | the evidence: the client version, the trace or capture, the counts |
| `promotion_rules` | the observation that would raise the status, written before it is made |
| `engine_contract` | which code reads the key and what changes when it changes |

### Status vocabulary, in increasing order of trust

| Status | Meaning |
|---|---|
| `guess` | nobody has evidence; a placeholder so the engine runs |
| `disputed_existence` | the key names something no shipped data or recording shows exists; ranked with `guess`, because nobody has evidence either way |
| `hypothesis` | an argument from the shape of the data, not an observation |
| `community` | multiple independent third parties agree, with no primary source |
| `datamined` | taken from shipped game data — state the file and the vintage |
| `measured` | observed in the real client by this project's own instruments |
| `owner_ruling` | a maintainer's direct observation of the live client, quoted verbatim and dated |

`owner_ruling` ranks **with** `measured`, not below it: a direct observation of the live client is
a primary source, and the live client is the target. A ruling may settle only part of a key — the
sign of a push but not its vector, say — in which case `confidence` names which half is which and
a `promotion_rule` stays open for the rest.

## How far to trust the provenance

The status vocabulary is one claim and the provenance prose is another, and they are not
equally well checked.

A re-read on 2026-09-22 went through 24 of the 149 entries against the corpus. It moved no
status and no value. What it turned up was in the evidence the statuses rest on: 42 places
where a cited number, recording name or piece of arithmetic does not hold. Take that as a
reason to re-derive, not as 42 established defects. Only a handful of the 42 have since been
recomputed by hand, and one of those did not survive the recomputation. The supported claim is
that the set needs re-reading.

`formation.GROUND_Y_CLAMP` is the worked example. Its status of `measured` was defensible and
four of its statements were wrong, including a capture whose real numbers are 31053/31057
where the entry said 31000. It has been rewritten. What is true of that key now: the clamp is
pinned by 34 members over 17 clean groups, the two seats' back-edge bounds are a full row apart
rather than half a row, side 1's river bound is one native unit looser than the rotation rather
than tighter, and side 0's range is pinned by nothing at all, which the entry now says.

125 entries have not been re-read. So, concretely:

- **The status on a key is worth trusting.** No status moved in the re-read.
- **The shape of an entry is worth trusting where a judgement was made, but it is not
  universal, so check rather than assume.** All 149 carry a status. 89 name the rivals the
  value was chosen against and 99 state what would move it, and 86 do both. The gap is mostly
  the 40 `datamined` keys, where the number was read out of a shipped table and no choice was
  made, so a candidate list would be a category error; those carry a vintage and an engine
  contract instead, which is the right shape for them. But the gap is not only those. 16 of
  the 49 `measured` entries name no rival at all, and 12 of those state no promotion criterion
  either. In a file whose rule is that evidence is discrimination and never origin, a measured
  key with no candidate list has recorded nothing that it was discriminated against. Some are
  harmless (`time.TICK_MS` has no plausible rival); `pathfinding.PATH_GOAL_RULE` and
  `movement.CONTACT_DOMAIN` are exactly the kind of rule that should say what it beat. They
  are on the re-read list.
- **Any specific number inside a provenance string is worth re-deriving before you build on
  it, and that includes a rival's score and a promotion criterion.** Those are not safer than
  the rest. Of the 24 entries re-read, 10 findings land on a candidate list and 7 on a
  promotion rule. `GROUND_Y_CLAMP`'s fourth error was in its `promotion_rules`, which called
  side 1's river bound undiscriminated while a clean group stood two Goblins exactly on it.
  `targeting.ATTACK_RANGE_RULE` states a losing candidate's sum as 8500, which is Range plus
  the tower's own radius, the very term that arm drops; the real sum is 8100, so the
  observation still discriminates, but eight ticks later than the entry implies.

The ledger is checked against its own corpus, and that check is not finished.

## Changing a value

Change a value only with new evidence, and write the evidence into the entry: what was measured,
on which client version, and in which trace or capture.

`oracle/calibrate.py` enforces the ordering. A value at `measured` or `owner_ruling` cannot be
overwritten with a different value without `--supersede`, so a later result that disagrees with an
earlier measurement is refused rather than applied quietly. Superseded readings stay in the
entry's history instead of being deleted — an argument that loses to a measurement belongs beside
what overturned it.

After changing any `value`, rebuild: `..\.venv\Scripts\maturin develop --release`. Prose-only
edits need no rebuild (see `contributing.md`).

## Ground truth, and which source wins

Three bodies of evidence sit behind the `measured` entries:

| Source | Client | Where |
|---|---|---|
| Offline traces | 15.535.29 | `data/oracle-native/` (gitignored, large) |
| Live captures | 16.402 | recorded from the real game by the client instrument; each entry names the capture it rests on, and the recordings are not distributed |
| Shipped game data | 2016-2018 vendored, 2023 cross-reference | `data/raw/` |

Where the two clients disagree, **the live 16.402 client wins** and the entry says so: the target
is the live game, not a frozen build. Shipped data from 2016-2018 is evidence, not spec — ground
movement was rewritten on 2025-03-31, so anything pre-2025 about movement is archaeology until it
is re-measured.

`tools/oracle_diff.py` diffs the engine against a trace tick by tick;
`crates/royalesim/tests/oracle2026.rs` gates the recorded first paths.

## Open keys and what would settle them

Everything below is at `guess`, `hypothesis`, `community` or `datamined`-but-unverified, which
means the engine runs on a placeholder and the behaviour it produces is not evidence about the
real game. Each row names what a recording would have to show. The ledger is the authority; this
table is a reading guide over it, and a key's own `status` and `promotion_rules` win where the two
disagree. Of the 148 keys with a status, 48 are `measured` and one is an `owner_ruling`.

| Key | Value today | Status | Settled by |
|---|---|---|---|
| `collision.PUSH_MODEL` | `mass_weighted` | guess, LOW | a mass-ladder recording: units of known Mass pushing each other |
| `collision.BUILDING_FOOTPRINT_MODEL` | `collision_radius_circle` | guess, LOW | a walk past a building; note that circle and 2x2 box differ by 0.044 tile at best, so only 3x3-vs-not is separable |
| `collision.SEPARATION_ITERATIONS` | 1 | guess, LOW | a crowd recording with per-tick positions |
| `pathfinding.TIE_BREAK` | `ortho_first_placeholder` | guess, LOW | read only by the trace-fitted arm (`path2026.rs`); the selected arm reproduces the published node lists outright, so the key no longer gates it |
| `combat.DAMAGE_ARITHMETIC` | `integer` | guess, LOW | hit counts to kill a tower at known levels |
| `combat.CROWN_TOWER_DAMAGE_ROUNDING` | `ceil_kept_share` | community, MEDIUM | a crown tower's displayed hp before and after a spell at two card levels |
| `targeting.LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET` | 25 | datamined, LOW | vendor the modern data, or measure a target held past its range |
| `targeting.LOGIC_XPOS_BASED_TOWER_TARGETING` | true | datamined, LOW | a centre-column deploy: which tower it walks at |
| `match.LOGIC_BATTLE_START_COOLDOWN_MS` | 4500 | datamined, LOW | any recording of a match start |
| `match.KING_ACTIVATE_TIME_MS` | 3300 | datamined, MEDIUM | a recording of what the delay actually delays |
| `arena.ARENA_SOURCE_VINTAGE` | ~2018 tilemap | datamined, MEDIUM | a calibrated screenshot of a live arena; bridge width varied by arena even in 2018 |
| `knockback` (5 of 8 keys) | the measured ladder, with its duration, water, stacking, zero-vector and deploying-unit edges unfixed | guess / hypothesis, LOW-MEDIUM | each key's `promotion_rules` names the capture it needs. `DISPLACEMENT_LAW` and `ATTACK_RESET` are measured and `DIRECTION_ROLLING` is an `owner_ruling` |
| `spells.*` (8 keys) | see `spell-spec.md` | guess / hypothesis, LOW | each key in `spell-spec.md` carries its own deciding observation |
| `status.*` (stun and buff timing) | see `spell-spec.md` | community / hypothesis | likewise |
| `rng.GENERATOR` | `pcg32` | guess, LOW | not settleable, and not a goal — see `architecture.md`, Determinism |

The keys that carry the measured 2026 movement and pathfinding model — `time.TICK_MS`,
`time.SPEED_TO_SUBTILES_PER_TICK`, `time.PROJECTILE_SPEED_TO_SUBTILES_PER_TICK`,
`pathfinding.PATH_SEARCH`, `collision.CONTACT_LAW`, the `movement.*` section, and the cost, goal
and replan keys — are at `measured`/HIGH. Their evidence is in `pathfinding.md` and
`movement-measurements.md`.

These keys were measured later, on the 16.402 corpus, and are `measured` too:

| Key | What it settles |
|---|---|
| `match.TICK_ORDER` | attack updates before move updates, the move pass in creation order |
| `movement.JUMP_WATER_HOP` | a `JumpEnabled` troop's river hop |
| `movement.DYING_UNIT_VISIBILITY` | whether a dying neighbour is still an obstacle this tick |
| `combat.STAT_BASE_LEVEL`, `combat.TOWER_HITPOINT_LADDER` | level scaling and the crown-tower ladder |
| `combat.ATTACK_CYCLE`, `combat.PROJECTILE_LAUNCH`, `combat.KAMIKAZE_DEATH` | the attack cycle, the launch point, the kamikaze death |
| `lifetime.HP_DECAY` | a building's hit-point drain over its lifetime |
| `formation.LAYOUT`, `DEPLOY_STAGGER`, `GROUND_Y_CLAMP` | where a card's summons stand, and when each appears |
| `spawner` (6 of 10 keys) | emission timing, the first wave, the start-time origin, the two deploy-time defaults, the death-spawn layout |
| `knockback.DISPLACEMENT_LAW`, `ATTACK_RESET` | the push ladder and what a landed push does to the attack |
| `charge.CHARGE_RANGE_UNIT`, `CHARGED_HIT_TIMING` | the run-up's unit and when the charged hit lands |

The `hide.*` and `status.*` sections are community and hypothesis throughout; each key carries the
observation that would settle it.
