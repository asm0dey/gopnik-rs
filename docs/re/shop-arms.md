# `bmar` rows 1–6: the purchase arms

**Scope.** What the dealers' handler does *after* a row key is typed at the
`^0Барыги\` prompt, for menu rows 1..6 only. The menu half — which rows print,
at what price, behind which district gate — is `docs/re/tables.md` §2 and
`data/shops.json`, and is not restated here. Rows 7, 8 and 9 (the pistol, its
cartridges, its silencer) were mapped by Task 18 and are already ported; they
are named here only where a row 1–6 claim depends on them.

**Machine-readable twin:** `data/shop_arms.json`.
**Re-derivation:** `python3 tools/test_shop_arms.py` decodes every address and
every literal in both this file and the artifact out of `orig/g.exe`, so
neither can drift from the binary.

**The market's nine arms are the second half of this file** — see *`mar` rows
1–9: the purchase arms* below, mapped by Task 25 over its own range
(`1000:b94a`..`1000:c4be`). The two shops were measured separately and neither
half's findings are inferred from the other's; where they differ, they differ
loudly.

Every claim below is **established from flow** unless it says otherwise. The
range decoded is `1000:c8ce`..`1000:ccc4` — the row-1 setup to the row-7
setup — as one run of 474 instructions. Its alignment is not assumed: every
row's key compare is an instruction boundary an aligned walk from `entry`
reaches, and each is exactly six instructions past the span start the walk
below is anchored on.

---

## The shape all six share

One line is read into the buffer at `20ae:3a72` by the `ReadLn` at
`1000:c8c9`, after the prompt `^0Барыги\` (CS `0x937b`) is printed. Then each
row compares that one buffer against its own one-character literal with
Borland's shortstring compare `0f78:0bd8`, using a rigid six-instruction push
idiom:

```
1000:c8ce  mov di,0x3a72     ; the typed line, in DGROUP
           push ds / push di
1000:c8d3  mov di,0x8dca     ; the row key literal, in CS
           push cs / push di
