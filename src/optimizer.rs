use crate::lexer::{Token, TokenKind};

/// Runs the optimization passes for `level` (the `-O` flag).
///
/// Expects resolved jumps. Loop ids are labels, so passes can drop tokens freely
/// as long as a loop's `[` and `]` are kept or dropped together.
pub fn optimize(tokens: Vec<Token>, level: u8) -> Vec<Token> {
    if level == 0 {
        return tokens;
    }

    if level >= 1 {
        collapse_runs(tokens)
    } else {
        tokens
    }
}

/// -O1: merges runs of `+`/`-` and `>`/`<` into single ops, dropping runs that cancel out.
///
/// A merged token keeps the span of the first token in its run.
fn collapse_runs(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());

    for token in tokens {
        match (out.last_mut().map(Token::kind_mut), token.kind()) {
            (Some(TokenKind::Add(a)), TokenKind::Add(b)) => *a = a.wrapping_add(*b),
            (Some(TokenKind::Move(a)), TokenKind::Move(b)) => *a += b,
            _ => {
                out.push(token);
                continue;
            }
        }

        // Only the last token can have become a no-op. Dropping it can't leave two
        // mergeable tokens next to each other: those were already compared on push.
        if let Some(TokenKind::Add(0) | TokenKind::Move(0)) = out.last().map(Token::kind) {
            out.pop();
        }
    }

    out
}

#[cfg(test)]
mod tests;
