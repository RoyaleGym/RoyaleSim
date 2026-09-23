"""Count the calibration ledger, and check a doc's stated counts against it.

WHY THIS EXISTS. The counts in docs/calibration.md were corrected on 2026-09-22 and went
stale the same afternoon, twice, because every status promotion moves them. A number typed
into prose is stale the moment the ledger moves, and nothing was watching. So the counts are
now produced here and gated by tests/test_ledger_census.py.

    python tools/ledger_census.py                     # print the counts
    python tools/ledger_census.py --check docs/calibration.md   # STALE or current

An ENTRY is one dict carrying a string `status`. The walk descends until it finds one and
then STOPS, so an entry nested inside another entry is NOT counted. The only one in the file
today is `pathfinding.PATHFINDING_COSTS.application`. That exclusion matches what
`tools/check_data.py`, the README tile and the integrator's gate all count, so changing it
would make four tools disagree; `tests/test_ledger_census.py` pins both the convention and
the exclusion.
"""

from __future__ import annotations

import argparse
import collections
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LEDGER = ROOT / "data" / "calibration.json"


def entries(node: dict, prefix: str = "") -> dict:
    """Every dict carrying a string status, descending until one is found and then stopping.

    So an entry nested INSIDE an entry is not counted (see the module docstring).
    """
    out: dict[str, dict] = {}
    for key, value in node.items():
        if key.startswith("$") or not isinstance(value, dict):
            continue
        if isinstance(value.get("status"), str):
            out[prefix + key] = value
        else:
            out.update(entries(value, prefix + key + "."))
    return out


def nested_entries(node: dict, prefix: str = "") -> dict:
    """Every dict carrying a string status, INCLUDING ones nested inside another entry.

    `entries()` stops at the first status it finds, which is the convention four tools
    share. This does not stop, so the two differ by the entries that sit inside entries.
    Both are reported, because a convention that undercounts is still an undercount if
    nobody says by how much.
    """
    out: dict[str, dict] = {}
    for key, value in node.items():
        if key.startswith("$") or not isinstance(value, dict):
            continue
        if isinstance(value.get("status"), str):
            out[prefix + key] = value
        out.update(nested_entries(value, prefix + key + "."))
    return out


def census(ledger: dict | None = None) -> dict:
    ledger = ledger if ledger is not None else json.loads(LEDGER.read_text(encoding="utf-8"))
    e = entries(ledger)
    deep = nested_entries(ledger)
    by_status = collections.Counter(v["status"] for v in e.values())
    deep_status = collections.Counter(v["status"] for v in deep.values())
    measured = [k for k, v in e.items() if v["status"] == "measured"]
    no_rival = [k for k in measured if not e[k].get("candidates")]
    return {
        "entries": len(e),
        "entries_with_nested": len(deep),
        "measured_with_nested": deep_status["measured"],
        "nested_inside_an_entry": sorted(set(deep) - set(e)),
        "candidates": sum(1 for v in e.values() if v.get("candidates")),
        "promotion_rules": sum(1 for v in e.values() if v.get("promotion_rules")),
        "both": sum(1 for v in e.values() if v.get("candidates") and v.get("promotion_rules")),
        "measured_no_rival": len(no_rival),
        "measured_no_rival_no_promotion": sum(1 for k in no_rival if not e[k].get("promotion_rules")),
        **{status: n for status, n in sorted(by_status.items())},
    }


# Each pattern must match the doc exactly once, and its group is the number the census
# supplies. A pattern that stops matching is itself a failure: the sentence moved and its
# number is no longer being watched.
CHECKS = [
    (r"went through 24 of the (\d+) entries", "entries"),
    (r"\n(\d+) entries have not been re-read", None),  # entries minus the 24 re-read
    (r"All (\d+) carry a status", "entries"),
    (r"(\d+) name the rivals the\n  value was chosen against", "candidates"),
    (r"and (\d+) state what would move it", "promotion_rules"),
    (r"and (\d+) do both", "both"),
    (r"the (\d+) `datamined` keys", "datamined"),
    (r"(\d+) of\n  the (\d+) `measured` entries name no rival", ("measured_no_rival", "measured")),
    (r"and (\d+) of those state no promotion criterion", "measured_no_rival_no_promotion"),
    (r"Of the (\d+) top-level keys with a status, (\d+) are `measured`", ("entries", "measured")),
    # The nested entry the convention leaves out. Watched so the undercount cannot be
    # quietly dropped from the page later.
    (r"counting every status in the file gives (\d+)\s+and (\d+) measured",
     ("entries_with_nested", "measured_with_nested")),
]


def check(doc: pathlib.Path) -> int:
    c = census()
    text = doc.read_text(encoding="utf-8")
    stale, unmatched = [], []
    for pattern, want in CHECKS:
        found = re.findall(pattern, text)
        if len(found) != 1:
            unmatched.append(f"{pattern!r} matched {len(found)} times, expected 1")
            continue
        got = found[0]
        keys = want if isinstance(want, tuple) else (want,)
        gots = got if isinstance(got, tuple) else (got,)
        # strict: a pattern whose group count stops matching its keys would otherwise
        # truncate silently and skip a check, which is the failure this file exists to stop.
        for g, k in zip(gots, keys, strict=True):
            expect = c["entries"] - 24 if k is None else c[k]
            if int(g) != expect:
                stale.append(f"{pattern!r}: doc says {g}, the ledger says {expect}")
    if unmatched:
        print("PATTERNS THAT NO LONGER MATCH (the sentence moved, so its number is unwatched):")
        for u in unmatched:
            print("   ", u)
    if stale:
        print(f"STALE: {doc} disagrees with data/calibration.json")
        for line in stale:
            print("   ", line)
    if stale or unmatched:
        print("\nthe ledger now reads:")
        for k, v in c.items():
            print(f"    {k}: {v}")
        return 1
    print(f"{doc} is current against data/calibration.json")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--check", metavar="DOC", help="compare a doc's counts against the ledger")
    args = ap.parse_args()
    if args.check:
        return check(pathlib.Path(args.check))
    for key, value in census().items():
        print(f"{key}: {value}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
