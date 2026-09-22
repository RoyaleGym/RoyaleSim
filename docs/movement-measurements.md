# Movement and pathfinding: the offline measurements

How the 2026 Clash Royale engine moves ground troops and plans their paths, measured on recorded
traces of the real client (Clash Royale 15.535.29). This is the primary source behind the
`pathfinding.*`, `time.*` and `movement.*` entries of `data/calibration.json`, and behind
`pathfinder-spec.md`, which turns these measurements into an implementation contract.

The traces carry one frame per 50 ms tick of a real battle. No number below comes from a
community engine or from an argument. Where a claim is *not* measured — and several of the ones
that get repeated loudest are not — it says so.

The model the engine selects today was measured a client version later, on 16.402, and is in
`pathfinding.md`. It supersedes the search and the contact law described here; the frame, the
units, the movement laws and the path representation are unchanged between the two.

## 1. Provenance

| Item | Value |
|---|---|
| Client | Clash Royale 15.535.29 |
| Corpus | recorded traces of client 15.535.29, one frame per tick, under `data/oracle-native/` (not distributed) |
| Step | 0.05 s per tick, one recorded frame per tick |

The traces carry one frame per 50 ms tick and no wall-clock time. Every experiment starts a
fresh battle and deploys after tick 100 (the game rejects a deploy before roughly tick 100).

## 2. The corpus

145 traces under `data/oracle-native/`, plus one orientation trace. Counting one unit per trace
undercounts; enumerating **every `(side, generation_key)` pair** yields **150 units**, and the
difference matters, because the extra units are exactly the ones that break the tidy laws.

| Directory | Files | Units | What |
|---|---:|---:|---|
| `walk/` | 6 | 8 | One card each at own-tile (3.5, 8.5), empty board, 500 ticks. `Skeletons` holds **three** units, not one |
| `lane_sweep_Knight/` | 128 | 128 | A Knight at the centre of every second legal tile, 320 ticks |
| `building_Giant/` | 6 | 6 | Giant at (3.5, 3.5) with a friendly Cannon at (3.5+dx, 9.5), dx ∈ {−2,−1,0,+1,+2} plus control |
| `repath_Giant/` | 2 | 2 | Giant at (3.5, 5.5); friendly Cannon dropped at (3.5+dx, 11.5) after 60 ticks |
| `meet/` | 3 | 6 | Two mobile units. The only side-1 units and the only moving targets in the corpus |
| `orient/` | 1 | — | Tower layout and deploy-legality probe grids |

**The deploy command is clamped to the deployer's own half**, so the 128 lane-sweep files
contain only **64 distinct experiments**: every `cNN_rRR` with `RR >= 14` lands at
y = 14500, and nine files share the start (2499, 14500). Duplicates produce byte-identical
paths, so no conclusion changes, but any "128/128" or "140/140" figure is roughly twice
its true sample size. Figures below are stated over units or over first-paths, not files.

## 3. How it was measured

Four independent analyses — timing and the speed law; node encoding and consumption; occlusion
geometry and cost; the full A* fit — were each re-measured from scratch by an independently
written implementation, with the loaders, decoders, integer arithmetic and Dijkstra rewritten so
that a shared bug could not hide in all four. The analysis is read-only and reads this repo's
`data/oracle-native/` and `data/raw/` (`ROYALESIM_DATA_DIR` overrides).

Every number in sections 4-7 below comes from that final re-measurement unless it is attributed
to one of the four analyses; section 10 lists what it covers.

## 4. Frame, time and movement

### 4.1 The frame

Native coordinates are integer arena units, **1000 per tile**, x ∈ [0, 18000),
y ∈ [0, 32000). This comes from the oracle trace itself, not inferred: the trace header puts the
side-0 king at (9000, 3000) and the princess towers at (3500, 6500) and (14500, 6500),
and the orientation trace's probe grids declare width 18, height 32, cell size 1000.
Side 1 is the exact 180° rotation.

### 4.2 Tick length: 50 ms, one position write per tick

The traces carry one frame per 50 ms, which alone cannot rule out finer internal
sub-steps. The discriminator is position truncation, and it is decisive. Modelling *n*
independently truncated sub-steps per tick, re-aiming at the waypoint from each
intermediate position and fitting the per-sub-step speed **freely** over 1..400:

| Sub-steps per tick | Surviving speeds |
|---|---|
| n = 1 | exactly one per card (60, 52, 54, 90, 90, 120) |
| n = 2..5 | **none, for any card** |

Independently: `DeployTime / 50` equals `first_move_tick − first_seen_tick` for 149 of
150 units (the exception is a recording-window artefact — that trace's first recorded
frame is already tick 114), and tower-damage gaps are whole ticks at 50 ms for every card
with a `HitSpeed` in the corpus (Knight 1200 → 24, MiniPekka/HogRider 1600 → 32, Giant
1500 → 30, Golem 2500 → 50).

`TICK_MS = 50`. The position update runs once per 50 ms.

**A note on the ledger's counter-argument.** `time.TICK_MS`'s `disagreement.60_TPS_16.67ms`
says PhoenixNoRespawn's `DeployTime = 733 ms` "is 44 ticks at 60Hz (733.33) but 14.66 at
20Hz". 733 / 16.667 = 43.98, not 44 — the argument needs 733.33 and the shipped value is
733. Scanning every millisecond duration in `csv_logic/characters/*.toml`, only 7 values
are not multiples of 50, and one of them (Lightning `HitSpeed = 460`) is a whole number
of ticks at *none* of 50, 33.3 or 16.67 ms. "Shipped durations are whole ticks" is void
as an argument in either direction; that disagreement entry should be struck, not left
standing beside a measured value.

### 4.3 The Speed column is millitiles per tick

Fitting an integer `S` such that every moving tick satisfies
`step = (trunc(S·dir_x/256), trunc(S·dir_y/256))`, with `S` free over 1..5000 and no card
table, leaves exactly one survivor per card. Every unit of a given card fits the same `S`:

| card_id | Card | Units | `S` (units/tick) | TOML `Speed` | Step law holds |
|---|---|---:|---:|---:|---|
| 26000000 | Knight | 131 | 60 | 60 | 26101 / 26575 = 98.22 % |
| 26000003 | Giant | 12 | 52 | 45 | 3238 / 3732 = 86.76 % |
| 26000009 | Golem | 1 | 54 | 45 | 270 / 270 = 100 % |
| 26000010 | **Skeletons** | 3 | 90 | 90 | 260 / 367 = 70.84 % |
| 26000018 | **MiniPekka** | 1 | 90 | 90 | 165 / 165 = 100 % |
| 26000021 | HogRider | 2 | 120 | 120 | 229 / 250 = 91.60 % |

