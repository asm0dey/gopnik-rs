# Понтовость: XP, level thresholds and stat growth (Task 9b)

Everything below is read out of `orig/g.exe` and checked against the original
running under the Task 3 oracle. Artifacts: `data/xp.json` (built by
`tools/capture_xp_cases.py`), port in `src/progress.rs`, tests in
`tests/progression.rs`.

Addresses are Ghidra `segment:offset`. The load image begins at file offset
`0x18d0` with code segment `1000`, so `1000:xxxx` is file offset
`0x18d0 + xxxx`; the data segment is Ghidra's `20ae`, i.e. file offset
`0x18d0 + (0x20ae - 0x1000) * 16 = 0x123b0` for `DS:0000`. That base is not
assumed: `tools/capture_xp_cases.py` refuses to read anything out of the
image unless the shortstring at `DS:002e` decodes to `Дохляк`.

## The state

| global | `.SAV` | meaning | evidence |
|---|---|---|---|
| `DS:389c` | `0x200` | class / rank index | `1000:25aa` indexes the weight table with it, `1000:712a` stores it |
| `DS:38a6` | `0x20a` | level (понтовость), 0..40 | `1000:258a` increments it, `1000:2580` caps it |
| `DS:38ce` | `0x232` | XP not yet spent on a level | `1000:2536`, `1000:254d` |
| `DS:38d0` | `0x234` | XP needed for the next level | `1000:2550`, `1000:6de0` |
| `DS:38d2` | `0x236` | growth log, `array[1..40] of string[2]` | `1000:2641`..`1000:267a` writes it |

The growth-log base is worth spelling out. The code computes the element
address as `level * 3 + 0x38cf` (`1000:2647`..`1000:2651`), which is Borland's
biased base for a one-based array: element 1 sits at `0x38d2`, i.e. `.SAV`
`0x236`. That is why the array does not collide with the XP words at `0x232`
and `0x234` — see "Cross-checks" for the decoded contents of all five saves.

## The threshold

There is no curve table in the image. The original stores the current
requirement and moves it:

* `1000:6de0` — `mov word [0x38d0],0xa`. A new character owes 10.
* `1000:2550` — `add word [0x38d0],0xa`, once per level gained.
* `1000:4ac7` — `sub word [0x38d0],0xa`, once per level *lost* to the flee
  penalty, which keeps the two in step downwards as well.

So the requirement at level *n* is **`10 + 10 * n`**, and
`xp_to_next(level) = 10 + 10 * level` in `src/progress.rs`.

The level and the threshold come apart in exactly one place: at the cap. The
draining loop (below) is uncapped, so at level 40 with the capped flag the
threshold keeps rising while the level does not. `Progress` therefore carries
the threshold as state rather than deriving it from the level;
`tests/progression.rs::level_cap_stops_the_level_but_not_the_threshold` pins
it.

## The award

`1000:51b9`..`1000:51c8`, inside `FUN_1000_3d11`:

```
award := enemy.strength + enemy.agility + enemy.vitality + enemy.luck
```

printed as `^6За отпин врага ты получаешь # качков опыта` (string `CS:398d`,
pushed at `1000:51b4`) and added to `DS:38ce` at `1000:51e9`.

Nothing between those two points reads the player's level. Thirty captured
kills at player levels 0, 1, 2, 10, 11, 15, 16, 20, 21, 30, 31 and 32 print
exactly this sum (`data/xp.json`, `award_cases`).

Two ways the award does not happen at all:

* `1000:51a6` jumps straight past it when the fight was the rector or the
  endgame (`param_1` 3 or 4). Those two paths instead force a level with
  `xp := threshold` and the **uncapped** flag (`1000:508e` and `1000:5094`,
  `1000:513f` and `1000:5145`).
* A second, unrelated XP source at `1000:582e` adds `chapter * 10` and then
  levels up capped (`1000:5832`, `1000:5835`). `chapter` is the byte at
  `DS:3692`, set to `level div 10 + 1` at `1000:6d93`. What triggers this
  path is not established here.

