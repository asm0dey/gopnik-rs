# Item, shop and enemy tables (Task 10)

Machine-readable form: `data/items.json`, `data/shops.json`,
`data/enemies.json`. Regenerate with `python3 tools/extract_tables.py`;
check with `python3 tools/test_extract_tables.py` and `cargo test --test
data_load`.

Every address below is a Ghidra address in the load layout Task 4 used:
`CODE_0` at `1000:0000` (file `0x18D0`), the const/data segment at
`20ae:0000` (file `0x123B0`). `docs/re/METHODOLOGY.md`, "Address convention, and its range of validity", is the authority for the rule; `tools/addr.py` is its executable form and `python3 tools/re_query.py resolve <citation>` checks any single address against the bytes.

## Runtime vs. provenance

Each table is two files, not one:

* **`data/items.json`, `data/shops.json`, `data/enemies.json`** --
  **runtime**. Exactly the fields the game needs to play: names, prices,
  gates, stat blocks, the `sold`/`generated` booleans that change what the
  game does. `src/data.rs` embeds and deserialises only these three;
  `Item`, `ShopEntry` and `Enemy` in that file carry no address, no file
  offset, no "which byte of `orig/g.exe`" field at all. If you are writing
  gameplay code (Task 11 and later), this is the file, and the struct, you
  want.
* **`data/items.provenance.json`, `data/shops.provenance.json`,
  `data/enemies.provenance.json`** -- **provenance**. Where each runtime
  fact was read: Ghidra addresses, file offsets, and the cross-checks that
  confirmed them (e.g. `shops.provenance.json`'s `charged`, which records
  that *some* debit site in the binary reads the row's `price_addr`; see
  "The row idiom" below). If you are auditing a value, chasing a citation,
  or extending the extractor, this is the file you want. Nothing in `src/`
  reads these; they exist so every fact below still traces back to a byte
  of `orig/g.exe`, per this project's rule that an RE finding lands both in
  a `docs/re/` note and in a `data/` artifact.

A provenance file is a JSON object keyed by the runtime row's natural key --
`id` for items and enemies, which already have one, and `"<shop>:<key>"`
(e.g. `"bmar:9"`) for shop rows, which do not -- so any runtime row can be
traced back to its bytes with one lookup: read the row's key out of the
runtime file, then look that key up in the sibling `.provenance.json`.

Both files for a table come out of the same extractor run and describe the
same rows; splitting them changed no extracted value; see
`.superpowers/sdd/task-10-report.md`, "Fix wave 2" for the field-by-field
diff that confirmed it.

---

## 1. Items

