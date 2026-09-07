//! The club's key dispatch -- `kl`'s arms, `1000:e065`..`1000:e36b`.
//!
//! `crate::game` keeps the verb (`Command::Club` -> `Game::enter_shop`), the
//! two gates in front of it, the two intro lines, the two
//! [`crate::game::IMM_ROWS`] menu rows, the stake init and the prompt; this
//! module is only what happens after the `ReadLn` at
//! `1000:e03e`..`1000:e060`. The map it is written from is `docs/re/club.md`
//! and `data/club_arms.json`, whose `club.what_the_port_must_change` array is
//! the work order and which `python3 tools/test_club_arms.py` and
//! `python3 tools/test_arms_artifacts.py` re-derive from `orig/g.exe`.
//!
//! ## Four keys, and only three of them are this module's
//!
//! **Established from flow** (`data/club_arms.json`'s `key_set`). Each key is
//! a `0f78:0bd8` shortstring compare against the club's own buffer
//! `20ae:3a72`: `p` `1000:e06f`, `1` `1000:e27e`, `2` `1000:e2f3` and the
//! shared `w` at `1000:e361`. `w` belongs to `Game::shop_turn`'s catch-all
//! the same way the gym's `1000:e93c` does, so [`key_dispatches`] returns
//! `false` for it -- that `false` is `1000:e35c`, the fall-through into the
//! exit compare.
//!
//! ## The menu block and the arm block are not the same code
//!
//! `1000:df6f`..`1000:e020` prints the two rows once, on entry;
//! `1000:e065`..`1000:e36b` is the chain of key compares. Three five-byte
//! predicates are byte-identical between them -- the two price tests and the
//! district gate -- and `data/club_arms.json`'s `menu_vs_arm_finding` records
//! that the longest run the two blocks share is 26 bytes at `1000:dfb0` and
//! `1000:e2ce`, ending on the `jbe` opcode whose displacement is the first
//! byte at which they differ. What they DO is different in kind:
//!
//! * in the menu the price test is **cosmetic** -- both arms of `1000:df74`
//!   and `1000:dfd0` store a colour digit into `20ae:3b7a` and reconverge,
//!   so no club row is ever hidden by price;
//! * the menu's district gate skips a PRINT (`1000:dfc9 jbe 0xe020`), the
//!   arm's skips a key COMPARE (`1000:e2e7 jbe 0xe357`).
//!
//! So `Game::imm_row_visible` owns the row predicates and nothing here calls
//! it.
//!
//! ## Three things this module deliberately does not do
//!
//! * **No refusal for the key the district hides.** At district 1
//!   `1000:e2e7` jumps past the `2` compare at `1000:e2f3` entirely, so the
//!   key is never compared and nothing is printed. [`key_dispatches`]
//!   evaluates the district before the key for exactly that reason, and
//!   there is no "wrong district" literal anywhere in the range for a
//!   refusal to use.
//! * **No message for an unrecognised key.** `1000:e368` is the loop's back
//!   edge and it targets the PROMPT at `1000:e025`, not the menu; the
//!   CS-literal sweep over `1000:df06`..`1000:e390` is a set equality and
//!   finds no refusal literal for a bad key.
//! * **No re-print of the menu between turns.** Same back edge.
//!
//! ## The stake is per VISIT, not per turn
//!
//! `1000:e020 mov byte [0x3c82],0x5` is five bytes before the loop top at
//! `1000:e025` and is the join point of the menu's district gate, so it runs
//! once per entry. [`crate::game::Game::club_stake`] is that byte; resetting
//! it at the top of each prompt iteration would make the whole `p` arm
//! unreachable past its first hand, and it is deliberately absent from
//! `data/save_layout.json` because the original never saves it.
//!
//! Address convention: `docs/re/METHODOLOGY.md`, "Address convention, and
//! its range of validity"; `python3 tools/re_query.py resolve <citation>`
//! converts one and prints the bytes there. Every string literal below is
//! quoted from `data/strings.json` at the file offset its `mov di,<n>` push
//! resolves to, markup and trailing spaces included, and each inline
//! citation is written as ``file `0xNNNN` `^Nthe string``` on one line
//! directly above the `term::print`/`term::println` that prints it, which is
//! the shape `tools/test_string_citations.py` can actually resolve (the
//! shorter spelling reports nothing at all, which is how a whole module once
//! passed a guard whose entire purpose is to catch a wrong offset).

use crate::game::Game;
use crate::progress;
use crate::term;
use crate::text;
use std::io;

