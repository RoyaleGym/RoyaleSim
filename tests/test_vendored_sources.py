"""Every vendored checker still hashes to what was recorded when it was copied.

WHY A MANIFEST RATHER THAN A COMMENT IN EACH FILE. The rule for these copies is
byte-identical, so that a diff against the source shows drift. Writing "copied from X at
commit Y" INTO the file would break exactly the property the rule exists for. The record
therefore lives beside them.

WHAT THIS CATCHES AND WHAT IT CANNOT. It catches LOCAL drift: someone editing a vendored
file here, including to satisfy this repo's lint, which is the thing that would make the
next real drift invisible. It cannot catch the other direction -- the original moving ahead
-- because the original is not in a clone, and no test inside this repo ever will.

THAT SECOND DIRECTION IS NOT HYPOTHETICAL AND IT IS WHY THE MANIFEST NAMES A SOURCE. On
2026-09-23 this repo vendored the fence checker from a SIBLING repo's copy rather than from
the original, 89 minutes after a new rule had landed upstream, and inherited a 49-line
staleness. Every comparison available inside this repo passed: the copy was byte-identical
to two other repos' copies. Three instruments agreeing is not confirmation when the thing
they agree with is each other. The missing rule was `counts_without_a_suite`, written that
evening because THIS repo's Rust suite was red while every green count published about it
was the Python suite.

So the manifest records the source path and the source repo's commit, which is what lets
anyone holding both trees check the direction this file cannot.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
MANIFEST = HERE / "vendored_sources.json"


def manifest() -> dict:
    return json.loads(MANIFEST.read_text(encoding="utf-8"))


def test_the_manifest_covers_every_vendored_file_in_this_directory():
    """The list is the thing that rots, so it is checked against the directory rather than
    trusted. A vendored copy nobody recorded is one nobody can check."""
    recorded = set(manifest()["vendored"])
    # A vendored checker is a module here that is not a test and is imported by one.
    found = {
        f"tests/{p.name}"
        for p in HERE.glob("_*.py")
        if p.name != "__init__.py"
    }
    assert found == recorded, (
        f"the directory holds {sorted(found)} and the manifest records {sorted(recorded)}; "
        "an unrecorded vendored file cannot be checked against its source by anyone"
    )


@pytest.mark.parametrize("dest", sorted(manifest()["vendored"]))
def test_the_vendored_copy_is_the_bytes_that_were_recorded(dest: str):
    row = manifest()["vendored"][dest]
    data = (HERE.parent / dest).read_bytes()
    got = hashlib.sha256(data).hexdigest()
    assert got == row["sha256"], (
        f"{dest} has been edited since it was vendored from {row['source']}.\n"
        f"  recorded {row['sha256'][:16]} ({row['lines']} lines)\n"
        f"  now      {got[:16]} ({data.count(chr(10).encode())} lines)\n"
        "These copies are kept byte-identical ON PURPOSE: a copy reformatted to pass this "
        "repo's lint passes lint and makes the next drift invisible. If the original moved, "
        "re-copy it and update the manifest; do not hand-edit the copy."
    )


def test_the_manifest_names_where_to_look():
    m = manifest()
    assert m["source_repo"], "the manifest must name the repo the copies came from"
    assert len(m["source_commit"]) >= 7, "the manifest must name the source commit"
    for dest, row in m["vendored"].items():
        assert row["source"].endswith(".py"), dest
        assert row["lines"] > 50, f"{dest}: {row['lines']} lines recorded, which is not a checker"
