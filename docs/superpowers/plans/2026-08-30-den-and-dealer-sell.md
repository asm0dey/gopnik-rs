# The den's submenu and the dealers' sell path — the two largest unported handlers

**Spec authority:** `docs/re/METHODOLOGY.md` (binding evidence standard) and
the Global Constraints of `docs/superpowers/plans/2026-08-17-gopnik-rust-port.md`,
which this plan inherits verbatim and does not restate.

**Branch:** `main`, by explicit owner instruction ("work on main").

## Why this slice, with the number

`data/branches.json`'s `port_touched` — a game branch counts when its own
address or its guard's appears as a `SEG:OFF` citation in `src/**/*.rs` or
`data/command_dispatch.json` — stands at **388 / 838 (46.3%)** at `7695ef9`,
recomputed by the block in `docs/re/branches.md` under *Recomputation, from the
shipped artifacts → Coverage* (the same block's `git worktree` validation at
`82a08d8` still holds; do not trust a figure this plan quotes without re-running
it).

Split `entry`'s 406 branches by dispatch handler, using
`data/command_dispatch.json`'s `confirmed_dispatch_chain` compare addresses as
the range boundaries:

| handler | untouched / total | largest span inside |
|---|---:|---|
| `run`/`w` wander (`ae97`..`b94a`) | 51 / 97 | fragmented — no span above 5 |
| `bmar` dealers (`c4be`..`d3a6`) | 37 / 82 | **24** (`ced9..d382`) |
| `pr` den (`d802`..`df06`) | **35 / 44** | **22** (`d8c9..dc0d`) + 9 (`dd33..ddf5`) |
| `trn` gym (`e390`..`e973`) | 23 / 38 | 21 (`e590..e947`) |
| `rep` vet (`d3a6`..`d6ed`) | 20 / 23 | |

`run`'s 51 is the largest count but the worst yield: it is spread over spans of
five branches and under, and the handler is already 46/97 ported, so the
remainder is edge cases, not a feature. The two slices this plan takes are the
largest *coherent, unported* blocks:

1. **The den's submenu**, `1000:d802`..`1000:df06` — 35 untouched of 44. Only
   the intro (`Game::print_den_intro`, `1000:d82f`..`1000:d8b9`) and the `a`
   reveal (`Game::den_reveal`, `1000:dcbf`..`1000:dce3`) are ported. Everything
   between and after them is not: the block that reads `20ae:3b78`/`20ae:3b79`
   and `20ae:38cb` against `0x64`, the `20ae:3e35` arm, the `20ae:38c3` arm, and
   the three `CMP DX,BX` / `CMP AX,CX` blocks at `1000:dda8`..`1000:ddf1` that
   are the only 32-bit compares in the handler.
2. **The dealers' sell path** — `x` (`1000:ce80`, junk) and `wes`
   (`1000:ced8`, items) — 25 untouched of 26, and `wes`'s 24 branches are the
   single largest untouched contiguous run in the whole game. Both verbs parse
   (`Command::SellJunk` / `Command::SellItems`, `src/commands.rs`), and both
   handlers are **hardcoded refusals**: `Game::sell_junk` always prints
   `^4Тебе нечего спихнуть.` and `Game::sell_items` always prints
   `^6У тебя нет неужных вещей.`. `docs/re/gaps.md` already records that
   `sell_junk`'s stated justification is stale — Task 13 made `Fighter::junk`
   non-zero, "so the dealers' sell-junk branch is no longer always the one
   taken". The port refuses a sale the original makes.

Together: **60 branches, 7.2% of the game population**, and two features a
player can see.

## What moves the metric

`port_touched` reads `src/**/*.rs` and `data/command_dispatch.json` — **it does
not read `docs/`**. An RE task that only writes `docs/re/` moves it by zero
(Tasks 16 and 17 documented +103 branches and moved `src/` behaviour by nothing;
see `CLAUDE.md`, *Priority*). So each RE task here is immediately consumed by a
porting task that cites the same addresses in `src/` doc comments **and** makes
the arm do what the original does.

A `src/` diff that is only comments does not close a porting task.

**Out of scope, deliberately:** the 12 untouched branches in the dealers' *menu*
region (`1000:c4c3`..`1000:c83b`, the per-row listing gates). The port already
implements that filter in `Game::listed_rows`; a task there would be citations
over unchanged behaviour, which is the comment-only diff this plan forbids.

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
- **Output never establishes flow.** A string you found in `data/strings.json`
  is evidence that a string exists, never that a branch prints it.

---

### Task 27 (RE): map the den's submenu

