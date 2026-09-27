# Which cards to build next, ranked by how often they actually appear

**Status at RoyaleSim `126992a`:** the Barbarian Barrel, Lightning and the Royal Ghost load. Miner,
Graveyard and the action-graph reader remain. The loader's census at that commit refuses 36 of the
144 cards, and 18 of those refusals name an action graph the loader does not read.

Written 2026-09-23 from a second user's measurement of their own opponent pool, which is the
first time this backlog has been ordered by anything other than what looked interesting.

## The measurement

The deck counts and the 24% in this file are one user's report of 2026-09-23, against their own
held-out pool of 293 opponent decks. This project did not measure them. The refusal counts come
from the loader's own census.

A user running RoyaleSim against held-out real matchups reported: of 293 held-out opponent
decks, the cards that stop a deck loading are **Barbarian Barrel in 93**, **Lightning in 40**,
**Royal Ghost in 33**, **Miner in 30** and **Graveyard in 27**. With Tornado implemented and
nothing else, **24% of their held-out matchups load**.

That last figure is the one that matters. A card's cost is not how hard it is to build, it is
how many battles it keeps you out of, and until this measurement arrived nobody here had that
number for any card.

## What each one actually needs

Rejection reasons are the loader's own, from `data/derived/replay/card_census.json`. They are
specific, which is the useful part: none of these is "hard", each is a named mechanic.

| card | decks blocked | what the loader says it needs |
|---|---:|---|
| Barbarian Barrel | 93 | **Loads at RoyaleSim `126992a`.** It needed a rolling projectile that carries targets, spawns and buffs at once (`BarbLogProjectileRolling`): the Log's roll plus a spawn on stop. |
| Lightning | 40 | **Loads at RoyaleSim `126992a`.** It needed a pulsing area effect whose mechanic is not in the buff columns at all: it picks N highest-hitpoint targets, which no column expresses. |
| Royal Ghost | 33 | **Loads at RoyaleSim `126992a`.** It was not in the rejected list under that name; it is the hide/reveal mechanic. |
| Miner | 30 | spawns at the caster's own king tower and travels underground to the tap (`SpawnPathfindSpeed 650`). A second locomotion mode, not a combat rule. |
| Graveyard | 27 | an action graph (`ActionGroup`, `ActionSpawnToLocation`) that spawns skeletons at intervals across an area. |

At RoyaleSim `126992a`, 36 of 144 cards are refused, and by rejection reason **18 of them need an
action graph the loader does not read** (the Rage Barbarian's is in its death area effect). That is
one piece of work, not eighteen.

## The order this suggests, and why it is not simply the frequency order

1. **Barbarian Barrel.** Done: it loads at RoyaleSim `126992a`. Three times the next card's
   frequency, and the closest to something that existed: the Log's rolling projectile.
2. **The action-graph reader.** Graveyard is 27 decks on its own, and at RoyaleSim `126992a` 17
   other refused cards name the same reader. Ordered above Lightning and Miner despite a lower
   single-card count, because it is the only item here whose value is not capped by its own
   frequency.
3. **Miner.** Self-contained: one extra locomotion mode with a speed already in the data.
4. **Lightning.** Done: it loads at RoyaleSim `126992a`. It needed a targeting rule (N
   highest-hitpoint victims) that no existing column carries, so it was a new mechanic rather
   than a new card.
5. **Royal Ghost.** Done: it loads at RoyaleSim `126992a`.

## The honest caveat about the 24%

The 24% is from that same user report of 2026-09-23, over their 293 held-out decks.

That figure is **their** deck pool, not a universal one. A different ladder bracket, a different
region or a different month gives different frequencies, and the ranking above inherits that.
What does not change is the shape of the argument: order the backlog by decks blocked, not by
interest, and prefer the one item whose value is not capped by a single card's frequency.

If you are running RoyaleSim against real matchups and your blocked-deck counts differ from
these, that is worth sending — two pools disagreeing is more informative than one pool measured
twice.
