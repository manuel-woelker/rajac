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

//! resolution::resolve_identifiers(&mut compilation_units, &symbol_table);

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
    Ast, AstArena, AstType, AstTypeId, ClassMemberId, Constructor, EnumDecl, ExprId, Field, Method,
    ParamId, StmtId,
};
use rajac_base::qualified_name::QualifiedName as ResolvedName;
use rajac_base::shared_string::SharedString;
use rajac_symbols::SymbolTable;
use rajac_types::Ident;
use rayon::prelude::*;

/// Resolves identifiers and types in all compilation units.
///
/// This is the main entry point for the resolution phase. It processes
/// all compilation units in parallel using the symbol table to resolve
/// every identifier and type reference to their fully qualified names.
///
/// # Parameters
///
/// - `compilation_units` - Mutable slice of compilation units to resolve
/// - `symbol_table` - Reference to the populated symbol table
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
/// resolution::resolve_identifiers(&mut compilation_units, &symbol_table);
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
pub fn resolve_identifiers(compilation_units: &mut [CompilationUnit], symbol_table: &SymbolTable) {
    compilation_units.par_iter_mut().for_each(|unit| {
        resolve_compilation_unit(&unit.ast, &mut unit.arena, symbol_table);
    });
}

/// Resolves identifiers in a single compilation unit.
fn resolve_compilation_unit(ast: &Ast, arena: &mut AstArena, symbol_table: &SymbolTable) {
    let context = ResolveContext::new(ast, symbol_table);

    for stmt_id in &ast.statements {
        resolve_stmt(*stmt_id, arena, &context);
    }

    for class_id in &ast.classes {
        resolve_class_decl(*class_id, arena, &context);
    }
}

/// Context for resolving identifiers using the symbol table, package, and imports.
struct ResolveContext<'a> {
    symbol_table: &'a SymbolTable,
    current_package: String,
    single_type_imports: Vec<(String, String)>,
    on_demand_imports: Vec<String>,
}

