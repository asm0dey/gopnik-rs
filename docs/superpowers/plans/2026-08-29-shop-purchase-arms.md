# Shop purchase arms — the largest untouched block in `entry`

**Spec authority:** `docs/re/METHODOLOGY.md` (binding evidence standard) and
the Global Constraints of `docs/superpowers/plans/2026-08-17-gopnik-rust-port.md`,
which this plan inherits verbatim and does not restate.

**Branch:** `main`, by explicit owner instruction ("develop on main").

## Why this slice, with the number

`data/branches.json`'s `port_touched` — a game branch counts when its own
address or its guard's appears as a `SEG:OFF` citation in `src/**/*.rs` or
`data/command_dispatch.json` — stands at **305 / 838** at `bfad0b4`.

Per-function, the untouched population is dominated by one function:

| func entry | total | touched | untouched |
|---|---:|---:|---:|
| `1000:ab59` (`entry`, main loop) | 406 | 107 | **299** |
| `1000:3d11` (combat) | 224 | 94 | 130 |
| `1000:1a03` (character sheet) | 83 | 54 | 29 |

Inside `entry`, the two largest *contiguous* untouched runs are:

| run | branches | falls inside |
|---|---:|---|
| `1000:c526` .. `1000:ccd3` | **44** | the `bmar` handler (`c4be` .. `d3a6`) |
| `1000:bcb5` .. `1000:c32e` | **42** | the `mar` handler (`b94a` .. `c4be`) |

Both bounds come from `data/command_dispatch.json`'s `confirmed_dispatch_chain`
(`mar` compare `1000:b94a`, `bmar` compare `1000:c4be`, `rep` compare
`1000:d3a6`). `1000:ccd8` — the first of the three pistol rows Task 18 ported —
is the first address *after* the `c526..ccd3` run, which independently confirms
the run is exactly the dealer rows Task 18 left alone.

Those 86 branches are 10.3% of the whole game-branch population and are the
single largest coverage win available. They are also a real, player-visible
behaviour gap, already on the remaining-work list: `docs/re/gaps.md`, **"Shop
purchase effects — open for every row except three"** — 15 of the 18 shop rows
deduct their price and echo their *menu* line, apply no effect, and refuse a
district-gated row the original would sell.

Recompute the table above at any commit with the block in `docs/re/branches.md`
under *Recomputation, from the shipped artifacts → Coverage*.

## What moves the metric

`port_touched` reads `src/**/*.rs` and `data/command_dispatch.json` — **it does
not read `docs/`**. An RE task that only writes `docs/re/` moves it by zero
(Tasks 16 and 17 documented +103 branches and moved `src/` behaviour by nothing;
see `CLAUDE.md`, *Priority*). So each RE task here is immediately consumed by a
porting task that cites the same addresses in `src/` doc comments **and** makes
the arm do what the original does.

A `src/` diff that is only comments does not close a porting task.

## Task-local constraints

These bind every task below, in addition to the inherited Global Constraints.

- **Every claim states its evidence tier and cites an address.** Decode the
  bytes; `python3 tools/re_query.py resolve <citation>` runs the address
  arithmetic. A citation found by grepping for a literal is unverified.
- **Never regenerate the five oracles under `data/`.**
- **State the runner with every test number.** `python3 -m unittest discover -s
  tools -p 'test_*.py'` and `.venv/bin/pytest tools/ -q` disagree by design and
  both are right. `cargo test` for Rust.
- **Where the original has a bug, reproduce it and write it down** — the
  silencer's 60-charged/70-printed split (`docs/re/tables.md`) is the shape.
  Divergences this port keeps go in `docs/re/gaps.md`.
- **Do not fix, rename or re-scope anything outside the task's stated range.**

---

### Task 23 (RE): map the `bmar` rows 1–6 purchase arms

**Range:** `1000:c4be` .. `1000:ccd8` — from the `bmar` verb compare to the
start of row 7's arm, which Task 18 already mapped and ported.

Rows 1–6 are Косяк (15), Краденый мобильник (30), Офигенный косяк (20),
зоновская наколка (10), Кастет (25), Дубинка (50) — `data/shops.json`, and the
menu-row table in `docs/re/tables.md` §2.

Recover, **from flow**, for each of rows 1..6:

1. The address of the row-key compare that selects the arm (row 7's is
   `1000:ccd8`; the six below it follow the same chain).
2. Every gate the arm tests before it charges — the "already own it" test, the
   affordability test (`cmp ax,[0x38c7]` / `jle` for rows 7–9; confirm the sense
   for each of 1–6 rather than assuming it), and any prerequisite-item test.
   **Whether any arm tests the district at all** is a specific open question:
   `docs/re/gaps.md` records that `1000:cc04`..`1000:ccd8` carry no district
   test, so the port's blanket refusal of a gated row may be a divergence.
3. The refusal string each failed gate prints, if any — with its CS offset — and
   the arms that print **nothing at all** (row 9's first two gates are the known
   precedent).
4. The **effect**: every global the success path writes, as
   `20ae:<off>` with the exact instruction (`mov byte [x],1`,
   `add word [x],n`, `inc`, …), and the debit site.
