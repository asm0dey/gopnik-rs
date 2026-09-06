# The gym (`trn`) and the joint (`kos`) — Task 31

Machine-readable twin: `data/gym_arms.json`. Both are re-derived from
`orig/g.exe` by `tools/test_gym_arms.py`; neither reads `src/`, a screen, or
Ghidra's decompiled C. `docs/re/den.md` and `data/den_arms.json` are the shape
this follows.

Every claim below states its tier per `docs/re/METHODOLOGY.md`. Almost all of
them are **established from flow** — an aligned decode of
`1000:e390`..`1000:ea94` walked forward from `entry` (`1000:ab59`) — and the
few that are not say so.

**This document changes no Rust.** It is the map a porting task consumes; what
the port has to do with it is in "What the port must change" at the end, and in
`data/gym_arms.json`'s `what_the_port_must_change` block.

## Address convention

`1000:xxxx` is a Ghidra label (Form A of `docs/re/METHODOLOGY.md`) and, because
the game's code segment is the load base, its OFF is also the image offset.
`20ae:xxxx` is DGROUP. `0eed:` and `0f78:` are Form B runtime segments. A "CS
offset" is the image offset of a Pascal shortstring's length byte — the exact
immediate of the `mov di,imm16` that pushes it — and the file offset is that
plus the `0x18d0`-byte MZ header.

Two near calls in this range encode a `rel16` that **wraps**: `1000:e7df`
reaches `1000:2526` and `1000:e966` reaches `1000:29c4` only modulo 64 KiB.
`tools/re_derive.py`'s `near_calls_to` is the helper that exists for exactly
that mistake, and it is what found both.

## Range and boundaries

The half-open image range `1000:e390`..`1000:ea94`, 790 instructions.

* `1000:e390` **is** the `trn` verb compare. Its buffer push, five bytes ahead
  of the literal push, is `1000:e386 mov di,0x3972` — so the verb is read out of
  the STREET buffer, not the gym's own `20ae:3a72`.
* `1000:e973` is the `kos` verb compare — the split between the two handlers.
* `1000:ea94` **is** the `i` verb compare and is *not* decoded here.

`data/command_dispatch.json` is the independent authority on all three, and
`tools/test_gym_arms.py` asserts agreement with it rather than restating it.

```
1000:e386  mov di,0x3972
1000:e38b  mov di,0xa353
1000:e390  call 0xf78:0xbd8
1000:e395  jz 0xe39a
1000:e397  jmp 0xe961
1000:e39a  cmp byte [0x369a],0x1
1000:e39f  jz 0xe3a4
1000:e3a1  jmp 0xe948
```

`0xa353` is `trn` (CS `0xa353`, file `0x0BC23`) and `0xa61f` is `kos`
(CS `0xa61f`, file `0x0BEEF`).

The verb MISS at `1000:e397 jmp 0xe961` and the gym's own exit both land there,
which is not the next verb: it is a shared tail that pushes the street buffer
and calls `1000:29c4`, the beer sub-dispatcher for `h`/`mh`. `1000:e966` is one
of its two callers image-wide (the other, `1000:4b00`, is inside the fight).
`src/game.rs`'s `Game::beer` already documents that routine; the new fact here
is only that the gym's exit passes through it.

## The whole shape, in order

| span | what |
|---|---|
| `1000:e390`..`1000:e39a` | `trn` verb compare |
| `1000:e39a`..`1000:e3a4` | discovery gate on `20ae:369a` |
| `1000:e3a4`..`1000:e3e7` | the trained-armour recompute into `20ae:3e34` |
| `1000:e3e7`..`1000:e400` | the intro line |
| `1000:e400`..`1000:e5e4` | five menu lines, printed ONCE |
| `1000:e5e4`..`1000:e624` | prompt, `ReadLn`, case fold |
| `1000:e624`..`1000:e948` | six key arms |
| `1000:e948`..`1000:e961` | the undiscovered refusal |
| `1000:e961`..`1000:e969` | the shared beer tail |
| `1000:e969`..`1000:ea8a` | `kos` |
| `1000:ea8a`..`1000:ea94` | the `i` verb's own setup |

