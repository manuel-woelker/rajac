use crate::parser::Parser;
use rajac_ast::*;
use rajac_token::TokenKind;
use rajac_types::Ident;

impl<'a> Parser<'a> {
    /// Parse an expression
    pub fn parse_expression(&mut self) -> Option<ExprId> {
        let expr = self.parse_ternary()?;
        self.parse_assignment(expr)
    }

    fn parse_ternary(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_or()?;

        if self.consume(TokenKind::Question) {
            let then_expr = self.parse_expression()?;
            self.expect(TokenKind::Colon);
            let else_expr = self.parse_expression()?;

            let ternary = Expr::Ternary {
                condition: expr,
                then_expr,
                else_expr,
            };
            expr = self.arena.alloc_expr(ternary);
        }

        Some(expr)
    }

    fn parse_or(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_and()?;

        while self.consume(TokenKind::Or) {
            let rhs = self.parse_and()?;
            let binary = Expr::Binary {
                op: BinaryOp::Or,
                lhs: expr,
                rhs,
            };
            expr = self.arena.alloc_expr(binary);
        }

        Some(expr)
    }

    fn parse_and(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_bitwise_or()?;

        while self.consume(TokenKind::And) {
            let rhs = self.parse_bitwise_or()?;
            let binary = Expr::Binary {
                op: BinaryOp::And,
                lhs: expr,
                rhs,
            };
            expr = self.arena.alloc_expr(binary);
        }

        Some(expr)
    }

    fn parse_bitwise_or(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_bitwise_xor()?;

        while self.is(TokenKind::Pipe) && !self.is_or() {
            self.bump();
            let rhs = self.parse_bitwise_xor()?;
            let binary = Expr::Binary {
                op: BinaryOp::BitOr,
                lhs: expr,
                rhs,
            };
            expr = self.arena.alloc_expr(binary);
        }

        Some(expr)
    }

    fn parse_bitwise_xor(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_bitwise_and()?;

        while self.consume(TokenKind::Caret) {
            let rhs = self.parse_bitwise_and()?;
            let binary = Expr::Binary {
                op: BinaryOp::BitXor,
                lhs: expr,
                rhs,
            };
            expr = self.arena.alloc_expr(binary);
        }

        Some(expr)
    }

    fn parse_bitwise_and(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_equality()?;

        while self.is(TokenKind::Ampersand) && !self.is_and() {
            self.bump();
            let rhs = self.parse_equality()?;
            let binary = Expr::Binary {
                op: BinaryOp::BitAnd,
                lhs: expr,
                rhs,
            };
            expr = self.arena.alloc_expr(binary);
        }

        Some(expr)
    }

