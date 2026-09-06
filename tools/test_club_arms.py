#!/usr/bin/env python3
"""`data/club_arms.json` and `docs/re/club.md` re-derived from `orig/g.exe`.

The artifact and the prose are the two places the same claims about the club
(`kl`, `1000:df06`..`1000:e390`) and the command list (`i`,
`1000:ea94`..`1000:ec82`) live; this is what stops either drifting from the
binary it describes.  Nothing here reads `src/`, a screen, or Ghidra's C.
`tools/test_gym_arms.py` is the model, and the same two signals are kept apart
for the same reason (`docs/re/METHODOLOGY.md`, "Is this address a call site?"):

  * **alignment** -- the address is reached by decoding forward from its
    enclosing function's entry, so it is a real instruction boundary and not a
    byte-scan hit in the middle of one;
  * **identity** -- the instruction decoded there says what the artifact says
    it says.

The claims that are NOT restatements of a single decode are asserted by SET
EQUALITY against a sweep of the binary:

  * **`strings[]` is complete** in both ranges, so "the `w` arm prints
    nothing", "there is no unknown-key literal" and "the `i` list is seventeen
    lines" are measurements.
  * **the gate inventory is complete** -- every CONDITIONAL branch in either
    range must be named somewhere in the artifact.  The string sweep cannot see
    a silent gate, and "at district 1 the key `2` is not even compared" is a
    headline claim.
  * **`effects[]` is complete.**  Every instruction carrying an absolute-memory
    operand falls in a WRITE or a READ bucket -- an unclassified one fails
    loudly -- and the WRITE bucket equals the union of every recorded effect.
    The `i` handler's "writes nothing" is the same sweep, run over a range
    where the answer is zero, with the club's seventeen used as the proof that
    the sweep can see a write at all.
  * **the ONE draw.**  `1000:e0b7` is the club's only `Random` site and the `i`
    handler has none; a zero is not evidence on its own, so the same signature
    is swept image-wide and required to find the population
    `docs/re/METHODOLOGY.md` records.
  * **the menu block and the arm block are measured against each other, not
    assumed symmetric.**  The three shared five-byte predicates are re-sliced
    out of `orig/g.exe`, each `jcc` beside them is required to differ, and the
    longest run the two whole blocks share is recomputed by
    longest-common-substring rather than remembered.
  * **the forced exit is decoded, not described.**  `1000:e251` writes `"w"`
    into the club's own input buffer, which is only true if `0f78:0b01` takes
    its SOURCE from `ss:bx+0xa` -- the reverse of the push order a reader would
    assume.  The callee is decoded here.
  * **the opponent announcement's three copies are searched for**, not listed:
    the 66-byte run must occur at exactly `1000:c3f1`, `1000:dc16` and
    `1000:e1a2`.
  * **the `i` list's seven gates chain.**  Each `jnz`'s own displacement is
    decoded and required to land on the NEXT gate, so "no gate can hide another
    line" is arithmetic, and the gate ORDER is asserted to differ from the
    flag-address order -- the swap `src/locations.rs` warns about.

    python3 tools/test_club_arms.py
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
ART = REPO / "data" / "club_arms.json"
BRANCHES = REPO / "data" / "branches.json"
TABLES = REPO / "data" / "string_tables.json"
DOC = REPO / "docs" / "re" / "club.md"

#: The club: the `kl` verb compare through, but not including, the `trn` verb
#: compare that `docs/re/gym.md` owns.
CLO, CHI = 0xdf06, 0xe390

#: The command list: the `i` verb compare through, but not including, `s`.
ILO, IHI = 0xea94, 0xec82

RANDOM_CALL = b"\x9a\x4b\x11\x78\x0f"
STR_COMPARE = b"\x9a\xd8\x0b\x78\x0f"

#: Copied verbatim from `tools/test_gym_arms.py`, including the reasons: the
#: MNEMONIC decides, never operand order; `push [N]` is a read; `xchg` is in
#: NEITHER bucket so an unclassified shape fails the sweep instead of passing
#: as a read.
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


class ClubTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.art = json.loads(ART.read_text(encoding="utf-8"))
        cls.club = cls.art["club"]
        cls.list = cls.art["command_list"]
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        cls.prog = re_query.Program()
        cls.md = DOC.read_text(encoding="utf-8")
        cls.spans = inline_spans(strip_fences(cls.md))

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

    def all_addresses(self):
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

    def arms(self):
        return self.club["arms"]

    # ------------------------------------------------------------- decode set
    def test_both_ranges_decode_as_one_aligned_run(self):
        """The instruction counts are the anchor every negative rests on.

        An empty hit list means nothing unless the walk is known to have
        covered the range: a walk that stopped early must not pass as a search
        that found nothing.
        """
        for key, lo, hi in (("club", CLO, CHI), ("command_list", ILO, IHI)):
            run = self.run_of(lo, hi)
            rec = self.art[key]["range"]
            self.assertEqual(
                len(run), rec["instruction_count"],
                "%s: the aligned decode of %s..%s yields %d instructions, the "
                "artifact records %d -- one of the two is wrong and every "
                "sweep below rests on this number"
                % (key, cit(lo), cit(hi), len(run), rec["instruction_count"]))
            self.assertEqual(run[0].off, lo)
            self.assertEqual(
                run[-1].off + run[-1].length, hi,
                "%s: the run does not end exactly on %s, so the range is not a "
                "whole number of instructions" % (key, cit(hi)))
            self.assertEqual(rec["start"], cit(lo))
            self.assertEqual(rec["end"], cit(hi))

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
                "%s: data/club_arms.json says %s at %s, orig/g.exe decodes %s "
                "there" % (path, node["text"], node["addr"], ins.text))

    #: An instruction claim written INSIDE a prose string, which carries
    #: neither a separate `addr` key nor a separate `text` key and so escapes
    #: every other check here.
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
            len(found), 3,
            "the prose-embedded instruction sweep matched only %d spans; a "
            "scan that measures nothing must not pass" % len(found))
        # And prove the regex can still see a claim of the shape it hunts, so
        # "no matches" can never be mistaken for "no defects".
        self.assertEqual(
            self.PROSE_INSN.findall("x `1000:e2c5 inc [0x38a0]` y"),
            [("1000:e2c5", "inc [0x38a0]")],
            "the prose-embedded pattern no longer matches the claim shape it "
            "hunts")
        for path, c, text in found:
            ins = self.at(c)
            self.assertEqual(
                ins.text, text,
                "%s: data/club_arms.json writes `%s %s` inside a prose string, "
                "but orig/g.exe decodes %r there" % (path, c, text, ins.text))

    def test_every_address_the_artifact_names_is_a_boundary(self):
        exempt = {e["addr"]
                  for e in self.club["known_not_boundaries"]["entries"]}
        cits = sorted(self.all_addresses() - exempt)
        self.assertGreaterEqual(
            len(cits), 180,
            "the artifact names only %d distinct 1000: addresses; a scan that "
            "measures nothing must not pass" % len(cits))
        for c in cits:
            self.at(c)

    def test_each_exemption_names_the_instruction_that_covers_it(self):
        """The byte role in an exemption's prose is re-derived, not authored.

        Both entries claim to be the DISPLACEMENT byte of a `jbe`, which is the
        load-bearing half of the longest-common-run finding: the shared run
        stops on the opcode byte and the two displacements are the first
        difference.  So the covering instruction is decoded, it must really
        cover the exempt address, and the index must be the offset within it.
        """
        entries = self.club["known_not_boundaries"]["entries"]
        self.assertEqual(len(entries), 2)
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
            self.assertTrue(
                host.text.startswith("jbe"),
                "%s: the covering instruction is recorded as a `jbe`; it "
                "decodes %r" % (e["addr"], host.text))
            self.assertIn(
                cit(hi), self.aligned,
                "%s: %s is not a boundary, so %s cannot be interior to a "
                "single instruction" % (e["addr"], cit(hi), e["addr"]))

    def test_the_boundary_exemption_list_is_honest(self):
        """An exemption that names a real boundary would hide a wrong address."""
        for e in self.club["known_not_boundaries"]["entries"]:
            self.assertNotIn(
                e["addr"], self.aligned,
                "%s is exempted from the boundary walk but IS a boundary; the "
                "exemption is either stale or covering for a wrong address"
                % e["addr"])

    # -------------------------------------------------------------- literals
    def test_every_literal_decodes_to_the_recorded_text(self):
        seen = self.walk(("cs_offset", "file_offset", "text"))
        self.assertGreaterEqual(
            len(seen), 45,
            "only %d literal records; the artifact lost its strings"
            % len(seen))
        for node, path in seen:
            cs = int(node["cs_offset"], 16)
            self.assertEqual(
                int(node["file_offset"], 16), cs + addrmod.HEADER_BYTES,
                "%s: file_offset is not cs_offset + the MZ header size" % path)
            self.assertEqual(
                self.cs_literal(cs), node["text"],
                "%s: the Pascal shortstring at CS %s is %r, the artifact "
                "records %r" % (path, node["cs_offset"], self.cs_literal(cs),
                                node["text"]))
            push = node["push"]
            self.assertEqual(
                self.at(push["addr"]).text, "mov di,%s" % node["cs_offset"],
                "%s: the push at %s does not load %s"
                % (path, push["addr"], node["cs_offset"]))

    def test_the_recorded_strings_are_every_cs_literal_pushed(self):
        """SET EQUALITY, both directions, over BOTH ranges.

        Swept: every `mov di,imm16` followed by `push cs` / `push di`.
        Recorded: every literal record whose push falls in that range.  "The
        `w` arm prints nothing" and "the `i` list is exactly seventeen lines"
        are this measurement, not notes.
        """
        recorded_all = [n for n, _ in self.walk(("cs_offset", "text"))]
        for key, lo, hi in (("club", CLO, CHI), ("command_list", ILO, IHI)):
            run = self.run_of(lo, hi)
            swept = set()
            for k, i in enumerate(run):
                if (i.text.startswith("mov di,0x") and k + 2 < len(run)
                        and run[k + 1].text == "push cs"
                        and run[k + 2].text == "push di"):
                    swept.add(cit(i.off))
            recorded = {n["push"]["addr"] for n in recorded_all
                        if lo <= off_of(n["push"]["addr"]) < hi}
            self.assertEqual(
                swept, recorded,
                "%s: the CS-literal push sweep and the artifact disagree: only "
                "swept %s, only recorded %s"
                % (key, sorted(swept - recorded), sorted(recorded - swept)))
            self.assertEqual(
                len(swept), self.art[key]["sweeps"]["cs_literal_pushes"],
                "%s: sweeps.cs_literal_pushes says %d, the sweep finds %d"
                % (key, self.art[key]["sweeps"]["cs_literal_pushes"],
                   len(swept)))

    def test_every_literal_in_the_dispatch_region_belongs_to_an_arm(self):
        """The measurable half of "an unrecognised key is silent".

        This does NOT prove the negative on its own -- no scan can prove that a
        string is not a "не понял" line by looking at it.  What it measures is
        ATTRIBUTION: every CS literal pushed between the case fold and the exit
        compare is one this map already assigns to a named arm, so there is no
        unaccounted literal in the dispatch region for such a line to be.  The
        silence itself rests on the branch inventory being complete, which is
        `test_the_recorded_gates_are_every_conditional_branch`.
        """
        recorded = set()
        for a in self.arms():
            recorded |= {s["push"]["addr"] for s in a["strings"]}
            recorded.add(a["key_literal"]["push"]["addr"])
        run = self.run_of(0xe065, 0xe36b)
        swept = {cit(i.off) for k, i in enumerate(run)
                 if i.text.startswith("mov di,0x")
                 and k + 2 < len(run) and run[k + 1].text == "push cs"
                 and run[k + 2].text == "push di"}
        self.assertEqual(
            swept - recorded, set(),
            "literals pushed between the case fold and the exit that belong to "
            "no recorded arm: %s" % sorted(swept - recorded))
        self.assertGreaterEqual(len(swept), 15)

    # ---------------------------------------------------------------- gates
    def test_the_recorded_gates_are_every_conditional_branch(self):
        named = self.all_addresses()
        for key, lo, hi in (("club", CLO, CHI), ("command_list", ILO, IHI)):
            swept = {cit(i.off) for i in self.run_of(lo, hi)
                     if re.match(r"^j(?!mp)", i.text)}
            missing = sorted(swept - named)
            self.assertEqual(
                missing, [],
                "%s: conditional branches in %s..%s that the artifact never "
                "names: %s" % (key, cit(lo), cit(hi), missing))
            self.assertEqual(
                len(swept), self.art[key]["sweeps"]["conditional_branches"],
                "%s: sweeps.conditional_branches says %d, the sweep finds %d"
                % (key, self.art[key]["sweeps"]["conditional_branches"],
                   len(swept)))

    def test_the_branch_census_reproduces_data_branches_json(self):
        for key, (lo, hi) in (("club", (0xdf06, 0xe38f)),
                              ("command_list", (0xea94, 0xec81))):
            cen = self.art[key]["branch_census"]
            rng = [b for b in self.branches["branches"]
                   if lo <= off_of(b["addr"]) <= hi]
            self.assertEqual(
                len(rng), cen["branches"],
                "%s: data/branches.json holds %d branches in %s..%s, the "
                "artifact records %d"
                % (key, len(rng), cit(lo), cit(hi), cen["branches"]))
            untouched = sum(1 for b in rng if not b["port_touched"])
            self.assertEqual(
                untouched, cen["port_touched_false"],
                "%s: %d branches have port_touched false, the artifact records "
                "%d" % (key, untouched, cen["port_touched_false"]))

    # ---------------------------------------------------------------- draws
    def test_the_club_draws_exactly_once_and_the_sweep_can_find_a_draw(self):
        """One site in the club, none in `i` -- and the sweep is shown to work.

        A count is not evidence until the sweep is known to be able to find
        one, so the same signature is swept over the whole image and required
        to find the population `docs/re/METHODOLOGY.md` records (86 far calls).
        """
        club_sites = [cit(i.off) for i in self.run_of(CLO, CHI)
                      if i.raw[:5] == RANDOM_CALL]
        self.assertEqual(
            club_sites, [a["draws"][0]["addr"] for a in self.arms()
                         if a["draws"]],
            "the club's Random sites are %s; the artifact records %s"
            % (club_sites, [a["draws"][0]["addr"] for a in self.arms()
                            if a["draws"]]))
        self.assertEqual(club_sites, ["1000:e0b7"])
        self.assertEqual(self.club["sweeps"]["random_call_sites"], 1)
        self.assertEqual(
            [cit(i.off) for i in self.run_of(ILO, IHI)
             if i.raw[:5] == RANDOM_CALL], [])
        self.assertEqual(self.list["sweeps"]["random_call_sites"], 0)
        image_wide = self.img.count(RANDOM_CALL)
        self.assertGreaterEqual(
            image_wide, 80,
            "the Random-call sweep finds only %d sites in the whole image; "
            "docs/re/METHODOLOGY.md records 86 far calls, so a sweep that "
            "cannot find them proves nothing" % image_wide)

    def test_the_draw_n_is_re_derived_by_the_walk_back(self):
        """`district * 12`, from `tools/re_query.py`'s own idiom walk."""
        d = self.club["arms"][0]["draws"][0]
        got = re_query.pushed_n(self.prog, d["addr"])
        self.assertEqual(
            got["n_expr"], d["n_expr"],
            "%s pushes %r, the artifact records %r"
            % (d["addr"], got["n_expr"], d["n_expr"]))
        self.assertEqual(
            self.club["arms"][0]["luck_compare"]["n_expr"], d["n_expr"])

    # -------------------------------------------------------------- effects
    def test_the_recorded_effects_are_every_absolute_write_in_the_club(self):
        writes, reads, unclassified = set(), set(), []
        for i in self.run_of(CLO, CHI):
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
        recorded.add(self.club["stake_init"]["effect"]["addr"])
        for m in self.club["menu_lines"]:
            recorded.add(m["colour_digit"]["affordable_store"]["addr"])
            recorded.add(m["colour_digit"]["unaffordable_store"]["addr"])
        self.assertEqual(
            writes, recorded,
            "the absolute-write sweep and the artifact disagree: only swept "
            "%s, only recorded %s"
            % (sorted(writes - recorded), sorted(recorded - writes)))
        self.assertEqual(
            len(writes), self.club["sweeps"]["absolute_memory_writes"])
        self.assertGreater(len(reads), 20)

    def test_the_command_list_writes_nothing_and_that_sweep_can_see_a_write(self):
        """The `i` handler's headline negative, with its sweep proven live."""
        found = [cit(i.off) for i in self.run_of(ILO, IHI)
                 if WRITES_ABS_MEM.match(i.text)]
        self.assertEqual(
            found, [],
            "the `i` handler is recorded as writing nothing; the sweep finds "
            "%s" % found)
        self.assertEqual(self.list["sweeps"]["absolute_memory_writes"], 0)
        # The same regex over the club must find its seventeen, or the zero
        # above would be a check that cannot fail.
        club = [cit(i.off) for i in self.run_of(CLO, CHI)
                if WRITES_ABS_MEM.match(i.text)]
        self.assertEqual(
            len(club), 17,
            "the write regex finds %d writes in the club; if it found none the "
            "`i` handler's zero would prove nothing" % len(club))

    def test_the_w_arm_writes_and_prints_nothing(self):
        arm = next(a for a in self.arms() if a["key"] == "w")
        body = self.run_of(off_of(arm["span"]["start"]),
                           off_of(arm["span"]["end"]))
        self.assertGreater(len(body), 4,
                           "the `w` arm's span decoded to %d instructions"
                           % len(body))
        self.assertEqual(
            [cit(i.off) for i in body if WRITES_ABS_MEM.match(i.text)], [],
            "the `w` arm is recorded as writing nothing")
        self.assertEqual(
            [cit(i.off) for i in body
             if i.text in ("call 0xeed:0x1c2", "call 0xeed:0x0")], [],
            "the `w` arm is recorded as printing nothing")
        self.assertEqual(arm["effects"], [])
        self.assertEqual(arm["prints"], [])

    def test_every_recorded_print_is_a_writeln_call(self):
        n = 0
        for a in self.arms():
            for p in a["prints"]:
                self.assertEqual(self.at(p["addr"]).text, "call 0xeed:0x1c2",
                                 "arm %s: %s is not a WriteLn"
                                 % (a["key"], p["addr"]))
                n += 1
        for ln in self.list["lines"]:
            self.assertEqual(self.at(ln["print"]["addr"]).text,
                             "call 0xeed:0x1c2")
            n += 1
        self.assertGreaterEqual(n, 25)

    # -------------------------------------------------------------- globals
    def test_the_dgroup_addresses_touched_are_the_recorded_globals(self):
        for key, lo, hi in (("club", CLO, CHI), ("command_list", ILO, IHI)):
            swept = {"20ae:" + m for i in self.run_of(lo, hi)
                     if "[0x" in i.text
                     for m in re.findall(r"\[0x([0-9a-f]+)\]", i.text)}
            recorded = {g["ds"] for g in self.art[key]["globals"]}
            self.assertEqual(
                swept, recorded,
                "%s: the DGROUP-operand sweep and globals[] disagree: only "
                "swept %s, only recorded %s"
                % (key, sorted(swept - recorded), sorted(recorded - swept)))
            self.assertEqual(
                len(swept),
                self.art[key]["sweeps"]["dgroup_addresses_touched"])

    def test_each_globals_write_list_is_its_writes_in_range(self):
        for key, lo, hi in (("club", CLO, CHI), ("command_list", ILO, IHI)):
            for g in self.art[key]["globals"]:
                dskey = "[0x%s]" % g["ds"].split(":")[1]
                run = self.run_of(lo, hi)
                swept = [cit(i.off) for i in run
                         if WRITES_ABS_MEM.match(i.text) and dskey in i.text]
                self.assertEqual(
                    swept, g["written_in_range"],
                    "%s/%s: the sweep finds %s written in range, the artifact "
                    "records %s" % (key, g["ds"], swept,
                                    g["written_in_range"]))
                reads = len([i for i in run if dskey in i.text
                             and not WRITES_ABS_MEM.match(i.text)])
                self.assertEqual(
                    reads, g["read_sites_in_range"],
                    "%s/%s: %d read sites in range, the artifact records %d"
                    % (key, g["ds"], reads, g["read_sites_in_range"]))

    def test_every_globals_xref_census_is_what_re_query_reports(self):
        """`named_from` and "the only writer" are re-derived, not trusted."""
        checked = 0
        for key in ("club", "command_list"):
            for g in self.art[key]["globals"]:
                scan = re_query.xrefs_to(self.prog, g["ds"])["scan"]
                xr = g["xrefs"]
                self.assertEqual(
                    (scan["raw_hits"], len(scan["accepted"]),
                     len(scan["discarded"])),
                    (xr["raw_hits"], xr["accepted"], xr["discarded"]),
                    "%s: `xrefs-to` reports raw=%d accepted=%d discarded=%d, "
                    "the artifact records raw=%d accepted=%d discarded=%d"
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
                    "`xrefs-to` finds %s -- an 'only writer' claim that "
                    "stopped the next search is exactly what this check exists "
                    "for" % (g["ds"], xr["writers_image_wide"], writers))
                for e in g["evidence"]:
                    self.assertEqual(
                        self.at(e["addr"]).text, e["text"],
                        "%s: evidence at %s says %r, orig/g.exe decodes %r"
                        % (g["ds"], e["addr"], e["text"],
                           self.at(e["addr"]).text))
                checked += 1
        self.assertEqual(checked, 19,
                         "twelve club globals and seven flags were expected, "
                         "%d were checked" % checked)

    def test_the_stake_is_club_local_with_exactly_three_writers(self):
        """`20ae:3c82` is the card game's stake -- the naming this task adds.

        `docs/re/tables.md` records it as the one debited byte variable whose
        `what` is null.  Containment is what makes "it does not survive leaving
        the club" a measurement rather than an assumption, so every accepted
        reference image-wide is required to land inside the `p` arm's own
        address window.
        """
        f = self.club["stake_finding"]
        scan = re_query.xrefs_to(self.prog, "20ae:3c82")["scan"]
        offs = sorted(off_of(a["at"]) for a in scan["accepted"])
        self.assertTrue(offs, "the 20ae:3c82 census is empty")
        self.assertGreaterEqual(offs[0], 0xe020)
        self.assertLessEqual(offs[-1], 0xe25d)
        writers = [a["at"] for a in scan["accepted"]
                   if WRITES_ABS_MEM.match(a["text"])]
        self.assertEqual(
            writers, ["1000:e020", "1000:e0f7", "1000:e145"],
            "20ae:3c82 is written at %s" % writers)
        self.assertEqual(
            [s["at"] for s in f["life_cycle"]][:3],
            ["1000:e020", "1000:e0a8", "1000:e0d7"])
        self.assertEqual(
            f["command"], "python3 tools/re_query.py xrefs-to 20ae:3c82")

    def test_the_stake_ladder_immediates_are_the_recorded_ones(self):
        """5, +2, reset 5, and the two bounds 17 and 5 -- read off the bytes.

        `stake_finding.arithmetic` says six consecutive wins reach the caught
        bound.  That is a statement about three immediates that live in three
        different instructions, so all three are decoded and the ladder is
        walked with the DECODED values rather than with the literals in the
        sentence.
        """
        init = self.at("1000:e020")
        step = self.at("1000:e0f7")
        bound = self.at("1000:e174")
        self.assertEqual(init.text, "mov byte [0x3c82],0x5")
        self.assertEqual(step.text, "add byte [0x3c82],0x2")
        self.assertEqual(self.at("1000:e145").text, "mov byte [0x3c82],0x5")
        self.assertEqual(self.at("1000:e14a").text, "cmp byte [0x3c82],0x11")
        self.assertEqual(self.at("1000:e151").text, "cmp byte [0x3c82],0x5")
        self.assertEqual(bound.text, "cmp byte [0x3c82],0x11")
        v = int(init.text.rsplit(",", 1)[1], 16)
        d = int(step.text.rsplit(",", 1)[1], 16)
        limit = int(bound.text.rsplit(",", 1)[1], 16)
        wins = 0
        while v < limit:
            v += d
            wins += 1
        self.assertEqual((v, wins), (17, 6),
                         "the decoded ladder reaches %d after %d wins; "
                         "stake_finding.arithmetic says 17 after six"
                         % (v, wins))
        self.assertIn("six consecutive wins",
                      self.club["arms"][0]["caught_block"]["entered_when"])

    def test_the_stake_init_is_outside_the_loop(self):
        """The reset is per VISIT, not per prompt iteration.

        `1000:e020` must be strictly BEFORE the loop top, and the back edge
        must target the loop top, or "the stake carries across keys within one
        visit" is wrong.
        """
        init = off_of(self.club["stake_init"]["effect"]["addr"])
        top = off_of(self.club["loop"]["top"])
        self.assertLess(init, top)
        back = self.at(self.club["loop"]["back_edge"]["addr"])
        self.assertEqual(cit(self.rel_target(back)), self.club["loop"]["top"])
        # And the loop top is the PROMPT push, not the menu: the last menu
        # line's WriteLn is strictly before it.
        self.assertEqual(self.at(self.club["loop"]["top"]).text,
                         "mov di,%s"
                         % self.club["input_read"]["prompt_literal"]["cs_offset"])
        self.assertGreater(
            top, off_of(self.club["menu_lines"][-1]["print"]["addr"]))
        exit_edge = self.at(self.club["loop"]["exit_edge"]["addr"])
        self.assertEqual(self.rel_target(exit_edge),
                         off_of(self.club["bounded_on_the_right_by"]
                                ["setup_addr"]))

    # ------------------------------------------------------------ boundaries
    def test_the_range_boundaries_are_verb_compares(self):
        pairs = ((self.club["verb"], "kl"),
                 (self.club["bounded_on_the_right_by"], "trn"),
                 (self.list["verb"], "i"),
                 (self.list["bounded_on_the_right_by"], "s"))
        for node, key in pairs:
            c = node["compare_addr"]
            ins = self.at(c)
            self.assertEqual(
                ins.raw[:5], STR_COMPARE,
                "%s is recorded as the %r verb compare but decodes %r"
                % (c, key, ins.text))
            self.assertEqual(node["key_literal"]["text"], key)
        self.assertEqual(off_of(self.club["verb"]["compare_addr"]), CLO)
        self.assertEqual(
            off_of(self.club["bounded_on_the_right_by"]["compare_addr"]), CHI)
        self.assertEqual(off_of(self.list["verb"]["compare_addr"]), ILO)
        self.assertEqual(
            off_of(self.list["bounded_on_the_right_by"]["compare_addr"]), IHI)
        # `data/command_dispatch.json` is the independent authority.
        chain = json.loads(
            (REPO / "data" / "command_dispatch.json").read_text(
                encoding="utf-8"))["confirmed_dispatch_chain"]
        by_verb = {e["verb"]: e for e in chain}
        self.assertEqual(by_verb["kl"]["compare_addr"],
                         self.club["verb"]["compare_addr"])
        self.assertEqual(by_verb["trn"]["compare_addr"],
                         self.club["bounded_on_the_right_by"]["compare_addr"])
        self.assertEqual(by_verb["i"]["compare_addr"],
                         self.list["verb"]["compare_addr"])
        self.assertEqual(by_verb["s"]["compare_addr"],
                         self.list["bounded_on_the_right_by"]["compare_addr"])

    def test_each_key_compare_is_the_shortstring_compare_on_the_club_buffer(self):
        for a in self.arms():
            ins = self.at(a["compare_addr"])
            self.assertEqual(
                ins.raw[:5], STR_COMPARE,
                "arm %s: %s is not the shortstring compare"
                % (a["key"], a["compare_addr"]))
            self.assertEqual(
                self.at(a["push_buffer"]["addr"]).text, "mov di,0x3a72",
                "arm %s: the compare does not read the club's own buffer"
                % a["key"])
            self.assertEqual(self.cs_literal(a["key_literal"]["cs_offset"]),
                             a["key"])
        self.assertEqual([a["key"] for a in self.arms()],
                         self.club["key_set"])
        swept = [cit(i.off) for i in self.run_of(CLO, CHI)
                 if i.raw[:5] == STR_COMPARE]
        self.assertEqual(len(swept),
                         self.club["sweeps"]["shortstring_compares"])
        self.assertEqual(
            set(swept),
            {a["compare_addr"] for a in self.arms()}
            | {self.club["verb"]["compare_addr"]})

    def test_the_district_gate_skips_the_key_compare(self):
        """"At district 1 typing `2` is silent" is a flow claim.

        It holds only if the gate's failure target is PAST the arm's own
        compare, so the branch is DECODED rather than read out of the artifact
        and compared with other artifact fields.
        """
        checked = 0
        for a in self.arms():
            if not a["own_gate"]:
                continue
            checked += 1
            br = self.at(a["own_gate"]["branch"]["addr"])
            self.assertEqual(br.text, a["own_gate"]["branch"]["text"])
            self.assertEqual(
                cit(self.rel_target(br)), a["own_gate"]["on_fail"],
                "arm %s: the gate branch targets %s, the artifact records "
                "on_fail %s" % (a["key"], cit(self.rel_target(br)),
                                a["own_gate"]["on_fail"]))
            fail = off_of(a["own_gate"]["on_fail"])
            self.assertGreater(
                fail, off_of(a["compare_addr"]),
                "arm %s: the district gate's failure target %s is BEFORE the "
                "key compare %s, so the key would still be compared"
                % (a["key"], a["own_gate"]["on_fail"], a["compare_addr"]))
            self.assertEqual(
                fail, off_of(a["span"]["end"]),
                "arm %s: the district gate does not skip the whole arm"
                % a["key"])
        self.assertEqual(checked, 1,
                         "exactly one club arm has a district gate of its own, "
                         "found %d" % checked)

    def test_the_spans_tile_both_ranges(self):
        for key, lo, hi in (("club", CLO, CHI), ("command_list", ILO, IHI)):
            spans = self.art[key]["spans"]
            cursor = lo
            for s in spans:
                self.assertEqual(
                    off_of(s["start"]), cursor,
                    "%s: span %r starts at %s, the previous one ended at %s -- "
                    "the tiling has a %s"
                    % (key, s["name"], s["start"], cit(cursor),
                       "gap" if off_of(s["start"]) > cursor else "overlap"))
                self.at(s["start"])
                cursor = off_of(s["end"])
            self.assertEqual(cursor, hi,
                             "%s: the spans stop at %s, the range ends at %s"
                             % (key, cit(cursor), cit(hi)))
        for a in self.arms():
            self.assertIn(
                (a["span"]["start"], a["span"]["end"]),
                [(s["start"], s["end"]) for s in self.club["spans"]],
                "arm %s's span is not one of the tiling's spans" % a["key"])

    # ------------------------------------------------- menu against the arms
    def test_the_menu_and_arm_predicates_differ_exactly_as_recorded(self):
        f = self.club["menu_vs_arm_finding"]
        self.assertEqual(len(f["pairs"]), 3)
        for p in f["pairs"]:
            m = self.sl(off_of(p["menu"]["start"]), off_of(p["menu"]["end"]))
            a = self.sl(off_of(p["arm"]["start"]), off_of(p["arm"]["end"]))
            self.assertEqual(m, p["menu_bytes"], "%s: menu bytes" % p["what"])
            self.assertEqual(a, p["arm_bytes"], "%s: arm bytes" % p["what"])
            self.assertEqual(
                m == a, p["identical"],
                "%s: the artifact records identical=%s, the bytes say %s"
                % (p["what"], p["identical"], m == a))
            self.assertTrue(
                p["identical"],
                "%s: all three recorded pairs are byte-identical predicates; "
                "that is the premise the finding argues against" % p["what"])
            jm = self.sl(off_of(p["menu"]["end"]), off_of(p["menu"]["end"]) + 2)
            ja = self.sl(off_of(p["arm"]["end"]), off_of(p["arm"]["end"]) + 2)
            msg = ("%s: the artifact records %%s, orig/g.exe holds %%s -- the "
                   "two blocks are told apart by these bytes, not by the "
                   "identical predicate above them" % p["what"])
            self.assertEqual(jm, p["jcc_menu"], msg % (p["jcc_menu"], jm))
            self.assertEqual(ja, p["jcc_arm"], msg % (p["jcc_arm"], ja))
            self.assertNotEqual(
                jm, ja,
                "%s: the two `jcc`s beside the identical predicate must "
                "differ, or the two blocks really would be the same code"
                % p["what"])
        # The two price predicates differ in SENSE (`jl` vs `jnl`); the
        # district gate shares its opcode and differs only in displacement.
        price = [p for p in f["pairs"] if "price" in p["what"]]
        self.assertEqual(len(price), 2)
        for p in price:
            self.assertNotEqual(p["jcc_menu"][:2], p["jcc_arm"][:2],
                                "%s: the opcodes are expected to differ"
                                % p["what"])
        gate = next(p for p in f["pairs"] if p["what"] == "the district gate")
        self.assertEqual(gate["jcc_menu"][:2], gate["jcc_arm"][:2])
        self.assertNotEqual(gate["jcc_menu"][3:], gate["jcc_arm"][3:])

    def test_the_longest_run_the_two_blocks_share_is_the_recorded_one(self):
        """The measurement that answers "is the second block a copy?".

        Recomputed here rather than remembered: a longest-common-substring over
        the two spans.  If it grew, the two blocks really do share a body and
        the finding is wrong.
        """
        f = self.club["menu_vs_arm_finding"]
        # The two spans are anchored to addresses this artifact already
        # carries for other reasons, and the anchors are resolved FIRST.
        # Review round 1 showed the cost of leaving them free: widening
        # `arm_span.end` to `1000:e366` and updating `byte_length` to match
        # passed green, so the check proved only that two length fields agreed
        # with each other.
        anchors = {
            "menu_span.start": (f["menu_span"]["start"],
                                f["menu_span"]["start_is"],
                                self.club["menu_lines"][0]["colour_digit"]
                                ["test"]["addr"]),
            "menu_span.end": (f["menu_span"]["end"], f["menu_span"]["end_is"],
                              self.club["stake_init"]["span"]["start"]),
            "arm_span.start": (f["arm_span"]["start"],
                               f["arm_span"]["start_is"],
                               self.arms()[1]["miss_branch"]["addr"]),
            "arm_span.end": (f["arm_span"]["end"], f["arm_span"]["end_is"],
                             self.arms()[2]["span"]["end"]),
        }
        want_is = {"menu_span.start": "menu_lines[0].colour_digit.test.addr",
                   "menu_span.end": "stake_init.span.start",
                   "arm_span.start": "arms[1].miss_branch.addr",
                   "arm_span.end": "arms[2].span.end"}
        for field, (got, says, anchor) in anchors.items():
            self.assertEqual(
                says, want_is[field],
                "%s: the recorded anchor name is %r, this check resolves %r"
                % (field, says, want_is[field]))
            self.assertEqual(
                got, anchor,
                "%s is %s, but %s -- the address it is anchored to -- is %s.  "
                "The span is not a free choice: a span moved to flatter the "
                "longest-common-substring fails here"
                % (field, got, says, anchor))
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
        # The run's last byte is the `jbe` OPCODE and the next byte is the
        # displacement, which is where the two blocks first differ.  Both
        # displacement bytes are the `known_not_boundaries` entries.
        menu_end = off_of(f["menu_span"]["start"]) + best[1] + best[0]
        arm_end = off_of(f["arm_span"]["start"]) + best[2] + best[0]
        self.assertNotEqual(self.img[menu_end], self.img[arm_end])
        self.assertEqual(
            sorted(e["addr"] for e in
                   self.club["known_not_boundaries"]["entries"]),
            sorted([cit(menu_end), cit(arm_end)]),
            "the exemption list must name exactly the two bytes at which the "
            "shared run stops")

    def test_the_menu_price_test_never_hides_a_row(self):
        """Both colour arms reconverge; only 20ae:3b7a differs between them."""
        run = self.run_of(CLO, CHI)
        offs = [i.off for i in run]
        for m in self.club["menu_lines"]:
            lo = self.at(m["colour_digit"]["affordable_store"]["addr"])
            hi = self.at(m["colour_digit"]["unaffordable_store"]["addr"])
            join = run[offs.index(hi.off) + 1]
            after_lo = run[offs.index(lo.off) + 1]
            self.assertTrue(
                after_lo.text.startswith("jmp"),
                "club row %s: the affordable store is not followed by a jump"
                % m["key"])
            self.assertEqual(
                self.rel_target(after_lo), join.off,
                "club row %s: the two colour arms do not reconverge on %s"
                % (m["key"], cit(join.off)))
            self.assertIn("[0x3b7a]", lo.text)
            self.assertIn("[0x3b7a]", hi.text)

    # ------------------------------------------------------------ arm detail
    def test_the_luck_compare_is_the_32_bit_idiom_with_the_win_falling_through(self):
        """The operands and the sense, which is what the port needs.

        `Game::luck_below_random_32` already models the PREDICATE.  What this
        checks is the two operands (`20ae:38a4` sign-extended, the draw
        zero-extended) and that the club's arrangement makes the FALL-THROUGH
        the win -- the opposite of the den's two copies.
        """
        lc = self.club["arms"][0]["luck_compare"]
        self.assertEqual(
            self.at(lc["right_operand"]["widened_at"]["addr"]).text,
            "xor dx,dx",
            "the draw must ZERO-extend, or the high halves would not decide "
            "the way the artifact records")
        self.assertEqual(self.at(lc["left_operand"]["loaded_at"]["addr"]).text,
                         "mov ax,[0x38a4]")
        self.assertEqual(self.at(lc["left_operand"]["widened_at"]["addr"]).text,
                         "cwd",
                         "luck must SIGN-extend; that asymmetry is the whole "
                         "reason a `jl` sits beside a `jb`")
        hi_cmp = self.at("1000:e0c6")
        self.assertEqual(hi_cmp.text, "cmp dx,bx")
        win = self.at("1000:e0c8")
        self.assertEqual(win.text[:4], "jnle")
        self.assertEqual(cit(self.rel_target(win)), "1000:e0d0",
                         "the high-half WIN branch must reach the payout")
        lose_hi = self.at("1000:e0ca")
        self.assertEqual(lose_hi.text[:2], "jl")
        lose_lo = self.at("1000:e0ce")
        self.assertEqual(lose_lo.text[:2], "jb")
        self.assertEqual(cit(self.rel_target(lose_hi)),
                         cit(self.rel_target(lose_lo)),
                         "both LOSE branches must reach the same block")
        self.assertEqual(cit(self.rel_target(lose_lo)), "1000:e129")
        # The fall-through of the low compare is the payout, so a win is
        # `luck >= draw` -- the inverse of `luck_below_random_32`.
        self.assertEqual(cit(lose_lo.off + lose_lo.length), "1000:e0d0")
        self.assertEqual(lc["left_operand"]["ds"], "20ae:38a4")

    def test_the_forced_exit_writes_w_into_the_clubs_own_buffer(self):
        """Which pointer is the SOURCE is settled by decoding `0f78:0b01`.

        The push order at `1000:e243`/`1000:e248` reads as "literal, then
        buffer", and `0f78:0ae7` -- the routine every menu row uses -- takes
        its destination from the FIRST push.  `0f78:0b01` does not: it fetches
        the source from `ss:bx+0xa`, which is the first push.  Reading the
        callee is the only thing that settles it, and getting it backwards
        would turn "the club ejects the player" into "the club overwrites a
        code-segment literal".
        """
        fe = self.club["arms"][0]["caught_block"]["forced_exit"]
        self.assertEqual(self.at("1000:e243").text, "mov di,0x848e")
        self.assertEqual(self.cs_literal(0x848e), "w")
        self.assertEqual(self.at("1000:e248").text, "mov di,0x3a72")
        self.assertEqual(self.at("1000:e24d").text, "mov ax,0xff")
        self.assertEqual(self.at(fe["call"]["addr"]).text, "call 0xf78:0xb01")
        start = addrmod.image_off_of_citation("1f78:0b01")
        body = []
        for i in dis16.decode_run(self.img, start, start + 0x60):
            body.append(i.text)
            if i.text.startswith("ret"):
                break
        self.assertTrue(body[-1].startswith("ret"),
                        "the walk over 0f78:0b01 never reached a return: %s"
                        % body)
        self.assertEqual(body[-1], "retf 0xa",
                         "0f78:0b01 takes two far pointers and a word, so it "
                         "must end in `retf 0xa`; it ends in %r" % body[-1])
        self.assertEqual(
            body[-1], fe["callee_return"],
            "0f78:0b01 is recorded as ending in %r; it ends in %r"
            % (fe["callee_return"], body[-1]))
        self.assertIn(
            fe["source_fetch"], body,
            "the artifact records %r as how 0f78:0b01 fetches its SOURCE; the "
            "decode of the callee is %s -- get this backwards and `the club "
            "ejects the player` becomes `the club overwrites a code-segment "
            "literal`" % (fe["source_fetch"], body))
        self.assertIn(
            fe["dest_fetch"], body,
            "the artifact records %r as how 0f78:0b01 fetches its "
            "DESTINATION; the decode of the callee is %s"
            % (fe["dest_fetch"], body))
        # The source must be the FIRST push and the destination the SECOND --
        # the reverse of 0f78:0ae7's reading, which is the whole trap.
        self.assertGreater(
            int(re.search(r"bx\+0x([0-9a-f]+)", fe["source_fetch"]).group(1),
                16),
            int(re.search(r"bx\+0x([0-9a-f]+)", fe["dest_fetch"]).group(1),
                16),
            "the SOURCE must be fetched from the deeper stack slot (the first "
            "push); %r against %r says otherwise"
            % (fe["source_fetch"], fe["dest_fetch"]))
        # And the consequence: after the write the next compare on that buffer
        # is the `1` arm's, and the `w` compare is the last one in the range.
        after = self.at("1000:e256")
        self.assertEqual(cit(self.rel_target(after)),
                         self.club["arms"][1]["push_buffer"]["addr"])
        self.assertEqual(self.club["arms"][-1]["key"], "w")

    def test_the_opponent_announcement_has_exactly_three_copies(self):
        """Searched for, not listed."""
        f = self.club["announcement_finding"]
        run = self.img[0xe1a2:0xe1a2 + f["byte_length"]]
        self.assertEqual(len(run), 66)
        found, i = [], 0
        while True:
            j = self.img.find(run, i)
            if j < 0:
                break
            found.append(cit(j))
            i = j + 1
        self.assertEqual(
            found, f["occurrences"],
            "the 66-byte announcement occurs at %s; the artifact records %s"
            % (found, f["occurrences"]))
        self.assertEqual(len(found), 3)
        # Each copy is a `param_1 = 1` opponent-roll site, so each is preceded
        # within 0x20 bytes by a near call that wraps to 1000:0d14.
        callers = set(near_calls_to(self.img, 0x0d14))
        for c in found:
            self.assertTrue(
                any(cit(off_of(c) - d) in callers for d in range(1, 0x30)),
                "%s: no `call 1000:0d14` within 0x30 bytes before the "
                "announcement" % c)

    def test_the_four_near_calls_out_wrap_to_the_recorded_targets(self):
        want = {"1000:2526": [], "1000:0d14": [], "1000:3d11": []}
        for c in self.club["arms"][0]["calls_out"]:
            if c["target"] in want:
                want[c["target"]].append(c["addr"])
        for target, sites in want.items():
            callers = near_calls_to(self.img, off_of(target))
            for s in sites:
                self.assertIn(s, callers,
                              "%s does not near-call %s modulo 64 KiB"
                              % (s, target))
            self.assertTrue(sites, "no recorded call to %s" % target)
        for c in self.club["arms"][0]["calls_out"]:
            if c["param"]:
                self.assertEqual(self.at(c["param"]["set_at"]).text,
                                 "mov al,0x%x" % c["param"]["value"])
                self.assertEqual(self.at(c["param"]["push_at"]).text, "push ax")

    def test_the_club_arms_effects_carry_their_branch(self):
        """Every conditional effect in the `p` arm names the branch it hangs on.

        A `p` effect recorded as unconditional when it sits on the win, the
        lose or the caught path is the divergence a screen capture cannot see.
        """
        p = self.club["arms"][0]
        by_branch = {}
        for e in p["effects"]:
            by_branch.setdefault(e.get("branch"), []).append(e["addr"])
        self.assertEqual(
            sorted(by_branch), ["CAUGHT", "LOSE", "WIN",
                                "always, once the money gate passes"])
        self.assertEqual(by_branch["WIN"],
                         ["1000:e0d7", "1000:e0f7", "1000:e11d"])
        self.assertEqual(by_branch["LOSE"], ["1000:e145"])
        self.assertEqual(by_branch["CAUGHT"],
                         ["1000:e184", "1000:e215", "1000:e23e"])
        for e in p["effects"]:
            if e.get("condition"):
                self.at(e["condition"])

    # ------------------------------------------------------- the `i` handler
    def test_the_command_list_calls_nothing_but_writeln(self):
        """"Pure output" is a claim about every call in the range.

        The sweep counts `call` instructions, not just the ones the artifact
        lists: past the verb compare at `1000:ea94` the only call left must be
        `0eed:01c2`, so there is no `ReadLn`, no case fold and no call out.
        """
        calls = [(cit(i.off), i.text) for i in self.run_of(ILO, IHI)
                 if i.text.startswith("call")]
        self.assertEqual(calls[0][0], cit(ILO))
        self.assertEqual(calls[0][1], "call 0xf78:0xbd8")
        rest = {t for _, t in calls[1:]}
        self.assertEqual(
            rest, {"call 0xeed:0x1c2"},
            "past the verb compare the `i` handler is recorded as calling "
            "nothing but WriteLn; it also calls %s"
            % sorted(rest - {"call 0xeed:0x1c2"}))
        self.assertEqual(len(calls) - 1,
                         self.list["sweeps"]["writeln_calls"])
        self.assertEqual(self.list["sweeps"]["write_calls"], 0)

    def test_the_command_list_is_seventeen_lines_one_seven_nine(self):
        lines = self.list["lines"]
        self.assertEqual(len(lines), 17)
        gated = [ln for ln in lines if ln["gate"]]
        self.assertEqual(len(gated), 7)
        self.assertEqual([ln["n"] for ln in gated], [2, 3, 4, 5, 6, 7, 8],
                         "the seven gated lines are not contiguous at 2..8")
        # `shape` is checked against the MEASURED partition, not against its
        # own arithmetic: a sum that adds up is not evidence that the three
        # parts are the three parts.
        head = [ln for ln in lines if not ln["gate"]
                and ln["n"] < min(g["n"] for g in gated)]
        tail = [ln for ln in lines if not ln["gate"]
                and ln["n"] > max(g["n"] for g in gated)]
        shape = self.list["shape"]
        self.assertEqual(
            (len(head), len(gated), len(tail), len(lines)),
            (shape["ungated_head"], shape["gated"], shape["ungated_tail"],
             shape["total"]),
            "the measured partition is %s, the artifact records %s"
            % ((len(head), len(gated), len(tail), len(lines)),
               (shape["ungated_head"], shape["gated"], shape["ungated_tail"],
                shape["total"])))
        self.assertEqual(len(head) + len(gated) + len(tail), len(lines),
                         "some line is neither head, gated, nor tail")

    def test_each_command_list_gate_skips_exactly_its_own_line(self):
        """Arithmetic, not a sentence: each `jnz` lands on the NEXT gate.

        This is what makes "the seven are independent -- no gate can hide
        another line" a measurement.  The last one lands on the start of the
        ungated tail, which is the span the tiling records.
        """
        gated = [ln for ln in self.list["lines"] if ln["gate"]]
        tail = next(s for s in self.list["spans"]
                    if s["name"] == "the nine ungated tail lines")
        for n, ln in enumerate(gated):
            br = self.at(ln["gate"]["branch"]["addr"])
            self.assertEqual(br.text[:3], "jnz")
            target = cit(self.rel_target(br))
            expect = (gated[n + 1]["gate"]["test"]["addr"]
                      if n + 1 < len(gated) else tail["start"])
            self.assertEqual(
                target, expect,
                "line %d's gate jumps to %s; it must land on %s so it skips "
                "exactly its own line" % (ln["n"], target, expect))
            self.assertEqual(
                target, ln["gate"]["skips_to"],
                "line %d: the artifact records skips_to %s; the decoded "
                "displacement says it must land on %s"
                % (ln["n"], ln["gate"]["skips_to"], target))
            # The literal it guards sits strictly between the branch and the
            # skip target.
            push = off_of(ln["string"]["push"]["addr"])
            self.assertLess(br.off, push)
            self.assertLess(push, off_of(target))

    def test_each_command_list_gate_reads_its_verbs_own_discovery_byte(self):
        """The flag->line binding, from flow on BOTH sides.

        The gate here and the verb's own handler gate must be the same
        `cmp byte [0x....],0x1`.  That is what makes the Vet/Den assignment a
        corroboration of `docs/re/command-dispatch.md` rather than a reading of
        the printed text.
        """
        seen = []
        for ln in self.list["lines"]:
            g = ln["gate"]
            if not g:
                continue
            here = self.at(g["test"]["addr"])
            other = re.search(r"at (1000:[0-9a-f]{4})", g["same_byte_as"])
            self.assertIsNotNone(
                other, "line %d does not name the handler's own gate" % ln["n"])
            assert other is not None
            there = self.at(other.group(1))
            key = "[0x%s]" % g["ds"].split(":")[1]
            self.assertIn(key, here.text)
            self.assertEqual(
                here.text, there.text,
                "line %d: the list gate %r and the %s handler's gate at %s "
                "(%r) must be the same compare"
                % (ln["n"], here.text, ln["advertises"], other.group(1),
                   there.text))
            seen.append(g["ds"])
        self.assertEqual(len(set(seen)), 7,
                         "the seven gates must read seven distinct bytes")
        self.assertEqual(sorted(seen),
                         ["20ae:%04x" % a for a in range(0x3694, 0x369b)])

    def test_the_gate_order_is_not_the_flag_address_order(self):
        """The trap `src/locations.rs` already paid for once.

        The list's order is the command-table order, and Vet comes before Den
        in it while `20ae:3696` (Den) comes before `20ae:3698` (Vet) in memory.
        A port that "tidies" the list into address order silently reintroduces
        the swap.
        """
        order = [ln["gate"]["ds"] for ln in self.list["lines"] if ln["gate"]]
        want = ["20ae:3694", "20ae:3695", "20ae:3698", "20ae:3697",
                "20ae:3696", "20ae:3699", "20ae:369a"]
        self.assertEqual(
            order, want,
            "the seven gates are recorded in the order %s; the artifact now "
            "says %s -- the gate order puts Vet before Girl before Den and is "
            "NOT the flag-address order" % (want, order))
        self.assertNotEqual(order, sorted(order),
                            "the gate order is recorded as NOT the address "
                            "order; if it became sorted the finding is wrong")
        self.assertEqual(sorted(order), sorted(set(order)))

    def test_the_command_list_lines_are_in_ascending_push_order(self):
        pushes = [off_of(ln["string"]["push"]["addr"])
                  for ln in self.list["lines"]]
        self.assertEqual(pushes, sorted(pushes),
                         "the recorded line order is not the order the "
                         "handler pushes them")
        self.assertEqual(len(set(pushes)), 17)

    # ------------------------------------------------------------- the prose
    #: The label each "counts this map rests on" table row uses, mapped to the
    #: `sweeps` key it must equal.  The prose tables are the numbers a reader
    #: quotes; nothing else here checks them, so they are parsed and compared
    #: rather than left to drift from the artifact they summarise.
    COUNT_LABELS = {
        "instructions (aligned)": "instructions",
        "CS-literal pushes": "cs_literal_pushes",
        "conditional branches": "conditional_branches",
        "`Random` call sites": "random_call_sites",
        "absolute-memory writes": "absolute_memory_writes",
        "DGROUP addresses touched": "dgroup_addresses_touched",
        "DS-pointer pushes": "ds_pointer_pushes",
        "shortstring compares": "shortstring_compares",
        "`WriteLn` calls": "writeln_calls",
    }

    def test_the_prose_count_tables_are_the_artifacts_sweeps(self):
        sections = re.split(r"^# Part ", self.md, flags=re.M)
        self.assertEqual(len(sections), 3,
                         "docs/re/club.md is expected to have two `# Part` "
                         "halves; found %d" % (len(sections) - 1))
        checked = 0
        for body, key in ((sections[1], "club"),
                          (sections[2], "command_list")):
            sweeps = self.art[key]["sweeps"]
            seen = 0
            for label, value in re.findall(
                    r"^\|\s*(.+?)\s*\|\s*\**(\d+)\**\s*\|$", body, re.M):
                want = self.COUNT_LABELS.get(label)
                if want is None:
                    continue
                seen += 1
                checked += 1
                self.assertEqual(
                    int(value), sweeps[want],
                    "%s: docs/re/club.md's count table says %s = %s, "
                    "data/club_arms.json's sweeps.%s is %d"
                    % (key, label, value, want, sweeps[want]))
            self.assertGreaterEqual(
                seen, 8,
                "%s: only %d count-table rows matched a known label; the "
                "table was renamed and this check stopped measuring it"
                % (key, seen))
        self.assertGreaterEqual(checked, 17)

    def test_every_prose_address_is_an_instruction_boundary(self):
        exempt = {e["addr"]
                  for e in self.club["known_not_boundaries"]["entries"]}
        cits = sorted(set(CITE.findall(strip_fences(self.md))) - exempt)
        self.assertGreaterEqual(
            len(cits), 80,
            "docs/re/club.md names only %d distinct 1000: addresses; a prose "
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
                "docs/re/club.md writes `%s %s`, but tools/dis16.py decodes "
                "%r there" % (c, text, self.at(c).text))
        self.assertGreaterEqual(
            checked, 10,
            "only %d `addr text` spans in docs/re/club.md" % checked)

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
                    "docs/re/club.md writes `%s %s` in a fence, but "
                    "tools/dis16.py decodes %r there"
                    % (c, text, self.aligned[c].text))
        self.assertGreaterEqual(
            checked, 30,
            "only %d fenced instruction lines in docs/re/club.md" % checked)

    def test_every_prose_literal_comes_out_of_the_binary(self):
        offs = [int(m.group(1), 16)
                for m in re.finditer(r"CS `0x([0-9a-f]{4})`", self.md)]
        self.assertGreaterEqual(len(offs), 30, "only %d CS offsets" % len(offs))
        for o in offs:
            self.assertTrue(self.img[o],
                            "CS 0x%04x has a zero length byte" % o)
            self.cs_literal(o)
        # The `, file 0x.....` half is optional: this document writes both
        # forms, and a pattern that only saw the bare one would silently check
        # a third of the quotes it looks like it checks.
        pairs = re.findall(
            r"`((?!1000:)[^`]+)`\s*\(CS `0x([0-9a-f]{4})`"
            r"(?:,\s*file `0x[0-9A-Fa-f]{5}`)?\)", self.md, re.S)
        self.assertGreaterEqual(len(pairs), 25, "only %d pairs" % len(pairs))
        self.assertGreaterEqual(
            len([p for p in pairs if re.search(r"[Ѐ-ӿ]", p[0])]), 20,
            "only %d of the quoted literals are Russian; the pairing is "
            "matching something else" % len(pairs))
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
            "Russian in docs/re/club.md that matches no literal in orig/g.exe "
            "at any address the doc or the artifact names: %r" % unmatched)

    def test_the_prose_and_the_artifact_agree_on_every_arm_and_line(self):
        for a in self.arms():
            for c in (a["compare_addr"], a["span"]["start"]):
                self.assertIn(
                    c, self.md,
                    "docs/re/club.md never names %s, which "
                    "data/club_arms.json records for arm %s" % (c, a["key"]))
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
        for ln in self.list["lines"]:
            self.assertIn(
                ln["string"]["cs_offset"], self.md,
                "the prose does not carry the `i` list's CS %s"
                % ln["string"]["cs_offset"])
            if ln["gate"]:
                self.assertIn(ln["gate"]["test"]["addr"], self.md)

    def test_the_port_change_lists_are_addressed_and_falsifiable(self):
        """Every numbered item names an original address and a consequence.

        **This is a lint over the artifact's own prose, not evidence about
        `orig/g.exe`.**  It cannot judge whether a `do_not_fix` string really
        describes a trap; what it does check is that the item is anchored to an
        address and that its consequence is addressed to the PORTING task
        rather than being a note about this RE task's scope.  Review round 1
        found `command_list` item 4 satisfying the lint through a `do_not_fix`
        whose content ("This RE task does not edit them") inverted the field's
        name, so the second half is narrow by construction: it names one shape
        that has actually occurred here, and it establishes nothing about the
        other items beyond that they do not have that shape.  The numbers
        those consequences quote are checked by
        `test_every_counted_consequence_is_recomputed_from_the_decode`.
        """
        scope_note = re.compile(r"\bthis RE task\b|\bTask 33 (?:does|did) not\b",
                                re.I)
        for key, least in (("club", 10), ("command_list", 4)):
            items = self.art[key]["what_the_port_must_change"]
            self.assertGreaterEqual(len(items), least)
            self.assertEqual([i["n"] for i in items],
                             list(range(1, len(items) + 1)))
            for it in items:
                self.assertTrue(
                    CITE.findall(it["what"]) or "CS `0x" in it["what"],
                    "%s item %d names no original address: %r"
                    % (key, it["n"], it["what"]))
                self.assertTrue(
                    "falsifiable_as" in it or "do_not_fix" in it,
                    "%s item %d carries neither a falsifiable consequence nor "
                    "a do-not-fix trap" % (key, it["n"]))
                for field in ("falsifiable_as", "do_not_fix"):
                    if field in it:
                        self.assertIsNone(
                            scope_note.search(it[field]),
                            "%s item %d's %s is a note about this RE task's "
                            "own scope, not a consequence for the porting "
                            "task: %r" % (key, it["n"], field, it[field]))
                self.assertIn("blocked", it)
        # And the pattern still matches the shape it hunts, so "no hits" can
        # never be mistaken for "no defects".
        self.assertIsNotNone(
            scope_note.search("This RE task does not edit them"),
            "the scope-note pattern no longer matches the string that "
            "prompted it")

    def test_every_counted_consequence_is_recomputed_from_the_decode(self):
        """A consequence that states a COUNT has that count derived here.

        Review round 1 found `command_list` item 1 promising "eleven lines"
        where flow says twelve -- an off-by-one in the one number the porting
        task is invited to test against, transcribed rather than derived, and
        read by nothing.  So every item whose consequence quotes a number
        carries a `derived` block naming the recipe, and the recipe is run
        against `orig/g.exe` here.  The spelled form must also appear in the
        item's own prose, or the block and the sentence could drift apart.
        """
        seen = []
        for key in ("club", "command_list"):
            for it in self.art[key]["what_the_port_must_change"]:
                d = it.get("derived")
                if d is None:
                    continue
                got = self.recompute(key, d)
                self.assertEqual(
                    got, d["value"],
                    "%s item %d: %s recomputes to %d, the artifact records %d"
                    % (key, it["n"], d["claim"], got, d["value"]))
                prose = " ".join(it.get(f, "") for f in
                                 ("what", "falsifiable_as", "do_not_fix"))
                self.assertIn(
                    d["spelled"], prose,
                    "%s item %d: the derived value is spelled %r but the "
                    "item's own prose does not carry it: %r"
                    % (key, it["n"], d["spelled"], prose))
                seen.append(d["kind"])
        self.assertEqual(
            sorted(seen),
            ["abs_writes_to_in_span", "gated_line_count",
             "keys_needing_dispatch", "random_sites_in_span"],
            "the four counted consequences are %s; a `derived` block that "
            "stopped being carried would make its number unchecked again"
            % sorted(seen))

    def recompute(self, key, d):
        """Run one `derived` recipe against `orig/g.exe`."""
        kind = d["kind"]
        if kind == "keys_needing_dispatch":
            swept = [cit(i.off) for i in self.run_of(CLO, CHI)
                     if i.raw[:5] == STR_COMPARE]
            verb = self.club["verb"]["compare_addr"]
            exit_arm = next(a for a in self.arms() if a["key"] == "w")
            return len([c for c in swept
                        if c != verb and c != exit_arm["compare_addr"]])
        if kind == "random_sites_in_span":
            return len([i for i in self.run_of(off_of(d["span"]["start"]),
                                               off_of(d["span"]["end"]))
                        if i.raw[:5] == RANDOM_CALL])
        if kind == "abs_writes_to_in_span":
            dskey = "[0x%s]" % d["ds"].split(":")[1]
            return len([i for i in self.run_of(off_of(d["span"]["start"]),
                                               off_of(d["span"]["end"]))
                        if WRITES_ABS_MEM.match(i.text) and dskey in i.text])
        if kind == "gated_line_count":
            lines = self.list["lines"]
            gated = [ln for ln in lines if ln["gate"]]
            head = len([ln for ln in lines if not ln["gate"]
                        and ln["n"] < min(g["n"] for g in gated)])
            tail = len([ln for ln in lines if not ln["gate"]
                        and ln["n"] > max(g["n"] for g in gated)])
            on = len([ln for ln in gated
                      if ln["gate"]["ds"] in d["flags_set"]])
            self.assertEqual(len(d["flags_set"]), len(set(d["flags_set"])))
            self.assertEqual(
                on, len(d["flags_set"]),
                "the recipe names %d flags but only %d of them gate a line"
                % (len(d["flags_set"]), on))
            return head + on + tail
        self.fail("%s: unknown derived kind %r" % (key, kind))


if __name__ == "__main__":
    unittest.main(verbosity=2)
