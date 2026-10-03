use crate::{
    codegen::TAPE_LEN,
    lexer::{Token, TokenKind},
};

/// How many steps -O3 runs a program for at compile time before giving up, unless
/// `--step-limit` says otherwise. Even a program that never finishes only adds a
/// few milliseconds of compile time.
///
/// A step is one token, or one cell moved by a `Scan`.
pub const DEFAULT_STEP_LIMIT: u64 = 1_000_000;

/// The state a program is in after running part of it at compile time.
pub struct Evaluation {
    /// Everything the program printed.
    pub output: Vec<u8>,
    /// Where the compiled program has to carry on, or `None` if the program finished.
    pub resume: Option<Resume>,
}

pub struct Resume {
    /// The token to run next.
    pub index: usize,
    /// The pointer, as a cell index. It can be off the tape if the program moved
    /// there without touching a cell yet.
    pub pointer: isize,
    pub tape: Vec<u8>,
}

impl Evaluation {
    /// The state before anything ran, which is where compiling without -O3 starts.
    pub fn start() -> Self {
        Self {
            output: Vec::new(),
            resume: Some(Resume {
                index: 0,
                pointer: 0,
                tape: vec![0; TAPE_LEN],
            }),
        }
    }
}

/// -O3: runs the program at compile time for up to `step_limit` steps.
///
/// Stops early at the first `,` (input isn't known yet), and before any op that
/// would touch a cell off the tape (that's undefined, so it's left to the compiled
/// program). If the program finishes, all that's left of it is its output.
pub fn evaluate(tokens: &[Token], step_limit: u64) -> Evaluation {
    use TokenKind::*;

    let partner = match_loops(tokens);
    let mut tape = vec![0u8; TAPE_LEN];
    let mut pointer: isize = 0;
    let mut output = Vec::new();
    let mut index = 0;
    let mut steps = 0;

    // Index on the tape of the cell at `offset` from the pointer, if it's on the tape.
    let at = |pointer: isize, offset: isize| -> Option<usize> {
        usize::try_from(pointer + offset)
            .ok()
            .filter(|&i| i < TAPE_LEN)
    };

    'run: while index < tokens.len() {
        if steps >= step_limit {
            break;
        }
        steps += 1;

        match *tokens[index].kind() {
            Add(n) => {
                let Some(i) = at(pointer, 0) else { break };
                tape[i] = tape[i].wrapping_add(n);
            }
            AddAt(offset, n) => {
                let Some(i) = at(pointer, offset) else { break };
                tape[i] = tape[i].wrapping_add(n);
            }
            Move(n) => pointer += n,
            Set(offset, n) => {
                let Some(i) = at(pointer, offset) else { break };
                tape[i] = n;
            }
            MulAt(from, to, factor) => {
                let Some(from) = at(pointer, from) else { break };
                // With a 0 counter it adds nothing, wherever the target is. The loop
                // it came from wouldn't have run, so the target can even be off the tape.
                if tape[from] != 0 {
                    let Some(to) = at(pointer, to) else { break };
                    tape[to] = tape[to].wrapping_add(tape[from].wrapping_mul(factor));
                }
            }
            // Stopping partway through a scan is fine: resuming it carries on from
            // wherever the pointer got to.
            Scan(step) => loop {
                let Some(i) = at(pointer, 0) else { break 'run };
                if tape[i] == 0 {
                    break;
                }
                if steps >= step_limit {
                    break 'run;
                }
                steps += 1;
                pointer += step;
            },
            Output(offset) => {
                let Some(i) = at(pointer, offset) else { break };
                output.push(tape[i]);
            }
            Input(_) => break,
            // Jumping to the partner and then stepping past it lands after `]` for
            // `[`, and at the start of the body for `]`.
            JmpZ(_, offset) => {
                let Some(i) = at(pointer, offset) else { break };
                if tape[i] == 0 {
                    index = partner[index];
                }
            }
            JmpNZ(_, offset) => {
                let Some(i) = at(pointer, offset) else { break };
                if tape[i] != 0 {
                    index = partner[index];
                }
            }
        }

        index += 1;
    }

    let resume = (index < tokens.len()).then_some(Resume {
        index,
        pointer,
        tape,
    });

    Evaluation { output, resume }
}

/// For each bracket, the index of its partner. Other tokens map to 0.
fn match_loops(tokens: &[Token]) -> Vec<usize> {
    let mut partner = vec![0; tokens.len()];
    let mut open = Vec::new();

    for (i, token) in tokens.iter().enumerate() {
        match token.kind() {
            TokenKind::JmpZ(..) => open.push(i),
            TokenKind::JmpNZ(..) => {
                let j = open.pop().expect("jumps are resolved");
                partner[i] = j;
                partner[j] = i;
            }
            _ => (),
        }
    }

    partner
}

#[cfg(test)]
mod tests;
