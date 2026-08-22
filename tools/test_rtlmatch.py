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

#: The four routines this program links from a different build of the runtime
#: than the library it was matched against (`docs/re/rtl.md`, "which build of
#: TP 7").  Pinned by citation so a regeneration that loses one, or promotes a
#: fifth, fails here instead of changing a count in a document.
DIVERGENT = {"0f78:0c8f", "0f16:003b", "0f16:02a8", "0f16:02c8"}

#: `Delete`'s divergence, at `0f78:0c8f` + 32, checkable WITHOUT the library:
#: `cmp word [bp+8],1` / `jge +5` / `mov word [bp+8],1`.  The library's copy
#: has no such clamp -- it has `cmp word [bp+8],0` / `jle` to the epilogue, 19
#: bytes earlier.  See `docs/re/gaps.md`, "`Delete`'s index clamp".
DELETE_CLAMP = (32, "83 7e 08 01 7d 05 c7 46 08 01 00")

#: The one routine OF THE 104 NAMED HERE whose `size` does NOT tile into whole
#: instructions when it is read as a SPAN.  `size` is Ghidra's count of the
#: addresses in the body, and `0f78:1117` is the runtime's one split body --
#: named as such in `docs/re/branches.md` ("getNumAddresses", lines 248-258 and
#: 603-611) -- so its 22 is 10 bytes at `1117`..`1120` plus two 6-byte
#: out-of-line error tails at `113f` and `1145`, NOT a 22-byte window.  The
#: export is right; the span is the approximation.  Decoding 22 bytes forward
#: from `1117` therefore walks into `0f78:1121`, `0f78:1125` and the unexported
#: routine at `0f78:1129`, and stops two bytes short: the `call` at `0f78:112b`
#: is `e8 63 ff`, three bytes at `112b`..`112d`, and the window ends after
#: `112c`.  `data/functions.json` is correct and is not edited, so the
#: exception is named here rather than removed.  (Over all 123 records, not
#: just these 104, one more fails to tile -- `1000:0d14`, in the game's own
#: code, under-reading by 2.  `tools/test_re_query.py` owns that census.)
DOES_NOT_TILE = {"0f78:1117"}


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

    def test_every_routine_decodes_and_its_bytes_tile_into_instructions(self):
        """`insn.length > 0` was trivially true whenever `decode` returned.

        The check that can actually fail is that decoding forward from the
        entry lands EXACTLY on the recorded end: an entry citation that
        drifted into the interior of an instruction, or a size that no longer
        agrees with the code, breaks the tiling.  One record legitimately does
        not tile -- `DOES_NOT_TILE` says which and why -- and this asserts it
        is still that one and no other.
        """
        ragged = {}
        for r in ROUTINES:
            with self.subTest(r["citation"]):
                end, pos = r["image_off"] + r["size"], r["image_off"]
                first = dis16.decode(IMG, pos)
                self.assertEqual(first.off, r["image_off"])
                while pos < end:
                    pos = dis16.decode(IMG, pos).end
                if pos != end:
                    ragged[r["citation"]] = (r["size"], pos - r["image_off"])
        self.assertEqual(set(ragged), DOES_NOT_TILE,
                         "recorded size vs bytes decoded: %r" % (ragged,))

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

    def test_divergent_routines_are_exactly_the_four_documented_ones(self):
        """`long_runs > 0` was true by construction.

        `rtlmatch.classify` sets `mode == "divergent"` if and only if `big` is
        non-empty, and `long_runs` is `len(big)`, so asserting it restated the
        implementation and could not fail.  What can fail: the divergent SET
        drifting from the four `docs/re/rtl.md` names.
        """
        div = {r["citation"] for r in ROUTINES
               if r["match"]["mode"] == "divergent"}
        self.assertEqual(div, DIVERGENT)

    def test_deletes_divergence_is_visible_in_the_image_itself(self):
        """The divergence claim, re-derived without the library.

        `docs/re/rtl.md` says this program's `Delete` carries an index clamp
        the library's copy does not.  The clamp is IN `orig/g.exe`, so the
        claim is checkable here: 11 bytes at `0f78:0c8f` + 32.
        """
        off = addr.file_off_of_citation("0f78:0c8f") + DELETE_CLAMP[0]
        want = bytes.fromhex(DELETE_CLAMP[1])
        self.assertEqual(EXE[off:off + len(want)], want)
        rec = next(r for r in ROUTINES if r["citation"] == "0f78:0c8f")
        self.assertEqual(rec["name"], "Delete")
        self.assertEqual(rec["match"]["mode"], "divergent")


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


#: Where a LINEAR sweep with `tools/dis16.py` stops in each runtime segment,
#: and why.  A linear sweep is not a disassembly: it walks bytes from offset 0
#: and halts at the first byte it will not decode, whether or not that byte is
#: code.  Recorded per segment so the population the cross-check below runs on
#: is a stated number rather than an implied "the whole segment".
#:
#: `0f78` is the one that matters: it halts 0x273 bytes into a 0x1360-byte
#: segment, on the byte `0x67` -- the letter `g` of "Copyright" inside
#: `Portions Copyright (c) 1983,92 Borland` at `0f78:0264`.  `0x67` is the
#: address-size prefix, which `dis16` refuses by design.  So the sweep covers
#: 289 of capstone's 2235 instructions for that segment; the other 87%, which
#: holds 81 of the 107 routines, is reached by
#: `test_named_routines_decode_identically` instead, which starts at each
#: routine's own entry.
SWEEP_STOP = {
    0x0EE5: (0x0080, None),
    0x0EED: (0x028F, "modrm runs off the end of the buffer at 0x28f"),
    0x0F16: (0x061F, "modrm runs off the end of the buffer at 0x61f"),
    0x0F78: (0x0273, "address-size prefix at 0x273: 32-bit addressing is not "
                     "decoded here"),
}


