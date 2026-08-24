"""Drive gdb against the qemu guest stub and collect an auto-logged trace.

Why the trace loop is written as an explicit gdb `while` loop rather than
breakpoint `commands`: qemu's i386 gdbstub reports `$pc` as the raw 16-bit
`eip`, while a breakpoint is inserted at the LINEAR address
(`cs_base + eip`).  gdb therefore cannot match the stop against its own
breakpoint, reports a bare `SIGTRAP`, and never runs the breakpoint's
`commands` block.  Observed directly: a stop at Random printed
"Program received signal SIGTRAP" with `$pc = 0x114b` while the breakpoint
was recorded at `0x32d7b`.  The loop below stops on the same trap and
dispatches on `$pc` itself, which is exactly the Ghidra/segment offset.

The same mismatch makes the resume unsafe: because gdb does not know a
breakpoint is at the stop address, it never performs its usual
remove/step/reinsert dance, and QEMU re-traps on the very same instruction
forever.  Measured directly: a run with a plain `continue` loop logged 833654
bytes of stops, every one of them at the SAME `$pc` (`ae63`), while the guest
made no progress at all -- and `hbreak` behaved identically, so QEMU's
hardware breakpoints do not escape it either.  The loop therefore steps over
the breakpoint by hand: `disable`, `stepi`, `enable`.  Getting this wrong is
exactly the failure mode this harness must not have -- the guest looks alive
(its screen is unchanged, so a screen-driven driver keeps typing) while the
trace silently stops.

Turn marker, and the second channel: the stop at `1000:ae63` (the top-level
prompt's `ReadLn`) prints `P` and then one `S` line holding every variable
`run.state_fields()` names, read straight out of guest memory while the guest
is stopped there.  That is the per-turn state trace (Task 11i); it is written
to `data/state_trace.json` and never into `data/rng_trace.json`, which is the
frozen draw oracle.

Observation point: the `retf 2` at the tail of `Random` (`0f78:1165`).  There
the callee has restored SP to its entry value, so the caller's frame is still
intact -- `[sp]` return offset, `[sp+2]` return segment, `[sp+4]` the pushed
`n` -- AND `ax` already holds the result.  One stop per draw yields call site,
`n` and result together, with no entry/exit pairing to get wrong.
"""
import subprocess
import time
from pathlib import Path

# Ghidra/segment offsets the tracer dispatches on (these are what $pc reports).
OFF_RANDOM_ENTRY = 0x114B      # 0f78:114b
OFF_RANDOM_RETF = 0x1165       # 0f78:1165, the `retf word 0x2`
OFF_MAIN_READLN = 0xAE63       # 1000:ae63, the top-level prompt's ReadLn call
# Task 13's two extra markers, both inside FUN_1000_3d11 and both on confirmed
# instruction boundaries (`python3 tools/re_query.py resolve 1000:3d11` /
# `... 1000:441d`):
#   * 1000:3d11 is the combat function's own entry (`55` `89 e5`), reached once
#     per fight, and the enemy record at 20ae:3952.. is already rolled there.
#   * 1000:441d is the `Битва\` prompt's ReadLn (`9a c6 06 78 0f`), the same
#     runtime entry 1000:ae63 calls but with combat's own buffer DS:3a72
#     (`bf 72 3a` / `1e` / `57` at 1000:4414).  One stop per combat prompt.
OFF_COMBAT_ENTRY = 0x3D11      # 1000:3d11, FUN_1000_3d11's prologue
# Task 16's marker, and the probe's DEFAULT target: FUN_1000_1a03's own entry
# (`55` `89 e5`, re-derived with `python3 tools/re_query.py resolve 1000:1a03`).
# It is NEAR-called and ends in a bare `ret` at 1000:248e -- zero parameter
# bytes -- so a stop here has no argument to read.  The only question it
# answers is WHETHER a typed verb reaches it, so the probe samples nothing here
# and just counts stops.
#
# The target is a PARAMETER of `build_verbprobe_script`, not this constant:
# Task 17 re-points the same probe at FUN_1000_1348 (`1000:1348`, the `sv`
# handler).  The constant stays as the default so Task 16's invocation is
# unchanged.
OFF_SHEET_ENTRY = 0x1A03      # 1000:1a03, FUN_1000_1a03's prologue
OFF_COMBAT_READLN = 0x441D     # 1000:441d, the `Битва\` prompt's ReadLn call

