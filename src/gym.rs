//! The gym's key dispatch -- `trn`'s second block.
//!
//! `crate::game` keeps the verb (`Command::Gym` -> `Game::enter_shop`), the
//! intro line, the five [`crate::game::IMM_ROWS`] menu rows and the prompt;
//! this module is only what happens after that.
//!
//! ## The second block is the key dispatch, not a second menu
//!
//! The menu prints once, on entry; the chain of key compares is reached by
//! fall-through from the prompt and by nothing else. Most of the guard
//! conditions match between the menu and the dispatch chain, but two do
//! not, and both matter here:
//!
//! * the `5` arm's ceiling is `(district - 2) * 10` where the row's is
//!   `district * 2`, so the arm is reachable through a menu that no longer
//!   lists it;
//! * the `4` arm refuses when the tooth guard is already owned and the row
//!   has no such gate, so the row stays listed after the purchase.
//!
//! So the two halves share no code here either. `crate::game`'s
//! `Game::imm_row_visible` owns the menu predicates; nothing in this module
//! calls it.
//!
//! ## Three things this module deliberately does not do
//!
//! * **No refusal for a key the district hides.** At district 1 the `3`,
//!   `4` and `5` compares are jumped over entirely, so the key is never
//!   compared and nothing is printed. [`key_dispatches`] evaluates the
//!   district before the key for exactly that reason: testing the key
//!   first and the district second would print a refusal the original has
//!   no path to.
//! * **No message for an unrecognised key.** The chain falls off its end,
//!   which jumps to the PROMPT and not to the menu, and there is no
//!   `Непонятно` literal for it to print.
//! * **No re-print of the menu between turns.** Its target is the prompt.

use crate::game::Game;
use crate::progress;
use crate::term;
use crate::text;

