"""oracle/extract_tracks.py: the trace schema the harness reads, the homography, the
automatic river calibration and the blob tracker -- all on synthetic inputs, because
no real recording exists yet and OpenCV is not installed (the module must not need it)."""

from __future__ import annotations

import builtins
import copy
import json
import math
import sys
from pathlib import Path

import numpy as np
import pytest

ROOT = Path(__file__).resolve().parent.parent
for p in (ROOT, ROOT / "tools"):
    if str(p) not in sys.path:
        sys.path.insert(0, str(p))

from oracle import extract_tracks as X  # noqa: E402
from oracle import synth  # noqa: E402

import diff_harness as H  # noqa: E402

ARENA = json.loads((ROOT / "data" / "derived" / "arena.json").read_text(encoding="utf-8"))


@pytest.fixture(scope="module")
def loaded():
    doc, scen = H.load_scenarios()
    return doc, scen, H.load_backend(None)


def _synthetic(loaded, sid: str, cand: str, seed: int = 4) -> dict:
    doc, scen, be = loaded
    scn = scen[sid]
    cfg = H.truth_rows(be, doc, scn)[cand][0][1]
    return H.synth_recording(be, doc, scn, cfg, seed)


# --------------------------------------------------------------------------- #
# schema                                                                      #
# --------------------------------------------------------------------------- #


@pytest.mark.parametrize(
    ("sid", "cand"),
    [
        ("S01_speed_unit", "tiles_per_minute"),
        ("S05_push_mass_ladder", "equal_split"),
        ("S04_tick_rate_phase_lock", "20"),
        ("S07_deploy_snap", "tile"),
    ],
)
def test_synthetic_traces_validate_and_round_trip_through_the_writer(tmp_path, loaded, sid, cand):
    """The schema test the protocol asks for: what synth.py renders is exactly what the
    extractor's writer accepts, byte-for-byte what the harness reads back, and the
    harness ranks it."""
    doc, scen, be = loaded
    kind = scen[sid]["kind"]
    rec = _synthetic(loaded, sid, cand)
    X.validate_trace(rec, kind)
    path = tmp_path / f"{sid}.json"
    sha = X.write_trace(path, rec, kind)
    assert sha == X.sha256_file(path)
    back = json.loads(path.read_text(encoding="utf-8"))
    assert back == json.loads(json.dumps(rec))
    res = H.rank(back, scen[sid], doc, be)
    assert res["winner"] == cand


def _minimal() -> dict:
    return {
        "format": X.TRACE_FORMAT,
        "scenario_id": "S01_speed_unit",
        "time_unit": "s",
        "space_unit": "tile",
        "source": {"kind": "sim"},
        "units": {
            "knight": {
                "card": "Knight",
                "team": "blue",
                "t": [0.0, 0.1, 0.2],
                "x": [7.0, 7.0, 7.0],
                "y": [9.5, 9.6, 9.7],
            }
        },
        "events": [],
    }


def test_minimal_trace_is_valid():
    X.validate_trace(_minimal(), "trajectory")


BREAKS = {
    "format": lambda d: d.__setitem__("format", "oracle-trace/0"),
    "units_of_time": lambda d: d.__setitem__("time_unit", "ms"),
    "lengths": lambda d: d["units"]["knight"]["x"].pop(),
    "pixels": lambda d: d["units"]["knight"].__setitem__("x", [640.0, 641.0, 642.0]),
    "nan": lambda d: d["units"]["knight"]["y"].__setitem__(1, float("nan")),
    "reordered_time": lambda d: d["units"]["knight"].__setitem__("t", [0.0, 0.2, 0.1]),
    "team": lambda d: d["units"]["knight"].__setitem__("team", "green"),
    "source_kind": lambda d: d["source"].__setitem__("kind", "vibes"),
    "video_without_hash": lambda d: d.__setitem__("source", {"kind": "video"}),
    "bool_as_number": lambda d: d["units"]["knight"]["t"].__setitem__(0, True),
    "empty_units": lambda d: d.__setitem__("units", {}),
    "event_without_time": lambda d: d.__setitem__("events", [{"name": "spawn"}]),
    "placement_shape": lambda d: d.__setitem__("observed_placements", {"knight": {"tile": [1]}}),
}


