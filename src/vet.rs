//! The vet (`rep`), `1000:d3a6`..`1000:d6ed` -- mapped and ported in one
//! task.
//!
//! `crate::game` keeps the verb (`Command::Vet` -> `Game::enter_shop`), the
//! discovery gate, the intro line, the healthy-skip and the two
//! [`crate::game::IMM_ROWS`] menu rows; this module is the loop top at
//! `1000:d4ba`, the two key arms and the two exits. The map is
//! `docs/re/vet.md` and `data/vet_arms.json`, which
//! `python3 tools/test_arms_artifacts.py` re-derives from `orig/g.exe`.
//!
//! ## What the two keys actually do -- and what the port used to think
//!
//! **Established from flow**, decoded for this task with
//! `python3 tools/re_query.py resolve 1000:d3a6 -n 840 -i 460`. Before this
//! task `Game::heal_jaw` charged 3 rubles to clear `20ae:38b0` and
//! `Game::heal_leg` charged 7 to clear `20ae:38b1`, one break each, with a
//! shared `pay_and_heal`. Neither key does that:
//!
//! | key | gate | price | effect |
//! |---|---|---|---|
//! | `r` `1000:d537` | `1000:d53e`/`1000:d545` -- **either** break set | 7 (`1000:d54c`) | clears **both**, `1000:d558` AND `1000:d55d` |
//! | `h` `1000:d5b9` | `1000:d5c3` -- hp < hpmax | 3 (`1000:d5cf`) | `hp += 5` (`1000:d5de`), clamped to hpmax (`1000:d5ea`..`1000:d5ef`); touches neither break |
//!
//! So `h` is not a jaw at all -- it is the vet's five-point heal, and its
//! menu row says so: file `0xB2B2` is `3^7 рубля тебя залатают` ("they patch
//! you up"), against `r`'s file `0xB2D9` `7^7 рублей починят переломы`
//! ("they fix your fractures"). The old reading also gave `pay_and_heal` a
//! "you are healthy" refusal on the key, which no arm has: both arms fail
//! **silently** when their own precondition is unmet (`1000:d54a` and
//! `1000:d5cc` both jump straight to the `w` compare) and the
//! `^0Док: вали отсюда ты здоров.` line belongs to the LOOP TOP instead.
//!
//! ## The loop top is a health test, and it ejects
//!
//! `1000:d4ba` is the join of the healthy-skip at `1000:d3f4` and the back
//! edge at `1000:d6c5`, so it runs on entry AND before every prompt:
//!
//! ```text
//! d4ba  mov ax,[0x38ac] / d4bd cmp ax,[0x38ae] / d4c1 jl 0xd4ed
//! d4c3  cmp byte [0x38b0],0x0 / d4c8 jnz 0xd4ed
//! d4ca  cmp byte [0x38b1],0x0 / d4cf jnz 0xd4ed
//! d4d1  mov di,0x9a25 .. d4e5 WriteLn        ; `^0Док: вали отсюда ты здоров.`
//! d4ea  jmp 0xd6c8                            ; and out
//! ```
//!
//! Every one of the three jumps goes to the PROMPT and the fall-through is
//! the eject, so the player is thrown out the moment they are whole -- which
//! is also how a successful `h` or `r` ends the visit. [`loop_top`] is that
//! block.
//!
//! **The same predicate is spelled twice, oppositely.** `1000:d3d3`..
//! `1000:d3f2` computes it into `al` (`d3da jl`, `d3e1 jnz`, `d3e8 jz`, then
//! `or al,al` / `d3f2 jz`) to decide whether the MENU prints; `1000:d4ba`
//! branches on it directly. `Game::print_shop_intro`'s early return is the
//! first, [`loop_top`] the second. They are not the same instruction and
//! neither is derived from the other here.
//!
//! ## Two exit keys, not one
//!
//! `1000:d6a8` compares the shared `w` (CS `0x848e`) and `1000:d6b9`
//! compares `e` (CS `0x9b6e`, file `0xB43E` -- the same literal the street's
//! quit verb uses at `1000:edfa`). Both `jz` to `1000:d6c8`, which
//! `jmp short`s to `1000:d6e3`, the `girl` verb's setup on the STREET buffer
//! `20ae:3972` -- i.e. the street dispatch chain resumes. So `e` at the vet
//! prompt LEAVES THE VET; it does not quit the game, because the vet's
//! `ReadLn` never reaches `entry`'s own compare chain. [`exits`] is that
//! pair. The vet is the only location in this port with a second exit key.
//!
//! ## The `h` arm's flavour draw
//!
//! `1000:d5f6` is the only `Random` call site in the range and its `n` is
//! the immediate 3 pushed at `1000:d5f2`/`1000:d5f5` -- not a district
//! product, unlike every other draw in a location handler. `ax` at the two
//! dispatch compares `1000:d5fb` and `1000:d61b` is that draw's return
//! value and nothing else: no instruction between `1000:d5f6` and
//! `1000:d61b` writes `ax` (the run is `cmp ax,0x1` / `jnz` / the push
//! block, which only touches `ax` via `xor ax,ax`/`push ax` INSIDE the
//! `1000:d600` arm that `1000:d5fe` skips on the way to `1000:d61b`).
//! It picks one of three flavour blocks and changes nothing else; the
//! `^2Здоровья #/#` line after it prints on all three paths.
//!
//! Address convention: `docs/re/METHODOLOGY.md`, "Address convention, and
//! its range of validity". Every string literal below is quoted from
//! `data/strings.json` at the file offset its `mov di,<n>` push resolves to,
//! markup included, and each inline citation is written as
//! ``file `0xNNNN` `^Nthe string``` on one line directly above the
//! `term::println` that prints it -- the shape
//! `tools/test_string_citations.py` can resolve.

