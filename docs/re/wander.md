# The wander turn: the complete `Random` sequence (Task 11b)

Machine-readable form: `data/wander.json`. This document is pure reverse
engineering; it changes no Rust.

Addresses here are dumped out of `orig/g.exe`, and `data/wander.json`
carries the literal opcode bytes at every address it cites, so a five-byte
drift is checkable without a disassembler. Treat those bytes as the check.
Do not treat "every address was verified" as a given: one was not, and the
callout two paragraphs down says which. Per `docs/re/METHODOLOGY.md`, each claim states its
tier: **established from flow**, **corroborated**, or **unverified**.

> **Tier update (Task 11d): every one of the eighteen draws below has now been
> observed in the running original.** `tools/rngtrace` breaks on `Random`'s
> `retf 2` in a qemu guest with `RandSeed` pinned in a patched COPY of the
> binary; five runs logged 1387 draws. All eighteen fired at the call site
> catalogued here, with the `n` catalogued here — including the two computed
> ones, checked at two different districts — and **nothing was contradicted**.
> Their tier is therefore **established from flow, corroborated by live
> trace**; the individual "established from flow" labels below are raised by
> this paragraph rather than being rewritten one by one, because the flow
> claims themselves are unchanged. Per-draw verdicts: `data/wander.json`'s
> `live_trace` fields. Method, the seed patch, the guards, and the full
> comparison: `docs/re/rng-trace.md` and `data/rng_trace.json`.

> **Two citation forms are in play here, and they are not the same
> arithmetic.** Most addresses below are Ghidra `1000:` labels, but the
> runtime entry points — `0f78:114b` (`Random`), `0f78:06c6` (`ReadLn`),
> `0f78:0772` (`Rewrite`), `0f78:081e` (`BlockRead`), `0f78:0825`
> (`BlockWrite`), `0eed:01c2` (`WriteLn`), `0f16:031a` (the delay) — are real
> runtime segments and take the other form. `docs/re/METHODOLOGY.md`, "Address convention, and its range of validity", is the authority for the rule; `tools/addr.py` is its executable form and `python3 tools/re_query.py resolve <citation>` checks any single address against the bytes. Worked and
> byte-verified: `0f78:114b` is file `0x1219b`, which disassembles to the
> 32×16 high-take `mul word [ss:bx+0x4]` pair and `retf 0x2` — `Random`
> itself. Applying the Ghidra form to a far target lands `0xf780` bytes
> short.

> **One address in the first version of this catalogue was wrong**, and it was
> wrong in the way `docs/re/METHODOLOGY.md` warns about: `data/wander.json`
> cited `1000:13dc` as the player-class rank-name lookup. That address is
> `mov di,[0x3952]` — the *enemy's* class. It came from a byte scan and was
> never re-disassembled. The player-class site is `1000:1a36`. Corrected in
> `data/wander.json`'s `class_389c.consumers`, and recorded in
> `.superpowers/sdd/task-11b-report.md`.

## A naming note: `chapter` here is the corpus's `district`

This document says `chapter` throughout — `chapter*20`, `chapter == 5`, the
`chapter-5` endgame arm. That is the byte at `20ae:3692`, and the rest of the
repo calls it **`district`**: `tools/extract_tables.py`'s `DISTRICT_ADDR`,
`docs/re/tables.md`, `docs/re/gaps.md`, `data/other_price_sites.json` and
`src/locations.rs` all use that name. One byte, one value, two labels.

**`district` is canonical; use it in new code.** `data/wander.json`'s `globals`
entry now carries `"name": "district", "aka": "chapter"`. The prose and the
`n_expr` strings below are left reading `chapter` rather than rewritten,
because they were reviewed and byte-verified in that form and a sweeping rename
would put the citations at risk for no gain in meaning. Nothing about the flow
findings changes either way — this is a label, not a claim.

## The shape of `data/wander.json`

* `random` — the call encoding and the byte-scan that establishes the site list
  is complete.
* `globals` — every `20ae:xxxx` this document names, with its tier. Anything
  not established keeps an `unk_<hex>` name.
* `steps` — **the ordered sequence**, one entry per step of the turn, draw and
  non-draw alike, in execution order. Draw steps carry a `draw_ordinal`
  (1..14), the `n` pushed (or `n_expr` when it is computed), the `gate` that
  decides whether the site is reached at all, the `test` that buckets the
  result, and each bucket's effect. `gate: null` means the draw is
  unconditional once the turn starts.
* `nested_routines` — the two callees the turn invokes, including the church's
  own four draws (ordinals 15, 16, 17 and 18), which are part of the same
  stream. 16 and 17/18 sit on mutually exclusive arms, so at most three of the
  four fire on any one turn.
* `class_389c`, `den_setters`, `a_token_reveal` — the other three questions.

## Entry

**Established from flow.** `w` (`1000:ae86`) and `run` (`1000:ae97`) both jump
to `1000:aea1`. `run` additionally prints `^6Забегал мудак.` at `1000:aeeb`,
which costs no draw. There is exactly one wander path; see "Corrections" below.