**Range:** `1000:d802` .. `1000:df06` — from the `pr` verb compare to the `kl`
compare. Two blocks inside it are ALREADY ported and are **out of scope**: the
district-keyed intro `1000:d82f`..`1000:d8b9` (`Game::print_den_intro`) and the
`a` reveal `1000:dcbf`..`1000:dce3` (`Game::den_reveal`). Do not re-derive or
edit either; if the mapping proves one of them wrong, say so and stop there —
correcting it is the porting task's call.

Recover, **from flow**:

1. **The submenu's key set.** The den reads its own keys with its own `ReadLn`
   into `DS:3a72` (the shape `src/game.rs`'s `shop_turn` doc comment describes,
   and the same buffer the dealers' `x`/`wes` use). Recover every key the
   handler compares, each compare's address, and each key's token CS offset.
   `a` is known (`Game::den_reveal`); `hp` is named in `src/commands.rs` as
   living "inside `pr`'s own submenu" — that is a **string** observation, so
   confirm or refute it at a compare address.
2. **Per arm: every gate, in order**, with the address and the exact
   instruction. The recurring guards visible in `data/branches.json` are
   `cmp byte [0x3b78],1` (`1000:d8cd`, `1000:da3a`, `1000:dbf8`),
   `cmp byte [0x3b79],0` (`1000:d8ed`, `1000:dac7`, `1000:dd55`),
   `cmp word [0x38cb],0x64` (`1000:d8f4`, `1000:dac0`, `1000:dd4b`),
   `cmp byte [0x3e35],0` (`1000:d9f1`, `1000:db8d`) and
   `cmp word [0x38c3],0` (`1000:d989`, `1000:db38`). Name what each global IS
   from its writers and readers (`python3 tools/re_query.py xrefs-to <addr>`),
   never from the adjacent string.
3. **The three 32-bit compare blocks** `1000:dda8`..`1000:ddb1` and
   `1000:dded`..`1000:ddf1` (`CMP DX,BX` / `CMP AX,CX`, `JL`/`JG`/`JC`/`JNC`).
   These are the handler's only wide compares; recover what pair of 32-bit
   quantities each compares and which arm each outcome reaches. A signed/
   unsigned mix (`JL` beside `JC`) is a finding, not a transcription slip.
4. **Every RNG draw** in the range: the call site, the `n` pushed
   (`python3 tools/re_query.py pushed-n <addr>`), and where the result lands.
   `1000:d83f` is already ported and is the baseline for the idiom.
5. **Every effect**: each global the success path writes, as `20ae:<off>` with
   the exact instruction, and every debit/credit site.
6. **Every string the arm prints**, with its CS file offset and decoded text,
   and the arms that print **nothing at all**.
7. **Whether any arm is unreachable in this port** — a gate on a global nothing
   in `src/` ever sets is a finding the porting task needs stated, not
   discovered.

**Deliverables**

- `docs/re/den.md` — new. One section per arm, each claim tiered and addressed,
  ending with a "what the port must change" summary the porting task reads.
- `data/den_arms.json` — machine-readable twin, one record per arm:
  `{key, compare_addr, gates[], draws[], effects[], strings[], notes}`,
  addresses in `SEG:OFF` form, strings carried as their CS file offsets **and**
  their decoded UTF-8 text. Follow `data/shop_arms.json` for shape, including
  its explicit COVERAGE BOUNDARY note.
- `tools/test_den_arms.py` — re-derives every address and every string in
  `data/den_arms.json` from `orig/g.exe`, so the artifact cannot drift from the
  binary. Follow `tools/test_shop_arms.py` for shape.
- A pointer line into `docs/re/gaps.md` recording what this closes and what it
  leaves open.

**Done when** the new tests pass under both runners and every arm in range has
either a recovered effect or an explicit, addressed statement that it has none.

---

### Task 28 (port): the den's submenu does what the original does

Consumes Task 27. Touches `src/`.

1. Add whatever state the arms need — on `crate::model::Fighter` when the
   original stores it in the fighter record, on `Game` when it is a standalone
   global. Name each field after what the arm proves it is, never after what
   the printed text advertises.
2. Implement each arm inside `Game::shop_turn`'s `Location::Den` path, in the
   shape the dealers' and vet's submenus already use. Extend rather than
   duplicate: if two arms share a gate sequence, factor it — do not repeat a
   logic block verbatim.
3. **Cite the address of every gate, every draw, every effect and every string**
   in the `src/` doc comments, in `SEG:OFF` form. This is what moves
   `port_touched`; an uncited port of a mapped arm scores zero.
4. Where an arm's gate reads a global nothing in this port ever sets, implement
   the arm anyway and record the unreachability in `docs/re/gaps.md` with the
   `xrefs-to` evidence. Do not delete the arm.
5. Where the original's arithmetic is a 32-bit compare, reproduce the width and
   the signedness. A `JC` reproduced as a signed compare is a divergence, and an
   undocumented one is a defect.

**Tests:** a Rust test per arm asserting each gate's refusal is reached, the
effect applied on the success path, and the exact printed lines. Where an arm
spends a draw, assert the draw is spent (the RNG sequence is observable state).

**Done when** `cargo test` is green, every arm in range either applies its
effect or carries an addressed statement that it cannot, and the `port_touched`
count is reported before/after with the command that produced it.

---

### Task 29 (RE): map the dealers' sell path — `x` and `wes`

**Range:** `1000:ce80` .. `1000:d3a6` — from the `x` compare to the `rep`
compare. `1000:ce80` and `1000:ced8` are the two sub-verb compares, both against
`DS:3a72` (`src/commands.rs`); `1000:ce87`..`1000:ce97` is already cited in
`src/model.rs` as the junk-into-money move and is the one piece of this range
the port has looked at.

`x` (junk) is two branches and one of them is already touched. `wes` (items) is
24 branches, all untouched, and `data/branches.json` shows it as **seven
repeated arms** — `1000:ce85`, `cf72`, `d027`, `d0dc`, `d19f`, `d25b`, `d310`
each open a block whose guards test a *pair* of item flags:
`[0x38b4]`/`[0x38b7]`, `[0x38b5]`/`[0x38b8]`, `[0x38b6]`/`[0x38b9]`, then
`[0x38ba]`, `[0x394b]`, `[0x38c2]`, `[0x394c]` — the same flags the *buy* rows
7–9 test at `1000:cc1d`..`1000:cc2e`. Establish what that pairing means from
flow; the obvious reading (own-flag plus equipped-flag) is a hypothesis until an
address carries it.

Recover, per arm:

1. The compare or dispatch that selects it, and what it is keyed on — the seven
   blocks are not obviously key-driven, so establish whether `wes` walks a list
   or reads a second key.
2. Every gate, with address and exact instruction, including the pairing above.
3. The **refund**: the exact credit instruction into `20ae:38c7` and its amount.
   Whether the refund equals the buy price, a fraction of it, or a literal is
   the central finding — the menu text cannot answer it.
4. Every other global the arm writes (clearing the item flag, unequipping, the
   `[0x3e33],0xff` compare at `1000:d33f`).
5. Every string printed, with CS file offset and decoded text; and the arms that
   print nothing.
6. For `x`: the full junk arm, including `1000:ce87`..`1000:ce97`'s move and
   zeroing, the `cmp word [0x38c9],0` at `1000:ce8c`, and whether any rate or
   multiplier applies between the junk count and the money credit.

**Deliverables:** extend `docs/re/shop-arms.md` with a sell section (its
COVERAGE BOUNDARY note in `data/shop_arms.json` must be amended, not left to
claim the file is buy-only), add the sell arms to `data/shop_arms.json` under
their own key, and extend `tools/test_shop_arms.py` to sweep them by the same
four binary re-derivations it already runs. Update the `docs/re/gaps.md` entry
that calls the sell path a stub.

**Done when** the tests pass under both runners and every one of the seven `wes`
arms plus `x` has a recovered refund or an addressed statement that it has none.

---

### Task 30 (port): the dealers buy your stuff back

Consumes Task 29. Same contract as Task 28, for `Game::sell_junk` and
`Game::sell_items`.

1. Replace both hardcoded refusals with the real arms. The refusal lines stay —
   they are what the original prints when nothing is sellable — but they must
   become the *else* of a recovered gate, not the whole function.
2. `Game::sell_junk`'s doc comment currently justifies the stub with a claim
   Task 13 already falsified (`docs/re/gaps.md`: `Fighter::junk` is non-zero
   after a won fight). Delete the stale justification; do not leave it beside
   working code.
3. Cite every gate, refund and string address in `SEG:OFF` form.

**Tests:** a Rust test per arm — junk sold at the recovered rate, each item
sold, each refusal reached, and the money delta asserted as a number.

**Done when** `cargo test` is green and the `port_touched` count is reported
before/after with its command.

---

## Final

After Task 30, re-run the coverage block from `docs/re/branches.md`, update the
verbatim expected output that block carries (it currently quotes Task 26's
tree), and update the measured-history table in `docs/superpowers/RESUME.md` —
both the `docs/re/*.md` metric and the `port_touched` metric, each labelled with
its method. Do not overwrite a row that does not reproduce; record the
discrepancy, as the existing table does.
