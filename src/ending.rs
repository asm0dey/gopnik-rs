use crate::term;

pub const OPENER_1: [&str; 1] = ["^4Отдай кошелёк урод!"];

pub const OPENER_3: [&str; 4] = [
    "^2Ну вот мы и встретились мудак!",
    "^4Чё те нада козёл?!",
    "^2Урыть тебя ублюдок!",
    "^4Ну ты меня достал ща урою!",
];

pub const OPENER_4: [&str; 4] = [
    "^6Тут заходит настоящий ректор.",
    "^4Мудак! ты тупой дебил, думал что я идиот?",
    "^2Я думаю ты сконил!",
    "^4Ну тада сдохни!",
];

pub const ENDING_4: [&str; 5] = [
    "^1Ты замочил самого ректора!!! ТЫ САМЫЙ КРУТОЙ!!!",
    "^1Вновь сила торжествует над интелектом.",
    "^1После этого сразу началась анархия и полный беспредел.",
    "^1И не стыдно тебе гоп чёртов?",
    "^1А результат:",
];

pub const ENDING_3: [&str; 2] = [
    "^1Ты замочил самого ректора!!!^6 о чёрт! да это ж не ректор был.",
    "^6Это был проректор СУНЦа!",
];

pub const ERRAND_AWARDS: [(i32, &str); 2] = [
    (
        20,
        "^2Ты отпинал этого мудака - пацаны этого незабудут. Понтовость улутшилась на #.",
    ),
    (10, "^6Ты получаешь # качков опыта за помощь"),
];

pub const BANNER: [&str; 8] = [
    "│    │ ┌────┐ ┌────┐  ┌────┐     │    │ ┌────┐ │    │ ┌────┐",
    "│    │ │      │    │  │          │    │ │    │ │    │ │    │",
    "│    │ │      │    │  │           \\  /  │    │ │    │ │    │",
    "│   /│ │      │    │  │            \\/   │    │ │    │ │    │",
    "│  / │ │      ├────┘  ├────        /\\   │    │ ├────┤ │    │",
    "│ /  │ │      │       │           /  \\  ├────┤ │    │ ├────┤",
    "│/   │ │      │       │          │    │ │    │ │    │ │    │",
    "│    │ │      │       └────┘     │    │ │    │ │    │ │    │",
];

pub const BANNER_INDENT: &str = "          ^";

pub const DEATH_LINE: &str = "                                      ^4Ты сдох.";

pub const VICTORY_LINE: &str = "                                    ^2Ты победил.";

pub const ANY_KEY: &str = "                          ^6Нажми какую-нибудь кнопку";

pub const END_SCREEN_GAPS: &[(usize, &str)] = &[(0, "BB"), (2, "BBBCCCCCCCCBBBBB"), (3, "BBBBK")];

pub fn end_screen(victory: bool, lines: &mut dyn Iterator<Item = std::io::Result<String>>) {
    term::println("");
    term::println("");
    term::println(if victory { VICTORY_LINE } else { DEATH_LINE });
    for _ in 0..3 {
        term::println("");
    }
    let digit = if victory { '2' } else { '4' };
    for row in BANNER {
        term::println(&format!("{BANNER_INDENT}{digit}{row}"));
    }
    for _ in 0..5 {
        term::println("");
    }
    term::println(ANY_KEY);
    for _ in 0..4 {
        term::println("");
    }
    term::read_key(lines);
}

pub const MARQUEE_INDENT: usize = 32;

pub const MARQUEE_PHASES: u16 = 9;

pub fn marquee_frame(phase: u16) -> String {
    let d = |i: u16| char::from(b'0' + ((i + phase - 1) % 8) as u8);
    let mut out = String::new();
    for (i, fragment) in MARQUEE_FRAGMENTS.iter().enumerate() {
        out.push_str(fragment);
        // Ten digits for eleven fragments: every one but the last is
        // followed by its rotating colour digit, which is what makes the
        // final `П` carry none.
        if let Ok(nth) = u16::try_from(i + 1) {
            if nth < MARQUEE_FRAGMENTS.len() as u16 {
                out.push(d(nth));
            }
        }
    }
    out
}

pub const MARQUEE_FRAGMENTS: [&str; 11] = [
    "^", "Т^", "Ы ^", "С^", "У^", "П^", "Е^", "Р ^", "Г^", "О^", "П",
];

