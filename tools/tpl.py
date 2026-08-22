#!/usr/bin/env python3
"""Reader for a Turbo Pascal 7 resident library (`TURBO.TPL`) and its units.

`orig/g.exe` links the Turbo Pascal 7 runtime.  That runtime is not this
project's unknown: it ships, compiled, in `BIN/TURBO.TPL` of a TP 7
distribution, together with each unit's symbol table.  This module reads that
file so `tools/rtlmatch.py` can align the runtime segments of `g.exe` against
it instead of reverse engineering them.

Nothing here reads `orig/g.exe`; nothing here decides what a routine *is*.
This is a file-format reader and only that.

The layout below was recovered from the file itself, from the smallest unit
outward (`PRINTER`, 480 bytes, one procedure) and then checked against the
largest (`SYSTEM`).  Every offset is derived, none is guessed:

    TPL          = TPU | TPU | ...        (units concatenated, no directory)
    TPU          = header | symbols | entries | blocks | ... | code | fixups

    header       96 bytes, `TPUQ` then 16-bit fields.  `w[6]..w[15]` are
                 offsets of the sections that follow the symbol area, in
                 ascending order; `w[17]` is the CODE size and `w[21]` the
                 unit's DGROUP size.
    code         starts at `align16(w[15])`, runs for `w[17]` bytes, and holds
                 the unit's typed constants as well as its code.
    fixups       start at `align16(code_end)` and run to the end of the unit,
                 8 bytes each: `kind` word, `arg`, `arg2`, `off`.
    blocks       `w[7]..w[8]`, 8 bytes each: the second word is the block's
                 SIZE.  Blocks tile the code section exactly in table order --
                 `check_unit()` asserts that, and it is what makes the linker's
                 block-granular smart linking legible: a program keeps a subset
                 of the blocks, in order, and drops the rest.
    entries      `w[6]+4 .. w[7]`, 8 bytes each: `tag` (a block table BYTE
                 offset) and `val` (an offset within that block).  An entry's
                 own byte offset in this table is the linkage token a fixup in
                 another unit carries, so `entry_offset -> (block, offset)` is
                 the cross-unit call ABI.
    symbols      `w[5] .. w[6]`, records of `type, len, name, payload`.  Two
                 record types matter here:
                   0x52  a normal procedure/function: payload word 2 is the
                         ENTRY TABLE OFFSET, so the name resolves to code.
                   0x51  a variable: the payload word at offset 1 is its
                         DGROUP offset.
                   0x56/0x57  a STANDARD procedure/function (`Random`, `Move`,
                         ...).  Its payload carries a compiler intrinsic id,
                         NOT an entry offset -- see the warning below.

**The `SYSTEM` unit names do not resolve to code.**  `CRT`, `DOS`, `OVERLAY`
and `PRINTER` export ordinary routines (type 0x52) whose records point into
the entry table, so their names land on exact code offsets.  `SYSTEM` exports
only *standard* procedures and functions, and those records carry a small
intrinsic id (`Assign` = 0x28, `Random` = 0xa0, ...) that the COMPILER maps to
an entry offset; the mapping is inside `TPC.EXE`, not in the unit.  So this
module can tell you every `SYSTEM` entry point's address and every `SYSTEM`
global's name, but not which standard procedure a given entry point is.  Do
not paper over that gap by matching the two lists up by position.

Standard library only.  The TPL is not part of this repository; point
`GOPNIK_TPL` at one, or pass a path.
"""
import os
import re
import struct
from pathlib import Path

__all__ = [
    "TplError", "Unit", "Block", "Entry", "Fixup", "Symbol",
    "SIGNATURE", "default_path", "read_tpl", "units", "check_unit",
    "entry_base_phase",
    "FIXUP_FAR_CALL",
]


class TplError(ValueError):
    """The bytes are not the TPU/TPL structure this module documents."""


SIGNATURE = b"TPUQ"          # Turbo Pascal 7's unit signature
_HDR = 0x60                  # fixed header size, = w[5] in every unit seen
FIXUP_FAR_CALL = 0x30        # high byte of a fixup that is a cross-unit call


def default_path():
    """`$GOPNIK_TPL`, else the TP 7 install this work was done against."""
    return Path(os.environ.get(
        "GOPNIK_TPL", str(Path.home() / "Downloads" / "TP" / "BIN" / "TURBO.TPL")))


def read_tpl(path=None) -> bytes:
    p = Path(path) if path else default_path()
    return p.read_bytes()


class Block:
    """One smart-linkable run of code, and where it sits in the unit."""

    __slots__ = ("index", "tag", "off", "size")

    def __init__(self, index, tag, off, size):
        self.index, self.tag, self.off, self.size = index, tag, off, size

    def __repr__(self):
        return "Block(%d tag=%#06x off=%#06x size=%#06x)" % (
            self.index, self.tag, self.off, self.size)


