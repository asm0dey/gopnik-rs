#!/usr/bin/env python3
"""Tests for tools/mutate.py -- the gate that catches checks which cannot fail.

The centrepiece is `VacuousCaseTest`.  A gate against this defect class that
has never been seen catching an instance of it is the defect wearing a third
coat of paint, so a deliberately vacuous case is registered here and the gate
is required to report it BY NAME with a non-zero exit -- once in its natural
habitat (a real committed test paired with an oracle it does not read) and once
in the smallest possible form (an assertion that cannot fail at all).

The other half is the safety property, asserted the same way: not by reading
the source for writes, but by running the gate over the shipped manifest and
recomputing the real digests and `git status` afterwards.

Standard library only.  `VacuousCaseTest` and `ShippedManifestTest` build a
shadow tree and compile the port in it, so they take a few seconds; every other
test drives the runner with `python3 -c` commands and is instant.
"""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import mutate  # noqa: E402

REPO = mutate.REPO

# A test command that READS the artifact and reports the value it found, so a
# case built on it is honestly green-then-red.  Run with cwd = the shadow tree.
#
# The message is written at RUNTIME rather than carried in an `assert`'s second
# operand, and that is not a style choice: a Python traceback echoes the source
# line it failed on, so an `assert x == 61, "the draw moved"` prints "the draw
# moved" even when it died of a TypeError three tokens earlier.  An `expect`
# matched against that would pass on any crash -- the gate's own version of the
# defect it exists to catch.  Found while writing
# `test_a_corrupted_artifact_does_not_count_as_red`, which is the test that
# would have been vacuous.
HONEST_TEST = [
    "python3", "-c",
    "import json,sys\n"
    "d = json.load(open('data/rng_trace.json'))\n"
    "r = d['runs'][0]['draws'][100]['r']\n"
    "sys.stderr.write('run A draw 101 came back %r\\n' % (r,))\n"
    "sys.exit(r != 61)\n",
]
HONEST_EXPECT = "run A draw 101 came back 62"
DRAW_MUTATION = {"op": "json-set", "path": ["runs", 0, "draws", 100, "r"],
                 "from": 61, "to": 62}


# The `cargo test` flags used here that CONSUME the argument after them.  Any
# other value-taking flag would need adding, and the direction of that mistake
# is the safe one: an unlisted flag's value reads as a filter, which REJECTS a
# finding loudly rather than admitting a bogus one.
CARGO_VALUE_FLAGS = ("--test",)


def is_filtered(cmd):
    """Does this command run a SUBSET of the tests rather than a whole suite?

    Both runners take a filter as a bare argument:
    `cargo test --test wander_sequence run_a_replays_exactly`, and
    `python3 tools/test_rngtrace.py FightFoldTest.test_x`.  So: any bare
    argument that is neither the thing being invoked nor a flag's value.

    Counting bare arguments and requiring more than one -- which is what this
    did first -- misses `cargo test <filter>`, which has exactly one.  That
    shape runs every BINARY but only the tests whose name matches, so a finding
    registered with it would claim a suite-wide silence it never established.
    """
    if cmd[:2] == ["cargo", "test"]:
        rest = list(cmd[2:])
        for flag in CARGO_VALUE_FLAGS:
            while flag in rest:
                i = rest.index(flag)
                del rest[i:i + 2]       # the flag and the binary it names
        return any(not a.startswith("-") for a in rest)
    return len(cmd) > 2 and not cmd[-1].startswith("-")


def manifest_file(td, cases):
    p = Path(td) / "mutations.json"
    p.write_text(json.dumps({"cases": cases}))
    return str(p)


def gate(cases, only=None):
    """Run the gate over `cases`, returning (exit status, printed report).

    Mirrors `mutate.main`, which turns a `GateError` into status 2.
    """
    import io
    out = io.StringIO()
    with tempfile.TemporaryDirectory() as td:
        try:
            rc = mutate.run_manifest(manifest_file(td, cases), only=only,
                                     out=out)
        except mutate.GateError as e:
            return 2, out.getvalue() + "\nGateError: %s" % e
    return rc, out.getvalue()


