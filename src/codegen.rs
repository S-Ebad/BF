use crate::lexer::{Token, TokenKind};
use std::fmt::Write;

// initial code
const INITIAL: &str = r#"
section .bss
tape: resb 30000 ; create tape

section .text
global _start

"#;

// helpers to format instructions
fn instr(out: &mut String, code: &str) {
    writeln!(out, "  {code:<24}").unwrap();
}

fn label(out: &mut String, name: &str) {
    writeln!(out, "{name}:").unwrap();
}

//Code generation
pub fn generate(tokens: &[Token]) -> String {
    // initial code generation
    let mut out = String::from(INITIAL);
    label(&mut out, "_start");
    instr(&mut out, "lea rbx, [rel tape]");

    for token in tokens {
        // create jump labels
        match token.kind() {
            TokenKind::Add => instr(&mut out, "inc byte [rbx]"),
            TokenKind::Sub => instr(&mut out, "dec byte [rbx]"),
            TokenKind::RMove => instr(&mut out, "inc rbx"),
            TokenKind::LMove => instr(&mut out, "dec rbx"),

            TokenKind::JmpZ(n) => {
                label(&mut out, &format!(".loop_{}", n));
                instr(&mut out, &format!("cmp byte [rbx], 0\n  je .end_{}", n));
            }
            TokenKind::JmpNZ(n) => {
                instr(&mut out, &format!("cmp byte [rbx], 0\n  jne .loop_{}", n));
                label(&mut out, &format!(".end_{}", n));
            }

            TokenKind::Output => {
                instr(&mut out, "mov rax, 1");
                instr(&mut out, "mov rdi, 1");
                instr(&mut out, "mov rsi, rbx");
                instr(&mut out, "mov rdx, 1");
                instr(&mut out, "syscall");
            }

            TokenKind::Input => {
                instr(&mut out, "mov rax, 0");
                instr(&mut out, "mov rdi, 0");
                instr(&mut out, "mov rsi, rbx");
                instr(&mut out, "mov rdx, 1");
                instr(&mut out, "syscall");
            }
        }
    }

    // exit instruction
    instr(&mut out, "mov eax, 60");
    instr(&mut out, "xor edi, edi");
    instr(&mut out, "syscall");

    out
}