    fn parse_equality(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_relational()?;

        loop {
            let op = match self.peek() {
                TokenKind::EqEq => BinaryOp::EqEq,
                TokenKind::BangEq => BinaryOp::BangEq,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_relational()?;
            let binary = Expr::Binary { op, lhs: expr, rhs };
            expr = self.arena.alloc_expr(binary);
        }

        Some(expr)
    }

    fn parse_relational(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_shift()?;

        loop {
            if self.consume(TokenKind::Lt) {
                let rhs = self.parse_shift()?;
                let binary = Expr::Binary {
                    op: BinaryOp::Lt,
                    lhs: expr,
                    rhs,
                };
                expr = self.arena.alloc_expr(binary);
            } else if self.consume(TokenKind::LtEq) {
                let rhs = self.parse_shift()?;
                let binary = Expr::Binary {
                    op: BinaryOp::LtEq,
                    lhs: expr,
                    rhs,
                };
                expr = self.arena.alloc_expr(binary);
            } else if self.consume(TokenKind::Gt) {
                let rhs = self.parse_shift()?;
                let binary = Expr::Binary {
                    op: BinaryOp::Gt,
                    lhs: expr,
                    rhs,
                };
                expr = self.arena.alloc_expr(binary);
            } else if self.consume(TokenKind::GtEq) {
                let rhs = self.parse_shift()?;
                let binary = Expr::Binary {
                    op: BinaryOp::GtEq,
                    lhs: expr,
                    rhs,
                };
                expr = self.arena.alloc_expr(binary);
            } else if self.consume(TokenKind::KwInstanceof) {
                let ty = self.parse_type()?;
                let instanceof = Expr::InstanceOf { expr, ty };
                expr = self.arena.alloc_expr(instanceof);
            } else {
                break;
            }
        }

        Some(expr)
    }

    fn parse_shift(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_additive()?;

        loop {
            let op = match self.peek() {
                TokenKind::LtLt => BinaryOp::LShift,
                TokenKind::GtGt => BinaryOp::RShift,
                TokenKind::GtGtGt => BinaryOp::ARShift,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_additive()?;
            let binary = Expr::Binary { op, lhs: expr, rhs };
            expr = self.arena.alloc_expr(binary);
        }

        Some(expr)
    }

    fn parse_additive(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_multiplicative()?;

        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_multiplicative()?;
            let binary = Expr::Binary { op, lhs: expr, rhs };
            expr = self.arena.alloc_expr(binary);
        }

        Some(expr)
    }

    fn parse_multiplicative(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_unary()?;

        loop {
            let op = match self.peek() {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Mod,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_unary()?;
            let binary = Expr::Binary { op, lhs: expr, rhs };
            expr = self.arena.alloc_expr(binary);
        }

        Some(expr)
    }

    fn parse_unary(&mut self) -> Option<ExprId> {
        match self.peek() {
            TokenKind::Plus => {
                self.bump();
                let expr = self.parse_unary()?;
                let unary = Expr::Unary {
                    op: UnaryOp::Plus,
                    expr,
                };
                Some(self.arena.alloc_expr(unary))
            }
            TokenKind::Minus => {
                self.bump();
                let expr = self.parse_unary()?;
                let unary = Expr::Unary {
                    op: UnaryOp::Minus,
                    expr,
                };
                Some(self.arena.alloc_expr(unary))
            }
            TokenKind::Bang => {
                self.bump();
                let expr = self.parse_unary()?;
                let unary = Expr::Unary {
                    op: UnaryOp::Bang,
                    expr,
                };
                Some(self.arena.alloc_expr(unary))
            }
            TokenKind::Tilde => {
                self.bump();
                let expr = self.parse_unary()?;
                let unary = Expr::Unary {
                    op: UnaryOp::Tilde,
                    expr,
                };
                Some(self.arena.alloc_expr(unary))
            }
            TokenKind::PlusPlus => {
                self.bump();
                let expr = self.parse_unary()?;
                let unary = Expr::Unary {
                    op: UnaryOp::Increment,
                    expr,
                };
                Some(self.arena.alloc_expr(unary))
            }
            TokenKind::MinusMinus => {
                self.bump();
                let expr = self.parse_unary()?;
                let unary = Expr::Unary {
                    op: UnaryOp::Decrement,
                    expr,
                };
                Some(self.arena.alloc_expr(unary))
            }
            TokenKind::LParen => {
                self.bump();
                if matches!(
                    self.peek(),
                    TokenKind::KwBoolean
                        | TokenKind::KwByte
                        | TokenKind::KwChar
                        | TokenKind::KwShort
                        | TokenKind::KwInt
                        | TokenKind::KwLong
                        | TokenKind::KwFloat
                        | TokenKind::KwDouble
                        | TokenKind::KwVoid
                        | TokenKind::KwVar
                ) {
                    let ty = self.parse_type()?;
                    self.expect(TokenKind::RParen);
                    let expr = self.parse_unary()?;
                    let cast = Expr::Cast { ty, expr };
                    return Some(self.arena.alloc_expr(cast));
                }

                let expr = self.parse_expression()?;
                self.expect(TokenKind::RParen);
                Some(expr)
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Option<ExprId> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek() {
                TokenKind::Dot => {
                    self.bump();
                    if self.peek() == TokenKind::Ident {
                        let name = Ident::new(self.ident_text());
                        self.bump();

                        if self.is(TokenKind::LParen) {
                            // Method call
                            self.bump();
                            let args = self.parse_arguments();
                            self.expect(TokenKind::RParen);

                            let method_call = Expr::MethodCall {
                                expr: Some(expr),
                                name,
                                type_args: None,
                                args,
                                method_id: None,
                            };
                            expr = self.arena.alloc_expr(method_call);
                        } else {
                            // Field access
                            let field_access = Expr::FieldAccess {
                                expr,
                                name,
                                field_id: None,
                            };
                            expr = self.arena.alloc_expr(field_access);
                        }
                    } else if self.peek() == TokenKind::KwClass {
                        // .class literal
                        self.bump();
                        let length = Expr::ArrayLength { array: expr };
                        expr = self.arena.alloc_expr(length);
                    }
                }
                TokenKind::LBracket => {
                    self.bump();
                    let index = self.parse_expression()?;
                    self.expect(TokenKind::RBracket);

                    let array_access = Expr::ArrayAccess { array: expr, index };
                    expr = self.arena.alloc_expr(array_access);
                }
                TokenKind::PlusPlus => {
                    self.bump();
                    let unary = Expr::Unary {
                        op: UnaryOp::Increment,
                        expr,
                    };
                    expr = self.arena.alloc_expr(unary);
                }
                TokenKind::MinusMinus => {
                    self.bump();
                    let unary = Expr::Unary {
                        op: UnaryOp::Decrement,
                        expr,
                    };
                    expr = self.arena.alloc_expr(unary);
                }
                _ => break,
            }
        }

        Some(expr)
    }

    fn parse_primary(&mut self) -> Option<ExprId> {
        match self.peek() {
            TokenKind::IntLiteral | TokenKind::LongLiteral => {
                let value = rajac_base::shared_string::SharedString::new(
                    &self.source[self.current.span.clone()],
                );
                let kind = if matches!(self.peek(), TokenKind::LongLiteral) {
                    LiteralKind::Long
                } else {
                    LiteralKind::Int
                };
                self.bump();
                let lit = Literal { kind, value };
                Some(self.arena.alloc_expr(Expr::Literal(lit)))
            }
            TokenKind::FloatLiteral => {
                let value = rajac_base::shared_string::SharedString::new(
                    &self.source[self.current.span.clone()],
                );
                self.bump();
                let lit = Literal {
                    kind: LiteralKind::Float,
                    value,
                };
                Some(self.arena.alloc_expr(Expr::Literal(lit)))
            }
            TokenKind::DoubleLiteral => {
                let value = rajac_base::shared_string::SharedString::new(
                    &self.source[self.current.span.clone()],
                );
                self.bump();
                let lit = Literal {
                    kind: LiteralKind::Double,
                    value,
                };
                Some(self.arena.alloc_expr(Expr::Literal(lit)))
            }
            TokenKind::StringLiteral => {
                let raw_value = &self.source[self.current.span.clone()];
                // Strip the surrounding quotes from string literal
                let value = if raw_value.len() >= 2
                    && raw_value.starts_with('"')
                    && raw_value.ends_with('"')
                {
                    &raw_value[1..raw_value.len() - 1]
                } else {
                    raw_value
                };
                let value = rajac_base::shared_string::SharedString::new(value);
                self.bump();
                let lit = Literal {
                    kind: LiteralKind::String,
                    value,
                };
                Some(self.arena.alloc_expr(Expr::Literal(lit)))
            }
            TokenKind::CharLiteral => {
                let value = rajac_base::shared_string::SharedString::new(
                    &self.source[self.current.span.clone()],
                );
                self.bump();
                let lit = Literal {
                    kind: LiteralKind::Char,
                    value,
                };
                Some(self.arena.alloc_expr(Expr::Literal(lit)))
            }
            TokenKind::KwTrue | TokenKind::KwFalse => {
                let value = rajac_base::shared_string::SharedString::new(
                    &self.source[self.current.span.clone()],
                );
                self.bump();
                let lit = Literal {
                    kind: LiteralKind::Bool,
                    value,
                };
                Some(self.arena.alloc_expr(Expr::Literal(lit)))
            }
            TokenKind::NullLiteral => {
                let value = rajac_base::shared_string::SharedString::new("null");
                self.bump();
                let lit = Literal {
                    kind: LiteralKind::Null,
                    value,
                };
                Some(self.arena.alloc_expr(Expr::Literal(lit)))
            }
            TokenKind::Ident => {
                let name = Ident::new(self.ident_text());
                self.bump();

                if self.is(TokenKind::LParen) {
                    // Method call without receiver
                    self.bump();
                    let args = self.parse_arguments();
                    self.expect(TokenKind::RParen);

                    let method_call = Expr::MethodCall {
                        expr: None,
                        name,
                        type_args: None,
                        args,
                        method_id: None,
                    };
                    Some(self.arena.alloc_expr(method_call))
                } else {
                    // Just an identifier
                    Some(self.arena.alloc_expr(Expr::Ident(name)))
                }
            }
            TokenKind::KwThis => {
                self.bump();
                let expr = if self.is(TokenKind::LParen) {
                    self.bump();
                    Some(self.arena.alloc_expr(Expr::Ident(Ident::new(
                        rajac_base::shared_string::SharedString::new("this"),
                    ))))
                } else {
                    None
                };
                Some(self.arena.alloc_expr(Expr::This(expr)))
            }
            TokenKind::KwSuper => {
                self.bump();
                if self.is(TokenKind::LParen) {
                    self.bump();
                    let args = self.parse_arguments();
                    self.expect(TokenKind::RParen);
                    let super_call = Expr::SuperCall {
                        name: Ident::new(rajac_base::shared_string::SharedString::new("super")),
                        type_args: None,
                        args,
                        method_id: None,
                    };
                    Some(self.arena.alloc_expr(super_call))
                } else {
                    Some(self.arena.alloc_expr(Expr::Super))
                }
            }
            TokenKind::KwNew => {
                self.bump();
                if let Some(ty) = self.parse_type_without_array_suffix() {
                    if self.is(TokenKind::LBracket) {
                        // Array instantiation
                        let mut dimensions = Vec::new();
                        while self.consume(TokenKind::LBracket) {
                            if let Some(dim) = self.parse_expression() {
                                dimensions.push(dim);
                            }
                            self.expect(TokenKind::RBracket);
                        }
                        let new_array = Expr::NewArray { ty, dimensions };
                        Some(self.arena.alloc_expr(new_array))
                    } else if self.is(TokenKind::LParen) {
                        // Constructor call
                        self.bump();
                        let args = self.parse_arguments();
                        self.expect(TokenKind::RParen);
                        let new_expr = Expr::New { ty, args };
                        Some(self.arena.alloc_expr(new_expr))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            TokenKind::LParen => {
                self.bump();
                let expr = self.parse_expression()?;
                self.expect(TokenKind::RParen);
                Some(expr)
            }
            _ => None,
        }
    }

    fn parse_arguments(&mut self) -> Vec<ExprId> {
        let mut args = Vec::new();

        if self.is(TokenKind::RParen) {
            return args;
        }

        loop {
            if let Some(arg) = self.parse_expression() {
                args.push(arg);
            }
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }

        args
    }

    // Helper to check for assignment operators
    pub fn parse_assignment(&mut self, expr: ExprId) -> Option<ExprId> {
        let op = match self.peek() {
            TokenKind::Eq => AssignOp::Eq,
            TokenKind::PlusEq => AssignOp::AddEq,
            TokenKind::MinusEq => AssignOp::SubEq,
            TokenKind::StarEq => AssignOp::MulEq,
            TokenKind::SlashEq => AssignOp::DivEq,
            TokenKind::PercentEq => AssignOp::ModEq,
            TokenKind::AmpersandEq => AssignOp::AndEq,
            TokenKind::PipeEq => AssignOp::OrEq,
            TokenKind::CaretEq => AssignOp::XorEq,
            TokenKind::LtLtEq => AssignOp::LShiftEq,
            TokenKind::GtGtEq => AssignOp::RShiftEq,
            TokenKind::GtGtGtEq => AssignOp::ARShiftEq,
            _ => return Some(expr),
        };
        self.bump();
        let rhs = self.parse_ternary()?;
        let assign = Expr::Assign { op, lhs: expr, rhs };
        Some(self.arena.alloc_expr(assign))
    }

    // Check if next two tokens are || (logical OR)
    fn is_or(&self) -> bool {
        self.peek() == TokenKind::Or
    }

    // Check if next two tokens are && (logical AND)
    fn is_and(&self) -> bool {
        self.peek() == TokenKind::And
    }
}
