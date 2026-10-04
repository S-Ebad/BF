//! `--tape-size` and `--bounds`: programs at the edges of the tape.
//!
//! Needs `nasm` and `ld` on the PATH.

mod common;

use common::*;

/// Compiles and runs `src` at every optimization level, on a tape of `size` cells,
/// and checks it ends like the reference interpreter says: the same output, and for
/// an abort, the error on stderr and exit code 1.
fn check(src: &str, input: &[u8], size: usize, bounds: Bounds) {
    check_with(src, input, size, bounds, &[]);
}

/// Like `check`, with extra arguments for the compiler.
fn check_with(src: &str, input: &[u8], size: usize, bounds: Bounds, args: &[&str]) {
    let expected = interpret_on(src, input, 10_000_000, size, bounds)
        .unwrap_or_else(|| panic!("reference run didn't finish: {src:?}"));
    let size_arg = size.to_string();

    for opt in OPT_LEVELS {
        let ws = Workspace::new();
        let mut all = vec!["--tape-size", &size_arg, "--bounds", bounds.flag()];
        all.extend_from_slice(args);
        let exe = build_with(&ws, src, opt, &all);
        let out = run(&exe, input);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let context = format!(
            "-O{opt} --tape-size {size} --bounds {} {}: {src:?}",
            bounds.flag(),
            args.join(" ")
        );

        match &expected {
            Outcome::Finished(stdout) => {
                assert!(
                    out.status.success(),
                    "{context}: exited with {}, {stderr}",
                    out.status
                );
                assert_eq!(&out.stdout, stdout, "{context}");
                assert!(stderr.is_empty(), "{context}: {stderr}");
            }
            Outcome::Aborted(stdout) => {
                assert_eq!(out.status.code(), Some(1), "{context}: {}", out.status);
                assert_eq!(&out.stdout, stdout, "{context}: output before the error");
                assert!(
                    stderr.contains(&format!("outside the tape (cells 0 to {})", size - 1)),
                    "{context}: {stderr}"
                );
            }
        }
    }
}

// --tape-size

#[test]
fn last_cell_of_any_size_is_usable() {
    for size in [1, 2, 3, 10, 4096, 30001] {
        let src = format!("{}+++.", ">".repeat(size - 1));
        for bounds in [Bounds::Undefined, Bounds::Abort, Bounds::Wrap] {
            check(&src, b"", size, bounds);
        }
    }
}

#[test]
fn one_past_the_last_cell_is_off_the_tape() {
    for size in [1, 2, 10, 4096] {
        check(
            &format!("+.{}+", ">".repeat(size)),
            b"",
            size,
            Bounds::Abort,
        );
    }
}

#[test]
fn huge_tape() {
    // A billion cells: only the pages the program touches are ever used.
    let size = 1_000_000_000;
    let src = "+>-<[>+<-]>.";
    for opt in OPT_LEVELS {
        let ws = Workspace::new();
        let exe = build_with(&ws, src, opt, &["--tape-size", &size.to_string()]);
        assert_eq!(run(&exe, b"").stdout, [0], "-O{opt}");
    }
}

#[test]
fn rejects_bad_tape_sizes() {
    let ws = Workspace::new();
    ws.write("f.bf", "+.");
    for size in ["0", "-1", "lots", "2147483648"] {
        let out = ws.brainfk(["f.bf", "--tape-size", size]);
        assert_eq!(out.status.code(), Some(2), "--tape-size {size}");
    }
}

// --bounds abort

#[test]
fn abort_left_of_the_tape() {
    check("+++.<+", b"", 30000, Bounds::Abort);
    check("+++.<.", b"", 30000, Bounds::Abort);
    check("+++.<,", b"x", 30000, Bounds::Abort);
    check("+[<]", b"", 30000, Bounds::Abort);
}

#[test]
fn abort_right_of_the_tape() {
    check("+[>+]", b"", 100, Bounds::Abort);
    check("+>+>+>+[>]", b"", 4, Bounds::Abort);
    check(".>>>>>+", b"", 5, Bounds::Abort);
}

#[test]
fn moving_off_the_tape_and_back_is_fine() {
    check("<<<>>>+.", b"", 10, Bounds::Abort);
    check("+[<<<<<>>>>>-].", b"", 10, Bounds::Abort);
    check(">>>>>>>>>>>><<<<<<<<<<<<+.", b"", 10, Bounds::Abort);
}

#[test]
fn abort_at_an_offset() {
    // -O2 runs these at an offset from the pointer.
    check(">+[<<+>>-]", b"", 30000, Bounds::Abort);
    check("+.>++[<.<<.>>>-]", b"", 30000, Bounds::Abort);
    check(">>>>+++[<<<<<<+>>>>>>-]", b"", 30000, Bounds::Abort);
}

#[test]
fn abort_is_the_same_at_every_level() {
    // The source as written touches cell -1, even where the optimizer removes the
    // access because it cancels out.
    for src in [
        "<+-",
        "<+->",
        "<-+",
        "+.<+->>+.",
        "+[<+->-]",
        "<[-]+[-]>",
        "<<+-+->>",
    ] {
        check(src, b"", 30000, Bounds::Abort);
    }
    // These only move off the tape without touching anything there.
    for src in ["<>++.", ">-+<.", "<<>>+-.", "<<<<<>>>>>[-]"] {
        check(src, b"", 30000, Bounds::Abort);
    }
}

