#!/usr/bin/env python3
"""`data/shop_arms.json` and `docs/re/shop-arms.md` re-derived from `orig/g.exe`.

The artifact and the prose are the two places the same claims about the `bmar`
row-1..6 PURCHASE arms live; this is what stops either drifting from the binary
it describes.  Nothing here reads `src/`, a screen, or Ghidra's C.
`tools/test_character_sheet.py` is the model, and the same two signals are kept
separate for the same reason (`docs/re/METHODOLOGY.md`, "Is this address a call
site?"):

  * **alignment** -- the address is reached by decoding forward from its
    enclosing function's entry, so it is a real instruction boundary and not a
    byte-scan hit in the middle of one;
  * **identity** -- the instruction decoded there says what the artifact says
    it says.

Five claims here are not restatements of a decode and get their own checks,
because each is the kind of thing that reads authoritative and is easy to get
wrong.  Three of them are INVENTORY claims, and every one of those is asserted
by SET EQUALITY against a sweep of the binary -- never by checking that the
listed entries hold up.  That distinction is the whole point: fix round 1
found this file shipping an inventory of two district gates where the binary
has five, and every listed entry checked out.

  * **`strings[]` is complete.**  Every `mov di,imm16` inside a row's span that
    is followed by `push cs` / `push di` -- the CS-literal push idiom -- must be
    either the row's key literal or one of its recorded strings, and every
    recorded string must be one of those pushes.  So "this arm prints nothing
    else" is a measurement over the span, not a list someone stopped writing.
  * **`gates[]` is complete.**  Every conditional branch in a row's span must
    be accounted for by the artifact -- as the miss branch, a gate branch, a
    conjunct, an effect guard or a `Random` arm.  The strings sweep cannot
    catch a SILENT omitted gate, and "no gate in rows 1..6 refuses silently"
    is a headline claim of `docs/re/shop-arms.md`.
  * **the district-gate inventory is complete.**  Set equality against every
    `[0x3692]` operand in `1000:c4be`..`1000:ccd8`, plus a raw `92 36` count
    over the same range.
  * **no arm tests the district.**  The whole `1000:c8ce`..`1000:ccc4` span is
    decoded as one aligned run and searched for an operand equal to `0x3692`,
    and the raw byte pair `92 36` is counted over the same span so the negative
    does not rest on the decoder alone.  `1000:ccc4`..`1000:ce80`, Task 18's
    three arms, is measured the same way: the divergence is the whole buy path.
  * **the club-without-knuckles bug.**  `1000:cc69`'s not-taken path must reach
    the confirmation push with no `add` on it, while the loot arm's
    `1000:55d8` -- the same guard on the same flag, granting the same item --
    lands on `add word [0x38a8],0x4`.  The bug is the DIFFERENCE between two
    sites in one binary, so both halves are re-derived, and the adds are
    pinned by ADDRESS inside row 6's span rather than by decoded text (the
    loot arm spells the same two instructions).

    python3 tools/test_shop_arms.py
"""
import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import addr as addrmod            # noqa: E402
import dis16                      # noqa: E402
import re_query                   # noqa: E402
from re_derive import (CITE, aligned_boundaries, load_image,  # noqa: E402
                       inline_spans, strip_fences)

REPO = Path(__file__).resolve().parents[1]
ART = REPO / "data" / "shop_arms.json"
SHOPS = REPO / "data" / "shops.json"
BRANCHES = REPO / "data" / "branches.json"
DOC = REPO / "docs" / "re" / "shop-arms.md"

#: The player's money.  Every arm debits it, and it is referenced 107 times
#: across the image -- `docs/re/tables.md`'s "Other price sources" already owns
#: that population, so `globals[]` does not restate it per row.
MONEY = "20ae:38c7"

#: Addresses the prose names that are deliberately NOT instruction boundaries.
#: Empty here on purpose: every address `docs/re/shop-arms.md` cites -- the
#: exclusive span end `1000:ccc4` included, because that address IS the row-7
#: setup's first instruction -- is a real boundary, so there is no exemption to
#: keep honest.  Kept as a named constant so adding one is a visible edit.
NOT_A_BOUNDARY = {}


def data_off(cit):
    """Image offset of a `20ae:xxxx` DGROUP citation."""
    return addrmod.image_off_of_citation(cit)


class ArmsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.hdr = addrmod.header_bytes(addrmod.read_exe())
        cls.art = json.loads(ART.read_text(encoding="utf-8"))
        cls.shops = json.loads(SHOPS.read_text(encoding="utf-8"))
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        cls.prog = re_query.Program()
        cls._xref_cache = {}

    # ---------------------------------------------------------------- helpers
    def at(self, cit):
        self.assertIn(cit, self.aligned,
                      "%s is not an instruction boundary reached by decoding "
                      "forward from any enclosing function's entry -- the "
                      "citation is a byte-scan hit, not an address" % cit)
        return self.aligned[cit]

    def check_insn(self, rec, where):
        ins = self.at(rec["addr"])
        self.assertEqual(
            ins.text, rec["text"],
            "%s: data/shop_arms.json says %s at %s, orig/g.exe decodes %s "
            "there" % (where, rec["text"], rec["addr"], ins.text))
        return ins

    def cs_literal(self, cs_offset):
        off = int(cs_offset, 16)
        n = self.img[off]
        return self.img[off + 1:off + 1 + n].decode("cp866")

    def rows(self):
        return self.art["rows"]

    def span(self, row):
        return (addrmod.image_off_of_citation(row["span"]["start"]),
                addrmod.image_off_of_citation(row["span"]["end"]))

    def xrefs(self, ds):
        if ds not in self._xref_cache:
            self._xref_cache[ds] = re_query.xrefs_to(self.prog, ds)["scan"]
        return self._xref_cache[ds]

    def insn_records(self):
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

    def literal_records(self):
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

    def branch_target(self, ins):
        """The image offset a decoded near/short branch jumps to."""
        m = re.search(r"0x([0-9a-f]+)$", ins.text)
        self.assertIsNotNone(m, "%r is not a branch this test can read"
                             % ins.text)
        assert m is not None          # the unittest assert above already
        return int(m.group(1), 16)    # raised; this is for the type checker

    # ------------------------------------------------------------------ tests
    def test_every_cited_instruction_decodes_to_what_the_artifact_says(self):
        seen = self.insn_records()
        self.assertGreater(len(seen), 100,
                           "the artifact stopped carrying instruction records; "
                           "a walk that finds nothing must not pass (found %d)"
                           % len(seen))
        for node, path in seen:
            self.check_insn(node, path)

    def test_every_literal_decodes_to_the_recorded_text(self):
        seen = self.literal_records()
        self.assertGreaterEqual(len(seen), 25,
                                "literal walk found only %d" % len(seen))
        for node, path in seen:
            cs = node["cs_offset"]
            self.assertEqual(
                self.cs_literal(cs), node["text"],
                "%s: the Pascal shortstring at CS %s is not what the artifact "
                "records (%r there, %r recorded)"
                % (path, cs, self.cs_literal(cs), node["text"]))
            self.assertEqual(
                int(node["file_offset"], 16), int(cs, 16) + self.hdr,
                "%s: file_offset %s is not CS %s plus the 0x%x-byte MZ header "
                "-- the two address forms have been mixed"
                % (path, node["file_offset"], cs, self.hdr))

    def test_the_six_rows_are_the_six_bmar_rows_the_menu_prints(self):
        rows = self.rows()
        self.assertEqual([r["key"] for r in rows], list("123456"))
        self.assertEqual({r["shop"] for r in rows}, {"bmar"})
        menu = {r["key"]: r for r in self.shops if r["shop"] == "bmar"}
        for r in rows:
            m = menu[r["key"]]
            self.assertEqual(
                r["price"], m["price"],
                "row %s: shop_arms.json says %d, data/shops.json says %d"
                % (r["key"], r["price"], m["price"]))
            self.assertEqual(
                m["price"], m["displayed_price"],
                "row %s now has a price/displayed_price split like the "
                "silencer's; the arms doc claims rows 1..6 have none"
                % r["key"])
            self.assertIn(
                r["item"], m["text"],
                "row %s: the artifact's item label %r is not a substring of "
                "the menu literal %r, so it is a name someone invented"
                % (r["key"], r["item"], m["text"]))

    def test_each_key_compare_is_the_shortstring_compare_on_the_typed_buffer(self):
        buf = int(self.art["input_read"]["buffer"].split(":")[1], 16)
        for r in self.rows():
            where = "row %s" % r["key"]
            cmp_ins = self.check_insn(r["compare"], where)
            self.assertEqual(r["compare_addr"], r["compare"]["addr"])
            self.assertEqual(
                cmp_ins.raw, b"\x9a\xd8\x0b\x78\x0f",
                "%s: %s is not the `call 0f78:0bd8` shortstring compare (%s)"
                % (where, r["compare_addr"], cmp_ins.hex()))
            # the six-instruction push idiom in front of it
            start, _ = self.span(r)
            run = dis16.decode_run(self.img, start, cmp_ins.off)
            texts = [i.text for i in run]
            self.assertEqual(
                texts,
                ["mov di,0x%x" % buf, "push ds", "push di",
                 "mov di,%s" % r["key_literal"]["cs_offset"],
                 "push cs", "push di"],
                "%s: the six instructions before the compare are not the "
                "buffer/literal push idiom: %r" % (where, texts))
            self.assertEqual(
                self.cs_literal(r["key_literal"]["cs_offset"]), r["key"],
                "%s: the literal it compares against is not %r"
                % (where, r["key"]))

    def test_each_row_hands_the_line_on_to_the_next_row(self):
        rows = self.rows()
        for a, b in zip(rows, rows[1:]):
            miss = self.check_insn(a["miss_branch"], "row %s" % a["key"])
            self.assertEqual(
                a["miss_branch"]["target"], b["span"]["start"],
                "row %s's miss branch does not hand off to row %s's setup"
                % (a["key"], b["key"]))
            self.assertEqual(a["span"]["end"], b["span"]["start"])
            del miss
        # the last row hands off to the row-7 setup, which is Task 18's block:
        # the far call four bytes past it compares the literal `7`.
        last = rows[-1]
        self.assertEqual(last["miss_branch"]["target"], last["span"]["end"])
        seven_setup = addrmod.image_off_of_citation(last["span"]["end"])
        run = dis16.decode_run(self.img, seven_setup, seven_setup + 0x10)
        lit = [i for i in run if i.raw[:1] == b"\xbf"][1]
        self.assertEqual(
            self.cs_literal("0x%04x" % int(lit.text.split(",")[1], 16)), "7",
            "the instruction stream after row 6 does not set up the `7` "
            "compare, so the row-1..6 span is not bounded where it says")

    def test_the_gate_reads_the_price_the_debit_takes(self):
        for r in self.rows():
            where = "row %s" % r["key"]
            price_off = int(r["price_addr"].split(":")[1], 16)
            self.assertEqual(
                self.img[data_off(r["price_addr"])], r["price"],
                "%s: the price byte at %s is %d, not %d"
                % (where, r["price_addr"], self.img[data_off(r["price_addr"])],
                   r["price"]))
            afford = [g for g in r["gates"] if g["kind"] == "afford"]
            self.assertEqual(len(afford), 1, "%s: %d afford gates"
                             % (where, len(afford)))
            g = afford[0]
            self.check_insn(g["load"], where)
            self.assertEqual(g["load"]["text"], "mov al,[0x%x]" % price_off)
            test = self.check_insn(g["test"], where)
            self.assertEqual(test.text, "cmp ax,[0x%x]"
                             % int(MONEY.split(":")[1], 16))
            branch = self.check_insn(g["branch"], where)
            self.assertEqual(
                branch.raw[0], 0x7E,
                "%s: the affordability branch at %s is %s, not the `jle` the "
                "artifact records the sense from"
                % (where, g["branch"]["addr"], branch.text))
            self.assertEqual("1000:%04x" % self.branch_target(branch),
                             g["pass_target"])
            # the debit reads the SAME price address, two instructions before
            debit = self.at(r["debit_addr"])
            self.assertEqual(debit.text, "sub [0x%x],ax"
                             % int(MONEY.split(":")[1], 16))
            back = dis16.decode_run(
                self.img, addrmod.image_off_of_citation(g["pass_target"]),
                debit.off)
            self.assertEqual(
                [i.text for i in back[-2:]],
                ["mov al,[0x%x]" % price_off, "xor ah,ah"],
                "%s: the debit at %s is not fed by the same price byte the "
                "affordability test read -- that is the silencer's bug shape "
                "and rows 1..6 are claimed not to have it"
                % (where, r["debit_addr"]))
            self.assertEqual(r["debit_addr"],
                             [e["addr"] for e in r["effects"]
                              if e["role"] == "debit"][0])

    def test_no_arm_tests_the_district(self):
        f = self.art["district_finding"]
        lo = addrmod.image_off_of_citation(f["span"]["start"])
        hi = addrmod.image_off_of_citation(f["span"]["end"])
        self.assertEqual(lo, self.span(self.rows()[0])[0])
        self.assertEqual(hi, self.span(self.rows()[-1])[1])
        run = dis16.decode_run(self.img, lo, hi)
        self.assertEqual(
            len(run), f["instruction_count"],
            "the aligned run over %s..%s is %d instructions, not the %d the "
            "artifact records" % (f["span"]["start"], f["span"]["end"],
                                  len(run), f["instruction_count"]))
        district = int(f["district_ds"].split(":")[1], 16)
        hits = ["1000:%04x %s" % (i.off, i.text) for i in run
                for op in i.operands
                if op.kind in ("disp16", "disp16x", "moffs16")
                and op.value == district]
        self.assertEqual(
            hits, [],
            "an arm DOES read the district byte %s: %r -- the whole "
            "`what the port must change` section rests on this being empty"
            % (f["district_ds"], hits))
        want = district.to_bytes(2, "little")
        raw = ["0x%x" % o for o in range(lo, hi - 1)
               if self.img[o:o + 2] == want]
        self.assertEqual(
            raw, [],
            "the byte pair %s occurs in the span at %r; the decoder-based "
            "negative above would then be resting on the decoder alone"
            % (want.hex(" "), raw))
        # ... and the menu-print gates the finding points at DO read it, and
        # are outside every arm.  Without this the negative could be passing
        # because the district byte was misidentified.
        self.assertTrue(f["district_gates_are_menu_only"])
        for g in f["district_gates_are_menu_only"]:
            ins = self.check_insn(g, "district_finding")
            self.assertIn("0x%x" % district, ins.text)
            self.assertFalse(lo <= ins.off < hi,
                             "%s is inside the arm span" % g["addr"])
            self.check_insn(g["branch"], "district_finding")
        # ... and the recorded list is ALL of them.  Without this the checks
        # above prove only that the listed gates check out, never that the
        # list is complete -- which is how an earlier revision of this
        # artifact shipped two of the five and called them "the two district
        # gates that exist in this handler".
        sw = f["district_gates_sweep_range"]
        slo = addrmod.image_off_of_citation(sw["start"])
        shi = addrmod.image_off_of_citation(sw["end"])
        self.assertLess(slo, lo, "the sweep range must start before the arms")
        swept = {"1000:%04x" % i.off
                 for i in dis16.decode_run(self.img, slo, shi)
                 for op in i.operands
                 if op.kind in ("disp16", "disp16x", "moffs16")
                 and op.value == district}
        recorded = {g["addr"] for g in f["district_gates_are_menu_only"]}
        self.assertEqual(
            swept, recorded,
            "the district-gate inventory over %s..%s is not complete: the "
            "binary has %r, the artifact records %r"
            % (sw["start"], sw["end"], sorted(swept), sorted(recorded)))
        # the same sweep by raw bytes, so an omission cannot hide behind a
        # decode that skipped an instruction
        raw_sw = {"0x%x" % (o - 2) for o in range(slo, shi - 1)
                  if self.img[o:o + 2] == want}
        self.assertEqual(
            len(raw_sw), len(recorded),
            "the raw `%s` byte scan over %s..%s finds %d candidate sites and "
            "the artifact records %d gates"
            % (want.hex(" "), sw["start"], sw["end"], len(raw_sw),
               len(recorded)))

    def test_the_rows_7_to_9_arms_carry_no_district_test_either(self):
        """I2's measured half: the divergence is the whole buy path.

        Task 18 mapped those three arms and this task does not re-map them,
        but the district negative has to be measured over them too or the
        port instruction scopes the fix to two rows instead of five.
        """
        f = self.art["district_finding"]
        district = int(f["district_ds"].split(":")[1], 16)
        g = f["rows_7_9_arms_carry_no_district_test_either"]
        lo = addrmod.image_off_of_citation(g["span"]["start"])
        hi = addrmod.image_off_of_citation(g["span"]["end"])
        self.assertEqual(lo, addrmod.image_off_of_citation(
            self.rows()[-1]["span"]["end"]),
            "the rows 7..9 range must start where the rows 1..6 range ends, "
            "or the two measurements leave a hole between them")
        hits = ["1000:%04x %s" % (i.off, i.text)
                for i in dis16.decode_run(self.img, lo, hi)
                for op in i.operands
                if op.kind in ("disp16", "disp16x", "moffs16")
                and op.value == district]
        self.assertEqual(hits, [], "rows 7..9 DO test the district: %r" % hits)
        want = district.to_bytes(2, "little")
        raw = ["0x%x" % o for o in range(lo, hi - 1)
               if self.img[o:o + 2] == want]
        self.assertEqual(raw, [], "the byte pair occurs at %r" % raw)

    def test_the_recorded_gates_are_every_conditional_branch_in_the_arm(self):
        """The `gates[]` analogue of the `strings[]` completeness sweep.

        A refusal literal nobody pushes is caught by the strings sweep; a
        SILENT omitted gate is not, and "no gate in rows 1..6 refuses
        silently" is a headline claim of `docs/re/shop-arms.md`.  So every
        conditional branch in a row's span must be one the artifact records --
        as its miss branch, a gate branch, a conjunct branch, an effect guard
        or a `Random` arm.  Unconditional `jmp`s are excluded by decoding the
        recorded address rather than by trusting its field name.
        """
        total = 0
        for r in self.rows():
            lo, hi = self.span(r)
            swept = {"1000:%04x" % i.off
                     for i in dis16.decode_run(self.img, lo, hi)
                     if 0x70 <= i.raw[0] <= 0x7F
                     or (i.raw[0] == 0x0F and 0x80 <= i.raw[1] <= 0x8F)
                     or i.raw[0] in (0xE0, 0xE1, 0xE2, 0xE3)}
            named = {r["miss_branch"]["addr"]}
            for g in r["gates"]:
                if "branch" in g:
                    named.add(g["branch"]["addr"])
                for c in g.get("conjuncts", []):
                    named.add(c["branch"]["addr"])
                if "fail_branch" in g:
                    named.add(g["fail_branch"]["addr"])
            for e in r["effects"]:
                if e.get("guard_branch"):
                    named.add(e["guard_branch"]["addr"])
            for a in r.get("roll", {}).get("arms", []):
                named.add(a["branch"]["addr"])
            conditional = {a for a in named
                           if self.at(a).raw[0] != 0xEB
                           and self.at(a).raw[0] != 0xE9}
            self.assertEqual(
                swept, conditional,
                "row %s: the conditional branches inside %s..%s are %r, the "
                "artifact accounts for %r -- an unrecorded branch is an "
                "unrecorded gate, and a silent one prints nothing for the "
                "strings sweep to catch"
                % (r["key"], r["span"]["start"], r["span"]["end"],
                   sorted(swept), sorted(conditional)))
            total += len(swept)
        self.assertEqual(
            total, 27,
            "the six arms hold %d conditional branches, not the 27 this "
            "inventory was built over" % total)

    def test_every_recorded_string_is_pushed_inside_its_own_arm(self):
        for r in self.rows():
            lo, hi = self.span(r)
            for s in r["strings"]:
                push = self.check_insn(s["push"], "row %s" % r["key"])
                self.assertEqual(push.text, "mov di,%s" % s["cs_offset"])
                self.assertTrue(
                    lo <= push.off < hi,
                    "row %s: the push of %s at %s is outside the arm's own "
                    "span" % (r["key"], s["cs_offset"], s["push"]["addr"]))

    def test_the_recorded_strings_are_every_literal_the_arm_pushes(self):
        """Completeness, measured -- not a list that stopped being written."""
        for r in self.rows():
            lo, hi = self.span(r)
            run = dis16.decode_run(self.img, lo, hi)
            pushed = set()
            for i, ins in enumerate(run[:-2]):
                if ins.raw[:1] != b"\xbf":
                    continue
                if run[i + 1].text != "push cs" or run[i + 2].text != "push di":
                    continue          # `push ds` -> the DGROUP input buffer
                pushed.add("0x%04x" % int(ins.text.split(",")[1], 16))
            recorded = {s["cs_offset"] for s in r["strings"]}
            recorded.add(r["key_literal"]["cs_offset"])
            self.assertEqual(
                pushed, recorded,
                "row %s: the CS literals actually pushed inside %s..%s are %r, "
                "the artifact records %r -- `strings[]` is not complete"
                % (r["key"], r["span"]["start"], r["span"]["end"],
                   sorted(pushed), sorted(recorded)))

    def test_no_gate_in_rows_1_to_6_refuses_silently(self):
        """Row 9's first two gates print nothing; none of these do.

        Each gate names a refusal literal, every gate's literal is distinct
        from its siblings', and each is one of the arm's recorded strings.  Two
        gates sharing one literal would mean a refusal path was attributed to
        the wrong test.
        """
        for r in self.rows():
            self.assertEqual(r["silent_gates"], [],
                             "row %s records a silent gate" % r["key"])
            offs = [g["refusal_cs_offset"] for g in r["gates"]]
            self.assertTrue(all(offs), "row %s has a gate with no refusal "
                                       "string" % r["key"])
            self.assertEqual(len(set(offs)), len(offs),
                             "row %s: two gates share a refusal literal %r"
                             % (r["key"], offs))
            by_off = {s["cs_offset"]: s for s in r["strings"]
                      if s["role"] == "refusal"}
            for g, off in zip(r["gates"], offs):
                self.assertIn(off, by_off,
                              "row %s gate %r names %s, which is not a "
                              "recorded refusal string"
                              % (r["key"], g["name"], off))
                if "fail_target" in g and g["kind"] != "afford":
                    self.assertEqual(
                        by_off[off]["push"]["addr"], g["fail_target"],
                        "row %s gate %r: the fail target %s is not where its "
                        "refusal literal is pushed"
                        % (r["key"], g["name"], g["fail_target"]))

    def test_every_effect_writes_the_dgroup_address_it_names(self):
        for r in self.rows():
            for e in r["effects"]:
                ins = self.check_insn(e, "row %s effect" % r["key"])
                lo, hi = self.span(r)
                self.assertTrue(lo <= ins.off < hi,
                                "row %s: effect %s is outside the arm"
                                % (r["key"], e["addr"]))
                self.assertIn("0x%x" % int(e["ds"].split(":")[1], 16),
                              ins.text,
                              "row %s: %s does not write %s"
                              % (r["key"], e["addr"], e["ds"]))
                if e.get("guard"):
                    self.check_insn(e["guard"], "row %s guard" % r["key"])
                    self.check_insn(e["guard_branch"],
                                    "row %s guard branch" % r["key"])

    def test_the_globals_an_arm_writes_are_read_elsewhere(self):
        """Brief item 6, recomputed: a flag nothing reads is a finding."""
        checked = 0
        for r in self.rows():
            lo, hi = self.span(r)
            written = {e["ds"] for e in r["effects"] if e["ds"] != MONEY}
            self.assertEqual(
                {g["ds"] for g in r["globals"]}, written,
                "row %s: globals[] and effects[] disagree about which "
                "addresses the arm writes" % r["key"])
            for g in r["globals"]:
                scan = self.xrefs(g["ds"])
                acc = scan["accepted"]
                outside = [a for a in acc
                           if not lo <= int(a["image_off"], 16) < hi]
                self.assertEqual(
                    len(acc), g["xref_count"],
                    "row %s %s: the operand-field scan finds %d references, "
                    "the artifact records %d"
                    % (r["key"], g["ds"], len(acc), g["xref_count"]))
                self.assertEqual(
                    len(outside), g["refs_outside_arm"],
                    "row %s %s: %d references outside the arm, artifact says "
                    "%d" % (r["key"], g["ds"], len(outside),
                            g["refs_outside_arm"]))
                inside = {a["at"] for a in acc} - {a["at"] for a in outside}
                for w in g["written_at"]:
                    self.assertIn(w, inside,
                                  "row %s: %s is recorded as writing %s but "
                                  "the scan does not place it inside the arm"
                                  % (r["key"], w, g["ds"]))
                # a REAL read, not just another store: at least one reference
                # outside the arm whose instruction is not `mov <mem>,imm`.
                reads = [a for a in outside
                         if not re.match(r"mov (byte|word) \[0x[0-9a-f]+\],",
                                         a["text"])]
                self.assertEqual(
                    bool(reads), g["read_elsewhere"],
                    "row %s %s: read_elsewhere=%r but the scan finds %d "
                    "non-store references outside the arm"
                    % (r["key"], g["ds"], g["read_elsewhere"], len(reads)))
                for s in g["sample_read_at"]:
                    self.assertIn(
                        s["addr"], {a["at"] for a in outside},
                        "row %s %s: the recorded reader %s is not in the "
                        "scan's accepted set outside the arm"
                        % (r["key"], g["ds"], s["addr"]))
                    checked += 1
        self.assertGreater(checked, 15,
                           "only %d sample readers checked" % checked)

    def test_the_club_grants_no_damage_without_the_knuckles(self):
        """The bug, as the DIFFERENCE between two sites in one binary."""
        bug = [b for b in self.art["bugs"]
               if b["label"] == "club-without-knuckles-grants-no-damage"][0]
        guard = self.at(bug["where"]["guard"])
        branch = self.at(bug["where"]["branch"])
        self.assertEqual(guard.text, "cmp byte [0x38ba],0x0")
        self.assertEqual(branch.raw[0], 0x74, "not a `jz`: %s" % branch.text)
        skip = self.branch_target(branch)
        # The not-taken path holds exactly the two adds the artifact records --
        # BY ADDRESS, not by text.  Comparing decoded text to decoded text
        # passes just as happily when `adds` names the loot arm's 1000:55da /
        # 1000:55df, which spell the same two instructions; `effects[]` pins
        # its addresses in-span and `bugs[].where.adds` is a separate list
        # with no such pin, so the pin is made here.
        taken = dis16.decode_run(self.img, branch.end, skip)
        row6_lo, row6_hi = self.span(
            [x for x in self.rows() if x["key"] == "6"][0])
        adds = bug["where"]["adds"]
        self.assertEqual(
            ["1000:%04x" % i.off for i in taken], adds,
            "the not-taken path of %s is %r, but the artifact names %r as the "
            "adds the guard skips" % (bug["where"]["branch"],
                                      ["1000:%04x" % i.off for i in taken],
                                      adds))
        for a_ in adds:
            off = addrmod.image_off_of_citation(a_)
            self.assertTrue(
                row6_lo <= off < row6_hi,
                "%s is not inside row 6's span %s..%s, so the bug record is "
                "citing an instruction from somewhere else that happens to "
                "decode the same" % (a_, "1000:%04x" % row6_lo,
                                     "1000:%04x" % row6_hi))
        self.assertEqual([self.at(a_).text for a_ in adds],
                         ["add word [0x38a8],0x2", "add word [0x38aa],0x2"])
        # ... and the taken path lands straight on the confirmation push, so
        # there is NO `add word [0x38a8],0x4` arm the way the loot site has one.
        row6 = [r for r in self.rows() if r["key"] == "6"][0]
        confirm = [s for s in row6["strings"] if s["role"] == "confirmation"][0]
        self.assertEqual(
            "1000:%04x" % skip, confirm["push"]["addr"],
            "the `jz` at %s does not land on the confirmation push; the arm "
            "may have a without-knuckles branch after all"
            % bug["where"]["branch"])
        # the counter-example: same guard, same flag, same item, two branches.
        ce = bug["counter_example_in_the_same_binary"]
        ce_guard = self.check_insn(ce["guard"], "counter-example")
        ce_branch = self.check_insn(ce["branch"], "counter-example")
        self.assertEqual(ce_guard.text, guard.text,
                         "the counter-example does not test the same flag")
        without = self.branch_target(ce_branch)
        self.assertEqual("1000:%04x" % without, ce["without_knuckles"][0]["addr"])
        for rec in ce["with_knuckles"] + ce["without_knuckles"]:
            self.check_insn(rec, "counter-example")
        self.assertEqual([r["text"] for r in ce["without_knuckles"]],
                         ["add word [0x38a8],0x4", "add word [0x38aa],0x4"])
        self.assertNotEqual([r["text"] for r in ce["with_knuckles"]],
                            [r["text"] for r in ce["without_knuckles"]])

    def test_the_better_weapon_gates_are_conjunctions_here(self):
        """Shop: every conjunct must be set to refuse.  Loot: any one does."""
        bug = [b for b in self.art["bugs"]
               if b["label"] == "shop-better-weapon-gate-is-AND-where-loot-is-OR"][0]
        for r in self.rows():
            for g in r["gates"]:
                if g["kind"] != "prereq":
                    continue
                cj = g["conjuncts"]
                proceed = None
                for c in cj[:-1]:
                    br = self.check_insn(c["branch"], "row %s" % r["key"])
                    self.assertEqual(br.raw[0], 0x74,
                                     "row %s: non-final conjunct branch %s is "
                                     "not a `jz` past the refusal"
                                     % (r["key"], c["branch"]["addr"]))
                    tgt = self.branch_target(br)
                    if proceed is None:
                        proceed = tgt
                    self.assertEqual(tgt, proceed,
                                     "row %s: conjuncts jump to different "
                                     "proceed labels" % r["key"])
                last = self.check_insn(cj[-1]["branch"], "row %s" % r["key"])
                fail = g.get("fail_branch")
                if fail:
                    self.assertEqual(self.branch_target(last), proceed)
                    fb = self.check_insn(fail, "row %s" % r["key"])
                    self.assertEqual("1000:%04x" % self.branch_target(fb),
                                     g["fail_target"])
                else:
                    self.assertEqual(last.raw[0], 0x75)
                    self.assertEqual("1000:%04x" % self.branch_target(last),
                                     g["fail_target"])
                self.assertEqual("1000:%04x" % proceed,
                                 r["gates"][r["gates"].index(g) + 1]
                                 ["test"]["addr"],
                                 "row %s: the conjunction's proceed label is "
                                 "not the next gate" % r["key"])
                # and the artifact's per-row conjunct list is the whole of it
                self.assertEqual([c["test"]["addr"] for c in cj],
                                 bug["shop"]["row%s" % r["key"]])
        for name, sites in bug["loot"].items():
            for a in sites:
                ins = self.at(a)
                nxt = self.at("1000:%04x" % ins.end)
                self.assertEqual(
                    nxt.raw[0], 0x75,
                    "the loot %s conjunct at %s is followed by %s, not the "
                    "`jnz <refuse>` that makes it a disjunction"
                    % (name, a, nxt.text))

    def test_the_row_3_roll_is_a_random_4_with_four_arms(self):
        r = [x for x in self.rows() if x["key"] == "3"][0]
        roll = r["roll"]
        call = self.check_insn(roll["call"], "row 3 roll")
        self.assertEqual(call.raw, b"\x9a\x4b\x11\x78\x0f",
                         "%s is not the `Random` far call"
                         % roll["call"]["addr"])
        n = re_query.pushed_n(self.prog, roll["call"]["addr"])
        self.assertEqual(n["n"], roll["n"],
                         "the idiom before %s pushes %r, not %d"
                         % (roll["call"]["addr"], n["n"], roll["n"]))
        self.assertEqual([a["value"] for a in roll["arms"]], [0, 1, 2, 3])
        for a in roll["arms"]:
            ins = self.check_insn(a["compare"], "row 3 arm %d" % a["value"])
            self.assertEqual(ins.text, "cmp ax,0x%x" % a["value"])
            self.check_insn(a["branch"], "row 3 arm %d" % a["value"])
        # every effect that names a Random condition names one of these values
        for e in r["effects"]:
            if e["condition"]:
                self.assertRegex(e["condition"], r"Random\(4\) == [0-3]")


