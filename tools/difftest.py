#!/usr/bin/env python3
"""Differential test: the Rust port's authored constants against `orig/g.exe`.

## What this compares, and what it deliberately does not

The port already replays the original's `Random` stream draw for draw
(`data/rng_trace.json`, 1387 captured draws) and its per-turn guest state
(`data/state_trace.json`, 91 samples of 35 variables).  `tests/wander_sequence.rs`
asserts both, and both capture files came out of the original under a qemu+gdb
tracer.  Every number the game *computes* is covered there, and covered more
strongly than any screen scrape could: the draw oracle sits upstream of
everything that gets printed.

What neither oracle observes is the numbers the game was *authored* with -- a
shop price, a level threshold, a class's opening stat line -- because a
captured run does not have to exercise them.  This tool is exactly that
residue:

  * every priced menu row: its key, its charged price, its displayed price
    and its text, for all five priced locations including `kl` and `trn`,
  * the XP threshold curve and the two immediates that generate it,
  * what one level-up stat grant moves, and by how much,
  * the class weight table and the four character-creation stat lines,
  * item bonuses,
  * menu numbering and row order.

## The two sides are read through different code

The reference side of every comparison below is read **out of
`orig/g.exe`, by this file**: it opens the image, scans for the instruction
shapes the values live in, and reads the bytes.  It does not import
`tools/extract_tables.py`, and it does not read `data/shops.json`,
`data/items.json` or `data/xp.json` -- those are what the port was *built*
from (`build.rs` bakes them in), so reading them here would compare the port
against its own input and could not fail.  The port side is the record stream
`gopnik --trace-deterministic` prints.

That still leaves a real limit, and it is stated rather than implied: this
file and `tools/extract_tables.py` both read the same bytes, so a *shared
misreading* of what an instruction means would agree on both sides.  What the
comparison does catch is a stale artifact, a wrong `build.rs` mapping, a
transcription slip, and any later edit to the port's constants.

`--oracle` adds a third, weaker channel: five keystroke scripts run against
the original under the Task 3 DOSBox-X harness, with the numbers read off the
screens the original itself printed.  Per `docs/re/METHODOLOGY.md` that is
output-tier evidence -- it can falsify a price, never establish one -- and it
reaches only the part of the menu a scripted run can get to.  The exact count
it confirms is printed, never rounded up to "all".

Address convention: `tools/addr.py` (`docs/re/METHODOLOGY.md`).  Instruction
boundaries: `tools/dis16.py`.  Standard library only.

    python3 tools/difftest.py                 # compare port vs orig/g.exe
    python3 tools/difftest.py --dump          # print the reference stream
    python3 tools/difftest.py --oracle        # add the DOSBox-X screen channel
"""
import argparse
import pathlib
import re
import struct
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))

import addr  # noqa: E402
import dis16  # noqa: E402

SCRIPTS = ROOT / "data" / "difftest_scripts"

# ---------------------------------------------------------------------------
# Addresses this file quotes rather than derives.  Each one is checked against
# the bytes at it before it is used, so a stale citation fails loudly instead
# of reading the wrong number.
# ---------------------------------------------------------------------------

#: `mov word [20ae:38d0],0xa` -- a new character's XP threshold.
THRESHOLD_BASE_SITE = ("1000:6de0", bytes.fromhex("c706d038"))
#: `add word [20ae:38d0],0xa` -- one level's worth of threshold.
THRESHOLD_STEP_SITE = ("1000:2550", bytes.fromhex("8306d038"))
#: `cmp word [20ae:38a6],0x28` -- the level cap, tested when `param_1` is 0.
MAX_LEVEL_SITE = ("1000:2580", bytes.fromhex("833ea638"))
#: `cmp word [bp-8],2` -- the stat-grant loop bound, i.e. gains per level.
GAINS_SITE = ("1000:287d", bytes.fromhex("837ef8"))
#: `add di,0x2e` -- the rank-name table's base, which bounds the weight table.
RANK_TABLE_SITE = ("1000:1a3e", bytes.fromhex("81c7"))
#: `add word [20ae:389c],3` -- class = answer + 3 at character creation.
CLASS_OFFSET_SITE = ("1000:71b8", bytes.fromhex("83069c38"))
#: `mov ax,0xa` -- the value pushed into `trn` row 3's `#`.
TRN3_FILL_SITE = ("1000:e505", bytes.fromhex("b8"))

#: The player's fighter record (`docs/re/progression.md`, "The state"), by the
#: absolute `20ae:` address of each field -- which is what a `[disp16]` operand
#: in the image carries.  Only the fields a level-up grant can move are here;
#: the record base is `20ae:389c`, so `strength` is `+0x02` and so on.
RECORD_FIELDS = {
    0x389E: "strength",
    0x38A0: "agility",
    0x38A2: "vitality",
    0x38A4: "luck",
    0x38A8: "dmg_min",
    0x38AA: "dmg_max",
    0x38AC: "hp",
    0x38AE: "hpmax",
}

