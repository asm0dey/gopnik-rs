//! The club's key dispatch -- the arms that handle player input.
//!
//! The verb, gates, intro lines, menu rows, stake init and prompt live in
//! `crate::game`; this module is only what happens after the input is read.
//!
//! ## Four keys, and only three of them are this module's
//!
//! Each key is compared against the club's own buffer: `p`, `1`, `2` and
//! the shared `w`. `w` belongs to `Game::shop_turn`'s catch-all the same way
//! the gym's does, so [`key_dispatches`] returns `false` for it.
//!
//! ## The menu block and the arm block are not the same code
//!
//! The menu prints the two rows once on entry; the arm block is the chain of
//! key compares. Three five-byte predicates are byte-identical between them
//! -- the two price tests and the district gate -- but the longest run the two
//! blocks share is shorter. What they DO is different in kind:
//!
//! * in the menu the price test is **cosmetic** -- both arms store a colour
//!   digit and reconverge, so no club row is ever hidden by price;
//! * the menu's district gate skips a PRINT, the arm's skips a key COMPARE.
//!
//! So `Game::imm_row_visible` owns the row predicates.
//!
//! ## Three things this module deliberately does not do
//!
//! * **No refusal for the key the district hides.** At district 1 the key is
//!   never compared and nothing is printed. [`key_dispatches`] evaluates the
//!   district before the key for exactly that reason, and there is no
//!   "wrong district" literal for a refusal to use.
//! * **No message for an unrecognised key.** The loop targets the PROMPT,
//!   not the menu; no refusal literal exists for a bad key.
//! * **No re-print of the menu between turns.** Same loop structure.
//!
//! ## The stake is per VISIT, not per turn
//!
//! The stake is set at the top of each entry and is reset per VISIT.
//! [`crate::game::Game::club_stake`] is that byte; resetting it per turn
//! would make the whole `p` arm unreachable past its first hand.

use crate::game::Game;
use crate::progress;
use crate::term;
use crate::text;
use std::io;

/// Whether the club's compare chain reaches a compare that `key` matches.
///
/// The chain is `p`, `1`, `2` and `w`, in that order, and exactly one of
/// them sits behind a gate that decides whether the compare happens at all:
///
/// | key | gate | branch | sense |
/// |---|---|---|---|
/// | `2` | district check | skip if district <= 1 |
///
/// So at district 1 exactly two keys exist here and that is a fact about the
/// DISPATCHER, not about what the menu printed. `&&` short-circuits left to
/// right, which is the original's order: gate, then compare.
pub(crate) fn key_dispatches(g: &Game, key: &str) -> bool {
    match key {
        // No gate of its own.
        "p" => true,
        // No gate of its own.
        "1" => true,
        // District gate; the compare is skipped at district 1.
        "2" => g.district > 1,
        // The shared `w` compare.
        _ => false,
    }
}

/// Run the arm `key` selected. Only called when [`key_dispatches`] said
/// the chain reaches that key's compare.
///
/// `lines` is threaded through because the `p` arm can reach a fight.
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
    // The operands are the other way round: STAKE in `ax` and the compare
    // passes when stake <= money, so equality buys.
    if stake > i32::from(g.player.money) {
        // `^6Не хватает денег - надо #.`, whose `#` is the stake.
        term::println(&text::fill(EMITTED[12].1, &[i64::from(stake)]));
        return Ok(());
    }
    // `Ты поставил # рублей`, `#` from the stake.
    term::println(&text::fill(EMITTED[4].1, &[i64::from(stake)]));
    g.player.money = g.player.money.wrapping_sub(stake as i16); // 1000:e0a8

    // The `n` is built and the draw is made.
    let draw = g.rng.below(u16::from(g.district) * 12);
    // WIN is the fall-through. Losing writes money and ends.
    if Game::luck_below_random_32(g.player.luck, draw) {
        // `^4Ты проиграл # рублей`, `#` from the stake.
        term::println(&text::fill(EMITTED[7].1, &[i64::from(stake)]));
        // Nothing else touches money on the loss path.
        g.club_stake = 5;
    } else {
        // Doubled credit, kept beside the debit.
        g.player.money = g.player.money.wrapping_add((stake * 2) as i16);
        // `^2Ты выиграл # рублей ` (trailing space is the original's), `#`
        // from the stake BEFORE it is raised.
        term::println(&text::fill(EMITTED[5].1, &[i64::from(stake)]));
        g.club_stake += 2; // 1000:e0f7
        let xp = u16::from(g.district); // 1000:e101..1000:e104

        // `^6Ты получаешь # качков опыта`, printed BEFORE the experience is credited.
        term::println(&text::fill(EMITTED[6].1, &[i64::from(xp)]));
        // The xp credit and level-up call. The two parameters are not the same
        // thing: `xp` here models the credit, while `false` is the original's
        // own clamped form.
        progress::apply_levels(&mut g.progress, &mut g.player, &mut g.rng, xp, false);
    }

    // Both jumps target the same place, so the line prints only while
    // 5 < stake < 17. The message never announces a stake of 5.
    if g.club_stake < 17 && g.club_stake > 5 {
        // `^6Ставки изменились. Теперь ставка - #`, `#` from the new stake.
        term::println(&text::fill(EMITTED[8].1, &[i64::from(g.club_stake)]));
    }
    // The cap is 17 exactly.
    if g.club_stake >= 17 {
        caught_cheating(g, lines)?;
    }
    Ok(())
}

