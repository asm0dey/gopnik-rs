#!/usr/bin/env python3
"""Align `orig/g.exe`'s runtime segments against a Turbo Pascal 7 `TURBO.TPL`.

`data/functions.json` records 107 functions outside the game's own code
segment.  Reverse engineering them one at a time is redundant work: the
Borland runtime is published, compiled, in `BIN/TURBO.TPL`.  This tool aligns
the two so a runtime routine can be *identified* instead of rediscovered.

How the alignment works, and why it can fail
--------------------------------------------

TP 7 links at BLOCK granularity (`tools/tpl.py`): each unit's code section is
a tiling of blocks, and a program keeps a subset of them, in table order, and
drops the rest.  So aligning is: walk the program's segment from offset 0 and,
at each position, find the next block of the unit whose bytes fit there.

"Fit" is not "equal".  The library stores each block with its address fields
UNRESOLVED -- a placeholder the linker overwrites -- so a correctly aligned
block still differs from the linked copy at every fixup.  The test used here
is therefore about the SHAPE of the difference: every maximal run of differing
bytes must be at most `MAX_FIXUP_RUN` (4) bytes long, the width of a far
pointer.  Unrelated code fails that immediately -- differences land in runs of
tens of bytes -- and `reject` below demonstrates it failing rather than
asserting that it would.

A block that anchors (its first `PREFIX` bytes fit) but then diverges in a
LONGER run is reported, not hidden: `long_runs` counts those runs.  That is
how the four routines this program links from a different build of the
runtime than this library's showed up.

Nothing here names anything.  Names live in `data/rtl_names.json`, which
`emit` writes; the runtime-unit names come from `tools/tpl.py`'s symbol tables
and the rest from `NAMES` below, each with its own evidence.

Standard library only.  The library is not in the repository, so `emit`,
`align`, `units` and `reject` all need one -- point `GOPNIK_TPL` at a
`TURBO.TPL` or pass the path.  `tools/test_rtlmatch.py` checks the committed
result against `orig/g.exe` and needs no library at all.
"""
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import addr  # noqa: E402
import tpl   # noqa: E402

REPO = Path(__file__).resolve().parent.parent
OUT_PATH = REPO / "data" / "rtl_names.json"
FUNCTIONS = REPO / "data" / "functions.json"

#: Longest run of differing bytes a single relocated field can produce: the
#: 4 bytes of a far pointer (`ptr_off` + `ptr_seg`).
MAX_FIXUP_RUN = 4
#: Bytes of a block that must fit before it is considered anchored here.
PREFIX = 12
#: How far past the end of a block to look for the next one.  A block linked
#: from a different build of the unit can be a few bytes longer or shorter
#: than the library's copy, which shifts everything after it.
SLACK = 24
#: A block whose prefix fits but whose body then differs in LONG runs -- runs
#: no single relocated field could produce -- over more than this fraction of
#: its bytes is not this block.  The prefix test alone is not enough:
#: `SYSTEM` block 35 opens with the same 12 bytes as `CRT` block 1, so
#: aligning `CRT`'s segment against `SYSTEM` "placed" 1139 bytes that differ
#: in 1127.  The measure has to be long-run bytes, not all differing bytes: a
#: small block that is genuinely this block can be nearly half relocation
#: (`CRT`'s 46-byte initialisation differs in 21 bytes, every one of them a
#: fixup).  On this image the two populations are far apart -- `reject` prints
#: both sides -- so the threshold sits between them.
MAX_LONG_RUN_FRACTION = 1.0 / 3.0

#: The runtime segments, as relative segment -> (image offset, length).  The
#: lengths are the distance to the next segment, so they include the padding
#: the linker inserts to reach a paragraph boundary.
SEGMENTS = {
    0x0EE5: (0x0EE50, 0x0080),
    0x0EED: (0x0EED0, 0x0290),
    0x0F16: (0x0F160, 0x0620),
    0x0F78: (0x0F780, 0x1360),
}


# --- difference shape --------------------------------------------------------

def diff_runs(a, b):
    """Maximal runs of differing bytes as `[start, end]` pairs, inclusive."""
    n = min(len(a), len(b))
    runs = []
    for i in range(n):
        if a[i] == b[i]:
            continue
        if runs and i == runs[-1][1] + 1:
            runs[-1][1] = i
        else:
            runs.append([i, i])
    return runs


def fits(a, b, max_run=MAX_FIXUP_RUN):
    """True when every differing run is short enough to be one relocated field."""
    return all(r[1] - r[0] + 1 <= max_run for r in diff_runs(a, b))


def _score(cand, seen):
    """`(long runs, bytes in them, all differing bytes, runs)`.

    Ordered so `min()` over candidates prefers the one that needs the fewest
    unexplained differences, then the fewest differences of any kind.
    """
    runs = diff_runs(cand, seen)
    big = [r for r in runs if r[1] - r[0] + 1 > MAX_FIXUP_RUN]
    return (len(big),
            sum(r[1] - r[0] + 1 for r in big),
            sum(r[1] - r[0] + 1 for r in runs),
            len(runs))


