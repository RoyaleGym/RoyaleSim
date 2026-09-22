# Measured performance

This page is for contributors who need to know how fast the engine runs. It collects the timing
figures this repository has measured, with the conditions each one was measured under.

All figures below were measured on one machine, single core, release build with overflow checks
on. **Every one of them predates the current engine**: the engine-throughput tables are from
**2026-09-13**, before the 16.402 search and contact law landed, and the Python-surface figures
from **2026-09-21**, before the 15.535 card table, the status effects and the measured tick
order. They are the numbers to beat, not a promise, and each section says which engine it
describes. Re-measure with:

```
cd crates\royalesim && cargo test --release --test throughput -- --ignored --nocapture
```

## Engine throughput

| Scenario | Rate | Conditions |
|---|---|---|
| Scripted battle with spells | **112,022 ticks/s** | 4800 ticks in 42 ms, mean 8 live entities |
| Scripted battle | **70,095 ticks/s** | the same battle without spells, mean 14 live |
| Full 6-minute scripted battle | **57,966 ticks/s** | 7200 ticks in 124 ms, mean 15 live, peak 29 |
| Brawl, `DiagonalLookahead` | 51,204 / 26,567 ticks/s | mean 22 / 38 live |
| Brawl, `LaneSnap` | 34,810 / 24,523 ticks/s | mean 22 / 37 live |
| Brawl, `GridAStar` | 30,102 / 16,069 ticks/s | mean 22 / 37 live |
| Cost per entity-tick | ~0.9-1.6 us | all of the above |
| Save / load | ~140 us / ~150 us, ~7 KB | 23 live entities |

About 1 us per entity-tick is slow for integer Rust and nobody has profiled it. It is not a
bottleneck: at 20 ticks per second a 3-minute match is 3,600 ticks, which is ~60 ms of engine
time.

## Through the Python surface, 2026-09-21 (before the mechanics work below it landed)

`tools/throughput.py` runs five three-minute battles with both seats deploying at random (18,000
ticks, 20 ticks per `step`, mean ~10 live entities) and prints the rate. On 2026-09-21, on one core
with the selected 16.402 search and contact law, four consecutive runs landed at **54,870-59,898
ticks/s** with state decoded every step (18,000 ticks in 0.31 s) and 63,025 ticks/s with state
never decoded (`--read-every 0`). The spread between runs on the same machine is wider than the
difference between decoding every step and never, so treat the figure your own run prints as the
one that matters. A 3-minute match (3,600 ticks) is therefore some 60 ms of engine time at that
density.

## Through the Python layer (2026-09-13)

| Scenario | Rate | Conditions |
|---|---|---|
| `env.step()`, full 18-card thin slice on `RustEngine` | **890/s** | masked random both seats, 600 steps, 500 ms decisions, observations, mask and reward included |
| `env.step()` through the full Python stack | Rust **1,012/s** vs mock 837/s | 10 live entities, 10 ticks per step |
| Engine ticks/s through Python, step only | Rust 111,326 vs mock 72,450 | 6-12 live |

The Python observation builder is the bottleneck, not the engine: it runs at roughly 1.5k/s for
both seats, which is why the two engines' `env.step()` rates are so much closer than their raw
tick rates. Anything on the per-tick path in Python costs two orders of magnitude.

## A whole battle, end to end

`tools/watch_battle.py` plays a 3-minute battle, scores its five gates and writes a
self-contained page in about **5 s** (2026-09-13), including the full re-simulation and a 1.4 MB
page. On 2026-09-21 the same run (`--seed 7 --steps 400 --noop-prob 0.2`) took **1.4 s** end to
end, page included (2.8 MB, 4001 frames; 119 troops, 335 hp drops, crowns 1-1 at tick 4000).

Over seeds 1-5 (`--steps 800 --noop-prob 0.2`, both seats uniform-over-legal-actions): **Blue 2 /
Red 2 / Draw 1**, crowns `[0,0] [1,0] [0,1] [0,1] [3,0]`, final ticks 4800 (a draw that went to
overtime), 3600, 3738, 3600 and 3280 (a 3-crown instant win). Every gate green on all five.

That is a smoke policy. It says nothing about balance, and nothing about whether any of it
resembles Clash Royale. That is what the recorded traces are for.
