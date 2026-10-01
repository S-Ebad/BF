use crate::{codegen::generate, lexer::tokenize, optimizer::optimize, resolver::resolve_jumps};
use clap::Parser;

mod cli;
mod codegen;
mod emit;
mod errors;
mod lexer;
mod optimizer;
mod resolver;

fn compile(src: &str, opt: u8) -> Result<String, Vec<errors::BFError>> {
    let mut tokens = tokenize(src);

    // Resolve before optimizing, so every bracket is checked against the source as written.
    if let Err(mut errors) = resolve_jumps(&mut tokens) {
        errors.sort_by_key(|err| err.span());

        return Err(errors);
    }

    let tokens = optimize(tokens, opt);

    let asm = generate(&tokens);

    Ok(asm)
}

fn main() -> anyhow::Result<()> {
    let config = cli::Config::parse();
    let src = config.read_source()?;

    let asm = compile(&src, config.opt).map_err(|errors| {
        let count = errors.len();
        errors::report(&src, errors);

        anyhow::anyhow!("could not compile due to {} previous error(s)", count)
    })?;

    emit::write_output(&asm, &config)?;

    Ok(())
}

#[cfg(test)]
mod tests;
