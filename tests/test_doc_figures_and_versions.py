"""Two more vendored documentation gates: figure provenance, and version claims.

FIGURE PROVENANCE. A section that states a percentage has to say what produced it, IN THAT
SECTION. The rule is per-claim rather than per-page because a reader arriving from a link
lands on a heading and reads down; they do not scroll up past it to find a caveat. That is
the defect docs caught in my own SUPERSEDED line and in this repo's per-card table, stated
as a gate.

    Nine sections across five pages refused the first time it ran here, and every one was a
    real gap: the corpus was named in each page's opening paragraph and nowhere else. They
    now name the run beside the figure.

    ONE THING WORTH KNOWING IF THIS EVER REFUSES A PAGE YOU BELIEVE: the checker accepts a
    fixed vocabulary -- a hex digest, an ISO date, phrases like "measured on" or "from the
    run", or a named corpus size. "Counted over the offline trace corpus of client
    15.535.29" is as informative to a reader and does not match. Prefer rewording to the
    accepted phrasing ONLY when the reworded sentence is equally true; if it is not, the
    checker is wrong and should be fixed rather than the page.

    It also cannot tell a measured percentage from shipped CARD DATA. `spell-spec.md` states
    crown-tower percentages that are table values, not results, and the honest fix there was
    to date the ruling that decides which table is loaded -- NOT to invent a measurement
    date. A provenance rule invites exactly that lie, which the checker's own docstring says
    it cannot see.

VERSION MATCH. What a README promises against what the build enforces: requires-python,
rust-version and the maturin pin. It is anchored on the tool name, so a "1.16 to 1.38x"
speed ratio is not read as a version.

    A coupling to know about rather than be caught by: RoyaleLearn's README promises Rust
    1.80 and RoyaleLearn has no Cargo.toml. THIS repo's manifest is what enforces that
    promise, so raising `rust-version` here silently falsifies a sentence in another repo.
"""

from __future__ import annotations

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
    # PATHS ONLY. The checker's usage line reads `<repo> <path>...`, but it treats every
    # argument as a file, so passing the repo directory first gives PermissionError on the
    # directory rather than a verdict -- which the caller would read as a refusal.
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
    """14 and 12 cases. Run them, do not assume them: a guard whose self-test is never run is
    a guard nobody has seen work."""
    for checker, expect in ((PROVENANCE, "14 of 14"), (VERSIONS, "12 of 12")):
        done = subprocess.run(
            [sys.executable, str(checker), "--selftest"],
            capture_output=True, text=True, timeout=300, cwd=REPO,
        )
        assert done.returncode == 0, (done.stdout + done.stderr)[-2000:]
        assert expect in done.stdout, f"{checker.name}: {done.stdout[-600:]}"


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
