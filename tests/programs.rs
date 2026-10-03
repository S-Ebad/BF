//! Compiles and runs Brainfuck programs, checking their output.
//!
//! Needs `nasm` and `ld` on the PATH.

mod common;

use common::*;
use std::io::Read;
use std::sync::mpsc;
use std::time::Duration;

const HELLO: &str = include_str!("bf/hello.bf");
const HELLO_WRAPPING: &str = include_str!("bf/hello_wrapping.bf");
const SQUARES: &str = include_str!("bf/squares.bf");
const CAT: &str = include_str!("bf/cat.bf");
const REVERSE: &str = include_str!("bf/reverse.bf");

// Known programs

#[test]
fn hello_world() {
    assert_output(HELLO, b"", b"Hello World!\n");
}

#[test]
fn hello_world_wrapping() {
    assert_output(HELLO_WRAPPING, b"", b"Hello World!\n");
}

#[test]
fn squares() {
    let expected: Vec<u8> = (0..=100)
        .flat_map(|i| format!("{}\n", i * i).into_bytes())
        .collect();
    assert_output(SQUARES, b"", &expected);
}

// Cells and tape

#[test]
fn empty_program() {
    assert_output("", b"", b"");
}

#[test]
fn comments_are_ignored() {
    assert_output("only comments: ünïcødé 日本語 🚀\n", b"", b"");
    assert_output("a ü+ b 日+ c 🚀+ d.", b"", b"\x03");
}

#[test]
fn cells_wrap() {
    assert_output("-.", b"", b"\xff");
    assert_output(&"+".repeat(256), b"", b"");
    assert_output(&format!("{}.", "+".repeat(256)), b"", b"\x00");
    assert_output(&format!("{}.", "+".repeat(257)), b"", b"\x01");
}

#[test]
fn last_cell_is_usable() {
    assert_output(&format!("{}+++.", ">".repeat(TAPE_LEN - 1)), b"", b"\x03");
}

#[test]
fn moving_back_and_forth() {
    let src = format!(
        "{}+{}{}",
        ">".repeat(100),
        "<".repeat(100),
        ".>".repeat(101)
    );
    let mut expected = vec![0; 100];
    expected.push(1);
    assert_output(&src, b"", &expected);
}

// Loops

#[test]
fn loop_on_zero_is_skipped() {
    assert_output("[.+++]+.", b"", b"\x01");
    assert_output("[[[.]]]+.", b"", b"\x01");
}

#[test]
fn clear_loop() {
    assert_output("+++++[-].", b"", b"\x00");
    assert_output("+++++[+].", b"", b"\x00");
}

#[test]
fn multiply_loop_that_does_not_run_touches_nothing() {
    // The loop's target is 5000 cells left of the tape, but the counter is 0 (EOF),
    // so the loop never runs. -O2 must not touch the target anyway.
    let src = format!(",[{}+{}-]+.", "<".repeat(5000), ">".repeat(5000));
    assert_output(&src, b"", b"\x01");
}

#[test]
fn loops_at_an_offset() {
    // At -O2 the `[<.>-]` runs at offset 1 without moving the pointer, and the
    // current cell is still 1 after it, so `[.-]` runs.
    assert_output("+>+[<.>-]<[.-]", b"", b"\x01\x01");
    // A copy loop one cell over, inside a loop that walks left along the tape.
    assert_output(">+>+>+[>[-<+>]<<]>.>.>.", b"", b"\x03\x00\x00");
}

#[test]
fn check_after_set_reads_the_new_value() {
    // After the first loop the zero flag says the cell is 0. `[-]+++` becomes a
    // Set, which changes the cell without touching the flags, so the next loop
    // must check the cell again.
    assert_output("+[.-][-]+++[.-]", b"", b"\x01\x03\x02\x01");
}

#[test]
fn deeply_nested_loops() {
    let src = format!("+{}-{}+.", "[".repeat(500), "]".repeat(500));
    assert_output(&src, b"", b"\x01");
}

#[test]
fn many_sequential_loops() {
    assert_output(&format!("{}+.", "+[-]".repeat(2000)), b"", b"\x01");
}

// Output buffering (the buffer is 4096 bytes)

#[test]
fn output_around_buffer_size() {
    for n in [1, 4095, 4096, 4097, 8191, 8192, 8193, 100_000] {
        let src = format!("{}{}", "+".repeat(65), ".".repeat(n));
        assert_output(&src, b"", &vec![b'A'; n]);
    }
}

#[test]
fn output_from_loops() {
    // 255 * 255 bytes of 'F', flushed many times over.
    assert_output(
        "+++++++[>++++++++++<-]>>+[>+[<<.>>+]<+]",
        b"",
        &vec![b'F'; 255 * 255],
    );
}

// Input

#[test]
fn cat() {
    assert_output(CAT, b"hello, world\n", b"hello, world\n");
    assert_output(CAT, b"", b"");
}

#[test]
fn cat_all_nonzero_bytes() {
    let input: Vec<u8> = (1..=255).collect();
    assert_output(CAT, &input, &input);
}

