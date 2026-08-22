#!/usr/bin/env python3
"""Tests for tools/dis16.py and tools/re_query.py.

Everything here is checked against `orig/g.exe` or against evidence produced
independently of these tools:

  * `pushed-n` is checked against `data/wander.json`, whose `n_at` / `n_bytes`
    were derived BY HAND in Task 11c across 18 draw sites.  If the walk-back is
    wrong anywhere, it disagrees with a byte string somebody already read out
    of the image.
  * the decoder's instruction lengths are cross-checked against `ndisasm` over
    every in-function instruction, when ndisasm is installed.  That check is
    skipped without it -- so it never stands alone, and the unconditional tests
    below pin the encodings that matter.
  * `xrefs-to`'s straddle filter is exercised over a range of DGROUP targets,
    not just the one address (`20ae:3b74`) that produced the original symptom,
    and every hit it ACCEPTS is re-verified straight from the image bytes.

Run:  python3 -m unittest tools.test_re_query -v
"""
import json
import re
import shutil
import struct
import subprocess
import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "tools"))

import addr  # noqa: E402
import dis16  # noqa: E402
import re_query  # noqa: E402

EXE = (REPO / "orig" / "g.exe").read_bytes()
IMAGE = addr.load_image(EXE)
WANDER = json.loads((REPO / "data" / "wander.json").read_text())
FUNCTIONS = json.loads((REPO / "data" / "functions.json").read_text())

PROG = re_query.Program()


def wander_draw_sites():
    """`[(site, n, n_expr, n_at, n_bytes), ...]` from data/wander.json.

    Task 11c read each of these out of the image by eye; they are independent
    of everything in tools/re_query.py.
    """
    rows, seen = [], set()

    def walk(o):
        if isinstance(o, dict):
            if "draw_ordinal" in o and ("at" in o or "site" in o):
                site = o.get("at") or o.get("site")
                key = (site, o.get("n_at"))
                if key not in seen:
                    seen.add(key)
                    rows.append((site, o.get("n"), o.get("n_expr"),
                                 o.get("n_at"), o.get("n_bytes")))
            for v in o.values():
                walk(v)
        elif isinstance(o, list):
            for v in o:
                walk(v)

    walk(WANDER)
    return rows


def random_call_sites():
    out, i = [], IMAGE.find(addr.RANDOM_CALL_BYTES)
    while i != -1:
        out.append(i)
        i = IMAGE.find(addr.RANDOM_CALL_BYTES, i + 1)
    return out