The equipment set is not a table in the binary at all -- it is fifteen
bonus-carrying display strings, each of which states its own bonus (the same
`^1`-prefixed status-screen block also has seven further possession/state
strings that are not equipment; see the end of this section). The `file off`
column below is `src_off`, and lives in `data/items.provenance.json` (keyed
by `id`), not in `data/items.json`; likewise the price row's *source*
(`price_src`, "which shop row and address this item's price was linked
from") below is provenance, while the linked `price` value itself is
runtime:

| file off | text | kind | bonus | effect |
|---|---|---|---|---|
| `0x2FB2` | `^1Крестик(Удача +2) ` | charm | 2 | luck |
| `0x2FC7` | `^1Кольцо "Гс"(Удача +1) ` | charm | 1 | luck |
| `0x2FF0` | `^1Кольцо "Пг"(Всё +1) ` | charm | 1 | all |
| `0x3007` | `^1Мега Кольцо(Всё +4) ` | charm | 4 | all |
| `0x301E` | `^1Кольцо "Гп"(Самолечение) ` | charm | 0 | regen |
| `0x30FE` | `^1Бутсы(+1) ` | weapon | 1 | — |
| `0x3114` | `^1Понтовые бутсы(Урон+2) ` | weapon | 2 | damage |
| `0x312E` | `^1Кастет(+2) ` | weapon | 2 | — |
| `0x3146` | `^1Дубинка(+4)  ` | weapon | 4 | — |
| `0x3161` | `^1Нож(+6) ` | weapon | 6 | — |
| `0x3173` | `^1Тесак(Урон+9) ` | weapon | 9 | damage |
| `0x323E` | `^1Костюм Adidas(+2) ` | suit | 2 | — |
| `0x3253` | `^1Костюм Abibas(+1) ` | suit | 1 | — |
| `0x3273` | `^1Крутая кожанка(+4) ` | armor | 4 | — |
| `0x3289` | `^1Кожанка(+2) ` | armor | 2 | — |

`tools/extract_tables.py` recovers them by pattern, not by that list: it
scans `data/strings.json` for strings that start with the `^1` directive and
whose text ends in one of the five bonus forms. The `^1` restriction is what
keeps the shop menu out -- `#^7 руб. Понтовёйшие бутсы(Урон+2)` (`0xA63D`)
matches the `(Урон+N)` form perfectly well and would otherwise arrive as an
item named `# руб. Понтовёйшие бутсы`. The scan finds exactly these fifteen
and nothing else.

`kind` for the bare `(+N)` forms comes from `BARE_KIND` in the extractor, a
name->slot map; a name not in it falls through to `misc` rather than being
guessed. `effect` is only what the suffix literally names, so it is null for
every bare `(+N)`.

**Prices are deliberately null for thirteen of the fifteen -- and those
thirteen are not all the same kind of null.** `data/items.json` now carries
`sold: bool` to separate them:

**Seven are loot-only and will never have a price** (`sold: false`):
Крестик, Кольцо "Гс", Кольцо "Пг", Мега Кольцо, Кольцо "Гп", Нож, Тесак. The
evidence is a second, independent print of (most of) these names: file
`0x5389`..`0x5555` (`1000:3ab9`..`1000:3c85`) is a wandering-encounter event
table, one contiguous run of one-shot pickups, each shaped identically --
`cmp byte [flag],0` / `mov byte [flag],1` / load the string / far call
`0xeed:0x1c2`:

| item | text at the find site | file off | print site | one-shot flag |
|---|---|---|---|---|
| Крестик | `Ты нашёл крестик: удача +2` | `0x5423` | `1000:5498` | `20ae:38bd` |
| Кольцо "Гс" | `Ты нашёл кольцо "Господи спаси": удача +1` | `0x5440` | `1000:54c8` | `20ae:38be` |
| Кольцо "Пг" | `Кольцо "Помоги Господи"` | `0x5389` | `1000:5316` | `20ae:38bf` |
| Мега Кольцо | `"Мега Кольцо"! со своего, можно сказать, пальца` (2nd copy at `0x93ed`) | `0x53a3` | `1000:5371` | `20ae:38c0` |
| Кольцо "Гп" | `Ваще полезное кольцо "Господи помилуй"` + `Восст. жизни - 3, 5% - самозарост переломов` | `0x53b4` / `0x53dd` | `1000:53c0` / `1000:53d9` | `20ae:38c1` |
| Нож | `Ты нашел ножик(урон+6).` | `0x54fc` | `1000:569d` | `20ae:38c2` |
| Тесак | `Ты нашел тесак(урон+9)!!! - ужасное оружие.` | `0x553b` | `1000:5743` | `20ae:394c` |

Кольцо "Пг", Мега Кольцо and Кольцо "Гп" have no literal "нашёл" in their
find text, so they are not classed loot-only on wording alone -- their find
sites sit inside the same contiguous block, gated by the same one-shot-flag
idiom, and printed by the same far call as the four rows that do say
"нашёл". That structural identity, not the words, is the evidence.

**The other eight null prices are genuinely pending, not loot-only**
(`sold: true`, `price: null`). `Костюм Adidas`/`Abibas` and
`Кожанка`/`Крутая кожанка` are sold under paraphrased names with matching
bonuses -- `mar` rows 4, 6, 7 and 9 below -- but the extractor only links a
price on a *verbatim* name match (see `link_item_prices`), which a
paraphrase does not satisfy; deciding the link is Task 11's job, not a
string-matching guess. `Бутсы`/`Понтовые бутсы` are the boots ambiguity,
below.

The only price source *for these fifteen equipment strings* is the shop rows
(section 2), and their text is not the same string as the inventory text.
Two rows name their item verbatim *and* agree on the bonus --
`#^7 руб. Кастет(урон+2)` and `#^7 руб. Дубинка(урон+4), заменяет кастет` --
and only those two get a price this way (25 and 50). The worst case is
boots: the inventory has one line, `Понтовые бутсы(Урон+2)`, while the market
sells `Понтовые бутсы(Увеличивают урон)` for 15 and `Понтовёйшие
бутсы(Урон+2)` for 30. **UNVERIFIED: which market row produces the inventory
line `Понтовые бутсы(Урон+2)`.** Deciding it needs the purchase handlers'
effects read, which is Task 11's job; a guess here would be an invention.

`data/strings.json` has seven further `^1`-prefixed status-screen lines in
the same block as the fifteen item strings that the pattern scan correctly
does not pick up, because none of them carries a `(+N)`-shaped bonus: `0x3198
^1Зубная защита  `, `0x303a ^1У тебя есть мобильник`, `0x3052 ^1У тебя есть
тёмные очки`, `0x306c ^1На тебе зоновская наколка`, `0x3088 ^1У тебя есть
пистолет`, `0x309f ^1 с гушителем` and `0x30ae ^1! патронов - #`. These are
inventory *state* (possessions and counters the status screen reports), not
bonus-carrying equipment, so they correctly are not items -- but Task 11
will want them for its own inventory model. `гушителем` is the original's
own spelling, not a transcription slip; it stays as printed.

The binary has price sources besides these fifteen strings and the shop
rows below. See "Other price sources, not extracted" at the end of section 2.

---

## 2. Shops

There are two menus that the original itself calls out by a location tag --
`mar` (`0xA42C`, "Базар") and `bmar` (`0xAA24`, "Барыги") -- and this section
extracts both, nine rows each. That is not a claim that these are the only
two places the game charges money: see "Other price sources, not extracted"
below for what else debits `20ae:38c7` and was deliberately left out of
`data/shops.json`.

### Where the prices live

Prices are **not** immediates. Every row reads a byte out of a 19-entry const
array at `20ae:0b2e` (file `0x12EDE`). Its extent is pinned on both sides by
neighbours that were established earlier: the `ranks` string table occupies
`20ae:002e` for 11 * 256 = `0xB00` bytes, ending exactly at `20ae:0b2e`, and
the `krutizna` table starts at `20ae:0b42` (`docs/re/string-tables.md`). The
20 bytes between them are:

```
20ae:0b2e  02 05 0a 0f 0f 19 1e 1e 32 00 0f 1e 14 0a 19 32 96 46 3c 00
```

`20ae:0b37` (`0x00`) is referenced by nothing in the binary and
`20ae:0b41` (`0x00`) is the pad before `krutizna`. **UNVERIFIED: what
`20ae:0b37` was meant to be.** It sits exactly between the two shops' price
runs and is left as an unknown zero rather than assigned to a row.

### The row idiom

Each menu row compiles to one rigid sequence; the first is at file `0xD283`
(`1000:b9b3`). `tools/extract_tables.py` scans for this pattern, so the row
count and the price->row mapping fall out of the bytes:

```
A0 lo hi        mov al,[price]          ; 20ae:0b2e..0b40
30 E4           xor ah,ah
3B 06 C7 38     cmp ax,[0x38c7]         ; 20ae:38c7 = the player's money
7E 07           jng +7
C6 06 7A 3B 34  mov byte [0x3b7a],'4'   ; red   - cannot afford
EB 05           jmp +5
C6 06 7A 3B 30  mov byte [0x3b7a],'0'   ; black - can afford
8D BE dd dd     lea di,[bp+disp]
16 57           push ss / push di
BF lo hi        mov di,<prefix>         ; "^61^7 - ^" -- carries the hotkey
0E 57           push cs / push di
9A E7 0A 78 0F  call 0f78:0ae7
8D BE dd dd / 16 57 / A0 7A 3B / 50
9A 03 0C 78 0F  call 0f78:0c03          ; append the colour digit
9A 66 0B 78 0F  call 0f78:0b66
BF lo hi        mov di,<row text>       ; "#^7 руб. ..."
0E 57
9A 66 0B 78 0F  call 0f78:0b66
A0 lo hi        mov al,[price]          ; the price that is PRINTED
30 E4 / 50
```

The debit site is a second, equally rigid idiom,
`A0 lo hi / 30 E4 / 29 06 C7 38` (`sub [money],ax`). The extractor collects
every address any such site debits into a single file-wide set, and records
`charged: true` in `data/shops.provenance.json` on a row when the address
its affordability test reads is a member of that set -- i.e. *some* debit
site in the binary reads the same address, not that *this row's own*
purchase handler is the one that debits it. That is sound for these 18 rows
because every address in the set is debited exactly once (checked by hand
against the disassembly), so membership and row-specific debit happen to
coincide here; a row that shared its price address with another row's debit
site would pass this check without actually being charged. All 18 rows are
`charged: true`. `charged` is provenance, not a runtime field: it is a
cross-check on the extraction (does *some* debit site read this address?),
not something a game loop consults -- every row is always charged its
`price` when bought, so there is no decision left for the game to make with
it.

Rows are attributed to a shop by the last short all-lowercase-ASCII string
the code loads before the menu -- `mar` at file `0xD215`, `bmar` at
`0xDD89`.

### The table

The `addr` column below is `price_addr` -- provenance, recorded in
`data/shops.provenance.json` keyed by `"<shop>:<key>"`, not in
`data/shops.json`. `code_off`/`code_addr`/`prefix_off`/`text_off` (the row's
own location in the code segment, unused by anything in `src/`) live there
too.

`district` is the byte at `20ae:3692`, 1..5, raised at file `0xC462`
(`1000:ab92`, `inc byte [0x3692]`) once понтовость reaches `district * 10`. Gates come from
`cmp byte [x],n` + conditional jump, read at block scope: a gate applies to
every row between it and its jump target, which is why `mar` rows 6 and 7
share one.

#### `mar` -- Базар

| key | price | addr | gate | text |
|---|---|---|---|---|
| 1 | 2 | `20ae:0b2e` | — | `#^7 руб.  Хотдог(3-4 з)` |
| 2 | 5 | `20ae:0b2f` | — | `#^7 руб.  Пиво(#з)` |
| 3 | 10 | `20ae:0b30` | — | `#^7 руб. Затемнённые очки(Чтоб менты не узнали)` |
| 4 | 15 | `20ae:0b31` | — | `#^7 руб. Реальный спортивный костюм abibas(Смягчает пинок на 1)` |
| 5 | 15 | `20ae:0b32` | — | `#^7 руб. Понтовые бутсы(Увеличивают урон)` |
| 6 | 25 | `20ae:0b33` | district>1 | `#^7 руб. Реальную кожанку(Дополнительная защита от случайностей на 2)` |
| 7 | 30 | `20ae:0b34` | district>1 | `#^7 руб. Реальный спортивный костюм adidas(Смягчает пинок на 2)` |
| 8 | 30 | `20ae:0b35` | district>2 | `#^7 руб. Понтовёйшие бутсы(Урон+2)` |
| 9 | 50 | `20ae:0b36` | district>3 | `#^7 руб. Ваще крутую кожанку(Броня +4)` |

The second `#` of row 2 is a literal `5` pushed at file `0xD32A`
(`mov ax,5 / push ax`), which is why the screen reads `Пиво(5з)`.

#### `bmar` -- Барыги

| key | price | addr | gate | text |
|---|---|---|---|---|
| 1 | 15 | `20ae:0b38` | — | `#^7 руб. Косяк` |
| 2 | 30 | `20ae:0b39` | — | `#^7 руб. Краденый мобильник(Подмога быстрее приходит)` |
| 3 | 20 | `20ae:0b3a` | — | `#^7 руб. Офигенный косяк(Очко прокачки)` |
| 4 | 10 | `20ae:0b3b` | — | `#^7 руб. Сделать типа зоновскую наколку(...)` |
| 5 | 25 | `20ae:0b3c` | district>1 | `#^7 руб. Кастет(урон+2)` |
| 6 | 50 | `20ae:0b3d` | district>2 | `#^7 руб. Дубинка(урон+4), заменяет кастет` |
| 7 | 150 | `20ae:0b3e` | district>3 | `#^7 руб. Самопальный пистолет (...)` |
| 8 | 70 | `20ae:0b3f` | district>3 | `#^7 руб. Патроны - 6.` |
| 9 | **60** | `20ae:0b40` | district>3, `byte[20ae:394d]!=0`, `byte[20ae:3e32]==25` | `#^7 руб. Глушитель.` |

### The silencer bug -- reproduce it, do not fix it

Row 9 of `bmar` prints the **wrong** price. At file `0xE102` the affordability
colour test reads `[20ae:0b40]` (60) and at `0xE6DE`/`0xE709` the purchase
compares and debits `[20ae:0b40]` (60) -- but at `0xE147` the value pushed
into the row's `#` placeholder is `[20ae:0b3f]` (70), the *ammunition* price
from the row above. So the menu advertises 70 руб. and the till takes 60.
Confirmed on the original's own screen; see section 4.

`data/shops.json` keeps both: `price` (60, what is charged) and
`displayed_price` (70, what is printed). A port that renders `price` on the
menu is wrong.

Row 9's extra gates: `20ae:394d` is the "owns a pistol" flag set at `0xE5D5`;
`20ae:3e32` is a counter incremented once per `w`/`run` command at `0xC802`,
but only while the player knows `bmar` and owns a pistol, and it stops at 25
(`0xC7FB`). So the silencer appears 25 wanders after the pistol is bought.

### Other price sources, not extracted into `data/shops.json`

`mar` and `bmar` are not the only places the binary charges the player. The
player's money is the word at `20ae:38c7`, and exactly two instruction
encodings subtract from it:

| encoding | disassembly | occurrences in `orig/g.exe` |
|---|---|---|
| `29 06 C7 38` | `sub word [20ae:38c7],ax` | 21 |
| `83 2E C7 38 ib` | `sub word [20ae:38c7],imm8` | 11 |

`tools/extract_tables.py` scans **both bare `sub` encodings** over the whole
file and writes every match to `data/other_price_sites.json`, so that file is
complete by construction rather than by inspection. The `ax` sites are then
classified by the idiom that produced `ax`; `ax_debit_sites.count` equals both
the number of `29 06 C7 38` occurrences in the binary and the sum of its
category counts, and `tools/test_extract_tables.py` re-scans `orig/g.exe`
itself to assert it.

**This was got wrong once and is worth stating plainly.** An earlier revision
scanned only the *composite* idiom `A0 addr / 30 E4 / 29 06 C7 38` and emitted
each category independently, on the reasoning that the remaining forms "have
no fixed byte idiom to scan for". They do -- `29 06 C7 38` is a fixed idiom,
and it is the same four bytes in every one of the 21 sites. What resists
scanning is the *meaning* of a site (which service is paid for, which string
is printed), never the site itself. Scanning the longer idiom hid the debit at
`1000:5014` (file `0x68e4`) from every array in the artifact while its `note`
claimed to record every place the game debits money.

#### The 21 `sub [money],ax` sites

| category | count | where the amount comes from | recorded in |
|---|---|---|---|
| `shop_row` | 18 | `A0 addr / 30 E4` -- a byte of the `20ae:0b2e` price array | `data/shops.json` |
| `variable` | 1 | `A0 addr / 30 E4` -- a byte *variable*, `20ae:3c82` | `var_sites` |
| `computed` | 1 | `A0 addr / 30 E4 / BA imm16 / F7 E2` -- byte times a constant | `computed_sites` |
| `other` | 1 | `9A off seg` -- a far call's return value | `other_ax_sites` |

The 18 `shop_row` sites are the `mar`/`bmar` rows above; they appear in
`ax_debit_sites` only as a cross-reference, each naming its `"<shop>:<key>"`
row. The other three:

| addr | file off | amount | what |
|---|---|---|---|
| `1000:5014` | `0x68e4` | return value of `call 0f78:1131` | **unidentified** |
| `1000:761d` | `0x8eed` | `byte[20ae:3692] * 50` (district*50) | Рушель Блаво save-game service — **but see the note below: the price he quotes is half this** |
| `1000:e0a8` | `0xf978` | `byte[20ae:3c82]` | **unidentified** (see below) |

`1000:5014` is preceded by `mov di,0x4000 / call 0f78:1111 / call 0f78:1131`,
so the debited amount is whatever that call returns; its `what` is still
`null`, but Task 11h identified the calls — see `docs/re/rtl.md`. The whole
sequence from `1000:4ff0` is `0f78:1125` (32-bit integer to 6-byte real),
`0f78:1117` (real divide, by the literal in `cx`/`si`/`di` at `1000:4ff5`),
`0f78:1111` (real multiply, by the literal at `1000:5002`) and `0f78:1131`
(real back to integer, error 207 on overflow). So the amount is
`round(x / K1 * K2)`. Reading `K1` and `K2` as decimal numbers needs the
6-byte real layout confirmed against a known value and is **not established**. It sits 24 bytes before the `imm8` site
at `1000:502c`, so it is a genuinely separate debit, not a second reading of
that one.

The `computed` site's own bytes are scanned like every other: the multiplied
address (`20ae:3692`, the district counter -- "Availability gates" above) and
the multiplier (`0x32` = 50) both come out of `MUL_CHARGE_RE`, and so does the
`formula` string. Only the service name and the string cross-reference are
hand-verified with `ndisasm -b16`: the printed string `За # рублей он может
сделать сохранение прямо здесь.` is at file `0x8d2d`, the `mov di,0x745d` that
loads it is at `1000:7583` (file `0x8e53`), and the affordability guard
(`cmp ax,[0x38c7] / jng`) is at `1000:760a` -- a reminder that a string's own
byte address and the address of the instruction that references it are two
different numbers.

**The quoted price and the charged price differ, in the original.** Task 11b
found the other half of this pair, and it is not a transcription slip in this
document -- both numbers are in the binary:

| what | address | bytes | value |
|---|---|---|---|
| the price *printed* into `За # рублей...` | `1000:758d` | `ba 19 00` | `district * 25` |
| the price *checked* against money | `1000:7605` | `ba 32 00` | `district * 50` |
| the price *debited* | `1000:7618` (`sub` at `1000:761d`) | `ba 32 00` | `district * 50` |

All three are the same `mov al,[0x3692] / xor ah,ah / mov dx,imm / mul dx`
shape, differing only in the immediate. Рушель Блаво quotes half what he
takes. Do not "fix" this in the port -- reproduce it. Flow, byte-verified;
`docs/re/wander.md` § "The mage" has the surrounding control flow.

#### The 11 `sub [money],imm8` sites

Price baked straight into the instruction, rather than read from the
`20ae:0b2e` array (found by scanning `83 2E C7 38 ib`):

| addr | file off | imm | what |
|---|---|---|---|
| `1000:502c` | `0x68fc` | 7 | unidentified |
| `1000:d553` | `0xee23` | 7 | unidentified |
| `1000:d5d9` | `0xeea9` | 3 | unidentified |
| `1000:d78e` | `0xf05e` | 12 | unidentified |
| `1000:e2a7` | `0xfb77` | 15 | Клуб: `15^7  потусоваться на дискотеке(Ловкость +1)` (file `0xba50`) |
| `1000:e31c` | `0xfbec` | 22 | Клуб: `22^7  разузнать приемы мухлёжников(Удача +1)` (file `0xba85`) |
| `1000:e657` | `0xff27` | 20 | unidentified |
| `1000:e6e3` | `0xffb3` | 20 | unidentified |
| `1000:e796` | `0x10066` | 10 | unidentified |
| `1000:e823` | `0x100f3` | 30 | unidentified |
| `1000:e8b8` | `0x10188` | 20 | unidentified |

The two Клуб (gambling) rows print their own price as the first characters
of the row text itself -- `15^7 ...`, `22^7 ...` -- rather than through the
`#` placeholder the shop rows use, which is why `SHOP_ROW_RE` does not match
them and why the `^1` restriction in section 1 has to exclude the second of
these two (`0xBA85`) from the item scan by name, not by this table.

#### `20ae:3c82`, the one debited byte variable

`20ae:3c82` is a BSS byte -- it maps past the end of the file image, so it has
no value to read out of `orig/g.exe` directly -- and it is the amount the
`variable`-form debit at `1000:e0a8` (file `0xf978`) subtracts. **It is not a
constant.** Its two address bytes occur 14 times in the file, and all 14 are
instruction operands, accounted for by four scanned idioms:

| idiom | field | count | sites |
|---|---|---|---|
| `C6 06 82 3C ib` (`mov byte`) | `write_sites` | 2 | `1000:e020` = 5, `1000:e145` = 5 |
| `80 06 82 3C ib` (`add byte`) | `write_sites` | 1 | `1000:e0f7` += 2 |
| `A0 82 3C` (`mov al`) | `read_sites` | 8 | `1000:e079`, `e08c`, `e0a3`, `e0d0`, `e0e0`, `e12e`, `e15d`, `e25d` |
| `80 3E 82 3C ib` (`cmp byte`) | `compare_sites` | 3 | `1000:e14a` vs 17, `1000:e151` vs 5, `1000:e174` vs 17 |

`data/other_price_sites.json` records `ref_count` (14) alongside
`recorded_sites` (2 + 1 + 8 + 3 = 14); if some further idiom touched the
variable the two would disagree, which is the guard that replaced the earlier
account. That earlier account said the variable was "initialised to 5 and read
at eight *further* sites" -- wrong twice over: it missed the `add` and the
three compares entirely, and the eight `A0` reads are not *further* than the
debit, they **include** it (`1000:e0a3` is both). `charge_sites` is a subset of
`read_sites` for exactly that reason and is not added into `recorded_sites`.

What the byte actually prices is still **not established**, so its `what` stays
`null`. What the bytes do show, from `ndisasm -b16` over file
`0xf940`..`0xfa50`, is the shape of a stake that grows: the value is read,
compared against the player's money, printed, subtracted from the money at
`1000:e0a8`, and then on one branch doubled back into the money
(`shl ax,1 / add [0x38c7],ax` at `1000:e0d5`/`1000:e0d7`) before `1000:e0f7`
raises it by 2; elsewhere it is reset to 5 and bounded against 17. Naming the
mechanic is Task 11's job -- this section records the instructions, not a
story about them.

#### What is derived and what is hand-annotated

`data/other_price_sites.json` is generated by `tools/extract_tables.py`.
**Derived from `orig/g.exe`:** every address, file offset, immediate,
multiplier, call target, formula, and the category each debit site falls into
-- including the `computed` row, which is now scanned rather than hand-listed.
**Hand-annotated:** only the `what` text and its supporting cross-references --
the two Клуб strings (`SUB_IMM8_WHAT`) and the save-game service's name,
guard address and string fields (`COMPUTED_WHAT`). Both tables are keyed by
file offset and were verified by reading the shortstrings at those offsets
straight out of `orig/g.exe`.

Nine of the eleven `imm8` sites, the call-result site, and `20ae:3c82` carry
`what: null` and are named nowhere in this artifact: working out what each one
charges for, and which item or service takes each price, is Task 11's job, not
this task's.

**Task 12 named eight of those nine**, in `docs/re/difftest.md`, by attributing
each debit to the verb-dispatch span it falls in and to the key its nearest
preceding string compare tests: `1000:d553` and `1000:d5d9` are the vet's two
rows (7 and 3), `1000:d78e` is the `girl` visit (12), and `1000:e657`,
`1000:e6e3`, `1000:e796`, `1000:e823`, `1000:e8b8` are the gym's five rows (20,
20, 10, 30, 20). Only `1000:502c` (file `0x68fc`, 7) is still unidentified.
`data/other_price_sites.json` is generated and Task 12 did not regenerate it,
so its eight `what` fields still read `null`; the table above is likewise
unchanged. Neither is wrong about the bytes — they are stale about what is
known.

