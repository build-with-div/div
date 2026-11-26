//! A CSS tokenizer (CSS Syntax Level 3, section 4) and component values
//! (section 5).
//!
//! The media query parser reads component values: tokens with blocks and
//! functions already matched, as Blink's `CSSParserTokenStream` hands them
//! out. Comments are dropped. Numbers keep the double value Blink's
//! tokenizer computes, and whether they were written as integers.

/// One token.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    /// A function name, without its `(`.
    Function(String),
    AtKeyword(String),
    Hash(String),
    String(String),
    BadString,
    Url(String),
    BadUrl,
    Delim(char),
    Number {
        value: f64,
        integer: bool,
    },
    Percentage(f64),
    Dimension {
        value: f64,
        integer: bool,
        unit: String,
    },
    Whitespace,
    Cdo,
    Cdc,
    Colon,
    Semicolon,
    Comma,
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    OpenBrace,
    CloseBrace,
}

/// A component value: a preserved token, a simple block or a function.
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentValue {
    Token(Token),
    /// `(...)`, `[...]` or `{...}`; the char is the opening bracket.
    Block(char, Vec<ComponentValue>),
    Function(String, Vec<ComponentValue>),
}

impl ComponentValue {
    pub fn is_whitespace(&self) -> bool {
        matches!(self, Self::Token(Token::Whitespace))
    }

    /// The identifier, if this is an ident token.
    pub fn ident(&self) -> Option<&str> {
        match self {
            Self::Token(Token::Ident(name)) => Some(name),
            _ => None,
        }
    }
}

/// Tokenize `input`. Comments are removed.
pub fn tokenize(input: &str) -> Vec<Token> {
    let chars: Vec<char> = preprocess(input).chars().collect();
    let mut tokenizer = Tokenizer { chars, at: 0 };
    let mut tokens = Vec::new();
    while let Some(token) = tokenizer.next_token() {
        tokens.push(token);
    }
    tokens
}

/// Parse `input` into a list of component values.
pub fn component_values(input: &str) -> Vec<ComponentValue> {
    let tokens = tokenize(input);
    let mut index = 0;
    let mut values = Vec::new();
    while index < tokens.len() {
        values.push(consume_component(&tokens, &mut index));
    }
    values
}

fn consume_component(tokens: &[Token], index: &mut usize) -> ComponentValue {
    let token = tokens[*index].clone();
    *index += 1;
    match token {
        Token::OpenParen => {
            ComponentValue::Block('(', consume_block(tokens, index, &Token::CloseParen))
        }
        Token::OpenBracket => {
            ComponentValue::Block('[', consume_block(tokens, index, &Token::CloseBracket))
        }
        Token::OpenBrace => {
            ComponentValue::Block('{', consume_block(tokens, index, &Token::CloseBrace))
        }
        Token::Function(name) => {
            ComponentValue::Function(name, consume_block(tokens, index, &Token::CloseParen))
        }
        other => ComponentValue::Token(other),
    }
}

/// The contents of a block, up to its closing token or the end of input.
fn consume_block(tokens: &[Token], index: &mut usize, close: &Token) -> Vec<ComponentValue> {
    let mut values = Vec::new();
    while *index < tokens.len() {
        if &tokens[*index] == close {
            *index += 1;
            return values;
        }
        values.push(consume_component(tokens, index));
    }
    values
}

/// CSS Syntax 3.3: CR LF, CR and FF become LF; NUL becomes U+FFFD.
fn preprocess(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\u{c}' => out.push('\n'),
            '\0' => out.push('\u{fffd}'),
            other => out.push(other),
        }
    }
    out
}

struct Tokenizer {
    chars: Vec<char>,
    at: usize,
}

fn is_whitespace(c: char) -> bool {
    matches!(c, '\n' | '\t' | ' ')
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}

fn is_name(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == '-'
}

fn is_non_printable(c: char) -> bool {
    matches!(c, '\0'..='\u{8}' | '\u{b}' | '\u{e}'..='\u{1f}' | '\u{7f}')
}