1000:c8d8  call 0xf78:0xbd8
1000:c8dd  jnz 0xc92b        ; -> the NEXT row's setup
```

Each row's miss branch targets the next row's setup, and each arm's tail
rejoins there too, so the six arms are a chain of independent `if`s over one
buffer — the same shape `docs/re/combat-dispatch.md` records for the combat
prompt. Row 6's miss lands on `1000:ccc4`, whose own compare at
`1000:ccce` — the same six-instruction idiom, one literal further on — is
Task 18's `7`. That is what bounds this range on the right without assuming
it.

| row | key compare | key literal | price byte | debit |
|---|---|---|---|---|
| 1 Косяк | `1000:c8d8` | CS `0x8dca` | `20ae:0b38` = 15 | `1000:c90a` |
| 2 Краденый мобильник | `1000:c935` | CS `0x8e4b` | `20ae:0b39` = 30 | `1000:c973` |
| 3 Офигенный косяк | `1000:c9b5` | CS `0x8ea5` | `20ae:0b3a` = 20 | `1000:c9eb` |
| 4 зоновскую наколку | `1000:cad1` | CS `0x8ef7` | `20ae:0b3b` = 10 | `1000:cb0f` |
| 5 Кастет | `1000:cb51` | CS `0x8f6b` | `20ae:0b3c` = 25 | `1000:cba7` |
| 6 Дубинка | `1000:cc0e` | CS `0x8fc6` | `20ae:0b3d` = 50 | `1000:cc60` |

**The affordability sense is the same in all six, and it was checked per row
rather than assumed.** Each is `mov al,[price]` / `xor ah,ah` /
`cmp ax,[0x38c7]` / `jle <buy>` — the branch byte is `7e` at `1000:c8e8`,
`1000:c94c`, `1000:c9c8`, `1000:cae8`, `1000:cb80` and `1000:cc39` — so the
purchase goes through when `price <= money`, and the *refusal* is the
fall-through. That is the same sense rows 7–9 use.

**No row 1–6 has the silencer's price bug.** For every row the byte the colour
test reads, the byte the affordability test reads and the byte the debit
subtracts are the same address, and it is the address `docs/re/tables.md` §2
already records. `bmar` row 9 remains the only split (60 charged, 70 printed).

---

## The district question — answered, and it is a divergence

**No arm of `bmar` rows 1..6 reads the district byte `20ae:3692` at all.** Not
one instruction in the 474-instruction aligned run over
`1000:c8ce`..`1000:ccc4` carries a direct-memory operand equal to `0x3692`, and
the byte pair `92 36` does not occur anywhere in that span — so there is not
even a byte-scan candidate to discard, and the negative does not rest on the
decoder alone.

**Nor do rows 7, 8 and 9.** Their arms are Task 18's and are not re-mapped
here, but the same two measurements were taken over `1000:ccc4`..`1000:ce80`
(the row-7 setup to the `x` compare): no operand equal to `0x3692`, and no
`92 36` byte pair. So the dealers' **buy path carries no district test at all,
for any of the nine rows.**

**Every district gate in the handler is a menu-print gate, and there are
five.** The inventory below is asserted set-equal to every `[0x3692]` operand
an aligned decode of `1000:c4be`..`1000:ccd8` finds — the verb compare to the
start of Task 18's block — so an omission reds
`tools/test_shop_arms.py`. (An earlier revision of this document said "the two
district gates that exist in this handler" and listed the first two. Every
address in that list was right; the list was not. That is the recurring defect
`docs/re/METHODOLOGY.md` names — an inventory whose completeness claim stopped
the next search — and it is why the sweep now exists.)

| gate | branch | skips the menu line for |
|---|---|---|
| `1000:c755 cmp byte [0x3692],0x3` | `1000:c75a jbe 0xc7ba` | row 7, pistol (`20ae:0b3e` at `1000:c75c`) |
| `1000:c7ba cmp byte [0x3692],0x3` | `1000:c7bf jbe 0xc81d` | row 8, cartridges (`20ae:0b3f` at `1000:c7c1`) |
| `1000:c81d cmp byte [0x3692],0x3` | `1000:c822 jbe 0xc88e` | row 9, silencer (`1000:c824` is its two extra menu gates; `1000:c88e` is the prompt push) |

and the two this section is really about:

| gate | branch | skips the menu line for |
|---|---|---|
| `1000:c68d cmp byte [0x3692],0x1` | `1000:c692 jbe 0xc6f1` | row 5, Кастет (`20ae:0b3c` at `1000:c694`) |
| `1000:c6f1 cmp byte [0x3692],0x2` | `1000:c6f6 jbe 0xc755` | row 6, Дубинка (`20ae:0b3d` at `1000:c6f8`) |

All five gate the *listing*, nothing else. The three `district>3` entries are
the ones `docs/re/tables.md` §2 already records for rows 7-9.

So at district 1 the menu shows neither row 5 nor row 6, and typing `5` or `6`
buys them anyway; below district 4 the same holds for rows 7, 8 and 9. This
closes the open question `docs/re/gaps.md` recorded against
`1000:cc04`..`1000:ccd8` and widens it twice over: it is not only row 6, and it
is not only rows 5 and 6.

**This is a claim about `bmar` and about nothing else.** The `mar` shop's arms
were not decoded for this task, and inferring one shop's behaviour from the
other's is exactly the symmetry-as-evidence error `docs/re/METHODOLOGY.md`
forbids.

---

## Row 1 — Косяк, 15 руб.

Span `1000:c8ce`..`1000:c92b`. Key compare `1000:c8d8`, miss
`1000:c8dd jnz 0xc92b`.

**Gates: one.** `1000:c8e4 cmp ax,[0x38c7]` / `1000:c8e8 jle 0xc905`. There is
no already-own test and no prerequisite test, so the row is **repeatable**.
Refusal: `^4Чёрт, бабок не хватает.` (CS `0x9385`), pushed at `1000:c8ea`.

**Effect.**

```
1000:c90a  sub [0x38c7],ax
1000:c90e  inc [0x38c5]
```

`20ae:38c5` is a **word count**, not a flag — the joints in the player's
pocket. Confirmation `^2Ты купил косяк` (CS `0x939f`), pushed at `1000:c912`.

**Read where.** 12 operand-field references image-wide, 11 of them outside this
arm: the character sheet's joints line reads it at `1000:23b4`, and both
copies of the `kos` handler consume one at `1000:4b44` (in a fight) and
`1000:e9aa` (at the street prompt). So the effect is fully consumed and the
port has real behaviour to implement, not just a flag.

---

## Row 2 — Краденый мобильник, 30 руб.

Span `1000:c92b`..`1000:c9ab`. Key compare `1000:c935`, miss
`1000:c93a jnz 0xc9ab`.

**Gates: two, in this order.**

1. already-own — `1000:c93c cmp byte [0x38bb],0x0` / `1000:c941 jnz 0xc992`,
   refusing with `^6У тебя уже есть мобила.` (CS `0x93d6`);
2. afford — `1000:c948` / `1000:c94c jle 0xc969`, refusing with
   `^4Нету денег` (CS `0x93b0`).

**Effect.** `1000:c969 mov byte [0x38bb],0x1`, then the debit at `1000:c973`.
Confirmation `^2Чё ты модный типа да?.` (CS `0x93bd`).

**Read where.** 14 references, 12 outside the arm: the sheet at `1000:1cd8`,
the in-combat backup countdown at `1000:4cdb`, and five wander sites
(`1000:af3d`, `1000:af7d`, `1000:afe3`, `1000:b022`, `1000:b0ce` —
`docs/re/wander.md`). The menu line's promise that backup arrives faster is
`1000:4cdb`.

---

## Row 3 — Офигенный косяк, 20 руб.

Span `1000:c9ab`..`1000:cac7`. Key compare `1000:c9b5`; the miss is an inverted
pair, `1000:c9ba jz 0xc9bf` over `1000:c9bc jmp 0xcac7`, because the arm is too
long for a short branch.

**Gates: one** — afford, `1000:c9c4` / `1000:c9c8 jle 0xc9e6`, refusing with
`^4Не хватает` (CS `0x8e4d`). No already-own test: **repeatable**, and each
purchase rolls again.

**Effect.** Debit at `1000:c9eb`, then `^2Пошли стероиды!` (CS `0x93f0`), then
a `Random(4)` at `1000:ca0c` (`mov ax,0x4` at `1000:ca08`) dispatched over four
compares — `1000:ca11`, `1000:ca53`, `1000:ca77`, `1000:caa5`:

| roll | writes | prints |
|---|---|---|
| 0 | `1000:ca16 inc [0x389e]` (Сила); `1000:ca33 inc [0x38aa]` (dmg max); `1000:ca45 inc [0x38a8]` (dmg min) **only when the new Сила is even**; `1000:ca49 inc [0x38ae]` (hp max); `1000:ca4d inc [0x38ac]` (hp) | `^1Сила +1 ` (CS `0x9402`) |
| 1 | `1000:ca58 inc [0x38a0]` (Ловкость) | `^1Ловкость +1 ` (CS `0x940d`) |
| 2 | `1000:ca7c inc [0x38a2]` (Живучесть); `1000:ca99 add word [0x38ae],0x5`; `1000:ca9e add word [0x38ac],0x5` | `^1Живучесть +1 ` (CS `0x941c`) |
| 3 | `1000:caaa inc [0x38a4]` (Удача) | `^1Удача +1 ` (CS `0x942c`) |

The even/odd split is `1000:ca37`..`1000:ca43` — `mov ax,[0x389e]` / `cwd` /
`mov cx,0x2` / `idiv cx` / `xchg ax,dx` / `or ax,ax` / `jnz 0xca49`, so the
`inc [0x38a8]` happens when the remainder is zero. It is the mirror image of
the in-combat stat-loss arm `docs/re/combat-dispatch.md` records at
`1000:498f`, which decrements the same three and takes the dmg-min half when
Сила is *odd*.

**Read where.** Every one of the eight globals is consumed elsewhere — the
sheet's stat line at `1000:1baa`, the damage computation inside the `k` blow
loop at `1000:448f` / `1000:4492` (`docs/re/combat.md`), the death test at
`1000:4f82`. It is the only one of the nine `bmar` rows that grants a stat
point — rows 7–9 are Task 18's and grant none. The `mar` arms were not decoded
here, so nothing is claimed about them.

---

## Row 4 — зоновскую наколку, 10 руб.

Span `1000:cac7`..`1000:cb47`. Key compare `1000:cad1`, miss
`1000:cad6 jnz 0xcb47`.

**Gates: two.** Already-own `1000:cad8 cmp byte [0x38bc],0x0` /
`1000:cadd jnz 0xcb2e`, refusing with
`^6Сделать, конечно, можно но толку не будет.` (CS `0x9446`); then afford
`1000:cae4` / `1000:cae8 jle 0xcb05`, refusing with `^4Нету денег`
(CS `0x93b0`) — the same literal row 2 uses.

**Effect.** `1000:cb05 mov byte [0x38bc],0x1`, debit at `1000:cb0f`,
confirmation `^2Чистый зек.` (CS `0x9438`).

**Read where.** Four references in the whole image, two outside the arm: the
sheet at `1000:1d18`, and `1000:b5da cmp byte [0x38bc],0x1` — the wander
mugging roll, which `docs/re/gaps.md` already records as halving the chance
when the flag is set. That single branch is this row's entire gameplay effect.
The menu line's `-50%` agrees, but the halving is established from `1000:b5da`;
the line is corroboration, not the evidence.

---

## Row 5 — Кастет, 25 руб.

Span `1000:cb47`..`1000:cc04`. Key compare `1000:cb51`; miss is again the
inverted pair `1000:cb56 jz 0xcb5b` over `1000:cb58 jmp 0xcc04`.

**Gates: three.**

1. **better-weapon**, a short-circuit conjunction:

```
1000:cb5b  cmp byte [0x394b],0x0
1000:cb60  jz 0xcb70
1000:cb62  cmp byte [0x38c2],0x0
1000:cb67  jz 0xcb70
1000:cb69  cmp byte [0x394c],0x0
1000:cb6e  jnz 0xcbeb
```

   It refuses only when the club **and** the knife **and** the cleaver are all
   owned — any one of them missing falls through to `1000:cb70` and the sale
   proceeds. Refusal:
   `^6Нафиг тебе он нужен, когда есть более мощное оружие.` (CS `0x94da`).
2. already-own — `1000:cb70 cmp byte [0x38ba],0x0` / `1000:cb75 jnz 0xcbd0`,
   refusing with `^6У тебя есть эта железка.` (CS `0x94bf`).
3. afford — `1000:cb7c` / `1000:cb80 jle 0xcb9d`, refusing with
   `^4Не хватает деньжат` (CS `0x9473`).

**Effect.**

```
1000:cb9d  mov byte [0x38ba],0x1
1000:cba7  sub [0x38c7],ax
1000:cbab  add word [0x38a8],0x2
1000:cbb0  add word [0x38aa],0x2
```

Confirmation `^2Ты купил кастет смотри чтоб менты с ним не запалили.`
(CS `0x9488`). The +2/+2 is unconditional here, matching the menu's `урон+2`.

**Read where.** 15 references, 13 outside the arm: the sheet's own line at
`1000:1eef`, the combat loot arm's split at `1000:55d3`, and — inside this same
handler — row 6's damage guard at `1000:cc64`.

---

## Row 6 — Дубинка, 50 руб.

Span `1000:cc04`..`1000:ccc4`. Key compare `1000:cc0e`; miss
`1000:cc13 jz 0xcc18` over `1000:cc15 jmp 0xccc4`.

**Gates: three.**

1. **better-weapon**, two conjuncts this time — `1000:cc18 cmp byte [0x38c2],0x0`
   / `1000:cc1d jz 0xcc29` and `1000:cc1f cmp byte [0x394c],0x0` /
   `1000:cc24 jz 0xcc29`, falling to `1000:cc26 jmp 0xccab` only when both are
   set. Refusal
   `^6Да нафиг она нужна, когда есть более мощное оружие.` (CS `0x957c`).
2. already-own — `1000:cc29 cmp byte [0x394b],0x0` / `1000:cc2e jnz 0xcc90`,
   refusing with `^6У тебя есть дубина.` (CS `0x9566`).
3. afford — `1000:cc35` / `1000:cc39 jle 0xcc56`, refusing with
   `^4Не хватает на дубинку деньжат` (CS `0x9511`).

**Effect.**

```
1000:cc56  mov byte [0x394b],0x1
1000:cc60  sub [0x38c7],ax
1000:cc64  cmp byte [0x38ba],0x0
1000:cc69  jz 0xcc75
1000:cc6b  add word [0x38a8],0x2
1000:cc70  add word [0x38aa],0x2
```

Confirmation `^2Ты купил дубинку - похоже задумал чё-то нехорошее.`
(CS `0x9531`), pushed at `1000:cc75` — which is exactly where `1000:cc69`
jumps.

**Read where.** 20 references, 18 outside the arm: the sheet's line at
`1000:1f59`, the loot arm's already-own guard at `1000:55a0`, and row 5's first
conjunct at `1000:cb5b`.

---

## Three original-behaviour findings — reproduce, do not fix

### 1. The club grants no damage at all without the knuckles

The menu advertises `урон+4`. `1000:cc69 jz 0xcc75` skips **both** adds when
`20ae:38ba` (the knuckles) is clear, and there is no other add on that path:
its target is the confirmation push. So buying the Дубинка *first* costs 50
руб., sets the flag, prints the confirmation and changes the damage range by
nothing.

The counter-example is in the same binary, granting the same item. The combat
loot arm tests the same flag and has both halves:

```
1000:55d3  cmp byte [0x38ba],0x0
1000:55d8  jz 0x55e6
1000:55da  add word [0x38a8],0x2
1000:55df  add word [0x38aa],0x2
...
1000:55e6  add word [0x38a8],0x4
1000:55eb  add word [0x38aa],0x4
```

The shop arm is the loot arm with the `+4` branch missing. Reading it as
"replaces the knuckles" (the menu line says `заменяет кастет`) explains the
`+2`, but not the absence of the `+4`.

### 2. The shop's better-weapon gate is an AND where the loot's is an OR

Rows 5 and 6 refuse only when **every** flag they name is set (`1000:cb5b`,
`1000:cb62`, `1000:cb69`; `1000:cc18`, `1000:cc1f` — all but the last conjunct
branch *past* the refusal). The combat loot arms that grant the same two items
refuse when **any** one is set: every conjunct at `1000:555f`, `1000:5566`,
`1000:556d` (knuckles) and `1000:55c5`, `1000:55cc` (club) is a
`jnz <refusal>`. So a player holding a knife can still buy the knuckles at the
dealers but cannot loot them, and the two paths disagree by construction.

### 3. The dealers' buy path carries no district test at all

Not for rows 5 and 6, and not for rows 7, 8 and 9 either — measured over
`1000:c8ce`..`1000:ccc4` and `1000:ccc4`..`1000:ce80` respectively. Every
district-gated `bmar` row is buyable below the district its menu line is gated
on. See "The district question" above.

---

## What the port had to change, and what Task 24 changed

**Task 24 did all five of the following, and this section is now a record of
what it changed, not a list of outstanding work.** Read it with
`Game::shop_action`, `Game::listed_rows` and `Game::buy_dealer_row` in
`src/game.rs` (`grep -n 'fn buy_dealer_row' src/game.rs` finds the
definition; `grep -n 'buy_dealer_row(' src/game.rs` finds it plus its call
site and the one in the test that guards it). `Game::buy_pistol_row` is
the name that function carried before it grew rows 1–6; nothing answers to it
now. The generic path used to debit the price, echo the *menu* line, and
refuse a district-gated row. For `bmar` 1..6 that was wrong in five ways:

1. **Stop refusing on district anywhere on the dealers' BUY path — all five
   gated rows, not two.** `Game::shop_action` *used to* call
   `gate_open(row.gate)` before it delegated, so rows 7, 8 and 9 were refused
   below district 4 as well, and their arms carry no district test either
   (`1000:ccc4`..`1000:ce80`, no `[0x3692]` operand and no `92 36` byte
   pair). The menu had to KEEP its gate — all five of `1000:c68d`,
   `1000:c6f1`, `1000:c755`, `1000:c7ba` and `1000:c81d` are real — so the two
   uses of the district were separated rather than the gate deleted:
   `Game::listed_rows` is the menu's filter and the buy path never consults
   it.
2. **Print the arm's own confirmation, not the menu line.** Six confirmations
   plus row 3's four stat lines; every CS offset is in the table above and in
   `data/shop_arms.json`. All ten are now printed by their own arm.
3. **Print the arm's own refusal.** Rows 2, 4, 5 and 6 needed an already-own
   refusal; rows 5 and 6 the better-weapon refusal; every row's out-of-money
   wording differs from the generic `^4Чёрт, бабок не хватает.` except
   row 1's, which is where that literal comes from — the generic path no
   longer prints it, row 1's arm does. No gate in rows 1–6 is silent — unlike
   row 9's first two.
4. **Apply the effects.** Joint count (`20ae:38c5`, a word), mobile
   (`20ae:38bb`), the four-way stat roll, tattoo (`20ae:38bc`), knuckles
   (`20ae:38ba`, +2/+2) and club (`20ae:394b`, +2/+2 **only** when the
   knuckles are owned). Every one of these is read elsewhere in the original,
   so none of them is a write-only flag, and every one is now applied.
5. **Reproduce findings 1 and 2 above** — the club's missing `+4` and the
   AND/OR mismatch. Both are reproduced, cited in the arm, asserted by a test
   and recorded in `docs/re/gaps.md` rather than fixed.

Row 3 is the only one of the six that draws: `1000:ca0c` is a `Random(4)`
(`python3 tools/re_query.py pushed-n 1000:ca0c` recovers the `n` from the
idiom before it), so buying it advances the RNG stream and any trace the port
compares against has to account for that.

Rows 1 and 3 are repeatable and stay repeatable; rows 2, 4, 5 and 6 are
one-shot through their own already-own test, not through any menu state.

---
---

# `mar` rows 1–9: the purchase arms

**Scope.** What the market's handler does *after* a row key is typed at the
`^0Базар\` prompt, for all nine menu rows. The menu half — which rows print, at
what price — is `docs/re/tables.md` §2 and `data/shops.json`, and is not
restated here; the one part of the menu block this section does measure is
*which* menu lines each district gate covers, because the row-7 finding below
is the difference between that and the buy path. The market pickpocket block
that follows row 9 (`1000:c353`..`1000:c369`, `docs/re/gaps.md`) is **out of
scope** and is not re-derived here.

**Machine-readable twin:** the `mar` key of `data/shop_arms.json`.
**Re-derivation:** the same `python3 tools/test_shop_arms.py`.

Every claim below is **established from flow** unless it says otherwise. The
range decoded is `1000:bd48`..`1000:c31f` — the row-1 setup to the setup that
follows row 9 — as one aligned run of 696 instructions. Its alignment is not
assumed: every address here is an instruction boundary an aligned walk from
`entry` reaches.

---

## The shape all nine share, and the one way three of them differ

One line is read into the buffer at `20ae:3a72` by the `ReadLn` at
`1000:bd43`, after the prompt `^0Базар\` (CS `0x8dc1`) is printed at
`1000:bd08`. Then each row compares that one buffer against its own
one-character literal with Borland's shortstring compare `0f78:0bd8`, using
the same rigid six-instruction push idiom `bmar` uses, and each row's miss
branch targets the next row's span start. Row 9's miss lands on `1000:c31f`,
whose own compare at `1000:c329` is the pickpocket verb — that is what bounds
this range on the right without assuming it.

| row | key compare | key literal | price byte | debit | district gate |
|---|---|---|---|---|---|
| 1 Хотдог | `1000:bd52` | CS `0x8dca` | `20ae:0b2e` = 2 | `1000:bdb3` | — |
| 2 Пиво | `1000:be14` | CS `0x8e4b` | `20ae:0b2f` = 5 | `1000:be49` | — |
| 3 Затемнённые очки | `1000:bec2` | CS `0x8ea5` | `20ae:0b30` = 10 | `1000:bf00` | — |
| 4 abibas | `1000:bf42` | CS `0x8ef7` | `20ae:0b31` = 15 | `1000:bf8a` | — |
| 5 Понтовые бутсы | `1000:bfeb` | CS `0x8f6b` | `20ae:0b32` = 15 | `1000:c033` | — |
| 6 Реальную кожанку | `1000:c0a2` | CS `0x8fc6` | `20ae:0b33` = 25 | `1000:c0ea` | `1000:c08e` |
| 7 adidas | `1000:c14c` | CS `0x9023` | `20ae:0b34` = 30 | `1000:c18d` | — |
| 8 Понтовёйшие бутсы | `1000:c1eb` | CS `0x9055` | `20ae:0b35` = 30 | `1000:c22c` | `1000:c1d7` |
| 9 Ваще крутую кожанку | `1000:c293` | CS `0x906a` | `20ae:0b36` = 50 | `1000:c2d4` | `1000:c27f` |

The three rows with a district gate have it in front of their *setup*, not
inside the arm, so their span starts at the gate: `1000:c08e`, `1000:c1d7` and
`1000:c27f` are the span starts of rows 6, 8 and 9. The other six spans start
at their `mov di,0x3a72`: `1000:bd48`, `1000:be0a`, `1000:beb8`, `1000:bf38`,
`1000:bfe1` and `1000:c142`.

**The affordability sense is the same in all nine, and it was checked per row
rather than assumed.** Each is `mov al,[price]` / `xor ah,ah` /
`cmp ax,[0x38c7]` / `jle <buy>` — the branch byte is `7e` at `1000:bd91`,
`1000:be27`, `1000:bed9`, `1000:bf63`, `1000:c00c`, `1000:c0c3`, `1000:c166`,
`1000:c205` and `1000:c2ad` — so the purchase goes through when
`price <= money`, and the *refusal* is the fall-through.

**No `mar` row has the silencer's price bug.** For every row the byte the
affordability test reads and the byte the debit subtracts are the same
address, and it is the address `docs/re/tables.md` §2 already records.

---

## The district question — answered, and it is the opposite of `bmar`

**Three `mar` arms DO test the district byte `20ae:3692`, and `bmar`'s do
not.** The whole `1000:bd48`..`1000:c31f` run carries exactly three
direct-memory operands equal to `0x3692` — `1000:c08e`, `1000:c1d7` and
`1000:c27f` — and the artifact asserts that set equal to a sweep of the run,
with a raw `92 36` byte scan over the same range counted beside it so the
result does not rest on the decoder alone. Task 23's finding was about `bmar`
and established nothing here; this one was measured over its own range.

| gate | branch | skip | reaches |
|---|---|---|---|
| `1000:c08e cmp byte [0x3692],0x1` | `1000:c093 ja 0xc098` | `1000:c095 jmp 0xc142` | row 6's setup at `1000:c098` |
| `1000:c1d7 cmp byte [0x3692],0x2` | `1000:c1dc ja 0xc1e1` | `1000:c1de jmp 0xc27f` | row 8's setup at `1000:c1e1` |
| `1000:c27f cmp byte [0x3692],0x3` | `1000:c284 ja 0xc289` | `1000:c286 jmp 0xc31f` | row 9's setup at `1000:c289` |

**Each of the three prints nothing.** The gate sits ahead of the key compare,
so below the district the compare never runs: the skip jumps straight to the
row's own span end, which is the next row's span start, and the three
instructions between the gate and the setup are the `cmp`, the `ja` and the
`jmp` and nothing else. So at district 1, typing `6` at the market prints no
*message* — not a refusal, not an echo: the line falls through every remaining
compare to `1000:c47b jmp 0xbd08`, the handler's own re-prompt, exactly as an
unrecognised key does. These are the only silent *district* tests in either
handler; `bmar` row 9's first two gates are silent as well (Task 18's, not
mapped here).

### Row 7 is hidden from the menu and sold anyway

**The menu's gate at `1000:bb80` covers two rows; the buy path's covers one.**
The menu block is not re-derived here, but which lines a gate covers is: the
price bytes an aligned decode of `1000:bb8a`..`1000:bc42` loads are
`20ae:0b33` (at `1000:bb8a`) and `20ae:0b34` (at `1000:bbe6`), rows 6 and 7.
The other two menu gates cover one row each — `1000:bc42 cmp byte [0x3692],0x2`
with `1000:bc47 jbe 0xbca5` over row 8's `20ae:0b35` at `1000:bc49`, and
`1000:bca5 cmp byte [0x3692],0x3` with `1000:bcaa jbe 0xbd08` over row 9's
`20ae:0b36` at `1000:bcac`. That agrees with `data/shops.json`, which records
`district>1` for rows 6 *and* 7, and the test asserts the agreement rather
than assuming it.

The buy path has no gate in front of row 7. Row 7's span starts at
`1000:c142`, which is exactly where row 6's gate jumps when it skips
(`1000:c095 jmp 0xc142`), and `1000:c142` is the `mov di,0x3a72` setup itself:
nothing stands between the skip and the key compare at `1000:c14c`.

**So at district 1 the market lists rows 1–5, typing `6` falls through to the
re-prompt without a word, and typing `7` buys the adidas suit for 30 руб. and
applies its armour.** Rows
8 and 9 have matching menu and buy gates and are genuinely unreachable below
districts 3 and 4. The difference between the two measured sets is exactly
`{7}`, and the test asserts that — not that row 7 is in one of them.

---

## Row 1 — Хотдог, 2 руб.

Span `1000:bd48`..`1000:be0a`. Key compare `1000:bd52`; the miss is an
inverted pair, `1000:bd57 jz 0xbd5c` over `1000:bd59 jmp 0xbe0a`.

**Gates: three, in this order.**

1. **broken jaw** — `1000:bd5c cmp byte [0x38b0],0x1` / `1000:bd61 jnz 0xbd7f`.
   The branch jumps *past* the refusal, so this one refuses on the
   fall-through, unlike every `bmar` own-gate. Refusal
   `^4Ты не можешь хавать из-за сломаной челюсти.` (CS `0x8dcc`).
2. **already healthy** — `1000:bd7f mov ax,[0x38ac]` /
   `1000:bd82 cmp ax,[0x38ae]` / `1000:bd86 jnl 0xbdf1`, refusing with
   `^6Да неохота хавать` (CS `0x8e37`) when hp is already at hp max.
3. **afford** — `1000:bd91 jle 0xbdae`, refusing with
   `^4Чёрт, бабок даже на жратву не хватает.` (CS `0x8dfa`). That is the
   literal `Game::shop_action`'s generic path prints today.

**Effect.** Debit at `1000:bdb3`, then a `Random(2)` at `1000:bdbb`
(`mov ax,0x2` at `1000:bdb7`, recovered with
`python3 tools/re_query.py pushed-n 1000:bdbb`), then
`1000:bdc0 add ax,0x3` and `1000:bdc3 add [0x38ac],ax` — so the hot dog heals
**3 or 4**, and the run between the draw and the add is that one `add` and
nothing else. The clamp is `1000:bdce jle 0xbdd6` over
`1000:bdd3 mov [0x38ac],ax`, which writes hp max back into hp when the heal
overshot. Confirmation `^2Ты сожрал хот-дог` (CS `0x8e23`).

There is no already-own test: the row is **repeatable**, and each purchase
draws again.

**Read where.** `20ae:38ac` has 74 operand-field references image-wide, 70 of
them outside this arm — the death test at `1000:4f82` and the sheet's hp bar
at `1000:210c` among them.

---

## Row 2 — Пиво, 5 руб.

Span `1000:be0a`..`1000:beb8`. Key compare `1000:be14`, miss
`1000:be19 jz 0xbe1e` over `1000:be1b jmp 0xbeb8`.

**Gates: one** — afford, `1000:be27 jle 0xbe44`, refusing with
`^4Не хватает` (CS `0x8e4d`). **Repeatable.**

**Effect.** Debit at `1000:be49`, then a `Random(3)` at `1000:be51`
(`mov ax,0x3` at `1000:be4d`) dispatched over three compares — `1000:be56`,
`1000:be76`, `1000:be96` — that print `^2Глинское? Чё за нафиг? А ладно.`
(CS `0x8e5a`), `^2Пивко. Холодненькое.` (CS `0x8e7c`) and
`^2Ну чё по пиву?.` (CS `0x8e93`) and change no state at all. All three
converge on `1000:beb4 inc [0x38c3]`, the beer counter.

**The draw is cosmetic but it is still a draw**, so buying beer advances the
RNG stream and any trace the port compares against has to account for that.

**The counter is incremented after the confirmation print**, as the
equipment rows' bonuses are, so the refusal path was checked rather than
assumed: `1000:be42 jmp short 0xbeb8` lands on the span end, past
`1000:beb4`, so a failed purchase adds no beer.

**Read where.** `20ae:38c3` is a word count in half-litres with 19 references,
18 outside the arm: the sheet at `1000:23d5`, and the drink handler's
`1000:2a47` guard and `1000:2a51 dec [0x38c3]` inside `FUN_1000_29c4`. So the
market is where that counter comes from, and it is the same global
`src/model.rs` already carries.

---

## Row 3 — Затемнённые очки, 10 руб.

Span `1000:beb8`..`1000:bf38`. Key compare `1000:bec2`, miss
`1000:bec7 jnz 0xbf38`.

**Gates: two.** Already-own `1000:bec9 cmp byte [0x38b3],0x0` /
`1000:bece jnz 0xbf1f`, refusing with `^6У тебя есть очки от солнца.`
(CS `0x8ed9`); then afford `1000:bed9 jle 0xbef6`, refusing with
`^4Не хватает бабок` (CS `0x8ea7`).

**Effect.** `1000:bef6 mov byte [0x38b3],0x1`, **then** the debit at
`1000:bf00` — the flag before the money, the same order seven of the nine
`mar` rows use. Confirmation `^2Модные такие очки от солнца.` (CS `0x8eba`).

**Read where.** Six references, four outside the arm: the sheet at
`1000:1cf8`, the wander cop encounter at `1000:b7c6 cmp byte [0x38b3],0x1`
(`docs/re/gaps.md` records that the glasses avoid the fight), and the combat
loot arm at `1000:5621`, which grants the same flag at
`1000:5628 mov byte [0x38b3],0x1`.

---

## Row 4 — abibas, 15 руб.

Span `1000:bf38`..`1000:bfe1`. Key compare `1000:bf42`, miss
`1000:bf47 jz 0xbf4c` over `1000:bf49 jmp 0xbfe1`.

**Gates: three.**

1. **better item** — `1000:bf4c cmp byte [0x38b7],0x0` /
   `1000:bf51 jnz 0xbfc8`, refusing with
   `^6У тебя есть более крутой костюм.` (CS `0x8f48`) when the adidas suit is
   already owned. **One test, not a conjunction** — unlike `bmar` rows 5
   and 6, where three flags and two flags are ANDed.
2. already-own — `1000:bf53 cmp byte [0x38b4],0x0` / `1000:bf58 jnz 0xbfad`,
   refusing with `^6У тебя уже есть костюм.` (CS `0x8f2e`).
3. afford — `1000:bf63 jle 0xbf80`, refusing with `^4Не хватает денег`
   (CS `0x8ef9`).

**Effect.** `1000:bf80 mov byte [0x38b4],0x1`, debit at `1000:bf8a`,
confirmation `^2Теперь ты больше похож на гопа.` (CS `0x8f0c`), then
`1000:bfa7 inc [0x38b2]` — the armour byte, **+1**, a byte `inc`. The menu
line's `Смягчает пинок на 1` agrees, but the `+1` is established from
`1000:bfa7`; the line is corroboration.

**Read where.** `20ae:38b4` has eight references, six outside the arm: the
sheet at `1000:22a1`, the gym's armour recompute at `1000:e3aa`, and row 7's
own upgrade guard at `1000:c1aa`. `20ae:38b2` has twelve, eleven outside: the
sheet's armour line at `1000:227b`, the kick's damage reduction at
`1000:4769`, and the gym's `1000:e3a4`.

---

## Row 5 — Понтовые бутсы, 15 руб.

Span `1000:bfe1`..`1000:c08e`. Key compare `1000:bfeb`, miss
`1000:bff0 jz 0xbff5` over `1000:bff2 jmp 0xc08e` — and `1000:c08e` is row 6's
district gate, not its setup.

**Gates: three.** Better item `1000:bff5 cmp byte [0x38b8],0x0` /
`1000:bffa jnz 0xc075`, refusing with `^6У тебя бутсы по круче.`
(CS `0x8fad`); already-own `1000:bffc cmp byte [0x38b5],0x0` /
`1000:c001 jnz 0xc05a`, refusing with `^6У тебя такие уже есть.`
(CS `0x8f94`); afford `1000:c00c jle 0xc029`, refusing with
`^4Нету на них денег` (CS `0x8f6d`).

**Effect.** `1000:c029 mov byte [0x38b5],0x1`, debit at `1000:c033`,
confirmation `^2Зацени красовки.` (CS `0x8f81`), then
`1000:c050 inc [0x38a8]` and `1000:c054 inc [0x38aa]` — **the damage range,
+1/+1, unconditionally.** The menu says only `Увеличивают урон`; the number
comes from those two instructions.

**Read where.** `20ae:38b5` has eight references, six outside: the sheet at
`1000:1e81` and row 8's upgrade guard at `1000:c249`. The damage words are the
same `20ae:38a8` / `20ae:38aa` the `k` blow loop reads at `1000:4492` and
`1000:448f`.

---

## Row 6 — Реальную кожанку, 25 руб.

Span `1000:c08e`..`1000:c142`, which **starts at the district gate**. Key
compare `1000:c0a2`, miss `1000:c0a7 jz 0xc0ac` over `1000:c0a9 jmp 0xc142`.

**Gates: four, counting the silent one.** The district gate
`1000:c08e cmp byte [0x3692],0x1` first; then better item
`1000:c0ac cmp byte [0x38b9],0x0` / `1000:c0b1 jnz 0xc129`, refusing with
`^6Утебя есть кожанка круче.` (CS `0x9007`) — the missing space is the
original's; then already-own `1000:c0b3 cmp byte [0x38b6],0x0` /
`1000:c0b8 jnz 0xc10e`, refusing with `^6Ты уже купил это.` (CS `0x8ff3`);
then afford `1000:c0c3 jle 0xc0e0`, refusing with `^4Не достаточно бабла`
(CS `0x8fc8`).

**Effect.** `1000:c0e0 mov byte [0x38b6],0x1`, debit at `1000:c0ea`,
confirmation `^2Ну весь на понтах.` (CS `0x8fde`), then
`1000:c107 add byte [0x38b2],0x2` — armour **+2**, unconditional.

**Read where.** `20ae:38b6` has eight references, six outside: the sheet at
`1000:2323`, the gym at `1000:e3c8`, and row 9's upgrade guard at
`1000:c2f1`.

---

## Row 7 — adidas, 30 руб.

Span `1000:c142`..`1000:c1d7`. Key compare `1000:c14c`, miss
`1000:c151 jz 0xc156` over `1000:c153 jmp 0xc1d7` — and `1000:c1d7` is row 8's
district gate.

**Gates: two, and no district test.** Already-own
`1000:c156 cmp byte [0x38b7],0x0` / `1000:c15b jnz 0xc1be`, refusing with
`^6У тебя уже есть этот костюм.` (CS `0x9036`); afford
`1000:c166 jle 0xc183`, refusing with `^4Не хватает денег` (CS `0x8ef9`) —
the same literal row 4 uses. **There is no better-item gate** — the sweep of
this span finds four conditional branches and those two gates, the miss and
the upgrade guard below account for all of them.

**Effect.** `1000:c183 mov byte [0x38b7],0x1`, debit at `1000:c18d`,
confirmation `^2Чистый гопник.` (CS `0x9025`), then the **upgrade split**:

```
1000:c1aa  cmp byte [0x38b4],0x0
1000:c1af  jz 0xc1b7
1000:c1b1  inc [0x38b2]
1000:c1b5  jmp short 0xc1bc
1000:c1b7  add byte [0x38b2],0x2
```

So it grants armour **+1 when the abibas suit is already owned** and **+2 when
it is not**. Either way the player ends on +2 of suit armour, whichever order
the two rows were bought in — which is why the gym's recompute subtracts 1 for
`20ae:38b4` alone and 2 for `20ae:38b7` (`docs/re/gaps.md`; that table is
corroboration here, not part of this decode).

**Read where.** `20ae:38b7` has eight references, six outside: the sheet at
`1000:22fc`, the gym at `1000:e3bc`, and row 4's better-item gate at
`1000:bf4c`.

---

## Row 8 — Понтовёйшие бутсы, 30 руб.

Span `1000:c1d7`..`1000:c27f`, starting at the district gate
`1000:c1d7 cmp byte [0x3692],0x2`. Key compare `1000:c1eb`, miss
`1000:c1f0 jz 0xc1f5` over `1000:c1f2 jmp 0xc27f`.

**Gates: three, counting the silent one.** Already-own
`1000:c1f5 cmp byte [0x38b8],0x0` / `1000:c1fa jnz 0xc266`, refusing with
`^6У тебя такие уже есть.` (CS `0x8f94`) — row 5's literal; afford
`1000:c205 jle 0xc222`, refusing with `^4Нету на них денег` (CS `0x8f6d`) —
also row 5's. No better-item gate.

**Effect.** `1000:c222 mov byte [0x38b8],0x1`, debit at `1000:c22c`,
confirmation `^2Офигенные бутцы.` (CS `0x9057`), then the same upgrade split
on the damage range:

```
1000:c249  cmp byte [0x38b5],0x0
1000:c24e  jz 0xc25a
1000:c250  inc [0x38a8]
1000:c254  inc [0x38aa]
1000:c258  jmp short 0xc264
1000:c25a  add word [0x38a8],0x2
1000:c25f  add word [0x38aa],0x2
```

**+1/+1 with the lesser boots already owned, +2/+2 without** — matching the
menu's `Урон+2` as a total, not as this arm's own add.

**Read where.** `20ae:38b8` has eight references, six outside: the sheet at
`1000:1ecf` and row 5's better-item gate at `1000:bff5`.

---

## Row 9 — Ваще крутую кожанку, 50 руб.

Span `1000:c27f`..`1000:c31f`, starting at the district gate
`1000:c27f cmp byte [0x3692],0x3`. Key compare `1000:c293`, miss
`1000:c298 jz 0xc29d` over `1000:c29a jmp 0xc31f`.

**Gates: three, counting the silent one.** Already-own
`1000:c29d cmp byte [0x38b9],0x0` / `1000:c2a2 jnz 0xc306`, refusing with
`^6Ты уже купил это.` (CS `0x8ff3`) — row 6's literal; afford
`1000:c2ad jle 0xc2ca`, refusing with `^4Не достаточно бабла` (CS `0x8fc8`) —
also row 6's.

**Effect.** `1000:c2ca mov byte [0x38b9],0x1`, debit at `1000:c2d4`,
confirmation `^2Ну крутой, сдохнуть можно!` (CS `0x906c`), then

```
1000:c2f1  cmp byte [0x38b6],0x0
1000:c2f6  jz 0xc2ff
1000:c2f8  add byte [0x38b2],0x2
1000:c2fd  jmp short 0xc304
1000:c2ff  add byte [0x38b2],0x4
```

**+2 with the lesser jacket already owned, +4 without.**

**Read where.** `20ae:38b9` has eight references, six outside: the sheet at
`1000:237e`, the gym at `1000:e3db`, and row 6's better-item gate at
`1000:c0ac`.

---

## Findings — reproduce, do not fix

### 1. Row 7 is menu-gated but not buy-gated

See "Row 7 is hidden from the menu and sold anyway" above. `1000:bb80` hides
rows 6 and 7 below district 2; `1000:c08e` gates only row 6's arm; row 7's
setup at `1000:c142` is reached with no district test.

### 2. The three buy-path district gates say nothing

They do not refuse — they skip. `1000:c095`, `1000:c1de` and `1000:c286` jump
straight past their row with nothing printed. `bmar` has one comparable shape — row 9's first two gates,
which the port already reproduces — but `mar` is the only handler where a
*district* test is silent.

### 3. The upgrade rows grant the delta, not the bonus

Rows 7, 8 and 9 each read the lesser item's ownership flag and add only the
difference. Not a bug — the totals come out the same in either purchase order,
and they match what the gym's recompute subtracts (`docs/re/gaps.md`'s table;
corroboration, not part of this decode) — but reproducing it needs the guard,
not just the add: an arm that always applied the full bonus would double-count
the upgrade.

### 4. No global written by a `mar` arm is write-only

Every one of `20ae:38ac`, `20ae:38c3`, `20ae:38b2`, `20ae:38b3`, `20ae:38b4`,
`20ae:38b5`, `20ae:38b6`, `20ae:38b7`, `20ae:38b8`, `20ae:38b9`, `20ae:38a8`
and `20ae:38aa` is read somewhere outside its arm, and the artifact records
the counts. So there is no flag here the port can set and forget.

---

## What the port had to change — done by Task 26

**This section was written as a directive, before any of it was done. All six
items are now landed**; each is annotated below with where it lives in `src/`.
The nine `mar` arms are `Game::buy_market_row`
(`grep -n 'fn buy_market_row' src/game.rs`), built on the same
`Game::buy_after_gates` the dealers' nine use, and `Game::shop_action` no
longer has a generic "debit and echo the menu line" path at all — the original
has none, and that echo was the port's own invention.

The six items, as written then:

1. **Give each row its own arm**, the way `Game::buy_dealer_row` gives the
   dealers theirs — gates in image order (better-item, then already-own, then
   afford), the row's own refusal per gate, the row's own confirmation.
2. **Keep the district gate on the buy path for rows 6, 8 and 9 — and drop it
   for row 7.** `Game::gate_open` is currently consulted for every gated `mar`
   row, so row 7 is refused at district 1 where `1000:c142` sells it. This is
   the reverse of the `bmar` fix: there the gate had to come off the buy path
   entirely, here it has to stay on for three rows and come off for one.
3. **Keep the silence when a gated row is out of reach.** Rows 6, 8 and 9
   below their district print *nothing* — not a refusal, not the menu echo —
   and `Game::shop_action` already returns without printing when
   `Game::gate_open` is false, so that half already matches. Whatever replaces
   the generic path has to keep it: a gated row must not be routed into a
   refusal line.
4. **Apply the effects**, and in the original's order: rows 3–9 set their
   ownership flag *before* the debit, rows 1 and 2 have no flag. Hot dog:
   hp `+= 3 + Random(2)`, clamped to hp max. Beer: `20ae:38c3` `+= 1` after a
   cosmetic `Random(3)`. Glasses, suits, boots and jackets: their flag, plus
   armour or damage.
5. **Draw the two `Random`s, in the right places.** `1000:bdbb` is a
   `Random(2)` and `1000:be51` a `Random(3)`, and the second changes no state
   at all — so a port that skips it because "nothing depends on the result"
   desynchronises every RNG trace after the first beer.
6. **Reproduce the upgrade splits**, not the advertised bonuses: `1000:c1af`,
   `1000:c24e` and `1000:c2f6`. Applying `+2`, `+2/+2` and `+4`
   unconditionally would over-grant whenever the lesser item is already owned.

Closing 4 also closes the divergence `docs/re/gaps.md` records under "The four
armour flags are carried but the gym's `abs` ignores them": until a `mar`
purchase can set `20ae:38b4`, `20ae:38b6`, `20ae:38b7` and `20ae:38b9`, only a
loaded `.SAV` can reach it.

### How each landed, and what was observed red

| # | where in `src/` | falsified by |
|---|---|---|
| 1 | `Game::buy_market_row`, nine arms over `Game::buy_after_gates` | `every_market_row_has_an_arm_of_its_own` |
| 2 | the three `below_district` gates in rows 6, 8 and 9; row 7 has none, and no `mar` arm reads `row.gate` | `the_market_sells_row_7_off_a_menu_that_never_listed_it` — observed red against a tree with a `district <= 1` gate added to row 7 (2 tests red) |
| 3 | those three gates are `(refuse, None)`, the silent form `bmar` row 9 already used | same test: at district 1, typing `6` moves neither money nor flag |
| 4 | each arm's effect closure | one `#[test]` per row pair, asserting the changed NUMBER |
| 5 | `rng.below_at("1000:bdbb", 2)` and `rng.below_at("1000:be51", 3)` | `the_market_beer_counts_up_and_draws_a_die_that_changes_nothing` — observed red against a tree with the cosmetic draw replaced by a constant |
| 6 | `if has_abibas { 1 } else { 2 }`, `if has_boots { 1 } else { 2 }`, `if has_jacket { 2 } else { 4 }` | the three `..._in_either_purchase_order` tests — observed red against a tree applying the full bonus unconditionally (3 tests red) |

