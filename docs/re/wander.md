# The wander turn: the complete `Random` sequence (Task 11b)

Machine-readable form: `data/wander.json`. This document is pure reverse
engineering; it changes no Rust.

Every address here was dumped out of `orig/g.exe` and re-disassembled before
being written down — `file_off = 0x18d0 + off` for a `1000:off` code address,
`0x123b0` for `20ae:0000`. `data/wander.json` carries the literal opcode bytes
at every address it cites, so a five-byte drift is checkable without a
disassembler. Per `docs/re/METHODOLOGY.md`, each claim states its tier:
**established from flow**, **corroborated**, or **unverified**.

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
  own two draws (ordinals 15 and 16), which are part of the same stream.
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
| 9 | `1000:b272` | 20 | `[0x38c1] != 0` (`1000:b24a`) | `0` | clears `[0x38b1]` (leg, `1000:b289`) and/or `[0x38b0]` (jaw, `1000:b2ae`) |
| 10 | `1000:b2fa` | `chapter*20` | `[0x389c] == 6` (`1000:b2ea`) | `<= luck` | proceed to draw 11 |
| 11 | `1000:b321` | `chapter*5` | draw 10 succeeded | any | `[0x3b74] := r+1`; money `+= [0x3b74]`; file `0xA096` |
| 12 | `1000:b353` | 25 | always | `1..25` | the bucket roll; `[0x3971] := r+1`, then buckets into `[0x3970]` |
| 13 | `1000:b39e` | 200 | always | `0` | `call 1000:7c67` — the church. **Spends 1–2 more draws and clears `[0x3970]`** |
| 14 | `1000:b3ae` | 100 | always | `0` | `call 1000:7538` — the mage's paid save. Spends no draws |
| 15 | `1000:7f63` | 5 | the church fired | see below | unconditional inside the church |
| 16 | `1000:7fff` | 4 | draw 15 returned `1` | `0..3` | one stat +1 |

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
`1` (`1000:7f68` is `3d 00 00 74 03`, i.e. `cmp ax,0` / `jz`; the `1` arm is
the `cmp ax,1` at `1000:7ff3`).

`Random(5)` outcomes: `0` text only (file `0x930C`, which *claims* a
понтовость rise the code does not make); `1` a stat blessing via draw 16;
`2` the first unfired one-shot gift (`[0x38bf]` `1000:8134`, `[0x38c0]`
`1000:8184`, `[0x38c1]` `1000:81c4`); `3` `inc byte [0x38b2]`
(`1000:81e9`, message `^1Накладываю на тебя защиту!`); `4`
`[0x38cb] += chapter*50 + 50` (`1000:820d`..`1000:821a`).

### The mage — `1000:7538`

**Established from flow.** `1000:7538`..`1000:7778`, called only from
`1000:b3b7`. It contains no `Random` call. It prints `Бродя по окрестностям с
самыми грязными намериниями...`, `Ты встретил великого мага и экстрасенса -
Рушеля Блаво.`, `За # рублей он может сделать сохранение прямо здесь.` and
`Ты хочешь сохраниться?`, then `ReadLn`s into a **stack local** `[bp-0x100]`
(`1000:75c7`..`1000:75d1`) — neither `DS:3972` nor `DS:3a72`, so this is a
third input buffer — and compares it against the token `y` (file `0x8D79`).

On `y` it charges and writes both save files: the 694-byte record from
`DS:369c` into the hard-coded name `save_r0.sav` (file `0x8D7B`, `Rewrite`
record size `0x2b6` at `1000:764e`, `BlockWrite` at `1000:765d`), and the seven
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
The two `BlockWrite` counterparts are `1000:7658` (the mage) and `1000:acc3`
(the district-advance autosave at `1000:ac5e`..`1000:ad12`).

Answer `4` is `4-Чё за батва?` (file `0x7FA1`): it zeroes the scratch, prints
the four class descriptions, and re-prompts. Those descriptions name the
bonuses, and each one is a branch this task traced:

