use crate::parser::Parser;
use rajac_ast::*;
use rajac_token::TokenKind;

impl<'a> Parser<'a> {
    /// Parse a block of statements
    pub fn parse_block(&mut self) -> Option<StmtId> {
        if !self.consume(TokenKind::LBrace) {
            return None;
        }

        let mut stmts = Vec::new();
        while !self.is(TokenKind::RBrace) && !self.is(TokenKind::Eof) {
            if let Some(stmt) = self.parse_statement() {
                stmts.push(stmt);
            } else {
                break;
            }
        }

        self.expect(TokenKind::RBrace);
        let block = Stmt::Block(stmts);
        Some(self.arena.alloc_stmt(block))
    }

    /// Parse a single statement
    pub fn parse_statement(&mut self) -> Option<StmtId> {
        match self.peek() {
            TokenKind::LBrace => self.parse_block(),
            TokenKind::Semi => {
                self.bump();
                Some(self.arena.alloc_stmt(Stmt::Empty))
            }
            TokenKind::KwIf => self.parse_if_stmt(),
            TokenKind::KwWhile => self.parse_while_stmt(),
            TokenKind::KwDo => self.parse_do_while_stmt(),
            TokenKind::KwFor => self.parse_for_stmt(),
            TokenKind::KwSwitch => self.parse_switch_stmt(),
            TokenKind::KwReturn => self.parse_return_stmt(),
            TokenKind::KwBreak => self.parse_break_stmt(),
            TokenKind::KwContinue => self.parse_continue_stmt(),
            TokenKind::KwThrow => self.parse_throw_stmt(),
            TokenKind::KwTry => self.parse_try_stmt(),
            TokenKind::KwSynchronized => self.parse_synchronized_stmt(),
            // Type keywords indicate local variable declaration
            TokenKind::KwBoolean
            | TokenKind::KwByte
            | TokenKind::KwChar
            | TokenKind::KwShort
            | TokenKind::KwInt
            | TokenKind::KwLong
            | TokenKind::KwFloat
            | TokenKind::KwDouble
            | TokenKind::KwVoid
            | TokenKind::Ident => {
                // Try to parse as local variable or expression statement
                if self.is_local_var_decl() {
                    self.parse_local_var_decl()
                } else {
                    self.parse_expr_stmt()
                }
            }
            _ => {
                // Try expression statement
                if let Some(expr) = self.parse_expression() {
                    self.expect(TokenKind::Semi);
                    Some(self.arena.alloc_stmt(Stmt::Expr(expr)))
                } else {
                    None
                }
            }
        }
    }

    fn is_local_var_decl(&self) -> bool {
        // Quick heuristic: if we see a type followed by an identifier, it's likely a local var
        matches!(
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
        )
    }

    fn parse_local_var_decl(&mut self) -> Option<StmtId> {
        if let Some(ty) = self.parse_type()
            && self.peek() == TokenKind::Ident
        {
            let name = Ident::new(self.ident_text());
            self.bump();

            let initializer = if self.consume(TokenKind::Eq) {
                self.parse_expression()
            } else {
                None
            };

            self.expect(TokenKind::Semi);

            let stmt = Stmt::LocalVar {
                ty,
                name,
                initializer,
            };
            return Some(self.arena.alloc_stmt(stmt));
        }
        None
    }

    fn parse_expr_stmt(&mut self) -> Option<StmtId> {
        if let Some(expr) = self.parse_expression() {
            self.expect(TokenKind::Semi);
            Some(self.arena.alloc_stmt(Stmt::Expr(expr)))
        } else {
            None
        }
    }

    fn parse_if_stmt(&mut self) -> Option<StmtId> {
        self.expect(TokenKind::KwIf);
        self.expect(TokenKind::LParen);
        let condition = self.parse_expression()?;
        self.expect(TokenKind::RParen);

        let then_branch = self.parse_statement()?;

        let else_branch = if self.consume(TokenKind::KwElse) {
            self.parse_statement()
        } else {
            None
        };

        let stmt = Stmt::If {
            condition,
            then_branch,
            else_branch,
        };
        Some(self.arena.alloc_stmt(stmt))
    }

    fn parse_while_stmt(&mut self) -> Option<StmtId> {
        self.expect(TokenKind::KwWhile);
        self.expect(TokenKind::LParen);
        let condition = self.parse_expression()?;
        self.expect(TokenKind::RParen);

        let body = self.parse_statement()?;

        let stmt = Stmt::While { condition, body };
        Some(self.arena.alloc_stmt(stmt))
    }

    fn parse_do_while_stmt(&mut self) -> Option<StmtId> {
        self.expect(TokenKind::KwDo);
        let body = self.parse_statement()?;
        self.expect(TokenKind::KwWhile);
        self.expect(TokenKind::LParen);
        let condition = self.parse_expression()?;
        self.expect(TokenKind::RParen);
        self.expect(TokenKind::Semi);

        let stmt = Stmt::DoWhile { body, condition };
        Some(self.arena.alloc_stmt(stmt))
    }

    fn parse_for_stmt(&mut self) -> Option<StmtId> {
        self.expect(TokenKind::KwFor);
        self.expect(TokenKind::LParen);

        let init = if self.is(TokenKind::Semi) {
            None
        } else if self.is_local_var_decl() {
            let ty = self.parse_type()?;
            let name = if self.peek() == TokenKind::Ident {
                Ident::new(self.ident_text())
            } else {
                return None;
            };
            self.bump();

            let initializer = if self.consume(TokenKind::Eq) {
                self.parse_expression()
            } else {
                None
            };

            Some(ForInit::LocalVar {
                ty,
                name,
                initializer,
            })
        } else {
            self.parse_expression().map(ForInit::Expr)
        };

        self.expect(TokenKind::Semi);

        let condition = if self.is(TokenKind::Semi) {
            None
        } else {
            self.parse_expression()
        };

        self.expect(TokenKind::Semi);

        let update = if self.is(TokenKind::RParen) {
            None
        } else {
            self.parse_expression()
        };

        self.expect(TokenKind::RParen);

        let body = self.parse_statement()?;

        let stmt = Stmt::For {
            init,
            condition,
            update,
            body,
        };
        Some(self.arena.alloc_stmt(stmt))
    }

