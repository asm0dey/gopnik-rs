# The club (`kl`) and the command list (`i`) — Task 33

Machine-readable twin: `data/club_arms.json`. Both are re-derived from
`orig/g.exe` by `tools/test_club_arms.py`; neither reads `src/`, a screen, or
Ghidra's decompiled C. `docs/re/gym.md` and `docs/re/den.md` are the shape this
follows.

Every claim below states its tier per `docs/re/METHODOLOGY.md`. Almost all of
them are **established from flow** — an aligned decode of
`1000:df06`..`1000:e390` and `1000:ea94`..`1000:ec82` walked forward from
`entry` (`1000:ab59`) — and the few that are not say so.

**This document changes no Rust.** It is the map a porting task consumes; what
the port has to do with it is in "What the port must change" at the end of each
half, and in `data/club_arms.json`'s two `what_the_port_must_change` blocks.

## Address convention

`1000:xxxx` is a Ghidra label (Form A of `docs/re/METHODOLOGY.md`) and, because
the game's code segment is the load base, its OFF is also the image offset.
`20ae:xxxx` is DGROUP. `0eed:` and `0f78:` are Form B runtime segments. A "CS
offset" is the image offset of a Pascal shortstring's length byte — the exact
immediate of the `mov di,imm16` that pushes it — and the file offset is that
plus the `0x18d0`-byte MZ header.

Four near calls in the club encode a `rel16` that **wraps**: `1000:e124` and
`1000:e21c` reach `1000:2526`, `1000:e181` reaches `1000:0d14`, and
`1000:e222` reaches `1000:3d11`, each only modulo 64 KiB.
`tools/re_derive.py`'s `near_calls_to` is the helper that exists for exactly
that mistake, and it is what found all four.

---

# Part 1 — the club, `1000:df06`..`1000:e390`

## Range and boundaries

The half-open image range `1000:df06`..`1000:e390`, 539 instructions.

* `1000:df06` **is** the `kl` verb compare. Its buffer push is
  `1000:defc mov di,0x3972` and its literal push `1000:df01 mov di,0xa0ea`
  (`kl`, CS `0xa0ea`, file `0x0b9ba`) — both five and ten bytes before the
  range start, so the verb is read out of the STREET buffer, not the club's
  own `20ae:3a72`.
* `1000:e390` **is** the `trn` verb compare and is *not* decoded here;
  `docs/re/gym.md` owns everything from it.

`data/command_dispatch.json` is the independent authority on both, and
`tools/test_club_arms.py` asserts agreement with it rather than restating it.

```
1000:defc  mov di,0x3972
1000:df01  mov di,0xa0ea
1000:df06  call 0xf78:0xbd8
1000:df0b  jz 0xdf10
1000:df0d  jmp 0xe386
1000:df10  cmp byte [0x3699],0x1
1000:df15  jz 0xdf1a
1000:df17  jmp 0xe36d
1000:df1a  cmp byte [0x3b77],0x0
1000:df1f  jbe 0xdf3d
```

## The whole shape, in order

| span | what |
|---|---|
| `1000:df06`..`1000:df10` | `kl` verb compare |
| `1000:df10`..`1000:df1a` | discovery gate on `20ae:3699` |
| `1000:df1a`..`1000:df3d` | the BAN gate on `20ae:3b77` and its refusal |
| `1000:df3d`..`1000:df6f` | the two intro lines |
| `1000:df6f`..`1000:dfc4` | menu row 1 |
| `1000:dfc4`..`1000:e020` | menu row 2, behind the district gate |
| `1000:e020`..`1000:e025` | the stake init |
| `1000:e025`..`1000:e065` | prompt, `ReadLn`, case fold |
| `1000:e065`..`1000:e274` | the `p` arm — the card game |
| `1000:e274`..`1000:e2e2` | the `1` arm |
| `1000:e2e2`..`1000:e357` | the `2` arm |
| `1000:e357`..`1000:e36b` | the `w` arm |
| `1000:e36b`..`1000:e36d` | the shared exit jump |
| `1000:e36d`..`1000:e386` | the undiscovered refusal |
| `1000:e386`..`1000:e390` | the `trn` verb's own setup |

Those spans tile the range end to end with no gap and no overlap;
`tools/test_club_arms.py` asserts the tiling, so a block cannot be dropped from
this map by being left out of every span.

## Two gates before the menu, not one

**Established from flow.** Two gates stand between the `kl` verb compare and
the intro, back to back, and only the first is ported.

The discovery gate `1000:df10 cmp byte [0x3699],0x1` refuses with
`^6Ты пока что неузнал где в этом районе клуб` (CS `0xa326`, file `0x0bbf6`),
printed at `1000:e381`, and falls into `1000:e386`. That gate is ported
(`grep -n '1000:df10' src/game.rs`).

