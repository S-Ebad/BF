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
            Output(0),
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

// -O2. Loops start after `,` so the cell isn't known to be 0 and the loop isn't dead.

#[test]
fn clear_loops_become_set() {
    assert_eq!(optimized(",[-]", 2), [Input, Set(0, 0)]);
    assert_eq!(optimized(",[+]", 2), [Input, Set(0, 0)]);
    assert_eq!(optimized(",[---]", 2), [Input, Set(0, 0)]);
    assert_eq!(optimized(",[- comment ]", 2), [Input, Set(0, 0)]);
    assert_eq!(optimized(",[[-]]", 2), [Input, Set(0, 0)]);
}

#[test]
fn even_clear_loops_stay_loops() {
    // `[--]` never reaches 0 from an odd value, so it can't become Set(0, 0).
    assert_eq!(optimized(",[--]", 2), [Input, JmpZ(0), Add(254), JmpNZ(0)]);
    assert_eq!(optimized(",[]", 2), [Input, JmpZ(0), JmpNZ(0)]);
}

#[test]
fn loop_patterns_only_at_o2() {
    assert_eq!(optimized(",[-]", 1), [Input, JmpZ(0), Add(255), JmpNZ(0)]);
}

#[test]
fn set_absorbs_neighbouring_adds() {
    assert_eq!(optimized(",[-]+++", 2), [Input, Set(0, 3)]);
    assert_eq!(optimized(",[-]---", 2), [Input, Set(0, 253)]);
    assert_eq!(optimized(",+++[-]", 2), [Input, Set(0, 0)]);
}

#[test]
fn sets_get_offsets() {
    assert_eq!(optimized(",>[-]<", 2), [Input, Set(1, 0)]);
    assert_eq!(optimized(",>[-]>[-]<<", 2), [Input, Set(1, 0), Set(2, 0)]);
    assert_eq!(optimized(",>[-]++<", 2), [Input, Set(1, 2)]);
    assert_eq!(optimized(",>++[-]<", 2), [Input, Set(1, 0)]);
}

#[test]
fn set_in_outer_loop() {
    // The inner clear becomes Set(1, 0) once the moves around it are folded.
    assert_eq!(
        optimized(",[>[-]<-]", 2),
        [Input, JmpZ(0), Set(1, 0), Add(255), JmpNZ(0)]
    );
}

#[test]
fn moves_around_adds_become_offsets() {
    assert_eq!(optimized(">>++<<", 2), [AddAt(2, 2)]);
    assert_eq!(optimized("<<-->>", 2), [AddAt(-2, 254)]);
    assert_eq!(optimized(">+>+<<", 2), [AddAt(1, 1), AddAt(2, 1)]);
    assert_eq!(optimized(">+<+", 2), [AddAt(1, 1), Add(1)]);
}

#[test]
fn net_move_is_kept() {
    assert_eq!(optimized(">>+>", 2), [AddAt(2, 1), Move(3)]);
    assert_eq!(optimized(">>+", 2), [AddAt(2, 1), Move(2)]);
}

#[test]
fn outputs_get_offsets() {
    assert_eq!(optimized(">+.<+", 2), [AddAt(1, 1), Output(1), Add(1)]);
    assert_eq!(optimized(">>.<.<", 2), [Output(2), Output(1)]);
}

#[test]
fn offsets_stop_at_ops_that_need_the_pointer() {
    assert_eq!(
        optimized(">,<+", 2),
        [Move(1), Input, AddAt(-1, 1), Move(-1)]
    );
    assert_eq!(
        optimized(">+[<+>-]", 2),
        [
            AddAt(1, 1),
            Move(1),
            JmpZ(0),
            MulAt(-1, 1),
            Set(0, 0),
            JmpNZ(0)
        ]
    );
}

#[test]
fn offset_adds_merge_and_cancel() {
    assert_eq!(optimized(">+<>+<", 2), [AddAt(1, 2)]);
    assert_eq!(optimized(">+<>-<", 2), []);
}

#[test]
fn copy_loops_become_mul() {
    assert_eq!(
        optimized(",[->+<]", 2),
        [Input, JmpZ(0), MulAt(1, 1), Set(0, 0), JmpNZ(0)]
    );
    assert_eq!(
        optimized(",[>+<-]", 2),
        [Input, JmpZ(0), MulAt(1, 1), Set(0, 0), JmpNZ(0)]
    );
    assert_eq!(
        optimized(",[-<<->>]", 2),
        [Input, JmpZ(0), MulAt(-2, 255), Set(0, 0), JmpNZ(0)]
    );
}

