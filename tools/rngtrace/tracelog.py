"""Parse a gdb trace log into draws, and verify it hard.

The verification is the reason this harness can be trusted at all.  A tracer
that silently logs three draws when nine happened is WORSE than no tracer,
because a short trace reads as evidence of absence.  Every guard below exists
because a short trace can be produced in more than one way, and each way looks
plausible from the outside:

  1. `install`  -- the log must show both breakpoints accepted and a READY line.
  2. `count`    -- a run that logs zero draws is an error, never an empty file.
  3. `unparsed` -- a line that looks like harness output but does not parse is
     an error.  `printf "? %04x"` emits five hex digits if `$pc` ever exceeds
     0xffff, and a four-digit-only pattern would make an unexpected stop VANISH
     instead of tripping guard 2.
  4. `abort`    -- a gdb command error inside the `while 1` loop aborts the
     sourced script and drops gdb to its prompt WITH THE GUEST STOPPED AT A
     BREAKPOINT.  Nothing else notices: gdb is alive, the log grew (with gdb's
     own error text), the frozen screen still classifies as the street prompt
     so the driver keeps typing, and a truncated prefix replays perfectly.  The
     harness kills the VM on purpose at the end of a run, which aborts the
     script too -- so the abort itself is not the signal, its MESSAGE is:
     the deliberate one always reads `Remote connection closed`.
  5. `walk`     -- every `w` typed at the street prompt must produce a stop at
     the top-level ReadLn (1000:ae63).  Fewer stops than walks means the guest
     stopped progressing while the driver kept typing.  This is the flow-tier
     check on guard 4's failure, independent of any log text.
  6. `progress` -- the screen or RandSeed must differ between the start and the
     end of the drive; a guest frozen from the first keystroke changes neither.
  7. `segment`  -- every draw's RETURN SEGMENT must equal the load segment.
     Call sites are attributed by offset alone, which is only unambiguous while
     every caller lives in the base segment.
  8. `replay`   -- with the seed pinned, the whole draw stream is predictable:
     the k-th logged draw must equal `(step^k(seed) * n) >> 32`.  A MISSED draw
     desynchronises the LCG and every later prediction fails, so this catches
     under-reporting, not just wrong reads.  It is the completeness proof, and
     it works only because `Random` (0f78:114b) is the sole runtime path into
     `@Rand`: orig/g.exe contains 86 far calls to 0f78:114b, 0 to 0f78:11a8 and
     0 to 0f78:1168 -- the only other `@Rand` caller, `0f78:1168` (the
     Real-valued `Random`), is itself a near caller of `@Rand` but is never far
     called, and no other near call reaches either from outside `Random`.
  9. `final seed` -- the guest's own RandSeed at the end must equal the seed
     stepped once per logged draw.

Guards 4, 5, 6 and 7 were added in fix wave 1; 4/5/6 close the gdb-script-abort
path, which passed every one of the original guards.
"""
import re

from . import rng

RE_DRAW = re.compile(r"^R ([0-9a-f]{4}) ([0-9a-f]{4}) ([0-9a-f]{4}) ([0-9a-f]{4})\s*$")
RE_PROMPT = re.compile(r"^P\s*$")
# 1..8 digits deliberately: `printf "? %04x", $pc` pads to four but does not
# truncate, so a $pc above 0xffff prints five and a {4}-only pattern would drop
# the line instead of reporting the unexpected stop.
RE_UNEXPECTED = re.compile(r"^\? ([0-9a-f]{1,8})\s*$")
RE_READY = re.compile(r"^READY base=([0-9a-f]+) retf=([0-9a-f]+) readln=([0-9a-f]+)\s*$")
CALL_LEN = 5  # `9a 4b 11 78 0f` -- a far call is 5 bytes

# The message gdb prints when the sourced script is aborted BY DESIGN: run.py
# kills the VM first, so the pending `continue` fails with this and gdb drops to
# a prompt where `quit` is read.  Any other abort message means a command inside
# the trace loop failed while the guest was still running.
EXPECTED_ABORT_MESSAGE = "Remote connection closed"

RE_ABORT = re.compile(r"Error in sourced command file:\s*$")
RE_WARNING = re.compile(r"^warning:")
# gdb chatter this harness expects to see, enumerated from the five committed
# runs' logs.  Anything outside this list and the harness's own output is an
# `unparsed` line and fails the run rather than being dropped silently.
GDB_NOISE = [
    re.compile(r"^Program received signal [A-Z0-9]+, .*$"),
    re.compile(r"^0x[0-9a-f]+ in \?\? \(\)$"),
    re.compile(r"^Num\s+Type\s+Disp\s+Enb\s+Address\s+What$"),
    re.compile(r"^\d+\s+breakpoint\s+keep\s+[yn]\s+0x[0-9a-f]+\s*$"),
    re.compile(r'^The target architecture is set to ".*"\.$'),
    re.compile(r"^\(gdb\)$"),
]
RE_STYLE = re.compile(r"^[^\x20-\x7e]+\s*")


