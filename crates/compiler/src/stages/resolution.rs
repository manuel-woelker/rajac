//! # Identifier Resolution Stage

//!

//! This module handles the fourth stage of the compilation pipeline: resolving

//! identifiers and type references to their fully qualified declarations.

//!

//! ## Purpose

//!

//! The resolution stage is responsible for:

//! - Converting simple identifiers to fully qualified names

//! - Resolving type references using symbol tables and imports

//! - Handling package scoping and import statements

//! - Preparing ASTs for bytecode generation

//! - Ensuring all references can be properly linked

//!

//! ## Implementation Details

//!

//! Resolution involves complex analysis of:

//! - Import statements and their impact on name resolution

//! - Package structure and current compilation context

//! - Symbol table lookups for type and identifier resolution

//! - Java language scoping rules and visibility

//! - Common Java type handling (String, Object, etc.)

//!

//! ## Process

//!

//! The resolution algorithm:

//! 1. Builds context from package, imports, and symbol table

//! 2. Traverses AST nodes recursively

//! 3. Resolves each identifier to its fully qualified form

//! 4. Updates AST nodes with resolution information

//! 5. Handles special cases for built-in Java types

//!

//! ## Usage

//!

//! This stage is typically called from the main compiler pipeline but can

//! be used independently for type checking or analysis purposes.

//!

//! ```rust,no_run,ignore

//! use rajac_compiler::stages::resolution;

//! use rajac_compiler::CompilationUnit;

//! use rajac_symbols::SymbolTable;

//!

//! let compilation_units = vec!/* ... */;

//! let symbol_table = SymbolTable::new();

//! resolution::resolve_identifiers(&mut compilation_units, &mut symbol_table);

//! println!("Resolved identifiers in {} compilation units", compilation_units.len());

//! # Ok::<(), Box<dyn std::error::Error>>(())

//! ```

/* 📖 # Why separate resolution into its own stage?
Resolution is a complex phase where identifiers and types are resolved
to their fully qualified names using the symbol table. This involves
traversing the entire AST and resolving all references. Separating
this stage makes the resolution logic more testable and allows for
optimization of the resolution algorithms without affecting other phases.
*/

use crate::CompilationUnit;
use rajac_ast::{
    Ast, AstArena, AstType, AstTypeId, ClassDeclId, ClassMember, ClassMemberId, Constructor,
    EnumDecl, ExprId, Field, Method, Modifiers, ParamId, StmtId,
};
use rajac_base::logging::instrument;
use rajac_base::qualified_name::FullyQualifiedClassName as ResolvedName;
use rajac_base::shared_string::SharedString;
use rajac_symbols::SymbolTable;
use rajac_types::{
    FieldId, FieldModifiers, FieldSignature, MethodId, MethodModifiers, MethodSignature, Type,
    TypeId,
};
use std::collections::{HashMap, HashSet};

/// Resolves identifiers and types in all compilation units.
///
/// This is the main entry point for the resolution phase. It processes
/// all compilation units in parallel using the symbol table to resolve
/// every identifier and type reference to their fully qualified names.
///
/// # Parameters
///
/// - `compilation_units` - Mutable slice of compilation units to resolve
/// - `symbol_table` - Mutable reference to the populated symbol table
///
/// # Resolution Process
///
/// For each compilation unit:
/// 1. Creates a resolution context with package and imports
/// 2. Traverses all AST nodes (statements, classes, members)
/// 3. Resolves identifiers using symbol table lookups
/// 4. Handles special cases for common Java types
/// 5. Updates AST nodes with fully qualified names
///
/// # Parallel Processing
///
/// Uses `rayon` for parallel resolution:
/// - Each compilation unit is processed independently
/// - Symbol table is shared safely across threads
/// - Results are collected without ordering requirements
///
/// # Examples
///
/// ```rust,no_run,ignore
/// use rajac_compiler::stages::resolution;
/// use rajac_compiler::CompilationUnit;
/// use rajac_symbols::SymbolTable;
///
/// let mut compilation_units = vec!/* parsed compilation units */;
/// let symbol_table = SymbolTable::new();
///
/// resolution::resolve_identifiers(&mut compilation_units, &mut symbol_table);
///
/// for unit in &compilation_units {
///     println!("Resolved compilation unit: {}", unit.source_file.as_str());
/// }
/// ```
///
/// # Resolution Rules
///
/// The resolver follows Java language rules:
/// - Current package has highest priority for unqualified names
/// - Single-type imports take precedence over on-demand imports
/// - Built-in types (String, Object) are always available
/// - Fully qualified names bypass import resolution
/// - Inner classes have special resolution rules
#[instrument(
    name = "compiler.phase.resolution",
    skip(compilation_units, symbol_table),
    fields(compilation_units = compilation_units.len())
)]
pub fn resolve_identifiers(
    compilation_units: &mut [CompilationUnit],
    symbol_table: &mut SymbolTable,
) {
    for unit in compilation_units.iter_mut() {
        resolve_compilation_unit(&unit.ast, &mut unit.arena, symbol_table, &unit.source_file);
    }
}

/// Resolves identifiers in a single compilation unit.
#[instrument(
    name = "compiler.phase.resolution.file",
    skip(ast, arena, symbol_table, source_file),
    fields(source_file = %source_file.as_str())
)]
fn resolve_compilation_unit(
    ast: &Ast,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    source_file: &rajac_base::file_path::FilePath,
) {
    let context = ResolveContext::new(ast);

    for class_id in &ast.classes {
        resolve_class_decl_signatures(*class_id, arena, symbol_table, &context);
    }

    populate_compilation_unit_methods(ast, arena, symbol_table);

    for stmt_id in &ast.statements {
        resolve_stmt(*stmt_id, arena, symbol_table, &context, None);
    }

    for class_id in &ast.classes {
        resolve_class_decl(*class_id, arena, symbol_table, &context);
    }
}

fn resolve_class_decl_signatures(
    class_id: ClassDeclId,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
) {
    let (members, extends, implements, permits) = {
        let class = &mut arena.class_decls[class_id.0 as usize];
        (
            class.members.clone(),
            class.extends,
            class.implements.clone(),
            class.permits.clone(),
        )
    };

    if let Some(type_id) = extends {
        resolve_type(type_id, arena, symbol_table, context);
    }
    for type_id in &implements {
        resolve_type(*type_id, arena, symbol_table, context);
    }
    for type_id in permits {
        resolve_type(type_id, arena, symbol_table, context);
    }
    for member_id in members {
        resolve_class_member_signatures(member_id, arena, symbol_table, context);
    }
}

