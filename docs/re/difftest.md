# The differential test (Task 12)

`tools/difftest.py` compares the Rust port against `orig/g.exe` on the numbers
the game was **authored** with — shop prices, XP thresholds, level-up stat
gains, starting stats per class, item bonuses, menu numbering. Nothing else.

    python3 tools/difftest.py               # runs `cargo build --release` itself
    python3 tools/difftest.py --oracle      # add the DOSBox-X screen channel
    python3 tools/test_difftest.py          # 31 tests, 13 of which must see a failure

---

## Why this file is not the whole guarantee, and what covers the rest

Two other oracles already exist, and both are stronger than anything this file
does:

| channel | artifact | what it covers | asserted by |
|---|---|---|---|
| draw-level RNG | `data/rng_trace.json` — 1387 ordered `Random` draws captured from `orig/g.exe` under qemu+gdb | every number the game **computes** from the generator: damage, encounters, wander events, loot | `tests/wander_sequence.rs` |
| per-turn state | `data/state_trace.json` — 91 samples of 35 guest variables | the player's whole record, money, flags and counters, once per turn | `tests/wander_sequence.rs` |
| **this file** | none — it reads `orig/g.exe` directly | the constants **compiled into** the image, which no captured run has to exercise | `tools/test_difftest.py` |

The plan's original Task 12 asked for a full-sequence screen comparison: run both sides on the same keystrokes, scrape every integer off
both screens, diff the two lists. **That is superseded and was not built.** The
draw oracle sits upstream of everything that gets printed, so it already
implies the screen comparison and implies it more strongly; building the weaker
check afterwards would add a passing test and no information. See
`.superpowers/sdd/task-12-supplement.md`.

What the two capture files cannot reach is a number the game *holds* rather
than *derives*. A shop price only appears if a run walks into that shop; a
class's opening stat line only appears if a run picks that class. This file is
exactly that residue.

---

## Covered, with counts

Every count below is printed by `python3 tools/difftest.py`. Its output, run
against `HEAD`:

```
quantity              orig  port
  class_weights         11    11
  imm_row_site           9     9
  item                  15    15
  levelup_gain          10    10
  menu_order             5     5
  priced_row            27    27
  scalar                 4     4
  start_stats            4     4
  xp_threshold          41    41

priced rows: 18 from the byte-array scan, 9 from the immediate scan
  27 purchase debits paired with a menu row by the key each tests
  note bmar row 9 quotes 70 and charges 60 -- reproduced, not fixed
  trn row 3's `#` is filled with 10 (1000:e505)


