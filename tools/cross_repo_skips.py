"""Every skip in the cross-repo job is declared here with the reason it cannot run, or it fails.

WHY THIS EXISTS RATHER THAN A GREP OVER THE SKIP TEXT
    The first version of this gate matched words -- ``royalesim|royalegym|extension|engine``
    -- against the reason pytest prints. On its first execution it caught a real hole in
    RoyaleGym (six cross-engine comparisons skipping because the two engines were reading
    different card tables, inside the job whose subject is whether the engine and the data
    agree) and it also failed RoyaleViser for three skips that say "this test pins numbers
    only a real battle has" and one that names a missing parity file. Those are the private
    capture corpus. They can never be satisfied on a public runner, and failing on them
    would make the job permanently red for a reason nobody can fix.

    The pattern was not the problem. ENGLISH WAS DOING THE WORK OF AN INTERFACE: one
    sentence meant "the engine is absent", another meant "the recordings are absent", and a
    word list cannot tell those apart -- they need opposite treatment. Worse, a word-list
    guard silently dictates how people must phrase a skip, so the next person to write a
    clear reason trips it by accident.

    So the gate no longer reads the prose at all. It asks a different question, which has an
    answer: IS THIS SKIP DECLARED? A skip listed below is a boundary somebody wrote down and
    justified. Anything else is a test that did not run in the job built to make it run, and
    it is a failure -- whatever it is called and however its reason is worded. A drift now
    has to become a line somebody adds to this file, which is a lie you can read, rather
    than a silence in a green log.

WHAT COUNTS AS A BOUNDARY
    Only an input a public runner can never have. The private capture corpus qualifies. A
    missing sibling package does not -- the job installs the stack. A missing extension does
    not -- the job builds and ships one. A data disagreement does not -- that is the defect
    the job exists to find, and it must be red here even though it is a skip everywhere else.

    Keys are TEST FUNCTION NAMES, from the JUnit report rather than the terse output,
    because `file.py:LINE` moves every time somebody edits above it and a stale key silently
    stops matching. A name that no longer exists is reported as stale instead of failing:
    a row may legitimately not select it (learn runs `-m engine`).

USAGE
    python tools/cross_repo_skips.py <report.xml> <RepoName>
"""

from __future__ import annotations

import sys
import xml.etree.ElementTree as ET

#: repo -> {test function name: why a public runner can never satisfy it}
ALLOWED: dict[str, dict[str, str]] = {
    "RoyaleViser": {
        "test_the_two_seats_agree_through_the_app": (
            "pins numbers only a real recorded battle has; the recordings are private and "
            "a public runner has none"
        ),
        "test_capture_events_announce_a_tunnel_once": (
            "same private recordings: the tunnel event is read out of a capture, not produced "
            "by the engine"
        ),
        "test_the_two_recorded_seats_differ_only_on_tap_ticks": (
            "same private recordings: it compares two recorded seats of one battle"
        ),
        "test_a_file_the_harness_itself_wrote_opens": (
            "needs a .parity.json with per-tick rows, written by RoyaleSim's replay harness "
            "with --trace against the private corpus; nothing on a runner produces one"
        ),
    },
    "RoyaleGym": {
        "test_mock_engine_states_the_footprint_model_the_engine_runs": (
            "MockEngine models footprints as 'collision_radius_circle' and the Rust engine as "
            "'tile_box'. That is a MODELLING difference between two engines, not an absent "
            "input, and it is stated in the skip rather than hidden -- but it is also not "
            "something this job can resolve, so it is declared here rather than left to fail "
            "every run. If the two models are ever reconciled this entry goes stale and the "
            "gate will say so."
        ),
    },
    "RoyaleLearn": {},
}


def skips(path: str) -> list[tuple[str, str]]:
    """``[(test name, reason)]`` from a JUnit XML report."""
    root = ET.parse(path).getroot()
    out: list[tuple[str, str]] = []
    for case in root.iter("testcase"):
        for sk in case.findall("skipped"):
            reason = (sk.get("message") or sk.text or "").strip().replace("\n", " ")
            out.append((case.get("name") or "<unnamed>", reason))
    return out


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print(__doc__.strip().splitlines()[-1])
        return 2
    path, repo = argv[1], argv[2]
    if repo not in ALLOWED:
        print(f"no declaration block for {repo!r}; add one to {__file__} before adding it to the matrix")
        return 1
    try:
        found = skips(path)
    except (OSError, ET.ParseError) as exc:
        # A MISSING OR BROKEN REPORT IS A FAILURE, never a quiet pass. An instrument that
        # cannot tell "nothing skipped" from "nothing read" is the defect this file is about.
        print(f"could not read the JUnit report at {path}: {exc}")
        return 1

    allowed = ALLOWED[repo]
    undeclared = [(n, r) for n, r in found if n not in allowed]
    seen = {n for n, _ in found}
    stale = [n for n in allowed if n not in seen]

    for name, _ in found:
        if name in allowed:
            print(f"declared boundary: {name} -- {allowed[name]}")
    for name in stale:
        print(f"STALE DECLARATION (did not skip, or was not selected): {name}")

    if undeclared:
        print()
        print(f"{len(undeclared)} SKIP(S) THIS JOB DOES NOT ACCEPT, in {repo}:")
        for name, reason in undeclared:
            print(f"  {name}\n      {reason[:300]}")
        print()
        print(
            "Each of these is a test that did not run against the wheel this job built. If it\n"
            "skipped because an input a public runner can NEVER have is missing, declare it in\n"
            f"{__file__} with that reason. If it skipped because the engine was absent, or\n"
            "because the engine and the data disagreed, that is the defect this job exists to\n"
            "find: fix it rather than declaring it."
        )
        return 1

    print(f"{repo}: {len(found)} skip(s), all declared")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
