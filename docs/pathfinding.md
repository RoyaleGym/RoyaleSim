# Pathfinding and contact on client 16.402

This is the model the engine runs on: how the client chooses a route, how it prices the board, how
often it replans, and how a unit's step is resolved against everything it touches. Every rule here
was settled against recorded captures of client 16.402 and re-verified against the offline 15.535
corpus; three independent reproductions of the search agree on every one of them.

Two companion files sit under it: `movement-measurements.md` is the offline evidence for the
movement laws and the cost model, and `pathfinder-spec.md` is the implementation contract written
from that evidence. This file supersedes both where they disagree, because it is measured on the
newer client.

## What the model is

A ground unit's route is an 8-connected weighted A* over a **36 x 64 grid of half-tile cells**
(500 native units per cell), with road cells at cost 5 and plain cells at 8, diagonals at x1.4,
water priced rather than refused, and every alive building of either side stamped at cost 50 over
a half-open box of half-width `CollisionRadius`. The path ends at the first cell within
`Range + own CollisionRadius` of the target.

A *recorded route* below is the node list a unit walked in a capture. Scores, all against
recorded routes:

| Measurement | Result |
|---|---|
| Cost-optimal on live 16.402 first paths | 345 / 345 (2026-09-19) |
| Cost-optimal on offline 15.535 paths | every one |
| **Exact recorded node sequence**, live 16.402 | 615 / 616 |
| **Exact recorded node sequence**, offline 15.535 lane sweep | 128 / 128 |
| Contact law: live unit-tick positions reproduced exactly | 240,389 / 242,232 = 0.99239 |

The three live misses are units chasing a *moving* troop: the sample carries the previous tick's
target position, so the goal cell the client picked is not recoverable from the trace. They are
skipped **by name** in the gate so that a fourth one cannot hide behind them.

## The search

Entered through the path request (below); implemented in `path16402.rs`.

- **One goal cell**, chosen beforehand by the move component. The search stops when *that* cell
  has been popped and expanded.
- Among equally cheap routes, the recorded one is reproduced by an open list ordered on **`f`
  alone** — no g, no h, no insertion order — in which equal keys keep their existing order. Every
  tie-break here was chosen because it reproduces the recorded node lists, not because an
  argument ruled out the alternative.
- Neighbours in the order **N, S, W, E, NW, SW, SE, NE**; orthogonals at factor 10, diagonals
  at 14.
- Step cost = the **entered** cell's cost x factor, integer, no division. A plain diagonal is 112,
  a road diagonal 70.
- `h = 5 x (10 (dx + dy) - 6 min(dx, dy))` = `50 max + 20 min` (heuristic method 1, times the
  default heuristic cost).
- An open node reached with a strictly better `f` takes the new parent, g and f
  (`REFRESH_OPENNODES`). A closed node is never revisited (`REOPEN_CLOSEDNODES = FALSE`).
- The start cell is expanded before the loop, then closed. Each popped cell is closed and
  expanded, and the goal test runs **after** the expansion.
- The result is the parent chain from the goal, **goal first**, stopping before the start cell.
  The move component copies it verbatim, dropping consecutive duplicates only. Nothing trims the list before it is recorded: the "missing start-adjacent node" traces show is the first tick's ordinary
  pop.

## The cost field

- A cell costs -1 only when it is out of bounds.
- **Water** (tilemap bit 0x20): a walker pays `BLOCKED = 50`, and that cost is final before the
  occlusion max. Hovering and `JumpEnabled` units pay `WATER = 7`.
- No cell of the shipped arena tilemap sets bit 0x40, so nothing is priced by it.
- Lane bits `& 3`: `ROAD = MATCHINGROAD = 5`, so the "matching road" concept changes nothing.
  Everything else is `DEFAULT = 8`. Then `max(base, occlusion[cell])`.
- **Bit 16 plays no part.** The king block is priced by the king tower's own R = 1400 box, and the
  arena-edge strips cost 8.
- **Water is priced, not refused.** No recorded path ever uses a water cell, because water is
  never cheaper — but pricing it still decides which of several equal-cost routes comes out.
  Refusing water instead costs 306/388 live and 24/128 offline exact sequences.