Those spans tile the range end to end with no gap and no overlap;
`tools/test_gym_arms.py` asserts the tiling, so a block cannot be dropped from
this map by being left out of every span.

The discovery gate `1000:e39a cmp byte [0x369a],0x1` refuses with
`^6Ты пока незнаешь где в этом районе качалка` (CS `0xa5f2`, file `0x0BEC2`),
printed at `1000:e95c`, and falls straight into the shared tail. The submenu is
never entered and no `ReadLn` happens.

## The headline: what the second block is

The task this map answers first is "what is `1000:e633`..`1000:e941`, and how
is it reached from `1000:e400`..`1000:e594`?" — established from flow, not from
the printed menu.

**It is the key dispatch. The first block is the menu print.** They are not two
tiers, not a repeat-purchase loop, and not a sub-verb.

*How it is reached:* fall-through, once. The last menu line's `WriteLn` at
`1000:e5df` is followed by the prompt at `1000:e5e4`, the three-call `ReadLn` at
`1000:e60b`..`1000:e615`, the case fold at `1000:e61f`, and then the first key
compare at `1000:e62e`. No branch anywhere jumps INTO the second block, and the
only branch back is `1000:e943`, whose target is `1000:e5e4` — the **prompt**,
not the menu. So the menu prints once per visit and an unrecognised key is
silent.

*How different they are, measured.* Identical guard constants are a hypothesis,
not a finding, so the two spans were diffed rather than eyeballed:

| pair | menu | arm | result |
|---|---|---|---|
| row 3 level predicate | `1000:e4b1`..`1000:e4c2` | `1000:e746`..`1000:e757` | 17 bytes, **byte-identical**; only the `jcc` after differs, and it differs in sense |
| row 5 armour predicate | `1000:e57d`..`1000:e58d` | `1000:e87f`..`1000:e894` | 16 bytes against 21, **not identical** |
| five price compares | `1000:e400`, `1000:e455`, `1000:e4c4`, `1000:e521`, `1000:e58f` | `1000:e635`, `1000:e6c1`, `1000:e774`, `1000:e801`, `1000:e896` | five bytes each, all byte-identical |
| three district compares | `1000:e4aa`, `1000:e51a`, `1000:e576` | `1000:e728`, `1000:e7e2`, `1000:e861` | five bytes each, all byte-identical |

The row-5 difference is exact, and it is a different *number*, not a different
spelling of the same one:

```
menu  1000:e57d  a0 92 36 30 e4          d1 e0                 8b d0 a0 34 3e 30 e4 3b c2
arm   1000:e87f  a0 92 36 30 e4          48 48 ba 0a 00 f7 e2  8b d0 a0 34 3e 30 e4 3b c2
```

Five identical head bytes, nine identical tail bytes, and in between `shl ax,1`
(district × 2) against `dec ax` / `dec ax` / `mov dx,0xa` / `mul dx`
(`(district − 2) × 10`).

```
1000:e57d  mov al,[0x3692]
1000:e582  shl ax,1
1000:e584  mov dx,ax
1000:e586  mov al,[0x3e34]
1000:e58b  cmp ax,dx
1000:e58d  jnl 0xe5e4
```

```
1000:e87f  mov al,[0x3692]
1000:e884  dec ax
1000:e886  mov dx,0xa
1000:e889  mul dx
1000:e88b  mov dx,ax
1000:e88d  mov al,[0x3e34]
1000:e892  cmp ax,dx
1000:e894  jnl 0xe8f9
```

Over the two spans as wholes — 405 bytes against 783 — the **longest byte run
they share is 25**, at `1000:e496` and `1000:e714`, and it is five zeroed
`WriteLn` format words, the `call 0eed:01c2` after them, and the first five
bytes of a district compare. That is boilerplate every print in the image
shares, not a shared body. `tools/test_gym_arms.py` recomputes that run rather
than remembering it.

Two more asymmetries the constants would never have shown:

* The `4` arm refuses when the guard is already owned
  (`1000:e7fa cmp byte [0x394a],0x0`). Menu row 4's only gate is
  `1000:e51a cmp byte [0x3692],0x1`, so the row stays listed after the purchase
  and the arm answers `^6У тебя есть эта штучка.` (CS `0xa54a`, file `0x0BE1A`).
