"""A pulsing area effect first applies one HitSpeed after it lands, and the buff it hangs is bound to it: pulses on its
clock, capped to its life, removed with it (spells.PULSING_AREA_EFFECT = hit_speed_period_delayed and
status.AREA_BUFF_SOURCE_BINDING).

WHAT THIS PINS. Measured on client 15.535.29 (the Poison, Earthquake, Tornado and Zap sweep scenarios), with L the
landing tick (the first tick the spell stands on the board) and a Knight walking through the area:
  * Poison (HitSpeed 250): the Knight's first slowed step is L+5 and its pulses fall on L+24 and L+44. The first
    application is on L+4 = L + HitSpeed/50 - 1: the landing tick counts as the area's first 50 ms.
  * Earthquake (HitSpeed 100, buff HitTickFromSource, area CapBuffTimeToAreaEffectTime): first slowed step L+2; pulses
    on L+19, L+39 and L+59, the area's own 1000 ms marks and not a period after the buff's first application; the
    last slowed step is L+61, two ticks after the area's 3000 ms mark, not a BuffTime later.
  * Tornado (area ControlsBuff, buff ControlledByParent): ONE pulse, on L+11. The buff dies with the area, so the
    second pulse its 550 ms HitFrequency would give on L+22 never falls.
Today's engine applies every pulsing area on L, pulses a buff a period after its application, and lets the buff
outlive the area by its BuffTime.

WHY THE CONTROLS ARE HERE. The delay belongs to PULSING areas only: a Zap (one application, no HitSpeed) must still
hit and stun on L. The steps and pulses are read off the Knight, the only unit on the board, and no tower reaches it.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `pulsing_area_applies_on_landing` -- the delayed arm applies on the landing tick:
    test_poison_first_applies_one_hit_speed_after_landing.
  * `area_cap_unread` -- CapBuffTimeToAreaEffectTime unread, the buff lives BuffTime:
    test_the_earthquake_buff_runs_on_the_area_clock_and_ends_with_it.
  * `hit_tick_own_clock` -- HitTickFromSource unread, the buff pulses on its own clock:
    test_the_earthquake_buff_runs_on_the_area_clock_and_ends_with_it.
  * `controlled_buff_outlives_area` -- a ControlledByParent buff outlives its area by its BuffTime:
    test_the_tornado_buff_dies_with_its_area.
"""

from __future__ import annotations

import json
import math
from itertools import pairwise

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "status.AREA_BUFF_SOURCE_BINDING"
NEW_ARM, OLD_ARM = "client_source_bound", "not_read"
PULSING = "spells.PULSING_AREA_EFFECT"
DELAYED, FROM_LANDING = "hit_speed_period_delayed", "hit_speed_period_from_landing"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CAST_STEP = 10
LANDING = CAST_STEP + 1
#: a walking Knight steps 59 native; Poison (-15) slows it to about 50, Earthquake (-50) to about 30
POISON_SLOW, QUAKE_SLOW = 55, 45
POISON_PULSE, QUAKE_PULSE, TORNADO_PULSE, ZAP_HIT = 92, 81, 84, 192


def overrides(binding: str, pulsing: str) -> dict:
    return {KEY: json.dumps(binding), PULSING: json.dumps(pulsing)}


