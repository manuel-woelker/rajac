//! # Attribute Analysis Stage
//!
//! This module handles semantic analysis after name resolution and before
//! bytecode generation.
//!
//! ## What does this stage own?
//!
//! The attribute analysis stage is the compiler's semantic pass for resolved
//! ASTs. It folds constant sign literals, binds locals and parameters through
//! nested scopes, validates core typing rules, and emits semantic diagnostics.

/* 📖 # Why make attribute analysis the semantic owner for local typing?
Resolution can determine declared types and many member references, but it does
not model Java's local scopes or assignment rules. Centralizing those checks in
attribute analysis keeps bytecode generation from compensating for missing
semantic information and gives the compiler a single phase for type errors.
*/

use crate::CompilationUnit;
use rajac_ast::{
    Ast, AstArena, BinaryOp, ClassDecl, ClassDeclId, ClassMember, ClassMemberId, Constructor,
    EnumDecl, Expr, ExprId, Field, ForInit, Literal, LiteralKind, Method, Stmt, StmtId, UnaryOp,
};
use rajac_base::file_path::FilePath;
use rajac_base::logging::instrument;
use rajac_base::shared_string::SharedString;
use rajac_diagnostics::{Annotation, Diagnostic, Diagnostics, Severity, SourceChunk, Span};
use rajac_symbols::SymbolTable;
use rajac_types::{FieldId, Ident, MethodId, PrimitiveType, Type, TypeId};
use std::collections::{HashMap, HashSet};

/// Performs attribute analysis on resolved compilation units and returns the
/// diagnostics produced by the semantic pass.
#[instrument(
    name = "compiler.phase.attribute_analysis",
    skip(compilation_units, symbol_table),
    fields(compilation_units = compilation_units.len())
)]
pub fn analyze_attributes(
    compilation_units: &mut [CompilationUnit],
    symbol_table: &mut SymbolTable,
) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();

    for compilation_unit in compilation_units {
        analyze_compilation_unit(compilation_unit, symbol_table, &mut diagnostics);
    }

    diagnostics
}

fn analyze_compilation_unit(
    compilation_unit: &mut CompilationUnit,
    symbol_table: &SymbolTable,
    diagnostics: &mut Diagnostics,
) {
    fold_sign_literals(&compilation_unit.ast, &mut compilation_unit.arena);

    let mut analyzer = SemanticAnalyzer::new(
        &compilation_unit.source_file,
        compilation_unit.ast.source.as_str(),
        &mut compilation_unit.arena,
        symbol_table,
        package_name_from_ast(&compilation_unit.ast),
    );
    analyzer.analyze_ast(&compilation_unit.ast);

    let semantic_diagnostics = analyzer.finish();
    compilation_unit
        .diagnostics
        .extend(semantic_diagnostics.iter().cloned());
    diagnostics.extend(semantic_diagnostics);
}

struct SemanticAnalyzer<'a> {
    source_file: &'a FilePath,
    source: &'a str,
    arena: &'a mut AstArena,
    symbol_table: &'a SymbolTable,
    diagnostics: Diagnostics,
    scopes: Vec<HashMap<SharedString, TypeId>>,
    current_package: SharedString,
    current_class_type_id: Option<TypeId>,
    current_return_type: Option<TypeId>,
}

impl<'a> SemanticAnalyzer<'a> {
    fn new(
        source_file: &'a FilePath,
        source: &'a str,
        arena: &'a mut AstArena,
        symbol_table: &'a SymbolTable,
        current_package: SharedString,
    ) -> Self {
        Self {
            source_file,
            source,
            arena,
            symbol_table,
            diagnostics: Diagnostics::new(),
            scopes: Vec::new(),
            current_package,
            current_class_type_id: None,
            current_return_type: None,
        }
    }

    fn finish(self) -> Diagnostics {
        self.diagnostics
    }

    fn analyze_ast(&mut self, ast: &Ast) {
        self.push_scope();
        for stmt_id in &ast.statements {
            self.analyze_stmt(*stmt_id);
        }
        self.pop_scope();

        for class_id in &ast.classes {
            self.analyze_class_decl(*class_id);
        }
    }

    fn analyze_class_decl(&mut self, class_id: ClassDeclId) {
        let class = self.arena.class_decl(class_id).clone();
        let previous_class = self.current_class_type_id;
        self.current_class_type_id = self.lookup_class_type_id(&class);

        for member_id in class.members {
            self.analyze_class_member(member_id);
        }

        self.current_class_type_id = previous_class;
    }

    fn analyze_class_member(&mut self, member_id: ClassMemberId) {
        match self.arena.class_member(member_id).clone() {
            ClassMember::Field(field) => self.analyze_field(&field),
            ClassMember::Method(method) => self.analyze_method(&method),
            ClassMember::Constructor(constructor) => self.analyze_constructor(&constructor),
            ClassMember::StaticBlock(stmt_id) => {
                self.push_scope();
                self.analyze_stmt(stmt_id);
                self.pop_scope();
            }
            ClassMember::NestedClass(class_id)
            | ClassMember::NestedInterface(class_id)
            | ClassMember::NestedRecord(class_id)
            | ClassMember::NestedAnnotation(class_id) => self.analyze_class_decl(class_id),
            ClassMember::NestedEnum(enum_decl) => self.analyze_enum_decl(&enum_decl),
        }
    }

    fn analyze_enum_decl(&mut self, enum_decl: &EnumDecl) {
        for entry in &enum_decl.entries {
            for arg in &entry.args {
                self.analyze_expr(*arg);
            }

            if let Some(body) = &entry.body {
                for member_id in body {
                    self.analyze_class_member(*member_id);
                }
            }
        }

        for member_id in &enum_decl.members {
            self.analyze_class_member(*member_id);
        }
    }

    fn analyze_field(&mut self, field: &Field) {
        if let Some(initializer) = field.initializer {
            let initializer_ty = self.analyze_expr(initializer);
            let field_ty = self.arena.ty(field.ty).ty();
            self.check_assignment_compatibility(
                field_ty,
                initializer_ty,
                initializer,
                field.name.name.as_str(),
            );
        }
    }

    fn analyze_method(&mut self, method: &Method) {
        let previous_return_type = self.current_return_type;
        self.current_return_type = Some(self.arena.ty(method.return_ty).ty());
        self.push_scope();

        for param_id in &method.params {
            let param = self.arena.param(*param_id).clone();
            let param_ty = self.arena.ty(param.ty).ty();
            self.declare_local(param.name.name.clone(), param_ty, param.name.name.as_str());
        }

        if let Some(body) = method.body {
            self.analyze_stmt(body);
        }

        self.pop_scope();
        self.current_return_type = previous_return_type;
    }

    fn analyze_constructor(&mut self, constructor: &Constructor) {
        let previous_return_type = self.current_return_type;
        self.current_return_type = Some(
            self.symbol_table
                .primitive_type_id("void")
                .unwrap_or(TypeId::INVALID),
        );
        self.push_scope();

        for param_id in &constructor.params {
            let param = self.arena.param(*param_id).clone();
            let param_ty = self.arena.ty(param.ty).ty();
            self.declare_local(param.name.name.clone(), param_ty, param.name.name.as_str());
        }

        if let Some(body) = constructor.body {
            self.analyze_stmt(body);
        }

        self.pop_scope();
        self.current_return_type = previous_return_type;
    }