- **The building box:** the centre is snapped **up** to the next multiple of 500 on each axis
  (`((x - 1) / 500) * 500 + 500`), then every cell overlapping the half-open box `[c - R, c + R)`
  is written `max(cell, BUILDING = 50)`, with `R = CollisionRadius`. A box that would poke outside
  the grid on any side is dropped entirely — no clamping. For tile-centred buildings this is
  exactly the measured box.
- **The occlusion rebuild** runs before every tick's entity updates, from scratch, over every
  building and occluder of **both** sides, with no side test. Restricting the stamping to the
  mover's own side scores 336/388 against 388/388, so the stamping is side-blind. Only the *own*
  side's occluder set changing triggers a replan, which is what
  `PATHFINDING_FRIENDLYONLY_OCCLUSIONS` costs in observable behaviour.

## The path request, the replan gate, and consumption

The move component owns all three.

**Start** is the unit's own cell `(x / 500, y / 500)` at the moment of the request.

**The goal cell** comes from `(target, reach)` with `reach = Range + own CollisionRadius`
(`ADD_CHARACTER_RANGE_TO_RADIUS`; the target's radius is *not* added, and continuous-damage
attackers subtract 500):

- scan rows ascending over `target_cell +- (reach / 500 + 1)`, columns **ascending when the
  mover's x < 9000 native and descending otherwise**;
- keep cells whose centre is within `reach` of the target centre (`<=` on squared distances);
- class 2 = dry and outside every building box, class 1 = water or boxed — the box demotion
  applies only when `KS_POS_TO_TARGET_GROUND_AVOID_BUILDINGS` is set and the target is not flying;
- take the highest class, then the strictly smallest squared distance **to the mover**, so the
  first scanned cell wins a tie.

That scan is **absolute**, not seat-relative, which is why 51 of 54 rotated twin problems with a
cell-corner start pick a different goal cell for the two seats.

**The gate**, evaluated every tick a unit walks (states 1/6/7) and on target change: search when
there is no path, when the fresh goal cell differs from the path's head node, or when the goal is
unchanged but the **own** side's occluder set changed this tick. Otherwise keep the path. **There
is no periodic re-path and no distance trigger.** In the third case, with
`PATHFINDING_SAMEPATH_EPSILON` nonzero, the recorded list does not change unless the same-path
test finds an old node newly boxed or a new node newly freed.

**Consumption**, per tick:

```
aim  = the LAST node's centre
dist = isqrt(dist2)
step = min(speed, dist, 250)                  # sub-steps of 250 for speeds >= 250
dir  = tdiv((aim - pos) << 8, max(dist, 1))
move = tdiv(dir * step, 256)                  # per axis
# then, with the NEW position:
proj = tdiv((aim.x - x') * seg.x, 256) + tdiv((aim.y - y') * seg.y, 256)
# pop the node when proj <= 1000, one node per sub-step
```

The segment direction is then frozen from the new position toward the new last node
(`normalize(..., 256)`). Flying units get a single node — the goal cell — and no search.

**Building removal replans through the target change, not through the occluder flag.** When a
building a unit was walking at dies, the target is cleared on that tick, the old path is held one
tick with no target, and the new target (the tower) forces the new path on the next tick.

## The contact law

Settled the same way and replayed over every frame pair of all 31 live captures: **240,389 of
242,232 unit-tick positions exact** (0.99239). By situation: isolated 0.99367, separation active
0.98319, two or more overlaps 0.98406, against buildings 0.99829, avoidance active 0.98221, spawn
overlaps 0.99656, attacking 0.99277. The post-decay `avoidance_offset` is exact on 0.99962 of all
unit-ticks.

**The update order is itself part of the law.** Replaying with every neighbour at its
previous-tick position reproduces only 0.462 of the contact ticks. What the client does:

- Each tick, **every attack update runs before every move update**, and the move updates run one
  unit after another in **creation order** against a bucket index of collision circles (each grown
  by 250 for movers) built from the start-of-tick positions. Unit *i* sees units before it already
  moved.
