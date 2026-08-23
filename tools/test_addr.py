#!/usr/bin/env python3
"""Tests for tools/addr.py -- the address convention, against the real bytes.

The point of this file is that every assertion is anchored to something in
`orig/g.exe`, not to the implementation.  `image_off_of_ghidra(0x1000, 0xb353)
== 0xb353` would restate the code and prove nothing; what proves something is
that the file offset the convention produces holds the bytes the convention
claims are there.

Run:  python3 -m unittest tools.test_addr -v
"""
import struct
import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "tools"))

import addr  # noqa: E402

EXE = (REPO / "orig" / "g.exe").read_bytes()

# The two landmarks docs/re/METHODOLOGY.md names, and what is actually there.
RANDOM_CALL = bytes.fromhex("9a4b11780f")          # call far 0f78:114b
RANDOM_PROLOGUE = bytes.fromhex("e85a008bdc")      # call 0f78:11a8 / mov bx,sp


class TestLandmarksAgainstTheBytes(unittest.TestCase):
    """Both landmarks, checked by reading g.exe -- not by re-deriving."""

    def test_ghidra_landmark_resolves_to_a_random_call(self):
        cit = addr.citation("1000:b353")
        self.assertEqual(cit.form, "ghidra")
        self.assertEqual(cit.file_off, 0xCC23)
        self.assertEqual(EXE[cit.file_off:cit.file_off + 5], RANDOM_CALL)

    def test_runtime_landmark_resolves_to_the_borland_random(self):
        cit = addr.citation("0f78:114b")
        self.assertEqual(cit.form, "runtime")
        self.assertEqual(cit.file_off, 0x1219B)
        body = EXE[cit.file_off:cit.file_off + 29]
        self.assertEqual(body[:5], RANDOM_PROLOGUE)
        # The 32x16 high take, twice, and the far return that pops the argument.
        self.assertEqual(body.count(bytes.fromhex("36f76704")), 2)
        self.assertEqual(body[26:29], bytes.fromhex("ca0200"))

    def test_the_same_address_in_both_forms_lands_on_the_same_bytes(self):
        """`0f78:114b` and Ghidra's `1f78:114b` are one address."""
        a = addr.citation("0f78:114b")
        b = addr.citation("1f78:114b")
        self.assertEqual(a.image_off, b.image_off)
        self.assertEqual(a.ghidra_label, "1f78:114b")
        self.assertEqual(EXE[b.file_off:b.file_off + 5], RANDOM_PROLOGUE)

    def test_the_64k_undershoot_lands_in_the_middle_of_another_instruction(self):
        """Form A applied to a Form B address is 64 KiB short -- and the proof
        is the bytes, not the arithmetic: 0x219b is the interior of the
        `lea di,[bp-0x102]` at 1000:08ca, in a screen-drawing loop."""
        right = addr.citation("0f78:114b").file_off
        wrong = addr.HEADER_BYTES + (0x0F78 - addr.GHIDRA_BASE_SEG) * 16 + 0x114B
        self.assertEqual(right - wrong, addr.GHIDRA_BASE_SEG * 16)
        self.assertEqual(right - wrong, 0x10000)
        lea = addr.file_off_of_citation("1000:08ca")
        self.assertEqual(EXE[lea:lea + 4], bytes.fromhex("8dbefefe"))
        self.assertLess(lea, wrong)                 # the wrong offset is INSIDE it
        self.assertLess(wrong, lea + 4)
        self.assertNotEqual(EXE[wrong:wrong + 5], RANDOM_PROLOGUE)

    def test_check_image_reports_the_evidence(self):
        ev = addr.check_image(EXE)
        self.assertEqual(ev["header_paragraphs"], 397)
        self.assertEqual(ev["header_bytes"], 0x18D0)
        self.assertEqual(ev["1000:b353"]["bytes"], RANDOM_CALL.hex(" "))
        self.assertEqual(ev["0f78:114b"]["file_off"], 0x1219B)


