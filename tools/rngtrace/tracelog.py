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
 10. `state samples` (Task 11i) -- every turn marker must carry one state
     sample, there must be at least `walks + 1` of them, and the LAST one must
     equal the `final_state` read out of a whole-memory dump.  The samples come
     over gdb and `final_state` comes over the qemu monitor, so this compares
     two independent paths into guest memory: a sample list that lost an entry
     (and so sits on the wrong turns), or a field read at the wrong address or
     the wrong width in either path, fails here rather than being published.

Guards 4, 5, 6 and 7 were added in fix wave 1; 4/5/6 close the gdb-script-abort
path, which passed every one of the original guards.
"""
import re

from . import rng

RE_DRAW = re.compile(r"^R ([0-9a-f]{4}) ([0-9a-f]{4}) ([0-9a-f]{4}) ([0-9a-f]{4})\s*$")
RE_PROMPT = re.compile(r"^P\s*$")
# The per-turn state sample (Task 11i): `S` followed by one `%x` per name in
# `run.state_fields()`, in that order.  Deliberately shape-only here -- the
# count is checked against the caller's name list, so a widened table that the
# reader was not told about produces an UNPARSED line (which fails the run)
# rather than a silently shifted set of columns.
RE_STATE = re.compile(r"^S ((?:[0-9a-f]+)(?: [0-9a-f]+)*)\s*$")
# Task 13's two extra markers, same shape-only discipline as `S`:
#   `F` + `E <values>`  -- one fight, and the enemy record it was entered with
#                          (1000:3d11).
#   `C` + `B <values>`  -- one `Битва\` prompt, and both fighters' hp and break
#                          flags at that moment (1000:441d).
# A value list whose length does not match the name list the reader was handed
# lands in `unparsed`, which fails the run.
RE_FIGHT = re.compile(r"^F\s*$")
RE_ENEMY = re.compile(r"^E ((?:[0-9a-f]+)(?: [0-9a-f]+)*)\s*$")
RE_ROUND_MARK = re.compile(r"^C\s*$")
RE_ROUND = re.compile(r"^B ((?:[0-9a-f]+)(?: [0-9a-f]+)*)\s*$")
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


def parse(text: str, state_names=(), enemy_names=(), round_names=()) -> dict:
    """Turn a gdb log into {draws, prompts, unexpected, aborts, unparsed, ...}.

    `state_names` is the ordered field list the log's `S` lines were printed
    from (`run.state_field_names()`).  It defaults to empty so the five
    committed pre-Task-11i logs still parse; an `S` line seen without it, or
    one whose value count differs from the name count, lands in `unparsed` and
    therefore fails `check_unparsed`.

    `enemy_names` / `round_names` are the same contract for Task 13's fight
    channels (`E` and `B` lines, written only by
    `gdbsession.build_fight_script`).  They also default to empty, so a log
    from `build_script` -- which never emits those tags -- parses exactly as it
    did before, and a fight log parsed WITHOUT the names fails rather than
    dropping the lines.
    """
    state_names = list(state_names)
    enemy_names = list(enemy_names)
    round_names = list(round_names)
    ready = None
    draws = []
    state_samples = []
    fights = []
    rounds = []
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
        m = RE_STATE.match(line)
        if m:
            values = [int(v, 16) for v in m.group(1).split()]
            if state_names and len(values) == len(state_names):
                state_samples.append({"turn": prompts,
                                      "draws_before": len(draws),
                                      "values": dict(zip(state_names, values))})
                seq.append("S")
                continue
            unparsed.append(line)
            continue
        if RE_FIGHT.match(line):
            fights.append({"index": len(fights) + 1, "turn": prompts,
                           "draws_before": len(draws), "enemy": None,
                           "prompts": 0})
            seq.append("F")
            continue
        m = RE_ENEMY.match(line)
        if m:
            values = [int(v, 16) for v in m.group(1).split()]
            if enemy_names and len(values) == len(enemy_names) and fights \
                    and fights[-1]["enemy"] is None:
                fights[-1]["enemy"] = dict(zip(enemy_names, values))
                seq.append("E")
                continue
            unparsed.append(line)
            continue
        if RE_ROUND_MARK.match(line):
            rounds.append({"index": len(rounds) + 1, "turn": prompts,
                           "fight": len(fights), "draws_before": len(draws),
                           "values": None})
            if fights:
                fights[-1]["prompts"] += 1
            seq.append("C")
            continue
        m = RE_ROUND.match(line)
        if m:
            values = [int(v, 16) for v in m.group(1).split()]
            if round_names and len(values) == len(round_names) and rounds \
                    and rounds[-1]["values"] is None:
                rounds[-1]["values"] = dict(zip(round_names, values))
                seq.append("B")
                continue
            unparsed.append(line)
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
            "state_samples": state_samples,
            "fights": fights, "combat_prompts": rounds,
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

    The bound is `walks + 1`, not `walks`.  The game stops at the prompt once
    before the first `w` is typed and once after each completed turn, so a
    healthy run of N walks records N+1 stops -- which all five captured runs
    do.  Requiring only N would tolerate exactly one lost turn, and there are
    two ways to spend that slack that every other guard passes: a freeze during
    the FINAL walk, and a single mis-classified screen where `driver.walk`
    counts a turn the game never took.  Neither can corrupt logged data (the
    LCG replay and the final-RandSeed reconciliation still prove no draw that
    happened is missing), so what leaks is coverage, not correctness -- but a
    silently short DRIVE is the same class of defect as a silently short trace,
    and this guard exists to refuse it.
    """
    stops = parsed["prompt_stops"]
    if stops < walks + 1:
        raise TraceError(
            "the driver typed `w` %d times, so the guest should have stopped "
            "at the top-level ReadLn (1000:ae63) %d times (once before the "
            "first walk, once after each), but it stopped only %d times: it "
            "stopped making progress while the driver kept typing into a "
            "frozen screen.  The trace is truncated and must not be published."
            % (walks, walks + 1, stops))
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