#[test]
fn multiply_loop_that_does_not_run_does_not_abort() {
    // The target is left of the tape, but the counter is 0 (EOF), so the loop never
    // runs and nothing off the tape is touched.
    check(",[<+>-]+.", b"", 30000, Bounds::Abort);
    check(",[<<<+>>>>>>>+<<<<-]+.", b"", 30000, Bounds::Abort);
    // With a non-zero counter it does touch the target.
    check(",[<+>-]+.", b"\x02", 30000, Bounds::Abort);
}

#[test]
fn output_is_flushed_before_the_error() {
    let src = format!("{}{}<+", "+".repeat(65), ".".repeat(5000));
    check(&src, b"", 30000, Bounds::Abort);
}

// --bounds wrap

#[test]
fn wrap_both_ways() {
    check("<+++.>>>>>.", b"", 5, Bounds::Wrap);
    check("+++[>+++<-]>[<<+>>-]<<.", b"", 4, Bounds::Wrap);
    check("<<<<<<<<<<<<<+.>>>>>>>>>>>>>.", b"", 7, Bounds::Wrap);
}

#[test]
fn wrap_reads_the_last_cell_left_of_the_first() {
    // `<.` reads cell N-1.
    check("<.", b"", 5, Bounds::Wrap);
    check(">>>>+++<<<<<.", b"", 5, Bounds::Wrap);
    check("<+++>.<.", b"", 5, Bounds::Wrap);
}

#[test]
fn wrap_on_tiny_tapes() {
    // Every move lands on the same cell.
    check("+>+>+<+.", b"", 1, Bounds::Wrap);
    check("+>++>+++>.>.>.", b"", 3, Bounds::Wrap);
}

#[test]
fn wrap_large_moves() {
    let src = format!("+{}+{}.", ">".repeat(1000), "<".repeat(333));
    check(&src, b"", 7, Bounds::Wrap);
}

#[test]
fn wrap_at_offsets() {
    // Multiply and copy loops, and offset adds, across the edge.
    check("<+++[->>+<<]>>.", b"", 4, Bounds::Wrap);
    check("+++[<<+>>-]<<.", b"", 5, Bounds::Wrap);
    check(">+>++<<<<+++<.>>.>>.", b"", 5, Bounds::Wrap);
}

#[test]
fn wrap_scans_around_the_tape() {
    // Cells 0 to 6 are set; the scan wraps round to the one free cell.
    check("+>+>+>+>+>+>+>[>]+<<.", b"", 8, Bounds::Wrap);
    check("+>+>+>+>+<<<<<[<]+.", b"", 8, Bounds::Wrap);
}

// Fuzzing against the reference interpreter, on a small tape so programs hit the edges.

/// Random program that mostly terminates and often leaves a 16-cell tape.
fn gen_program(rng: &mut Rng, depth: u32, budget: usize) -> String {
    const OPS: &[u8] = b"+++---<<<>>>..,";
    const FRAGMENTS: &[&str] = &[
        "[-]",
        "[<]",
        "[>]",
        "[->+<]",
        "[-<+>]",
        "[->>+++<<]",
        "[-<<<+>>>]",
        ">[-]<",
        "<[-]>",
        ">.<",
        "<.>",
        "[>>]",
        "[<<]",
        "[>+<-]",
        // Accesses the optimizer removes, which must still count with `abort`.
        "<+->",
        ">-+<",
        "<<+-+->>",
        "<[-]+[-]>",
        "<>",
    ];

    let mut out = String::new();
    for _ in 0..budget {
        if rng.chance(0.12) {
            out += FRAGMENTS[rng.range(0, FRAGMENTS.len() - 1)];
        } else if depth < 3 && rng.chance(0.1) {
            let inner_budget = rng.range(1, 8);
            let inner = gen_program(rng, depth + 1, inner_budget);
            out += &format!("[{inner}-]");
        } else {
            out.push(OPS[rng.range(0, OPS.len() - 1)] as char);
        }
    }
    out
}

fn fuzz(bounds: Bounds, seed: u64) {
    const CASES: usize = 60;
    const SIZE: usize = 16;

    let mut rng = Rng::new(seed);
    let mut tested = 0;
    let mut aborted = 0;

    while tested < CASES {
        let budget = rng.range(5, 50);
        let src = gen_program(&mut rng, 0, budget);
        let len = rng.range(0, 10);
        let input = rng.bytes(len);

        // Skip programs that run too long.
        let Some(expected) = interpret_on(&src, &input, 100_000, SIZE, bounds) else {
            continue;
        };
        if matches!(expected, Outcome::Aborted(_)) {
            aborted += 1;
        }

        // A small step limit makes -O3 stop and resume at many different points.
        let step_limit = rng.range(0, 300).to_string();
        check_with(&src, &input, SIZE, bounds, &["--step-limit", &step_limit]);
        tested += 1;
    }

    // Make sure the fuzzer actually reaches the edges.
    if bounds == Bounds::Abort {
        assert!(
            aborted >= CASES / 5,
            "only {aborted} of {CASES} programs left the tape"
        );
    }
}

#[test]
fn fuzz_abort() {
    fuzz(Bounds::Abort, 0xAB0B7);
}

#[test]
fn fuzz_wrap() {
    fuzz(Bounds::Wrap, 0x3A9);
}
