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

Each table is written as two files: `data/<table>.json` carries only what
the game reads to play (the runtime file `src/data.rs` embeds), and
`data/<table>.provenance.json` carries only where each fact came from in
`orig/g.exe` -- addresses, file offsets, and the cross-checks that
confirmed them. See docs/re/tables.md, "Runtime vs. provenance" for which
file to read for what.

Regenerate:  python3 tools/extract_tables.py
Check:       python3 tools/test_extract_tables.py
"""
import json
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import addr  # noqa: E402  -- the address convention, defined once

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE = ROOT / "orig" / "g.exe"
STRINGS = ROOT / "data" / "strings.json"
STRING_TABLES = ROOT / "data" / "string_tables.json"
OUT_ITEMS = ROOT / "data" / "items.json"
OUT_SHOPS = ROOT / "data" / "shops.json"
OUT_ENEMIES = ROOT / "data" / "enemies.json"
OUT_ITEMS_PROV = ROOT / "data" / "items.provenance.json"
OUT_SHOPS_PROV = ROOT / "data" / "shops.provenance.json"
OUT_ENEMIES_PROV = ROOT / "data" / "enemies.provenance.json"
OUT_OTHER_PRICES = ROOT / "data" / "other_price_sites.json"

# ---------------------------------------------------------------------------
# Segment arithmetic.  Defined once, in tools/addr.py -- not restated here.
# docs/re/METHODOLOGY.md, "Address convention, and its range of validity", is
# the human-readable authority; `python3 tools/re_query.py resolve <citation>`
# checks any single address against the bytes.
CODE_SEG = addr.GHIDRA_BASE_SEG
DATA_SEG = addr.DATA_SEG_GHIDRA
FILE_BASE = addr.file_off_of_image_off(0)


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
# actually debits the player.  Collected into a single file-wide map from the
# match position to the debited byte's address; a row is `charged: true` when
# the address its affordability test reads is one of the values.  That is
# membership in the set, not a per-row check that *this row's own* handler is
# what debits it -- it is sound here only because every address in the set
# turns out to be debited exactly once (verified by hand), so the two
# coincide.  See docs/re/tables.md, "The row idiom".
#
# This is only one of three ways the binary produces the `ax` that
# `sub [20ae:38c7],ax` subtracts; the "Other price sites" section below scans
# the bare `sub` and classifies *every* occurrence, so no debit site can go
# unrecorded.  The map this returns is what that section reuses -- there is
# no second full-file rescan.
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


def parse_shops(blob: bytes) -> tuple:
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

    # One full-file CHARGE_RE scan, kept as {match position -> debited byte
    # address} so the "Other price sites" section below can classify each
    # `sub [money],ax` site without rescanning the file.
    charge_map = {m.start(): u16(m.group("addr")) for m in CHARGE_RE.finditer(blob)}
    charged = set(charge_map.values())
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
    return rows, charge_map


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
                "source": (
                    f"1000:{m.start() - FILE_BASE:04x} (file 0x{m.start():04x}), "
                    f"inside FUN_1000_11c2, param_1 == {variant}"
                ),
            }
        )
    return enemies


# ---------------------------------------------------------------------------
# Other price sites (data/other_price_sites.json)
# ---------------------------------------------------------------------------
#
# The player's money is the word at 20ae:38c7. Every instruction in the binary
# that debits it is one of exactly two `sub` encodings:
#
#   29 06 C7 38      sub word [20ae:38c7],ax     -- amount in ax
#   83 2E C7 38 ib   sub word [20ae:38c7],imm8   -- amount is an immediate
#
# Both are scanned over the whole file. The `ax` form is the one that used to
# be recorded only indirectly (through CHARGE_RE, which matches the *whole*
# `mov al,[byte] / xor ah,ah / sub` idiom and therefore silently skipped every
# site whose ax comes from somewhere else); a `sub [money],ax` at file 0x68e4
# was left unrecorded that way. So the scan below is on the bare `sub`, and
# every one of its matches is then classified by which idiom produced ax.
# Total-equals-sum is therefore true by construction rather than by hope, and
# tools/test_extract_tables.py re-scans orig/g.exe itself and asserts it.
#
# See docs/re/tables.md, "Other price sources, not extracted".

# `sub word [20ae:38c7],ax` -- the amount is whatever is in ax.
SUB_AX_RE = re.compile(rb"\x29\x06\xC7\x38")

# `sub word [20ae:38c7],imm8` -- the price is an immediate operand, not read
# from any array or variable. Found by scanning for `83 2E C7 38 ib`.
SUB_IMM8_RE = re.compile(rb"\x83\x2E\xC7\x38(?P<imm>.)", re.DOTALL)

# The three idioms that produce the `ax` SUB_AX_RE subtracts. CHARGE_RE (in
# the shops section above) is the first of them; these two are the others.
#
# `mov al,[byte] / xor ah,ah / mov dx,imm16 / mul dx / sub [money],ax` -- the
# amount is a byte variable times a constant. This is the Рушель Блаво
# save-game charge's idiom, which an earlier revision of this script claimed
# had "no fixed byte idiom to scan for". It does: these are the exact bytes
# at file 0x8eed. What resists scanning is the *meaning* (which service is
# being paid for, which string is printed), not the site -- so the site,
# the multiplied address and the multiplier are all scanned, and only the
# annotation is hand-verified (COMPUTED_WHAT below).
MUL_CHARGE_RE = re.compile(
    rb"\xA0(?P<addr>..)\x30\xE4\xBA(?P<mul>..)\xF7\xE2\x29\x06\xC7\x38", re.DOTALL
)

# `call far seg:off / sub [money],ax` -- the amount is a call's return value.
CALL_CHARGE_RE = re.compile(
    rb"\x9A(?P<off>..)(?P<seg>..)\x29\x06\xC7\x38", re.DOTALL
)

# `mov byte [addr],imm8` / `add byte [addr],imm8` / `mov al,[addr]` /
# `cmp byte [addr],imm8` -- the four idioms that touch a byte variable. Used
# below to account for every reference to a debited variable's address.
MOV_BYTE_IMM_RE = re.compile(rb"\xC6\x06(?P<addr>..)(?P<imm>.)", re.DOTALL)
ADD_BYTE_IMM_RE = re.compile(rb"\x80\x06(?P<addr>..)(?P<imm>.)", re.DOTALL)
MOV_AL_ADDR_RE = re.compile(rb"\xA0(?P<addr>..)", re.DOTALL)
# `cmp byte [addr],imm8` is byte-for-byte the availability-gate idiom the
# shops section already compiles; aliased rather than compiled twice so the
# two can never drift apart.
CMP_BYTE_IMM_RE = CMP_GATE_RE

# ---- The only hand-written knowledge in this section ----------------------
#
# Everything else below -- every address, file offset, immediate, multiplier,
# call target, and the category each debit site falls into -- is derived from
# orig/g.exe by the scans above. These two tables are not: they are the
# `what` annotations, verified by reading the shortstrings at the named file
# offsets straight out of orig/g.exe, and they are keyed by file offset so a
# shifted scan cannot silently re-attach them to the wrong site.

# Ten of the eleven immediate-price sites are identified. The two Клуб
# (gambling) rows print their price as the literal `#` at the front of their
# own row text (docs/re/tables.md), rather than through the shop `#`
# placeholder, which is why they are not `mar`/`bmar` rows. Verified by
# reading the shortstrings at these exact file offsets straight out of
# orig/g.exe -- not inferred from proximity to the charge site, which
# false-matches more than once when tried (see
# .superpowers/sdd/task-10-report.md).
#
# The other eight were added by the final-review fix wave, from the same two
# signals `docs/re/difftest.md`, "Eight of the nine unnamed `sub [money],imm8`
# sites are now named", records -- the verb-dispatch span the site falls in and
# the token the nearest PRECEDING `call 0f78:0bd8` compares -- both re-derived
# from orig/g.exe rather than copied from that document:
#
#   site        nearest preceding compare   token (file)   imm
#   1000:d553   1000:d537                   `r`  (0xb320)   7
#   1000:d5d9   1000:d5b9                   `h`  (0xb392)   3
#   1000:d78e   1000:d6ed                   `girl` (0xb46a) 12
#   1000:e657   1000:e62e                   `1`  (0xa69a)  20
#   1000:e6e3   1000:e6ba                   `2`  (0xa71b)  20
#   1000:e796   1000:e73c                   `3`  (0xa775)  10
#   1000:e823   1000:e7f3                   `4`  (0xa7c7)  30
#   1000:e8b8   1000:e875                   `5`  (0xa83b)  20
#
# Each debit's immediate equals the price its menu row displays (`IMM_ROWS` in
# src/game.rs, cross-checked by tools/difftest.py's `imm_row_site` quantity),
# which is the independent second signal: `rep` `h`=3 / `r`=7 and `trn`
# 1..5 = 20/20/10/30/20. `girl` is not a menu row -- it is the flat 12 rubles
# `1000:d78e` takes inside the visit at `1000:d701`..`1000:d798`.
#
# `0x68fc` (`1000:502c`, 7) stays `what: null`: it is in no location handler,
# and Ghidra puts it inside FUN_1000_3d11, the combat routine. Unknown means
# unknown.
SUB_IMM8_WHAT = {
    0xEE23: 'rep row `r` (compare 1000:d537): 7^7 рублей починят переломы, string at file 0xb2d9',
    0xEEA9: 'rep row `h` (compare 1000:d5b9): 3^7 рубля тебя залатают, string at file 0xb2b2',
    0xF05E: 'girl: the flat 12 rubles the visit takes at 1000:d78e (compare 1000:d6ed); not a menu row',
    0xFB77: 'Клуб: 15^7  потусоваться на дискотеке(Ловкость +1), string at file 0xba50',
    0xFBEC: 'Клуб: 22^7  разузнать приемы мухлёжников(Удача +1), string at file 0xba85',
    0xFF27: 'trn row `1` (compare 1000:e62e): 20^7  качаться гателями и шгангой(Сила +1), string at file 0xbc55',
    0xFFB3: 'trn row `2` (compare 1000:e6ba): 20^7  качаться на тренажерах(Выносливость +1), string at file 0xbc80',
    0x10066: 'trn row `3` (compare 1000:e73c): 10^7  прокачать # качков опыта, string at file 0xbcb6',
    0x100F3: 'trn row `4` (compare 1000:e7f3): 30^7  купить зубную защиту боксёров(-75% что сломают челюсть), string at file 0xbcdd',
    0x10188: 'trn row `5` (compare 1000:e875): 20^7  прокачать пресс(Броня +1), string at file 0xbd23',
}

# The one `mul` charge site. The formula falls out of the scan; the service
# name and the string cross-reference (which string is printed, and where the
# instruction that loads it lives -- two different numbers) are hand-verified
# with `ndisasm -b16` over the file region. See docs/re/tables.md.
COMPUTED_WHAT = {
    0x8EED: {
        "what": "Рушель Блаво save-game service",
        "guard_addr": "1000:760a",
        "string_file_off": "0x8d2d",
        "string_text": "За # рублей он может сделать сохранение прямо здесь.",
        "string_load_addr": "1000:7583",
        "string_load_file_off": "0x8e53",
    }
}

# byte[20ae:3692] is the district counter -- see CMP_GATE_RE above. Naming it
# in a derived formula string is the same fact gate_name() already asserts.
VAR_NAMES = {DISTRICT_ADDR: "district"}


def code_addr(file_off: int) -> str:
    """Ghidra `1000:off` for a file offset.

    Derived, never transcribed: the inverse of tools/addr.py's Form A with
    seg == 0x1000, i.e. the image offset is `file_off - FILE_BASE`.
    """
    return f"1000:{file_off - FILE_BASE:04x}"


def find_sub_imm8_sites(blob: bytes) -> list:
    sites = []
    for m in SUB_IMM8_RE.finditer(blob):
        sites.append(
            {
                "addr": code_addr(m.start()),
                "file_off": f"0x{m.start():04x}",
                "imm": m.group("imm")[0],
                "what": SUB_IMM8_WHAT.get(m.start()),
            }
        )
    return sites


def classify_ax_debit_sites(blob: bytes, charge_map: dict, shop_rows: dict) -> list:
    """Every `sub [20ae:38c7],ax` in orig/g.exe, classified by ax's source.

    One row per SUB_AX_RE match, in file order -- so the row count is the
    number of `29 06 C7 38` occurrences in the binary, and a site cannot be
    dropped by belonging to no category: it would come out as
    `amount_form: "unrecognised"` instead, loudly.

    `charge_map` is parse_shops()'s {CHARGE_RE match position -> debited byte
    address}; `shop_rows` maps a shop price byte's address to its
    "<shop>:<key>" row key, so a site that debits a data/shops.json row says
    which row.
    """
    # CHARGE_RE / MUL_CHARGE_RE / CALL_CHARGE_RE all end with the four `sub`
    # bytes, so `match.end() - 4` is the position SUB_AX_RE reports.
    byte_form = {pos + len(b"\xA0\x00\x00\x30\xE4"): addr for pos, addr in charge_map.items()}
    mul_form = {
        m.end() - 4: (u16(m.group("addr")), u16(m.group("mul")))
        for m in MUL_CHARGE_RE.finditer(blob)
    }
    call_form = {
        m.end() - 4: (u16(m.group("seg")), u16(m.group("off")))
        for m in CALL_CHARGE_RE.finditer(blob)
    }

    sites = []
    for m in SUB_AX_RE.finditer(blob):
        pos = m.start()
        site = {
            "addr": code_addr(pos),
            "file_off": f"0x{pos:04x}",
            "amount_form": "unrecognised",
            "amount_addr": None,
            "category": "unrecognised",
            "shop_row": None,
            "recorded_in": None,
        }
        if pos in mul_form:
            addr, _mul = mul_form[pos]
            site["amount_form"] = "byte_times_imm16"
            site["amount_addr"] = f"20ae:{addr:04x}"
            site["category"] = "computed"
            site["recorded_in"] = "computed_sites"
        elif pos in byte_form:
            addr = byte_form[pos]
            site["amount_form"] = "byte"
            site["amount_addr"] = f"20ae:{addr:04x}"
            if addr in shop_rows:
                site["category"] = "shop_row"
                site["shop_row"] = shop_rows[addr]
                site["recorded_in"] = "data/shops.json"
            else:
                site["category"] = "variable"
                site["recorded_in"] = "var_sites"
        elif pos in call_form:
            site["amount_form"] = "call_result"
            site["category"] = "other"
            site["recorded_in"] = "other_ax_sites"
        sites.append(site)
    return sites


def find_var_sites(blob: bytes, ax_sites: list) -> list:
    """Full accounting of every byte variable a `sub [money],ax` site debits.

    For each such address this records every site that touches it, split by
    the idiom that touches it -- writes (`mov byte [addr],imm8` and `add byte
    [addr],imm8`), reads (`mov al,[addr]`) and compares (`cmp byte
    [addr],imm8`) -- plus `ref_count`, the number of times the address's two
    little-endian bytes occur anywhere in the file image. `ref_count` equal to
    the number of recorded sites is what says the account is complete rather
    than merely long: if some further idiom touched the variable, the counts
    would disagree. All re-derived from orig/g.exe, nothing transcribed.

    `charge_sites` is a *subset* of `read_sites` (the debit reads the variable
    with `mov al,[addr]` like any other read), so it is not added into
    `recorded_sites`.
    """
    out = []
    addrs = sorted(
        {
            int(s["amount_addr"].split(":")[1], 16)
            for s in ax_sites
            if s["category"] == "variable"
        }
    )
    for addr in addrs:
        raw = bytes([addr & 0xFF, addr >> 8])
        charge_sites = [
            {"addr": code_addr(s), "file_off": f"0x{s:04x}"}
            for s in sorted(
                int(x["file_off"], 16) - len(b"\xA0\x00\x00\x30\xE4")
                for x in ax_sites
                if x["amount_addr"] == f"20ae:{addr:04x}" and x["category"] == "variable"
            )
        ]
        write_sites = sorted(
            [
                (m.start(), "mov", m.group("imm")[0])
                for m in MOV_BYTE_IMM_RE.finditer(blob)
                if u16(m.group("addr")) == addr
            ]
            + [
                (m.start(), "add", m.group("imm")[0])
                for m in ADD_BYTE_IMM_RE.finditer(blob)
                if u16(m.group("addr")) == addr
            ]
        )
        read_sites = [
            m.start()
            for m in MOV_AL_ADDR_RE.finditer(blob)
            if u16(m.group("addr")) == addr
        ]
        compare_sites = [
            (m.start(), m.group("imm")[0])
            for m in CMP_BYTE_IMM_RE.finditer(blob)
            if u16(m.group("addr")) == addr
        ]
        out.append(
            {
                "addr": f"20ae:{addr:04x}",
                "scanned_idioms": {
                    "write_sites": "C6 06 <addr> ib (mov) / 80 06 <addr> ib (add)",
                    "read_sites": "A0 <addr> (mov al)",
                    "compare_sites": "80 3E <addr> ib (cmp byte)",
                    "charge_sites": "A0 <addr> 30 E4 29 06 C7 38 (subset of read_sites)",
                },
                "ref_count": blob.count(raw),
                "recorded_sites": len(write_sites) + len(read_sites) + len(compare_sites),
                "charge_sites": charge_sites,
                "write_sites": [
                    {
                        "addr": code_addr(s),
                        "file_off": f"0x{s:04x}",
                        "op": op,
                        "imm": imm,
                    }
                    for s, op, imm in write_sites
                ],
                "read_sites": [
                    {"addr": code_addr(s), "file_off": f"0x{s:04x}"} for s in read_sites
                ],
                "compare_sites": [
                    {"addr": code_addr(s), "file_off": f"0x{s:04x}", "imm": imm}
                    for s, imm in compare_sites
                ],
                "what": None,
            }
        )
    return out


def find_computed_sites(blob: bytes) -> list:
    """The `byte * imm16` debit form, scanned rather than hand-listed.

    `formula` and both addresses fall out of MUL_CHARGE_RE; only the `what`
    and the string cross-reference come from COMPUTED_WHAT.
    """
    out = []
    for m in MUL_CHARGE_RE.finditer(blob):
        pos = m.end() - 4
        addr = u16(m.group("addr"))
        mul = u16(m.group("mul"))
        name = VAR_NAMES.get(addr, f"byte[20ae:{addr:04x}]")
        row = {
            "addr": code_addr(pos),
            "file_off": f"0x{pos:04x}",
            "amount_addr": f"20ae:{addr:04x}",
            "multiplier": mul,
            "formula": f"{name}*{mul}",
            "what": None,
        }
        row.update(COMPUTED_WHAT.get(pos, {}))
        out.append(row)
    return out


def find_other_ax_sites(blob: bytes, ax_sites: list) -> list:
    """`sub [money],ax` sites that are neither an array/variable read nor a
    multiply -- currently one, whose amount is a far call's return value.

    Nothing is known about what it charges for, so `what` is null. It is
    recorded because it debits the player's money, not because its purpose is
    established.
    """
    call_form = {
        m.end() - 4: (u16(m.group("seg")), u16(m.group("off")))
        for m in CALL_CHARGE_RE.finditer(blob)
    }
    out = []
    for s in ax_sites:
        if s["category"] not in {"other", "unrecognised"}:
            continue
        pos = int(s["file_off"], 16)
        row = {
            "addr": s["addr"],
            "file_off": s["file_off"],
            "amount_form": s["amount_form"],
            "call_target": None,
            "what": None,
        }
        if pos in call_form:
            seg, off = call_form[pos]
            row["call_target"] = f"{seg:04x}:{off:04x}"
        out.append(row)
    return out


def build_other_price_sites(blob: bytes, charge_map: dict, shop_rows: dict) -> dict:
    ax_sites = classify_ax_debit_sites(blob, charge_map, shop_rows)
    counts = {}
    for s in ax_sites:
        counts[s["category"]] = counts.get(s["category"], 0) + 1
    return {
        "note": (
            "Every place in orig/g.exe that debits [20ae:38c7], the player's "
            "money. The player's money is subtracted by exactly two "
            "instruction encodings -- `29 06 C7 38` (sub [money],ax) and "
            "`83 2E C7 38 ib` (sub [money],imm8) -- and both are scanned over "
            "the whole file by tools/extract_tables.py, so this file is "
            "complete by construction rather than by inspection: "
            "ax_debit_sites lists every `ax` site there is, classified, and "
            "its count equals the sum of its categories. Eighteen of the `ax` "
            "sites are the mar/bmar rows already in data/shops.json and are "
            "listed here only as cross-reference; the sections below carry "
            "the detail for the rest. DERIVED from the binary: every address, "
            "file offset, immediate, multiplier, call target, formula and "
            "category. HAND-ANNOTATED (verified by reading the shortstrings "
            "at the cited file offsets out of orig/g.exe, see SUB_IMM8_WHAT "
            "and COMPUTED_WHAT in tools/extract_tables.py): the ten named "
            "imm8 `what` strings, and the computed row's `what`, `guard_addr` "
            "and `string_*` fields. Each imm8 annotation names the verb-"
            "dispatch span the site falls in and the token the nearest "
            "preceding `call 0f78:0bd8` compares; see docs/re/difftest.md. "
            "A null `what` means the purpose is not established -- one imm8 "
            "site (0x68fc, inside FUN_1000_3d11) and the one `other` ax site "
            "are still unidentified."
        ),
        "ax_debit_sites": {
            "idiom": "29 06 C7 38 -- sub word [20ae:38c7],ax",
            "count": len(ax_sites),
            "counts_by_category": dict(sorted(counts.items())),
            "sites": ax_sites,
        },
        "sub_word_imm8_sites": find_sub_imm8_sites(blob),
        "var_sites": find_var_sites(blob, ax_sites),
        "computed_sites": find_computed_sites(blob),
        "other_ax_sites": find_other_ax_sites(blob, ax_sites),
    }


# ---------------------------------------------------------------------------
# Runtime / provenance split.
#
# Each table's own parser above still produces one row per item that mixes
# what the game needs to play (name, price, stats...) with where in
# orig/g.exe that fact was read (an address, a file offset, a cross-check
# flag). The game has no business seeing the second kind -- see
# docs/re/tables.md, "Runtime vs. provenance". These three `split_*`
# functions are the only place that boundary is drawn; everything above them
# still deals in one combined row per item, the way the original scans work
# out most naturally.
#
# The provenance file for each table is a JSON object keyed by the row's
# natural key -- `id` for items and enemies, `"<shop>:<key>"` for shop rows,
# which have no `id` -- so any runtime row can be traced back to its bytes
# with a single lookup.


def split_items(items: list) -> tuple:
    runtime = []
    provenance = {}
    for it in items:
        runtime.append(
            {
                "id": it["id"],
                "name": it["name"],
                "kind": it["kind"],
                "bonus": it["bonus"],
                "effect": it["effect"],
                "price": it["price"],
                "sold": it["sold"],
            }
        )
        provenance[it["id"]] = {
            "price_src": it["price_src"],
            "src_off": it["src_off"],
        }
    return runtime, provenance


def split_shops(shops: list) -> tuple:
    runtime = []
    provenance = {}
    for row in shops:
        runtime.append(
            {
                "shop": row["shop"],
                "key": row["key"],
                "text": row["text"],
                "price": row["price"],
                "displayed_price": row["displayed_price"],
                "gate": row["gate"],
                "extra_gates": row["extra_gates"],
            }
        )
        key = f'{row["shop"]}:{row["key"]}'
        provenance[key] = {
            "price_addr": row["price_addr"],
            "displayed_price_addr": row["displayed_price_addr"],
            # See docs/re/tables.md, "The row idiom": `charged` records
            # whether *some* debit site in the binary reads `price_addr`,
            # i.e. it is a cross-check on the extraction, not a fact the
            # game reads to decide whether to charge -- every row is always
            # charged its `price` when bought.
            "charged": row["charged"],
            "code_off": row["code_off"],
            "code_addr": row["code_addr"],
            "prefix_off": row["prefix_off"],
            "text_off": row["text_off"],
        }
    return runtime, provenance


def split_enemies(enemies: list) -> tuple:
    runtime = []
    provenance = {}
    for e in enemies:
        runtime.append(
            {
                "id": e["id"],
                "name": e["name"],
                "class": e["class"],
                "level": e["level"],
                "stats": e["stats"],
                "growth_weights": e["growth_weights"],
                "generated": e["generated"],
            }
        )
        provenance[e["id"]] = {"source": e["source"]}
    return runtime, provenance


def write_json(path: pathlib.Path, payload) -> None:
    path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )


def main() -> None:
    blob = EXE.read_bytes()
    strings = json.loads(STRINGS.read_text(encoding="utf-8"))
    tables = json.loads(STRING_TABLES.read_text(encoding="utf-8"))
    ranks = next(t for t in tables["tables"] if t["name"] == "ranks")["entries"]

    shops, charge_map = parse_shops(blob)
    items = parse_items(strings)
    link_item_prices(items, shops)
    enemies = parse_enemies(blob, ranks)

    items_runtime, items_prov = split_items(items)
    shops_runtime, shops_prov = split_shops(shops)
    enemies_runtime, enemies_prov = split_enemies(enemies)

    # addr -> "<shop>:<key>", so a debit site that charges a data/shops.json
    # row can name the row instead of just its price byte.
    shop_rows = {
        int(r["price_addr"].split(":")[1], 16): f'{r["shop"]}:{r["key"]}'
        for r in shops
    }
    other_prices = build_other_price_sites(blob, charge_map, shop_rows)

    write_json(OUT_ITEMS, items_runtime)
    write_json(OUT_ITEMS_PROV, items_prov)
    write_json(OUT_SHOPS, shops_runtime)
    write_json(OUT_SHOPS_PROV, shops_prov)
    write_json(OUT_ENEMIES, enemies_runtime)
    write_json(OUT_ENEMIES_PROV, enemies_prov)
    write_json(OUT_OTHER_PRICES, other_prices)
    print(
        f"wrote {len(items_runtime)} items, {len(shops_runtime)} shop rows, "
        f"{len(enemies_runtime)} enemy rows (+3 provenance files, "
        f"+data/other_price_sites.json)"
    )


if __name__ == "__main__":
    main()
