use crate::{codegen::generate, lexer::tokenize, resolver::resolve_jumps};
use clap::Parser;

mod cli;
mod codegen;
mod emit;
mod errors;
mod lexer;
mod resolver;

fn compile(src: &str) -> Result<String, Vec<errors::BFError>> {
    let mut tokens = tokenize(src);

    if let Err(mut errors) = resolve_jumps(&mut tokens) {
        errors.sort_by_key(|err| err.span());

        return Err(errors);
    }

    let asm = generate(&tokens);

    Ok(asm)
}

fn main() -> anyhow::Result<()> {
    let config = cli::Config::parse();
    let src = config.read_source()?;

    let asm = compile(&src).map_err(|errors| {
        let count = errors.len();
        errors::report(&src, errors);

        anyhow::anyhow!("could not compile due to {} previous error(s)", count)
    })?;

    emit::write_output(&asm, &config)?;

    Ok(())
}
