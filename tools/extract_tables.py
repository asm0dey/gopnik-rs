#!/usr/bin/env python3
"""Extract the item, shop and enemy tables from orig/g.exe (Task 10).

Three tables, three different sources -- all of them the original binary,
never the Rust port:

* **items** (`data/items.json`).  The equipment the status screen lists.  Each
  name carries its own bonus in the display text ("^1Тесак(Урон+9) "), so the
  table falls out of a pattern scan over `data/strings.json` (Tasks 2/4b/4c,
  itself read straight out of `orig/g.exe`).  Prices are *not* in these
  strings; see below.
* **shops** (`data/shops.json`).  Every shop row is one rigid machine-code
  idiom in the code segment, and the price it prints is a byte in the const
  array at `20ae:0b2e`.  This module scans `orig/g.exe` for that idiom, so
  the row count and the price->row mapping fall out of the binary rather than
  being transcribed.
* **enemies** (`data/enemies.json`).  There is no table of enemy stat blocks:
  classes 0..9 are rolled at `1000:0d14` from the class-weight array at
  `20ae:0002`, so the honest table is the eleven class names (the `ranks`
  string table) with their weight rows, plus the two *scripted* endgame
  fights, whose stats are literal immediates in `1000:11c2`.

Every address in this file is documented in docs/re/tables.md.

Regenerate:  python3 tools/extract_tables.py
Check:       python3 tools/test_extract_tables.py
"""
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE = ROOT / "orig" / "g.exe"
STRINGS = ROOT / "data" / "strings.json"
STRING_TABLES = ROOT / "data" / "string_tables.json"
OUT_ITEMS = ROOT / "data" / "items.json"
OUT_SHOPS = ROOT / "data" / "shops.json"
OUT_ENEMIES = ROOT / "data" / "enemies.json"

# ---------------------------------------------------------------------------
# Segment arithmetic.
#
# Ghidra loads g.exe with CODE_0 at 1000:0000 and the const/data segment at
# 20ae:0000.  The file offset of a segment:offset pair is
#     0x18D0 + (segment - 0x1000) * 16 + offset
# (0x18D0 is the end of the MZ header, i.e. the file offset of 1000:0000).
# Task 4b's data/string_pointers.json uses the identical formula.
CODE_SEG = 0x1000
DATA_SEG = 0x20AE
FILE_BASE = 0x18D0


def code_file_off(off: int) -> int:
    return FILE_BASE + off


def data_file_off(off: int) -> int:
    return FILE_BASE + (DATA_SEG - CODE_SEG) * 16 + off


MARKUP_RE = re.compile(r"\^[0-7]")


def strip_markup(s: str) -> str:
    """Remove the original's `^N` colour directives, leaving displayable text."""
    return MARKUP_RE.sub("", s)


def read_shortstring(blob: bytes, file_off: int) -> str:
    """Read one Borland Pascal length-prefixed CP866 shortstring."""
    n = blob[file_off]
    return blob[file_off + 1 : file_off + 1 + n].decode("cp866")


# ---------------------------------------------------------------------------
# Items
# ---------------------------------------------------------------------------
#
# name-suffix -> kind.  Order matters: the specific "Урон+N" / "Удача +N" /
# "Всё +N" forms must be tried before the bare "+N".
# (regex, kind, effect).  `effect` is what the parenthesised suffix literally
# says the bonus applies to; the bare "(+N)" form says nothing, so it is None
# rather than guessed from the slot.
ITEM_PATTERNS = [
    (re.compile(r"^(?P<name>.+?)\(Урон\s*\+(?P<bonus>\d+)\)"), "weapon", "damage"),
    (re.compile(r"^(?P<name>.+?)\(Удача\s*\+(?P<bonus>\d+)\)"), "charm", "luck"),
    (re.compile(r"^(?P<name>.+?)\(Всё\s*\+(?P<bonus>\d+)\)"), "charm", "all"),
    (re.compile(r"^(?P<name>.+?)\(Самолечение\)"), "charm", "regen"),
    (re.compile(r"^(?P<name>.+?)\(\+(?P<bonus>\d+)\)"), None, None),
]

