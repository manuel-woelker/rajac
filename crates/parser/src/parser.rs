use rajac_ast::*;
#[allow(unused_imports)]
use rajac_base::file_path::FilePath;
use rajac_base::shared_string::SharedString;
use rajac_diagnostics::Diagnostics;
use rajac_lexer::Lexer;
use rajac_token::{Token, TokenKind};
use rajac_types::Ident;
use std::collections::VecDeque;

/// Result of parsing, containing the AST and arena
pub struct ParseResult {
    pub ast: Ast,
    pub arena: AstArena,
    pub diagnostics: Diagnostics,
}

pub struct Parser<'a> {
    pub lexer: Lexer<'a>,
    pub source: &'a str,
    pub current: Token,
    #[allow(dead_code)]
    tokens: VecDeque<Token>,
    pub arena: AstArena,
    in_interface: bool,
}

impl<'a> Parser<'a> {
    pub fn new(mut lexer: Lexer<'a>, source: &'a str) -> Self {
        let mut tokens = VecDeque::new();
        // Preload first token
        if let Some(token) = lexer.next() {
            tokens.push_back(token);
        }
        let current = tokens.pop_front().unwrap_or(Token {
            kind: TokenKind::Eof,
            span: 0..0,
        });

        Self {
            lexer,
            source,
            current,
            tokens,
            arena: AstArena::new(),
            in_interface: false,
        }
    }

    // === Token management ===

    /// Move to the next token
    pub fn bump(&mut self) {
        if self.current.kind != TokenKind::Eof {
            self.fill_lookahead(1);
            if let Some(token) = self.tokens.pop_front() {
                self.current = token;
            } else {
                self.current = Token {
                    kind: TokenKind::Eof,
                    span: self.source.len()..self.source.len(),
                };
            }
        }
    }

    /// Peek at the current token without consuming it
    pub fn peek(&self) -> TokenKind {
        self.current.kind
    }

    /// Peek ahead by `n` tokens without consuming them.
    pub fn peek_n(&mut self, n: usize) -> TokenKind {
        if n == 0 {
            return self.peek();
        }

        self.fill_lookahead(n);
        self.tokens
            .get(n - 1)
            .map(|token| token.kind)
            .unwrap_or(TokenKind::Eof)
    }

    /// Get the span of the current token
    fn current_span(&self) -> Span {
        Span(self.current.span.clone())
    }

    /// Get identifier text from current token
    pub fn ident_text(&self) -> rajac_base::shared_string::SharedString {
        rajac_base::shared_string::SharedString::new(&self.source[self.current.span.clone()])
    }

    /// Check if current token is of given kind
    pub fn is(&self, kind: TokenKind) -> bool {
        self.peek() == kind
    }

    /// Expect a specific token, consume it and return its span
    pub fn expect(&mut self, kind: TokenKind) -> Span {
        let span = self.current_span();
        if self.peek() == kind {
            self.bump();
        }
        span
    }

    /// Consume if current token matches, return true if consumed
    pub fn consume(&mut self, kind: TokenKind) -> bool {
        if self.peek() == kind {
            self.bump();
            true
        } else {
            false
        }
    }

    fn fill_lookahead(&mut self, count: usize) {
        while self.tokens.len() < count {
            let Some(token) = self.lexer.next() else {
                break;
            };
            self.tokens.push_back(token);
        }
    }

    // === Parsing entry points ===

    pub fn parse_compilation_unit(mut self) -> ParseResult {
        let mut ast = Ast::new(SharedString::new(self.source));

        // Parse package declaration if present
        if self.is(TokenKind::KwPackage) {
            ast.package = self.parse_package_decl();
        }

        // Parse imports
        while self.is(TokenKind::KwImport) {
            ast.imports.push(self.parse_import_decl());
        }

        // Parse top-level classes/interfaces/enums/records
        while !self.is(TokenKind::Eof) {
            if let Some(class_id) = self.parse_class_decl() {
                ast.classes.push(class_id);
            } else {
                // Skip malformed declarations
                self.bump();
            }
        }

        ParseResult {
            ast,
            arena: self.arena,
            diagnostics: self.lexer.take_diagnostics(),
        }
    }

    // === Declaration parsing ===

    fn parse_package_decl(&mut self) -> Option<PackageDecl> {
        self.expect(TokenKind::KwPackage);
        let name = self.parse_qualified_name()?;
        self.expect(TokenKind::Semi);
        Some(PackageDecl { name })
    }

