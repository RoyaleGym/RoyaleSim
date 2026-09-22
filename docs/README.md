# RoyaleSim documentation

| File | What it covers |
|---|---|
| [architecture.md](architecture.md) | how the engine is built: representation, the tick, determinism, where the numbers come from, the module map |
| [contributing.md](contributing.md) | the build loop, every gate and how to run it, the test plants, the code conventions |
| [calibration.md](calibration.md) | `data/calibration.json`: what an entry means, the status vocabulary, how to change a value, which keys are still open |
| [mechanics.md](mechanics.md) | what the engine models, what it does not, the known defects and the open questions |
| [pathfinding.md](pathfinding.md) | the pathfinder and contact law measured on client 16.402: the model, the scores, the evidence |
| [pathfinder-spec.md](pathfinder-spec.md) | the implementation contract for the earlier trace-fitted model, still the contract for the grid, units and per-tick update |
| [movement-measurements.md](movement-measurements.md) | the offline 15.535 measurements the movement laws rest on, with the corpus and the method |
| [spell-spec.md](spell-spec.md) | the five spells card by card: data chain, behaviour in tick order, what is unsettled |
| [spell-spec.json](spell-spec.json) | the same content as structured data |
| [performance.md](performance.md) | measured throughput, and what the bottleneck actually is |
| `media/` | the graphics the top-level README embeds: the family diagram, one RoyaleViser still of a random-policy engine battle, and placeholders whose subtitles say what each real recording must show |

New here? [architecture.md](architecture.md) then [contributing.md](contributing.md) will get you
building and testing; [mechanics.md](mechanics.md) tells you how far to trust the result.