#: Stat order of the four range tests in `FUN_1000_2526`, which is the order
#: of the four weight-table columns (`1000:25aa`..`1000:25ee`).
STAT_ORDER = ["strength", "agility", "vitality", "luck"]


class DifftestError(RuntimeError):
    pass


# ---------------------------------------------------------------------------
# Image access
# ---------------------------------------------------------------------------

def load():
    """The load image, with `tools/addr.py`'s landmarks re-checked first."""
    addr.check_image()
    return addr.load_image(addr.read_exe())


def shortstring(img, image_off):
    """A Turbo Pascal `string[n]` at an image offset, decoded from CP866."""
    n = img[image_off]
    return img[image_off + 1 : image_off + 1 + n].decode("cp866")


def strip_markup(text):
    """Drop `^0`..`^7`.  Colour markup is not content and is never compared."""
    out, i = [], 0
    while i < len(text):
        if text[i] == "^" and i + 1 < len(text) and text[i + 1].isdigit():
            i += 2
            continue
        out.append(text[i])
        i += 1
    return "".join(out)


def at(img, image_off, want, what):
    """Check the bytes at a quoted address, then hand back the offset."""
    got = img[image_off : image_off + len(want)]
    if got != want:
        raise DifftestError(
            "%s: 1000:%04x holds %s, expected it to start %s"
            % (what, image_off, got.hex(" "), want.hex(" "))
        )
    return image_off


def site(img, citation_and_bytes, what):
    citation, want = citation_and_bytes
    off = addr.image_off_of_citation(citation)
    return at(img, off, want, "%s (%s)" % (what, citation))


# ---------------------------------------------------------------------------
# The verb-dispatch chain: which handler an address belongs to
# ---------------------------------------------------------------------------

#: `push ds:0x3972` / `push cs:<token>` / `call 0f78:0bd8` -- one link of the
#: street verb chain.  Restricting to `DS:3972` is what keeps the
#: encounter/sub-prompt variable `DS:3a72` out (`docs/re/command-dispatch.md`).
DISPATCH_RE = re.compile(
    rb"\xbf\x72\x39\x1e\x57\xbf(..)\x0e\x57\x9a\xd8\x0b\x78\x0f", re.S
)


def dispatch_chain(img):
    """`[(compare_off, verb), ...]` for every link, in address order."""
    out = []
    for m in DISPATCH_RE.finditer(img):
        token = shortstring(img, struct.unpack("<H", m.group(1))[0])
        out.append((m.start() + 10, token))
    if not out:
        raise DifftestError("no verb-dispatch links found")
    return out


def handler_span(chain, verb):
    """`(start, stop)` -- from `verb`'s compare to the next verb's."""
    for i, (off, tok) in enumerate(chain):
        if tok == verb:
            stop = chain[i + 1][0] if i + 1 < len(chain) else None
            return off, stop
    raise DifftestError("verb %r is not in the dispatch chain" % verb)


def shop_of(chain, image_off, shops):
    for verb in shops:
        lo, hi = handler_span(chain, verb)
        if lo <= image_off and (hi is None or image_off < hi):
            return verb
    return None


# ---------------------------------------------------------------------------
# Priced menu rows
# ---------------------------------------------------------------------------
#
# Every priced row in the image is assembled by the same fixed instruction
# run: test affordability, set the colour digit `20ae:3b7a` to '0' or '4',
# copy the row's prefix shortstring into a scratch string, append the digit,
# write it, write the row's own text, then `Write` the value that fills the
# text's `#`.  The two families below differ only in how the price reaches the
# test -- out of the `20ae:0b2e` byte array, or as an instruction immediate.

_TAIL = (
    rb"\x8d\xbe\x00\xfe\x16\x57\xbf(..)\x0e\x57"          # prefix string
    rb"\x9a\xe7\x0a\x78\x0f"                              # assign
    rb"\x8d\xbe\x00\xff\x16\x57\xa0\x7a\x3b\x50"          # + the colour digit
    rb"\x9a\x03\x0c\x78\x0f\x9a\x66\x0b\x78\x0f"          # concat, write
    rb"\xbf(..)\x0e\x57\x9a\x66\x0b\x78\x0f"              # row text, write
)

#: `mov al,[20ae:0bNN]` / `xor ah,ah` / `cmp ax,[20ae:38c7]` / `jle`.
BYTE_ROW_RE = re.compile(
    rb"\xa0(.)\x0b\x30\xe4\x3b\x06\xc7\x38\x7e\x07"
    rb"\xc6\x06\x7a\x3b\x34\xeb\x05\xc6\x06\x7a\x3b\x30" + _TAIL + rb"\xa0(.)\x0b\x30\xe4\x50",
    re.S,
)