## The sequence

Fourteen `Random` sites lie between `1000:ae5a` and `1000:b3ba`, and a byte
scan for the far-call encoding `9a 4b 11 78 0f` over the whole 88656-byte
image finds 86 sites in total and exactly these fourteen in that range:

```
af68 afc7 b030 b0dc b186 b1b8 b1ea b21c b272 b2fa b321 b353 b39e b3ae
```

That confirms the brief's eleven-plus-three list is right and complete. The
encoding is fixed-length, so the scan cannot miss a call of this form.

| # | site | `n` | reached when | on what result | effect |
|---|---|---|---|---|---|
| 1 | `1000:af68` | 20 | `[0x3b78] == 0` (`1000:af5d`) | `0` | sets `[0x3b78] := 1` at `1000:af71` **unconditionally**, then prints the den errand only if `[0x3696]` and `[0x38bb]` |
| 2 | `1000:afc7` | 20 | `[0x3b79] == 0` (`1000:afbc`) | `0` | sets `[0x3b79] := 1` at `1000:afd0` **unconditionally**, then prints only if `[0x3696]`, `[0x38cb] >= 100`, `[0x38bb]` |
| 3 | `1000:b030` | 200 | `[0x38bb] == 1` (`1000:b022`) | `0` | the wrong-number gag, files `0x9E44`/`0x9E58`/`0x9E63`/`0x9E65`/`0x9E7D`. No state change |
| 4 | `1000:b0dc` | 100 | `[0x38bb] == 1` (`1000:b0ce`) | `0` | if `[0x3697]`, prints files `0x9EAE`/`0x9EEC`. No state change |
| 5 | `1000:b186` | 10 | always | `0` | if `[0x3698] == 0`, set it (`1000:b196`), print file `0x9F8B` — **Vet** |
| 6 | `1000:b1b8` | 10 | always | `0` | if `[0x3694] == 0`, set it (`1000:b1c8`), print file `0x9FB2` — **Market** |
| 7 | `1000:b1ea` | 100 | always | `0` | if `[0x3699] == 0`, set it (`1000:b1fa`), print file `0x9FC4` — **Club** |
| 8 | `1000:b21c` | 100 | always | `0` | if `[0x369a] == 0`, set it (`1000:b22c`), print file `0x9FFE` — **Gym** |
| 9 | `1000:b272` | 20 | `[0x38c1] != 0` (`1000:b24a`) | `0` | clears **at most one** fracture — jaw (`[0x38b0]`, `1000:b2ae`) takes precedence over leg (`[0x38b1]`, `1000:b289`); see below |
| 10 | `1000:b2fa` | `chapter*20` | `[0x389c] == 6` (`1000:b2ea`) | `<= luck` | proceed to draw 11 |
| 11 | `1000:b321` | `chapter*5` | draw 10 succeeded | any | `[0x3b74] := r+1`; money `+= [0x3b74]`; file `0xA096` |
| 12 | `1000:b353` | 25 | always | `1..25` | the bucket roll; `[0x3971] := r+1`, then buckets into `[0x3970]` |
| 13 | `1000:b39e` | 200 | always | `0` | `call 1000:7c67` — the church. **Spends 1, 2 or 3 more draws and clears `[0x3970]`** |
| 14 | `1000:b3ae` | 100 | always | `0` | `call 1000:7538` — the mage's paid save. Spends no draws |
| 15 | `1000:7f63` | 5 | the church fired | see below | unconditional inside the church |
| 16 | `1000:7fff` | 4 | draw 15 returned `1` (`1000:7ff3`) | `0..3` | one stat +1 |
| 17 | `1000:25fe` | `Σ` class weights | draw 15 returned `0` (`1000:7f68`) **and** level `!= 40` (`1000:2580`) | any | one stat point, inside the level-up routine |
| 18 | `1000:25fe` | `Σ` class weights | same gate as 17 | any | second stat point — the loop bound at `1000:287d` is exactly 2 |

The non-draw steps between them matter to state, not to the stream, and are in
`data/wander.json`'s `steps` array in order: the joint-buff countdown
(`1000:aea1`), the den-loan credit (`1000:af04`), the dealers' 25-walk delivery
counter (`1000:af1d`), the two cooldown "it blew over" messages (`1000:b11e`,
`1000:b145`), the cooldown decrements (`1000:b16c`), the ring's +3 HP
(`1000:b251`) and the Отморозок +1 HP (`1000:b2d4`).

### Draws 1 and 2 burn their slot even when nothing prints

**Established from flow.** `1000:af71` and `1000:afd0` write the never-repeat
flag *before* `1000:af76`/`1000:afd5` test whether the den is known and
`1000:af7d`/`1000:afe3` test whether the player owns a phone. A player who
rolls the `0` without a phone loses the errand permanently and sees nothing.
This is exactly the class of behaviour `docs/re/METHODOLOGY.md` warns cannot be
recovered from output.

### Draw 9 clears at most one fracture, jaw first

**Established from flow.** The table row above used to say "leg and/or jaw",
which is loose; `data/wander.json` was already precise. The branch is:

