"""A pulsing area's first application waits its own HitSpeedOffset, and an area with none applies on its landing
tick (spells.PULSING_AREA_EFFECT = hit_speed_offset).

WHAT THIS PINS. Measured on the 16.402 corpus. L is the landing tick, the first tick an area stands on the board. C is
the tick a Rage's bottle first stands on the board; the bottle releases the Rage's area on C + 9 and it first acts on
C + 10 (spells.SUMMON_FUSE_START), so for a Rage L = C + 10.
  * Rage (HitSpeed 300, no HitSpeedOffset): an own unit's first raged step is the step into C + 11, in 5 of 5 casts
    (9 units). The area buffs on L. hit_speed_period_delayed, the client 15.535.29 law, gives C + 16.
  * Poison (HitSpeed 250, HitSpeedOffset 250): the first pulse on a Knight at the tap falls in (L+22, L+24], one cast.
    The landing arm gives L + 20. The offset arm gives L + 24, and the first slowed step L + 5.
  * Earthquake (HitSpeed 100, no HitSpeedOffset): pulses in (L+18, L+20] and (L+38, L+40], one cast on a building.
    With status.AREA_BUFF_SOURCE_BINDING at not_read the offset arm gives L + 20 and L + 40, and the delayed arm
    L + 21 and L + 41. With client_source_bound both give L + 19 and L + 39, so that arm cannot tell them apart here.
The offsets are the ledger's spells.PULSING_AREA_EFFECT.hit_speed_offset_ms (client 16.402's HitSpeedOffset).

WHAT THIS CANNOT TELL. Both listed values equal their cards' HitSpeed (Poison 250 and 250, Tornado 50 and 50), and a
calibration override replaces only a key's `value`, never hit_speed_offset_ms. So an engine that waits one HitSpeed for
every listed card, whatever value is listed, passes this file. crates/royalesim/tests/status.rs
`the_offset_arm_waits_the_listed_value_not_one_hit_speed` moves the Poison's value off its HitSpeed (100 and 500) and
tells the two apart. Its plant `pulsing_offset_listed_hit_speed` gives this file's scenes the same numbers.

WHY THE CONTROLS ARE HERE. The delayed arm must still give the client 15.535.29 Rage onset (C + 16) and the landing arm
C + 11, or the Rage test below could pass on an engine whose arms all do the same thing.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run
this file: the named tests go red and the rest stay green.
  * `pulsing_offset_by_hit_speed` -- the offset arm waits one HitSpeed, as client 15.535.29 does:
    test_a_rage_buffs_on_its_landing_tick, test_an_earthquake_pulses_on_its_landing_clock.
  * `pulsing_area_applies_on_landing` -- every arm applies on the landing tick:
    test_a_poison_waits_its_hit_speed_offset, test_the_other_arms_still_run.
"""

from __future__ import annotations

import json
import math
from itertools import pairwise

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
PULSING, BINDING = "spells.PULSING_AREA_EFFECT", "status.AREA_BUFF_SOURCE_BINDING"
OFFSET, DELAYED, FROM_LANDING = "hit_speed_offset", "hit_speed_period_delayed", "hit_speed_period_from_landing"
CAST_STEP = 10
#: the first tick a cast object stands on the board: a Poison's or an Earthquake's L, a Rage bottle's C
FIRST = CAST_STEP + 1
#: a walking Knight steps 59 native; raged 78; Poison (-15) slows it to about 50, Earthquake (-50) to about 30
RAGED, POISON_SLOW, QUAKE_SLOW = 70, 55, 45
POISON_PULSE, QUAKE_PULSE = 92, 81


