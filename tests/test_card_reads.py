"""`tools/check_card_reads.py`: the gate that catches a card the engine LOADS and
then runs without one of its mechanics.

WHAT THIS FILE IS FOR, AND WHAT IT IS NOT
    The gate's own passes are in the tool. What is here is the discipline around
    them: that the gate still goes RED when the defect it exists for is planted,
    that it goes red for the RIGHT reason, that its two hand-written tables cannot
    quietly rot, and that a checkout without the 15.535 card table, without the
    register and without a built extension module still gets a real answer or a
    loud skip.

    A green run of the gate is not evidence on its own. The evidence is
    `test_each_plant_lands`: four deliberate defects, each aimed at a different
    part of the chain, each of which must turn the gate red while the clean tree
    stays green.
"""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CARDS = os.path.join(ROOT, "data", "derived", "cards.json")
CARDS_2018 = os.path.join(ROOT, "data", "derived", "cards-2018.json")


def _load():
    tool = os.path.join(ROOT, "tools", "check_card_reads.py")
    spec = importlib.util.spec_from_file_location("check_card_reads", tool)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


ccr = _load()


@pytest.fixture(scope="module")
def cards():
    if not os.path.exists(CARDS):
        pytest.skip(f"{CARDS} is generated and absent; run tools/extract_cards.py")
    return CARDS


# --- the gate itself -------------------------------------------------------------


def test_the_gate_is_green(cards):
    """No card the engine loads carries a mechanic card.rs never reads, except the
    thin-slice gaps the tool lists by name."""
    r = ccr.run(cards)
    assert r.failures == [], "\n".join(r.failures)


def test_the_2018_table_scores_too():
    """The 2018 card table is the vintage a checkout without the modern bundle runs
    on. The gate has to be green there as well, and on its own slice."""
    if not os.path.exists(CARDS_2018):
        pytest.skip(f"{CARDS_2018} is generated and absent; run tools/extract_cards.py --vintage 2018")
    r = ccr.run(CARDS_2018)
    assert r.failures == [], "\n".join(r.failures)


# --- the evidence: each plant lands, and lands where it was aimed ----------------

# plant -> a fragment the failure it causes must contain. A plant that reddens the
# gate for some OTHER reason is not evidence for the pass it was aimed at.
PLANT_AIM = {
    "slice_mechanic": "Knight",
    "unread_field": "minimum_range_milli",
    "stale_gap": "retire the entry",
    "blind_ledger": "ChargeRange",
}


def test_plants_cover_every_plant_in_the_tool():
    assert set(PLANT_AIM) == set(ccr.PLANTS), "a plant was added or removed without an aim here"


@pytest.mark.parametrize("plant", sorted(PLANT_AIM))
def test_each_plant_lands(cards, plant):
    """The gate is green without the plant (test_the_gate_is_green) and red with it,
    with the failure naming what the plant broke."""
    r = ccr.run(cards, plant=plant)
    assert r.failures, f"plant {plant} did not redden the gate: it is certifying nothing"
    assert any(PLANT_AIM[plant] in f for f in r.failures), f"plant {plant} reddened the gate elsewhere: {r.failures}"


# --- the two hand-written tables cannot rot --------------------------------------


def test_the_prologue_table_still_covers_the_extractor(cards):
    """`ast` reads which card-table column becomes which cards.json field out of
    `norm_unit`'s dict literal; five columns are read before it and are listed by
    hand. `ccr.load` raises when that list stops covering the function, which is the
    only way a column can go missing from the map and so look unread."""
    ccr.load(cards)  # raises SystemExit with the missing names


def test_every_known_slice_gap_is_still_carried(cards):
    """An entry that no thin-slice card carries any more is a closed gap left open
    on paper. The gate fails on it; this pins that it is the gate's job."""
    r = ccr.run(cards)
    listed = {c for c, (v, _) in ccr.KNOWN_SLICE_GAPS.items() if v in ("15.535", "both")}
    assert listed == r.slice_gaps_seen, f"listed but not carried: {sorted(listed - r.slice_gaps_seen)}"


def test_the_gaps_table_says_which_card_tables_show_each_gap():
    for col, (vintages, why) in ccr.KNOWN_SLICE_GAPS.items():
        assert vintages in ("2018", "15.535", "both"), col
        assert len(why) > 40, f"{col}: an entry has to say what the engine does instead"


def test_register_families_called_read_name_what_is_still_unread():
    """Calling a whole family read is the coarsest thing this gate does, so each
    entry has to name the columns the engine reads AND the ones inside that family
    it still does not."""
    for fam, why in ccr.REGISTER_FAMILIES_READ.items():
        assert "unread" in why.lower(), f"{fam}: an entry has to say what is still unread inside the family"
        assert len(why) > 120, fam