The ban gate `1000:df1a cmp byte [0x3b77],0x0` is next, and it is **not**.
`jbe` on an unsigned byte compared against zero is `== 0`, so the club is open
only while the countdown is zero; a non-zero countdown falls THROUGH to
`1000:df21`, which prints `^6Тебе не стоит пока туда соваться` (CS `0xa0ed`,
file `0x0b9bd`) at `1000:df35` and leaves at `1000:df3a`.

`mar`'s countdown gate `1000:b95e cmp byte [0x3b76],0x0` decides the same
condition and lays it out the other way round: `1000:b963 jz 0xb968` takes the
open case into the intro and `1000:b965 jmp 0xc480` takes the refusal to a
distant block, where the club's refusal is the adjacent fall-through. Both take
the ZERO case out of the fall-through; what differs is the mnemonic (`jz`
against `jbe`, equivalent on an unsigned byte against 0) and where the refusal
lives.

`docs/re/gaps.md`'s "The two ban countdowns are modelled and decremented but
never set" already owns the omission and already names both `1000:df1a` and
`1000:e23e`. What this map adds is the *condition* under which `1000:e23e`
runs: it is not a random event, it is the punishment for winning six hands in a
row. See "The `p` arm" below.

## The two intro lines and the two menu rows

The intro is `Ты пришел в клуб напиши  ^6w^7  чтобы уйти` (CS `0xa110`, file
`0x0b9e0`) at `1000:df51`, then
` Здесь можно сыграть в карты (^6p^7 Минимальная ставка- 5р.)` (CS `0xa13b`,
file `0x0ba0b`) at `1000:df6a`. The second line is where `p` is advertised —
that is OUTPUT, and the key itself is established at the compare `1000:e06f`.

Both rows are assembled the same way as every other priced row in the image —
`0f78:0ae7` (`rtl_str_assign`), `0f78:0c03` (`rtl_char_to_str`), two
`0f78:0b66` (`rtl_str_append`) and `0eed:01c2`. Names from `data/rtl_names.json`.

| row | price | gates | colour test | prefix | text |
|---|---:|---|---|---|---|
| 1 | 15 | none | `1000:df6f` | CS `0xa178` | CS `0xa180` |
| 2 | 22 | `1000:dfc4` district > 1 | `1000:dfcb` | CS `0xa1ad` | CS `0xa1b5` |

The prefixes are ` 1 -  ^` (CS `0xa178`) and ` 2 -  ^` (CS `0xa1ad`) — the same
two the gym's rows 1 and 2 use — and the row texts are
`15^7  потусоваться на дискотеке(Ловкость +1)` (CS `0xa180`) and
`22^7  разузнать приемы мухлёжников(Удача +1)` (CS `0xa1b5`). Both are already
ported as `IMM_ROWS["kl","1"]` and `IMM_ROWS["kl","2"]`
(`grep -n '1000:df6f' src/game.rs`).

## The headline: what `1000:e283`..`1000:e366` is

The task this map answers first is the brief's own question: is
`1000:e283`..`1000:e366` a duplicate of `1000:df74`..`1000:dfd0`? The two spans
read the same `[0x38c7]` constants `0xf` and `0x16` and the same `[0x3692]`
district gate, which is a **hypothesis, not a finding**.

**It is not a duplicate. The first block is the MENU PRINT; the second is the
KEY DISPATCH.** Established from flow, and measured rather than eyeballed:

| pair | menu | arm | result |
|---|---|---|---|
| row-1 / arm-1 price predicate | `1000:df6f`..`1000:df74` | `1000:e285`..`1000:e28a` | 5 bytes, **byte-identical**; `jcc` `7c 07` against `7d 1b` — different sense AND different distance |
| row-2 / arm-2 price predicate | `1000:dfcb`..`1000:dfd0` | `1000:e2fa`..`1000:e2ff` | 5 bytes, **byte-identical**; same `7c 07` against `7d 1b` |
| district gate | `1000:dfc4`..`1000:dfc9` | `1000:e2e2`..`1000:e2e7` | 5 bytes, **byte-identical**, and the same `jbe` OPCODE; only the displacement differs, `55` against `6e` |

Over the two blocks as wholes — `1000:df6f`..`1000:e020` (177 bytes) against
`1000:e283`..`1000:e357` (212 bytes) — the **longest byte run they share is
26**, at `1000:dfb0` and `1000:e2ce`:

```
31 c0 50 31 c0 50 31 c0 50 31 c0 50 31 c0 50 9a c2 01 ed 0e 80 3e 92 36 01 76
```

That is five zeroed `WriteLn` format words, the `call 0eed:01c2` after them, the
district compare, and the single opcode byte `76` of the `jbe` that follows. The
run stops exactly on that opcode: the next byte is each `jbe`'s displacement,
and those are the first two bytes at which the blocks differ.
`tools/test_club_arms.py` recomputes the run rather than remembering it, and
requires the two bytes past its end to differ.

