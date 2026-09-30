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

    /// Optimization level: 0 = none, 1 = collapse runs, 2 = pattern opcodes
    #[arg(short = 'O', default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=2))]
    pub opt: u8,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Emit {
    Asm,
    Obj,
    Exe,
}
