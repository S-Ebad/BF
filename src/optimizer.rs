use crate::lexer::{Token, TokenKind};

/// Runs the optimization passes for `level` (the `-O` flag).
///
/// Expects resolved jumps. Loop ids are labels, so passes can drop tokens freely
/// as long as a loop's `[` and `]` are kept or dropped together.
///
/// With `guard_multiplies`, multiply loops keep their brackets, so their targets are
/// only touched (and checked) when the loop would have run. That's needed with
/// `--bounds abort`, where the tape has no padding for a multiply that doesn't run
/// to touch.
pub fn optimize(mut tokens: Vec<Token>, level: u8, guard_multiplies: bool) -> Vec<Token> {
    if level >= 1 {
        tokens = collapse_runs(tokens);
    }

    if level >= 2 {
        // Loop patterns are matched on folded bodies, where a copy loop is just adds.
        tokens = fold_offsets(tokens);
        tokens = replace_loops(tokens, guard_multiplies);
        // Fold again so the Sets and Outputs that replaced loops get offsets too.
        tokens = fold_offsets(tokens);
        tokens = remove_dead_loops(tokens);
    }

    tokens
}

/// For `--bounds abort`, and only then: puts a `Check` in front of every stretch of
/// the source as written that reads or writes a cell, with the range of cells it
/// touches.
///
/// A stretch runs up to and including the next loop bracket or I/O, which touch the
/// cell they're on too. Nothing in a stretch is visible from outside until its last
/// token, so checking the whole stretch at its start aborts at the same point as
/// checking each access. Runs before optimizing, on the tokens straight from the
/// lexer, so the checks cover accesses the optimizer later removes.
pub fn insert_access_checks(tokens: Vec<Token>) -> Vec<Token> {
    use TokenKind::*;

    let mut out = Vec::with_capacity(tokens.len());
    let mut stretch: Vec<Token> = Vec::new();
    // Where the pointer is, from the start of the stretch, and the cells touched.
    let mut offset = 0isize;
    let mut touched: Option<(isize, isize)> = None;

    let end_stretch =
        |out: &mut Vec<Token>, stretch: &mut Vec<Token>, touched: &mut Option<(isize, isize)>| {
            if let (Some((min, max)), Some(first)) = (touched.take(), stretch.first()) {
                out.push(Token::new(Check(min, max), first.span()));
            }
            out.append(stretch);
        };

    for token in tokens {
        let kind = *token.kind();
        if let Move(n) = kind {
            offset += n;
        } else {
            touched = Some(touched.map_or((offset, offset), |(lo, hi)| {
                (lo.min(offset), hi.max(offset))
            }));
        }
        stretch.push(token);

        if matches!(kind, Output(_) | Input(_) | JmpZ(..) | JmpNZ(..)) {
            end_stretch(&mut out, &mut stretch, &mut touched);
            offset = 0;
        }
    }
    end_stretch(&mut out, &mut stretch, &mut touched);

    out
}

/// `Add(n)` and `AddAt(offset, n)` as one shape: an add at an offset.
fn as_add(kind: TokenKind) -> Option<(isize, u8)> {
    match kind {
        TokenKind::Add(n) => Some((0, n)),
        TokenKind::AddAt(offset, n) => Some((offset, n)),
        _ => None,
    }
}

/// An add at `offset`, using the plain `Add` for the current cell.
fn add_at(offset: isize, n: u8) -> TokenKind {
    match offset {
        0 => TokenKind::Add(n),
        _ => TokenKind::AddAt(offset, n),
    }
}