@pytest.mark.parametrize("name", sorted(BREAKS))
def test_each_schema_break_is_rejected(name):
    d = copy.deepcopy(_minimal())
    BREAKS[name](d)
    assert d != _minimal(), "plant edit changed nothing"
    with pytest.raises(X.TraceSchemaError):
        X.validate_trace(d, "trajectory")


def test_kind_vacuity_guards():
    ev = dict(_minimal(), units={}, events=[{"name": "appear", "t": 1.0}])
    with pytest.raises(X.TraceSchemaError, match="needs >= 2"):
        X.validate_trace(ev, "event_phase")
    lat = dict(_minimal(), units={}, spawn_positions=[[1.0, 9.0]] * 7)
    with pytest.raises(X.TraceSchemaError, match=">= 8"):
        X.validate_trace(lat, "deploy_lattice")
    with pytest.raises(X.TraceSchemaError, match="unknown scenario kind"):
        X.validate_trace(_minimal(), "trajectry")


def test_video_trace_requirements():
    d = _minimal()
    d["source"] = {
        "kind": "video",
        "video_path": "a.mp4",
        "video_sha256": "ab" * 32,
        "capture_fps": 60.0,
        "extractor": "oracle.extract_tracks",
        "method": "auto",
    }
    X.validate_trace(d, "trajectory")
    d["source"]["video_sha256"] = "AB" * 32
    with pytest.raises(X.TraceSchemaError, match="lowercase sha256"):
        X.validate_trace(d, "trajectory")


# --------------------------------------------------------------------------- #
# landmarks and homography                                                    #
# --------------------------------------------------------------------------- #


def test_landmarks_are_derived_from_arena_json():
    marks = X.arena_landmarks(ARENA)
    assert len(marks) == 8
    half = ARENA["half_tiles_per_tile"]
    y_blue = ARENA["water_half_rows"][0] / half
    y_red = (ARENA["water_half_rows"][1] + 1) / half
    xs = sorted({b["x_min"] for b in ARENA["bridges"]} | {b["x_max"] for b in ARENA["bridges"]})
    assert sorted({x for x, _ in marks.values()}) == xs
    assert sorted({y for _, y in marks.values()}) == [y_blue, y_red]
    # derived means derived: a different arena gives different landmarks
    wide = copy.deepcopy(ARENA)
    wide["bridges"][0]["x_min"] -= 0.5
    assert X.arena_landmarks(wide) != marks


def _h_true() -> np.ndarray:
    """tile -> pixel for a 540x960 portrait frame with perspective (far side narrower)."""
    tiles = np.array([[0, 0], [18, 0], [0, 32], [18, 32]], float)
    px = np.array([[20, 940], [520, 940], [80, 40], [460, 40]], float)
    return X.fit_homography(tiles, px)


def test_homography_recovers_a_known_map():
    Ht = _h_true()
    rng = np.random.default_rng(1)
    tiles = rng.uniform([0, 0], [18, 32], size=(12, 2))
    pixels = X.apply_homography(Ht, tiles)
    H_px = X.fit_homography(pixels, tiles)
    assert X.reprojection_rms(H_px, pixels, tiles) < 1e-6
    noisy = pixels + rng.normal(0, 0.5, pixels.shape)
    assert X.reprojection_rms(X.fit_homography(noisy, tiles), noisy, tiles) < 0.05


def test_homography_rejects_degenerate_input():
    line = np.array([[0, 0], [1, 1], [2, 2], [3, 3]], float)
    with pytest.raises(ValueError, match="degenerate"):
        X.fit_homography(line, line * 2)
    with pytest.raises(ValueError, match=">= 4"):
        X.fit_homography(line[:3], line[:3])


WATER = (50, 140, 210)
GRASS = (90, 160, 60)
WOOD = (150, 110, 70)
OUTSIDE = (20, 20, 20)


