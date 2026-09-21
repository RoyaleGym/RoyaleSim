#!/usr/bin/env python3
"""Screen recording of the real game -> per-frame unit tracks in tile coordinates.

WHAT IT DOES
    1. CALIBRATE.  Fits a ground-plane homography (pixel -> tile) from landmarks
       whose tile coordinates come from data/derived/arena.json, never typed here:
       the eight corners where the two bridges meet the two river banks, plus (for
       manual clicking) the arena outer corners and both king platforms.  `auto`
       finds the bridge corners from the river colour; `manual` shows a frame and
       asks a person to click each landmark.  The result, its landmarks and its
       reprojection residual go to a sidecar JSON next to the video
       (<video>.homography.json), keyed by the video sha256.

       THE RESIDUAL IS NOT THE ACCURACY.  The eight bridge corners sit on two rows
       two tiles apart: they fit themselves to 0.008 tile and still leave the ground
       position uncertain far from the river.  Measured on a rendered perspective
       arena (tests/test_oracle_extract_tracks.py), positional uncertainty over each
       scenario measurement region, per landmark set:
         bridges only, 0.5 px detection     0.135 - 0.314 tile   REFUSED
         + red-side corners and king        0.083 - 0.199 tile   refused for 3 of 4
         + any near-side landmark set       0.032 - 0.073 tile   accepted
       `track` therefore recomputes that uncertainty for the scenario at hand and
       refuses above synth.CaptureModel.offset_sigma_tiles (0.10), the homography
       error the harness self-test assumed.  Click the far landmarks.
    2. TRACK.  Background-subtracts against the empty arena at the start of the
       recording, finds foreground blobs inside the projected arena, takes each
       blob's foot point, projects it to tiles, and associates blobs with the
       scenario's scripted units (label, card, where and when it was placed).
       `annotate` is the manual fallback: a person clicks the unit's feet every
       few frames, which is slow and always works.
    3. WRITE.  An oracle-trace/1 JSON, the exact input tools/diff_harness.py reads
       (validated by `validate_trace`, which the harness also calls), carrying
       the video's path and sha256 so oracle/calibrate.py can verify the chain
       video -> trace -> harness result before it promotes anything.

THE HARD LIMIT -- READ THIS BEFORE BELIEVING A TRACK
    The client renders INTERPOLATED positions between logic ticks, draws sprites
    whose feet are not their logic position, and the phone captures frames on its
    own clock.  Video therefore NEVER recovers per-tick truth: not the tick a unit
    moved on, not a sub-tick rounding rule, not Supercell's PRNG.  What it resolves
    is (a) categorical questions -- which lane, whether a unit bent before a
    building or after touching it, which unit gave way -- and (b) quantities where
    the candidates differ by more than the capture error budget: the 20% speed
    divisor question, a 0.5 s vs 0.05 s repath lag over ten trials.  The budget the
    harness self-test assumes is oracle/synth.py:CaptureModel (per-frame jitter
    0.06 tile, homography offset 0.10 tile, sprite foot bias 0.08 tile, ~1080p
    60 fps).  Those numbers are ARGUED, NOT MEASURED; replace them from the
    residuals of the first real recording.

WHY IT EXISTS
    The oracle is the only instrument here measured against reality, and this is how
    it reads reality: from ordinary screen recordings of play.

WHAT IT CANNOT CATCH
    * Two units whose sprites overlap (the S05 push ladder at contact): blobs merge
      and the tracker holds the last separate positions.  Use `annotate` there.
    * A homography that fits its landmarks and is wrong elsewhere (a lens or a
      camera that is not a pinhole).  The residual only speaks for the landmarks.
    * Water-colour thresholds were written without a real frame to test them on;
      `auto` refuses when its residual is large, and `manual` always remains.
    * A `--pixel-sigma` nobody measured: the uncertainty gate is only as honest as
      that estimate (0.5 px auto, 1.5 px manual).  Re-estimate it by clicking one
      landmark ten times and taking the spread.
    * Anything under the HUD (elixir bar, card hand, emotes).

USAGE
    python oracle/extract_tracks.py landmarks
    python oracle/extract_tracks.py validate trace.json [--kind trajectory]
    python oracle/extract_tracks.py calibrate rec.mp4 [--method auto|manual] [--frame-s 0.5]
    python oracle/extract_tracks.py track rec.mp4 --scenario S01_speed_unit \\
        --recording-id REC-A-1 --recorded-date 2026-09-14 --out rec-a-1.trace.json
    python oracle/extract_tracks.py annotate rec.mp4 --scenario S09_repath_lag --out t.json
    python oracle/extract_tracks.py events rec.mp4 --scenario S04_tick_rate_phase_lock --out t.json

    `landmarks` and `validate` need only numpy.  Everything that decodes video
    needs OpenCV, imported lazily so this module (and the harness that imports its
    schema) loads without it.

    Exit codes: 0 ok, 1 defect (invalid trace, residual over
    budget, a scripted unit never found), 2 usage error or missing dependency.
"""

from __future__ import annotations

import argparse
import datetime as _dt
import hashlib
import itertools
import json
import math
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
for _p in (ROOT, ROOT / "tools"):
    if str(_p) not in sys.path:
        sys.path.insert(0, str(_p))

TRACE_FORMAT = "oracle-trace/1"  # must equal oracle.synth.TRACE_FORMAT (a test checks)
HOMOGRAPHY_FORMAT = "oracle-homography/1"
EXTRACTOR_VERSION = 1
ARENA = ROOT / "data" / "derived" / "arena.json"
SCENARIOS = ROOT / "oracle" / "scenarios.json"

SOURCE_KINDS_SYNTHETIC = frozenset({"sim", "synthetic_recording"})
SOURCE_KINDS_REAL = frozenset({"video"})
TEAMS = frozenset({"blue", "red"})
# A tile coordinate this far outside the 18x32 arena is not a small calibration
# error; it is pixels, or a flipped axis, wearing a tile's name.
COORD_SLACK_TILES = 3.0

CV2_INSTALL_MESSAGE = (
    "OpenCV is not installed in this interpreter. Video decoding, manual calibration and "
    "tracking need it. Install it into the project venv with:\n"
    "    .venv/Scripts/python.exe -m pip install opencv-python\n"
    "(`landmarks` and `validate` work without it.)"
)


class TraceSchemaError(ValueError):
    """A trace that tools/diff_harness.py would misread."""


class MissingDependency(RuntimeError):
    pass


def _cv2():
    try:
        import cv2
    except ImportError as e:  # pragma: no cover - depends on the interpreter
        raise MissingDependency(CV2_INSTALL_MESSAGE) from e
    return cv2


# --------------------------------------------------------------------------- #
# schema -- what diff_harness.py consumes                                     #
# --------------------------------------------------------------------------- #


def _finite(v: Any) -> bool:
    return isinstance(v, int | float) and not isinstance(v, bool) and math.isfinite(v)


def _arena_bounds() -> tuple[float, float]:
    try:
        tiles = json.loads(ARENA.read_text(encoding="utf-8"))["tiles"]
        return float(tiles[0]), float(tiles[1])
    except (OSError, KeyError, ValueError):
        # validate_trace must not depend on generated data being present; the
        # bound only guards against pixels-as-tiles, so the 2018 size is enough
        return 18.0, 32.0