    fn parse_import_decl(&mut self) -> ImportDecl {
        self.expect(TokenKind::KwImport);
        let is_static = self.consume(TokenKind::KwStatic);
        let name = self
            .parse_qualified_name()
            .unwrap_or_else(|| QualifiedName::new(vec![SharedString::new("error")]));
        let is_on_demand = self.consume(TokenKind::Dot) && self.consume(TokenKind::Star);
        self.expect(TokenKind::Semi);
        ImportDecl {
            name,
            is_static,
            is_on_demand,
        }
    }

    fn parse_qualified_name(&mut self) -> Option<QualifiedName> {
        let mut segments = Vec::new();
        if self.peek() == TokenKind::Ident {
            segments.push(self.ident_text());
            self.bump();
            while self.consume(TokenKind::Dot) {
                if self.peek() == TokenKind::Ident {
                    segments.push(self.ident_text());
                    self.bump();
                } else {
                    break;
                }
            }
            Some(QualifiedName::new(segments))
        } else {
            None
        }
    }

    fn parse_modifiers(&mut self) -> Modifiers {
        let mut flags = 0u32;
        loop {
            match self.peek() {
                TokenKind::KwPublic => {
                    flags |= Modifiers::PUBLIC;
                    self.bump();
                }
                TokenKind::KwPrivate => {
                    flags |= Modifiers::PRIVATE;
                    self.bump();
                }
                TokenKind::KwProtected => {
                    flags |= Modifiers::PROTECTED;
                    self.bump();
                }
                TokenKind::KwStatic => {
                    flags |= Modifiers::STATIC;
                    self.bump();
                }
                TokenKind::KwFinal => {
                    flags |= Modifiers::FINAL;
                    self.bump();
                }
                TokenKind::KwAbstract => {
                    flags |= Modifiers::ABSTRACT;
                    self.bump();
                }
                TokenKind::KwSynchronized => {
                    flags |= Modifiers::SYNCHRONIZED;
                    self.bump();
                }
                TokenKind::KwVolatile => {
                    flags |= Modifiers::VOLATILE;
                    self.bump();
                }
                TokenKind::KwTransient => {
                    flags |= Modifiers::TRANSIENT;
                    self.bump();
                }
                TokenKind::KwNative => {
                    flags |= Modifiers::NATIVE;
                    self.bump();
                }
                TokenKind::KwStrictfp => {
                    flags |= Modifiers::STRICTFP;
                    self.bump();
                }
                _ => break,
            }
        }
        Modifiers(flags)
    }