# --- alignment ---------------------------------------------------------------

def align(segment_bytes, unit, prefix=PREFIX, slack=SLACK):
    """Which blocks of `unit` this segment is, in order, and where.

    Returns `(placements, covered)`.  Each placement records the block, where
    it starts in the segment, and how well it matched.  `covered` is the
    offset the walk reached: for a segment that IS this unit it lands on the
    end of the code, and everything after it is the linker's padding.
    """
    blocks = unit.blocks()
    code = unit.code
    out, pos, first = [], 0, 0
    while pos < len(segment_bytes) and first < len(blocks):
        chosen = None
        for s in range(slack + 1):
            q = pos + s
            if q >= len(segment_bytes):
                break
            cands = []
            for j in range(first, len(blocks)):
                blk = blocks[j]
                n = min(prefix, blk.size, len(segment_bytes) - q)
                if n < 8:
                    continue
                cand = code[blk.off:blk.off + blk.size]
                if not fits(cand[:n], segment_bytes[q:q + n]):
                    continue
                m = min(blk.size, len(segment_bytes) - q)
                sc = _score(cand[:m], segment_bytes[q:q + m])
                if sc[1] > m * MAX_LONG_RUN_FRACTION:
                    continue
                cands.append(sc + (j, m))
            if cands:
                cands.sort()
                chosen = (s, cands[0])
                break
        if chosen is None:
            break
        s, (big, bigbytes, nbytes, nruns, j, m) = chosen
        blk = blocks[j]
        out.append({
            "block": blk.index,
            "tag": blk.tag,
            "unit_off": blk.off,
            "size": blk.size,
            "seg_off": pos + s,
            "matched_bytes": m,
            "diff_runs": nruns,
            "diff_bytes": nbytes,
            "long_runs": big,
            "long_run_bytes": bigbytes,
            "gap_before": s,
        })
        pos = pos + s + m
        first = j + 1
    return out, pos


def block_map(placements):
    """`tag -> segment offset of the block`, for resolving entry tokens."""
    return {p["tag"]: p["seg_off"] for p in placements}


# --- what the alignment says about one function ------------------------------

def classify(img, seg, placements, unit, entry_off, size):
    """Where a function at `entry_off` sits in the aligned unit, and how well.

    `mode` is `exact` when not one byte differs, `fixups_only` when every
    differing run is short enough to be a relocated field, `divergent` when a
    longer run says the program links a different build of that routine, and
    `unmatched` when no block covers it.
    """
    base, _ = SEGMENTS[seg]
    for p in placements:
        if not (p["seg_off"] <= entry_off < p["seg_off"] + p["matched_bytes"]):
            continue
        rel = entry_off - p["seg_off"]
        n = min(size, p["matched_bytes"] - rel)
        here = img[base + entry_off:base + entry_off + n]
        there = unit.code[p["unit_off"] + rel:p["unit_off"] + rel + n]
        runs = diff_runs(there, here)
        big = [r for r in runs if r[1] - r[0] + 1 > MAX_FIXUP_RUN]
        return {
            "unit": unit.name,
            "block_tag": p["tag"],
            "block_off": rel,
            "unit_code_off": p["unit_off"] + rel,
            "compared_bytes": n,
            "diff_runs": len(runs),
            "diff_bytes": sum(r[1] - r[0] + 1 for r in runs),
            "long_runs": len(big),
            "mode": ("exact" if not runs else
                     "fixups_only" if not big else "divergent"),
        }
    return {"unit": None, "mode": "unmatched"}


# --- names -------------------------------------------------------------------