def validate_trace(doc: Any, kind: str | None = None) -> None:
    """Raise TraceSchemaError unless `doc` is an oracle-trace/1 document that
    tools/diff_harness.py reads correctly for a scenario of `kind`.

    Fields the harness reads, and so this checks:
      format, time_unit ("s"), space_unit ("tile"), scenario_id (str|None)
      source.kind, source.capture_fps (event_phase), source.recording_id/recorded_date
      units[label] = {card, team, t[], x[], y[]}        trajectory
      events[] = {name, t, unit?}                       event_phase, lag scenarios
      spawn_positions[] = [x, y]                        deploy_lattice
      observed_placements[label] = {tile: [x, y], t}    optional, trajectory
    A real (source.kind == "video") trace must also say which video it came from."""
    if not isinstance(doc, dict):
        raise TraceSchemaError("trace is not a JSON object")
    if doc.get("format") != TRACE_FORMAT:
        raise TraceSchemaError(f"format {doc.get('format')!r}, expected {TRACE_FORMAT}")
    if doc.get("time_unit") != "s" or doc.get("space_unit") != "tile":
        raise TraceSchemaError(
            f"units must be seconds and tiles, got time_unit={doc.get('time_unit')!r} "
            f"space_unit={doc.get('space_unit')!r}"
        )
    sid = doc.get("scenario_id")
    if sid is not None and not isinstance(sid, str):
        raise TraceSchemaError(f"scenario_id must be a string or null, got {sid!r}")
    src = doc.get("source")
    if not isinstance(src, dict) or not isinstance(src.get("kind"), str):
        raise TraceSchemaError("source.kind missing")
    if src["kind"] not in SOURCE_KINDS_REAL | SOURCE_KINDS_SYNTHETIC:
        raise TraceSchemaError(f"source.kind {src['kind']!r} is not one of video/sim/synthetic")
    fps = src.get("capture_fps")
    if fps is not None and not (_finite(fps) and fps > 0):
        raise TraceSchemaError(f"source.capture_fps {fps!r} is not a positive number")
    if src["kind"] == "video":
        for f in ("video_path", "video_sha256", "capture_fps", "extractor", "method"):
            if not src.get(f):
                raise TraceSchemaError(f"a video trace must carry source.{f}")
        vs = str(src["video_sha256"])
        if len(vs) != 64 or any(c not in "0123456789abcdef" for c in vs):
            raise TraceSchemaError(f"source.video_sha256 {vs!r} is not a lowercase sha256")
    W, H = _arena_bounds()

    def check_xy(x: Any, y: Any, where: str) -> None:
        if not (_finite(x) and _finite(y)):
            raise TraceSchemaError(f"{where}: non-finite coordinate ({x!r}, {y!r})")
        if not (-COORD_SLACK_TILES <= x <= W + COORD_SLACK_TILES) or not (
            -COORD_SLACK_TILES <= y <= H + COORD_SLACK_TILES
        ):
            raise TraceSchemaError(
                f"{where}: ({x}, {y}) is outside the {W:g}x{H:g} tile arena by more than "
                f"{COORD_SLACK_TILES} tiles -- pixels or a flipped axis, not tiles"
            )

    units = doc.get("units")
    if not isinstance(units, dict):
        raise TraceSchemaError("units must be an object (label -> track)")
    for lbl, u in units.items():
        if not isinstance(lbl, str) or not lbl:
            raise TraceSchemaError(f"unit label {lbl!r} is not a non-empty string")
        if not isinstance(u, dict):
            raise TraceSchemaError(f"units[{lbl}] is not an object")
        if not isinstance(u.get("card"), str) or u.get("team") not in TEAMS:
            raise TraceSchemaError(f"units[{lbl}] needs card (str) and team (blue|red)")
        t, x, y = u.get("t"), u.get("x"), u.get("y")
        if not all(isinstance(v, list) for v in (t, x, y)):
            raise TraceSchemaError(f"units[{lbl}] t/x/y must be lists")
        if not len(t) == len(x) == len(y):
            raise TraceSchemaError(f"units[{lbl}] t/x/y lengths differ: {len(t)}/{len(x)}/{len(y)}")
        for i, (ti, xi, yi) in enumerate(zip(t, x, y, strict=True)):
            if not _finite(ti):
                raise TraceSchemaError(f"units[{lbl}].t[{i}] = {ti!r} is not finite")
            check_xy(xi, yi, f"units[{lbl}][{i}]")
        # The harness interpolates with np.interp, which silently returns garbage
        # for a decreasing abscissa.  Capture timestamps jitter, so allow a tiny
        # step back but not a reordered track.
        if any(b < a - 0.01 for a, b in itertools.pairwise(t)):
            raise TraceSchemaError(f"units[{lbl}].t is not time-ordered")
    events = doc.get("events")
    if not isinstance(events, list):
        raise TraceSchemaError("events must be a list")
    for i, e in enumerate(events):
        if not isinstance(e, dict) or not isinstance(e.get("name"), str) or not _finite(e.get("t")):
            raise TraceSchemaError(f"events[{i}] needs name (str) and finite t: {e!r}")
        if "unit" in e and e["unit"] is not None and not isinstance(e["unit"], str):
            raise TraceSchemaError(f"events[{i}].unit must be a string")
    if "spawn_positions" in doc:
        sp = doc["spawn_positions"]
        if not isinstance(sp, list):
            raise TraceSchemaError("spawn_positions must be a list of [x, y]")
        for i, p in enumerate(sp):
            if not (isinstance(p, list) and len(p) == 2):
                raise TraceSchemaError(f"spawn_positions[{i}] is not [x, y]")
            check_xy(p[0], p[1], f"spawn_positions[{i}]")
    obs = doc.get("observed_placements")
    if obs is not None:
        if not isinstance(obs, dict):
            raise TraceSchemaError("observed_placements must be an object")
        for lbl, o in obs.items():
            if (
                not isinstance(o, dict)
                or not isinstance(o.get("tile"), list)
                or len(o["tile"]) != 2
            ):
                raise TraceSchemaError(f"observed_placements[{lbl}] needs tile [x, y]")
            check_xy(o["tile"][0], o["tile"][1], f"observed_placements[{lbl}]")
            if o.get("t") is not None and not _finite(o["t"]):
                raise TraceSchemaError(f"observed_placements[{lbl}].t is not finite")
    # vacuity guards per scenario kind: an extraction that found nothing must not
    # read like a recording in which nothing happened.
    if kind == "trajectory":
        if not units:
            raise TraceSchemaError("trajectory trace has no units")
        short = [lbl for lbl, u in units.items() if len(u["t"]) < 3]
        if short:
            raise TraceSchemaError(f"units with fewer than 3 samples: {short}")
    elif kind == "event_phase":
        if len(events) < 2:
            raise TraceSchemaError(f"event_phase trace has {len(events)} events; needs >= 2")
    elif kind == "deploy_lattice":
        if len(doc.get("spawn_positions", [])) < 8:
            raise TraceSchemaError("deploy_lattice trace needs >= 8 spawn_positions")
    elif kind is not None:
        raise TraceSchemaError(f"unknown scenario kind {kind!r}")


