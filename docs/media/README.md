# README media

Every file the README embeds. A *placeholder* is a generated SVG whose caption says what the
real image or screen recording must show; replace the file, keep the name.

| File | Kind | Shows / must show |
|---|---|---|
| `battle-page.svg` | video placeholder | A full battle on the watch_battle page - tools/watch_battle.py --seed 7 --open: the page scrubbing through a 3-minute engine battle, both hands, both elixir bars, every unit and tower hp, the five gates green |
| `cards-and-spells.svg` | video placeholder | 65 cards, towers, spells, overtime - a Fireball landing on a crowd beside a princess tower, the tower's hp bar dropping, then the clock entering overtime and a 3-crown finish |
| `contact-law.svg` | video placeholder | Crowds that push like the real game - recording and engine side by side, 20 ticks: Skeletons dropped onto a Knight spread apart at 150 native units a tick, only the lighter unit moves |
| `deploy-legality.svg` | image placeholder | Deploy legality as a query - the arena coloured by check_deploy's answer for a Giant at every tile: OK, WATER, NO_DEPLOY, OUT_OF_TERRITORY, with one enemy princess tower down |
| `determinism.svg` | image placeholder | Same seed, same battle - two runs of one seed on two machines, the per-tick state hash column identical to the last tick; the determinism gate's OK line |
| `engine-battle-viewer.png` | real still | already final |
| `family.svg` | diagram | the five repos and how they depend on each other; final |
| `ledger.svg` | image placeholder | Every number says how it is known - one entry of data/calibration.json: value, status measured, confidence HIGH, and the provenance naming the client version and the recording |
| `measured-routes.svg` | image placeholder | Routes measured off real battles - one deploy, two routes on one board: the recorded 16.402 route and the engine's, node for node, plus the 751 of 752 score |
| `snapshots.svg` | image placeholder | Save a battle, branch it - one snapshot from b.save() loaded into ten engines, each stepped with a different play; ten boards and ten state hashes from one starting point |
| `throughput.svg` | image placeholder | Tens of thousands of ticks a second - tools/throughput.py output: five 3-minute battles, 18,000 ticks, 0.3 s on one core, ticks/s shown |
