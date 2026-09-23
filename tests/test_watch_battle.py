"""tools/watch_battle.py: every gate, and every plant landing on the gate it aims at.

WHY THIS EXISTS
    watch_battle.py is the only command that produces a battle you can watch,
    and it is a GATE as well as a demo -- so its
    own gates have to be shown capable of failing, or a green battle.html is
    decoration.  The tool carries its plants; this file is
    what makes pytest run them, so a later change to the env, the recorder, the
    renderer or the arena cannot quietly switch one of them off.

    It runs on the MockEngine so it needs no built extension, and on a short
    battle so it stays inside a normal test run.  The RUST path is exercised by
    ``test_baseline_is_green_on_the_rust_engine``, which SKIPS (loudly, via
    pytest.skip) when the extension is absent -- the tool itself never
    substitutes one engine for the other.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
for p in (ROOT, ROOT / "tools"):
    if str(p) not in sys.path:
        sys.path.insert(0, str(p))

# `tools/watch_battle.py` imports `royalegym.env`, which lives in the SIBLING RoyaleGym
# checkout. A RoyaleSim-only clone is a legitimate environment -- the README's stages 1 to 4
# install this repo alone, and the four-repo install is a later section -- so the absence is
# a skip rather than an error. Without this the whole module fails to IMPORT, which is how
# CI found it: not a failing assertion, a collection error on a clean runner.
pytest.importorskip(
    "royalegym",
    reason="SKIPPED, NOT PASSED: tools/watch_battle.py drives the engine through the env"
    " layer, which is the sibling RoyaleGym checkout. Install it with"
    r" `..\.venv\Scripts\python -m pip install -e ..\RoyaleGym` (forward slashes on"
    " macOS and Linux), per the README's four-repo install. Nothing about the viewer has been"
    " checked here.",
)

import watch_battle as W  # noqa: E402

PY = sys.executable
# Short enough for a test run, long enough that the vacuity floors are cleared by
# a working engine: measured on MockEngine, 90 steps gives >40 troops and hp drops.
STEPS = 90


def args(tmp_path: Path, **over: object) -> object:
    ns = W.argparse.Namespace(
        engine="mock",
        seed=3,
        steps=STEPS,
        decision_ms=500,
        noop_prob=0.2,
        out=tmp_path / "battle.html",
        trace_out=None,
        stride=1,
        open=False,
        plant=None,
        all_plants=False,
    )
    for k, v in over.items():
        setattr(ns, k, v)
    return ns


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> object:
    return W.run_once(args(tmp_path_factory.mktemp("base")), None)


def test_the_baseline_battle_is_green_on_every_gate(baseline: object) -> None:
    assert baseline.red == [], [(g.name, g.detail) for g in baseline.gates if not g.ok]
    assert {g.name for g in baseline.gates} == {
        "determinism",
        "vacuity",
        "arena",
        "dry",
        "render",
    }


def test_the_baseline_is_not_vacuous(baseline: object) -> None:
    # The numbers the vacuity gate's floors were set from. If a change makes a
    # working battle produce fewer than these, the floors are the thing to look
    # at -- not this assertion (a check that cannot fail).
    assert baseline.summary["troops"] >= W.MIN_TROOP_UIDS * 2
    assert baseline.summary["hp_drops"] > 0
    assert baseline.summary["max_displacement_subtiles"] > 1000
    assert baseline.summary["accepted"]["blue"] > 0
    assert baseline.summary["accepted"]["red"] > 0
    assert baseline.summary["ground_entity_positions_checked"] > 500


@pytest.mark.parametrize("plant", W.PLANTS)
def test_every_plant_lands_on_the_gate_it_aims_at(
    plant: str, tmp_path: Path, baseline: object
) -> None:
    assert baseline.red == [], "a plant is only evidence on a green baseline"
    rep = W.run_once(args(tmp_path, plant=plant), plant)
    aim = W.AIMED_AT[plant]
    assert aim in rep.red, (
        f"PLANT DID NOT LAND: {plant} left {aim} green (red: {rep.red}). "
        "Re-point the plant at the shipped code, or record the gate as uncertified."
    )


@pytest.fixture(scope="module")
def tiny_trace() -> object:
    """A two-step battle: crown towers only, so no troop exists to move."""
    trace, _ = W.play("mock", 3, 2, 500, 1.0)
    return trace


def test_an_unknown_plant_is_refused_not_ignored(tiny_trace: object) -> None:
    with pytest.raises(W.Skip, match="unknown plant"):
        W.apply_plant("not_a_plant", tiny_trace)


def test_a_plant_with_nothing_to_patch_refuses(tiny_trace: object) -> None:
    # The failure mode plants themselves have: a plant that
    # edits nothing grades a CLEAN battle and reads as caught. wet_troop on a
    # troopless trace is exactly that case, and it must refuse rather than return.
    assert not [e for f in tiny_trace.frames for e in f.entities if e.kind == W.EntityKind.TROOP]
    with pytest.raises(W.Skip, match="no ground troop"):
        W.apply_plant("wet_troop", tiny_trace)


def test_usage_errors_exit_2(tmp_path: Path) -> None:
    for argv in (["--stride", "0"], ["--steps", "0"], ["--noop-prob", "2"]):
        r = subprocess.run(
            [PY, str(ROOT / "tools" / "watch_battle.py"), "--engine", "mock", *argv],
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        assert r.returncode == 2, (argv, r.returncode, r.stdout, r.stderr)


def test_baseline_is_green_on_the_rust_engine(tmp_path: Path) -> None:
    try:
        W.make_engine("rust")
    except W.Skip as exc:
        pytest.skip(f"no usable Rust extension: {exc}")
    rep = W.run_once(args(tmp_path, engine="rust"), None)
    assert rep.red == [], [(g.name, g.detail) for g in rep.gates if not g.ok]


def test_the_cli_writes_a_page_and_exits_0(tmp_path: Path) -> None:
    out = tmp_path / "b.html"
    r = subprocess.run(
        [
            PY,
            str(ROOT / "tools" / "watch_battle.py"),
            "--engine",
            "mock",
            "--steps",
            str(STEPS),
            "--noop-prob",
            "0.2",
            "--out",
            str(out),
        ],
        capture_output=True,
        text=True,
        cwd=ROOT,
    )
    assert r.returncode == 0, r.stdout + r.stderr
    assert out.is_file()
    assert out.stat().st_size > 10_000
    assert "EVERY GATE GREEN" in r.stdout