/// Pushes `token`, merging it into the last token in `out` when the two combine into one.
///
/// Every pass builds its output with this, so no pass leaves mergeable neighbours behind.
fn push(out: &mut Vec<Token>, token: Token) {
    use TokenKind::*;

    let Some(last) = out.last_mut() else {
        out.push(token);
        return;
    };

    let (a, b) = (*last.kind(), *token.kind());
    let merged = match (a, b) {
        (Move(x), Move(y)) => Move(x + y),
        // Two checks at the same pointer: one check of both ranges fails exactly when
        // one of them would have, before anything visible happens either way.
        (Check(a, b), Check(c, d)) => Check(a.min(c), b.max(d)),
        _ => match (as_add(a), as_add(b), a, b) {
            (Some((i, x)), Some((j, y)), _, _) if i == j => add_at(i, x.wrapping_add(y)),
            (_, Some((j, y)), Set(i, x), _) if i == j => Set(i, x.wrapping_add(y)),
            // Setting a cell overwrites whatever was done to it just before.
            (Some((i, _)), _, _, Set(j, y)) if i == j => Set(j, y),
            (_, _, Set(i, _), Set(j, y)) if i == j => Set(j, y),
            _ => {
                out.push(token);
                return;
            }
        },
    };
    *last.kind_mut() = merged;

    // Only the last token can have become a no-op. Dropping it can't leave two
    // mergeable tokens next to each other: those were already compared on push.
    if let Add(0) | AddAt(_, 0) | Move(0) = merged {
        out.pop();
    }
}

/// -O1: merges runs of `+`/`-` and `>`/`<` into single ops, dropping runs that cancel out.
///
/// A merged token keeps the span of the first token in its run.
fn collapse_runs(tokens: Vec<Token>) -> Vec<Token> {
    let mut out = Vec::with_capacity(tokens.len());

    for token in tokens {
        push(&mut out, token);
    }

    out
}

/// -O2: turns pointer moves around cell ops into offsets, so `>>++<<` becomes `AddAt(2, 2)`.
///
/// Moves are only tracked as an offset, and every op that works on a cell gets that
/// offset added to its own. Loops whose body doesn't move the pointer overall get
/// it too, checking their cell at an offset, so the pointer doesn't move to them and
/// back. Only `Scan` and loops that do move the pointer need the real pointer: a
/// single `Move` for the net distance is emitted before them.
fn fold_offsets(tokens: Vec<Token>) -> Vec<Token> {
    use TokenKind::*;

    let balanced = balanced_loops(&tokens);
    let mut out = Vec::with_capacity(tokens.len());
    // Net distance moved since the pointer was last real, and the first move's token.
    let mut pending: Option<(isize, Token)> = None;

    for token in tokens {
        let base = pending.as_ref().map_or(0, |(offset, _)| *offset);

        let shifted = match *token.kind() {
            Move(n) => {
                match &mut pending {
                    Some((offset, _)) => *offset += n,
                    None => pending = Some((n, token)),
                }
                continue;
            }
            Add(n) => Some(add_at(base, n)),
            AddAt(offset, n) => Some(add_at(base + offset, n)),
            Set(offset, n) => Some(Set(base + offset, n)),
            MulAt(from, to, n) => Some(MulAt(base + from, base + to, n)),
            Output(offset) => Some(Output(base + offset)),
            Input(offset) => Some(Input(base + offset)),
            Check(min, max) => Some(Check(base + min, base + max)),
            JmpZ(id, offset) if balanced[id] => Some(JmpZ(id, base + offset)),
            JmpNZ(id, offset) if balanced[id] => Some(JmpNZ(id, base + offset)),
            Scan(_) | JmpZ(..) | JmpNZ(..) => None,
        };

        match shifted {
            Some(kind) => push(&mut out, Token::new(kind, token.span())),
            None => {
                flush_move(&mut out, pending.take());
                push(&mut out, token);
            }
        }
    }

    flush_move(&mut out, pending);
    out
}

