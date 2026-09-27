"""The Ram Rider through the Python binding: its attached rider (card.rs `AttachDef`; calibration rider.*).

WHAT THIS PINS. The rider is a unit of its own in `state_json` and reports the Ram Rider's catalogue id, the id of
the card that put it on the board (py.rs `ids_of_indices`, which walks every block `CardDb::unit_refs` names, the
attached rider included); `rider_states` names it with the Ram it rides; and it stands, on every tick, where the Ram
stood on the tick before (rider.POSITION, measured on client 16.402 on one Ram Rider: 53 of 53 tick pairs).

PLANTS. Each is a cfg in the engine source. Prove one on a plant build of the module in a scratch venv
(`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run this file:
  * `rider_copies_post_move` -- the rider copies the Ram after the Ram moved:
    test_the_rider_stands_where_the_ram_stood_a_tick_before.
  * `rider_walks_itself` -- the rider is not carried and walks on its own: the same test.
The catalogue id has no plant here of its own: crates/royalesim/tests/unit_refs.rs holds the enumeration it reads
(plant `unit_refs_skips_new_paths`).
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CARDS = ["RamRider", "Knight", "Musketeer"]


def run(ticks):
    """A blue Ram Rider set up at (9500, 12500) native, walking to the red towers; one row per tick, the uid -> row
    map of the non-tower entities, and the `rider_states` of that tick."""
    try:
        b = royalesim.Battle(CARDS, [[0, 1, 2], [0, 1, 2]])
    except ValueError as e:
        pytest.fail(f"this royalesim build does not load the Ram Rider ({e}): rebuild the extension from this tree")
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, [(0, 0, 9500 * SUB, 12500 * SUB, -1)])
    rows = []
    for _ in range(ticks + 1):
        ents = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0}
        rows.append((ents, b.rider_states()))
        b.step([], 1)
    return rows


def test_the_rider_is_its_own_unit_under_the_ram_riders_card_id():
    rows = run(2)
    ents, riders = rows[0]
    assert len(ents) == 2, f"a Ram Rider puts two units on the board, the Ram and its rider: {list(ents.values())}"
    assert len(riders) == 1, f"rider_states: {riders}"
    rider, ram = riders[0]
    assert set(ents) == {rider, ram}, f"rider_states names {riders}, the board holds {sorted(ents)}"
    cid = CARDS.index("RamRider")
    assert ents[ram][F["card_id"]] == cid, "the Ram reports the Ram Rider's card id"
    assert ents[rider][F["card_id"]] == cid, "the rider reports the Ram Rider's card id"
    at = (ents[ram][F["x"]], ents[ram][F["y"]])
    assert (ents[rider][F["x"]], ents[rider][F["y"]]) == at, "the rider is born on the Ram"


def test_the_rider_stands_where_the_ram_stood_a_tick_before():
    rows = run(120)
    rider, ram = rows[0][1][0]
    moved = 0
    for t in range(1, len(rows)):
        now, before = rows[t][0], rows[t - 1][0]
        if ram not in now:
            break  # the towers killed the Ram, and the rider with it
        assert (now[rider][F["x"]], now[rider][F["y"]]) == (before[ram][F["x"]], before[ram][F["y"]]), f"tick {t}"
        moved += (now[ram][F["x"]], now[ram][F["y"]]) != (before[ram][F["x"]], before[ram][F["y"]])
    assert moved >= 60, f"vacuous: the Ram moved on {moved} ticks"