**Neither span is a free choice.** A longest-common-substring is span-relative,
so a span moved to flatter the number would make the finding say less than it
looks like it says: review round 1 showed that widening the arm span to
`1000:e366` and updating its recorded length to match passed green. Each
endpoint is now anchored to an address this map already carries for another
reason — the menu span runs from menu row 1's colour test to the stake init,
the arm span from arm `1`'s miss branch to arm `2`'s span end — and
`tools/test_club_arms.py` resolves those four anchors BEFORE it looks at any
length. The finding is also robust to the choice: the review measured the run
over a second span pair and got the same 26 at the same two addresses.

**What the identical predicates DO is different in kind:**

* In the menu the price compare is **cosmetic**. Both arms of `1000:df74` and
  `1000:dfd0` store into `20ae:3b7a` (`1000:df76` / `1000:df7d`,
  `1000:dfd2` / `1000:dfd9`) and reconverge, so no club row is ever hidden by
  price. In the arms the identical five bytes are a refusal that skips the whole
  effect block.
* The menu's district gate skips a PRINT (`1000:dfc9 jbe 0xe020`, past row 2's
  text). The arm's district gate skips a key COMPARE
  (`1000:e2e7 jbe 0xe357`, past `1000:e2f3`). So at district 1 the row is not
  listed *and* typing `2` is **silent**, not refused — the same shape the gym's
  three gated keys have.

## Prompt, `ReadLn`, and the loop

```
1000:e020  mov byte [0x3c82],0x5
1000:e025  mov di,0xa1e2
1000:e039  call 0xeed:0x0
1000:e03e  mov di,0x3ecc
1000:e043  mov di,0x3a72
1000:e048  mov ax,0xff
1000:e04c  call 0xf78:0x6c6
1000:e051  call 0xf78:0x59d
1000:e056  call 0xf78:0x291
1000:e05b  mov di,0x3a72
1000:e060  call 0xeed:0x216
```

The prompt is `^0Клуб\` (CS `0xa1e2`, file `0x0bab2`), written with `0eed:0000`
(no newline), so the typed line continues it. The submenu buffer is
`20ae:3a72` — the same one the den, the dealers, the vet and the gym use — and
`0eed:0216` lowercases ASCII `A`..`Z` in place and does **not** trim. So club
keys are case-insensitive and whitespace-SENSITIVE; `Game::shop_turn` trims,
which is the divergence `docs/re/gaps.md`'s trimmed-prompt entry already owns.

The loop is `1000:e025` (top) → `1000:e368 jmp 0xe025` (back edge) with
`1000:e36b jmp short 0xe386` as the exit. Nothing between them reprints the
menu, and an unrecognised key is silent: the branch inventory over the range is
a set equality against `data/branches.json`'s independent population, so there
is no unnamed gate for a refusal to hang on, and every CS literal pushed between
the case fold and the exit is attributed to a named arm.

**`1000:e020` is outside the loop.** It is five bytes before the loop top, and
it is the join point of the district gate at `1000:dfc9`. So the stake is reset
once per VISIT and carries across keys within a visit. Resetting it per
iteration would make the whole `p` arm unreachable past its first hand.

## `20ae:3c82` — the stake, named

**Established from flow.** `docs/re/tables.md`'s "`20ae:3c82`, the one debited
byte variable" records all four idioms that touch it, and says twice that what
it prices is not established and its `what` stays `null`. It is the club card
game's **stake**.

`python3 tools/re_query.py xrefs-to 20ae:3c82` reports 14 references, 14
accepted, 0 discarded, and **every one of them falls between `1000:e020` and
`1000:e25d`** — so the byte is club-local exactly as `20ae:3e34` is gym-local,
and `data/save_layout.json` has no field for it. Three of the fourteen are
writes:

| at | what |
|---|---|
| `1000:e020` | `:= 5` on entry, once, outside the loop |
| `1000:e0f7` | `+= 2` after a win |
| `1000:e145` | `:= 5` after a loss |

and the compares are `1000:e14a` and `1000:e174` against 17 and `1000:e151`
against 5. Six consecutive wins walk 5 → 7 → 9 → 11 → 13 → 15 → 17, and 17 is
where the house calls the player a cheat. `tools/test_club_arms.py` walks that
ladder with the three DECODED immediates rather than with the numbers in this
sentence.

## The `p` arm — `1000:e065`..`1000:e274`

Key literal CS `0x9ec1` (`p`, file `0x0b791` — the same literal the den's beer
key uses); compare `1000:e06f`; hit `1000:e074`, miss `1000:e076`.

### The money gate

```
1000:e079  mov al,[0x3c82]
1000:e07c  xor ah,ah
1000:e07e  cmp ax,[0x38c7]
1000:e082  jle 0xe087
1000:e084  jmp 0xe258
```

The operands are the other way round from the club's own `1` and `2` arms
(`cmp word [0x38c7],imm`) and from the gym's five money gates: it is the STAKE
in `ax` compared against money, so `jle` passes when `stake <= money` and
equality buys. The refusal is `^6Не хватает денег - надо #.` (CS `0xa2d4`, file
`0x0bba4`) with the stake as its `#` (`1000:e25d`) — and it is none of the
four "not enough money" literals `docs/re/gym.md` records for the gym, so the
two handlers between them carry five.

