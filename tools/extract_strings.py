#!/usr/bin/env python3
"""Extract Borland Pascal shortstrings (length-prefixed, CP866) from g.exe,
anchored on offsets recovered from real code rather than guessed by a blind
forward scan for length-prefix bytes.

Task 2's blind scan misframed strings whenever a *content* byte happened to
look like a plausible length prefix -- e.g. offset 0xBCF8 is an ordinary
space (0x20) inside the боксёров line, misread as a length of 32, which cut
that string short at "...сломают челюст". No threshold tweak fixes an
ambiguity like that; the fix is to stop guessing where strings start.

This module instead reads one shortstring at each offset in two places that
were independently verified, not guessed:

- `data/string_pointers.json` (Task 4b): offsets recovered from real 16-bit
  immediate operands in the disassembly -- places the compiled code actually
  loads as a string address.
- `data/string_tables.json` (Task 4c): the `ranks`/`krutizna` indexed
  `array[..] of string[255]` tables, reached only by `base + i*256` index
  arithmetic and so invisible to pointer recovery; recovered separately by
  walking each table's fixed stride.

A third pass (Task 2c) then walks the gaps between adjacent recovered
offsets and accepts a gap's bytes only when they tile *exactly* as a chain
of complete shortstrings between two non-suspect anchors -- never a blind
forward scan, just verification that the space between two independently
confirmed offsets is itself fully accounted for. See docs/re/strings.md for
the merge, the gap-tiling rule, and the before/after comparison against
Task 2's blind scan.
"""
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE = ROOT / "orig" / "g.exe"
POINTERS = ROOT / "data" / "string_pointers.json"
TABLES = ROOT / "data" / "string_tables.json"
OUT = ROOT / "data" / "strings.json"

MARKUP_RE = re.compile(r"\^[0-7]")
CYRILLIC_RE = re.compile(r"[А-Яа-яЁё]")


def strip_markup(s: str) -> str:
    """Remove the original's ^N colour directives, leaving displayable text."""
    return MARKUP_RE.sub("", s)


def longest_cyrillic_run(s: str) -> int:
    best = cur = 0
    for ch in s:
        if CYRILLIC_RE.match(ch):
            cur += 1
            best = max(best, cur)
        else:
            cur = 0
    return best


def is_suspect(plain: str) -> bool:
    """Heuristic flag for entries that are probably machine code, not text.

    Real game text either contains a space or has a run of three or more
    consecutive Cyrillic letters. Byte sequences that merely satisfy the
    length-prefix scan tend to alternate letters with digits and symbols.
    Flagged entries are kept, never deleted -- see the plan for why. A
    pointer or table offset can in principle still land on non-text, so this
    guard stays even though the offsets themselves are no longer guessed.
    """
    return longest_cyrillic_run(plain) < 3 and " " not in plain


def read_string(blob: bytes, off: int) -> dict:
    """Read one length-prefixed CP866 shortstring at a known-good offset."""
    n = blob[off]
    if off + 1 + n > len(blob):
        raise ValueError(
            f"{off:#x}: payload of length {n} runs past EOF ({len(blob)} bytes)"
        )
    payload = blob[off + 1 : off + 1 + n]
    text = payload.decode("cp866")
    plain = strip_markup(text)
    return {"off": off, "text": text, "plain": plain, "suspect": is_suspect(plain)}


