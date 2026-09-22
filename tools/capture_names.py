#!/usr/bin/env python3
"""Capture names and seat letters, shared by the fixture makers.

A ground-truth capture is named by its stamp (``<8 digits>-<6 digits>``) and by the seat it
was recorded from. Every fixture names the seats A, B, ... in sort order over the names it
is given, so a case name carries the capture and the seat and nothing else. One rule, in
one place: four copies of it drifted apart and each drift cost a fixture.

`distinct_captures` keeps one path per file: a captures folder may hold several names for
the same bytes, and a maker that reads both counts one battle twice.

`folder_seats` is the map every maker should use: computed over the whole captures folder, a
letter belongs to the seat. Computed over a subset, A goes to whichever seat sorts first in
that subset, so two fixtures built from different subsets can spell the same seat differently.
"""
from __future__ import annotations

import glob
import os
import re

# What a ground-truth capture file is called.
CAPTURE_SUFFIX = ".native.oracle.jsonl.gz"

# The seat that follows a capture's stamp, in a file name or in a `<capture>:<...>` case
# name. Anchored on the stamp so the stamp's own second field is never mistaken for it.
SEAT_TAG = re.compile(r"(?<=\d{8}-\d{6})-(\d+)(?=[.:]|$)")


def seat_letters(names: list[str]) -> dict[str, str]:
    """Map every seat these names carry to a letter, A first, in sort order."""
    tags = sorted({m.group(1) for n in names for m in [SEAT_TAG.search(n)] if m})
    return {tag: chr(ord("A") + i) for i, tag in enumerate(tags)}


def folder_seats(
    reports: str | None, suffix: str = CAPTURE_SUFFIX, also: list[str] | None = None
) -> dict[str, str]:
    """The seat map of a whole captures folder -- the one map a fixture should name seats by.

    `also` adds names read from outside the folder (a capture given by path, its placement
    logs), so a run that reaches past the folder still sorts its seats together with it.
    """
    names = [os.path.basename(n) for n in (also or [])]
    if reports and os.path.isdir(reports):
        names += [
            os.path.basename(p)
            for p in distinct_captures(glob.glob(os.path.join(reports, "*" + suffix)))
        ]
    return seat_letters(names)


def public_name(raw: str, seats: dict[str, str]) -> str:
    """`raw` with its seat rewritten to the letter `seats` gives it."""
    return SEAT_TAG.sub(lambda m: "-" + seats[m.group(1)], raw)


def distinct_captures(paths: list[str]) -> list[str]:
    """One path per file, sorted: several names for the same bytes collapse to one.

    The name that carries a seat wins, so the letter a capture gets is decided by the seat
    map and not by whichever name happened to sort first.
    """
    best: dict[object, str] = {}
    for p in sorted(paths):
        try:
            st = os.stat(p)
            key: object = (st.st_dev, st.st_ino) if st.st_ino else p
        except OSError:
            key = p
        cur = best.get(key)
        if cur is None or (SEAT_TAG.search(os.path.basename(p)) and not SEAT_TAG.search(os.path.basename(cur))):
            best[key] = p
    return sorted(best.values())


def argv_guard(argv: list[str], doc: str | None) -> None:
    """Accept only `[]` and `["--check"]`.

    Anything else prints the docstring and exits, so a mistyped flag cannot fall through
    to a rewrite of a committed fixture.
    """
    if argv in ([], ["--check"]):
        return
    print(doc or "")
    raise SystemExit(0 if argv in (["-h"], ["--help"]) else 2)