class VacuousCaseTest(unittest.TestCase):
    """The gate must be SEEN catching a check that cannot fail."""

    def test_a_real_test_paired_with_an_oracle_it_never_reads_is_caught(self):
        """The defect in its natural habitat, beside an honest case.

        `run_a_replays_exactly` in `tests/wander_sequence.rs` is a real,
        committed, passing test.  Pointed at `data/combat_trace.json` -- which
        it does not read -- it cannot fail no matter what that oracle says,
        which is exactly the shape every review of this project has found.

        Both cases run in one gate invocation so the report can be read
        side by side: the honest one must be `ok`, the vacuous one `FAIL`, and
        the process status must be non-zero.  If the vacuous case were also
        reported `ok` the gate would be worthless, and if the honest one were
        reported `FAIL` the gate would be broken in a way that hides that.
        """
        honest = {
            "label": "honest-combat-draw-stream",
            "defends": "the combat draw stream, asserted by the test that reads it",
            "artifact": "data/combat_trace.json",
            "mutate": {"op": "json-set",
                       "path": ["runs", 0, "draws", 150, "r"],
                       "from": 79, "to": 80},
            "test": ["cargo", "test", "--test", "combat_sequence",
                     "run_a_replays_exactly"],
            "expect": "draw sequence diverges from data/combat_trace.json",
        }
        vacuous = dict(honest, label="vacuous-wrong-test-for-the-artifact",
                       defends="nothing: this test never opens the artifact",
                       test=["cargo", "test", "--test", "wander_sequence",
                             "run_a_replays_exactly"],
                       expect="draw sequence diverges")
        rc, report = gate([honest, vacuous])
        self.assertNotEqual(rc, 0, report)
        self.assertIn("ok     honest-combat-draw-stream", report)
        self.assertIn("FAIL   vacuous-wrong-test-for-the-artifact", report)
        self.assertIn("still PASSED with data/combat_trace.json mutated",
                      report)
        self.assertIn("this assertion cannot fail", report)
        self.assertIn("GATE FAILURE: 1 of 2 case(s) did not go red: "
                      "vacuous-wrong-test-for-the-artifact", report)
        # And the real oracle is untouched by a run that FAILED, which is the
        # path a safety bug would hide in.
        # Derived from `mutate.GUARDED`, not spelled out: pinning the literal
        # made this test fail the day a root was ADDED (Task 16 added `docs`,
        # for a case that patches a `.md`), which is a false alarm about the
        # very property it is here to confirm -- that a FAILED run still left
        # every real file alone.
        self.assertIn("real file(s) under %s unchanged"
                      % ("/, ".join(mutate.GUARDED) + "/"), report)
        for root in mutate.GUARDED:
            self.assertIn(root + "/", report)

    def test_an_assertion_that_cannot_fail_at_all_is_caught(self):
        """The smallest form: a throwaway assertion with no way to be false."""
        rc, report = gate([{
            "label": "vacuous-assert-true",
            "defends": "nothing whatsoever",
            "artifact": "data/rng_trace.json",
            "mutate": DRAW_MUTATION,
            "test": ["python3", "-c", "assert True  # cannot fail"],
            "expect": "unreachable",
        }])
        self.assertEqual(rc, 1, report)
        self.assertIn("FAIL   vacuous-assert-true", report)
        self.assertIn("this assertion cannot fail", report)

    def test_an_honest_case_is_reported_as_going_red(self):
        rc, report = gate([{
            "label": "honest",
            "defends": "the draw the command reads",
            "artifact": "data/rng_trace.json",
            "mutate": DRAW_MUTATION,
            "test": HONEST_TEST,
            "expect": HONEST_EXPECT,
        }])
        self.assertEqual(rc, 0, report)
        self.assertIn("ok     honest", report)
        self.assertIn("green -> red", report)
        self.assertIn("all 1 case(s) went red", report)