#: `cmp word [20ae:38c7],imm8` / `jl`|`jge` -- the price is the immediate.
IMM_ROW_RE = re.compile(
    rb"\x83\x3e\xc7\x38(.)[\x7c\x7d]\x07"
    rb"\xc6\x06\x7a\x3b.\xeb\x05\xc6\x06\x7a\x3b." + _TAIL,
    re.S,
)


def row_key(prefix):
    """The key the player types, read off the row's own prefix string.

    `^61^7 - ^` -> `1`, ` 3 -  ^` -> `3`, `  ^2h^7 - за ^` -> `h`.  One rule
    for all three shapes: drop the markup, take the first whitespace-separated
    token.  Deriving the number from the prefix rather than from the row's
    position is what makes menu numbering a compared quantity instead of an
    assumption.
    """
    fields = strip_markup(prefix).split()
    if not fields:
        raise DifftestError("row prefix %r carries no key" % prefix)
    return fields[0]


def priced_rows(img, chain):
    """Every priced menu row in the image, keyed by `(shop, key)`.

    Returns `(rows, byte_hits, imm_hits)` so the caller can report how many
    rows each scan found rather than just how many survived.
    """
    rows = []
    byte_shops = ("mar", "bmar")
    imm_shops = ("rep", "kl", "trn")

    byte_hits = list(BYTE_ROW_RE.finditer(img))
    for m in byte_hits:
        price_off = addr.image_off_of_data_off(0x0B00 | m.group(1)[0])
        disp_off = addr.image_off_of_data_off(0x0B00 | m.group(4)[0])
        prefix = shortstring(img, struct.unpack("<H", m.group(2))[0])
        text = shortstring(img, struct.unpack("<H", m.group(3))[0])
        shop = shop_of(chain, m.start(), byte_shops)
        if shop is None:
            raise DifftestError(
                "byte-priced row at 1000:%04x is in no known handler" % m.start()
            )
        rows.append(
            {
                "shop": shop,
                "key": row_key(prefix),
                "price": img[price_off],
                "displayed": img[disp_off],
                "text": text,
                "site": "1000:%04x" % m.start(),
            }
        )

    imm_hits = list(IMM_ROW_RE.finditer(img))
    for m in imm_hits:
        price = m.group(1)[0]
        prefix = shortstring(img, struct.unpack("<H", m.group(2))[0])
        text = shortstring(img, struct.unpack("<H", m.group(3))[0])
        shop = shop_of(chain, m.start(), imm_shops)
        if shop is None:
            raise DifftestError(
                "immediate-priced row at 1000:%04x is in no known handler" % m.start()
            )
        # These nine carry their price in the text as literal digits instead
        # of a `#`.  Checking that the digits equal the immediate is a real
        # cross-check: the two live in different places in the file.
        lead = re.match(r"(\d+)", strip_markup(text))
        if not lead or int(lead.group(1)) != price:
            raise DifftestError(
                "row at 1000:%04x charges %d but its text reads %r"
                % (m.start(), price, strip_markup(text))
            )
        rows.append(
            {
                "shop": shop,
                "key": row_key(prefix),
                "price": price,
                "displayed": price,
                "text": text,
                "site": "1000:%04x" % m.start(),
            }
        )

    return rows, len(byte_hits), len(imm_hits)


# ---------------------------------------------------------------------------
# What the purchase side actually debits
# ---------------------------------------------------------------------------

#: `mov al,[20ae:0bNN]` ... `sub word [20ae:38c7],ax` -- the byte-priced debit.
AX_DEBIT_RE = re.compile(rb"\xa0(.)\x0b\x30\xe4.{0,24}?\x29\x06\xc7\x38", re.S)
#: `sub word [20ae:38c7],imm8` -- the immediate-priced debit.
IMM_DEBIT_RE = re.compile(rb"\x83\x2e\xc7\x38(.)", re.S)
#: `push cs:<token>` / `call 0f78:0bd8` -- any string compare, dispatch chain
#: or submenu.  The nearest one before a debit names the key that reaches it.
COMPARE_RE = re.compile(rb"\xbf(..)\x0e\x57\x9a\xd8\x0b\x78\x0f", re.S)


def key_compares(img):
    """`[(offset, token), ...]` for every string compare, in address order."""
    return [
        (m.start(), shortstring(img, struct.unpack("<H", m.group(1))[0]))
        for m in COMPARE_RE.finditer(img)
    ]


