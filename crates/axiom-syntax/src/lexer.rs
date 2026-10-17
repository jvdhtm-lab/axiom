//! Lexer: source text -> tokens with byte spans.
//!
//! The lexer never panics. It records diagnostics for invalid characters and
//! malformed strings, and keeps scanning. Line breaks are emitted as explicit
//! [`TokenKind::Newline`] tokens (consecutive breaks collapse) because a
//! statement or declaration is terminated by `;` or a line break.

use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    String(String),
    Integer(i64),
    Float(f64),
    Duration { millis: u64, unit: String },
    Size { bytes: u64, unit: String },
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Colon,
    Comma,
    Dot,
    Semicolon,
    Equals,
    EqEq,
    BangEq,
    Arrow,
    FatArrow,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    AmpAmp,
    PipePipe,
    Pipe,
    Question,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    Newline,
    Eof,
}

impl TokenKind {
    /// A human-readable description for diagnostics.
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Ident(name) => format!("identifier `{name}`"),
            TokenKind::String(_) => "string literal".to_string(),
            TokenKind::Integer(_) | TokenKind::Float(_) => "number".to_string(),
            TokenKind::Duration { .. } => "duration".to_string(),
            TokenKind::Size { .. } => "size".to_string(),
            TokenKind::LBrace => "`{`".to_string(),
            TokenKind::RBrace => "`}`".to_string(),
            TokenKind::LParen => "`(`".to_string(),
            TokenKind::RParen => "`)`".to_string(),
            TokenKind::LBracket => "`[`".to_string(),
            TokenKind::RBracket => "`]`".to_string(),
            TokenKind::Lt => "`<`".to_string(),
            TokenKind::Gt => "`>`".to_string(),
            TokenKind::LtEq => "`<=`".to_string(),
            TokenKind::GtEq => "`>=`".to_string(),
            TokenKind::Colon => "`:`".to_string(),
            TokenKind::Comma => "`,`".to_string(),
            TokenKind::Dot => "`.`".to_string(),
            TokenKind::Semicolon => "`;`".to_string(),
            TokenKind::Equals => "`=`".to_string(),
            TokenKind::EqEq => "`==`".to_string(),
            TokenKind::BangEq => "`!=`".to_string(),
            TokenKind::Arrow => "`->`".to_string(),
            TokenKind::FatArrow => "`=>`".to_string(),
            TokenKind::Plus => "`+`".to_string(),
            TokenKind::Minus => "`-`".to_string(),
            TokenKind::Star => "`*`".to_string(),
            TokenKind::Slash => "`/`".to_string(),
            TokenKind::Percent => "`%`".to_string(),
            TokenKind::Bang => "`!`".to_string(),
            TokenKind::AmpAmp => "`&&`".to_string(),
            TokenKind::PipePipe => "`||`".to_string(),
            TokenKind::Pipe => "`|`".to_string(),
            TokenKind::Question => "`?`".to_string(),
            TokenKind::PlusEq => "`+=`".to_string(),
            TokenKind::MinusEq => "`-=`".to_string(),
            TokenKind::StarEq => "`*=`".to_string(),
            TokenKind::SlashEq => "`/=`".to_string(),
            TokenKind::Newline => "line break".to_string(),
            TokenKind::Eof => "end of file".to_string(),
        }
    }

    pub fn is_ident(&self, text: &str) -> bool {
        matches!(self, TokenKind::Ident(name) if name == text)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn describe(&self) -> String {
        self.kind.describe()
    }
}

struct Scanner<'a> {
    src: &'a str,
    chars: Vec<(usize, char)>,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(src: &'a str) -> Self {
        Scanner {
            src,
            chars: src.char_indices().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).map(|&(_, c)| c)
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).map(|&(_, c)| c)
    }

    fn offset(&self) -> u32 {
        self.chars
            .get(self.pos)
            .map(|&(i, _)| i as u32)
            .unwrap_or(self.src.len() as u32)
    }

    fn bump(&mut self) -> Option<(u32, char)> {
        let entry = self.chars.get(self.pos).copied();
        if entry.is_some() {
            self.pos += 1;
        }
        entry.map(|(i, c)| (i as u32, c))
    }

    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }
}

