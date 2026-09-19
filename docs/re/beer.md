# The beer routine — `FUN_1000_29c4`, all 666 bytes (Task 41)

`h` (drink one half-litre) and `mh` (drink until full or dry), the half-open
image range `1000:29c4`..`1000:2c5e`. Machine-readable twin:
`data/beer_arms.json`. Branch classification: `data/beer_uncited.json`.
Re-derived from `orig/g.exe` by `tools/test_beer_arms.py`.

Every claim below states its tier per `docs/re/METHODOLOGY.md`. Almost all of
them are **established from flow** — an aligned decode of all 298 instructions
from the function entry, obtained with

```
python3 tools/re_query.py resolve 1000:29c4 -n 666 -i 400
```

Where a claim is corroborated rather than established, it says so.

**`build/decomp/FUN_1000_29c4_1000_29c4.c` is a lead and was used as one.** It
annotates **106** of the routine's 298 instruction addresses; the other 192 —
including `1000:2a14`, the store that creates the value the whole `mh` tail
compares against, and `1000:2c36`, one of the nineteen branches — appear
nowhere in it. Ghidra also names the argument buffer `local_102` where the
machine encoding is `[bp-0x100]`, two bytes off, and represents the `[bp-0x102]`
snapshot as a register variable `iVar2` with no stack slot at all. Reading the
tail out of that C would have produced a two-byte address miss and a missing
store. The count is recomputed by `tools/test_beer_arms.py`'s
`test_the_decompilation_is_a_lead_not_a_census`.

## The range, and why it is 666 and not 670

`data/branches.json` records `entry 1000:29c4`, `size 666`. `0x29c4 + 666` is
`0x2c5e`, and the decode agrees: `1000:2c5b ret 0x4` is the last instruction and
`1000:2c5e` is the first byte of the 4275-byte literal run that
`docs/re/branches.md`'s undefined-run table lists at file 17710. The plan's
brief writes the end as `1000:2c61`; three bytes past `ret` is inside that
literal run, not inside the function. The four-byte difference is the ordinary
size of an RE miss here, so it is written down rather than silently corrected.

## Both call sites, and what they push

**Established from flow.** `tools/re_derive.py`'s `near_calls_to` over the whole
image returns exactly two, and `far_calls_to` returns none — which is
`data/branches.json`'s `caller_count: 2` reproduced independently.

| site | bytes | caller | pushes | buffer |
|---|---|---|---|---|
| `1000:e966` | `e8 5b 40` | `entry` | `1000:e961 mov di,0x3972` / `1000:e964 push ds` / `1000:e965 push di` | `20ae:3972`, the street line |
| `1000:4b00` | `e8 c1 de` | `FUN_1000_3d11` | `1000:4afb mov di,0x3a72` / `1000:4afe push ds` / `1000:4aff push di` | `20ae:3a72`, combat's own line |

**One of the two wraps, and only one.** `1000:e966`'s `rel16` is `0x405b` and
`0xe969 + 0x405b` is `0x129c4`, so only `& 0xffff` lands on the entry — the
mistake `near_calls_to` exists to avoid, and the reason a scan comparing the
unwrapped sum misses it. `1000:4b00`'s `rel16` is `0xdec1`, which as a signed
displacement is `-0x213f`, and `0x4b03 - 0x213f` is `0x29c4` with no wrap at
all: it calls backward. An earlier draft of this document said both wrapped;
`tools/test_beer_arms.py`'s
`test_both_call_sites_wrap_to_the_entry_and_there_is_no_third` caught it,
because it requires a site recorded as wrapping to actually need the wrap.

**The two differ in nothing the port must model.** Both push the same far-pointer
shape, and the routine's first act is to copy the shortstring into its own frame
(`1000:29de lds si,[bp+0x4]`), so the only difference is which buffer the text
came from. It reads no other caller state, and its complete write set is
`20ae:38ac` and `20ae:38c3`, both global. The port matches: `grep -rn 'self\.beer(' src/`
returns two call sites, one in `Game::run_command` and one in `Game::run_combat`,
each passing only the `Beer` discriminant.

## The prologue, and the two locals

**Established from flow**, `1000:29c4`..`1000:29ee`:

