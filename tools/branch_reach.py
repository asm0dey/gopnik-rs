#!/usr/bin/env python3
"""How many of the game's branches have a guard a `.SAV` file can set?

    python3 tools/branch_reach.py            # the number, and how it was got
    python3 tools/branch_reach.py --json     # the same, machine-readable

**Why this is a script and not a sentence.** The figure justifies
`tools/savegen.py` existing at all, and it was first published as a bare
number with no command behind it. `docs/superpowers/RESUME.md` has already
had to correct `134/838` and `157/838` for exactly that reason. So the
method lives here, in code, and whatever this prints is the number.

## Method, in one sentence

For each `class == "game"` branch in `data/branches.json`, take its guard's
disassembly text, extract every ABSOLUTE data displacement -- a bracketed
operand that is nothing but a hex literal, which excludes `[BP + 0x4]` and
friends -- and count the branch if any of them lands inside the 694-byte
character record `[RECORD_BASE, RECORD_BASE + SIZE)`.

## What the window is, and the mistake it is easy to make

The record is `20ae:369c`..`20ae:3951` -- `RECORD_BASE` = `0x369c` through
`RECORD_BASE + 0x2b6`. It is **not** `0x389c`: that is the record base plus
`0x200`, the offset of the eight stat words *inside* the record
(`docs/re/save-format.md`). Using `0x389c` as the base shifts the whole
window up by `0x200` and counts **26 branches the record cannot reach**,
because `[0x3952, 0x3b52)` is mostly the ENEMY's record (`DS:3952`,
`docs/re/combat.md`) plus the wander bucket byte `20ae:3971` -- none of
which is in a `.SAV` file. It also misses two that ARE reachable, the
empty-name tests at `1000:7225` and `1000:ed64` (`cmp byte [0x379c],0`, the
name shortstring's length byte at `.SAV 0x100`).

That shifted window is where a `355 / 838` figure came from. The number is
`331`; `--window stat-block-base` reproduces the wrong one on demand so the
discrepancy stays checkable rather than remembered.

## What the number does and does not mean

It counts branches whose guard **reads a byte a save file writes**. It does
not claim the guard is satisfiable for every value, that reaching the branch
needs nothing else, or that a synthesised state is one a player could reach
(`docs/superpowers/RESUME.md`, "Reaching states without grinding"). It is an
upper bound on reach-by-save, and the honest reading is "this is the lever's
size", not "42% of the game is covered".

Limits worth stating, both in the direction of UNDER-counting:

* it reads only the guard's own text, so a branch whose condition was
  computed several instructions earlier from a record byte is not counted;
* a store or read through a pointer register carries no displacement and
  cannot be seen at all -- the same limit `docs/re/wander.md`'s `[0x389c]`
  scan states about itself.

Standard library only.
"""
import argparse
import collections
import json
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from decode_save import RECORD_BASE, SIZE                            # noqa: E402

REPO = pathlib.Path(__file__).resolve().parent.parent

#: A bracketed operand that is NOTHING but a hex literal. The `^...$` anchors
#: inside the brackets are what keep `[BP + 0x4]`, `[BP + -0x2]` and
#: `[BP + DI + 0x6]` out: those are stack and register addressing, not a
#: DGROUP displacement. 484 of the 551 bracketed operands in the game
#: branches' guards are of this form; the other 67 are the register ones.
ABS_OPERAND = re.compile(r"\[(?:0x)?([0-9a-fA-F]{3,4})\]")

#: The two windows, so the wrong one is reproducible rather than described.
WINDOWS = {
    # The record: 20ae:369c .. 20ae:3951. This is the right one.
    "record": (RECORD_BASE, RECORD_BASE + SIZE),
    # The record base misread as the STAT-BLOCK base, i.e. shifted up by the
    # 0x200 the stat words sit at inside the record. Kept only so the
    # 355/838 figure can be reproduced and seen to be wrong.
    "stat-block-base": (RECORD_BASE + 0x200, RECORD_BASE + 0x200 + SIZE),
}


def branches(path=None):
    path = pathlib.Path(path or REPO / "data" / "branches.json")
    return [b for b in json.loads(path.read_text())["branches"]
            if b["class"] == "game"]


def guard_displacements(branch):
    """Every absolute data displacement the branch's guard names."""
    guard = branch.get("guard")
    if not guard:
        return []
    return [int(x, 16) for x in ABS_OPERAND.findall(guard.get("text", ""))]


def reachable(bs, window="record"):
    """The branches whose guard reads a byte inside `window`."""
    lo, hi = WINDOWS[window]
    return [b for b in bs
            if any(lo <= v < hi for v in guard_displacements(b))]


def report(path=None, window="record"):
    bs = branches(path)
    hits = reachable(bs, window)
    per = collections.Counter(b["func"] for b in hits)
    total = collections.Counter(b["func"] for b in bs)
    lo, hi = WINDOWS[window]
    return {
        "window": window,
        "window_lo": "20ae:%04x" % lo,
        "window_hi_exclusive": "20ae:%04x" % hi,
        "reachable": len(hits),
        "game_branches": len(bs),
        "percent": round(100.0 * len(hits) / len(bs), 1),
        "by_function": [
            {"func": f, "reachable": n, "branches": total[f]}
            for f, n in sorted(per.items(), key=lambda kv: -kv[1])
        ],
    }


def main(argv=None):
    ap = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--branches", default=None)
    ap.add_argument("--window", default="record", choices=sorted(WINDOWS))
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args(argv)

    r = report(args.branches, args.window)
    if args.json:
        print(json.dumps(r, indent=1))
        return 0
    print("%d of %d game branches (%.1f%%) have a guard that reads a byte in "
          "the %s window %s..%s"
          % (r["reachable"], r["game_branches"], r["percent"], r["window"],
             r["window_lo"], r["window_hi_exclusive"]))
    for row in r["by_function"]:
        if row["reachable"] >= 5:
            print("  %-16s %4d of %4d" % (row["func"], row["reachable"],
                                          row["branches"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
