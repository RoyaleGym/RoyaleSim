"""tools/make_spell_impact_fixture.py and what its fixture says about
calibration combat.CROWN_TOWER_DAMAGE_ROUNDING.

The fixture (crates/royalesim/tests/fixtures/spell_impacts.json) holds every spell hit on a
crown tower the live captures show. A CLEAN hit whose cast also hit an ordinary victim that
survived carries the spell's full damage D (that victim's hp step, a bracket when the victim
is a building losing its lifetime decay) beside the tower's step T. A candidate rounding
reproduces the hit at a percent p when rule(D x p) == T for some D in the bracket.

What the captures settle, and what they do not:
  - at the two percents in question, 40 (the 2018 data) and 25 (the 15.535 data), exactly
    one pair reproduces any direct-damage hit: ceil_kept_share at 25, on
    the Lightning (1057 x 25 % = 264.25, the tower lost 265). The Fireball (D 687..688, T 159)
    and the Log (D 268, T 35) match neither percent under any rule: their live percents are
    neither 40 nor 25. Which percent the engine ships is a separate decision; nothing here
    makes it.
  - at a free integer percent (CrownTowerDamagePercent is an integer column in every vintage),
    ceil_kept_share reproduces every direct-damage hit (Lightning at 25, Fireball at 23, Log at
    13); floor reproduces none of the three and round_half_up misses the Lightning and the
    Fireball.
  - hits dealt over time (Tornado) are kept in the fixture but not tested here: such a hit is
    itself a rounded product of a damage rate, and the crown share may be taken before it is.
"""

from __future__ import annotations

import importlib.util
import json
import os
import re
from collections import defaultdict

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURE = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures", "spell_impacts.json")
CALIBRATION = os.path.join(ROOT, "data", "calibration.json")
CARDS = os.path.join(ROOT, "data", "derived", "cards.json")
LIVE = os.environ.get("ROYALELIVE_REPORTS")

#: crates/royalesim/src/combat.rs damage_against, on n = damage x percent (n >= 0).
RULES = {
    "ceil_kept_share": lambda n: (n + 99) // 100,
    "floor": lambda n: n // 100,
    "round_half_up": lambda n: (n + 50) // 100,
}
FIXED_PERCENTS = (40, 25)

#: The pairs (rule, percent in FIXED_PERCENTS) that reproduce each spell's clean hits.
SURVIVE_AT_FIXED = {
    "Lightning": {("ceil_kept_share", 25)},
    "Fireball": set(),
    "Log": set(),
}
#: The integer percents 1..100 at which each rule reproduces each spell's clean hits.
SURVIVE_AT_FREE = {
    "Lightning": {"ceil_kept_share": [25], "floor": [], "round_half_up": []},
    "Fireball": {"ceil_kept_share": [23], "floor": [], "round_half_up": []},
    "Log": {"ceil_kept_share": [13], "floor": [], "round_half_up": [13]},
}
CAPTURE = re.compile(r"^\d{8}-\d{6}(?:-[A-Z])?(?:\.b\d+)?$")


