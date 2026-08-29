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