So one `Speed` unit is one native unit per 50 ms tick, i.e. tiles/s = `Speed`/50. At
18000 subtiles per tile the multiplier is exactly **18**. The competing "Speed is
tiles/minute" reading is the one that gives 15, and it predicts a Knight at 1.00 tiles/s
against the measured 1.20 — a flat 20 % error on every unit in the game.

The scale check is genuine, not circular: in steady state the heading is never
axis-aligned (the unit is steering back onto its column), so `S` is pinned by the exact
integer step. A direction-free cross-check agrees — the modal pure-y displacement on
ticks with `dx = 0` is (0, 60), (0, 90), (0, 120), (0, 52), (0, 54) for the respective cards.

**Scope.** This is the *unbuffed base* speed at level 11. `csv_logic` is full of
`SpeedMultiplier` keys (rage, ice wizard, snowball, prince buff, hero forms) and nothing
here measures how they round. Separately, the traces show that the level multiplier
exists and does not touch Speed: measured tower damage per hit runs 2.556–2.559× the
level-11 TOML `Damage` for all six cards, while measured `S` equals TOML `Speed` exactly
for every non-stomp card.

### 4.4 The position update

```
x += trunc_toward_zero(S * movement_direction_x / 256)     # per axis, independently
y += trunc_toward_zero(S * movement_direction_y / 256)
```

The sub-unit remainder is **discarded every tick**. There is no fractional accumulator:
integrating this rule alone from the first moving tick reproduces the final position
exactly for every isolated unit over 106–308 ticks, with zero drift.

Truncation toward zero, not floor, and not round. This is load-bearing and discriminating:
`dir_x < 0` on every moving tick of every walk unit, and floor differs from truncation on
the x axis on every one of those ~1000 ticks. Floor matches 0 ticks; round matches 17–43
per card; ceil matches 201/240 (Knight). In Rust, `i32 / i32` is already correct and
`div_euclid` is wrong.

### 4.5 The heading

```
d   = centre(path_nodes[-1] at tick t-1) - position(t-1)
L   = isqrt(d.x*d.x + d.y*d.y)                    # integer square root, floored
dir = (trunc(d.x*256 / L), trunc(d.y*256 / L))    # integer divide, toward zero
```

Computed from the **pre-move** position toward the node current at the start of the tick.
Post-move as the origin scores 85–264 matches per card against 106–307 for pre-move.

The floored length is what matters, not the choice of integer-sqrt routine:
`floor(math.sqrt(L2))` and `math.isqrt(L2)` score identically, while dividing by the
*unfloored* length matches 14–42 ticks per card and `ceil(sqrt)` 14–41. Over 1258 walk
ticks: floored-sqrt with truncating divide 1258/1258; nearest-integer sqrt 1179;
sqrt-after-scaling-by-256 191; ceil 173; a +1 sqrt 76.

The 256 is a normalisation, not a clamp. Because each axis truncates independently,
`|dir|` legitimately ranges over 254.678..256.236 — the rounded magnitudes are {255, 256}
and 257 never appears. Do not renormalise.

Worked example, Knight tick 121: `pos_120 = (3499, 8500)`, node 654 → cell (col 6,
row 18) → centre (3250, 9250), `d = (−249, 750)`, `isqrt(624501) = 790`,
`(−249·256)//790 = −80`, `(750·256)//790 = 243` → observed `movement_direction = (−80, 243)`.

Heading is **stateless**: a pure function of position and current node, recomputed from
scratch each tick. It snaps — 71.57° in a single tick on a spawn tick. There is no
turn-rate limit *that binds*: no sample anywhere exceeds 71.57°, so a hypothetical cap
above ~72°/tick would be invisible here. Implement it stateless; do not add inertia.

### 4.6 There is no ease-in

The apparent ramp over the first ~40 moving ticks (Knight |step| 58.82 → 60.00) is
entirely per-axis truncation of an off-axis heading while the unit steers onto its column.
The fitted `S` already reproduces the *first* moving tick exactly. No acceleration term,
no partial first step.

### 4.7 The Giant/Golem anomaly, and how much of it is actually solved

Giant and Golem both carry `Speed = 45` yet measure 52 and 54. Both are among the five
shipped cards with `StopMovementAfterMS`, and both visibly freeze in the traces. The
proposed rule:

```
S = floor(Speed * (StopMovementAfterMS + WaitMS) / StopMovementAfterMS)
```

Giant: `floor(45·740/640) = 52`. Golem: `floor(45·1200/1000) = 54`. Both correct.

**But this is a two-point fit and it is not uniquely identified.** The Golem's exact
value is 54.00000, so it discriminates no rounding mode at all; only the Giant's 52.03125
separates floor/trunc (52) from ceil (53). Two algebraically distinct rivals agree on
every card Supercell ships:

| Rival | Giant | Golem | GoblinGiant | RoyalGiant | IceGolemite |
|---|---:|---:|---:|---:|---:|
| `floor(Speed·(Stop+Wait)/Stop)` | 52 | 54 | 69 | 52 | 52 |
| `Speed + floor(Speed·Wait/Stop)` | 52 | 54 | 69 | 52 | 52 |
| `Speed + Mass − 11` | 52 | 54 | — | — | — |

The real evidence for the duty-cycle reading is not the arithmetic — it is that the *same*
`Stop`/`Wait` numbers independently predict the observed pause cadence. That coupling is
what should be cited. The rounding mode stays undetermined; `IceGolemite` (Speed 45,
470/80 → 52.66) is the one shipped card that would separate floor (52) from round (53),
and it has no trace.

### 4.8 The stomp pause schedule

With `k = 0` on the unit's first moving tick and never reset, the unit is stationary iff

```
((k + 1) * TICK_MS) mod (StopMovementAfterMS + WaitMS) > StopMovementAfterMS
```

Both free choices are pinned, not asserted. Strict `>` beats `>=` (Giant 308/308 vs
304/308; Golem 306/306 vs 294/306 — the Golem discriminates because `Stop = 1000` lands
exactly on the 50 ms grid), and `(k+1)` beats `k` (Giant 268/308, Golem 282/306). Every
phase offset 0..T/50 was brute-forced; only offset 0 fits.

Observed: Giant 12 moving ticks, 2 stopped, then 13/2 repeating with a 1-tick stop every
fifth cycle (74 ticks = 5 × 740 ms). Golem 20 moving, 3 stopped, then 21/3 on a 24-tick
period. Holds 100 % on 7 of the 8 unobstructed Giant traces.

Two corrections to figures that were circulating: the Giant's duty cycle is **9 stop ticks
per 74**, duty 0.8784, long-run mean **45.68** units/tick against an observed 45.79 — so
the Giant *overshoots* `Speed = 45` by about 1.5 %, in the same direction as the Golem
(47.25 model, 47.53 observed). The earlier "duty 0.8649 → 44.97, undershoots" figure is
wrong. The 50 ms grid samples the pause window coarsely; the engine must reproduce `S`
and the schedule, not the average.