def debits(img, chain, shops):
    """`{(shop, key): amount}` -- what each purchase arm actually takes.

    The menu print writes a price down; this reads what the buy path takes.
    They are separate instructions in separate parts of the handler, so
    comparing them **per row** catches a row that quotes one number and
    charges another -- which the image really does contain (`bmar` row 9
    shows 70 and takes 60).  A per-shop multiset would not: 70 and 60 both
    appear on either side of that shop's list whichever row owns which.

    Each debit is attributed to the row whose key the nearest preceding
    string compare tests, so the pairing is read off the code rather than
    assumed from the order the arms happen to be laid out in -- the vet's two
    arms are in the opposite order to its two menu rows.
    """
    compares = key_compares(img)
    out = {}

    def key_before(off):
        best = None
        for c_off, token in compares:
            if c_off < off and (best is None or c_off > best[0]):
                best = (c_off, token)
        return None if best is None else best[1]

    def record(off, amount):
        shop = shop_of(chain, off, shops)
        if shop is None:
            return
        key = key_before(off)
        if key is None:
            raise DifftestError("debit at 1000:%04x has no key compare before it" % off)
        slot = (shop, key)
        if slot in out:
            raise DifftestError("two debits attributed to %s row %s" % slot)
        out[slot] = amount

    for m in AX_DEBIT_RE.finditer(img):
        record(m.start(), img[addr.image_off_of_data_off(0x0B00 | m.group(1)[0])])
    for m in IMM_DEBIT_RE.finditer(img):
        record(m.start(), m.group(1)[0])
    return out


# ---------------------------------------------------------------------------
# Progression constants
# ---------------------------------------------------------------------------

def scalars(img):
    base = img[site(img, THRESHOLD_BASE_SITE, "threshold base") + 4]
    step = img[site(img, THRESHOLD_STEP_SITE, "threshold step") + 4]
    cap = img[site(img, MAX_LEVEL_SITE, "level cap") + 4]
    gains = img[site(img, GAINS_SITE, "gains per level") + 3]
    return {
        "threshold_base": base,
        "threshold_step": step,
        "max_level": cap,
        "gains_per_level": gains,
    }