class TestDecoder(unittest.TestCase):
    def test_known_encodings(self):
        """Lengths and operand layout for encodings this repo already cites."""
        cases = [
            ("1000:b353", 5, "call 0xf78:0x114b", [("ptr_off", 0x114B),
                                                   ("ptr_seg", 0x0F78)]),
            ("1000:b359", 3, "mov [0x3971],al", [("moffs16", 0x3971)]),
            ("1000:b35c", 5, "cmp byte [0x3971],0xa", [("disp16", 0x3971),
                                                       ("imm8", 0x0A)]),
            ("1000:b34d", 3, "mov ax,0x5", [("imm16", 5)]),
            ("1000:08ca", 4, "lea di,[bp-0x102]", [("disp16x", 0xFEFE)]),
            ("0f78:1152", 4, "mul [ss:bx+0x4]", [("disp8", 4)]),
            ("0f78:1165", 3, "retf 0x2", [("imm16", 2)]),
        ]
        for cit, length, text, operands in cases:
            insn = dis16.decode(IMAGE, addr.image_off_of_citation(cit))
            self.assertEqual(insn.length, length, cit)
            self.assertEqual(insn.text, text, cit)
            self.assertEqual([(o.kind, o.value) for o in insn.operands],
                             operands, cit)

    def test_operand_spans_point_at_the_right_bytes(self):
        insn = dis16.decode(IMAGE, addr.image_off_of_citation("1000:b35c"))
        disp = insn.operands[0]
        self.assertEqual(struct.unpack_from("<H", IMAGE, disp.start)[0], 0x3971)
        self.assertEqual(insn.operand_at(disp.start, size=2), disp)
        self.assertIsNone(insn.operand_at(disp.start + 1, size=2))

    def test_unknown_opcode_raises_rather_than_guessing_a_length(self):
        with self.assertRaises(dis16.DecodeError):
            dis16.decode(b"\x0f\x0b", 0)
        with self.assertRaises(dis16.DecodeError):
            dis16.decode(b"\x67\x8b\x00", 0)      # 32-bit addressing
        with self.assertRaises(dis16.DecodeError):
            dis16.decode(b"\xc4\xc4", 0)          # les with mod=3 is illegal
        with self.assertRaises(dis16.DecodeError):
            dis16.decode(b"\x81", 0)              # runs off the end

    def test_decode_run_stops_where_asked(self):
        lo = addr.image_off_of_citation("1000:b353")
        insns = dis16.decode_run(IMAGE, lo, lo + 9)
        self.assertEqual([i.length for i in insns], [5, 1, 3])
        self.assertEqual(insns[-1].end, lo + 9)

    @unittest.skipUnless(shutil.which("ndisasm"), "ndisasm is not installed")
    def test_lengths_agree_with_ndisasm_over_every_in_function_instruction(self):
        ranges = [(addr.image_off_of_citation(f["entry"]),
                   addr.image_off_of_citation(f["entry"]) + f["size"])
                  for f in FUNCTIONS]

        def in_function(o):
            return any(lo <= o < hi for lo, hi in ranges)

        out = subprocess.run(
            ["ndisasm", "-b", "16", "-e", hex(addr.HEADER_BYTES), "-o", "0",
             str(REPO / "orig" / "g.exe")],
            capture_output=True, text=True, check=True).stdout
        pat = re.compile(r"^([0-9A-F]{8})\s+([0-9A-Fa-f]+)\s+\S")
        compared, mismatches = 0, []
        for line in out.splitlines():
            m = pat.match(line)
            if not m:
                continue
            off, length = int(m.group(1), 16), len(m.group(2)) // 2
            if not in_function(off):
                continue          # ndisasm sweeps through data too; we do not
            compared += 1
            try:
                insn = dis16.decode(IMAGE, off)
            except dis16.DecodeError as e:
                mismatches.append((hex(off), str(e)))
                continue
            if insn.length != length:
                mismatches.append((hex(off), "mine=%d ndisasm=%d"
                                   % (insn.length, length)))
        self.assertGreater(compared, 19000)
        self.assertEqual(mismatches, [], "%d mismatches of %d"
                         % (len(mismatches), compared))


class TestResolve(unittest.TestCase):
    def test_both_landmarks(self):
        a = re_query.resolve(PROG, "1000:b353")
        self.assertEqual((a["form"], a["file_off"]), ("ghidra", "0xcc23"))
        self.assertTrue(a["bytes"].startswith("9a 4b 11 78 0f"))
        self.assertEqual(a["instructions"][0]["text"], "call 0xf78:0x114b")

        b = re_query.resolve(PROG, "0f78:114b")
        self.assertEqual((b["form"], b["file_off"]), ("runtime", "0x1219b"))
        self.assertEqual(b["ghidra_label"], "1f78:114b")
        self.assertEqual([i["text"] for i in b["instructions"]],
                         ["call 0x10928", "mov bx,sp", "mov cx,dx",
                          "mul [ss:bx+0x4]"])

    def test_instructions_are_labelled_in_the_form_they_were_asked_in(self):
        b = re_query.resolve(PROG, "0f78:114b")
        self.assertEqual([i["at"] for i in b["instructions"]],
                         ["0f78:114b", "0f78:114e", "0f78:1150", "0f78:1152"])

    def test_random_is_eleven_instructions_not_five(self):
        """docs/re/METHODOLOGY.md quotes five byte-groups for 0f78:114b; the
        body is eleven instructions, and the elided half is the second
        `mul word [ss:bx+4]` of the 32x16 widening multiply."""
        lo = addr.image_off_of_citation("0f78:114b")
        insns = dis16.decode_run(IMAGE, lo, lo + 29)
        self.assertEqual(len(insns), 11)
        self.assertEqual(
            [i.text for i in insns],
            ["call 0x10928", "mov bx,sp", "mov cx,dx", "mul [ss:bx+0x4]",
             "mov ax,cx", "mov cx,dx", "mul [ss:bx+0x4]", "add ax,cx",
             "adc dx,0x0", "mov ax,dx", "retf 0x2"])
        self.assertEqual(sum(1 for i in insns if i.hex() == "36 f7 67 04"), 2)

    def test_a_runtime_segment_written_as_a_ghidra_label_is_the_same_address(self):
        self.assertEqual(re_query.resolve(PROG, "1f78:114b")["file_off"],
                         re_query.resolve(PROG, "0f78:114b")["file_off"])


