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

/// What codegen knows about the registers at the current point.
#[derive(Default, Clone, Copy)]
struct State {
    /// The offset of the cell the zero flag reflects (it's set when that cell is 0),
    /// so the next check of that cell can skip its `cmp`. Arithmetic on a cell sets
    /// it, and so does a loop or scan check: every jump into the code after one agrees.
    flags_cell: Option<isize>,
    /// The cell whose value is in `eax`, so a group of multiplies loads it once.
    loaded: Option<isize>,
}

impl State {
    /// The same knowledge after the pointer moves by `n` cells without touching
    /// the flags or `eax` (a `lea`).
    fn moved(self, n: isize) -> Self {
        Self {
            flags_cell: self.flags_cell.map(|offset| offset - n),
            loaded: self.loaded.map(|offset| offset - n),
        }
    }
}

/// Emits an op that works on cells, with every offset `shift` cells further on.
///
/// Returns false for ops that aren't straight-line cell ops.
fn cell_op(out: &mut String, state: &mut State, kind: TokenKind, shift: isize) -> bool {
    match kind {
        TokenKind::Add(n) => {
            add_cell(out, shift, n);
            *state = State {
                flags_cell: Some(shift),
                loaded: None,
            };
        }
        TokenKind::AddAt(offset, n) => {
            add_cell(out, offset + shift, n);
            *state = State {
                flags_cell: Some(offset + shift),
                loaded: None,
            };
        }
        TokenKind::MulAt(from, to, factor) => {
            let (from, to) = (from + shift, to + shift);
            if state.loaded != Some(from) {
                instr(out, &format!("movzx eax, {}", cell(from)));
            }
            match factor {
                1 => instr(out, &format!("add {}, al", cell(to))),
                u8::MAX => instr(out, &format!("sub {}, al", cell(to))),
                _ => {
                    instr(out, &format!("imul ecx, eax, {factor}"));
                    instr(out, &format!("add {}, cl", cell(to)));
                }
            }
            // The last instruction is the add or sub into the target.
            *state = State {
                flags_cell: Some(to),
                loaded: Some(from),
            };
        }
        _ => return false,
    }
    true
}

/// Emits a group of `Set`s, combining neighbouring cells into wider stores: setting
/// 9 cells to 0 is a `qword` store and a `byte` store instead of 9 `byte` stores.
///
/// `sets` are in program order; a later set of the same cell wins.
fn set_cells(out: &mut String, state: &mut State, sets: &[(isize, u8)], shift: isize) {
    let mut cells: Vec<(isize, u8)> = Vec::with_capacity(sets.len());
    for &(offset, n) in sets {
        let offset = offset + shift;
        match cells.iter_mut().find(|(o, _)| *o == offset) {
            Some(cell) => cell.1 = n,
            None => cells.push((offset, n)),
        }
    }
    cells.sort_by_key(|&(offset, _)| offset);

    for run in cells.chunk_by(|a, b| a.0 + 1 == b.0) {
        let mut rest = run;
        while let Some(&(offset, _)) = rest.first() {
            let size = [8, 4, 2, 1]
                .into_iter()
                .find(|&size| size <= rest.len() && fits_store(&rest[..size]))
                .unwrap();
            let value = rest[..size]
                .iter()
                .rev()
                .fold(0u64, |acc, &(_, n)| (acc << 8) | u64::from(n));
            let width = match size {
                8 => "qword",
                4 => "dword",
                2 => "word",
                _ => "byte",
            };
            instr(out, &format!("mov {width} [rbx{offset:+}], {value}"));
            rest = &rest[size..];
        }
    }

    // `mov` leaves the flags alone, so they still hold unless that cell was set.
    if let Some(flagged) = state.flags_cell
        && cells.iter().any(|&(offset, _)| offset == flagged)
    {
        state.flags_cell = None;
    }
    state.loaded = None;
}