def class_weights(img):
    """The growth-weight table, four bytes per class, at `20ae:0002`.

    Its extent is not assumed: `1000:25aa` indexes it at `[class*4 + 2]`, and
    the rank-name table that the same index selects starts at `20ae:002e`
    (`1000:1a3e` `add di,0x2e`), so the weights occupy `0x02..0x2e` -- eleven
    rows exactly, with no room for a twelfth.
    """
    rank_base = struct.unpack_from(
        "<H", img, site(img, RANK_TABLE_SITE, "rank-name table base") + 2
    )[0]
    span = rank_base - 2
    if span <= 0 or span % 4:
        raise DifftestError(
            "weight table spans %d bytes, which is not a whole number of rows" % span
        )
    base = addr.image_off_of_data_off(2)
    return [list(img[base + i * 4 : base + i * 4 + 4]) for i in range(span // 4)]


#: The four `mov word [20ae:389e|38a0|38a2|38a4],imm16` stores of one
#: character-creation arm, in strength/agility/vitality/luck order.
CREATION_RE = re.compile(
    rb"\xc7\x06\x9e\x38(.)\x00\xc7\x06\xa0\x38(.)\x00"
    rb"\xc7\x06\xa2\x38(.)\x00\xc7\x06\xa4\x38(.)\x00",
    re.S,
)


def start_stats(img):
    """`{answer: (class, [str, agi, vit, luck])}` for the four class choices.

    Three arms are reached by `cmp ax,N` / `jne` on the typed answer; the
    fourth is the else, which `1000:712d`'s clamp folds every other answer
    (including 0) into.  The stored class is the answer plus the immediate at
    `1000:71b8`.
    """
    offset = img[site(img, CLASS_OFFSET_SITE, "class offset") + 4]
    out = {}
    for m in CREATION_RE.finditer(img):
        stats = [g[0] for g in m.groups()]
        back = img[m.start() - 5 : m.start()]
        if back[0] == 0x3D and back[3] == 0x75:  # cmp ax,imm16 / jne
            answer = struct.unpack("<H", back[1:3])[0]
        else:
            answer = 0
        if answer in out:
            raise DifftestError("two creation arms both answer %d" % answer)
        out[answer] = (answer + offset, stats)
    if sorted(out) != [0, 1, 2, 3]:
        raise DifftestError("creation arms cover %s, expected 0..3" % sorted(out))
    return out


#: `cmp ax,[bp-0xa]` -- one of the four prefix-sum range tests that pick which
#: stat a `Random(sum)+1` draw grants.
RANGE_TEST_RE = re.compile(rb"\x3b\x46\xf6", re.S)


def levelup_gains(img):
    """What each of the four stat grants moves, read out of `FUN_1000_2526`.

    The loop-counter compare `cmp word [bp-8],2` ends the last arm and every
    arm jumps to it, so the four spans are `[grant, first jump to that
    compare]`.  Inside a span every `inc word [rec]` and `add word [rec],imm`
    on the player's record is collected, and an instruction a forward
    conditional jump skips is marked conditional -- which is how strength's
    `dmg_min + 1` (guarded by `1000:268f`, "the new strength is even") is
    separated from the four unconditional ones without this file restating
    the predicate.
    """
    tail = site(img, GAINS_SITE, "gains per level")
    tests = [m.start() for m in RANGE_TEST_RE.finditer(img) if 0x2526 < m.start() < tail]
    if len(tests) != 4:
        raise DifftestError(
            "found %d range tests in FUN_1000_2526, expected 4" % len(tests)
        )

    starts = []
    for t in tests:
        jcc = img[t + 3]
        rel = img[t + 4] - (0x100 if img[t + 4] >= 0x80 else 0)
        if jcc == 0x7D:      # jge: the grant is the jump target
            starts.append(t + 5 + rel)
        elif jcc == 0x7C:    # jl: the grant is the fall-through
            starts.append(t + 5)
        else:
            raise DifftestError(
                "range test at 1000:%04x is followed by %02x, not a jl/jge" % (t, jcc)
            )

    jumps_to_tail = []
    for off in range(min(starts), tail + 1):
        if img[off] == 0xE9:
            rel = struct.unpack_from("<h", img, off + 1)[0]
            if off + 3 + rel == tail:
                jumps_to_tail.append(off)
        elif img[off] == 0xEB:
            rel = img[off + 1] - (0x100 if img[off + 1] >= 0x80 else 0)
            if off + 2 + rel == tail:
                jumps_to_tail.append(off)

    out = {}
    for stat, start in zip(STAT_ORDER, starts):
        stop = next((j for j in jumps_to_tail if j >= start), None)
        if stop is None:
            raise DifftestError("%s's grant arm never reaches the loop tail" % stat)
        gains, skip_until = [], None
        for insn in dis16.decode_run(img, start, stop):
            if skip_until is not None and insn.off >= skip_until:
                skip_until = None
            if 0x70 <= insn.raw[0] <= 0x7F:
                rel = insn.raw[1] - (0x100 if insn.raw[1] >= 0x80 else 0)
                target = insn.end + rel
                if insn.end < target <= stop:
                    skip_until = target
                continue
            field = None
            if insn.raw[:2] == b"\xff\x06":
                field, delta = RECORD_FIELDS.get(struct.unpack_from("<H", insn.raw, 2)[0]), 1
            elif insn.raw[:2] == b"\x83\x06":
                field = RECORD_FIELDS.get(struct.unpack_from("<H", insn.raw, 2)[0])
                delta = insn.raw[4]
            if field is not None:
                gains.append((field, delta, skip_until is not None))
        out[stat] = sorted(gains)
    return out


# ---------------------------------------------------------------------------
# Items
# ---------------------------------------------------------------------------
#
# Item bonuses are not in a table: each item's own inventory line spells its
# bonus out (`^1Тесак(Урон+9) `).  The scan below walks the whole image for a
# Pascal shortstring that starts with `^1` and matches one of those forms, so
# it depends on no artifact and on no earlier extraction pass.

ITEM_PATTERNS = [
    re.compile(r"^(?P<name>.+?)\(Урон\s*\+(?P<bonus>\d+)\)"),
    re.compile(r"^(?P<name>.+?)\(Удача\s*\+(?P<bonus>\d+)\)"),
    re.compile(r"^(?P<name>.+?)\(Всё\s*\+(?P<bonus>\d+)\)"),
    re.compile(r"^(?P<name>.+?)\(Самолечение\)"),
    re.compile(r"^(?P<name>.+?)\(\+(?P<bonus>\d+)\)"),
]


def items(img):
    """`[(image_off, name, bonus), ...]`, in image order."""
    out = []
    for off in range(len(img) - 2):
        n = img[off]
        if n < 5 or off + 1 + n > len(img):
            continue
        if img[off + 1 : off + 3] != b"\x5e\x31":  # '^1'
            continue
        try:
            text = img[off + 1 : off + 1 + n].decode("cp866")
        except UnicodeDecodeError:
            continue
        plain = strip_markup(text).strip()
        for rx in ITEM_PATTERNS:
            m = rx.match(plain)
            if not m:
                continue
            bonus = int(m.groupdict().get("bonus") or 0)
            out.append((off, m.group("name").strip(), bonus))
            break
    return out


# ---------------------------------------------------------------------------
# The reference stream
# ---------------------------------------------------------------------------

def reference(img):
    """The record stream, built from an `orig/g.exe` load image alone."""
    chain = dispatch_chain(img)
    lines = []
    ev = {}

    sc = scalars(img)
    for name in ("max_level", "gains_per_level", "threshold_base", "threshold_step"):
        lines.append("scalar %s %d" % (name, sc[name]))
    for level in range(sc["max_level"] + 1):
        lines.append(
            "xp_threshold %d %d" % (level, sc["threshold_base"] + sc["threshold_step"] * level)
        )
    ev["xp_threshold"] = sc["max_level"] + 1

    weights = class_weights(img)
    for i, w in enumerate(weights):
        lines.append("class_weights %d %d %d %d %d" % (i, *w))
    ev["class_weights"] = len(weights)

    starts = start_stats(img)
    for answer in sorted(starts):
        cls, stats = starts[answer]
        lines.append("start_stats %d %d %d %d %d %d" % (answer, cls, *stats))
    ev["start_stats"] = len(starts)

    gains = levelup_gains(img)
    n_gains = 0
    for stat in STAT_ORDER:
        for field, delta, conditional in gains[stat]:
            lines.append(
                "levelup_gain %s %s %d %s"
                % (stat, field, delta, "conditional" if conditional else "always")
            )
            n_gains += 1
    ev["levelup_gain"] = n_gains

    found = items(img)
    for _, name, bonus in found:
        lines.append("item %d %s" % (bonus, name))
    ev["item"] = len(found)

    rows, byte_hits, imm_hits = priced_rows(img, chain)
    ev["priced_row"] = len(rows)
    ev["priced_row_byte"] = byte_hits
    ev["priced_row_imm"] = imm_hits
    shops = ["mar", "bmar", "rep", "kl", "trn"]
    by_shop = {s: [r for r in rows if r["shop"] == s] for s in shops}
    for s in shops:
        for r in by_shop[s]:
            lines.append(
                "priced_row %s %s %d %d %s"
                % (s, r["key"], r["price"], r["displayed"], strip_markup(r["text"]))
            )
    for s in shops:
        lines.append("menu_order %s %s" % (s, ",".join(r["key"] for r in by_shop[s])))
    ev["menu_order"] = len(shops)

    # The address each immediate-priced row's price is written down at.  The
    # port carries the same nine as citations on `src/game.rs`'s `IMM_ROWS`;
    # emitting them turns a quoted address into a compared one.
    n_sites = 0
    for s in ("rep", "kl", "trn"):
        for r in by_shop[s]:
            lines.append("imm_row_site %s %s %s" % (s, r["key"], r["site"]))
            n_sites += 1
    ev["imm_row_site"] = n_sites

    # Cross-check: what the buy path debits, row by row, against what the
    # menu tests.  Reported, not silently folded into the stream.
    charged = debits(img, chain, shops)
    ev["debit_rows"] = len(charged)
    ev["debit_mismatch"] = []
    for r in rows:
        slot = (r["shop"], r["key"])
        got = charged.pop(slot, None)
        if got != r["price"]:
            ev["debit_mismatch"].append((slot, r["price"], got))
    ev["debit_unmatched"] = sorted(charged)
    # The one row in the image whose quoted price is not the price it tests
    # and takes (`bmar` 9: text says 70, `1000:c832` tests 60).
    ev["quote_gap"] = [
        (r["shop"], r["key"], r["displayed"], r["price"])
        for r in rows
        if r["displayed"] != r["price"]
    ]

    # `trn` row 3 is the only row whose text keeps a `#`; the value pushed
    # into it is its own immediate, at a different address from the price.
    ev["trn3_fill"] = struct.unpack_from(
        "<H", img, site(img, TRN3_FILL_SITE, "trn row 3 fill") + 1
    )[0]
    return lines, ev


# ---------------------------------------------------------------------------
# The port
# ---------------------------------------------------------------------------

#: Everything the record stream is compiled from.  A binary older than any of
#: these is stale, and comparing a stale binary is the exact shape of check
#: that cannot fail: the edit under test is simply not in it.
PORT_INPUTS = ["src", "build.rs", "Cargo.toml", "data/items.json",
               "data/shops.json", "data/enemies.json"]


def build_port():
    """`cargo build --release`, always, before the binary is read.

    Not a convenience.  Comparing a binary older than the source that was
    edited is a check that cannot fail -- the change under test is simply not
    in it -- and it is easy to hit by accident, because several of this repo's
    other tools rewrite `data/*.json` and so move the port's inputs.  Building
    first costs nothing when nothing changed.
    """
    r = subprocess.run(
        ["cargo", "build", "--release"],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        timeout=600,
    )
    if r.returncode != 0:
        raise DifftestError("cargo build --release failed:\n%s" % r.stderr.strip())


def port_binary():
    build_port()
    p = ROOT / "target" / "release" / "gopnik"
    if not p.exists():
        raise DifftestError(
            "cargo build --release succeeded but %s is missing" % p.relative_to(ROOT)
        )
    return check_fresh(p)


def check_fresh(binary):
    built = binary.stat().st_mtime
    newer = []
    for name in PORT_INPUTS:
        path = ROOT / name
        if path.is_dir():
            candidates = [p for p in path.rglob("*") if p.is_file()]
        else:
            candidates = [path] if path.exists() else []
        newer += [p for p in candidates if p.stat().st_mtime > built]
    if newer:
        raise DifftestError(
            "%s is older than %s -- run `cargo build --release` first, or this "
            "compares a binary that does not contain the change under test"
            % (binary.relative_to(ROOT), ", ".join(sorted(
                str(p.relative_to(ROOT)) for p in newer)[:5]))
        )
    return binary


def port_stream():
    r = subprocess.run(
        [str(port_binary()), "--trace-deterministic"],
        capture_output=True,
        text=True,
        timeout=60,
    )
    if r.returncode != 0:
        raise DifftestError(
            "gopnik --trace-deterministic exited %d: %s" % (r.returncode, r.stderr.strip())
        )
    return r.stdout.splitlines()


# ---------------------------------------------------------------------------
# Comparison
# ---------------------------------------------------------------------------

def kind_of(line):
    return line.split(" ", 1)[0] if line else ""


def compare(ref, port):
    """`(ok, report_lines, per_kind_counts)`.

    Records are compared as ordered lists, so an extra, missing or reordered
    record is a difference like any other.
    """
    report = []
    counts = {}
    for line in ref:
        counts.setdefault(kind_of(line), [0, 0])[0] += 1
    for line in port:
        counts.setdefault(kind_of(line), [0, 0])[1] += 1

    ok = True
    for i, (a, b) in enumerate(zip(ref, port)):
        if a != b:
            lo = max(0, i - 3)
            report.append("FAIL record %d differs" % i)
            report.append("  orig: %s" % a)
            report.append("  port: %s" % b)
            report.append("  context (orig): %s" % ref[lo:i])
            ok = False
    if len(ref) != len(port):
        report.append(
            "FAIL record count: orig %d, port %d" % (len(ref), len(port))
        )
        extra = ref[len(port) :] or port[len(ref) :]
        report.append("  first unmatched: %s" % (extra[:3],))
        ok = False
    return ok, report, counts


# ---------------------------------------------------------------------------
# The optional DOSBox-X screen channel
# ---------------------------------------------------------------------------

#: `1 - 2 руб.  Хотдог(3-4 з)` as the original prints it, markup already gone.
SCREEN_ROW_RE = re.compile(r"^\s*(\d)\s+-\s+(\d+)\s+руб", re.M)
#: `Сл:5 Лв:2 Жв:4 Уд:1` on the `s` screen.
SCREEN_STATS_RE = re.compile(r"Сл:(\d+)\s+Лв:(\d+)\s+Жв:(\d+)\s+Уд:(\d+)")
#: `Сейчас у тебя 0 опыта, А для прокачки надо 10`
SCREEN_THRESHOLD_RE = re.compile(r"для прокачки надо (\d+)")


def run_oracle(script, out_dir):
    sys.path.insert(0, str(HERE / "oracle"))
    import capture  # noqa: E402  (imported late: it needs dosbox-x)

    keys = script.read_text(encoding="utf-8")
    return capture.run(keys, out_dir, seed=1)


def oracle_channel(ref_lines, scratch):
    """Confirm on the original's own screens what a scripted run can reach.

    Returns `(ok, report, confirmed, unreached)`.  `confirmed` counts the
    reference values a screen actually showed; `unreached` says, in as many
    words, which ones no script here can get to.  Neither number is rounded
    up to "all" -- see `docs/re/METHODOLOGY.md`: output can falsify a price,
    never establish one, and a menu a script cannot open confirms nothing.
    """
    report = []
    ok = True
    confirmed = 0

    ref_rows = {}
    for line in ref_lines:
        if line.startswith("priced_row mar "):
            _, _, key, price, disp, _ = line.split(" ", 5)
            ref_rows[key] = int(disp)
    ref_stats = {}
    for line in ref_lines:
        if line.startswith("start_stats "):
            f = line.split()
            ref_stats[int(f[1])] = [int(x) for x in f[3:7]]
    ref_threshold = next(
        int(l.split()[2]) for l in ref_lines if l.startswith("scalar threshold_base ")
    )
    other_rows = sum(1 for l in ref_lines if l.startswith("priced_row ")) - len(ref_rows)

    scripts = sorted(SCRIPTS.glob("*.txt"))
    if len(scripts) < 5:
        raise DifftestError("expected at least 5 scripts, found %d" % len(scripts))

    rows_seen_keys = set()
    stats_seen = 0
    thresholds_seen = 0
    for script in scripts:
        frames = run_oracle(script, scratch / script.stem)
        screen = "\n".join(frames)
        if script.stem == "market_rows_district1":
            seen = {k: int(v) for k, v in SCREEN_ROW_RE.findall(screen)}
            if not seen:
                report.append("FAIL %s: no priced row on any of the %d frames"
                              % (script.name, len(frames)))
                ok = False
            for key, price in sorted(seen.items()):
                want = ref_rows.get(key)
                if want != price:
                    report.append(
                        "FAIL %s: row %s shows %d, orig/g.exe says %s"
                        % (script.name, key, price, want)
                    )
                    ok = False
                else:
                    confirmed += 1
                    rows_seen_keys.add(key)
            report.append(
                "OK   %s: mar rows %s read off the screen"
                % (script.name, ",".join(sorted(seen)))
            )
        elif script.stem.startswith("stats_class"):
            answer = int(script.stem[len("stats_class"):])
            m = SCREEN_STATS_RE.search(screen)
            if not m:
                report.append("FAIL %s: no stat line on any of the %d frames"
                              % (script.name, len(frames)))
                ok = False
                continue
            got = [int(g) for g in m.groups()]
            want = ref_stats[answer]
            if got != want:
                report.append(
                    "FAIL %s: screen shows %s, orig/g.exe says %s"
                    % (script.name, got, want)
                )
                ok = False
            else:
                confirmed += 1
                stats_seen += 1
                report.append("OK   %s: starting stats %s" % (script.name, got))
            t = SCREEN_THRESHOLD_RE.search(screen)
            if not t:
                report.append("FAIL %s: no threshold line on any frame" % script.name)
                ok = False
            elif int(t.group(1)) != ref_threshold:
                report.append(
                    "FAIL %s: threshold on screen is %s, orig/g.exe says %d"
                    % (script.name, t.group(1), ref_threshold)
                )
                ok = False
            else:
                confirmed += 1
                thresholds_seen += 1
        else:
            report.append("SKIP %s: no reader for this script" % script.name)
            ok = False

    missing_rows = sorted(set(ref_rows) - rows_seen_keys)
    unreached = [
        "mar rows %s: their own `district > 1` test (1000:bb80, 1000:bc42, "
        "1000:bca5) keeps them off a district-1 screen"
        % (",".join(missing_rows) or "(none)"),
        "the other %d priced rows (bmar, rep, kl, trn): each location refuses "
        "entry until its discovery flag at 20ae:3694..369a is set, and the vet "
        "prints its two rows only to a hurt character (1000:d3d3). None of the "
        "five scripts here opens those menus; whether some longer script could "
        "is not settled by this run" % other_rows,
        "every xp_threshold above the first, class_weights, item bonuses and "
        "levelup_gain: no screen prints them as such",
    ]
    return ok, report, {
        "confirmed": confirmed,
        "mar_rows": (len(rows_seen_keys), len(ref_rows)),
        "start_stats": (stats_seen, len(ref_stats)),
        "threshold_sightings": thresholds_seen,
        "unreached": unreached,
    }


# ---------------------------------------------------------------------------

def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", action="store_true", help="print the reference stream")
    ap.add_argument(
        "--oracle",
        action="store_true",
        help="also read the values off the original's screens (needs dosbox-x)",
    )
    ap.add_argument("--scratch", default="/tmp/difftest")
    args = ap.parse_args(argv)

    img = load()
    ref, ev = reference(img)

    if args.dump:
        print("\n".join(ref))
        return 0

    port = port_stream()
    ok, report, counts = compare(ref, port)

    print("quantity              orig  port")
    for kind in sorted(counts):
        a, b = counts[kind]
        print("  %-18s %5d %5d%s" % (kind, a, b, "" if a == b else "   <-- differs"))
    print()
    print("priced rows: %d from the byte-array scan, %d from the immediate scan"
          % (ev["priced_row_byte"], ev["priced_row_imm"]))
    print("  %d purchase debits paired with a menu row by the key each tests"
          % ev["debit_rows"])
    for slot, want, got in ev["debit_mismatch"]:
        print("  FAIL %s row %s: menu tests %d, purchase takes %s" % (*slot, want, got))
        ok = False
    for slot in ev["debit_unmatched"]:
        print("  FAIL debit for %s row %s has no menu row" % slot)
        ok = False
    for shop, key, displayed, price in ev["quote_gap"]:
        print(
            "  note %s row %s quotes %d and charges %d -- reproduced, not fixed"
            % (shop, key, displayed, price)
        )
    print("  trn row 3's `#` is filled with %d (1000:e505)" % ev["trn3_fill"])
    print()
    for line in report:
        print(line)

    if args.oracle:
        print()
        scratch = pathlib.Path(args.scratch)
        oracle_ok, oracle_report, acc = oracle_channel(ref, scratch)
        for line in oracle_report:
            print(line)
        print(
            "screen channel: %d values confirmed on the original's own screens, "
            "out of %d reference records" % (acc["confirmed"], len(ref))
        )
        print("  mar priced rows shown: %d of %d" % acc["mar_rows"])
        print("  starting stat lines shown: %d of %d" % acc["start_stats"])
        print("  threshold_base sightings: %d" % acc["threshold_sightings"])
        for line in acc["unreached"]:
            print("  NOT confirmed by any screen: %s" % line)
        ok = ok and oracle_ok

    print()
    print("OK   %d records match" % len(ref) if ok else "FAIL see above")
    return 0 if ok else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except DifftestError as exc:
        print("FAIL %s" % exc)
        sys.exit(1)
