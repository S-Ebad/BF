use crate::lexer::{Token, TokenKind};

/// Runs the optimization passes for `level` (the `-O` flag).
///
/// Expects resolved jumps. Loop ids are labels, so passes can drop tokens freely
/// as long as a loop's `[` and `]` are kept or dropped together.
pub fn optimize(mut tokens: Vec<Token>, level: u8) -> Vec<Token> {
    if level >= 1 {
        tokens = collapse_runs(tokens);
    }

    if level >= 2 {
        // Loop patterns are matched on folded bodies, where a copy loop is just adds.
        tokens = fold_offsets(tokens);
        tokens = replace_loops(tokens);
        // Fold again so the Sets and Outputs that replaced loops get offsets too.
        tokens = fold_offsets(tokens);
        tokens = remove_dead_loops(tokens);
    }

    tokens
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
/// Between two ops that need the real pointer (input, loops, `MulAt`, `Scan`), moves
/// are only tracked as an offset. Adds, Sets and Outputs get that offset added to
/// their own, and a single `Move` for the net distance is emitted before the next op
/// that needs the pointer.
fn fold_offsets(tokens: Vec<Token>) -> Vec<Token> {
    use TokenKind::*;

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
            Output(offset) => Some(Output(base + offset)),
            MulAt(..) | Scan(_) | Input | JmpZ(_) | JmpNZ(_) => None,
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
fn replace_loops(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    // Index in `out` of each open loop's `[`.
    let mut open = Vec::new();

    for token in tokens {
        match token.kind() {
            TokenKind::JmpZ(_) => {
                open.push(out.len());
                push(&mut out, token);
            }
            TokenKind::JmpNZ(_) => {
                let start = open.pop().expect("jumps are resolved");
                let span = out[start].span();

                match loop_replacement(&out[start + 1..]) {
                    Some(Replacement::Unguarded(kinds)) => {
                        out.truncate(start);
                        for kind in kinds {
                            push(&mut out, Token::new(kind, span));
                        }
                    }
                    Some(Replacement::Guarded(kinds)) => {
                        // Keep the `[` check and the `]`; the body now runs at most once.
                        out.truncate(start + 1);
                        for kind in kinds {
                            push(&mut out, Token::new(kind, span));
                        }
                        push(&mut out, token);
                    }
                    None => push(&mut out, token),
                }
            }
            _ => push(&mut out, token),
        }
    }

    out
}

enum Replacement {
    /// Replaces the whole loop.
    Unguarded(Vec<TokenKind>),
    /// Replaces the loop body. The loop's brackets stay, so the new body only runs
    /// when the cell isn't 0, and it always leaves the cell at 0.
    Guarded(Vec<TokenKind>),
}

/// What a loop with this body can be replaced with, if anything.
fn loop_replacement(body: &[Token]) -> Option<Replacement> {
    use TokenKind::*;

    match body.iter().map(|t| *t.kind()).collect::<Vec<_>>()[..] {
        // `[>]`, `[<<]`: find the next zero cell.
        [Move(step)] => Some(Replacement::Unguarded(vec![Scan(step)])),
        // `[[-]]`: the inner loop already clears the cell.
        [Set(0, 0)] => Some(Replacement::Unguarded(vec![Set(0, 0)])),
        _ => multiply_loop(body),
    }
}

/// Matches clear and multiply loops: bodies that only add constants, with no net move.
///
/// The loop runs until the counter (offset 0) reaches 0, adding the same amount to
/// each target every time. With an odd step, that count is fixed by the counter's
/// value, so `[->++<]` becomes `[MulAt(1, 2), Set(0, 0)]`. A body without targets is
/// a clear loop. Even steps can loop forever (`[--]` on an odd cell), so they stay loops.
///
/// Multiplies stay guarded by the loop's `[`: when the counter is 0 the original loop
/// never touches its targets, and those can be outside the tape (Mandelbrot does this
/// near the start of the tape).
fn multiply_loop(body: &[Token]) -> Option<Replacement> {
    let mut step = 0u8;
    let mut targets: Vec<(isize, u8)> = Vec::new();

    for token in body {
        let (offset, n) = as_add(*token.kind())?;

        if offset == 0 {
            step = step.wrapping_add(n);
        } else if let Some((_, total)) = targets.iter_mut().find(|(o, _)| *o == offset) {
            *total = total.wrapping_add(n);
        } else {
            targets.push((offset, n));
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
        .map(|(offset, n)| TokenKind::MulAt(offset, n.wrapping_mul(per_unit)))
        .filter(|kind| !matches!(kind, TokenKind::MulAt(_, 0)))
        .collect();

    if kinds.is_empty() {
        return Some(Replacement::Unguarded(vec![TokenKind::Set(0, 0)]));
    }

    kinds.push(TokenKind::Set(0, 0));
    Some(Replacement::Guarded(kinds))
}

/// -O2: removes loops that can never run, because the current cell is known to be 0.
///
/// That's the case at the start of the program, right after a loop or `Scan` (both
/// only end on a zero cell), and after `Set(0, 0)`. This also drops comment loops,
/// like the header many programs start with. A `Set(0, 0)` on a known-zero cell is
/// dropped too.
fn remove_dead_loops(tokens: Vec<Token>) -> Vec<Token> {
    use TokenKind::*;

    let mut out = Vec::with_capacity(tokens.len());
    let mut known_zero = true;
    let mut tokens = tokens.into_iter();

    while let Some(token) = tokens.next() {
        let kind = *token.kind();

        match kind {
            JmpZ(id) if known_zero => {
                // Skip everything up to and including this loop's `]`.
                tokens.by_ref().find(|t| *t.kind() == JmpNZ(id));
                continue;
            }
            Set(0, 0) if known_zero => continue,
            _ => {}
        }

        known_zero = match kind {
            JmpNZ(_) | Scan(_) => true,
            Set(0, n) => n == 0,
            // These don't touch the current cell.
            AddAt(..) | Set(..) | MulAt(..) | Output(_) => known_zero,
            Add(_) | Move(_) | Input | JmpZ(_) => false,
        };

        push(&mut out, token);
    }

    out
}

#[cfg(test)]
mod tests;
