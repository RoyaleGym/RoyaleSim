# Pathfinder and movement spec

This page is for contributors implementing the 2026 ground pathfinder and the movement laws
around it. It is the implementation contract, derived entirely from the offline traces of client
15.535.29. `movement-measurements.md` holds the evidence, the corpus and the provenance for every
rule here. Read it first if you want to know *why* a rule is what it is.

**Scope.** This spec describes the trace-fitted model (`path2026.rs`, ledger arm
`trace_fitted_astar`). The arm the engine selects today is the search measured directly on client
16.402. It supersedes sections 3, 4, 6 and 8 below, and `pathfinding.md` describes it. The rest of
this file (the grid, the units, path layout and ownership, the per-tick update and the deploy
rules) still describes what the engine does, and the two models agree on it.

Every rule below carries the measurement that pins it. Rules marked **UNVERIFIED** are choices
the traces do not constrain: implement them behind a flag and do not let a fitted card constant
absorb them later.

Constants in `«double angle brackets»` come from `data/calibration.json` and must be read from
there, never hardcoded. Section 10 maps the rules onto the ledger keys that carry them.

---

## 1. Units and representation

| Quantity | Value | Evidence |
|---|---|---|
| Native arena unit | 1 millitile; 1000 per tile | Oracle header: king (9000, 3000), princess (3500, 6500); probe grids declare 18 × 32 tiles at cell size 1000 |
| Arena | x ∈ [0, 18000), y ∈ [0, 32000) | same |
| Engine position | `i32` subtiles, `«representation.SUBTILE_PER_TILE»` = 18000 per tile | 1 native unit = 18 subtiles exactly; every measured speed is an integer number of subtiles |
| Tick | `«time.TICK_MS»` = 50 ms | Free-sub-speed fit: no integer speed survives at 2–5 sub-steps per tick |
| Squared distances | **`i64`** | Reaches 3.3 × 10¹¹ in subtiles. An `i32` squared distance is a silent overflow |

Side 1 is the exact 180° rotation of side 0. **No per-side sign handling is needed
anywhere**: the heading, step, stomp and path-node laws all hold unchanged on side-1 units
(`meet/` traces: HogRider side 1 heading 108/108, Giant side 1 307/308, stomp 328/328).
`path_nodes` are absolute arena cells, not per-side (side-1 decode error 118.4 units
against 10 238 for a rotated decode).

---

## 2. The pathfinding grid

**Rule 2.1.** The grid is **36 columns × 64 rows** of half-tile cells, **500 native units**
(9000 subtiles) on a side. Load it from
`data/derived/arena.json`, which the tracked data generates; it agrees with the 2026 tilemap
(`data/raw/cr-15.535.29/tilemaps/tilemap.csv`, not distributed) on every geometry bit.

```
col = x / 500                 row = y / 500
node_index = row * 36 + col
cell_centre = (col*500 + 250, row*500 + 250)
```

*Evidence*, counted over the offline traces of client 15.535.29 (`movement-measurements.md`
holds the corpus): `path_segment_direction == norm256(cell_centre(tail) − pos)` on 3781/3781
segment-assignment ticks at exactly `(width 36, +250, +250)`. A 10-unit offset shift drops
it to 2–3 % (x) or 74 % (y); widths 18/32/34/35/37/38/64/72 all score ≤ 0.08 %.

**Rule 2.2.** Cell terrain comes from the tilemap bit flags:

| Bits | Meaning | Treatment |
|---|---|---|
| 1 or 2 | lane / road | cost `«pathfinding.PATHFINDING_COSTS.road»` = 5 |
| 32 | water | **impassable to ground units** |
| 16 | arena edge + king-tower block | **impassable** |
| 128, 256, 512 | 2026-only markers (8, 2 and 2 cells) | ignore them. Adding them to the road set changes nothing |
| otherwise | plain | cost `«…default»` = 8 |

*Evidence:* histogram `{0: 1386, 1: 309, 2: 309, 16: 104, 17: 36, 18: 36, 32: 112,
128: 8, 257: 1, 258: 1, 512: 2}`. Alignment is forced by the bit-16 block spanning
cols 15–20 × rows 3–8, centred on the side-0 king at (9000, 3000).

