#!/usr/bin/env python3
"""Regression test for tools/extract_tables.py (Task 10).

The expectations below are transcribed from the disassembly of `orig/g.exe`
(see docs/re/tables.md for every address).  They are therefore a *second
reading of the same source* the extractor reads, not an independent oracle --
their job is to stop the extractor drifting, and to fail loudly if a pattern
scan silently stops matching.  The genuinely independent checks are the
DOSBox-X oracle screens recorded in docs/re/tables.md.
"""
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_items():
    items = json.loads((ROOT / "data" / "items.json").read_text(encoding="utf-8"))

    names = {i["name"] for i in items}
    for expected in ["Тесак", "Кастет", "Дубинка", "Нож", "Бутсы",
                     "Костюм Adidas", "Кожанка", "Мега Кольцо"]:
        assert expected in names, f"missing item {expected}"

    by_name = {i["name"]: i for i in items}
    assert by_name["Тесак"]["kind"] == "weapon"
    assert by_name["Тесак"]["bonus"] == 9
    assert by_name["Нож"]["bonus"] == 6
    assert by_name["Костюм Adidas"]["kind"] == "suit"
    assert by_name["Костюм Adidas"]["bonus"] == 2

    ids = [i["id"] for i in items]
    assert len(set(ids)) == len(ids), "item ids must be unique"

    for i in items:
        assert i["kind"] in {"weapon", "armor", "suit", "charm", "consumable", "misc"}
        assert isinstance(i["price"], int) or i["price"] is None
        # `^N` markup must never reach a name.
        assert "^" not in i["name"], i["name"]

    # The whole equipment set the status screen can display: 15 rows, no
    # more (the pattern scan is not allowed to invent one) and no fewer.
    assert len(items) == 15, f"expected 15 items, got {len(items)}"

    # Only the two rows whose shop text names the item verbatim with the
    # same bonus carry a price; everything else stays null.
    priced = {i["name"]: i["price"] for i in items if i["price"] is not None}
    assert priced == {"Кастет": 25, "Дубинка": 50}, priced

    # `sold` splits the thirteen null prices: loot-only items never have a
    # price to find (docs/re/tables.md, "Prices are deliberately null...").
    not_sold = {i["name"] for i in items if not i["sold"]}
    assert not_sold == {
        "Крестик", 'Кольцо "Гс"', 'Кольцо "Пг"', "Мега Кольцо",
        'Кольцо "Гп"', "Нож", "Тесак",
    }, not_sold
    for i in items:
        if not i["sold"]:
            assert i["price"] is None, i

    print(f"OK {len(items)} items extracted")


def test_shops():
    shops = json.loads((ROOT / "data" / "shops.json").read_text(encoding="utf-8"))
    assert len(shops) == 18, f"expected 18 shop rows, got {len(shops)}"

    by_shop = {}
    for row in shops:
        by_shop.setdefault(row["shop"], []).append(row)
    assert set(by_shop) == {"mar", "bmar"}, sorted(by_shop)
    assert len(by_shop["mar"]) == 9
    assert len(by_shop["bmar"]) == 9

    # Keys 1..9 in screen order, per shop.
    for shop, rows in by_shop.items():
        assert [r["key"] for r in rows] == list("123456789"), (shop, rows)

    # Prices, in screen order, read from DS:0b2e.. -- docs/re/tables.md.
    assert [r["price"] for r in by_shop["mar"]] == [2, 5, 10, 15, 15, 25, 30, 30, 50]
    assert [r["price"] for r in by_shop["bmar"]] == [15, 30, 20, 10, 25, 50, 150, 70, 60]

    # The original's silencer row prints the *ammo* price. Reproduce the bug.
    silencer = by_shop["bmar"][8]
    assert "Глушитель" in silencer["text"]
    assert silencer["price"] == 60
    assert silencer["displayed_price"] == 70
    assert silencer["price_addr"] != silencer["displayed_price_addr"]

    for row in shops:
        assert isinstance(row["price"], int)
        assert row["charged"] is True, row
        assert row["price_addr"].startswith("20ae:")
        assert row["text"].startswith("#")

    # Availability gates, as read at 1000:b9b3.. and 1000:c51e...
    assert [r["gate"] for r in by_shop["mar"]] == [
        None, None, None, None, None,
        "district>1", "district>1", "district>2", "district>3",
    ]
    print(f"OK {len(shops)} shop rows extracted")


def test_enemies():
    enemies = json.loads((ROOT / "data" / "enemies.json").read_text(encoding="utf-8"))
    assert len(enemies) == 13, f"expected 13 enemy rows, got {len(enemies)}"

    by_id = {e["id"]: e for e in enemies}
    names = [e["name"] for e in enemies]
    assert names[:11] == [
        "Дохляк", "Нефор", "Нарк", "Подтсан", "Отморозок", "Гопник",
        "Вор", "Беспредельщик", "Мент", "Маньячок", "Ректор НГУ",
    ]

    # Classes 0..9 are rolled at 1000:0d14; no fixed stat block exists.
    for e in enemies[:10]:
        assert e["generated"] is True
        assert e["level"] is None
        assert e["stats"] is None
        assert len(e["growth_weights"]) == 4
    # Class 10 is clamped out of the random roll, so its rank row carries no
    # stats either; the two scripted variants below do.
    assert enemies[10]["generated"] is False
    assert enemies[10]["stats"] is None

    # 1000:0d14 rolls the class weights at DS:0002; index 9 is the last one
    # a random encounter can produce.
    assert enemies[9]["growth_weights"] == [5, 6, 8, 3]
    assert enemies[0]["growth_weights"] == [1, 2, 1, 2]

    # The two scripted endgame fights, both class 10, from 1000:11c2.
    boss0 = by_id["rektor_ngu_v0"]
    boss1 = by_id["rektor_ngu_v1"]
    assert boss0["generated"] is False and boss1["generated"] is False
    assert boss0["level"] == 125 and boss1["level"] == 160
    assert boss0["stats"] == {
        "strength": 41, "agility": 50, "vitality": 123, "luck": 36,
        "dmg_min": 20, "dmg_max": 41, "hp": 666, "hpmax": 666, "armor": 60,
    }
    assert boss1["stats"] == {
        "strength": 50, "agility": 60, "vitality": 188, "luck": 32,
        "dmg_min": 25, "dmg_max": 50, "hp": 1000, "hpmax": 1000, "armor": 80,
    }
    print(f"OK {len(enemies)} enemy rows extracted")


if __name__ == "__main__":
    subprocess.run([sys.executable, str(ROOT / "tools" / "extract_tables.py")], check=True)
    test_items()
    test_shops()
    test_enemies()