* In the menu the price compare is **cosmetic**: both of its arms fall into the
  same string assembly and only the colour byte `20ae:3b7a` differs. No menu row
  is ever hidden by price. In the arms the identical five bytes are a refusal
  that skips the whole effect block.

## `20ae:3e34`, the trained armour

**Established from flow**, `1000:e3a4`..`1000:e3e2`. The gym seeds a scratch
byte from the armour byte `20ae:38b2` and subtracts back out the armour that
came from bought clothing, leaving the part the player *trained*.

```
1000:e3a4  mov al,[0x38b2]
1000:e3a7  mov [0x3e34],al
1000:e3aa  cmp byte [0x38b4],0x0
1000:e3af  jz 0xe3bc
1000:e3b1  cmp byte [0x38b7],0x0
1000:e3b6  jnz 0xe3bc
1000:e3b8  dec [0x3e34]
1000:e3bc  cmp byte [0x38b7],0x0
1000:e3c1  jz 0xe3c8
1000:e3c3  sub byte [0x3e34],0x2
1000:e3c8  cmp byte [0x38b6],0x0
1000:e3cd  jz 0xe3db
1000:e3cf  cmp byte [0x38b9],0x0
1000:e3d4  jnz 0xe3db
1000:e3d6  sub byte [0x3e34],0x2
1000:e3db  cmp byte [0x38b9],0x0
1000:e3e0  jz 0xe3e7
1000:e3e2  sub byte [0x3e34],0x4
```

The four subtrahends are the four `mar` rows' own advertised bonuses: `20ae:38b4`
is the abibas suit (+1, set at `1000:bf80`), `20ae:38b7` the adidas suit (+2,
`1000:c183`), `20ae:38b6` the leather jacket (+2, `1000:c0e0`) and `20ae:38b9`
the крутая кожанка (+4, `1000:c2ca`).

**The three "unported first-block gates" the brief names.** `1000:e395` is not
part of this block at all — it is the `trn` verb compare's own hit branch, and
its fallthrough `1000:e397` is the miss; it is "unported" only in the sense that
`src/` cites the guard and not the branch. The two that *are* in this block,
`1000:e3b6` and `1000:e3d4`, are each the SECOND test of a pair, and what they
skip is the *smaller* subtraction of a pair of items:

* the `jnz` at `1000:e3b6` skips the `-1` for the abibas suit when the adidas
  suit is also owned;
* the `jnz` at `1000:e3d4` skips the `-2` for the plain jacket when the крутая
  кожанка is also owned.

So the pairs are exclusive by subtrahend rather than additive: both suits cost
2, not 3, and both jackets cost 4, not 6. That is why `SAVE_R4`, which carries
`20ae:38b4`, `20ae:38b6` and `20ae:38b7` with armour 10, computes
`10 − 2 − 2 = 6` and not `10 − 1 − 2 − 2 = 5`.

**Who reads it.** `python3 tools/re_query.py xrefs-to 20ae:3e34` reports eight
references, all of them between `1000:e3a7` and `1000:e8da`, so the byte is
gym-local: it appears in no other handler, and `data/save_layout.json` has no
field for it — it is rebuilt on every entry. Two of the eight are reads, and
**they use different thresholds**:

| read | threshold | address of the compare |
|---|---|---|
| `1000:e586` — the MENU's row-5 visibility test | `district * 2` | `1000:e58b` |
| `1000:e88d` — the `5` ARM's own ceiling | `(district - 2) * 10` | `1000:e892` |

`docs/re/gaps.md` said "Exactly one thing reads the result", which stopped one
search short; the entry is corrected there. At district 3 the row disappears at
trained armour 6 while the arm keeps working to 10, so the arm stays usable
through a menu that no longer lists it — which is consistent with the loop
reprinting only the prompt.

The remaining six references are writes: the seed `1000:e3a7`, the four
subtractions above, and `1000:e8da`, where the `5` arm raises it in step with
the armour byte.

## The intro and the five menu lines

The intro is `Ты пришел в качалку напиши  ^6w^7  чтобы уйти` (CS `0xa357`, file
`0x0BC27`), written at `1000:e3fb`.

