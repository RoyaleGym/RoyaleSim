"""A refutation must be readable by a TOOL, not only by a human reading prose.

`status` is what generators publish. An entry can be `measured` and still carry, in its
`confidence` string, the fact that the value is refuted for some cases or refuted outright.
A generator reads the status, publishes "measured", and the refutation never reaches the
page. That happened to spells.LAUNCH_POINT, which is `measured` while its own evidence shows
the value is wrong for Arrows.

So: an entry whose `confidence` says REFUTED in capitals, which is how this file marks a
claim about the VALUE, must also carry `refuted_for`. Lower-case "refuted" is used for a
RIVAL candidate being ruled out, which is the healthy case and is not what this gates.

`refuted_for` is either a list of the cases the value is wrong for, or "*" when the value is
refuted outright and `status` does not say so.
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
import ledger_census  # noqa: E402

LEDGER = json.loads((ROOT / "data" / "calibration.json").read_text(encoding="utf-8"))
ENTRIES = ledger_census.nested_entries(LEDGER)
VALUE_REFUTED = re.compile(r"\bREFUTED\b")


def claims_the_value_is_refuted(entry: dict) -> bool:
    return bool(VALUE_REFUTED.search(str(entry.get("confidence", ""))))


def test_a_refuted_value_is_machine_readable():
    missing = [
        key for key, entry in ENTRIES.items()
        if claims_the_value_is_refuted(entry) and "refuted_for" not in entry
    ]
    assert not missing, (
        "these entries say REFUTED in prose only, so a generator reading `status` publishes them "
        f"as though nothing were wrong: {missing}"
    )


def test_refuted_for_has_a_shape_a_tool_can_use():
    for key, entry in ENTRIES.items():
        scope = entry.get("refuted_for")
        if scope is None:
            continue
        assert scope == "*" or (isinstance(scope, list) and scope and all(isinstance(x, str) for x in scope)), (
            f"{key}: refuted_for must be \"*\" or a non-empty list of case names, not {scope!r}"
        )
        assert claims_the_value_is_refuted(entry), (
            f"{key}: carries refuted_for but its confidence does not say REFUTED, so the two disagree"
        )


def test_the_gate_catches_a_refutation_left_in_prose():
    """THE PLANT. Without it this file passes on a ledger where nothing is refuted at all."""
    planted = {"confidence": "REFUTED for Arrows", "status": "measured"}
    assert claims_the_value_is_refuted(planted)
    assert "refuted_for" not in planted, "the plant must be the thing the gate looks for"
    # and the healthy lower-case use must NOT be caught, or the gate would fire on every
    # entry that correctly records a ruled-out rival
    rival = {"confidence": "MEDIUM (ten_subareas_no_dedupe is refuted on 7)", "status": "measured"}
    assert not claims_the_value_is_refuted(rival)


def test_both_known_cases_are_covered_and_distinguishable():
    """The two shapes differ and a consumer must be able to tell them apart: a value wrong
    for SOME cases, and a value wrong outright whose status does not say so."""
    subset = ENTRIES["spells.LAUNCH_POINT"]
    assert subset["status"] == "measured"
    assert subset["refuted_for"] == ["Arrows"]
    whole = ENTRIES["spawner.SPAWN_POINT"]
    assert whole["refuted_for"] == "*", "a wholly refuted value is marked with *"
    assert whole["status"] != "refuted", (
        "if `status` ever gains a refuted value this gate should be re-read: the point of "
        "refuted_for is that the status does NOT carry the fact"
    )