def write_trace(path: Path, doc: dict, kind: str | None = None) -> str:
    """Validate, then write.  Returns the sha256 of the bytes written, which is what
    the harness records and calibrate.py re-checks."""
    validate_trace(doc, kind)
    data = (json.dumps(doc, indent=1) + "\n").encode("utf-8")
    Path(path).write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path, chunk: int = 1 << 20) -> str:
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        while block := f.read(chunk):
            h.update(block)
    return h.hexdigest()


# --------------------------------------------------------------------------- #
# arena landmarks -- derived, never typed                              #
# --------------------------------------------------------------------------- #


def load_arena(path: Path = ARENA) -> dict:
    if not path.exists():
        raise FileNotFoundError(
            f"{path} missing -- run tools/extract_arena.py (data/derived is generated)"
        )
    return json.loads(path.read_text(encoding="utf-8"))


def river_edges(arena: dict) -> tuple[float, float]:
    """(y of the Blue-side bank, y of the Red-side bank) in tiles."""
    half = float(arena["half_tiles_per_tile"])
    lo, hi = arena["water_half_rows"]
    return lo / half, (hi + 1) / half


def arena_landmarks(
    arena: dict | None = None, extended: bool = False
) -> dict[str, tuple[float, float]]:
    """Ground-plane points a person or the auto-calibrator can find on screen.

    Bridge-bank corners are flat painted ground, so their screen position does not
    depend on sprite height; tower centres are NOT offered because a tower's base
    centre is hidden under a 3D sprite.  Names say which bank (blue = the
    recording player's own side, at the BOTTOM of the screen).

    The eight bridge corners lie on TWO rows two tiles apart, so on their own they
    pin the perspective along y poorly: measured on a rendered perspective arena
    with 1-pixel detection, 0.33 tile error at the far ends of the arena while the
    landmarks themselves fit to < 0.08 (tests/test_oracle_extract_tracks.py).
    `extended` adds the arena's outer corners and both king platforms' corners
    (arena.json `tiles` and `king_blocks`) for manual clicking; skip any the HUD or
    a tower sprite hides."""
    arena = arena or load_arena()
    y_blue, y_red = river_edges(arena)
    bridges = sorted(arena["bridges"], key=lambda b: b["x_min"])
    if len(bridges) != 2:
        raise ValueError(f"expected 2 bridges in arena.json, found {len(bridges)}")
    out = {}
    for side, br in zip(("left", "right"), bridges, strict=True):
        for bank, y in (("blue", y_blue), ("red", y_red)):
            out[f"{side}_bridge_{bank}_bank_outer"] = (
                float(br["x_min"] if side == "left" else br["x_max"]),
                y,
            )
            out[f"{side}_bridge_{bank}_bank_inner"] = (
                float(br["x_max"] if side == "left" else br["x_min"]),
                y,
            )
    if extended:
        W, Ht = float(arena["tiles"][0]), float(arena["tiles"][1])
        for bank, y in (("blue", 0.0), ("red", Ht)):
            out[f"arena_corner_{bank}_left"] = (0.0, y)
            out[f"arena_corner_{bank}_right"] = (W, y)
        for kb in sorted(arena["king_blocks"], key=lambda k: k["y_min"]):
            team = "blue" if kb["y_min"] < Ht / 2 else "red"
            for vert, y in (("near", kb["y_min"]), ("far", kb["y_max"])):
                out[f"king_platform_{team}_{vert}_left"] = (float(kb["x_min"]), float(y))
                out[f"king_platform_{team}_{vert}_right"] = (float(kb["x_max"]), float(y))
    return out


# --------------------------------------------------------------------------- #
# homography -- pure numpy so it is testable without OpenCV                   #
# --------------------------------------------------------------------------- #


def _normaliser(pts: np.ndarray) -> np.ndarray:
    c = pts.mean(axis=0)
    d = np.sqrt(((pts - c) ** 2).sum(axis=1)).mean()
    s = math.sqrt(2) / d if d > 0 else 1.0
    return np.array([[s, 0, -s * c[0]], [0, s, -s * c[1]], [0, 0, 1]], float)


def fit_homography(src: np.ndarray, dst: np.ndarray) -> np.ndarray:
    """Normalised DLT: H with dst ~ H @ [src, 1].  Needs >= 4 points, no three
    collinear.  src = pixels, dst = tiles, for the pixel -> tile map."""
    src, dst = np.asarray(src, float), np.asarray(dst, float)
    if src.shape != dst.shape or src.ndim != 2 or src.shape[1] != 2 or len(src) < 4:
        raise ValueError(f"need >= 4 point pairs, got {src.shape} / {dst.shape}")
    Ts, Td = _normaliser(src), _normaliser(dst)
    s = (Ts @ np.c_[src, np.ones(len(src))].T).T
    d = (Td @ np.c_[dst, np.ones(len(dst))].T).T
    rows = []
    for (x, y, _), (u, v, _) in zip(s, d, strict=True):
        rows.append([-x, -y, -1, 0, 0, 0, u * x, u * y, u])
        rows.append([0, 0, 0, -x, -y, -1, v * x, v * y, v])
    _, sv, vt = np.linalg.svd(np.asarray(rows))
    if sv[-2] < 1e-9 * sv[0]:
        raise ValueError("degenerate landmark configuration (collinear points?)")
    Hn = vt[-1].reshape(3, 3)
    H = np.linalg.inv(Td) @ Hn @ Ts
    return H / H[2, 2]


def apply_homography(H: np.ndarray, pts: np.ndarray) -> np.ndarray:
    pts = np.asarray(pts, float).reshape(-1, 2)
    p = (H @ np.c_[pts, np.ones(len(pts))].T).T
    return p[:, :2] / p[:, 2:3]


def reprojection_rms(H: np.ndarray, src: np.ndarray, dst: np.ndarray) -> float:
    e = apply_homography(H, src) - np.asarray(dst, float)
    return float(np.sqrt((e**2).sum(axis=1).mean()))


# --------------------------------------------------------------------------- #
# automatic calibration from the river                                        #
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class WaterRule:
    """A pixel is water when blue dominates red by `b_minus_r` and green is not far
    below blue.  UNVERIFIED on a real frame (see module docstring); tune with
    --water-* flags, and `auto` still refuses when the residual is large."""

    b_minus_r: int = 40
    b_min: int = 90
    g_minus_b_min: int = -120  # rejects deep navy/purple UI; river water is blue-green
    band_fraction: float = 0.5  # a river row has >= this fraction of the densest row's water
    edge_inset_px: int = 2


# WaterRule is frozen, so one shared default instance is safe to use as a default argument.
DEFAULT_WATER_RULE = WaterRule()


def water_mask(frame_rgb: np.ndarray, rule: WaterRule = DEFAULT_WATER_RULE) -> np.ndarray:
    f = frame_rgb.astype(np.int16)
    r, g, b = f[..., 0], f[..., 1], f[..., 2]
    return (b - r >= rule.b_minus_r) & (b >= rule.b_min) & (g - b >= rule.g_minus_b_min)


def _runs(row: np.ndarray) -> list[tuple[int, int]]:
    """[start, end) of True runs in a 1-D boolean array."""
    padded = np.r_[False, row, False].astype(np.int8)
    d = np.diff(padded)
    return list(zip(np.nonzero(d == 1)[0].tolist(), np.nonzero(d == -1)[0].tolist(), strict=True))


