#!/usr/bin/env python3
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_tables():
    subprocess.run(
        [sys.executable, str(ROOT / "tools" / "extract_tables_indexed.py")], check=True
    )
    data = json.loads((ROOT / "data" / "string_tables.json").read_text(encoding="utf-8"))
    tables = {t["name"]: t for t in data["tables"]}

    assert set(tables) == {"ranks", "krutizna"}, f"unexpected tables: {sorted(tables)}"

    ranks = tables["ranks"]
    assert ranks["base"] == 0x123DE
    assert ranks["stride"] == 256
    assert len(ranks["entries"]) == 11
    assert [e["plain"] for e in ranks["entries"]] == [
        "Дохляк", "Нефор", "Нарк", "Подтсан", "Отморозок", "Гопник",
        "Вор", "Беспредельщик", "Мент", "Маньячок", "Ректор НГУ",
    ]

    kr = tables["krutizna"]
    assert kr["base"] == 0x12EF2
    assert kr["stride"] == 256
    assert len(kr["entries"]) == 43
    assert kr["entries"][0]["plain"] == "Опущеный"
    assert kr["entries"][21]["plain"] == "Пацан"
    assert kr["entries"][42]["plain"] == "Пацан, который всех опрокинул"

    # Offsets must follow the stride exactly.
    for t in data["tables"]:
        for i, e in enumerate(t["entries"]):
            assert e["off"] == t["base"] + i * t["stride"], (
                f"{t['name']}[{i}] off {e['off']:#x} breaks stride"
            )
            assert e["index"] == i

    print(f"OK {sum(len(t['entries']) for t in data['tables'])} table entries extracted")


if __name__ == "__main__":
    test_tables()
