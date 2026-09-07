#!/usr/bin/env python3
"""`data/combat_opener.json`, `data/combat_uncited.json` and
`docs/re/combat-opener.md` re-derived from `orig/g.exe` and
`data/branches.json`.

Nothing here reads a screen or Ghidra's C.  `tools/test_gym_arms.py` is the
model, and the same two signals are kept apart for the same reason
(`docs/re/METHODOLOGY.md`, "Is this address a call site?"):

  * **alignment** -- the address is reached by decoding forward from its
    enclosing function's entry, so it is a real instruction boundary and not a
    byte-scan hit in the middle of one;
  * **identity** -- the instruction decoded there says what the artifact says
    it says.

The claims that are NOT restatements of a single decode are asserted by SET
EQUALITY against a sweep, never by checking that the listed entries hold up:

  * **`strings[]` is complete.**  Every `mov di,imm16` / `push cs` / `push di`
    in `1000:3d32`..`1000:3e8d` must appear exactly once in the artifact and
    vice versa, and each one's TEXT is re-decoded from `orig/g.exe` as a
    length-prefixed CP866 shortstring at `cs_offset + 0x18d0`.  So "the ten
    class values print these nine lines and nothing else" is a measurement.
  * **the gate inventory is complete.**  Every CONDITIONAL branch in the range
    must be one of the ten the arms record, in the original's order.
  * **the exit is the only exit and the two entries are the only entries.**
    Both are swept -- the first over every transfer inside the range, the
    second over every aligned branch in segment `1000`.  `docs/re/gaps.md`
    asserted both without a sweep behind them; this is the sweep.
  * **there is no `Random` draw in range -- and the sweep that says so works.**
    A zero count proves nothing on its own, so the same signature is swept over
    the whole image and required to find the 86 `docs/re/METHODOLOGY.md`
    records.  Without that second half this would be the "check that cannot
    fail" that document names.
  * **the range writes nothing.**  Every instruction naming memory must fall in
    a WRITE, READ or ADDRESS-ARITHMETIC bucket -- an unclassified one fails
    loudly -- and the WRITE bucket must be EMPTY.  That is what makes "the
    opener is print-only" a measurement rather than an impression.
  * **the range tiles.**  The eleven spans must cover `1000:3d32`..`1000:3e8d`
    end to end with no gap and no overlap, so a block cannot be dropped from
    the map by being left out of every span.
  * **`data/combat_uncited.json` covers exactly the derived uncited set.**  The
    row set is RECOMPUTED here by reimplementing `docs/re/branches.md`'s
    Coverage rule -- the `CITATION` pattern, the segment normalisation, the
    citation globs read from `data/branches.json`'s own
    `port_citation_sources`, and `port_touched` = the branch's own address or
    its guard's -- and compared for set equality.  **The total is derived from
    the artifact; a hard-coded 117 would be a check that cannot fail.**

    python3 tools/test_combat_opener.py
"""
import collections
import glob
import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import addr as addrmod            # noqa: E402
import re_query                   # noqa: E402
from re_derive import (aligned_boundaries, inline_spans,  # noqa: E402
                       load_image, strip_fences)

REPO = Path(__file__).resolve().parents[1]
ART = REPO / "data" / "combat_opener.json"
UNCITED = REPO / "data" / "combat_uncited.json"
BRANCHES = REPO / "data" / "branches.json"
DOC = REPO / "docs" / "re" / "combat-opener.md"

#: The half-open image range this map owns: the key load through, but not
#: including, the `param_1 == 1` compare that follows the exit.
LO, HI = 0x3d32, 0x3e8d

#: The function whose uncited branches `data/combat_uncited.json` classifies.
FUNC_ENTRY = "1000:3d11"

#: The `Random` far call, `call 0f78:114b`, by its exact five bytes.
RANDOM_CALL = b"\x9a\x4b\x11\x78\x0f"

#: How many of them the whole image holds -- `docs/re/METHODOLOGY.md`.  The
#: in-range count is 0, which is worth nothing unless the same sweep finds
#: this.
RANDOM_CALLS_IMAGE_WIDE = 86

#: The MZ header, so a `cs_offset` becomes the `data/strings.json` file offset.
HDR = 0x18d0

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

#: A transfer of control that names an absolute target in the decoder's text.
TRANSFER = re.compile(r"^(j[a-z]+|call|loop[a-z]*)\s+(?:short\s+)?"
                      r"0x([0-9a-f]+)$")

#: `docs/re/branches.md`'s Coverage rule, character for character: Java's
#: default `\b` is ASCII, so this one is too.
CITATION = re.compile(r"\b([0-9a-fA-F]{4}):([0-9a-fA-F]{1,4})\b", re.ASCII)

#: Rust keywords and prelude names that a `src.expr` can mention without
#: saying anything about the construct it is filed against.  Everything else
#: an expression names has to be findable inside that FUNCTION'S BODY -- see
#: `test_every_implemented_row_names_the_function_that_evaluates_it`.
RUST_NOISE = {"return", "match", "false", "break", "while", "continue",
              "matches", "println", "print", "self", "value", "else",
              "Some", "None", "Self", "Vec", "String", "true"}

#: What counts as a name in a `src.expr`.  Three things this has to get right,
#: each of which a previous revision got wrong:
#:
#:   * **both cases.**  snake_case catches fields, methods and locals;
#:     CamelCase catches the enum variants and types.  Without the second
#:     half, `b'2' => Some(Stat::Agility)` names nothing checkable at all and
#:     its row's pairing is asserted by nothing.
#:   * **word boundaries.**  Unanchored, `[a-z][a-z0-9_]{4,}` matches the
#:     SUFFIX `ommand` inside `Command::Fight` -- a token that occurs in the
#:     module for the wrong reason, so the check passed on a string that is
#:     not an identifier at all.
#:   * **four characters, not five.**  `mine`, `roll`, `luck`, `line`, `rank`
#:     and `blow` are load-bearing names in these expressions; at a five-char
#:     floor three rows had nothing checkable left.
EXPR_IDENT = re.compile(r"\b[a-z][a-z0-9_]{3,}\b|\b[A-Z][A-Za-z0-9_]{3,}\b")