/// Whether the club's compare chain reaches a compare that `key` matches.
///
/// **Established from flow.** The chain is `p` (`1000:e06f`), `1`
/// (`1000:e27e`), `2` (`1000:e2f3`) and `w` (`1000:e361`), in that order,
/// and exactly one of them sits behind a gate that decides whether the
/// compare happens at all:
///
/// | key | gate | branch | sense |
/// |---|---|---|---|
/// | `2` | `1000:e2e2` `cmp byte [0x3692],0x1` | `1000:e2e7` `jbe 0xe357` | district > 1 |
///
/// So at district 1 exactly two keys exist here -- `p` and `1` -- and that
/// is a fact about the DISPATCHER, not about what the menu printed. `&&`
/// short-circuits left to right, which is the original's order: gate, then
/// compare.
pub(crate) fn key_dispatches(g: &Game, key: &str) -> bool {
    match key {
        // 1000:e06f / 1000:e074 -- no gate of its own.
        "p" => true,
        // 1000:e27e / 1000:e283 -- no gate of its own.
        "1" => true,
        // 1000:e2e2 gates the compare at 1000:e2f3; 1000:e2e7 jumps past it
        // to the `w` compare's own setup at 1000:e357, and 1000:e2f8 is the
        // compare's own miss to the same place when the district opens it.
        "2" => g.district > 1,
        // 1000:e361: the shared `w` compare, then 1000:e368 back to the
        // prompt.
        _ => false,
    }
}

/// Run the arm `key` selected. Only ever called when [`key_dispatches`] said
/// the chain reaches that key's compare, which is where the district gate
/// lives; the arms below carry only their own gates.
///
/// `lines` is threaded through because the `p` arm can reach a fight
/// (`1000:e222`), which reads the combat prompt.
pub(crate) fn run_key(
    g: &mut Game,
    key: &str,
    lines: &mut dyn Iterator<Item = io::Result<String>>,
) -> io::Result<()> {
    match key {
        "p" => play_cards(g, lines),
        "1" => {
            dance(g);
            Ok(())
        }
        "2" => {
            learn_tricks(g);
            Ok(())
        }
        _ => Ok(()),
    }
}