/// Context for resolving identifiers using the symbol table, package, and imports.
struct ResolveContext {
    current_package: SharedString,
    single_type_imports: Vec<(SharedString, SharedString)>,
    on_demand_imports: Vec<SharedString>,
}

impl ResolveContext {
    /// Builds a resolution context from the current AST and symbol table.
    fn new(ast: &Ast) -> Self {
        let current_package = ast
            .package
            .as_ref()
            .map(|p| package_name_from_segments(&p.name.segments))
            .unwrap_or_else(|| SharedString::new(""));

        let mut single_type_imports = Vec::new();
        let mut on_demand_imports = Vec::new();

        for import in &ast.imports {
            if import.is_on_demand {
                on_demand_imports.push(package_name_from_segments(&import.name.segments));
            } else if let Some((package, name)) = split_import_name(&import.name.segments) {
                single_type_imports.push((package, name));
            }
        }

        Self {
            current_package,
            single_type_imports,
            on_demand_imports,
        }
    }
}

/// Resolves a class declaration and all nested members.
fn resolve_class_decl(
    class_id: rajac_ast::ClassDeclId,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
) {
    let (class_name, members, extends, implements, permits) = {
        let class = &mut arena.class_decls[class_id.0 as usize];
        // Note: class names no longer need resolution since we use TypeIds for types
        for _param in &mut class.type_params {
            // TODO: Implement type parameter name resolution for SharedString
        }
        (
            class.name.name.clone(),
            class.members.clone(),
            class.extends,
            class.implements.clone(),
            class.permits.clone(),
        )
    };

    let current_class_type_id =
        symbol_table.lookup_type_id(context.current_package.as_str(), class_name.as_str());

    if let Some(type_id) = extends {
        resolve_type(type_id, arena, symbol_table, context);
    }
    for type_id in &implements {
        resolve_type(*type_id, arena, symbol_table, context);
    }
    for type_id in permits {
        resolve_type(type_id, arena, symbol_table, context);
    }
    if let Some(class_type_id) = current_class_type_id {
        update_class_hierarchy(class_type_id, extends, &implements, arena, symbol_table);
    }
    for member_id in members {
        resolve_class_member(
            member_id,
            arena,
            symbol_table,
            context,
            current_class_type_id,
        );
    }
}

fn update_class_hierarchy(
    class_type_id: TypeId,
    extends: Option<AstTypeId>,
    implements: &[AstTypeId],
    arena: &AstArena,
    symbol_table: &mut SymbolTable,
) {
    let superclass = extends
        .map(|type_id| arena.ty(type_id).ty())
        .filter(|type_id| *type_id != TypeId::INVALID);
    let interfaces = implements
        .iter()
        .map(|type_id| arena.ty(*type_id).ty())
        .filter(|type_id| *type_id != TypeId::INVALID)
        .collect::<Vec<_>>();

    if let Type::Class(class_type) = symbol_table.type_arena_mut().get_mut(class_type_id) {
        class_type.superclass = superclass;
        class_type.interfaces = interfaces;
    }
}

/// Resolves identifiers in a class member.
fn resolve_class_member(
    member_id: ClassMemberId,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
    current_class_type_id: Option<TypeId>,
) {
    let mut member = arena.class_members[member_id.0 as usize].clone();

    match &mut member {
        rajac_ast::ClassMember::Field(field) => {
            resolve_field(field, arena, symbol_table, context, current_class_type_id)
        }
        rajac_ast::ClassMember::Method(method) => {
            resolve_method(method, arena, symbol_table, context, current_class_type_id)
        }
        rajac_ast::ClassMember::Constructor(constructor) => resolve_constructor(
            constructor,
            arena,
            symbol_table,
            context,
            current_class_type_id,
        ),
        rajac_ast::ClassMember::StaticBlock(stmt_id) => resolve_stmt(
            *stmt_id,
            arena,
            symbol_table,
            context,
            current_class_type_id,
        ),
        rajac_ast::ClassMember::NestedClass(class_id)
        | rajac_ast::ClassMember::NestedInterface(class_id)
        | rajac_ast::ClassMember::NestedRecord(class_id)
        | rajac_ast::ClassMember::NestedAnnotation(class_id) => {
            resolve_class_decl(*class_id, arena, symbol_table, context)
        }
        rajac_ast::ClassMember::NestedEnum(enum_decl) => {
            resolve_enum_decl(enum_decl, arena, symbol_table, context)
        }
    }

    arena.class_members[member_id.0 as usize] = member;
}

fn resolve_class_member_signatures(
    member_id: ClassMemberId,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
) {
    let mut member = arena.class_members[member_id.0 as usize].clone();

    match &mut member {
        ClassMember::Field(field) => {
            resolve_type(field.ty, arena, symbol_table, context);
        }
        ClassMember::Method(method) => {
            for param_id in method.params.clone() {
                resolve_param(param_id, arena, symbol_table, context);
            }
            resolve_type(method.return_ty, arena, symbol_table, context);
            for throws_id in method.throws.clone() {
                resolve_type(throws_id, arena, symbol_table, context);
            }
        }
        ClassMember::Constructor(constructor) => {
            for param_id in constructor.params.clone() {
                resolve_param(param_id, arena, symbol_table, context);
            }
            for throws_id in constructor.throws.clone() {
                resolve_type(throws_id, arena, symbol_table, context);
            }
        }
        ClassMember::StaticBlock(_) => {}
        ClassMember::NestedClass(class_id)
        | ClassMember::NestedInterface(class_id)
        | ClassMember::NestedRecord(class_id)
        | ClassMember::NestedAnnotation(class_id) => {
            resolve_class_decl_signatures(*class_id, arena, symbol_table, context);
        }
        ClassMember::NestedEnum(enum_decl) => {
            resolve_enum_decl_signatures(enum_decl, arena, symbol_table, context);
        }
    }

    arena.class_members[member_id.0 as usize] = member;
}

