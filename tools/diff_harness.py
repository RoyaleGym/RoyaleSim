#!/usr/bin/env python3
"""The differential scorer: which candidate model config best explains a recording?

WHAT IT CHECKS
    Given a recorded trace (from oracle/extract_tracks.py, or synthetic) and a
    scenario from oracle/scenarios.json, it simulates EVERY candidate config of that
    scenario through a pluggable backend, scores divergence against the recording,
    profiles out nuisance parameters, and prints a ranked table.  It declares a
    winner only when the winner beats the runner-up by a stated margin; otherwise
    the verdict is INCONCLUSIVE, which is a result, not a failure to produce one.

    Scores by scenario kind:
      trajectory      mean / max / rms tile error after a fitted common time offset
                      (the recording clock is not the battle clock), plus named
                      observables (time-between-crossings, x-at-y, closest approach,
                      deviation onset) reported beside the ranking.
      event_phase     Rayleigh phase coherence of event timestamps at each candidate
                      tick period, compared with Monte-Carlo predictions per candidate.
      deploy_lattice  negative log-likelihood of spawn positions under each candidate
                      snap lattice, phase and noise profiled out.
      lag_likelihood  (a trajectory scenario ranked by reaction lag) likelihood of each
                      trial's lag under the lag set a candidate predicts over its
                      unobservable per-trial phase.

    --self-test renders synthetic truth from EVERY candidate x EVERY alternative x
    EVERY global nuisance value (not just the first row), for several seeds, and
    requires the true candidate back out as a decisive winner.  Each scenario records
    in `self_test_expect` which candidates it is EXPECTED to recover and, for the
    ones it cannot, which gate goes red; a claim the run contradicts in EITHER
    direction fails the self-test.

WHY IT EXISTS
    Pathfinding can be made to "look right" by reasoning for a long time, against a
    game Supercell has since changed, with no instrument that can say "that model is
    wrong".  This is that instrument.
    It is the acceptance gate for promoting any movement constant:
    oracle/calibrate.py refuses a promotion whose evidence is not a decisive result
    from this tool on a REAL recording.

WHAT IT CANNOT CATCH
    * A candidate nobody wrote down.  If the real game is none of the candidates,
      the least-wrong one still wins; read the winner's absolute error against the
      capture error budget before believing it (the table prints both).
    * Per-tick truth.  The client interpolates between ticks; see
      oracle/extract_tracks.py for the error budget and what is in reach.
    * Its own backend's bugs: synth scoring synth only proves the harness can
      discriminate, not that the synth model is the game.
    * A capture error budget that is wrong.  synth.CaptureModel is argued, not
      measured; the self-test is only as hard as that budget.

PREREQUISITE PINS
    A scenario may declare `prerequisite_pins` {param: {scenario: ...}}: a nuisance
    that must be SETTLED by an earlier recording, because leaving it free lets a
    wrong candidate win decisively (measured for S09 and speed_unit, see rank()).
    A recorded run without the pin is ranked for information but marked
    `blocked` and never decisive.  Pins come from --pin param=value, or from
    data/calibration.json when every registry key the prerequisite promotes is
    already `measured`.

OBSERVED PLACEMENTS
    A person cannot tap tile (7.0, 9.5) exactly.  A recorded trace may carry
    `observed_placements` {label: {tile: [x, y], t: seconds}} (extract_tracks.py
    writes it from each unit's first detection); candidates are then simulated from
    where the units really appeared, not where the protocol asked for them.

USAGE
    python tools/diff_harness.py --self-test                     # all scenarios, 3 seeds
    python tools/diff_harness.py --self-test --scenario S01_speed_unit
    python tools/diff_harness.py --plant all                     # prove each gate goes red
    python tools/diff_harness.py --predict S03_building_lookahead
    python tools/diff_harness.py --scenario S01_speed_unit --recorded rec.json --out result.json
    python tools/diff_harness.py --scenario S09_repath_lag --recorded r*.json
        --pin speed_unit=tiles_per_minute        # one command, wrapped here
    python tools/diff_harness.py ... --backend mypkg.mymod:Backend   # drop-in simulator

    Exit codes: 0 clean, 1 defect / gate red / plant did not
    land, 2 usage error.  A recorded run exits 0 for a decisive result and 1 for an
    inconclusive or blocked one, so an && chain cannot walk past "we could not tell".

BACKEND PROTOCOL
    An object with:
      simulate(scenario: dict, config: ModelConfig) -> trace dict (oracle-trace/1)
      snap_lattice(config) -> (period_tiles, [offset_tiles]) | None
      base_config(**candidate_only) -> ModelConfig
    oracle.synth.SynthBackend is the reference implementation.  The Rust engine
    drops in by implementing the same three calls over royalesim.
"""

from __future__ import annotations

import argparse
import cmath
import copy
import datetime as _dt
import hashlib
import importlib
import itertools
import json
import math
import random
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from oracle import synth  # noqa: E402
from oracle.extract_tracks import TraceSchemaError, validate_trace  # noqa: E402

SCENARIOS = ROOT / "oracle" / "scenarios.json"
RESULT_FORMAT = "oracle-harness-result/1"
SELF_TEST_SEEDS = (11, 22, 33)
PHASE_MC_RUNS = 8
GATES = ("winner-is-expected", "decisive-margin", "vacuity", "prerequisite-pinned")


# --------------------------------------------------------------------------- #
# scenario plumbing                                                           #
# --------------------------------------------------------------------------- #


def load_scenarios(path: Path = SCENARIOS) -> tuple[dict, dict]:
    doc = json.loads(path.read_text(encoding="utf-8"))
    scen = {s["id"]: s for s in doc["scenarios"]}
    if len(scen) < 8:
        # vacuity guard: a truncated scenarios file would let --self-test pass
        # having tested nothing.
        raise SystemExit(f"{path}: only {len(scen)} scenarios parsed; expected >= 8")
    return doc, scen


def load_backend(spec: str | None):
    if not spec:
        return synth.SynthBackend()
    mod, _, attr = spec.partition(":")
    obj = getattr(importlib.import_module(mod), attr or "Backend")
    return obj() if isinstance(obj, type) else obj


def _alternatives(v) -> list[dict]:
    return v if isinstance(v, list) else [v]


def candidate_configs(
    backend, doc: dict, scn: dict, pins: dict | None = None
) -> dict[str, list[tuple[dict, synth.ModelConfig]]]:
    """candidate name -> [(assignment, config)] over alternatives x nuisance grid.
    A pinned nuisance contributes only its pinned value."""
    co = {k: v for k, v in doc["defaults"]["candidate_only"].items() if not k.startswith("$")}
    base = backend.base_config(**co).with_overrides(scn.get("base_overrides", {}))
    nuis = dict(scn.get("nuisance", {}))
    for p, v in (pins or {}).items():
        if p not in nuis:
            raise ValueError(f"{scn['id']}: pin {p!r} is not a nuisance of this scenario")
        if v not in nuis[p]:
            raise ValueError(f"{scn['id']}: pin {p}={v!r} is not one of {nuis[p]}")
        nuis[p] = [v]
    keys = sorted(nuis)
    grid = [
        dict(zip(keys, combo, strict=True)) for combo in itertools.product(*(nuis[k] for k in keys))
    ]
    grid = grid or [{}]
    out: dict[str, list] = {}
    for name, val in scn["candidates"].items():
        rows = []
        for alt in _alternatives(val):
            for g in grid:
                assign = {**g, **alt}
                rows.append((assign, base.with_overrides(assign)))
        out[name] = rows
    return out