def check_state_samples(parsed: dict, *, walks, names, final_state):
    """The per-turn state channel: one sample per turn marker, and the last of
    them must agree with the full-memory read.

    Three failures this refuses, each of which would otherwise publish a state
    trace that is quietly wrong rather than obviously missing:

      * **a stop with no sample.** Every `1000:ae63` stop prints `P` and then
        the sample; if the two counts disagree, some sample was lost (a failed
        read aborts the sourced script, which `check_script_abort` catches --
        but a sample dropped any other way would leave the remaining ones
        shifted onto the wrong turns).
      * **fewer samples than turns.** `walks + 1` stops are required by
        `check_walk_completed` (once before the first `w`, once after each), so
        a shorter sample list is a truncated trace by the same argument.
      * **the two transports disagreeing.** The samples come over gdb's remote
        protocol from `run.state_fields()`'s addresses; `final_state` is the
        same table read out of a `pmemsave` dump of the whole guest after the
        drive.  The guest sits in `ReadLn` between the two reads and changes
        none of these variables there, so they must be equal -- and if they are
        not, the addresses or the widths are wrong in one of the two paths and
        nothing downstream could tell which.
    """
    out = check_state_sample_shape(parsed, walks=walks, names=names)
    out.update(reconcile_last_sample(parsed, names=names,
                                     final_state=final_state))
    return out


def check_state_sample_shape(parsed: dict, *, walks, names):
    """The first three failures `check_state_samples` refuses -- a stop with no
    sample, fewer samples than turns, and a sample missing a field.

    Split out (Task 13) so the fight capture can require exactly these while
    reconciling the two transports differently: a run that ends inside a fight
    is not sitting at the turn marker when the final dump is taken, so
    `reconcile_last_sample`'s premise does not hold there.  Nothing about the
    shape checks changes -- `check_state_samples` still runs both halves, in
    the same order, and is what `run.py` calls.
    """
    samples = parsed["state_samples"]
    names = list(names)
    if not names:
        raise TraceError("no state field names were given, so the per-turn "
                         "state channel cannot be verified: an unverified "
                         "channel must not be published")
    if len(samples) != parsed["prompt_stops"]:
        raise TraceError(
            "the guest stopped at the top-level ReadLn %d times but only %d "
            "state samples were logged: the samples that remain sit on the "
            "wrong turns" % (parsed["prompt_stops"], len(samples)))
    if len(samples) < walks + 1:
        raise TraceError(
            "%d state samples for %d walks: a healthy run samples once before "
            "the first `w` and once after each turn (%d)"
            % (len(samples), walks, walks + 1))
    want_turns = list(range(1, len(samples) + 1))
    if [s["turn"] for s in samples] != want_turns:
        raise TraceError("state samples are not one per consecutive turn: %s"
                         % [s["turn"] for s in samples])
    for s in samples:
        if sorted(s["values"]) != sorted(names):
            raise TraceError("a state sample does not carry every field: %s"
                             % sorted(set(names) ^ set(s["values"])))
    return {"state_samples": len(samples),
            "state_fields_per_sample": len(names)}


