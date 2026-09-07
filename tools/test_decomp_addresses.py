"""Falsify the address annotation, and the `src/` line numbers the docs cite.

`tools/ghidra/ExportAll.java` now writes, per decompiled line, the SET of
machine addresses whose p-code contributed to it.  Everything downstream will
trust those addresses -- that is the point of the change and also its danger.
`docs/re/METHODOLOGY.md`: *an assertion over a captured oracle is not evidence
until it has been observed FAILING*.  So this suite:

* re-decodes every annotated address out of `orig/g.exe` and requires an
  ALIGNED INSTRUCTION START, anchored at an exported function entry -- not a
  well-formed `SEG:OFF`, which `1000:ce99` also is and is still the interior of
  `1000:ce97`;
* takes the six handlers whose branches were walked and cited BY HAND -- the
  market, the dealers' sell path, the vet, the den, the club and the gym -- and
  checks each branch and guard address `data/branches.json` records in range
  against the annotation of the enclosing function's file;
* pins the whole result in `tools/decomp_addresses_golden.json` so later drift
  fails.  There is deliberately NO percentage threshold: a threshold chosen so
  it passes today is the same defect one level up.

The current figures, and the command that produced them:

    $ PYTHONPATH=tools python3 tools/decomp_addresses.py all

    annotation: files=123 tokens=12072 distinct=9220
      malformed tokens: 0
      not aligned instruction starts: 1
        2000:f88f -- image offset 0x1f88f is past the end of the load image
      TOTAL 345/374 = 92.25%

`build/decomp/` is gitignored scratch, so the annotation half skips when it is
absent.  The `src/`-citation half does not: it reads only committed files.
"""

import json
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "tools"))

import decomp_addresses as da  # noqa: E402


GOLDEN = json.loads(da.GOLDEN.read_text())


def _decomp_present():
    return da.DECOMP_DIR.is_dir() and any(da.DECOMP_DIR.glob("*.c"))


class TestTheCheckerItself(unittest.TestCase):
    """Each channel below is shown rejecting something before it is trusted."""

    def test_a_mid_instruction_address_is_rejected(self):
        """`1000:ce99` is the last byte of `1000:ce97 mov [0x38c9],ax`.

        It parses, it resolves, it is inside a real function -- and it is not
        an instruction start.  Without this the alignment check would be a
        well-formedness check wearing its name.
        """
        image = da.Image()
        ok, why = image.classify("1000:ce99")
        self.assertFalse(ok)
        self.assertIn("not an instruction start", why)
        # ... and the instruction it is the interior of DOES pass, so the
        # rejection is about alignment and not about the neighbourhood.
        self.assertEqual(image.classify("1000:ce97"), (True, None))

    def test_an_address_past_the_image_is_rejected(self):
        ok, why = da.Image().classify("2000:ffff")
        self.assertFalse(ok)
        self.assertIn("past the end of the load image", why)

    def test_a_malformed_annotation_token_is_reported_not_dropped(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "x.c"
            p.write_text("  foo();   // @ 1000:ab59 not-an-address 1000:ab5c\n")
            toks, bad = da.parse_file(p)
            self.assertEqual([t.text for t in toks], ["1000:ab59", "1000:ab5c"])
            self.assertEqual([b.text for b in bad], ["not-an-address"])

    def test_the_src_citation_parser_reports_each_of_its_four_verdicts(self):
        """verified / mismatch / unbound are all reachable on synthetic input.

        A parser whose `mismatch` arm is unreachable would report a clean
        docs/ tree forever.
        """
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "src").mkdir()
            (root / "src" / "f.rs").write_text("one\ntwo\nthree\n")
            docs = root / "docs"
            docs.mkdir()
            (docs / "a.md").write_text(
                "good `src/f.rs:2` `two` here.\n"
                "bad `src/f.rs:3` `two` here.\n"
                "lonely `src/f.rs:1` sits with prose only.\n"
                "gone `src/missing.rs:1` `two` here.\n"
            )
            rep = da.src_citations(docs, root)
        self.assertEqual(len(rep["verified"]), 1, rep)
        self.assertEqual(len(rep["mismatch"]), 1, rep)
        self.assertEqual(len(rep["unbound"]), 1, rep)
        self.assertEqual(len(rep["unreadable"]), 1, rep)

    def test_a_comma_line_list_is_not_read_as_a_range(self):
        self.assertEqual(da._linespec_rows("202,210"), [202, 210])
        self.assertEqual(da._linespec_rows("173-175"), [173, 174, 175])
        self.assertEqual(da._linespec_rows("61"), [61])

    def test_a_continuation_binds_to_the_last_file_cited_of_any_kind(self):
        """`` `:941` `` after `` `tests/w.rs:484` `` is NOT a `src/` citation.

        Tracking only `src/` paths bound `docs/re/difftest.md`'s `:941` to a
        `src/` file three paragraphs earlier and reported a line number that
        file does not have -- a wrong finding, which is worse than none.
        """
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "src").mkdir()
            (root / "src" / "f.rs").write_text("one\ntwo\n")
            docs = root / "docs"
            docs.mkdir()
            (docs / "a.md").write_text(
                "`src/f.rs:1` `one` then `tests/w.rs:484` and `:941`.\n")
            rep = da.src_citations(docs, root)
        self.assertEqual(len(rep["verified"]), 1, rep)
        self.assertEqual(rep["unreadable"], [], rep)
        self.assertEqual(rep["unbound"], [], rep)