def _variant_actions(scn: dict):
    """(label as it appears in a trace, action dict) for every scripted action."""
    if "variants" in scn:
        for v in scn["variants"]:
            for a in v["actions"]:
                yield f"{v['name']}.{a['label']}", a
    else:
        for a in scn.get("setup", {}).get("actions", []):
            yield a["label"], a


def effective_scenario(scn: dict, rec: dict) -> dict:
    """The scenario as it was actually played: scripted positions and times replaced
    by `observed_placements` from the recording, where present.  Times are taken
    relative to the earliest observed action, anchored at that action's scripted
    t_ms, because the recording clock is not the battle clock."""
    obs = rec.get("observed_placements") or {}
    if not obs or scn["kind"] != "trajectory":
        return scn
    s = copy.deepcopy(scn)
    acts = [(lbl, a) for lbl, a in _variant_actions(s) if lbl in obs]
    if not acts:
        return scn
    groups: dict[str, list] = {}
    for lbl, a in acts:
        groups.setdefault(lbl.split(".")[0] if "variants" in s else "", []).append((lbl, a))
    for members in groups.values():
        timed = [(lbl, a) for lbl, a in members if obs[lbl].get("t") is not None]
        anchor = min(timed, key=lambda la: obs[la[0]]["t"]) if timed else None
        for lbl, a in members:
            x, y = obs[lbl]["tile"]
            a["tile_100"] = [round(float(x) * 100), round(float(y) * 100)]
            if anchor is not None and obs[lbl].get("t") is not None:
                dt = float(obs[lbl]["t"]) - float(obs[anchor[0]]["t"])
                a["t_ms"] = int(anchor[1]["t_ms"]) + round(dt * 1000)
    digest = hashlib.sha256(json.dumps(obs, sort_keys=True).encode()).hexdigest()[:12]
    s["id"] = f"{scn['id']}@obs{digest}"
    return s


def simulate_scenario(backend, scn: dict, cfg: synth.ModelConfig) -> dict:
    """One trace for the whole scenario; variants get their unit labels prefixed."""
    if "variants" not in scn:
        return backend.simulate(scn, cfg)
    merged = None
    for v in scn["variants"]:
        sub = dict(scn, setup={"duration_ms": v["duration_ms"], "actions": v["actions"]})
        sub.pop("variants")
        tr = backend.simulate(sub, cfg)
        if merged is None:
            merged = dict(tr, units={}, events=[])
        for lbl, u in tr["units"].items():
            merged["units"][f"{v['name']}.{lbl}"] = u
        merged["events"].extend(dict(e, unit=f"{v['name']}.{e.get('unit')}") for e in tr["events"])
    return merged


# --------------------------------------------------------------------------- #
# observables -- computed identically on recorded and simulated traces        #
# --------------------------------------------------------------------------- #


def _arr(u: dict) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    return np.asarray(u["t"], float), np.asarray(u["x"], float), np.asarray(u["y"], float)


def _smooth(v: np.ndarray, k: int = 5) -> np.ndarray:
    """Running median: an onset detector on raw tracker output fires on noise."""
    if len(v) < k:
        return v
    pad = k // 2
    vv = np.pad(v, pad, mode="edge")
    return np.median(np.lib.stride_tricks.sliding_window_view(vv, k), axis=1)


def _first_cross(t: np.ndarray, v: np.ndarray, level: float) -> float | None:
    s = _smooth(v)
    idx = np.nonzero(s >= level)[0] if s[-1] >= s[0] else np.nonzero(s <= level)[0]
    if len(idx) == 0 or idx[0] == 0:
        return None if len(idx) == 0 else float(t[0])
    i = idx[0]
    a, b = s[i - 1], s[i]
    w = 0.0 if b == a else (level - a) / (b - a)
    return float(t[i - 1] + w * (t[i] - t[i - 1]))


def observable(trace: dict, spec: dict) -> float | None:
    u = trace["units"].get(spec["unit"])
    if u is None or len(u["t"]) < 3:
        return None
    t, x, y = _arr(u)
    fn = spec["fn"]
    if fn == "time_between_y":
        a, b = _first_cross(t, y, spec["y_from"]), _first_cross(t, y, spec["y_to"])
        return None if a is None or b is None else b - a
    if fn == "x_at_y":
        tc = _first_cross(t, y, spec["y"])
        return None if tc is None else float(np.interp(tc, t, _smooth(x)))
    if fn == "min_dist_to_point":
        px, py = spec["point"]
        return float(np.min(np.hypot(_smooth(x) - px, _smooth(y) - py)))
    if fn == "deviation_onset_y":
        dev = np.abs(_smooth(x) - spec["x_ref"])
        idx = np.nonzero(dev > spec["threshold"])[0]
        return None if len(idx) == 0 else float(_smooth(y)[idx[0]])
    if fn == "min_x":
        return float(np.min(_smooth(x)))
    if fn == "max_x":
        return float(np.max(_smooth(x)))
    if fn == "max_abs_dx":
        return float(np.max(np.abs(_smooth(x) - spec["x_ref"])))
    raise ValueError(f"unknown observable fn {fn!r}")


# --------------------------------------------------------------------------- #
# scorers                                                                     #
# --------------------------------------------------------------------------- #


def score_trajectory(rec: dict, sim: dict, scn: dict, doc: dict) -> dict:
    """Mean tile error after fitting two nuisances every real recording has:
    a common TIME offset (recording clock vs battle clock) and a bounded common
    TRANSLATION (homography offset plus sprite foot-point bias).  Both are shared
    by every unit in the recording, so neither can bend one candidate's SHAPE
    toward another's.  The translation is capped at max_translation_tiles so a
    wrong lane cannot be "explained" as a calibration error."""
    d = doc["defaults"]
    W = float(scn.get("time_offset_search_s", d["time_offset_search_s"]))
    cap = float(scn.get("max_translation_tiles", d.get("max_translation_tiles", 0.3)))
    win = scn.get("score_window_s")
    labels = sorted(rec["units"])
    if not labels:
        raise ValueError("recorded trace has no units")
    missing = [lbl for lbl in labels if lbl not in sim["units"]]
    if missing:
        raise ValueError(f"simulated trace lacks recorded unit(s) {missing}")
    pairs = []
    for lbl in labels:
        rt, rx, ry = _arr(rec["units"][lbl])
        st, sx, sy = _arr(sim["units"][lbl])
        if len(rt) and len(st):
            pairs.append((rt, rx, ry, st, sx, sy))
    if not pairs:
        raise ValueError("no unit has samples in both traces")

    def residuals(offset: float) -> tuple[np.ndarray, np.ndarray]:
        exs, eys = [], []
        for rt, rx, ry, st, sx, sy in pairs:
            # samples outside the simulated window are compared with the clamped
            # end position, so an offset cannot buy a low score by discarding data
            tt = np.clip(rt - offset, st[0], st[-1])
            m = np.ones_like(tt, dtype=bool)
            if win is not None:
                m = (rt - offset >= win[0]) & (rt - offset <= win[1])
            exs.append(np.interp(tt[m], st, sx) - rx[m])
            eys.append(np.interp(tt[m], st, sy) - ry[m])
        ex, ey = np.concatenate(exs), np.concatenate(eys)
        if len(ex):
            ex = ex - np.clip(np.mean(ex), -cap, cap)
            ey = ey - np.clip(np.mean(ey), -cap, cap)
        return ex, ey

    def cost(offset: float) -> float:
        ex, ey = residuals(offset)
        return float(np.mean(np.hypot(ex, ey))) if len(ex) else math.inf

    coarse = np.arange(-W, W + 1e-9, 0.04)
    best = min(coarse, key=cost)
    fine = np.arange(best - 0.04, best + 0.04 + 1e-9, 0.004)
    best = float(min(fine, key=cost))
    ex, ey = residuals(best)
    e = np.hypot(ex, ey)
    if not len(e):
        raise ValueError("score window selects no samples")
    obs = {}
    for spec in scn.get("observables", []):
        r, s_ = observable(rec, spec), observable(sim, spec)
        obs[spec["name"]] = {
            "recorded": r,
            "sim": s_,
            "abs_err": None if r is None or s_ is None else abs(r - s_),
        }
    return {
        "score": float(np.mean(e)),
        "mean_err": float(np.mean(e)),
        "max_err": float(np.max(e)),
        "rms_err": float(np.sqrt(np.mean(e * e))),
        "n": len(e),
        "time_offset_s": best,
        "observables": obs,
    }


