use crate::lexer::Lexer;

mod lexer;

fn main() {
    let lexer = Lexer::tokenize("[+++--]");
    dbg!(lexer);
}