Every row is assembled the same way — `0f78:0ae7` (`rtl_str_assign`) copies the
prefix into a local, `0f78:0c03` (`rtl_char_to_str`) turns the colour byte into
a one-character string, `0f78:0b66` (`rtl_str_append`) appends it, a second
`0f78:0b66` appends the row text, and `0eed:01c2` writes the line. Five rows,
so five of each of the first two calls and ten appends; the counts are asserted.
Names from `data/rtl_names.json`.

| row | price | gates | colour test | prefix | text |
|---|---:|---|---|---|---|
| 1 | 20 | none | `1000:e400` | CS `0xa178` | CS `0xa385` |
| 2 | 20 | none | `1000:e455` | CS `0xa1ad` | CS `0xa3b0` |
| 3 | 10 | `1000:e4aa` district > 1; `1000:e4be` `district*10-3 > level` | `1000:e4c4` | CS `0xa3de` | CS `0xa3e6` |
| 4 | 30 | `1000:e51a` district > 1 | `1000:e521` | CS `0xa405` | CS `0xa40d` |
| 5 | 20 | `1000:e576` district > 2; `1000:e58b` trained armour `< district*2` | `1000:e58f` | CS `0xa44b` | CS `0xa453` |

The prefixes are ` 1 -  ^` (CS `0xa178`), ` 2 -  ^` (CS `0xa1ad`),
` 3 -  ^` (CS `0xa3de`), ` 4 -  ^` (CS `0xa405`) and ` 5 -  ^` (CS `0xa44b`) —
each ending in a bare `^` that the colour digit completes. The row texts are
`20^7  качаться гателями и шгангой(Сила +1)` (CS `0xa385`),
`20^7  качаться на тренажерах(Выносливость +1)` (CS `0xa3b0`),
`10^7  прокачать # качков опыта` (CS `0xa3e6`),
`30^7  купить зубную защиту боксёров(-75% что сломают челюсть)` (CS `0xa40d`)
and `20^7  прокачать пресс(Броня +1)` (CS `0xa453`). The typos are the
original's.

Row 3's single `#` is filled from a separate immediate `1000:e505 mov ax,0xa`,
not from the price compare at `1000:e4c4`; the two happen to be equal.
`src/game.rs`'s `print_imm_rows` already records that.

All five rows are ported (`IMM_ROWS` plus `Game::imm_row_visible`); this section
exists to bound the second block, and because the row-5 gate is the one the port
gets from the wrong value.

## Prompt, `ReadLn`, and the loop

```
1000:e5e4  mov di,0xa473
1000:e5f8  call 0xeed:0x0
1000:e5fd  mov di,0x3ecc
1000:e602  mov di,0x3a72
1000:e607  mov ax,0xff
1000:e60b  call 0xf78:0x6c6
1000:e610  call 0xf78:0x59d
1000:e615  call 0xf78:0x291
1000:e61a  mov di,0x3a72
1000:e61f  call 0xeed:0x216
```

