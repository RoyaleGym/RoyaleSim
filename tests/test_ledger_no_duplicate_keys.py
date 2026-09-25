"""The ledger has no duplicate keys.

JSON allows an object to repeat a key and every reader keeps only the LAST copy, silently. The
ledger carried three (2026-09-25: two write-ups under one `measured_2026_09_22` in
match.START_MANA and in match.MANA_REGEN_MS_OVERTIME, and two `proposed_value` in the latter), so
the first copy of each was invisible to the engine, the tools and every reader, while a person
reading the file saw both. A duplicate is a shape no correct ledger takes, so it is refused here
rather than looked for.
"""

import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_no_object_in_the_ledger_repeats_a_key():
    dups: list[str] = []

    def hook(pairs):
        seen = set()
        for key, _ in pairs:
            if key in seen:
                dups.append(key)
            seen.add(key)
        return dict(pairs)

    json.loads((ROOT / "data" / "calibration.json").read_text(encoding="utf-8"), object_pairs_hook=hook)
    assert not dups, f"duplicate keys, each hiding its first copy from every reader: {dups}"


def test_the_check_sees_a_duplicate():
    """THE PLANT: without it this file passes on a checker that never looked."""
    dups: list[str] = []

    def hook(pairs):
        keys = [k for k, _ in pairs]
        dups.extend(k for k in set(keys) if keys.count(k) > 1)
        return dict(pairs)

    json.loads('{"a": {"x": 1, "x": 2}}', object_pairs_hook=hook)
    assert dups == ["x"]
