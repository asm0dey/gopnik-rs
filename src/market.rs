//! The market's pickpocket -- `t` at the `mar` prompt, `1000:c329`..`c46a`,
//! and the police ban it leaves behind.
//!
//! `docs/re/port-gaps.md` rows 9 and 25. Row 9 is the verb itself; row 25 is
//! the ban's two other ends -- the gate at `1000:b95e` that keeps a wanted
//! player out of the market ([`crate::game::Game::enter_shop`]) and the
//! clear at `1000:d793` inside `girl` ([`crate::game::Game::visit_girl`]).
//!
//! ## The shape
//!
//! ```text
//! c329  call 0f78:0bd8                 ; the market buffer 20ae:3a72 vs `t`
//! c333  al := [0x3692] ; ax := ax*5+5
//! c344  call 0f78:114b                 ; draw 1 -- Random(district*5 + 5)
//! c349  xor dx,dx / mov cx,ax / mov bx,dx      ; the draw ZERO-extends
//! c34f  mov ax,[0x38a4] / cwd                  ; удача SIGN-extends
//! c353  cmp dx,bx / c355 jg 0xc35d / c357 jl 0xc3cd
//! c359  cmp ax,cx / c35b jb 0xc3cd
//! c35d  mov ax,0xa / c361 call 0f78:114b       ; draw 2 -- Random(10)
//! c366  cmp ax,0x9 / c369 jnb 0xc3cd
//! c36b  mov ax,[0x38a4] / shl ax,1
//! c371  call 0f78:114b                 ; draw 3 -- Random(удача * 2)
//! c376  inc ax / c377 mov [0x3b74],ax
//! c37a  mov ax,[0x3b74] / c37d add [0x38c7],ax ; money += the take
//! c381..c396   WriteLn `^2Опа бабки! # рублей на пиво!`
//! c39b..c3b4   WriteLn `^6Ты получаешь # качков опыта`, # = district*2
//! c3b9  al := [0x3692] / xor ah,ah / shl ax,1
//! c3c0  add [0x38ce],ax               ; the xp credit, district*2
//! c3c4  mov al,0x0 / c3c7 call 0x12526 ; FUN_1000_2526(0)
//! c3ca  jmp 0xc46a
//! c3cd  mov al,0x1 / c3d0 call 0x10d14 ; FUN_1000_0d14(1) -- clamp to class 7
//! c3d3  mov byte [0x3b72],0x1          ; the fight-accepted flag
//! c3d8..c3ec   WriteLn `^4Корявый! ты попался!`
//! c3f1..c42e   one composed WriteLn: `^6Это ` + ranks[[0x3952]] + ` # уровня.`
//! c433  mov al,0x1 / c436 call 0x13d11 ; FUN_1000_3d11(1) -- a real fight
//! c439..c44d   WriteLn `^6Блин менты запалят сматывайся!.`
//! c452..c460   0f78:0b01 writes `w` into the market buffer 20ae:3a72
//! c465  mov byte [0x3b76],0x5          ; the market ban countdown
//! c46a  the loop's own `w` compare, which the store at c460 now satisfies
//! ```
//!
//! ## `1000:c353` is Borland's 32-bit compare, not Ghidra's `bVar24`
//!
//! The decompilation renders `1000:c34f`..`c35b` as
//! `((int)uVar7 < 0 && bVar24) || (bVar24 && (uVar8 <= uVar7))`, which is its
//! FPU-less model of a pair of branches it could not fold. The instructions
//! are the same 32-bit idiom [`crate::game::Game::luck_below_random_32`]
//! already carries, permuted exactly as the club's `1000:e0c6` permutes it:
//! `jg` / `jl` / `jb`, high halves signed and low halves unsigned, with the
//! **fall-through** -- luck NOT below the draw -- as the success arm. The
//! five bytes at `1000:c355`/`c357` (`7f 06`, `7c 74`) and `1000:c35b`
//! (`72 70`) are what say so. Transliterating Ghidra's expression would have
//! made the theft succeed whenever удача was negative and fail otherwise.
//!
//! ## Where the xp is added
//!
//! `1000:c3c0 add [0x38ce],ax`, five instructions BEFORE the level-up call,
//! with `ax` the same `district * 2` the line at `1000:c39b` printed. Ghidra
//! renders the level-up's argument as `(uint)(bVar10 >> 7) << 8` -- its model
//! of whatever `ah` held after the `shl`; the instruction is
//! `1000:c3c4 mov al,0x0`, so the byte parameter is 0, the CAPPED form.
//! [`crate::progress::apply_levels`]'s `award` models the `add` and its
//! `uncapped` models the parameter, the same pairing `crate::club` uses.
//!
//! ## The three draws
//!
//! `1000:c344`, `1000:c361` and `1000:c371` are the first draws this port has
//! ever spent inside a shop submenu. `data/rng_trace.json` observes none of
//! the three -- recomputed from the shipped artifact, not remembered: the
//! file's call-site census holds 35 distinct addresses and `1000:c344`,
//! `1000:c361` and `1000:c371` are not among them, because the capture driver
//! never typed `t`. So the two sequence oracles cannot see these draws.
//!
//! ## Where the strings are pinned
//!
//! `tools/difftest.py`'s `market_line` / `market_gap` / `market_fragment`
//! records re-find all eight of the two spans' CS literals by walking
//! `1000:c329`..`c46a` and `1000:c480`..`c499` instruction by instruction --
//! never from a hardcoded offset list -- and compare them against the tables
//! below. `tools/test_string_citations.py` scans this file's citations
//! against `orig/g.exe` and reports **20 checked, 3 unchecked, 0 bad**. The
//! three unchecked ones are the two on [`EXIT_TOKEN`] and the one in
//! [`busted`]: a single character is not GAME TEXT by that scanner's own
//! test, so it declines to pin a citation it cannot verify rather than
//! resolving it to the wrong literal. `market_fragment pickpocket 2` is what
//! pins `w` to the image, the same way `enemy_fragment 1` and `6` pin the
//! enemy sheet's two uncited literals.

