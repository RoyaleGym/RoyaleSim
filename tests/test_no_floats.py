"""The engine is integer-only, and until now that was a `grep` written on a page.

WHY IT MATTERS HERE
    Determinism is the product. Two machines must play the same battle tick for tick, and
    floating point is where that stops being true: the same expression can round differently
    under a different compiler, target or optimisation level, and the divergence appears
    hundreds of ticks later as a unit standing somewhere else. Every quantity in this crate
    is a fixed-point integer for that reason, and the fixtures that pin parity against
    recordings are only meaningful while it stays true.

WHY IT IS A TEST NOW
    `docs/contributing.md` carried `grep -rn 'f32\\|f64' src/ tests/` in its gate list with
    "must print nothing" beside it. That is an instruction, not a gate: nobody runs a page,
    and the one contributor who does runs it on the shell the page was written for. The rule
    was unenforced the whole time it looked enforced.

WHAT IS SCANNED
    `src/`, `tests/`, `examples/` and `build.rs` of the crate. Examples are shipped code and
    a float that reaches one is as much a determinism hole as a float in `src/`.

    A mention inside a comment counts. That is deliberate and it is the cheap rule: a scan
    that parses Rust to tell code from comment is a scan that can be wrong, and the cost of
    the strict version is that a comment must say "floating point" rather than naming the
    type. Every comment in the crate already does.
"""

from __future__ import annotations

import re
from pathlib import Path

import pytest

CRATE = Path(__file__).resolve().parents[1] / "crates" / "royalesim"

#: Word-bounded, so `f32` matches and `hpf32x` does not.
FLOAT_TYPE = re.compile(r"\bf(?:32|64)\b")


def rust_sources() -> list[Path]:
    found: list[Path] = []
    for sub in ("src", "tests", "examples"):
        found += sorted((CRATE / sub).rglob("*.rs"))
    build = CRATE / "build.rs"
    if build.exists():
        found.append(build)
    return found


SOURCES = rust_sources()


def test_the_scan_actually_found_the_crate():
    """A path typo would make every assertion below pass over an empty list."""
    assert len(SOURCES) >= 20, [p.name for p in SOURCES]
    names = {p.name for p in SOURCES}
    assert "state.rs" in names, sorted(names)
    assert "build.rs" in names, sorted(names)


@pytest.mark.parametrize("path", SOURCES, ids=lambda p: p.name)
def test_no_floating_point_anywhere_in_the_engine(path: Path):
    text = path.read_text(encoding="utf-8")
    hits = [
        f"{path.name}:{i}: {line.strip()[:90]}"
        for i, line in enumerate(text.split("\n"), 1)
        if FLOAT_TYPE.search(line)
    ]
    assert not hits, (
        "the engine is integer-only because determinism is the product, and these lines name a "
        "floating-point type:\n  " + "\n  ".join(hits)
    )


def test_the_scan_would_notice_one():
    """The plant, in process and mechanical: text with a float must be caught, and text
    without one must not be, or the assertion above is a pass over an empty search."""
    planted = "let x: f32 = 1.0;\nlet y = 2;\n"
    assert [ln for ln in planted.split("\n") if FLOAT_TYPE.search(ln)], "a planted f32 went unseen"
    assert FLOAT_TYPE.search("fn f(x: f64) -> f64"), "a float in a signature went unseen"
    clean = "let x: i32 = 1;\nlet r = self.f32x;\nlet s = tf32(1);\n"
    assert not [ln for ln in clean.split("\n") if FLOAT_TYPE.search(ln)], (
        "the scan fires on integer code, which is how a real guard gets switched off"
    )
