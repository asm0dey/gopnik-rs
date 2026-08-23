#!/usr/bin/env python3
"""Tests for tools/rngtrace.

WHAT IS COVERED HERE (no emulator needed, runs anywhere):
  * the LCG predictor, against data/rng_vectors.json -- the committed ground
    truth produced by interpreting orig/g.exe's own instruction bytes;
  * the seed patch: exact site bytes, exact replacement, equal length, refusal
    on a site that does not match, and that orig/g.exe is never written;
  * the load-base derivation arithmetic, including the `- 0x18d0` header
    subtraction that separates FILE offsets from IMAGE offsets, on a synthetic
    MZ image loaded at a synthetic base;
  * the trace-log parser: draw lines, turn markers, call-site arithmetic;
  * every short-trace guard: no READY, no breakpoints, zero draws, an
    unexpected stop, a DROPPED draw (the LCG replay must catch it, because a
    missed draw desynchronises everything after it), and -- the one the first
    round of guards missed -- a gdb-script ABORT MID-WALK, which every other
    guard passes: gdb is alive, the log grew, the frozen screen still reads as
    the street prompt, and a truncated prefix replays perfectly;
  * the guest-memory verification that runs before a breakpoint is installed
    (wrong bytes at Random, the seed patch absent, RandSeed already stepped),
    and the final-RandSeed reconciliation, on synthetic memory;
  * the gdb script shape, as a regression on the reason it is a `while` loop
    (qemu reports $pc as the 16-bit offset, so breakpoint `commands` never run);
  * Task 13's FIGHT capture: the four-breakpoint script, the two extra log
    channels and every way one of their payloads can go missing, the
    per-sample LCG check that replaces the two-transport reconciliation on a
    run that died mid-turn, and the fight-span arithmetic the fold publishes.

WHAT IS NOT COVERED HERE (needs the emulator, and is NOT faked):
  * booting FreeDOS under qemu, the monitor protocol, sendkey, pmemsave, and
    the cp866 screen decode;
  * that a breakpoint on the guest's Random actually fires;
  * tools/rngtrace/driver.py's prompt handling.
  Those are exercised only by a real run of tools/rngtrace/run.py, whose own
  output is checked by the guards above; see docs/re/rng-trace.md.  No mock
  emulator stands in for them here -- a faked run would prove nothing.
"""
import hashlib
import json
import random
import re
import struct
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "tools"))

import addr  # noqa: E402  -- the address convention, defined once
from rngtrace import (combattrace, compare, driver, fightrun,  # noqa: E402
                      gdbsession, loadbase, rng, seedpatch, tracelog, vm)
from rngtrace import run as runmod  # noqa: E402


class TestRngPredictor(unittest.TestCase):
    """The predictor must agree with data/rng_vectors.json, which was produced
    by an 8086 interpreter over orig/g.exe -- not by the Rust port."""

    @classmethod
    def setUpClass(cls):
        cls.vectors = json.loads((REPO / "data" / "rng_vectors.json").read_text())

    def test_next_u32(self):
        for block in self.vectors["seeds"]:
            s = block["seed"]
            for i, expected in enumerate(block["next_u32"]):
                s = rng.step(s)
                self.assertEqual(s, expected,
                                 "seed %#x step %d" % (block["seed"], i + 1))

    def test_random_of_n(self):
        for block in self.vectors["seeds"]:
            for case in block["below"]:
                got = rng.predict(block["seed"], [case["n"]] * len(case["expected"]))
                self.assertEqual(got, case["expected"],
                                 "seed %#x n=%d" % (block["seed"], case["n"]))

    def test_random_zero_is_zero(self):
        _, r = rng.draw(0x12345678, 0)
        self.assertEqual(r, 0)


class TestSeedPatch(unittest.TestCase):
    def setUp(self):
        self.exe = (REPO / "orig" / "g.exe").read_bytes()

    def test_site_bytes_are_randomize(self):
        at = seedpatch.RANDOMIZE_FILE_OFF
        self.assertEqual(self.exe[at:at + 13], seedpatch.RANDOMIZE_ORIG)
        # mov ah,0x2c / int 0x21 / mov [0x367e],cx / mov [0x3680],dx / retf
        self.assertEqual(seedpatch.RANDOMIZE_ORIG[:4], bytes.fromhex("b42ccd21"))
        self.assertEqual(seedpatch.RANDOMIZE_ORIG[-1], 0xCB)

    def test_patch_is_same_length_and_writes_both_halves(self):
        p = seedpatch.build_patch(0xDEADBEEF)
        self.assertEqual(len(p), len(seedpatch.RANDOMIZE_ORIG))
        self.assertEqual(p.hex(" "), "c7 06 7e 36 ef be c7 06 80 36 ad de cb")

    def test_patch_bytes_only_touches_the_site(self):
        patched, rec = seedpatch.patch_bytes(self.exe, 0x12345678)
        self.assertEqual(len(patched), len(self.exe))
        at = seedpatch.RANDOMIZE_FILE_OFF
        self.assertEqual(patched[:at], self.exe[:at])
        self.assertEqual(patched[at + 13:], self.exe[at + 13:])
        self.assertEqual(rec["bytes_before"], seedpatch.RANDOMIZE_ORIG.hex(" "))

    def test_refuses_a_site_that_does_not_match(self):
        broken = bytearray(self.exe)
        broken[seedpatch.RANDOMIZE_FILE_OFF] ^= 0xFF
        with self.assertRaises(ValueError):
            seedpatch.patch_bytes(bytes(broken), 1)

    def test_original_is_never_modified(self):
        before = hashlib.md5((REPO / "orig" / "g.exe").read_bytes()).hexdigest()
        with tempfile.TemporaryDirectory() as td:
            seedpatch.write_patched_copy(REPO / "orig" / "g.exe",
                                         Path(td) / "G.EXE", 0x1234)
        after = hashlib.md5((REPO / "orig" / "g.exe").read_bytes()).hexdigest()
        self.assertEqual(before, after)
        self.assertEqual(after, seedpatch.ORIG_MD5)