/// Whether the gym's compare chain reaches a compare that `key` matches.
///
/// Six keys exist -- `1`, `2`, `3`, `4`, `5`, `w` -- and three of them sit
/// behind a district test that decides whether the compare happens at all:
///
/// | key | district |
/// |---|---|
/// | `3` | > 1 |
/// | `4` | > 1 |
/// | `5` | > 2 |
///
/// So at district 1 exactly three keys exist -- `1`, `2` and `w` -- and that
/// is a fact about the DISPATCHER, not about what the menu printed. `&&`
/// short-circuits left to right: gate, then compare.
///
/// `w` is not here. It is the shared exit every location's prompt has, and
/// `Game::shop_turn`'s catch-all already owns it; a `false` from this
/// function is the fall-through into it.
pub(crate) fn key_dispatches(g: &Game, key: &str) -> bool {
    match key {
        // No gate of its own.
        "1" => true,
        // No gate of its own.
        "2" => true,
        "3" => g.district > 1,
        "4" => g.district > 1,
        "5" => g.district > 2,
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

/// `1` -- `качаться гантелями и штангой`, 20 rubles.
///
/// **The damage split is easy to get backwards.** урон max rises on EVERY
/// purchase, while урон min rises only when the NEW strength -- the one
/// just incremented -- is even. Both-conditional and both-unconditional
/// readings are equally wrong and equally invisible without checking the
/// numbers.
///
/// `Fighter::strength` is a `u16`, so `is_multiple_of(2)` is a safe test for
/// evenness. Nothing one-shot is consumed, so the arm repeats.
fn train_strength(g: &mut Game) {
    // A signed word compare, and `Fighter::money` is an `i16`, so this is
    // the same compare on the same width.
    if g.player.money < 20 {
        // Prints `^4Не хватает` and leaves.
        term::println(EMITTED[2].1);
        return;
    }
    g.player.money = g.player.money.wrapping_sub(20_i16); // 1000:e657

    // Prints `^2Ты прокачиваешь силу.` BEFORE the six stat changes.
    term::println(EMITTED[3].1);
    g.player.strength += 1; // 1000:e675
    g.player.hpmax += 1; // 1000:e679
    g.player.hp += 1; // 1000:e67d

    // The remainder of the NEW strength div 2.
    if g.player.strength.is_multiple_of(2) {
        g.player.dmg_min += 1; // 1000:e68f
    }
    g.player.dmg_max += 1; // 1000:e693 -- outside the branch, every time.

    // Prints `^1Сила +1 ` (the trailing space is intentional).
    term::println(EMITTED[4].1);
}

/// `2` -- `качаться на тренажерах`, 20 rubles.
///
/// Same price and the same refusal literal as `1`, and that is where the
/// resemblance stops: this arm has four effects and **no conditional at
/// all**, both health words rise by a flat 5, and neither damage word is
/// touched. The two arms are not a template of each other.
fn train_stamina(g: &mut Game) {
    if g.player.money < 20 {
        // Prints `^4Не хватает` and leaves.
        term::println(EMITTED[5].1);
        return;
    }
    g.player.money = g.player.money.wrapping_sub(20_i16); // 1000:e6e3

    // Prints `^2Ты прокачиваешь выносливость.`
    term::println(EMITTED[6].1);
    g.player.vitality += 1; // 1000:e701 -- 20ae:38a2 is `+0x06`, живучесть
    g.player.hpmax += 5; // 1000:e705
    g.player.hp += 5; // 1000:e70a

    // Prints `^1Выносливость +1 ` (the trailing space is intentional).
    term::println(EMITTED[7].1);
}

/// `3` -- `прокачать 10 качков опыта`, 10 rubles.
///
/// **The gate order is load-bearing.** The level test comes first and the
/// money test second, so a player who is both too strong and too poor sees
/// `^6Ты слишком крутой...` and never the money refusal. The arm runs iff
/// `district * 10 - 3 > level` -- the same arithmetic as menu row 3's,
/// differing only in that the menu SKIPS the row where the arm PROCEEDS.
///
/// **The printed `#` is a separate immediate, not the xp total.** Menu row
/// 3's `#` has its own value. The two are equal and independent.
///
/// **This is why [`progress::apply_levels`] is called with `award = 0` in
/// its CAPPED form (`uncapped: false`).** The xp is credited and printed
/// before the level-up runs, and `apply_levels` adds its own `award`
/// *before* the threshold test, so passing `award = 10` after the manual
/// `xp += 10` would grant twenty.
///
/// **The outer threshold guard duplicates the callee's own entry check --
/// kept intentionally.**
///
/// This is the only arm in the whole range that can move the RNG stream,
/// and it moves it indirectly: the gym contains no `Random` call site of
/// its own, while levelling up spends two draws per level gained.
fn train_xp(g: &mut Game) {
    // The multiply is unsigned and the compare is signed; district is 1..5,
    // so neither can wrap.
    if i32::from(g.district) * 10 - 3 <= i32::from(g.player.level) {
        // Prints `^6Ты слишком крутой чтобы тренироваться здесь.` and leaves.
        term::println(EMITTED[8].1);
        return;
    }
    // Checked second, so the line above wins when both fail.
    if g.player.money < 10 {
        // Prints `^4Не хватает деньжат` and leaves.
        term::println(EMITTED[9].1);
        return;
    }
    g.player.money = g.player.money.wrapping_sub(10_i16); // 1000:e796

    // Prints `^2Ты тренируешься.`
    term::println(EMITTED[10].1);
    g.progress.xp += 10; // 1000:e7b4

    // The `#` is a fixed immediate, not the xp total. Prints
    // `^1 +# качков опыта `.
    term::println(&text::fill(EMITTED[11].1, &[10]));
    if g.progress.xp < g.progress.threshold {
        return;
    }
    // `award` is 0 because the grant already happened above.
    progress::apply_levels(&mut g.progress, &mut g.player, &mut g.rng, 0, false);
}

/// `4` -- `купить зубную защиту боксёров` (зубная защита), 30 rubles.
///
/// **The ownership gate is the arm's and the menu row has no counterpart.**
/// Row 4's only gate is `district > 1`, so after the purchase
/// the row is still listed and the key still prints
/// `^6У тебя есть эта штучка.` That is intentional, not a bug
/// to fix in the menu.
///
/// The gate order is the ownership test first and the money test second, so
/// an owner with no money still sees the already-owned line.
///
/// Nothing clears the purchase once made -- not the district reset, which
/// clears the discovery flags and leaves this one alone -- so the purchase
/// is permanent for the character. It splits a jaw break into the plain arm
/// and an extra roll: a draw-count difference, not flavour.
fn buy_tooth_guard(g: &mut Game) {
    // Checked first, so it wins over the money test below.
    if g.tooth_guard {
        // Prints `^6У тебя есть эта штучка.`
        term::println(EMITTED[14].1);
        return;
    }
    if g.player.money < 30 {
        // Prints `^4А не хватает рубликов` and leaves.
        term::println(EMITTED[12].1);
        return;
    }
    g.player.money = g.player.money.wrapping_sub(30_i16); // 1000:e823
    g.tooth_guard = true; // 1000:e828

    // Prints `^2Ты купил защиту.`
    term::println(EMITTED[13].1);
}

/// `5` -- `прокачать пресс`, 20 rubles.
///
/// **This arm's ceiling is NOT menu row 5's.** The row's is `district * 2`;
/// this is `(district - 2) * 10` -- a different NUMBER, not a different
/// spelling of the same one. At district 3 the row disappears at trained
/// armour 6 while the arm keeps working to 10, so the arm is reachable
/// through a menu that no longer lists it, which is exactly why the loop
/// reprints only the prompt. Sharing one predicate between the row and the
/// arm is wrong in both directions. The `district - 2` subtraction cannot
/// underflow on the reachable path: the gate already required district > 2.
///
/// **The hint is suppressed from district 4 up.** `^6Качай дальше в
/// следующем районе` follows the ceiling line only while `district < 4`.
///
/// **`Game::trained_armour` is derived, not the raw armour.** It is
/// recomputed on every entry to the gym as the armour stat minus the
/// armour that came from equipment, so it is the armour the player
/// TRAINED. Both readers -- this arm's ceiling and
/// `Game::imm_row_visible`'s `("trn","5")` row -- call it instead of
/// substituting `armor`.
///
/// Armour and the trained-armour scratch move together in one statement
/// here: since the scratch is derived, incrementing the armour it is
/// derived from moves both. The arm still terminates.
fn train_abs(g: &mut Game) {
    let ceiling = (i32::from(g.district) - 2) * 10;
    // The trained-armour scratch, zero-extended. `Game::trained_armour` is
    // its port.
    if i32::from(g.trained_armour()) >= ceiling {
        // Prints `^6Ты максимально прокачал пресс для своего уровня`.
        term::println(EMITTED[18].1);
        if g.district < 4 {
            // Prints `^6Качай дальше в следующем районе`.
            term::println(EMITTED[19].1);
        }
        return;
    }
    // The money check comes second, so the ceiling line wins over it.
    if g.player.money < 20 {
        // Prints `^4Не хватает рубликов` and leaves.
        term::println(EMITTED[15].1);
        return;
    }
    g.player.money = g.player.money.wrapping_sub(20_i16); // 1000:e8b8

    // Prints `^2Ты прокачиваешь пресс.`
    term::println(EMITTED[16].1);
    // Raises the visible Броня and the scratch this arm's own ceiling is
    // tested against together, as one statement, because the port keeps
    // one value for both.
    g.player.armor = g.player.armor.wrapping_add(1);
    // Prints `^1Броня +1`.
    term::println(EMITTED[17].1);
}

/// The gym's list of printed lines.
///
/// `(closes, text)` -- `closes` is true for a `WriteLn`, false for a
/// `Write` the next literal continues.
pub(crate) const EMITTED: [(bool, &str); 28] = [
    (true, "Ты пришел в качалку напиши  ^6w^7  чтобы уйти"), // 1000:e3e7
    (false, "^0Качалка\\"),                                  // 1000:e5e4
    (true, "^4Не хватает"),                                  // 1000:e63c
    (true, "^2Ты прокачиваешь силу."),                       // 1000:e65c
    (true, "^1Сила +1 "),                                    // 1000:e697
    (true, "^4Не хватает"),                                  // 1000:e6c8
    (true, "^2Ты прокачиваешь выносливость."),               // 1000:e6e8
    (true, "^1Выносливость +1 "),                            // 1000:e70f
    (true, "^6Ты слишком крутой чтобы тренироваться здесь."), // 1000:e759
    (true, "^4Не хватает деньжат"),                          // 1000:e77b
    (true, "^2Ты тренируешься."),                            // 1000:e79b
    (true, "^1 +# качков опыта "),                           // 1000:e7b9
    (true, "^4А не хватает рубликов"),                       // 1000:e808
    (true, "^2Ты купил защиту."),                            // 1000:e82d
    (true, "^6У тебя есть эта штучка."),                     // 1000:e848
    (true, "^4Не хватает рубликов"),                         // 1000:e89d
    (true, "^2Ты прокачиваешь пресс."),                      // 1000:e8bd
    (true, "^1Броня +1"),                                    // 1000:e8de
    (true, "^6Ты максимально прокачал пресс для своего уровня"), // 1000:e8f9
    (true, "^6Качай дальше в следующем районе"),             // 1000:e919
    (true, "^6Ты пока незнаешь где в этом районе качалка"),  // 1000:e948
    (true, "^4Ты не схавать колёса из-за сломаной челюсти."), // 1000:e984
    (false, "^2Колёса прибавляют #з. "),                     // 1000:e9d7
    (true, "^2Здоровья:#/#. Осталось # косяков"),            // 1000:e9fb
    (
        true,
        "^2Колёса прибавляют #з. Здоровья:#/#. Осталось # косякова",
    ), // 1000:ea1e
    (true, "^2Сила +2."),                                    // 1000:ea3b
    (true, "^4У тебя нет косяков"),                          // 1000:ea56
    (true, "^6Ты неможешь схавать ещё один косяк."),         // 1000:ea71
];

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

    /// A gym-ready game: the gym discovered, the player
    /// standing in it, at `district`, with `money` in the pocket. `Game`'s
    /// `mode` is private to `crate::game`, so [`turn`] names the location
    /// explicitly the way `Game::run` does from `Mode::Shop(loc)`.
    fn gym(district: u8, money: i16) -> Game {
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

    /// Buying twice from an ODD strength makes the new strength even once
    /// and odd once, so урон min must rise by 1 and урон max by 2.
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

    /// A strength of 19 refuses and 20 buys; the ceiling is 20.
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

    /// Same price as `1`.
    #[test]
    fn arm_2_refuses_at_nineteen() {
        let mut g = gym(1, 19);
        let v0 = g.player.vitality;
        assert_eq!(turn(&mut g, "2"), vec!["^4Не хватает"], "1000:e6dc");
        assert_eq!(g.player.money, 19);
        assert_eq!(g.player.vitality, v0);
    }

    // -- arm `3` ---------------------------------------------------------

    /// The printed `#` and the xp credited are both 10, independently --
    /// a port that printed the xp total, or credited the printed number
    /// twice, would be wrong.
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

    /// `apply_levels` gets `award = 0` because the xp grant already
    /// happened: the xp left after the level-up must be `(xp + 10) -
    /// threshold`. Passing `award = 10` would leave ten more than that.
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

    /// Below the threshold nothing is called and the level stands.
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

    /// The arm runs iff `district * 10 - 3 > level`. At district 2 that is
    /// 17, so level 16 trains and level 17 does not.
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

    /// The level test comes before the money test. Too strong AND too
    /// poor must print the level refusal; testing money first would print
    /// the other line.
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

    /// 9 refuses and 10 buys.
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

    /// Costs 30 rubles and sets the зубная защита flag.
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

    /// **Do not "fix" the menu.** Row 4's only gate is `district > 1`;
    /// the ownership test is the arm's alone, so after the purchase the
    /// row is still listed and the key still prints the already-owned
    /// line. The ownership test also comes before the money test, so an
    /// owner with nothing in the pocket still sees the owned line and not
    /// the money one.
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

    /// Costs 20 rubles and raises the armour.
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

    /// The arm's ceiling is `(district - 2) * 10` and the menu row's is
    /// `district * 2`. At district 3 that is 10 against 6, so at armour 6
    /// the row is gone from the menu even though the arm still works.
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

    /// The value the ceiling is tested against stays in step with the
    /// purchase, so the arm terminates. At district 3 the ceiling is 10
    /// and exactly ten purchases fit.
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

    /// The training ceiling tracks a scratch value, not the armour byte,
    /// so equipment does NOT eat into the training budget: a district-3
    /// player buys the same 10 whether or not he owns the Крутая кожанка,
    /// ending on armour 14 instead of 10 because the jacket's 4 sits on
    /// top of the trained 10. Intentional -- counting the jacket against
    /// the ceiling would stop him at 6.
    #[test]
    fn arm_5_counts_trained_armour_not_worn_armour() {
        let mut g = gym(3, 10_000);
        g.player.armor = 4; // the jacket's own +4, already granted
        g.wear_jacket_krutaya = true;
        let mut bought = 0;
        for _ in 0..40 {
            let out = turn(&mut g, "5");
            if out[0] == "^6Ты максимально прокачал пресс для своего уровня"
            {
                break;
            }
            bought += 1;
        }
        assert_eq!(bought, 10, "the ceiling is (3 - 2) * 10 of TRAINED armour");
        assert_eq!(g.player.armor, 14);
        assert_eq!(g.trained_armour(), 10);
    }

    /// The hint follows the ceiling line while district < 4, and is
    /// suppressed from district 4 on.
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

    /// The ceiling test comes before the money test.
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

    /// At district 1, keys `3`, `4` and `5` are not recognised at all: no
    /// refusal is printed and nothing changes. This is intentional -- the
    /// district 1 gym has no path to a refusal for these keys.
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

    /// District 2 opens `3` and `4`, and still hides `5`.
    #[test]
    fn district_two_opens_three_and_four_but_not_five() {
        let g = gym(2, 1_000);
        assert!(key_dispatches(&g, "3"), "1000:e72d");
        assert!(key_dispatches(&g, "4"), "1000:e7e7");
        assert!(!key_dispatches(&g, "5"), "1000:e866 needs district > 2");
        let g = gym(3, 1_000);
        assert!(key_dispatches(&g, "5"));
    }

    /// **There is no `Непонятно` line.** An unrecognised key prints
    /// nothing; the turn re-prompts, and the player stays in the gym.
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

    /// `w` is the shared exit, owned by `Game::shop_turn`'s catch-all
    /// rather than by this module, and `key_dispatches` must let it
    /// through to there.
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

    /// The gym's own input reading does not trim, only lowercases, and
    /// `Game::shop_turn` no longer trims either. ` 1` is a MISS, here as
    /// there.
    #[test]
    fn the_gym_prompt_refuses_untrimmed_input_like_the_original() {
        let mut g = gym(1, 20);
        assert!(turn(&mut g, " 1").is_empty(), "a miss here and there");
        assert_eq!(g.player.money, 20, "and nothing was spent");
    }

    /// And it is case-insensitive in both: input is lowercased before
    /// matching.
    #[test]
    fn the_gym_prompt_is_case_insensitive() {
        let mut g = gym(5, 1_000);
        assert!(turn(&mut g, "W").is_empty());
        assert_eq!(g.location, Location::Street);
    }
}
