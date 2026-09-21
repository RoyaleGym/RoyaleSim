"""tools/diff_harness.py: the pieces that decide a verdict, and every plant landing."""

from __future__ import annotations

import copy
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


def _s09_recs(loaded, cand: str, speed: str, seed: int = 3):
    doc, scen, be = loaded
    scn = scen["S09_repath_lag"]
    rows = H.truth_rows(be, doc, scn)[cand]
    cfg = next(c for a, c in rows if a["speed_unit"] == speed)
    return scn, H.synth_repetitions(be, doc, scn, cfg, seed)


def test_unpinned_prerequisite_blocks_a_decisive_verdict(loaded):
    doc, _, be = loaded
    scn, recs = _s09_recs(loaded, "10", "tiles_per_minute")
    res = H.rank(recs, scn, doc, be)
    assert res["blocked"]
    assert "speed_unit" in res["blocked"]
    assert res["decisive"] is False
    assert any(f.startswith("prerequisite-pinned") for f in H.gate(res, "10"))


def test_pinned_prerequisite_decides(loaded):
    doc, _, be = loaded
    scn, recs = _s09_recs(loaded, "10", "tiles_per_minute")
    res = H.rank(recs, scn, doc, be, pins={"speed_unit": "tiles_per_minute"})
    assert res["blocked"] is None
    assert res["winner"] == "10", res["margin"]
    assert res["decisive"], res["margin"]


def test_bad_pins_are_usage_errors(loaded):
    doc, scen, be = loaded
    with pytest.raises(ValueError, match="not one of"):
        H.candidate_configs(be, doc, scen["S09_repath_lag"], {"speed_unit": "furlongs"})
    with pytest.raises(ValueError, match="not a nuisance"):
        H.candidate_configs(be, doc, scen["S09_repath_lag"], {"tps": 20})


def test_registry_pins_require_a_measured_prerequisite(loaded):
    _, scen, be = loaded
    scn = scen["S09_repath_lag"]
    cal = json.loads((ROOT / "data" / "calibration.json").read_text(encoding="utf-8"))
    # The shipped registry has the speed multiplier MEASURED (18 subtiles per Speed
    # unit per tick), so it pins here -- which is the whole point of the rule. The
    # unmeasured half of the property is kept by rewinding the status on a copy; the
    # test is about the RULE, not about today's registry contents.
    assert H.registry_pins(scen, scn, be, cal) == {
        "speed_unit": synth.base_config(cal).speed_unit
    }
    hypothesis = copy.deepcopy(cal)
    hypothesis["time"]["SPEED_TO_SUBTILES_PER_TICK"]["status"] = "hypothesis"
    assert H.registry_pins(scen, scn, be, hypothesis) == {}


def test_global_nuisance_is_shared_across_repetitions(loaded):
    """The superseded per-recording rule must still be reachable (plants use it) and
    the default must group by global assignment: every repetition's best assignment
    for a candidate carries the same speed_unit."""
    doc, _, be = loaded
    scn, recs = _s09_recs(loaded, "1", "millitiles_per_50ms")
    res = H.rank(recs, scn, doc, be, pins={"speed_unit": "millitiles_per_50ms"})
    for row in res["ranking"]:
        assert row["best_assignment"]["speed_unit"] == "millitiles_per_50ms"
    old = H.rank(recs, scn, doc, be, pins=None, profile_global_per_recording=True)
    assert old["ranking"], "superseded rule no longer runs"


def test_truth_rows_cover_every_global_nuisance_value(loaded):
    doc, scen, be = loaded
    rows = H.truth_rows(be, doc, scen["S01_speed_unit"])
    for cand, r in rows.items():
        combos = {(a["tps"], a["path_model"]) for a, _ in r}
        assert len(combos) == 6, (cand, combos)  # 3 tps x 2 path models
    s09 = H.truth_rows(be, doc, scen["S09_repath_lag"])
    # per-trial nuisance (repath phase) is NOT enumerated as a truth row
    assert all(len(r) == 2 for r in s09.values()), {k: len(v) for k, v in s09.items()}


