"""Do this repo's pages link to things that exist, for a reader on github.com?

WHY THIS EXISTS
    `docs/media/README.md` linked three levels UP out of the repo, at
    `../../../RoyaleGym/docs/media/make_media.py`. A cloner is fine, because the four repos
    sit side by side. GitHub is not: it does not clamp the path, it rewrites the link to a
    URL under this repo's own blob tree, and the reader gets a 404. Docs found it by LOADING
    THE PAGE rather than by reasoning about the path, which is the only way that class is
    ever found.

    Every link on every page is now checked, and images have to exist.

WHAT IT CHECKS, AND WHAT IT DELIBERATELY DOES NOT
    That relative links resolve and images are present, over every TRACKED .md. It asks
    `git ls-files` rather than the disk, because a working copy carries gitignored files a
    reader will never have -- the same distinction that made five tests pass here and fail
    on every clone tonight.

    It does not fetch anything. An http link is not checked for being alive; that needs the
    network and would make the suite flaky, and a dead external link is a different and
    slower kind of wrong than one that never could have resolved.

    The checker is `tests/_doc_links.py`, a VENDORED COPY kept byte-identical to the
    original so the copies can be diffed against each other. `pyproject.toml` waives the two
    lint rules it trips, narrowly and with the reason there: reformatting a copy makes the
    next drift invisible, which is the one thing a copy exists to prevent.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CHECKER = Path(__file__).resolve().parent / "_doc_links.py"


def test_every_link_on_every_tracked_page_resolves() -> None:
    done = subprocess.run(
        [sys.executable, str(CHECKER), str(REPO)],
        capture_output=True, text=True, timeout=300, cwd=REPO,
    )
    assert done.returncode == 0, (
        "a page links to something that is not there:\n" + (done.stdout + done.stderr)[-3000:]
        + "\n\nA path that leaves the repo resolves for a cloner and 404s on github.com, "
        "which is the case this exists for. Use the web URL for another repo."
    )


def test_the_checkers_own_self_test_passes() -> None:
    """15 cases, and the one that matters is the last: a real broken img sitting beside prose
    ABOUT img tags, which is what proves the code-blanking did not blind it. Run it, do not
    assume it."""
    done = subprocess.run(
        [sys.executable, str(CHECKER), "--selftest"],
        capture_output=True, text=True, timeout=300, cwd=REPO,
    )
    assert done.returncode == 0, (done.stdout + done.stderr)[-2000:]
    assert "15 of 15" in done.stdout, done.stdout[-800:]


def test_the_checker_is_the_vendored_copy_and_not_a_local_rewrite() -> None:
    """It cannot be compared with the original, which is not in a clone. What it can be held
    to is the shape that makes it a copy: standard library only, and still runnable alone."""
    text = CHECKER.read_text(encoding="utf-8")
    assert "def selftest(" in text, "the vendored checker has lost its self-test"
    assert "__main__" in text, "the vendored checker is no longer runnable on its own"
    imports = {
        line.split()[1].split(".")[0]
        for line in text.splitlines()
        if line.startswith("import ") or line.startswith("from ")
    }
    stdlib_only = {"re", "sys", "subprocess", "pathlib", "unicodedata", "__future__"}
    assert imports <= stdlib_only, (
        f"the vendored checker imports {sorted(imports - stdlib_only)}, so it no longer runs "
        "on a fresh clone with nothing installed"
    )