class RedForTheWrongReasonTest(unittest.TestCase):
    """Going red is not enough; it has to go red on the claim."""

    def test_a_failure_that_never_names_the_claim_is_a_gate_failure(self):
        rc, report = gate([{
            "label": "red-but-not-on-the-claim",
            "defends": "the draw",
            "artifact": "data/rng_trace.json",
            "mutate": DRAW_MUTATION,
            "test": HONEST_TEST,
            "expect": "a message this test never prints",
        }])
        self.assertEqual(rc, 1, report)
        self.assertIn("FAIL   red-but-not-on-the-claim", report)
        self.assertIn("did not fail on the claim this case defends", report)

    def test_a_corrupted_artifact_does_not_count_as_red(self):
        """A mutation that merely breaks the file proves nothing.

        The whole draw record is replaced by a string, so the reader dies of a
        `TypeError` before it ever compares anything.  The process exits
        non-zero -- "the test went red" -- and the claim was never tested.  The
        `expect` check is the only thing that separates the two, which is why
        it is not optional.
        """
        rc, report = gate([{
            "label": "corrupted-not-falsified",
            "defends": "the draw",
            "artifact": "data/rng_trace.json",
            "mutate": {"op": "json-set", "path": ["runs", 0, "draws", 100],
                       "from": {"i": 101, "turn": 6, "site": "1000:b39e",
                                "n": 200, "r": 61},
                       "to": "not a draw at all"},
            "test": HONEST_TEST,
            "expect": HONEST_EXPECT,
        }])
        self.assertEqual(rc, 1, report)
        self.assertIn("FAIL   corrupted-not-falsified", report)
        self.assertIn("did not fail on the claim", report)


class BaselineTest(unittest.TestCase):
    """A test that was already failing proves nothing by failing again."""

    def test_a_case_whose_test_is_already_red_is_a_gate_failure(self):
        rc, report = gate([{
            "label": "already-red",
            "defends": "nothing, because it never passed",
            "artifact": "data/rng_trace.json",
            "mutate": DRAW_MUTATION,
            "test": ["python3", "-c", "raise SystemExit(3)"],
            "expect": "anything",
        }])
        self.assertEqual(rc, 1, report)
        self.assertIn("FAIL   already-red", report)
        self.assertIn("ALREADY red on the unmutated copy", report)


class ManifestDriftTest(unittest.TestCase):
    """A case that no longer matches the artifact must stop the run."""

    def test_a_from_value_that_has_moved_is_an_error_not_a_silent_pass(self):
        rc, report = gate([{
            "label": "drifted",
            "defends": "the draw",
            "artifact": "data/rng_trace.json",
            "mutate": dict(DRAW_MUTATION, **{"from": 999}),
            "test": HONEST_TEST,
            "expect": HONEST_EXPECT,
        }])
        self.assertEqual(rc, 2, report)

    def test_a_perturbation_that_changes_nothing_is_an_error(self):
        with tempfile.TemporaryDirectory() as td:
            f = Path(td) / "a.json"
            f.write_text(json.dumps({"x": 1}))
            with self.assertRaises(mutate.GateError) as cm:
                mutate.apply_mutation(
                    {"op": "json-set", "path": ["x"], "from": 1, "to": 1},
                    f, "noop")
            self.assertIn("does not change anything", str(cm.exception))

    def test_a_text_replacement_that_does_not_match_is_an_error(self):
        with tempfile.TemporaryDirectory() as td:
            f = Path(td) / "a.py"
            f.write_text("keep me\n")
            with self.assertRaises(mutate.GateError):
                mutate.apply_mutation(
                    {"op": "text-replace", "from": "gone", "to": ""},
                    f, "textual")

    def test_an_unknown_op_is_an_error(self):
        with tempfile.TemporaryDirectory() as td:
            f = Path(td) / "a.json"
            f.write_text("{}")
            with self.assertRaises(mutate.GateError):
                mutate.apply_mutation({"op": "delete-everything"}, f, "bad")

    def test_an_empty_manifest_is_refused_rather_than_passing_vacuously(self):
        with tempfile.TemporaryDirectory() as td:
            with self.assertRaises(mutate.GateError) as cm:
                mutate.run_manifest(manifest_file(td, []))
            self.assertIn("registers no cases", str(cm.exception))

    def test_duplicate_labels_are_refused(self):
        c = {"label": "same", "defends": "", "artifact": "data/rng_trace.json",
             "mutate": DRAW_MUTATION, "test": HONEST_TEST, "expect": "x"}
        with tempfile.TemporaryDirectory() as td:
            with self.assertRaises(mutate.GateError):
                mutate.run_manifest(manifest_file(td, [c, dict(c)]))