```
29c4  55              push bp
29c5  89 e5           mov bp,sp
29c7  b8 02 01        mov ax,0x102
29ca  9a cd 02 78 0f  call 0xf78:0x2cd   ; the stack check (docs/re/rtl.md)
29cf  81 ec 02 01     sub sp,0x102        ; 256 buffer + 1 word
29d3  8c d3           mov bx,ss
29d5  8e c3           mov es,bx           ; ES := SS -- the string ops stay on the frame
29d7  8c db           mov bx,ds
29d9  fc              cld
29da  8d be 00 ff     lea di,[bp-0x100]
29de  c5 76 04        lds si,[bp+0x4]     ; the caller's shortstring, FAR
29e1  ac              lodsb
29e2  aa              stosb               ; the length byte
29e3  91              xchg ax,cx
29e4  30 ed           xor ch,ch
29e6  f3 a4           rep movsb           ; the content
29e8  8e db           mov ds,bx
```

* **`[bp-0x100]`** is the local copy of the typed line. `1000:29e2` and
  `1000:29e6` are the only instructions in the whole routine whose destination
  is that buffer, and their destination is `ES:DI` with `ES` set to `SS` at
  `1000:29d5` — so the copy cannot reach DGROUP, and the line cannot change
  after `1000:29e6`.
* **`[bp-0x102]`** is a separate word BELOW the buffer. It is written exactly
  once, at `1000:2a14`, and read four times.

`1000:2c5b ret 0x4` pops the four bytes of far pointer the caller pushed.

## The gates, in the original's order

`1000:29f0 mov di,0x28c7` pushes the token `h`; `1000:29f5` is Borland's shortstring
compare `0f78:0bd8`; `1000:29fa jz 0x2a11` is the hit. `1000:2a02 mov di,0x28c9`
does the same for `mh` at `1000:2a0c jz 0x2a11`, and `1000:2a0e jmp 0x2c58`
returns for every other line — **no store and no output**, which is what makes
`h`/`mh` invisible to a dispatcher scan of `entry`.

Then, in order:

1. `1000:2a11 mov ax,[0x38ac]` / `1000:2a14 mov [bp-0x102],ax` — the hp snapshot,
   taken **before** the jaw test, which is what lets the refusal path still mean
   "nothing was drunk" downstream.
2. `1000:2a18 cmp byte [0x38b0],0x1` / `1000:2a1d jnz 0x2a3b` — the broken jaw.
   Not taken, `1000:2a1f mov di,0x28cc` prints the refusal and `1000:2a38 jmp 0x2baa`
   goes **into the `mh` tail**, not to the return.
3. **Loop top, `1000:2a3b`.** `1000:2a3e cmp ax,[0x38ae]` / `1000:2a42 jl 0x2a47`
   — hp < hpmax, SIGNED. Missed, `1000:2a44 jmp 0x2b67` prints CS `0x297c` and
   returns.
4. `1000:2a47 cmp word [0x38c3],0x0` / `1000:2a4c jnle 0x2a51` — beer > 0, SIGNED.
   Missed, `1000:2a4e jmp 0x2b3a` runs the no-beer arm.
5. `1000:2a51 dec [0x38c3]` — one half-litre, unconditionally, before any
   message. `1000:2a55 mov ax,[0x38ae]` / `1000:2a58 sub ax,[0x38ac]`
   is the shortfall and `1000:2a5c cmp ax,0x5` / `1000:2a5f jl 0x2a64` sizes the
   drink. Missed, `1000:2a61 jmp 0x2ae7` takes the flat-5 arm.
6. **The partial arm, `1000:2a64`..`1000:2ae7`.** `1000:2a74 jnz 0x2a94` skips
   CS `0x28fd` unless the line is `h`; `1000:2a94 mov ax,[0x38ae]` /
   `1000:2a97 mov [0x38ac],ax` tops hp up to hpmax; `1000:2aaa jnz 0x2ae5` skips
   CS `0x2914`.
7. **The flat arm, `1000:2ae7`..`1000:2b3a`.** `1000:2ae7 add word [0x38ac],0x5`,
   then `1000:2afc jnz 0x2b38` skips the combined CS `0x2938`.
8. **The no-beer arm, `1000:2b3a`..`1000:2b67`.** `1000:2b4a jnz 0x2b65` skips
   CS `0x2970`. `mh` writes nothing here and lets the loop-continue test end the
   loop.
9. **The already-healthy arm, `1000:2b67`..`1000:2b83`.** CS `0x297c` is written
   **unconditionally** — it is the only message in the routine with no `h`/`mh`
   gate — and `1000:2b80 jmp 0x2c58` returns, so this arm never reaches the tail.
