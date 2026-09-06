//! The gym's key dispatch -- `trn`'s second block, `1000:e624`..`1000:e948`.
//!
//! `crate::game` keeps the verb (`Command::Gym` -> `Game::enter_shop`), the
//! intro line, the five [`crate::game::IMM_ROWS`] menu rows and the prompt;
//! this module is only what happens after the `ReadLn` at
//! `1000:e60b`..`1000:e615`. The map it is written from is `docs/re/gym.md`
//! and `data/gym_arms.json`, whose `what_the_port_must_change` array is the
//! work order and which `python3 tools/test_gym_arms.py` re-derives from
//! `orig/g.exe`.
//!
//! ## The second block is the key dispatch, not a second menu
//!
//! **Established from flow** (`data/gym_arms.json`'s `menu_vs_arm_finding`).
//! `1000:e400`..`1000:e594` prints the menu, once, on entry;
//! `1000:e633`..`1000:e941` is the chain of key compares, reached by
//! fall-through from the prompt and by nothing else. Eight of the ten guard
//! constants are byte-identical between the two halves and one whole
//! predicate is (row 3's level test, 17 bytes at `1000:e4b1` and
//! `1000:e746`) -- but two things are not, and both matter here:
//!
//! * the `5` arm's ceiling is `(district - 2) * 10` (`1000:e87f`..`1000:e894`)
//!   where the row's is `district * 2` (`1000:e57d`..`1000:e58d`), so the arm
//!   is reachable through a menu that no longer lists it;
//! * the `4` arm refuses when the tooth guard is already owned
//!   (`1000:e7fa`) and the row has no such gate (`1000:e51a` is its only
//!   one), so the row stays listed after the purchase.
//!
//! So the two halves share no code here either. `crate::game`'s
//! `Game::imm_row_visible` owns the menu predicates; nothing in this module
//! calls it.
//!
//! ## Three things this module deliberately does not do
//!
//! * **No refusal for a key the district hides.** At district 1 the `3`,
//!   `4` and `5` compares are jumped over entirely (`1000:e72d`,
//!   `1000:e7e7`, `1000:e866`), so the key is never compared and nothing is
//!   printed. [`key_dispatches`] evaluates the district before the key for
//!   exactly that reason: testing the key first and the district second
//!   would print a refusal the original has no path to.
//! * **No message for an unrecognised key.** The chain falls off its end at
//!   `1000:e943`, which jumps to the PROMPT and not to the menu, and there
//!   is no `Непонятно` literal anywhere in `1000:e390`..`1000:ea94` for it
//!   to print (`data/gym_arms.json`'s string sweep over the range).
//! * **No re-print of the menu between turns.** `1000:e943`'s target is
//!   `1000:e5e4`, the prompt.
//!
//! Address convention: `docs/re/METHODOLOGY.md`, "Address convention, and
//! its range of validity"; `python3 tools/re_query.py resolve <citation>`
//! converts one and prints the bytes there. Every string literal below is
//! quoted from `data/strings.json` at the file offset its `mov di,<n>` push
//! resolves to, markup and trailing spaces included.
//!
//! ## Why the string citations are written the way they are
//!
//! `tools/test_string_citations.py` scans this file (Task 32's review round
//! added it to that scanner's `SOURCES`, which had let the whole module
//! through — the same omission its own docstring records for `src/game.rs`
//! at Task 20). It checks two things and both need a particular shape:
//! `scan` resolves a `file 0xNNNN` citation only when a backtick-quoted
//! literal sits within one line of it, and `comment_code_pairs` binds that
//! quoted literal to the Rust literal on a code line within two below. So
//! each of the **18** inline citations here is written as
//! `file `0xNNNN` `^Nthe string``, on one line, directly above the
//! `term::println` that prints it. Written the shorter way — the offset in
//! the comment and the text only inside the `println` — both halves report
//! nothing at all, which is how a module with 18 of them passed a guard
//! whose entire purpose is to catch a wrong one.
//!
//! **The citations that stay `unchecked` here are of two kinds, and neither
//! is a gap.** Most are inside the `text` disassembly fences of the arm docs
//! below (`mov di,0xa47e (file 0xBD4E)` and its like) — transcript lines,
//! where quoting the string beside the offset would corrupt the transcript —
//! and the rest are prose mentions in this doc and the arm docs. **Every
//! offset in both kinds is checked elsewhere in this file** by an inline
//! citation of the same offset, so `unchecked` here means "cited twice, once
//! in a checkable form", not "unverified". The one exception is `CS 0x848e`,
//! the shared `w` exit token, whose string is the single character `w` and
//! therefore can never match the scanner's game-text rule at all.
//!
//! Both statements are recomputable rather than counted here: the per-kind
//! tally and the "checked elsewhere" containment come from running
//! `tools/test_string_citations.py`'s `scan` and `literals_near` over this
//! file, which is what the Task 32 fix-round report shows. A raw count in
//! this comment would go stale on the next edit, which is the defect
//! `docs/re/METHODOLOGY.md` warns about under "A port citation cites the
//! command, not the line it printed".

use crate::game::Game;
use crate::progress;
use crate::term;
use crate::text;

