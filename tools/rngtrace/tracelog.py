"""Parse a gdb trace log into draws, and verify it hard.

The verification is the reason this harness can be trusted at all.  A tracer
that silently logs three draws when nine happened is WORSE than no tracer,
because a short trace reads as evidence of absence.  Three independent guards:

  1. `install` -- the log must show both breakpoints accepted and a READY line.
  2. `count`   -- a run that logs zero draws is an error, never an empty file.
  3. `replay`  -- with the seed pinned, the whole draw stream is predictable:
     the k-th logged draw must equal `(step^k(seed) * n) >> 32`.  A MISSED draw
     desynchronises the LCG and every later prediction fails, so this guard
     catches under-reporting, not just wrong reads.  It is the completeness
     proof, and it works only because `Random` (0f78:114b) is the sole
     runtime path into `@Rand`: orig/g.exe contains 86 far calls to 0f78:114b,
     0 to 0f78:11a8, 0 to 0f78:1168, and no near call reaches any of them from
     outside `Random` itself.
"""
import re

from . import rng

RE_DRAW = re.compile(r"^R ([0-9a-f]{4}) ([0-9a-f]{4}) ([0-9a-f]{4}) ([0-9a-f]{4})\s*$")
RE_PROMPT = re.compile(r"^P\s*$")
RE_UNEXPECTED = re.compile(r"^\? ([0-9a-f]{4})\s*$")
RE_READY = re.compile(r"^READY base=([0-9a-f]+) retf=([0-9a-f]+) readln=([0-9a-f]+)\s*$")
CALL_LEN = 5  # `9a 4b 11 78 0f` -- a far call is 5 bytes


class TraceError(RuntimeError):
    pass


def strip_gdb_noise(line: str) -> str:
    """gdb interleaves its own prompt with our printf output."""
    return line.replace("(gdb) ", "").strip()


def parse(text: str) -> dict:
    """Turn a gdb log into {draws, prompts, unexpected, ready, ...}."""
    ready = None
    draws = []
    prompts = 0
    unexpected = []
    bp_lines = []
    seq = []          # interleaved event kinds, in order
    for raw in text.splitlines():
        line = strip_gdb_noise(raw)
        m = RE_READY.match(line)
        if m:
            ready = {"image_base": int(m.group(1), 16),
                     "retf": int(m.group(2), 16),
                     "readln": int(m.group(3), 16)}
            continue
        m = RE_DRAW.match(line)
        if m:
            ret_off = int(m.group(1), 16)
            ret_seg = int(m.group(2), 16)
            n = int(m.group(3), 16)
            result = int(m.group(4), 16)
            draws.append({
                "ordinal": len(draws) + 1,
                "turn": prompts,               # draws seen after the Nth prompt read
                "return_offset": ret_off,
                "return_segment": ret_seg,
                "call_site_offset": (ret_off - CALL_LEN) & 0xFFFF,
                "n": n,
                "result": result,
            })
            seq.append("R")
            continue
        if RE_PROMPT.match(line):
            prompts += 1
            seq.append("P")
            continue
        m = RE_UNEXPECTED.match(line)
        if m:
            unexpected.append(int(m.group(1), 16))
            seq.append("?")
            continue
        if line.startswith("Breakpoint ") or line.startswith("Num "):
            bp_lines.append(line)
    return {"ready": ready, "draws": draws, "prompt_stops": prompts,
            "unexpected_stops": unexpected, "breakpoint_lines": bp_lines,
            "event_sequence": "".join(seq)}


def check_install(parsed: dict, expect_breakpoints=2):
    if parsed["ready"] is None:
        raise TraceError("gdb never reported READY: the breakpoints were never installed")
    accepted = [l for l in parsed["breakpoint_lines"] if l.startswith("Breakpoint ")]
    if len(accepted) < expect_breakpoints:
        raise TraceError("expected %d breakpoints to be accepted, log shows %d: %r"
                         % (expect_breakpoints, len(accepted), parsed["breakpoint_lines"]))


def check_nonempty(parsed: dict, min_draws=1):
    n = len(parsed["draws"])
    if n < min_draws:
        raise TraceError(
            "only %d draws logged (minimum %d).  A short trace is not evidence "
            "that draws did not happen -- it is a broken tracer." % (n, min_draws))
    if parsed["unexpected_stops"]:
        raise TraceError("stops at unexpected $pc values: %s"
                         % [hex(x) for x in parsed["unexpected_stops"]])


def replay(draws, seed: int, max_skip: int = 0):
    """Verify the draw stream against the LCG.

    Returns (skipped, states).  `skipped` is the number of leading LCG states
    consumed before the first logged draw -- draws that happened before the
    breakpoint was installed.  It must be 0 for a run attached before the
    game's first draw; `max_skip` allows a bounded search when it is not.
    """
    if not draws:
        raise TraceError("nothing to replay")
    for skip in range(max_skip + 1):
        s = seed
        for _ in range(skip):
            s = rng.step(s)
        ok = True
        states = []
        cur = s
        for d in draws:
            cur, r = rng.draw(cur, d["n"])
            states.append(cur)
            if r != d["result"]:
                ok = False
                break
        if ok:
            return skip, states
    # Report precisely where it first diverged with skip=0.
    s = seed
    for i, d in enumerate(draws):
        s, r = rng.draw(s, d["n"])
        if r != d["result"]:
            raise TraceError(
                "LCG replay diverged at draw %d (call site 1000:%04x, n=%d): "
                "predicted %d, observed %d.  Either a draw was missed (the trace "
                "under-reports) or the reads are wrong; a short trace must never "
                "be published as evidence." % (d["ordinal"], d["call_site_offset"],
                                               d["n"], r, d["result"]))
    raise TraceError("replay failed but no divergence found -- inconsistent state")


def verify(parsed: dict, seed: int, min_draws=1, max_skip=0):
    check_install(parsed)
    check_nonempty(parsed, min_draws=min_draws)
    skip, states = replay(parsed["draws"], seed, max_skip=max_skip)
    for d, st in zip(parsed["draws"], states):
        d["seed_after"] = st
    return {"lcg_replay": "match", "leading_states_skipped": skip,
            "draws_verified": len(parsed["draws"])}


def group_by_turn(draws):
    """Draws bucketed by how many top-level prompt reads preceded them."""
    turns = {}
    for d in draws:
        turns.setdefault(d["turn"], []).append(d)
    return turns
