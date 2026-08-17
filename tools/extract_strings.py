#!/usr/bin/env python3
"""Extract Borland Pascal shortstrings (length-prefixed, CP866) from g.exe.

A shortstring is one length byte N followed by exactly N payload bytes.
We accept a candidate only when every payload byte is printable in CP866
and at least two of them are Cyrillic, which is what separates real game
text from machine code that happens to look string-shaped.
"""
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE = ROOT / "orig" / "g.exe"
OUT = ROOT / "data" / "strings.json"

MIN_LEN = 3
MAX_LEN = 200
MIN_CYRILLIC = 2


def is_cyrillic(b: int) -> bool:
    # CP866: 0x80-0xAF is А-п, 0xE0-0xF1 is р-я plus Ё/ё.
    return 0x80 <= b <= 0xAF or 0xE0 <= b <= 0xF1


def is_printable(b: int) -> bool:
    return 32 <= b < 127 or is_cyrillic(b) or b == 0xB0


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
    Flagged entries are kept, never deleted -- see the plan for why.
    """
    return longest_cyrillic_run(plain) < 3 and " " not in plain


def extract(blob: bytes) -> list[dict]:
    out = []
    i = 0
    end = len(blob)
    while i < end:
        n = blob[i]
        if MIN_LEN <= n <= MAX_LEN and i + 1 + n <= end:
            payload = blob[i + 1 : i + 1 + n]
            if all(is_printable(c) for c in payload) and sum(
                is_cyrillic(c) for c in payload
            ) >= MIN_CYRILLIC:
                text = payload.decode("cp866")
                plain = strip_markup(text)
                out.append(
                    {
                        "off": i,
                        "text": text,
                        "plain": plain,
                        "suspect": is_suspect(plain),
                    }
                )
                i += 1 + n
                continue
        i += 1
    return out


def main() -> None:
    items = extract(EXE.read_bytes())
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(
        json.dumps(items, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )
    print(f"wrote {len(items)} strings to {OUT}")


if __name__ == "__main__":
    main()
