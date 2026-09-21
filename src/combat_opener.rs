//! The class-keyed combat opener.
//!
//! The first thing the fight function does is greet the enemy based on the
//! enemy's class. This module is that opener only.

use crate::term;

/// Write the greeting for a rolled enemy of `enemy_class`.
///
/// `player_name` is the player's name and `player_rank` yields the player's
/// rank from the class table. Both are the **player's**, not the enemy's.
pub fn greet(enemy_class: u16, player_name: &str, player_rank: impl FnOnce() -> String) {
    match enemy_class {
        0..=2 => {
            term::println("Слышь Вась..");
            term::println("^4А чё ваще?");
        }
        3..=6 => {
            term::println("^4Пацан ты из какого района?");
            term::println("А ты по пинкам суди!");
        }
        7 => {
            term::println("^4Эй мудак?!");
        }
        8 => {
            term::print("^4Блин! это же ");
            term::print(player_name);
            term::print("^4 - известный ");
            term::println(&player_rank());
        }
        9 => {
            term::println("^4Я МАНЬЯК!!!");
            term::print("Рад познакомиться - ");
            term::println(&player_rank());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::capture;

    /// The nine greeting strings, decoded from the game binary.
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

    /// The CP866 encoding as decoded from the game.
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
        capture::lines(|| greet(class, "Вася", || "Гопник".to_string()))
    }

    /// The whole greeting chain, arm by arm, against the game's strings.
    #[test]
    fn every_class_key_reaches_the_arm_the_image_says_it_does() {
        let s = strings_from_the_image();
        let g = |cs: usize| s[&cs.to_string()].clone();

        for class in [0u16, 1, 2] {
            assert_eq!(at(class), vec![g(0x2c5e), g(0x2c6b)], "class {class}");
        }
        for class in [3u16, 4, 5, 6] {
            assert_eq!(at(class), vec![g(0x2c78), g(0x2c95)], "class {class}");
        }
        assert_eq!(at(7), vec![g(0x2caa)]);
        assert_eq!(at(8), vec![format!("{}Вася{}Гопник", g(0x2cb7), g(0x2cc7))]);
        assert_eq!(at(9), vec![g(0x2cd7), format!("{}Гопник", g(0x2ce5))]);
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

    /// The rank lookup is evaluated only by arms 8 and 9.
    #[test]
    fn the_rank_is_read_only_by_the_two_arms_that_read_it() {
        for class in [0u16, 1, 2, 3, 4, 5, 6, 7, 10, 400] {
            let out = capture::lines(|| {
                greet(class, "Вася", || {
                    panic!("class {class} must not read the rank table")
                })
            });
            assert!(
                !out.iter().any(|l| l.contains("Гопник")),
                "class {class}: {out:?}"
            );
        }
        // And the two that DO read it still do.
        for class in [8u16, 9] {
            let out = capture::lines(|| greet(class, "Вася", || "Гопник".to_string()));
            assert!(
                out.iter().any(|l| l.ends_with("Гопник")),
                "class {class}: {out:?}"
            );
        }
    }

    /// Classes 8 and 9 use the PLAYER's name and rank, not the enemy's.
    #[test]
    fn the_spliced_name_and_rank_are_the_arguments_not_a_constant() {
        let out = capture::lines(|| greet(8, "Петя", || "Ректор НГУ".to_string()));
        assert_eq!(out.len(), 1);
        assert!(
            out[0].contains("Петя") && out[0].contains("Ректор НГУ"),
            "{out:?}"
        );
        assert!(
            out[0].find("Петя").unwrap() < out[0].find("Ректор НГУ").unwrap(),
            "{out:?}"
        );
        let nine = capture::lines(|| greet(9, "Петя", || "Ректор НГУ".to_string()));
        assert_eq!(nine.len(), 2);
        assert!(!nine[0].contains("Ректор НГУ"), "the first line is plain");
        assert!(nine[1].ends_with("Ректор НГУ"), "{nine:?}");
    }
}