use crate::game::Game;
use crate::term;
use crate::text;

/// Whether the vet's compare chain reaches a compare that `key` matches.
///
/// **Established from flow.** The chain is `r` (`1000:d537`), `h`
/// (`1000:d5b9`), `w` (`1000:d6ad`) and `e` (`1000:d6be`), in that order.
/// Neither key arm sits behind a gate that skips its own compare -- both
/// gates (`1000:d53e`/`1000:d545` and `1000:d5c3`) stand AFTER the compare
/// and inside the arm, which is why they live in [`run_key`] and not here.
/// The two exits are [`exits`]'.
pub(crate) fn key_dispatches(key: &str) -> bool {
    matches!(key, "r" | "h")
}

/// Whether `key` is one of the vet's two exit tokens.
///
/// `w` at `1000:d6ad` is the shared exit nine push sites use image-wide;
/// `e` at `1000:d6be` is the vet's own second one and has no counterpart at
/// the den, the club or the gym. Both hits reach `1000:d6c8`.
pub(crate) fn exits(key: &str) -> bool {
    key == "w" || key == "e"
}

/// Run the arm `key` selected.
pub(crate) fn run_key(g: &mut Game, key: &str) {
    match key {
        "r" => fix_fractures(g),
        "h" => patch_up(g),
        _ => {}
    }
}

/// `1000:d4ba`..`1000:d4ed` -- the loop top. Returns `true` when the visit
/// is over.
///
/// Reached on entry (from `1000:d3f4`, the healthy-skip past the menu) and
/// from the back edge `1000:d6c5`, so it runs before every prompt. When
/// hp is at maximum and neither break is set it prints and jumps out at
/// `1000:d4ea`; otherwise all three of `1000:d4c1`, `1000:d4c8` and
/// `1000:d4cf` reach the prompt at `1000:d4ed`.
///
/// **The eject is the fall-through, not a branch.** Writing the condition
/// the other way round -- ejecting when any of the three tests jumps --
/// would throw the player out exactly when they need the vet.
pub(crate) fn loop_top(g: &mut Game) -> bool {
    // 1000:d4ba / 1000:d4bd / 1000:d4c1, then 1000:d4c3 / 1000:d4c8, then
    // 1000:d4ca / 1000:d4cf.
    if g.player.hp < g.player.hpmax || g.player.broken_jaw || g.player.broken_leg {
        return false;
    }
    // 1000:d4d1 pushes file `0xB2F5` `^0Док: вали отсюда ты здоров.`,
    // printed by 1000:d4e5.
    term::println("^0Док: вали отсюда ты здоров.");
    // 1000:d4ea -> 1000:d6c8 -> 1000:d6e3, the street chain.
    g.leave_shop();
    true
}