**The pause is not an absolute freeze.** In `meet/Giant_vs_Giant_seed15` the side-0 Giant
moves on 2 ticks the schedule marks as pauses (t281 and t296) while being jostled. The
schedule must gate the unit's own locomotion, not the position write, so that separation
and pushback still displace it during a freeze.

### 4.9 Where the movement laws stop

The old domain statement — "the step law fails iff `avoidance_offset != 0`" — is **false**,
and this is the most consequential correction in the whole pass.

Every step-law failure in the corpus is one of three regimes, and **two of them set no
flag at all**:

| Regime | `behavior_state` | Flags | Cases | Signature |
|---|---|---|---:|---|
| Avoidance | 1 | `avoidance_offset != 0` | ~940 | heading deflected, \|step\| within ±2.3 % of `S` |
| Crowd separation | 1 | **none** | 137 | \|step\| ≈ `S`, lateral residual 15–28 units ⟂ to `movement_direction` |
| Combat pushback | 2 | **none** | 36 | displacement 0.06–2.5 × `S`, can point *opposite* to `movement_direction` |
| (one further case at `behavior_state` 4) | 4 | none | 1 | — |

No field in the corpus records the contact impulse, so any push must be inferred from the
residual (observed step minus predicted step).

Concrete witnesses. `walk/Skeletons_x3.5_y8.5_seed2.jsonl.gz` contains **three** skeletons, and
a loader that latches the first non-tower entity sees only one of them. Skeletons #1 and #2 fail the step law on 54/137
and 53/123 of their moving ticks and **no integer `S` in 1..400 fits either** — e.g. #1 at
t193 steps (−23, 88) where the law predicts (4, 89), a pure lateral push of (−27, −1)
while |step| stays 87–92 for `S = 90`. In `meet/Knight_vs_HogRider_seed15` t204–t221 the
Knight is pushed at up to 46 units/tick in the direction opposite to its own
`movement_direction`. In `meet/Giant_vs_Giant_seed15` ticks 289–301, both Giants in
`behavior_state` 1 with every flag clear, the separation is 1465–1501 units — exactly
2 × `CollisionRadius` 750.

The honest domain is: **the movement laws hold for a unit that is not in contact with
another unit.** That is a narrower and more useful statement than the flag test, and it
hands the collision work a dataset that was not obviously there.

### 4.10 `avoidance_offset` is a bounded walk, not a decay

Corpus-wide, `avoidance_offset` changes by only ±10 and ±190. Onsets are always 0 → ±190
(23 negative, 13 positive; 36 runs total). But **only 20 of 36 runs shrink monotonically**:
runs are 19 to 67 ticks long (median 21) against the 19 a pure decay implies, and 16 runs
ramp back *up* by 10/tick before coming down — e.g. `lane_sweep_Knight/c02_r02` goes
−190, −180, … −20, −30, −40, … −70, −60, −50, … −10.

It is a ±10-per-tick walk bounded at ±190, not a decay. "Set to ±190, decremented by 10
per tick toward 0" must not be written into the ledger. The deflection it applies is also
not a simple rotation: the ratio of heading error to `|avoidance_offset|` ranges 0.327 to
0.387, non-linearly. What the offset *does* is unmeasured.

Avoidance also perturbs speed slightly, contradicting "the unit still moves at exactly `S`":
truncation toward zero can only shorten a step and `|dir| <= 256.24`, so `|step| <= 60.06`
for `S = 60`; the measured maximum on avoidance ticks is 61.39, and 255 of 437 lane-sweep
avoidance ticks exceed that ceiling. The magnitude histogram is {59: 60, 60: 276, 61: 101}
— roughly ±2.3 %.

### 4.11 Deploy timing

A unit first appears on the tick **after** the accepted deploy command. `behavior_state`
is 4 (deploying) for `DeployTime/50 − 1` ticks, flips to 1 on the tick *before* the first
move, and

```
first_move_tick = first_seen_tick + DeployTime / TICK_MS
```

149 of 150 units. `LoadTime` (600–1500 ms across these cards) does not gate movement; it
gates the first attack.

The spawn tick, not the command tick, is the anchor. The Golem's command is accepted at
tick 113 rather than 100 because it waits on elixir (the header's `resource_before` shows
elixir 8 against `card_cost` 8, versus elixir 7 / cost 3 for the Knight), and it spawns at
114, targets, paths and moves at 174. A rule written against the deploy tick fits the
other five cards perfectly and is 14 ticks wrong on the Golem.

## 5. The path representation

### 5.1 Encoding

`path_nodes` are half-tile cell indices on a **36 × 64** grid of 500-unit cells:

```
col = v % 36        row = v // 36        centre = (col*500 + 250, row*500 + 250)
```

The decode is pinned by an exact integer identity, not by a plausibility score.
`path_segment_direction` equals the 256-scaled direction from the unit to the last
element's cell centre on **3781 / 3781** segment-assignment ticks. Sweeping the offset,
a 10-unit shift in either axis collapses the match to 2–3 % (x) or 74 % (y); grid widths
18/32/34/35/37/38/64/72 all score ≤ 0.08 % while 36 scores 100 %. A brute-force sweep of
every width 8..128 in both row-major and column-major, scored by trajectory distance,
puts width-36 row-major at mean 120.6 units against 4613.3 for the runner-up.

(An earlier decoder score based on point-to-polyline distance is nearly blind to the row
offset — every walker moves almost due +y, so a 500-unit row shift costs almost nothing.
Record the segment-direction identity as the evidence, not that score.)

Nodes are **absolute arena cells, not per-side**. Side-1 walkers decode in the same grid:
mean trajectory distance 118.4 and 141.1 units, against 10238 and 11484 for a
180°-rotated decode.

### 5.2 Ordering and shape

The list is stored **goal-first**; the next waypoint is the **last** element, popped from
the tail. Over ~486 000 adjacent pairs in the corpus, **zero** are not 8-neighbours — it
is a contiguous cell chain with no smoothing or string-pulling.

Paths are weakly monotone toward the goal: no recorded path in the corpus contains both a
forward and a backward row step. (The 6334 apparent "southward" steps are all side-1
walkers, whose goal is at low y.) They are *not* column-monotone — 1339 purely horizontal
steps occur, e.g. (2,15) → (3,15) and (10,29) → (9,29).

The recorded path is **not anchored at the unit's cell**: the first recorded node sits
at Chebyshev distance 2 from the unit's cell in 113 of 128 lane-sweep traces and distance
1 in the other 15. That distance is a *consequence* of the consumption rule (5.3), not a
rule of its own — a "drop the first two cells" rule gets the Giant wrong, because both
`repath_Giant` first paths drop only one.

Longest list observed: **46** nodes. The trace schema caps at 115 and no experiment here
comes close, so whether the engine truncates is untested.

### 5.3 Node consumption: a 1000-unit standoff, tested after the move