/// `p` -- `1000:e065`..`1000:e274`, the card game.
///
/// **Established from flow**, re-disassembled for this task with
/// `python3 tools/re_query.py resolve 1000:e065 -n 560 -i 330`:
///
/// ```text
/// e079  mov al,[0x3c82] / e07c xor ah,ah / e07e cmp ax,[0x38c7]
/// e082  jle 0xe087                         ; stake <= money, SIGNED
/// e084  jmp 0xe258                          ; the refusal, with the stake
/// e087  mov di,0xa1ea .. e09e WriteLn       ; `Ты поставил # рублей`
/// e0a3  mov al,[0x3c82] / e0a8 sub [0x38c7],ax
/// e0ac  mov al,[0x3692] / e0b1 mov dx,0xc / e0b4 mul dx
/// e0b7  call 0f78:114b                      ; Random(district * 12)
/// e0bc  xor dx,dx / e0be mov cx,ax / e0c0 mov bx,dx
/// e0c2  mov ax,[0x38a4] / e0c5 cwd
/// e0c6  cmp dx,bx / e0c8 jnle 0xe0d0 / e0ca jl 0xe129
/// e0cc  cmp ax,cx / e0ce jb 0xe129
/// ```
///
/// **The compare is the 32-bit idiom [`Game::luck_below_random_32`] models,
/// permuted a THIRD way and used with the opposite sense.** The den's first
/// copy is `jl` / `jle` / `jb` (`1000:dda8`, `1000:ddaa`, `1000:ddb1`) and
/// its second `jl` / `jnle` / `jnb` (`1000:ddeb`, `1000:dded`, `1000:ddf1`);
/// the club's is `jnle` / `jl` / `jb`. The operands are the same two: удача
/// `20ae:38a4` SIGN-extended by the `cwd` at `1000:e0c5`, the draw
/// ZERO-extended by the `xor dx,dx` at `1000:e0bc` -- which is why a `jl`
/// sits beside a `jb`. What differs is where the fall-through goes: both
/// `jb`/`jnb` here take the "luck below the draw" case to `1000:e129`, the
/// LOSS, so the club wins when the predicate is **false**. Writing
/// `if luck_below_random_32(..)` around the payout would invert the game.
///
/// `1000:e0b7` is the only `Random` call site in `1000:df06`..`1000:e390`
/// (`data/club_arms.json`'s `sweeps.random_call_sites`), and
/// `python3 tools/re_query.py pushed-n 1000:e0b7` re-derives its `n` as
/// `byte[0x3692] * 12`.
///
/// **`money += stake * 2` after `money -= stake` is a net `+stake`, and both
/// are kept.** Collapsing them changes nothing observable and loses the
/// debit the gate at `1000:e082` is measured against on the NEXT hand.
///
/// **The `#` of `^2Ты выиграл # рублей ` is the STAKE**, read at
/// `1000:e0e0` before `1000:e0f7` raises it -- the net gain, not the doubled
/// credit.
fn play_cards(g: &mut Game, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
    let stake = i32::from(g.club_stake);
    // 1000:e079..1000:e082 -- the operands are the other way round from
    // every other money gate in the image: the STAKE is in `ax` and `jle`
    // passes when it is <= money, so equality buys.
    if stake > g.player.money {
        // 1000:e258 pushes file `0xBBA4` `^6Не хватает денег - надо #.`,
        // whose `#` is 1000:e25d's read of the stake; printed by 1000:e26f.
        term::println(&text::fill(
            "^6Не хватает денег - надо #.",
            &[i64::from(stake)],
        ));
        return Ok(());
    }
    // 1000:e087 pushes file `0xBABA` `Ты поставил # рублей`, `#` from
    // 1000:e08c; printed by 1000:e09e.
    term::println(&text::fill("Ты поставил # рублей", &[i64::from(stake)]));
    g.player.money -= stake; // 1000:e0a8

    // 1000:e0ac..1000:e0b4 build the `n`; 1000:e0b7 is the draw.
    let draw = g.rng.below_at("1000:e0b7", u16::from(g.district) * 12);
    // 1000:e0c6..1000:e0ce -- the WIN is the fall-through, i.e. the
    // predicate being FALSE. See the doc above.
    if Game::luck_below_random_32(g.player.luck, draw) {
        // 1000:e129 pushes file `0xBAE6` `^4Ты проиграл # рублей`, `#` from
        // 1000:e12e; printed by 1000:e140.
        term::println(&text::fill("^4Ты проиграл # рублей", &[i64::from(stake)]));
        // 1000:e145 -- and nothing else. The money already left at
        // 1000:e0a8; the lose path 1000:e129..1000:e14a carries zero
        // instructions that touch `20ae:38c7`.
        g.club_stake = 5;
    } else {
        // 1000:e0d3/1000:e0d5/1000:e0d7 -- the doubled credit, kept beside
        // the debit above.
        g.player.money += stake * 2;
        // 1000:e0db pushes file `0xBACF` `^2Ты выиграл # рублей ` (the
        // trailing space is the original's), `#` from 1000:e0e0 -- the
        // stake BEFORE 1000:e0f7 raises it; printed by 1000:e0f2.
        term::println(&text::fill("^2Ты выиграл # рублей ", &[i64::from(stake)]));
        g.club_stake += 2; // 1000:e0f7
        let xp = u32::from(g.district); // 1000:e101..1000:e104

        // 1000:e0fc pushes file `0xA95B` `^6Ты получаешь # качков опыта`,
        // printed by 1000:e113 -- BEFORE 1000:e11d credits the same value.
        term::println(&text::fill(
            "^6Ты получаешь # качков опыта",
            &[i64::from(xp)],
        ));
        // 1000:e11d `add [0x38ce],ax` is the xp credit and 1000:e124 is the
        // level-up call. **The two parameters are not the same thing**:
        // `xp` here is [`progress::apply_levels`]'s `award`, which models the
        // `add` at 1000:e11d, while the `false` is the original's own
        // `param_1 = 0` set at 1000:e121 -- the CAPPED form (1000:257a),
        // not an award of zero. `Game::den_job` pairs 1000:debe with
        // 1000:dec5 the same way; `crate::gym`'s `3` arm adds first and
        // passes `award = 0` instead, which is the other faithful spelling.
        // 1000:e124 has no outer threshold guard of its own.
        progress::apply_levels(&mut g.progress, &mut g.player, &mut g.rng, xp, false);
    }

    // 1000:e14a / 1000:e14f then 1000:e151 / 1000:e156 -- both jump to
    // 1000:e174, so the line prints only while 5 < stake < 17. After a loss
    // the stake is 5 and the message never announces a stake of 5.
    if g.club_stake < 17 && g.club_stake > 5 {
        // 1000:e158 pushes file `0xBAFD`
        // `^6Ставки изменились. Теперь ставка - #`, `#` from 1000:e15d;
        // printed by 1000:e16f.
        term::println(&text::fill(
            "^6Ставки изменились. Теперь ставка - #",
            &[i64::from(g.club_stake)],
        ));
    }
    // 1000:e174 / 1000:e179 -- `jnb`, so 17 exactly is caught.
    if g.club_stake >= 17 {
        caught_cheating(g, lines)?;
    }
    Ok(())
}