```
0000B277  09C0              or ax,ax
0000B279  7751              ja 0xb2cc            ; result != 0 -> skip everything
0000B27B  803EB03800        cmp byte [0x38b0],0x0  ; jaw
0000B280  7525              jnz 0xb2a7           ; jaw broken -> straight to the jaw arm
0000B282  803EB13800        cmp byte [0x38b1],0x0  ; leg
0000B287  741E              jz 0xb2a7
0000B289  C606B13800        mov byte [0x38b1],0x0  ; clear leg, print file 0xA043
...
0000B2A7  803EB03800        cmp byte [0x38b0],0x0
0000B2AC  741E              jz 0xb2cc
0000B2AE  C606B03800        mov byte [0x38b0],0x0  ; clear jaw, print file 0xA06B
```

The leg block is reached only when the jaw is intact (`jnz` at `1000:b280`),
and the jaw block is reached only when the jaw is broken. So the two are
mutually exclusive and the jaw has precedence: with both broken, only the jaw
is cleared.

### The bucket roll (draw 12)

`1000:b34d` is `b8 05 00 f7 e8` — `mov ax,5` then the one-operand `imul ax`,
so `DX:AX := 5*5`; the argument pushed is **25**. `1000:b358` is `inc ax` /
`mov [0x3971],al`, giving `1..25`. The chain runs highest first:

```
b35c  cmp [0x3971],0x0a / jc  b368   -> b363  [0x3970] := 4     10..25   16/25
b368  cmp [0x3971],0x09 / ja  b37b
b36f  cmp [0x3971],0x05 / jc  b37b   -> b376  [0x3970] := 3      5..9     5/25
b37b  cmp [0x3971],0x04 / ja  b38e
b382  cmp [0x3971],0x02 / jc  b38e   -> b389  [0x3970] := 2      2..4     3/25
b38e  cmp [0x3971],0x01 / jnz b39a   -> b395  [0x3970] := 1      1        1/25
```

Nothing writes `[0x3970]` when no arm matches — which is what the church
exploits.

### The church can cancel the turn — `1000:7c67`

**Established from flow, and this is the finding with the widest blast radius.**
`1000:7c67`..`1000:82af` is a single procedure (one prologue, one epilogue
`89 ec 5d c3` at `1000:82af`, called from exactly one site, `1000:b3a7`). Its
last act before the epilogue is `1000:8282` = `c6 06 70 39 00`,
`mov byte [0x3970],0`. No jump inside the routine targets an address above
`1000:8282`, so **every** path executes it: on a church turn the bucket is
zeroed after it was rolled, the dispatch at `1000:b3ba` matches no arm, and the
turn produces no encounter.

The church also spends draws. Its three stage arms (`[0x3951] == 2` at
`1000:7c76`, `== 1` at `1000:7ceb`, `== 0` at `1000:7dcb`) all converge on
`1000:7f5f`, so `Random(5)` at `1000:7f63` is unconditional once the church
fires, and `Random(4)` at `1000:7fff` follows when the first returns exactly
`1` — that test is `1000:7ff3` `3d 01 00` (`cmp ax,1`) / `1000:7ff6` `74 03`
(`jz 0x7ffb`). `1000:7f68` is `3d 00 00 74 03`, the **zero** arm, which is a
different outcome entirely (see below). `data/wander.json` cited `1000:7f68` as
draw 16's gate in its first version; that is corrected.

`Random(5)` outcomes: `0` a **forced level-up** — three draws for the church
in total, counting this one (15, 17, 18); see below; `1` a stat blessing via draw 16; `2` the first unfired one-shot gift
(`[0x38bf]` `1000:8134`, `[0x38c0]` `1000:8184`, `[0x38c1]` `1000:81c4`);
`3` `inc byte [0x38b2]` (`1000:81e9`, message
`^1Накладываю на тебя защиту!`); `4` `[0x38cb] += chapter*50 + 50`
(`1000:820d`..`1000:821a`).

The `[0x38bf]` gift at `1000:8134` also does
`add [0x38a8], 1 - (strength mod 2)` at `1000:8130` (state only, no draw), and
so does draw 16's bucket `0` at `1000:8043`. Both were missing from the first
version of this table. `docs/re/progression.md`'s post-kill copy of the same
gift already records the `dmg_min += 1 - str mod 2` term.

#### The `Random(5) == 0` arm grants a level and spends two more draws

**Established from flow.** This is the correction of the worst error in the
first version of this catalogue, which said the arm "prints file `0x930C`
only; no state change despite the text claiming a pontovost rise". That
reasoned from OUTPUT against FLOW and got it backwards — the code makes
exactly the rise the text claims. The arm ends:

```
00007FE4  A1D038            mov ax,[0x38d0]      ; xp threshold
00007FE7  A3CE38            mov [0x38ce],ax      ; xp := threshold
00007FEA  B000              mov al,0x0
00007FEC  50                push ax
00007FED  E836A5            call 0x2526          ; the level-up routine
00007FF0  E94F02            jmp 0x8242
```