def auto_landmarks(
    frame_rgb: np.ndarray, arena: dict | None = None, rule: WaterRule = DEFAULT_WATER_RULE
) -> dict[str, tuple[float, float]]:
    """Pixel positions of the eight bridge-bank corners, found from the river band.
    Raises ValueError with the reason when the frame does not look like it."""
    arena = arena or load_arena()
    m = water_mask(frame_rgb, rule)
    frac = m.mean(axis=1)
    if frac.max() <= 0.05:
        raise ValueError(f"no river found: densest water row covers {frac.max():.1%} of the width")
    rows = np.nonzero(frac >= rule.band_fraction * frac.max())[0]
    # the longest contiguous band of river rows
    splits = np.split(rows, np.nonzero(np.diff(rows) > 1)[0] + 1)
    band = max(splits, key=len)
    if len(band) < 4:
        raise ValueError(f"river band only {len(band)} px tall")
    top, bottom = int(band[0]), int(band[-1]) + 1  # screen rows of the two bank edges
    out: dict[str, tuple[float, float]] = {}
    # screen top = the RED bank (far side), screen bottom = the BLUE bank
    for bank, edge_y, probe in (
        ("red", top, top + rule.edge_inset_px),
        ("blue", bottom, bottom - 1 - rule.edge_inset_px),
    ):
        runs = [r for r in _runs(m[probe]) if r[1] - r[0] >= 3]
        if len(runs) < 3:
            raise ValueError(f"{bank} bank row {probe}: {len(runs)} water runs, need 3 (2 bridges)")
        # the two widest gaps between consecutive water runs are the bridges
        gaps = sorted(
            (
                (runs[i + 1][0] - runs[i][1], runs[i][1], runs[i + 1][0])
                for i in range(len(runs) - 1)
            ),
            reverse=True,
        )[:2]
        (_, l0, l1), (_, r0, r1) = sorted(gaps, key=lambda g: g[1])
        out[f"left_bridge_{bank}_bank_outer"] = (float(l0), float(edge_y))
        out[f"left_bridge_{bank}_bank_inner"] = (float(l1), float(edge_y))
        out[f"right_bridge_{bank}_bank_inner"] = (float(r0), float(edge_y))
        out[f"right_bridge_{bank}_bank_outer"] = (float(r1), float(edge_y))
    return out


def homography_from_landmarks(
    pixels: dict[str, tuple[float, float]], arena: dict | None = None
) -> tuple[np.ndarray, float, list[dict]]:
    tiles = arena_landmarks(arena, extended=True)
    names = [n for n in tiles if n in pixels]
    if len(names) < 4:
        raise ValueError(f"only {len(names)} landmarks located; a homography needs >= 4")
    src = np.array([pixels[n] for n in names], float)
    dst = np.array([tiles[n] for n in names], float)
    H = fit_homography(src, dst)
    rms = reprojection_rms(H, src, dst)
    marks = [{"name": n, "tile": list(tiles[n]), "pixel": list(pixels[n])} for n in names]
    return H, rms, marks


def homography_uncertainty(
    marks: list[dict],
    points_tiles: np.ndarray,
    pixel_sigma: float,
    n: int = 64,
    seed: int = 0,
) -> float:
    """Largest standard deviation (tiles) of the ground position of `points_tiles`
    when every landmark's pixel is re-drawn with `pixel_sigma` of localisation error
    and the homography refitted.  The residual cannot say this: eight collinear-ish
    landmarks fit themselves perfectly and extrapolate badly."""
    src = np.array([m["pixel"] for m in marks], float)
    dst = np.array([m["tile"] for m in marks], float)
    H = fit_homography(src, dst)
    pts_px = apply_homography(np.linalg.inv(H), np.asarray(points_tiles, float))
    rng = np.random.default_rng(seed)
    outs = []
    for _ in range(n):
        Hk = fit_homography(src + rng.normal(0.0, pixel_sigma, src.shape), dst)
        outs.append(apply_homography(Hk, pts_px))
    sd = np.std(np.stack(outs), axis=0)  # (points, 2)
    return float(np.sqrt((sd**2).sum(axis=1)).max())


def scenario_region(
    scn: dict, arena: dict, margin_tiles: float = 1.0, step: float = 1.0
) -> np.ndarray | None:
    """Tile points where a scenario is measured, or None when it is measured in time
    only (event_phase).  Trajectory: the box around every position the scenario's
    first candidate predicts for any unit (the walk to the enemy tower included),
    grown by `margin_tiles`.  Deploy lattice: the tap box.  Clipped to the arena."""
    W, Ht = float(arena["tiles"][0]), float(arena["tiles"][1])
    if scn["kind"] == "event_phase":
        return None
    if scn["kind"] == "deploy_lattice":
        su = scn["setup"]
        xs = [v / 100 for v in su["tap_x_100"]]
        ys = [v / 100 for v in su["tap_y_100"]]
    else:
        import diff_harness as harness

        doc = json.loads(SCENARIOS.read_text(encoding="utf-8"))
        be = harness.load_backend(None)
        cfg = next(iter(harness.candidate_configs(be, doc, scn).values()))[0][1]
        tr = harness.simulate_scenario(be, scn, cfg)
        xs = [x for u in tr["units"].values() for x in u["x"]]
        ys = [y for u in tr["units"].values() for y in u["y"]]
        xs += [a["tile_100"][0] / 100 for _, a in _actions(scn)]
        ys += [a["tile_100"][1] / 100 for _, a in _actions(scn)]
    if not xs:
        raise ValueError(f"{scn['id']}: no positions to bound its measurement region")
    x0, x1 = max(0.0, min(xs) - margin_tiles), min(W, max(xs) + margin_tiles)
    y0, y1 = max(0.0, min(ys) - margin_tiles), min(Ht, max(ys) + margin_tiles)
    gx, gy = np.meshgrid(np.arange(x0, x1 + 1e-9, step), np.arange(y0, y1 + 1e-9, step))
    return np.c_[gx.ravel(), gy.ravel()]


def _actions(scn: dict):
    if "variants" in scn:
        for v in scn["variants"]:
            for a in v["actions"]:
                yield v["name"], a
    else:
        for a in scn.get("setup", {}).get("actions", []):
            yield None, a


def homography_sidecar_path(video: Path) -> Path:
    return Path(str(video) + ".homography.json")


def save_homography(
    path: Path, H: np.ndarray, rms: float, marks: list[dict], video_sha: str, meta: dict
) -> str:
    doc = {
        "format": HOMOGRAPHY_FORMAT,
        "video_sha256": video_sha,
        "H_pixel_to_tile": np.asarray(H).tolist(),
        "reprojection_rms_tiles": rms,
        "landmarks": marks,
        "created_utc": _dt.datetime.now(_dt.UTC).isoformat(timespec="seconds"),
        **meta,
    }
    data = (json.dumps(doc, indent=1) + "\n").encode("utf-8")
    Path(path).write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def load_homography(path: Path, video_sha: str | None = None) -> tuple[np.ndarray, dict]:
    doc = json.loads(Path(path).read_text(encoding="utf-8"))
    if doc.get("format") != HOMOGRAPHY_FORMAT:
        raise ValueError(f"{path}: not an {HOMOGRAPHY_FORMAT} sidecar")
    if video_sha is not None and doc.get("video_sha256") != video_sha:
        raise ValueError(
            f"{path}: calibrated for video {doc.get('video_sha256')}, not {video_sha}; "
            "re-calibrate (the phone may have moved or the file was re-encoded)"
        )
    return np.asarray(doc["H_pixel_to_tile"], float), doc