fn resolve_enum_decl_signatures(
    enum_decl: &mut EnumDecl,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
) {
    for type_id in enum_decl.implements.clone() {
        resolve_type(type_id, arena, symbol_table, context);
    }

    for entry in &mut enum_decl.entries {
        if let Some(members) = &entry.body {
            for member_id in members.clone() {
                resolve_class_member_signatures(member_id, arena, symbol_table, context);
            }
        }
    }

    for member_id in enum_decl.members.clone() {
        resolve_class_member_signatures(member_id, arena, symbol_table, context);
    }
}

/// Resolves identifiers in an enum declaration.
fn resolve_enum_decl(
    enum_decl: &mut EnumDecl,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
) {
    // Note: enum names no longer need resolution since we use TypeIds for types

    for type_id in enum_decl.implements.clone() {
        resolve_type(type_id, arena, symbol_table, context);
    }

    for entry in &mut enum_decl.entries {
        // Note: entry names no longer need resolution since they're just identifiers
        for expr_id in entry.args.clone() {
            resolve_expr(expr_id, arena, symbol_table, context, None);
        }
        if let Some(members) = &entry.body {
            for member_id in members.clone() {
                resolve_class_member(member_id, arena, symbol_table, context, None);
            }
        }
    }

    for member_id in enum_decl.members.clone() {
        resolve_class_member(member_id, arena, symbol_table, context, None);
    }
}

/// Resolves identifiers in a field declaration.
fn resolve_field(
    field: &mut Field,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
    current_class_type_id: Option<TypeId>,
) {
    // Note: field names no longer need resolution since they're just identifiers
    resolve_type(field.ty, arena, symbol_table, context);
    if let Some(expr_id) = field.initializer {
        resolve_expr(expr_id, arena, symbol_table, context, current_class_type_id);
    }
}

/// Resolves identifiers in a method declaration.
fn resolve_method(
    method: &mut Method,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
    current_class_type_id: Option<TypeId>,
) {
    // Note: method names no longer need resolution since they're just identifiers
    for param_id in method.params.clone() {
        resolve_param(param_id, arena, symbol_table, context);
    }
    resolve_type(method.return_ty, arena, symbol_table, context);
    for throws_id in method.throws.clone() {
        resolve_type(throws_id, arena, symbol_table, context);
    }
    if let Some(body) = method.body {
        resolve_stmt(body, arena, symbol_table, context, current_class_type_id);
    }
}

/// Resolves identifiers in a constructor declaration.
fn resolve_constructor(
    constructor: &mut Constructor,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
    current_class_type_id: Option<TypeId>,
) {
    // Note: constructor names no longer need resolution since they're just identifiers
    for param_id in constructor.params.clone() {
        resolve_param(param_id, arena, symbol_table, context);
    }
    for throws_id in constructor.throws.clone() {
        resolve_type(throws_id, arena, symbol_table, context);
    }
    if let Some(body) = constructor.body {
        resolve_stmt(body, arena, symbol_table, context, current_class_type_id);
    }
}

/// Resolves identifiers in a parameter.
fn resolve_param(
    param_id: ParamId,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
) {
    let param = &mut arena.params[param_id.0 as usize];
    // Note: parameter names no longer need resolution since they're just identifiers
    resolve_type(param.ty, arena, symbol_table, context);
}

/// Resolves identifiers in a statement.
fn resolve_stmt(
    stmt_id: StmtId,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
    current_class_type_id: Option<TypeId>,
) {
    let (exprs, stmts, types, params) = {
        let stmt = &mut arena.stmts[stmt_id.0 as usize];
        let mut exprs = Vec::new();
        let mut stmts = Vec::new();
        let mut types = Vec::new();
        let mut params = Vec::new();

        match stmt {
            rajac_ast::Stmt::Empty => {}
            rajac_ast::Stmt::Block(items) => {
                stmts.extend(items.iter().copied());
            }
            rajac_ast::Stmt::Expr(expr_id) => exprs.push(*expr_id),
            rajac_ast::Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                exprs.push(*condition);
                stmts.push(*then_branch);
                if let Some(else_branch) = else_branch {
                    stmts.push(*else_branch);
                }
            }
            rajac_ast::Stmt::While { condition, body } => {
                exprs.push(*condition);
                stmts.push(*body);
            }
            rajac_ast::Stmt::DoWhile { body, condition } => {
                stmts.push(*body);
                exprs.push(*condition);
            }
            rajac_ast::Stmt::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(init) = init {
                    match init {
                        rajac_ast::ForInit::Expr(expr_id) => exprs.push(*expr_id),
                        rajac_ast::ForInit::LocalVar {
                            ty,
                            name: _,
                            initializer,
                        } => {
                            types.push(*ty);
                            // Note: local variable names no longer need resolution since they're just identifiers
                            if let Some(init) = initializer {
                                exprs.push(*init);
                            }
                        }
                    }
                }
                if let Some(condition) = condition {
                    exprs.push(*condition);
                }
                if let Some(update) = update {
                    exprs.push(*update);
                }
                stmts.push(*body);
            }
            rajac_ast::Stmt::Switch { expr, cases } => {
                exprs.push(*expr);
                for case in cases {
                    for label in &case.labels {
                        match label {
                            rajac_ast::SwitchLabel::Case(expr_id) => exprs.push(*expr_id),
                            rajac_ast::SwitchLabel::Default => {}
                        }
                    }
                    stmts.extend(case.body.iter().copied());
                }
            }
            rajac_ast::Stmt::Return(expr_id) => {
                if let Some(expr_id) = expr_id {
                    exprs.push(*expr_id);
                }
            }
            rajac_ast::Stmt::Break(name) | rajac_ast::Stmt::Continue(name) => {
                if let Some(_name) = name {
                    // Note: local variable names no longer need resolution since they're just identifiers
                }
            }
            rajac_ast::Stmt::Label(name, body) => {
                // Note: label names no longer need resolution since they're just identifiers
                let _name = name;
                stmts.push(*body);
            }
            rajac_ast::Stmt::Try {
                try_block,
                catches,
                finally_block,
            } => {
                stmts.push(*try_block);
                for clause in catches {
                    params.push(clause.param);
                    stmts.push(clause.body);
                }
                if let Some(finally_block) = finally_block {
                    stmts.push(*finally_block);
                }
            }
            rajac_ast::Stmt::Throw(expr_id) => exprs.push(*expr_id),
            rajac_ast::Stmt::Synchronized { expr, block } => {
                if let Some(expr_id) = expr {
                    exprs.push(*expr_id);
                }
                stmts.push(*block);
            }
            rajac_ast::Stmt::LocalVar {
                ty,
                name: _,
                initializer,
            } => {
                types.push(*ty);
                // Note: local variable names no longer need resolution since they're just identifiers
                if let Some(init) = initializer {
                    exprs.push(*init);
                }
            }
        }

        (exprs, stmts, types, params)
    };

    for type_id in types {
        resolve_type(type_id, arena, symbol_table, context);
    }
    for param_id in params {
        resolve_param(param_id, arena, symbol_table, context);
    }
    for expr_id in exprs {
        resolve_expr(expr_id, arena, symbol_table, context, current_class_type_id);
    }
    for stmt_id in stmts {
        resolve_stmt(stmt_id, arena, symbol_table, context, current_class_type_id);
    }
}

