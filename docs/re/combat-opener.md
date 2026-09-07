# The class-keyed combat opener — `1000:3d32`..`1000:3e8d` (Task 39)

The first thing `FUN_1000_3d11` does, on the two `param_1` values that reach it,
is greet the enemy. `docs/re/combat.md` had the block's shape and its zero-draw
property; `docs/re/gaps.md` carried its **text** as an open hole in two places.
This file closes that hole, and adds the zero-*store* property the draw claim
never covered.

Machine-readable twin: **`data/combat_opener.json`**, and
`python3 tools/test_combat_opener.py` re-derives **both halves** from
`orig/g.exe` — every address checked for alignment (a walk from
`FUN_1000_3d11`'s own entry reaches it) *and* identity (the instruction there
says what the record says), every string re-decoded from CP866 at the file
offset the `mov di,imm16` names, and every sweep recomputed as a SET EQUALITY
so an omission reds the test rather than shrinking the claim.

Addresses are Ghidra form A; `tools/addr.py` is the executable authority.
`CS <hex>` offsets are image offsets inside the game's code segment, `20ae:`
offsets are DGROUP. Russian is verbatim, typos included.

---

## Two dispatches, not one, and they key on different things

**Established from flow.** The confusion this block invites is that there are
two compare chains within thirty bytes of each other and they read different
values.

```
1000:3d24  8a 46 04         mov al,[bp+0x4]        ; the FUNCTION PARAMETER
1000:3d27  3c 00            cmp al,0x0
1000:3d29  74 07            jz 0x3d32
1000:3d2b  3c 06            cmp al,0x6
1000:3d2d  74 03            jz 0x3d32
1000:3d2f  e9 5b 01         jmp 0x3e8d             ; param_1 1 / 3 / 4 / 5
1000:3d32  a1 52 39         mov ax,[0x3952]        ; the ENEMY's CLASS
1000:3d35  3d 00 00         cmp ax,0x0
```

`docs/re/gaps.md` records the outer chain correctly — "0 and 6 take the same
target `1000:3d32`" — and it is about `param_1`, the opponent-kind argument
`entry` and the den pass in. The inner chain, the subject of this file, keys on
`20ae:3952`, the **rolled enemy's class**. A reading that calls the outer pair
"classes 0 and 6" collapses the two, and the two are not the same variable.

**Both inherited claims verified, not inherited.**

* *"The arm 0 and 6 actually take is `1000:3d32`..`1000:3e8d` … 168
  instructions."* An aligned walk from `FUN_1000_3d11`'s entry produces exactly
  **168** instructions in that half-open range and they tile it end to end with
  no gap and no overlap.
* *"whose only exit is `1000:3e8a jmp 0x3fa7`."* Every other branch and jump in
  the range targets an address inside it. Confirmed by decoding all **fifteen**
  of them — ten conditional, five unconditional. (The fourteen `call`s in range
  are far calls into the runtime and transfer nothing within the code segment.)
* *"entered only by `1000:3d29 jz 0x3d32` and `1000:3d2d jz 0x3d32`."* A sweep
  of every aligned branch and jump in segment `1000` finds exactly those two
  targeting anything in the range from outside it.

## `20ae:3952` — the census

**Established from flow.** `python3 tools/re_query.py xrefs-to 20ae:3952`
accepts **45** references image-wide out of 45 raw hits, discarding none:
`FUN_1000_0d14` 30, `entry` 8, `FUN_1000_3d11` 4, `FUN_1000_1348` 2,
`FUN_1000_11c2` 1.

The four inside `FUN_1000_3d11` are all **loads**, and the function writes the
word nowhere:

| at | what |
|---|---|
| `1000:3d32` | this chain's key |
| `1000:528b` | the victory block's понтовость award, `class + 1 + level div 3` |
| `1000:541b` | the joint arm's `class == 2` test |
| `1000:5473` | the item table's class key |

The writers are elsewhere. `FUN_1000_0d14` — the wander's enemy generator —
clamps its roll at 9 (`1000:0d9a cmp word [0x3952],0x9` / `1000:0d9f jle 0xda7`
/ `1000:0da1 mov word [0x3952],0x9`), and `FUN_1000_11c2` stores **10** outright
at `1000:11d0 mov word [0x3952],0xa`. Ten is `Ректор НГУ` in
`data/string_tables.json`'s `ranks`. That matters below.

## The chain: ten class values, six arms

**Established from flow.** The ten `cmp ax,N` links run `N = 0`..`9` in order.
Five arms print; the eleventh case — every value the chain does not name —
prints nothing at all.

| classes | gates, in the original's order | arm | exit |
|---|---|---|---|
| 0, 1, 2 | `1000:3d35`/`3d38`, `3d3a`/`3d3d`, `3d3f`/`3d42` | `1000:3d44` | `1000:3d76 jmp 0x3e8a` |
| 3, 4, 5, 6 | `1000:3d79`/`3d7c`, `3d7e`/`3d81`, `3d83`/`3d86`, `3d88`/`3d8b` | `1000:3d8d` | `1000:3dbf jmp 0x3e8a` |
| 7 | `1000:3dc2`/`3dc5` | `1000:3dc7` | `1000:3de0 jmp 0x3e8a` |
| 8 | `1000:3de3`/`3de6` | `1000:3de8` | `1000:3e33 jmp short 0x3e8a` |
| 9 | `1000:3e35`/`3e38` | `1000:3e3a` | falls into `1000:3e8a` |
| ≥ 10 | — | — | `1000:3e38 jnz 0x3e8a`, silent |