class TestSrcCitations(unittest.TestCase):
    """The `` `src/f.rs:NNN` `symbol` `` pairs docs/ already writes."""

    def setUp(self):
        self.report = da.src_citations()
        self.golden = GOLDEN["src_citations"]

    def test_every_citation_lands_in_exactly_one_bucket(self):
        """No citation is silently dropped between parse and report."""
        found = sum(len(v) for v in self.report.values())
        expected = sum(len(v) for v in self.golden.values())
        self.assertEqual(found, expected,
                         "docs/ gained or lost a `src/f.rs:NNN` citation; "
                         "regenerate the golden and read the diff")

    def test_the_verified_pairs_still_verify(self):
        self.assertEqual(self.report["verified"], self.golden["verified"])

    def test_the_failing_pairs_are_exactly_the_recorded_ones(self):
        """Six mismatches stand, each named in the report for this task.

        Three are in a PLAN document -- `src/game.rs:2380` for
        `Game::show_command_list` (now 2608) and `src/game.rs:3575` for
        `Game::smoke` (now 3878) in the service-handlers plan, and
        `src/game.rs:6425` for `Game::crowd` in the combat-opener plan --
        quoted as they stood when the plan was written, and a plan is not
        retro-edited. The third VERIFIED at Task 39 and moved here at Task 40,
        which added twenty citation comments inside `Game::crowd` and shifted
        it down the file: the drift is the point, and it is why
        `docs/re/METHODOLOGY.md` forbids a `src/` line number in a claim that
        has to stay true.

        Three are the ENCLOSING-SCOPE form (`` `Game` in `src/game.rs:479` ``),
        where the bound symbol names the struct or function the cited line sits
        inside rather than text on the line, so a "the symbol is at that line"
        checker cannot confirm them either way. Two of those three had their
        line numbers corrected by this task anyway, by hand, from `:438`/`:387`
        to the actual `38c2` declaration and load site.

        All six are recorded rather than special-cased: an exemption rule
        wide enough to hide them would hide the next real one too.

        **The rule this keeps re-teaching: a document that quotes a `src/`
        line number must add its golden row in the SAME commit.** The plan
        commit `c8ab775` quoted `src/game.rs:6425` and added no row, which
        left `main` red -- `python3 -m unittest discover -s tools -p
        'test_decomp*.py'` gives 2 failures there -- until `7e86c2e` repaired
        it. Then Task 40 moved `Game::crowd` down the file and the same
        citation became the sixth mismatch above. Both rounds were avoidable
        by the same one-line habit, and `docs/re/METHODOLOGY.md`'s "a port
        citation cites the command, not the line it printed" is the better
        habit still: prefer a `grep -n` a reader can re-run to a number that
        is stale by construction.
        """
        self.assertEqual(self.report["mismatch"], self.golden["mismatch"])

    def test_unparseable_pairs_are_reported_not_skipped(self):
        self.assertEqual(self.report["unbound"], self.golden["unbound"])
        self.assertEqual(self.report["unreadable"], self.golden["unreadable"])


@unittest.skipUnless(_decomp_present(),
                     "build/decomp/ is gitignored scratch; run "
                     "`bash tools/ghidra/run_ghidra.sh --decomp-only` first")