`1000:7fed` is the **only near call inside the church** — a linear sweep of
`1000:7c67`..`1000:82b2` that lands exactly on the `89 ec 5d c3` epilogue
finds `call 0x2526` and nothing else of that form. The first version did not
follow it.

`1000:2526` is the level-up routine already documented at
`docs/re/progression.md` § "The level-up: `FUN_1000_2526` (`1000:2526`)" and
its "The weight table" subsection, and cross-checked in `docs/re/combat.md`.
What is new here is only that the **church reaches it**, and therefore that
the wander stream contains its two draws. Its entry test is

```
00002535  A1CE38            mov ax,[0x38ce]
00002538  3B06D038          cmp ax,[0x38d0]
0000253C  7D03              jnl 0x2541           ; xp >= threshold -> proceed
0000253E  E98003            jmp 0x28c1           ; else return
```

and the church has just made the two equal, so the level-up **always**
proceeds. The xp loop at `1000:2546`..`1000:255f` then runs exactly once
(`xp := 0`, `threshold += 10`, level count `:= 1`), because equal-then-subtract
leaves `xp` below the raised threshold. Inside the per-level body:

```
0000257A  807E0400          cmp byte [bp+0x4],0x0   ; church passes param 0
0000257E  750A              jnz 0x258a
00002580  833EA63828        cmp word [0x38a6],0x28  ; level == 40?
00002585  7503              jnz 0x258a
00002587  E91603            jmp 0x28a0              ; capped: no draw, no level
0000258A  A1A638            mov ax,[0x38a6]
0000258D  40                inc ax
0000258E  A3A638            mov [0x38a6],ax         ; the rise the text claims
```

then the two draws, ordinals 17 and 18:

```
000025AA  8B3E9C38          mov di,[0x389c]         ; class
000025AE  D1E7              shl di,1
000025B0  D1E7              shl di,1                ; class*4
000025B2  8A850500          mov al,[di+0x5]         ; ... and [di+2], [di+3], [di+4]
...
000025EE  8946FA            mov [bp-0x6],ax         ; weight_sum
000025F1  C746F80100        mov word [bp-0x8],0x1
000025F6  EB03              jmp 0x25fb
000025F8  FF46F8            inc word [bp-0x8]
000025FB  FF76FA            push word [bp-0x6]
000025FE  9A4B11780F        call word 0xf78:word 0x114b   ; Random(weight_sum)
00002603  40                inc ax
00002604  8946F6            mov [bp-0xa],ax
```

with the loop bound

```
0000287D  837EF802          cmp word [bp-0x8],0x2
00002881  7403              jz 0x2886
00002883  E972FD            jmp 0x25f8
```

— exactly two iterations, hence exactly two draws per level gained, and the
church always gains exactly one level. `n` is the sum of the four class growth
weights at `DS:(class*4 + 2 .. class*4 + 5)`, the same table
`data/xp.json` holds.

So a church turn with `Random(5) == 0` spends **three** draws (15, 17, 18),
grants a level, and rewrites `[0x38a6]`, `[0x38ce]`, `[0x38d0]` and the growth
log. At level 40 it spends **one** draw and grants nothing — but note that
`[0x38ce]` and `[0x38d0]` are rewritten *before* the level-40 test, so the xp
bookkeeping happens either way.

Ordinals 15..18 are labels, not positions in one linear stream: 16 belongs to
the `== 1` arm and 17/18 to the `== 0` arm, and the arms are mutually
exclusive. A church turn spends **1, 2 or 3** draws.

### The mage — `1000:7538`

**Established from flow.** `1000:7538`..`1000:7778`, called only from
`1000:b3b7`. It contains no `Random` call. It prints `Бродя по окрестностям с
самыми грязными намериниями...`, `Ты встретил великого мага и экстрасенса -
Рушеля Блаво.`, `За # рублей он может сделать сохранение прямо здесь.` and
`Ты хочешь сохраниться?`, then `ReadLn`s into a **stack local** `[bp-0x100]`
(`1000:75c7`..`1000:75d1`) — neither `DS:3972` nor `DS:3a72`, so this is a
third input buffer — and compares it against the token `y` (file `0x8D79`).

On `y` it charges and writes both save files: the 694-byte record from
`DS:369c` into the hard-coded name `save_r0.sav` (file `0x8D7B`; the record
size is `mov ax,0x2b6` at `1000:764a`, the `Rewrite` call `0f78:0772` is at
`1000:764e`, the `BlockWrite` call `0f78:0825` is at `1000:765d`), and the seven
discovery flags one byte at a time into `places.sav` (file `0x8D87`,
`1000:766f`..).

**A divergence inside the original.** The price it *prints* is `chapter * 25`
(`1000:758d` is `ba 19 00`). The price it *checks* and *charges* is
`chapter * 50` (`1000:7605` and `1000:7618`, both `ba 32 00`; the debit is
`1000:761d`). `docs/re/tables.md` already recorded the `1000:761d` figure as
`district * 50`; the printed half of the pair is new here.

## `[20ae:389c]` is the character class — closed

