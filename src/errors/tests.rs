use super::*;

/// Snippet for the first bracket in `src`.
fn snippet_at_bracket(src: &str) -> Snippet {
    snippet(src, Span::new(src.find(['[', ']']).unwrap()))
}

/// The visible char at display column `col` of `text`.
fn char_at_column(text: &str, col: usize) -> Option<char> {
    let mut at = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if w > 0 && at == col {
            return Some(c);
        }
        at += w;
    }
    None
}

fn text_width(text: &str) -> usize {
    text.chars().map(display_width).sum()
}

#[test]
fn line_and_column() {
    for (src, line, column) in [
        ("[", 1, 1),
        ("+]", 1, 2),
        ("abc\n\nx]y", 3, 2),
        ("\r\n]", 2, 1),
        ("ünï日本🚀]", 1, 7),
        ("\t\t]", 1, 3),
        (&format!("{}]", "\n".repeat(12)), 13, 1),
    ] {
        let s = snippet_at_bracket(src);
        assert_eq!((s.line, s.column), (line, column), "{src:?}");
    }
}

#[test]
fn caret_points_at_bracket() {
    for src in [
        "[",
        "a\tb\t]",
        "\t\t]",
        "ünï日本🚀]",
        "x\u{301}]",
        &format!("{}]{}", "a".repeat(100), "b".repeat(100)),
        &format!("{}[{}", "a".repeat(10), "b".repeat(200)),
        &format!("{}[", "b".repeat(200)),
        &format!("{}]{}", "日".repeat(100), "日".repeat(100)),
        &format!("{}]x", "\t".repeat(80)),
        &format!("{}]{}", "\t日".repeat(50), "x\t".repeat(50)),
    ] {
        let s = snippet_at_bracket(src);
        let bracket = src.chars().find(|&c| c == '[' || c == ']');
        assert_eq!(
            char_at_column(&s.text, s.caret),
            bracket,
            "{src:?} -> {s:?}"
        );
    }
}

#[test]
fn tabs_are_expanded() {
    let s = snippet_at_bracket("a\tb\t]");
    assert!(!s.text.contains('\t'));
    assert_eq!(text_width(&s.text), 3 + 2 * TAB_WIDTH);
}

#[test]
fn short_lines_are_not_cut() {
    for src in ["[", &format!("{}[", "b".repeat(MAX_WIDTH - 1))] {
        let s = snippet_at_bracket(src);
        assert!(!s.cut_start && !s.cut_end, "{src:?}");
    }
}

#[test]
fn long_lines_fit_max_width() {
    for (src, cut_start, cut_end) in [
        (format!("{}[", "b".repeat(MAX_WIDTH)), true, false),
        (
            format!("{}[{}", "a".repeat(10), "b".repeat(200)),
            false,
            true,
        ),
        (format!("{}[", "b".repeat(200)), true, false),
        (
            format!("{}]{}", "a".repeat(100), "b".repeat(100)),
            true,
            true,
        ),
        (
            format!("{}]{}", "日".repeat(100), "日".repeat(100)),
            true,
            true,
        ),
        (format!("{}]x", "\t".repeat(80)), true, false),
    ] {
        let s = snippet_at_bracket(&src);
        assert!(text_width(&s.text) <= MAX_WIDTH, "{src:?} -> {s:?}");
        assert_eq!((s.cut_start, s.cut_end), (cut_start, cut_end), "{src:?}");
    }
}

#[test]
fn cut_window_is_centred_on_error() {
    let s = snippet_at_bracket(&format!("{}]{}", "a".repeat(100), "b".repeat(100)));
    assert_eq!(s.caret, MAX_WIDTH / 2);
}
