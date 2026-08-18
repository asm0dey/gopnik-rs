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
    unexpected stop, and -- the important one -- a DROPPED draw, which the LCG
    replay must catch because a missed draw desynchronises everything after it;
  * the gdb script shape, as a regression on the reason it is a `while` loop
    (qemu reports $pc as the 16-bit offset, so breakpoint `commands` never run).

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
import struct
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "tools"))

from rngtrace import compare, gdbsession, loadbase, rng, seedpatch, tracelog  # noqa: E402


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
        # A Ghidra `1000:XXXX` citation is an IMAGE offset; file = 0x18d0 + it.
        self.assertEqual(loadbase.file_off_of_image_off(
            loadbase.image_off_of_ghidra(0xB353)), 0x18D0 + 0xB353)
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


def synth_log(draws, prompts_before=(), base=0x224B0, ready=True, bp=2,
              unexpected=()):
    """A gdb log shaped exactly like the real one."""
    out = []
    if bp:
        for i in range(bp):
            out.append("Breakpoint %d at 0x%x" % (i + 1, base + i))
    if ready:
        out.append("READY base=%x retf=%x readln=%x"
                   % (base, base + 0x108E5, base + 0xAE63))
    for i, d in enumerate(draws):
        if i in prompts_before:
            out.append("P")
        out.append("R %04x %04x %04x %04x"
                   % (d["ret_off"], d.get("ret_seg", 0x224B), d["n"], d["result"]))
    for pc in unexpected:
        out.append("? %04x" % pc)
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
        s = gdbsession.build_script(0x224B0, Path("/dev/null"), 1234)
        self.assertIn("break *0x32d95", s)     # 0x224b0 + 0x108e5, Random's retf
        self.assertIn("break *0x2d313", s)     # 0x224b0 + 0xae63, the prompt ReadLn
        self.assertNotIn("0x224B0", s.replace("base=%x", ""))  # never hardcoded twice

    def test_dispatches_on_pc_not_on_breakpoint_commands(self):
        """Regression: qemu reports $pc as the 16-bit offset while the
        breakpoint is at the linear address, so gdb cannot attribute the stop
        and a `commands` block never runs.  The loop must dispatch on $pc."""
        s = gdbsession.build_script(0x224B0, Path("/dev/null"), 1234)
        self.assertIn("while 1", s)
        self.assertIn("if $pc == 0x1165", s)
        self.assertIn("if $pc == 0xae63", s)
        self.assertNotIn("commands", s)

    def test_steps_over_the_breakpoint_by_hand(self):
        """Regression on the retrap: gdb does not know a breakpoint is at the
        stop address, so it never removes/steps/reinserts, and QEMU re-traps
        the same instruction forever while the guest makes no progress."""
        s = gdbsession.build_script(0x224B0, Path("/dev/null"), 1234)
        loop = s.split("while 1", 1)[1]
        self.assertIn("disable", loop)
        self.assertIn("stepi", loop)
        self.assertIn("enable", loop)
        self.assertLess(loop.index("disable"), loop.index("stepi"))
        self.assertLess(loop.index("stepi"), loop.index("enable"))

    def test_reads_the_caller_frame_at_the_retf(self):
        s = gdbsession.build_script(0x224B0, Path("/dev/null"), 1234)
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

    def test_turn_signature_preserves_order(self):
        draws = [{"turn": 1, "call_site_offset": 0xAF68},
                 {"turn": 1, "call_site_offset": 0xB353},
                 {"turn": 2, "call_site_offset": 0xB353}]
        self.assertEqual(compare.turn_site_sequences(draws),
                         {1: ["af68", "b353"], 2: ["b353"]})


if __name__ == "__main__":
    unittest.main(verbosity=2)
