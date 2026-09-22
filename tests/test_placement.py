"""The building placement surface the env layer and the viewer read.

A building is judged by its TILE FOOTPRINT, not by the point it was tapped at, and
a tap whose footprint does not fit is MOVED rather than refused (see
data/calibration.json placement.*). Three things carry that across the boundary:

  * ``Battle.building_placement`` says where a tap would land and what box it takes;
  * every building and crown tower row of ``state_json`` carries that box;
  * every building row of ``catalogue_json`` carries its footprint in tiles.

These tests are what a consumer can rely on. They would all pass on an engine that
judged the tap point, EXCEPT ``test_a_cannon_at_the_wall_does_not_stay_there``,
which is the reported defect.
"""

import json

import pytest

royalesim = pytest.importorskip("royalesim")

TILE = 18_000
DECK = ["Cannon", "Tesla", "Knight", "Archers", "Fireball", "Musketeer", "MiniPekka", "Skeletons"]


def tile_centre(x, y):
    return (x * TILE + TILE // 2, y * TILE + TILE // 2)


@pytest.fixture
def battle():
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    ids = list(range(len(DECK)))
    b.reset(0, [ids, ids], 0, 0, [10000, 10000], None, [])
    return b


def test_catalogue_carries_the_footprint_in_tiles(battle):
    rows = json.loads(battle.catalogue_json())
    by_name = {r[0]: r for r in rows}
    assert len(rows[0]) == 8, "a catalogue row carries the footprint as its 8th element"
    assert by_name["Cannon"][7] == 3, "a Cannon is 3x3 tiles"
    assert by_name["Tesla"][7] == 2, "a Tesla is 2x2 tiles"
    assert by_name["Knight"][7] is None, "a troop has no placement footprint"
    assert by_name["Fireball"][7] is None, "nor does a spell"


def test_state_rows_carry_the_footprint_box(battle):
    st = json.loads(battle.state_json())
    rows = st["entities"]
    assert rows, "the towers are on the board"
    for r in rows:
        assert len(r) == 15, "an entity row carries the footprint as its 15th element"
    boxes = [r[14] for r in rows if r[14] is not None]
    assert len(boxes) == 6, "six crown towers, each with a box"
    for b in boxes:
        assert len(b) == 4
        w, h = b[2] - b[0], b[3] - b[1]
        assert w == h, f"a tower's box is square: {b}"
        assert w in (3 * TILE, 4 * TILE), f"a princess is 3x3 and a king 4x4: {b}"
    kings = [b for b in boxes if b[2] - b[0] == 4 * TILE]
    assert len(kings) == 2, "exactly the two kings are 4x4"


def test_building_placement_reports_where_a_legal_tap_lands(battle):
    x, y = tile_centre(6, 8)
    got = battle.building_placement(0, "Cannon", x + 1234, y - 2345)
    assert got is not None
    cx, cy, box = got
    assert (cx, cy) == (x, y), "the tap snaps to the tapped tile's centre"
    assert box == [x - TILE - TILE // 2, y - TILE - TILE // 2, x + TILE + TILE // 2, y + TILE + TILE // 2]


def test_an_even_sized_building_takes_a_tile_corner(battle):
    x, y = tile_centre(6, 8)
    cx, cy, _ = battle.building_placement(0, "Tesla", x, y)
    assert (cx, cy) == (6 * TILE, 8 * TILE), "a 2x2 goes to the tapped tile's corner, not its centre"


def test_a_cannon_at_the_wall_does_not_stay_there(battle):
    """THE REPORTED DEFECT. The tap is a legal point 0.05 tiles from the back wall;
    a 3x3 Cannon cannot stand there, so it must be moved."""
    x = 9 * TILE + TILE // 2
    tap_y = TILE // 20
    cx, cy, box = battle.building_placement(0, "Cannon", x, tap_y)
    assert cy != tap_y, "the Cannon stayed on the tap"
    assert box[1] >= 0, f"its box left the arena: {box}"
    assert cy == TILE + TILE // 2, "it moves exactly far enough to fit"
    assert cx == x, "and does not drift sideways"


def test_a_troop_tap_is_unaffected(battle):
    """Troops are not judged by a box, and a troop card has no placement to report."""
    assert battle.building_placement(0, "Knight", *tile_centre(6, 8)) is None
    assert battle.check_deploy(0, DECK.index("Knight"), *tile_centre(6, 8)) == 0


def test_the_two_seats_agree_on_rotated_taps(battle):
    w, h = 18 * TILE, 32 * TILE
    for card in ("Cannon", "Tesla"):
        for tile in [(9, 0), (0, 8), (9, 14), (6, 8), (3, 6)]:
            x, y = tile_centre(*tile)
            blue = battle.building_placement(0, card, x, y)
            red = battle.building_placement(1, card, w - x, h - y)
            assert (blue is None) == (red is None), f"{card} at {tile}: one seat placed and the other did not"
            if blue is None:
                continue
            assert (w - blue[0], h - blue[1]) == (red[0], red[1]), f"{card} at {tile} is not seat-symmetric"
