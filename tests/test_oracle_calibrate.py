"""oracle/calibrate.py: the one tool that writes `measured` must refuse everything that
is not a measurement.  Every test works on a COPY of data/calibration.json in a temp
dir; a module fixture proves the real registry is byte-identical afterwards."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
for p in (ROOT, ROOT / "tools"):
    if str(p) not in sys.path:
        sys.path.insert(0, str(p))

from oracle import calibrate as C  # noqa: E402
from oracle.extract_tracks import sha256_file  # noqa: E402

REAL = ROOT / "data" / "calibration.json"
PY = sys.executable


@pytest.fixture(scope="module", autouse=True)
def real_registry_untouched():
    before = sha256_file(REAL)
    yield
    assert sha256_file(REAL) == before, "a calibrate test modified data/calibration.json"


@pytest.fixture
def fx(tmp_path):
    f = C.build_fixture(tmp_path / "fx")
    assert f.registry.resolve() != REAL.resolve()
    return f


def _refused_on(fx, gate: str, **kw):
    with pytest.raises(C.Refusal) as ei:
        C.promote([fx.result], fx.registry, status=kw.pop("status", fx.status), **kw)
    assert ei.value.gate == gate, str(ei.value)
    return str(ei.value)


def test_baseline_promotion_keeps_history_and_evidence(fx):
    before = json.loads(fx.registry.read_text(encoding="utf-8"))
    new, diff = C.promote([fx.result], fx.registry, status="hypothesis")
    on_disk = json.loads(fx.registry.read_text(encoding="utf-8"))
    assert on_disk == new
    node = on_disk["time"]["SPEED_TO_SUBTILES_PER_TICK"]
    old = before["time"]["SPEED_TO_SUBTILES_PER_TICK"]
    assert node["value"] == 18
    assert node["status"] == "hypothesis"
    assert node["history"][-1]["value"] == old["value"] == 15
    assert node["history"][-1]["status"] == old["status"]
    assert node["history"][-1]["provenance"] == old["provenance"]
    ev = node["evidence"][-1]
    assert ev["results"][0]["scenario_id"] == "S01_speed_unit"
    assert ev["results"][0]["recordings"][0]["video_sha256"] == sha256_file(fx.video)
    assert '-      "value": 15' in diff
    assert '+      "value": 18' in diff


def test_dry_run_writes_nothing_and_prints_the_diff(fx):
    before = fx.registry.read_bytes()
    _, diff = C.promote([fx.result], fx.registry, status="hypothesis", dry_run=True)
    assert fx.registry.read_bytes() == before
    assert '+      "value": 18' in diff
    rc = C.main(
        [
            "--result",
            str(fx.result),
            "--registry",
            str(fx.registry),
            "--status",
            "hypothesis",
            "--dry-run",
        ]
    )
    assert rc == 0
    assert fx.registry.read_bytes() == before


def test_synthetic_trace_is_refused(tmp_path):
    syn = C.build_fixture(tmp_path / "syn", synthetic=True)
    msg = _refused_on(syn, "not-synthetic", status="hypothesis")
    assert "never evidence" in msg


def test_missing_evidence_is_refused(fx):
    res = json.loads(fx.result.read_text(encoding="utf-8"))
    res.pop("recorded")
    fx.result.write_text(json.dumps(res), encoding="utf-8")
    _refused_on(fx, "evidence-attached")


def test_missing_video_is_refused(fx):
    fx.video.unlink()
    _refused_on(fx, "evidence-attached")


def test_downgrade_is_refused(fx):
    reg = json.loads(fx.registry.read_text(encoding="utf-8"))
    reg["time"]["SPEED_TO_SUBTILES_PER_TICK"]["status"] = "datamined"
    fx.registry.write_text(json.dumps(reg), encoding="utf-8")
    before = fx.registry.read_bytes()
    _refused_on(fx, "no-downgrade", status="hypothesis")
    assert fx.registry.read_bytes() == before, "a refused promotion wrote the registry"


def test_measured_needs_measured_dependencies(fx):
    msg = _refused_on(fx, "dependencies-measured", status="measured")
    assert "time.TICK_MS" in msg


def test_measured_conflict_needs_supersede(fx):
    reg = json.loads(fx.registry.read_text(encoding="utf-8"))
    reg["time"]["TICK_MS"]["status"] = "measured"
    reg["time"]["SPEED_TO_SUBTILES_PER_TICK"]["status"] = "measured"
    fx.registry.write_text(json.dumps(reg), encoding="utf-8")
    _refused_on(fx, "no-silent-conflict", status="measured")
    new, _ = C.promote([fx.result], fx.registry, status="measured", supersede="test: re-measured")
    node = new["time"]["SPEED_TO_SUBTILES_PER_TICK"]
    assert node["value"] == 18
    assert node["status"] == "measured"
    assert node["history"][-1]["status"] == "measured"
    assert node["evidence"][-1]["supersede_reason"] == "test: re-measured"


def test_record_only_changes_no_value_or_status(fx):
    before = json.loads(fx.registry.read_text(encoding="utf-8"))
    new, _ = C.promote([fx.result], fx.registry, status="measured", record_only=True)
    node, old = (
        new["time"]["SPEED_TO_SUBTILES_PER_TICK"],
        before["time"]["SPEED_TO_SUBTILES_PER_TICK"],
    )
    assert (node["value"], node["status"]) == (old["value"], old["status"])
    assert node["evidence"][-1]["applied"] is False
    assert "history" not in node


def test_tampered_result_is_refused(fx):
    res = json.loads(fx.result.read_text(encoding="utf-8"))
    res["winner"], res["runner_up"] = res["runner_up"], res["winner"]
    fx.result.write_text(json.dumps(res), encoding="utf-8")
    _refused_on(fx, "reproducible")


def test_result_for_another_scenario_than_its_trace_is_refused(fx):
    res = json.loads(fx.result.read_text(encoding="utf-8"))
    res["scenario_id"] = "S02_lane_snap_vs_diagonal"
    fx.result.write_text(json.dumps(res), encoding="utf-8")
    assert "was extracted for" in _refused_on(fx, "evidence-attached")


def test_unpromotable_scenario_is_refused_on_key_exists(fx):
    _, scen = C.harness.load_scenarios()
    for sid in ("S07_deploy_snap", "S08_cannon_pull_hog"):
        res = {"winner": next(iter(scen[sid]["candidates"]))}
        b = C.Bundle(fx.result, "0" * 64, res, scen[sid], [{}])
        with pytest.raises(C.Refusal) as ei:
            C.plan_changes([b], json.loads(fx.registry.read_text(encoding="utf-8")))
        assert ei.value.gate == "key-exists"
        assert "promotes no registry key" in str(ei.value)


def test_winner_without_a_registry_value_is_refused(fx):
    _, scen = C.harness.load_scenarios()
    b = C.Bundle(fx.result, "0" * 64, {"winner": "30"}, scen["S04_tick_rate_phase_lock"], [{}])
    with pytest.raises(C.Refusal) as ei:
        C.plan_changes([b], json.loads(fx.registry.read_text(encoding="utf-8")))
    assert ei.value.gate == "value-mapped"


def test_value_map_formulas_are_exact():
    cal = json.loads(REAL.read_text(encoding="utf-8"))
    assert C._eval_formula("SUBTILE_PER_TILE / (60 * TPS)", cal) == 15
    assert C._eval_formula("SUBTILE_PER_TILE / MILLITILE_PER_TILE * 1000 / (50 * TPS)", cal) == 18
    assert (
        C._eval_formula("lane_flow_with_local_avoidance", cal) == "lane_flow_with_local_avoidance"
    )
    with pytest.raises(C.Refusal, match="not an integer"):
        C._eval_formula("SUBTILE_PER_TILE / 7", cal)


def test_status_order_covers_every_status_in_the_registry():
    cal = json.loads(REAL.read_text(encoding="utf-8"))
    seen = set()

    def walk(n):
        if isinstance(n, dict):
            if "status" in n and "value" in n:
                seen.add(n["status"])
            for k, v in n.items():
                if not k.startswith("$"):
                    walk(v)

    walk(cal)
    assert seen, "vacuity: no statuses found"
    assert seen <= set(C.STATUS_RANK), seen - set(C.STATUS_RANK)


@pytest.mark.slow
def test_calibrate_self_test_and_plants_land():
    """The tool's own gate, read from its output (every plant must say LANDED)."""
    p = subprocess.run(
        [PY, str(ROOT / "oracle" / "calibrate.py"), "--self-test"],
        capture_output=True,
        text=True,
        cwd=ROOT,
        timeout=600,
    )
    out = p.stdout + p.stderr
    for name in C.PLANTS:
        assert f"PLANT '{name}' LANDED" in out, f"{name}:\n{out[-3000:]}"
    assert "calibrate self-test green" in out, out[-3000:]
    assert p.returncode == 0, out[-3000:]


def test_joint_key_needs_both_results(tmp_path, fx):
    """ALGORITHM is promoted jointly by S02 and S03; one half alone is refused."""
    res = json.loads(fx.result.read_text(encoding="utf-8"))
    res["scenario_id"] = "S02_lane_snap_vs_diagonal"
    res["winner"] = "diagonal"
    res["decisive"] = True
    fx.result.write_text(json.dumps(res), encoding="utf-8")
    _doc, scen = C.harness.load_scenarios()
    b = C.Bundle(fx.result, "0" * 64, res, scen["S02_lane_snap_vs_diagonal"], [{}])
    cal = json.loads(fx.registry.read_text(encoding="utf-8"))
    with pytest.raises(C.Refusal) as ei:
        C.plan_changes([b], cal)
    assert ei.value.gate == "joint-agrees"
    shutil.rmtree(tmp_path / "fx", ignore_errors=True)