The layout is **not** symmetric, which is why the map keeps each arm's gates in
their own span: the gates for 0..2 sit *before* the first body, and every later
arm's gates sit *after* the previous body.

### The strings

Every one was read out of `orig/g.exe` as a length-prefixed CP866 shortstring at
the file offset the `mov di,imm16` names, `cs_offset + 0x18d0`.
`docs/re/gaps.md`'s known-starts list (`0x452E`, `0x453B`, `0x4548`, `0x4565`,
`0x457A`, …) is five of the **nine** the range pushes; that trailing "…" was
doing real work, and the other four are `0x4587`, `0x4597`, `0x45A7` and
`0x45B5`.

| push | CS | file | text |
|---|---|---|---|
| `1000:3d44` | `0x2c5e` | `0x452E` | `Слышь Вась..` |
| `1000:3d5d` | `0x2c6b` | `0x453B` | `^4А чё ваще?` |
| `1000:3d8d` | `0x2c78` | `0x4548` | `^4Пацан ты из какого района?` |
| `1000:3da6` | `0x2c95` | `0x4565` | `А ты по пинкам суди!` |
| `1000:3dc7` | `0x2caa` | `0x457A` | `^4Эй мудак?!` |
| `1000:3dee` | `0x2cb7` | `0x4587` | `^4Блин! это же ` |
| `1000:3e02` | `0x2cc7` | `0x4597` | `^4 - известный ` |
| `1000:3e3a` | `0x2cd7` | `0x45A7` | `^4Я МАНЬЯК!!!` |
| `1000:3e59` | `0x2ce5` | `0x45B5` | `Рад познакомиться - ` |

Nine CS-literal pushes is the whole set: the sweep collects every
`mov di,imm16` followed by `push cs` / `push di` across all 168 instructions and
matches it against the artifact.

### The arms for classes 0–7 are plain `WriteLn`s

`1000:3d58`, `1000:3d71`, `1000:3da1`, `1000:3dba` and `1000:3ddb` are
`call 0eed:01c2` with the literal's far pointer and five zeroed format words
pushed ahead of them — the shape every fence in `docs/re/den.md` drops.

### Arm 8 splices the player's name and rank

```
3de8  8d be e8 fd      lea di,[bp-0x218]     ; a STACK local; push ss, not ds
3dee  bf b7 2c         mov di,0x2cb7         ; '^4Блин! это же '
3df3  9a e7 0a 78 0f   call 0f78:0ae7        ; rtl_str_assign
3df8  bf 9c 37         mov di,0x379c         ; the player's NAME, .SAV 0x100
3dfd  9a 66 0b 78 0f   call 0f78:0b66        ; rtl_str_append
3e02  bf c7 2c         mov di,0x2cc7         ; '^4 - известный '
3e07  9a 66 0b 78 0f   call 0f78:0b66
3e0c  8b 3e 9c 38      mov di,[0x389c]       ; the PLAYER's class
3e10  b1 08            mov cl,0x8
3e12  d3 e7            shl di,cl
3e14  81 c7 2e 00      add di,0x2e           ; ranks[class], base 20ae:002e
3e1a  9a 66 0b 78 0f   call 0f78:0b66
3e2e  9a c2 01 ed 0e   call 0eed:01c2
```

So class 8 — `Мент` — opens with **`^4Блин! это же <name>^4 - известный
<rank>`**. `1000:3e0c` reads `20ae:389c`, the *player's* record, not the
enemy's; `[0x389c] shl 8 + 0x2e` is exactly the 256-byte-stride table
`data/string_tables.json` records at base `74718` (`0x123DE` = `20ae:002e`).

The `WriteLn` at `1000:3e2e` has no visible string push because `0f78:0ae7` and
`0f78:0b66` return with `retf 0x4`: each pops only its **source** and leaves the
destination far pointer on the stack, which `0eed:01c2`'s `retf 0xe` — one far
pointer plus five format words — is what consumes. Same reading as
`docs/re/den.md`'s `1000:d9d4`.

### Arm 9 prints twice

`1000:3e3a` writes `^4Я МАНЬЯК!!!` on its own, then builds
`Рад познакомиться - <rank>` in the same stack local (`1000:3e53` /
`1000:3e5e` / `1000:3e71`) and writes it at `1000:3e85`. Class 9 is `Маньячок`,
and the rank spliced is again the **player's**, from `[0x389c]`.

Arm 9 is the only arm that does not jump to the exit: `1000:3e85`'s `WriteLn` is
followed directly by `1000:3e8a`.

### The eleventh case prints nothing

`1000:3e38 jnz 0x3e8a` is the chain's last miss, so any value of `[0x3952]` the
ten compares do not name reaches the exit having printed nothing. That is the
whole flow claim, and it is all this file asserts.