| answer | class | rank | the menu's own text | the code |
|---|---|---|---|---|
| 0 | 3 | Подтсан | `^1Пацан - это нормальный тип. (Бонус - Гёлфренд, Клуб).` | `1000:73cf`/`1000:73d4` set Girl + Club |
| 1 | 4 | Отморозок | `^1Отморозок - тупой корявый мудак. (Бонус - Самолечение царапин).` | `1000:b2cf`..`1000:b2dd`, +1 HP per walk |
| 2 | 5 | Гопник | `^1Гопник - гоп он и есть гоп. (Бонус - Притон)` | `1000:73c3` sets Den |
| 3 | 6 | Вор | `^1Вор - везучий ублюдок. (Бонус - Воровство, Барыги)` | draws 10/11 above, and `1000:73e0` sets BigMarket |

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
BigMarket, so a Вор loses the dealers on a district advance while a Подтсан
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
(file `0xB899`) at `1000:dcea`/`1000:dcef`. The hidden BigMarket+Gym reveal is
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
* **`20ae:394d`** — set at `1000:cd05` by a 150-rouble dealer purchase
  (price byte `DS:0b3e`); arms the 25-walk `DS:3e32` counter.
  `docs/re/tables.md` calls that counter "the silencer"; the item's name is
  **not** established here, so the JSON keeps the neutral
  `dealer_order_placed`.
* **`20ae:38b2`** — left as `unk_38b2`. `1000:81e9` increments it under
  `^1Накладываю на тебя защиту!`; no consumer was traced.

## Corrections to existing `docs/re/` content

1. **`docs/re/command-dispatch.md` step 5 is wrong.** It says `1000:b358` sits
   "within the *district-transition* preamble" and that "the specific `Random`
   call feeding the regular-turn branch was not found". There is one wander
   path. `w`/`run` land at `1000:aea1`, run straight through
   `1000:af04`..`1000:b34d`, roll at `1000:b353`, and fall into the dispatch at
   `1000:b3ba` → `1000:b3bd` → `1000:b4e8` → `1000:b5ae`. `1000:b353` **is**
   the call feeding the regular-turn branch. `src/game.rs`'s assumption is
   correct; the reasoning recorded for it was not. (The genuine
   district-transition block is `1000:ab75`..`1000:ae18`, a different region.)
2. **`docs/re/gaps.md`** carried `[0x389c]` as unverified in three places. It is
   the class; see above.
3. **`docs/re/gaps.md`** says the preamble's "other seven draws have not been
   catalogued (no `n`, no gate, no effect)". All eleven now are, plus the two
   after the bucket roll and the church's two.
4. **`docs/re/progression.md`**'s one-shot event 3 (`DS:38bf`/`38c0`/`38c1`
   table) lists `[0x38c1]` as "text only". It grants the ring whose effect is
   the per-walk regen at `1000:b24a`. The same three gifts are also reachable
   from the church at `1000:80c8`..`1000:81c9` — a second grant site for all
   three flags, not just the post-kill block.
5. **`docs/re/tables.md`**'s `1000:761d` row (`district * 50`) is right, but the
   same routine *prints* `district * 25`. Both numbers are in the binary.

## What this does NOT settle

* Whether `Random`'s `n` of `0` is reachable at draws 10/11 — `chapter` is
  1..5, so `chapter*20` and `chapter*5` are never 0 in any state this pass saw,
  but `[0x3692]`'s full write set was not enumerated.
* Bucket 3's fight flow (`1000:b5ae`..`1000:b82c`) still has the two open
  questions `docs/re/gaps.md` records: which of `1000:b691` / `1000:b721` a
  real encounter reaches, and the cop-class stealth path at `1000:b76a`.
  Draws at `b54e`, `b5f1`, `b725`, `b792`, `b841`, `b871`, `b891`, `b8bd` are
  downstream of the bucket dispatch and are out of this task's scope.
* Whether `FUN_1000_3d11(4)` returns, and therefore whether the chapter-5
  block at `1000:ae1f` really re-runs every turn.
* The name of the item at `DS:394d`.
* `unk_38b2`.
* No live breakpoint was used. Everything above is static flow; a `tools/qemu`
  run enumerating the fourteen sites in order on a pinned seed would corroborate
  the sequence, and is the obvious next step for Task 12.
