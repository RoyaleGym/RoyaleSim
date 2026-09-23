"""A kill must not cost the killer its attack cycle.

WHAT THIS PINS. A unit that fires every HitSpeed keeps firing every HitSpeed when its
victims die under it. The engine used to zero the attack progress on every target change,
so a princess tower killing one-shot skeletons reloaded in 19 ticks instead of its own 16
-- a 150 ms tax on every kill, paid ONCE PER TARGET CHANGE and therefore on every shot
against a swarm. `combat.RETARGET_PROGRESS` carries the evidence.

WHY THE CONTROLS ARE HERE AND WHY THEIR NUMBERS DIFFER. Asserting "16" on the swarm alone
would pass for a build that pinned every cadence at 16. So the same file asks for 16 from a
tower whose victim never dies, and 20 from a Musketeer, whose LoadTime of 300 ms already
covered the gap and which must therefore be unchanged by this. Three scenarios, two
distinct numbers, and the swarm figure is only meaningful against the tower's own.

WHERE THE SHOOTERS STAND. Mid-field, out of every crown tower's range. The first version of
this measurement stood the shooter beside a princess tower and read the tower's 16-tick
clock interleaved with the shooter's 20 (5, 14, 2, 16, 2 ...). Nothing errored; it simply
was not measuring one shooter.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

TILE = 18_000
#: explicit, because catalogue ids are positional: making one more card loadable
#: renumbers every later id, and these scenarios are written in ids
DECK = ["Skeletons", "Giant", "Musketeer", "Archer", "Knight", "Minions", "Cannon", "Tesla"]
IDS = list(range(len(DECK)))
SK, GI, MU = 0, 1, 2
#: the blue left princess tower, and a victim 2.5 tiles in front of it
TOWER_VICTIM = (int(3.5 * TILE), int(9 * TILE))
#: mid-field: more than seven tiles from every crown tower, so a troop shooter is alone
MIDFIELD_BLUE = (9 * TILE, 14 * TILE)
MIDFIELD_RED = (9 * TILE, 18 * TILE)


def damage_events(spawns, ticks):
    """(tick, uid, 'hit'|'death') for every red unit, in tick order."""
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, spawns)
    prev, out = {}, []
    for t in range(ticks):
        red = {e[0]: e for e in json.loads(b.state_json())["entities"] if e[1] == 1 and e[4] < 0}
        for uid, e in red.items():
            if uid in prev and e[7] < prev[uid]:
                out.append((t, uid, "hit"))
        for uid in set(prev) - set(red):
            out.append((t, uid, "death"))
        prev = {uid: e[7] for uid, e in red.items()}
        b.step([], 1)
    return out


def gaps(events):
    ts = [t for t, _, _ in events]
    return [b - a for a, b in zip(ts, ts[1:])]


def test_the_princess_towers_cadence_is_its_hit_speed():
    """The control: one victim, no retarget. PrincessTower HitSpeed 800 ms = 16 ticks."""
    ev = damage_events([(1, GI, *TOWER_VICTIM, -1)], 90)
    g = gaps(ev)
    assert len(g) >= 4, f"too few hits to read a cadence: {ev}"
    assert set(g) == {16}, f"the cadence itself moved, before any retarget is involved: {g}"


def test_a_kill_does_not_cost_the_tower_a_reload():
    """The defect: against one-shot skeletons every shot is also a retarget."""
    skels = [(1, SK, int((2.5 + 0.5 * i) * TILE), int((8.5 + 0.4 * (i % 3)) * TILE), -1) for i in range(8)]
    g = gaps(damage_events(skels, 200))
    steady = g[2:]
    assert len(steady) >= 4, f"too few kills: {g}"
    assert max(steady) <= 17, (
        "the tower reloads more slowly when its target dies than when it does not. Its "
        f"cadence is 16 ticks; killing one-shot victims it took {steady}. The engine zeroes "
        "the attack progress on a target change, so the time since the last shot is thrown "
        "away and the cycle restarts (combat.RETARGET_PROGRESS)."
    )


def test_the_gap_that_spans_a_death_matches_the_gaps_that_do_not():
    """The separator, and the sharpest form of the claim.

    Skeletons given 300 hp take three shots each at 109 damage, so the tower holds its
    target across two gaps and changes it across the third. Before the fix those read
    16, 16, 19 and repeated identically for every victim: the 3 ticks were paid once per
    target change, not once per shot.
    """
    skels = [(1, SK, int((2.5 + 0.6 * i) * TILE), int((8.6 + 0.3 * (i % 2)) * TILE), 300) for i in range(4)]
    ev = damage_events(skels, 220)
    assert len(ev) >= 9, f"too few events to see a second victim: {ev}"
    held = [b - a for (a, _, ka), (b, _, kb) in zip(ev, ev[1:]) if ka == "hit" and kb == "hit"]
    crossing = [b - a for (a, _, ka), (b, _, _) in zip(ev, ev[1:]) if ka == "death"]
    assert held and crossing, f"the scenario produced no comparison: {ev}"
    assert set(held) == {16}, f"the within-victim gaps are not the cadence: {held}"
    assert set(crossing) <= {16, 17}, (
        f"only the gap that spans a target change is slow: within a victim {held}, "
        f"across a death {crossing}"
    )


def test_a_long_load_time_shooter_is_unaffected():
    """The prediction that made the mechanism testable, kept as a regression.

    A Musketeer's LoadTime is 300 ms, enough to credit back the time between its shot and
    its victim's death, so it never paid the tax and must not start paying one -- nor
    lose its own 20-tick cadence to a fix aimed at the tower.
    """
    ev = damage_events([(0, MU, *MIDFIELD_BLUE, -1), (1, GI, *MIDFIELD_RED, -1)], 140)
    g = gaps(ev)
    assert len(g) >= 4, f"too few hits: {ev}"
    assert all(19 <= x <= 21 for x in g), f"the Musketeer's own cadence moved: {g}"
