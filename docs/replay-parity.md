# Replay parity: the engine against recorded battles

Measured 2026-09-21 on the whole corpus.

The engine's claim is that it reproduces the game. This file is the measurement of that claim
over every recorded battle the repository has. The script of what a player did is played through
the engine, and the engine's per-tick state is scored against what the game then showed. It is a
property of the engine at this commit, not a record of a work pass; re-run the commands below and
the tables come back.

## 1. What is scored

A **fixture** (`tools/make_replay_fixture.py`) splits one recorded battle, client 16.402, into

- a **script**: the decks, the tower levels and starting hitpoints, the card levels, and one
  deploy per group of a card's own units, at the tick they came to exist, plus the spell casts;
- a **truth**: per frame tick, per entity, the columns the harness scores (position, hitpoints,
  alive, target, path-node count, behaviour state).

`crates/royalesim/examples/replay_parity.rs` plays the script and pairs the engine's entities
with the truth's, then counts **unit-ticks**. A unit-tick is one matched pair (or one unmatched
entity) on one truth frame tick where at least one side has the unit alive. A position column (`<=250`, `<=500`,
`<=1000`) is the both-alive unit-ticks whose position error is within that many native units
(a tile is 1000). `<=250 moving` drops the frames on which both sides are still standing on the
spawn point, where a position match says nothing about movement. `walk <=250` / `walk <=20` are
the isolated-walk subset (truth walking at a tower or at nothing, full hitpoints). `walk <=20`
is the bit-exact walk: a unit whose slowest step is 37 native per tick is either on the game's
own integer path or it is not.

Towers are scored too and they are easy: they do not move. Every headline number below is the
**no-towers** row, which is the one that says whether the engine plays the game.

## 2. The corpus and the run

| | |
|---|---|
| Client | Clash Royale 16.402 |
| Recordings | 73 whole battles, one frame per 50 ms tick, both sides (not distributed) |
| Card table | `data/derived/cards.json`, FNV-1a 64 `5a1dac3d2fb1b4a9` |
| Engine card census | 97 loadable, 49 rejected, 9 summon-only, against the same card table |
| Fixtures | 73, one per recording (a recording carrying more than one name contributes once) |
| Whole battles playable | 25 |
| Played as a prefix | 42 — the battle runs to its first deploy of a card the loader refuses |
| Not played at all | 6 — every one of them a recording that begins after its battle began |

```
# 1. the engine's own card list, which decides a fixture's playability
cd crates/royalesim && cargo run --release --example replay_parity -- --census

# 2. the fixtures, rebuilt against that card list and that card table
ROYALELIVE_REPORTS=<the recordings folder> python tools/make_replay_fixture.py --all

# 3. the corpus run: 25 whole battles and 42 prefixes
cd crates/royalesim && cargo run --release --example replay_parity -- --all --prefix

# the whole-battle-only run: the 25, no prefixes
cd crates/royalesim && cargo run --release --example replay_parity -- --all
```

The fixtures and the census must be built against the card table the engine loads. A fixture
built against another one classifies a unit as a spawn that the engine deploys, and the run
scores the difference as the engine's error. Both carry the table's hash and both the manifest
and the per-fixture report say when they disagree. The run below carries no such note.

## 3. The aggregate

**25 whole battles and 42 prefixes, 67 battles.** 219491 no-tower unit-ticks; the walk columns
are over 122770 isolated-walk unit-ticks.

| | unit-ticks | <=250 | <=250 moving | <=500 | <=1000 | hp exact | target | path n | alive/missing/extra | walk <=250 | walk <=20 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| ALL | 700138 | 83.9% | 83.9% | 88.2% | 92.2% | 83.3% | 17.5% | 83.8% | 3.3% | 52.5% | 31.0% |
| ALL but towers | 219491 | 49.4% | 46.9% | 62.9% | 75.7% | 81.4% | 38.6% | 48.9% | 10.0% | 52.5% | 31.0% |

**The 25 whole battles alone**, no prefixes. 131960 no-tower unit-ticks; 73087 isolated-walk
unit-ticks.

| | unit-ticks | <=250 | <=250 moving | <=500 | <=1000 | hp exact | target | path n | alive/missing/extra | walk <=250 | walk <=20 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| ALL | 465112 | 85.6% | 85.6% | 88.9% | 92.3% | 80.9% | 17.9% | 85.2% | 3.3% | 57.6% | 32.0% |
| ALL but towers | 131960 | 50.4% | 48.1% | 62.1% | 74.1% | 80.1% | 42.0% | 49.0% | 10.8% | 57.6% | 32.0% |

The two modes agree to about a point. The prefixes are the harder half of the corpus (they carry
the decks with the cards the loader refuses), but they are also the shorter half. Cutting them
out moves the no-tower position score from 49.4 % to 50.4 % and the bit-exact walk from 31.0 % to
32.0 %. Nothing in the comparison depends on which mode is read.

Counts behind the prefix-mode no-tower row, for anyone recomputing: 108612 of 219491 within 250;
96649 of 205640 moving; 138277 within 500; 166292 within 1000; 178822 hitpoints exact; 84767
target matches; 107539 path-node matches; 20832 alive mismatches and 1287 truth entities the
engine never produced, against 0 the engine produced and the game did not; 64515 of 122770 walk
unit-ticks within 250 and 38089 within 20. 6244 of the 219491 fall after the engine had already
declared the battle over and its state stopped moving.