/// Tokenize `src`. Returns the tokens (always ending with `Eof`) and any
/// diagnostics gathered along the way.
pub fn tokenize(src: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut scanner = Scanner::new(src);
    let mut tokens: Vec<Token> = Vec::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    loop {
        skip_trivia(&mut scanner, &mut tokens, &mut diagnostics);
        if scanner.at_end() {
            tokens.push(Token {
                kind: TokenKind::Eof,
                span: Span::empty(scanner.offset()),
            });
            break;
        }
        let start = scanner.offset();
        let c = scanner.peek().unwrap();
        match c {
            c if is_ident_start(c) => {
                let name = scan_ident(&mut scanner);
                tokens.push(Token {
                    kind: TokenKind::Ident(name),
                    span: Span::new(start, scanner.offset()),
                });
            }
            '"' => {
                let value = scan_string(&mut scanner, &mut diagnostics);
                tokens.push(Token {
                    kind: TokenKind::String(value),
                    span: Span::new(start, scanner.offset()),
                });
            }
            c if c.is_ascii_digit() => {
                scan_number(&mut scanner, start, &mut tokens, &mut diagnostics);
            }
            _ => {
                if let Some(kind) = scan_punct(&mut scanner) {
                    tokens.push(Token {
                        kind,
                        span: Span::new(start, scanner.offset()),
                    });
                } else {
                    scanner.bump();
                    diagnostics.push(Diagnostic::error(
                        Span::new(start, scanner.offset()),
                        format!("unexpected character `{c}`"),
                    ));
                }
            }
        }
    }

    (tokens, diagnostics)
}

