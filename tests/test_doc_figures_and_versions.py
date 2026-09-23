"""Two more vendored documentation gates: figure provenance, and version claims.

FIGURE PROVENANCE. A section that states a percentage has to say what produced it, IN THAT
SECTION. The rule is per-claim rather than per-page because a reader arriving from a link
lands on a heading and reads down; they do not scroll up past it to find a caveat. That is
the defect docs caught in my own SUPERSEDED line and in this repo's per-card table, stated
as a gate.

    Nine sections across five pages refused the first time it ran here, and every one was a
    real gap: the corpus was named in each page's opening paragraph and nowhere else. They
    now name the run beside the figure.

    IF IT EVER REFUSES A PAGE YOU BELIEVE, SUSPECT THE CHECKER. Its first version here took a
    narrow vocabulary -- "measured on", "from the run" -- and I reworded three correct
    sentences to fit it. They were equally true after, which is what makes it bad rather than
    harmless: the check learned nothing, the pages got no better, and a guard was quietly
    deciding how this repo writes. Docs widened it to the verbs people actually use and I put
    the sentences back. A check that edits prose into its own vocabulary is worse than one
    that misses, because a miss is visible and a rewrite is not.

    It cannot tell a measured percentage from shipped CARD DATA on its own, so a row or a
    fenced token may be marked DATA. `spell-spec.md` states crown-tower percentages that are
    table values, not results. The honest fix there was to date the RULING that decides which
    table is loaded -- NOT to invent a measurement date, which the checker's own docstring
    says would satisfy it perfectly.

VERSION MATCH. What a README promises against what the build enforces: requires-python,
rust-version and the maturin pin. It is anchored on the tool name, so a "1.16 to 1.38x"
speed ratio is not read as a version.

    A coupling to know about rather than be caught by: RoyaleLearn's README promises Rust
    1.80 and RoyaleLearn has no Cargo.toml. THIS repo's manifest is what enforces that
    promise, so raising `rust-version` here silently falsifies a sentence in another repo.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
PROVENANCE = Path(__file__).resolve().parent / "_figure_provenance.py"
VERSIONS = Path(__file__).resolve().parent / "_version_match.py"


def tracked_pages() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "*.md"], capture_output=True, text=True, cwd=REPO, timeout=120
    )
    return [p for p in out.stdout.split("\n") if p.strip()]


def test_every_figure_says_what_produced_it() -> None:
    pages = tracked_pages()
    assert len(pages) >= 12, f"only {len(pages)} tracked pages found, so this checks almost nothing"
    # PATHS ONLY, deliberately, though the checker now accepts a repo as its first argument
    # too. Passing the list makes the POPULATION this test's own, which is what the
    # non-vacuity assertion above is about; handing over a directory would mean asserting a
    # count over a set chosen by something else. (Passing both double-counts: 28 of 28.)
    done = subprocess.run(
        [sys.executable, str(PROVENANCE), *pages],
        capture_output=True, text=True, timeout=300, cwd=REPO,
    )
    assert done.returncode == 0, (
        "a section states a figure and does not say where it came from:\n"
        + (done.stdout + done.stderr)[-3000:]
        + "\n\nName the run, the build or the date BESIDE the figure. A caveat under the page's "
        "first heading is not read by someone who arrived at this one from a link."
    )


def test_the_readme_promises_the_versions_the_build_enforces() -> None:
    done = subprocess.run(
        [sys.executable, str(VERSIONS), str(REPO)],
        capture_output=True, text=True, timeout=300, cwd=REPO,
    )
    assert done.returncode == 0, (done.stdout + done.stderr)[-3000:]


def test_both_checkers_own_self_tests_pass() -> None:
    """Run them, do not assume them: a guard whose self-test is never run is a guard nobody
    has seen work.

    ALL of them behaved, and at least ten exist -- not a pinned count. The first version
    pinned 14 and 12, and both moved within the hour when the checkers gained cases, which
    is a number going stale for the best possible reason. A floor catches a self-test that
    has quietly emptied; a pin catches an upstream improvement and calls it a failure.
    """
    for checker in (PROVENANCE, VERSIONS):
        done = subprocess.run(
            [sys.executable, str(checker), "--selftest"],
            capture_output=True, text=True, timeout=300, cwd=REPO,
        )
        assert done.returncode == 0, (done.stdout + done.stderr)[-2000:]
        m = re.search(r"(\d+) of (\d+) self-tests behaved", done.stdout)
        assert m, f"{checker.name} printed no self-test tally: {done.stdout[-600:]}"
        behaved, total = int(m.group(1)), int(m.group(2))
        assert behaved == total, f"{checker.name}: {behaved} of {total}"
        assert total >= 10, f"{checker.name}: only {total} self-tests, so it is barely checked"


def test_the_checkers_are_vendored_copies_and_not_local_rewrites() -> None:
    for checker in (PROVENANCE, VERSIONS):
        text = checker.read_text(encoding="utf-8")
        assert "def selftest(" in text, f"{checker.name} has lost its self-test"
        assert "__main__" in text, f"{checker.name} is no longer runnable on its own"
        imports = {
            line.split()[1].split(".")[0]
            for line in text.splitlines()
            if line.startswith("import ") or line.startswith("from ")
        }
        stdlib_only = {"re", "sys", "subprocess", "pathlib", "tomllib", "unicodedata", "__future__"}
        assert imports <= stdlib_only, (
            f"{checker.name} imports {sorted(imports - stdlib_only)}, so it no longer runs on a "
            "fresh clone with nothing installed"
        )