    fn parse_class_decl(&mut self) -> Option<ClassDeclId> {
        let modifiers = self.parse_modifiers();

        let kind = match self.peek() {
            TokenKind::KwClass => {
                self.bump();
                ClassKind::Class
            }
            TokenKind::KwInterface => {
                self.bump();
                ClassKind::Interface
            }
            TokenKind::KwEnum => {
                self.bump();
                ClassKind::Enum
            }
            TokenKind::KwRecord => {
                self.bump();
                ClassKind::Record
            }
            _ => return None,
        };

        // Class name
        let name = if self.peek() == TokenKind::Ident {
            Ident::new(self.ident_text())
        } else {
            return None;
        };
        self.bump();

        // Type parameters
        let type_params = if self.is(TokenKind::Lt) {
            self.parse_type_params()
        } else {
            Vec::new()
        };

        if matches!(kind, ClassKind::Enum) {
            return self.parse_enum_decl_with_name(name, modifiers);
        }

        // Extends clause
        let extends = if self.consume(TokenKind::KwExtends) {
            self.parse_type()
        } else {
            None
        };

        // Implements clause
        let mut implements = Vec::new();
        if self.consume(TokenKind::KwImplements) {
            loop {
                if let Some(ty) = self.parse_type() {
                    implements.push(ty);
                }
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
        }

        // Permits clause (for sealed classes)
        let mut permits = Vec::new();
        if self.consume(TokenKind::KwPermits) {
            loop {
                if let Some(ty) = self.parse_type() {
                    permits.push(ty);
                }
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
        }

        // Class body
        // Set interface context before parsing members
        let old_in_interface = self.in_interface;
        self.in_interface = matches!(kind, ClassKind::Interface);

        self.expect(TokenKind::LBrace);
        let members = self.parse_class_members(&name);
        self.expect(TokenKind::RBrace);

        // Restore previous context
        self.in_interface = old_in_interface;

        let class_decl = ClassDecl {
            kind,
            name,
            type_params,
            extends,
            implements,
            permits,
            enum_entries: Vec::new(),
            members,
            modifiers,
        };

        Some(self.arena.alloc_class_decl(class_decl))
    }

    fn parse_type_params(&mut self) -> Vec<AstTypeParam> {
        let mut params = Vec::new();
        if !self.consume(TokenKind::Lt) {
            return params;
        }

        loop {
            if self.peek() == TokenKind::Ident {
                let name = Ident::new(self.ident_text());
                self.bump();

                let mut bounds = Vec::new();
                if self.consume(TokenKind::KwExtends) {
                    loop {
                        if let Some(ty) = self.parse_type() {
                            bounds.push(ty);
                        }
                        if !self.consume(TokenKind::Ampersand) {
                            break;
                        }
                    }
                }

                params.push(AstTypeParam {
                    name: name.name,
                    bounds,
                });
            }

            if !self.consume(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::Gt);
        params
    }

    fn parse_class_members(&mut self, class_name: &Ident) -> Vec<ClassMemberId> {
        let mut members = Vec::new();

        while !self.is(TokenKind::RBrace) && !self.is(TokenKind::Eof) {
            // Static block
            if self.is(TokenKind::KwStatic) && self.peek() == TokenKind::LBrace {
                self.bump();
                if let Some(body) = self.parse_block() {
                    let member = ClassMember::StaticBlock(body);
                    members.push(self.arena.alloc_class_member(member));
                }
                continue;
            }

            let modifiers = self.parse_modifiers();

            // Check for nested types
            match self.peek() {
                TokenKind::KwClass => {
                    self.bump();
                    if let Some(nested_id) =
                        self.parse_class_decl_inner(modifiers, ClassKind::Class)
                    {
                        let member = ClassMember::NestedClass(nested_id);
                        members.push(self.arena.alloc_class_member(member));
                    }
                    continue;
                }
                TokenKind::KwInterface => {
                    self.bump();
                    if let Some(nested_id) =
                        self.parse_class_decl_inner(modifiers, ClassKind::Interface)
                    {
                        let member = ClassMember::NestedInterface(nested_id);
                        members.push(self.arena.alloc_class_member(member));
                    }
                    continue;
                }
                TokenKind::KwEnum => {
                    self.bump();
                    if let Some(enum_id) = self.parse_enum_decl(modifiers) {
                        let member = ClassMember::NestedEnum(enum_id);
                        members.push(self.arena.alloc_class_member(member));
                    }
                    continue;
                }
                TokenKind::KwRecord => {
                    self.bump();
                    if let Some(record_id) =
                        self.parse_class_decl_inner(modifiers, ClassKind::Record)
                    {
                        let member = ClassMember::NestedRecord(record_id);
                        members.push(self.arena.alloc_class_member(member));
                    }
                    continue;
                }
                _ => {}
            }

            // Parse field or method
            if let Some(ty) = self.parse_type() {
                if self.is(TokenKind::LParen) && self.type_matches_constructor_name(ty, class_name)
                {
                    if let Some(constructor) = self.parse_constructor(class_name.clone(), modifiers)
                    {
                        let member = ClassMember::Constructor(constructor);
                        members.push(self.arena.alloc_class_member(member));
                    }
                    continue;
                }

                if self.peek() == TokenKind::Ident {
                    let name = Ident::new(self.ident_text());
                    self.bump();

                    if self.is(TokenKind::LParen) {
                        // Method
                        if let Some(method_id) = self.parse_method(name, ty, modifiers) {
                            let member = ClassMember::Method(
                                self.arena.methods[method_id.0 as usize].clone(),
                            );
                            members.push(self.arena.alloc_class_member(member));
                        }
                    } else {
                        // Field
                        let initializer = if self.consume(TokenKind::Eq) {
                            self.parse_expression()
                        } else {
                            None
                        };

                        self.expect(TokenKind::Semi);

                        let field = Field {
                            name,
                            ty,
                            initializer,
                            modifiers,
                        };

                        let field_id = self.arena.alloc_field(field);
                        let member =
                            ClassMember::Field(self.arena.fields[field_id.0 as usize].clone());
                        members.push(self.arena.alloc_class_member(member));
                    }
                }
            } else if self.peek() == TokenKind::Ident {
                // Constructor
                let name = Ident::new(self.ident_text());
                self.bump();
                if let Some(constructor) = self.parse_constructor(name, modifiers) {
                    let member = ClassMember::Constructor(constructor);
                    members.push(self.arena.alloc_class_member(member));
                }
            } else {
                self.bump();
            }
        }

        members
    }

    fn parse_class_decl_inner(
        &mut self,
        modifiers: Modifiers,
        kind: ClassKind,
    ) -> Option<ClassDeclId> {
        let name = if self.peek() == TokenKind::Ident {
            Ident::new(self.ident_text())
        } else {
            return None;
        };
        self.bump();

        let type_params = if self.is(TokenKind::Lt) {
            self.parse_type_params()
        } else {
            Vec::new()
        };

        let extends = if self.consume(TokenKind::KwExtends) {
            self.parse_type()
        } else {
            None
        };

        let mut implements = Vec::new();
        if self.consume(TokenKind::KwImplements) {
            loop {
                if let Some(ty) = self.parse_type() {
                    implements.push(ty);
                }
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
        }

        // Set interface context before parsing members
        let old_in_interface = self.in_interface;
        self.in_interface = matches!(kind, ClassKind::Interface);

        self.expect(TokenKind::LBrace);
        let members = self.parse_class_members(&name);
        self.expect(TokenKind::RBrace);

        // Restore previous context
        self.in_interface = old_in_interface;

        let class_decl = ClassDecl {
            kind,
            name,
            type_params,
            extends,
            implements,
            permits: Vec::new(),
            enum_entries: Vec::new(),
            members,
            modifiers,
        };

        Some(self.arena.alloc_class_decl(class_decl))
    }

    fn parse_enum_decl(&mut self, modifiers: Modifiers) -> Option<ClassDeclId> {
        let name = if self.peek() == TokenKind::Ident {
            Ident::new(self.ident_text())
        } else {
            return None;
        };
        self.bump();
        self.parse_enum_decl_with_name(name, modifiers)
    }

    fn parse_enum_decl_with_name(
        &mut self,
        name: Ident,
        modifiers: Modifiers,
    ) -> Option<ClassDeclId> {
        let mut implements = Vec::new();
        if self.consume(TokenKind::KwImplements) {
            loop {
                if let Some(ty) = self.parse_type() {
                    implements.push(ty);
                }
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect(TokenKind::LBrace);

        let mut entries = Vec::new();
        while !self.is(TokenKind::RBrace) && !self.is(TokenKind::Eof) {
            if self.peek() == TokenKind::Ident {
                let entry_name = Ident::new(self.ident_text());
                self.bump();

                let args = if self.is(TokenKind::LParen) {
                    self.expect(TokenKind::LParen);
                    let mut args = Vec::new();
                    if !self.is(TokenKind::RParen) {
                        loop {
                            if let Some(arg) = self.parse_expression() {
                                args.push(arg);
                            }
                            if !self.consume(TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                    self.expect(TokenKind::RParen);
                    args
                } else {
                    Vec::new()
                };

                entries.push(EnumEntry {
                    name: entry_name,
                    args,
                    body: None,
                });

                if !self.consume(TokenKind::Comma) {
                    break;
                }
            } else {
                break;
            }
        }

        // Optional member declarations in enum
        let mut members = Vec::new();
        if self.consume(TokenKind::Semi) {
            members = self.parse_class_members(&name);
        }

        self.expect(TokenKind::RBrace);

        Some(self.arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Enum,
            name,
            type_params: Vec::new(),
            extends: None,
            implements,
            permits: Vec::new(),
            enum_entries: entries,
            members,
            modifiers,
        }))
    }

    fn parse_method(
        &mut self,
        name: Ident,
        return_ty: AstTypeId,
        modifiers: Modifiers,
    ) -> Option<MethodId> {
        self.expect(TokenKind::LParen);
        let params = self.parse_parameters();
        self.expect(TokenKind::RParen);

        let mut throws = Vec::new();
        if self.consume(TokenKind::KwThrows) {
            loop {
                if let Some(ty) = self.parse_type() {
                    throws.push(ty);
                }
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
        }

        let body = if self.is(TokenKind::LBrace) {
            self.parse_block()
        } else {
            self.consume(TokenKind::Semi);
            None
        };

        // Interface methods are implicitly public
        let final_modifiers = if self.in_interface
            && (modifiers.0
                & (rajac_ast::Modifiers::PUBLIC
                    | rajac_ast::Modifiers::PRIVATE
                    | rajac_ast::Modifiers::PROTECTED))
                == 0
        {
            Modifiers(modifiers.0 | rajac_ast::Modifiers::PUBLIC)
        } else {
            modifiers
        };

        let method = Method {
            name,
            params,
            return_ty,
            body,
            throws,
            modifiers: final_modifiers,
        };

        Some(self.arena.alloc_method(method))
    }

    fn parse_constructor(&mut self, name: Ident, modifiers: Modifiers) -> Option<Constructor> {
        self.expect(TokenKind::LParen);
        let params = self.parse_parameters();
        self.expect(TokenKind::RParen);

        let mut throws = Vec::new();
        if self.consume(TokenKind::KwThrows) {
            loop {
                if let Some(ty) = self.parse_type() {
                    throws.push(ty);
                }
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
        }

        let body = self.parse_block();

        Some(Constructor {
            name,
            params,
            body,
            throws,
            modifiers,
        })
    }

    fn parse_parameters(&mut self) -> Vec<ParamId> {
        let mut params = Vec::new();

        if self.is(TokenKind::RParen) {
            return params;
        }

        loop {
            if let Some(ty) = self.parse_type() {
                let varargs = self.consume(TokenKind::Dot)
                    && self.consume(TokenKind::Dot)
                    && self.consume(TokenKind::Dot);

                if self.peek() == TokenKind::Ident {
                    let name = Ident::new(self.ident_text());
                    self.bump();

                    let param = Param { ty, name, varargs };
                    params.push(self.arena.alloc_param(param));
                }
            }

            if !self.consume(TokenKind::Comma) {
                break;
            }
        }

        params
    }

    fn type_matches_constructor_name(&self, ty: AstTypeId, class_name: &Ident) -> bool {
        matches!(
            self.arena.ty(ty),
            AstType::Simple { name, .. } if name == &class_name.name
        )
    }

    pub fn parse_type(&mut self) -> Option<AstTypeId> {
        self.parse_type_with_array_suffix(true)
    }

    pub fn parse_type_without_array_suffix(&mut self) -> Option<AstTypeId> {
        self.parse_type_with_array_suffix(false)
    }

    fn parse_type_with_array_suffix(&mut self, allow_array_suffix: bool) -> Option<AstTypeId> {
        let ty = match self.peek() {
            TokenKind::KwBoolean => {
                self.bump();
                AstType::primitive(PrimitiveType::Boolean)
            }
            TokenKind::KwByte => {
                self.bump();
                AstType::primitive(PrimitiveType::Byte)
            }
            TokenKind::KwChar => {
                self.bump();
                AstType::primitive(PrimitiveType::Char)
            }
            TokenKind::KwShort => {
                self.bump();
                AstType::primitive(PrimitiveType::Short)
            }
            TokenKind::KwInt => {
                self.bump();
                AstType::primitive(PrimitiveType::Int)
            }
            TokenKind::KwLong => {
                self.bump();
                AstType::primitive(PrimitiveType::Long)
            }
            TokenKind::KwFloat => {
                self.bump();
                AstType::primitive(PrimitiveType::Float)
            }
            TokenKind::KwDouble => {
                self.bump();
                AstType::primitive(PrimitiveType::Double)
            }
            TokenKind::KwVoid => {
                self.bump();
                AstType::primitive(PrimitiveType::Void)
            }
            TokenKind::KwVar => {
                self.bump();
                // var is desugared to int for now - type inference should happen in later phases
                AstType::primitive(PrimitiveType::Int)
            }
            TokenKind::Ident => {
                let name = self.ident_text();
                self.bump();

                let type_args = if self.is(TokenKind::Lt) {
                    self.parse_type_arguments()
                } else {
                    None
                };

                AstType::simple_with_args(name, type_args.unwrap_or_default())
            }
            TokenKind::Question => {
                self.bump();
                let bound = if self.consume(TokenKind::KwExtends) {
                    self.parse_type().map(WildcardBound::Extends)
                } else if self.consume(TokenKind::KwSuper) {
                    self.parse_type().map(WildcardBound::Super)
                } else {
                    None
                };
                AstType::wildcard(bound)
            }
            _ => return None,
        };

        let ty_id = self.arena.alloc_type(ty);

        if !allow_array_suffix {
            return Some(ty_id);
        }

        // Handle array notation
        let mut dimensions = 0;
        while self.consume(TokenKind::LBracket) {
            self.expect(TokenKind::RBracket);
            dimensions += 1;
        }

        if dimensions > 0 {
            let array_type = AstType::array(ty_id, dimensions);
            Some(self.arena.alloc_type(array_type))
        } else {
            Some(ty_id)
        }
    }

    fn parse_type_arguments(&mut self) -> Option<Vec<AstTypeId>> {
        if !self.consume(TokenKind::Lt) {
            return None;
        }

        let mut args = Vec::new();
        loop {
            if self.peek() == TokenKind::Question {
                self.bump();
                let bound = if self.consume(TokenKind::KwExtends) {
                    self.parse_type().map(WildcardBound::Extends)
                } else if self.consume(TokenKind::KwSuper) {
                    self.parse_type().map(WildcardBound::Super)
                } else {
                    None
                };
                let wildcard = AstType::wildcard(bound);
                args.push(self.arena.alloc_type(wildcard));
            } else if let Some(ty) = self.parse_type() {
                args.push(ty);
            }

            if !self.consume(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::Gt);
        Some(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_src(source: &str) -> ParseResult {
        let lexer = Lexer::new(source, FilePath::new("test.java"));
        let parser = Parser::new(lexer, source);
        parser.parse_compilation_unit()
    }

    #[test]
    fn test_empty_class() {
        let result = parse_src("class HelloWorld {}");
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_class_with_main_method() {
        let source = r#"
            public class HelloWorld {
                public static void main(String[] args) {
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_package_and_import() {
        let source = r#"
            package com.example;
            import java.util.List;
            import static java.lang.Math.*;
            
            class Example {}
        "#;
        let result = parse_src(source);
        assert!(result.ast.package.is_some());
        assert_eq!(result.ast.imports.len(), 2);
    }

    #[test]
    fn test_field_declarations() {
        let source = r#"
            class Example {
                private int x;
                public static final String name = "test";
                protected boolean flag;
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_method_declaration() {
        let source = r#"
            class Calculator {
                public int add(int a, int b) {
                    return a + b;
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_return_new_array_expression() {
        let source = r#"
            class Arrays {
                int[] make(int size) {
                    return new int[size];
                }
            }
        "#;
        let result = parse_src(source);
        let class = result.arena.class_decl(result.ast.classes[0]);
        let method = class
            .members
            .iter()
            .find_map(|member_id| match result.arena.class_member(*member_id) {
                ClassMember::Method(method) => Some(method),
                _ => None,
            })
            .expect("method");
        let body_id = method.body.expect("method body");
        let Stmt::Block(statements) = result.arena.stmt(body_id) else {
            panic!("expected method body block");
        };
        let Stmt::Return(Some(expr_id)) = result.arena.stmt(statements[0]) else {
            panic!("expected return with expression");
        };
        let Expr::NewArray {
            dimensions,
            initializer,
            ..
        } = result.arena.expr(*expr_id)
        else {
            panic!("expected new array expression");
        };
        assert_eq!(dimensions.len(), 1);
        assert!(initializer.is_none());
    }

    #[test]
    fn test_return_multidimensional_new_array_expression() {
        let source = r#"
            class Arrays {
                int[][] make(int rows, int cols) {
                    return new int[rows][cols];
                }
            }
        "#;
        let result = parse_src(source);
        let class = result.arena.class_decl(result.ast.classes[0]);
        let method = class
            .members
            .iter()
            .find_map(|member_id| match result.arena.class_member(*member_id) {
                ClassMember::Method(method) => Some(method),
                _ => None,
            })
            .expect("method");
        let body_id = method.body.expect("method body");
        let Stmt::Block(statements) = result.arena.stmt(body_id) else {
            panic!("expected method body block");
        };
        let Stmt::Return(Some(expr_id)) = result.arena.stmt(statements[0]) else {
            panic!("expected return with expression");
        };
        let Expr::NewArray {
            dimensions,
            initializer,
            ..
        } = result.arena.expr(*expr_id)
        else {
            panic!("expected new array expression");
        };
        assert_eq!(dimensions.len(), 2);
        assert!(initializer.is_none());
    }

    #[test]
    fn test_return_new_array_initializer_expression() {
        let source = r#"
            class Arrays {
                int[] make() {
                    return new int[] { 1, 2, 3 };
                }
            }
        "#;
        let result = parse_src(source);
        let class = result.arena.class_decl(result.ast.classes[0]);
        let method = class
            .members
            .iter()
            .find_map(|member_id| match result.arena.class_member(*member_id) {
                ClassMember::Method(method) => Some(method),
                _ => None,
            })
            .expect("method");
        let body_id = method.body.expect("method body");
        let Stmt::Block(statements) = result.arena.stmt(body_id) else {
            panic!("expected method body block");
        };
        let Stmt::Return(Some(expr_id)) = result.arena.stmt(statements[0]) else {
            panic!("expected return with expression");
        };
        let Expr::NewArray {
            dimensions,
            initializer: Some(initializer),
            ..
        } = result.arena.expr(*expr_id)
        else {
            panic!("expected new array with initializer");
        };
        assert!(dimensions.is_empty());
        let Expr::ArrayInitializer { elements } = result.arena.expr(*initializer) else {
            panic!("expected array initializer");
        };
        assert_eq!(elements.len(), 3);
    }

    #[test]
    fn test_return_nested_array_initializer_expression() {
        let source = r#"
            class Arrays {
                int[][] make() {
                    return new int[][] { { 1 }, { 2, 3 } };
                }
            }
        "#;
        let result = parse_src(source);
        let class = result.arena.class_decl(result.ast.classes[0]);
        let method = class
            .members
            .iter()
            .find_map(|member_id| match result.arena.class_member(*member_id) {
                ClassMember::Method(method) => Some(method),
                _ => None,
            })
            .expect("method");
        let body_id = method.body.expect("method body");
        let Stmt::Block(statements) = result.arena.stmt(body_id) else {
            panic!("expected method body block");
        };
        let Stmt::Return(Some(expr_id)) = result.arena.stmt(statements[0]) else {
            panic!("expected return with expression");
        };
        let Expr::NewArray {
            initializer: Some(initializer),
            ..
        } = result.arena.expr(*expr_id)
        else {
            panic!("expected new array with initializer");
        };
        let Expr::ArrayInitializer { elements } = result.arena.expr(*initializer) else {
            panic!("expected outer array initializer");
        };
        assert_eq!(elements.len(), 2);
        assert!(matches!(
            result.arena.expr(elements[0]),
            Expr::ArrayInitializer { .. }
        ));
    }

    #[test]
    fn test_constructor_declaration() {
        let source = r#"
            class Person {
                private String name;
                
                public Person(String name) {
                    this.name = name;
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
        let class = result.arena.class_decl(result.ast.classes[0]);
        assert!(class.members.iter().any(|member_id| matches!(
            result.arena.class_member(*member_id),
            ClassMember::Constructor(_)
        )));
    }

    #[test]
    fn test_interface_declaration() {
        let source = r#"
            public interface Drawable {
                void draw();
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_enum_declaration() {
        let source = r#"
            public enum Color {
                RED, GREEN, BLUE
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
        let class = result.arena.class_decl(result.ast.classes[0]);
        assert_eq!(class.kind, ClassKind::Enum);
        assert_eq!(class.enum_entries.len(), 3);
        assert_eq!(class.enum_entries[0].name.as_str(), "RED");
    }

    #[test]
    fn test_generic_class() {
        let source = r#"
            public class Box<T> {
                private T content;
                
                public T get() {
                    return content;
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_extends_and_implements() {
        let source = r#"
            public class ArrayList<E> extends AbstractList<E> implements List<E> {
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_expression_literals() {
        let source = r#"
            class Test {
                int a = 42;
                long b = 100L;
                float c = 3.14f;
                double d = 2.71828;
                String s = "hello";
                char ch = 'x';
                boolean flag = true;
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);

        let class_decl = result.arena.class_decl(result.ast.classes[0]);
        let literal_kinds: Vec<_> = class_decl
            .members
            .iter()
            .filter_map(|member_id| match result.arena.class_member(*member_id) {
                ClassMember::Field(field) => {
                    field
                        .initializer
                        .map(|expr_id| match result.arena.expr(expr_id) {
                            Expr::Literal(literal) => literal.kind.clone(),
                            _ => panic!("expected literal initializer"),
                        })
                }
                _ => None,
            })
            .collect();

        assert_eq!(
            literal_kinds,
            vec![
                LiteralKind::Int,
                LiteralKind::Long,
                LiteralKind::Float,
                LiteralKind::Double,
                LiteralKind::String,
                LiteralKind::Char,
                LiteralKind::Bool,
            ]
        );
    }

    #[test]
    fn test_binary_operations() {
        let source = r#"
            class Calculator {
                void test() {
                    int x = 5 + 3 * 2;
                    int y = 10 - 4;
                    int z = 8 / 2;
                    boolean b = x > y && y < z;
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_assignment_expression_statement() {
        let source = r#"
            class Test {
                void test() {
                    int sum = 0;
                    sum = sum + 1;
                }
            }
        "#;
        let result = parse_src(source);
        let class_decl = result.arena.class_decl(result.ast.classes[0]);
        let ClassMember::Method(method) = result.arena.class_member(class_decl.members[0]) else {
            panic!("expected method");
        };
        let body_id = method.body.expect("expected method body");
        let Stmt::Block(stmts) = result.arena.stmt(body_id) else {
            panic!("expected block body");
        };
        let Stmt::Expr(expr_id) = result.arena.stmt(stmts[1]) else {
            panic!("expected expression statement");
        };

        assert!(matches!(result.arena.expr(*expr_id), Expr::Assign { .. }));
    }

    #[test]
    fn test_if_statement() {
        let source = r#"
            class Test {
                void test() {
                    if (x > 0) {
                        x = x - 1;
                    } else {
                        x = 0;
                    }
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_loop_statements() {
        let source = r#"
            class Test {
                void test() {
                    while (x > 0) {
                        x--;
                    }
                    
                    do {
                        y++;
                    } while (y < 10);
                    
                    for (int i = 0; i < 10; i++) {
                        sum += i;
                    }
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_switch_statement() {
        let source = r#"
            class Test {
                void test(int day) {
                    switch (day) {
                        case 1:
                            System.out.println("Monday");
                            break;
                        case 2:
                            System.out.println("Tuesday");
                            break;
                        default:
                            System.out.println("Other day");
                    }
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_labeled_statement() {
        let source = r#"
            class Test {
                void test(int limit) {
                    outer: while (limit > 0) {
                        limit--;
                        continue outer;
                    }
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_try_catch() {
        let source = r#"
            class Test {
                void test() {
                    try {
                        int x = Integer.parseInt("123");
                    } catch (NumberFormatException e) {
                        System.out.println("Invalid number");
                    } finally {
                        System.out.println("Done");
                    }
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_method_calls() {
        let source = r#"
            class Test {
                void test() {
                    String s = "hello";
                    int len = s.length();
                    System.out.println("Length: " + len);
                    list.add(42);
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_new_expression() {
        let source = r#"
            class Test {
                void test() {
                    Object obj = new Object();
                    List list = new ArrayList<>();
                    int[] arr = new int[10];
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_cast_expression() {
        let source = r#"
            class Test {
                void test() {
                    Object obj = "string";
                    String s = (String) obj;
                    int x = (int) 3.14;
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_ternary_expression() {
        let source = r#"
            class Test {
                void test() {
                    int x = 5 > 3 ? 1 : 0;
                    String msg = flag ? "yes" : "no";
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_assignment_operators() {
        let source = r#"
            class Test {
                void test() {
                    int x = 10;
                    x += 5;
                    x -= 3;
                    x *= 2;
                    x /= 4;
                    x %= 3;
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_nested_class() {
        let source = r#"
            public class Outer {
                public class Inner {
                    void innerMethod() {}
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_varargs_parameter() {
        let source = r#"
            class Test {
                void print(String... args) {
                    for (String arg : args) {
                        System.out.println(arg);
                    }
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }

    #[test]
    fn test_array_access() {
        let source = r#"
            class Test {
                void test() {
                    int[] arr = {1, 2, 3};
                    int first = arr[0];
                }
            }
        "#;
        let result = parse_src(source);
        assert_eq!(result.ast.classes.len(), 1);
    }
}