# Bare "(+N)" says nothing about the slot, so classify by name.  Every key
# here is a string that exists verbatim in data/strings.json; nothing is
# invented, and a name that is not listed falls through to "misc" rather than
# being guessed at.
BARE_KIND = {
    "Бутсы": "weapon",
    "Кастет": "weapon",
    "Дубинка": "weapon",
    "Нож": "weapon",
    "Костюм Adidas": "suit",
    "Костюм Abibas": "suit",
    "Кожанка": "armor",
    "Крутая кожанка": "armor",
}

# These seven never appear in a shop row: they are printed a second time, as
# a find rather than a possession, by the wandering-encounter event table at
# file 0x5389..0x5555 (1000:3ab9..1000:3c85) -- a run of one-shot
# `cmp byte [flag],0` / `mov byte [flag],1` / print-string pickups, all in
# the identical shape. Four of the seven literally say "Ты нашёл"/"нашел"
# ("you found"); the other three (rings) do not, but sit inside the same
# contiguous block, gated by the same one-shot-flag idiom, and printed by
# the same far call -- structural evidence, not a guess from wording. See
# docs/re/tables.md, "Prices are deliberately null...".  `price` for these
# stays null forever, not pending: nothing sells them, so `sold` is False.
LOOT_ONLY = {
    "Крестик",
    "Кольцо \"Гс\"",
    "Кольцо \"Пг\"",
    "Мега Кольцо",
    "Кольцо \"Гп\"",
    "Нож",
    "Тесак",
}

TRANSLIT = {
    "а": "a", "б": "b", "в": "v", "г": "g", "д": "d", "е": "e", "ё": "e",
    "ж": "zh", "з": "z", "и": "i", "й": "y", "к": "k", "л": "l", "м": "m",
    "н": "n", "о": "o", "п": "p", "р": "r", "с": "s", "т": "t", "у": "u",
    "ф": "f", "х": "h", "ц": "c", "ч": "ch", "ш": "sh", "щ": "sch",
    "ъ": "", "ы": "y", "ь": "", "э": "e", "ю": "yu", "я": "ya",
}


def slug(name: str) -> str:
    out = []
    for ch in name.lower():
        if ch in TRANSLIT:
            out.append(TRANSLIT[ch])
        elif ch.isalnum():
            out.append(ch)
        elif out and out[-1] != "_":
            out.append("_")
    return "".join(out).strip("_")


def parse_items(strings: list) -> list:
    """Pull the equipment set out of the recovered string table.

    Only strings that begin with the `^1` colour directive are considered: in
    the original those are the inventory lines the status screen prints, and
    the restriction is what keeps the shop menu (which starts `#^7 `) and the
    gambling menu out of the item table.  Both would otherwise match the bare
    `(+N)` / `(Удача +N)` forms and put a row like
    "# руб. Понтовёйшие бутсы" into the table.
    """
    items = {}
    for s in strings:
        if not s["text"].startswith("^1"):
            continue
        text = strip_markup(s["text"]).strip()
        for rx, kind, effect in ITEM_PATTERNS:
            m = rx.match(text)
            if not m:
                continue
            name = m.group("name").strip()
            bonus = int(m.groupdict().get("bonus") or 0)
            k = kind or BARE_KIND.get(name, "misc")
            if name not in items:
                items[name] = {
                    "id": slug(name),
                    "name": name,
                    "kind": k,
                    "bonus": bonus,
                    "effect": effect,
                    "price": None,
                    "price_src": None,
                    "sold": name not in LOOT_ONLY,
                    "src_off": s["off"],
                }
            break
    ordered = sorted(items.values(), key=lambda i: i["src_off"])
    seen = set()
    for it in ordered:
        base = it["id"]
        n = 2
        while it["id"] in seen:
            it["id"] = f"{base}_{n}"
            n += 1
        seen.add(it["id"])
    return ordered


