use std::fmt;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct Span(usize);

#[derive(Debug, PartialEq, Eq)]
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

const TAB_WIDTH: usize = 4;

/// Terminal columns a char takes up in the snippet (tabs are expanded to spaces).
fn display_width(c: char) -> usize {
    match c {
        '\t' => TAB_WIDTH,
        c => c.width().unwrap_or(0),
    }
}

const MAX_WIDTH: usize = 60;

/// The part of a source line shown under an error, with tabs already expanded.
#[derive(Debug)]
struct Snippet {
    line: usize,
    column: usize,
    text: String,
    /// Display column of the caret within `text`.
    caret: usize,
    cut_start: bool,
    cut_end: bool,
}

fn snippet(source: &str, span: Span) -> Snippet {
    let pos = span.0;

    let line = source[..pos].bytes().filter(|&b| b == b'\n').count() + 1;
    let line_start = source[..pos].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[pos..].find('\n').map_or(source.len(), |i| pos + i);

    let line_text = &source[line_start..line_end];
    let column = source[line_start..pos].chars().count() + 1;

    let chars: Vec<char> = line_text.chars().collect();
    let col0 = column - 1;
    let widths: Vec<usize> = chars.iter().map(|&c| display_width(c)).collect();

    // Pick a window of at most MAX_WIDTH columns, centred on the error when possible.
    let (start, end) = if widths.iter().sum::<usize>() <= MAX_WIDTH {
        (0, chars.len())
    } else {
        let mut start = col0;
        let mut used = 0;
        while start > 0 && used + widths[start - 1] <= MAX_WIDTH / 2 {
            start -= 1;
            used += widths[start];
        }

        let mut end = start;
        let mut used = 0;
        while end < chars.len() && used + widths[end] <= MAX_WIDTH {
            used += widths[end];
            end += 1;
        }

        // Near the end of the line: spend the leftover columns on the left.
        while start > 0 && used + widths[start - 1] <= MAX_WIDTH {
            start -= 1;
            used += widths[start];
        }

        (start, end)
    };

    let text = chars[start..end]
        .iter()
        .map(|&c| match c {
            '\t' => " ".repeat(TAB_WIDTH),
            c => c.to_string(),
        })
        .collect();

    Snippet {
        line,
        column,
        text,
        caret: widths[start..col0].iter().sum(),
        cut_start: start > 0,
        cut_end: end < chars.len(),
    }
}

fn render_error(source: &str, error: &BFError) {
    let Snippet {
        line,
        column,
        text,
        caret,
        cut_start,
        cut_end,
    } = snippet(source, error.span());

    let prefix = if cut_start { "..." } else { "" };
    let suffix = if cut_end { "..." } else { "" };
    let caret_pad = prefix.len() + caret;

    let width = line.to_string().len();
    let gutter = " ".repeat(width);

    eprintln!("error: {error} at line {line}, column {column}");
    eprintln!("{gutter} |");
    eprintln!("{line:>width$} | {prefix}{text}{suffix}");
    eprintln!("{gutter} | {}^", " ".repeat(caret_pad));
}

pub fn report(source: &str, errors: Vec<BFError>) {
    for err in errors {
        render_error(source, &err);
        eprintln!();
    }
}

#[cfg(test)]
mod tests;
