"""Does the version a README promises match the one the build enforces?

The last pure-shape row of the doc-gates table. A README tells a reader which Python and which Rust
they need. A manifest tells the build the same thing. Nothing has ever compared the two, in any of
the four repos, and they are edited by different people for different reasons: prose gets updated
when someone rewrites the install, `requires-python` gets updated when someone hits a build error.

Two ways this goes wrong, and the second is the one worth catching:

1. **They disagree.** The README says Python 3.11, the manifest says `>=3.12`. A reader installs
   3.11, the install fails late, and the page that sent them there still reads correctly.
2. **The README promises a minimum nothing enforces.** The README says "Rust 1.80 or newer" and no
   `Cargo.toml` declares `rust-version`. Cargo will happily start a build on 1.70 and fail somewhere
   inside a dependency, which is the failure the README existed to prevent. An unenforced promise
   looks exactly like an enforced one from the reader's side.

WHAT COUNTS AS A CLAIM: a version attached to a NAMED tool. `Python 3.12`, `Rust 1.80`, `rustc
1.80`, `maturin>=1.15`, and the `python-3.12+` shields.io badge. Anchoring on the tool name is what
keeps this usable: a README is full of bare decimals that are not versions -- `1.16 to 1.38x`
faster, a `1.03 MB` gif, `0.4%` of the error -- and a check that read those as versions would
refuse every correct page it saw.

WHAT IT DOES NOT CHECK: whether the declared minimum is TRUE (that needs a build on that version),
whether a newer version also works, or anything about transitive dependencies. Shape only.

Usage:  python version_match_check.py <repo> [more repos...]
        python version_match_check.py --selftest
Exit 0 if every repo passes, 1 otherwise.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

FENCE = re.compile(r"^( {0,3})(`{3,}|~{3,})[^\n]*\n.*?^\1?\2`*~*[ \t]*$", re.M | re.S)

# A version only counts when a tool name is attached to it.
CLAIMS = {
    "python": re.compile(r"\bPython\s+v?(\d+\.\d+)|\bpython-(\d+\.\d+)\+?-", re.I),
    "rust": re.compile(r"\bRust(?:c)?\s+v?(\d+\.\d+)|\brust-(\d+\.\d+)\+?-", re.I),
    "maturin": re.compile(r"\bmaturin\s*[>=~^]*\s*(\d+\.\d+)", re.I),
}

DECLARED = {
    "python": re.compile(r"""^requires-python\s*=\s*["']\s*>=\s*(\d+\.\d+)""", re.M),
    "rust": re.compile(r"""^rust-version\s*=\s*["'](\d+\.\d+)""", re.M),
    "maturin": re.compile(r"""maturin\s*>=\s*(\d+\.\d+)""")
}

# Where the build's own answer lives. A repo without the file simply has no claim to check.
MANIFESTS = {
    "python": ["pyproject.toml"],
    "rust": ["Cargo.toml", "crates/*/Cargo.toml"],
    "maturin": ["pyproject.toml"],
}


def declared_versions(repo: Path) -> dict[str, tuple[str, str]]:
    """{tool: (version, which file said so)} from this repo's manifests, then its siblings'.

    The siblings matter and this check refused a correct page before they were searched.
    RoyaleLearn's README promises Rust 1.80 because its install compiles RoyaleSim, and RoyaleLearn
    has no `Cargo.toml` of its own; the manifest enforcing that promise is next door. Refusing it
    would have been cry-wolf.

    But the claim is still worth checking, because it is the same shape as the stale figures found
    on 2026-09-22: one fact written on several pages, where correcting the source leaves the copies
    wrong. If RoyaleSim ever raises `rust-version`, nothing updates RoyaleLearn's README. So a
    sibling's answer counts, and the reported file name says which repo it came from.
    """
    found: dict[str, tuple[str, str]] = {}
    others = sorted(p for p in repo.resolve().parent.iterdir()
                    if p.is_dir() and p.name.startswith("Royale") and p.name != repo.resolve().name)
    for tool, globs in MANIFESTS.items():
        for where in [repo, *others]:
            for pattern in globs:
                paths = sorted(where.glob(pattern)) if "*" in pattern else [where / pattern]
                for path in paths:
                    if not path.is_file():
                        continue
                    m = DECLARED[tool].search(path.read_text(encoding="utf-8", errors="replace"))
                    if m:
                        label = path.name if where == repo else f"{where.name}/{path.name}"
                        found[tool] = (m.group(1), label)
                        break
                if tool in found:
                    break
            if tool in found:
                break
    return found


