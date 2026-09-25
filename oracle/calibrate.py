#!/usr/bin/env python3
"""Promote a measured oracle result into data/calibration.json -- with its evidence.

WHAT IT DOES
    Takes one or more tools/diff_harness.py result files (oracle-harness-result/1),
    verifies the whole evidence chain behind each, maps the winning candidate to a
    registry value through the scenario's `promotes` block in oracle/scenarios.json,
    and writes the new value with the evidence attached and the previous value kept
    in a `history` list.  --dry-run prints the diff and writes nothing.

    Every refusal is a named gate (plants aim at these names):
      evidence-attached     the result lists its recordings; every trace and video exists
      hashes-match          result -> trace sha256 and trace -> video sha256 re-hash equal
      trace-valid           each trace passes oracle.extract_tracks.validate_trace
      not-synthetic         no trace came from oracle/synth.py (or claims any source but video)
      video-container       the "video" is a video container, not a JSON file renamed
      decisive              the result is decisive, not blocked, with the protocol's repetitions
      reproducible          re-ranking the attached traces NOW gives the same winner
      pins-settled          every prerequisite pin the result used is a `measured` registry value
      key-exists            the registry key the scenario promotes exists
      value-mapped          the winner maps to a registry value (S04's 30 TPS does not)
      joint-agrees          a jointly-promoted key (ALGORITHM from S02+S03) has both halves agreeing
      dependencies-measured `measured` is only written on top of `measured` dependencies
      status-known          the current status is one this tool can order
      no-downgrade          a status is never lowered
      no-silent-conflict    a `measured` value is never replaced without --supersede REASON

WHY IT EXISTS
    A registry that stores plausible and measured numbers the same way makes a wrong
    guess indistinguishable from a fact, and tuning then absorbs the error where
    nobody can find it.  data/calibration.json's statuses only mean something if the
    one tool that writes `measured` refuses everything that is not a measurement.
    Never promote a constant from a synthetic trace.

WHAT IT CANNOT CATCH
    * Forgery.  A synthetic trace relabelled `"kind": "video"` next to any real video
      file passes every gate here -- the self-test's own baseline does exactly that,
      in a temp dir.  The not-synthetic gate stops ACCIDENTS (feeding synth.py output
      to the promotion), not deliberate fabrication.
    * A recording of the wrong scenario that happens to rank decisively.
    * A candidate set that does not contain the real game (see diff_harness.py).
    * Formatting: the registry is re-serialised (indent 2), so the first write
      reflows hand-formatted one-line entries.  The printed diff is semantic.

USAGE
    python oracle/calibrate.py --result s01.result.json --dry-run
    python oracle/calibrate.py --result s01.result.json --status hypothesis
    python oracle/calibrate.py --result s02.result.json --result s03.result.json
    python oracle/calibrate.py --result s04.result.json --result s01.result.json   # deps first
    python oracle/calibrate.py --result x.json --supersede "re-measured at 120 fps"
    python oracle/calibrate.py --self-test            # baseline allowed, every plant refused
    python oracle/calibrate.py --plant synthetic

    Exit codes: 0 promoted (or would be, under --dry-run),
    1 refused / plant did not land, 2 usage error.
"""

from __future__ import annotations

import argparse
import ast
import copy
import datetime as _dt
import difflib
import hashlib
import json
import os
import random
import sys
import tempfile
from dataclasses import dataclass, field
from fractions import Fraction
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))
TOOLS = ROOT / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

from oracle import synth  # noqa: E402
from oracle.extract_tracks import (  # noqa: E402
    SOURCE_KINDS_SYNTHETIC,
    TraceSchemaError,
    sha256_file,
    validate_trace,
)

import diff_harness as harness  # noqa: E402

REGISTRY = ROOT / "data" / "calibration.json"
TOOL_VERSION = 1
# Increasing trust, from the registry's own $comment.  `disputed_existence` (the
# status of PATHFINDING_COSTS) is ranked with guess: nobody has evidence either way.
# `owner_ruling` -- a maintainer's direct observation of the live 2026 game, recorded
# verbatim in the key's provenance -- is ranked WITH measured, not below it.  Ranking
# it lower would let an oracle run overwrite such an observation in silence; ranking
# it here means the no-silent-conflict gate below covers it and --supersede REASON is
# required.
STATUS_RANK = {
    "guess": 0,
    "disputed_existence": 0,
    "hypothesis": 1,
    "community": 2,
    # One outside measurement, its source, method and sample named, not reproduced by this
    # project's instruments. Level with community and NOT in PROTECTED, so an oracle
    # `measured` replaces it without --supersede. Added 2026-09-24: two such readings had been
    # filed as measured, which protected them against the very correction that should
    # replace them, and let a promotion that depended on them pass `dependencies-measured`.
    "third_party_measured": 2,
    "datamined": 3,
    "measured": 4,
    "owner_ruling": 4,
}
# Statuses that may not be overwritten with a DIFFERENT value without --supersede.
PROTECTED = ("measured", "owner_ruling")
# Leading bytes of containers a phone or a screen recorder writes.  ISO-BMFF
# (mp4/mov/3gp/m4v) carries 'ftyp' at offset 4; Matroska/WebM starts with EBML.
VIDEO_MAGIC = ((4, b"ftyp"), (0, b"\x1a\x45\xdf\xa3"), (0, b"RIFF"))


