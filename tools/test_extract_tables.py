#!/usr/bin/env python3
"""Regression test for tools/extract_tables.py (Task 10).

The expectations below are transcribed from the disassembly of `orig/g.exe`
(see docs/re/tables.md for every address).  They are therefore a *second
reading of the same source* the extractor reads, not an independent oracle --
their job is to stop the extractor drifting, and to fail loudly if a pattern
scan silently stops matching.  The genuinely independent checks are the
DOSBox-X oracle screens recorded in docs/re/tables.md.

Each table checks two files: `data/<table>.json` (runtime -- exactly what
the game reads) and `data/<table>.provenance.json` (where each runtime row's
facts came from in orig/g.exe, keyed by the row's natural key).
"""
import json
import pathlib
import re
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import addr  # noqa: E402  -- the address convention, defined once

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_items():
    items = json.loads((ROOT / "data" / "items.json").read_text(encoding="utf-8"))
    prov = json.loads(
        (ROOT / "data" / "items.provenance.json").read_text(encoding="utf-8")
    )

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

    # The runtime file must carry nothing provenance-shaped.
    for i in items:
        assert i["kind"] in {"weapon", "armor", "suit", "charm", "consumable", "misc"}
        assert isinstance(i["price"], int) or i["price"] is None
        # `^N` markup must never reach a name.
        assert "^" not in i["name"], i["name"]
        assert "price_src" not in i, i
        assert "src_off" not in i, i

    # Every runtime row traces back to exactly one provenance entry, keyed
    # by id, and nothing is lost in the split.
    assert set(prov) == set(ids), "provenance ids must match runtime ids exactly"
    for i in prov.values():
        assert set(i) == {"price_src", "src_off"}, i
        assert isinstance(i["src_off"], int)

    # The whole equipment set the status screen can display: 15 rows, no
    # more (the pattern scan is not allowed to invent one) and no fewer.
    assert len(items) == 15, f"expected 15 items, got {len(items)}"

    # Only the two rows whose shop text names the item verbatim with the
    # same bonus carry a price; everything else stays null.
    priced = {i["name"]: i["price"] for i in items if i["price"] is not None}
    assert priced == {"Кастет": 25, "Дубинка": 50}, priced

    # The provenance file is where that link is recorded.
    kastet_id = by_name["Кастет"]["id"]
    dubinka_id = by_name["Дубинка"]["id"]
    assert prov[kastet_id]["price_src"] == "bmar:5 20ae:0b3c"
    assert prov[dubinka_id]["price_src"] == "bmar:6 20ae:0b3d"

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

    print(f"OK {len(items)} items extracted ({len(prov)} provenance rows)")


def test_shops():
    shops = json.loads((ROOT / "data" / "shops.json").read_text(encoding="utf-8"))
    prov = json.loads(
        (ROOT / "data" / "shops.provenance.json").read_text(encoding="utf-8")
    )
    assert len(shops) == 18, f"expected 18 shop rows, got {len(shops)}"

    def key(row):
        return f'{row["shop"]}:{row["key"]}'

    # Shop rows have no `id`; the provenance file is keyed by "<shop>:<key>",
    # and every runtime row must trace back to exactly one such entry.
    assert set(prov) == {key(r) for r in shops}, "provenance keys must match rows exactly"
    assert len(prov) == len(shops), "a key collision would hide a dropped row"
    for p in prov.values():
        assert set(p) == {
            "price_addr", "displayed_price_addr", "charged",
            "code_off", "code_addr", "prefix_off", "text_off",
        }, p

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
    silencer_prov = prov[key(silencer)]
    assert silencer_prov["price_addr"] != silencer_prov["displayed_price_addr"]

    for row in shops:
        assert isinstance(row["price"], int)
        assert row["text"].startswith("#")
        assert "price_addr" not in row, row
        assert "code_addr" not in row, row
    for p in prov.values():
        assert p["charged"] is True, p
        assert p["price_addr"].startswith("20ae:")

    # Availability gates, as read at 1000:b9b3.. and 1000:c51d...
    assert [r["gate"] for r in by_shop["mar"]] == [
        None, None, None, None, None,
        "district>1", "district>1", "district>2", "district>3",
    ]
    print(f"OK {len(shops)} shop rows extracted ({len(prov)} provenance rows)")