`1000:7fe4`..`1000:7fed` is a third forced level-up (`xp := threshold`, capped
flag). Its trigger is likewise not established.

## The level-up: `FUN_1000_2526` (`1000:2526`)

One argument, `param_1` at `[bp+4]`, which this port calls `uncapped`.

```
if xp < threshold then exit                     ; 1000:2536..1000:253c

levels := 0                                     ; 1000:2541
repeat                                          ; 1000:2546
  xp        := xp - threshold                   ; 1000:254d
  threshold := threshold + 10                   ; 1000:2550
  levels    := levels + 1                       ; 1000:2555
until xp < threshold                            ; 1000:255f

for i := 1 to levels do begin                   ; 1000:2567, 1000:2895
  if (param_1 = 0) and (level = 40) then break   ; 1000:257a..1000:2587
  level := level + 1                            ; 1000:258a
  write('^1Понтовость увеличивается: ')          ; 1000:2591, CS:248f
  sum := w[0] + w[1] + w[2] + w[3]              ; 1000:25aa..1000:25ee
  for k := 1 to 2 do begin                      ; 1000:25f1, 1000:287d
    r := Random(sum) + 1                        ; 1000:25fe, 1000:2603
    ...grant one stat, see below...
  end
  wait for a key                                ; 1000:2886, 1000:2890
end

if param_1 = 0 then
  write('^6Сейчас у тебя # качков опыта. До слеующей прокачки надо #',
        xp, threshold)                          ; 1000:28a6, CS:24ea
```

Two things to notice. The draining loop runs **before** and independently of
the cap, so XP consumed while at level 40 buys nothing. And the level cap is
the *only* thing the argument controls besides the closing message: the
rector and endgame kills can push a character past 40.

### The weight table

`1000:25aa`..`1000:25b6` reads `[[0x389c] * 4 + 2]`, and the three siblings at
`+3`, `+4`, `+5`. So the table is at `DS:0002`, four bytes per class, in
strength / agility / vitality / luck order. Read out of the image, with the
rank names from the 256-byte-stride table at `DS:002e` that the same index
selects (`1000:13dc`):

| class | rank name | str | agi | vit | luck |
|---|---|---:|---:|---:|---:|
| 0 | Дохляк | 1 | 2 | 1 | 2 |
| 1 | Нефор | 2 | 2 | 2 | 3 |
| 2 | Нарк | 2 | 2 | 2 | 2 |
| 3 | Подтсан | 3 | 3 | 3 | 3 |
| 4 | Отморозок | 5 | 2 | 4 | 1 |
| 5 | Гопник | 4 | 3 | 3 | 2 |
| 6 | Вор | 3 | 3 | 2 | 4 |
| 7 | Беспредельщик | 5 | 3 | 4 | 2 |
| 8 | Мент | 5 | 5 | 5 | 5 |
| 9 | Маньячок | 5 | 6 | 8 | 3 |
| 10 | Ректор НГУ | 0 | 0 | 0 | 0 |

The draw is `Random(sum) + 1`, so `1..sum`, tested against the running
prefix sums in that order (`1000:2615`, `1000:26c0`, `1000:275c`,
`1000:2814`). With all-zero weights every test fails and the draw grants
nothing; class 10 is the only such row and no player character holds it.

### What one stat increase does

| stat | code | effect | address |
|---|---|---|---|
| strength | `'1'` | `str+1`, `dmg_max+1`, `dmg_min+1` when the new `str` is even, `hpmax+1`, `hp+1` | `1000:261d`, `1000:267f`..`1000:2699` |
| agility | `'2'` | `agi+1` | `1000:26c5` |
| vitality | `'3'` | `vit+1`, `hpmax+5`, `hp+5` | `1000:2761`, `1000:27c3`..`1000:27c8` |
| luck | `'4'` | `luck+1` | `1000:2819` |

