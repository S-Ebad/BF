use crate::lexer::{Token, TokenKind};

pub fn resolve_jumps(tokens: &mut [Token]) {
    let mut open_loops: Vec<usize> = vec![];
    let mut counter = 0usize;

    for token in tokens.iter_mut() {
        match token.kind_mut() {
            TokenKind::JmpZ(n) => {
                *n = counter;
                open_loops.push(*n);

                counter += 1;
            },
            TokenKind::JmpNZ(n) => {
                if let Some(open) = open_loops.pop() {
                    *n = open;
                } else {
                    todo!("handle no opening");
                }
            },

            _ => (),
        }
    }

    if !open_loops.is_empty() {
        todo!("handle no closing");
    }
}
