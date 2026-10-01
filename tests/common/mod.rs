#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

pub const TAPE_LEN: usize = 30000;

/// Every optimization level; program tests run against all of them.
pub const OPT_LEVELS: [u8; 4] = [0, 1, 2, 3];

/// Reference interpreter: 8-bit wrapping cells, 30000-cell tape, EOF sets the cell to 0.
///
/// Returns `None` if the program leaves the tape (UB) or runs longer than `step_limit`.
pub fn interpret(src: &str, input: &[u8], step_limit: u64) -> Option<Vec<u8>> {
    let code: Vec<u8> = src.bytes().filter(|b| b"+-<>.,[]".contains(b)).collect();

    let mut jump = vec![0; code.len()];
    let mut open = Vec::new();
    for (i, &c) in code.iter().enumerate() {
        match c {
            b'[' => open.push(i),
            b']' => {
                let j = open.pop().expect("unbalanced program");
                jump[i] = j;
                jump[j] = i;
            }
            _ => (),
        }
    }
    assert!(open.is_empty(), "unbalanced program");

    let mut tape = vec![0u8; TAPE_LEN];
    let mut input = input.iter();
    let mut out = Vec::new();
    let (mut ptr, mut ip, mut steps) = (0usize, 0usize, 0u64);

    while ip < code.len() {
        steps += 1;
        if steps > step_limit {
            return None;
        }

        match code[ip] {
            b'+' => tape[ptr] = tape[ptr].wrapping_add(1),
            b'-' => tape[ptr] = tape[ptr].wrapping_sub(1),
            b'>' => {
                ptr += 1;
                if ptr == TAPE_LEN {
                    return None;
                }
            }
            b'<' => ptr = ptr.checked_sub(1)?,
            b'.' => out.push(tape[ptr]),
            b',' => {
                tape[ptr] = input.next().copied().unwrap_or(0);
            }
            b'[' if tape[ptr] == 0 => ip = jump[ip],
            b']' if tape[ptr] != 0 => ip = jump[ip],
            _ => (),
        }
        ip += 1;
    }

    Some(out)
}

/// Deterministic PRNG (splitmix64) so fuzz failures are reproducible.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `lo..=hi`.
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next() % (hi - lo + 1) as u64) as usize
    }

    pub fn chance(&mut self, p: f64) -> bool {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }

    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

/// A temp directory holding a source file, for running the compiler on.
pub struct Workspace {
    pub dir: TempDir,
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn write(&self, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// Runs the compiler with the workspace as the working directory.
    pub fn brainfk<I, S>(&self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        Command::new(env!("CARGO_BIN_EXE_brainfk"))
            .args(args)
            .current_dir(self.dir.path())
            .output()
            .unwrap()
    }
}

/// Compiles `src` to an executable at the given optimization level.
///
/// Panics with the compiler's stderr if compilation fails.
pub fn build(ws: &Workspace, src: &str, opt: u8) -> PathBuf {
    ws.write("prog.bf", src);
    let exe = ws.path(&format!("prog-O{opt}"));

    let out = ws.brainfk([
        "prog.bf".as_ref(),
        "-o".as_ref(),
        exe.as_os_str(),
        format!("-O{opt}").as_ref(),
    ]);
    assert!(
        out.status.success(),
        "brainfk failed (is nasm installed?):\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    exe
}

/// Runs an executable with the given stdin; stdin is fed from a thread so large
/// input and output can't deadlock.
pub fn run(exe: &Path, input: &[u8]) -> Output {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    let feeder = std::thread::spawn(move || {
        // The program may exit without reading everything.
        let _ = stdin.write_all(&input);
    });

    let out = child.wait_with_output().unwrap();
    feeder.join().unwrap();
    out
}

/// Compiles and runs `src` at every optimization level, asserting that each
/// exits cleanly and prints `expected`.
pub fn assert_output(src: &str, input: &[u8], expected: &[u8]) {
    for opt in OPT_LEVELS {
        let ws = Workspace::new();
        let out = run(&build(&ws, src, opt), input);

        assert!(
            out.status.success(),
            "-O{opt}: program exited with {}",
            out.status
        );
        assert!(
            out.stdout == expected,
            "-O{opt}: output mismatch\n  expected ({} bytes): {:?}\n  got      ({} bytes): {:?}",
            expected.len(),
            String::from_utf8_lossy(&expected[..expected.len().min(200)]),
            out.stdout.len(),
            String::from_utf8_lossy(&out.stdout[..out.stdout.len().min(200)]),
        );
    }
}

/// Like `assert_output`, with the expected output taken from the reference interpreter.
pub fn assert_matches_interpreter(src: &str, input: &[u8]) {
    let expected = interpret(src, input, u64::MAX).expect("program leaves the tape");
    assert_output(src, input, &expected);
}
