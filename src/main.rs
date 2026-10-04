use crate::{
    codegen::{Tape, generate},
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

/// How to compile a program.
#[derive(Clone, Copy)]
pub struct Options {
    /// Optimization level (`-O`).
    pub opt: u8,
    /// How many steps -O3 runs the program for at compile time.
    pub step_limit: u64,
    pub tape: Tape,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            opt: 0,
            step_limit: evaluator::DEFAULT_STEP_LIMIT,
            tape: Tape::default(),
        }
    }
}

fn compile(src: &str, options: &Options) -> Result<String, Vec<errors::BFError>> {
    let Options {
        opt,
        step_limit,
        tape,
    } = *options;
    let mut tokens = tokenize(src);

    // Resolve before optimizing, so every bracket is checked against the source as written.
    if let Err(mut errors) = resolve_jumps(&mut tokens) {
        errors.sort_by_key(|err| err.span());

        return Err(errors);
    }

    // With --bounds abort, and only then, record which cells the source as written
    // touches, before the optimizer can remove any of those accesses.
    let abort = tape.bounds == cli::Bounds::Abort;
    if abort {
        tokens = optimizer::insert_access_checks(tokens);
    }

    // With --bounds abort, a multiply loop that doesn't run mustn't touch its targets:
    // they can be off the tape, and that tape has no padding.
    let tokens = optimize(tokens, opt, abort);

    // -O3 runs as much of the program as it can at compile time.
    let start = if opt >= 3 {
        evaluate(&tokens, step_limit, tape.size)
    } else {
        Evaluation::start()
    };

    let asm = generate(&tokens, &start, tape);

    Ok(asm)
}

fn main() -> anyhow::Result<()> {
    let config = cli::Config::parse();
    let src = config.read_source()?;

    let asm = compile(&src, &config.options()).map_err(|errors| {
        let count = errors.len();
        errors::report(&src, errors);

        anyhow::anyhow!("could not compile due to {} previous error(s)", count)
    })?;

    emit::write_output(&asm, &config)?;

    Ok(())
}

#[cfg(test)]
mod tests;
