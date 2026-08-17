#!/usr/bin/env python3
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_extraction():
    subprocess.run([sys.executable, str(ROOT / "tools" / "extract_strings.py")], check=True)
    items = json.loads((ROOT / "data" / "strings.json").read_text(encoding="utf-8"))

    # A coarse regression floor, not an equality: a silent bulk loss inside
    # a suspect-bracketed region (where the tiling check below stays quiet)
    # would otherwise pass unnoticed. Do not tighten this to the exact
    # current count -- it exists to catch a big drop, not to pin a number.
    assert len(items) >= 780, f"expected >=780 strings, got {len(items)}"

    offs = [i["off"] for i in items]
    assert offs == sorted(offs), "strings must be sorted by offset"
    assert len(set(offs)) == len(offs), "offsets must be unique"

    text_by_off = {i["off"]: i["text"] for i in items}
    assert text_by_off[0x2B44] == "Не в этой жизни."
    assert text_by_off[0x2FB2] == '^1Крестик(Удача +2) '
    assert text_by_off[0x3173] == "^1Тесак(Урон+9) "
    assert text_by_off[0x4548] == "^4Пацан ты из какого района?"

    joined = "\n".join(i["text"] for i in items)
    assert "Кольцо \"Гп\"(Самолечение)" in joined
    assert "Костюм Adidas(+2)" in joined

    for i in items:
        assert "\x00" not in i["text"]

    # ^N is markup, not content: it must survive in `text` and be absent
    # from `plain`, and stripping must not disturb anything else.
    plain = {i["off"]: i["plain"] for i in items}
    assert plain[0x2B44] == "Не в этой жизни.", "plain equals text when no markup"
    assert plain[0x3173] == "Тесак(Урон+9) "
    assert plain[0x4548] == "Пацан ты из какого района?"

    markup = re.compile(r"\^[0-7]")
    for i in items:
        assert not markup.search(i["plain"]), (
            f"markup survived stripping at {i['off']:#x}: {i['plain']!r}"
        )
    assert any(markup.search(i["text"]) for i in items), (
        "no markup found in any raw text -- the extractor or the test is wrong"
    )

    # Anchored extraction must fix the framing bugs the blind scan produced.
    by_off = {i["off"]: i for i in items}

    assert 0xBCDD in by_off, "the боксёров line was not extracted"
    assert by_off[0xBCDD]["plain"].endswith("челюсть)"), (
        f"still truncated: {by_off[0xBCDD]['plain']!r}"
    )

    # Framing is checked structurally, by tiling. The string region is packed
    # with no delimiter, so a truncated string strands its tail in the gap
    # before the next string's start, and an over-long one runs into it.
    blob = (ROOT / "orig" / "g.exe").read_bytes()

    # The 40-byte gap threshold below is intentionally duplicated from
    # tools/extract_strings.py's gap_tile() (same inter-region-span cutoff:
    # a gap >= 40 bytes is treated as padding between string regions, not
    # stranded text, on both sides). `alnum()` defines the byte classes this
    # check treats as "stranded letter text" for its own diagnostic purpose
    # -- gap_tile() no longer filters by byte content itself, but this test
    # still needs its own definition of what a leftover fragment of real
    # text would look like. Drift in either copy silently changes what this
    # check can detect: a narrower `40` here than gap_tile()'s would flag
    # gaps the extractor correctly treats as inter-region spans, and a wider
    # one would let some already-recovered gaps go unchecked; a narrower or
    # wider `alnum()` would miss or falsely flag stranded bytes. Keep both
    # numbers in step with tools/extract_strings.py by hand.
    def alnum(c):
        return (0x80 <= c <= 0xAF or 0xE0 <= c <= 0xF1
                or 48 <= c <= 57 or 65 <= c <= 90 or 97 <= c <= 122)

    offs = sorted(by_off)
    for a, b in zip(offs, offs[1:]):
        end = a + 1 + blob[a]
        assert end <= b, f"0x{a:X} (len {blob[a]}) overlaps next string 0x{b:X}"
        if by_off[a]["suspect"] or by_off[b]["suspect"]:
            continue
        if b - end < 40:
            tail = blob[end:b]
            assert not any(alnum(c) for c in tail), (
                f"letter bytes stranded after 0x{a:X}: {tail!r}"
            )

    # The gap-tiled command tokens (Task 2c) -- the game's single-character
    # command parser tokens that the pointer scan's N>=3 Cyrillic-run floor
    # skipped.
    assert by_off[0x4E71]["plain"] == "sv", "the sv command token is missing"
    assert by_off[0x4E6F]["plain"] == "s"
    assert by_off[0x3D87]["plain"] == "1"
    assert by_off[0x23A4]["plain"] == "С^"

    suspects = [i for i in items if i["suspect"]]
    print(f"OK {len(items)} strings extracted, {len(suspects)} flagged suspect")


if __name__ == "__main__":
    test_extraction()