OK   126 records match
```

| quantity | records | what is compared | where the original's value is read |
|---|---:|---|---|
| `priced_row` | 27 | shop, key, price tested for affordability, price displayed, row text (colour markup stripped) | two instruction-shape scans, below |
| `menu_order` | 5 | the keys of one location's rows, in the order the handler prints them | the same scans, in address order |
| `xp_threshold` | 41 | levels 0..40 | `1000:6de0` `mov word [20ae:38d0],0xa` and `1000:2550` `add word [20ae:38d0],0xa` |
| `scalar` | 4 | `threshold_base`, `threshold_step`, `max_level`, `gains_per_level` | the two above, plus `1000:2580` `cmp word [20ae:38a6],0x28` and `1000:287d` `cmp word [bp-8],2` |
| `class_weights` | 11 | the growth-weight table, four bytes per class | `20ae:0002`, bounded by the rank-name table base `1000:1a3e` `add di,0x2e` |
| `start_stats` | 4 | the four character-creation stat lines and the class each stores | `1000:7148`, `1000:7167`, `1000:7186`, `1000:71a0`, plus `1000:71b8` `add word [20ae:389c],3` |
| `levelup_gain` | 10 | every record field one stat grant moves, and by how much | `FUN_1000_2526`, decoded per arm — below |
| `item` | 15 | each item's bonus | the item's own `^1…(+N)` inventory string, scanned for in the whole image |
| `imm_row_site` | 9 | the address each immediate-priced row's price is written down at | the offset of the `cmp` the scan matched |
| **total** | **126** | | |

## Not covered — say it plainly

* **The RNG sequence and per-turn state.** Covered elsewhere, by
  `tests/wander_sequence.rs` against `data/rng_trace.json` and
  `data/state_trace.json`. This file does not touch either, and passing it says
  nothing about them.
* **Row gates.** Which rows a location prints at a given district/level is
  *not* in the record stream. The nine immediate-priced rows' gates are pinned
  separately, in Rust, by `src/game.rs`'s
  `district_one_opens_only_the_ungated_imm_rows`,
  `the_gyms_experience_row_closes_at_its_level_ceiling`,
  `the_gyms_abs_row_needs_a_third_district_and_room_to_train` and
  `a_high_district_opens_every_imm_row`. `mar`/`bmar` gates come from
  `data/shops.json`'s `gate` field and are not re-derived here.
* **Instruction order inside a level-up grant.** The port derives its gains by
  applying `progress::grant` and diffing the record, which cannot see an
  order, so both sides sort by field name. The original's order is
  `strength, dmg_max, dmg_min, hpmax, hp` (`1000:261d`, `1000:267f`,
  `1000:2691`, `1000:2695`, `1000:2699`); nothing checks that the port
  performs them in that sequence, and nothing needs to — they are independent
  additions.
* **The predicate on the one conditional gain.** Both sides agree that
  strength's `dmg_min + 1` is conditional; neither side compares *what* the
  condition is. That the condition is "the new strength is even"
  (`1000:2683`..`1000:2691`) is pinned by `tests/progression.rs`.
* **What a purchase does.** Prices are compared; effects are not. The port
  deducts a row's price and prints its text but applies no item effect — an
  open gap, recorded in `docs/re/gaps.md`.
* **Item names, kinds and effects.** Only the *bonus* number is compared. The
  `kind`/`effect` classification in `data/items.json` is a naming decision made
  by `tools/extract_tables.py`, not a number in the image.
* **The one remaining unidentified `sub [money],imm8` site**, `1000:502c`
  (file `0x68fc`, 7 rubles). It is in no location handler; see below.
* **A shared misreading.** `tools/difftest.py` and `tools/extract_tables.py`
  read the same bytes. If both misunderstand the same instruction, both agree
  and this test passes. What it does catch is a stale artifact, a wrong
  `build.rs` mapping, a transcription slip, and any later edit to a port
  constant.

---

## How the two sides are kept apart

The reference side is read out of `orig/g.exe` **by `tools/difftest.py`
itself**. It does not import `tools/extract_tables.py` and does not open
`data/shops.json`, `data/items.json` or `data/xp.json` — those are what the
port is *built* from (`build.rs` bakes them into the binary), so reading them
here would compare the port with its own input and could not fail. The port
side is whatever `gopnik --trace-deterministic` prints.

`tools/difftest.py` runs `cargo build --release` itself before reading the
binary, and then refuses a binary older than `src/`, `build.rs`, `Cargo.toml`
or the three baked-in JSON tables. Both guards exist for the same reason:
comparing a stale binary is a check that cannot fail, because the edit under
test is not in it — and it is easy to hit here by accident, since
`tools/test_extract_tables.py` rewrites `data/*.json` on every run.

That the comparison *can* fail is not asserted, it is exercised.
`tools/test_difftest.py` mutates a copy of the load image, at least once per
covered quantity, and requires the mutation to move the reference stream *and*
make `compare()` return a failure:

| mutation | the record it must move |
|---|---|
| the price byte `20ae:0b2e` | `priced_row mar 1` |
| the displayed-price byte `20ae:0b3f` | `priced_row bmar 8`'s two numbers **and** `bmar 9`'s displayed number, which is the whole point: row 9 reads it for display and `20ae:0b40` for the charge |
| `1000:2550`'s immediate | `scalar threshold_step` and all 41 `xp_threshold` |
| `1000:6de0`'s immediate | `scalar threshold_base` |
| `1000:2580`'s immediate | `scalar max_level` |
| `1000:287d`'s immediate | `scalar gains_per_level` |
| a byte of the weight table | `class_weights 4` |
| `1000:7148`'s first store | `start_stats 1` |
| `1000:71b8`'s immediate | every `start_stats` class |
| `1000:27c3`'s immediate | `levelup_gain vitality hpmax` |
| the `9` in `^1Тесак(Урон+9) ` | `item 9 Тесак` -> `item 7 Тесак` |

Two further checks cover the tool's own guards — changing an immediate-priced
row's price so it contradicts the digits in its own text must *raise*, and
blanking an instruction this file quotes must be refused rather than read past
— and one drops a record from the stream instead of changing one, so the
length check is exercised as well as the element check. Thirteen in all.
`orig/g.exe` is never written to; every mutation is a `bytearray` copy.

---

## The 27 priced rows

Every priced menu row in the image is assembled by one fixed instruction run:
test affordability, set the colour digit `20ae:3b7a` to `'0'` or `'4'`, copy
the row's prefix shortstring into a scratch string, append the digit, write it,
write the row's own text, then `Write` whatever fills the text's `#`. Two
families differ only in how the price reaches the test.

**Family 1 — the price is a byte of the `20ae:0b2e` array (18 rows).**
`mov al,[20ae:0bNN]` / `xor ah,ah` / `cmp ax,[20ae:38c7]` / `jle`. These are
the `mar` and `bmar` rows `data/shops.json` records.

**Family 2 — the price is an instruction immediate (9 rows).**
`cmp word [20ae:38c7],imm8` / `jl`|`jge`. These are the vet's, the club's and
the gym's rows. They are **not** in `data/shops.json` — `extract_tables.py`
scans the price array, and these rows never touch it — which is why
`docs/re/gaps.md` listed "`kl` / `trn` priced rows" as unreproduced. Task 12
traced all nine; they are `src/game.rs`'s `IMM_ROWS`.

| shop | key | price | site | row text (markup stripped) |
|---|---|---:|---|---|
| `rep` | `h` | 3 | `1000:d410` | `3 рубля тебя залатают` |
| `rep` | `r` | 7 | `1000:d465` | `7 рублей починят переломы` |
| `kl` | 1 | 15 | `1000:df6f` | `15  потусоваться на дискотеке(Ловкость +1)` |
| `kl` | 2 | 22 | `1000:dfcb` | `22  разузнать приемы мухлёжников(Удача +1)` |
| `trn` | 1 | 20 | `1000:e400` | `20  качаться гателями и шгангой(Сила +1)` |
| `trn` | 2 | 20 | `1000:e455` | `20  качаться на тренажерах(Выносливость +1)` |
| `trn` | 3 | 10 | `1000:e4c4` | `10  прокачать # качков опыта` |
| `trn` | 4 | 30 | `1000:e521` | `30  купить зубную защиту боксёров(-75% что сломают челюсть)` |
| `trn` | 5 | 20 | `1000:e58f` | `20  прокачать пресс(Броня +1)` |

These nine carry their price in the row text as literal digits rather than
through a `#`. That the digits equal the immediate is checked, not assumed:
the two live at different addresses, and a disagreement stops the run.

### Which handler a row belongs to

Not by proximity. The street verb chain is scanned for its own idiom —
`push ds:0x3972` / `push cs:<token>` / `call 0f78:0bd8` — which finds 21 links,
each with the token string it compares pushed five bytes before the call. Those
tokens are read out of the image, so `mar`'s span is bounded by the address at
which the image itself compares the string `mar`, not by a number written down
here. A row is attributed to the verb whose span contains it. The 21 links, in
address order, are `w run run mar bmar rep girl fight pr kl trn kos i s f k
name version help exit e` — the same set and the same addresses as
`docs/re/command-dispatch.md`'s table.

### The menu numbers

A row's key is read off its own prefix shortstring, not from its position in
the list: strip the markup, take the first whitespace-separated token.
`^61^7 - ^` → `1`, ` 3 -  ^` → `3`, `  ^2h^7 - за ^` → `h`. That is what makes
menu numbering a compared quantity rather than an assumption, and it is why
the vet's rows are keyed `h`/`r` while everything else is keyed by digit.

### Quoted price versus charged price

A byte-priced row names a price three times, at three addresses: the
affordability `cmp` the menu colours the row from, the load that fills the
row's `#` a few instructions later, and the `sub [20ae:38c7],…` the purchase
performs. `difftest.py` reads all three and pairs the debit with the row
**per row**, attributing it to the row whose key the nearest preceding string
compare tests. A per-shop multiset would not do: `bmar` rows 8 and 9 are 70 and
60 either way round.

That pairing is what surfaces the one real gap in the image: **`bmar` row 9
displays 70 and charges 60.** `1000:c877` `mov al,[0xb3f]` pushes 70 into the
row's `#`, while `1000:c832` `mov al,[0xb40]` is what the affordability test
reads and `1000:ce39`/`1000:ce3e` (`mov al,[0xb40]` / `sub [0x38c7],ax`) is
what the purchase takes — 60.
Reproduced in the port, not fixed — the same policy `docs/re/tables.md` applies
to the mage, who quotes `district * 25` and takes `district * 50`.

The vet's two purchase arms are laid out in the opposite order to its two menu
rows (`1000:d553` takes 7, `1000:d5d9` takes 3), which is the concrete reason
the pairing is done by key rather than by position.

### Eight of the nine unnamed `sub [money],imm8` sites are now named

`docs/re/tables.md` lists eleven `83 2E C7 38 ib` sites, two of them annotated
(the club's) and nine `null`. Attributing each to the verb span it falls in and
to the key its nearest preceding string compare tests names eight of the nine:

| site | file | imm | what |
|---|---|---:|---|
| `1000:d553` | `0xee23` | 7 | `rep` row `r` — set the broken bones |
| `1000:d5d9` | `0xeea9` | 3 | `rep` row `h` — patch the jaw |
| `1000:d78e` | `0xf05e` | 12 | the `girl` visit (`1000:d701`..`1000:d798`), not a menu row |
| `1000:e657` | `0xff27` | 20 | `trn` row 1 |
| `1000:e6e3` | `0xffb3` | 20 | `trn` row 2 |
| `1000:e796` | `0x10066` | 10 | `trn` row 3 |
| `1000:e823` | `0x100f3` | 30 | `trn` row 4 |
| `1000:e8b8` | `0x10188` | 20 | `trn` row 5 |

`1000:502c` (file `0x68fc`, 7 rubles) remains unidentified. It is in no
location handler — it sits below `mar`'s token compare at `1000:b94a`, i.e.
before the first of them — and Ghidra puts it inside `FUN_1000_3d11`, the
combat routine.
`data/other_price_sites.json` is a generated artifact this task did not
regenerate, so its `what` fields still read `null` for these eight.

### `20ae:3e34`, the gym's scratch byte

Named here for the first time. `1000:e3a4`..`1000:e3e2` recomputes it on every
entry to the gym: it starts as the armour byte `20ae:38b2` and then has the
armour that came from *equipment* subtracted back out —

* `-1` when `[20ae:38b4]` is set and `[20ae:38b7]` is not (`1000:e3aa`),
* `-2` when `[20ae:38b7]` is set (`1000:e3bc`),
* `-2` when `[20ae:38b6]` is set and `[20ae:38b9]` is not (`1000:e3c8`),
* `-4` when `[20ae:38b9]` is set (`1000:e3db`).

Those four bytes are ownership flags for four `mar` rows, and the four
subtrahends are exactly those rows' own advertised bonuses: `1000:bf80` sets
`[20ae:38b4]` (row 4, abibas, "Смягчает пинок на 1"), `1000:c183` sets
`[20ae:38b7]` (row 7, adidas, "на 2"), `1000:c0e0` sets `[20ae:38b6]` (row 6,
the leather jacket, "защиты … на 2") and `1000:c2ca` sets `[20ae:38b9]` (row 9,
"Броня +4"). So `20ae:3e34` is the part of the armour the player *trained*,
and `trn` row 5 — which grants `Броня +1` and increments both the armour and
the scratch (`1000:e8d6`, `1000:e8da`) — is capped by it at `district * 2`.
**Established from flow.** The four flags sit inside `data/save_layout.json`'s
`unk_0214` run and are not named there; this does not rename them.

**Port divergence, stated rather than papered over:** the port owns none of the
four flags (buying a `mar` row deducts the price and prints the text but
applies no effect), so all four adjustments are inert and `20ae:3e34` is
exactly `armor`. Faithful to the state this port models; wrong the moment
equipment exists.

---

## The level-up gains

`FUN_1000_2526`'s stat-grant loop ends at `cmp word [bp-8],2` (`1000:287d`),
which is also `gains_per_level`. Four `cmp ax,[bp-0xa]` range tests
(`1000:2615`, `1000:26c0`, `1000:275c`, `1000:2814`) pick the stat; each arm
runs from the instruction the test admits to the first jump back to
`1000:287d`. Inside an arm, every `inc word [rec]` and `add word [rec],imm` on
the player's record is collected, and an instruction that a forward conditional
jump skips is marked `conditional`.

That is how strength's `dmg_min + 1` — guarded by `1000:268f` `jnz 0x2695` —
comes out `conditional` without this file restating the predicate. Ten gains,
one of them conditional:

```
levelup_gain strength dmg_max 1 always
levelup_gain strength dmg_min 1 conditional
levelup_gain strength hp 1 always
levelup_gain strength hpmax 1 always
levelup_gain strength strength 1 always
levelup_gain agility agility 1 always
levelup_gain vitality hp 5 always
levelup_gain vitality hpmax 5 always
levelup_gain vitality vitality 1 always
levelup_gain luck luck 1 always
```

---

## The screen channel (`--oracle`)

Five keystroke scripts in `data/difftest_scripts/` run against `orig/g.exe`
under the Task 3 DOSBox-X harness, and the numbers are read off the screens the
original itself printed. Per `docs/re/METHODOLOGY.md` this is **output**-tier
evidence: it can falsify a price, it can never establish one, and a menu no
script can open confirms nothing at all.

| script | what it reaches |
|---|---|
| `market_rows_district1.txt` | the `mar` menu at district 1 — rows 1..5 with their prices |
| `stats_class0.txt` … `stats_class3.txt` | the `s` status screen right after creation — `Сл:# Лв:# Жв:# Уд:#` and the first XP threshold, one per class |

`python3 tools/difftest.py --oracle`, run against `HEAD`:

```
OK   market_rows_district1.txt: mar rows 1,2,3,4,5 read off the screen
OK   stats_class0.txt: starting stats [3, 3, 3, 3]
OK   stats_class1.txt: starting stats [5, 2, 4, 1]
OK   stats_class2.txt: starting stats [4, 3, 3, 2]
OK   stats_class3.txt: starting stats [3, 3, 2, 4]
screen channel: 13 values confirmed on the original's own screens, out of 126 reference records
  mar priced rows shown: 5 of 9
  starting stat lines shown: 4 of 4
  threshold_base sightings: 4
  NOT confirmed by any screen: mar rows 6,7,8,9: their own `district > 1` test (1000:bb80, 1000:bc42, 1000:bca5) keeps them off a district-1 screen
  NOT confirmed by any screen: the other 18 priced rows (bmar, rep, kl, trn): each location refuses entry until its discovery flag at 20ae:3694..369a is set, and the vet prints its two rows only to a hurt character (1000:d3d3). None of the five scripts here opens those menus; whether some longer script could is not settled by this run
  NOT confirmed by any screen: every xp_threshold above the first, class_weights, item bonuses and levelup_gain: no screen prints them as such
```

**13 of 126.** The other 113 records rest on the image bytes alone. That is not
a defect — flow outranks output — but it is the honest number, and it is
printed rather than described.

`tools/test_difftest.py` runs this channel too, and skips it *with a message*
when `dosbox-x` is not installed.

---

## The port side

`gopnik --trace-deterministic` writes the record stream and exits; `src/trace.rs`
is the whole of it. It takes no input, spends no `Random` draws and prints no
colour. Every text field has its `^N` markup removed before it is written:
markup is not content, so it is neither printed nor compared. An unrecognised
argument exits 2 rather than falling through to an interactive session, so a
typo cannot look like an empty trace.

The 27 priced rows do not all carry the same weight, and the "27 match" line
should not be read as if they did:

* **18** (`mar`, `bmar`) reach the port through
  `tools/extract_tables.py` → `data/shops.json` → `build.rs`, all written in
  earlier tasks. Comparing them against a fresh scan is a genuine
  cross-pipeline check.
* **2** (the vet's) were already hardcoded in `src/game.rs`, from an earlier
  task, as one fused format string per row. Task 12 split each into the
  prefix/text pair the image actually holds; the numbers are unchanged, and
  the comparison now pins them.
* **7** (the club's two and the gym's five) were transcribed into the port
  **by this same task**, from the same disassembly the reference side reads.
  For those seven this is a transcription check and a regression pin, not an
  independent one.

The gym is not reachable in the port — nothing sets its discovery flag
`20ae:369a` — so its five rows reach a screen only through
`--trace-deterministic` and `src/game.rs`'s
`an_imm_row_is_prefix_then_colour_digit_then_text` today. The club is
reachable: `girl` sets `20ae:3699` at `1000:d751`.