# The shop rows say "Кастет(урон+2)" where the inventory says "Кастет(+2)".
# Attach a price to an item only when a shop row names it *verbatim* and the
# bonus in the shop text matches the bonus in the item text; anything looser
# (e.g. "Понтовые бутсы(Увеличивают урон)" at 15 руб. vs. "Понтовёйшие
# бутсы(Урон+2)" at 30 руб. against the single inventory line "Понтовые
# бутсы(Урон+2)") is ambiguous and is deliberately left null.
SHOP_ITEM_RE = re.compile(r"(?P<name>[^.]+?)\((?:урон|Урон)\s*\+(?P<bonus>\d+)\)")


def link_item_prices(items: list, shops: list) -> None:
    for row in shops:
        text = strip_markup(row["text"])
        m = SHOP_ITEM_RE.search(text)
        if not m:
            continue
        name = m.group("name").strip()
        bonus = int(m.group("bonus"))
        for it in items:
            if it["name"] == name and it["bonus"] == bonus and it["price"] is None:
                it["price"] = row["price"]
                it["price_src"] = f'{row["shop"]}:{row["key"]} {row["price_addr"]}'


# ---------------------------------------------------------------------------
# Shops
# ---------------------------------------------------------------------------
#
# One shop row compiles to exactly this, every time (verified against ndisasm
# of orig/g.exe; the first row is at file offset 0xD283 = 1000:B9B3):
#
#   A0 lo hi        mov al,[price]        ; affordability colour test
#   30 E4           xor ah,ah
#   3B 06 C7 38     cmp ax,[0x38c7]       ; 20ae:38c7 = the player's money
#   7E 07           jng +7
#   C6 06 7A 3B 34  mov byte [0x3b7a],'4' ; red   - cannot afford
#   EB 05           jmp +5
#   C6 06 7A 3B 30  mov byte [0x3b7a],'0' ; black - can afford
#   8D BE dd dd     lea di,[bp+disp]
#   16 57           push ss / push di
#   BF lo hi        mov di,<prefix string>    ; "^61^7 - ^" : the hotkey
#   0E 57           push cs / push di
#   9A E7 0A 78 0F  call 0f78:0ae7            ; assign string
#   8D BE dd dd     lea di,[bp+disp]
#   16 57
#   A0 7A 3B        mov al,[0x3b7a]
#   50              push ax
#   9A 03 0C 78 0F  call 0f78:0c03            ; append the colour digit
#   9A 66 0B 78 0F  call 0f78:0b66            ; append
#   BF lo hi        mov di,<row string>       ; "#^7 руб. ..."
#   0E 57
#   9A 66 0B 78 0F  call 0f78:0b66
#   A0 lo hi        mov al,[price]            ; the price that is *printed*
#   30 E4
#   50              push ax
#
# The two `mov al,[price]` operands are normally the same address.  They are
# not for the silencer -- see docs/re/tables.md.
SHOP_ROW_RE = re.compile(
    rb"\xA0(?P<gate_price>..)\x30\xE4\x3B\x06\xC7\x38\x7E\x07"
    rb"\xC6\x06\x7A\x3B\x34\xEB\x05\xC6\x06\x7A\x3B\x30"
    rb"\x8D\xBE..\x16\x57\xBF(?P<prefix>..)\x0E\x57\x9A\xE7\x0A\x78\x0F"
    rb"\x8D\xBE..\x16\x57\xA0\x7A\x3B\x50\x9A\x03\x0C\x78\x0F"
    rb"\x9A\x66\x0B\x78\x0F"
    rb"\xBF(?P<name>..)\x0E\x57\x9A\x66\x0B\x78\x0F"
    rb"\xA0(?P<shown_price>..)\x30\xE4\x50",
    re.DOTALL,
)

# `mov di,<imm>` / `push cs` / `push di` -- how the code hands a code-segment
# string to a runtime routine.  Used to find the location tag ("mar", "bmar")
# that precedes each shop's menu.
STR_LOAD_RE = re.compile(rb"\xBF(?P<off>..)\x0E\x57", re.DOTALL)
TAG_RE = re.compile(r"^[a-z]{2,6}$")

# The hotkey lives inside the row prefix, between two colour directives:
# "^61^7 - ^" -> "1".
KEY_RE = re.compile(r"\^[0-7](?P<key>.)")