def phase_features(times: list[float], periods_s: list[float]) -> list[float]:
    if len(times) < 2:
        return [0.0 for _ in periods_s]
    return [
        abs(sum(cmath.exp(2j * math.pi * (t / p)) for t in times)) / len(times) for p in periods_s
    ]


def _stable_seed(*parts) -> int:
    return int.from_bytes(hashlib.sha256("|".join(map(str, parts)).encode()).digest()[:4], "little")


def score_event_phase(
    rec: dict, backend, scn: dict, cfg: synth.ModelConfig, all_tps: list[int]
) -> dict:
    fps = rec["source"].get("capture_fps", 60.0)
    periods = [1.0 / k for k in sorted(set(all_tps)) if k < fps]
    if not periods:
        raise ValueError("no candidate tick period is resolvable at this capture fps")
    times = [e["t"] for e in rec["events"]]
    obs = phase_features(times, periods)
    cap = synth.CaptureModel(fps=float(fps))
    preds = []
    for k in range(PHASE_MC_RUNS):
        sd = _stable_seed(scn["id"], cfg.tps, k)
        lg = synth.event_phase_logic(scn, cfg, sd)
        ev = synth.capture_events(lg, cap, sd + 1)["events"]
        preds.append(phase_features([e["t"] for e in ev], periods))
    mean = [float(np.mean([p[i] for p in preds])) for i in range(len(periods))]
    d = float(np.sqrt(sum((o - m) ** 2 for o, m in zip(obs, mean, strict=True))))
    return {
        "score": d,
        "features_recorded": obs,
        "features_predicted": mean,
        "periods_s": periods,
        "n_events": len(times),
    }


def _wrapped_logpdf(u: np.ndarray, centres: list[float], sigma: float) -> np.ndarray:
    dens = np.zeros_like(u)
    for c in centres:
        for k in (-1, 0, 1):
            dens += np.exp(-0.5 * ((u - c - k) / sigma) ** 2) / (sigma * math.sqrt(2 * math.pi))
    return np.log(dens / len(centres) + 1e-300)


def score_deploy_lattice(rec: dict, backend, cfg: synth.ModelConfig) -> dict:
    pts = np.asarray(rec["spawn_positions"], float)
    if len(pts) < 8:
        raise ValueError(f"only {len(pts)} spawn positions; lattice fit needs >= 8")
    lat = backend.snap_lattice(cfg)
    if lat is None:
        return {
            "score": 0.0,
            "nll_per_point": 0.0,
            "period": None,
            "shift": None,
            "sigma": None,
            "penalty": 0.0,
        }
    period, offsets = lat
    # Each axis has its own homography offset, so each gets its own phase; sigma
    # is capped at period/4 because a "lattice" blurrier than that is uniform noise
    # wearing a lattice's name.
    sigmas = [v for v in (0.03, 0.045, 0.06, 0.08, 0.1, 0.125) if v <= period / 4 + 1e-12]
    total, fit = 0.0, []
    for axis in (0, 1):
        u = np.mod(pts[:, axis], 1.0)
        best = None
        for sigma in sigmas:
            for shift in np.linspace(0.0, period, 50, endpoint=False):
                centres = sorted(
                    {
                        ((o + shift + k * period) % 1.0)
                        for o in offsets
                        for k in range(round(1.0 / period))
                    }
                )
                nll = -float(_wrapped_logpdf(u, centres, sigma).sum())
                if best is None or nll < best[0]:
                    best = (nll, float(shift), sigma)
        total += best[0]
        fit.append({"shift": best[1], "sigma": best[2]})
    # A lattice model fits free parameters (phase and sigma per axis) that the
    # uniform model does not, so it can always shave a little off pure noise.
    # BIC's k*ln(N)/2 is the price, expressed per spawn point.  Without it a
    # recording of unsnapped taps read as "half-tile snap" -- the self-test caught
    # exactly that on its first run.
    k, n_obs = 4, 2 * len(pts)
    penalty = k * math.log(n_obs) / 2 / len(pts)
    nll_pp = total / len(pts)
    return {
        "score": nll_pp + penalty,
        "nll_per_point": nll_pp,
        "period": period,
        "shift": [f["shift"] for f in fit],
        "sigma": [f["sigma"] for f in fit],
        "penalty": penalty,
    }


def lag_of(trace: dict, spec: dict) -> float | None:
    ev = [
        e["t"]
        for e in trace.get("events", [])
        if e.get("name") == spec["event"] and e.get("unit") == spec["event_unit"]
    ]
    u = trace["units"].get(spec["unit"])
    if not ev or u is None or len(u["t"]) < 5:
        return None
    t, x, _ = _arr(u)
    # The reference line is the unit's OWN pre-event lateral position, not the
    # nominal lane x: a 0.1-tile homography offset against an absolute reference
    # fired the 0.2-tile onset detector at t=0 on half the synthetic trials.
    pre = (t >= ev[0] - 0.5) & (t <= ev[0])
    if pre.sum() < 3:
        return None
    ref = float(np.median(x[pre]))
    dev = np.abs(_smooth(x) - ref)
    idx = np.nonzero((dev > spec["threshold"]) & (t >= ev[0]))[0]
    return None if len(idx) == 0 else float(t[idx[0]] - ev[0])


def score_lag(rec: dict, sims: list[dict], scn: dict) -> dict:
    """Likelihood of the observed reaction lag under a candidate whose lag depends
    on an unobservable per-trial phase.  The candidate's predicted lags are the
    set over every phase it allows (a uniform prior) and the observation is scored
    against that mixture with timing error sigma_s.  Profiling the phase per trial
    instead (best phase wins) lets a 10-tick repath clock with the right phase
    impersonate a 1-tick clock on every trial -- the self-test measured margins of
    0.0004 tile doing it that way.

    Not adopted, measured 2026-09-13: forward-modelling the capture error into the
    predicted lags (Monte-Carlo renders through synth.CaptureModel, K=6/12, kernel
    0.03-0.05 s).  With speed_unit pinned it was no better than the clean lag set
    (0/30 wrong either way), and unpinned it could not rescue S09 (2-7 of 20 seeds
    still wrong).  It would also tie the verdict to an unmeasured capture budget."""
    spec = scn["lag"]
    obs = lag_of(rec, spec)
    preds = [p for p in (lag_of(sim, spec) for sim in sims) if p is not None]
    if obs is None or not preds:
        return {"score": math.inf, "lag_recorded": obs, "lags_predicted": preds}
    sig = float(spec["sigma_s"])
    dens = sum(math.exp(-0.5 * ((obs - p) / sig) ** 2) for p in preds) / (
        len(preds) * sig * math.sqrt(2 * math.pi)
    )
    return {
        "score": -math.log(dens + 1e-300),
        "lag_recorded": obs,
        "lags_predicted": sorted(round(p, 3) for p in preds),
        "n": 1,
    }


# --------------------------------------------------------------------------- #
# ranking and gates                                                           #
# --------------------------------------------------------------------------- #