class TestAnnotation(unittest.TestCase):

    @classmethod
    def setUpClass(cls):
        cls.report = da.alignment_report()
        cls.golden = GOLDEN["annotation"]

    def test_the_parser_understood_every_token_it_found(self):
        self.assertEqual(self.report["malformed"], self.golden["malformed"])
        self.assertEqual(self.report["malformed"], [])

    def test_the_shape_of_the_export_has_not_moved(self):
        for key in ("files", "annotated_tokens", "distinct_addresses"):
            self.assertEqual(self.report[key], self.golden[key], key)

    def test_the_annotation_is_a_set_of_addresses_not_a_minimum(self):
        """1390 lines carry more than one address.

        `getMinAddress()` alone would make this zero while leaving every other
        assertion in this file green -- which is exactly why it is asserted
        separately from the alignment sweep.
        """
        n = da.multi_address_lines()
        self.assertEqual(n, self.golden["lines_carrying_more_than_one_address"])
        self.assertGreater(n, 0)

    def test_every_annotated_address_is_an_aligned_instruction_start(self):
        """...except the one Ghidra emits for its own decode failures.

        `2000:f88f` is where the decompiler gave up ("Could not follow
        disassembly flow into non-existing memory"); its image offset 0x1f88f
        is past the end of the 0x14180-byte load image, so it is not an address
        in this program at all. It is recorded with that reason rather than
        filtered, because a filter would also swallow a real miss.
        """
        self.assertEqual(self.report["not_instruction_starts"],
                         self.golden["not_instruction_starts"])
        self.assertEqual(self.report["not_instruction_start_sites"],
                         self.golden["not_instruction_start_sites"])

    def test_the_only_recorded_exception_is_outside_the_image(self):
        """The exception list cannot quietly grow a MISALIGNED member.

        Pinning the dict alone would let a future misaligned address in as long
        as the golden were regenerated with it. This says what the exception
        class is allowed to be.
        """
        for cit, why in self.report["not_instruction_starts"].items():
            self.assertIn("past the end of the load image", why, cit)