# `mov al,[price]` / `xor ah,ah` / `sub [20ae:38c7],ax` -- the site that
# actually debits the player.  Collected into a single file-wide set of
# debited addresses; a row is `charged: true` when the address its
# affordability test reads is a member.  That is membership in the set, not
# a per-row check that *this row's own* handler is what debits it -- it is
# sound here only because every address in the set turns out to be debited
# exactly once (verified by hand), so the two coincide.  See
# docs/re/tables.md, "The row idiom".
CHARGE_RE = re.compile(rb"\xA0(?P<addr>..)\x30\xE4\x29\x06\xC7\x38", re.DOTALL)

# `cmp byte [imm16],imm8` followed by a conditional jump: the availability
# gates.  20ae:3692 is the district counter (1..5), raised at file 0xC462
# (1000:ab92, `inc byte [0x3692]`) once крутизна reaches district*10.
CMP_GATE_RE = re.compile(rb"\x80\x3E(?P<addr>..)(?P<imm>.)", re.DOTALL)

# 0x72/0x73 (jb/jnb) are the unsigned counterparts of 0x7C/0x7D (jl/jge), and
# at least one gate site outside the shop blocks uses them (1000:e912, a
# district>=4 gate on an unrelated menu). Missing from this table, they used
# to fail `if op not in JCC: continue` silently -- a gate using them inside
# a shop block would have been dropped from the row without any error. None
# of the 18 shop rows currently use this encoding, but the failure mode was
# wrong regardless, so they are listed rather than left to raise.
JCC = {0x72: "<", 0x73: ">=", 0x74: "==", 0x75: "!=", 0x76: "<=", 0x77: ">",
       0x7C: "<", 0x7D: ">=", 0x7E: "<=", 0x7F: ">"}
NEGATE = {"==": "!=", "!=": "==", "<=": ">", ">": "<=", "<": ">=", ">=": "<"}

DISTRICT_ADDR = 0x3692


def u16(b: bytes) -> int:
    return b[0] | (b[1] << 8)


def gate_name(addr: int) -> str:
    return "district" if addr == DISTRICT_ADDR else f"byte[20ae:{addr:04x}]"


def find_gates(blob: bytes, lo: int, hi: int) -> list:
    """Every `cmp byte [x],n` + conditional jump in [lo, hi).

    Returns (position, condition-to-enter, skip-target) triples in file
    offsets.  Two encodings occur: a plain `Jcc rel8` that jumps *past* the
    guarded block, and -- when the block is longer than a rel8 reaches --
    `Jcc +3` over an `E9 rel16` near jump, which inverts the sense.
    """
    gates = []
    for m in CMP_GATE_RE.finditer(blob, lo, hi):
        addr = u16(m.group("addr"))
        imm = m.group("imm")[0]
        p = m.end()
        op = blob[p]
        if op not in JCC:
            continue
        rel = blob[p + 1]
        if rel == 3 and blob[p + 2] == 0xE9:
            # Jcc over a near jmp: taken means *enter* the block.
            cond = JCC[op]
            disp = u16(blob[p + 3 : p + 5])
            target = p + 5 + (disp - 0x10000 if disp >= 0x8000 else disp)
        else:
            cond = NEGATE[JCC[op]]
            target = p + 2 + (rel - 0x100 if rel >= 0x80 else rel)
        gates.append((m.start(), f"{gate_name(addr)}{cond}{imm}", target))
    return gates