Three of these numbers are quoted in the engine's own history as the state this tree reached:
49.5 % of no-tower unit-ticks within 250 native, 81.4 % hitpoints exact, 31.1 % of the isolated
walk bit-exact. Measured here, on the corpus rebuilt against this card table: 49.4 %, 81.4 %,
31.0 %.

## 4. Where the corpus is hardest

The cards carrying the most unit-ticks, prefix mode, no towers:

| card | unit-ticks | <=250 | <=1000 | hp exact | alive/missing/extra | walk <=20 |
|---|---|---|---|---|---|---|
| Goblins | 39785 | 42.5% | 77.0% | 87.5% | 8.9% | 18.4% |
| Skeletons | 37662 | 43.9% | 63.8% | 86.4% | 13.5% | 30.3% |
| Tombstone | 37458 | 20.7% | 60.0% | 76.9% | 18.6% | 0.0% |
| SkeletonArmy | 18301 | 49.8% | 83.2% | 90.0% | 9.9% | 16.3% |
| Knight | 13383 | 82.5% | 93.2% | 80.4% | 2.0% | 74.9% |
| Giant | 12088 | 68.6% | 87.4% | 74.5% | 3.1% | 72.4% |
| MinionHorde | 9738 | 45.5% | 78.2% | 88.3% | 6.9% | 33.0% |
| GoblinGang | 7424 | 49.4% | 72.1% | 84.3% | 11.8% | 40.2% |

The shape is the whole result in miniature. A single unit walking alone is close to solved:
Knight 82.5 % within 250 and 74.9 % of its isolated walk bit-exact, Musketeer 81.6 %, Prince
73.8 %, Wallbreakers and Hog Rider effectively exact. A **crowd** is not: five cheap swarm cards
carry more than half the corpus' unit-ticks between them and none reaches 50 % within 250. The
Tombstone, at 20.7 %, is the extreme case. It is a building that is itself stationary and
correct, scored through the Skeletons it emits, whose spawn points and spawn ticks the engine
does not yet place where the game places them.

## 5. First divergence, by cause

Each battle's **first divergence** is its first unit-tick past 1000 native or its first alive
mismatch. The harness reads a cause at the onset, the first tick that unit's error passed 250.
One cause per battle, so the table below ranks what goes wrong *first*, not what costs most
in total; the unit-tick columns say how much of the corpus sits behind battles that begin that
way. 25 of the 67 battles never diverge at all (short prefixes, most of them).

| cause | battles | no-tower unit-ticks | of them beyond 250 | share of the corpus' missed unit-ticks | within 250 |
|---|---|---|---|---|---|
| spawn | 10 | 67039 | 36047 | 32.5% | 46.2% |
| contact | 13 | 62166 | 34381 | 31.0% | 44.7% |
| death | 11 | 56541 | 24234 | 21.9% | 57.1% |
| attack-timing | 6 | 28057 | 15386 | 13.9% | 45.2% |
| walking | 2 | 1898 | 442 | 0.4% | 76.7% |
| (no divergence) | 25 | 3790 | 389 | 0.4% | 89.7% |

Read together with section 4:

- **spawn** (where and when a summoned or emitted unit comes into being) heads the fewest
  battles of the top three but sits under the most unit-ticks, because the battles it heads are
  the long ones full of swarm and spawner cards.
- **contact** (where a unit stops when something is already standing where it is going) heads
  the most battles. It is the law the engine models least, and it is what turns a formation that
  landed correctly into a crowd standing in the wrong places a second later.
- **death** (the tick a unit dies on) is third by battles and third by cost, and it is the
  cheapest of the three per battle: those battles still score 57.1 % within 250.
- **walking**, the search and the step law, heads 2 battles of 67 and 0.4 % of the missed
  unit-ticks. The path is not the problem any more. What happens when a unit arrives is.

## 5a. One divergence that is known and deliberate

Before reading a knockback divergence as a new defect: the engine holds a unit's target and its
planned route for the whole of a knockback ladder, and the corpus says the game does not. A
Golem in capture 20260920-071744-B retargets and replans mid-ladder, six steps before the
back-step. This is recorded in `knockback.DISPLACEMENT_LAW`'s open items and in
[`mechanics.md`](mechanics.md). It is a known gap rather than a surprise, and a run that meets
it has found the thing that is already on the list.

## 6. What this does not say

- Pairing is by group size and time, then by least total distance between members. A formation
  the engine lays out in the right shape but the wrong orientation can pair member to member and
  score as 8 wrong positions rather than 1 wrong rotation.
- A prefix stops at the first deploy the engine cannot load. The engine is therefore not scored
  on the decks that use those cards, and those decks are not a random sample: they are the ones
  with the mechanics the engine has not modelled.
- Position is scored in native units against a recording that misses a frame here and there; a
  unit-tick on a missed frame is not scored at all, not scored as a match.
- `target` is the one column where the towers drag the average down rather than up (17.5 % all,
  38.6 % no towers). A tower with several units in range picks among them by a rule the engine
  does not yet reproduce, and the column counts the pick, not the damage.
