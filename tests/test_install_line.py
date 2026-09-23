"""The README's install line and pyproject's `dev` extra name the same packages.

WHY THIS EXISTS. The first CI run of this repo's own documented install failed on a clean
runner with `ModuleNotFoundError: No module named 'numpy'` and the same for `msgspec`. Five
test modules could not IMPORT. Both are declared in pyproject's `dev` extra and neither
appeared anywhere in the README, so a reader following the published install got a repo whose
suite cannot load five of its own test files.

NOBODY HERE COULD HAVE SEEN IT. numpy is on every machine in this project's history. The
defect is not in anyone's care; it is that the documented install had never been executed
anywhere that did not already satisfy it.

WHY A GUARD RATHER THAN CITING ONE SOURCE. Citing is the better repair and the build order
forbids it. `pip install -e ".[dev]"` runs the maturin backend, and `arena.rs` does
`include_str!("../../../data/derived/arena.json")` on a file that stage 3 has not generated
when stage 2 runs -- so the build fails before the extras resolve. The README's list is
therefore a forced copy of pyproject's, and a forced copy needs something that fails when it
drifts. This repo has paid for a hand-kept list three times in one night: a plant list in an
argument parser, an `include_str!` scan that read one directory, and a stdlib allow-list that
called a correct upstream copy broken.

WHAT IT DOES NOT CHECK: whether either list is right, whether the versions agree, or whether
the suite needs all of them. It checks that the two say the same names, which is the thing
that silently stopped being true.
"""

from __future__ import annotations

import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"
PYPROJECT = ROOT / "pyproject.toml"

#: The install line in the README's stage 2, as a reader runs it.
INSTALL = re.compile(r"^\.venv\\Scripts\\python -m pip install ([^\n]+)$", re.M)


def readme_packages() -> set[str]:
    text = README.read_text(encoding="utf-8")
    lines = [m.group(1).split() for m in INSTALL.finditer(text)]
    # Only the bare-package line: the sibling repos are installed with `-e <path>` and are a
    # different statement.
    bare = [words for words in lines if not any(w == "-e" for w in words)]
    assert len(bare) == 1, f"expected exactly one bare pip install line in the README, found {len(bare)}"
    return set(bare[0])


def dev_extra() -> set[str]:
    with open(PYPROJECT, "rb") as fh:
        doc = tomllib.load(fh)
    spec = doc["project"]["optional-dependencies"]["dev"]
    # names only: "maturin>=1.15,<2.0" -> "maturin"
    return {re.split(r"[><=!\[;\s]", s, maxsplit=1)[0] for s in spec}


def test_the_readme_install_names_every_dev_dependency():
    readme, dev = readme_packages(), dev_extra()
    assert readme == dev, (
        "the README's install line and pyproject's `dev` extra have drifted.\n"
        f"  in pyproject and NOT in the README: {sorted(dev - readme)}\n"
        f"  in the README and NOT in pyproject: {sorted(readme - dev)}\n\n"
        "A reader runs the README. If a package is declared and not published there, their "
        "suite fails to import on a machine that does not already have it -- which is every "
        "machine except the ones this was written on."
    )


def test_both_lists_are_non_empty_and_plausible():
    """Green means nothing if either side parsed to an empty set."""
    readme, dev = readme_packages(), dev_extra()
    assert len(dev) >= 5, f"the dev extra parsed to {sorted(dev)}, which is not the dev extra"
    assert "pytest" in readme, f"the README install line parsed to {sorted(readme)}"


def test_the_comparison_would_notice_a_drift():
    """The plant, in process: the comparison must report a name on either side."""
    a, b = {"pytest", "ruff"}, {"pytest", "ruff", "numpy"}
    assert (b - a) == {"numpy"}, "a package declared and not published must be reported"
    assert (a - b) == set()
    assert a != b, "the comparison is an equality, so a one-sided difference fails it"
