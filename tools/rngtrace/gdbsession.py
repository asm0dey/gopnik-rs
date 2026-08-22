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

IMAGE_OFF_RANDOM_RETF = 0xF78 * 16 + OFF_RANDOM_RETF     # 0x108e5
IMAGE_OFF_MAIN_READLN = OFF_MAIN_READLN                  # segment 1000 == image base


GDB_C_TYPE = {1: "unsigned char", 2: "unsigned short", 4: "unsigned int"}


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
    if not state_fields:
        raise ValueError("no state fields: the per-turn state channel would be "
                         "silently absent, and a missing channel must not look "
                         "like an empty one")
    reads = []
    for name, image_off, width in state_fields:
        try:
            ctype = GDB_C_TYPE[width]
        except KeyError:
            raise ValueError("%s: width %d has no gdb type" % (name, width))
        reads.append("*(%s*)(%s)" % (ctype, hex(image_base + image_off)))
    fmt = " ".join("%x" for _ in reads)
    return 'printf "S %s\\n", %s' % (fmt, ", ".join(reads))


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