# Names for the `SYSTEM` unit, which the library's symbol table CANNOT supply:
# `SYSTEM` exports only standard procedures, and their symbol records carry a
# compiler intrinsic id, not a code offset (see `tools/tpl.py`).  So every one
# of these rests on the routine's own disassembly, cited below; a few are
# additionally pinned by a cross-unit call whose caller is known.
#
# `kind` says what sort of name it is, and the distinction is load-bearing:
#   borland      -- a Borland standard-procedure name.  The routine implements
#                   that procedure and nothing else does.
#   behavioural  -- a name coined here for a routine with no user-visible
#                   Borland name (a helper, an operator).  It is a description,
#                   NOT a Borland symbol; do not cite it as one.
# Everything here is `established from flow` per docs/re/METHODOLOGY.md.
NAMES = {
    "0f78:0000": ("rtl_init", "behavioural",
                  "0f78:0000 sets ds to DGROUP (0x10ae), stores es (the PSP "
                  "segment) at 20ae:3678, calls the CPU probe at 0f78:00b1, "
                  "then sizes the stack and heap from sp"),
    "0f78:00b1": ("rtl_cpu_probe", "behavioural",
                  "0f78:00b1 pushes and pops FLAGS twice, masking bits 12-15 "
                  "(0f78:00b5 `and bh,0xf`, 0f78:00bc `and ch,0xf0`), the "
                  "8086/286/386 discrimination; it counts the CPU class in ax"),
    "0f78:010f": ("rtl_runerror_here", "behavioural",
                  "0f78:010f pops the caller's near return address into cx/bx "
                  "and falls into 0f78:011a, so the error report names the "
                  "call site"),
    "0f78:0116": ("rtl_halt", "behavioural",
                  "0f78:0116 stores the code at 20ae:3672, walks the exit "
                  "chain at 20ae:3650, and prints via 0f78:01f0/0f78:01fe; it "
                  "is the target of every `jmp 0x10f` error raise"),
    "0f78:01f0": ("rtl_write_cs_asciiz", "behavioural",
                  "0f78:01f0 loads `cs:[bx]`, stops at NUL, and writes each "
                  "byte through 0f78:0232"),
    "0f78:01fe": ("rtl_write_dec_word", "behavioural",
                  "0f78:01fe divides by 100 then by 10 through 0f78:020a -- "
                  "the three digits of a runtime error number"),
    "0f78:020a": ("rtl_write_dec_digit", "behavioural",
                  "0f78:020a does `div cl`, `add al,0x30`, writes, and returns "
                  "the remainder in al"),
    "0f78:0218": ("rtl_write_hex_word", "behavioural",
                  "0f78:0218 writes ah then al through 0f78:021f"),
    "0f78:021f": ("rtl_write_hex_byte", "behavioural",
                  "0f78:021f writes the high then the low nibble through "
                  "0f78:022a"),
    "0f78:022a": ("rtl_write_hex_digit", "behavioural",
                  "0f78:022a is `add al,0x30` with the `cmp al,0x3a` / "
                  "`add al,7` correction for A-F"),
    "0f78:0232": ("rtl_write_char_dos", "behavioural",
                  "0f78:0232 is INT 21h AH=06h with dl = al"),
    "0f78:028a": ("IOResult", "borland",
                  "0f78:028c `xchg [0x367c],ax` after `xor ax,ax` reads and "
                  "clears InOutRes in one step, which is exactly IOResult's "
                  "contract; 20ae:367c is InOutRes (SYSTEM's symbol table puts "
                  "it at unit data offset 0x3a, and RandSeed at 0x3c is "
                  "20ae:367e -- docs/re/rng.md)"),
    "0f78:0291": ("rtl_io_check", "behavioural",
                  "0f78:0291 raises InOutRes as a runtime error when it is "
                  "non-zero -- the code `{$I+}` emits after every I/O call"),
    "0f78:02cd": ("rtl_stack_check", "behavioural",
                  "0f78:02cd compares sp minus the requested frame against "
                  "20ae:367a and raises error 202 (0xca, stack overflow); it "
                  "is the `lcall 0f78:02cd` in every game prologue, e.g. "
                  "1000:3d17"),
    "0f78:02e6": ("Assign", "borland",
                  "0f78:02e6 zeroes a TextRec, writes mode 0xd7b0 (fmClosed) "
                  "and buffer size 0x80, and copies the name; PRINTER.TPU's "
                  "initialisation calls SYSTEM entry +0x228 -- which resolves "
                  "here -- with (Lst, 'LPT1')"),
    "0f78:0364": ("Reset", "borland",
                  "0f78:0364 sets dx to 0xd7b1 (fmInput) and falls into the "
                  "shared open at 0f78:0371; CRT.TPU's initialisation calls "
                  "SYSTEM entry +0x230, which resolves here"),
    "0f78:0369": ("Rewrite", "borland",
                  "0f78:0369 sets dx to 0xd7b2 (fmOutput) and falls into "
                  "0f78:0371; both PRINTER.TPU and CRT.TPU call SYSTEM entry "
                  "+0x238, which resolves here"),
    "0f78:0371": ("rtl_text_open", "behavioural",
                  "0f78:0371 is the body shared by 0f78:0364 and 0f78:0369: "
                  "it rejects a TextRec whose mode is not 0xd7b0/1/2 with "
                  "error 0x66 and calls the open vector"),
    "0f78:03be": ("Close", "borland",
                  "0f78:03be requires mode 0xd7b1 or 0xd7b2, else error 0x67 "
                  "(103, file not open), then calls the TextRec vector at "
                  "+0x14; entry +0x240 continues the +0x228/+0x230/+0x238 run "
                  "of Assign/Reset/Rewrite"),
    "0f78:03fa": ("rtl_text_call_vector", "behavioural",
                  "0f78:03fe `lcall es:[bx+di]` then records a non-zero result "
                  "in InOutRes"),
    "0f78:0499": ("rtl_text_read_bytes", "behavioural",
                  "0f78:0499 refills a TextRec (mode 0xd7b1) from its buffer, "
                  "calling the input vector when BufPos reaches BufEnd"),
    "0f78:04f7": ("rtl_text_write_bytes", "behavioural",
                  "0f78:04f7 appends dx bytes to a TextRec's buffer "
                  "(mode 0xd7b2), flushing when it fills"),
    "0f78:0546": ("rtl_text_write_block", "behavioural",
                  "0f78:0546 is 0f78:04f7's counterpart taking the count in "
                  "ax; 0f78:05dd calls it with ax=2 and si=0x3690"),
    "0f78:059d": ("ReadLn", "borland",
                  "0f78:059d reads through 0f78:0499 with ax=0x5bb -- the "
                  "skip-to-end-of-line handler -- and then runs the "
                  "end-of-statement flush at 0f78:0627"),
    "0f78:05dd": ("WriteLn", "borland",
                  "0f78:05dd writes exactly 2 bytes from 20ae:3690 through "
                  "0f78:0546: the CR/LF pair"),
    "0f78:05fe": ("rtl_text_flush_if_set", "behavioural",
                  "0f78:05fe calls the flush vector when TextRec+0x1a is "
                  "non-zero and InOutRes is clear -- the tail of a Write "
                  "statement"),
    "0f78:0619": ("rtl_text_inout_vector", "behavioural",
                  "0f78:061b `lcall es:[bx+0x14]`, recording the result in "
                  "InOutRes"),
    "0f78:0627": ("rtl_text_flush_vector", "behavioural",
                  "0f78:0629 `lcall es:[bx+0x18]`, recording the result in "
                  "InOutRes"),
    "0f78:0635": ("rtl_text_read_char", "behavioural",
                  "0f78:0635 takes one byte from a TextRec's buffer, calling "
                  "0f78:0619 when it is empty"),
    "0f78:067b": ("rtl_text_write_char", "behavioural",
                  "0f78:067b pads to the field width at [bp+6] through "
                  "0f78:04f7 and then stores one character"),
    "0f78:06c6": ("rtl_text_read_string", "behavioural",
                  "0f78:06c6 reads into the string at [bp+8] up to the maximum "
                  "length at [bp+6] using 0f78:0499 -- the `Read(Text,String)` "
                  "half of a ReadLn statement, whose other half is 0f78:059d"),
    "0f78:0701": ("rtl_text_write_string", "behavioural",
                  "0f78:0701 pads to the width at [bp+6] and writes the "
                  "string's own length byte's worth of bytes"),
    "0f78:072e": ("Assign", "borland",
                  "0f78:072e zeroes a 0x16-word FileRec, writes mode 0xd7b0 "
                  "and the name; it is the typed/untyped-file twin of "
                  "0f78:02e6 and entry +0x248 heads the +0x248/+0x250/+0x258/"
                  "+0x260 run that mirrors the text one"),
    "0f78:0769": ("Reset", "borland",
                  "0f78:0769 issues INT 21h AH=3Dh (open) with the access byte "
                  "from FileMode at 20ae:368e"),
    "0f78:0772": ("Rewrite", "borland",
                  "0f78:0772 issues INT 21h AX=3C00h (create/truncate)"),
    "0f78:07ea": ("Close", "borland",
                  "0f78:07ea issues INT 21h AH=3Eh (close) for handles above "
                  "4 and resets the mode to 0xd7b0"),
    "0f78:080f": ("rtl_file_check_open", "behavioural",
                  "0f78:080f rejects a FileRec whose mode is not 0xd7b3 with "
                  "error 0x67 (103, file not open)"),
    "0f78:081e": ("rtl_file_read", "behavioural",
                  "0f78:081e is INT 21h AH=3Fh of RecSize bytes, error 0x64 "
                  "(100, disk read error) on a short read.  Both `Read` on a "
                  "typed file and `BlockRead` reach a routine of this shape, "
                  "so it carries no standard-procedure name here"),
    "0f78:0825": ("rtl_file_write", "behavioural",
                  "0f78:0825 is INT 21h AH=40h of RecSize bytes, error 0x65 "
                  "(101, disk write error) on a short write; same "
                  "ambiguity as 0f78:081e"),
    "0f78:08bc": ("Seek", "borland",
                  "0f78:08bc multiplies the record number at [bp+6]/[bp+8] by "
                  "RecSize (`mul es:[di+4]` twice, at 0f78:08ca and "
                  "0f78:08d3) and issues INT 21h AX=4200h, seek from the start "
                  "-- the only standard procedure that positions a typed file "
                  "by record number"),
    "0f78:08ec": ("GetDir", "borland",
                  "0f78:08ec issues INT 21h AH=19h (current drive) when the "
                  "drive argument at [bp+0xc] is 0, then AH=47h"),
    "0f78:093d": ("ChDir", "borland",
                  "0f78:093d converts the path through 0f78:09a8, handles a "
                  "leading drive letter (`and al,0xdf` / `sub al,0x41` at "
                  "0f78:0953), and changes directory"),
    "0f78:097e": ("MkDir", "borland",
                  "0f78:0985 converts the path then calls 0f78:09c3 with "
                  "ah=0x39 -- INT 21h AH=39h, create directory"),
    "0f78:0993": ("RmDir", "borland",
                  "0f78:099a converts the path then calls 0f78:09c3 with "
                  "ah=0x3a -- INT 21h AH=3Ah, remove directory"),
    "0f78:09a8": ("rtl_str_to_asciiz", "behavioural",
                  "0f78:09a8 copies a Pascal string to a NUL-terminated stack "
                  "buffer, clamping the length to 0x7f"),
    "0f78:09c3": ("rtl_dos_path_call", "behavioural",
                  "0f78:09c9 `int 0x21` with ds:dx pointing at that buffer, "
                  "recording a failure in InOutRes"),
    "0f78:09d2": ("rtl_longint_mul", "behavioural",
                  "0f78:09d2 branches on the CPU class byte at 20ae:368c and "
                  "either does a 386 `imul ecx` (0f78:09eb) or a four-part "
                  "16-bit multiply"),
    "0f78:0a0f": ("rtl_longint_divmod", "behavioural",
                  "0f78:0a0f mirrors 0f78:09d2 for division: `idiv ecx` at "
                  "0f78:0a2c on a 386, a long-division loop otherwise, "
                  "returning quotient and remainder"),
    "0f78:0ae7": ("rtl_str_assign", "behavioural",
                  "0f78:0ae7 copies a length byte and that many bytes from "
                  "ss:[bx+4] to ss:[bx+8] with no truncation"),
    "0f78:0b01": ("rtl_str_assign_max", "behavioural",
                  "0f78:0b01 is 0f78:0ae7 with the length clamped to the "
                  "maximum at ss:[bx+4] (0f78:0b13 `cmp al,cl`)"),
    "0f78:0b25": ("Copy", "borland",
                  "0f78:0b25 takes (source, index, count, destination), "
                  "clamps a non-positive index to 1 (0f78:0b3a) and copies "
                  "count bytes from index -- `Copy(s, index, count)`.  "
                  "0f78:0c30 and 0f78:0c8f both build their result out of two "
                  "calls to it"),
    "0f78:0b66": ("rtl_str_append", "behavioural",
                  "0f78:0b66 adds the source length to the destination's, "
                  "saturating at 0xff (0f78:0b7e), and appends -- what `+` "
                  "and `Concat` are built from, so it is not either name"),
    "0f78:0b92": ("Pos", "borland",
                  "0f78:0b92 scans the string at [bp+6] for the pattern at "
                  "[bp+0xa] and returns the 1-based index or 0"),
    "0f78:0bd8": ("rtl_str_compare", "behavioural",
                  "0f78:0bf8 `repe cmpsb` over the shorter length then "
                  "0f78:0bfc `cmp al,ah` on the lengths, leaving flags for "
                  "the relational operators; it returns no value"),
    "0f78:0c03": ("rtl_char_to_str", "behavioural",
                  "0f78:0c03 writes the length byte 1 and one character"),
    "0f78:0c30": ("Insert", "borland",
                  "0f78:0c30 builds Copy(dest,1,index-1) + source + "
                  "Copy(dest,index,255) and assigns it back with the "
                  "destination's maximum length -- `Insert(source, dest, "
                  "index)`; entry +0xa0"),
    "0f78:0c8f": ("Delete", "borland",
                  "0f78:0c8f builds Copy(dest,1,index-1) + "
                  "Copy(dest,index+count,255) -- `Delete(dest, index, count)`; "
                  "entry +0xa8, immediately after Insert"),
    "0f78:0dea": ("rtl_real_neg_add", "behavioural",
                  "0f78:0dea flips the sign bit of the second operand "
                  "(`xor di,0x8000`) and falls into 0f78:0dee"),
    "0f78:0dee": ("rtl_real_add", "behavioural",
                  "0f78:0dee aligns two 6-byte reals by exponent and adds "
                  "their mantissas"),
    "0f78:0eb1": ("rtl_real_mul", "behavioural",
                  "0f78:0eb1 xors the sign bits, adds the exponents and "
                  "multiplies the mantissas of two 6-byte reals"),
    "0f78:0fad": ("rtl_real_zero", "behavioural",
                  "0f78:0fad clears ax/bx/dx -- the 6-byte real zero -- and is "
                  "the early-out 0f78:0fb4 jumps to"),
    "0f78:0fb4": ("rtl_real_div", "behavioural",
                  "0f78:0fb4 xors the sign bits, subtracts the exponents and "
                  "divides the mantissas of two 6-byte reals"),
    "0f78:102b": ("rtl_real_sign_cmp", "behavioural",
                  "0f78:102b compares two 6-byte reals and leaves the ordering "
                  "in the flags"),
    "0f78:1042": ("rtl_real_equal", "behavioural",
                  "0f78:1042 compares the exponent and all three mantissa "
                  "words of two 6-byte reals for equality"),
    "0f78:1055": ("rtl_real_from_longint", "behavioural",
                  "0f78:1055 normalises a signed 32-bit value in dx:ax into "
                  "the 6-byte real form"),
    "0f78:1091": ("rtl_real_to_longint", "behavioural",
                  "0f78:1091 denormalises a 6-byte real to a 32-bit integer, "
                  "shifting by 0xa0 minus the exponent (0f78:1092); ch "
                  "selects rounding, which is what 0f78:1131 sets"),
    "0f78:10ff": ("rtl_real_op_add", "behavioural",
                  "0f78:10ff is the far entry that calls 0f78:0dee and, on "
                  "carry, falls to 0f78:113f which raises error 205"),
    "0f78:1105": ("rtl_real_op_sub", "behavioural",
                  "0f78:1105 is the far entry that calls 0f78:0dea"),
    "0f78:1111": ("rtl_real_op_mul", "behavioural",
                  "0f78:1111 is the far entry that calls 0f78:0eb1"),
    "0f78:1117": ("rtl_real_op_div", "behavioural",
                  "0f78:1117 rejects a zero second operand (`or cl,cl` / `je` "
                  "to 0f78:1145, which raises error 200) then calls "
                  "0f78:0fb4; docs/re/gaps.md reached the same conclusion "
                  "independently while recovering character generation"),
    "0f78:1121": ("rtl_real_op_cmp", "behavioural",
                  "0f78:1121 is the far entry that calls 0f78:102b"),
    "0f78:1125": ("rtl_real_op_from_longint", "behavioural",
                  "0f78:1125 is the far entry that calls 0f78:1055"),
    "0f78:1131": ("rtl_real_op_to_longint", "behavioural",
                  "0f78:1131 sets ch=1, calls 0f78:1091 and raises error 207 "
                  "(0xcf, invalid floating point operation) on overflow -- the "
                  "worker behind Trunc and Round.  docs/re/gaps.md establishes "
                  "the ch=1 rounding path independently from data/rng_trace.json"),
    "0f78:114b": ("Random", "borland",
                  "0f78:114b steps RandSeed through 0f78:11a8 and scales the "
                  "result by the pushed range with a 32x16 widening multiply; "
                  "docs/re/rng.md establishes it from flow and "
                  "data/rng_trace.json replays it"),
    "0f78:11a8": ("rtl_rand_step", "behavioural",
                  "0f78:11a8 is the linear congruential step on RandSeed "
                  "(20ae:367e), multiplier 0x08088405 with the low half stored "
                  "at 0f78:11de -- docs/re/rng.md"),
    "0f78:11e0": ("Randomize", "borland",
                  "0f78:11e0 is INT 21h AH=2Ch storing cx:dx into RandSeed at "
                  "20ae:367e -- docs/re/rng.md"),
    "0f78:11ed": ("rtl_longint_to_digits", "behavioural",
                  "0f78:11ed emits the decimal digits of a signed 32-bit value "
                  "through 0f78:1209, handling the sign first"),
    "0f78:1209": ("rtl_digit_out", "behavioural",
                  "0f78:1209 divides the running value by si and emits one "
                  "digit, with the `add dl,7` correction above '9'"),
    "0f78:1229": ("rtl_parse_number", "behavioural",
                  "0f78:1229 parses an optional sign then digits from es:di, "
                  "returning the value and the position it stopped at"),
    "0f78:12d0": ("Str", "borland",
                  "0f78:12d0 converts the 32-bit value at [bp+0xe]/[bp+0x10] "
                  "with 0f78:11ed into a stack buffer and then into the "
                  "destination string, honouring the field width"),
    "0f78:131b": ("Val", "borland",
                  "0f78:131b skips leading spaces in the string at [bp+0xa] "
                  "and parses it with 0f78:1229, returning the error position; "
                  "entry +0x310 directly follows Str's +0x308"),
}

