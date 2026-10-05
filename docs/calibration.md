# The calibration ledger

`data/calibration.json` holds every physics constant the engine runs on, and each one carries how
well it is known. Read this page before you rely on one of those constants or change one. It tells
you what an entry means, how far to trust it, and which keys are still open. Nothing in the Rust
core or in the Python layer may hardcode a number that appears in this file.

## Why it exists

This project's predecessor spent weeks trying to make pathfinding match the real game. The cause
was not difficulty. Plausible numbers and measured numbers were stored the same way, so a wrong
guess was indistinguishable from a fact. Later tuning then absorbed the error instead of revealing
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
| `status` | how the value is known, from the vocabulary below |
| `confidence` | HIGH / MEDIUM / LOW, and which *part* of a key is which when a source settles only part of it |
| `candidates` | the other values the engine implements. `state.rs::pick` refuses a candidate string with no implementation, so a candidate is always runnable |
| `provenance` | the evidence: the client version, the trace or capture, the counts |
| `promotion_rules` | the observation that would raise the status, written before it is made |
| `engine_contract` | which code reads the key and what changes when it changes |
| `refuted_for` | the part of the space this entry's own shipped `value` is known to be wrong for, or `*` for all of it. It sits beside the `confidence` that says so |
| `supersedes_globals` | present when a measured value departs from a shipped table ON PURPOSE: the key, the value that table holds, and why. `tools/extract_globals.py` reports a declared divergence and fails an undeclared one, so the two can differ without breaking the build and cannot differ silently |

### Status vocabulary, in increasing order of trust

| Status | Meaning |
|---|---|
| `guess` | nobody has evidence; a placeholder so the engine runs |
| `disputed_existence` | the key names something no shipped data or recording shows exists; ranked with `guess`, because nobody has evidence either way |
| `hypothesis` | an argument from the shape of the data, not an observation |
| `community` | multiple independent third parties agree, with no primary source |
| `third_party_measured` | one outside measurement, its source, method and sample named, not reproduced by this project's instruments; ranked with `community` |
| `datamined` | taken from shipped game data. State the file and the vintage |
| `measured` | observed in the real client by this project's own instruments |
| `owner_ruling` | a maintainer's direct observation of the live client, quoted verbatim and dated |

`third_party_measured` ranks **with** `community`, and unlike `measured` it is never overwrite-protected: this project's own measurement replaces it without `--supersede`. It was added on 2026-09-24, when two outside readings were found filed as `measured`, a status that would have resisted the very correction that should replace them.

`owner_ruling` ranks **with** `measured`, not below it: a direct observation of the live client is
a primary source, and the live client is the target. A ruling may settle only part of a key, the
sign of a push but not its vector, say. In that case `confidence` names which half is which and
a `promotion_rule` stays open for the rest. No entry holds this status today. The only one did:
`knockback.DIRECTION_ROLLING`, a ruling on the sign of the Log's push. A measurement overturned
it on 2026-09-25, and the ruling is kept in that entry's `supersedes`.

## How far to trust the provenance

The status vocabulary is one claim and the provenance prose is another, and they are not
equally well checked.

A re-read on 2026-09-22 went through 24 entries against the corpus, of the 148 the ledger held
that day. The ledger has grown since. Counted against it today, the re-read
went through 24 of the 470 entries. It moved no status and no value. What it turned up was in
the evidence the statuses rest on: 42 places where a cited number, recording name or piece of
arithmetic does not hold. Take that as a reason to re-derive, not as 42 established defects.
Only a handful of the 42 have since been recomputed by hand, and one of those did not survive
the recomputation. The supported claim is that the set needs re-reading.

`formation.GROUND_Y_CLAMP` is the worked example. Its status of `measured` was defensible and
four of its statements were wrong, including a capture whose real numbers are 31053/31057
where the entry said 31000. It has been rewritten. What is true of that key now: the clamp is
pinned by 34 members over 17 clean groups, the two seats' back-edge bounds are a full row apart
rather than half a row, side 1's river bound is one native unit looser than the rotation rather
than tighter, and side 0's range is pinned by nothing at all, which the entry now says.

446 entries have not been re-read. So, concretely (every count on this page comes from
`python tools/ledger_census.py`, and `tests/test_ledger_census.py` fails when the page and the
ledger disagree, because these figures went stale twice in one afternoon before that gate existed):