/// `r` -- `1000:d532`..`1000:d5af`, `починят переломы`, 7 rubles.
///
/// **Established from flow:**
///
/// ```text
/// d53e  cmp byte [0x38b0],0x1 / d543 jz 0xd54c      ; jaw set -> proceed
/// d545  cmp byte [0x38b1],0x1 / d54a jnz 0xd5af     ; else leg set, or SILENT
/// d54c  cmp word [0x38c7],0x7 / d551 jl 0xd596      ; can pay 7, SIGNED
/// d553  sub word [0x38c7],0x7
/// d558  mov byte [0x38b0],0x0                        ; BOTH breaks
/// d55d  mov byte [0x38b1],0x0
/// d562  mov di,0x9a52 (file 0xB322) .. d576 WriteLn
/// d57b  mov di,0x9a80 (file 0xB350) .. d58f WriteLn / d594 jmp short 0xd5af
/// d596  mov di,0x9a9a (file 0xB36A) .. d5aa WriteLn
/// ```
///
/// **One price clears both breaks**, and the gate is an OR: `1000:d543`
/// takes the jaw case straight to the money test and only a clear jaw
/// reaches the leg test. So a player with two breaks pays 7 once, and a
/// player with none is answered with **silence** -- `1000:d54a` jumps to
/// `1000:d5af`, the `h` compare, printing nothing. There is no
/// "you are healthy" literal on this path; that line is [`loop_top`]'s.
///
/// **Two lines print on success, not one.** `^0Ого! да тебя не иначе как
/// грузовик откатал!` precedes `^2Твои переломы залечены.`
fn fix_fractures(g: &mut Game) {
    // 1000:d53e / 1000:d543 then 1000:d545 / 1000:d54a -- an OR, and silent
    // when neither holds.
    if !(g.player.broken_jaw || g.player.broken_leg) {
        return;
    }
    // 1000:d54c / 1000:d551 -- `jl`, so 6 refuses and 7 buys.
    if g.player.money < 7 {
        // 1000:d596 pushes file `0xB36A`
        // `^4Блин халявщик, медицина не бесплатная`, printed by 1000:d5aa.
        term::println("^4Блин халявщик, медицина не бесплатная");
        return;
    }
    g.player.money -= 7; // 1000:d553
    g.player.broken_jaw = false; // 1000:d558
    g.player.broken_leg = false; // 1000:d55d

    // 1000:d562 pushes file `0xB322`
    // `^0Ого! да тебя не иначе как грузовик откатал!`, printed by 1000:d576.
    term::println("^0Ого! да тебя не иначе как грузовик откатал!");
    // 1000:d57b pushes file `0xB350` `^2Твои переломы залечены.`, printed by
    // 1000:d58f.
    term::println("^2Твои переломы залечены.");
}

