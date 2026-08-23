#!/usr/bin/env python3
"""Tests for `tools/difftest.py`.

Three things are checked, in this order of importance:

1. **The comparison can fail.**  Thirteen mutations of the load image, at
   least one per quantity the tool claims to cover, each have to move the
   reference stream or raise; a fourteenth check drops a record instead of
   changing one.  That is the whole of `TheComparisonCanFail`, whose method
   count is 14.
   A comparator nobody has ever seen fail is not evidence, and this
   project has shipped one before (`docs/re/METHODOLOGY.md`, "a completeness
   check printing 14/14 by formatting one value against itself").  `orig/g.exe`
   is never touched: every mutation is applied to a copy of the load image in
   memory.
2. **The port agrees with the original** on every record.
3. **The enumerations are what the tool says they are** -- 18 byte-priced rows,
   9 immediate-priced rows, 27 debits paired one-to-one with them, 11 weight
   rows, 15 items.

The DOSBox-X screen channel is exercised too when `dosbox-x` is installed;
when it is not, that test SKIPs with a message rather than passing quietly.

    python3 tools/test_difftest.py
"""
import pathlib
import shutil
import subprocess
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))

import addr  # noqa: E402
import difftest  # noqa: E402


IMG = difftest.load()
REFERENCE, EVIDENCE = difftest.reference(IMG)


def mutate(image_off, value):
    """A copy of the load image with one byte changed."""
    m = bytearray(IMG)
    m[image_off] = value
    return bytes(m)


class ReferenceEnumerations(unittest.TestCase):
    """The counts the tool reports, checked against the scans that produce them."""

    def test_priced_row_scans_find_18_and_9(self):
        self.assertEqual(EVIDENCE["priced_row_byte"], 18)
        self.assertEqual(EVIDENCE["priced_row_imm"], 9)
        self.assertEqual(EVIDENCE["priced_row"], 27)

    def test_every_priced_row_lands_in_a_named_handler(self):
        rows = [l for l in REFERENCE if l.startswith("priced_row ")]
        per_shop = {}
        for line in rows:
            per_shop.setdefault(line.split()[1], []).append(line.split()[2])
        self.assertEqual(
            per_shop,
            {
                "mar": list("123456789"),
                "bmar": list("123456789"),
                "rep": ["h", "r"],
                "kl": ["1", "2"],
                "trn": list("12345"),
            },
        )

    def test_menu_order_is_the_literal_numbering_each_menu_prints(self):
        # Deliberately NOT re-derived from the `priced_row` lines: both sides
        # build `menu_order` by walking the same list they emitted the rows
        # from, so comparing the two is a list against itself and cannot fail
        # except on a field-splitting bug.  The expected orders are written
        # out, so a reordered scan or a dropped row moves this test.
        got = {}
        for line in REFERENCE:
            if line.startswith("menu_order "):
                _, shop, order = line.split()
                got[shop] = order
        self.assertEqual(
            got,
            {
                "mar": "1,2,3,4,5,6,7,8,9",
                "bmar": "1,2,3,4,5,6,7,8,9",
                "rep": "h,r",
                "kl": "1,2",
                "trn": "1,2,3,4,5",
            },
        )

    def test_every_debit_pairs_with_exactly_one_row(self):
        self.assertEqual(EVIDENCE["debit_rows"], 27)
        self.assertEqual(EVIDENCE["debit_mismatch"], [])
        self.assertEqual(EVIDENCE["debit_unmatched"], [])

    def test_the_one_quote_gap_is_bmar_row_9(self):
        self.assertEqual(EVIDENCE["quote_gap"], [("bmar", "9", 70, 60)])

    def test_weight_table_has_eleven_rows_bounded_by_the_name_table(self):
        rows = [l for l in REFERENCE if l.startswith("class_weights ")]
        self.assertEqual(len(rows), 11)
        self.assertEqual(EVIDENCE["class_weights"], 11)

    def test_fifteen_items_carry_a_bonus(self):
        self.assertEqual(EVIDENCE["item"], 15)

    def test_ten_level_up_gains_one_of_them_conditional(self):
        gains = [l for l in REFERENCE if l.startswith("levelup_gain ")]
        self.assertEqual(len(gains), 10)
        self.assertEqual(
            [g for g in gains if g.endswith(" conditional")],
            ["levelup_gain strength dmg_min 1 conditional"],
        )

    def test_trn_row_3_fill_equals_its_price(self):
        self.assertEqual(EVIDENCE["trn3_fill"], 10)

    def test_the_nine_immediate_row_sites_are_emitted(self):
        sites = [l for l in REFERENCE if l.startswith("imm_row_site ")]
        self.assertEqual(EVIDENCE["imm_row_site"], 9)
        self.assertEqual(len(sites), 9)
        self.assertEqual(
            sites[2:4], ["imm_row_site kl 1 1000:df6f", "imm_row_site kl 2 1000:dfcb"]
        )

    def test_no_colour_markup_survives_into_the_reference(self):
        for line in REFERENCE:
            self.assertNotIn("^", line, line)


