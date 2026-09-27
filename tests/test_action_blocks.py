"""The Rune Giant's `enchant_friends` block (tools/extract_cards.py `enchant_friends`).

WHAT THIS PINS. The extractor writes the block on the one 15.535 row whose OnStartingAction is an
ActionGiantBufferCollectFriends that sends a homing, own-troops-only, damage-less projectile whose hit runs an
ActionGiantBufferBuff at delay 0 beside cosmetic actions: the GiantBuffer card and its unit, and nothing else. The
block carries the parameters the loader reads (card.rs `enchant_of`), the 13 multiplier pairs in table order, and the
rows the tables tag NO_GIANTBUFFER_CHEF_ENCHANTMENT (PhoenixEgg). The 2018 table carries no such block. The builder
fails closed: an action row with a key it has never read gives no block, so the card stays refused with today's
reason.

WHAT NEEDS WHAT. The committed data/derived/cards-15.535.json is read by every clone. Rebuilding the block from the
client's tables needs data/raw/cr-15.535.29/, which a clone does not have: those tests skip loudly there.

PLANT. `enchant_block_ignores_unknown_keys`, by monkeypatch: the builder's three key sets widened to accept a key it
has never read. test_the_plant_enchant_block_ignores_unknown_keys_lands shows that under it the refusal the other
test pins does not happen, so that test would go red.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
TABLE_15535 = ROOT / "data" / "derived" / "cards-15.535.json"
TABLE_2018 = ROOT / "data" / "derived" / "cards-2018.json"
RAW_15535 = ROOT / "data" / "raw" / "cr-15.535.29" / "csv_logic"

MULTIPLIERS = [
    ["ElectroWizard", 500],
    ["FirecrackerExplosion", 200],
    ["FirecrackerExplosion_EV1", 200],
    ["HunterProjectile", 100],
    ["Hunter_EV1_shotgun_projectile", 100],
    ["RamRiderBola", 0],
    ["SpearGoblinGiantProjectile", 0],
    ["GoblinMachineRocketProjectile", 0],
    ["PhoenixFireball", 0],
    ["GoblinDemolisherDeathProjectile", 0],
    ["BarbLogHeroProjectileReRolling", 0],
    ["EliteArcherHero_Ability_Triple_Shot_Projectile", 340],
    ["EliteArcherHero_Ability_Power_Shot_Projectile_Middle", 340],
]

BLOCK = {
    "collect": {
        "action": "giantbuffer_collect_friend_troops",
        "action_delay_ms": 1000,
        "cooldown_ms": 3000,
        "max_targets": 2,
        "pick_radius_milli": 7000,
        "buff_radius_milli": 8500,
        "buff_delay_ms": 280,
        "target_filter": "friendly_troops_for_rune_giant",
    },
    "projectile": {"name": "GiantBuffProjectile", "speed": 600},
    "enchant": {
        "action": "giantbuffer_enchanting_buff",
        "attack_amount": 3,
        "added_damage": 86,
        "added_crown_tower_damage": 86,
        "finish_if_instigator_dies_ms": 5000,
        "multipliers": MULTIPLIERS,
    },
    "excluded_units": ["PhoenixEgg"],
    "classes": ["ActionGiantBufferBuff", "ActionGiantBufferBuffVisual", "ActionGiantBufferCollectFriends"],
}


sys.path.insert(0, str(ROOT / "tools"))
import extract_cards as ec  # noqa: E402


def _skip_without(path: Path, what: str) -> None:
    if not path.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {path} is absent, so {what} was not checked")


@pytest.fixture(scope="module")
def table_15535() -> dict:
    _skip_without(TABLE_15535, "the committed 15.535 table's enchant block")
    return json.loads(TABLE_15535.read_text(encoding="utf-8"))


@pytest.fixture(scope="module")
def tables():
    _skip_without(RAW_15535, "the builder against the client's tables")
    return ec.load_tables(ec.DEFAULT_VINTAGE)


def test_exactly_the_rune_giant_carries_the_block(table_15535):
    cards = [c["name"] for c in table_15535["cards"] + table_15535["towers"] if "enchant_friends" in c]
    units = [n for n, u in table_15535["units"].items() if "enchant_friends" in u]
    assert cards == ["GiantBuffer"], cards
    assert units == ["GiantBuffer"], units
    card = next(c for c in table_15535["cards"] if c["name"] == "GiantBuffer")
    assert card["enchant_friends"] == BLOCK
    assert table_15535["units"]["GiantBuffer"]["enchant_friends"] == BLOCK


def test_no_2018_row_carries_it():
    _skip_without(TABLE_2018, "the 2018 table")
    assert "enchant_friends" not in TABLE_2018.read_text(encoding="utf-8")


def test_the_builder_reads_the_tables(tables):
    block = ec.enchant_friends(tables, tables["characters"].get("GiantBuffer"))
    assert block == BLOCK
    others = [
        n
        for k in ("characters", "buildings")
        for n, r in tables[k].records.items()
        if n != "GiantBuffer" and ec.enchant_friends(tables, r) is not None
    ]
    assert others == [], others


def _refuses_unknown_keys(tables, monkeypatch) -> dict[str, bool]:
    """For each of the three action rows, whether one key the builder has never read refuses the block."""
    acts = tables["actions"].records
    rows = {
        "collect": "giantbuffer_collect_friend_troops",
        "buff": "giantbuffer_enchanting_buff",
        "visual": "giantbuffer_target_adddamage_action",
    }
    out = {}
    for what, name in rows.items():
        with monkeypatch.context() as m:
            m.setitem(acts[name], "UnreadKey", 1)
            out[what] = ec.enchant_friends(tables, tables["characters"].get("GiantBuffer")) is None
    return out


def test_an_unknown_key_refuses_the_block(tables, monkeypatch):
    assert _refuses_unknown_keys(tables, monkeypatch) == {"collect": True, "buff": True, "visual": True}
    # And the block comes back once the key is gone.
    assert ec.enchant_friends(tables, tables["characters"].get("GiantBuffer")) == BLOCK


def test_the_plant_enchant_block_ignores_unknown_keys_lands(tables, monkeypatch):
    for name in ("ENCHANT_COLLECT_KEYS", "ENCHANT_BUFF_KEYS", "ENCHANT_VISUAL_KEYS"):
        monkeypatch.setattr(ec, name, getattr(ec, name) | {"UnreadKey"})
    assert _refuses_unknown_keys(tables, monkeypatch) == {"collect": False, "buff": False, "visual": False}


def test_a_16402_style_pause_refuses_the_block(tables, monkeypatch):
    """The 16.402 client's tables replace the inline OnBuffAction with a named group that sets NO_ATTACK and NO_MOVE:
    a non-cosmetic OnBuffAction gives no block, so no engine runs that client's Rune Giant without the pause."""
    row = tables["actions"].records["giantbuffer_collect_friend_troops"]
    monkeypatch.setitem(row, "OnBuffAction", "giantbuffer_enchanting_on_buff_friends")
    assert ec.enchant_friends(tables, tables["characters"].get("GiantBuffer")) is None
