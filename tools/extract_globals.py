#!/usr/bin/env python3
"""Turn Supercell's globals.csv into data/derived/globals.json, and hold the
calibration registry to it.

WHY THIS EXISTS
    data/calibration.json cites "globals.csv" as the provenance for a dozen
    constants.  A citation nobody re-reads is how a wrong number becomes a fact:
    so every registry entry that claims globals.csv as its source is re-read
    here, from the file, and the build stops if the two disagree.

    It also settles, from the primary file rather than from anybody's summary,
    which PATHFINDING keys actually exist (see calibration.json
    pathfinding.PATHFINDING_COSTS, status "disputed_existence").

VINTAGE -- READ THIS
    The data is retroroyale's ~2018 client.  The target is the LIVE 2026 game.
    Every value below is EVIDENCE, NOT SPEC.  A registry constant cited to a
    NEWER globals.csv can legitimately be absent from this one; that is reported
    as ABSENT, never silently treated as agreement.

THE FORMAT
    Name row, type row, then data rows.  Columns: Name, NumberValue,
    BooleanValue, TextValue, StringArray, NumberArray.  An array value is the
    named row plus every following row whose Name is blank (continuation rows).
    Exactly one typed column is expected to be populated per key; a key with
    none populated is kept with type "empty" (Supercell ships a few).

USAGE
    python tools/extract_globals.py                   # extract + cross-check
    python tools/extract_globals.py --plant disagree  # prove the cross-check fires
    python tools/extract_globals.py --plant bool
    python tools/extract_globals.py --plant array
    python tools/extract_globals.py --plant context   # a globals value may not be declared context
    python tools/extract_globals.py --plant vintage   # a value cited to a newer build is checked against it
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RAW = ROOT / "data" / "raw" / "retroroyale-2018" / "csv_logic"
SRC = RAW / "globals.csv"
CALIBRATION = ROOT / "data" / "calibration.json"
OUT = ROOT / "data" / "derived" / "globals.json"

VINTAGE_WARNING = (
    "PRE-2025 VINTAGE (~2018 client data). The simulator targets the LIVE 2026 game. "
    "Every number in this file is EVIDENCE, NOT SPEC: it records what Supercell shipped "
    "in 2018, not what the live game does now."
)

# Registry names that differ from the globals.csv key.  Only names whose mapping
# is not stated verbatim in the registry entry's own provenance belong here; the
# rest are discovered by scanning provenance text for a real globals key.
ALIASES = {
    "MANA_REGEN_MS_1X": "MANA_REGEN_MS",
    "MANA_REGEN_MS_2X": "MANA_REGEN_MS_END",
}

KEYWORDS = ["RANGE", "SIGHT", "PUSH", "COLLISION", "MASS", "SPEED", "TICK", "MANA"]


# --- reading -----------------------------------------------------------------


def read_globals(path: Path) -> tuple[dict, list[str]]:
    rows = list(csv.reader(path.open(encoding="utf-8-sig")))
    header = rows[0]
    expected = ["Name", "NumberValue", "BooleanValue", "TextValue", "StringArray", "NumberArray"]
    if header[: len(expected)] != expected:
        raise SystemExit(f"{path}: unexpected header {header}")
    col = {h: i for i, h in enumerate(header)}

    order: list[str] = []
    raw: dict[str, list[list[str]]] = {}
    current = None
    orphans = 0
    for r in rows[2:]:
        r = r + [""] * (len(header) - len(r))
        name = r[0].strip()
        if name:
            if name in raw:
                raise SystemExit(f"{path}: duplicate key {name!r}")
            current = name
            order.append(name)
            raw[name] = [r]
        elif current is None:
            orphans += 1
        else:
            raw[current].append(r)
    if orphans:
        raise SystemExit(f"{path}: {orphans} continuation rows before any named row")

    out: dict[str, dict] = {}
    for name in order:
        block = raw[name]
        first = block[0]
        populated = [h for h in expected[1:] if first[col[h]].strip()]
        arrays_cont = [
            h for h in ("StringArray", "NumberArray") if any(b[col[h]].strip() for b in block[1:])
        ]
        if len(populated) > 1:
            raise SystemExit(f"{path}: key {name!r} populates {populated}; expected one")
        # A continuation row carrying a scalar column would be a shape we have not
        # decoded -- stop rather than drop it.
        for b in block[1:]:
            for h in ("NumberValue", "BooleanValue", "TextValue"):
                if b[col[h]].strip():
                    raise SystemExit(f"{path}: continuation row under {name!r} sets scalar {h}")
        kind = populated[0] if populated else (arrays_cont[0] if arrays_cont else None)
        if kind is None:
            out[name] = {"type": "empty", "value": None}
        elif kind == "NumberValue":
            out[name] = {"type": "number", "value": int(first[col[kind]])}
        elif kind == "BooleanValue":
            v = first[col[kind]].strip().lower()
            if v not in ("true", "false"):
                raise SystemExit(f"{path}: {name!r} boolean {v!r}")
            out[name] = {"type": "boolean", "value": v == "true"}
        elif kind == "TextValue":
            out[name] = {"type": "text", "value": first[col[kind]]}
        else:
            vals = [b[col[kind]].strip() for b in block]
            if kind == "NumberArray":
                out[name] = {"type": "number_array", "value": [int(v) if v else None for v in vals]}
            else:
                out[name] = {"type": "string_array", "value": [v or None for v in vals]}
    return out, order


def load_vintage(version: str) -> dict | None:
    """globals of a NEWER build decoded by tools/decode_sc_assets.py, or None if not on disk.

    Newer builds are Supercell's files and are gitignored (data/raw/cr-*/), so a
    fresh clone may lack them: an entry cited to one is then reported UNVERIFIED,
    never silently counted as agreement and never failed for the file's absence.
    """
    path = ROOT / "data" / "raw" / f"cr-{version}" / "csv_logic" / "globals.csv"
    if not path.is_file():
        return None
    return read_globals(path)[0]


# --- the registry cross-check ------------------------------------------------


def registry_constants(cal: dict) -> list[tuple[str, dict]]:
    out = []
    for section, body in cal.items():
        if not isinstance(body, dict) or section in ("meta",):
            continue
        for name, entry in body.items():
            if isinstance(entry, dict) and "value" in entry:
                out.append((f"{section}.{name}", entry))
    return out


def cross_check(g: dict, cal: dict) -> tuple[list[str], list[str], list[str], list[str]]:
    """Return (failures, agreements, absences, context_only).

    Walks EVERY registry constant whose provenance cites globals, so a new
    registry entry cannot dodge the check by not being on a hand-written list.

    CONTEXT-ONLY ENTRIES. A provenance may cite a globals key as supporting context
    for a value that does not come from globals at all -- spells.AOE_HIT_TEST cites
    ADD_CHARACTER_RANGE_TO_RADIUS to justify 'edge_inclusive', and on 2026-09-13 this
    check compared that string with the boolean and failed. Rewording the provenance
    to avoid the word would have dodged the check invisibly. Instead an entry
    declares `"globals_role": "context"`: it is then listed on every run rather than
    compared, and the declaration is REFUSED when the entry's own name is a globals
    key, because then the value really is a globals value and must be compared.
    """
    fail, agree, absent, context = [], [], [], []
    for qual, entry in registry_constants(cal):
        prov = str(entry.get("provenance", ""))
        name = qual.split(".", 1)[1]
        if "globals" not in prov.lower():
            continue
        if entry.get("globals_role") == "context":
            if name in g or name in ALIASES:
                fail.append(
                    f"context-only declaration: {qual} -- its name is a globals key, so its "
                    f"value comes from globals and must be cross-checked, not declared context"
                )
            else:
                context.append(qual)
            continue
        vintage = entry.get("globals_vintage")
        if vintage:
            newer = load_vintage(vintage)
            keys = entry.get("globals_keys") or {None: name}
            want_all = entry["value"]
            for sub, k in keys.items():
                want = want_all[sub] if sub is not None else want_all
                old = g.get(k, {}).get("value", "<absent in 2018 data>")
                if newer is None:
                    absent.append(f"UNVERIFIED {qual}{'.' + sub if sub else ''}: cited to globals of {vintage}, "
                                  f"which is not on disk (run tools/decode_sc_assets.py); 2018 data has {old!r}")
                    continue
                if k not in newer:
                    fail.append(f"calibration agrees: {name} -- cites {k} in globals of {vintage}, no such key")
                    continue
                got = newer[k]["value"]
                if type(got) is not type(want) or got != want:
                    fail.append(f"calibration agrees: {name} -- registry {want!r} vs globals[{vintage}] {k}={got!r}")
                else:
                    agree.append(f"{qual}{'.' + sub if sub else ''} == globals[{vintage}].{k} == {got!r}"
                                 f"  (2018 data: {old!r})")
            continue
        key = None
        if name in g:
            key = name
        elif name in ALIASES:
            key = ALIASES[name]
        else:
            cited = [t for t in re.findall(r"[A-Z][A-Z0-9_]{3,}", prov) if t in g]
            if len(cited) == 1:
                key = cited[0]
        if key is None or key not in g:
            absent.append(
                f"{qual}: registry cites globals.csv but no such key in this 2018 data"
                f" (looked for {ALIASES.get(name, name)!r})"
            )
            continue
        got = g[key]["value"]
        want = entry["value"]
        # type-strict: in Python 1 == True, and a registry boolean must not be
        # "confirmed" by a globals number that happens to be 1.
        if type(got) is not type(want) or got != want:
            fail.append(
                f"calibration agrees: {name} -- registry {want!r} vs globals.csv {key}={got!r}"
            )
        else:
            agree.append(f"{qual} == globals.{key} == {got!r}")
    return fail, agree, absent, context


# --- the report ---------------------------------------------------------------


def scan_all_csv_for(token: str) -> list[str]:
    """Every key (globals row name) AND every column name, in every csv_logic
    file, containing `token`. Column names count: 'SpawnPathfindSpeed' is as
    much a pathfinding parameter as a globals row would be."""
    hits = []
    for p in sorted(RAW.glob("*.csv")):
        rows = list(csv.reader(p.open(encoding="utf-8-sig")))
        if not rows:
            continue
        for h in rows[0]:
            if token in h.upper():
                hits.append(f"{p.name}: column {h!r}")
        for r in rows[2:]:
            for cell in r:
                if token in cell.upper():
                    hits.append(f"{p.name}: cell {cell!r} (row {r[0]!r})")
    return hits


def build(g: dict, order: list[str]) -> dict:
    return {
        "version": "globals-2018.1",
        "vintage_warning": VINTAGE_WARNING,
        "provenance": {
            "source": "retroroyale/ClashRoyale GameAssets csv_logic/globals.csv",
            "source_sha256": hashlib.sha256(SRC.read_bytes()).hexdigest(),
            "vintage": "~2018 client data (pre-2025)",
            "generated_by": "tools/extract_globals.py",
        },
        "globals": {k: g[k] for k in order},
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--plant",
        choices=["disagree", "bool", "array", "context", "vintage"],
        help="corrupt the parsed globals on purpose and prove the gate goes red",
    )
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args()

    cal = json.loads(CALIBRATION.read_text(encoding="utf-8"))
    g, order = read_globals(SRC)

    print("*** " + VINTAGE_WARNING)

    if args.plant:
        aimed_at = {
            "disagree": "calibration agrees: MELEE_RANGE_LIMIT",
            "bool": "calibration agrees: ADD_CHARACTER_RANGE_TO_RADIUS",
            "array": "array continuation rows fold",
            "context": "context-only declaration: targeting.MELEE_RANGE_LIMIT",
            "vintage": "calibration agrees: LOGIC_DEFAULT_TARGET_USE_LANE_ID -- registry",
        }[args.plant]
        base_fail, _, _, _ = cross_check(g, cal)
        base_fail += shape_gate(g)
        if base_fail:
            print(f"PLANT '{args.plant}' INCONCLUSIVE -- baseline already red:", file=sys.stderr)
            for f in base_fail:
                print(f"   {f}", file=sys.stderr)
            return 1
        bad = json.loads(json.dumps(g))
        if args.plant == "disagree":
            bad["MELEE_RANGE_LIMIT"]["value"] += 1
        elif args.plant == "bool":
            bad["ADD_CHARACTER_RANGE_TO_RADIUS"]["value"] = False
        elif args.plant == "array":
            # simulate a parser that ignores continuation rows
            v = bad["LOSE_SCORE_PERCENTAGES"]["value"]
            bad["LOSE_SCORE_PERCENTAGES"]["value"] = v[:1]
        bad_cal = cal
        if args.plant == "context":
            # A globals-sourced constant wrongly declared context-only must be refused,
            # or the declaration becomes a way to switch the cross-check off.
            bad_cal = json.loads(json.dumps(cal))
            bad_cal["targeting"]["MELEE_RANGE_LIMIT"]["globals_role"] = "context"
            assert bad_cal != cal, "context plant did not change the registry copy"
        if args.plant == "vintage":
            # A value cited to a newer build must be compared with THAT build: flip the
            # registry copy and the newer-build comparison has to go red.
            if load_vintage("15.535.29") is None:
                print("PLANT 'vintage' INCONCLUSIVE -- globals of 15.535.29 not on disk", file=sys.stderr)
                return 1
            bad_cal = json.loads(json.dumps(cal))
            e = bad_cal["targeting"]["LOGIC_DEFAULT_TARGET_USE_LANE_ID"]
            e["value"] = not e["value"]
        fail, _, _, _ = cross_check(bad, bad_cal)
        fail += shape_gate(bad)
        hit = [f for f in fail if f.startswith(aimed_at)]
        if hit:
            extra = len(fail) - len(hit)
            print(
                f"PLANT '{args.plant}' LANDED -- '{aimed_at}' went red as intended"
                + (f" (+{extra} neighbouring gate(s) also red)" if extra else "")
                + ":"
            )
            for f in hit:
                print(f"   {f}")
            return 0
        print(f"PLANT '{args.plant}' DID NOT LAND -- '{aimed_at}' stayed GREEN.", file=sys.stderr)
        return 1

    fail, agree, absent, context = cross_check(g, cal)
    fail += shape_gate(g)

    if not args.quiet:
        print("\n== PATHFINDING: every key or column containing PATHFIND, in every csv_logic file")
        hits = scan_all_csv_for("PATHFIND")
        for h in hits:
            print("   " + h)
        for k in order:
            if "PATHFIND" in k.upper():
                print(f"   globals value: {k} = {g[k]['value']!r}")
        cost = [k for k in order if "COST" in k.upper()]
        print(
            f"   PATHFINDING_*_COST keys present: "
            f"{[k for k in order if 'PATHFIND' in k and 'COST' in k] or 'NONE'}"
        )
        print(f"   (all *COST* globals, for completeness: {cost})")

        print("\n== LOGIC_* keys")
        for k in order:
            if k.startswith("LOGIC_"):
                print(f"   {k} = {g[k]['value']!r}")
        print("\n== keys containing " + "/".join(KEYWORDS))
        for kw in KEYWORDS:
            ks = [k for k in order if kw in k.upper()]
            print(f"   [{kw}] " + ("NONE" if not ks else ""))
            for k in ks:
                print(f"      {k} = {g[k]['value']!r}")

        print("\n== calibration.json cross-check")
        for a in agree:
            print("   AGREE   " + a)
        for a in absent:
            print("   ABSENT  " + a)
        for a in context:
            print("   CONTEXT " + a + "  (globals cited as context only; value NOT compared)")

    if fail:
        print("GLOBALS GATE FAILED:", file=sys.stderr)
        for f in fail:
            print(f"   {f}", file=sys.stderr)
        return 1

    doc = build(g, order)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    # newline="\n": derived artifacts must be byte-identical on every OS.
    OUT.write_text(json.dumps(doc, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(
        f"\nglobals -> {OUT.relative_to(ROOT)}  ({len(order)} keys, "
        f"{len(agree)} registry agreements, {len(absent)} absent-in-vintage, "
        f"{len(context)} context-only, 0 failures)"
    )
    return 0


def shape_gate(g: dict) -> list[str]:
    """Independent expectations about the parse itself, so a parser that drops
    continuation rows cannot pass."""
    fail = []
    # LOSE_SCORE_PERCENTAGES is visibly an 8-row array in the CSV (0..100).
    v = g.get("LOSE_SCORE_PERCENTAGES", {}).get("value")
    if not (isinstance(v, list) and len(v) == 8 and v[0] == 0 and v[-1] == 100):
        fail.append(f"array continuation rows fold: LOSE_SCORE_PERCENTAGES = {v!r}")
    for k, e in g.items():
        if isinstance(e["value"], float):
            fail.append(f"no floats: {k}")
    return fail


if __name__ == "__main__":
    raise SystemExit(main())
