# RoyaleSim

<p align="center"><a href="https://github.com/RoyaleGym/RoyaleSim/actions/workflows/suite.yml"><img alt="CI" src="https://github.com/RoyaleGym/RoyaleSim/actions/workflows/suite.yml/badge.svg"></a> <img alt="License" src="https://img.shields.io/github/license/RoyaleGym/RoyaleSim?style=flat-square&color=555"> <img alt="Python" src="https://img.shields.io/badge/python-3.10+-3776AB?style=flat-square&logo=python&logoColor=white"> <a href="https://royalegym.github.io/RoyaleGym/"><img alt="Docs" src="https://img.shields.io/badge/docs-royalegym.github.io-8957e5?style=flat-square&logo=readthedocs&logoColor=white"></a> <a href="https://discord.gg/4D2BS5JBHP"><img alt="Discord" src="https://img.shields.io/discord/1551699576304705647?style=flat-square&logo=discord&logoColor=white&label=discord&color=5865F2"></a> <img alt="Last commit" src="https://img.shields.io/github/last-commit/RoyaleGym/RoyaleSim?style=flat-square&color=555"></p>

<p align="center"><img alt="Engine: Rust, whole numbers only" src="https://img.shields.io/badge/engine-Rust%2C%20whole%20numbers%20only-DEA584?style=flat-square&logo=rust&logoColor=white"> <img alt="Tick: 50 ms, 20 per second" src="https://img.shields.io/badge/tick-50%20ms%2C%2020%20per%20second-555?style=flat-square"> <img alt="Coordinates: 18,000 units to one tile" src="https://img.shields.io/badge/coordinates-18%2C000%20per%20tile-555?style=flat-square"></p>

The Clash Royale battle engine your bot plays in. It runs the whole match on your own machine.
Most people use it through [RoyaleGym](https://github.com/RoyaleGym/RoyaleGym) and never call it directly.

<p align="center"><img src="docs/media/battle-page.gif" width="100%" alt="An engine battle in the RoyaleViser viewer, 18 units on the board"></p>

## Install

```bash
pip install "royalegym[all]" --find-links https://github.com/RoyaleGym/RoyaleGym/releases/expanded_assets/v0.1.1
```

That installs this engine and everything else, on 3.12 or 3.13 (the engine alone runs on 3.10 and newer). [Install](https://royalegym.github.io/RoyaleGym/install/) has more.

## Try it

```python
import json, royalesim

b = royalesim.Battle(card_names=None, slot_of_k=[[0, 1, 2], [0, 1, 2]])
ids = {row[0]: i for i, row in enumerate(json.loads(b.catalogue_json()))}
deck = [ids[n] for n in ["Knight", "Archer", "Goblins", "Giant", "Musketeer", "Fireball", "Arrows", "Skeletons"]]
b.reset(seed=0, decks=[deck, deck], shuffle=1, start_tick=0, elixir_milli=[5000, 5000], tower_hp=None, spawns=[])
b.step([], 100)                                          # nobody can play in the opening seconds
r = b.step([(0, 0, 9000 * 18, 10000 * 18)], 20)         # blue plays its first card, then 20 ticks pass
print(json.loads(b.state_json())["tick"], royalesim.DEPLOY_REASONS[r[0][1]])   # 120 OK
```

## Next

- The engine's Python API: [API](https://royalegym.github.io/RoyaleGym/repos/royalesim/api/)
- How it works inside, how fast and how accurate it is, building from source: [Engine](https://royalegym.github.io/RoyaleGym/repos/royalesim/engine/)
- Questions: [Discord](https://discord.gg/4D2BS5JBHP)

MIT License.
