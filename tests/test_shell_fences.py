"""Would a reader's shell actually run the commands on this repo's pages?

WHY THIS EXISTS
    On 2026-09-22 the published install block ran in NO shell a newcomer would use, and
    had not since the repos were split. Four readers hit it from four entry points. The
    shapes, each confirmed on a real machine rather than argued: ``&&`` is a parse error
    in Windows PowerShell 5.1 before anything executes; a trailing ``#`` comment is not a
    comment in ``cmd``, so ``python -m venv .venv  # Python 3.12`` silently creates four
    directories and no environment; and a Windows backslash path inside a ```bash fence
    loses its backslashes.

    The second is the dangerous one, because it does not fail. The reader gets no error
    and no working venv.

    This repo was the last of the four with no doc-checking test at all, and it had four
    pages refusing when the guard first ran here: the contributor page, the performance
    page, the parity page and the media page.

WHAT IT CHECKS, AND WHAT IT DELIBERATELY DOES NOT
    Only those shapes. It does not read the commands and cannot tell you the instructions
    are correct: the real check is a person running the recipe verbatim in PowerShell from
    an empty folder, which is how this was found. This is the guard that stops it coming
    back through ordinary editing.

    EVERY markdown page in the repo, not a hand-picked list. The list would be the thing
    that rots: a page added later, or a fence added to a page nobody listed, would be
    ungated and the suite would still report N of N. A page with a genuinely POSIX-only
    example says so with a ```bash tag, which is a sentence the reader needs anyway.

    The checker is ``tests/_shell_fences.py``, a vendored copy kept BYTE-IDENTICAL to the
    original so the four public repos' copies can be diffed against each other and against
    it. That is why ``pyproject.toml`` waives E501 for that one path: a copy reformatted to
    pass this repo's lint passes lint and makes the next drift invisible. Its own
    ``--selftest`` plants all four shapes plus three that must NOT fire, and that self-test
    is run here: a guard whose self-test is never run is a guard nobody has seen work.

A GUARD WITH FALSE POSITIVES IS ONE SOMEBODY SWITCHES OFF
    These pages pass today, so if this test goes red on a page a person has just verified
    by hand, suspect the guard before the page.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

from _shell_fences import check_text, selftest

REPO = Path(__file__).resolve().parents[1]
CHECKER = Path(__file__).resolve().parent / "_shell_fences.py"


def pages() -> list[Path]:
    """Every markdown page in the repo, found rather than listed.

    ``crates/*/target`` is not walked, so a build directory cannot slow this down or
    smuggle a page in.
    """
    found = sorted(REPO.glob("*.md")) + sorted((REPO / "docs").rglob("*.md"))
    vendored = REPO / "data" / "raw" / "retroroyale-2018" / "README.md"
    if vendored.exists():
        found.append(vendored)
    return found


PAGES = pages()
PAGE_IDS = [p.relative_to(REPO).as_posix() for p in PAGES]


def test_the_page_list_is_not_empty_and_holds_the_one_that_matters():
    """Guards the parametrisation from finding nothing and reporting green for it."""
    assert len(PAGES) >= 12, PAGE_IDS
    assert "README.md" in PAGE_IDS, PAGE_IDS


@pytest.mark.parametrize("page", PAGES, ids=PAGE_IDS)
def test_a_reader_could_paste_this_page_into_their_shell(page: Path) -> None:
    problems = check_text(page.read_text(encoding="utf-8"), page.relative_to(REPO).as_posix())
    assert not problems, (
        f"{page.relative_to(REPO).as_posix()} has {len(problems)} block(s) a reader's shell "
        "would not run:\n  "
        + "\n  ".join(problems)
        + "\n\nThese are shapes, not opinions: && is a parse error in Windows PowerShell, "
        "a trailing # is not a comment in cmd, and bash eats backslashes. If the page is "
        "right and this is wrong, fix the guard rather than silencing it."
    )


def test_the_guards_own_self_test_passes() -> None:
    """It plants all four shapes and three that must not fire. Run it, do not assume it."""
    # selftest() returns a COUNT and prints a line per case, so read the printed lines for
    # which case misbehaved. Compared against 0 rather than truthiness on purpose: a count
    # is not a list, and formatting it as one would raise here instead of reporting.
    assert selftest() == 0, (
        "the shell-fence guard's own self-test does not behave. Its printed lines say "
        "which planted shape went unrefused, or which safe page was refused; run "
        "`python tests/_shell_fences.py --selftest` to see them."
    )


def test_the_guard_runs_standalone_on_a_fresh_clone() -> None:
    """No pytest, no package, no dependencies: ``python tests/_shell_fences.py --selftest``.

    That is the whole point of vendoring it. A contributor who has cloned the repo and not
    yet built anything can still check a page they are editing, which is exactly the moment
    they need it.
    """
    done = subprocess.run(
        [sys.executable, str(CHECKER), "--selftest"],
        capture_output=True,
        text=True,
        timeout=120,
        cwd=REPO,
    )
    assert done.returncode == 0, (
        f"the vendored checker does not run standalone (exit {done.returncode}):\n"
        f"{(done.stdout + done.stderr)[-1500:]}"
    )


def test_the_checker_is_the_vendored_copy_and_not_a_local_rewrite() -> None:
    """It is a copy on purpose, and a copy that has drifted is worth knowing about.

    This cannot compare against the original, which lives outside this repo and is not
    present in a clone. What it can do is hold the copy to the shape that makes it a copy:
    no imports beyond the standard library, so it runs anywhere, and the self-test and
    entry point still present so a reader can run it.
    """
    text = CHECKER.read_text(encoding="utf-8")
    assert "def selftest(" in text, "the vendored checker has lost its self-test"
    assert "__main__" in text, "the vendored checker is no longer runnable on its own"
    imports = {
        line.split()[1].split(".")[0]
        for line in text.splitlines()
        if line.startswith("import ") or line.startswith("from ")
    }
    # ASKED OF PYTHON, not kept by hand. The rule is "standard library only, so it runs on a
    # clone with nothing installed", and the first version wrote out the modules the checker
    # happened to import that day. One of them gained `os` upstream and the gate called a
    # correct copy broken: a hand-kept list of what exists falling behind what exists.
    stdlib = set(sys.stdlib_module_names) | {"__future__"}
    assert imports <= stdlib, (
        f"the vendored checker imports {sorted(imports - stdlib)}, which is not in the standard "
        "library, so it no longer runs on a fresh clone with nothing installed"
    )