IMAGE_OFF_RANDOM_RETF = 0xF78 * 16 + OFF_RANDOM_RETF     # 0x108e5
IMAGE_OFF_MAIN_READLN = OFF_MAIN_READLN                  # segment 1000 == image base
IMAGE_OFF_COMBAT_ENTRY = OFF_COMBAT_ENTRY                # segment 1000 == image base
IMAGE_OFF_COMBAT_READLN = OFF_COMBAT_READLN              # segment 1000 == image base
IMAGE_OFF_SHEET_ENTRY = OFF_SHEET_ENTRY                  # segment 1000 == image base


GDB_C_TYPE = {1: "unsigned char", 2: "unsigned short", 4: "unsigned int"}


def build_script(image_base: int, port: int, state_fields) -> str:
    """The whole trace loop, as a gdb script.  Nothing here writes a file: the
    log is gdb's own stdout, redirected by GdbSession.

    `state_fields` is required rather than optional: the per-turn state channel
    is part of what a run publishes (`data/state_trace.json`), and a default
    would let a caller drop it without anything failing.
    """
    retf = image_base + IMAGE_OFF_RANDOM_RETF
    readln = image_base + IMAGE_OFF_MAIN_READLN
    sample = state_printf(image_base, state_fields)
    return f"""set confirm off
set pagination off
set height 0
set width 0
set architecture i8086
target remote :{port}
break *{hex(retf)}
break *{hex(readln)}
info breakpoints
printf "READY base=%x retf=%x readln=%x\\n", {hex(image_base)}, {hex(retf)}, {hex(readln)}
while 1
  continue
  if $pc == {hex(OFF_RANDOM_RETF)}
    printf "R %04x %04x %04x %04x\\n", *(unsigned short*)($ss*16+$sp), *(unsigned short*)($ss*16+$sp+2), *(unsigned short*)($ss*16+$sp+4), $ax
  else
    if $pc == {hex(OFF_MAIN_READLN)}
      printf "P\\n"
      {sample}
    else
      printf "? %04x\\n", $pc
    end
  end
  disable
  stepi
  enable
end
"""


def _sample_printf(tag: str, image_base: int, fields) -> str:
    """One gdb `printf` that prints `<tag> <hex> <hex> ...` for `fields`.

    Same shape and the same guarantees as `state_printf` (which keeps its own
    name because `data/state_trace.json`'s reader is keyed to the `S` tag):
    the values come out in the order of `fields`, and the reader is handed the
    same order, so a widened table cannot silently shift columns -- a line
    whose value count does not match the name count is an UNPARSED line, which
    fails the run.
    """
    if not fields:
        raise ValueError("no fields for the %r sample: a missing channel must "
                         "not look like an empty one" % tag)
    reads = []
    for name, image_off, width in fields:
        try:
            ctype = GDB_C_TYPE[width]
        except KeyError:
            raise ValueError("%s: width %d has no gdb type" % (name, width))
        reads.append("*(%s*)(%s)" % (ctype, hex(image_base + image_off)))
    fmt = " ".join("%x" for _ in reads)
    return 'printf "%s %s\\n", %s' % (tag, fmt, ", ".join(reads))


