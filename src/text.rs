//! Rendering of the original's inline `^N` colour codes and `#` placeholders.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Black,
    Blue,
    Green,
    Cyan,
    Red,
    Magenta,
    Brown,
    White,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub color: Option<Color>,
    pub text: String,
}

pub fn from_code(c: char) -> Option<Color> {
    Some(match c {
        '0' => Color::Black,
        '1' => Color::Blue,
        '2' => Color::Green,
        '3' => Color::Cyan,
        '4' => Color::Red,
        '5' => Color::Magenta,
        '6' => Color::Brown,
        '7' => Color::White,
        _ => return None,
    })
}

impl Color {
    fn sgr(self) -> &'static str {
        match self {
            Color::Black => "\x1b[30m",
            Color::Blue => "\x1b[34m",
            Color::Green => "\x1b[32m",
            Color::Cyan => "\x1b[36m",
            Color::Red => "\x1b[31m",
            Color::Magenta => "\x1b[35m",
            Color::Brown => "\x1b[33m",
            Color::White => "\x1b[37m",
        }
    }
}

/// Split source text into styled spans. This is the only place that
/// understands the `^N` markup; `render` and `strip` are both built on it.
pub fn parse(src: &str) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut color: Option<Color> = None;
    let mut buf = String::new();
    let mut chars = src.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '^' {
            if let Some(&next) = chars.peek() {
                if let Some(new_color) = from_code(next) {
                    chars.next();
                    if !buf.is_empty() {
                        spans.push(Span { color, text: std::mem::take(&mut buf) });
                    }
                    color = Some(new_color);
                    continue;
                }
            }
        }
        buf.push(c);
    }
    if !buf.is_empty() {
        spans.push(Span { color, text: buf });
    }
    spans
}

pub fn render(src: &str) -> String {
    let spans = parse(src);
    let mut out = String::with_capacity(src.len());
    let mut styled = false;
    for span in &spans {
        if let Some(c) = span.color {
            out.push_str(c.sgr());
            styled = true;
        }
        out.push_str(&span.text);
    }
    if styled {
        out.push_str("\x1b[0m");
    }
    out
}

#[allow(dead_code)]
pub fn strip(src: &str) -> String {
    parse(src).into_iter().map(|s| s.text).collect()
}

#[allow(dead_code)]
pub fn fill(template: &str, values: &[i64]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut it = values.iter();
    for c in template.chars() {
        if c == '#' {
            match it.next() {
                Some(v) => out.push_str(&v.to_string()),
                None => out.push('#'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_map_to_colors() {
        assert_eq!(from_code('0'), Some(Color::Black));
        assert_eq!(from_code('4'), Some(Color::Red));
        assert_eq!(from_code('7'), Some(Color::White));
        assert_eq!(from_code('9'), None);
    }

    #[test]
    fn parse_splits_into_styled_spans() {
        let spans = parse("^4Ты сдох.");
        assert_eq!(
            spans,
            vec![Span { color: Some(Color::Red), text: "Ты сдох.".to_string() }]
        );
    }

    #[test]
    fn parse_handles_leading_plain_text_and_multiple_colors() {
        let spans = parse("Зрители:^6Мочи его!");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0], Span { color: None, text: "Зрители:".to_string() });
        assert_eq!(spans[1].color, Some(Color::Brown));
        assert_eq!(spans[1].text, "Мочи его!");
    }

    #[test]
    fn strip_removes_markup_entirely() {
        assert_eq!(strip("^4Gopnik: ^7version 1.02"), "Gopnik: version 1.02");
        assert_eq!(strip("^7 Mudila"), " Mudila");
        assert_eq!(strip("Не в этой жизни."), "Не в этой жизни.");
    }

    #[test]
    fn strip_output_contains_no_sigils() {
        for s in ["^0a^1b^2c^3d^4e^5f^6g^7h", "^1Крестик(Удача +2) "] {
            assert!(!strip(s).contains('^'), "sigil survived in {s:?}");
        }
    }

    #[test]
    fn caret_not_followed_by_digit_is_literal() {
        assert_eq!(strip("2^3"), "2");
        assert_eq!(strip("a^zb"), "a^zb");
        assert_eq!(strip("trailing^"), "trailing^");
    }

    #[test]
    fn render_emits_ansi_and_resets() {
        let out = render("^4Ты сдох.");
        assert!(out.starts_with("\x1b[31m"), "got {out:?}");
        assert!(out.contains("Ты сдох."));
        assert!(out.ends_with("\x1b[0m"));
    }

    #[test]
    fn render_passes_through_plain_text() {
        assert_eq!(render("Не в этой жизни."), "Не в этой жизни.");
    }

    #[test]
    fn fill_substitutes_in_order() {
        assert_eq!(fill("Урон #-#", &[3, 7]), "Урон 3-7");
        assert_eq!(fill("Здоровье #/#  ", &[118, 129]), "Здоровье 118/129  ");
    }

    #[test]
    fn fill_leaves_surplus_placeholders_literal() {
        assert_eq!(fill("Сл:# Лв:# Жв:#", &[1, 2]), "Сл:1 Лв:2 Жв:#");
    }

    #[test]
    fn fill_handles_no_placeholders() {
        assert_eq!(fill("Пива нет", &[]), "Пива нет");
    }
}
