#!/usr/bin/env python3
"""`data/beer_arms.json`, `data/beer_uncited.json` and `docs/re/beer.md`
re-derived from `orig/g.exe`.

The three artifacts are the places the same claims about `FUN_1000_29c4` --
the `h` / `mh` beer routine, the half-open image range
`1000:29c4`..`1000:2c5e` -- live; this is what stops any of them drifting from
the binary they describe.  `tools/test_gym_arms.py` is the model and the same
two signals are kept apart for the same reason (`docs/re/METHODOLOGY.md`, "Is
this address a call site?"):

  * **alignment** -- the address is reached by decoding forward from its
    enclosing function's entry, so it is a real instruction boundary and not a
    byte-scan hit in the middle of one;
  * **identity** -- the instruction decoded there says what the artifact says
    it says.

The claims that are NOT restatements of a single decode are asserted by SET
EQUALITY against a sweep of the binary, never by checking that the listed
entries hold up:

  * **the classification covers the branch population EXACTLY.**  The row set
    of `data/beer_uncited.json` must equal the `class == "game"` branches
    `data/branches.json` records for `func_entry == "1000:29c4"`, and every
    recorded count must be a tally of the rows.  **No count in this file is a
    literal** -- a hard-coded `19` in an assertion is the check that cannot
    fail `docs/re/METHODOLOGY.md` names, and it is the exact defect this task's
    brief forbids.
  * **`strings[]` is complete.**  Every `mov di,imm16` / `push cs` / `push di`
    in the range must appear exactly once in the artifact and vice versa, and
    the 253-byte literal pool before the entry must TILE with zero residue --
    so "nine literals and no tenth" is a measurement, and "the routine writes
    nothing else" has no room to be wrong.
  * **the gate inventory is complete.**  Every CONDITIONAL branch in the range
    must be named by the artifact and classified by the twin.
  * **`effects[]` is complete.**  Every instruction in the range with a memory
    DESTINATION must fall in a bucket -- absolute DGROUP, frame, or the two
    string moves -- and an unclassified shape fails loudly rather than passing
    as a read.  That is what makes "hp and beer and nothing else" a
    measurement.
  * **the input buffer is written only by the prologue.**  This is the whole
    argument that one `single` boolean is faithful to six shortstring
    compares, so it is swept rather than asserted.
  * **there is no `Random` draw in range -- and the sweep that says so works.**
    A zero count proves nothing on its own, so the same signature is swept over
    the whole image and required to find the 86 `docs/re/METHODOLOGY.md`
    records.
  * **the call-site census is a set equality, and the wrap is checked per
    site.**  `1000:e966` reaches `1000:29c4` only modulo 64 KiB and
    `re_derive.near_calls_to` is the helper that exists for that mistake;
    `1000:4b00` calls BACKWARD and does not wrap.  A site the artifact records
    as wrapping must actually NEED the wrap, so "both wrap" -- which an earlier
    draft of `data/beer_arms.json` said -- goes red rather than passing.
  * **the range tiles.**  The sixteen spans must cover the range end to end
    with no gap and no overlap, so a block cannot be dropped from the map by
    being left out of every span.

`SrcPairingTest` is kept in its own class because it is the only part of this
file that reads `src/` at all.  It pins each `implemented` row to the FUNCTION
that evaluates the condition, not merely to the module, and it RUNS each row's
`recompute` command -- `docs/re/METHODOLOGY.md` forbids a `src/` line number,
so the command is the citation and a command that has stopped matching is a
broken citation.

    python3 tools/test_beer_arms.py
"""
import collections
import json
import re
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import addr as addrmod            # noqa: E402
import dis16                      # noqa: E402
import re_query                   # noqa: E402
from re_derive import (CITE, aligned_boundaries, far_calls_to,  # noqa: E402
                       inline_spans, load_image, near_calls_to, strip_fences)
# `fn_bodies`, `EXPR_IDENT` and `RUST_NOISE` are Task 39's, and importing them
# is deliberate: `CLAUDE.md` says to re-point the existing instrument rather
# than build a sibling, and a second copy of the rustfmt brace rule is exactly
# the kind of divergence that makes one of the two silently weaker.
from test_combat_opener import (EXPR_IDENT, RUST_NOISE,  # noqa: E402
                                fn_bodies)

REPO = Path(__file__).resolve().parents[1]
ARMS = REPO / "data" / "beer_arms.json"
UNCITED = REPO / "data" / "beer_uncited.json"
BRANCHES = REPO / "data" / "branches.json"
STRINGS = REPO / "data" / "strings.json"
DOC = REPO / "docs" / "re" / "beer.md"
GAPS = REPO / "docs" / "re" / "gaps.md"
DECOMP = REPO / "build" / "decomp" / "FUN_1000_29c4_1000_29c4.c"

#: The routine this file maps.  Everything else -- LO, HI, the branch count,
#: the instruction count -- is DERIVED from `data/branches.json` and the
#: decode, so this entry citation is the only number written down.
ENTRY = "1000:29c4"

#: The `Random` far call, `call 0f78:114b`, by its exact five bytes.
RANDOM_CALL = b"\x9a\x4b\x11\x78\x0f"

#: The Borland shortstring compare, `call 0f78:0bd8`.
STR_COMPARE = b"\x9a\xd8\x0b\x78\x0f"

#: The image-wide `Random` population `docs/re/METHODOLOGY.md` establishes.
#: It is the NEGATIVE CONTROL for the zero-in-range claim, not a fact this
#: file needs on its own -- without it, "zero hits" would be satisfied by a
#: sweep that scans nothing.
IMAGE_WIDE_RANDOM_SITES = 86

#: An instruction whose DESTINATION is an absolute DGROUP address.  The
#: MNEMONIC decides, never operand order; `push [N]` and `cmp [N],imm` are
#: reads.  Copied verbatim from `tools/test_arms_artifacts.py`.
WRITES_ABS_MEM = re.compile(
    r"^(mov|add|sub|adc|sbb|and|or|xor|inc|dec|neg|not"
    r"|shl|shr|sar|rol|ror|rcl|rcr)\s+"
    r"(byte |word |dword )?\[0x[0-9a-f]+\]")

#: An instruction that only READS an absolute-memory operand. Copied verbatim
#: from `tools/test_arms_artifacts.py`, including the `xchg` exclusion -- it
#: belongs in NEITHER bucket so an unclassified shape fails the sweep instead
#: of passing as a read.
READS_ABS_MEM = re.compile(
    r"^(cmp|test|push)\s+(byte |word |dword )?\[0x[0-9a-f]+\]"
    r"|^(?!xchg\b)[a-z]{2,5}\s+[a-z]{2,3},"
    r"(byte |word |dword )?\[0x[0-9a-f]+\]")

#: An instruction whose DESTINATION is anywhere in memory at all -- absolute
#: or frame-relative.  Used to prove the effect inventory is not merely a list
#: of the absolute writes someone happened to notice.
WRITES_ANY_MEM = re.compile(
    r"^(mov|add|sub|adc|sbb|and|or|xor|inc|dec|neg|not"
    r"|shl|shr|sar|rol|ror|rcl|rcr|xchg)\s+"
    r"(byte |word |dword )?\[")

#: The string moves, whose destination is `ES:DI` and therefore the frame --
#: see `argument.es_is_ss` in the artifact.
STRING_MOVES = ("stosb", "stosw", "movsb", "movsw", "rep movsb", "rep movsw")

#: A conditional branch.  `jmp` and `jmp short` are excluded by the negative
#: lookahead, and `jcxz`/`jecxz` would be included -- the routine has none,
#: which is itself checked by the set equality against `data/branches.json`.
JCC = re.compile(r"^j(?!mp\b)\w+\s")

