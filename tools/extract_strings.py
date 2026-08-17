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

See docs/re/strings.md for the merge and the before/after comparison against
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
    payload = blob[off + 1 : off + 1 + n]
    text = payload.decode("cp866")
    plain = strip_markup(text)
    return {"off": off, "text": text, "plain": plain, "suspect": is_suspect(plain)}


def extract(blob: bytes) -> tuple[list[dict], int]:
    """Build the merged entry list: every recovered pointer, plus every
    indexed-table entry whose offset a pointer didn't already cover.

    Returns (entries sorted by offset, count of entries merged in from the
    tables).
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

    return [by_off[off] for off in sorted(by_off)], merged


def main() -> None:
    blob = EXE.read_bytes()
    items, merged = extract(blob)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(
        json.dumps(items, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )
    print(
        f"wrote {len(items)} strings to {OUT} "
        f"({len(items) - merged} pointer-anchored, {merged} merged from string_tables.json)"
    )


if __name__ == "__main__":
    main()
