# The in-combat command dispatcher — `1000:4400`..`1000:5080`

`FUN_1000_3d11` is not only the blow loop. Between the `^0Битва\` prompt and
the death test it runs a **command dispatcher**: nine string compares against
one buffer, plus one subroutine that holds eight more. This is what each verb
does, which of them draw RNG, and how the fight ends by flee, by rector death,
and by hospital rescue.

Machine-readable twin: **`data/combat_dispatch.json`**. Most of what follows is
also a record there, and `python3 tools/test_combat_dispatch.py` re-derives
**both halves** from `orig/g.exe`:

- **the artifact** — every record's address checked for alignment (it is a
  boundary an aligned walk from `FUN_1000_3d11`'s entry reaches) *and* identity
  (the instruction there says what the record says), plus every literal, the
  closure of the buffer's reference set, the arm each handler's first
  instruction belongs to, and the cited/uncited branch split;
- **this file** — every `1000:xxxx` in the prose resolved to an instruction
  boundary; every inline-code span that pairs an address with an instruction
  compared against `tools/dis16.py`; and every Russian run inside inline code
  traced back to a literal at an address the prose or the artifact names.

Addresses are Ghidra form A; `tools/addr.py` is the executable authority.
`CS <hex>` offsets are image offsets inside the game's code segment, `20ae:`
offsets are DGROUP. Russian is verbatim, typos included.

---

## The shape: nine independent `if`s, not an `if`/`else` chain

**Established from flow.** One `Битва\` prompt runs the whole chain top to
bottom. No arm returns to the top of the loop — `1000:583e jmp 0x40f2` is the
function's only back edge — so every arm rejoins the straight line and a
verb's handler is followed by every compare after it, with the buffer never
reset in between.

That is why there are **two** `k` compares. `1000:4440` matches `k` and jumps
into the blow loop at `1000:444a`; the blow loop's three exits
(`1000:467c`, `1000:48cb`, `1000:48d2`) all land on `1000:48d7`, which is the
`run` compare's setup — with the buffer still holding `k`. The second compare
at `1000:4c75` is how the attack verb gets a second effect after the blows are
exchanged. Both were already in `docs/re/gaps.md`'s table; what was not written
down is *why* two exist.

The prompt itself:

| at | what |
|---|---|
| `1000:43f6` | `mov di,0x3179` — the prompt string `^0Битва\` (CS `0x3179`) |
| `1000:440f` | `mov di,0x3ecc` — the `Text` file `ReadLn` reads from |
| `1000:4414` | `mov di,0x3a72` — the destination, the command buffer `20ae:3a72` |
| `1000:441d` | `call 0xf78:0x6c6` — `rtl_text_read_string` (`docs/re/rtl.md`) |
| `1000:4422` / `1000:4427` | `call 0xf78:0x59d` / `call 0xf78:0x291` — the `ReadLn` line-skip and the `{$I+}` check |
| `1000:4431` | `call 0xeed:0x216` — the **case fold**, so every verb below is case-insensitive |

## The verb table

**Established from flow.** Each row's token is read out of the
`mov di,<token>` that precedes the compare, never from proximity.

| compare | branch | token | CS | arm | what it does |
|---|---|---|---|---|---|
| `1000:4440` | `1000:4445 jz 0x444a` | `k` | `0x3182` | `[1000:444a, 1000:48d7)` | the blow exchange — `docs/re/combat.md` |
| `1000:48e1` | `1000:48e6 jz 0x48eb` | `run` | `0x33bb` | `[1000:48eb, 1000:4afb)` | flee, below |
| `1000:4b0d` | `1000:4b12 jz 0x4b17` | `kos` | `0x34b1` | `[1000:4b17, 1000:4c24)` | smoke a joint — `docs/re/gaps.md` traced it in full |
| `1000:4c2e` | `1000:4c33 jnz 0x4c38` | `s` | `0x359f` | `1000:4c35` | `call 0x1a03` — the player's sheet, `docs/re/character-sheet.md` |
| `1000:4c42` | `1000:4c47 jnz 0x4c4c` | `sv` | `0x35a1` | `1000:4c49` | `call 0x1348` — the **enemy's** sheet, below |
| `1000:4c56` | `1000:4c5b jnz 0x4c64` | `e` | `0x35a4` | `[1000:4c5d, 1000:4c64)` | `xor ax,ax` then `call 0xf78:0x116` — `rtl_halt`, i.e. `Halt(0)` |
| `1000:4c75` | `1000:4c7a jnz 0x4ca0` | `k` (2nd) | `0x3182` | `[1000:4c7c, 1000:4ca0)` | the backup countdown, below |
| `1000:4caa` | `1000:4caf jz 0x4cb4` | `v` | `0x35c6` | `[1000:4cb4, 1000:4d93)` | call for backup, below |
| `1000:4ea8` | `1000:4ead jz 0x4eb2` | `f` | `0x3714` | `[1000:4eb2, 1000:4f82)` | fire the pistol, below |

`docs/re/gaps.md`'s in-combat table was checked against the binary rather than
imported, and it is right in every row. Two things it did not say: the `k`
row's `jz` target is the blow loop (so the second `k` compare is reachable),
and the second `k` compare's own guard is `1000:4c64 cmp word [0x3c80],0x1` /
`1000:4c69 jl 0x4ca0` — the backup counter must already be at least 1, i.e.
the backup must have been called.

### `h` and `mh` are dispatched too, from a subroutine

**Established from flow.** `1000:4afb` pushes the buffer and `1000:4b00`
`call 0x29c4` hands it to `FUN_1000_29c4`, which holds eight more
`rtl_str_compare` sites: `h` (CS `0x28c7`) at `1000:29f5`, `1000:2a6f`,
`1000:2aa5`, `1000:2af7`, `1000:2b45`, `1000:2b8e`, and `mh` (CS `0x28c9`) at
`1000:2a07` and `1000:2bb5`. So the in-combat verb set is **ten**, not nine.

### The chain is closed, and what that rules out

**Established from flow.** `python3 tools/re_query.py xrefs-to 20ae:3a72`
accepts 102 references image-wide and exactly **twelve** of them are inside
`FUN_1000_3d11`. All twelve are accounted for:

| role | sites |
|---|---|
| `ReadLn` destination | `1000:4414` |
| case fold | `1000:442c` |
| the nine compares' setups | `1000:4436`, `1000:48d7`, `1000:4b03`, `1000:4c24`, `1000:4c38`, `1000:4c4c`, `1000:4c6b`, `1000:4ca0`, `1000:4e9e` |
| the `FUN_1000_29c4` call | `1000:4afb` |

Nothing else in the fight loop reads the buffer — no byte compare, no second
subroutine, no computed dispatch. `FUN_1000_29c4` is the **only** near call in
`FUN_1000_3d11` that receives it; the other ten near calls
(`1000:4c35`, `1000:4c49`, `1000:4fb4`, `1000:5074`, `1000:5097`, `1000:512b`,
`1000:5133`, `1000:5148`, `1000:523b`, `1000:5835`) do not.

So the dispatcher accepts exactly `k`, `run`, `kos`, `s`, `sv`, `e`, `v`, `f`,
`h`, `mh`, and **any other line falls through the whole chain and prints
nothing at all**. In particular the fight prompt does **not** accept `x`
(CS `0x96ce`) or `wes` (CS `0x970a`) — those two are compared at `1000:ce80`
and `1000:ced8`, both in `entry`, and `docs/re/gaps.md`'s in-combat table never
claimed them; nor `i`, `w`, `mar`, `kl`, `trn` or `help`. A verb typed at the
fight prompt that produces no output has not necessarily been dispatched —
here it provably has not.

## `sv` calls the ENEMY's sheet — `FUN_1000_1348`

**Established from flow.** Task 16 established that `1000:4c49` calls
`FUN_1000_1348` and not the character sheet, and left *what it is* open.
It is the enemy's sheet, 791 bytes, `1000:1348`..`1000:165e`, ending in a bare
`ret` at `1000:165e` — no parameters, like the player's sheet. Every global it
reads is in the enemy record at `20ae:3952`, none in the player's:

| at | reads | line |
|---|---|---|
| `1000:135c` | `20ae:395c`, enemy level | `cmp word [0x395c],0x28` — above 40 the rank ladder has no entry and `Не в этой жизни.` (CS `0x1274`) is used instead |
| `1000:1363`..`1000:136b` | `20ae:395c` | `krutizna[level]`, base `20ae:0b42`, stride 256 — the same table the player's sheet indexes at `1000:1a53` |
| `1000:13c0` | `20ae:3952`, enemy class | `cmp word [0x3952],0x8` / `1000:13c5 jl 0x13cc` — at class 8 and up `1000:13c7` zeroes the local's length byte, dropping the ladder-name suffix |
| `1000:13dc`..`1000:13e4` | `20ae:3952` | `ranks[class]`, base `20ae:002e`, stride 256 |
| `1000:141e`..`1000:142a` | `20ae:3954`, `20ae:3956`, `20ae:3958`, `20ae:395a` | `Сл:# Лв:# Жв:# Уд:#` (CS `0x129a`) |
| `1000:143b` / `1000:143f` | `20ae:395e`, `20ae:3960` | `Урон #-#` (CS `0x12ae`) |
| `1000:1456` / `1000:1487` | `20ae:3966`, `20ae:3967` | the broken jaw / leg lines |
| `1000:1557` / `1000:155b` | `20ae:3962`, `20ae:3964` | hp and hpmax |
| `1000:156d` | `20ae:3956`, enemy agility | the accuracy block, below |
| `1000:1638` | `20ae:3968`, enemy armour | `^2Броня #    ` (CS `0x133a`) |

