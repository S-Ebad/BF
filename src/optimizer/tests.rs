use super::*;
use crate::errors::Span;
use crate::lexer::tokenize;
use crate::resolver::resolve_jumps;
use TokenKind::*;

fn optimized(src: &str, level: u8) -> Vec<TokenKind> {
    let mut tokens = tokenize(src);
    resolve_jumps(&mut tokens).unwrap_or_else(|_| panic!("unbalanced: {src:?}"));

    optimize(tokens, level, false)
        .iter()
        .map(|t| *t.kind())
        .collect()
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
            Input(0),
            Move(2),
            JmpZ(0, 0),
            Move(2),
            JmpNZ(0, 0)
        ]
    );
}

#[test]
fn merged_token_keeps_first_span() {
    let mut tokens = tokenize("ab+++cd>>");
    resolve_jumps(&mut tokens).ok().unwrap();
    let spans: Vec<Span> = optimize(tokens, 1, false).iter().map(Token::span).collect();

    assert_eq!(spans, [Span::new(2), Span::new(7)]);
}

// -O2. Loops start after `,` so the cell isn't known to be 0 and the loop isn't dead.

#[test]
fn clear_loops_become_set() {
    assert_eq!(optimized(",[-]", 2), [Input(0), Set(0, 0)]);
    assert_eq!(optimized(",[+]", 2), [Input(0), Set(0, 0)]);
    assert_eq!(optimized(",[---]", 2), [Input(0), Set(0, 0)]);
    assert_eq!(optimized(",[- comment ]", 2), [Input(0), Set(0, 0)]);
    assert_eq!(optimized(",[[-]]", 2), [Input(0), Set(0, 0)]);
}

#[test]
fn even_clear_loops_stay_loops() {
    // `[--]` never reaches 0 from an odd value, so it can't become Set(0, 0).
    assert_eq!(
        optimized(",[--]", 2),
        [Input(0), JmpZ(0, 0), Add(254), JmpNZ(0, 0)]
    );
    assert_eq!(optimized(",[]", 2), [Input(0), JmpZ(0, 0), JmpNZ(0, 0)]);
}

#[test]
fn loop_patterns_only_at_o2() {
    assert_eq!(
        optimized(",[-]", 1),
        [Input(0), JmpZ(0, 0), Add(255), JmpNZ(0, 0)]
    );
}

#[test]
fn set_absorbs_neighbouring_adds() {
    assert_eq!(optimized(",[-]+++", 2), [Input(0), Set(0, 3)]);
    assert_eq!(optimized(",[-]---", 2), [Input(0), Set(0, 253)]);
    assert_eq!(optimized(",+++[-]", 2), [Input(0), Set(0, 0)]);
}

#[test]
fn sets_get_offsets() {
    assert_eq!(optimized(",>[-]<", 2), [Input(0), Set(1, 0)]);
    assert_eq!(
        optimized(",>[-]>[-]<<", 2),
        [Input(0), Set(1, 0), Set(2, 0)]
    );
    assert_eq!(optimized(",>[-]++<", 2), [Input(0), Set(1, 2)]);
    assert_eq!(optimized(",>++[-]<", 2), [Input(0), Set(1, 0)]);
}