On success `Ты поставил # рублей` (CS `0xa1ea`, file `0x0baba`) prints with the
stake, and then `1000:e0a8 sub [0x38c7],ax` debits it.

### The draw and the compare

```
1000:e0ac  mov al,[0x3692]
1000:e0b1  mov dx,0xc
1000:e0b4  mul dx
1000:e0b7  call 0xf78:0x114b
1000:e0bc  xor dx,dx
1000:e0be  mov cx,ax
1000:e0c0  mov bx,dx
1000:e0c2  mov ax,[0x38a4]
1000:e0c5  cwd
1000:e0c6  cmp dx,bx
1000:e0c8  jnle 0xe0d0
1000:e0ca  jl 0xe129
1000:e0cc  cmp ax,cx
1000:e0ce  jb 0xe129
```

`1000:e0b7 call 0xf78:0x114b` is the **only** `Random` call site in
`1000:df06`..`1000:e390`, and
`python3 tools/re_query.py pushed-n 1000:e0b7` re-derives its `n` as
`byte[0x3692] * 12`.

This is the 32-bit signed-high / unsigned-low compare `Game::luck_below_random_32`
already models (`grep -n 'fn luck_below_random_32' src/game.rs`), recovered at
`1000:dda8`..`1000:ddb1`. **The brief's open question was its two operands, and
they are:** the left is `20ae:38a4`, удача, loaded at `1000:e0c2` and
SIGN-extended by the `1000:e0c5 cwd`; the right is the draw, ZERO-extended
by the `1000:e0bc xor dx,dx`. That asymmetry is why a `jl` sits beside a
`jb`.

Two further things the port needs and the predicate alone does not give:

* **The branches are permuted a THIRD way.** The den's first copy is
  `jl` / `jle` / `jb` (`1000:dda8`, `1000:ddaa`, `1000:ddb1`) and its second is
  `jl` / `jnle` / `jnb` (`1000:ddeb`, `1000:dded`, `1000:ddf1`). The club's is
  `jnle` / `jl` / `jb`.
* **The sense is inverted.** The fall-through of `1000:e0ce jb 0xe129` is
  `1000:e0d0 mov al,[0x3c82]`, the payout. So the club WINS when
  `luck >= draw`, i.e. when `luck_below_random_32` is FALSE.

### Win, lose, caught

```
1000:e0d0  mov al,[0x3c82]
1000:e0d5  shl ax,1
1000:e0d7  add [0x38c7],ax
1000:e0f7  add byte [0x3c82],0x2
1000:e118  mov al,[0x3692]
1000:e11d  add [0x38ce],ax
1000:e121  mov al,0x0
1000:e124  call 0x12526
1000:e127  jmp short 0xe14a
```

`money += stake * 2` after `money -= stake` is a net `+stake`, and the `#` of
`^2Ты выиграл # рублей ` (CS `0xa1ff`, file `0x0bacf`) is the stake
(`1000:e0e0`) — the net, not the credit. Then
`^6Ты получаешь # качков опыта` (CS `0x908b`, file `0x0a95b`) with the district
as its `#` (`1000:e101`), `1000:e11d` adds the same district to xp, and
`1000:e124` calls the level-up with `param_1 = 0`.

The lose path is one line and one store: `^4Ты проиграл # рублей`
(CS `0xa216`, file `0x0bae6`) with the stake, then
`1000:e145 mov byte [0x3c82],0x5` resets it to 5.
The money already left at `1000:e0a8`; there is no second money instruction on
the lose path, and `effects[]` is a set equality over the whole range, so that
is a measurement.

Both paths join at `1000:e14a`:

```
1000:e14a  cmp byte [0x3c82],0x11
1000:e14f  jnb 0xe174
1000:e151  cmp byte [0x3c82],0x5
1000:e156  jbe 0xe174
1000:e174  cmp byte [0x3c82],0x11
1000:e179  jnb 0xe17e
1000:e17b  jmp 0xe256
```

`^6Ставки изменились. Теперь ставка - #` (CS `0xa22d`, file `0x0bafd`) prints
only while `5 < stake < 17`, so after a loss — stake 5 — it is suppressed and
the message never announces a stake of 5.

At 17 the caught-cheating block runs, `1000:e17e`..`1000:e256`:

```
1000:e17e  mov al,0x1
1000:e181  call 0x10d14
1000:e184  mov byte [0x3b72],0x1
1000:e1e9  mov al,[0x3692]
1000:e1ee  mov si,ax
1000:e1f0  shl ax,1
1000:e1f2  shl ax,1
1000:e1f4  add ax,si
1000:e215  add [0x38ce],ax
1000:e219  mov al,0x0
1000:e21c  call 0x12526
1000:e21f  mov al,0x2
1000:e221  push ax
1000:e222  call 0x13d11
1000:e23e  mov byte [0x3b77],0x5
```

In order: roll an opponent with `param_1 = 1` — the clamp-to-class-7 form, so
this fight never draws a Мент; the clamp is at `1000:0da7`/`1000:0dba` and
`docs/re/den.md` records the same reading for `1000:dc0e`, the second of the
three `param_1 = 1` sites — set the fight-accepted flag `20ae:3b72`, print
`^4Козёл! Да ты мухлевал!` (CS `0xa254`, file `0x0bb24`) and the opponent
announcement, print
`^6Ты получаешь # качков опыта за победу в игре` (CS `0xa26d`, file `0x0bb3d`)
with `district * 5`, add that same `district * 5` to xp, level up, **then**
fight, then print
`^6Уноси ноги, пока не отобрали деньги другие канадидаты` (CS `0xa29c`, file
`0x0bb6c`), then set the club ban to 5.

**The XP is awarded before the fight, not after it.** That is invisible on
screen unless the award crosses a level threshold, which is exactly why it is
written down.

The fence above quotes the effects and skips the four print blocks between
them. Nothing is elided from the map: `data/club_arms.json`'s `arms[p].prints`
and `arms[p].strings` carry all ten prints and all twelve literals of the arm,
and both are asserted by SET EQUALITY against a sweep of the range, so a print
this document does not quote still has to be recorded.

### The opponent announcement is one of three identical copies

**Established from flow.** The 66 bytes at `1000:e1a2`..`1000:e1e4` build
`^6Это ` (CS `0x90c0`, file `0x0a990`) + `ranks[class]` + ` # уровня.`
(CS `0x90c7`, file `0x0a997`) with `20ae:395c` as the `#`:

```
1000:e1a8  mov di,0x90c0
1000:e1ad  call 0xf78:0xae7
1000:e1b2  mov di,[0x3952]
1000:e1b6  mov cl,0x8
1000:e1b8  shl di,cl
1000:e1ba  add di,0x2e
1000:e1c0  call 0xf78:0xb66
1000:e1c5  mov di,0x90c7
1000:e1ca  call 0xf78:0xb66
1000:e1cf  push [0x395c]
```