def test_effective_scenario_uses_observed_placements(loaded):
    _, scen, _ = loaded
    scn = scen["S03_building_lookahead"]
    rec = {
        "observed_placements": {
            "cannon": {"tile": [3.5, 12.0], "t": 5.0},
            "knight": {"tile": [3.4, 8.6], "t": 6.6},
        }
    }
    eff = H.effective_scenario(scn, rec)
    acts = {a["label"]: a for a in eff["setup"]["actions"]}
    assert acts["cannon"]["tile_100"] == [350, 1200]
    assert acts["knight"]["tile_100"] == [340, 860]
    assert acts["cannon"]["t_ms"] == 0
    assert acts["knight"]["t_ms"] == 1600
    assert eff["id"] != scn["id"], "cache key must change with the placements"
    assert scn["setup"]["actions"][0]["tile_100"] == [400, 1200], "scenario mutated in place"
    assert H.effective_scenario(scn, {}) is scn


def test_effective_scenario_handles_variants(loaded):
    _, scen, _ = loaded
    scn = scen["S05_push_mass_ladder"]
    rec = {
        "observed_placements": {
            "knight.hog": {"tile": [3.6, 8.5], "t": 0.2},
            "knight.blocker": {"tile": [3.5, 10.5], "t": 0.0},
        }
    }
    eff = H.effective_scenario(scn, rec)
    v = {x["name"]: x for x in eff["variants"]}
    hog = next(a for a in v["knight"]["actions"] if a["label"] == "hog")
    assert hog["tile_100"] == [360, 850]
    assert hog["t_ms"] == 200
    skel_hog = next(a for a in v["skeleton"]["actions"] if a["label"] == "hog")
    assert skel_hog["tile_100"] == [375, 850], "an unobserved variant must keep its script"


def test_recorded_cli_round_trip(tmp_path, loaded):
    """A synthetic recording written by the extractor's writer is read by the CLI, and
    the result records the exact bytes it scored."""
    doc, scen, be = loaded
    scn = scen["S01_speed_unit"]
    cfg = H.truth_rows(be, doc, scn)["millitiles_per_50ms"][0][1]
    rec = H.synth_recording(be, doc, scn, cfg, 99)
    trace = tmp_path / "rec.json"
    sha = extract_tracks.write_trace(trace, rec, "trajectory")
    out = tmp_path / "result.json"
    rc = H.main(
        ["--scenario", "S01_speed_unit", "--recorded", str(trace), "--out", str(out), "--quiet"]
    )
    res = json.loads(out.read_text(encoding="utf-8"))
    assert rc == 0
    assert res["winner"] == "millitiles_per_50ms"
    assert res["decisive"]
    assert res["recorded"][0]["sha256"] == sha
    assert res["recorded"][0]["source_kind"] == "synthetic_recording"


def test_recorded_cli_rejects_a_malformed_trace(tmp_path, capsys):
    bad = {
        "format": "oracle-trace/1",
        "time_unit": "s",
        "space_unit": "tile",
        "source": {"kind": "sim"},
        "events": [],
        "units": {
            "knight": {
                "card": "Knight",
                "team": "blue",
                "t": [0, 1, 2],
                "x": [640, 641, 642],
                "y": [900, 950, 1000],
            }
        },
    }
    p = tmp_path / "pixels.json"
    p.write_text(json.dumps(bad), encoding="utf-8")
    rc = H.main(["--scenario", "S01_speed_unit", "--recorded", str(p), "--quiet"])
    assert rc == 2
    assert "pixels or a flipped axis" in capsys.readouterr().err


@pytest.mark.slow
def test_every_harness_plant_lands():
    p = subprocess.run(
        [PY, str(ROOT / "tools" / "diff_harness.py"), "--plant", "all"],
        capture_output=True,
        text=True,
        cwd=ROOT,
        timeout=900,
    )
    out = p.stdout + p.stderr
    names = [*H.PLANTS, *H.SELFTEST_PLANTS]
    for n in names:
        assert f"PLANT '{n}' LANDED" in out, f"{n}:\n{out[-4000:]}"
    assert "DID NOT LAND" not in out
    assert "INCONCLUSIVE" not in out
    assert "DID NOT APPLY" not in out
    assert p.returncode == 0, out[-4000:]