# --------------------------------------------------------------------------- #
# detection and association -- numpy, testable on synthetic frames            #
# --------------------------------------------------------------------------- #


def label_components(mask: np.ndarray, max_iter: int = 10_000) -> tuple[np.ndarray, int]:
    """4-connected component labels by iterated min-propagation.  Slow on a
    full-resolution frame; track() uses OpenCV's labeller when it is available and
    this one on the downsampled mask otherwise (and in the tests)."""
    h, w = mask.shape
    big = h * w + 1
    lab = np.where(mask, np.arange(h * w).reshape(h, w) + 1, big).astype(np.int64)
    for _ in range(max_iter):
        prev = lab
        p = np.pad(lab, 1, constant_values=big)
        nb = np.minimum.reduce(
            [p[1:-1, 1:-1], p[:-2, 1:-1], p[2:, 1:-1], p[1:-1, :-2], p[1:-1, 2:]]
        )
        lab = np.where(mask, nb, big)
        if np.array_equal(lab, prev):
            break
    uniq = np.unique(lab[mask])
    remap = {int(v): i + 1 for i, v in enumerate(uniq)}
    out = np.zeros((h, w), np.int32)
    for v, i in remap.items():
        out[lab == v] = i
    return out, len(uniq)


@dataclass(frozen=True)
class Blob:
    foot_px: tuple[float, float]
    area_px: int
    bbox: tuple[int, int, int, int]  # x0, y0, x1, y1 (exclusive)


def detect_blobs(
    frame_rgb: np.ndarray,
    background_rgb: np.ndarray,
    roi: np.ndarray | None = None,
    diff_threshold: int = 45,
    min_area_px: int = 60,
    downsample: int = 2,
    labeller=None,
) -> list[Blob]:
    """Foreground blobs against an empty-arena background.  The foot point is the
    bottom-centre of the blob: sprites stand on their shadow, so the lowest
    foreground row is the nearest thing video has to the logic position (its
    residual bias is CaptureModel.foot_bias_sigma_tiles)."""
    diff = np.abs(frame_rgb.astype(np.int16) - background_rgb.astype(np.int16)).sum(axis=2)
    fg = diff >= diff_threshold
    if roi is not None:
        fg &= roi
    k = max(1, int(downsample))
    if k > 1:
        h, w = fg.shape
        fg = fg[: h - h % k, : w - w % k].reshape(h // k, k, w // k, k).any(axis=(1, 3))
    labels, n = labeller(fg) if labeller is not None else label_components(fg)
    blobs = []
    for i in range(1, n + 1):
        ys, xs = np.nonzero(labels == i)
        area = len(xs) * k * k
        if area < min_area_px:
            continue
        x0, x1, y0, y1 = xs.min() * k, (xs.max() + 1) * k, ys.min() * k, (ys.max() + 1) * k
        blobs.append(
            Blob(((x0 + x1) / 2.0, float(y1)), int(area), (int(x0), int(y0), int(x1), int(y1)))
        )
    return blobs


@dataclass
class ScriptedUnit:
    label: str  # as the harness names it ("hog", or "near_after.hog" for variants)
    card: str
    team: str
    tile: tuple[float, float]
    t_s: float
    is_building: bool


def scripted_units(scn: dict, variant: str | None = None) -> list[ScriptedUnit]:
    from oracle import synth

    if "variants" in scn:
        names = [v["name"] for v in scn["variants"]]
        if variant not in names:
            raise ValueError(f"{scn['id']} has variants {names}; pass --variant")
        v = next(v for v in scn["variants"] if v["name"] == variant)
        acts, prefix = v["actions"], f"{variant}."
    else:
        acts, prefix = scn.get("setup", {}).get("actions", []), ""
    out = []
    for a in acts:
        st = synth.card_stats(a["card"])
        out.append(
            ScriptedUnit(
                label=prefix + a["label"],
                card=a["card"],
                team=a["team"],
                tile=(a["tile_100"][0] / 100.0, a["tile_100"][1] / 100.0),
                t_s=a["t_ms"] / 1000.0,
                is_building=bool(st["is_building"]),
            )
        )
    return out


@dataclass
class TrackState:
    unit: ScriptedUnit
    t: list
    x: list
    y: list
    first_t: float | None = None


def associate(
    tracks: list[TrackState],
    detections: list[tuple[float, float]],
    t: float,
    t_zero: float | None,
    spawn_radius_tiles: float = 1.5,
    gate_tiles: float = 0.6,
) -> None:
    """Greedy nearest-first association, one detection per track per frame.

    A track that has not started claims a NEW detection near its scripted spawn
    tile once the recording has reached its scripted time (relative to the first
    placement, t_zero).  A started track claims the nearest detection within
    gate_tiles of its last position: at 60 fps the fastest candidate speed moves a
    unit 0.04 tile per frame, so 0.6 tile tolerates dropped frames and blob jitter
    without letting a track jump to a neighbour."""
    used: set[int] = set()
    pairs = []
    for ti, tr in enumerate(tracks):
        if tr.first_t is None:
            if t_zero is not None and t + 0.25 < t_zero + tr.unit.t_s:
                continue
            ref, lim = tr.unit.tile, spawn_radius_tiles
        else:
            ref, lim = (tr.x[-1], tr.y[-1]), gate_tiles
        for di, (dx, dy) in enumerate(detections):
            d = math.hypot(dx - ref[0], dy - ref[1])
            if d <= lim:
                pairs.append((d, ti, di))
    taken_tracks: set[int] = set()
    for _, ti, di in sorted(pairs):
        if ti in taken_tracks or di in used:
            continue
        tr = tracks[ti]
        if tr.first_t is None:
            tr.first_t = t
        tr.t.append(t)
        tr.x.append(detections[di][0])
        tr.y.append(detections[di][1])
        taken_tracks.add(ti)
        used.add(di)


def settle_position(
    tr: TrackState, after_s: float = 0.3, until_s: float = 0.8
) -> tuple[float, float]:
    """Where a unit was placed: the median position over [first+0.3 s, first+0.8 s],
    after the deploy drop animation and before the unit starts walking (every
    thin-slice troop deploys for 1 s in the 2018 data)."""
    sel = [
        (x, y)
        for t, x, y in zip(tr.t, tr.x, tr.y, strict=True)
        if tr.first_t is not None and tr.first_t + after_s <= t <= tr.first_t + until_s
    ] or list(zip(tr.x[:5], tr.y[:5], strict=False))
    arr = np.asarray(sel, float)
    return float(np.median(arr[:, 0])), float(np.median(arr[:, 1]))


def build_trajectory_trace(
    scn: dict,
    tracks: list[TrackState],
    source: dict,
) -> dict:
    """Tracks -> oracle-trace/1.  Time is re-zeroed so the earliest placement sits at
    its scripted t_ms, because the harness only searches +-time_offset_search_s for
    the recording-clock offset."""
    started = [tr for tr in tracks if tr.first_t is not None]
    if not started:
        raise ValueError("no scripted unit was ever detected")
    anchor = min(started, key=lambda tr: tr.first_t)
    zero = anchor.first_t - anchor.unit.t_s
    units, events, placements = {}, [], {}
    for tr in tracks:
        if tr.first_t is None:
            continue
        px, py = settle_position(tr)
        placements[tr.unit.label] = {"tile": [px, py], "t": tr.first_t - zero}
        events.append({"name": "spawn", "unit": tr.unit.label, "t": tr.first_t - zero})
        if tr.unit.is_building:
            continue
        units[tr.unit.label] = {
            "card": tr.unit.card,
            "team": tr.unit.team,
            "t": [t - zero for t in tr.t],
            "x": list(tr.x),
            "y": list(tr.y),
        }
    return {
        "format": TRACE_FORMAT,
        "scenario_id": scn["id"],
        "source": dict(source, time_zero_video_s=zero),
        "time_unit": "s",
        "space_unit": "tile",
        "units": units,
        "events": sorted(events, key=lambda e: e["t"]),
        "observed_placements": placements,
    }


# --------------------------------------------------------------------------- #
# video plumbing (OpenCV)                                                      #
# --------------------------------------------------------------------------- #


def _open(video: Path):
    cv2 = _cv2()
    cap = cv2.VideoCapture(str(video))
    if not cap.isOpened():
        raise ValueError(f"OpenCV cannot open {video}")
    return cv2, cap


def read_frames(video: Path, start_s: float = 0.0, end_s: float | None = None):
    """Yield (t_seconds, frame_rgb) using the container's per-frame timestamps, so a
    variable-frame-rate phone recording still gets true times."""
    cv2, cap = _open(video)
    try:
        cap.set(cv2.CAP_PROP_POS_MSEC, start_s * 1000.0)
        while True:
            ok, bgr = cap.read()
            if not ok:
                break
            t = cap.get(cv2.CAP_PROP_POS_MSEC) / 1000.0
            if end_s is not None and t > end_s:
                break
            yield t, cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)
    finally:
        cap.release()