/// For each loop id, whether the loop leaves the pointer where it found it.
///
/// That's when its body's moves add up to 0, it has no `Scan`, and every loop
/// inside it is balanced too. Such a loop can run at an offset: the pointer is
/// the same at the start of every iteration.
fn balanced_loops(tokens: &[Token]) -> Vec<bool> {
    let loops = tokens
        .iter()
        .filter(|t| matches!(t.kind(), TokenKind::JmpZ(..)))
        .count();
    let ids = tokens.iter().filter_map(|t| match t.kind() {
        TokenKind::JmpZ(id, _) => Some(*id + 1),
        _ => None,
    });
    let mut balanced = vec![false; ids.max().unwrap_or(0).max(loops)];
    // Net move and whether it can still be balanced, for each open loop.
    let mut open: Vec<(isize, bool)> = Vec::new();

    for token in tokens {
        match *token.kind() {
            TokenKind::JmpZ(..) => open.push((0, true)),
            TokenKind::Move(n) => {
                if let Some((net, _)) = open.last_mut() {
                    *net += n;
                }
            }
            TokenKind::Scan(_) => {
                if let Some((_, ok)) = open.last_mut() {
                    *ok = false;
                }
            }
            TokenKind::JmpNZ(id, _) => {
                let (net, ok) = open.pop().expect("jumps are resolved");
                balanced[id] = ok && net == 0;
                if !balanced[id]
                    && let Some((_, outer_ok)) = open.last_mut()
                {
                    *outer_ok = false;
                }
            }
            _ => (),
        }
    }

    balanced
}

/// Emits the net move tracked by `fold_offsets`, keeping the first move's span.
fn flush_move(out: &mut Vec<Token>, pending: Option<(isize, Token)>) {
    if let Some((offset, mut first)) = pending
        && offset != 0
    {
        *first.kind_mut() = TokenKind::Move(offset);
        push(out, first);
    }
}

/// -O2: replaces loops whose effect is known with straight-line ops.
///
/// Patterns are checked when a loop's `]` arrives, so inner loops are already
/// replaced by the time their outer loop is checked.
fn replace_loops(tokens: Vec<Token>, guard_multiplies: bool) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    // Index in `out` of each open loop's `[`.
    let mut open = Vec::new();

    for token in tokens {
        match *token.kind() {
            TokenKind::JmpZ(..) => {
                open.push(out.len());
                push(&mut out, token);
            }
            TokenKind::JmpNZ(_, offset) => {
                let start = open.pop().expect("jumps are resolved");

                match loop_replacement(&out[start + 1..], offset) {
                    Some(kinds) => {
                        let span = out[start].span();
                        let guard = guard_multiplies
                            && kinds
                                .iter()
                                .any(|k| matches!(k, TokenKind::MulAt(..) | TokenKind::Check(..)));

                        // A guarded multiply keeps its `[` and `]`, so it only touches its
                        // targets (and only checks them) when the loop would have run. It
                        // ends with a Set of the counter to 0, so the `]` never jumps back.
                        out.truncate(if guard { start + 1 } else { start });
                        for kind in kinds {
                            push(&mut out, Token::new(kind, span));
                        }
                        if guard {
                            push(&mut out, token);
                        }
                    }
                    None => push(&mut out, token),
                }
            }
            _ => push(&mut out, token),
        }
    }

    out
}

/// What a loop checking the cell at `offset`, with this body, can be replaced with.
///
/// With `--bounds abort` the body has `Check`s for the cells it touches. They don't
/// change what the loop does, so patterns are matched without them, and their range
/// is kept in front of a replacement that touches those cells.
fn loop_replacement(body: &[Token], offset: isize) -> Option<Vec<TokenKind>> {
    use TokenKind::*;

    let mut checked: Option<(isize, isize)> = None;
    let mut ops: Vec<Token> = Vec::with_capacity(body.len());
    for token in body {
        match *token.kind() {
            Check(min, max) => {
                checked = Some(checked.map_or((min, max), |(lo, hi)| (lo.min(min), hi.max(max))));
            }
            _ => ops.push(Token::new(*token.kind(), token.span())),
        }
    }

    let mut kinds = match ops.iter().map(|t| *t.kind()).collect::<Vec<_>>()[..] {
        // `[>]`, `[<<]`: find the next zero cell. A body that moves is never at an
        // offset. With `--bounds abort` a scan checks every cell it steps onto itself.
        [Move(step)] => return Some(vec![Scan(step)]),
        // `[[-]]`: the inner loop already clears the cell.
        [Set(cell, 0)] if cell == offset => vec![Set(offset, 0)],
        _ => multiply_loop(&ops, offset)?,
    };

    // The loop's own cell was already checked by the `[` before it.
    if let Some((min, max)) = checked
        && (min, max) != (offset, offset)
    {
        kinds.insert(0, Check(min, max));
    }
    Some(kinds)
}