#[test]
fn cat_large_input() {
    let input: Vec<u8> = Rng::new(1)
        .bytes(50_000)
        .into_iter()
        .map(|b| b.max(1))
        .collect();
    assert_output(CAT, &input, &input);
}

#[test]
fn reverse() {
    assert_output(REVERSE, b"abcdef", b"fedcba");
}

#[test]
fn eof_sets_cell_to_zero() {
    assert_output("+++++,.", b"", b"\x00");
    assert_output(",.,.", b"A", b"A\x00");
    assert_output(",>,.<.", b"A", b"\x00A");
}

#[test]
fn reads_zero_byte() {
    assert_output("+,.", b"\x00", b"\x00");
}

#[test]
fn interleaved_input_and_output() {
    assert_output(",.>,.>,.", b"xyz", b"xyz");

    let src = format!("{}{}>,.", "+".repeat(65), ".".repeat(5000));
    let mut expected = vec![b'A'; 5000];
    expected.push(b'Z');
    assert_output(&src, b"Z", &expected);
}

#[test]
fn output_is_flushed_before_reading() {
    for opt in OPT_LEVELS {
        let ws = Workspace::new();
        let exe = build(&ws, "++++++++[>++++++++<-]>+.,.", opt);

        let mut child = std::process::Command::new(exe)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();

        // Read the prompt without sending any input; it must arrive while the program waits on `,`.
        let mut stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut byte = [0];
            let _ = stdout.read_exact(&mut byte);
            tx.send(byte[0]).unwrap();
        });

        let prompt = rx.recv_timeout(Duration::from_secs(5));
        drop(child.stdin.take());
        child.wait().unwrap();

        assert_eq!(prompt, Ok(b'A'), "-O{opt}: prompt not flushed before `,`");
    }
}

// Fuzzing against the reference interpreter

/// Random program; loops mostly decrement a counter with balanced moves so they tend to terminate.
fn gen_program(rng: &mut Rng, depth: u32, budget: usize) -> String {
    const OPS: &[u8] = b"+++---><>><<..,";
    // Shapes the -O2 passes look for: clears, scans, multiply loops, offset ops,
    // and loops that are dead when the cell is known to be 0.
    const FRAGMENTS: &[&str] = &[
        "[-]",
        "[+]",
        "[---]",
        "[[-]]",
        ">[-]<",
        "<[-]+>",
        "[>]",
        "[<]",
        "[>>]",
        "[<<<]",
        "[->+<]",
        "[>+<-]",
        "[->>+++<<]",
        "[-<+>>--<]",
        "[+>-<]",
        "[--->+<]",
        "[-->+<]",
        ">.<",
        "<.>",
        ">>.<<",
        "[.]",
        "[,.]",
        // Loops whose body doesn't move the pointer overall, which run at an offset.
        ">[<+>-]<",
        "<[>.<-]>",
        ">>[<+<++>>-]<<",
        ">[<[-]>-]<",
        ">,[<.>-]<",
        "[>[-<+>]<-]",
        "[>>>>>>>>>]",
        "[<<<<<<<<<]",
    ];

    let mut out = String::new();
    for _ in 0..budget {
        if rng.chance(0.08) {
            out += FRAGMENTS[rng.range(0, FRAGMENTS.len() - 1)];
        } else if depth < 5 && rng.chance(0.1) {
            let inner_budget = rng.range(1, 12);
            let inner: String = gen_program(rng, depth + 1, inner_budget)
                .chars()
                .filter(|&c| c != '<' && c != '>')
                .collect();
            let k = rng.range(1, 3);
            let tail = if rng.chance(0.85) { "-" } else { "" };
            out += &format!("[{}{inner}{}{tail}]", ">".repeat(k), "<".repeat(k));
        } else {
            out.push(OPS[rng.range(0, OPS.len() - 1)] as char);
        }
    }
    out
}

fn fuzz(opt: u8) {
    const CASES: usize = 250;

    let mut rng = Rng::new(0xB7AF);
    let ws = Workspace::new();
    let mut tested = 0;

    while tested < CASES {
        let budget = rng.range(5, 80);
        let src = format!(">>>>>>>>>>{}", gen_program(&mut rng, 0, budget));
        let len = rng.range(0, 20);
        let input = rng.bytes(len);

        // Skip programs that loop forever or leave the tape.
        let Some(expected) = interpret(&src, &input, 200_000) else {
            continue;
        };

        let out = run(&build(&ws, &src, opt), &input);
        assert!(
            out.status.success() && out.stdout == expected,
            "-O{opt}: mismatch\n  program: {src}\n  input: {input:?}\n  expected: {expected:?}\n  got: {:?} ({})",
            out.stdout,
            out.status,
        );
        tested += 1;
    }
}

#[test]
fn fuzz_o0() {
    fuzz(0);
}

#[test]
fn fuzz_o1() {
    fuzz(1);
}

#[test]
fn fuzz_o2() {
    fuzz(2);
}

#[test]
fn fuzz_o3() {
    fuzz(3);
}