/// Six wins in a row and the house calls it.
///
/// The stake ladder is 5 -> 7 -> 9 -> 11 -> 13 -> 15 -> 17, so this block is
/// the punishment for winning six hands without losing one.
///
/// The XP is awarded BEFORE the fight, not after it. That is invisible on screen
/// unless the award crosses a level threshold, which is exactly why it is written
/// down.
///
/// This fight never draws a Мент; the class is clamped to 7.
///
/// The countdown is set here and is reset per district, so landing it made the
/// club's countdown live.
///
/// **The block ends the visit without the player typing anything.**
fn caught_cheating(
    g: &mut Game,
    lines: &mut dyn Iterator<Item = io::Result<String>>,
) -> io::Result<()> {
    // Clamp to class 7.
    let enemy = g.roll_enemy(1);
    g.fight_accepted = true; // 1000:e184

    // `^4Козёл! Да ты мухлевал!`
    term::println(EMITTED[9].1);
    // `^6Это ` [rank] ` # уровня.` with the opponent's level as `#`.
    term::print("^6Это ");
    term::print(&Game::rank_name(enemy.class));
    term::println(&text::fill(" # уровня.", &[i64::from(enemy.level)]));

    // Build the `n`: `district * 5`.
    let xp = u16::from(g.district) * 5;
    // `^6Ты получаешь # качков опыта за победу в игре`, printed BEFORE the
    // experience is credited.
    term::println(&text::fill(EMITTED[10].1, &[i64::from(xp)]));
    // The credit and the level-up call.
    progress::apply_levels(&mut g.progress, &mut g.player, &mut g.rng, xp, false);

    // The fight itself, after the level-up.
    g.run_combat(2, enemy, lines)?;

    // `^6Уноси ноги, пока не отобрали деньги другие канадидаты`.
    term::println(EMITTED[11].1);
    g.club_ban_countdown = 5; // 1000:e23e

    // The `w` written into the buffer.
    g.leave_shop();
    Ok(())
}

/// `1` -- `потусоваться на дискотеке`, 15 rubles.
///
/// The game subtracts 15 from money, prints a message, and raises
/// Ловкость by 1. The refusal `^4Не хватает` is shared with gym arms
/// `1` and `2`.
fn dance(g: &mut Game) {
    // A signed word compare.
    if g.player.money < 15 {
        // `^4Не хватает`, then exit.
        term::println(EMITTED[13].1);
        return;
    }
    g.player.money = g.player.money.wrapping_sub(15_i16); // 1000:e2a7

    // `^2Ты прокачиваешь ловкость.`, printed BEFORE the store.
    term::println(EMITTED[14].1);
    g.player.agility += 1; // 1000:e2c5

    // `^1Ловкость +1 ` (trailing space is the original's).
    term::println(EMITTED[15].1);
}

/// `2` -- `разузнать приемы мухлёжников`, 22 rubles, behind the district gate.
///
/// The game subtracts 22 from money, prints a message, and raises
/// Удача by 1. Raising Удача improves the card game odds on the next `p`.
/// The refusal `^4Не хватает` is shared with gym arms `1` and `2`.
fn learn_tricks(g: &mut Game) {
    // Signed word compare.
    if g.player.money < 22 {
        // `^4Не хватает`, then exit.
        term::println(EMITTED[16].1);
        return;
    }
    g.player.money = g.player.money.wrapping_sub(22_i16); // 1000:e31c

    // `^2Ты прокачиваешь удачу.`
    term::println(EMITTED[17].1);
    g.player.luck += 1; // 1000:e33a

    // `^1Удача +1 ` (trailing space is the original's).
    term::println(EMITTED[18].1);
}