---

## 3. Enemies

**There is no table of enemy stat blocks, and inventing one would be a lie.**
The prose citations below (`1000:0d14 rolls stats from the weights at
...`, etc.) are each row's `source` string, kept in
`data/enemies.provenance.json` (keyed by `id`) rather than in
`data/enemies.json`; `generated` (does the game roll this enemy, or is it
scripted?) is the one boolean of the two the game actually branches on, so
it stays runtime.

`FUN_1000_0d14` (`1000:0d14`..`1000:11bf`) generates a random encounter.
**Fully recovered by Task 11f** -- the step list below is now established
from flow, and `docs/re/gaps.md`'s "The random-encounter opponent" section
carries the per-site table, the `n` each site pushes and the two `20ae:3693`
readers. In outline, with `param_1 = 0` (what `1000:b5b8` passes):

1. picks a class index into `20ae:3952` from `Random(0x33)` folded through a
   triangular walk, plus `Random(district)`, plus `Random(4)` when
   `[0x3693]` is set, clamped to 9 (`1000:0d22`..`1000:0da5`). The fold
   **inverts** the roll: a `Random(0x33)` of 0-1 gives class 8, 44-50 gives
   class 0;
2. rolls крутизна into `20ae:395c` (`1000:0dc6`..`1000:0e76`):
   `Round(player_level * f / d + s - 2) + 4 * Random(district)`, floored at
   0, then multiplied by 1.5 when `[0x3693]` is set;
