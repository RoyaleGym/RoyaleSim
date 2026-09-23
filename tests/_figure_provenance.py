"""Does every published percentage say where it came from?

The "measured figures are current" row of the doc-gates table, approached from the side that can
actually be checked. Whether a figure is CURRENT needs a fresh measurement and a judgement. Whether
it says WHAT PRODUCED IT is pure shape, and it is the half that keeps working after the figure goes
stale: a number carrying its run stays checkable forever, a bare number is only checkable by
someone who already knows which run it came from, which is nobody in six weeks.

This was worth building because of what happened on 2026-09-22. A figure of 49.4 per cent sat on
three pages. When it moved to 56.5, one page was two runs behind and nobody could tell by reading
it, because no page said which run it was from. Correcting one left the other two wrong, twice over
in one evening.

WHAT COUNTS AS A MEASUREMENT: a percentage. Not every number, deliberately. A README is full of
digits that measure nothing -- elixir costs, tile counts, version numbers, tick rates -- and a
check that demanded provenance for all of them would refuse every correct page it read. A
percentage is the shape a reader trusts as a result, and it is the shape that goes stale.

WHAT COUNTS AS PROVENANCE, within the same section as the figure:

  - a build digest or commit: 7 to 40 hex characters as a word
  - a date: 2026-09-22
  - a phrase that names the run: "measured at", "build digest", "from the run", "at build"
  - a named corpus size: "over 270,972 unit-ticks", "73 fixtures"

A SECTION, not the whole page, and not a character window. A reader arriving from a link lands on a
heading and reads down from it; they do not scroll up past a heading to find a caveat. That is the
learn session's freshness-banner defect stated as a rule: per-claim beats per-page, and a section
is the coarsest unit that still holds.

WHAT IT DOES NOT CHECK: whether the figure is right, whether the provenance is real, whether the
run it names is the latest, or whether a digest belongs to the number beside it. A gate oversold is
worse than none. It also cannot see the failure it most wants to: a fabricated digest satisfies
this check perfectly, which is why the postmortem treats provenance rules as inviting that
specific lie.

Usage:  python figure_provenance_check.py <path-or-repo> [more...]
        python figure_provenance_check.py --selftest
Exit 0 if every file passes, 1 otherwise.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

FENCE = re.compile(r"^( {0,3})(`{3,}|~{3,})[^\n]*\n.*?^\1?\2`*~*[ \t]*$", re.M | re.S)
# An HTML attribute is not a measurement. Every one of this check's first four refusals was
# width="100%" on an <img>, which is a layout instruction and the only percentage on those pages.
HTML_TAG = re.compile(r"<[^>]+>", re.S)
# A URL is not prose. Percent-ENCODING reads as a percentage: a shields.io badge whose label is
# "78%20public" contains the characters of "78%", and three of this check's next four refusals were
# badge URLs. Blank the target of every markdown link and image.
MD_TARGET = re.compile(r"\]\(([^)\s]*)")
HEADING = re.compile(r"^#{1,6} .*$", re.M)
PERCENT = re.compile(r"\b\d{1,3}(?:[.,]\d+)?\s?%|\b\d{1,3}(?:[.,]\d+)?\s+per cent\b")

PROVENANCE = [
    re.compile(r"\b[0-9a-f]{7,40}\b"),
    re.compile(r"\b20\d\d-\d\d-\d\d\b"),
    # The verb list was too narrow and the sim session caught it doing real damage: "counted over
    # the offline trace corpus of client 15.535.29" is as informative as "measured on" and did not
    # match, so pages were REWORDED to fit the list rather than improved by it. A check that edits
    # prose into its own vocabulary is worse than one that misses, because the page gets less
    # readable and the check still learns nothing. Accept the ways people actually say it.
    re.compile(r"\b(measure[ds]?|counted|sampled|observed|taken|scored|derived)\s+"
               r"(at|on|over|across|from|in)\b", re.I),
    re.compile(r"\b(build digest|build_digest|from the run|at build|the run at|re-measured)\b",
               re.I),
    re.compile(r"\bcorpus\b", re.I),
    re.compile(r"\bclient\s+\d+\.\d+", re.I),
    re.compile(r"\b\d[\d,]{2,}\s+(unit-ticks|ticks|battles|fixtures|samples|rows)\b", re.I),
    re.compile(r"\b\d+\s+(fixtures|battles|recorded battles)\b", re.I),
]

# A tag marking a number as a SHIPPED VALUE rather than something we measured. RoyaleSim's specs
# tag every table row `DATA`, `COMMUNITY` or `INFERENCE`, and a crown-tower percentage read out of
# the card tables has no run behind it to name. Matched only as a fenced or tabled token, never as
# the bare word, so a sentence mentioning data cannot exempt a real figure beside it.
NOT_A_MEASUREMENT = re.compile(r"`(DATA|COMMUNITY)`|\|\s*(DATA|COMMUNITY)\s*\|")


def expand(args: list[str]) -> list[str]:
    """Files as given; a DIRECTORY or repo becomes its tracked `.md` files.

    The usage line said `<repo> <path>...` and the code opened every argument as a file, so the
    documented first argument raised PermissionError on the directory. A caller reads that as a
    refusal from the check rather than as a mistake in the check, which is the worst way for a
    guard to be wrong: it reports on a page it never read. The sim session hit it.
    """
    out: list[str] = []
    for arg in args:
        path = Path(arg)
        if path.is_dir():
            done = subprocess.run(["git", "-C", arg, "ls-files", "*.md", "**/*.md"],
                                  capture_output=True)
            names = done.stdout.decode("utf-8", errors="replace").split() if not done.returncode \
                else [str(p.relative_to(path)) for p in sorted(path.rglob("*.md"))]
            out.extend(f"{arg}/{n}" for n in names)
        else:
            out.append(arg)
    return out


def line_holding(text: str, at: int) -> str:
    """The single line a match sits on. The DATA tag marks a ROW, not a section."""
    start = text.rfind("\n", 0, at) + 1
    end = text.find("\n", at)
    return text[start:] if end == -1 else text[start:end]


def sections(text: str) -> list[tuple[int, str]]:
    """The page split at headings, each with the line its body starts on."""
    cuts = [m.start() for m in HEADING.finditer(text)]
    bounds = [0] + cuts + [len(text)]
    out = []
    for i in range(len(bounds) - 1):
        chunk = text[bounds[i]:bounds[i + 1]]
        if chunk.strip():
            out.append((text[:bounds[i]].count("\n") + 1, chunk))
    return out


def check_page(page: str, text: str) -> list[str]:
    blank = lambda m: "\n" * m.group(0).count("\n")  # noqa: E731 - keeps line numbers honest
    body = MD_TARGET.sub(blank, HTML_TAG.sub(blank, FENCE.sub(blank, text)))
    out = []
    for first_line, chunk in sections(body):
        figures = [m for m in PERCENT.finditer(chunk)
                   if not NOT_A_MEASUREMENT.search(line_holding(chunk, m.start()))]
        if not figures:
            continue
        if any(p.search(chunk) for p in PROVENANCE):
            continue
        shown = ", ".join(sorted({m.group(0).strip() for m in figures})[:4])
        line = first_line + chunk[:figures[0].start()].count("\n")
        out.append(
            f"{page}:{line}: this section states {len(figures)} measured figure(s) ({shown}) and "
            f"nothing in it says what produced them. A reader who arrived at this heading from a "
            f"link cannot scroll up to a caveat. Name the run, the build or the date beside the "
            f"figure."
        )
    return out


SELFTEST = [
    ("a bare percentage", "# H\n\nThe engine agrees 56.5% of the time.\n", True),
    ("a percentage with a digest", "# H\n\n56.5% at build `d872d792711934c2`.\n", False),
    ("a percentage with a date", "# H\n\n56.5%, measured 2026-09-22.\n", False),
    ("a percentage with a corpus", "# H\n\n56.5% over 270,972 unit-ticks.\n", False),
    ("per cent spelled out", "# H\n\nThe engine agrees 56.5 per cent of the time.\n", True),
    ("no figures at all", "# H\n\nThe engine replays recorded battles.\n", False),
    ("a figure inside a fence only", "# H\n\n```\n56.5%\n```\n", False),
    # The section rule: a caveat under a LATER heading does not cover an earlier figure.
    ("provenance in a different section",
     "# A\n\nThe engine agrees 56.5% of the time.\n\n# B\n\nMeasured 2026-09-22.\n", True),
    ("provenance in the same section, below the figure",
     "# A\n\nThe engine agrees 56.5% of the time.\n\nAll of it measured 2026-09-22.\n", False),
    # A page-top banner is exactly the shape this refuses to accept as cover.
    ("a banner above the first heading",
     "Everything below was measured 2026-09-22.\n\n# A\n\nIt agrees 56.5% of the time.\n", True),
    # An HTML attribute is a layout instruction, not a result.
    ("a width attribute", '# H\n\n<img src="a.svg" width="100%" alt="a thing">\n', False),
    ("a real figure beside a width attribute",
     '# H\n\n<img src="a.svg" width="100%" alt="a thing">\n\nIt agrees 56.5% of the time.\n', True),
    # Percent-ENCODING in a badge URL is not a percentage.
    ("a percent-encoded badge url", "# H\n\n![cards](https://x/badge/cards-78%20public-555)\n", False),
    ("a real figure beside a badge url",
     "# H\n\n![c](https://x/badge/cards-78%20public-555)\n\nIt agrees 56.5% of the time.\n", True),
    # A shipped table value has no run to name. The tag must be a tabled or fenced token.
    ("a DATA-tagged table row", "# H\n\n| crown damage | 30% | DATA | towers.csv |\n", False),
    ("a real figure beside a DATA row",
     "# H\n\n| crown damage | 30% | DATA | towers.csv |\n\nIt agrees 56.5% of the time.\n", True),
    ("the bare word data does not exempt",
     "# H\n\nThe data shows it agrees 56.5% of the time.\n", True),
    # The vocabulary must not force a rewrite. These say where the number came from.
    ("counted over a named corpus",
     "# H\n\n56.5%, counted over the offline trace corpus of client 15.535.29.\n", False),
    ("sampled from a corpus", "# H\n\n56.5%, sampled from the replay corpus.\n", False),
    ("a client version alone", "# H\n\n56.5% on client 15.535.29.\n", False),
]


def selftest() -> int:
    bad = 0
    for name, text, must_fail in SELFTEST:
        got = bool(check_page("<page>", text))
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
        raise SystemExit("usage: figure_provenance_check.py <path-or-repo> [more...] | --selftest")
    bad = total = 0
    for path in expand(argv[1:]):
        with open(path, encoding="utf-8", errors="replace") as fh:
            problems = check_page(path, fh.read())
        for p in problems:
            print(f"REFUSE {p}")
        bad += bool(problems)
        total += 1
    print(f"\n{total - bad} of {total} files passed")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
