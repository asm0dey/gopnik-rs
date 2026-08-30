#!/usr/bin/env python3
"""`data/shop_arms.json` and `docs/re/shop-arms.md` re-derived from `orig/g.exe`.

The artifact and the prose are the two places the same claims about the `bmar`
row-1..6 and `mar` row-1..9 PURCHASE arms live; this is what stops either
drifting from the binary it describes.  Nothing here reads `src/`, a screen,
or Ghidra's C.
`tools/test_character_sheet.py` is the model, and the same two signals are kept
separate for the same reason (`docs/re/METHODOLOGY.md`, "Is this address a call
site?"):

  * **alignment** -- the address is reached by decoding forward from its
    enclosing function's entry, so it is a real instruction boundary and not a
    byte-scan hit in the middle of one;
  * **identity** -- the instruction decoded there says what the artifact says
    it says.

Eight claims here are not restatements of a decode and get their own checks,
because each is the kind of thing that reads authoritative and is easy to get
wrong.  SIX of them are INVENTORY claims, and every one of those is asserted
by SET EQUALITY against a sweep of the binary -- never by checking that the
listed entries hold up.  That distinction is the whole point: Task 23's fix
round 1 found this file shipping an inventory of two district gates where the
binary has five, and every listed entry checked out.  Task 25's found four
more inventories that were factually complete and guarded by nothing, which is
the same defect one review earlier.

A sweep is also only as good as its RANGE.  `check_sweep_start` pins each
district sweep to its handler's own verb compare, because
`sweep_start < first_arm` alone let Task 25's review narrow `mar`'s range past
two of its six gates with both district tests still green.

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
  * **`bmar`'s district-gate inventory is complete.**  Set equality against
    every `[0x3692]` operand in `1000:c4be`..`1000:ccd8`, plus a raw `92 36`
    count over the same range.
  * **no `bmar` arm tests the district.**  The whole `1000:c8ce`..`1000:ccc4`
    span is decoded as one aligned run and searched for an operand equal to `0x3692`,
    and the raw byte pair `92 36` is counted over the same span so the negative
    does not rest on the decoder alone.  `1000:ccc4`..`1000:ce80`, Task 18's
    three arms, is measured the same way: the divergence is the whole buy path.
  * **`mar`'s district gates are complete, and the row-7 gap is real.**
    `mar` is the shop where the district DOES reach the buy path, so its
    inventory is a POSITIVE claim, and it can carry the same defect a negative
    one can.  Both halves are swept together over `1000:b94a`..`1000:c31f` --
    the three menu gates and the three buy-path gates -- because the finding is
    the DIFFERENCE between the two sets, and a difference computed from a list
    that stopped being written is worth nothing.  Which MENU lines each menu
    gate covers is measured too (the price-byte operands inside the gate's own
    listed range), and cross-checked against `data/shops.json`, which already
    owns the menu half.
  * **`effects[]` is complete.**  Every instruction in a row's span that
    touches an absolute-memory operand must fall in a WRITE bucket or a READ
    bucket -- an unclassified one fails loudly -- and the WRITE bucket must
    equal `effects[]`.  Two `mar` claims are about the whole set rather than
    the listed entries: row 2's `Random(3)` "changes no state at all", and
    "no global a `mar` arm writes is write-only".
  * **`roll[]` is complete.**  The five-byte `Random` far-call signature is
    swept over each span, because a draw nobody recorded still advances the
    RNG stream and would desynchronise every trace after it.
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

#: The `Random` far call, `call 0f78:114b`, by its exact five bytes.  Used to
#: sweep a span for draws rather than to confirm the recorded ones decode.
RANDOM_CALL = b"\x9a\x4b\x11\x78\x0f"

#: How an instruction that WRITES an absolute-memory operand decodes, and how
#: one that only READS one does.  `tools/dis16.py` carries no read/write flag,
#: so the classification is by decoded text -- and it is the mnemonic that
#: decides, never operand order: `cmp byte [0x38b0],0x1` puts memory first and
#: writes nothing.  The two buckets are asserted EXHAUSTIVE over each span, so
#: a write shape neither describes fails the sweep instead of vanishing from
#: it.  The eleven forms these fifteen spans actually contain are
#: `add [N],ax`, `add byte [N],N`, `add word [N],N`, `inc [N]`, `mov [N],ax`,
#: `mov byte [N],N`, `sub [N],ax` (writes) and `cmp ax,[N]`,
#: `cmp byte [N],N`, `mov al,[N]`, `mov ax,[N]` (reads).  `xchg` is excluded
#: from the READ bucket on purpose: `xchg ax,[0x..]` puts memory SECOND and
#: still writes it, so rather than let it pass as a read it is left
#: unclassified, which fails the sweep instead of silently shrinking it.
WRITES_ABS_MEM = re.compile(
    r"^(mov|add|sub|adc|sbb|and|or|xor|inc|dec|neg|not"
    r"|shl|shr|sar|rol|ror|rcl|rcr|xchg)\s+"
    r"(byte |word |dword )?\[0x[0-9a-f]+\]")
READS_ABS_MEM = re.compile(
    r"^(cmp|test)\s+(byte |word |dword )?\[0x[0-9a-f]+\]"
    r"|^(?!xchg\b)[a-z]{2,5}\s+[a-z]{2,3},"
    r"(byte |word |dword )?\[0x[0-9a-f]+\]")

#: Addresses the prose names that are deliberately NOT instruction boundaries.
#: Empty here on purpose: every address `docs/re/shop-arms.md` cites -- the
#: exclusive span end `1000:ccc4` included, because that address IS the row-7
#: setup's first instruction -- is a real boundary, so there is no exemption to
#: keep honest.  Kept as a named constant so adding one is a visible edit.
NOT_A_BOUNDARY = {}


def data_off(cit):
    """Image offset of a `20ae:xxxx` DGROUP citation."""
    return addrmod.image_off_of_citation(cit)


class _ArtifactBase(unittest.TestCase):
    """Fixtures and helpers shared by `ArmsTest` and `SellTest`.

    Carries no tests of its own: `SellTest` needs the same aligned-boundary
    map, the same decode-check and the same `xrefs-to` cache, and inheriting
    them from `ArmsTest` would re-run all of `ArmsTest`'s cases a second time
    under a second name.
    """

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

    def bmar(self):
        """The `bmar` handler block -- the artifact's top level."""
        return self.art

    def mar(self):
        """The `mar` handler block, added by Task 25."""
        return self.art["mar"]

    def handlers(self):
        return [self.bmar(), self.mar()]

    def rows(self):
        """Every row of BOTH shops.

        Row-level checks are shop-agnostic and run over all fifteen; the
        handler-level ones take their rows from `handlers()` instead, because
        the two shops' district findings are opposite and must not be mixed.
        """
        return [r for h in self.handlers() for r in h["rows"]]

    def span(self, row):
        return (addrmod.image_off_of_citation(row["span"]["start"]),
                addrmod.image_off_of_citation(row["span"]["end"]))

    def xrefs(self, ds):
        if ds not in self._xref_cache:
            self._xref_cache[ds] = re_query.xrefs_to(self.prog, ds)["scan"]
        return self._xref_cache[ds]

    def string_values(self):
        """Every string VALUE anywhere in the artifact, with its path.

        `insn_records` and `literal_records` below walk dicts that carry
        SEPARATE keys.  An address written inside a `note`, a `what`, a
        `claim` or a `named_from` sentence has no such key, so neither walk
        sees it -- and the prose twin's citations ARE swept by `ProseTest`.
        That asymmetry is what this feeds.
        """
        def rec(node, path):
            if isinstance(node, dict):
                for k, v in node.items():
                    yield from rec(v, "%s.%s" % (path, k))
            elif isinstance(node, list):
                for i, v in enumerate(node):
                    yield from rec(v, "%s[%d]" % (path, i))
            elif isinstance(node, str):
                yield path, node
        return list(rec(self.art, "$"))

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

    def check_sweep_start(self, start, handler):
        """A district sweep must begin at its handler's own verb compare.

        `assertLess(sweep_start, first_arm)` alone lets the range be narrowed
        past the menu gates without going red -- Task 25's review moved
        `mar`'s start from `1000:b94a` to `1000:bb80`, skipping two of the six
        gates, and both district tests still passed.  `shop_tag_at` is the anchor the
        artifact already carried and nothing asserted; it is decode-checked
        here so it cannot itself drift.
        """
        self.assertEqual(
            start, handler["shop_tag_at"],
            "the %s district sweep starts at %s, not at the handler's own "
            "verb compare %s -- a sweep whose range can be narrowed does not "
            "establish completeness"
            % (handler["shop"], start, handler["shop_tag_at"]))
        tag = self.at(handler["shop_tag_at"])
        self.assertEqual(
            tag.raw, b"\x9a\xd8\x0b\x78\x0f",
            "%s: shop_tag_at %s decodes %s, not the `call 0f78:0bd8` verb "
            "compare" % (handler["shop"], handler["shop_tag_at"], tag.text))

    def branch_target(self, ins):
        """The image offset a decoded near/short branch jumps to."""
        m = re.search(r"0x([0-9a-f]+)$", ins.text)
        self.assertIsNotNone(m, "%r is not a branch this test can read"
                             % ins.text)
        assert m is not None          # the unittest assert above already
        return int(m.group(1), 16)    # raised; this is for the type checker

    def near_branch_target(self, ins):
        """The image offset a decoded near/short branch jumps to, from BYTES.

        `branch_target` reads the rendered text, which only exists for the
        branches `tools/dis16.py` renders; this walks the `rel8`/`rel16`
        operand instead, so it can be run over a whole function.
        """
        for o in ins.operands:
            if o.kind == "rel8":
                d = o.value - 0x100 if o.value > 0x7f else o.value
                return (ins.end + d) & 0xffff
            if o.kind == "rel16":
                d = o.value - 0x10000 if o.value > 0x7fff else o.value
                return (ins.end + d) & 0xffff
        return None