class SimCache:
    """Candidate simulations are deterministic in (scenario, config); the self-test
    scores many recordings against the same candidates.  Keyed by scenario id, so
    an effective_scenario() with observed placements gets its own entries."""

    def __init__(self, backend):
        self.backend = backend
        self._c: dict = {}

    def get(self, scn: dict, cfg: synth.ModelConfig) -> dict:
        key = (scn["id"], cfg)
        if key not in self._c:
            self._c[key] = simulate_scenario(self.backend, scn, cfg)
        return self._c[key]


def global_key(assign: dict, per_trial: set[str]) -> tuple:
    """The part of a candidate assignment that is a property of THE GAME, and so is
    the same in every repetition: everything except per_trial_nuisance."""
    return tuple(sorted((k, repr(v)) for k, v in assign.items() if k not in per_trial))


def missing_pins(scn: dict, pins: dict | None) -> list[str]:
    return sorted(set(scn.get("prerequisite_pins", {})) - set(pins or {}))


def registry_pins(scen: dict, scn: dict, backend, cal: dict | None = None) -> dict:
    """Pins that data/calibration.json can supply: a prerequisite whose every
    promoted registry key is already `measured` pins its param to the value the
    backend's base_config derives from the registry.  Anything less is not a pin."""
    cal = cal if cal is not None else synth.load_calibration()
    out = {}
    base = backend.base_config()
    for param, req in scn.get("prerequisite_pins", {}).items():
        pre = scen.get(req["scenario"])
        keys = [p["key"] for p in (pre or {}).get("promotes", []) if p.get("param") == param]
        if not keys:
            continue
        statuses = []
        for k in keys:
            node = cal
            for part in k.split("."):
                node = node.get(part, {}) if isinstance(node, dict) else {}
            statuses.append(node.get("status") if isinstance(node, dict) else None)
        if all(s == "measured" for s in statuses):
            out[param] = getattr(base, param)
    return out


def rank(
    recs: dict | list[dict],
    scn: dict,
    doc: dict,
    backend,
    cache: SimCache | None = None,
    pins: dict | None = None,
    *,
    profile_global_per_recording: bool = False,
    invert: bool = False,
) -> dict:
    """Rank candidates against one recording or several repetitions of it.

    A candidate's rows are grouped by their GLOBAL assignment (alternative plus every
    nuisance that is a game constant, e.g. speed_unit).  One global assignment must
    explain every repetition at once: its score is the mean over recordings, and
    the candidate takes its best global assignment.  Only `per_trial_nuisance`
    (a repath clock phase, which genuinely differs per trial) is re-chosen per
    recording -- marginalised for lag_likelihood, profiled otherwise.

    LETTING A NUISANCE FLOAT PER TRIAL DOES NOT ERR ON THE SAFE SIDE, and the
    intuition that it only ever helps the WRONG candidate -- and so errs toward
    INCONCLUSIVE -- is measurably false.  With truth rendered from
    repath_interval_ticks=10 at speed_unit=millitiles_per_50ms (a truth row the
    first-row-only self-test never drew), S09 returned candidate '1' as a DECISIVE
    winner on 11 of 20 seeds.  Per-trial speed_unit let '1' offer two sharp lags
    (0.35 s and 0.50 s) to every trial, which beat '10's honest spread.  Floating a
    nuisance per trial helps whichever candidate is SHARPER, and that can be the
    wrong one by a decisive margin.

    Sharing speed_unit across trials was NOT enough on its own: truth '1' at
    tiles_per_minute then lost to '10' at millitiles_per_50ms on 17 of 20 seeds,
    because a lag is a time and a faster unit reaches the 0.2-tile onset sooner, so
    '10'-at-fast covers '1'-at-slow.  With speed_unit pinned to its true value all
    four truths came back right on 30 of 30 seeds.  Hence prerequisite_pins.

    `profile_global_per_recording` and `invert` exist ONLY so plants can put the
    superseded rule back or turn the ranking upside down; nothing else sets them."""
    recs = [recs] if isinstance(recs, dict) else list(recs)
    if not recs:
        raise ValueError("no recordings")
    cache = cache or SimCache(backend)
    blocked = missing_pins(scn, pins)
    cands = candidate_configs(backend, doc, scn, pins)
    all_tps = sorted({cfg.tps for rows in cands.values() for _, cfg in rows})
    per_trial = set(scn.get("per_trial_nuisance", []))
    lag_mode = scn.get("rank_by") == "lag_likelihood"
    escns = [effective_scenario(scn, rec) for rec in recs]
    table = []
    for name, rows in cands.items():
        groups: dict[tuple, list[tuple[dict, synth.ModelConfig]]] = {}
        for assign, cfg in rows:
            groups.setdefault(global_key(assign, per_trial), []).append((assign, cfg))

        def score_group(i: int, members: list, name: str = name) -> tuple[dict, dict]:
            rec, escn = recs[i], escns[i]
            if lag_mode:
                sims = [cache.get(escn, cfg) for _, cfg in members]
                gassign = {k: v for k, v in members[0][0].items() if k not in per_trial}
                return score_lag(rec, sims, escn), gassign
            best = None
            for assign, cfg in members:
                if scn["kind"] == "trajectory":
                    m = score_trajectory(rec, cache.get(escn, cfg), escn, doc)
                elif scn["kind"] == "event_phase":
                    m = score_event_phase(rec, backend, scn, cfg, all_tps)
                elif scn["kind"] == "deploy_lattice":
                    m = score_deploy_lattice(rec, backend, cfg)
                else:
                    raise ValueError(f"unknown scenario kind {scn['kind']!r}")
                if not math.isfinite(m["score"]):
                    raise ValueError(f"non-finite score for {name} {assign}")
                if best is None or m["score"] < best[0]["score"]:
                    best = (m, assign)
            return best

        if profile_global_per_recording:
            # the superseded rule, kept runnable only so a plant can put it back
            per_rec = []
            for i in range(len(recs)):
                cells = [score_group(i, members) for members in groups.values()]
                per_rec.append(min(cells, key=lambda c: c[0]["score"]))
        else:
            per_rec, best_mean = None, math.inf
            for members in groups.values():
                cells = [score_group(i, members) for i in range(len(recs))]
                mean = float(np.mean([c[0]["score"] for c in cells]))
                if per_rec is None or mean < best_mean:
                    per_rec, best_mean = cells, mean
        if not all(math.isfinite(c[0]["score"]) for c in per_rec):
            raise ValueError(f"no finite score for {name}: {per_rec[0][0]}")
        score = float(np.mean([b[0]["score"] for b in per_rec]))
        table.append(
            {
                "candidate": name,
                "score": score,
                "best_assignment": per_rec[0][1],
                "metrics": per_rec[0][0],
                "per_recording_scores": [b[0]["score"] for b in per_rec],
            }
        )
    table.sort(key=lambda r: (r["score"], r["candidate"]), reverse=invert)
    m_abs = float(scn.get("margin_abs", doc["defaults"]["margin_abs"]))
    m_rel = float(scn.get("margin_rel", doc["defaults"]["margin_rel"]))
    win, run = table[0], table[1]
    required = max(m_abs, m_rel * win["score"])
    gap = abs(run["score"] - win["score"]) if invert else run["score"] - win["score"]
    return {
        "format": RESULT_FORMAT,
        "scenario_id": scn["id"],
        "kind": scn["kind"],
        "focal_param": scn["focal_param"],
        "backend": getattr(backend, "name", type(backend).__name__),
        "n_recordings": len(recs),
        "pins": dict(pins or {}),
        "blocked": (
            None
            if not blocked
            else f"prerequisite pin(s) missing: {blocked} -- settle "
            f"{sorted({scn['prerequisite_pins'][p]['scenario'] for p in blocked})} first"
        ),
        "ranking": table,
        "winner": win["candidate"],
        "runner_up": run["candidate"],
        "margin": gap,
        "margin_required": required,
        "decisive": bool(gap >= required) and not blocked,
        "promotes": scn.get("promotes", []),
    }