/// Whether the gym's compare chain reaches a compare that `key` matches.
///
/// **Established from flow.** Six keys exist, each at its own `0f78:0bd8`
/// compare against the gym's own buffer `20ae:3a72` -- `1` `1000:e62e`,
/// `2` `1000:e6ba`, `3` `1000:e73c`, `4` `1000:e7f3`, `5` `1000:e875`,
/// `w` `1000:e93c` -- and three of them sit behind a district test that
/// decides whether the compare happens at all:
///
/// | key | gate | branch | sense |
/// |---|---|---|---|
/// | `3` | `1000:e728` `cmp byte [0x3692],0x1` | `1000:e72d` `ja 0xe732` | district > 1 |
/// | `4` | `1000:e7e2` `cmp byte [0x3692],0x1` | `1000:e7e7` `jbe 0xe861` | district > 1 |
/// | `5` | `1000:e861` `cmp byte [0x3692],0x2` | `1000:e866` `ja 0xe86b` | district > 2 |
///
/// So at district 1 exactly three keys exist -- `1`, `2` and `w` -- and that
/// is a fact about the DISPATCHER, not about what the menu printed. `&&`
/// short-circuits left to right, which is the original's order: gate, then
/// compare.
///
/// `w` is not here. Its compare at `1000:e93c` is the shared exit every
/// location's prompt has (the literal at CS `0x848e` has nine push sites
/// image-wide), and `Game::shop_turn`'s catch-all already owns it; a
/// `false` from this function is `1000:e932`, the fall-through into it.
pub(crate) fn key_dispatches(g: &Game, key: &str) -> bool {
    match key {
        // 1000:e62e / 1000:e633 -- no gate of its own.
        "1" => true,
        // 1000:e6ba / 1000:e6bf -- no gate of its own.
        "2" => true,
        // 1000:e728 gates the compare at 1000:e73c, whose hit is
        // 1000:e741; 1000:e72f jumps the whole arm.
        "3" => g.district > 1,
        // 1000:e7e2 gates the compare at 1000:e7f3, whose miss is
        // 1000:e7f8.
        "4" => g.district > 1,
        // 1000:e861 gates the compare at 1000:e875, whose hit is
        // 1000:e87a; 1000:e868 jumps the whole arm.
        "5" => g.district > 2,
        // 1000:e932: the `w` compare, then 1000:e943 back to the prompt.
        _ => false,
    }
}

/// Run the arm `key` selected. Only ever called when [`key_dispatches`]
/// said the chain reaches that key's compare, which is where the district
/// gates live; the arms below carry only their own gates.
pub(crate) fn run_key(g: &mut Game, key: &str) {
    match key {
        "1" => train_strength(g),
        "2" => train_stamina(g),
        "3" => train_xp(g),
        "4" => buy_tooth_guard(g),
        "5" => train_abs(g),
        _ => {}
    }
}

/// `1` -- `1000:e624`..`1000:e6b0`, `качаться гантелями и штангой`, 20 rubles.
///
/// **Established from flow**, re-disassembled for this task with
/// `python3 tools/re_query.py resolve 1000:e624 -n 800 -i 500`:
///
/// ```text
/// e635  cmp word [0x38c7],0x14 / e63a jnl 0xe657   ; can pay 20, SIGNED
/// e63c  mov di,0x8e4d (file 0xA71D) .. e650 WriteLn / e655 jmp short 0xe6b0
/// e657  sub word [0x38c7],0x14                     ; бабки -20
/// e65c  mov di,0xa47e (file 0xBD4E) .. e670 WriteLn
/// e675  inc [0x389e]                               ; Сила +1
/// e679  inc [0x38ae]                               ; здоровье max +1
/// e67d  inc [0x38ac]                               ; здоровье +1
/// e681  mov ax,[0x389e] / e684 cwd / e685 mov cx,2 / e688 idiv cx
/// e68a  xchg ax,dx / e68b or ax,ax / e68d jnz 0xe693
/// e68f  inc [0x38a8]                               ; урон min +1
/// e693  inc [0x38aa]                               ; урон max +1
/// e697  mov di,0x9402 (file 0xACD2) .. e6ab WriteLn
/// ```
///
/// **The damage split is the one thing here that is easy to get backwards.**
/// `1000:e68d`'s `jnz` is a four-byte skip and `1000:e68f inc [0x38a8]` is
/// exactly four bytes, so the jump lands on `1000:e693` and урон max rises on
/// EVERY purchase while урон min rises only when the NEW strength -- the one
/// `1000:e675` just incremented -- is even. Both-conditional and
/// both-unconditional are equally wrong and equally invisible in a screen
/// capture, which is why
/// `arm_1_raises_dmg_max_every_time_and_dmg_min_only_on_an_even_strength`
/// buys twice from an odd strength and asserts +1 against +2 separately.
///
/// The `idiv` is signed and `Fighter::strength` is a `u16`, so the negative
/// half of the remainder test is unrepresentable and `is_multiple_of(2)`
/// decides as `or ax,ax` / `jnz` does. Nothing one-shot is consumed, so the
/// arm repeats.
fn train_strength(g: &mut Game) {
    // 1000:e635 / 1000:e63a -- a signed word compare in the original; money
    // is an i32 here, which is the standing width divergence, not a new one.
    if g.player.money < 20 {
        // 1000:e63c pushes file `0xA71D` `^4Не хватает`, printed by
        // 1000:e650; 1000:e655 leaves.
        term::println("^4Не хватает");
        return;
    }
    g.player.money -= 20; // 1000:e657

    // 1000:e65c pushes file `0xBD4E` `^2Ты прокачиваешь силу.`, printed by
    // 1000:e670 -- BEFORE the six stores.
    term::println("^2Ты прокачиваешь силу.");
    g.player.strength += 1; // 1000:e675
    g.player.hpmax += 1; // 1000:e679
    g.player.hp += 1; // 1000:e67d

    // 1000:e681..1000:e68d: the remainder of the NEW strength div 2.
    if g.player.strength.is_multiple_of(2) {
        g.player.dmg_min += 1; // 1000:e68f
    }
    g.player.dmg_max += 1; // 1000:e693 -- outside the branch, every time.

    // 1000:e697 pushes file `0xACD2` `^1Сила +1 ` (the trailing space is
    // the original's), printed by 1000:e6ab.
    term::println("^1Сила +1 ");
}