`docs/re/den.md` already records that the same two literals are pushed at
`1000:c3f7` and `1000:e1a8`. This measures it: searching `orig/g.exe` for those
66 bytes finds **exactly three** occurrences — `1000:c3f1`, `1000:dc16` and
`1000:e1a2` — the three `param_1 = 1` opponent-roll sites, and
`tools/test_club_arms.py` also requires each one to be preceded by a near call
that wraps to `1000:0d14`. The port has this construction once already
(`grep -n '1000:dc1c' src/game.rs` finds the den's copy).

### The forced exit — `1000:e243`..`1000:e256`

```
1000:e243  mov di,0x848e
1000:e248  mov di,0x3a72
1000:e24d  mov ax,0xff
1000:e251  call 0xf78:0xb01
1000:e256  jmp short 0xe274
```

`0f78:0b01` is `rtl_str_assign_max`. **Which pointer is the source is settled by
decoding the callee, not by the push order**: `0f78:0b06 lds si,[ss:bx+0xa]`
takes the SOURCE from the first push and `0f78:0b0a les di,[ss:bx+0x6]` the
DESTINATION from the second — the reverse of how `0f78:0ae7`, the routine every
menu row uses, reads its two arguments. So this writes `w` (CS `0x848e`, file
`0x09d5e`) INTO the club's own input buffer `20ae:3a72`.

The consequence is flow, not a screen: at `1000:e274` the buffer holds `w`, so
the `1` compare at `1000:e27e` misses, the `2` compare at `1000:e2f3` misses (or
is skipped by its district gate), and the `w` compare at `1000:e361` hits.
**Being caught cheating ejects the player from the club, at every district,
without the player typing anything.**

That write does not carry an absolute-memory operand — it goes through a pushed
far pointer — so it is outside `effects[]` by construction, the same way the
save loader's block copy is outside the `20ae:394a` census in
`data/gym_arms.json`. It is recorded here so the completeness claim on
`effects[]` cannot hide it.

## The `1` arm — потусоваться на дискотеке, 15 rubles

Span `1000:e274`..`1000:e2e2`; key literal CS `0x8dca`; compare `1000:e27e`;
miss `1000:e283`. Money gate `1000:e285` / `1000:e28a`, refusing with
`^4Не хватает` (CS `0x8e4d`, file `0x0a71d`) — the same literal gym arms `1`
and `2` use.

```
1000:e2a7  sub word [0x38c7],0xf
1000:e2c5  inc [0x38a0]
```

with `^2Ты прокачиваешь ловкость.` (CS `0xa2f1`, file `0x0bbc1`) and
`^1Ловкость +1 ` (CS `0x940d`, file `0x0acdd`) printed around them. The string
says +1 and `1000:e2c5 inc [0x38a0]` is an increment, so string and effect
agree — checked, not assumed. Nothing one-shot is consumed, so the arm repeats.

## The `2` arm — разузнать приемы мухлёжников, 22 rubles

Span `1000:e2e2`..`1000:e357`. Own gate `1000:e2e2` / `1000:e2e7`
(district > 1), whose failure target `1000:e357` is past the key compare. Key
literal CS `0x8e4b`; compare `1000:e2f3`; miss `1000:e2f8`. Money gate
`1000:e2fa` / `1000:e2ff` with the same `^4Не хватает` (CS `0x8e4d`).

```
1000:e31c  sub word [0x38c7],0x16
1000:e33a  inc [0x38a4]
```

with `^2Ты прокачиваешь удачу.` (CS `0xa30d`, file `0x0bbdd`) and
`^1Удача +1 ` (CS `0x942c`, file `0x0acfc`).

`1000:e33a inc [0x38a4]` raises the very byte the `p` arm's compare reads at
`1000:e0c2 mov ax,[0x38a4]`, so the two arms are coupled: 22 rubles at
district 2 or higher buys a permanently better card game.

## The `w` arm

Span `1000:e357`..`1000:e36b`; key literal CS `0x848e`; compare `1000:e361`.
The hit `1000:e366` reaches `1000:e36b`, which jumps to the `trn` setup; the
MISS `1000:e368` is the loop back edge. The arm writes nothing and prints
nothing — both measured over its own span, not omitted.

## The counts this map rests on

Over `1000:df06`..`1000:e390`, all asserted by set equality in
`tools/test_club_arms.py`:

| sweep | count |
|---|---:|
| instructions (aligned) | 539 |
| CS-literal pushes | 32 |
| conditional branches | 20 |
| `Random` call sites | **1** |
| absolute-memory writes | 17 |
| DGROUP addresses touched | 12 |
| DS-pointer pushes | 9 |
| shortstring compares | 5 |

A count is not evidence on its own, so the `Random` sweep also runs over the
whole image and is required to find the population `docs/re/METHODOLOGY.md`
records (86 far calls).

## What the port must change — the club

The full, falsifiable list is `data/club_arms.json`'s
`club.what_the_port_must_change`, thirteen numbered items. In summary:

1. `Game::shop_turn` recognises **no club key**. Three exist, at `1000:e06f`,
   `1000:e27e` and `1000:e2f3`; `w` (`1000:e361`) is already the shared exit.
2. `1` is money >= 15 → `money -= 15`, `agility += 1`. `2` is the same shape at
   22 raising luck, behind a district gate.
3. At district 1 the key `2` is not compared at all, so typing it is silent.
   There is no "wrong district" literal to print.
4. The `p` arm: stake gate, debit, one `Random(district * 12)` draw, win iff
   `!luck_below_random_32(luck, draw)`.
5. Win: `money += stake * 2`, `stake += 2`, `xp += district`, then
   `apply_levels(award = 0)`.
6. Lose: `stake := 5`, nothing else.
7. The stake resets per VISIT (`1000:e020`), not per prompt iteration, and is
   not saved.
8. The "stakes changed" line prints only while `5 < stake < 17`.
9. At 17: roll, flag, print, `xp += district * 5`, level up, **then** fight
   with `param_1 = 2`, then the closing line, then `club_ban_countdown := 5`.
10. The caught block ends the visit by writing `w` into the buffer.
11. `1000:e23e` is the only writer of `20ae:3b77` outside the walk decrement
    and the district reset — but landing it without the gate at `1000:df1a`
    would be worse than landing neither.
12. The menu prints once; the back edge returns to the prompt; an unrecognised
    key is silent.
13. The club's `ReadLn` at `1000:e060` does not trim; `Game::shop_turn` does.

**All thirteen landed in Task 34**, in `src/club.rs` and in
`Game::enter_shop`. Nothing in this range is *blocked*: `20ae:38a0`, `20ae:38a4`, `20ae:38c7`,
`20ae:38ce`, `20ae:3692`, `20ae:3699`, `20ae:3b72` and `20ae:3b77` all have
fields in `src/`, and `1000:0d14`, `1000:2526` and `1000:3d11` are all ported.
The one thing the port lacks is the stake byte itself — missing code, not a
missing input.

---

# Part 2 — the command list, `1000:ea94`..`1000:ec82`

## Range and boundaries

The half-open image range `1000:ea94`..`1000:ec82`, 261 instructions.
`1000:ea94` **is** the `i` verb compare, where `docs/re/gym.md` stops;
`1000:ec82` **is** the `s` verb compare, which `data/command_dispatch.json`
records. `1000:ec78`..`1000:ec82` is `s`'s own setup and is the last span of the
tiling.

## Seventeen lines: one, then seven gated, then nine

**Established from flow.** The handler is pure output: over its 261
instructions it makes **zero** absolute-memory writes, spends **zero** `Random`
draws, and past the verb compare at `1000:ea94` its only call is
`0eed:01c2` (`WriteLn`) — seventeen of them and nothing else, so there is no `ReadLn`,
no prompt and no loop. All of those are sweeps over the range, and the write
sweep is the same regex that finds the club's seventeen writes, so the zero is
not a check that cannot fail.

```
1000:ea94  call 0xf78:0xbd8
1000:ea99  jz 0xea9e
1000:ea9b  jmp 0xec78
1000:ea9e  mov di,0xa710
1000:eab2  call 0xeed:0x1c2
1000:eab7  cmp byte [0x3694],0x1
1000:eabc  jnz 0xead7
```

| # | advertises | CS | file | gate | branch |
|---:|---|---|---|---|---|
| 1 | `w` | `0xa710` | `0x0bfe0` | — | — |
| 2 | `mar` | `0xa762` | `0x0c032` | `20ae:3694` | `1000:eabc` |
| 3 | `bmar` | `0xa787` | `0x0c057` | `20ae:3695` | `1000:eadc` |
| 4 | `rep` | `0xa7ad` | `0x0c07d` | `20ae:3698` | `1000:eafc` |
| 5 | `girl` | `0xa7d6` | `0x0c0a6` | `20ae:3697` | `1000:eb1c` |
| 6 | `pr` | `0xa809` | `0x0c0d9` | `20ae:3696` | `1000:eb3c` |
| 7 | `kl` | `0xa83d` | `0x0c10d` | `20ae:3699` | `1000:eb5c` |
| 8 | `trn` | `0xa860` | `0x0c130` | `20ae:369a` | `1000:eb7c` |
| 9 | `s` | `0xa886` | `0x0c156` | — | — |
| 10 | `sv` | `0xa8c5` | `0x0c195` | — | — |
| 11 | `k` | `0xa8fc` | `0x0c1cc` | — | — |
| 12 | `v` | `0xa940` | `0x0c210` | — | — |
| 13 | `kos` | `0xa96c` | `0x0c23c` | — | — |
| 14 | `h` | `0xa991` | `0x0c261` | — | — |
| 15 | `mh` | `0xa9d1` | `0x0c2a1` | — | — |
| 16 | `name` | `0xa9ff` | `0x0c2cf` | — | — |
| 17 | `e` | `0xaa27` | `0x0c2f7` | — | — |

The gates are `1000:eab7`, `1000:ead7`, `1000:eaf7`, `1000:eb17`, `1000:eb37`,
`1000:eb57` and `1000:eb77`. **Each `jnz` skips exactly its own line**: its
displacement lands on the NEXT gate, and the last one lands on `1000:eb97`, the
start of the ungated tail. `tools/test_club_arms.py` decodes all seven
displacements, so "no gate can hide another line" is arithmetic rather than a
sentence.

The seventeen line texts are in `data/club_arms.json`'s `command_list.lines[]`,
verbatim from `orig/g.exe`; two of the four the port has never printed are
`Напиши: ^6kl^7   чтобы идти в клуб` (CS `0xa83d`) and
`Напиши: ^6trn^7  чтобы идти в качалку` (CS `0xa860`).

## Each gate reads its own verb's discovery byte

**Established from flow on both sides.** For every one of the seven, the
instruction at the list's gate and the instruction at that verb's own handler
gate are the *same compare*:

| list gate | handler gate | byte | location |
|---|---|---|---|
| `1000:eab7` | `1000:b954` | `20ae:3694` | Market |
| `1000:ead7` | `1000:c4c8` | `20ae:3695` | Dealers |
| `1000:eaf7` | `1000:d3b0` | `20ae:3698` | Vet |
| `1000:eb17` | `1000:d6f7` | `20ae:3697` | Girl |
| `1000:eb37` | `1000:d80c` | `20ae:3696` | Den |
| `1000:eb57` | `1000:df10` | `20ae:3699` | Club |
| `1000:eb77` | `1000:e39a` | `20ae:369a` | Gym |

That is what makes the Vet/Den assignment a **corroboration** of
`docs/re/command-dispatch.md`'s discovery-gate table and of the PLACES.SAV read
order at `1000:6ca2`..`1000:6d0e`, rather than a reading of the printed text.

## The gate order is not the flag-address order — do not tidy it

The seven gates run `3694`, `3695`, **`3698`**, `3697`, **`3696`**, `3699`,
`369a`. Vet comes before Girl before Den, where the addresses go Den, Girl, Vet.

`src/locations.rs` records that earlier revisions of this port carried Den and
Vet **swapped** at slots 2 and 4, on the order the command tokens appear in
`data/strings.json`. The `i` list is that same misleading order, in code this
time. Read as flow it confirms the file order; read as an ordering it would
reintroduce the swap. A port that "tidies" this list into address order breaks
`locations::TRACKED`.

## The port divergence — closed by Task 34

At Task 33, when this document was written, `Game::show_command_list` printed
**thirteen** fixed lines with no gating. Recomputed then — the command is in
`data/club_arms.json`'s `command_list.port_divergence.command` — all thirteen
were verbatim original lines and in the original's relative order, but:

* **four are never printed at all**: CS `0xa787` (`bmar`, gate `1000:ead7`),
  `0xa7d6` (`girl`, `1000:eb17`), `0xa83d` (`kl`, `1000:eb57`) and `0xa860`
  (`trn`, `1000:eb77`);
* **three are printed unconditionally that the original gates**: CS `0xa762`
  (`mar`, `1000:eab7`), `0xa7ad` (`rep`, `1000:eaf7`) and `0xa809` (`pr`,
  `1000:eb37`).

The remaining ten — the ungated head and the ungated tail — were right.

**Task 34 rewrote the function to the seventeen-line partition above.**
`data/club_arms.json`'s `command_list.port_divergence.closed_by` carries the
command that recomputes the port half against the shape it has now; it prints
`17 port lines; [] never printed`. `measured` is kept as the record of what
the divergence was, not of what `src/` holds today — a claim about the port
cites the command that recomputes it, and that is what `closed_by` is.

**Where the thirteen came from.**
`docs/re/oracle-captures/command-table-and-combat.md` captures exactly these
thirteen at district 1, and that file's own preamble says in bold that its
screens are EVIDENCE, not specification, and must never stand in for finding the
branch. The thirteen are consistent with `20ae:3694`, `20ae:3698` and
`20ae:3696` set and the other four clear at capture time — 1 + 3 + 9 = 13 — but
that arithmetic is corroboration of the flow reading above, not its source.
**FIVE places carried the thirteen** from that same screen, not four:
`data/command_dispatch.json`'s `i` note, `docs/re/command-dispatch.md`'s `i`
row, two comments in `src/commands.rs` and one in `src/game.rs`. The last of
those writes `13-line list`, not `13-line command list`, so a grep for the
longer phrase misses it — which is how the first draft of this document counted
four. `grep -rn '13-line' src/ data/ docs/` is the command that finds all of
them.

Task 33 corrected the two outside `src/`: the `i` row in
`docs/re/command-dispatch.md` and the `i` note in
`data/command_dispatch.json` now say seventeen and point here. Task 34
corrected the three inside it — `grep -rn '13-line' src/` now prints nothing.

## The counts this map rests on

Over `1000:ea94`..`1000:ec82`:

| sweep | count |
|---|---:|
| instructions (aligned) | 261 |
| CS-literal pushes | 18 |
| conditional branches | 8 |
| `Random` call sites | 0 |
| absolute-memory writes | **0** |
| DGROUP addresses touched | 7 |
| DS-pointer pushes | 1 |
| shortstring compares | 1 |
| `WriteLn` calls | 17 |

Eighteen CS-literal pushes against seventeen lines: the eighteenth is the `s`
verb's own literal at `1000:ec7d`, inside the last span.

## What the port must change — the command list

Four numbered items in `data/club_arms.json`'s
`command_list.what_the_port_must_change`:

1. Print seventeen lines, not thirteen: `1000:ea9e` ungated, then seven gated on
   Market, Dealers, Vet, Girl, Den, Club, Gym **in that order**, then the nine
   from `1000:eb97` ungated. On a character with only Market and Vet found
   that is `1 + 2 + 9` = **twelve** lines. `data/club_arms.json` derives that
   twelve from the decoded partition rather than carrying it as prose — an
   earlier draft said eleven, and nothing read it.
2. Add the four lines the port has never had.
3. Gate the three it prints unconditionally.
4. Stop the thirteen propagating: `grep -rn '13-line' src/` found three
   comments at Task 33's commit — two in `src/commands.rs`, one in
   `src/game.rs` — and must find none after the port lands. **All four items
   landed in Task 34**; the two sites outside `src/` were already corrected.

## Branch coverage

`data/branches.json` holds 20 branches in `1000:df06`..`1000:e38f` and 8 in
`1000:ea94`..`1000:ec81`. Its STORED `port_touched` column marks 19 and 8 of
them untouched; recomputing the metric the file itself documents against today's
tree gives 15 and 8, which is the pair Task 33's brief quotes. Image-wide the
same disagreement is 84 stored against 476 recomputed, so it is structural — the
column is a snapshot — and not a property of this range.
`docs/re/gym.md`'s "Branch coverage, and why two numbers disagree" is the
precedent, and the command that recomputes both is in
`data/club_arms.json`'s `club.branch_census.command`.

Neither number is a to-do list. `port_touched == false` means "no address
citation", never "unimplemented".