/// `1000:e17e`..`1000:e256` -- six wins in a row and the house calls it.
///
/// The stake ladder is 5 -> 7 -> 9 -> 11 -> 13 -> 15 -> 17 (`1000:e020`
/// `:= 5`, `1000:e0f7` `+= 2`, `1000:e145` `:= 5`), so this block is not a
/// random event: it is the punishment for winning six hands without losing
/// one.
///
/// **Established from flow:**
///
/// ```text
/// e17e  mov al,0x1 / e181 call 0x10d14   ; FUN_1000_0d14(1), clamp-to-7
/// e184  mov byte [0x3b72],0x1            ; the fight-accepted flag
/// e189  mov di,0xa254 .. e19d WriteLn    ; `^4Козёл! Да ты мухлевал!`
/// e1a8  mov di,0x90c0 / e1b2 mov di,[0x3952] / e1c5 mov di,0x90c7
/// e1cf  push [0x395c] .. e1df WriteLn    ; the opponent announcement
/// e1e4  mov di,0xa26d / e1e9..e1f4 district*5 .. e203 WriteLn
/// e208..e215  add [0x38ce],ax            ; xp += district*5
/// e219  mov al,0x0 / e21c call 0x12526   ; FUN_1000_2526(0)
/// e21f  mov al,0x2 / e222 call 0x13d11   ; FUN_1000_3d11(2) -- the fight
/// e225  mov di,0xa29c .. e239 WriteLn
/// e23e  mov byte [0x3b77],0x5            ; the club ban countdown
/// e243  mov di,0x848e / e248 mov di,0x3a72 / e251 call 0f78:0b01
/// ```
///
/// **The XP is awarded BEFORE the fight, not after it.** `1000:e21c` runs
/// and `1000:e222` follows it. That is invisible on screen unless the award
/// crosses a level threshold, which is exactly why it is written down.
///
/// **`1000:e181`'s `param_1 = 1` is the clamp-to-class-7 form**
/// (`1000:0da7`/`1000:0dba`), so this fight never draws a Мент; the den's
/// `1000:dc0e` is the same reading, and the 66-byte announcement run at
/// `1000:e1a2` occurs at exactly three addresses image-wide -- `1000:c3f1`,
/// `1000:dc16` and `1000:e1a2`, the three `param_1 = 1` sites.
///
/// **`1000:e23e` is the only writer of `20ae:3b77`** outside the walk
/// decrement at `1000:b17e` and the district reset at `1000:abd3`, so
/// landing it turns `docs/re/gaps.md`'s "two ban countdowns ... never set"
/// into one live countdown -- but only because the GATE at `1000:df1a`
/// lands with it in [`Game::enter_shop`]. Setting the countdown without the
/// gate would be worse than neither.
///
/// **The block ends the visit without the player typing anything.**
/// `1000:e251` is `rtl_str_assign_max`, whose SOURCE is the first push
/// (`0f78:0b06 lds si,[ss:bx+0xa]`) and DESTINATION the second
/// (`0f78:0b0a les di,[ss:bx+0x6]`) -- the reverse of the argument order
/// `0f78:0ae7` uses -- so it writes `w` INTO the club's own buffer
/// `20ae:3a72`. The `1` compare at `1000:e27e` then misses, the `2` compare
/// at `1000:e2f3` misses or is skipped, and the `w` compare at `1000:e361`
/// hits. `Game::leave_shop` is that consequence; a message telling the
/// player to leave would not be.
fn caught_cheating(
    g: &mut Game,
    lines: &mut dyn Iterator<Item = io::Result<String>>,
) -> io::Result<()> {
    // 1000:e17e / 1000:e181 -- FUN_1000_0d14(1).
    let enemy = g.roll_enemy(1);
    g.fight_accepted_3b72 = true; // 1000:e184

    // 1000:e189 pushes file `0xBB24` `^4Козёл! Да ты мухлевал!`, printed by
    // 1000:e19d.
    term::println("^4Козёл! Да ты мухлевал!");
    // 1000:e1a8 pushes file `0xA990` `^6Это `, 1000:e1b2..1000:e1ba indexes
    // the rank table, 1000:e1c5 pushes file `0xA997` ` # уровня.` with
    // 1000:e1cf's `20ae:395c` as its `#`; one WriteLn at 1000:e1df.
    term::print("^6Это ");
    term::print(&Game::rank_name(enemy.class));
    term::println(&text::fill(" # уровня.", &[i64::from(enemy.level)]));

    // 1000:e1e9..1000:e1f4 -- `mov si,ax` / `shl` / `shl` / `add ax,si`.
    let xp = u32::from(g.district) * 5;
    // 1000:e1e4 pushes file `0xBB3D`
    // `^6Ты получаешь # качков опыта за победу в игре`, printed by
    // 1000:e203 -- BEFORE 1000:e215 credits it.
    term::println(&text::fill(
        "^6Ты получаешь # качков опыта за победу в игре",
        &[i64::from(xp)],
    ));
    // 1000:e215 is the credit and 1000:e21c the call; `xp` is
    // `apply_levels`'s `award` (modelling the `add`) and `false` is the
    // original's `param_1 = 0` from 1000:e219 -- see `play_cards` above.
    progress::apply_levels(&mut g.progress, &mut g.player, &mut g.rng, xp, false);

    // 1000:e21f / 1000:e222 -- FUN_1000_3d11(2), AFTER the level-up.
    g.run_combat(2, enemy, lines)?;

    // 1000:e225 pushes file `0xBB6C`
    // `^6Уноси ноги, пока не отобрали деньги другие канадидаты`, printed by
    // 1000:e239.
    term::println("^6Уноси ноги, пока не отобрали деньги другие канадидаты");
    g.club_ban_countdown = 5; // 1000:e23e

    // 1000:e243..1000:e251 -- the `w` written into 20ae:3a72.
    g.leave_shop();
    Ok(())
}