The prompt is `^0Качалка\` (CS `0xa473`, file `0x0BD43`), written with
`0eed:0000` (no newline), so the typed line continues it. The submenu buffer is
`20ae:3a72` — the same one the den, the dealers and the vet use — and
`0eed:0216` lowercases ASCII `A`..`Z` in place and does **not** trim. So gym
keys are case-insensitive and whitespace-SENSITIVE; `Game::shop_turn` trims,
which is the divergence `docs/re/gaps.md`'s trimmed-prompt entry already owns.

The loop is `1000:e5e4` (top) → `1000:e943 jmp 0xe5e4` (back edge) with
`1000:e946 jmp short 0xe961` as the exit, and `1000:e61f call 0xeed:0x216` is
the whole of the input normalisation. Nothing between them reprints the menu.

## The six arms

Six keys, each established at a COMPARE address on `20ae:3a72`, never from the
menu text: `1` (`1000:e62e`), `2` (`1000:e6ba`), `3` (`1000:e73c`),
`4` (`1000:e7f3`), `5` (`1000:e875`), `w` (`1000:e93c`).

Three of them sit behind a district gate that jumps over the compare itself —
`1000:e72d ja 0xe732` is the `3` arm's, and its fallthrough jumps the whole arm.
So **at district 1 exactly three keys exist** — `1`, `2` and `w` — and typing
`3` there is silent rather than refused. That is a claim about the dispatcher,
and `tools/test_gym_arms.py` checks it by requiring each district gate's failure
target to be past its arm's compare.

Five priced arms share only FOUR "not enough money" literals: `^4Не хватает`
(CS `0x8e4d`) is used by both `1` and `2`, and the other three each have their
own — `^4Не хватает деньжат` (CS `0x9473`) for `3`,
`^4А не хватает рубликов` (CS `0xa51f`) for `4` and
`^4Не хватает рубликов` (CS `0xa564`) for `5`. The last two differ by one
character. Attributing the wrong one to an arm is the mistake the literal sweep
exists to catch.

### `1` — качаться гантелями и штангой, 20 rubles

Span `1000:e624`..`1000:e6b0`; key literal CS `0x8dca`; compare `1000:e62e`;
miss `1000:e633`. Money gate `1000:e635` / `1000:e63a`, refusing with
`^4Не хватает` (CS `0x8e4d`, file `0x0A71D`). On success it prints
`^2Ты прокачиваешь силу.` (CS `0xa47e`, file `0x0BD4E`) and then:

```
1000:e657  sub word [0x38c7],0x14
1000:e675  inc [0x389e]
1000:e679  inc [0x38ae]
1000:e67d  inc [0x38ac]
1000:e681  mov ax,[0x389e]
1000:e684  cwd
1000:e685  mov cx,0x2
1000:e688  idiv cx
1000:e68a  xchg ax,dx
1000:e68b  or ax,ax
1000:e68d  jnz 0xe693
1000:e68f  inc [0x38a8]
1000:e693  inc [0x38aa]
```

**The damage split is the one thing here that is easy to get backwards.**
`1000:e68d` is a two-byte `jnz` with a displacement of 4, and `1000:e68f` is
exactly four bytes long, so the jump lands on `1000:e693`, not past it:

* урон max (`20ae:38aa`, `1000:e693 inc [0x38aa]`) rises on **every**
  purchase;
* урон min (`20ae:38a8`, `1000:e68f inc [0x38a8]`) rises only when the **new**
  strength is even.

The closing line is `^1Сила +1 ` (CS `0x9402`, file `0x0ACD2`) at `1000:e6ab`;
the string says +1 and `1000:e675` is an `inc`, so string and effect agree —
checked, not assumed. Nothing one-shot is consumed, so the arm is repeatable.

### `2` — качаться на тренажерах, 20 rubles

Span `1000:e6b0`..`1000:e728`; key literal CS `0x8e4b`; compare `1000:e6ba`;
miss `1000:e6bf`; money gate `1000:e6c1` / `1000:e6c6` with the same
`^4Не хватает` (CS `0x8e4d`). Confirmation
`^2Ты прокачиваешь выносливость.` (CS `0xa496`, file `0x0BD66`), closing line
`^1Выносливость +1 ` (CS `0xa4b6`, file `0x0BD86`).

```
1000:e6e3  sub word [0x38c7],0x14
1000:e701  inc [0x38a2]
1000:e705  add word [0x38ae],0x5
1000:e70a  add word [0x38ac],0x5
```

Same price and same refusal literal as `1`, and **no conditional inside**: both
health words rise by a flat 5 and neither damage word is touched. The two arms
are not a template of each other — `1` has six effects and a parity branch, `2`
has four and none.

### `3` — прокачать 10 качков опыта, 10 rubles

Span `1000:e728`..`1000:e7e2`. Own gate `1000:e728` / `1000:e72d`
(district > 1). Key literal CS `0x8ea5`; compare `1000:e73c`.

Then the level gate, whose 17 bytes are identical to menu row 3's:

```
1000:e746  mov al,[0x3692]
1000:e74b  mov dx,0xa
1000:e74e  mul dx
1000:e750  sub ax,0x3
1000:e753  cmp ax,[0x38a6]
1000:e757  jnle 0xe774
```

Failing it prints `^6Ты слишком крутой чтобы тренироваться здесь.`
(CS `0xa4c9`, file `0x0BD99`). The money gate `1000:e774` / `1000:e779` refuses
with `^4Не хватает деньжат` (CS `0x9473`, file `0x0AD43`) — a *different*
refusal literal from arms `1` and `2`, and different again from arms `4` and
`5`; four distinct "not enough money" strings in one handler.

```
1000:e796  sub word [0x38c7],0xa
1000:e7b4  add word [0x38ce],0xa
1000:e7d3  mov ax,[0x38ce]
1000:e7d6  cmp ax,[0x38d0]
1000:e7da  jl 0xe7e2
1000:e7dc  mov al,0x0
1000:e7de  push ax
```

with `^2Ты тренируешься.` (CS `0xa4f8`, file `0x0BDC8`) and
`^1 +# качков опыта ` (CS `0xa50b`, file `0x0BDDB`) printed in between, the
latter's `#` filled from its own `1000:e7be mov ax,0xa`.