/// `2` -- `1000:e6b0`..`1000:e728`, `качаться на тренажерах`, 20 rubles.
///
/// **Established from flow**, same decode:
///
/// ```text
/// e6c1  cmp word [0x38c7],0x14 / e6c6 jnl 0xe6e3
/// e6c8  mov di,0x8e4d (file 0xA71D) .. e6dc WriteLn / e6e1 jmp short 0xe728
/// e6e3  sub word [0x38c7],0x14
/// e6e8  mov di,0xa496 (file 0xBD66) .. e6fc WriteLn
/// e701  inc [0x38a2]                 ; Выносливость +1
/// e705  add word [0x38ae],0x5        ; здоровье max +5
/// e70a  add word [0x38ac],0x5        ; здоровье +5
/// e70f  mov di,0xa4b6 (file 0xBD86) .. e723 WriteLn
/// ```
///
/// Same price and the same refusal literal as `1`, and that is where the
/// resemblance stops: this arm has four effects and **no conditional at
/// all**, both health words rise by a flat 5, and neither damage word is
/// touched. The two arms are not a template of each other.
fn train_stamina(g: &mut Game) {
    // 1000:e6c1 / 1000:e6c6.
    if g.player.money < 20 {
        // 1000:e6c8 pushes file `0xA71D` `^4Не хватает`, printed by
        // 1000:e6dc; 1000:e6e1 leaves.
        term::println("^4Не хватает");
        return;
    }
    g.player.money -= 20; // 1000:e6e3

    // 1000:e6e8 pushes file `0xBD66` `^2Ты прокачиваешь выносливость.`,
    // printed by 1000:e6fc.
    term::println("^2Ты прокачиваешь выносливость.");
    g.player.vitality += 1; // 1000:e701 -- 20ae:38a2 is `+0x06`, живучесть
    g.player.hpmax += 5; // 1000:e705
    g.player.hp += 5; // 1000:e70a

    // 1000:e70f pushes file `0xBD86` `^1Выносливость +1 ` (trailing space
    // is the original's), printed by 1000:e723.
    term::println("^1Выносливость +1 ");
}

/// `3` -- `1000:e728`..`1000:e7e2`, `прокачать 10 качков опыта`, 10 rubles.
///
/// **Established from flow**, same decode:
///
/// ```text
/// e746  mov al,[0x3692] / e749 xor ah,ah / e74b mov dx,0xa / e74e mul dx
/// e750  sub ax,0x3 / e753 cmp ax,[0x38a6] / e757 jnle 0xe774
/// e759  mov di,0xa4c9 (file 0xBD99) .. e76d WriteLn / e772 jmp short 0xe7e2
/// e774  cmp word [0x38c7],0xa / e779 jnl 0xe796
/// e77b  mov di,0x9473 (file 0xAD43) .. e78f WriteLn / e794 jmp short 0xe7e2
/// e796  sub word [0x38c7],0xa                 ; бабки -10
/// e79b  mov di,0xa4f8 (file 0xBDC8) .. e7af WriteLn
/// e7b4  add word [0x38ce],0xa                 ; опыт +10
/// e7b9  mov di,0xa50b (file 0xBDDB) / e7be mov ax,0xa / e7c1 push ax
/// e7ce  WriteLn
/// e7d3  mov ax,[0x38ce] / e7d6 cmp ax,[0x38d0] / e7da jl 0xe7e2
/// e7dc  mov al,0x0 / e7de push ax / e7df call 0x12526   ; FUN_1000_2526(0)
/// ```
///
/// **The gate order is load-bearing.** The level test comes first and the
/// money test second, so a player who is both too strong and too poor sees
/// `^6Ты слишком крутой...` and never the money refusal. `1000:e757` is
/// `jnle`, so the arm runs iff `district * 10 - 3 > level` -- byte-identical
/// arithmetic to menu row 3's at `1000:e4b1`..`1000:e4be`, differing only in
/// the `jcc` that follows, because the menu SKIPS the row where the arm
/// PROCEEDS.
///
/// **The printed `#` is a separate immediate, not the xp total.** The one
/// `#` of file `0xBDDB` is filled from `1000:e7be mov ax,0xa` pushed at
/// `1000:e7c1`; menu row 3's `#` has its own `mov ax,0xa` at `1000:e505`.
/// The two are equal and independent.
///
/// **The ordering `1000:e796` / `1000:e7b4` / `1000:e7ce` / `1000:e7df`
/// is why [`progress::apply_levels`] is called with `award = 0`.** The xp is
/// credited by the `add` at `1000:e7b4` and printed before the level-up runs,
/// and `apply_levels` adds its own `award` *before* the threshold test, so
/// passing `award = 10` after the manual `xp += 10` would grant twenty.
/// `arm_3_credits_ten_qualification_points_once_not_twice` is the falsifier.
///
/// **The outer threshold guard is kept.** `1000:e7d3`..`1000:e7da` duplicates
/// the callee's own entry test at `1000:2535`: the seven bytes
/// `a1 ce 38 3b 06 d0 38` are identical at `1000:e7d3` and `1000:2535`, and
/// only the `jcc` after them differs (`1000:e7da jl` against
/// `1000:253c jnl`) -- and the callee's early-out lands at
/// `1000:28c1`, past the closing message at `1000:28a6`, so keeping it and
/// dropping it are both faithful. It is kept because it is a branch of the
/// original and this is a port. `1000:e7df`'s `param_1 = 0` is the CAPPED
/// form (`1000:257a`..`1000:2587`), which is `uncapped: false`.
///
/// This is the only arm in the whole range that can move the RNG stream, and
/// it moves it indirectly: the gym contains no `Random` call site of its own,
/// while `1000:2526` spends two draws per level gained at `1000:25fe`.
fn train_xp(g: &mut Game) {
    // 1000:e746..1000:e753 / 1000:e757 `jnle`. `mul dx` is unsigned and the
    // compare is signed; district is 1..5, so neither can wrap.
    if i32::from(g.district) * 10 - 3 <= i32::from(g.player.level) {
        // 1000:e772 leaves. file `0xBD99` `^6Ты слишком крутой чтобы тренироваться здесь.`,
        // pushed at 1000:e759 and printed by 1000:e76d.
        term::println("^6Ты слишком крутой чтобы тренироваться здесь.");
        return;
    }
    // 1000:e774 / 1000:e779 -- second, so the line above wins when both fail.
    if g.player.money < 10 {
        // 1000:e77b pushes file `0xAD43` `^4Не хватает деньжат`, printed by
        // 1000:e78f; 1000:e794 leaves.
        term::println("^4Не хватает деньжат");
        return;
    }
    g.player.money -= 10; // 1000:e796

    // 1000:e79b pushes file `0xBDC8` `^2Ты тренируешься.`, printed by
    // 1000:e7af.
    term::println("^2Ты тренируешься.");
    g.progress.xp += 10; // 1000:e7b4

    // The `#` is 1000:e7be's own `mov ax,0xa`, pushed at 1000:e7c1 -- not
    // [0x38ce]. file `0xBDDB` `^1 +# качков опыта `, pushed at 1000:e7b9
    // and printed by 1000:e7ce.
    term::println(&text::fill("^1 +# качков опыта ", &[10]));
    // 1000:e7d3 / 1000:e7d6 / 1000:e7da.
    if g.progress.xp < g.progress.threshold {
        return;
    }
    // 1000:e7dc / 1000:e7df -- FUN_1000_2526(0). `award` is 0 because the
    // grant already happened at 1000:e7b4.
    progress::apply_levels(&mut g.progress, &mut g.player, &mut g.rng, 0, false);
}