def render_arena(Ht: np.ndarray, shape=(960, 540)) -> np.ndarray:
    """A flat-colour top-down arena seen through Ht: grass, a river with two bridges
    placed where arena.json says, nothing else.  Pixel centres at +0.5."""
    h, w = shape
    ys, xs = np.mgrid[0:h, 0:w]
    t = X.apply_homography(np.linalg.inv(Ht), np.c_[xs.ravel() + 0.5, ys.ravel() + 0.5])
    tx, ty = t[:, 0].reshape(h, w), t[:, 1].reshape(h, w)
    y_blue, y_red = X.river_edges(ARENA)
    img = np.empty((h, w, 3), np.uint8)
    img[:] = OUTSIDE
    inside = (tx >= 0) & (tx <= 18) & (ty >= 0) & (ty <= 32)
    img[inside] = GRASS
    river = inside & (ty >= y_blue) & (ty < y_red)
    img[river] = WATER
    for b in ARENA["bridges"]:
        img[river & (tx >= b["x_min"]) & (tx < b["x_max"])] = WOOD
    return img


def test_auto_calibration_finds_the_bridges_on_a_rendered_arena():
    Ht = _h_true()
    frame = render_arena(Ht)
    pixels = X.auto_landmarks(frame, ARENA)
    assert len(pixels) == 8
    H_px, rms, _marks = X.homography_from_landmarks(pixels, ARENA)
    assert rms < 0.02, rms
    rng = np.random.default_rng(2)
    # near the river the eight bridge corners are enough...
    near = rng.uniform([1, 12], [17, 20], size=(40, 2))
    err = X.apply_homography(H_px, X.apply_homography(Ht, near)) - near
    assert float(np.abs(err).max()) < 0.1, float(np.abs(err).max())
    # ...and far from it they are not, which is what the uncertainty gate exists for
    far = rng.uniform([1, 1], [17, 8], size=(40, 2))
    ferr = X.apply_homography(H_px, X.apply_homography(Ht, far)) - far
    assert float(np.abs(ferr).max()) > 0.15, float(np.abs(ferr).max())


def _clicked_marks(Ht: np.ndarray, keep=lambda n: True, sigma: float = 1.5, seed: int = 0):
    ext = X.arena_landmarks(ARENA, extended=True)
    rng = np.random.default_rng(seed)
    px = {
        n: tuple(X.apply_homography(Ht, np.array([t]))[0] + rng.normal(0, sigma, 2))
        for n, t in ext.items()
        if keep(n)
    }
    return X.homography_from_landmarks(px, ARENA)


def test_calibration_uncertainty_gate(loaded):
    """The residual cannot see this, so the gate resamples the landmark pixels:
    bridge corners alone are refused for a trajectory scenario, and adding the far
    landmarks passes.  The threshold is the declared capture budget, not a number
    invented here."""
    _, scen, _ = loaded
    Ht = _h_true()
    budget = synth.CaptureModel().offset_sigma_tiles
    auto_px = X.auto_landmarks(render_arena(Ht), ARENA)
    _, _, auto_marks = X.homography_from_landmarks(auto_px, ARENA)
    auto_doc = {"landmarks": auto_marks, "method": "auto", "pixel_sigma": 0.5}
    with pytest.raises(X.CalibrationTooUncertain, match="capture budget"):
        X.check_calibration(auto_doc, scen["S06_tower_footprint"], ARENA)
    assert auto_doc["uncertainty_tiles"] > budget
    assert X.check_calibration(auto_doc, scen["S06_tower_footprint"], ARENA, accept=True) > budget
    _, _, ext_marks = _clicked_marks(Ht)
    ext_doc = {"landmarks": ext_marks, "method": "manual", "pixel_sigma": 1.5}
    unc = X.check_calibration(ext_doc, scen["S06_tower_footprint"], ARENA)
    assert unc < budget, unc
    # a time-only scenario has no spatial region to be uncertain about
    assert X.check_calibration(auto_doc, scen["S04_tick_rate_phase_lock"], ARENA) == 0.0
    assert X.scenario_region(scen["S04_tick_rate_phase_lock"], ARENA) is None


def test_scenario_region_covers_the_whole_walk(loaded):
    _, scen, _ = loaded
    reg = X.scenario_region(scen["S01_speed_unit"], ARENA)
    assert reg[:, 1].min() <= 9.0, (reg[:, 1].min(), reg[:, 1].max())
    assert reg[:, 1].max() >= 22.0, (reg[:, 1].min(), reg[:, 1].max())
    lat = X.scenario_region(scen["S07_deploy_snap"], ARENA)
    assert lat[:, 0].min() <= 1.0
    assert lat[:, 1].max() >= 14.0