### This corrects `combat.md`'s "Second blow and multi-blow display"

`docs/re/combat.md` mapped `1000:15bd`..`1000:1611` as the display side of the
blow budget. The block is exactly where that section puts it and the arithmetic
it reads off is right — but it is the **enemy's** accuracy, inside the `sv`
sheet: `1000:156d mov ax,[0x3956]` is the enemy record's agility word, and
`1000:1574 cmp word [bp-0x204],0xe` is the test on it.

The player's copy of the same formula is a different block in a different
function — the block at `1000:21b0` in `FUN_1000_1a03`, mapped in Task 16 —
and that is the one a `SAVE_R2`/`SAVE_R5` screen exercises. The formula is
identical in both, so `PER_BLOW = 18` and the `+4` are unaffected; what changes
is which fighter each citation is about. Both citations now say so.

### The live probe, including its negatives

**Established from flow.** `tools/rngtrace/verbprobe.py`, re-pointed with
`--target 1000:1348` (Task 17 made the target a parameter; the READY line
prints the breakpoint actually installed, so the report cannot name a function
the probe did not break on):

```
python3 tools/rngtrace/verbprobe.py --boot-img build/rngtrace/boot.img \
    --target 1000:1348 --street-plan s,w,w \
    --combat-plan sv,s,sv,kos,k,run --want-combat-prompts 6 \
    --out build/rngtrace/verbprobe-1348.json
```