def test_provenance_entries_each_carry_a_reason():
    for key, why in ccr.PROVENANCE.items():
        assert len(why) > 20, f"{key}: an exemption with no reason is a hole"


# --- what a thin checkout gets ---------------------------------------------------


def test_it_runs_with_no_extension_module_and_says_so(cards, monkeypatch):
    """No built `royalesim`: the engine's own catalogue is out of reach, so the gate
    scores every card in the file and NAMES the fallback. It must not go quiet."""
    monkeypatch.setitem(sys.modules, "royalesim", None)  # an import of None raises
    r = ccr.run(cards)
    assert any("SKIPPED the engine's own catalogue" in n for n in r.notes)
    assert r.failures == [], "\n".join(r.failures)


def test_it_runs_with_no_mechanic_register_and_says_so(cards, monkeypatch, tmp_path):
    """The register is generated from the card-table bundle and gitignored, so a
    fresh clone has none. The gate must still score passes A to C and say out loud
    that D did not run."""
    monkeypatch.setattr(ccr, "REGISTER", tmp_path / "nothing.json")
    r = ccr.run(cards)
    assert any("SKIPPED the register pass" in n for n in r.notes)
    assert r.failures == [], "\n".join(r.failures)


def test_the_register_pass_never_fails(cards, monkeypatch):
    """Pass D matches FAMILY names, which is coarser than the rest of the gate: a
    family can hold both read and unread fields. It reports and never fails, so a
    coarse instrument cannot redden a tree on its own."""
    if not os.path.exists(os.path.join(ROOT, "data", "derived", "mechanic_register.json")):
        pytest.skip("mechanic_register.json is generated and absent")
    doc, consumed, colmap, register = ccr.load(cards)
    for entry in register["cards"].values():
        entry["families"] = dict(entry["families"], warp=["Warp"], tether=["Tether"])
    r = ccr.check(doc, consumed, colmap, only=[], register=register)
    assert r.failures == [], "a family the engine does not run must not fail the gate"
    assert any("warp" in (v.get("register_families") or []) for v in r.per_card.values())


# --- the command line ------------------------------------------------------------


def test_the_command_line_exits_zero_when_green(cards):
    out = subprocess.run(
        [sys.executable, os.path.join(ROOT, "tools", "check_card_reads.py"), "--cards", cards],
        capture_output=True,
        text=True,
        cwd=ROOT,
    )
    assert out.returncode == 0, out.stdout[-4000:] + out.stderr[-2000:]
    assert out.stdout.rstrip().endswith("GREEN")


def test_the_command_line_exits_one_under_a_plant(cards):
    out = subprocess.run(
        [sys.executable, os.path.join(ROOT, "tools", "check_card_reads.py"), "--cards", cards, "--all-plants"],
        capture_output=True,
        text=True,
        cwd=ROOT,
    )
    assert out.returncode == 0, "every plant should land: " + out.stdout[-4000:]
    assert "DID NOT LAND" not in out.stdout


# --- what the gate reports, as data ----------------------------------------------


def test_the_report_names_the_cards_the_mechanics_doc_calls_out(cards):
    """docs/mechanics.md lists mechanics the engine does not run. The gate has to
    find them on the cards that carry them, or it is looking in the wrong place."""
    r = ccr.run(cards)
    want = {
        "InfernoDragon": "VariableDamage2",
        "MegaKnight": "DashDamage",
        "Mortar": "MinimumRange",
        "ElectroGiant": "ReflectedAttackDamage",
        "GoldenKnight": "DashDamage",
        "Monk": "VariableDamage2",
    }
    missing = {}
    for card, column in want.items():
        entry = r.per_card.get(card)
        if entry is None or not any(k == column or k.endswith(column) for k in entry["keys"]):
            missing[card] = column
    seen = {k: v["keys"] for k, v in r.per_card.items()}
    assert not missing, f"the gate did not see these: {missing}; report keys: {seen}"


def test_the_thin_slice_report_is_small_and_the_catalogue_report_is_not(cards):
    """The shape the whole exercise is about: the slice is nearly covered and the
    catalogue is not, so a deck drawn from the catalogue is likely to hold a card
    the engine runs as a plainer card."""
    r = ccr.run(cards)
    with open(cards, encoding="utf-8") as fh:
        doc = json.load(fh)
    slice_names = set(doc["thin_slice"])
    flagged = set(r.per_card)
    assert len(flagged - slice_names) > 40, "the catalogue report is suspiciously short"
    assert flagged & slice_names, "the slice gaps are listed by name in the tool; they should still be reported"