def synthetic_exe(image_len=0x11000, relocs=((0x1000, 0x40), (0x8000, 0x10),
                                             (0xD000, 0x88))):
    """An MZ whose header is the same 397 paragraphs as orig/g.exe."""
    rnd = random.Random(1234)
    image = bytearray(rnd.randbytes(image_len))
    header = bytearray(loadbase.HEADER_BYTES)
    header[0:2] = b"MZ"
    lfarlc = 0x40
    struct.pack_into("<H", header, 0x06, len(relocs))     # e_crlc
    struct.pack_into("<H", header, 0x08, loadbase.HEADER_BYTES // 16)
    struct.pack_into("<H", header, 0x18, lfarlc)          # e_lfarlc
    for i, (off, seg) in enumerate(relocs):
        struct.pack_into("<HH", header, lfarlc + i * 4, off, seg)
        struct.pack_into("<H", image, seg * 16 + off, 0x0123)  # the segment word
    return bytes(header) + bytes(image)


def synthetic_memory(exe, base, size=0x100000):
    image = bytearray(loadbase.load_image(exe))
    load_seg = base // 16
    for r in loadbase.parse_relocations(exe):
        word = (int.from_bytes(image[r:r + 2], "little") + load_seg) & 0xFFFF
        image[r:r + 2] = word.to_bytes(2, "little")
    mem = bytearray(size)
    mem[base:base + len(image)] = image
    return bytes(mem)


class TestLoadBase(unittest.TestCase):
    def test_offset_conventions_do_not_mix(self):
        # The convention itself lives in tools/addr.py and is tested against
        # the bytes of orig/g.exe in tools/test_addr.py; this only pins the
        # two entry points loadbase re-exports.
        # A Ghidra `1000:XXXX` citation is an IMAGE offset; file = 0x18d0 + it.
        self.assertEqual(loadbase.file_off_of_image_off(
            loadbase.image_off_of_ghidra(0x1000, 0xB353)), 0x18D0 + 0xB353)
        # A real seg:off like 0f78:114b is seg*16 + off, then + 0x18d0.
        self.assertEqual(loadbase.file_off_of_image_off(
            loadbase.image_off_of_seg_off(0x0F78, 0x114B)), 0x1219B)

    def test_real_exe_header_is_397_paragraphs(self):
        exe = (REPO / "orig" / "g.exe").read_bytes()
        hdrpara, = struct.unpack_from("<H", exe, 0x08)
        self.assertEqual(hdrpara * 16, loadbase.HEADER_BYTES)
        self.assertEqual(len(loadbase.parse_relocations(exe)), 1580)

    def test_derives_the_base_and_verifies_every_relocation(self):
        exe = synthetic_exe()
        for base in (0x224B0, 0x1A000, 0x30000):
            mem = synthetic_memory(exe, base)
            info = loadbase.derive(mem, exe)
            self.assertEqual(info["image_base"], base)
            self.assertEqual(info["load_seg"], base // 16)
            self.assertEqual(info["relocations_checked"], 3)
            self.assertEqual(loadbase.linear(info["image_base"], 0x108CB),
                             base + 0x108CB)

    def test_rejects_a_base_whose_relocations_do_not_agree(self):
        exe = synthetic_exe()
        base = 0x224B0
        mem = bytearray(synthetic_memory(exe, base))
        r = loadbase.parse_relocations(exe)[0]
        mem[base + r] ^= 0xFF
        with self.assertRaises(ValueError):
            loadbase.verify_base(bytes(mem), loadbase.load_image(exe),
                                 loadbase.parse_relocations(exe), base)

    def test_rejects_an_unaligned_base(self):
        exe = synthetic_exe()
        with self.assertRaises(ValueError):
            loadbase.verify_base(bytes(0x100000), loadbase.load_image(exe),
                                 loadbase.parse_relocations(exe), 0x224B1)

    def test_fails_loudly_when_the_image_is_absent(self):
        exe = synthetic_exe()
        with self.assertRaises(ValueError):
            loadbase.derive(bytes(0x100000), exe)

    def test_anchor_avoids_relocations(self):
        exe = synthetic_exe()
        image = loadbase.load_image(exe)
        relocs = loadbase.parse_relocations(exe)
        a = loadbase.find_anchor(image, relocs, 0x1000 - 30, 64)
        self.assertTrue(all(not (a <= r < a + 64) for r in relocs))


SEED = 0x12345678

# A stand-in for run.state_fields(): three fields are enough to check the
# printf's shape and the parser's column discipline, and keep these tests
# independent of how wide the real table happens to be.  The addresses come
# from tools/addr.py through loadbase, never written out as literals.
STATE_FIELDS = [
    ("money_38c7", loadbase.DATA_SEG_IMAGE_OFF + 0x38C7, 2),
    ("district_3692", loadbase.DATA_SEG_IMAGE_OFF + 0x3692, 1),
    ("randseed_367e", loadbase.IMAGE_OFF_RANDSEED, 4),
]
STATE_NAMES = [n for n, _, _ in STATE_FIELDS]


# Verbatim gdb chatter from the committed runs' logs, styled glyphs included.
GDB_PREAMBLE = [
    "\u26a0\ufe0f warning: A handler for the OS ABI \"GNU/Linux\" is not built into this configuration",
    "of GDB.  Attempting to continue with the default i8086 settings.",
    "",
    "The target architecture is set to \"i8086\".",
    "\u26a0\ufe0f warning: No executable has been specified and target does not support",
    "determining executable automatically.  Try using the \"file\" command.",
    "0x0000bdf4 in ?? ()",
]
GDB_STOP = ["", "Program received signal SIGTRAP, Trace/breakpoint trap.",
            "0x00001165 in ?? ()"]


def synth_log(draws, prompts_before=(), base=0x224B0, ready=True, bp=2,
              unexpected=(), abort=None, chatter=False, trailing=(),
              state=None):
    """A gdb log shaped exactly like the real one.

    `abort` is the gdb "Error in sourced command file:" pair: the harness's own
    shutdown always reports `Remote connection closed`, and anything else means
    a command inside the trace loop failed with the guest still stopped.

    `state` is the per-turn state channel: an ordered list of value lists, one
    per prompt stop, printed as the real script prints them (`P`, then `S`
    followed by one lowercase `%x` per field of `STATE_FIELDS`).  `None` means
    a log from before Task 11i, which carries no `S` lines at all.
    """
    out = []
    if chatter:
        out.extend(GDB_PREAMBLE)
    if bp:
        for i in range(bp):
            out.append("Breakpoint %d at 0x%x" % (i + 1, base + i))
    if ready:
        out.append("READY base=%x retf=%x readln=%x"
                   % (base, base + 0x108E5, base + 0xAE63))
    seen_prompts = 0
    for i, d in enumerate(draws):
        if i in prompts_before:
            out.append("P")
            if state is not None:
                out.append("S " + " ".join("%x" % v for v in state[seen_prompts]))
            seen_prompts += 1
        if chatter:
            out.extend(GDB_STOP)
        out.append("R %04x %04x %04x %04x"
                   % (d["ret_off"], d.get("ret_seg", 0x224B), d["n"], d["result"]))
    for pc in unexpected:
        out.append("? %04x" % pc)
    out.extend(trailing)
    if abort:
        out.append("\u274c\ufe0f build/rngtrace/run/trace.gdb:25: Error in sourced command file:")
        out.append(abort)
        out.append("(gdb)")
    return "\n".join(out) + "\n"


def real_stream(ns, seed=SEED, sites=None):
    """Draws that a correct tracer would log for `ns`, on the pinned seed."""
    results = rng.predict(seed, ns)
    sites = sites or [0xB353] * len(ns)
    return [{"ret_off": (s + 5) & 0xFFFF, "n": n, "result": r}
            for s, n, r in zip(sites, ns, results)]


class TestTraceLog(unittest.TestCase):
    def test_parses_draws_and_call_sites(self):
        draws = real_stream([20, 20, 10, 10], sites=[0xAF68, 0xAFC7, 0xB186, 0xB1B8])
        parsed = tracelog.parse(synth_log(draws))
        self.assertEqual(len(parsed["draws"]), 4)
        self.assertEqual([hex(d["call_site_offset"]) for d in parsed["draws"]],
                         ["0xaf68", "0xafc7", "0xb186", "0xb1b8"])
        self.assertEqual(parsed["draws"][0]["n"], 20)
        self.assertEqual(parsed["ready"]["image_base"], 0x224B0)

    def test_strips_interleaved_gdb_prompt(self):
        parsed = tracelog.parse("(gdb) R af6d 224b 0014 0000\n")
        self.assertEqual(len(parsed["draws"]), 1)
        self.assertEqual(parsed["draws"][0]["call_site_offset"], 0xAF68)

    def test_turn_markers_segment_the_stream(self):
        draws = real_stream([20, 20, 25, 20, 20, 25])
        parsed = tracelog.parse(synth_log(draws, prompts_before={0, 3}))
        turns = tracelog.group_by_turn(parsed["draws"])
        self.assertEqual(sorted(turns), [1, 2])
        self.assertEqual(len(turns[1]), 3)
        self.assertEqual(len(turns[2]), 3)

    def test_replay_matches_a_correct_stream(self):
        draws = real_stream([20, 20, 10, 10, 100, 100, 25, 200, 100])
        parsed = tracelog.parse(synth_log(draws))
        v = tracelog.verify(parsed, SEED)
        self.assertEqual(v["lcg_replay"], "match")
        self.assertEqual(v["draws_verified"], 9)
        self.assertEqual(v["leading_states_skipped"], 0)

    def test_replay_catches_a_DROPPED_draw(self):
        """The guard that matters: a tracer that under-reports must fail, not
        emit a short trace that reads as evidence of absence."""
        ns = [20, 20, 10, 10, 100, 100, 25, 200, 100]
        draws = real_stream(ns)
        del draws[4]                       # the tracer missed one
        parsed = tracelog.parse(synth_log(draws))
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.verify(parsed, SEED)
        self.assertIn("diverged", str(cm.exception))

    def test_replay_catches_a_wrong_result_read(self):
        draws = real_stream([20, 20, 10])
        draws[2]["result"] ^= 1
        parsed = tracelog.parse(synth_log(draws))
        with self.assertRaises(tracelog.TraceError):
            tracelog.verify(parsed, SEED)

    def test_replay_reports_draws_spent_before_the_attach(self):
        ns = [20, 20, 10]
        results = rng.predict(SEED, [200, 100] + ns)[2:]
        draws = [{"ret_off": 0xB358, "n": n, "result": r} for n, r in zip(ns, results)]
        parsed = tracelog.parse(synth_log(draws))
        skip, _ = tracelog.replay(parsed["draws"], SEED, max_skip=4)
        self.assertEqual(skip, 2)

    def test_zero_draws_is_an_error_not_an_empty_file(self):
        parsed = tracelog.parse(synth_log([]))
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.verify(parsed, SEED)
        self.assertIn("0 draws", str(cm.exception))

    def test_min_draws_guard(self):
        parsed = tracelog.parse(synth_log(real_stream([20, 20])))
        with self.assertRaises(tracelog.TraceError):
            tracelog.verify(parsed, SEED, min_draws=9)

    def test_missing_ready_is_an_error(self):
        parsed = tracelog.parse(synth_log(real_stream([20]), ready=False))
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.verify(parsed, SEED)
        self.assertIn("READY", str(cm.exception))

    def test_missing_breakpoint_is_an_error(self):
        parsed = tracelog.parse(synth_log(real_stream([20]), bp=1))
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.verify(parsed, SEED)
        self.assertIn("breakpoints", str(cm.exception))

    def test_unexpected_stop_is_an_error(self):
        parsed = tracelog.parse(synth_log(real_stream([20]), unexpected=(0x1234,)))
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.verify(parsed, SEED)
        self.assertIn("unexpected", str(cm.exception))


class TestGdbScript(unittest.TestCase):
    def test_breaks_on_the_derived_linear_address(self):
        s = gdbsession.build_script(0x224B0, 1234, STATE_FIELDS)
        self.assertIn("break *0x32d95", s)     # 0x224b0 + 0x108e5, Random's retf
        self.assertIn("break *0x2d313", s)     # 0x224b0 + 0xae63, the prompt ReadLn
        self.assertNotIn("0x224B0", s.replace("base=%x", ""))  # never hardcoded twice

    def test_dispatches_on_pc_not_on_breakpoint_commands(self):
        """Regression: qemu reports $pc as the 16-bit offset while the
        breakpoint is at the linear address, so gdb cannot attribute the stop
        and a `commands` block never runs.  The loop must dispatch on $pc."""
        s = gdbsession.build_script(0x224B0, 1234, STATE_FIELDS)
        self.assertIn("while 1", s)
        self.assertIn("if $pc == 0x1165", s)
        self.assertIn("if $pc == 0xae63", s)
        self.assertNotIn("commands", s)

    def test_steps_over_the_breakpoint_by_hand(self):
        """Regression on the retrap: gdb does not know a breakpoint is at the
        stop address, so it never removes/steps/reinserts, and QEMU re-traps
        the same instruction forever while the guest makes no progress."""
        s = gdbsession.build_script(0x224B0, 1234, STATE_FIELDS)
        loop = s.split("while 1", 1)[1]
        self.assertIn("disable", loop)
        self.assertIn("stepi", loop)
        self.assertIn("enable", loop)
        self.assertLess(loop.index("disable"), loop.index("stepi"))
        self.assertLess(loop.index("stepi"), loop.index("enable"))

    def test_reads_the_caller_frame_at_the_retf(self):
        s = gdbsession.build_script(0x224B0, 1234, STATE_FIELDS)
        self.assertIn("($ss*16+$sp)", s)      # return offset
        self.assertIn("($ss*16+$sp+2)", s)    # return segment
        self.assertIn("($ss*16+$sp+4)", s)    # n
        self.assertIn("$ax", s)               # result


class TestCatalogueComparison(unittest.TestCase):
    """The comparison itself: it must not launder a disagreement into a match."""

    @classmethod
    def setUpClass(cls):
        cls.wander = json.loads((REPO / "data" / "wander.json").read_text())

    def test_catalogue_has_all_eighteen_draws_in_order(self):
        cat = compare.catalogue(self.wander)
        self.assertEqual([c["draw_ordinal"] for c in cat], list(range(1, 19)))
        self.assertEqual(cat[0]["at"], "1000:af68")
        self.assertEqual(cat[11]["at"], "1000:b353")
        self.assertEqual(cat[11]["n"], 25)

    def _observed(self, site, n, count=1):
        return [{"call_site_offset": site, "n": n, "result": 0, "turn": 1}
                for _ in range(count)]

    def test_corroborated_when_site_and_n_match(self):
        cat = [c for c in compare.catalogue(self.wander) if c["draw_ordinal"] == 12]
        res, extra = compare.compare(cat, self._observed(0xB353, 25, 3),
                                     {"n_expr_values": {}, "state_note": ""})
        self.assertEqual(res[0]["verdict"], "corroborated")
        self.assertEqual(res[0]["observed_count"], 3)
        self.assertEqual(extra, {})

    def test_contradicted_when_n_differs(self):
        cat = [c for c in compare.catalogue(self.wander) if c["draw_ordinal"] == 12]
        res, _ = compare.compare(cat, self._observed(0xB353, 24),
                                 {"n_expr_values": {}, "state_note": ""})
        self.assertEqual(res[0]["verdict"], "contradicted")
        self.assertIn("catalogued n=25", res[0]["detail"])
        self.assertIn("observed n=[24]", res[0]["detail"])

    def test_not_observed_carries_the_gate(self):
        cat = [c for c in compare.catalogue(self.wander) if c["draw_ordinal"] == 9]
        res, _ = compare.compare(cat, [], {"n_expr_values": {}, "state_note": ""})
        self.assertEqual(res[0]["verdict"], "not observed")
        self.assertIn("ring", res[0]["why"])

    def test_computed_n_is_checked_against_the_expression(self):
        cat = [c for c in compare.catalogue(self.wander) if c["draw_ordinal"] == 10]
        ctx = {"n_expr_values": {"10": 60}, "state_note": "district 3"}
        good, _ = compare.compare(cat, self._observed(0xB2FA, 60), ctx)
        self.assertEqual(good[0]["verdict"], "corroborated")
        bad, _ = compare.compare(cat, self._observed(0xB2FA, 20), ctx)
        self.assertEqual(bad[0]["verdict"], "contradicted")

    def test_uncatalogued_sites_are_reported_and_located(self):
        cat = compare.catalogue(self.wander)
        # 1000:b100 is inside 1000:ae5a..1000:b3ba, the range the catalogue's
        # byte scan claims to enumerate completely, so a draw there would
        # contradict that claim; 1000:0efd and 1000:b54e are downstream of the
        # bucket dispatch, which the catalogue puts out of scope.
        obs = (self._observed(0xB100, 2) + self._observed(0x0EFD, 8)
               + self._observed(0xB54E, 2))
        _, extra = compare.compare(cat, obs, {"n_expr_values": {}, "state_note": ""})
        self.assertTrue(extra["1000:b100"]["inside_preamble_range"])
        self.assertFalse(extra["1000:0efd"]["inside_preamble_range"])
        self.assertFalse(extra["1000:b54e"]["inside_preamble_range"])

    def test_a_computed_n_with_no_context_is_an_error_not_a_fourth_verdict(self):
        """Three verdicts exist and only three.  compare() used to emit a fourth,
        `observed`, for a computed-`n` draw with no expected value -- silently,
        and outside anything downstream knows how to read."""
        cat = [c for c in compare.catalogue(self.wander) if c["draw_ordinal"] == 10]
        with self.assertRaises(ValueError) as cm:
            compare.compare(cat, self._observed(0xB2FA, 60),
                            {"n_expr_values": {}, "state_note": ""})
        self.assertIn("cannot judge", str(cm.exception))

    def test_draws_sharing_a_call_site_are_flagged_as_one_set_of_stops(self):
        """Draws 17 and 18 both fire at 1000:25fe.  Reported side by side with
        `observed_count: 2` each, they read as four independent observations in
        the machine-readable artifact; there were two stops in all."""
        cat = [c for c in compare.catalogue(self.wander)
               if c["draw_ordinal"] in (17, 18)]
        ctx = {"n_expr_values": {"17": 12, "18": 12}, "state_note": "class 3"}
        res, _ = compare.compare(cat, self._observed(0x25FE, 12, 2), ctx)
        self.assertEqual([r["shared_call_site"]["with_draws"] for r in res],
                         [[18], [17]])
        for r in res:
            self.assertIn("must not be added across these entries",
                          r["shared_call_site"]["note"])

    def test_a_draw_at_its_own_call_site_is_not_flagged(self):
        cat = [c for c in compare.catalogue(self.wander) if c["draw_ordinal"] == 12]
        res, _ = compare.compare(cat, self._observed(0xB353, 25),
                                 {"n_expr_values": {}, "state_note": ""})
        self.assertNotIn("shared_call_site", res[0])

    def test_the_committed_artifact_flags_draws_17_and_18(self):
        trace = json.loads((REPO / "data" / "rng_trace.json").read_text())
        for entry in trace["comparison"]:
            if entry["draw_ordinal"] in (17, 18):
                self.assertIn("shared_call_site", entry)
            elif "at" in entry:
                self.assertNotIn("shared_call_site", entry)

    def test_the_two_class_tables_do_not_drift(self):
        """compare.py is run as a script and cannot import the package, so it
        carries its own copy of the class names; they must agree."""
        self.assertEqual(compare.CLASS_NAME_BY_VALUE, driver.CLASS_NAME_BY_VALUE)

    def test_a_run_that_loaded_a_save_reports_the_guest_class(self):
        """Run E loaded SAVE_R3.SAV (a Вор, class 6) and never answered the
        class prompt; the artifact used to echo the CLI default, class 3."""
        trace = json.loads((REPO / "data" / "rng_trace.json").read_text())
        for run in trace["runs"]:
            self.assertEqual(run["class_value"], run["final_state"]["class_389c"])
            if run["loaded_save"]:
                self.assertIsNone(run["class_answer"])

    def test_turn_signature_preserves_order(self):
        draws = [{"turn": 1, "call_site_offset": 0xAF68},
                 {"turn": 1, "call_site_offset": 0xB353},
                 {"turn": 2, "call_site_offset": 0xB353}]
        self.assertEqual(compare.turn_site_sequences(draws),
                         {1: ["af68", "b353"], 2: ["b353"]})


class TestGdbAbortAndWalkGuards(unittest.TestCase):
    """The failure the first round of guards missed, and its two detectors.

    A gdb command error inside the `while 1` loop aborts the sourced script and
    drops gdb to its prompt WITH THE GUEST STOPPED AT A BREAKPOINT.  Every
    original guard passes there: gdb is alive, the log grew (with gdb's own
    error text), the frozen screen still classifies as the street prompt so the
    driver keeps typing and returns normally, the truncated prefix replays
    against the LCG, and the stopped guest spends no further draws so its
    RandSeed still equals the replay.  The result is a 40-of-393-draw trace that
    exits 0 and reads as evidence the other 353 draws never happened.
    """

    def short_run(self, drawn=40, prompts=3, abort=None):
        ns = ([20, 20, 10, 10, 100, 100, 25, 200, 100] * 10)[:drawn]
        sites = ([0xAF68, 0xAFC7, 0xB186, 0xB1B8, 0xB1EA, 0xB21C, 0xB353,
                  0xB39E, 0xB3AE] * 10)[:drawn]
        draws = real_stream(ns, sites=sites)
        at = set(range(0, drawn, max(1, drawn // prompts)))
        at = set(sorted(at)[:prompts])
        state = [[7, 1, 0x1000 + i] for i in range(len(at))]
        self.state = state
        return tracelog.parse(synth_log(draws, prompts_before=at, abort=abort,
                                        state=state),
                              state_names=STATE_NAMES)

    def final_state(self):
        """The `final_state` a healthy run would have read back: the last
        per-turn sample, because the guest changes none of these while it sits
        in ReadLn waiting for the next line."""
        return dict(zip(STATE_NAMES, self.state[-1]))

    def test_every_original_guard_passes_the_truncated_trace(self):
        """This is why the new guards had to exist: the old ones say OK."""
        parsed = self.short_run()
        v = tracelog.verify(parsed, SEED)               # install, count, replay
        self.assertEqual(v["lcg_replay"], "match")
        self.assertEqual(v["draws_verified"], 40)
        # ... and the state-tier check passes too: a STOPPED guest spends no
        # further draws, so its RandSeed still matches the logged prefix.
        seed_after_40 = parsed["draws"][-1]["seed_after"]
        self.assertEqual(
            tracelog.reconcile_final_randseed(parsed["draws"], SEED,
                                              seed_after_40)["final_randseed_matches_replay"],
            True)

    def test_walk_guard_catches_the_truncated_trace(self):
        """40 draws, 3 prompt stops, 30 walks asked for: the `w`s that were
        typed after the guest froze never reached 1000:ae63."""
        parsed = self.short_run()
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_walk_completed(parsed, 30)
        self.assertIn("only 3 times", str(cm.exception))

    def test_verify_run_rejects_the_truncated_trace(self):
        parsed = self.short_run()
        final = SEED
        for _ in parsed["draws"]:
            final = rng.step(final)
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.verify_run(parsed, SEED, walks=30, load_seg=0x224B,
                                screen_before="a", screen_after="b",
                                randseed_at_attach=0, randseed_final=final,
                                state_names=STATE_NAMES,
                                final_state=self.final_state())
        self.assertIn("top-level ReadLn", str(cm.exception))

    def test_verify_run_accepts_a_complete_run(self):
        parsed = self.short_run(drawn=40, prompts=4)
        final = SEED
        for _ in parsed["draws"]:
            final = rng.step(final)
        v = tracelog.verify_run(parsed, SEED, walks=3, load_seg=0x224B,
                                screen_before="a", screen_after="b",
                                randseed_at_attach=0, randseed_final=final,
                                state_names=STATE_NAMES,
                                final_state=self.final_state())
        self.assertTrue(v["final_randseed_matches_replay"])
        self.assertEqual(v["draws_verified"], 40)

    def test_walk_guard_passes_a_complete_run(self):
        parsed = self.short_run(drawn=40, prompts=4)
        self.assertEqual(tracelog.check_walk_completed(parsed, 3)["prompt_stops"], 4)

    def test_walk_guard_rejects_exactly_one_lost_turn(self):
        """The boundary: `walks` stops for `walks` walks must be REJECTED.

        A healthy run stops once before the first `w` and once after each
        completed turn, so N walks give N+1 stops -- all five captured runs do.
        A `>= walks` bound would tolerate exactly one lost turn, and there are
        two ways to spend that slack that every other guard passes: a freeze
        during the FINAL walk, and a mis-classified screen where the driver
        counts a turn the game never took.  Neither corrupts logged data, but a
        silently short DRIVE is the same defect class as a silently short trace.
        """
        parsed = self.short_run(drawn=40, prompts=3)
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_walk_completed(parsed, 3)
        self.assertIn("only 3 times", str(cm.exception))
        # And the healthy N+1 case still passes, so the bound is not simply
        # shifted past every real run.
        self.assertEqual(
            tracelog.check_walk_completed(self.short_run(drawn=40, prompts=4),
                                          3)["prompt_stops"], 4)

    def test_command_error_abort_is_an_error(self):
        """The other detector: the abort MESSAGE.  gdb aborts the script on any
        command error, and the harness's own shutdown aborts it too -- so the
        message is what separates them."""
        parsed = self.short_run(abort="Cannot access memory at address 0x1234")
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_script_abort(parsed)
        self.assertIn("Cannot access memory", str(cm.exception))

    def test_deliberate_shutdown_abort_is_accepted(self):
        parsed = self.short_run(abort=tracelog.EXPECTED_ABORT_MESSAGE)
        self.assertIn("deliberate", tracelog.check_script_abort(parsed)["gdb_script_abort"])

    def test_no_abort_at_all_is_accepted(self):
        self.assertEqual(tracelog.check_script_abort(self.short_run())["gdb_script_abort"],
                         "none")

    def test_two_aborts_are_an_error(self):
        text = synth_log(real_stream([20, 20]), abort=tracelog.EXPECTED_ABORT_MESSAGE)
        text += ("❌️ x.gdb:25: Error in sourced command file:\n%s\n"
                 % tracelog.EXPECTED_ABORT_MESSAGE)
        with self.assertRaises(tracelog.TraceError):
            tracelog.check_script_abort(tracelog.parse(text))

    def test_events_after_the_abort_are_an_error(self):
        text = synth_log(real_stream([20, 20]), abort=tracelog.EXPECTED_ABORT_MESSAGE)
        text += "R af6d 224b 0014 0000\n"
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_script_abort(tracelog.parse(text))
        self.assertIn("after the gdb script aborted", str(cm.exception))

    def test_the_five_committed_runs_would_pass_these_guards(self):
        """The guards must not reject the runs already published: each real log
        ends with exactly one abort, and its message is the deliberate one."""
        for label in "ABCDE":
            log = REPO / "build" / "rngtrace" / ("run%s" % label) / "trace.gdb.log"
            if not log.exists():          # the workdirs are gitignored scratch
                self.skipTest("no committed run logs in build/ on this machine")
            parsed = tracelog.parse(log.read_text(errors="replace"))
            self.assertEqual(parsed["unparsed"], [], label)
            self.assertIn("deliberate",
                          tracelog.check_script_abort(parsed)["gdb_script_abort"], label)


class TestProgressGuard(unittest.TestCase):
    def test_a_frozen_guest_is_an_error(self):
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_guest_progressed("screen", "screen", 0x12345678, 0x12345678)
        self.assertIn("no evidence", str(cm.exception))

    def test_a_changed_screen_is_progress(self):
        out = tracelog.check_guest_progressed("a", "b", 1, 1)
        self.assertTrue(out["screen_changed_during_drive"])

    def test_a_moved_randseed_is_progress(self):
        out = tracelog.check_guest_progressed("a", "a", 1, 2)
        self.assertTrue(out["randseed_moved_during_drive"])

    def test_missing_screens_are_an_error_not_a_pass(self):
        with self.assertRaises(tracelog.TraceError):
            tracelog.check_guest_progressed(None, None, 1, 2)


class TestReturnSegmentGuard(unittest.TestCase):
    """Call sites are attributed by OFFSET alone, which needs one segment."""

    def parsed(self, segs):
        draws = real_stream([20] * len(segs))
        for d, seg in zip(draws, segs):
            d["ret_seg"] = seg
        return tracelog.parse(synth_log(draws))

    def test_all_draws_from_the_load_segment_pass(self):
        out = tracelog.check_return_segments(self.parsed([0x224B, 0x224B]), 0x224B)
        self.assertTrue(out["return_segment_equals_load_seg"])

    def test_a_foreign_return_segment_is_an_error(self):
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_return_segments(self.parsed([0x224B, 0x3F00]), 0x224B)
        self.assertIn("3f00", str(cm.exception))


class TestLogLinesAreNeverDropped(unittest.TestCase):
    def test_a_five_digit_unexpected_stop_is_not_dropped(self):
        """`printf "? %04x", $pc` pads to four digits but does not truncate: a
        $pc above 0xffff prints five, and a {4}-only pattern would silently drop
        the line instead of reporting the unexpected stop."""
        parsed = tracelog.parse(synth_log(real_stream([20])) + "? 12345\n")
        self.assertEqual(parsed["unexpected_stops"], [0x12345])
        self.assertEqual(parsed["unparsed"], [])
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.verify(parsed, SEED)
        self.assertIn("unexpected", str(cm.exception))

    def test_a_malformed_harness_line_fails_the_run(self):
        parsed = tracelog.parse(synth_log(real_stream([20])) + "R af6d 224b 0014\n")
        self.assertEqual(len(parsed["unparsed"]), 1)
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.verify(parsed, SEED)
        self.assertIn("unparsed", str(cm.exception))

    def test_real_gdb_chatter_parses_clean(self):
        parsed = tracelog.parse(synth_log(real_stream([20, 20, 10]), chatter=True,
                                          abort=tracelog.EXPECTED_ABORT_MESSAGE))
        self.assertEqual(parsed["unparsed"], [])
        self.assertEqual(len(parsed["draws"]), 3)


class TestStateSampleChannel(unittest.TestCase):
    """Task 11i's per-turn state channel: the parser, its column discipline,
    and the guards that stop a state trace from being published wrong.

    A state trace whose samples sit on the wrong turns, or whose columns have
    shifted, is worse than no state trace -- it reads as evidence about turns
    it never described.  Every test here is one way that can happen.
    """

    def log(self, prompts=3, state=None, names=STATE_NAMES):
        draws = real_stream([20] * 9, sites=[0xAF68] * 9)
        at = set(range(prompts))
        if state is None:
            state = [[7, 1, 0x1000 + i] for i in range(prompts)]
        return tracelog.parse(synth_log(draws, prompts_before=at, state=state),
                              state_names=names)

    def test_one_sample_per_turn_marker(self):
        parsed = self.log(prompts=3)
        self.assertEqual([s["turn"] for s in parsed["state_samples"]], [1, 2, 3])
        self.assertEqual(parsed["state_samples"][0]["values"],
                         {"money_38c7": 7, "district_3692": 1,
                          "randseed_367e": 0x1000})
        self.assertEqual(parsed["unparsed"], [])

    def test_a_sample_line_with_too_few_columns_is_never_read_short(self):
        """The failure this refuses: a widened table the reader was not told
        about would otherwise zip names onto the wrong values silently."""
        parsed = tracelog.parse(synth_log(real_stream([20]), prompts_before={0},
                                          state=[[7, 1]]),
                                state_names=STATE_NAMES)
        self.assertEqual(parsed["state_samples"], [])
        self.assertEqual(len(parsed["unparsed"]), 1)
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_unparsed(parsed)
        self.assertIn("unparsed", str(cm.exception))

    def test_a_sample_line_without_a_name_list_is_unparsed(self):
        """A reader that was given no names must not drop the line: the
        committed pre-11i logs have no `S` lines, so one appearing without
        names means the two ends of the channel disagree."""
        parsed = tracelog.parse(synth_log(real_stream([20]), prompts_before={0},
                                          state=[[7, 1, 5]]))
        self.assertEqual(parsed["state_samples"], [])
        self.assertEqual(len(parsed["unparsed"]), 1)

    def test_the_committed_logs_still_parse_clean_without_names(self):
        parsed = tracelog.parse(synth_log(real_stream([20, 20]),
                                          prompts_before={0}))
        self.assertEqual(parsed["unparsed"], [])
        self.assertEqual(parsed["state_samples"], [])

    def test_a_stop_that_produced_no_sample_is_an_error(self):
        parsed = self.log(prompts=3)
        parsed["state_samples"].pop()
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_state_samples(parsed, walks=2, names=STATE_NAMES,
                                         final_state={})
        self.assertIn("wrong turns", str(cm.exception))

    def test_fewer_samples_than_walks_is_an_error(self):
        parsed = self.log(prompts=2)
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_state_samples(
                parsed, walks=5, names=STATE_NAMES,
                final_state=dict(zip(STATE_NAMES, [7, 1, 0x1001])))
        self.assertIn("2 state samples for 5 walks", str(cm.exception))

    def test_the_two_transports_must_agree(self):
        """The samples come over gdb; `final_state` comes out of a pmemsave
        dump of the whole guest.  A disagreement means one of the two reads the
        wrong address or the wrong width, and nothing downstream could say
        which."""
        parsed = self.log(prompts=3)
        final = dict(zip(STATE_NAMES, [7, 1, 0x1002]))
        out = tracelog.check_state_samples(parsed, walks=2, names=STATE_NAMES,
                                           final_state=final)
        self.assertTrue(out["final_state_matches_last_sample"])
        self.assertEqual(out["state_samples"], 3)

        final["money_38c7"] = 8
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_state_samples(parsed, walks=2, names=STATE_NAMES,
                                         final_state=final)
        self.assertIn("money_38c7", str(cm.exception))

    def test_a_final_state_missing_a_sampled_field_is_an_error(self):
        parsed = self.log(prompts=3)
        final = dict(zip(STATE_NAMES, [7, 1, 0x1002]))
        del final["district_3692"]
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_state_samples(parsed, walks=2, names=STATE_NAMES,
                                         final_state=final)
        self.assertIn("district_3692", str(cm.exception))

    def test_no_names_at_all_is_an_error_not_an_empty_channel(self):
        parsed = self.log(prompts=3)
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.check_state_samples(parsed, walks=2, names=[],
                                         final_state={})
        self.assertIn("cannot be verified", str(cm.exception))


class TestStateFieldTable(unittest.TestCase):
    """The table Task 11i widened, and the two transports built from it."""

    def fields(self):
        return runmod.state_fields()

    def test_the_task_11i_fields_are_present_and_cited(self):
        by_name = {n: (io, w) for n, io, w in self.fields()}
        for name, ds_off in (("beer_38c3", 0x38C3), ("money_38c7", 0x38C7),
                             ("hlam_38c9", 0x38C9), ("enemy_beer_396a", 0x396A),
                             ("enemy_money_396c", 0x396C),
                             ("enemy_hlam_396e", 0x396E)):
            self.assertIn(name, by_name)
            image_off, width = by_name[name]
            # The address arithmetic is tools/addr.py's, never restated: the
            # field's image offset must be exactly DGROUP + the DS offset its
            # name carries.
            self.assertEqual(image_off,
                             addr.image_off_of_ghidra(addr.DATA_SEG_GHIDRA, ds_off),
                             name)
            # Word-sized because `1000:523e`..`1000:5251` touches all six with
            # word instructions (`a1 xx xx` / `01 06 xx xx`).
            self.assertEqual(width, 2, name)
            self.assertTrue(runmod.STATE_CITATIONS[name].strip(), name)

    def test_the_victory_block_really_names_these_six_addresses(self):
        """The citation, checked against orig/g.exe's bytes rather than
        trusted: `1000:523e` must be the three `mov ax,[enemy]` /
        `add [player],ax` pairs the field table says it is."""
        exe = addr.read_exe()
        at = addr.file_off_of_citation("1000:523e")
        self.assertEqual(exe[at:at + 21].hex(" "),
                         "a1 6a 39 01 06 c3 38 a1 6c 39 01 06 c7 38 "
                         "a1 6e 39 01 06 c9 38")

    def test_every_field_is_a_readable_width(self):
        for name, _, width in self.fields():
            self.assertIn(width, (1, 2, 4), name)

    def test_the_names_are_unique_and_ordered_with_the_printf(self):
        names = runmod.state_field_names()
        self.assertEqual(len(names), len(set(names)))
        script = gdbsession.build_script(0x224B0, 1234, self.fields())
        line = [l for l in script.splitlines() if l.strip().startswith('printf "S ')][0]
        self.assertEqual(line.count("%x"), len(names))

    def test_both_transports_read_the_same_bytes(self):
        """The claim the last-sample reconciliation rests on: `read_state`
        (over a pmemsave dump) and the gdb printf read the SAME addresses with
        the SAME widths.  Checked by executing the printf's own operands
        against synthetic memory and comparing with `read_state`.
        """
        base = 0x224B0
        fields = self.fields()
        size = base + max(io for _, io, _ in fields) + 8
        rnd = random.Random(11)
        mem = bytes(rnd.randrange(256) for _ in range(size))

        script = gdbsession.build_script(base, 1234, fields)
        line = [l for l in script.splitlines() if l.strip().startswith('printf "S ')][0]
        ops = re.findall(r"\*\((unsigned \w+)\*\)\((0x[0-9a-f]+)\)", line)
        self.assertEqual(len(ops), len(fields))
        widths = {"unsigned char": 1, "unsigned short": 2, "unsigned int": 4}
        as_gdb_would = {}
        for (name, _, _), (ctype, at) in zip(fields, ops):
            w = widths[ctype]
            a = int(at, 16)
            as_gdb_would[name] = int.from_bytes(mem[a:a + w], "little")

        self.assertEqual(as_gdb_would, runmod.read_state(mem, base))

    def test_an_empty_field_list_is_refused(self):
        with self.assertRaises(ValueError):
            gdbsession.state_printf(0x224B0, [])


def guest_memory(exe, base, seed, patched=True, randseed=0, corrupt_random=False):
    """Physical memory holding the real image at `base`, as the guest would."""
    img = bytearray(loadbase.load_image(exe))
    if patched:
        p = seedpatch.build_patch(seed)
        img[loadbase.IMAGE_OFF_RANDOMIZE:loadbase.IMAGE_OFF_RANDOMIZE + len(p)] = p
    if corrupt_random:
        img[loadbase.IMAGE_OFF_RANDOM + 4] ^= 0xFF
    img[loadbase.IMAGE_OFF_RANDSEED:loadbase.IMAGE_OFF_RANDSEED + 4] = \
        randseed.to_bytes(4, "little")
    mem = bytearray(base + len(img) + 16)
    mem[base:base + len(img)] = img
    return bytes(mem)


class TestGuestCodeVerification(unittest.TestCase):
    """The defence against attaching at a WRONG base, which is the failure that
    produces a plausible EMPTY trace.  It was unreachable inside run.py's
    main(); it is a pure function over a memory image now."""

    BASE = 0x224B0
    SEED = 0x12345678

    def setUp(self):
        self.exe = (REPO / "orig" / "g.exe").read_bytes()
        self.patch = seedpatch.build_patch(self.SEED)

    def test_accepts_the_real_image_with_randseed_still_at_the_image_value(self):
        mem = guest_memory(self.exe, self.BASE, self.SEED)
        checks, randseed = loadbase.verify_guest_code(mem, self.exe, self.BASE,
                                                     self.SEED, self.patch)
        self.assertEqual(randseed, 0)
        self.assertIn("has not run yet", checks["randseed_state"])
        self.assertEqual(checks["random_linear"], "0x%X" % (self.BASE + 0x108CB))

    def test_accepts_the_pinned_seed_before_any_draw(self):
        mem = guest_memory(self.exe, self.BASE, self.SEED, randseed=self.SEED)
        checks, randseed = loadbase.verify_guest_code(mem, self.exe, self.BASE,
                                                     self.SEED, self.patch)
        self.assertEqual(randseed, self.SEED)
        self.assertIn("no draw has been spent", checks["randseed_state"])

    def test_rejects_wrong_bytes_at_the_breakpoint(self):
        mem = guest_memory(self.exe, self.BASE, self.SEED, corrupt_random=True)
        with self.assertRaises(loadbase.GuestCodeError) as cm:
            loadbase.verify_guest_code(mem, self.exe, self.BASE, self.SEED, self.patch)
        self.assertIn("expected Random", str(cm.exception))

    def test_rejects_a_wrong_base(self):
        mem = guest_memory(self.exe, self.BASE, self.SEED)
        with self.assertRaises(loadbase.GuestCodeError):
            loadbase.verify_guest_code(mem, self.exe, self.BASE + 0x10, self.SEED,
                                       self.patch)

    def test_rejects_an_unpatched_guest(self):
        mem = guest_memory(self.exe, self.BASE, self.SEED, patched=False)
        with self.assertRaises(loadbase.GuestCodeError) as cm:
            loadbase.verify_guest_code(mem, self.exe, self.BASE, self.SEED, self.patch)
        self.assertIn("seed patch is not in guest memory", str(cm.exception))

    def test_rejects_a_randseed_that_has_already_stepped(self):
        """Draws spent before the attach: the trace would be missing its head,
        and the LCG replay would then need a skip to line up at all."""
        mem = guest_memory(self.exe, self.BASE, self.SEED,
                           randseed=rng.step(self.SEED))
        with self.assertRaises(loadbase.GuestCodeError) as cm:
            loadbase.verify_guest_code(mem, self.exe, self.BASE, self.SEED, self.patch)
        self.assertIn("already", str(cm.exception))


class TestFinalRandSeedReconciliation(unittest.TestCase):
    """The tail-truncation guard: the LCG replay of a PREFIX is self-consistent,
    so only the guest's own final RandSeed catches draws lost at the end."""

    def draws(self, n=9):
        parsed = tracelog.parse(synth_log(real_stream([20, 20, 10, 10, 100, 100,
                                                       25, 200, 100][:n])))
        return parsed["draws"]

    def stepped(self, k):
        s = SEED
        for _ in range(k):
            s = rng.step(s)
        return s

    def test_matches_the_guest_seed(self):
        out = tracelog.reconcile_final_randseed(self.draws(), SEED, self.stepped(9))
        self.assertTrue(out["final_randseed_matches_replay"])

    def test_one_unlogged_draw_at_the_tail_is_caught(self):
        with self.assertRaises(tracelog.TraceError) as cm:
            tracelog.reconcile_final_randseed(self.draws(), SEED, self.stepped(10))
        self.assertIn("incomplete", str(cm.exception))

    def test_one_step_short_is_caught(self):
        with self.assertRaises(tracelog.TraceError):
            tracelog.reconcile_final_randseed(self.draws(), SEED, self.stepped(8))


class TestClassRecord(unittest.TestCase):
    """`--class-answer` is what the driver TYPED; DS:389c is what the guest
    HOLDS.  Run E of Task 11d loaded a save (a Вор, class 6) and the trace JSON
    still recorded class 3 from the CLI default."""

    def test_a_loaded_save_takes_the_class_from_the_guest(self):
        rec = driver.class_record(0, True, 6)
        self.assertEqual(rec["class_value"], 6)
        self.assertEqual(rec["class_name"], "Вор")   # Вор
        self.assertIsNone(rec["class_answer"])

    def test_a_created_character_records_the_agreeing_answer(self):
        rec = driver.class_record(3, False, 6)
        self.assertEqual(rec["class_value"], 6)
        self.assertEqual(rec["class_answer"], 3)
        self.assertTrue(rec["class_answer_agrees_with_guest"])

    def test_an_answer_that_did_not_land_in_the_class_prompt_is_an_error(self):
        """The documented drift: a blind key script put the class answer in the
        NAME prompt.  The guest's own class settles it."""
        with self.assertRaises(driver.DriveError) as cm:
            driver.class_record(3, False, 3)
        self.assertIn("did not land in the class prompt", str(cm.exception))


class TestOrderCheck(unittest.TestCase):
    """`in the catalogued order` is claimed in docs/re/gaps.md and
    docs/re/wander.md; compare() matches on site and `n` only, so without this
    a re-run whose order had drifted would still read as corroborated."""

    @classmethod
    def setUpClass(cls):
        cls.cat = compare.catalogue(json.loads((REPO / "data" / "wander.json").read_text()))

    def turn(self, sites, t=1):
        return [{"turn": t, "call_site_offset": s, "n": 0, "result": 0} for s in sites]

    def test_a_catalogued_turn_is_in_order(self):
        out = compare.check_order(self.cat, self.turn(
            [0xAF68, 0xAFC7, 0xB186, 0xB1B8, 0xB1EA, 0xB21C, 0xB353, 0xB39E, 0xB3AE]))
        self.assertTrue(out["in_catalogue_order"])
        self.assertEqual(out["turns_checked"], 1)

    def test_two_draws_swapped_is_a_violation(self):
        out = compare.check_order(self.cat, self.turn(
            [0xAF68, 0xB186, 0xAFC7, 0xB353]), label="X")
        self.assertFalse(out["in_catalogue_order"])
        self.assertEqual(out["violations"][0]["ordinals"], [1, 5, 2, 12])
        self.assertEqual(out["violations"][0]["run"], "X")

    def test_the_same_site_twice_in_one_turn_is_a_violation(self):
        out = compare.check_order(self.cat, self.turn([0xB353, 0xB353]))
        self.assertFalse(out["in_catalogue_order"])

    def test_each_turn_is_checked_independently(self):
        obs = self.turn([0xB353, 0xB39E], t=1) + self.turn([0xAF68, 0xAFC7], t=2)
        out = compare.check_order(self.cat, obs)
        self.assertTrue(out["in_catalogue_order"])
        self.assertEqual(out["turns_checked"], 2)

    def test_church_draws_are_outside_the_check(self):
        """15..18 fire nested inside another routine, so their position in the
        turn is not the preamble's ordinal sequence."""
        out = compare.check_order(self.cat, self.turn([0xB39E, 0x25FE, 0xB3AE]))
        self.assertTrue(out["in_catalogue_order"])

    def test_the_committed_runs_are_in_order(self):
        trace = json.loads((REPO / "data" / "rng_trace.json").read_text())
        self.assertTrue(trace["order_check"]["in_catalogue_order"])
        self.assertEqual(trace["order_check"]["turns_checked"], 86)
        self.assertEqual(trace["order_check"]["violations"], [])


class TestVmLifecycle(unittest.TestCase):
    """No qemu here -- only that the context manager cannot leak one."""

    def test_enter_kills_the_vm_when_start_raises(self):
        with tempfile.TemporaryDirectory() as td:
            machine = vm.Vm("/dev/null", td, td, sock_dir=td)
            killed = []

            def boom():
                machine.proc = object()          # qemu is already running here
                raise vm.MonitorError("monitor never came up")

            machine.start = boom
            machine.kill = lambda: killed.append(True)
            with self.assertRaises(vm.MonitorError):
                with machine:
                    self.fail("the body must not run")
            self.assertEqual(killed, [True])




class FoldTest(unittest.TestCase):
    """`compare.fold` is where a per-run field survived into a folded verdict.

    Nine shipped `comparison` entries carried a non-observing run's `why`
    beside a `corroborated` verdict.  Every other guard in this harness has a
    test that fails without it; this fold had none, so it gets one directly
    rather than only being exercised through a whole comparison run.
    """

    @staticmethod
    def entry(ordinal, verdict, **kw):
        e = {"draw_ordinal": ordinal, "verdict": verdict,
             "observed_count": kw.pop("observed_count", 0),
             "observed_n": kw.pop("observed_n", [])}
        e.update(kw)
        return e

    def test_a_later_corroboration_drops_the_earlier_runs_why(self):
        missed = self.entry(9, "not observed",
                            why="gate never satisfied in these runs: ...")
        fired = self.entry(9, "corroborated", observed_count=25,
                           observed_n=[100], detail="fired 25x")
        [out] = compare.fold([("A", [missed]), ("B", [fired])])
        self.assertEqual(out["verdict"], "corroborated")
        self.assertNotIn("why", out)
        self.assertEqual(out["observed_count"], 25)
        self.assertEqual(out["per_run"], {"B": [100]})

    def test_a_run_that_never_fires_keeps_its_why(self):
        missed = self.entry(9, "not observed", why="gate never satisfied")
        [out] = compare.fold([("A", [missed])])
        self.assertEqual(out["verdict"], "not observed")
        self.assertEqual(out["why"], "gate never satisfied")

    def test_counts_and_n_values_merge_across_runs(self):
        a = self.entry(3, "corroborated", observed_count=25, observed_n=[10])
        b = self.entry(3, "corroborated", observed_count=61, observed_n=[10, 20])
        [out] = compare.fold([("A", [a]), ("B", [b])])
        self.assertEqual(out["observed_count"], 86)
        self.assertEqual(out["observed_n"], [10, 20])
        self.assertEqual(out["per_run"], {"A": [10], "B": [10, 20]})

    def test_a_contradiction_outranks_a_corroboration(self):
        ok = self.entry(5, "corroborated", observed_count=5, observed_n=[2],
                        detail="fine")
        bad = self.entry(5, "contradicted", observed_count=1, observed_n=[3],
                         detail="n was 3, catalogue says 2")
        [out] = compare.fold([("A", [ok]), ("B", [bad])])
        self.assertEqual(out["verdict"], "contradicted")
        self.assertEqual(out["detail"], "n was 3, catalogue says 2")

    def test_empty_detail_is_dropped_not_shipped_as_an_empty_string(self):
        a = self.entry(4, "not observed", why="never fired")
        b = self.entry(4, "corroborated", observed_count=2, observed_n=[5])
        [out] = compare.fold([("A", [a]), ("B", [b])])
        self.assertNotIn("detail", out)

    def test_runs_that_contributed_nothing_are_dropped_from_per_run(self):
        a = self.entry(7, "not observed", why="never fired")
        b = self.entry(7, "corroborated", observed_count=3, observed_n=[9])
        [out] = compare.fold([("A", [a]), ("B", [b])])
        self.assertEqual(out["per_run"], {"B": [9]})


# ---------------------------------------------------------------------------
# Task 13: the fight capture.


def fight_log(lines):
    return "\n".join(lines) + "\n"


class FightScriptTest(unittest.TestCase):
    """`gdbsession.build_fight_script` -- four breakpoints, four dispatches."""

    BASE = 0x224B0

    def fields(self, names, width=2, off=0x3952):
        return [(n, loadbase.DATA_SEG_IMAGE_OFF + off + 2 * i, width)
                for i, n in enumerate(names)]

    def script(self):
        return gdbsession.build_fight_script(
            self.BASE, 1234,
            self.fields(["a", "b"]),
            self.fields(["e1", "e2"]),
            self.fields(["r1"]))

    def test_it_installs_four_breakpoints(self):
        s = self.script()
        self.assertEqual(s.count("\nbreak *"), 4)
        for off in (gdbsession.IMAGE_OFF_RANDOM_RETF,
                    gdbsession.IMAGE_OFF_MAIN_READLN,
                    gdbsession.IMAGE_OFF_COMBAT_ENTRY,
                    gdbsession.IMAGE_OFF_COMBAT_READLN):
            self.assertIn("break *%s" % hex(self.BASE + off), s)

    def test_every_breakpoint_has_a_pc_dispatch_and_an_unexpected_arm(self):
        s = self.script()
        for off in (gdbsession.OFF_RANDOM_RETF, gdbsession.OFF_MAIN_READLN,
                    gdbsession.OFF_COMBAT_ENTRY, gdbsession.OFF_COMBAT_READLN):
            self.assertIn("if $pc == %s" % hex(off), s)
        # A stop at none of the four must still be reported, not absorbed.
        self.assertIn('printf "? %04x\\n", $pc', s)

    def test_it_steps_over_the_breakpoint_by_hand(self):
        # The reason the loop exists at all: qemu re-traps forever otherwise.
        s = self.script()
        self.assertIn("disable\n  stepi\n  enable", s)

    def test_the_frozen_builder_is_untouched_by_it(self):
        # build_script produced data/rng_trace.json and data/state_trace.json.
        s = gdbsession.build_script(self.BASE, 1234, self.fields(["a", "b"]))
        self.assertEqual(s.count("\nbreak *"), 2)
        self.assertNotIn(hex(gdbsession.OFF_COMBAT_ENTRY), s)
        self.assertNotIn('printf "F', s)

    def test_an_empty_channel_is_refused(self):
        with self.assertRaises(ValueError):
            gdbsession.build_fight_script(self.BASE, 1234,
                                          self.fields(["a"]), [],
                                          self.fields(["r"]))
        with self.assertRaises(ValueError):
            gdbsession.build_fight_script(self.BASE, 1234,
                                          self.fields(["a"]),
                                          self.fields(["e"]), [])


class FightParseTest(unittest.TestCase):
    ENEMY = ["ec", "eseed"]
    ROUND = ["rhp", "rseed"]

    def parse(self, lines):
        return tracelog.parse(fight_log(lines), state_names=["s", "randseed_367e"],
                              enemy_names=self.ENEMY, round_names=self.ROUND)

    def test_a_fight_marker_takes_the_next_enemy_line(self):
        p = self.parse(["F", "E 5 abc", "C", "B 1f 0"])
        self.assertEqual(len(p["fights"]), 1)
        self.assertEqual(p["fights"][0]["enemy"], {"ec": 5, "eseed": 0xABC})
        self.assertEqual(p["fights"][0]["prompts"], 1)
        self.assertEqual(p["combat_prompts"][0]["values"], {"rhp": 0x1F, "rseed": 0})
        self.assertEqual(p["combat_prompts"][0]["fight"], 1)
        self.assertEqual(p["unparsed"], [])

    def test_draw_counts_are_recorded_per_marker(self):
        p = self.parse(["R 1234 224b 0064 0007", "F", "E 1 0",
                        "R 1234 224b 0064 0007", "C", "B 2 0"])
        self.assertEqual(p["fights"][0]["draws_before"], 1)
        self.assertEqual(p["combat_prompts"][0]["draws_before"], 2)

    def test_a_second_payload_for_one_marker_is_unparsed(self):
        p = self.parse(["F", "E 1 0", "E 2 0"])
        self.assertEqual(p["unparsed"], ["E 2 0"])

    def test_a_payload_with_the_wrong_column_count_is_unparsed(self):
        p = self.parse(["F", "E 1 2 3"])
        self.assertEqual(p["unparsed"], ["E 1 2 3"])
        self.assertIsNone(p["fights"][0]["enemy"])

    def test_a_fight_log_parsed_without_the_names_drops_nothing_silently(self):
        p = tracelog.parse(fight_log(["F", "E 1 0"]), state_names=["s"])
        self.assertEqual(p["unparsed"], ["E 1 0"])

    def test_a_build_script_log_still_parses_exactly_as_before(self):
        # The five committed runs' logs carry no F/E/C/B lines at all.
        p = tracelog.parse(fight_log(["R 1234 224b 0064 0007", "P", "S 3"]),
                           state_names=["s"])
        self.assertEqual(p["unparsed"], [])
        self.assertEqual(p["fights"], [])
        self.assertEqual(p["combat_prompts"], [])
        self.assertEqual(len(p["state_samples"]), 1)


class FightMarkerGuardTest(unittest.TestCase):
    NAMES = (["ec"], ["rhp"])

    def check(self, parsed):
        return tracelog.check_fight_markers(parsed, enemy_names=self.NAMES[0],
                                            round_names=self.NAMES[1])

    def parsed(self, fights, rounds):
        return {"fights": fights, "combat_prompts": rounds}

    def fight(self, i=1, enemy={"ec": 1}, prompts=1, draws=0):
        return {"index": i, "turn": 1, "draws_before": draws,
                "enemy": dict(enemy) if enemy is not None else None,
                "prompts": prompts}

    def rnd(self, i=1, values={"rhp": 5}, fight=1):
        return {"index": i, "turn": 1, "fight": fight, "draws_before": 0,
                "values": dict(values) if values is not None else None}

    def test_a_good_pair_passes(self):
        out = self.check(self.parsed([self.fight()], [self.rnd()]))
        self.assertTrue(out["every_fight_has_an_enemy_record"])
        self.assertEqual(out["fights"], 1)

    def test_a_fight_without_its_enemy_record_fails(self):
        with self.assertRaises(tracelog.TraceError):
            self.check(self.parsed([self.fight(enemy=None)], [self.rnd()]))

    def test_a_prompt_without_its_sample_fails(self):
        with self.assertRaises(tracelog.TraceError):
            self.check(self.parsed([self.fight()], [self.rnd(values=None)]))

    def test_a_fight_that_never_reached_its_prompt_fails(self):
        with self.assertRaises(tracelog.TraceError):
            self.check(self.parsed([self.fight(prompts=0)], []))

    def test_a_prompt_before_any_fight_marker_fails(self):
        with self.assertRaises(tracelog.TraceError):
            self.check(self.parsed([self.fight()], [self.rnd(fight=0)]))

    def test_a_record_missing_a_field_fails(self):
        with self.assertRaises(tracelog.TraceError):
            self.check(self.parsed([self.fight(enemy={"other": 1})],
                                   [self.rnd()]))

    def test_no_field_names_at_all_fails(self):
        with self.assertRaises(tracelog.TraceError):
            tracelog.check_fight_markers(self.parsed([], []),
                                         enemy_names=[], round_names=["rhp"])


class SampleSeedTest(unittest.TestCase):
    SEED = 0x12345678

    def sample(self, draws_before, value, field="randseed_367e"):
        return {"draws_before": draws_before, "values": {field: value}}

    def stepped(self, n):
        s = self.SEED
        for _ in range(n):
            s = rng.step(s)
        return s

    def test_correct_seeds_pass_and_are_counted(self):
        out = tracelog.check_sample_seeds(
            {}, self.SEED,
            channels=[("turn", [self.sample(0, self.SEED),
                                self.sample(3, self.stepped(3))],
                       "randseed_367e")])
        self.assertEqual(out["turn_seeds_match_lcg"], 2)

    def test_a_sample_at_the_wrong_point_in_the_stream_fails(self):
        with self.assertRaises(tracelog.TraceError):
            tracelog.check_sample_seeds(
                {}, self.SEED,
                channels=[("turn", [self.sample(2, self.stepped(3))],
                           "randseed_367e")])

    def test_an_empty_channel_is_refused_rather_than_passing_vacuously(self):
        with self.assertRaises(tracelog.TraceError):
            tracelog.check_sample_seeds({}, self.SEED,
                                        channels=[("fight", [], "x")])

    def test_a_sample_without_the_field_fails(self):
        with self.assertRaises(tracelog.TraceError):
            tracelog.check_sample_seeds(
                {}, self.SEED,
                channels=[("turn", [self.sample(0, 1, field="other")],
                           "randseed_367e")])


class StateSampleSplitTest(unittest.TestCase):
    """The refactor that let the fight capture reuse the shape checks."""

    def parsed(self, samples, prompts=None):
        return {"state_samples": samples,
                "prompt_stops": len(samples) if prompts is None else prompts}

    def sample(self, turn, values):
        return {"turn": turn, "draws_before": 0, "values": dict(values)}

    def test_shape_alone_does_not_look_at_final_state(self):
        p = self.parsed([self.sample(1, {"a": 1}), self.sample(2, {"a": 9})])
        out = tracelog.check_state_sample_shape(p, walks=1, names=["a"])
        self.assertEqual(out["state_samples"], 2)

    def test_check_state_samples_still_runs_both_halves(self):
        p = self.parsed([self.sample(1, {"a": 1}), self.sample(2, {"a": 9})])
        out = tracelog.check_state_samples(p, walks=1, names=["a"],
                                           final_state={"a": 9})
        self.assertEqual(out["state_samples"], 2)
        self.assertTrue(out["final_state_matches_last_sample"])
        with self.assertRaises(tracelog.TraceError):
            tracelog.check_state_samples(p, walks=1, names=["a"],
                                         final_state={"a": 8})


class FightRunFieldTest(unittest.TestCase):
    def test_every_sampled_field_is_inside_the_data_segment(self):
        for fields in (fightrun.enemy_fields(), fightrun.round_fields()):
            for name, image_off, width in fields:
                self.assertGreaterEqual(image_off, loadbase.DATA_SEG_IMAGE_OFF,
                                        name)
                self.assertIn(width, (1, 2, 4), name)

    def test_the_enemy_record_offsets_match_the_documented_layout(self):
        by_name = {n: o - loadbase.DATA_SEG_IMAGE_OFF
                   for n, o, _ in fightrun.enemy_fields()}
        # docs/re/combat.md, "The fighter record": the enemy's copy starts at
        # 20ae:3952 and the field name carries its own offset.
        for name, off in by_name.items():
            if name.startswith("e_randseed"):
                continue
            self.assertEqual("%04x" % off, name.rsplit("_", 1)[1], name)

    def test_both_channels_carry_randseed(self):
        for key, fields in (("fight", fightrun.enemy_fields()),
                            ("round", fightrun.round_fields())):
            names = fightrun.field_names(fields)
            self.assertIn(fightrun.SEED_FIELD[key], names)
            [(_, off, width)] = [f for f in fields
                                 if f[0] == fightrun.SEED_FIELD[key]]
            self.assertEqual(off, loadbase.IMAGE_OFF_RANDSEED)
            self.assertEqual(width, 4)

    def test_the_post_drive_image_check_rejects_a_clobbered_image(self):
        exe = (REPO / "orig" / "g.exe").read_bytes()
        base = 0x30000
        seed = 0x12345678
        patch = seedpatch.build_patch(seed)
        # `guest_memory` does not apply relocations (its callers do not need
        # them); this check does, so the fixture applies them the way DOS
        # would -- memory word = file word + load segment.
        mem = bytearray(guest_memory(exe, base, seed))
        load_seg = base // 16
        for r in loadbase.parse_relocations(exe):
            at = base + r
            w = int.from_bytes(mem[at:at + 2], "little")
            mem[at:at + 2] = ((w + load_seg) & 0xFFFF).to_bytes(2, "little")
        out, got = fightrun.verify_image_after_drive(mem, exe, base, patch)
        self.assertTrue(out["random_bytes_intact"])
        self.assertEqual(out["relocations_checked"], out["relocations_total"])
        self.assertEqual(got, 0)
        # COMMAND.COM's transient landing on Random is exactly what this is
        # here to catch.
        mem[base + loadbase.IMAGE_OFF_RANDOM] ^= 0xFF
        with self.assertRaises(loadbase.GuestCodeError):
            fightrun.verify_image_after_drive(mem, exe, base, patch)


class FightFoldTest(unittest.TestCase):
    def trace(self, fights, ndraws):
        return {"fights": fights,
                "draws": [{"ordinal": i + 1, "turn": 1, "call_site_offset": 0x4460,
                           "n": 100, "result": 1} for i in range(ndraws)]}

    def f(self, i, at, prompts=1):
        return {"index": i, "turn": 1, "draws_before": at,
                "enemy": {"e_class_3952": 3}, "prompts": prompts}

    def test_a_fight_spans_up_to_the_next_one(self):
        t = self.trace([self.f(1, 10), self.f(2, 40)], 100)
        rows = combattrace.fight_records(t, combattrace.compact_draws(t))
        self.assertEqual([r["draws_until_next_fight"] for r in rows], [30, 60])
        self.assertEqual([r["first_draw_index"] for r in rows], [10, 40])

    def test_one_fight_runs_to_the_end_of_the_stream(self):
        t = self.trace([self.f(1, 7)], 20)
        [row] = combattrace.fight_records(t, combattrace.compact_draws(t))
        self.assertEqual(row["draws_until_next_fight"], 13)

    def test_the_frozen_oracles_are_only_ever_read(self):
        # combattrace records their digests; it must never name them as an
        # output.  This is the file-level form of the Task 11i rule.
        src = (REPO / "tools" / "rngtrace" / "combattrace.py").read_text()
        for name in combattrace.FROZEN:
            self.assertIn(name, src)
        self.assertNotIn("write_text(", src.split("def digest")[0])


class FightDriverTest(unittest.TestCase):
    def test_the_dos_prompt_is_recognised_and_game_prompts_are_not(self):
        self.assertTrue(driver.game_gone("x\nC:\\>"))
        self.assertTrue(driver.game_gone("x\nA:\\GAME>"))
        self.assertFalse(driver.game_gone("x\n\\"))
        self.assertFalse(driver.game_gone("x\n\u0411\u0438\u0442\u0432\u0430\\"))
        # A game line that merely ends in `>` is not the guest having exited.
        self.assertFalse(driver.game_gone("x\nfoo>"))

    def test_only_the_two_dispatched_verbs_are_accepted(self):
        for bad in ("y", "kos", "", "K"):
            with self.assertRaises(driver.DriveError):
                driver.fight(None, 1, combat_answer=bad)

    def test_the_accept_token_is_the_literal_the_original_compares(self):
        # file 0x9BF3, compared at 1000:b548 / 1000:b696 / 1000:b718.
        self.assertEqual(driver.ACCEPT, "y")


if __name__ == "__main__":
    unittest.main(verbosity=2)