class Refusal(Exception):
    def __init__(self, gate: str, message: str):
        super().__init__(f"{gate}: {message}")
        self.gate = gate


@dataclass
class Bundle:
    path: Path
    sha256: str
    result: dict
    scenario: dict
    traces: list[dict]
    recordings: list[dict] = field(default_factory=list)


def _now() -> str:
    return _dt.datetime.now(_dt.UTC).isoformat(timespec="seconds")


def _rel(p: Path) -> str:
    try:
        return str(Path(p).resolve().relative_to(ROOT)).replace("\\", "/")
    except ValueError:
        return str(Path(p).resolve())


def _resolve(p: str, base: Path) -> Path:
    q = Path(p)
    if q.is_absolute():
        return q
    for cand in (base / q, ROOT / q, Path.cwd() / q):
        if cand.exists():
            return cand
    return base / q


def looks_like_video(path: Path) -> bool:
    with Path(path).open("rb") as fh:
        head = fh.read(16)
    return any(head[off : off + len(m)] == m for off, m in VIDEO_MAGIC)


# --------------------------------------------------------------------------- #
# evidence chain                                                              #
# --------------------------------------------------------------------------- #


def load_bundle(result_path: Path, scen: dict) -> Bundle:
    result_path = Path(result_path)
    if not result_path.exists():
        raise Refusal("evidence-attached", f"result file {result_path} does not exist")
    raw = result_path.read_bytes()
    try:
        res = json.loads(raw)
    except json.JSONDecodeError as e:
        raise Refusal("evidence-attached", f"{result_path} is not JSON: {e}") from e
    if res.get("format") != harness.RESULT_FORMAT:
        raise Refusal(
            "evidence-attached",
            f"{result_path}: format {res.get('format')!r}, expected {harness.RESULT_FORMAT}",
        )
    sid = res.get("scenario_id")
    if sid not in scen:
        raise Refusal("evidence-attached", f"{result_path}: unknown scenario {sid!r}")
    rec_meta = res.get("recorded")
    if not isinstance(rec_meta, list) or not rec_meta:
        raise Refusal(
            "evidence-attached",
            f"{result_path}: no recordings attached (result['recorded'] is {rec_meta!r}); a "
            "harness self-test or --predict output is not evidence",
        )
    traces, recordings = [], []
    for i, m in enumerate(rec_meta):
        if not isinstance(m, dict) or not m.get("path") or not m.get("sha256"):
            raise Refusal("evidence-attached", f"recorded[{i}] lacks path and sha256: {m!r}")
        tpath = _resolve(m["path"], result_path.parent)
        if not tpath.exists():
            raise Refusal("evidence-attached", f"trace {tpath} (recorded[{i}]) does not exist")
        tsha = sha256_file(tpath)
        if tsha != m["sha256"]:
            raise Refusal(
                "hashes-match",
                f"trace {tpath} hashes to {tsha}, the result recorded {m['sha256']} -- the "
                "trace changed after it was scored",
            )
        trace = json.loads(tpath.read_bytes())
        src = trace.get("source", {}) if isinstance(trace, dict) else {}
        kind = src.get("kind") if isinstance(src, dict) else None
        if kind in SOURCE_KINDS_SYNTHETIC or (
            isinstance(src, dict) and ("truth_config" in src or "generator" in src)
        ):
            raise Refusal(
                "not-synthetic",
                f"trace {tpath} is synthetic (source.kind={kind!r}); a synthetic trace is "
                "never evidence",
            )
        if kind != "video":
            raise Refusal("not-synthetic", f"trace {tpath}: source.kind {kind!r} is not 'video'")
        try:
            validate_trace(trace, scen[sid]["kind"])
        except TraceSchemaError as e:
            raise Refusal("trace-valid", f"trace {tpath}: {e}") from e
        if trace.get("scenario_id") not in (None, sid):
            raise Refusal(
                "evidence-attached",
                f"trace {tpath} was extracted for {trace.get('scenario_id')!r}, result is {sid!r}",
            )
        vpath = _resolve(src["video_path"], tpath.parent)
        if not vpath.exists():
            raise Refusal(
                "evidence-attached",
                f"video {vpath} behind trace {tpath} does not exist -- keep the recording",
            )
        if not looks_like_video(vpath):
            raise Refusal(
                "video-container",
                f"{vpath} does not start like an mp4/mov/mkv/webm/avi container",
            )
        vsha = sha256_file(vpath)
        if vsha != src["video_sha256"]:
            raise Refusal(
                "hashes-match",
                f"video {vpath} hashes to {vsha}, trace {tpath} recorded {src['video_sha256']}",
            )
        traces.append(trace)
        recordings.append(
            {
                "trace_path": _rel(tpath),
                "trace_sha256": tsha,
                "video_path": _rel(vpath),
                "video_sha256": vsha,
                "recording_id": src.get("recording_id"),
                "recorded_date": src.get("recorded_date"),
                "extraction_method": src.get("method"),
            }
        )
    return Bundle(result_path, hashlib.sha256(raw).hexdigest(), res, scen[sid], traces, recordings)


