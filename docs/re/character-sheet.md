# `FUN_1000_1a03` — the character sheet

2700 bytes, 83 conditional branches (10% of the game's 838), third-largest
function in the binary, and until Task 16 nothing under `docs/re/` said a word
about it. This is what it is.

Machine-readable twin: **`data/character_sheet.json`**. Every address and every
Russian literal below is also a record there, and
`python3 tools/test_character_sheet.py` re-derives all of it from `orig/g.exe` —
alignment (the address is a boundary an aligned walk from the enclosing
function's entry reaches) *and* identity (the instruction there says what the
record says). Four cases in `tools/mutations.json` show those checks going red.

Addresses are Ghidra form A; `tools/addr.py` is the executable authority.
`CS <hex>` offsets are image offsets inside the game's code segment, `20ae:`
offsets are DGROUP. Russian is verbatim, typos included.

---

## The hypothesis this refutes

`docs/superpowers/RESUME.md` carried, tier `unverified`:

> it is the character-sheet / stats renderer — the body behind `stats` from the
> main loop and `sv` ("size up the enemy") from combat.

**Half right, and the interesting half is wrong.** It *is* the character-sheet
renderer. But:

- **`stats` is not a verb.** The five bytes `stats` do not occur anywhere in
  `orig/g.exe`, so no dispatcher can match them. The verb is **`s`**.
  *(Established from flow: `1000:ec82` compares `DS:3972` against the CS
  literal at `0x9f85`, which is the one-character shortstring `s`; the whole
  image is scanned for `stats` in
  `tools/test_character_sheet.py::test_the_live_probe_agrees_with_the_static_call_sites`.)*
- **`sv` does not reach this function at all.** `1000:4c42` compares the combat
  buffer against the CS literal at `0x35a1` (`sv`) and, on a match,
  `1000:4c49` calls **`FUN_1000_1348`** — a different 791-byte function. That
  is where "size up the enemy" lives. *(Established from flow;
  `1000:4c49` is `e8 fc c6`, target `1000:1348`.)* This was already
  half-written down: `docs/re/gaps.md`'s in-combat verb table has carried the
  `s → 1000:4c35 call 0x1a03` and `sv → 1000:4c49 call 0x1348` rows since the
  final-review fix wave. What was new in Task 16 is that nobody had read those
  two rows against the `RESUME.md` hypothesis, and that the breakpoint now
  settles it live as well as statically.

The `sv` half was at risk and lost. `s` at the *combat* prompt is what shows
the player's own sheet mid-fight (`1000:4c2e` / `1000:4c35`).

## Step 1 — the live probe, including its negative

**Established from flow.** `tools/rngtrace/verbprobe.py` breaks on three
addresses under qemu+gdb — `1000:ae63` (the street prompt's `ReadLn`, tag `P`),
`1000:441d` (the `Битва\` prompt's `ReadLn`, tag `C`), and `1000:1a03` itself
(tag `T`) — types a scripted verb list, and credits every `T` to the line typed
at the prompt stop that precedes it. gdb stops at the `ReadLn` *call*, before
the line is read, so the window is unambiguous; the driver's screen
classification is cross-checked against the guest's own markers position by
position, and a mismatch fails the run rather than being attributed.

Seed `0x12345678`, a fresh Пацан (class 3, read from the guest's `DS:389c`).
Marker stream, identical across two runs:

```
P T P P P P T P C C C T C P
```

| prompt | line typed | prompts | entries at `1000:1a03` | |
|---|---|---:|---:|---|
| street | `s` | 2 | **2** | reaches |
| street | `stats` | 1 | 0 | does **not** reach |
| street | `i` | 1 | 0 | does **not** reach |
| street | `w` | 2 | 0 | does **not** reach |
| combat | `s` | 1 | **1** | reaches |
| combat | `sv` | 2 | 0 | does **not** reach |
| combat | `run` | 1 | 0 | does **not** reach |

Five negatives against two positives, which is what makes this an experiment
rather than a demonstration: a run showing only `s` reaching it could not tell
that apart from everything reaching it. A breakpoint that did **not** fire is
flow-tier evidence for a negative; a screen never is
(`docs/re/METHODOLOGY.md`).

`tools/test_character_sheet.py` asserts the two lanes agree: for each probed
`(prompt, verb)` pair, the observed answer must equal the answer the
disassembly predicts from the call-site table below.

---

## The entry, and the argument convention

**This function takes no arguments at all.** Three independent readings, all
flow-tier:

1. The epilogue is `1000:248b mov sp,bp` / `1000:248d pop bp` / `1000:248e ret`
   — a **bare `ret`, no immediate**. Turbo Pascal is callee-cleanup, so a
   parameterised routine ends in `ret n`.
2. **No instruction in the whole 2700 bytes uses a positive `bp` displacement.**
   That is where a Turbo Pascal parameter lives (`[bp+4]`, `[bp+6]`), and the
   sweep over every decoded instruction in `[1000:1a03, 1000:248f)` finds none.
   The 83 branches' `[bp+…]`-shaped guards are all *negative* —
   `[bp-0x104]`, `[bp-0x106]` — locals in the `0x606` bytes the prologue
   reserves.
3. No register argument either: `1000:1a06 mov ax,0x606` overwrites `ax` as the
   third instruction, before anything reads it, and `1000:1a12 mov di,0x165f`
   does the same for `di`.

So the difference between the call sites **cannot** be an argument — which
settles the brief's "most valuable single thing" question with a negative. The
function reads the player's globals and nothing else, so all four call sites
render the same sheet. What varies between `s` and `sv` is *which function is
called*, not what is passed to it.

Prologue: `55` / `89 e5` / `b8 06 06` / `9a cd 02 78 0f` (`rtl_stack_check`,
`docs/re/rtl.md`) / `81 ec 06 06`.

## The four call sites

`data/character_sheet.json` records them and the test re-scans the whole image
for a fifth — near (`e8`, matched **modulo 64 KiB**, which is how the two in
`entry` are encoded) and far (`9a`). There is none.

| site | in | reached by |
|---|---|---|
| `1000:ec89` | `entry` | the verb **`s`** at the top-level `\` prompt. Compare `1000:ec82` (`rtl_str_compare`, `DS:3972` vs CS `0x9f85` = `s`), branch `1000:ec87 jnz 0xec8c`. |
| `1000:ee36` | `entry` | the quit tail — `^6Блин не быть тебе нормальным пацаном` (CS `0xab23`), `^1А результат:` (CS `0xab4a`), the sheet, `ReadKey` (`1000:ee39`), `Halt` (`1000:ee43`). |
| `1000:4c35` | `FUN_1000_3d11` | the verb **`s`** at the `Битва\` prompt. Compare `1000:4c2e` (`DS:3a72` vs CS `0x359f` = `s`), branch `1000:4c33 jnz 0x4c38`. |
| `1000:512b` | `FUN_1000_3d11` | the **rector-victory ending** — see below. |

Both `entry` sites decode as `call 0x11a03` in `tools/dis16.py` *and* in
`capstone`: the two decoders agree, and the rendering is an un-wrapped buffer
offset, not a disagreement. The real target is `0x11a03 & 0xFFFF = 0x1a03`.
A byte scan that requires the un-wrapped equality misses both — that is how
`docs/superpowers/RESUME.md`'s "called by exactly two things" stayed plausible
while naming the wrong pair.

### The rector-victory ending, `1000:512b`

**Established from flow.** This is a *different* branch from
`docs/re/gaps.md`'s "rector death branch" (`1000:4f8c`, `[0x3c83] == 1`, the
player dying); this is the arm where the player WINS against opponent kind 4,
and nothing under `docs/re/` described it before. Inside combat, after the blow
loop:

- `1000:507b cmp word [0x3962],0x0` / `1000:5080 jle 0x5085` — the enemy is
  down.
- `1000:5085 cmp byte [bp+0x4],0x4` / `1000:5089 jz 0x508e` — the opponent kind
  is 4. (`FUN_1000_3d11` ends in `ret 0x2` at `1000:5849`, so `[bp+0x4]` is its
  one word parameter; each of its seven call sites pushes a `mov al,K` /
  `push ax` pair with K in 0..6, and `1000:ae36` is the one that pushes 4.)
- `1000:508e`..`1000:5097`: `[0x38ce] := [0x38d0]` (xp set to the threshold),
  then `call 0x2526` with `al = 1`.
- Then four lines and the sheet:
  `^1Ты замочил самого ректора!!! ТЫ САМЫЙ КРУТОЙ!!!` (CS `0x3862`),
  `^1Вновь сила торжествует над интелектом.` (CS `0x3894`),
  `^1После этого сразу началась анархия и полный беспредел.` (CS `0x38bd`),
  `^1И не стыдно тебе гоп чёртов?` (CS `0x38f6`),
  `^1А результат:` (CS `0x3915`), each followed by `ReadKey` (`0f16:031a`).
- `1000:512b` prints the sheet, `1000:512e` `ReadKey`, `1000:5133` calls
  `FUN_1000_0aec`, `1000:5136` jumps to `1000:5838`.

The losing counterpart is two instructions earlier in the same block:
`1000:5053` writes `^4Ты сдох.` (CS `0x3857`) and `1000:5074` calls
`FUN_1000_074b` — already known.

## It reaches no game code

**Established from flow.** Every call the body makes is a far call, and the
segments are exactly `0eed` and `0f78` — the runtime. There is not one near
call in the 2700 bytes. Counts:

| routine | calls | what (`docs/re/rtl.md`, Task 11h) |
|---|---:|---|
| `0eed:0000` | 30 | `Write` — the game's colour-code-aware formatter |
| `0f78:0b66` | 21 | `rtl_str_append` |
| `0eed:01c2` | 20 | `WriteLn` — same formatter, with a newline |
| `0f78:0ae7` | 9 | `rtl_str_assign` |
| `0f78:0c03` | 6 | `rtl_char_to_str` |
| `0f78:05dd` + `0f78:0291` | 6 + 6 | Borland `WriteLn` on the text file at `DS:3fcc` (the same one `data/wander.json` records at `1000:828c`) + the `{$I+}` check — the blank lines between sections |
| `0f78:0b01` | 5 | `rtl_str_assign_max` |
| `0f78:1125` | 4 | `rtl_real_op_from_longint` |
| `0f78:1121` / `0f78:1117` | 2 / 2 | `rtl_real_op_cmp` / `rtl_real_op_div` |
| `0f78:02cd` | 1 | `rtl_stack_check` (the prologue) |

The `Write`/`WriteLn` calling shape, read off the sites: a string is
accumulated by chained `rtl_str_append`, then up to five word arguments are
pushed, and `#` placeholders in the string take them in order — e.g.
`Урон #-#    ` with `[0x38a8]` and `[0x38aa]`.

## The two table lookups

**Established from flow.** Both are `index * 256 + base`, the stride
`docs/re/string-tables.md` already records.

| at | index | table | first instructions |
|---|---|---|---|
| `1000:1a36` | `20ae:389c`, the player's **class** | `ranks`, base `20ae:002e`, 11 entries | `8b 3e 9c 38` / `b1 08` / `d3 e7` / `81 c7 2e 00` |
| `1000:1a53` | `20ae:38a6`, the player's **level** | `krutizna`, base `20ae:0b42`, 43 entries | `8b 3e a6 38` / `b1 08` / `d3 e7` / `81 c7 42 0b` |

`class 3 → Подтсан`, `class 6 → Вор`; `level 0 → Опущеный`,
`level 11 → Нормальный Чувак`. The test reads those out of DGROUP.

Task 16's brief called `1000:1a36` "the player rank-name lookup". It is the
**class**-name lookup; the rank ladder (`krutizna`) is `1000:1a53`, 29 bytes
later.

## The sheet, line by line

The header, then the name, then experience, then the stat line, then the flag
lines below.

**Header** — `^2Ты ` (CS `0x1664`) + `ranks[class]` + ` # уровня - `
(CS `0x166a`) + `krutizna[level]`, with `[0x38a6]` pushed at `1000:1a66` and
`WriteLn` at `1000:1a76`.

**Name** — `^2А зовут тебя: ` (CS `0x1677`) + the string at `DS:379c`,
appended at `1000:1a90`, `WriteLn` at `1000:1aa4`.

**Experience** — guarded by `1000:1aa9 cmp word [0x38a6],0x27` /
`1000:1aae jg 0x1acb`: at level > 39 the line is skipped, which is what a
43-entry ladder with no next threshold needs.
`^6Сейчас у тебя # опыта, А для прокачки надо #` (CS `0x1688`) with `[0x38ce]`
and `[0x38d0]`.

**The stat line and its colour string.** `1000:1a12` assigns the CS literal
`7777` (CS `0x165f`) into the local at `[bp-0x100]`. Each of its four
characters is a Turbo colour digit, patched to `1` when a worn item boosts that
stat, and the format string interleaves them: `Сл:^` (CS `0x16b7`) + char,
`#^7 Лв:^` (CS `0x16bc`) + char, `#^7 Жв:^` (CS `0x16c5`) + char,
`#^7 Уд:^` (CS `0x16ce`) + char, `#` (CS `0x16d7`); then `[0x389e]`,
`[0x38a0]`, `[0x38a2]`, `[0x38a4]` at `1000:1baa`..`1000:1bb6` and `WriteLn` at
`1000:1bbd`.

| slot | stat | patched to `1` at | when any of |
|---|---|---|---|
| 0 | Сила `20ae:389e` | `1000:1ae0` | `20ae:38cd`, `20ae:38bf`, `20ae:38c0` |
| 1 | Ловкость `20ae:38a0` | `1000:1af3` | `20ae:38bf`, `20ae:38c0` |
| 2 | Живучесть `20ae:38a2` | `1000:1af8` | `20ae:38bf`, `20ae:38c0` |
| 3 | Удача `20ae:38a4` | `1000:1b19` | `20ae:38bd`, `20ae:38be`, `20ae:38bf`, `20ae:38c0` |

That is internally consistent with the item labels the same function prints
further down — `Кольцо "Пг"(Всё +1)` is `20ae:38bf` and highlights all four
slots; `Крестик(Удача +2)` is `20ae:38bd` and highlights only Удача — which is
what makes the flag→label attribution below more than a guess.

### Flag lines

**Established from flow.** For each row, the guard's operand *is* the DS
address, and the literal push sits inside the arm that guard selects — both
checked mechanically, so a label attributed to the wrong flag fails the test.

| DS byte/word | line | guard | guard instruction | literal pushed at | text |
|---|---|---|---|---|---|
| `20ae:38bd` | Крестик | `1000:1be9` | `cmp byte [0x38bd],0x0` | `1000:1bf0` | `^1Крестик(Удача +2) ` |
| `20ae:38be` | Кольцо "Гс" | `1000:1c09` | `cmp byte [0x38be],0x0` | `1000:1c10` | `^1Кольцо "Гс"(Удача +1) ` |
| `20ae:38bf` | Кольцо "Пг" | `1000:1c69` | `cmp byte [0x38bf],0x0` | `1000:1c70` | `^1Кольцо "Пг"(Всё +1) ` |
| `20ae:38c0` | Мега Кольцо | `1000:1c89` | `cmp byte [0x38c0],0x0` | `1000:1c90` | `^1Мега Кольцо(Всё +4) ` |
| `20ae:38c1` | Кольцо "Гп" | `1000:1ca9` | `cmp byte [0x38c1],0x0` | `1000:1cb0` | `^1Кольцо "Гп"(Самолечение) ` |
| `20ae:38bb` | мобильник | `1000:1cd8` | `cmp byte [0x38bb],0x0` | `1000:1cdf` | `^1У тебя есть мобильник` |
| `20ae:38b3` | тёмные очки | `1000:1cf8` | `cmp byte [0x38b3],0x0` | `1000:1cff` | `^1У тебя есть тёмные очки` |
| `20ae:38bc` | наколка | `1000:1d18` | `cmp byte [0x38bc],0x0` | `1000:1d1f` | `^1На тебе зоновская наколка` |
| `20ae:394d` | пистолет | `1000:1d38` | `cmp byte [0x394d],0x0` | `1000:1d51` | `^1У тебя есть пистолет` |
| `20ae:394e` | глушитель | `1000:1d6a` | `cmp byte [0x394e],0x0` | `1000:1d71` | `^1 с гушителем` |
| `20ae:394f` | патроны | `1000:1d8a` | `cmp word [0x394f],0x0` | `1000:1d91` | `^1! патронов - #` |
| `20ae:38b5` | Бутсы | `1000:1e81` | `cmp byte [0x38b5],0x0` | `1000:1e8f` | `^1Бутсы(+1) ` |
| `20ae:38b8` | Понтовые бутсы | `1000:1ecf` | `cmp byte [0x38b8],0x0` | `1000:1ed6` | `^1Понтовые бутсы(Урон+2) ` |
| `20ae:38ba` | Кастет | `1000:1eef` | `cmp byte [0x38ba],0x0` | `1000:1f0b` | `^1Кастет(+2) ` |
| `20ae:394b` | Дубинка | `1000:1f59` | `cmp byte [0x394b],0x0` | `1000:1f6e` | `^1Дубинка(+4)  ` |
| `20ae:38c2` | Нож | `1000:1fb5` | `cmp byte [0x38c2],0x0` | `1000:1fc3` | `^1Нож(+6) ` |
| `20ae:394c` | Тесак | `1000:2003` | `cmp byte [0x394c],0x0` | `1000:200a` | `^1Тесак(Урон+9) ` |
| `20ae:38b0` | сломанная челюсть | `1000:2037` | `cmp byte [0x38b0],0x1` | `1000:204f` | `^4Сломана челюсть  ` |
| `20ae:394a` | зубная защита | `1000:2068` | `cmp byte [0x394a],0x1` | `1000:2080` | `^1Зубная защита  ` |
| `20ae:38b1` | сломанная нога | `1000:2099` | `cmp byte [0x38b1],0x1` | `1000:20b1` | `^4Сломана нога  ` |
| `20ae:38cd` | обдолбанность | `1000:20ca` | `cmp byte [0x38cd],0x0` | `1000:20e2` | `^6Обдолбаный  ` |
| `20ae:38b2` | броня | `1000:227b` | `cmp byte [0x38b2],0x0` | `1000:2285` | `^2Броня #    ` |
| `20ae:38b4` | костюм Abibas | `1000:22a1` | `cmp byte [0x38b4],0x0` | `1000:22e3` | `^1Костюм Abibas(+1) ` |
| `20ae:38b7` | костюм Adidas | `1000:22fc` | `cmp byte [0x38b7],0x0` | `1000:230a` | `^1Костюм Adidas(+2) ` |
| `20ae:38b6` | кожанка | `1000:2323` | `cmp byte [0x38b6],0x0` | `1000:2365` | `^1Кожанка(+2) ` |
| `20ae:38b9` | крутая кожанка | `1000:237e` | `cmp byte [0x38b9],0x0` | `1000:238c` | `^1Крутая кожанка(+4) ` |
| `20ae:38c5` | косяки | `1000:23b4` | `cmp word [0x38c5],0x0` | `1000:23bb` | `Косяки #` |
| `20ae:38c3` | пиво | `1000:23d5` | `cmp word [0x38c3],0x0` | `1000:23dc` | `Пиво #.#л.` |
| `20ae:38c7` | бабки | `1000:242e` | `cmp word [0x38c7],0x0` | `1000:2435` | `Бабки #` |
| `20ae:38c9` | хлам | `1000:246a` | `cmp word [0x38c9],0x0` | `1000:2471` | `Хлам #` |

**Four of these are new to `docs/re/`.** `git grep` over `docs/re/*.md` finds
`20ae:38b5` (Бутсы), `20ae:38b8` (Понтовые бутсы), `20ae:394e` (глушитель) and
`20ae:394f` (the patron count) in this file and nowhere else.
`20ae:394a` was known to the tooling as `p_tooth_guard_394a`
(`tools/rngtrace/fightrun.py`) but was never named under `docs/re/`; the sheet
labels it `Зубная защита` itself.

The rest corroborate existing findings from a second, independent site:
`20ae:38b3` (тёмные очки, `docs/re/combat.md`), `20ae:38ba`/`20ae:394b` and
`20ae:38c2`/`20ae:394c` (the four hand weapons, same file),
`20ae:38b6`/`20ae:38b9` (the two jackets, `docs/re/difftest.md`), and
`20ae:38b2` — recorded in Task 11c as "the armour byte, not `unk_38b2`" —
which the sheet prints as `Броня #`.

**The best-item-wins pattern.** Weapons, suits and jackets come in pairs, and
the sheet prints the superseded one in `^4` (dim) beside the good one. The
shape is always two arms: `[lesser] and not [better]` → the bright label;
`[lesser] and [better]` → the dim label, then the better one's bright label.
E.g. `1000:1e81`/`1000:1e88` → `^1Бутсы(+1) `; `1000:1ea8`/`1000:1eaf` →
`^4Бутсы ` (CS `0x183b`); `1000:1ecf` → `^1Понтовые бутсы(Урон+2) `. The dim
arms are among the branches left uncited — see below.

## Three derived values, and not a single `Random`

The function draws **no** random number: `0f78:114b` is not among its call
targets. Everything below is arithmetic on the globals.

**Пиво is stored in half-litres.** `Пиво #.#л.` (CS `0x19d1`) is fed
`[0x38c3] div 2` (`1000:23e5`/`1000:23e8`) and
`((([0x38c3] div 2) remainder) * 5) mod 10` (`1000:23f4`..`1000:2403`) — so an
odd count prints `.5`. This is the sheet corroborating, from a second site,
what `docs/re/gaps.md` records about `20ae:38c3`.

**Accuracy, from Ловкость alone.** `1000:21b0` copies `[0x38a0]` into
`[bp-0x104]`.

- `1000:21b7 cmp word [bp-0x104],0xe` / `1000:21bc jg 0x21e7`. At agility ≤ 14:
  `Точность #%` (CS `0x1909`) with `agility * 5 + 20` — `1000:21c9`/`21cb` are
  the two `shl ax,1`, `1000:21cd add ax,si` makes ×5, `1000:21cf add ax,0x14`.
- Above 14: `Точность 90% ` (CS `0x1915`) with no newline, then
  `1000:2204 sub ax,0xe`, `1000:220b` sets the hit count to 1, and
  `1000:2211 cmp word [bp-0x104],0x12` / `1000:2216 jle 0x2223` /
  `1000:2218 sub word [bp-0x104],0x12` / `1000:221d inc [bp-0x106]` is a loop
  that spends 18 points per extra hit.
- One extra hit (`1000:2223`/`1000:2228`): `   Второй удар #%` (CS `0x1923`)
  with `remainder * 5`.
- More (`1000:224d`/`1000:2252`): `- # ударов,  Точность # удара #%`
  (CS `0x1935`) with the hit count, the hit count + 1, and `remainder * 5`.

**The health line's colour.** `[bp-0x101]` starts at `'4'` (`1000:20fb`), then
`hp / hpmax` is computed twice in 6-byte reals — `[0x38ac]` and `[0x38ae]`
through `rtl_real_op_from_longint` (`1000:2104`, `1000:2110`), divided by
`rtl_real_op_div` (`1000:2118`), compared by `rtl_real_op_cmp` (`1000:2124`)
— and the digit becomes `'6'` above the first constant (`1000:212b`) and `'2'`
above the second (`1000:215b`). The two comparands differ in exactly one
register: `1000:211d mov cx,0x7f` versus `1000:214d mov cx,0x80`, with
`si = di = 0` both times.

**What is NOT established: the decimal value of those two constants.**
`docs/re/rtl.md` already records that reading a 6-byte real as a decimal needs
the layout confirmed against a known value, and that has not been done. The
ordering *is* established — the second threshold is strictly above the first,
since only the exponent-carrying register differs and it differs by one — but
"25% and 50%" would be a guess. Registered in `docs/re/gaps.md`.

## The branch skeleton: 59 of 83 cited, 24 left

`data/character_sheet.json`'s `branch_partition` holds the split, recomputed by
the test from `data/branches.json` and from the artifact's own citations (the
partition node itself is excluded from that scan — leaving it in made the
check agree with a recomputation its own contents had produced, which the test
caught while it was being written).

Every one of the 83 is a comparison against a global, against one of the two
accuracy locals, or against the flags `rtl_real_op_cmp` left; there is no
indirect dispatch and no computed jump anywhere in the body — a scan of every
decoded instruction in `[1000:1a03, 1000:248f)` finds no register- or
memory-operand `jmp` or `call`. Grouped:

- **RTL plumbing:** none. Not one of the 83 is a call-return test.
- **Real game-state tests, cited:** the 30 flag-line guards, the 9 stat-colour
  guards (`1000:1acb`..`1000:1b12`), the experience gate (`1000:1aa9`), the two
  health-colour comparisons (`1000:2129`, `1000:2159` — `blocked_by_call` in
  `data/branches.json`, because the flags come from `rtl_real_op_cmp`), and the
  four accuracy branches.
- **Formatting decisions, left uncited (24):** the section-header disjunctions
  (`Феньки: ` at `1000:1bc2`/`1000:1bc9`, `Мощные феньки: ` at
  `1000:1c38`..`1000:1c46`, the weapon line at `1000:1e06`..`1000:1e30`), the
  ammo-quantity flavour (`1000:1dab`..`1000:1dd7`), and the dim `^4` arms of
  the best-item-wins pairs. They decide whether a *header* prints and in which
  colour; none of them reads a global the cited arms do not already read.
  `docs/re/gaps.md` says what would settle them.

## What this changes elsewhere

- `docs/superpowers/RESUME.md`'s "called by exactly two things (`entry` and
  combat)" was right about the two functions and wrong about which line
  reaches it; its `stats`/`sv` hypothesis is refuted above.
- `docs/re/gaps.md`'s "`sv`/`v`/`x`/`wes` dispatcher sites" entry: `sv` is now
  settled — `1000:4c42` compares it, `1000:4c49` calls `FUN_1000_1348`. `v` is
  `1000:4caa` in combat (CS literal `0x35c6`, `v`; branch `1000:4caf jz 0x4cb4`,
  and the handler's first act is `1000:4cb4 cmp byte [0x3696],0x1`, the den
  flag); `x` and `wes` are `1000:ce80` (CS `0x96ce`) and `1000:ced8`
  (CS `0x970a`) in `entry`. Those three are cited compare sites, not mapped
  bodies.
- The rector-**victory** arm (`1000:5085`, opponent kind 4 with the enemy at 0
  hp) is mapped above and was not described anywhere before. It is not the same
  branch as `docs/re/gaps.md`'s "rector death branch" (`1000:4f8c`), which is
  still open, and the hospital-rescue arm is untouched.