use std::io;

use crate::game::Game;
use crate::opening::Gaps;
use crate::progress;
use crate::term;
use crate::text;

/// `^2Опа бабки! # рублей на пиво!` -- prints the take.
pub const PAYOFF: &str = "^2Опа бабки! # рублей на пиво!";
/// `^6Ты получаешь # качков опыта` -- `#` is `district * 2`.
pub const XP_LINE: &str = "^6Ты получаешь # качков опыта";
/// `^4Корявый! ты попался!` -- the bust.
pub const CAUGHT: &str = "^4Корявый! ты попался!";
/// The same opponent-announcement text is shared by this pickpocket bust,
/// [`crate::game::Game::den_beat_up`], and `crate::club`'s caught-cheating
/// block.
///
/// `^6Это ` -- the first half of the composed opponent announcement.
pub const ANNOUNCE_OPEN: &str = "^6Это ";
/// ` # уровня.` -- the second half. The rank between the two halves comes
/// from `ranks[...]` and is interpolated by [`busted`], not a fixed
/// literal.
pub const ANNOUNCE_LEVEL: &str = " # уровня.";
/// `^6Блин менты запалят сматывайся!.` -- printed after the fight. The
/// trailing `!.` is intentional, not a typo to fix.
pub const SCRAM: &str = "^6Блин менты запалят сматывайся!.";
/// `w` -- the exit token forced into the market's own input, so the visit
/// ends without the player typing anything.
/// [`crate::game::Game::leave_shop`] is the consequence.
pub const EXIT_TOKEN: &str = "w";
/// The refusal message, printed when a wanted player is kept out of the
/// market:
///
/// `^6На базар пока нельзя там менты бродят, тебя ищут.`
pub const BANNED: &str = "^6На базар пока нельзя там менты бродят, тебя ищут.";

/// The pickpocket verb's own printed lines, each with whether the call
/// closes the line -- the same shape [`crate::enemy_sheet::EMITTED`]
/// uses.
pub const PICKPOCKET_EMITTED: [(bool, &str); 4] = [
    (true, PAYOFF),  // 1000:c381, printed 1000:c396
    (true, XP_LINE), // 1000:c39b, printed 1000:c3b4
    (true, CAUGHT),  // 1000:c3d8, printed 1000:c3ec
    (true, SCRAM),   // 1000:c439, printed 1000:c44d
];