- **The status on a key is worth trusting.** No status moved in the re-read.
- **The shape of an entry is worth trusting where a judgement was made, but it is not
  universal, so check rather than assume.** All 470 carry a status. That 470 counts TOP-LEVEL
  entries; one further entry, `pathfinding.PATHFINDING_COSTS.application`, is nested inside
  another and carries its own `measured` status, so counting every status in the file gives 471
  and 357 measured. The tools agree on 470 by convention, and the convention undercounts by one. 411 name the rivals the
  value was chosen against and 425 state what would move it, and 404 do both. The gap is mostly
  the 30 `datamined` keys, where the number was read out of a shipped table and no choice was
  made, so a candidate list would be a category error; those carry a vintage and an engine
  contract instead, which is the right shape for them. But the gap is not only those. 26 of
  the 356 `measured` entries name no rival at all, and 13 of those state no promotion criterion
  either. This file's rule is that evidence is discrimination and never origin. A measured key
  with no candidate list has therefore recorded nothing that it was discriminated against. Some
  are harmless (`time.TICK_MS` has no plausible rival). `pathfinding.PATH_GOAL_RULE` and
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
entry's history instead of being deleted. An argument that loses to a measurement belongs beside
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
is the live game, not a frozen build. Shipped data from 2016-2018 is evidence, not spec. Ground
movement was rewritten on 2025-03-31, so anything pre-2025 about movement is archaeology until
somebody re-measures it.

`tools/oracle_diff.py` diffs the engine against a trace tick by tick;
`crates/royalesim/tests/oracle2026.rs` gates the recorded first paths.

## Open keys and what would settle them

Everything below is at `guess`, `hypothesis`, `community` or `datamined`-but-unverified. The
engine runs on a placeholder for these, so the behaviour it produces is not evidence about the
real game. Each row names what a recording would have to show. The ledger is the authority; this
table is a reading guide over it, and a key's own `status` and `promotion_rules` win where the two
disagree. Of the 470 top-level keys with a status, 356 are `measured`; one more entry, nested inside another, is measured too (471 in all).

