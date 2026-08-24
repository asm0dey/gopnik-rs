#!/usr/bin/env python3
"""`tools/branch_reach.py` -- the number, and the two ways to get it wrong."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import branch_reach                                                  # noqa: E402
from decode_save import LAYOUT, RECORD_BASE, SIZE                    # noqa: E402


class WindowTest(unittest.TestCase):
    def test_the_record_window_is_the_record(self):
        lo, hi = branch_reach.WINDOWS["record"]
        self.assertEqual((lo, hi), (0x369C, 0x3952))
        self.assertEqual(hi - lo, SIZE)
        self.assertEqual(lo, RECORD_BASE)

    def test_the_wrong_window_is_the_right_one_shifted_by_0x200(self):
        """The defect it exists to reproduce, stated as arithmetic."""
        rec = branch_reach.WINDOWS["record"]
        bad = branch_reach.WINDOWS["stat-block-base"]
        self.assertEqual(bad[0] - rec[0], 0x200)
        self.assertEqual(bad[1] - rec[1], 0x200)
        # 0x200 is where the stat words sit INSIDE the record, which is why
        # the two are confusable at all.
        self.assertEqual(rec[0] + 0x200, 0x389C)


class OperandTest(unittest.TestCase):
    def test_only_bare_absolute_displacements_match(self):
        f = branch_reach.ABS_OPERAND.findall
        self.assertEqual(f("CMP byte ptr [0x38b0],0x0"), ["38b0"])
        self.assertEqual(f("CMP word ptr [0x389c],0x3"), ["389c"])
        # Stack and register addressing must NOT match: counting `[BP + 0x4]`
        # as DGROUP `0x0004` would be the same class of error as the window
        # shift, and there are 67 such operands among the game guards.
        for text in ("CMP byte ptr [BP + 0x4],0x0",
                     "CMP byte ptr [BP + -0x2],0x0",
                     "CMP byte ptr [BP + DI + 0x6],0x0",
                     "CMP byte ptr [DI + 0x38b0],0x0"):
            self.assertEqual(f(text), [], text)

    def test_a_guardless_branch_contributes_nothing(self):
        self.assertEqual(branch_reach.guard_displacements({}), [])
        self.assertEqual(branch_reach.guard_displacements({"guard": None}), [])


class CountTest(unittest.TestCase):
    def setUp(self):
        self.branches = branch_reach.branches()

    def test_the_committed_number(self):
        r = branch_reach.report()
        self.assertEqual(r["game_branches"], 838)
        self.assertEqual(r["reachable"], 331)
        self.assertEqual(r["percent"], 39.5)

    def test_the_shifted_window_reproduces_the_figure_it_should_not_have(self):
        """355/838 was published without a command. This is the command, and
        it shows the number is an artifact of the wrong base."""
        self.assertEqual(
            branch_reach.report(window="stat-block-base")["reachable"], 355)

    def test_the_difference_is_the_enemy_record_and_the_wander_bucket(self):
        """Not a vague 'the windows differ': every address in the gap is
        named, and none of them is in a .SAV file."""
        rec = {b["addr"] for b in branch_reach.reachable(self.branches)}
        bad = {b["addr"] for b in
               branch_reach.reachable(self.branches, "stat-block-base")}
        lo, hi = branch_reach.WINDOWS["record"]
        gained = set()
        for b in self.branches:
            if b["addr"] not in bad - rec:
                continue
            for v in branch_reach.guard_displacements(b):
                if not lo <= v < hi:
                    gained.add(v)
        # DS:3952 is the ENEMY's record (docs/re/combat.md) and 20ae:3971 is
        # the wander bucket byte (docs/re/wander.md). A save writes neither.
        self.assertTrue(gained, "the windows must actually differ")
        self.assertTrue(all(v >= 0x3952 for v in gained), sorted(map(hex, gained)))
        self.assertEqual(len(bad - rec), 26)

        # ...and the shifted window MISSES two real ones: the empty-name
        # tests, which read the name shortstring's length byte at .SAV 0x100.
        lost = rec - bad
        self.assertEqual(lost, {"1000:7225", "1000:ed64"})

    def test_every_counted_branch_really_names_a_record_byte(self):
        """No branch is counted for a reason the report cannot show."""
        lo, hi = branch_reach.WINDOWS["record"]
        hits = branch_reach.reachable(self.branches)
        self.assertEqual(len(hits), 331)
        # `0 <= v - lo < SIZE` was here and could not fail: `v` is already
        # filtered by `lo <= v < hi` with `hi == lo + SIZE`. What CAN fail,
        # and is the actual claim, is that the offset names a real field.
        fields = {f["off"] for f in LAYOUT["fields"]}
        spans = [(f["off"], f["off"] + f["len"]) for f in LAYOUT["fields"]]
        self.assertTrue(fields)
        for b in hits:
            hit = [v for v in branch_reach.guard_displacements(b)
                   if lo <= v < hi]
            self.assertTrue(hit, b["addr"])
            for v in hit:
                off = v - lo
                self.assertTrue(
                    any(a <= off < z for a, z in spans),
                    "%s reads .SAV 0x%03x, which no field covers" %
                    (b["addr"], off))

    def test_the_by_function_rows_sum_to_the_total(self):
        r = branch_reach.report()
        self.assertEqual(sum(x["reachable"] for x in r["by_function"]),
                         r["reachable"])
        for row in r["by_function"]:
            self.assertLessEqual(row["reachable"], row["branches"])


if __name__ == "__main__":
    unittest.main()
