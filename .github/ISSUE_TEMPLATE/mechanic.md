---
name: Differs from the game
about: The engine plays a mechanic differently from the real game
labels: mechanic
---

**What the game does**
What you saw, with evidence: a video or replay with timestamps, the client version, and the card levels.

**What the engine does**
A few lines of Python that show it, with the seed.

**The rule, if you know it**
Its ledger key in `data/calibration.json` (for example `targeting.EQUAL_DISTANCE_TIE`), or the page of
`docs/mechanics.md` it is on.

**Where you checked**
The situations you saw it in, and any where the game did something else.

**Your engine**
Paste the output of:

    python -c "import royalesim, sys; print(royalesim.__version__, royalesim.Battle.provenance(), sys.version)"
