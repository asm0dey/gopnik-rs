# Combat (Task 9)

The combat routine is **`FUN_1000_3d11`, Ghidra address `1000:3d11`**, in
memory block `CODE_0`. It spans `1000:3d11`..`1000:584c` (6971 bytes) and is
called exactly once, from `entry`. It is not only the blow loop: it is the
whole battle sub-loop, including the `Битва\` prompt, the command dispatch,
fleeing, and the post-kill XP award.

Ported to `src/combat.rs` and `src/model.rs`; vectors in
`data/combat_vectors.json`, captured by `tools/capture_combat_vectors.py`.

The **command dispatcher** half of it — the verb table, the flee arm, the `v`
backup arc, the pistol, and the four `Random` sites this file could only
inventory — is mapped in **`docs/re/combat-dispatch.md`** (Task 17), with
`data/combat_dispatch.json` and `tools/test_combat_dispatch.py` beside it.

## How the function was identified

Not by size, and not by assuming Task 4's guess. Task 4's
`docs/re/functions.md` explicitly refused to name a combat function and
listed `1000:1a03`, `1000:6a0d`, `1000:7c67` as candidates — none of which is
right.

The identification is a string cross-reference. `data/string_pointers_audit.tsv`
(Task 4b) records, for every bare immediate operand in the disassembly, the
instruction address and the string offset it resolves to. Five combat strings
resolve to instructions inside `1000:3d11`'s body and nowhere else:

| string (file offset) | text | referencing instruction |
|---|---|---|
| `0x4B13` | `^4Ты промазал` | `1000:460b` `MOV DI,0x3243` |
| `0x4C49` | `^2Враг промазал` | `1000:4888` `MOV DI,0x3379` |
| `0x4B67` | `^4Тебе не хило врезали!` | `1000:4730` `MOV DI,0x3297` |
| `0x46BC` | `^2Из-за твоей хорошей ловкости враг сможет пнуть тебя раз # вместо #` | `1000:4013` `MOV DI,0x2dec` |
| `0x4701` | `^4Из-за хорошей ловкости врага ты сможешь пнуть его раз # вместо #` | `1000:40b6` `MOV DI,0x2e31` |

Reproduce with:

```bash
for o in 0x4B13 0x4C49 0x4B67 0x46BC 0x4701; do
    grep -P "\t$o\t" data/string_pointers_audit.tsv
done
```

All five addresses fall in `1000:3d11`..`1000:584c`. Confirmed a second way by
running the original: the two agility messages and both miss messages are all
printed during a scripted fight. Regenerate those captures with
`python3 tools/capture_combat_vectors.py`, which leaves every frame under
`build/combat_capture/<run>/screens.txt` (gitignored).

## The fighter record