3. zeroes strength/agility/vitality/luck (`20ae:3954`..`20ae:395a`);
4. distributes `(w0+w1+w2+w3) + крутизна*2` points over the four stats by
   repeated `Random(w0+w1+w2+w3)`, in proportion to the four weight bytes at
   `20ae:0002 + class*4` -- the same array Task 9b recovered as
   `progress::CLASS_WEIGHTS`;
5. derives `dmg_min = strength div 2`, `dmg_max = strength`,
   `hpmax = vitality*5 + strength + 10`, `hp = hpmax`;
6. rolls the three loot words `20ae:396a`/`396c`/`396e` (beer, money, Хлам)
   and the armour byte `20ae:3968`.

**Correction.** An earlier revision of step 4 said the loop drew
`Random(sum)` where `sum` was the *running remaining points*. It is the
constant weight-row sum: `1000:0ed1` stores it once into `[bp-2]` and
`1000:0ef7` pushes that same byte every iteration. `data/rng_trace.json`
observed exactly `{6, 8, 9, 12, 20, 22}` at `1000:0efd` across 348 stops --
the six distinct weight-row sums of classes 0..9 -- which a decreasing
remainder could not produce.

So `data/enemies.json` carries, for classes 0..9, the class name and its
weight row and nothing else (`generated: true`, `level: null`,
`stats: null`). Names come from the `ranks` string table at `20ae:002e`
(`docs/re/string-tables.md`).