#[test]
fn multiply_loops_become_mul() {
    assert_eq!(
        optimized(",[->+++>++<<]", 2),
        [
            Input,
            JmpZ(0),
            MulAt(1, 3),
            MulAt(2, 2),
            Set(0, 0),
            JmpNZ(0)
        ]
    );
    // Targets are combined per offset, and ones that cancel out are dropped.
    assert_eq!(
        optimized(",[->+>+<<>+>-<<]", 2),
        [Input, JmpZ(0), MulAt(1, 2), Set(0, 0), JmpNZ(0)]
    );
}

#[test]
fn multiply_loops_keep_their_guard() {
    // The targets must only be touched when the loop would have run.
    assert_eq!(
        optimized(",[-<+>]", 2),
        [Input, JmpZ(0), MulAt(-1, 1), Set(0, 0), JmpNZ(0)]
    );
    // A multiply with no targets left is a plain clear, which needs no guard.
    assert_eq!(optimized(",[->+>-<<>>+<-<]", 2), [Input, Set(0, 0)]);
}

#[test]
fn counting_up_multiplies_by_the_negation() {
    // `[+>+<]` runs 256 - v times, which adds -v (mod 256) to the target.
    assert_eq!(
        optimized(",[+>+<]", 2),
        [Input, JmpZ(0), MulAt(1, 255), Set(0, 0), JmpNZ(0)]
    );
    // Step -3 runs v * 3⁻¹ times, and 3⁻¹ is 171 (mod 256).
    assert_eq!(
        optimized(",[--->+<]", 2),
        [Input, JmpZ(0), MulAt(1, 171), Set(0, 0), JmpNZ(0)]
    );
}

#[test]
fn loops_that_are_not_multiplies_stay() {
    // Moves the pointer.
    assert_eq!(
        optimized(",[->+]", 2),
        [Input, JmpZ(0), Add(255), AddAt(1, 1), Move(1), JmpNZ(0)]
    );
    // Even counter step.
    assert!(optimized(",[-->+<]", 2).contains(&JmpZ(0)));
    // I/O in the body.
    assert!(optimized(",[->.<]", 2).contains(&JmpZ(0)));
    // A nested loop that isn't replaced.
    assert!(optimized(",[->[-->+<]<]", 2).contains(&JmpZ(0)));
}

#[test]
fn scan_loops() {
    assert_eq!(optimized(",[>]", 2), [Input, Scan(1)]);
    assert_eq!(optimized(",[<<]", 2), [Input, Scan(-2)]);
    assert_eq!(optimized(",[>>>>>>>>>]", 2), [Input, Scan(9)]);
}

#[test]
fn dead_loops_are_removed() {
    // At the start, and after loops, scans and clears, the cell is 0.
    assert_eq!(optimized("[comment, with. commands+]+", 2), [Add(1)]);
    assert_eq!(
        optimized(",[>]<[+.]", 2),
        [
            Input,
            Scan(1),
            Move(-1),
            JmpZ(1),
            Add(1),
            Output(0),
            JmpNZ(1)
        ]
    );
    // `+[-]` merges into Set(0, 0), which is redundant after the scan.
    assert_eq!(optimized(",[>]+[-][.]", 2), [Input, Scan(1)]);
    assert_eq!(
        optimized(",[.,][.]", 2),
        [Input, JmpZ(0), Output(0), Input, JmpNZ(0)]
    );
    assert_eq!(optimized(",[-][.]", 2), [Input, Set(0, 0)]);
}

#[test]
fn ops_elsewhere_keep_the_cell_known_zero() {
    assert_eq!(optimized(">+<.[.]", 2), [AddAt(1, 1), Output(0)]);
}

#[test]
fn redundant_clears_are_removed() {
    // The clear and the add merge into one Set.
    assert_eq!(optimized("[-]+", 2), [Set(0, 1)]);
    assert_eq!(
        optimized(",[.,][-]", 2),
        [Input, JmpZ(0), Output(0), Input, JmpNZ(0)]
    );
}

#[test]
fn live_loops_are_kept() {
    assert_eq!(
        optimized("+[.-]", 2),
        [Add(1), JmpZ(0), Output(0), Add(255), JmpNZ(0)]
    );
    assert_eq!(
        optimized("<[.]", 2),
        [Move(-1), JmpZ(0), Output(0), JmpNZ(0)]
    );
}