`1000:e7df` calls `1000:2526`, the level-up already documented in
`docs/re/progression.md` and ported as `crate::progress::apply_levels`. The
pushed `param_1` is 0, the CAPPED call — level stops at 40 and the closing
Сейчас у тебя # качков опыта line is printed (`docs/re/progression.md` quotes it
from CS 0x24ea). The den's own level-up call at `1000:dec5` pushes the same 0.

**This is the only arm in the range that can move the RNG stream**, and it moves
it indirectly: the gym and the joint contain zero `Random` call sites, but
`1000:2526` spends two draws per level gained at `1000:25fe`. "No draws in
range" is not "no draws".

The outer guard at `1000:e7d3`..`1000:e7da` duplicates the callee's own entry
test — `a1 ce 38 3b 06 d0 38` is byte-identical at `1000:e7d3` and
`1000:2535`, and only the following `jcc` differs — and the callee's early-out
target `1000:28c1` is past its closing message at `1000:28a6`. So the guard
changes nothing observable. It is recorded because a port that keeps it and a
port that drops it are both faithful, and the map should say which.

### `4` — купить зубную защиту боксёров, 30 rubles

Span `1000:e7e2`..`1000:e861`. Own gate `1000:e7e2` / `1000:e7e7`
(district > 1). Key literal CS `0x8ef7`; compare `1000:e7f3`; miss `1000:e7f8`.

```
1000:e7fa  cmp byte [0x394a],0x0
1000:e7ff  jnz 0xe848
1000:e801  cmp word [0x38c7],0x1e
1000:e806  jnl 0xe823
1000:e823  sub word [0x38c7],0x1e
1000:e828  mov byte [0x394a],0x1
```

Strings: `^4А не хватает рубликов` (CS `0xa51f`, file `0x0BDEF`),
`^2Ты купил защиту.` (CS `0xa537`, file `0x0BE07`) and
`^6У тебя есть эта штучка.` (CS `0xa54a`).

The only purchase in the gym that sets a flag rather than bumping a number.
`python3 tools/re_query.py xrefs-to 20ae:394a` reports five references, of which
`1000:e828 mov byte [0x394a],0x1` is the **only** absolute-memory write
image-wide — nothing clears
it, not even the district reset at `1000:abbd` that clears the discovery flags.
(The save loader restores it as part of the 182-byte state block,
`data/save_layout.json` `.SAV` offset 686; that is a block copy, not an
absolute-memory operand, so it is outside that census by construction.) What the
flag buys is `1000:47ce`/`1000:47f3`: it splits a jaw break into the plain arm
and a `Random(4)` — a draw-count difference, not flavour.

### `5` — прокачать пресс, 20 rubles

Span `1000:e861`..`1000:e932`. Own gate `1000:e861` / `1000:e866`
(district > 2). Key literal CS `0x8f6b`; compare `1000:e875`. Ceiling gate
`1000:e892` / `1000:e894` against `(district − 2) × 10` (the disassembly is
quoted under the headline above). Money gate `1000:e896` / `1000:e89b`, refusing
with `^4Не хватает рубликов` (CS `0xa564`, file `0x0BE34`) — a distinct literal
from arm `4`'s CS `0xa51f`, differing only by the leading `А `, so the length
bytes are 21 and 23.

```
1000:e8b8  sub word [0x38c7],0x14
1000:e8d6  inc [0x38b2]
1000:e8da  inc [0x3e34]
```

