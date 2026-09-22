#!/usr/bin/env python3
"""REGENERATE crates/royalesim/tests/fixtures/oracle2026/client16402_jumps.json.

    python tools/make_client16402_jump_fixture.py            # rewrite
    python tools/make_client16402_jump_fixture.py --check    # diff only (exit 1 on a difference)

WHERE THE INPUTS ARE
    The live capture comes from the client instrument that records ground-truth traces from the
    real game (CR 16.402). Its folder is named by the environment:
      ROYALELIVE_REPORTS  the captures folder (required; there is no default)
    The card rows come from this repo's decoded 15.535 data (data/raw/cr-15.535.29, gitignored).

WHAT IT HOLDS
    The five JumpEnabled water hops (and one bridge walk) the LIVE client (CR 16.402) published in
    capture 20260920-002736 (native side 0): a Hog Rider and a Prince placed at the river centre
    (9500, 12500) and the four Royal Hogs of a card placed at (11285, 12500). Each case carries
    the unit's per-tick position, behavior_state and published node list (goal first,
    `row * 36 + col` decoded to [col, row]) from its first walking tick (Hog, Prince) or from two
    ticks before the hop (Royal Hogs, whose early walk is a four-unit crowd the engine has no
    card for) to fifteen ticks after the landing. The capture does not carry every tick; the
    frames list only the ticks it has, so a gate compares tick by tick where a frame exists.

    Every position is native millitiles, absolute. `card_15535` is the live client's own row for
    the columns the walk reads (data/raw/cr-15.535.29/csv_logic/characters/*.toml). The Rust
    gate is tests/jump16402.rs: the leap arithmetic (jump16402.rs) replayed from each hop's
    trigger position must reproduce every later frame and the landing tick, and a Hog Rider /
    Prince spawned where the capture placed them must reproduce the whole track through the
    shipped engine, published lists included.

    CASE NAMES are `<capture>:<Card>:<k>`, k the unit's ordinal among that card's units in the
    capture (order of appearance). Seats are named A, B, ... in sort order over the whole
    captures folder (tools/capture_names.py `folder_seats`), so every fixture spells a seat the
    same way and a case name carries the capture and the seat and nothing else.
"""
from __future__ import annotations

import glob
import gzip
import json
import os
import sys
import tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from capture_names import argv_guard, distinct_captures, folder_seats, public_name  # noqa: E402

LIVE = os.environ.get("ROYALELIVE_REPORTS")
# The battle, recorded from both seats; the side-0 seat's file is the one used (its header says so).
BATTLE = "20260920-002736"
SUFFIX = ".native.oracle.jsonl.gz"
# The live client's own rows (CR 15.535 csv_logic, the 16.402 values for these columns):
# the engine's cards.json holds whichever vintage was last extracted into it -- the name
# does not carry the vintage, data/derived/ is gitignored, and the README's recipe installs
# the 2018 tables there while a run with no --vintage installs the live 15.535 ones. The
# 2018 Prince ships Range 1850 / CollisionRadius 650 and no jump block, so the gate
# overrides the Prince from this block to reproduce its hop under EITHER vintage -- data
# from the decoded assets, never a number in a test.
CHARACTERS = os.path.join(ROOT, "data", "raw", "cr-15.535.29", "csv_logic", "characters")
TOML = {"HogRider": "hogrider", "Prince": "prince", "RoyalHog": "royalhog"}
STAT_COLUMNS = ("Range", "CollisionRadius", "Speed", "SightRange", "JumpEnabled", "JumpHeight", "JumpSpeed",
                "ChargeRange", "ChargeSpeedMultiplier", "DamageSpecial", "DeployTime")
OUT = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures", "oracle2026", "client16402_jumps.json")
COLS = 36
# the river's half-rows on the shipped arena (data/derived/arena.json water_half_rows)
WATER_ROWS = (30, 33)
JUMP_STATE = 5
WALK_STATE = 1
AFTER_LANDING = 15
BEFORE_HOP = 2
SIDE = 0

# The cases: every side-0 unit of these cards, in order of appearance (the capture's card ids).
CARD_IDS = {"HogRider": 26000021, "Prince": 26000016, "RoyalHog": 26000059}


def load(path: str) -> tuple[dict, list[dict]]:
    header, frames = None, []
    with gzip.open(path, "rt", encoding="utf-8") as f:
        for line in f:
            rec = json.loads(line)
            if rec.get("record") == "header":
                header = rec
            else:
                frames.append(rec["state"])
    assert header is not None, path
    return header, frames


