"""What an area's actions RUN, in order: tools/extract_cards.py `action_schedule` and `schedule_entry`.

WHAT THIS PINS. The loader reads two action shapes it used to refuse: the Goblin Curse's area, whose one action makes
another area (card.rs `area_spawns_area`), and that area's hit, which hangs two buffs from inline tables
(card.rs `on_hit_buffs`). Both read the schedule the extractor writes, so the schedule has to say exactly what the
tables say:
  1. the Goblin Curse's parent area: one entry that is not cosmetic, an ActionSpawn of the AreaEffectType
     GoblinCurseBase at delay 0, and two cosmetic ActionPlayEffect entries at 0 and 5950 (the group's SubActionsDelay);
  2. GoblinCurseBase's hit: two entries read from INLINE tables (no action name), BuffType GoblinCurse and
     GoblinCurseDamage, each with its SpawnTime 100 and its buff row;
  3. the global Lightning's spawn carries an ActionDelay (5000), which is the entry's delay: the loader reads a spawn
     due at its parent's life end as lost (card.rs `late_area_spawn`);
  4. a group that names one action twice keeps both (the Graveyard's twelve entries over its eight actions), in the
     order and with the delays the group lists;
  5. the 2018 build carries no `schedule` or `on_hit` key, so its file stays byte-identical.

The tables come from data/raw/cr-15.535.29, the decoded asset pack, which a clone does not have: every test here
skips LOUDLY without it, naming the directory, and a skip is not a pass.

PLANT. Test 4 checks itself: it rebuilds the Graveyard's schedule with the walker patched to drop a repeated action
(the way `action_graph` de-duplicates what it visits), and asserts that version is shorter, so a walker that
de-duplicated would fail the unpatched count.
"""

from __future__ import annotations

import os
import sys
import tomllib

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RAW = os.path.join(ROOT, "data", "raw", "cr-15.535.29")
sys.path.insert(0, os.path.join(ROOT, "tools"))
import extract_cards as ec  # noqa: E402


@pytest.fixture(scope="module")
def tables():
    if not os.path.isdir(os.path.join(RAW, "csv_logic")):
        pytest.skip(
            f"SKIPPED, NOT PASSED: {RAW} is absent. It is the decoded 15.535.29 asset pack, which a clone never has; "
            "nothing about the action schedule has been checked here."
        )
    return ec.load_tables("15.535.29")


def _aeo(t, name):
    rec = ec.norm_aeo(t, name)
    assert rec is not None, f"no area_effect_objects row {name}"
    return rec


def test_the_curse_parent_runs_one_spawn_of_an_area(tables):
    sched = _aeo(tables, "GoblinCurse")["schedule"]
    assert sched is not None
    assert sched["root"] == "GoblinCurseSet"
    live = [e for e in sched["entries"] if not e.get("cosmetic")]
    assert len(live) == 1, f"one entry that is not cosmetic: {sched['entries']}"
    e = live[0]
    got = (e["class"], e["spawn_type"], e["spawn"], e["delay_ms"])
    assert got == ("ActionSpawn", "AreaEffectType", "GoblinCurseBase", 0)
    assert "unread" not in e, e
    cosmetic = [(x["class"], x["delay_ms"]) for x in sched["entries"] if x.get("cosmetic")]
    assert cosmetic == [("ActionPlayEffect", 0), ("ActionPlayEffect", 5950)], cosmetic


def test_the_curse_circle_hangs_two_buffs_from_inline_tables(tables):
    rec = _aeo(tables, "GoblinCurseBase")
    assert rec["schedule"] is None, "the circle's row names no OnStartingAction"
    oh = rec["on_hit"]
    assert oh is not None
    assert oh["root"] == "GoblinCurseCreateBuffs"
    got = [
        (e["action"], e["class"], e["spawn_type"], e["spawn"], e["delay_ms"], e["spawn_time_ms"])
        for e in oh["entries"]
    ]
    assert got == [
        (None, "ActionSpawn", "BuffType", "GoblinCurse", 0, 100),
        (None, "ActionSpawn", "BuffType", "GoblinCurseDamage", 0, 100),
    ], got
    for e in oh["entries"]:
        assert "unread" not in e, e
        assert e["buff"]["name"] == e["spawn"], e
    mark, damage = oh["entries"][0]["buff"], oh["entries"][1]["buff"]
    assert mark["death_spawn"]["character"] == "GoblinCurseGoblin"
    assert mark["death_spawn"]["other_buff_death_spawn_allowed"] is True
    assert (damage["damage_per_second"], damage["crown_tower_damage_per_hit"]) == (14, 4)


