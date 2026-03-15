use rajac_base::file_path::FilePath;
use rajac_base::shared_string::SharedString;
use rajac_diagnostics::{Annotation, Diagnostic, Diagnostics, Severity, SourceChunk, Span};
use rajac_token::{Token, TokenKind};
use std::ops::Range;

#[allow(dead_code)]
pub struct Lexer<'a> {
    source: &'a str,
    path: FilePath,
    pos: usize,
    line: usize,
    line_start: usize,
    diagnostics: Diagnostics,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str, path: FilePath) -> Self {
        Self {
            source,
            path,
            pos: 0,
            line: 1,
            line_start: 0,
            diagnostics: Diagnostics::new(),
        }
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    pub fn take_diagnostics(&mut self) -> Diagnostics {
        std::mem::take(&mut self.diagnostics)
    }

    #[allow(dead_code)]
    fn add_error(
        &mut self,
        error_msg: impl Into<SharedString>,
        annotation_msg: impl Into<SharedString>,
        span: Range<usize>,
    ) {
        let error_msg: SharedString = error_msg.into();
        let annotation_msg: SharedString = annotation_msg.into();
        let line_fragment = self.get_current_line();
        let line_offset = span.start.saturating_sub(self.line_start);
        let span_length = span.end - span.start;

        self.diagnostics.add(Diagnostic {
            severity: Severity::Error,
            message: error_msg,
            chunks: vec![SourceChunk {
                path: self.path.clone(),
                fragment: line_fragment,
                offset: self.line_start,
                line: self.line,
                annotations: vec![Annotation {
                    span: Span(line_offset..line_offset + span_length),
                    message: annotation_msg,
                }],
            }],
        });
    }

    fn get_current_line(&self) -> SharedString {
        let line_end = self.source[self.pos..]
            .find('\n')
            .map(|i| self.pos + i)
            .unwrap_or(self.source.len());
        self.source[self.line_start..line_end].into()
    }

    fn is_valid_escape_char(&self, c: char) -> bool {
        match c {
            'b' | 't' | 'n' | 'f' | 'r' | '"' | '\'' | '\\' => true,
            '0'..='7' => true, // Octal escape
            'u' => true,       // Unicode escape (handled separately)
            _ => false,
        }
    }

    fn validate_unicode_escape(&mut self) -> bool {
        // Check for one or more 'u' characters (e.g., \u, \uu)
        while self.peek() == Some('u') {
            self.bump();
        }

        // Expect exactly 4 hex digits
        for _ in 0..4 {
            match self.peek() {
                Some(c) if c.is_ascii_hexdigit() => {
                    self.bump();
                }
                _ => return false,
            }
        }

        true
    }

    #[allow(dead_code)]
    fn current_span(&self, start: usize) -> Span {
        Span(start..self.pos)
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.source[self.pos..].chars().next()?;
        self.pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.line_start = self.pos;
        }
        Some(ch)
    }

    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
    }

    fn skip_block_comment(&mut self) {
        let start = self.pos.saturating_sub(2); // position of the '/*'
        let start_line = self.line;
        let start_line_start = self.line_start;

        loop {
            match self.peek() {
                Some('*') => {
                    self.bump();
                    if self.peek() == Some('/') {
                        self.bump();
                        return;
                    }
                }
                Some(_) => {
                    self.bump();
                }
                None => {
                    // Report error on the line where the comment started
                    // We need to temporarily restore the line position for error reporting
                    let current_line = self.line;
                    let current_line_start = self.line_start;
                    let current_pos = self.pos;

                    self.line = start_line;
                    self.line_start = start_line_start;
                    self.pos = start;

                    let error_span = start..(start + 2); // Just point to the "/*" start

                    self.add_error("unclosed comment", "unclosed comment", error_span);

                    // Restore current position
                    self.line = current_line;
                    self.line_start = current_line_start;
                    self.pos = current_pos;

                    return;
                }
            }
        }
    }

    fn read_ident(&mut self, start: usize) -> TokenKind {
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' || c == '$' {
                self.bump();
            } else {
                break;
            }
        }
        let lit = &self.source[start..self.pos];
        match lit {
            "abstract" => TokenKind::KwAbstract,
            "assert" => TokenKind::KwAssert,
            "boolean" => TokenKind::KwBoolean,
            "break" => TokenKind::KwBreak,
            "byte" => TokenKind::KwByte,
            "case" => TokenKind::KwCase,
            "catch" => TokenKind::KwCatch,
            "char" => TokenKind::KwChar,
            "class" => TokenKind::KwClass,
            "const" => TokenKind::KwConst,
            "continue" => TokenKind::KwContinue,
            "default" => TokenKind::KwDefault,
            "do" => TokenKind::KwDo,
            "double" => TokenKind::KwDouble,
            "else" => TokenKind::KwElse,
            "enum" => TokenKind::KwEnum,
            "extends" => TokenKind::KwExtends,
            "final" => TokenKind::KwFinal,
            "finally" => TokenKind::KwFinally,
            "float" => TokenKind::KwFloat,
            "for" => TokenKind::KwFor,
            "goto" => TokenKind::KwGoto,
            "if" => TokenKind::KwIf,
            "implements" => TokenKind::KwImplements,
            "import" => TokenKind::KwImport,
            "instanceof" => TokenKind::KwInstanceof,
            "int" => TokenKind::KwInt,
            "interface" => TokenKind::KwInterface,
            "long" => TokenKind::KwLong,
            "native" => TokenKind::KwNative,
            "new" => TokenKind::KwNew,
            "package" => TokenKind::KwPackage,
            "private" => TokenKind::KwPrivate,
            "protected" => TokenKind::KwProtected,
            "public" => TokenKind::KwPublic,
            "return" => TokenKind::KwReturn,
            "short" => TokenKind::KwShort,
            "static" => TokenKind::KwStatic,
            "strictfp" => TokenKind::KwStrictfp,
            "super" => TokenKind::KwSuper,
            "switch" => TokenKind::KwSwitch,
            "synchronized" => TokenKind::KwSynchronized,
            "this" => TokenKind::KwThis,
            "throw" => TokenKind::KwThrow,
            "throws" => TokenKind::KwThrows,
            "transient" => TokenKind::KwTransient,
            "try" => TokenKind::KwTry,
            "void" => TokenKind::KwVoid,
            "volatile" => TokenKind::KwVolatile,
            "while" => TokenKind::KwWhile,
            "true" => TokenKind::KwTrue,
            "false" => TokenKind::KwFalse,
            "var" => TokenKind::KwVar,
            "record" => TokenKind::KwRecord,
            "sealed" => TokenKind::KwSealed,
            "permits" => TokenKind::KwPermits,
            "non-sealed" => TokenKind::KwNonSealed,
            "yield" => TokenKind::KwYield,
            "null" => TokenKind::NullLiteral,
            _ => TokenKind::Ident,
        }
    }

    fn read_number(&mut self, start: usize) -> TokenKind {
        let mut has_decimal_point = false;
        let mut has_exponent = false;
        let mut has_float_suffix = false;
        let mut has_double_suffix = false;
        let mut has_long_suffix = false;

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.bump();
            } else if c == '.' && !has_decimal_point && !has_exponent {
                self.bump();
                has_decimal_point = true;

                // Check if there's another decimal point after this one
                if let Some(next_char) = self.peek()
                    && next_char == '.'
                {
                    self.add_error(
                        "malformed number",
                        "malformed number format",
                        start..self.pos,
                    );
                    // Continue consuming to avoid getting stuck
                    self.bump();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit()
                            || c == '.'
                            || c == 'e'
                            || c == 'E'
                            || c == 'f'
                            || c == 'F'
                            || c == 'l'
                            || c == 'L'
                            || c == 'x'
                            || c == 'X'
                            || c == 'b'
                            || c == 'B'
                        {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    return TokenKind::Error;
                }
            } else if (c == 'e' || c == 'E') && !has_exponent {
                self.bump();
                has_exponent = true;
                // Allow optional sign after exponent
                if let Some(sign_char) = self.peek()
                    && (sign_char == '+' || sign_char == '-')
                {
                    self.bump();
                }
            } else if c == 'f' || c == 'F' {
                self.bump();
                has_float_suffix = true;
                break;
            } else if c == 'd' || c == 'D' {
                self.bump();
                has_double_suffix = true;
                break;
            } else if c == 'l' || c == 'L' {
                self.bump();
                has_long_suffix = true;
                break;
            } else if c == 'x' || c == 'X' || c == 'b' || c == 'B' {
                self.bump();
            } else {
                break;
            }
        }

        if has_float_suffix {
            TokenKind::FloatLiteral
        } else if has_double_suffix || has_decimal_point || has_exponent {
            TokenKind::DoubleLiteral
        } else if has_long_suffix {
            TokenKind::LongLiteral
        } else {
            let _ = start;
            TokenKind::IntLiteral
        }
    }

    fn read_string(&mut self, start: usize) -> TokenKind {
        let mut has_error = false;
        while let Some(c) = self.peek() {
            if c == '"' {
                self.bump();
                return if has_error {
                    TokenKind::Error
                } else {
                    TokenKind::StringLiteral
                };
            }
            if c == '\\' {
                self.bump();
                if let Some(escape_char) = self.peek() {
                    let escape_start = self.pos - 1;
                    if escape_char == 'u' {
                        self.bump(); // consume 'u'
                        if !self.validate_unicode_escape() {
                            self.add_error(
                                "illegal unicode escape",
                                "illegal unicode escape",
                                escape_start..self.pos,
                            );
                            has_error = true;
                        }
                    } else if !self.is_valid_escape_char(escape_char) {
                        self.add_error(
                            "illegal escape character",
                            format!("illegal escape character '\\{}'", escape_char),
                            escape_start..self.pos + 1,
                        );
                        self.bump();
                        has_error = true;
                    } else {
                        self.bump();
                    }
                } else {
                    self.add_error(
                        "incomplete escape sequence",
                        "incomplete escape sequence",
                        self.pos - 1..self.pos,
                    );
                    break;
                }
                continue;
            }
            if c == '\n' {
                self.add_error(
                    "unclosed string literal",
                    "string literal starts here",
                    start..self.pos,
                );
                return TokenKind::Error;
            }
            self.bump();
        }
        self.add_error(
            "unclosed string literal",
            "string literal starts here",
            start..self.pos,
        );
        TokenKind::Error
    }

    fn read_char(&mut self, start: usize) -> TokenKind {
        // Check for empty character literal
        if self.peek() == Some('\'') {
            self.add_error(
                "empty character literal",
                "empty character literal",
                start..self.pos + 1,
            );
            self.bump(); // consume closing quote
            return TokenKind::Error;
        }

        let mut has_error = false;
        while let Some(c) = self.peek() {
            if c == '\'' {
                self.bump();
                return if has_error {
                    TokenKind::Error
                } else {
                    TokenKind::CharLiteral
                };
            }
            if c == '\\' {
                self.bump();
                if let Some(escape_char) = self.peek() {
                    let escape_start = self.pos - 1;
                    if escape_char == 'u' {
                        self.bump(); // consume 'u'
                        if !self.validate_unicode_escape() {
                            self.add_error(
                                "illegal unicode escape",
                                "illegal unicode escape",
                                escape_start..self.pos,
                            );
                            has_error = true;
                        }
                    } else if !self.is_valid_escape_char(escape_char) {
                        self.add_error(
                            "illegal escape character",
                            format!("illegal escape character '\\{}'", escape_char),
                            escape_start..self.pos + 1,
                        );
                        self.bump();
                        has_error = true;
                    } else {
                        self.bump();
                    }
                } else {
                    self.add_error(
                        "incomplete escape sequence",
                        "incomplete escape sequence",
                        self.pos - 1..self.pos,
                    );
                    break;
                }
                continue;
            }
            if c == '\n' {
                self.add_error(
                    "unclosed character literal",
                    "unclosed character literal",
                    start..self.pos,
                );
                return TokenKind::Error;
            }
            self.bump();
        }
        self.add_error(
            "unclosed character literal",
            "unclosed character literal",
            start..self.pos,
        );
        TokenKind::Error
    }

    fn single_char_token(&self, c: char) -> Option<TokenKind> {
        Some(match c {
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ';' => TokenKind::Semi,
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            '.' => TokenKind::Dot,
            '+' => return None,
            '-' => return None,
            '*' => TokenKind::Star,
            '/' => return None, // Handle in main match for comments
            '%' => TokenKind::Percent,
            '&' => return None,
            '|' => return None,
            '^' => TokenKind::Caret,
            '~' => TokenKind::Tilde,
            '!' => return None,
            '?' => TokenKind::Question,
            '=' => return None,
            '<' => return None,
            '>' => return None,
            _ => return None,
        })
    }

    fn next_token(&mut self) -> TokenKind {
        let c = match self.bump() {
            Some(c) => c,
            None => return TokenKind::Eof,
        };

        if let Some(kind) = self.single_char_token(c) {
            return kind;
        }

        match c {
            '/' => match self.peek() {
                Some('/') => {
                    self.bump();
                    self.skip_line_comment();
                    self.next_token()
                }
                Some('*') => {
                    self.bump();
                    self.skip_block_comment();
                    self.next_token()
                }
                Some('=') => {
                    self.bump();
                    TokenKind::SlashEq
                }
                _ => TokenKind::Slash,
            },
            '+' => match self.peek() {
                Some('+') => {
                    self.bump();
                    TokenKind::PlusPlus
                }
                Some('=') => {
                    self.bump();
                    TokenKind::PlusEq
                }
                _ => TokenKind::Plus,
            },
            '-' => match self.peek() {
                Some('-') => {
                    self.bump();
                    TokenKind::MinusMinus
                }
                Some('=') => {
                    self.bump();
                    TokenKind::MinusEq
                }
                Some('>') => {
                    self.bump();
                    TokenKind::Arrow
                }
                _ => TokenKind::Minus,
            },
            '&' => match self.peek() {
                Some('&') => {
                    self.bump();
                    TokenKind::And
                }
                Some('=') => {
                    self.bump();
                    TokenKind::AmpersandEq
                }
                _ => TokenKind::Ampersand,
            },
            '|' => match self.peek() {
                Some('|') => {
                    self.bump();
                    TokenKind::Or
                }
                Some('=') => {
                    self.bump();
                    TokenKind::PipeEq
                }
                _ => TokenKind::Pipe,
            },
            '=' => match self.peek() {
                Some('=') => {
                    self.bump();
                    TokenKind::EqEq
                }
                _ => TokenKind::Eq,
            },
            '!' => match self.peek() {
                Some('=') => {
                    self.bump();
                    TokenKind::BangEq
                }
                _ => TokenKind::Bang,
            },
            '<' => match self.peek() {
                Some('<') => {
                    self.bump();
                    match self.peek() {
                        Some('=') => {
                            self.bump();
                            TokenKind::LtLtEq
                        }
                        _ => TokenKind::LtLt,
                    }
                }
                Some('=') => {
                    self.bump();
                    TokenKind::LtEq
                }
                _ => TokenKind::Lt,
            },
            '>' => match self.peek() {
                Some('>') => {
                    self.bump();
                    match self.peek() {
                        Some('=') => {
                            self.bump();
                            TokenKind::GtGtEq
                        }
                        Some('>') => {
                            self.bump();
                            match self.peek() {
                                Some('=') => {
                                    self.bump();
                                    TokenKind::GtGtGtEq
                                }
                                _ => TokenKind::GtGtGt,
                            }
                        }
                        _ => TokenKind::GtGt,
                    }
                }
                Some('=') => {
                    self.bump();
                    TokenKind::GtEq
                }
                _ => TokenKind::Gt,
            },
            '*' => match self.peek() {
                Some('=') => {
                    self.bump();
                    TokenKind::StarEq
                }
                _ => TokenKind::Star,
            },
            '%' => match self.peek() {
                Some('=') => {
                    self.bump();
                    TokenKind::PercentEq
                }
                _ => TokenKind::Percent,
            },
            '^' => match self.peek() {
                Some('=') => {
                    self.bump();
                    TokenKind::CaretEq
                }
                _ => TokenKind::Caret,
            },
            '"' => self.read_string(self.pos - 1),
            '\'' => self.read_char(self.pos - 1),
            _ if c.is_ascii_digit() => {
                let start_pos = self.pos - 1;
                let number_kind = self.read_number(start_pos);

                // Check if this is actually an invalid identifier (digit followed by identifier chars)
                if let Some(next_char) = self.peek() {
                    if next_char.is_alphabetic() || next_char == '_' || next_char == '$' {
                        self.add_error(
                            "invalid identifier start",
                            "identifier cannot start with a digit",
                            start_pos..self.pos,
                        );
                        // Continue reading as identifier to consume the rest
                        self.read_ident(start_pos);
                        TokenKind::Error
                    } else if next_char == '.' {
                        // This might be a malformed number like 1.2.3
                        // Check what comes after the dot
                        self.bump(); // consume the dot
                        if let Some(after_dot) = self.peek() {
                            if after_dot.is_ascii_digit() {
                                // We have something like 1.2.3 - this is malformed
                                self.add_error(
                                    "malformed number",
                                    "malformed number format",
                                    start_pos..self.pos + 1,
                                );
                                // Consume the rest of the digits
                                while let Some(c) = self.peek() {
                                    if c.is_ascii_digit() {
                                        self.bump();
                                    } else {
                                        break;
                                    }
                                }
                                TokenKind::Error
                            } else {
                                // Not a malformed number, put the dot back
                                self.pos -= 1;
                                number_kind
                            }
                        } else {
                            // End of input, not malformed
                            self.pos -= 1;
                            number_kind
                        }
                    } else {
                        number_kind
                    }
                } else {
                    number_kind
                }
            }
            _ if c.is_alphabetic() || c == '_' || c == '$' => self.read_ident(self.pos - 1),
            _ => {
                self.add_error(
                    "illegal character",
                    format!("illegal character '{}'", c),
                    self.pos - c.len_utf8()..self.pos,
                );
                TokenKind::Error
            }
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        self.skip_whitespace();
        let start = self.pos;
        let kind = self.next_token();
        if kind == TokenKind::Eof && start == self.pos {
            return None;
        }
        Some(Token {
            kind,
            span: start..self.pos,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;
    use rajac_diagnostics::render_diagnostics;
    use strip_ansi::strip_ansi;

    #[test]
    fn test_unterminated_string() {
        let source = r#" "Hello, World! "#;
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let tokens: Vec<_> = lexer.by_ref().collect();

        assert!(tokens.iter().any(|t| t.kind == TokenKind::Error));
        assert!(!lexer.diagnostics().is_empty());

        let output = render_diagnostics(lexer.diagnostics());
        let stripped = strip_ansi(&output);

        expect![[r#"
            error: unclosed string literal
              ╭▸ test.java:1:2
              │
            1 │  "Hello, World! 
              ╰╴ ━━━━━━━━━━━━━━━ string literal starts here"#]]
        .assert_eq(&stripped);
    }

    #[test]
    fn test_illegal_character() {
        let source = "int price = 20£;";
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let tokens: Vec<_> = lexer.by_ref().collect();

        assert!(tokens.iter().any(|t| t.kind == TokenKind::Error));
        assert!(!lexer.diagnostics().is_empty());

        let output = render_diagnostics(lexer.diagnostics());
        let stripped = strip_ansi(&output);

        expect![[r#"error: illegal character
  ╭▸ test.java:1:15
  │
1 │ int price = 20£;
  ╰╴              ━ illegal character '£'"#]]
        .assert_eq(&stripped);
    }

    #[test]
    fn test_invalid_escape_sequence() {
        let source = r#"String s = "\q";"#;
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let tokens: Vec<_> = lexer.by_ref().collect();

        assert!(tokens.iter().any(|t| t.kind == TokenKind::Error));
        assert!(!lexer.diagnostics().is_empty());

        let output = render_diagnostics(lexer.diagnostics());
        let stripped = strip_ansi(&output);

        expect![[r#"error: illegal escape character
  ╭▸ test.java:1:13
  │
1 │ String s = "\q";
  ╰╴            ━━ illegal escape character '\q'"#]]
        .assert_eq(&stripped);
    }

    #[test]
    fn test_bad_unicode_escape() {
        let source = "char c = '\\u00G1';";
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let tokens: Vec<_> = lexer.by_ref().collect();

        assert!(tokens.iter().any(|t| t.kind == TokenKind::Error));
        assert!(!lexer.diagnostics().is_empty());

        let output = render_diagnostics(lexer.diagnostics());
        let stripped = strip_ansi(&output);

        expect![[r#"
            error: illegal unicode escape
              ╭▸ test.java:1:11
              │
            1 │ char c = '\u00G1';
              ╰╴          ━━━━ illegal unicode escape"#]]
        .assert_eq(&stripped);
    }

    #[test]
    fn test_empty_char_literal() {
        let source = "char c = '';";
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let tokens: Vec<_> = lexer.by_ref().collect();

        assert!(tokens.iter().any(|t| t.kind == TokenKind::Error));
        assert!(!lexer.diagnostics().is_empty());

        let output = render_diagnostics(lexer.diagnostics());
        let stripped = strip_ansi(&output);

        expect![[r#"
            error: empty character literal
              ╭▸ test.java:1:10
              │
            1 │ char c = '';
              ╰╴         ━━ empty character literal"#]]
        .assert_eq(&stripped);
    }

    #[test]
    fn test_invalid_identifier_start() {
        let source = "int 2value = 42;";
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let tokens: Vec<_> = lexer.by_ref().collect();

        assert!(tokens.iter().any(|t| t.kind == TokenKind::Error));
        assert!(!lexer.diagnostics().is_empty());

        let output = render_diagnostics(lexer.diagnostics());
        let stripped = strip_ansi(&output);

        expect![[r#"
            error: invalid identifier start
              ╭▸ test.java:1:5
              │
            1 │ int 2value = 42;
              ╰╴    ━ identifier cannot start with a digit"#]]
        .assert_eq(&stripped);
    }

    #[test]
    fn test_malformed_number() {
        let source = "double d = 1.2.3;";
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let tokens: Vec<_> = lexer.by_ref().collect();

        assert!(tokens.iter().any(|t| t.kind == TokenKind::Error));
        assert!(!lexer.diagnostics().is_empty());

        let output = render_diagnostics(lexer.diagnostics());
        let stripped = strip_ansi(&output);

        expect![[r#"error: malformed number
  ╭▸ test.java:1:12
  │
1 │ double d = 1.2.3;
  ╰╴           ━━━━━ malformed number format"#]]
        .assert_eq(&stripped);
    }

    #[test]
    fn test_unclosed_block_comment() {
        let source = "/* comment\nint x = 5;";
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let _tokens: Vec<_> = lexer.by_ref().collect();

        assert!(!lexer.diagnostics().is_empty());

        let output = render_diagnostics(lexer.diagnostics());
        let stripped = strip_ansi(&output);

        expect![[r#"
            error: unclosed comment
              ╭▸ test.java:1:1
              │
            1 │ /* comment
              ╰╴━━ unclosed comment"#]]
        .assert_eq(&stripped);
    }

    #[test]
    fn test_unclosed_char_literal() {
        let source = "char c = 'a;";
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let tokens: Vec<_> = lexer.by_ref().collect();

        assert!(tokens.iter().any(|t| t.kind == TokenKind::Error));
        assert!(!lexer.diagnostics().is_empty());

        let output = render_diagnostics(lexer.diagnostics());
        let stripped = strip_ansi(&output);

        expect![[r#"
            error: unclosed character literal
              ╭▸ test.java:1:10
              │
            1 │ char c = 'a;
              ╰╴         ━━━ unclosed character literal"#]]
        .assert_eq(&stripped);
    }

    #[test]
    fn test_incomplete_escape_sequence() {
        let source = r#"String s = "\"#;
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let tokens: Vec<_> = lexer.by_ref().collect();

        assert!(tokens.iter().any(|t| t.kind == TokenKind::Error));
        assert!(!lexer.diagnostics().is_empty());

        let output = render_diagnostics(lexer.diagnostics());
        let stripped = strip_ansi(&output);

        expect![[r#"
            error: incomplete escape sequence
              ╭▸ test.java:1:13
              │
            1 │ String s = "\
              │             ━ incomplete escape sequence
              ╰╴
            error: unclosed string literal
              ╭▸ test.java:1:12
              │
            1 │ String s = "\
              ╰╴           ━━ string literal starts here"#]]
        .assert_eq(&stripped);
    }

    #[test]
    fn test_valid_unicode_escape() {
        let source = "char c = '\\u0041';";
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let tokens: Vec<_> = lexer.by_ref().collect();

        // Should not produce any errors
        assert!(tokens.iter().all(|t| t.kind != TokenKind::Error));
        assert!(lexer.diagnostics().is_empty());
    }

    #[test]
    fn test_valid_escape_sequences() {
        let source = r#"String s = "\b\n";"#;
        let mut lexer = Lexer::new(source, FilePath::new("test.java"));
        let tokens: Vec<_> = lexer.by_ref().collect();

        // Should not produce any errors
        assert!(tokens.iter().all(|t| t.kind != TokenKind::Error));
        assert!(lexer.diagnostics().is_empty());
    }
}
