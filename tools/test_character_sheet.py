#!/usr/bin/env python3
"""`data/character_sheet.json` re-derived from `orig/g.exe`, claim by claim.

The artifact and `docs/re/character-sheet.md` are the two places the same
claims live; this is what stops them drifting from the binary they describe.
Nothing here reads `src/`, a screen, or Ghidra's C.

Every address in the artifact is checked two ways, because they are different
signals and only the second one settles anything (`docs/re/METHODOLOGY.md`,
"Is this address a call site?"):

  * **alignment** -- the address is reached by decoding forward from its
    enclosing function's entry, so it is a real instruction boundary and not a
    byte-scan hit in the middle of one;
  * **identity** -- the instruction decoded there says what the artifact says
    it says.

    python3 tools/test_character_sheet.py
"""
import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import addr as addrmod            # noqa: E402
import dis16                      # noqa: E402
# The four scans and the two markdown helpers live in one place now, because
# tools/test_combat_dispatch.py needs the same ones and each encodes a mistake
# already made once.  Re-exported under their old names so nothing else here
# changes.
from re_derive import (CITE, aligned_boundaries, far_calls_to,  # noqa: E402,F401
                       inline_spans, load_image, near_calls_to, strip_fences)

REPO = Path(__file__).resolve().parents[1]
ART = REPO / "data" / "character_sheet.json"
BRANCHES = REPO / "data" / "branches.json"

DOC = REPO / "docs" / "re" / "character-sheet.md"

# `1000:248f` is the EXCLUSIVE end of the range `[1000:1a03, 1000:248f)`, so it
# is one past the last instruction and cannot be a boundary.  It is the only
# address in the prose that is not meant to be one, and naming it here is what
# keeps "every other citation resolves" a real assertion rather than a
# tolerance.
NOT_A_BOUNDARY = {"1000:248f": "the exclusive end of the function's byte range"}


class SheetTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.art = json.loads(ART.read_text(encoding="utf-8"))
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.funcs = {f["entry"]: f for f in cls.branches["functions"]}
        # Decode every function that holds a cited address, once, from its
        # entry.  The keys of `cls.aligned` are exactly the instruction
        # boundaries an aligned walk reaches.
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        cls.insn = cls.aligned

    # ---------------------------------------------------------------- helpers
    def at(self, cit):
        """The instruction at `cit`, requiring it to be an aligned boundary."""
        self.assertIn(cit, self.aligned,
                      "%s is not an instruction boundary reached by decoding "
                      "forward from any enclosing function's entry -- the "
                      "citation is a byte-scan hit, not an address" % cit)
        return self.aligned[cit]

    def check_insn(self, rec, where):
        ins = self.at(rec["addr"])
        self.assertEqual(
            ins.text, rec["text"],
            "%s: data/character_sheet.json says %s at %s, orig/g.exe decodes "
            "%s there" % (where, rec["text"], rec["addr"], ins.text))
        return ins

    def cs_literal(self, cs_offset):
        off = int(cs_offset, 16)
        n = self.img[off]
        return self.img[off + 1:off + 1 + n].decode("cp866")

    def ds_literal(self, ds_offset):
        off = addrmod.DATA_SEG_IMAGE_OFF + ds_offset
        n = self.img[off]
        return self.img[off + 1:off + 1 + n].decode("cp866")

    def walk_records(self):
        """Every `{addr, text}` pair anywhere in the artifact, with its path."""
        def rec(node, path):
            if isinstance(node, dict):
                if isinstance(node.get("addr"), str) \
                        and isinstance(node.get("text"), str):
                    yield node, path
                for k, v in node.items():
                    yield from rec(v, "%s.%s" % (path, k))
            elif isinstance(node, list):
                for i, v in enumerate(node):
                    yield from rec(v, "%s[%d]" % (path, i))
        return list(rec(self.art, "$"))

    # ------------------------------------------------------------------ tests
    def test_every_cited_instruction_decodes_to_what_the_artifact_says(self):
        seen = self.walk_records()
        self.assertGreater(len(seen), 80,
                           "the artifact stopped carrying instruction records; "
                           "a walk that finds nothing must not pass")
        for node, path in seen:
            self.check_insn(node, path)

    def test_every_cs_literal_decodes_to_the_recorded_text(self):
        found = 0
        for node, path in self._literal_nodes():
            self.assertEqual(
                self.cs_literal(node["cs_offset"]), node["text"],
                "%s: the Pascal shortstring at CS %s is not what the artifact "
                "records" % (path, node["cs_offset"]))
            found += 1
        self.assertGreaterEqual(found, 40, "literal walk found only %d" % found)

    def _literal_nodes(self):
        def rec(node, path):
            if isinstance(node, dict):
                if isinstance(node.get("cs_offset"), str) \
                        and isinstance(node.get("text"), str):
                    yield node, path
                for k, v in node.items():
                    yield from rec(v, "%s.%s" % (path, k))
            elif isinstance(node, list):
                for i, v in enumerate(node):
                    yield from rec(v, "%s[%d]" % (path, i))
        return list(rec(self.art, "$"))

    def test_the_function_takes_no_parameters(self):
        fn = self.art["function"]
        entry = addrmod.image_off_of_citation(fn["entry"])
        body = dis16.decode_run(self.img, entry, entry + fn["size_bytes"])
        last = body[-1]
        self.assertEqual(last.raw, b"\xc3",
                         "the last instruction is %r, not a bare `ret`; a "
                         "`ret imm16` would mean the callee pops parameters"
                         % last.text)
        self.assertEqual(fn["parameter_bytes"], 0,
                         "the artifact records %r parameter bytes, but the "
                         "epilogue's bare `ret` pops none"
                         % fn["parameter_bytes"])
        # The independent half: nothing in the body reads a POSITIVE bp
        # displacement, which is where a Turbo Pascal parameter would live.
        positive = [("1000:%04x" % i.off, i.text) for i in body
                    if re.search(r"bp\+0x", i.text)]
        self.assertEqual(
            positive, [],
            "the body reads a positive bp displacement, so it DOES take a "
            "parameter: %r" % positive[:5])
        # ... and `ax` is written before anything could read an incoming value.
        self.assertTrue(body[2].text.startswith("mov ax,"),
                        "the third instruction is %r, not the `mov ax,imm` the "
                        "no-register-argument claim rests on" % body[2].text)

    def test_the_call_sites_really_call_this_function(self):
        sites = self.art["call_sites"]
        self.assertEqual(len(sites), 4)
        target = addrmod.image_off_of_citation(self.art["function"]["entry"])
        for cs in sites:
            ins = self.at(cs["addr"])
            self.assertEqual(ins.raw[0], 0xE8,
                             "%s is not a near call: %s" % (cs["addr"], ins.text))
            disp = int.from_bytes(ins.raw[1:3], "little", signed=True)
            self.assertEqual(
                (ins.end + disp) & 0xFFFF, target & 0xFFFF,
                "%s does not target %s" % (cs["addr"], self.art["function"]["entry"]))
        # And the whole image holds no OTHER call to it -- neither a fifth near
        # call nor a far one.  This is what makes "exactly these four" a
        # finding rather than a search that stopped early.
        near = near_calls_to(self.img, target)
        self.assertEqual(sorted(near), sorted(cs["addr"] for cs in sites),
                         "the byte scan finds near calls the artifact does not "
                         "list (or the reverse): %r" % near)
        far = far_calls_to(self.img, target)
        self.assertEqual(far, [], "a far call to the sheet exists at %r" % far)

    def test_sv_calls_a_different_function(self):
        nm = self.art["near_miss"]
        ins = self.at(nm["addr"])
        disp = int.from_bytes(ins.raw[1:3], "little", signed=True)
        self.assertEqual(
            "1000:%04x" % ((ins.end + disp) & 0xFFFF), nm["target"],
            "the `sv` arm no longer calls %s -- the refutation this artifact "
            "records rests on it" % nm["target"])
        self.assertNotEqual(nm["target"], self.art["function"]["entry"])

    def test_the_flag_lines_print_inside_the_arm_their_guard_selects(self):
        lines = self.art["flag_lines"]
        self.assertEqual(len(lines), 30)
        ds_seen = set()
        for f in lines:
            guard = self.check_insn(f["guard"], f["label"])
            ds_off = int(f["ds"].split(":")[1], 16)
            self.assertIn(
                "0x%x" % ds_off, guard.text,
                "%s: the guard at %s does not read %s"
                % (f["label"], f["guard"]["addr"], f["ds"]))
            self.assertNotIn(f["ds"], ds_seen, "%s is guarded twice" % f["ds"])
            ds_seen.add(f["ds"])
            branch = self.check_insn(f["branch"], f["label"])
            self.check_insn(f["literal_push"], f["label"])
            start = addrmod.image_off_of_citation(f["branch"]["arm_start"])
            end = addrmod.image_off_of_citation(f["branch"]["arm_end"])
            push = addrmod.image_off_of_citation(f["literal_push"]["addr"])
            self.assertLess(start, end, "%s: empty arm" % f["label"])
            self.assertTrue(
                start <= push < end,
                "%s: the literal push at %s is not inside the arm %s..%s that "
                "the guard at %s selects -- the label is attributed to the "
                "wrong flag" % (f["label"], f["literal_push"]["addr"],
                                f["branch"]["arm_start"], f["branch"]["arm_end"],
                                f["guard"]["addr"]))
            tgt = int(branch.text.split()[-1], 16)
            if f["branch"]["arm"] == "fallthrough":
                self.assertEqual(branch.end, start)
                self.assertEqual(tgt, end)
            else:
                self.assertEqual(tgt, start)
            # the pushed literal is the one the record names
            self.assertIn(f["literal"]["cs_offset"].replace("0x0", "0x"),
                          f["literal_push"]["text"].replace("0x0", "0x"),
                          "%s: %s does not push %s"
                          % (f["label"], f["literal_push"]["addr"],
                             f["literal"]["cs_offset"]))

    def test_the_two_table_lookups_index_the_documented_tables(self):
        for t in self.art["table_lookups"]:
            load = self.check_insn(t["load"], t["table"])
            self.assertIn("0x%x" % int(t["index_ds"].split(":")[1], 16), load.text)
            self.check_insn(t["shift"], t["table"])
            base = self.check_insn(t["base"], t["table"])
            base_ds = int(t["table_base_ds"].split(":")[1], 16)
            self.assertIn("0x%x" % base_ds, base.text,
                          "%s: %s does not add the table base %s"
                          % (t["table"], t["base"]["addr"], t["table_base_ds"]))
            self.assertEqual(t["stride"], 256)
            for s in t["samples"]:
                self.assertEqual(
                    self.ds_literal(base_ds + s["index"] * t["stride"]), s["text"],
                    "%s[%d] is not %r" % (t["table"], s["index"], s["text"]))

    def test_the_branch_partition_is_the_whole_branch_set(self):
        part = self.art["branch_partition"]
        B = [b for b in self.branches["branches"] if b["func"] == "FUN_1000_1a03"]
        self.assertEqual(part["total"], len(B))
        self.assertEqual(part["total"], self.art["function"]["branch_count"])
        # The partition itself is EXCLUDED from the scan.  Leaving it in makes
        # the check tautological in the worst way: the `uncited` list would
        # cite every address on it, and the partition would then agree with a
        # recomputation that its own contents produced.  Caught by this test
        # while it was being written.
        without = {k: v for k, v in self.art.items() if k != "branch_partition"}
        text = json.dumps(without, ensure_ascii=False)
        cited = {m.group(0).lower() for m in CITE.finditer(text)}
        want_cited, want_uncited = [], []
        for b in B:
            hit = (b["addr"].lower() in cited
                   or bool(b["guard"] and b["guard"]["addr"].lower() in cited))
            (want_cited if hit else want_uncited).append(b["addr"])
        self.assertEqual(part["cited"], want_cited,
                         "the recorded cited set is not what the artifact "
                         "actually cites")
        self.assertEqual(part["uncited"], want_uncited,
                         "the recorded uncited set is not what is actually "
                         "left uncited")
        self.assertEqual(sorted(part["cited"] + part["uncited"]),
                         sorted(b["addr"] for b in B))
        self.assertEqual(set(part["cited"]) & set(part["uncited"]), set())

    def test_the_function_calls_nothing_in_its_own_segment(self):
        fn = self.art["function"]
        entry = addrmod.image_off_of_citation(fn["entry"])
        body = dis16.decode_run(self.img, entry, entry + fn["size_bytes"])
        near_calls = [i.text for i in body if i.raw[:1] == b"\xe8"]
        self.assertEqual(
            near_calls, [],
            "the sheet makes a near call, so it does NOT only reach the "
            "runtime: %r" % near_calls[:5])
        self.assertTrue(fn["calls_leave_segment_1000"])
        segs = {i.text.split()[1].split(":")[0] for i in body
                if i.raw[:1] == b"\x9a"}
        self.assertEqual(segs, {"0xeed", "0xf78"},
                         "the far calls leave for segments %r, not the two "
                         "runtime segments the artifact claims" % segs)


    def test_the_live_probe_agrees_with_the_static_call_sites(self):
        """Which verbs the breakpoint saw must be which verbs the code calls.

        This is the one check that crosses the two evidence lanes.  The
        disassembly says exactly two typed verbs reach the sheet -- `s` at the
        top-level prompt (1000:ec82/ec89) and `s` at the `Битва\\` prompt
        (1000:4c2e/4c35) -- so the predicted answer for any OTHER line is "does
        not reach".  The probe drove seven distinct (prompt, line) pairs; every
        one of them has to land where the static reading puts it, or one of the
        two lanes is wrong.
        """
        by_prompt = {"street": "entry", "combat": "FUN_1000_3d11"}
        verbs = {}
        for cs in self.art["call_sites"]:
            if cs["kind"] == "verb":
                verbs.setdefault(cs["in_function"], set()).add(cs["literal"]["text"])
        probe = self.art["live_probe"]["per_verb"]
        self.assertGreaterEqual(len(probe), 5,
                                "the probe covers too few verbs to separate "
                                "`these two reach it` from `everything does`")
        self.assertTrue(any(not v["reaches"] for v in probe),
                        "the probe recorded no negative at all, which cannot "
                        "distinguish two verbs reaching it from all of them")
        self.assertTrue(any(v["reaches"] for v in probe))
        for v in probe:
            fn = by_prompt[v["prompt"]]
            predicted = v["line"] in verbs.get(fn, set())
            self.assertEqual(
                predicted, v["reaches"],
                "%r typed at the %s prompt: the disassembly predicts "
                "reaches=%s (the verbs %s calls the sheet for are %s) and the "
                "breakpoint at 1000:1a03 observed reaches=%s over %d prompt(s)"
                % (v["line"], v["prompt"], predicted, fn,
                   sorted(verbs.get(fn, set())), v["reaches"], v["prompts"]))
            self.assertEqual(v["reaches"], v["entries"] > 0)
        # `stats` -- the verb the Task 16 hypothesis named -- is not a byte
        # sequence the image contains at all, so no dispatcher can match it.
        self.assertNotIn(b"stats", self.img,
                         "`stats` now appears in the image; the claim that the "
                         "hypothesis named a verb that does not exist would "
                         "have to be re-derived")