Both fighters are plain records in the data segment (`DS` = `20ae`), laid out
identically, and the player's is **byte-for-byte the `.SAV` file**: memory at
`DS:369c` for 694 bytes equals the save file exactly. That was checked
directly against `SAVE_R2.SAV` by dumping guest memory (see "Capturing the
vectors" below) — `mem[0x369c:0x389c] == save[0x000:0x200]` and
`mem[0x38b0:0x3952] == save[0x214:0x2ba]`, both `True`.

This pins `docs/re/save-format.md`'s layout too, not just this document —
the eight `.SAV` stat words at `0x200`..`0x20f` are this same table's
`+0x00`..`+0x0e` rows. (Fix wave 1: that finding did not originally land in
`data/save_layout.json`/`docs/re/save-format.md`; it now does, under the
same names as this table.)

| record offset | `.SAV` offset | player | enemy | meaning |
|---|---|---|---|---|
| `+0x00` | `0x200` | `DS:389c` | `DS:3952` | rank/class name index — **not** the level |
| `+0x02` | `0x202` | `DS:389e` | `DS:3954` | strength (`Сила`) |
| `+0x04` | `0x204` | `DS:38a0` | `DS:3956` | agility (`Ловкость`) |
| `+0x06` | `0x206` | `DS:38a2` | `DS:3958` | vitality (`Живучесть`) |
| `+0x08` | `0x208` | `DS:38a4` | `DS:395a` | luck (`Удача`) |
| `+0x0a` | `0x20a` | `DS:38a6` | `DS:395c` | level (`понтовость`), 0..40 |
| `+0x0c` | `0x20c` | `DS:38a8` | `DS:395e` | `dmg_min` |
| `+0x0e` | `0x20e` | `DS:38aa` | `DS:3960` | `dmg_max` |
| `+0x10` | `0x210` | `DS:38ac` | `DS:3962` | `hp` |
| `+0x12` | `0x212` | `DS:38ae` | `DS:3964` | `hpmax` |
| `+0x14` | `0x214` | `DS:38b0` | `DS:3966` | `broken_jaw`, one byte |
| `+0x15` | `0x215` | `DS:38b1` | `DS:3967` | `broken_leg`, one byte |
| `+0x16` | `0x216` | `DS:38b2` | `DS:3968` | `armor` (`Броня`), one byte |
| `+0x32` | `0x232` | `DS:38ce` | — | experience (`качки опыта`) |
| `+0x34` | `0x234` | `DS:38d0` | — | experience needed for the next level |
| `+0x33`.. | `0x233`.. | `DS:38cf` | — | per-level record of which stats were gained, `string[2]` each |

### The word at `.SAV` offset `0x200` is the rank name, not the level

Task 9b's brief says "`SAVE_R0` is level 4, `SAVE_R2`–`R4` are level 6,
`SAVE_R5` is level 5 (word at save offset `0x200`, pending Task 9's
confirmation of that field's meaning)". **That is wrong, and this is the
confirmation.** The level is at `+0x0a`, and the five reference saves are
levels 15, 10, 20, 30, 40. Three independent checks:

* `1000:1404` pushes `[0x395c]` — the `+0x0a` word — as the number in
  `^2Это <name> # уровня` (the format string is loaded at `1000:13ef`), so it
  is what the game calls "уровня". Live: district 2 loads `SAVE_R2`, whose
  `+0x0a` is 10, and the game displays it as level 10.
* `1000:258a` increments `*(int *)0x38a6` — the `+0x0a` word — under the
  message `^1Понтовость увеличивается:`, and `1000:2580` caps it at `0x28`
  (40). So `понтовость` *is* the level.
* The XP threshold is `10 + 10 * level` (`1000:2550`, below). All five saves
  agree: `(level 15, 160)`, `(10, 110)`, `(20, 210)`, `(30, 310)`,
  `(40, 410)`.

The `+0x00` word instead indexes a 256-byte-stride string table at `DS:002e`
holding the rank names — `1000:13dc`..`1000:13e4`, `[0x3952] shl 8 + 0x2e`.
A second 256-byte-stride table at `DS:0b42` is indexed by the *level* for the
title (`1000:1363`..`1000:136b`), guarded by `cmp [0x395c],0x28 / jg` at
`1000:135c`, which falls through to `Не в этой жизни.` above level 40.

## The formulas

### Blow budget and blow count — `1000:3fa7`..`1000:408f`

Each fighter's round is bought with an agility budget. The enemy's is computed
at `1000:3fa7`..`1000:3fec` and the player's at `1000:404a`..`1000:408f`; the
two are the same instruction sequence with the record addresses swapped.

```
mine   := agility + 4                       ; 1000:3fa7 / 1000:404a
theirs := opponent.agility + 4              ; 1000:3fb1 / 1000:4054
if mine > 10 then                           ; 1000:3fbb  cmp mine,0x0a / jng
  while theirs > 18 do begin                ; 1000:3fc2  cmp theirs,0x12 / jng
    if mine < 28 then begin                 ; 1000:3fc9  cmp mine,0x1c / jl
      mine := 10;                           ; 1000:3fe2  mov mine,0x0a
      break
    end;
    mine   := mine - 18;                    ; 1000:3fd4
    theirs := theirs - 18                   ; 1000:3fdb
  end
```

The blow loop (`1000:445c`..`1000:4660`) is a do-while: swing, subtract 18
(`1000:4624`), keep going while what remains is positive (`1000:4652`). So

```
blows = ceil(budget / 18), minimum 1
```

Reported to the player by `^2Из-за твоей хорошей ловкости враг сможет пнуть
тебя раз # вместо #` (`1000:4013`) and its mirror (`1000:40b6`), which print
`(budget - 1) div 18 + 1` before and after the reduction (`1000:4018`).

Live check, district 5 (`SAVE_R5`, agility 120) against the boss (agility 50):
the game printed `раз 5 вместо 7` for the player and `раз 1 вместо 3` for the
enemy. The formula gives 7→5 and 3→1.

### Accuracy — `1000:446a`..`1000:4484`

```
roll := Random(100) + 1                     ; 1000:4460
if (budget_left * 5 < roll) or (roll > 90)  ; 1000:446a (shl/shl/add = *5)
  then miss                                 ; 1000:447f  cmp roll,0x5a
```

`budget_left` is the budget *at that blow*, so it drops by 18 per blow within
a round. Effective chance:

```
accuracy% = min(budget_left * 5, 90)
```

and since `budget = agility + 4`, the first blow's chance is
`agility * 5 + 20` capped at 90 — exactly what the status screen prints
(`1000:1574` tests `agility > 14`; `1000:157b` prints `Точность #%` with
`agility * 5 + 20`; `1000:15a4` prints the flat `Точность 90% `), and what
the in-game help text at `1000:613e` states in words:
`^0 Точность = (20+Ловкость*5)%`.

The enemy's copy of the same test is `1000:468d`..`1000:46a7`.

### Second blow and multi-blow display — `1000:15bd`..`1000:1611`

**Whose display: the ENEMY's.** This block is inside `FUN_1000_1348`, the
`sv` (size-up) sheet, and `1000:156d mov ax,[0x3956]` loads the ENEMY record's
agility word — see `docs/re/combat-dispatch.md`. The player's copy of the same
formula is a different block in a different function, `1000:21b0` onwards in
`FUN_1000_1a03` (`docs/re/character-sheet.md`), and that is the one the
`SAVE_R2` / `SAVE_R5` screens below exercised. The arithmetic is identical in
both, so nothing about `PER_BLOW = 18` or the `+4` changes; only the
attribution of each citation does.

```
if agility <= 14 then no second blow        ; 1000:1574  cmp agility,0x0e / jg
v := agility - 14;                          ; 1000:15c1
n := 1;
while v > 18 do begin v := v - 18; n := n + 1 end   ; 1000:15ce..1000:15de
if n = 1 then print '   Второй удар #%', v * 5      ; 1000:15e7
if n > 1 then print '- # ударов,  Точность # удара #%', n, n + 1, v * 5  ; 1000:1611
```

This is the same budget arithmetic seen from the display side and is what
pins `PER_BLOW = 18` and the `+4`: `agility - 14` is `(agility + 4) - 18`.
Live check: `SAVE_R2` (agility 15) printed `Точность 90%    Второй удар 5%`;
`SAVE_R5` (agility 120) printed `Точность 90% - 6 ударов,  Точность 7 удара
80%`, i.e. seven blows with the seventh at `(120 - 14 - 5*18) * 5 = 80`.

### Damage — `1000:448f`..`1000:4560` (player) / `1000:46b2`..`1000:4783` (enemy)

```
damage := dmg_min + Random(dmg_max - dmg_min) + 1     ; 1000:448f / 1000:46b2
crit   := Random(100) + 1 < attacker.luck * 3         ; 1000:44b8 / 1000:46db
if crit then begin
  damage := damage + attacker.dmg_max;                ; 1000:44d8 / 1000:46fb
  case Random(3) of                                   ; 1000:44e3 / 1000:4706
    0: '^2Точный удар!!!'   1: '^2Не хило приложил!!!'   2: '^2Двойной урон!!!'
  end
end;
damage := damage - defender.armor;                    ; 1000:4546 / 1000:4769
if damage < 0 then damage := 0;                       ; 1000:454f / 1000:4772
defender.hp := defender.hp - damage                   ; 1000:4560 / 1000:4783
```

Notes that matter for a faithful port:

* `dmg_max - dmg_min` is a 16-bit `sub` and the result is passed to
  `System.Random` as a `Word`; it wraps rather than clamping.
* The damage range is `dmg_min + 1 .. dmg_max`, not `dmg_min .. dmg_max`.
* `luck * 3` is computed with `shl`/`add` in 16 bits, then `cwd`
  sign-extends it, and the comparison against the roll is Borland's
  32-bit pair — high words signed (`jg`/`jl`), low words unsigned
  (`jna`/`ja`). The test is strict `>`.
* Armour is a *byte* in the record, zero-extended (`mov al,[…] / xor ah,ah`)
  before the subtraction, and the floor at zero is a **signed** test.
* The crit's `Random(3)` picks a taunt and nothing else — but it steps the
  seed, so a port that skips it desynchronises. (Confirmed by mutation: the
  vectors reject an implementation that omits it.)

Damage is printed by `^2Ты пнул врага на #з. У него осталось #`
(`1000:45ea`) as `hp_before - hp_after`, i.e. the post-armour figure.

`dmg_min`/`dmg_max` are stored, not derived, because equipment adds to them;
the base is `Сила div 2` .. `Сила` per the help text at `1000:6125`, and the
stat-loss path at `1000:499a` (the flee penalty, `^4Сила -1`) keeps them in
step: it also does `dmg_max - 1`, and `dmg_min - 1` when strength is odd.

### Jaw and leg breaks — `1000:4564`..`1000:45ea` (player) / `1000:4787`..`1000:4867` (enemy)

```
if Random(defender.luck * 3 + 200) + 1 < attacker.luck * 3 then   ; 1000:4571 / 1000:4794
  if Random(2) = 0                                                ; 1000:4595 / 1000:47be
    then defender.broken_jaw := true      ; message only if not already broken
    else defender.broken_leg := true
```

Compared exactly like the crit (16-bit product, `cwd`, signed 32-bit pair,
strict `>`). The `Random(2)` is drawn **even when that limb is already
broken** — only the message is suppressed (`1000:459e` / `1000:47c7`) — so
the draw count does not depend on the flags.

Neither flag feeds back into accuracy or damage. Grepping every use of
`0x38b0`/`0x38b1`/`0x3966`/`0x3967` in `FUN_1000_3d11` finds them only at the
set sites, at the flee guard (`1000:3d11`'s `run` handler,
`^4Ты не можешь убежать на сломаной ноге.`), at the pill guard
(`^4Ты не схавать колёса из-за сломаной челюсти.`), and where both are
cleared after the fight. They are status effects on *actions*, not modifiers
on the math.

**Player-only branch.** When the *player* is the defender and owns the
зубная защита (`DS:394a`), a jaw break draws one more `Random(4)`
(`1000:47fe`): a 0 breaks the jaw anyway
(`^4Враг сломал тебе челюсть, даже защита не помогла.`, file `0x4BB1`),
anything else prints `^2Защита спасла твои кривые клыки.` (file `0x4BE5`) and
leaves the jaw intact.

Three corrections to the first version of this passage, all from re-deriving
the block at `1000:47b3`..`1000:4867`:

* **The call is at `1000:47fe`, not `1000:47fa`.** `1000:47fa` is
  `b8 04 00` / `50` — `mov ax,4` / `push ax`, the argument idiom — and the
  `9a 4b 11 78 0f` is four bytes later. `python3 tools/re_query.py
  is-call-site 1000:47fa` says so, and this is the same near-miss shape
  `docs/re/METHODOLOGY.md` warns about with `1000:d83b`.
* **The draw is gated on the jaw not already being broken.** `1000:47c7`
  `cmp byte [0x38b0],0` / `jnz 0x4840` jumps past the whole block — the
  guard's `Random(4)` included — when the jaw is already broken. So it is not
  "one extra draw per jaw break"; it is one extra draw on the *first* jaw
  break of a guarded player. (The `Random(2)` at `1000:47be` is unaffected:
  it sits before that test, which is why it is drawn regardless.)
* **It is now modelled**, as `Game::tooth_guard` + `combat::Swing`, and it had
  to be: `SAVE_R3`, `SAVE_R4` and `SAVE_R5` all ship a 1 at `.SAV 0x2ae`, so a
  replay of a save-loaded fight without it desynchronises at the first player
  jaw break. Still **UNVERIFIED by observation** — see "What Task 13's capture
  did and did not reach".

### Draw order per blow

This is the part a differential test depends on. Player-swinging /
enemy-swinging addresses:

| # | draw | when | address |
|---|---|---|---|
| 1 | `Random(100)` | always | `1000:4460` / `1000:4683` |
| 2 | `Random(dmg_max - dmg_min)` | on a hit | `1000:4497` / `1000:46ba` |
| 3 | `Random(100)` | on a hit | `1000:44b8` / `1000:46db` |
| 4 | `Random(3)` | on a crit | `1000:44e3` / `1000:4706` |
| 5 | `Random(defender.luck*3 + 200)` | on a hit | `1000:4571` / `1000:4794` |
| 6 | `Random(2)` | on a break | `1000:4595` / `1000:47be` |
| 7 | `Random(4)` | jaw break, player defending, has the guard, jaw **not already broken** | `1000:47fe` |

A miss consumes exactly one draw and nothing else: `1000:445c` →
`1000:447a`/`1000:4486` → `1000:460b`, a straight jump to the miss message
with no intervening call. The vector capture relies on this (below).

### XP award and the level threshold

XP for a kill is the sum of the enemy's four stats — `1000:51b9`..`1000:51c8`:

```
award := enemy.strength + enemy.agility + enemy.vitality + enemy.luck
```

printed as `^6За отпин врага ты получаешь # качков опыта` (`1000:51b4`) and
added to `DS:38ce` at `1000:51e9`. It does not depend on the player's level.
Skipped entirely when the fight was `param_1 = 3` or `4` (the rector and the
endgame, `1000:51a6`).

Levelling is `FUN_1000_2526` (`1000:2526`):

```
while xp >= threshold do begin
  xp        := xp - threshold;      ; 1000:254d
  threshold := threshold + 10;      ; 1000:2550  add word [0x38d0],0x0a
  levels    := levels + 1
end
```

so `threshold(level) = 10 + 10 * level`, starting at 10 for a fresh level-0
character. Verified against every reference save: `(level 15, 160)`,
`(10, 110)`, `(20, 210)`, `(30, 310)`, `(40, 410)`, and against a fresh
character, which the game shows as `Сейчас у тебя 0 опыта, А для прокачки
надо 10`. Each level grants **two** stat increases (`1000:287d`, the inner
loop bound), drawn by `Random(sum of four class weights)` against a per-class
weight table of four bytes at `DS:(class * 4 + 2)` — `1000:25aa`..`1000:25b6`
reads `[[0x389c] * 4 + 5]` and the three siblings follow. Level is capped at
40 (`1000:2580`), unless the caller passes `param_1 <> 0`.

**Task 9b carries this through**: `docs/re/progression.md` has the full
level-up routine with its side effects, the weight table read out of the
image, character creation, and 30 kills captured from the original that
confirm both the award and the threshold arithmetic. `src/progress.rs` is the
port. It is recorded here as well because the award is computed inside the
combat function.

## The whole fight, not just the blow — Task 13

`data/combat_vectors.json` covers the blow arithmetic: 295 seed-pinned cases
from the original asserting per-blow `hit` and `damage`. What a per-blow vector
set structurally cannot cover is the fight as a **control flow** — which draws
a whole fight spends, in what order, and what happens after the last blow. The
five runs of `data/rng_trace.json` could not fill that in either: they decline
or flee every encounter, and between them contain **zero** `Random` sites
inside `[0x3d11, 0x584c)`.

`data/combat_trace.json` (`tools/rngtrace/fightrun.py`) is that oracle: four
live runs, **1900 draws, 15 fights**, replayed draw for draw by
`tests/combat_sequence.rs`. Method and guards: `docs/re/rng-trace.md`, "The
fight channel". Three blocks had to be recovered to make it replay.

### The crowd — `1000:40f2`..`1000:4168`, and it fires every prompt

**Established from flow.** Disassembled from `1000:40ed`, the
`c6 86 ed fe 00` (`mov byte [bp-0x113],0`) that zeroes the counter. That store
is **outside** the prompt loop: the loop's top is `1000:40f2` and its only
back edge in the whole function is `1000:583e` `jmp 0x40f2`, so the counter is
per **fight**.

```
40ed  c6 86 ed fe 00   mov byte [bp-0x113],0     ; once per fight
40f2  80 be ed fe 05   cmp byte [bp-0x113],5     ; loop top
40f7  73 24            jae 0x411d                ; already 5: skip the inc
40f9  fe 86 ed fe      inc byte [bp-0x113]
40fd  80 be ed fe 05   cmp byte [bp-0x113],5
4102  75 19            jne 0x411d
4104  bf 74 2e         mov di,0x2e74             ; file 0x4744
411d  80 3e 83 3c 00   cmp byte [0x3c83],0       ; the rector flag
4127  80 be ed fe 05   cmp byte [bp-0x113],5
4131  b8 0a 00 / 50    mov ax,10 / push ax
4135  9a 4b 11 78 0f   call Random               ; nonzero -> the prompt
4141  b8 12 00 / 50    mov ax,18 / push ax
4145  9a 4b 11 78 0f   call Random               ; picks one of 18 lines
```

The counter **stops** at 5 (`jae` skips the `inc`), so `== 5` stays true for
every later prompt. `Random(10)` at `1000:4135` therefore fires at every
`Битва\` prompt **from the fifth onward**, not once, and `Random(18)` at
`1000:4145` follows on a 0.

The reading that gets this wrong — "a one-off event on round 5" — is exactly
what a live capture settles. Run A's single 30-prompt fight shows **26** stops
at `1000:4135` (30 − 4), run B's six fights of 8/5/4/4/3/3 prompts show 5
(4+1+0+0+0+0), run C's three of 4/5/3 show 1, and run D's five one-prompt
fleeing fights show 0. Per-fight, not per-session, and per-prompt, not
per-fight.

`[0x3c83]` is the rector flag and nothing in this port sets it, so the
`1000:411d` gate is always open here. The eighteen lines are the crowd
heckling (`Зрители:^6Мочи его, мочи!` and so on, files `0x4762`..`0x4A1D`);
`r = 4` splices the **player's rank name** (`[0x389c] * 0x100 + 0x2e`, the
`DS:002e` table) and `r = 17` the player's own name (`DS:379c`, `.SAV 0x100`).

### The class-keyed opener — `1000:3d32`..`1000:3e8a` — costs no draw

**Established from flow.** It is a `cmp [0x3952],N` chain over the enemy class
that writes one or two intro lines per arm and nothing else. Scanning
`[0x3d11, 0x3f00)` for `9a 4b 11 78 0f` returns **zero** hits, so this block
cannot move the generator whatever it prints. Its text is still not extracted
— registered in `docs/re/gaps.md`.

### The victory block — `1000:5189`..`1000:57cc`

**Established from flow**, disassembled forward from `1000:5189`.
`docs/re/progression.md` already carried the shape of this block and
`data/xp.json`'s `post_kill_stat_events` the one-shot deltas; the addresses
were re-derived here and every `Random` site named carries the signature.

| address | what |
|---|---|
| `1000:51b9`..`1000:51e9` | `award := enemy.str+agi+vit+luck`, `[0x38ce] += award` (skipped for `param_1` 3 or 4) |
| `1000:51ed` | `xp >= threshold` -> `1000:523b call 0x2526`; otherwise files `0x528A` and `0x52C8` |
| `1000:523e`..`1000:5251` | the loot: `[0x38c3] += [0x396a]`, `[0x38c7] += [0x396c]`, `[0x38c9] += [0x396e]` |
| `1000:526c` | `hp += 5`, clamped to `hpmax` at `1000:5271` |
| `1000:5280` | `[0x38cb] += enemy.class + 1 + enemy.level div 3` |
| `1000:5295` | den flag when `level - (district-1)*10 >= 3` (`1000:52ae` `cmp ax,3` / `jl`) |
| `1000:52d5` | `Random(30)`; only `0` reaches the one-shot gift chain |
| `1000:5402` | `Random(district*25)`; `luck >= r` **and** enemy class 2 -> `1000:5427` `Random(3)` joints |
| `1000:5454` | `Random(district*40)`; `luck >= r` -> a class-keyed item, each arm with its own draw |

The `1000:5454` arms, and the draw each spends:

| enemy class | site | `n` | grants |
|---|---|---|---|
| 1 | `1000:5482` | 3 | крестик (`luck+2`, `[0x38bd]`), кольцо (`luck+1`, `[0x38be]`), or the mobile (`[0x38bb]`) |
| 3..6 | `1000:5530` | 2 | кастет (`[0x38ba]`, урон +2) or дубинка (`[0x394b]`, +4 or +2) |
| 7 | `1000:5617` | 2 | тёмные очки (`[0x38b3]`) or the mobile |
| 9 | `1000:5681` | 2 | ножик (`[0x38c2]`) or тесак (`[0x394c]`) |
| 0, 2, 8 | — | — | no draw at all |

Both luck comparisons are Borland's 32-bit pair with the roll **zero**-extended
(`xor dx,dx` at `1000:5407` / `1000:5459`) and luck **sign**-extended (`cwd` at
`1000:5410` / `1000:5462`): `luck >= roll`.

The weapon arms' damage bonuses are gated on which *other* weapons are already
owned, and the gate differs per arm — `1000:555f`/`1000:5566`/`1000:556d` for
the кастет, `1000:55c5`/`1000:55cc`/`1000:55d3` for the дубинка, and two
chains of independent `if`s for the ножик and тесак (`1000:56bc`..`1000:5709`
and `1000:5762`..`1000:57c9`, whose leading `mov al,1` / `or al,al` / `jz` is a
never-taken branch the compiler left in). All four flags are therefore carried
on `Game` even though nothing else reads them.

### Death, and the hospital — `1000:4f82`..`1000:5077`

The death test at `1000:4f82` (`cmp word [0x38ac],0` / `jle`) runs **before**
the victory test at `1000:507b`. **No arm of it draws**: there is no
`9a 4b 11 78 0f` anywhere in `1000:4f82`..`1000:5077`.

* `[0x3c83] == 1` (the rector) -> file `0x509C`, then `FUN_1000_074b`.
* `[0x3696]` set **and** `[0x38cb] >= 10` -> the rescue at `1000:4fce`: file
  `0x50DF`, `[0x38cb] -= 10`, a bill of `Round(hpmax / 5 * 3)` off `[0x38c7]`,
  `hp := hpmax`, and — if either limb is broken — 7 more roubles and both
  flags cleared. A negative purse is paid out of the street cred
  (`1000:5042`). The player lives and leaves the fight.
* otherwise `1000:5053` -> file `0x5127` (`^4Ты сдох.`) and `FUN_1000_074b`,
  which ends the process.

The two constants in the bill are six-byte Borland reals decoded from their
register loads, not guessed: `cx=0x83, si=0, di=0x2000` is
`1.25 * 2^(0x83-129) = 5.0` (the divisor, `0f78:1117`) and
`cx=0x82, si=0, di=0x4000` is `1.5 * 2^(0x82-129) = 3.0` (the multiplier,
`0f78:1111`), with `0f78:1131` rounding half away from zero.

Those two decimals assume a bias of 129, which `docs/re/rtl.md` records as
**not established**. The bill does not need it: the exponent bytes differ by
exactly one step, so the bias cancels out of `K2/K1` and the debit is
`Round(hpmax * 3 / 5)` under any bias — see `docs/re/combat-dispatch.md`,
"The bill does not need the exponent bias". `5.0` and `3.0` are one consistent
pair, not the only one.

### What Task 13's capture did and did not reach

Stated because a capture that reaches less than the recovery claims is exactly
the defect this project keeps paying for.

**Reached, and asserted by `tests/combat_sequence.rs`:** every site in the
blow loop on both sides; `1000:4135` and `1000:4145`; `1000:52d5`,
`1000:5402`, `1000:5427` and `1000:5454`; the loot award; a **player** jaw
break (run A, `20ae:38b0` from prompt 13 on) and an **enemy** jaw break (run B,
`20ae:3966` in two of its six fights); the death block at `1000:5053` (runs A
and C); and the whole flee path with zero draws (run D).

**Not reached by any captured run**, and so still UNVERIFIED by observation:

* **the зубная защита's `Random(4)`** at `1000:47fe`. Run C loads the one
  save that ships it (`.SAV 0x2ae` = 1) and spends six break rolls at
  `1000:4794`, but none of them passed the luck comparison, so the branch was
  never entered. It is modelled anyway, because a `SAVE_R3`/`R4`/`R5` replay
  desynchronises at the first player jaw break without it.
* **the hospital rescue** at `1000:4fce`. Both deaths captured are of
  characters without the den flag (run A is a fresh character; run C's
  `SAVE_R3` is level 20 in district 3, so `1000:52b3`'s
  `level - (district-1)*10 >= 3` never fires and the flag stays clear).
* **a leg break on either side.** All five limb picks captured
  (`1000:4595` three times, `1000:47be` twice) returned **0**, the jaw. The
  leg arms at `1000:45c5` and `1000:4842` are therefore still text-only
  predictions.
* **the four class-keyed item arms** `1000:5482` / `1000:5530` / `1000:5617` /
  `1000:5681`. The `1000:5454` gate was reached eight times and passed once —
  run B fight 6, roll 14 against luck 17 — and that fight's enemy was class 2,
  which has no arm. The other seven rolls (69, 69, 54, 24, 52 in run B, 101
  and 106 in run C) all beat the player's luck. So none of the four draws has
  ever been observed, and the weapon-gating arithmetic above rests on the
  disassembly alone.

**Reached after all, and worth naming because the first draft of this list got
it wrong:** the one-shot gift chain at `1000:52e1` **does** fire. Run B's
fight 5 rolled `1000:52d5` = 0, and `SAVE_R2` ships `[0x38bf]` and `[0x38c0]`
already set with `[0x38c1]` clear, so the chain reached its third arm and
granted the ring "Господи помилуй". The capture's `final_state.ring_38c1` is
`1` where the save's `.SAV 0x225` was `0`, and `tests/combat_sequence.rs`'s
`run_b_final_state_matches` asserts it. The joint arm is exercised too: run B
fight 2 passed `1000:5402` (roll 12 against luck 17) against a class-2 Нарк
and drew `1000:5427`.

## Seed pinning

`orig/g.exe` seeds itself from the DOS clock (`System.Randomize` at
`1f78:11e0`, file offset `0x12230`), so no fight reproduces — Task 8 proved
this empirically. Pinning replaces `Randomize`'s 13-byte body with a constant
store of the same length, so nothing in the image moves:

```
c7 06 7e 36 lo lo    mov word [0x367e], seed_lo   ; RandSeed.lo
c7 06 80 36 hi hi    mov word [0x3680], seed_hi   ; RandSeed.hi
cb                   retf
```

The addressing is DS-relative, exactly like the `mov [0x367e],cx` /
`mov [0x3680],dx` it replaces, so it stores through the same segment.

### Applying it

```bash
# via the harness (this is the intended route)
tools/oracle/run_oracle.sh '\n1\n\n\n\n\n\n\n0\n\ns\n' out/ --seed 0 --expect-frames 15

# or from Python
python3 -c "
import sys; sys.path.insert(0, 'tools/oracle'); import capture
capture.pin_seed(pathlib.Path('scratch/g.exe'), 0)"
```

`capture.pin_seed` refuses to patch anything that does not have
`Randomize`'s exact body at `0x12230`, so a wrong file or a shifted offset is
an error rather than silent corruption.

### Removing it

**Nothing to undo.** The pin is applied only to the scratch copy that
`capture._prepare` rebuilds from `orig/` at the start of every run, and it is
applied only when `--seed`/`seed=` is passed:

* Run without `--seed` and the game seeds itself from the clock, exactly as
  it ships. That is the default, and it is what a task needing real-game
  behaviour should use.
* Delete the run's `work/` directory (or just let the next run overwrite it)
  and no patched binary exists anywhere.
* `orig/g.exe` is never opened for writing. Its MD5 is still
  `10eb0af07a2d2f5e9da790df7058891c`; `tools/verify_corpus.py` checks that.
* No pinned binary is committed. `build/` is gitignored.

The Rust port is unaffected: `src/rng.rs` has no `Randomize` equivalent and
`Rng::new` has always taken the seed explicitly, so a fixed seed is only ever
something a test or a CLI flag chooses.

Task 12 can reuse this as-is for its differential harness.

## Capturing the vectors

`data/combat_vectors.json` is generated by
`tools/capture_combat_vectors.py` (`python3 tools/capture_combat_vectors.py`).
**Nothing in that path touches the Rust crate**: it does not build, link,
import or shell out to `gopnik`, and it never reads `src/combat.rs`. The
vectors would be identical if `src/combat.rs` did not exist.

Where each number comes from:

* **Blows** — the 80x25 text screen, via the Task 3 oracle. One frame per
  blocking key read; a round's output is the scroll delta between the frame
  that answered the Enter of a `k` command and the next frame. `Ты пнул врага
  на #з` gives a hit and its damage, `Ты промазал` a miss.
* **Fighter stats** — the guest's own data segment. `scrhook.com` was
  extended for this task to append, at every key read, the interrupted
  program's `DS` plus 2048 bytes from `DS:3600` to `STATE.BIN`. Both fighter
  records live in that window. The window is validated before use: the
  `.SAV` banner `^4Gopnik: ^7version 1.02 june,sept 2003` must be at
  `DS:369c`, which is what proves `DS` really is the game's DGROUP at
  `INT 16h` time rather than something the harness assumed.
* **The seed** — `RandSeed` at `DS:367e` in the same window. Taken at the
  Enter that completes a `k` command. Between that key read and the round's
  first `Random(100)` the game only finishes `ReadLn` (`1000:4431`) and
  compares the string against `"k"` (`1000:4440`); neither draws.

**The pin works, and here is the evidence.** Two complete 18-run captures,
executed separately, produced a byte-identical `data/combat_vectors.json`
(md5 `b6c6129bc7be4fdc9e752b0b52e7f407`). Task 8 showed that unpinned runs of
a script that reaches RNG-dependent output diverge from each other — three
walking runs gave three different screens — so this is not the trivially
deterministic intro path.

Two cross-checks run during capture and abort it on failure:

1. The damage the game *printed* must equal the HP the enemy actually lost,
   read from the other channel (guest memory), for every round where the
   enemy survived and did not swing back.
2. The frame count of every run is pinned with `--expect-frames`, so a
   truncated capture fails instead of quietly yielding fewer cases.

### Enemy-as-attacker cases

The seed is only directly observable at a command prompt, which is the start
of the player's half of a round. To reach the other direction, the tool uses
the one case where the player's half provably consumes exactly one draw: a
round whose player half is a **single miss** takes the straight line
`1000:445c` → `1000:447a` → `1000:460b`, one `Random(100)` and no other call.
The enemy's first `Random(100)` therefore steps `lcg_step(seed)`, computed
from the recurrence in `docs/re/rng.md` — arithmetic on a captured seed, not
combat logic. Rounds where the player owns the зубная защита are skipped,
because an enemy jaw break there would draw the unmodelled `Random(4)`.

That derivation is a one-instruction-path claim, but it is not taken on
trust: 95 enemy-attacker cases were produced this way and all 95 pass. If the
draw count or the mirrored formula were wrong, every one of them would fail.

### What was captured

18 runs across all six starting districts (a fresh character and the five
reference saves), on 15 distinct pinned seeds. **295 cases, 352 blows** — 200 with the
player attacking, 95 with the enemy attacking. (Fix wave 1 regenerated this
file after removing the truncation described below; the case count is
unchanged, the blow count grew from 314 because every blow of every round is
now recorded, not just the leading run at the opening accuracy.)

| dimension | values reached |
|---|---|
| attacker agility | 1, 2, 3, 4, 13, 15, 19, 20, 25, 45, 120, 121 |
| defender agility | 1, 2, 3, 4, 6, 7, 9, 10, 11, 13, 15, 19, 20, 27, 31, 50, 60 |
| attacker armour | 0, 1, 2, 3, 4, 10, 26 |
| defender armour | 0, 1, 2, 3, 4, 8, 12, 13, 31, 33, 60, 80 |
| attacker luck | 1, 2, 3, 4, 6, 7, 8, 10, 17, 31, 49, 50, 52 (5 never occurs) |
| defender luck | 1..8, 10, 11, 13, 14, 16, 17, 19, 20, 32, 34, 36 |
| attacker level | 0, 2, 10, 12, 15, 18, 20, 30, 40, 41 |
| defender level | 0, 2, 6, 10, 11, 12, 13, 15, 16, 18, 19, 20, 22, 25, 46, 62, 125, 160 |
| opening accuracy | 25, 30, 35, 40, 50, 55, 85, 90 % |
| blows in a round | 1, 2, 3, 4, 5 |
| broken jaw / leg | all four combinations, on both attacker and defender |
| outcomes | 161 hits, 191 misses |
| blow index within a round | 0, 1, 2, 3, 4 (every index now has ground truth — see below) |

The brief's target list is covered as follows: zero armour ✔, high armour ✔
(up to 80), broken jaw ✔, broken leg ✔, large agility gap in both directions
✔ (120 vs 50 and 50 vs 120, both budget-collapse and budget-decrement paths),
level 1 vs level 6 ✔ and far beyond (level 0 against level 160) — though see
the next section: level does not enter the combat math at all.

### `expected_blows` covers the whole round (fix wave 1)

`resolve_blow` answers for one blow at the round's *opening* accuracy; later
blows in the same round are drawn at a budget 18 lower, via
`resolve_blow_nth(rng, attacker, defender, blow_index)`. The original capture
truncated `expected_blows` to the leading run whose effective accuracy
matched the opening one — exactly what repeated `resolve_blow` calls model —
and left `resolve_blow_nth` at `blow_index > 0` with zero ground truth, since
every case that survived truncation happened to sit at the capped 90%
accuracy where later-blow budget differences are invisible. This was a fifth
UNVERIFIED gap that the "Not verified" list below never recorded — until
fix wave 1, `resolve_blow_nth`/`budget_at` at `blow_index > 0` had zero
ground-truth coverage despite being the API Task 11 needs.

**Closed by regenerating the vectors.** `tools/capture_combat_vectors.py` no
longer truncates: `expected_blows` is every blow of the round, in the order
the screen printed it, and `tests/combat_vectors.rs` asserts entry `i`
against `resolve_blow_nth(rng, attacker, defender, i)` — the real per-blow
index, not always 0 — drawing from one shared `Rng` across the whole round.
That is ground truth for `resolve_blow_nth` at every index a capture reached
(0 through 4), not just the opening one. Confirmed by mutation: perturbing
`budget_at`'s index term (`PER_BLOW.wrapping_mul(blow_index as i16)` →
`(PER_BLOW - 1).wrapping_mul(blow_index as i16)`) now fails
`combat_matches_original` at case 273, blow 1 — a mutant the truncated data
could never have caught. Not because those cases were absent: the old,
truncated `data/combat_vectors.json` did contain 9 cases with 2–5 blows per
round, i.e. `blow_index > 0` did occur in the captured data. What was
missing was any *assertion* at `blow_index > 0` — the old
`tests/combat_vectors.rs` called `resolve_blow` (always index 0, ignoring
the blow's real position in the loop) for every blow of every case — and
those retained multi-blow cases all sat at the capped 90% opening accuracy,
where `budget_at`'s per-index term makes no observable difference anyway.

`blows_in_round` is kept as a separate field precisely equal to
`expected_blows.len()`; it exists because it is read off the screen
independently of `resolve_blow_nth`, and `tests/combat_vectors.rs` also
checks it against `blows_per_round(attacker, defender)` — see the next
section.

### `blows_in_round` is now asserted against `blows_per_round` (fix wave 1)

`blows_in_round` was captured from the very first version of this tool but
was never deserialised or checked by `tests/combat_vectors.rs` — a free,
independent check of the blow-count formula went unused. It now is.
`blows_per_round(attacker, defender)` (the formula, `ceil(budget / 18)`) is
never less than the screen-counted `blows_in_round`: across all 295 cases,
they are equal in 287 and `blows_per_round` is strictly greater in the other
8, every one of which is a round the defender did not survive — the printed
damage of the recorded blows already sums to at least the defender's
starting HP, i.e. the loop at `1000:4629`/`1000:48c6` broke out early on a
kill rather than swinging the full budgeted count. `tests/combat_vectors.rs`
asserts `blows_per_round(a, d) >= case.blows_in_round` unconditionally, and
`==` specifically when the recorded damage total is less than the
defender's starting HP (i.e. the round could not have ended in a kill).

`opening_accuracy_pct` in `data/combat_vectors.json` is **not** asserted, and
must not be: it is computed in Python from the captured agility values using
the same formula `src/combat.rs` implements, so checking it against the Rust
port would just be checking the formula against itself. It is retained as a
reading aid only, and the payload's `opening_accuracy_pct_is` note says so.

## Not verified — UNVERIFIED, not reachable by scripted play

These were established by reading the disassembly and are **not** covered by
any captured vector. Each was checked by mutating `src/combat.rs` and
confirming the vector suite still passes, i.e. the data genuinely does not
constrain it.

1. **Armour fully absorbing a hit.** `1000:454f`/`1000:4772` floor the
   damage at 0, so `damage = max(0, rolled - armor)`. No captured blow hits
   the floor. Reaching it needs a weak attacker against an armoured
   defender, and the game's progression does not pair them: the armour-0 and
   armour-2/3 enemies are the ones a low-damage character meets, while the
   armour-60/80 enemies only appear opposite a character whose luck (49–52)
   makes `luck * 3 > 100` — a guaranteed crit, which adds `dmg_max` and puts
   the result far above the floor. Predicted behaviour: a hit whose rolled
   damage is at or below the defender's armour prints
   `Ты пнул врага на 0з` and takes no HP.
2. **The crit and break comparisons at exact equality.** Both are strict
   `>` (`cmp ax,cx / jna` for the crit at `1000:44d4`, `cmp ax,cx / ja` for
   the break at `1000:47b3`). The crit boundary happens to be exercised by
   the data — a `>=` mutant fails — but the break boundary is not: no
   captured blow has `Random(luck*3+200) + 1` landing exactly on
   `attacker.luck * 3`.
3. **The зубная защита `Random(4)` branch** (`1000:47fe` — the call; the
   `mov ax,4` / `push ax` four bytes earlier at `1000:47fa` is the argument
   idiom, see "Player-only branch" above). **Modelled** since Task 13, as
   `Game::tooth_guard` carried into `combat::Swing::enemy`, but still not
   *observed*: no draw at `1000:47fe` appears in `data/combat_trace.json`.
   Run C loads `SAVE_R3.SAV`, which ships the guard, and fights three
   fights — but no jaw break landed on the player in any of them, and
   `tools/capture_combat_vectors.py` skips the enemy-side rounds that could
   have exercised it. What would settle it: a capture in which a guarded
   player's jaw is broken.
   `docs/re/gaps.md` records the same thing under "Opened and closed by
   Task 13".
4. **The collapse constants in isolation.** The budget-collapse path is
   exercised (31 cases sit at the collapsed 50% accuracy), and the data pins
   the collapsed budget to 10 or 11 — a captured roll of 50 hits and a roll
   of 56 misses. That it is exactly 10, and that the threshold is exactly 28,
   comes from the literals `mov word [bp-0x10e],0x0a` (`1000:3fe2`) and
   `cmp word [bp-0x10e],0x1c` (`1000:3fc9`), not from a vector.

## Boundaries that cannot be observed — Task 15

`cargo mutants -f src/combat.rs -f src/rng.rs` reported eight mutations of
`src/` that no test noticed. Seven were comparison and arithmetic boundaries in
the blow logic. Three of those are **equivalent mutants** — the mutated program
computes the same answer for every input, so no test can kill one and a test
that appeared to would be measuring something else. They are skipped in
`.cargo/mutants.toml`, each with the address it rests on; the arguments are
here.

All three are **established from flow**: each rests on the literals in the
instructions cited, resolved with `python3 tools/re_query.py resolve`.

### `mine > 10` and `mine >= 10` are the same program (`1000:3fbb`)

```
1000:3fbb  83 be f2 fe 0a   cmp word [bp-0x10e],0xa      ; the guard bound
1000:3fc0  7e 2a            jle 0x3fec                   ; <= 10 leaves
...
1000:3fe2  c7 86 f2 fe 0a 00  mov word [bp-0x10e],0xa    ; the collapse value
```

The guard's bound and the collapse's value are the **same 10**. At `mine == 10`
the two senses agree: `jle` skips the loop and returns 10; letting it in either
falls straight out (`theirs <= 18`) or reaches `1000:3fe2` and is set to 10.

### `mine < 28` and `mine <= 28` are the same program (`1000:3fc9`)

```
1000:3fc9  83 be f2 fe 1c   cmp word [bp-0x10e],0x1c     ; the collapse bound
1000:3fce  7c 12            jl 0x3fe2                    ; below 28, collapse
1000:3fd4  2d 12 00         sub ax,0x12                  ; otherwise step by 18
```

`0x1c - 0x12 == 0x0a`: one step from the bound lands exactly on the collapse
value. At `mine == 28` the unmutated path subtracts 18, reaches 10, and on the
next turn round the loop either exits with 10 or collapses to 10. The mutated
path collapses to 10 at once. Same answer, always.

Corroborated as well as argued, one tier down: a transcription of the loop into
Python was run over the only input classes on which the two programs execute
different instructions (`mine == 10` and `mine == 28`), for all 65536 values of
`theirs`, and the mutated and unmutated forms agreed everywhere. That is a
check on a transcription, not on `blow_budget` itself — the argument above is
what the skip rests on.
`the_blow_budget_boundaries_are_unobservable` in `src/combat.rs` is the
regression test. It does not kill the two mutants (nothing can); it goes red if
the identity `0x0a + 0x12 == 0x1c` is ever broken, which is what would make the
skips wrong.

This is the **opposite** of the asymmetry Task 13 found in the two blow loops,
`1000:4629` `cmp word [0x3962],0x0` / `jnle` against `1000:48cd`
`cmp word [0x38ac],0x0` / `jl`, where the two senses genuinely differ and a
player at exactly 0 gets swung at again. A boundary is not automatically a
finding; whether it is one depends on the constants around it.

### `damage < 0` and `damage <= 0` are the same program (`1000:454f`)

```
1000:454b  29 86 f4 fe      sub [bp-0x10c],ax            ; less the armour byte
1000:454f  83 be f4 fe 00   cmp word [bp-0x10c],0x0      ; the bound
1000:4554  7d 06            jnl 0x455c                   ; >= 0 keeps it
1000:4556  31 c0            xor ax,ax                    ; the floor
```

The value stored by the floor **is** the bound tested, so widening the test to
`<= 0` writes a 0 over a 0.

`damage == 0` at the same site is NOT equivalent, and it was a genuine gap: it
lets a blow lighter than the armour stay negative, which `damage as u16` turns
into 65482 and which `1000:4560` `sub [0x3962],ax` would apply as **healing**.
Armour 60 is `Ректор НГУ`'s, so this is reachable play, not a contrived input.
Killed by `armour_heavier_than_the_blow_floors_the_damage_at_zero`.

### The break test is strict — `1000:4576` and `1000:458f`

Three of the eight mutants sat on one comparison: `+ 1` → `- 1`, `+ 1` → `* 1`,
and `>` → `>=`.

```
1000:4571  9a 4b 11 78 0f   call 0f78:114b               ; Random(luck*3 + 200)
1000:4576  40               inc ax                       ; the + 1
1000:4577  31 d2            xor dx,dx
...
1000:4587  3b d3            cmp dx,bx
1000:4589  7f 06            jnle 0x4591                  ; high word, signed
1000:458b  7c 5d            jl 0x45ea
1000:458d  3b c1            cmp ax,cx
1000:458f  76 59            jbe 0x45ea                   ; low word, unsigned
```

`jbe` at `1000:458f` takes the branch AWAY from the break when the two are
EQUAL, so the test is strictly greater. The enemy's copy says the same with the
sense flipped: `1000:47b5` `ja 0x47ba` reaches the break only from strictly
above. All three mutations move the comparison off that exact point, so one
case at the point kills all three — `the_break_test_is_strict_at_1000_458f`,
which puts `luck 20 * 3 = 60` against a `1000:4571` draw of 59 (`+ 1` = 60) out
of `data/rng_vectors.json`'s seed-0 chain, and asserts both that nothing breaks
and that the `1000:4595` limb draw is never spent. That oracle read is
registered in `tools/mutations.json` as `rng-vectors-break-boundary`.

### The eighth: `Rng::set_state`

Not a boundary. `src/rng.rs`'s `set_state` had **no caller anywhere in the
crate**, so replacing its body with `()` changed nothing and all 164 tests
passed. Nothing in the port rewinds a generator — every one is started from a
seed with `Rng::new` and then only stepped — and the recovered save layout
(`docs/re/save-format.md`) carries no seed field, so nothing has a reason to.
It was deleted rather than tested: a test written to exercise it would have
been the only caller.

## Open questions

* **Level does not enter combat at all.** Nothing in `1000:445c`..`1000:4867`
  reads `+0x0a` from either record. Levels differ only through the stats they
  bought. `Fighter.level` is carried for display and XP, not for the math.
  This is a finding, not an omission — but it means the brief's "level 1 vs a
  level 6 fighter" coverage row cannot distinguish a right implementation
  from a wrong one.
* **`+0x00`, the rank/class index — closed by Task 9b.** It selects the
  displayed name (`DS:002e` table) and the per-class stat-growth weights at
  `DS:(v*4+2)`. The stored value is the class prompt's answer plus 3
  (`1000:712a`, `1000:71b8`), so a fresh `0-Пацан` gets `3` and `3-Вор` gets
  `6`, matching `SAVE_R2`. All eleven rank names and weight rows are read out
  of the image into `data/xp.json`; see `docs/re/progression.md`.
* ~~**The `string[2]` array at `+0x33`.**~~ **Closed by Task 17 for the flee
  half.** It is `growth_log`, `array[1..40] of string[2]` at `.SAV 0x236`
  (`docs/re/save-format.md`), indexed by level, holding the codes `'1'`..`'4'`
  for which stats each level granted, written by `FUN_1000_2526` next to each
  `^1Сила +1`-style message (the first at `1000:2621`). The flee arm copies
  `growth_log[level]` at `1000:4954`..`1000:496e`, **clears the source entry**
  at `1000:497d`, and then walks the copy **forward**, `i := 1` then `2`
  (`1000:4982` / `1000:4989` / `1000:4a6f`), applying the inverse of each code.
  "In reverse" meant *inverted*, not *backwards*, and `1000:499a` is the
  `^4Сила -1 ` literal push, not the replay.
  `docs/re/combat-dispatch.md` has the per-code table. The rest of the
  162-byte tail is still opaque.
* ~~**`DS:3c83`** ... not confirmed.~~ **Confirmed by Task 17: it is the
  rector-showdown flag.** Six references image-wide, two writes and four
  reads, and nothing ever clears it. `1000:7364` arms it after
  `^1Пора наконец отомстить ректору...` and `1000:ae13` after
  `^1А вот и он...`, immediately before the two fights at `1000:ae2d`
  (opponent kind 3) and `1000:ae39` (kind 4, the rector). Set, it suppresses
  the spectator taunts (`1000:411d`, which requires it CLEAR), refuses the
  flee (`1000:48eb`) and selects the rector death message (`1000:4f8c`).
  `docs/re/combat-dispatch.md` has the reference table.
* **The spectator taunt block draws RNG.** From the fifth iteration of the
  battle command loop onward, `1000:4131` draws `Random(10)` and, on a 0,
  `Random(0x12)` (`1000:4141`) picks one of 18 lines to print — *before* the command is read. Task
  11 must reproduce this to keep a whole battle in sync; it is outside a
  single blow and so outside `src/combat.rs`.
* **Twelve more `Random` call sites inside `FUN_1000_3d11` are recorded here
  but not mapped.** (fix wave 1; **eight of the twelve — the whole post-kill
  group — were mapped by Task 9b, see `docs/re/progression.md`, "The post-kill
  stat-gain block". The four flee/other-command sites are still unmapped.**)
  Of the function's 27 `call 1f78:114b`
  (`System.Random`) sites, the blow loops and the spectator taunt block above
  account for 15; these 12 do not, and a whole-battle differential replay
  (Task 12) will desynchronise on every one of them unless they are
  reproduced. Confirmed as real `call word 0xf78:0x114b` instructions by
  disassembling each address (`python3 tools/re_query.py is-call-site
  1000:XXXX`, which reports identity and alignment separately), not
  assumed from the address list alone:
  * ~~Flee/other command handlers~~ — **all four mapped by Task 17**, and
    none of them is in the flee arm, which draws nothing at all:
    `1000:4db7` is `Random(district * 4)`, the argument built from
    `20ae:3692` at `1000:4dad`..`1000:4db4`, and it rolls the backup's damage;
    `1000:4e16` (`Random(2)`) is the backup's attrition tick;
    `1000:4ef5` (`Random(0x32)`) is the pistol's hit test and
    `1000:4f18` (`Random(0xa)`) its damage. See
    `docs/re/combat-dispatch.md`.
  * Post-kill loot/stat-gain, after the XP award: `1000:52d5`
    (`Random(0x1e)`), `1000:5402` (argument is `ax` after `mul dx` with
    `dx = 0x19`, not a literal), `1000:5427` (`Random(3)`), `1000:5454`
    (argument is `ax` after `mul dx` with `dx = 0x28`, not a literal),
    `1000:5482` (`Random(3)`), `1000:5530` (`Random(2)`), `1000:5617`
    (`Random(2)`), `1000:5681` (`Random(2)`).
  * Arguments are given where the instructions immediately before the call
    make the pushed value unambiguous (a literal `mov ax,N` or a `mul` by a
    literal); three (`4db7`, `5402`, `5454`) push a value computed from a
    variable this reading did not trace back further, and are noted as such
    rather than guessed. Mapping what any of these twelve actually do is out
    of scope here — this is only the inventory Task 12 needs to know what it
    has to reproduce.

## Harness changes made for this task

* `tools/oracle/scrhook.asm` / `scrhook.com` — added the `STATE.BIN` dump
  described above (`STATE_BASE`/`STATE_SIZE`). The screen capture is
  unchanged.
* `tools/oracle/capture.py` — added `--seed`/`seed=` (the pin),
  `decode_states`, and `pin_seed`/`pin_seed_patch`.
* `tools/oracle/dosbox-oracle.conf` — added `quit warning=false`. With
  `SDL_VIDEODRIVER=dummy` a window should never appear, but if one ever does,
  closing it while the game is running pops a modal confirmation that would
  block an unattended run forever.
* `tools/oracle/capture.py` teardown is now `SIGKILL` rather than `SIGTERM`.
  Every frame is committed to disk as it is written (`INT 21h/AH=68h`), so
  there is nothing to flush; a `SIGTERM`'d dosbox-x runs its own shutdown
  path, which is what can raise that modal.
