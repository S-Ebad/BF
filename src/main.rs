use crate::{lexer::tokenize, resolver::resolve_jumps};

mod lexer;
mod resolver;

fn main() {
    let mut tokens = tokenize("-[++]+");
    resolve_jumps(&mut tokens);

    dbg!(&tokens);
}
