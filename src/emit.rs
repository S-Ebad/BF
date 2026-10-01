use std::process::Command;

use anyhow::Context;

use crate::cli::{self};

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
        anyhow::bail!("`{program}` failed ({status})");
    }

    Ok(())
}

pub fn write_output(asm: &str, config: &cli::Config) -> anyhow::Result<()> {
    let output = config.output();

    match config.emit {
        cli::Emit::Asm => std::fs::write(&output, asm)
            .with_context(|| format!("couldn't write `{}`", output.display())),

        cli::Emit::Obj | cli::Emit::Exe => {
            let tmp = tempfile::tempdir().context("couldn't create temp directory")?;
            let asm_path = tmp.path().join("out.asm");

            std::fs::write(&asm_path, asm).context("couldn't write temporary assembly file")?;

            let obj_path = match config.emit {
                cli::Emit::Obj => output.clone(),
                _ => tmp.path().join("out.o"),
            };

            run(Command::new("nasm")
                .args(["-f", "elf64", "-o"])
                .arg(&obj_path)
                .arg(&asm_path)
                .arg("-O1"))?;

            if let cli::Emit::Exe = config.emit {
                run(Command::new("ld").arg("-o").arg(&output).arg(&obj_path))?;
            }

            Ok(())
        }
    }
}
