# The vet (`rep`) — Task 34

Machine-readable twin: `data/vet_arms.json`. Both are re-derived from
`orig/g.exe` by `tools/test_arms_artifacts.py`, the generic verifier that also
covers `data/club_arms.json` and `data/gym_arms.json`; neither reads `src/`, a
screen, or Ghidra's decompiled C. `docs/re/club.md` and `docs/re/gym.md` are
the shape this follows.

**Unlike them, this map was written by the task that consumed it.** Task 34
mapped the vet and ported it in one pass, so this document is deliberately
shorter than its two predecessors: it carries the findings and the addresses a
reader needs to check `src/vet.rs` against the binary, not a second
serialisation of every byte `data/vet_arms.json` already holds. The counts
below are all set-equality sweeps and every one of them is asserted.

Every claim states its tier per `docs/re/METHODOLOGY.md`. All of them are
**established from flow** — an aligned decode of `1000:d3a6`..`1000:d6ed`
walked forward from `entry` (`1000:ab59`), obtained with
`python3 tools/re_query.py resolve 1000:d3a6 -n 840 -i 460`.

## Address convention

`1000:xxxx` is a Ghidra label (Form A of `docs/re/METHODOLOGY.md`) and, because
the game's code segment is the load base, its OFF is also the image offset.
`20ae:xxxx` is DGROUP. `0eed:` and `0f78:` are Form B runtime segments. A "CS
offset" is the image offset of a Pascal shortstring's length byte — the exact
immediate of the `mov di,imm16` that pushes it — and the file offset is that
plus the `0x18d0`-byte MZ header.

## Range and boundaries

The half-open image range `1000:d3a6`..`1000:d6ed`, 379 instructions.

* `1000:d3a6` **is** the `rep` verb compare. Its buffer push is
  `1000:d39c mov di,0x3972` and its token push `1000:d3a1 mov di,0x9966`
  (`rep`, CS `0x9966`, file `0x0b236`) — so the verb is read out of the STREET
  buffer, not the vet's own `20ae:3a72`.
* `1000:d6ed` **is** the `girl` verb compare and is *not* decoded here.

`data/command_dispatch.json` is the independent authority on both.

## The whole shape, in order

| span | what |
|---|---|
| `1000:d3a6`..`1000:d3b0` | `rep` verb compare |
| `1000:d3b0`..`1000:d3ba` | discovery gate on `20ae:3698` |
| `1000:d3ba`..`1000:d3d3` | the intro line |
| `1000:d3d3`..`1000:d3f7` | the healthy test that decides whether the menu prints |
| `1000:d3f7`..`1000:d410` | the doctor's menu header |
| `1000:d410`..`1000:d465` | menu row 1 — `h`, 3 rubles |
| `1000:d465`..`1000:d4ba` | menu row 2 — `r`, 7 rubles |
| `1000:d4ba`..`1000:d4ed` | **the loop top**: the same health test, and the eject |
| `1000:d4ed`..`1000:d532` | prompt, `ReadLn`, case fold |
| `1000:d532`..`1000:d5af` | the `r` arm |
| `1000:d5af`..`1000:d6a3` | the `h` arm |
| `1000:d6a3`..`1000:d6b4` | the `w` exit compare |
| `1000:d6b4`..`1000:d6c8` | the `e` exit compare and the loop back edge |
| `1000:d6c8`..`1000:d6ca` | the shared exit jump |
| `1000:d6ca`..`1000:d6e3` | the undiscovered refusal |
| `1000:d6e3`..`1000:d6ed` | the `girl` verb's own setup |

Those sixteen spans tile the range end to end with no gap and no overlap;
`tools/test_arms_artifacts.py` asserts the tiling, so a block cannot be dropped
from this map by being left out of every span.

## The headline: `h` is not a jaw and `r` is not one leg

**Established from flow.** Before this task the port carried

```rust
fn heal_jaw(&mut self) { self.pay_and_heal(3, self.player.broken_jaw,  ..) }
fn heal_leg(&mut self) { self.pay_and_heal(7, self.player.broken_leg,  ..) }
```

with a shared `pay_and_heal` that refused a healthy player with
`^0Док: вали отсюда ты здоров.` **Every one of those five claims is wrong**,
and the two prices are the only thing that survives:

