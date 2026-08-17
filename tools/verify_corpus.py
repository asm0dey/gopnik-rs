#!/usr/bin/env python3
"""Fail loudly if the reference corpus is missing or altered."""
import hashlib
import pathlib
import sys

ORIG = pathlib.Path(__file__).resolve().parent.parent / "orig"

EXPECTED = {
    "g.exe": ("10eb0af07a2d2f5e9da790df7058891c", 88656),
    "PLACES.SAV": (None, 7),
    "SAVE_R0.SAV": (None, 694),
    "SAVE_R2.SAV": (None, 694),
    "SAVE_R3.SAV": (None, 694),
    "SAVE_R4.SAV": (None, 694),
    "SAVE_R5.SAV": (None, 694),
}


def main() -> int:
    failures = []
    for name, (want_md5, want_size) in EXPECTED.items():
        path = ORIG / name
        if not path.exists():
            failures.append(f"{name}: MISSING")
            continue
        blob = path.read_bytes()
        if len(blob) != want_size:
            failures.append(f"{name}: size {len(blob)} != {want_size}")
        if want_md5 is not None:
            got = hashlib.md5(blob).hexdigest()
            if got != want_md5:
                failures.append(f"{name}: md5 {got} != {want_md5}")
    for line in failures:
        print("FAIL", line)
    if failures:
        return 1
    print(f"OK {len(EXPECTED)} corpus files verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