# `CRT` and `DOS` routines the library's symbol table does NOT name: internal
# helpers, and interiors Ghidra promoted to functions.  Same rules as above --
# every one rests on its own disassembly.
NAMES.update({
    "0f16:0000": ("rtl_crt_halt255", "behavioural",
                  "0f16:0000 is `mov ax,0xff` then a far call to the runtime "
                  "halt at 0f78:0116; CRT.TPU's first fixup is a call to "
                  "SYSTEM entry +0x018, which resolves to 0f78:0116"),
    "0f16:000d": ("Crt_initialization", "behavioural",
                  "0f16:000d is the block CRT.TPU's entry +0x0000 resolves to. "
                  "It probes the display through 0f16:003b, then runs "
                  "AssignCrt (0f16:033c) plus SYSTEM entry +0x230 (Reset) on "
                  "one TextRec and AssignCrt plus +0x238 (Rewrite) on the "
                  "other -- Input and Output"),
    "0f16:003b": ("rtl_crt_detect_display", "behavioural",
                  "0f16:003b reads the video mode with INT 10h AH=0Fh through "
                  "0f16:0614, forces mode 3 when it is above 7, and reads the "
                  "window size"),
    "0f16:00a3": ("rtl_crt_set_mode", "behavioural",
                  "0f16:00a3 clears bit 0 of 0040:0087 through the segment "
                  "cached at 20ae:3684 and issues INT 10h AH=00h with the mode "
                  "in al"),
    "0f16:00f0": ("rtl_crt_read_window_size", "behavioural",
                  "0f16:00f0 reads the mode (AH=0Fh) and the EGA/VGA font "
                  "geometry (AX=1130h) and derives the row count"),
    "0f16:014e": ("rtl_crt_flush_keyboard", "behavioural",
                  "0f16:014e drains the BIOS keyboard buffer with INT 16h "
                  "AH=01h/AH=00h while the flag at 20ae:3eca is set"),
    "0f16:02c8": ("rtl_crt_wait_retrace", "behavioural",
                  "0f16:02c8 spins on `cmp al,es:[di]` / `loope` -- the CGA "
                  "snow-avoidance wait.  This program's copy uses al where "
                  "this library's uses bl, one of the four routines it links "
                  "from a different build"),
    "0f16:039f": ("rtl_crt_read_line", "behavioural",
                  "0f16:039f is the CRT text device's input function: it loops "
                  "on ReadKey (0f16:031a) and handles BS, ^S, ^D, ESC and ^A, "
                  "filling the TextRec buffer at es:[di+0xc]"),
    "0f16:0482": ("rtl_crt_newline", "behavioural",
                  "0f16:0482 writes 0x0d then 0x0a through 0f16:0489"),
    "0f16:0489": ("rtl_crt_put_char", "behavioural",
                  "0f16:0489 special-cases BEL, BS, CR and LF and otherwise "
                  "writes one cell with INT 10h AH=09h, advancing the cursor "
                  "and scrolling through 0f16:04e2"),
    "0f16:04e2": ("rtl_crt_scroll_if_last_row", "behavioural",
                  "0f16:04e2 issues INT 10h AX=0601h over the window recorded "
                  "at 20ae:3ec0..3ec3 when the cursor is past the last row"),
    "0f16:0503": ("rtl_crt_get_cursor", "behavioural",
                  "0f16:0503 is INT 10h AH=03h with bh=0, through 0f16:0614"),
    "0f16:050a": ("rtl_crt_set_cursor", "behavioural",
                  "0f16:050a is INT 10h AH=02h with bh=0, through 0f16:0614"),
    "0f16:0614": ("rtl_crt_bios_video", "behavioural",
                  "0f16:0614 saves si/di/bp/es around a bare INT 10h -- the "
                  "wrapper every CRT BIOS call goes through"),
    "0ee5:0058": ("rtl_dos_findnext_interior", "behavioural",
                  "0ee5:0058 is DOS unit code offset 0x1c9, inside FindNext "
                  "(0x1af, entry +0x098) and before UnpackTime (0x1ed).  It "
                  "starts with a conditional jump, so it is a branch target "
                  "Ghidra promoted to a function, not a routine of its own"),
})


