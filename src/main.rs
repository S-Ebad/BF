use crate::{
    codegen::generate,
    evaluator::{Evaluation, evaluate},
    lexer::tokenize,
    optimizer::optimize,
    resolver::resolve_jumps,
};
use clap::Parser;

mod cli;
mod codegen;
mod emit;
mod errors;
mod evaluator;
mod lexer;
mod optimizer;
mod resolver;

fn compile(src: &str, opt: u8, step_limit: u64) -> Result<String, Vec<errors::BFError>> {
    let mut tokens = tokenize(src);

    // Resolve before optimizing, so every bracket is checked against the source as written.
    if let Err(mut errors) = resolve_jumps(&mut tokens) {
        errors.sort_by_key(|err| err.span());

        return Err(errors);
    }

    let tokens = optimize(tokens, opt);

    // -O3 runs as much of the program as it can at compile time.
    let start = if opt >= 3 {
        evaluate(&tokens, step_limit)
    } else {
        Evaluation::start()
    };

    let asm = generate(&tokens, &start);

    Ok(asm)
}

fn main() -> anyhow::Result<()> {
    let config = cli::Config::parse();
    let src = config.read_source()?;

    let asm = compile(&src, config.opt, config.step_limit).map_err(|errors| {
        let count = errors.len();
        errors::report(&src, errors);

        anyhow::anyhow!("could not compile due to {} previous error(s)", count)
    })?;

    emit::write_output(&asm, &config)?;

    Ok(())
}

#[cfg(test)]
mod tests;