- **Avoidance** (walking and deploying units, not attacking ones): the entities whose circle
  overlaps the circle of radius `min(R, 500)` around `pos + facing` vote; a static wins over
  movers and the last seen wins; the offset goes 0 -> +-200; a running offset moves +-20 only when
  a static is seen, clamped to +-200; then decays by 10 toward zero every walk tick. A static
  whose circle contains the current waypoint's centre pops that waypoint.
- **Separation** (walking, deploying *and* attacking units): every overlapping neighbour of either
  side — buildings and towers included, touching counts (`d2 <= (R1 + R2)^2`, own radius capped at
  500 against a static) — contributes `trunc(d * mag / dist)` away from it, with
  `mag = min(299, trunc(min(R1 + R2 - dist, 300) * M_other / M_self)) + 1`. The **mean** of those
  pushes, capped at 150, is added to the step.
- **Mass** is taken from the card data: an empty Mass column becomes
  `tdiv(floor(R^2 / 250) * R, 62500)`, and every Mass is clamped to [1, 20]. Towers, Tombstone and
  Goblin Hut weigh 20, a Cannon 13, a Tesla 8 — which is why a Knight touching a tower flies back
  at the cap.
- **The step:** `dist = max(1, isqrt)`, `step = min(speed, dist, 250)`, heading
  `trunc((aim - pos) << 8 / dist)`, per-axis `trunc(heading * step / 256)`; `facing` becomes
  `normalize256(aim - pos)`; the avoidance offset rotates the step by the blend
  `v' = ((256 - |a|) v + a perp(v)) >> 8` renormalised to the step length
  (`tan theta = a / (256 - |a|)`, with no lookup table); the collision mean is added; the position
  is written with the grid edge clamped and, **for a deploying ground unit only**, a water edge
  clamped; then `reached := proj(aim - pos', segment) <= 1000`.
- With an empty path and a target out of range, the unit walks at the point `reach` away from the
  target along the line to itself.
- **Buffs compose into the speed**, not into the step: the effective speed is
  `tdiv(max(0, min(100, 100 - maxneg)) * tdiv(maxpos * S, 100), 100)`, where Rage contributes
  +130 and Freeze -100, which floors the speed at 0. Measured live.
- **The stomp clock** (Giant-family cards with `StopMovementAfterMS` / `WaitMS`) advances by
  `tdiv(buff(100), 2)` ms per walking tick — 65 under Rage — and a tick is paused exactly when the
  clock is strictly inside `(Stop, Stop + Wait)`.

## Seats and frames

The client plans in **absolute arena coordinates**, so a Red unit's route is *not* the rotation of
its Blue twin's. `tests/mirror.rs` pins 20 of 54 mid-cell twin problems on the shipped arena
taking different routes under this arm, against 0 under the frame-planned one. The engine
reproduces that; `NavRequest.team` lets `plan_cells` un-rotate a Red request.

The rotation-mirror gates (`tests/mirror.rs`, `tests/setup_spawn_order.rs`, and the env layer's
rotation tests through `SymmetricRustEngine` / `Battle(path_search="trace_fitted_astar")`) run
under the frame-planned arm, so they keep catching seat bias in everything else.

One consequence worth knowing: `mechanics.rs::opposing_giants_pass_each_other_on_a_bridge`
deadlocked as soon as the search became the client's, because both Giants take the *same* bridge
column, as they do live; the two Giants passing each other was an artefact of mirror-image
planning. The measured contact law closes it, and the test is green again with no change to its
assertion.

## How this is gated

- `crates/royalesim/src/path16402.rs` is the search, selected by `pathfinding.PATH_SEARCH =
  client16402`. `crates/royalesim/src/move16402.rs` is the contact law, selected by
  `collision.CONTACT_LAW = client16402`; `state.rs phase_path16402` drives it in creation order.
  `Grid16402` in `Scratch` keeps the terrain, the current and previous occlusion arrays, and the
  pathfinder shared by all units. `Entities` carries `facing` and `avoid_offset` for this arm
  (`architecture.md` names the current snapshot format).