/// Resolves identifiers in an expression.
fn resolve_expr(
    expr_id: ExprId,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
    current_class_type_id: Option<TypeId>,
) {
    let (exprs, types) = {
        let expr = arena.expr(expr_id);
        let mut exprs = Vec::new();
        let mut types = Vec::new();

        match expr {
            rajac_ast::Expr::Error => {}
            rajac_ast::Expr::Ident(_name) => {} // Note: identifier names no longer need resolution
            rajac_ast::Expr::Literal(_) => {}
            rajac_ast::Expr::Unary { expr, .. } => exprs.push(*expr),
            rajac_ast::Expr::Binary { lhs, rhs, .. } => {
                exprs.push(*lhs);
                exprs.push(*rhs);
            }
            rajac_ast::Expr::Assign { lhs, rhs, .. } => {
                exprs.push(*lhs);
                exprs.push(*rhs);
            }
            rajac_ast::Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                exprs.push(*condition);
                exprs.push(*then_expr);
                exprs.push(*else_expr);
            }
            rajac_ast::Expr::Cast { ty, expr } => {
                types.push(*ty);
                exprs.push(*expr);
            }
            rajac_ast::Expr::InstanceOf { expr, ty } => {
                exprs.push(*expr);
                types.push(*ty);
            }
            rajac_ast::Expr::FieldAccess { expr, .. } => {
                exprs.push(*expr);
            }
            rajac_ast::Expr::MethodCall {
                expr,
                type_args,
                args,
                ..
            } => {
                if let Some(expr_id) = expr {
                    exprs.push(*expr_id);
                }
                if let Some(type_args) = type_args {
                    types.extend(type_args.iter().copied());
                }
                exprs.extend(args.iter().copied());
            }
            rajac_ast::Expr::New { ty, args } => {
                types.push(*ty);
                exprs.extend(args.iter().copied());
            }
            rajac_ast::Expr::NewArray {
                ty,
                dimensions,
                initializer,
            } => {
                types.push(*ty);
                exprs.extend(dimensions.iter().copied());
                if let Some(initializer) = initializer {
                    exprs.push(*initializer);
                }
            }
            rajac_ast::Expr::ArrayInitializer { elements } => {
                exprs.extend(elements.iter().copied())
            }
            rajac_ast::Expr::ArrayAccess { array, index } => {
                exprs.push(*array);
                exprs.push(*index);
            }
            rajac_ast::Expr::ArrayLength { array } => exprs.push(*array),
            rajac_ast::Expr::This(expr_id) => {
                if let Some(expr_id) = expr_id {
                    exprs.push(*expr_id);
                }
            }
            rajac_ast::Expr::Super => {}
            rajac_ast::Expr::SuperCall {
                type_args, args, ..
            } => {
                if let Some(type_args) = type_args {
                    types.extend(type_args.iter().copied());
                }
                exprs.extend(args.iter().copied());
            }
        }

        (exprs, types)
    };

    for type_id in types {
        resolve_type(type_id, arena, symbol_table, context);
    }
    for expr_id in exprs {
        resolve_expr(expr_id, arena, symbol_table, context, current_class_type_id);
    }

    let mut expr = arena.expr(expr_id).clone();
    let mut expr_ty = TypeId::INVALID;

    match &mut expr {
        rajac_ast::Expr::Error => {}
        rajac_ast::Expr::Ident(name) => {
            if let Some(resolved_name) = resolve_class_name(&name.name, context, symbol_table)
                && let Some(type_id) = symbol_table.lookup_type_id(
                    resolved_name.package_name().as_str(),
                    resolved_name.name().as_str(),
                )
            {
                expr_ty = type_id;
            }
        }
        rajac_ast::Expr::Literal(literal) => {
            expr_ty = literal_type_id(literal, symbol_table);
        }
        rajac_ast::Expr::Unary { op, expr } => {
            let operand_ty = arena.expr_typed(*expr).ty;
            expr_ty = unary_result_type(op, operand_ty, symbol_table);
        }
        rajac_ast::Expr::Binary { op, lhs, rhs } => {
            let lhs_ty = arena.expr_typed(*lhs).ty;
            let rhs_ty = arena.expr_typed(*rhs).ty;
            expr_ty = binary_result_type(op, lhs_ty, rhs_ty, symbol_table);
        }
        rajac_ast::Expr::Assign { lhs, .. } => {
            expr_ty = arena.expr_typed(*lhs).ty;
        }
        rajac_ast::Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_ty = arena.expr_typed(*then_expr).ty;
            let else_ty = arena.expr_typed(*else_expr).ty;
            expr_ty = if then_ty != TypeId::INVALID && then_ty == else_ty {
                then_ty
            } else {
                TypeId::INVALID
            };
        }
        rajac_ast::Expr::Cast { ty, .. } => {
            expr_ty = arena.ty(*ty).ty();
        }
        rajac_ast::Expr::InstanceOf { .. } => {
            expr_ty = symbol_table
                .primitive_type_id("boolean")
                .unwrap_or(TypeId::INVALID);
        }
        rajac_ast::Expr::FieldAccess {
            expr,
            name,
            field_id,
        } => {
            let receiver_ty = arena.expr_typed(*expr).ty;
            if let Some(field) = resolve_field_in_type(receiver_ty, &name.name, symbol_table) {
                *field_id = Some(field);
                expr_ty = symbol_table.field_arena().get(field).ty;
            }
        }
        rajac_ast::Expr::MethodCall {
            expr,
            name,
            method_id,
            args,
            ..
        } => {
            let receiver_ty = expr
                .as_ref()
                .map(|expr_id| arena.expr_typed(*expr_id).ty)
                .or(current_class_type_id)
                .unwrap_or(TypeId::INVALID);
            let arg_types = args
                .iter()
                .map(|arg| arena.expr_typed(*arg).ty)
                .collect::<Vec<_>>();
            if let Some(method) =
                resolve_method_in_type(receiver_ty, &name.name, &arg_types, symbol_table)
            {
                *method_id = Some(method);
                expr_ty = symbol_table.method_arena().get(method).return_type;
            }
        }
        rajac_ast::Expr::New { ty, .. } => {
            expr_ty = arena.ty(*ty).ty();
        }
        rajac_ast::Expr::NewArray {
            ty,
            dimensions,
            initializer,
        } => {
            if initializer.is_some() {
                expr_ty = arena.ty(*ty).ty();
            } else {
                let element_id = arena.ty(*ty).ty();
                if element_id != TypeId::INVALID {
                    let mut array_id = element_id;
                    for _ in 0..dimensions.len() {
                        array_id = symbol_table.type_arena_mut().alloc(Type::array(array_id));
                    }
                    expr_ty = array_id;
                }
            }
        }
        rajac_ast::Expr::ArrayInitializer { .. } => {}
        rajac_ast::Expr::ArrayAccess { array, .. } => {
            let array_ty = arena.expr_typed(*array).ty;
            expr_ty = array_element_type(array_ty, symbol_table);
        }
        rajac_ast::Expr::ArrayLength { .. } => {
            expr_ty = symbol_table
                .primitive_type_id("int")
                .unwrap_or(TypeId::INVALID);
        }
        rajac_ast::Expr::This(_) => {
            expr_ty = current_class_type_id.unwrap_or(TypeId::INVALID);
        }
        rajac_ast::Expr::Super => {
            expr_ty = superclass_type_id(current_class_type_id, symbol_table);
        }
        rajac_ast::Expr::SuperCall {
            method_id, args, ..
        } => {
            let receiver_ty = superclass_type_id(current_class_type_id, symbol_table);
            let arg_types = args
                .iter()
                .map(|arg| arena.expr_typed(*arg).ty)
                .collect::<Vec<_>>();
            let constructor_name = match symbol_table.type_arena().get(receiver_ty) {
                Type::Class(class_type) => class_type.name.clone(),
                _ => SharedString::new("super"),
            };
            if let Some(method) =
                resolve_method_in_type(receiver_ty, &constructor_name, &arg_types, symbol_table)
            {
                *method_id = Some(method);
                expr_ty = symbol_table
                    .primitive_type_id("void")
                    .unwrap_or(TypeId::INVALID);
            }
        }
    }

    let typed_expr = arena.expr_typed_mut(expr_id);
    typed_expr.expr = expr;
    typed_expr.ty = expr_ty;
}