10. **The loop-continue test, `1000:2b83`.** `1000:2b93 jnz 0x2b97` — `h` takes
    the fallthrough `1000:2b95 jmp short 0x2baa` and leaves after exactly one unit.
    `mh` runs `1000:2b9a cmp ax,[0x38ae]` / `1000:2b9e jnl 0x2baa` and
    `1000:2ba0 cmp word [0x38c3],0x0` / `1000:2ba5 jle 0x2baa`, and only when
    both miss does `1000:2ba7 jmp 0x2a3b` take the back edge.
11. **The tail's gate, `1000:2baa`.** `1000:2bb0 mov di,0x28c9` /
    `1000:2bba jz 0x2bbf` — everything below is `mh`-only, because
    `1000:2bbc jmp 0x2c58` is `h`'s exit.

## The three `[bp-0x102]` compares — what the slot holds and what each decides

**Established from flow.** The slot holds **hp as it stood on entry**: written
by `1000:2a11` / `1000:2a14`, and an aligned decode of all 298 instructions
finds no second writer. Reads: `1000:2bc2`, `1000:2bd0`, `1000:2c09`,
`1000:2c32`.

| branch | guard | decides | taken |
|---|---|---|---|
| `1000:2bc6` | `1000:2bc2 cmp ax,[bp-0x102]` | hp == hp0 → skip the aggregate line CS `0x2938` | `1000:2c06` |
| `1000:2c0d` | `1000:2c09 cmp ax,[bp-0x102]` | hp == hp0 → skip CS `0x29b3` | `1000:2c2f` |
| `1000:2c36` | `1000:2c32 cmp ax,[bp-0x102]` | hp != hp0 → skip CS `0x2970` | `1000:2c58` |

(`[bp+0xfefe]` is how `data/branches.json` renders the same `fe fe`
displacement the decoder prints as `[bp-0x102]`; `0x10000 - 0x102 == 0xfefe`.)

The second and third are paired with a beer test — `1000:2c0f cmp word [0x38c3],0x0`
/ `1000:2c14 jnle 0x2c2f`, and `1000:2c38 cmp word [0x38c3],0x0` /
`1000:2c3d jnle 0x2c58` — so the tail is really **two** independent predicates,
`hp != hp0` and `beer <= 0`, tested three times, and it writes:

| healed | dry | writes |
|---|---|---|
| yes | yes | the aggregate CS `0x2938`, then CS `0x29b3` (`^4Кончилось пиво`) |
| yes | no | the aggregate CS `0x2938` only |
| no | yes | CS `0x2970` (`^4Пива нету`) |
| no | no | nothing |

`1000:2bd0 sub ax,[bp-0x102]` is what makes the aggregate line's first
field the **total** healed across the whole `mh` run rather than the last unit's
gain.

## The string census — nine literals, and the pool tiles

**Established from flow**, and complete rather than listed. The routine holds
**17** `mov di,imm16` / `push cs` / `push di` sequences pushing **9** distinct
CS offsets, and the 253 bytes immediately before the entry are exactly those 9
Pascal shortstrings: walking `p += 1 + img[p]` from `0x28c7` lands on `0x29c4`
with **zero** residue. There is no room for a tenth. That run is
`docs/re/branches.md`'s undefined-run row `1000:28c7`, file 16791, length 253, 9 records, 0 residual.

An image-wide byte scan of `bf <imm16>` finds **no** push of any of the nine
outside this routine — the pool is private.

| CS | file | role | pushed at | text |
|---|---|---|---|---|
| `0x28c7` | `0x04197` | token | `1000:29f0`, `1000:2a6a`, `1000:2aa0`, `1000:2af2`, `1000:2b40`, `1000:2b89` | `h` |
| `0x28c9` | `0x04199` | token | `1000:2a02`, `1000:2bb0` | `mh` |
| `0x28cc` | `0x0419c` | message | `1000:2a1f` | `^4Ты не можешь пить пиво из-за сломаной челюсти.` |
| `0x28fd` | `0x041cd` | message | `1000:2a76` | `^2Пиво прибавляет #з. ` |
| `0x2914` | `0x041e4` | message | `1000:2aac` | `^2Здоровья:#/#. Осталось #.#л. пива` |
| `0x2938` | `0x04208` | message | `1000:2afe`, `1000:2bc8` | `^2Пиво прибавляет #з. Здоровья:#/#. Осталось #.#л. пива` |
| `0x2970` | `0x04240` | message | `1000:2b4c`, `1000:2c3f` | `^4Пива нету` |
| `0x297c` | `0x0424c` | message | `1000:2b67` | `^6Блин только тупить не надо - и так здоровья до фига.` |
| `0x29b3` | `0x04283` | message | `1000:2c16` | `^4Кончилось пиво` |