The `r` arm, whose key compare is `1000:d537`:

```
1000:d537  call 0xf78:0xbd8
1000:d53c  jnz 0xd5af
1000:d53e  cmp byte [0x38b0],0x1   ; jaw set -> the money test
1000:d543  jz 0xd54c
1000:d545  cmp byte [0x38b1],0x1   ; else leg set, else SILENT
1000:d54a  jnz 0xd5af
1000:d54c  cmp word [0x38c7],0x7
1000:d551  jl 0xd596
1000:d553  sub word [0x38c7],0x7
1000:d558  mov byte [0x38b0],0x0   ; BOTH breaks
1000:d55d  mov byte [0x38b1],0x0
```

The `h` arm, whose key compare is `1000:d5b9`:

```
1000:d5b9  call 0xf78:0xbd8
1000:d5be  jz 0xd5c3
1000:d5c0  jmp 0xd6a3
1000:d5c3  mov ax,[0x38ac]
1000:d5c6  cmp ax,[0x38ae]
1000:d5ca  jl 0xd5cf
1000:d5cc  jmp 0xd6a3              ; hp >= hpmax: SILENT
1000:d5cf  cmp word [0x38c7],0x3
1000:d5d4  jnl 0xd5d9
1000:d5d9  sub word [0x38c7],0x3
1000:d5de  add word [0x38ac],0x5   ; +5 HEALTH
1000:d5e3  mov ax,[0x38ac]
1000:d5e6  cmp ax,[0x38ae]
1000:d5ea  jle 0xd5f2
1000:d5ec  mov ax,[0x38ae]
1000:d5ef  mov [0x38ac],ax         ; clamped to hpmax
```

* **`r` clears both breaks for one price.** `1000:d543` takes the jaw case
  straight past the leg test, so the gate is an OR, and `1000:d558` and
  `1000:d55d` both store zero behind the single `cmp` at `1000:d54c`.
* **`h` touches neither break byte.** `20ae:38b0` has five image-wide writers
  and `20ae:38b1` four (`python3 tools/re_query.py xrefs-to 20ae:38b0`, and the
  same for `38b1`); the vet's are `1000:d558` and `1000:d55d`, both in the `r`
  arm. Nothing in `1000:d5af`..`1000:d6a3` names either address.
* **Both arms fail silently.** `1000:d54a` and `1000:d5cc` jump to the next
  compare with nothing printed. The `^0Док: вали отсюда ты здоров.` literal
  (CS `0x9a25`) has exactly one push site in the range, `1000:d4d1`, and it is
  in the loop top.

