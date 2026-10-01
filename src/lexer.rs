use crate::errors::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Add to the current cell, wrapping (`-` is `Add(255)`).
    Add(u8),
    /// Move the pointer by this many cells (negative is left).
    Move(isize),
    Output,
    Input,
    JmpZ(usize),
    JmpNZ(usize),
}

#[derive(Debug)]
pub struct Token {
    kind: TokenKind,
    span: Span,
}

impl Token {
    fn new(kind: TokenKind, span: Span) -> Self {
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
            '.' => Some(Self::Output),
            ',' => Some(Self::Input),
            '[' => Some(Self::JmpZ(0)),
            ']' => Some(Self::JmpNZ(0)),
            _ => None,
        }
    }
}

pub fn tokenize(expr: &str) -> Vec<Token> {
    expr.char_indices()
        .filter_map(|(i, c)| TokenKind::from_char(c).map(|k| Token::new(k, Span::new(i))))
        .collect()
}