def frame_at(video: Path, t_s: float) -> np.ndarray:
    for _, f in read_frames(video, t_s):
        return f
    raise ValueError(f"{video}: no frame at {t_s} s")


def interval_stats(times: list[float]) -> dict:
    if len(times) < 3:
        return {"n": len(times)}
    dt = np.diff(np.asarray(times)) * 1000.0
    med = float(np.median(dt))
    return {
        "n": len(times),
        "median_ms": med,
        "p1_ms": float(np.percentile(dt, 1)),
        "p99_ms": float(np.percentile(dt, 99)),
        "fraction_off_by_20pct": float(np.mean(np.abs(dt - med) > 0.2 * med)),
    }


def video_source(
    video: Path, args: argparse.Namespace, method: str, hsha: str | None, hdoc: dict
) -> dict:
    return {
        "kind": "video",
        "extractor": "oracle.extract_tracks",
        "extractor_version": EXTRACTOR_VERSION,
        "method": method,
        "video_path": str(Path(video).resolve()),
        "video_sha256": sha256_file(video),
        "video_bytes": Path(video).stat().st_size,
        "recording_id": getattr(args, "recording_id", None),
        "recorded_date": getattr(args, "recorded_date", None),
        "homography": {
            "path": str(homography_sidecar_path(video)),
            "sha256": hsha,
            "method": hdoc.get("method"),
            "reprojection_rms_tiles": hdoc.get("reprojection_rms_tiles"),
            "uncertainty_tiles_over_scenario": hdoc.get("uncertainty_tiles"),
            "uncertainty_budget_tiles": hdoc.get("uncertainty_budget_tiles"),
        },
    }


def projected_roi(H_px_to_tile: np.ndarray, shape: tuple[int, int], arena: dict) -> np.ndarray:
    """Pixels whose ground point falls inside the arena rectangle."""
    h, w = shape
    ys, xs = np.mgrid[0:h, 0:w]
    tiles = apply_homography(H_px_to_tile, np.c_[xs.ravel(), ys.ravel()])
    W, Ht = float(arena["tiles"][0]), float(arena["tiles"][1])
    inside = (tiles[:, 0] >= 0) & (tiles[:, 0] <= W) & (tiles[:, 1] >= 0) & (tiles[:, 1] <= Ht)
    return inside.reshape(h, w)


# --------------------------------------------------------------------------- #
# subcommands                                                                  #
# --------------------------------------------------------------------------- #


def _load_scenario(sid: str) -> dict:
    doc = json.loads(SCENARIOS.read_text(encoding="utf-8"))
    for s in doc["scenarios"]:
        if s["id"] == sid:
            return s
    raise KeyError(f"unknown scenario {sid!r}")


def cmd_landmarks(args: argparse.Namespace) -> int:
    marks = arena_landmarks()
    for name, (x, y) in marks.items():
        print(f"  {name:34s} tile ({x:5.2f}, {y:5.2f})")
    print(f"{len(marks)} landmarks from {ARENA}")
    return 0 if len(marks) == 8 else 1


def cmd_validate(args: argparse.Namespace) -> int:
    try:
        validate_trace(json.loads(Path(args.trace).read_text(encoding="utf-8")), args.kind)
    except (TraceSchemaError, json.JSONDecodeError) as e:
        print(f"INVALID {args.trace}: {e}", file=sys.stderr)
        return 1
    print(f"valid {TRACE_FORMAT}: {args.trace}")
    return 0


def _manual_click(frame_rgb: np.ndarray, names: list[str]) -> dict[str, tuple[float, float]]:
    cv2 = _cv2()
    shown = cv2.cvtColor(frame_rgb, cv2.COLOR_RGB2BGR)
    clicks: dict[str, tuple[float, float]] = {}
    state = {"i": 0}

    def on_mouse(event, x, y, *_):
        if event == cv2.EVENT_LBUTTONDOWN and state["i"] < len(names):
            clicks[names[state["i"]]] = (float(x), float(y))
            cv2.circle(shown, (x, y), 5, (0, 0, 255), 2)
            state["i"] += 1

    win = "calibrate: click the named landmark, s = not visible, q = done"
    cv2.namedWindow(win, cv2.WINDOW_NORMAL)
    cv2.setMouseCallback(win, on_mouse)
    while state["i"] < len(names):
        view = shown.copy()
        cv2.putText(
            view, names[state["i"]], (20, 40), cv2.FONT_HERSHEY_SIMPLEX, 1.0, (0, 255, 255), 2
        )
        cv2.imshow(win, view)
        key = cv2.waitKey(30) & 0xFF
        if key == ord("s"):
            state["i"] += 1
        elif key == ord("q"):
            break
    cv2.destroyWindow(win)
    return clicks


