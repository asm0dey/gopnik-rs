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
    def test_the_row_5_ceiling_is_a_different_number_not_a_different_spelling(self):
        """The gym's own residue of `menu_vs_arm_finding`.

        `tools/test_arms_artifacts.py` checks every recorded pair's bytes,
        its two `jcc`s and the longest run the two blocks share, for all
        three artifacts.  What is only true HERE is the row-5 substitution:
        identical head, identical tail, `shl ax,1` against
        `dec`/`dec`/`mov dx,10`/`mul dx` in between -- a different NUMBER,
        not a different spelling of the same one.  That is what makes the
        `5` arm reachable through a menu that no longer lists it.
        """
        f = self.art["menu_vs_arm_finding"]
        row5 = next(p for p in f["pairs"] if "armour" in p["what"])
        mb = self.img[off_of(row5["menu"]["start"]):off_of(row5["menu"]["end"])]
        ab = self.img[off_of(row5["arm"]["start"]):off_of(row5["arm"]["end"])]
        self.assertEqual(mb[:5], ab[:5], "row 5: the heads differ")
        self.assertEqual(mb[-9:], ab[-9:], "row 5: the tails differ")
        self.assertEqual(mb[5:-9].hex(" "), "d1 e0",
                         "row 5: the menu's middle is not `shl ax,1`")
        self.assertEqual(ab[5:-9].hex(" "), "48 48 ba 0a 00 f7 e2",
                         "row 5: the arm's middle is not "
                         "`dec ax`/`dec ax`/`mov dx,0xa`/`mul dx`")
        self.assertNotEqual(mb, ab, "row 5: the two predicates are the same "
                                    "bytes, so there is no substitution")


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

    def test_the_prose_carries_the_joint_and_the_armour_recompute(self):
        """The non-arm half of the prose/artifact agreement.

        `tools/test_arms_artifacts.py` checks the ARMS half for all three
        artifacts. `kos`'s effects and the trained-armour recompute's steps
        belong to neither an arm nor another map, so they stay here.
        """
        for e in self.art["joint"]["effects"]:
            self.assertIn(
                e["addr"], self.md,
                "the prose does not carry the joint's effect at %s"
                % e["addr"])
        for s in self.art["abs_recompute"]["steps"]:
            self.assertIn(
                s["effect"]["addr"], self.md,
                "the prose does not carry the recompute step at %s"
                % s["effect"]["addr"])
        self.assertGreaterEqual(len(self.art["joint"]["effects"]), 5)
        self.assertGreaterEqual(len(self.art["abs_recompute"]["steps"]), 3)



if __name__ == "__main__":
    unittest.main(verbosity=2)
