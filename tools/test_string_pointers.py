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

    # The consecutive run at 0x18D0-0x18DA is the signature of naive byte
    # scanning. Real instruction operands do not produce it.
    run = [o for o in ptrs if 0x18D0 <= o <= 0x18DA]
    assert len(run) <= 2, f"byte-scan false positives leaked in: {[hex(o) for o in run]}"

    print(f"OK {len(ptrs)} string pointers recovered and validated")


if __name__ == "__main__":
    test_pointers()
