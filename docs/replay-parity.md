# Replay parity: the engine against recorded battles

Measured 2026-09-22 at 18:40 PDT on the whole corpus, against the engine of that day: tile
footprints for buildings, 102 loadable cards and 6 starting elixir. An earlier run on
2026-09-21 is in the history note at the end; do not read the two as one series.

This page is for contributors who want to know how close the engine is to the game. The engine's
claim is that it reproduces the game, and this file is the measurement of that claim over every
recorded battle the repository has. The harness plays the script of what a player did through the
engine, then scores the engine's per-tick state against what the game then showed. It is a
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
| Card table | `data/derived/cards.json`, FNV-1a 64 `5a1dac3d2fb1b4a9`, the **15.535.29 LIVE** tables. The NAME does not carry the vintage (`data/derived/` is gitignored and `extract_cards.py` writes whichever vintage it was asked for to that one name), so the hash is what pins it: `cards-2018.json` is `2c4978693f313a1a`. |
| Engine card census | 102 loadable, 44 rejected, 12 summon-only, against the same card table |
| Fixtures | 73, one per recording (a recording carrying more than one name contributes once) |
| Whole battles playable | 27 |
| Played as a prefix | 40. The battle runs to its first deploy of a card the loader refuses |
| Not played at all | 6. Every one of them is a recording that begins after its battle began |

Four steps, in Windows PowerShell. First the engine's own card list, which is what decides a
fixture's playability. Then the fixtures, rebuilt against that card list and that card table.
Then the corpus run, whole battles and prefixes together. The last line is the whole-battle-only
run, which is the smaller population and the one to quote when a figure is about complete
battles.

```
cd crates\royalesim
cargo run --release --example replay_parity -- --census
cd ..\..
$env:ROYALELIVE_REPORTS = "<the recordings folder>"
python tools\make_replay_fixture.py --all
cd crates\royalesim
cargo run --release --example replay_parity -- --all --prefix
cargo run --release --example replay_parity -- --all
```

The fixtures and the census must be built against the card table the engine loads. A fixture
built against another one classifies a unit as a spawn that the engine deploys, and the run
scores the difference as the engine's error. Both carry the table's hash, and the manifest and
the per-fixture report each say when they disagree. The run below carries no such note.

## 3. The aggregate

**27 whole battles and 40 prefixes, 67 battles.** 271384 no-tower unit-ticks; the walk
columns are over 141002 isolated-walk unit-ticks.

| | unit-ticks | <=250 | <=250 moving | <=500 | <=1000 | hp exact | target | path n | alive/missing/extra | walk <=250 | walk <=20 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| ALL | 800243 | 82.6% | 82.6% | 87.0% | 91.2% | 81.7% | 18.0% | 82.6% | 3.9% | 51.9% | 30.7% |
| ALL but towers | 271384 | 49.4% | 47.2% | 62.4% | 74.8% | 79.4% | 37.0% | 49.4% | 11.0% | 51.9% | 30.7% |

**49.4 % IS NOT UNCHANGED SINCE 2026-09-21, it is the same number over a different
population.** That run scored 219491 no-tower unit-ticks and this one scores 271384,
23 % more, because more cards load so more units stand on the board. A score over a bigger and
harder population that lands on the same figure is not a result that stayed still, and the two
should not be subtracted from each other.

**The 27 whole battles alone**, no prefixes. 167980 no-tower unit-ticks; 88243 isolated-walk
unit-ticks.

| | unit-ticks | <=250 | <=250 moving | <=500 | <=1000 | hp exact | target | path n | alive/missing/extra | walk <=250 | walk <=20 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| ALL | 539982 | 84.1% | 84.0% | 87.3% | 91.1% | 79.1% | 19.1% | 83.6% | 4.1% | 59.1% | 33.4% |
| ALL but towers | 167980 | 49.7% | 47.4% | 60.0% | 72.3% | 76.6% | 41.5% | 48.2% | 12.5% | 59.1% | 33.4%

The two modes agree to about a point. The prefixes are the harder half of the corpus (they carry
the decks with the cards the loader refuses), but they are also the shorter half. Cutting them
out moves the no-tower position score from 49.4 % to 49.7 % and the bit-exact walk from 30.7 % to
33.4 %. Nothing in the comparison depends on which mode is read.

Counts behind the prefix-mode no-tower row, for anyone recomputing: 134331 of 271384 within 250;
169366 within 500; 203158 within 1000; 215520 hitpoints exact; 100634
target matches; 134301 path-node matches; 28601 alive mismatches and 1287 truth
entities the engine never produced, against 83 the engine produced and the game did not;
73318 of 141002 walk unit-ticks within 250 and 43334 within 20. 6284 of the
271384 fall after the engine had already declared the battle over and its state stopped moving.

