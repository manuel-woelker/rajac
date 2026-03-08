use rajac_token::{Token, TokenKind};

pub struct Lexer<'a> {
    source: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source, pos: 0 }
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.source[self.pos..].chars().next()?;
        self.pos += ch.len_utf8();
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
        loop {
            match self.peek() {
                Some('*') => {
                    self.bump();
                    if self.peek() == Some('/') {
                        self.bump();
                        break;
                    }
                }
                Some(_) => {
                    self.bump();
                }
                None => break,
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

    fn read_number(&mut self, _start: usize) -> TokenKind {
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
        TokenKind::IntLiteral
    }

    fn read_string(&mut self, _start: usize) -> TokenKind {
        while let Some(c) = self.peek() {
            if c == '"' {
                self.bump();
                return TokenKind::StringLiteral;
            }
            if c == '\\' {
                self.bump();
                self.bump();
                continue;
            }
            if c == '\n' {
                break;
            }
            self.bump();
        }
        TokenKind::StringLiteral
    }

    fn read_char(&mut self, _start: usize) -> TokenKind {
        while let Some(c) = self.peek() {
            if c == '\'' {
                self.bump();
                return TokenKind::CharLiteral;
            }
            if c == '\\' {
                self.bump();
                self.bump();
                continue;
            }
            if c == '\n' {
                break;
            }
            self.bump();
        }
        TokenKind::CharLiteral
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
            '.' => TokenKind::Dot,
            '+' => return None,
            '-' => return None,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
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
                    #[allow(clippy::needless_return)]
                    return self.next_token();
                }
                Some('*') => {
                    self.bump();
                    self.skip_block_comment();
                    #[allow(clippy::needless_return)]
                    return self.next_token();
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
            '"' => {
                self.read_string(self.pos - 1);
                TokenKind::StringLiteral
            }
            '\'' => {
                self.read_char(self.pos - 1);
                TokenKind::CharLiteral
            }
            _ if c.is_ascii_digit() => {
                self.read_number(self.pos - 1);
                TokenKind::IntLiteral
            }
            _ if c.is_alphabetic() || c == '_' || c == '$' => {
                self.read_ident(self.pos - 1);
                TokenKind::Ident
            }
            _ => TokenKind::Eof,
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