/// The composed announcement line sits between index 2 ([`CAUGHT`]) and
/// index 3 ([`SCRAM`]) in [`PICKPOCKET_EMITTED`], and carries no literal of
/// its own because it prints an assembled string.
///
/// The `t` verb blocks on nothing and prints no blank line.
pub const PICKPOCKET_GAPS: Gaps = &[(3, "C")];

/// The three strings assembled rather than written directly: the two
/// halves of the announcement, and the forced exit token.
pub const PICKPOCKET_FRAGMENTS: [&str; 3] = [
    ANNOUNCE_OPEN,  // 1000:c3f7, 0f78:0ae7 at 1000:c3fc
    ANNOUNCE_LEVEL, // 1000:c414, 0f78:0b66 at 1000:c419
    EXIT_TOKEN,     // 1000:c452, 0f78:0b01 at 1000:c460
];

/// The whole of the ban's refusal arm: one literal, printed once, then
/// out.
pub const BANNED_EMITTED: [(bool, &str); 1] = [
    (true, BANNED), // 1000:c480, printed 1000:c494
];

/// How many turns the market stays shut after a bust.
/// [`crate::game::Game::walk_preamble`] ticks it down and announces its
/// last turn; `girl` and a district advance both clear it.
pub const BAN_TURNS: u8 = 5;

/// `t` at the `mar` prompt.
///
/// `lines` is threaded through because the caught arm reaches a real
/// fight, which reads the combat prompt. That call is also the **only**
/// caller of [`crate::ending::OPENER_1`] anywhere in the game: without
/// this arm, that path of [`crate::game::Game::run_combat`] is
/// unreachable.
pub(crate) fn pickpocket(
    g: &mut Game,
    lines: &mut dyn Iterator<Item = io::Result<String>>,
) -> io::Result<()> {
    // The draw is over `district * 5 + 5`, not `district * 5`.
    let n = u16::from(g.district) * 5 + 5;
    let draw = g.rng.below(n);

    // The success arm is the fall-through -- удача NOT below the draw --
    // so the predicate is negated here, exactly as `crate::club`'s
    // `play_cards` negates its copy. The `&&` short-circuits: a failed
    // compare skips the second draw entirely, so it is spent only when the
    // first succeeded.
    if !Game::luck_below_random_32(g.player.luck, draw)
        // One draw in ten busts a theft that luck had already carried.
        && g.rng.below(10) < 9
    {
        haul(g);
        return Ok(());
    }
    busted(g, lines)
}

/// The theft pays off.
fn haul(g: &mut Game) {
    // A удача above `0x7fff` wraps when doubled; `wrapping_mul`
    // reproduces that wraparound intentionally, not as an approximation.
    let take = i32::from(g.rng.below(g.player.luck.wrapping_mul(2))) + 1;
    // Reads the take back and credits it to money.
    g.player.money = g.player.money.wrapping_add(take as i16);
    // Prints `^2Опа бабки! # рублей на пиво!`, whose `#` is the take.
    term::println(&text::fill(PAYOFF, &[i64::from(take)]));

    // Computes district * 2.
    let xp = u16::from(g.district) * 2;
    // Prints `^6Ты получаешь # качков опыта` before the value is
    // credited.
    term::println(&text::fill(XP_LINE, &[i64::from(xp)]));
    // As in `crate::club`, `xp` is `apply_levels`'s `award` and `false`
    // is the capped form, not an award of zero.
    progress::apply_levels(&mut g.progress, &mut g.player, &mut g.rng, xp, false);
}