def gap_tile(blob: bytes, by_off: dict) -> int:
    """Recover short strings stranded in the gap between two adjacent,
    already-verified anchors (Task 2c).

    Walks consecutive pairs of currently-recovered offsets `(a, b)`. A
    `suspect` entry is not a known-good anchor, so a gap next to one proves
    nothing about the bytes between them -- skipped. Gaps >= 40 bytes are
    inter-region spans, not stranded strings -- skipped. No filter is applied
    to what the gap's bytes look like; see below for why not. Otherwise the
    gap (from `a`'s payload end up to `b`) is walked as a chain of Pascal
    shortstrings: read a length byte, skip that many payload bytes, repeat.
    Only if the chain lands *exactly* on `b` is every element in it accepted
    as real; if it overruns `b`, nothing is emitted for that gap.

    An earlier revision of this function also required the gap to contain a
    byte in a Cyrillic/digit/ASCII-letter class ("is_letter_byte"), on the
    theory that tiling alone was weak evidence -- a sample of 20000 random
    windows across offsets 0x18D0-0x158F2 tiled at ~13% regardless of gap
    length, so "it tiles" looked like a coin flip. That sample was a
    sampling artifact: it silently included the 0x11000+ tail, which is
    69.0% NUL bytes, and a run of 0x00 is a chain of zero-length strings
    that tiles at *any* length -- that's what made the rate flat across gap
    lengths. Measured per region instead (20000 random windows each):

        region                          NUL    2B    3B    7B   20B   40B
        0x18D0-0x11000 (recovered strings live here)
                                        2.1%  1.7%  0.6%  1.1%  0.1%  0.2%
        0x11000-0x158F2 (tail, mostly NUL padding)
                                       69.0% 67.2% 67.5% 66.4% 64.4% 64.7%
        union (the misleading original sample)
                                       17.4% 17.1% 15.8% 15.7% 14.8% 14.1%

    In the region that actually holds the recovered strings, exact tiling is
    strong evidence (~0.1-1.7%), not a coin flip -- for a 2-byte gap the rate
    is just P(byte == 0x01) = 1.64%. It is equally strong for a length-1
    literal as for a longer one: `write(' ')` emits exactly a length-1
    shortstring, so single-punctuation entries are expected content. The
    byte-content filter was therefore removed; what justifies an entry is
    that the gap lies between two independently verified, non-suspect
    anchors and tiles exactly -- nothing about the bytes themselves needs to
    look like a letter.

    Never relax this into a scan -- an unanchored forward scan is the
    original framing defect this whole sequence of tasks exists to fix.

    Mutates `by_off` in place with any newly recovered entries and returns
    how many were added.
    """
    offs = sorted(by_off)
    added = 0
    for a, b in zip(offs, offs[1:]):
        if by_off[a]["suspect"] or by_off[b]["suspect"]:
            continue
        end = a + 1 + blob[a]
        if end >= b or b - end >= 40:
            continue
        chain = []
        cursor = end
        while cursor < b:
            n = blob[cursor]
            nxt = cursor + 1 + n
            if nxt > b:
                chain = None
                break
            chain.append(cursor)
            cursor = nxt
        if chain is not None and cursor == b:
            for off in chain:
                by_off[off] = read_string(blob, off)
                added += 1
    return added


def extract(blob: bytes) -> tuple[list[dict], int, int]:
    """Build the merged entry list: every recovered pointer, every indexed-
    table entry whose offset a pointer didn't already cover, plus every
    string recovered by tiling the gaps between those anchors.

    Returns (entries sorted by offset, count of entries merged in from the
    tables, count of entries recovered by gap tiling).
    """
    pointers = json.loads(POINTERS.read_text(encoding="utf-8"))["pointers"]
    by_off = {off: read_string(blob, off) for off in pointers}

    tables = json.loads(TABLES.read_text(encoding="utf-8"))["tables"]
    merged = 0
    for table in tables:
        for entry in table["entries"]:
            off = entry["off"]
            if off in by_off:
                continue
            plain = entry["plain"]
            by_off[off] = {
                "off": off,
                "text": entry["text"],
                "plain": plain,
                "suspect": is_suspect(plain),
            }
            merged += 1

    tiled = gap_tile(blob, by_off)

    return [by_off[off] for off in sorted(by_off)], merged, tiled


def main() -> None:
    blob = EXE.read_bytes()
    items, merged, tiled = extract(blob)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(
        json.dumps(items, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )
    print(
        f"wrote {len(items)} strings to {OUT} "
        f"({len(items) - merged - tiled} pointer-anchored, "
        f"{merged} merged from string_tables.json, "
        f"{tiled} recovered by gap tiling)"
    )


if __name__ == "__main__":
    main()