**Whether that case ever runs is NOT established here.** One writer does store
such a value — `1000:11d0 mov word [0x3952],0xa`, the `Ректор НГУ` row — but
reaching this chain with it also needs a call whose `param_1` is 0 or 6 while
the word still holds 10, and the two rector fights `docs/re/combat-dispatch.md`
names pass kinds 3 and 4 (`1000:ae2d`, `1000:ae39`), which take
`1000:3d2f jmp 0x3e8d` and never enter the range at all. Settling it means
tracing `FUN_1000_11c2`'s own callers, which Task 39 did not do. Recorded as
open rather than resolved by symmetry.

## The block spends no draw **and writes no state**

**Established from flow.** The draw half was already recorded: no
`9a 4b 11 78 0f` in `[0x3d11, 0x3f00)`. Re-measured over this range it is again
**zero**, and the same signature swept over the whole image finds **86** — the
population `docs/re/METHODOLOGY.md` names — so the zero is a measurement and not
a scan that found nothing because it was looking wrongly.

The store half is new, and it is what the draw claim never covered: a block that
spends no draw can still move the save file.

Exactly **five** instructions in the range name memory at all:

| at | text | what |
|---|---|---|
| `1000:3d32` | `mov ax,[0x3952]` | read, the enemy class |
| `1000:3de8` | `lea di,[bp-0x218]` | address arithmetic, not an access |
| `1000:3e0c` | `mov di,[0x389c]` | read, the player class |
| `1000:3e53` | `lea di,[bp-0x218]` | address arithmetic |
| `1000:3e63` | `mov di,[0x389c]` | read, the player class |

Three absolute reads, **zero** absolute writes, no `stos`, no `movs`, no `rep`
and no write through a register operand. The only memory the two shortstring
helpers touch is the stack local at `ss:[bp-0x218]` — `1000:3dec` and
`1000:3e57` push `ss`, not `ds`.

**Consequence for the port:** the opener is print-only. Adding it cannot change
a byte of state or a single RNG draw, so `tests/combat_sequence.rs` and the five
frozen oracles under `data/` are unaffected by it.

## The branch skeleton

`data/branches.json` records ten `class == "game"` branches in the range, and at
Task 39 all ten were `port_touched: false`. **Task 40 ported the block and all
ten are now cited** — `src/combat_opener.rs`. Recompute with the block under
*Recomputation, from the shipped artifacts → Coverage* in
`docs/re/branches.md`, filtered to `func_entry == "1000:3d11"`.

`data/combat_uncited.json` is the classification of **all** of that function's
uncited game branches — the ten here and the rest of the function — against the
port, one row each, with `implemented` rows naming the `src/` construct that
already evaluates the condition. Since Task 40 its row set is PARTITIONED by
`port_status`: 115 `cited` and 2 `never-cited`. Both halves are derived by the
same block, not typed — `tools/test_combat_opener.py` requires the
`never-cited` rows to equal a fresh recomputation, requires every `cited` row
to name a `src/` citation that really exists, and requires every row to be a
`class == "game"` branch of the function at all — so no count in that lane is
written down by hand.

Where a row's port construct decides the same thing by a **different**
predicate, the file's `port_equivalences` says why the two agree and on what
assumption — the shape `data/gym_arms.json`'s `joint.port_equivalences` uses.
Three are recorded: the `_` arm of a Rust `match` standing in for the last
`cmp ax,N` link of a draw-keyed chain, `saturating_sub` standing in for a
signed shortfall, and a literal token compare standing in for
`crate::commands::parse` at the two verbs `parse` folds together.

## What Task 40 did with it

`src/combat_opener.rs` is the port: `greet(enemy_class, player_name,
player_rank)`, six arms, the ten links in the original's order, and the silent
default. It takes no `Rng` and returns nothing, because the block spends no
draw and writes no state — the two findings above are what let the module have
that signature at all.

`crate::game::Game::run_combat` calls it behind the OUTER chain
(`1000:3d27`..`1000:3d2f`), which is why that method now takes the original's
`param_1`. Only the opener gate reads it; the other four things the original
does with `param_1` are still unported and listed on `run_combat` itself.

Not everything in the range became a citation. `1000:3d29` and `1000:3d2d`
were already cited before Task 40 — they are the outer gate, not this chain —
and the eleventh case has no branch of its own to cite: `1000:3e38 jnz 0x3e8a`
IS the ninth link and carries the default with it.

## What this changes elsewhere

- `docs/re/gaps.md`, "The class-keyed combat-opener table" and "The class-keyed
  opener's text": both said the text was not extracted. It is, here — and Task
  40 then ported it, which both entries now record.
- `docs/re/combat.md`, "The class-keyed opener": the same sentence, and the
  zero-store finding is added beside the zero-draw one.
- `docs/re/gaps.md`, the `param_1 = 5` entry (the den's cop fight, `1000:ddfc`):
  *"What those 168 instructions do was not decoded"* — decoded here. What the
  cop fight loses by skipping the prologue is now answerable: at most two
  printed lines, no draw and no store.