    fn analyze_stmt(&mut self, stmt_id: StmtId) {
        match self.arena.stmt(stmt_id).clone() {
            Stmt::Empty | Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Block(statements) => {
                self.push_scope();
                for nested_stmt_id in statements {
                    self.analyze_stmt(nested_stmt_id);
                }
                self.pop_scope();
            }
            Stmt::Expr(expr_id) => {
                self.analyze_expr(expr_id);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.require_boolean_condition(condition, "if");
                self.analyze_stmt(then_branch);
                if let Some(else_branch) = else_branch {
                    self.analyze_stmt(else_branch);
                }
            }
            Stmt::While { condition, body } => {
                self.require_boolean_condition(condition, "while");
                self.analyze_stmt(body);
            }
            Stmt::DoWhile { body, condition } => {
                self.analyze_stmt(body);
                self.require_boolean_condition(condition, "do-while");
            }
            Stmt::For {
                init,
                condition,
                update,
                body,
            } => {
                self.push_scope();
                if let Some(init) = init {
                    match init {
                        ForInit::Expr(expr_id) => {
                            self.analyze_expr(expr_id);
                        }
                        ForInit::LocalVar {
                            ty,
                            name,
                            initializer,
                        } => {
                            self.analyze_local_var(ty, name, initializer);
                        }
                    }
                }
                if let Some(condition) = condition {
                    self.require_boolean_condition(condition, "for");
                }
                if let Some(update) = update {
                    self.analyze_expr(update);
                }
                self.analyze_stmt(body);
                self.pop_scope();
            }
            Stmt::Switch { expr, cases } => {
                self.analyze_expr(expr);
                for case in cases {
                    for label in case.labels {
                        if let rajac_ast::SwitchLabel::Case(expr_id) = label {
                            self.analyze_expr(expr_id);
                        }
                    }
                    self.push_scope();
                    for body_stmt_id in case.body {
                        self.analyze_stmt(body_stmt_id);
                    }
                    self.pop_scope();
                }
            }
            Stmt::Return(expr) => {
                let Some(expected_ty) = self.current_return_type else {
                    return;
                };

                match expr {
                    Some(expr_id) => {
                        let expr_ty = self.analyze_expr(expr_id);
                        let void_ty = self
                            .symbol_table
                            .primitive_type_id("void")
                            .unwrap_or(TypeId::INVALID);

                        if expected_ty == void_ty {
                            self.emit_error(
                                "unexpected return value in method returning void",
                                Some("return"),
                            );
                        } else {
                            self.check_assignment_compatibility(
                                expected_ty,
                                expr_ty,
                                expr_id,
                                "return",
                            );
                        }
                    }
                    None => {
                        if expected_ty
                            != self
                                .symbol_table
                                .primitive_type_id("void")
                                .unwrap_or(TypeId::INVALID)
                        {
                            self.emit_error(
                                format!(
                                    "missing return value for method returning {}",
                                    self.type_display_name(expected_ty)
                                ),
                                Some("return"),
                            );
                        }
                    }
                }
            }
            Stmt::Label(_, stmt_id) => self.analyze_stmt(stmt_id),
            Stmt::Try {
                try_block,
                catches,
                finally_block,
            } => {
                self.analyze_stmt(try_block);
                for catch_clause in catches {
                    self.push_scope();
                    let param = self.arena.param(catch_clause.param).clone();
                    let param_ty = self.arena.ty(param.ty).ty();
                    self.declare_local(param.name.name.clone(), param_ty, param.name.name.as_str());
                    self.analyze_stmt(catch_clause.body);
                    self.pop_scope();
                }
                if let Some(finally_block) = finally_block {
                    self.analyze_stmt(finally_block);
                }
            }
            Stmt::Throw(expr_id) => {
                let expr_ty = self.analyze_expr(expr_id);
                let throwable_ty = self.symbol_table.lookup_type_id("java.lang", "Throwable");
                if expr_ty != TypeId::INVALID
                    && !self.is_reference_type(expr_ty)
                    && !self.is_null_literal(expr_id)
                {
                    self.emit_error(
                        format!(
                            "throw expression must be a reference type, found {}",
                            self.expr_type_display_name(expr_id, expr_ty)
                        ),
                        Some("throw"),
                    );
                } else if let Some(throwable_ty) = throwable_ty
                    && expr_ty != TypeId::INVALID
                    && !self.is_null_literal(expr_id)
                    && !self.is_reference_assignable(throwable_ty, expr_ty)
                {
                    self.emit_error(
                        format!(
                            "throw expression must be Throwable-compatible, found {}",
                            self.expr_type_display_name(expr_id, expr_ty)
                        ),
                        Some("throw"),
                    );
                }
            }
            Stmt::Synchronized { expr, block } => {
                if let Some(expr_id) = expr {
                    let expr_ty = self.analyze_expr(expr_id);
                    if expr_ty != TypeId::INVALID
                        && !self.is_reference_type(expr_ty)
                        && !self.is_null_literal(expr_id)
                    {
                        self.emit_error(
                            format!(
                                "synchronized expression must be a reference type, found {}",
                                self.expr_type_display_name(expr_id, expr_ty)
                            ),
                            Some("synchronized"),
                        );
                    }
                }
                self.analyze_stmt(block);
            }
            Stmt::LocalVar {
                ty,
                name,
                initializer,
            } => self.analyze_local_var(ty, name, initializer),
        }
    }

    fn analyze_local_var(
        &mut self,
        ty: rajac_ast::AstTypeId,
        name: Ident,
        initializer: Option<ExprId>,
    ) {
        let declared_ty = self.arena.ty(ty).ty();
        if let Some(initializer) = initializer {
            let initializer_ty = self.analyze_expr(initializer);
            self.check_assignment_compatibility(
                declared_ty,
                initializer_ty,
                initializer,
                name.name.as_str(),
            );
        }
        self.declare_local(name.name.clone(), declared_ty, name.name.as_str());
    }

    fn analyze_expr(&mut self, expr_id: ExprId) -> TypeId {
        let expr = self.arena.expr(expr_id).clone();
        let result_ty = match expr {
            Expr::Error => TypeId::INVALID,
            Expr::Ident(name) => self.analyze_ident_expr(expr_id, &name),
            Expr::Literal(literal) => literal_type_id(&literal, self.symbol_table),
            Expr::Unary { op, expr } => self.analyze_unary_expr(op, expr),
            Expr::Binary { op, lhs, rhs } => self.analyze_binary_expr(op, lhs, rhs),
            Expr::Assign { op, lhs, rhs } => self.analyze_assign_expr(op, lhs, rhs),
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => self.analyze_ternary_expr(condition, then_expr, else_expr),
            Expr::Cast { ty, expr } => self.analyze_cast_expr(ty, expr),
            Expr::InstanceOf { expr, ty } => self.analyze_instanceof_expr(expr, ty),
            Expr::FieldAccess {
                expr,
                name,
                field_id,
            } => self.analyze_field_access_expr(expr_id, expr, name, field_id),
            Expr::MethodCall {
                expr,
                name,
                type_args: _,
                args,
                method_id,
            } => self.analyze_method_call_expr(expr_id, expr, name, args, method_id),
            Expr::New { ty, args } => self.analyze_new_expr(ty, args),
            Expr::NewArray { ty, dimensions } => {
                self.analyze_new_array_expr(expr_id, ty, dimensions)
            }
            Expr::ArrayAccess { array, index } => self.analyze_array_access_expr(array, index),
            Expr::ArrayLength { array } => self.analyze_array_length_expr(array),
            Expr::This(expr) => {
                if let Some(expr_id) = expr {
                    self.analyze_expr(expr_id);
                }
                self.current_class_type_id.unwrap_or(TypeId::INVALID)
            }
            Expr::Super => self.superclass_type_id(),
            Expr::SuperCall {
                name,
                type_args: _,
                args,
                method_id,
            } => self.analyze_super_call_expr(expr_id, name, args, method_id),
        };

        self.arena.expr_typed_mut(expr_id).ty = result_ty;
        result_ty
    }

    fn analyze_ident_expr(&mut self, expr_id: ExprId, name: &Ident) -> TypeId {
        if let Some(local_ty) = self.lookup_local(&name.name) {
            return local_ty;
        }

        if let Some(field_id) = self
            .current_class_type_id
            .and_then(|class_ty| resolve_field_in_type(class_ty, &name.name, self.symbol_table))
        {
            return self.symbol_table.field_arena().get(field_id).ty;
        }

        let existing_ty = self.arena.expr_typed(expr_id).ty;
        if existing_ty != TypeId::INVALID {
            return existing_ty;
        }

        self.emit_error(
            format!("cannot find symbol '{}'", name.name.as_str()),
            Some(name.name.as_str()),
        );
        TypeId::INVALID
    }

    fn analyze_unary_expr(&mut self, op: UnaryOp, expr: ExprId) -> TypeId {
        let operand_ty = self.analyze_expr(expr);
        match op {
            UnaryOp::Plus | UnaryOp::Minus => {
                if self.is_numeric_type(operand_ty) {
                    operand_ty
                } else {
                    self.emit_error(
                        format!(
                            "bad operand type {} for unary operator {}",
                            self.expr_type_display_name(expr, operand_ty),
                            unary_operator_display(&op)
                        ),
                        Some(unary_operator_display(&op)),
                    );
                    TypeId::INVALID
                }
            }
            UnaryOp::Bang => {
                if self.is_boolean_type(operand_ty) {
                    operand_ty
                } else {
                    self.emit_error(
                        format!(
                            "bad operand type {} for unary operator !",
                            self.expr_type_display_name(expr, operand_ty)
                        ),
                        Some("!"),
                    );
                    TypeId::INVALID
                }
            }
            UnaryOp::Tilde => {
                if self.is_integral_type(operand_ty) {
                    operand_ty
                } else {
                    self.emit_error(
                        format!(
                            "bad operand type {} for unary operator ~",
                            self.expr_type_display_name(expr, operand_ty)
                        ),
                        Some("~"),
                    );
                    TypeId::INVALID
                }
            }
            UnaryOp::Increment | UnaryOp::Decrement => {
                if !self.is_assignable_expr(expr) {
                    self.emit_error(
                        format!(
                            "operator {} requires a variable",
                            unary_operator_display(&op)
                        ),
                        Some(unary_operator_display(&op)),
                    );
                    return TypeId::INVALID;
                }
                if self.is_numeric_type(operand_ty) {
                    operand_ty
                } else {
                    self.emit_error(
                        format!(
                            "bad operand type {} for unary operator {}",
                            self.expr_type_display_name(expr, operand_ty),
                            unary_operator_display(&op)
                        ),
                        Some(unary_operator_display(&op)),
                    );
                    TypeId::INVALID
                }
            }
        }
    }