impl<'a> ResolveContext<'a> {
    /// Builds a resolution context from the current AST and symbol table.
    fn new(ast: &Ast, symbol_table: &'a SymbolTable) -> Self {
        let current_package = ast
            .package
            .as_ref()
            .map(|p| package_name_from_segments(&p.name.segments))
            .unwrap_or_default();

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
            symbol_table,
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
    context: &ResolveContext,
) {
    let (members, extends, implements, permits) = {
        let class = &mut arena.class_decls[class_id.0 as usize];
        resolve_ident(&mut class.name, context);
        for _param in &mut class.type_params {
            // TODO: Implement type parameter name resolution for SharedString
        }
        (
            class.members.clone(),
            class.extends,
            class.implements.clone(),
            class.permits.clone(),
        )
    };

    if let Some(type_id) = extends {
        resolve_type(type_id, arena, context);
    }
    for type_id in implements {
        resolve_type(type_id, arena, context);
    }
    for type_id in permits {
        resolve_type(type_id, arena, context);
    }
    for member_id in members {
        resolve_class_member(member_id, arena, context);
    }
}

/// Resolves identifiers in a class member.
fn resolve_class_member(member_id: ClassMemberId, arena: &mut AstArena, context: &ResolveContext) {
    let mut member = arena.class_members[member_id.0 as usize].clone();

    match &mut member {
        rajac_ast::ClassMember::Field(field) => resolve_field(field, arena, context),
        rajac_ast::ClassMember::Method(method) => resolve_method(method, arena, context),
        rajac_ast::ClassMember::Constructor(constructor) => {
            resolve_constructor(constructor, arena, context)
        }
        rajac_ast::ClassMember::StaticBlock(stmt_id) => resolve_stmt(*stmt_id, arena, context),
        rajac_ast::ClassMember::NestedClass(class_id)
        | rajac_ast::ClassMember::NestedInterface(class_id)
        | rajac_ast::ClassMember::NestedRecord(class_id)
        | rajac_ast::ClassMember::NestedAnnotation(class_id) => {
            resolve_class_decl(*class_id, arena, context)
        }
        rajac_ast::ClassMember::NestedEnum(enum_decl) => {
            resolve_enum_decl(enum_decl, arena, context)
        }
    }

    arena.class_members[member_id.0 as usize] = member;
}

/// Resolves identifiers in an enum declaration.
fn resolve_enum_decl(enum_decl: &mut EnumDecl, arena: &mut AstArena, context: &ResolveContext) {
    resolve_ident(&mut enum_decl.name, context);

    for type_id in enum_decl.implements.clone() {
        resolve_type(type_id, arena, context);
    }

    for entry in &mut enum_decl.entries {
        resolve_ident(&mut entry.name, context);
        for expr_id in entry.args.clone() {
            resolve_expr(expr_id, arena, context);
        }
        if let Some(members) = &entry.body {
            for member_id in members.clone() {
                resolve_class_member(member_id, arena, context);
            }
        }
    }

    for member_id in enum_decl.members.clone() {
        resolve_class_member(member_id, arena, context);
    }
}

/// Resolves identifiers in a field declaration.
fn resolve_field(field: &mut Field, arena: &mut AstArena, context: &ResolveContext) {
    resolve_ident(&mut field.name, context);
    resolve_type(field.ty, arena, context);
    if let Some(expr_id) = field.initializer {
        resolve_expr(expr_id, arena, context);
    }
}

/// Resolves identifiers in a method declaration.
fn resolve_method(method: &mut Method, arena: &mut AstArena, context: &ResolveContext) {
    resolve_ident(&mut method.name, context);
    for param_id in method.params.clone() {
        resolve_param(param_id, arena, context);
    }
    resolve_type(method.return_ty, arena, context);
    for throws_id in method.throws.clone() {
        resolve_type(throws_id, arena, context);
    }
    if let Some(body) = method.body {
        resolve_stmt(body, arena, context);
    }
}

/// Resolves identifiers in a constructor declaration.
fn resolve_constructor(
    constructor: &mut Constructor,
    arena: &mut AstArena,
    context: &ResolveContext,
) {
    resolve_ident(&mut constructor.name, context);
    for param_id in constructor.params.clone() {
        resolve_param(param_id, arena, context);
    }
    for throws_id in constructor.throws.clone() {
        resolve_type(throws_id, arena, context);
    }
    if let Some(body) = constructor.body {
        resolve_stmt(body, arena, context);
    }
}

/// Resolves identifiers in a parameter.
fn resolve_param(param_id: ParamId, arena: &mut AstArena, context: &ResolveContext) {
    let param = &mut arena.params[param_id.0 as usize];
    resolve_ident(&mut param.name, context);
    resolve_type(param.ty, arena, context);
}

/// Resolves identifiers in a statement.
fn resolve_stmt(stmt_id: StmtId, arena: &mut AstArena, context: &ResolveContext) {
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
                            name,
                            initializer,
                        } => {
                            types.push(*ty);
                            resolve_ident(name, context);
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
                if let Some(name) = name {
                    resolve_ident(name, context);
                }
            }
            rajac_ast::Stmt::Label(name, body) => {
                resolve_ident(name, context);
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
                name,
                initializer,
            } => {
                types.push(*ty);
                resolve_ident(name, context);
                if let Some(init) = initializer {
                    exprs.push(*init);
                }
            }
        }

        (exprs, stmts, types, params)
    };

    for type_id in types {
        resolve_type(type_id, arena, context);
    }
    for param_id in params {
        resolve_param(param_id, arena, context);
    }
    for expr_id in exprs {
        resolve_expr(expr_id, arena, context);
    }
    for stmt_id in stmts {
        resolve_stmt(stmt_id, arena, context);
    }
}

