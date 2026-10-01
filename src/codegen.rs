use crate::{
    evaluator::Evaluation,
    lexer::{Token, TokenKind},
};
use std::fmt::Write;

/// Number of cells on the tape.
pub const TAPE_LEN: usize = 30000;

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

/// Writes the `len` bytes at `name` to stdout.
fn write_stdout(out: &mut String, name: &str, len: usize) {
    instr(out, "mov eax, 1");
    instr(out, "mov edi, 1");
    instr(out, &format!("lea rsi, [rel {name}]"));
    instr(out, &format!("mov edx, {len}"));
    instr(out, "syscall");
}

fn exit(out: &mut String) {
    instr(out, "mov eax, 60");
    instr(out, "xor edi, edi");
    instr(out, "syscall");
}

/// Defines `name` as these bytes.
///
/// They go after the final `exit` in `.text` rather than in `.rodata`: they're only
/// ever read, and a separate section adds a page-aligned segment that doubles the
/// size of a small executable.
fn data(out: &mut String, name: &str, bytes: &[u8]) {
    label(out, name);
    for chunk in bytes.chunks(32) {
        let bytes: Vec<String> = chunk.iter().map(u8::to_string).collect();
        instr(out, &format!("db {}", bytes.join(", ")));
    }
}

/// Index of the outermost loop's `[` that is still open at `index`, or `index` itself.
///
/// Code before it can never run once the program is at `index`.
fn first_reachable(tokens: &[Token], index: usize) -> usize {
    let mut open = Vec::new();

    for (i, token) in tokens[..index].iter().enumerate() {
        match token.kind() {
            TokenKind::JmpZ(_) => open.push(i),
            TokenKind::JmpNZ(_) => {
                open.pop();
            }
            _ => (),
        }
    }

    open.first().copied().unwrap_or(index)
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

/// Generates the program, starting from `start`: the state the program is in
/// after running part of it at compile time (-O3), or the initial state.
pub fn generate(tokens: &[Token], start: &Evaluation) -> String {
    let mut out = String::new();
    let output = &start.output;

    let Some(resume) = &start.resume else {
        // The whole program ran at compile time: all that's left is its output.
        out.push_str("section .text\nglobal _start\n\n");
        label(&mut out, "_start");
        if !output.is_empty() {
            write_stdout(&mut out, "output", output.len());
        }
        exit(&mut out);

        if !output.is_empty() {
            data(&mut out, "output", output);
        }
        return out;
    };

    writeln!(
        out,
        "section .bss\ntape: resb {TAPE_LEN}\noutbuf: resb 4096\n\nsection .text\nglobal _start\n"
    )
    .unwrap();
    create_putc(&mut out);

    label(&mut out, "_start");

    // Restore what ran at compile time: print its output, then copy in the tape.
    if !output.is_empty() {
        write_stdout(&mut out, "output", output.len());
    }

    let used = resume.tape.iter().position(|&c| c != 0).map(|lo| {
        let hi = resume.tape.iter().rposition(|&c| c != 0).unwrap();
        (lo, &resume.tape[lo..=hi])
    });
    if let Some((lo, cells)) = used {
        instr(&mut out, "lea rsi, [rel tape_init]");
        instr(&mut out, &format!("lea rdi, [rel tape + {lo}]"));
        instr(&mut out, &format!("mov ecx, {}", cells.len()));
        instr(&mut out, "rep movsb");
    }

    instr(&mut out, "lea rbx, [rel tape]");
    if resume.pointer != 0 {
        move_ptr(&mut out, resume.pointer);
    }
    instr(&mut out, "xor r12d, r12d");

    // Carry on where compile time stopped. That can be inside a loop, whose `]`
    // jumps back to code before that point, so code is emitted from the
    // outermost open loop on.
    let first = first_reachable(tokens, resume.index);
    if first < resume.index {
        instr(&mut out, "jmp .resume");
    }

    // Whether the zero flag currently says if the current cell is 0, so the next
    // loop check can skip its `cmp`. Arithmetic on the cell sets it, and so does a
    // loop or scan check: every jump into the code after one agrees on the cell.
    let mut flags_from_cell = false;
    let mut scans = 0;
    let mut prev = None;

    for (i, token) in tokens.iter().enumerate().skip(first) {
        if i == resume.index && first < resume.index {
            label(&mut out, ".resume");
            // Reached by the jump too, so the flags say nothing about the cell.
            flags_from_cell = false;
        }

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
    exit(&mut out);

    if !output.is_empty() {
        data(&mut out, "output", output);
    }
    if let Some((_, cells)) = used {
        data(&mut out, "tape_init", cells);
    }

    out
}
