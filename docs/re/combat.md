# Combat (Task 9)

The combat routine is **`FUN_1000_3d11`, Ghidra address `1000:3d11`**, in
memory block `CODE_0`. It spans `1000:3d11`..`1000:584c` (6971 bytes) and is
called exactly once, from `entry`. It is not only the blow loop: it is the
whole battle sub-loop, including the `Битва\` prompt, the command dispatch,
fleeing, and the post-kill XP award.

Ported to `src/combat.rs` and `src/model.rs`; vectors in
`data/combat_vectors.json`, captured by `tools/capture_combat_vectors.py`.

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
(`1000:47fa`): a 0 breaks the jaw anyway
(`^4Враг сломал тебе челюсть, даже защита не помогла.`), anything else prints
`^2Защита спасла твои кривые клыки.` and leaves the jaw intact. `Fighter` has
no field for the item, so `src/combat.rs` does not model this — see Open
questions.

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
| 7 | `Random(4)` | jaw break, player defending, has the guard | `1000:47fa` |

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
надо 10`. Each level grants **two** stat increases (`1000:2567`, the
loop bound), drawn by `Random(sum of four class weights)` against a per-class
weight table of four bytes at `DS:(class * 4 + 2)` — `1000:25aa`..`1000:25b6`
reads `[[0x389c] * 4 + 5]` and the three siblings follow. Level is capped at
40 (`1000:2580`). Full treatment belongs to Task 9b; recorded here because
the award is computed inside the combat function.

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
reference saves), on 15 distinct pinned seeds. **295 cases, 314 blows** — 200 with the
player attacking, 95 with the enemy attacking.

| dimension | values reached |
|---|---|
| attacker agility | 1, 2, 3, 4, 13, 15, 19, 20, 25, 45, 120, 121 |
| defender agility | 1..15, 19, 20, 27, 31, 50, 60 |
| attacker armour | 0, 1, 2, 3, 4, 10, 26 |
| defender armour | 0, 1, 2, 3, 4, 8, 12, 13, 31, 33, 60, 80 |
| attacker luck | 1..8, 10, 17, 31, 49, 50, 52 |
| defender luck | 1..8, 10, 11, 13, 14, 16, 17, 19, 20, 32, 34, 36 |
| attacker level | 0, 2, 10, 12, 15, 18, 20, 30, 40, 41 |
| defender level | 0..25, 46, 62, 125, 160 |
| opening accuracy | 25, 30, 35, 40, 50, 55, 85, 90 % |
| blows in a round | 1, 2, 3, 4, 5 |
| broken jaw / leg | all four combinations, on both attacker and defender |
| outcomes | 147 hits, 167 misses |

The brief's target list is covered as follows: zero armour ✔, high armour ✔
(up to 80), broken jaw ✔, broken leg ✔, large agility gap in both directions
✔ (120 vs 50 and 50 vs 120, both budget-collapse and budget-decrement paths),
level 1 vs level 6 ✔ and far beyond (level 0 against level 160) — though see
the next section: level does not enter the combat math at all.

### `expected_blows` is truncated deliberately

`resolve_blow` answers for one blow at the round's *opening* accuracy. Later
blows in the same round are drawn at a budget 18 lower, so a case records
only the leading run of blows whose effective accuracy is unchanged; the full
count is kept in `blows_in_round` and every truncation is listed in
`capture_notes`. Use `resolve_blow_nth` for later blows.

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
3. **The зубная защита `Random(4)` branch** (`1000:47fa`). Not modelled at
   all, because `Fighter` as the brief specifies it has no field for the
   item. Any replay where the player is the *defender* and owns it will
   desynchronise after the first jaw break.
4. **The collapse constants in isolation.** The budget-collapse path is
   exercised (31 cases sit at the collapsed 50% accuracy), and the data pins
   the collapsed budget to 10 or 11 — a captured roll of 50 hits and a roll
   of 56 misses. That it is exactly 10, and that the threshold is exactly 28,
   comes from the literals `mov word [bp-0x10e],0x0a` (`1000:3fe2`) and
   `cmp word [bp-0x10e],0x1c` (`1000:3fc9`), not from a vector.

## Open questions

* **Level does not enter combat at all.** Nothing in `1000:445c`..`1000:4867`
  reads `+0x0a` from either record. Levels differ only through the stats they
  bought. `Fighter.level` is carried for display and XP, not for the math.
  This is a finding, not an omission — but it means the brief's "level 1 vs a
  level 6 fighter" coverage row cannot distinguish a right implementation
  from a wrong one.
* **`+0x00`, the rank/class index.** It selects the displayed name
  (`DS:002e` table) and the per-class stat-growth weights at `DS:(v*4+2)`.
  A fresh `0-Пацан` character gets `3`; `SAVE_R2` has `6`. The mapping from
  the class the player picks to this value is not established here, and the
  `DS:0002` weight table has not been read out. Task 9b's territory.
* **The `string[2]` array at `+0x33`.** Indexed by level, holding the codes
  `'1'`..`'4'` for which stats each level granted, written by
  `FUN_1000_2526` next to each `^1Сила +1`-style message (the first at
  `1000:2621`) and replayed in reverse when the player flees (`1000:499a`). Segmented enough
  to explain the flee penalty; the rest of the 162-byte tail is still opaque.
* **`DS:3c83`** gates the spectator taunts and the flee refusal
  (`^4Ректор: Кудa? Стоять!`). Read as an arena/boss flag; not confirmed.
* **The spectator taunt block draws RNG.** From the fifth iteration of the
  battle command loop onward, `1000:4131` draws `Random(10)` and, on a 0,
  `Random(0x12)` (`1000:4141`) picks one of 18 lines to print — *before* the command is read. Task
  11 must reproduce this to keep a whole battle in sync; it is outside a
  single blow and so outside `src/combat.rs`.

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