The often-repeated description of this mechanism is "the unit reaches the node". It
does not. Measuring, for every consecutive tick pair with a structurally stable list,
the distance from the **post-move** position to the tail node's cell centre:

| | n | min | max |
|---|---:|---:|---:|
| Ordinary drops (list still non-empty) | 3731 | 637.2 | **1074.1** |
| Terminal drops (list becomes empty) | 120 | 774.1 | 1436.1 |
| Keeps | 27 824 | **1000.6** | — |

**No single Euclidean threshold exists** — the drop maximum (1074.1) exceeds the keep
minimum (1000.6). What *is* exact is one-sided:

| Predicate | Missed drops | False drops |
|---|---:|---:|
| **post-move Euclid ≤ 1000** | **148** | **0** |
| post-move Euclid ≤ 1003 | 66 | 41 |
| post-move Euclid ≤ 1001 | 121 | 3 |
| pre-move Euclid − `S` ≤ 1000 | 133 | 83 |
| post-move Chebyshev ≤ 1000 | 23 | 1988 |
| post-move Manhattan ≤ 1000 | 2525 | 0 |
| integer squared ≤ 10⁶ | 148 | 0 |

`dist <= 1000` from the post-move position **never fires early** across 27 824 keep-ticks,
and fires late on 148 of 3731 drops (4.0 %), by at most 74 units. 60 of the 148 carry
`avoidance_offset != 0`; the 88 with no flag span 1000.0–1030.4. The threshold is
card-independent (best per-card fits: Knight 1003, Giant 1003, Golem 1001, HogRider 996,
Skeletons 993) — it is one tile, not a function of `CollisionRadius` or `Speed`.

**At most one node per tick.** Zero of 3851 tail-drop ticks dropped more than one.

The rival "integer remaining-distance counter decremented by the speed" model is
measurably *worse*, not better: it gets the wrong pop tick on 518 of 3550 segments with a
systematic error histogram {−2: 148, −1: 102, +1: 102, +6: 91, +7: 15}, against 271 for
live Euclid. The earlier reading that the overshoot favours the counter is backwards.

**The goal node is never walked onto.** It is dropped on the tick the unit enters
`behavior_state` 2, still 1046.7–1436.1 units from the goal cell centre; across 116
terminal pops, every one has `behavior_state != 2` on the previous tick and 2 on the pop
tick, and none was within the 1000 standoff. The census of `(behavior_state, has_path,
has_target)` has exactly four cells over 46 304 ticks — (1, T, T) walking, (2, F, T)
attacking with the path cleared, (4, F, F) deploying, (1, F, F) the single transition tick
— so the path clear and the state change are the same event.

Because the unit turns a full tile before each node, **corners are cut**: closest approach
to a corner node averages 152.9 units and reaches 395.5. That falls out of the 1000-unit
standoff plus per-tick re-aim; it needs no separate rule.

### 5.4 The two direction fields

`movement_direction` is the live bearing: `norm256(centre(node_{t−1}) − pos_{t−1})`, i.e.
the heading of the step that produced frame *t*. It matches on 29 717 / 29 996 live-path
ticks (99.07 %), against 70.54 % for aim-from-current-position, 52.71 % for the frozen
segment, and **0.56 %** for "the quantised step actually taken".

An independent check that does not fit one derived quantity to another: the angle between
`(pos_t − pos_{t−1})` and the `movement_direction` recorded at tick *t* is under 1° on
30 267 / 31 359 moving ticks (median 0.529°), against 27 154 for the value at *t−1*.

`path_segment_direction` is **sticky**: it changes only on ticks with
`path_node_consumed == 1`, and then equals `norm256(next_node_centre − post-move position)`.
Zero exceptions on the walk traces. It is the segment heading frozen at acquisition and is
otherwise unused.

`entity.x2 / y2` is exactly the previous tick's `(x, y)` — **49 333 / 49 333**, no
exceptions. That makes the heading law checkable inside a single frame, and gives a
trace-diff harness a free per-tick anchor.

### 5.5 `path_node_consumed`

Not a one-tick latch. The census over all 150 units:

| `has_path` | `path_node_consumed` | Ticks |
|---|---|---:|
| False | 1 | 17 624 |
| True | 1 | 4 031 |
| True | 0 | 27 828 |
| False | 0 | **0** |

It sits high for the whole deploy phase and the whole attacking phase — 4004 runs of 1s,
373 of them longer than one tick, the longest **257 ticks**. The reading that fits every
tick is:

> `path_node_consumed == 1` iff there is no segment currently being walked — i.e. the path
> is empty, **or** the segment was (re)assigned this tick.

The circulating rule "a recompute that leaves the last node unchanged must NOT set it" is
inverted by the data: such recomputes split 95 with the flag set against 2 clear. Two of
the four supposed exceptions are path *creations* (`repath_Giant/row11.5_dx±` at tick 121,
previous list empty), where the rule cannot even apply, and every other creation in the
corpus carries 1.

## 6. The pathfinder

### 6.1 Grid

The 2026 tilemap (`data/raw/cr-15.535.29/tilemaps/tilemap.csv`) parses to 64 rows × 36
columns, histogram `{0: 1386, 1: 309, 2: 309, 16: 104, 17: 36, 18: 36, 32: 112, 128: 8,
257: 1, 258: 1, 512: 2}`. It is bit-identical to the 2018 data behind
`data/derived/arena.json` except 12 cells that only *gain* marker bits: 256 at the two
bridge centres (7,32) and (28,32), 512 at the arena centre (17,41) and (18,41), and 128 on
eight cells. Adding 256 to the road set changes nothing, so the 2026-only bits are not
pathfinding costs. Alignment is forced by geometry, not assumed: the bit-16 block spans
cols 15–20 × rows 3–8, centred exactly on the side-0 king at (9000, 3000), and water
occupies rows 30–33 with non-water columns 5–8 and 27–30 at the bridges.

The cost grid is invariant under 180° rotation and under independent vertical and
horizontal flips (0 mismatching cells), so a row- or column-orientation error when parsing
is harmless for pathfinding — though it would matter if the lane bit 1-vs-2 distinction
ever becomes load-bearing.

### 6.2 Costs

Scored as "is the oracle's own recorded path exactly cost-minimal between its own
endpoints", over all **150 first-paths** including the `meet/` traces and side-1 units,
with a Dijkstra written independently of the analysis scripts:

| Model | Suboptimal | Infeasible |
|---|---:|---:|
| **road 5 / plain 8 / water blocked / bit-16 blocked / diag √2** | **0** | **0** |
| flat 8 everywhere (no road discount) | 96 | 0 |
| road 4 / plain 8 | 1 | 0 |
| road 6 / plain 8 | 96 | 0 |
| road 5 / plain 7 | 96 | 0 |
| road 5 / plain 9 | 1 | 0 |
| water traversable at cost 7 | 25 | 0 |
| water at cost 50 | 0 | 0 |
| bit-16 at cost 50 instead of blocked | 0 | 0 |
| no occluders at all | 12 | 0 |

