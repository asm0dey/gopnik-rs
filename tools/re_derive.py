#!/usr/bin/env python3
"""Shared re-derivation helpers for the `docs/re/` claim tests.

`tools/test_character_sheet.py` grew these first; `tools/test_combat_dispatch.py`
needs the same four, and each of them encodes a mistake this project has already
made once:

  * `near_calls_to` matches modulo 64 KiB, because two of the character sheet's
    four call sites encode a `rel16` that wraps and a scan comparing the
    un-wrapped sum finds neither;
  * `far_calls_to` ignores the segment word, because filtering on a guessed
    segment made a "nothing far-calls this" claim narrower than it read;
  * `aligned_boundaries` decodes EVERY segment-1000 function from its own
    entry, so a citation into another function is not reported as unaligned for
    the wrong reason;
  * `strip_fences` removes double-backtick spans as well as fences, because
    either one desynchronises the single-backtick pairing and silently drops
    every span after it.

Standard library only, and it reads `orig/g.exe` through `tools/addr.py` so the
address arithmetic still lives in exactly one place.
"""
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import addr as addrmod            # noqa: E402
import dis16                      # noqa: E402

__all__ = ["CITE", "load_image", "near_calls_to", "far_calls_to",
           "aligned_boundaries", "strip_fences", "inline_spans"]

#: Every `1000:xxxx` citation, the only segment these documents cite by address.
CITE = re.compile(r"\b1000:[0-9a-f]{4}\b")


def load_image():
    return addrmod.load_image(addrmod.read_exe())


def near_calls_to(img, target_off):
    """Every `e8 rel16` in `img` whose 16-bit-wrapped target is `target_off`.

    The wrap is the point: two of the character sheet's four call sites live at
    `1000:ec89` and `1000:ee36`, where `off + 3 + disp` is `0x11a03`.  A scan
    that compares the un-wrapped sum finds neither, which is how
    `docs/superpowers/RESUME.md` named the wrong pair of callers for a whole
    session.
    """
    want = target_off & 0xFFFF
    out = []
    for off in range(len(img) - 3):
        if img[off] != 0xE8:
            continue
        disp = int.from_bytes(img[off + 1:off + 3], "little", signed=True)
        if (off + 3 + disp) & 0xFFFF == want:
            out.append("1000:%04x" % off)
    return out


def far_calls_to(img, target_off):
    """Every `9a <off16> <seg16>` in `img` whose offset word is `target_off`.

    ANY segment word counts.  An earlier revision compared the segment against
    `target_off // 16`, which is not a segment value under either convention in
    `docs/re/METHODOLOGY.md` -- it only ever matched through the `0`
    alternative beside it, so the scan was narrower than the claim it backed.
    Ignoring the segment entirely is both simpler and strictly stronger: the
    claim is "nothing far-calls this offset", and a hit under any segment word
    is worth surfacing rather than filtering out.
    """
    want = (target_off & 0xFFFF).to_bytes(2, "little")
    return ["1000:%04x" % off for off in range(len(img) - 4)
            if img[off] == 0x9A and img[off + 1:off + 3] == want]


def aligned_boundaries(img, branches):
    """`{"1000:xxxx": Insn}` for every instruction an ALIGNED walk reaches.

    Every segment-`1000` function in `data/branches.json` is decoded from its
    own entry, not just the handful a given document happens to cite, because
    these documents cite addresses in `entry`, `FUN_1000_3d11`, `FUN_1000_1a03`
    and `FUN_1000_074b` alike and a citation that lands outside the decoded set
    would otherwise be reported as unaligned for the wrong reason.
    """
    out = {}
    for f in branches["functions"]:
        if f["seg"] != "1000":
            continue
        start = addrmod.image_off_of_citation(f["entry"])
        for ins in dis16.decode_run(img, start, start + f["size"]):
            out["1000:%04x" % ins.off] = ins
    return out


def strip_fences(md):
    """The prose with ``` blocks and ``double-backtick`` spans removed.

    Fenced blocks hold pasted disassembly with its own backticks; leaving them
    in desynchronises the inline-code scan (a run of three backticks pairs
    wrongly) and every span after the first fence is then read at an offset.
    Found the hard way: the first version of the prose scan matched ZERO
    `addr text` spans in a file that has twenty-six of them.

    ``...`` -- markdown's way of writing a span that itself contains a
    backtick -- desynchronises the single-backtick pairing exactly as a fence
    does, so it goes too.  Both removals are why the callers assert a MINIMUM
    number of spans: a desync silently drops every span after it, and a check
    that quietly measures nothing is the defect these files exist to prevent.
    """
    md = re.sub(r"^```.*?^```", "", md, flags=re.S | re.M)
    return re.sub(r"``.*?``", "", md, flags=re.S)


def inline_spans(md):
    return [" ".join(x.split()) for x in re.findall(r"`([^`]+)`", md, re.S)]
