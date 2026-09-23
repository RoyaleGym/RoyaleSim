"""A ledger value change and its rebuild are ONE step, and this is what says so first.

WHY IT EXISTS
    `data/calibration.json` is read at RUNTIME by all four repos. The compiled engine
    carries a copy of it, and `royalegym.RustEngine` refuses to construct when the two
    disagree -- loudly, naming the key and both values, which is the right behaviour and
    not the problem.

    The problem is WHO pays. Between the edit and the rebuild, every other session's engine
    is down. On 2026-09-22 that happened twice in twenty minutes, both times from this
    repo: once when `spawner.SPAWN_POINT` was promoted, and once when a new key was added
    and the engine had been built without it. The second was twenty minutes after the rule
    had been written into the shared log, which is the whole lesson: a rule in a log is a
    stale gate made of prose, because nobody reads history before editing a file.

    So the rule now lives in two places that are read at the right moment: the top of
    `calibration.json` itself, and here. This test fails in the editor's own suite, before
    the edit reaches anyone else.

WHAT IT COMPARES
    Every `value` in the ledger ON DISK against the copy COMPILED INTO the installed
    extension module (`royalesim.EMBEDDED_CALIBRATION_JSON`). No royalegym import, so it
    works from a clone of this repo alone. A missing key counts as a difference in both
    directions: an engine built without a key is as stale as one built with a different
    value for it.
"""

from __future__ import annotations

import json
import os

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LEDGER = os.path.join(ROOT, "data", "calibration.json")
REBUILD = r"..\.venv\Scripts\maturin develop --release   (from RoyaleSim)"


def values(doc: object, path: str = "") -> dict[str, object]:
    """Every `value` in the registry, keyed by its dotted path."""
    out: dict[str, object] = {}
    if isinstance(doc, dict):
        if "value" in doc and not isinstance(doc["value"], (dict, list)):
            out[path] = doc["value"]
        for k, v in doc.items():
            if k.startswith("$"):
                continue
            out.update(values(v, f"{path}.{k}" if path else k))
    return out


def test_the_installed_engine_carries_the_ledger_on_disk():
    core = pytest.importorskip(
        "royalesim",
        reason="the extension module is not installed; run `maturin develop --release`"
        " -- a skip here is not a pass",
    )
    built = values(json.loads(core.EMBEDDED_CALIBRATION_JSON))
    with open(LEDGER, encoding="utf-8") as fh:
        now = values(json.load(fh))
    diffs = [
        f"{key}: built {built.get(key, '<absent>')!r}, on disk {now.get(key, '<absent>')!r}"
        for key in sorted(set(built) | set(now))
        if built.get(key, "<absent>") != now.get(key, "<absent>")
    ]
    assert not diffs, (
        "the ledger on disk and the installed engine disagree, so every session's "
        "RustEngine() is refusing to construct RIGHT NOW:\n  "
        + "\n  ".join(diffs)
        + f"\n\nA value edit and its rebuild are ONE step. Run:\n  {REBUILD}"
    )


def test_the_comparison_is_looking_at_the_whole_registry():
    """Green means nothing if the walk found two keys. The registry is large and the
    count is a floor rather than a pin, because entries are added most days."""
    with open(LEDGER, encoding="utf-8") as fh:
        now = values(json.load(fh))
    assert len(now) >= 100, f"the walk found only {len(now)} values, so it is not reading the registry"
    assert "movement.STOMP_PAUSE_SCHEDULE" in now, sorted(now)[:20]


@pytest.mark.parametrize(
    ("built", "disk", "why"),
    [
        ({"a": {"value": 1}}, {"a": {"value": 2}}, "a changed value"),
        ({"a": {"value": 1}}, {"a": {"value": 1}, "b": {"value": 2}}, "a NEW key, which is the one that bit"),
        ({"a": {"value": 1}, "b": {"value": 2}}, {"a": {"value": 1}}, "a removed key"),
    ],
)
def test_the_comparison_would_notice(built, disk, why):
    """The plant, in process: each shape must be reported, or the test above is a walk
    over an agreement it cannot fail to find."""
    b, d = values(built), values(disk)
    diffs = [k for k in set(b) | set(d) if b.get(k, "<absent>") != d.get(k, "<absent>")]
    assert diffs, why


def test_the_comparison_is_quiet_when_they_agree():
    """And the other direction, or a guard that always fires is one somebody removes."""
    same = {"a": {"value": 1}, "s": {"x": {"value": "t"}}}
    b, d = values(same), values(json.loads(json.dumps(same)))
    assert [k for k in set(b) | set(d) if b.get(k) != d.get(k)] == []


