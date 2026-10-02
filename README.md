# RoyaleSim

[![suite](https://github.com/RoyaleGym/RoyaleSim/actions/workflows/suite.yml/badge.svg)](https://github.com/RoyaleGym/RoyaleSim/actions/workflows/suite.yml) [![license: MIT](https://img.shields.io/badge/license-MIT-555)](LICENSE)

The Clash Royale battle engine your bot plays in. It runs the whole match on your own machine.
Most people use it through [RoyaleGym](https://github.com/RoyaleGym/RoyaleGym) and never call it directly.

<p align="center"><img src="docs/media/battle-page.gif" width="100%" alt="An engine battle in the RoyaleViser viewer, 16 units on the board"></p>

## Install

```bash
pip install "royalegym[all]"
```

That installs this engine and everything else. Until the packages are on PyPI, the wheels are on the
[Releases page](https://github.com/RoyaleGym/RoyaleSim/releases).

## Try it

```python
import json
import royalesim

b = royalesim.Battle(card_names=None, slot_of_k=[[0, 1, 2], [0, 1, 2]])
ids = {row[0]: i for i, row in enumerate(json.loads(b.catalogue_json()))}
deck = [ids[n] for n in ["Knight", "Archer", "Goblins", "Giant", "Musketeer", "Fireball", "Arrows", "Skeletons"]]
b.reset(seed=0, decks=[deck, deck], shuffle=1, start_tick=0, elixir_milli=[5000, 5000], tower_hp=None, spawns=[])
b.step([], 100)                                          # nobody can play in the opening seconds
r = b.step([(0, 0, 9000 * 18, 10000 * 18)], 20)         # blue plays its first card, then 20 ticks pass
print(json.loads(b.state_json())["tick"], royalesim.DEPLOY_REASONS[r[0][1]])   # 120 OK
```

## Next

- The engine's Python API: [docs/api.md](docs/api.md)
- How it works inside, how fast and how accurate it is, building from source: [docs/engine.md](docs/engine.md)
- Questions: [Discord](https://discord.gg/4D2BS5JBHP)

MIT License.