**Established from flow, corroborated by the game's own creation menu.**
`docs/re/gaps.md` carried this as "what `[0x389c]` *means* remains unverified"
while `docs/re/progression.md` already had it as the class/rank index. The
progression reading is the right one, and every use lines up.

Two other places in the repo, besides `progression.md` above, already read
`[0x389c]` as the class and are consistent with this: `docs/re/combat.md` (the class-indexed growth-weight and
rank-name tables, and the enemy's mirror field `[0x3952]`), and
`tools/capture_xp_cases.py`'s `CLASS_OF_ANSWER_ADD` (the `answer + 3` mapping
that `1000:71b8` implements). This section confirms them from the write side;
it does not originate the reading.

Five write sites — a byte scan for the two-byte operand `9c 38` over
`0x1000`..`0x11000` finds 41 references, and classifying each by the opcode
byte(s) in front of it gives 23 `mov di,[0x389c]`, 10 `cmp word [0x389c],imm8`,
3 `mov ax,[0x389c]` and exactly five stores:

| at | bytes | what |
|---|---|---|
| `1000:6fed` | `a3 9c 38` | `Val()` of the first creation prompt's answer |
| `1000:6ffc` | `a3 9c 38` | reset to 0 when that answer was `4` (test `1000:6ff0`) |
| `1000:712a` | `a3 9c 38` | `Val()` of the re-prompt's answer |
| `1000:713d` | `a3 9c 38` | reset to 0 when out of `0..3` (`1000:712d`, `1000:7134`) |
| `1000:71b8` | `83 06 9c 38 03` | `add word [0x389c],3` |

Everything else is a read. The scan's limit, stated so it is not over-read: it
finds only direct memory operands carrying the literal displacement, so a store
through a pointer register would not appear. The one such store that does exist
is the 694-byte character-record `BlockRead` at `1000:6c01` (`0f78:081e` into `DS:369c`);
`DS:369c + 0x200 = DS:389c`, which is why the class is `.SAV` offset `0x200`.
The two `BlockWrite` (`0f78:0825`) counterparts are the calls at `1000:765d`
(the mage) and `1000:acc8` (the district-advance autosave at
`1000:ac5e`..`1000:ad12`). `1000:7658` and `1000:acc3` are the `bf 9c 36`
(`mov di,0x369c`) that sets each one up, not the call itself.

Answer `4` is `4-Чё за батва?` (file `0x7FA1`): it zeroes the scratch, prints
the four class descriptions, and re-prompts. Those descriptions name the
bonuses, and each one is a branch this task traced:

| answer | class | rank | the menu's own text | the code |
|---|---|---|---|---|
| 0 | 3 | Подтсан | `^1Пацан - это нормальный тип. (Бонус - Гёлфренд, Клуб).` | `1000:73cf`/`1000:73d4` set Girl + Club |
| 1 | 4 | Отморозок | `^1Отморозок - тупой корявый мудак. (Бонус - Самолечение царапин).` | `1000:b2cf`..`1000:b2dd`, +1 HP per walk |
| 2 | 5 | Гопник | `^1Гопник - гоп он и есть гоп. (Бонус - Притон)` | `1000:73c3` sets Den |
| 3 | 6 | Вор | `^1Вор - везучий ублюдок. (Бонус - Воровство, Барыги)` | draws 10/11 above, and `1000:73e0` sets Dealers |

That is a flow claim (the branches) corroborated by output (the menu text),
never the other way round.

### What reaches `1000:73bb` — closed

**Established from flow.** `1000:6a0d`..`1000:73ed` is one procedure (the only
`55 89 e5` prologue in `0x6800`..`0x7400`, `ret` at `1000:73ed`), called once
from `1000:ab72`. Both of its exits converge on `1000:7262`: the
new-character path falls through `1000:71ea`..`1000:7261`, and the
successful-load path jumps there from `1000:6da0` (both `places.sav` arms
converge on `1000:6d8c` — the success arm via `1000:6d39` `jmp 0x6d8c`, the
failure arm by falling through `1000:6d87`). `1000:7262` dispatches the
district intro on `[0x3692]`, and `1000:7369` (`cmp byte [0x3692],1` /
`jnz 0x73bb`) skips only the district-1 text — never the grants. So
**`1000:73bb` runs on every entry into the game, new character or loaded save**,
and re-applies the class bonuses. It also sets `[0x3e35] := 5` (`1000:73e5`),
the den-loan credit the wander preamble tops up.

### Why the two resets spare what they spare

`1000:6d45`/`1000:6d56` (`== 3`, club and girl) and `1000:6d67` (`== 5`, den),
and the same three compares at `1000:aba0`/`1000:abb1`/`1000:abc2`, are exactly
the class bonuses. The resets clear what you *discovered* and keep what you
*are*. **One asymmetry, established from flow:** neither reset spares
Dealers, so a Вор loses the dealers on a district advance while a Подтсан
keeps girl+club and a Гопник keeps the den. `1000:73bb` restores it, but only
on game entry, not on district advance. That is the original's behaviour, not a
transcription slip: `1000:abbd` is an unconditional `c6 06 95 36 00`.