class ArmsTest(_ArtifactBase):
    #: An instruction claim written INSIDE a prose string -- it carries
    #: neither a separate `addr` key nor a separate `text` key, so
    #: `insn_records` never sees it.  Same regex `tools/test_den_arms.py`
    #: uses, for the same reason.
    PROSE_INSN = re.compile(r"`(1000:[0-9a-f]{4})\s+([a-z][^`]*)`")

    # ------------------------------------------------------------------ tests
    def test_every_address_the_artifact_names_anywhere_is_a_boundary(self):
        """The hole Task 29's review found, closed the way Task 27 closed it.

        `at()` was reachable only from the two structured walks, so a
        `1000:xxxx` written inside a sentence was checked by nothing --
        while the same address in `docs/re/shop-arms.md` went through
        `ProseTest.test_every_prose_address_is_an_instruction_boundary`.
        Task 29 shipped `1000:ce8e`..`1000:ce99` into
        `sell.what_the_port_must_change[0].what` through exactly that gap:
        `1000:ce99` is the last BYTE of `1000:ce97 mov [0x38c9],ax` and
        decodes as `cmp [bx-0x6930],bh`, the authoritative-looking two-byte
        miss `docs/re/METHODOLOGY.md` warns about.

        So every string value in the artifact is scanned, not just the ones
        with an `addr` key, and the whole file is covered -- `rows`, `mar`
        and `sell` alike.
        """
        seen = {}
        for path, s in self.string_values():
            for m in CITE.finditer(s):
                seen.setdefault(m.group(0), []).append(path)
        self.assertGreater(
            len(seen), 400,
            "the artifact-wide citation scan found only %d distinct "
            "addresses; the walk or the regex has broken and this test is "
            "checking almost nothing" % len(seen))
        bad = sorted(c for c in seen
                     if c not in self.aligned and c not in NOT_A_BOUNDARY)
        self.assertEqual(
            bad, [],
            "data/shop_arms.json names %r in free text, and an aligned "
            "decode from every segment-1000 function entry never reaches "
            "it -- so it is a byte offset, not an address. Paths: %r"
            % (bad, {c: seen[c] for c in bad}))

    def test_every_instruction_claim_in_artifact_prose_says_what_the_binary_says(self):
        """The identity half of the same hole.

        A claim written as `` `1000:cdf4 jz 0xcdf9` `` inside a `what`
        sentence is aligned -- so the boundary sweep above passes it -- and
        still carries a mnemonic nothing compares against the binary.  Task
        27 found three false ones in `data/den_arms.json`'s `named_from`
        fields through this exact shape.
        """
        found = []
        for path, s in self.string_values():
            for m in self.PROSE_INSN.finditer(s):
                found.append((path, m.group(1), m.group(2)))
        self.assertGreaterEqual(
            len(found), 8,
            "only %d prose-embedded instruction claims found; the regex has "
            "drifted and this test is checking almost nothing" % len(found))
        for path, cit, text in found:
            ins = self.at(cit)
            self.assertEqual(
                ins.text, text,
                "%s: data/shop_arms.json writes `%s %s` inside a prose "
                "string, but orig/g.exe decodes %r there"
                % (path, cit, text, ins.text))

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

    def test_each_shops_rows_are_the_rows_its_menu_prints(self):
        expected = {"bmar": list("123456"), "mar": list("123456789")}
        for h in self.handlers():
            tag = h["shop"]
            rows = h["rows"]
            self.assertEqual([r["key"] for r in rows], expected[tag])
            self.assertEqual({r["shop"] for r in rows}, {tag})
            menu = {r["key"]: r for r in self.shops if r["shop"] == tag}
            self._check_menu_rows(rows, menu)

    def _check_menu_rows(self, rows, menu):
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
            # the six-instruction push idiom in front of it.  `mar` rows 6,
            # 8 and 9 have a district gate ahead of the setup, so the span
            # starts three instructions earlier than the idiom does and the
            # walk is anchored on `setup_addr`, not on the span.
            setup = r.get("setup_addr", r["span"]["start"])
            if r.get("district_gate") is None:
                self.assertEqual(setup, r["span"]["start"],
                                 "row %s has no district gate, so its span "
                                 "must start at its setup" % r["key"])
            run = dis16.decode_run(
                self.img, addrmod.image_off_of_citation(setup), cmp_ins.off)
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
        for h in self.handlers():
            rows = h["rows"]
            for a, b in zip(rows, rows[1:]):
                self.check_insn(a["miss_branch"], "row %s" % a["key"])
                self.assertEqual(
                    a["miss_branch"]["target"], b["span"]["start"],
                    "%s row %s's miss branch does not hand off to row %s"
                    % (h["shop"], a["key"], b["key"]))
                self.assertEqual(a["span"]["end"], b["span"]["start"])
            # the last row hands off to a setup that belongs to the NEXT
            # thing in the handler, and the literal that setup pushes is what
            # bounds this range on the right without assuming it.
            last = rows[-1]
            bound = h["rows_bounded_on_the_right_by"]
            self.assertEqual(last["miss_branch"]["target"], last["span"]["end"])
            self.assertEqual(bound["setup_addr"], last["span"]["end"])
            setup = addrmod.image_off_of_citation(bound["setup_addr"])
            run = dis16.decode_run(self.img, setup, setup + 0x10)
            lit = [i for i in run if i.raw[:1] == b"\xbf"][1]
            self.assertEqual(
                "0x%04x" % int(lit.text.split(",")[1], 16),
                bound["literal"]["cs_offset"],
                "%s: the setup after the last row does not push the literal "
                "the artifact says bounds it" % h["shop"])
            self.assertEqual(
                self.cs_literal(bound["literal"]["cs_offset"]),
                bound["literal"]["text"])
            cmp_ins = self.check_insn(bound["compare"], h["shop"] + " bound")
            self.assertEqual(bound["compare_addr"], bound["compare"]["addr"])
            self.assertEqual(cmp_ins.raw, b"\x9a\xd8\x0b\x78\x0f")

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
        self.assertEqual(lo, self.span(self.bmar()["rows"][0])[0])
        self.assertEqual(hi, self.span(self.bmar()["rows"][-1])[1])
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
        self.check_sweep_start(sw["start"], self.bmar())
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
            self.bmar()["rows"][-1]["span"]["end"]),
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
        SILENT omitted gate is not, and "no gate refuses silently" is a
        headline claim of `docs/re/shop-arms.md` for both handlers.  So every
        conditional branch in a row's span must be one the artifact records --
        as its miss branch, its district gate, a gate branch, a conjunct
        branch, an effect guard or a `Random` arm.  Unconditional `jmp`s are
        excluded by decoding the recorded address rather than by trusting its
        field name.
        """
        want = {"bmar": 27, "mar": 40}
        for h in self.handlers():
            total = sum(self._check_row_branches(r) for r in h["rows"])
            self.assertEqual(
                total, want[h["shop"]],
                "the %s arms hold %d conditional branches, not the %d this "
                "inventory was built over"
                % (h["shop"], total, want[h["shop"]]))

    def _check_row_branches(self, r):
        lo, hi = self.span(r)
        swept = {"1000:%04x" % i.off
                 for i in dis16.decode_run(self.img, lo, hi)
                 if 0x70 <= i.raw[0] <= 0x7F
                 or (i.raw[0] == 0x0F and 0x80 <= i.raw[1] <= 0x8F)
                 or i.raw[0] in (0xE0, 0xE1, 0xE2, 0xE3)}
        named = {r["miss_branch"]["addr"]}
        if r.get("district_gate"):
            named.add(r["district_gate"]["branch"]["addr"])
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
            "%s row %s: the conditional branches inside %s..%s are %r, the "
            "artifact accounts for %r -- an unrecorded branch is an "
            "unrecorded gate, and a silent one prints nothing for the "
            "strings sweep to catch"
            % (r["shop"], r["key"], r["span"]["start"], r["span"]["end"],
               sorted(swept), sorted(conditional)))
        return len(swept)

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
                "%s row %s: the CS literals actually pushed inside %s..%s are "
                "%r, the artifact records %r -- `strings[]` is not complete"
                % (r["shop"], r["key"], r["span"]["start"], r["span"]["end"],
                   sorted(pushed), sorted(recorded)))

    def test_the_only_silent_gate_in_any_arm_is_a_mar_district_gate(self):
        """No `gates[]` entry anywhere refuses silently.

        Each gate names a refusal literal, every gate's literal is distinct
        from its siblings', and each is one of the arm's recorded strings.  Two
        gates sharing one literal would mean a refusal path was attributed to
        the wrong test.

        `silent_gates` is therefore never a `gates[]` entry: among the fifteen
        rows this artifact maps, the only silent test is a `mar` district
        gate, which sits in front of the row's SETUP rather than inside the
        arm, and `silent_gates` is asserted equal to exactly that -- so a
        genuinely silent refusal added to `gates[]` cannot be hidden by
        listing it here.  (`bmar` row 9's two silent gates are Task 18's and
        are not among the fifteen.)
        """
        for r in self.rows():
            dg = r.get("district_gate")
            self.assertEqual(
                r["silent_gates"], [dg["addr"]] if dg else [],
                "row %s %s: silent_gates must be exactly the row's district "
                "gate, or empty when it has none" % (r["shop"], r["key"]))
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

    def test_the_recorded_effects_are_every_write_in_the_arm(self):
        """`effects[]` completeness, measured -- the `gates[]` sweep's twin.

        `test_every_effect_writes_the_dgroup_address_it_names` above checks
        every RECORDED effect and can never notice a write the artifact left
        out.  That is the Task-23 inventory shape, and two `mar` claims rest on
        it: row 2's `Random(3)` "changes no state at all", and "no global a
        `mar` arm writes is write-only" -- both are statements about the WHOLE
        set of writes in a span.

        `tools/dis16.py` carries no read/write flag, so the classification is
        by decoded text, and it is made honest two ways rather than trusted:

        * every instruction in the span that touches an absolute-memory
          operand must fall in the WRITE bucket or the READ bucket -- an
          unclassified one fails loudly, so a write shape the regex does not
          describe cannot be silently dropped.  `cmp byte [0x..],imm` is why
          "memory operand first" alone is not the rule: it puts memory first
          and writes nothing.
        * every RECORDED effect must itself land in the WRITE bucket, so a
          bucket that stopped matching real writes reds here too.
        """
        total = 0
        for r in self.rows():
            lo, hi = self.span(r)
            written, read, unclassified = set(), set(), []
            for i in dis16.decode_run(self.img, lo, hi):
                if "[0x" not in i.text:
                    continue
                cit = "1000:%04x" % i.off
                if WRITES_ABS_MEM.match(i.text):
                    written.add(cit)
                elif READS_ABS_MEM.match(i.text):
                    read.add(cit)
                else:
                    unclassified.append("%s %s" % (cit, i.text))
            self.assertEqual(
                unclassified, [],
                "%s row %s: %r touches absolute memory and is neither a "
                "recognised write nor a recognised read, so the sweep below "
                "cannot claim to have seen every write"
                % (r["shop"], r["key"], unclassified))
            self.assertTrue(
                read,
                "%s row %s: every instruction touching absolute memory in "
                "this span landed in the WRITE bucket and none in the READ "
                "one -- at minimum the affordability test's `cmp ax,[0x38c7]` "
                "reads, so the two buckets are not partitioning anything"
                % (r["shop"], r["key"]))
            recorded = {e["addr"] for e in r["effects"]}
            self.assertEqual(
                written, recorded,
                "%s row %s: the absolute memory writes inside %s..%s are %r, "
                "the artifact records %r -- `effects[]` is not complete"
                % (r["shop"], r["key"], r["span"]["start"], r["span"]["end"],
                   sorted(written), sorted(recorded)))
            for e in r["effects"]:
                self.assertRegex(
                    self.at(e["addr"]).text, WRITES_ABS_MEM,
                    "%s row %s: the recorded effect %s does not match the "
                    "WRITE bucket the sweep uses, so the bucket and the "
                    "artifact disagree about what a write is"
                    % (r["shop"], r["key"], e["addr"]))
            total += len(written)
        self.assertEqual(
            total, 56,
            "the fifteen arms hold %d absolute memory writes, not the 56 this "
            "inventory was built over (25 in the six `bmar` arms, 31 in the "
            "nine `mar` ones)" % total)

    def test_the_recorded_rolls_are_every_random_call_in_the_arm(self):
        """`roll[]` completeness, measured over the `Random` call signature.

        `docs/re/shop-arms.md`'s directive for Task 26 says a port that skips
        one of these desynchronises every RNG trace after it, which is a claim
        about the WHOLE set of draws in each arm.  Checking the recorded ones
        decode cannot establish that; sweeping the five-byte far-call
        signature can.
        """
        found = 0
        for r in self.rows():
            lo, hi = self.span(r)
            swept = {"1000:%04x" % i.off
                     for i in dis16.decode_run(self.img, lo, hi)
                     if i.raw == RANDOM_CALL}
            roll = r.get("roll")
            recorded = {roll["call"]["addr"]} if roll else set()
            self.assertEqual(
                swept, recorded,
                "%s row %s: the `Random` call sites inside %s..%s are %r, the "
                "artifact records %r -- an unrecorded draw still advances the "
                "RNG stream"
                % (r["shop"], r["key"], r["span"]["start"], r["span"]["end"],
                   sorted(swept), sorted(recorded)))
            found += len(swept)
        self.assertEqual(
            found, 3,
            "the fifteen arms hold %d `Random` draws, not the 3 this "
            "inventory was built over (bmar row 3, mar rows 1 and 2)" % found)

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
            [x for x in self.bmar()["rows"] if x["key"] == "6"][0])
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
        row6 = [r for r in self.bmar()["rows"] if r["key"] == "6"][0]
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
        for r in self.bmar()["rows"]:
            for g in r["gates"]:
                if g["kind"] != "prereq" or "conjuncts" not in g:
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
        r = [x for x in self.bmar()["rows"] if x["key"] == "3"][0]
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

    # ------------------------------------------------------ the `mar` shop
    def test_the_mar_district_gates_are_every_district_operand_in_the_handler(self):
        """`mar`'s inventory is a POSITIVE claim, swept the same way.

        `bmar`'s finding is that no arm reads `20ae:3692`; `mar`'s is that
        three do, and that the menu gates and the buy-path gates do NOT cover
        the same rows.  A difference between two sets is worth nothing if
        either set is a list that stopped being written, so both are asserted
        SET-EQUAL to one sweep of the whole handler -- and the raw `92 36`
        byte scan is counted over the same range so the sweep cannot have
        missed a site the decoder skipped.
        """
        f = self.mar()["district_finding"]
        district = int(f["district_ds"].split(":")[1], 16)
        rows = self.mar()["rows"]
        lo = addrmod.image_off_of_citation(f["span"]["start"])
        hi = addrmod.image_off_of_citation(f["span"]["end"])
        self.assertEqual(lo, self.span(rows[0])[0])
        self.assertEqual(hi, self.span(rows[-1])[1])
        run = dis16.decode_run(self.img, lo, hi)
        self.assertEqual(
            len(run), f["instruction_count"],
            "the aligned run over %s..%s is %d instructions, not the %d the "
            "artifact records -- without this a set equality could be passing "
            "because the walk stopped early"
            % (f["span"]["start"], f["span"]["end"], len(run),
               f["instruction_count"]))

        def hits(seq):
            return {"1000:%04x" % i.off for i in seq
                    for op in i.operands
                    if op.kind in ("disp16", "disp16x", "moffs16")
                    and op.value == district}

        buy = {g["addr"] for g in f["buy_path_gates"]}
        self.assertEqual(
            hits(run), buy,
            "the district operands inside the nine arms are %r, the artifact "
            "records %r as the buy-path gates"
            % (sorted(hits(run)), sorted(buy)))
        sw = f["sweep_range"]
        self.check_sweep_start(sw["start"], self.mar())
        slo = addrmod.image_off_of_citation(sw["start"])
        shi = addrmod.image_off_of_citation(sw["end"])
        self.assertLess(slo, lo, "the sweep must start before the arms")
        self.assertEqual(shi, hi, "the sweep must end where the arms do, "
                                  "which is what keeps the pickpocket block's "
                                  "three district reads out of it")
        menu = {g["addr"] for g in f["menu_gates"]}
        self.assertEqual(
            hits(dis16.decode_run(self.img, slo, shi)), buy | menu,
            "the district-gate inventory over %s..%s is not complete"
            % (sw["start"], sw["end"]))
        self.assertFalse(buy & menu, "a gate is recorded as both")
        want = district.to_bytes(2, "little")
        raw = {"0x%x" % (o - 2) for o in range(slo, shi - 1)
               if self.img[o:o + 2] == want}
        self.assertEqual(
            len(raw), len(buy | menu),
            "the raw `%s` byte scan over %s..%s finds %d candidate sites and "
            "the artifact records %d gates"
            % (want.hex(" "), sw["start"], sw["end"], len(raw),
               len(buy | menu)))
        for g in f["buy_path_gates"] + f["menu_gates"]:
            ins = self.check_insn(g, "mar district_finding")
            self.assertIn("0x%x" % district, ins.text)
            self.check_insn(g["branch"], "mar district_finding")

    def test_the_mar_buy_path_gates_stand_in_front_of_the_row_and_print_nothing(self):
        """Below its district a gated `mar` row is unreachable, not refused.

        The gate sits ahead of the row's SETUP, so the key compare never runs
        and no literal is pushed -- which is why the port cannot reproduce it
        with a refusal line.  Both halves are measured: the skip target is the
        row's own span end, and the instruction run from the gate to the setup
        is exactly the three instructions the artifact records, so there is no
        push hiding in it.
        """
        f = self.mar()["district_finding"]
        by_row = {g["row"]: g for g in f["buy_path_gates"]}
        self.assertEqual(sorted(by_row), sorted(f["rows_gated_on_the_buy_path"]))
        gated = []
        for r in self.mar()["rows"]:
            dg = r.get("district_gate")
            if dg is None:
                self.assertEqual(r["span"]["start"], r["setup_addr"])
                self.assertIsNone(r["district_test"])
                continue
            gated.append(r["key"])
            self.assertEqual(r["district_test"], f["district_ds"])
            self.assertEqual(dg["addr"], by_row[r["key"]]["addr"])
            self.assertEqual(
                r["span"]["start"], dg["addr"],
                "row %s's span must start at its district gate, or the gate "
                "falls in the gap between two arms and no sweep sees it"
                % r["key"])
            gate = self.check_insn(dg, "mar row %s gate" % r["key"])
            branch = self.check_insn(dg["branch"], "mar row %s" % r["key"])
            self.assertEqual(
                branch.raw[0], 0x77,
                "row %s: the district branch at %s is %s, not the `ja` the "
                "artifact reads the sense from"
                % (r["key"], dg["branch"]["addr"], branch.text))
            self.assertEqual("1000:%04x" % self.branch_target(branch),
                             r["setup_addr"])
            skip = self.check_insn(dg["skip"], "mar row %s" % r["key"])
            self.assertEqual(skip.raw[0], 0xE9, "not a near jmp: %s" % skip.text)
            self.assertEqual(
                "1000:%04x" % self.branch_target(skip), r["span"]["end"],
                "row %s: the district gate does not skip the whole arm"
                % r["key"])
            self.assertEqual(r["miss_branch"]["target"], r["span"]["end"])
            head = dis16.decode_run(
                self.img, addrmod.image_off_of_citation(dg["addr"]),
                addrmod.image_off_of_citation(r["setup_addr"]))
            self.assertEqual(
                [i.text for i in head],
                [gate.text, branch.text, skip.text],
                "row %s: something other than the gate stands between %s and "
                "the setup at %s" % (r["key"], dg["addr"], r["setup_addr"]))
            self.assertEqual(
                int(gate.text.rsplit(",", 1)[1], 16),
                int(by_row[r["key"]]["sense"].rsplit(" ", 1)[1]),
                "row %s: the recorded sense does not name the immediate the "
                "gate compares against" % r["key"])
        self.assertEqual(gated, f["rows_gated_on_the_buy_path"])

    def test_mar_row_7_is_menu_gated_but_not_buy_gated(self):
        """The divergence, as the difference between two MEASURED sets.

        Which menu lines a menu gate covers is measured -- the price-byte
        operands an aligned decode of the gate's own listed range finds -- and
        cross-checked against `data/shops.json`, which already owns the menu
        half.  The buy-path set comes from the sweep in the test above.  Only
        then is the difference taken.
        """
        f = self.mar()["district_finding"]
        rng = f["price_byte_range"]
        lo_p = int(rng["first"].split(":")[1], 16)
        hi_p = int(rng["last"].split(":")[1], 16)
        rows = self.mar()["rows"]
        self.assertEqual(
            [r["price_addr"] for r in rows],
            ["20ae:%04x" % o for o in range(lo_p, hi_p + 1)],
            "the nine mar price bytes are not the contiguous run "
            "price_byte_range names")
        by_price = {r["price_addr"]: r["key"] for r in rows}
        menu_gated = {}
        for g in f["menu_gates"]:
            ins = self.check_insn(g, "mar menu gate")
            imm = int(ins.text.rsplit(",", 1)[1], 16)
            self.assertEqual(g["gate"], "district>%d" % imm)
            branch = self.check_insn(g["branch"], "mar menu gate")
            if branch.raw[0] == 0x77:            # `ja <listed>` + `jmp <skip>`
                self.assertEqual("1000:%04x" % self.branch_target(branch),
                                 g["listed_from"])
                nxt = self.at("1000:%04x" % branch.end)
                self.assertEqual(nxt.raw[0], 0xE9, "not a near jmp: %s"
                                 % nxt.text)
                self.assertEqual("1000:%04x" % self.branch_target(nxt),
                                 g["skip_target"])
            else:                                # `jbe <skip>`, fall to listed
                self.assertEqual(branch.raw[0], 0x76, "unexpected branch %s"
                                 % branch.text)
                self.assertEqual("1000:%04x" % self.branch_target(branch),
                                 g["skip_target"])
                self.assertEqual("1000:%04x" % branch.end, g["listed_from"])
            glo = addrmod.image_off_of_citation(g["listed_from"])
            ghi = addrmod.image_off_of_citation(g["skip_target"])
            self.assertLess(glo, ghi)
            covered = {"20ae:%04x" % op.value
                       for i in dis16.decode_run(self.img, glo, ghi)
                       for op in i.operands
                       if op.kind in ("disp16", "disp16x", "moffs16")
                       and lo_p <= op.value <= hi_p}
            self.assertEqual(
                covered, set(g["covers_price_bytes"]),
                "the menu gate at %s lists the rows whose price bytes are %r, "
                "the artifact records %r"
                % (g["addr"], sorted(covered), sorted(g["covers_price_bytes"])))
            self.assertEqual({by_price[c] for c in covered},
                             set(g["covers_rows"]))
            for k in g["covers_rows"]:
                self.assertNotIn(k, menu_gated, "row %s is behind two menu "
                                                "gates" % k)
                menu_gated[k] = g["gate"]
        self.assertEqual(sorted(menu_gated), sorted(f["rows_gated_in_the_menu"]))
        # ... and the menu half must agree with the artifact that owns it
        for r in rows:
            m = [x for x in self.shops
                 if x["shop"] == "mar" and x["key"] == r["key"]][0]
            self.assertEqual(
                m["gate"], menu_gated.get(r["key"]),
                "mar row %s: data/shops.json says gate %r, the menu-gate "
                "measurement here says %r"
                % (r["key"], m["gate"], menu_gated.get(r["key"])))
        buy_gated = {g["row"] for g in f["buy_path_gates"]}
        self.assertEqual(sorted(buy_gated),
                         sorted(f["rows_gated_on_the_buy_path"]))
        self.assertEqual(
            sorted(set(menu_gated) - buy_gated), ["7"],
            "the rows the menu hides but the buy path sells are %r, and the "
            "whole row-7 finding rests on that being exactly ['7']"
            % sorted(set(menu_gated) - buy_gated))
        self.assertEqual(sorted(buy_gated - set(menu_gated)), [],
                         "a row is buy-gated but not menu-gated, which no "
                         "part of the artifact claims")
        bug = [b for b in self.mar()["bugs"]
               if b["label"] == "mar-row-7-is-menu-gated-but-not-buy-gated"][0]
        row7 = [r for r in rows if r["key"] == "7"][0]
        self.assertIsNone(row7["district_gate"])
        self.assertEqual(bug["where"]["row_7_span_start"], row7["span"]["start"])
        self.assertEqual(bug["where"]["row_7_key_compare"], row7["compare_addr"])
        self.assertEqual(
            bug["where"]["buy_gate_for_row_6"],
            [r for r in rows if r["key"] == "6"][0]["district_gate"]["addr"])
        self.assertEqual(bug["where"]["menu_gate"], f["menu_gates"][0]["addr"])

    def test_the_mar_upgrade_rows_grant_the_delta_not_the_bonus(self):
        """Rows 7, 8 and 9 add less when the lesser item is already owned."""
        bug = [b for b in self.mar()["bugs"]
               if b["label"] == "mar-upgrade-rows-grant-the-delta-not-the-bonus"
               ][0]
        guards = bug["where"]["guards"]
        branches = bug["where"]["guard_branches"]
        rejoins = bug["where"]["delta_rejoins"]
        self.assertEqual(len(guards), 3)
        self.assertEqual(len(branches), 3)
        self.assertEqual(len(rejoins), 3)
        for key, guard, br, rj in zip(("7", "8", "9"), guards, branches,
                                      rejoins):
            r = [x for x in self.mar()["rows"] if x["key"] == key][0]
            gi = self.at(guard)
            bi = self.at(br)
            self.assertEqual(bi.raw[0], 0x74, "%s: not a `jz`: %s" % (br,
                                                                     bi.text))
            delta = [e for e in r["effects"]
                     if e.get("guard") and "!= 0" in e["condition"]]
            full = [e for e in r["effects"]
                    if e.get("guard") and "== 0" in e["condition"]]
            self.assertTrue(delta and full,
                            "row %s records no upgrade split" % key)
            for e in delta + full:
                self.assertEqual(e["guard"]["addr"], guard)
                self.assertEqual(e["guard_branch"]["addr"], br)
            self.assertEqual(
                "1000:%04x" % bi.end, delta[0]["addr"],
                "row %s: the guard's not-taken path is not the delta add"
                % key)
            self.assertEqual(
                "1000:%04x" % self.branch_target(bi), full[0]["addr"],
                "row %s: the guard does not jump to the full-bonus add" % key)
            self.assertNotEqual(
                [e["text"] for e in delta], [e["text"] for e in full],
                "row %s: the two arms of the split add the same amount, so "
                "there is no upgrade delta to reproduce" % key)
            # the delta arm rejoins PAST the full-bonus arm, so the two are
            # exclusive and neither falls into the other.  These three
            # instructions are the ones `docs/re/shop-arms.md` shows inside a
            # fence, where the prose checker cannot reach them.
            ri = self.check_insn(rj, "mar row %s delta rejoin" % key)
            self.assertEqual(ri.raw[0], 0xEB, "%s is not a short jmp: %s"
                             % (rj["addr"], ri.text))
            self.assertTrue(
                addrmod.image_off_of_citation(delta[-1]["addr"]) < ri.off
                < addrmod.image_off_of_citation(full[0]["addr"]),
                "row %s: %s does not sit between the delta arm and the "
                "full-bonus arm" % (key, rj["addr"]))
            self.assertGreater(
                self.branch_target(ri),
                addrmod.image_off_of_citation(full[-1]["addr"]),
                "row %s: the delta arm falls into the full-bonus add instead "
                "of jumping over it, so the two would both apply" % key)
            # the flag the guard reads is the LESSER row's own ownership flag
            lesser = {"7": "4", "8": "5", "9": "6"}[key]
            other = [x for x in self.mar()["rows"] if x["key"] == lesser][0]
            self.assertIn(
                "0x%x" % int(other["own_test"].split(":")[1], 16), gi.text,
                "row %s's upgrade guard %s does not read row %s's ownership "
                "flag %s" % (key, guard, lesser, other["own_test"]))

    def test_the_mar_rolls_are_a_random_2_and_a_random_3(self):
        """Row 1 heals `3 + Random(2)`; row 2's `Random(3)` is cosmetic."""
        rows = {r["key"]: r for r in self.mar()["rows"]}
        self.assertEqual(sorted(k for k, r in rows.items() if r.get("roll")),
                         ["1", "2"],
                         "some other mar row grew a Random draw")
        for k in ("1", "2"):
            roll = rows[k]["roll"]
            call = self.check_insn(roll["call"], "mar row %s roll" % k)
            self.assertEqual(call.raw, b"\x9a\x4b\x11\x78\x0f",
                             "%s is not the `Random` far call"
                             % roll["call"]["addr"])
            self.check_insn(roll["n_push"], "mar row %s roll" % k)
            n = re_query.pushed_n(self.prog, roll["call"]["addr"])
            self.assertEqual(n["n"], roll["n"],
                             "the idiom before %s pushes %r, not %d"
                             % (roll["call"]["addr"], n["n"], roll["n"]))
        # row 1: the draw is arithmetic, not a dispatch
        r1 = rows["1"]
        self.assertEqual(r1["roll"]["arms"], [])
        add3 = self.check_insn(r1["roll"]["consumed_arithmetically"], "mar 1")
        heal = [e for e in r1["effects"] if e["ds"] == "20ae:38ac"][0]
        between = dis16.decode_run(
            self.img, self.at(r1["roll"]["call"]["addr"]).end,
            addrmod.image_off_of_citation(heal["addr"]))
        self.assertEqual(
            [i.text for i in between], [add3.text],
            "something other than `%s` stands between row 1's Random(2) and "
            "the hp add at %s, so `3 + Random(2)` is not what it heals"
            % (add3.text, heal["addr"]))
        # row 2: three arms, all converging on the beer counter
        r2 = rows["2"]
        arms = r2["roll"]["arms"]
        self.assertEqual([a["value"] for a in arms], [0, 1, 2])
        recorded = {s["cs_offset"]: s for s in r2["strings"]}
        for a in arms:
            ins = self.check_insn(a["compare"], "mar row 2 arm %d" % a["value"])
            self.assertEqual(ins.text, "cmp ax,0x%x" % a["value"])
            br = self.check_insn(a["branch"], "mar row 2 arm %d" % a["value"])
            self.assertEqual(br.raw[0], 0x75, "not a `jnz`: %s" % br.text)
            self.assertIn(a["prints_cs_offset"], recorded)
            push = addrmod.image_off_of_citation(
                recorded[a["prints_cs_offset"]]["push"]["addr"])
            self.assertTrue(
                br.end <= push < self.branch_target(br),
                "mar row 2 arm %d: its literal is pushed at 0x%x, which is "
                "not on the branch's not-taken path"
                % (a["value"], push))
        beer = [e for e in r2["effects"] if e["ds"] == "20ae:38c3"][0]
        self.assertEqual(
            "1000:%04x" % self.branch_target(self.at(arms[-1]["branch"]["addr"])),
            beer["addr"],
            "the last Random arm does not fall on the beer counter, so the "
            "three arms do not converge the way the artifact says")
        # ... and the refusal rejoins PAST it, so a failed buy adds no beer
        gate = [g for g in r2["gates"] if g["kind"] == "afford"][0]
        rejoin = self.check_insn(gate["refusal_rejoin"], "mar row 2 refusal")
        self.assertEqual("1000:%04x" % self.branch_target(rejoin),
                         gate["refusal_rejoin_target"])
        self.assertEqual(gate["refusal_rejoin_target"], r2["span"]["end"])
        self.assertLess(addrmod.image_off_of_citation(beer["addr"]),
                        addrmod.image_off_of_citation(
                            gate["refusal_rejoin_target"]),
                        "the beer counter is not inside the arm the refusal "
                        "jumps over")