5. The confirmation string the success path prints, with its CS offset.
6. Any place the arm's effect is *read* — a global written by an arm and never
   read anywhere is a finding worth stating, because the port then has nothing
   to implement beyond the flag. `python3 tools/re_query.py xrefs-to <addr>`
   answers this.

**Deliverables**

- `docs/re/shop-arms.md` — new. One section per row, each claim tiered and
  addressed. Include a "what the port must change" summary the porting task
  reads. Cross-link from `docs/re/tables.md` §2 (a pointer line, not a rewrite)
  and update the `docs/re/gaps.md` "Shop purchase effects" entry to record what
  is now closed for `bmar` 1–6 and what remains.
- `data/shop_arms.json` — machine-readable twin, one record per row:
  `{shop, key, compare_addr, gates[], effects[], strings[], debit_addr}`,
  addresses in `SEG:OFF` form, strings carried as their CS file offsets **and**
  their decoded UTF-8 text.
- `tools/test_shop_arms.py` — re-derives every address and every string in
  `data/shop_arms.json` from `orig/g.exe`, so the artifact cannot drift from the
  binary. Follow `tools/test_character_sheet.py` for shape.

**Done when** the tests pass under both runners and every row 1..6 has either a
recovered effect or an explicit, addressed statement that it has none.

---

### Task 24 (port): `bmar` rows 1–6 do what the original does

Consumes Task 23. Touches `src/`.

1. Add whatever state rows 1–6 need — on `crate::model::Fighter` when the
   original stores it in the fighter record, on `Game` when the original stores
   it in a standalone global. Follow `Game::pistol` (Task 18) for the standalone
   shape and name each field after what the arm proves it is, never after what
   the menu text advertises.
2. Implement each arm in the shape of `Game::buy_pistol_row`: its own gates, its
   own refusal lines, its own confirmation line, its own effect, and the debit at
   the point the original debits. Extend or generalise `buy_pistol_row` rather
   than writing a sibling if the arms turn out to share a shape — do not
   duplicate a logic block verbatim.
3. **Cite the address of every gate, every effect and every string** in the
   `src/` doc comments, in `SEG:OFF` form. This is what moves `port_touched`;
   an uncited port of a mapped arm scores zero.
4. Resolve the district-gate divergence Task 23 settles: if rows 1–6's buy
   compares carry no district test, `Game::shop_action`'s `gate_open` refusal
   must stop applying to the buy path (the gate stays on the *menu*), and the
   change gets a `docs/re/gaps.md` entry saying so.
5. If any row's effect has no representable target — the original writes a
   global nothing ever reads — say so in the doc comment with the `xrefs-to`
   evidence rather than inventing a `Fighter` field for it.

**Tests:** a Rust test per row asserting money debited, effect applied, and each
refusal arm reached. Where an effect changes combat numbers, assert the changed
number, not just the flag.

**Done when** `cargo test` is green, every row 1..6 either applies its effect or
carries an addressed statement that it has none, and the `port_touched` count
for `1000:ab59` has risen. Report the before/after count with the command that
produced it.

---

### Task 25 (RE): map the `mar` rows 1–9 purchase arms

**Range:** `1000:b94a` .. `1000:c4be` — the `mar` handler, whose untouched run
is `1000:bcb5`..`1000:c32e`. Note the range also contains the market pickpocket
block already cited at `1000:c353`..`1000:c369` (`docs/re/gaps.md`) — that is
**out of scope**; do not re-derive or edit it.

Rows are Хотдог (2), Пиво (5), очки (10), abibas (15), бутсы (15), кожанка (25),
adidas (30), бутсы+2 (30), кожанка+4 (50).

Recover the same six things Task 23 lists, per row. Two questions specific to
this shop:

- **Хотдог and Пиво are consumables**, and beer's counter (`20ae:38c3`) and
  `FUN_1000_29c4` are already known (`src/model.rs`, `docs/re/gaps.md`). Whether
  the market rows write those same globals is a flow question, not an analogy.
- Rows 4/7 (abibas/adidas) advertise "Смягчает пинок на 1 / на 2" and rows 5/8
  advertise damage, row 6/9 armour. Whether the arm writes `armor`
  (`+0x16` of the player record, `20ae:38b2`), `dmg_min`/`dmg_max`, or a separate
  global consulted at combat time is the finding. The menu text is **output**
  and cannot establish it.

**Deliverables:** extend `docs/re/shop-arms.md`, `data/shop_arms.json` and
`tools/test_shop_arms.py` with the `mar` rows. Same standard.

---

### Task 26 (port): `mar` rows 1–9 do what the original does

Consumes Task 25. Same contract as Task 24, for the nine market rows, including
the `Пиво(#з)` second-placeholder literal `5` (file `0xD32A`) that
`Game::shop_action`'s doc comment currently records as an unfilled gap.

**Done when** `cargo test` is green and the `port_touched` count is reported
before/after with its command.

---

## Final

After Task 26, re-run the coverage block from `docs/re/branches.md` and update
the measured-history table in `docs/superpowers/RESUME.md` — both the
`docs/re/*.md` metric and the `port_touched` metric, each labelled with its
method. Do not overwrite a row that does not reproduce; record the discrepancy,
as the existing table does.