class TestIsCallSite(unittest.TestCase):
    """Alignment and identity, separately -- and alignment never decides."""

    def test_the_trap_address_scores_on_alignment_and_still_is_not_a_call_site(self):
        r = re_query.is_call_site(PROG, "1000:d83b")
        self.assertFalse(r["identity"]["match"])
        self.assertEqual(r["verdict"], "NOT a call site")
        # It IS a real instruction boundary -- that is the whole trap.
        self.assertTrue(r["alignment"]["anchored_from_function_entry"])
        self.assertGreaterEqual(r["alignment"]["sweep_votes"],
                                r["alignment"]["sweep_tried"] - 1)
        # ...and the call it was mistaken for is four bytes later.
        self.assertEqual(r["identity"]["nearest_signature_deltas"], [4])
        self.assertEqual(r["identity"]["bytes_here"], "b8 06 00 50 9a")

    def test_the_real_site_four_bytes_on(self):
        r = re_query.is_call_site(PROG, "1000:d83f")
        self.assertTrue(r["identity"]["match"])
        self.assertEqual(r["verdict"], "call site")
        self.assertEqual(r["identity"]["nearest_signature_deltas"], [0])

    def test_every_signature_site_in_the_image_is_confirmed_by_identity(self):
        sites = random_call_sites()
        self.assertEqual(len(sites), 86)
        for io in sites:
            cit = re_query._citation_for(PROG, io)
            r = re_query.is_call_site(PROG, cit)
            self.assertTrue(r["identity"]["match"], cit)

    def test_a_data_byte_that_is_not_a_boundary(self):
        """One byte into the call: neither identity nor alignment."""
        r = re_query.is_call_site(PROG, "1000:d840")
        self.assertFalse(r["identity"]["match"])
        self.assertFalse(r["alignment"]["anchored_from_function_entry"])
        self.assertEqual(r["identity"]["nearest_signature_deltas"], [-1])