Three of these numbers are quoted in the engine's own history as the state this tree reached:
49.5 % of no-tower unit-ticks within 250 native, 81.4 % hitpoints exact, 31.1 % of the isolated
walk bit-exact. Measured here, on the corpus rebuilt against card table `5a1dac3d2fb1b4a9`
(named rather than "this", because the filename does not carry a vintage): 49.4 %, 81.4 %, 31.0 %.

## 4. Where the corpus is hardest

The cards carrying the most unit-ticks, prefix mode, no towers:

| card | unit-ticks | <=250 | <=1000 | hp exact | alive/missing/extra | walk <=20 |
|---|---|---|---|---|---|---|
| Tombstone | 53875 | 20.0% | 60.4% | 77.2% | 18.3% | 0.0% |
| Goblins | 44808 | 40.3% | 74.9% | 86.0% | 10.0% | 17.2% |
| Skeletons | 41532 | 45.8% | 65.9% | 86.3% | 13.7% | 31.3% |
| SkeletonArmy | 18903 | 55.7% | 81.2% | 87.6% | 12.4% | 15.6% |
| Giant | 17604 | 64.5% | 84.8% | 68.9% | 5.6% | 70.7% |
| Knight | 16913 | 73.1% | 84.6% | 74.2% | 3.6% | 73.3% |
| MinionHorde | 9658 | 46.0% | 81.1% | 89.2% | 5.2% | 33.0% |
| GoblinGang | 7295 | 63.5% | 76.6% | 86.7% | 10.1% | 61.5% |

The shape is the whole result in miniature. A single unit walking alone is close to solved:
Wall Breakers are exact, the Hog Rider is 90.1 % within 250 with 99.2 % of its isolated walk
bit-exact, and the Prince, the Knight and the Musketeer sit between 71 % and 74 %. A **crowd** is
not: five cheap swarm cards carry 63 % of the no-tower unit-ticks between them, and four of the
five are under 50 % within 250. The Tombstone, at 20.0 %, is the extreme case. It is a building
that is itself stationary and correct, scored through the Skeletons it emits, whose spawn points
and spawn ticks the engine does not yet place where the game places them.

Two things moved against the 2026-09-21 run and are recorded rather than explained: the single
walkers came DOWN (the Knight was 82.5 % and is 73.1 %, the Musketeer 81.6 % and now 71.9 %), and
the Skeleton Army came UP through 50 %, at 55.7 %, so "none of the swarms reaches 50 %" is no
longer true. The corpus is 23 % larger and its composition changed with it, so neither figure is
a like-for-like delta on the same battles.

## 5. First divergence, by cause

Each battle's **first divergence** is its first unit-tick past 1000 native or its first alive
mismatch. The harness reads a cause at the onset, the first tick that unit's error passed 250.
One cause per battle, so the table below ranks what goes wrong *first*, not what costs most
in total. The unit-tick columns say how much of the corpus sits behind battles that begin that
way. 19 of the 67 battles never diverge at all (short prefixes, most of them).

| cause | battles | no-tower unit-ticks | of them beyond 250 | share of the corpus' missed unit-ticks | within 250 |
|---|---|---|---|---|---|
| spawn | 10 | 73583 | 40144 | 29.3% | 45.4% |
| contact | 14 | 65289 | 35014 | 25.5% | 46.4% |
| death | 14 | 74186 | 32519 | 23.7% | 56.2% |
| attack-timing | 10 | 56348 | 29427 | 21.5% | 47.8% |

Read together with section 4:

- **spawn** (where and when a summoned or emitted unit comes into being) heads the fewest
  battles of the top three but the largest share of the missed unit-ticks, because the battles
  it heads are the long ones full of swarm and spawner cards.
- **contact** (where a unit stops when something is already standing where it is going) heads
  the most battles alongside death. It is the law the engine models least, and it is what turns
  a formation that landed correctly into a crowd standing in the wrong places a second later.
- **death** (the tick a unit dies on) sits under the most unit-ticks of any cause but is the
  cheapest of them per tick: those battles still score 56.2 % within 250.
- **attack-timing** has grown into a peer of the other three rather than a distant fourth,
  which is the clearest change since the 2026-09-21 run.
- **walking**, the search and the step law, heads NO battle in this run. In the previous one it
  headed 2 of 67 and 0.4 % of the missed unit-ticks. The path is not the problem. What happens
  when a unit arrives is.

The battle counts come from the run's own cause table; the unit-tick columns are aggregated from
its per-battle table, because the harness does not write them itself. Both are this run.

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
- `target` is the one column where the towers drag the average down rather than up (18.1 % all,
  37.1 % no towers, from the 2026-09-23 corpus run at build_digest d872d792711934c2, 67 of
  73 fixtures played). A tower with several units in range picks among them by a rule the engine
  does not yet reproduce, and the column counts the pick, not the damage.
