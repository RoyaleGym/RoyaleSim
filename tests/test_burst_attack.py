"""The Dagger Duchess's `burst_attack` block (tools/extract_cards.py `burst_attack`).

WHAT THIS PINS. The extractor writes the block on the one row whose OnStartingAction is an inline ActionBurstAttack:
the DaggerDuchess unit, in the 15.535.29 table and both 160402017 tables (the packs carry the same row), and nothing
else. It carries the charges, their recharge, the attack sequence entry each charged attack takes and the one a depleted
tower takes, and each entry's HitSpeedMultiplier. The builder fails closed: a key it has never read, a value that is
not a whole number, or an index past the row's AttackSequenceList gives no block.

WHAT NEEDS WHAT. The committed data/derived/cards-*.json are read by every clone. Rebuilding the block from the client's
tables needs data/raw/cr-15.535.29/, which a clone does not have: that test skips loudly there.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parent.parent
DERIVED = ROOT / "data" / "derived"
TABLES = ["cards-15.535.json", "cards-160402017.json", "cards-160402017-20261006.json"]
RAW_15535 = ROOT / "data" / "raw" / "cr-15.535.29" / "csv_logic"

BLOCK = {
    "max_charges": 8,
    "recharge_ms": 900,
    "recharge_increment": 1,
    "sequence": [2, 0, 1, 0, 1, 0, 1, 0],
    "depleted_index": 3,
    "hit_speed_multipliers": [100, 100, 70, 90],
}

sys.path.insert(0, str(ROOT / "tools"))
import extract_cards as ec  # noqa: E402


@pytest.mark.parametrize("table", TABLES)
def test_the_committed_table_carries_the_duchess_burst_alone(table):
    with open(DERIVED / table, encoding="utf-8") as fh:
        doc = json.load(fh)
    carriers = {n: u["burst_attack"] for n, u in doc["units"].items() if "burst_attack" in u}
    assert carriers == {"DaggerDuchess": BLOCK}
    assert not [c["name"] for c in doc["cards"] if "burst_attack" in c], "a tower troop is no card"


def test_the_block_is_built_from_the_packs_row():
    if not RAW_15535.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {RAW_15535} is absent, so the block was not rebuilt from the row")
    t = ec.load_tables("15.535.29")
    assert ec.burst_attack(t, "buildings", "DaggerDuchess", t["buildings"].get("DaggerDuchess")) == BLOCK


def _row(**action):
    base = {"ClassType": "ActionBurstAttack", "MaxChargeCount": 8, "RechargeTime": 900, "RechargeIncrement": 1,
            "AttackSequenceIndices": [2, 0, 1, 0, 1, 0, 1, 0], "DepletedAttackSequenceIndex": 3,
            "IndicatorExportName": "dagger_dutches_charge_counter", "AIStateName": "DaggerDuchessBurstAttack"}
    rec = ec.Row({"OnStartingAction"}, {"OnStartingAction": {**base, **action}})
    seq = [{"Projectile": "P", "HitSpeedMultiplier": m} for m in (100, 100, 70, 90)]
    t = {"buildings": SimpleNamespace(arrays={"D": {"AttackSequenceList": seq}})}
    return t, rec


def test_the_builder_fails_closed():
    t, rec = _row()
    assert ec.burst_attack(t, "buildings", "D", rec) == BLOCK
    t, rec = _row(ChargeSpeed=2)
    assert ec.burst_attack(t, "buildings", "D", rec) is None, "a key the builder has never read"
    t, rec = _row(DepletedAttackSequenceIndex=4)
    assert ec.burst_attack(t, "buildings", "D", rec) is None, "an index past the sequence list"
    t, rec = _row(RechargeTime=900.5)
    assert ec.burst_attack(t, "buildings", "D", rec) is None, "a value that is not a whole number"
    t, rec = _row(ClassType="ActionGiantBufferCollectFriends")
    assert ec.burst_attack(t, "buildings", "D", rec) is None, "another action"