class TestPushedN(unittest.TestCase):
    def test_agrees_with_every_hand_derived_draw_site_in_wander_json(self):
        rows = wander_draw_sites()
        self.assertEqual(len(rows), 17)     # 18 draws, two of them one site
        for site, n, n_expr, n_at, n_bytes in rows:
            r = re_query.pushed_n(PROG, site)
            self.assertEqual(r["n_at"], n_at, site)
            self.assertEqual(r["n_bytes"], n_bytes, site)
            if n is not None:
                self.assertEqual(r["n"], n, site)
            else:
                self.assertIsNone(r["n"], site)

    def test_the_two_computed_district_forms(self):
        """`chapter` in data/wander.json is the byte at DGROUP 0x3692; the tool
        names the address, since it cannot know the label."""
        self.assertEqual(re_query.pushed_n(PROG, "1000:b2fa")["n_expr"],
                         "byte[0x3692] * 20")
        self.assertEqual(re_query.pushed_n(PROG, "1000:b321")["n_expr"],
                         "byte[0x3692] * 5")
        self.assertEqual(addr.image_off_of_citation("20ae:3692"),
                         addr.DATA_SEG_IMAGE_OFF + 0x3692)

    def test_a_push_of_a_stack_slot_is_reported_undetermined_not_guessed(self):
        r = re_query.pushed_n(PROG, "1000:25fe")
        self.assertIsNone(r["n"])
        self.assertIsNone(r["n_expr"])
        self.assertEqual(r["n_bytes"], "ff 76 fa")
        self.assertIsNotNone(r["undetermined"])

    def test_refuses_an_address_that_is_not_a_call_site(self):
        with self.assertRaises(ValueError) as cm:
            re_query.pushed_n(PROG, "1000:d83b")
        self.assertIn("is-call-site", str(cm.exception))

    def test_every_signature_site_resolves_and_its_bytes_are_really_there(self):
        undetermined = 0
        for io in random_call_sites():
            cit = re_query._citation_for(PROG, io)
            r = re_query.pushed_n(PROG, cit)
            raw = bytes.fromhex(r["n_bytes"].replace(" ", ""))
            start = io - len(raw)
            # The reported idiom really is the bytes immediately before the call.
            self.assertEqual(IMAGE[start:io], raw, cit)
            self.assertEqual(addr.image_off_of_citation(r["n_at"]), start, cit)
            if r["n"] is None and r["n_expr"] is None:
                undetermined += 1
        self.assertEqual(undetermined, 13)


