//! The class-keyed combat opener -- `1000:3d32`..`1000:3e8d`.
//!
//! The first thing `FUN_1000_3d11` does, on the two `param_1` values that
//! reach it, is greet the enemy. `docs/re/combat-opener.md` is the map and
//! `data/combat_opener.json` its machine-readable twin; this module is the
//! port of it, and nothing else.
//!
//! **Established from flow** throughout: every address below was re-derived
//! from `orig/g.exe` with `python3 tools/re_query.py resolve <citation>`
//! before it was written down, and every string was decoded as a
//! length-prefixed CP866 shortstring at the file offset the `mov di,imm16`
//! names (`cs_offset + 0x18d0`). `tools/test_combat_opener.py` re-derives
//! both halves from the image.
//!
//! ## Two dispatches, not one
//!
//! `1000:3d24 mov al,[bp+0x4]` reads the FUNCTION PARAMETER, and
//! `1000:3d27`/`1000:3d29` and `1000:3d2b`/`1000:3d2d` send `param_1` 0 and 6
//! to `1000:3d32`; everything else takes `1000:3d2f jmp 0x3e8d` and never
//! enters this block. That gate is [`crate::game::Game::run_combat`]'s, at
//! the call site. The chain THIS module implements is the inner one:
//! `1000:3d32 mov ax,[0x3952]` loads the **rolled enemy's class** and ten
//! `cmp ax,N` links key on it. Calling the outer pair "classes 0 and 6"
//! collapses two different variables, which is the one wording
//! `docs/re/combat-opener.md` corrects in `docs/re/gaps.md`.
//!
//! ## Ten class values, six arms
//!
//! | classes | gates, in the original's order | arm | exit |
//! |---|---|---|---|
//! | 0, 1, 2 | `1000:3d35`/`3d38`, `3d3a`/`3d3d`, `3d3f`/`3d42` | `1000:3d44` | `1000:3d76 jmp 0x3e8a` |
//! | 3, 4, 5, 6 | `1000:3d79`/`3d7c`, `3d7e`/`3d81`, `3d83`/`3d86`, `3d88`/`3d8b` | `1000:3d8d` | `1000:3dbf jmp 0x3e8a` |
//! | 7 | `1000:3dc2`/`3dc5` | `1000:3dc7` | `1000:3de0 jmp 0x3e8a` |
//! | 8 | `1000:3de3`/`3de6` | `1000:3de8` | `1000:3e33 jmp short 0x3e8a` |
//! | 9 | `1000:3e35`/`3e38` | `1000:3e3a` | falls into `1000:3e8a` |
//! | >= 10 | -- | -- | `1000:3e38 jnz 0x3e8a`, silent |
//!
//! The eleventh case is real, not a Rust artefact: `1000:3e38` is the chain's
//! last miss, so any `[0x3952]` the ten compares do not name reaches the exit
//! having printed nothing. `1000:11d0 mov word [0x3952],0xa` does store such a
//! value (`Ректор НГУ`); whether that value ever reaches this chain is open --
//! `docs/re/combat-opener.md`, "The eleventh case prints nothing".
//!
//! ## It spends no draw and writes no state
//!
//! **Established from flow.** Zero `9a 4b 11 78 0f` across the 168
//! instructions of the range (measured against 86 image-wide, so the zero is a
//! measurement and not a scan looking wrongly), and zero absolute-memory
//! writes: the only memory the two shortstring helpers touch is the stack
//! local at `ss:[bp-0x218]` (`1000:3de8` / `1000:3e53` push `ss`, not `ds`).
//! So this module takes no [`crate::rng::Rng`] and returns nothing -- adding
//! it cannot move a draw or a byte of state, which is why
//! `tests/combat_sequence.rs` and the five frozen oracles under `data/` are
//! unaffected by it.

use crate::term;