"Road" is the tilemap lane bits 1 or 2. `PATHFINDING_ROAD_COST` and
`PATHFINDING_MATCHINGROAD_COST` are both 5 in the live globals, so nothing here can
distinguish a unit's own lane from the other one.

**Water is effectively impassable to ground units.** `PATHFINDING_WATER_COST = 7` is in
the shipped globals but modelling it that way makes 25 first-paths strictly dearer than
the optimum — the oracle refused water shortcuts it would have taken. Cost 50 and a hard
block are indistinguishable here, as are cost 50 and a hard block for bit-16 terrain and
for buildings: no oracle path ever needed to cross one.

### 6.3 The diagonal weight

A diagonal step costs **√2 × the entered cell's cost**. Sweeping the multiplier over all
150 first-paths:

| Multiplier | Suboptimal paths |
|---|---:|
| 1.0 (diagonal = straight) | 3 |
| 1.2 | 2 |
| **1.4142** | **0** |
| 1.5 | 1 |
| 2.0 | 96 |

In exact rational arithmetic the zero-failure window is [1.38, 1.48], which contains √2.
(Float rounding at 1e-6 produces a spurious 116/128 failure for √2 — use `Fraction` if
you re-check this.) The witnesses that kill the uniform diagonal are
`lane_sweep_Knight/c00_r06`, `c08_r04` and `c10_r06`, each of which pays one or two extra
steps to keep its diagonals scarce.

This matters beyond pedantry: with a uniform diagonal, `road 5 / plain 8` and a flat cost
8 classify every path identically, so the road discount is *unidentifiable*. Fix the
diagonal and flat-8 fails 96 paths. The cost model is a **joint** fit — the road discount,
the √2 diagonal and the occlusion set are each load-bearing for the others, and no
per-axis sensitivity table is meaningful outside the winner.

### 6.4 Building occlusion

A building blocks every half-tile cell overlapping the **axis-aligned, half-open** box
`[cx − R, cx + R) × [cy − R, cy + R)` with `R` = its `CollisionRadius`:

```
cols (cx - R)//500 .. (cx + R - 1)//500
rows (cy - R)//500 .. (cy + R - 1)//500
```

Per-axis, never Euclidean. Scored over all 3776 recorded path lists, the per-axis square
gives 0 suboptimal / 0 infeasible, while circle-overlap at `R` gives 45 / 216,
circle-overlap at `R + 250` gives 27 / 246, and cell-centre-within-Euclid gives 23 / 0.

Radius brackets, from the first-path optimality and blocked-but-used tests:

| Occluder | Shipped `CollisionRadius` | Measured bracket | Evidence |
|---|---:|---|---|
| Cannon | 600 | **(500, 1000]** | R ≤ 500 → 2 suboptimal; R ≥ 1005 → 18 blocked-but-used |
| Princess tower | 1000 | **(500, 1000]** | R ≤ 500 → 11 suboptimal; R ≥ 1100 → 155 blocked-but-used |
| King tower | 1400 | **unmeasured** | R = 0 scores identically to R = 1400 (0/0); only R ≥ 2000 fails |

The 500-unit cell quantum is why the brackets are wide: for a building on a tile centre,
every R in (500, 1000] blocks the same cells. `R = CollisionRadius` is therefore
*consistent with* the data and corroborated by three different shipped radii producing the
three observed footprints under one unfitted rule — it is not independently measured to
scale with `CollisionRadius`.

**Half-open, not closed.** A closed box blocks 57 cells the oracle's own paths use. The
discriminator is the *tower*, not the Cannon: for all five Cannon positions the half-open
and closed boxes are identical (none of `x ± 600`, `y ± 600` is a multiple of 500), while
the towers' radii 1000 and 1400 are exact multiples of the cell size.

**No mover radius, no clearance pad — the term is exactly zero.** A pad of even **1
native unit** blocks 155 cells the oracle's own paths use, because `cx − R = 2500` lands
exactly on a cell boundary for the princess towers and the Giant's control path runs up
column 4 at rows 11–16. Cannon 600 + Giant 750 = 1350 would block the very column the
`cannon_dx+0.0` detour takes. The Cannon alone brackets the pad to [0, 400]; including the
towers pins it at 0.

**The deployer's own towers must occlude** — without any occluders, 12 first-paths are
strictly dearer than the optimum.

**`FRIENDLYONLY_OCCLUSIONS` is not measured here.** Adding the enemy towers as occluders
scores identically to friendly-only (0 suboptimal, 0 infeasible, 0 blocked-but-used),
because no interior cell of any first path lies inside an enemy tower box. The argument
that "goal cells sit inside the enemy box, so enemy towers cannot occlude" is answered
equally by the goal-cell exemption below. The globals say `TRUE`; the traces are silent.

**The goal cell is exempt from occlusion.** Four of 150 first-path goal cells lie inside a
tower box — (6, 49) for MiniPekka and Skeletons, (7, 49) for Skeletons — and across all
recorded lists 81 paths end inside an occlusion box, always as the final cell and never
as an interior one. A planner that refuses occluded cells without exempting the goal finds
no path at all for the short-reach cards.

### 6.5 The goal cell

The returned list ends at the first cell on the path whose **centre** is within
`Range + CollisionRadius` of the **target's centre point** — not its footprint, and not the
sum of both collision radii.

Verified as a two-sided condition (goal in reach *and* its predecessor out of reach) on
**150 / 150** first paths, including the `meet/` traces with moving targets and side-1
units. Rivals: "Range only" 1/140, "Range + both radii" 0/140, "to the footprint edge"
0/140.

The observed stop cells from the identical deploy tile (3.5, 8.5), all approaching the
enemy left princess tower at (3500, 25500):

| Card | Range | CollisionRadius | Reach | Goal cell | Distance to target |
|---|---:|---:|---:|---|---:|
| Giant | 1200 | 750 | 1950 | (6, 47) | 1767.8 |
| Knight | 1200 | 500 | 1700 | (6, 48) | 1274.8 |
| Golem | 750 | 750 | 1500 | (6, 48) | 1274.8 |
| HogRider | 800 | 600 | 1400 | (6, 48) | 1274.8 |
| MiniPekka | 800 | 450 | 1250 | (6, 49) | 790.6 |
| Skeletons | 500 | 500 | 1000 | (6, 49) | 790.6 |

Two caveats that belong with the rule:

1. **The constant is pinned only to about ±75 units.** A constant offset `k` added to
   `Range + CollisionRadius` fits every one of the 150 first paths for any
   `k ∈ [−125.2, +24.8)`. Zero is inside that window; so are several other values.