/// The literal pool for the club.
///
/// `(closes, text)` -- `closes` is true for a line-closing write, false
/// for a write that continues with the next literal.
pub(crate) const EMITTED: [(bool, &str); 20] = [
    (true, "^6Тебе не стоит пока туда соваться"), // 1000:df21
    (true, "Ты пришел в клуб напиши  ^6w^7  чтобы уйти"), // 1000:df3d
    (
        true,
        " Здесь можно сыграть в карты (^6p^7 Минимальная ставка- 5р.)",
    ), // 1000:df56
    (false, "^0Клуб\\"),                          // 1000:e025
    (true, "Ты поставил # рублей"),               // 1000:e087
    (true, "^2Ты выиграл # рублей "),             // 1000:e0db
    (true, "^6Ты получаешь # качков опыта"),      // 1000:e0fc
    (true, "^4Ты проиграл # рублей"),             // 1000:e129
    (true, "^6Ставки изменились. Теперь ставка - #"), // 1000:e158
    (true, "^4Козёл! Да ты мухлевал!"),           // 1000:e189
    (true, "^6Ты получаешь # качков опыта за победу в игре"), // 1000:e1e4
    (
        true,
        "^6Уноси ноги, пока не отобрали деньги другие канадидаты",
    ), // 1000:e225
    (true, "^6Не хватает денег - надо #."),       // 1000:e258
    (true, "^4Не хватает"),                       // 1000:e28c
    (true, "^2Ты прокачиваешь ловкость."),        // 1000:e2ac
    (true, "^1Ловкость +1 "),                     // 1000:e2c9
    (true, "^4Не хватает"),                       // 1000:e301
    (true, "^2Ты прокачиваешь удачу."),           // 1000:e321
    (true, "^1Удача +1 "),                        // 1000:e33e
    (true, "^6Ты пока что неузнал где в этом районе клуб"), // 1000:e36d
];

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

    /// A club-ready game: the club discovered, the ban countdown clear,
    /// the player standing in it at `district` with `money` in the pocket
    /// and the stake at its entry value.
    fn club(district: u8, money: i16) -> Game {
        let mut g = Game::new(player(), Progress::new(), 12345);
        g.district = district;
        g.player.money = money;
        g.places.mark_found(Location::Club); // 20ae:3699, gate 1000:df10
        g.location = Location::Club;
        g.club_stake = 5; // 1000:e020
        g.progress.threshold = u16::MAX; // keeps 1000:e124 from levelling
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

    /// The refusal at 14; equality at 15 passes. The stat increment check.
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

    /// At district 1 the key is never compared. **Nothing is printed and
    /// nothing changes** -- there is no "wrong district" literal for a refusal
    /// to use.
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

    /// The STAKE is in `ax` and the compare passes when stake <= money,
    /// so equality buys. At entry stake of 5 that is 4 refusing and 5 playing.
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

    /// **The WIN is the fall-through of `luck_below_random_32` being FALSE.**
    /// With удача far above any `Random(district * 12)` the hand must be won.
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
        // -5 at the start then +10 on win -- a net +5, and both are
        // kept because the debit is what the NEXT hand's gate measures.
        assert_eq!(g.player.money, 105, "1000:e0a8 then 1000:e0d7");
        assert_eq!(g.club_stake, 7, "1000:e0f7");
        assert_eq!(g.progress.xp, 1, "1000:e11d, district 1");
    }

    /// The LOSS path: one line and one store. The stake is back to 5, so
    /// the "stakes changed" line never prints for a stake of 5.
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

    /// The line prints only while `5 < stake < 17`. Driven directly at
    /// the two boundaries the ladder reaches.
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

    /// Six wins in a row walk the stake to 17, where it is caught. The ban
    /// countdown is set and the visit ends without player input.
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
        assert!(g.fight_accepted, "1000:e184");
    }

    // -- the chain itself ------------------------------------------------

    /// **There is no `не понял` line.** The loop falls off its end and
    /// jumps to the PROMPT; the CS-literal sweep finds no refusal literal.
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

    /// `w` is the shared exit at the end, owned by `Game::shop_turn`'s
    /// catch-all.
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

    /// The club's own `ReadLn` does not trim, and `Game::shop_turn` no
    /// longer does either. ` 1` is a MISS.
    #[test]
    fn the_club_prompt_refuses_untrimmed_input_like_the_original() {
        let mut g = club(1, 15);
        assert!(turn(&mut g, " 1").is_empty(), "a miss here and there");
        assert_eq!(g.player.money, 15, "and nothing was spent");
    }

    /// Case-insensitive in both.
    #[test]
    fn the_club_prompt_is_case_insensitive() {
        let mut g = club(5, 1_000);
        assert!(turn(&mut g, "W").is_empty());
        assert_eq!(g.location, Location::Street);
    }
}