The two menu rows agree — file `0x0b2b2` is `3^7 рубля тебя залатают` ("they
patch you up") and file `0x0b2d9` is `7^7 рублей починят переломы` ("they fix
your fractures") — but those are OUTPUT and could only have corroborated the
reading. The `add` at `1000:d5de` established it.

## The loop top is a health test, and it ejects

**Established from flow.** `1000:d4ba` is the join of two edges — the
healthy-skip `1000:d3f4 jmp 0xd4ba`, which jumps PAST the two menu rows, and
the back edge `1000:d6c5 jmp 0xd4ba` — so it runs on entry and before every
prompt:

```
1000:d4ba  mov ax,[0x38ac]
1000:d4bd  cmp ax,[0x38ae]
1000:d4c1  jl 0xd4ed
1000:d4c3  cmp byte [0x38b0],0x0
1000:d4c8  jnz 0xd4ed
1000:d4ca  cmp byte [0x38b1],0x0
1000:d4cf  jnz 0xd4ed
1000:d4d1  mov di,0x9a25            ; the eject line, printed by 1000:d4e5
1000:d4ea  jmp 0xd6c8               ; and out
```

All three branches go to the PROMPT at `1000:d4ed` and the **eject is the
fall-through**. So the player is thrown out the moment they are whole, a
successful `h` or `r` can end the visit by itself, and entering the vet
undamaged prints the intro and the eject line and nothing else. Inverting the
sense would eject exactly the players who came for treatment.

**The same predicate is spelled twice, oppositely, and the two spellings decide
different things.** `1000:d3d3`..`1000:d3f2` computes it into `al` —

```
1000:d3d3  mov ax,[0x38ac]
1000:d3d6  cmp ax,[0x38ae]
1000:d3da  jl 0xd3ea
1000:d3dc  cmp byte [0x38b0],0x0
1000:d3e1  jnz 0xd3ea
1000:d3e3  cmp byte [0x38b1],0x0
1000:d3e8  jz 0xd3ee
1000:d3ea  mov al,0x0
1000:d3ec  jmp short 0xd3f0
1000:d3ee  mov al,0x1
1000:d3f0  or al,al
1000:d3f2  jz 0xd3f7                ; not whole -> print the menu
1000:d3f4  jmp 0xd4ba               ; whole -> the loop top
```

— and decides whether the MENU prints; `1000:d4ba` branches on the three
compares directly and decides whether the VISIT continues. They are not the
same instruction and neither is derived from the other. `Game::print_shop_intro`
already carried the first (its `1000:d3d3` citation); `crate::vet::loop_top` is
the second, and it is what the port was missing.

## Two exit keys, not one

`1000:d6a8` pushes the shared `w` (CS `0x848e`, nine push sites image-wide) and
`1000:d6b9` pushes `e` (CS `0x9b6e`, file `0x0b43e`) — the same literal
`entry`'s quit verb compares at `1000:edfa`. Both compares `jz 0xd6c8`, and
`1000:d6c8 jmp short 0xd6e3` lands on the `girl` verb's setup, i.e. the STREET
dispatch chain resumes on `20ae:3972`.

So **`e` at the vet prompt leaves the vet; it cannot quit the game**, because
the vet's `ReadLn` fills `20ae:3a72` and `1000:edfa` reads `20ae:3972`. The vet
is the only location in this port with a second exit key.

## The `h` arm's flavour draw

`1000:d5f6` is the only `Random` call site in the range, and it is the only
draw in any location handler whose `n` is a bare immediate rather than a
district product: `1000:d5f2 mov ax,0x3` / `1000:d5f5 push ax`.

```
1000:d5f2  mov ax,0x3
1000:d5f5  push ax
1000:d5f6  call 0xf78:0x114b
1000:d5fb  cmp ax,0x1
1000:d5fe  jnz 0xd61b               ; 1 -> file 0x0B394, printed by 1000:d614
1000:d61b  cmp ax,0x2
1000:d61e  jnz 0xd63b               ; 2 -> file 0x0B3BD, printed by 1000:d634
1000:d63b  mov di,0x9b27            ; 0 -> file 0x0B3F7 and file 0x0B418
1000:d654  mov di,0x9b48
1000:d66d  mov di,0x9b5f            ; file 0x0B42F, `^2Здоровья #/#`
1000:d672  push [0x38ac]
1000:d676  push [0x38ae]
```

**`ax` at the two dispatch compares is that draw's return value and nothing
else.** No instruction between `1000:d5f6` and `1000:d61b` writes `ax` outside
the `1000:d600` block, which `1000:d5fe` skips on the way there. All three arms
converge on `1000:d66d`, only the `0` arm prints two lines, and none of them
touches a global — so the draw is flavour, but it still moves the RNG stream and
a port that elided it would desynchronise.

## The literals, by owner

Every one is quoted from `orig/g.exe` at the CS offset beside it; the file
offset is that plus `0x18d0`.

| owner | CS | text |
|---|---|---|
| the verb | `0x9966` | `rep` |
| the intro | `0x996a` | `Ты пришел на ремот, к ветеринару напиши  ^6w^7  чтобы уйти` (CS `0x996a`) |
| the menu header | `0x99a5` | `^0Док: не волнуйся всё зарастёт как на собаке` (CS `0x99a5`) |
| row 1 prefix / text | `0x99d3` / `0x99e2` | `  ^2h^7 - за ^` (CS `0x99d3`) and `3^7 рубля тебя залатают` (CS `0x99e2`) |
| row 2 prefix / text | `0x99fa` / `0x9a09` | `  ^2r^7 - за ^` (CS `0x99fa`) and `7^7 рублей починят переломы` (CS `0x9a09`) |
| the loop top's eject | `0x9a25` | `^0Док: вали отсюда ты здоров.` (CS `0x9a25`) |
| the prompt | `0x9a43` | `^0Ветеренар\` (CS `0x9a43`) |
| `r`'s key | `0x9a50` | `r` |
| `r`, line 1 | `0x9a52` | `^0Ого! да тебя не иначе как грузовик откатал!` (CS `0x9a52`) |
| `r`, line 2 | `0x9a80` | `^2Твои переломы залечены.` (CS `0x9a80`) |
| both refusals | `0x9a9a` | `^4Блин халявщик, медицина не бесплатная` (CS `0x9a9a`) |
| `h`'s key | `0x9ac2` | `h` |
| `h`, draw 1 | `0x9ac4` | `^0Щас гайки подтянем и будешь как новый!` (CS `0x9ac4`) |
| `h`, draw 2 | `0x9aed` | `^0Так чё тут у нас? Ага, пара швов и всё будет в порядке.` (CS `0x9aed`) |
| `h`, draw 0 | `0x9b27` / `0x9b48` | `^6Эй, Док а зачем тебе паяльник?` (CS `0x9b27`) and `^0Док: Молчи животное!` (CS `0x9b48`) |
| `h`, always | `0x9b5f` | `^2Здоровья #/#` (CS `0x9b5f`) |
| the `e` exit's key | `0x9b6e` | `e` |
| the undiscovered refusal | `0x9b70` | `^6Сначала найди где находтся эта больница` (CS `0x9b70`) |

The shared exit token `w` (CS `0x848e`) and the `girl` verb token (CS `0x9b9a`)
are the two the vet does not own.

## The counts this map rests on

Over `1000:d3a6`..`1000:d6ed`, all asserted by set equality in
`tools/test_arms_artifacts.py`:

| sweep | count |
|---|---:|
| instructions (aligned) | 379 |
| CS-literal pushes | 23 |
| conditional branches | 23 |
| `Random` call sites | **1** |
| absolute-memory writes | 10 |
| DGROUP addresses touched | 7 |
| DS-pointer pushes | 8 |
| shortstring compares | 5 |
| `WriteLn` calls | 15 |
| `Write` calls | 1 |

Twenty-three CS-literal pushes against twenty-one strings the handler owns: the
other two are the `rep` token at `1000:d3a1` and the `girl` token at
`1000:d6e8`, both inside a boundary span. The refusal literal CS `0x9a9a` is
pushed twice, once per arm (`1000:d596` and `1000:d68a`).

A count is not evidence on its own, so the `Random` sweep also runs over the
whole image and is required to find the population `docs/re/METHODOLOGY.md`
records (86 far calls).

## What the port must change — and did

The full, falsifiable list is `data/vet_arms.json`'s
`what_the_port_must_change`, six numbered items, all landed by Task 34 in
`src/vet.rs`:

1. `r` clears **both** breaks for 7; `h` adds **5 health** for 3 and touches
   neither break.
2. Both arms are **silent** when their own precondition fails. There is no
   refusal literal in the range for one.
3. The loop top re-tests health before every prompt and **ejects** on the
   fall-through.
4. `e` is a second exit key. It leaves the vet; it does not quit.
5. The `h` arm spends one `Random(3)` on flavour and always closes with
   `^2Здоровья #/#`.
6. The vet's `ReadLn` at `1000:d528` does not trim; `Game::shop_turn` does —
   the standing `docs/re/gaps.md` divergence, whose population the vet joins.

Nothing in this range is *blocked*: `20ae:3698`, `20ae:38ac`, `20ae:38ae`,
`20ae:38b0`, `20ae:38b1` and `20ae:38c7` all have fields in `src/`, the handler
calls into no unported routine, and its one draw goes through
`crate::rng::Rng::below_at` like every other. `20ae:3b7a`, the colour-digit
scratch, is modelled as a value by `Game::afford` rather than as a byte — the
existing treatment for all 58 of its image-wide writers.

## Branch coverage

`data/branches.json` holds 23 branches in `1000:d3a6`..`1000:d6ec` and its
STORED `port_touched` column marks 20 of them untouched. That column is a
snapshot taken when Ghidra last ran; the recomputed metric against a later tree
is a different, larger number, and image-wide the same disagreement is 84 stored
against 476 recomputed, so it is structural.
`docs/re/gym.md`'s "Branch coverage, and why two numbers disagree" is the
precedent and `data/vet_arms.json`'s `branch_census.command` recomputes the
stored pair.

Neither number is a to-do list. `port_touched == false` means "no address
citation", never "unimplemented".