#: A `src/` line that EVALUATES a condition: an `if` head, an `else if` head
#: (rustfmt writes it as `} else if ..`), a `match` head, or a match arm.
#: `} else {` is deliberately NOT one -- it carries no predicate -- and
#: neither is `loop {` or a bare `{`.  This is what a row's `recompute` window
#: is searched for to find the construct the row is filed against.
COND_HEAD = re.compile(r"^(\}\s*)?else\s+if\s|^if\s|^match\s|=>")

#: `grep -n` marks a MATCHED line `N:` and a CONTEXT line `N-`, either of them
#: optionally behind a `path:`.  Both are part of the window the citation
#: printed, so both count.
GREP_LINENO = re.compile(r"^(?:[^:\n]*:)?(\d+)[:-]")

#: Addresses the prose names that are deliberately NOT instruction boundaries,
#: each with the reason.  An exemption without a reason is how a boundary
#: check stops being one.
PROSE_ADDRESS_EXEMPTIONS = {
    "1000:28c7": "the first byte of the routine's literal pool -- DATA, and "
                 "the start of the undefined run docs/re/branches.md records",
    "1000:2c5e": "one past the last instruction; the first byte of the "
                 "4275-byte literal run at file 17710",
    "1000:2c61": "the end address the plan's brief writes, quoted in the doc "
                 "precisely so the four-byte miss is written down rather than "
                 "silently corrected -- it is inside the literal run",
}


def cit(off):
    return "1000:%04x" % off


def off_of(c):
    return int(c.split(":")[1], 16)


