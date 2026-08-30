# The den (`pr`), `1000:d802`..`1000:df06`

The `pr` handler, from its own verb compare to `kl`'s. Machine-readable twin:
`data/den_arms.json`; both are re-derived from `orig/g.exe` by
`tools/test_den_arms.py`, so neither can drift from the binary. Every claim
below states its tier and cites an address, per `docs/re/METHODOLOGY.md`.

Two blocks in range were already ported before this map existed and were
mapped but not re-derived for editing: the district-keyed intro
`1000:d82f`..`1000:d8b9` (`Game::print_den_intro`) and the `a` reveal
`1000:dcba`..`1000:dd32` (`Game::den_reveal`). Everything else in range is
unported at the time of writing.

## Shape

**Established from flow.** One aligned decode from `entry`'s own `1000:ab59`
covers `1000:d802`..`1000:df06` in 825 instructions, and the range tiles
exactly:

| block | what |
|---|---|
| `1000:d802`..`1000:d80c` | the `pr` verb compare, on the STREET buffer `20ae:3972` |
| `1000:d80c`..`1000:d816` | the discovery gate on `20ae:3696` |
| `1000:d816`..`1000:dae2` | seventeen menu lines, printed **once** |
| `1000:dae2`..`1000:db22` | the prompt and the den's own `ReadLn` |
| `1000:db22`..`1000:dee3` | the seven key arms, in order `p r hp s a d w` |
| `1000:dee3`..`1000:defc` | the not-discovered refusal |
| `1000:defc`..`1000:df06` | the `kl` verb's own setup — the right boundary |