class CaseSelectionTest(unittest.TestCase):
    def test_case_runs_exactly_one(self):
        honest = {"label": "honest", "defends": "", "artifact":
                  "data/rng_trace.json", "mutate": DRAW_MUTATION,
                  "test": HONEST_TEST, "expect": HONEST_EXPECT}
        broken = dict(honest, label="vacuous",
                      test=["python3", "-c", "assert True"])
        rc, report = gate([honest, broken], only="honest")
        self.assertEqual(rc, 0, report)
        self.assertIn("1 case(s)", report)
        self.assertNotIn("vacuous", report)

    def test_an_unknown_label_names_the_ones_that_exist(self):
        honest = {"label": "honest", "defends": "", "artifact":
                  "data/rng_trace.json", "mutate": DRAW_MUTATION,
                  "test": HONEST_TEST, "expect": "x"}
        with tempfile.TemporaryDirectory() as td:
            with self.assertRaises(mutate.GateError) as cm:
                mutate.run_manifest(manifest_file(td, [honest]), only="nope")
            self.assertIn("honest", str(cm.exception))


class ContainmentTest(unittest.TestCase):
    """Nothing the gate writes may land outside the shadow tree."""

    def test_an_absolute_artifact_path_is_refused(self):
        with self.assertRaises(mutate.GateError):
            mutate._under("/tmp", str(REPO / "data" / "rng_trace.json"))

    def test_a_dot_dot_artifact_path_is_refused(self):
        with self.assertRaises(mutate.GateError):
            mutate._under("/tmp", "../data/rng_trace.json")

    def test_an_empty_artifact_path_is_refused(self):
        with self.assertRaises(mutate.GateError):
            mutate._under("/tmp", "")

    def test_a_symlink_out_of_the_tree_is_refused(self):
        """The lexical check alone would let this through."""
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "shadow"
            (root / "data").mkdir(parents=True)
            (root / "data" / "rng_trace.json").symlink_to(
                REPO / "data" / "rng_trace.json")
            with self.assertRaises(mutate.GateError) as cm:
                mutate._under(root, "data/rng_trace.json")
            self.assertIn("outside", str(cm.exception))

    def test_a_case_naming_a_path_outside_the_tree_never_writes(self):
        before = mutate.digest(REPO / "data" / "rng_trace.json")
        rc, report = gate([{
            "label": "escaping",
            "defends": "",
            "artifact": "../../data/rng_trace.json",
            "mutate": DRAW_MUTATION,
            "test": HONEST_TEST,
            "expect": "x",
        }])
        self.assertEqual(rc, 2, report)
        self.assertEqual(mutate.digest(REPO / "data" / "rng_trace.json"),
                         before)

    def test_a_shadow_tree_inside_the_repo_is_refused(self):
        with self.assertRaises(mutate.GateError):
            mutate.build_shadow(REPO / "build" / "mutate-shadow")

    def test_read_real_returns_the_real_bytes_and_refuses_a_path_outside(self):
        """The single door to the repo goes through the same containment check.

        This says nothing about WRITES -- see
        `test_a_full_run_works_with_every_perturbed_artifact_read_only`, which
        is the test that does.
        """
        data = mutate._read_real("data/rng_trace.json")
        self.assertEqual(hash_of(data),
                         mutate.digest(REPO / "data" / "rng_trace.json"))
        with self.assertRaises(mutate.GateError):
            mutate._read_real("/etc/passwd")


