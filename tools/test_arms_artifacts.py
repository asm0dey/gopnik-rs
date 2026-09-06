#!/usr/bin/env python3
"""One verifier over every `data/*_arms.json` that maps a whole handler.

`tools/test_club_arms.py` and `tools/test_gym_arms.py` grew to 1380 and 972
lines, of which **18 test methods were identically named** and about half of
each file was the same shape over a different JSON file.  A third handler
(the vet, Task 34) would have made it three.  So the shape moved here, once,
parametrised over the corpus below, and the two bespoke suites keep only what
is genuinely handler-specific.  `data/vet_arms.json` has no bespoke suite at
all: everything in it is checked here.

**Nothing here reads `src/`, a screen, or Ghidra's decompiled C.**  Every
assertion is a re-derivation from `orig/g.exe`, and the same two signals are
kept apart for the same reason as before (`docs/re/METHODOLOGY.md`, "Is this
address a call site?"):

  * **alignment** -- the address is reached by decoding forward from its
    enclosing function's entry, so it is a real instruction boundary and not a
    byte-scan hit in the middle of one;
  * **identity** -- the instruction decoded there says what the artifact says
    it says.

The claims that are NOT restatements of a single decode are asserted by SET
EQUALITY against a sweep of the binary, in both directions:

  * **`strings[]` is complete** in every range, so "the `w` arm prints
    nothing", "there is no unknown-key literal" and "the `i` list is seventeen
    lines" are measurements rather than notes.
  * **the gate inventory is complete** -- every CONDITIONAL branch in every
    range must be named somewhere in its artifact.  The string sweep cannot see
    a silent gate, and "at district 1 the key is not even compared" is a
    headline claim in two of the three maps.
  * **the effect inventory is complete.**  Every instruction carrying an
    absolute-memory operand falls in a WRITE or a READ bucket -- an
    unclassified shape fails loudly rather than passing as a read -- and every
    write must be recorded by the artifact *as an effect*, not merely
    mentioned somewhere in it (see `EFFECT_PATH`).
  * **the sweep counts are the decode**, all eleven of them per range, so a
    negative like "zero absolute writes" rests on a walk known to have covered
    the range.
  * **the spans tile** each range end to end, so a block cannot be dropped out
    of a map by being left out of every span -- which is how an effects or
    strings sweep could be narrowed without any of them going red.

## What is deliberately NOT here

Handler-specific findings stay in the handler's own suite: the club's forced
buffer write and its `i`-list gate chain, the gym's parity branch and its
trained-armour scratch, and the two artifacts' own residue of the
menu-versus-arm comparison.  The rule applied was *"if it would read as a
different claim about a different handler, it does not belong in a loop"*.

    python3 tools/test_arms_artifacts.py
"""
import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import addr as addrmod            # noqa: E402
import re_query                   # noqa: E402
from re_derive import (CITE, aligned_boundaries, load_image,  # noqa: E402
                       inline_spans, strip_fences)

REPO = Path(__file__).resolve().parents[1]
BRANCHES = REPO / "data" / "branches.json"
DISPATCH = REPO / "data" / "command_dispatch.json"
TABLES = REPO / "data" / "string_tables.json"

RANDOM_CALL = b"\x9a\x4b\x11\x78\x0f"
STR_COMPARE = b"\x9a\xd8\x0b\x78\x0f"

#: Copied verbatim from the two suites this file replaces, including the
#: reasons: the MNEMONIC decides, never operand order; `push [N]` is a read;
#: `xchg` is in NEITHER bucket so an unclassified shape fails the sweep
#: instead of passing as a read.
WRITES_ABS_MEM = re.compile(
    r"^(mov|add|sub|adc|sbb|and|or|xor|inc|dec|neg|not"
    r"|shl|shr|sar|rol|ror|rcl|rcr)\s+"
    r"(byte |word |dword )?\[0x[0-9a-f]+\]")
READS_ABS_MEM = re.compile(
    r"^(cmp|test|push)\s+(byte |word |dword )?\[0x[0-9a-f]+\]"
    r"|^(?!xchg\b)[a-z]{2,5}\s+[a-z]{2,3},"
    r"(byte |word |dword )?\[0x[0-9a-f]+\]")

#: An instruction claim written INSIDE a prose string, which carries neither a
#: separate `addr` key nor a separate `text` key and so escapes every other
#: check here.  `tools/test_den_arms.py` found three such claims wrong in
#: review round 1; the pattern is kept identical.
PROSE_INSN = re.compile(r"`(1000:[0-9a-f]{4})\s+([a-z][^`]*)`")

#: A BARE instruction claim -- a mnemonic and a hex operand with no address in
#: front of it, as `data/club_arms.json`'s exemptions write their covering
#: `jbe`. `PROSE_INSN` cannot see this form, which is why it has its own
#: pattern and its own positive control.
BARE_INSN = re.compile(r"^(?:j[a-z]{1,3}|jmp|call|loop[a-z]*)\s+0x[0-9a-f]+$")

#: The JSON paths under which a recorded absolute WRITE counts as an EFFECT.
#:
#: The suites this replaces each hard-coded the handful of paths their own
#: artifact used (`arms[].effects[]`, `menu_lines[].colour_digit.*_store`,
#: `stake_init.effect`, `joint.effects[]`, `abs_recompute.steps[].effect`,
#: `abs_recompute.seed[1]`) and required the write sweep to equal exactly that
#: union.  Matching on the PATH keeps that property -- a write recorded only
#: as a passing `addr`/`text` mention still fails -- without naming one
#: artifact's tree here.  `test_the_effect_path_markers_are_all_exercised`
#: is what stops a marker being added and never matching anything.
EFFECT_PATH = re.compile(r"\.effects?(\[|\.|$)|colour_digit|abs_recompute")


def cit(off):
    return "1000:%04x" % off


def off_of(c):
    return int(c.split(":")[1], 16)


class Corpus:
    """One `(artifact, prose)` pair and the thresholds its size supports.

    The `min_*` numbers exist for the same reason the instruction counts do:
    a scan that stopped early must not pass as a search that found nothing.
    They are floors under the CURRENT content, not targets -- each is well
    below what the artifact holds today, so ordinary editing does not trip
    them and emptying the file does.
    """

    def __init__(self, art, doc, min_addresses, min_insn_records,
                 min_literals, min_prose_addresses, min_prose_insn,
                 min_fence_insn, min_prose_cs, min_prose_pairs,
                 min_prose_cyrillic_pairs,
                 min_port_items, port_items_carry_consequences):
        self.art_path = REPO / art
        self.doc_path = REPO / doc
        self.art_rel, self.doc_rel = art, doc
        self.min_addresses = min_addresses
        self.min_insn_records = min_insn_records
        self.min_literals = min_literals
        self.min_prose_addresses = min_prose_addresses
        self.min_prose_insn = min_prose_insn
        self.min_fence_insn = min_fence_insn
        self.min_prose_cs = min_prose_cs
        self.min_prose_pairs = min_prose_pairs
        #: How many of those pairs must actually contain Cyrillic. Without
        #: this floor the pair regex can start matching something that is not
        #: a game literal at all -- an English phrase in backticks beside a
        #: `(CS 0x....)` -- and every one of those matches would pass the
        #: equality below vacuously. `tools/test_club_arms.py` carried it at
        #: `858f5fb` and the first merge into this file dropped it; it is the
        #: fourth assertion the merge lost.
        self.min_prose_cyrillic_pairs = min_prose_cyrillic_pairs
        self.min_port_items = min_port_items
        #: Whether this artifact's `what_the_port_must_change` items follow
        #: the `falsifiable_as` / `do_not_fix` convention. Task 33 introduced
        #: it with `data/club_arms.json`; `data/gym_arms.json` predates it and
        #: its nine items carry `what` and `blocked` only. Retrofitting them
        #: would be this task authoring consequences for a handler it did not
        #: map, so the flag records the fact instead -- and
        #: `test_the_port_change_lists_are_addressed_and_falsifiable` requires
        #: at least two artifacts to have it set, so it cannot be turned off
        #: everywhere to make the lint vacuous.
        self.port_items_carry_consequences = port_items_carry_consequences

    def __repr__(self):
        return self.art_rel