class TestCapstoneAgreesWithDis16(unittest.TestCase):
    """Two independent decoders over the same bytes must draw the same boundaries.

    `tools/dis16.py` is the shipped decoder (validated against `ndisasm`); this
    is a third opinion over the runtime segments.  A disagreement is a real
    defect in one of them and is worth more than whatever it was found while
    doing -- so this asserts agreement rather than reporting a ratio.

    Both tests below report the POPULATION they ran on, and both fail if it
    shrinks.  The earlier version of the segment test skipped offsets capstone
    had no instruction at with a bare `continue`, and stopped its own sweep on
    the first `DecodeError` with a bare `break`; between them a headline
    "1282 instructions over all four runtime segments" was really 1282 over
    one-and-a-bit segments.  Nothing was skipped for the first reason -- the
    count is 0 and is now asserted -- and everything was lost to the second.
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
        """Both decoders over one straight run, plus where `dis16` stopped.

        Returns `(cs_len, d_len, stop, why)`.  `stop` is the offset the linear
        `dis16` sweep reached; `why` is the `DecodeError` that ended it, or
        `None` when it ran to the end.  Callers MUST look at those -- a sweep
        that halts early is the difference between "over the segment" and
        "over the first 4% of the segment".
        """
        buf = IMG[base:base + length]
        cs_len = {}
        for i in self.md.disasm(buf, 0):
            cs_len[i.address] = i.size
        d_len, off, why = {}, 0, None
        while off < length:
            try:
                insn = dis16.decode(buf, off)
            except dis16.DecodeError as e:
                why = str(e)
                break
            d_len[off] = insn.length
            off += insn.length
        return cs_len, d_len, off, why

    def test_the_linear_sweep_stops_where_it_is_documented_to(self):
        """The population the next test runs on, asserted rather than implied."""
        for seg, (base, length) in sorted(rtlmatch.SEGMENTS.items()):
            with self.subTest("%04x" % seg):
                _, _, stop, why = self._lengths(base, length)
                want_stop, want_why = SWEEP_STOP[seg]
                self.assertEqual(stop, want_stop)
                self.assertEqual(why, want_why)
        base, length = rtlmatch.SEGMENTS[0x0F78]
        self.assertEqual(IMG[base + 0x273], 0x67, "the `g` of Copyright")
        self.assertIn(b"Portions Copyright (c) 1983,92 Borland",
                      bytes(IMG[base + 0x240:base + 0x290]))

    def test_runtime_segments_decode_identically(self):
        compared, unmatched, totals = 0, 0, {}
        for seg, (base, length) in sorted(rtlmatch.SEGMENTS.items()):
            cs_len, d_len, stop, _ = self._lengths(base, length)
            totals["%04x" % seg] = (len(d_len), len(cs_len), stop, length)
            for off, n in sorted(d_len.items()):
                if off not in cs_len:
                    unmatched += 1
                    continue
                compared += 1
                self.assertEqual(
                    n, cs_len[off],
                    "%04x:%04x: dis16 says %d bytes, capstone says %d (%s)"
                    % (seg, off, n, cs_len[off],
                       IMG[base + off:base + off + max(n, cs_len[off])].hex(" ")))
        # Not "> 1000": the exact population, so a sweep that silently
        # shortens fails instead of still clearing a floor.
        self.assertEqual(compared, 1282, "population changed: %r" % (totals,))
        self.assertEqual(unmatched, 0,
                         "offsets dis16 decoded that capstone did not start an "
                         "instruction at -- these were skipped silently: %r"
                         % (totals,))
        # And the honest coverage: the sweep reaches only 289 of the 2235
        # instructions capstone finds in `0f78`.
        self.assertEqual(totals["0f78"][0], 289)
        self.assertEqual(totals["0f78"][1], 2235)

    def test_named_routines_decode_identically(self):
        """The wide cross-check: entry-anchored, so it is not sweep-limited."""
        compared, unmatched, covered, sized = 0, 0, 0, 0
        for r in ROUTINES:
            if not r["name"]:
                continue
            cs_len, d_len, stop, _ = self._lengths(r["image_off"], r["size"])
            covered += stop
            sized += r["size"]
            for off, n in sorted(d_len.items()):
                if off not in cs_len:
                    unmatched += 1
                    continue
                compared += 1
                self.assertEqual(n, cs_len[off],
                                 "%s +%#x" % (r["citation"], off))
        self.assertEqual(compared, 2258)
        self.assertEqual(unmatched, 0)
        # 4973 of 4975 bytes.  The two missing are the last two of the 22-byte
        # WINDOW taken for `0f78:1117`, the same `DOES_NOT_TILE` record: that
        # window is not a body (see above), and decoding forward from the entry
        # reaches the 3-byte `call` at `0f78:112b` with only 2 bytes of window
        # left, so the sweep stops at +20 with "instruction at 0x14 runs off
        # the end of the buffer".  No other named routine loses a byte.
        self.assertEqual((covered, sized), (4973, 4975))


if __name__ == "__main__":
    unittest.main(verbosity=2)
