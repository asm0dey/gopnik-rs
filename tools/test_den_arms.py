#!/usr/bin/env python3
"""`data/den_arms.json` and `docs/re/den.md` re-derived from `orig/g.exe`.

The artifact and the prose are the two places the same claims about the den
(`pr`, `1000:d802`..`1000:df06`) live; this is what stops either drifting from
the binary it describes.  Nothing here reads `src/`, a screen, or Ghidra's C.
`tools/test_shop_arms.py` is the model, and the same two signals are kept
apart for the same reason (`docs/re/METHODOLOGY.md`, "Is this address a call
site?"):

  * **alignment** -- the address is reached by decoding forward from its
    enclosing function's entry, so it is a real instruction boundary and not a
    byte-scan hit in the middle of one;
  * **identity** -- the instruction decoded there says what the artifact says
    it says.

Seven claims are not restatements of a single decode, and every one of them is
asserted by SET EQUALITY against a sweep of the binary rather than by checking
that the listed entries hold up.  That distinction is the whole point of this
file: an inventory that is factually complete and guarded by nothing is the
defect every review on this project has found.

  * **`strings[]` is complete.**  Every `mov di,imm16` followed by `push cs` /
    `push di` -- the CS-literal push idiom -- inside `1000:d802`..`1000:df06`
    must appear exactly once in the artifact, and every literal the artifact
    records must be one of those pushes.  So "this arm prints nothing else",
    and "the `w` arm prints nothing at all", are measurements over the range.
  * **the gate inventory is complete.**  Every CONDITIONAL branch in the range
    must be named somewhere in the artifact.  The string sweep cannot see a
    silent gate, and "an unrecognised key is silent" is a headline claim.
  * **`draws[]` is complete.**  The five-byte `Random` far-call signature is
    swept over the whole range, because a draw nobody recorded still advances
    the RNG stream and would desynchronise every trace after it.  Each `n` is
    re-derived with `tools/re_query.py`'s own walk-back, not copied.
  * **`effects[]` is complete.**  Every instruction in the range carrying an
    absolute-memory operand must fall in a WRITE bucket or a READ bucket -- an
    unclassified one fails loudly -- and the WRITE bucket must equal the union
    of the artifact's effects.  This is what makes "the `s` arm writes
    nothing" a measurement.
  * **the three threshold blocks differ, and the difference is exact.**  The
    bytes are re-sliced out of `orig/g.exe`: blocks #1 and #2 must be equal,
    block #3 must not, and the nine-byte shortfall must be exactly the four
    instructions the artifact names.  A port that folds the three into one
    helper is the mistake this case exists to catch.
  * **the `param_1` inventory in `FUN_1000_3d11` is complete -- BOTH ways.**
    "`FUN_1000_3d11` distinguishes exactly 0, 1, 3, 4 and 6" is a NEGATIVE
    over another function, so the walk's instruction count is asserted too:
    without it an empty hit list could mean an empty search.  TWO sweeps are
    needed, not one -- `[bp+0x4]` ModRM references AND `cmp al,imm8`, because
    `1000:3d24` copies the parameter into `al` and the real dispatch is a
    register-compare chain the first sweep cannot see.  The chain's
    contiguity is asserted as well, since that is what makes a register
    compare readable as a parameter test.
  * **`globals[].named_from` cannot fabricate an instruction.**  Fix round 1
    found three false provenance claims in that field -- a `dec [0x38a4]` at
    an address that decodes `cmp byte [bp+di-0x10a],0x34`, a read described as
    a write, and an "only writer" that had three more.  All three escaped
    every check here, because the identity walk only sees dicts with SEPARATE
    `addr` and `text` keys and instruction text embedded in a prose string has
    neither.  So every `` `1000:xxxx <text>` `` span in every string value in
    the artifact is now decoded too, and each global's `xrefs` census is
    re-derived by running `re_query.xrefs_to` -- which turns "only writer"
    from a phrase into a measured set.
  * **the range tiles.**  The seven arm spans, the menu, the prompt and the
    two boundary blocks must cover `1000:d802`..`1000:df06` end to end with no
    gap and no overlap, so a block cannot be dropped from the map by being
    left out of every span.

    python3 tools/test_den_arms.py
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
ART = REPO / "data" / "den_arms.json"
BRANCHES = REPO / "data" / "branches.json"
FUNCTIONS = REPO / "data" / "functions.json"
TABLES = REPO / "data" / "string_tables.json"
DOC = REPO / "docs" / "re" / "den.md"

#: The half-open image range this map owns: the `pr` verb compare through, but
#: not including, the `kl` verb compare.
LO, HI = 0xd802, 0xdf06

#: The `Random` far call, `call 0f78:114b`, by its exact five bytes.  Used to
#: SWEEP the range for draws, never to confirm the recorded ones decode.
RANDOM_CALL = b"\x9a\x4b\x11\x78\x0f"

#: The Borland shortstring compare, `call 0f78:0bd8`.  Every key compare in
#: this handler is this exact call.
STR_COMPARE = b"\x9a\xd8\x0b\x78\x0f"

#: How an instruction that WRITES an absolute-memory operand decodes, and how
#: one that only READS one does.  `tools/dis16.py` carries no read/write flag,
#: so the classification is by decoded text -- and the MNEMONIC decides, never
#: operand order: `cmp byte [0x3b78],0x1` puts memory first and writes
#: nothing.  The two buckets are asserted EXHAUSTIVE over the range, so a
#: write shape neither describes fails the sweep instead of vanishing from it.
#: `push [N]` is in the READ bucket on purpose -- it reads that word and
#: writes only the stack -- and `xchg` is in NEITHER, because `xchg ax,[N]`
#: puts memory second and still writes it; leaving it unclassified fails the
#: sweep rather than letting it pass as a read.
WRITES_ABS_MEM = re.compile(
    r"^(mov|add|sub|adc|sbb|and|or|xor|inc|dec|neg|not"
    r"|shl|shr|sar|rol|ror|rcl|rcr)\s+"
    r"(byte |word |dword )?\[0x[0-9a-f]+\]")
READS_ABS_MEM = re.compile(
    r"^(cmp|test|push)\s+(byte |word |dword )?\[0x[0-9a-f]+\]"
    r"|^(?!xchg\b)[a-z]{2,5}\s+[a-z]{2,3},"
    r"(byte |word |dword )?\[0x[0-9a-f]+\]")


def cit(off):
    return "1000:%04x" % off


class DenTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.hdr = addrmod.header_bytes(addrmod.read_exe())
        cls.art = json.loads(ART.read_text(encoding="utf-8"))
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        cls.prog = re_query.Program()
        cls.md = DOC.read_text(encoding="utf-8")
        cls.spans = inline_spans(strip_fences(cls.md))
        # The aligned walk over the den's own range, decoded from the
        # enclosing function's entry so every offset below is a real
        # boundary.  `entry` (1000:ab59) is what contains the handler.
        cls.insns = [ins for off, ins in cls.aligned.items()
                     if LO <= ins.off < HI and off.startswith("1000:")]
        cls.insns.sort(key=lambda i: i.off)

    # ---------------------------------------------------------------- helpers
    def at(self, c):
        if c not in self.aligned:
            self.fail("%s is not an instruction boundary reached by decoding "
                      "forward from any enclosing function's entry -- the "
                      "citation is a byte-scan hit, not an address" % c)
        return self.aligned[c]

    def cs_literal(self, off):
        if isinstance(off, str):
            off = int(off, 16)
        n = self.img[off]
        return self.img[off + 1:off + 1 + n].decode("cp866")

    def walk(self, want):
        """Every dict in the artifact carrying all of `want`, with its path."""
        def rec(node, path):
            if isinstance(node, dict):
                if all(isinstance(node.get(k), str) for k in want):
                    yield node, path
                for k, v in node.items():
                    yield from rec(v, "%s.%s" % (path, k))
            elif isinstance(node, list):
                for i, v in enumerate(node):
                    yield from rec(v, "%s[%d]" % (path, i))
        return list(rec(self.art, "$"))

    def arms(self):
        return self.art["arms"]

    def span_offs(self, node):
        return (int(node["span"]["start"].split(":")[1], 16),
                int(node["span"]["end"].split(":")[1], 16))

    def all_addresses(self):
        """Every `1000:xxxx` string anywhere in the artifact."""
        out = set()

        def rec(node):
            if isinstance(node, dict):
                for v in node.values():
                    rec(v)
            elif isinstance(node, list):
                for v in node:
                    rec(v)
            elif isinstance(node, str):
                out.update(CITE.findall(node))
        rec(self.art)
        return out

    # ------------------------------------------------------------- decode set
    def test_the_range_decodes_as_one_aligned_run(self):
        """The instruction count is the anchor every negative claim rests on.

        An empty hit list means nothing unless the walk is known to have
        covered the range; `data/den_arms.json` records the count so a walk
        that stopped early cannot pass as a search that found nothing.
        """
        self.assertEqual(
            len(self.insns), self.art["range"]["instruction_count"],
            "the aligned decode of %s..%s yields %d instructions, the "
            "artifact records %d -- one of the two is wrong and every sweep "
            "below rests on this number"
            % (cit(LO), cit(HI), len(self.insns),
               self.art["range"]["instruction_count"]))
        self.assertEqual(self.insns[0].off, LO)
        self.assertEqual(self.insns[-1].off + self.insns[-1].length, HI,
                         "the run does not end exactly on %s, so the range is "
                         "not a whole number of instructions" % cit(HI))

    def test_every_cited_instruction_decodes_to_what_the_artifact_says(self):
        seen = self.walk(("addr", "text"))
        self.assertGreater(len(seen), 150,
                           "the artifact stopped carrying instruction "
                           "records; a walk that finds nothing must not pass "
                           "(found %d)" % len(seen))
        for node, path in seen:
            ins = self.at(node["addr"])
            self.assertEqual(
                ins.text, node["text"],
                "%s: data/den_arms.json says %s at %s, orig/g.exe decodes %s "
                "there" % (path, node["text"], node["addr"], ins.text))

    #: The reviewer's own sweep, kept verbatim: an instruction claim written
    #: INSIDE a prose string, which carries neither a separate `addr` key nor
    #: a separate `text` key and so escaped every other check in this file.
    PROSE_INSN = re.compile(r"`(1000:[0-9a-f]{4})\s+([a-z][^`]*)`")

    def test_every_prose_embedded_instruction_says_what_the_binary_says(self):
        """The identity hole fix round 1 opened.

        `test_every_cited_instruction_decodes_to_what_the_artifact_says` walks
        dicts carrying SEPARATE `addr` and `text` keys.  A claim written as
        `` `1000:4a49 dec [0x38a4]` `` inside a `named_from` sentence has
        neither, so it escaped that walk, escaped the boundary walk (it IS
        aligned -- it is just a different instruction), and escaped the
        markdown scans because it lives only in the JSON.  Three such claims
        shipped, one of them load-bearing for the `d` arm's whole predicate.
        """
        found = []

        def rec(node, path):
            if isinstance(node, dict):
                for k, v in node.items():
                    rec(v, "%s.%s" % (path, k))
            elif isinstance(node, list):
                for i, v in enumerate(node):
                    rec(v, "%s[%d]" % (path, i))
            elif isinstance(node, str):
                for m in self.PROSE_INSN.finditer(node):
                    found.append((path, m.group(1), m.group(2)))
        rec(self.art, "$")
        # The floor exists so a regex that stops matching goes red rather
        # than quietly measuring nothing.  Four is the CURRENT population,
        # not a target: fix round 1 moved most instruction claims out of
        # prose and into structured `evidence[]` records, where the identity
        # walk above already sees them, so this number went DOWN and should
        # keep going down.  Lower it deliberately if it does; do not raise
        # the prose count to satisfy it.
        self.assertGreaterEqual(
            len(found), 4,
            "the prose-embedded instruction sweep matched only %d spans; a "
            "scan that measures nothing must not pass" % len(found))
        # And prove the regex can still see a claim of the shape it hunts,
        # so "no matches" can never be mistaken for "no defects".
        self.assertEqual(
            self.PROSE_INSN.findall("x `1000:4a49 dec [0x38a4]` y"),
            [("1000:4a49", "dec [0x38a4]")],
            "the prose-embedded pattern no longer matches the exact claim "
            "shape that shipped wrong in the first revision")
        for path, c, text in found:
            ins = self.at(c)
            self.assertEqual(
                ins.text, text,
                "%s: data/den_arms.json writes `%s %s` inside a prose string, "
                "but orig/g.exe decodes %r there"
                % (path, c, text, ins.text))

    def test_every_globals_xref_census_is_what_re_query_reports(self):
        """`named_from` is re-derived, not trusted.

        Brief item 2 says to name each global from its writers and readers
        with `tools/re_query.py xrefs-to`, never from the adjacent string.
        Nothing checked that until fix round 1, and three of the eighteen were
        wrong.  Now the raw/accepted/discarded counts and the image-wide
        writer set are re-derived here, so an "only writer" claim is a
        measurement or it is red.
        """
        seen = 0
        for g in self.art["globals"]:
            scan = re_query.xrefs_to(self.prog, g["ds"])["scan"]
            xr = g["xrefs"]
            self.assertEqual(
                (scan["raw_hits"], len(scan["accepted"]),
                 len(scan["discarded"])),
                (xr["raw_hits"], xr["accepted"], xr["discarded"]),
                "%s: `xrefs-to` reports raw=%d accepted=%d discarded=%d, the "
                "artifact records raw=%d accepted=%d discarded=%d"
                % (g["ds"], scan["raw_hits"], len(scan["accepted"]),
                   len(scan["discarded"]), xr["raw_hits"], xr["accepted"],
                   xr["discarded"]))
            self.assertEqual(
                xr["command"],
                "python3 tools/re_query.py xrefs-to " + g["ds"],
                "%s: the recorded command does not recompute the census "
                "beside it" % g["ds"])
            writers = [a["at"] for a in scan["accepted"]
                       if WRITES_ABS_MEM.match(a["text"])]
            if isinstance(xr["writers_image_wide"], list):
                self.assertEqual(
                    xr["writers_image_wide"], writers,
                    "%s: the artifact lists %s as its image-wide writers, "
                    "`xrefs-to` finds %s -- an 'only writer' claim that "
                    "stopped the next search is exactly what this check "
                    "exists for"
                    % (g["ds"], xr["writers_image_wide"], writers))
                seen += 1
            else:
                # The three populations too large to list are recorded as a
                # sentence naming the COUNT; the count is still re-derived.
                m = re.match(r"^(\d+) writers", xr["writers_image_wide"])
                self.assertIsNotNone(
                    m, "%s: writers_image_wide is neither a list nor a "
                       "`<n> writers ...` sentence: %r"
                       % (g["ds"], xr["writers_image_wide"]))
                assert m is not None
                self.assertEqual(
                    int(m.group(1)), len(writers),
                    "%s: the note says %s writers, `xrefs-to` finds %d"
                    % (g["ds"], m.group(1), len(writers)))
        self.assertGreaterEqual(
            seen, 14,
            "only %d globals carry an explicit writer LIST; the sweep is "
            "meant to cover every population small enough to read whole"
            % seen)
        # There is deliberately NO further "every evidence[] address is in
        # the accepted set" loop here.  It was written, and then removed:
        # `re_query.xrefs_to` accepts exactly the aligned instructions whose
        # operand field holds the address, so any `evidence` record that
        # survives the identity walk above is in that set by construction,
        # and no perturbation of an address could make it fail without the
        # identity walk firing first.  That is the assertion-that-cannot-fail
        # this file exists to keep out, so it is gone rather than kept as
        # decoration.

    def test_every_address_the_artifact_names_is_a_boundary(self):
        exempt = {e["addr"]
                  for e in self.art["known_not_boundaries"]["entries"]}
        for c in sorted(self.all_addresses() - exempt):
            self.at(c)

    def test_the_boundary_exemption_list_is_honest(self):
        """The exemption is itself a claim, so it is checked on its own.

        Each entry must really NOT be a boundary, or the list becomes a place
        to park a good address and skip the check on it.  Split out of the
        walk above so the mutation case can target it without the walk's own
        failure masking the message.
        """
        exempt = {e["addr"]
                  for e in self.art["known_not_boundaries"]["entries"]}
        self.assertTrue(exempt, "known_not_boundaries is empty")
        for c in sorted(exempt):
            self.assertNotIn(
                c, self.aligned,
                "%s is listed in known_not_boundaries but IS an instruction "
                "boundary -- the exemption is hiding a good address" % c)

    def test_every_literal_decodes_to_the_recorded_text(self):
        seen = self.walk(("cs_offset", "file_offset", "text"))
        self.assertGreaterEqual(len(seen), 30,
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

    # ----------------------------------------------------------- completeness
    def _pushed_literals(self, lo=LO, hi=HI):
        """Every CS-literal push idiom in an image range, as (addr, cs)."""
        sel = [i for i in self.insns if lo <= i.off < hi]
        out = set()
        for n, i in enumerate(sel):
            if i.raw[0] != 0xBF or n + 2 >= len(sel):
                continue
            if sel[n + 1].raw == b"\x0e" and sel[n + 2].raw == b"\x57":
                out.add((cit(i.off), "0x%04x" % i.operands[0].value))
        return out

    def test_the_recorded_strings_are_every_cs_literal_the_handler_pushes(self):
        swept = self._pushed_literals()
        self.assertEqual(
            len(swept), self.art["sweeps"]["cs_literal_pushes"],
            "the sweep finds %d CS-literal pushes, the artifact's anchor says "
            "%d" % (len(swept), self.art["sweeps"]["cs_literal_pushes"]))
        recorded = {(n["push"]["addr"], n["cs_offset"])
                    for n, _ in self.walk(("cs_offset", "text"))
                    if isinstance(n.get("push"), dict)
                    and LO <= int(n["push"]["addr"].split(":")[1], 16) < HI}
        self.assertEqual(
            recorded, swept,
            "strings[] is not complete over %s..%s.\n  in the binary but not "
            "the artifact: %s\n  in the artifact but not the binary: %s"
            % (cit(LO), cit(HI), sorted(swept - recorded),
               sorted(recorded - swept)))

    def test_the_recorded_gates_are_every_conditional_branch_in_range(self):
        swept = {cit(i.off) for i in self.insns
                 if i.text.startswith("j") and not i.text.startswith("jmp")}
        self.assertEqual(
            len(swept), self.art["sweeps"]["conditional_branches"],
            "the sweep finds %d conditional branches, the anchor says %d"
            % (len(swept), self.art["sweeps"]["conditional_branches"]))
        # `data/branches.json`, generated by Ghidra and not by `dis16`, is the
        # independent second opinion on that population.
        ghidra = {b["addr"] for b in self.branches["branches"]
                  if b["addr"].startswith("1000:")
                  and LO <= int(b["addr"].split(":")[1], 16) < HI}
        self.assertEqual(
            swept, ghidra,
            "tools/dis16.py and data/branches.json disagree about which "
            "addresses in %s..%s are conditional branches: %s"
            % (cit(LO), cit(HI), sorted(swept ^ ghidra)))
        # SET EQUALITY against the artifact's STRUCTURED branch records --
        # every {addr,text} pair whose text is a conditional branch.  A
        # looser "is this address mentioned anywhere in the file" check
        # passed with a gate's branch address swapped for its neighbour's,
        # because the neighbour was also named in a prose note; the mutation
        # gate found that, which is what it is for.
        recorded = {n["addr"] for n, _ in self.walk(("addr", "text"))
                    if n["text"].startswith("j")
                    and not n["text"].startswith("jmp")
                    and LO <= int(n["addr"].split(":")[1], 16) < HI}
        self.assertEqual(
            recorded, swept,
            "the artifact's structured conditional-branch records are not "
            "the conditional branches in %s..%s -- a gate nothing records is "
            "exactly the silent refusal this map exists to rule out.\n  in "
            "the binary but not recorded: %s\n  recorded but not in the "
            "binary: %s"
            % (cit(LO), cit(HI), sorted(swept - recorded),
               sorted(recorded - swept)))

    def test_the_recorded_draws_are_every_random_call_in_range(self):
        swept = {cit(i.off) for i in self.insns if i.raw == RANDOM_CALL}
        raw = self.img[LO:HI].count(RANDOM_CALL)
        self.assertEqual(
            len(swept), raw,
            "the aligned sweep finds %d `Random` calls but the raw byte scan "
            "finds %d in the same range -- one of them is on a boundary the "
            "other does not see" % (len(swept), raw))
        self.assertEqual(len(swept), self.art["sweeps"]["random_call_sites"])
        recorded = {d["call"]["addr"]
                    for a in self.arms() for d in a["draws"]}
        # `1000:d83f` is the ported intro's own draw and lives on a menu line,
        # not on an arm; it is named in that line's note.
        intro = {"1000:d83f"}
        self.assertTrue(
            intro <= self.all_addresses(),
            "the intro draw 1000:d83f is not named anywhere in the artifact")
        self.assertEqual(
            recorded | intro, swept,
            "draws[] is not every `Random` site in the range: %s"
            % sorted(swept ^ (recorded | intro)))

    def test_every_recorded_draw_pushes_the_n_it_records(self):
        for a in self.arms():
            for d in a["draws"]:
                got = re_query.pushed_n(self.prog, d["call"]["addr"])
                self.assertEqual(got["n_at"], d["n_at"])
                self.assertEqual(got["n_bytes"], d["n_bytes"])
                self.assertEqual(
                    got.get("n_expr") or got.get("n"), d["n"],
                    "arm %s draw %s: the idiom at %s pushes %r, the artifact "
                    "records %r" % (a["key"], d["call"]["addr"], got["n_at"],
                                    got.get("n_expr") or got.get("n"),
                                    d["n"]))

    def _classify_abs_mem(self, lo, hi):
        """(writes, reads) over an image range; unclassified fails loudly."""
        writes, reads = [], []
        for i in self.insns:
            if not (lo <= i.off < hi):
                continue
            if not any(op.kind in ("disp16", "moffs16") for op in i.operands):
                continue
            if WRITES_ABS_MEM.match(i.text):
                writes.append(cit(i.off))
            elif READS_ABS_MEM.match(i.text):
                reads.append(cit(i.off))
            else:
                self.fail(
                    "%s %r carries an absolute-memory operand and matches "
                    "neither the WRITE nor the READ shape -- the buckets are "
                    "asserted exhaustive, so an unclassified form must fail "
                    "rather than shrink the sweep" % (cit(i.off), i.text))
        return writes, reads

    def test_the_recorded_effects_are_every_absolute_write_in_range(self):
        writes, reads = self._classify_abs_mem(LO, HI)
        self.assertEqual(
            len(writes), self.art["sweeps"]["absolute_memory_writes"],
            "the sweep finds %d absolute writes, the anchor says %d"
            % (len(writes), self.art["sweeps"]["absolute_memory_writes"]))
        # SET EQUALITY against the arms' and the menu lines' own effects[].
        # `globals[].written_in_range` is deliberately NOT folded in: it
        # carries the same addresses and made this an assertion that could
        # not fail when an arm's effect address was swapped for a sibling's.
        # It has its own test below.
        recorded = {e["addr"] for a in self.arms() for e in a["effects"]}
        recorded |= {e["addr"] for m in self.art["menu_lines"]
                     for e in m["effects"]}
        self.assertEqual(
            recorded, set(writes),
            "effects[] over the arms and menu lines is not the set of "
            "absolute writes in %s..%s.\n  in the binary but not the "
            "artifact: %s\n  in the artifact but not the binary: %s"
            % (cit(LO), cit(HI), sorted(set(writes) - recorded),
               sorted(recorded - set(writes))))
        for m in self.art["menu_lines"]:
            for e in m["effects"]:
                off = int(e["ds"].split(":")[1], 16)
                self.assertIn("[0x%x]" % off, e["text"])
        # And each effect writes the DGROUP address it names.
        for a in self.arms():
            for e in a["effects"]:
                off = int(e["ds"].split(":")[1], 16)
                self.assertIn(
                    "[0x%x]" % off, e["text"],
                    "arm %s: the effect at %s says it writes %s but decodes "
                    "%r" % (a["key"], e["addr"], e["ds"], e["text"]))

    def test_the_arm_with_no_effects_really_has_none(self):
        """`s` and `w` claim to write nothing; that is measured, not asserted."""
        claimed = [a for a in self.arms() if "no_effect_claim" in a]
        self.assertEqual(
            sorted(a["key"] for a in claimed), ["s", "w"],
            "the set of arms claiming no effect changed; the sweep below is "
            "written for exactly `s` and `w`")
        for a in claimed:
            lo, hi = self.span_offs(a["no_effect_claim"])
            writes, _ = self._classify_abs_mem(lo, hi)
            self.assertEqual(
                writes, [],
                "arm %s claims to write nothing, but %s..%s contains %s"
                % (a["key"], cit(lo), cit(hi), writes))
            self.assertEqual(
                a["effects"], [],
                "arm %s carries a no_effect_claim and a non-empty effects[]"
                % a["key"])

    def test_the_w_arm_pushes_no_literal_of_its_own(self):
        w = next(a for a in self.arms() if a["key"] == "w")
        lo, hi = self.span_offs(w)
        pushed = self._pushed_literals(lo, hi)
        self.assertEqual(
            pushed, {(w["key_literal"]["push"]["addr"],
                      w["key_literal"]["cs_offset"])},
            "the `w` arm pushes something besides its own key literal, so "
            "`it prints nothing` is false: %s" % sorted(pushed))
        self.assertEqual(w["strings"], [])

    def test_the_dgroup_addresses_touched_are_the_recorded_globals(self):
        swept = set()
        for i in self.insns:
            for op in i.operands:
                if op.kind in ("disp16", "moffs16"):
                    swept.add("20ae:%04x" % op.value)
                    break
        self.assertEqual(
            len(swept), self.art["sweeps"]["dgroup_addresses_touched"],
            "the sweep finds %d DGROUP addresses as memory operands, the "
            "anchor says %d"
            % (len(swept), self.art["sweeps"]["dgroup_addresses_touched"]))
        recorded = {g["ds"] for g in self.art["globals"]}
        self.assertEqual(
            recorded, swept,
            "globals[] is not the set of DGROUP addresses this handler "
            "touches as memory operands: %s" % sorted(recorded ^ swept))

    def test_each_globals_write_list_is_its_writes_in_range(self):
        by_ds = {}
        for i in self.insns:
            for op in i.operands:
                if op.kind in ("disp16", "moffs16"):
                    if WRITES_ABS_MEM.match(i.text):
                        by_ds.setdefault("20ae:%04x" % op.value,
                                         []).append(cit(i.off))
                    break
        for g in self.art["globals"]:
            self.assertEqual(
                g["written_in_range"], by_ds.get(g["ds"], []),
                "%s: the artifact lists %s as its writes in range, the sweep "
                "finds %s" % (g["ds"], g["written_in_range"],
                              by_ds.get(g["ds"], [])))

    def test_the_ds_pointer_pushes_are_complete(self):
        swept = {}
        for n, i in enumerate(self.insns):
            if i.raw[0] != 0xBF or n + 2 >= len(self.insns):
                continue
            if self.insns[n + 1].raw == b"\x1e" \
                    and self.insns[n + 2].raw == b"\x57":
                swept.setdefault("20ae:%04x" % i.operands[0].value,
                                 []).append(cit(i.off))
        self.assertEqual(
            sum(len(v) for v in swept.values()),
            self.art["sweeps"]["ds_pointer_pushes"])
        recorded = {e["ds"]: e["pushed_at"]
                    for e in self.art["globals_not_memory_operands"]["entries"]}
        self.assertEqual(
            recorded, swept,
            "the `mov di,imm16` / `push ds` / `push di` pointer pushes in "
            "range are %s, the artifact records %s" % (swept, recorded))

    # ------------------------------------------------------------- structure
    def test_the_range_boundaries_are_the_two_verb_compares(self):
        left = self.at(self.art["verb"]["compare_addr"])
        self.assertEqual(left.off, LO)
        self.assertEqual(
            left.raw, STR_COMPARE,
            "%s is not the `call 0f78:0bd8` shortstring compare"
            % self.art["verb"]["compare_addr"])
        right = self.at(self.art["bounded_on_the_right_by"]["compare_addr"])
        self.assertEqual(right.off, HI)
        self.assertEqual(right.raw, STR_COMPARE)
        # Both verbs are compared against the STREET buffer, not the den's.
        for c, want in ((self.art["verb"]["push_buffer"]["addr"], 0x3972),
                        ("1000:defc", 0x3972)):
            ins = self.at(c)
            self.assertEqual(ins.operands[0].value, want,
                             "%s does not push 20ae:%04x" % (c, want))
        for lit, cs in ((self.art["verb"], "0x9ced"),
                        (self.art["bounded_on_the_right_by"], "0xa0ea")):
            self.assertEqual(lit["key_literal"]["cs_offset"], cs)

    def test_each_key_compare_is_the_shortstring_compare_on_the_den_buffer(self):
        buf = int(self.art["input_read"]["buffer"].split(":")[1], 16)
        keys = []
        for a in self.arms():
            ins = self.at(a["compare_addr"])
            self.assertEqual(a["compare_addr"], a["compare"]["addr"])
            self.assertEqual(
                ins.raw, STR_COMPARE,
                "arm %s: %s is not the `call 0f78:0bd8` shortstring compare "
                "(%s)" % (a["key"], a["compare_addr"], ins.raw.hex()))
            # the three-instruction buffer push, then the literal push, then
            # the compare -- read forward from the recorded buffer push.
            push = self.at(a["push_buffer"]["addr"])
            self.assertEqual(
                push.operands[0].value, buf,
                "arm %s: %s pushes 0x%04x, not the den's own buffer 0x%04x"
                % (a["key"], a["push_buffer"]["addr"],
                   push.operands[0].value, buf))
            self.assertEqual(
                self.cs_literal(a["key_literal"]["cs_offset"]), a["key"],
                "arm %s: the literal at CS %s is %r"
                % (a["key"], a["key_literal"]["cs_offset"],
                   self.cs_literal(a["key_literal"]["cs_offset"])))
            keys.append(a["key"])
        self.assertEqual(keys, self.art["key_set"])
        self.assertEqual(len(set(keys)), len(keys), "duplicate arm key")

    def test_the_arm_spans_tile_the_loop_body(self):
        spans = [self.span_offs(a) for a in self.arms()]
        for (a_lo, a_hi), (b_lo, _) in zip(spans, spans[1:]):
            self.assertEqual(
                a_hi, b_lo,
                "the arm spans do not tile: %s ends at %s, the next begins "
                "at %s" % (cit(a_lo), cit(a_hi), cit(b_lo)))
        first, last = spans[0][0], spans[-1][1]
        # The seven arms begin right after the case-fold call and end right
        # before the discovery refusal block.
        fold = self.at(self.art["input_read"]["case_fold"]["addr"])
        self.assertEqual(
            fold.off + fold.length, first,
            "the first arm does not begin where the input read ends")
        self.assertEqual(
            last, int(self.art["discovery_gate"]["refusal"]["push"]["addr"]
                      .split(":")[1], 16),
            "the last arm does not end where the discovery refusal begins")
        for lo, hi in spans:
            self.assertTrue(LO <= lo < hi <= HI)

    def test_the_loop_back_edge_returns_to_the_prompt(self):
        back = self.at(self.art["loop"]["back_edge"]["addr"])
        target = int(re.search(r"0x([0-9a-f]+)$", back.text).group(1), 16)
        self.assertEqual(
            cit(target & 0xFFFF), self.art["loop"]["top"],
            "%s jumps to %s, not to the recorded loop top %s"
            % (self.art["loop"]["back_edge"]["addr"], cit(target & 0xFFFF),
               self.art["loop"]["top"]))
        # ... and the loop top is the PROMPT push, not the menu.
        top = self.at(self.art["loop"]["top"])
        self.assertEqual(
            "0x%04x" % top.operands[0].value,
            self.art["input_read"]["prompt_literal"]["cs_offset"],
            "the loop top does not push the prompt literal, so `the menu is "
            "not reprinted` is not established")

    # -------------------------------------------------------- the three blocks
    def test_the_three_threshold_blocks_are_two_identical_and_one_different(self):
        blocks = self.art["threshold_blocks"]
        self.assertEqual(len(blocks), 3)
        raw = []
        for b in blocks:
            lo = int(b["start"].split(":")[1], 16)
            hi = int(b["end_exclusive"].split(":")[1], 16)
            got = self.img[lo:hi]
            self.assertEqual(
                got.hex(" "), b["bytes"],
                "%s: the artifact's byte string is not what is at %s..%s"
                % (b["start"], b["start"], b["end_exclusive"]))
            self.assertEqual(len(got), b["byte_length"])
            raw.append(got)
        self.assertEqual(
            raw[0], raw[1],
            "blocks #1 (%s) and #2 (%s) are NOT byte-identical, which is what "
            "the artifact claims: %s vs %s"
            % (blocks[0]["start"], blocks[1]["start"], raw[0].hex(" "),
               raw[1].hex(" ")))
        self.assertNotEqual(
            raw[0], raw[2],
            "block #3 (%s) is byte-identical to block #1, so the headline "
            "finding -- that the three are not one helper -- is false"
            % blocks[2]["start"])
        self.assertEqual(
            len(raw[0]) - len(raw[2]), 9,
            "block #3 is %d bytes shorter than block #1, not the 9 the "
            "artifact's four named instructions account for"
            % (len(raw[0]) - len(raw[2])))
        # The nine bytes are exactly `sub ax,0x5`, `mov si,ax`, one extra
        # `shl ax,1` and `add ax,si`.  Three of those four forms do not occur
        # in block #3 at all; the fourth, `shl ax,1`, occurs TWICE in #1 and
        # ONCE in #3, so it is counted rather than searched for -- an
        # assertion that #3 contains no `d1 e0` could not pass and would be a
        # check that cannot fail in the other direction.
        extra = [self.at("1000:d92f"), self.at("1000:d932"),
                 self.at("1000:d936"), self.at("1000:d938")]
        self.assertEqual([i.text for i in extra],
                         ["sub ax,0x5", "mov si,ax", "shl ax,1", "add ax,si"])
        self.assertEqual(sum(i.length for i in extra), 9)
        for i in extra:
            if i.text == "shl ax,1":
                self.assertEqual(
                    (raw[0].count(i.raw), raw[2].count(i.raw)), (2, 1),
                    "block #1 should hold two `shl ax,1` and block #3 one; "
                    "found %d and %d"
                    % (raw[0].count(i.raw), raw[2].count(i.raw)))
                continue
            self.assertIn(i.raw, raw[0])
            self.assertNotIn(
                i.raw, raw[2],
                "%r appears inside block #3 after all, so the difference is "
                "not what the artifact says it is" % i.text)
        # The shared prefix -- both discovery-flag compares and the `jz`
        # between them -- is identical in all three.
        pre = self.art["threshold_blocks_finding"]["shared_prefix_bytes"]
        self.assertEqual(len(pre.split()), 13)
        for r, b in zip(raw, blocks):
            self.assertEqual(
                r[:13].hex(" "), pre,
                "%s does not open with the shared 13-byte prefix" % b["start"])

    def test_each_threshold_block_multiplier_matches_its_recorded_formula(self):
        """`*5` vs `*2` is the whole finding; read it off the bytes."""
        want = {0: "* 5", 1: "* 5", 2: "* 2"}
        for b in self.art["threshold_blocks"]:
            texts = [self.at(a["addr"]).text for a in b["arithmetic"]]
            shls = texts.count("shl ax,1")
            adds = texts.count("add ax,si")
            mult = 5 if (shls == 2 and adds == 1) else 2 if shls == 1 else None
            self.assertIsNotNone(
                mult, "%s: %d `shl ax,1` and %d `add ax,si` is neither the "
                "*5 nor the *2 shape" % (b["start"], shls, adds))
            self.assertIn(
                want[b["index"]], b["formula"],
                "block #%d's recorded formula %r does not say %s"
                % (b["index"], b["formula"], want[b["index"]]))
            self.assertEqual(
                mult, 5 if want[b["index"]] == "* 5" else 2,
                "block #%d multiplies by %d, its formula says %s"
                % (b["index"], mult, want[b["index"]]))
            has_sub5 = "sub ax,0x5" in texts
            self.assertEqual(
                has_sub5, "- 5" in b["formula"],
                "block #%d: `sub ax,0x5` present=%s but the formula %r "
                "disagrees" % (b["index"], has_sub5, b["formula"]))

    # ------------------------------------------------------ the fight's param
    def test_the_fight_param_sites_are_every_bp4_reference_in_the_fight(self):
        funcs = json.loads(FUNCTIONS.read_text(encoding="utf-8"))
        f = next(x for x in funcs if x["entry"] == "1000:3d11")
        lo = 0x3d11
        hi = lo + f["size"]
        body = list(dis16.decode_run(self.img, lo, hi))
        self.assertEqual(
            len(body), 3043,
            "the aligned walk over FUN_1000_3d11 covers %d instructions, not "
            "the 3043 the negative claim rests on" % len(body))
        hits = []
        for i in body:
            if i.modrm is None:
                continue
            if ((i.modrm >> 6) & 3) == 1 and (i.modrm & 7) == 6 \
                    and any(op.kind == "disp8" and op.value == 4
                            for op in i.operands):
                hits.append(cit(i.off))
        rec = self.art["fight_param_finding"]
        self.assertEqual(
            hits, [s["addr"] for s in rec["sites"]],
            "the `[bp+0x4]` sweep over FUN_1000_3d11 finds %s, the artifact "
            "records %s" % (hits, [s["addr"] for s in rec["sites"]]))
        texts = [self.at(c).text for c in hits]
        self.assertIn(
            "cmp byte [bp+0x4],0x6", texts,
            "FUN_1000_3d11 does NOT compare its param against 6, so the `hp` "
            "arm's fight has no special case and the artifact's claim is "
            "false")
        # And the two den call sites really push 5 and 6.
        pushed = {}
        for a in self.arms():
            for c in a["calls_out"]:
                if c["target"] == "1000:3d11":
                    ins = self.at(c["arg_push"]["addr"])
                    pushed[a["key"]] = (ins.text, c["param_1"])
        self.assertEqual(pushed, {"hp": ("mov al,0x6", 6),
                                  "d": ("mov al,0x5", 5)})

    def test_the_fight_param_dispatch_chain_is_complete(self):
        """The `[bp+0x4]` sweep alone is NOT enough, and this is why.

        `1000:3d24` copies the parameter into `al` and every later test is a
        REGISTER compare no `[bp+0x4]` scan can see.  A first draft of
        `fight_param_finding` had only that scan and concluded `param_1 == 5
        has no compare of its own` from it -- which happens to be true, but
        was established by an inventory that had stopped searching.  So the
        `cmp al,imm8` population is swept too, and the chain's contiguity --
        no instruction between one link's miss and the next link's compare --
        is what licenses reading a register compare as a parameter test.
        """
        funcs = json.loads(FUNCTIONS.read_text(encoding="utf-8"))
        f = next(x for x in funcs if x["entry"] == "1000:3d11")
        body = list(dis16.decode_run(self.img, 0x3d11, 0x3d11 + f["size"]))
        swept = [i for i in body if i.raw[0] == 0x3C]
        disp = self.art["fight_param_finding"]["dispatch"]
        self.assertEqual(
            len(swept), disp["cmp_al_imm_sites_in_body"],
            "FUN_1000_3d11 holds %d `cmp al,imm8` instructions, the artifact "
            "records %d" % (len(swept), disp["cmp_al_imm_sites_in_body"]))
        self.assertEqual(
            [cit(i.off) for i in swept],
            [l["compare"]["addr"] for l in disp["chain"]],
            "the `cmp al,imm8` sweep finds %s, the recorded chain is %s -- a "
            "compare outside the chain would be a parameter test nobody read"
            % ([cit(i.off) for i in swept],
               [l["compare"]["addr"] for l in disp["chain"]]))
        self.assertEqual(self.at(disp["load"]["addr"]).text,
                         "mov al,[bp+0x4]")
        for n, link in enumerate(disp["chain"]):
            c = self.at(link["compare"]["addr"])
            self.assertEqual(
                c.text, "cmp al,0x%x" % link["value"],
                "chain link %d says it tests %d, %s decodes %r"
                % (n, link["value"], link["compare"]["addr"], c.text))
            b = self.at(link["branch"]["addr"])
            self.assertEqual(
                b.off, c.off + c.length,
                "chain link %d: the branch is not adjacent to its compare"
                % n)
            tgt = cit(int(re.search(r"0x([0-9a-f]+)$",
                                    b.text).group(1), 16) & 0xFFFF)
            fall = cit(b.off + b.length)
            # `jz` takes the branch on a MATCH, `jnz` on a miss.  Which of
            # the two the artifact must name for the target is decided by
            # the opcode, not by which reading is convenient.
            self.assertIn(b.text[:3], ("jz ", "jnz"),
                          "chain link %d: %r is neither `jz` nor `jnz`"
                          % (n, b.text))
            if b.text.startswith("jz"):
                self.assertEqual(tgt, link["hit_target"])
                miss_from = fall
            else:
                self.assertEqual(tgt, link["miss_reaches"],
                                 "chain link %d: the `jnz` at %s goes to %s, "
                                 "the artifact says the miss reaches %s"
                                 % (n, link["branch"]["addr"], tgt,
                                    link["miss_reaches"]))
                self.assertEqual(fall, link["hit_target"],
                                 "chain link %d: the `jnz`'s fall-through is "
                                 "%s, not the recorded hit target %s"
                                 % (n, fall, link["hit_target"]))
                miss_from = link["miss_reaches"]
            # Contiguity: nothing runs between this link's miss and the next
            # compare, so `al` still holds the parameter there.  A `jnz`
            # reaches it directly, so there is nothing to bridge.
            if miss_from != link["miss_reaches"]:
                bridge = dis16.decode(
                    self.img, int(miss_from.split(":")[1], 16))
                self.assertEqual(
                    bridge.text,
                    "jmp 0x%x" % int(link["miss_reaches"].split(":")[1], 16),
                    "chain link %d: the instruction at the fall-through %s "
                    "is %r, not one bare `jmp %s` -- so something runs "
                    "between the miss and the next compare and `al` may no "
                    "longer hold the parameter"
                    % (n, miss_from, bridge.text, link["miss_reaches"]))
                self.assertIn(
                    bridge.raw[0], (0xE9, 0xEB),
                    "chain link %d: %s is not an unconditional near/short "
                    "jump" % (n, miss_from))
        self.assertEqual(
            [l["value"] for l in disp["chain"]],
            disp["values_distinguished"])
        self.assertNotIn(
            5, disp["values_distinguished"],
            "5 is in the distinguished set, so the `d` arm's fight is not "
            "the default-arm call the artifact says it is")
        self.assertEqual(
            disp["chain"][-1]["miss_reaches"], disp["default_arm"],
            "the last link's miss does not reach the recorded default arm")
        # The default arm is not itself another `cmp al,imm8`.
        self.assertNotEqual(
            self.at(disp["default_arm"]).raw[0], 0x3C,
            "%s is another `cmp al,imm8`, so the chain does not end there"
            % disp["default_arm"])

    def test_every_call_out_reaches_the_target_it_names(self):
        """A near `call rel16` target is taken modulo 64 KiB, not summed."""
        seen = 0
        for a in self.arms():
            for c in a["calls_out"]:
                ins = self.at(c["call"]["addr"])
                self.assertEqual(ins.raw[0], 0xE8,
                                 "%s is not a near call" % c["call"]["addr"])
                disp = int.from_bytes(ins.raw[1:3], "little", signed=True)
                target = (ins.off + ins.length + disp) & 0xFFFF
                self.assertEqual(
                    cit(target), c["target"],
                    "%s calls %s, the artifact says %s"
                    % (c["call"]["addr"], cit(target), c["target"]))
                arg = self.at(c["arg_push"]["addr"])
                self.assertEqual(
                    arg.text, "mov al,0x%x" % c["param_1"],
                    "%s: the argument push decodes %r, param_1 is recorded "
                    "as %d" % (c["call"]["addr"], arg.text, c["param_1"]))
                seen += 1
        self.assertEqual(seen, 5, "expected five near calls out of the den "
                                  "(2 in `hp`, 3 in `d`), found %d" % seen)

    def test_the_hp_announcement_indexes_the_ranks_table(self):
        """`[0x3952]*0x100 + 0x2e` is the `ranks` table, not a coincidence."""
        tables = json.loads(TABLES.read_text(encoding="utf-8"))
        ranks = next(t for t in tables["tables"] if t["name"] == "ranks")
        base_dgroup = ranks["base"] - self.hdr - 0x10AE0
        self.assertEqual(
            base_dgroup, 0x2e,
            "data/string_tables.json puts `ranks` at DGROUP 0x%x, not the "
            "0x2e the `hp` arm's `add di,0x2e` names" % base_dgroup)
        self.assertEqual(ranks["stride"], 256,
                         "the stride is not the 0x100 `shl di,cl` with cl=8 "
                         "produces")
        for c, want in (("1000:dc26", "mov di,[0x3952]"),
                        ("1000:dc2a", "mov cl,0x8"),
                        ("1000:dc2c", "shl di,cl"),
                        ("1000:dc2e", "add di,0x2e")):
            self.assertEqual(self.at(c).text, want)

    # ------------------------------------------------------- the input read
    def test_the_case_fold_lowercases_and_does_not_trim(self):
        """`0eed:0216` is why the den's keys are case-insensitive.

        And why they are NOT whitespace-insensitive: the routine's only
        comparisons are the 'A' and 'Z' bounds, so nothing in it can strip a
        space.  Asserted over the whole body, not over a chosen window.
        """
        lo = 0x0EED * 16 + 0x0216
        body = list(dis16.decode_run(self.img, lo, lo + 117))
        texts = [i.text for i in body]
        for want in ("cmp byte [es:di],0x41", "cmp byte [es:di],0x5a",
                     "add ax,0x20"):
            self.assertIn(want, texts,
                          "0eed:0216 does not contain %r, so `it lowercases "
                          "ASCII A..Z` is not established" % want)
        blanks = [t for t in texts if re.search(r",0x20$", t)
                  and t.startswith("cmp")]
        self.assertEqual(
            blanks, [],
            "0eed:0216 compares against 0x20 (space) at %r, so the `it does "
            "not trim` claim is false" % blanks)
        self.assertEqual(body[-1].text, "retf 0x4",
                         "0eed:0216's tail is %r; the one-far-pointer "
                         "argument shape (4 bytes popped) is what the den's "
                         "call site at 1000:db1d relies on" % body[-1].text)

    def test_the_string_building_calls_leave_their_destination_on_the_stack(self):
        """Why a `WriteLn` with no visible string push still has one.

        `0f78:0ae7` (assign) and `0f78:0b66` (append) pop only the SOURCE far
        pointer and `0f78:0c03` pops only the character word, so the
        destination the menu row pushed first is still there when
        `0eed:01c2` (`retf 0xe` = a far pointer plus five format words) runs.
        Without this the two dimmed menu rows read as prints with no argument.
        """
        for seg_off, want in ((0x0F78 * 16 + 0x0AE7, "retf 0x4"),
                              (0x0F78 * 16 + 0x0B66, "retf 0x4"),
                              (0x0F78 * 16 + 0x0C03, "retf 0x2"),
                              (0x0EED * 16 + 0x01C2, "retf 0xe")):
            tail = None
            for i in dis16.decode_run(self.img, seg_off, seg_off + 90):
                if i.raw[0] in (0xCA, 0xCB):
                    tail = i.text
                    break
            self.assertEqual(
                tail, want,
                "the routine at image 0x%x returns with %r, not %r"
                % (seg_off, tail, want))
            # These four are RUNTIME segments, so `CITE` never sees them and
            # the prose scans above cannot reach the claim.  Tie it to the
            # doc by hand rather than leave an assertion that only the frozen
            # binary can move.
            self.assertIn(
                "`%s`" % want, self.md,
                "docs/re/den.md no longer quotes %r, so this check stops "
                "guarding anything the prose says" % want)

    # ------------------------------------------------------------- the prose
    def test_every_prose_address_is_an_instruction_boundary(self):
        exempt = {e["addr"]
                  for e in self.art["known_not_boundaries"]["entries"]}
        cits = sorted(set(CITE.findall(strip_fences(self.md))) - exempt)
        self.assertGreaterEqual(
            len(cits), 60,
            "docs/re/den.md names only %d distinct 1000: addresses; a prose "
            "scan that measures nothing must not pass" % len(cits))
        for c in cits:
            self.at(c)

    def test_every_prose_instruction_says_what_the_binary_says(self):
        checked = 0
        for span in self.spans:
            m = re.match(r"^(1000:[0-9a-f]{4})\s+([a-z].*)$", span)
            if not m:
                continue
            c, text = m.groups()
            checked += 1
            self.assertEqual(
                self.aligned[c].text, text,
                "docs/re/den.md writes `%s %s`, but tools/dis16.py decodes "
                "%r there" % (c, text, self.aligned[c].text))
        self.assertGreaterEqual(
            checked, 20,
            "only %d `addr text` spans in docs/re/den.md" % checked)

    def test_every_instruction_inside_a_fence_says_what_the_binary_says(self):
        checked = 0
        for block in re.findall(r"^```.*?\n(.*?)^```", self.md,
                                re.S | re.M):
            for line in block.splitlines():
                m = re.match(r"^(1000:[0-9a-f]{4})\s+([a-z][^;]*?)\s*(;.*)?$",
                             line)
                if not m:
                    continue
                c, text = m.group(1), m.group(2).strip()
                self.assertIn(c, self.aligned,
                              "%r: not a boundary" % line)
                checked += 1
                self.assertEqual(
                    self.aligned[c].text, text,
                    "docs/re/den.md writes `%s %s` in a fence, but "
                    "tools/dis16.py decodes %r there"
                    % (c, text, self.aligned[c].text))
        self.assertGreaterEqual(
            checked, 25,
            "only %d fenced instruction lines in docs/re/den.md" % checked)

    def test_every_prose_literal_comes_out_of_the_binary(self):
        offs = [int(m.group(1), 16)
                for m in re.finditer(r"CS `0x([0-9a-f]{4})`", self.md)]
        self.assertGreaterEqual(len(offs), 25, "only %d CS offsets" % len(offs))
        for o in offs:
            self.assertTrue(self.img[o],
                            "CS 0x%04x has a zero length byte" % o)
            self.cs_literal(o)
        pairs = re.findall(r"`((?!1000:)[^`]+)`\s*\(CS `0x([0-9a-f]{4})`\)",
                           self.md, re.S)
        self.assertGreaterEqual(len(pairs), 15, "only %d pairs" % len(pairs))
        for text, off in pairs:
            self.assertEqual(
                self.cs_literal(int(off, 16)), text,
                "the prose quotes %r beside CS 0x%s, which holds %r"
                % (text, off, self.cs_literal(int(off, 16))))
        # Every literal the artifact names, PLUS the extracted string tables
        # -- the doc cites `ranks` by name and index for the forced class 8,
        # and those entries come out of the same binary
        # (`tools/extract_tables_indexed.py`, guarded by
        # `tools/test_string_tables.py`).  Nothing else widens the set: a
        # Russian word the doc invents still fails.
        known = {self.cs_literal(n["cs_offset"])
                 for n, _ in self.walk(("cs_offset", "text"))}
        tables = json.loads(TABLES.read_text(encoding="utf-8"))
        known |= {e["text"] for t in tables["tables"] for e in t["entries"]}
        # ...and every literal the DOC itself cites by CS offset, which is
        # what the failure message below has always promised ("at any address
        # the doc or the artifact names").  Each of those offsets was already
        # decoded and, where the doc quotes it, checked against the binary by
        # the two loops above, so this widens the set only to literals that
        # are themselves verified.  A Russian word the doc invents, or one
        # lifted from a literal it never cites, still fails.
        known |= {self.cs_literal(o) for o in offs}
        unmatched = sorted({run for span in self.spans
                            for run in re.findall(r"[Ѐ-ӿ]+", span)
                            if not any(run in k for k in known)})
        self.assertEqual(
            unmatched, [],
            "Russian in docs/re/den.md that matches no literal in orig/g.exe "
            "at any address the doc or the artifact names: %r" % unmatched)

    def test_the_prose_and_the_artifact_agree_on_every_arm(self):
        for a in self.arms():
            for c in (a["compare_addr"], a["span"]["start"]):
                self.assertIn(
                    c, self.md,
                    "docs/re/den.md never names %s, which data/den_arms.json "
                    "records for arm %s" % (c, a["key"]))
            for s in a["strings"]:
                self.assertIn(
                    s["cs_offset"], self.md,
                    "arm %s: the prose does not carry CS %s"
                    % (a["key"], s["cs_offset"]))
            for e in a["effects"]:
                self.assertIn(
                    e["addr"], self.md,
                    "arm %s: the prose does not carry the effect at %s"
                    % (a["key"], e["addr"]))
        for b in self.art["threshold_blocks"]:
            self.assertIn(b["start"], self.md)


if __name__ == "__main__":
    unittest.main(verbosity=2)