def test_the_real_gate_fires_when_the_two_actually_differ(monkeypatch):
    """The plant on the REAL test's wiring, not on its arithmetic.

    The parametrised cases above prove the comparison notices a changed, an added and a
    removed key. They do not prove this file reads the installed module and the ledger on
    disk, which is the part that was wrong in the incident. So: replace the module's
    embedded copy with one that is missing a key -- exactly the 23:06 shape -- and require
    the gate to go red naming it. The real file is never touched, so no other session's
    engine is disturbed to run this.
    """
    core = pytest.importorskip("royalesim", reason="the extension module is not installed")
    doc = json.loads(core.EMBEDDED_CALIBRATION_JSON)
    removed = doc["movement"].pop("STOMP_PAUSE_SCHEDULE", None)
    assert removed is not None, "the plant removed nothing, so it proves nothing"
    monkeypatch.setattr(core, "EMBEDDED_CALIBRATION_JSON", json.dumps(doc))
    with pytest.raises(AssertionError) as caught:
        test_the_installed_engine_carries_the_ledger_on_disk()
    assert "movement.STOMP_PAUSE_SCHEDULE" in str(caught.value), str(caught.value)[:300]
    assert "ONE step" in str(caught.value), "the failure must say what to do, not only what is wrong"


# --- the other three files this crate compiles in ------------------------------------

#: (module constant, path on disk, what a stale copy costs). FOUR files are `include_str!`ed
#: into the extension and only calibration.json and arena.json were ever compared with the
#: disk. The other two had the same property that took the workspace's engine down twice on
#: 2026-09-22, unarmed, and theirs is the worse failure: calibration's mismatch REFUSES
#: loudly, while a stale arena or rarity table is a silently different battle.
#: The fourth element is HOW to compare, and it is not the same question for each file.
#: "values" reads the registry's `value` fields, because calibration.json's prose changes
#: most days and a reworded provenance is not a stale engine -- the first test in this file
#: does that one. "json" compares the parsed document minus `provenance`, which is the rule
#: royalegym already uses for the arena and the reason a regenerated timestamp does not cry
#: wolf. "bytes" is for the two CSVs, which have no structure to compare and where any
#: difference at all is a different table.
EMBEDDED = [
    ("EMBEDDED_CALIBRATION_JSON", ("data", "calibration.json"), "values",
     "every constant in the engine"),
    ("EMBEDDED_ARENA_JSON", ("data", "derived", "arena.json"), "json",
     "the arena's geometry, silently -- a wrong river or tower box with nothing to say so"),
    ("EMBEDDED_RARITIES_CSV", ("data", "raw", "retroroyale-2018", "csv_logic", "rarities.csv"), "bytes",
     "every card's level scaling, silently"),
    ("EMBEDDED_GLOBALS_CSV", ("data", "raw", "retroroyale-2018", "csv_logic", "globals.csv"), "bytes",
     "the shipped globals the engine reads, silently"),
]


@pytest.mark.parametrize(("const", "parts", "how", "cost"), EMBEDDED, ids=[e[0] for e in EMBEDDED])
def test_every_compiled_in_file_matches_the_one_on_disk(const, parts, how, cost):
    """A loud skip when the installed module does not expose the constant: the two CSV ones
    were added later than the module in this venv, so the honest report is "this workspace's
    engine predates the check" rather than a pass."""
    core = pytest.importorskip(
        "royalesim",
        reason="the extension module is not installed; run `maturin develop --release`"
        " -- a skip here is not a pass",
    )
    if not hasattr(core, const):
        pytest.skip(
            f"the installed engine predates {const}, so this file is STILL UNWATCHED here."
            f" Rebuild with `maturin develop --release` to arm it -- a skip here is not a pass"
        )
    with open(os.path.join(ROOT, *parts), encoding="utf-8") as fh:
        on_disk = fh.read()
    built = getattr(core, const)
    if how == "values":
        built, on_disk = values(json.loads(built)), values(json.loads(on_disk))
    elif how == "json":
        built, on_disk = json.loads(built), json.loads(on_disk)
        built.pop("provenance", None)
        on_disk.pop("provenance", None)
    assert built == on_disk, (
        f"{os.path.join(*parts)} has changed since the engine was built, so the installed "
        f"engine is running a different {cost}.\n\nRun:\n  {REBUILD}"
    )


def test_all_four_compiled_in_files_are_listed_here():
    """The list is what rots. If the crate gains a fifth `include_str!` this must gain a row,
    and nothing else in the repo would notice.

    `os.walk`, not `os.listdir`. The first version read only the files directly in `src/`,
    which is correct today because the crate has no subdirectories there and WRONG the moment
    anyone adds one -- and missing a file in a subdirectory is precisely what this test
    exists to prevent. A scan that sees part of the space reports on the whole space and
    looks healthy. Found by the integrator, whose own scanner walks.
    """
    src = os.path.join(ROOT, "crates", "royalesim", "src")
    found = set()
    seen_files = 0
    for folder, _dirs, names in os.walk(src):
        for name in names:
            if not name.endswith(".rs"):
                continue
            seen_files += 1
            with open(os.path.join(folder, name), encoding="utf-8") as fh:
                for line in fh:
                    if "include_str!(" in line and "data/" in line:
                        found.add(line.split('include_str!("')[1].split('")')[0].split("../")[-1])
    assert seen_files >= 10, f"the walk read {seen_files} .rs files, so it is not reading the crate"
    listed = {"/".join(row[1]) for row in EMBEDDED}
    assert found == listed, (
        f"the crate compiles in {sorted(found)} and this file watches {sorted(listed)}; "
        "an unwatched one is a silently different battle"
    )