    fn analyze_binary_expr(&mut self, op: BinaryOp, lhs: ExprId, rhs: ExprId) -> TypeId {
        let lhs_ty = self.analyze_expr(lhs);
        let rhs_ty = self.analyze_expr(rhs);

        match op {
            BinaryOp::Add => {
                if self.is_string_type(lhs_ty) || self.is_string_type(rhs_ty) {
                    return self
                        .symbol_table
                        .lookup_type_id("java.lang", "String")
                        .unwrap_or(TypeId::INVALID);
                }
                self.require_numeric_binary(op, lhs, lhs_ty, rhs, rhs_ty)
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                self.require_numeric_binary(op, lhs, lhs_ty, rhs, rhs_ty)
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                if self.is_boolean_type(lhs_ty) && self.is_boolean_type(rhs_ty) {
                    self.symbol_table
                        .primitive_type_id("boolean")
                        .unwrap_or(TypeId::INVALID)
                } else if self.is_integral_type(lhs_ty) && self.is_integral_type(rhs_ty) {
                    binary_numeric_promotion(lhs_ty, rhs_ty, self.symbol_table)
                } else {
                    self.emit_error(
                        format!(
                            "operator {} cannot be applied to {} and {}",
                            binary_operator_display(&op),
                            self.expr_type_display_name(lhs, lhs_ty),
                            self.expr_type_display_name(rhs, rhs_ty)
                        ),
                        Some(binary_operator_display(&op)),
                    );
                    TypeId::INVALID
                }
            }
            BinaryOp::LShift | BinaryOp::RShift | BinaryOp::ARShift => {
                if self.is_integral_type(lhs_ty) && self.is_integral_type(rhs_ty) {
                    lhs_ty
                } else {
                    self.emit_error(
                        format!(
                            "operator {} cannot be applied to {} and {}",
                            binary_operator_display(&op),
                            self.expr_type_display_name(lhs, lhs_ty),
                            self.expr_type_display_name(rhs, rhs_ty)
                        ),
                        Some(binary_operator_display(&op)),
                    );
                    TypeId::INVALID
                }
            }
            BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
                if self.is_numeric_type(lhs_ty) && self.is_numeric_type(rhs_ty) {
                    self.symbol_table
                        .primitive_type_id("boolean")
                        .unwrap_or(TypeId::INVALID)
                } else {
                    self.emit_error(
                        format!(
                            "operator {} cannot be applied to {} and {}",
                            binary_operator_display(&op),
                            self.expr_type_display_name(lhs, lhs_ty),
                            self.expr_type_display_name(rhs, rhs_ty)
                        ),
                        Some(binary_operator_display(&op)),
                    );
                    TypeId::INVALID
                }
            }
            BinaryOp::EqEq | BinaryOp::BangEq => {
                if self.are_equality_comparable(lhs_ty, rhs_ty, lhs, rhs) {
                    self.symbol_table
                        .primitive_type_id("boolean")
                        .unwrap_or(TypeId::INVALID)
                } else {
                    self.emit_error(
                        format!(
                            "operator {} cannot be applied to {} and {}",
                            binary_operator_display(&op),
                            self.expr_type_display_name(lhs, lhs_ty),
                            self.expr_type_display_name(rhs, rhs_ty)
                        ),
                        Some(binary_operator_display(&op)),
                    );
                    TypeId::INVALID
                }
            }
            BinaryOp::And | BinaryOp::Or => {
                if self.is_boolean_type(lhs_ty) && self.is_boolean_type(rhs_ty) {
                    self.symbol_table
                        .primitive_type_id("boolean")
                        .unwrap_or(TypeId::INVALID)
                } else {
                    self.emit_error(
                        format!(
                            "operator {} cannot be applied to {} and {}",
                            binary_operator_display(&op),
                            self.expr_type_display_name(lhs, lhs_ty),
                            self.expr_type_display_name(rhs, rhs_ty)
                        ),
                        Some(binary_operator_display(&op)),
                    );
                    TypeId::INVALID
                }
            }
        }
    }

    fn analyze_assign_expr(&mut self, op: rajac_ast::AssignOp, lhs: ExprId, rhs: ExprId) -> TypeId {
        let lhs_ty = self.analyze_expr(lhs);
        let rhs_ty = self.analyze_expr(rhs);

        if !self.is_assignable_expr(lhs) {
            self.emit_error("assignment target must be a variable", Some("="));
            return TypeId::INVALID;
        }

        if op == rajac_ast::AssignOp::Eq {
            self.check_assignment_compatibility(lhs_ty, rhs_ty, rhs, "=");
            return lhs_ty;
        }

        let synthetic_op = binary_op_for_assign(op);
        let compound_ty = self.analyze_binary_expr(synthetic_op, lhs, rhs);
        if compound_ty != TypeId::INVALID {
            self.check_assignment_compatibility(lhs_ty, compound_ty, rhs, "=");
            lhs_ty
        } else {
            TypeId::INVALID
        }
    }

    fn analyze_ternary_expr(
        &mut self,
        condition: ExprId,
        then_expr: ExprId,
        else_expr: ExprId,
    ) -> TypeId {
        self.require_boolean_condition(condition, "ternary");
        let then_ty = self.analyze_expr(then_expr);
        let else_ty = self.analyze_expr(else_expr);

        if then_ty == else_ty {
            return then_ty;
        }
        if self.is_numeric_type(then_ty) && self.is_numeric_type(else_ty) {
            return binary_numeric_promotion(then_ty, else_ty, self.symbol_table);
        }
        if self.is_reference_assignable(then_ty, else_ty) {
            return then_ty;
        }
        if self.is_reference_assignable(else_ty, then_ty) {
            return else_ty;
        }

        self.emit_error(
            format!(
                "incompatible types in ternary expression: {} and {}",
                self.expr_type_display_name(then_expr, then_ty),
                self.expr_type_display_name(else_expr, else_ty)
            ),
            Some("?"),
        );
        TypeId::INVALID
    }

    fn analyze_cast_expr(&mut self, ty: rajac_ast::AstTypeId, expr: ExprId) -> TypeId {
        let target_ty = self.arena.ty(ty).ty();
        let expr_ty = self.analyze_expr(expr);
        if target_ty != TypeId::INVALID
            && expr_ty != TypeId::INVALID
            && !self.is_cast_compatible(target_ty, expr_ty)
        {
            self.emit_error(
                format!(
                    "cannot cast {} to {}",
                    self.expr_type_display_name(expr, expr_ty),
                    self.type_display_name(target_ty)
                ),
                Some("("),
            );
        }
        target_ty
    }

    fn analyze_instanceof_expr(&mut self, expr: ExprId, ty: rajac_ast::AstTypeId) -> TypeId {
        let expr_ty = self.analyze_expr(expr);
        let target_ty = self.arena.ty(ty).ty();

        if expr_ty != TypeId::INVALID
            && !self.is_reference_type(expr_ty)
            && !self.is_null_literal(expr)
        {
            self.emit_error(
                format!(
                    "instanceof requires a reference operand, found {}",
                    self.expr_type_display_name(expr, expr_ty)
                ),
                Some("instanceof"),
            );
        }
        if target_ty != TypeId::INVALID && !self.is_reference_type(target_ty) {
            self.emit_error(
                format!(
                    "instanceof requires a reference type, found {}",
                    self.type_display_name(target_ty)
                ),
                Some("instanceof"),
            );
        }

        self.symbol_table
            .primitive_type_id("boolean")
            .unwrap_or(TypeId::INVALID)
    }

    fn analyze_field_access_expr(
        &mut self,
        expr_id: ExprId,
        expr: ExprId,
        name: Ident,
        field_id: Option<FieldId>,
    ) -> TypeId {
        let receiver_ty = self.analyze_expr(expr);
        if let Some(resolved_field_id) =
            resolve_field_in_type(receiver_ty, &name.name, self.symbol_table)
        {
            if let Expr::FieldAccess { field_id, .. } = self.arena.expr_mut(expr_id) {
                *field_id = Some(resolved_field_id);
            }
            return self.symbol_table.field_arena().get(resolved_field_id).ty;
        }

        let _ = field_id;
        self.emit_error(
            format!(
                "cannot find field '{}' on {}",
                name.name.as_str(),
                self.type_display_name(receiver_ty)
            ),
            Some(name.name.as_str()),
        );
        TypeId::INVALID
    }

    fn analyze_method_call_expr(
        &mut self,
        expr_id: ExprId,
        expr: Option<ExprId>,
        name: Ident,
        args: Vec<ExprId>,
        method_id: Option<MethodId>,
    ) -> TypeId {
        let receiver_ty = expr
            .map(|receiver| self.analyze_expr(receiver))
            .or(self.current_class_type_id)
            .unwrap_or(TypeId::INVALID);
        let arg_types = args
            .iter()
            .map(|arg| self.analyze_expr(*arg))
            .collect::<Vec<_>>();

        if let Some(resolved_method_id) =
            resolve_method_in_type(receiver_ty, &name.name, &arg_types, self.symbol_table)
        {
            if let Expr::MethodCall { method_id, .. } = self.arena.expr_mut(expr_id) {
                *method_id = Some(resolved_method_id);
            }

            if let Some(receiver_expr) = expr
                && self.receiver_is_type_name(receiver_expr)
                && !self
                    .symbol_table
                    .method_arena()
                    .get(resolved_method_id)
                    .modifiers
                    .is_static()
            {
                self.emit_error(
                    format!(
                        "non-static method '{}' cannot be referenced from a static context",
                        name.name.as_str()
                    ),
                    Some(name.name.as_str()),
                );
                return TypeId::INVALID;
            }

            return self
                .symbol_table
                .method_arena()
                .get(resolved_method_id)
                .return_type;
        }

        let _ = method_id;
        self.emit_error(
            format!(
                "no applicable method '{}' for argument types ({})",
                name.name.as_str(),
                arg_types
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| {
                        let _ = index;
                        self.type_display_name(*ty)
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some(name.name.as_str()),
        );
        TypeId::INVALID
    }

    fn analyze_new_expr(&mut self, ty: rajac_ast::AstTypeId, args: Vec<ExprId>) -> TypeId {
        let target_ty = self.arena.ty(ty).ty();
        let arg_types = args
            .iter()
            .map(|arg| self.analyze_expr(*arg))
            .collect::<Vec<_>>();

        if target_ty == TypeId::INVALID {
            return TypeId::INVALID;
        }

        let constructor_name = match self.symbol_table.type_arena().get(target_ty) {
            Type::Class(class_type) => class_type.name.clone(),
            _ => SharedString::new("<init>"),
        };

        if resolve_method_in_type(target_ty, &constructor_name, &arg_types, self.symbol_table)
            .is_none()
            && target_ty != TypeId::INVALID
        {
            self.emit_error(
                format!(
                    "no applicable constructor for {}({})",
                    self.type_display_name(target_ty),
                    arg_types
                        .iter()
                        .map(|ty| self.type_display_name(*ty))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Some("new"),
            );
        }

        target_ty
    }

    fn analyze_new_array_expr(
        &mut self,
        expr_id: ExprId,
        ty: rajac_ast::AstTypeId,
        dimensions: Vec<ExprId>,
    ) -> TypeId {
        for dimension in &dimensions {
            let dimension_ty = self.analyze_expr(*dimension);
            if !self.is_int_compatible_type(dimension_ty) {
                self.emit_error(
                    format!(
                        "array dimension must be int-compatible, found {}",
                        self.expr_type_display_name(*dimension, dimension_ty)
                    ),
                    Some("new"),
                );
            }
        }

        let typed_ty = self.arena.expr_typed(expr_id).ty;
        if typed_ty != TypeId::INVALID {
            return typed_ty;
        }

        let _ = ty;
        TypeId::INVALID
    }

    fn analyze_array_access_expr(&mut self, array: ExprId, index: ExprId) -> TypeId {
        let array_ty = self.analyze_expr(array);
        let index_ty = self.analyze_expr(index);

        if !self.is_int_compatible_type(index_ty) {
            self.emit_error(
                format!(
                    "array index must be int-compatible, found {}",
                    self.expr_type_display_name(index, index_ty)
                ),
                Some("["),
            );
        }

        array_element_type(array_ty, self.symbol_table)
    }

    fn analyze_array_length_expr(&mut self, array: ExprId) -> TypeId {
        let array_ty = self.analyze_expr(array);
        if array_ty == TypeId::INVALID
            || !matches!(self.symbol_table.type_arena().get(array_ty), Type::Array(_))
        {
            self.emit_error(
                format!(
                    "length field is only available on arrays, found {}",
                    self.expr_type_display_name(array, array_ty)
                ),
                Some("length"),
            );
        }

        self.symbol_table
            .primitive_type_id("int")
            .unwrap_or(TypeId::INVALID)
    }

    fn analyze_super_call_expr(
        &mut self,
        expr_id: ExprId,
        name: Ident,
        args: Vec<ExprId>,
        method_id: Option<MethodId>,
    ) -> TypeId {
        let receiver_ty = self.superclass_type_id();
        let arg_types = args
            .iter()
            .map(|arg| self.analyze_expr(*arg))
            .collect::<Vec<_>>();

        if let Some(resolved_method_id) =
            resolve_method_in_type(receiver_ty, &name.name, &arg_types, self.symbol_table)
        {
            if let Expr::SuperCall { method_id, .. } = self.arena.expr_mut(expr_id) {
                *method_id = Some(resolved_method_id);
            }
            return self
                .symbol_table
                .method_arena()
                .get(resolved_method_id)
                .return_type;
        }

        let _ = method_id;
        self.emit_error(
            format!("cannot find super method '{}'", name.name.as_str()),
            Some(name.name.as_str()),
        );
        TypeId::INVALID
    }

    fn require_boolean_condition(&mut self, expr_id: ExprId, construct: &str) {
        let condition_ty = self.analyze_expr(expr_id);
        if !self.is_boolean_type(condition_ty) {
            self.emit_error(
                format!(
                    "{construct} condition must be boolean, found {}",
                    self.expr_type_display_name(expr_id, condition_ty)
                ),
                Some(construct),
            );
        }
    }

    fn require_numeric_binary(
        &mut self,
        op: BinaryOp,
        lhs: ExprId,
        lhs_ty: TypeId,
        rhs: ExprId,
        rhs_ty: TypeId,
    ) -> TypeId {
        if self.is_numeric_type(lhs_ty) && self.is_numeric_type(rhs_ty) {
            binary_numeric_promotion(lhs_ty, rhs_ty, self.symbol_table)
        } else {
            self.emit_error(
                format!(
                    "operator {} cannot be applied to {} and {}",
                    binary_operator_display(&op),
                    self.expr_type_display_name(lhs, lhs_ty),
                    self.expr_type_display_name(rhs, rhs_ty)
                ),
                Some(binary_operator_display(&op)),
            );
            TypeId::INVALID
        }
    }

    fn check_assignment_compatibility(
        &mut self,
        target_ty: TypeId,
        source_ty: TypeId,
        source_expr: ExprId,
        marker: &str,
    ) -> bool {
        if self.is_assignment_compatible(target_ty, source_ty, source_expr) {
            return true;
        }

        self.emit_error(
            format!(
                "incompatible types: found {}, required {}",
                self.expr_type_display_name(source_expr, source_ty),
                self.type_display_name(target_ty)
            ),
            Some(marker),
        );
        false
    }

    fn is_assignment_compatible(
        &self,
        target_ty: TypeId,
        source_ty: TypeId,
        source_expr: ExprId,
    ) -> bool {
        if target_ty == TypeId::INVALID || source_ty == TypeId::INVALID {
            return true;
        }
        if target_ty == source_ty {
            return true;
        }
        if self.is_null_literal(source_expr) {
            return self.is_reference_type(target_ty);
        }
        if let (Some(target_primitive), Some(source_primitive)) = (
            self.primitive_type(target_ty),
            self.primitive_type(source_ty),
        ) {
            if matches!(
                target_primitive,
                PrimitiveType::Byte | PrimitiveType::Short | PrimitiveType::Char
            ) && self
                .constant_integer_value(source_expr)
                .is_some_and(|value| constant_fits_primitive(value, &target_primitive))
            {
                return true;
            }
            return primitive_assignable_from(target_primitive, source_primitive);
        }
        self.is_reference_assignable(target_ty, source_ty)
    }

    fn is_reference_assignable(&self, target_ty: TypeId, source_ty: TypeId) -> bool {
        if target_ty == source_ty {
            return true;
        }
        if !self.is_reference_type(target_ty) || !self.is_reference_type(source_ty) {
            return false;
        }

        let mut stack = vec![source_ty];
        let mut visited = HashSet::new();
        while let Some(current_ty) = stack.pop() {
            if !visited.insert(current_ty) {
                continue;
            }
            if current_ty == target_ty {
                return true;
            }

            match self.symbol_table.type_arena().get(current_ty) {
                Type::Class(class_type) => {
                    if let Some(superclass) = class_type.superclass {
                        stack.push(superclass);
                    }
                    for interface in &class_type.interfaces {
                        stack.push(*interface);
                    }
                }
                Type::Array(_) => {
                    if let Some(object_ty) = self.symbol_table.lookup_type_id("java.lang", "Object")
                    {
                        stack.push(object_ty);
                    }
                }
                _ => {}
            }
        }

        false
    }

    fn are_equality_comparable(
        &self,
        lhs_ty: TypeId,
        rhs_ty: TypeId,
        lhs_expr: ExprId,
        rhs_expr: ExprId,
    ) -> bool {
        if lhs_ty == TypeId::INVALID || rhs_ty == TypeId::INVALID {
            return true;
        }
        if self.is_numeric_type(lhs_ty) && self.is_numeric_type(rhs_ty) {
            return true;
        }
        if self.is_boolean_type(lhs_ty) && self.is_boolean_type(rhs_ty) {
            return true;
        }
        if self.is_null_literal(lhs_expr) && self.is_reference_type(rhs_ty) {
            return true;
        }
        if self.is_null_literal(rhs_expr) && self.is_reference_type(lhs_ty) {
            return true;
        }
        self.is_reference_assignable(lhs_ty, rhs_ty) || self.is_reference_assignable(rhs_ty, lhs_ty)
    }

    fn is_cast_compatible(&self, target_ty: TypeId, source_ty: TypeId) -> bool {
        if target_ty == source_ty || target_ty == TypeId::INVALID || source_ty == TypeId::INVALID {
            return true;
        }
        match (
            self.primitive_type(target_ty),
            self.primitive_type(source_ty),
        ) {
            (Some(target), Some(source)) => {
                if target == PrimitiveType::Boolean || source == PrimitiveType::Boolean {
                    target == source
                } else {
                    true
                }
            }
            (None, None) => true,
            _ => false,
        }
    }

    fn receiver_is_type_name(&self, expr_id: ExprId) -> bool {
        let Expr::Ident(name) = self.arena.expr(expr_id) else {
            return false;
        };
        let receiver_ty = self.arena.expr_typed(expr_id).ty;
        if receiver_ty == TypeId::INVALID {
            return false;
        }
        self.lookup_local(&name.name).is_none()
            && self
                .current_class_type_id
                .and_then(|class_ty| resolve_field_in_type(class_ty, &name.name, self.symbol_table))
                .is_none()
            && matches!(
                self.symbol_table.type_arena().get(receiver_ty),
                Type::Class(_)
            )
    }

    fn lookup_class_type_id(&self, class: &ClassDecl) -> Option<TypeId> {
        self.symbol_table
            .lookup_type_id(self.current_package.as_str(), class.name.name.as_str())
    }

    fn lookup_local(&self, name: &SharedString) -> Option<TypeId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn declare_local(&mut self, name: SharedString, ty: TypeId, marker: &str) {
        let scope = self
            .scopes
            .last_mut()
            .expect("scope stack must not be empty");
        if scope.contains_key(&name) {
            self.emit_error(
                format!("duplicate local variable '{}'", name.as_str()),
                Some(marker),
            );
            return;
        }
        scope.insert(name, ty);
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn primitive_type(&self, ty: TypeId) -> Option<PrimitiveType> {
        if ty == TypeId::INVALID {
            return None;
        }
        match self.symbol_table.type_arena().get(ty) {
            Type::Primitive(kind) => Some(kind.clone()),
            _ => None,
        }
    }

    fn is_numeric_type(&self, ty: TypeId) -> bool {
        self.primitive_type(ty)
            .is_some_and(|kind| kind.is_numeric())
    }

    fn is_integral_type(&self, ty: TypeId) -> bool {
        self.primitive_type(ty)
            .is_some_and(|kind| kind.is_integral() && kind != PrimitiveType::Boolean)
    }

    fn is_boolean_type(&self, ty: TypeId) -> bool {
        self.primitive_type(ty) == Some(PrimitiveType::Boolean)
    }

    fn is_reference_type(&self, ty: TypeId) -> bool {
        if ty == TypeId::INVALID {
            return false;
        }
        matches!(
            self.symbol_table.type_arena().get(ty),
            Type::Class(_) | Type::Array(_)
        )
    }

    fn is_string_type(&self, ty: TypeId) -> bool {
        if ty == TypeId::INVALID {
            return false;
        }
        self.symbol_table.lookup_type_id("java.lang", "String") == Some(ty)
    }

    fn is_int_compatible_type(&self, ty: TypeId) -> bool {
        matches!(
            self.primitive_type(ty),
            Some(PrimitiveType::Byte)
                | Some(PrimitiveType::Char)
                | Some(PrimitiveType::Short)
                | Some(PrimitiveType::Int)
        )
    }

    fn is_assignable_expr(&self, expr_id: ExprId) -> bool {
        match self.arena.expr(expr_id) {
            Expr::Ident(name) => {
                self.lookup_local(&name.name).is_some()
                    || self
                        .current_class_type_id
                        .and_then(|class_ty| {
                            resolve_field_in_type(class_ty, &name.name, self.symbol_table)
                        })
                        .is_some()
            }
            Expr::FieldAccess { .. } | Expr::ArrayAccess { .. } => true,
            _ => false,
        }
    }

    fn is_null_literal(&self, expr_id: ExprId) -> bool {
        matches!(
            self.arena.expr(expr_id),
            Expr::Literal(Literal {
                kind: LiteralKind::Null,
                ..
            })
        )
    }

    fn constant_integer_value(&self, expr_id: ExprId) -> Option<i128> {
        match self.arena.expr(expr_id) {
            Expr::Literal(literal) => literal_integer_value(literal),
            Expr::Unary { op, expr } => {
                let value = self.constant_integer_value(*expr)?;
                match op {
                    UnaryOp::Plus => Some(value),
                    UnaryOp::Minus => Some(-value),
                    UnaryOp::Tilde => Some(!value),
                    _ => None,
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let lhs = self.constant_integer_value(*lhs)?;
                let rhs = self.constant_integer_value(*rhs)?;
                match op {
                    BinaryOp::Add => Some(lhs + rhs),
                    BinaryOp::Sub => Some(lhs - rhs),
                    BinaryOp::Mul => Some(lhs * rhs),
                    BinaryOp::Div => Some(lhs / rhs),
                    BinaryOp::Mod => Some(lhs % rhs),
                    BinaryOp::BitAnd => Some(lhs & rhs),
                    BinaryOp::BitOr => Some(lhs | rhs),
                    BinaryOp::BitXor => Some(lhs ^ rhs),
                    BinaryOp::LShift => Some(lhs << rhs),
                    BinaryOp::RShift | BinaryOp::ARShift => Some(lhs >> rhs),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn expr_type_display_name(&self, expr_id: ExprId, ty: TypeId) -> String {
        if self.is_null_literal(expr_id) {
            return "null".to_owned();
        }
        self.type_display_name(ty)
    }

    fn type_display_name(&self, ty: TypeId) -> String {
        if ty == TypeId::INVALID {
            return "<error>".to_owned();
        }

        match self.symbol_table.type_arena().get(ty) {
            Type::Primitive(kind) => primitive_type_display(kind).to_owned(),
            Type::Class(class_type) => class_type.name.as_str().to_owned(),
            Type::Array(array_type) => {
                format!("{}[]", self.type_display_name(array_type.element_type))
            }
            Type::TypeVariable(type_variable) => type_variable.name.as_str().to_owned(),
            Type::Wildcard(_) => "?".to_owned(),
            Type::Error => "<error>".to_owned(),
        }
    }

    fn superclass_type_id(&self) -> TypeId {
        superclass_type_id(self.current_class_type_id, self.symbol_table)
    }

    fn emit_error(&mut self, message: impl Into<String>, marker: Option<&str>) {
        let message = message.into();
        let chunk = source_chunk_for_marker(self.source_file, self.source, marker);
        self.diagnostics.add(Diagnostic {
            severity: Severity::Error,
            message: SharedString::new(&message),
            chunks: vec![chunk],
        });
    }
}

fn fold_sign_literals(ast: &Ast, arena: &mut AstArena) {
    for stmt_id in &ast.statements {
        fold_stmt_sign_literals(*stmt_id, arena);
    }

    for class_id in &ast.classes {
        fold_class_sign_literals(*class_id, arena);
    }
}

fn fold_class_sign_literals(class_id: ClassDeclId, arena: &mut AstArena) {
    let members = arena.class_decl(class_id).members.clone();

    for member_id in members {
        fold_class_member_sign_literals(member_id, arena);
    }
}

fn fold_class_member_sign_literals(member_id: ClassMemberId, arena: &mut AstArena) {
    let member = arena.class_member(member_id).clone();

    match member {
        ClassMember::Field(field) => fold_field_sign_literals(&field, arena),
        ClassMember::Method(method) => fold_method_sign_literals(&method, arena),
        ClassMember::Constructor(constructor) => {
            if let Some(body) = constructor.body {
                fold_stmt_sign_literals(body, arena);
            }
        }
        ClassMember::StaticBlock(stmt_id) => fold_stmt_sign_literals(stmt_id, arena),
        ClassMember::NestedClass(class_id)
        | ClassMember::NestedInterface(class_id)
        | ClassMember::NestedRecord(class_id)
        | ClassMember::NestedAnnotation(class_id) => fold_class_sign_literals(class_id, arena),
        ClassMember::NestedEnum(enum_decl) => fold_enum_sign_literals(&enum_decl, arena),
    }
}

fn fold_field_sign_literals(field: &Field, arena: &mut AstArena) {
    if let Some(initializer) = field.initializer {
        fold_expr_sign_literals(initializer, arena);
    }
}

fn fold_method_sign_literals(method: &Method, arena: &mut AstArena) {
    if let Some(body) = method.body {
        fold_stmt_sign_literals(body, arena);
    }
}

fn fold_enum_sign_literals(enum_decl: &EnumDecl, arena: &mut AstArena) {
    for entry in &enum_decl.entries {
        for arg in &entry.args {
            fold_expr_sign_literals(*arg, arena);
        }

        if let Some(body) = &entry.body {
            for member_id in body {
                fold_class_member_sign_literals(*member_id, arena);
            }
        }
    }

    for member_id in &enum_decl.members {
        fold_class_member_sign_literals(*member_id, arena);
    }
}

fn fold_stmt_sign_literals(stmt_id: StmtId, arena: &mut AstArena) {
    let stmt = arena.stmt(stmt_id).clone();

    match stmt {
        Stmt::Empty | Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::Block(statements) => {
            for nested_stmt_id in statements {
                fold_stmt_sign_literals(nested_stmt_id, arena);
            }
        }
        Stmt::Expr(expr_id) | Stmt::Throw(expr_id) => fold_expr_sign_literals(expr_id, arena),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            fold_expr_sign_literals(condition, arena);
            fold_stmt_sign_literals(then_branch, arena);
            if let Some(else_branch) = else_branch {
                fold_stmt_sign_literals(else_branch, arena);
            }
        }
        Stmt::While { condition, body } => {
            fold_expr_sign_literals(condition, arena);
            fold_stmt_sign_literals(body, arena);
        }
        Stmt::DoWhile { body, condition } => {
            fold_stmt_sign_literals(body, arena);
            fold_expr_sign_literals(condition, arena);
        }
        Stmt::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                match init {
                    ForInit::Expr(expr_id) => fold_expr_sign_literals(expr_id, arena),
                    ForInit::LocalVar { initializer, .. } => {
                        if let Some(initializer) = initializer {
                            fold_expr_sign_literals(initializer, arena);
                        }
                    }
                }
            }

            if let Some(condition) = condition {
                fold_expr_sign_literals(condition, arena);
            }

            if let Some(update) = update {
                fold_expr_sign_literals(update, arena);
            }

            fold_stmt_sign_literals(body, arena);
        }
        Stmt::Switch { expr, cases } => {
            fold_expr_sign_literals(expr, arena);
            for case in cases {
                for label in case.labels {
                    if let rajac_ast::SwitchLabel::Case(expr_id) = label {
                        fold_expr_sign_literals(expr_id, arena);
                    }
                }
                for body_stmt_id in case.body {
                    fold_stmt_sign_literals(body_stmt_id, arena);
                }
            }
        }
        Stmt::Return(expr) => {
            if let Some(expr_id) = expr {
                fold_expr_sign_literals(expr_id, arena);
            }
        }
        Stmt::Label(_, stmt_id) => fold_stmt_sign_literals(stmt_id, arena),
        Stmt::Try {
            try_block,
            catches,
            finally_block,
        } => {
            fold_stmt_sign_literals(try_block, arena);
            for catch_clause in catches {
                fold_stmt_sign_literals(catch_clause.body, arena);
            }
            if let Some(finally_block) = finally_block {
                fold_stmt_sign_literals(finally_block, arena);
            }
        }
        Stmt::Synchronized { expr, block } => {
            if let Some(expr_id) = expr {
                fold_expr_sign_literals(expr_id, arena);
            }
            fold_stmt_sign_literals(block, arena);
        }
        Stmt::LocalVar { initializer, .. } => {
            if let Some(expr_id) = initializer {
                fold_expr_sign_literals(expr_id, arena);
            }
        }
    }
}

fn fold_expr_sign_literals(expr_id: ExprId, arena: &mut AstArena) {
    let expr = arena.expr(expr_id).clone();

    match expr {
        Expr::Error | Expr::Ident(_) | Expr::Literal(_) | Expr::Super => {}
        Expr::Unary { op, expr } => {
            fold_expr_sign_literals(expr, arena);

            if let Some(literal) = fold_signed_literal(op, expr, arena) {
                arena.expr_typed_mut(expr_id).expr = Expr::Literal(literal);
            }
        }
        Expr::Binary { lhs, rhs, .. } | Expr::Assign { lhs, rhs, .. } => {
            fold_expr_sign_literals(lhs, arena);
            fold_expr_sign_literals(rhs, arena);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            fold_expr_sign_literals(condition, arena);
            fold_expr_sign_literals(then_expr, arena);
            fold_expr_sign_literals(else_expr, arena);
        }
        Expr::Cast { expr, .. } | Expr::InstanceOf { expr, .. } => {
            fold_expr_sign_literals(expr, arena);
        }
        Expr::FieldAccess { expr, .. } => fold_expr_sign_literals(expr, arena),
        Expr::MethodCall { expr, args, .. } => {
            if let Some(expr_id) = expr {
                fold_expr_sign_literals(expr_id, arena);
            }
            for arg in args {
                fold_expr_sign_literals(arg, arena);
            }
        }
        Expr::New { args, .. } => {
            for arg in args {
                fold_expr_sign_literals(arg, arena);
            }
        }
        Expr::NewArray { dimensions, .. } => {
            for dimension in dimensions {
                fold_expr_sign_literals(dimension, arena);
            }
        }
        Expr::ArrayAccess { array, index } => {
            fold_expr_sign_literals(array, arena);
            fold_expr_sign_literals(index, arena);
        }
        Expr::ArrayLength { array } | Expr::This(Some(array)) => {
            fold_expr_sign_literals(array, arena);
        }
        Expr::This(None) => {}
        Expr::SuperCall { args, .. } => {
            for arg in args {
                fold_expr_sign_literals(arg, arena);
            }
        }
    }
}

fn fold_signed_literal(op: UnaryOp, expr_id: ExprId, arena: &AstArena) -> Option<Literal> {
    let Expr::Literal(literal) = arena.expr(expr_id) else {
        return None;
    };

    if !matches!(
        literal.kind,
        LiteralKind::Int | LiteralKind::Long | LiteralKind::Float | LiteralKind::Double
    ) {
        return None;
    }

    let folded_value = match op {
        UnaryOp::Plus => normalize_signed_literal(literal.value.as_str()),
        UnaryOp::Minus => negate_signed_literal(literal.value.as_str()),
        _ => return None,
    };

    Some(Literal {
        kind: literal.kind.clone(),
        value: SharedString::new(&folded_value),
    })
}

fn normalize_signed_literal(value: &str) -> String {
    if let Some(value) = value.strip_prefix('+') {
        value.to_owned()
    } else {
        value.to_owned()
    }
}

fn negate_signed_literal(value: &str) -> String {
    if let Some(value) = value.strip_prefix('-') {
        value.to_owned()
    } else if let Some(value) = value.strip_prefix('+') {
        format!("-{value}")
    } else {
        format!("-{value}")
    }
}

fn literal_type_id(literal: &Literal, symbol_table: &SymbolTable) -> TypeId {
    match literal.kind {
        LiteralKind::Int => symbol_table.primitive_type_id("int"),
        LiteralKind::Long => symbol_table.primitive_type_id("long"),
        LiteralKind::Float => symbol_table.primitive_type_id("float"),
        LiteralKind::Double => symbol_table.primitive_type_id("double"),
        LiteralKind::Char => symbol_table.primitive_type_id("char"),
        LiteralKind::Bool => symbol_table.primitive_type_id("boolean"),
        LiteralKind::String => symbol_table.lookup_type_id("java.lang", "String"),
        LiteralKind::Null => None,
    }
    .unwrap_or(TypeId::INVALID)
}

fn package_name_from_ast(ast: &Ast) -> SharedString {
    ast.package
        .as_ref()
        .map(|package| {
            SharedString::new(
                package
                    .name
                    .segments
                    .iter()
                    .map(|segment| segment.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
            )
        })
        .unwrap_or_else(|| SharedString::new(""))
}

fn resolve_method_in_type(
    type_id: TypeId,
    name: &SharedString,
    arg_types: &[TypeId],
    symbol_table: &SymbolTable,
) -> Option<MethodId> {
    if type_id == TypeId::INVALID {
        return None;
    }

    let type_arena = symbol_table.type_arena();
    let mut stack = vec![type_id];
    let mut visited = HashSet::new();

    while let Some(current_id) = stack.pop() {
        if !visited.insert(current_id) {
            continue;
        }
        if let Type::Class(class_type) = type_arena.get(current_id) {
            if let Some(methods) = class_type.methods.get(name)
                && let Some(method_id) = select_method_by_args(methods, arg_types, symbol_table)
            {
                return Some(method_id);
            }
            if let Some(super_id) = class_type.superclass {
                stack.push(super_id);
            }
            for interface_id in &class_type.interfaces {
                stack.push(*interface_id);
            }
        }
    }

    None
}

fn select_method_by_args(
    methods: &[MethodId],
    arg_types: &[TypeId],
    symbol_table: &SymbolTable,
) -> Option<MethodId> {
    methods.iter().copied().find(|method_id| {
        let signature = symbol_table.method_arena().get(*method_id);
        if signature.params.len() != arg_types.len() {
            return false;
        }
        signature
            .params
            .iter()
            .zip(arg_types)
            .all(|(param, arg)| method_argument_assignable(*param, *arg, symbol_table))
    })
}

fn method_argument_assignable(
    param_ty: TypeId,
    arg_ty: TypeId,
    symbol_table: &SymbolTable,
) -> bool {
    if param_ty == TypeId::INVALID || arg_ty == TypeId::INVALID || param_ty == arg_ty {
        return true;
    }

    match (
        symbol_table.type_arena().get(param_ty),
        symbol_table.type_arena().get(arg_ty),
    ) {
        (Type::Primitive(param_kind), Type::Primitive(arg_kind)) => {
            primitive_assignable_from(param_kind.clone(), arg_kind.clone())
        }
        (Type::Class(_), Type::Class(_))
        | (Type::Class(_), Type::Array(_))
        | (Type::Array(_), Type::Array(_)) => reference_assignable(param_ty, arg_ty, symbol_table),
        _ => false,
    }
}

fn resolve_field_in_type(
    type_id: TypeId,
    name: &SharedString,
    symbol_table: &SymbolTable,
) -> Option<FieldId> {
    if type_id == TypeId::INVALID {
        return None;
    }

    let type_arena = symbol_table.type_arena();
    let mut stack = vec![type_id];
    let mut visited = HashSet::new();

    while let Some(current_id) = stack.pop() {
        if !visited.insert(current_id) {
            continue;
        }
        if let Type::Class(class_type) = type_arena.get(current_id) {
            if let Some(fields) = class_type.fields.get(name)
                && let Some(field_id) = fields.first()
            {
                return Some(*field_id);
            }
            if let Some(super_id) = class_type.superclass {
                stack.push(super_id);
            }
            for interface_id in &class_type.interfaces {
                stack.push(*interface_id);
            }
        }
    }

    None
}

fn array_element_type(array_type_id: TypeId, symbol_table: &SymbolTable) -> TypeId {
    if array_type_id == TypeId::INVALID {
        return TypeId::INVALID;
    }
    match symbol_table.type_arena().get(array_type_id) {
        Type::Array(array) => array.element_type,
        _ => TypeId::INVALID,
    }
}

fn superclass_type_id(current_class_type_id: Option<TypeId>, symbol_table: &SymbolTable) -> TypeId {
    let current_id = match current_class_type_id {
        Some(id) => id,
        None => return TypeId::INVALID,
    };

    match symbol_table.type_arena().get(current_id) {
        Type::Class(class_type) => class_type.superclass.unwrap_or(TypeId::INVALID),
        _ => TypeId::INVALID,
    }
}

fn binary_numeric_promotion(lhs_ty: TypeId, rhs_ty: TypeId, symbol_table: &SymbolTable) -> TypeId {
    if lhs_ty == TypeId::INVALID || rhs_ty == TypeId::INVALID {
        return TypeId::INVALID;
    }
    let lhs = symbol_table.type_arena().get(lhs_ty);
    let rhs = symbol_table.type_arena().get(rhs_ty);
    let result_name =
        match (lhs, rhs) {
            (Type::Primitive(PrimitiveType::Double), _)
            | (_, Type::Primitive(PrimitiveType::Double)) => "double",
            (Type::Primitive(PrimitiveType::Float), _)
            | (_, Type::Primitive(PrimitiveType::Float)) => "float",
            (Type::Primitive(PrimitiveType::Long), _)
            | (_, Type::Primitive(PrimitiveType::Long)) => "long",
            _ => "int",
        };

    symbol_table
        .primitive_type_id(result_name)
        .unwrap_or(TypeId::INVALID)
}

fn primitive_assignable_from(target: PrimitiveType, source: PrimitiveType) -> bool {
    if target == source {
        return true;
    }

    matches!(
        (target, source),
        (PrimitiveType::Short, PrimitiveType::Byte)
            | (PrimitiveType::Int, PrimitiveType::Byte)
            | (PrimitiveType::Int, PrimitiveType::Short)
            | (PrimitiveType::Int, PrimitiveType::Char)
            | (PrimitiveType::Long, PrimitiveType::Byte)
            | (PrimitiveType::Long, PrimitiveType::Short)
            | (PrimitiveType::Long, PrimitiveType::Char)
            | (PrimitiveType::Long, PrimitiveType::Int)
            | (PrimitiveType::Float, PrimitiveType::Byte)
            | (PrimitiveType::Float, PrimitiveType::Short)
            | (PrimitiveType::Float, PrimitiveType::Char)
            | (PrimitiveType::Float, PrimitiveType::Int)
            | (PrimitiveType::Float, PrimitiveType::Long)
            | (PrimitiveType::Double, PrimitiveType::Byte)
            | (PrimitiveType::Double, PrimitiveType::Short)
            | (PrimitiveType::Double, PrimitiveType::Char)
            | (PrimitiveType::Double, PrimitiveType::Int)
            | (PrimitiveType::Double, PrimitiveType::Long)
            | (PrimitiveType::Double, PrimitiveType::Float)
    )
}

fn reference_assignable(target_ty: TypeId, source_ty: TypeId, symbol_table: &SymbolTable) -> bool {
    if target_ty == TypeId::INVALID || source_ty == TypeId::INVALID {
        return false;
    }
    if target_ty == source_ty {
        return true;
    }

    let mut stack = vec![source_ty];
    let mut visited = HashSet::new();
    while let Some(current_ty) = stack.pop() {
        if !visited.insert(current_ty) {
            continue;
        }
        if current_ty == target_ty {
            return true;
        }
        match symbol_table.type_arena().get(current_ty) {
            Type::Class(class_type) => {
                if let Some(superclass) = class_type.superclass {
                    stack.push(superclass);
                }
                for interface in &class_type.interfaces {
                    stack.push(*interface);
                }
            }
            Type::Array(_) => {
                if let Some(object_ty) = symbol_table.lookup_type_id("java.lang", "Object") {
                    stack.push(object_ty);
                }
            }
            _ => {}
        }
    }
    false
}

fn primitive_type_display(kind: &PrimitiveType) -> &'static str {
    match kind {
        PrimitiveType::Boolean => "boolean",
        PrimitiveType::Byte => "byte",
        PrimitiveType::Char => "char",
        PrimitiveType::Short => "short",
        PrimitiveType::Int => "int",
        PrimitiveType::Long => "long",
        PrimitiveType::Float => "float",
        PrimitiveType::Double => "double",
        PrimitiveType::Void => "void",
    }
}

fn constant_fits_primitive(value: i128, primitive: &PrimitiveType) -> bool {
    match primitive {
        PrimitiveType::Byte => i8::try_from(value).is_ok(),
        PrimitiveType::Short => i16::try_from(value).is_ok(),
        PrimitiveType::Char => u16::try_from(value).is_ok(),
        _ => false,
    }
}

fn literal_integer_value(literal: &Literal) -> Option<i128> {
    match literal.kind {
        LiteralKind::Int | LiteralKind::Long => parse_integer_literal_value(literal.value.as_str()),
        LiteralKind::Char => parse_char_literal_value(literal.value.as_str()).map(i128::from),
        _ => None,
    }
}

fn parse_integer_literal_value(value: &str) -> Option<i128> {
    let sanitized = value.replace('_', "");
    let (sign, digits) = if let Some(rest) = sanitized.strip_prefix('-') {
        (-1i128, rest)
    } else if let Some(rest) = sanitized.strip_prefix('+') {
        (1i128, rest)
    } else {
        (1i128, sanitized.as_str())
    };
    let digits = digits.strip_suffix(['l', 'L']).unwrap_or(digits);

    let parsed = if let Some(rest) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i128::from_str_radix(rest, 16).ok()?
    } else if let Some(rest) = digits
        .strip_prefix("0b")
        .or_else(|| digits.strip_prefix("0B"))
    {
        i128::from_str_radix(rest, 2).ok()?
    } else if digits.starts_with('0') && digits.len() > 1 {
        i128::from_str_radix(&digits[1..], 8).ok()?
    } else {
        digits.parse::<i128>().ok()?
    };

    Some(sign * parsed)
}

fn parse_char_literal_value(value: &str) -> Option<u32> {
    let contents = value.strip_prefix('\'')?.strip_suffix('\'')?;
    if let Some(rest) = contents.strip_prefix("\\u") {
        return u32::from_str_radix(rest, 16).ok();
    }
    if let Some(rest) = contents.strip_prefix('\\') {
        return match rest {
            "n" => Some('\n' as u32),
            "r" => Some('\r' as u32),
            "t" => Some('\t' as u32),
            "b" => Some('\u{0008}' as u32),
            "f" => Some('\u{000C}' as u32),
            "\\" => Some('\\' as u32),
            "'" => Some('\'' as u32),
            "\"" => Some('"' as u32),
            _ if rest.len() == 3 && rest.chars().all(|ch| ('0'..='7').contains(&ch)) => {
                u32::from_str_radix(rest, 8).ok()
            }
            _ => None,
        };
    }

    contents.chars().next().map(u32::from)
}

fn unary_operator_display(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Plus => "+",
        UnaryOp::Minus => "-",
        UnaryOp::Bang => "!",
        UnaryOp::Tilde => "~",
        UnaryOp::Increment => "++",
        UnaryOp::Decrement => "--",
    }
}

fn binary_operator_display(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::LShift => "<<",
        BinaryOp::RShift => ">>",
        BinaryOp::ARShift => ">>>",
        BinaryOp::Lt => "<",
        BinaryOp::LtEq => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::GtEq => ">=",
        BinaryOp::EqEq => "==",
        BinaryOp::BangEq => "!=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
    }
}

fn binary_op_for_assign(op: rajac_ast::AssignOp) -> BinaryOp {
    match op {
        rajac_ast::AssignOp::Eq => BinaryOp::Add,
        rajac_ast::AssignOp::AddEq => BinaryOp::Add,
        rajac_ast::AssignOp::SubEq => BinaryOp::Sub,
        rajac_ast::AssignOp::MulEq => BinaryOp::Mul,
        rajac_ast::AssignOp::DivEq => BinaryOp::Div,
        rajac_ast::AssignOp::ModEq => BinaryOp::Mod,
        rajac_ast::AssignOp::AndEq => BinaryOp::BitAnd,
        rajac_ast::AssignOp::OrEq => BinaryOp::BitOr,
        rajac_ast::AssignOp::XorEq => BinaryOp::BitXor,
        rajac_ast::AssignOp::LShiftEq => BinaryOp::LShift,
        rajac_ast::AssignOp::RShiftEq => BinaryOp::RShift,
        rajac_ast::AssignOp::ARShiftEq => BinaryOp::ARShift,
    }
}

fn source_chunk_for_marker(
    source_file: &FilePath,
    source: &str,
    marker: Option<&str>,
) -> SourceChunk {
    let offset = marker.and_then(|marker| source.find(marker)).unwrap_or(0);
    let (line, line_start, line_end) = line_bounds_for_offset(source, offset);
    let fragment = &source[line_start..line_end];
    let annotation_start = marker.and_then(|marker| fragment.find(marker)).unwrap_or(0);
    let annotation_end = marker
        .map(|marker| annotation_start + marker.len().max(1))
        .unwrap_or(annotation_start + 1);

    SourceChunk {
        path: source_file.clone(),
        fragment: SharedString::new(fragment),
        offset: line_start,
        line,
        annotations: vec![Annotation {
            span: Span(annotation_start..annotation_end),
            message: SharedString::new(""),
        }],
    }
}

fn line_bounds_for_offset(source: &str, offset: usize) -> (usize, usize, usize) {
    let clamped_offset = offset.min(source.len());
    let line = source[..clamped_offset]
        .chars()
        .filter(|ch| *ch == '\n')
        .count()
        + 1;
    let line_start = source[..clamped_offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let line_end = source[clamped_offset..]
        .find('\n')
        .map(|index| clamped_offset + index)
        .unwrap_or(source.len());

    (line, line_start, line_end)
}

#[cfg(test)]
mod tests {
    use super::analyze_attributes;
    use crate::CompilationUnit;
    use crate::stages::{collection, resolution};
    use rajac_ast::{Ast, AstArena, Expr, Literal, LiteralKind, Stmt, UnaryOp};
    use rajac_base::{file_path::FilePath, shared_string::SharedString};
    use rajac_diagnostics::Diagnostics;
    use rajac_lexer::Lexer;
    use rajac_parser::Parser;
    use rajac_symbols::SymbolTable;
    use std::fs;

    #[test]
    fn stub_attribute_analysis_accepts_empty_inputs() {
        let mut compilation_units = Vec::new();
        let mut symbol_table = SymbolTable::new();

        let diagnostics = analyze_attributes(&mut compilation_units, &mut symbol_table);

        assert!(compilation_units.is_empty());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn folds_negative_integer_literals() {
        let mut unit = compilation_unit_with_expr(|arena| {
            let literal = arena.alloc_expr(Expr::Literal(Literal {
                kind: LiteralKind::Int,
                value: SharedString::new("127"),
            }));

            arena.alloc_expr(Expr::Unary {
                op: UnaryOp::Minus,
                expr: literal,
            })
        });

        let diagnostics =
            analyze_attributes(std::slice::from_mut(&mut unit), &mut SymbolTable::new());

        let expr_id = root_expr_id(&unit);
        let Expr::Literal(literal) = &unit.arena.expr_typed(expr_id).expr else {
            panic!("expected folded literal");
        };

        assert_eq!(literal.value.as_str(), "-127");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn folds_positive_integer_literals() {
        let mut unit = compilation_unit_with_expr(|arena| {
            let literal = arena.alloc_expr(Expr::Literal(Literal {
                kind: LiteralKind::Int,
                value: SharedString::new("127"),
            }));

            arena.alloc_expr(Expr::Unary {
                op: UnaryOp::Plus,
                expr: literal,
            })
        });

        let diagnostics =
            analyze_attributes(std::slice::from_mut(&mut unit), &mut SymbolTable::new());

        let Stmt::Expr(expr_id) = unit.arena.stmt(unit.ast.statements[0]).clone() else {
            panic!("expected expression statement");
        };
        let Expr::Literal(literal) = &unit.arena.expr_typed(expr_id).expr else {
            panic!("expected folded literal");
        };

        assert_eq!(literal.value.as_str(), "127");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn leaves_non_numeric_unary_expressions_unchanged() {
        let mut unit = compilation_unit_with_expr(|arena| {
            let literal = arena.alloc_expr(Expr::Literal(Literal {
                kind: LiteralKind::Bool,
                value: SharedString::new("true"),
            }));

            arena.alloc_expr(Expr::Unary {
                op: UnaryOp::Minus,
                expr: literal,
            })
        });

        let _ = analyze_attributes(std::slice::from_mut(&mut unit), &mut SymbolTable::new());

        let Stmt::Expr(expr_id) = unit.arena.stmt(unit.ast.statements[0]).clone() else {
            panic!("expected expression statement");
        };

        assert!(matches!(
            unit.arena.expr(expr_id),
            Expr::Unary {
                op: UnaryOp::Minus,
                ..
            }
        ));
    }

    #[test]
    fn types_local_identifiers_from_scopes() {
        let source = r#"
class Example {
    int run(int limit) {
        int sum = 0;
        {
            int next = limit;
            sum = next;
        }
        return sum;
    }
}
"#;

        let (mut units, mut symbol_table) = resolved_units(source);
        let diagnostics = analyze_attributes(&mut units, &mut symbol_table);

        assert!(diagnostics.is_empty());
        let unit = &units[0];
        let typed_count = unit
            .arena
            .exprs
            .iter()
            .filter(|expr| expr.ty != rajac_types::TypeId::INVALID)
            .count();
        assert!(typed_count > 0);
    }

    #[test]
    fn reports_duplicate_local_variables() {
        let source = r#"
class Example {
    int run() {
        int value = 1;
        int value = 2;
        return value;
    }
}
"#;

        let (mut units, mut symbol_table) = resolved_units(source);
        let diagnostics = analyze_attributes(&mut units, &mut symbol_table);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .as_str()
                .contains("duplicate local variable")
        }));
    }

    #[test]
    fn reports_incompatible_local_initializers() {
        let source = r#"
class Example {
    int run() {
        int value = true;
        return value;
    }
}
"#;

        let (mut units, mut symbol_table) = resolved_units(source);
        let diagnostics = analyze_attributes(&mut units, &mut symbol_table);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.as_str().contains("incompatible types"))
        );
    }

    #[test]
    fn reports_non_boolean_conditions() {
        let source = r#"
class Example {
    int run() {
        while (1) {
            return 1;
        }
        return 0;
    }
}
"#;

        let (mut units, mut symbol_table) = resolved_units(source);
        let diagnostics = analyze_attributes(&mut units, &mut symbol_table);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .as_str()
                .contains("condition must be boolean")
        }));
    }

    #[test]
    fn reports_return_type_mismatches() {
        let source = r#"
class Example {
    int run() {
        return true;
    }
}
"#;

        let (mut units, mut symbol_table) = resolved_units(source);
        let diagnostics = analyze_attributes(&mut units, &mut symbol_table);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.as_str().contains("incompatible types"))
        );
    }

    #[test]
    fn resolves_basic_method_calls() {
        let source = r#"
class Example {
    int helper(int value) {
        return value;
    }

    int run() {
        return helper(1);
    }
}
"#;

        let (mut units, mut symbol_table) = resolved_units(source);
        let diagnostics = analyze_attributes(&mut units, &mut symbol_table);

        assert!(diagnostics.is_empty());
        let unit = &units[0];
        assert!(unit.arena.exprs.iter().any(|expr| matches!(
            &expr.expr,
            Expr::MethodCall {
                method_id: Some(_),
                ..
            }
        )));
    }

    #[test]
    fn verification_sources_have_no_semantic_diagnostics() {
        let mut failures = Vec::new();
        let verification_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../verification/sources/verify");
        let mut base_symbol_table = SymbolTable::new();
        collection::collect_classpath_symbols(
            &mut base_symbol_table,
            &[FilePath::new("/usr/lib/jvm/java-8-openjdk/jre/lib/rt.jar")],
        )
        .expect("collect classpath symbols");

        for entry in walkdir::WalkDir::new(&verification_root) {
            let entry = entry.expect("walkdir entry");
            if !entry.file_type().is_file()
                || entry
                    .path()
                    .extension()
                    .is_none_or(|extension| extension != "java")
            {
                continue;
            }

            let source = fs::read_to_string(entry.path()).expect("read verification source");
            let (mut units, _) = resolved_units(&source);
            let mut symbol_table = base_symbol_table.clone();
            collection::collect_compilation_unit_symbols(&mut symbol_table, &units)
                .expect("collect symbols");
            resolution::resolve_identifiers(&mut units, &mut symbol_table);
            let diagnostics = analyze_attributes(&mut units, &mut symbol_table);

            if !diagnostics.is_empty() {
                failures.push(format!(
                    "{}: {}",
                    entry.path().display(),
                    diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.message.as_str().to_owned())
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    fn root_expr_id(unit: &CompilationUnit) -> rajac_ast::ExprId {
        let Stmt::Expr(expr_id) = unit.arena.stmt(unit.ast.statements[0]).clone() else {
            panic!("expected expression statement");
        };
        expr_id
    }

    fn compilation_unit_with_expr(
        build_expr: impl FnOnce(&mut AstArena) -> rajac_ast::ExprId,
    ) -> CompilationUnit {
        let mut arena = AstArena::new();
        let expr_id = build_expr(&mut arena);
        let stmt_id = arena.alloc_stmt(Stmt::Expr(expr_id));
        let mut ast = Ast::new(SharedString::new(""));
        ast.statements.push(stmt_id);

        CompilationUnit {
            source_file: FilePath::new("Test.java"),
            ast,
            arena,
            diagnostics: Diagnostics::new(),
        }
    }

    fn resolved_units(source: &str) -> (Vec<CompilationUnit>, SymbolTable) {
        let parse_result = Parser::new(Lexer::new(source, FilePath::new("Test.java")), source)
            .parse_compilation_unit();
        let mut units = vec![CompilationUnit {
            source_file: FilePath::new("Test.java"),
            ast: parse_result.ast,
            arena: parse_result.arena,
            diagnostics: parse_result.diagnostics,
        }];
        let mut symbol_table = SymbolTable::new();
        collection::collect_compilation_unit_symbols(&mut symbol_table, &units)
            .expect("collect symbols");
        resolution::resolve_identifiers(&mut units, &mut symbol_table);
        (units, symbol_table)
    }
}