The right edge is not a number picked to fit: `1000:defc` is where both den
exits land (`1000:dee1 jmp short 0xdefc` from the `w` arm, and the
not-discovered refusal's own fall-through out of `1000:def7`) and also where
the `pr` verb's own miss goes (`1000:d809 jmp 0xdefc`); `1000:df06` is the
`kl` compare (`call 0f78:0bd8` against `kl`, CS `0xa0ea`).

### It is a loop with a one-shot preamble

**Established from flow.** The loop's only back edge is `1000:dede`, and it
targets `1000:dae2` — the PROMPT push, not the menu:

```
1000:decd  mov di,0x3a72
1000:ded2  mov di,0x848e
1000:ded7  call 0xf78:0xbd8
1000:dedc  jz 0xdee1
1000:dede  jmp 0xdae2
1000:dee1  jmp short 0xdefc
```

So the seventeen menu lines print once on entry and are never reprinted, and
an unrecognised key falls off the end of all seven compares and reaches the
prompt again **in silence** — there is no "unknown command" string anywhere in
the range for it to print. The port must not invent one.

### The input read

**Established from flow.** `1000:dafb`..`1000:db1d`: `20ae:3ecc` (the input
`Text`) and `20ae:3a72` (the destination) are pushed with a maximum length of
`1000:db05 mov ax,0xff`, then `1000:db09 call 0xf78:0x6c6`
(`rtl_text_read_string`), `1000:db0e call 0xf78:0x59d` (`ReadLn`'s
skip-to-end-of-line) and `1000:db13 call 0xf78:0x291` (the `{$I+}` check) —
the names are `data/rtl_names.json`'s. Then `1000:db1d call 0xeed:0x216`.

That last call is why the den's keys are case-insensitive and **not**
whitespace-insensitive. `0eed:0216` walks indices 1..len and, for each byte
between `cmp byte [es:di],0x41` and `cmp byte [es:di],0x5a`, does
`add ax,0x20` and stores it back. It compares against nothing else — in
particular against no `0x20` — so it cannot strip a space.
`Game::shop_turn` trims; `docs/re/gaps.md`'s trimmed-prompt entry already owns
that divergence and the den joins its population.

The prompt itself is `^0Притон\` (CS `0x9eb7`), written with `1000:daf6
call 0xeed:0x0` — `Write`, no newline — which is why the typed line continues
the prompt's own line.

## The three `[0x3695]`/`[0x369a]`/`cmp ax,0x28` blocks are NOT one helper

This is the finding the porting task most needs. All three open with the same
thirteen bytes — `cmp byte [0x3695],0x0` / `jz +7` / `cmp byte [0x369a],0x0` /
`jnz` — so the skip happens only when Dealers **and** Gym are both set. Then
they diverge.

**Established from flow**, by re-slicing the bytes out of `orig/g.exe`:
blocks #1 (`1000:d90f`) and #2 (`1000:da6e`) are **byte-identical**, 52 bytes
each, displacements included. Block #3 (`1000:dcba`) is 43 bytes.

```
1000:d90f  cmp byte [0x3695],0x0
1000:d914  jz 0xd91d
1000:d916  cmp byte [0x369a],0x0
1000:d91b  jnz 0xd95c
1000:d91d  mov al,[0x3692]
1000:d920  xor ah,ah
1000:d922  dec ax
1000:d923  mov dx,0xa
1000:d926  mul dx
1000:d928  mov dx,ax
1000:d92a  mov ax,[0x38a6]
1000:d92d  sub ax,dx
1000:d92f  sub ax,0x5
1000:d932  mov si,ax
1000:d934  shl ax,1
1000:d936  shl ax,1
1000:d938  add ax,si
1000:d93a  add ax,[0x38cb]
1000:d93e  cmp ax,0x28
1000:d941  jl 0xd95c
```

```
1000:dcba  cmp byte [0x3695],0x0
1000:dcbf  jz 0xdcc8
1000:dcc1  cmp byte [0x369a],0x0
1000:dcc6  jnz 0xdd32
1000:dcc8  mov al,[0x3692]
1000:dccb  xor ah,ah
1000:dccd  dec ax
1000:dcce  mov dx,0xa
1000:dcd1  mul dx
1000:dcd3  mov dx,ax
1000:dcd5  mov ax,[0x38a6]
1000:dcd8  sub ax,dx
1000:dcda  shl ax,1
1000:dcdc  add ax,[0x38cb]
1000:dce0  cmp ax,0x28
1000:dce3  jl 0xdd32
```

The nine missing bytes are exactly `1000:d92f sub ax,0x5`, `1000:d932 mov
si,ax`, the second `1000:d936 shl ax,1` and `1000:d938 add ax,si`. So, with
`k = level - (district-1)*10`:

| block | gates | predicate |
|---|---|---|
| #1 `1000:d90f` | menu line 8 | `5k - 25 + cred >= 40`, i.e. `5k + cred >= 65` |
| #2 `1000:da6e` | menu line 15, the `a` row | same |
| #3 `1000:dcba` | the `a` **arm** | `2k + cred >= 40` |

**Neither implies the other**, and both directions are reachable: `k = 1`,
`cred = 38` satisfies #3 and not #2, so the `a` arm fires with no menu line
offering it; `k = 13`, `cred = 0` satisfies #2 and not #3, so the menu offers
`a` and the arm refuses silently. Folding the three into one helper changes
behaviour either way.

`Game::den_reveal` implements #3 and is correct for #3. #1 and #2 are unported.

## The menu, `1000:d816`..`1000:dae2`

**Established from flow.** Seventeen lines. The four district suffixes and the
prefix are ported; the rest are not.

| # | gate | prints |
|---|---|---|
| 0 | — | `Ты пришел в притон - ` (CS `0x9cf0`), `1000:d82a call 0xeed:0x0` — no newline |
| 1 | `1000:d82f cmp byte [0x3692],0x1` | `^0общагу №#` (CS `0x9d06`), with `Random(6)` at `1000:d83f` and `1000:d844 add ax,0x3` |
| 2 | `1000:d859 cmp byte [0x3692],0x2` | `^0общагу ВКИ` (CS `0x9d12`) |
| 3 | `1000:d879 cmp byte [0x3692],0x3` | `^0гоповский притон` (CS `0x9d1f`) |
| 4 | `1000:d899 cmp byte [0x3692],0x4` | `^0притон отморозков` (CS `0x9d32`) |
| 5 | — | a bare `WriteLn` on the output `Text` at `20ae:3fcc` (`1000:d8be`, `1000:d8c3`) — a blank line, no literal |
| 6 | `1000:d8c8 cmp byte [0x3b78],0x1` | `^6На одного пацана наехал какой-то урод` (CS `0x9d46`) |
| 7 | `1000:d8e8 cmp byte [0x3b79],0x0` and `1000:d8ef cmp word [0x38cb],0x64` | `^6Ты пацан нормальный. Есть дело.` (CS `0x9d6e`) |
| 8 | threshold block #1 | `^6Пацаны хотят тебе кое-чё сказать` (CS `0x9d90`) |
| 9 | — | a second blank line (`1000:d961`, `1000:d966`) |
| 10 | — | `Напиши ^6w^7 чтобы уйти` (CS `0x9db3`) |
| 11 | — (colour only) | `Напиши ^` (CS `0x9dcb`) + colour + `p^7  чтобы угостить пацанов пивом` (CS `0x9dd4`) |
| 12 | `1000:d9ec cmp byte [0x3e35],0x0` | `Напиши ^` (CS `0x9dcb`) + colour + `r^7  чтобы занять 2 рубля` (CS `0x9df6`) |
| 13 | `1000:da35 cmp byte [0x3b78],0x1` | `Напиши ^6hp^7 чтобы отпинать мудака который наезжал на пацана` (CS `0x9e10`) |
| 14 | — | `Напиши ^6s^7  чтобы узнать отношение` (CS `0x9e4e`) |
| 15 | threshold block #2 | `Напиши ^6a^7  чтобы спросить чё-то` (CS `0x9e73`) |
| 16 | `1000:dabb cmp word [0x38cb],0x64` and `1000:dac2 cmp byte [0x3b79],0x0` | `Напиши ^6d^7 чтобы пойти на дело` (CS `0x9e96`) |

### The two dimmed rows

**Established from flow.** Rows 11 and 12 use the same three-call idiom
`docs/re/tables.md` records for the shop menu rows, and the same colour byte
`20ae:3b7a`:

```
1000:d984  cmp word [0x38c3],0x0
1000:d989  jnz 0xd992
1000:d98b  mov byte [0x3b7a],0x34
1000:d990  jmp short 0xd997
1000:d992  mov byte [0x3b7a],0x30
1000:d997  lea di,[bp-0x200]
1000:d99d  mov di,0x9dcb
1000:d9a2  call 0xf78:0xae7
1000:d9a7  lea di,[bp-0x100]
1000:d9ad  mov al,[0x3b7a]
1000:d9b0  push ax
1000:d9b1  call 0xf78:0xc03
1000:d9b6  call 0xf78:0xb66
1000:d9bb  mov di,0x9dd4
1000:d9c0  call 0xf78:0xb66
1000:d9d4  call 0xeed:0x1c2
```

`0x34` is ASCII `4` and `0x30` is ASCII `0`, so no beer dims the `p` row and
any beer leaves it normal. The `WriteLn` at `1000:d9d4` has no visible string
push because `0f78:0ae7` and `0f78:0b66` return with `retf 0x4` and
`0f78:0c03` with `retf 0x2`: each pops only its SOURCE and leaves the
destination far pointer on the stack, and `0eed:01c2` returns with `retf 0xe`
— one far pointer plus five format words. That is what consumes it.

Row 12 does the same with `1000:d9d9 cmp word [0x38cb],0x2`. **Order matters**:
that colour store runs BEFORE the `1000:d9ec` visibility gate, so `20ae:3b7a`
is written even on a turn where the row is not printed.

## The arms

**A note on every fence in this document.** They are the aligned decode with
exactly two rigidly repeated shapes dropped, and nothing else: the
`push cs` / `push di` (or `push ds` / `push di`, or `push ss` / `push di`)
that follows every `mov di,imm16` or `lea di,[bp-N]`, and the five
`xor ax,ax` / `push ax` pairs that precede every `call 0eed:0x1c2` — the
`WriteLn` format-spec words. A `push ax` that pushes a VALUE is kept, because
it is the argument. The one exception is the `param_1` chain under "The
`param_1` dispatch behind item 6", which is a skeleton of five links and says
so there. `tools/test_den_arms.py` re-decodes every line of every fence, and
sweeps the COMPLETE decode besides, so an instruction dropped from a fence
would still show up as an unaccounted branch, string, draw or write.

### `p` — `1000:db22`, treat the lads to beer

**Established from flow.**

```
1000:db22  mov di,0x3a72
1000:db27  mov di,0x9ec1
1000:db2c  call 0xf78:0xbd8
1000:db31  jnz 0xdb77
1000:db33  cmp word [0x38c3],0x0
1000:db38  jle 0xdb5e
1000:db3a  dec [0x38c3]
1000:db3e  add word [0x38cb],0x5
1000:db43  mov di,0x9ec3
1000:db57  call 0xeed:0x1c2
1000:db5c  jmp short 0xdb77
1000:db5e  mov di,0x9efb
```

One gate, `[0x38c3] > 0` (signed `jle`, so zero and negative both refuse).
Two effects: `1000:db3a` spends one half-litre of `пиво` and `1000:db3e` adds
5 to понтовость на улице, `20ae:38cb`. Prints
`^2Ты угостил пацанов пивом. Понтовость улутшилась на 5.` (CS `0x9ec3`) on
success and `^6А нет у тебя пива.` (CS `0x9efb`) on refusal. The confirmation
says 5 and the add is 5 — checked against the bytes, not assumed. Repeatable:
nothing one-shot is consumed.

### `r` — `1000:db77`, borrow two roubles

**Established from flow.**

```
1000:db77  mov di,0x3a72
1000:db7c  mov di,0x9a50
1000:db81  call 0xf78:0xbd8
1000:db86  jnz 0xdbf3
1000:db88  cmp byte [0x3e35],0x0
1000:db8d  jbe 0xdbda
1000:db8f  cmp word [0x38cb],0x0
1000:db94  jle 0xdbbf
1000:db96  add word [0x38c7],0x2
1000:db9b  sub word [0x38cb],0x2
1000:dba0  dec [0x3e35]
1000:dba4  mov di,0x9f10
1000:dbb8  call 0xeed:0x1c2
1000:dbbd  jmp short 0xdbd8
1000:dbbf  mov di,0x9f49
1000:dbd3  call 0xeed:0x1c2
1000:dbd8  jmp short 0xdbf3
1000:dbda  mov di,0x9f66
```

Two gates with **two distinct, non-interchangeable refusals**, checked in this
order: credit first (`1000:db8d`, an unsigned `jbe` on a byte compared with 0,
so it refuses exactly `[0x3e35] == 0`) prints
`^6Ты уже всю мелочь выгреб!` (CS `0x9f66`); понтовость second
(`1000:db94`, signed) prints `^6Ты не можешь занять денег.` (CS `0x9f49`).
Success prints
`^2Ты занял 2 рубля на пиво. Понтовость уменьшилась на 2.` (CS `0x9f10`).

Three effects. `20ae:3e35` is the one-shot resource: it starts at 5
(`1000:73e5`) and is topped up once per walk while below `district * 10`
(`1000:af19`), both of which the port already models.

### `hp` — `1000:dbf3`, beat up the lout

**Established from flow.** `src/commands.rs` names `hp` as living inside
`pr`'s own submenu on the strength of the menu literal; that is a **string**
observation. It is CONFIRMED here at a compare: `1000:dc04 call 0xf78:0xbd8`
against `hp` (CS `0x9f82`), on the den's own buffer `20ae:3a72`.

```
1000:dbf3  cmp byte [0x3b78],0x1
1000:dbf8  jnz 0xdc63
1000:dbfa  mov di,0x3a72
1000:dbff  mov di,0x9f82
1000:dc04  call 0xf78:0xbd8
1000:dc09  jnz 0xdc63
1000:dc0b  mov al,0x1
1000:dc0d  push ax
1000:dc0e  call 0x10d14
1000:dc11  mov byte [0x3b72],0x1
1000:dc16  lea di,[bp-0x100]
1000:dc1c  mov di,0x90c0
1000:dc21  call 0xf78:0xae7
1000:dc26  mov di,[0x3952]
1000:dc2a  mov cl,0x8
1000:dc2c  shl di,cl
1000:dc2e  add di,0x2e
1000:dc34  call 0xf78:0xb66
1000:dc39  mov di,0x90c7
1000:dc3e  call 0xf78:0xb66
1000:dc43  push [0x395c]
1000:dc53  call 0xeed:0x1c2
1000:dc58  mov al,0x6
1000:dc5a  push ax
1000:dc5b  call 0x13d11
1000:dc5e  mov byte [0x3b78],0x0
```

This is the **only** arm whose gate stands in front of its own key compare
rather than behind it: with no errand pending, `hp` is never compared at all
and the line falls straight through to the `s` compare.

The two near calls wrap: `1000:dc0e`'s `rel16` sums to image `0x10d14`, which
is `1000:0d14` modulo 64 KiB — `FUN_1000_0d14`, the opponent roll, with
`param_1 = 1`, the clamp-to-class-7 form (so this errand never draws a Мент).
`1000:dc5b` reaches `1000:3d11`, the fight, with `param_1 = 6`.

The announcement is `^6Это ` (CS `0x90c0`) + the rank name + ` # уровня.`
(CS `0x90c7`), with `20ae:395c`, the rolled level, as the `#`. `1000:dc26`..
`1000:dc2e` computes `[0x3952] * 0x100 + 0x2e`, which is exactly the `ranks`
table `data/string_tables.json` records — DGROUP `0x2e`, stride 256. The same
two literals are pushed at `1000:c3f7` and `1000:e1a8`, the other two
`param_1 = 1` sites, so they are not den-private.

Two effects: `1000:dc11` sets the fight-accepted flag `20ae:3b72` and
`1000:dc5e` consumes the errand.

### `s` — `1000:dc63`, ask how the lads regard you

**Established from flow.**

```
1000:dc63  mov di,0x3a72
1000:dc68  mov di,0x9f85
1000:dc6d  call 0xf78:0xbd8
1000:dc72  jnz 0xdcba
1000:dc74  mov di,0x9f87
1000:dc79  push [0x38cb]
1000:dc89  call 0xeed:0x1c2
1000:dc8e  mov al,[0x3692]
1000:dc91  xor ah,ah
1000:dc93  mov dx,0xa
1000:dc96  mul dx
1000:dc98  add ax,0xa
1000:dc9b  cmp ax,[0x38cb]
1000:dc9f  jnle 0xdcba
1000:dca1  mov di,0x9fa5
```

Always prints `^4Твоя понтовость сейчас = #.` (CS `0x9f87`) with `20ae:38cb`
substituted; adds `^0Да если чё мы за тебя впрягаемся.` (CS `0x9fa5`) when
`district*10 + 10 <= [0x38cb]`.

Note that this arithmetic does **not** `dec ax` first, unlike all three
threshold blocks: it is `district*10 + 10`, not `(district-1)*10`.

**This arm writes nothing.** That is a measurement, not an omission: the
absolute-write sweep over `1000:dc63`..`1000:dcba` finds zero stores.
`1000:dc79 push [0x38cb]` is a read — the `#` argument.

### `a` — `1000:dcba`, the Dealers+Gym reveal

**Already ported** as `Game::den_reveal`, whose own doc comment carries the
re-derivation. Mapped here only so threshold block #3 can be compared with #1
and #2. Its gate is block #3 in full; its key compare is `1000:dcef` against
`a` (CS `0x9fc9`); its effects are `1000:dcf6` and `1000:dcfb` (both discovery
flags, unconditionally once reached); it prints
`^0Тут у нас есть пара мест куда тебе стоит сходить` (CS `0x9fcb`) and
`^2Ты узнал где находится качалка и где находятся барыги` (CS `0x9ffe`).

### `d` — `1000:dd32`, go on the job

**Established from flow.** The largest arm, and the only one with wide
compares or draws.

```
1000:dd32  mov di,0x3a72
1000:dd37  mov di,0xa036
1000:dd3c  call 0xf78:0xbd8
1000:dd41  jz 0xdd46
1000:dd43  jmp 0xdecd
1000:dd46  cmp word [0x38cb],0x64
1000:dd4b  jnl 0xdd50
1000:dd4d  jmp 0xdecd
1000:dd50  cmp byte [0x3b79],0x0
1000:dd55  jnz 0xdd5a
1000:dd57  jmp 0xdecd
1000:dd5a  mov di,0xa038
1000:dd6e  call 0xeed:0x1c2
1000:dd73  mov di,0xa04a
```

Two gates, both silent on failure: понтовость at least 100, and errand two
pending. Then it prints `^0Давай быстрее..` (CS `0xa038`) and
`^2Ты пришел воровать деньги` (CS `0xa04a`) and rolls.

#### The two wide compares

```
1000:dd8c  mov al,[0x3692]
1000:dd8f  xor ah,ah
1000:dd91  mov dx,0xf
1000:dd94  mul dx
1000:dd96  push ax
1000:dd97  call 0xf78:0x114b
1000:dd9c  xor dx,dx
1000:dd9e  mov cx,ax
1000:dda0  mov bx,dx
1000:dda2  mov ax,[0x38a4]
1000:dda5  cwd
1000:dda6  cmp dx,bx
1000:dda8  jl 0xddb6
1000:ddaa  jle 0xddaf
1000:ddac  jmp 0xde36
1000:ddaf  cmp ax,cx
1000:ddb1  jb 0xddb6
1000:ddb3  jmp 0xde36
```

**The `JL` beside the `JB` is not a slip.** It is Borland's canonical 32-bit
compare: the high halves are compared SIGNED and the low halves UNSIGNED, and
the 32-bit width comes from promoting `Random`'s `Word` result against the
`Integer` at `20ae:38a4` (Удача). `1000:dd9c xor dx,dx` zero-extends the
random into `bx:cx`; `1000:dda5 cwd` sign-extends luck into `dx:ax`. The whole
predicate is `Longint([0x38a4]) < Longint(Random(district*15))`: true reaches
`1000:ddb6`, false reaches `1000:de36`.

`docs/re/wander.md`'s already-ported `1000:b5f1`..`1000:b61b` is the same
idiom with the three branches permuted, and `Game::walk`'s comment there
already records that this port widens both sides by zero-extension.

The second compare has the same predicate and the branches permuted again:

```
1000:ddcf  mov al,[0x3692]
1000:ddd2  xor ah,ah
1000:ddd4  mov dx,0xf
1000:ddd7  mul dx
1000:ddd9  push ax
1000:ddda  call 0xf78:0x114b
1000:dddf  xor dx,dx
1000:dde1  mov cx,ax
1000:dde3  mov bx,dx
1000:dde5  mov ax,[0x38a4]
1000:dde8  cwd
1000:dde9  cmp dx,bx
1000:ddeb  jl 0xddf3
1000:dded  jnle 0xde1a
1000:ddef  cmp ax,cx
1000:ddf1  jnb 0xde1a
```

`1000:ddf3` is the luck-lost arm, `1000:de1a` the luck-won one.

#### The three outcomes

```
1000:ddf3  mov al,0x2
1000:ddf5  push ax
1000:ddf6  call 0x10d14
1000:ddf9  mov al,0x5
1000:ddfb  push ax
1000:ddfc  call 0x13d11
1000:ddff  mov di,0xa075
1000:de13  call 0xeed:0x1c2
1000:de18  jmp short 0xde33
1000:de1a  mov di,0xa084
1000:de2e  call 0xeed:0x1c2
1000:de33  jmp 0xdec8
```

- **Luck lost twice** — `1000:ddb6` prints `^4Шухер менты!` (CS `0xa066`),
  then `1000:ddf6` rolls an opponent with `param_1 = 2`, which
  `FUN_1000_0d14` FORCES to class 8, the `Мент` of
  `data/string_tables.json`'s `ranks`. `1000:ddfc` fights it with
  `param_1 = 5`, and `^6Пора валить!` (CS `0xa075`) prints after.
- **Luck lost then won** — `1000:de1a` prints
  `^2Ты смылся от ментов.` (CS `0xa084`) and nothing else happens.
- **Luck won first time** — `1000:de36`, the haul:

```
1000:de36  mov di,0xa09b
1000:de4a  call 0xeed:0x1c2
1000:de4f  mov al,[0x3692]
1000:de52  xor ah,ah
1000:de54  mov dx,0xa
1000:de57  mul dx
1000:de59  push ax
1000:de5a  call 0xf78:0x114b
1000:de5f  mov cx,ax
1000:de61  mov al,[0x3692]
1000:de64  xor ah,ah
1000:de66  mov dx,0xa
1000:de69  mul dx
1000:de6b  add ax,cx
1000:de6d  add [0x38c7],ax
1000:de71  mov al,[0x3692]
1000:de74  xor ah,ah
1000:de76  mov dx,0xa
1000:de79  mul dx
1000:de7b  push ax
1000:de7c  call 0xf78:0x114b
1000:de81  mov cx,ax
1000:de83  mov al,[0x3692]
1000:de86  xor ah,ah
1000:de88  mov dx,0xa
1000:de8b  mul dx
1000:de8d  add ax,cx
1000:de8f  add [0x38c9],ax
1000:de93  mov di,0x908b
1000:de98  mov al,[0x3692]
1000:de9b  xor ah,ah
1000:de9d  mov dx,0xc
1000:dea0  mul dx
1000:dea2  push ax
1000:deaf  call 0xeed:0x1c2
1000:deb4  mov al,[0x3692]
1000:deb7  xor ah,ah
1000:deb9  mov dx,0xc
1000:debc  mul dx
1000:debe  add [0x38ce],ax
1000:dec2  mov al,0x0
1000:dec4  push ax
1000:dec5  call 0x12526
1000:dec8  mov byte [0x3b79],0x0
```

`^2Ты наваровал денег` (CS `0xa09b`), then money and хлам (`20ae:38c9`) each gain
`district*10 + Random(district*10)`, then
`^6Ты получаешь # качков опыта` (CS `0x908b`) prints `district*12` and
`1000:debe` credits it, and `1000:dec5` reaches `1000:2526` — the level-up
drain — with `param_1 = 0`, the CAPPED form (`1000:257a`), the same one the
ordinary combat path passes at `1000:5238`. The award is the `add`, not an
argument.

`1000:dec8` consumes errand two on **every** path that got past `1000:dd55`,
including both cop outcomes.

**Draw count per invocation**, established from flow: 3 on the haul path
(`1000:dd97`, `1000:de5a`, `1000:de7c`), 2 in this range on either cop path
(`1000:dd97`, `1000:ddda`) plus whatever `1000:0d14` and `1000:3d11` draw.

### `w` — `1000:decd`, leave

**Established from flow.** `w` (CS `0x848e`) compared at `1000:ded7`; on a
match `1000:dee1` jumps out. It prints nothing and writes nothing — measured,
not asserted: the write sweep and the literal sweep over
`1000:decd`..`1000:dee3` find no store and no push but its own key literal.
CS `0x848e` is shared by nine push sites image-wide, so it is every location's
exit key and not the den's own.

## The not-discovered path

**Established from flow.** `1000:d80c cmp byte [0x3696],0x1`; a miss jumps to
`1000:dee3`, prints
`^4Тебя мудака такого туда не пустят - поднимай понтовость` (CS `0xa0b0`) and
falls into `1000:defc`. No `ReadLn` happens and the submenu is never entered.

## What the port must change

1. **Six more keys.** `Game::shop_turn` recognises exactly one den key, `a`.
   Established at compare addresses: `p` (`1000:db2c`), `r` (`1000:db81`),
   `hp` (`1000:dc04`), `s` (`1000:dc6d`), `d` (`1000:dd3c`), `w`
   (`1000:ded7`).
2. **Twelve more menu lines.** `1000:d8b9` onward is unported;
   `Game::print_den_intro`'s own doc already says so. Four of them are gated
   and two are colour-dimmed.
3. **Do not factor the three threshold blocks together.** See above. #1 and #2
   are one predicate, #3 is another, and `Game::den_reveal` already has #3
   right.
4. **`p` and `r` port directly** — plain state edits with no call out.
5. **`s` ports directly** and writes nothing.
6. **`hp` and the `d` arm's cop branch are BLOCKED.** Both call `1000:3d11`
   with a `param_1` the port does not model — 6 and 5. `Game::run_combat`
   models no `param_1` at all and `docs/re/gaps.md` already records that as
   open. `param_1 = 6` is a value the fight function really does treat
   specially; `param_1 = 5` is not, but it still reaches a different arm than
   the port's own `0`. The rest of `d` — both luck compares, the haul, the
   xp, `1000:2526` — has no such obstacle.
7. **The den does not trim its input.** See "The input read" above.
8. **An unrecognised key is silent.** `Game::shop_turn` already behaves this
   way; recorded so the porting task does not add a refusal line the original
   has no string for.

### Nothing here is unreachable for want of a setter

**Established from flow for the original addresses; the port half cites the
command that recomputes it.** Every global a den gate reads has a writer in
`src/`: `20ae:3696` (`grep -n 'mark_found(Location::Den)' src/game.rs`),
`20ae:3b78` and `20ae:3b79` (`grep -n 'den_errand_._pending = true'
src/game.rs`), `20ae:38c3` (`grep -n 'beer_dl += 1' src/game.rs`),
`20ae:3e35` (`grep -n 'den_loan_credit' src/game.rs`), `20ae:38cb`
(`grep -n 'pontovost_street +=' src/game.rs`), `20ae:38a4`
(`grep -n 'luck' src/model.rs`), `20ae:3695` and `20ae:369a`
(`grep -n 'mark_found(Location::' src/game.rs`). What blocks the port is item
6, not reachability.

### The `param_1` dispatch behind item 6

**Established from flow**, by TWO sweeps over one aligned decode from
`FUN_1000_3d11`'s own entry `1000:3d11` across the 6971 bytes
`data/functions.json` records for it — 3043 instructions.

The first sweep collects every instruction whose ModRM addresses `[bp+0x4]`.
There are exactly eight: `1000:3d24 mov al,[bp+0x4]`, `1000:5085`,
`1000:5139`, `1000:51a6`, `1000:51ac`, `1000:51f6`, `1000:51fc` and
`1000:57ce`, with immediates 4, 3, 3, 4, 3, 4 and 6.

**That sweep alone is not enough, and a first draft of this section was wrong
to stop there.** `1000:3d24` copies the parameter into `al`, and the actual
dispatch is a chain of REGISTER compares no `[bp+0x4]` scan can see. So the
second sweep collects every `cmp al,imm8` in the body. There are exactly
five, and all five are that chain. The listing below is a SKELETON, not a
contiguous run — it shows the five links and the load that feeds them, and
skips the arm each link jumps into:

```
1000:3d24  mov al,[bp+0x4]
1000:3d27  cmp al,0x0
1000:3d29  jz 0x3d32
1000:3d2b  cmp al,0x6
1000:3d2d  jz 0x3d32
1000:3d2f  jmp 0x3e8d
1000:3e8d  cmp al,0x1
1000:3e8f  jnz 0x3ead
1000:3ead  cmp al,0x3
1000:3eaf  jnz 0x3f2b
1000:3f2b  cmp al,0x4
1000:3f2d  jnz 0x3fa7
```

Nothing runs between one link's miss and the next link's compare — each miss
is a fall-through, the single `1000:3d2f jmp 0x3e8d` sitting at a
fall-through, or a `jnz`'s own target — which is what licenses reading these
register compares as parameter tests.

So `FUN_1000_3d11` distinguishes exactly **five** values: 0 and 6 reach
`1000:3d32`, 1 reaches `1000:3e91`, 3 reaches `1000:3eb1`, 4 reaches
`1000:3f2f`, and **everything else, `param_1 = 5` included, reaches
`1000:3fa7`** — which is `mov ax,[0x3956]`, the fight body, not another
compare.

`param_1 = 6` then has one further block of its own:
`1000:57ce cmp byte [bp+0x4],0x6` guards `1000:57de add [0x38cb],ax` with
`ax = district*20` (`1000:57d4`..`1000:57dc`) — the errand's own понтовость
reward, which lives inside the fight function and not in the den.

The instruction count is recorded because these are NEGATIVE claims: without
it, an empty hit list could mean an empty search.

## What this map could not establish

- **What a `param_1` of 5 actually costs relative to 0.** The two sweeps
  prove 5 is undifferentiated among the values `FUN_1000_3d11` tests and
  reaches the default arm `1000:3fa7`; they do NOT prove the two calls behave
  identically, because 0 takes `1000:3d32` instead and that arm was not
  decoded. Settling it means reading `1000:3d32`..`1000:3fa7`, which is a
  combat-dispatch task and outside this range.
- **Whether `1000:0d14`, `1000:3d11` and `1000:2526` draw as the port's
  `roll_enemy`, `run_combat` and `apply_levels` do** on these argument values.
  Their bodies were not decoded here; only the five call sites and their
  pushed arguments were.

## A wrong citation found on the way

`docs/re/save-format.md`'s xp row cites `1000:2536` for `FUN_1000_2526`'s
first read of `20ae:38ce`. That address is one byte into
`1000:2535 mov ax,[0x38ce]` (`a1 ce 38`) and is not an instruction boundary;
the read is at `1000:2535`. Recorded in `data/den_arms.json`'s
`known_not_boundaries`, where `tools/test_den_arms.py` asserts it really is
not a boundary, and pointed at from `docs/re/gaps.md`. Not corrected in
`save-format.md` here — that file is outside this task's range.