def reconcile_last_sample(parsed: dict, *, names, final_state):
    """The two transports must agree: the LAST per-turn sample (gdb reads at
    1000:ae63) against `final_state` (a `pmemsave` dump of the whole guest).

    Its premise is that the guest sits in `ReadLn` at the top-level prompt
    between the two reads and changes none of these variables there.  That is
    true of every `run.py` drive, which always ends at the street prompt; it is
    NOT true of a fight capture whose player died mid-turn, and
    `verify_combat_run` therefore only calls this when the drive ended at the
    turn marker -- verifying both transports against the LCG instead when it
    did not.
    """
    samples = parsed["state_samples"]
    names = list(names)
    last = samples[-1]["values"]
    differ = {k: (last[k], final_state[k]) for k in names
              if k in final_state and last[k] != final_state[k]}
    if differ:
        raise TraceError(
            "the last per-turn state sample (gdb reads at 1000:ae63) disagrees "
            "with final_state (a pmemsave dump after the drive) on %s -- the "
            "two paths into guest memory do not agree, so neither can be "
            "trusted: %r" % (sorted(differ), differ))
    missing = [k for k in names if k not in final_state]
    if missing:
        raise TraceError("final_state is missing sampled field(s) %s, so the "
                         "reconciliation covers less than it claims" % missing)
    return {"final_state_matches_last_sample": True}


def check_sample_seeds(parsed: dict, seed: int, *, channels):
    """Every gdb-read `RandSeed` must equal the LCG stepped once per draw
    logged before it.

    This is what replaces the two-transport comparison on a run that cannot
    end at the turn marker, and it is not a weaker substitute: instead of
    checking the gdb path against the `pmemsave` path, it checks EACH of them
    against `docs/re/rng.md`'s recurrence.  `reconcile_final_randseed` already
    pins the `pmemsave` value that way; this pins every gdb sample the same
    way, at every turn, every fight and every combat prompt -- so a sample read
    at the wrong address or the wrong width fails here, and so does a sample
    sitting at the wrong point in the draw stream.

    `channels` is `[(label, samples, field), ...]`; each sample must carry
    `draws_before` and `values[field]`.  An empty channel is refused: a channel
    that verified nothing must not report a pass.
    """
    out = {}
    for label, samples, field in channels:
        if not samples:
            raise TraceError(
                "the %s channel has no samples, so `check_sample_seeds` would "
                "pass by checking nothing" % label)
        for s in samples:
            if field not in s["values"]:
                raise TraceError("%s sample %r carries no %s"
                                 % (label, s, field))
            want = seed
            for _ in range(s["draws_before"]):
                want = rng.step(want)
            got = s["values"][field]
            if got != want:
                raise TraceError(
                    "%s sample after %d draws holds RandSeed 0x%08X, but the "
                    "LCG stepped %d times from 0x%08X gives 0x%08X: the sample "
                    "was read at the wrong address/width, or it does not sit "
                    "where the draw stream says it does"
                    % (label, s["draws_before"], got, s["draws_before"], seed,
                       want))
        out["%s_seeds_match_lcg" % label] = len(samples)
    return out


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
               state_names, final_state, min_draws=1, max_skip=0):
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
    out.update(check_state_samples(parsed, walks=walks, names=state_names,
                                   final_state=final_state))
    return out


def check_fight_markers(parsed: dict, *, enemy_names, round_names):
    """Task 13: every fight marker carries its enemy record, every combat
    prompt carries its round sample, and no fight is empty.

    Each `F` (1000:3d11) is followed by exactly one `E` line and each `C`
    (1000:441d) by exactly one `B` line; `parse` only attaches a payload to a
    marker that has none, so a lost or duplicated payload leaves a marker with
    `None` here -- and an EXTRA payload lands in `unparsed`, which
    `check_unparsed` already fails on.  Without this the fight channel could be
    published with holes that look like fights the guest never had.

    `prompts == 0` for a fight is refused as well: a stop at 1000:3d11 that
    never reached 1000:441d would mean the combat function was entered and left
    without its own prompt ever being read, which no path in
    `docs/re/gaps.md`'s nine-verb dispatch does -- so it is a lost marker, not
    a short fight.
    """
    enemy_names = list(enemy_names)
    round_names = list(round_names)
    if not enemy_names or not round_names:
        raise TraceError("the fight channel has no field names, so it cannot "
                         "be verified: an unverified channel must not be "
                         "published")
    fights = parsed["fights"]
    rounds = parsed["combat_prompts"]
    missing = [f["index"] for f in fights if f["enemy"] is None]
    if missing:
        raise TraceError(
            "fight marker(s) %s carry no enemy record: the 1000:3d11 stop was "
            "logged but its `E` sample was not, so those fights would be "
            "published with a hole" % missing)
    missing = [r["index"] for r in rounds if r["values"] is None]
    if missing:
        raise TraceError(
            "combat prompt(s) %s carry no round sample: the 1000:441d stop was "
            "logged but its `B` sample was not" % missing)
    for f in fights:
        if not f["prompts"]:
            raise TraceError(
                "fight %d logged no combat prompt at all: FUN_1000_3d11 was "
                "entered at 1000:3d11 and 1000:441d never fired, which no arm "
                "of its dispatch does -- a marker was lost"
                % f["index"])
    for f in fights:
        if sorted(f["enemy"]) != sorted(enemy_names):
            raise TraceError("fight %d's enemy record does not carry every "
                             "field: %s" % (f["index"],
                                            sorted(set(enemy_names) ^ set(f["enemy"]))))
    for r in rounds:
        if sorted(r["values"]) != sorted(round_names):
            raise TraceError("combat prompt %d does not carry every field: %s"
                             % (r["index"],
                                sorted(set(round_names) ^ set(r["values"]))))
    stray = [r["index"] for r in rounds if r["fight"] == 0]
    if stray:
        raise TraceError(
            "combat prompt(s) %s were logged before any fight marker: the "
            "1000:3d11 breakpoint missed a fight the 1000:441d one saw" % stray)
    return {"fights": len(fights), "combat_prompts": len(rounds),
            "fight_fields": len(enemy_names),
            "round_fields": len(round_names),
            "every_fight_has_an_enemy_record": True,
            "every_combat_prompt_has_a_round_sample": True}


