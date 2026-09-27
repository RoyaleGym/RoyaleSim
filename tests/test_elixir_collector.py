"""The elixir economy's data and its payout through the protocol (tools/extract_cards.py `norm_unit` `mana`,
`summon_card` / `spell_card` `omit_from_starting_hand`; card.rs `ManaDef`; state.rs `mana_pass`; economy.*).

WHAT THIS PINS, 1: THE EXPORT. The four Mana columns of the 15.535.29 tables reach cards.json as a `mana` block on
exactly the rows that set one (the Elixir Collector, the three Elixir Golem generations and three event rows), and
on the card rows built from them; OmitFromStartingHand reaches exactly the Elixir Collector and Mirror. The 2018
file carries neither, so its Elixir Collector stays refused (no `mana` block, no HitSpeed).

WHAT THIS PINS, 2: THE PAYOUT, through the protocol (it needs an extension built from this tree). A Collector set
down already deployed pays its owner one elixir 259 ticks later (its first tick's step counts), measured on client
16.402 as D + 259 from the deploy end; the opponent gains nothing; under
economy.PRODUCTION_RATE_IN_DOUBLE_ELIXIR = scaled_with_elixir_rate the first payout in double elixir comes after
129 ticks, and under the shipped fixed_interval after 259, which is what makes the key's two arms tell apart.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
CARDS = ROOT / "data" / "derived" / "cards.json"
CARDS_2018 = ROOT / "data" / "derived" / "cards-2018.json"

MANA_ROWS = {
    "ElixirCollector": (1, 13000, 1, None),
    "ElixirGolem1": (None, None, None, 1000),
    "ElixirGolem2": (None, None, None, 500),
    "ElixirGolem4": (None, None, None, 500),
    "ElixirCollectorNeutral": (None, None, 2, None),
    "BoostObject_TrickOrTreat_Elixir": (None, None, 4, None),
    "ElixirBarrel": (1, 3000, None, 4000),
}


def load(p: Path) -> dict:
    if not p.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {p.relative_to(ROOT)} is absent")
    return json.loads(p.read_text(encoding="utf-8"))


def as_tuple(m: dict) -> tuple:
    return (m["collect_amount"], m["generate_time_ms"], m["on_death"], m["on_death_for_opponent"])


def test_the_mana_block_is_on_exactly_the_rows_that_set_a_mana_column():
    doc = load(CARDS)
    got = {n: as_tuple(u["mana"]) for n, u in doc["units"].items() if "mana" in u}
    assert got == MANA_ROWS
    cards = {c["name"]: as_tuple(c["mana"]) for c in doc["cards"] if "mana" in c}
    assert cards == {"Elixir Collector": MANA_ROWS["ElixirCollector"], "ElixirGolem": MANA_ROWS["ElixirGolem1"]}


def test_omit_from_starting_hand_is_on_the_collector_and_mirror_alone():
    doc = load(CARDS)
    got = sorted(c["name"] for c in doc["cards"] if c.get("omit_from_starting_hand"))
    assert got == ["Elixir Collector", "Mirror"]


def test_the_2018_file_carries_neither():
    doc = load(CARDS_2018)
    assert not any("mana" in u for u in doc["units"].values())
    assert not any("mana" in c or "omit_from_starting_hand" in c for c in doc["cards"])


# ---- 2. the payout, through the protocol

DECK = ["Elixir Collector", "Knight", "Archer", "Giant", "Musketeer", "Fireball", "Zap", "Valkyrie"]
IDS = list(range(len(DECK)))
#: a blue building mid-way down its own half, native units
AT = (9000, 8000)


@pytest.fixture(scope="module")
def royalesim():
    rs = pytest.importorskip("royalesim")
    try:
        rs.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    except Exception as e:
        pytest.fail(f"this build refuses the Elixir Collector ({e}): rebuild the extension from this tree")
    return rs


def first_payout(rs, overrides: dict, start_tick: int) -> tuple[int, int, int]:
    """(ticks to Blue's first gain over a battle without the Collector, the gain in thousandths, Red's gain then).
    Both battles start at `start_tick` with 0 elixir each, so no cap is reached in 300 ticks."""
    sub = rs.SUBTILE_PER_MILLITILE
    runs = []
    for spawns in ([(0, 0, AT[0] * sub, AT[1] * sub, None)], []):
        b = rs.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
        b.reset(0, [IDS, IDS], 0, start_tick, [0, 0], None, spawns)
        seq = []
        for _ in range(300):
            b.step([], 1)
            p = json.loads(b.state_json())["players"]
            seq.append((p[0]["elixir_milli"], p[1]["elixir_milli"]))
        runs.append(seq)
    for k, ((bw, rw), (bo, ro)) in enumerate(zip(*runs, strict=True)):
        if bw != bo:
            return k, bw - bo, rw - ro
    pytest.fail("the Collector paid nothing in 300 ticks")


def test_a_collector_pays_one_elixir_259_ticks_after_it_is_deployed(royalesim):
    k, gain, red = first_payout(royalesim, {}, 200)
    assert (k, gain, red) == (259, 1000, 0)


def test_the_double_elixir_arm_halves_the_interval(royalesim):
    scaled = {"economy.PRODUCTION_RATE_IN_DOUBLE_ELIXIR": json.dumps("scaled_with_elixir_rate")}
    # 2500 ticks is 125 s: inside the last 60 s of regulation, where the regen is double.
    assert first_payout(royalesim, {}, 2500)[0] == 259, "fixed_interval: the single-elixir cadence in double elixir"
    assert first_payout(royalesim, scaled, 2500)[0] == 129