Messages, verbatim, with the code written next to each into the growth log:
`^1Сила +1 ` (`CS:24ac`, code `CS:24b7`), `^1Ловкость +1 ` (`CS:24b9`,
`CS:24c8`), `^1Живучесть +1 ` (`CS:24ca`, `CS:24da`), `^1Удача +1 `
(`CS:24dc`, `CS:24e8`). The log write is guarded by `level <= 40`
(`1000:263a`).

Neither branch clamps `hp` to `hpmax`; both rise by the same amount.

### Character creation (`1000:7140`..`1000:71e8`)

The baseline the growth runs from, and the reason the derived formulas hold.
The class prompt (`0-Пацан, 1-Отморозок, 2-Гопник, 3-Вор`) parses the typed
answer at `1000:7125`, clamps it to `0..=3` (`1000:712d`..`1000:713b`), stores
the four starting stats, then adds 3 (`1000:71b8`):

| answer | class stored | str | agi | vit | luck | address |
|---|---|---:|---:|---:|---:|---|
| 1 | 4, Отморозок | 5 | 2 | 4 | 1 | `1000:7148` |
| 2 | 5, Гопник | 4 | 3 | 3 | 2 | `1000:7167` |
| 3 | 6, Вор | 3 | 3 | 2 | 4 | `1000:7186` |
| other | 3, Подтсан | 3 | 3 | 3 | 3 | `1000:71a0` |

**The starting stats are the class's weight row.** That closes
`docs/re/save-format.md`'s open question about the class-choice → `rank_index`
mapping: the stored word is the answer plus 3, and it selects both the
displayed rank name and the growth weights.

Then, at `1000:71bd`..`1000:71e8`:

```
hpmax   := vitality * 5 + 10 + strength
hp      := hpmax
dmg_min := strength div 2
dmg_max := strength
```

and the level-up rules above preserve all four (each strength point adds 1 to
`hpmax`, each vitality point adds 5).

### The de-level penalty (`1000:4931`..`1000:4ad9`)

Named here because it is the level-up run backwards and it is what makes the
growth log necessary. It reads the log entry for the current level, clears it
(`1000:497d`), and for each of the two codes undoes that stat: `'1'` →
`str-1`, `dmg_max-1`, `dmg_min-1` when the new `str` is odd, `hpmax-1`, `hp`
clamped; `'2'` → `agi-1`; `'3'` → `vit-1`, `hpmax-5`, `hp` clamped; `'4'` →
`luck-1`. Then `level-1` (`1000:4ac3`), `threshold-10` (`1000:4ac7`), and
`xp := threshold - 1` if the XP total would otherwise still be over the lowered
threshold (`1000:4ad5`). Messages `^4Сила -1` (`CS:343c`) and siblings.
Modelling it is Task 11's; the rules are recorded here because they are the
mirror image of the ones this task ports.

## The post-kill stat-gain block

Where the twelve unmapped `Random` sites `docs/re/combat.md` lists sit, and
what the ones after the XP award do. Everything below runs *after*
`FUN_1000_2526` returns (`1000:523b`), so none of it is a level-up.

```
1000:523e  money and two other counters += the enemy's 0x396a/0x396c/0x396e
1000:526c  hp += 5, clamped to hpmax
1000:5280  [0x38cb] += (enemy.level div 3) + enemy.class + 1
1000:5295  chapter flag: if level - (chapter-1)*10 >= 3, set DS:3696 and say so
1000:52d5  Random(0x1e); anything but 0 skips the whole one-shot block
```

On a 0, the first of three one-shot events that has not fired yet fires:

| event | flag | `.SAV` | grants | address |
|---|---|---|---|---|
| 1 | `DS:38bf` | `0x223` | +1 to each stat, `hpmax`/`hp` +6, `dmg_max` +1, `dmg_min` += `1 - str mod 2` | `1000:532f`..`1000:5361` |
| 2 | `DS:38c0` | `0x224` | +4 to each stat, `hpmax`/`hp` +24, `dmg_max` +4, `dmg_min` +2 | `1000:538a`..`1000:53b1` |
| 3 | `DS:38c1` | `0x225` | text only | `1000:53c0`..`1000:53f2` |

