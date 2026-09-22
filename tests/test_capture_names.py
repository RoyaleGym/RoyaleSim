"""tools/capture_names.py, and the discipline the fixture makers have to keep.

A seat LETTER is not a fact about a file, it is an index into a sort: a map computed over a
subset gives A to whichever seat sorts first in THAT subset. Four makers each computed their
own, over four different subsets, and a capture could come out "-A" in one fixture and "-B"
in another with nothing to catch it. `folder_seats` is the one map, computed over the whole
captures folder.

Two committed fixtures cannot be compared to each other for this: if the letters disagree,
the data does not say so -- "A" is "A" in both files. So the guard here is at the source: no
maker may build its own map. The rest is what a plain checkout CAN see in the fixtures --
letters, not seat numbers, and no gaps in the alphabet.
"""

from __future__ import annotations

import importlib.util
import os
import re
from collections import defaultdict

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures")
TOOLS = os.path.join(ROOT, "tools")
MAKERS = (
    "make_client16402_paths_fixture.py",
    "make_client16402_jump_fixture.py",
    "make_live_levels_fixture.py",
    "make_replay_fixture.py",
    "make_spell_impact_fixture.py",
)
# <8 digits>-<6 digits>, an optional batch tag, then the seat letter.
STAMPED = re.compile(r"\b(\d{8}-\d{6})(?:\.b\d+)?-([A-Z])\b")
NUMBERED = re.compile(r"\b\d{8}-\d{6}(?:\.b\d+)?-\d")


def _load():
    spec = importlib.util.spec_from_file_location(
        "capture_names", os.path.join(TOOLS, "capture_names.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


@pytest.fixture(scope="module")
def m():
    return _load()


def _json_fixtures() -> list[str]:
    out = []
    for dirpath, _dirs, files in os.walk(FIXTURES):
        out += [os.path.join(dirpath, f) for f in files if f.endswith(".json")]
    return out


def test_seat_tag_anchors_on_the_stamp(m):
    # The stamp's own second field is six digits preceded by a dash, so an unanchored pattern
    # rewrites the TIME as a seat. And a batch tag is not a seat.
    assert m.SEAT_TAG.sub("!", "20260918-115249.b1:44:Goblins") == "20260918-115249.b1:44:Goblins"
    assert m.SEAT_TAG.search("frames-auto-20260918-115249.b1") is None
    assert m.SEAT_TAG.sub("!", "20260920-002736-52") == "20260920-002736!"
    assert m.SEAT_TAG.search("20260920-002736-52:7:Giant").group(1) == "52"


def test_seat_letters_are_an_index_into_a_sort(m):
    # Why folder_seats exists, stated as a test: the same seat takes a different letter from
    # a different subset, and nothing downstream can tell. (The tags here are made up: a seat
    # tag is whatever digits follow the stamp, and only the sort over them matters.)
    both = m.seat_letters(["20260920-002736-31", "20260920-002736-52"])
    alone = m.seat_letters(["20260920-002736-52"])
    assert both["52"] == "B"
    assert alone["52"] == "A"


def test_every_maker_takes_its_seat_map_from_the_folder():
    # THE REGRESSION GUARD. A maker that calls seat_letters itself is building a map over
    # whatever subset it happens to hold, which is how the letters drifted apart before.
    offenders = []
    for name in MAKERS:
        with open(os.path.join(TOOLS, name), encoding="utf-8") as f:
            body = "\n".join(
                ln for ln in f.read().splitlines() if not ln.lstrip().startswith("#")
            )
        calls_own = re.search(r"(?<!def )\bseat_letters\(", body)
        folder_wide = "folder_seats(" in body or "the whole captures folder" in body
        if calls_own and not folder_wide:
            offenders.append(name)
    assert not offenders, (
        "these makers build their own seat map instead of taking the folder's "
        f"(tools/capture_names.py folder_seats): {offenders}"
    )


def test_no_committed_fixture_names_a_seat_by_its_number():
    bad = []
    for path in _json_fixtures():
        with open(path, encoding="utf-8") as f:
            if NUMBERED.search(f.read()):
                bad.append(os.path.relpath(path, FIXTURES))
    assert not bad, f"a capture is named by its seat number rather than its letter: {bad}"


def test_the_fixtures_use_a_dense_alphabet_from_a():
    # A single capture may be kept from one seat only, so a stamp alone can read "-B"; what
    # the CORPUS cannot have is a gap, which is what a per-subset map produces when two maps
    # disagree about how many seats there are.
    letters: dict[str, set[str]] = defaultdict(set)
    for path in _json_fixtures():
        with open(path, encoding="utf-8") as f:
            for stamp, letter in STAMPED.findall(f.read()):
                letters[stamp].add(letter)
    assert letters, "no fixture carries a stamped capture name: this test covers nothing"
    used = set().union(*letters.values())
    want = {chr(ord("A") + i) for i in range(len(used))}
    assert used == want, f"the fixtures use seat letters {sorted(used)}, not A, B, ... from A"


def test_distinct_captures_collapses_two_names_for_one_file(tmp_path, m):
    real = tmp_path / "frames-auto-20260920-083112-31.native.oracle.jsonl.gz"
    real.write_bytes(b"x")
    alias = tmp_path / "frames-auto-20260920-083112-A.native.oracle.jsonl.gz"
    try:
        os.link(real, alias)
    except OSError:  # pragma: no cover - a filesystem without hard links
        pytest.skip("no hard links here; distinct_captures keys on (st_dev, st_ino)")
    kept = m.distinct_captures([str(real), str(alias)])
    assert len(kept) == 1
    # The name that carries a SEAT wins, so the letter still comes from the seat map.
    assert kept[0].endswith("-31.native.oracle.jsonl.gz")


def test_argv_guard_refuses_a_mistyped_flag(m):
    m.argv_guard([], "doc")
    m.argv_guard(["--check"], "doc")
    with pytest.raises(SystemExit) as e:
        m.argv_guard(["--nonsense"], "doc")
    assert e.value.code == 2
