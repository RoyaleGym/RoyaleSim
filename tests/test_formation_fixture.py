"""tools/make_formation_fixture.py: the committed measurement tests/formations.rs pins
(crates/royalesim/tests/fixtures/formations/measured.json) has the shape the Rust test
reads, and the maker's clean-group rule does what it says on the committed replay
sample alone (its one Skeletons deploy hugs the Blue king: excluded by the tower margin,
a clean three-member group without it, deployed after Red's own-left princess fell)."""

from __future__ import annotations

import importlib.util
import json
import os

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures")
SAMPLE = os.path.join(FIXTURES, "replay", "sample.json")
MEASURED = os.path.join(FIXTURES, "formations", "measured.json")


def _load():
    spec = importlib.util.spec_from_file_location(
        "make_formation_fixture", os.path.join(ROOT, "tools", "make_formation_fixture.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


@pytest.fixture(scope="module")
def m():
    return _load()


def test_the_committed_measurement_has_the_shape_the_gate_reads(m):
    with open(MEASURED, encoding="utf-8") as fh:
        doc = json.load(fh)
    groups = doc["groups"]
    assert len(groups) >= 20
    cards = {g["card"] for g in groups}
    assert len(cards) >= 12
    for g in groups:
        assert g["side"] in (0, 1)
        assert g["source"] in ("tap_tile", "centroid")
        assert len(g["tap"]) == 2
        assert all(isinstance(v, int) for v in g["tap"])
        assert len(g["members"]) >= 2
        for mem in g["members"]:
            assert len(mem["offset"]) == 2
            assert isinstance(mem["stagger"], int)
        for side, slot in g["towers_down"]:
            assert side in (0, 1)
            assert slot in (0, 1, 2)
    # both seats and both lanes are represented
    assert {g["side"] for g in groups} == {0, 1}
    assert {g["tap"][0] >= m.CENTRE_X for g in groups} == {True, False}


def test_the_sample_s_skeletons_hug_the_king_and_are_clean_only_without_the_margin(m):
    # The sample's three cycled Skeletons (tap (8500, 1500), laid against the Blue
    # king's box): a member sits inside the box widened by TOWER_MARGIN, so the
    # group is not clean; without the margin it is, three members, one deploy end
    # for all three (no SummonDeployDelay), Red's engine-left princess (slot 1)
    # already down (the Prince took it at tick 454: the documented battle).
    assert [g["card"] for g in m.collect([SAMPLE])] == []
    m.TOWER_MARGIN = 0
    try:
        groups = m.collect([SAMPLE])
    finally:
        m.TOWER_MARGIN = 500
    sk = [g for g in groups if g["card"] == "Skeletons"]
    assert len(sk) == 1
    g = sk[0]
    assert len(g["members"]) == 3
    assert g["towers_down"] == [[1, 1]]
    assert len({mem["stagger"] for mem in g["members"]}) == 1
    # the selection keeps at most PER_BUCKET per (card, side, lane half, tap row)
    def key(h):
        return (h["card"], h["side"], h["tap"][0] >= m.CENTRE_X, h["tap"][1] // m.TILE_NATIVE)

    picked = m.select(groups * 3)
    for b in {key(h) for h in picked}:
        assert len([h for h in picked if key(h) == b]) <= m.PER_BUCKET


def test_a_group_shown_late_takes_its_slack_from_its_deploy_end_range_not_the_frame_gap(m):
    # A frame-gap row: the spawn lies in (first_seen - gap, first_seen], the latest taken.
    gap_row = {
        "tick": 100,
        "first_seen": 100,
        "first_seen_gap": 3,
        "tick_evidence": "range [98, 100] (frame gap, no transition seen), latest used",
    }
    assert m.spawn_tick_slack(gap_row) == 2
    # A group the capture showed late (make_replay_fixture.py shown_late_spawn): its first
    # frame is 3 ticks after the range its deploy ends leave, so the gap bounds nothing.
    shown = "the capture shows the group from 572, 3 tick(s) after it"
    late = {
        "tick": 569,
        "first_seen": 572,
        "first_seen_gap": 1,
        "tick_evidence": f"range [567, 569] (deploy-end transition; {shown}), latest used",
    }
    assert m.spawn_tick_slack(late) == 2
    assert m.spawn_tick_slack({**late, "tick_evidence": f"exact (deploy-end transition; {shown})"}) == 0
