"""tools/cross_repo_skips.py, including the three ways a gate like it fails open.

The gate it guards runs only in the cross-repo workflow, where nothing in this suite can
watch it. So its behaviour is pinned here, on hand-written JUnit reports, and every case
that must REFUSE is exercised -- a checker nobody has seen refuse is the thing this whole
file exists to prevent.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
TOOL = ROOT / "tools" / "cross_repo_skips.py"

sys.path.insert(0, str(ROOT / "tools"))
import cross_repo_skips as C  # noqa: E402


def report(path: Path, cases: list[tuple[str, str | None]]) -> Path:
    """A JUnit report: ``(test name, skip reason or None)``."""
    body = []
    for name, reason in cases:
        if reason is None:
            body.append(f'<testcase classname="t" name="{name}"/>')
        else:
            body.append(
                f'<testcase classname="t" name="{name}">'
                f'<skipped message="{reason}" type="pytest.skip"/></testcase>'
            )
    path.write_text(
        '<?xml version="1.0" encoding="utf-8"?><testsuites><testsuite name="pytest">'
        + "".join(body)
        + "</testsuite></testsuites>",
        encoding="utf-8",
    )
    return path


def run(xml: Path, repo: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(TOOL), str(xml), repo], capture_output=True, text=True, cwd=ROOT
    )


def test_a_run_with_no_skips_passes(tmp_path: Path) -> None:
    r = run(report(tmp_path / "r.xml", [("test_a", None), ("test_b", None)]), "RoyaleGym")
    assert r.returncode == 0, r.stdout + r.stderr
    assert "0 skip(s), all declared" in r.stdout


def test_a_declared_skip_passes_and_the_reason_is_printed(tmp_path: Path) -> None:
    name = next(iter(C.ALLOWED["RoyaleViser"]))
    r = run(report(tmp_path / "r.xml", [(name, "whatever the test says today")]), "RoyaleViser")
    assert r.returncode == 0, r.stdout + r.stderr
    # The REASON PRINTED IS THIS FILE'S, not the test's: the declaration is what was
    # reviewed, and a test that later reworded its own message has not renegotiated it.
    assert "declared boundary" in r.stdout and "private" in r.stdout
    assert "whatever the test says today" not in r.stdout


def test_an_undeclared_skip_is_refused(tmp_path: Path) -> None:
    """The case the whole file is for."""
    r = run(
        report(tmp_path / "r.xml", [("test_something_new", "royalesim is not built")]),
        "RoyaleGym",
    )
    assert r.returncode == 1, r.stdout + r.stderr
    assert "1 SKIP(S) THIS JOB DOES NOT ACCEPT" in r.stdout
    assert "test_something_new" in r.stdout


def test_a_skip_whose_wording_is_innocent_is_still_refused(tmp_path: Path) -> None:
    """The prose gate's opposite failure: a reason mentioning nothing engine-shaped.

    The word-list version passed anything that did not name the engine, so a test skipping
    with "not supported here" was invisible to it. This gate never reads the words.
    """
    r = run(report(tmp_path / "r.xml", [("test_quiet", "not supported here")]), "RoyaleViser")
    assert r.returncode == 1, r.stdout + r.stderr
    assert "test_quiet" in r.stdout


def test_a_missing_report_fails_rather_than_reading_as_clean(tmp_path: Path) -> None:
    """Absent must not look like empty.

    Four instruments in this project have been found unable to tell those apart in a single
    night. A gate that exits 0 because it read nothing is the worst of them, because it is
    the one that certifies.
    """
    r = run(tmp_path / "does_not_exist.xml", "RoyaleGym")
    assert r.returncode == 1, r.stdout + r.stderr
    assert "could not read the JUnit report" in r.stdout


def test_a_malformed_report_fails(tmp_path: Path) -> None:
    p = tmp_path / "r.xml"
    p.write_text("<testsuites><testcase", encoding="utf-8")
    r = run(p, "RoyaleGym")
    assert r.returncode == 1, r.stdout + r.stderr
    assert "could not read the JUnit report" in r.stdout


def test_an_unknown_repo_is_refused_rather_than_defaulting_to_empty(tmp_path: Path) -> None:
    """A repo with no block must not read as "nothing is allowed, and nothing skipped"."""
    r = run(report(tmp_path / "r.xml", []), "RoyaleNotAThing")
    assert r.returncode == 1, r.stdout + r.stderr
    assert "no declaration block" in r.stdout


def test_a_declaration_that_no_longer_fires_is_reported(tmp_path: Path) -> None:
    r = run(report(tmp_path / "r.xml", [("test_a", None)]), "RoyaleViser")
    assert r.returncode == 0, r.stdout + r.stderr
    assert "STALE DECLARATION" in r.stdout


@pytest.mark.parametrize("repo", sorted(C.ALLOWED))
def test_every_declaration_says_why(repo: str) -> None:
    """A name with no reason is a name somebody waved through."""
    for name, why in C.ALLOWED[repo].items():
        assert len(why) > 40, (repo, name, why)
        assert name.startswith("test_"), (repo, name)
