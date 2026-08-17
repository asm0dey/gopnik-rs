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

    # Exact golden counts, deliberately not a floor.
    #
    # The gap check further down cannot catch a lost string: gap_tile() fills
    # exactly the non-suspect sub-40-byte gaps that check inspects, so on the
    # committed data 0 pairs ever reach its assertion. Worse, gap tiling
    # *repairs* the damage -- drop a real offset from string_pointers.json and
    # tiling silently re-emits the same string, so a coarse floor sees almost
    # nothing: dropping 20 real pointers still lands at 781-791 entries.
    #
    # What the loss does move is the split between the two sources: each
    # dropped pointer turns into one or more gap-tiled entries. Measured over
    # random drops of 1/3/10/20 pointers, `tiled` rose to 48/48-50/52-56/52-62
    # while `len(items)` barely moved. So pinning both numbers catches what
    # neither catches alone.
    #
    # Extraction is deterministic from a fixed binary plus two committed
    # artifacts, so these are invariants, not tuning. If a later task
    # legitimately changes them, re-measure and update these numbers, the
    # counts in docs/re/strings.md, and the plan together.
    assert len(items) == 796, f"expected 796 strings, got {len(items)}"

    pointers = json.loads(
        (ROOT / "data" / "string_pointers.json").read_text(encoding="utf-8")
    )["pointers"]
    assert len(pointers) == 695, (
        f"string_pointers.json holds {len(pointers)} offsets, expected 695 -- "
        "gap tiling would mask the loss by re-emitting the missing strings"
    )

    tiled = [i for i in items if i["off"] not in set(pointers)]
    table_offsets = {
        e["off"]
        for t in json.loads(
            (ROOT / "data" / "string_tables.json").read_text(encoding="utf-8")
        )["tables"]
        for e in t["entries"]
    }
    tiled = [i for i in tiled if i["off"] not in table_offsets]
    assert len(tiled) == 47, (
        f"{len(tiled)} entries came from gap tiling, expected 47 -- a rise "
        "means an anchor was lost and tiling silently covered for it"
    )

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
