"""An area's BuffTime on client 16.402 (cards.CLIENT16402_VALUES, the AreaBuffTime column).

WHAT THIS PINS. On the 16.402 corpus a Freeze holds what it hits for 70 ticks, where the 15.535.29 tables' BuffTime
4000 holds it 80: the walking Knights of 20260920-005517 (cast on 703) and 20260920-010218 (cast on 312, both seats)
stand still through the cast + 70 and walk again (the engine: through + 80), and the Tombstone of 20260920-070448,
frozen on 3120 with a Skeleton due, releases it on 3191 (the engine: 3201). 3500 ms is 70 ticks. The column is not
listed in the shipped value.values: this key ships client16402, and each test lists it itself, as an override would.

THE CHECKS. A Blue Knight walks up the left lane and a Red Freeze lands on it. With Freeze AreaBuffTime 3500 listed it
stands still for 70 ticks; under the shipped list, and under the old arm, for 80. An Ice Golem's death area takes the
column too: listed at 2500, the slow it hangs on a Tombstone beside it starts at 2500 ms (the tables: 2000).

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run
this file: the named tests go red and the rest stay green.
  * `area_buff_time_unread` -- the overlay accepts the column and leaves the BuffTime:
    test_a_listed_freeze_holds_for_its_16402_buff_time, test_a_listed_death_area_hangs_its_16402_buff_time.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "cards.CLIENT16402_VALUES"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
FREEZE_16402, GOLEM_SLOW_16402 = 3500, 2500
KNIGHT_AT, FREEZE_AT, FREEZE_ON = (3500, 13000), (3500, 13500), 103


def overrides(arm, listed):
    """The key's compiled-in value with `arm` and the `listed` {card: {column: value}} added; None: no override."""
    if arm is None:
        return {}
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    value = ledger["cards"]["CLIENT16402_VALUES"]["value"]
    values = {card: dict(cols) for card, cols in value["values"].items()}
    for card, cols in listed.items():
        values.setdefault(card, {}).update(cols)
    return {KEY: json.dumps({**value, "arm": arm, "values": values})}


def freeze_hold(arm, listed, ticks=120):
    """The ticks from the Freeze on which the Knight stands where it stood when the Freeze landed."""
    b = royalesim.Battle(["Knight", "Freeze"], [[0, 1, 1], [0, 1, 1]], calibration_overrides=overrides(arm, listed))
    b.reset(2, [[0] * 8, [1] * 8], 0, 100, [10_000, 10_000], None, [(0, 0, KNIGHT_AT[0] * SUB, KNIGHT_AT[1] * SUB, -1)])
    rows = []
    for _ in range(ticks):
        tick = json.loads(b.state_json())["tick"]
        cmds = [(1, 0, FREEZE_AT[0] * SUB, FREEZE_AT[1] * SUB)] if tick == FREEZE_ON else []
        played = b.step(cmds, 1)
        if cmds:
            assert played, "the scene drifted: the Freeze play returned nothing"
            assert played[0][1] == 0, f"the scene drifted: the Freeze was refused: {played}"
        s = json.loads(b.state_json())
        k = [e for e in s["entities"] if e[F["tower_slot"]] < 0 and e[F["team"]] == 0]
        assert len(k) == 1, "the scene drifted: the Knight is gone"
        rows.append((s["tick"], (k[0][F["x"]], k[0][F["y"]]), k[0][F["stun_ticks"]]))
    held = [i for i, r in enumerate(rows) if r[2] > 0]
    assert held, "the scene drifted: the Freeze never held the Knight"
    start = held[0]
    still = 0
    for r in rows[start + 1 :]:
        if r[1] != rows[start][1]:
            break
        still += 1
    assert start + 1 + still < len(rows), "the scene drifted: the Knight never walked again"
    walking = [i for i in range(1, start) if rows[i][1] != rows[i - 1][1]]
    assert walking, "the scene drifted: the Knight was not walking before the Freeze"
    return still


def test_a_listed_freeze_holds_for_its_16402_buff_time():
    old = freeze_hold("client16402", {})
    new = freeze_hold("client16402", {"Freeze": {"AreaBuffTime": FREEZE_16402}})
    assert new == old - 10, (
        f"listed at {FREEZE_16402} ms the Freeze held the Knight {new} ticks, and {old} unlisted; the 500 ms less is "
        "10 ticks less"
    )


def test_the_shipped_list_and_the_old_arm_hold_for_the_tables_buff_time():
    shipped = freeze_hold(None, {})
    old_arm = freeze_hold("none", {"Freeze": {"AreaBuffTime": FREEZE_16402}})
    assert shipped == old_arm, f"the shipped list held the Knight {shipped} ticks and the old arm {old_arm}"
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    assert "Freeze" not in ledger["cards"]["CLIENT16402_VALUES"]["value"]["values"], "the shipped list names the Freeze"


def golem_slow_ms(arm, listed):
    """The ms left on the Ice Golem's death slow on the Tombstone beside it, on the tick it lands."""
    names = ["Tombstone", "IceGolemite"]
    b = royalesim.Battle(names, [[0, 1, 1], [0, 1, 1]], calibration_overrides=overrides(arm, listed))
    # a Red Tombstone, and a Blue Ice Golem of 1 hp beside it that the Tombstone's Skeletons kill
    spawns = [(1, 0, 13500 * SUB, 20500 * SUB, -1), (0, 1, 13500 * SUB, 19800 * SUB, 1)]
    b.reset(2, [[0] * 8, [0] * 8], 0, 100, [10_000, 10_000], None, spawns)
    for _ in range(80):
        b.step([], 1)
        s = json.loads(b.state_json())
        red = [e for e in s["entities"] if e[F["team"]] == 1 and e[F["tower_slot"]] < 0]
        stones = [e for e in red if e[F["radius"]] >= 1000 * SUB]
        assert len(stones) == 1, "the scene drifted: the Tombstone is gone"
        slows = [ms for name, ms in stones[0][F["buffs"]] if name.startswith("IceWizardSlowDown")]
        if slows:
            return slows[0]
    raise AssertionError("the scene drifted: the Ice Golem's death never slowed the Tombstone")


def test_a_listed_death_area_hangs_its_16402_buff_time():
    unlisted = golem_slow_ms("client16402", {})
    assert unlisted == 2000, f"the tables' death slow starts at {unlisted} ms, want 2000"
    listed = golem_slow_ms("client16402", {"IceGolemite": {"AreaBuffTime": GOLEM_SLOW_16402}})
    assert listed == GOLEM_SLOW_16402, f"listed at {GOLEM_SLOW_16402} ms the death slow starts at {listed} ms"