class ProseTest(unittest.TestCase):
    """`docs/re/shop-arms.md` re-derived from `orig/g.exe`.

    The artifact half is checked above; the prose is where a wrong address
    actually propagates, because that is what the next task reads.
    """

    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        cls.art = json.loads(ART.read_text(encoding="utf-8"))
        cls.shops = json.loads(SHOPS.read_text(encoding="utf-8"))
        cls.md = strip_fences(DOC.read_text(encoding="utf-8"))
        cls.spans = inline_spans(cls.md)

    def cs_literal(self, off):
        n = self.img[off]
        return self.img[off + 1:off + 1 + n].decode("cp866")

    def known_literals(self):
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
        known = {self.cs_literal(o) for o in offs}
        known |= set(walk(self.art, "text"))
        # the menu literals, which `tools/test_extract_tables.py` re-derives
        known |= {r["text"] for r in self.shops}
        return known

    def test_every_prose_address_is_an_instruction_boundary(self):
        cites = sorted({m.group(0) for m in CITE.finditer(self.md)})
        self.assertGreater(len(cites), 60,
                           "the prose scan found only %d citations; a scan "
                           "that finds nothing must not pass" % len(cites))
        bad = [c for c in cites
               if c not in self.aligned and c not in NOT_A_BOUNDARY]
        self.assertEqual(
            bad, [],
            "docs/re/shop-arms.md cites %r, which an aligned decode from every "
            "segment-1000 function entry never reaches -- so it is a byte "
            "offset, not an address" % bad)

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
                "docs/re/shop-arms.md writes `%s %s`, but tools/dis16.py "
                "decodes %r there" % (cit, text, self.aligned[cit].text))
        self.assertGreaterEqual(
            checked, 25,
            "only %d `1000:xxxx <instruction>` spans found in the prose; the "
            "pattern has drifted and this test is checking almost nothing"
            % checked)

    def test_every_prose_literal_comes_out_of_the_binary(self):
        offs = [int(m.group(1), 16)
                for m in re.finditer(r"CS `0x([0-9a-f]{4})`", self.md)]
        self.assertGreaterEqual(len(offs), 20, "only %d CS offsets" % len(offs))
        for o in offs:
            self.assertTrue(self.img[o], "CS 0x%04x has a zero length byte" % o)
            self.cs_literal(o)
        pairs = re.findall(r"`((?!1000:)[^`]+)`\s*\(CS `0x([0-9a-f]{4})`\)",
                           self.md, re.S)
        self.assertGreaterEqual(len(pairs), 15, "only %d pairs" % len(pairs))
        for text, off in pairs:
            self.assertEqual(
                self.cs_literal(int(off, 16)), text,
                "the prose quotes %r beside CS 0x%s, which holds %r"
                % (text, off, self.cs_literal(int(off, 16))))
        known = self.known_literals()
        unmatched = sorted({run for span in self.spans
                            for run in re.findall(r"[Ѐ-ӿ]+", span)
                            if not any(run in k for k in known)})
        self.assertEqual(
            unmatched, [],
            "Russian in docs/re/shop-arms.md that matches no literal in "
            "orig/g.exe at any address the doc or the artifact names: %r"
            % unmatched)

    def test_the_prose_and_the_artifact_agree_on_every_row(self):
        """The two places rule, both directions, for the addresses that matter."""
        for r in self.art["rows"]:
            for cit in (r["compare_addr"], r["debit_addr"],
                        r["span"]["start"]):
                self.assertIn(
                    cit, self.md,
                    "docs/re/shop-arms.md never names %s, which "
                    "data/shop_arms.json records for row %s"
                    % (cit, r["key"]))
            for s in r["strings"]:
                self.assertIn(
                    s["cs_offset"], self.md,
                    "row %s: the prose does not carry CS %s"
                    % (r["key"], s["cs_offset"]))


if __name__ == "__main__":
    unittest.main(verbosity=2)
