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

    assert len(items) == 696, f"expected 696 strings, got {len(items)}"

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

    print(f"OK {len(items)} strings extracted and validated")


if __name__ == "__main__":
    test_extraction()