class Base(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.arms = json.loads(ARMS.read_text(encoding="utf-8"))
        cls.uncited = json.loads(UNCITED.read_text(encoding="utf-8"))
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        fns = [f for f in cls.branches["functions"] if f["entry"] == ENTRY]
        assert len(fns) == 1, fns
        cls.fn = fns[0]
        cls.lo = addrmod.image_off_of_citation(ENTRY)
        cls.hi = cls.lo + cls.fn["size"]
        cls.ins = dis16.decode_run(cls.img, cls.lo, cls.hi)
        cls.by = {i.off: i for i in cls.ins}
        cls.rows = cls.uncited["branches"]
        #: The branch population, RE-DERIVED rather than read out of a count.
        cls.population = [b for b in cls.branches["branches"]
                          if b["class"] == "game" and b["func_entry"] == ENTRY]

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

    def all_addresses(self, node=None):
        """Every `1000:xxxx` string anywhere in an artifact."""
        out = set()

        def rec(n):
            if isinstance(n, dict):
                for v in n.values():
                    rec(v)
            elif isinstance(n, list):
                for v in n:
                    rec(v)
            elif isinstance(n, str):
                out.update(CITE.findall(n))
        rec(self.arms if node is None else node)
        return out

    def insn_records(self, node=None):
        """Every `{addr, text}` pair anywhere in an artifact."""
        out = []

        def rec(n, path):
            if isinstance(n, dict):
                if isinstance(n.get("addr"), str) and isinstance(
                        n.get("text"), str):
                    out.append((n["addr"], n["text"], path))
                for k, v in n.items():
                    rec(v, "%s.%s" % (path, k))
            elif isinstance(n, list):
                for i, v in enumerate(n):
                    rec(v, "%s[%d]" % (path, i))
        rec(self.arms if node is None else node, "$")
        return out


class BeerTest(Base):
    # ------------------------------------------------------------ the range
    def test_the_range_is_the_recorded_function_extent_and_decodes_as_one_run(
            self):
        """`1000:29c4`..`1000:2c5e`, and 666 bytes tiled by 298 instructions.

        Both ends are derived: `lo` from the entry citation and `hi` from
        `data/branches.json`'s own `size`, so writing a wrong end in the
        artifact reds this rather than moving the check.
        """
        self.assertEqual(self.arms["range"]["start"], ENTRY)
        self.assertEqual(self.arms["range"]["end"], cit(self.hi))
        self.assertEqual(self.arms["range"]["size_bytes"], self.fn["size"])
        self.assertEqual(self.arms["range"]["instruction_count"],
                         len(self.ins))
        self.assertEqual(sum(i.length for i in self.ins), self.fn["size"],
                         "the decode does not tile the recorded extent")
        self.assertEqual(self.ins[-1].end, self.hi)
        self.assertEqual(self.ins[-1].text, "ret 0x4",
                         "the last instruction is not the far return")

    def test_the_spans_tile_the_range(self):
        prev = self.lo
        total = 0
        for s in self.arms["spans"]:
            lo, hi = off_of(s["start"]), off_of(s["end"])
            self.assertEqual(
                lo, prev,
                "span %s starts at %s; the previous one ended at %s -- the "
                "map has a gap or an overlap" % (s["name"], s["start"],
                                                 cit(prev)))
            self.assertIn(lo, self.by, "%s is not an instruction start" % lo)
            self.assertTrue(hi in self.by or hi == self.hi,
                            "%s is not an instruction start" % s["end"])
            n = sum(1 for i in self.ins if lo <= i.off < hi)
            self.assertEqual(s["instruction_count"], n,
                             "span %s records %d instructions, the decode "
                             "finds %d" % (s["name"], s["instruction_count"],
                                           n))
            total += n
            prev = hi
        self.assertEqual(prev, self.hi, "the spans stop short of the range end")
        self.assertEqual(total, len(self.ins))

    # ------------------------------------------------------- classification
    def test_the_rows_cover_the_branch_population_exactly(self):
        """The count is DERIVED, both sides, and never written down.

        `data/branches.json` is filtered by the same rule
        `docs/re/branches.md`'s Coverage block uses -- `class == "game"` and
        `func_entry == "1000:29c4"` -- and the row set must EQUAL it.  A
        missing row, an invented row and a row filed against another function
        each red this.  There is deliberately no literal branch count anywhere
        in this file: asserting `19` against a file that says `19` is the
        check that cannot fail.
        """
        want = {b["addr"] for b in self.population}
        got = [r["addr"] for r in self.rows]
        self.assertEqual(len(got), len(set(got)),
                         "data/beer_uncited.json lists an address twice")
        self.assertEqual(
            set(got), want,
            "the classification and the branch population disagree: only in "
            "the artifact %s, only in data/branches.json %s"
            % (sorted(set(got) - want), sorted(want - set(got))))
        self.assertEqual(self.uncited["counts"]["total"], len(want),
                         "counts.total is not the derived population size")

    def test_every_row_reproduces_its_branches_json_record(self):
        rec = {b["addr"]: b for b in self.population}
        for r in self.rows:
            b = rec[r["addr"]]
            with self.subTest(addr=r["addr"]):
                self.assertEqual(r["text"], b["text"].replace("0x1000:", "0x"))
                self.assertEqual(r["taken"], b["taken"])
                self.assertEqual(r["fallthrough"], b["fallthrough"])
                self.assertEqual(r["guard_status"], b["guard_status"])
                if b["guard"]:
                    self.assertEqual(r["guard"], b["guard"]["addr"])
                    self.assertEqual(r["guard_text"], b["guard"]["text"])
                else:
                    self.assertIsNone(r["guard"])
                    self.assertEqual(r["guard_flag_source_call"],
                                     b["guard_flag_source_call"])
                # and the branch really decodes there, in the binary
                ins = self.at(r["addr"])
                self.assertTrue(
                    JCC.match(ins.text),
                    "%s is classified as a branch and decodes %r"
                    % (r["addr"], ins.text))

    def test_the_recorded_counts_are_the_tally_of_the_rows(self):
        tally = collections.Counter(r["class"] for r in self.rows)
        counts = self.uncited["counts"]
        for k in ("implemented", "unimplemented", "unreachable-in-port"):
            self.assertEqual(counts[k], tally[k],
                             "counts.%s says %d, the rows tally %d"
                             % (k, counts[k], tally[k]))
        self.assertEqual(sum(tally.values()), counts["total"])
        self.assertEqual(set(tally) | {"implemented", "unimplemented",
                                       "unreachable-in-port"},
                         {"implemented", "unimplemented",
                          "unreachable-in-port"},
                         "a row carries a class outside the declared three")

    def test_an_unreachable_in_port_row_must_cite_a_gaps_entry(self):
        """The class exists; using it without evidence must not.

        Today no row carries it, so this loop is empty -- and that is stated
        rather than hidden: the artifact's `empty_classes_are_reported` has to
        be present and non-empty, so the emptiness is a recorded finding
        instead of a silent zero.
        """
        for r in self.rows:
            if r["class"] != "unreachable-in-port":
                continue
            self.assertIn("gaps_entry", r,
                          "%s is unreachable-in-port and cites no "
                          "docs/re/gaps.md entry" % r["addr"])
            self.assertIn(r["gaps_entry"],
                          GAPS.read_text(encoding="utf-8"),
                          "%s cites a docs/re/gaps.md entry that is not there"
                          % r["addr"])
        self.assertTrue(self.uncited["empty_classes_are_reported"].strip())

    def test_the_recorded_divergence_is_in_gaps(self):
        """Every divergence's OWN `recorded_in` must resolve, not one literal.

        The first revision of this test hunted the string
        `"mh` with a broken jaw skips the tail"` -- a literal, not
        `d["recorded_in"]`. A SECOND divergence added later, whose gaps entry
        was never written, would have passed the loop while the mutation case
        beside it claimed to defend the class. That is the project's named
        recurring defect: a guard written against one past symptom rather than
        the class. `recorded_in` is a structured `{file, heading}` record so
        the heading is derived rather than parsed out of a sentence.

        A record that declares itself CLOSED must also have said so in the
        prose, and that is checked here rather than trusted. Task 42 fixed
        this divergence in `src/`; the failure mode it opens is the mirror of
        the one above -- the code stops diverging, the artifact says so, and
        `docs/re/gaps.md` goes on describing an open defect that no longer
        exists. The marker is looked for on the heading's OWN line, so a
        "CLOSED" appearing anywhere else in that 3000-line file cannot stand
        in for it.
        """
        self.assertTrue(self.uncited["divergences"],
                        "the divergence list is empty; Task 41 found one")
        for d in self.uncited["divergences"]:
            with self.subTest(divergence=d["id"]):
                for a in (CITE.findall(d["original"])
                          + CITE.findall(d["port"])):
                    self.at(a)
                rec = d["recorded_in"]
                self.assertIsInstance(
                    rec, dict,
                    "`recorded_in` must be a {file, heading} record so the "
                    "heading can be DERIVED; a sentence has to be parsed and "
                    "a literal cannot be checked at all")
                path = REPO / rec["file"]
                self.assertTrue(path.is_file(),
                                "%s records its divergence in %s, which does "
                                "not exist" % (d["id"], rec["file"]))
                self.assertTrue(rec["heading"].strip(),
                                "%s records an empty heading" % d["id"])
                text = path.read_text(encoding="utf-8")
                self.assertIn(
                    rec["heading"], text,
                    "%s says it is recorded in %s under %r and that heading "
                    "is not there -- the finding exists in one artifact and "
                    "not the other"
                    % (d["id"], rec["file"], rec["heading"]))
                if "CLOSED" in d.get("status", "").upper():
                    lines = [ln for ln in text.splitlines()
                             if ln.startswith("#") and rec["heading"] in ln]
                    self.assertTrue(
                        lines,
                        "%s names a heading that is not a heading in %s"
                        % (d["id"], rec["file"]))
                    self.assertTrue(
                        all("CLOSED" in ln.upper() for ln in lines),
                        "%s records `status` CLOSED and %s's heading %r does "
                        "not say so -- the port stopped diverging and the "
                        "gaps entry still reads as an open defect: %s"
                        % (d["id"], rec["file"], rec["heading"], lines))
                    self.assertIn(
                        "recompute", d.get("fixed_by", {}),
                        "%s says CLOSED and names no `fixed_by.recompute`, "
                        "so nothing holds the fix in place" % d["id"])

    # ------------------------------------------------------------- the gates
    def test_the_recorded_gates_are_every_conditional_branch_in_range(self):
        swept = [cit(i.off) for i in self.ins if JCC.match(i.text)]
        self.assertEqual(
            swept, self.arms["gates"]["addresses"],
            "the conditional-branch sweep and the artifact disagree")
        self.assertEqual(self.arms["gates"]["count"], len(swept))
        # and the sweep must reproduce the artifact this coverage metric is
        # computed from, so a branch cannot exist in one and not the other.
        self.assertEqual(set(swept), {b["addr"] for b in self.population},
                         "the aligned decode and data/branches.json disagree "
                         "about which instructions in this range branch")

    # ----------------------------------------------------------- the strings
    def test_the_recorded_strings_are_every_cs_literal_pushed(self):
        swept = collections.OrderedDict()
        for k, i in enumerate(self.ins):
            if not i.text.startswith("mov di,0x"):
                continue
            self.assertLess(k + 2, len(self.ins))
            self.assertEqual(
                (self.ins[k + 1].text, self.ins[k + 2].text),
                ("push cs", "push di"),
                "%s loads a CS literal and is not followed by the "
                "`push cs` / `push di` pair -- the sweep's shape assumption "
                "is wrong here" % cit(i.off))
            swept.setdefault("0x%04x" % int(i.text.split(",0x")[1], 16),
                             []).append(cit(i.off))
        recorded = collections.OrderedDict(
            (e["cs_offset"], e["pushed_at"]) for e in self.arms["strings"])
        self.assertEqual(swept, recorded,
                         "the CS-literal push sweep and the artifact "
                         "disagree: swept %s, recorded %s" % (swept, recorded))
        self.assertEqual(self.arms["sweeps"]["cs_literal_pushes"],
                         sum(len(v) for v in swept.values()))
        self.assertEqual(self.arms["sweeps"]["distinct_cs_literals"],
                         len(swept))

    def test_every_recorded_literal_decodes_to_the_recorded_text(self):
        for e in self.arms["strings"]:
            off = int(e["cs_offset"], 16)
            with self.subTest(cs=e["cs_offset"]):
                self.assertEqual(self.img[off], e["length"],
                                 "the Pascal length byte at %s is %d, the "
                                 "artifact records %d"
                                 % (e["cs_offset"], self.img[off],
                                    e["length"]))
                self.assertEqual(self.cs_literal(off), e["text"])
                self.assertEqual(
                    int(e["file_offset"], 16),
                    addrmod.file_off_of_image_off(off),
                    "%s: the file offset is not the image offset plus the MZ "
                    "header" % e["cs_offset"])

    def test_the_literal_pool_tiles_with_no_residue(self):
        """Nine records, end to end, landing exactly on the function entry.

        This is what makes `strings[]` a CENSUS: an omitted tenth literal
        would leave the walk short of `1000:29c4` or overshoot it.
        """
        pool = self.arms["literal_pool"]
        p = int(pool["start"], 16)
        self.assertEqual(int(pool["end"], 16), self.lo)
        recs = []
        while p < self.lo:
            recs.append("0x%04x" % p)
            p += 1 + self.img[p]
        self.assertEqual(
            p, self.lo,
            "the pool walk from %s overshoots the entry by %d bytes -- it is "
            "not a chain of Pascal records" % (pool["start"], p - self.lo))
        self.assertEqual(
            recs, pool["records"],
            "the literal-pool walk finds %s and the artifact records %s"
            % (recs, pool["records"]))
        self.assertEqual(
            pool["record_count"], len(recs),
            "the literal pool records %d Pascal strings and the walk finds "
            "%d -- the string census is not complete"
            % (pool["record_count"], len(recs)))
        self.assertEqual(pool["residual_bytes"], 0,
                         "the literal pool is recorded with residue")
        self.assertEqual(
            recs, [e["cs_offset"] for e in self.arms["strings"]],
            "the pool records and the pushed literals are not the same set -- "
            "either a literal is never pushed or one is pushed from outside "
            "the pool")

    def test_the_literal_pool_is_private_to_the_routine(self):
        for e in self.arms["strings"]:
            off = int(e["cs_offset"], 16)
            pat = b"\xbf" + off.to_bytes(2, "little")
            hits = [k for k in range(len(self.img) - 2)
                    if self.img[k:k + 3] == pat]
            outside = [cit(h) for h in hits
                       if not (self.lo <= h < self.hi)]
            with self.subTest(cs=e["cs_offset"]):
                self.assertEqual(
                    outside, [],
                    "%s is loaded by a `mov di,imm16` outside the routine at "
                    "%s, so the pool is not private and 'which arm writes it' "
                    "is incomplete" % (e["cs_offset"], outside))
                self.assertEqual(
                    len(hits), len(e["pushed_at"]),
                    "%s: %d `mov di,imm16` sites in the image, %d recorded"
                    % (e["cs_offset"], len(hits), len(e["pushed_at"])))

    def test_the_two_verb_tokens_are_the_gap_strings_json_leaves(self):
        """Two artifacts corroborating each other, both re-derived.

        `data/strings.json` has no entry for the `h` and `mh` tokens, and
        `docs/re/branches.md`'s unaccounted-bytes table has a 5-byte run at
        `1000:28c7` / file `0x4197`.  Those are the same five bytes.  Checked
        because the doc says so, and because a `strings.json` that later grew
        the entries would make the doc's sentence wrong.
        """
        known = {e["off"] for e in json.loads(
            STRINGS.read_text(encoding="utf-8"))}
        tokens = [e for e in self.arms["strings"] if e["role"] == "token"]
        self.assertTrue(tokens)
        total = 0
        for e in tokens:
            fo = int(e["file_offset"], 16)
            self.assertNotIn(
                fo, known,
                "data/strings.json now HAS file %s (%r); "
                "data/beer_arms.json's `strings_json_gap` and "
                "docs/re/branches.md's 28-byte table both say it does not"
                % (e["file_offset"], e["text"]))
            total += 1 + e["length"]
        self.assertEqual(total, 5,
                         "the two tokens are %d bytes, not the 5 the "
                         "unaccounted-bytes table records" % total)
        for e in self.arms["strings"]:
            if e["role"] != "token":
                fo = int(e["file_offset"], 16)
                self.assertIn(
                    fo, known,
                    "file %s (%r) is a message and data/strings.json does not "
                    "know it -- the gap is wider than recorded"
                    % (e["file_offset"], e["text"]))

    # ----------------------------------------------------------- the effects
    def test_the_recorded_effects_are_every_absolute_write_in_range(self):
        """And every memory destination falls in a named bucket.

        A list of the absolute writes alone would be satisfied by a sweep that
        quietly ignored a shape it did not recognise, so every instruction
        with ANY memory destination is bucketed and an unclassified one fails
        by name.
        """
        abs_writes, frame, moves, unclassified = [], [], [], []
        for i in self.ins:
            if i.text in STRING_MOVES or i.text.startswith("rep "):
                moves.append(cit(i.off))
            elif WRITES_ABS_MEM.match(i.text):
                abs_writes.append(cit(i.off))
            elif WRITES_ANY_MEM.match(i.text):
                if re.search(r"\[(bp|sp|bx|si|di)[+-]", i.text):
                    frame.append(cit(i.off))
                else:
                    unclassified.append((cit(i.off), i.text))
        self.assertEqual(unclassified, [],
                         "an instruction with a memory destination fell in no "
                         "bucket, so the effect inventory is not a sweep")
        self.assertEqual(
            abs_writes, [e["addr"] for e in self.arms["effects"]
                         ["absolute_writes"]],
            "the absolute-write sweep and the artifact disagree")
        self.assertEqual(self.arms["effects"]["count"], len(abs_writes))
        self.assertEqual(
            sorted({e["ds"] for e in self.arms["effects"]["absolute_writes"]}),
            ["20ae:38ac", "20ae:38c3"],
            "the routine is recorded as writing hp and beer and nothing else")
        # the two string moves and the one frame word, named rather than left
        # as residue
        self.assertEqual(moves, ["1000:29e2", "1000:29e6"])
        self.assertEqual(frame, ["1000:2a14"])

    def test_the_recorded_globals_are_every_dgroup_address_the_range_touches(
            self):
        """A SET EQUALITY, which is what makes `globals[]` a measurement.

        `test_the_globals_read_and_write_lists_are_the_decode` iterates the
        four already recorded, so it cannot see a fifth -- and the effects
        sweep only buckets WRITES, so a global the routine merely READS is
        invisible to both. That gap is exactly what
        `tools/test_arms_artifacts.py`'s
        `test_the_dgroup_addresses_touched_are_the_recorded_globals` covers for
        the three handler maps, and this artifact is outside that corpus on
        purpose, so the sweep is written here rather than inherited.

        Two directions, plus a bucketing completeness check: every instruction
        carrying an absolute-memory operand must be classified a READ or a
        WRITE, so an unrecognised shape fails by name instead of counting as
        neither.
        """
        swept, unclassified = set(), []
        for i in self.ins:
            hits = re.findall(r"\[0x([0-9a-f]+)\]", i.text)
            if not hits:
                continue
            swept.update("20ae:" + h for h in hits)
            if not (WRITES_ABS_MEM.match(i.text)
                    or READS_ABS_MEM.match(i.text)):
                unclassified.append((cit(i.off), i.text))
        self.assertEqual(
            unclassified, [],
            "an instruction carrying an absolute-memory operand fell in "
            "neither the READ nor the WRITE bucket, so the census below is "
            "not a sweep")
        recorded = {g["ds"] for g in self.arms["globals"]}
        self.assertEqual(
            swept, recorded,
            "the DGROUP sweep and `globals[]` disagree: only in the decode "
            "%s, only in the artifact %s -- `globals[]` is claimed to be "
            "EVERY address the range touches"
            % (sorted(swept - recorded), sorted(recorded - swept)))
        self.assertEqual(
            self.arms["sweeps"]["dgroup_addresses_touched"], len(swept),
            "sweeps.dgroup_addresses_touched records %s, the decode touches "
            "%d" % (self.arms["sweeps"]["dgroup_addresses_touched"],
                    len(swept)))
        self.assertTrue(
            self.arms["globals_are_a_measurement"].strip(),
            "the artifact must say that `globals[]` is a measurement, so the "
            "claim this test defends is written down beside it")

    def test_the_globals_read_and_write_lists_are_the_decode(self):
        for g in self.arms["globals"]:
            off = g["ds"].split(":")[1]
            reads, writes = [], []
            for i in self.ins:
                if "[0x%s]" % off not in i.text:
                    continue
                (writes if WRITES_ABS_MEM.match(i.text) else reads).append(
                    cit(i.off))
            with self.subTest(ds=g["ds"]):
                self.assertEqual(g["read_at"], reads)
                self.assertEqual(g["written_at"], writes)

    def test_the_input_buffer_is_written_only_by_the_prologue(self):
        """The whole argument that one `single` boolean is faithful to six.

        If anything after `1000:29e6` could write `[bp-0x100]`, the six later
        shortstring compares would not all be re-evaluations of the same
        predicate and `Game::beer`'s `single` would be a divergence rather
        than a hoist.  So it is swept.
        """
        def writes_the_buffer(t):
            # the buffer is [bp-0x100] .. [bp-0x001]; [bp-0x102] is the hp
            # snapshot BELOW it and is a different slot, checked separately.
            m = re.match(r"^\w+\s+(?:byte |word )?\[bp-(0x[0-9a-f]+)\],",
                         t)
            return bool(m) and 1 <= int(m.group(1), 16) <= 0x100
        writers = [cit(i.off) for i in self.ins
                   if i.text in STRING_MOVES or i.text.startswith("rep ")
                   or writes_the_buffer(i.text)]
        self.assertEqual(
            writers, ["1000:29e2", "1000:29e6"],
            "something other than the prologue's `stosb` / `rep movsb` writes "
            "the frame buffer, so the `single` hoist is not established")
        # ES is SS, which is what keeps those two off DGROUP.
        es = self.arms["argument"]["es_is_ss"]
        self.assertEqual(self.at(es["addr"]).text, es["text"])
        self.assertEqual(self.at(es["then"]["addr"]).text, es["then"]["text"])
        # and the six compares the hoist stands for really are six.
        h = [e for e in self.arms["strings"] if e["text"] == "h"]
        mh = [e for e in self.arms["strings"] if e["text"] == "mh"]
        self.assertEqual(len(h), 1)
        self.assertEqual(len(mh), 1)
        self.assertEqual(len(h[0]["pushed_at"]) + len(mh[0]["pushed_at"]),
                         self.arms["sweeps"]["shortstring_compares"],
                         "the token pushes and the shortstring compares do "
                         "not pair one to one")

    def test_the_hp_snapshot_slot_is_written_once_and_read_four_times(self):
        slot = self.arms["stack_slot_bp_minus_0x102"]
        written = [cit(i.off) for i in self.ins
                   if re.match(r"^\w+\s+(byte |word )?\[bp-0x102\]", i.text)]
        read = [cit(i.off) for i in self.ins
                if "[bp-0x102]" in i.text and cit(i.off) not in written]
        self.assertEqual(
            written, [slot["written_at"][1]["addr"]],
            "the entry-hp slot is written more than once, so the three tail "
            "compares are not all against the ENTRY value")
        self.assertEqual(read, [r["addr"] for r in slot["read_at"]])
        # the snapshot is taken BEFORE the jaw test -- which is what lets the
        # refusal path fall into the tail and still mean "nothing was drunk".
        jaw = [r for r in self.rows if r["addr"] == "1000:2a1d"][0]
        self.assertLess(off_of(written[0]), off_of(jaw["guard"]))
        for c in slot["the_three_compares"]:
            b = [r for r in self.rows if r["addr"] == c["branch"]][0]
            self.assertEqual(b["guard"], c["guard"])
            self.assertEqual(b["taken"], c["taken"])
            self.assertIn("[bp-0x102]", self.at(c["guard"]).text)

    # ------------------------------------------------------------- the draws
    def test_the_range_spends_no_draw_and_the_sweep_that_says_so_works(self):
        """Zero in range, 86 image-wide.  The second half is the control."""
        hits = [cit(k) for k in range(self.lo, self.hi)
                if self.img[k:k + 5] == RANDOM_CALL]
        self.assertEqual(hits, [],
                         "the beer routine draws at %s" % hits)
        self.assertEqual(self.arms["draws"]["count"], 0)
        wide = sum(1 for k in range(len(self.img) - 4)
                   if self.img[k:k + 5] == RANDOM_CALL)
        self.assertEqual(
            wide, IMAGE_WIDE_RANDOM_SITES,
            "the same sweep finds %d Random sites image-wide, not the %d "
            "docs/re/METHODOLOGY.md establishes -- so a zero in range says "
            "nothing" % (wide, IMAGE_WIDE_RANDOM_SITES))
        self.assertEqual(self.arms["draws"]["image_wide_random_sites"], wide)
        # and the flow half: four call targets, none of them Random.
        targets = sorted({i.text[len("call "):] for i in self.ins
                          if i.text.startswith("call ")})
        want = sorted("0x%s:0x%s" % (t.split(":")[0].lstrip("0") or "0",
                                     t.split(":")[1].lstrip("0") or "0")
                      for t in self.arms["draws"]["call_targets"])
        self.assertEqual(targets, want,
                         "the aligned decode's call targets are %s" % targets)

    # -------------------------------------------------------- the call sites
    def test_both_call_sites_wrap_to_the_entry_and_there_is_no_third(self):
        near = near_calls_to(self.img, self.lo)
        far = far_calls_to(self.img, self.lo)
        census = self.arms["call_sites"]["census"]
        self.assertEqual(near, census["near"])
        self.assertEqual(far, census["far"])
        self.assertEqual(far, [], "a far call to the routine appeared")
        self.assertEqual(
            [s["addr"] for s in self.arms["call_sites"]["sites"]
             if s["wraps"]], ["1000:e966"],
            "exactly one of the two call sites needs the 16-bit wrap; an "
            "earlier draft of the artifact said both did")
        self.assertEqual(
            len(near), self.fn["caller_count"],
            "the near-call census and data/branches.json's caller_count "
            "disagree")
        for s in self.arms["call_sites"]["sites"]:
            with self.subTest(site=s["addr"]):
                ins = self.at(s["addr"])
                self.assertEqual(ins.hex(), s["bytes"])
                self.assertEqual(ins.raw[0], 0xE8, "not a near call")
                disp = int.from_bytes(ins.raw[1:3], "little", signed=True)
                raw = off_of(s["addr"]) + 3 + disp
                self.assertEqual(
                    raw & 0xFFFF, self.lo,
                    "%s does not reach the entry even modulo 64 KiB"
                    % s["addr"])
                self.assertEqual(
                    raw != self.lo, s["wraps"],
                    "%s records wraps=%s and the SIGNED displacement %#x "
                    "gives %#x -- a site recorded as wrapping must actually "
                    "need the wrap, or the check is not exercising the "
                    "mistake it exists for"
                    % (s["addr"], s["wraps"], disp, raw))
                for p in s["pushes"]:
                    self.assertEqual(self.at(p["addr"]).text, p["text"])
                self.assertEqual(
                    "20ae:" + s["pushes"][0]["text"].split(",0x")[1],
                    s["buffer"])
        self.assertEqual(
            sorted(s["addr"] for s in self.arms["call_sites"]["sites"]),
            sorted(near))

    # ------------------------------------------------------- generic sweeps
    def test_every_cited_instruction_decodes_to_what_the_artifact_says(self):
        seen = 0
        for a, t, path in self.insn_records():
            if not CITE.fullmatch(a):
                continue
            with self.subTest(addr=a, path=path):
                self.assertEqual(
                    self.at(a).text, t,
                    "data/beer_arms.json says %s is %r at %s; orig/g.exe "
                    "decodes %r" % (a, t, path, self.at(a).text))
            seen += 1
        self.assertGreaterEqual(
            seen, 15,
            "only %d {addr,text} records found -- the walk is not reaching "
            "the artifact's content" % seen)

    def test_every_address_the_artifact_names_is_a_boundary(self):
        found = self.all_addresses()
        self.assertGreaterEqual(len(found), 80,
                                "only %d addresses found in the artifact"
                                % len(found))
        for a in sorted(found):
            if a in PROSE_ADDRESS_EXEMPTIONS:
                continue
            self.at(a)

    def test_every_recorded_sweep_count_is_the_aligned_decode(self):
        s = self.arms["sweeps"]
        got = {
            "instructions": len(self.ins),
            "bytes": self.hi - self.lo,
            "conditional_branches": sum(1 for i in self.ins
                                        if JCC.match(i.text)),
            "unconditional_jumps": sum(1 for i in self.ins
                                       if i.text.startswith("jmp ")),
            "returns": sum(1 for i in self.ins if i.text.startswith("ret")),
            "cs_literal_pushes": sum(1 for i in self.ins
                                     if i.text.startswith("mov di,0x")),
            "distinct_cs_literals": len({i.text for i in self.ins
                                         if i.text.startswith("mov di,0x")}),
            "shortstring_compares": sum(1 for i in self.ins
                                        if i.raw[:5] == STR_COMPARE),
            "writeln_calls": sum(1 for i in self.ins
                                 if i.text == "call 0xeed:0x1c2"),
            "write_calls": sum(1 for i in self.ins
                               if i.text == "call 0xeed:0x0"),
            "absolute_writes": sum(1 for i in self.ins
                                   if WRITES_ABS_MEM.match(i.text)),
            "random_calls": 0,
        }
        for k, v in got.items():
            self.assertEqual(s[k], v,
                             "sweeps.%s records %s, the decode counts %s"
                             % (k, s[k], v))

    def test_the_globals_xref_census_is_what_re_query_reports(self):
        """"the only writer that lowers hpmax" is a measured set, not a phrase."""
        prog = re_query.Program()
        # Every writer of ANY of the four recorded globals: the identity notes
        # argue from PAIRS (`20ae:38ac` is hp because the level-up raises it
        # beside `20ae:38ae`), so a note legitimately cites a sibling's writer.
        # An invented address still fails, which is the point.
        every_writer = set()
        for g in self.arms["globals"]:
            every_writer.update(
                a["at"] for a in re_query.xrefs_to(prog, g["ds"])["scan"]
                ["accepted"] if WRITES_ABS_MEM.match(a["text"]))
        for g in self.arms["globals"]:
            scan = re_query.xrefs_to(prog, g["ds"])["scan"]
            writers = [a["at"] for a in scan["accepted"]
                       if WRITES_ABS_MEM.match(a["text"])]
            with self.subTest(ds=g["ds"]):
                self.assertEqual(
                    g["xrefs_accepted"], len(scan["accepted"]),
                    "%s: the artifact records %d accepted references and "
                    "xrefs-to reports %d"
                    % (g["ds"], g["xrefs_accepted"], len(scan["accepted"])))
                self.assertEqual(
                    g["xrefs_writes"], len(writers),
                    "%s: the artifact records %d writers and xrefs-to reports "
                    "%d" % (g["ds"], g["xrefs_writes"], len(writers)))
                self.assertIn("%d refs" % g["xrefs_accepted"],
                              g["identity"].replace("references", "refs")
                              .replace("accepts ", ""),
                              "%s: the identity PROSE and the numeric fields "
                              "disagree" % g["ds"])
                # Every address the note names from OUTSIDE this routine is
                # a claim about the image-wide writer set and must be in it.
                # In-range addresses are this routine's own reads and are
                # checked by `test_the_globals_read_and_write_lists_are_the_
                # decode` instead.
                for a in CITE.findall(g["identity"]):
                    if self.lo <= off_of(a) < self.hi:
                        continue
                    self.assertIn(
                        a, every_writer,
                        "%s's identity note cites %s as an image-wide writer "
                        "and xrefs-to lists it as a writer of none of the "
                        "four globals" % (g["ds"], a))
                for a in g["written_at"]:
                    self.assertIn(a, writers)

    @unittest.skipUnless(DECOMP.is_file(),
                         "build/decomp/ is gitignored scratch; regenerate "
                         "with ./tools/ghidra/run_ghidra.sh --decomp-only")
    def test_the_decompilation_is_a_lead_not_a_census(self):
        """The doc's 106-of-298 figure, recomputed.

        `docs/re/beer.md` uses it to justify not reading the tail out of
        Ghidra's C.  A figure that drifted would make the justification
        rhetorical.
        """
        c = DECOMP.read_text(encoding="utf-8")
        seen = set(CITE.findall(c))
        mine = {cit(i.off) for i in self.ins}
        annotated = len(seen & mine)
        self.assertIn("**%d** of the routine's %d instruction addresses"
                      % (annotated, len(self.ins)),
                      DOC.read_text(encoding="utf-8"),
                      "docs/re/beer.md's annotation-coverage figure is not "
                      "%d of %d" % (annotated, len(self.ins)))
        for a in ("1000:2a14", "1000:2c36"):
            self.assertNotIn(
                a, seen,
                "%s IS annotated in the decompilation now; docs/re/beer.md "
                "names it as one of the folded-out addresses" % a)


class ProseTest(Base):
    """`docs/re/beer.md` against the binary, span by span."""

    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        cls.md = DOC.read_text(encoding="utf-8")
        cls.prose = strip_fences(cls.md)

    def test_every_prose_address_is_an_instruction_boundary(self):
        found = set(CITE.findall(self.prose))
        self.assertGreaterEqual(
            len(found), 90,
            "only %d addresses found in the prose -- the scan is not reading "
            "the document" % len(found))
        for a in sorted(found):
            if a in PROSE_ADDRESS_EXEMPTIONS:
                continue
            self.at(a)
        # and every exemption must actually be USED, so the list cannot grow
        # into a way of silencing a real miss.
        for a, why in PROSE_ADDRESS_EXEMPTIONS.items():
            self.assertIn(a, found,
                          "%s is exempted from the boundary check (%s) and "
                          "the prose does not name it" % (a, why))
            self.assertNotIn(a, self.aligned,
                             "%s is exempted from the boundary check and IS a "
                             "boundary -- the exemption is stale" % a)

    def test_every_prose_instruction_says_what_the_binary_says(self):
        seen = 0
        for s in inline_spans(self.prose):
            m = re.match(r"^(1000:[0-9a-f]{4})\s+([a-z].*)$", s)
            if not m:
                continue
            a, t = m.group(1), m.group(2)
            with self.subTest(span=s):
                self.assertEqual(
                    self.at(a).text, t,
                    "docs/re/beer.md writes `%s` and orig/g.exe decodes %r"
                    % (s, self.at(a).text))
            seen += 1
        self.assertGreaterEqual(
            seen, 60,
            "only %d `addr text` spans matched -- `strip_fences` has "
            "desynchronised and the scan is quietly measuring nothing" % seen)

    def test_every_instruction_inside_a_fence_says_what_the_binary_says(self):
        seen = 0
        for f in re.findall(r"^```\n(.*?)^```", self.md, re.S | re.M):
            for ln in f.splitlines():
                m = re.match(r"^([0-9a-f]{4})\s+((?:[0-9a-f]{2} )+)\s*"
                             r"(\S.*?)(?:\s{2,};.*)?$", ln)
                if not m:
                    continue
                a = cit(int(m.group(1), 16))
                with self.subTest(line=ln):
                    self.assertEqual(self.at(a).hex(), m.group(2).strip())
                    self.assertEqual(self.at(a).text, m.group(3).strip())
                seen += 1
        self.assertGreaterEqual(
            seen, 15,
            "only %d fenced disassembly lines matched" % seen)

    def test_every_prose_literal_comes_out_of_the_binary(self):
        """Every `CS 0x....`/text pair in the prose table, re-decoded."""
        seen = 0
        for e in self.arms["strings"]:
            row = "`%s` | `%s` |" % (e["cs_offset"], e["file_offset"])
            self.assertIn(row, self.md,
                          "the prose string table has no row for %s"
                          % e["cs_offset"])
            self.assertIn("`%s` |" % e["text"], self.md,
                          "the prose string table does not carry %r"
                          % e["text"])
            seen += 1
        self.assertEqual(seen, len(self.arms["strings"]))

    def test_the_prose_and_the_artifact_agree_on_the_divergence(self):
        for d in self.uncited["divergences"]:
            named = set(CITE.findall(self.md))
            for a in CITE.findall(d["original"]) + CITE.findall(d["port"]):
                self.assertTrue(
                    a in named,
                    "data/beer_uncited.json's divergence cites %s and "
                    "docs/re/beer.md does not name it -- the two halves of "
                    "the finding have drifted" % a)
            rec = d["recorded_in"]
            self.assertIn(rec["file"], self.md,
                          "the prose does not point at %s" % rec["file"])
            # The prose wraps, so both sides are flattened -- otherwise this
            # would fail on a line break and tempt the next author to weaken
            # it back to a prefix match.
            self.assertIn(
                " ".join(rec["heading"].split()), " ".join(self.md.split()),
                "docs/re/beer.md points at %s but does not quote the heading "
                "%r the record names, so a reader cannot find the entry and a "
                "renamed heading would not be caught here"
                % (rec["file"], rec["heading"]))


class SrcPairingTest(Base):
    """The only class here that reads `src/`.

    Three independent checks per `implemented` row, because they fail
    differently: the identifier check pins the row to the FUNCTION that
    evaluates the condition, the construct resolver pins it to the LINE inside
    that function, and running the `recompute` command establishes
    that the citation `docs/re/METHODOLOGY.md` requires -- a command, never a
    line number -- still finds it.
    """

    #: `{module: (lines, fn_bodies)}`, so a module is read once per process.
    _mod_cache = {}

    def _module(self, rel):
        """`(lines, fn_bodies)` for one `src/` module, read once."""
        if rel not in self._mod_cache:
            whole = (REPO / rel).read_text(encoding="utf-8")
            self._mod_cache[rel] = (whole.splitlines(), fn_bodies(whole))
        return self._mod_cache[rel]

    def _run(self, cmd):
        return subprocess.run(cmd, shell=True, cwd=REPO,
                              capture_output=True, text=True)

    def resolve_construct(self, r):
        """The ONE `src/` line a row's citation resolves to.

        `docs/re/METHODOLOGY.md` forbids writing a line number into an
        artifact, so nothing in `data/beer_uncited.json` names one.  This
        recomputes it, every run, from what the row already carries: run
        `src.recompute`, keep the line numbers the command PRINTED (matched
        and context alike) that fall inside `src.function`'s body, and take
        the first of those that is a condition head.  The return value is
        `(line number, the line's text)` -- the number is used only to group
        rows within this process and is never written anywhere.
        """
        src = r["src"]
        lines, fns = self._module(src["module"])
        name = src["function"].split("::")[-1]
        span = fns.get(name)
        self.assertTrue(span, "%s names %s and %s defines no unambiguous "
                              "`fn %s`" % (r["addr"], src["function"],
                                           src["module"], name))
        lo = span[0]
        hi = lo + span[1].count("\n")
        out = self._run(src["recompute"]).stdout
        printed = sorted({int(m.group(1)) for m in
                          (GREP_LINENO.match(ln) for ln in out.splitlines())
                          if m})
        heads = [n for n in printed
                 if lo <= n <= hi and COND_HEAD.search(lines[n - 1].strip())]
        self.assertTrue(
            heads,
            "%s: `%s` printed %d line(s), none of which is a condition head "
            "inside `fn %s` -- the command runs but does not reach the "
            "construct that EVALUATES this branch, which is the shape "
            "`1000:2c0d` shipped with (a window four lines below its own `if`, "
            "passing on the bare token `healed` as a call argument)"
            % (r["addr"], src["recompute"], len(printed), name))
        return heads[0], lines[heads[0] - 1].strip()

    def test_every_implemented_row_names_the_function_that_evaluates_it(self):
        bodies = {}
        seen = 0
        for r in self.rows:
            if r["class"] != "implemented":
                self.assertNotIn("src", r,
                                 "%s is not `implemented` and names a src "
                                 "construct" % r["addr"])
                continue
            src = r["src"]
            path = REPO / src["module"]
            self.assertTrue(path.is_file(),
                            "%s names a module that does not exist: %s"
                            % (r["addr"], src["module"]))
            if src["module"] not in bodies:
                whole = path.read_text(encoding="utf-8")
                bodies[src["module"]] = (fn_bodies(whole), whole)
            name = src["function"].split("::")[-1]
            found_fns, whole = bodies[src["module"]]
            span = found_fns.get(name, False)
            self.assertIsNot(span, False,
                             "%s names %s, and %s defines no `fn %s`"
                             % (r["addr"], src["function"], src["module"],
                                name))
            self.assertIsNotNone(
                span, "%s names %s and the span is ambiguous"
                % (r["addr"], src["function"]))
            body = span[1]
            idents = set(EXPR_IDENT.findall(src["expr"])) - RUST_NOISE
            self.assertTrue(
                idents,
                "%s pairs %s with an expression naming nothing checkable, so "
                "the pairing is asserted by nothing"
                % (r["addr"], src["function"]))
            for ident in sorted(idents):
                self.assertIn(
                    ident, body,
                    "%s pairs %s with an expression naming `%s`, which occurs "
                    "%s" % (r["addr"], src["function"], ident,
                            "ELSEWHERE IN THE MODULE but not in this "
                            "function's body -- the row names the wrong "
                            "construct" if ident in whole else
                            "NOWHERE IN THE MODULE at all"))
            seen += 1
        self.assertEqual(seen, self.uncited["counts"]["implemented"])

    def test_no_two_rows_share_a_construct_without_saying_which_half_they_mean(
            self):
        """The CLASS behind the `1000:2c36` defect, not just that instance.

        `1000:2c36` tests hp-vs-hp0 and was filed with the `expr` and
        `recompute` of `1000:2c3d`, which tests `beer_dl == 0` -- byte
        identical, distinguishable only by a `why` sentence no test reads. The
        identifier check cannot see it: `player`, `beer_dl` and `term` all
        occur in `Game::beer`, so the pairing passes for the wrong construct,
        and Task 42 would have put `// 1000:2c36` on the `beer_dl` line.

        Sharing a construct is LEGITIMATE here -- the port evaluates one
        predicate where the original tests it more than once -- so the rule is
        not "never share". It is: a shared construct must be declared, and each
        sharer must say what it means by it.

        The first revision of this test grouped on `(module, function, expr,
        recompute)`, which is TEXT. `1000:2bc6`, `1000:2c0d` and `1000:2c36`
        all evaluate at the single `if healed != 0 {` -- the `Game::beer` doc
        comment says "this one test stands for all three" -- and because each
        row worded its `expr` differently the check grouped NONE of them and
        demanded NO discriminator. That is the guard-written-against-one-past-
        symptom defect, inside the test written to prevent it, and the
        whole-branch review of this plan found it. So the key is no longer the
        text: `resolve_construct` RUNS each row's own citation and resolves it
        to one line of the shipped tree, and rows that land on the same line
        are the group -- a fact about `src/`, not about how a row is worded.

        Two shapes of sharing, declared per row as `src.shares`:

          * `disjunct` -- the port folded several original branches into
            several sub-expressions of ONE line (`if single || hp >= hpmax ||
            beer_dl == 0`). Each `discriminator` is the sub-expression that
            row means, and they must be DISTINCT.
          * `re-evaluation` -- the original tests one predicate more than once
            and the port evaluates it once, so there is no "which half". Every
            `discriminator` is the same sub-expression and they must AGREE;
            two that differ mean at least one row is misfiled.

        Either way the discriminator must occur in the RESOLVED SOURCE LINE,
        not merely in the row's `expr` paraphrase -- a paraphrase is written by
        the same hand as the discriminator, so checking one against the other
        is close to checking a value against itself.

        WHAT THIS STILL DOES NOT CATCH, stated because the shape it misses is
        the one it was written for. Nothing here knows which predicate the
        BRANCH evaluates. The resolver takes the FIRST condition head in the
        window the row's own command printed, so a row whose window is widened
        backwards past another head resolves to that other head. That usually
        reds -- it lands in a group whose members disagree, or alone with a
        discriminator it must not carry -- but "usually" is not "always", and
        reading each row's `why` against the flow is still what closes it.
        """
        groups = collections.defaultdict(list)
        seen = 0
        for r in self.rows:
            if r["class"] != "implemented":
                continue
            line, text = self.resolve_construct(r)
            groups[(r["src"]["module"], line, text)].append(r)
            seen += 1
        self.assertEqual(seen, self.uncited["counts"]["implemented"])
        shared = {k: v for k, v in groups.items() if len(v) > 1}
        for key, rs in sorted(shared.items()):
            construct = key[2]
            addrs = [r["addr"] for r in rs]
            with self.subTest(rows=addrs):
                kinds = {r["src"].get("shares") for r in rs}
                self.assertEqual(
                    len(kinds), 1,
                    "%s resolve to the one construct %r and declare more than "
                    "one `src.shares` (%s) -- a construct is shared one way or "
                    "the other, so at least one row is misfiled"
                    % (addrs, construct, sorted(map(repr, kinds))))
                kind = next(iter(kinds))
                self.assertIn(
                    kind, ("disjunct", "re-evaluation"),
                    "%s resolve to the one construct %r and declare "
                    "`src.shares` = %r -- it must be `disjunct` (several "
                    "sub-expressions of one line) or `re-evaluation` (one "
                    "predicate the original tests more than once)"
                    % (addrs, construct, kind))
                discs = []
                for r in rs:
                    d = r["src"].get("discriminator")
                    self.assertTrue(
                        d,
                        "%s resolves to the same construct as %s (%r) and "
                        "names no `src.discriminator`, so nothing says what it "
                        "means by it -- and a row that has drifted onto a "
                        "sibling's construct is indistinguishable from one "
                        "that legitimately shares it"
                        % (r["addr"], [a for a in addrs if a != r["addr"]],
                           construct))
                    self.assertIn(
                        d, construct,
                        "%s's discriminator %r is not a sub-expression of the "
                        "source line it resolves to (%r) -- either the "
                        "discriminator is wrong or the row is filed against "
                        "the wrong construct" % (r["addr"], d, construct))
                    discs.append(d)
                if kind == "disjunct":
                    self.assertEqual(
                        len(set(discs)), len(discs),
                        "%s share the `disjunct` construct %r and two of them "
                        "claim the same half (%s), so at least one names a "
                        "predicate it does not evaluate"
                        % (addrs, construct, discs))
                else:
                    self.assertEqual(
                        len(set(discs)), 1,
                        "%s share the construct %r as a `re-evaluation` of one "
                        "predicate, so their discriminators must AGREE and "
                        "these do not (%s) -- either the group is really "
                        "`disjunct` or a row is filed against the wrong "
                        "construct" % (addrs, construct, discs))
        # A row that resolves to a line of its OWN must carry neither field,
        # or a discriminator left behind by an earlier revision would sit
        # there unread -- which is how the artifact stops describing itself.
        alone = [r for k, v in groups.items() if len(v) == 1 for r in v]
        for r in alone:
            for field in ("shares", "discriminator"):
                self.assertNotIn(
                    field, r["src"],
                    "%s resolves to a construct no other row resolves to and "
                    "still names `src.%s` -- either it is stale or the row it "
                    "used to share with has moved" % (r["addr"], field))
        # And the shape must actually occur, or the check above is vacuous.
        self.assertTrue(
            shared,
            "no two `implemented` rows resolve to the same construct, so this "
            "test asserted nothing -- if the artifact really has no shared "
            "constructs, delete it rather than leave it passing vacuously")

    def test_every_recompute_command_still_finds_its_construct(self):
        """A `src/` citation is a COMMAND here, so the command is run.

        `docs/re/METHODOLOGY.md`: a `file:line` reference is
        stale-by-construction and a command is not.  That only holds while the
        command still matches, which is what this establishes.
        """
        seen = 0
        for r in self.rows:
            if r["class"] != "implemented":
                continue
            cmd = r["src"]["recompute"]
            with self.subTest(addr=r["addr"]):
                p = subprocess.run(cmd, shell=True, cwd=REPO,
                                   capture_output=True, text=True)
                self.assertEqual(
                    p.returncode, 0,
                    "%s: `%s` exited %d -- the citation no longer finds the "
                    "construct%s" % (r["addr"], cmd, p.returncode,
                                     "\n" + p.stderr if p.stderr else ""))
                self.assertTrue(
                    p.stdout.strip(),
                    "%s: `%s` printed nothing" % (r["addr"], cmd))
                self.assertIn(
                    r["src"]["module"], cmd,
                    "%s: the recompute command does not name the module the "
                    "row claims (%s): `%s`"
                    % (r["addr"], r["src"]["module"], cmd))
                # the construct the row paraphrases must be in what it printed
                idents = (set(EXPR_IDENT.findall(r["src"]["expr"]))
                          - RUST_NOISE)
                self.assertTrue(
                    any(i in p.stdout for i in idents),
                    "%s: `%s` printed %d line(s) naming none of %s -- the "
                    "command runs but no longer lands on the construct"
                    % (r["addr"], cmd, len(p.stdout.splitlines()),
                       sorted(idents)))
            seen += 1
        self.assertEqual(seen, self.uncited["counts"]["implemented"])

    def test_the_port_reaches_the_routine_only_through_the_two_tokens(self):
        """The claim that makes the two token compares `implemented`.

        `1000:29fa` and `1000:2a0c` are filed against `commands::parse` rather
        than against `Game::beer`, and that is only honest if `Game::beer` is
        unreachable except through `Command::Drink` / `Command::BingeDrink`.
        Swept, not assumed.
        """
        callers = subprocess.run(
            r"grep -rn 'self\.beer(' src/", shell=True, cwd=REPO,
            capture_output=True, text=True).stdout.splitlines()
        self.assertTrue(callers)
        for ln in callers:
            self.assertRegex(
                ln, r"Command::(Drink|BingeDrink)\s*=>\s*self\.beer\(",
                "`Game::beer` is called from a site that is not a "
                "`Command::Drink` / `Command::BingeDrink` arm: %r -- the two "
                "token rows may no longer be filed against `commands::parse`"
                % ln)
        arms = subprocess.run(
            r"grep -n 'Command::Drink\|Command::BingeDrink' src/commands.rs",
            shell=True, cwd=REPO, capture_output=True, text=True).stdout
        for tok, variant in (("h", "Drink"), ("mh", "BingeDrink")):
            self.assertIn('"%s" => Command::%s' % (tok, variant), arms,
                          "commands::parse no longer maps %r to Command::%s"
                          % (tok, variant))

    def test_every_port_change_record_is_true_of_the_shipped_tree(self):
        """`data/beer_arms.json`'s `what_the_port_must_change`, RUN.

        Until the whole-branch review of this plan, no test in this suite read
        that block at all.  All four entries described the port at BASE
        `fe2e8bf`, all four were closed inside `fe2e8bf..2b3fceb`, and the
        block still asserted them in the present tense with `blocked: false`
        -- entry [1] quoting a `grep -n 'Six later' src/game.rs` that by then
        returned NOTHING.  A tracked `data/` file asserting what the tree
        falsifies is exactly what this project keeps shipping, so each entry
        now carries a `status`, a `closed_by`, and `verify` COMMANDS whose
        result is asserted here rather than quoted.

        `expect: null` means the command must print nothing; a string means it
        must occur in what the command printed.  The return code is NOT
        asserted -- `grep` exits 1 on no match, which is the expected outcome
        of every `expect: null` record.
        """
        recs = self.arms["what_the_port_must_change"]
        self.assertTrue(recs, "the ledger is empty, so this test asserts "
                              "nothing")
        for i, e in enumerate(recs):
            with self.subTest(entry=i, what=e["what"][:60]):
                self.assertIn(
                    e.get("status"), ("OPEN", "CLOSED"),
                    "entry [%d] carries no `status` -- which is how all four "
                    "of them went on describing the tree at BASE after the "
                    "range had closed them" % i)
                if e["status"] == "CLOSED":
                    closed = e.get("closed_by") or {}
                    for k in ("task", "commit", "what_changed"):
                        self.assertTrue(
                            closed.get(k),
                            "entry [%d] is CLOSED and its `closed_by` names no "
                            "%r -- a closure with nothing behind it is not a "
                            "record" % (i, k))
                checks = e.get("verify")
                self.assertTrue(
                    checks,
                    "entry [%d] carries no `verify` command, so nothing "
                    "recomputes the claim it makes about `src/`" % i)
                for c in checks:
                    cmd = c["command"]
                    out = self._run(cmd).stdout
                    if c["expect"] is None:
                        self.assertEqual(
                            out.strip(), "",
                            "entry [%d] records that `%s` prints nothing and "
                            "it printed:\n%s" % (i, cmd, out))
                    else:
                        self.assertIn(
                            c["expect"], out,
                            "entry [%d]: `%s` no longer prints %r -- it "
                            "printed %d line(s)"
                            % (i, cmd, c["expect"], len(out.splitlines())))


if __name__ == "__main__":
    unittest.main(verbosity=2)