class ScanTest(unittest.TestCase):
    """The two byte scans, shown able to find something.

    `test_the_call_sites_really_call_this_function` asserts each scan returns
    exactly the expected set over the SHIPPED image, and over that image the
    far-call answer is the empty list -- an assertion that passes whether the
    scan works or not.  These run the same functions over a doctored copy that
    really does contain the pattern, so "no far call exists" is a measurement
    rather than a scan that never matched anything.
    """

    def setUp(self):
        self.img = load_image()

    def test_the_near_scan_wraps_at_64k(self):
        found = near_calls_to(self.img, 0x1A03)
        self.assertIn("1000:ec89", found)     # 0xec8c + 0x2d77 == 0x11a03
        self.assertIn("1000:ee36", found)     # 0xee39 + 0x2bca == 0x11a03
        self.assertIn("1000:4c35", found)     # negative rel16, no wrap
        self.assertIn("1000:512b", found)
        self.assertEqual(len(found), 4)

    def test_the_near_scan_finds_a_planted_call(self):
        doctored = bytearray(self.img)
        at = 0x0100
        disp = (0x1A03 - (at + 3)) & 0xFFFF
        doctored[at:at + 3] = bytes([0xE8]) + disp.to_bytes(2, "little")
        self.assertIn("1000:0100", near_calls_to(bytes(doctored), 0x1A03))

    def test_the_far_scan_is_empty_on_the_shipped_image(self):
        self.assertEqual(far_calls_to(self.img, 0x1A03), [])

    def test_the_far_scan_finds_a_planted_call_under_any_segment(self):
        # Both segment words: 0x0000 is the game code's relative segment, and
        # 0x1a0 is what the old `target // 16` expression compared against.
        # Neither is filtered now, so a plant under either is found.
        for seg in (0x0000, 0x01A0, 0x0F78):
            doctored = bytearray(self.img)
            doctored[0x0100:0x0105] = (b"\x9a" + (0x1A03).to_bytes(2, "little")
                                       + seg.to_bytes(2, "little"))
            self.assertEqual(far_calls_to(bytes(doctored), 0x1A03),
                             ["1000:0100"], "segment 0x%04x" % seg)