def hash_of(b):
    import hashlib
    return hashlib.sha256(b).hexdigest()


class ShippedManifestTest(unittest.TestCase):
    """The manifest this repo actually ships."""

    def setUp(self):
        self.spec = json.loads(Path(mutate.MANIFEST).read_text())

    def red(self):
        return [c for c in self.spec["cases"] if c.get("expect_red", True)]

    def findings(self):
        return [c for c in self.spec["cases"]
                if not c.get("expect_red", True)]

    def test_every_case_is_complete_and_names_a_real_artifact(self):
        seen = set()
        for c in self.spec["cases"]:
            keys = ["label", "defends", "artifact", "mutate", "test"]
            if c.get("expect_red", True):
                keys.append("expect")
            for key in keys:
                self.assertIn(key, c, c.get("label"))
                self.assertTrue(c[key], "%s: empty %s" % (c["label"], key))
            self.assertNotIn(c["label"], seen)
            seen.add(c["label"])
            self.assertTrue((REPO / c["artifact"]).is_file(),
                            "%s: no such artifact %s"
                            % (c["label"], c["artifact"]))

    def test_the_findings_are_registered_rather_than_dropped(self):
        """A column that would not go red is a finding, kept in the manifest.

        Dropping it would leave the gate reporting only its successes, which
        is the failure mode the whole tool exists against.  Each finding must
        say so in its own text, and must run a WHOLE suite rather than one
        filtered test -- "nothing noticed" is only a claim worth making when
        everything had the chance to.

        "Whole suite" is checked without pinning the command to `cargo`: an
        earlier version required exactly `cargo test --test <name>`, which made
        a Python-side unmutable column impossible to REGISTER at all, so the
        gap it should have recorded would have gone unrecorded instead.
        """
        self.assertTrue(self.findings(), "no findings recorded at all")
        for c in self.findings():
            self.assertIn("FINDING", c["defends"], c["label"])
            self.assertNotIn("expect", c,
                             "%s: a finding has no expected failure message"
                             % c["label"])
            self.assertFalse(is_filtered(c["test"]),
                             "%s: %s names a single test, so 'nothing asserts "
                             "this column' is not established -- a finding must"
                             " run the whole suite"
                             % (c["label"], " ".join(c["test"])))

    def test_all_three_frozen_oracles_are_covered(self):
        """A gate that skipped an oracle would still print `all cases ok`."""
        artifacts = {c["artifact"] for c in self.red()}
        for name in mutate.FROZEN:
            self.assertIn(name, artifacts,
                          "%s has no mutation case, so no assertion over it "
                          "has been shown to be falsifiable" % name)

    def test_every_oracle_consuming_test_file_is_covered(self):
        """The four files the brief scopes the gate to, by their commands."""
        cmds = " ".join(" ".join(c["test"]) for c in self.red())
        for target in ("wander_sequence", "combat_sequence", "combat_vectors",
                       "test_rngtrace.py"):
            self.assertIn(target, cmds,
                          "no case runs %s" % target)

    def test_no_two_cases_defend_the_same_assertion(self):
        """Several mutations reddening one assertion is noise, not coverage."""
        tests = [tuple(c["test"]) for c in self.red()]
        dupes = {t for t in tests if tests.count(t) > 1}
        # `run_a_replays_exactly` on the combat oracle legitimately appears
        # twice: it is the entry point for FOUR channels, and the draw-stream
        # case and the break-flag case fail on different assertions inside it.
        for t in dupes:
            expects = sorted(c["expect"] for c in self.red()
                             if tuple(c["test"]) == t)
            self.assertEqual(len(set(expects)), len(expects),
                             "two cases expect the same failure from %s" % (t,))

    def test_the_whole_shipped_gate_passes(self):
        """Run it.  Every registered channel must actually go red."""
        import io
        out = io.StringIO()
        rc = mutate.run_manifest(out=out)
        self.assertEqual(rc, 0, out.getvalue())
        self.assertIn("all %d case(s) went red" % len(self.red()),
                      out.getvalue())
        self.assertIn("%d further column(s) confirmed still asserted by "
                      "nothing" % len(self.findings()), out.getvalue())


