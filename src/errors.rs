use std::fmt;

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct Span(usize);

pub enum BFError {
    UnmatchedOpenBracket(Span),
    UnmatchedCloseBracket(Span),
}

impl Span {
    pub fn new(pos: usize) -> Self {
        Self(pos)
    }
}

impl BFError {
    pub fn span(&self) -> Span {
        match self {
            BFError::UnmatchedOpenBracket(span) | BFError::UnmatchedCloseBracket(span) => *span,
        }
    }
}

impl fmt::Display for BFError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnmatchedOpenBracket(_) => write!(f, "unmatched `[`"),
            Self::UnmatchedCloseBracket(_) => write!(f, "unmatched `]`"),
        }
    }
}

pub fn render_error(source: &str, error: &BFError) {
    const MAX_WIDTH: usize = 60;

    let span = match error {
        BFError::UnmatchedOpenBracket(span) | BFError::UnmatchedCloseBracket(span) => span,
    };

    let pos = span.0;

    let line = source[..pos].bytes().filter(|&b| b == b'\n').count() + 1;
    let line_start = source[..pos].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[pos..].find('\n').map_or(source.len(), |i| pos + i);

    let line_text = &source[line_start..line_end];
    let column = source[line_start..pos].chars().count() + 1;

    let chars: Vec<char> = line_text.chars().collect();
    let col0 = column - 1;
    let (start, end) = if chars.len() <= MAX_WIDTH {
        (0, chars.len())
    } else {
        let start = col0
            .saturating_sub(MAX_WIDTH / 2)
            .min(chars.len() - MAX_WIDTH);
        (start, start + MAX_WIDTH)
    };

    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < chars.len() { "..." } else { "" };
    let snippet: String = chars[start..end].iter().collect();
    let caret_pad = prefix.len() + (col0 - start);

    let width = line.to_string().len();
    let gutter = " ".repeat(width);

    eprintln!("error: {error} at line {line}, column {column}");
    eprintln!("{gutter} |");
    eprintln!("{line:>width$} | {prefix}{snippet}{suffix}");
    eprintln!("{gutter} | {}^", " ".repeat(caret_pad));
}