/// `1` -- `1000:e274`..`1000:e2e2`, `потусоваться на дискотеке`, 15 rubles.
///
/// **Established from flow**, same decode:
///
/// ```text
/// e285  cmp word [0x38c7],0xf / e28a jnl 0xe2a7   ; can pay 15, SIGNED
/// e28c  mov di,0x8e4d (file 0xA71D) .. e2a0 WriteLn / e2a5 jmp short 0xe2e2
/// e2a7  sub word [0x38c7],0xf
/// e2ac  mov di,0xa2f1 (file 0xBBC1) .. e2c0 WriteLn
/// e2c5  inc [0x38a0]                              ; Ловкость +1
/// e2c9  mov di,0x940d (file 0xACDD) .. e2dd WriteLn
/// ```
///
/// The refusal literal is `^4Не хватает` -- the same one gym arms `1` and
/// `2` use, and none of the gym's other three. The string says +1 and
/// `1000:e2c5` is an increment, so string and effect agree; that is checked,
/// not assumed. Nothing one-shot is consumed, so the arm repeats.
fn dance(g: &mut Game) {
    // 1000:e285 / 1000:e28a -- a signed word compare in the original; money
    // is an i32 here, which is the standing width divergence.
    if g.player.money < 15 {
        // 1000:e28c pushes file `0xA71D` `^4Не хватает`, printed by
        // 1000:e2a0; 1000:e2a5 leaves.
        term::println("^4Не хватает");
        return;
    }
    g.player.money -= 15; // 1000:e2a7

    // 1000:e2ac pushes file `0xBBC1` `^2Ты прокачиваешь ловкость.`, printed
    // by 1000:e2c0 -- BEFORE the store.
    term::println("^2Ты прокачиваешь ловкость.");
    g.player.agility += 1; // 1000:e2c5

    // 1000:e2c9 pushes file `0xACDD` `^1Ловкость +1 ` (the trailing space is
    // the original's), printed by 1000:e2dd.
    term::println("^1Ловкость +1 ");
}