`data/xp.json`'s `post_kill_stat_events` holds these deltas, **decoded out of
the instructions** by `tools/capture_xp_cases.py` rather than transcribed, so
a misreading of the listing cannot reach the artifact.

The rest of the block is loot, gated on luck against a chapter-scaled roll:

| address | draw | what it gates |
|---|---|---|
| `1000:5402` | `Random(chapter * 0x19)` | if `luck` beats it and the enemy is class 2 (Нарк) |
| `1000:5427` | `Random(3)` | joints looted, added to `DS:38c5` |
| `1000:5454` | `Random(chapter * 0x28)` | if `luck` beats it and the enemy is class 1 (Нефор) |
| `1000:5482` | `Random(3)` | picks one of three one-shot rewards: `luck+2` (`1000:5493`, flag `DS:38bd`), `luck+1` (`1000:54c4`, flag `DS:38be`), or a third that grants no stat (`1000:54ed`) |
| `1000:5530` | `Random(2)` | enemy classes 3..6: one-shot weapons, `dmg_min`/`dmg_max` +2 or +4 (`1000:5574`, `1000:55da`, `1000:55e6`) |
| `1000:5617` | `Random(2)` | enemy class 7: two more one-shot items |
| `1000:5681` | `Random(2)` | enemy class 9: a one-shot item, `dmg_min`/`dmg_max` +2 or +4 (`1000:56cf`, `1000:56e0`) |

`chapter` is `DS:3692` again. The remaining four unmapped sites
(`1000:4db7`, `1000:4e16`, `1000:4ef5`, `1000:4f18`) are in the flee/other
command handlers and are still unmapped.

## Where the ground truth came from

Three sources, none of them `src/progress.rs`.

**1. The load image.** `tools/capture_xp_cases.py` reads the class weight
table, the rank names, the four sets of starting stats and the post-kill event
deltas straight out of `orig/g.exe` at fixed offsets, checking the opcode
bytes at each site before taking an immediate. A wrong offset is an error, not
a wrong number.

**2. The Task 3 oracle.** Seven pinned-seed runs of the original under
DOSBox-X (`data/xp.json`, `level_up_cases`), 30 kills. Per kill: the award and
the status line off the 80x25 text screen, and the player record, the enemy
record, the XP total and the threshold out of the guest's data segment either
side. The runs start either fresh (district 1) or from a shipped save
(districts 0, 2, 3, 4), which is how the capture reaches levels 15, 20 and 30
inside a 1024-keystroke script. All 30 agree with the formulas above:
`award = sum of the enemy's four stats`, `threshold = 10 + 10 * level` at
every level seen, `xp -= threshold` / `threshold += 10` per level, two
`+1` messages per level.

**3. The five shipped saves.** `(level, threshold)` at `0x20a` and `0x234`:
`(15,160) (10,110) (20,210) (30,310) (40,410)`. And the growth log at `0x236`,
which decodes cleanly in all five: exactly `level` entries of exactly two
codes drawn from `'1'`..`'4'`, and nothing after them. `SAVE_R2`, `SAVE_R3`
and `SAVE_R4` share a prefix — R3's first ten entries are R2's ten, R4's first
twenty are R3's twenty — so they are three snapshots of one character.

### Coverage, honestly

Thresholds are witnessed at levels 0, 1, 2, 10, 11, 15, 16, 20, 21, 30, 31,
32 (oracle) and 10, 15, 20, 30, 40 (saves). **Levels 3..9, 12..14, 17..19,
22..29 and 33..39 were never reached**: a fresh character dies before it can
grind that far inside one keystroke script, and no shipped save sits there.
Those entries in `data/xp.json` are marked `UNVERIFIED by observation` and
carry only what `1000:6de0` and `1000:2550` predict. Nothing was invented to
fill the table.