2. **The rule is necessary, not determinative.** It is a condition on the oracle's own last
   node, not a predictor of which cell the search will stop at. Between 12 and 52 cells per
   sample satisfy it (median 32), and the oracle's goal cell is not the *cheapest*
   reachable in-reach cell in 114 of 140 samples, with no cost ties among them. Handing an
   A* the oracle's goal cell raises exact-sequence reproduction from 22/140 to 43/140 —
   the goal cell is an output of the expansion order, not an independent rule.

For a target that is *below* the unit, the sign flips: the standoff is on the approach
side. In `meet/Knight_vs_HogRider` ticks 250–273 the Knight's target sits below it and the
goal row is `cell_centre_y + reach`. An unconditional minus puts the goal on the far side
of the target.

The goal *column* is the unit's own column clamped into the target's collision box. The
half-width is only bracketed to **(500, 1000]** by the traces — 750, 900 and 1000 score
identically at 98.96 % per-tick — and 1000 is corroborated by `csv_logic/buildings.csv`
column 91, which gives `CollisionRadius` 1000 on every PrincessTower variant and 1400 on
KingTower. Read it per target; do not hard-code 1000.

### 6.6 Replan cadence: event-driven, no timer

Across 150 units and 31 859 path ticks there are **150 structural recomputes**, 0.47 per
100 path ticks, evenly spread across families (walk 0.00, lane_sweep 0.46, building_Giant
0.57, repath_Giant 1.03, meet 0.69). Every one is explained by exactly two triggers, and
the rule was tested as a *prediction*, not a post-hoc label:

**Trigger 1 — the goal cell moves.** 137 / 137 predicted goal-column flips are followed by
a replan, with **0** predicted flips that produce none and **0** replans without a
predicted flip other than the two building drops. Lag histogram {1 tick: 128, 2 ticks: 9},
mean 1.07. The flips happen as the unit crosses x = 3000, 3500, 4000, 14000, 15000 and the
column clamp moves. The goal *row* never moves in any of the 139 recomputes.

The trigger is not specific to the unit's own motion: in `meet/Knight_vs_HogRider` a Knight
chasing a moving HogRider recomputes at ticks 169, 172, 176, 180, 185, 189 — gaps of
3,4,4,4,5, which looks periodic until you classify them, and all six are goal-cell moves
driven by the target's motion.

**Trigger 2 — a friendly building comes into existence.** Lag **0 ticks from the entity**,
+1 from the accepted command. In both `repath_Giant` traces the command is accepted at
tick 160, the Cannon entity first appears in a frame at tick 161, and the Giant's path is
already replanned in that same frame. An engine that waits a tick would be wrong.

The two `repath_Giant` ticks are the strongest occlusion witness in the corpus and are easy to
miss: a filter that looks for the node list *growing* skips tick 161
(35 → 35 nodes) and picks up ticks 389 and 290 instead, by which point the Giant has
walked past the Cannon and both paths are dead-straight single columns that any model
reproduces. At tick 161 the two traces diverge in exactly the right way: with the Cannon at
(3500, 11500) (box cols 5–8, rows 21–24) the route detours to column 4 for those rows; with
it at (2500, 11500) (box cols 3–6) the route swings out to column 7. Each path avoids its
own box and violates the other's.

The replan is **whole-route**, not a spliced local bypass: in the `building_Giant` traces
the detour appears in the *first* path the unit is ever recorded with, ten rows (5 tiles) ahead of
it, and in `repath_Giant/row11.5_dx+0.0` the replan changes rows 42–46, far beyond the
obstacle. Which side it takes falls out of the cost, not a handedness constant — the two
informative offsets go opposite ways (dx+0 left at 210.28 vs 217.36; dx−1 right at 202.43
vs 232.54), reproduced at a second y by the `repath_Giant` pair. Note that in both cases
the cheaper side is also the least-lateral-deviation side, so the traces do not separate
those two hypotheses; only the *negative* claim (not a fixed handedness) is established.

**Friendly troops trigger nothing.** In `meet/Knight_then_Giant_behind` a Giant walking
directly behind a friendly Knight has 2 structural path changes in 316 path ticks. Troops
are handled by `avoidance_offset`, not by the path grid.

**The caveat that cannot be removed.** A recompute returning an identical list is invisible:
8 of 94 tail-surviving recomputes leave `path_segment_direction` frozen *and*
`path_node_consumed` clear, so a same-result replan leaves no fingerprint at all.
`PATHFINDING_SAMEPATH_EPSILON = 3` may be suppressing near-identical results. "No periodic
replan" is a statement about *observable* path changes.

### 6.7 Timing of the first path

```
first_path_tick = spawn_tick + DeployTime / TICK_MS
```

Target acquisition, the first path and the first step all land on that same tick — 149/150
units, with zero ticks anywhere in the corpus holding a non-empty path and a null target.
The path is computed from the unit's **pre-move** position: the first surviving node is at
Chebyshev distance 2 from the pre-move cell on 140/140 lane-sweep and walk units, but
distance 1 from the post-move cell on 15 of them.

The path is not created once and then only shortened. In `meet/Knight_vs_HogRider` the
Knight's path is emptied and re-created four more times (ticks 250, 271, 273, 313) while it
holds a target throughout, as it drops in and out of the attacking state. The gate is
"never build a path while `target` is None", not "build the path once at acquisition".

### 6.8 What we still cannot reproduce: the node sequence