def verify_combat_run(parsed: dict, seed: int, *, walks_completed, load_seg,
                      screen_before, screen_after, randseed_at_attach,
                      randseed_final, state_names, final_state, enemy_names,
                      round_names, ended_at_turn_marker, seed_field,
                      min_draws=1, max_skip=0):
    """Task 13's whole-run verification.  Keyword-only, for the same reason
    `verify_run` is: leaving a guard out must be a TypeError.

    It is `verify_run` plus `check_fight_markers`, with ONE substitution:
    `check_walk_completed` is passed the number of walks the driver saw
    COMPLETE (a turn that came back to the street prompt) rather than the
    number it set out to do.  A fight capture can end its drive early -- the
    player dying inside a turn takes the guest to `FUN_1000_074b` and out of
    the process, and that turn never returns to 1000:ae63 -- so requiring
    `requested + 1` stops would fail every losing run.  The bound is otherwise
    the identical `n + 1` argument: one stop before the first `w`, one after
    each completed turn.

    `expect_breakpoints=4`, because `build_fight_script` installs four.
    """
    check_install(parsed, expect_breakpoints=4)
    check_nonempty(parsed, min_draws=min_draws)
    out = {}
    out.update(check_unparsed(parsed))
    out.update(check_script_abort(parsed))
    skip, states = replay(parsed["draws"], seed, max_skip=max_skip)
    for d, st in zip(parsed["draws"], states):
        d["seed_after"] = st
    out.update({"lcg_replay": "match", "leading_states_skipped": skip,
                "draws_verified": len(parsed["draws"])})
    out.update(check_walk_completed(parsed, walks_completed))
    out.update(check_guest_progressed(screen_before, screen_after,
                                      randseed_at_attach, randseed_final))
    out.update(check_return_segments(parsed, load_seg))
    out.update(reconcile_final_randseed(parsed["draws"], seed, randseed_final))
    out.update(check_state_sample_shape(parsed, walks=walks_completed,
                                        names=state_names))
    out.update(check_fight_markers(parsed, enemy_names=enemy_names,
                                   round_names=round_names))
    out.update(check_sample_seeds(parsed, seed, channels=[
        ("turn", parsed["state_samples"], "randseed_367e"),
        ("fight", [dict(values=f["enemy"], draws_before=f["draws_before"])
                   for f in parsed["fights"]], seed_field["fight"]),
        ("combat_prompt", parsed["combat_prompts"], seed_field["round"]),
    ]))
    if ended_at_turn_marker:
        out.update(reconcile_last_sample(parsed, names=state_names,
                                         final_state=final_state))
    else:
        # Stated, never silent.  The premise of the two-transport comparison is
        # that the guest sits in the top-level `ReadLn` between the gdb sample
        # and the `pmemsave` dump; a drive that ended inside a fight (the
        # player died, and `1000:5053` -> FUN_1000_074b -> Halt took the guest
        # out of the process) does not satisfy it, and forcing the comparison
        # there would compare two different moments.  Both transports are still
        # verified, each against the LCG rather than against each other:
        # `check_sample_seeds` above for the gdb path and
        # `reconcile_final_randseed` for the `pmemsave` path.
        out["final_state_matches_last_sample"] = (
            "not applicable: the drive did not end at the turn marker, so the "
            "last 1000:ae63 sample and the final dump are different moments.  "
            "Both transports are verified against the LCG instead -- see "
            "turn_seeds_match_lcg and final_randseed_matches_replay.")
    return out


def group_by_turn(draws):
    """Draws bucketed by how many top-level prompt reads preceded them."""
    turns = {}
    for d in draws:
        turns.setdefault(d["turn"], []).append(d)
    return turns