with `^2Ты прокачиваешь пресс.` (CS `0xa57a`, file `0x0BE4A`) and
`^1Броня +1` (CS `0xa593`, file `0x0BE63`) printed around them.

`1000:e8d6 inc [0x38b2]` is the visible Броня and `1000:e8da inc [0x3e34]` the
scratch, and both are unconditional once the gates pass. **Both must happen**:
the second is the value the next purchase is tested against, so a port that
raises only the armour never stops.

At the ceiling the arm prints
`^6Ты максимально прокачал пресс для своего уровня` (CS `0xa59e`, file
`0x0BE6E`) and then, only while `1000:e912 cmp byte [0x3692],0x4` /
`1000:e917 jnb 0xe932` lets it through — that is, at district 3 and no
higher — `^6Качай дальше в следующем районе` (CS `0xa5d0`, file `0x0BEA0`).

### `w` — leave

Span `1000:e932`..`1000:e948`; key literal CS `0x848e`; compare `1000:e93c`.
The hit `1000:e941` reaches `1000:e946`, which jumps to the shared tail; the
MISS `1000:e943` is the loop back edge. The arm writes nothing and prints
nothing — both measured over its own span, not omitted. The `w` literal is
shared with every other location's exit, which is why the port keeps one shared
exit arm.

## `kos`, the joint

`kos` is a **street verb**, not a submenu. Established from flow: there is no
prompt literal, no `0eed:0000`-then-`ReadLn` pair and no back edge anywhere in
`1000:e97d`..`1000:ea8a`, and every path ends at `1000:ea8a`, the `i` verb's own
setup. It sits next to the gym in address space and nowhere else, which is why
this map keeps it in its own section and why the port leaves it with
`Game::smoke`.

```
1000:e97d  cmp byte [0x38b0],0x1
1000:e982  jnz 0xe9a0
1000:e9a0  cmp byte [0x38cd],0x0
1000:e9a5  jz 0xe9aa
1000:e9aa  cmp word [0x38c5],0x0
1000:e9af  jnle 0xe9b4
1000:e9b4  dec [0x38c5]
1000:e9b8  mov byte [0x38cd],0xa
1000:e9bd  add word [0x389e],0x2
1000:e9c2  inc [0x38a8]
1000:e9c6  add word [0x38aa],0x2
1000:e9cb  mov ax,[0x38ae]
1000:e9ce  sub ax,[0x38ac]
1000:e9d2  cmp ax,0xa
1000:e9d5  jnl 0xea19
1000:e9f8  mov [0x38ac],ax
1000:ea19  add word [0x38ac],0xa
```

Three refusals: `^4Ты не схавать колёса из-за сломаной челюсти.` (CS `0xa623`,
file `0x0BEF3`) for a broken jaw, `^6Ты неможешь схавать ещё один косяк.`
(CS `0xa6e8`, file `0x0BFB8`) while the countdown is non-zero, and
`^4У тебя нет косяков` (CS `0xa6d3`, file `0x0BFA3`) with none left.

The heal splits like the beer routine's. When the shortfall is under 10,
`^2Колёса прибавляют #з. ` (CS `0xa652`, file `0x0BF22`) is written **without a
newline** at `1000:e9f0` and completed by
`^2Здоровья:#/#. Осталось # косяков` (CS `0xa66b`, file `0x0BF3B`), whose three
`#` are `20ae:38ac`, `20ae:38ae` and `20ae:38c5` in push order. Otherwise the
single combined `^2Колёса прибавляют #з. Здоровья:#/#. Осталось # косякова`
(CS `0xa68e`, file `0x0BF5E`) carries the flat 10 first. The trailing `косякова`
is the original's typo. Either way the arm closes with `^2Сила +2.`
(CS `0xa6c8`, file `0x0BF98`).