@unittest.skipUnless(_decomp_present(), "build/decomp/ is gitignored scratch")
class TestHandlerCoverage(unittest.TestCase):
    """The six handlers walked and cited by hand are the ground truth."""

    @classmethod
    def setUpClass(cls):
        cls.cov = da.handler_coverage()
        cls.golden = GOLDEN["handlers"]

    def test_the_six_hand_mapped_ranges_are_all_checked(self):
        self.assertEqual(sorted(self.cov), sorted(self.golden))
        self.assertEqual(sorted(self.cov),
                         ["club", "den", "gym", "market", "sell", "vet"])

    def test_each_handler_matches_its_golden_coverage(self):
        for name in sorted(self.cov):
            with self.subTest(handler=name):
                got, want = self.cov[name], self.golden[name]
                self.assertEqual(got["wanted"], want["wanted"])
                self.assertEqual(got["present"], want["present"])
                self.assertEqual(got["missing"], want["missing"])

    def test_the_golden_totals_are_arithmetic_over_the_parts(self):
        """A hand-edited total cannot pass.

        `present + len(missing) == wanted` per handler, and the recorded total
        is the sum -- recomputed here from the report, never restated from the
        golden's own total.
        """
        wanted = present = 0
        for name, rec in sorted(self.cov.items()):
            self.assertEqual(rec["present"] + len(rec["missing"]), rec["wanted"],
                             name)
            wanted += rec["wanted"]
            present += rec["present"]
        self.assertEqual(wanted, GOLDEN["handler_total"]["wanted"])
        self.assertEqual(present, GOLDEN["handler_total"]["present"])
        # Deliberately a SECOND home for the number, hand-written, and it will
        # need hand-editing when the coverage genuinely moves. That is the
        # point: every other assertion here compares the report against a file
        # `--write-golden` rewrites, so a regeneration run that silently moved
        # the total would leave them all green. This literal is the one thing
        # in the suite that a regeneration cannot update, so it forces the
        # human to notice. The report's figure and this literal are the same
        # fact recorded in two places on purpose.
        self.assertEqual((present, wanted), (345, 374))

    def test_the_market_is_covered_whole(self):
        """109/109. One range with NO hole rules out a systematic miss --
        if the annotation dropped every conditional jump, the market would
        show the same 12-per-hundred hole the den does."""
        self.assertEqual(self.cov["market"]["missing"], [])

    def test_every_missing_address_has_its_partner_annotated(self):
        """This is what "the decompiler folded the pair" MEANS, as a property.

        The partner of a branch address is its guard's address and vice versa,
        from `data/branches.json`. A miss caused by the fold necessarily leaves
        its partner in the annotation; a miss caused by anything else -- a lost
        region, a dropped statement, an exporter bug -- does not have to. So an
        unannotated partner is the finding, and there are none: 29 misses, 29
        partners, all annotated, at |delta| <= 5.

        It replaces a proximity test that could not discriminate. That one
        asked whether a missing address had ANY annotated address within 10
        bytes, and over `1000:b94a`..`1000:e972` the largest gap between
        consecutive annotated offsets is 11 -- so all 12328 byte offsets in the
        span passed it, not just the 29. It would have fired only on a
        contiguous unannotated run of 21 bytes or more, which is a different
        failure from the one it was presented as testing.

        This assertion also SUBSUMES `orphaned_pairs`: a pair that lost both
        halves is a miss whose partner is itself missing, so it fails here
        first. `orphaned_pairs` is still checked below, in the pair's own
        vocabulary, but it is the weak form.
        """
        got = da.fold_partners(self.cov)
        # Property first, golden second, deliberately: the golden comparison
        # prints a 3800-character dict diff, which buries the one address that
        # actually broke. Asserting the property first makes the failure name
        # the address.
        for miss, rec in sorted(got.items()):
            with self.subTest(address=miss):
                self.assertNotEqual(rec["partners"], [],
                                    "%s has no branches.json partner at all, so "
                                    "the fold cannot explain it" % miss)
                self.assertEqual(rec["unannotated_partners"], [],
                                 "%s is missing AND so is its partner" % miss)
        self.maxDiff = None
        self.assertEqual(got, GOLDEN["fold_partners"])

    def test_the_fold_runs_in_both_directions(self):
        """25 misses are the jump; 4 are the compare. Not one mechanism.

        Ghidra usually attributes the CBRANCH's p-code to the compare's
        address, so the jump vanishes (`1000:cee7 CMP byte [0x38b4],0x0` kept,
        `1000:ceec JNZ` gone). Four go the other way -- the guard vanishes and
        the branch is kept: `1000:d93e cmp ax,0x28` / `1000:d941 jl 0xd95c`,
        and the same shape at `da9d`/`daa0`, `e58b`/`e58d`, `e892`/`e894`.
        The general claim (one half of an adjacent compare-and-branch survives)
        holds for all 29; the DIRECTION does not, and a reader who takes the
        usual direction as the rule will misread those four.
        """
        import addr as addrmod
        forward = backward = 0
        for miss, rec in sorted(GOLDEN["fold_partners"].items()):
            off = addrmod.image_off_of_citation(miss)
            partner = addrmod.image_off_of_citation(rec["partners"][0])
            if partner < off:
                forward += 1          # guard kept, jump missing
            else:
                backward += 1         # jump kept, guard missing
        self.assertEqual((forward, backward), (25, 4))

    def test_no_branch_guard_pair_loses_both_halves(self):
        """The weak form of the assertion above, kept for its vocabulary."""
        self.assertEqual(da.orphaned_pairs(self.cov), [])
        self.assertEqual(
            GOLDEN["branch_guard_pairs_with_neither_half_annotated"], [])