#[test]
fn set_in_outer_loop() {
    // The inner clear becomes Set(1, 0) once the moves around it are folded.
    assert_eq!(
        optimized(",[>[-]<-]", 2),
        [Input(0), JmpZ(0, 0), Set(1, 0), Add(255), JmpNZ(0, 0)]
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
fn input_gets_offsets() {
    assert_eq!(optimized(">,<+", 2), [Input(1), Add(1)]);
}

#[test]
fn offsets_stop_at_loops_that_move() {
    // A scan, and a loop whose body moves the pointer overall.
    assert_eq!(
        optimized(">+[>]<+", 2),
        [AddAt(1, 1), Move(1), Scan(1), AddAt(-1, 1), Move(-1)]
    );
    assert_eq!(
        optimized(",>+[>+<<]<+", 2),
        [
            Input(0),
            AddAt(1, 1),
            Move(1),
            JmpZ(0, 0),
            AddAt(1, 1),
            Move(-1),
            JmpNZ(0, 0),
            AddAt(-1, 1),
            Move(-1)
        ]
    );
}

#[test]
fn balanced_loops_run_at_an_offset() {
    // The loop's body leaves the pointer where it was, so it runs at offset 1.
    assert_eq!(
        optimized(",>[<.>-]<", 2),
        [Input(0), JmpZ(0, 1), Output(0), AddAt(1, 255), JmpNZ(0, 1)]
    );
    // The inner `[.<]` moves the pointer, so neither loop can run at an offset.
    assert_eq!(
        optimized(",>>[<[.<]>]<<", 2),
        [
            Input(0),
            Move(2),
            JmpZ(0, 0),
            Move(-1),
            JmpZ(1, 0),
            Output(0),
            Move(-1),
            JmpNZ(1, 0),
            Move(1),
            JmpNZ(0, 0),
            Move(-2)
        ]
    );
    // Nested loops that don't move both run at offsets.
    assert_eq!(
        optimized(",>>[<[.-]>-]<<", 2),
        [
            Input(0),
            JmpZ(0, 2),
            JmpZ(1, 1),
            Output(1),
            AddAt(1, 255),
            JmpNZ(1, 1),
            AddAt(2, 255),
            JmpNZ(0, 2)
        ]
    );
}

#[test]
fn multiply_loops_at_an_offset() {
    // `>[-<+>]<` runs at offset 1: counter at 1, target at 0.
    assert_eq!(
        optimized(">+[<+>-]", 2),
        [AddAt(1, 1), MulAt(1, 0, 1), Set(1, 0), Move(1)]
    );
    // Mandelbrot's hot loop: a copy loop one cell over, inside a loop that walks
    // 9 cells at a time. The copy needs no pointer moves.
    assert_eq!(
        optimized(",[>[-<<<<<<<<<+>>>>>>>>>]<<<<<<<<<<]", 2),
        [
            Input(0),
            JmpZ(0, 0),
            MulAt(1, -8, 1),
            Set(1, 0),
            Move(-9),
            JmpNZ(0, 0)
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
        [Input(0), MulAt(0, 1, 1), Set(0, 0)]
    );
    assert_eq!(
        optimized(",[>+<-]", 2),
        [Input(0), MulAt(0, 1, 1), Set(0, 0)]
    );
    assert_eq!(
        optimized(",[-<<->>]", 2),
        [Input(0), MulAt(0, -2, 255), Set(0, 0)]
    );
}

#[test]
fn multiply_loops_become_mul() {
    assert_eq!(
        optimized(",[->+++>++<<]", 2),
        [Input(0), MulAt(0, 1, 3), MulAt(0, 2, 2), Set(0, 0)]
    );
    // Targets are combined per offset, and ones that cancel out are dropped.
    assert_eq!(
        optimized(",[->+>+<<>+>-<<]", 2),
        [Input(0), MulAt(0, 1, 2), Set(0, 0)]
    );
}

#[test]
fn multiply_loops_have_no_guard() {
    // Codegen pads the tape, so touching the targets with a 0 counter is safe.
    assert_eq!(
        optimized(",[-<+>]", 2),
        [Input(0), MulAt(0, -1, 1), Set(0, 0)]
    );
    // A multiply with no targets left is a plain clear.
    assert_eq!(optimized(",[->+>-<<>>+<-<]", 2), [Input(0), Set(0, 0)]);
}

#[test]
fn counting_up_multiplies_by_the_negation() {
    // `[+>+<]` runs 256 - v times, which adds -v (mod 256) to the target.
    assert_eq!(
        optimized(",[+>+<]", 2),
        [Input(0), MulAt(0, 1, 255), Set(0, 0)]
    );
    // Step -3 runs v * 3⁻¹ times, and 3⁻¹ is 171 (mod 256).
    assert_eq!(
        optimized(",[--->+<]", 2),
        [Input(0), MulAt(0, 1, 171), Set(0, 0)]
    );
}

#[test]
fn loops_that_are_not_multiplies_stay() {
    // Moves the pointer.
    assert_eq!(
        optimized(",[->+]", 2),
        [
            Input(0),
            JmpZ(0, 0),
            Add(255),
            AddAt(1, 1),
            Move(1),
            JmpNZ(0, 0)
        ]
    );
    // Even counter step.
    assert!(optimized(",[-->+<]", 2).contains(&JmpZ(0, 0)));
    // I/O in the body.
    assert!(optimized(",[->.<]", 2).contains(&JmpZ(0, 0)));
    // A nested loop that isn't replaced.
    assert!(optimized(",[->[-->+<]<]", 2).contains(&JmpZ(0, 0)));
}

#[test]
fn scan_loops() {
    assert_eq!(optimized(",[>]", 2), [Input(0), Scan(1)]);
    assert_eq!(optimized(",[<<]", 2), [Input(0), Scan(-2)]);
    assert_eq!(optimized(",[>>>>>>>>>]", 2), [Input(0), Scan(9)]);
}

#[test]
fn dead_loops_are_removed() {
    // At the start, and after loops, scans and clears, the cell is 0.
    assert_eq!(optimized("[comment, with. commands+]+", 2), [Add(1)]);
    assert_eq!(
        optimized(",[>]<[+.]", 2),
        [
            Input(0),
            Scan(1),
            JmpZ(1, -1),
            AddAt(-1, 1),
            Output(-1),
            JmpNZ(1, -1),
            Move(-1)
        ]
    );
    // `+[-]` merges into Set(0, 0), which is redundant after the scan.
    assert_eq!(optimized(",[>]+[-][.]", 2), [Input(0), Scan(1)]);
    assert_eq!(
        optimized(",[.,][.]", 2),
        [Input(0), JmpZ(0, 0), Output(0), Input(0), JmpNZ(0, 0)]
    );
    assert_eq!(optimized(",[-][.]", 2), [Input(0), Set(0, 0)]);
}

#[test]
fn loops_at_an_offset_say_nothing_about_the_current_cell() {
    // The loop at offset 1 ends with cell 1 at 0, not the current cell, so the
    // `[.-]` after it must stay.
    assert!(optimized("+>+[<.>-]<[.-]", 2).contains(&JmpZ(1, 0)));
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
        [Input(0), JmpZ(0, 0), Output(0), Input(0), JmpNZ(0, 0)]
    );
}

#[test]
fn live_loops_are_kept() {
    assert_eq!(
        optimized("+[.-]", 2),
        [Add(1), JmpZ(0, 0), Output(0), Add(255), JmpNZ(0, 0)]
    );
    assert_eq!(
        optimized("<[.]", 2),
        [JmpZ(0, -1), Output(-1), JmpNZ(0, -1), Move(-1)]
    );
}

#[test]
fn multiply_loops_can_keep_their_guard() {
    let mut tokens = tokenize(",[->+<]");
    resolve_jumps(&mut tokens).ok().unwrap();
    let kinds: Vec<TokenKind> = optimize(tokens, 2, true)
        .iter()
        .map(|t| *t.kind())
        .collect();
    assert_eq!(
        kinds,
        [Input(0), JmpZ(0, 0), MulAt(0, 1, 1), Set(0, 0), JmpNZ(0, 0)]
    );
}

/// Tokens with `--bounds abort` checks, before and after optimizing at `level`.
fn checked(src: &str, level: u8) -> Vec<TokenKind> {
    let mut tokens = tokenize(src);
    resolve_jumps(&mut tokens).unwrap_or_else(|_| panic!("unbalanced: {src:?}"));
    let tokens = insert_access_checks(tokens);
    optimize(tokens, level, true)
        .iter()
        .map(|t| *t.kind())
        .collect()
}

#[test]
fn checks_cover_the_cells_a_stretch_touches() {
    // Cancelling accesses still count: the source as written touches cell -1.
    assert_eq!(
        checked("<+->", 0),
        [Check(-1, -1), Move(-1), Add(1), Add(255), Move(1)]
    );
    // Moving off the tape without touching anything there isn't checked.
    assert_eq!(
        checked("<>++", 0),
        [Check(0, 0), Move(-1), Move(1), Add(1), Add(1)]
    );
    // I/O and brackets end a stretch and touch their cell; moves after them only
    // start a new stretch that touches nothing.
    assert_eq!(
        checked(">.<", 0),
        [Check(1, 1), Move(1), Output(0), Move(-1)]
    );
    assert_eq!(
        checked("[>+<-]", 0),
        [
            Check(0, 0),
            JmpZ(0, 0),
            Check(0, 1),
            Move(1),
            Add(1),
            Move(-1),
            Add(255),
            JmpNZ(0, 0)
        ]
    );
}

#[test]
fn checks_survive_optimizing() {
    // `+-` cancels out, but the check for cell -1 stays.
    assert_eq!(checked("<+-", 2), [Check(-1, -1), Move(-1)]);
    assert_eq!(checked("<+->", 2), [Check(-1, -1)]);
    // Folded moves shift the check with them.
    assert_eq!(checked(">>+<<", 2), [Check(2, 2), AddAt(2, 1)]);
}

#[test]
fn checks_stay_inside_multiply_guards() {
    // Only a loop that runs touches its targets, so only then are they checked.
    assert_eq!(
        checked(",[->+<]", 2),
        [
            Check(0, 0),
            Input(0),
            Check(0, 0),
            JmpZ(0, 0),
            Check(0, 1),
            MulAt(0, 1, 1),
            Set(0, 0),
            JmpNZ(0, 0)
        ]
    );
    // A clear loop only touches its own cell, which its `[` already checked.
    assert_eq!(
        checked(",[-]", 2),
        [Check(0, 0), Input(0), Check(0, 0), Set(0, 0)]
    );
}

#[test]
fn neighbouring_checks_merge() {
    // The dead loop at the start goes, which leaves the check for its `[` right next
    // to the check of the `<+` after it, at the same pointer.
    assert_eq!(checked("[.]<+", 2), [Check(-1, 0), AddAt(-1, 1), Move(-1)]);
}