class TraceError(RuntimeError):
    pass


def strip_gdb_noise(line: str) -> str:
    """gdb interleaves its own prompt with our printf output."""
    return line.replace("(gdb) ", "").strip()


def strip_style(line: str) -> str:
    """Recent gdb prefixes warnings and errors with a styled glyph."""
    return RE_STYLE.sub("", line)


def parse(text: str) -> dict:
    """Turn a gdb log into {draws, prompts, unexpected, aborts, unparsed, ...}."""
    ready = None
    draws = []
    prompts = 0
    unexpected = []
    bp_lines = []
    aborts = []
    unparsed = []
    seq = []          # interleaved event kinds, in order
    pending = None    # "warning" | "abort": the next line continues it
    for raw in text.splitlines():
        line = strip_style(strip_gdb_noise(raw))
        if not line:
            pending = None
            continue
        if pending == "abort":
            aborts[-1]["message"] = line
            pending = None
            continue
        if pending == "warning":
            pending = None
            continue
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
        if RE_ABORT.search(line):
            aborts.append({"header": line, "message": None,
                           "events_before": len(seq)})
            seq.append("A")
            pending = "abort"
            continue
        if RE_WARNING.match(line):
            pending = "warning"
            continue
        if line.startswith("Breakpoint ") or line.startswith("Num "):
            bp_lines.append(line)
            continue
        if any(p.match(line) for p in GDB_NOISE):
            continue
        unparsed.append(line)
    return {"ready": ready, "draws": draws, "prompt_stops": prompts,
            "unexpected_stops": unexpected, "breakpoint_lines": bp_lines,
            "script_aborts": aborts, "unparsed": unparsed,
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


def check_unparsed(parsed: dict):
    """Nothing in the log may be dropped on the floor.

    The harness's own printf lines are fixed-shape; a line that neither parses
    nor matches the enumerated gdb chatter is either malformed harness output
    (an unexpected stop whose $pc did not fit the format, say) or a gdb message
    nobody has looked at.  Both must fail the run rather than vanish.
    """
    if parsed["unparsed"]:
        raise TraceError(
            "%d unparsed line(s) in the trace log -- a dropped line could be a "
            "stop the guards never see: %r"
            % (len(parsed["unparsed"]), parsed["unparsed"][:5]))
    return {"unparsed_lines": 0}


def check_script_abort(parsed: dict):
    """The gdb `while 1` loop must only have been aborted on purpose.

    Any command error inside the loop -- a bad memory read, a stale register, a
    gdb version that dislikes one of the expressions -- aborts the sourced
    script and drops gdb to its prompt with THE GUEST STOPPED AT A BREAKPOINT.
    Every other guard still passes there: gdb is alive, the log grew, the frozen
    screen still reads as the street prompt, and the truncated prefix replays.
    The harness's own shutdown aborts the script too (the VM is killed first, on
    purpose), so the discriminator is the message, not the abort.
    """
    aborts = parsed["script_aborts"]
    if not aborts:
        return {"gdb_script_abort": "none"}
    if len(aborts) > 1:
        raise TraceError("the gdb script aborted %d times: %r"
                         % (len(aborts), aborts))
    a = aborts[0]
    if a["message"] != EXPECTED_ABORT_MESSAGE:
        raise TraceError(
            "the gdb trace loop aborted with %r, not the deliberate shutdown "
            "(%r).  A command error inside the loop leaves gdb at its prompt "
            "with the guest STOPPED at a breakpoint: every other guard passes, "
            "and the trace silently ends there."
            % (a["message"], EXPECTED_ABORT_MESSAGE))
    after = parsed["event_sequence"][a["events_before"] + 1:]
    if after:
        raise TraceError(
            "events logged after the gdb script aborted (%r): the abort is "
            "supposed to be the last thing that happens" % after)
    return {"gdb_script_abort": EXPECTED_ABORT_MESSAGE + " (the deliberate shutdown)"}


def check_walk_completed(parsed: dict, walks: int):
    """Every `w` typed at the street prompt must reach the top-level ReadLn.

    Flow-tier, and independent of anything gdb printed: the turn marker is a
    breakpoint on 1000:ae63.  A guest that stopped progressing mid-drive cannot
    produce them, however healthy the driver's screen looked.
    """
    stops = parsed["prompt_stops"]
    if stops < walks:
        raise TraceError(
            "the driver typed `w` %d times but the guest stopped at the "
            "top-level ReadLn (1000:ae63) only %d times: it stopped making "
            "progress while the driver kept typing into a frozen screen.  The "
            "trace is truncated and must not be published." % (walks, stops))
    return {"prompt_stops": stops, "walks_requested": walks}


def check_guest_progressed(screen_before, screen_after,
                           randseed_at_attach, randseed_final):
    """Independent evidence that the guest ran at all during the drive."""
    if screen_before is None or screen_after is None:
        raise TraceError("no screens captured around the drive: the progress "
                         "guard cannot be evaluated, so the run is not trusted")
    screen_moved = screen_before != screen_after
    seed_moved = randseed_final != randseed_at_attach
    if not (screen_moved or seed_moved):
        raise TraceError(
            "the guest shows no evidence of having run during the drive: the "
            "screen is byte-identical and RandSeed is unchanged (0x%08X).  A "
            "guest halted at a breakpoint looks exactly like this."
            % randseed_final)
    return {"screen_changed_during_drive": screen_moved,
            "randseed_moved_during_drive": seed_moved}


def check_return_segments(parsed: dict, load_seg: int):
    """Call sites are attributed by OFFSET; that needs one segment.

    Every catalogued caller lives in the base segment, so the return offset is
    the Ghidra offset -- but only while the return SEGMENT is the load segment.
    A draw from another code segment whose offset happened to collide with a
    catalogued one would otherwise be reported as a corroboration.
    """
    segs = sorted({d["return_segment"] for d in parsed["draws"]})
    bad = [s for s in segs if s != load_seg]
    if bad:
        raise TraceError(
            "draws returned into segment(s) %s, not the load segment %04x: "
            "their call-site offsets are not segment-1000 offsets and must not "
            "be matched against the catalogue by offset alone"
            % (["%04x" % s for s in bad], load_seg))
    return {"return_segments_seen": ["%04x" % s for s in segs],
            "return_segment_equals_load_seg": True}


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


def reconcile_final_randseed(draws, seed: int, final_randseed: int):
    """State-tier completeness: the guest's own RandSeed at the end.

    Replaying the LCG one step per logged draw must land exactly on it.  A draw
    the tracer failed to log leaves the guest ahead of the replay, which catches
    truncation at the TAIL -- where the LCG replay of a prefix cannot.
    """
    expected = seed
    for _ in draws:
        expected = rng.step(expected)
    if final_randseed != expected:
        raise TraceError(
            "final RandSeed 0x%08X does not match the replay of %d logged draws "
            "(0x%08X): the trace is incomplete."
            % (final_randseed, len(draws), expected))
    return {"final_randseed": "0x%08X" % final_randseed,
            "final_randseed_matches_replay": True}


def verify(parsed: dict, seed: int, min_draws=1, max_skip=0):
    """The log-only guards: install, count, unparsed, abort, LCG replay."""
    check_install(parsed)
    check_nonempty(parsed, min_draws=min_draws)
    out = {}
    out.update(check_unparsed(parsed))
    out.update(check_script_abort(parsed))
    skip, states = replay(parsed["draws"], seed, max_skip=max_skip)
    for d, st in zip(parsed["draws"], states):
        d["seed_after"] = st
    out.update({"lcg_replay": "match", "leading_states_skipped": skip,
                "draws_verified": len(parsed["draws"])})
    return out


def verify_run(parsed: dict, seed: int, *, walks, load_seg, screen_before,
               screen_after, randseed_at_attach, randseed_final,
               min_draws=1, max_skip=0):
    """Every guard, log-tier and run-tier.  Keyword-only ON PURPOSE.

    run.py calls exactly this and nothing else, so a guard cannot be forgotten
    at the call site: leaving one of these out is a TypeError, not a silently
    weaker run.
    """
    out = verify(parsed, seed, min_draws=min_draws, max_skip=max_skip)
    out.update(check_walk_completed(parsed, walks))
    out.update(check_guest_progressed(screen_before, screen_after,
                                      randseed_at_attach, randseed_final))
    out.update(check_return_segments(parsed, load_seg))
    out.update(reconcile_final_randseed(parsed["draws"], seed, randseed_final))
    return out


def group_by_turn(draws):
    """Draws bucketed by how many top-level prompt reads preceded them."""
    turns = {}
    for d in draws:
        turns.setdefault(d["turn"], []).append(d)
    return turns