class TestTheCommittedFixture(unittest.TestCase):
    """Runs on a FRESH CLONE. `build/decomp/` does not have to exist.

    Without this, 11 of the suite's tests skip on a clone and the runner still
    prints OK -- and in the 597-test `unittest discover` run that skip count
    merges with unrelated ones, so nothing distinguishes "annotation checked"
    from "annotation absent". `tools/fixtures/decomp/` is three committed
    files from the same export, chosen to cover the interesting shapes:
    `FUN_1f78_114b` is Borland's `Random` (`docs/re/rng.md`), it has a line
    carrying three addresses, and `FUN_1f78_1111` carries the `2000:f88f`
    decode-failure sentinel.
    """

    @classmethod
    def setUpClass(cls):
        cls.report = da.alignment_report(da.FIXTURE_DIR)
        cls.golden = GOLDEN["fixture"]

    def test_the_fixture_exists_and_is_annotated(self):
        self.assertEqual(sorted(p.name for p in da.FIXTURE_DIR.glob("*.c")),
                         self.golden["files"])
        self.assertEqual(self.report["annotated_tokens"],
                         self.golden["annotated_tokens"])
        self.assertGreater(self.report["annotated_tokens"], 0)

    def test_the_parser_understood_every_fixture_token(self):
        self.assertEqual(self.report["malformed"], [])

    def test_every_fixture_address_is_an_aligned_instruction_start(self):
        """Same check as over `build/decomp/`, on committed bytes.

        `2000:f88f` is expected here too -- the fixture deliberately includes
        the file that carries the sentinel, so the exception path is exercised
        on a fresh clone rather than only where the gitignored tree exists.
        """
        self.assertEqual(self.report["not_instruction_starts"],
                         self.golden["not_instruction_starts"])
        for cit, why in self.report["not_instruction_starts"].items():
            self.assertIn("past the end of the load image", why, cit)

    def test_the_fixture_carries_a_multi_address_line(self):
        """`FUN_1f78_114b`'s return merges three instructions.

        A set, not a minimum -- asserted on committed bytes so the property
        cannot go unchecked on a clone.
        """
        self.assertGreater(da.multi_address_lines(da.FIXTURE_DIR), 0)

    @unittest.skipUnless(_decomp_present(), "build/decomp/ is gitignored scratch")
    def test_the_fixture_matches_the_live_export(self):
        """The fixture cannot silently rot behind a re-export.

        This one DOES skip on a clone, and that is correct: it is the only
        assertion here that needs the gitignored tree.
        """
        for name in self.golden["files"]:
            with self.subTest(file=name):
                self.assertEqual((da.FIXTURE_DIR / name).read_bytes(),
                                 (da.DECOMP_DIR / name).read_bytes(),
                                 "%s drifted from build/decomp/; re-copy it" % name)


class TestTheGoldenIsSelfConsistent(unittest.TestCase):
    """Runs on a FRESH CLONE, over the committed golden alone.

    Without it, `test_the_golden_totals_are_arithmetic_over_the_parts`
    recomputes from the live report and therefore skips with everything else,
    leaving NOTHING validating the committed file. These assertions read only
    `tools/decomp_addresses_golden.json`, so a hand-edited or half-regenerated
    golden is caught whether or not `build/decomp/` exists.
    """

    def test_every_site_key_has_a_reason(self):
        ann = GOLDEN["annotation"]
        self.assertEqual(sorted(ann["not_instruction_start_sites"]),
                         sorted(ann["not_instruction_starts"]))

    def test_the_recorded_exception_class_is_outside_the_image(self):
        for cit, why in GOLDEN["annotation"]["not_instruction_starts"].items():
            self.assertIn("past the end of the load image", why, cit)

    def test_each_handler_total_is_arithmetic_over_its_own_parts(self):
        for name, rec in sorted(GOLDEN["handlers"].items()):
            with self.subTest(handler=name):
                self.assertEqual(rec["present"] + len(rec["missing"]),
                                 rec["wanted"])

    def test_the_grand_total_is_the_sum_of_the_handlers(self):
        self.assertEqual(
            sum(r["wanted"] for r in GOLDEN["handlers"].values()),
            GOLDEN["handler_total"]["wanted"])
        self.assertEqual(
            sum(r["present"] for r in GOLDEN["handlers"].values()),
            GOLDEN["handler_total"]["present"])

    def test_the_fold_partner_map_covers_exactly_the_missing_addresses(self):
        missing = sorted(a for r in GOLDEN["handlers"].values()
                         for a in r["missing"])
        self.assertEqual(sorted(GOLDEN["fold_partners"]), missing)
        for miss, rec in sorted(GOLDEN["fold_partners"].items()):
            with self.subTest(address=miss):
                self.assertNotEqual(rec["partners"], [])
                self.assertEqual(rec["unannotated_partners"], [])

    def test_the_src_citation_buckets_are_disjoint(self):
        seen = set()
        for bucket, records in sorted(GOLDEN["src_citations"].items()):
            for r in records:
                self.assertNotIn(r, seen, "%s appears in two buckets" % r)
                seen.add(r)

    def test_the_fixture_is_recorded(self):
        self.assertNotEqual(GOLDEN["fixture"]["files"], [])
        self.assertGreater(GOLDEN["fixture"]["annotated_tokens"], 0)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