def gate(result: dict, expected: str, min_samples: int = 20) -> list[str]:
    """Named gates for one self-test cell.  Names are stable: plants aim at them."""
    fail = []
    sid = result["scenario_id"]
    if result.get("blocked"):
        fail.append(f"prerequisite-pinned: {sid} {result['blocked']}")
    if result["winner"] != expected:
        scores = [(r["candidate"], round(r["score"], 4)) for r in result["ranking"]]
        fail.append(
            f"winner-is-expected: {sid} truth={expected} got={result['winner']} (scores {scores})"
        )
    if result["margin"] < result["margin_required"]:
        fail.append(
            f"decisive-margin: {sid} truth={expected} margin={result['margin']:.4f} "
            f"< required {result['margin_required']:.4f}"
        )
    m0 = result["ranking"][0]["metrics"]
    n = m0.get("n", m0.get("n_events", min_samples))
    if "mean_err" in m0 and n < min_samples:
        fail.append(f"vacuity: {sid} winner scored on only {n} samples")
    return fail


def gate_name(failure: str) -> str:
    return failure.split(":", 1)[0]


def truth_config(backend, doc: dict, scn: dict, cand: str) -> synth.ModelConfig:
    return candidate_configs(backend, doc, scn)[cand][0][1]


def truth_rows(backend, doc: dict, scn: dict) -> dict[str, list[tuple[dict, synth.ModelConfig]]]:
    """Every distinct truth a candidate can be: each alternative x each GLOBAL
    nuisance value.  Per-trial nuisance is not enumerated here because
    synth_repetitions re-draws it for every repetition, as the game would.

    Enumerating only the candidate's FIRST row is not enough: that renders S01 truth
    only at 20 TPS with lane_snap and S09 only at tiles_per_minute, and so never sees
    the S09 wrong-winner documented in rank()."""
    per_trial = set(scn.get("per_trial_nuisance", []))
    out = {}
    for name, rows in candidate_configs(backend, doc, scn).items():
        seen: dict[tuple, tuple[dict, synth.ModelConfig]] = {}
        for assign, cfg in rows:
            seen.setdefault(global_key(assign, per_trial), (assign, cfg))
        out[name] = list(seen.values())
    return out


def synth_recording(
    backend,
    doc: dict,
    scn: dict,
    cfg: synth.ModelConfig,
    seed: int,
    cap: synth.CaptureModel | None = None,
) -> dict:
    cap = cap or synth.CaptureModel()
    if scn["kind"] == "trajectory":
        return synth.render_recording(simulate_scenario(backend, scn, cfg), cap, seed)
    if scn["kind"] == "event_phase":
        return synth.capture_events(synth.event_phase_logic(scn, cfg, seed), cap, seed + 7)
    return synth.deploy_positions(scn, cfg, cap, seed)


def synth_repetitions(
    backend,
    doc: dict,
    scn: dict,
    cfg: synth.ModelConfig,
    seed: int,
    cap: synth.CaptureModel | None = None,
) -> list[dict]:
    """As many synthetic recordings as the procedure asks for.  Each
    repetition draws its own camera errors: that is optimistic if the operator never
    moves the phone between takes, which is why the protocol says to re-calibrate."""
    n = int(scn.get("repetitions", 1))
    out = []
    for k in range(n):
        c = cfg
        rng = random.Random(_stable_seed(seed, k, "trial"))
        for p in scn.get("per_trial_nuisance", []):
            # the real game draws these per trial; truth must too, or a candidate
            # that pins them looks better on synthetic data than it ever will on video
            c = c.with_overrides({p: rng.choice(scn["nuisance"][p])})
        out.append(synth_recording(backend, doc, scn, c, _stable_seed(seed, k), cap))
    return out


def self_test(
    doc: dict,
    scen: dict,
    ids: list[str],
    backend,
    seeds=SELF_TEST_SEEDS,
    verbose: bool = True,
    *,
    truth_shift: int = 0,
    rank_kwargs: dict | None = None,
    drop_pins: bool = False,
) -> dict:
    """Returns {scenario_id: {candidate: {"fails": [...], "gates": {...}, "cells": n,
    "winners": [...]}}}; an empty fails list is green.

    truth_shift, rank_kwargs and drop_pins are plant overrides: the plants call THIS
    function, so a landed plant certifies the code path --self-test runs."""
    report: dict = {}
    rank_kwargs = rank_kwargs or {}
    for sid in ids:
        scn = scen[sid]
        cache = SimCache(backend)
        rows = truth_rows(backend, doc, scn)
        names = list(scn["candidates"])
        report[sid] = {}
        for ci, cand in enumerate(names):
            src = names[(ci + truth_shift) % len(names)]
            fails: list[str] = []
            winners: list[str] = []
            gates_red: set[str] = set()
            n_cells = 0
            for assign, cfg in rows[src]:
                pins = {} if drop_pins else {p: assign[p] for p in scn.get("prerequisite_pins", {})}
                gk = global_key(assign, set(scn.get("per_trial_nuisance", [])))
                shown = {
                    k: v
                    for k, v in assign.items()
                    if k in scn.get("nuisance", {}) and k not in scn.get("per_trial_nuisance", [])
                }
                for sd in seeds:
                    recs = synth_repetitions(backend, doc, scn, cfg, _stable_seed(sid, src, sd, gk))
                    res = rank(recs, scn, doc, backend, cache, pins or None, **rank_kwargs)
                    g = gate(res, cand)
                    n_cells += 1
                    winners.append(res["winner"])
                    gates_red.update(gate_name(f) for f in g)
                    fails += [f"seed {sd} truth-row {shown} -- {f}" for f in g]
                    if verbose:
                        print(
                            f"  {sid:28s} truth={cand:22s} seed={sd:<3d} "
                            f"reps={len(recs):<2d} winner={res['winner']:22s} "
                            f"margin={res['margin']:.3f}/{res['margin_required']:.3f} "
                            f"{'ok ' if not g else 'RED'} {shown}"
                        )
            report[sid][cand] = {
                "fails": fails,
                "gates": sorted(gates_red),
                "cells": n_cells,
                "winners": winners,
            }
    return report


def _claim(value) -> tuple[bool, list[str] | None]:
    if isinstance(value, bool):
        return value, None
    if isinstance(value, dict) and isinstance(value.get("green"), bool):
        rg = value.get("red_gates")
        return value["green"], None if rg is None else sorted(rg)
    raise ValueError(f"malformed self_test_expect claim {value!r}")


def claim_mismatches(scen: dict, report: dict) -> list[str]:
    """scenarios.json records, per candidate, whether the self-test is EXPECTED to
    recover it decisively, and for an expected-red candidate WHICH gates go red.  A
    claim the tool contradicts is a defect in either direction: an expected-green
    cell going red is a lost discrimination, an expected-red cell going green means
    the file under-sells a scenario that may have been skipped because of it, and a
    red cell going red on a DIFFERENT gate (indecisive -> wrong winner) is a
    discrimination that got worse while staying red."""
    out = []
    for sid, cells in report.items():
        claims = scen[sid].get("self_test_expect")
        if not isinstance(claims, dict):
            out.append(f"{sid}: no self_test_expect claims recorded")
            continue
        extra = sorted(set(claims) - set(cells) - {"$comment"})
        if extra:
            out.append(f"{sid}: claims for unknown candidate(s) {extra}")
        for cand, cell in cells.items():
            if cand not in claims:
                out.append(f"{sid}/{cand}: no claim recorded")
                continue
            green, red_gates = _claim(claims[cand])
            observed_green = not cell["fails"]
            if green != observed_green:
                out.append(
                    f"{sid}/{cand}: claims green={green} but self-test observed "
                    f"green={observed_green} (red gates {cell['gates']})"
                )
            elif not green and red_gates is not None and red_gates != cell["gates"]:
                out.append(
                    f"{sid}/{cand}: claims red on {red_gates} but self-test observed red on "
                    f"{cell['gates']}"
                )
    return out