#: Segments whose code is NOT the Borland runtime.  `0eed` links against the
#: runtime (`lcall 0f78:02cd` in each prologue) but matches no unit of the
#: library, and its instruction encodings are the game's, not Borland's -- see
#: docs/re/rtl.md.  Recorded so the count of "runtime functions" stops
#: including them.
NON_RUNTIME_SEGMENTS = {0x0EED}


def tpl_symbol_names(unit, placements):
    """`segment offset -> (name, entry offset)` from a unit's symbol table.

    Only for units that export ordinary routines: `CRT` and `DOS` do,
    `SYSTEM` does not (`tools/tpl.py`).
    """
    ents = {e.off: e for e in unit.entries()}
    bmap = block_map(placements)
    out = {}
    for sym in unit.symbols():
        eo = sym.entry_off
        if eo is None or eo not in ents:
            continue
        e = ents[eo]
        if e.tag not in bmap:
            continue
        out[bmap[e.tag] + e.val] = (sym.name, eo)
    return out


# --- the export --------------------------------------------------------------

def rtl_functions():
    """The `data/functions.json` records outside the game's code segment."""
    funcs = json.loads(FUNCTIONS.read_text())
    out = []
    for f in funcs:
        seg = int(f["entry"].split(":")[0], 16)
        if seg < 0x1000:
            continue
        rel = seg - 0x1000
        if rel in SEGMENTS:
            out.append((rel, int(f["entry"].split(":")[1], 16), f))
    out.sort(key=lambda t: (t[0], t[1]))
    return out