/// `2` -- `1000:e2e2`..`1000:e357`, `разузнать приемы мухлёжников`,
/// 22 rubles, behind the district gate.
///
/// **Established from flow**, same decode:
///
/// ```text
/// e2fa  cmp word [0x38c7],0x16 / e2ff jnl 0xe31c
/// e301  mov di,0x8e4d (file 0xA71D) .. e315 WriteLn / e31a jmp short 0xe357
/// e31c  sub word [0x38c7],0x16
/// e321  mov di,0xa30d (file 0xBBDD) .. e335 WriteLn
/// e33a  inc [0x38a4]                              ; Удача +1
/// e33e  mov di,0x942c (file 0xACFC) .. e352 WriteLn
/// ```
///
/// **`1000:e33a` raises the very byte the `p` arm's compare reads at
/// `1000:e0c2`**, so the two arms are coupled: 22 rubles at district 2 or
/// higher buys a permanently better card game. The gate that stands in front
/// of this arm is [`key_dispatches`]'s, not this function's, because in the
/// original it decides whether `1000:e2f3` is reached at all.
fn learn_tricks(g: &mut Game) {
    // 1000:e2fa / 1000:e2ff.
    if g.player.money < 22 {
        // 1000:e301 pushes file `0xA71D` `^4Не хватает`, printed by
        // 1000:e315; 1000:e31a leaves.
        term::println("^4Не хватает");
        return;
    }
    g.player.money -= 22; // 1000:e31c

    // 1000:e321 pushes file `0xBBDD` `^2Ты прокачиваешь удачу.`, printed by
    // 1000:e335.
    term::println("^2Ты прокачиваешь удачу.");
    g.player.luck += 1; // 1000:e33a

    // 1000:e33e pushes file `0xACFC` `^1Удача +1 ` (the trailing space is
    // the original's), printed by 1000:e352.
    term::println("^1Удача +1 ");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locations::Location;
    use crate::model::Fighter;
    use crate::progress::Progress;
    use crate::term::capture;

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

    /// A club-ready game: the club discovered (`20ae:3699`), the ban
    /// countdown clear (`20ae:3b77`), the player standing in it at
    /// `district` with `money` in the pocket and the stake at its
    /// entry value.
    fn club(district: u8, money: i32) -> Game {
        let mut g = Game::new(player(), Progress::new(), 12345);
        g.district = district;
        g.player.money = money;
        g.places.mark_found(Location::Club); // 20ae:3699, gate 1000:df10
        g.location = Location::Club;
        g.club_stake = 5; // 1000:e020
        g.progress.threshold = 100_000; // keeps 1000:e124 from levelling
        g
    }

    fn no_input() -> std::iter::Empty<std::io::Result<String>> {
        std::iter::empty()
    }

    /// One turn at the club prompt, through the real dispatch in
    /// `Game::shop_turn` rather than through [`run_key`] directly, so the
    /// wiring is under test too.
    fn turn(g: &mut Game, key: &str) -> Vec<String> {
        capture::lines(|| {
            g.shop_turn(Location::Club, key, &mut no_input()).unwrap();
        })
    }

    // -- arm `1` ---------------------------------------------------------

    /// `1000:e285` is `cmp ...,0xf` and `1000:e28a` is `jnl`, so 14 refuses
    /// and 15 buys; `1000:e2c5` is the increment.
    #[test]
    fn arm_1_refuses_at_fourteen_and_raises_agility_at_fifteen() {
        let mut g = club(1, 14);
        let a0 = g.player.agility;
        assert_eq!(turn(&mut g, "1"), vec!["^4Не хватает"], "1000:e2a0");
        assert_eq!(g.player.money, 14, "no debit on the refusal");
        assert_eq!(g.player.agility, a0, "and no effect either");

        let mut g = club(1, 15);
        assert_eq!(
            turn(&mut g, "1"),
            vec!["^2Ты прокачиваешь ловкость.", "^1Ловкость +1 "],
            "1000:e2c0 then 1000:e2dd"
        );
        assert_eq!(g.player.money, 0, "1000:e2a7 sub 0xf");
        assert_eq!(g.player.agility, a0 + 1, "1000:e2c5");
    }

    // -- arm `2` ---------------------------------------------------------

    /// `1000:e2fa` is `cmp ...,0x16`; `1000:e33a` raises удача, which is the
    /// byte the `p` arm's compare reads at `1000:e0c2`.
    #[test]
    fn arm_2_refuses_at_twentyone_and_raises_luck_at_twentytwo() {
        let mut g = club(2, 21);
        let l0 = g.player.luck;
        assert_eq!(turn(&mut g, "2"), vec!["^4Не хватает"], "1000:e315");
        assert_eq!(g.player.luck, l0);

        let mut g = club(2, 22);
        assert_eq!(
            turn(&mut g, "2"),
            vec!["^2Ты прокачиваешь удачу.", "^1Удача +1 "],
            "1000:e335 then 1000:e352"
        );
        assert_eq!(g.player.money, 0, "1000:e31c sub 0x16");
        assert_eq!(g.player.luck, l0 + 1, "1000:e33a");
    }

    /// At district 1 `1000:e2e7 jbe 0xe357` jumps PAST the `2` compare at
    /// `1000:e2f3`, so the key is never compared. **Nothing is printed and
    /// nothing changes** -- there is no "wrong district" literal in
    /// `1000:df06`..`1000:e390` for a refusal to use, so a port that
    /// compared the key first and the district second would invent one.
    #[test]
    fn district_one_swallows_the_two_key_in_silence() {
        let mut g = club(1, 1_000);
        let before = g.player.clone();
        assert!(turn(&mut g, "2").is_empty(), "1000:e2e7 skips the compare");
        assert_eq!(g.player, before);
        assert!(!key_dispatches(&g, "2"), "gate 1000:e2e2");
        let g = club(2, 1_000);
        assert!(key_dispatches(&g, "2"), "district 2 opens it");
    }

    // -- arm `p` ---------------------------------------------------------

    /// `1000:e082` is `jle` with the STAKE in `ax`, so `stake <= money`
    /// passes and equality buys. At the entry stake of 5 that is 4 refusing
    /// and 5 playing, and the refusal names the stake.
    #[test]
    fn arm_p_refuses_below_the_stake_and_equality_buys() {
        let mut g = club(1, 4);
        assert_eq!(
            turn(&mut g, "p"),
            vec!["^6Не хватает денег - надо 5."],
            "1000:e26f, the # is 1000:e25d's stake"
        );
        assert_eq!(g.player.money, 4, "1000:e0a8 is past the refusal");

        let mut g = club(1, 5);
        assert!(!turn(&mut g, "p").is_empty(), "5 <= 5 buys");
    }

    /// **The WIN is the fall-through of `1000:e0ce`, i.e.
    /// `luck_below_random_32` being FALSE.** With удача far above any
    /// `Random(district * 12)` the hand must be won, and a port that used
    /// the predicate the other way round would lose every hand here.
    #[test]
    fn arm_p_wins_when_luck_is_not_below_the_draw() {
        let mut g = club(1, 100);
        g.player.luck = 30_000; // never below Random(1 * 12)
        let out = turn(&mut g, "p");
        assert_eq!(
            out,
            vec![
                "Ты поставил 5 рублей",
                "^2Ты выиграл 5 рублей ",
                "^6Ты получаешь 1 качков опыта",
                "^6Ставки изменились. Теперь ставка - 7",
            ],
            "1000:e09e, 1000:e0f2, 1000:e113, 1000:e16f"
        );
        // -5 at 1000:e0a8 then +10 at 1000:e0d7 -- a net +5, and both are
        // kept because the debit is what the NEXT hand's gate measures.
        assert_eq!(g.player.money, 105, "1000:e0a8 then 1000:e0d7");
        assert_eq!(g.club_stake, 7, "1000:e0f7");
        assert_eq!(g.progress.xp, 1, "1000:e11d, district 1");
    }

    /// The LOSS path is one line and one store: `1000:e145` resets the stake
    /// and the money already left at `1000:e0a8`. And because the stake is
    /// back to 5, `1000:e156` suppresses the "stakes changed" line -- the
    /// message never announces a stake of 5.
    #[test]
    fn arm_p_loses_with_zero_further_money_instructions_and_no_stake_line() {
        let mut g = club(3, 100);
        g.player.luck = 0; // always below Random(3 * 12) unless it draws 0
        g.club_stake = 9;
        loop {
            let out = turn(&mut g, "p");
            if out.iter().any(|l| l.starts_with("^4Ты проиграл")) {
                assert_eq!(
                    out,
                    vec!["Ты поставил 9 рублей", "^4Ты проиграл 9 рублей"],
                    "1000:e09e then 1000:e140, and 1000:e156 suppresses \
                     1000:e158"
                );
                assert_eq!(g.player.money, 91, "only 1000:e0a8 debited");
                assert_eq!(g.club_stake, 5, "1000:e145");
                break;
            }
            g.club_stake = 9;
            g.player.money = 100;
        }
    }

    /// `1000:e14a`/`1000:e14f` and `1000:e151`/`1000:e156` both jump to
    /// `1000:e174`, so the line prints only while `5 < stake < 17`. Driven
    /// directly at the two boundaries the ladder reaches.
    #[test]
    fn the_stakes_changed_line_prints_only_strictly_between_five_and_seventeen() {
        for (before, printed) in [(5u8, true), (13, true), (15, false)] {
            let mut g = club(1, 10_000);
            g.player.luck = 30_000; // always a win
            g.club_stake = before;
            let out = turn(&mut g, "p");
            assert_eq!(
                out.iter().any(|l| l.starts_with("^6Ставки изменились")),
                printed,
                "stake {before} -> {} at 1000:e14a/1000:e151",
                before + 2
            );
        }
    }

    /// Six wins in a row walk the stake 5 -> 7 -> 9 -> 11 -> 13 -> 15 -> 17
    /// (`1000:e020`, six times `1000:e0f7`), and 17 is where `1000:e179`
    /// `jnb` reaches the caught-cheating block. It sets the ban countdown
    /// `20ae:3b77` at `1000:e23e` and writes `w` into the buffer at
    /// `1000:e251`, so the visit ends without the player typing anything.
    #[test]
    fn six_wins_reach_the_caught_block_which_bans_and_ejects() {
        let mut g = club(1, 10_000);
        g.player.luck = 30_000; // always a win
        g.player.hp = 1; // lose the fight fast; the block runs either way
        let mut stakes = vec![g.club_stake];
        for _ in 0..6 {
            let out = turn(&mut g, "p");
            stakes.push(g.club_stake);
            if out.iter().any(|l| l == "^4Козёл! Да ты мухлевал!") {
                break;
            }
        }
        assert_eq!(
            stakes,
            vec![5, 7, 9, 11, 13, 15, 17],
            "1000:e020 then six 1000:e0f7"
        );
        assert_eq!(g.club_ban_countdown, 5, "1000:e23e");
        assert_eq!(g.location, Location::Street, "1000:e251 writes `w`");
    }

    /// The caught block's order: roll, flag, accusation, announcement, the
    /// xp line, the level-up, **then** the fight, then the closing line,
    /// then the ban. The XP being awarded BEFORE the fight is invisible on
    /// screen unless the award crosses a threshold, so it is asserted
    /// directly: with a threshold the award crosses, the level must already
    /// have risen by the time the fight's own output appears.
    #[test]
    fn the_caught_block_awards_the_xp_before_the_fight() {
        let mut g = club(3, 10_000);
        g.player.luck = 30_000;
        g.club_stake = 15;
        g.progress.xp = 0;
        g.progress.threshold = 3; // district*5 = 15 crosses it
        let level0 = g.player.level;
        g.player.hp = 1;
        let out = turn(&mut g, "p");
        assert!(
            out.iter()
                .any(|l| l == "^6Ты получаешь 15 качков опыта за победу в игре"),
            "1000:e203, district 3 * 5"
        );
        assert!(
            g.player.level > level0,
            "1000:e21c ran, and it ran before 1000:e222"
        );
        assert!(g.fight_accepted_3b72, "1000:e184");
    }

    // -- the chain itself ------------------------------------------------

    /// **There is no `не понял` line.** The chain falls off its end at
    /// `1000:e368`, which jumps to the PROMPT at `1000:e025`; the CS-literal
    /// sweep over the whole range finds no refusal literal for a bad key.
    /// The turn must also leave the player in the club.
    #[test]
    fn an_unrecognised_key_prints_nothing_and_stays_in_the_club() {
        for key in ["3", "0", "x", "", "hp"] {
            let mut g = club(5, 1_000);
            let before = g.player.clone();
            assert!(turn(&mut g, key).is_empty(), "{key:?} must print nothing");
            assert_eq!(g.player, before);
            assert_eq!(g.location, Location::Club, "1000:e368 -> 1000:e025");
        }
    }

    /// `w` is the shared exit at `1000:e361`, owned by `Game::shop_turn`'s
    /// catch-all rather than by this module.
    #[test]
    fn w_still_leaves_the_club() {
        let mut g = club(5, 1_000);
        assert!(!key_dispatches(&g, "w"), "1000:e361 is not this module's");
        assert!(turn(&mut g, "w").is_empty());
        assert_eq!(g.location, Location::Street, "1000:e366 -> 1000:e36b");
    }

    /// Everything [`key_dispatches`] admits must have an arm in
    /// [`run_key`]: at a district that opens all three, and with money for
    /// the dearest of them, each key prints at least one line.
    #[test]
    fn every_key_the_chain_reaches_has_an_arm() {
        for key in ["p", "1", "2"] {
            let mut g = club(5, 1_000);
            assert!(key_dispatches(&g, key), "district 5 opens {key}");
            assert!(
                !turn(&mut g, key).is_empty(),
                "{key} dispatches but run_key does nothing"
            );
        }
    }

    /// The club's own `ReadLn` at `1000:e060` does not trim (`0eed:0216`
    /// lowercases ASCII `A`..`Z` and compares against no `0x20`), while
    /// `Game::shop_turn` trims -- the standing trimmed-prompt divergence in
    /// `docs/re/gaps.md`, whose population the club now joins. Pinned so it
    /// is measured rather than remembered: ` 1` is a MISS in the original
    /// and a hit here.
    #[test]
    fn the_club_prompt_accepts_untrimmed_input_the_original_refuses() {
        let mut g = club(1, 15);
        assert!(!turn(&mut g, " 1").is_empty(), "trimmed here, a miss there");
        assert_eq!(g.player.money, 0);
    }

    /// And it is case-insensitive in both, because `0eed:0216` lowercases.
    #[test]
    fn the_club_prompt_is_case_insensitive() {
        let mut g = club(5, 1_000);
        assert!(turn(&mut g, "W").is_empty());
        assert_eq!(g.location, Location::Street);
    }
}