class TheComparisonCanFail(unittest.TestCase):
    """One image mutation per covered quantity; each must be visible."""

    def assert_moves(self, image_off, value, expect_in):
        got, _ = difftest.reference(mutate(image_off, value))
        changed = [b for a, b in zip(REFERENCE, got) if a != b]
        self.assertTrue(changed, "mutation at %#x changed nothing" % image_off)
        self.assertTrue(
            any(expect_in in c for c in changed),
            "mutation at %#x changed %s, none matching %r"
            % (image_off, changed[:3], expect_in),
        )
        ok, report, _ = difftest.compare(REFERENCE, got)
        self.assertFalse(ok, "compare() called a mutated stream a match")
        self.assertTrue(report)

    def test_a_byte_priced_row(self):
        self.assert_moves(addr.image_off_of_data_off(0x0B2E), 99, "priced_row mar 1 99")

    def test_a_displayed_price(self):
        # bmar row 9 reads 20ae:0b3f for display and 20ae:0b40 for the charge,
        # so moving 0b3f must move row 9's displayed number and leave its
        # charged one alone.  (It moves both of row 8's, which reads 0b3f for
        # each -- that is why the assertion names row 9.)
        self.assert_moves(
            addr.image_off_of_data_off(0x0B3F), 99, "priced_row bmar 9 60 99"
        )

    def test_an_immediate_priced_row(self):
        # `kl` row 1's price is at 1000:df6f+4.  Changing it alone contradicts
        # the digits in the row's own text, which the tool refuses rather than
        # reports -- the contradiction is itself the failure.
        with self.assertRaises(difftest.DifftestError):
            difftest.reference(mutate(0xDF6F + 4, 99))

    def test_the_threshold_step(self):
        self.assert_moves(0x2550 + 4, 7, "scalar threshold_step 7")

    def test_the_threshold_base(self):
        self.assert_moves(0x6DE0 + 4, 5, "scalar threshold_base 5")

    def test_the_level_cap(self):
        self.assert_moves(0x2580 + 4, 50, "scalar max_level 50")

    def test_the_gains_per_level(self):
        self.assert_moves(0x287D + 3, 3, "scalar gains_per_level 3")

    def test_a_class_weight(self):
        self.assert_moves(
            addr.image_off_of_data_off(2) + 16, 9, "class_weights 4 9 2 4 1"
        )

    def test_a_creation_stat(self):
        self.assert_moves(0x7148 + 4, 9, "start_stats 1 4 9 2 4 1")

    def test_the_class_offset(self):
        self.assert_moves(0x71B8 + 4, 5, "start_stats 0 5 3 3 3 3")

    def test_a_level_up_delta(self):
        self.assert_moves(0x27C3 + 4, 6, "levelup_gain vitality hpmax 6 always")

    def test_an_item_bonus(self):
        # The `9` of `^1Тесак(Урон+9) `, file 0x3173 / image 0x18a3.  Item
        # bonuses live in the item's own inventory string, so the mutation
        # has to be to that string, not to a table.
        self.assert_moves(0x18B1, ord("7"), "item 7 Тесак")

    def test_a_stale_citation_is_refused_rather_than_read(self):
        # Blank the `mov word [20ae:38d0],0xa` this file quotes; the byte check
        # in `site()` must reject it instead of reading whatever is there.
        with self.assertRaises(difftest.DifftestError):
            difftest.reference(mutate(0x6DE0, 0x90))

    def test_compare_reports_a_missing_record(self):
        ok, report, counts = difftest.compare(REFERENCE, REFERENCE[:-1])
        self.assertFalse(ok)
        self.assertTrue(any("record count" in line for line in report))


class ThePortAgrees(unittest.TestCase):
    def test_the_port_stream_equals_the_reference(self):
        port = difftest.port_stream()
        ok, report, counts = difftest.compare(REFERENCE, port)
        self.assertTrue(ok, "\n".join(report))
        self.assertEqual(len(port), len(REFERENCE))
        for kind, (a, b) in counts.items():
            self.assertEqual(a, b, kind)

    def test_an_unknown_flag_is_refused(self):
        r = subprocess.run(
            [str(difftest.port_binary()), "--trace-determinstic"],
            capture_output=True,
            text=True,
            timeout=60,
        )
        self.assertNotEqual(r.returncode, 0)
        self.assertEqual(r.stdout, "")


class TheScripts(unittest.TestCase):
    def test_at_least_five_scripts_exist(self):
        scripts = sorted(difftest.SCRIPTS.glob("*.txt"))
        self.assertGreaterEqual(len(scripts), 5, [s.name for s in scripts])

    def test_every_script_is_short_enough_for_the_capture_hook(self):
        # tools/oracle/capture.py refuses a script longer than the resident
        # 1024-key buffer; catching that here beats catching it mid-capture.
        for s in sorted(difftest.SCRIPTS.glob("*.txt")):
            self.assertLessEqual(len(s.read_text(encoding="utf-8")), 1024, s.name)

    def test_every_script_has_a_reader(self):
        for s in sorted(difftest.SCRIPTS.glob("*.txt")):
            self.assertTrue(
                s.stem == "market_rows_district1" or s.stem.startswith("stats_class"),
                "%s has no reader in difftest.oracle_channel" % s.name,
            )


class TheScreenChannel(unittest.TestCase):
    def test_the_original_prints_what_the_image_says(self):
        if shutil.which("dosbox-x") is None:
            self.skipTest(
                "dosbox-x is not installed; the screen channel cannot run here. "
                "The image-vs-port comparison above is unaffected."
            )
        scratch = pathlib.Path("/tmp/difftest-oracle")
        ok, report, acc = difftest.oracle_channel(REFERENCE, scratch)
        self.assertTrue(ok, "\n".join(report))
        self.assertEqual(acc["mar_rows"], (5, 9))
        self.assertEqual(acc["start_stats"], (4, 4))
        self.assertEqual(acc["threshold_sightings"], 4)
        self.assertEqual(acc["confirmed"], 13)


if __name__ == "__main__":
    unittest.main(verbosity=1)
