#!/usr/bin/env python3
"""`tools/savegen.py` produces a valid record, and touches nothing frozen."""
import hashlib
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import savegen                                                       # noqa: E402
from decode_save import SIZE, decode, decode_fields, encode          # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
ORIG = ROOT / "orig"
BASE = ORIG / "SAVE_R3.SAV"


def _digests():
    return {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(ORIG.glob("*.SAV"))}


class SaveGenTest(unittest.TestCase):
    def setUp(self):
        self.base = BASE.read_bytes()

    def test_no_change_at_all_reproduces_the_base_byte_for_byte(self):
        self.assertEqual(savegen.synthesise(self.base), self.base)

    def test_a_named_field_lands_at_the_offset_the_layout_gives_it(self):
        out = savegen.synthesise(self.base, {"money": 1234, "level": 7})
        r = decode_fields(out)
        self.assertEqual(r["money"], 1234)
        self.assertEqual(r["level"], 7)
        # ...and nothing else moved: every byte that changed lies inside one
        # of the two fields' own spans. Stated as a subset rather than an
        # exact list because a high byte that already held the wanted value
        # is not a change -- SAVE_R3's level is 20, so writing 7 moves only
        # the low byte, and an exact list would encode that accident.
        allowed = set(range(0x20A, 0x20C)) | set(range(0x22B, 0x22D))
        differ = {i for i in range(SIZE) if out[i] != self.base[i]}
        self.assertTrue(differ, "nothing changed at all")
        self.assertTrue(differ <= allowed, sorted(differ - allowed))

    def test_the_result_round_trips_through_the_reference_decoder(self):
        out = savegen.synthesise(
            self.base,
            {"name": "^7 probe", "pistol": 1, "cartridges": 300},
            savegen.sentinel_bytes([(0x214, 0x231)]),
        )
        self.assertEqual(len(out), SIZE)
        self.assertEqual(encode(decode(out)), out)
        self.assertEqual(decode_fields(out)["name"], "^7 probe")
        self.assertEqual(decode_fields(out)["cartridges"], 300)

    def test_a_signed_field_accepts_negatives_and_rejects_overflow(self):
        out = savegen.synthesise(self.base, {"money": -5})
        self.assertEqual(decode_fields(out)["money"], -5)
        with self.assertRaises(savegen.SaveGenError):
            savegen.synthesise(self.base, {"money": 40000})
        with self.assertRaises(savegen.SaveGenError):
            savegen.synthesise(self.base, {"pistol": 2})

    def test_an_unknown_field_name_is_refused_rather_than_ignored(self):
        with self.assertRaises(savegen.SaveGenError):
            savegen.synthesise(self.base, {"tooth_gaurd": 1})

    def test_raw_bytes_are_applied_after_named_fields(self):
        out = savegen.synthesise(self.base, {"money": 0x1111}, {0x22B: 0xEE})
        self.assertEqual(out[0x22B], 0xEE)
        self.assertEqual(out[0x22C], 0x11)

    def test_sentinels_are_distinct_and_never_boolean_valued(self):
        s = savegen.sentinel_bytes([(0x214, 0x231), (0x2AE, 0x2B6)])
        self.assertEqual(len(s), 0x231 - 0x214 + 0x2B6 - 0x2AE)
        self.assertEqual(len(set(s.values())), len(s))
        self.assertTrue(all(v > 1 for v in s.values()))

    def test_the_cli_refuses_every_frozen_oracle_not_just_orig(self):
        """The guard is oracle-scoped, not directory-scoped.

        Its first revision only checked `--out`'s parent against `orig/`,
        so `--out data/rng_trace.json` was accepted while the error message
        talked about frozen ground truth. Every name in `savegen.FROZEN` is
        checked here, plus a relative path that walks through `..` to reach
        one, plus `orig/` as a directory.
        """
        before = _digests()
        targets = [str(ROOT / rel) for rel in savegen.FROZEN]
        targets.append(str(ORIG / "SAVE_R9.SAV"))          # orig/ as a dir
        targets.append(str(ROOT / "data" / ".." / "data" / "rng_trace.json"))
        self.assertGreaterEqual(len(targets), 14)
        for t in targets:
            with self.assertRaises(savegen.SaveGenError, msg=t):
                savegen.main(["--base", str(BASE), "--out", t])
        self.assertEqual(_digests(), before)
        self.assertFalse((ORIG / "SAVE_R9.SAV").exists())
        # ...and an ordinary destination is still accepted, or the guard
        # would be passing by refusing everything.
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(
                savegen.main(["--base", str(BASE),
                              "--out", str(pathlib.Path(d) / "x.SAV")]), 0)

    def test_the_cli_writes_a_loadable_record(self):
        before = _digests()
        with tempfile.TemporaryDirectory() as d:
            dest = pathlib.Path(d) / "SAVE_R3.SAV"
            self.assertEqual(
                savegen.main(["--base", str(BASE), "--out", str(dest),
                              "--set", "level=6", "--set", "money=0x100",
                              "--set-byte", "0x214=0x5a"]),
                0)
            out = dest.read_bytes()
        self.assertEqual(len(out), SIZE)
        self.assertEqual(decode_fields(out)["level"], 6)
        self.assertEqual(decode_fields(out)["money"], 256)
        self.assertEqual(out[0x214], 0x5A)
        # The corpus is ground truth: running the generator must not have
        # touched a byte of it.
        self.assertEqual(_digests(), before)


if __name__ == "__main__":
    unittest.main()