- **G6** (`tests/oracle2026.rs`) runs the whole generated fixture
  `tests/fixtures/oracle2026/client16402_first_paths.json` — **747 cases**: 619
  first paths from the recorded 16.402 battles and 128 from the offline 15.535 lane sweep — through
  `plan_cells` and compares the exact node list, goal first. Three moving-target cases are
  skipped by name, and **one case diverges**, also named:
  `auto-20260920-072831-A:8:Giant`, where the engine walks the lane straight and the client drifts
  one column sideways. Both reach the goal; which of the equal-cost lane routes a unit takes is
  the open expansion-order item. The assertion fails if the set of diverging cases changes at all,
  so a second divergence cannot hide behind the first. Net of the three skipped cases, **743 of 744**
  are reproduced exactly. `tools/make_client16402_paths_fixture.py --check` reports whether the
  fixture is in sync with the recordings; regenerating it re-scores the gate and is a deliberate
  step, not a side effect.
- The contact law is gated by an equivalence test: 20,000 random crowded worlds stepped through
  both implementations, which must agree exactly on positions, facing, offsets, reached flags and
  dropped waypoints (20,000/20,000 on two seeds; about 16,000 of them with a nonzero offset and
  about 4,400 with units moved by separation alone).
- The walk gate (`tools/oracle_diff.py`) stays 6/6 bit-exact against the offline traces (one of
  the six over 105 of its 107 ticks; `oracle_diff.py` names the shortfall).
  First-path cells are 19/21 identical to the offline corpus; the two that differ are MiniPekka
  and Royal Giant, which pick another goal cell because `cards.json` still carries their 2018
  Range. That is card data, not the search.
- Controls, so the fit is not mistaken for a free parameter: water impassable scores 306/388 live
  and 24/128 offline; friendly-only occlusion 336/388; `AVOID_BUILDINGS` off 421/425. The
  trace-fitted arm of 2026-09-18 scored 168/345 live and 15/128 offline.

## Evidence

The captures that settled each rule, by subject. Names in parentheses are the recording each
measurement came from.

### The search is deterministic

Five Knights deployed from the identical cell on an empty board across one battle
(`frames-auto-20260918-164951`) walked byte-identical 30-node paths within each target group —
three while the princess tower stood, two after it fell and the king became the target. The
expansion order is a pure function of (start cell, goal, board).

### Enemy buildings occlude

Two designed witnesses, one per lane (`frames-auto-20260919-182539-*`). A Knight's first path
bulges around an **enemy** Goblin Hut's box (cols 5-8, rows 37-40), leaving col 6 for col 4 and
back; on the other lane a replan toward the tower bulges around an enemy Tombstone's box (cols
27-30, rows 37-40). Both routes are cost-optimal only with the enemy building stamped, and not
optimal without it.

Two earlier attempts were **not** witnesses, and are recorded because the distinction matters when
designing a scenario: an enemy Tombstone 11 tiles out and a friendly Tesla both sat beside the
route rather than on it, so the recorded path was cost-optimal with or without them.

### The building box is R = CollisionRadius around a snapped centre

A friendly Tesla requested at (14500, 12500) was placed by the client at the grid **vertex**
(14000, 12000) — buildings snap to a vertex, not to a tile centre. The Knight's first path steps
around cols 27-28 x rows 23-24, which is exactly the half-open R = 500 box about that vertex, and
is optimal only with the Tesla stamped.

### Buff and stomp interaction

A raged Golem (`frames-auto-20260919-143305`) walks at exactly **S = 70 = floor(54 x 130/100)**
over 29 unambiguous ticks: the stomp schedule is applied first, then the buff, with floor
rounding. The other ordering, `floor(floor(45 x 1.3) x 1.2) = 69`, is excluded. The **stomp clock
also runs 1.3x faster** under Rage — pauses every 18-19 ticks instead of 24, with the pause length
unchanged at 3 ticks. Unraged, the same Golem holds S = 54 for 300 ticks with 3-tick pauses every
24.

### Freeze is a whole-unit hold, not a speed multiplier