def track(frames: list[dict], eid: str) -> list[dict]:
    out = []
    for st in frames:
        for e in st["entities"]:
            if e["id"] == eid:
                out.append({
                    "tick": st["tick"],
                    "pos": [e["x"], e["y"]],
                    "state": e["behavior_state"],
                    "nodes": [[n % COLS, n // COLS] for n in e["path_nodes"]],
                    "seg": [e["path_segment_direction_x"], e["path_segment_direction_y"]],
                    "reached": e["path_node_consumed"],
                    "target": e["target"],
                })
    return out


def live_row(card: str) -> dict:
    with open(os.path.join(CHARACTERS, TOML[card] + ".toml"), "rb") as f:
        raw = tomllib.load(f)
    row = raw["CHARACTER"][card]
    assert row["Name"] == card, (card, row["Name"])
    return {k: row[k] for k in STAT_COLUMNS if k in row}


def units_of(frames: list[dict], card_id: int) -> list[str]:
    """The ids of the side-0 units of a card, in order of appearance (ties by the capture's order)."""
    seen: list[str] = []
    for st in frames:
        for e in sorted(st["entities"], key=lambda e: e["generation_key"]):
            if e["side"] == SIDE and e["card_id"] == card_id and e["id"] not in seen:
                seen.append(e["id"])
    return seen


def find_capture() -> str:
    """The side-0 seat's capture of the battle, by its header.

    The folder may carry one capture under several names; `distinct_captures` keeps one per
    file, so a second name for the same bytes cannot look like a second side-0 seat.
    """
    if not LIVE:
        sys.exit("set ROYALELIVE_REPORTS to the folder holding frames-auto-" + BATTLE + "-*" + SUFFIX)
    paths = distinct_captures(glob.glob(os.path.join(LIVE, "frames-auto-" + BATTLE + "-*" + SUFFIX)))
    hits = []
    for path in paths:
        with gzip.open(path, "rt", encoding="utf-8") as f:
            header = json.loads(f.readline())
        if header.get("record") == "header" and header.get("local_side_native") == SIDE:
            hits.append(path)
    if len(hits) != 1:
        sys.exit(f"expected one side-{SIDE} capture of {BATTLE} under {LIVE}, found {hits}")
    return os.path.basename(hits[0]).removesuffix(SUFFIX)


def build() -> dict:
    raw = find_capture()
    header, frames = load(os.path.join(LIVE, raw + SUFFIX))
    by_id = {e["id"]: e for st in frames for e in st["entities"]}
    # The letter is the WHOLE FOLDER's (capture_names.folder_seats), which is what the other
    # fixtures name seats by: a map over this battle alone would call its lowest seat "A"
    # whatever the rest of the corpus calls it.
    capture = public_name(raw.removeprefix("frames-auto-"), folder_seats(LIVE, SUFFIX))
    cases = []
    for card, card_id in CARD_IDS.items():
        for k, eid in enumerate(units_of(frames, card_id)):
            name = f"{capture}:{card}:{k}"
            tr = track(frames, eid)
            spawn = tr[0]
            first_walk = next(i for i, f in enumerate(tr) if f["state"] == WALK_STATE)
            hops = [i for i, f in enumerate(tr) if f["state"] == JUMP_STATE]
            if not hops:
                # the first Royal Hog reached the bridge column before the river and
                # simply walked across: the negative witness (a jumper whose next node is
                # never water never hops). Kept up to its first frame past the river.
                past = next(i for i, f in enumerate(tr) if f["pos"][1] >= (WATER_ROWS[1] + 1) * 500)
                lo, hi = first_walk, past
                tgt = by_id[tr[first_walk + 1]["target"]]
                hop_start, landing = None, None
            else:
                hop_start, hop_end = hops[0], hops[-1]
                landing = hop_end + 1
                assert tr[landing]["state"] == WALK_STATE, name
                assert all(f["state"] == JUMP_STATE for f in tr[hop_start:hop_end + 1]), name
                lo = first_walk if card in ("HogRider", "Prince") else max(first_walk, hop_start - BEFORE_HOP)
                hi = min(len(tr) - 1, landing + AFTER_LANDING)
                tgt = by_id[tr[hop_start]["target"]]
            cases.append({
                "name": name,
                "card": card,
                "card_15535": live_row(card),
                "side": SIDE,
                "spawn_tick": spawn["tick"],
                "spawn_native": spawn["pos"],
                "target_native": [tgt["x"], tgt["y"]],
                "hop_tick": None if hop_start is None else tr[hop_start]["tick"],
                "landing_tick": None if landing is None else tr[landing]["tick"],
                "hop_node": None if hop_start is None else tr[hop_start]["nodes"][0],
                "frames": [
                    {"tick": f["tick"], "pos": f["pos"], "state": f["state"], "nodes": f["nodes"], "seg": f["seg"]}
                    for f in tr[lo:hi + 1]
                ],
            })
    return {
        "$comment": "GENERATED by tools/make_client16402_jump_fixture.py from live capture " + capture +
                    " (client 16.402); do not edit by hand. Native millitiles, absolute; nodes goal "
                    "first as recorded; `state` is the recorded regime tag, and the hop frames carry a "
                    "different one from the walking frames; ticks the capture does not carry are absent.",
        "capture": capture,
        "towers": header["towers"],
        "cases": cases,
    }


def main() -> None:
    argv_guard(sys.argv[1:], __doc__)
    fixture = build()
    text = json.dumps(fixture, indent=1) + "\n"
    if "--check" in sys.argv:
        with open(OUT, encoding="utf-8") as f:
            if f.read() != text:
                print("FIXTURE DIFFERS from the capture; rerun without --check")
                sys.exit(1)
        print("fixture matches the capture")
        return
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    for c in fixture["cases"]:
        print(f"{c['name']}: hop t{c['hop_tick']} -> node {c['hop_node']}, "
              f"landing t{c['landing_tick']}, {len(c['frames'])} frames")
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
