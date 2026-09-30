use crate::{
    errors::{BFError, Span},
    lexer::{Token, TokenKind},
};

pub fn resolve_jumps(tokens: &mut [Token]) -> Result<(), Vec<BFError>> {
    let mut open_loops: Vec<(usize, Span)> = vec![];
    let mut errors: Vec<BFError> = Vec::new();

    let mut counter = 0usize;

    for token in tokens.iter_mut() {
        match token.kind_mut() {
            TokenKind::JmpZ(n) => {
                *n = counter;
                open_loops.push((*n, token.span()));

                counter += 1;
            }
            TokenKind::JmpNZ(n) => {
                if let Some((open, _)) = open_loops.pop() {
                    *n = open;
                } else {
                    errors.push(BFError::UnmatchedCloseBracket(token.span()));
                }
            }

            _ => (),
        }
    }

    for (_, span) in open_loops {
        errors.push(BFError::UnmatchedOpenBracket(span));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
