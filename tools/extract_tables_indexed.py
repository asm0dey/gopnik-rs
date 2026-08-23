#!/usr/bin/env python3
"""Recover indexed string array tables from g.exe (Task 4c).

Task 4b recovers strings the code addresses by a literal pointer. It
structurally cannot recover strings the code reaches by index arithmetic --
Pascal `array[..] of string[255]` elements, addressed at runtime as
`base + i * stride`. No literal offset for element `i` exists anywhere in the
binary for such tables, so pointer recovery (Task 4b) misses them entirely.

This walks each known table's fixed byte stride starting at its base offset,
reading one length-prefixed CP866 shortstring per slot, and stops the walk
the moment a slot is no longer a well-formed string. The entry count is never
hardcoded -- it falls out of the walk.

See docs/re/string-tables.md for how the two tables below were located and
verified.
"""
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE = ROOT / "orig" / "g.exe"
OUT = ROOT / "data" / "string_tables.json"

MIN_LEN = 1
MAX_LEN = 200

# (name, base file offset, byte stride between successive elements)
TABLES = [
    ("ranks", 0x123DE, 256),
    ("krutizna", 0x12EF2, 256),
]

MARKUP_RE = re.compile(r"\^[0-7]")


def strip_markup(s: str) -> str:
    """Remove the original's ^N colour directives, leaving displayable text."""
    return MARKUP_RE.sub("", s)


def is_payload_byte(b: int) -> bool:
    # Same content filter used for pointer-recovered strings (Task 4b):
    # printable ASCII, CP866 high-range, or the line-separator bytes the
    # original uses inside multi-line menu strings.
    return (0x20 <= b <= 0x7E) or (0x80 <= b <= 0xF1) or b in (0x07, 0x0A, 0x0D)


def is_well_formed(blob: bytes, off: int) -> bool:
    if off < 0 or off >= len(blob):
        return False
    n = blob[off]
    if not (MIN_LEN <= n <= MAX_LEN):
        return False
    if off + 1 + n > len(blob):
        return False
    payload = blob[off + 1 : off + 1 + n]
    return all(is_payload_byte(b) for b in payload)


def walk_table(blob: bytes, base: int, stride: int) -> list[dict]:
    entries = []
    i = 0
    while True:
        off = base + i * stride
        if not is_well_formed(blob, off):
            break
        n = blob[off]
        payload = blob[off + 1 : off + 1 + n]
        text = payload.decode("cp866")
        plain = strip_markup(text)
        entries.append(
            {
                "index": i,
                "off": off,
                "text": text,
                "plain": plain,
            }
        )
        i += 1
    return entries


def main() -> None:
    blob = EXE.read_bytes()
    tables = []
    for name, base, stride in TABLES:
        entries = walk_table(blob, base, stride)
        tables.append(
            {
                "name": name,
                "base": base,
                "stride": stride,
                "entries": entries,
            }
        )

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(
        json.dumps({"tables": tables}, ensure_ascii=False, indent=1) + "\n",
        encoding="utf-8",
    )
    total = sum(len(t["entries"]) for t in tables)
    print(f"wrote {total} table entries across {len(tables)} tables to {OUT}")


if __name__ == "__main__":
    main()