class Entry:
    """One entry-point token: `off` is the value another unit's fixup carries."""

    __slots__ = ("off", "tag", "val")

    def __init__(self, off, tag, val):
        self.off, self.tag, self.val = off, tag, val

    def __repr__(self):
        return "Entry(+%#06x tag=%#06x val=%#06x)" % (self.off, self.tag, self.val)


class Fixup:
    __slots__ = ("kind", "arg", "arg2", "off")

    def __init__(self, kind, arg, arg2, off):
        self.kind, self.arg, self.arg2, self.off = kind, arg, arg2, off

    def __repr__(self):
        return "Fixup(kind=%#06x arg=%#06x arg2=%#06x off=%#06x)" % (
            self.kind, self.arg, self.arg2, self.off)


class Symbol:
    __slots__ = ("type", "name", "at", "payload")

    def __init__(self, type_, name, at, payload):
        self.type, self.name, self.at, self.payload = type_, name, at, payload

    @property
    def entry_off(self):
        """Entry-table offset for a type-0x52 record, else None.

        The record is `52 len name 09 00 <entry off> 00 ...`; the entry offset
        is the second word after the name.  Returning None for every other
        record type is deliberate: a 0x56/0x57 (standard procedure) record's
        first word is an intrinsic id in the compiler's numbering and pointing
        it at the entry table would be a 64-different-routines mistake.
        """
        if self.type != 0x52 or len(self.payload) < 4:
            return None
        return struct.unpack_from("<H", self.payload, 2)[0]

    @property
    def data_off(self):
        """DGROUP offset for a type-0x51 (variable) record, else None."""
        if self.type != 0x51 or len(self.payload) < 3:
            return None
        return struct.unpack_from("<H", self.payload, 1)[0]

    def __repr__(self):
        return "Symbol(%02x %s)" % (self.type, self.name)


_NAME = re.compile(rb"[A-Za-z_][A-Za-z0-9_]*\Z")


