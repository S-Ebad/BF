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

    let mut scans = 0;

    for token in tokens {
        match *token.kind() {
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
                instr(&mut out, "cmp byte [rbx], 0");
                instr(&mut out, &format!("je .scan_end_{scans}"));
                label(&mut out, &format!(".scan_{scans}"));
                move_ptr(&mut out, step);
                instr(&mut out, "cmp byte [rbx], 0");
                instr(&mut out, &format!("jne .scan_{scans}"));
                label(&mut out, &format!(".scan_end_{scans}"));
                scans += 1;
            }

            TokenKind::JmpZ(n) => {
                label(&mut out, &format!(".loop_{n}"));
                instr(&mut out, "cmp byte [rbx], 0");
                instr(&mut out, &format!("je .end_{n}"));
            }
            TokenKind::JmpNZ(n) => {
                instr(&mut out, "cmp byte [rbx], 0");
                instr(&mut out, &format!("jne .loop_{n}"));
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
    }

    instr(&mut out, "call flush");

    // exit instruction
    instr(&mut out, "mov eax, 60");
    instr(&mut out, "xor edi, edi");
    instr(&mut out, "syscall");

    out
}