def test_auto_calibration_refuses_a_frame_without_a_river():
    frame = render_arena(_h_true())
    frame[(frame == np.array(WATER, np.uint8)).all(axis=2)] = GRASS
    with pytest.raises(ValueError, match="no river"):
        X.auto_landmarks(frame, ARENA)


def test_homography_sidecar_is_bound_to_its_video(tmp_path):
    H_px = np.linalg.inv(_h_true())
    side = tmp_path / "v.mp4.homography.json"
    X.save_homography(side, H_px, 0.03, [], "a" * 64, {"method": "auto"})
    H2, doc = X.load_homography(side, "a" * 64)
    assert np.allclose(H2, H_px)
    assert doc["method"] == "auto"
    with pytest.raises(ValueError, match="re-calibrate"):
        X.load_homography(side, "b" * 64)


# --------------------------------------------------------------------------- #
# detection, association, trace assembly                                      #
# --------------------------------------------------------------------------- #


def _disc(img: np.ndarray, cx: float, bottom: float, r: int = 9, rgb=(230, 60, 60)) -> None:
    h, w, _ = img.shape
    ys, xs = np.mgrid[0:h, 0:w]
    cy = bottom - r
    img[(xs + 0.5 - cx) ** 2 + (ys + 0.5 - cy) ** 2 <= r * r] = rgb


def test_label_components_separates_blobs():
    m = np.zeros((20, 30), bool)
    m[2:5, 2:6] = True
    m[10:15, 20:25] = True
    m[10, 5] = True
    lab, n = X.label_components(m)
    assert n == 3
    assert len({int(v) for v in lab[m]}) == 3


def test_tracker_follows_two_units_across_frames():
    Ht = _h_true()
    H_px = np.linalg.inv(Ht)
    bg = render_arena(Ht)
    scn = {
        "id": "T",
        "kind": "trajectory",
        "setup": {
            "actions": [
                {
                    "t_ms": 0,
                    "card": "Knight",
                    "team": "blue",
                    "tile_100": [700, 950],
                    "label": "knight",
                },
                {
                    "t_ms": 0,
                    "card": "Giant",
                    "team": "blue",
                    "tile_100": [1100, 950],
                    "label": "giant",
                },
            ]
        },
    }
    units = X.scripted_units(scn)
    assert [u.is_building for u in units] == [False, False]
    tracks = [X.TrackState(u, [], [], []) for u in units]
    truth = {"knight": [], "giant": []}
    t0_video = 12.0
    n_frames, deploy_frames = 100, 60  # troops stand still for their 1 s deploy
    for f in range(n_frames):
        t = t0_video + f / 60.0
        walked = max(0, f - deploy_frames)
        frame = bg.copy()
        for lbl, (x, y) in (
            ("knight", (7.0, 9.5 + walked * 0.02)),
            ("giant", (11.0, 9.5 + walked * 0.015)),
        ):
            px = X.apply_homography(Ht, np.array([[x, y]]))[0]
            _disc(frame, px[0], px[1])
            truth[lbl].append((x, y))
        blobs = X.detect_blobs(frame, bg, None, 45, 30, 2)
        dets = [tuple(p) for p in X.apply_homography(H_px, np.array([b.foot_px for b in blobs]))]
        X.associate(tracks, dets, t, t0_video)
    for tr in tracks:
        assert len(tr.t) == n_frames, (tr.unit.label, len(tr.t))
        err = np.hypot(
            np.array(tr.x) - [p[0] for p in truth[tr.unit.label]],
            np.array(tr.y) - [p[1] for p in truth[tr.unit.label]],
        )
        # foot = bottom-centre of the disc, quantised by the 2x downsample: well
        # inside the 0.08-tile foot-bias budget synth.CaptureModel assumes
        assert float(err.max()) < 0.12, (tr.unit.label, float(err.max()))
    trace = X.build_trajectory_trace(scn, tracks, {"kind": "sim"})
    X.validate_trace(trace, "trajectory")
    assert trace["units"]["knight"]["t"][0] == pytest.approx(0.0)
    assert set(trace["observed_placements"]) == {"knight", "giant"}
    kx, ky = trace["observed_placements"]["knight"]["tile"]
    assert math.hypot(kx - 7.0, ky - 9.5) < 0.12