An isolated Giant hit by an Ice Spirit (`frames-auto-20260919-144043`) holds **step 0 for exactly
22 ticks (1100 ms)** with its target cleared, its movement direction and its 17-node path
untouched, and resumes at the same heading on the same path with no replan. The stomp clock
freezes with the unit: its phase resumes exactly where it stopped. The projectile flight is
visible in the same trace — pending damage and a 450 ms event timer appear on one tick and count
down 50 ms per tick until the hit lands.

### Spawn separation

Skeletons deployed onto a Knight (`frames-auto-20260919-144043`) step exactly **150 native units
per tick straight away from the overlapping unit** while the centre distance is below R1 + R2, and
only the lighter unit moves: `(0, +150)` for the one directly above, `(+-130, -75)` for the two at
+-30 degrees, which is `trunc(150 x dir / 256)` with `dir` the 256-normalised offset. The distance
goes 807 -> 957 -> 1107 and stops. Nothing else moves until the units leave the deploy state.

The same shape repeats against a walking Giant: still-deploying Skeletons are shoved 150 per tick
each time the Giant's advance brings the centre distance back under R1 + R2, and the Giant
(Mass 18) never moves for them. Four Goblins from one tap spawn on a square of half-side 761
around the tap point.

### Card ids in the 16.402 corpus

One capture contains a **hero-form Musketeer**, played as a hero rather than as an evolution,
under card id 203000014 (the ordinary Musketeer is 26000014). Every one of its recorded paths
fits the ordinary Musketeer row — Range 6000, CollisionRadius 500 — at max hp 721 and S = 60 at
level 11. What name string the client attaches to that id is an open 16.402 card-data question;
nothing about its movement differs.

### Avoidance fires on the unit's own route, not on nearby geometry

A Knight passing a building box two columns away keeps `avoidance_offset` at 0. On the same walk
it fires twice, both times where its own path runs along a building box edge: -180 right after
spawn while steering out from beside its own princess tower, and -190 -> -80 approaching the enemy
tower before the attack — in both cases decaying by 10 per tick, and continuing to decay while
attacking.

## Open

1. **The A\* expansion order among equally cheap routes**, which is what the one named G6
   divergence turns on.
2. **The three moving-target residuals** need a sample that carries the target's position on the
   planning tick.
3. **Card data.** `cards.json` Range values for MiniPekka, Royal Giant and Knight come from a
   table the fixture does not trust for reach, so the goal cell is only right when the correct
   reach is supplied (the fixture passes the live reach in).
4. **The movement states the captures show for spawn pathfinding** (6 and 7: no lane bonus,
   buildings ignored) and for attached characters are not modelled. `movement.SPAWN_PATHFIND_STATES`
   is the ledger key, at `hypothesis`, because nothing recorded yet exercises it.
5. **The buff tags** — `NO_PUSHED_BY_*`, `DISABLE_PHYSICAL_INTERACTIONS_WITH_OBJECTS`,
   `AVOIDANCE_AS_OBSTACLE`, the facing lock — are assumed clear, because no card in the corpus
   carries them.
6. **Unit update order.** Units update in an order approximated by (spawn tick, slot) rather than
   creation order. The 655 unexplained live unit-ticks — 0.27% — are Skeleton crowds with dying
   neighbours, where that order decides.

Four items that stood here are closed and are documented above and in `mechanics.md`: the
knockback ladder (`knockback.DISPLACEMENT_LAW = client16402`), the building demotion's second
condition (`path2026.rs avoid_buildings16402`), the river hop (`movement.JUMP_WATER_HOP`,
`jump16402.rs`) and the tick order (`match.TICK_ORDER = client16402`).

## The earlier arms

Three pre-2026 path models remain in `path.rs` behind `PathModel`: `LaneSnap`, `GridAStar` and
`DiagonalLookahead`. They are community-derived rather than measured, and they are weakly
distinguishable from each other: on a reference Giant walk, mean track separation is 1.37 tiles
between LaneSnap and GridAStar, 1.21 between LaneSnap and Diagonal, and only **0.52 tiles between
GridAStar and Diagonal** — which is why picking between them by eye, or by one recorded walk,
cannot work. They are kept for the throughput comparison in `performance.md` and for the
selectable-arm machinery; new work belongs on the measured arm.