| class | name | weights (str/agi/vit/luck) |
|---|---|---|
| 0 | Дохляк | 1 2 1 2 |
| 1 | Нефор | 2 2 2 3 |
| 2 | Нарк | 2 2 2 2 |
| 3 | Подтсан | 3 3 3 3 |
| 4 | Отморозок | 5 2 4 1 |
| 5 | Гопник | 4 3 3 2 |
| 6 | Вор | 3 3 2 4 |
| 7 | Беспредельщик | 5 3 4 2 |
| 8 | Мент | 5 5 5 5 |
| 9 | Маньячок | 5 6 8 3 |
| 10 | Ректор НГУ | 0 0 0 0 |

Class 10 is clamped out of the random roll, so its rank row also carries no
stats. Its two stat blocks are scripted.

### The two scripted bosses

`FUN_1000_11c2` (`1000:11c2`, file `0x2A92`) is the only place a fixed enemy
is written, and it is called twice back to back at file `0xC6F7`
(`1000:ae27`) and `0xC703` (`1000:ae33`) with `param_1` 0 then 1, each
followed by a call to the combat routine `FUN_1000_3d11` -- the endgame
double fight. The stores are literal
immediates:

```
C7 06 5C 39 imm   mov word [20ae:395c],imm   ; крутизна / level
C7 06 54 39 imm   mov word [20ae:3954],imm   ; strength
C7 06 56 39 imm   mov word [20ae:3956],imm   ; agility
C7 06 58 39 imm   mov word [20ae:3958],imm   ; vitality
C7 06 5A 39 imm   mov word [20ae:395a],imm   ; luck
C6 06 68 39 imm   mov byte [20ae:3968],imm   ; armor
```

