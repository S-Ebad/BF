use super::compile;
use crate::errors::{
    BFError::{UnmatchedCloseBracket as Close, UnmatchedOpenBracket as Open},
    Span,
};

#[test]
fn valid_programs_compile() {
    for src in [
        "",
        "no commands here",
        "[]",
        "[[][[]]]",
        include_str!("../tests/bf/hello.bf"),
        include_str!("../tests/bf/squares.bf"),
    ] {
        assert!(compile(src).is_ok(), "{src:?}");
    }
}

#[test]
fn unmatched_open() {
    assert_eq!(compile("[").unwrap_err(), [Open(Span::new(0))]);
    assert_eq!(compile("+[[]").unwrap_err(), [Open(Span::new(1))]);
}

#[test]
fn unmatched_close() {
    assert_eq!(compile("]").unwrap_err(), [Close(Span::new(0))]);
    assert_eq!(compile("[]]").unwrap_err(), [Close(Span::new(2))]);
}

#[test]
fn reports_every_error_in_source_order() {
    assert_eq!(
        compile("][ ]]").unwrap_err(),
        [Close(Span::new(0)), Close(Span::new(4))]
    );
    assert_eq!(
        compile("abc\n\nx]y\n[[z").unwrap_err(),
        [Close(Span::new(6)), Open(Span::new(9)), Open(Span::new(10))]
    );
}

#[test]
fn spans_are_byte_offsets() {
    assert_eq!(compile("ü]").unwrap_err(), [Close(Span::new(2))]);
    assert_eq!(compile("🚀[").unwrap_err(), [Open(Span::new(4))]);
}