/// Resolves identifiers in a type.
fn resolve_type(
    type_id: AstTypeId,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    context: &ResolveContext,
) {
    let types = {
        let ty = arena.ty_mut(type_id);
        let mut types = Vec::new();

        match ty {
            AstType::Error => {}
            AstType::Primitive { kind, ty } => {
                if let Some(type_id) = symbol_table.primitive_type_id(primitive_name_from_ast(kind))
                {
                    *ty = type_id;
                }
            }
            AstType::Simple {
                name,
                type_args,
                ty,
            } => {
                if !type_args.is_empty() {
                    types.extend(type_args.iter().copied());
                }

                // Resolve the class name and set the TypeId
                if let Some(resolved_name) = resolve_class_name(name, context, symbol_table) {
                    let package_str = resolved_name.package_name().as_str();
                    let class_str = resolved_name.name().as_str();

                    if let Some(type_id) = symbol_table.lookup_type_id(package_str, class_str) {
                        *ty = type_id;
                    }
                    // Note: If the type is not found in the symbol table, it means
                    // it's not available in the classpath, which is an error condition
                    // that should be handled elsewhere. We don't create fallback entries.
                }

                // TODO: Handle primitive types (int, String, etc.)
            }
            AstType::Array { element_type, .. } => {
                types.push(*element_type);
            }
            AstType::Wildcard { .. } => {
                // TODO: Handle wildcard bounds
            }
        }

        types
    };

    for type_id in types {
        resolve_type(type_id, arena, symbol_table, context);
    }

    let (element_type_id, dimensions, needs_array) = match arena.ty(type_id) {
        AstType::Array {
            element_type,
            dimensions,
            ty,
        } => (*element_type, *dimensions, *ty == TypeId::INVALID),
        _ => (AstTypeId::INVALID, 0, false),
    };

    if needs_array {
        let element_id = arena.ty(element_type_id).ty();
        if element_id != TypeId::INVALID {
            let mut array_id = element_id;
            for _ in 0..dimensions {
                array_id = symbol_table.type_arena_mut().alloc(Type::array(array_id));
            }
            if let AstType::Array { ty, .. } = arena.ty_mut(type_id) {
                *ty = array_id;
            }
        }
    }
}

/// Resolves a class name using the current package and imports.
fn resolve_class_name(
    name: &SharedString,
    context: &ResolveContext,
    symbol_table: &SymbolTable,
) -> Option<ResolvedName> {
    let name_str = name.as_str();

    for (package, import_name) in &context.single_type_imports {
        if import_name == name && package_has_symbol(symbol_table, package, name_str) {
            return Some(ResolvedName::new(package.clone(), name.clone()));
        }
    }

    if package_has_symbol(symbol_table, &context.current_package, name_str) {
        return Some(ResolvedName::new(
            context.current_package.clone(),
            name.clone(),
        ));
    }

    // Check java.lang package first (implicitly imported in Java)
    if package_has_symbol(symbol_table, "java.lang", name_str) {
        return Some(ResolvedName::new(
            SharedString::new("java.lang"),
            name.clone(),
        ));
    }

    for package in &context.on_demand_imports {
        if package_has_symbol(symbol_table, package, name_str) {
            return Some(ResolvedName::new(package.clone(), name.clone()));
        }
    }

    None
}