and `1000:1228`..`1000:124f` (file `0x2af8`..`0x2b1f`) then derives the same
three quantities the random path does at `1000:0ff3`..`1000:1005` (file
`0x28c3`..`0x28d5`) -- both read `strength`/`vitality` back out of
`20ae:3954`/`20ae:3958` and compute the identical `dmg_min`/`dmg_max`/`hpmax`.

**Corrected citation.** An earlier draft of this table cited `1000:2af8` for
this derivation -- that is the *file offset* `0x2af8` wearing a `1000:`
segment prefix it was never assigned to. The real segment address is
`1000:1228` (`0x2af8 - 0x18d0 = 0x1228`); disassembling the literal address
`1000:2af8` (file `0x43c8`) lands in unrelated code that reads
`[20ae:38c3]` (the крутизна counter), not `[20ae:3954]`. Confirmed by
disassembling both file regions directly (`ndisasm -b16`). Every other
`seg:off`/file-offset pair cited in this document and in
`tools/extract_tables.py` was re-checked against the convention named at the
top of this file (`tools/extract_tables.py` now imports `tools/addr.py`
rather than recomputing it) and against
a direct disassembly where one was available; none of the others had this
error. See `.superpowers/sdd/task-10-report.md`, "Fix wave 1" for the list
of what was checked.