#: Every artifact this file covers. `test_the_corpus_is_every_arms_artifact
#: _declaring_handlers` is what forbids a fourth one from escaping it.
CORPUS = [
    Corpus("data/club_arms.json", "docs/re/club.md",
           min_addresses=180, min_insn_records=150, min_literals=45,
           min_prose_addresses=80, min_prose_insn=10, min_fence_insn=30,
           min_prose_cs=30, min_prose_pairs=25, min_prose_cyrillic_pairs=20,
           min_port_items=17,
           port_items_carry_consequences=True),
    Corpus("data/gym_arms.json", "docs/re/gym.md",
           min_addresses=180, min_insn_records=150, min_literals=40,
           min_prose_addresses=80, min_prose_insn=15, min_fence_insn=25,
           min_prose_cs=25, min_prose_pairs=15, min_prose_cyrillic_pairs=12,
           min_port_items=9,
           port_items_carry_consequences=False),
    Corpus("data/vet_arms.json", "docs/re/vet.md",
           min_addresses=90, min_insn_records=70, min_literals=20,
           min_prose_addresses=40, min_prose_insn=2, min_fence_insn=20,
           min_prose_cs=20, min_prose_pairs=12, min_prose_cyrillic_pairs=10,
           min_port_items=6,
           port_items_carry_consequences=True),
]


class ArmsArtifactTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        cls.prog = re_query.Program()
        cls.arts, cls.mds, cls.spans = {}, {}, {}
        for c in CORPUS:
            cls.arts[c.art_rel] = json.loads(
                c.art_path.read_text(encoding="utf-8"))
            cls.mds[c.art_rel] = c.doc_path.read_text(encoding="utf-8")
            cls.spans[c.art_rel] = inline_spans(
                strip_fences(cls.mds[c.art_rel]))

    # ---------------------------------------------------------------- helpers
    def at(self, c):
        if c not in self.aligned:
            self.fail("%s is not an instruction boundary reached by decoding "
                      "forward from any enclosing function's entry -- the "
                      "citation is a byte-scan hit, not an address" % c)
        return self.aligned[c]

    def run_of(self, lo, hi):
        return sorted([i for c, i in self.aligned.items()
                       if c.startswith("1000:") and lo <= i.off < hi],
                      key=lambda i: i.off)

    def cs_literal(self, off):
        if isinstance(off, str):
            off = int(off, 16)
        n = self.img[off]
        return self.img[off + 1:off + 1 + n].decode("cp866")

    def sl(self, lo, hi):
        return " ".join("%02x" % b for b in self.img[lo:hi])

    def rel_target(self, ins):
        """The 16-bit-wrapped target of a `jcc`/`jmp` printed as an offset."""
        return int(ins.text.split()[-1], 16) & 0xFFFF

    @staticmethod
    def walk(art, want):
        """Every dict in `art` carrying all of `want`, with its path."""
        def rec(node, path):
            if isinstance(node, dict):
                if all(isinstance(node.get(k), str) for k in want):
                    yield node, path
                for k, v in node.items():
                    yield from rec(v, "%s.%s" % (path, k))
            elif isinstance(node, list):
                for i, v in enumerate(node):
                    yield from rec(v, "%s[%d]" % (path, i))
        return list(rec(art, "$"))

    @staticmethod
    def all_addresses(art):
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
        rec(art)
        return out

    @staticmethod
    def find_all(art, pred):
        """Every (node, path) in `art` for which `pred(node)` is true."""
        def rec(node, path):
            if isinstance(node, dict):
                if pred(node):
                    yield node, path
                for k, v in node.items():
                    yield from rec(v, "%s.%s" % (path, k))
            elif isinstance(node, list):
                for i, v in enumerate(node):
                    yield from rec(v, "%s[%d]" % (path, i))
        return list(rec(art, "$"))

    @classmethod
    def blocks(cls, art):
        """The handler blocks of an artifact, as (block, path).

        A handler block is any dict carrying a `range` with `start` and `end`.
        That is the one shape the three artifacts share: `data/gym_arms.json`
        is flat (one block, the whole file) and `data/club_arms.json` is
        nested (`club` and `command_list`), so nothing here may assume either.
        """
        def is_block(n):
            r = n.get("range")
            return (isinstance(r, dict) and isinstance(r.get("start"), str)
                    and isinstance(r.get("end"), str)
                    and r["start"].startswith("1000:"))
        return cls.find_all(art, is_block)

    @staticmethod
    def resolve_path(node, path):
        """`a.b[2].c` against a JSON subtree; `None` if it does not resolve."""
        for part in re.findall(r"[^.\[\]]+", path):
            if isinstance(node, list):
                if not part.isdigit() or int(part) >= len(node):
                    return None
                node = node[int(part)]
            elif isinstance(node, dict):
                if part not in node:
                    return None
                node = node[part]
            else:
                return None
        return node

    def each(self):
        for c in CORPUS:
            yield c, self.arts[c.art_rel]

    def each_block(self):
        for c in CORPUS:
            for blk, path in self.blocks(self.arts[c.art_rel]):
                lo, hi = off_of(blk["range"]["start"]), off_of(blk["range"]["end"])
                yield c, blk, path, lo, hi

    # ------------------------------------------------------------- population
    def test_the_corpus_is_every_arms_artifact_declaring_handlers(self):
        """A fourth artifact must not be able to escape this file.

        `handlers` is the marker: `data/club_arms.json`, `data/gym_arms.json`
        and `data/vet_arms.json` carry it, `data/den_arms.json` and
        `data/shop_arms.json` -- whose schemas predate this shape and whose
        own suites still own them in full -- do not. So the test is a set
        equality over the glob, not a reading of `CORPUS`.
        """
        declaring = sorted(
            "data/" + p.name
            for p in sorted((REPO / "data").glob("*_arms.json"))
            if isinstance(json.loads(p.read_text(encoding="utf-8"))
                          .get("handlers"), list))
        self.assertEqual(
            declaring, sorted(c.art_rel for c in CORPUS),
            "these data/*_arms.json declare `handlers` and are not in CORPUS "
            "(or vice versa); a handler artifact outside this file is "
            "unverified by it")
        for c, art in self.each():
            self.assertTrue(art["handlers"],
                            "%s declares an empty handler list" % c.art_rel)
            self.assertTrue(
                c.doc_path.is_file(),
                "%s has no prose twin at %s" % (c.art_rel, c.doc_rel))

    def test_each_range_is_anchored_to_the_verb_compares_not_to_itself(self):
        """`each_block()` derives `lo`/`hi` from the block's own `range`.

        So every "every X in range" sweep in this file moves with the
        declaration, and a NARROWED range with its four counts updated to
        match would pass all of them. The two bespoke suites carried a
        `test_the_range_boundaries_are_verb_compares` that closed this;
        `data/vet_arms.json` has no bespoke suite, and the first revision of
        this file never mentioned `data/command_dispatch.json` at all, so the
        vet's range was anchored to nothing.

        Both endpoints are pinned three ways: they decode to the five-byte
        `0f78:0bd8` shortstring compare, the token beside each is the verb's
        own literal out of `orig/g.exe`, and `data/command_dispatch.json` --
        an artifact this task did not touch and which no `*_arms.json`
        derives from -- records the same two addresses for the same two
        verbs.
        """
        chain = json.loads(DISPATCH.read_text(encoding="utf-8"))[
            "confirmed_dispatch_chain"]
        by_verb = {e["verb"]: e for e in chain}
        checked = 0
        for c, blk, path, lo, hi in self.each_block():
            for node, key, want, side in (
                    (blk["verb"], "key", lo, "start"),
                    (blk["bounded_on_the_right_by"], "verb", hi, "end")):
                verb = node[key]
                with self.subTest(artifact=c.art_rel, block=path, verb=verb):
                    addr = node["compare_addr"]
                    self.assertEqual(
                        off_of(addr), want,
                        "%s %s: range.%s is %s but the %r verb compare this "
                        "block is bounded by is %s -- the range is not a free "
                        "choice" % (c.art_rel, path, side, cit(want), verb,
                                    addr))
                    ins = self.at(addr)
                    self.assertEqual(
                        ins.raw[:5], STR_COMPARE,
                        "%s is recorded as the %r verb compare but decodes %r"
                        % (addr, verb, ins.text))
                    self.assertEqual(
                        node["key_literal"]["text"], verb,
                        "%s: the literal beside the compare is %r, not %r"
                        % (addr, node["key_literal"]["text"], verb))
                    self.assertEqual(
                        self.at(node["key_literal"]["push"]["addr"]).text,
                        "mov di,%s" % node["key_literal"]["cs_offset"])
                    self.assertIn(
                        verb, by_verb,
                        "data/command_dispatch.json has no row for %r" % verb)
                    self.assertEqual(
                        by_verb[verb]["compare_addr"], addr,
                        "%s: data/command_dispatch.json records the %r "
                        "compare at %s, this artifact says %s -- the "
                        "independent authority disagrees"
                        % (path, verb, by_verb[verb]["compare_addr"], addr))
                    checked += 1
        self.assertGreaterEqual(
            checked, 8,
            "only %d range endpoints anchored; the corpus has four blocks and "
            "two endpoints each" % checked)

    # ------------------------------------------------------------- decode set
    def test_each_range_decodes_as_one_aligned_run(self):
        """The instruction counts are the anchor every negative rests on.

        An empty hit list means nothing unless the walk is known to have
        covered the range: a walk that stopped early must not pass as a search
        that found nothing.
        """
        for c, blk, path, lo, hi in self.each_block():
            with self.subTest(artifact=c.art_rel, block=path):
                run = self.run_of(lo, hi)
                rec = blk["range"]
                self.assertEqual(
                    len(run), rec["instruction_count"],
                    "%s %s: the aligned decode of %s..%s yields %d "
                    "instructions, the artifact records %d -- one of the two "
                    "is wrong and every sweep below rests on this number"
                    % (c.art_rel, path, cit(lo), cit(hi), len(run),
                       rec["instruction_count"]))
                self.assertEqual(run[0].off, lo)
                self.assertEqual(
                    run[-1].off + run[-1].length, hi,
                    "%s %s: the run does not end exactly on %s, so the range "
                    "is not a whole number of instructions"
                    % (c.art_rel, path, cit(hi)))

    def test_every_address_the_artifact_names_is_a_boundary(self):
        for c, art in self.each():
            with self.subTest(artifact=c.art_rel):
                exempt = {e["addr"] for n, _ in self.find_all(
                    art, lambda n: isinstance(n.get("entries"), list)
                    and all(isinstance(x, dict) and "byte_index" in x
                            for x in n["entries"]))
                    for e in n["entries"]}
                cits = sorted(self.all_addresses(art) - exempt)
                self.assertGreaterEqual(
                    len(cits), c.min_addresses,
                    "%s names only %d distinct 1000: addresses; a scan that "
                    "measures nothing must not pass" % (c.art_rel, len(cits)))
                for a in cits:
                    self.at(a)

    def test_each_exemption_names_the_instruction_that_covers_it(self):
        """The byte role in an exemption's prose is re-derived, not authored.

        Task 32's fix round found `1000:e594` described as "the last byte of
        the row-5 colour `jl`" when it is the OPCODE byte -- the conclusion
        beside it was right and the sentence was not, and no check reached it
        because `why` is free prose with no `addr`/`text` pair inside it. So
        each entry carries `inside` and `byte_index` and both are decoded:
        the covering instruction must be a real boundary, it must actually
        cover the exempt address, and the index must be the offset within it.
        """
        checked, bare_seen = 0, [0]
        for c, art in self.each():
            for e in self.exemptions(art):
                with self.subTest(artifact=c.art_rel, addr=e["addr"]):
                    host = self.at(e["inside"])
                    lo, hi = host.off, host.off + host.length
                    self.assertTrue(
                        lo < off_of(e["addr"]) < hi,
                        "%s: the artifact says it falls inside %s (%s, %d "
                        "bytes), but that instruction spans %s..%s"
                        % (e["addr"], e["inside"], host.text, host.length,
                           cit(lo), cit(hi)))
                    self.assertEqual(
                        off_of(e["addr"]) - lo, e["byte_index"],
                        "%s: the artifact records byte_index %d inside %s; it "
                        "is at byte %d" % (e["addr"], e["byte_index"],
                                           e["inside"],
                                           off_of(e["addr"]) - lo))
                    self.assertIn(
                        cit(hi), self.aligned,
                        "%s: %s is not a boundary, so %s cannot be interior "
                        "to a single instruction"
                        % (e["addr"], cit(hi), e["addr"]))
                    # `why` is free prose, so the only part of it any other
                    # check reaches is an `addr text` pair -- which the gym's
                    # two entries write and the club's two do not. The club's
                    # say "the displacement byte of `jbe 0xe020`": a BARE
                    # instruction, no address beside it, invisible to
                    # `PROSE_INSN`. Task 32's fix round found exactly this
                    # surface carrying a byte role the binary contradicts, so
                    # the bare form is re-derived here against the covering
                    # instruction rather than left to a reader.
                    bare = [t for t in re.findall(r"`([^`]+)`", e.get("why", ""))
                            if BARE_INSN.match(t)]
                    for t in bare:
                        self.assertEqual(
                            t, host.text,
                            "%s: its `why` calls the covering instruction %r, "
                            "but orig/g.exe decodes %r at %s"
                            % (e["addr"], t, host.text, e["inside"]))
                    bare_seen[0] += len(bare)
                    checked += 1
        self.assertGreaterEqual(
            checked, 3,
            "only %d exemptions across the corpus; the exemption walk has "
            "stopped finding them" % checked)
        # And the bare-instruction pattern still matches the shape it hunts,
        # so "no bare claims found" can never be mistaken for "no defects".
        self.assertGreaterEqual(
            bare_seen[0], 2,
            "the bare-instruction sweep over `why` matched %d claims across "
            "the corpus; the club's two entries carry one each, so a zero "
            "means the pattern stopped matching" % bare_seen[0])
        self.assertTrue(BARE_INSN.match("jbe 0xe020"))
        self.assertIsNone(BARE_INSN.match("1000:e594 jl 0xe59d"),
                          "the bare pattern must not swallow the `addr text` "
                          "form `PROSE_INSN` already checks")

    def test_the_boundary_exemption_list_is_honest(self):
        """An exemption that names a real boundary would hide a wrong address.

        So each entry must actually be OUTSIDE the aligned set -- the
        exemption list cannot be used to smuggle one in.
        """
        seen = 0
        for c, art in self.each():
            for e in self.exemptions(art):
                with self.subTest(artifact=c.art_rel, addr=e["addr"]):
                    self.assertNotIn(
                        e["addr"], self.aligned,
                        "%s is exempted from the boundary walk but IS a "
                        "boundary; the exemption is either stale or covering "
                        "for a wrong address" % e["addr"])
                seen += 1
        # Both bespoke suites carried `len(entries) >= 1` per artifact, which
        # the vet cannot satisfy -- it exempts nothing, and that zero is a
        # measurement. The population control moves to the corpus so an empty
        # walk still cannot pass as an honest list.
        self.assertGreaterEqual(
            seen, 4, "only %d exemptions walked; the corpus carries four "
                     "(two club, two gym)" % seen)

    @staticmethod
    def exemptions(art):
        out = []
        for node, _ in ArmsArtifactTest.find_all(
                art, lambda n: isinstance(n.get("entries"), list)
                and all(isinstance(x, dict) and "byte_index" in x
                        for x in n["entries"])):
            out.extend(node["entries"])
        return out

    def test_every_cited_instruction_decodes_to_what_the_artifact_says(self):
        for c, art in self.each():
            with self.subTest(artifact=c.art_rel):
                seen = self.walk(art, ("addr", "text"))
                self.assertGreater(
                    len(seen), c.min_insn_records,
                    "%s stopped carrying instruction records; a walk that "
                    "finds nothing must not pass (found %d)"
                    % (c.art_rel, len(seen)))
                for node, path in seen:
                    ins = self.at(node["addr"])
                    self.assertEqual(
                        ins.text, node["text"],
                        "%s: %s says %s at %s, orig/g.exe decodes %s there"
                        % (path, c.art_rel, node["text"], node["addr"],
                           ins.text))

    def test_every_prose_embedded_instruction_says_what_the_binary_says(self):
        # The regex still matches the shape it hunts, so "no matches" can
        # never be mistaken for "no defects".
        self.assertEqual(
            PROSE_INSN.findall("x `1000:e68f inc [0x38a8]` y"),
            [("1000:e68f", "inc [0x38a8]")],
            "the prose-embedded pattern no longer matches the claim shape it "
            "hunts")
        total = 0
        for c, art in self.each():
            with self.subTest(artifact=c.art_rel):
                found = []

                def rec(node, path):
                    if isinstance(node, dict):
                        for k, v in node.items():
                            rec(v, "%s.%s" % (path, k))
                    elif isinstance(node, list):
                        for i, v in enumerate(node):
                            rec(v, "%s[%d]" % (path, i))
                    elif isinstance(node, str):
                        for m in PROSE_INSN.finditer(node):
                            found.append((path, m.group(1), m.group(2)))
                rec(art, "$")
                for path, a, text in found:
                    ins = self.at(a)
                    self.assertEqual(
                        ins.text, text,
                        "%s: %s writes `%s %s` inside a prose string, but "
                        "orig/g.exe decodes %r there"
                        % (path, c.art_rel, a, text, ins.text))
                total += len(found)
        self.assertGreaterEqual(
            total, 8,
            "the prose-embedded instruction sweep matched only %d spans "
            "across the corpus; a scan that measures nothing must not pass"
            % total)

    # ---------------------------------------------------------------- literals
    def test_every_literal_decodes_to_the_recorded_text(self):
        for c, art in self.each():
            with self.subTest(artifact=c.art_rel):
                seen = self.walk(art, ("cs_offset", "file_offset", "text"))
                self.assertGreaterEqual(
                    len(seen), c.min_literals,
                    "%s: only %d literal records; the artifact lost its "
                    "strings" % (c.art_rel, len(seen)))
                for node, path in seen:
                    cs = int(node["cs_offset"], 16)
                    self.assertEqual(
                        int(node["file_offset"], 16),
                        cs + addrmod.HEADER_BYTES,
                        "%s: file_offset is not cs_offset + the MZ header "
                        "size" % path)
                    self.assertEqual(
                        self.cs_literal(cs), node["text"],
                        "%s: the Pascal shortstring at CS %s is %r, the "
                        "artifact records %r"
                        % (path, node["cs_offset"], self.cs_literal(cs),
                           node["text"]))
                    push = node["push"]
                    self.assertEqual(
                        self.at(push["addr"]).text,
                        "mov di,%s" % node["cs_offset"],
                        "%s: the push at %s does not load %s"
                        % (path, push["addr"], node["cs_offset"]))

    def test_the_recorded_strings_are_every_cs_literal_pushed(self):
        """SET EQUALITY, both directions, over every range in the corpus.

        Swept: every `mov di,imm16` followed by `push cs` / `push di`.
        Recorded: every literal record whose push falls in that range.  "The
        `w` arm prints nothing" and "the `i` list is exactly seventeen lines"
        are this measurement, not notes.
        """
        for c, blk, path, lo, hi in self.each_block():
            with self.subTest(artifact=c.art_rel, block=path):
                run = self.run_of(lo, hi)
                swept = set()
                for k, i in enumerate(run):
                    if (i.text.startswith("mov di,0x") and k + 2 < len(run)
                            and run[k + 1].text == "push cs"
                            and run[k + 2].text == "push di"):
                        swept.add(cit(i.off))
                recorded = {n["push"]["addr"]
                            for n, _ in self.walk(self.arts[c.art_rel],
                                                  ("cs_offset", "text"))
                            if lo <= off_of(n["push"]["addr"]) < hi}
                self.assertEqual(
                    swept, recorded,
                    "%s %s: the CS-literal push sweep and the artifact "
                    "disagree: only swept %s, only recorded %s"
                    % (c.art_rel, path, sorted(swept - recorded),
                       sorted(recorded - swept)))
                self.assertEqual(
                    len(swept), blk["sweeps"]["cs_literal_pushes"],
                    "%s %s: sweeps.cs_literal_pushes says %d, the sweep finds "
                    "%d" % (c.art_rel, path,
                            blk["sweeps"]["cs_literal_pushes"], len(swept)))

    # ------------------------------------------------------------------ gates
    def test_the_recorded_gates_are_every_conditional_branch_in_range(self):
        """No silent gate.

        The string sweep cannot see a branch that prints nothing, and "an
        unrecognised key is silent" and "at district 1 the key is not even
        compared" both depend on the branch inventory being whole.
        """
        for c, blk, path, lo, hi in self.each_block():
            with self.subTest(artifact=c.art_rel, block=path):
                swept = {cit(i.off) for i in self.run_of(lo, hi)
                         if re.match(r"^j(?!mp)", i.text)}
                named = self.all_addresses(self.arts[c.art_rel])
                missing = sorted(swept - named)
                self.assertEqual(
                    missing, [],
                    "%s %s: conditional branches in %s..%s that the artifact "
                    "never names: %s"
                    % (c.art_rel, path, cit(lo), cit(hi), missing))
                self.assertEqual(
                    len(swept), blk["sweeps"]["conditional_branches"],
                    "%s %s: sweeps.conditional_branches says %d, the sweep "
                    "finds %d" % (c.art_rel, path,
                                  blk["sweeps"]["conditional_branches"],
                                  len(swept)))

    def test_the_branch_census_reproduces_data_branches_json(self):
        censuses = 0
        for c, art in self.each():
            for node, path in self.find_all(
                    art, lambda n: isinstance(n.get("range"), list)
                    and "branches" in n and "port_touched_false" in n):
                with self.subTest(artifact=c.art_rel, block=path):
                    lo, hi = (off_of(x) for x in node["range"])
                    rng = [b for b in self.branches["branches"]
                           if lo <= off_of(b["addr"]) <= hi]
                    self.assertEqual(
                        len(rng), node["branches"],
                        "%s %s: data/branches.json holds %d branches in "
                        "%s..%s, the artifact records %d"
                        % (c.art_rel, path, len(rng), cit(lo), cit(hi),
                           node["branches"]))
                    untouched = sum(1 for b in rng if not b["port_touched"])
                    self.assertEqual(
                        untouched, node["port_touched_false"],
                        "%s %s: %d branches have port_touched false, the "
                        "artifact records %d"
                        % (c.art_rel, path, untouched,
                           node["port_touched_false"]))
                    censuses += 1
        self.assertGreaterEqual(
            censuses, 5,
            "only %d branch censuses across the corpus; the walk has stopped "
            "finding them" % censuses)

    # ------------------------------------------------------------------ draws
    def test_the_random_sweep_is_the_recorded_count_and_can_find_a_draw(self):
        """A zero is not evidence on its own.

        So the same five-byte `call 0f78:114b` signature that counts each
        range's draws is also run over the WHOLE image and required to find
        the population `docs/re/METHODOLOGY.md` records.
        """
        image_wide = self.img.count(RANDOM_CALL)
        self.assertEqual(
            image_wide, 86,
            "the `call 0f78:114b` signature finds %d sites image-wide; "
            "docs/re/METHODOLOGY.md records 86, so the sweep that reports a "
            "zero in range cannot be trusted" % image_wide)
        for c, blk, path, lo, hi in self.each_block():
            with self.subTest(artifact=c.art_rel, block=path):
                swept = [cit(i.off) for i in self.run_of(lo, hi)
                         if i.text == "call 0xf78:0x114b"]
                self.assertEqual(
                    len(swept), blk["sweeps"]["random_call_sites"],
                    "%s %s: sweeps.random_call_sites says %d, the sweep finds "
                    "%s" % (c.art_rel, path,
                            blk["sweeps"]["random_call_sites"], swept))

    def test_every_recorded_draw_site_re_derives_its_n(self):
        """`n_expr` is walked back out of the binary, not read off the file.

        A draw's `n` is the one number a map can get wrong without any sweep
        noticing: `sweeps.random_call_sites` counts the CALLS, not what they
        push. `tools/re_query.py`'s `pushed-n` reproduces all 17 hand-written
        `data/wander.json` sites byte for byte, so it is the authority here.
        """
        checked = 0
        for c, art in self.each():
            for node, path in self.walk(art, ("addr", "n_expr")):
                with self.subTest(artifact=c.art_rel, at=node["addr"]):
                    ins = self.at(node["addr"])
                    self.assertEqual(
                        ins.text, "call 0xf78:0x114b",
                        "%s: an `n_expr` record must sit on a `Random` call "
                        "site; %s decodes %r"
                        % (path, node["addr"], ins.text))
                    d = re_query.pushed_n(self.prog, node["addr"])
                    got = str(d.get("n_expr") or d.get("n"))
                    self.assertEqual(
                        got, node["n_expr"],
                        "%s: the walk-back derives n = %r at %s, the artifact "
                        "records %r"
                        % (path, got, node["addr"], node["n_expr"]))
                    checked += 1
        self.assertGreaterEqual(
            checked, 2,
            "only %d recorded draw sites across the corpus; the club's "
            "1000:e0b7 and the vet's 1000:d5f6 are both `n_expr` records, so "
            "a smaller number means the walk stopped finding them" % checked)

    # ---------------------------------------------------------------- effects
    def test_the_recorded_effects_are_every_absolute_write_in_range(self):
        for c, blk, path, lo, hi in self.each_block():
            with self.subTest(artifact=c.art_rel, block=path):
                writes, reads, unclassified = set(), set(), []
                for i in self.run_of(lo, hi):
                    if "[0x" not in i.text:
                        continue
                    if WRITES_ABS_MEM.match(i.text):
                        writes.add(cit(i.off))
                    elif READS_ABS_MEM.match(i.text):
                        reads.add(cit(i.off))
                    else:
                        unclassified.append((cit(i.off), i.text))
                self.assertEqual(
                    unclassified, [],
                    "%s %s: instructions with an absolute-memory operand that "
                    "neither bucket describes -- an unclassified shape must "
                    "fail loudly rather than pass as a read: %s"
                    % (c.art_rel, path, unclassified))
                recorded = {
                    n["addr"] for n, p in
                    self.walk(self.arts[c.art_rel], ("addr", "text"))
                    if EFFECT_PATH.search(p)
                    and WRITES_ABS_MEM.match(n["text"])
                    and lo <= off_of(n["addr"]) < hi}
                self.assertEqual(
                    writes, recorded,
                    "%s %s: the absolute-write sweep and the artifact's "
                    "EFFECT records disagree: only swept %s, only recorded %s"
                    % (c.art_rel, path, sorted(writes - recorded),
                       sorted(recorded - writes)))
                self.assertEqual(
                    len(writes), blk["sweeps"]["absolute_memory_writes"],
                    "%s %s: sweeps.absolute_memory_writes says %d, the sweep "
                    "finds %d" % (c.art_rel, path,
                                  blk["sweeps"]["absolute_memory_writes"],
                                  len(writes)))
                # The READ bucket is not recorded anywhere, so it is not
                # compared -- but it must not be empty IN THIS BLOCK, or the
                # classifier that sorts writes from reads was never exercised
                # here and the write set could be everything the block
                # touches. A corpus-wide floor does not say that: the first
                # revision of this restore used one, and under it the club
                # could fall to zero reads while the gym's 56 carried the
                # total. The floor is per block for the same reason every
                # other sweep in this file is.
                self.assertEqual(
                    len(reads), blk["sweeps"]["absolute_memory_reads"],
                    "%s %s: the absolute-READ sweep finds %d instructions, "
                    "the artifact records %d"
                    % (c.art_rel, path, len(reads),
                       blk["sweeps"]["absolute_memory_reads"]))
                self.assertGreater(
                    len(reads), 0,
                    "%s %s: the absolute-READ bucket is empty, so the "
                    "write/read classifier was never exercised over this "
                    "block and the write set could be everything it touches"
                    % (c.art_rel, path))

    def test_the_effect_path_markers_are_all_exercised(self):
        """`EFFECT_PATH` must not grow a marker that matches nothing.

        The path filter is what keeps the effects sweep a completeness check
        rather than a mention check, so a marker added to widen it -- and
        thereby to let a write be "recorded" from anywhere -- has to be paid
        for by really matching a write record somewhere in the corpus.
        """
        markers = [r"\.effects?(\[|\.|$)", "colour_digit", "abs_recompute"]
        self.assertEqual(
            EFFECT_PATH.pattern, "|".join(markers),
            "EFFECT_PATH changed; update the marker list this test walks")
        for m in markers:
            pat = re.compile(m)
            hits = [p for c, art in self.each()
                    for n, p in self.walk(art, ("addr", "text"))
                    if pat.search(p) and WRITES_ABS_MEM.match(n["text"])]
            self.assertTrue(
                hits, "EFFECT_PATH marker %r matches no recorded write "
                      "anywhere in the corpus; it widens the filter for "
                      "nothing" % m)

    # ---------------------------------------------------------------- globals
    def test_the_dgroup_addresses_touched_are_the_recorded_globals(self):
        for c, blk, path, lo, hi in self.each_block():
            with self.subTest(artifact=c.art_rel, block=path):
                swept = {"20ae:" + m for i in self.run_of(lo, hi)
                         if "[0x" in i.text
                         for m in re.findall(r"\[0x([0-9a-f]+)\]", i.text)}
                recorded = {g["ds"] for g in blk["globals"]}
                self.assertEqual(
                    swept, recorded,
                    "%s %s: the DGROUP-operand sweep and globals[] disagree: "
                    "only swept %s, only recorded %s"
                    % (c.art_rel, path, sorted(swept - recorded),
                       sorted(recorded - swept)))
                self.assertEqual(
                    len(swept), blk["sweeps"]["dgroup_addresses_touched"])

    def test_each_globals_write_list_is_its_writes_in_range(self):
        for c, blk, path, lo, hi in self.each_block():
            run = self.run_of(lo, hi)
            for g in blk["globals"]:
                with self.subTest(artifact=c.art_rel, block=path, ds=g["ds"]):
                    dskey = "[0x%s]" % g["ds"].split(":")[1]
                    swept = [cit(i.off) for i in run
                             if WRITES_ABS_MEM.match(i.text)
                             and dskey in i.text]
                    self.assertEqual(
                        swept, g["written_in_range"],
                        "%s/%s: the sweep finds %s written in range, the "
                        "artifact records %s"
                        % (path, g["ds"], swept, g["written_in_range"]))
                    reads = len([i for i in run if dskey in i.text
                                 and not WRITES_ABS_MEM.match(i.text)])
                    self.assertEqual(
                        reads, g["read_sites_in_range"],
                        "%s/%s: %d read sites in range, the artifact records "
                        "%d" % (path, g["ds"], reads,
                                g["read_sites_in_range"]))

    def test_every_globals_xref_census_is_what_re_query_reports(self):
        """`named_from` and "the only writer" are re-derived, not trusted."""
        checked = 0
        for c, blk, path, lo, hi in self.each_block():
            for g in blk["globals"]:
                with self.subTest(artifact=c.art_rel, ds=g["ds"]):
                    scan = re_query.xrefs_to(self.prog, g["ds"])["scan"]
                    xr = g["xrefs"]
                    self.assertEqual(
                        (scan["raw_hits"], len(scan["accepted"]),
                         len(scan["discarded"])),
                        (xr["raw_hits"], xr["accepted"], xr["discarded"]),
                        "%s: `xrefs-to` reports raw=%d accepted=%d "
                        "discarded=%d, the artifact records raw=%d "
                        "accepted=%d discarded=%d"
                        % (g["ds"], scan["raw_hits"], len(scan["accepted"]),
                           len(scan["discarded"]), xr["raw_hits"],
                           xr["accepted"], xr["discarded"]))
                    self.assertEqual(
                        xr["command"],
                        "python3 tools/re_query.py xrefs-to " + g["ds"],
                        "%s: the recorded command does not recompute the "
                        "census beside it" % g["ds"])
                    writers = [a["at"] for a in scan["accepted"]
                               if WRITES_ABS_MEM.match(a["text"])]
                    self.assertEqual(
                        xr["writers_image_wide"], writers,
                        "%s: the artifact lists %s as its image-wide writers, "
                        "`xrefs-to` finds %s -- an 'only writer' claim that "
                        "stopped the next search is exactly what this check "
                        "exists for"
                        % (g["ds"], xr["writers_image_wide"], writers))
                    for e in g["evidence"]:
                        self.assertEqual(
                            self.at(e["addr"]).text, e["text"],
                            "%s: evidence at %s says %r, orig/g.exe decodes "
                            "%r" % (g["ds"], e["addr"], e["text"],
                                    self.at(e["addr"]).text))
                    checked += 1
        self.assertGreaterEqual(
            checked, 25,
            "only %d global censuses across the corpus were checked; the "
            "walk has stopped finding them" % checked)

    # ------------------------------------------------------------------ spans
    def test_the_spans_tile_each_range(self):
        for c, blk, path, lo, hi in self.each_block():
            with self.subTest(artifact=c.art_rel, block=path):
                cursor = lo
                for s in blk["spans"]:
                    self.assertEqual(
                        off_of(s["start"]), cursor,
                        "%s %s: span %r starts at %s, the previous one ended "
                        "at %s -- the tiling has a %s"
                        % (c.art_rel, path, s["name"], s["start"],
                           cit(cursor),
                           "gap" if off_of(s["start"]) > cursor else "overlap"))
                    self.at(s["start"])
                    cursor = off_of(s["end"])
                self.assertEqual(
                    cursor, hi,
                    "%s %s: the spans stop at %s, the range ends at %s"
                    % (c.art_rel, path, cit(cursor), cit(hi)))
                for a in blk.get("arms", []):
                    self.assertIn(
                        (a["span"]["start"], a["span"]["end"]),
                        [(s["start"], s["end"]) for s in blk["spans"]],
                        "%s %s: arm %s's span is not one of the tiling's "
                        "spans" % (c.art_rel, path, a["key"]))

    # ------------------------------------------------- menu against the arms
    def test_the_menu_and_arm_predicates_differ_exactly_as_recorded(self):
        """The shared five-byte predicates, re-sliced out of `orig/g.exe`.

        Two of the three maps carry a `menu_vs_arm_finding` whose whole
        argument is that identical bytes do different things; this is the
        half of it that is the same measurement in both. The artifact's own
        further claims about WHICH `jcc` differs how stay in its own suite.
        """
        found = 0
        for c, art in self.each():
            for f, path in self.find_all(
                    art, lambda n: isinstance(n.get("pairs"), list)
                    and "longest_common_byte_run" in n):
                for p in f["pairs"]:
                    with self.subTest(artifact=c.art_rel, pair=p["what"]):
                        m = self.sl(off_of(p["menu"]["start"]),
                                    off_of(p["menu"]["end"]))
                        a = self.sl(off_of(p["arm"]["start"]),
                                    off_of(p["arm"]["end"]))
                        self.assertEqual(m, p["menu_bytes"],
                                         "%s: menu bytes" % p["what"])
                        self.assertEqual(a, p["arm_bytes"],
                                         "%s: arm bytes" % p["what"])
                        self.assertEqual(
                            m == a, p["identical"],
                            "%s: the artifact records identical=%s, the bytes "
                            "say %s" % (p["what"], p["identical"], m == a))
                        self.assertEqual(
                            self.sl(off_of(p["menu"]["end"]),
                                    off_of(p["menu"]["end"]) + 2),
                            p["jcc_menu"],
                            "%s: the artifact records %s as the menu's `jcc`"
                            % (p["what"], p["jcc_menu"]))
                        self.assertEqual(
                            self.sl(off_of(p["arm"]["end"]),
                                    off_of(p["arm"]["end"]) + 2),
                            p["jcc_arm"],
                            "%s: the artifact records %s as the arm's `jcc`"
                            % (p["what"], p["jcc_arm"]))
                        found += 1
                for s in f.get("shared_constants", []):
                    with self.subTest(artifact=c.art_rel, const=s["what"]):
                        mb = self.sl(off_of(s["menu"]), off_of(s["menu"]) + 5)
                        ab = self.sl(off_of(s["arm"]), off_of(s["arm"]) + 5)
                        self.assertEqual(mb, s["bytes"],
                                         "%s: menu bytes" % s["what"])
                        self.assertEqual(
                            (mb == ab), s["identical"],
                            "%s: the artifact records identical=%s, the bytes "
                            "say %s" % (s["what"], s["identical"], mb == ab))
        self.assertGreaterEqual(
            found, 5,
            "only %d menu/arm predicate pairs across the corpus" % found)

    def test_the_longest_run_the_two_blocks_share_is_the_recorded_one(self):
        """The measurement that answers "is the second block a copy?".

        Recomputed here rather than remembered: a longest-common-substring
        over the two spans. If it grew, the two blocks really do share a body
        and the finding is wrong.

        **Neither span is a free choice** where the artifact says so. Task
        33's review showed the cost of leaving them free: widening
        `arm_span.end` and updating `byte_length` to match passed green, so
        the check proved only that two length fields agreed with each other.
        An artifact may therefore record a `<field>_is` naming the JSON path
        the endpoint is anchored to, and that path is RESOLVED here before any
        length is looked at.
        """
        found = 0
        for c, art in self.each():
            for f, path in self.find_all(
                    art, lambda n: isinstance(n.get("longest_common_byte_run"),
                                              dict)):
                block = self.enclosing_block(art, path)
                for side in ("menu_span", "arm_span"):
                    for end in ("start", "end"):
                        says = f[side].get("%s_is" % end)
                        if says is None:
                            continue
                        with self.subTest(artifact=c.art_rel,
                                          field="%s.%s" % (side, end)):
                            self.assertFalse(
                                says.startswith("menu_vs_arm_finding"),
                                "%s.%s_is points back into the finding it "
                                "anchors; that is not an independent anchor"
                                % (side, end))
                            anchor = self.resolve_path(block, says)
                            self.assertIsNotNone(
                                anchor,
                                "%s.%s_is names %r, which does not resolve in "
                                "the handler block" % (side, end, says))
                            self.assertEqual(
                                f[side][end], anchor,
                                "%s.%s is %s, but %s -- the address it is "
                                "anchored to -- is %s.  The span is not a "
                                "free choice: a span moved to flatter the "
                                "longest-common-substring fails here"
                                % (side, end, f[side][end], says, anchor))
                with self.subTest(artifact=c.art_rel, finding=path):
                    X = self.img[off_of(f["menu_span"]["start"]):
                                 off_of(f["menu_span"]["end"])]
                    Y = self.img[off_of(f["arm_span"]["start"]):
                                 off_of(f["arm_span"]["end"])]
                    self.assertEqual(len(X), f["menu_span"]["byte_length"])
                    self.assertEqual(len(Y), f["arm_span"]["byte_length"])
                    best = (0, 0, 0)
                    prev = [0] * (len(Y) + 1)
                    for i in range(1, len(X) + 1):
                        cur = [0] * (len(Y) + 1)
                        xi = X[i - 1]
                        for j in range(1, len(Y) + 1):
                            if xi == Y[j - 1]:
                                cur[j] = prev[j - 1] + 1
                                if cur[j] > best[0]:
                                    best = (cur[j], i - cur[j], j - cur[j])
                        prev = cur
                    rec = f["longest_common_byte_run"]
                    self.assertEqual(
                        best[0], rec["length"],
                        "the two blocks share a %d-byte run; the artifact "
                        "records %d" % (best[0], rec["length"]))
                    self.assertEqual(
                        cit(off_of(f["menu_span"]["start"]) + best[1]),
                        rec["menu_at"])
                    self.assertEqual(
                        cit(off_of(f["arm_span"]["start"]) + best[2]),
                        rec["arm_at"])
                    self.assertEqual(X[best[1]:best[1] + best[0]].hex(" "),
                                     rec["bytes"])
                    found += 1
        self.assertGreaterEqual(
            found, 2, "only %d longest-common-run findings in the corpus"
                      % found)

    def enclosing_block(self, art, path):
        """The handler block a `$....` path falls inside."""
        best = art
        for blk, bpath in self.blocks(art):
            if path.startswith(bpath if bpath != "$" else "$"):
                if bpath == "$" or path.startswith(bpath + "."):
                    if best is art or len(bpath) > 1:
                        best = blk
        return best

    def test_the_menu_price_test_never_hides_a_row(self):
        """Both colour arms reconverge; only 20ae:3b7a differs between them.

        This is what makes "in the menu the price test is cosmetic" a
        measurement.  The `'0'` store is followed by a `jmp short` whose
        target is the instruction right after the `'4'` store, so the two
        paths join before anything else happens.
        """
        rows = 0
        for c, blk, path, lo, hi in self.each_block():
            run = self.run_of(lo, hi)
            offs = [i.off for i in run]
            for m in blk.get("menu_lines", []):
                if not isinstance(m, dict) or "colour_digit" not in m:
                    continue
                with self.subTest(artifact=c.art_rel, row=m["key"]):
                    stores = sorted(
                        (self.at(m["colour_digit"]["affordable_store"]["addr"]),
                         self.at(m["colour_digit"]["unaffordable_store"]
                                 ["addr"])), key=lambda i: i.off)
                    first, second = stores
                    # WHICH store comes first is not fixed across the corpus:
                    # the club and the gym test with `jl` and store `'0'`
                    # first, the vet tests with `jnl` and stores `'4'` first
                    # (`data/vet_arms.json`'s `colour_digit_order_finding`).
                    # The reconvergence is what this measures, so it is
                    # asserted on the ORDER the decode reports.
                    join = run[offs.index(second.off) + 1]
                    after_first = run[offs.index(first.off) + 1]
                    self.assertTrue(
                        after_first.text.startswith("jmp"),
                        "%s row %s: the first colour store at %s is not "
                        "followed by a jump"
                        % (c.art_rel, m["key"], cit(first.off)))
                    self.assertEqual(
                        self.rel_target(after_first), join.off,
                        "%s row %s: the two colour arms do not reconverge on "
                        "%s" % (c.art_rel, m["key"], cit(join.off)))
                    self.assertIn("[0x3b7a]", first.text)
                    self.assertIn("[0x3b7a]", second.text)
                    # `'0'` is affordable, `'4'` is not -- checked, so the two
                    # labels cannot be swapped in an artifact.
                    self.assertIn(
                        "0x30",
                        self.at(m["colour_digit"]["affordable_store"]
                                ["addr"]).text,
                        "%s row %s: the affordable store does not write '0'"
                        % (c.art_rel, m["key"]))
                    self.assertIn(
                        "0x34",
                        self.at(m["colour_digit"]["unaffordable_store"]
                                ["addr"]).text,
                        "%s row %s: the unaffordable store does not write '4'"
                        % (c.art_rel, m["key"]))
                    rows += 1
        self.assertGreaterEqual(
            rows, 9, "only %d priced menu rows across the corpus; the walk "
                     "has stopped finding them" % rows)

    # ------------------------------------------------------------- the w arm
    def test_the_w_arm_writes_and_prints_nothing(self):
        """The `w` arm's two negatives, measured over its own span."""
        arms = 0
        for c, blk, path, lo, hi in self.each_block():
            arm = next((a for a in blk.get("arms", []) if a["key"] == "w"),
                       None)
            if arm is None:
                continue
            with self.subTest(artifact=c.art_rel, block=path):
                alo, ahi = off_of(arm["span"]["start"]), off_of(
                    arm["span"]["end"])
                body = self.run_of(alo, ahi)
                self.assertGreater(
                    len(body), 4,
                    "the `w` arm's span decoded to %d instructions"
                    % len(body))
                self.assertEqual(
                    [cit(i.off) for i in body
                     if WRITES_ABS_MEM.match(i.text)], [],
                    "the `w` arm is recorded as writing nothing")
                self.assertEqual(
                    [cit(i.off) for i in body
                     if i.text in ("call 0xeed:0x1c2", "call 0xeed:0x0")], [],
                    "the `w` arm is recorded as printing nothing")
                self.assertEqual(arm["effects"], [])
                self.assertEqual(arm["prints"], [])
                arms += 1
        self.assertGreaterEqual(
            arms, 2, "only %d `w` arms in the corpus; the two that exist are "
                     "the club's and the gym's" % arms)

    # --------------------------------------------------- the port work order
    def test_the_port_change_lists_are_addressed_and_falsifiable(self):
        """Every numbered item names an original address and a consequence.

        **This is a lint over the artifact's own prose, not evidence about
        `orig/g.exe`.**  It cannot judge whether a `do_not_fix` string really
        describes a trap; what it does check is that the item is anchored to
        an address and that its consequence is addressed to the PORTING task
        rather than being a note about the RE task's own scope.  Task 33's
        review found an item satisfying the lint through a `do_not_fix` whose
        content ("This RE task does not edit them") inverted the field's name,
        so the second half is narrow by construction: it names one shape that
        has actually occurred here.
        """
        scope_note = re.compile(
            r"\bthis RE task\b|\bTask \d+ (?:does|did) not\b", re.I)
        # The pattern still matches the shape it hunts, so "no hits" can never
        # be mistaken for "no defects".
        self.assertIsNotNone(
            scope_note.search("This RE task does not edit them"),
            "the scope-note pattern no longer matches the string that "
            "prompted it")
        self.assertGreaterEqual(
            sum(1 for c in CORPUS if c.port_items_carry_consequences), 2,
            "the consequence convention is claimed by fewer than two "
            "artifacts; the lint below would be nearly vacuous")
        lists = 0
        for c, art in self.each():
            total = 0
            for node, path in self.find_all(
                    art, lambda n: isinstance(
                        n.get("what_the_port_must_change"), list)):
                items = node["what_the_port_must_change"]
                total += len(items)
                with self.subTest(artifact=c.art_rel, block=path):
                    # Per BLOCK, not only per artifact: the two bespoke floors
                    # this replaced were 10 for `club` and 4 for
                    # `command_list`, and an artifact-total floor alone lets
                    # one block empty out while the other grows.
                    self.assertGreaterEqual(
                        len(items), 4,
                        "%s carries only %d work-order items" % (path, len(items)))
                    self.assertEqual([i["n"] for i in items],
                                     list(range(1, len(items) + 1)))
                    for it in items:
                        self.assertTrue(
                            CITE.findall(it["what"]) or "CS `0x" in it["what"],
                            "%s item %d names no original address: %r"
                            % (path, it["n"], it["what"]))
                        if c.port_items_carry_consequences:
                            self.assertTrue(
                                "falsifiable_as" in it or "do_not_fix" in it,
                                "%s item %d carries neither a falsifiable "
                                "consequence nor a do-not-fix trap"
                                % (path, it["n"]))
                        for field in ("falsifiable_as", "do_not_fix"):
                            if field in it:
                                self.assertIsNone(
                                    scope_note.search(it[field]),
                                    "%s item %d's %s is a note about an RE "
                                    "task's own scope, not a consequence for "
                                    "the porting task: %r"
                                    % (path, it["n"], field, it[field]))
                        self.assertIn("blocked", it)
                lists += 1
            with self.subTest(artifact=c.art_rel):
                self.assertGreaterEqual(
                    total, c.min_port_items,
                    "%s carries %d work-order items across all its blocks; "
                    "the floor is %d" % (c.art_rel, total, c.min_port_items))
        self.assertGreaterEqual(
            lists, 4, "only %d port-change lists across the corpus" % lists)

    # ------------------------------------------------------------- the prose
    def test_every_prose_address_is_an_instruction_boundary(self):
        for c, art in self.each():
            with self.subTest(doc=c.doc_rel):
                exempt = {e["addr"] for e in self.exemptions(art)}
                cits = sorted(set(CITE.findall(
                    strip_fences(self.mds[c.art_rel]))) - exempt)
                self.assertGreaterEqual(
                    len(cits), c.min_prose_addresses,
                    "%s names only %d distinct 1000: addresses; a prose scan "
                    "that measures nothing must not pass"
                    % (c.doc_rel, len(cits)))
                for a in cits:
                    self.at(a)

    def test_every_prose_instruction_says_what_the_binary_says(self):
        for c, art in self.each():
            with self.subTest(doc=c.doc_rel):
                checked = 0
                for span in self.spans[c.art_rel]:
                    m = re.match(r"^(1000:[0-9a-f]{4})\s+([a-z].*)$", span)
                    if not m:
                        continue
                    a, text = m.groups()
                    checked += 1
                    self.assertEqual(
                        self.at(a).text, text,
                        "%s writes `%s %s`, but tools/dis16.py decodes %r "
                        "there" % (c.doc_rel, a, text, self.at(a).text))
                self.assertGreaterEqual(
                    checked, c.min_prose_insn,
                    "only %d `addr text` spans in %s" % (checked, c.doc_rel))

    def test_every_instruction_inside_a_fence_says_what_the_binary_says(self):
        for c, art in self.each():
            with self.subTest(doc=c.doc_rel):
                checked = 0
                for block in re.findall(r"^```.*?\n(.*?)^```",
                                        self.mds[c.art_rel], re.S | re.M):
                    for line in block.splitlines():
                        m = re.match(
                            r"^(1000:[0-9a-f]{4})\s+([a-z][^;]*?)\s*(;.*)?$",
                            line)
                        if not m:
                            continue
                        a, text = m.group(1), m.group(2).strip()
                        self.assertIn(a, self.aligned,
                                      "%r: not a boundary" % line)
                        checked += 1
                        self.assertEqual(
                            self.aligned[a].text, text,
                            "%s writes `%s %s` in a fence, but tools/dis16.py "
                            "decodes %r there"
                            % (c.doc_rel, a, text, self.aligned[a].text))
                self.assertGreaterEqual(
                    checked, c.min_fence_insn,
                    "only %d fenced instruction lines in %s"
                    % (checked, c.doc_rel))

    def test_every_prose_literal_comes_out_of_the_binary(self):
        tables = json.loads(TABLES.read_text(encoding="utf-8"))
        table_texts = {e["text"] for t in tables["tables"]
                       for e in t["entries"]}
        for c, art in self.each():
            with self.subTest(doc=c.doc_rel):
                md = self.mds[c.art_rel]
                offs = [int(m.group(1), 16)
                        for m in re.finditer(r"CS `0x([0-9a-f]{4})`", md)]
                self.assertGreaterEqual(
                    len(offs), c.min_prose_cs,
                    "%s: only %d CS offsets" % (c.doc_rel, len(offs)))
                for o in offs:
                    self.assertTrue(
                        self.img[o],
                        "CS 0x%04x has a zero length byte" % o)
                    self.cs_literal(o)
                # The `, file 0x.....` half is optional: the docs write both
                # forms, and a pattern that only saw the bare one would
                # silently check a fraction of the quotes it looks like it
                # checks.
                pairs = re.findall(
                    r"`((?!1000:)[^`]+)`\s*\(CS `0x([0-9a-f]{4})`"
                    r"(?:,\s*file `0x[0-9A-Fa-f]{5}`)?\)", md, re.S)
                self.assertGreaterEqual(
                    len(pairs), c.min_prose_pairs,
                    "%s: only %d pairs" % (c.doc_rel, len(pairs)))
                cyr = [p for p in pairs if re.search(r"[\u0400-\u04ff]", p[0])]
                self.assertGreaterEqual(
                    len(cyr), c.min_prose_cyrillic_pairs,
                    "%s: only %d of the %d quoted literals are Russian; the "
                    "pairing is matching something else"
                    % (c.doc_rel, len(cyr), len(pairs)))
                for text, o in pairs:
                    self.assertEqual(
                        self.cs_literal(int(o, 16)), text,
                        "%s quotes %r beside CS 0x%s, which holds %r"
                        % (c.doc_rel, text, o, self.cs_literal(int(o, 16))))
                known = {self.cs_literal(n["cs_offset"])
                         for n, _ in self.walk(art, ("cs_offset", "text"))}
                known |= table_texts
                known |= {self.cs_literal(o) for o in offs}
                # Docs may also quote a literal by its FILE offset; resolve
                # those the same way rather than reporting the Russian in them
                # as unmatched.
                known |= {self.cs_literal(
                    addrmod.image_off_of_file_off(int(m, 16)))
                    for m in re.findall(r"file `0x([0-9A-Fa-f]{5})`", md)}
                unmatched = sorted({run for span in self.spans[c.art_rel]
                                    for run in re.findall(r"[Ѐ-ӿ]+", span)
                                    if not any(run in k for k in known)})
                self.assertEqual(
                    unmatched, [],
                    "Russian in %s that matches no literal in orig/g.exe at "
                    "any address the doc or the artifact names: %r"
                    % (c.doc_rel, unmatched))

    def test_the_prose_and_the_artifact_agree_on_every_arm(self):
        arms = 0
        for c, blk, path, _, _ in self.each_block():
            md = self.mds[c.art_rel]
            for a in blk.get("arms", []):
                with self.subTest(artifact=c.art_rel, arm=a["key"]):
                    for x in (a["compare_addr"], a["span"]["start"]):
                        self.assertIn(
                            x, md,
                            "%s never names %s, which %s records for arm %s"
                            % (c.doc_rel, x, c.art_rel, a["key"]))
                    for s in a["strings"]:
                        self.assertIn(
                            s["cs_offset"], md,
                            "arm %s: %s does not carry CS %s"
                            % (a["key"], c.doc_rel, s["cs_offset"]))
                    for e in a["effects"]:
                        self.assertIn(
                            e["addr"], md,
                            "arm %s: %s does not carry the effect at %s"
                            % (a["key"], c.doc_rel, e["addr"]))
                    arms += 1
        self.assertGreaterEqual(
            arms, 10, "only %d arms across the corpus" % arms)

    # --------------------------------------------------------- sweep totals
    def test_every_recorded_sweep_count_is_the_aligned_decode(self):
        """Each `sweeps` entry recomputed, so no count is read off the file.

        The counts are what every negative in the three maps rests on --
        "zero absolute writes", "one draw", "no unknown-key literal" -- so a
        wrong one is not a cosmetic defect. `SIGNATURES` maps the countable
        keys to the decode predicate that produces them; a `sweeps` key with
        no predicate here is reported rather than skipped.
        """
        SIGNATURES = {
            "str_assign_calls": lambda i: i.text == "call 0xf78:0xae7",
            "char_to_str_calls": lambda i: i.text == "call 0xf78:0xc03",
            "str_append_calls": lambda i: i.text == "call 0xf78:0xb66",
            "writeln_calls": lambda i: i.text == "call 0xeed:0x1c2",
            "write_calls": lambda i: i.text == "call 0xeed:0x0",
            "shortstring_compares": lambda i: i.text == "call 0xf78:0xbd8",
        }
        #: `sweeps` keys checked by a test of their own above, plus the free
        #: prose keys. Anything outside both sets fails below.
        ELSEWHERE = {"instructions", "cs_literal_pushes",
                     "conditional_branches", "random_call_sites",
                     "absolute_memory_writes", "absolute_memory_reads",
                     "dgroup_addresses_touched",
                     "ds_pointer_pushes", "near_calls_out"}
        for c, blk, path, lo, hi in self.each_block():
            run = self.run_of(lo, hi)
            with self.subTest(artifact=c.art_rel, block=path):
                self.assertEqual(len(run), blk["sweeps"]["instructions"])
                dsp = [i for k, i in enumerate(run)
                       if i.text.startswith("mov di,0x") and k + 2 < len(run)
                       and run[k + 1].text == "push ds"
                       and run[k + 2].text == "push di"]
                self.assertEqual(
                    len(dsp), blk["sweeps"]["ds_pointer_pushes"],
                    "%s %s: sweeps.ds_pointer_pushes says %d, the sweep finds "
                    "%d" % (c.art_rel, path,
                            blk["sweeps"]["ds_pointer_pushes"], len(dsp)))
                unknown = [k for k in blk["sweeps"]
                           if not k.endswith("_note") and k != "note"
                           and k not in SIGNATURES and k not in ELSEWHERE]
                self.assertEqual(
                    unknown, [],
                    "%s %s: sweeps keys with no predicate here and no test of "
                    "their own: %s -- an uncounted count is a number nothing "
                    "recomputes" % (c.art_rel, path, unknown))
                for key, pred in SIGNATURES.items():
                    if key not in blk["sweeps"]:
                        continue
                    got = len([i for i in run if pred(i)])
                    self.assertEqual(
                        got, blk["sweeps"][key],
                        "%s %s: sweeps.%s says %d, the sweep finds %d"
                        % (c.art_rel, path, key, blk["sweeps"][key], got))


if __name__ == "__main__":
    unittest.main(verbosity=2)
