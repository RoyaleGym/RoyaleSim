"""The Lightning's strikes on client 16.402 (cards.CLIENT16402_VALUES: the Lightning's AreaHitSpeed 500), and when a
striking area ends (spells.STRIKE_AREA_END).

NOT LISTED. cards.CLIENT16402_VALUES ships client16402, and its value.values do not carry the Lightning's AreaHitSpeed:
the 500 rests on one cast and is not scored, so the shipped Lightning is the tables' 460 row. Every override below adds
the 500 to the ledger's values itself (`arms`).

WHAT THIS PINS. The 16.402 corpus has one Lightning cast that strikes (20260920-070448, seen from both seats). Cast on
the elixir drop D = tick 2982 at (14500, 21500), it makes its strike objects on D + 10 (the red right princess tower,
3052 hp) and D + 20 (a Knight, 1766 hp), and each loss lands a tick later: 265 on D + 11 and 1057 on D + 21 (level
11). The 15.535.29 tables' HitSpeed 460 gives D + 9 and D + 18 on the engine's clock (spells.STRIKE_TIMER_LEFTOVER =
carried, which fits 67 of 67 client 15.535.29 strike objects); the client 16.402 row's 500 gives D + 10 and D + 20 on
the same clock. The step that carries the play is k = 0, so D + n is k = n.

THE THIRD STRIKE, inferred. The 500 row schedules strikes at 500, 1000 and 1500 ms; the third is due exactly at the
1500 ms LifeDuration, on D + 30. The corpus cast had no enemy in reach then. Under spells.STRIKE_AREA_END =
with_last_strike the area ends with its last scheduled strike and makes it; under the shipped at_life_end it ends on
D + 29 and loses it. With the tables' 460 the third strike (D + 27) comes first, and the two arms run the same battle.
The three-target scene: red Knights of 1200, 1400 and 1300 hp at (8000, 22000), (9000, 22000) and (10000, 22000),
a Blue Lightning at (9000, 22000); struck highest hp first.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then
run this file: the named tests go red and the rest stay green.
  * `card_values_unread` -- client16402 runs the tables' values: test_client16402_strikes_on_d_plus_11_and_21,
    test_client16402_with_last_strike_three_knights_on_d_plus_11_21_31 and
    test_client16402_at_life_end_loses_the_third_strike.
  * `strike_timer_restarts` -- the leftover dropped: every strike test here.
  * `strike_area_ends_at_life` -- the life ends the area whatever the key:
    test_client16402_with_last_strike_three_knights_on_d_plus_11_21_31.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "cards.CLIENT16402_VALUES"
END = "spells.STRIKE_AREA_END"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: explicit, because catalogue ids are positional; the Lightning first, so it is in the opening hand at slot 0
DECK = ["Lightning", "Knight", "Giant", "Musketeer", "Archer", "Bats", "Minions", "Goblins"]
IDS = list(range(len(DECK)))
LIGHTNING, KNIGHT = 0, 1
#: the corpus cast: its tap, the red right princess tower, the Knight about where it stood on D
TAP = (14500, 21500)
TOWER_AT = (14500, 25500)
KNIGHT_AT, KNIGHT_HP = (14735, 20500), 1766
#: the Lightning's AreaHitSpeed on client 16.402 (the 15.535.29 tables: 460); not in the shipped value.values
AREA_HIT_SPEED_16402 = 500
#: the corpus's losses at level 11: one strike, and a princess tower's share of it
STRIKE, TOWER_SHARE = 1057, 265
TICKS = 40
#: the three-target scene: the tap, and each red Knight's (position, hp)
THREE_TAP = (9000, 22000)
THREE = [((8000, 22000), 1200), ((9000, 22000), 1400), ((10000, 22000), 1300)]
THREE_TICKS = 45
#: what each end gives the three-target scene under client16402 (k, the struck Knight's starting hp, hp lost)
WITH_LAST_STRIKE = [(11, 1400, STRIKE), (21, 1300, STRIKE), (31, 1200, STRIKE)]
AT_LIFE_END = [(11, 1400, STRIKE), (21, 1300, STRIKE)]
#: ... and under none, either end
TABLES_460 = [(10, 1400, STRIKE), (19, 1300, STRIKE), (28, 1200, STRIKE)]


def ledger() -> dict:
    return json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)


def ledger_value() -> dict:
    value = ledger().get("cards", {}).get("CLIENT16402_VALUES", {}).get("value")
    if value is None:
        pytest.fail(f"{KEY} is not in the compiled-in ledger: this build predates the key")
    return value


def arms(values: str, end: str | None = None) -> dict:
    """Overrides for cards.CLIENT16402_VALUES = `values`, with the Lightning's AreaHitSpeed 500 added to the ledger's
    values (and its table kept), and, when given, spells.STRIKE_AREA_END = `end`."""
    value = ledger_value()
    listed = {**value["values"], "Lightning": {"AreaHitSpeed": AREA_HIT_SPEED_16402}}
    out = {KEY: json.dumps({**value, "arm": values, "values": listed})}
    if end is not None:
        if "STRIKE_AREA_END" not in ledger().get("spells", {}):
            pytest.fail(f"{END} is not in the compiled-in ledger: this build predates the key")
        out[END] = json.dumps(end)
    return out


def battle(overrides: dict, spawns: list) -> object:
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(0, [IDS, IDS], 0, 200, [100_000, 100_000], None,
            [(1, KNIGHT, x * SUB, y * SUB, hp) for (x, y), hp in spawns])
    return b


def red(b) -> list:
    return [e for e in json.loads(b.state_json())["entities"] if e[F["team"]] == 1]


def losses(b, label, tap, ticks, cast_on=0) -> list:
    """Every (k, label, hp lost) over `ticks` steps, the Lightning played at `tap` on step `cast_on`; `label` names the
    watched red entities by uid."""
    before = {label[e[F["uid"]]]: e[F["hp"]] for e in red(b) if e[F["uid"]] in label}
    got = []
    for k in range(ticks):
        res = b.step([(0, LIGHTNING, tap[0] * SUB, tap[1] * SUB)] if k == cast_on else [], 1)
        if k == cast_on:
            assert [r[1] for r in res] == [0], f"the cast was refused: {res}"
        now = {label[e[F["uid"]]]: e[F["hp"]] for e in red(b) if e[F["uid"]] in label}
        for who, h in before.items():
            lost = h - now.get(who, 0)
            if lost > 0:
                got.append((k, who, lost))
        before = now
    return got


def cast(overrides: dict) -> list:
    """The corpus cast: every (k, 'tower' or 'knight', hp lost) over TICKS steps."""
    b = battle(overrides, [(KNIGHT_AT, KNIGHT_HP)])
    label = {}
    for e in red(b):
        if e[F["tower_slot"]] < 0:
            label[e[F["uid"]]] = "knight"
        elif (e[F["x"]], e[F["y"]]) == (TOWER_AT[0] * SUB, TOWER_AT[1] * SUB):
            label[e[F["uid"]]] = "tower"
    hp = {who: next(e[F["hp"]] for e in red(b) if label.get(e[F["uid"]]) == who) for who in ("tower", "knight")}
    assert set(label.values()) == {"tower", "knight"}, f"the scene drifted: {label}"
    assert hp["tower"] > hp["knight"] == KNIGHT_HP, f"the scene drifted: the tower must outrank the Knight: {hp}"
    return losses(b, label, TAP, TICKS)


def three(overrides: dict, cast_on: int = 0) -> list:
    """The three-target scene: every (k, the struck Knight's starting hp, hp lost) over THREE_TICKS steps."""
    b = battle(overrides, THREE)
    label = {e[F["uid"]]: e[F["hp"]] for e in red(b) if e[F["tower_slot"]] < 0}
    assert sorted(label.values()) == sorted(hp for _, hp in THREE), f"the scene drifted: {label}"
    return losses(b, label, THREE_TAP, THREE_TICKS, cast_on)


def assert_three_strikes(got: list, want: list, what: str) -> None:
    """The three-target scene's assertion, shared with the instrument controls (parity's mirror and proxy)."""
    assert got == want, f"{what}: {got}; expected {want}"


def test_client16402_strikes_on_d_plus_11_and_21():
    got = cast(arms("client16402"))
    assert got == [(11, "tower", TOWER_SHARE), (21, "knight", STRIKE)], (
        f"client16402: {got}; client 16.402 strikes the tower on D + 10 and the Knight on D + 20, so the losses land "
        f"on D + 11 and D + 21")


def test_none_strikes_on_the_tables_d_plus_10_and_19():
    got = cast(arms("none"))
    assert got == [(10, "tower", TOWER_SHARE), (19, "knight", STRIKE)], (
        f"none: {got}; the 15.535.29 tables' HitSpeed 460 strikes on D + 9 and D + 18, so the losses land on D + 10 "
        f"and D + 19")


def test_client16402_with_last_strike_three_knights_on_d_plus_11_21_31():
    assert_three_strikes(three(arms("client16402", "with_last_strike")), WITH_LAST_STRIKE,
                         "client16402, with_last_strike: the 500 row's third strike, due at the 1500 ms LifeDuration, "
                         "falls on D + 30")


def test_client16402_at_life_end_loses_the_third_strike():
    assert_three_strikes(three(arms("client16402", "at_life_end")), AT_LIFE_END,
                         "client16402, at_life_end: the area ends on D + 29, before the third strike")


def test_the_tables_460_strike_three_times_under_either_end():
    for end in ("at_life_end", "with_last_strike"):
        assert_three_strikes(three(arms("none", end)), TABLES_460, f"none, {end}: D + 9, D + 18 and D + 27")


def test_the_shipped_engine_runs_the_tables_lightning_and_at_life_end():
    value = ledger_value()
    timings = {card: cols for card, cols in value["values"].items() if "AreaHitSpeed" in cols}
    assert not timings, f"{KEY} lists an AreaHitSpeed before it is scored: {timings}"
    end = ledger().get("spells", {}).get("STRIKE_AREA_END", {}).get("value")
    assert end == "at_life_end", f"{END} ships {end!r}, not the old arm 'at_life_end'"
    got = cast({})
    assert got == [(10, "tower", TOWER_SHARE), (19, "knight", STRIKE)], (
        f"the shipped engine: {got}; with the Lightning unlisted it strikes as the tables' 460 does, on D + 9 and "
        f"D + 18")