**Rule 2.3.** Do **not** implement `PATHFINDING_WATER_COST = 7` as a traversable cost for
ground units. It is in the shipped globals but it is wrong for ground: modelling it makes
25 of 150 oracle first-paths strictly dearer than the optimum. The oracle refused water
shortcuts it would have taken. Water at cost 50 and a hard block are indistinguishable;
pick the hard block for ground and leave cost 7 available for whatever it is really for.

---

## 3. The search

**Rule 3.1. Connectivity.** 8-connected, no corner-cutting restriction.
*Evidence:* 4-connected reproduces 44/150; the oracle's own paths contain diagonals; over
~486 000 adjacent node pairs, zero are non-8-neighbours. Corner-cut strict fails 6 of 140.

**Rule 3.2. Step cost.** The cost of a step is the cost of the cell **entered**, times
√2 for a diagonal:

```
step_cost = cell_cost                 orthogonal
step_cost = cell_cost * 1414 / 1000   diagonal   (5 -> 7, 8 -> 11, 50 -> 70)
```

*Evidence:* multiplier sweep over 150 first-paths: 1.0 → 3 suboptimal, 1.2 → 2,
**1.4142 → 0**, 1.5 → 1, 2.0 → 96. Exact-rational zero-failure window [1.38, 1.48].
Witnesses for the uniform diagonal being wrong: `lane_sweep_Knight/c00_r06` (173 vs 171),
`c08_r04` (197 vs 190), `c10_r06` (181 vs 174).

**This is load-bearing for the cost model as a whole.** With a uniform diagonal,
`road 5 / plain 8` and a flat cost 8 classify every path identically: the road discount
becomes unidentifiable. Fix the diagonal first, then flat-8 fails 96 paths. Use exact or
integer arithmetic; float comparison at 1e-6 produces spurious failures.

**Rule 3.3. Heuristic.** `«pathfinding.PATHFINDING_DEFAULTHEURISTIC_COST»` = 5 per cell.
The form consistent with the √2 step cost is octile:

```
h = 5 * (max(|dc|,|dr|) - min(|dc|,|dr|)) + 7 * min(|dc|,|dr|)     // 5*sqrt(2) ~ 7.07
```

**UNVERIFIED.** `PATHFINDING_HEURISTIC_METHOD = 1` is datamined but its meaning is not.
The heuristic does not change *which* paths are optimal (the cost model was validated with
Dijkstra), only the expansion order. That is exactly the thing we cannot yet reproduce
(rule 3.6). Keep it swappable.

**Rule 3.4. Open/closed policy.** `REFRESH_OPENNODES = TRUE`,
`REOPEN_CLOSEDNODES = FALSE`. Datamined, not measured.

**Rule 3.5. Admissibility trap.** If the goal is expressed as "any cell within reach of
the target" (rule 6), the heuristic must point at the *goal set*, not at the target cell.
An h computed to the target cell over-estimates the remaining cost to the actual goal by
up to `reach/500 × 5`. The first in-reach cell popped is then not the cheapest one.
Either subtract the reach from h, or take h as a min over the in-reach cells. One of the
analysis scripts hit exactly this bug, and it manufactured some of its goal-cell mismatches.

**Rule 3.6. Tie-breaking. UNVERIFIED, and known to be insufficient.** Where several
successors are equally optimal, prefer the orthogonal one (1568 of 1672 ambiguous steps,
93.8 %, over the offline traces of client 15.535.29). **Treat this as a placeholder**: an `ortho_first` neighbour order still reproduces
only 22/140 exact node sequences. Exact ties are rare with the √2 diagonal, so the
residual divergence is not a classic tie-break at all. Do not write a tie-break rule into
`calibration.json` as settled. Keep the open list behind a trait so the discipline can be
swapped (binary heap, index-ordered array, g-ordered, h-then-index have all been tried;
none exceeds 13/76 distinct experiments).

---

## 4. Occlusion

**Rule 4.1. Shape.** A building blocks every half-tile cell overlapping the
**axis-aligned, half-open** box `[cx − R, cx + R) × [cy − R, cy + R)`:

```
blocked_cols = (cx - R) / 500  ..=  (cx + R - 1) / 500
blocked_rows = (cy - R) / 500  ..=  (cy + R - 1) / 500
```

