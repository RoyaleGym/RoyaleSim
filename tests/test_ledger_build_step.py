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
