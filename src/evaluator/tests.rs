use super::*;
use crate::lexer::tokenize;
use crate::optimizer::optimize;
use crate::resolver::resolve_jumps;

fn tokens(src: &str, level: u8) -> Vec<Token> {
    let mut tokens = tokenize(src);
    resolve_jumps(&mut tokens).unwrap_or_else(|_| panic!("unbalanced: {src:?}"));
    optimize(tokens, level)
}

fn eval(src: &str, level: u8, step_limit: u64) -> Evaluation {
    evaluate(&tokens(src, level), step_limit)
}

#[test]
fn finished_programs_leave_only_output() {
    for level in [0, 1, 2] {
        let e = eval(
            include_str!("../../tests/bf/hello.bf"),
            level,
            DEFAULT_STEP_LIMIT,
        );
        assert_eq!(e.output, b"Hello World!\n", "-O{level}");
        assert!(e.resume.is_none(), "-O{level}");
    }
}

#[test]
fn empty_program_finishes() {
    let e = eval("", 2, DEFAULT_STEP_LIMIT);
    assert!(e.output.is_empty());
    assert!(e.resume.is_none());
}

#[test]
fn stops_at_input() {
    let src = "+++.>++,+.";
    let e = eval(src, 0, DEFAULT_STEP_LIMIT);
    assert_eq!(e.output, [3]);

    let resume = e.resume.unwrap();
    assert_eq!(*tokens(src, 0)[resume.index].kind(), TokenKind::Input(0));
    assert_eq!(resume.pointer, 1);
    assert_eq!(resume.tape[..3], [3, 2, 0]);
}

#[test]
fn stops_at_step_limit() {
    let e = eval("+.[]", 0, 1000);
    assert_eq!(e.output, [1]);
    assert!(e.resume.is_some());
}

#[test]
fn step_limit_counts_tokens() {
    // Three tokens: the third one is where it stops.
    let resume = eval("+++", 0, 2).resume.unwrap();
    assert_eq!(resume.index, 2);
    assert_eq!(resume.tape[0], 2);
}

#[test]
fn can_stop_inside_a_loop() {
    // 4 `+`, then one pass through the loop is 6 steps; `]` jumps back to the body.
    let resume = eval("++++[>+<-]", 0, 10).resume.unwrap();
    assert_eq!(resume.index, 5);
    assert_eq!(resume.pointer, 0);
    assert_eq!(resume.tape[..2], [3, 1]);
}

#[test]
fn stops_before_leaving_the_tape() {
    // `<+` touches the cell left of the tape: undefined, so left to the program.
    let e = eval("+.<+", 0, DEFAULT_STEP_LIMIT);
    assert_eq!(e.output, [1]);
    let resume = e.resume.unwrap();
    assert_eq!(resume.index, 3);
    assert_eq!(resume.pointer, -1);

    // Same with an offset op at -O2.
    let resume = eval(",<+>", 2, DEFAULT_STEP_LIMIT).resume.unwrap();
    assert_eq!(resume.index, 0);
}

#[test]
fn scans_stop_partway() {
    // Cells 1..=5 are set; the scan from cell 1 has 5 cells to cross.
    let src = ">+>+>+>+>+<<<<[>]";
    let full = eval(src, 2, DEFAULT_STEP_LIMIT);
    assert!(full.resume.is_none());

    let tokens = tokens(src, 2);
    let scan = tokens.len() - 1;
    let stopped = evaluate(&tokens, scan as u64 + 3).resume.unwrap();
    assert_eq!(stopped.index, scan);
    assert_eq!(stopped.pointer, 3);
}

#[test]
fn optimized_ops_match_their_loops() {
    // Multiply, clear, scan and offset ops, checked against -O0 running the loops.
    let src = "+++++[->++>+++<<]>>[-<+>]<.>>+<<[>]<<.";
    let plain = eval(src, 0, DEFAULT_STEP_LIMIT);
    let optimized = eval(src, 2, DEFAULT_STEP_LIMIT);
    assert_eq!(plain.output, optimized.output);
    assert!(optimized.resume.is_none());
}