/// `h` -- `1000:d5af`..`1000:d6a3`, `тебя залатают`, 3 rubles for +5 hp.
///
/// **Established from flow:**
///
/// ```text
/// d5c3  mov ax,[0x38ac] / d5c6 cmp ax,[0x38ae] / d5ca jl 0xd5cf
/// d5cc  jmp 0xd6a3                                   ; hp >= hpmax: SILENT
/// d5cf  cmp word [0x38c7],0x3 / d5d4 jnl 0xd5d9 / d5d6 jmp 0xd68a
/// d5d9  sub word [0x38c7],0x3
/// d5de  add word [0x38ac],0x5                        ; hp += 5
/// d5e3  mov ax,[0x38ac] / d5e6 cmp ax,[0x38ae] / d5ea jle 0xd5f2
/// d5ec  mov ax,[0x38ae] / d5ef mov [0x38ac],ax       ; clamp to hpmax
/// d5f2  mov ax,0x3 / d5f5 push ax / d5f6 call 0f78:114b   ; Random(3)
/// d5fb  cmp ax,0x1 / d5fe jnz 0xd61b
/// d600  mov di,0x9ac4 (file 0xB394) .. d614 WriteLn / d619 jmp short 0xd66d
/// d61b  cmp ax,0x2 / d61e jnz 0xd63b
/// d620  mov di,0x9aed (file 0xB3BD) .. d634 WriteLn / d639 jmp short 0xd66d
/// d63b  mov di,0x9b27 (file 0xB3F7) .. d64f WriteLn
/// d654  mov di,0x9b48 (file 0xB418) .. d668 WriteLn
/// d66d  mov di,0x9b5f (file 0xB42F) / d672 push [0x38ac] / d676 push [0x38ae]
/// d683  WriteLn / d688 jmp short 0xd6a3
/// d68a  mov di,0x9a9a (file 0xB36A) .. d69e WriteLn
/// ```
///
/// **Neither break byte appears anywhere in this arm.** `20ae:38b0` and
/// `20ae:38b1` have exactly four write sites between them image-wide and the
/// vet's are `1000:d558` and `1000:d55d`, both in the `r` arm
/// (`python3 tools/re_query.py xrefs-to 20ae:38b0` and `20ae:38b1`).
///
/// **The `Random(3)` is flavour only**: all three arms converge on
/// `1000:d66d` and none of them touches a global. The `0` arm is the only
/// one with two lines. The draw still moves the RNG stream, which is why it
/// goes through [`crate::rng::Rng::below_at`] rather than being elided.
///
/// **The clamp is `jle`, not `jl`.** At exactly hpmax the store at
/// `1000:d5ef` is skipped, which is the same value either way -- but the
/// branch is a branch of the original and `1000:d5ea` is where the port
/// records it.
fn patch_up(g: &mut Game) {
    // 1000:d5c3 / 1000:d5ca / 1000:d5cc -- silent, not refused.
    if g.player.hp >= g.player.hpmax {
        return;
    }
    // 1000:d5cf / 1000:d5d4 -- `jnl`, so 2 refuses and 3 buys.
    if g.player.money < 3 {
        // 1000:d68a pushes file `0xB36A`
        // `^4Блин халявщик, медицина не бесплатная` -- the SAME literal the
        // `r` arm refuses with, at its own push site; printed by 1000:d69e.
        term::println("^4Блин халявщик, медицина не бесплатная");
        return;
    }
    g.player.money -= 3; // 1000:d5d9
    g.player.hp += 5; // 1000:d5de

    // 1000:d5e3..1000:d5ea then 1000:d5ec/1000:d5ef.
    if g.player.hp > g.player.hpmax {
        g.player.hp = g.player.hpmax;
    }

    // 1000:d5f2/1000:d5f5 push the literal 3; 1000:d5f6 is the draw.
    match g.rng.below_at("1000:d5f6", 3) {
        // 1000:d5fb / 1000:d5fe.
        1 => {
            // 1000:d600 pushes file `0xB394`
            // `^0Щас гайки подтянем и будешь как новый!`, printed by
            // 1000:d614.
            term::println("^0Щас гайки подтянем и будешь как новый!");
        }
        // 1000:d61b / 1000:d61e.
        2 => {
            // 1000:d620 pushes file `0xB3BD`
            // `^0Так чё тут у нас? Ага, пара швов и всё будет в порядке.`,
            // printed by 1000:d634.
            term::println("^0Так чё тут у нас? Ага, пара швов и всё будет в порядке.");
        }
        // 1000:d63b -- the fall-through of both compares, i.e. a draw of 0,
        // and the only arm with two lines.
        _ => {
            // 1000:d63b pushes file `0xB3F7`
            // `^6Эй, Док а зачем тебе паяльник?`, printed by 1000:d64f.
            term::println("^6Эй, Док а зачем тебе паяльник?");
            // 1000:d654 pushes file `0xB418` `^0Док: Молчи животное!`,
            // printed by 1000:d668.
            term::println("^0Док: Молчи животное!");
        }
    }
    // 1000:d66d pushes file `0xB42F` `^2Здоровья #/#`, whose two `#` are
    // 1000:d672's hp and 1000:d676's hpmax, in that order; printed by
    // 1000:d683.
    term::println(&text::fill(
        "^2Здоровья #/#",
        &[i64::from(g.player.hp), i64::from(g.player.hpmax)],
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locations::Location;

    /// The location this module owns, named once for the tests below.
    const LOCATION: Location = Location::Vet;
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

    /// A vet-ready game. `Game::new` already marks the vet found
    /// (`1000:6dc3`), so only the standing-in-it half is set here.
    fn vet(money: i32) -> Game {
        let mut g = Game::new(player(), Progress::new(), 12345);
        g.player.money = money;
        assert!(g.places.is_found(LOCATION), "1000:6dc3");
        g.location = LOCATION;
        g
    }

    fn no_input() -> std::iter::Empty<io::Result<String>> {
        std::iter::empty()
    }

    /// One turn at the vet prompt, through the real dispatch in
    /// `Game::shop_turn`, so the wiring -- including the loop top that runs
    /// after the arm -- is under test too.
    fn turn(g: &mut Game, key: &str) -> Vec<String> {
        capture::lines(|| {
            g.shop_turn(LOCATION, key, &mut no_input()).unwrap();
        })
    }

    // -- arm `r` ---------------------------------------------------------

    /// `1000:d558` AND `1000:d55d` both store 0, behind ONE price test at
    /// `1000:d54c`. A port that healed one break per 7 rubles leaves the
    /// other set here.
    #[test]
    fn arm_r_clears_both_breaks_for_one_price() {
        let mut g = vet(7);
        g.player.broken_jaw = true;
        g.player.broken_leg = true;
        g.player.hp = 1; // keeps the loop top from ejecting
        assert_eq!(
            turn(&mut g, "r"),
            vec![
                "^0Ого! да тебя не иначе как грузовик откатал!",
                "^2Твои переломы залечены.",
            ],
            "1000:d576 then 1000:d58f"
        );
        assert!(!g.player.broken_jaw, "1000:d558");
        assert!(!g.player.broken_leg, "1000:d55d");
        assert_eq!(g.player.money, 0, "1000:d553 sub 0x7, once");
    }

    /// `1000:d543` takes the JAW case straight to the money test, so a leg
    /// alone must reach it through `1000:d545` and heal just the same.
    #[test]
    fn arm_r_runs_from_either_break_alone() {
        for (jaw, leg) in [(true, false), (false, true)] {
            let mut g = vet(7);
            g.player.broken_jaw = jaw;
            g.player.broken_leg = leg;
            g.player.hp = 1;
            assert_eq!(turn(&mut g, "r").len(), 2, "jaw {jaw} leg {leg}");
            assert_eq!(g.player.money, 0);
        }
    }

    /// `1000:d54a jnz 0xd5af` jumps to the `h` compare with nothing printed.
    /// **No break, no line** -- the `^0Док: вали отсюда ты здоров.` an
    /// earlier port printed here is the LOOP TOP's, and the loop top does
    /// not fire while hp is short either.
    #[test]
    fn arm_r_is_silent_with_no_break_at_all() {
        let mut g = vet(100);
        g.player.hp = 1;
        assert!(turn(&mut g, "r").is_empty(), "1000:d54a");
        assert_eq!(g.player.money, 100, "and nothing is charged");
    }

    /// `1000:d54c` is `cmp ...,0x7` and `1000:d551` is `jl`, so 6 refuses
    /// and 7 buys.
    #[test]
    fn arm_r_refuses_at_six_and_buys_at_seven() {
        let mut g = vet(6);
        g.player.broken_leg = true;
        g.player.hp = 1;
        assert_eq!(
            turn(&mut g, "r"),
            vec!["^4Блин халявщик, медицина не бесплатная"],
            "1000:d5aa"
        );
        assert!(g.player.broken_leg, "no heal on the refusal");
        assert_eq!(g.player.money, 6);
    }

    // -- arm `h` ---------------------------------------------------------

    /// **`h` is not a jaw.** `1000:d5de` is `add word [0x38ac],0x5` and no
    /// instruction in `1000:d5af`..`1000:d6a3` touches `20ae:38b0` or
    /// `20ae:38b1`. A port that cleared a break here fails on the break.
    #[test]
    fn arm_h_adds_five_health_and_touches_neither_break() {
        let mut g = vet(3);
        g.player.hp = 4;
        g.player.broken_jaw = true;
        let out = turn(&mut g, "h");
        assert_eq!(g.player.hp, 9, "1000:d5de add 0x5");
        assert!(g.player.broken_jaw, "1000:d558 is the `r` arm's, not this");
        assert_eq!(g.player.money, 0, "1000:d5d9 sub 0x3");
        assert_eq!(
            out.last().unwrap(),
            "^2Здоровья 9/20",
            "1000:d683, the # pair is 1000:d672 then 1000:d676"
        );
    }

    /// `1000:d5e3`..`1000:d5ef` clamps to hpmax, so the last +5 cannot
    /// overshoot and the reported pair is `hpmax/hpmax`.
    #[test]
    fn arm_h_clamps_the_overshoot_to_hpmax() {
        let mut g = vet(3);
        g.player.hp = 18; // 18 + 5 = 23 > 20
        let out = turn(&mut g, "h");
        assert_eq!(g.player.hp, 20, "1000:d5ef");
        assert!(
            out.iter().any(|l| l == "^2Здоровья 20/20"),
            "1000:d683 after the clamp"
        );
    }

    /// `1000:d5c3`/`1000:d5cc` -- at full health the arm jumps straight to
    /// the `w` compare. Nothing is printed by the ARM; the eject line that
    /// follows is the loop top's, and it is asserted separately below.
    #[test]
    fn arm_h_is_silent_at_full_health_with_a_break_standing() {
        let mut g = vet(100);
        g.player.broken_leg = true; // keeps the loop top from ejecting
        assert!(turn(&mut g, "h").is_empty(), "1000:d5cc");
        assert_eq!(g.player.money, 100);
    }

    /// `1000:d5cf` is `cmp ...,0x3` and `1000:d5d4` is `jnl`, so 2 refuses
    /// and 3 buys -- with the same literal the `r` arm uses, pushed at its
    /// own site `1000:d68a`.
    #[test]
    fn arm_h_refuses_at_two_and_buys_at_three() {
        let mut g = vet(2);
        g.player.hp = 4;
        assert_eq!(
            turn(&mut g, "h"),
            vec!["^4Блин халявщик, медицина не бесплатная"],
            "1000:d69e"
        );
        assert_eq!(g.player.hp, 4, "no heal on the refusal");
    }

    /// `1000:d5fb`/`1000:d61e` split `Random(3)` three ways and all three
    /// converge on `1000:d66d`. The `0` arm is the only one with two lines,
    /// and every arm ends with `^2Здоровья #/#`. Driven by seeding the RNG
    /// until each value has been seen.
    #[test]
    fn arm_h_has_three_flavour_arms_and_always_reports_the_health() {
        let one = "^0Щас гайки подтянем и будешь как новый!";
        let two = "^0Так чё тут у нас? Ага, пара швов и всё будет в порядке.";
        let zero = "^6Эй, Док а зачем тебе паяльник?";
        let mut seen: Vec<&str> = Vec::new();
        for seed in 1u32..400 {
            let mut g = Game::new(player(), Progress::new(), seed);
            g.player.money = 3;
            g.player.hp = 4;
            g.location = LOCATION;
            let out = turn(&mut g, "h");
            assert_eq!(
                out.last().unwrap(),
                "^2Здоровья 9/20",
                "1000:d66d is on every path"
            );
            let head = out[0].clone();
            if head == zero {
                assert_eq!(out.len(), 3, "1000:d63b then 1000:d654 then 1000:d66d");
                assert_eq!(out[1], "^0Док: Молчи животное!");
            } else {
                assert_eq!(out.len(), 2, "1000:d619 / 1000:d639 skip to 1000:d66d");
            }
            for &l in &[one, two, zero] {
                if head == l && !seen.contains(&l) {
                    seen.push(l);
                }
            }
            if seen.len() == 3 {
                break;
            }
        }
        assert_eq!(seen.len(), 3, "all three arms of 1000:d5fb/1000:d61e");
    }

    // -- the loop top ----------------------------------------------------

    /// `1000:d4ba`'s fall-through is the eject. Healing the last point of
    /// health with `h` must therefore print the doctor's line and put the
    /// player back on the street in the SAME turn -- the back edge
    /// `1000:d6c5` returns to `1000:d4ba`, not to the prompt.
    #[test]
    fn the_loop_top_ejects_as_soon_as_the_player_is_whole() {
        let mut g = vet(3);
        g.player.hp = 16; // 16 + 5 clamps to 20
        let out = turn(&mut g, "h");
        assert_eq!(
            out.last().unwrap(),
            "^0Док: вали отсюда ты здоров.",
            "1000:d4e5, after 1000:d683"
        );
        assert_eq!(g.location, Location::Street, "1000:d4ea -> 1000:d6c8");
    }

    /// And a standing break holds the player in even at full hp: all three
    /// tests must pass for the fall-through to be reached.
    #[test]
    fn the_loop_top_keeps_a_broken_player_in_at_full_health() {
        for (jaw, leg) in [(true, false), (false, true)] {
            let mut g = vet(0);
            g.player.broken_jaw = jaw;
            g.player.broken_leg = leg;
            assert!(turn(&mut g, "zzz").is_empty(), "1000:d4c8 / 1000:d4cf");
            assert_eq!(g.location, LOCATION);
        }
    }

    /// Entering the vet whole prints the intro, then the doctor's line, and
    /// leaves -- `1000:d3f4` skips the menu to `1000:d4ba` and the eject is
    /// there, not in the menu block.
    #[test]
    fn entering_whole_prints_the_intro_then_ejects() {
        let mut g = vet(100);
        g.location = Location::Street;
        let out = capture::lines(|| {
            g.enter_vet_for_test();
        });
        assert_eq!(
            out,
            vec![
                "Ты пришел на ремот, к ветеринару напиши  ^6w^7  чтобы уйти",
                "^0Док: вали отсюда ты здоров.",
            ],
            "1000:d3ce then 1000:d4e5; 1000:d3f4 skipped the menu"
        );
        assert_eq!(g.location, Location::Street);
    }

    // -- the exits -------------------------------------------------------

    /// **Two exit keys.** `w` at `1000:d6ad` and `e` at `1000:d6be` both
    /// `jz 0xd6c8`. `e` is the street's quit token (file `0xB43E`), but the
    /// vet reads its own buffer, so here it only leaves the vet.
    #[test]
    fn both_w_and_e_leave_the_vet_and_e_does_not_quit() {
        for key in ["w", "e"] {
            let mut g = vet(0);
            g.player.hp = 1;
            assert!(turn(&mut g, key).is_empty(), "{key}");
            assert_eq!(g.location, Location::Street, "{key}: 1000:d6c8");
            // Quitting is `Command::Quit`'s, reached only from `entry`'s
            // own compare at 1000:edfa on the STREET buffer; the vet's
            // 1000:d6be reads 20ae:3a72 and can never get there.
            assert!(g.mode_is_street(), "{key} left the shop, it did not quit");
        }
        assert!(exits("w") && exits("e"));
        assert!(!exits("r") && !exits("h"));
    }

    /// **There is no unknown-key line.** `1000:d6c5 jmp 0xd4ba` is the back
    /// edge and it targets the loop top, not the menu; the CS-literal sweep
    /// over the range finds no refusal literal for a bad key.
    #[test]
    fn an_unrecognised_key_prints_nothing_and_stays_in_the_vet() {
        for key in ["1", "2", "x", "", "hp"] {
            let mut g = vet(100);
            g.player.hp = 1;
            let before = g.player.clone();
            assert!(turn(&mut g, key).is_empty(), "{key:?} must print nothing");
            assert_eq!(g.player, before);
            assert_eq!(g.location, LOCATION, "1000:d6c5 -> 1000:d4ba");
        }
    }

    /// The vet's own `ReadLn` at `1000:d528` lowercases and does not trim,
    /// while `Game::shop_turn` trims -- the standing trimmed-prompt
    /// divergence in `docs/re/gaps.md`, whose population the vet joins.
    #[test]
    fn the_vet_prompt_is_case_insensitive_and_accepts_untrimmed_input() {
        let mut g = vet(3);
        g.player.hp = 4;
        assert!(!turn(&mut g, " H").is_empty(), "trimmed here, a miss there");
        assert_eq!(g.player.hp, 9);
    }
}