def build(tpl_path=None):
    data = tpl.read_tpl(tpl_path)
    sha = hashlib.sha256(data).hexdigest()
    units = {u.name: u for u in tpl.units(data)}
    for u in units.values():
        tpl.check_unit(u)

    img = addr.load_image(addr.read_exe())

    # Which unit each runtime segment is, chosen by how much of the segment
    # the alignment accounts for -- not by the segment number.
    seg_unit, seg_place = {}, {}
    for seg, (base, length) in sorted(SEGMENTS.items()):
        block = img[base:base + length]
        best = None
        for name, u in units.items():
            placements, covered = align(block, u)
            if best is None or covered > best[2]:
                best = (name, placements, covered)
        name, placements, covered = best
        if covered == 0 or seg in NON_RUNTIME_SEGMENTS:
            seg_unit[seg], seg_place[seg] = None, []
        else:
            seg_unit[seg], seg_place[seg] = name, placements

    routines = []
    named = 0
    for seg, off, f in rtl_functions():
        cit = "%04x:%04x" % (seg, off)
        uname = seg_unit[seg]
        info = ({"unit": None, "mode": "not_runtime" if seg in NON_RUNTIME_SEGMENTS
                 else "unmatched"}
                if uname is None
                else classify(img, seg, seg_place[seg], units[uname], off,
                              f["size"]))
        rec = {
            "citation": cit,
            "ghidra": f["entry"],
            "ghidra_name": f["name"],
            "size": f["size"],
            "image_off": addr.image_off_of_seg_off(seg, off),
            "file_off": addr.file_off_of_citation(cit),
            "name": None,
            "name_kind": None,
            "tier": None,
            "evidence": None,
            "match": info,
        }
        if uname in ("CRT", "DOS", "OVERLAY", "PRINTER"):
            syms = tpl_symbol_names(units[uname], seg_place[seg])
            if off in syms:
                nm, eo = syms[off]
                rec.update(name=nm, name_kind="tpl_symbol", tier="flow",
                           evidence=("%s.TPU's symbol table names entry +%#06x "
                                     "%r, and that entry resolves to this "
                                     "address" % (uname, eo, nm)))
        if rec["name"] is None and cit in NAMES:
            nm, kind, ev = NAMES[cit]
            rec.update(name=nm, name_kind=kind, tier="flow", evidence=ev)
        named += rec["name"] is not None
        routines.append(rec)

    doc = {
        "note": ("Generated by tools/rtlmatch.py.  Do not hand-edit; regenerate "
                 "with `python3 tools/rtlmatch.py emit`.  `name_kind` "
                 "tpl_symbol = read verbatim from the library's symbol table, "
                 "borland = a Borland standard-procedure name established from "
                 "the routine's own flow, behavioural = a name coined here for "
                 "a routine with no user-visible Borland name."),
        "library": {
            "sha256": sha,
            "size": len(data),
            "units": [tpl.check_unit(u) for u in tpl.units(data)],
        },
        "segments": [
            {
                "segment": "%04x" % seg,
                "image_off": SEGMENTS[seg][0],
                "length": SEGMENTS[seg][1],
                "unit": seg_unit[seg],
                "blocks": seg_place[seg],
                "covered": (sum(p["matched_bytes"] + p["gap_before"]
                                for p in seg_place[seg])),
            }
            for seg in sorted(SEGMENTS)
        ],
        "counts": {"routines": len(routines), "named": named,
                   "unnamed": len(routines) - named},
        "routines": routines,
    }
    return doc