def parse_shops(blob: bytes) -> list:
    rows = []
    for m in SHOP_ROW_RE.finditer(blob):
        start = m.start()
        prefix = read_shortstring(blob, code_file_off(u16(m.group("prefix"))))
        text = read_shortstring(blob, code_file_off(u16(m.group("name"))))
        key_m = KEY_RE.match(prefix)
        key = key_m.group("key") if key_m else prefix
        rows.append(
            {
                "shop": None,
                "key": key,
                "text": text,
                "price": blob[data_file_off(u16(m.group("gate_price")))],
                "price_addr": f'20ae:{u16(m.group("gate_price")):04x}',
                "displayed_price": blob[data_file_off(u16(m.group("shown_price")))],
                "displayed_price_addr": f'20ae:{u16(m.group("shown_price")):04x}',
                "charged": False,
                "gate": None,
                "extra_gates": [],
                "code_off": start,
                "code_addr": f"1000:{start - FILE_BASE:04x}",
                "prefix_off": code_file_off(u16(m.group("prefix"))),
                "text_off": code_file_off(u16(m.group("name"))),
            }
        )

    charged = {u16(m.group("addr")) for m in CHARGE_RE.finditer(blob)}
    for row in rows:
        row["charged"] = int(row["price_addr"].split(":")[1], 16) in charged

    # Attribute rows to shops: the location tag ("mar", "bmar") is the last
    # short all-lowercase-ASCII string the code loads before the menu.
    tags = []
    for m in STR_LOAD_RE.finditer(blob, 0, max(r["code_off"] for r in rows) + 1):
        try:
            s = read_shortstring(blob, code_file_off(u16(m.group("off"))))
        except (IndexError, UnicodeDecodeError):
            continue
        if TAG_RE.match(s):
            tags.append((m.start(), s))
    for row in rows:
        preceding = [t for t in tags if t[0] < row["code_off"]]
        row["shop"] = preceding[-1][1] if preceding else None

    # Gates, per shop block.
    for shop in {r["shop"] for r in rows}:
        block = [r for r in rows if r["shop"] == shop]
        lo = min(r["code_off"] for r in block)
        hi = max(r["code_off"] for r in block) + 1
        gates = find_gates(blob, lo - 0x40, hi)
        for row in block:
            active = [
                g for g in gates if g[0] < row["code_off"] < g[2]
            ]
            district = [g[1] for g in active if g[1].startswith("district")]
            row["gate"] = district[-1] if district else None
            row["extra_gates"] = [g[1] for g in active if not g[1].startswith("district")]
    return rows


# ---------------------------------------------------------------------------
# Enemies
# ---------------------------------------------------------------------------
#
# 1000:0d14 rolls a random encounter: it picks the class index at 20ae:3952,
# then distributes (weights-sum + крутизна*2) points over strength / agility /
# vitality / luck in proportion to the four weight bytes at 20ae:0002 + class*4.
# So classes 0..9 have no stat block to extract -- only a weight row.
CLASS_WEIGHTS_ADDR = 0x0002
CLASS_WEIGHT_STRIDE = 4

# 1000:11c2 (file 0x2A92) is the only place a fixed enemy is written.  Both
# calls sit in the endgame, back to back, at 1000:ae27/1000:ae33 (file
# 0xC6F7/0xC703) -- each followed by a call to the combat routine
# FUN_1000_3d11.  The store order is fixed:
#   C7 06 5C 39 <level>   mov word [20ae:395c],imm   ; крутизна
#   C7 06 54 39 <str>     mov word [20ae:3954],imm
#   C7 06 56 39 <agi>     mov word [20ae:3956],imm
#   C7 06 58 39 <vit>     mov word [20ae:3958],imm
#   C7 06 5A 39 <luck>    mov word [20ae:395a],imm
#   C6 06 68 39 <armor>   mov byte [20ae:3968],imm
BOSS_RE = re.compile(
    rb"\xC7\x06\x5C\x39(?P<level>..)"
    rb"\xC7\x06\x54\x39(?P<strength>..)"
    rb"\xC7\x06\x56\x39(?P<agility>..)"
    rb"\xC7\x06\x58\x39(?P<vitality>..)"
    rb"\xC7\x06\x5A\x39(?P<luck>..)"
    rb"\xC6\x06\x68\x39(?P<armor>.)",
    re.DOTALL,
)

BOSS_CLASS = 10


