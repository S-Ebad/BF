use crate::errors::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Add to the current cell, wrapping (`-` is `Add(255)`).
    Add(u8),
    /// Move the pointer by this many cells (negative is left).
    Move(isize),
    /// Add to the cell at this offset from the pointer, without moving it (only made by -O2).
    AddAt(isize, u8),
    /// Set the cell at this offset to this value (only made by -O2). `[-]` is `Set(0, 0)`.
    Set(isize, u8),
    /// `MulAt(from, to, factor)`: add the cell at `from` times `factor` to the cell at
    /// `to`, both offsets from the pointer (only made by -O2).
    ///
    /// Multiply loops like `[->++<]` become a `MulAt` per target, then a `Set` of the counter.
    MulAt(isize, isize, u8),
    /// Move the pointer by this step until it lands on a zero cell (only made by -O2).
    ///
    /// `[>]` is `Scan(1)`, `[<<]` is `Scan(-2)`.
    Scan(isize),
    /// `Check(min, max)`, only with `--bounds abort`: the source as written reads or
    /// writes cells from offset `min` to `max` (and none further out) before the
    /// next loop bracket or I/O. Aborts if either end is off the tape.
    ///
    /// Made before optimizing and never dropped by it, so a program that touches a
    /// cell off the tape aborts at every level, even where the optimizer removed
    /// the access itself (`<+-`).
    Check(isize, isize),
    /// Print the cell at this offset (`.` is `Output(0)`).
    Output(isize),
    /// Read a byte into the cell at this offset (`,` is `Input(0)`).
    Input(isize),
    /// `JmpZ(id, offset)`: `[`, checking the cell at this offset.
    ///
    /// The offset is only non-zero for loops whose body doesn't move the pointer
    /// overall, which -O2 runs without moving the pointer to them.
    JmpZ(usize, isize),
    /// `JmpNZ(id, offset)`: `]`, checking the cell at this offset.
    JmpNZ(usize, isize),
}

#[derive(Debug)]
pub struct Token {
    kind: TokenKind,
    span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn kind(&self) -> &TokenKind {
        &self.kind
    }

    pub fn kind_mut(&mut self) -> &mut TokenKind {
        &mut self.kind
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

impl TokenKind {
    fn from_char(c: char) -> Option<Self> {
        match c {
            '+' => Some(Self::Add(1)),
            '-' => Some(Self::Add(u8::MAX)),
            '>' => Some(Self::Move(1)),
            '<' => Some(Self::Move(-1)),
            '.' => Some(Self::Output(0)),
            ',' => Some(Self::Input(0)),
            '[' => Some(Self::JmpZ(0, 0)),
            ']' => Some(Self::JmpNZ(0, 0)),
            _ => None,
        }
    }
}

pub fn tokenize(expr: &str) -> Vec<Token> {
    expr.char_indices()
        .filter_map(|(i, c)| TokenKind::from_char(c).map(|k| Token::new(k, Span::new(i))))
        .collect()
}