def test_buildings_become_placements_and_events_not_tracks():
    scn = {
        "id": "T",
        "kind": "trajectory",
        "setup": {
            "actions": [
                {
                    "t_ms": 0,
                    "card": "Cannon",
                    "team": "blue",
                    "tile_100": [400, 1200],
                    "label": "cannon",
                },
                {
                    "t_ms": 1500,
                    "card": "Knight",
                    "team": "blue",
                    "tile_100": [350, 850],
                    "label": "knight",
                },
            ]
        },
    }
    units = X.scripted_units(scn)
    cannon, knight = (X.TrackState(u, [], [], []) for u in units)
    for k in range(10):
        X.associate([cannon, knight], [(4.0, 12.0)], 30.0 + k / 60, 30.0)
    for k in range(10):
        X.associate([cannon, knight], [(4.0, 12.0), (3.5, 8.5 + k * 0.02)], 31.5 + k / 60, 30.0)
    trace = X.build_trajectory_trace(scn, [cannon, knight], {"kind": "sim"})
    assert set(trace["units"]) == {"knight"}
    assert {e["unit"] for e in trace["events"]} == {"cannon", "knight"}
    assert trace["observed_placements"]["knight"]["t"] == pytest.approx(1.5, abs=0.02)
    X.validate_trace(trace, "trajectory")


def test_variant_scenarios_need_a_variant(loaded):
    _, scen, _ = loaded
    with pytest.raises(ValueError, match="--variant"):
        X.scripted_units(scen["S08_cannon_pull_hog"])
    labels = [u.label for u in X.scripted_units(scen["S08_cannon_pull_hog"], "far_first")]
    assert labels == ["far_first.cannon", "far_first.hog"]


# --------------------------------------------------------------------------- #
# OpenCV is optional                                                          #
# --------------------------------------------------------------------------- #


def test_video_commands_fail_cleanly_without_opencv(monkeypatch, tmp_path, capsys):
    real_import = builtins.__import__

    def no_cv2(name, *a, **k):
        if name == "cv2":
            raise ImportError("planted: no cv2")
        return real_import(name, *a, **k)

    monkeypatch.setattr(builtins, "__import__", no_cv2)
    monkeypatch.delitem(sys.modules, "cv2", raising=False)
    with pytest.raises(X.MissingDependency, match="pip install opencv-python"):
        X._cv2()
    rc = X.main(
        [
            "track",
            str(tmp_path / "none.mp4"),
            "--scenario",
            "S01_speed_unit",
            "--out",
            str(tmp_path / "t.json"),
        ]
    )
    err = capsys.readouterr().err
    assert rc == 2
    assert "SKIPPED" in err
    assert "opencv-python" in err


def test_schema_commands_run_without_opencv(tmp_path, loaded, capsys):
    assert X.main(["landmarks"]) == 0
    rec = _synthetic(loaded, "S02_lane_snap_vs_diagonal", "diagonal")
    p = tmp_path / "r.json"
    X.write_trace(p, rec, "trajectory")
    assert X.main(["validate", str(p), "--kind", "trajectory"]) == 0
    rec["units"]["knight"]["x"][0] = 700.0
    p.write_text(json.dumps(rec), encoding="utf-8")
    assert X.main(["validate", str(p), "--kind", "trajectory"]) == 1
    assert "INVALID" in capsys.readouterr().err


def test_synth_and_extractor_agree_on_label_names(loaded):
    """Variant units are prefixed 'variant.label' by the harness; the extractor must
    produce the same names or the harness reports every unit missing."""
    doc, scen, be = loaded
    scn = scen["S05_push_mass_ladder"]
    cfg = H.truth_rows(be, doc, scn)["mass_weighted"][0][1]
    sim = H.simulate_scenario(be, scn, cfg)
    extracted = {u.label for v in scn["variants"] for u in X.scripted_units(scn, v["name"])}
    assert set(sim["units"]) == extracted
    assert synth.TRACE_FORMAT == X.TRACE_FORMAT