def test_enemies():
    enemies = json.loads((ROOT / "data" / "enemies.json").read_text(encoding="utf-8"))
    prov = json.loads(
        (ROOT / "data" / "enemies.provenance.json").read_text(encoding="utf-8")
    )
    assert len(enemies) == 13, f"expected 13 enemy rows, got {len(enemies)}"

    by_id = {e["id"]: e for e in enemies}
    ids = [e["id"] for e in enemies]
    assert set(prov) == set(ids), "provenance ids must match runtime ids exactly"
    assert len(prov) == len(enemies), "a key collision would hide a dropped row"
    for p in prov.values():
        assert set(p) == {"source"}, p
    for e in enemies:
        assert "source" not in e, e

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
    assert "FUN_1000_11c2" in prov["rektor_ngu_v0"]["source"]
    assert prov["rektor_ngu_v0"]["source"].startswith("1000:11dc (file 0x2aac)")
    assert prov["rektor_ngu_v1"]["source"].startswith("1000:1205 (file 0x2ad5)")
    print(f"OK {len(enemies)} enemy rows extracted ({len(prov)} provenance rows)")


def test_other_price_sites():
    """The completeness invariant on data/other_price_sites.json.

    This one is not a second reading of the extractor's own output: it
    re-scans `orig/g.exe` for both `sub [20ae:38c7],*` encodings itself and
    demands that every single occurrence appear in the file, classified. That
    is what stops a debit site going unrecorded again -- the previous version
    of the artifact was missing the `sub [money],ax` at file 0x68e4 because
    the extractor scanned a longer idiom that site does not match, and the
    categories were emitted independently, so nothing noticed they no longer
    summed to the number of debits in the binary.
    """
    blob = (ROOT / "orig" / "g.exe").read_bytes()
    sites = json.loads(
        (ROOT / "data" / "other_price_sites.json").read_text(encoding="utf-8")
    )
    shops_prov = json.loads(
        (ROOT / "data" / "shops.provenance.json").read_text(encoding="utf-8")
    )

    # Ghidra's `1000:XXXX` and the file offset are two numbers for one place,
    # and getting them out of step is this project's recurring defect. Every
    # pair in the file must agree with tools/addr.py -- the one definition of
    # the convention -- everywhere, whatever the key is called.
    def check_addr_pairs(node, path="$"):
        n = 0
        if isinstance(node, dict):
            for a_key, f_key in [
                ("addr", "file_off"),
                ("string_load_addr", "string_load_file_off"),
            ]:
                a, f = node.get(a_key), node.get(f_key)
                if isinstance(a, str) and isinstance(f, str) and a.startswith("1000:"):
                    want = addr.file_off_of_citation(a)
                    assert int(f, 16) == want, (path, a_key, a, f, hex(want))
                    n += 1
            for k, v in node.items():
                n += check_addr_pairs(v, f"{path}.{k}")
        elif isinstance(node, list):
            for i, v in enumerate(node):
                n += check_addr_pairs(v, f"{path}[{i}]")
        return n

    pairs = check_addr_pairs(sites)
    assert pairs >= 21, f"expected every 1000: citation checked, saw {pairs}"

    # --- the `ax` form: 29 06 C7 38 --------------------------------------
    raw_ax = [m.start() for m in re.finditer(rb"\x29\x06\xC7\x38", blob)]
    ax = sites["ax_debit_sites"]
    assert ax["count"] == len(ax["sites"])
    assert ax["count"] == sum(ax["counts_by_category"].values()), ax["counts_by_category"]
    # Total equals what is in the binary, not what the extractor felt like
    # emitting: every `sub [money],ax` occurrence is present exactly once.
    listed = [int(s["file_off"], 16) for s in ax["sites"]]
    assert listed == sorted(listed), "sites must be in file order"
    assert listed == raw_ax, (len(listed), len(raw_ax))
    assert ax["count"] == 21, f"expected 21 sub [money],ax sites, got {ax['count']}"

    # Nothing may fall through the classifier unnoticed.
    for s in ax["sites"]:
        assert s["amount_form"] != "unrecognised", s
        assert s["category"] != "unrecognised", s
        assert s["recorded_in"], s

    assert ax["counts_by_category"] == {
        "computed": 1, "other": 1, "shop_row": 18, "variable": 1,
    }, ax["counts_by_category"]

    # Each category's sites must actually appear in the section that claims
    # them -- the sum is only meaningful if the detail is really there.
    by_cat = {}
    for s in ax["sites"]:
        by_cat.setdefault(s["category"], []).append(s)

    assert {s["shop_row"] for s in by_cat["shop_row"]} == set(shops_prov), (
        "every shop_row debit must name a row that exists in data/shops.json"
    )
    for s in by_cat["shop_row"]:
        assert s["recorded_in"] == "data/shops.json", s
        assert shops_prov[s["shop_row"]]["price_addr"] == s["amount_addr"], s

    assert {s["amount_addr"] for s in by_cat["variable"]} == {
        v["addr"] for v in sites["var_sites"]
    }
    assert {s["file_off"] for s in by_cat["computed"]} == {
        c["file_off"] for c in sites["computed_sites"]
    }
    assert {s["file_off"] for s in by_cat["other"]} == {
        o["file_off"] for o in sites["other_ax_sites"]
    }

    # The one `other` site: amount is a far call's return value, purpose
    # unknown. It is recorded *because* it debits money, not because anyone
    # knows what it charges for -- `what` must stay null until someone does.
    other = sites["other_ax_sites"]
    assert len(other) == 1, other
    assert other[0]["addr"] == "1000:5014" and other[0]["file_off"] == "0x68e4"
    assert other[0]["amount_form"] == "call_result"
    assert other[0]["call_target"] == "0f78:1131"
    assert other[0]["what"] is None, "unknown means unknown"

    # The computed site is scanned now, not hand-listed: both the multiplied
    # address and the multiplier come out of the bytes.
    computed = sites["computed_sites"]
    assert len(computed) == 1, computed
    assert computed[0]["file_off"] == "0x8eed"
    assert computed[0]["amount_addr"] == "20ae:3692"  # the district counter
    assert computed[0]["multiplier"] == 50
    assert computed[0]["formula"] == "district*50"

    # --- the `imm8` form: 83 2E C7 38 ib ---------------------------------
    raw_imm8 = [
        (m.start(), m.group(1)[0])
        for m in re.finditer(rb"\x83\x2E\xC7\x38(.)", blob, re.DOTALL)
    ]
    imm8 = sites["sub_word_imm8_sites"]
    assert len(imm8) == len(raw_imm8) == 11, (len(imm8), len(raw_imm8))
    assert [(int(s["file_off"], 16), s["imm"]) for s in imm8] == raw_imm8

    # Only the two Клуб rows are identified; the other nine stay null.
    named = {s["file_off"]: s["what"] for s in imm8 if s["what"]}
    assert set(named) == {"0xfb77", "0xfbec"}, named
    for s in imm8:
        assert isinstance(s["imm"], int)

    # --- the debited byte variable: 20ae:3c82 -----------------------------
    assert len(sites["var_sites"]) == 1
    var = sites["var_sites"][0]
    assert var["addr"] == "20ae:3c82"
    assert var["what"] is None, "unknown means unknown"
    # 14 references, and the four scanned idioms account for all 14 -- an
    # unaccounted reference means some further idiom touches the variable and
    # the account below is not the whole story.
    assert var["ref_count"] == blob.count(b"\x82\x3c") == 14, var["ref_count"]
    assert var["recorded_sites"] == var["ref_count"], var
    assert var["recorded_sites"] == (
        len(var["write_sites"]) + len(var["read_sites"]) + len(var["compare_sites"])
    )
    assert set(var["scanned_idioms"]) == {
        "write_sites", "read_sites", "compare_sites", "charge_sites",
    }
    # Not a constant: written 5 twice, stepped by 2 once, compared against 17.
    assert [(w["op"], w["imm"]) for w in var["write_sites"]] == [
        ("mov", 5), ("add", 2), ("mov", 5),
    ], var["write_sites"]
    assert [c["imm"] for c in var["compare_sites"]] == [17, 5, 17]
    assert len(var["read_sites"]) == 8
    # charge_sites is the debit read, a subset of read_sites -- not added in.
    assert len(var["charge_sites"]) == 1
    assert var["charge_sites"][0]["file_off"] in {
        r["file_off"] for r in var["read_sites"]
    }

    print(
        f"OK {ax['count']} sub[money],ax sites classified "
        f"({ax['counts_by_category']}), {len(imm8)} imm8 sites, "
        f"{var['recorded_sites']}/{var['ref_count']} refs to {var['addr']} accounted for"
    )


if __name__ == "__main__":
    subprocess.run([sys.executable, str(ROOT / "tools" / "extract_tables.py")], check=True)
    test_items()
    test_shops()
    test_enemies()
    test_other_price_sites()