def _load():
    spec = importlib.util.spec_from_file_location(
        "make_spell_impact_fixture", os.path.join(ROOT, "tools", "make_spell_impact_fixture.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


@pytest.fixture(scope="module")
def m():
    return _load()


@pytest.fixture(scope="module")
def doc():
    with open(FIXTURE, encoding="utf-8") as fh:
        return json.load(fh)


def reproduces(rule: str, damage: list[int], percent: int, drop: int) -> bool:
    return any(RULES[rule](d * percent) == drop for d in range(damage[0], damage[1] + 1))


def rounding_rows(doc: dict) -> list[dict]:
    """The rows the rounding is tested on: clean, direct damage, full damage known."""
    return [
        e for e in doc["events"]
        if e["status"] == "clean" and e["damage_kind"] == "direct" and e["full_damage"]
    ]


# -- the rules themselves ---------------------------------------------------------------


def test_the_rules_are_the_engines_and_ceil_is_the_kept_share():
    # the three candidates on the two hits that separate them
    assert [RULES[r](1057 * 25) for r in RULES] == [265, 264, 264]
    assert [RULES[r](688 * 23) for r in RULES] == [159, 158, 158]
    # calibration's definition: damage + trunc_toward_zero(damage x (pct - 100) / 100)
    for d in range(1, 1500):
        for p in range(1, 101):
            kept = d - (d * (100 - p)) // 100
            assert RULES["ceil_kept_share"](d * p) == kept, (d, p)


# -- the maker's pure pieces --------------------------------------------------------------


def test_decay_is_read_from_the_steps_around_the_hit_and_hits_are_left_out(m):
    # a Tesla's steps: 2 or 3 a tick, 5 over a two-tick gap, and a 40 that is a hit
    assert m.decay_bracket([(2, 1), (3, 1), (5, 2), (2, 1)]) == (2, 3)
    assert m.decay_bracket([(2, 1), (40, 1)]) == (2, 2)
    assert m.decay_bracket([(40, 1)]) is None


def test_a_building_witness_gives_a_bracket_and_a_troop_an_exact_value(m):
    # the Tesla under the Fireball: 690 in one tick, 693 over two, losing 2..3 a tick
    assert m.damage_bracket(690, 1, (2, 3), True) == [687, 688]
    assert m.damage_bracket(693, 2, (2, 3), True) == [687, 689]
    assert m.damage_bracket(1057, 1, None, False) == [1057, 1057]
    assert m.damage_bracket(690, 1, None, True) is None


def test_only_a_steady_load_countdown_rules_out_a_hit(m):
    row = [None] * 11

    def unit(load):
        r = list(row)
        r[m.LOAD] = load
        return tuple(r)

    assert m.hit_ruled_out(unit(1100), unit(1050), 1)
    assert m.hit_ruled_out(unit(1100), unit(1000), 2)
    assert not m.hit_ruled_out(unit(100), unit(1100), 1)  # set back: a hit landed
    assert not m.hit_ruled_out(unit(50), unit(0), 1)  # ran out: nothing proves it
    assert not m.hit_ruled_out(None, unit(1000), 1)  # not on both frames


def test_the_two_seats_of_a_capture_share_a_battle(m):
    assert m.battle_of("20260920-072148-A") == m.battle_of("20260920-072148-B") == "20260920-072148"
    assert m.battle_of("20260918-112751") == "20260918-112751"


# -- the committed fixture ------------------------------------------------------------------


def test_the_committed_fixture_reads_as_documented(doc):
    text = json.dumps(doc)
    assert "0x" not in text
    assert doc["census"]["crown_tower_hits"] == len(doc["events"])
    for e in doc["events"]:
        assert CAPTURE.match(e["capture"]), e["capture"]
        assert e["status"] in ("clean", "ambiguous", "destroyed")
        assert e["damage_kind"] in ("direct", "over_time")
        assert e["frames"][0] < e["frames"][1]
        assert (e["status"] == "clean") == (not e["why"] and e["hp_after"] is not None)
        if e["hp_after"] is not None:
            assert e["drop"] == e["hp_before"] - e["hp_after"] > 0
        if e["full_damage"]:
            lo, hi = e["full_damage"]
            assert e["status"] == "clean"
            assert 0 < lo <= hi
            for w in e["witnesses"]:
                assert w["damage"][0] <= lo
                assert hi <= w["damage"][1]
    # enough to test the rounding on: three spells, both seats of each battle
    assert {e["spell"] for e in rounding_rows(doc)} == set(SURVIVE_AT_FIXED)
    assert len(rounding_rows(doc)) >= 6


def test_both_seats_of_a_battle_record_the_same_hit(doc):
    hits = defaultdict(list)
    for e in doc["events"]:
        hits[(e["battle"], e["spell"], e["tower"]["x"], e["tower"]["y"])].append(e)
    pairs = 0
    for rows in hits.values():
        for a in rows:
            for b in rows:
                if a is b or a["capture"] >= b["capture"]:
                    continue
                if a["frames"][1] <= b["frames"][0] or b["frames"][1] <= a["frames"][0]:
                    continue  # different hits of one cast
                pairs += 1
                assert (a["hp_before"], a["hp_after"], a["status"]) == (b["hp_before"], b["hp_after"], b["status"])
                if a["full_damage"] and b["full_damage"]:
                    assert a["full_damage"][0] <= b["full_damage"][1]
                    assert b["full_damage"][0] <= a["full_damage"][1]
    assert pairs >= 5, "no two seats recorded one hit: this test covers nothing"


def test_at_40_and_25_only_ceil_at_25_survives_and_only_on_the_lightning(doc):
    for e in rounding_rows(doc):
        got = {
            (r, p) for r in RULES for p in FIXED_PERCENTS
            if reproduces(r, e["full_damage"], p, e["drop"])
        }
        assert got == SURVIVE_AT_FIXED[e["spell"]], (e["capture"], e["spell"], got)


def test_at_a_free_integer_percent_only_ceil_survives_every_hit(doc):
    every = set(RULES)
    for e in rounding_rows(doc):
        got = {
            r: [p for p in range(1, 101) if reproduces(r, e["full_damage"], p, e["drop"])]
            for r in RULES
        }
        assert got == SURVIVE_AT_FREE[e["spell"]], (e["capture"], e["spell"], got)
        every &= {r for r, ps in got.items() if ps}
    assert every == {"ceil_kept_share"}


def test_the_shipped_rounding_is_the_one_that_survives(doc):
    with open(CALIBRATION, encoding="utf-8") as fh:
        shipped = json.load(fh)["combat"]["CROWN_TOWER_DAMAGE_ROUNDING"]["value"]
    for e in rounding_rows(doc):
        assert any(reproduces(shipped, e["full_damage"], p, e["drop"]) for p in range(1, 101)), (
            f"calibration combat.CROWN_TOWER_DAMAGE_ROUNDING = {shipped} reproduces the "
            f"{e['spell']} hit of {e['capture']} at no integer percent"
        )


# -- staleness --------------------------------------------------------------------------------


@pytest.mark.skipif(
    not (LIVE and os.path.isdir(LIVE) and os.path.exists(CARDS)),
    reason="needs ROYALELIVE_REPORTS (the captures folder) and data/derived/cards.json; "
    "a skip here is not a pass",
)
def test_the_committed_fixture_is_what_the_captures_give(m):
    with open(FIXTURE, encoding="utf-8") as fh:
        committed = fh.read()
    assert m.text_of(m.build(LIVE)) == committed, (
        "spell_impacts.json is stale: rerun python tools/make_spell_impact_fixture.py"
    )
