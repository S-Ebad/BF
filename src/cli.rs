use crate::{
    Options,
    codegen::{DEFAULT_TAPE_SIZE, Tape},
    evaluator::DEFAULT_STEP_LIMIT,
};
use anyhow::Context;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Brainfuck to x86-64 compiler")]
pub struct Config {
    /// Brainfuck source file
    pub input: PathBuf,

    /// Output file
    #[arg(short)]
    pub output: Option<PathBuf>,

    /// What to produce
    #[arg(long, value_enum, default_value_t = Emit::Exe)]
    pub emit: Emit,

    /// Optimization level: 0 = none, 1 = collapse runs, 2 = pattern opcodes,
    /// 3 = also run the program at compile time
    #[arg(short = 'O', default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=3))]
    pub opt: u8,

    /// How many steps -O3 runs the program for at compile time before giving up
    #[arg(short, long, default_value_t = DEFAULT_STEP_LIMIT)]
    pub step_limit: u64,

    /// Number of cells on the tape
    #[arg(long, default_value_t = DEFAULT_TAPE_SIZE as u64,
          value_parser = clap::value_parser!(u64).range(1..=i32::MAX as u64))]
    pub tape_size: u64,

    /// What happens when the program touches a cell outside the tape
    #[arg(long, value_enum, default_value_t = Bounds::Undefined)]
    pub bounds: Bounds,
}

/// What happens when the program touches a cell outside the tape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Bounds {
    /// Not checked: it reads or writes whatever memory is there (fastest)
    Undefined,
    /// Stop with an error message and exit code 1
    Abort,
    /// The tape is circular: past the last cell is the first one, and the other way round
    Wrap,
}

impl Config {
    /// The output path: -o if given, otherwise the default for --emit.
    pub fn output(&self) -> PathBuf {
        self.output
            .clone()
            .unwrap_or_else(|| self.emit.default_output().into())
    }

    /// The settings `compile` needs.
    pub fn options(&self) -> Options {
        Options {
            opt: self.opt,
            step_limit: self.step_limit,
            tape: Tape {
                size: self.tape_size as usize,
                bounds: self.bounds,
            },
        }
    }

    /// Reads the input file, with the path in the error.
    pub fn read_source(&self) -> anyhow::Result<String> {
        std::fs::read_to_string(&self.input)
            .with_context(|| format!("couldn't read `{}`", self.input.display()))
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Emit {
    Asm,
    Obj,
    Exe,
}

impl Emit {
    /// The file name used when -o isn't given.
    pub fn default_output(self) -> &'static str {
        match self {
            Emit::Asm => "a.asm",
            Emit::Obj => "a.o",
            Emit::Exe => "a.out",
        }
    }
}