**`data/strings.json` does not hold the two tokens.** File `0x04197` and
`0x04199` are missing from it, and their five bytes are exactly the
`1000:28c7` / `0x4197` / 5 bytes row of `docs/re/branches.md`'s unaccounted-bytes table
— two artifacts corroborating each other. The seven offsets the shipped doc
comment lists are the seven **messages**; it is not a census of the pool.

Every message but one goes through `1000:2a33 call 0xeed:0x1c2`
(`WriteLn`, per `docs/re/character-sheet.md`) with five numeric format words —
zeros where the string has no `#`. The exception is CS `0x28fd`, written by
`1000:2a8f call 0xeed:0x0` (`Write`, no newline), so CS `0x2914` continues the
same physical line.

## The store scan — hp and beer, and nothing else

**Established from flow.** Every instruction in the range whose destination is
an absolute DGROUP address, from the aligned decode:

```
1000:2a51  ff 0e c3 38     dec [0x38c3]
1000:2a97  a3 ac 38        mov [0x38ac],ax
1000:2ae7  83 06 ac 38 05  add word [0x38ac],0x5
```

Three, and that is the whole set. The routine never writes `20ae:38ae` and never
writes `20ae:38b0`. The only other memory destinations anywhere in the 298
instructions are `1000:29e2 stosb`, `1000:29e6 rep movsb` (`ES:DI`, and `ES` is
`SS`) and `1000:2a14 mov [bp-0x102],ax` — all on the frame. Recompute:

```
python3 tools/test_beer_arms.py BeerTest.test_the_recorded_effects_are_every_absolute_write_in_range
```

Identity of the four globals, each confirmed with
`python3 tools/re_query.py xrefs-to 20ae:<off>`:

| DGROUP | what | census | the writer that settles it |
|---|---|---|---|
| `20ae:38ac` | hp | 74 refs, 33 writes | combat's damage line `1000:4783 sub [0x38ac],ax` moves this and not `20ae:38ae` |
| `20ae:38ae` | hpmax | 59 refs, 15 writes, **none in this range** | the level-up raises both together (`1000:2695 inc [0x38ae]` beside `1000:2699 inc [0x38ac]`) |
| `20ae:38b0` | broken jaw | 17 refs, 5 writes, every one an immediate `0` or `1` | `1000:47ee mov byte [0x38b0],0x1` in combat sets it |
| `20ae:38c3` | beer, HALF-litres | 19 refs, 4 writes | the shop's `1000:beb4 inc [0x38c3]` buys one; this routine's `1000:2a51` spends one |

Half-litres, not litres: `data/save_layout.json` records the field as `i16`
`beer_half_litres`, and `docs/re/character-sheet.md` records the sheet printing
`[0x38c3] div 2` with a `.5` for the odd half. This routine spends exactly one
unit for five hp.

## The arithmetic, and the zero-draw finding

The litres/tenths pair is `[0x38c3] div 2` and `(([0x38c3] mod 2) * 5) mod 10`,
built three times — at `1000:2ab9`, `1000:2b0f` and `1000:2bdd` — from a signed
`idiv 2`, an `xchg ax,dx` for the remainder, `mov si,ax` / `shl ax,1` /
`shl ax,1` / `add ax,si` for the `*5`, and a second `idiv` by ten. The `mod 10`
is real: `1000:2ad5`, `1000:2b2b` and `1000:2bf9` are its three `cwd`s.

**Zero draws.** Established two ways, because a zero count on its own is the
check that cannot fail:

1. The aligned decode of all 298 instructions holds exactly **four** distinct
   call targets — `0f78:02cd`, `0f78:0bd8`, `0eed:0000`, `0eed:01c2` — and
   `0f78:114b` is not among them.
2. A byte scan for `9a 4b 11 78 0f` over `[0x29c4, 0x2c62)` returns **zero**
   hits, while the same scan over the whole image returns the **86** sites
   `docs/re/METHODOLOGY.md` names. That second number is the negative control.