The generator state at each captured level-up was not recorded, so the two
stat draws are *not* replayed against the original. What is checked instead is
stronger than nothing and weaker than a full replay: the stats the screen
announced, pushed through `grant`, land exactly on the stats the guest's own
memory held afterwards, in all 30 cases.

## Cross-checks

**`SAVE_R0` rebuilt from scratch.** `tests/progression.rs::save_r0_rebuilds_from_its_growth_log`
starts at `new_character(1)` — the answer that stores the class `SAVE_R0`
holds — replays the 30 stat grants its growth log records, applies the three
one-shot post-kill events whose flag bytes the file has set, and lands on
`(24, 13, 19, 7)` with `hpmax` 129: the file's stats, to the point, with no
residual. Growth log, flags and target stats are bytes in a file that shipped
with the game; the rules come out of the executable.

The other four saves do not close to zero — they carry extra stats from
sources this task did not map (the shop and the gym, Task 10/11 territory) —
so only `SAVE_R0` is asserted. R2, for instance, is +5 strength and +4
vitality over what its log and flags account for.

**The `hpmax` discrepancy is resolved.** Task 9 left it open that
`hpmax = 10 + 5*vitality + strength` holds for `SAVE_R0`, `SAVE_R3` and
`SAVE_R5` but is 2 low for `SAVE_R2` and `SAVE_R4`. The cause is a temporary
buff:

* `1000:4b57` — using a consumable (`dec word [0x38c5]` at `1000:4b4e`) adds
  `+2 strength`, `+1 dmg_min`, `+2 dmg_max` and sets a countdown byte
  `[0x38cd] := 3` (`1000:4b52`). It does **not** touch `hpmax`.
* `1000:aeb3` — when the countdown reaches 0 it subtracts the same
  `2 / 1 / 2` back. It does not touch `hpmax` either.

So while the buff is up, `strength` is 2 higher than `hpmax` reflects. The
countdown byte is `.SAV 0x231`, and it is nonzero in exactly `SAVE_R2` (1) and
`SAVE_R4` (2) and zero in the other three — the same two saves, and only those
two. With that one term, the identity holds on all five:

| save | vit | str | `.SAV 0x231` | `10 + 5*vit + str - 2*buff` | actual `hpmax` |
|---|---:|---:|---:|---:|---:|
| `SAVE_R0` | 19 | 24 | 0 | 129 | 129 |
| `SAVE_R2` | 15 | 16 | 1 | 99 | 99 |
| `SAVE_R3` | 28 | 28 | 0 | 178 | 178 |
| `SAVE_R4` | 44 | 42 | 2 | 270 | 270 |
| `SAVE_R5` | 45 | 90 | 0 | 325 | 325 |

`tests/progression.rs::reference_saves_agree_with_the_curve` asserts it.

The damage range corroborates the same reading. Because the buff moves
`strength` by 2 and `dmg_min` by 1, both `dmg_max - strength` and
`dmg_min - strength div 2` are unaffected by it, and they agree on a single
weapon bonus per save: 5, 3, 10, 11, 12 for R0, R2, R3, R4, R5.

## Open questions

* **The two draws per level are not replayed against the original.** See
  "Coverage, honestly" above. Closing this needs `RandSeed` captured at the
  key read immediately before a level-up, which the current script shape does
  not guarantee.
* **What triggers the two forced level-ups** at `1000:582e` and `1000:7fe4`,
  and what `DS:38cb` (`.SAV 0x22f`) counts.
* **`1000:4db7`, `1000:4e16`, `1000:4ef5`, `1000:4f18`** — the four `Random`
  sites in the flee and other command handlers are still unmapped.
* **Levels 3..9 and the other gaps** in the threshold table are predictions,
  not observations.
* **The rest of the 162-byte tail.** The growth log (`0x236`..`0x2ad`), the XP
  words (`0x232`, `0x234`), the buff countdown (`0x231`) and the four one-shot
  flags (`0x221`..`0x225`) are now named; the remaining bytes are not.