class ProseTest(unittest.TestCase):
    """`docs/re/character-sheet.md` re-derived from `orig/g.exe`.

    The artifact half of the two-places rule was checked from the start; the
    PROSE half was not, and `tools/test_character_sheet.py` did not open the
    `.md` at all.  That is exactly where the self-disclosed
    `1000:584a` -> `1000:5849` drift lived: 27 of the doc's citations -- the
    whole rector-victory arm, `1000:ae36`, the `v`/`x`/`wes` compare sites --
    appear only in prose and had no net under them.  These three tests are that
    net, so the sentence at the top of the document is now true.
    """

    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        cls.art = json.loads(ART.read_text(encoding="utf-8"))
        cls.md = strip_fences(DOC.read_text(encoding="utf-8"))
        cls.spans = inline_spans(cls.md)

    def cs_literal(self, off):
        n = self.img[off]
        return self.img[off + 1:off + 1 + n].decode("cp866")

    def known_literals(self):
        """Every literal the doc or the artifact anchors to an address."""
        offs = {int(m.group(1), 16)
                for m in re.finditer(r"CS `0x([0-9a-f]{4})`", self.md)}

        def walk(node, key):
            if isinstance(node, dict):
                if isinstance(node.get(key), str):
                    yield node[key]
                for v in node.values():
                    yield from walk(v, key)
            elif isinstance(node, list):
                for v in node:
                    yield from walk(v, key)
        offs |= {int(o, 16) for o in walk(self.art, "cs_offset")}
        return {self.cs_literal(o) for o in offs} | set(walk(self.art, "text"))

    def test_every_prose_address_is_an_instruction_boundary(self):
        cites = sorted({m.group(0) for m in CITE.finditer(self.md)})
        self.assertGreater(len(cites), 100,
                           "the prose scan found only %d citations; a scan "
                           "that finds nothing must not pass" % len(cites))
        bad = [c for c in cites
               if c not in self.aligned and c not in NOT_A_BOUNDARY]
        self.assertEqual(
            bad, [],
            "docs/re/character-sheet.md cites %r, which an aligned decode from "
            "every segment-1000 function entry never reaches -- so it is a "
            "byte offset, not an address" % bad)
        for c, why in NOT_A_BOUNDARY.items():
            if c in cites:
                self.assertNotIn(c, self.aligned,
                                 "%s is exempted as %s but IS a boundary; the "
                                 "exemption has gone stale" % (c, why))

    def test_every_prose_instruction_says_what_the_binary_says(self):
        checked = 0
        for span in self.spans:
            m = re.match(r"^(1000:[0-9a-f]{4})\s+([a-z].*)$", span)
            if not m:
                continue
            cit, text = m.groups()
            self.assertIn(cit, self.aligned, "%r: not a boundary" % span)
            checked += 1
            self.assertEqual(
                self.aligned[cit].text, text,
                "docs/re/character-sheet.md writes `%s %s`, but tools/dis16.py "
                "decodes %r there" % (cit, text, self.aligned[cit].text))
        self.assertGreaterEqual(
            checked, 20,
            "only %d `1000:xxxx <instruction>` spans found in the prose; the "
            "pattern has drifted and this test is checking almost nothing"
            % checked)

    def test_every_prose_literal_comes_out_of_the_binary(self):
        # 1. every CS offset the prose names decodes to a real shortstring
        offs = [int(m.group(1), 16)
                for m in re.finditer(r"CS `0x([0-9a-f]{4})`", self.md)]
        self.assertGreaterEqual(len(offs), 25, "only %d CS offsets" % len(offs))
        for o in offs:
            self.assertTrue(self.img[o], "CS 0x%04x has a zero length byte" % o)
            self.cs_literal(o)          # raises on undecodable bytes
        # 2. every `TEXT` (CS `0x....`) pair matches byte for byte.  The
        #    negative lookahead keeps the ADDRESS span before such a pair from
        #    being read as its text.
        pairs = re.findall(r"`((?!1000:)[^`]+)`\s*\(CS `0x([0-9a-f]{4})`\)",
                           self.md, re.S)
        self.assertGreaterEqual(len(pairs), 20, "only %d pairs" % len(pairs))
        for text, off in pairs:
            self.assertEqual(
                self.cs_literal(int(off, 16)), text,
                "the prose quotes %r beside CS 0x%s, which holds %r"
                % (text, off, self.cs_literal(int(off, 16))))
        # 3. every run of Cyrillic inside inline code is part of some literal
        #    the doc or the artifact anchors.  Runs, not whole spans, because
        #    the prose legitimately quotes a label without its `^N` colour
        #    prefix and writes things like `class 3 -> Подтсан`.
        known = self.known_literals()
        unmatched = sorted({run for span in self.spans
                            for run in re.findall(r"[\u0400-\u04ff]+", span)
                            if not any(run in k for k in known)})
        self.assertEqual(
            unmatched, [],
            "Russian in docs/re/character-sheet.md that matches no literal in "
            "orig/g.exe at any address the doc or the artifact names: %r"
            % unmatched)


if __name__ == "__main__":
    unittest.main(verbosity=2)