    fn parse_switch_stmt(&mut self) -> Option<StmtId> {
        self.expect(TokenKind::KwSwitch);
        self.expect(TokenKind::LParen);
        let expr = self.parse_expression()?;
        self.expect(TokenKind::RParen);

        self.expect(TokenKind::LBrace);

        let mut cases = Vec::new();
        while !self.is(TokenKind::RBrace) && !self.is(TokenKind::Eof) {
            match self.peek() {
                TokenKind::KwCase => {
                    self.bump();
                    if let Some(case_expr) = self.parse_expression() {
                        self.expect(TokenKind::Colon);
                        let label = SwitchLabel::Case(case_expr);

                        let mut body = Vec::new();
                        while !self.is(TokenKind::KwCase)
                            && !self.is(TokenKind::KwDefault)
                            && !self.is(TokenKind::RBrace)
                        {
                            if let Some(stmt) = self.parse_statement() {
                                body.push(stmt);
                            } else {
                                break;
                            }
                        }

                        cases.push(SwitchCase {
                            labels: vec![label],
                            body,
                        });
                    }
                }
                TokenKind::KwDefault => {
                    self.bump();
                    self.expect(TokenKind::Colon);

                    let mut body = Vec::new();
                    while !self.is(TokenKind::KwCase)
                        && !self.is(TokenKind::KwDefault)
                        && !self.is(TokenKind::RBrace)
                    {
                        if let Some(stmt) = self.parse_statement() {
                            body.push(stmt);
                        } else {
                            break;
                        }
                    }

                    cases.push(SwitchCase {
                        labels: vec![SwitchLabel::Default],
                        body,
                    });
                }
                _ => break,
            }
        }

        self.expect(TokenKind::RBrace);

        let stmt = Stmt::Switch { expr, cases };
        Some(self.arena.alloc_stmt(stmt))
    }

    fn parse_return_stmt(&mut self) -> Option<StmtId> {
        self.expect(TokenKind::KwReturn);
        let expr = if self.is(TokenKind::Semi) {
            None
        } else {
            self.parse_expression()
        };
        self.expect(TokenKind::Semi);

        let stmt = Stmt::Return(expr);
        Some(self.arena.alloc_stmt(stmt))
    }

    fn parse_break_stmt(&mut self) -> Option<StmtId> {
        self.expect(TokenKind::KwBreak);
        let label = if self.peek() == TokenKind::Ident && !self.is(TokenKind::Semi) {
            Some(Ident::new(self.ident_text()))
        } else {
            None
        };
        if label.is_some() {
            self.bump();
        }
        self.expect(TokenKind::Semi);

        let stmt = Stmt::Break(label);
        Some(self.arena.alloc_stmt(stmt))
    }

    fn parse_continue_stmt(&mut self) -> Option<StmtId> {
        self.expect(TokenKind::KwContinue);
        let label = if self.peek() == TokenKind::Ident && !self.is(TokenKind::Semi) {
            Some(Ident::new(self.ident_text()))
        } else {
            None
        };
        if label.is_some() {
            self.bump();
        }
        self.expect(TokenKind::Semi);

        let stmt = Stmt::Continue(label);
        Some(self.arena.alloc_stmt(stmt))
    }

    fn parse_throw_stmt(&mut self) -> Option<StmtId> {
        self.expect(TokenKind::KwThrow);
        let expr = self.parse_expression()?;
        self.expect(TokenKind::Semi);

        let stmt = Stmt::Throw(expr);
        Some(self.arena.alloc_stmt(stmt))
    }

    fn parse_try_stmt(&mut self) -> Option<StmtId> {
        self.expect(TokenKind::KwTry);

        let try_block = self.parse_block()?;

        let mut catches = Vec::new();
        while self.is(TokenKind::KwCatch) {
            self.bump();
            self.expect(TokenKind::LParen);

            let ty = self.parse_type()?;
            let name = if self.peek() == TokenKind::Ident {
                Ident::new(self.ident_text())
            } else {
                Ident::new(rajac_base::shared_string::SharedString::new("e"))
            };
            self.bump();

            self.expect(TokenKind::RParen);

            let body = self.parse_block()?;

            catches.push(CatchClause {
                param: self.arena.alloc_param(Param {
                    ty,
                    name,
                    varargs: false,
                }),
                body,
            });
        }

        let finally_block = if self.consume(TokenKind::KwFinally) {
            self.parse_block()
        } else {
            None
        };

        let stmt = Stmt::Try {
            try_block,
            catches,
            finally_block,
        };
        Some(self.arena.alloc_stmt(stmt))
    }

    fn parse_synchronized_stmt(&mut self) -> Option<StmtId> {
        self.expect(TokenKind::KwSynchronized);

        let expr = if self.consume(TokenKind::LParen) {
            let e = self.parse_expression();
            self.expect(TokenKind::RParen);
            e
        } else {
            None
        };

        let block = self.parse_block()?;

        let stmt = Stmt::Synchronized { expr, block };
        Some(self.arena.alloc_stmt(stmt))
    }
}
