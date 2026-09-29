use crate::lexer::{Token, TokenKind};

pub fn resolve_jumps(tokens: &mut [Token]) {
    let mut jump_table: Vec<(&mut usize, usize)> = vec![];

    for (i, token) in tokens.iter_mut().enumerate() {
        match token.kind_mut() {
            TokenKind::JmpZ(n) => {
                jump_table.push((n, i));
            }
            TokenKind::JmpNZ(n) => {
                if let Some((last, idx)) = jump_table.pop() {
                    *last = i + 1 - idx;
                    *n = i - idx;
                } else {
                    todo!("handle case where there's a missing opening bracket")
                }
            }

            _ => (),
        }
    }

    if !jump_table.is_empty() {
        todo!("handle case where there's a missing closing bracket");
    }
}