The cost model is right — the oracle's path is exactly cost-minimal on **150 / 150**
first-paths — but the exact node *sequence* is not reproduced. The best configurable A*
reaches **22 / 140** exact matches (43/140 when handed the oracle's goal cell), and once
the corpus is deduplicated to 76 distinct experiments the ceiling is **13 / 76**, of which
6 are the walk traces (one start, one straight column) and 6 are a single lane-sweep
column.

The search space already explored: 5 heuristics × 5 queue tie-breaks × 8 neighbour orders
× relax-on-`<` vs `<=`, forward and backward, goal as a reach test on pop and as a fixed
cell with truncation — 400 configurations. Plus 640 more with open lists ordered by cell
index ascending/descending, by `g` ascending/descending, and h-then-index (plausible shapes
for a `REFRESH_OPENNODES` linked or array open list rather than a binary heap). The ceiling
does not move.

Where the optimum is ambiguous the oracle takes the orthogonal successor on 1568 of 1672
steps (93.8 %) — but an `ortho_first` neighbour order still scores 22/140, so this is a
description of the output, not the rule. With the √2 diagonal, exact ties are rare, which
means the residual error is not a classic tie-break at all. Something structural is
missing: most likely the real open-list discipline, or a post-processing step.

A typical failure: `c00_r08` agrees for 16 nodes, then the oracle moves from column 6 to
column 5 at row 35 while the model stays on column 6. The switch row varies with the start
cell for the same goal (row 35, 36, 37, 44, 48 for starts c00, c02, c04, c08, c06) — the
signature of an expansion-order artefact.

### 6.9 `path_nodes` is a post-tick snapshot

142 ticks in the corpus have a `movement_direction` that the heading law does not explain
from the recorded node list. **All 142 carry `path_node_consumed == 1`**, and 140 of them
are explained exactly by a cell **1–2 rows nearer** in the same column:

| Explaining offset from the recorded tail | Count |
|---|---:|
| (dcol 0, drow −1) | 85 |
| (dcol 0, drow −2) | 50 |
| (dcol −1, drow −1) | 3 |
| (dcol +1, drow −1) | 2 |
| unexplained | 2 |

The list also *grew* across 47 of those ticks. The mechanism, verified tick by tick on
`lane_sweep_Knight/c08_r12` t226–t228: the unit repaths at the start of the tick, the new
path inserts one or two extra half-tile nodes below the old waypoint, the unit aims at the
nearest one and consumes it **within the same tick**, so that node appears in neither the
`t−1` nor the `t` frame. At t227, `dir = (−173, 188) = norm256((3750, 16750) − (3998,
16481))` exactly, and (3750, 16750) is cell 1195, absent from both lists.

Two consequences. First, the heading law is **100 %**, not 99.54 %, and there is no
bridge or `KS_POS_TO_TARGET` exception to chase — these ticks cluster on the river rows
only because that is where the bridge approach forces a repath. Second, and more
importantly for anyone fitting a pathfinder: **the recorded `path_nodes` is a post-tick
snapshot, a repath can insert nodes that never appear in any frame, and the effective
consumption count in a tick can exceed the one visible in the lists.** Any pathfinder
fitted to the recorded node lists is missing those insertions. (The step law holds on all
142 of these ticks, so movement is untouched; the defect is purely "which node was
current".)

A large single-tick heading swing is sometimes attributed to proximity —
"31.57° with the unit 113.4 units from the node centre". The measurement's own output says
1293.0 units for that sample, at which a 60-unit step can rotate the bearing by at most
2.66°. The proximity story is arithmetically impossible; the cause is the mid-tick repath
above. Across all 128 lane-sweep traces, **0 of 127** heading changes greater than 10° with
a genuinely unchanged target node are consistent with the heading law at both ticks.

## 7. What survived, what did not

Each analysis was re-measured from scratch by an independently written implementation — loaders,
decoders, integer arithmetic and Dijkstra rewritten — so that a shared bug could not hide in all
four. The verdicts below are from that re-measurement, over all 150 units.

### Survived unchanged

| Claim | Strength |
|---|---|
| `TICK_MS = 50`, one position write per tick | Free-sub-speed fit: no integer speed survives at n = 2..5 |
| `SPEED_TO_SUBTILES_PER_TICK = 18` (Speed is millitiles/tick) | Unique integer fit per card over 1..5000; direction-free cross-check agrees |
| Position update: per-axis `trunc(S·dir/256)`, remainder discarded | Zero drift over 106–308 ticks; floor 0/1000, round 17–43 |
| Heading: `norm256(node_centre − pre-move pos)` with floored integer length | 100 % once mid-tick repath insertions are accounted for |
| No ease-in, no acceleration, no partial first step | Fitted `S` reproduces the first moving tick exactly |
| Heading is stateless; no turn-rate limit binds | Stateless model reproduces 1242/1242 walk ticks |
| Half-tile 36 × 64 node encoding, goal-first, popped from the tail | 3781/3781 exact segment-direction identity |
| `path_segment_direction` is sticky, frozen at assignment from the post-move position | Zero exceptions on walk traces |
| Cost model road 5 / plain 8, water blocked, 8-connected | 0/150 suboptimal; flat-8 96, water@7 25 |
| Occlusion = half-open AABB at `CollisionRadius`, per-axis | 0/3776; closed box 57 blocked-but-used |
| Goal truncation at `Range + CollisionRadius` to the target *centre* | 150/150 including moving targets and side 1 |
| Replan is event-driven: goal cell moves, or a friendly building spawns | 137/137 predicted flips, 0 over-predictions |
| Whole-route replan, not a local splice | Detour in the first recorded path, 5 tiles ahead |
| `first_move = first_seen + DeployTime/TICK_MS` | 149/150 units |

### Refuted or materially corrected

| Claim as stated | Verdict |
|---|---|
| "The step law fails **iff** `avoidance_offset != 0`" | **Refuted.** 174 failures with every flag clear: 137 crowd separation (state 1), 36 combat pushback (state 2), 1 at state 4. Nothing in the corpus records the push |
| "During avoidance the unit still moves at exactly `S`" | **Refuted.** \|step\| reaches 61.39 for `S = 60`, above the 60.06 truncation ceiling; ±2.3 % |
| "`avoidance_offset` is set to ±190 and decays by 10/tick" | **Refuted.** A ±10 walk bounded at ±190; 16 of 36 runs ramp back up; runs reach 67 ticks |
| "The model reproduces **every walk trace** bit-exactly" | **Refuted on coverage.** 6 files, 7 units: two of the three Skeletons fit *no* integer `S` |
| "`path_node_consumed` is a one-tick latch; an unchanged-tail recompute does not set it" | **Refuted.** Runs up to 257 ticks; unchanged-tail recomputes split 95 set / 2 clear |
| "The diagonal cost is not settled by these traces" | **Refuted.** It is settled: √2, window [1.38, 1.48]; uniform diagonal fails 3 paths |
| "Occlusion is inflated by the walker's own `CollisionRadius`" | **Refuted.** A pad of 1 unit blocks 155 used cells |
| "Reaction to a new friendly building is 1 tick / 50 ms" | **Corrected.** 0 ticks from the entity existing; tick 160 is the command, 161 is the spawn *and* the replan |
| "Path computed at `deploy_tick + DeployTime/50`" | **Corrected.** `spawn_tick + DeployTime/TICK_MS`; the Golem's elixir wait makes the two differ by 14 ticks |
| "The first two path cells are always consumed immediately" | **Corrected.** A consequence of the 1000-unit standoff; both `repath_Giant` first paths drop only one |
| "Node consumption is an integer remaining-distance counter" | **Refuted.** Wrong pop tick on 518/3550 segments vs 271 for live Euclid |
| "The princess tower's 2×2 footprint is a new tower-specific rule" | **Corrected.** It is the ordinary rule at the shipped `CollisionRadius` 1000 |
| "The king tower's 3×3 footprint is measured" | **Refuted.** `R_king = 0` scores identically to 1400; no trace goes near a king |
| "Enemy buildings demonstrably do not occlude" | **Not established.** Indistinguishable from friendly-only once the goal cell is exempt |
| "The goal rule alone explains every observed goal cell" | **Overstated.** Necessary, not determinative: the goal cell is an output of the expansion order |
| "The detour side is the cheaper side (5 offsets)" | **Overstated.** Only 2 of 5 offsets involve a detour at all, and in both the cost and least-deviation hypotheses agree |
| "Giant duty 0.8649 → 44.97 units/tick, undershoots Speed 45" | **Wrong arithmetic.** 9 stop ticks per 74, duty 0.8784, mean 45.68 — it overshoots |
| "The stomp schedule is exact" | **Scoped.** Exact for an unobstructed path-follower; a jostled Giant moves on 2 scheduled pause ticks |
| "The stomp formula is solved (high confidence)" | **Scoped.** Two algebraically distinct rivals agree on every shipped card; rounding mode undetermined |
| "117 heading failures are a river/bridge exception" | **Resolved.** All 142 are mid-tick repath insertions; the heading law is 100 % |

### A corpus pitfall worth keeping

The convenient loader latches the first non-tower entity of one side and follows only that one.
It hides 107 step-law failures inside the `walk/Skeletons` trace, makes every opposing-side unit
invisible, and skips the `meet/` directory entirely — the only traces with two mobile units, a
moving target, or a side-1 walker. Enumerate `(side, generation_key)` pairs instead; it is fifteen
lines.

## 8. Open questions

Open as of this corpus. Items 1 and 2 have since been settled on client 16.402 — the expansion
order is reproduced outright and the contact law is measured — and items 5, 6 and 9 were settled
by the same captures; `pathfinding.md` carries all of them. The rest are open, and the experiment
each one needs is named.

1. **The tie-break / expansion order.** 13/76 distinct experiments is the ceiling of a
   1040-configuration search. This is the single thing standing between the measured cost
   model and bit-exact path reproduction. Best next experiment: traces with a *unique*
   optimal path everywhere (pepper the lane with friendly Cannons so no ties exist) to
   confirm the cost model to 100 %, and traces with exactly one two-way tie to read the
   tie-break off directly.
2. **The collision / separation law.** Two contact regimes break the movement law and
   neither sets a flag, and no field records the push, so it must be fitted from the
   residual. The data already exists and
   nobody has used it: `walk/Skeletons` units #1 and #2 (crowd separation, 107 clean
   samples) and all three `meet/` traces (pushback, 36 samples plus 11 in state 1 at
   exactly 2 × `CollisionRadius`).
3. **What `avoidance_offset` actually does.** The magnitude is roughly preserved, the
   offset walks ±10 within ±190, but the applied deflection fits no constant rotation
   scale (observed ratio 0.327–0.387, non-linear). What *sets* it to ±190, and what
   selects the sign, is unknown.
4. **The residual 4 % of node pops** that fire up to 74 units late. 60 of 148 carry
   avoidance; 88 carry nothing. Likely the same fixed-point arithmetic that produces the
   Giant/Golem 52/54 mapping. A live capture carrying the mover's state fields would
   settle it faster than another trace.
5. **Occluded cells: hard block or `PATHFINDING_BUILDING_COST = 50`?** Indistinguishable
   here because no oracle path ever needed to cross one. Same for bit-16 terrain and for
   water (50 vs impassable). Settle with a corridor where going *through* is cheaper than
   going around.
6. **`FRIENDLYONLY_OCCLUSIONS`.** The globals say TRUE; the traces cannot tell. Needs a
   ground unit routed past an *enemy* building that is not its target.
7. **The goal rule's free parameters.** The reach constant is pinned only to ±75 units; the
   column clamp half-width only to (500, 1000]; the row rule rests on six data points, one
   per card, all from one start and one approach direction. The cheapest discriminating
   experiment is a Skeletons (Range 500, radius 500) in a lane 750 units off the tower
   centre, where the per-axis model says row 49 and a 2-D in-range model says row 50. A
   side-on or from-above approach would settle the axis question at the same time.
8. **Rule A vs rule H for occlusion.** "Square of half-width `CollisionRadius`" and "the
   building's own tile grown by half a tile" predict identical cells here, because every
   occluder in the corpus sits on a tile centre. Place a building at a half-tile x offset
   (x = 3000 or 4000), or one with a different radius (Tesla, Bomb Tower 600, Barbarian
   Hut), and re-run the six-offset sweep.
9. **The stomp rounding mode.** One `IceGolemite` trace (predicts 52 under floor, 53 under
   round) and one `GoblinGiant` trace (predicts 69, testing the formula outside Speed 45).
10. **`SpeedMultiplier`.** Completely unmeasured. One rage trace would calibrate the buff
    path.
11. **Does building *removal* trigger the same 0-tick replan?** The Cannon's `LifeTime` is
    30 000 ms and the traces end before expiry.
12. **`PROJECTILE_SPEED_TO_SUBTILES_PER_TICK`.** No projectile flies in these traces. The
    troop side measuring Speed/50 is weak supporting evidence for 18 on projectiles too,
    given the 2023 frame test that measured ×1.2 for both a Musketeer projectile and a
    Giant — but it is not measured.
13. **Path length cap.** The schema allows 115; the longest route these deploys can produce
    is 46. No experiment in the corpus can reach the cap.

## 9. Using this in the engine

The implementation contract derived from these measurements — grid, costs, heuristic, occlusion,
goal, per-tick update order, node consumption, replan triggers and the verification gates — is in
`pathfinder-spec.md`, and section 10 of that file maps each rule onto the ledger key that carries
it.

The regression gates these measurements support, in increasing strictness:

1. All 150 oracle first-paths are exactly cost-minimal on the engine's own grid.
2. The movement laws reproduce every *isolated* unit bit-exactly from its first moving
   tick (walk: 239, 307, 305, 164, 121, 106 ticks; zero drift; identical final positions).
3. The node-consumption predicate never fires early across the 27 824 keep-ticks.
4. Goal truncation holds two-sided on all 150 first-paths.
5. Exact node sequences — out of reach for this model (13/76), gated on the 16.402 arm instead
   (`pathfinding.md`).

## 10. Reproducing

These figures were measured over the offline corpus, which is not distributed, so the run itself
is not reproducible from a clone. What it covered, section by section:

| Section | Content |
|---|---|
| A | Node consumption: pre-move vs post-move, threshold sweeps, rival predicates |
| B | Step and heading laws over all 150 units; the 142 heading anomalies and their explaining cells |
| B2 | Free per-unit speed fit with no card table |
| C | `x2/y2`, `avoidance_offset` deltas and runs, `path_node_consumed` census |
| D | First path / first target / first move timing |
| E | Diagonal weight sweep in exact rational arithmetic |
| F | Cost-axis discrimination at √2 |
| G, G2 | Occlusion radius brackets, box shape, mover pad, goal exemption, enemy occlusion |
| H | Goal truncation rule and its admissible constant window |

The rules these sections support are gated inside this repo by `tests/oracle2026.rs` and
`tools/oracle_diff.py`, which a reader can run against any trace in the oracle format.
