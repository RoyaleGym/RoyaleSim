"""THE EVO GIANT'S RAGE, READ WHOLE: tools/extract_cards.py `rage_areas_block` (the 16.635 table on).

The block is read off characters/giant_ev1.toml or the build stops. Pinned:
  1. the 16.635 table's record carries the block exactly, and its summon is the single unit at the tap;
  2. no earlier table builds the form (every earlier pack's Giant_EV1 row is a NotInUse placeholder);
  3. each change to a row the engine does not read the same way stops the build: another PauseTag, a StartCounterAt or
     an AffectedByHitSpeed on the clock, an attack entry with a Damage of its own, an area that leaves the Rage spell's
     RageDamage, a clock faster than an area's life (the older-area kill would act), a summon member off the tap, a
     second health share. Each test checks the row's old value first, so it edits the row the block reads.
The client pack is needed for 3; it is skipped where data/raw/cr-160635007 is absent.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, os.path.join(ROOT, "tools"))
import extract_cards as ec  # noqa: E402

DERIVED = ROOT / "data" / "derived"
VINTAGE = "160635007-20261103"
BLOCK = {"health_pct": 50, "first_ms": 0, "every_ms": 1500, "paused_by": "COMBAT_DISABLED",
         "hit_speed_scaled": False, "spawn_speed_scaled": False, "counts_hits_after_rage": True,
         "thresholds": [1, 2, 3], "areas": ["GiantEvoRage1", "GiantEvoRage2", "GiantEvoRage3", "GiantEvoRage4"],
         "offset_milli": -2500, "older_kill_delay_ms": 150}
TAP = [{"character": "Giant_EV1", "offset_x_milli": 0, "offset_y_milli": 0}]


def record(name: str) -> dict | None:
    data = json.loads((DERIVED / name).read_text(encoding="utf-8"))
    return next((e for e in data.get("evolutions", []) if e["name"] == "Giant_EV1"), None)


def test_the_16635_table_carries_the_block():
    r = record("cards-160635007-20261103.json")
    assert r is not None, "the Evo Giant is built"
    assert r["evo_rage_areas"] == BLOCK
    assert "summon_members" not in r, "the single summon"
    assert "summon_offsets_x_mirrored" not in r
    assert (r["form_of"], r["evo_cycles"], r["cloned_version"]) == ("Giant", 1, "Giant")


@pytest.mark.parametrize("name", ["cards-15.535.json", "cards-160402017.json", "cards-160402017-20261006.json",
                                  "cards-160402017-20261019.json"])
def test_no_earlier_table_builds_the_form(name):
    assert record(name) is None


@pytest.fixture(scope="module")
def t():
    if not ec.VINTAGES[VINTAGE].raw.is_dir():
        pytest.skip("the 160635007 pack is not here")
    return ec.load_tables(VINTAGE)


def card() -> dict:
    """The card fields the block reads, as `evolution_records` hands them over."""
    return {"summon_character": "Giant_EV1", "form_of": "Giant", "summon_members": [dict(m) for m in TAP],
            "summon_offsets_x_mirrored": False}


def test_the_pack_gives_the_block(t):
    c = card()
    assert ec.rage_areas_block(t, c) == BLOCK
    assert "summon_members" not in c
    assert "summon_offsets_x_mirrored" not in c


def edit(holder: dict, key: str, old, new):
    """Set holder[key] from `old` (checked first) to `new`; give back the undo."""
    assert holder[key] == old, f"{key}: {holder[key]!r}, not {old!r}"
    holder[key] = new

    def undo():
        holder[key] = old
    return undo


def stops(t, why: str, c: dict | None = None) -> None:
    with pytest.raises(SystemExit, match=why):
        ec.rage_areas_block(t, c or card())


def test_another_pause_tag_stops_the_build(t):
    undo = edit(t["actions"].records["giant_evo_start_interval"], "PauseTag", "COMBAT_DISABLED", "STUNNED")
    try:
        stops(t, "its rage's clock")
    finally:
        undo()


@pytest.mark.parametrize("key", ["StartCounterAt", "AffectedByHitSpeed"])
def test_a_clock_key_more_stops_the_build(t, key):
    undo = edit(t["actions"].records["giant_evo_start_interval"], key, None, 0 if key == "StartCounterAt" else True)
    try:
        stops(t, "its rage's clock")
    finally:
        undo()


def test_an_attack_entry_with_damage_stops_the_build(t):
    entry = t["characters"].arrays["Giant_EV1"]["AttackSequenceList"][0]
    assert entry == {}, entry
    entry["Damage"] = 200
    try:
        stops(t, "its attack entries")
    finally:
        entry.pop("Damage")


def test_an_area_with_the_rage_damage_stops_the_build(t):
    undo = edit(t["area_effect_objects"].records["GiantEvoRage3"], "SpawnAreaEffectObject", None, "RageDamage")
    try:
        stops(t, "is not a plain Rage")
    finally:
        undo()


def test_a_clock_faster_than_an_areas_life_stops_the_build(t):
    undo = edit(t["actions"].records["giant_evo_start_interval"], "Interval", 1500, 1000)
    try:
        stops(t, "an older area the newer one can kill")
    finally:
        undo()


def test_a_member_off_the_tap_stops_the_build(t):
    c = card()
    edit(c["summon_members"][0], "offset_x_milli", 0, 500)
    stops(t, "its summon", c)


def test_a_second_health_share_stops_the_build(t):
    acts = t["actions"]
    name = "giant_evo_on_on_health"
    assert ec.col_list(acts, name, "HealthPercentages") == [50]
    cols = acts.arrays.setdefault(name, {})
    old = cols.get("HealthPercentages")
    cols["HealthPercentages"] = [50, 25]
    try:
        stops(t, "its trigger's shares")
    finally:
        if old is None:
            cols.pop("HealthPercentages")
        else:
            cols["HealthPercentages"] = old