Per-axis (Chebyshev), **never Euclidean**. Use floor division on both edges.

*Evidence:* over 3776 published path lists, per-axis square 0 suboptimal / 0 infeasible;
circle-overlap at R 45/216; circle-overlap at R+250 27/246; cell-centre-within-Euclid 23/0.
A **closed** box blocks 57 cells the oracle's own paths use. The discriminator is the
tower, not the Cannon, because 1000 and 1400 are exact multiples of the cell size while
none of `600 ± x` is.

**Rule 4.2. Radius.** `R` = the building's `CollisionRadius`, read from the card data
(`csv_logic/characters/*.toml` `[BUILDING.*]`, `csv_logic/buildings.csv` column 91).
Cannon 600, PrincessTower 1000, KingTower 1400.

*Evidence and its limits:* the traces bracket the Cannon to (500, 1000] and the princess
tower to (500, 1000]. The 500-unit cell quantum means every R in that range blocks the same
cells for a building on a tile centre, so **the data does not show the box scales with
`CollisionRadius`**. It shows that one unfitted rule at three different shipped radii
reproduces the three observed footprints. **The king tower's footprint is entirely
unmeasured**: `R_king = 0` scores identically to 1400 (0/0), and only R ≥ 2000 fails. No
trace in the corpus goes near a king.

**Rule 4.3. No mover radius, no clearance pad. The term is exactly zero.**
*Evidence:* a pad of even **1 native unit** blocks 155 cells the oracle's own paths use,
because `3500 − 1000 = 2500` lands exactly on a cell boundary and the Giant's control path
runs up column 4 at rows 11–16. Cannon 600 + Giant 750 = 1350 would block the very column
the `cannon_dx+0.0` detour takes.

**Rule 4.4. The deployer's own crown towers occlude.** Without any occluders, 12 of 150
first-paths are strictly dearer than the optimum. Do **not** reuse the tilemap `NO_DEPLOY`
(bit 16) block as the occlusion set: it covers only the two king 3×3 boxes and the river
banks, and the princess towers carry **zero** `NO_DEPLOY` cells.

**Rule 4.5. Enemy occlusion. UNVERIFIED.** The globals say
`PATHFINDING_FRIENDLYONLY_OCCLUSIONS = TRUE`. The traces cannot tell: adding the enemy
towers as occluders scores identically to friendly-only (0/0/0), because no interior cell
of any first path lies inside an enemy tower box. Follow the globals; flag it.

**Rule 4.6. The goal cell is exempt from occlusion.** Refuse occluded cells during
expansion but allow the goal cell itself.
*Evidence:* 4 of 150 first-path goal cells lie inside a tower box: (6, 49) for MiniPekka
and Skeletons, (7, 49) for Skeletons. Across all published lists 81 paths end inside
an occlusion box, always as the final cell, never as an interior one. Without the
exemption the short-reach cards get no path at all.

**Rule 4.7. Occluded cells: block or cost 50? UNVERIFIED.**
`PATHFINDING_BUILDING_COST = 50` and a hard block are indistinguishable in this corpus.
No oracle path ever needed to cross a building. Same for bit-16 terrain. Either reproduces
every trace. Pick one, flag it, and settle it with a corridor where crossing is cheaper
than going around.

---

## 5. Path ownership and layout

**Rule 5.1.** Store the path as a `Vec` of half-tile cell indices, **goal-first**, and pop
from the **back** (`Vec::pop`). This matches the oracle's own layout, which keeps a
byte-level trace diff trivial.
*Evidence:* over 29 996 live-path ticks the distance from the unit to the last element is
never below 737.9 (median 1256.2). A path rebuilt from the unit's cell each tick would put
the last element within ~353 units.

**Rule 5.2.** Return the path **without the start cell**. Do *not* implement "drop the
first two cells": the observed Chebyshev-2 gap is a consequence of rule 7.5 firing once
after the first move. It is Chebyshev-1 for both `repath_Giant` first paths, which a
literal drop-2 rule gets wrong.

**Rule 5.3.** Clear the path (length 0) when the unit transitions to attacking; do not
require it to reach the goal node. The oracle abandons the goal 1046.7–1436.1 units from
the goal cell centre, on the tick `behavior_state` becomes 2. The census of
`(behavior_state, has_path, has_target)` has exactly four cells over 46 304 ticks and not
one tick anywhere has `behavior_state == 2` with a live path. The path clear and the state
change are the same event.