| id | level | str | agi | vit | luck | dmg | hp/hpmax | armor |
|---|---|---|---|---|---|---|---|---|
| `rektor_ngu_v0` | 125 | 41 | 50 | 123 | 36 | 20-41 | 666 | 60 |
| `rektor_ngu_v1` | 160 | 50 | 60 | 188 | 32 | 25-50 | 1000 | 80 |

Both are named "Ректор НГУ"; in play the first turns out to be the проректор
СУНЦа and the second is the real one.

---

## 4. Cross-checks

### Against `data/combat_vectors.json` (Task 9)

Those 295 cases carry real enemy stat blocks read out of the guest's own data
segment during real fights, so they are independent of anything here. Every
one of the 295 enemy records satisfies the three derived-stat rules used for
the boss rows:

```
hpmax == vitality*5 + strength + 10 ,  dmg_min == strength div 2 ,  dmg_max == strength
295 matching, 0 mismatching
```

### Against the DOSBox-X oracle

Prices and boss stats were read off the original's own screens. All runs go
through `tools/oracle/capture.py` (never a bare `dosbox-x`), and none of them
patches the binary.

1. **`mar`, all nine rows.** `\n4\n` then 25 `w\n` (to find the market) then
   `mar\n`:

   ```
   1 - 2 руб.  Хотдог(3-4 з)
   2 - 5 руб.  Пиво(5з)
   3 - 10 руб. Затемнённые очки(Чтоб менты не узнали)
   4 - 15 руб. Реальный спортивный костюм abibas(Смягчает пинок на 1)
   5 - 15 руб. Понтовые бутсы(Увеличивают урон)
   6 - 25 руб. Реальную кожанку(Дополнительная защита от случайностей на 2)
   7 - 30 руб. Реальный спортивный костюм adidas(Смягчает пинок на 2)
   8 - 30 руб. Понтовёйшие бутсы(Урон+2)
   9 - 50 руб. Ваще крутую кожанку(Броня +4)
   ```

   Matches `data/shops.json` exactly, including the literal `5` of `Пиво(5з)`.