fn skip_trivia(scanner: &mut Scanner, tokens: &mut Vec<Token>, diagnostics: &mut Vec<Diagnostic>) {
    loop {
        match scanner.peek() {
            Some(' ') | Some('\t') | Some('\r') => {
                scanner.bump();
            }
            Some('\n') => {
                let start = scanner.offset();
                scanner.bump();
                let span = Span::new(start, scanner.offset());
                if !matches!(tokens.last().map(|t| &t.kind), Some(TokenKind::Newline)) {
                    tokens.push(Token {
                        kind: TokenKind::Newline,
                        span,
                    });
                }
            }
            Some('/') if scanner.peek_at(1) == Some('/') => {
                while let Some(c) = scanner.peek() {
                    if c == '\n' {
                        break;
                    }
                    scanner.bump();
                }
            }
            Some('/') if scanner.peek_at(1) == Some('*') => {
                let start = scanner.offset();
                scanner.bump();
                scanner.bump();
                let mut saw_newline = false;
                let mut closed = false;
                while let Some(c) = scanner.peek() {
                    if c == '\n' {
                        saw_newline = true;
                    }
                    if c == '*' && scanner.peek_at(1) == Some('/') {
                        scanner.bump();
                        scanner.bump();
                        closed = true;
                        break;
                    }
                    scanner.bump();
                }
                if !closed {
                    diagnostics.push(Diagnostic::error(
                        Span::new(start, scanner.offset()),
                        "unterminated block comment".to_string(),
                    ));
                } else if saw_newline {
                    let span = Span::new(start, scanner.offset());
                    if !matches!(tokens.last().map(|t| &t.kind), Some(TokenKind::Newline)) {
                        tokens.push(Token {
                            kind: TokenKind::Newline,
                            span,
                        });
                    }
                }
            }
            _ => break,
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

fn scan_ident(scanner: &mut Scanner) -> String {
    let mut name = String::new();
    while let Some(c) = scanner.peek() {
        if is_ident_continue(c) {
            name.push(c);
            scanner.bump();
        } else {
            break;
        }
    }
    name
}

fn scan_string(scanner: &mut Scanner, diagnostics: &mut Vec<Diagnostic>) -> String {
    let start = scanner.offset();
    scanner.bump(); // opening quote
    let mut value = String::new();
    loop {
        match scanner.bump() {
            None => {
                diagnostics.push(Diagnostic::error(
                    Span::new(start, scanner.offset()),
                    "unterminated string literal".to_string(),
                ));
                break;
            }
            Some((_, '"')) => break,
            Some((_, '\\')) => match scanner.bump() {
                Some((_, '"')) => value.push('"'),
                Some((_, '\\')) => value.push('\\'),
                Some((_, 'n')) => value.push('\n'),
                Some((_, 'r')) => value.push('\r'),
                Some((_, 't')) => value.push('\t'),
                Some((esc_start, other)) => {
                    diagnostics.push(Diagnostic::error(
                        Span::new(
                            esc_start.saturating_sub(1),
                            esc_start + other.len_utf8() as u32,
                        ),
                        format!("unknown escape sequence `\\{other}`"),
                    ));
                    value.push(other);
                }
                None => {
                    diagnostics.push(Diagnostic::error(
                        Span::new(start, scanner.offset()),
                        "unterminated string literal".to_string(),
                    ));
                    break;
                }
            },
            Some((_, c)) => value.push(c),
        }
    }
    value
}

fn scan_number(
    scanner: &mut Scanner,
    start: u32,
    tokens: &mut Vec<Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut text = String::new();
    while let Some(c) = scanner.peek() {
        if c.is_ascii_digit() {
            text.push(c);
            scanner.bump();
        } else {
            break;
        }
    }

    let is_float =
        scanner.peek() == Some('.') && scanner.peek_at(1).is_some_and(|c| c.is_ascii_digit());
    if is_float {
        text.push('.');
        scanner.bump();
        while let Some(c) = scanner.peek() {
            if c.is_ascii_digit() {
                text.push(c);
                scanner.bump();
            } else {
                break;
            }
        }
        match text.parse::<f64>() {
            Ok(value) => tokens.push(Token {
                kind: TokenKind::Float(value),
                span: Span::new(start, scanner.offset()),
            }),
            Err(_) => diagnostics.push(Diagnostic::error(
                Span::new(start, scanner.offset()),
                format!("invalid number `{text}`"),
            )),
        }
        return;
    }

    if let Some((bytes, unit)) = try_size_unit(scanner, &text) {
        tokens.push(Token {
            kind: TokenKind::Size { bytes, unit },
            span: Span::new(start, scanner.offset()),
        });
        return;
    }

    if let Some((millis, unit)) = try_duration_unit(scanner, &text) {
        tokens.push(Token {
            kind: TokenKind::Duration { millis, unit },
            span: Span::new(start, scanner.offset()),
        });
        return;
    }

    match text.parse::<i64>() {
        Ok(value) => tokens.push(Token {
            kind: TokenKind::Integer(value),
            span: Span::new(start, scanner.offset()),
        }),
        Err(_) => diagnostics.push(Diagnostic::error(
            Span::new(start, scanner.offset()),
            format!("integer literal out of range: `{text}`"),
        )),
    }
}

/// Recognize byte-size units (`B`, `kB`/`KB`, `MB`, `GB`, `TB`, `KiB`, `MiB`,
/// `GiB`, `TiB`) immediately following an integer.
fn try_size_unit(scanner: &mut Scanner, digits: &str) -> Option<(u64, String)> {
    let rest: Vec<char> = (0..4).filter_map(|i| scanner.peek_at(i)).collect();
    let (unit, len): (&str, usize) = match rest.as_slice() {
        ['K', 'i', 'B', ..] => ("KiB", 3),
        ['M', 'i', 'B', ..] => ("MiB", 3),
        ['G', 'i', 'B', ..] => ("GiB", 3),
        ['T', 'i', 'B', ..] => ("TiB", 3),
        ['k', 'B', ..] => ("kB", 2),
        ['K', 'B', ..] => ("KB", 2),
        ['M', 'B', ..] => ("MB", 2),
        ['G', 'B', ..] => ("GB", 2),
        ['T', 'B', ..] => ("TB", 2),
        ['B', next, ..] if !next.is_ascii_alphanumeric() && *next != '_' => ("B", 1),
        _ => return None,
    };
    let value: u64 = digits.parse().ok()?;
    let factor: u64 = match unit {
        "B" => 1,
        "kB" | "KB" => 1_000,
        "MB" => 1_000_000,
        "GB" => 1_000_000_000,
        "TB" => 1_000_000_000_000,
        "KiB" => 1 << 10,
        "MiB" => 1 << 20,
        "GiB" => 1 << 30,
        "TiB" => 1 << 40,
        _ => return None,
    };
    for _ in 0..len {
        scanner.bump();
    }
    Some((value.saturating_mul(factor), unit.to_string()))
}

/// Recognize `ms`, `s`, `m`, `h` immediately following an integer, provided the
/// character after the unit cannot continue an identifier.
fn try_duration_unit(scanner: &mut Scanner, digits: &str) -> Option<(u64, String)> {
    let unit = match (scanner.peek(), scanner.peek_at(1)) {
        (Some('m'), Some('s')) => "ms",
        (Some('s'), next) if !next.is_some_and(is_ident_continue) => "s",
        (Some('m'), next) if !next.is_some_and(is_ident_continue) => "m",
        (Some('h'), next) if !next.is_some_and(is_ident_continue) => "h",
        _ => return None,
    };
    let value: u64 = digits.parse().ok()?;
    let factor = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        _ => return None,
    };
    // Consume the unit characters.
    for _ in 0..unit.chars().count() {
        scanner.bump();
    }
    Some((value * factor, unit.to_string()))
}

fn scan_punct(scanner: &mut Scanner) -> Option<TokenKind> {
    let c = scanner.peek()?;
    let next = scanner.peek_at(1);
    let two = match (c, next) {
        ('-', Some('>')) => Some(TokenKind::Arrow),
        ('=', Some('>')) => Some(TokenKind::FatArrow),
        ('=', Some('=')) => Some(TokenKind::EqEq),
        ('!', Some('=')) => Some(TokenKind::BangEq),
        ('<', Some('=')) => Some(TokenKind::LtEq),
        ('>', Some('=')) => Some(TokenKind::GtEq),
        ('&', Some('&')) => Some(TokenKind::AmpAmp),
        ('|', Some('|')) => Some(TokenKind::PipePipe),
        ('+', Some('=')) => Some(TokenKind::PlusEq),
        ('-', Some('=')) => Some(TokenKind::MinusEq),
        ('*', Some('=')) => Some(TokenKind::StarEq),
        ('/', Some('=')) => Some(TokenKind::SlashEq),
        _ => None,
    };
    if let Some(kind) = two {
        scanner.bump();
        scanner.bump();
        return Some(kind);
    }

    let kind = match c {
        '{' => TokenKind::LBrace,
        '}' => TokenKind::RBrace,
        '(' => TokenKind::LParen,
        ')' => TokenKind::RParen,
        '[' => TokenKind::LBracket,
        ']' => TokenKind::RBracket,
        '<' => TokenKind::Lt,
        '>' => TokenKind::Gt,
        ':' => TokenKind::Colon,
        ',' => TokenKind::Comma,
        '.' => TokenKind::Dot,
        ';' => TokenKind::Semicolon,
        '=' => TokenKind::Equals,
        '+' => TokenKind::Plus,
        '-' => TokenKind::Minus,
        '*' => TokenKind::Star,
        '/' => TokenKind::Slash,
        '%' => TokenKind::Percent,
        '!' => TokenKind::Bang,
        '|' => TokenKind::Pipe,
        '?' => TokenKind::Question,
        _ => return None,
    };
    scanner.bump();
    Some(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src).0.into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn collapses_consecutive_newlines() {
        assert_eq!(
            kinds("a\n\n\n  b"),
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Newline,
                TokenKind::Ident("b".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn recognizes_identifiers_keywords_and_paths() {
        assert_eq!(
            kinds("openai_compatible"),
            vec![TokenKind::Ident("openai_compatible".into()), TokenKind::Eof]
        );
    }

    #[test]
    fn numbers_floats_and_durations() {
        assert_eq!(
            kinds("42 0.2 120s 250ms 2m 1h"),
            vec![
                TokenKind::Integer(42),
                TokenKind::Float(0.2),
                TokenKind::Duration {
                    millis: 120_000,
                    unit: "s".into()
                },
                TokenKind::Duration {
                    millis: 250,
                    unit: "ms".into()
                },
                TokenKind::Duration {
                    millis: 120_000,
                    unit: "m".into()
                },
                TokenKind::Duration {
                    millis: 3_600_000,
                    unit: "h".into()
                },
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn a_number_followed_by_an_identifier_is_not_a_duration() {
        assert_eq!(
            kinds("2 xy"),
            vec![
                TokenKind::Integer(2),
                TokenKind::Ident("xy".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn size_literals_use_decimal_and_binary_units() {
        assert_eq!(
            kinds("2MB 2MiB 1KB 1KiB 500B 20GB"),
            vec![
                TokenKind::Size {
                    bytes: 2_000_000,
                    unit: "MB".into()
                },
                TokenKind::Size {
                    bytes: 2 * (1 << 20),
                    unit: "MiB".into()
                },
                TokenKind::Size {
                    bytes: 1_000,
                    unit: "KB".into()
                },
                TokenKind::Size {
                    bytes: 1 << 10,
                    unit: "KiB".into()
                },
                TokenKind::Size {
                    bytes: 500,
                    unit: "B".into()
                },
                TokenKind::Size {
                    bytes: 20_000_000_000,
                    unit: "GB".into()
                },
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn operators_are_lexed_longest_first() {
        assert_eq!(
            kinds("-> => == != <= >= && || | < > + - * / % ! ? = += +"),
            vec![
                TokenKind::Arrow,
                TokenKind::FatArrow,
                TokenKind::EqEq,
                TokenKind::BangEq,
                TokenKind::LtEq,
                TokenKind::GtEq,
                TokenKind::AmpAmp,
                TokenKind::PipePipe,
                TokenKind::Pipe,
                TokenKind::Lt,
                TokenKind::Gt,
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::Bang,
                TokenKind::Question,
                TokenKind::Equals,
                TokenKind::PlusEq,
                TokenKind::Plus,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn strings_support_escapes() {
        assert_eq!(
            kinds(r#""a\"b\n\t""#),
            vec![TokenKind::String("a\"b\n\t".into()), TokenKind::Eof]
        );
    }

    #[test]
    fn unterminated_string_is_diagnosed_not_panicked() {
        let (tokens, diagnostics) = tokenize("\"abc");
        assert!(diagnostics.iter().any(|d| d.is_error()));
        assert!(matches!(tokens.last().unwrap().kind, TokenKind::Eof));
    }

    #[test]
    fn bad_escape_is_diagnosed() {
        let (_, diagnostics) = tokenize(r#""a\qb""#);
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("unknown escape")));
    }

    #[test]
    fn comments_are_skipped_and_a_block_comment_breaks_entries() {
        let (tokens, diagnostics) = tokenize("a // trailing\n/* one\ntwo */ b");
        assert!(diagnostics.is_empty());
        let kinds: Vec<_> = tokens.into_iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Newline,
                TokenKind::Ident("b".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn unexpected_character_is_diagnosed() {
        let (_, diagnostics) = tokenize("agent @ A");
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("unexpected character")));
    }

    #[test]
    fn spans_point_at_the_right_bytes() {
        let (tokens, _) = tokenize("model xs");
        assert_eq!(tokens[0].span, Span::new(0, 5));
        assert_eq!(tokens[1].span, Span::new(6, 8));
    }
}
