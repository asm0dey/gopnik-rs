#!/usr/bin/env python3
"""Tests for the runtime identification: `data/rtl_names.json` vs `orig/g.exe`.

The library the names came from is not in the repository, so these tests do
NOT re-run the match.  They check the committed result against the bytes it
claims to describe, which is the part that can rot: a citation that drifts, a
size that stops agreeing with `data/functions.json`, a name attached to an
address whose bytes are not what the evidence says.

They also exercise the matcher's *rejection*: `fits()` is the whole basis for
calling two byte strings the same routine, and a test suite that only ever
feeds it matching input would pass no matter what it did.

Run:  python3 tools/test_rtlmatch.py
"""
import json
import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "tools"))

import addr      # noqa: E402
import dis16     # noqa: E402
import rtlmatch  # noqa: E402

EXE = addr.read_exe()
IMG = addr.load_image(EXE)
DOC = json.loads((REPO / "data" / "rtl_names.json").read_text())
FUNCS = json.loads((REPO / "data" / "functions.json").read_text())
ROUTINES = DOC["routines"]

#: A name and the bytes that must be at its address.  These are the anchors:
#: if the table is ever regenerated against a different image or the address
#: convention slips, these fail rather than the counts quietly changing.
ANCHORS = {
    "0f78:114b": ("Random", "e8 5a 00 8b dc"),
    "0f78:11e0": ("Randomize", "b4 2c cd 21"),
    "0f78:02cd": ("rtl_stack_check", "05 00 02 72 0d"),
    "0f78:028a": ("IOResult", "33 c0 87 06 7c 36"),
    "0f78:0c03": ("rtl_char_to_str", "fc 8b dc 36 c4 7f 06"),
    "0f16:0614": ("rtl_crt_bios_video", "56 57 55 06 cd 10"),
    "0ee5:0000": ("FindFirst", "55 8b ec 83 ec 50"),
}

#: Names that legitimately appear at more than one address, because Turbo
#: Pascal implements the standard procedure twice -- once for `Text` and once
#: for a typed/untyped `File`.  Anything else repeating is a mistake.
OVERLOADED = {"Assign", "Reset", "Rewrite", "Close"}


class TestTableAgainstTheImage(unittest.TestCase):

    def test_every_record_addresses_resolve_through_addr(self):
        for r in ROUTINES:
            with self.subTest(r["citation"]):
                cit = addr.citation(r["citation"])
                self.assertEqual(cit.form, "runtime")
                self.assertEqual(cit.image_off, r["image_off"])
                self.assertEqual(cit.file_off, r["file_off"])
                self.assertEqual(cit.ghidra_label, r["ghidra"])

    def test_anchor_bytes_are_where_the_names_are(self):
        for cit, (name, hexbytes) in ANCHORS.items():
            with self.subTest(cit):
                want = bytes.fromhex(hexbytes)
                off = addr.file_off_of_citation(cit)
                self.assertEqual(EXE[off:off + len(want)], want)
                rec = next(r for r in ROUTINES if r["citation"] == cit)
                self.assertEqual(rec["name"], name)

    def test_every_routine_starts_on_a_decodable_instruction(self):
        for r in ROUTINES:
            with self.subTest(r["citation"]):
                insn = dis16.decode(IMG, r["image_off"])
                self.assertGreater(insn.length, 0)

    def test_sizes_and_membership_agree_with_functions_json(self):
        byentry = {f["entry"]: f for f in FUNCS}
        rtl = {f["entry"] for f in FUNCS
               if int(f["entry"].split(":")[0], 16) - 0x1000
               in rtlmatch.SEGMENTS}
        self.assertEqual({r["ghidra"] for r in ROUTINES}, rtl)
        for r in ROUTINES:
            with self.subTest(r["citation"]):
                self.assertEqual(r["size"], byentry[r["ghidra"]]["size"])
                self.assertEqual(r["ghidra_name"],
                                 byentry[r["ghidra"]]["name"])


class TestTableIsInternallyHonest(unittest.TestCase):

    def test_counts_are_recomputed_not_restated(self):
        named = [r for r in ROUTINES if r["name"]]
        self.assertEqual(DOC["counts"]["routines"], len(ROUTINES))
        self.assertEqual(DOC["counts"]["named"], len(named))
        self.assertEqual(DOC["counts"]["unnamed"], len(ROUTINES) - len(named))
        self.assertEqual(DOC["counts"]["named"] + DOC["counts"]["unnamed"],
                         DOC["counts"]["routines"])

    def test_every_name_carries_a_kind_a_tier_and_evidence(self):
        for r in ROUTINES:
            with self.subTest(r["citation"]):
                if r["name"] is None:
                    self.assertIsNone(r["name_kind"])
                    self.assertIsNone(r["evidence"])
                    continue
                self.assertIn(r["name_kind"],
                              ("tpl_symbol", "borland", "behavioural"))
                self.assertEqual(r["tier"], "flow")
                self.assertGreater(len(r["evidence"] or ""), 20)

    def test_names_repeat_only_where_the_procedure_is_overloaded(self):
        seen = {}
        for r in ROUTINES:
            if not r["name"]:
                continue
            seen.setdefault(r["name"], []).append(r["citation"])
        for name, cits in seen.items():
            with self.subTest(name):
                if len(cits) > 1:
                    self.assertIn(name, OVERLOADED, "%s at %r" % (name, cits))
                    self.assertEqual(len(cits), 2)

    def test_unnamed_routines_are_the_ones_outside_the_runtime(self):
        unnamed = [r for r in ROUTINES if not r["name"]]
        self.assertTrue(unnamed, "an all-named table would make this vacuous")
        for r in unnamed:
            with self.subTest(r["citation"]):
                self.assertEqual(r["match"]["mode"], "not_runtime")

    def test_divergent_routines_are_reported_not_hidden(self):
        div = [r for r in ROUTINES if r["match"]["mode"] == "divergent"]
        self.assertTrue(div, "the four known divergences must still be flagged")
        for r in div:
            with self.subTest(r["citation"]):
                self.assertGreater(r["match"]["long_runs"], 0)


