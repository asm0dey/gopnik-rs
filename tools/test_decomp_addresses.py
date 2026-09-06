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
        """Five mismatches stand, each named in the report for this task.

        Two are in a PLAN document -- `src/game.rs:2380` for
        `Game::show_command_list` (now 2608) and `src/game.rs:3575` for
        `Game::smoke` (now 3878) -- quoted as they stood when the plan was
        written, and a plan is not retro-edited.

        Three are the ENCLOSING-SCOPE form (`` `Game` in `src/game.rs:479` ``),
        where the bound symbol names the struct or function the cited line sits
        inside rather than text on the line, so a "the symbol is at that line"
        checker cannot confirm them either way. Two of those three had their
        line numbers corrected by this task anyway, by hand, from `:438`/`:387`
        to the actual `38c2` declaration and load site.

        All five are recorded rather than special-cased: an exemption rule
        wide enough to hide them would hide the next real one too.
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
        self.assertEqual((present, wanted), (345, 374))

    def test_the_market_is_covered_whole(self):
        """109/109. One range with NO hole rules out a systematic miss --
        if the annotation dropped every conditional jump, the market would
        show the same 12-per-hundred hole the den does."""
        self.assertEqual(self.cov["market"]["missing"], [])

    def test_no_branch_guard_pair_loses_both_halves(self):
        """The 29 misses are one half of a folded compare-and-branch.

        Ghidra attributes the CBRANCH's p-code to the compare's address when
        the two are adjacent, so e.g. `1000:cee7 CMP byte [0x38b4],0x0` is
        annotated and its `1000:ceec JNZ` is not. That is a fold, not a loss:
        every pair keeps at least one address, and a pair that lost both would
        be a whole conditional missing from the annotation.
        """
        self.assertEqual(da.orphaned_pairs(self.cov), [])
        self.assertEqual(
            GOLDEN["branch_guard_pairs_with_neither_half_annotated"], [])

    def test_every_missing_address_has_an_annotated_neighbour(self):
        """...within 10 bytes, which is what "folded onto the compare" means.

        Stated as a property rather than as prose so it can fail: an address
        missing because the annotation lost a whole REGION would have no
        annotated neighbour, and would not be explained by the fold.
        """
        import addr as addrmod
        per_file, _ = da.annotations()
        have = {addrmod.image_off_of_citation(t.text)
                for t in per_file[da.HANDLER_FILE]}
        for name, rec in sorted(self.cov.items()):
            for cit in rec["missing"]:
                off = addrmod.image_off_of_citation(cit)
                with self.subTest(handler=name, address=cit):
                    self.assertTrue(
                        any(abs(h - off) <= 10 for h in have),
                        "%s has no annotated address within 10 bytes" % cit)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