**Rule 5.4.** Never build a path while `target` is `None` (0 ticks in 46 304 with a
non-empty path and a null target). Note the path may be created **more than once**: in
`meet/Knight_vs_HogRider` the Knight's path is emptied and re-created four times while
holding a target throughout, as it drops in and out of attacking.

---

## 6. The goal cell

**Rule 6.1.** The path is truncated at the **first cell whose centre is within
`Range + CollisionRadius` of the target's centre point**: the target's centre, not its
footprint, and not the sum of both collision radii.

*Evidence:* verified two-sided (goal in reach *and* its predecessor out of reach) on
**150/150** first-paths, including the `meet/` traces with moving targets and side-1 units.
Rivals: "Range only" 1/140, "Range + both radii" 0/140, "footprint edge" 0/140. The
per-card stop cells from one deploy tile: Giant (reach 1950) row 47; Knight 1700, Golem
1500, HogRider 1400 all row 48; MiniPekka 1250 and Skeletons 1000 row 49.

**Caveat, and it matters.** A constant `k` added to `Range + CollisionRadius` fits all 150
paths for any `k ∈ [−125.2, +24.8)`. Zero is inside that window; so are other values. The
reach is pinned to roughly ±75 units, not exactly.

**Rule 6.2. The rule is necessary, not determinative.** It is a condition on the last node,
not a predictor of which cell the search stops at. Between 12 and 52 cells per sample
satisfy it (median 32). The oracle's goal cell is not the cheapest reachable in-reach
cell in 114 of 140 samples, with no cost ties among them. Handing an A* the oracle's goal
cell raises exact-sequence reproduction from 22/140 to 43/140. **The goal cell is an output
of the expansion order.** Do not implement "pick the nearest or cheapest in-reach cell".
Implement the reach test as the goal predicate and let the search's pop order choose.

**Rule 6.3. Sign.** The standoff is on the *approach* side, not unconditionally negative
in y. In `meet/Knight_vs_HogRider` ticks 250–273 the Knight's target is below it and the
goal row is `cell_centre_y + reach`. An unconditional minus puts the goal on the far side
of the target.

**Rule 6.4. The column clamp.** The goal column is the unit's own column clamped into the
target's collision box, `target_x ± CollisionRadius(target)`. Read the radius from the
target's card data (PrincessTower 1000, KingTower 1400). Do **not** hardcode 1000.
**Partly unverified:** the traces bracket the half-width only to (500, 1000]; 750, 900 and
1000 score identically at 98.96 % per tick. The row half of the rule rests on six data
points, one per card, all from one start and one approach direction. Across all 139
recomputes the goal row never moves.

**Rule 6.5. Re-derive the goal cell every tick** and recompute the path only when the
resulting *cell* changes. That reproduces the 137 observed goal-driven replans with the
observed 1–2 tick lag.

---

## 7. The per-tick update

Order matters. This sequence is what reproduces the traces; permuting it does not.

```
for each mobile unit, each tick:
  1. if deploying (deploy timer not expired) -> tick the timer, do nothing else
  2. re-derive the goal cell from the current target          (rule 6.1, 6.4)
  3. if the goal cell changed, or the occluder set changed since last plan:
         replan the whole route                               (rule 8)
  4. if the stomp schedule marks this tick as a pause -> skip 5-6 (locomotion only)
  5. heading  := norm256(centre(path.last()) - position)      // PRE-move position
  6. position += (trunc(S * heading.x / 256), trunc(S * heading.y / 256))
  7. if !path.is_empty() and dist(position, centre(path.last())) <= 1000:
         path.pop()                                           // at most ONE per tick
```

**Rule 7.1. Heading from the pre-move position.**

```
d   = centre(path.last()) - position        // i64 components
L   = isqrt(d.x*d.x + d.y*d.y)              // integer sqrt, FLOORED
dir = (d.x * 256 / L, d.y * 256 / L)        // integer divide, truncate toward zero
```