def check_decisive(b: Bundle) -> None:
    r = b.result
    if r.get("blocked"):
        raise Refusal("decisive", f"{b.path}: result is BLOCKED: {r['blocked']}")
    if r.get("decisive") is not True:
        raise Refusal(
            "decisive",
            f"{b.path}: not decisive (margin {r.get('margin')} < {r.get('margin_required')}); "
            "an inconclusive result is a result, not a promotion",
        )
    want = int(b.scenario.get("repetitions", 1))
    if len(b.traces) < want:
        raise Refusal(
            "decisive",
            f"{b.path}: {len(b.traces)} recording(s), protocol requires {want}; the "
            "scenario's self_test_expect claims were made at {want}",
        )


def check_reproducible(b: Bundle, doc: dict, backend) -> dict:
    """Re-rank the attached traces with the current harness and scenarios.  A result
    file is plain JSON: without this, editing `winner` would be a promotion."""
    pins = b.result.get("pins") or None
    try:
        again = harness.rank(b.traces, b.scenario, doc, backend, pins=pins)
    except ValueError as e:
        raise Refusal("reproducible", f"{b.path}: re-ranking failed: {e}") from e
    if again["winner"] != b.result.get("winner") or again["decisive"] != b.result.get("decisive"):
        raise Refusal(
            "reproducible",
            f"{b.path}: attached result says winner={b.result.get('winner')!r} "
            f"decisive={b.result.get('decisive')}, re-ranking now gives "
            f"winner={again['winner']!r} decisive={again['decisive']} "
            f"(margin {again['margin']:.4f}/{again['margin_required']:.4f})",
        )
    return again


def check_pins(b: Bundle, scen: dict, cal: dict) -> None:
    pins = b.result.get("pins") or {}
    for param, value in pins.items():
        pre = b.scenario.get("prerequisite_pins", {}).get(param)
        if pre is None:
            raise Refusal("pins-settled", f"{b.path}: pin {param} is not a declared prerequisite")
        keys = [
            p["key"] for p in scen[pre["scenario"]].get("promotes", []) if p.get("param") == param
        ]
        statuses = {k: node_of(cal, k).get("status") for k in keys}
        if not keys or any(s != "measured" for s in statuses.values()):
            raise Refusal(
                "pins-settled",
                f"{b.path}: pinned {param}={value!r} but its prerequisite "
                f"{pre['scenario']} is not measured in the registry ({statuses})",
            )
        settled = getattr(synth.base_config(cal), param)
        if settled != value:
            raise Refusal(
                "pins-settled",
                f"{b.path}: pinned {param}={value!r} but the registry settles it as {settled!r}",
            )


# --------------------------------------------------------------------------- #
# registry values                                                             #
# --------------------------------------------------------------------------- #


def node_of(cal: dict, dotted: str) -> dict:
    node = cal
    for part in dotted.split("."):
        if not isinstance(node, dict) or part not in node or part.startswith("$"):
            raise Refusal(
                "key-exists",
                f"registry has no key {dotted!r} (interface demand on the registry owner)",
            )
        node = node[part]
    if not isinstance(node, dict) or "value" not in node:
        raise Refusal("key-exists", f"registry entry {dotted!r} has no value field")
    return node


_ALLOWED_NAMES = ("SUBTILE_PER_TILE", "MILLITILE_PER_TILE", "TPS")