/// Matches clear and multiply loops: bodies that only add constants, with no net move.
///
/// The loop runs until the counter (the cell at `offset`) reaches 0, adding the same
/// amount to each target every time. With an odd step, that count is fixed by the
/// counter's value, so `[->++<]` becomes `MulAt(0, 1, 2), Set(0, 0)`. A body
/// without targets is a clear loop. Even steps can loop forever (`[--]` on an odd
/// cell), so they stay loops.
///
/// When the counter is 0, the replacement still touches the targets (adding 0) where
/// the loop wouldn't have run at all. Those cells can be off the tape, so codegen
/// pads the tape on both sides by the largest offset in the program.
fn multiply_loop(body: &[Token], offset: isize) -> Option<Vec<TokenKind>> {
    let mut step = 0u8;
    let mut targets: Vec<(isize, u8)> = Vec::new();

    for token in body {
        let (cell, n) = as_add(*token.kind())?;

        if cell == offset {
            step = step.wrapping_add(n);
        } else if let Some((_, total)) = targets.iter_mut().find(|(c, _)| *c == cell) {
            *total = total.wrapping_add(n);
        } else {
            targets.push((cell, n));
        }
    }

    if step.is_multiple_of(2) {
        return None;
    }

    // The loop runs `count` times, where `value + count * step == 0` (mod 256), so
    // `count == value * -step⁻¹`. Each target gets `count * n` added.
    let inverse = (1..=u8::MAX)
        .find(|&x| x.wrapping_mul(step) == 1)
        .expect("odd numbers are invertible mod 256");
    let per_unit = inverse.wrapping_neg();

    let mut kinds: Vec<TokenKind> = targets
        .into_iter()
        .map(|(cell, n)| TokenKind::MulAt(offset, cell, n.wrapping_mul(per_unit)))
        .filter(|kind| !matches!(kind, TokenKind::MulAt(_, _, 0)))
        .collect();
    kinds.push(TokenKind::Set(offset, 0));

    Some(kinds)
}

/// -O2: removes loops that can never run, because the current cell is known to be 0.
///
/// That's the case at the start of the program, right after a loop on the current
/// cell or a `Scan` (both only end on a zero cell), and after `Set(0, 0)`. This also
/// drops comment loops, like the header many programs start with. A `Set(0, 0)` on
/// a known-zero cell is dropped too.
fn remove_dead_loops(tokens: Vec<Token>) -> Vec<Token> {
    use TokenKind::*;

    let mut out = Vec::with_capacity(tokens.len());
    let mut known_zero = true;
    let mut tokens = tokens.into_iter();

    while let Some(token) = tokens.next() {
        let kind = *token.kind();

        match kind {
            JmpZ(id, 0) if known_zero => {
                // Skip everything up to and including this loop's `]`.
                tokens
                    .by_ref()
                    .find(|t| matches!(*t.kind(), JmpNZ(end, _) if end == id));
                continue;
            }
            Set(0, 0) if known_zero => continue,
            _ => {}
        }

        known_zero = match kind {
            JmpNZ(_, 0) | Scan(_) => true,
            Set(0, n) => n == 0,
            // A loop on another cell may change this one.
            Add(_) | Move(_) | Input(0) | JmpZ(..) | JmpNZ(..) => false,
            MulAt(_, to, _) => known_zero && to != 0,
            // These don't touch the current cell.
            AddAt(..) | Set(..) | Output(_) | Input(_) | Check(..) => known_zero,
        };

        push(&mut out, token);
    }

    out
}

#[cfg(test)]
mod tests;