class SellTest(_ArtifactBase):
    """`data/shop_arms.json`'s `sell` block, Task 29's half of the handler.

    The purchase tests above take their rows from `handlers()`; the sell path
    is not a row -- it has no key, no price byte and no affordability test --
    so it gets its own class rather than being forced into `rows()`.  What it
    keeps is the SHAPE of the checks: every completeness claim is asserted by
    SET EQUALITY against a sweep of `orig/g.exe` over the whole recorded
    range, never by checking that the listed entries hold up.

    Six sweeps, all over `1000:ce76`..`1000:d383`:

      * **the spans tile the range.**  Every other sweep here is per-range and
        would still pass if an arm's span silently overlapped its neighbour,
        so the eleven spans are asserted to tile with no gap and no overlap
        and to sum to `sweeps.instructions`.
      * **`gates[]` is complete.**  Every conditional branch in the range must
        be named -- as a gate branch, a conjunct branch, a decline branch, the
        junk arm's miss or gate, the `wes` hit branch, the sentinel branch or
        the exit branch.  All twelve gates in this range are SILENT (they
        print nothing), so the strings sweep cannot catch an omitted one.
      * **`strings[]` is complete.**  Set equality against every
        `mov di,imm` / `push cs` / `push di` in the range.
      * **`effects[]` is complete.**  The same WRITE/READ bucket partition the
        purchase sweep uses, over the whole range.
      * **`roll[]` is complete**, and each `n` and `base` is RE-DERIVED --
        the `n` by `re_query.pushed_n` walking back from the call, the base by
        decoding the `add ax,imm` -- rather than read out of the artifact.
        This is what the refund finding rests on.
      * **the DGROUP set is complete.**  `sweeps.dgroup_addresses` is set-equal
        to every direct-memory operand value in the range, and that is what
        makes "no price byte is read" and "no stat global is written"
        measurements rather than lists that stopped being written.  Both
        negatives are cross-checked by a raw byte-pair count so they do not
        rest on the decoder alone.

    Plus the two claims that are not sweeps: the range's right-hand end is
    established by scanning every near branch in `entry` for one that targets
    `1000:d383`, and the ten item flags' NAMES are re-derived from the
    character sheet's own guard and label push rather than borrowed from
    `docs/re/character-sheet.md`.
    """

    #: `bmar`'s price table (`20ae:0b38`..`20ae:0b40`), `mar`'s
    #: (`20ae:0b2e`..`20ae:0b36`), and the three stat globals a sale might
    #: plausibly have unwound.  None may appear in the sell range.
    PRICE_BYTES = tuple(range(0x0b2e, 0x0b41))
    STAT_GLOBALS = (0x38b2, 0x38a8, 0x38aa)

    def sell(self):
        return self.art["sell"]

    def sell_range(self):
        s = self.sell()["range"]
        return (addrmod.image_off_of_citation(s["start"]),
                addrmod.image_off_of_citation(s["end"]))

    def sell_run(self):
        lo, hi = self.sell_range()
        return dis16.decode_run(self.img, lo, hi)

    def ordered_spans(self):
        """The eleven spans, in address order, with the node that owns each."""
        s = self.sell()
        out = [("junk_arm", s["junk_arm"]["span"]),
               ("wes_dispatch", s["wes_dispatch"]["span"]),
               ("prologue", {"start": s["prologue"]["addr"],
                             "end": s["arms"][0]["span"]["start"]})]
        out += [("arm %d" % a["n"], a["span"]) for a in s["arms"]]
        out.append(("tail", s["tail"]["span"]))
        return out

    # ------------------------------------------------------------- the range
    def test_the_sell_spans_tile_the_range_with_no_gap_and_no_overlap(self):
        s = self.sell()
        lo, hi = self.sell_range()
        spans = self.ordered_spans()
        cursor = lo
        total = 0
        for name, sp in spans:
            start = addrmod.image_off_of_citation(sp["start"])
            end = addrmod.image_off_of_citation(sp["end"])
            self.assertEqual(
                start, cursor,
                "sell span %s starts at %s, but the previous span ended at "
                "1000:%04x -- the spans do not tile, so every per-range sweep "
                "below could be measuring a different set of bytes than the "
                "per-span records claim" % (name, sp["start"], cursor))
            self.at(sp["start"])
            self.assertGreater(end, start, "sell span %s is empty" % name)
            total += len(dis16.decode_run(self.img, start, end))
            cursor = end
        self.assertEqual(cursor, hi,
                         "the sell spans end at 1000:%04x, the recorded range "
                         "at %s" % (cursor, s["range"]["end"]))
        self.assertEqual(
            s["sweeps"]["range"],
            {"start": s["range"]["start"], "end": s["range"]["end"]},
            "`sweeps.range` is not the range the rest of the block "
            "describes, so every count under it could have been measured "
            "somewhere else")
        run = self.sell_run()
        ds_push = sum(1 for n, i in enumerate(run[:-2])
                      if i.raw[:1] == b"\xbf"
                      and run[n + 1].text == "push ds"
                      and run[n + 2].text == "push di")
        self.assertEqual(
            ds_push, s["sweeps"]["ds_pointer_pushes"],
            "the range pushes %d DGROUP pointers, `sweeps` says %d -- the "
            "count is what separates the CS-literal pushes the strings sweep "
            "measures from the buffer pushes it must ignore"
            % (ds_push, s["sweeps"]["ds_pointer_pushes"]))
        self.assertEqual(
            total, s["sweeps"]["instructions"],
            "the eleven spans hold %d instructions, `sweeps.instructions` "
            "says %d" % (total, s["sweeps"]["instructions"]))
        self.assertEqual(
            len(self.sell_run()), s["sweeps"]["instructions"],
            "one aligned decode of the whole range yields %d instructions, "
            "`sweeps.instructions` says %d"
            % (len(self.sell_run()), s["sweeps"]["instructions"]))

    def test_the_sell_range_ends_where_the_artifact_says_it_does(self):
        """A bound is not an inventory -- so the end is MEASURED.

        Task 28 shipped a residue documented as ending 282 bytes past the
        arm's real end, because the interval was taken from a brief instead of
        from the branch graph.  Here the whole `entry` function is decoded as
        one aligned run and every near branch in it is checked for a target of
        `1000:d383`: exactly one exists, and it is outside the handler, so
        `1000:d381` is the sell path's last instruction.
        """
        s = self.sell()
        rng = s["range"]
        last = self.check_insn(rng["last_instruction"], "sell range end")
        end = addrmod.image_off_of_citation(rng["end"])
        self.assertEqual(last.end, end,
                         "%s does not end where the range does"
                         % rng["last_instruction"]["addr"])
        ent = [f for f in self.branches["functions"] if f["name"] == "entry"][0]
        lo = addrmod.image_off_of_citation(ent["entry"])
        run = dis16.decode_run(self.img, lo, lo + ent["size"])
        self.assertGreater(len(run), 7000,
                           "the `entry` decode yielded only %d instructions; "
                           "a scan that covers almost nothing must not pass"
                           % len(run))
        start = addrmod.image_off_of_citation(rng["start"])
        into_end, into_start = [], []
        for i in run:
            tgt = self.near_branch_target(i)
            if tgt == end:
                into_end.append("1000:%04x" % i.off)
            elif tgt == start:
                into_start.append("1000:%04x" % i.off)
        self.assertEqual(
            into_end, ["1000:c4cf"],
            "the near branches in `entry` that target %s are %r -- the "
            "artifact's claim that the sell path stops before it rests on "
            "there being exactly one, outside the handler"
            % (rng["end"], into_end))
        for e in rng["entered_from"]:
            self.check_insn(e, "sell range entry edge")
        recorded = [e["addr"] for e in rng["entered_from"]]
        self.assertEqual(
            sorted(into_start), sorted(a for a in recorded
                                       if a != "1000:ce71"),
            "the near branches into %s are %r; `range.entered_from` records "
            "%r besides the fall-through"
            % (rng["start"], sorted(into_start),
               sorted(a for a in recorded if a != "1000:ce71")))
        self.assertIn("1000:ce71", recorded)
        self.assertEqual(
            self.at("1000:ce71").end, start,
            "1000:ce71 is recorded as falling through into the sell path but "
            "does not end where it starts")

    # ------------------------------------------------------------ the sweeps
    def named_conditional_branches(self):
        s = self.sell()
        named = set()
        for a in s["arms"]:
            for g in a["gates"]:
                if "branch" in g:
                    named.add(g["branch"]["addr"])
                for c in g.get("conjuncts", []):
                    named.add(c["branch"]["addr"])
            named.add(a["answer"]["decline_branch"]["addr"])
        j = s["junk_arm"]
        named.add(j["miss_branch"]["addr"])
        for g in j["gates"]:
            named.add(g["branch"]["addr"])
        named.add(s["wes_dispatch"]["hit_branch"]["addr"])
        named.add(s["tail"]["sentinel_branch"]["addr"])
        named.add(s["tail"]["exit_compare"]["leave_branch"]["addr"])
        return named

    def test_the_recorded_gates_are_every_conditional_branch_in_the_range(self):
        s = self.sell()
        swept = {"1000:%04x" % i.off for i in self.sell_run()
                 if 0x70 <= i.raw[0] <= 0x7F
                 or (i.raw[0] == 0x0F and 0x80 <= i.raw[1] <= 0x8F)
                 or i.raw[0] in (0xE0, 0xE1, 0xE2, 0xE3)}
        named = self.named_conditional_branches()
        for a in sorted(named):
            self.assertNotIn(self.at(a).raw[0], (0xEB, 0xE9),
                             "%s is recorded as a conditional branch but "
                             "decodes as an unconditional jump" % a)
        self.assertEqual(
            swept, named,
            "the conditional branches inside %s..%s are %r, the artifact "
            "accounts for %r -- an unrecorded branch is an unrecorded gate, "
            "and every gate in this range is SILENT, so the strings sweep "
            "cannot catch it" % (s["range"]["start"], s["range"]["end"],
                                 sorted(swept), sorted(named)))
        self.assertEqual(
            len(swept), s["sweeps"]["conditional_branches"],
            "the range holds %d conditional branches, `sweeps` says %d"
            % (len(swept), s["sweeps"]["conditional_branches"]))

    def recorded_literal_pushes(self):
        """Every CS-literal push address the `sell` block names."""
        def walk(node):
            if isinstance(node, dict):
                if isinstance(node.get("cs_offset"), str) \
                        and isinstance(node.get("push"), dict):
                    yield node["push"]["addr"], node["cs_offset"]
                for v in node.values():
                    yield from walk(v)
            elif isinstance(node, list):
                for v in node:
                    yield from walk(v)
        lo, hi = self.sell_range()
        return {a: c for a, c in walk(self.sell())
                if lo <= addrmod.image_off_of_citation(a) < hi}

    def test_the_recorded_strings_are_every_cs_literal_the_range_pushes(self):
        s = self.sell()
        run = self.sell_run()
        pushed = {}
        for n, ins in enumerate(run[:-2]):
            if ins.raw[:1] != b"\xbf":
                continue
            if run[n + 1].text != "push cs" or run[n + 2].text != "push di":
                continue                # `push ds` -> the DGROUP buffers
            pushed["1000:%04x" % ins.off] = "0x%04x" % int(
                ins.text.split(",")[1], 16)
        recorded = self.recorded_literal_pushes()
        self.assertEqual(
            pushed, recorded,
            "the CS literals actually pushed inside %s..%s are %r, the "
            "artifact records %r -- `strings[]` is not complete"
            % (s["range"]["start"], s["range"]["end"],
               sorted(pushed.items()), sorted(recorded.items())))
        self.assertEqual(len(pushed), s["sweeps"]["cs_literal_pushes"],
                         "the range pushes %d CS literals, `sweeps` says %d"
                         % (len(pushed), s["sweeps"]["cs_literal_pushes"]))

    def recorded_effects(self):
        s = self.sell()
        out = {}
        for a in s["arms"]:
            for e in a["effects"]:
                out[e["addr"]] = e
        for e in s["junk_arm"]["effects"]:
            out[e["addr"]] = e
        out[s["prologue"]["addr"]] = s["prologue"]
        return out

    def test_the_recorded_effects_are_every_absolute_write_in_the_range(self):
        s = self.sell()
        written, read, unclassified = set(), set(), []
        for i in self.sell_run():
            if "[0x" not in i.text:
                continue
            cit = "1000:%04x" % i.off
            if WRITES_ABS_MEM.match(i.text):
                written.add(cit)
            elif READS_ABS_MEM.match(i.text):
                read.add(cit)
            else:
                unclassified.append("%s %s" % (cit, i.text))
        self.assertEqual(unclassified, [],
                         "%r touches absolute memory and is neither a "
                         "recognised write nor a recognised read" % unclassified)
        self.assertTrue(read, "no READ landed in the read bucket, so the two "
                              "buckets are not partitioning anything")
        recorded = self.recorded_effects()
        self.assertEqual(
            written, set(recorded),
            "the absolute memory writes inside %s..%s are %r, the artifact "
            "records %r -- `effects[]` is not complete"
            % (s["range"]["start"], s["range"]["end"],
               sorted(written), sorted(recorded)))
        for a, e in recorded.items():
            self.assertRegex(self.at(a).text, WRITES_ABS_MEM)
            self.assertIn("0x%x" % int(e["ds"].split(":")[1], 16),
                          self.at(a).text,
                          "%s does not write %s" % (a, e["ds"]))
        self.assertEqual(len(written), s["sweeps"]["absolute_memory_writes"],
                         "the range holds %d absolute writes, `sweeps` says %d"
                         % (len(written), s["sweeps"]["absolute_memory_writes"]))

    def test_the_recorded_rolls_are_every_random_call_and_each_n_is_rederived(self):
        """The refund finding's foundation, measured twice.

        A draw nobody recorded still advances the RNG stream; and an `n` or a
        base read out of the artifact would make the refund table a copy of
        itself.  So the sites come from the five-byte signature sweep and the
        two constants come from the binary -- `re_query.pushed_n` for the `n`,
        the decoded `add ax,imm` for the base.
        """
        s = self.sell()
        swept = {"1000:%04x" % i.off for i in self.sell_run()
                 if i.raw == RANDOM_CALL}
        recorded = {a["roll"]["call"]["addr"] for a in s["arms"]}
        self.assertIsNone(s["junk_arm"]["roll"],
                          "the junk arm is recorded as drawing; the sweep "
                          "below is what decides that")
        self.assertEqual(
            swept, recorded,
            "the `Random` call sites inside %s..%s are %r, the artifact "
            "records %r" % (s["range"]["start"], s["range"]["end"],
                            sorted(swept), sorted(recorded)))
        self.assertEqual(len(swept), s["sweeps"]["random_call_sites"],
                         "the range holds %d draws, `sweeps` says %d"
                         % (len(swept), s["sweeps"]["random_call_sites"]))
        for a in s["arms"]:
            roll = a["roll"]
            got = re_query.pushed_n(self.prog, roll["call"]["addr"])
            self.assertEqual(
                got["n"], roll["n"],
                "arm %d: the idiom before %s pushes %r, the artifact records "
                "%r" % (a["n"], roll["call"]["addr"], got["n"], roll["n"]))
            add = self.check_insn(roll["base_add"], "arm %d base" % a["n"])
            self.assertEqual(
                add.text, "add ax,0x%x" % roll["base"],
                "arm %d: the base add at %s is %r, the artifact records %d"
                % (a["n"], roll["base_add"]["addr"], add.text, roll["base"]))
            self.assertEqual(roll["refund"],
                             "%d + Random(%d)" % (roll["base"], roll["n"]))
            self.assertEqual(roll["refund_min"], roll["base"])
            self.assertEqual(roll["refund_max"], roll["base"] + roll["n"] - 1)
        # and the same table, restated in `refund_finding`, must not drift
        for rec, a in zip(s["refund_finding"]["per_arm"], s["arms"]):
            self.assertEqual(rec["call"], a["roll"]["call"])
            self.assertEqual(rec["refund"], a["roll"]["refund"])
            self.assertEqual(rec["refund_min"], a["roll"]["refund_min"])
            self.assertEqual(rec["refund_max"], a["roll"]["refund_max"])
            self.assertEqual(rec["buy_price"], a["buy_price"])

    def test_the_range_reads_no_price_byte_and_writes_no_stat_global(self):
        """The two negatives the refund finding and the no-unwind finding rest
        on, each measured by the decoder AND by a raw byte scan.

        `sweeps.dgroup_addresses` is asserted SET-EQUAL to every direct-memory
        operand value in the range, so the two `assertNotIn`s below are
        statements about the complete set rather than about a list someone
        stopped writing.
        """
        s = self.sell()
        lo, hi = self.sell_range()
        seen = sorted({o.value for i in self.sell_run() for o in i.operands
                       if o.kind in ("disp16", "moffs16")})
        recorded = [int(x, 16) for x in s["sweeps"]["dgroup_addresses"]]
        self.assertEqual(
            seen, recorded,
            "the DGROUP addresses the range references are %r, `sweeps` "
            "records %r" % (["0x%04x" % v for v in seen],
                            s["sweeps"]["dgroup_addresses"]))
        raw = self.img[lo:hi]
        for v in self.PRICE_BYTES + self.STAT_GLOBALS:
            self.assertNotIn(
                v, seen,
                "0x%04x is referenced inside the sell range, so `the refund "
                "is not read from a price` / `nothing is unwound` is false"
                % v)
            self.assertNotIn(
                v.to_bytes(2, "little"), raw,
                "the operand bytes of 0x%04x occur inside %s..%s, so the "
                "negative above rests on the decoder alone"
                % (v, s["range"]["start"], s["range"]["end"]))
        self.assertIn(0x38c7, seen, "the money is not referenced in the sell "
                                    "range at all, so this sweep is not "
                                    "looking at the sell path")

    # ------------------------------------------------------- the flow claims
    def test_each_gate_branch_lands_where_the_flow_claim_needs_it(self):
        """The `own && (any better rung)` shape, as branch TARGETS.

        Every claim about the gate is a claim about where a branch goes, so
        each target is decoded and compared against the address the shape
        requires: the own-branch reaches the first conjunct, every conjunct
        reaches the offer, and every miss reaches the NEXT arm's span start.
        """
        s = self.sell()
        nxt = [a["span"]["start"] for a in s["arms"][1:]] \
            + [s["tail"]["span"]["start"]]
        for a, after in zip(s["arms"], nxt):
            own = [g for g in a["gates"] if g["kind"] == "own"][0]
            lad = [g for g in a["gates"] if g["kind"] == "ladder"][0]
            offer = [x for x in a["strings"] if x["role"] == "offer"][0]
            self.assertEqual(
                "1000:%04x" % self.branch_target(self.at(own["branch"]["addr"])),
                lad["conjuncts"][0]["test"]["addr"],
                "arm %d: the own-flag branch does not reach the first "
                "required-flag test" % a["n"])
            for c in lad["conjuncts"]:
                self.assertEqual(
                    "1000:%04x" % self.branch_target(
                        self.at(c["branch"]["addr"])),
                    offer["push"]["addr"],
                    "arm %d: the %s conjunct does not jump forward to the "
                    "offer, so the gate is not a short-circuit `or`"
                    % (a["n"], c["ds"]))
            for g in a["gates"]:
                self.assertEqual(
                    "1000:%04x" % self.branch_target(
                        self.at(g["miss"]["addr"])), after,
                    "arm %d: gate %r's miss does not reach the next arm"
                    % (a["n"], g["name"]))
                self.assertTrue(g["silent"])
            self.assertEqual(
                "1000:%04x" % self.branch_target(
                    self.at(a["answer"]["decline_branch"]["addr"])), after,
                "arm %d: declining does not reach the next arm" % a["n"])
            self.assertEqual(
                {c["ds"] for c in lad["conjuncts"]},
                set(a["requires_any_of"]),
                "arm %d: `requires_any_of` and the conjuncts disagree" % a["n"])

    def test_an_accepted_arm_falls_straight_into_the_next_arms_gate(self):
        """`wes` is six sequential offers, not a six-way choice.

        The claim is that ONE `wes` can sell up to six items, and it rests on
        there being no jump between an arm's confirmation and the next arm's
        first gate.  So the instruction after each confirmation `WriteLn` is
        decoded and must BE the next arm's own-flag test.
        """
        s = self.sell()
        nxt = [a["gates"][0]["test"]["addr"] for a in s["arms"][1:]] \
            + [s["tail"]["sentinel_test"]["addr"]]
        for a, after in zip(s["arms"], nxt):
            conf = [x for x in a["strings"]
                    if x["role"] == "confirmation"][0]
            write = self.at(conf["write"]["addr"])
            self.assertEqual(
                "1000:%04x" % write.end, after,
                "arm %d: the instruction after its confirmation is not %s, "
                "so the arms are not sequential" % (a["n"], after))

    def test_every_arm_rolls_and_stores_before_it_asks(self):
        """`sentinel_finding` and `the_draw_is_spent_either_way`, as ORDER.

        Both claims are about the position of `mov [0x3e33],al` relative to
        the yes/no compare; a port that rolls on acceptance would satisfy
        every other check here.
        """
        s = self.sell()
        pro = addrmod.image_off_of_citation(s["prologue"]["addr"])
        for a in s["arms"]:
            store = addrmod.image_off_of_citation(a["roll"]["store"]["addr"])
            call = addrmod.image_off_of_citation(a["roll"]["call"]["addr"])
            cmp_ = addrmod.image_off_of_citation(
                a["answer"]["compare"]["addr"])
            read = addrmod.image_off_of_citation(
                a["read"]["read_string"]["addr"])
            self.assertLess(pro, call, "arm %d rolls before the sentinel is "
                                       "set" % a["n"])
            self.assertLess(read, call,
                            "arm %d draws before it reads the answer" % a["n"])
            self.assertLess(store, cmp_,
                            "arm %d stores its roll AFTER the yes/no compare, "
                            "so declining would leave the sentinel intact and "
                            "`sentinel_finding` would be false" % a["n"])
        self.check_insn(s["sentinel_finding"]["where"]["test"], "sentinel")
        self.assertEqual(s["sentinel_finding"]["where"]["test"]["text"],
                         "cmp byte [0x3e33],0xff")
        self.assertEqual(s["prologue"]["text"], "mov byte [0x3e33],0xff")

    def test_the_junk_credit_carries_no_rate(self):
        """`x`'s central question: is the junk scaled before it is credited?

        Answered over the whole junk span rather than at the two addresses --
        a shift, a multiply or a divide anywhere in it would be a rate.
        """
        j = self.sell()["junk_arm"]
        lo = addrmod.image_off_of_citation(j["span"]["start"])
        hi = addrmod.image_off_of_citation(j["span"]["end"])
        run = dis16.decode_run(self.img, lo, hi)
        gate = addrmod.image_off_of_citation(j["gates"][0]["test"]["addr"])
        after = addrmod.image_off_of_citation(j["strings"][0]["push"]["addr"])
        seq = [i.text for i in run if gate <= i.off < after]
        self.assertEqual(
            seq, j["rate_finding"]["sequence"],
            "the junk arm's gate-to-store path in orig/g.exe is %r, the "
            "artifact records %r -- the one-for-one claim is exactly the "
            "absence of anything between the load and the add"
            % (seq, j["rate_finding"]["sequence"]))
        scaled = [i.text for i in run
                  if re.match(r"^(mul|imul|div|idiv|shl|shr|sar|sal|rol|ror)\b",
                              i.text)]
        self.assertEqual(scaled, [],
                         "the junk arm contains %r, so a rate stands between "
                         "the junk count and the credit" % scaled)
        self.assertEqual(
            {"1000:%04x" % i.off for i in run if i.raw == RANDOM_CALL}, set(),
            "the junk arm draws, so the refund is not the raw count")
        # and the refusal really is where the gate's branch lands
        gate0 = j["gates"][0]
        refusal = [x for x in j["strings"]
                   if x["cs_offset"] == gate0["refusal_cs_offset"]][0]
        self.assertEqual(
            "1000:%04x" % self.branch_target(self.at(gate0["branch"]["addr"])),
            refusal["push"]["addr"],
            "the junk gate's branch does not land on the push of %s, so the "
            "refusal is attributed to the wrong path"
            % gate0["refusal_cs_offset"])

    def test_the_tail_overwrites_the_buffer_so_the_wes_path_always_loops(self):
        """`buffer_overwrite_finding`, as the DIFFERENCE between two literals.

        The `w` compare at the handler's exit reads the same buffer each arm's
        ReadLn wrote.  What makes the `wes` path unconditional is that the
        assign at `1000:d368` puts a literal there that is NOT `w` -- so both
        literals are decoded and asserted different, and the assign is
        asserted to precede the compare.
        """
        t = self.sell()["tail"]
        assign = t["buffer_reset"]
        exit_ = t["exit_compare"]
        self.assertEqual(assign["dest_push"]["text"], "mov di,0x3a72")
        self.assertEqual(exit_["buffer_push"]["text"], "mov di,0x3a72")
        self.assertNotEqual(
            assign["literal"]["text"], exit_["literal"]["text"],
            "the tail assigns the very literal the exit compare tests, so the "
            "`wes` path would LEAVE the shop rather than loop")
        self.assertLess(
            addrmod.image_off_of_citation(assign["call"]["addr"]),
            addrmod.image_off_of_citation(exit_["compare"]["addr"]),
            "the assign does not precede the exit compare")
        self.assertEqual(
            "1000:%04x" % self.branch_target(
                self.at(exit_["leave_branch"]["addr"])),
            exit_["leave"]["addr"])
        self.assertEqual(
            "1000:%04x" % self.branch_target(self.at(exit_["leave"]["addr"])),
            exit_["leave_target"])
        self.assertEqual(
            "1000:%04x" % self.branch_target(
                self.at(exit_["loop_back"]["addr"])), exit_["loop_target"],
            "%s does not loop back to the recorded prompt push %s"
            % (exit_["loop_back"]["addr"], exit_["loop_target"]))
        # and the `wes` miss reaches the exit compare WITHOUT the assign
        self.assertEqual(
            "1000:%04x" % self.branch_target(
                self.at(self.sell()["wes_dispatch"]["miss_branch"]["addr"])),
            exit_["buffer_push"]["addr"],
            "the `wes` miss does not land past the buffer assign, so an "
            "unrecognised key would have its line overwritten too")

    # ---------------------------------------------------------- the globals
    def test_the_item_flag_names_come_from_the_character_sheets_own_lines(self):
        """Task 27's review turned on exactly this: a `named_from` that was
        never re-derived.  Each of the ten item flags is named from the sheet
        line that prints it, and both halves -- the guard that tests the flag
        and the push of the label -- are decoded here out of `orig/g.exe`.
        """
        checked = 0
        for g in self.sell()["globals"]:
            if "sheet_label" not in g or g["ds"] == "20ae:38c9":
                continue
            guard, push = g["evidence"][0], g["evidence"][1]
            self.check_insn(guard, "%s sheet guard" % g["ds"])
            self.assertEqual(
                guard["text"],
                "cmp byte [0x%x],0x0" % int(g["ds"].split(":")[1], 16),
                "%s: the recorded sheet guard does not test that flag"
                % g["ds"])
            self.check_insn(push, "%s sheet label push" % g["ds"])
            self.assertEqual(
                push["text"], "mov di,%s" % g["sheet_label"]["cs_offset"],
                "%s: the sheet label push at %s does not push CS %s, so the "
                "name is attributed to the wrong line"
                % (g["ds"], push["addr"], g["sheet_label"]["cs_offset"]))
            self.assertEqual(
                self.cs_literal(g["sheet_label"]["cs_offset"]),
                g["sheet_label"]["text"],
                "%s: CS %s does not hold the recorded label"
                % (g["ds"], g["sheet_label"]["cs_offset"]))
            checked += 1
        self.assertEqual(checked, 10,
                         "only %d of the ten item flags carry a re-derived "
                         "sheet name" % checked)

    def test_the_sell_global_census_is_what_xrefs_to_reports(self):
        lo, hi = self.sell_range()
        store = re.compile(r"^(mov|inc|dec|add|sub|xchg)\s+"
                           r"(byte |word |dword )?\[0x[0-9a-f]+\]")
        touched = {"0x%04x" % o.value for i in self.sell_run()
                   for o in i.operands if o.kind in ("disp16", "moffs16")}
        recorded_ds = {g["ds"] for g in self.sell()["globals"]}
        self.assertEqual(
            recorded_ds | {MONEY},
            {"20ae:" + t[2:] for t in touched},
            "`globals[]` plus the money is not the set of DGROUP addresses "
            "the range touches")
        for g in self.sell()["globals"]:
            x = g["xrefs"]
            scan = self.xrefs(g["ds"])
            acc = scan["accepted"]
            inside = [a for a in acc
                      if lo <= int(a["image_off"], 16) < hi]
            self.assertEqual(scan["raw_hits"], x["raw_hits"],
                             "%s: %d raw hits, artifact says %d"
                             % (g["ds"], scan["raw_hits"], x["raw_hits"]))
            self.assertEqual(len(acc), x["accepted"],
                             "%s: %d accepted, artifact says %d"
                             % (g["ds"], len(acc), x["accepted"]))
            self.assertEqual(len(scan["discarded"]), x["discarded"],
                             "%s: %d discarded, artifact says %d"
                             % (g["ds"], len(scan["discarded"]),
                                x["discarded"]))
            self.assertEqual([d["why"] for d in scan["discarded"]],
                             x["discarded_why"])
            self.assertEqual(len(inside), x["refs_in_range"],
                             "%s: %d references in the sell range, artifact "
                             "says %d" % (g["ds"], len(inside),
                                          x["refs_in_range"]))
            self.assertEqual(len(acc) - len(inside), x["refs_outside_range"])
            self.assertEqual(
                sorted(a["at"] for a in acc if store.match(a["text"])),
                x["writers_image_wide"],
                "%s: the image-wide writer set is not what the artifact "
                "records -- `ladder_finding.why_not_equipped` rests on it"
                % g["ds"])
            self.assertEqual(x["command"],
                             "python3 tools/re_query.py xrefs-to %s" % g["ds"])

    def test_the_top_rung_is_never_cleared_and_the_scratch_byte_is_private(self):
        """The two claims that are about a set being EMPTY.

        `20ae:394c` (Тесак) is the rung nothing sells, and `20ae:3e33` is the
        byte nothing outside this range reads -- which is what lets it be
        named from its own use rather than borrowed from a neighbour.
        """
        s = self.sell()
        by = {g["ds"]: g for g in s["globals"]}
        lo, hi = self.sell_range()
        cleaver = [a for a in self.xrefs("20ae:394c")["accepted"]
                   if lo <= int(a["image_off"], 16) < hi
                   and a["text"].startswith("mov ")]
        self.assertEqual(cleaver, [],
                         "20ae:394c is written inside the sell range, so the "
                         "cleaver IS sellable and the ladder finding is wrong")
        self.assertEqual(by["20ae:394c"]["xrefs"]["writers_image_wide"],
                         ["1000:573e"])
        self.assertEqual(
            by["20ae:3e33"]["xrefs"]["refs_outside_range"], 0,
            "20ae:3e33 is referenced outside the sell range, so it is not "
            "the private scratch byte its `named_from` says it is")
        for ds in ("20ae:38b7", "20ae:38b8", "20ae:38b9"):
            self.assertEqual(
                len(by[ds]["xrefs"]["writers_image_wide"]), 1,
                "%s has more than one image-wide writer, so it could be an "
                "equipped bit after all" % ds)

    def test_the_ladders_are_the_arms_own_flags(self):
        """`ladder_finding.ladders[]` must be a view of the arms, not prose.

        The whole finding is that an arm sells a rung and needs a HIGHER one,
        so each ladder's `sellable` list has to be exactly the own-flags of
        the arms whose rung it holds, and each arm's required flags have to be
        rungs of its own ladder and strictly above it.
        """
        s = self.sell()
        arms = {a["own_flag"]: a for a in s["arms"]}
        seen = set()
        for lad in s["ladder_finding"]["ladders"]:
            rungs = lad["rungs"]
            self.assertEqual(
                lad["sellable"], [r for r in rungs if r in arms],
                "ladder %r: `sellable` is not the rungs an arm actually "
                "clears" % lad["ladder"])
            for i, r in enumerate(rungs):
                a = arms.get(r)
                if a is None:
                    continue
                self.assertEqual(
                    a["requires_any_of"], rungs[i + 1:],
                    "the arm that sells %s requires %r, but the rungs above "
                    "it on the %r ladder are %r"
                    % (r, a["requires_any_of"], lad["ladder"], rungs[i + 1:]))
            seen.update(rungs)
        self.assertEqual(
            seen, {g["ds"] for g in s["globals"]
                   if g["ds"] not in ("20ae:38c9", "20ae:3e33")},
            "the ladders do not cover every item flag the range touches")

    def test_the_arithmetic_coincidence_is_stated_as_a_coincidence(self):
        """The one place this map states a numeric relation.

        The base happens to equal `(price + 1) div 2` for all five priced arms
        and the maximum happens to equal `price * 4 div 5` for four of them.
        Both halves are recomputed here so the table cannot drift into
        claiming a formula the binary does not contain.
        """
        f = self.sell()["refund_finding"]["arithmetic_coincidence"]
        rows = f["table"]
        self.assertEqual(len(rows), 5, "there are five priced sell arms")
        halves = [r["base"] == (r["price"] + 1) // 2 for r in rows]
        fifths = [r["max"] == r["price"] * 4 // 5 for r in rows]
        self.assertEqual(halves, [True] * 5,
                         "the `(price + 1) div 2` half of the coincidence no "
                         "longer holds for every priced arm: %r" % rows)
        self.assertEqual(
            sum(fifths), 4,
            "the coincidence text says four of five maxima are `price * 4 "
            "div 5`; the table gives %d" % sum(fifths))
        self.assertFalse(
            fifths[4],
            "arm 5 (Дубинка) is the one the text names as the exception")


class ProseTest(unittest.TestCase):
    """`docs/re/shop-arms.md` re-derived from `orig/g.exe`.

    The artifact half is checked above; the prose is where a wrong address
    actually propagates, because that is what the next task reads.

    Both surfaces of the file are covered, not just one: `strip_fences` has to
    remove the fenced blocks before the inline-code scan can pair backticks at
    all, so the fenced disassembly is collected separately in `setUpClass` and
    fed through the same matcher.  Until Task 25's fix round that left nine
    fences and 45 instruction lines unchecked.
    """

    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        cls.art = json.loads(ART.read_text(encoding="utf-8"))
        cls.shops = json.loads(SHOPS.read_text(encoding="utf-8"))
        raw = DOC.read_text(encoding="utf-8")
        cls.raw = raw
        cls.md = strip_fences(raw)
        cls.spans = inline_spans(cls.md)
        # `strip_fences` exists so the single-backtick pairing cannot
        # desynchronise, but it also hides every pasted disassembly BLOCK from
        # the two checks below -- nine fences carrying 45 `1000:xxxx <insn>`
        # lines in this file.  So the fenced lines are collected separately
        # and fed through the same matcher.  The trailing `; ...` comment some
        # of them carry is stripped; continuation lines that name no address
        # (`push ds / push di`, `...`) simply do not match and are skipped by
        # the caller, exactly as a non-matching inline span is.
        cls.fenced = [re.sub(r"\s*;.*$", "", ln).strip()
                      for blk in re.findall(r"^```.*?^```", raw,
                                            re.S | re.M)
                      for ln in blk.split("\n")]

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
        checked = self._check_instruction_lines(self.spans, "inline span")
        self.assertGreaterEqual(
            checked, 25,
            "only %d `1000:xxxx <instruction>` spans found in the prose; the "
            "pattern has drifted and this test is checking almost nothing"
            % checked)

    def test_every_instruction_inside_a_fence_says_what_the_binary_says(self):
        """The fenced blocks, which `strip_fences` hides from the scan above.

        Nine fences in this file hold pasted disassembly -- the two
        better-weapon conjunctions, the four effect blocks, the loot-arm
        counter-example and the three `mar` upgrade splits.  Every one of
        those lines is a claim about the binary, and until this test existed
        none of them was checked: `bugs[].where.delta_rejoins` covers three of
        the seventeen `mar` ones and nothing covered the rest.
        """
        checked = self._check_instruction_lines(self.fenced, "fenced line")
        self.assertGreaterEqual(
            checked, 40,
            "only %d `1000:xxxx <instruction>` lines found inside fences; "
            "either the fences were rewritten or the comment stripping "
            "broke, and this test is checking almost nothing" % checked)

    def _check_instruction_lines(self, lines, what):
        checked = 0
        for line in lines:
            m = re.match(r"^(1000:[0-9a-f]{4})\s+([a-z].*)$", line)
            if not m:
                continue
            cit, text = m.groups()
            self.assertIn(cit, self.aligned,
                          "%s %r: not a boundary" % (what, line))
            checked += 1
            self.assertEqual(
                self.aligned[cit].text, text,
                "docs/re/shop-arms.md writes `%s %s` in a %s, but "
                "tools/dis16.py decodes %r there"
                % (cit, text, what, self.aligned[cit].text))
        return checked

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
        rows = self.art["rows"] + self.art["mar"]["rows"]
        for r in rows:
            for cit in (r["compare_addr"], r["debit_addr"],
                        r["span"]["start"]):
                self.assertIn(
                    cit, self.md,
                    "docs/re/shop-arms.md never names %s, which "
                    "data/shop_arms.json records for %s row %s"
                    % (cit, r["shop"], r["key"]))
            for s in r["strings"]:
                self.assertIn(
                    s["cs_offset"], self.md,
                    "%s row %s: the prose does not carry CS %s"
                    % (r["shop"], r["key"], s["cs_offset"]))

    #: How `docs/re/shop-arms.md` advertises `SellTest`'s size.  Kept as a
    #: single spelling so both mentions are found by one pattern; a count
    #: written any other way is invisible here, which is why the doc says
    #: this and nothing else.
    #: `\s+`, not a literal space: the count and the class name can fall on
    #: either side of a line wrap, and a wrap must not hide a count.  The
    #: floor below caught exactly that while this check was being written.
    SELLTEST_COUNT = re.compile(r"\b([a-z]+)\s+`SellTest` cases\b")

    #: Only the spellings the doc may use.  A number word outside this map
    #: fails loudly rather than being skipped.
    NUMBER_WORDS = {"fourteen": 14, "fifteen": 15, "sixteen": 16,
                    "seventeen": 17, "eighteen": 18, "nineteen": 19,
                    "twenty": 20, "twentyone": 21}

    def test_the_selltest_count_this_file_advertises_is_the_real_one(self):
        """A count in the shipped map, guarded by `unittest`'s own case list.

        Task 29 shipped `sixteen` twice against a class of seventeen. The
        number is not decoration: it is what a reader uses to decide whether
        the sweep list in the class docstring is the whole set.
        """
        real = len(unittest.TestLoader().getTestCaseNames(SellTest))
        found = self.SELLTEST_COUNT.findall(self.raw)
        self.assertEqual(
            len(found), 2,
            "docs/re/shop-arms.md advertises `SellTest`'s size %d time(s), "
            "not the 2 this check was written over -- either a mention was "
            "removed or one was written in a spelling the pattern does not "
            "see, which is how the wrong count shipped" % len(found))
        for word in found:
            self.assertIn(
                word, self.NUMBER_WORDS,
                "docs/re/shop-arms.md spells `SellTest`'s size %r, which is "
                "not a number word this check can read" % word)
            self.assertEqual(
                self.NUMBER_WORDS[word], real,
                "docs/re/shop-arms.md says %s `SellTest` cases; "
                "unittest.TestLoader finds %d" % (word, real))

    def test_the_prose_and_the_artifact_agree_on_the_sell_path(self):
        """The two-places rule for Task 29's half.

        The addresses Task 30 will actually type -- each arm's gate, its
        draw, the flag it clears and the credit -- plus every literal, plus
        the four range anchors the extent claim rests on.

        This one searches the RAW file, not `strip_fences`'s output: half the
        sell path's addresses are quoted inside fenced disassembly, and those
        lines carry their own identity check in
        `test_every_instruction_inside_a_fence_says_what_the_binary_says`.
        """
        s = self.art["sell"]
        for a in s["arms"]:
            for cit in (a["span"]["start"], a["roll"]["call"]["addr"],
                        a["clear_addr"], a["credit_addr"]):
                self.assertIn(
                    cit, self.raw,
                    "docs/re/shop-arms.md never names %s, which "
                    "data/shop_arms.json records for sell arm %d"
                    % (cit, a["n"]))
            for x in a["strings"]:
                self.assertIn(x["cs_offset"], self.raw,
                              "sell arm %d: the prose does not carry CS %s"
                              % (a["n"], x["cs_offset"]))
            self.assertIn(a["roll"]["refund"], self.raw,
                          "the prose does not state sell arm %d's refund %r"
                          % (a["n"], a["roll"]["refund"]))
        j = s["junk_arm"]
        for cit in (j["compare"]["addr"], j["gates"][0]["branch"]["addr"],
                    j["effects"][0]["addr"], j["effects"][1]["addr"]):
            self.assertIn(cit, self.raw,
                          "the prose never names %s from the junk arm" % cit)
        for cit in (s["range"]["start"], s["range"]["end"],
                    s["range"]["last_instruction"]["addr"], "1000:c4cf"):
            self.assertIn(cit, self.raw,
                          "the prose never names %s, which the extent claim "
                          "rests on" % cit)
        self.assertIn(str(s["sweeps"]["instructions"]), self.raw,
                      "the prose does not state the range's instruction "
                      "count, so its span claim rests on nothing")
        for e in s["tail"]["strings"] + [s["tail"]["buffer_reset"]["literal"],
                                         s["tail"]["exit_compare"]["literal"]]:
            self.assertIn(e["cs_offset"], self.raw,
                          "the prose does not carry the tail literal CS %s"
                          % e["cs_offset"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
