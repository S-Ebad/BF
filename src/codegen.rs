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

/// Sets the zero flag from the cell at `offset`, unless it already reflects that cell.
fn test_cell(out: &mut String, flags_cell: Option<isize>, offset: isize) {
    if flags_cell != Some(offset) {
        instr(out, &format!("cmp {}, 0", cell(offset)));
    }
}

/// How many cells a `Scan` checks per iteration.
const SCAN_UNROLL: isize = 4;

/// How far from the pointer the program can touch a cell, in either direction.
///
/// The tape is padded by this much on both sides. Multiply loops are replaced
/// without their check, so they touch their targets even when the counter is 0 and
/// the loop wouldn't have run. They only add 0 there, and with the padding that's
/// always memory the program owns.
fn padding(tokens: &[Token]) -> usize {
    tokens
        .iter()
        .map(|t| match *t.kind() {
            // A scan checks the cells it steps over in order, so it never reads past
            // the zero it stops at.
            TokenKind::Add(_) | TokenKind::Move(_) | TokenKind::Scan(_) => 0,
            TokenKind::AddAt(offset, _)
            | TokenKind::Set(offset, _)
            | TokenKind::Output(offset)
            | TokenKind::Input(offset)
            | TokenKind::JmpZ(_, offset)
            | TokenKind::JmpNZ(_, offset) => offset.unsigned_abs(),
            TokenKind::MulAt(from, to, _) => from.unsigned_abs().max(to.unsigned_abs()),
        })
        .max()
        .unwrap_or(0)
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
            TokenKind::JmpZ(..) => open.push(i),
            TokenKind::JmpNZ(..) => {
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

    let padding = padding(tokens);
    writeln!(
        out,
        "section .bss\nresb {padding}\ntape: resb {TAPE_LEN}\nresb {padding}\noutbuf: resb 4096\n\nsection .text\nglobal _start\n"
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

    // The offset of the cell the zero flag currently reflects (0 means it's set), so
    // the next check of that cell can skip its `cmp`. Arithmetic on a cell sets it,
    // and so does a loop or scan check: every jump into the code after one agrees.
    let mut flags_cell: Option<isize> = None;
    // The cell whose value is in `eax`, so a group of multiplies loads it once.
    let mut loaded: Option<isize> = None;
    let mut scans = 0;
    let mut prev = None;

    for (i, token) in tokens.iter().enumerate().skip(first) {
        if i == resume.index && first < resume.index {
            label(&mut out, ".resume");
            // Reached by the jump too, so nothing is known about the flags or `eax`.
            flags_cell = None;
            loaded = None;
        }

        let kind = *token.kind();
        // A `]` right after its cell was cleared never jumps back, so it needs no check.
        let loop_ends_cleared = match kind {
            TokenKind::JmpNZ(_, offset) => prev == Some(TokenKind::Set(offset, 0)),
            _ => false,
        };

        match kind {
            TokenKind::Add(n) => add_cell(&mut out, 0, n),
            TokenKind::AddAt(offset, n) => add_cell(&mut out, offset, n),
            TokenKind::Move(n) => move_ptr(&mut out, n),
            TokenKind::Set(offset, n) => instr(&mut out, &format!("mov {}, {n}", cell(offset))),

            TokenKind::MulAt(from, to, factor) => {
                if loaded != Some(from) {
                    instr(&mut out, &format!("movzx eax, {}", cell(from)));
                }
                match factor {
                    1 => instr(&mut out, &format!("add {}, al", cell(to))),
                    u8::MAX => instr(&mut out, &format!("sub {}, al", cell(to))),
                    _ => {
                        instr(&mut out, &format!("imul ecx, eax, {factor}"));
                        instr(&mut out, &format!("add {}, cl", cell(to)));
                    }
                }
            }

            // Laid out like a loop, checking SCAN_UNROLL cells per iteration. The
            // pointer moves with `lea`, which leaves the flags alone, so every way out
            // arrives with the zero flag set by the cell it stopped on.
            TokenKind::Scan(step) => {
                let k = scans;
                scans += 1;

                test_cell(&mut out, flags_cell, 0);
                instr(&mut out, &format!("je .scan_end_{k}"));
                label(&mut out, &format!(".scan_{k}"));
                for ahead in 1..SCAN_UNROLL {
                    instr(&mut out, &format!("cmp {}, 0", cell(step * ahead)));
                    instr(&mut out, &format!("je .scan_{k}_{ahead}"));
                }
                instr(&mut out, &format!("lea rbx, [rbx{:+}]", step * SCAN_UNROLL));
                instr(&mut out, "cmp byte [rbx], 0");
                instr(&mut out, &format!("jne .scan_{k}"));
                instr(&mut out, &format!("jmp .scan_end_{k}"));
                // Found `ahead` cells on: each label steps once and falls into the next.
                for ahead in (1..SCAN_UNROLL).rev() {
                    label(&mut out, &format!(".scan_{k}_{ahead}"));
                    instr(&mut out, &format!("lea rbx, [rbx{step:+}]"));
                }
                label(&mut out, &format!(".scan_end_{k}"));
            }

            // `[` checks on entry and `]` jumps back to the start of the body, so each
            // iteration runs a single check.
            TokenKind::JmpZ(n, offset) => {
                test_cell(&mut out, flags_cell, offset);
                instr(&mut out, &format!("je .end_{n}"));
                label(&mut out, &format!(".loop_{n}"));
            }
            TokenKind::JmpNZ(n, offset) => {
                if !loop_ends_cleared {
                    test_cell(&mut out, flags_cell, offset);
                    instr(&mut out, &format!("jne .loop_{n}"));
                }
                label(&mut out, &format!(".end_{n}"));
            }

            TokenKind::Output(offset) => {
                instr(&mut out, &format!("mov cl, {}", cell(offset)));
                instr(&mut out, "call putc");
            }

            TokenKind::Input(offset) => {
                instr(&mut out, "call flush");

                // EOF (or a failed read) leaves the cell at 0.
                instr(&mut out, &format!("mov {}, 0", cell(offset)));
                instr(&mut out, "mov rax, 0");
                instr(&mut out, "mov rdi, 0");
                instr(&mut out, &format!("lea rsi, [rbx{offset:+}]"));
                instr(&mut out, "mov rdx, 1");
                instr(&mut out, "syscall");
            }
        }

        flags_cell = match kind {
            TokenKind::Add(_) | TokenKind::Scan(_) => Some(0),
            TokenKind::AddAt(offset, _) | TokenKind::JmpZ(_, offset) => Some(offset),
            // Without its check, the code after a `]` is reached with the flags of
            // whatever the body did last.
            TokenKind::JmpNZ(_, offset) if !loop_ends_cleared => Some(offset),
            // The last instruction is the add or sub into the target.
            TokenKind::MulAt(_, to, _) => Some(to),
            // `mov` leaves the flags alone, so they still hold for any other cell.
            TokenKind::Set(offset, _) if flags_cell != Some(offset) => flags_cell,
            _ => None,
        };
        loaded = match kind {
            TokenKind::MulAt(from, _, _) => Some(from),
            _ => None,
        };
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
