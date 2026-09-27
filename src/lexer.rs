#[derive(Debug)]
pub enum TokenKind {
    Add,
    Sub,
    RMove,
    LMove,
    Output, // output
    Input,  // Input
    JmpZ,
    JmpNZ,
}

#[derive(Debug)]
pub struct Span(usize);

#[derive(Debug)]
pub struct Token {
    kind: TokenKind,
    span: Span,
}

#[derive(Debug)]
pub struct Lexer {
    tokens: Vec<Token>,
}

impl Span {
    fn new(pos: usize) -> Self {
        Self(pos)
    }
}

impl Token {
    fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

impl TokenKind {
    fn new(c: char) -> Option<Self> {
        match c {
            '+' => Some(Self::Add),
            '-' => Some(Self::Sub),
            '>' => Some(Self::RMove),
            '<' => Some(Self::LMove),
            '.' => Some(Self::Output),
            ',' => Some(Self::Input),
            '[' => Some(Self::JmpZ),
            ']' => Some(Self::JmpNZ),
            _ => None,
        }
    }
}

impl Lexer {
    pub fn tokenize(expr: &str) -> Lexer {
        let mut tokens: Vec<Token> = vec![];

        for (size, c) in expr.char_indices() {
            if let Some(kind) = TokenKind::new(c) {
                let span = Span::new(size);

                let token = Token::new(kind, span);
                tokens.push(token);
            }
        }

        Lexer { tokens }
    }
}