class TestHeaderIsDerived(unittest.TestCase):
    """0x18d0 must come out of the MZ header, not out of a constant."""

    def test_header_bytes_comes_from_e_cparhdr(self):
        hdrpara, = struct.unpack_from("<H", EXE, 0x08)
        self.assertEqual(addr.header_bytes(EXE), hdrpara * 16)
        self.assertEqual(addr.HEADER_BYTES, hdrpara * 16)
        self.assertEqual(addr.HEADER_BYTES, addr.EXPECTED_HEADER_BYTES)

    def test_a_different_header_size_derives_differently(self):
        """The derivation actually reads the field: change it, and the answer
        changes.  (A test that only checked `== 0x18d0` could not tell a
        derivation from a literal.)"""
        other = bytearray(EXE[:0x40])
        struct.pack_into("<H", other, 0x08, 0x100)
        self.assertEqual(addr.header_bytes(bytes(other)), 0x1000)
        self.assertNotEqual(addr.header_bytes(bytes(other)), addr.HEADER_BYTES)

    def test_check_image_rejects_a_wrong_header_size(self):
        other = bytearray(EXE)
        struct.pack_into("<H", other, 0x08, 0x100)
        with self.assertRaises(addr.AddressError):
            addr.check_image(bytes(other))

    def test_not_an_mz_image(self):
        with self.assertRaises(addr.AddressError):
            addr.header_bytes(b"PK\x03\x04" + bytes(0x40))

    def test_file_and_image_offsets_round_trip(self):
        self.assertEqual(addr.image_off_of_file_off(
            addr.file_off_of_image_off(0xB353)), 0xB353)
        with self.assertRaises(addr.AddressError):
            addr.image_off_of_file_off(addr.HEADER_BYTES - 1)


class TestRangesOfValidity(unittest.TestCase):
    """Each form rejects the other form's segments: the 64 KiB error raises."""

    def test_ghidra_form_rejects_a_runtime_segment(self):
        for seg in (0x0000, 0x0EED, 0x0F16, 0x0F78, 0x0FFF):
            with self.assertRaises(addr.AddressError):
                addr.image_off_of_ghidra(seg, 0x114B)

    def test_runtime_form_rejects_a_ghidra_label(self):
        for seg in (0x1000, 0x1F78, 0x20AE, 0xFFFF):
            with self.assertRaises(addr.AddressError):
                addr.image_off_of_seg_off(seg, 0x114B)

    def test_the_error_names_the_other_function(self):
        with self.assertRaises(addr.AddressError) as cm:
            addr.image_off_of_ghidra(0x0F78, 0x114B)
        self.assertIn("image_off_of_seg_off", str(cm.exception))
        with self.assertRaises(addr.AddressError) as cm:
            addr.image_off_of_seg_off(0x1F78, 0x114B)
        self.assertIn("image_off_of_ghidra", str(cm.exception))

    def test_offsets_must_fit_in_16_bits(self):
        with self.assertRaises(addr.AddressError):
            addr.image_off_of_ghidra(0x1000, 0x10000)
        with self.assertRaises(addr.AddressError):
            addr.image_off_of_seg_off(0x0F78, -1)

    def test_citation_picks_the_form_so_a_caller_never_can(self):
        self.assertEqual(addr.citation("1000:b353").form, "ghidra")
        self.assertEqual(addr.citation("0f78:114b").form, "runtime")
        self.assertEqual(addr.citation("0000:0000").form, "runtime")
        self.assertEqual(addr.citation("1000:0000").form, "ghidra")

    def test_malformed_citations_raise(self):
        for bad in ("1000", "1000:", ":114b", "g000:0000", "1000:114b:0",
                    "0x1000:0x114b", ""):
            with self.assertRaises(addr.AddressError):
                addr.citation(bad)