/// `4` -- `1000:e7e2`..`1000:e861`, `купить зубную защиту боксёров`, 30 rubles.
///
/// **Established from flow**, same decode:
///
/// ```text
/// e7fa  cmp byte [0x394a],0x0 / e7ff jnz 0xe848
/// e801  cmp word [0x38c7],0x1e / e806 jnl 0xe823
/// e808  mov di,0xa51f (file 0xBDEF) .. e81c WriteLn / e821 jmp short 0xe846
/// e823  sub word [0x38c7],0x1e            ; бабки -30
/// e828  mov byte [0x394a],0x1             ; зубная защита := 1
/// e82d  mov di,0xa537 (file 0xBE07) .. e841 WriteLn / e846 jmp short 0xe861
/// e848  mov di,0xa54a (file 0xBE1A) .. e85c WriteLn
/// ```
///
/// **The ownership gate is the arm's and the menu row has no counterpart.**
/// Row 4's only gate is `1000:e51a`, `district > 1`, so after the purchase
/// the row is still listed and the key still prints
/// `^6У тебя есть эта штучка.` That is the original's behaviour, not a bug
/// to fix in the menu; `arm_4_stays_listed_and_refuses_after_the_purchase`
/// pins it.
///
/// The gate order is the ownership test first and the money test second, so
/// an owner with no money still sees the already-owned line.
///
/// `1000:e828` is the only image-wide absolute write to `20ae:394a`
/// (`python3 tools/re_query.py xrefs-to 20ae:394a` reports five references,
/// one write) and nothing clears it -- not the district reset at
/// `1000:abbd`, which clears the discovery flags and leaves this one alone --
/// so the purchase is permanent for the character. What it buys is
/// `1000:47ce`/`1000:47f3`, which splits a jaw break into the plain arm and
/// a `Random(4)`: a DRAW-COUNT difference, not flavour.
fn buy_tooth_guard(g: &mut Game) {
    // 1000:e7fa / 1000:e7ff -- first, so it wins over the money test below.
    if g.tooth_guard {
        // 1000:e848 pushes file `0xBE1A` `^6У тебя есть эта штучка.`,
        // printed by 1000:e85c.
        term::println("^6У тебя есть эта штучка.");
        return;
    }
    // 1000:e801 / 1000:e806.
    if g.player.money < 30 {
        // 1000:e808 pushes file `0xBDEF` `^4А не хватает рубликов`, printed
        // by 1000:e81c; 1000:e821 leaves.
        term::println("^4А не хватает рубликов");
        return;
    }
    g.player.money -= 30; // 1000:e823
    g.tooth_guard = true; // 1000:e828

    // 1000:e82d pushes file `0xBE07` `^2Ты купил защиту.`, printed by
    // 1000:e841.
    term::println("^2Ты купил защиту.");
}