def cmd_calibrate(args: argparse.Namespace) -> int:
    video = Path(args.video)
    arena = load_arena()
    frame = frame_at(video, args.frame_s)
    if args.method == "auto":
        rule = WaterRule(b_minus_r=args.water_b_minus_r, b_min=args.water_b_min)
        try:
            pixels = auto_landmarks(frame, arena, rule)
        except ValueError as e:
            print(f"AUTO CALIBRATION FAILED: {e}\n  retry with --method manual", file=sys.stderr)
            return 1
    else:
        pixels = _manual_click(frame, list(arena_landmarks(arena, extended=True)))
    H, rms, marks = homography_from_landmarks(pixels, arena)
    if rms > args.max_rms:
        print(
            f"REFUSED: reprojection rms {rms:.3f} tile > {args.max_rms} on {len(marks)} landmarks",
            file=sys.stderr,
        )
        return 1
    out = Path(args.out) if args.out else homography_sidecar_path(video)
    save_homography(
        out,
        H,
        rms,
        marks,
        sha256_file(video),
        {
            "method": args.method,
            "frame_s": args.frame_s,
            "image_size": list(frame.shape[:2]),
            "pixel_sigma": _pixel_sigma(args),
            "pixel_sigma_note": "localisation error ESTIMATE per landmark, not measured",
        },
    )
    print(f"homography ({args.method}, {len(marks)} landmarks, rms {rms:.3f} tile) -> {out}")
    return 0


# Pixel localisation error per landmark, by method.  ESTIMATES, not measured: the
# auto detector quantises a bank edge to a whole pixel; a person clicking a corner
# on a phone frame is assumed to land within a couple of pixels.
PIXEL_SIGMA_DEFAULT = {"auto": 0.5, "manual": 1.5}


def _pixel_sigma(args: argparse.Namespace) -> float:
    v = getattr(args, "pixel_sigma", None)
    return float(v) if v is not None else PIXEL_SIGMA_DEFAULT[args.method]


class CalibrationTooUncertain(ValueError):
    pass


def check_calibration(hdoc: dict, scn: dict, arena: dict, accept: bool = False) -> float:
    """The homography's positional uncertainty over the region this scenario is
    measured in, refused above synth.CaptureModel.offset_sigma_tiles: the harness
    self-test only proved discrimination under THAT homography error, so a
    calibration worse than it voids the self-test's claims for this recording."""
    from oracle import synth

    budget = synth.CaptureModel().offset_sigma_tiles
    sigma = float(hdoc.get("pixel_sigma", PIXEL_SIGMA_DEFAULT.get(hdoc.get("method"), 1.5)))
    region = scenario_region(scn, arena)
    if region is None:  # measured in time only; position error cannot move a timestamp
        hdoc["uncertainty_tiles"] = None
        return 0.0
    unc = homography_uncertainty(hdoc["landmarks"], region, sigma)
    hdoc["uncertainty_tiles"] = unc
    hdoc["uncertainty_budget_tiles"] = budget
    if unc > budget and not accept:
        raise CalibrationTooUncertain(
            f"homography uncertainty {unc:.3f} tile over {scn['id']}'s region exceeds the "
            f"{budget} tile capture budget the self-test assumed ({len(hdoc['landmarks'])} "
            "landmarks). Re-calibrate with --method manual and click the far landmarks "
            "(arena corners, king platforms), or pass --accept-calibration-uncertainty and "
            "treat the result as unvalidated"
        )
    return unc


def _homography_for(
    video: Path, args: argparse.Namespace, scn: dict
) -> tuple[np.ndarray, str, dict]:
    path = Path(args.homography) if args.homography else homography_sidecar_path(video)
    if not path.exists():
        raise FileNotFoundError(f"{path} missing -- run `calibrate` first")
    H, hdoc = load_homography(path, sha256_file(video))
    check_calibration(hdoc, scn, load_arena(), args.accept_calibration_uncertainty)
    return H, sha256_file(path), hdoc