class TestRelocationTable(unittest.TestCase):
    """The relocation table's relative segments are exactly the domain of the
    runtime form -- which is why Form B has no `- 0x1000` term."""

    def test_exactly_four_relative_segments(self):
        self.assertEqual(addr.relocation_segments(EXE),
                         (0x0000, 0x0EED, 0x0F16, 0x0F78))

    def test_every_relative_segment_is_accepted_by_the_runtime_form(self):
        """Each relative segment's base must land inside the image, and the
        Ghidra form must refuse it.  Checked against the file's own size rather
        than by restating the arithmetic."""
        for seg in addr.relocation_segments(EXE):
            self.assertLess(seg, addr.GHIDRA_BASE_SEG)
            base = addr.file_off_of_image_off(addr.image_off_of_seg_off(seg, 0))
            self.assertGreaterEqual(base, addr.HEADER_BYTES)
            self.assertLess(base, len(EXE))
            with self.assertRaises(addr.AddressError):
                addr.image_off_of_ghidra(seg, 0)
        # And the one whose contents are known: 0f78:114b is Random.
        self.assertIn(0x0F78, addr.relocation_segments(EXE))
        fo = addr.file_off_of_citation("0f78:114b")
        self.assertEqual(EXE[fo:fo + 5], RANDOM_PROLOGUE)

    def test_relocation_count_and_offsets(self):
        entries = addr.relocation_entries(EXE)
        self.assertEqual(len(entries), 1580)

        # Read the table's own header fields and its first/last entries
        # directly from the bytes -- independent of relocation_entries' own
        # arithmetic -- and confirm relocation_image_offs, and its
        # parse_relocations alias, agree with that independent read.
        crlc, = struct.unpack_from("<H", EXE, 0x06)
        lfarlc, = struct.unpack_from("<H", EXE, 0x18)
        self.assertEqual(crlc, 1580)
        first_off, first_seg = struct.unpack_from("<HH", EXE, lfarlc)
        last_off, last_seg = struct.unpack_from(
            "<HH", EXE, lfarlc + (crlc - 1) * 4)

        image_offs = addr.relocation_image_offs(EXE)
        self.assertEqual(len(image_offs), crlc)
        self.assertEqual(image_offs[0], first_seg * 16 + first_off)
        self.assertEqual(image_offs[-1], last_seg * 16 + last_off)

        parsed = addr.parse_relocations(EXE)
        self.assertEqual(parsed[0], first_seg * 16 + first_off)
        self.assertEqual(parsed[-1], last_seg * 16 + last_off)

    def test_every_random_call_carries_its_segment_word_in_the_table(self):
        """The claim METHODOLOGY.md rests Form B on: all 86 `9a 4b 11 78 0f`
        far calls have their segment word relocated, so `0f78` is a RELATIVE
        segment awaiting a fixup."""
        image = addr.load_image(EXE)
        relocs = set(addr.relocation_image_offs(EXE))
        sites, i = [], image.find(addr.RANDOM_CALL_BYTES)
        while i != -1:
            sites.append(i)
            i = image.find(addr.RANDOM_CALL_BYTES, i + 1)
        self.assertEqual(len(sites), 86)
        # byte 0 is 0x9a, bytes 1-2 the offset, bytes 3-4 the segment word.
        self.assertTrue(all(s + 3 in relocs for s in sites))

    def test_load_image_is_everything_after_the_header(self):
        self.assertEqual(addr.load_image(EXE), EXE[addr.HEADER_BYTES:])
        self.assertEqual(len(addr.load_image(EXE)), len(EXE) - 0x18D0)


class TestDataSegment(unittest.TestCase):
    def test_dgroup_image_offset_is_derived_from_the_ghidra_form(self):
        self.assertEqual(addr.DATA_SEG_IMAGE_OFF, 0x10AE0)

    def test_randseed_sits_where_docs_re_rng_says_it_does(self):
        """`20ae:367e` is RandSeed; in the FILE it still holds the image value
        0, because the runtime writes it only after the loader has run."""
        image_off = addr.image_off_of_citation("20ae:367e")
        self.assertEqual(image_off, 0x1415E)
        self.assertEqual(addr.file_off_of_image_off(image_off), 0x15A2E)
        self.assertEqual(addr.data_off_of_image_off(image_off), 0x367E)
        self.assertEqual(addr.image_off_of_data_off(0x367E), image_off)

    def test_data_helpers_reject_a_code_offset(self):
        self.assertFalse(addr.is_data_image_off(0xB353))
        with self.assertRaises(addr.AddressError):
            addr.data_off_of_image_off(0xB353)

    def test_dgroup_is_set_by_the_runtime_itself(self):
        """`0f78:0000` is `mov dx,0x10ae` / `mov ds,dx` -- DGROUP's relative
        segment, which is what makes `20ae` the Ghidra label for it."""
        fo = addr.file_off_of_citation("0f78:0000")
        self.assertEqual(EXE[fo:fo + 5], bytes.fromhex("baae108eda"))
        self.assertEqual(0x10AE, addr.DATA_SEG_GHIDRA - addr.GHIDRA_BASE_SEG)


if __name__ == "__main__":
    unittest.main()
