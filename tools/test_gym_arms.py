#!/usr/bin/env python3
"""`data/gym_arms.json` and `docs/re/gym.md` re-derived from `orig/g.exe`.

The artifact and the prose are the two places the same claims about the gym
(`trn`) and the joint (`kos`) live -- the half-open image range
`1000:e390`..`1000:ea94`; this is what stops either drifting from the binary it
describes.  Nothing here reads `src/`, a screen, or Ghidra's C.
`tools/test_den_arms.py` is the model, and the same two signals are kept apart
for the same reason (`docs/re/METHODOLOGY.md`, "Is this address a call site?"):

  * **alignment** -- the address is reached by decoding forward from its
    enclosing function's entry, so it is a real instruction boundary and not a
    byte-scan hit in the middle of one;
  * **identity** -- the instruction decoded there says what the artifact says
    it says.

The claims that are NOT restatements of a single decode are asserted by SET
EQUALITY against a sweep of the binary, never by checking that the listed
entries hold up:

  * **`strings[]` is complete.**  Every `mov di,imm16` / `push cs` / `push di`
    in the range must appear exactly once in the artifact and vice versa, so
    "this arm prints nothing else" and "the `w` arm prints nothing at all" are
    measurements.
  * **the gate inventory is complete.**  Every CONDITIONAL branch in the range
    must be named somewhere in the artifact.  The string sweep cannot see a
    silent gate, and "an unrecognised key is silent" is a headline claim.
  * **there is no `Random` draw in range -- and the sweep that says so works.**
    A zero count proves nothing on its own, so the same signature is swept over
    the whole image and required to find the population `data/rng_trace.json`
    already knows about.  Without that second half this would be the "check
    that cannot fail" `docs/re/METHODOLOGY.md` names.
  * **`effects[]` is complete.**  Every instruction carrying an absolute-memory
    operand must fall in a WRITE or a READ bucket -- an unclassified one fails
    loudly -- and the WRITE bucket must equal the union of every recorded
    effect.  That is what makes "the `w` arm writes nothing" a measurement.
  * **the menu block and the arm block are measured against each other, not
    assumed symmetric.**  The bytes are re-sliced out of `orig/g.exe`: the row-3
    level predicate must be byte-identical across the two, the row-5 armour
    predicate must NOT be, the difference must be exactly the recorded
    `d1 e0` / `48 48 ba 0a 00 f7 e2` substitution, and the longest byte run the
    two whole spans share must be the recorded 25 at the recorded addresses.
  * **arm `1`'s parity branch skips ONE increment.**  `1000:e68d`'s rel8 is
    decoded and its target required to be `1000:e693`, with `1000:e68f` exactly
    four bytes long.  Reading that `jnz` as skipping both damage increments is
    the silent divergence this file exists to catch.
  * **the two near calls wrap.**  `1000:e7df` and `1000:e966` reach
    `1000:2526` and `1000:29c4` only modulo 64 KiB; both are re-derived with
    `re_derive.near_calls_to`, which is the helper that exists for that mistake.
  * **`globals[].named_from` cannot fabricate an instruction.**  Every
    `` `1000:xxxx <text>` `` span inside any string value is decoded, and each
    global's `xrefs` census is re-derived by running `re_query.xrefs_to` -- so
    "the only image-wide writer" is a measured set, not a phrase.
  * **the range tiles.**  The 21 spans must cover `1000:e390`..`1000:ea94` end
    to end with no gap and no overlap, so a block cannot be dropped from the
    map by being left out of every span.

    python3 tools/test_gym_arms.py
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
                       inline_spans, near_calls_to, strip_fences)

REPO = Path(__file__).resolve().parents[1]
ART = REPO / "data" / "gym_arms.json"
BRANCHES = REPO / "data" / "branches.json"
TABLES = REPO / "data" / "string_tables.json"
DOC = REPO / "docs" / "re" / "gym.md"

#: The half-open image range this map owns: the `trn` verb compare through, but
#: not including, the `i` verb compare.
LO, HI = 0xe390, 0xea94

#: The `Random` far call, `call 0f78:114b`, by its exact five bytes.
RANDOM_CALL = b"\x9a\x4b\x11\x78\x0f"

#: The Borland shortstring compare, `call 0f78:0bd8`.
STR_COMPARE = b"\x9a\xd8\x0b\x78\x0f"

#: How an instruction that WRITES an absolute-memory operand decodes, and how
#: one that only READS one does -- copied verbatim from
#: `tools/test_den_arms.py`, including the reasons: the MNEMONIC decides, never
#: operand order; `push [N]` is a read; `xchg` is in NEITHER bucket so an
#: unclassified shape fails the sweep instead of passing as a read.
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


def off_of(c):
    return int(c.split(":")[1], 16)


class GymTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.art = json.loads(ART.read_text(encoding="utf-8"))
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        cls.prog = re_query.Program()
        cls.md = DOC.read_text(encoding="utf-8")
        cls.spans = inline_spans(strip_fences(cls.md))
        cls.insns = sorted(
            [i for c, i in cls.aligned.items()
             if c.startswith("1000:") and LO <= i.off < HI],
            key=lambda i: i.off)

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

    def sl(self, lo, hi):
        return " ".join("%02x" % b for b in self.img[lo:hi])

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
        covered the range, so the count is recorded and re-derived here: a walk
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
        self.assertEqual(
            self.insns[-1].off + self.insns[-1].length, HI,
            "the run does not end exactly on %s, so the range is not a whole "
            "number of instructions" % cit(HI))

    def test_every_cited_instruction_decodes_to_what_the_artifact_says(self):
        seen = self.walk(("addr", "text"))
        self.assertGreater(
            len(seen), 150,
            "the artifact stopped carrying instruction records; a walk that "
            "finds nothing must not pass (found %d)" % len(seen))
        for node, path in seen:
            ins = self.at(node["addr"])
            self.assertEqual(
                ins.text, node["text"],
                "%s: data/gym_arms.json says %s at %s, orig/g.exe decodes %s "
                "there" % (path, node["text"], node["addr"], ins.text))

    #: An instruction claim written INSIDE a prose string, which carries
    #: neither a separate `addr` key nor a separate `text` key and so escapes
    #: every other check here.  `tools/test_den_arms.py` found three such
    #: claims wrong in review round 1; the pattern is kept identical.
    PROSE_INSN = re.compile(r"`(1000:[0-9a-f]{4})\s+([a-z][^`]*)`")

    def test_every_prose_embedded_instruction_says_what_the_binary_says(self):
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
        self.assertGreaterEqual(
            len(found), 4,
            "the prose-embedded instruction sweep matched only %d spans; a "
            "scan that measures nothing must not pass" % len(found))
        # And prove the regex can still see a claim of the shape it hunts, so
        # "no matches" can never be mistaken for "no defects".
        self.assertEqual(
            self.PROSE_INSN.findall("x `1000:e68f inc [0x38a8]` y"),
            [("1000:e68f", "inc [0x38a8]")],
            "the prose-embedded pattern no longer matches the claim shape it "
            "hunts")
        for path, c, text in found:
            ins = self.at(c)
            self.assertEqual(
                ins.text, text,
                "%s: data/gym_arms.json writes `%s %s` inside a prose string, "
                "but orig/g.exe decodes %r there" % (path, c, text, ins.text))

    def test_every_address_the_artifact_names_is_a_boundary(self):
        exempt = {e["addr"]
                  for e in self.art["known_not_boundaries"]["entries"]}
        cits = sorted(self.all_addresses() - exempt)
        self.assertGreaterEqual(
            len(cits), 180,
            "the artifact names only %d distinct 1000: addresses; a scan that "
            "measures nothing must not pass" % len(cits))
        for c in cits:
            self.at(c)

    def test_each_exemption_names_the_instruction_that_covers_it(self):
        """The byte role in an exemption's prose is re-derived, not authored.

        Fix round 1 found `1000:e594` described as "the last byte of the row-5
        colour `jl`" when it is the OPCODE byte -- the conclusion beside it was
        right and the sentence was not, and no check here reached it because
        `why` is free prose with no `addr`/`text` pair inside it. So each entry
        now carries `inside` (the covering instruction) and `byte_index`, and
        both are decoded: the covering instruction must be a real boundary, it
        must actually cover the exempt address, and the index must be the
        offset within it. That turns "one byte into a two-byte `jcc`" from a
        sentence into a measurement.
        """
        entries = self.art["known_not_boundaries"]["entries"]
        self.assertGreaterEqual(len(entries), 1)
        for e in entries:
            host = self.at(e["inside"])
            lo, hi = host.off, host.off + host.length
            self.assertTrue(
                lo < off_of(e["addr"]) < hi,
                "%s: the artifact says it falls inside %s (%s, %d bytes), but "
                "that instruction spans %s..%s"
                % (e["addr"], e["inside"], host.text, host.length,
                   cit(lo), cit(hi)))
            self.assertEqual(
                off_of(e["addr"]) - lo, e["byte_index"],
                "%s: the artifact records byte_index %d inside %s; it is at "
                "byte %d" % (e["addr"], e["byte_index"], e["inside"],
                             off_of(e["addr"]) - lo))
            # And the instruction AFTER the host is a boundary, which is the
            # half of the old sentence that was true and is worth keeping.
            self.assertIn(
                cit(hi), self.aligned,
                "%s: %s is not a boundary, so %s cannot be interior to a "
                "single instruction" % (e["addr"], cit(hi), e["addr"]))

    def test_the_boundary_exemption_list_is_honest(self):
        """An exemption that names a real boundary would hide a wrong address.

        So each entry must actually be OUTSIDE the aligned set -- the
        exemption list cannot be used to smuggle one in.
        """
        entries = self.art["known_not_boundaries"]["entries"]
        self.assertGreaterEqual(len(entries), 1)
        for e in entries:
            self.assertNotIn(
                e["addr"], self.aligned,
                "%s is exempted from the boundary walk but IS a boundary; the "
                "exemption is either stale or covering for a wrong address"
                % e["addr"])

    # -------------------------------------------------------------- literals
    def test_every_literal_decodes_to_the_recorded_text(self):
        seen = self.walk(("cs_offset", "file_offset", "text"))
        self.assertGreaterEqual(
            len(seen), 40,
            "only %d literal records; the artifact lost its strings" % len(seen))
        for node, path in seen:
            cs = int(node["cs_offset"], 16)
            self.assertEqual(
                int(node["file_offset"], 16), cs + addrmod.HEADER_BYTES,
                "%s: file_offset is not cs_offset + the MZ header size"
                % path)
            self.assertEqual(
                self.cs_literal(cs), node["text"],
                "%s: the Pascal shortstring at CS %s is %r, the artifact "
                "records %r" % (path, node["cs_offset"],
                                self.cs_literal(cs), node["text"]))
            push = node["push"]
            self.assertEqual(
                self.at(push["addr"]).text, "mov di,%s" % node["cs_offset"],
                "%s: the push at %s does not load %s"
                % (path, push["addr"], node["cs_offset"]))

    def test_the_recorded_strings_are_every_cs_literal_the_handler_pushes(self):
        """SET EQUALITY, both directions.

        Swept: every `mov di,imm16` followed by `push cs` / `push di` in the
        range.  Recorded: every literal record in the artifact whose push is in
        range.  "The `w` arm prints nothing" is this measurement, not a note.
        """
        swept = set()
        for k, i in enumerate(self.insns):
            if (i.text.startswith("mov di,0x") and k + 2 < len(self.insns)
                    and self.insns[k + 1].text == "push cs"
                    and self.insns[k + 2].text == "push di"):
                swept.add(cit(i.off))
        recorded = {n["push"]["addr"]
                    for n, _ in self.walk(("cs_offset", "text"))
                    if LO <= off_of(n["push"]["addr"]) < HI}
        self.assertEqual(
            swept, recorded,
            "the CS-literal push sweep and the artifact disagree: only swept "
            "%s, only recorded %s"
            % (sorted(swept - recorded), sorted(recorded - swept)))
        self.assertEqual(
            len(swept), self.art["sweeps"]["cs_literal_pushes"],
            "sweeps.cs_literal_pushes says %d, the sweep finds %d"
            % (self.art["sweeps"]["cs_literal_pushes"], len(swept)))

    # ---------------------------------------------------------------- gates
    def test_the_recorded_gates_are_every_conditional_branch_in_range(self):
        """No silent gate.

        The string sweep cannot see a branch that prints nothing, and "an
        unrecognised key is silent" and "at district 1 the key is not even
        compared" both depend on the branch inventory being whole.
        """
        swept = {cit(i.off) for i in self.insns
                 if re.match(r"^j(?!mp)", i.text)}
        named = self.all_addresses()
        missing = sorted(swept - named)
        self.assertEqual(
            missing, [],
            "conditional branches in %s..%s that the artifact never names: %s"
            % (cit(LO), cit(HI), missing))
        self.assertEqual(
            len(swept), self.art["sweeps"]["conditional_branches"],
            "sweeps.conditional_branches says %d, the sweep finds %d"
            % (self.art["sweeps"]["conditional_branches"], len(swept)))

    def test_the_branch_census_reproduces_data_branches_json(self):
        cen = self.art["branch_census"]
        recs = [b for b in self.branches["branches"]]
        for key, (lo, hi) in (("trn", (0xe390, 0xe972)),
                              ("kos", (0xe973, 0xea93))):
            rng = [b for b in recs if lo <= off_of(b["addr"]) <= hi]
            self.assertEqual(
                len(rng), cen[key]["branches"],
                "%s: data/branches.json holds %d branches in %s..%s, the "
                "artifact records %d"
                % (key, len(rng), cit(lo), cit(hi), cen[key]["branches"]))
            untouched = sum(1 for b in rng if not b["port_touched"])
            self.assertEqual(
                untouched, cen[key]["port_touched_false"],
                "%s: %d branches have port_touched false, the artifact "
                "records %d" % (key, untouched, cen[key]["port_touched_false"]))

    # ---------------------------------------------------------------- draws
    def test_there_is_no_random_call_in_range_and_the_sweep_can_find_one(self):
        """A zero count is not evidence until the sweep is shown to work.

        Half of this test is the negative -- no `Random` site in the gym or the
        joint.  The other half sweeps the SAME signature over the whole image
        and requires it to find the population the project already knows
        about, so "found none" cannot mean "searched nothing".
        """
        in_range = [cit(i.off) for i in self.insns
                    if i.raw[:5] == RANDOM_CALL]
        self.assertEqual(
            in_range, [],
            "the artifact claims no Random draw in %s..%s, the sweep finds %s"
            % (cit(LO), cit(HI), in_range))
        self.assertEqual(self.art["sweeps"]["random_call_sites"], 0)
        image_wide = self.img.count(RANDOM_CALL)
        self.assertGreaterEqual(
            image_wide, 80,
            "the Random-call sweep finds only %d sites in the whole image; "
            "docs/re/METHODOLOGY.md records 86 far calls, so a sweep that "
            "cannot find them proves nothing about the gym" % image_wide)

    # -------------------------------------------------------------- effects
    def test_the_recorded_effects_are_every_absolute_write_in_range(self):
        writes, reads, unclassified = set(), set(), []
        for i in self.insns:
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
            "instructions with an absolute-memory operand that neither bucket "
            "describes -- an unclassified shape must fail loudly rather than "
            "pass as a read: %s" % unclassified)
        recorded = set()
        for a in self.arms():
            recorded |= {e["addr"] for e in a["effects"]}
        recorded |= {e["addr"] for e in self.art["joint"]["effects"]}
        recorded |= {s["effect"]["addr"]
                     for s in self.art["abs_recompute"]["steps"]}
        recorded.add(self.art["abs_recompute"]["seed"][1]["addr"])
        for m in self.art["menu_lines"]:
            recorded.add(m["colour_digit"]["affordable_store"]["addr"])
            recorded.add(m["colour_digit"]["unaffordable_store"]["addr"])
        self.assertEqual(
            writes, recorded,
            "the absolute-write sweep and the artifact disagree: only swept "
            "%s, only recorded %s"
            % (sorted(writes - recorded), sorted(recorded - writes)))
        self.assertEqual(
            len(writes), self.art["sweeps"]["absolute_memory_writes"],
            "sweeps.absolute_memory_writes says %d, the sweep finds %d"
            % (self.art["sweeps"]["absolute_memory_writes"], len(writes)))

    def test_the_w_arm_writes_and_prints_nothing(self):
        """The `w` arm's two negatives, measured over its own span."""
        arm = next(a for a in self.arms() if a["key"] == "w")
        lo = off_of(arm["span"]["start"])
        hi = off_of(arm["span"]["end"])
        body = [i for i in self.insns if lo <= i.off < hi]
        self.assertGreater(len(body), 4, "the `w` arm's span decoded to %d "
                                         "instructions" % len(body))
        self.assertEqual(
            [cit(i.off) for i in body if WRITES_ABS_MEM.match(i.text)], [],
            "the `w` arm is recorded as writing nothing")
        self.assertEqual(
            [cit(i.off) for i in body
             if i.text in ("call 0xeed:0x1c2", "call 0xeed:0x0")], [],
            "the `w` arm is recorded as printing nothing")
        self.assertEqual(arm["effects"], [])
        self.assertEqual(arm["prints"], [])

    # -------------------------------------------------------------- globals
    def test_the_dgroup_addresses_touched_are_the_recorded_globals(self):
        swept = {"20ae:" + m for i in self.insns if "[0x" in i.text
                 for m in re.findall(r"\[0x([0-9a-f]+)\]", i.text)}
        recorded = {g["ds"] for g in self.art["globals"]}
        self.assertEqual(
            swept, recorded,
            "the DGROUP-operand sweep and globals[] disagree: only swept %s, "
            "only recorded %s"
            % (sorted(swept - recorded), sorted(recorded - swept)))
        self.assertEqual(
            len(swept), self.art["sweeps"]["dgroup_addresses_touched"])

    def test_each_globals_write_list_is_its_writes_in_range(self):
        for g in self.art["globals"]:
            key = "[0x%s]" % g["ds"].split(":")[1]
            swept = [cit(i.off) for i in self.insns
                     if WRITES_ABS_MEM.match(i.text) and key in i.text]
            self.assertEqual(
                swept, g["written_in_range"],
                "%s: the sweep finds %s written in range, the artifact "
                "records %s" % (g["ds"], swept, g["written_in_range"]))
            reads = len([i for i in self.insns
                         if key in i.text and not WRITES_ABS_MEM.match(i.text)])
            self.assertEqual(
                reads, g["read_sites_in_range"],
                "%s: %d read sites in range, the artifact records %d"
                % (g["ds"], reads, g["read_sites_in_range"]))

    def test_every_globals_xref_census_is_what_re_query_reports(self):
        """`named_from` and "the only writer" are re-derived, not trusted."""
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
            self.assertEqual(
                xr["writers_image_wide"], writers,
                "%s: the artifact lists %s as its image-wide writers, "
                "`xrefs-to` finds %s -- an 'only writer' claim that stopped "
                "the next search is exactly what this check exists for"
                % (g["ds"], xr["writers_image_wide"], writers))
            for e in g["evidence"]:
                self.assertEqual(
                    self.at(e["addr"]).text, e["text"],
                    "%s: evidence at %s says %r, orig/g.exe decodes %r"
                    % (g["ds"], e["addr"], e["text"],
                       self.at(e["addr"]).text))

    def test_the_trained_armour_scratch_is_gym_local(self):
        """Every reference to 20ae:3e34 in the IMAGE is inside the gym.

        `docs/re/gaps.md` said "exactly one thing reads the result".  There are
        two readers with two different thresholds, which is the correction this
        map carries; the containment claim is measured here so it cannot go
        the same way.
        """
        scan = re_query.xrefs_to(self.prog, "20ae:3e34")["scan"]
        offs = sorted(off_of(a["at"]) for a in scan["accepted"])
        self.assertTrue(offs, "the 20ae:3e34 census is empty")
        self.assertGreaterEqual(offs[0], 0xe3a7)
        self.assertLessEqual(offs[-1], 0xe8da)
        readers = [a["at"] for a in scan["accepted"]
                   if not WRITES_ABS_MEM.match(a["text"])]
        self.assertEqual(
            readers, self.art["abs_recompute_finding"]["readers"],
            "20ae:3e34 is read at %s, the artifact records %s"
            % (readers, self.art["abs_recompute_finding"]["readers"]))

    def test_the_broken_jaw_byte_only_ever_holds_0_or_1(self):
        """The fourth `kos` representation difference, made executable.

        `1000:e97d` runs the arm iff `20ae:38b0` is **not 1**; `Game::smoke`
        refuses iff a bool is **true**. The two agree only while the byte holds
        nothing but 0 and 1, which is the same shape of argument the `u16
        joints` entry makes. So every image-wide absolute write to the byte is
        re-derived and its stored immediate required to be 0 or 1 -- a writer
        storing 2 would make the original RUN the arm where the port refuses
        it, and that is what this check would catch.
        """
        eq = [e for e in self.art["joint"]["port_equivalences"]
              if e["original"].startswith("1000:e97d")]
        self.assertEqual(len(eq), 1,
                         "the jaw gate is recorded as a port equivalence")
        scan = re_query.xrefs_to(self.prog, "20ae:38b0")["scan"]
        writers = [a for a in scan["accepted"] if WRITES_ABS_MEM.match(a["text"])]
        self.assertGreaterEqual(
            len(writers), 5,
            "only %d writers of 20ae:38b0; a sweep that finds nothing cannot "
            "support a 'the byte only ever holds 0 or 1' claim" % len(writers))
        for w in writers:
            m = re.match(r"^mov byte \[0x38b0\],0x([0-9a-f]+)$", w["text"])
            self.assertIsNotNone(
                m, "%s writes 20ae:38b0 as %r -- not an immediate store, so "
                   "the value it leaves is not bounded by this check"
                   % (w["at"], w["text"]))
            assert m is not None
            self.assertIn(
                int(m.group(1), 16), (0, 1),
                "%s stores %s into 20ae:38b0; the jaw equivalence holds only "
                "for 0 and 1" % (w["at"], m.group(1)))
        # And the writer set itself, so the census cannot silently shrink.
        self.assertEqual(
            [w["at"] for w in writers],
            ["1000:47ee", "1000:4820", "1000:5031", "1000:b2ae", "1000:d558"],
            "the image-wide writer set of 20ae:38b0 changed")

    def test_the_tooth_guard_has_exactly_one_absolute_write(self):
        scan = re_query.xrefs_to(self.prog, "20ae:394a")["scan"]
        writers = [a["at"] for a in scan["accepted"]
                   if WRITES_ABS_MEM.match(a["text"])]
        self.assertEqual(
            writers, ["1000:e828"],
            "the `4` arm is recorded as the only absolute-memory writer of "
            "20ae:394a; `xrefs-to` finds %s" % writers)

    def test_the_ds_pointer_pushes_are_complete(self):
        swept = {}
        for k, i in enumerate(self.insns):
            if (i.text.startswith("mov di,0x") and k + 2 < len(self.insns)
                    and self.insns[k + 1].text == "push ds"
                    and self.insns[k + 2].text == "push di"):
                swept.setdefault(
                    "20ae:" + i.text.split(",0x")[1], []).append(cit(i.off))
        recorded = {e["ds"]: e["pushed_at"]
                    for e in self.art["globals_not_memory_operands"]["entries"]}
        # The `trn` verb's own push is before the range start and is recorded
        # under verb.push_buffer, so drop it from the comparison exactly the
        # way the artifact's own note says it is dropped.
        recorded["20ae:3972"] = [a for a in recorded["20ae:3972"]
                                 if LO <= off_of(a) < HI]
        self.assertEqual(
            swept, recorded,
            "the DS-pointer push sweep and the artifact disagree: swept %s, "
            "recorded %s" % (swept, recorded))
        self.assertEqual(
            sum(len(v) for v in swept.values()),
            self.art["sweeps"]["ds_pointer_pushes"])

    # ------------------------------------------------------------ boundaries
    def test_the_range_boundaries_are_the_two_verb_compares(self):
        for node, key in ((self.art["verb"], "trn"),
                          (self.art["bounded_on_the_right_by"], "i")):
            c = node["compare_addr"]
            ins = self.at(c)
            self.assertEqual(
                ins.raw[:5], STR_COMPARE,
                "%s: %s is recorded as the %r verb compare but decodes %r"
                % (c, c, key, ins.text))
            self.assertEqual(node["key_literal"]["text"], key)
        self.assertEqual(off_of(self.art["verb"]["compare_addr"]), LO)
        self.assertEqual(
            off_of(self.art["bounded_on_the_right_by"]["compare_addr"]), HI)
        self.assertEqual(self.art["joint"]["key_literal"]["text"], "kos")
        self.assertEqual(
            self.at(self.art["joint"]["compare_addr"]).raw[:5], STR_COMPARE)
        # `data/command_dispatch.json` is the independent authority on which
        # verb each compare belongs to; agreeing with it is the point.
        chain = json.loads(
            (REPO / "data" / "command_dispatch.json").read_text(
                encoding="utf-8"))["confirmed_dispatch_chain"]
        by_verb = {e["verb"]: e for e in chain}
        self.assertEqual(by_verb["trn"]["compare_addr"],
                         self.art["verb"]["compare_addr"])
        self.assertEqual(by_verb["kos"]["compare_addr"],
                         self.art["joint"]["compare_addr"])
        self.assertEqual(
            by_verb["i"]["compare_addr"],
            self.art["bounded_on_the_right_by"]["compare_addr"])

    def test_each_key_compare_is_the_shortstring_compare_on_the_gym_buffer(self):
        for a in self.arms():
            ins = self.at(a["compare_addr"])
            self.assertEqual(
                ins.raw[:5], STR_COMPARE,
                "arm %s: %s is not the shortstring compare"
                % (a["key"], a["compare_addr"]))
            self.assertEqual(
                self.at(a["push_buffer"]["addr"]).text, "mov di,0x3a72",
                "arm %s: the compare does not read the gym's own buffer"
                % a["key"])
            self.assertEqual(self.cs_literal(a["key_literal"]["cs_offset"]),
                             a["key"])
        self.assertEqual([a["key"] for a in self.arms()], self.art["key_set"])
        # And the whole population: eight shortstring compares in range -- six
        # keys plus the two verb compares that bound the handlers.
        swept = [cit(i.off) for i in self.insns if i.raw[:5] == STR_COMPARE]
        self.assertEqual(len(swept), self.art["sweeps"]["shortstring_compares"])
        self.assertEqual(
            set(swept),
            {a["compare_addr"] for a in self.arms()}
            | {self.art["verb"]["compare_addr"],
               self.art["joint"]["compare_addr"]})

    def test_the_district_gates_skip_the_key_compare(self):
        """"At district 1 the key is not even compared" is a flow claim.

        It holds only if the district gate's failure target is PAST the arm's
        own compare, so `on_fail` is DECODED rather than read out of the
        artifact and compared with other artifact fields. Two shapes exist and
        the test requires each arm to match exactly one, naming which:

        * arm `4` -- the recorded branch IS the failure direction
          (`1000:e7e7 jbe 0xe861`), so its own rel8 target is `on_fail`;
        * arms `3` and `5` -- the recorded branch is the PASS direction
          (`1000:e72d ja 0xe732`, `1000:e866 ja 0xe86b`) and the failure is the
          `jmp` sitting at its fallthrough (`1000:e72f`, `1000:e868`). Fix
          round 1 found that `jmp` decoded by nothing: `on_fail` was correct
          and unverified, which is a strength-of-evidence hole of exactly the
          kind `docs/re/METHODOLOGY.md` names.
        """
        checked, shapes = 0, {}
        for a in self.arms():
            if not a["own_gate"]:
                continue
            checked += 1
            fail = off_of(a["own_gate"]["on_fail"])
            br = self.at(a["own_gate"]["branch"]["addr"])
            self.assertEqual(
                br.text, a["own_gate"]["branch"]["text"],
                "arm %s: the gate branch does not decode as recorded" % a["key"])
            branch_target = int(br.text.split()[-1], 16) & 0xFFFF
            if branch_target == fail:
                shapes[a["key"]] = "branch is the failure direction"
            else:
                after = self.at(cit(br.off + br.length))
                self.assertTrue(
                    after.text.startswith("jmp"),
                    "arm %s: the gate branch at %s is the PASS direction "
                    "(target %s), so its fallthrough %s must be the failure "
                    "jump; it decodes %r"
                    % (a["key"], a["own_gate"]["branch"]["addr"],
                       cit(branch_target), cit(after.off), after.text))
                jmp_target = int(after.text.split()[-1], 16) & 0xFFFF
                self.assertEqual(
                    cit(jmp_target), a["own_gate"]["on_fail"],
                    "arm %s: the failure jump at %s targets %s, the artifact "
                    "records on_fail %s"
                    % (a["key"], cit(after.off), cit(jmp_target),
                       a["own_gate"]["on_fail"]))
                shapes[a["key"]] = ("branch is the pass direction, failure "
                                    "jump at " + cit(after.off))
            self.assertGreater(
                fail, off_of(a["compare_addr"]),
                "arm %s: the district gate's failure target %s is BEFORE the "
                "key compare %s, so the key would still be compared"
                % (a["key"], a["own_gate"]["on_fail"], a["compare_addr"]))
            self.assertEqual(
                fail, off_of(a["span"]["end"]),
                "arm %s: the district gate does not skip the whole arm"
                % a["key"])
        self.assertEqual(checked, 3,
                         "three arms are recorded with a district gate of "
                         "their own, found %d" % checked)
        # Both shapes must actually occur, or one of the two branches of the
        # check above has never run and the test is narrower than it reads.
        self.assertEqual(
            sorted({s.split(",")[0] for s in shapes.values()}),
            ["branch is the failure direction",
             "branch is the pass direction"],
            "only one gate shape occurs (%s); half of this check is dead"
            % shapes)

    def test_the_spans_tile_the_range(self):
        spans = self.art["spans"]
        cursor = LO
        for s in spans:
            self.assertEqual(
                off_of(s["start"]), cursor,
                "span %r starts at %s, the previous one ended at %s -- the "
                "tiling has a %s" % (s["name"], s["start"], cit(cursor),
                                     "gap" if off_of(s["start"]) > cursor
                                     else "overlap"))
            self.at(s["start"])
            cursor = off_of(s["end"])
        self.assertEqual(cursor, HI,
                         "the spans stop at %s, the range ends at %s"
                         % (cit(cursor), cit(HI)))
        for a in self.arms():
            self.assertIn((a["span"]["start"], a["span"]["end"]),
                          [(s["start"], s["end"]) for s in spans],
                          "arm %s's span is not one of the tiling's spans"
                          % a["key"])

    def test_the_loop_back_edge_returns_to_the_prompt(self):
        back = self.at(self.art["loop"]["back_edge"]["addr"])
        target = (int(back.text.split()[-1], 16)) & 0xFFFF
        self.assertEqual(
            cit(target), self.art["loop"]["top"],
            "the back edge at %s targets %s, the artifact records the loop "
            "top as %s" % (self.art["loop"]["back_edge"]["addr"], cit(target),
                           self.art["loop"]["top"]))
        # The loop top is the PROMPT, not the menu: the first menu line is
        # strictly before it, so nothing reprints the menu.
        self.assertGreater(off_of(self.art["loop"]["top"]),
                           off_of(self.art["menu_lines"][-1]["prints"][0]["addr"]))
        exit_edge = self.at(self.art["loop"]["exit_edge"]["addr"])
        exit_target = (int(exit_edge.text.split()[-1], 16)) & 0xFFFF
        self.assertEqual(cit(exit_target),
                         self.art["shared_tail"]["span"]["start"])

    # ------------------------------------------------- menu against the arms
    def test_the_menu_and_arm_predicates_differ_exactly_as_recorded(self):
        f = self.art["menu_vs_arm_finding"]
        for p in f["pairs"]:
            m = self.sl(off_of(p["menu"]["start"]), off_of(p["menu"]["end"]))
            a = self.sl(off_of(p["arm"]["start"]), off_of(p["arm"]["end"]))
            self.assertEqual(m, p["menu_bytes"], "%s: menu bytes" % p["what"])
            self.assertEqual(a, p["arm_bytes"], "%s: arm bytes" % p["what"])
            self.assertEqual(
                m == a, p["identical"],
                "%s: the artifact records identical=%s, the bytes say %s"
                % (p["what"], p["identical"], m == a))
            self.assertEqual(
                self.sl(off_of(p["menu"]["end"]), off_of(p["menu"]["end"]) + 2),
                p["jcc_menu"])
            self.assertEqual(
                self.sl(off_of(p["arm"]["end"]), off_of(p["arm"]["end"]) + 2),
                p["jcc_arm"])
        # The row-5 substitution, spelled out: identical head, identical tail,
        # `shl ax,1` against `dec/dec/mov dx,10/mul dx` in between.
        row5 = next(p for p in f["pairs"] if "armour" in p["what"])
        mb = self.img[off_of(row5["menu"]["start"]):off_of(row5["menu"]["end"])]
        ab = self.img[off_of(row5["arm"]["start"]):off_of(row5["arm"]["end"])]
        self.assertEqual(mb[:5], ab[:5], "row 5: the heads differ")
        self.assertEqual(mb[-9:], ab[-9:], "row 5: the tails differ")
        self.assertEqual(mb[5:-9], bytes.fromhex("d1e0"),
                         "row 5: the menu's middle is not `shl ax,1`")
        self.assertEqual(mb[5:-9].hex(" "), "d1 e0")
        self.assertEqual(ab[5:-9].hex(" "), "48 48 ba 0a 00 f7 e2",
                         "row 5: the arm's middle is not "
                         "`dec ax`/`dec ax`/`mov dx,0xa`/`mul dx`")
        for s in f["shared_constants"]:
            mb = self.sl(off_of(s["menu"]), off_of(s["menu"]) + 5)
            ab = self.sl(off_of(s["arm"]), off_of(s["arm"]) + 5)
            self.assertEqual(mb, s["bytes"], "%s: menu bytes" % s["what"])
            self.assertEqual(
                (mb == ab), s["identical"],
                "%s: the artifact records identical=%s, the bytes say %s"
                % (s["what"], s["identical"], mb == ab))

    def test_the_longest_run_the_two_blocks_share_is_the_recorded_one(self):
        """The measurement that answers "is the second block a copy?".

        Recomputed here rather than remembered: a longest-common-substring over
        the two spans.  If it grew, the two blocks really do share a body and
        the finding is wrong.
        """
        f = self.art["menu_vs_arm_finding"]
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
            "the two blocks share a %d-byte run; the artifact records %d"
            % (best[0], rec["length"]))
        self.assertEqual(cit(off_of(f["menu_span"]["start"]) + best[1]),
                         rec["menu_at"])
        self.assertEqual(cit(off_of(f["arm_span"]["start"]) + best[2]),
                         rec["arm_at"])
        self.assertEqual(X[best[1]:best[1] + best[0]].hex(" "), rec["bytes"])

    def test_the_menu_price_test_never_hides_a_row(self):
        """Both colour arms reconverge; only 20ae:3b7a differs between them.

        This is what makes "in the menu the price test is cosmetic" a
        measurement.  The `'0'` store is followed by a `jmp short` whose target
        is the instruction right after the `'4'` store, so the two paths join
        before anything else happens.
        """
        for m in self.art["menu_lines"]:
            lo = self.at(m["colour_digit"]["affordable_store"]["addr"])
            hi = self.at(m["colour_digit"]["unaffordable_store"]["addr"])
            join = self.insns[[i.off for i in self.insns].index(hi.off) + 1]
            after_lo = self.insns[[i.off for i in self.insns].index(lo.off) + 1]
            self.assertTrue(
                after_lo.text.startswith("jmp"),
                "menu row %s: the affordable store is not followed by a jump"
                % m["key"])
            self.assertEqual(
                (int(after_lo.text.split()[-1], 16)) & 0xFFFF, join.off,
                "menu row %s: the two colour arms do not reconverge on %s"
                % (m["key"], cit(join.off)))
            self.assertIn("[0x3b7a]", lo.text)
            self.assertIn("[0x3b7a]", hi.text)

    # ------------------------------------------------------------ arm detail
    def test_arm_1s_parity_branch_skips_only_the_dmg_min_increment(self):
        """The one thing in this range that is easy to get backwards.

        `1000:e68d jnz` is a two-byte rel8.  Its target must be `1000:e693`
        (`inc [0x38aa]`), NOT past it, and `1000:e68f inc [0x38a8]` must be
        exactly the four bytes the displacement skips.  A port that treats both
        increments as conditional passes every screen check and is wrong.
        """
        arm = next(a for a in self.arms() if a["key"] == "1")
        br = self.at("1000:e68d")
        self.assertEqual(br.raw[0], 0x75, "1000:e68d is not a `jnz`")
        rel = br.raw[1] - 256 if br.raw[1] > 127 else br.raw[1]
        target = 0xe68d + 2 + rel
        self.assertEqual(cit(target), "1000:e693",
                         "the parity jnz targets %s" % cit(target))
        skipped = self.at("1000:e68f")
        self.assertEqual(skipped.text, "inc [0x38a8]")
        self.assertEqual(skipped.length, 4)
        self.assertEqual(self.at("1000:e693").text, "inc [0x38aa]")
        conditional = [e["addr"] for e in arm["effects"] if e["condition"]]
        self.assertEqual(
            conditional, ["1000:e68f"],
            "arm 1 must record exactly one CONDITIONAL effect, the dmg_min "
            "increment; it records %s" % conditional)
        self.assertIsNone(
            next(e for e in arm["effects"] if e["addr"] == "1000:e693")
            ["condition"],
            "arm 1's dmg_max increment is unconditional")

    def test_the_two_near_calls_wrap_to_the_targets_the_artifact_names(self):
        arm3 = next(a for a in self.arms() if a["key"] == "3")
        call = arm3["calls_out"][0]
        self.assertIn(call["addr"], near_calls_to(self.img, 0x2526),
                      "%s does not near-call 1000:2526 modulo 64 KiB"
                      % call["addr"])
        self.assertEqual(
            self.at(call["param"]["set_at"]).text,
            "mov al,0x%x" % call["param"]["value"])
        self.assertEqual(self.at(call["param"]["push_at"]).text, "push ax")
        tail = self.art["shared_tail"]
        self.assertIn(tail["call"]["addr"], near_calls_to(self.img, 0x29c4))
        self.assertEqual(near_calls_to(self.img, 0x29c4),
                         ["1000:4b00", tail["call"]["addr"]],
                         "1000:29c4 is recorded as having two callers")

    def test_the_level_up_outer_guard_duplicates_the_callee_entry_test(self):
        """Byte-for-byte, so "redundant" is measured and not asserted."""
        gym = self.sl(0xe7d3, 0xe7da)
        callee = self.sl(0x2535, 0x253c)
        self.assertEqual(
            gym, callee,
            "the gym's outer level-up guard %s and 1000:2526's own entry test "
            "%s are recorded as identical" % (gym, callee))
        self.assertEqual(self.at("1000:e7da").text[:2], "jl")
        self.assertEqual(self.at("1000:253c").text[:3], "jnl")

    # --------------------------------------------------------------- the RTL
    def test_the_string_building_call_counts_match_five_menu_rows(self):
        s = self.art["sweeps"]
        for key, text in (("str_assign_calls", "call 0xf78:0xae7"),
                          ("char_to_str_calls", "call 0xf78:0xc03"),
                          ("str_append_calls", "call 0xf78:0xb66"),
                          ("writeln_calls", "call 0xeed:0x1c2"),
                          ("write_calls", "call 0xeed:0x0")):
            n = sum(1 for i in self.insns if i.text == text)
            self.assertEqual(n, s[key],
                             "sweeps.%s says %d, the sweep finds %d"
                             % (key, s[key], n))
        self.assertEqual(s["str_assign_calls"], len(self.art["menu_lines"]))
        self.assertEqual(s["char_to_str_calls"], len(self.art["menu_lines"]))
        self.assertEqual(s["str_append_calls"], 2 * len(self.art["menu_lines"]))

    def test_the_case_fold_lowercases_and_does_not_trim(self):
        """`0eed:0216` is the gym's whole input normalisation.

        "Keys are case-insensitive and whitespace-SENSITIVE" is a claim about
        this routine, so the routine is decoded rather than described: it must
        contain the A..Z range compares and the `add ax,0x20`, and it must
        contain no compare against 0x20 (space) at all.

        **The negative is scoped to the ROUTINE, not to a round number.** The
        first revision decoded a fixed `start + 0x60` window and asserted the
        absence over it; the routine runs to a `retf 0x4` at `+0x72`, so 0x13
        bytes of it sat outside the window and "contains no compare against
        0x20" was asserted over 83% of what it named. The walk now stops ON the
        `retf` and the test asserts it reached one, so a window that ends early
        fails instead of quietly narrowing the claim.
        """
        self.assertEqual(self.at(self.art["input_read"]["case_fold"]["addr"])
                         .text, "call 0xeed:0x216")
        start = addrmod.image_off_of_citation("1eed:0216")
        body = []
        for ins in dis16.decode_run(self.img, start, start + 0x100):
            body.append(ins)
            if ins.text.startswith("retf") or ins.text.startswith("ret"):
                break
        texts = [i.text for i in body]
        self.assertTrue(
            texts and texts[-1].startswith("ret"),
            "the walk over 0eed:0216 never reached a return, so the negative "
            "below would be scoped to an arbitrary window: %s" % texts)
        self.assertEqual(
            texts[-1], "retf 0x4",
            "0eed:0216 is expected to end in `retf 0x4` (it takes one far "
            "string pointer); it ends in %r" % texts[-1])
        self.assertIn("cmp byte [es:di],0x41", texts,
                      "0eed:0216 has no 'A' bound: %s" % texts)
        self.assertIn("cmp byte [es:di],0x5a", texts,
                      "0eed:0216 has no 'Z' bound: %s" % texts)
        self.assertTrue(any(t == "add ax,0x20" for t in texts),
                        "0eed:0216 never adds 0x20: %s" % texts)
        self.assertEqual(
            [t for t in texts if re.search(r"cmp .*,0x20$", t)], [],
            "0eed:0216 compares against 0x20; the no-trim claim would be "
            "wrong: %s" % texts)

    # ------------------------------------------------------------- the prose
    def test_every_prose_address_is_an_instruction_boundary(self):
        exempt = {e["addr"]
                  for e in self.art["known_not_boundaries"]["entries"]}
        cits = sorted(set(CITE.findall(strip_fences(self.md))) - exempt)
        self.assertGreaterEqual(
            len(cits), 80,
            "docs/re/gym.md names only %d distinct 1000: addresses; a prose "
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
                self.at(c).text, text,
                "docs/re/gym.md writes `%s %s`, but tools/dis16.py decodes "
                "%r there" % (c, text, self.at(c).text))
        self.assertGreaterEqual(
            checked, 15,
            "only %d `addr text` spans in docs/re/gym.md" % checked)

    def test_every_instruction_inside_a_fence_says_what_the_binary_says(self):
        checked = 0
        for block in re.findall(r"^```.*?\n(.*?)^```", self.md, re.S | re.M):
            for line in block.splitlines():
                m = re.match(r"^(1000:[0-9a-f]{4})\s+([a-z][^;]*?)\s*(;.*)?$",
                             line)
                if not m:
                    continue
                c, text = m.group(1), m.group(2).strip()
                self.assertIn(c, self.aligned, "%r: not a boundary" % line)
                checked += 1
                self.assertEqual(
                    self.aligned[c].text, text,
                    "docs/re/gym.md writes `%s %s` in a fence, but "
                    "tools/dis16.py decodes %r there"
                    % (c, text, self.aligned[c].text))
        self.assertGreaterEqual(
            checked, 25,
            "only %d fenced instruction lines in docs/re/gym.md" % checked)

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
        for text, o in pairs:
            self.assertEqual(
                self.cs_literal(int(o, 16)), text,
                "the prose quotes %r beside CS 0x%s, which holds %r"
                % (text, o, self.cs_literal(int(o, 16))))
        known = {self.cs_literal(n["cs_offset"])
                 for n, _ in self.walk(("cs_offset", "text"))}
        tables = json.loads(TABLES.read_text(encoding="utf-8"))
        known |= {e["text"] for t in tables["tables"] for e in t["entries"]}
        known |= {self.cs_literal(o) for o in offs}
        unmatched = sorted({run for span in self.spans
                            for run in re.findall(r"[Ѐ-ӿ]+", span)
                            if not any(run in k for k in known)})
        self.assertEqual(
            unmatched, [],
            "Russian in docs/re/gym.md that matches no literal in orig/g.exe "
            "at any address the doc or the artifact names: %r" % unmatched)

    def test_the_prose_and_the_artifact_agree_on_every_arm(self):
        for a in self.arms():
            for c in (a["compare_addr"], a["span"]["start"]):
                self.assertIn(
                    c, self.md,
                    "docs/re/gym.md never names %s, which data/gym_arms.json "
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
        for e in self.art["joint"]["effects"]:
            self.assertIn(e["addr"], self.md,
                          "the prose does not carry the joint's effect at %s"
                          % e["addr"])
        for s in self.art["abs_recompute"]["steps"]:
            self.assertIn(s["effect"]["addr"], self.md)


if __name__ == "__main__":
    unittest.main(verbosity=2)