class Unit:
    """One TPU inside the library."""

    def __init__(self, data, start, end):
        self.data, self.start, self.end = data, start, end
        if data[start:start + 4] != SIGNATURE:
            raise TplError("no %r signature at %#x" % (SIGNATURE, start))
        self.w = struct.unpack_from("<24H", data, start)
        if self.w[5] != _HDR:
            raise TplError("unit at %#x: header size %#x, expected %#x"
                           % (start, self.w[5], _HDR))
        self.code_off = (self.w[15] + 15) & ~15
        self.code_size = self.w[17]
        self.data_size = self.w[21]
        if self.code_off + self.code_size > end - start:
            raise TplError("unit at %#x: code runs past the unit" % start)

    # --- naming ---------------------------------------------------------

    @property
    def source(self):
        """The `.PAS` file name recorded in the unit, e.g. `SYSTEM.PAS`."""
        m = re.search(rb"[A-Z0-9]+\.PAS", self.data[self.start:self.end])
        return m.group(0).decode() if m else None

    @property
    def name(self):
        s = self.source
        return s[:-4] if s else None

    # --- sections -------------------------------------------------------

    @property
    def code(self) -> bytes:
        a = self.start + self.code_off
        return self.data[a:a + self.code_size]

    def blocks(self):
        a, b = self.start + self.w[7], self.start + self.w[8]
        out, off = [], 0
        for i in range((b - a) // 8):
            size = struct.unpack_from("<H", self.data, a + i * 8 + 2)[0]
            out.append(Block(i, i * 8, off, size))
            off += size
        return out

    def entries(self):
        """The entry table.

        The base is `w[6] + 4` in every unit of the library, which is not a
        guess: at any other phase the records stop being valid (a tag that is
        not a block, or an offset past its block), and the phase is picked by
        that test in `entry_base_phase()` rather than by eye.  A record with
        tag `0xffff` is an unused slot -- `SYSTEM` has one at offset 0.
        """
        a, b = self.start + self.w[6] + 4, self.start + self.w[7]
        out = []
        for i in range((b - a) // 8):
            tag, val = struct.unpack_from("<HH", self.data, a + i * 8)
            out.append(Entry(i * 8, tag, val))
        return out

    def fixups(self):
        a = self.start + ((self.code_off + self.code_size + 15) & ~15)
        out = []
        for p in range(a, self.end - 7, 8):
            out.append(Fixup(*struct.unpack_from("<4H", self.data, p)))
        return out

    def symbols(self):
        """Public symbol records, scanned over the symbol area.

        The area is a run of variable-length records whose payload size varies
        by type, so this scans for `type, len, name` with a plausible name
        rather than walking record by record; a walk that mis-sizes one
        payload silently desynchronises everything after it, and this task
        only needs the records it can positively recognise.
        """
        a, b = self.start + _HDR, self.start + self.w[6]
        out, p = [], a
        while p < b - 2:
            t = self.data[p]
            n = self.data[p + 1]
            if t in (0x51, 0x52, 0x56, 0x57) and 1 <= n <= 32 and p + 2 + n <= b:
                nm = self.data[p + 2:p + 2 + n]
                if _NAME.match(nm):
                    out.append(Symbol(t, nm.decode(), p - self.start,
                                      self.data[p + 2 + n:p + 2 + n + 16]))
                    # Resume just after the NAME, not after a guessed payload
                    # size: payload width varies by record type and a wrong
                    # width desynchronises every record after it.
                    p += 2 + n
                    continue
            p += 1
        return out


def units(data):
    """Every unit in a `.TPL`, in file order.  A bare `.TPU` yields one."""
    starts = [m.start() for m in re.finditer(re.escape(SIGNATURE), data)
              if m.start() == 0 or _plausible(data, m.start())]
    if not starts:
        raise TplError("no TPU signature found")
    out = []
    for i, s in enumerate(starts):
        e = starts[i + 1] if i + 1 < len(starts) else len(data)
        out.append(Unit(data, s, e))
    return out


def _plausible(data, s):
    """A `TPUQ` at `s` starts a unit only if its header fields make sense."""
    if s + _HDR > len(data):
        return False
    w = struct.unpack_from("<24H", data, s)
    return w[5] == _HDR and 0 < w[15] < len(data) - s


def check_unit(u) -> dict:
    """Assert the structural invariants, and return them as evidence.

    The load-bearing one is that the block sizes sum EXACTLY to the code
    size: that is what licenses treating the code section as a tiling of
    blocks in table order, which is how `tools/rtlmatch.py` recovers which
    blocks a program kept.  A sum that merely fits would not.
    """
    blks = u.blocks()
    total = sum(b.size for b in blks)
    if total != u.code_size:
        raise TplError("%s: blocks sum to %#x, code section is %#x"
                       % (u.name, total, u.code_size))
    ents = [e for e in u.entries() if e.tag != 0xFFFF]
    tags = {b.tag for b in blks}
    bad = [e for e in ents if e.tag not in tags]
    if bad:
        raise TplError("%s: %d entry/entries name no block (e.g. %r)"
                       % (u.name, len(bad), bad[0]))
    over = [e for e in ents
            if e.val > next(b.size for b in blks if b.tag == e.tag)]
    if over:
        raise TplError("%s: entry %r points past its block" % (u.name, over[0]))
    phase = entry_base_phase(u)
    if phase not in (4, None):
        raise TplError("%s: entry table validates at phase %d, not 4"
                       % (u.name, phase))
    return {
        "unit": u.name,
        "source": u.source,
        "file_off": u.start,
        "code_off": u.code_off,
        "code_size": u.code_size,
        "data_size": u.data_size,
        "blocks": len(blks),
        "entries": len(ents),
        "fixups": len(u.fixups()),
        "entry_phase": phase,
    }


def entry_base_phase(u, span=8):
    """The phase (bytes after `w[6]`) at which the entry table validates.

    Returned rather than assumed so the choice is falsifiable: for every
    candidate phase this counts the 8-byte records whose `tag` names a real
    block and whose `val` lands inside that block AND is not the trivial
    `(0, 0)`.  On this library exactly one phase scores non-trivially, and it
    is 4 in all five units.  A tie or a zero score means the format assumption
    has broken and the caller should stop, not pick a winner.
    """
    blks = {b.tag: b.size for b in u.blocks()}
    scores = []
    for delta in range(span + 1):
        a = u.start + u.w[6] + delta
        b = u.start + u.w[7]
        n = 0
        for i in range((b - a) // 8):
            tag, val = struct.unpack_from("<HH", u.data, a + i * 8)
            if tag in blks and val <= blks[tag] and (tag or val):
                n += 1
        scores.append((n, delta))
    scores.sort(reverse=True)
    if scores[0][0] == 0 or scores[0][0] == scores[1][0]:
        # Not decidable.  `PRINTER` reaches this honestly: its entry region is
        # 8 bytes and exports nothing, so there is no evidence either way.
        # Returning None keeps the caller from treating a degenerate table as
        # a confirmation.
        return None
    return scores[0][1]


def _main(argv):
    import json
    import sys
    path = argv[1] if len(argv) > 1 else None
    data = read_tpl(path)
    print(json.dumps([check_unit(u) for u in units(data)], indent=2))
    return 0


if __name__ == "__main__":
    import sys
    raise SystemExit(_main(sys.argv))