def knight_track(spell: str, binding: str, pulsing: str, ticks: int = 90) -> list[tuple]:
    """A red Knight spawned at (9500, 22500) walks toward the blue towers; a blue `spell` is cast at the tile centre
    (9500, 21500) on step CAST_STEP, so it first stands on the board on tick LANDING. Returns (tick, x, y, hp)."""
    b = royalesim.Battle([spell, "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(binding, pulsing))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, [(1, 1, 9500 * SUB, 22500 * SUB, -1)])
    rows = []
    for t in range(ticks):
        st = json.loads(b.state_json())
        k = next(e for e in st["entities"] if e[F["team"]] == 1 and e[F["tower_slot"]] < 0)
        rows.append((t, k[F["x"]] / SUB, k[F["y"]] / SUB, k[F["hp"]]))
        if spell != "Zap" and t in (LANDING - 1, LANDING):
            assert bool(st["spells"]) == (t == LANDING), f"the {spell} does not land on tick {LANDING}"
        b.step([(0, 0, 9500 * SUB, 21500 * SUB)] if t == CAST_STEP else [], 1)
    return rows


def steps(track: list[tuple], landing: int) -> dict:
    """{tick - landing: the Knight's displacement on that tick}."""
    return {b[0] - landing: math.hypot(b[1] - a[1], b[2] - a[2]) for a, b in pairwise(track)}


def pulses(track: list[tuple], landing: int, amount: int, until: int) -> list[int]:
    return [b[0] - landing for a, b in pairwise(track) if a[3] - b[3] == amount and 0 <= b[0] - landing <= until]


def slow_span(track: list[tuple], landing: int, slow: float) -> tuple:
    """(first, last) tick, relative to landing, of the run of slowed steps that starts after landing."""
    s = steps(track, landing)
    first = next(r for r in sorted(s) if r >= 0 and s[r] <= slow)
    last = first
    while s.get(last + 1, slow + 1) <= slow:
        last += 1
    return first, last


def assert_poison_delayed(track, landing):
    first = slow_span(track, landing, POISON_SLOW)[0]
    assert first == 5, f"first slowed step L+{first}, not L+5"
    got = pulses(track, landing, POISON_PULSE, 45)
    assert got == [24, 44], f"pulses on L+{got}, not L+[24, 44]"


def assert_earthquake_source_bound(track, landing):
    got = pulses(track, landing, QUAKE_PULSE, 80)
    assert got == [19, 39, 59], f"pulses on L+{got}, not on the area's 1000 ms marks L+[19, 39, 59]"
    span = slow_span(track, landing, QUAKE_SLOW)
    assert span == (2, 61), f"slowed steps L+{span[0]}..L+{span[1]}, not L+2..L+61"


def assert_tornado_one_pulse(track, landing):
    got = pulses(track, landing, TORNADO_PULSE, 40)
    assert got == [11], f"pulses on L+{got}, not one on L+11"


def assert_zap_on_landing(track, landing):
    s = steps(track, landing)
    assert pulses(track, landing, ZAP_HIT, 20) == [0]
    assert [r for r in range(1, 12) if s[r] == 0] == list(range(1, 11)), "held on L+1..L+10, walking on L+11"


def test_poison_first_applies_one_hit_speed_after_landing():
    assert_poison_delayed(knight_track("Poison", NEW_ARM, DELAYED), LANDING)


def test_the_earthquake_buff_runs_on_the_area_clock_and_ends_with_it():
    assert_earthquake_source_bound(knight_track("Earthquake", NEW_ARM, DELAYED), LANDING)


def test_the_tornado_buff_dies_with_its_area():
    assert_tornado_one_pulse(knight_track("Tornado", NEW_ARM, DELAYED), LANDING)


def test_a_one_shot_area_still_lands_on_its_landing_tick():
    assert_zap_on_landing(knight_track("Zap", NEW_ARM, DELAYED, ticks=30), LANDING)


def test_the_old_arms_are_todays_engine():
    """Checked on the shared build of 2026-09-25 with the new key and the unimplemented candidate dropped."""
    poison = knight_track("Poison", OLD_ARM, FROM_LANDING)
    assert slow_span(poison, LANDING, POISON_SLOW)[0] == 1
    assert pulses(poison, LANDING, POISON_PULSE, 45) == [20, 40]
    quake = knight_track("Earthquake", OLD_ARM, FROM_LANDING)
    assert pulses(quake, LANDING, QUAKE_PULSE, 80) == [20, 40, 60]
    assert slow_span(quake, LANDING, QUAKE_SLOW) == (1, 78)
    tornado = knight_track("Tornado", OLD_ARM, FROM_LANDING)
    assert pulses(tornado, LANDING, TORNADO_PULSE, 40) == [11, 22]
    assert_zap_on_landing(knight_track("Zap", OLD_ARM, FROM_LANDING, ticks=30), LANDING)