def state_printf(image_base: int, state_fields) -> str:
    """The per-turn state sample, as one gdb `printf`.

    `state_fields` is `[(name, image_off, width), ...]` -- `run.state_fields()`.
    Targeted reads on purpose: the sampled variables are ~70 bytes, and the
    only other way to read guest memory here is the monitor's `pmemsave`, which
    pulls the whole 1 MiB AND cannot be aimed at this moment at all -- the
    Python side does not know when a breakpoint stopped the guest.  gdb is
    stopped at `1000:ae63` when this runs, so the sample is the state the
    top-level prompt is about to be read against.

    The values are printed in the order of `state_fields`, and the reader
    (`tracelog.parse`) is handed that same order, so a widened table cannot
    silently shift the columns: a line whose value count does not match the
    name count is an unparsed line, which fails the run.
    """
    return _sample_printf("S", image_base, state_fields)


def build_fight_script(image_base: int, port: int, state_fields,
                       fight_fields, round_fields) -> str:
    """Task 13's trace loop: `build_script`'s two breakpoints plus two more.

    `build_script` is left exactly as it was -- it is what produced
    `data/rng_trace.json` and `data/state_trace.json`, and neither is ever
    regenerated -- so the fight capture gets its own builder rather than a
    parameter that could change the frozen path.

    The two extra stops:

      * `1000:3d11` prints `F` and one line of `fight_fields` (the enemy
        record, already rolled by `FUN_1000_0d14` before combat is entered).
        One stop per fight, so it is also the fight delimiter the draw stream
        otherwise has no marker for.
      * `1000:441d` prints `C` and one line of `round_fields` (both fighters'
        hp and their four break flags).  One stop per `Битва\\` prompt, i.e.
        the state the previous round left behind.

    Four breakpoints, four `$pc` values, and an `else` that still reports an
    unexpected stop -- the dispatch cannot silently absorb one.
    """
    retf = image_base + IMAGE_OFF_RANDOM_RETF
    readln = image_base + IMAGE_OFF_MAIN_READLN
    fight = image_base + IMAGE_OFF_COMBAT_ENTRY
    croom = image_base + IMAGE_OFF_COMBAT_READLN
    sample = state_printf(image_base, state_fields)
    fsample = _sample_printf("E", image_base, fight_fields)
    csample = _sample_printf("B", image_base, round_fields)
    return f"""set confirm off
set pagination off
set height 0
set width 0
set architecture i8086
target remote :{port}
break *{hex(retf)}
break *{hex(readln)}
break *{hex(fight)}
break *{hex(croom)}
info breakpoints
printf "READY base=%x retf=%x readln=%x\\n", {hex(image_base)}, {hex(retf)}, {hex(readln)}
while 1
  continue
  if $pc == {hex(OFF_RANDOM_RETF)}
    printf "R %04x %04x %04x %04x\\n", *(unsigned short*)($ss*16+$sp), *(unsigned short*)($ss*16+$sp+2), *(unsigned short*)($ss*16+$sp+4), $ax
  else
    if $pc == {hex(OFF_MAIN_READLN)}
      printf "P\\n"
      {sample}
    else
      if $pc == {hex(OFF_COMBAT_ENTRY)}
        printf "F\\n"
        {fsample}
      else
        if $pc == {hex(OFF_COMBAT_READLN)}
          printf "C\\n"
          {csample}
        else
          printf "? %04x\\n", $pc
        end
      end
    end
  end
  disable
  stepi
  enable
end
"""


