use super::*;
use crate::lexer::tokenize;
use crate::optimizer::optimize;
use crate::resolver::resolve_jumps;

fn tokens(src: &str, level: u8) -> Vec<Token> {
    let mut tokens = tokenize(src);
    resolve_jumps(&mut tokens).unwrap_or_else(|_| panic!("unbalanced: {src:?}"));
    optimize(tokens, level, false)
}

fn eval(src: &str, level: u8, step_limit: u64) -> Evaluation {
    evaluate(&tokens(src, level), step_limit, 30000)
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
    // Only the cells used so far: the rest of the tape is 0.
    assert_eq!(resume.tape, [3, 2]);
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
    let stopped = evaluate(&tokens, scan as u64 + 3, 30000).resume.unwrap();
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

#[test]
fn tape_size_is_respected() {
    // Cell 4 is the last one on a 5-cell tape; cell 5 is off it.
    let e = evaluate(&tokens(">>>>+.>+.", 0), DEFAULT_STEP_LIMIT, 5);
    assert_eq!(e.output, [1]);
    let resume = e.resume.unwrap();
    assert_eq!(resume.pointer, 5);

    // A huge tape costs nothing: only the used part exists.
    let e = evaluate(&tokens(">>+.", 0), DEFAULT_STEP_LIMIT, i32::MAX as usize);
    assert!(e.resume.is_none());
}

#[test]
fn scans_stop_before_stepping_off_the_tape() {
    // The scan would step from cell 0 to -1; it stops on cell 0 instead, so the
    // compiled program takes that step (and checks it).
    let tokens = tokens("+[<]", 2);
    let resume = evaluate(&tokens, DEFAULT_STEP_LIMIT, 30000).resume.unwrap();
    assert_eq!(*tokens[resume.index].kind(), TokenKind::Scan(-1));
    assert_eq!(resume.pointer, 0);
}
