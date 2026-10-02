"""A flight spell's row carries the ticks it has been moving (SPELL_FIELDS "ticks_flown", spell.rs `Spell::flown`).

An observer that shows an enemy spell's landing point only after k ticks of flight needs the count from the spell's
first moving tick, not from when it first saw it. Pinned: a Rocket's rows count up by one on each tick it flies,
from its first moving tick.
"""

import json

import royalesim

SUB = royalesim.SUBTILE_PER_MILLITILE
F = {name: i for i, name in enumerate(royalesim.SPELL_FIELDS)}


def test_a_flight_counts_its_ticks_in_the_air():
    assert "ticks_flown" in F, royalesim.SPELL_FIELDS
    b = royalesim.Battle(["Rocket", "Knight"], [[0, 1, 2], [0, 1, 2]])
    b.reset(0, [[0] * 8, [0] * 8], 0, 200, [100_000, 100_000], None, [])
    counts = []
    for t in range(1, 80):
        b.step([(0, 0, 9000 * SUB, 26000 * SUB)] if t == 1 else [], 1)
        rows = [r for r in json.loads(b.state_json())["spells"] if r[F["motion"]] == 0]
        if not rows:
            if counts:
                break
            continue
        counts.append(rows[0][F["ticks_flown"]])
    assert len(counts) >= 5, f"the Rocket flew {len(counts)} ticks: {counts}"
    assert counts == list(range(counts[0], counts[0] + len(counts))), f"not one more a tick: {counts}"
    assert counts[0] in (0, 1), f"the count starts at the first moving tick: {counts}"