| Key | Value today | Status | Settled by |
|---|---|---|---|
| `collision.PUSH_MODEL` | `mass_weighted` | guess, LOW | a mass-ladder recording: units of known Mass pushing each other |
| `collision.BUILDING_FOOTPRINT_MODEL` | `collision_radius_circle` | guess, LOW | a walk past a building; note that circle and 2x2 box differ by 0.044 tile at best, so only 3x3-vs-not is separable |
| `collision.SEPARATION_ITERATIONS` | 1 | guess, LOW | a crowd recording with per-tick positions |
| `pathfinding.TIE_BREAK` | `ortho_first_placeholder` | guess, LOW | read only by the trace-fitted arm (`path2026.rs`); the selected arm reproduces the published node lists outright, so the key no longer gates it |
| `combat.DAMAGE_ARITHMETIC` | `integer` | guess, LOW | hit counts to kill a tower at known levels |
| `targeting.LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET` | 25 | datamined, LOW | vendor the modern data, or measure a target held past its range |
| `targeting.LOGIC_XPOS_BASED_TOWER_TARGETING` | true | datamined, LOW | a centre-column deploy: which tower it walks at |
| `match.LOGIC_BATTLE_START_COOLDOWN_MS` | 4500 | datamined, LOW | any recording of a match start |
| `arena.ARENA_SOURCE_VINTAGE` | ~2018 tilemap | datamined, MEDIUM | a calibrated screenshot of a live arena; bridge width varied by arena even in 2018 |
| `knockback` (5 of 13 keys) | the measured ladder, with its duration, water, stacking, zero-vector and deploying-unit edges unfixed | guess / hypothesis, LOW-MEDIUM | each key's `promotion_rules` names the capture it needs. `DISPLACEMENT_LAW`, `ATTACK_RESET`, `PUSH_LOAD_TIMER`, `DIRECTION_ROLLING`, `ROLLING_CONTACT_RADIUS`, `ATTACK_PUSHBACK` and `DEATH_PUSHBACK` are measured |
| `spells.*` (10 of 41 keys) | `AOE_HIT_TEST` and `SPAWNING_SPELL_WATER_RULE` are in `spell-spec.md`; the other eight are not, so read their ledger entries | guess / hypothesis / community | each key's own `promotion_rules` in the ledger names its deciding observation |
| `status.*` (13 of 26 keys: stun and buff timing) | see `spell-spec.md` | community / hypothesis / guess | likewise |
| `economy` (2 of 7 keys) | the elixir a death pays the opponent, the starting-hand rule | guess / community, LOW-MEDIUM | each key's `promotion_rules` names the 15.535.29 scenario it needs. `MANA_ON_DEATH_FOR_OPPONENT_UNIT` rests on the tables' pattern. The other five are measured: the Elixir Collector's payout at the cap, its overflow, its step in double elixir, its stun and the elixir its death pays its owner |
| `spawner.INTERVAL_START_ORIGIN` | `placement_counter_first_frame_counts` | hypothesis, MEDIUM | an interval spawner whose DeployTime is not StartCounterAt - 950: the tick of its first unit |
| `rng.GENERATOR` | `pcg32` | guess, LOW | not settleable, and not a goal. See `architecture.md`, Determinism |
| `enchant` (5 of 17 keys) | the Rune Giant's places, cooldown origin, stun at the pick, crown-tower bonus and arrival reach | hypothesis / guess, LOW-MEDIUM | each key's `promotion_rules` names the run it needs; the other 12 are measured on client 15.535.29 |
| `transform.HEALTH_TRIGGER_COMPARE` | `at_or_below` | guess, LOW | a hit that brings a Goblin Demolisher to exactly 650 of its 1300 hp |
| `parry.SAME_TICK_PICK` | `first_created_attacker` | guess, LOW | two melee hits landing on one tick on a ready Ronin (a swarm's first contact) |
| `parry.READY_AT` | `spawn` | guess, LOW | a melee hit on a Ronin within its first 20 ticks, while it deploys |

The keys that carry the measured 2026 movement and pathfinding model are at `measured`, most of
them at HIGH; each entry's `confidence` names the ones that are not. They are `time.TICK_MS`,
`time.SPEED_TO_SUBTILES_PER_TICK`, `time.PROJECTILE_SPEED_TO_SUBTILES_PER_TICK`,
`pathfinding.PATH_SEARCH`, `collision.CONTACT_LAW`, the `movement.*` section except
`BUFF_SPEED_COMPOSITION`, `SPAWN_PATHFIND_STATES`, `SPAWN_PATHFIND_START` and `JUMP_LANDING_CONTACT` (hypotheses), and the cost, goal and replan
keys. Their evidence is in `pathfinding.md` and `movement-measurements.md`.

These keys were measured later, on the 16.402 corpus or client 15.535.29, and are `measured` too:

| Key | What it settles |
|---|---|
| `match.TICK_ORDER` | attack updates before move updates, the move pass in creation order |
| `match.KING_ACTIVATE_TIME_MS` | the king's activation delay, 3550 ms |
| `combat.CROWN_TOWER_DAMAGE_ROUNDING` | how a crown tower's reduced share of a spell's damage rounds (`ceil_kept_share`) |
| `movement.JUMP_WATER_HOP` | a `JumpEnabled` troop's river hop |
| `movement.DYING_UNIT_VISIBILITY` | whether a dying neighbour is still an obstacle this tick |
| `combat.STAT_BASE_LEVEL`, `combat.TOWER_HITPOINT_LADDER` | level scaling and the crown-tower ladder |
| `combat.ATTACK_CYCLE`, `combat.PROJECTILE_LAUNCH`, `combat.KAMIKAZE_DEATH` | the attack cycle, the launch point, the kamikaze death |
| `lifetime.HP_DECAY` | a building's hit-point drain over its lifetime |
| `formation.LAYOUT`, `DEPLOY_STAGGER`, `GROUND_Y_CLAMP` | where a card's summons stand, and when each appears |
| `spawner` (34 of 41 keys) | emission timing, the first wave, the start-time origin, the two deploy-time defaults, the death-spawn layout, an emission's water turn, and more. The Goblin Hut's wake reach, wake targets and spawn speed (`LIFE_STATE_WAKE_REACH`, `LIFE_STATE_WAKE_TARGETS`, `ACTION_SPAWNER_SPAWN_SPEED`) were measured on client 15.535.29 only |
| `knockback.DISPLACEMENT_LAW`, `ATTACK_RESET` | the push ladder and what a landed push does to the attack |
| `charge.CHARGE_RANGE_UNIT`, `CHARGED_HIT_TIMING` | the run-up's unit and when the charged hit lands |

The `hide.*` section is mostly community (5 of its 7 keys). Half of the `status.*` section is
community, hypothesis and guess (13 of its 26 keys). `hide.RISE_LAW`,
`hide.TARGETABLE_WHILE_RISING`, `status.ATTRACT_LAW`, `status.ATTRACT_WHILE_HELD`,
`status.FULL_STOP_BUFF_IS_STUN`, `status.BUFF_PULSE_AMOUNT`, `status.AREA_BUFF_SOURCE_BINDING`,
`status.APPLY_BUFF_BEFORE_DAMAGE`, `status.BUFF_DEATH_SPAWN_DEPLOY_TIME`,
`status.CROWN_TOWER_DAMAGE_PER_HIT_SCALING`, `status.DAMAGE_REDUCTION`, `status.IDLE_BUFF`,
`status.STUN_CLEARS_TARGET`, `status.RESUME_RETARGET_WINDUP` and `status.WAITED_PRESS_CAST` are measured.
`status.DAMAGE_REDUCTION` is measured at reductions of 100 and 65 on client 15.535.29 and at 15 and 60
on 16.402.
Each open key carries the observation that would settle it.