def _eval_formula(expr: str, cal: dict) -> int | str:
    """A value_map entry is either a literal registry value or an arithmetic formula
    over SUBTILE_PER_TILE, MILLITILE_PER_TILE and TPS (from time.TICK_MS), evaluated
    exactly.  A formula that is not an integer is refused: the engine stores ints."""
    tree = ast.parse(expr, mode="eval")
    if not any(isinstance(n, ast.BinOp) for n in ast.walk(tree)):
        return expr
    tick = node_of(cal, "time.TICK_MS")["value"]
    if 1000 % int(tick):
        raise Refusal("value-mapped", f"TICK_MS={tick} does not give an integer TPS")
    env = {
        "SUBTILE_PER_TILE": Fraction(int(node_of(cal, "representation.SUBTILE_PER_TILE")["value"])),
        "MILLITILE_PER_TILE": Fraction(
            int(node_of(cal, "representation.MILLITILE_PER_TILE")["value"])
        ),
        "TPS": Fraction(1000 // int(tick)),
    }

    def ev(n: ast.AST) -> Fraction:
        if isinstance(n, ast.Expression):
            return ev(n.body)
        if isinstance(n, ast.Constant) and isinstance(n.value, int):
            return Fraction(n.value)
        if isinstance(n, ast.Name) and n.id in env:
            return env[n.id]
        if isinstance(n, ast.BinOp) and isinstance(n.op, ast.Add | ast.Sub | ast.Mult | ast.Div):
            a, b = ev(n.left), ev(n.right)
            if isinstance(n.op, ast.Add):
                return a + b
            if isinstance(n.op, ast.Sub):
                return a - b
            if isinstance(n.op, ast.Mult):
                return a * b
            return a / b
        raise Refusal("value-mapped", f"formula {expr!r}: unsupported element {ast.dump(n)}")

    v = ev(tree)
    if v.denominator != 1:
        raise Refusal("value-mapped", f"formula {expr!r} = {v} is not an integer")
    return int(v)


def winner_value(b: Bundle, entry: dict, cal: dict):
    scn, winner = b.scenario, b.result["winner"]
    param = entry["param"]
    alts = harness._alternatives(scn["candidates"][winner])
    vals = {json.dumps(a.get(param), sort_keys=True) for a in alts}
    if len(vals) != 1 or json.loads(next(iter(vals))) is None:
        raise Refusal(
            "value-mapped",
            f"{scn['id']}: winner {winner!r} does not fix {param} to one value ({sorted(vals)})",
        )
    raw = json.loads(next(iter(vals)))
    vm = entry.get("value_map", {})
    if str(raw) not in vm:
        raise Refusal(
            "value-mapped",
            f"{scn['id']}: winner {winner!r} ({param}={raw!r}) has no registry value in "
            f"value_map {sorted(vm)} -- {entry.get('note', '')}",
        )
    mapped = vm[str(raw)]
    return _eval_formula(mapped, cal) if isinstance(mapped, str) else mapped


@dataclass
class Change:
    key: str
    new_value: object
    bundles: list[Bundle]


def plan_changes(bundles: list[Bundle], cal: dict) -> list[Change]:
    by_sid = {b.scenario["id"]: b for b in bundles}
    changes: dict[str, Change] = {}
    for b in bundles:
        scn = b.scenario
        if not scn.get("promotes"):
            raise Refusal(
                "key-exists",
                f"{scn['id']} promotes no registry key: {scn.get('promotes_missing_key', '')}",
            )
        for entry in scn["promotes"]:
            node_of(cal, entry["key"])
            value = winner_value(b, entry, cal)
            joint = entry.get("joint_with")
            if joint:
                if joint not in by_sid:
                    raise Refusal(
                        "joint-agrees",
                        f"{entry['key']} is promoted jointly by {scn['id']} and {joint}; pass "
                        f"both results in one invocation",
                    )
                jb = by_sid[joint]
                jentry = next(e for e in jb.scenario["promotes"] if e["key"] == entry["key"])
                jvalue = winner_value(jb, jentry, cal)
                if jvalue != value:
                    raise Refusal(
                        "joint-agrees",
                        f"{entry['key']}: {scn['id']} winner {b.result['winner']!r} maps to "
                        f"{value!r} but {joint} winner {jb.result['winner']!r} maps to "
                        f"{jvalue!r}; the registry has no candidate for that combination",
                    )
            prev = changes.get(entry["key"])
            if prev is not None:
                if prev.new_value != value:
                    raise Refusal(
                        "joint-agrees",
                        f"{entry['key']}: results disagree ({prev.new_value!r} vs {value!r})",
                    )
                if b not in prev.bundles:
                    prev.bundles.append(b)
                continue
            changes[entry["key"]] = Change(entry["key"], value, [b])
    return list(changes.values())


def dependencies(bundles: list[Bundle], key: str) -> set[str]:
    deps = set()
    for b in bundles:
        for e in b.scenario.get("promotes", []):
            if e["key"] == key:
                deps.update(e.get("depends_on", []))
    return deps


def apply_changes(
    cal: dict,
    changes: list[Change],
    bundles: list[Bundle],
    status: str,
    supersede: str | None,
    record_only: bool,
) -> dict:
    """Returns the new registry (a deep copy).  Changes whose dependencies are also
    being promoted are applied after them, so `--result s04 --result s01` works."""
    if status not in STATUS_RANK:
        raise Refusal("status-known", f"--status {status!r} is not one of {sorted(STATUS_RANK)}")
    new = copy.deepcopy(cal)
    keys = {c.key for c in changes}
    pending = list(changes)
    ordered: list[Change] = []
    while pending:
        ready = [
            c
            for c in pending
            if not (dependencies(c.bundles, c.key) & {p.key for p in pending if p is not c})
        ]
        if not ready:
            raise Refusal("dependencies-measured", f"dependency cycle among {sorted(keys)}")
        ordered += ready
        pending = [c for c in pending if c not in ready]
    now = _now()
    for c in ordered:
        node = node_of(new, c.key)
        cur_status = node.get("status")
        if cur_status not in STATUS_RANK:
            raise Refusal(
                "status-known",
                f"{c.key}: current status {cur_status!r} is not orderable; refusing to guess "
                "whether this is a downgrade",
            )
        evidence = {
            "id": hashlib.sha256("|".join(b.sha256 for b in c.bundles).encode()).hexdigest()[:16],
            "tool": f"oracle/calibrate.py v{TOOL_VERSION}",
            "promoted_utc": now,
            "applied": not record_only,
            "status_written": None if record_only else status,
            "value_indicated": c.new_value,
            "supersede_reason": supersede,
            "results": [
                {
                    "scenario_id": b.scenario["id"],
                    "path": _rel(b.path),
                    "sha256": b.sha256,
                    "winner": b.result["winner"],
                    "runner_up": b.result.get("runner_up"),
                    "margin": b.result.get("margin"),
                    "margin_required": b.result.get("margin_required"),
                    "n_recordings": len(b.traces),
                    "pins": b.result.get("pins") or {},
                    "backend": b.result.get("backend"),
                    "recordings": b.recordings,
                }
                for b in c.bundles
            ],
        }
        if record_only:
            node.setdefault("evidence", []).append(evidence)
            continue
        if status == "measured":
            for dep in sorted(dependencies(c.bundles, c.key)):
                dst = node_of(new, dep).get("status")
                if dst != "measured":
                    raise Refusal(
                        "dependencies-measured",
                        f"{c.key} depends on {dep}, which is {dst!r}; a measurement on top of "
                        "an unmeasured dependency is not a measurement. Promote the dependency "
                        "first, or write this at --status hypothesis (or --record-only)",
                    )
        if STATUS_RANK[status] < STATUS_RANK[cur_status]:
            raise Refusal(
                "no-downgrade",
                f"{c.key}: writing status {status!r} over {cur_status!r} would lower it",
            )
        if cur_status in PROTECTED and node.get("value") != c.new_value and not supersede:
            why = (
                "Two measurements disagree"
                if cur_status == "measured"
                else "An owner ruling is a direct observation of the live game, and an "
                "oracle result does not quietly outrank it"
            )
            raise Refusal(
                "no-silent-conflict",
                f"{c.key}: registry holds {cur_status.upper()} value {node.get('value')!r}; this "
                f"evidence indicates {c.new_value!r}. {why} -- find out why, then pass "
                "--supersede REASON if the new one wins",
            )
        node.setdefault("history", []).append(
            {
                "value": node.get("value"),
                "status": cur_status,
                "confidence": node.get("confidence"),
                "provenance": node.get("provenance"),
                "superseded_utc": now,
                "superseded_by_evidence": evidence["id"],
            }
        )
        node.setdefault("evidence", []).append(evidence)
        node["value"] = c.new_value
        node["status"] = status
        node["provenance"] = (
            f"oracle: {', '.join(b.scenario['id'] for b in c.bundles)} on {now[:10]}; "
            f"see evidence id {evidence['id']}"
        )
        if status == "measured":
            node["confidence"] = "high"
    return new


def serialize_registry(cal: dict) -> str:
    return json.dumps(cal, indent=2, ensure_ascii=False) + "\n"


def promote(
    result_paths: list[Path],
    registry: Path = REGISTRY,
    status: str = "measured",
    supersede: str | None = None,
    record_only: bool = False,
    dry_run: bool = False,
    backend_spec: str | None = None,
    out=sys.stdout,
) -> tuple[dict, str]:
    """Raises Refusal; returns (new registry, semantic diff text)."""
    doc, scen = harness.load_scenarios()
    backend = harness.load_backend(backend_spec)
    cal = json.loads(Path(registry).read_text(encoding="utf-8"))
    bundles = [load_bundle(p, scen) for p in result_paths]
    for b in bundles:
        want = getattr(backend, "name", type(backend).__name__)
        if b.result.get("backend") != want:
            raise Refusal(
                "reproducible",
                f"{b.path} was ranked by backend {b.result.get('backend')!r}; re-ranking with "
                f"{want!r} (pass --backend)",
            )
        check_decisive(b)
        check_pins(b, scen, cal)
        check_reproducible(b, doc, backend)
    changes = plan_changes(bundles, cal)
    new = apply_changes(cal, changes, bundles, status, supersede, record_only)
    diff = "".join(
        difflib.unified_diff(
            serialize_registry(cal).splitlines(keepends=True),
            serialize_registry(new).splitlines(keepends=True),
            fromfile=f"{registry} (current)",
            tofile=f"{registry} (promoted)",
        )
    )
    if not dry_run:
        tmp = Path(registry).with_suffix(".json.tmp")
        tmp.write_bytes(serialize_registry(new).encode("utf-8"))
        json.loads(tmp.read_text(encoding="utf-8"))  # never leave an unparsable registry
        os.replace(tmp, registry)
    return new, diff


# --------------------------------------------------------------------------- #
# self-test and plants -- a temp dir, never the real registry                 #
# --------------------------------------------------------------------------- #

PLANTS = {
    # name: (aimed gate, description)
    "synthetic": ("not-synthetic", "the trace is oracle/synth.py output, unrelabelled"),
    "missing_evidence": ("evidence-attached", "the result's recorded list is emptied"),
    "missing_video": ("evidence-attached", "the video file behind the trace is deleted"),
    "renamed_json_video": ("video-container", "the 'video' is a JSON file with an .mp4 name"),
    "tampered_trace": ("hashes-match", "one trace sample is edited after the result was written"),
    "tampered_result": ("reproducible", "the result's winner is edited to the other candidate"),
    "not_decisive": ("decisive", "the result is marked decisive=false"),
    "downgrade": ("no-downgrade", "the registry key already holds a higher status"),
    "dependency": ("dependencies-measured", "status 'measured' over an unmeasured TICK_MS"),
    "conflict": ("no-silent-conflict", "the key already holds a different MEASURED value"),
    "overrule_owner": (
        "no-silent-conflict",
        "the key holds an OWNER_RULING value and an oracle result disagrees with it. "
        "Ranking a ruling at 4 passes no-downgrade, so a guard that tests "
        "`cur_status == 'measured'` alone would let it be overwritten in silence",
    ),
}
SELF_TEST_SCENARIO = "S01_speed_unit"
SELF_TEST_TRUTH = "millitiles_per_50ms"
FAKE_MP4_HEADER = b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom"


@dataclass
class Fixture:
    root: Path
    registry: Path
    result: Path
    trace: Path
    video: Path
    status: str = "hypothesis"


def build_fixture(tmp: Path, synthetic: bool = False) -> Fixture:
    """A decisive S01 result over a trace that LOOKS like a real recording.  It is a
    synthetic render with its synthetic markers stripped next to a fake mp4 -- the
    forgery this tool cannot detect (module docstring).  It exists only in `tmp`."""
    tmp.mkdir(parents=True, exist_ok=True)
    doc, scen = harness.load_scenarios()
    backend = harness.load_backend(None)
    scn = scen[SELF_TEST_SCENARIO]
    cfg = harness.truth_rows(backend, doc, scn)[SELF_TEST_TRUTH][0][1]
    rec = harness.synth_recording(backend, doc, scn, cfg, harness._stable_seed("calibrate-fixture"))
    video = tmp / "rec-a.mp4"
    rng = random.Random(7)
    video.write_bytes(FAKE_MP4_HEADER + bytes(rng.getrandbits(8) for _ in range(4096)))
    if not synthetic:
        rec["source"] = {
            "kind": "video",
            "extractor": "oracle.extract_tracks",
            "extractor_version": 1,
            "method": "calibrate-self-test-fixture",
            "video_path": str(video),
            "video_sha256": sha256_file(video),
            "capture_fps": 60.0,
            "recording_id": "SELFTEST",
            "recorded_date": "2026-09-13",
        }
    trace = tmp / "rec-a.trace.json"
    trace.write_bytes((json.dumps(rec, indent=1) + "\n").encode("utf-8"))
    res = harness.rank(rec, scn, doc, backend)
    res["recorded"] = [{"path": str(trace), "sha256": sha256_file(trace)}]
    result = tmp / "rec-a.result.json"
    result.write_text(json.dumps(res, indent=1), encoding="utf-8")
    registry = tmp / "calibration.json"
    # REWIND THE KEY THIS FIXTURE PROMOTES, in the copy only.
    #
    # The self-test promotes time.SPEED_TO_SUBTILES_PER_TICK from an S01 result, and
    # on 2026-09-18 that key was measured on client 15.535.29: the shipped registry now
    # holds 18 at status `measured`. Promoting a key that already carries the value
    # and the status being promoted TO exercises none of the gates -- no history
    # entry, no conflict, no diff -- so the fixture would silently stop testing
    # anything. Rewinding it to the state the promotion came FROM (its own last
    # history entry) keeps the fixture a test of the MACHINERY rather than of what
    # the registry happens to say today, and it is taken from the file instead of
    # typed in so it cannot drift from it.
    reg = json.loads(REGISTRY.read_text(encoding="utf-8"))
    # Both the key and its DEPENDENCY: the dependencies-measured gate exists to
    # refuse a `measured` promotion on top of an unmeasured time.TICK_MS, and
    # TICK_MS was promoted in the same pass, so leaving it measured would silently
    # disarm that gate too.
    for path in (("time", "SPEED_TO_SUBTILES_PER_TICK"), ("time", "TICK_MS")):
        node = reg[path[0]][path[1]]
        hist = node.get("history") or []
        if node.get("status") != "measured" or not hist:
            continue
        prior = hist[-1]
        for k in ("value", "status", "confidence", "provenance"):
            if k in prior:
                node[k] = prior[k]
        rest = hist[:-1]
        if rest:
            node["history"] = rest
        else:
            node.pop("history", None)
        node.pop("evidence", None)
    registry.write_text(json.dumps(reg, indent=2, ensure_ascii=False) + chr(10), encoding="utf-8")
    return Fixture(tmp, registry, result, trace, video)


def _edit_json(path: Path, fn) -> None:
    before = path.read_bytes()
    d = json.loads(before)
    fn(d)
    after = (json.dumps(d, indent=1) + "\n").encode("utf-8")
    if after == before:
        raise AssertionError(f"plant edit of {path} changed nothing")
    path.write_bytes(after)


def plant(name: str, fx: Fixture) -> Fixture:
    """Break the fixture in `fx.root` for plant `name`; asserts each edit landed."""
    if name == "synthetic":
        return build_fixture(fx.root / "synthetic", synthetic=True)
    if name == "missing_evidence":
        _edit_json(fx.result, lambda d: d.__setitem__("recorded", []))
    elif name == "missing_video":
        fx.video.unlink()
        assert not fx.video.exists()
    elif name == "renamed_json_video":
        fx.video.write_bytes(fx.trace.read_bytes())

        def repoint(d):
            d["source"]["video_sha256"] = sha256_file(fx.video)

        _edit_json(fx.trace, repoint)
        _edit_json(
            fx.result, lambda d: d["recorded"][0].__setitem__("sha256", sha256_file(fx.trace))
        )
    elif name == "tampered_trace":
        _edit_json(fx.trace, lambda d: next(iter(d["units"].values()))["x"].__setitem__(0, 9.0))
    elif name == "tampered_result":

        def flip(d):
            d["winner"], d["runner_up"] = d["runner_up"], d["winner"]

        _edit_json(fx.result, flip)
    elif name == "not_decisive":
        _edit_json(fx.result, lambda d: d.__setitem__("decisive", False))
    elif name == "downgrade":
        _edit_json(
            fx.registry,
            lambda d: d["time"]["SPEED_TO_SUBTILES_PER_TICK"].__setitem__("status", "datamined"),
        )
    elif name == "dependency":
        fx.status = "measured"
    elif name == "conflict":

        def measured(d):
            d["time"]["TICK_MS"]["status"] = "measured"
            d["time"]["SPEED_TO_SUBTILES_PER_TICK"]["status"] = "measured"

        _edit_json(fx.registry, measured)
        fx.status = "measured"
    elif name == "overrule_owner":
        # The ruling is on the key the result is about, so the incoming value
        # differs from it; TICK_MS is promoted too so the plant lands on
        # no-silent-conflict and not on dependencies-measured first.
        def ruled(d):
            d["time"]["TICK_MS"]["status"] = "measured"
            d["time"]["SPEED_TO_SUBTILES_PER_TICK"]["status"] = "owner_ruling"

        _edit_json(fx.registry, ruled)
        fx.status = "measured"
    else:
        raise KeyError(name)
    return fx


def _attempt(fx: Fixture) -> tuple[bool, str | None, str]:
    try:
        _, diff = promote([fx.result], fx.registry, status=fx.status, dry_run=False, out=None)
    except Refusal as r:
        return False, r.gate, str(r)
    return True, None, diff


def run_plant(name: str, verbose: bool = True) -> int:
    real_before = sha256_file(REGISTRY)
    aimed, desc = PLANTS[name]
    with tempfile.TemporaryDirectory(prefix="calibrate-plant-") as td:
        tmp = Path(td)
        base = build_fixture(tmp / "baseline")
        if base.registry.resolve() == REGISTRY.resolve():
            raise AssertionError("fixture registry is the real registry")
        ok, gate_hit, msg = _attempt(base)
        if not ok:
            print(
                f"PLANT '{name}' INCONCLUSIVE -- baseline promotion refused ({msg})",
                file=sys.stderr,
            )
            return 1
        fx = plant(name, build_fixture(tmp / "planted"))
        ok, gate_hit, msg = _attempt(fx)
    if sha256_file(REGISTRY) != real_before:
        print("DEFECT: data/calibration.json changed during a plant run", file=sys.stderr)
        return 1
    if not ok:
        if gate_hit == aimed:
            if verbose:
                print(f"PLANT '{name}' LANDED -- refused on '{aimed}' ({desc}):\n   {msg}")
            return 0
        print(
            f"PLANT '{name}' DID NOT LAND -- refused, but on '{gate_hit}' not '{aimed}': {msg}",
            file=sys.stderr,
        )
        return 1
    print(
        f"PLANT '{name}' DID NOT LAND -- promotion ALLOWED with the defect ({desc})",
        file=sys.stderr,
    )
    return 1


def self_test() -> int:
    real_before = sha256_file(REGISTRY)
    with tempfile.TemporaryDirectory(prefix="calibrate-selftest-") as td:
        fx = build_fixture(Path(td))
        before = json.loads(fx.registry.read_text(encoding="utf-8"))
        ok, gate_hit, msg = _attempt(fx)
        if not ok:
            print(f"SELF-TEST RED: baseline refused on {gate_hit}: {msg}", file=sys.stderr)
            return 1
        after = json.loads(fx.registry.read_text(encoding="utf-8"))
        node = after["time"]["SPEED_TO_SUBTILES_PER_TICK"]
        old = before["time"]["SPEED_TO_SUBTILES_PER_TICK"]
        checks = {
            "value is the millitile multiplier (18 at 20 TPS)": node["value"] == 18,
            "status unchanged at hypothesis": node["status"] == "hypothesis",
            "history keeps the previous value": node["history"][-1]["value"] == old["value"],
            "history keeps the previous status": node["history"][-1]["status"] == old["status"],
            "evidence attached": node["evidence"][-1]["results"][0]["winner"] == SELF_TEST_TRUTH,
            "nothing else changed": {k: v for k, v in after.items() if k != "time"}
            == {k: v for k, v in before.items() if k != "time"},
        }
    rc = 0
    for name, good in checks.items():
        print(f"  {'ok ' if good else 'RED'} {name}")
        rc |= 0 if good else 1
    for name in PLANTS:
        rc |= run_plant(name)
    if sha256_file(REGISTRY) != real_before:
        print("DEFECT: data/calibration.json changed during the self-test", file=sys.stderr)
        return 1
    print(f"calibrate self-test {'green' if rc == 0 else 'RED'}: 1 baseline, {len(PLANTS)} plants")
    return rc


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--result", type=Path, action="append", help="harness result JSON (repeatable)")
    ap.add_argument("--registry", type=Path, default=REGISTRY)
    ap.add_argument("--status", default="measured", choices=sorted(STATUS_RANK))
    ap.add_argument("--supersede", metavar="REASON")
    ap.add_argument("--record-only", action="store_true", help="attach evidence, change nothing")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--backend")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--plant", choices=[*sorted(PLANTS), "all"])
    args = ap.parse_args(argv)
    if args.self_test:
        return self_test()
    if args.plant:
        names = sorted(PLANTS) if args.plant == "all" else [args.plant]
        return max(run_plant(n) for n in names)
    if not args.result:
        ap.print_usage(sys.stderr)
        print("need --result (or --self-test / --plant)", file=sys.stderr)
        return 2
    try:
        _, diff = promote(
            args.result,
            args.registry,
            args.status,
            args.supersede,
            args.record_only,
            args.dry_run,
            args.backend,
        )
    except Refusal as r:
        print(f"REFUSED {r}", file=sys.stderr)
        return 1
    print(diff or "(no change)")
    print(("DRY RUN: would write " if args.dry_run else "wrote ") + str(args.registry))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