## The three Den setters — closed

* **`1000:52b3`** — the post-kill block in `FUN_1000_3d11`. If `[0x3696] == 0`
  (`1000:5295`) and `level - (chapter-1)*10 >= 3` (`1000:529c`..`1000:52b1`,
  a `jl` skip), set the flag and print file `0x5311`. **Flow.**
* **`1000:4aa5`** — the de-level (flee) penalty routine. Skipped entirely when
  `[0x389c] == 5` (`1000:4a87`), then requires `level - (chapter-1)*10` to be
  **exactly** 3 (`1000:4aa0`/`1000:4aa3`, a `jnz`), evaluated *before* the
  level decrement at `1000:4ac3`. **Flow.** The store and its message
  contradict each other: the byte is `c6 06 96 36 01` (verified), i.e. the den
  flag is *set*, while the line printed is
  `^4Такого конявого непустят в местный притон!`. The den gate at `1000:d80c`
  reads nothing but this flag, so the refusal has no mechanical effect. Whether
  a clear was intended is **unverified** and would need the author, not the
  binary.
* **`1000:ae1f`** — the endgame. `1000:adbf` tests `chapter == 5`; that arm
  prints files `0x9CF2`/`0x9D16`/`0x9D4E`, sets `[0x3c83] := 1` at
  `1000:ae13`, and `1000:ae18` then tests the byte it just wrote, so the branch
  is always taken there. The block grants the den and runs
  `FUN_1000_11c2(0)`, `FUN_1000_3d11(3)` (the rector), `FUN_1000_11c2(1)`,
  `FUN_1000_3d11(4)` (the endgame). **Flow.**

  Worth flagging: `1000:ae18` sits at the top of every turn — the main loop's
  back-edge is `1000:ee01` `jmp 0xab75` — and nothing clears `[0x3c83]`
  (its only writes are `1000:7364` and `1000:ae13`). So once chapter 5 is
  reached, this block runs every turn. Whether `FUN_1000_3d11(4)` returns is
  not traced here.

## The `a` reveal's input buffer — closed

**Established from flow.** The den handler (`pr`, `1000:d802`) prints its menu
and then does a single `ReadLn` into `DS:3a72` at `1000:db00`..`1000:db09` —
the only `0f78:06c6` call between `1000:d802` and `1000:dd48` — and compares
that buffer against a chain of tokens, one of which is the single character `a`
(file `0xB899`) at `1000:dcea`/`1000:dcef`. The hidden Dealers+Gym reveal is
therefore typed at the den's `^0Притон\` prompt, not at the top-level prompt.

## Named globals this task established

Full list with tiers in `data/wander.json`'s `globals`. The ones that were
unknown before:

* **`20ae:38bb` — the player owns a mobile phone.** Set at `1000:550d` and
  `1000:566d` (both print `^1Ты нашёл мобилу`, file `0x546C`) and at
  `1000:c969`, the dealers' 48-rouble purchase (price byte `DS:0b39`,
  message file `0xAC8D`). Every phone-call event in the preamble is gated on
  it. **Flow, corroborated by the strings.**
* **`20ae:38c1` — the ring "Господи помилуй".** Named an anonymous one-shot
  flag in `docs/re/progression.md` ("text only"). Its granting text is
  `^1Восст. жизни - 3, 5% - самозарост переломов` (file `0x53DD`), and
  `1000:b24a`..`1000:b2cb` is precisely that: +3 HP per walk and a `Random(20)`
  — 1 in 20, the advertised 5% — that clears a leg or jaw fracture. **Flow,
  corroborated by the item's own description.** `docs/re/progression.md`'s
  "text only" should be read as "grants no immediate stat"; the ring has an
  ongoing effect.
* **`20ae:38cb` — a street-cred counter, distinct from the level.** Grown per
  kill at `1000:5291`, spent 2 at `1000:db9b` when borrowing beer money in the
  den, printed at `1000:dc79` under `^4Твоя понтовость сейчас = #.`
  (file `0xB857`), granted `chapter*50 + 50` by the church at `1000:821a`.
  The original calls both this and the level (`DS:38a6`) "понтовость"; they are
  different words in memory. This closes half of
  `docs/re/progression.md`'s "what `DS:38cb` counts" question.
* **`20ae:38b0` / `20ae:38b1` — jaw and leg injuries**, named by the messages
  at `1000:b2ae` (file `0xA06B`) and `1000:b289` (file `0xA043`).
* **`20ae:3b76` / `20ae:3b77` — market and club cooldowns.** Set to 5 at
  `1000:c465` / `1000:e23e`, decremented once per walk at `1000:b173` /
  `1000:b17e`, consumed as gates at `1000:b95e` (`mar`) and `1000:df1a` (`kl`).
* **`20ae:3b78` / `20ae:3b79` — the two one-shot den errands**, set by draws 1
  and 2 and consumed in the den at `1000:dc5e` / `1000:dec8`.
