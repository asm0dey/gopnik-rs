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


class TestResolveReportsTheRuntimeName(unittest.TestCase):
    """`docs/re/rtl.md` says the names are consumed here instead of being
    written into `data/functions.json`.  Nothing asserted that until now: the
    pre-existing `resolve` tests traverse the branch, so a crash would have
    shown, but no test read the `rtl` block or its `kind`.

    `data/rtl_names.json` is committed, so these need no library.
    """

    def test_an_entry_address_gets_the_name_its_kind_and_its_unit(self):
        out = re_query.resolve(PROG, "0f78:114b")
        self.assertIn("rtl", out)
        self.assertEqual(out["rtl"]["name"], "Random")
        self.assertEqual(out["rtl"]["kind"], "borland")
        self.assertEqual(out["rtl"]["unit"], "SYSTEM")
        self.assertTrue(out["rtl"]["is_entry"])
        self.assertIn("0f78:114b", out["rtl"]["evidence"])

    def test_an_interior_address_gets_the_containing_routine_not_entry(self):
        out = re_query.resolve(PROG, "0f78:1152")
        self.assertEqual(out["rtl"]["name"], "Random")
        self.assertFalse(out["rtl"]["is_entry"])

    def test_a_coined_name_is_reported_as_coined(self):
        """The `kind` is the whole guard against citing a coined name as a
        Borland symbol, so it must survive the trip through `resolve`."""
        out = re_query.resolve(PROG, "0f16:000d")
        self.assertEqual(out["rtl"]["name"], "rtl_crt_initialization")
        self.assertEqual(out["rtl"]["kind"], "behavioural")
        self.assertEqual(out["rtl"]["unit"], "CRT")

    def test_a_symbol_table_name_is_reported_as_such(self):
        out = re_query.resolve(PROG, "0ee5:0000")
        self.assertEqual((out["rtl"]["name"], out["rtl"]["kind"],
                          out["rtl"]["unit"]),
                         ("FindFirst", "tpl_symbol", "DOS"))

    def test_game_code_and_the_unnamed_segment_get_no_rtl_block(self):
        self.assertNotIn("rtl", re_query.resolve(PROG, "1000:b353"))
        # `0eed` is the game's own second code segment: named nowhere, so it
        # must not acquire a runtime name by being outside segment 1000.
        self.assertNotIn("rtl", re_query.resolve(PROG, "0eed:0000"))

    def test_the_one_overlapping_ghidra_extent_resolves_to_the_inner_routine(self):
        """`size` is Ghidra's ADDRESS-SET byte count and `Program._ranges`
        reads it as a span, so a split body produces overlapping spans.
        `data/functions.json` has one such OVERLAPPING record -- `1f78:1117`,
        named as the image's sole overlapping one in `docs/re/branches.md`
        (lines 248-258 and 612-628; the second non-contiguous record,
        `1000:0d14`, under-reads instead and overlaps nothing) -- and its
        span `1117`..`112c` covers the two later entries `1f78:1121` and
        `1f78:1125`, hence two overlapping pairs from one outer record.
        First-match-in-file-order answered both with `FUN_1f78_1117` and
        `resolve` reported `rtl_real_op_div` for them; `docs/re/tables.md:346`
        cites `0f78:1125` by name, so this is a name a document depends on.
        The export is correct and is not edited; the span is the
        approximation, and the two tests below pin what it still gets wrong."""
        overlaps = []
        for lo, hi, f in PROG._ranges:
            for lo2, _, g in PROG._ranges:
                if f is not g and lo < lo2 < hi:
                    overlaps.append((f["entry"], g["entry"]))
        self.assertEqual(sorted(overlaps),
                         [("1f78:1117", "1f78:1121"),
                          ("1f78:1117", "1f78:1125")])
        for cit, name in (("0f78:1121", "rtl_real_op_cmp"),
                          ("0f78:1125", "rtl_real_op_from_longint"),
                          ("0f78:1117", "rtl_real_op_div")):
            with self.subTest(cit):
                out = re_query.resolve(PROG, cit)
                self.assertEqual(out["rtl"]["name"], name)
                self.assertTrue(out["rtl"]["is_entry"])
        # An address genuinely interior to the outer routine still gets it.
        self.assertEqual(re_query.resolve(PROG, "0f78:1119")["rtl"]["name"],
                         "rtl_real_op_div")

    def test_the_span_approximation_over_reads_past_the_split_body(self):
        """Failure direction 1 of reading `size` as a span, still live.

        `0f78:1117`'s span ends at `112c`, so it covers `0f78:1129`..`112c` --
        the head of a routine that `data/functions.json` does not export at
        all, and that is NOT part of `FUN_1f78_1117`'s body.  Nothing in
        `_ranges` can tell the difference, so `resolve` names the wrong
        routine there.  Asserted rather than quietly tolerated.
        """
        entries = {f["entry"] for f in PROG.functions}
        self.assertNotIn("1f78:1129", entries)
        for cit in ("0f78:1129", "0f78:112b", "0f78:112c"):
            with self.subTest(cit):
                out = re_query.resolve(PROG, cit)
                self.assertEqual(out["function"], "FUN_1f78_1117")
                self.assertEqual(out["rtl"]["name"], "rtl_real_op_div")
        # One past the span end is nobody's, which is also wrong -- `112d` is
        # the last byte of the `call` at `0f78:112b`.
        self.assertIsNone(re_query.resolve(PROG, "0f78:112d")["function"])
        # The bytes, straight from the image: `mov ch,0` / `call` / `jb` /
        # `retf` is a routine of its own, not a continuation of `1117`.
        off = addr.image_off_of_citation("0f78:1129")
        self.assertEqual(IMAGE[off:off + 8].hex(" "),
                         "b5 00 e8 63 ff 72 09 cb")

    def test_the_span_approximation_loses_the_split_bodys_out_of_line_tails(self):
        """Failure direction 2, in the opposite direction, also still live.

        `FUN_1f78_1117`'s 22 addresses are 10 at `1117`..`1120` plus two
        6-byte out-of-line error tails at `113f` and `1145`.  The span stops at
        `112c`, so every byte of both tails resolves to no function at all,
        and `resolve` falls back to the back-sweep anchor there.

        That the tails are error exits of this run of real-arithmetic thunks
        is read out of the image, not asserted: the `je` at `0f78:1119`
        targets `1145` and the `jb` at `0f78:111e` targets `113f`.  `1145` is
        reached from `1117` alone; `113f` is a SHARED tail, reached by the
        `jb` of all four thunks (`10ff`, `1105`, `1111`, `1117`), and Ghidra
        charged it to `1117` -- the other three are recorded as 6 bytes each,
        head only.  A shared tail can belong to only one address set, which is
        the same reason `1139`..`113e` is charged to `1131`.
        """
        head = addr.image_off_of_citation("0f78:1117")
        # `0a c9` or cl,cl / `74 2a` je +0x2a -> 1145 / `e8 96 fe` call /
        # `72 1f` jb +0x1f -> 113f / `cb` retf
        self.assertEqual(IMAGE[head:head + 10].hex(" "),
                         "0a c9 74 2a e8 96 fe 72 1f cb")
        self.assertEqual(0x1119 + 2 + 0x2a, 0x1145)
        self.assertEqual(0x111e + 2 + 0x1f, 0x113f)
        # The shared tail: all four thunks branch to `113f`, and the other
        # three are 6 bytes each, so only `1117` counts it in.
        seg = addr.image_off_of_citation("0f78:0000")
        for at, want, size in ((0x1102, "72 3b", 6), (0x1108, "72 35", 6),
                               (0x1114, "72 29", 6), (0x111e, "72 1f", 22)):
            with self.subTest("%#x" % at):
                b = IMAGE[seg + at:seg + at + 2]
                self.assertEqual(b.hex(" "), want)
                self.assertEqual(at + 2 + b[1], 0x113f)
        self.assertEqual(
            [f["size"] for f in PROG.functions
             if f["entry"] in ("1f78:10ff", "1f78:1105", "1f78:1111")],
            [6, 6, 6])
        for cit, want in (("0f78:113f", "b8 cd 00 e9 ca ef"),
                          ("0f78:1145", "b8 c8 00 e9 c4 ef")):
            off = addr.image_off_of_citation(cit)
            self.assertEqual(IMAGE[off:off + 6].hex(" "), want)
        for cit in ("0f78:113f", "0f78:1141", "0f78:1144",
                    "0f78:1145", "0f78:1148", "0f78:114a"):
            with self.subTest(cit):
                out = re_query.resolve(PROG, cit)
                self.assertIsNone(out["function"])
                self.assertNotIn("rtl", out)
        # 10 + 6 + 6 is the recorded size, so the export counted the tails in
        # and the span model is what drops them.
        rec = next(f for f in PROG.functions if f["entry"] == "1f78:1117")
        self.assertEqual(rec["size"], 10 + 6 + 6)

    def test_exactly_two_records_have_a_non_contiguous_body(self):
        """The census behind the two tests above, over all 123 records.

        Decoding forward from an entry for exactly `size` bytes tiles onto the
        recorded end whenever the body is contiguous.  Two records do not
        tile, in opposite directions, and this pins the set: a third one, or a
        change that repairs one of these, fails here instead of silently
        widening what the span model gets wrong.
        """
        ragged = {}
        for f in PROG.functions:
            lo = addr.image_off_of_citation(f["entry"])
            end, pos = lo + f["size"], lo
            while pos < end:
                pos = dis16.decode(IMAGE, pos).end
            if pos != end:
                ragged[f["entry"]] = pos - end
        self.assertEqual(len(PROG.functions), 123)
        self.assertEqual(ragged, {"1f78:1117": 1, "1000:0d14": 2})

    def test_the_game_functions_last_two_bytes_are_outside_its_span_too(self):
        """The second non-contiguous record, `1000:0d14`, under-reads by 2.

        Its `ret 0x2` is `c2 02 00` at `1000:11bf`..`11c1` and the next entry
        is `1000:11c2`, so the contiguous body is 1198 bytes; Ghidra's address
        count is 1196.  Whatever two addresses the export leaves out, the span
        `[0d14, 11c0)` stops before the `ret`'s operand, so those two bytes
        resolve to no function.
        """
        rec = next(f for f in PROG.functions if f["entry"] == "1000:0d14")
        self.assertEqual(rec["size"], 1196)
        self.assertIn("1000:11c2", {f["entry"] for f in PROG.functions})
        off = addr.image_off_of_citation("1000:11bf")
        self.assertEqual(IMAGE[off:off + 3].hex(" "), "c2 02 00")
        self.assertEqual(
            re_query.resolve(PROG, "1000:11bf")["function"], "FUN_1000_0d14")
        for cit in ("1000:11c0", "1000:11c1"):
            with self.subTest(cit):
                self.assertIsNone(re_query.resolve(PROG, cit)["function"])
        self.assertEqual(
            re_query.resolve(PROG, "1000:11c2")["function"], "FUN_1000_11c2")

    def test_every_named_routine_resolves_to_its_own_name(self):
        doc = json.loads((REPO / "data" / "rtl_names.json").read_text())
        n = 0
        for r in doc["routines"]:
            if not r["name"]:
                continue
            with self.subTest(r["citation"]):
                out = re_query.resolve(PROG, r["citation"])
                self.assertEqual(out["rtl"]["name"], r["name"])
                self.assertEqual(out["rtl"]["kind"], r["name_kind"])
                n += 1
        self.assertEqual(n, 104)

    def test_the_cli_prints_the_rtl_block(self):
        out = subprocess.run(
            [sys.executable, str(REPO / "tools" / "re_query.py"),
             "resolve", "0f78:114b"], capture_output=True, text=True)
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertIn("rtl:", out.stdout)
        self.assertIn("Random", out.stdout)
        self.assertIn("borland", out.stdout)


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