2. **`bmar`, rows 1..8.** Loading the player's own save (key `0`) also loads
   `PLACES.SAV`, which has all seven location flags set, so both shops are
   reachable immediately. `SAVE_R0` is only понтовость 15, i.e. district 2,
   which shows rows 1..5; to get district 4 the run was repeated with a
   *staged corpus* -- a scratch copy of `orig/` with `SAVE_R4.SAV` copied
   over `SAVE_R0.SAV`, driven by pointing `capture.ORIG` at that directory.
   `orig/` itself is never written. Result: 15, 30, 20, 10, 25, 50, 150, 70.

3. **The silencer, and its bug.** Same staged corpus, script
   `\n0\n bmar\n 7\n w\n` + 40 x `run\n` + `s\n bmar\n 9\n w\n s\n` (buy the
   pistol, let the 25-turn counter run out, look at the money, buy the
   silencer, look again):

   ```
   9 - 70 руб. Глушитель.      <- displayed_price, from 20ae:0b3f
   Бабки 970                    <- before
   Бабки 910                    <- after: 60 charged, from 20ae:0b40
   ```

   The 10-ruble gap between what the menu says and what the purchase costs is
   the bug, observed rather than inferred.

4. **Boss v0.** `\n5\n` (start at district 5, which goes straight to the
   endgame) then `sv` in the fight:

   ```
   Это Ректор НГУ 125 уровня
   Сл:41 Лв:50 Жв:123 Уд:36
   Урон 20-41
   Здоровье 666/666
   Броня 60
   ```

5. **Boss v1.** Same start, `k\nsv\n` x 25 to kill v0 and inspect v1:

   ```
   Это Ректор НГУ 160 уровня
   Сл:50 Лв:60 Жв:188 Уд:32
   Урон 25-50
   Здоровье 701/1000  Сломана челюсть
   Броня 80
   ```

6. **Nine of the fifteen item strings**, off the `s` (status) screen of a
   finished game: `Крестик(Удача +2) Кольцо "Гс"(Удача +1)`,
   `Кольцо "Пг"(Всё +1) Мега Кольцо(Всё +4) Кольцо "Гп"(Самолечение)`,
   `Понтовые бутсы(Урон+2) Нож(+6)`, `Костюм Adidas(+2) Крутая кожанка(+4)`.
   The other six (`Бутсы`, `Кастет`, `Дубинка`, `Тесак`, `Костюм Abibas`,
   `Кожанка`) are the same fifteen-string block and the same code path, but
   **were not observed on an oracle screen** -- they need a character carrying
   that particular item.

### What is still unverified

- `20ae:0b37` -- an unreferenced zero byte inside the price array.
- Which market boots row corresponds to the inventory line
  `Понтовые бутсы(Урон+2)`, hence the null price on that item.
- Six of the fifteen item strings were not seen on an oracle screen (above).
- The class-weight rows agree with `progress::CLASS_WEIGHTS` (Task 9b), but
  that is a *second reading of the same bytes*, not an independent check. The
  independent evidence for the weights is Task 9b's own oracle work.