class FindingCommandShapeTest(unittest.TestCase):
    """`is_filtered` decides whether a finding's silence claim is admissible.

    A finding says "nothing asserts this column", and that is only established
    if the command it ran gave EVERYTHING the chance to notice.  So the
    predicate that separates a whole-suite run from a single-test run is load
    bearing, and until this class existed its only evidence was a probe in a
    report -- which, by the rule this task added to `METHODOLOGY.md`, is not
    evidence at all.

    The error direction matters and is asserted by the table: a whole-suite
    command misread as filtered REJECTS a finding loudly; a filtered command
    misread as whole-suite ADMITS a bogus one silently.  Only the second is
    dangerous, so every ambiguous shape must come out `True`.
    """

    CASES = [
        # (command, filtered?, why)
        (["cargo", "test", "--test", "combat_sequence"], False,
         "the whole binary -- the shape every shipped finding uses"),
        (["cargo", "test", "--test", "combat_sequence", "run_a_replays_exactly"],
         True, "--test names the binary, the trailing bare arg is a filter"),
        (["cargo", "test"], False, "every binary, every test"),
        (["cargo", "test", "run_a_replays_exactly"], True,
         "every binary but only MATCHING tests -- a filter with no --test"),
        (["cargo", "test", "--release"], False,
         "a flag that takes no value is not a filter"),
        (["cargo", "test", "--release", "run_a_replays_exactly"], True,
         "a filter after a valueless flag is still a filter"),
        (["python3", "tools/test_rngtrace.py"], False, "the whole module"),
        (["python3", "tools/test_rngtrace.py", "FightFoldTest.test_x"], True,
         "unittest takes its filter the same way"),
    ]

    def test_the_two_runners_are_classified_correctly(self):
        for cmd, want, why in self.CASES:
            with self.subTest(cmd=" ".join(cmd)):
                self.assertEqual(is_filtered(cmd), want,
                                 "%s: %s" % (" ".join(cmd), why))

    def test_a_filtered_command_is_rejected_by_the_manifest_check(self):
        """The predicate is WIRED to the gate, not merely correct in isolation.

        A finding registered as `cargo test <filter>` must be refused, so the
        manifest check is run here against a manifest holding exactly that.
        """
        bogus = {"label": "bogus-finding", "expect_red": False,
                 "defends": "FINDING: nothing reads this",
                 "artifact": "data/rng_trace.json", "mutate": DRAW_MUTATION,
                 "test": ["cargo", "test", "run_a_replays_exactly"]}
        case = ShippedManifestTest(
            "test_the_findings_are_registered_rather_than_dropped")
        result = unittest.TestResult()
        with tempfile.TemporaryDirectory() as td:
            with mock.patch.object(mutate, "MANIFEST",
                                   manifest_file(td, [bogus])):
                case.run(result)
        self.assertEqual(result.errors, [])
        self.assertEqual(len(result.failures), 1,
                         "a finding registered as `cargo test <filter>` was "
                         "accepted: its suite-wide silence claim was never "
                         "established")
        self.assertIn("must run the whole suite", result.failures[0][1])