def parse_enemies(blob: bytes, ranks: list) -> list:
    enemies = []
    for rank in ranks:
        idx = rank["index"]
        base = data_file_off(CLASS_WEIGHTS_ADDR + idx * CLASS_WEIGHT_STRIDE)
        weights = list(blob[base : base + CLASS_WEIGHT_STRIDE])
        weights_addr = f"20ae:{CLASS_WEIGHTS_ADDR + idx * CLASS_WEIGHT_STRIDE:04x}"
        if idx < BOSS_CLASS:
            source = (
                f"1000:0d14 rolls stats from the weights at {weights_addr}; "
                "name from the `ranks` string table at 20ae:002e."
            )
        else:
            # 1000:0d9a (`cmp word [0x3952],0x9` / `jng`) clamps the rolled
            # class to 9, so 1000:0d14 never rolls class 10 and never reads
            # this weight row -- unlike every row above, whose `generated`
            # is True. Its stats are the two scripted rows below instead.
            source = (
                "1000:0d9a clamps the rolled class to 9, so 1000:0d14 never "
                f"produces this class and never reads the weights at "
                f"{weights_addr}; name from the `ranks` string table at "
                "20ae:002e, stats from the two scripted `_v0`/`_v1` rows below."
            )
        enemies.append(
            {
                "id": slug(rank["plain"]),
                "name": rank["plain"],
                "class": idx,
                "level": None,
                "stats": None,
                "growth_weights": weights,
                # 1000:0d9a clamps the rolled class to 9, so index 10 is
                # never a random encounter; its two stat blocks are the
                # scripted `*_v0` / `*_v1` rows below.
                "generated": idx < BOSS_CLASS,
                "source": source,
            }
        )

    boss_name = next(r["plain"] for r in ranks if r["index"] == BOSS_CLASS)
    for variant, m in enumerate(BOSS_RE.finditer(blob)):
        level = u16(m.group("level"))
        strength = u16(m.group("strength"))
        vitality = u16(m.group("vitality"))
        # 1000:1228..1000:124f (file 0x2af8..0x2b1f), shared with the random
        # path at 1000:0ff3..1000:1005 (file 0x28c3..0x28d5):
        #   dmg_min = strength div 2, dmg_max = strength,
        #   hpmax = vitality*5 + strength + 10, hp = hpmax.
        # (An earlier draft cited the *file offset* 0x2af8 with a `1000:`
        # segment prefix it was never assigned to -- that literal address,
        # 1000:2af8, disassembles to unrelated code reading [20ae:38c3].
        # Verified with `ndisasm -b16` over both file regions directly; see
        # docs/re/tables.md.)
        hpmax = vitality * 5 + strength + 10
        enemies.append(
            {
                "id": f"{slug(boss_name)}_v{variant}",
                "name": boss_name,
                "class": BOSS_CLASS,
                "level": level,
                "stats": {
                    "strength": strength,
                    "agility": u16(m.group("agility")),
                    "vitality": vitality,
                    "luck": u16(m.group("luck")),
                    "dmg_min": strength // 2,
                    "dmg_max": strength,
                    "hp": hpmax,
                    "hpmax": hpmax,
                    "armor": m.group("armor")[0],
                },
                "growth_weights": list(
                    blob[
                        data_file_off(CLASS_WEIGHTS_ADDR + BOSS_CLASS * CLASS_WEIGHT_STRIDE) :
                        data_file_off(CLASS_WEIGHTS_ADDR + BOSS_CLASS * CLASS_WEIGHT_STRIDE)
                        + CLASS_WEIGHT_STRIDE
                    ]
                ),
                "generated": False,
                "source": f"1000:11c2 (file 0x{m.start():04x}), param_1 == {variant}",
            }
        )
    return enemies


def write_json(path: pathlib.Path, payload) -> None:
    path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )


def main() -> None:
    blob = EXE.read_bytes()
    strings = json.loads(STRINGS.read_text(encoding="utf-8"))
    tables = json.loads(STRING_TABLES.read_text(encoding="utf-8"))
    ranks = next(t for t in tables["tables"] if t["name"] == "ranks")["entries"]

    shops = parse_shops(blob)
    items = parse_items(strings)
    link_item_prices(items, shops)
    enemies = parse_enemies(blob, ranks)

    write_json(OUT_ITEMS, items)
    write_json(OUT_SHOPS, shops)
    write_json(OUT_ENEMIES, enemies)
    print(
        f"wrote {len(items)} items, {len(shops)} shop rows, "
        f"{len(enemies)} enemy rows"
    )


if __name__ == "__main__":
    main()
