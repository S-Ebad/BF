use super::*;
use crate::errors::Span;
use crate::lexer::tokenize;
use crate::resolver::resolve_jumps;
use TokenKind::*;

fn optimized(src: &str, level: u8) -> Vec<TokenKind> {
    let mut tokens = tokenize(src);
    resolve_jumps(&mut tokens).unwrap_or_else(|_| panic!("unbalanced: {src:?}"));

    optimize(tokens, level).iter().map(|t| *t.kind()).collect()
}

#[test]
fn o0_changes_nothing() {
    assert_eq!(
        optimized("++-->><<", 0),
        [
            Add(1),
            Add(1),
            Add(255),
            Add(255),
            Move(1),
            Move(1),
            Move(-1),
            Move(-1)
        ]
    );
}

#[test]
fn runs_are_merged() {
    assert_eq!(optimized("+++", 1), [Add(3)]);
    assert_eq!(optimized("---", 1), [Add(253)]);
    assert_eq!(optimized(">>>>", 1), [Move(4)]);
    assert_eq!(optimized("<<<", 1), [Move(-3)]);
    assert_eq!(optimized("+++--", 1), [Add(1)]);
    assert_eq!(optimized(">><<<", 1), [Move(-1)]);
}

#[test]
fn comments_do_not_split_runs() {
    assert_eq!(optimized("+ + comment +\n+", 1), [Add(4)]);
}

#[test]
fn adds_wrap() {
    assert_eq!(optimized(&"+".repeat(257), 1), [Add(1)]);
    assert_eq!(optimized(&"-".repeat(300), 1), [Add(212)]);
}

#[test]
fn cancelling_runs_are_dropped() {
    assert_eq!(optimized("+-", 1), []);
    assert_eq!(optimized("><", 1), []);
    assert_eq!(optimized(&"+".repeat(256), 1), []);
}

#[test]
fn dropping_a_run_merges_its_neighbours() {
    assert_eq!(optimized(">+-<", 1), []);
    assert_eq!(optimized("+>+-<+", 1), [Add(2)]);
    assert_eq!(optimized("+><+", 1), [Add(2)]);
}

#[test]
fn other_ops_split_runs() {
    assert_eq!(
        optimized("++.++,>>[>>]", 1),
        [
            Add(2),
            Output,
            Add(2),
            Input,
            Move(2),
            JmpZ(0),
            Move(2),
            JmpNZ(0)
        ]
    );
}

#[test]
fn merged_token_keeps_first_span() {
    let mut tokens = tokenize("ab+++cd>>");
    resolve_jumps(&mut tokens).ok().unwrap();
    let spans: Vec<Span> = optimize(tokens, 1).iter().map(Token::span).collect();

    assert_eq!(spans, [Span::new(2), Span::new(7)]);
}
