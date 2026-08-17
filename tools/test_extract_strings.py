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

    offs = [i["off"] for i in items]
    assert offs == sorted(offs), "strings must be sorted by offset"
    assert len(set(offs)) == len(offs), "offsets must be unique"

    by_off = {i["off"]: i["text"] for i in items}
    assert by_off[0x2B44] == "Не в этой жизни."
    assert by_off[0x2FB2] == '^1Крестик(Удача +2) '
    assert by_off[0x3173] == "^1Тесак(Урон+9) "
    assert by_off[0x4548] == "^4Пацан ты из какого района?"

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

    # No entry may end cut off mid-word: if the payload's last byte and the
    # byte immediately after it are both alphanumeric, the string was cut.
    blob = (ROOT / "orig" / "g.exe").read_bytes()

    def alnum(c):
        return (0x80 <= c <= 0xAF or 0xE0 <= c <= 0xF1
                or 48 <= c <= 57 or 65 <= c <= 90 or 97 <= c <= 122)

    cut = []
    for i in items:
        if i["suspect"]:
            continue
        off = i["off"]
        end = off + 1 + blob[off]
        if end < len(blob) and alnum(blob[end - 1]) and alnum(blob[end]):
            cut.append(hex(off))
    assert len(cut) <= 5, f"{len(cut)} entries still cut mid-word: {cut[:10]}"

    suspects = [i for i in items if i["suspect"]]
    print(f"OK {len(items)} strings extracted, {len(suspects)} flagged suspect")


if __name__ == "__main__":
    test_extraction()
