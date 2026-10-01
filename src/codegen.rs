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

fn create_putc(out: &mut String) {
    label(out, "putc");
    instr(out, "lea rax, [rel outbuf]");
    instr(out, "mov cl, [rbx]");
    instr(out, "mov [rax + r12], cl");
    instr(out, "inc r12");
    instr(out, "cmp r12, 4096");
    instr(out, "je flush");
    instr(out, "ret");

    label(out, "flush"); // flush label
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

//Code generation
pub fn generate(tokens: &[Token]) -> String {
    // initial code generation
    let mut out = String::from(INITIAL);
    create_putc(&mut out);

    label(&mut out, "_start");
    instr(&mut out, "lea rbx, [rel tape]");
    instr(&mut out, "xor r12d, r12d");

    for token in tokens {
        match token.kind() {
            TokenKind::Add(1) => instr(&mut out, "inc byte [rbx]"),
            TokenKind::Add(u8::MAX) => instr(&mut out, "dec byte [rbx]"),
            TokenKind::Add(n) => instr(&mut out, &format!("add byte [rbx], {n}")),

            TokenKind::Move(1) => instr(&mut out, "inc rbx"),
            TokenKind::Move(-1) => instr(&mut out, "dec rbx"),
            TokenKind::Move(n) if *n > 0 => instr(&mut out, &format!("add rbx, {n}")),
            TokenKind::Move(n) => instr(&mut out, &format!("sub rbx, {}", n.unsigned_abs())),

            TokenKind::JmpZ(n) => {
                label(&mut out, &format!(".loop_{}", n));
                instr(&mut out, &format!("cmp byte [rbx], 0\n  je .end_{}", n));
            }
            TokenKind::JmpNZ(n) => {
                instr(&mut out, &format!("cmp byte [rbx], 0\n  jne .loop_{}", n));
                label(&mut out, &format!(".end_{}", n));
            }

            TokenKind::Output => {
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