# --- commands ----------------------------------------------------------------

def cmd_emit(argv):
    doc = build(argv[0] if argv else None)
    OUT_PATH.write_text(json.dumps(doc, indent=1, sort_keys=False) + "\n")
    c = doc["counts"]
    print("wrote %s: %d routines, %d named, %d unnamed"
          % (OUT_PATH.relative_to(REPO), c["routines"], c["named"], c["unnamed"]))
    return 0


def cmd_units(argv):
    data = tpl.read_tpl(argv[0] if argv else None)
    print(json.dumps([tpl.check_unit(u) for u in tpl.units(data)], indent=2))
    return 0


def cmd_align(argv):
    doc = build(argv[0] if argv else None)
    for s in doc["segments"]:
        print("%s -> %s, %d/%d bytes in %d blocks"
              % (s["segment"], s["unit"], s["covered"], s["length"],
                 len(s["blocks"])))
        for b in s["blocks"]:
            print("   seg %04x  block %2d tag %#06x  %4d bytes  "
                  "diff %3d bytes in %2d runs, %d long"
                  % (b["seg_off"], b["block"], b["tag"], b["matched_bytes"],
                     b["diff_bytes"], b["diff_runs"], b["long_runs"]))
    return 0


def cmd_reject(argv):
    """Show the matcher REJECTING code it should not match.

    A matcher only ever run on the things it matches proves nothing about what
    it would refuse.  Two negatives here: the game's own code segment, which
    is not a runtime unit at all, and each aligned runtime segment against
    every unit it is NOT.
    """
    data = tpl.read_tpl(argv[0] if argv else None)
    units = list(tpl.units(data))
    img = addr.load_image(addr.read_exe())
    ok = True
    game = img[0:0x0EE50]
    for u in units:
        _, covered = align(game, u)
        print("game code segment 1000 vs %-8s: %d of %d bytes aligned"
              % (u.name, covered, len(game)))
        ok &= covered == 0
    for seg, (base, length) in sorted(SEGMENTS.items()):
        block = img[base:base + length]
        for u in units:
            _, covered = align(block, u)
            print("segment %04x vs %-8s: %d of %d bytes aligned"
                  % (seg, u.name, covered, length))
    print("VERDICT: the game's own code matches no unit" if ok
          else "VERDICT: FAILED -- game code aligned to a runtime unit")
    return 0 if ok else 1


def _main(argv):
    cmds = {"emit": cmd_emit, "units": cmd_units, "align": cmd_align,
            "reject": cmd_reject}
    if len(argv) < 2 or argv[1] not in cmds:
        print(__doc__)
        print("usage: %s {%s} [TURBO.TPL]" % (argv[0], "|".join(sorted(cmds))))
        return 2
    return cmds[argv[1]](argv[2:])


if __name__ == "__main__":
    raise SystemExit(_main(sys.argv))
