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

REPO = Path(__file__).resolve().parents[1]
ART = REPO / "data" / "character_sheet.json"
BRANCHES = REPO / "data" / "branches.json"

# The functions whose bodies hold a cited address, and their extents, taken
# from data/branches.json rather than written down here.
CITE = re.compile(r"\b1000:[0-9a-f]{4}\b")


def load_image():
    return addrmod.load_image(addrmod.read_exe())


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
        cls.aligned = {}
        cls.insn = {}
        for entry in ("1000:1a03", "1000:1348", "1000:3d11", "1000:ab59"):
            f = cls.funcs[entry]
            start = addrmod.image_off_of_citation(entry)
            for ins in dis16.decode_run(cls.img, start, start + f["size"]):
                cls.aligned["1000:%04x" % ins.off] = ins
                cls.insn["1000:%04x" % ins.off] = ins

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
        near = []
        for off in range(len(self.img) - 3):
            if self.img[off] == 0xE8:
                d = int.from_bytes(self.img[off + 1:off + 3], "little", signed=True)
                if (off + 3 + d) & 0xFFFF == target & 0xFFFF:
                    near.append("1000:%04x" % off)
        self.assertEqual(sorted(near), sorted(cs["addr"] for cs in sites),
                         "the byte scan finds near calls the artifact does not "
                         "list (or the reverse): %r" % near)
        seg = (target // 16)
        far = [off for off in range(len(self.img) - 5)
               if self.img[off] == 0x9A
               and int.from_bytes(self.img[off + 1:off + 3], "little") == target & 0xFFFF
               and int.from_bytes(self.img[off + 3:off + 5], "little") in (0, seg)]
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

if __name__ == "__main__":
    unittest.main(verbosity=2)