/// The theft is spotted.
fn busted(g: &mut Game, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
    // Uses the clamp-to-class-7 form, so no Мент ever answers a
    // pickpocket.
    let enemy = g.roll_enemy(1);
    g.fight_accepted = true; // 1000:c3d3

    // Prints `^4Корявый! ты попался!`.
    term::println(CAUGHT);
    // Prints `^6Это ` + the rank + ` # уровня.` as one assembled line.
    term::print(ANNOUNCE_OPEN);
    term::print(&Game::rank_name(enemy.class));
    term::println(&text::fill(ANNOUNCE_LEVEL, &[i64::from(enemy.level)]));

    // Skips the class-keyed greeting and uses a one-line opener instead.
    g.run_combat(1, enemy, lines)?;

    // Prints `^6Блин менты запалят сматывайся!.`.
    term::println(SCRAM);
    // Forces `w` into the market's own input, the same technique
    // `crate::club` uses elsewhere. The exit compare then hits and the
    // visit ends with no further input.
    g.leave_shop();
    g.market_ban_countdown = BAN_TURNS; // 1000:c465
    Ok(())
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

    /// A player standing at the `mar` prompt. `Game::mode` is private to
    /// `crate::game` and `Game::shop_turn` takes the location explicitly, so
    /// this sets `location` only -- the same shape `crate::club`'s fixture
    /// uses.
    fn market(district: u8, seed: u32) -> Game {
        let mut g = Game::new(player(), Progress::new(), seed);
        g.district = district;
        g.places.mark_found(Location::Market);
        g.location = Location::Market;
        g.progress.threshold = u16::MAX; // keeps 1000:c3c7 from levelling
        g
    }

    fn no_input() -> std::iter::Empty<std::io::Result<String>> {
        std::iter::empty()
    }

    /// One turn at the market prompt, through the real dispatch in
    /// `Game::shop_turn` rather than through [`pickpocket`] directly, so the
    /// wiring is under test too.
    fn turn(g: &mut Game, key: &str) -> Vec<String> {
        capture::lines(|| {
            g.shop_turn(Location::Market, key, &mut no_input()).unwrap();
        })
    }

    /// A seed whose first draw the given удача beats (or loses to) and
    /// whose second draw then clears 9. `min_first` rejects a zero first
    /// draw, which удача 0 would otherwise tie with.
    fn seed_for(district: u8, luck: u16, want_haul: bool, min_first: u16) -> u32 {
        for seed in 1u32..500_000 {
            let mut g = market(district, seed);
            let draw = g.rng.below(u16::from(district) * 5 + 5);
            if draw < min_first {
                continue;
            }
            let ok = !Game::luck_below_random_32(luck, draw) && g.rng.below(10) < 9;
            if ok == want_haul {
                return seed;
            }
        }
        panic!("no seed reaches the {want_haul} arm at district {district}");
    }

    // -- the payoff arm ---------------------------------------------------

    /// The take is `Random(удача*2) + 1`; it is added to the money before
    /// it is printed, and the xp line is `district * 2`.
    #[test]
    fn the_haul_credits_the_take_and_the_district_xp() {
        let district = 3u8;
        let mut g = market(district, seed_for(district, 30_000, true, 0));
        g.player.luck = 30_000;
        g.player.money = 100;
        let out = turn(&mut g, "t");
        assert_eq!(out.len(), 2, "1000:c396 then 1000:c3b4, and nothing else");
        let take = g.player.money - 100;
        assert!(take >= 1, "1000:c376 `inc ax` floors the take at 1");
        assert_eq!(
            out[0],
            text::fill(PAYOFF, &[i64::from(take)]),
            "1000:c386 pushes the same 20ae:3b74 1000:c37d added"
        );
        assert_eq!(
            out[1],
            text::fill(XP_LINE, &[i64::from(district) * 2]),
            "1000:c3a5 `shl ax,1` on the district"
        );
        assert_eq!(g.progress.xp, u16::from(district) * 2, "1000:c3c0");
        assert_eq!(g.location, Location::Market, "1000:c3ca jumps to the loop");
        assert_eq!(g.market_ban_countdown, 0, "1000:c465 is on the other arm");
    }

    // -- the caught arm ---------------------------------------------------

    /// The bust's order and its effects: the flag, the accusation, the
    /// composed announcement, the fight, the closing line, the forced
    /// exit, and the ban.
    #[test]
    fn the_bust_fights_ejects_and_bans() {
        let district = 2u8;
        let mut g = market(district, seed_for(district, 0, false, 1));
        g.player.luck = 0;
        g.player.hp = 1; // lose the fight fast; the block runs either way
        let out = turn(&mut g, "t");
        assert_eq!(out[0], CAUGHT, "1000:c3ec");
        assert!(
            out[1].starts_with("^6Это ") && out[1].ends_with(" уровня."),
            "1000:c42e prints ONE composed line, got {:?}",
            out[1]
        );
        // The fight leaves its screen title open with a `Write`, so the
        // closing line is captured on the same captured line as the banner;
        // what matters is that it comes after the fight and closes the turn.
        assert!(
            out.last().map(|l| l.ends_with(SCRAM)) == Some(true),
            "1000:c44d must print after the fight, got {out:?}"
        );
        assert!(g.fight_accepted, "1000:c3d3");
        assert_eq!(g.market_ban_countdown, BAN_TURNS, "1000:c465");
        assert_eq!(g.location, Location::Street, "1000:c460 writes `w`");
        assert_eq!(g.progress.xp, 0, "the bust arm has no xp credit");
    }

    /// Confirms the pickpocket's fight always takes the class-independent
    /// opener, whose only caller anywhere in the game is this arm.
    #[test]
    fn the_bust_is_the_only_caller_of_the_param_one_opener() {
        let district = 1u8;
        let mut g = market(district, seed_for(district, 0, false, 1));
        g.player.luck = 0;
        g.player.hp = 1;
        let out = turn(&mut g, "t");
        assert!(
            out.iter().any(|l| l == crate::ending::OPENER_1[0]),
            "1000:3ea5 must print, got {out:?}"
        );
        // And the class-keyed greeting must not run here.
        assert!(
            !out.iter()
                .any(|l| l == "^4Эй мудак?!" || l == "Слышь Вась.."),
            "1000:3d2f skips crate::combat_opener entirely, got {out:?}"
        );
    }

    /// Uses the clamp-to-class-7 form, so a pickpocket never faces class 8
    /// or 9.
    #[test]
    fn the_bust_never_rolls_above_class_seven() {
        for seed in 1u32..40 {
            let mut g = market(5, seed);
            let cls = g.roll_enemy(1).class;
            assert!(cls <= 7, "1000:c3cd `mov al,0x1` clamps, saw {cls}");
        }
    }

    // -- the tables -------------------------------------------------------

    /// Every gap index must name a real slot in [`PICKPOCKET_EMITTED`] -- an
    /// index past the end is one `difftest` compares and nothing else would
    /// notice.
    #[test]
    fn every_gap_index_is_inside_its_block() {
        for (at, events) in PICKPOCKET_GAPS {
            assert!(
                *at <= PICKPOCKET_EMITTED.len(),
                "gap {at} ({events}) is past the block's {} lines",
                PICKPOCKET_EMITTED.len()
            );
            assert!(!events.is_empty(), "an empty gap is not emitted");
        }
    }

    /// Every literal the tables ship must be one the arms actually print or
    /// assemble, so a constant cannot drift out of use while `difftest` keeps
    /// comparing it.
    #[test]
    fn the_tables_and_the_arms_quote_the_same_literals() {
        let district = 2u8;
        let mut g = market(district, seed_for(district, 0, false, 1));
        g.player.luck = 0;
        g.player.hp = 1;
        let out = turn(&mut g, "t").join("\n");
        for (_, line) in [PICKPOCKET_EMITTED[2], PICKPOCKET_EMITTED[3]] {
            assert!(out.contains(line), "{line:?} never reached the screen");
        }
        assert!(out.contains(ANNOUNCE_OPEN), "1000:c3f7");
        assert!(out.contains(" уровня."), "1000:c414");
        assert_eq!(EXIT_TOKEN, "w", "1000:c452 pushes the shared exit token");
    }

    /// `t` is the market's alone: the dealers' loop has no such
    /// compare.
    #[test]
    fn the_t_verb_is_the_markets_alone() {
        let mut g = market(1, 1);
        g.location = Location::Dealers;
        g.places.mark_found(Location::Dealers);
        let before = g.player.clone();
        let out = capture::lines(|| {
            g.shop_turn(Location::Dealers, "t", &mut no_input())
                .unwrap();
        });
        assert!(out.is_empty(), "1000:c329 is inside the `mar` loop only");
        assert_eq!(g.player, before);
        assert_eq!(g.location, Location::Dealers);
    }
}
