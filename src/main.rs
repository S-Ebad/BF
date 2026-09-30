use crate::{
    cli::Emit, codegen::generate, errors::render_error, lexer::tokenize, resolver::resolve_jumps,
};

use anyhow::{Context, bail};
use clap::Parser;

use std::process::Command;

mod cli;
mod codegen;
mod errors;
mod lexer;
mod resolver;

fn run(cmd: &mut Command) -> anyhow::Result<()> {
    let program = cmd.get_program().to_string_lossy().into_owned();

    let status = cmd.status().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("`{program}` not found; is it installed and on your PATH?")
        } else {
            anyhow::Error::new(err).context(format!("couldn't run `{program}`"))
        }
    })?;

    if !status.success() {
        bail!("`{program}` failed ({status})");
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let config = cli::Config::parse();
    let src = std::fs::read_to_string(&config.input)
        .with_context(|| format!("couldn't read `{}`", config.input.display()))?;

    let mut tokens = tokenize(&src);

    if let Err(mut errors) = resolve_jumps(&mut tokens) {
        errors.sort_by_key(|err| err.span());

        for err in errors {
            render_error(&src, &err);
            eprintln!();
        }

        std::process::exit(1);
    }

    let asm = generate(&tokens);
    let output = config.output.unwrap_or_else(|| match config.emit {
        Emit::Asm => "a.asm".into(),
        Emit::Obj => "a.o".into(),
        Emit::Exe => "a.out".into(),
    });

    match config.emit {
        Emit::Asm => std::fs::write(&output, &asm)
            .with_context(|| format!("couldn't write `{}`", output.display()))?,

        Emit::Obj | Emit::Exe => {
            let tmp = tempfile::tempdir().context("couldn't create temp directory")?;
            let asm_path = tmp.path().join("out.asm");

            std::fs::write(&asm_path, &asm).context("couldn't write temporary assembly file")?;

            let obj_path = match config.emit {
                Emit::Obj => output.clone(),
                _ => tmp.path().join("out.o"),
            };

            run(Command::new("nasm")
                .args(["-f", "elf64", "-o"])
                .arg(&obj_path)
                .arg(&asm_path))?;

            if let Emit::Exe = config.emit {
                run(Command::new("ld").arg("-o").arg(&output).arg(&obj_path))?;
            }
        }
    }

    Ok(())
}
