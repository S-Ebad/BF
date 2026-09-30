use std::io::{self, Write};

use crate::{codegen::generate, lexer::tokenize, resolver::resolve_jumps};

mod codegen;
mod lexer;
mod resolver;

fn main() {
    let mut tokens = tokenize(
        r#"
>++++++++[<+++++++++>-]<.>++++[<+++++++>-]<+.+++++++..+++.>>++++++[<+++++++>-]<+
+.------------.>++++++[<+++++++++>-]<+.<.+++.------.--------.>>>++++[<++++++++>-
]<+.

    "#,
    );
    resolve_jumps(&mut tokens);

    let asm = generate(&tokens);
    _ = io::stdout().write_all(asm.as_bytes());
    // println!("{}", asm);

    // dbg!(&tokens);
}