Two more mutations were run and both went red: deleting the hp clamp at
`1000:bdd3` (1 test), and deleting the district filter from
`Game::listed_rows`, the MENU half (2 tests, one of them the dealers'). The
menu assertion goes through `Game::listed_rows` itself rather than a copy of
the predicate, which is what makes the second of those falsifiable — the
mistake Task 24's review caught.

**What `cargo test` cannot check here.** `crate::term` has no capture hook, so
no test transcribes a refusal literal or observes the ORDER the gates are
tested in. Every refusal assertion above is "the money did not move and the
flag did not change", which a wrong-but-refusing arm would also satisfy. The
literals themselves are checked only against `data/strings.json` by
`tools/test_shop_arms.py`, off the binary, not against what the port prints.

### One thing found while doing it, outside the nine arms

`bmar` row 7's menu line carries **three** `#` placeholders, not one:
`1000:c7a1 mov al,[0xb3e]` pushes the price, then `1000:c7a7 mov ax,0x14` and
`1000:c7ab mov ax,0x1e` push 20 and 30, so the line reads `урон(20-30)`. The
port was printing bare `#`s there, the same defect as `mar` row 2's `Пиво(#з)`
that this task was sent to fix; it is fixed in the same function
(`Game::row_fill_values`). **The 30 is the original's own off-by-one and is
reproduced, not corrected**: the shot the row advertises rolls
`20 + Random(10)` (`1000:4f14 mov ax,0xa`, `1000:4f1d add ax,0x14`), so its
real range is 20..=29 while its menu line says 20-30.