*Evidence:* 100 % on every isolated unit. Post-move as the origin scores 85–264 per card
against 106–307. The **floored length** is what is load-bearing, not the sqrt routine:
`floor(sqrt)` and `isqrt` are identical; dividing by the unfloored length matches 14–42
ticks per card; `ceil(sqrt)` 14–41. Over 1258 walk ticks: floored 1258, nearest-integer
1179, scale-then-sqrt 191, ceil 173.

`|dir|` legitimately ranges 254.678..256.236 because each axis truncates independently.
**Do not renormalise or clamp to 256.** If you assert a magnitude band for trace diffing,
257 never appears and 254 does.

Heading is **stateless**, a pure function of position and current node, recomputed from
scratch. No inertia, no angular smoothing, no turn-rate limit. It snaps 71.57° in one tick
on a spawn tick and nothing in the corpus exceeds that. (`RotateAngleSpeed` exists as a
card column but only turret-like cards set it: cannon_cart, zapmachine, minizapmachine,
dartbarrell. None of those are troops.)

**Rule 7.2. The step.** Per axis, independently:

```
position.x += trunc_toward_zero(S * dir.x / 256)
position.y += trunc_toward_zero(S * dir.y / 256)
```

The sub-unit remainder is **discarded every tick**. There is no fractional accumulator.
Integrating this alone from the first moving tick reproduces the final position exactly
over 106–308 ticks with zero drift.

In Rust, `i32 / i32` already truncates toward zero and is correct. **Do not use `f32` and
do not use `div_euclid`.** Floor matches 0 of ~1000 discriminating ticks (`dir.x < 0` on
every moving tick of every walk unit, so floor and truncation differ on every one);
round matches 17–43 per card; ceil 201/240.

**Rule 7.3. `S`, the per-tick speed.**

```
S = Speed                                                        if StopMovementAfterMS is unset
S = Speed * (StopMovementAfterMS + WaitMS) / StopMovementAfterMS  otherwise   (integer, floor)
```

with `Speed` read from the TOML overlay (`csv_logic/characters/*.toml`; the
`characters.csv` rows are blank in all 335 columns except `Name` for every card here).
In subtiles: `subtiles_per_tick = Speed * «time.SPEED_TO_SUBTILES_PER_TICK»` = `Speed * 18`.

*Evidence:* free integer fit, no card table, 1..5000. Exactly one survivor per card, and
every unit of a card fits the same value: Knight 60 (131 units), Giant 52 (12), Golem 54,
Skeletons 90 (3), MiniPekka 90, HogRider 120.

**Partly unverified.** The stomp formula is a two-point fit. `Speed + floor(Speed·Wait/Stop)`
agrees on every shipped stomp card, and the rounding mode is undetermined (floor, round and
truncate all give Giant 52 / Golem 54; only ceil is excluded). Harmless for the engine, but
do not record it as uniquely identified.

Do **not** apply `WalkingSpeedTweakPercentage` to movement. It is animation only (the
Golem carries both a 15 % tweak and a stop/wait, and 45 × 1.15 = 51.75 ≠ 54).
`S` is the **unbuffed base**. `csv_logic` is full of `SpeedMultiplier` keys, so the engine
needs a multiplier entry point. Its rounding is completely unmeasured.

**Rule 7.4. The stomp pause schedule.** Keep the unit's moving-tick index `k` (`k = 0` on
its first moving tick, **never reset**) and skip the position update iff

```
((k + 1) * TICK_MS) % (StopMovementAfterMS + WaitMS) > StopMovementAfterMS
```

*Evidence:* Giant 308/308, Golem 306/306; strict `>` beats `>=` (304/308, 294/306);
`(k+1)` beats `k` (268/308, 282/306); every phase offset brute-forced, only 0 fits. A naive
"move 640 ms then wait 100 ms" countdown gives 13 moving ticks in the Giant's first block
and the oracle shows 12.

**The pause gates the unit's own locomotion, not the position write.** External
displacement still applies during a freeze. A jostled Giant moves on 2 of 323 scheduled
pause ticks.

**Rule 7.5. Node consumption.** After the move, if the path is non-empty and the
Euclidean distance from the **post-move** position to the last node's cell centre is
`<= 1000` native units (1 tile, 2 cells), pop it. **At most one node per tick.**

