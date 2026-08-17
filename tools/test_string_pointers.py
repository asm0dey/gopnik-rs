#!/usr/bin/env python3
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_pointers():
    data = json.loads((ROOT / "data" / "string_pointers.json").read_text(encoding="utf-8"))
    ptrs = data["pointers"]

    assert ptrs == sorted(ptrs), "pointers must be sorted"
    assert len(set(ptrs)) == len(ptrs), "pointers must be unique"
    assert len(ptrs) >= 400, f"expected >=400 string pointers, got {len(ptrs)}"

    blob = (ROOT / "orig" / "g.exe").read_bytes()

    # Every pointer must land on a well-formed length-prefixed string.
    for off in ptrs:
        n = blob[off]
        assert 3 <= n <= 250, f"{off:#x}: implausible length {n}"
        assert off + 1 + n <= len(blob), f"{off:#x}: payload runs past EOF"

    # The known-truncated case from Task 2 must now resolve completely.
    assert 0xBCDD in ptrs, "0xBCDD (the боксёров line) not recovered"
    n = blob[0xBCDD]
    text = blob[0xBCDE : 0xBCDE + n].decode("cp866")
    assert text.endswith("челюсть)"), f"still truncated: {text!r}"

    # Coverage must not regress against the blind scan. Every non-suspect
    # entry the old scanner found must either appear as a pointer or fall
    # inside some pointer's payload span (i.e. be superseded by a correctly
    # framed, longer string). Anything else is real game text we lost.
    old = json.loads((ROOT / "data" / "strings.json").read_text(encoding="utf-8"))
    ptr_set = set(ptrs)
    missing = []
    for entry in old:
        if entry["suspect"]:
            continue
        off = entry["off"]
        if off in ptr_set:
            continue
        if any(q <= off < q + 1 + blob[q] for q in ptrs):
            continue
        missing.append(entry)
    assert len(missing) <= 10, (
        f"{len(missing)} real strings lost vs the blind scan, e.g. "
        f"{[(hex(m['off']), m['plain'][:40]) for m in missing[:5]]}"
    )

    print(f"OK {len(ptrs)} string pointers recovered, {len(missing)} blind-scan entries unaccounted for")


if __name__ == "__main__":
    test_pointers()
