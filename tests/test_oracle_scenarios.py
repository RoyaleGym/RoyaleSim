"""oracle/scenarios.json is only worth the recording time if what it SAYS each
scenario discriminates is what the harness MEASURES.  These tests hold every stored
prediction and claim to the code that produced it, so a scenario file cannot drift
away from the harness that reads it."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
for p in (ROOT, ROOT / "tools"):
    if str(p) not in sys.path:
        sys.path.insert(0, str(p))

from oracle import extract_tracks, synth  # noqa: E402

import diff_harness as H  # noqa: E402

PY = sys.executable


@pytest.fixture(scope="module")
def loaded():
    doc, scen = H.load_scenarios()
    return doc, scen, H.load_backend(None)


def test_scenario_set_is_plausibly_sized(loaded):
    _, scen, _ = loaded
    assert len(scen) >= 9, sorted(scen)


def test_every_candidate_has_a_well_formed_claim(loaded):
    _, scen, _ = loaded
    for sid, s in scen.items():
        claims = s.get("self_test_expect")
        assert isinstance(claims, dict), f"{sid}: no self_test_expect"
        assert set(claims) == set(s["candidates"]), f"{sid}: claims {sorted(claims)}"
        for cand, c in claims.items():
            green, red_gates = H._claim(c)
            if not green:
                # an expected-red candidate must say WHY, or a drift from 'indecisive' to
                # 'wrong winner' would stay invisible
                assert red_gates, f"{sid}/{cand}: red claim without red_gates"
                assert set(red_gates) <= set(H.GATES), f"{sid}/{cand}: {red_gates}"


def test_every_scenario_states_its_discrimination(loaded):
    _, scen, _ = loaded
    for sid, s in scen.items():
        d = s.get("discrimination")
        assert isinstance(d, dict), sid
        for f in ("verdict", "worth_recording", "measured", "note"):
            assert d.get(f) not in (None, ""), f"{sid}: discrimination.{f} missing"
        any_red = any(not H._claim(c)[0] for c in s["self_test_expect"].values())
        if any_red:
            # an undiscriminating scenario must be flagged, not just recorded
            assert d["worth_recording"] is not True, f"{sid}: red claims but worth_recording=True"


def test_unpromotable_scenarios_are_flagged(loaded):
    _, scen, _ = loaded
    for sid, s in scen.items():
        if not s.get("promotes"):
            assert s.get("promotes_missing_key"), sid
            assert s["discrimination"]["worth_recording"] is not True, sid


def test_stored_predictions_match_the_backend(loaded):
    """'predictions' are computed, never typed: recompute every one."""
    doc, scen, be = loaded
    for sid, s in scen.items():
        fresh = json.loads(json.dumps(H.predict(doc, s, be)))
        assert s.get("predictions") == fresh, f"{sid}: stored predictions drifted from --predict"


def test_prediction_drift_is_detected(loaded):
    """Plant for the test above: a single nudged number must not compare equal."""
    doc, scen, be = loaded
    s = scen["S01_speed_unit"]
    fresh = json.loads(json.dumps(H.predict(doc, s, be)))
    nudged = json.loads(json.dumps(s["predictions"]))
    nudged["candidates"]["tiles_per_minute"]["knight_s_y18_to_y22"][0] += 0.001
    assert nudged != fresh


def test_prerequisite_pins_are_consistent(loaded):
    _, scen, _ = loaded
    seen = 0
    for sid, s in scen.items():
        for param, req in s.get("prerequisite_pins", {}).items():
            seen += 1
            assert param in s["nuisance"], f"{sid}: pin {param} is not a nuisance"
            assert param not in s.get("per_trial_nuisance", []), sid
            assert req["scenario"] in scen, f"{sid}: pin names unknown {req['scenario']}"
            assert req["scenario"] in s["depends_on_scenarios"], sid
            pre = scen[req["scenario"]]
            assert pre["focal_param"] == param, f"{sid}: {req['scenario']} does not settle {param}"
    assert seen >= 1, "vacuity: no scenario declares a prerequisite pin"


def test_trace_format_constants_agree():
    assert extract_tracks.TRACE_FORMAT == synth.TRACE_FORMAT


@pytest.mark.slow
def test_self_test_upholds_every_claim():
    """The gate itself, run as the checker list runs it; read the output,
    not just the exit code."""
    p = subprocess.run(
        [PY, str(ROOT / "tools" / "diff_harness.py"), "--self-test", "--quiet"],
        capture_output=True,
        text=True,
        cwd=ROOT,
        timeout=900,
    )
    out = p.stdout + p.stderr
    assert "0 contradicted claim(s)" in out, out[-3000:]
    assert "self-test exit code 0" in out, out[-3000:]
    assert "Traceback" not in out, out[-3000:]
    assert p.returncode == 0, out[-3000:]