```
python3 tools/test_beer_arms.py BeerTest.test_the_range_spends_no_draw_and_the_sweep_that_says_so_works
```

So `tests/combat_sequence.rs` and `tests/wander_sequence.rs` must stay green
draw-for-draw across any change to `Game::beer`; movement in either is a bug,
never a rebaseline.

## The audit of the shipped doc comment — and what Task 42 did with it

The `Game::beer` doc comment is the artifact Task 41 audited, not a source it
may inherit from. Recompute what it says with
`grep -n -B2 -A48 'fn beer(&mut self, how: Beer)' src/game.rs`. Two of its
addresses were load-bearing and correct; the rest were near misses, and one
was a miscount.

**Task 42 CORRECTED every row of the table below in place** rather than adding
new addresses beside the old ones, because `docs/re/METHODOLOGY.md` is explicit
that an address four bytes off a guard reads as authoritative. The table is
kept as the record of what was wrong and what replaced it: the "the comment
says" column is what the comment said at Task 41's tree, and the third column
is what it says now. Check the replacement rather than this prose:
`grep -n '1000:2a3b\|1000:2a55\|1000:2b83' src/game.rs` returns four lines and
**none of them sits on a condition** — the doc comment's own disclosure of this
correction, the loop's back-edge target in the same comment, the shortfall load
beside `1000:2a58`, and one test's doc. Each of the three addresses is now used
for what the instruction at it actually is, never for the decision four to
sixteen bytes later.

**Correct, and re-derived here.** `1000:e966` is the call site and its bytes
really are `e8 5b 40`; `1000:29f0` and `1000:2a02` really are the two token
pushes, at file `0x4197` and `0x4199`; `1000:2a18` really is the jaw guard;
`1000:2a47` really is the beer guard; `1000:2a51` really is the `dec`;
`1000:4b00` really is the second call site.

**Wrong, with the correction.**

| the comment says | what is there | the guard/branch it meant |
|---|---|---|
| "Six later `\"h\"` compares (`1000:2a6a`, `2aa0`, `2af2`, `2b40`, `2b89`)" | **five** addresses under the word "six" | the census is six pushes of CS `0x28c7` **in total** — `1000:29f0` plus those five |
| "`1000:2a3b` already at full hp" | `1000:2a3b mov ax,[0x38ac]`, a load | guard `1000:2a3e`, branch `1000:2a42` |
| "`1000:2a55`: when the shortfall is under 5" | `1000:2a55 mov ax,[0x38ae]`, a load | guard `1000:2a5c`, branch `1000:2a5f` |
| "`1000:2b83` `h` stops after that one unit" | `1000:2b83 lea di,[bp-0x100]` | branch `1000:2b93` |
| "The `#.#л.` pair is `beer/2` and `(beer mod 2) * 5` (`1000:2ab9`)" | `1000:2ab9` is the load that opens the idiom, and the expression drops the `mod 10` at `1000:2ad5` | `1000:2ab9`..`1000:2adb` |
| "file `0x4240` if nothing was drunk at all" | that line needs a **second** conjunct | `1000:2c36` **and** `1000:2c3d` — nothing drunk **and** beer exhausted |

**The miscount appeared twice, and not in the same words.** `src/game.rs` said
"Six later `"h"` compares"; `src/commands.rs`'s module doc said "Six
**further** `"h"` compares". Grepping the `game.rs` wording in `commands.rs`
came up empty, so it took two commands to find both. **Task 42 corrected
both.** The replacements are `grep -rn '\*\*Five\*\* further\|\*\*five\*\* later' src/`
— one hit each, `src/commands.rs` and `src/game.rs` — and each now spells the
census out (six pushes in all, `1000:29f0` plus five) instead of leaving a bare
count over a list that contradicts it. The old wording survives **only** where
each comment quotes it to say it was wrong, which
`grep -rn '"Six further"\|"six later"' src/` shows: those two disclosures and
nothing else. Both greps were run against this tree; a bare
`grep -rn 'Six later\|Six further' src/` would still hit one of the
disclosures, which is why the quoted forms are the commands written down here.

## The classification, and the one divergence (closed)