def cmd_track(args: argparse.Namespace) -> int:
    cv2 = _cv2()
    video = Path(args.video)
    scn = _load_scenario(args.scenario)
    if scn["kind"] != "trajectory":
        print(f"{scn['id']} is {scn['kind']}; use `events` or `annotate`", file=sys.stderr)
        return 2
    arena = load_arena()
    H, hsha, hdoc = _homography_for(video, args, scn)
    units = scripted_units(scn, args.variant)
    frames = read_frames(video, 0.0, args.end_s)
    bg_stack, times = [], []
    tracks = [TrackState(u, [], [], []) for u in units]
    roi = None
    t_zero = None

    def cv_label(mask):
        n, lab = cv2.connectedComponents(mask.astype(np.uint8), connectivity=4)
        return lab, n - 1

    for t, frame in frames:
        times.append(t)
        if t < args.background_s:
            bg_stack.append(frame)
            continue
        if roi is None:
            if not bg_stack:
                print("no background frames before --background-s", file=sys.stderr)
                return 1
            background = np.median(np.stack(bg_stack[:: max(1, len(bg_stack) // 15)]), axis=0)
            roi = projected_roi(H, frame.shape[:2], arena)
        blobs = detect_blobs(
            frame, background, roi, args.diff_threshold, args.min_area_px, 2, cv_label
        )
        dets = (
            [tuple(p) for p in apply_homography(H, np.array([b.foot_px for b in blobs]))]
            if blobs
            else []
        )
        if t_zero is None and dets:
            t_zero = t  # the first thing to appear is the first scripted placement
        associate(tracks, dets, t, None if t_zero is None else t_zero - min(u.t_s for u in units))
    lost = [tr.unit.label for tr in tracks if tr.first_t is None]
    if lost:
        print(f"TRACKING FAILED: never found {lost}; use `annotate`", file=sys.stderr)
        return 1
    src = video_source(video, args, "auto_track", hsha, hdoc)
    src["capture_fps"] = 1000.0 / interval_stats(times)["median_ms"]
    src["frame_intervals"] = interval_stats(times)
    if args.variant:
        src["variant"] = args.variant
    trace = build_trajectory_trace(scn, tracks, src)
    sha = write_trace(Path(args.out), trace, "trajectory")
    print(f"wrote {args.out} ({len(trace['units'])} units, sha256 {sha})")
    return 0


def cmd_annotate(args: argparse.Namespace) -> int:
    """Manual fallback: step through frames, click each labelled unit's feet.
    Keys: space = next step, b = back, n = next label, e = mark event here, q = save."""
    cv2 = _cv2()
    video = Path(args.video)
    scn = _load_scenario(args.scenario)
    H, hsha, hdoc = _homography_for(video, args, scn)
    frames = list(read_frames(video, args.start_s, args.end_s))
    if not frames:
        print("no frames in range", file=sys.stderr)
        return 1
    units = scripted_units(scn, args.variant) if scn["kind"] == "trajectory" else []
    labels = [u.label for u in units] or ["point"]
    clicks: dict[str, list] = {lbl: [] for lbl in labels}
    events: list[dict] = []
    spawn_points: list[list[float]] = []
    state = {"f": 0, "li": 0}
    win = "annotate: click feet | space next  b back  n next label  e event  q save"

    def on_mouse(event, x, y, *_):
        if event != cv2.EVENT_LBUTTONDOWN:
            return
        t = frames[state["f"]][0]
        tx, ty = apply_homography(H, np.array([[x, y]], float))[0]
        if scn["kind"] == "deploy_lattice":
            spawn_points.append([float(tx), float(ty)])
        else:
            clicks[labels[state["li"]]].append((t, float(tx), float(ty)))

    cv2.namedWindow(win, cv2.WINDOW_NORMAL)
    cv2.setMouseCallback(win, on_mouse)
    while True:
        t, f = frames[state["f"]]
        view = cv2.cvtColor(f, cv2.COLOR_RGB2BGR)
        cv2.putText(
            view,
            f"{labels[state['li']]}  t={t:.3f}s",
            (20, 40),
            cv2.FONT_HERSHEY_SIMPLEX,
            1.0,
            (0, 255, 255),
            2,
        )
        cv2.imshow(win, view)
        key = cv2.waitKey(0) & 0xFF
        if key == ord(" "):
            state["f"] = min(len(frames) - 1, state["f"] + args.step)
        elif key == ord("b"):
            state["f"] = max(0, state["f"] - args.step)
        elif key == ord("n"):
            state["li"] = (state["li"] + 1) % len(labels)
        elif key == ord("e"):
            ev = {"name": scn.get("setup", {}).get("event_name", "spawn"), "t": t}
            if units:
                ev["unit"] = labels[state["li"]]
            events.append(ev)
        elif key == ord("q"):
            break
    cv2.destroyAllWindows()
    src = video_source(video, args, "manual_annotation", hsha, hdoc)
    src["capture_fps"] = 1000.0 / interval_stats([t for t, _ in frames])["median_ms"]
    if scn["kind"] == "trajectory":
        tracks = []
        for u in units:
            pts = sorted(clicks[u.label])
            tr = TrackState(u, [p[0] for p in pts], [p[1] for p in pts], [p[2] for p in pts])
            tr.first_t = pts[0][0] if pts else None
            tracks.append(tr)
        trace = build_trajectory_trace(scn, tracks, src)
        zero = trace["source"]["time_zero_video_s"]
        trace["events"] = sorted(
            [*trace["events"], *({**e, "t": e["t"] - zero} for e in events)], key=lambda e: e["t"]
        )
    else:
        trace = {
            "format": TRACE_FORMAT,
            "scenario_id": scn["id"],
            "source": src,
            "time_unit": "s",
            "space_unit": "tile",
            "units": {},
            "events": events,
        }
        if scn["kind"] == "deploy_lattice":
            trace["spawn_positions"] = spawn_points
    sha = write_trace(Path(args.out), trace, scn["kind"])
    print(f"wrote {args.out} (sha256 {sha})")
    return 0


def cmd_events(args: argparse.Namespace) -> int:
    """S04/S07 automatic mode: every NEW blob that persists for `persist` frames is a
    deployment; its first frame time is an `appear` event and its settled foot
    point is a spawn position.  Uses real per-frame timestamps (event_phase needs
    them)."""
    cv2 = _cv2()
    video = Path(args.video)
    scn = _load_scenario(args.scenario)
    if scn["kind"] not in ("event_phase", "deploy_lattice"):
        print(f"{scn['id']} is {scn['kind']}; use `track`", file=sys.stderr)
        return 2
    arena = load_arena()
    H, hsha, hdoc = _homography_for(video, args, scn)

    def cv_label(mask):
        n, lab = cv2.connectedComponents(mask.astype(np.uint8), connectivity=4)
        return lab, n - 1

    bg_stack, times, roi, background = [], [], None, None
    live: list[dict] = []
    events, spawns = [], []
    for t, frame in read_frames(video, 0.0, args.end_s):
        times.append(t)
        if t < args.background_s:
            bg_stack.append(frame)
            continue
        if roi is None:
            background = np.median(np.stack(bg_stack[:: max(1, len(bg_stack) // 15)]), axis=0)
            roi = projected_roi(H, frame.shape[:2], arena)
        blobs = detect_blobs(
            frame, background, roi, args.diff_threshold, args.min_area_px, 2, cv_label
        )
        pts = (
            apply_homography(H, np.array([b.foot_px for b in blobs])) if blobs else np.zeros((0, 2))
        )
        for p in pts:
            near = [lv for lv in live if math.hypot(lv["x"][-1] - p[0], lv["y"][-1] - p[1]) < 0.8]
            if near:
                near[0]["x"].append(float(p[0]))
                near[0]["y"].append(float(p[1]))
                near[0]["t"].append(t)
            else:
                live.append({"t0": t, "t": [t], "x": [float(p[0])], "y": [float(p[1])]})
    for lv in live:
        if len(lv["t"]) < args.persist:
            continue
        events.append({"name": scn.get("setup", {}).get("event_name", "appear"), "t": lv["t0"]})
        sel = [
            (x, y)
            for t, x, y in zip(lv["t"], lv["x"], lv["y"], strict=True)
            if lv["t0"] + 0.3 <= t <= lv["t0"] + 0.8
        ]
        if sel:
            arr = np.asarray(sel)
            spawns.append([float(np.median(arr[:, 0])), float(np.median(arr[:, 1]))])
    src = video_source(video, args, "auto_events", hsha, hdoc)
    src["capture_fps"] = 1000.0 / interval_stats(times)["median_ms"]
    src["frame_intervals"] = interval_stats(times)
    trace = {
        "format": TRACE_FORMAT,
        "scenario_id": scn["id"],
        "source": src,
        "time_unit": "s",
        "space_unit": "tile",
        "units": {},
        "events": sorted(events, key=lambda e: e["t"]),
    }
    if scn["kind"] == "deploy_lattice":
        trace["spawn_positions"] = spawns
    sha = write_trace(Path(args.out), trace, scn["kind"])
    print(f"wrote {args.out} ({len(events)} events, sha256 {sha})")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("landmarks")
    v = sub.add_parser("validate")
    v.add_argument("trace")
    v.add_argument("--kind", choices=["trajectory", "event_phase", "deploy_lattice"])
    c = sub.add_parser("calibrate")
    c.add_argument("video")
    c.add_argument("--method", choices=["auto", "manual"], default="auto")
    c.add_argument("--frame-s", type=float, default=0.5)
    c.add_argument("--max-rms", type=float, default=0.15, help="refuse above this tile residual")
    c.add_argument("--pixel-sigma", type=float, help="landmark localisation error estimate, px")
    c.add_argument("--water-b-minus-r", type=int, default=WaterRule.b_minus_r)
    c.add_argument("--water-b-min", type=int, default=WaterRule.b_min)
    c.add_argument("--out")
    for name in ("track", "annotate", "events"):
        p = sub.add_parser(name)
        p.add_argument("video")
        p.add_argument("--scenario", required=True)
        p.add_argument("--variant")
        p.add_argument("--homography")
        p.add_argument("--recording-id")
        p.add_argument("--recorded-date")
        p.add_argument("--out", required=True)
        p.add_argument("--end-s", type=float)
        p.add_argument("--background-s", type=float, default=1.0)
        p.add_argument("--diff-threshold", type=int, default=45)
        p.add_argument("--min-area-px", type=int, default=60)
        p.add_argument("--start-s", type=float, default=0.0)
        p.add_argument("--step", type=int, default=6, help="annotate: frames per keypress")
        p.add_argument("--persist", type=int, default=20, help="events: frames a deploy must last")
        p.add_argument("--accept-calibration-uncertainty", action="store_true")
    args = ap.parse_args(argv)
    handlers = {
        "landmarks": cmd_landmarks,
        "validate": cmd_validate,
        "calibrate": cmd_calibrate,
        "track": cmd_track,
        "annotate": cmd_annotate,
        "events": cmd_events,
    }
    try:
        return handlers[args.cmd](args)
    except MissingDependency as e:
        print(f"SKIPPED: {e}", file=sys.stderr)
        return 2
    except (FileNotFoundError, KeyError, ValueError) as e:
        print(f"ERROR: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