/// Whether these cells can be set with one `mov` of an immediate. A `qword` store
/// only takes a sign-extended 32-bit immediate.
fn fits_store(cells: &[(isize, u8)]) -> bool {
    if cells.len() < 8 {
        return true;
    }
    let value = cells
        .iter()
        .rev()
        .fold(0u64, |acc, &(_, n)| (acc << 8) | u64::from(n));
    i32::try_from(value as i64).is_ok()
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
///
/// Runs of the same byte are written with `times`, so a tape full of 1s is one line.
fn data(out: &mut String, name: &str, bytes: &[u8]) {
    /// Runs at least this long get a `times` line of their own.
    const MIN_RUN: usize = 16;

    label(out, name);

    let mut pending: Vec<u8> = Vec::new();
    let flush = |out: &mut String, pending: &mut Vec<u8>| {
        for chunk in pending.chunks(32) {
            let bytes: Vec<String> = chunk.iter().map(u8::to_string).collect();
            instr(out, &format!("db {}", bytes.join(", ")));
        }
        pending.clear();
    };

    for run in bytes.chunk_by(|a, b| a == b) {
        if run.len() >= MIN_RUN {
            flush(out, &mut pending);
            instr(out, &format!("times {} db {}", run.len(), run[0]));
        } else {
            pending.extend_from_slice(run);
        }
    }
    flush(out, &mut pending);
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

/// For each bracket, the index of its partner. Other tokens map to 0.
fn match_loops(tokens: &[Token]) -> Vec<usize> {
    let mut partner = vec![0; tokens.len()];
    let mut open = Vec::new();

    for (i, token) in tokens.iter().enumerate() {
        match token.kind() {
            TokenKind::JmpZ(..) => open.push(i),
            TokenKind::JmpNZ(..) => {
                let j = open.pop().expect("jumps are resolved");
                partner[i] = j;
                partner[j] = i;
            }
            _ => (),
        }
    }

    partner
}

/// If a loop checking the cell at `offset` can be unrolled, how far its body moves
/// the pointer.
///
/// That's a body of only cell ops and `Set`s, with at most one `Move`, at the end
/// (which is where -O2 leaves it). A body that sets the loop's own cell runs at
/// most once, so there's nothing to gain.
fn unrollable(body: &[Token], offset: isize) -> Option<isize> {
    let (ops, step) = match body.split_last().map(|(last, rest)| (*last.kind(), rest)) {
        Some((TokenKind::Move(step), rest)) => (rest, step),
        _ => (body, 0),
    };

    let straight = ops.iter().all(|t| match *t.kind() {
        TokenKind::Add(_) | TokenKind::AddAt(..) | TokenKind::MulAt(..) => true,
        TokenKind::Set(cell, _) => cell != offset,
        _ => false,
    });

    (straight && !ops.is_empty()).then_some(step)
}

/// Emits a loop with its body twice per iteration: the second copy is shifted by
/// `step`, the distance one pass moves, so the pointer only moves once per two
/// passes and there's one jump back instead of two.
fn unrolled_loop(
    out: &mut String,
    state: &mut State,
    body: &[Token],
    n: usize,
    offset: isize,
    step: isize,
) {
    let ops = match body.last().map(|t| *t.kind()) {
        Some(TokenKind::Move(_)) => &body[..body.len() - 1],
        _ => body,
    };
    // After one pass, the exit lands one step on.
    let half_exit = if step == 0 {
        format!(".end_{n}")
    } else {
        format!(".half_{n}")
    };

    test_cell(out, state.flags_cell, offset);
    instr(out, &format!("je .end_{n}"));
    label(out, &format!(".loop_{n}"));
    *state = State {
        flags_cell: Some(offset),
        loaded: None,
    };

    for shift in [0, step] {
        let mut sets = Vec::new();
        for token in ops {
            match *token.kind() {
                TokenKind::Set(cell, value) => sets.push((cell, value)),
                kind => {
                    if !sets.is_empty() {
                        set_cells(out, state, &std::mem::take(&mut sets), shift);
                    }
                    cell_op(out, state, kind, shift);
                }
            }
        }
        if !sets.is_empty() {
            set_cells(out, state, &sets, shift);
        }

        if shift == 0 {
            test_cell(out, state.flags_cell, offset + step);
            instr(out, &format!("je {half_exit}"));
        }
    }

    if step != 0 {
        instr(out, &format!("lea rbx, [rbx{:+}]", 2 * step));
        *state = state.moved(2 * step);
    }
    test_cell(out, state.flags_cell, offset);
    instr(out, &format!("jne .loop_{n}"));

    if step != 0 {
        // Both ways out arrive with the zero flag set by the loop's cell; `lea`
        // moves the pointer onto it without touching the flags.
        instr(out, &format!("jmp .end_{n}"));
        label(out, &half_exit);
        instr(out, &format!("lea rbx, [rbx{step:+}]"));
    }
    label(out, &format!(".end_{n}"));
    *state = State {
        flags_cell: Some(offset),
        loaded: None,
    };
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

    // Nothing left to run: the whole program ran at compile time, or it had no
    // commands to begin with (or only loops -O2 removed as dead). All that's left is
    // its output.
    let Some(resume) = start.resume.as_ref().filter(|_| !tokens.is_empty()) else {
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

    let partner = match_loops(tokens);
    // Whether there's a `.resume` label to put before `tokens[resume.index]`.
    let resume_label = first < resume.index;
    let mut state = State::default();
    let mut scans = 0;
    // Cells the `Set`s just before the current token set to 0.
    let mut cleared: Vec<isize> = Vec::new();
    let mut i = first;

    while i < tokens.len() {
        if i == resume.index && resume_label {
            label(&mut out, ".resume");
            // Reached by the jump too, so nothing is known about the flags or `eax`.
            state = State::default();
        }

        let kind = *tokens[i].kind();
        let just_cleared = std::mem::take(&mut cleared);

        if cell_op(&mut out, &mut state, kind, 0) {
            i += 1;
            continue;
        }

        match kind {
            TokenKind::Set(..) => {
                // All the Sets in a row, but not past the `.resume` label.
                let mut sets = Vec::new();
                while let Some(TokenKind::Set(offset, n)) = tokens.get(i).map(|t| *t.kind())
                    && (sets.is_empty() || !(i == resume.index && resume_label))
                {
                    sets.push((offset, n));
                    i += 1;
                }
                set_cells(&mut out, &mut state, &sets, 0);
                cleared = sets
                    .iter()
                    .filter(|&&(offset, _)| {
                        sets.iter().rev().find(|(o, _)| *o == offset).unwrap().1 == 0
                    })
                    .map(|&(offset, _)| offset)
                    .collect();
                continue;
            }

            TokenKind::Move(n) => {
                move_ptr(&mut out, n);
                state = State::default();
            }

            // Laid out like a loop, checking SCAN_UNROLL cells per iteration. The
            // pointer moves with `lea`, which leaves the flags alone, so every way out
            // arrives with the zero flag set by the cell it stopped on.
            TokenKind::Scan(step) => {
                let k = scans;
                scans += 1;

                test_cell(&mut out, state.flags_cell, 0);
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
                state = State {
                    flags_cell: Some(0),
                    loaded: None,
                };
            }

            TokenKind::JmpZ(n, offset) => {
                let end = partner[i];
                let resume_inside = resume_label && (i + 1..=end).contains(&resume.index);

                if !resume_inside && let Some(step) = unrollable(&tokens[i + 1..end], offset) {
                    unrolled_loop(&mut out, &mut state, &tokens[i + 1..end], n, offset, step);
                    i = end + 1;
                    continue;
                }

                // `[` checks on entry and `]` jumps back to the start of the body, so
                // each iteration runs a single check.
                test_cell(&mut out, state.flags_cell, offset);
                instr(&mut out, &format!("je .end_{n}"));
                label(&mut out, &format!(".loop_{n}"));
                state = State {
                    flags_cell: Some(offset),
                    loaded: None,
                };
            }
            TokenKind::JmpNZ(n, offset) => {
                // A `]` right after its cell was cleared never jumps back, so it needs
                // no check. Then the code after it is reached with the flags of
                // whatever the body did last.
                if just_cleared.contains(&offset) {
                    state = State::default();
                } else {
                    test_cell(&mut out, state.flags_cell, offset);
                    instr(&mut out, &format!("jne .loop_{n}"));
                    state = State {
                        flags_cell: Some(offset),
                        loaded: None,
                    };
                }
                label(&mut out, &format!(".end_{n}"));
            }

            TokenKind::Output(offset) => {
                instr(&mut out, &format!("mov cl, {}", cell(offset)));
                instr(&mut out, "call putc");
                state = State::default();
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
                state = State::default();
            }

            TokenKind::Add(_) | TokenKind::AddAt(..) | TokenKind::MulAt(..) => {
                unreachable!("handled by cell_op")
            }
        }

        i += 1;
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