Seed `0x12345678`, a fresh Пацан (class 3, read from the guest's `DS:389c`),
district 1. Marker stream, identical across two runs: `PPPCTCCTCCCP`.

| prompt | line typed | prompts | entries at `1000:1348` | |
|---|---|---:|---:|---|
| combat | `sv` | 2 | **2** | reaches |
| combat | `s` | 1 | 0 | does **not** reach |
| combat | `kos` | 1 | 0 | does **not** reach |
| combat | `k` | 1 | 0 | does **not** reach |
| combat | `run` | 1 | 0 | does **not** reach |
| street | `s` | 1 | 0 | does **not** reach |
| street | `w` | 2 | 0 | does **not** reach |

Six negatives against one positive. The load-bearing one is combat `s`: it is a
verb that *does* call a function from this same chain — `1000:4c35` calls
`1000:1a03` — and still never enters `1000:1348`, so the probe is separating
two callees at the same prompt rather than separating "typed something" from
"typed nothing". A breakpoint that did **not** fire is flow-tier evidence for a
negative; a screen never is (`docs/re/METHODOLOGY.md`).

`tools/test_combat_dispatch.py` asserts the two lanes agree: for each probed
`(prompt, verb)` pair the observed answer must equal the answer the compare
table predicts.

## The flee arm — `[1000:48eb, 1000:4afb)`

**Established from flow.** Two refusals, a free exit at level 0, then the
penalty. No arm of it draws a random number: there is no `9a 4b 11 78 0f`
anywhere in `[1000:48eb, 1000:4afb)`.

1. **`1000:48eb cmp byte [0x3c83],0x1` / `1000:48f0 jnz 0x490e`** — the rector
   showdown. Prints `^4Ректор: Кудa? Стоять! Бейся до конца трусливый урод!`
   (CS `0x33bf`) and `1000:490b` jumps to `1000:4afb`, the next step in the
   chain. The fight continues.
2. **`1000:490e cmp byte [0x38b1],0x1` / `1000:4913 jnz 0x4931`** — a broken
   leg. `^4Ты не можешь убежать на сломаной ноге.` (CS `0x33f6`), then
   `1000:492e` jumps to `1000:4afb`. The fight continues.
3. **`1000:4931 cmp word [0x38a6],0x0` / `1000:4936 jnle 0x493b`** — at level 0
   there is nothing to take away: `1000:4938` jumps to `1000:4ade`, which
   prints `^4Враг: Засранец!` (CS `0x349f`) and falls into `1000:4af7`.

`1000:4af7 mov byte [bp-0x1],0x1` is the flag that ends the fight.
`1000:3d20` clears it once per call, `1000:5838 cmp byte [bp-0x1],0x0` at the
bottom of the loop tests it, and `1000:583e jmp 0x40f2` is the back edge. The
other two writers are `1000:5077` (death or hospital) and `1000:51a2`
(victory).

### The penalty: `growth_log[level]`, applied inverted

**Established from flow.** `docs/re/combat.md` recorded this as "replayed in
reverse when the player flees (`1000:499a`)". The address is the `^4Сила -1 `
literal push, not the replay, and "in reverse" means the codes are **inverted**
— each one decrements the stat it once granted — not that the array is walked
backwards. The loop runs forward.

`docs/re/gaps.md` already had the shape of this arm — the growth-log read, the
undo, the clear, the den flag, the level cost. What follows is the per-code
table it does not carry, plus two corrections.

- `1000:493b` prints `^4Враг: Трусливый засранец! ` (CS `0x341f`) with no
  newline.
- `1000:4954`..`1000:496e` copies `growth_log[level]` — `20ae:38cf + level*3`,
  Borland's biased base for `array[1..40] of string[2]` at `.SAV 0x236`
  (`docs/re/save-format.md`, `docs/re/progression.md`) — into the local at
  `[bp-0x10a]`, via `rtl_str_assign_max`.
- `1000:497d mov byte [di+0x38cf],0x0` then **clears the source entry**: the
  length byte goes to zero, so a level's growth log is spent the first time it
  is undone. The copy exists precisely so the clear can happen first.
- `1000:4982 mov word [bp-0x6],0x1` starts the loop; `1000:4989 inc [bp-0x6]`
  is the step and `1000:4a6f cmp word [bp-0x6],0x2` / `1000:4a73 jz 0x4a78`
  the exit, so `i` runs `1` then `2` — the two code characters, forward. The
  loop does **not** consult the copied string's length byte at `[bp-0x10a]`;
  what those two positions hold when the entry is already spent is not
  established, because the local is written only at `1000:496e`.

| code | at | effect |
|---|---|---|
| `'1'` | `1000:498f` | `dec [0x389e]` (Сила); `^4Сила -1 ` (CS `0x343c`); `1000:49b3 dec [0x38aa]` (dmg max); if `[0x389e]` is odd (`1000:49b7`..`1000:49c4`) also `dec [0x38a8]` (dmg min); `1000:49ca dec [0x38ae]` (hp max), then hp clamped to it |
| `'2'` | `1000:49e3` | `dec [0x38a0]` (Ловкость); `^4Ловкость -1 ` (CS `0x3447`) |
| `'3'` | `1000:4a0c` | `dec [0x38a2]` (Живучесть); `^4Живучесть -1 ` (CS `0x3456`); `1000:4a30 sub word [0x38ae],0x5`, then hp clamped |
| `'4'` | `1000:4a49` | `dec [0x38a4]` (Удача); `^4Удача -1 ` (CS `0x3466`) |

Anything else falls through to `1000:4a6f` and costs nothing.

### After the loop

- `1000:4a78`..`1000:4a82` — a blank line on the `Text` at `20ae:3fcc`.
- `1000:4a87 cmp word [0x389c],0x5` / `1000:4a8c jz 0x4ac3` — class 5 skips
  the den block entirely.
- `1000:4a8e`..`1000:4aa3` computes `level - (district-1)*10` from
  `20ae:38a6` and `20ae:3692` and tests it against 3 with
  `1000:4aa0 cmp ax,0x3` / `1000:4aa3 jnz 0x4ac3` — **equality**, where the
  post-kill twin at `1000:52ae` uses `jl` on the same expression.
- On equality, `1000:4aa5 mov byte [0x3696],0x1` and
  `^4Такого конявого непустят в местный притон!` (CS `0x3472`).

  **That store is backwards, and it is the original's, not a decode error** —
  a point `docs/re/gaps.md` already carries ("`1000:4aa5` sets the Den flag
  while printing a refusal"), where it is left unverified whether a clear was
  intended. Two mechanical facts this adds, neither of which settles intent:
  `20ae:3696` is a boolean. `python3 tools/re_query.py xrefs-to 20ae:3696`
  accepts 18 references: every immediate store is `0` (`1000:6d6e`,
  `1000:abc9`) or `1` (`1000:52b3`, `1000:73c3`, `1000:ae1f`, and this one),
  and the two references that are neither (`1000:6cc1`, `1000:76ca`) pass its
  address to `rtl_file_read` / `rtl_file_write` for the flags file, so they
  can only round-trip a value some store already wrote. `1000:d80c cmp byte
  [0x3696],0x1` is the gate that lets the player into the den. So the arm that
  announces the player is now too shabby for the den **grants** den access
  instead of revoking it. Recorded as an original-behaviour finding.
- `1000:4ac3 dec [0x38a6]` — the level itself.
  `1000:4ac7 sub word [0x38d0],0xa` lowers the next-level threshold by 10, and
  `1000:4acc`..`1000:4ad9` clamps xp to `threshold - 1` if it now exceeds it.
- `1000:4adc` jumps to `1000:4af7`, the fight-exit flag.

## `v`, `k`, and the backup counter `20ae:3c80`

**Established from flow.** `20ae:3c80` has exactly 17 references image-wide
(`python3 tools/re_query.py xrefs-to 20ae:3c80`) and **every one is inside
`FUN_1000_3d11`**, so it is combat's own variable even though it lives in
DGROUP. `1000:5841`/`1000:5843` zero it as the function returns, so it does not
survive a fight.

### `v` — calling the local gopota

The arm at `1000:4cb4`:

- `1000:4cb4 cmp byte [0x3696],0x1` / `1000:4cb9 jnz 0x4d03` — the den flag.
- `1000:4cbb`..`1000:4ccc` — `district*10 + 10` against `20ae:38cb`
  (понтовость / street cred): `1000:4cc8 cmp ax,[0x38cb]` /
  `1000:4ccc jnle 0x4d03`, so the call needs `cred >= district*10 + 10`.
- `1000:4cce cmp word [0x3c80],0x0` / `1000:4cd3 jnz 0x4cdb` — first call only:
  `1000:4cd5 mov word [0x3c80],0x1`.
- `1000:4cdb cmp byte [0x38bb],0x1` / `1000:4ce0 jnz 0x4d01` — **the mobile
  phone short-circuits the wait**: `1000:4ce2 mov word [0x3c80],0x3` and
  `^2Подошли пацаны - Ща начнется!.` (CS `0x35c8`).
- Refusals: `1000:4d03 cmp byte [0x3696],0x0` / `1000:4d08 jz 0x4d25` splits
  them — cred too low gives `^4Ни кто не хочет за тебя впрягаться.`
  (CS `0x35e9`) at `1000:4d0a`, no den flag gives
  `^6Сначала надо скорешиться с местной гопотой.` (CS `0x360f`) at
  `1000:4d25`.
- Status, `[1000:4d3e, 1000:4d93)`: nothing at `[0x3c80] <= 0`;
  `^6Тебе надо продержатся до подхода братвы # пинка.` (CS `0x363d`) with
  `1000:4d51 mov ax,0x3` / `1000:4d54 sub ax,[0x3c80]` below 3; and
  `^2Они уже здесь.` (CS `0x3670`) at 3 or more, suppressed at exactly 3 when
  the phone already printed its line (`1000:4d6c` / `1000:4d73`).

### `k` (2nd) — the countdown ticks on your own blows

`1000:4c7c inc [0x3c80]`, then `1000:4c80 cmp word [0x3c80],0x3` /
`1000:4c85 jnz 0x4ca0` prints `^2Подошли пацаны - Ща начнется!` (CS `0x35a6`,
the copy **without** the trailing dot) on the transition to 3. So the backup
arrives on the third attack after the call — which is exactly what the
`# пинка` line counts down.

### The backup fights: `[1000:4d93, 1000:4e9e)`

Entered every prompt, not only on `v`:
`1000:4d93 cmp word [0x3962],0x0` / `1000:4d98 jnle 0x4d9d` (the enemy must be
alive) and `1000:4d9d cmp word [0x3c80],0x3` / `1000:4da2 jnl 0x4da7`.

**`1000:4db7` — `Random(district * 4)`.** The argument the earlier inventory
could not trace back: `1000:4dad mov al,[0x3692]` is the district byte,
`1000:4db2 shl ax,1` and `1000:4db4 shl ax,1` make it `*4`, and `1000:4db6
push ax` is the argument. The damage is then

```
dmg := district*3 + Random(district*4)      ; 1000:4dbe..1000:4dc9
dmg := dmg - enemy.armour div 3             ; 1000:4dcf..1000:4dda, [0x3968]
if dmg < 0 then dmg := 0                    ; 1000:4dde / 1000:4de5
enemy.hp := enemy.hp - dmg                  ; 1000:4def
```

and `^2Врага отпинали на #з. У него осталось #` (CS `0x3681`) prints the
difference and the remainder.

**`1000:4e16` — `Random(2)`, the attrition coin.** `1000:4e12 mov ax,0x2` is
the argument; on `0` (`1000:4e1b or ax,ax` / `1000:4e1d jnz 0x4e43`)
`1000:4e1f inc [0x3c80]`. At `1000:4e43 cmp word [0x3c80],0x7` the counter
resets — `1000:4e4a xor ax,ax` / `1000:4e4c mov [0x3c80],ax` — and
`^2Твою подмогу отпинали.` (CS `0x36bd`) prints. Four increments take the
counter from its arrival value of 3 to 7.

Then unconditionally: `1000:4e68`..`1000:4e75` subtracts `district*5` from
`20ae:38cb`, and if the cred goes non-positive
(`1000:4e79` / `1000:4e7e`) the backup leaves the same way —
`1000:4e80 xor ax,ax` / `1000:4e82 mov [0x3c80],ax` — with
`^4Подмоге надоело столько парится из-за мало понтового мудака`
(CS `0x36d6`).

### `^2Подошли пацаны.` at `1000:4e2a` is dead code

**Established from flow.** The block is entered only when `[0x3c80] >= 3`
(`1000:4d9d` / `1000:4da2`), `1000:4e1f` raises it to 4 or more, and
`1000:4e23 cmp word [0x3c80],0x3` / `1000:4e28 jnz 0x4e43` can then never be
equal. A scan of every branch target in `FUN_1000_3d11` finds **no** jump into
`[1000:4e12, 1000:4e43)`, so fall-through from `1000:4e0d` is the only way in
and there is no second entry that could satisfy the test. The literal at
CS `0x36ab` is therefore in the image and never printed.

A second consequence of there being **two** increment sites, and this one is
reachable: `1000:4c7c` can raise the counter from 6 to 7 on a `k`, and
`1000:4e1f` can then raise it to 8 in the same prompt, while `1000:4e43` tests
for **exactly** 7. Above 7 the "backup beaten" reset is unreachable and only
the cred exhaustion at `1000:4e79` can end the backup.

## The pistol — `f`, `[1000:4eb2, 1000:4f82)`

**Established from flow.**

- `1000:4eb2 cmp byte [0x394d],0x0` / `1000:4eb7 jnz 0x4ebc` — **no pistol,
  no message**: `1000:4eb9` jumps straight to the death test. An accepted verb
  that prints nothing at all, which is `docs/re/METHODOLOGY.md`'s "absence of
  visible response is not absence of dispatch" with the dispatch now read off
  the compare rather than inferred.
- `1000:4ebc cmp byte [0x3693],0x0` / `1000:4ec1 jnz 0x4ee6` and
  `1000:4ec3 cmp byte [0x394e],0x0` / `1000:4ec8 jnz 0x4ee6` — shooting needs
  either `20ae:3693` set **or** the silencer `20ae:394e`; otherwise
  `^6Тельзя тут стрелять! Менты накроют!` (CS `0x3716`).

  **`1000:4ebc` is a third reader of `20ae:3693`, and `docs/re/gaps.md` says
  there are two.** Its entry reads "the readers are `1000:0d86` and
  `1000:0e54`, both inside `FUN_1000_0d14`" — a completeness claim that
  stopped the next search, which is the failure `docs/re/METHODOLOGY.md`
  names by that phrase and which that same entry was written to correct.
  `python3 tools/re_query.py xrefs-to 20ae:3693` accepts seven references:
  the toggle's read/write pair at `1000:b3c4`/`1000:b3ce`, two more reads in
  `entry` at `1000:b3d1` and `1000:b45b`, the two in `FUN_1000_0d14`, and this
  one. What the flag *means* is still not established; `docs/re/gaps.md` has
  it as a wander toggle flipped in bucket 1. The entry there is corrected.
- `1000:4ee6 cmp word [0x394f],0x0` / `1000:4eeb jle 0x4f69` — out of
  cartridges gives `^6Чё за батва? Блин патроны кончились!` (CS `0x37a5`).
- `1000:4eed dec [0x394f]` spends one.
- **`1000:4ef5` — `Random(0x32)`** (`1000:4ef1 mov ax,0x32`). The hit test is
  Borland's 32-bit pair, roll zero-extended (`1000:4efa xor dx,dx`) and agility
  sign-extended (`1000:4f03 cwd`): `1000:4f04 cmp dx,bx` / `1000:4f06 jnle
  0x4f0e` / `1000:4f08 jl 0x4f4e` / `1000:4f0a cmp ax,cx` / `1000:4f0c jbe
  0x4f4e`. **Hit iff `[0x38a0] > Random(50)`** — agility alone, no luck, no
  accuracy formula. A miss prints `^2Это был хреновый выстрел.` (CS `0x3789`).
- **`1000:4f18` — `Random(0xa)`** (`1000:4f14 mov ax,0xa`), and
  `1000:4f1d add ax,0x14`: damage is `20..29`, subtracted from the enemy's hp
  at `1000:4f28` with **no armour term**, unlike every other damage site in the
  function. `^2Ты выстрелил и ранил врага на #з. У него осталось #з., осталось патронов #` (CS `0x373c`).

## `20ae:3c83` — confirmed: it is the rector-showdown flag

`docs/re/combat.md` read it as an arena/boss flag and said so was *not*
confirmed. **Confirmed, and the boss is the rector.** Established from flow:
`python3 tools/re_query.py xrefs-to 20ae:3c83` accepts exactly six references
image-wide, two writes and four reads, and **nothing ever clears it**.

| at | in | what |
|---|---|---|
| `1000:7364` | `FUN_1000_6a0d` | `mov byte [0x3c83],0x1`, immediately after `^1Пора наконец отомстить ректору...` (CS `0x6925`) in the `al == 5` arm at `1000:7347` |
| `1000:ae13` | `entry` | `mov byte [0x3c83],0x1`, immediately after `^1А вот и он...` (CS `0x847e`) |
| `1000:ae18` | `entry` | `cmp byte [0x3c83],0x1` — a test of the value stored eight bytes earlier, so `1000:ae1d jnz 0xae3c` is never taken and the two fights it guards are unconditional: `1000:ae2d call 0x3d11` with opponent kind 3 and `1000:ae39 call 0x3d11` with kind 4 |
| `1000:411d` | `FUN_1000_3d11` | `cmp byte [0x3c83],0x0` — the spectator taunts run only while it is **clear** |
| `1000:48eb` | `FUN_1000_3d11` | the flee refusal above |
| `1000:4f8c` | `FUN_1000_3d11` | the rector death message below |

Kind 4 is the rector: it is what `1000:5085 cmp byte [bp+0x4],0x4` selects for
the victory ending Task 16 mapped, and `1000:ae39` is the site that pushes it.
So the flag is armed exactly when the showdown scene begins, and its three
effects are: no crowd, no fleeing, and a death message that names the killer.

## Death, and the hospital — `[1000:4f82, 1000:507b)`

`docs/re/combat.md` already had this block; it was re-derived rather than
trusted, and it holds. `1000:4f82 cmp word [0x38ac],0x0` /
`1000:4f87 jle 0x4f8c` runs **before** the victory test at `1000:507b`, and no
arm draws: there is no `9a 4b 11 78 0f` anywhere in `[1000:4f82, 1000:507b)`.

- **Rector.** `1000:4f8c cmp byte [0x3c83],0x1` / `1000:4f91 jnz 0x4fba` →
  `^4Ты сдох. Ректор тебя замочил. Ты так и не доказал свою крутизну.`
  (CS `0x37cc`), `ReadKey`, then `1000:4fb4 call 0x74b` with `al = 0`.
- **Hospital.** `1000:4fba cmp byte [0x3696],0x1` / `1000:4fbf jz 0x4fc4` and
  `1000:4fc4 cmp word [0x38cb],0xa` / `1000:4fc9 jnl 0x4fce` — the den flag
  **and** at least 10 street cred. Then
  `^1Тебе повезло знакомые пацаны отвезли тебя в больницу а то бы ты сдох.`
  (CS `0x380f`), `1000:4fe7 sub word [0x38cb],0xa`, the bill, `hp := hpmax`
  (`1000:5018` / `1000:501b`), and — if either limb is broken
  (`1000:501e` / `1000:5025`) — 7 more roubles and both flags cleared
  (`1000:502c`, `1000:5031`, `1000:5036`).
- **Otherwise** `1000:5053`: `^4Ты сдох.` (CS `0x3857`), `ReadKey`,
  `1000:5074 call 0x74b`.
- Both survivors' paths converge on `1000:5077 mov byte [bp-0x1],0x1`.

### The bill does not need the exponent bias

`docs/re/rtl.md` records that reading the two 6-byte real literals as decimals
"needs the 6-byte real layout confirmed against a known value and is **not
established**"; `docs/re/combat.md` nevertheless prints them as `5.0` and
`3.0`. Both can be right at once, because **the bill only depends on their
ratio, and the bias cancels out of it.**

The divisor is materialised at `1000:4ff5 mov cx,0x83` with `si = 0` and
`di = 0x2000` and consumed by `1000:4ffd call 0xf78:0x1117`; the multiplier at
`1000:5002 mov cx,0x82` with `si = 0` and `di = 0x4000`, consumed by
`1000:500a call 0xf78:0x1111`. `cl` is the exponent byte
(`docs/re/rtl.md`, from `0f78:1117`'s zero-divisor test), so the exponents
differ by exactly one step whatever the bias is, contributing a factor of
`1/2`. Under the mantissa layout the same two documents already use — sign in
the mantissa's top bit, leading `1` implicit — the significands are `1.25` and
`1.5`. So

```
K2/K1 = (1.5 / 1.25) * 2^-1 = 0.6      for every possible bias
```

and the bill is `Round(hpmax * 3 / 5)`. Rounding is unambiguous: `3*h/5` is
never exactly a half-integer, since `6h = 10k + 5` has no solution.
`5.0` and `3.0` are one consistent pair, the one that takes the bias to be
129; the **ratio** needs no such choice. What is still unestablished is the
bias itself, and with it the decimal value of either constant alone.

### A negative purse is settled out of street cred, and then zeroed

**Established from flow**, and re-derived because the first draft of this
section got it wrong. `1000:503b cmp word [0x38c7],0x0` /
`1000:5040 jnl 0x5051`, then

```
1000:5042  a1 cb 38     mov ax,[0x38cb]      ; street cred
1000:5045  03 06 c7 38  add ax,[0x38c7]      ; plus the (negative) purse
1000:5049  a3 cb 38     mov [0x38cb],ax      ; cred := cred + purse
1000:504c  31 c0        xor ax,ax
1000:504e  a3 c7 38     mov [0x38c7],ax      ; purse := 0
```

`docs/re/combat.md`'s one-line description — "a negative purse is paid out of
the street cred (`1000:5042`)" — is right, and this is the whole block behind
it. The `xor ax,ax` at `1000:504c` is two bytes and easy to drop from a
listing; dropping it turns `1000:504e` into a second copy of the cred and
invents a windfall that is not there. `tools/test_combat_dispatch.py` asserts
there is **exactly one** instruction between the two stores and that it is the
`xor`, so the reading cannot regress to the wrong one.

## The four `Random` sites, closed

**Established from flow**, and each confirmed as a real
`call word 0xf78:0x114b` by disassembling it
(`python3 tools/re_query.py is-call-site 1000:XXXX`, `identity.match: True`
on all four), not from an address list. A scan of `[0x4900, 0x5080)` for
`9a 4b 11 78 0f` returns exactly these four and nothing else.

| site | `n` | what it controls |
|---|---|---|
| `1000:4db7` | `district * 4`, from `20ae:3692` via `1000:4db2`/`1000:4db4` | the backup's damage roll, `district*3 + r` |
| `1000:4e16` | `2` | the backup's attrition tick — `0` advances `20ae:3c80` |
| `1000:4ef5` | `0x32` | the pistol's hit test, `agility > r` |
| `1000:4f18` | `0xa` | the pistol's damage, `r + 0x14` |

`docs/re/combat.md`'s "Open questions" listed `1000:4db7` as pushing "a shifted
variable ... not a literal" with the variable untraced. It is the district
byte. All four are now mapped, so the four flee/other-command sites drop off
that list.

## The branch skeleton

`data/combat_dispatch.json`'s `branch_partition` holds the split for
`[1000:4400, 1000:5080)` — **56 of 100 cited** by the artifact's own rule,
recomputed by the test from `data/branches.json` and from the artifact's own
citations, with the partition node itself excluded from that scan. The
project-wide `docs/re/*.md` metric counts the prose instead, and reads
`72 / 100` over the same range because this file cites addresses the artifact
does not carry (and vice versa). Read "cited" as "the address is on the map", never as "the branch is
understood" — the same caveat `docs/re/character-sheet.md` carries.

What is left uncited in the range is formatting: the five-`push ax` argument
padding has no branches, so every remaining one is inside the `kos` arm
(`docs/re/gaps.md` traced that arm byte for byte against its top-level twin,
and this document does not re-cite its internals) or inside the blow loop
(`docs/re/combat.md`).

## What this changes elsewhere

- `docs/re/combat.md`, "Open questions": the four unmapped `Random` sites, the
  `DS:3c83` reading, and the `1000:499a` flee-penalty entry are struck there
  and answered here.
- `docs/re/combat.md`, "Second blow and multi-blow display": retitled, because
  the block is the enemy's accuracy inside the `sv` sheet.
- `docs/re/combat.md`, "Death, and the hospital": the bill's constants and the
  purse copy are corrected there.
- `docs/re/gaps.md`'s "`sv`/`v`/`x`/`wes` dispatcher sites" entry: `sv`, `v`
  and the other six in-combat arms are mapped here; `x` and `wes` are `entry`
  verbs and are not compared at the fight prompt at all.