def test_a_spawns_action_delay_is_its_delay(tables):
    for name, child in (
        ("Event_Global_Lightning_Charge1", "Event_Global_Lightning_Charge2"),
        ("Event_Global_Lightning_Charge2", "Event_Global_Lightning"),
    ):
        sched = _aeo(tables, name)["schedule"]
        assert sched is not None
        [e] = sched["entries"]
        assert (e["class"], e["spawn"], e["delay_ms"]) == ("ActionSpawn", child, 5000), e
        assert "unread" not in e, e
    # A column the reader does not understand stays unread beside it (the setup area's ParentGOAsSource).
    [e] = _aeo(tables, "Event_Global_Lightning_Setup")["schedule"]["entries"]
    assert e.get("unread") == ["columns ParentGOAsSource"], e
    assert e["delay_ms"] > 0, e


def test_a_repeated_action_is_kept_in_order(tables, monkeypatch):
    rec = _aeo(tables, "Graveyard_rework")
    sched = rec["schedule"]
    assert sched is not None
    # The group as the table writes it: the list of sub-actions with its repeats, and their delays.
    acts = tables["actions"]
    root = sched["root"]
    subs = acts.arrays[root]["SubActions"]
    delays = acts.arrays[root]["SubActionsDelay"]
    assert [e["action"] for e in sched["entries"]] == list(subs)
    assert [e["delay_ms"] for e in sched["entries"]] == list(delays)
    assert len(set(subs)) < len(subs), "the scene drifted: the Graveyard's group names no action twice"
    # The walker patched to drop a repeat must give fewer entries, so the count above is the walker's own.
    original = ec.schedule_entry
    seen: set = set()

    def dedup(t, r, name, delay):
        if name in seen:
            return None
        seen.add(name)
        return original(t, r, name, delay)

    monkeypatch.setattr(ec, "schedule_entry", dedup)
    patched = [e for e in ec.action_schedule(tables, root)["entries"] if e is not None]
    assert len(patched) < len(sched["entries"]), "a de-duplicating walker gave as many entries"


def test_the_2018_build_carries_no_schedule():
    path = os.path.join(ROOT, "data", "derived", "cards-2018.json")
    if not os.path.exists(path):
        pytest.skip(f"SKIPPED, NOT PASSED: {path} is absent; tools/extract_cards.py --vintage 2018 writes it")
    with open(path, encoding="utf-8") as fh:
        text = fh.read()
    keys = ('"schedule"', '"on_hit"', '"spawn_time_ms"', '"ignore_buffs"', '"apply_buff_before_damage"', '"character2"')
    for key in keys:
        assert key not in text, f"the 2018 file carries {key}: it must stay byte-identical"


def test_the_goblin_curse_toml_is_what_the_schedule_reads(tables):
    """The schedule against the TOML it came from, read here without the extractor: the group's delays and the two
    inline tables' SpawnData."""
    path = os.path.join(RAW, "csv_logic", "characters", "goblin_curse.toml")
    with open(path, "rb") as fh:
        doc = tomllib.load(fh)
    group = doc["ACTION"]["GoblinCurseSet"]
    sched = _aeo(tables, "GoblinCurse")["schedule"]
    assert [e["delay_ms"] for e in sched["entries"]] == group["SubActionsDelay"]
    inline = doc["ACTION"]["GoblinCurseCreateBuffs"]["SubActions"]
    oh = _aeo(tables, "GoblinCurseBase")["on_hit"]
    assert [e["spawn"] for e in oh["entries"]] == [x["SpawnData"] for x in inline]