def knight_track(spell: str, pulsing: str, own: bool, binding: str = "not_read", ticks: int = 70) -> list[tuple]:
    """One Knight and one blue `spell` cast on step CAST_STEP, so the cast object first stands on the board on tick
    FIRST. own: a BLUE Knight spawned at (9500, 9500) walks up through a cast at (9500, 10500) (a Rage buffs own
    troops). Otherwise a RED Knight spawned at (9500, 22500) walks down through a cast at (9500, 21500). No tower
    reaches it. Returns (tick, x, y, hp)."""
    ov = {PULSING: json.dumps(pulsing), BINDING: json.dumps(binding)}
    team, y0, tap_y = (0, 9500, 10500) if own else (1, 22500, 21500)
    b = royalesim.Battle([spell, "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=ov)
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, [(team, 1, 9500 * SUB, y0 * SUB, -1)])
    rows = []
    for t in range(ticks):
        st = json.loads(b.state_json())
        k = next(e for e in st["entities"] if e[F["team"]] == team and e[F["tower_slot"]] < 0)
        rows.append((t, k[F["x"]] / SUB, k[F["y"]] / SUB, k[F["hp"]]))
        if t in (FIRST - 1, FIRST):
            assert bool(st["spells"]) == (t == FIRST), f"the {spell} does not first stand on the board on tick {FIRST}"
        b.step([(0, 0, 9500 * SUB, tap_y * SUB)] if t == CAST_STEP else [], 1)
    return rows


def steps(track: list[tuple]) -> dict:
    """{tick - FIRST: the Knight's displacement into that tick}."""
    return {b[0] - FIRST: math.hypot(b[1] - a[1], b[2] - a[2]) for a, b in pairwise(track)}


def first_raged(track: list[tuple]) -> int:
    s = steps(track)
    return next(r for r in sorted(s) if r >= 0 and s[r] > RAGED)


def first_slowed(track: list[tuple], slow: float) -> int:
    s = steps(track)
    return next(r for r in sorted(s) if r >= 0 and s[r] <= slow)


def pulses(track: list[tuple], amount: int, until: int) -> list[int]:
    return [b[0] - FIRST for a, b in pairwise(track) if a[3] - b[3] == amount and 0 <= b[0] - FIRST <= until]


def test_the_ledger_carries_the_offsets():
    """client 16.402's HitSpeedOffset for the loadable cards whose pulsing area has one. Each equals its card's
    HitSpeed, which is why the value itself is tested in crates/royalesim/tests/status.rs (module doc)."""
    entry = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)["spells"]["PULSING_AREA_EFFECT"]
    assert OFFSET in entry["candidates"]
    assert entry["hit_speed_offset_ms"] == {"Poison": 250, "Tornado": 50}


def test_a_rage_buffs_on_its_landing_tick():
    """16.402 corpus: the first raged step is the step into C + 11 (5 of 5 casts, 9 units)."""
    got = first_raged(knight_track("Rage", OFFSET, own=True, ticks=40))
    assert got == 11, f"first raged step into C+{got}, not C+11"


def test_a_poison_waits_its_hit_speed_offset():
    """16.402 corpus: the first pulse falls in (L+22, L+24]; HitSpeedOffset 250 puts the application on L + 4."""
    track = knight_track("Poison", OFFSET, own=False)
    got = pulses(track, POISON_PULSE, 45)
    assert got, "no Poison pulse by L+45"
    assert 22 < got[0] <= 24, f"first pulse on L+{got[0]}, outside the corpus's (L+22, L+24]"
    assert got == [24, 44], f"pulses on L+{got}, not L+[24, 44]"
    assert first_slowed(track, POISON_SLOW) == 5, "the slow shows on the step after the application on L+4"


@pytest.mark.parametrize("binding", ["not_read", "client_source_bound"])
def test_an_earthquake_pulses_on_its_landing_clock(binding):
    """16.402 corpus: pulses in (L+18, L+20] and (L+38, L+40]. No HitSpeedOffset, so the area applies on L and the
    slow shows on L + 1."""
    track = knight_track("Earthquake", OFFSET, own=False, binding=binding)
    got = pulses(track, QUAKE_PULSE, 45)
    assert len(got) == 2, f"pulses on L+{got}, not two by L+45"
    assert 18 < got[0] <= 20, f"first pulse on L+{got[0]}, outside the corpus's (L+18, L+20]"
    assert 38 < got[1] <= 40, f"second pulse on L+{got[1]}, outside the corpus's (L+38, L+40]"
    assert got == ([20, 40] if binding == "not_read" else [19, 39]), f"pulses on L+{got} under {binding}"
    assert first_slowed(track, QUAKE_SLOW) == 1, "the slow shows on the step after the application on L"


def test_the_other_arms_still_run():
    """The controls: the landing arm and the client 15.535.29 arm on the same scenes."""
    assert first_raged(knight_track("Rage", FROM_LANDING, own=True, ticks=40)) == 11
    assert first_raged(knight_track("Rage", DELAYED, own=True, ticks=40)) == 16
    assert pulses(knight_track("Poison", FROM_LANDING, own=False), POISON_PULSE, 45) == [20, 40]
    assert pulses(knight_track("Poison", DELAYED, own=False), POISON_PULSE, 45) == [24, 44]
    assert pulses(knight_track("Earthquake", DELAYED, own=False), QUAKE_PULSE, 45) == [21, 41]