impl Tokenizer {
    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.at + offset).copied()
    }

    fn valid_escape(first: Option<char>, second: Option<char>) -> bool {
        first == Some('\\') && second.is_some_and(|c| c != '\n')
    }

    fn starts_ident(&self, offset: usize) -> bool {
        let (a, b, c) = (
            self.peek(offset),
            self.peek(offset + 1),
            self.peek(offset + 2),
        );
        match a {
            Some('-') => {
                b.is_some_and(|b| is_name_start(b) || b == '-') || Self::valid_escape(b, c)
            }
            Some(a) if is_name_start(a) => true,
            Some('\\') => Self::valid_escape(a, b),
            _ => false,
        }
    }

    fn starts_number(&self) -> bool {
        match self.peek(0) {
            Some('+' | '-') => match self.peek(1) {
                Some(c) if c.is_ascii_digit() => true,
                Some('.') => self.peek(2).is_some_and(|c| c.is_ascii_digit()),
                _ => false,
            },
            Some('.') => self.peek(1).is_some_and(|c| c.is_ascii_digit()),
            Some(c) => c.is_ascii_digit(),
            None => false,
        }
    }

    fn next_token(&mut self) -> Option<Token> {
        // Comments.
        while self.peek(0) == Some('/') && self.peek(1) == Some('*') {
            self.at += 2;
            while self.at < self.chars.len()
                && !(self.peek(0) == Some('*') && self.peek(1) == Some('/'))
            {
                self.at += 1;
            }
            self.at = (self.at + 2).min(self.chars.len());
        }
        let c = self.peek(0)?;
        if is_whitespace(c) {
            while self.peek(0).is_some_and(is_whitespace) {
                self.at += 1;
            }
            return Some(Token::Whitespace);
        }
        Some(match c {
            '"' | '\'' => {
                self.at += 1;
                self.string(c)
            }
            '#' => {
                if self.peek(1).is_some_and(is_name)
                    || Self::valid_escape(self.peek(1), self.peek(2))
                {
                    self.at += 1;
                    Token::Hash(self.name())
                } else {
                    self.at += 1;
                    Token::Delim('#')
                }
            }
            '(' => self.single(Token::OpenParen),
            ')' => self.single(Token::CloseParen),
            '[' => self.single(Token::OpenBracket),
            ']' => self.single(Token::CloseBracket),
            '{' => self.single(Token::OpenBrace),
            '}' => self.single(Token::CloseBrace),
            ',' => self.single(Token::Comma),
            ':' => self.single(Token::Colon),
            ';' => self.single(Token::Semicolon),
            '+' | '.' if self.starts_number() => self.numeric(),
            '-' => {
                if self.starts_number() {
                    self.numeric()
                } else if self.peek(1) == Some('-') && self.peek(2) == Some('>') {
                    self.at += 3;
                    Token::Cdc
                } else if self.starts_ident(0) {
                    self.ident_like()
                } else {
                    self.single(Token::Delim('-'))
                }
            }
            '<' if self.peek(1) == Some('!')
                && self.peek(2) == Some('-')
                && self.peek(3) == Some('-') =>
            {
                self.at += 4;
                Token::Cdo
            }
            '@' => {
                if self.starts_ident(1) {
                    self.at += 1;
                    Token::AtKeyword(self.name())
                } else {
                    self.single(Token::Delim('@'))
                }
            }
            '\\' => {
                if Self::valid_escape(Some('\\'), self.peek(1)) {
                    self.ident_like()
                } else {
                    self.single(Token::Delim('\\'))
                }
            }
            c if c.is_ascii_digit() => self.numeric(),
            c if is_name_start(c) => self.ident_like(),
            other => self.single(Token::Delim(other)),
        })
    }

    fn single(&mut self, token: Token) -> Token {
        self.at += 1;
        token
    }

    fn escape(&mut self) -> char {
        // The backslash is consumed.
        let Some(c) = self.peek(0) else {
            return '\u{fffd}';
        };
        self.at += 1;
        if c.is_ascii_hexdigit() {
            let mut value = c.to_digit(16).expect("hex");
            let mut count = 1;
            while count < 6 && self.peek(0).is_some_and(|c| c.is_ascii_hexdigit()) {
                value = value * 16 + self.peek(0).and_then(|c| c.to_digit(16)).expect("hex");
                self.at += 1;
                count += 1;
            }
            if self.peek(0).is_some_and(is_whitespace) {
                self.at += 1;
            }
            match char::from_u32(value) {
                Some(c) if value != 0 && !(0xd800..=0xdfff).contains(&value) => c,
                _ => '\u{fffd}',
            }
        } else {
            c
        }
    }

    fn name(&mut self) -> String {
        let mut out = String::new();
        loop {
            match self.peek(0) {
                Some(c) if is_name(c) => {
                    out.push(c);
                    self.at += 1;
                }
                Some('\\') if Self::valid_escape(Some('\\'), self.peek(1)) => {
                    self.at += 1;
                    out.push(self.escape());
                }
                _ => return out,
            }
        }
    }

    fn ident_like(&mut self) -> Token {
        let name = self.name();
        if self.peek(0) == Some('(') {
            self.at += 1;
            if name.eq_ignore_ascii_case("url") {
                let mut lookahead = 0;
                while self.peek(lookahead).is_some_and(is_whitespace)
                    && self.peek(lookahead + 1).is_some_and(is_whitespace)
                {
                    lookahead += 1;
                }
                let next = if self.peek(lookahead).is_some_and(is_whitespace) {
                    self.peek(lookahead + 1)
                } else {
                    self.peek(lookahead)
                };
                if !matches!(next, Some('"' | '\'')) {
                    return self.url();
                }
            }
            return Token::Function(name);
        }
        Token::Ident(name)
    }

    fn url(&mut self) -> Token {
        while self.peek(0).is_some_and(is_whitespace) {
            self.at += 1;
        }
        let mut out = String::new();
        loop {
            match self.peek(0) {
                None => return Token::Url(out),
                Some(')') => {
                    self.at += 1;
                    return Token::Url(out);
                }
                Some(c) if is_whitespace(c) => {
                    while self.peek(0).is_some_and(is_whitespace) {
                        self.at += 1;
                    }
                    match self.peek(0) {
                        None => return Token::Url(out),
                        Some(')') => {
                            self.at += 1;
                            return Token::Url(out);
                        }
                        _ => return self.bad_url(),
                    }
                }
                Some('"' | '\'' | '(') => return self.bad_url(),
                Some(c) if is_non_printable(c) => return self.bad_url(),
                Some('\\') => {
                    if Self::valid_escape(Some('\\'), self.peek(1)) {
                        self.at += 1;
                        out.push(self.escape());
                    } else {
                        return self.bad_url();
                    }
                }
                Some(c) => {
                    out.push(c);
                    self.at += 1;
                }
            }
        }
    }

    fn bad_url(&mut self) -> Token {
        loop {
            match self.peek(0) {
                None => return Token::BadUrl,
                Some(')') => {
                    self.at += 1;
                    return Token::BadUrl;
                }
                Some('\\') if Self::valid_escape(Some('\\'), self.peek(1)) => {
                    self.at += 1;
                    self.escape();
                }
                Some(_) => self.at += 1,
            }
        }
    }

    fn string(&mut self, quote: char) -> Token {
        let mut out = String::new();
        loop {
            match self.peek(0) {
                None => return Token::String(out),
                Some(c) if c == quote => {
                    self.at += 1;
                    return Token::String(out);
                }
                Some('\n') => return Token::BadString,
                Some('\\') => match self.peek(1) {
                    None => self.at += 1,
                    Some('\n') => self.at += 2,
                    Some(_) => {
                        self.at += 1;
                        out.push(self.escape());
                    }
                },
                Some(c) => {
                    out.push(c);
                    self.at += 1;
                }
            }
        }
    }

    /// CSS Syntax 4.3.12, with the value computed from the source text as
    /// a double, as Blink's tokenizer does.
    fn number(&mut self) -> (f64, bool) {
        let start = self.at;
        let mut integer = true;
        if matches!(self.peek(0), Some('+' | '-')) {
            self.at += 1;
        }
        while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
        if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            integer = false;
            self.at += 1;
            while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                self.at += 1;
            }
        }
        if matches!(self.peek(0), Some('e' | 'E')) {
            let digits_at = match self.peek(1) {
                Some('+' | '-') => 2,
                _ => 1,
            };
            if self.peek(digits_at).is_some_and(|c| c.is_ascii_digit()) {
                integer = false;
                self.at += digits_at;
                while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                    self.at += 1;
                }
            }
        }
        let text: String = self.chars[start..self.at].iter().collect();
        let value = text.parse::<f64>().unwrap_or(0.0);
        (value, integer)
    }

    fn numeric(&mut self) -> Token {
        let (value, integer) = self.number();
        if self.starts_ident(0) {
            let unit = self.name();
            Token::Dimension {
                value,
                integer,
                unit,
            }
        } else if self.peek(0) == Some('%') {
            self.at += 1;
            Token::Percentage(value)
        } else {
            Token::Number { value, integer }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_dimensions_and_functions() {
        assert_eq!(
            tokenize("1.5em -2 +3% calc("),
            [
                Token::Dimension {
                    value: 1.5,
                    integer: false,
                    unit: "em".into()
                },
                Token::Whitespace,
                Token::Number {
                    value: -2.0,
                    integer: true
                },
                Token::Whitespace,
                Token::Percentage(3.0),
                Token::Whitespace,
                Token::Function("calc".into()),
            ]
        );
        assert_eq!(
            tokenize("1e3x"),
            [Token::Dimension {
                value: 1000.0,
                integer: false,
                unit: "x".into()
            }]
        );
    }

    #[test]
    fn idents_escapes_and_comments() {
        assert_eq!(
            tokenize("/* c */a\\62 c -x --y"),
            [
                Token::Ident("abc".into()),
                Token::Whitespace,
                Token::Ident("-x".into()),
                Token::Whitespace,
                Token::Ident("--y".into()),
            ]
        );
        assert_eq!(tokenize("and("), [Token::Function("and".into())]);
    }

    #[test]
    fn blocks_nest_and_close_at_the_end() {
        let values = component_values("(a (b) f(c)) [d");
        assert_eq!(values.len(), 3);
        let ComponentValue::Block('(', inner) = &values[0] else {
            panic!("{values:?}");
        };
        assert_eq!(inner.len(), 5);
        assert!(matches!(&inner[4], ComponentValue::Function(name, _) if name == "f"));
        assert!(matches!(&values[2], ComponentValue::Block('[', inner) if inner.len() == 1));
    }
}
