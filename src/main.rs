use std::io::{self, Write};

use crate::{codegen::generate, errors::render_error, lexer::tokenize, resolver::resolve_jumps};

mod codegen;
mod errors;
mod lexer;
mod resolver;

fn main() {
    let source = "++++++++++[>+>+++>+++++++>++++++++++<<<<-]>>>++.>+.+++++++..+++.<<++.>+++++++++++++++.>.+++.------.--------.<<+.<.";

    let mut tokens = tokenize(source);

    if let Err(mut errors) = resolve_jumps(&mut tokens) {
        errors.sort_by_key(|err| err.span());

        for err in errors {
            render_error(source, &err);
            eprintln!();
        }

        std::process::exit(1);
    }

    let asm = generate(&tokens);
    _ = io::stdout().write_all(asm.as_bytes());
}