*Evidence:* 0 of 3851 tail-drop ticks dropped more than one. The predicate has **zero false
positives across 27 824 keep-ticks** and misses 148 of 3731 drops (4.0 %) by at most 74
units. Chebyshev 1988 false drops, Manhattan 2525 missed, pre-move-minus-speed 216 total.

**Do not implement the counter model** (`remaining = floor(|d|)` decremented by the speed):
it gets the wrong pop tick on 518 of 3550 segments with a systematic error signature
{−2: 148, −1: 102, +1: 102, +6: 91, +7: 15}, against 271 for live Euclid.

**Do not tune the threshold to hide the residual.** 1001 buys 27 errors and costs 3 false
drops; 1003 gives 107 total against 148; 1005 and beyond are worse. Expect ~4 % of pop
ticks to diverge until the avoidance and separation laws are modelled.

**Rule 7.6. Corners are cut for free.** Because the unit turns a full tile before each
node, closest approach to a corner node averages 152.9 units and reaches 395.5. That is a
consequence of rules 7.1 and 7.5, not a separate rule. An engine that drives to node
centres before turning diverges at every bend.

**Rule 7.7. The segment direction field.** If the trace-diff harness wants it, keep it
separate from the live heading: it is `norm256(next_node_centre − POST-move position)`,
frozen at consumption and held for the whole segment. Zero exceptions on the walk traces.

---

## 8. Replanning

**Rule 8.1.** There is **no periodic replan timer**. Set
`«pathfinding.REPATH_INTERVAL_TICKS»` to event-driven. 150 structural recomputes over
31 859 path ticks (0.47 per 100), evenly spread across families; gaps within a trace are
{17, 24, 29, 100, 185, 190, 228, 248, 250} with no common period. A 30-tick timer would
give about 10 changes per trace.

**Rule 8.2. Trigger 1: the goal cell moves.** Re-derive the goal cell each tick (rule 6.5)
and replan when it changes. *Evidence:* 137/137 predicted goal-column flips are followed by
a replan, with **0** predicted flips producing none and **0** unexplained replans. Lag
{1 tick: 128, 2 ticks: 9}. The trigger fires for a *moving target* the same way. A Knight
chasing a HogRider replans every 3–5 ticks, all goal-cell moves.

**Rule 8.3. Trigger 2: a friendly building enters the world.** Replan **on the tick the
entity first exists**: 0 ticks of lag, +1 from the accepted deploy command. Do not wait a
tick. *Evidence:* command at 160, entity first in a frame at 161, path already replanned in
that same frame, in both `repath_Giant` traces, which produce opposite-sign detours from
the same state.

**Rule 8.4.** Replan the **entire remaining route**, not a spliced local bypass. The detour
appears in the first path the unit ever publishes, ten rows (5 tiles) ahead, and rows far
beyond the obstacle legitimately shift.

**Rule 8.5.** Do **not** hardcode a detour handedness. The side falls out of the cost.
*Caveat:* only 2 of the 5 Cannon offsets involve a detour at all, and in both the
cheaper-side and least-lateral-deviation hypotheses agree, so only the negative claim (not
a fixed handedness) is established.

**Rule 8.6.** Friendly **troops** do not enter the path grid. A Giant walking directly
behind a friendly Knight has 2 structural path changes in 316 path ticks. Troops are
handled by the avoidance term (section 9).

**Rule 8.7. What we cannot see.** A recompute returning an identical list is invisible in
the traces: 8 of 94 tail-surviving recomputes leave both `path_segment_direction` frozen
and `path_node_consumed` clear. `PATHFINDING_SAMEPATH_EPSILON = 3` may be suppressing
near-identical results. "No periodic replan" bounds only *observable* path changes.

---

## 9. Deploy, and the limits of this spec

**Rule 9.1. Deploy.** Spawn the entity on the tick **after** the command is accepted. Hold
it immobile for `DeployTime / «time.TICK_MS»` ticks. Acquire the target, build the first
path and take the first **full-length** step all on tick
`spawn_tick + DeployTime / TICK_MS`. Flip the movement state one tick early (the oracle's
`behavior_state` goes 4 → 1 before the first displacement). `LoadTime` gates the first
attack, not the first move. Never scale the first step.