/// `5` -- `1000:e861`..`1000:e932`, `прокачать пресс`, 20 rubles.
///
/// **Established from flow**, same decode:
///
/// ```text
/// e87f  mov al,[0x3692] / e882 xor ah,ah / e884 dec ax / e885 dec ax
/// e886  mov dx,0xa / e889 mul dx / e88b mov dx,ax        ; (district-2)*10
/// e88d  mov al,[0x3e34] / e890 xor ah,ah / e892 cmp ax,dx / e894 jnl 0xe8f9
/// e896  cmp word [0x38c7],0x14 / e89b jnl 0xe8b8
/// e89d  mov di,0xa564 (file 0xBE34) .. e8b1 WriteLn / e8b6 jmp short 0xe8f7
/// e8b8  sub word [0x38c7],0x14           ; бабки -20
/// e8bd  mov di,0xa57a (file 0xBE4A) .. e8d1 WriteLn
/// e8d6  inc [0x38b2]                     ; Броня +1
/// e8da  inc [0x3e34]                     ; the trained-armour scratch
/// e8de  mov di,0xa593 (file 0xBE63) .. e8f2 WriteLn / e8f7 jmp short 0xe932
/// e8f9  mov di,0xa59e (file 0xBE6E) .. e90d WriteLn
/// e912  cmp byte [0x3692],0x4 / e917 jnb 0xe932
/// e919  mov di,0xa5d0 (file 0xBEA0) .. e92d WriteLn
/// ```
///
/// **This arm's ceiling is NOT menu row 5's.** The row's is `district * 2`
/// (`1000:e57d`..`1000:e58d`); this is `(district - 2) * 10`. The two
/// predicates are 16 bytes against 21 and differ by exactly `d1 e0`
/// (`shl ax,1`) against `48 48 ba 0a 00 f7 e2` (`dec` / `dec` /
/// `mov dx,0xa` / `mul dx`) -- a different NUMBER, not a different spelling
/// of the same one. At district 3 the row disappears at trained armour 6
/// while the arm keeps working to 10, so the arm is reachable through a menu
/// that no longer lists it, which is exactly why the loop reprints only the
/// prompt. Sharing one predicate between the row and the arm is wrong in
/// both directions; `arm_5_keeps_working_after_its_menu_row_has_gone` is the
/// falsifier. The `dec ax` pair cannot underflow on the reachable path: the
/// gate at `1000:e861` already required district > 2.
///
/// **The hint is suppressed from district 4 up.** `1000:e912`/`1000:e917`
/// sits INSIDE the ceiling branch, so `^6Качай дальше в следующем районе`
/// follows the ceiling line only while `district < 4`.
///
/// **`20ae:3e34` is the one value this port does not have.** The original
/// recomputes it on every entry to the gym (`1000:e3a4`..`1000:e3e2`) as the
/// armour byte `20ae:38b2` minus the armour that came from equipment, so it
/// is the armour the player TRAINED; `Game::imm_row_visible` substitutes
/// `armor` for it, and this arm makes the same substitution against its own
/// threshold. The consequence is one-directional and identical to the row's:
/// the port's value is never smaller than the original's, so the arm can only
/// stop EARLIER than the original would. That is the standing
/// `docs/re/gaps.md` entry "The four armour flags are carried but the gym's
/// `abs` ignores them", whose population this arm joins; closing it is the
/// recompute at `1000:e3a4`..`1000:e3e2` and it is not this task's subject.
/// Under that substitution `1000:e8d6` and `1000:e8da` are the same
/// increment, which is why the single `armor += 1` below carries both
/// citations -- and the arm still terminates, which
/// `arm_5_stops_when_the_ceiling_is_reached` asserts by counting.
fn train_abs(g: &mut Game) {
    // 1000:e87f..1000:e892 / 1000:e894 `jnl 0xe8f9`.
    let ceiling = (i32::from(g.district) - 2) * 10;
    // 1000:e88d reads 20ae:3e34; see the doc above for the substitution.
    if i32::from(g.player.armor) >= ceiling {
        // file `0xBE6E` `^6Ты максимально прокачал пресс для своего уровня`,
        // pushed at 1000:e8f9 and printed by 1000:e90d.
        term::println("^6Ты максимально прокачал пресс для своего уровня");
        // 1000:e912 / 1000:e917 -- inside the ceiling branch only.
        if g.district < 4 {
            // file `0xBEA0` `^6Качай дальше в следующем районе`, pushed at
            // 1000:e919 and printed by 1000:e92d.
            term::println("^6Качай дальше в следующем районе");
        }
        return;
    }
    // 1000:e896 / 1000:e89b -- second, so the ceiling line wins over it.
    if g.player.money < 20 {
        // 1000:e89d pushes file `0xBE34` `^4Не хватает рубликов`, printed
        // by 1000:e8b1; 1000:e8b6 leaves.
        term::println("^4Не хватает рубликов");
        return;
    }
    g.player.money -= 20; // 1000:e8b8

    // 1000:e8bd pushes file `0xBE4A` `^2Ты прокачиваешь пресс.`, printed by
    // 1000:e8d1.
    term::println("^2Ты прокачиваешь пресс.");
    // 1000:e8d6 `inc [0x38b2]`, the visible Броня, AND 1000:e8da
    // `inc [0x3e34]`, the scratch this arm's own ceiling is tested against.
    // They are one statement here because the port has one value for both.
    g.player.armor += 1;
    // 1000:e8de pushes file `0xBE63` `^1Броня +1`, printed by 1000:e8f2.
    term::println("^1Броня +1");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locations::Location;
    use crate::model::Fighter;
    use crate::progress::Progress;
    use crate::term::capture;
    use std::io;

    fn player() -> Fighter {
        Fighter {
            name: "Тест".to_string(),
            hp: 20,
            hpmax: 20,
            strength: 5,
            agility: 5,
            vitality: 5,
            luck: 5,
            dmg_min: 1,
            dmg_max: 3,
            ..Fighter::default()
        }
    }

    /// A gym-ready game: the gym discovered (`20ae:369a`), the player
    /// standing in it, at `district`, with `money` in the pocket. `Game`'s
    /// `mode` is private to `crate::game`, so [`turn`] names the location
    /// explicitly the way `Game::run` does from `Mode::Shop(loc)`.
    fn gym(district: u8, money: i32) -> Game {
        let mut g = Game::new(player(), Progress::new(), 12345);
        g.district = district;
        g.player.money = money;
        g.places.mark_found(Location::Gym); // 20ae:369a, gate 1000:e39a
        g.location = Location::Gym;
        g
    }

    fn no_input() -> std::iter::Empty<io::Result<String>> {
        std::iter::empty()
    }

    /// One turn at the gym prompt, through the real dispatch in
    /// `Game::shop_turn` rather than through [`run_key`] directly, so the
    /// wiring is under test too. Returns the lines the turn printed.
    fn turn(g: &mut Game, key: &str) -> Vec<String> {
        capture::lines(|| {
            g.shop_turn(Location::Gym, key, &mut no_input()).unwrap();
        })
    }

    // -- arm `1` ---------------------------------------------------------

    /// `1000:e68d`'s four-byte `jnz` skips `1000:e68f inc [0x38a8]` and
    /// lands on `1000:e693 inc [0x38aa]`. Buying twice from an ODD strength
    /// makes the new strength even once and odd once, so урон min must rise
    /// by 1 and урон max by 2. Both-conditional gives +1/+1 and
    /// both-unconditional +2/+2; this fails on either.
    #[test]
    fn arm_1_raises_dmg_max_every_time_and_dmg_min_only_on_an_even_strength() {
        let mut g = gym(1, 100);
        assert_eq!(g.player.strength % 2, 1, "the premise is an ODD strength");
        let (min0, max0) = (g.player.dmg_min, g.player.dmg_max);
        turn(&mut g, "1"); // strength 5 -> 6, even
        assert_eq!(g.player.dmg_min, min0 + 1, "1000:e68f on the even step");
        assert_eq!(g.player.dmg_max, max0 + 1, "1000:e693");
        turn(&mut g, "1"); // strength 6 -> 7, odd
        assert_eq!(g.player.dmg_min, min0 + 1, "1000:e68d skips 1000:e68f");
        assert_eq!(
            g.player.dmg_max,
            max0 + 2,
            "1000:e693 is outside the branch"
        );
    }

    /// The other five stores of the arm, and the two lines around them.
    #[test]
    fn arm_1_charges_twenty_and_prints_its_two_lines_in_order() {
        let mut g = gym(1, 20);
        let (s0, hp0, hpmax0) = (g.player.strength, g.player.hp, g.player.hpmax);
        let out = turn(&mut g, "1");
        assert_eq!(g.player.money, 0, "1000:e657 sub 0x14");
        assert_eq!(g.player.strength, s0 + 1, "1000:e675");
        assert_eq!(g.player.hpmax, hpmax0 + 1, "1000:e679");
        assert_eq!(g.player.hp, hp0 + 1, "1000:e67d");
        assert_eq!(
            out,
            vec!["^2Ты прокачиваешь силу.", "^1Сила +1 "],
            "1000:e670 then 1000:e6ab"
        );
    }

    /// `1000:e635` is `cmp ... ,0x14` and `1000:e63a` is `jnl`, so 19 refuses
    /// and 20 buys. A ceiling off by one fails here.
    #[test]
    fn arm_1_refuses_at_nineteen_and_buys_at_twenty() {
        let mut g = gym(1, 19);
        let s0 = g.player.strength;
        assert_eq!(turn(&mut g, "1"), vec!["^4Не хватает"], "1000:e650");
        assert_eq!(g.player.money, 19, "no debit on the refusal");
        assert_eq!(g.player.strength, s0, "and no effect either");

        let mut g = gym(1, 20);
        assert!(!turn(&mut g, "1").is_empty());
        assert_eq!(g.player.money, 0);
        assert_eq!(g.player.strength, s0 + 1);
    }

    // -- arm `2` ---------------------------------------------------------

    /// Four effects, no conditional, and the damage words untouched --
    /// `1` and `2` are not a template of each other.
    #[test]
    fn arm_2_adds_flat_fives_and_leaves_the_damage_alone() {
        let mut g = gym(1, 20);
        let before = g.player.clone();
        let out = turn(&mut g, "2");
        assert_eq!(g.player.money, 0, "1000:e6e3 sub 0x14");
        assert_eq!(g.player.vitality, before.vitality + 1, "1000:e701");
        assert_eq!(g.player.hpmax, before.hpmax + 5, "1000:e705");
        assert_eq!(g.player.hp, before.hp + 5, "1000:e70a");
        assert_eq!(g.player.dmg_min, before.dmg_min, "no 1000:e68f here");
        assert_eq!(g.player.dmg_max, before.dmg_max, "no 1000:e693 here");
        assert_eq!(g.player.strength, before.strength);
        assert_eq!(
            out,
            vec!["^2Ты прокачиваешь выносливость.", "^1Выносливость +1 "]
        );
    }

    /// Same price as `1`, same literal, its own compare at `1000:e6c1`.
    #[test]
    fn arm_2_refuses_at_nineteen() {
        let mut g = gym(1, 19);
        let v0 = g.player.vitality;
        assert_eq!(turn(&mut g, "2"), vec!["^4Не хватает"], "1000:e6dc");
        assert_eq!(g.player.money, 19);
        assert_eq!(g.player.vitality, v0);
    }

    // -- arm `3` ---------------------------------------------------------

    /// The `#` of file `0xBDDB` is `1000:e7be`'s own immediate 10, and the
    /// credit at `1000:e7b4` is 10. A port that printed the xp TOTAL, or
    /// credited the printed number twice, fails here.
    #[test]
    fn arm_3_credits_ten_qualification_points_once_not_twice() {
        let mut g = gym(2, 10);
        g.progress.xp = 7;
        g.progress.threshold = 1000; // keeps 1000:e7da from calling out
        let out = turn(&mut g, "3");
        assert_eq!(g.player.money, 0, "1000:e796 sub 0xa");
        assert_eq!(g.progress.xp, 17, "1000:e7b4 add 0xa, exactly once");
        assert_eq!(
            out,
            vec!["^2Ты тренируешься.", "^1 +10 качков опыта "],
            "1000:e7af then 1000:e7ce; the # is 1000:e7be's 10, not the total"
        );
    }

    /// `1000:e7df` passes `param_1 = 0`, and the grant already happened at
    /// `1000:e7b4`. So `apply_levels` gets `award = 0`: the xp left after the
    /// level-up must be `(xp + 10) - threshold`. Passing `award = 10` would
    /// leave ten more than that.
    #[test]
    fn arm_3_does_not_grant_the_award_twice_at_the_level_up() {
        let mut g = gym(2, 10);
        g.progress.xp = 5;
        g.progress.threshold = 12;
        let level0 = g.player.level;
        let step = g.progress.threshold; // captured before apply_levels moves it
        turn(&mut g, "3");
        assert_eq!(g.player.level, level0 + 1, "1000:e7df, the capped form");
        assert_eq!(g.progress.xp, 5 + 10 - step, "award must be 0, not 10");
    }

    /// `1000:e7d3`..`1000:e7da` -- below the threshold nothing is called and
    /// the level stands.
    #[test]
    fn arm_3_below_the_threshold_does_not_level_up() {
        let mut g = gym(2, 10);
        g.progress.xp = 0;
        g.progress.threshold = 1000;
        let level0 = g.player.level;
        turn(&mut g, "3");
        assert_eq!(g.player.level, level0);
        assert_eq!(g.progress.xp, 10);
    }

    /// `1000:e757` is `jnle`: the arm runs iff `district * 10 - 3 > level`.
    /// At district 2 that is 17, so level 16 trains and level 17 does not.
    #[test]
    fn arm_3_ceiling_is_district_times_ten_minus_three() {
        for (level, trains) in [(16u16, true), (17, false)] {
            let mut g = gym(2, 10);
            g.player.level = level;
            g.progress.threshold = 1000;
            let out = turn(&mut g, "3");
            assert_eq!(
                out != vec!["^6Ты слишком крутой чтобы тренироваться здесь."],
                trains,
                "level {level} against 2*10-3 at 1000:e753"
            );
            assert_eq!(g.progress.xp == 10, trains);
        }
    }

    /// The level test is `1000:e757` and the money test is `1000:e779`, in
    /// that order. Too strong AND too poor must print the level refusal; a
    /// port that tested the money first would print the other line.
    #[test]
    fn arm_3_tests_the_level_before_the_money() {
        let mut g = gym(2, 0);
        g.player.level = 100;
        assert_eq!(
            turn(&mut g, "3"),
            vec!["^6Ты слишком крутой чтобы тренироваться здесь."],
            "1000:e76d, not 1000:e78f"
        );
    }

    /// `1000:e774` is `cmp ... ,0xa`: 9 refuses with its own literal, 10 buys.
    #[test]
    fn arm_3_refuses_at_nine_and_buys_at_ten() {
        let mut g = gym(2, 9);
        g.progress.threshold = 1000;
        assert_eq!(turn(&mut g, "3"), vec!["^4Не хватает деньжат"], "1000:e78f");
        assert_eq!(g.progress.xp, 0, "no credit on the refusal");

        let mut g = gym(2, 10);
        g.progress.threshold = 1000;
        turn(&mut g, "3");
        assert_eq!(g.player.money, 0);
        assert_eq!(g.progress.xp, 10);
    }

    // -- arm `4` ---------------------------------------------------------

    /// `1000:e801` is `cmp ... ,0x1e` and `1000:e828` sets `20ae:394a`.
    #[test]
    fn arm_4_costs_thirty_and_sets_the_flag() {
        let mut g = gym(2, 29);
        assert_eq!(
            turn(&mut g, "4"),
            vec!["^4А не хватает рубликов"],
            "1000:e81c"
        );
        assert!(!g.tooth_guard, "no flag on the refusal");
        assert_eq!(g.player.money, 29);

        let mut g = gym(2, 30);
        assert_eq!(turn(&mut g, "4"), vec!["^2Ты купил защиту."], "1000:e841");
        assert!(g.tooth_guard, "1000:e828");
        assert_eq!(g.player.money, 0, "1000:e823");
    }

    /// **Do not "fix" the menu.** Row 4's only gate is `1000:e51a`
    /// (`district > 1`) and the ownership test `1000:e7fa` is the ARM's, so
    /// after the purchase the row is still listed and the key still prints
    /// the already-owned line. The `1000:e7ff` gate also comes before the
    /// money test at `1000:e801`, so an owner with nothing in the pocket
    /// still sees the owned line and not the money one.
    #[test]
    fn arm_4_stays_listed_and_refuses_after_the_purchase() {
        let mut g = gym(2, 30);
        turn(&mut g, "4");
        assert!(g.tooth_guard);
        assert!(
            crate::game::IMM_ROWS
                .iter()
                .any(|r| r.shop == "trn" && r.key == "4" && g.imm_row_visible(r)),
            "1000:e51a is row 4's only gate -- the row must stay listed"
        );
        assert!(key_dispatches(&g, "4"), "1000:e7f3 is still compared");
        g.player.money = 0;
        assert_eq!(
            turn(&mut g, "4"),
            vec!["^6У тебя есть эта штучка."],
            "1000:e85c, and 1000:e7ff wins over 1000:e806"
        );
        assert_eq!(g.player.money, 0, "and nothing is charged");
    }

    // -- arm `5` ---------------------------------------------------------

    /// `1000:e896` is `cmp ... ,0x14`; `1000:e8d6` raises the armour.
    #[test]
    fn arm_5_costs_twenty_and_raises_the_armour() {
        let mut g = gym(3, 19);
        assert_eq!(
            turn(&mut g, "5"),
            vec!["^4Не хватает рубликов"],
            "1000:e8b1"
        );
        assert_eq!(g.player.armor, 0);

        let mut g = gym(3, 20);
        assert_eq!(
            turn(&mut g, "5"),
            vec!["^2Ты прокачиваешь пресс.", "^1Броня +1"],
            "1000:e8d1 then 1000:e8f2"
        );
        assert_eq!(g.player.armor, 1, "1000:e8d6 / 1000:e8da");
        assert_eq!(g.player.money, 0, "1000:e8b8");
    }

    /// The arm's ceiling is `(district - 2) * 10` (`1000:e87f`..`1000:e894`)
    /// and the MENU row's is `district * 2` (`1000:e57d`..`1000:e58d`). At
    /// district 3 that is 10 against 6, so at armour 6 the row is gone and
    /// the arm still works. A port that shared one predicate between them
    /// would refuse here.
    #[test]
    fn arm_5_keeps_working_after_its_menu_row_has_gone() {
        let mut g = gym(3, 20);
        g.player.armor = 6;
        let row5 = crate::game::IMM_ROWS
            .iter()
            .find(|r| r.shop == "trn" && r.key == "5")
            .expect("trn row 5");
        assert!(
            !g.imm_row_visible(row5),
            "6 is not < district*2 = 6, so 1000:e58d hides the row"
        );
        assert_eq!(
            turn(&mut g, "5"),
            vec!["^2Ты прокачиваешь пресс.", "^1Броня +1"],
            "the ARM's ceiling is 10, so it runs anyway"
        );
        assert_eq!(g.player.armor, 7);
    }

    /// `1000:e8da` keeps the value the ceiling is tested against in step
    /// with the purchase, so the arm terminates. At district 3 the ceiling
    /// is 10 and exactly ten purchases fit; an arm that raised nothing the
    /// gate reads would never stop.
    #[test]
    fn arm_5_stops_when_the_ceiling_is_reached() {
        let mut g = gym(3, 10_000);
        let mut bought = 0;
        for _ in 0..40 {
            let out = turn(&mut g, "5");
            if out[0] == "^6Ты максимально прокачал пресс для своего уровня"
            {
                break;
            }
            bought += 1;
        }
        assert_eq!(bought, 10, "(3 - 2) * 10 at 1000:e884..1000:e889");
        assert_eq!(g.player.armor, 10);
    }

    /// `1000:e912`/`1000:e917` is inside the ceiling branch: the hint
    /// follows the ceiling line while district < 4 and is suppressed from 4.
    #[test]
    fn arm_5_hint_is_suppressed_from_district_four() {
        let mut g = gym(3, 20);
        g.player.armor = 10; // at district 3's ceiling
        assert_eq!(
            turn(&mut g, "5"),
            vec![
                "^6Ты максимально прокачал пресс для своего уровня",
                "^6Качай дальше в следующем районе",
            ],
            "1000:e90d then 1000:e92d"
        );
        assert_eq!(g.player.money, 20, "the ceiling branch charges nothing");

        let mut g = gym(4, 20);
        g.player.armor = 20; // at district 4's ceiling
        assert_eq!(
            turn(&mut g, "5"),
            vec!["^6Ты максимально прокачал пресс для своего уровня"],
            "1000:e917 suppresses 1000:e919 from district 4"
        );
    }

    /// The ceiling test is `1000:e894` and the money test is `1000:e89b`, in
    /// that order.
    #[test]
    fn arm_5_tests_the_ceiling_before_the_money() {
        let mut g = gym(3, 0);
        g.player.armor = 10;
        assert_eq!(
            turn(&mut g, "5")[0],
            "^6Ты максимально прокачал пресс для своего уровня",
            "1000:e90d, not 1000:e8b1"
        );
    }

    // -- the chain itself ------------------------------------------------

    /// At district 1 the `3`, `4` and `5` compares are jumped over
    /// (`1000:e72d`, `1000:e7e7`, `1000:e866`), so the key is never compared
    /// and the line falls through to `1000:e932`. **Nothing is printed and
    /// nothing changes** -- a port that compared the key first and the
    /// district second would print a refusal the original has no path to.
    #[test]
    fn district_one_swallows_three_four_and_five_in_silence() {
        for key in ["3", "4", "5"] {
            let mut g = gym(1, 1_000);
            let before = g.player.clone();
            // The observable claim first, so a port that lost the gate
            // fails on the SILENCE rather than on the predicate below.
            assert!(turn(&mut g, key).is_empty(), "{key} must print nothing");
            assert_eq!(g.player, before, "and change nothing");
            assert!(!g.tooth_guard);
            assert!(!key_dispatches(&g, key), "gate for {key}");
        }
    }

    /// District 2 opens `3` and `4` (`1000:e728`, `1000:e7e2` are both
    /// `cmp ...,0x1`) and still hides `5` (`1000:e861` is `cmp ...,0x2`).
    #[test]
    fn district_two_opens_three_and_four_but_not_five() {
        let g = gym(2, 1_000);
        assert!(key_dispatches(&g, "3"), "1000:e72d");
        assert!(key_dispatches(&g, "4"), "1000:e7e7");
        assert!(!key_dispatches(&g, "5"), "1000:e866 needs district > 2");
        let g = gym(3, 1_000);
        assert!(key_dispatches(&g, "5"));
    }

    /// **There is no `Непонятно` line.** The chain falls off its end at
    /// `1000:e943`, which jumps to the PROMPT; the range holds no literal
    /// for an unrecognised key. The turn must also leave the player in the
    /// gym.
    #[test]
    fn an_unrecognised_key_prints_nothing_and_stays_in_the_gym() {
        for key in ["6", "0", "x", "", "hp"] {
            let mut g = gym(5, 1_000);
            let before = g.player.clone();
            assert!(turn(&mut g, key).is_empty(), "{key:?} must print nothing");
            assert_eq!(g.player, before);
            assert_eq!(g.location, Location::Gym, "1000:e943 returns to the prompt");
        }
    }

    /// `w` is the shared exit at `1000:e93c`, owned by `Game::shop_turn`'s
    /// catch-all rather than by this module, and `key_dispatches` must let
    /// it through to there.
    #[test]
    fn w_still_leaves_the_gym() {
        let mut g = gym(5, 1_000);
        assert!(!key_dispatches(&g, "w"), "1000:e93c is not this module's");
        assert!(turn(&mut g, "w").is_empty());
        assert_eq!(g.location, Location::Street, "1000:e946 -> 1000:e961");
    }

    /// Everything [`key_dispatches`] admits must have an arm in
    /// [`run_key`]: at a district that opens all five, and with money for
    /// the dearest of them, each key prints at least one line. A key added
    /// to one function and not the other fails here.
    #[test]
    fn every_key_the_chain_reaches_has_an_arm() {
        for key in ["1", "2", "3", "4", "5"] {
            let mut g = gym(5, 1_000);
            g.progress.threshold = 10_000;
            assert!(key_dispatches(&g, key), "district 5 opens {key}");
            assert!(
                !turn(&mut g, key).is_empty(),
                "{key} dispatches but run_key does nothing"
            );
        }
    }

    /// The gym's own `ReadLn` does not trim (`1000:e61f call 0eed:0216` only
    /// lowercases), while `Game::shop_turn` trims -- the standing
    /// trimmed-prompt divergence in `docs/re/gaps.md`, whose population the
    /// gym now joins. Pinned so the divergence is measured rather than
    /// remembered: ` 1` is a MISS in the original and a hit here.
    #[test]
    fn the_gym_prompt_accepts_untrimmed_input_the_original_refuses() {
        let mut g = gym(1, 20);
        assert!(!turn(&mut g, " 1").is_empty(), "trimmed here, a miss there");
        assert_eq!(g.player.money, 0);
    }

    /// And it is case-insensitive in both, because `0eed:0216` lowercases
    /// ASCII `A`..`Z` in place.
    #[test]
    fn the_gym_prompt_is_case_insensitive() {
        let mut g = gym(5, 1_000);
        assert!(turn(&mut g, "W").is_empty());
        assert_eq!(g.location, Location::Street);
    }
}
