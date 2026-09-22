# README media

Every file the README embeds, and how to make it again.

Everything here is generated. The generator is
[`../../../RoyaleGym/docs/media/make_media.py`](../../../RoyaleGym/docs/media/make_media.py),
which lives in RoyaleGym because it writes into all four repos and the four are cloned side by
side. From the folder that holds the repos:

```
.venv\Scripts\python RoyaleGym\docs\media\make_media.py --list
.venv\Scripts\python RoyaleGym\docs\media\make_media.py battle-page determinism
```

It needs the engine built, `royalegym` and `royaleviser` installed, and Pillow. The videos also
need RoyaleViser's `media` extra, which supplies ffmpeg; without it the videos are skipped and
the stills are still written. Re-running it gives the same bytes: the battle is played on a
fixed seed with a named deck, and the viewer's capture freezes the two numbers on screen that
move with the wall clock.

Each video is a `.gif` and an `.mp4` of the same thing. The README points at the gif, because
GitHub plays a gif inside an `<img>` tag and will not play an mp4 there. The mp4 is the better
copy for anyone who opens the file directly.

| File | How it is made | What it shows |
|---|---|---|
| `battle-page.gif` / `.mp4` | `make_media.py battle-page` | The busiest stretch of an engine battle in RoyaleViser, found by sweeping the trace for the tick with the most units alive. One frame per engine tick. |
| `cards-and-spells.gif` / `.mp4` | `make_media.py cards-and-spells` | A spell landing on a crowd late in the same battle, found the same way: the tick with a spell in the air and the most units on the board. |
| `deploy-legality.png` | `make_media.py deploy-legality` | The arena coloured by what `check_deploy` answers for one card, before and after an enemy princess tower falls. |
| `determinism.png` | `make_media.py determinism` | One seed run more than once, with the per-tick state hashes side by side. |
| `throughput.png` | `make_media.py throughput` | A real transcript of `tools/throughput.py`, and what the rate means in whole battles an hour. |
| `snapshots.png` | `make_media.py snapshots` | One saved position loaded into several engines and stepped on differently. |
| `ledger.png` | `make_media.py ledger` | One entry of `data/calibration.json` with its status and its evidence, and the spread of statuses across the file. |
| `engine-battle-viewer.png` | a RoyaleViser screenshot | final, not regenerated here |
| `family.svg` | hand-drawn | the five repos and how they depend on each other; final |

## Still placeholders, and why

A *placeholder* is a generated SVG whose alt text begins "Image placeholder:" or "Video
placeholder:" and says what the real picture must show. Replace the file, keep the name.

| File | What it must show | Why it is not made here |
|---|---|---|
| `measured-routes.svg` | one deploy, two routes on one board: the recorded route and the engine's, node for node, plus the 743 of 744 score | It needs a recording of a real battle beside the engine. The recordings are private, so this one cannot be generated from anything in a public checkout. It needs a decision about what, if anything, may be shown. |
| `contact-law.svg` | a recording and the engine side by side, 20 ticks, Skeletons dropped onto a Knight spreading apart | Same reason. |

Both of those are the two tiles that carry the project's strongest claims, so they are worth
making properly rather than approximating. Neither can be made without deciding what a public
picture of a real recording may contain.