class RecordedFindingTest(unittest.TestCase):
    """`expect_red: false` records a column nothing asserts -- loudly."""

    def finding(self, **kw):
        base = {"label": "recorded", "expect_red": False,
                "defends": "FINDING: nothing reads this",
                "artifact": "data/rng_trace.json", "mutate": DRAW_MUTATION,
                "test": ["python3", "-c", "raise SystemExit(0)"]}
        base.update(kw)
        return base

    def test_a_column_nothing_asserts_is_recorded_not_counted_as_coverage(self):
        rc, report = gate([self.finding()])
        self.assertEqual(rc, 0, report)
        self.assertIn("green -> green", report)
        self.assertIn("(asserted by nothing)", report)
        self.assertIn("all 0 case(s) went red", report)
        self.assertIn("1 further column(s) confirmed still asserted by "
                      "nothing -- findings, not coverage", report)

    def test_a_finding_that_starts_going_red_fails_the_gate(self):
        """The record rots loudly: coverage arriving is a reason to promote."""
        rc, report = gate([self.finding(test=HONEST_TEST)])
        self.assertEqual(rc, 1, report)
        self.assertIn("FAIL   recorded", report)
        self.assertIn("NOW goes red", report)
        self.assertIn("move this case to expect_red", report)


class SafetyPropertyTest(unittest.TestCase):
    """After a full run: the real files are byte-identical, and git agrees."""

    def git(self, *args):
        return subprocess.run(("git",) + args, cwd=str(REPO), text=True,
                              capture_output=True).stdout

    def test_a_full_run_changes_no_real_file_and_no_tracked_file(self):
        dirty = self.git("status", "--porcelain", "--", "data", "orig")
        self.assertEqual(dirty, "",
                         "data/ or orig/ is already modified in the working "
                         "tree, so this test cannot speak for the gate:\n"
                         + dirty)
        before = mutate.guarded_digests()
        self.assertEqual(mutate.main([]), 0)
        self.assertEqual(mutate.guarded_digests(), before)
        self.assertEqual(self.git("status", "--porcelain", "--",
                                  "data", "orig"), "")
        # And the three digests the brief pins, spelled out.
        for name, want in (
                ("data/rng_trace.json",
                 "148fe3c74ba7727754b9e14f7b24f25eac4cf1cc97ab6930bebc5496"
                 "25eb1025"),
                ("data/state_trace.json",
                 "6f7ae78aa6af0e4ff31d4e67c53ff3fe216913980238656a2f236f81"
                 "fa9613c7"),
                ("data/combat_trace.json",
                 "8c4b80e6162edd3df40e16273ba74de23a8b29efebbc83fd141c89b9"
                 "ee180acb")):
            self.assertEqual(before[name], want, name)

    def test_a_run_that_crashes_mid_case_still_leaves_the_repo_alone(self):
        before = mutate.guarded_digests()
        rc, report = gate([{
            "label": "explodes",
            "defends": "",
            "artifact": "data/rng_trace.json",
            "mutate": {"op": "json-set", "path": ["runs", 0, "nope"],
                       "from": 1, "to": 2},
            "test": HONEST_TEST,
            "expect": "x",
        }])
        self.assertEqual(rc, 2, report)
        self.assertEqual(mutate.guarded_digests(), before)

    def test_the_safety_block_is_printed_on_the_abort_path_too(self):
        """The digest verdict must survive a `GateError`, not only a clean run.

        This test computing the digests itself -- as
        `test_a_run_that_crashes_mid_case_still_leaves_the_repo_alone` above
        does -- checks the property while saying nothing about whether the GATE
        checked it.  The operator of an aborted run sees only what the gate
        prints, and "not on crash" is the case the brief singles out.  So this
        asserts the REPORT, not the repo.
        """
        rc, report = gate([{
            "label": "explodes",
            "defends": "",
            "artifact": "data/rng_trace.json",
            "mutate": {"op": "json-set", "path": ["runs", 0, "nope"],
                       "from": 1, "to": 2},
            "test": HONEST_TEST,
            "expect": "x",
        }])
        self.assertEqual(rc, 2, report)
        for name in mutate.FROZEN:
            self.assertIn(name, report)
            self.assertIn(mutate.digest(REPO / name), report)
        self.assertIn("real file(s) under", report)
        self.assertIn("unchanged", report)

    def test_a_vanished_oracle_prints_the_verdict_not_a_KeyError(self):
        """The verdict must survive the very scenario it exists to report.

        `_report_safety` printed the three frozen digests BEFORE computing what
        changed, so a `FROZEN` oracle missing from the after-snapshot raised a
        `KeyError` -- and since the call moved into the `finally`, that
        `KeyError` replaced the in-flight `GateError`, turning a caught exit 2
        into an uncaught traceback.  A file under `data/` disappearing mid-run
        is precisely what the backstop is for.
        """
        real = mutate.guarded_digests
        seen = []

        def vanishing(root=mutate.REPO):
            d = real(root)
            seen.append(1)
            if len(seen) > 1:                    # the AFTER snapshot
                d.pop(mutate.FROZEN[0])
            return d

        with mock.patch.object(mutate, "guarded_digests", vanishing):
            rc, report = gate([{
                "label": "explodes", "defends": "",
                "artifact": "data/rng_trace.json",
                "mutate": {"op": "json-set", "path": ["runs", 0, "nope"],
                           "from": 1, "to": 2},
                "test": HONEST_TEST, "expect": "x"}])
        self.assertEqual(rc, 2, report)
        self.assertIn("SAFETY FAILURE", report)
        self.assertIn(mutate.FROZEN[0], report)
        # ...and the abort that was in flight is still the one reported.
        self.assertIn("GateError: explodes", report)

    def test_a_full_run_works_with_every_perturbed_artifact_read_only(self):
        """Not "nothing changed" -- "nothing could have".

        The digest comparison proves no write LANDED.  It cannot distinguish
        that from a write that landed and was undone.  Stripping the write bit
        closes the difference: any `open(..., "w")` on one of these raises
        `PermissionError`, so a gate that still exits 0 never attempted one.

        Every artifact the shipped manifest names, not only the three frozen
        oracles -- otherwise the claim would skip `data/combat_vectors.json`
        and `tools/rngtrace/combattrace.py`, and the second of those is the one
        file a case actually PATCHES.
        """
        named = sorted({c["artifact"] for c in json.loads(
            Path(mutate.MANIFEST).read_text())["cases"]})
        self.assertEqual(set(mutate.FROZEN) - set(named), set(),
                         "a frozen oracle is not named by any case")
        targets = [REPO / n for n in named]
        modes = [p.stat().st_mode for p in targets]
        try:
            for p in targets:
                os.chmod(p, 0o444)
            self.assertEqual(mutate.main([]), 0)
        finally:
            for p, m in zip(targets, modes):
                os.chmod(p, m)
        self.assertEqual([p.stat().st_mode for p in targets], modes)

    def test_the_shadow_tree_is_deleted_afterwards(self):
        import io
        out = io.StringIO()
        with tempfile.TemporaryDirectory() as td:
            mutate.run_manifest(manifest_file(td, [{
                "label": "honest", "defends": "",
                "artifact": "data/rng_trace.json", "mutate": DRAW_MUTATION,
                "test": HONEST_TEST, "expect": HONEST_EXPECT}]),
                out=out)
        shadow = [w for w in out.getvalue().split() if "gopnik-mutate-" in w]
        self.assertEqual(len(shadow), 1, out.getvalue())
        self.assertFalse(os.path.exists(shadow[0]), shadow[0])


if __name__ == "__main__":
    unittest.main(verbosity=2)