/// Returns true if the symbol table contains a class in the given package.
fn package_has_symbol(symbol_table: &SymbolTable, package: &str, name: &str) -> bool {
    symbol_table
        .get_package(package)
        .is_some_and(|pkg| pkg.contains(name))
}

/// Joins qualified name segments into a Java-style package name.
fn package_name_from_segments(segments: &[SharedString]) -> SharedString {
    SharedString::new(
        segments
            .iter()
            .map(|segment| segment.as_str())
            .collect::<Vec<_>>()
            .join("."),
    )
}

/// Splits import segments into (package, name).
fn split_import_name(segments: &[SharedString]) -> Option<(SharedString, SharedString)> {
    let (name, package) = segments.split_last()?;
    let package = package_name_from_segments(package);
    Some((package, name.clone()))
}

fn literal_type_id(literal: &rajac_ast::Literal, symbol_table: &SymbolTable) -> TypeId {
    use rajac_ast::LiteralKind;

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

fn unary_result_type(
    op: &rajac_ast::UnaryOp,
    operand_ty: TypeId,
    symbol_table: &SymbolTable,
) -> TypeId {
    match op {
        rajac_ast::UnaryOp::Bang => symbol_table
            .primitive_type_id("boolean")
            .unwrap_or(TypeId::INVALID),
        _ => operand_ty,
    }
}

fn binary_result_type(
    op: &rajac_ast::BinaryOp,
    lhs_ty: TypeId,
    rhs_ty: TypeId,
    symbol_table: &SymbolTable,
) -> TypeId {
    use rajac_ast::BinaryOp;

    match op {
        BinaryOp::Lt
        | BinaryOp::LtEq
        | BinaryOp::Gt
        | BinaryOp::GtEq
        | BinaryOp::EqEq
        | BinaryOp::BangEq
        | BinaryOp::And
        | BinaryOp::Or => symbol_table
            .primitive_type_id("boolean")
            .unwrap_or(TypeId::INVALID),
        _ => {
            if lhs_ty != TypeId::INVALID {
                lhs_ty
            } else {
                rhs_ty
            }
        }
    }
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
    for method_id in methods {
        let signature = symbol_table.method_arena().get(*method_id);
        if signature.params.len() != arg_types.len() {
            continue;
        }
        if signature
            .params
            .iter()
            .zip(arg_types)
            .all(|(param, arg)| *arg == TypeId::INVALID || *param == *arg)
        {
            return Some(*method_id);
        }
    }

    methods.first().copied()
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

fn populate_compilation_unit_methods(
    ast: &Ast,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
) {
    let package_name = ast
        .package
        .as_ref()
        .map(|p| package_name_from_segments(&p.name.segments))
        .unwrap_or_else(|| SharedString::new(""));

    for class_id in &ast.classes {
        populate_class_methods(*class_id, arena, symbol_table, &package_name);
    }
}

fn populate_class_methods(
    class_id: ClassDeclId,
    arena: &mut AstArena,
    symbol_table: &mut SymbolTable,
    package_name: &SharedString,
) {
    let (class_name, members) = {
        let class = arena.class_decl(class_id);
        (class.name.name.clone(), class.members.clone())
    };

    let nested_classes: Vec<ClassDeclId> = members
        .iter()
        .filter_map(
            |member_id| match &arena.class_members[member_id.0 as usize] {
                ClassMember::NestedClass(class_id)
                | ClassMember::NestedInterface(class_id)
                | ClassMember::NestedRecord(class_id)
                | ClassMember::NestedAnnotation(class_id) => Some(*class_id),
                _ => None,
            },
        )
        .collect();

    let class_type_id = symbol_table
        .get_package(package_name.as_str())
        .and_then(|package| package.get(class_name.as_str()))
        .map(|symbol| symbol.ty);

    if let Some(class_type_id) = class_type_id {
        let primitive_lookup = symbol_table.primitive_types().clone();
        let (type_arena, method_arena, field_arena) = symbol_table.arenas_mut();
        let mut resolved_methods = Vec::new();
        let mut resolved_fields = Vec::new();
        for member_id in &members {
            match arena.class_members[member_id.0 as usize].clone() {
                ClassMember::Field(field) => {
                    let field_type =
                        type_id_for_ast_type(field.ty, arena, &primitive_lookup, type_arena);
                    let signature = FieldSignature::new(
                        field.name.name.clone(),
                        field_type,
                        field_modifiers_from_ast(&field.modifiers),
                    );
                    let field_id = field_arena.alloc(signature);
                    resolved_fields.push((field.name.name.clone(), field_id));
                }
                ClassMember::Method(method) => {
                    let signature = method_signature_from_method(
                        &method,
                        arena,
                        &primitive_lookup,
                        type_arena,
                        method_modifiers_from_ast(&method.modifiers),
                    );
                    let method_id = method_arena.alloc(signature);
                    resolved_methods.push((method.name.name.clone(), method_id));
                }
                ClassMember::Constructor(constructor) => {
                    let signature = method_signature_from_constructor(
                        &constructor,
                        &class_name,
                        arena,
                        &primitive_lookup,
                        type_arena,
                        method_modifiers_from_ast(&constructor.modifiers),
                    );
                    let method_id = method_arena.alloc(signature);
                    resolved_methods.push((class_name.clone(), method_id));
                }
                _ => {}
            }
        }

        if let Type::Class(class_type) = type_arena.get_mut(class_type_id) {
            for (name, method_id) in resolved_methods {
                class_type.add_method(name, method_id);
            }
            for (name, field_id) in resolved_fields {
                class_type.add_field(name, field_id);
            }
        }
    }

    for nested_class_id in nested_classes {
        populate_class_methods(nested_class_id, arena, symbol_table, package_name);
    }
}

fn method_signature_from_method(
    method: &Method,
    arena: &mut AstArena,
    primitive_lookup: &HashMap<SharedString, TypeId>,
    type_arena: &mut rajac_types::TypeArena,
    modifiers: MethodModifiers,
) -> MethodSignature {
    let params = method
        .params
        .iter()
        .map(|param_id| {
            let param = arena.param(*param_id);
            type_id_for_ast_type(param.ty, arena, primitive_lookup, type_arena)
        })
        .collect();
    let return_type = type_id_for_ast_type(method.return_ty, arena, primitive_lookup, type_arena);
    let throws = method
        .throws
        .iter()
        .map(|ty| type_id_for_ast_type(*ty, arena, primitive_lookup, type_arena))
        .collect();

    MethodSignature {
        name: method.name.name.clone(),
        params,
        return_type,
        throws,
        modifiers,
    }
}

fn method_signature_from_constructor(
    constructor: &Constructor,
    class_name: &SharedString,
    arena: &mut AstArena,
    primitive_lookup: &HashMap<SharedString, TypeId>,
    type_arena: &mut rajac_types::TypeArena,
    modifiers: MethodModifiers,
) -> MethodSignature {
    let params = constructor
        .params
        .iter()
        .map(|param_id| {
            let param = arena.param(*param_id);
            type_id_for_ast_type(param.ty, arena, primitive_lookup, type_arena)
        })
        .collect();
    let throws = constructor
        .throws
        .iter()
        .map(|ty| type_id_for_ast_type(*ty, arena, primitive_lookup, type_arena))
        .collect();

    MethodSignature {
        name: class_name.clone(),
        params,
        return_type: void_type_id(primitive_lookup),
        throws,
        modifiers,
    }
}

fn method_modifiers_from_ast(modifiers: &Modifiers) -> MethodModifiers {
    let mut bits = 0;
    if modifiers.is_public() {
        bits |= MethodModifiers::PUBLIC;
    }
    if modifiers.is_private() {
        bits |= MethodModifiers::PRIVATE;
    }
    if modifiers.is_protected() {
        bits |= MethodModifiers::PROTECTED;
    }
    if modifiers.is_static() {
        bits |= MethodModifiers::STATIC;
    }
    if modifiers.is_final() {
        bits |= MethodModifiers::FINAL;
    }
    if modifiers.is_abstract() {
        bits |= MethodModifiers::ABSTRACT;
    }
    if modifiers.0 & Modifiers::NATIVE != 0 {
        bits |= MethodModifiers::NATIVE;
    }
    if modifiers.0 & Modifiers::SYNCHRONIZED != 0 {
        bits |= MethodModifiers::SYNCHRONIZED;
    }
    if modifiers.0 & Modifiers::STRICTFP != 0 {
        bits |= MethodModifiers::STRICTFP;
    }

    MethodModifiers(bits)
}

fn field_modifiers_from_ast(modifiers: &Modifiers) -> FieldModifiers {
    let mut bits = 0;
    if modifiers.is_public() {
        bits |= FieldModifiers::PUBLIC;
    }
    if modifiers.is_private() {
        bits |= FieldModifiers::PRIVATE;
    }
    if modifiers.is_protected() {
        bits |= FieldModifiers::PROTECTED;
    }
    if modifiers.is_static() {
        bits |= FieldModifiers::STATIC;
    }
    if modifiers.is_final() {
        bits |= FieldModifiers::FINAL;
    }
    if modifiers.0 & Modifiers::VOLATILE != 0 {
        bits |= FieldModifiers::VOLATILE;
    }
    if modifiers.0 & Modifiers::TRANSIENT != 0 {
        bits |= FieldModifiers::TRANSIENT;
    }

    FieldModifiers(bits)
}

fn type_id_for_ast_type(
    type_id: AstTypeId,
    arena: &mut AstArena,
    primitive_lookup: &HashMap<SharedString, TypeId>,
    type_arena: &mut rajac_types::TypeArena,
) -> TypeId {
    let existing = arena.ty(type_id).ty();
    if existing != TypeId::INVALID {
        return existing;
    }

    match arena.ty(type_id) {
        AstType::Primitive { kind, .. } => {
            let resolved = primitive_lookup
                .get(&SharedString::new(primitive_name_from_ast(kind)))
                .copied()
                .unwrap_or(TypeId::INVALID);
            if let AstType::Primitive { ty, .. } = arena.ty_mut(type_id) {
                *ty = resolved;
            }
            resolved
        }
        AstType::Array { element_type, .. } => {
            let element = type_id_for_ast_type(*element_type, arena, primitive_lookup, type_arena);
            if element == TypeId::INVALID {
                TypeId::INVALID
            } else {
                let resolved = type_arena.alloc(Type::array(element));
                if let AstType::Array { ty, .. } = arena.ty_mut(type_id) {
                    *ty = resolved;
                }
                resolved
            }
        }
        _ => existing,
    }
}

fn primitive_name_from_ast(kind: &rajac_ast::PrimitiveType) -> &'static str {
    match kind {
        rajac_ast::PrimitiveType::Byte => "byte",
        rajac_ast::PrimitiveType::Short => "short",
        rajac_ast::PrimitiveType::Int => "int",
        rajac_ast::PrimitiveType::Long => "long",
        rajac_ast::PrimitiveType::Float => "float",
        rajac_ast::PrimitiveType::Double => "double",
        rajac_ast::PrimitiveType::Char => "char",
        rajac_ast::PrimitiveType::Boolean => "boolean",
        rajac_ast::PrimitiveType::Void => "void",
    }
}

fn void_type_id(primitive_lookup: &HashMap<SharedString, TypeId>) -> TypeId {
    primitive_lookup
        .get(&SharedString::new("void"))
        .copied()
        .unwrap_or(TypeId::INVALID)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CompilationUnit;
    use rajac_base::file_path::FilePath;
    use rajac_base::shared_string::SharedString;
    use rajac_parser::parse;
    use rajac_symbols::SymbolKind;

    #[test]
    fn resolves_methods_and_fields_by_name() {
        let mut symbol_table = SymbolTable::new();
        let class_name = SharedString::new("Widget");
        let type_id = symbol_table.add_class(
            "",
            class_name.as_str(),
            Type::class(rajac_types::ClassType::new(class_name.clone())),
            SymbolKind::Class,
        );

        let void_id = symbol_table
            .primitive_type_id("void")
            .unwrap_or(TypeId::INVALID);
        let int_id = symbol_table
            .primitive_type_id("int")
            .unwrap_or(TypeId::INVALID);

        let method_id = symbol_table.method_arena_mut().alloc(MethodSignature::new(
            SharedString::new("run"),
            Vec::new(),
            void_id,
            MethodModifiers(MethodModifiers::PUBLIC),
        ));
        let field_id = symbol_table.field_arena_mut().alloc(FieldSignature::new(
            SharedString::new("count"),
            int_id,
            FieldModifiers(FieldModifiers::PUBLIC),
        ));

        if let Type::Class(class_type) = symbol_table.type_arena_mut().get_mut(type_id) {
            class_type.add_method(SharedString::new("run"), method_id);
            class_type.add_field(SharedString::new("count"), field_id);
        }

        assert_eq!(
            resolve_method_in_type(type_id, &SharedString::new("run"), &[], &symbol_table),
            Some(method_id)
        );
        assert_eq!(
            resolve_field_in_type(type_id, &SharedString::new("count"), &symbol_table),
            Some(field_id)
        );
    }

    #[test]
    fn resolves_overload_by_argument_types() {
        let mut symbol_table = SymbolTable::new();
        let class_name = SharedString::new("Widget");
        let type_id = symbol_table.add_class(
            "",
            class_name.as_str(),
            Type::class(rajac_types::ClassType::new(class_name.clone())),
            SymbolKind::Class,
        );

        let void_id = symbol_table
            .primitive_type_id("void")
            .unwrap_or(TypeId::INVALID);
        let int_id = symbol_table
            .primitive_type_id("int")
            .unwrap_or(TypeId::INVALID);
        let bool_id = symbol_table
            .primitive_type_id("boolean")
            .unwrap_or(TypeId::INVALID);

        let int_method = symbol_table.method_arena_mut().alloc(MethodSignature::new(
            SharedString::new("pick"),
            vec![int_id],
            void_id,
            MethodModifiers(MethodModifiers::PUBLIC),
        ));
        let bool_method = symbol_table.method_arena_mut().alloc(MethodSignature::new(
            SharedString::new("pick"),
            vec![bool_id],
            void_id,
            MethodModifiers(MethodModifiers::PUBLIC),
        ));

        if let Type::Class(class_type) = symbol_table.type_arena_mut().get_mut(type_id) {
            class_type.add_method(SharedString::new("pick"), int_method);
            class_type.add_method(SharedString::new("pick"), bool_method);
        }

        assert_eq!(
            resolve_method_in_type(
                type_id,
                &SharedString::new("pick"),
                &[int_id],
                &symbol_table
            ),
            Some(int_method)
        );
        assert_eq!(
            resolve_method_in_type(
                type_id,
                &SharedString::new("pick"),
                &[bool_id],
                &symbol_table
            ),
            Some(bool_method)
        );
    }

    #[test]
    fn resolves_method_calls_after_signature_only_prep_pass() {
        let source = r#"
class Helper {}

class Example {
    void run(Helper helper) {}

    void test() {
        run(new Helper());
    }
}
"#;
        let parse_result = parse(source, FilePath::new("Example.java"));
        let mut units = vec![CompilationUnit {
            source_file: FilePath::new("Example.java"),
            ast: parse_result.ast,
            arena: parse_result.arena,
            diagnostics: parse_result.diagnostics,
        }];
        let mut symbol_table = SymbolTable::new();
        crate::stages::collection::collect_compilation_unit_symbols(&mut symbol_table, &units)
            .unwrap();

        resolve_identifiers(&mut units, &mut symbol_table);

        let example_class = units[0].ast.classes[1];
        let ClassMember::Method(test_method) = units[0]
            .arena
            .class_member(units[0].arena.class_decl(example_class).members[1])
            .clone()
        else {
            panic!("expected test method");
        };
        let Some(body_id) = test_method.body else {
            panic!("expected test body");
        };
        let rajac_ast::Stmt::Block(statements) = units[0].arena.stmt(body_id).clone() else {
            panic!("expected block body");
        };
        let rajac_ast::Stmt::Expr(expr_id) = units[0].arena.stmt(statements[0]).clone() else {
            panic!("expected expression statement");
        };
        let rajac_ast::Expr::MethodCall { method_id, .. } = units[0].arena.expr(expr_id).clone()
        else {
            panic!("expected method call");
        };

        assert!(method_id.is_some(), "expected method call to be resolved");
    }

    #[test]
    fn stores_resolved_superclass_on_class_types() {
        let source = r#"
class Base {}

class Example extends Base {}
"#;
        let parse_result = parse(source, FilePath::new("Example.java"));
        let mut units = vec![CompilationUnit {
            source_file: FilePath::new("Example.java"),
            ast: parse_result.ast,
            arena: parse_result.arena,
            diagnostics: parse_result.diagnostics,
        }];
        let mut symbol_table = SymbolTable::new();
        crate::stages::collection::collect_compilation_unit_symbols(&mut symbol_table, &units)
            .unwrap();

        resolve_identifiers(&mut units, &mut symbol_table);

        let example_type = symbol_table
            .lookup_type_id("", "Example")
            .expect("Example type");
        let base_type = symbol_table.lookup_type_id("", "Base").expect("Base type");
        let Type::Class(class_type) = symbol_table.type_arena().get(example_type) else {
            panic!("expected class type");
        };

        assert_eq!(class_type.superclass, Some(base_type));
    }

    #[test]
    fn resolves_multidimensional_array_types_with_full_rank() {
        let source = r#"
class Example {
    int[][] run() {
        return new int[1][2];
    }
}
"#;
        let parse_result = parse(source, FilePath::new("Example.java"));
        let mut units = vec![CompilationUnit {
            source_file: FilePath::new("Example.java"),
            ast: parse_result.ast,
            arena: parse_result.arena,
            diagnostics: parse_result.diagnostics,
        }];
        let mut symbol_table = SymbolTable::new();
        crate::stages::collection::collect_compilation_unit_symbols(&mut symbol_table, &units)
            .unwrap();

        resolve_identifiers(&mut units, &mut symbol_table);

        let example_class = units[0].ast.classes[0];
        let ClassMember::Method(run_method) = units[0]
            .arena
            .class_member(units[0].arena.class_decl(example_class).members[0])
            .clone()
        else {
            panic!("expected run method");
        };
        let return_ty = units[0].arena.ty(run_method.return_ty).ty();
        let Type::Array(outer_array) = symbol_table.type_arena().get(return_ty) else {
            panic!("expected outer array type");
        };
        assert!(matches!(
            symbol_table.type_arena().get(outer_array.element_type),
            Type::Array(_)
        ));
    }
}