def run_self_test(
    doc: dict,
    scen: dict,
    ids: list[str],
    backend,
    seeds,
    verbose: bool = True,
    summary: bool = True,
    **overrides,
) -> tuple[int, dict, list[str]]:
    rep = self_test(doc, scen, ids, backend, seeds, verbose=verbose, **overrides)
    cells = sum(c["cells"] for v in rep.values() for c in v.values())
    cands = sum(len(v) for v in rep.values())
    if summary:
        print()
        print("SELF-TEST SUMMARY (synthetic truth under the default capture error budget)")
        for sid in ids:
            for cand, c in rep[sid].items():
                state = "green" if not c["fails"] else f"RED ({len(c['fails'])}) on {c['gates']}"
                print(f"  {sid:28s} {cand:24s} {c['cells']:3d} cells  {state}")
                for f in c["fails"][:4]:
                    print(f"       {f}")
                if len(c["fails"]) > 4:
                    print(f"       ... {len(c['fails']) - 4} more")
    # vacuity: every scenario must have produced at least two candidates' worth of
    # cells, and every candidate at least one cell per seed.
    thin = [
        f"{sid}/{cand}" for sid in ids for cand, c in rep[sid].items() if c["cells"] < len(seeds)
    ]
    if cands < 2 * len(ids) or thin:
        print(
            f"VACUITY: {cands} candidates for {len(ids)} scenarios; thin: {thin}", file=sys.stderr
        )
        return 1, rep, []
    mism = claim_mismatches(scen, rep)
    green = sum(1 for v in rep.values() for c in v.values() if not c["fails"])
    if summary:
        print(
            f"  {cands} candidates / {cells} cells over {len(ids)} scenarios x {len(seeds)} "
            f"seed(s): {green} candidates green, {cands - green} red; "
            f"{len(mism)} contradicted claim(s)"
        )
        for m in mism:
            print(f"  CLAIM CONTRADICTED: {m}")
    return (1 if mism else 0), rep, mism


# --------------------------------------------------------------------------- #
# predictions (what scenarios.json stores)                                    #
# --------------------------------------------------------------------------- #


def predict(doc: dict, scn: dict, backend) -> dict:
    """Clean (no camera) observables per candidate, and the minimum trajectory
    separation between candidate pairs profiled over nuisance -- i.e. the gap the
    measurement has to resolve in the WORST nuisance case.  For a lag scenario, the
    set of reaction lags each candidate predicts, per global nuisance value."""
    out: dict = {"candidates": {}, "pairwise_min_separation": {}}
    if scn["kind"] != "trajectory":
        cache_cfg = candidate_configs(backend, doc, scn)
        for name, rows in cache_cfg.items():
            cfg = rows[0][1]
            if scn["kind"] == "event_phase":
                tps = sorted({c.tps for r in cache_cfg.values() for _, c in r})
                periods = [1.0 / k for k in tps if k < 60]
                feats = []
                for k in range(PHASE_MC_RUNS):
                    sd = _stable_seed(scn["id"], cfg.tps, k)
                    ev = synth.capture_events(
                        synth.event_phase_logic(scn, cfg, sd), synth.CaptureModel(), sd + 1
                    )
                    feats.append(phase_features([e["t"] for e in ev["events"]], periods))
                out["candidates"][name] = {
                    f"R_at_{round(p * 1000, 2)}ms": round(float(np.mean([f[i] for f in feats])), 3)
                    for i, p in enumerate(periods)
                }
            else:
                lat = backend.snap_lattice(cfg)
                out["candidates"][name] = {"lattice_period_tiles": None if lat is None else lat[0]}
        return out
    cands = candidate_configs(backend, doc, scn)
    traces = {
        n: [(a, simulate_scenario(backend, scn, c)) for a, c in rows] for n, rows in cands.items()
    }
    per_trial = set(scn.get("per_trial_nuisance", []))
    for name, rows in traces.items():
        per = {}
        for spec in scn.get("observables", []):
            vals = [observable(tr, spec) for _, tr in rows]
            vals = [v for v in vals if v is not None]
            per[spec["name"]] = None if not vals else [round(min(vals), 3), round(max(vals), 3)]
        if "lag" in scn:
            lags: dict[str, list[float]] = {}
            for a, tr in rows:
                gk = ",".join(
                    f"{k}={a[k]}" for k in sorted(scn.get("nuisance", {})) if k not in per_trial
                )
                v = lag_of(tr, scn["lag"])
                lags.setdefault(gk, []).append(None if v is None else round(v, 3))
            per["lag_s"] = {k: sorted(v, key=lambda z: (z is None, z)) for k, v in lags.items()}
        out["candidates"][name] = per
    names = list(traces)
    for a, b in itertools.combinations(names, 2):
        best = None
        for _, ta in traces[a]:
            for _, tb in traces[b]:
                d = score_trajectory(ta, tb, scn, doc)["mean_err"]
                best = d if best is None else min(best, d)
        out["pairwise_min_separation"][f"{a}|{b}"] = round(best, 3)
    return out


# --------------------------------------------------------------------------- #
# plants                                                                      #
# --------------------------------------------------------------------------- #

PLANTS = {
    # name: (scenario, truth candidate, gate prefix it aims at, description)
    "swap": (
        "S02_lane_snap_vs_diagonal",
        "diagonal",
        "winner-is-expected",
        "truth rendered from the OTHER candidate but labelled as the expected one",
    ),
    "blend": (
        "S02_lane_snap_vs_diagonal",
        "diagonal",
        "decisive-margin",
        "truth is the midpoint of both candidates' tracks -- no honest winner exists",
    ),
    "timescale": (
        "S01_speed_unit",
        "millitiles_per_50ms",
        "winner-is-expected",
        "recording clock stretched 1.2x, which turns a 1.2 tile/s unit into a 1.0 tile/s one",
    ),
    "noise": (
        "S03_building_lookahead",
        "lookahead",
        "decisive-margin",
        "capture error budget inflated 25x, far above the candidates' separation",
    ),
    "phase": (
        "S04_tick_rate_phase_lock",
        "20",
        "winner-is-expected",
        "every event timestamp jittered uniformly over one 50 ms tick, destroying the lock",
    ),
    "lattice": (
        "S07_deploy_snap",
        "tile",
        "winner-is-expected",
        "spawn positions smeared uniformly over a whole tile",
    ),
    "placement": (
        "S03_building_lookahead",
        "lookahead",
        "winner-is-expected",
        "the Cannon really went down 0.5 tile LEFT of the script (onto the lane line) but the "
        "recording's observed_placements are dropped, so candidates are simulated from the "
        "script -- the defect observed placements exist to prevent",
    ),
}