**`kos` is ported in full** by `Game::smoke` — `grep -n 'fn smoke' src/game.rs`
finds it. Its branches read `port_touched: false` in `data/branches.json`
because that column is a CITATION proxy over `src/**/*.rs`
(`data/branches.json`'s own `port_cross_reference.metric` says so), and
`src/game.rs` cites the guard `1000:e97d` but not the branches. Three places the
port's representation differs, with the reason each is equivalent, are recorded
in `data/gym_arms.json`'s `joint.port_equivalences`:

* `1000:e9a5` reads the countdown `20ae:38cd`; the port reads a `stoned` bool.
  They are written and cleared together and never independently, including
  `stoned: save.buff_countdown != 0` on load, so a loaded save cannot
  desynchronise them.
* `1000:e9af` is a SIGNED `> 0`; the port tests `== 0` on a `u16`, where the
  negative half is unrepresentable. A future change of that field to a signed
  type would break the equivalence, which is why it is written down.
* `1000:e9d5` compares a signed shortfall; the port uses `saturating_sub`. When
  hp exceeds hpmax the original's value is negative, `jnl` fails, and it takes
  the top-up branch that lowers hp to hpmax — which is the branch
  `saturating_sub`'s 0 also takes.

## The counts this map rests on

Over `1000:e390`..`1000:ea94`, all asserted by set equality in
`tools/test_gym_arms.py`:

| sweep | count |
|---|---:|
| instructions (aligned) | 790 |
| CS-literal pushes | 46 |
| conditional branches | 43 |
| `Random` call sites | **0** |
| absolute-memory writes | 39 |
| DGROUP addresses touched | 23 |
| DS-pointer pushes | 12 |
| shortstring compares | 8 |

A zero is not evidence on its own, so the `Random` sweep also runs over the
whole image and is required to find the population `docs/re/METHODOLOGY.md`
records (86 far calls). Without that second half, "no draws in the gym" would be
a check that cannot fail.

## Branch coverage, and why two numbers disagree

`data/branches.json` holds 38 branches in `1000:e390`..`1000:e972` and 5 in
`1000:e973`..`1000:ea93`. Its `port_touched` column marks **37** and **4** of
them untouched. Task 31's brief says 23 and 3; that pair is reproduced by a
different rule — scan `src/*.rs` for `1000:xxxx` citations and count a branch as
touched when its own address OR its guard's address appears, the rule this
document's wander cross-check block uses in `docs/re/branches.md`. Both numbers
are right about different questions, and both are recorded in
`data/gym_arms.json`'s `branch_census` with the command that recomputes them, so
a later reader does not "correct" one into the other.

Neither is a to-do list. `port_touched == false` means "no address citation",
never "unimplemented" — `kos` is the proof: fully ported, four branches
untouched.

## What the port must change

The full, falsifiable list is `data/gym_arms.json`'s
`what_the_port_must_change`. In summary:

1. `Game::shop_turn` recognises **no gym key**. Five exist, at
   `1000:e62e`, `1000:e6ba`, `1000:e73c`, `1000:e7f3` and `1000:e875`; `w`
   (`1000:e93c`) is already the shared exit.
2. Arm `1`'s damage split: урон max always, урон min only on an even
   strength. Both-conditional and both-unconditional are equally wrong and
   equally invisible in a screen capture.
3. Arm `3` is `money -= 10`, `xp += 10`, print, and only then the level-up. In
   `apply_levels` terms that is a manual `xp += 10` and a later call with
   `award = 0, uncapped = false`, because `apply_levels` adds its own award
   before the threshold test.
4. Arm `5` needs the trained armour against `(district − 2) × 10`, which is NOT
   the menu row's `district × 2`, and must raise both `20ae:38b2` and
   `20ae:3e34`.
5. Arm `4` refuses when already owned; the menu row does not gate on that. Do
   not "fix" the menu.
6. The menu prints once; the back edge returns to the prompt; an unrecognised
   key is silent and there is no string in range for a "не понял" line.
7. At district 1 the keys `3`, `4` and `5` are not compared at all, so typing
   one is silent rather than refused.
8. `kos` needs no change.
9. The gym's `ReadLn` does not trim; `Game::shop_turn` does. That is the
   existing trimmed-prompt gap, with the gym added to its population.

Nothing in this range is *blocked*: every global a gate reads is written
somewhere in `src/`, and the two routines the range calls out to are both
already ported. The one thing the port lacks is the trained-armour value itself,
and the four flags it is computed from have all been writable in play since
Task 26 — so that is missing code, not a missing input.
