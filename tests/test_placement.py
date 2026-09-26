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
    # BY THE PUBLISHED FIELD LIST, as the entity rows below: a catalogue row may grow at its end.
    fields = royalesim.CATALOGUE_FIELDS
    assert all(len(r) == len(fields) for r in rows), "a catalogue row is not as long as royalesim.CATALOGUE_FIELDS"
    fp = fields.index("footprint_tiles")
    assert by_name["Cannon"][fp] == 3, "a Cannon is 3x3 tiles"
    assert by_name["Tesla"][fp] == 2, "a Tesla is 2x2 tiles"
    assert by_name["Knight"][fp] is None, "a troop has no placement footprint"
    assert by_name["Fireball"][fp] is None, "nor does a spell"


def test_state_rows_carry_the_footprint_box(battle):
    st = json.loads(battle.state_json())
    rows = st["entities"]
    assert rows, "the towers are on the board"
    # BY THE PUBLISHED FIELD LIST, not a literal. This read `len(r) == 15` and `r[14]`, a second
    # copy of the row's shape that nothing tied to the first, and it failed the day five fields
    # were appended -- correctly, but only because the literal happened to be checked.
    # `royalesim.ENTITY_FIELDS` is what decoders refuse a mismatch against.
    fields = royalesim.ENTITY_FIELDS
    fp = fields.index("footprint")
    for r in rows:
        assert len(r) == len(fields), f"an entity row is not as long as royalesim.ENTITY_FIELDS: {r}"
    boxes = [r[fp] for r in rows if r[fp] is not None]
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
    """THE REPORTED DEFECT: a legal tap POINT whose 3x3 box is not legal.

    The river bank is the clean case. A tap on tile (9, 14) is dry ground, but a
    3x3 box centred there would cross into the water rows, and nothing else is
    near enough to decide where it goes instead, so it steps back exactly one row.
    """
    x, tap_y = tile_centre(9, 14)
    cx, cy, box = battle.building_placement(0, "Cannon", x, tap_y)
    assert (cx, cy) == tile_centre(9, 13), "it steps back one row, off the water"
    assert box[3] <= 15 * TILE, f"its box still crosses the river: {box}"

    # The side walls are the same defect and move further, because a tower box can
    # be what decides the landing. Here only "it moved, and it is legal" is pinned.
    for tap in [(TILE // 20, 8 * TILE + TILE // 2), (17 * TILE + 19 * TILE // 20, 8 * TILE + TILE // 2)]:
        cx, cy, box = battle.building_placement(0, "Cannon", *tap)
        assert (cx, cy) != tap, f"the Cannon stayed on the tap at {tap}"
        assert box[0] >= 0, f"its box left the arena on the left: {box}"
        assert box[2] <= 18 * TILE, f"its box left the arena on the right: {box}"


def test_a_troop_tap_is_unaffected(battle):
    """Troops are not judged by a box, and a troop card has no placement to report."""
    assert battle.building_placement(0, "Knight", *tile_centre(6, 8)) is None
    # PAST THE OPENING LOCKOUT FIRST (match.DEPLOY_LOCKOUT_TICKS). This test is about
    # WHERE a tap lands, and from 2026-09-23 a tap inside the first 90 ticks is refused
    # for WHEN it is -- so at tick 0 the assertion below stopped isolating its subject
    # and started reporting a second rule. Stepping is the fix; loosening the expected
    # code to "0 or 13" would have made it pass while testing neither.
    battle.step([], 90)
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