/// Write the greeting for a rolled enemy of `enemy_class`.
///
/// `player_name` is `20ae:379c` and `player_rank` is
/// `ranks[[0x389c]]` -- `crate::game::Game::rank_name(self.player.class)`,
/// the same `[0x389c] shl 8 + 0x2e` table `1000:3e14` and `1000:3e6b` index.
/// Both are the **player's**, not the enemy's: `1000:3e0c` and `1000:3e63`
/// read `20ae:389c`, the player's record.
pub fn greet(enemy_class: u16, player_name: &str, player_rank: &str) {
    match enemy_class {
        // 1000:3d35 `cmp ax,0x0` / 1000:3d38 `jz 0x3d44`
        // 1000:3d3a `cmp ax,0x1` / 1000:3d3d `jz 0x3d44`
        // 1000:3d3f `cmp ax,0x2` / 1000:3d42 `jnz 0x3d79`
        0..=2 => {
            // CS 0x2c5e / file 0x452E `Слышь Вась..`, pushed at 1000:3d44,
            // printed 1000:3d58.
            term::println("Слышь Вась..");
            // CS 0x2c6b / file 0x453B `^4А чё ваще?`, pushed at 1000:3d5d,
            // printed 1000:3d71.
            term::println("^4А чё ваще?");
        }
        // 1000:3d79 `cmp ax,0x3` / 1000:3d7c `jz 0x3d8d`
        // 1000:3d7e `cmp ax,0x4` / 1000:3d81 `jz 0x3d8d`
        // 1000:3d83 `cmp ax,0x5` / 1000:3d86 `jz 0x3d8d`
        // 1000:3d88 `cmp ax,0x6` / 1000:3d8b `jnz 0x3dc2`
        3..=6 => {
            // CS 0x2c78 / file 0x4548 `^4Пацан ты из какого района?`, pushed
            // at 1000:3d8d, printed 1000:3da1.
            term::println("^4Пацан ты из какого района?");
            // CS 0x2c95 / file 0x4565 `А ты по пинкам суди!`, pushed at
            // 1000:3da6, printed 1000:3dba.
            term::println("А ты по пинкам суди!");
        }
        // 1000:3dc2 `cmp ax,0x7` / 1000:3dc5 `jnz 0x3de3`
        7 => {
            // CS 0x2caa / file 0x457A `^4Эй мудак?!`, pushed at 1000:3dc7,
            // printed 1000:3ddb.
            term::println("^4Эй мудак?!");
        }
        // 1000:3de3 `cmp ax,0x8` / 1000:3de6 `jnz 0x3e35`
        8 => {
            // ONE WriteLn (1000:3e2e) of a string assembled in the stack local
            // at `ss:[bp-0x218]` (1000:3de8): assign CS 0x2cb7 (1000:3df3),
            // append the player's name at 20ae:379c (1000:3dfd), append CS
            // 0x2cc7 (1000:3e07), append ranks[[0x389c]] (1000:3e1a). The
            // three `term::print`s below join into that one captured line.
            // CS 0x2cb7 / file 0x4587 `^4Блин! это же `
            term::print("^4Блин! это же ");
            term::print(player_name);
            // CS 0x2cc7 / file 0x4597 `^4 - известный `
            term::print("^4 - известный ");
            term::println(player_rank);
        }
        // 1000:3e35 `cmp ax,0x9` / 1000:3e38 `jnz 0x3e8a`
        9 => {
            // CS 0x2cd7 / file 0x45A7 `^4Я МАНЬЯК!!!`, pushed at 1000:3e3a,
            // printed 1000:3e4e.
            term::println("^4Я МАНЬЯК!!!");
            // The second line is assembled in the same stack local
            // (1000:3e53): assign CS 0x2ce5 (1000:3e5e), append
            // ranks[[0x389c]] (1000:3e71), one WriteLn at 1000:3e85.
            // CS 0x2ce5 / file 0x45B5 `Рад познакомиться - `
            term::print("Рад познакомиться - ");
            term::println(player_rank);
        }
        // 1000:3e38's own miss. Not a Rust catch-all standing in for a value
        // the key cannot take: `[0x3952]` really can hold 10 (1000:11d0), and
        // the original prints nothing for it either.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::capture;

    /// The nine strings, re-decoded here from `orig/g.exe` rather than
    /// quoted, so a typo in the arm bodies above reds this test instead of
    /// agreeing with itself.
    ///
    /// `data/combat_opener.json` names the `cs_offset` of every push;
    /// `0x18d0` is the MZ header size `tools/addr.py` derives from
    /// `e_cparhdr`. The arms are then compared against the image's own bytes.
    fn strings_from_the_image() -> std::collections::HashMap<String, String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let image = std::fs::read(root.join("orig/g.exe")).expect("read orig/g.exe");
        let map: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("data/combat_opener.json")).unwrap())
                .expect("parse data/combat_opener.json");
        let mut out = std::collections::HashMap::new();
        for arm in map["arms"].as_array().expect("arms") {
            for s in arm["strings"].as_array().expect("strings") {
                let cs = s["cs_offset"].as_str().expect("cs_offset");
                let cs = usize::from_str_radix(cs.trim_start_matches("0x"), 16).expect("hex");
                let at = cs + 0x18d0;
                let len = image[at] as usize;
                let text: String = image[at + 1..at + 1 + len]
                    .iter()
                    .map(|&b| cp866(b))
                    .collect();
                out.insert(cs.to_string(), text);
            }
        }
        assert_eq!(out.len(), 9, "the range pushes nine CS literals");
        out
    }

    /// The CP866 upper half, exactly as `tools/extract_strings.py` decodes
    /// it: 0x80..0xAF is А..п, 0xE0..0xEF is р..я, and 0xF0/0xF1 are Ё/ё.
    fn cp866(b: u8) -> char {
        match b {
            0x00..=0x7f => b as char,
            0x80..=0xaf => char::from_u32(0x410 + (b - 0x80) as u32).unwrap(),
            0xe0..=0xef => char::from_u32(0x440 + (b - 0xe0) as u32).unwrap(),
            0xf0 => 'Ё',
            0xf1 => 'ё',
            _ => panic!("byte {b:#04x} is outside the range these nine strings use"),
        }
    }

    fn at(class: u16) -> Vec<String> {
        capture::lines(|| greet(class, "Вася", "Гопник"))
    }

    /// The whole chain, arm by arm, against the image's own bytes.
    ///
    /// This is the test the brief asks for: it fails if an arm's CLASS KEY is
    /// wrong (a class routed to the wrong arm), if an arm's STRING is wrong
    /// (the literal no longer matches `orig/g.exe`), or if an arm's line
    /// ORDER is wrong.
    #[test]
    fn every_class_key_reaches_the_arm_the_image_says_it_does() {
        let s = strings_from_the_image();
        let g = |cs: usize| s[&cs.to_string()].clone();

        // 1000:3d35 / 1000:3d3a / 1000:3d3f -- one arm, two plain lines.
        for class in [0u16, 1, 2] {
            assert_eq!(at(class), vec![g(0x2c5e), g(0x2c6b)], "class {class}");
        }
        // 1000:3d79 / 1000:3d7e / 1000:3d83 / 1000:3d88.
        for class in [3u16, 4, 5, 6] {
            assert_eq!(at(class), vec![g(0x2c78), g(0x2c95)], "class {class}");
        }
        // 1000:3dc2 -- the shortest arm, one line.
        assert_eq!(at(7), vec![g(0x2caa)]);
        // 1000:3de3 -- ONE line, four pieces, name then rank.
        assert_eq!(at(8), vec![format!("{}Вася{}Гопник", g(0x2cb7), g(0x2cc7))]);
        // 1000:3e35 -- two lines, the plain one FIRST.
        assert_eq!(at(9), vec![g(0x2cd7), format!("{}Гопник", g(0x2ce5))]);
        // 1000:3e38's miss. 10 is `Ректор НГУ`, the value 1000:11d0 stores.
        for class in [10u16, 11, 400] {
            assert!(at(class).is_empty(), "class {class} prints nothing");
        }
    }

    /// The arms are DISTINGUISHABLE. Without this the test above would still
    /// pass if every class routed to one arm whose strings happened to be
    /// compared against themselves.
    #[test]
    fn the_six_arms_print_six_different_things() {
        let mut seen = std::collections::HashSet::new();
        for class in [0u16, 3, 7, 8, 9, 10] {
            assert!(
                seen.insert(at(class)),
                "class {class} duplicates an earlier arm"
            );
        }
        assert_eq!(seen.len(), 6, "six arms, six outputs");
    }

    /// The class-8 and class-9 arms splice the PLAYER's name and rank, not
    /// the enemy's -- `1000:3e0c` and `1000:3e63` read `20ae:389c`, and
    /// `1000:3df8` reads `20ae:379c`. A port that spliced the enemy's would
    /// pass the arm test above with a fixture that happened to agree.
    #[test]
    fn the_spliced_name_and_rank_are_the_arguments_not_a_constant() {
        let out = capture::lines(|| greet(8, "Петя", "Ректор НГУ"));
        assert_eq!(out.len(), 1);
        assert!(
            out[0].contains("Петя") && out[0].contains("Ректор НГУ"),
            "{out:?}"
        );
        // Order: name first (1000:3dfd), rank last (1000:3e1a).
        assert!(
            out[0].find("Петя").unwrap() < out[0].find("Ректор НГУ").unwrap(),
            "{out:?}"
        );
        let nine = capture::lines(|| greet(9, "Петя", "Ректор НГУ"));
        assert_eq!(nine.len(), 2);
        assert!(!nine[0].contains("Ректор НГУ"), "the first line is plain");
        assert!(nine[1].ends_with("Ректор НГУ"), "{nine:?}");
    }
}