def claims_in(text: str) -> dict[str, set[str]]:
    """{tool: {versions the prose promises}}, with fenced blocks left in.

    Fences are NOT blanked here, unlike the other checks in this folder. An install block is a
    fence, and `python -m venv` sitting beside "you need Python 3.12" is exactly where the promise
    lives. What is blanked is nothing; what is anchored is the tool name.
    """
    out: dict[str, set[str]] = {}
    for tool, pattern in CLAIMS.items():
        got = {g for m in pattern.finditer(text) for g in m.groups() if g}
        if got:
            out[tool] = got
    return out


def check_repo(name: str, readme: str, declared: dict[str, tuple[str, str]]) -> list[str]:
    out = []
    for tool, versions in sorted(claims_in(readme).items()):
        if tool not in declared:
            out.append(
                f"{name}: the README promises {tool} {sorted(versions)[0]} or newer and no manifest "
                f"declares it. Nothing enforces that minimum, so the build starts on an older "
                f"{tool} and fails somewhere a reader cannot connect to this page."
            )
            continue
        want, where = declared[tool]
        if len(versions) > 1:
            # One of them can match the manifest and the page still contradicts itself. A badge
            # saying 3.11 above prose saying 3.12 is two promises, and a reader believes whichever
            # one they read first.
            out.append(
                f"{name}: the README gives {tool} more than one minimum, "
                f"{', '.join(sorted(versions))}. A reader believes whichever they read first, and "
                f"{where} can only agree with one of them."
            )
            continue
        if want not in versions:
            out.append(
                f"{name}: the README says {tool} {', '.join(sorted(versions))} and {where} says "
                f"{want}. One of them is wrong and the reader only sees the README."
            )
    return out


SELFTEST = [
    ("python agrees", "You need Python 3.12.", {"python": ("3.12", "pyproject.toml")}, False),
    ("python disagrees", "You need Python 3.11.", {"python": ("3.12", "pyproject.toml")}, True),
    ("a badge agrees", "![py](https://x/badge/python-3.12+-blue)",
     {"python": ("3.12", "pyproject.toml")}, False),
    ("a badge disagrees", "![py](https://x/badge/python-3.11+-blue)",
     {"python": ("3.12", "pyproject.toml")}, True),
    ("rust promised, nothing declares it", "You need Rust 1.80 or newer.", {}, True),
    ("rust promised and declared", "You need Rust 1.80 or newer.",
     {"rust": ("1.80", "Cargo.toml")}, False),
    ("maturin pinned in prose and build", "pip install maturin>=1.15",
     {"maturin": ("1.15", "pyproject.toml")}, False),
    # The reason this anchors on tool names: a README is full of decimals that are not versions.
    ("a speed ratio is not a version", "The Rust engine runs 1.16 to 1.38 times the stand-in.",
     {}, False),
    ("a file size is not a version", "A 1.03 MB gif.", {}, False),
    ("a percentage is not a version", "Walking is the smallest row at 0.4%.", {}, False),
    ("no version claims at all", "The engine replays recorded battles.", {}, False),
    # Both spellings of the same promise must agree with each other via the manifest.
    ("badge and prose disagree with each other",
     "![py](https://x/badge/python-3.11+-blue)\n\nYou need Python 3.12.",
     {"python": ("3.12", "pyproject.toml")}, True),
]


def selftest() -> int:
    bad = 0
    for name, readme, declared, must_fail in SELFTEST:
        got = bool(check_repo("<repo>", readme, declared))
        ok = got == must_fail
        bad += not ok
        print(f"{'ok  ' if ok else 'FAIL'} {name}: {'refused' if got else 'passed'} "
              f"({'should refuse' if must_fail else 'should pass'})")
    print(f"\n{len(SELFTEST) - bad} of {len(SELFTEST)} self-tests behaved")
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    if len(argv) == 2 and argv[1] == "--selftest":
        return selftest()
    if len(argv) < 2:
        raise SystemExit("usage: version_match_check.py <repo> [more repos...] | --selftest")
    bad = 0
    for name in argv[1:]:
        repo = Path(name)
        readme = repo / "README.md"
        if not readme.is_file():
            print(f"REFUSE {name}: no README.md")
            bad += 1
            continue
        problems = check_repo(name, readme.read_text(encoding="utf-8", errors="replace"),
                              declared_versions(repo))
        for p in problems:
            print(f"REFUSE {p}")
        bad += bool(problems)
    print(f"\n{len(argv) - 1 - bad} of {len(argv) - 1} repos passed")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