# Plants on the --self-test code path itself: (scenario ids, aimed gate, description).
SELFTEST_PLANTS = {
    "selftest_invert": (
        ["S01_speed_unit", "S02_lane_snap_vs_diagonal"],
        "winner-is-expected",
        "rank() sorts WORST first, so the harness scores the wrong candidate best",
    ),
    "selftest_swap_truth": (
        ["S01_speed_unit", "S05_push_mass_ladder"],
        "winner-is-expected",
        "every candidate's synthetic truth is rendered from the NEXT candidate but "
        "labelled as itself",
    ),
    "selftest_unpin": (
        ["S09_repath_lag"],
        "winner-is-expected",
        "S09's prerequisite pin on speed_unit is dropped, so speed_unit is profiled "
        "(the configuration measured to return wrong winners)",
    ),
    "selftest_claim_flip": (
        ["S01_speed_unit"],
        "CLAIM CONTRADICTED",
        "one green claim is flipped to red in the loaded copy; the claim gate must notice the "
        "file under-selling a scenario the self-test can in fact decide",
    ),
}


def apply_plant(name: str, backend, doc: dict, scn: dict, cand: str, seed: int):
    cfg = truth_config(backend, doc, scn, cand)
    rng = random.Random(seed)
    if name == "swap":
        other = next(c for c in scn["candidates"] if c != cand)
        return synth_recording(backend, doc, scn, truth_config(backend, doc, scn, other), seed)
    if name == "blend":
        other = next(c for c in scn["candidates"] if c != cand)
        a = simulate_scenario(backend, scn, cfg)
        b = simulate_scenario(backend, scn, truth_config(backend, doc, scn, other))
        mid = json.loads(json.dumps(a))
        for lbl, u in mid["units"].items():
            ub = b["units"][lbl]
            u["x"] = [(p + q) / 2 for p, q in zip(u["x"], ub["x"], strict=True)]
            u["y"] = [(p + q) / 2 for p, q in zip(u["y"], ub["y"], strict=True)]
        return synth.render_recording(mid, synth.CaptureModel(), seed)
    if name == "timescale":
        rec = synth_recording(backend, doc, scn, cfg, seed)
        for u in rec["units"].values():
            u["t"] = [t * 1.2 for t in u["t"]]
        return rec
    if name == "noise":
        return synth_recording(backend, doc, scn, cfg, seed, synth.CaptureModel().scaled(25.0))
    if name == "phase":
        rec = synth_recording(backend, doc, scn, cfg, seed)
        for e in rec["events"]:
            e["t"] += rng.uniform(0.0, 0.05)
        return rec
    if name == "lattice":
        rec = synth_recording(backend, doc, scn, cfg, seed)
        rec["spawn_positions"] = [
            [x + rng.uniform(-0.5, 0.5), y + rng.uniform(-0.5, 0.5)]
            for x, y in rec["spawn_positions"]
        ]
        return rec
    if name == "placement":
        rec = placement_truth(backend, doc, scn, cfg, seed)
        rec.pop("observed_placements")
        return rec
    raise KeyError(name)


# Measured (seed 5): S03 truth 'lookahead' with the Cannon 0.5 tile left is ranked
# 'collision_only' when placements are ignored and 'lookahead' by 2.01 tile when they
# are kept; truth 'collision_only' with the Cannon 0.5 tile RIGHT is a DECISIVE wrong
# 'lookahead' without them.  A 0.25-tile Hog Rider error in S05 does NOT work as the
# plant: S05's verdict survives it (see scenarios.json S05 discrimination).
PLACEMENT_PLANT = {"card": "Cannon", "dx_tiles_100": -50}


def placement_truth(backend, doc: dict, scn: dict, cfg: synth.ModelConfig, seed: int) -> dict:
    """Truth played with PLACEMENT_PLANT's card moved off its scripted tile, with the
    observed placements recorded as extract_tracks.py would write them."""
    played = copy.deepcopy(scn)
    obs = {}
    for lbl, a in _variant_actions(played):
        if a["card"] == PLACEMENT_PLANT["card"]:
            a["tile_100"] = [a["tile_100"][0] + PLACEMENT_PLANT["dx_tiles_100"], a["tile_100"][1]]
        obs[lbl] = {"tile": [a["tile_100"][0] / 100, a["tile_100"][1] / 100], "t": a["t_ms"] / 1000}
    rec = synth_recording(backend, doc, played, cfg, seed)
    rec["observed_placements"] = obs
    return rec


def run_plant(name: str, backend, doc: dict, scen: dict) -> int:
    if name in SELFTEST_PLANTS:
        return run_selftest_plant(name, backend, doc, scen)
    sid, cand, aimed, desc = PLANTS[name]
    scn = scen[sid]
    seed = _stable_seed("plant", name)
    cache = SimCache(backend)
    if int(scn.get("repetitions", 1)) != 1:
        print(
            f"PLANT '{name}' INCONCLUSIVE -- aimed at {sid}, which needs repetitions; plants "
            "must target a single-recording scenario",
            file=sys.stderr,
        )
        return 1
    cfg = truth_config(backend, doc, scn, cand)
    if name == "placement":
        base_rec = placement_truth(backend, doc, scn, cfg, seed)
    else:
        base_rec = synth_recording(backend, doc, scn, cfg, seed)
    baseline = gate(rank(base_rec, scn, doc, backend, cache), cand)
    if baseline:
        print(
            f"PLANT '{name}' INCONCLUSIVE -- the baseline is already red without the plant, "
            "so nothing it does is evidence:",
            file=sys.stderr,
        )
        for f in baseline:
            print(f"   {f}", file=sys.stderr)
        return 1
    planted = apply_plant(name, backend, doc, scn, cand, seed)
    if json.dumps(planted, sort_keys=True) == json.dumps(base_rec, sort_keys=True):
        print(
            f"PLANT '{name}' DID NOT APPLY -- the planted recording is byte-identical to the "
            "baseline, so grading it would grade the unmodified input.",
            file=sys.stderr,
        )
        return 1
    fails = gate(rank(planted, scn, doc, backend, cache), cand)
    hit = [f for f in fails if f.startswith(aimed)]
    if hit:
        extra = len(fails) - len(hit)
        print(
            f"PLANT '{name}' LANDED -- '{aimed}' went red as intended ({desc})"
            + (f" (+{extra} neighbouring gate(s) also red)" if extra else "")
            + ":"
        )
        for f in hit:
            print(f"   {f}")
        return 0
    print(
        f"PLANT '{name}' DID NOT LAND -- '{aimed}' stayed GREEN with the defect present "
        f"({desc}). Other gates red: {fails}",
        file=sys.stderr,
    )
    return 1


def run_selftest_plant(name: str, backend, doc: dict, scen: dict) -> int:
    """Baseline --self-test on the aimed scenarios must be GREEN with every claim
    upheld; the planted run must go red on the AIMED gate, through the same
    run_self_test() that --self-test uses, and its exit code must be 1."""
    ids, aimed, desc = SELFTEST_PLANTS[name]
    seeds = SELF_TEST_SEEDS
    rc0, rep0, mism0 = run_self_test(doc, scen, ids, backend, seeds, False, False)
    red0 = {f"{s}/{c}": v["gates"] for s, cs in rep0.items() for c, v in cs.items() if v["fails"]}
    if rc0 != 0 or mism0:
        print(
            f"PLANT '{name}' INCONCLUSIVE -- baseline self-test on {ids} is not clean "
            f"(rc={rc0}, contradicted claims {mism0}, red cells {red0})",
            file=sys.stderr,
        )
        return 1
    overrides: dict = {}
    pscen = scen
    if name == "selftest_invert":
        overrides["rank_kwargs"] = {"invert": True}
    elif name == "selftest_swap_truth":
        overrides["truth_shift"] = 1
    elif name == "selftest_unpin":
        overrides["drop_pins"] = True
    elif name == "selftest_claim_flip":
        pscen = copy.deepcopy(scen)
        claims = pscen[ids[0]]["self_test_expect"]
        target = next(c for c, v in claims.items() if v is True)
        claims[target] = {"green": False, "red_gates": ["decisive-margin"]}
        if claims == scen[ids[0]]["self_test_expect"]:
            print(
                f"PLANT '{name}' DID NOT APPLY -- the claim edit changed nothing", file=sys.stderr
            )
            return 1
    rc1, rep1, mism1 = run_self_test(doc, pscen, ids, backend, seeds, False, False, **overrides)
    if name != "selftest_claim_flip" and rep1 == rep0:
        print(
            f"PLANT '{name}' DID NOT APPLY -- planted report identical to baseline", file=sys.stderr
        )
        return 1
    if aimed == "CLAIM CONTRADICTED":
        hit = mism1
    else:
        hit = [
            f
            for s in rep1.values()
            for c in s.values()
            if aimed in c["gates"]
            for f in c["fails"]
            if f" -- {aimed}:" in f
        ]
    if hit and rc1 == 1:
        print(
            f"PLANT '{name}' LANDED -- '{aimed}' went red as intended ({desc}); "
            f"--self-test exit code {rc1}, {len(mism1)} contradicted claim(s):"
        )
        for f in hit[:3]:
            print(f"   {f}")
        return 0
    print(
        f"PLANT '{name}' DID NOT LAND -- aimed '{aimed}' hits={len(hit)}, self-test rc={rc1} "
        f"({desc}). Contradicted claims: {mism1}",
        file=sys.stderr,
    )
    return 1


