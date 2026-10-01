use crate::lexer::{Token, TokenKind};
use std::fmt::Write;

// initial code
const INITIAL: &str = r#"section .bss
tape: resb 30000 ; create tape
outbuf: resb 4096

section .text
global _start

"#;

// helpers to format instructions
fn instr(out: &mut String, code: &str) {
    writeln!(out, "  {code}").unwrap();
}

fn label(out: &mut String, name: &str) {
    writeln!(out, "{name}:").unwrap();
}

/// The memory operand for the cell at `offset` from the pointer.
fn cell(offset: isize) -> String {
    match offset {
        0 => "byte [rbx]".to_string(),
        _ => format!("byte [rbx{offset:+}]"),
    }
}

/// Adds `n` to the cell at `offset`. Sets the zero flag from the result.
fn add_cell(out: &mut String, offset: isize, n: u8) {
    let cell = cell(offset);

    match n {
        1 => instr(out, &format!("inc {cell}")),
        u8::MAX => instr(out, &format!("dec {cell}")),
        _ => instr(out, &format!("add {cell}, {n}")),
    }
}

/// Moves the pointer by `n` cells.
fn move_ptr(out: &mut String, n: isize) {
    match n {
        1 => instr(out, "inc rbx"),
        -1 => instr(out, "dec rbx"),
        n if n > 0 => instr(out, &format!("add rbx, {n}")),
        n => instr(out, &format!("sub rbx, {}", n.unsigned_abs())),
    }
}

/// Sets the flags from the current cell, unless the last instruction already did.
fn test_cell(out: &mut String, flags_from_cell: bool) {
    if !flags_from_cell {
        instr(out, "cmp byte [rbx], 0");
    }
}

/// `putc` appends `cl` to the output buffer, flushing it when full.
fn create_putc(out: &mut String) {
    label(out, "putc");
    instr(out, "lea rax, [rel outbuf]");
    instr(out, "mov [rax + r12], cl");
    instr(out, "inc r12");
    instr(out, "cmp r12, 4096");
    instr(out, "je flush");
    instr(out, "ret");

    label(out, "flush");
    instr(out, "test r12, r12");
    instr(out, "jz .done");
    instr(out, "mov eax, 1");
    instr(out, "mov edi, 1");
    instr(out, "lea rsi, [rel outbuf]");
    instr(out, "mov rdx, r12");
    instr(out, "syscall");
    instr(out, "xor r12d, r12d");
    label(out, ".done");
    instr(out, "ret");
}

pub fn generate(tokens: &[Token]) -> String {
    let mut out = String::from(INITIAL);
    create_putc(&mut out);

    label(&mut out, "_start");
    instr(&mut out, "lea rbx, [rel tape]");
    instr(&mut out, "xor r12d, r12d");

    // Whether the zero flag currently says if the current cell is 0, so the next
    // loop check can skip its `cmp`. Arithmetic on the cell sets it, and so does a
    // loop or scan check: every jump into the code after one agrees on the cell.
    let mut flags_from_cell = false;
    let mut scans = 0;
    let mut prev = None;

    for token in tokens {
        let kind = *token.kind();
        // A `]` right after the cell was cleared never jumps back (multiply loops
        // end like this), so it needs no check.
        let loop_ends_cleared =
            matches!(kind, TokenKind::JmpNZ(_)) && prev == Some(TokenKind::Set(0, 0));

        match kind {
            TokenKind::Add(n) => add_cell(&mut out, 0, n),
            TokenKind::AddAt(offset, n) => add_cell(&mut out, offset, n),
            TokenKind::Move(n) => move_ptr(&mut out, n),
            TokenKind::Set(offset, n) => instr(&mut out, &format!("mov {}, {n}", cell(offset))),

            TokenKind::MulAt(offset, factor) => {
                instr(&mut out, "movzx eax, byte [rbx]");
                match factor {
                    1 => instr(&mut out, &format!("add {}, al", cell(offset))),
                    u8::MAX => instr(&mut out, &format!("sub {}, al", cell(offset))),
                    _ => {
                        instr(&mut out, &format!("imul eax, eax, {factor}"));
                        instr(&mut out, &format!("add {}, al", cell(offset)));
                    }
                }
            }

            // Laid out like a loop: check once on entry, then step and check at the bottom.
            TokenKind::Scan(step) => {
                test_cell(&mut out, flags_from_cell);
                instr(&mut out, &format!("je .scan_end_{scans}"));
                label(&mut out, &format!(".scan_{scans}"));
                move_ptr(&mut out, step);
                instr(&mut out, "cmp byte [rbx], 0");
                instr(&mut out, &format!("jne .scan_{scans}"));
                label(&mut out, &format!(".scan_end_{scans}"));
                scans += 1;
            }

            // `[` checks on entry and `]` jumps back to the start of the body, so each
            // iteration runs a single check.
            TokenKind::JmpZ(n) => {
                test_cell(&mut out, flags_from_cell);
                instr(&mut out, &format!("je .end_{n}"));
                label(&mut out, &format!(".loop_{n}"));
            }
            TokenKind::JmpNZ(n) => {
                if !loop_ends_cleared {
                    test_cell(&mut out, flags_from_cell);
                    instr(&mut out, &format!("jne .loop_{n}"));
                }
                label(&mut out, &format!(".end_{n}"));
            }

            TokenKind::Output(offset) => {
                instr(&mut out, &format!("mov cl, {}", cell(offset)));
                instr(&mut out, "call putc");
            }

            TokenKind::Input => {
                instr(&mut out, "call flush");

                // EOF (or a failed read) leaves the cell at 0.
                instr(&mut out, "mov byte [rbx], 0");
                instr(&mut out, "mov rax, 0");
                instr(&mut out, "mov rdi, 0");
                instr(&mut out, "mov rsi, rbx");
                instr(&mut out, "mov rdx, 1");
                instr(&mut out, "syscall");
            }
        }

        // Without its check, the code after a `]` is reached with the flags of
        // whatever the body did last.
        flags_from_cell = matches!(
            kind,
            TokenKind::Add(_) | TokenKind::Scan(_) | TokenKind::JmpZ(_) | TokenKind::JmpNZ(_)
        ) && !loop_ends_cleared;
        prev = Some(kind);
    }

    instr(&mut out, "call flush");

    // exit instruction
    instr(&mut out, "mov eax, 60");
    instr(&mut out, "xor edi, edi");
    instr(&mut out, "syscall");

    out
}