class GdbSession:
    """gdb -q -nx -x script, stdout captured; SIGINT + quit for a clean flush."""

    def __init__(self, script_path: Path, log_path: Path, gdb="gdb"):
        self.script_path = Path(script_path)
        self.log_path = Path(log_path)
        self.gdb = gdb
        self.proc = None
        self._log = None

    def start(self):
        self._log = open(self.log_path, "w")
        self.proc = subprocess.Popen(
            [self.gdb, "-q", "-nx", "-x", str(self.script_path)],
            stdin=subprocess.PIPE, stdout=self._log, stderr=subprocess.STDOUT,
            text=True)
        return self

    def wait_ready(self, timeout=60):
        end = time.time() + timeout
        while time.time() < end:
            if self.log_path.exists() and "READY" in self.log_path.read_text(errors="replace"):
                return True
            if self.proc.poll() is not None:
                raise RuntimeError("gdb exited before attaching:\n%s"
                                   % self.log_path.read_text(errors="replace"))
            time.sleep(0.2)
        raise TimeoutError("gdb never reported READY:\n%s"
                           % self.log_path.read_text(errors="replace"))

    def stop(self, timeout=20):
        """Quit gdb so its stdout is flushed.  KILL THE VM FIRST.

        While the script's `while` loop is running, gdb is not reading stdin,
        so `quit` alone cannot reach it.  Killing the VM makes the pending
        `continue` fail with "Remote connection closed", which aborts the
        sourced script and drops gdb to its prompt, where `quit` is read.
        SIGINT is deliberately NOT used: it produces an extra stop at whatever
        address the guest happens to be at, which the trace would have to
        record as an unexpected stop.
        """
        if self.proc is None:
            return
        if self.proc.poll() is None:
            try:
                self.proc.stdin.write("quit\n")
                self.proc.stdin.flush()
            except (BrokenPipeError, OSError):
                pass
            try:
                self.proc.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=10)
        if self._log:
            self._log.close()
            self._log = None

    def alive(self):
        return self.proc is not None and self.proc.poll() is None

    def __enter__(self):
        return self.start()

    def __exit__(self, *exc):
        self.stop()
        return False


def build_verbprobe_script(image_base: int, port: int,
                           target_off: int = OFF_SHEET_ENTRY) -> str:
    """The probe loop: three markers, no samples.

    The question is which typed verbs reach `target_off`, so the loop breaks on
    the two prompts that read a verb and on the target function's own entry,
    and prints one tag per stop:

      * `1000:ae63` -> `P` -- the top-level prompt's ReadLn, i.e. the guest is
        about to read a STREET verb.
      * `1000:441d` -> `C` -- the `Битва\\` prompt's ReadLn, i.e. the guest is
        about to read a COMBAT verb.
      * `target_off` -> `T` -- the target function was entered.

    A `T` between prompt stop `i` and prompt stop `i+1` was caused by the line
    typed at prompt `i`, and a verb whose window holds no `T` did not reach it.
    That negative is the whole point: it is a breakpoint that did NOT fire,
    which is the only evidence `docs/re/METHODOLOGY.md` accepts for one.

    `target_off` defaults to Task 16's `1000:1a03` and is a parameter because
    the same three-marker shape answers the question for any function reached
    from a prompt; Task 17 points it at `1000:1348`.  The READY line prints the
    target it actually installed, so a report can never name a function the
    breakpoint was not set on.

    No state sample is read: neither target takes parameters (both end in a
    bare `ret`), so there is nothing at the stop that would distinguish one
    caller from another, and a sample would only invite reading state as if it
    were flow.  The `else` still reports an unexpected `$pc`, so a fourth stop
    cannot be absorbed silently.
    """
    readln = image_base + IMAGE_OFF_MAIN_READLN
    croom = image_base + IMAGE_OFF_COMBAT_READLN
    target = image_base + target_off
    return f"""set confirm off
set pagination off
set height 0
set width 0
set architecture i8086
target remote :{port}
break *{hex(readln)}
break *{hex(croom)}
break *{hex(target)}
info breakpoints
printf "READY base=%x target=%x readln=%x\\n", {hex(image_base)}, {hex(target)}, {hex(readln)}
while 1
  continue
  if $pc == {hex(OFF_MAIN_READLN)}
    printf "P\\n"
  else
    if $pc == {hex(OFF_COMBAT_READLN)}
      printf "C\\n"
    else
      if $pc == {hex(target_off)}
        printf "T\\n"
      else
        printf "? %04x\\n", $pc
      end
    end
  end
  disable
  stepi
  enable
end
"""