class TestXrefsTo(unittest.TestCase):
    KNOWN_3B74 = {"1000:b327", "1000:b32a", "1000:b336",
                  "1000:c377", "1000:c37a", "1000:c386"}

    def test_the_address_that_produced_the_original_symptom(self):
        r = re_query.xrefs_to(PROG, "20ae:3b74")
        self.assertEqual(r["scan"]["raw_hits"], 7)
        self.assertEqual({a["at"] for a in r["scan"]["accepted"]}, self.KNOWN_3B74)
        self.assertEqual(len(r["scan"]["discarded"]), 1)
        bad = r["scan"]["discarded"][0]
        self.assertEqual(bad["image_off"], "0xc358")
        self.assertIn("straddles", bad["why"])
        self.assertIn("jl", bad["why"])

    def test_a_bare_dgroup_offset_is_the_same_query(self):
        self.assertEqual(re_query.xrefs_to(PROG, "3b74"),
                         re_query.xrefs_to(PROG, "20ae:3b74"))

    def test_a_non_dgroup_citation_is_refused(self):
        with self.assertRaises(ValueError):
            re_query.xrefs_to(PROG, "1000:3b74")

    def test_the_bucket_byte_matches_what_docs_re_wander_documents(self):
        r = re_query.xrefs_to(PROG, "20ae:3971")
        ats = {a["at"] for a in r["scan"]["accepted"]}
        for cit in ("1000:b359", "1000:b35c", "1000:b368", "1000:b36f",
                    "1000:b37b", "1000:b382", "1000:b38e"):
            self.assertIn(cit, ats)

    def test_the_filter_is_a_class_guard_not_a_one_address_patch(self):
        """Over a range of DGROUP targets: every ACCEPTED hit is re-verified
        from the image bytes, and straddles are discarded at many different
        targets and sites -- not only at 20ae:3b74."""
        straddle_targets, straddle_sites = set(), set()
        raw = accepted = discarded = 0
        for off in range(0x3600, 0x3A00, 2):
            r = re_query.xrefs_to(PROG, "%04x" % off)
            raw += r["scan"]["raw_hits"]
            accepted += len(r["scan"]["accepted"])
            discarded += len(r["scan"]["discarded"])
            for a in r["scan"]["accepted"]:
                hit = int(a["hit_image_off"], 16)
                insn = dis16.decode(IMAGE, int(a["image_off"], 16))
                operand = insn.operand_at(hit, size=2)
                self.assertIsNotNone(operand, a)
                self.assertEqual(operand.value, off, a)
                self.assertEqual(struct.unpack_from("<H", IMAGE, hit)[0], off)
                self.assertEqual(int(a["operand_at"], 16), hit)
                self.assertTrue(insn.off <= hit < insn.end)
            for d in r["scan"]["discarded"]:
                if "straddles" in d["why"]:
                    straddle_targets.add(off)
                    straddle_sites.add(d["image_off"])
        self.assertGreater(raw, 500)
        self.assertGreater(accepted, 100)
        self.assertGreater(discarded, 100)
        self.assertGreater(len(straddle_targets), 20)
        self.assertGreater(len(straddle_sites), 50)

    def test_the_export_path_is_used_when_the_export_knows_the_address(self):
        r = re_query.xrefs_to(PROG, "20ae:3678")
        self.assertEqual(r["source"], "ghidra-export")
        self.assertTrue(r["export"]["available"])
        self.assertEqual(r["export"]["claims"], 3)
        self.assertEqual({v["at"] for v in r["export"]["verified"]},
                         {"1f78:0005", "1f78:0034", "1f78:015f"})
        self.assertEqual(r["export"]["rejected"], [])

    def test_a_wrong_export_claim_is_rejected_against_the_bytes(self):
        """Ghidra records the `mul [cs:0x11de]` at 1f78:11b1 as a reference to
        20ae:06fe.  The export is a lead, not an authority."""
        r = re_query.xrefs_to(PROG, "20ae:06fe")
        self.assertEqual(r["export"]["claims"], 1)
        self.assertEqual(r["export"]["verified"], [])
        self.assertEqual(len(r["export"]["rejected"]), 1)
        rej = r["export"]["rejected"][0]
        self.assertEqual(rej["at"], "1f78:11b1")
        self.assertIn("mul [cs:0x11de]", rej["why"])
        self.assertEqual(r["source"], "byte-scan")

    def test_the_export_carries_data_xrefs_at_all(self):
        """Guards against re-running an ExportAll that predates the field."""
        self.assertTrue(all("data_xrefs" in f for f in FUNCTIONS))
        self.assertGreater(sum(len(f["data_xrefs"]) for f in FUNCTIONS), 0)

    def test_the_export_is_sorted(self):
        """The nondeterminism fix, checked on the committed artifact."""
        for f in FUNCTIONS:
            self.assertEqual(f["calls"], sorted(f["calls"]), f["name"])
            self.assertEqual(f["called_by"], sorted(f["called_by"]), f["name"])
            keys = [(x["at"], x["to"], x["type"], x["op"]) for x in f["data_xrefs"]]
            self.assertEqual(keys, sorted(keys), f["name"])


class TestCli(unittest.TestCase):
    def run_cli(self, *args):
        out = subprocess.run(
            [sys.executable, str(REPO / "tools" / "re_query.py"), *args],
            capture_output=True, text=True)
        return out

    def test_resolve_prints_the_bytes(self):
        out = self.run_cli("resolve", "0f78:114b")
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertIn("0x1219b", out.stdout)
        self.assertIn("e8 5a 00", out.stdout)

    def test_json_on_either_side_of_the_subcommand(self):
        for args in (("--json", "resolve", "1000:b353"),
                     ("resolve", "1000:b353", "--json")):
            out = self.run_cli(*args)
            self.assertEqual(out.returncode, 0, out.stderr)
            self.assertEqual(json.loads(out.stdout)["file_off"], "0xcc23")

    def test_a_bad_citation_exits_non_zero_with_a_message(self):
        out = self.run_cli("resolve", "nonsense")
        self.assertEqual(out.returncode, 2)
        self.assertIn("not a seg:off citation", out.stderr)


if __name__ == "__main__":
    unittest.main()