#: One `fn` definition, with the indentation that decides where its body ends.
FN_DEF = re.compile(
    r"^(?P<indent>[ \t]*)(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?"
    r"(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]*\"\s+)?"
    r"fn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)")


def cit(off):
    return "1000:%04x" % off


def off_of(c):
    return int(c.split(":")[1], 16)


def flat(a):
    return int(a[:4], 16) * 16 + int(a[5:], 16)


def fn_bodies(text):
    """`{name: (first_line, body_text)}` for every `fn` the module defines.

    The span runs from the `fn` line to the first later line that is EXACTLY
    the `fn`'s own indentation followed by `}`.  That is rustfmt's guarantee --
    an item's closing brace sits in the item's indentation column and every
    line of its body is indented deeper -- and `cargo fmt --check` is one of
    this repo's gates, so the rule holds by construction rather than by
    convention.

    A name defined twice in one module maps to `None`, and the caller fails on
    it rather than picking one: an ambiguous span would silently widen the
    scope this whole check exists to narrow.
    """
    lines = text.splitlines()
    out = {}
    for i, ln in enumerate(lines):
        m = FN_DEF.match(ln)
        if not m:
            continue
        close = m.group("indent") + "}"
        span = None
        for j in range(i + 1, len(lines)):
            if lines[j] == close:
                span = (i + 1, "\n".join(lines[i:j + 1]))
                break
        name = m.group("name")
        out[name] = None if name in out else span
    return out


def uncited_branches(branches, repo):
    """The uncited game branches of `FUN_1000_3d11`, recomputed.

    This reimplements `tools/ghidra/EnumerateBranches.java`'s rule rather than
    reading its answer, exactly as `docs/re/branches.md`'s Coverage block does.
    Returns the rows AND the citation index they were filtered with, so a
    caller can say WHERE a row that is no longer uncited is now cited.
    """
    cite = collections.defaultdict(set)
    for pat in branches["port_citation_sources"]:
        for p in sorted(glob.glob(str(repo / pat), recursive=True)):
            text = Path(p).read_text(encoding="utf-8")
            for i, ln in enumerate(text.splitlines(), 1):
                for s, o in CITATION.findall(ln):
                    s = int(s, 16)
                    key = (s + 0x1000 if s < 0x1000 else s) * 16 + int(o, 16)
                    cite[key].add("%s:%d" % (p, i))
    out = []
    for b in branches["branches"]:
        if b["class"] != "game" or b["func_entry"] != FUNC_ENTRY:
            continue
        touched = flat(b["addr"]) in cite or (
            bool(b["guard"]) and flat(b["guard"]["addr"]) in cite)
        if not touched:
            out.append(b)
    out.sort(key=lambda b: flat(b["addr"]))
    return out, cite


class CombatOpenerTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.exe = addrmod.read_exe()
        cls.art = json.loads(ART.read_text(encoding="utf-8"))
        cls.uncited = json.loads(UNCITED.read_text(encoding="utf-8"))
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        cls.prog = re_query.Program()
        cls.md = DOC.read_text(encoding="utf-8")
        cls.spans = inline_spans(strip_fences(cls.md))
        cls.insns = sorted(
            (i for c, i in cls.aligned.items()
             if c.startswith("1000:") and LO <= i.off < HI),
            key=lambda i: i.off)

    # ---------------------------------------------------------------- helpers
    def at(self, c):
        if c not in self.aligned:
            self.fail("%s is not an instruction boundary reached by decoding "
                      "its function from its own entry" % c)
        return self.aligned[c]

    def walk_records(self, node, path="$"):
        """Every `{addr, text}` pair anywhere in the artifact."""
        if isinstance(node, dict):
            if isinstance(node.get("addr"), str) and \
                    isinstance(node.get("text"), str):
                yield path, node["addr"], node["text"]
            for k, v in node.items():
                yield from self.walk_records(v, "%s.%s" % (path, k))
        elif isinstance(node, list):
            for n, v in enumerate(node):
                yield from self.walk_records(v, "%s[%d]" % (path, n))

    def walk_citations(self, node, path="$"):
        """Every value that is a bare `1000:xxxx` citation."""
        if isinstance(node, dict):
            for k, v in node.items():
                yield from self.walk_citations(v, "%s.%s" % (path, k))
        elif isinstance(node, list):
            for n, v in enumerate(node):
                yield from self.walk_citations(v, "%s[%d]" % (path, n))
        elif isinstance(node, str) and re.fullmatch(r"1000:[0-9a-f]{4}", node):
            yield path, node

    # ----------------------------------------------------- alignment/identity
    def test_every_recorded_address_is_an_aligned_instruction_boundary(self):
        seen = 0
        for _path, c in self.walk_citations(self.art):
            self.at(c)
            seen += 1
        self.assertGreater(seen, 40,
                           "the citation walk found almost nothing, so it is "
                           "measuring the walker rather than the artifact")

    def test_every_recorded_instruction_decodes_to_what_the_artifact_says(self):
        seen = 0
        for path, c, text in self.walk_records(self.art):
            got = self.at(c).text
            self.assertEqual(
                text, got,
                "data/combat_opener.json %s says %s is %r; orig/g.exe decodes "
                "%r there" % (path, c, text, got))
            seen += 1
        self.assertGreater(seen, 30,
                           "the {addr,text} walk found almost nothing")

    # ------------------------------------------------------------ the extent
    def test_the_range_tiles_and_its_instruction_count_is_measured(self):
        at = LO
        for i in self.insns:
            self.assertEqual(i.off, at,
                             "the aligned walk skips %s" % cit(at))
            at = i.end
        self.assertEqual(at, HI,
                         "the walk overruns the half-open end %s" % cit(HI))
        self.assertEqual(
            self.art["range"]["instruction_count"], len(self.insns),
            "data/combat_opener.json records %d instructions; the aligned "
            "walk finds %d" % (self.art["range"]["instruction_count"],
                               len(self.insns)))

    def test_the_only_exit_is_the_one_the_artifact_records(self):
        out = []
        for i in self.insns:
            m = TRANSFER.match(i.text)
            if not m:
                continue
            target = int(m.group(2), 16)
            if not LO <= target < HI:
                out.append((cit(i.off), i.text))
        want = [(self.art["exit"]["site"]["addr"],
                 self.art["exit"]["site"]["text"])]
        self.assertEqual(
            sorted(out), sorted(want),
            "the transfers leaving 1000:3d32..1000:3e8d and the artifact's "
            "`exit` disagree")
        self.assertEqual(
            "1000:%04x" % int(TRANSFER.match(want[0][1]).group(2), 16),
            self.art["exit"]["target"],
            "the exit instruction's own target and `exit.target` disagree")

    def test_the_only_entries_are_the_two_the_artifact_records(self):
        into = []
        for c, i in self.aligned.items():
            if not c.startswith("1000:") or LO <= i.off < HI:
                continue
            m = TRANSFER.match(i.text)
            if m and LO <= int(m.group(2), 16) < HI:
                into.append((c, i.text))
        want = [(s["addr"], s["text"])
                for s in self.art["entered_by"]["sites"]]
        self.assertEqual(sorted(into), sorted(want),
                         "the image-wide sweep for branches into the range "
                         "and the artifact's `entered_by` disagree")

    # ---------------------------------------------------------- the literals
    def test_the_recorded_strings_are_every_cs_literal_pushed(self):
        swept = []
        for n, i in enumerate(self.insns):
            m = re.fullmatch(r"mov di,0x([0-9a-f]+)", i.text)
            if not m:
                continue
            if n + 2 < len(self.insns) and \
                    self.insns[n + 1].text == "push cs" and \
                    self.insns[n + 2].text == "push di":
                swept.append((cit(i.off), int(m.group(1), 16)))
        recorded = []
        for arm in self.art["arms"]:
            for s in arm["strings"]:
                recorded.append((s["push"]["addr"], int(s["cs_offset"], 16)))
        self.assertEqual(
            sorted(swept), sorted(recorded),
            "the CS-literal push sweep and data/combat_opener.json disagree")

    def test_every_recorded_string_re_decodes_from_the_binary(self):
        seen = 0
        for arm in self.art["arms"]:
            for s in arm["strings"]:
                cs = int(s["cs_offset"], 16)
                fo = cs + HDR
                self.assertEqual(
                    "0x%05x" % fo, s["file_offset"],
                    "%s: file_offset is not cs_offset + 0x18d0"
                    % s["push"]["addr"])
                n = self.exe[fo]
                self.assertEqual(n, s["length"],
                                 "%s: the recorded length is not the length "
                                 "byte in orig/g.exe" % s["file_offset"])
                got = self.exe[fo + 1:fo + 1 + n].decode("cp866")
                self.assertEqual(
                    got, s["text"],
                    "data/combat_opener.json says %s holds %r; orig/g.exe "
                    "holds %r" % (s["file_offset"], s["text"], got))
                seen += 1
        # Not a literal 9: `sweeps.cs_literal_pushes` is itself checked
        # against a fresh count of the range, so this is the derived number.
        self.assertEqual(seen, self.art["sweeps"]["cs_literal_pushes"],
                         "the artifact stopped recording one of the strings "
                         "the CS-literal push sweep finds")

    # ------------------------------------------------------------- the gates
    def test_the_gate_inventory_is_every_conditional_branch_in_range(self):
        swept = sorted(cit(i.off) for i in self.insns
                       if re.match(r"^j(?!mp\b)[a-z]+\s", i.text))
        recorded = sorted(g["branch"]["addr"]
                          for arm in self.art["arms"] for g in arm["gates"])
        self.assertEqual(
            swept, recorded,
            "every conditional branch in the range must be a recorded gate")
        self.assertEqual(len(swept), self.art["sweeps"]
                         ["conditional_branches"],
                         "sweeps.conditional_branches is not the number of "
                         "conditional branches in the range")

    def test_the_gates_test_the_class_values_the_arms_claim_in_order(self):
        seq = []
        for arm in self.art["arms"]:
            for g in arm["gates"]:
                seq.append((off_of(g["compare"]["addr"]), g["class_value"]))
                self.assertEqual(
                    self.at(g["compare"]["addr"]).text,
                    "cmp ax,0x%x" % g["class_value"],
                    "%s does not compare against %d"
                    % (g["compare"]["addr"], g["class_value"]))
        self.assertEqual(
            [v for _, v in sorted(seq)], list(range(10)),
            "the chain must test 0..9 in ascending address order")
        cover = {int(k): v for k, v in self.art["class_map"].items()
                 if k != "default"}
        self.assertEqual(sorted(cover), list(range(10)))
        for arm in self.art["arms"]:
            if arm["gates"]:
                self.assertEqual(
                    sorted(arm["classes"]),
                    sorted(k for k, v in cover.items() if v == arm["name"]),
                    "%s's `classes` and the class_map disagree" % arm["name"])

    def test_each_gate_branches_where_the_artifact_says(self):
        for arm in self.art["arms"]:
            for g in arm["gates"]:
                i = self.at(g["branch"]["addr"])
                m = TRANSFER.match(i.text)
                self.assertIsNotNone(m, "%s is not a branch"
                                     % g["branch"]["addr"])
                self.assertEqual(cit(int(m.group(2), 16)), g["taken"],
                                 "%s does not branch to the recorded target"
                                 % g["branch"]["addr"])
                self.assertEqual(cit(i.end), g["fallthrough"],
                                 "%s's fall-through is not the recorded one"
                                 % g["branch"]["addr"])

    # ------------------------------------------------------------ the sweeps
    def test_the_range_spends_no_draw_and_the_sweep_that_says_so_works(self):
        in_range = [o for o in range(LO, HI - len(RANDOM_CALL) + 1)
                    if self.img[o:o + len(RANDOM_CALL)] == RANDOM_CALL]
        self.assertEqual(in_range, [],
                         "a Random call site inside the opener")
        self.assertEqual(self.art["sweeps"]["random_call_sites"], 0,
                         "sweeps.random_call_sites must be the measured 0")
        image_wide = sum(
            1 for o in range(len(self.img) - len(RANDOM_CALL) + 1)
            if self.img[o:o + len(RANDOM_CALL)] == RANDOM_CALL)
        self.assertEqual(
            image_wide, RANDOM_CALLS_IMAGE_WIDE,
            "the signature sweep must find the population "
            "docs/re/METHODOLOGY.md records, or the zero above is a scan that "
            "found nothing because it was looking wrongly")

    def test_the_range_writes_no_memory_and_every_operand_is_classified(self):
        writes, reads, lea, unclassified = [], [], [], []
        for i in self.insns:
            if "[" not in i.text:
                continue
            if i.text.startswith("lea "):
                lea.append((cit(i.off), i.text))
            elif WRITES_ABS_MEM.match(i.text):
                writes.append((cit(i.off), i.text))
            elif READS_ABS_MEM.match(i.text):
                reads.append((cit(i.off), i.text))
            else:
                unclassified.append((cit(i.off), i.text))
        self.assertEqual(unclassified, [],
                         "an instruction naming memory fell in no bucket")
        self.assertEqual(writes, [],
                         "the opener writes memory after all")
        self.assertEqual(self.art["sweeps"]["absolute_memory_writes"], 0,
                         "sweeps.absolute_memory_writes must be the measured "
                         "0 -- the opener is print-only")
        self.assertEqual(len(reads),
                         self.art["sweeps"]["absolute_memory_reads"],
                         "sweeps.absolute_memory_reads is not the number of "
                         "absolute reads in the range")
        self.assertEqual(len(reads) + len(lea),
                         self.art["sweeps"]["memory_operand_instructions"],
                         "sweeps.memory_operand_instructions is not the "
                         "number of instructions naming memory")
        recorded = sorted((s["addr"], s["text"])
                          for g in self.art["globals"]
                          if g["kind"] == "absolute_read"
                          for s in g["sites"])
        self.assertEqual(sorted(reads), recorded,
                         "the absolute-read sweep and the `absolute_read` "
                         "entries of `globals[]` disagree")
        for g in self.art["globals"]:
            self.assertEqual(g["written_in_range"], [])
            self.assertIn(g["kind"],
                          ("absolute_read", "immediate_ds_pointer"))
            for s in g["sites"]:
                # A DS pointer is an IMMEDIATE, never a memory operand: this
                # is what keeps the read census from counting it twice.
                if g["kind"] == "immediate_ds_pointer":
                    self.assertNotIn("[0x", self.at(s["addr"]).text,
                                     "%s is a memory operand, not an "
                                     "immediate DS pointer" % s["addr"])

    def test_the_remaining_sweep_counts_are_a_fresh_measurement(self):
        s = self.art["sweeps"]
        self.assertEqual(len(self.insns), s["instructions"])
        counted = collections.Counter()
        for i in self.insns:
            if re.match(r"^jmp\b", i.text):
                counted["unconditional_jumps"] += 1
            if i.text == "call 0xeed:0x1c2":
                counted["writeln_calls"] += 1
            if i.text == "call 0xf78:0xae7":
                counted["str_assign_calls"] += 1
            if i.text == "call 0xf78:0xb66":
                counted["str_append_calls"] += 1
            if i.text == "push ds":
                counted["ds_pointer_pushes"] += 1
            if i.text == "push ss":
                counted["ss_pointer_pushes"] += 1
        for k in ("unconditional_jumps", "writeln_calls", "str_assign_calls",
                  "str_append_calls", "ds_pointer_pushes",
                  "ss_pointer_pushes"):
            self.assertEqual(counted[k], s[k],
                             "sweeps.%s is %d; a fresh count says %d"
                             % (k, s[k], counted[k]))
        self.assertEqual(
            len([1 for n, i in enumerate(self.insns)
                 if re.fullmatch(r"mov di,0x[0-9a-f]+", i.text)
                 and n + 2 < len(self.insns)
                 and self.insns[n + 1].text == "push cs"]),
            s["cs_literal_pushes"],
            "sweeps.cs_literal_pushes is not the number of CS-literal "
            "pushes in the range")

    def test_the_arms_ds_pointer_pushes_are_every_push_ds_in_range(self):
        """SET EQUALITY, because the cardinalities used to match by accident.

        One revision held `mov di` operands and `push ds` in a single arm-level
        `ds_pointer_pushes` array -- three entries, matching
        `sweeps.ds_pointer_pushes: 3` while sharing exactly one address with
        it.  Nothing related the two, so the matching count read as a
        cross-check that did not exist.  These are now two fields, and this is
        the measurement that ties one of them to the sweep.
        """
        swept = sorted((cit(i.off), i.text) for i in self.insns
                       if i.text == "push ds")
        recorded = sorted((x["addr"], x["text"]) for arm in self.art["arms"]
                          for x in arm.get("ds_pointer_pushes", []))
        self.assertEqual(swept, recorded,
                         "the `push ds` sweep and the arms' "
                         "`ds_pointer_pushes` disagree")
        self.assertEqual(len(swept),
                         self.art["sweeps"]["ds_pointer_pushes"],
                         "sweeps.ds_pointer_pushes is not the number of "
                         "`push ds` in the range")
        operands = sorted((x["addr"], x["text"]) for arm in self.art["arms"]
                          for x in arm.get("ds_operands", []))
        self.assertTrue(operands, "no arm records a DGROUP operand")
        self.assertEqual(
            set(a for a, _ in operands) & set(a for a, _ in swept), set(),
            "`ds_operands` and `ds_pointer_pushes` overlap; they are "
            "different instructions and conflating them is the defect this "
            "split exists to prevent")
        for a, text in operands:
            self.assertTrue(text.startswith("mov di,"),
                            "%s is in `ds_operands` but is %r" % (a, text))

    def test_the_spans_tile_the_range(self):
        spans = sorted((off_of(s["start"]), off_of(s["end"]), s["name"])
                       for s in self.art["spans"])
        at = LO
        for lo, hi, name in spans:
            self.assertEqual(lo, at, "%s leaves a gap or overlaps" % name)
            self.assertLess(lo, hi, "%s is empty or inverted" % name)
            self.assertIn(cit(lo), self.aligned,
                          "%s does not start on an instruction" % name)
            at = hi
        self.assertEqual(at, HI, "the spans stop short of %s" % cit(HI))

    # ---------------------------------------------------------- the DS census
    def test_the_key_xref_census_is_reproduced_by_running_the_query(self):
        want = self.art["key"]["xrefs"]
        scan = re_query.xrefs_to(self.prog, self.art["key"]["ds"])["scan"]
        self.assertEqual(scan["raw_hits"], want["raw_hits"],
                         "the recorded raw_hits for the key is not what "
                         "re_query.xrefs_to finds")
        self.assertEqual(len(scan["accepted"]), want["accepted"],
                         "the recorded accepted count for the key is not "
                         "what re_query.xrefs_to finds")
        self.assertEqual(len(scan["discarded"]), want["discarded"],
                         "the recorded discarded count for the key is not "
                         "what re_query.xrefs_to finds")
        by_fn = collections.Counter(x["function"] for x in scan["accepted"])
        self.assertEqual(dict(by_fn), want["by_function"],
                         "the recorded by_function census for the key is not "
                         "what re_query.xrefs_to finds")
        inside = [(x["at"], x["text"]) for x in scan["accepted"]
                  if x["at"].startswith("1000:")
                  and 0x3d11 <= off_of(x["at"]) < 0x584c]
        self.assertEqual(
            sorted(inside),
            sorted((r["at"], r["text"]) for r in want["in_fun_1000_3d11"]),
            "the recorded references inside FUN_1000_3d11 and the query "
            "disagree")
        for r in want["in_fun_1000_3d11"]:
            self.assertTrue(r["text"].startswith("mov "),
                            "%s is not a load" % r["at"])
            self.assertNotIn("[0x3952],", r["text"].split(",")[0] + ",",
                             "%s writes the key" % r["at"])

    # ------------------------------------------------- the classification set
    def test_combat_uncited_covers_exactly_the_derived_uncited_set(self):
        """The `never-cited` half of the partition, against a fresh derivation.

        Until Task 40 this asserted that EVERY row was still uncited, which
        that task necessarily broke by doing its job.  The claim that survives
        is the one worth keeping: whatever a fresh recomputation of
        `docs/re/branches.md`'s Coverage rule still reports as uncited for
        this function must be in this file, marked `never-cited`, and nothing
        else may carry that mark.  So a branch that loses its citation, and a
        row parked under the wrong status, both go red.

        The second equality is what stops a row being invented under the
        `cited` mark, where the first cannot see it: every row must be a
        `class == "game"` branch of `FUN_1000_3d11` in `data/branches.json`.
        """
        derived, _ = uncited_branches(self.branches, REPO)
        self.assertEqual(
            [r["addr"] for r in self.uncited["branches"]
             if r["port_status"] == "never-cited"],
            [b["addr"] for b in derived],
            "data/combat_uncited.json and a fresh recomputation of "
            "docs/re/branches.md's Coverage rule disagree about which "
            "branches of %s are uncited" % FUNC_ENTRY)
        game = [b["addr"] for b in self.branches["branches"]
                if b["class"] == "game" and b["func_entry"] == FUNC_ENTRY]
        self.assertEqual(
            [r["addr"] for r in self.uncited["branches"]
             if r["addr"] not in set(game)], [],
            "a fresh recomputation says this row is not a `class == \"game\"` "
            "branch of %s at all" % FUNC_ENTRY)
        self.assertEqual(self.uncited["counts"]["still_uncited"], len(derived),
                         "the recorded total is not the derived one")
        self.assertGreater(len(derived), 0,
                           "the derivation produced nothing, so the equality "
                           "above compares two empty lists")
        self.assertLess(len(derived), len(self.uncited["branches"]),
                        "every row is still uncited, so the `cited` half of "
                        "the partition is asserted by nothing")

    def test_the_recorded_counts_are_the_tally_of_the_rows(self):
        counts = collections.Counter(r["class"]
                                     for r in self.uncited["branches"])
        for k in ("implemented", "unimplemented", "unreachable-in-port"):
            self.assertEqual(counts[k], self.uncited["counts"][k],
                             "counts.%s is %d; the rows tally %d"
                             % (k, self.uncited["counts"][k], counts[k]))
        self.assertEqual(
            sum(self.uncited["counts"][k] for k in
                ("implemented", "unimplemented", "unreachable-in-port")),
            self.uncited["counts"]["total"],
            "the three classes do not sum to the total")
        self.assertEqual(set(counts) | {"unreachable-in-port"},
                         {"implemented", "unimplemented",
                          "unreachable-in-port"},
                         "a row carries a class this schema does not define")

    def test_every_uncited_row_decodes_to_what_it_says(self):
        # Every GAME BRANCH of the function, not only the ones still uncited:
        # since Task 40 most rows carry a citation, and their taken /
        # fallthrough / guard columns must still agree with
        # `data/branches.json`.
        derived = {b["addr"]: b for b in self.branches["branches"]
                   if b["class"] == "game" and b["func_entry"] == FUNC_ENTRY}
        for r in self.uncited["branches"]:
            self.assertEqual(self.at(r["addr"]).text, r["text"],
                             "%s does not decode to %r"
                             % (r["addr"], r["text"]))
            b = derived[r["addr"]]
            self.assertEqual(r["taken"], b["taken"],
                             "%s: taken disagrees with data/branches.json"
                             % r["addr"])
            self.assertEqual(r["fallthrough"], b["fallthrough"],
                             "%s: fallthrough disagrees with "
                             "data/branches.json" % r["addr"])
            guard = b["guard"]["addr"] if b["guard"] else None
            self.assertEqual(r["guard"], guard,
                             "%s: guard disagrees with data/branches.json"
                             % r["addr"])
            if guard:
                self.assertEqual(self.at(guard).text, r["guard_text"],
                                 "%s: guard %s does not decode to %r"
                                 % (r["addr"], guard, r["guard_text"]))
            else:
                self.assertIsNone(r["guard_text"])

    def test_every_implemented_row_names_the_function_that_evaluates_it(self):
        """The `implemented` pairing, checked at FUNCTION scope.

        What this establishes: the named module exists, it defines the named
        function exactly once, and every name the row's `src.expr` mentions
        occurs INSIDE THAT FUNCTION'S BODY.

        What that is worth, measured rather than asserted: of the 101
        `implemented` rows, **55 are pinned to exactly one of the functions
        their module names** -- moving them to any other would go red -- and
        46 would still pass under at least one sibling (`Game::spoil_charm`
        and `Game::spoil_glasses` share `below_at` and `has_mobile`, so an
        expression naming only those cannot tell them apart).  Zero rows have
        nothing checkable.  So this is a real filter, not a proof, and
        `src_pairing_check` in the artifact carries the floor that
        `test_the_function_scope_discriminates_as_well_as_recorded` ratchets:
        the number can be raised by writing more distinctive expressions, and
        cannot silently fall.

        The scope is the whole point, and an earlier revision of this test got
        it wrong.  It searched the whole module, which for 81 of the 101 rows
        is `src/game.rs` -- 10971 lines and eleven different functions named
        across those rows.  Under that check **all 101 rows passed no matter
        which of the eleven they named**, so the realistic defect (a row filed
        against `Game::claim_spoils` when the condition lives in
        `Game::spoil_club`, both in `src/game.rs`) was invisible, and the
        mutation case defending it only ever swapped MODULES -- the one case
        the module-wide search could catch.  A guard written against one
        symptom rather than the class, presented as the falsifier for the
        class: the defect `docs/re/METHODOLOGY.md` names, in the test whose
        job was to prevent it.  `combat-uncited-src-function` is the case that
        now exercises the in-module swap.

        What it still does NOT establish: that the Rust expression computes
        the same ANSWER as the original's compare.  That rests on the decode
        recorded in each row's `what`, and on a reviewer reading the pair.
        """
        bodies = {}
        seen = 0
        for r in self.uncited["branches"]:
            if r["class"] != "implemented":
                self.assertNotIn("src", r,
                                 "%s is not `implemented` but names a src "
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
                span,
                "%s names %s, and %s defines `fn %s` more than once or leaves "
                "it unterminated -- the span is ambiguous, so this check "
                "cannot be scoped and the row must name the function "
                "unambiguously" % (r["addr"], src["function"], src["module"],
                                   name))
            body = span[1]
            self.assertTrue(src["expr"].strip(),
                            "%s names no expression" % r["addr"])
            found = set(EXPR_IDENT.findall(src["expr"])) - RUST_NOISE
            self.assertTrue(
                found,
                "%s pairs %s with an expression naming nothing checkable, so "
                "the pairing is asserted by nothing" % (r["addr"],
                                                        src["function"]))
            for ident in sorted(found):
                # The two causes are separated because they call for
                # different repairs -- and because a single message would make
                # the wrong-function case and the wrong-name case
                # indistinguishable, which `tools/test_mutate.py`'s
                # `test_no_two_cases_defend_the_same_assertion` correctly
                # refuses to accept as two channels.
                self.assertTrue(
                    ident in body,
                    "%s pairs %s with an expression naming `%s`, which occurs "
                    "%s (%s:%d..)"
                    % (r["addr"], src["function"], ident,
                       "ELSEWHERE IN THE MODULE but not in this function's "
                       "body -- the row names the wrong construct"
                       if ident in whole else
                       "NOWHERE IN THE MODULE at all -- the name is wrong, or "
                       "was renamed out from under the row",
                       src["module"], span[0]))
            seen += 1
        self.assertEqual(seen, self.uncited["counts"]["implemented"],
                         "the rows carrying a `src` construct and "
                         "counts.implemented disagree")

    def test_the_function_scope_discriminates_as_well_as_recorded(self):
        """The ratchet behind the docstring above.

        A scoped check that happens to accept every function anyway is the
        module-wide check with extra steps.  This measures how many rows the
        scope actually PINS -- i.e. would go red if the row named any other
        function its module names -- and holds it at or above the floor the
        artifact records.  `>=`, never `==`: a more distinctive expression can
        only raise it, and a weakened extractor or a widened noise list reds.
        """
        rec = self.uncited["src_pairing_check"]
        bodies, named = {}, collections.defaultdict(set)
        rows = [r for r in self.uncited["branches"]
                if r["class"] == "implemented"]
        for r in rows:
            mod = r["src"]["module"]
            if mod not in bodies:
                bodies[mod] = fn_bodies(
                    (REPO / mod).read_text(encoding="utf-8"))
            named[mod].add(r["src"]["function"].split("::")[-1])
        pinned = vacuous = 0
        for r in rows:
            mod = r["src"]["module"]
            name = r["src"]["function"].split("::")[-1]
            idents = set(EXPR_IDENT.findall(r["src"]["expr"])) - RUST_NOISE
            if not idents:
                vacuous += 1
            others = [n for n in named[mod] if n != name]
            if not any(all(i in bodies[mod][n][1] for i in idents)
                       for n in others):
                pinned += 1
        self.assertEqual(
            vacuous, rec["rows_with_no_checkable_identifier"],
            "%d row(s) name nothing checkable; the artifact records %d"
            % (vacuous, rec["rows_with_no_checkable_identifier"]))
        self.assertGreaterEqual(
            pinned, rec["rows_uniquely_pinned_floor"],
            "the function scope now pins only %d of the %d implemented rows, "
            "below the recorded floor of %d -- the check got weaker"
            % (pinned, len(rows), rec["rows_uniquely_pinned_floor"]))
        self.assertLess(
            pinned, len(rows),
            "every row is pinned, so the artifact's floor and its caveat are "
            "stale: raise the floor and drop the caveat rather than leaving a "
            "claim weaker than the check")

    def test_the_function_span_finder_is_scoped_and_disjoint(self):
        """`fn_bodies` must really narrow the search, not just look like it.

        Without this, a bug that returned the whole module for every function
        would leave the check above green and useless -- which is precisely
        the state the previous revision was in.  So: every span the rows name
        is a strict subset of its module, the spans of two different functions
        in one module never overlap, and each starts on its own `fn` line.
        """
        by_module = collections.defaultdict(set)
        for r in self.uncited["branches"]:
            if r["class"] == "implemented":
                by_module[r["src"]["module"]].add(
                    r["src"]["function"].split("::")[-1])
        self.assertGreater(len(by_module), 1)
        for module, names in sorted(by_module.items()):
            text = (REPO / module).read_text(encoding="utf-8")
            found = fn_bodies(text)
            spans = []
            for name in sorted(names):
                span = found.get(name)
                self.assertIsNotNone(span, "%s: no unique `fn %s`"
                                     % (module, name))
                start, body = span
                self.assertLess(len(body), len(text),
                                "%s: the span for `fn %s` is the whole module"
                                % (module, name))
                self.assertRegex(body.splitlines()[0], r"\bfn %s\b"
                                 % re.escape(name))
                spans.append((start, start + len(body.splitlines()) - 1, name))
            spans.sort()
            for (a0, a1, an), (b0, b1, bn) in zip(spans, spans[1:]):
                self.assertLess(a1, b0,
                                "%s: the spans for `fn %s` and `fn %s` overlap"
                                % (module, an, bn))

    def test_every_port_equivalence_names_implemented_rows(self):
        """The equivalences describe rows, so they must describe real ones.

        An entry naming an address that is not in the file -- or one that is
        `unimplemented`, where there is no port predicate to be equivalent TO
        -- is a note about nothing, which is how an inventory stops being
        falsifiable.
        """
        by_addr = {r["addr"]: r for r in self.uncited["branches"]}
        seen = set()
        for e in self.uncited["port_equivalences"]:
            for key in ("original", "here", "why_it_is_the_same_decision"):
                self.assertTrue(e[key].strip(), "%s is empty" % key)
            for a in e["rows"]:
                self.assertIn(a, by_addr,
                              "port_equivalences names %s, which is not a "
                              "row of this file" % a)
                self.assertEqual(
                    by_addr[a]["class"], "implemented",
                    "port_equivalences names %s, which is %s -- there is no "
                    "port predicate for it to be equivalent to"
                    % (a, by_addr[a]["class"]))
                self.assertNotIn(a, seen,
                                 "%s is claimed by two equivalences" % a)
                seen.add(a)
        self.assertGreater(len(seen), 0,
                           "the equivalence walk found nothing, so the checks "
                           "above ran on no data")

    def test_every_cited_row_really_is_cited_where_it_says(self):
        """The `cited` half of the partition, and it reports WHERE.

        A row marked `cited` whose address and guard appear in no `src/` file
        is a claim asserted by nothing; a row marked `never-cited` that has
        acquired a citation is this artifact going stale.  Both go red here,
        and this test names the file and line, which
        `test_combat_uncited_covers_exactly_the_derived_uncited_set` cannot do
        -- that is why the two are kept apart rather than folded together.

        Before Task 40 this test was `test_no_row_is_cited_in_the_port_yet`
        and asserted the stronger, temporary claim that no row was cited at
        all.  Task 40 cited 115 of the 117; what is left is the claim that
        each row's recorded status matches the citation index.
        """
        _, cite = uncited_branches(self.branches, REPO)
        by_addr = {b["addr"]: b for b in self.branches["branches"]}
        wrong, cited = [], 0
        for r in self.uncited["branches"]:
            b = by_addr.get(r["addr"])
            where = sorted(cite.get(flat(r["addr"]), []))
            if b and b["guard"]:
                where += sorted(cite.get(flat(b["guard"]["addr"]), []))
            if r["port_status"] == "cited":
                cited += 1
                if not where:
                    wrong.append("%s is recorded `cited` and NOTHING in "
                                 "src/**.rs names it or its guard"
                                 % r["addr"])
            elif where:
                wrong.append("%s is recorded `%s` and is now cited at %s"
                             % (r["addr"], r["port_status"],
                                ", ".join(where)))
        self.assertEqual(wrong, [],
                         "data/combat_uncited.json's `port_status` and the "
                         "citation index disagree")
        self.assertEqual(cited, self.uncited["counts"]["cited"],
                         "counts.cited is %d; the rows tally %d"
                         % (self.uncited["counts"]["cited"], cited))
        self.assertGreater(cited, 0,
                           "no row is marked cited, so the walk above "
                           "checked nothing")

    def test_the_two_never_taken_rows_are_excluded_from_citation(self):
        """The controller ruling, made executable rather than remembered.

        `1000:56ba` and `1000:5760` are `mov al,1` / `or al,al` / `jz`: `al`
        holds an immediate 1, `or` clears ZF for any non-zero operand, so the
        jump can never be taken.  The port omits them, and they are
        permanently excluded from citation -- writing either address beside
        code that decides something else would move `port_touched` by two
        while making the map lie, which is the failure this whole lane is
        built around.

        Nothing here is trusted from the artifact: the three-instruction shape
        is decoded out of `orig/g.exe`, and the exclusion is checked against
        the citation index the port actually ships.
        """
        rec = self.uncited["excluded_from_citation"]
        _, cite = uncited_branches(self.branches, REPO)
        self.assertEqual(
            [e["addr"] for e in rec["rows"]],
            [r["addr"] for r in self.uncited["branches"]
             if r["port_status"] == "never-cited"],
            "the exclusion list and the `never-cited` rows disagree")
        self.assertGreater(len(rec["rows"]), 0,
                           "the exclusion list is empty, so the checks below "
                           "ran on no data")
        for e in rec["rows"]:
            load = self.at(e["load"])
            self.assertEqual(load.text, e["load_text"],
                             "%s does not decode to %r"
                             % (e["load"], e["load_text"]))
            self.assertEqual(load.text, "mov al,0x1",
                             "%s is not the immediate-1 load the ruling "
                             "rests on" % e["load"])
            self.assertEqual(self.at(e["guard"]).text, "or al,al",
                             "%s is not the `or al,al` that would have to set "
                             "ZF" % e["guard"])
            self.assertTrue(self.at(e["addr"]).text.startswith("jz "),
                            "%s is not a `jz`, so the never-taken argument "
                            "does not apply to it" % e["addr"])
            # And the three really are consecutive: a `mov al,1` somewhere
            # else in the function would prove nothing about this `or`.
            self.assertEqual(load.end, off_of(e["guard"]),
                             "%s is not the instruction immediately before "
                             "%s" % (e["load"], e["guard"]))
            self.assertEqual(self.at(e["guard"]).end, off_of(e["addr"]),
                             "%s is not the instruction immediately before "
                             "%s" % (e["guard"], e["addr"]))
            # Only the two addresses `port_touched` reads -- the branch's own
            # and its guard's. The `mov al,1` two instructions earlier is NOT
            # one of them, and `src/game.rs` names both copies of it while
            # describing the artefact (`grep -n '1000:56b6\|1000:575c'
            # src/game.rs`), which is fine and must not be confused with
            # citing the branch.
            for a in (e["addr"], e["guard"]):
                self.assertNotIn(
                    flat(a), cite,
                    "%s is cited in the port, and the ruling in "
                    "data/combat_uncited.json says it never may be" % a)

    # --------------------------------------------------------------- the doc
    def test_every_prose_address_is_an_instruction_boundary(self):
        seen = set()
        for c in re.findall(r"\b1000:[0-9a-f]{4}\b", self.md):
            self.at(c)
            seen.add(c)
        self.assertGreater(len(seen), 40,
                           "the prose scan found almost no citations, so it "
                           "is measuring the scanner")

    def test_every_prose_span_pairing_an_address_with_an_instruction_decodes(
            self):
        checked = 0
        for span in self.spans:
            m = re.fullmatch(r"(1000:[0-9a-f]{4})\s+(.+)", span)
            if not m:
                continue
            got = self.at(m.group(1)).text
            self.assertEqual(
                " ".join(m.group(2).split()), got,
                "docs/re/combat-opener.md says `%s`; orig/g.exe decodes %r"
                % (span, got))
            checked += 1
        self.assertGreaterEqual(
            checked, 5,
            "the inline-span pairing found almost nothing, which is what a "
            "backtick desync looks like")

    def test_every_prose_string_is_a_literal_the_artifact_records(self):
        known = {s["text"] for arm in self.art["arms"]
                 for s in arm["strings"]}
        # Only the table rows that pair a file offset with a text are checked:
        # those are the ones claiming to quote the binary.
        rows = re.findall(
            r"\|\s*`1000:[0-9a-f]{4}`\s*\|\s*`0x[0-9a-f]{4}`\s*\|"
            r"\s*`(0x[0-9A-F]{4})`\s*\|\s*`([^`]*)`\s*\|", self.md)
        self.assertEqual(len(rows), self.art["sweeps"]["cs_literal_pushes"],
                         "the string table in the prose does not have one "
                         "row per CS literal the range pushes")
        for file_off, text in rows:
            self.assertIn(text, known,
                          "the prose quotes %r, which the artifact does not "
                          "record" % text)
            fo = int(file_off, 16)
            n = self.exe[fo]
            self.assertEqual(
                self.exe[fo + 1:fo + 1 + n].decode("cp866"), text,
                "the prose pairs %s with %r; orig/g.exe disagrees"
                % (file_off, text))


if __name__ == "__main__":
    unittest.main(verbosity=2)
