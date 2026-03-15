//! # Attribute Analysis Stage
//!
//! This module handles the fifth stage of the compilation pipeline: attribute
//! analysis after symbol resolution and before bytecode generation.
//!
//! ## What is the purpose of this stage?
//!
//! The attribute analysis stage is the future home for semantic checks that
//! require resolved symbols and types, such as expression typing, overload
//! resolution, constant evaluation, and annotation validation.
//!
//! ## Why is this implementation currently a stub?
//!
//! The pipeline documented for the compiler already includes an attribute
//! analysis phase. Adding the stage now makes the runtime pipeline match that
//! design while keeping the implementation small until real checks are added.

/* 📖 # Why add a stub attribute analysis stage before implementing its logic?
The compiler documentation already describes attribute analysis as a distinct
phase between resolution and later semantic/codegen stages. Introducing the
stage now keeps the executable pipeline aligned with that architecture and
creates a stable integration point for future type-checking work.
*/

use crate::CompilationUnit;
use rajac_ast::{
    Ast, AstArena, ClassDeclId, ClassMember, ClassMemberId, EnumDecl, Expr, ExprId, Field, ForInit,
    Literal, LiteralKind, Method, Stmt, StmtId, UnaryOp,
};
use rajac_base::logging::instrument;
use rajac_base::shared_string::SharedString;
use rajac_symbols::SymbolTable;

/// Performs attribute analysis on resolved compilation units.
///
/// This stub currently preserves the pipeline shape without mutating the
/// compilation units or symbol table.
#[instrument(
    name = "compiler.phase.attribute_analysis",
    skip(compilation_units, symbol_table),
    fields(compilation_units = compilation_units.len())
)]
pub fn analyze_attributes(
    compilation_units: &mut [CompilationUnit],
    symbol_table: &mut SymbolTable,
) {
    let _ = symbol_table;

    for compilation_unit in compilation_units {
        analyze_compilation_unit(&compilation_unit.ast, &mut compilation_unit.arena);
    }
}

fn analyze_compilation_unit(ast: &Ast, arena: &mut AstArena) {
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

#[cfg(test)]
mod tests {
    use super::analyze_attributes;
    use crate::CompilationUnit;
    use rajac_ast::{Ast, AstArena, Expr, Literal, LiteralKind, Stmt, UnaryOp};
    use rajac_base::{file_path::FilePath, shared_string::SharedString};
    use rajac_diagnostics::Diagnostics;
    use rajac_symbols::SymbolTable;

    #[test]
    fn stub_attribute_analysis_accepts_empty_inputs() {
        let mut compilation_units = Vec::new();
        let mut symbol_table = SymbolTable::new();

        analyze_attributes(&mut compilation_units, &mut symbol_table);

        assert!(compilation_units.is_empty());
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

        analyze_attributes(std::slice::from_mut(&mut unit), &mut SymbolTable::new());

        let expr_id = root_expr_id(&unit);
        let Expr::Literal(literal) = &unit.arena.expr_typed(expr_id).expr else {
            panic!("expected folded literal");
        };

        assert_eq!(literal.value.as_str(), "-127");
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

        analyze_attributes(std::slice::from_mut(&mut unit), &mut SymbolTable::new());

        let Stmt::Expr(expr_id) = unit.arena.stmt(unit.ast.statements[0]).clone() else {
            panic!("expected expression statement");
        };
        let Expr::Literal(literal) = &unit.arena.expr_typed(expr_id).expr else {
            panic!("expected folded literal");
        };

        assert_eq!(literal.value.as_str(), "127");
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

        analyze_attributes(std::slice::from_mut(&mut unit), &mut SymbolTable::new());

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
}