# --------------------------------------------------------------------------- #
# CLI                                                                         #
# --------------------------------------------------------------------------- #


def print_table(res: dict) -> None:
    print(
        f"scenario {res['scenario_id']}  ({res['kind']}, focal: {res['focal_param']}, "
        f"backend: {res['backend']}, pins: {res['pins'] or 'none'})"
    )
    print(f"  {'rank':4s} {'candidate':26s} {'score':>9s}  best nuisance / metrics")
    for i, r in enumerate(res["ranking"]):
        m = r["metrics"]
        extra = ""
        if "mean_err" in m:
            extra = (
                f"mean {m['mean_err']:.3f} max {m['max_err']:.3f} tile, "
                f"offset {m['time_offset_s']:+.3f}s"
            )
        elif "features_recorded" in m:
            extra = (
                f"R rec {[round(v, 3) for v in m['features_recorded']]} "
                f"pred {[round(v, 3) for v in m['features_predicted']]}"
            )
        elif "period" in m:
            extra = f"period {m['period']} shift {m['shift']} sigma {m['sigma']}"
        elif "lag_recorded" in m:
            extra = f"lag rec {m['lag_recorded']} pred {m['lags_predicted']}"
        print(
            f"  {i + 1:<4d} {r['candidate']:26s} {r['score']:9.4f}  {r['best_assignment']}  {extra}"
        )
    if res.get("blocked"):
        print(f"  BLOCKED: {res['blocked']}")
    verdict = "DECISIVE" if res["decisive"] else "INCONCLUSIVE"
    print(
        f"  {verdict}: winner {res['winner']} by {res['margin']:.4f} "
        f"(required {res['margin_required']:.4f}) over {res['runner_up']}"
    )


def _parse_pins(items: list[str] | None) -> dict:
    pins = {}
    for it in items or []:
        k, sep, v = it.partition("=")
        if not sep:
            raise ValueError(f"--pin expects param=value, got {it!r}")
        try:
            pins[k] = json.loads(v)
        except json.JSONDecodeError:
            pins[k] = v
    return pins


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="differential scorer for oracle recordings")
    ap.add_argument("--scenario", action="append", help="scenario id (repeatable)")
    ap.add_argument(
        "--recorded",
        type=Path,
        nargs="+",
        help="recorded trace JSON(s) (oracle-trace/1), one per repetition",
    )
    ap.add_argument("--out", type=Path, help="write the ranked result JSON here")
    ap.add_argument("--backend", help="module:attr of a simulator backend (default oracle.synth)")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--seeds", type=int, default=len(SELF_TEST_SEEDS))
    ap.add_argument("--plant", choices=[*sorted(PLANTS), *sorted(SELFTEST_PLANTS), "all"])
    ap.add_argument("--predict", metavar="SCENARIO")
    ap.add_argument("--pin", action="append", metavar="PARAM=VALUE")
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args(argv)

    doc, scen = load_scenarios()
    backend = load_backend(args.backend)
    for s in args.scenario or []:
        if s not in scen:
            print(f"unknown scenario {s!r}; known: {sorted(scen)}", file=sys.stderr)
            return 2

    if args.plant:
        names = [*sorted(PLANTS), *sorted(SELFTEST_PLANTS)] if args.plant == "all" else [args.plant]
        rc = 0
        for n in names:
            rc |= run_plant(n, backend, doc, scen)
        return rc

    if args.predict:
        if args.predict not in scen:
            print(f"unknown scenario {args.predict!r}", file=sys.stderr)
            return 2
        print(json.dumps(predict(doc, scen[args.predict], backend), indent=1))
        return 0

    if args.self_test:
        ids = args.scenario or sorted(scen)
        if args.seeds <= len(SELF_TEST_SEEDS):
            seeds = tuple(SELF_TEST_SEEDS[: args.seeds])
        else:
            seeds = tuple(range(11, 11 + 11 * args.seeds, 11))
        rc, _, _ = run_self_test(doc, scen, ids, backend, seeds, verbose=not args.quiet)
        print(f"self-test exit code {rc}")
        return rc

    if not args.scenario or not args.recorded:
        ap.print_usage(sys.stderr)
        print(
            "need --scenario and --recorded (or --self-test / --plant / --predict)",
            file=sys.stderr,
        )
        return 2
    if len(args.scenario) != 1:
        print("score one scenario per recording", file=sys.stderr)
        return 2
    scn = scen[args.scenario[0]]
    recs, meta = [], []
    for path in args.recorded:
        raw = path.read_bytes()
        rec = json.loads(raw)
        try:
            validate_trace(rec, kind=scn["kind"])
        except TraceSchemaError as e:
            print(f"{path}: {e}", file=sys.stderr)
            return 2
        if rec.get("scenario_id") not in (None, scn["id"]):
            print(
                f"{path}: recorded for scenario {rec.get('scenario_id')!r}, not {scn['id']!r}",
                file=sys.stderr,
            )
            return 2
        recs.append(rec)
        src = rec.get("source", {})
        meta.append(
            {
                "path": str(path),
                "sha256": hashlib.sha256(raw).hexdigest(),
                "source_kind": src.get("kind"),
                "recording_id": src.get("recording_id"),
                "recorded_date": src.get("recorded_date"),
                "video_sha256": src.get("video_sha256"),
            }
        )
    try:
        pins = _parse_pins(args.pin)
    except ValueError as e:
        print(str(e), file=sys.stderr)
        return 2
    pin_source = "cli" if pins else None
    if not pins and scn.get("prerequisite_pins"):
        pins = registry_pins(scen, scn, backend)
        pin_source = "calibration.json" if pins else None
    want = int(scn.get("repetitions", 1))
    if len(recs) < want:
        print(
            f"NOTE: {scn['id']} protocol asks for {want} repetitions, scoring {len(recs)}; "
            f"its self_test_expect claims were made at {want}",
            file=sys.stderr,
        )
    try:
        res = rank(recs, scn, doc, backend, pins=pins or None)
    except ValueError as e:
        print(f"cannot rank: {e}", file=sys.stderr)
        return 2
    res["pin_source"] = pin_source
    res["recorded"] = meta
    res["repetitions_required"] = want
    res["generated_utc"] = _dt.datetime.now(_dt.UTC).isoformat(timespec="seconds")
    if not args.quiet:
        print_table(res)
    if args.out:
        args.out.write_text(json.dumps(res, indent=1), encoding="utf-8")
    return 0 if res["decisive"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