pub const MARQUEE_GAPS: &[(usize, &str)] = &[(0, "CKK")];

pub fn marquee(lines: &mut dyn Iterator<Item = std::io::Result<String>>) {
    for phase in 0..MARQUEE_PHASES {
        term::print(&" ".repeat(MARQUEE_INDENT));
        term::println(&marquee_frame(phase));
    }
    term::read_key(lines);
    term::read_key(lines);
    end_screen(true, lines);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_lines() -> std::vec::IntoIter<std::io::Result<String>> {
        Vec::new().into_iter()
    }

    #[test]
    fn phase_zero_restarts_the_digit_run_after_eight() {
        assert_eq!(marquee_frame(0), "^0Т^1Ы ^2С^3У^4П^5Е^6Р ^7Г^0О^1П");
    }

    /// `e = 8` and `e = 0` are the same frame: `(i + 8 - 1) mod 8` is
    /// `(i - 1) mod 8`. The cycle is nine passes long and eight frames wide,
    /// and that is the original's, not a rounding of it.
    #[test]
    fn the_last_phase_of_the_cycle_repeats_the_first() {
        assert_eq!(marquee_frame(8), marquee_frame(0));
        assert_eq!(MARQUEE_PHASES, 9);
    }

    #[test]
    fn every_frame_spells_the_same_words() {
        for phase in 0..MARQUEE_PHASES {
            assert_eq!(crate::text::strip(&marquee_frame(phase)), "ТЫ СУПЕР ГОП");
        }
    }

    /// The verdict line and the banner's colour digit are the only two
    /// things `param_1` changes, and they move together.
    #[test]
    fn the_verdict_picks_both_the_line_and_the_banner_colour() {
        let dead = term::capture::lines(|| end_screen(false, &mut no_lines()));
        let won = term::capture::lines(|| marquee(&mut no_lines()));
        assert!(dead.contains(&DEATH_LINE.to_string()));
        assert!(won.contains(&VICTORY_LINE.to_string()));
        assert!(dead.iter().any(|l| l.starts_with("          ^4│")));
        assert!(won.iter().any(|l| l.starts_with("          ^2│")));
        // Eight banner rows on each, and the same eight.
        for (side, colour) in [(&dead, '4'), (&won, '2')] {
            let rows: Vec<&String> = side
                .iter()
                .filter(|l| l.starts_with(&format!("{BANNER_INDENT}{colour}")))
                .collect();
            assert_eq!(rows.len(), BANNER.len());
            for (got, want) in rows.iter().zip(BANNER) {
                assert!(got.ends_with(want), "{got}");
            }
        }
    }

    #[test]
    fn the_four_blank_runs_are_two_three_five_four() {
        for victory in [false, true] {
            let out = term::capture::lines(|| end_screen(victory, &mut no_lines()));
            let runs: Vec<usize> = out
                .split(|line| !line.is_empty())
                .map(<[String]>::len)
                .filter(|n| *n > 0)
                .collect();
            assert_eq!(runs, vec![2, 3, 5, 4], "victory = {victory}");
        }
    }

    #[test]
    fn the_readkeys_consume_one_line_each() {
        for (run, want) in [(0usize, 1usize), (1, 3)] {
            let mut lines: std::vec::IntoIter<std::io::Result<String>> = (0..4)
                .map(|i| Ok(i.to_string()))
                .collect::<Vec<_>>()
                .into_iter();
            term::capture::lines(|| {
                if run == 0 {
                    end_screen(false, &mut lines);
                } else {
                    marquee(&mut lines);
                }
            });
            assert_eq!(4 - lines.count(), want, "run {run}");
        }
    }

    #[test]
    fn the_marquee_ends_with_the_end_screen() {
        let out = term::capture::lines(|| marquee(&mut no_lines()));
        let frames = out
            .iter()
            .filter(|l| l.contains("Т^") && l.contains("О^"))
            .count();
        assert_eq!(frames, usize::from(MARQUEE_PHASES));
        assert!(out.contains(&ANY_KEY.to_string()));
        let first_banner = out
            .iter()
            .position(|l| l.starts_with(BANNER_INDENT))
            .expect("the banner is drawn");
        let last_frame = out
            .iter()
            .rposition(|l| l.contains("Т^"))
            .expect("a frame is drawn");
        assert!(last_frame < first_banner, "the marquee runs first");
    }
}