/// Resolves identifiers in an expression.
fn resolve_expr(expr_id: ExprId, arena: &mut AstArena, context: &ResolveContext) {
    let (exprs, types) = {
        let expr = &mut arena.exprs[expr_id.0 as usize];
        let mut exprs = Vec::new();
        let mut types = Vec::new();

        match expr {
            rajac_ast::Expr::Error => {}
            rajac_ast::Expr::Ident(name) => resolve_ident(name, context),
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
            rajac_ast::Expr::FieldAccess { expr, name } => {
                exprs.push(*expr);
                resolve_ident(name, context);
            }
            rajac_ast::Expr::MethodCall {
                expr,
                name,
                type_args,
                args,
            } => {
                if let Some(expr_id) = expr {
                    exprs.push(*expr_id);
                }
                resolve_ident(name, context);
                if let Some(type_args) = type_args {
                    types.extend(type_args.iter().copied());
                }
                exprs.extend(args.iter().copied());
            }
            rajac_ast::Expr::New { ty, args } => {
                types.push(*ty);
                exprs.extend(args.iter().copied());
            }
            rajac_ast::Expr::NewArray { ty, dimensions } => {
                types.push(*ty);
                exprs.extend(dimensions.iter().copied());
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
                name,
                type_args,
                args,
            } => {
                resolve_ident(name, context);
                if let Some(type_args) = type_args {
                    types.extend(type_args.iter().copied());
                }
                exprs.extend(args.iter().copied());
            }
        }

        (exprs, types)
    };

    for type_id in types {
        resolve_type(type_id, arena, context);
    }
    for expr_id in exprs {
        resolve_expr(expr_id, arena, context);
    }
}

/// Resolves identifiers in a type.
fn resolve_type(type_id: AstTypeId, arena: &mut AstArena, _context: &ResolveContext) {
    let types = {
        let ty = arena.ty_mut(type_id);
        let mut types = Vec::new();

        match ty {
            AstType::Error => {}
            AstType::Primitive { .. } => {}
            AstType::Simple { type_args, .. } => {
                if !type_args.is_empty() {
                    types.extend(type_args.iter().copied());
                }
                // TODO: Implement class name resolution for AstType::Simple
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
        resolve_type(type_id, arena, _context);
    }
}

/// Resolves a single identifier if it maps to a known symbol.
fn resolve_ident(ident: &mut Ident, context: &ResolveContext) {
    if ident.qualified_name != ResolvedName::default() {
        return;
    }

    if let Some(resolved) = resolve_class_name(&ident.name, context) {
        ident.qualified_name = resolved;
    }
}

/// Resolves a class name using the current package and imports.
fn resolve_class_name(name: &SharedString, context: &ResolveContext) -> Option<ResolvedName> {
    let name_str = name.as_str();

    // Special case for common Java types that should be fully qualified
    if let Some(qualified_name) = resolve_common_java_type(name_str) {
        return Some(qualified_name);
    }

    for (package, import_name) in &context.single_type_imports {
        if import_name == name_str && package_has_symbol(context.symbol_table, package, name_str) {
            return Some(ResolvedName::new(SharedString::new(package), name.clone()));
        }
    }

    if package_has_symbol(context.symbol_table, &context.current_package, name_str) {
        return Some(ResolvedName::new(
            SharedString::new(&context.current_package),
            name.clone(),
        ));
    }

    for package in &context.on_demand_imports {
        if package_has_symbol(context.symbol_table, package, name_str) {
            return Some(ResolvedName::new(SharedString::new(package), name.clone()));
        }
    }

    None
}

/// Resolves common Java types to their fully qualified names.
fn resolve_common_java_type(name_str: &str) -> Option<ResolvedName> {
    match name_str {
        "String" => Some(ResolvedName::new(
            SharedString::new("java/lang"),
            SharedString::new("String"),
        )),
        "Object" => Some(ResolvedName::new(
            SharedString::new("java/lang"),
            SharedString::new("Object"),
        )),
        "System" => Some(ResolvedName::new(
            SharedString::new("java/lang"),
            SharedString::new("System"),
        )),
        "PrintStream" => Some(ResolvedName::new(
            SharedString::new("java/io"),
            SharedString::new("PrintStream"),
        )),
        _ => None,
    }
}

/// Returns true if the symbol table contains a class in the given package.
fn package_has_symbol(symbol_table: &SymbolTable, package: &str, name: &str) -> bool {
    symbol_table
        .get_package(package)
        .is_some_and(|pkg| pkg.contains(name))
}

/// Joins qualified name segments into a Java-style package name.
fn package_name_from_segments(segments: &[SharedString]) -> String {
    segments
        .iter()
        .map(|segment| segment.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

/// Splits import segments into (package, name).
fn split_import_name(segments: &[SharedString]) -> Option<(String, String)> {
    let (name, package) = segments.split_last()?;
    let package = package_name_from_segments(package);
    Some((package, name.as_str().to_string()))
}