* **`20ae:3e35` — den loan credit.** Initialised to 5 at `1000:73e5`, topped up
  once per walk while below `chapter*10` (`1000:af19`), spent one per beer loan
  at `1000:dba0` (which also does money `+= 2`, `[0x38cb] -= 2`).
* **`20ae:3c83` — endgame armed.** Written only at `1000:7364` and
  `1000:ae13`; read at `1000:411d`, `1000:48eb`, `1000:4f8c`, `1000:ae18`.
* **`20ae:394d` — the pistol.** Set at `1000:cd05` by a 150-rouble dealer
  purchase (price byte `DS:0b3e`); arms the 25-walk `DS:3e32` counter. The
  name was open until Task 16 read it off the character sheet:
  `1000:1d38 cmp byte [0x394d],0x0` selects the arm that prints
  `^1У тебя есть пистолет` at `1000:1d51`, with no branch in between — see
  `docs/re/character-sheet.md`. The JSON's `dealer_order_placed` is now a
  stale name, not a disagreement.
* **`20ae:38b2` — the armour byte (`Броня`).** Fighter-record offset `+0x16`
  (`0x38b2 - 0x389c`), in the same injury/state group as the jaw (`+0x14`)
  and the leg (`+0x15`) above.
  `1000:81e9` increments it under `^1Накладываю на тебя защиту!`, i.e. the
  church grants +1 armour. The consumer was open until Task 11c: subtracted
  from damage at `1000:4769`, printed as `^2Броня #` at `1000:227b` (value
  pushed at `1000:228a`; `docs/re/character-sheet.md`), saved at
  `.SAV 0x216` (`docs/re/save-format.md`). The enemy's mirror, `20ae:3968` /
  `[0x3968]`, is a separate byte with its own print routine at `1000:163f`,
  using a second copy of the same string — it is not this field and is not
  saved at `.SAV 0x216`. `data/wander.json`'s `unk_38b2` is now a stale
  name, not a disagreement.

## Corrections to existing `docs/re/` content

All five are now folded back into the documents themselves (fix wave 1); the
list is kept as the record of what changed and why.

1. **`docs/re/command-dispatch.md` step 5 was wrong.** It said `1000:b358` sits
   "within the *district-transition* preamble" and that "the specific `Random`
   call feeding the regular-turn branch was not found". There is one wander
   path. `w`/`run` land at `1000:aea1`, run straight through
   `1000:af04`..`1000:b34d`, roll at `1000:b353`, and fall into the dispatch at
   `1000:b3ba` → `1000:b3bd` → `1000:b4e8` → `1000:b5ae`. `1000:b353` **is**
   the call feeding the regular-turn branch. `src/game.rs`'s assumption is
   correct; the reasoning recorded for it was not. (The genuine
   district-transition block is `1000:ab75`..`1000:ae18`, a different region.)
   Step 4's "not catalogued" was corrected in the same pass. **Applied.**
2. **`docs/re/gaps.md`** carried `[0x389c]` as unverified in three places. It is
   the class; see above. **Applied.**
3. **`docs/re/gaps.md`** said the preamble's "other seven draws have not been
   catalogued (no `n`, no gate, no effect)". All eleven now are, plus the two
   after the bucket roll and the church's four. **Applied.**
4. **`docs/re/progression.md`**'s one-shot event 3 (`DS:38bf`/`38c0`/`38c1`
   table) listed `[0x38c1]` as "text only". It grants the ring whose effect is
   the per-walk regen at `1000:b24a`. The same three gifts are also reachable
   from the church at `1000:80c8`..`1000:81c9` — a second grant site for all
   three flags, not just the post-kill block. **Applied.**
5. **`docs/re/tables.md`**'s `1000:761d` row (`district * 50`) is right, but the
   same routine *prints* `district * 25` (`1000:758d`). Both numbers are in the
   binary and both are now recorded. **Applied.**
6. **`data/command_dispatch.json`** recorded the three Den setters
   (`1000:ae1f`, `1000:4aa5`, `1000:52b3`) as trigger-UNVERIFIED. All three are
   established from flow; `setters_found` now carries each trigger, and
   `1000:4aa5` keeps its unresolved set-while-refusing note. **Applied.**

## What a turn costs, in draws

**Established from flow.** There is no fixed per-turn count; a differential
test must walk the `steps` array and evaluate each `gate`. But the common case
is worth stating, because getting it wrong is easy:

For a fresh Подтсан with no phone and no ring, draws 3/4 (need the phone),
9 (needs the ring) and 10/11 (need class 6) are gated off, leaving
**1, 2, 5, 6, 7, 8, 12, 13, 14 — nine draws per turn.**

That nine is the *steady state*, not just turn 1. Draws 1 and 2 are each gated
on their one-shot flag being still clear (`1000:af5d`, `1000:afbc`), and the
flag is written at `1000:af71` / `1000:afd0` — **after** the
`or ax,ax / jnz` at `1000:af6d` / `1000:afcc`:

```
0000AF5D  803E783B00        cmp byte [0x3b78],0x0
0000AF62  7558              jnz 0xafbc          ; already fired -> skip the draw
0000AF64  B81400            mov ax,0x14
0000AF67  50                push ax
0000AF68  9A4B11780F        call word 0xf78:word 0x114b
0000AF6D  09C0              or ax,ax
0000AF6F  754B              jnz 0xafbc          ; non-zero -> flag NOT set
0000AF71  C606783B01        mov byte [0x3b78],0x1
```

So the flag is set only on the 1-in-20 roll that actually returns `0`. Until
that happens — expected ~20 turns for each, independently — the draw fires
every turn. The count falls to **eight** when one one-shot has fired and
**seven** only once both have.

Each other state shifts it again: a phone adds draws 3 and 4, the ring adds
draw 9, class 6 (Вор) adds draw 10 and, when the theft succeeds, draw 11. A
church turn (draw 13 returns `0`) adds 1, 2 or 3 more and produces no
encounter. A mage turn (draw 14 returns `0`) adds none but blocks on input.

**A bucket-3 turn adds far more than any of those.** `FUN_1000_0d14` alone
spends 13 or 14 draws outside its stat loop (14 when `[0x3693]` is set and
`1000:0d91` fires), and the stat loop itself spends
`Σ weights + крутизна * 2`. Across the thirteen encounters
`data/rng_trace.json` captured, that loop ran between **6** and **104**
times — 348 iterations in total; the 104 is run E's turn 6, class 8 at
крутизна 42. Then the notice roll at `1000:b5f1` or `1000:b792`, and the
decline roll at `1000:b725` on the aggressive block. Any differential test
that assumes a bounded per-turn draw count will be wrong on bucket 3.

## What this does NOT settle

* Whether `Random`'s `n` of `0` is reachable at draws 10/11 — `chapter` is
  1..5, so `chapter*20` and `chapter*5` are never 0 in any state this pass saw,
  but `[0x3692]`'s full write set was not enumerated.
* ~~Bucket 3's fight flow (`1000:b5ae`..`1000:b82c`) still has the two open
  questions `docs/re/gaps.md` records: which of `1000:b691` / `1000:b721` a
  real encounter reaches, and the cop-class stealth path at `1000:b76a`.~~
  **Both answered by Task 11f**, and the catalogue below now extends past the
  bucket dispatch to cover them; the full derivation is in
  `docs/re/gaps.md`'s "The random-encounter opponent" section.

  * `1000:b5fc`..`1000:b61b` decides between the two answer blocks: the
    player's luck against `Random(district * 7 + 15)` from `1000:b5f1`
    (halved first when `[0x38bc]`, the зоновская наколка, is set), then a
    class threshold of 3 if luck lost and 7 if luck won. Meeting it takes
    `1000:b6a0` (which has the `Random(2)` decline roll at `1000:b725`);
    otherwise `1000:b61e`, which has none.
  * `1000:b76a` is entered only when the *rolled enemy's* class is 8
    (`1000:b5c0`), asks nothing, and rolls the same `district * 7 + 15` at
    `1000:b792` — never halved. Luck wins → no fight; luck loses with
    `[0x38b3]` (тёмные очки) → no fight; luck loses without them →
    `^4Запалил!` and `FUN_1000_3d11(0)` straight away.
  * `1000:b54e` is bucket 2's own draw and was already catalogued as draw 13a
    in `Game::wander_girl`; `1000:b841`, `1000:b871`, `1000:b891` and
    `1000:b8bd` are bucket 4's and are still **outside** the catalogue (that
    bucket writes flavour only — `docs/re/gaps.md`).
  * The fourteen draws of `FUN_1000_0d14` itself
    (`1000:0d26`, `0d70`, `0d91`, `0dcc`, `0ddd`, `0df0`, `0e04`, `0efd`,
    `102e`, `109c`, `10c4`, `113c`, `1162`, `1197`) are catalogued in
    `docs/re/gaps.md` rather than duplicated here: they belong to a called
    routine, not to the wander turn's own straight-line sequence.
* Whether `FUN_1000_3d11(4)` returns, and therefore whether the chapter-5
  block at `1000:ae1f` really re-runs every turn.
* ~~The name of the item at `DS:394d`.~~ **CLOSED by Task 16** — the pistol,
  named by the character sheet at `1000:1d38`/`1000:1d51`
  (`docs/re/character-sheet.md`).
* ~~`unk_38b2`.~~ **CLOSED by Task 11c** — the armour byte, record `+0x16`,
  consumed at `1000:4769` and printed as `^2Броня #` (`1000:227b`).
* ~~No live breakpoint was used.~~ **Done in Task 11d** — `tools/rngtrace`,
  `docs/re/rng-trace.md`, `data/rng_trace.json`. The fourteen in-range sites
  were observed in order on a pinned seed (the order is asserted by
  `compare.check_order`; `data/rng_trace.json.order_check` records 86 turns
  checked, 0 violations), and the church's four when it fired.
  What that pass did NOT settle is listed there: the probabilities still come
  from the comparison constants and never from counting observed outcomes, the
  fight-flow questions below are untouched, bucket 2's `y` path was never
  taken, and only districts 1 and 3 were visited.