Anchor on the **spawn** tick, not the command tick: the Golem's command waits on elixir
(accepted 113, spawned 114, moves 174), so a deploy-tick formula fits five cards and is 14
ticks wrong on the sixth.

**Rule 9.2. The domain of everything above.** These laws hold for a unit that is **not in
contact with another unit**. Three regimes break them and **two set no flag at all**:

| Regime | `behavior_state` | Flags set | Signature |
|---|---|---|---|
| Avoidance | 1 | `avoidance_offset != 0` | heading deflected; \|step\| within ±2.3 % of `S` |
| Crowd separation | 1 | **none** | \|step\| ≈ `S` plus a 15–28 unit lateral residual ⟂ to the heading |
| Combat pushback | 2 | **none** | displacement 0.06–2.5 × `S`, can oppose the heading |

(174 flagless failures corpus-wide: 137 at state 1, 36 at state 2, 1 at state 4.)

**No field in the corpus records the contact impulse.** The push must be inferred from the
residual. The engine must keep these separate from the locomotion law (rule 7.4) so that a
pushed unit still moves during a stomp pause.

**Rule 9.3. `avoidance_offset`. UNVERIFIED, do not guess.** It is set to ±190 in one tick
and then performs a **±10-per-tick walk bounded at ±190**: 16 of 36 runs ramp back up
before coming down, and runs reach 67 ticks against the 19 a pure decay implies. Do **not**
implement "decremented by 10 per tick toward 0". What sets it, what selects the sign, and
what deflection it applies (the ratio of heading error to offset is 0.327–0.387,
non-linear) are all unmeasured.

**Rule 9.4. `path_node_consumed` semantics**, if the engine mirrors it for diffing:

> `1` iff there is **no segment currently being walked**: the path is empty, **or** the
> segment was (re)assigned this tick.

Not a one-tick pulse: it runs high for up to 257 consecutive ticks (4004 runs, 373 longer
than one tick). Census over 150 units: `(no path, 1)` 17 624, `(path, 1)` 4031,
`(path, 0)` 27 828, `(no path, 0)` **0**.

**Rule 9.5. `x2 / y2`** is exactly the previous tick's `(x, y)`: 49 333 / 49 333, no
exceptions. Free per-tick anchor for a trace-diff harness.

**Rule 9.6. The recorded `path_nodes` is a post-tick snapshot.** A repath can insert one
or two nodes below the old waypoint that the unit aims at and consumes **within the same
tick**, so they appear in no recorded frame. 142 ticks in the corpus show this; all carry
`path_node_consumed == 1` and 140 are explained exactly by a cell 1–2 rows nearer in the
same column. **Any pathfinder fitted to the recorded node lists is missing those
insertions**. That is part of why rule 3.6 is still open.

---

## 10. The ledger keys this model carries

Every rule above reaches the engine through `data/calibration.json`. The table maps each one onto
its key and records what the measurement did to it. All of these are applied and at `measured` as
of 2026-09-21. The ledger's promotion rules apply as always: a constant is promoted only by adding
evidence, and a `measured` value cannot be overwritten with a different value without
`--supersede`.