All nineteen branches are `implemented`; `data/beer_uncited.json` carries the
rows, the `src` construct for each, and the command that finds it. **Task 42
wrote all nineteen addresses into `src/`**, so `docs/re/branches.md`'s
per-entry row for `1000:29c4` went from `touched 2` to `touched 19`, and the
5-branch uncited span the ranking carried across this routine's tail left it.
Both figures are the Recomputation block's output at a named commit, quoted
there, never asserted here. (The span's own endpoints are not instruction
addresses and are deliberately not repeated in this file — `docs/re/branches.md`
is where a span is written down.)
`unimplemented` and `unreachable-in-port` are both **empty**, which is stated
rather than filled — see that file's `empty_classes_are_reported` for the
candidates that were checked and rejected.

**One boolean for six compares.** `Game::beer` uses a single `single` boolean
where the original runs six further shortstring compares. That is faithful, and
the argument is flow, not convenience: `1000:29fa` and `1000:2a0c` admit exactly
two lines into the body and `1000:2a0e` returns for every other, and the buffer
at `[bp-0x100]` is written only by `1000:29e2` and `1000:29e6` in the prologue.
So each later compare re-evaluates a predicate that cannot have changed.

**The one divergence: `mh` with a broken jaw skips the tail — CLOSED by Task
42.** Miss the jaw gate at `1000:2a1d` and the refusal at CS `0x28cc` prints,
and then `1000:2a38 jmp 0x2baa` lands in the tail rather than at the return.
With hp untouched since the snapshot at `1000:2a14`, `1000:2bba jz 0x2bbf`
admits `mh`, `1000:2bc6 jz 0x2c06` and `1000:2c0d jz 0x2c2f` both take,
`1000:2c36 jnz 0x2c58` does not, and `1000:2c3d jnle 0x2c58` falls through
whenever `20ae:38c3` is at or below zero — so `mh` + broken jaw + no beer writes
the refusal **and** `^4Пива нету` (CS `0x2970`, pushed at `1000:2c3f` and
written by `1000:2c53 call 0xeed:0x1c2`). The `Game::beer` Task 41 audited
returned immediately after the refusal and wrote one line.

It was reachable — `20ae:38b0` is set by combat at `1000:47ee` and
`1000:4820`, and `mh` is accepted inside a fight through `1000:4b00` — so
Task 42 **fixed** it rather than keeping it: the refusal arm no longer returns
and the drink loop moved into its `else`, so both arms reach the `1000:2bba`
gate. `docs/re/gaps.md`'s entry "`mh` with a broken jaw skips the tail the
original still runs" now carries the closure and the reason, and
`data/beer_uncited.json`'s `divergences[0]` its `status` and `fixed_by`. The
behaviour is held by `cargo test --lib
a_broken_jaw_still_runs_the_tail_for_mh_and_returns_for_h` and that assertion
has been seen failing, as `python3 tools/mutate.py --case
beer-jaw-arm-falls-into-the-tail`.

**Everything else compared clean.** The gate order, the flat-5 versus top-up
split, the six `h`/`mh` suppressions, the ungated full-health line, the loop
termination and all four combinations of the tail's two predicates match the
shipped `Game::beer` construct for construct; the comparison is the
row-by-row `src` block of `data/beer_uncited.json`. Task 41 established that by
reading; Task 42 put assertions under the three properties a `src` diff can
silently break without moving any state — `the_beer_gates_run_in_the_originals_order`
(jaw before full-health, full-health before no-beer),
`the_h_arms_write_the_literals_the_pool_holds` (each arm's exact literal,
including the two strings `1000:2a8f`'s `Write` joins into one line), and
`mh_stops_at_hpmax_and_at_the_last_half_litre` (both exits from the loop, and
what the tail writes for each).

## Two signedness postures, both already recorded

* `1000:2a4c`, `1000:2ba5`, `1000:2c14` and `1000:2c3d` read `20ae:38c3` with
  the SIGNED `jg`/`jle` pair while `Fighter::beer_dl` is a `u16`. This is the
  same posture `docs/re/gaps.md` records for the character sheet's quantity
  gates, closed by the load path's `.max(0) as u16`; no new entry is opened.
* `1000:2a42` and `1000:2b9e` compare hp against hpmax signed while both port
  fields are `u16`. `docs/re/gaps.md`'s open entry "`Fighter::hp` is a `u16`
  and the original's is a signed word" owns that; a saturated 0 takes the same
  arm as a negative value here, so this routine sees no difference from it.