class TestFitsCanReject(unittest.TestCase):
    """`fits()` is the match criterion; show it saying no."""

    def _bytes(self, cit, n):
        off = addr.file_off_of_citation(cit)
        return EXE[off:off + n]

    def test_two_different_string_helpers_do_not_fit_each_other(self):
        a = self._bytes("0f78:0ae7", 26)
        b = self._bytes("0f78:0b01", 26)
        self.assertFalse(rtlmatch.fits(a, b))

    def test_a_routine_fits_itself(self):
        a = self._bytes("0f78:114b", 29)
        self.assertTrue(rtlmatch.fits(a, a))

    def test_one_flipped_byte_still_fits_but_a_long_run_does_not(self):
        a = bytearray(self._bytes("0f78:0dee", 64))
        b = bytearray(a)
        b[10] ^= 0xFF
        self.assertTrue(rtlmatch.fits(bytes(a), bytes(b)))
        for i in range(20, 30):
            b[i] ^= 0xFF
        self.assertFalse(rtlmatch.fits(bytes(a), bytes(b)))

    def test_game_code_does_not_fit_a_runtime_routine(self):
        game = EXE[addr.file_off_of_citation("1000:3d11"):][:64]
        rtl = self._bytes("0f78:0dee", 64)
        self.assertFalse(rtlmatch.fits(game, rtl))

    def test_diff_runs_finds_the_runs_it_is_asked_about(self):
        self.assertEqual(rtlmatch.diff_runs(b"aaaa", b"aaaa"), [])
        self.assertEqual(rtlmatch.diff_runs(b"abcd", b"aXcd"), [[1, 1]])
        self.assertEqual(rtlmatch.diff_runs(b"abcd", b"aXYd"), [[1, 2]])
        self.assertEqual(rtlmatch.diff_runs(b"abcd", b"XbYd"), [[0, 0], [2, 2]])


class TestCapstoneAgreesWithDis16(unittest.TestCase):
    """Two independent decoders over the same bytes must draw the same boundaries.

    `tools/dis16.py` is the shipped decoder (validated against `ndisasm`); this
    is a third opinion over the runtime segments.  A disagreement is a real
    defect in one of them and is worth more than whatever it was found while
    doing -- so this asserts agreement rather than reporting a ratio.
    """

    def setUp(self):
        try:
            import capstone
        except ImportError:  # pragma: no cover - capstone is expected here
            raise unittest.SkipTest(
                "capstone is NOT installed, so the dis16 cross-check did not "
                "run; install it or this file's strongest test is absent")
        self.md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_16)
        self.md.detail = False

    def _lengths(self, base, length):
        """Instruction lengths from both decoders over one straight run."""
        buf = IMG[base:base + length]
        cs_len, off = {}, 0
        for i in self.md.disasm(buf, 0):
            cs_len[i.address] = i.size
            off = i.address + i.size
        d_len = {}
        off = 0
        while off < length:
            try:
                insn = dis16.decode(buf, off)
            except dis16.DecodeError:
                break
            d_len[off] = insn.length
            off += insn.length
        return cs_len, d_len

    def test_runtime_segments_decode_identically(self):
        compared = 0
        for seg, (base, length) in sorted(rtlmatch.SEGMENTS.items()):
            cs_len, d_len = self._lengths(base, length)
            for off, n in sorted(d_len.items()):
                if off not in cs_len:
                    continue
                compared += 1
                self.assertEqual(
                    n, cs_len[off],
                    "%04x:%04x: dis16 says %d bytes, capstone says %d (%s)"
                    % (seg, off, n, cs_len[off],
                       IMG[base + off:base + off + max(n, cs_len[off])].hex(" ")))
        self.assertGreater(compared, 1000,
                           "too few instructions compared for this to mean "
                           "anything")

    def test_named_routines_decode_identically(self):
        compared = 0
        for r in ROUTINES:
            if not r["name"]:
                continue
            cs_len, d_len = self._lengths(r["image_off"], r["size"])
            for off, n in sorted(d_len.items()):
                if off not in cs_len:
                    continue
                compared += 1
                self.assertEqual(n, cs_len[off],
                                 "%s +%#x" % (r["citation"], off))
        self.assertGreater(compared, 500)


if __name__ == "__main__":
    unittest.main(verbosity=2)