| Key | Before | After | What changed |
|---|---|---|---|
| `time.TICK_MS` | 50, hypothesis, LOW | 50, **measured, HIGH** | value unchanged, status promoted |
| `time.SPEED_TO_SUBTILES_PER_TICK` | 15, hypothesis, LOW | **18**, measured, HIGH | value **changed**; resolves `divide_by_50` vs `divide_by_60` |
| `pathfinding.ALGORITHM` | `lane_flow_with_local_avoidance`, guess | `weighted_grid_astar`, measured, HIGH | three of four candidates refuted |
| `pathfinding.REPATH_INTERVAL_TICKS` | 10, guess | `null` / event-driven, measured, HIGH | folklore constant removed |
| `pathfinding.PATHFINDING_COSTS` | datamined | datamined + **application measured** | adds the √2 diagonal, the per-cell-entry rule, and water-impassable-for-ground |
| `representation.SUBTILE_PER_TILE` | 18000, hypothesis, high | 18000, hypothesis, high | value and status unchanged; rationale confirmed |
| `pathfinding.CELL_SIZE_NATIVE` | not present | **new**, 500, measured, HIGH | 36 × 64 half-tile grid |
| `pathfinding.WAYPOINT_ARRIVE_RADIUS` | not present | **new**, 1000, measured, MEDIUM | one-sided exact; 4 % fire late |
| `pathfinding.PATH_GOAL_RULE` | not present | **new**, measured, MEDIUM | reach truncation; necessary not determinative |
| `pathfinding.OCCLUSION_MODEL` | not present | **new**, measured, HIGH | half-open AABB at `CollisionRadius`, no mover pad, goal exempt |
| `pathfinding.PATH_NODE_ENCODING` | not present | **new**, measured, HIGH | `row*36 + col`, centre `+250/+250` |
| `movement.POSITION_ROUNDING` | not present | **new**, measured, HIGH | per-axis truncate toward zero, remainder discarded |
| `movement.HEADING_LAW` | not present | **new**, measured, HIGH | `d*256 / floor(sqrt(|d|²))`, pre-move origin |
| `movement.STOMP_SPEED_RULE` | not present | **new**, measured, MEDIUM | rounding mode undetermined |
| `movement.STOMP_PAUSE_SCHEDULE` | not present | **new**, measured, HIGH | phase uniquely pinned |
| `movement.DEPLOY_TIMING` | not present | **new**, measured, HIGH | spawn-anchored |
| `movement.CONTACT_DOMAIN` | not present | **new**, measured, HIGH | scopes every movement law |
| `collision.PUSH_MODEL` | `mass_weighted`, guess | unchanged | add a note: the fields are zero corpus-wide; fit from the residual |

`time.PROJECTILE_SPEED_TO_SUBTILES_PER_TICK` is untouched by this corpus: no projectile flies in
these traces.

Two readings that did **not** survive the measurement, recorded so that nobody re-proposes them:

1. `time.TICK_MS.disagreement.60_TPS_16.67ms` should be **struck**. Its argument
   ("PhoenixNoRespawn DeployTime 733 ms is 44 ticks at 60 Hz") is arithmetically wrong:
   733 / 16.667 = 43.98, and the argument needs 733.33. Lightning `HitSpeed = 460` is a
   whole number of ticks at none of 50, 33.3 or 16.67 ms, so "shipped durations are whole
   ticks" is void in either direction.
2. `pathfinding.PATHFINDING_COSTS.action` asks four questions: per half-tile cell?, which
   cells are road?, is matching-road the unit's own lane?, does ground A* cross water at 7?
   Three are now answered (yes, per cell entered; road = tilemap lane bits 1 or 2; no,
   water is impassable to ground). The fourth is not: `ROAD_COST` and `MATCHINGROAD_COST`
   are both 5 in the live globals, so no trace can separate them.

---

## 11. Verification gates

Wire these as tests. In increasing strictness:

1. **Cost model.** All 150 oracle first-paths are exactly cost-minimal on the engine's own
   grid, with the engine's own occluders. This is the gate that catches a wrong cost
   constant, a wrong diagonal weight or a wrong occlusion box. The 8-run version that
   earlier work used passes under models that this one rejects.
2. **Movement.** The laws reproduce every *isolated* unit bit-exactly from its first moving
   tick: Knight 239, Giant 307, Golem 305, MiniPekka 164, HogRider 121, Skeletons #0 106
   ticks; zero drift; identical final positions.
3. **Consumption.** The pop predicate never fires early across the 27 824 keep-ticks.
4. **Goal.** Truncation holds two-sided on all 150 first-paths.
5. **Invariants.** Published paths are 8-connected with no non-adjacent step, and weakly
   monotone toward the goal: no path contains both a forward and a backward row step.
   (They are *not* column-monotone: 1339 purely horizontal steps occur.)
6. **Exact node sequences.** Out of reach for this model. Its ceiling is 13 of 76 distinct
   experiments, because rule 3.6 (the expansion order) is unsolved under it. It *is* gated on
   the arm measured on client 16.402, by G6 in `tests/oracle2026.rs`. See `pathfinding.md`.

Corpus note for the harness: enumerate `(side, generation_key)` pairs, do not follow one
unit per trace, and include `data/oracle-native/meet/`. The lane-sweep
directory is 128 files but only **64 distinct experiments**. The deploy command is clamped
to the deployer's own half, so every `cNN_rRR` with `RR >= 14` lands at y = 14500.
